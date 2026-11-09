use crypto_botters::{
    binance::{BinanceHttpUrl, BinanceOption},
    Client,
};
use rust_decimal::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,crypto_botters=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let mut client = Client::new();
    client.update_default_option(BinanceOption::HttpUrl(BinanceHttpUrl::Spot));

    // typed
    #[derive(Serialize)]
    struct TickerParams<'a> {
        symbol: &'a str,
    }

    #[derive(Deserialize)]
    struct Ticker {
        #[serde(with = "rust_decimal::serde::str")]
        price: Decimal,
        #[allow(dead_code)]
        symbol: String,
    }

    let ticker: Ticker = client
        .get(
            "/api/v3/ticker/price",
            Some(&TickerParams { symbol: "BTCUSDT" }),
            [BinanceOption::Default],
        )
        .await
        .expect("failed to get tickers");
    println!("BTC & ETH prices:\n{:?}", ticker.price);

    // not typed
    let orderbook: serde_json::Value = client
        .get(
            "https://api.binance.com/api/v3/ticker/bookTicker",
            Some(&json!({ "symbol": "BTCUSDT" })),
            [BinanceOption::HttpUrl(BinanceHttpUrl::None)],
        )
        .await
        .expect("failed get orderbook");
    println!("BTC bidPrice:\n{:?}", orderbook["bidPrice"]);
}
