use crypto_botters::{
    binance::{BinanceOption, BinanceRequestResult},
    Client,
};
use serde_json::json;

struct Api {
    client: Client,
}

impl Api {
    fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    async fn get_orderbook(&self) -> BinanceRequestResult<serde_json::Value> {
        self.client
            .get(
                "https://api.binance.com/api/v3/ticker/bookTicker",
                Some(&json!({ "symbol": "BTCUSDT" })),
                [BinanceOption::Default],
            )
            .await
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,crypto_botters=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let api = Api::new();
    let handle = tokio::spawn(async move {
        tracing::info!("in async block");
        let orderbook = api.get_orderbook().await;
        println!("{:?}", orderbook);
    });
    let result = handle.await;
    println!("{:?}", result);
}
