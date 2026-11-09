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
    client.update_default_option(BybitOption::RecvWindow(6000));

    let cancel_all: serde_json::Value = client
        .post(
            "/contract/v3/private/order/cancel-all",
            Some(json!({"symbol": "BTCUSDT"})),
            [BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)],
        )
        .await
        .expect("failed to cancel orders");
    println!("Cancel all result:\n{}", cancel_all);
}
