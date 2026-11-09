use crypto_botters::{
    binance::{BinanceOption, BinanceWebSocketUrl},
    Client,
};
use std::time::Duration;

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

    let connection = client
        .websocket(
            "/ws/btcusdt@trade",
            |message| println!("{}", message),
            [BinanceOption::WebSocketUrl(BinanceWebSocketUrl::Spot443)],
        )
        .await
        .expect("failed to connect websocket");
    // receive messages
    tokio::time::sleep(Duration::from_secs(1)).await;

    // manually reconnect
    connection.reconnect_state().request_reconnect();

    // receive messages. we should see no missing message during reconnection
    tokio::time::sleep(Duration::from_secs(3)).await;

    // close the connection
    drop(connection);

    // wait for the "close" message to be logged
    tokio::time::sleep(Duration::from_secs(1)).await;
}
