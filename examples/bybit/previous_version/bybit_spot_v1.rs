use crypto_botters::{
    bybit::{BybitHttpAuth, BybitOption},
    Client,
};
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

    // private DELETE
    let batch_cancel: serde_json::Value = client
        .delete(
            "/spot/order/batch-cancel",
            Some(&[("symbol", "BTCUSDT")]),
            [BybitOption::HttpAuth(BybitHttpAuth::SpotV1)],
        )
        .await
        .expect("failed to cancel orders");
    println!("Batch cancel result:\n{}", batch_cancel);

    // private GET
    let open_orders: serde_json::Value = client
        .get_no_query(
            "/spot/v1/open-orders",
            [BybitOption::HttpAuth(BybitHttpAuth::SpotV1)],
        )
        .await
        .expect("failed to get orders");
    println!("Open orders:\n{}", open_orders);

    // public GET
    let last_price: serde_json::Value = client
        .get(
            "/spot/quote/v1/ticker/price",
            Some(&[("symbol", "BTCUSDT")]),
            [BybitOption::Default],
        )
        .await
        .expect("failed to get price");
    println!("Last price:\n{}", last_price);
}
