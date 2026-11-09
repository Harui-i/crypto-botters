use crypto_botters::{bybit::BybitOption, Client};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,crypto_botters=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let client = Client::new();

    // public GET
    let symbols: serde_json::Value = client
        .get_no_query(
            "/contract/v3/public/copytrading/symbol/list",
            [BybitOption::Default],
        )
        .await
        .expect("failed to symbol list");
    println!("Symbols:\n{}", symbols);
}
