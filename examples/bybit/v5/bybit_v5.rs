use crypto_botters::{
    bybit::{BybitHttpAuth, BybitOption},
    Client,
};
use serde_json::json;
use std::env;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,crypto_botters=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let key = env::var("BYBIT_API_KEY").expect("no API key found");
    let secret = env::var("BYBIT_API_SECRET").expect("no API secret found");
    let mut client = Client::new();
    client.update_default_option(BybitOption::Key(key));
    client.update_default_option(BybitOption::Secret(secret));

    // private POST
    let cancel_all_result: serde_json::Value = client
        .post(
            "/v5/order/cancel-all",
            Some(json!({ "category": "spot" })),
            [BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)],
        )
        .await
        .expect("failed to cancel orders");
    println!("Cancel order result:\n{cancel_all_result}");

    // private GET
    let balance: serde_json::Value = client
        .get(
            "/v5/account/wallet-balance",
            Some(&[("accountType", "UNIFIED")]),
            [BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)],
        )
        .await
        .expect("failed to get balance");
    println!("Balance:\n{balance}");

    // public GET
    let ticker: serde_json::Value = client
        .get(
            "/v5/market/tickers",
            Some(&[("category", "spot"), ("symbol", "BTCUSDT")]),
            [BybitOption::Default],
        )
        .await
        .expect("failed to get ticker");
    println!("Ticker:\n{ticker}");
}
