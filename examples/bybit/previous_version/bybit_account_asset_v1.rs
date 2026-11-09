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

    // private GET
    let internal_transfers: serde_json::Value = client
        .get_no_query(
            "/asset/v1/private/transfer/list",
            [BybitOption::HttpAuth(BybitHttpAuth::BelowV3)],
        )
        .await
        .expect("failed to get internal transfer list");
    println!("Internal transfers:\n{}", internal_transfers);
}
