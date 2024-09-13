use crypto_botters::{bitbank::{BitbankHttpUrl, BitbankOption}, Client};
use log::LevelFilter;
use rust_decimal::prelude::*;
use serde::Deserialize;

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(LevelFilter::Debug)
        .init();


    let public = BitbankHttpUrl::Public;
    let mut client = Client::new();

    // change bitbank's url to public one.
    client.update_default_option(BitbankOption::HttpUrl(public));

    // ticker response is like this:
    /*
    
    {
        "success": 1,
        "data": {
            "sell": "1000000.0",
            "buy": "999000.0",
            "high": "1000000.0",
            "low": "990000.0",
            "last": "1000000.0",
            "vol": "100.0",
            "timestamp": 1620000000000
        }
     */


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
        .get_no_query(
            "/btc_jpy/ticker",
            [BitbankOption::Default],
        )
        .await
        .expect("failed to get ticker");

    println!("BTC ticker:\n{:?}", ticker);
}
