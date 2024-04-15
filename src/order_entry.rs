extern crate url;

use crate::output::send_email;
use crate::settings::{
    BINANCE_PUBLIC_WS_LIVE, BYBIT_PUBLIC_WS_LIVE, DEPLOY, FTX_PUBLIC_WS_LIVE, OKEX_PRIVATE_WS_LIVE,
    OKEX_PUBLIC_WS_LIVE,
};
use crate::strategy::{reset_strategy_values, OrderID, StrategyData};
use crate::{binance, bybit, coin_tickers::CoinTickerSymbols, ftx, okex};
use core::time;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use tokio_stream::wrappers::IntervalStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, PartialEq, Ord, Eq, PartialOrd, Serialize, Copy, Hash, Deserialize)]
pub enum CryptoExchange {
    Binance,
    Bybit,
    FTX,
    Okex,
}
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Copy, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Directive {
    pub exchange: CryptoExchange,
    pub coin: CoinTickerSymbols,
    pub symbol: String,
    pub side: Side,
    pub quantity: String,
    pub order_id: Option<String>,
    pub order_filled: bool,
    pub fill_price: Option<f64>,
}
impl Directive {
    pub async fn execute(self, shared_data: StrategyData) {
        match self.exchange {
            CryptoExchange::Okex => {
                let side = match self.side {
                    Side::Buy => "buy",
                    Side::Sell => "sell",
                };
                let leverage_ok = okex::set_leverage(&self.symbol, "1").await.unwrap();
                println!("Okex Leverage adjust success: {:#?}", leverage_ok);
                println!("Okex Args: {} {} {}", &self.symbol, side, &self.quantity);
                if DEPLOY && leverage_ok {
                    shared_data.write().await.user_messages.push_str(&format!(
                        "Okex Response: {:#?}",
                        okex::send_market_order(&self.symbol, side, &self.quantity).await
                    ));
                };
            }
            CryptoExchange::FTX => {
                let side = match self.side {
                    Side::Buy => "buy",
                    Side::Sell => "sell",
                };
                println!(
                    "FTX Args {} {} {}",
                    &self.symbol,
                    side,
                    self.quantity.parse::<f64>().unwrap()
                );
                if DEPLOY {
                    shared_data.write().await.user_messages.push_str(&format!(
                        "FTX Response: {:#?}",
                        crate::ftx::send_market_order(
                            &self.symbol,
                            side,
                            self.quantity.parse().unwrap()
                        )
                        .await
                    ))
                };
            }
            CryptoExchange::Bybit => {
                let side = match self.side {
                    Side::Buy => "Buy",
                    Side::Sell => "Sell",
                };
                let leverage_good = bybit::set_leverage(&self.symbol, 1).await.unwrap();
                println!("Bybit Leverage adjust success: {:#?}", leverage_good);
                println!(
                    "ByBit Args: {} {} {}",
                    &self.symbol,
                    side,
                    self.quantity.parse::<f64>().unwrap()
                );
                if DEPLOY && leverage_good {
                    shared_data.write().await.user_messages.push_str(&format!(
                        "Bybit Response: {:#?}",
                        bybit::send_market_order(
                            &self.symbol,
                            side,
                            self.quantity.parse().unwrap()
                        )
                        .await
                    ))
                }
            }
            CryptoExchange::Binance => {
                let side = match self.side {
                    Side::Buy => "BUY",
                    Side::Sell => "SELL",
                };
                println!("Binance Args: {} {} {} ", &self.symbol, side, self.quantity,);
                let leverage_ok = binance::set_leverage(&self.symbol, 1).await.unwrap();
                println!("Binance Leverage adjust success: {:#?}", leverage_ok);
                if DEPLOY && leverage_ok {
                    shared_data.write().await.user_messages.push_str(&format!(
                        "Binance Response {:#?}",
                        binance::send_market_order(&self.symbol, side, &self.quantity).await
                    ))
                }
            }
        }
    }
    pub fn create_closing_directive(&self) -> Self {
        Self {
            side: if self.side == Side::Buy {
                Side::Sell
            } else {
                Side::Buy
            },
            ..self.to_owned()
        }
    }
    pub async fn check_status(&self, shared_data: StrategyData, side: Side) {
        match self.exchange {
            CryptoExchange::Binance => {
                let positions = binance::get_positions().await.unwrap();
                let position = positions
                    .iter()
                    .find(|positions| positions["symbol"].as_str().unwrap() == self.symbol)
                    .unwrap();
                shared_data.write().await.user_messages.push_str(&format!(
                    "Binance: \n{}",
                    serde_json::to_string_pretty(position).unwrap()
                ));
                match side {
                    Side::Buy => {
                        shared_data.write().await.long_avg_price =
                            position["entryPrice"].as_str().unwrap().parse().unwrap();
                    }
                    Side::Sell => {
                        shared_data.write().await.short_avg_price =
                            position["entryPrice"].as_str().unwrap().parse().unwrap();
                    }
                }
            }
            CryptoExchange::Okex => {
                let positions = okex::get_positions().await.unwrap();
                if let Some(position) = positions
                    .iter()
                    .find(|positions| positions["instId"].as_str().unwrap() == self.symbol)
                {
                    shared_data.write().await.user_messages.push_str(&format!(
                        "Okex: \n{}",
                        serde_json::to_string_pretty(position).unwrap()
                    ));
                    match side {
                        Side::Buy => {
                            shared_data.write().await.long_avg_price =
                                position["avgPx"].as_str().unwrap().parse().unwrap();
                        }
                        Side::Sell => {
                            shared_data.write().await.short_avg_price =
                                position["avgPx"].as_str().unwrap().parse().unwrap();
                        }
                    }
                }
            }
            CryptoExchange::FTX => {
                let positions = ftx::get_positions().await.unwrap();
                if let Some(position) = positions
                    .iter()
                    .find(|positions| positions["future"].as_str().unwrap() == self.symbol)
                {
                    shared_data.write().await.user_messages.push_str(&format!(
                        "FTX: \n{}",
                        serde_json::to_string_pretty(position).unwrap()
                    ));
                    match side {
                        Side::Buy => {
                            shared_data.write().await.long_avg_price =
                                position["entryPrice"].as_f64().unwrap();
                        }
                        Side::Sell => {
                            shared_data.write().await.short_avg_price =
                                position["entryPrice"].as_f64().unwrap();
                        }
                    }
                }
            }
            CryptoExchange::Bybit => {
                let positions = bybit::get_positions().await.unwrap();
                if let Some(position) = positions
                    .iter()
                    .find(|positions| positions["symbol"].as_str().unwrap() == self.symbol)
                {
                    shared_data.write().await.user_messages.push_str(&format!(
                        "Bybit: \n{}",
                        serde_json::to_string_pretty(position).unwrap()
                    ));
                    match side {
                        Side::Buy => {
                            shared_data.write().await.long_avg_price =
                                position["entry_price"].as_f64().unwrap();
                        }
                        Side::Sell => {
                            shared_data.write().await.short_avg_price =
                                position["entry_price"].as_f64().unwrap();
                        }
                    }
                }
            }
        }
    }
    pub async fn private_socket(&self, data: StrategyData) {
        let private_url = match self.exchange {
            CryptoExchange::Binance => url::Url::parse(&format!(
                "{}/ws/{}",
                BINANCE_PUBLIC_WS_LIVE,
                binance::request_listen_key().await
            ))
            .unwrap(),
            CryptoExchange::Okex => url::Url::parse(OKEX_PRIVATE_WS_LIVE).unwrap(),
            CryptoExchange::Bybit => return,
            CryptoExchange::FTX => return,
        };
        println!("Connecting to Private {}...", private_url);
        match connect_async(private_url).await {
            Ok((stream, response)) => {
                let start_time = tokio::time::Instant::now();
                println!(
                    "{:#?} private socket status: {:#?}",
                    self.exchange,
                    response.status()
                );
                let (mut write, read) = stream.split();
                if self.exchange == CryptoExchange::Okex {
                    write
                        .send(Message::Text(okex::websocket_login()))
                        .await
                        .unwrap();
                    write
                        .send(Message::Text(okex::subscribe_to_order_updates()))
                        .await
                        .unwrap();
                }
                let receive_future = read.for_each(|message| async {
                    if data.read().await.close_sockets {
                        return;
                    }
                    match message {
                        Ok(msg) => match &msg {
                            Message::Text(textmsg) => {
                                let json: Value = serde_json::from_str(&textmsg).unwrap();
                                if check_for_order_fill(self.exchange, &json) {
                                    match self.side {
                                        Side::Buy => {
                                            data.write().await.buy_order_filled = true;

                                            send_email(
                                                "receiver@tradequant.pro",
                                                &serde_json::to_string_pretty(&json).unwrap(),
                                                "Order Filled",
                                            )
                                            .await;
                                        }
                                        Side::Sell => {
                                            data.write().await.sell_order_filled = true;
                                            send_email(
                                                "receiver@tradequant.pro",
                                                &serde_json::to_string_pretty(&json).unwrap(),
                                                "Order Filled",
                                            )
                                            .await;
                                        }
                                    }
                                }
                            }
                            Message::Ping(_) => {
                                println!("Received ping from {:#?} private", self.exchange)
                            }
                            Message::Pong(_) => {
                                println!("Received pong from {:#?} private", self.exchange)
                            }
                            a => {
                                println!("{:#?}", a)
                            }
                        },
                        Err(e) => {
                            println!("Error reading message in socket: {}", e)
                        }
                    }
                });
                let interval = tokio::time::interval(time::Duration::from_secs(15));
                let write_future =
                    IntervalStream::new(interval).fold(write, |mut pwrite, _| async {
                        if start_time.elapsed().as_secs() > 3600
                            && start_time.elapsed().as_secs() < 3616
                            && self.exchange == CryptoExchange::Binance
                        {
                            binance::keep_alive_listen_key().await.unwrap();
                        }
                        pwrite
                            .send(Message::Ping("ping".as_bytes().to_vec()))
                            .await
                            .unwrap();
                        if data.read().await.close_sockets {
                            match pwrite.close().await {
                                Ok(_) => {
                                    println!("Succesfully closed {:#?} socket", self.exchange);
                                    if self.exchange == CryptoExchange::Binance {
                                        binance::delete_binance_listen_key().await;
                                    }
                                    return pwrite;
                                }
                                Err(_) => println!("Could not closed {:#?} socket", self.exchange),
                            }
                        }
                        pwrite
                    });
                tokio::select! {
                    () = receive_future => (),
                    _ = write_future => (),
                }
            }
            Err(e) => {
                println!(
                    "Could not connect to {:#?} Private. Error: {}",
                    self.exchange, e
                )
            }
        }
    }
    pub async fn connect_to_sockets(&self, data: StrategyData) {
        let (exchange_ws_url, message) = match self.exchange {
            CryptoExchange::Binance => (
                url::Url::parse(&format!("{}/ws/{}", BINANCE_PUBLIC_WS_LIVE, self.symbol)).unwrap(),
                binance::subscribe_to_ticker(&self.symbol),
            ),
            CryptoExchange::Okex => (
                url::Url::parse(OKEX_PUBLIC_WS_LIVE).unwrap(),
                okex::subscribe_to_depth(&self.symbol),
            ),
            CryptoExchange::FTX => (
                url::Url::parse(FTX_PUBLIC_WS_LIVE).unwrap(),
                ftx::subscribe_to_ticker(&self.symbol),
            ),
            CryptoExchange::Bybit => (
                url::Url::parse(BYBIT_PUBLIC_WS_LIVE).unwrap(),
                bybit::subscribe_to_ticker(&self.symbol),
            ),
        };
        println!("Connecting to {}...", exchange_ws_url);
        match connect_async(exchange_ws_url).await {
            Ok((stream, response)) => {
                println!("{:#?} status: {:#?}", self.exchange, response.status());
                let (mut write, read) = stream.split();
                write.send(Message::Text(message)).await.unwrap();
                match self.exchange {
                    CryptoExchange::Bybit => {
                        write
                            .send(Message::Text(bybit::websocket_login()))
                            .await
                            .unwrap();
                        write
                            .send(Message::Text(bybit::subscribe_to_order_updates()))
                            .await
                            .unwrap();
                    }
                    CryptoExchange::FTX => {
                        write
                            .send(Message::Text(ftx::websocket_login()))
                            .await
                            .unwrap();
                        write
                            .send(Message::Text(ftx::subscribe_to_orders()))
                            .await
                            .unwrap()
                    }
                    _ => {}
                };
                let receive_future = read.for_each(|msg| async {
                    if data.read().await.close_sockets {
                        return;
                    }
                    match &msg.unwrap() {
                        Message::Text(textmsg) => {
                            let json: Value = serde_json::from_str(&textmsg).unwrap();
                            if check_for_order_fill(self.exchange, &json) {
                                match self.side {
                                    Side::Buy => {
                                        data.write().await.buy_order_filled = true;
                                        send_email(
                                            "receiver@tradequant.pro",
                                            &serde_json::to_string_pretty(&json).unwrap(),
                                            "Order Filled",
                                        )
                                        .await;
                                    }
                                    Side::Sell => {
                                        data.write().await.sell_order_filled = true;
                                        send_email(
                                            "receiver@tradequant.pro",
                                            &serde_json::to_string_pretty(&json).unwrap(),
                                            "Order Filled",
                                        )
                                        .await;
                                    }
                                }
                            };
                            if self.exchange != CryptoExchange::Bybit {
                                let quote = Quote::from_exchange(self.exchange, json);
                                match self.side {
                                    Side::Buy => {
                                        data.write().await.buy_quote = quote;
                                    }
                                    Side::Sell => {
                                        data.write().await.sell_quote = quote;
                                    }
                                }
                            } else {
                                match self.side {
                                    Side::Buy => {
                                        let mut lock = data.write().await;
                                        match &lock.buy_quote {
                                            Some(old_quote) => {
                                                lock.buy_quote =
                                                    Some(old_quote.from_bybit_update(json));
                                            }
                                            None => {
                                                lock.buy_quote = Some(Quote::from_bybit_new(json));
                                            }
                                        }
                                    }
                                    Side::Sell => {
                                        let mut lock = data.write().await;
                                        match &lock.buy_quote {
                                            Some(old_quote) => {
                                                lock.sell_quote =
                                                    Some(old_quote.from_bybit_update(json));
                                            }
                                            None => {
                                                lock.sell_quote = Some(Quote::from_bybit_new(json));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Message::Ping(_) => {
                            println!("Received ping from {:#?}", self.exchange)
                        }
                        Message::Pong(_) => {
                            println!("Received pong from {:#?}", self.exchange)
                        }
                        a => {
                            println!("{:#?}", a)
                        }
                    }
                });
                let interval = tokio::time::interval(time::Duration::from_secs(15));
                let write_future =
                    IntervalStream::new(interval).fold(write, |mut pwrite, _| async {
                        pwrite
                            .send(Message::Ping("ping".as_bytes().to_vec()))
                            .await
                            .unwrap();
                        if data.read().await.close_sockets {
                            match pwrite.close().await {
                                Ok(_) => {
                                    println!("Succesfully closed {:#?} socket", self.exchange);
                                    return pwrite;
                                }
                                Err(_) => println!("Could not closed {:#?} socket", self.exchange),
                            }
                        }
                        pwrite
                    });
                tokio::select! {
                    () = receive_future => (),
                    _ = write_future => (),
                }
            }
            Err(e) => {
                println!("Could not connect to {:#?}. Error: {}", self.exchange, e)
            }
        }
    }
    pub async fn execute_limit(&self, shared_data: StrategyData) {
        let quote = match self.side {
            Side::Buy => shared_data.read().await.buy_quote.clone().unwrap(),
            Side::Sell => shared_data.read().await.sell_quote.clone().unwrap(),
        };
        match self.exchange {
            CryptoExchange::Okex => {
                let (side, price) = match self.side {
                    Side::Buy => ("buy", quote.bid),
                    Side::Sell => ("sell", quote.ask),
                };
                let leverage_ok = okex::set_leverage(&self.symbol, "1").await.unwrap();
                println!("Okex Leverage adjust success: {:#?}", leverage_ok);
                println!(
                    "Okex Args: {} {} {} {}",
                    &self.symbol, side, &self.quantity, price
                );
                if DEPLOY && leverage_ok {
                    let order_id =
                        okex::send_limit_order(&self.symbol, side, &self.quantity, price)
                            .await
                            .unwrap();
                    match self.side {
                        Side::Buy => {
                            shared_data.write().await.buy_order = Some(OrderID {
                                exchange: self.exchange,
                                order_id,
                                filled: false,
                                symbol: self.symbol.clone(),
                                amount: self.quantity.clone(),
                                side: side.to_string(),
                            })
                        }
                        Side::Sell => {
                            shared_data.write().await.sell_order = Some(OrderID {
                                exchange: self.exchange,
                                order_id,
                                filled: false,
                                symbol: self.symbol.clone(),
                                amount: self.quantity.clone(),
                                side: side.to_string(),
                            })
                        }
                    }
                };
            }
            CryptoExchange::FTX => {
                let (side, price) = match self.side {
                    Side::Buy => ("buy", quote.bid),
                    Side::Sell => ("sell", quote.ask),
                };
                println!(
                    "FTX Args {} {} {} {}",
                    &self.symbol,
                    side,
                    price,
                    self.quantity.parse::<f64>().unwrap()
                );
                if DEPLOY {
                    let order_id = ftx::send_limit_order(
                        &self.symbol,
                        side,
                        price,
                        self.quantity.parse().unwrap(),
                    )
                    .await
                    .unwrap();
                    match self.side {
                        Side::Buy => {
                            shared_data.write().await.buy_order = Some(OrderID {
                                exchange: self.exchange,
                                order_id,
                                filled: false,
                                symbol: self.symbol.clone(),
                                amount: self.quantity.clone(),
                                side: side.to_string(),
                            })
                        }
                        Side::Sell => {
                            shared_data.write().await.sell_order = Some(OrderID {
                                exchange: self.exchange,
                                order_id,
                                filled: false,
                                symbol: self.symbol.clone(),
                                amount: self.quantity.clone(),
                                side: side.to_string(),
                            })
                        }
                    }
                };
            }
            CryptoExchange::Bybit => {
                let (side, price) = match self.side {
                    Side::Buy => ("Buy", quote.bid),
                    Side::Sell => ("Sell", quote.ask),
                };
                let leverage_good = bybit::set_leverage(&self.symbol, 1).await.unwrap();
                println!("Bybit Leverage adjust success: {:#?}", leverage_good);
                println!(
                    "ByBit Args: {} {} {} {}",
                    &self.symbol,
                    side,
                    price,
                    self.quantity.parse::<f64>().unwrap(),
                );
                if DEPLOY && leverage_good {
                    let order_id = bybit::send_limit_order(
                        &self.symbol,
                        side,
                        price,
                        self.quantity.parse().unwrap(),
                    )
                    .await
                    .unwrap();
                    match self.side {
                        Side::Buy => {
                            shared_data.write().await.buy_order = Some(OrderID {
                                exchange: self.exchange,
                                order_id,
                                filled: false,
                                symbol: self.symbol.clone(),
                                amount: self.quantity.clone(),
                                side: side.to_string(),
                            })
                        }
                        Side::Sell => {
                            shared_data.write().await.sell_order = Some(OrderID {
                                exchange: self.exchange,
                                order_id,
                                filled: false,
                                symbol: self.symbol.clone(),
                                amount: self.quantity.clone(),
                                side: side.to_string(),
                            })
                        }
                    }
                }
            }
            CryptoExchange::Binance => {
                let (side, price) = match self.side {
                    Side::Buy => ("BUY", quote.bid),
                    Side::Sell => ("SELL", quote.ask),
                };
                let leverage_ok = binance::set_leverage(&self.symbol, 1).await.unwrap();
                println!("Binance Leverage adjust success: {:#?}", leverage_ok);
                let decimals = binance::get_contract_precision_level(&self.symbol)
                    .await
                    .unwrap();
                println!("Binance decimals for {} = {}", self.symbol, decimals);
                let amount = format!(
                    "{:.prec$}",
                    self.quantity.parse::<f64>().unwrap(),
                    prec = decimals
                );
                println!(
                    "Binance Args: {} {} {} {}",
                    &self.symbol, side, price, amount
                );
                if DEPLOY && leverage_ok {
                    let order_id = binance::send_limit_order(&self.symbol, side, price, &amount)
                        .await
                        .unwrap();
                    match self.side {
                        Side::Buy => {
                            shared_data.write().await.buy_order = Some(OrderID {
                                exchange: self.exchange,
                                order_id,
                                filled: false,
                                symbol: self.symbol.clone(),
                                amount: self.quantity.clone(),
                                side: side.to_string(),
                            })
                        }
                        Side::Sell => {
                            shared_data.write().await.sell_order = Some(OrderID {
                                exchange: self.exchange,
                                order_id,
                                filled: false,
                                symbol: self.symbol.clone(),
                                amount: self.quantity.clone(),
                                side: side.to_string(),
                            })
                        }
                    }
                }
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct SpreadTrade {
    pub timestamp: i64,
    pub long_directive: Directive,
    pub short_directive: Directive,
}
impl SpreadTrade {
    /*
    pub async fn execute(self, shared_data: StrategyData) {
        tokio::join!(
            self.short_directive.execute(shared_data.clone()),
            self.long_directive.execute(shared_data.clone())
        );
        let mut lock = shared_data.write().await;
        send_email(
            "receiver@tradequant.pro",
            &lock.user_messages,
            "Trades Were Sent",
        )
        .await;
        lock.user_messages.clear();
    }
    */
    pub async fn execute_with_socket(&self, shared_data: StrategyData, exit: bool) {
        tokio::join!(
            self.short_directive.connect_to_sockets(shared_data.clone()),
            self.long_directive.connect_to_sockets(shared_data.clone()),
            self.short_directive.private_socket(shared_data.clone()),
            self.long_directive.private_socket(shared_data.clone()),
            manage_orders(shared_data.clone(), self, exit),
        );
        reset_strategy_values(shared_data).await;
    }
    pub fn create_closing_trades(self) -> Self {
        Self {
            long_directive: self.long_directive.create_closing_directive(),
            short_directive: self.short_directive.create_closing_directive(),
            ..self
        }
    }
    pub async fn check_positions(&self, shared_data: StrategyData) {
        tokio::join!(
            self.short_directive
                .check_status(shared_data.clone(), Side::Sell),
            self.long_directive
                .check_status(shared_data.clone(), Side::Buy)
        );
    }
    pub async fn get_current_spread(&self) -> f64 {
        let long_rate = match self.long_directive.exchange {
            CryptoExchange::Binance => {
                binance::get_single_funding_rate(&self.long_directive.symbol)
                    .await
                    .unwrap()
            }
            CryptoExchange::Okex => okex::get_single_funding_rate(&self.long_directive.symbol)
                .await
                .unwrap(),
            CryptoExchange::FTX => ftx::get_single_funding_rate(&self.long_directive.symbol)
                .await
                .unwrap(),
            CryptoExchange::Bybit => bybit::get_single_funding_rate(&self.long_directive.symbol)
                .await
                .unwrap(),
        };
        let short_rate = match self.short_directive.exchange {
            CryptoExchange::Binance => {
                binance::get_single_funding_rate(&self.short_directive.symbol)
                    .await
                    .unwrap()
            }
            CryptoExchange::Okex => okex::get_single_funding_rate(&self.short_directive.symbol)
                .await
                .unwrap(),
            CryptoExchange::FTX => ftx::get_single_funding_rate(&self.short_directive.symbol)
                .await
                .unwrap(),
            CryptoExchange::Bybit => bybit::get_single_funding_rate(&self.short_directive.symbol)
                .await
                .unwrap(),
        };
        let spread = 100.0 * (short_rate - long_rate);
        println!("Current Spread: {}", spread);
        spread
    }
    pub fn involve_exchange(&self, exchange: CryptoExchange) -> bool {
        self.long_directive.exchange == exchange || self.short_directive.exchange == exchange
    }
    pub async fn get_payments_so_far(&self) -> f64 {
        let long_payments: f64 = match self.long_directive.exchange {
            CryptoExchange::Binance => {
                binance::get_payments_so_far(&self.long_directive.symbol, self.timestamp)
                    .await
                    .unwrap()
            }
            CryptoExchange::Okex => {
                okex::get_payments_so_far(&self.long_directive.symbol, self.timestamp)
                    .await
                    .unwrap()
            }
            CryptoExchange::FTX => {
                ftx::get_payments_so_far(&self.long_directive.symbol, self.timestamp)
                    .await
                    .unwrap()
            }
            CryptoExchange::Bybit => {
                bybit::get_payments_so_far(&self.long_directive.symbol, self.timestamp)
                    .await
                    .unwrap()
            }
        };
        let short_payments: f64 = match self.short_directive.exchange {
            CryptoExchange::Binance => {
                binance::get_payments_so_far(&self.short_directive.symbol, self.timestamp)
                    .await
                    .unwrap()
            }
            CryptoExchange::Okex => {
                okex::get_payments_so_far(&self.short_directive.symbol, self.timestamp)
                    .await
                    .unwrap()
            }
            CryptoExchange::FTX => {
                ftx::get_payments_so_far(&self.short_directive.symbol, self.timestamp)
                    .await
                    .unwrap()
            }
            CryptoExchange::Bybit => {
                bybit::get_payments_so_far(&self.short_directive.symbol, self.timestamp)
                    .await
                    .unwrap()
            }
        };
        long_payments + short_payments
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub ask: f64,
    pub bid: f64,
    pub exchange: CryptoExchange,
}
impl Quote {
    pub fn from_exchange(exchange: CryptoExchange, message: Value) -> Option<Self> {
        let as_map = message.as_object().unwrap();
        match exchange {
            CryptoExchange::Binance => {
                if as_map.contains_key("a") && as_map.contains_key("b") {
                    Some(Self {
                        ask: message["a"].as_str().unwrap().parse().unwrap(),
                        bid: message["b"].as_str().unwrap().parse().unwrap(),
                        exchange,
                    })
                } else {
                    None
                }
            }
            CryptoExchange::Bybit => {
                panic!("this function not to be used with bybit");
            }
            CryptoExchange::FTX => {
                if as_map.contains_key("channel")
                    && as_map["channel"].as_str().unwrap() == "ticker"
                    && as_map["type"].as_str().unwrap() == "update"
                {
                    Some(Self {
                        ask: message["data"]["ask"].as_f64().unwrap(),
                        bid: message["data"]["bid"].as_f64().unwrap(),
                        exchange,
                    })
                } else {
                    None
                }
            }
            CryptoExchange::Okex => {
                if message["arg"]["channel"].as_str().unwrap() == "books5"
                    && !as_map.contains_key("event")
                {
                    Some(Self {
                        ask: message["data"][0]["asks"][0][0]
                            .as_str()
                            .unwrap()
                            .parse()
                            .unwrap(),
                        bid: message["data"][0]["bids"][0][0]
                            .as_str()
                            .unwrap()
                            .parse()
                            .unwrap(),
                        exchange,
                    })
                } else {
                    None
                }
            }
        }
    }
    pub fn from_bybit_new(message: Value) -> Self {
        Self {
            ask: message["data"]["ask1_price_e4"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap()
                / 10000.0,
            bid: message["data"]["bid1_price_e4"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap()
                / 10000.0,
            exchange: CryptoExchange::Bybit,
        }
    }
    pub fn from_bybit_update(&self, message: Value) -> Self {
        Self {
            ask: message["data"]["update"][0]["ask1_price_e4"]
                .as_str()
                .unwrap_or(&(self.ask * 10000.0).to_string())
                .parse::<f64>()
                .unwrap_or(self.ask)
                / 10000.0,
            bid: message["data"]["update"][0]["bid1_price_e4"]
                .as_str()
                .unwrap_or(&(self.bid * 10000.0).to_string())
                .parse::<f64>()
                .unwrap_or(self.bid)
                / 10000.0,
            ..*self
        }
    }
}

pub async fn manage_orders(shared_data: StrategyData, trades: &SpreadTrade, exit: bool) {
    loop {
        if !exit {
            let lock = shared_data.read().await;
            if lock.buy_quote.is_some() && lock.sell_quote.is_some() {
                if lock.orders_were_sent {
                    if lock.buy_order_filled && 
                    !lock.sell_order_filled && 
                    !lock.order_cancel_sent {
                        send_email(
                            "receiver@tradequant.pro",
                            &format!(
                                "{:#?}",
                                lock.sell_order
                                    .as_ref()
                                    .unwrap()
                                    .cancel_and_replace_market()
                                    .await
                            ),
                            "Order Replaced",
                        )
                        .await;
                        drop(lock);
                        shared_data.write().await.order_cancel_sent = true;
                    } else if !lock.buy_order_filled
                        && lock.sell_order_filled
                        && !lock.order_cancel_sent
                    {
                        send_email(
                            "receiver@tradequant.pro",
                            &format!(
                                "{:#?}",
                                lock.buy_order
                                    .as_ref()
                                    .unwrap()
                                    .cancel_and_replace_market()
                                    .await
                            ),
                            "Order Replaced",
                        )
                        .await;
                        drop(lock);
                        shared_data.write().await.order_cancel_sent = true;
                    } else if lock.buy_order_filled && lock.sell_order_filled {
                        drop(lock);
                        send_email(
                            "receiver@tradequant.pro",
                            "Both Orders Filled",
                            "Orders Filled",
                        )
                        .await;
                        shared_data.write().await.close_sockets = true;
                        return;
                    }
                } else if !lock.orders_were_sent {
                    drop(lock);
                    shared_data.write().await.orders_were_sent = true;
                    send_email(
                        "receiver@tradequant.pro",
                        "Orders Sent",
                        "Funding Rate Algo",
                    )
                    .await;
                    tokio::join!(
                        trades.long_directive.execute_limit(shared_data.clone()),
                        trades.short_directive.execute_limit(shared_data.clone())
                    );
                    let open = shared_data.read().await;
                    if open.sell_order.is_none() || open.buy_order.is_none() {
                        if open.sell_order.is_none() && open.buy_order.is_some() {
                            drop(open);
                            trades
                                .long_directive
                                .create_closing_directive()
                                .execute(shared_data.clone())
                                .await;
                            send_email(
                                "receiver@tradequant.pro",
                                "order rejected, and other order cancelled",
                                "Order Issue",
                            )
                            .await;
                        } else if open.sell_order.is_some() && open.buy_order.is_none() {
                            drop(open);
                            trades
                                .short_directive
                                .create_closing_directive()
                                .execute(shared_data.clone())
                                .await;
                            send_email(
                                "receiver@tradequant.pro",
                                "order rejected, and other order cancelled",
                                "Order Issue",
                            )
                            .await;
                        }
                    }
                } else {
                    drop(lock)
                }
            }
        } else {
            let lock = shared_data.read().await;
            if lock.buy_quote.is_some() && lock.sell_quote.is_some() {
                let long_profit = lock.buy_quote.as_ref().unwrap().bid - lock.long_avg_price;
                let short_profit = lock.short_avg_price - lock.sell_quote.as_ref().unwrap().ask;
                let combined_profit = long_profit + short_profit;
                if lock.orders_were_sent {
                    if lock.buy_order_filled && !lock.sell_order_filled && !lock.order_cancel_sent {
                        send_email(
                            "receiver@tradequant.pro",
                            &format!(
                                "{:#?}",
                                lock.sell_order
                                    .as_ref()
                                    .unwrap()
                                    .cancel_and_replace_market()
                                    .await
                            ),
                            "Order Replaced",
                        )
                        .await;
                        drop(lock);
                        shared_data.write().await.order_cancel_sent = true;
                        return;
                    } else if !lock.buy_order_filled
                        && lock.sell_order_filled
                        && !lock.order_cancel_sent
                    {
                        send_email(
                            "receiver@tradequant.pro",
                            &format!(
                                "{:#?}",
                                lock.buy_order
                                    .as_ref()
                                    .unwrap()
                                    .cancel_and_replace_market()
                                    .await
                            ),
                            "Order Replaced",
                        )
                        .await;
                        drop(lock);
                        return shared_data.write().await.order_cancel_sent = true;
                    } else if lock.buy_order_filled && lock.sell_order_filled {
                        drop(lock);
                        shared_data.write().await.close_sockets = true;
                        return;
                    }
                }
                if !lock.orders_were_sent && combined_profit >= 0.0 {
                    drop(lock);
                    shared_data.write().await.orders_were_sent = true;
                    send_email(
                        "receiver@tradequant.pro",
                        "Orders Sent",
                        "Funding Rate Algo",
                    )
                    .await;
                    tokio::join!(
                        trades.long_directive.execute_limit(shared_data.clone()),
                        trades.short_directive.execute_limit(shared_data.clone())
                    );
                    let open = shared_data.read().await;
                    if open.sell_order.is_none() || open.buy_order.is_none() {
                        if open.sell_order.is_none() && open.buy_order.is_some() {
                            drop(open);
                            trades
                                .long_directive
                                .create_closing_directive()
                                .execute(shared_data.clone())
                                .await;
                            send_email(
                                "receiver@tradequant.pro",
                                "order rejected, and other order cancelled",
                                "Order Issue",
                            )
                            .await;
                        } else if open.sell_order.is_some() && open.buy_order.is_none() {
                            drop(open);
                            trades
                                .short_directive
                                .create_closing_directive()
                                .execute(shared_data.clone())
                                .await;
                            send_email(
                                "receiver@tradequant.pro",
                                "order rejected, and other order cancelled",
                                "Order Issue",
                            )
                            .await;
                        }
                    }
                } else {
                    drop(lock)
                }
            }
        }
    }
}

pub fn check_for_order_fill(exchange: CryptoExchange, message: &Value) -> bool {
    match exchange {
        CryptoExchange::Binance => match message["o"]["X"].as_str() {
            Some(status) => status == "FILLED",
            None => false,
        },
        CryptoExchange::Bybit => match message["data"][0]["order_status"].as_str() {
            Some(status) => status == "Filled",
            None => false,
        },
        CryptoExchange::FTX => match message["data"]["status"].as_str() {
            Some(status) => status == "closed",
            None => false,
        },
        CryptoExchange::Okex => match message["data"][0]["state"].as_str() {
            Some(status) => status == "filled",
            None => false,
        },
    }
}
