//#![allow(dead_code)]
use crate::credentials::{FTX_KEY, FTX_SECRET_KEY};
use crate::funding_rates::FundingRates;
use crate::order_entry::{CryptoExchange, Quote};
use crate::settings::FTX_REST_API_URL_LIVE;
use crate::strategy::StrategyData;
use hmac::Hmac;
use hmac::Mac;
use hmac::NewMac;
use reqwest::header::HeaderMap;
use serde_json::{json, Value};
use sha2::Sha256;
type HmacSha256 = Hmac<Sha256>;



pub async fn get_contract_names() -> Result<Vec<String>, reqwest::Error> {
    let endpoint = format!("/funding_rates");
    let response = reqwest::Client::new()
        .get(format!("{}{}", FTX_REST_API_URL_LIVE, endpoint))
        .send()
        .await?
        .json::<Value>()
        .await?;
    let contracts = response["result"].as_array().unwrap();
    Ok(contracts
        .iter()
        .map(|contract| contract["future"].as_str().unwrap().to_string())
        .collect())
}

pub fn get_ftx_signature(
    timestamp: &str,
    method: &str,
    endpoint: &str,
    body: Option<&str>,
) -> String {
    let concantenated = match body {
        Some(bod) => {
            format!(
                "{}{}/api{}{}",
                timestamp,
                method.to_uppercase(),
                endpoint,
                bod
            )
        }
        None => {
            format!("{}{}/api{}", timestamp, method.to_uppercase(), endpoint)
        }
    };
    let mut mac: HmacSha256 = HmacSha256::new_from_slice(FTX_SECRET_KEY.as_bytes()).unwrap();
    mac.update(concantenated.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub async fn get_balance(data: StrategyData) {
    let endpoint = "/account";
    let url = format!("{}{}", FTX_REST_API_URL_LIVE, endpoint);
    let mut headers = HeaderMap::new();
    let timestamp = ftx_timestamp();
    headers.insert("FTX-KEY", FTX_KEY.parse().unwrap());
    headers.insert("FTX-TS", timestamp.parse().unwrap());
    headers.insert(
        "FTX-SIGN",
        get_ftx_signature(&timestamp, "GET", endpoint, None)
            .parse()
            .unwrap(),
    );
    let response = reqwest::Client::new()
        .get(url)
        .headers(headers)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let balance = response["result"]["collateral"].as_f64().unwrap();
    data.write()
        .await
        .account_balances
        .insert(CryptoExchange::FTX, balance);
}

pub fn ftx_timestamp() -> String {
    chrono::Utc::now().timestamp_millis().to_string()
}

pub async fn send_market_order(
    symbol: &str,
    side: &str,
    size: f64,
) -> Result<Value, reqwest::Error> {
    let request_body = serde_json::to_string(&json!({
      "market": symbol,
      "side": side,
      "type": "market",
      "size": size,
      "price":null,
    }))
    .unwrap();
    let endpoint = "/orders";
    let url = format!("{}{}", FTX_REST_API_URL_LIVE, endpoint);
    let mut headers = HeaderMap::new();
    let timestamp = ftx_timestamp();
    let signature = get_ftx_signature(&timestamp, "POST", endpoint, Some(&request_body));
    headers.insert("FTX-KEY", FTX_KEY.parse().unwrap());
    headers.insert("FTX-TS", timestamp.parse().unwrap());
    headers.insert("FTX-SIGN", signature.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    Ok(reqwest::Client::new()
        .post(url)
        .headers(headers)
        .body(request_body)
        .send()
        .await?
        .json::<Value>()
        .await?)
}

pub async fn send_limit_order(
    symbol: &str,
    side: &str,
    price: f64,
    size: f64,
) -> Result<String, reqwest::Error> {
    let request_body = serde_json::to_string(&json!({
      "market": symbol,
      "side": side,
      "type": "limit",
      "price": price,
      "size": size,
    }))
    .unwrap();
    let endpoint = "/orders";
    let url = format!("{}{}", FTX_REST_API_URL_LIVE, endpoint);
    let mut headers = HeaderMap::new();
    let timestamp = ftx_timestamp();
    let signature = get_ftx_signature(&timestamp, "POST", endpoint, Some(&request_body));
    headers.insert("FTX-KEY", FTX_KEY.parse().unwrap());
    headers.insert("FTX-TS", timestamp.parse().unwrap());
    headers.insert("FTX-SIGN", signature.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    let response = reqwest::Client::new()
        .post(url)
        .headers(headers)
        .body(request_body)
        .send()
        .await?
        .json::<Value>()
        .await?;
    println!("{:#?}",response);
    match response["result"]["id"].as_i64() {
        Some(id) => {
            Ok(id.to_string())
        },
        None => {
            Ok("".to_string())
        },
    }
}


pub async fn get_positions() -> Result<Vec<Value>, reqwest::Error> {
    let endpoint = "/positions";
    let url = format!("{}{}", FTX_REST_API_URL_LIVE, endpoint);
    let mut headers = HeaderMap::new();
    let timestamp = ftx_timestamp();
    headers.insert("FTX-KEY", FTX_KEY.parse().unwrap());
    headers.insert("FTX-TS", timestamp.parse().unwrap());
    headers.insert(
        "FTX-SIGN",
        get_ftx_signature(&timestamp, "GET", endpoint, None)
            .parse()
            .unwrap(),
    );
    let positions = reqwest::Client::new()
        .get(url)
        .headers(headers)
        .send()
        .await?
        .json::<Value>()
        .await?["result"]
        .as_array()
        .unwrap()
        .to_vec();
    let filtered:Vec<Value> = positions.into_iter().filter(|position|position["netSize"].as_f64().unwrap() != 0.0).collect();
    Ok(filtered)
}

pub async fn get_price(symbol: &str) -> Result<Quote, reqwest::Error> {
    let endpoint = "/markets";
    let response = reqwest::Client::new()
        .get(format!("{}{}/{}", FTX_REST_API_URL_LIVE, endpoint, symbol))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    Ok(Quote {
        ask: response["result"]["ask"].as_f64().unwrap(),
        bid: response["result"]["bid"].as_f64().unwrap(),
        exchange: CryptoExchange::FTX,
    })
}

pub async fn get_single_funding_rate_shared(future: &str) -> Result<Value, reqwest::Error> {
    let endpoint = format!("/futures/{}/stats", future);
    let response = reqwest::Client::new()
        .get(format!("{}{}", FTX_REST_API_URL_LIVE, endpoint))
        .send()
        .await?
        .json::<Value>()
        .await?;
    Ok(response["result"].clone())
}

pub async fn get_single_funding_rate(future: &str) -> Result<f64, reqwest::Error> {
    let endpoint = format!("/futures/{}/stats", future);
    let response = reqwest::Client::new()
        .get(format!("{}{}", FTX_REST_API_URL_LIVE, endpoint))
        .send()
        .await?
        .json::<Value>()
        .await?;
    Ok(response["result"]["nextFundingRate"].as_f64().unwrap())
}

pub async fn get_funding_rates(shared_data: StrategyData) {
    let now = std::time::Instant::now();
    let contracts = get_contract_names().await.unwrap();
    let one_fourteenth = contracts.len() / 14 as usize;
    tokio::join!(
        get_some_rates(contracts[0..one_fourteenth - 1].to_vec(), shared_data.clone()),
        get_some_rates(
            contracts[one_fourteenth..(2 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
        get_some_rates(
            contracts[2 * one_fourteenth..(3 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
        get_some_rates(
            contracts[3 * one_fourteenth..(4 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
        get_some_rates(
            contracts[4 * one_fourteenth..(5 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
        get_some_rates(
            contracts[5 * one_fourteenth..(6 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
        get_some_rates(
            contracts[6 * one_fourteenth..(7 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
        get_some_rates(
            contracts[7 * one_fourteenth..(8 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
        get_some_rates(
            contracts[9 * one_fourteenth..(10 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
        get_some_rates(
            contracts[10 * one_fourteenth..(11 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
        get_some_rates(
            contracts[11 * one_fourteenth..(12 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
        get_some_rates(
            contracts[12 * one_fourteenth..(13 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
        get_some_rates(
            contracts[13 * one_fourteenth..(14 * one_fourteenth) - 1].to_vec(),
            shared_data.clone()
        ),
    );
    println!(
        "Obtained {} from FTX in {} seconds.",
        contracts.len(),
        now.elapsed().as_secs()
    );
}

pub async fn get_some_rates(contracts: Vec<String>, shared_data: StrategyData) {
    let mut output: Vec<FundingRates> = Vec::new();
    for contract in contracts {
        let data = get_single_funding_rate_shared(&contract).await.unwrap();
        let fr = FundingRates::from_ftx(&data, &contract).unwrap();
        output.push(fr);
    }
    shared_data
        .write()
        .await
        .funding_rates
        .append(&mut output);
}

pub fn subscribe_to_ticker(ticker: &str) -> String {
    serde_json::json!({
    "op": "subscribe",
    "channel": "ticker",
    "market" : ticker,
    })
    .to_string()
}

pub fn subscribe_to_orders() -> String {
    serde_json::json!({
    "op": "subscribe",
    "channel": "orders",
    })
    .to_string()
}

pub async fn cancel_order(order_id:&str) -> Result<bool, reqwest::Error> {
    let endpoint = format!("/orders/{}",order_id);
    let url = format!("{}{}", FTX_REST_API_URL_LIVE, endpoint);
    let mut headers = HeaderMap::new();
    let timestamp = ftx_timestamp();
    headers.insert("FTX-KEY", FTX_KEY.parse().unwrap());
    headers.insert("FTX-TS", timestamp.parse().unwrap());
    headers.insert(
        "FTX-SIGN",
        get_ftx_signature(&timestamp, "DELETE", &endpoint, None)
            .parse()
            .unwrap(),
    );
    let response = reqwest::Client::new()
        .delete(url)
        .headers(headers)
        .send()
        .await?
        .json::<Value>().await.unwrap();
    match response["result"].as_str() {
        Some(answer) => {
            Ok(answer == "Order queued for cancellation")
        },
        None => {Ok(false)}
    }
}

fn get_ws_signature(
    timestamp: &str,
) -> String {
    let concantenated = format!("{}websocket_login",timestamp);
    let mut mac: HmacSha256 = HmacSha256::new_from_slice(FTX_SECRET_KEY.as_bytes()).unwrap();
    mac.update(concantenated.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}



pub fn websocket_login() -> String  {
    let ts = ftx_timestamp();
    let sig = get_ws_signature(&ts);
    let args = json!({
        "key": FTX_KEY,
        "sign": sig,
        "time": ts.parse::<i64>().unwrap(),
      });
    serde_json::to_string(&json!({
        "args": args,
        "op": "login",
      })).unwrap()
}

pub async fn get_payments_so_far(future:&str,start_time:i64) -> Result<f64, reqwest::Error> {
    let endpoint = "/funding_payments";
    let url = format!("{}{}", FTX_REST_API_URL_LIVE, endpoint);
    let mut headers = HeaderMap::new();
    let timestamp = ftx_timestamp();
    headers.insert("FTX-KEY", FTX_KEY.parse().unwrap());
    headers.insert("FTX-TS", timestamp.parse().unwrap());
    headers.insert(
        "FTX-SIGN",
        get_ftx_signature(&timestamp, "GET", endpoint, None)
            .parse()
            .unwrap(),
    );
    let response = reqwest::Client::new()
        .get(url)
        .headers(headers)
        .send()
        .await?
        .json::<Value>()
        .await?;
    let result = response["result"].as_array().unwrap();
    let filtered:Vec<&Value> = result.iter().filter(|payment|payment["future"] == future && chrono::DateTime::parse_from_rfc3339(payment["time"].as_str().unwrap()).unwrap().timestamp() > start_time).collect();
    let mut payments_so_far = 0.0;
    for payment in filtered {
        payments_so_far +=payment["payment"].as_f64().unwrap();
    }
    Ok(payments_so_far)
}

/*
pub async fn get_position(future:&str) -> Result<Value,reqwest::Error> {
    let endpoint = "/positions";
    let url = format!("{}{}", FTX_REST_API_URL_LIVE, endpoint);
    let mut headers = HeaderMap::new();
    let timestamp = ftx_timestamp();
    headers.insert("FTX-KEY", FTX_KEY.parse().unwrap());
    headers.insert("FTX-TS", timestamp.parse().unwrap());
    headers.insert(
        "FTX-SIGN",
        get_ftx_signature(&timestamp, "GET", endpoint, None)
            .parse()
            .unwrap(),
    );
    let request = reqwest::Client::new()
        .get(url)
        .headers(headers)
        .send()
        .await?
        .json::<Value>()
        .await?;
    let positions = request["result"].as_array().unwrap();
    Ok(positions.iter().find(|positions|positions["future"] == future).unwrap().clone())
}


pub async fn set_leverage(leverage: i64) -> Result<bool, reqwest::Error> {
    let request_body = serde_json::to_string(&json!({ "leverage": leverage })).unwrap();
    let endpoint = "/account/leverage";
    let url = format!("{}{}", FTX_REST_API_URL_LIVE, endpoint);
    let mut headers = HeaderMap::new();
    let timestamp = ftx_timestamp();
    headers.insert("FTX-KEY", FTX_KEY.parse().unwrap());
    headers.insert("FTX-TS", timestamp.parse().unwrap());
    let signature = get_ftx_signature(&timestamp, "POST", endpoint, Some(&request_body));
    headers.insert("FTX-SIGN", signature.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    let response = reqwest::Client::new()
        .post(url)
        .headers(headers)
        .body(request_body).send().await?.json::<Value>().await?;
    Ok(response["success"].as_bool().unwrap())
}

pub async fn cancel_orders() -> Result<Value, reqwest::Error> {
    let endpoint = "/orders";
    let url = format!("{}{}", FTX_REST_API_URL_LIVE, endpoint);
    let mut headers = HeaderMap::new();
    let timestamp = ftx_timestamp();
    headers.insert("FTX-KEY", FTX_KEY.parse().unwrap());
    headers.insert("FTX-TS", timestamp.parse().unwrap());
    headers.insert(
        "FTX-SIGN",
        get_ftx_signature(&timestamp, "DELETE", endpoint, None)
            .parse()
            .unwrap(),
    );
    Ok(reqwest::Client::new()
        .delete(url)
        .headers(headers)
        .send()
        .await?
        .json::<Value>()
        .await?)
}
*/