mod binance;
mod bybit;
mod coin_tickers;
mod credentials;
mod ftx;
mod funding_rates;
mod okex;
mod order_entry;
mod output;
mod settings;
mod strategy;
use crate::{
    funding_rates::{calculate_duration_to_next_rate, manage_trades},
    output::open_file,
    settings::{DEPLOY, EIGHT_HOURS, FIFTEEN_MIN, TRADES_FILENAME},
    strategy::{FundingRateArb, StrategyData},
};
use funding_rates::run_funding_rate_strategy;
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
#[tokio::main]
async fn main() {
    let shared_data: StrategyData = Arc::new(tokio::sync::RwLock::new(FundingRateArb::new()));
    let main_start_instant = tokio::time::Instant::now()
        + calculate_duration_to_next_rate().unwrap_or(time::Duration::ZERO);
    let main_interval = match DEPLOY {
        true => time::interval_at(main_start_instant, time::Duration::from_secs(EIGHT_HOURS)),
        false => time::interval(time::Duration::from_secs(EIGHT_HOURS)),
    };
    let main_future = IntervalStream::new(main_interval).for_each(|_| {
        let data = shared_data.clone();
        async {
            if let Err(_) = open_file(TRADES_FILENAME) {
                run_funding_rate_strategy(data).await;
            }
        }
    });
    let position_interval = time::interval(time::Duration::from_secs(FIFTEEN_MIN));
    let position_future = IntervalStream::new(position_interval).for_each(|_| {
        let data = shared_data.clone();
        async {
            if let Ok(rate_spread) = open_file(TRADES_FILENAME) {
                manage_trades(data, rate_spread).await
            }
        }
    });
    tokio::select! {
        _ = main_future => (),
        _ = position_future => ()
    }
}
