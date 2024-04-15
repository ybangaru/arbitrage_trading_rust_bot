pub const DEPLOY: bool = true;
pub const ACCT_VALUE_TO_USE: f64 = 0.8;
pub const TRADE_TIME_OFFSET_SEC: i64 = -1800;
// BASE URL'S
//BINANCE
pub const BINANCE_REST_API_URL_LIVE: &str = "https://fapi.binance.com";
pub const BINANCE_PUBLIC_WS_LIVE:&str = "wss://fstream.binance.com";
//BYBIT
pub const BYBIT_REST_API_URL_LIVE: &str = "https://api.bybit.com";
pub const BYBIT_PUBLIC_WS_LIVE:&str = "wss://stream.bybit.com/realtime_public";
//FTX
pub const FTX_REST_API_URL_LIVE: &str = "https://ftx.com/api";
pub const FTX_PUBLIC_WS_LIVE:&str = "wss://ftx.com/ws/";
//OKEX
    //AWS
pub const OKEX_REST_API_URL_LIVE: &str = "https://aws.okex.com";
pub const OKEX_PUBLIC_WS_LIVE:&str = "wss://wsaws.okex.com:8443/ws/v5/public";
pub const OKEX_PRIVATE_WS_LIVE:&str = "wss://wsaws.okex.com:8443/ws/v5/private";
    // NOT AWS
//pub const OKEX_REST_API_URL_LIVE: &str = "https://www.okex.com";
//pub const OKEX_PUBLIC_WS_LIVE:&str = "wss://ws.okex.com:8443/ws/v5/public";
//pub const OKEX_PRIVATE_WS_LIVE:&str = "wss://ws.okex.com:8443/ws/v5/private";
// COMMISSION_RATES
pub const BINANCE_MAKER_FEE: f64 = 0.018;
pub const BINANCE_TAKER_FEE: f64 = 0.036;
pub const BYBIT_MAKER_FEE: f64 = 0.025;
pub const BYBIT_TAKER_FEE: f64 = 0.07;
pub const OKEX_MAKER_FEE: f64 = 0.02;
pub const OKEX_TAKER_FEE: f64 = 0.05;
pub const FTX_MAKER_FEE: f64 = 0.02;
pub const FTX_TAKER_FEE: f64 = 0.07;
pub const ADJUSTER: f64 = 1000000000.0;
pub const EIGHT_HOURS: u64 = 28800;
pub const FIFTEEN_MIN: u64 = 900;
pub const TRADES_FILENAME: &str = "trades.json";
pub const OKEX_SPEC_FILE: &str = "okex_contract_spec.json";
