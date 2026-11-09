use crypto_botters::{bybit::BybitOption, Client};
use serde::Serialize;

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

    // typed parameter, public
    #[derive(Serialize)]
    struct SymbolInfoParams<'a> {
        category: &'a str,
        symbol: &'a str,
    }

    let symbol_info: serde_json::Value = client
        .get(
            "/derivatives/v3/public/tickers",
            Some(&SymbolInfoParams {
                category: "linear",
                symbol: "BTCUSDT",
            }),
            [BybitOption::Default],
        )
        .await
        .expect("failed get symbol info");
    println!("Unified margin BTCUSDT info:\n{}", symbol_info);
}
