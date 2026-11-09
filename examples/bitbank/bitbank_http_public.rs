use crypto_botters::{
    bitbank::{BitbankHttpUrl, BitbankOption},
    Client,
};
use rust_decimal::prelude::*;
use serde::Deserialize;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,crypto_botters=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let public = BitbankHttpUrl::Public;
    let mut client = Client::new();

    // change bitbank's url to public one.
    client.update_default_option(BitbankOption::HttpUrl(public));

    #[allow(dead_code)]
    #[derive(Deserialize, Debug)]
    struct Ticker {
        success: i32,
        data: TickerData,
    }

    #[allow(dead_code)]
    #[derive(Deserialize, Debug)]
    struct TickerData {
        #[serde(with = "rust_decimal::serde::float")]
        sell: Decimal,
        #[serde(with = "rust_decimal::serde::float")]
        buy: Decimal,
        #[serde(with = "rust_decimal::serde::float")]
        high: Decimal,
        #[serde(with = "rust_decimal::serde::float")]
        low: Decimal,
        #[serde(with = "rust_decimal::serde::float")]
        open: Decimal,
        #[serde(with = "rust_decimal::serde::float")]
        last: Decimal,
        #[serde(with = "rust_decimal::serde::float")]
        vol: Decimal,
        timestamp: i64,
    }

    // https://github.com/bitbankinc/bitbank-api-docs/blob/master/public-api.md
    // access https://public.bitbank.cc/btc_jpy/ticker

    let ticker: Ticker = client
        .get_no_query("/btc_jpy/ticker", [BitbankOption::Default])
        .await
        .expect("failed to get ticker");

    println!("BTC ticker:\n{:?}", ticker);
}
