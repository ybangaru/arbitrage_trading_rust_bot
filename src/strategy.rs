use std::{collections::HashMap, sync::Arc};

use crate::{binance, bybit, ftx, funding_rates::{FundingRates, RateSpread}, okex, order_entry::{CryptoExchange, Quote}};
pub type StrategyData = Arc<tokio::sync::RwLock<FundingRateArb>>;

#[derive(Debug, Clone)]
pub struct FundingRateArb {
    pub account_balances: HashMap<CryptoExchange,f64>,
    pub funding_rates: Vec<FundingRates>,
    pub trades: Option<RateSpread>,
    pub buy_quote: Option<Quote>,
    pub sell_quote: Option<Quote>,
    pub combined_pnl: f64,
    pub user_messages: String,
    pub buy_order: Option<OrderID>,
    pub sell_order: Option<OrderID>,
    pub close_sockets: bool,
    pub orders_were_sent:bool,
    pub order_cancel_sent: bool,
    pub buy_order_filled: bool,
    pub sell_order_filled:bool,
    pub long_avg_price: f64,
    pub short_avg_price: f64,
}
impl FundingRateArb {
    pub fn new() -> Self {
        Self {
            account_balances: HashMap::new(),
            funding_rates: Vec::new(),
            user_messages: String::new(),
            buy_quote: None,
            sell_quote: None,
            trades: None,
            close_sockets: false,
            orders_were_sent: false,
            buy_order_filled: false,
            sell_order_filled: false,
            order_cancel_sent: false,
            combined_pnl: 0.0,
            buy_order: None,
            sell_order: None,
            long_avg_price: 0.0,
            short_avg_price: 0.0,
        }
    }
    pub async fn get_account_balances(data:StrategyData) {
        tokio::join!(crate::binance::get_balance(data.clone()),
                     crate::bybit::get_balance(data.clone()),
                     crate::okex::get_balance(data.clone()),
                     crate::ftx::get_balance(data.clone())
                    );
    }
}

#[derive(Debug, Clone)]
pub struct OrderID {
    pub exchange: CryptoExchange,
    pub side: String,
    pub symbol: String,
    pub order_id: String,
    pub amount: String,
    pub filled: bool,
}
impl OrderID {
    pub async fn cancel(&self) -> bool {
        match self.exchange {
            CryptoExchange::Binance => {
                binance::cancel_order(&self.symbol, self.order_id.parse().unwrap()).await.unwrap()
            },
            CryptoExchange::Bybit => {
                bybit::cancel_order(&self.symbol, &self.order_id).await.unwrap()
            },
            CryptoExchange::FTX => {
                ftx::cancel_order(&self.order_id).await.unwrap()
            },
            CryptoExchange::Okex => {
                okex::cancel_order(&self.symbol, &self.order_id).await.unwrap()
            },
        }
    }
    pub async fn cancel_and_replace_market(&self) -> bool {
        if self.cancel().await {
            match self.exchange {
                CryptoExchange::Binance => {
                    binance::send_market_order(&self.symbol, &self.side, &self.amount).await.unwrap();
                    true
                },
                CryptoExchange::Bybit => {
                    bybit::send_market_order(&self.symbol, &self.side, self.amount.parse().unwrap()).await.unwrap();
                    true
                },
                CryptoExchange::FTX => {
                    ftx::send_market_order(&self.symbol, &self.side, self.amount.parse().unwrap()).await.unwrap();
                    true
                },
                CryptoExchange::Okex => {
                    okex::send_market_order(&self.symbol, &self.side, &self.amount).await.unwrap();
                    true
                },
            }
        } else {false}
    }
    
}

pub async fn reset_strategy_values(data:StrategyData) {
    let mut last_lock = data.write().await;
    last_lock.account_balances.clear();
    last_lock.funding_rates.clear();
    last_lock.trades = None;
    last_lock.buy_quote = None;
    last_lock.sell_quote = None;
    last_lock.combined_pnl = 0.0;
    last_lock.user_messages.clear();
    last_lock.buy_order = None;
    last_lock.sell_order = None;
    last_lock.close_sockets = false;
    last_lock.orders_were_sent = false;
    last_lock.order_cancel_sent = false;
    last_lock.buy_order_filled = false;
    last_lock.sell_order_filled = false;   
}