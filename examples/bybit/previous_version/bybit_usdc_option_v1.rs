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

    // private POST
    let cancel_all: serde_json::Value = client
        .post_no_body(
            "/option/usdc/openapi/private/v1/cancel-all",
            [BybitOption::HttpAuth(BybitHttpAuth::UsdcContractV1)],
        )
        .await
        .expect("failed to cancel orders");
    println!("Cancel all result:\n{}", cancel_all);

    // private GET
    let open_orders: serde_json::Value = client
        .get_no_query(
            "/option/usdc/openapi/private/v1/trade/query-active-orders",
            [BybitOption::HttpAuth(BybitHttpAuth::UsdcContractV1)],
        )
        .await
        .expect("failed to get orders");
    println!("Open orders:\n{}", open_orders);

    // public GET
    let symbol_info: serde_json::Value = client
        .get(
            "/option/usdc/openapi/public/v1/tick",
            Some(&[("symbol", "BTC-5JAN23-18500-C")]),
            [BybitOption::Default],
        )
        .await
        .expect("failed to get symbol info");
    println!("Symbol info:\n{}", symbol_info);
}
