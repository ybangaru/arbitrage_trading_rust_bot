use chrono::{DateTime, Duration, Local, NaiveDateTime, NaiveTime, Timelike, Utc};
use serde_json::Value;
extern crate derivative;
use crate::output::{create_csv, send_email};
use crate::settings::{ACCT_VALUE_TO_USE, ADJUSTER, BINANCE_MAKER_FEE, BINANCE_TAKER_FEE, BYBIT_MAKER_FEE, BYBIT_TAKER_FEE, FTX_MAKER_FEE, FTX_TAKER_FEE, OKEX_MAKER_FEE, OKEX_SPEC_FILE, OKEX_TAKER_FEE, TRADE_TIME_OFFSET_SEC};
use crate::strategy::reset_strategy_values;
use crate::{
    binance, bybit,
    coin_tickers::CoinTickerSymbols,
    ftx, funding_rates, okex,
    order_entry::{CryptoExchange, Directive, Side, SpreadTrade},
    strategy::{FundingRateArb, StrategyData},
};
use derivative::*;

use std::{collections::HashMap, str::FromStr};

#[derive(Clone, PartialEq, PartialOrd, Derivative)]
#[derivative(Debug)]
pub struct FundingRates {
    pub exchange: CryptoExchange,
    pub base_name: CoinTickerSymbols,
    pub rate: f64,
    pub funding_time_local: String,
    #[derivative(Debug = "ignore")]
    pub rate_tenk: i64,
    #[derivative(Debug = "ignore")]
    pub funding_timestamp: i64,
    #[derivative(Debug = "ignore")]
    pub symbol: String,
}

impl FundingRates {
    pub fn from_okex(data: &Value) -> Self {
        let rate = 100.0
            * (data["fundingRate"]
                .as_str()
                .unwrap_or("0.0")
                .parse::<f64>()
                .unwrap_or(0.0));
        let timestamp = data["fundingTime"].as_str().unwrap().parse().unwrap();
        let naive = NaiveDateTime::from_timestamp(timestamp / 1000, 0);
        let datetime_again: DateTime<Utc> = DateTime::from_utc(naive, Utc);
        let tz = Local::now().offset().clone();
        Self {
            base_name: CoinTickerSymbols::from_str(
                data["instId"]
                    .as_str()
                    .unwrap()
                    .strip_suffix("-USDT-SWAP")
                    .unwrap(),
            )
            .unwrap(),
            rate,
            funding_timestamp: timestamp,
            exchange: CryptoExchange::Okex,
            rate_tenk: (rate * ADJUSTER) as i64,
            funding_time_local: datetime_again.with_timezone(&tz).to_string(),
            symbol: data["instId"].as_str().unwrap().to_string(),
        }
    }
    pub fn from_binance(data: &Value) -> Option<Self> {
        let rate: f64 = 100.0
            * (data["lastFundingRate"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap());
        let timestamp = data["nextFundingTime"].as_i64().unwrap();
        let naive = NaiveDateTime::from_timestamp(timestamp / 1000, 0);
        let datetime_again: DateTime<Utc> = DateTime::from_utc(naive, Utc);
        let tz = Local::now().offset().clone();
        match data["symbol"].as_str().unwrap().strip_suffix("USDT") {
            Some(base) => match CoinTickerSymbols::from_str(base) {
                Ok(ticker) => Some(Self {
                    exchange: CryptoExchange::Binance,
                    base_name: ticker,
                    rate,
                    funding_timestamp: timestamp,
                    rate_tenk: (rate * ADJUSTER) as i64,
                    funding_time_local: datetime_again.with_timezone(&tz).to_string(),
                    symbol: data["symbol"].as_str().unwrap().to_string(),
                }),
                Err(_) => None,
            },
            None => None,
        }
    }
    pub fn from_ftx(data: &Value, symbol: &str) -> Option<Self> {
        let rate: f64 = data["nextFundingRate"].as_f64().unwrap();
        let next_time =
            DateTime::parse_from_rfc3339(data["nextFundingTime"].as_str().unwrap()).unwrap();
        let tz = Local::now().offset().clone();
        let base = symbol.strip_suffix("-PERP").unwrap();
        match CoinTickerSymbols::from_str(base) {
            Ok(ticker) => Some(Self {
                exchange: CryptoExchange::FTX,
                base_name: ticker,
                rate: rate * 100.0,
                rate_tenk: (rate * 100.0 * (ADJUSTER)) as i64,
                funding_timestamp: next_time.timestamp(),
                funding_time_local: next_time.with_timezone(&tz).to_string(),
                symbol: symbol.to_string(),
            }),
            Err(_) => {
                println!("{},", base);
                None
            }
        }
    }
    pub fn from_bybit(data: &Value) -> Self {
        let rate: f64 = data["funding_rate"].as_str().unwrap().parse().unwrap();
        let base = data["symbol"]
            .as_str()
            .unwrap()
            .strip_suffix("USDT")
            .unwrap_or("ETH");
        let next_time =
            DateTime::parse_from_rfc3339(data["next_funding_time"].as_str().unwrap()).unwrap();
        let coin = CoinTickerSymbols::from_str(&base).unwrap();
        let tz = Local::now().offset().clone();
        Self {
            exchange: CryptoExchange::Bybit,
            base_name: coin,
            rate: 100.0 * rate,
            rate_tenk: (rate * 100.0 * (ADJUSTER)) as i64,
            funding_timestamp: next_time.timestamp(),
            funding_time_local: next_time.with_timezone(&tz).to_string(),
            symbol: data["symbol"].as_str().unwrap().to_string(),
        }
    }
    pub fn calculate_fee(&self, fill_type: FillType) -> f64 {
        let fee = match fill_type {
            FillType::Maker => match self.exchange {
                CryptoExchange::Binance => BINANCE_MAKER_FEE,
                CryptoExchange::Okex => OKEX_MAKER_FEE,
                CryptoExchange::FTX => FTX_MAKER_FEE,
                CryptoExchange::Bybit => BYBIT_MAKER_FEE,
            },
            FillType::Taker => match self.exchange {
                CryptoExchange::Binance => BINANCE_TAKER_FEE,
                CryptoExchange::Okex => OKEX_TAKER_FEE,
                CryptoExchange::FTX => FTX_TAKER_FEE,
                CryptoExchange::Bybit => BYBIT_TAKER_FEE,
            },
        };
        2.0 * fee
    }
}

#[derive(Clone, PartialEq, PartialOrd, Derivative, serde::Serialize)]
#[derivative(Debug)]
pub struct RateSpread {
    pub coin: CoinTickerSymbols,
    pub buy_exchange: CryptoExchange,
    pub buy_low_rate: f64,
    pub sell_exchange: CryptoExchange,
    pub sell_high_rate: f64,
    pub gross_spread: f64,
    pub net_value_half: f64,
    pub net_value_maker: f64,
    pub net_value_taker: f64,
    pub trade_deadline: i64,
    #[derivative(Debug = "ignore")]
    #[serde(skip_serializing)]
    pub net_value_half_tenk: i64,
    pub buy_symbol: String,
    pub sell_symbol: String,
}
impl RateSpread {
    pub fn calculate(data: Vec<&FundingRates>) -> Option<Self> {
        if data.len() >= 2 {
            let mut rates = data.clone();
            rates.sort_by_key(|rates| rates.rate_tenk);
            let lowest_rate = rates[0].clone();
            rates.sort_by_key(|rates| -rates.rate_tenk);
            let highest_rate = rates[0].clone();
            let maker_fees = lowest_rate.calculate_fee(FillType::Maker)
                + highest_rate.calculate_fee(FillType::Maker);
            let taker_fees = lowest_rate.calculate_fee(FillType::Taker)
                + highest_rate.calculate_fee(FillType::Taker);
            let spread = highest_rate.rate - lowest_rate.rate;
            let mut taker_fees_vec = vec![lowest_rate.calculate_fee(FillType::Taker),highest_rate.calculate_fee(FillType::Taker)];
            let mut maker_fees_vec = vec![lowest_rate.calculate_fee(FillType::Maker),highest_rate.calculate_fee(FillType::Maker)];
            taker_fees_vec.sort_by(|a, b| a.partial_cmp(b).unwrap());
            maker_fees_vec.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut funding_time_vec = vec![lowest_rate.funding_timestamp,highest_rate.funding_timestamp];
            funding_time_vec.sort();
            let half_fees = taker_fees_vec.last().unwrap() + maker_fees_vec.last().unwrap();
            let nvh = spread - half_fees;
            if lowest_rate.exchange != highest_rate.exchange {
                Some(Self {
                    coin: lowest_rate.base_name,
                    buy_exchange: lowest_rate.exchange,
                    sell_exchange: highest_rate.exchange,
                    net_value_maker: spread - maker_fees,
                    net_value_half_tenk: (nvh * ADJUSTER) as i64,
                    buy_low_rate: lowest_rate.rate,
                    sell_high_rate: highest_rate.rate,
                    trade_deadline: *funding_time_vec.last().unwrap(),
                    net_value_taker: spread - taker_fees,
                    gross_spread: spread,
                    buy_symbol: lowest_rate.symbol,
                    sell_symbol: highest_rate.symbol,
                    net_value_half: nvh,
                })
            } else {
                None
            }
        } else {
            None
        }
    }
    pub async fn generate_trades(self, data: StrategyData) -> SpreadTrade {
        let lock = data.read().await;
        let mut relevant_balances = vec![
            lock.account_balances[&self.buy_exchange] * ACCT_VALUE_TO_USE,
            lock.account_balances[&self.sell_exchange] * ACCT_VALUE_TO_USE,
        ];
        relevant_balances.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let smaller_balance = relevant_balances[0];
        println!(
            "Account Balances in trade:{:#?}\nSmaller: {}",
            relevant_balances, smaller_balance
        );
        let mut okex_final_dollar_amount:Option<f64> = None;
        let mut long_directive = match self.buy_exchange {
            CryptoExchange::Binance => {
                let decimals = binance::get_contract_precision_level(&self.buy_symbol)
                    .await
                    .unwrap();
                println!("Binance decimals for {} = {}", self.buy_symbol, decimals);
                let quote = binance::get_price(&self.buy_symbol).await.unwrap();
                let amount = format!("{:.1$}", smaller_balance / quote.ask,decimals);
                Directive {
                    exchange: self.buy_exchange,
                    coin: self.coin,
                    symbol: self.buy_symbol,
                    side: Side::Buy,
                    quantity: amount,
                    order_id: None,
                    order_filled: false,
                    fill_price: None,
                }
            }
            CryptoExchange::Okex => {
                let quote = okex::get_price(&self.buy_symbol).await.unwrap();
                let contract_size = okex::get_contract_size(&self.buy_symbol);
                println!(
                    "Okex contract size: {} , coin ask {}",
                    contract_size, quote.ask
                );
                let contract_dollar_value = quote.ask * contract_size;
                let amount_in_order = integer_portion(smaller_balance / contract_dollar_value);
                println!("acct balance/contract$value = {}", amount_in_order);
                if amount_in_order < 1 {
                    panic!("okex account needs more money to take this trade")
                };
                okex_final_dollar_amount = Some(amount_in_order as f64*contract_dollar_value);
                Directive {
                    exchange: self.buy_exchange,
                    coin: self.coin,
                    symbol: self.buy_symbol,
                    side: Side::Buy,
                    quantity: amount_in_order.to_string(),
                    order_id: None,
                    order_filled: false,
                    fill_price: None,
                }
            }
            CryptoExchange::FTX => {
                let quote = ftx::get_price(&self.buy_symbol).await.unwrap();
                let amount = format!("{:.2}", smaller_balance / quote.ask);
                Directive {
                    exchange: self.buy_exchange,
                    coin: self.coin,
                    symbol: self.buy_symbol,
                    side: Side::Buy,
                    quantity: amount,
                    order_id: None,
                    order_filled: false,
                    fill_price: None,
                }
            }
            CryptoExchange::Bybit => {
                let quote = bybit::get_price(&self.buy_symbol).await.unwrap();
                let amount = format!("{:.2}", smaller_balance / quote.ask);
                Directive {
                    exchange: self.buy_exchange,
                    coin: self.coin,
                    symbol: self.buy_symbol,
                    side: Side::Sell,
                    quantity: amount,
                    order_id: None,
                    order_filled: false,
                    fill_price: None,
                }
            }
        };
        let mut short_directive = match self.sell_exchange {
            CryptoExchange::Binance => {
                let quote = binance::get_price(&self.sell_symbol).await.unwrap();
                let amount = format!("{}", smaller_balance / quote.bid);
                Directive {
                    exchange: self.sell_exchange,
                    coin: self.coin,
                    symbol: self.sell_symbol,
                    side: Side::Sell,
                    quantity: amount,
                    order_id: None,
                    order_filled: false,
                    fill_price: None,
                }
            }
            CryptoExchange::Okex => {
                let quote = okex::get_price(&self.sell_symbol).await.unwrap();
                let contract_size = okex::get_contract_size(&self.sell_symbol);
                println!(
                    "Okex contract size: {} , coin ask {}",
                    contract_size, quote.ask
                );
                let contract_dollar_value = quote.bid * contract_size;
                let amount_in_order = integer_portion(smaller_balance / contract_dollar_value);
                println!("acct balance/contract$value = {}", amount_in_order);
                if amount_in_order < 1 {
                    panic!("okex account needs more money to take this trade")
                };
                okex_final_dollar_amount = Some(amount_in_order as f64*contract_dollar_value);
                Directive {
                    exchange: self.sell_exchange,
                    coin: self.coin,
                    symbol: self.sell_symbol,
                    side: Side::Sell,
                    quantity: amount_in_order.to_string(),
                    order_id: None,
                    order_filled: false,
                    fill_price: None,
                }
            }
            CryptoExchange::FTX => {
                let quote = ftx::get_price(&self.sell_symbol).await.unwrap();
                let amount = format!("{:.2}", smaller_balance / quote.ask);
                Directive {
                    exchange: self.sell_exchange,
                    coin: self.coin,
                    symbol: self.sell_symbol,
                    side: Side::Sell,
                    quantity: amount,
                    order_id: None,
                    order_filled: false,
                    fill_price: None,
                }
            }
            CryptoExchange::Bybit => {
                let quote = bybit::get_price(&self.sell_symbol).await.unwrap();
                let amount = format!("{:.2}", smaller_balance / quote.bid);
                Directive {
                    exchange: self.sell_exchange,
                    coin: self.coin,
                    symbol: self.sell_symbol,
                    side: Side::Sell,
                    quantity: amount,
                    order_id: None,
                    order_filled: false,
                    fill_price: None,
                }
            }
        };
        match okex_final_dollar_amount {
            Some(finalokex) => {
                if long_directive.exchange == CryptoExchange::Okex {
                    short_directive.quantity = finalokex.to_string()
                }
                if short_directive.exchange == CryptoExchange::Okex {
                    long_directive.quantity = finalokex.to_string()
                }
                let mut matching_quantity = vec![long_directive.quantity.parse::<f64>().unwrap(),short_directive.quantity.parse::<f64>().unwrap()];
                matching_quantity.sort_by(|a, b| a.partial_cmp(b).unwrap());
                long_directive.quantity = matching_quantity[0].to_string();
                short_directive.quantity = matching_quantity[0].to_string();
                return SpreadTrade {
                    timestamp: chrono::Utc::now().timestamp(),
                    long_directive,
                    short_directive,
                }
            }
            None => {
                    let mut matching_quantity = vec![long_directive.quantity.parse::<f64>().unwrap(),short_directive.quantity.parse::<f64>().unwrap()];
                    matching_quantity.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    long_directive.quantity = matching_quantity[0].to_string();
                    short_directive.quantity = matching_quantity[0].to_string();
                }
        }
        SpreadTrade {
            timestamp: chrono::Utc::now().timestamp(),
            long_directive,
            short_directive,
        }
    }
}

pub async fn get_all(shared_data: StrategyData) -> Vec<FundingRates> {
    shared_data.write().await.funding_rates.clear();
    tokio::join!(
        crate::okex::get_funding_rates(None, shared_data.clone()),
        crate::binance::get_funding_rates(None.clone(), shared_data.clone()),
        crate::ftx::get_funding_rates(shared_data.clone()),
        crate::bybit::get_funding_rates(None, shared_data.clone())
    );
    shared_data.write().await.funding_rates.clone()
}

pub fn integer_portion(float: f64) -> i64 {
    float.to_string().split('.').collect::<Vec<&str>>()[0]
        .parse()
        .unwrap()
}

pub async fn run_funding_rate_strategy(shared_data: StrategyData) {
    let start = tokio::time::Instant::now();
    FundingRateArb::get_account_balances(shared_data.clone()).await;
    println!("{:#?}", shared_data.read().await.account_balances);
    let all_rates: Vec<FundingRates> = funding_rates::get_all(shared_data.clone()).await;
    let mut coinmap: HashMap<String, CoinTickerSymbols> = HashMap::new();
    for rate in &all_rates {
        if !coinmap.contains_key(&format!("{:?}", rate.base_name)) {
            coinmap.insert(format!("{:?}", rate.base_name), rate.base_name);
        }
    }
    println!(
        "Obtained funding rates for {} different coins",
        coinmap.keys().count()
    );
    let mut ratespreads: Vec<RateSpread> = Vec::new();
    for coin in coinmap.values() {
        let rates_in_coin: Vec<&FundingRates> = all_rates
            .iter()
            .filter(|allrates| allrates.base_name == *coin)
            .collect();
        if let Some(spread) = RateSpread::calculate(rates_in_coin) {
            ratespreads.push(spread)
        }
    }
    println!("Obtained spreads for {} coins", ratespreads.len());
    ratespreads.sort_by_key(|ratespreads| -ratespreads.net_value_half_tenk);
    create_csv(&ratespreads, "");
    if ratespreads[0].net_value_half > 0.02 {
        println!("{:#?}", ratespreads[0]);
        send_email("receiver@tradequant.pro", &format!("{:#?}",ratespreads[0]), "Taking Trades").await;
        shared_data.write().await.trades = Some(ratespreads[0].clone());
        let trades = ratespreads[0]
            .clone()
            .generate_trades(shared_data.clone())
            .await;
        println!("{:#?}", trades);
        crate::output::json("trades", &trades);
        trades.execute_with_socket(shared_data.clone(),false).await;
    } else {
        let email = format!("Criteria not met for trade. \nBest Spread:{:#?}", ratespreads[0]);
        println!("{}", email);
        send_email("receiver@tradequant.pro", &email, "No Trades").await;
    };
    crate::output::delete_file(OKEX_SPEC_FILE);
    println!("Execution Time: {} seconds", start.elapsed().as_secs());
}

pub fn calculate_duration_to_next_rate() -> Option<std::time::Duration> {
    let funding_rate_times: Vec<NaiveTime> = vec![
        chrono::NaiveTime::from_hms(0, 0, 0) + chrono::Duration::seconds(TRADE_TIME_OFFSET_SEC),
        chrono::NaiveTime::from_hms(8, 0, 0) + chrono::Duration::seconds(TRADE_TIME_OFFSET_SEC),
        chrono::NaiveTime::from_hms(16, 0, 0) + chrono::Duration::seconds(TRADE_TIME_OFFSET_SEC),
    ];
    let durations: Vec<Duration> = funding_rate_times
        .iter()
        .map(|time| *time - chrono::Utc::now().time())
        .collect();
    let mut positive_durations: Vec<&Duration> = durations
        .iter()
        .filter(|duration| **duration > Duration::zero())
        .collect();
    positive_durations.sort();
    if !positive_durations.is_empty() {
        Some(positive_durations[0].to_std().unwrap())
    } else {None}
    
}

pub async fn manage_trades(shared_data: StrategyData, rate_spread: Value) {
    let positions: SpreadTrade = serde_json::from_value(rate_spread).unwrap();
    positions.check_positions(shared_data.clone()).await;
    let payments = positions.get_payments_so_far().await;
    let current_rate_spread = positions.get_current_spread().await;
    let report = format!(
        "\nPositions have a net funding rate of: {}. Position have accumulated PNL from funding rates of {}",
        current_rate_spread,
        payments
    );
    if current_rate_spread < 0.0 {
            match positions.involve_exchange(CryptoExchange::FTX) {
            true => {
                if utc_time().minute() > 45 {
                    println!("Closing Positions...");
                    positions
                        .create_closing_trades()
                        .execute_with_socket(shared_data.clone(),true)
                        .await;
                    crate::output::delete_file("trades.json")
                }
            },
            false => {
                let duration_to_next_rate = calculate_duration_to_next_rate().unwrap_or(std::time::Duration::ZERO);
                if duration_to_next_rate.as_secs() < 900 {
                    println!("Closing Positions...");
                    positions
                        .create_closing_trades()
                        .execute_with_socket(shared_data.clone(),true)
                        .await;
                    crate::output::delete_file("trades.json")
                }
            },
        }
    }    
    shared_data.write().await.user_messages.push_str(&report);
    send_email("receiver@tradequant.pro", &shared_data.read().await.user_messages, "Periodic Report").await;
    reset_strategy_values(shared_data.clone()).await;
}

pub enum FillType {
    Maker,
    Taker,
}

pub fn utc_time() -> NaiveTime {
    chrono::Utc::now().time()
}