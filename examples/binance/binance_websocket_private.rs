use crypto_botters::{
    binance::{BinanceAuth, BinanceHttpUrl, BinanceOption, BinanceWebSocketUrl},
    Client,
};
use std::{env, time::Duration};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,crypto_botters=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let key = env::var("BINANCE_API_KEY").expect("no API key found");
    let secret = env::var("BINANCE_API_SECRET").expect("no API secret found");
    let mut client = Client::new();
    client.update_default_option(BinanceOption::Key(key));
    client.update_default_option(BinanceOption::Secret(secret));

    let key: serde_json::Value = client
        .post(
            "/sapi/v1/userDataStream/isolated",
            Some(&[("symbol", "BTCUSDT")]),
            [
                BinanceOption::HttpAuth(BinanceAuth::Key),
                BinanceOption::HttpUrl(BinanceHttpUrl::Spot),
            ],
        )
        .await
        .expect("failed to get listen key");

    let _connection = client
        .websocket(
            &format!("/ws/{}", key["listenKey"].as_str().unwrap()),
            |message| println!("{}", message),
            [BinanceOption::WebSocketUrl(BinanceWebSocketUrl::Spot9443)],
        )
        .await
        .expect("failed to connect websocket");

    // receive messages
    tokio::time::sleep(Duration::from_secs(60)).await;
}
