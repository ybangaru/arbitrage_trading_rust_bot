
use hmac::Hmac;
use hmac::Mac;
use hmac::NewMac;
use serde_json::Value;
use sha2::Sha256;
type HmacSha256 = Hmac<Sha256>;
use crate::credentials::{LIVE_BINANCE_PRIVATE_KEY, LIVE_BINANCE_PUBLIC_KEY};
use crate::funding_rates::FundingRates;
use crate::order_entry::CryptoExchange;
use crate::order_entry::Quote;
use crate::settings::BINANCE_REST_API_URL_LIVE;
use crate::strategy::StrategyData;

pub async fn get_funding_rates(symbol: Option<String>, shared_data: StrategyData) {
    let start = tokio::time::Instant::now();
    let endpoint = "/fapi/v1/premiumIndex";
    let mut query_data = Vec::new();
    if let Some(sym) = symbol {
        query_data.push(("symbol", Some(sym)))
    };
    match reqwest::Client::new()
        .get(format!("{}{}", BINANCE_REST_API_URL_LIVE, endpoint))
        .query(&query_data)
        .send()
        .await
    {
        Ok(response) => match response.text().await {
            Ok(contents) => {
                let object: Result<Value, serde_json::Error> = serde_json::from_str(&contents);
                match object {
                    Ok(json) => {
                        let mut output: Vec<FundingRates> = Vec::new();
                        if json.is_array() {
                            let vec = json.as_array().unwrap();
                            for perpetual in vec {
                                let fr = FundingRates::from_binance(&perpetual);
                                if fr.is_some() {
                                    output.push(fr.unwrap())
                                };
                            }
                        } else {
                            let fr = FundingRates::from_binance(&json);
                            if fr.is_some() {
                                output.push(fr.unwrap())
                            };
                        }
                        let mut lock = shared_data.write().await;
                        println!(
                            "Obtained {} from Binance in {} sec",
                            output.len(),
                            start.elapsed().as_secs()
                        );
                        lock.funding_rates.append(&mut output)
                    }
                    Err(e) => println!("Error parsing request text to json: {}", e),
                }
            }
            Err(e) => println!("Error reading response text to json: {}", e),
        },
        Err(e) => println!("Error sending request: {}", e),
    }
}

pub async fn send_market_order(
    symbol: &str,
    direction: &str,
    quantity: &str,
) -> Result<Value, reqwest::Error> {
    let parameters = format!(
        "symbol={}&side={}&type=MARKET&quantity={}&timestamp={}",
        symbol,
        direction.to_uppercase(),
        quantity,
        timestamp()
    );
    let signature = get_signature(&parameters, LIVE_BINANCE_PRIVATE_KEY);
    let body = format!("{}&signature={}", parameters, signature);
    let endpoint = "/fapi/v1/order";
    let url = format!("{}{}?{}", BINANCE_REST_API_URL_LIVE, endpoint, body);
    Ok(reqwest::Client::new()
        .post(url)
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await?
        .json::<Value>()
        .await?)
}

pub async fn send_limit_order(
    symbol: &str,
    direction: &str,
    price: f64,
    quantity: &str,
) -> Result<String, reqwest::Error> {
    let parameters = format!(
        "symbol={}&side={}&type=LIMIT&quantity={}&price={}&timeInForce=GTC&timestamp={}",
        symbol,
        direction.to_uppercase(),
        quantity,
        price,
        timestamp()
    );
    let signature = get_signature(&parameters, LIVE_BINANCE_PRIVATE_KEY);
    let body = format!("{}&signature={}", parameters, signature);
    let endpoint = "/fapi/v1/order";
    let url = format!("{}{}?{}", BINANCE_REST_API_URL_LIVE, endpoint, body);
    let response = reqwest::Client::new()
        .post(url)
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await?
        .json::<Value>()
        .await?;
    println!("{}",response);
    match response["orderId"].as_i64() {
        Some(id) => Ok(id.to_string()),
        None => Ok(String::new()),
    }
}

fn timestamp() -> String {
    chrono::Utc::now().timestamp_millis().to_string()
}

fn get_signature(totalparams: &str, private_key: &str) -> String {
    let mut mac: HmacSha256 = HmacSha256::new_from_slice(private_key.as_bytes()).unwrap();
    mac.update(totalparams.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub async fn get_balance(data: StrategyData) {
    let parameters = format!("timestamp={}", timestamp());
    let signature = get_signature(&parameters, &LIVE_BINANCE_PRIVATE_KEY);
    let body = format!("{}&signature={}", parameters, signature);
    let endpoint = "/fapi/v2/account";
    let response = reqwest::Client::new()
        .get(format!(
            "{}{}/?{}",
            BINANCE_REST_API_URL_LIVE, endpoint, body
        ))
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let balance = response["availableBalance"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    data.write()
        .await
        .account_balances
        .insert(CryptoExchange::Binance, balance);
}

pub async fn get_positions() -> Result<Vec<Value>, reqwest::Error> {
    let parameters = format!("timestamp={}", timestamp());
    let signature = get_signature(&parameters, &LIVE_BINANCE_PRIVATE_KEY);
    let body = format!("{}&signature={}", parameters, signature);
    let endpoint = "/fapi/v2/account";
    let response = reqwest::Client::new()
        .get(format!(
            "{}{}/?{}",
            BINANCE_REST_API_URL_LIVE, endpoint, body
        ))
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await?
        .json::<Value>()
        .await?;
    let all_positions = response["positions"].as_array().unwrap().to_vec();
    let open_positions:Vec<Value> = all_positions.into_iter().filter(|position|position["positionAmt"].as_str().unwrap().parse::<f64>().unwrap() != 0.0).collect();
    Ok(open_positions)
}

pub async fn get_price(symbol: &str) -> Result<Quote, reqwest::Error> {
    let endpoint = "/fapi/v1/ticker/bookTicker";
    let response = reqwest::Client::new()
        .get(format!("{}{}", BINANCE_REST_API_URL_LIVE, endpoint))
        .query(&[("symbol", symbol)])
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    Ok(Quote {
        ask: response["askPrice"].as_str().unwrap().parse().unwrap(),
        bid: response["bidPrice"].as_str().unwrap().parse().unwrap(),
        exchange: CryptoExchange::Binance,
    })
}

pub async fn set_leverage(symbol: &str, desired_leverage: i64) -> Result<bool, reqwest::Error> {
    let parameters = format!(
        "symbol={}&leverage={}&timestamp={}",
        symbol,
        desired_leverage,
        timestamp()
    );
    let signature = get_signature(&parameters, LIVE_BINANCE_PRIVATE_KEY);
    let body = format!("{}&signature={}", parameters, signature);
    let endpoint = "/fapi/v1/leverage";
    let response = reqwest::Client::new()
        .post(format!(
            "{}{}/?{}",
            BINANCE_REST_API_URL_LIVE, endpoint, body
        ))
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await?
        .json::<Value>()
        .await?;

    Ok(response["leverage"].as_i64().unwrap() == desired_leverage)
}

pub async fn get_contract_precision_level(symbol: &str) -> Result<usize, reqwest::Error> {
    let endpoint = "/fapi/v1/exchangeInfo";
    let request = reqwest::Client::new()
        .get(format!("{}{}", BINANCE_REST_API_URL_LIVE, endpoint))
        .send()
        .await?
        .json::<Value>()
        .await?;
    let symbols = request["symbols"].as_array().unwrap();
    let target_symbol = symbols
        .iter()
        .find(|symbols| symbols["symbol"] == symbol)
        .unwrap();
    Ok(target_symbol["quantityPrecision"].as_i64().unwrap() as usize)
}

pub async fn get_single_funding_rate(symbol: &str) -> Result<f64, reqwest::Error> {
    let endpoint = "/fapi/v1/premiumIndex";
    let request = reqwest::Client::new()
        .get(format!("{}{}", BINANCE_REST_API_URL_LIVE, endpoint))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .json::<Value>()
        .await?;
    Ok(request["lastFundingRate"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap())
}

pub async fn get_payments_so_far(symbol:&str, start_time:i64) -> Result<f64, reqwest::Error> {
    let parameters = format!("&symbol={}&startTime={}&incomeType=FUNDING_FEE&timestamp={}",symbol,start_time, timestamp());
    let signature = get_signature(&parameters, &LIVE_BINANCE_PRIVATE_KEY);
    let body = format!("{}&signature={}", parameters, signature);
    let endpoint = "/fapi/v1/income";
    let payments = reqwest::Client::new()
        .get(format!(
            "{}{}/?{}",
            BINANCE_REST_API_URL_LIVE, endpoint, body
        ))
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await?
        .json::<Value>()
        .await?.as_array().unwrap().to_vec();
    let mut sum = 0.0;
    for payment in payments {
        sum += payment["income"].as_str().unwrap().parse::<f64>().unwrap()
    }
    Ok(sum)
}

pub fn subscribe_to_ticker(ticker: &str) -> String {
    serde_json::json!({
    "method": "SUBSCRIBE",
    "params":
    [format!("{}@bookTicker",ticker.to_lowercase())],
    "id": 1
    })
    .to_string()
}

pub async fn delete_binance_listen_key() {
    let endpoint = "/fapi/v1/listenKey";
    let url = format!("{}{}", BINANCE_REST_API_URL_LIVE, endpoint);
    reqwest::Client::new()
        .delete(url)
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await
        .unwrap();
}

pub async fn request_listen_key() -> String {
    let endpoint = "/fapi/v1/listenKey";
    let url = format!("{}{}", BINANCE_REST_API_URL_LIVE, endpoint);
    let response = reqwest::Client::new()
        .post(url)
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await
        .unwrap().json::<Value>().await.unwrap();
    response["listenKey"].as_str().unwrap().to_string()
}

pub async fn keep_alive_listen_key() -> Result<Value, String> {
    let endpoint = "/fapi/v1/listenKey";
    let url = format!("{}{}", BINANCE_REST_API_URL_LIVE, endpoint);
    match reqwest::Client::new()
        .put(url)
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await
    {
        Ok(req) => {
            let ret = serde_json::from_str(&req.text().await.unwrap()).unwrap();
            Ok(ret)
        }
        Err(e) => Err(format!("error keeping alive binance listenkey: {}", e)),
    }
}

pub async fn cancel_order(symbol:&str, order_id:i64) -> Result<bool, reqwest::Error> {
    let parameters = format!("&symbol={}&orderId={}&timestamp={}",symbol,order_id, timestamp());
    let signature = get_signature(&parameters, &LIVE_BINANCE_PRIVATE_KEY);
    let body = format!("{}&signature={}", parameters, signature);
    let endpoint = "/fapi/v1/order";
    let response = reqwest::Client::new()
        .delete(format!(
            "{}{}/?{}",
            BINANCE_REST_API_URL_LIVE, endpoint, body
        ))
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await?
        .json::<Value>()
        .await?;
    match response["status"].as_str() {
        Some(answer) => {
            Ok(answer == "CANCELED")
        },
        None => {
            Ok(false)
        },
    }      
}
/*
pub async fn get_open_orders() -> Result<Value, reqwest::Error> {
    let parameters = format!("timestamp={}", timestamp());
    let signature = get_signature(&parameters, &LIVE_BINANCE_PRIVATE_KEY);
    let body = format!("{}&signature={}", parameters, signature);
    let endpoint = "/fapi/v1/openOrders";
    Ok(reqwest::Client::new()
        .get(format!(
            "{}{}/?{}",
            BINANCE_REST_API_URL_LIVE, endpoint, body
        ))
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await?
        .json::<Value>()
        .await?)
}


pub async fn get_position(symbol: &str) -> Result<Value,reqwest::Error> {
    let parameters = format!("symbol={}&timestamp={}",symbol, timestamp());
    let signature = get_signature(&parameters, &LIVE_BINANCE_PRIVATE_KEY);
    let body = format!("{}&signature={}", parameters, signature);
    let endpoint = "/fapi/v1/userTrades";
    let response = reqwest::Client::new()
        .get(format!(
            "{}{}/?{}",
            BINANCE_REST_API_URL_LIVE, endpoint, body
        ))
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await?
        .json::<Value>()
        .await?;
    Ok(response)
}

pub async fn cancel_all_orders(symbol:&str) -> Result<Value, reqwest::Error> {
    let parameters = format!("&symbol={}&timestamp={}",symbol, timestamp());
    let signature = get_signature(&parameters, &LIVE_BINANCE_PRIVATE_KEY);
    let body = format!("{}&signature={}", parameters, signature);
    let endpoint = "/fapi/v1/allOpenOrders";
    let response = reqwest::Client::new()
        .delete(format!(
            "{}{}/?{}",
            BINANCE_REST_API_URL_LIVE, endpoint, body
        ))
        .header("X-MBX-APIKEY", LIVE_BINANCE_PUBLIC_KEY)
        .send()
        .await?
        .json::<Value>()
        .await?;
    Ok(response)     
}

*/