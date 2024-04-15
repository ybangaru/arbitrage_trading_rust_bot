//#![allow(dead_code)]
use chrono::SecondsFormat;
use hmac::Hmac;
use hmac::Mac;
use hmac::NewMac;
use reqwest::header::HeaderMap;
use serde_json::{json, Value};
use sha2::Sha256;
type HmacSha256 = Hmac<Sha256>;
use crate::credentials::{OKEX_PASSPHRASE, OKEX_PUBLIC_KEY, OKEX_SECRET_KEY};
use crate::funding_rates::FundingRates;
use crate::order_entry::CryptoExchange;
use crate::order_entry::Quote;
use crate::settings::OKEX_REST_API_URL_LIVE;
use crate::strategy::StrategyData;

async fn get_okex_swaps(instrument_id: Option<String>) -> Result<Value, reqwest::Error> {
    let endpoint = "/api/v5/public/instruments";
    let mut query_data = vec![("instType", "SWAP")];
    if let Some(id) = &instrument_id {
        query_data.push(("instId", &id))
    };
    Ok(reqwest::Client::new()
        .get(format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint))
        .query(&query_data)
        .send()
        .await?
        .json::<Value>()
        .await?)
}

async fn get_okex_funding_rate(instrument_id: &str) -> Result<Value, String> {
    let endpoint = "/api/v5/public/funding-rate";
    let query_data = vec![("instId", instrument_id)];
    match reqwest::Client::new()
        .get(format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint))
        .query(&query_data)
        .send()
        .await
    {
        Ok(response) => match response.text().await {
            Ok(contents) => {
                let object: Result<Value, serde_json::Error> = serde_json::from_str(&contents);
                match object {
                    Ok(json) => Ok(json),
                    Err(e) => Err(format!("Error parsing request text to json: {}", e)),
                }
            }
            Err(e) => Err(format!("Error getting response text: {}", e)),
        },
        Err(e) => Err(format!("Error sending data request to server: {}", e)),
    }
}

async fn get_all_funding_rates(markets: Vec<&Value>, shared_data: StrategyData) {
    let mut funding_rates: Vec<FundingRates> = Vec::new();
    for market in markets {
        let fr = get_okex_funding_rate(market["instId"].as_str().unwrap())
            .await
            .unwrap();
        let parsed = FundingRates::from_okex(&fr["data"][0]);
        funding_rates.push(parsed);
    }
    let mut lock = shared_data.write().await;
    lock.funding_rates.append(&mut funding_rates);
}

pub async fn get_funding_rates(instrument_id: Option<String>, shared_data: StrategyData) {
    match instrument_id {
        Some(symbol) => {
            let fr = get_okex_funding_rate(&symbol).await.unwrap();
            let parsed = FundingRates::from_okex(&fr["data"][0]);
            shared_data.write().await.funding_rates.push(parsed)
        }
        None => {
            let start = tokio::time::Instant::now();
            let response = get_okex_swaps(instrument_id).await.unwrap();
            crate::output::json("okex_contract_spec", &response);
            let swaps = response["data"].as_array().unwrap().iter();
            let filtered: Vec<&Value> = swaps
                .filter(|swaps| swaps["settleCcy"].as_str().unwrap() == "USDT")
                .collect();
            let sixth: usize = ((filtered.len() - 1) / 6) as usize;
            tokio::join!(
                get_all_funding_rates(filtered[0..sixth - 1].to_vec(), shared_data.clone()),
                get_all_funding_rates(
                    filtered[sixth..(2 * sixth) - 1].to_vec(),
                    shared_data.clone()
                ),
                get_all_funding_rates(
                    filtered[2 * sixth..(3 * sixth) - 1].to_vec(),
                    shared_data.clone()
                ),
                get_all_funding_rates(
                    filtered[3 * sixth..(4 * sixth) - 1].to_vec(),
                    shared_data.clone()
                ),
                get_all_funding_rates(
                    filtered[4 * sixth..(5 * sixth) - 1].to_vec(),
                    shared_data.clone()
                ),
                get_all_funding_rates(
                    filtered[5 * sixth..filtered.len() - 1].to_vec(),
                    shared_data.clone()
                )
            );
            println!(
                "Obtained {} rates from Okex in {} sec",
                filtered.len(),
                start.elapsed().as_secs()
            )
        }
    };
}

pub fn get_okex_signature(
    secret_key: &str,
    timestamp: &str,
    method: &str,
    endpoint: &str,
    body: Option<&str>,
) -> String {
    let concantenated = match body {
        Some(bod) => {
            format!("{}{}{}{}", timestamp, method, endpoint, bod)
        }
        None => {
            format!("{}{}{}", timestamp, method, endpoint)
        }
    };
    let mut mac: HmacSha256 = HmacSha256::new_from_slice(secret_key.as_bytes()).unwrap();
    mac.update(concantenated.as_bytes());
    base64::encode(mac.finalize().into_bytes())
}

pub async fn send_market_order(
    symbol: &str,
    side: &str,
    size: &str,
) -> Result<Value, reqwest::Error> {
    let parameter_json =
        json!({"instId":symbol,"tdMode":"cross","side":side,"ordType":"market","sz":size.parse::<i64>().unwrap()})
            .to_string();
    let endpoint = "/api/v5/trade/order";
    let url = format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let sig = get_okex_signature(
        &OKEX_SECRET_KEY,
        &tstamp,
        "POST",
        &endpoint,
        Some(&parameter_json),
    );
    let mut headers = HeaderMap::new();
    headers.insert("OK-ACCESS-KEY", OKEX_PUBLIC_KEY.parse().unwrap());
    headers.insert("OK-ACCESS-SIGN", sig.parse().unwrap());
    headers.insert("OK-ACCESS-TIMESTAMP", tstamp.parse().unwrap());
    headers.insert("OK-ACCESS-PASSPHRASE", OKEX_PASSPHRASE.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    Ok(reqwest::Client::new()
        .post(url)
        .headers(headers)
        .body(parameter_json)
        .send()
        .await?
        .json::<Value>()
        .await?)
}


pub async fn send_limit_order(
    symbol: &str,
    side: &str,
    size: &str,
    price: f64
) -> Result<String, reqwest::Error> {
    let parameter_json =
        serde_json::to_string(&json!({
            "instId":symbol,
            "tdMode":"cross",
            "side":side,
            "ordType":"limit",
            "sz":size.parse::<i64>().unwrap(),
            "px": price,
        })).unwrap();
    let endpoint = "/api/v5/trade/order";
    let url = format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let sig = get_okex_signature(
        &OKEX_SECRET_KEY,
        &tstamp,
        "POST",
        &endpoint,
        Some(&parameter_json),
    );
    let mut headers = HeaderMap::new();
    headers.insert("OK-ACCESS-KEY", OKEX_PUBLIC_KEY.parse().unwrap());
    headers.insert("OK-ACCESS-SIGN", sig.parse().unwrap());
    headers.insert("OK-ACCESS-TIMESTAMP", tstamp.parse().unwrap());
    headers.insert("OK-ACCESS-PASSPHRASE", OKEX_PASSPHRASE.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    let response = reqwest::Client::new()
        .post(url)
        .headers(headers)
        .body(parameter_json)
        .send()
        .await?
        .json::<Value>()
        .await?;
    match response["data"][0]["ordId"].as_str() {
        Some(id) => Ok(id.to_string()),
        None => {Ok("".to_string())},
    }
}

pub async fn get_positions() -> Result<Vec<Value>, reqwest::Error> {
    let endpoint = "/api/v5/account/positions";
    let url = format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let sig = get_okex_signature(&OKEX_SECRET_KEY, &tstamp, "GET", &endpoint, None);
    let mut headers = HeaderMap::new();
    headers.insert("OK-ACCESS-KEY", OKEX_PUBLIC_KEY.parse().unwrap());
    headers.insert("OK-ACCESS-SIGN", sig.parse().unwrap());
    headers.insert("OK-ACCESS-TIMESTAMP", tstamp.parse().unwrap());
    headers.insert("OK-ACCESS-PASSPHRASE", OKEX_PASSPHRASE.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    Ok(reqwest::Client::new()
        .get(url)
        .headers(headers)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await?["data"]
        .as_array()
        .unwrap()
        .to_vec())
}

fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
// response["data"][0]["adjEq"] has the value
pub async fn get_balance(data: StrategyData) {
    let endpoint = "/api/v5/account/balance";
    let url = format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let sig = get_okex_signature(OKEX_SECRET_KEY, &tstamp, "GET", &endpoint, None);
    let mut headers = HeaderMap::new();
    headers.insert("OK-ACCESS-KEY", OKEX_PUBLIC_KEY.parse().unwrap());
    headers.insert("OK-ACCESS-SIGN", sig.parse().unwrap());
    headers.insert("OK-ACCESS-TIMESTAMP", tstamp.parse().unwrap());
    headers.insert("OK-ACCESS-PASSPHRASE", OKEX_PASSPHRASE.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    let response = reqwest::Client::new()
        .get(url)
        .headers(headers)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let balance = response["data"][0]["totalEq"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    data.write()
        .await
        .account_balances
        .insert(CryptoExchange::Okex, balance);
}

pub async fn get_price(symbol: &str) -> Result<Quote, reqwest::Error> {
    let endpoint = "/api/v5/market/ticker";
    let response = reqwest::Client::new()
        .get(format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint))
        .query(&[("instId", symbol)])
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    Ok(Quote {
        ask: response["data"][0]["askPx"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        bid: response["data"][0]["bidPx"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        exchange: CryptoExchange::Okex,
    })
}

pub fn get_contract_size(symbol: &str) -> f64 {
    let file = std::fs::File::open("okex_contract_spec.json").unwrap();
    let reader = std::io::BufReader::new(file);
    let data_file: Value = serde_json::from_reader(reader).unwrap();
    let mut data_array = data_file["data"].as_array().unwrap().iter();
    let contract_size = data_array
        .find(|data_array| data_array["instId"].as_str().unwrap() == symbol)
        .unwrap();
    contract_size["ctVal"].as_str().unwrap().parse().unwrap()
}

pub async fn set_leverage(symbol: &str, leverage: &str) -> Result<bool, reqwest::Error> {
    let parameter_json =
        json!({"instId":symbol,"lever":leverage, "mgnMode":"cross","posSide":"net"}).to_string();
    let endpoint = "/api/v5/account/set-leverage";
    let url = format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let sig = get_okex_signature(
        &OKEX_SECRET_KEY,
        &tstamp,
        "POST",
        &endpoint,
        Some(&parameter_json),
    );
    let mut headers = HeaderMap::new();
    headers.insert("OK-ACCESS-KEY", OKEX_PUBLIC_KEY.parse().unwrap());
    headers.insert("OK-ACCESS-SIGN", sig.parse().unwrap());
    headers.insert("OK-ACCESS-TIMESTAMP", tstamp.parse().unwrap());
    headers.insert("OK-ACCESS-PASSPHRASE", OKEX_PASSPHRASE.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    let response = reqwest::Client::new()
        .post(url)
        .headers(headers)
        .body(parameter_json)
        .send()
        .await?
        .json::<Value>()
        .await?;
    Ok(response["data"][0]["lever"].as_str().unwrap() == leverage.to_string())
}

pub async fn get_single_funding_rate(instrument_id: &str) -> Result<f64, reqwest::Error> {
    let endpoint = "/api/v5/public/funding-rate";
    let query_data = vec![("instId", instrument_id)];
    let response = reqwest::Client::new()
        .get(format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint))
        .query(&query_data)
        .send()
        .await?
        .json::<Value>()
        .await?;
    Ok(response["data"][0]["fundingRate"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap())
}

pub async fn get_payments_so_far(symbol: &str, start_time: i64) -> Result<f64, reqwest::Error> {
    let endpoint = "/api/v5/account/bills";
    let parameters = format!("instType=SWAP&type=8");
    let path = format!("{}?{}", endpoint, parameters);
    let tstamp = timestamp();
    let sig = get_okex_signature(OKEX_SECRET_KEY, &tstamp, "GET", &path, None);
    let mut headers = HeaderMap::new();
    headers.insert("OK-ACCESS-KEY", OKEX_PUBLIC_KEY.parse().unwrap());
    headers.insert("OK-ACCESS-SIGN", sig.parse().unwrap());
    headers.insert("OK-ACCESS-TIMESTAMP", tstamp.parse().unwrap());
    headers.insert("OK-ACCESS-PASSPHRASE", OKEX_PASSPHRASE.parse().unwrap());
    let response = reqwest::Client::new()
        .get(format!("{}{}", OKEX_REST_API_URL_LIVE, path))
        .headers(headers)
        .send()
        .await?
        .json::<Value>()
        .await?;
    let raw = response["data"].as_array().unwrap();
    let payments: Vec<&Value> = raw
        .iter()
        .filter(|raw| raw["instId"].as_str().unwrap() == symbol && raw["ts"].as_str().unwrap().parse::<i64>().unwrap() >= 1000*start_time)
        .collect();
        println!("{:#?}", payments);
    let mut total = 0.0;
    for payment in payments {
        total += payment["balChg"].as_str().unwrap().parse::<f64>().unwrap();
    }
    Ok(total)
}

pub fn subscribe_to_depth(symbol: &str) -> String {
    json!({
      "op": "subscribe",
      "args": [
        {
          "channel": "books5",
          "instId": symbol
        }
      ]
    })
    .to_string()
}

pub fn subscribe_to_order_updates() -> String {
    json!({
      "op": "subscribe",
      "args": [
        {
          "channel": "orders",
          "instType": "ANY",
        }
      ]
    })
    .to_string()
}

pub fn websocket_login() -> String {
    let tstamp = chrono::Utc::now().timestamp().to_string();
    json!({
    "op": "login",
    "args": [
        {
        "apiKey":OKEX_PUBLIC_KEY,
        "passphrase":OKEX_PASSPHRASE,
        "timestamp":tstamp,
        "sign":get_okex_signature(OKEX_SECRET_KEY,&tstamp, "GET", "/users/self/verify", None)
        }
        ],
    })
    .to_string()
}

pub async fn cancel_order(symbol: &str, order_id: &str) -> Result<bool, reqwest::Error> {
    let parameter_json =
        json!({"instId":symbol,"ordId":order_id}).to_string();
    let endpoint = "/api/v5/trade/cancel-order";
    let url = format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let sig = get_okex_signature(
        &OKEX_SECRET_KEY,
        &tstamp,
        "POST",
        &endpoint,
        Some(&parameter_json),
    );
    let mut headers = HeaderMap::new();
    headers.insert("OK-ACCESS-KEY", OKEX_PUBLIC_KEY.parse().unwrap());
    headers.insert("OK-ACCESS-SIGN", sig.parse().unwrap());
    headers.insert("OK-ACCESS-TIMESTAMP", tstamp.parse().unwrap());
    headers.insert("OK-ACCESS-PASSPHRASE", OKEX_PASSPHRASE.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    let response = reqwest::Client::new()
        .post(url)
        .headers(headers)
        .body(parameter_json)
        .send()
        .await?
        .json::<Value>()
        .await?;
    match response["data"][0]["sCode"].as_str() {
        Some(code) => {
            Ok(code == "0")
        },
        None => Ok(false),
    }
}
/*
pub async fn get_orders() -> Result<Value, reqwest::Error> {
    let endpoint = "/api/v5/trade/orders-pending";
    let path = format!("{}", endpoint);
    let tstamp = timestamp();
    let sig = get_okex_signature(OKEX_SECRET_KEY, &tstamp, "GET", &path, None);
    let mut headers = HeaderMap::new();
    headers.insert("OK-ACCESS-KEY", OKEX_PUBLIC_KEY.parse().unwrap());
    headers.insert("OK-ACCESS-SIGN", sig.parse().unwrap());
    headers.insert("OK-ACCESS-TIMESTAMP", tstamp.parse().unwrap());
    headers.insert("OK-ACCESS-PASSPHRASE", OKEX_PASSPHRASE.parse().unwrap());
    let response = reqwest::Client::new()
        .get(format!("{}{}", OKEX_REST_API_URL_LIVE, path))
        .headers(headers)
        .send()
        .await?
        .json::<Value>()
        .await?;
    Ok(response)
}



pub async fn get_account_configuration() -> Result<Value,reqwest::Error> {
    let endpoint = "/api/v5/account/config";
    let url = format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let sig = get_okex_signature(OKEX_SECRET_KEY,&tstamp, "GET", &endpoint, None);
    let mut headers = HeaderMap::new();
    headers.insert("OK-ACCESS-KEY", OKEX_PUBLIC_KEY.parse().unwrap());
    headers.insert("OK-ACCESS-SIGN", sig.parse().unwrap());
    headers.insert("OK-ACCESS-TIMESTAMP", tstamp.parse().unwrap());
    headers.insert("OK-ACCESS-PASSPHRASE", OKEX_PASSPHRASE.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    Ok(reqwest::Client::new()
        .get(url)
        .headers(headers)
        .send()
        .await?.json::<Value>().await?)
}

pub async fn set_account_configuration(mode:&str) -> Result<Value,reqwest::Error> {
    let body = json!({
        "posMode":mode
    });
    let endpoint = "/api/v5/account/set-position-mode";
    let url = format!("{}{}", OKEX_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let sig = get_okex_signature(OKEX_SECRET_KEY,&tstamp, "POST", &endpoint, Some(&body.to_string()));
    let mut headers = HeaderMap::new();
    headers.insert("OK-ACCESS-KEY", OKEX_PUBLIC_KEY.parse().unwrap());
    headers.insert("OK-ACCESS-SIGN", sig.parse().unwrap());
    headers.insert("OK-ACCESS-TIMESTAMP", tstamp.parse().unwrap());
    headers.insert("OK-ACCESS-PASSPHRASE", OKEX_PASSPHRASE.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    Ok(reqwest::Client::new()
        .post(url)
        .headers(headers)
        .body(body.to_string())
        .send()
        .await?.json::<Value>().await?)
}
*/