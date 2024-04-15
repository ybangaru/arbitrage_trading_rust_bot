
use crate::credentials::{BYBIT_PRIVATE_KEY, BYBIT_PUBLIC_KEY};
use crate::order_entry::{CryptoExchange, Quote};
use crate::settings::BYBIT_REST_API_URL_LIVE;
use crate::strategy::StrategyData;
use crate::{funding_rates::FundingRates};
use hmac::Hmac;
use hmac::Mac;
use hmac::NewMac;
use serde_json::{json, Value};
use sha2::Sha256;
type HmacSha256 = Hmac<Sha256>;


pub async fn get_funding_rates(future: Option<String>, shared_data: StrategyData) {
    let endpoint = "/v2/public/tickers";
    let mut query_data = vec![];
    if let Some(sym) = &future {
        query_data.push(("symbol", sym))
    }
    let start = tokio::time::Instant::now();
    match reqwest::Client::new()
        .get(format!("{}{}", BYBIT_REST_API_URL_LIVE, endpoint))
        .query(&query_data)
        .send()
        .await
    {
        Ok(response) => match response.text().await {
            Ok(contents) => {
                let object: Result<serde_json::Value, serde_json::Error> =
                    serde_json::from_str(&contents);
                match object {
                    Ok(json) => {
                        let mut output_vec: Vec<FundingRates> = Vec::new();
                        let array = json["result"].as_array().unwrap().iter();
                        let filtered: Vec<&serde_json::Value> = array
                            .filter(|array| {
                                !array["next_funding_time"].as_str().unwrap().is_empty()
                            })
                            .collect();
                        for rate in filtered {
                            output_vec.push(FundingRates::from_bybit(rate));
                        }
                        let mut lock = shared_data.write().await;
                        println!(
                            "Obtained {} rates from Bybit in {} sec",
                            output_vec.len(),
                            start.elapsed().as_secs()
                        );
                        lock.funding_rates.append(&mut output_vec)
                    }
                    Err(e) => println!("Error parsing request text to json: {}", e),
                }
            }
            Err(e) => println!("Error getting response text: {}", e),
        },
        Err(e) => println!("Error sending data request to server: {}", e),
    }
}

fn get_signature(query_data: Vec<(&str, String)>, private_key: &str) -> String {
    let totalparams = concatenate_query(query_data.clone());
    let mut mac: HmacSha256 = HmacSha256::new_from_slice(private_key.as_bytes()).unwrap();
    mac.update(totalparams.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub async fn get_positions() -> Result<Vec<Value>, reqwest::Error> {
    let endpoint = "/v2/private/position/list";
    let url = format!("{}{}", BYBIT_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let mut query_data: Vec<(&str, String)> = Vec::new();
    query_data.push(("timestamp", tstamp));
    query_data.push(("api_key", BYBIT_PUBLIC_KEY.to_string()));
    let signature = get_signature(query_data.clone(), BYBIT_PRIVATE_KEY);
    query_data.push(("sign", signature));
    query_data.sort();
    Ok(reqwest::Client::new()
        .get(url)
        .query(&query_data)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await?["result"].as_array().unwrap().to_vec())
}

fn timestamp() -> String {
    chrono::Utc::now().timestamp_millis().to_string()
}

fn concatenate_query(query_data: Vec<(&str, String)>) -> String {
    let mut cloned = query_data.clone();
    cloned.sort();
    let mut output = String::new();
    if cloned.len() == 1 {
        return format!("{}={}", cloned[0].0, cloned[0].1);
    } else {
        for n in 0..cloned.len() - 1 {
            output.push_str(&format!("{}={}&", cloned[n].0, cloned[n].1))
        }
        output.push_str(&format!(
            "{}={}",
            cloned[cloned.len() - 1].0,
            cloned[cloned.len() - 1].1
        ))
    }
    output
}

pub fn get_body_signature(body: &Value) -> String {
    let mut as_vec: Vec<(&str, String)> = Vec::new();
    for param in body.as_object().unwrap() {
        let value: String = match param.1.as_str() {
            Some(string_value) => string_value.to_string(),
            None => {
                format!("{}", param.1)
            }
        };
        as_vec.push((&param.0, value));
    }
    get_signature(as_vec, BYBIT_PRIVATE_KEY)
}

pub async fn send_limit_order(
    symbol: &str,
    side: &str,
    price:f64,
    quantity: f64,
) -> Result<String, reqwest::Error> {
    let endpoint = "/private/linear/order/create";
    let url = format!("{}{}", BYBIT_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let mut body = json!({
      "api_key": BYBIT_PUBLIC_KEY,
      "side": side,
      "symbol": symbol,
      "order_type": "Limit",
      "price": price,
      "qty": quantity,
      "time_in_force": "GoodTillCancel",
      "reduce_only": false,
      "close_on_trigger": false,
      "timestamp": tstamp,
    });
    let signature = get_body_signature(&body);
    body.as_object_mut()
        .unwrap()
        .insert("sign".to_string(), Value::String(signature));
    let response = reqwest::Client::new()
        .post(url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).unwrap())
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await?;
    match response["result"]["order_id"].as_str() {
        Some(id) => {Ok(id.to_string())},
        None => {Ok(String::new())},
    }
}

pub async fn send_market_order(
    symbol: &str,
    side: &str,
    quantity: f64,
) -> Result<Value, reqwest::Error> {
    let endpoint = "/private/linear/order/create";
    let url = format!("{}{}", BYBIT_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let mut body = json!({
      "api_key": BYBIT_PUBLIC_KEY,
      "side": side,
      "symbol": symbol,
      "order_type": "Market",
      "qty": quantity,
      "time_in_force": "GoodTillCancel",
      "reduce_only": false,
      "close_on_trigger": false,
      "timestamp": tstamp,
    });
    let signature = get_body_signature(&body);
    body.as_object_mut()
        .unwrap()
        .insert("sign".to_string(), Value::String(signature));
    Ok(reqwest::Client::new()
        .post(url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).unwrap())
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await?)
}
/*

*/
pub async fn get_balance(data:StrategyData) {
    let endpoint = "/v2/private/wallet/balance";
    let url = format!("{}{}", BYBIT_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let mut query_data: Vec<(&str, String)> = Vec::new();
    query_data.push(("timestamp", tstamp));
    query_data.push(("api_key", BYBIT_PUBLIC_KEY.to_string()));
    let signature = get_signature(query_data.clone(), BYBIT_PRIVATE_KEY);
    query_data.push(("sign", signature));
    query_data.sort();
    let response = reqwest::Client::new()
        .get(url)
        .query(&query_data)
        .send()
        .await.unwrap()
        .json::<Value>()
        .await.unwrap();
    let balance = response["result"]["USDT"]["available_balance"].as_f64().unwrap();
    data.write().await.account_balances.insert(CryptoExchange::Bybit, balance);
}   

pub async fn get_price(symbol:&str) -> Result<Quote, reqwest::Error> {
    let endpoint = "/v2/public/tickers";
    let response = reqwest::Client::new()
        .get(format!("{}{}", BYBIT_REST_API_URL_LIVE, endpoint))
        .query(&[("symbol", symbol)])
        .send()
        .await.unwrap()
        .json::<Value>()
        .await.unwrap();
    Ok(Quote {
        ask: response["result"][0]["ask_price"].as_str().unwrap().parse().unwrap(),
        bid: response["result"][0]["bid_price"].as_str().unwrap().parse().unwrap(),
        exchange: CryptoExchange::Bybit,
    })
}

pub async fn set_leverage(symbol: &str,leverage:u32) -> Result<bool, reqwest::Error> {
    let endpoint = "/private/linear/position/switch-isolated";
    let url = format!("{}{}", BYBIT_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let mut body = json!({
      "api_key": BYBIT_PUBLIC_KEY,
      "is_isolated": true,
      "symbol": symbol,
      "buy_leverage": leverage,
      "sell_leverage": leverage,
      "timestamp": tstamp,
    });
    let signature = get_body_signature(&body);
    body.as_object_mut()
        .unwrap()
        .insert("sign".to_string(), Value::String(signature));
    println!("{:#?}",body);
    let response = reqwest::Client::new()
        .post(url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).unwrap())
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await?;
    println!("{}",response);
    Ok(response["ret_msg"].as_str().unwrap() == "OK")
}

pub async fn get_single_funding_rate(symbol: &str) -> Result<f64,reqwest::Error>{
    let endpoint = "/v2/public/tickers";
    let response = reqwest::Client::new()
        .get(format!("{}{}", BYBIT_REST_API_URL_LIVE, endpoint))
        .query(&[("symbol", symbol)])
        .send()
        .await?.json::<Value>().await?;
    Ok(response["result"][0]["funding_rate"].as_str().unwrap().parse::<f64>().unwrap())
}

pub fn subscribe_to_ticker(symbol:&str) -> String {
    let symbol_format = format!("instrument_info.100ms.{}",symbol);
    serde_json::to_string(&json!({"op": "subscribe", "args": [symbol_format]})).unwrap()
}

pub async fn cancel_order(symbol: &str,order_id:&str) -> Result<bool, reqwest::Error> {
    let endpoint = "/private/linear/order/cancel";
    let url = format!("{}{}", BYBIT_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let mut body = json!({
      "api_key": BYBIT_PUBLIC_KEY,
      "is_isolated": true,
      "symbol": symbol,
      "order_id": order_id,
      "timestamp": tstamp,
    });
    let signature = get_body_signature(&body);
    body.as_object_mut()
        .unwrap()
        .insert("sign".to_string(), Value::String(signature));
    let response = reqwest::Client::new()
        .post(url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).unwrap())
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await?;
    if response["ret_msg"].as_str().unwrap_or("no") == "OK" && response["result"]["order_id"].as_str().unwrap_or("no") == order_id {Ok(true)} else {Ok(false)}
}

fn get_ws_signature(expires_at:i64, private_key: &str) -> String {
    let totalparams = format!("GET/realtime{}",expires_at);
    let mut mac: HmacSha256 = HmacSha256::new_from_slice(private_key.as_bytes()).unwrap();
    mac.update(totalparams.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn websocket_login() -> String {
    let expires_at = chrono::Utc::now().timestamp_millis() + 1000;
    let sig = get_ws_signature(expires_at,BYBIT_PRIVATE_KEY);
    serde_json::to_string(&json!({
        "op":"auth",
        "args":[BYBIT_PUBLIC_KEY,expires_at,sig]})).unwrap()
}

pub fn subscribe_to_order_updates() -> String {
    serde_json::to_string(&json!({"op": "subscribe", "args": ["order"]})).unwrap()
}

pub async fn get_payments_so_far(symbol:&str,start_time:i64) -> Result<f64,reqwest::Error> {
    let endpoint = "/private/linear/trade/execution/list";
    let url = format!("{}{}", BYBIT_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let mut query_data: Vec<(&str, String)> = Vec::new();
    query_data.push(("symbol", symbol.to_string()));
    query_data.push(("start_time", start_time.to_string()));
    query_data.push(("timestamp", tstamp));
    query_data.push(("api_key", BYBIT_PUBLIC_KEY.to_string()));
    let signature = get_signature(query_data.clone(), BYBIT_PRIVATE_KEY);
    query_data.push(("sign", signature));
    query_data.sort();
    let response = reqwest::Client::new()
        .get(url)
        .query(&query_data)
        .send()
        .await?
        .json::<Value>()
        .await?;
    let data = response["result"]["data"].as_array().unwrap().to_vec();
    let filtered:Vec<Value> = data.into_iter().filter(|item|item["exec_type"].as_str().unwrap() == "Funding").collect();
    let mut payments = 0.0;
    for item in filtered {
        payments += item["exec_value"].as_f64().unwrap();
    }
    Ok(payments)
}  

/* 

pub async fn cancel_all_orders(symbol: &str) -> Result<Value, reqwest::Error> {
    let endpoint = "/private/linear/order/cancel";
    let url = format!("{}{}", BYBIT_REST_API_URL_LIVE, endpoint);
    let tstamp = timestamp();
    let mut body = json!({
      "api_key": BYBIT_PUBLIC_KEY,
      "is_isolated": true,
      "symbol": symbol,
      "timestamp": tstamp,
    });
    let signature = get_body_signature(&body);
    body.as_object_mut()
        .unwrap()
        .insert("sign".to_string(), Value::String(signature));
    let response = reqwest::Client::new()
        .post(url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).unwrap())
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await?;
    Ok(response)
}

*/