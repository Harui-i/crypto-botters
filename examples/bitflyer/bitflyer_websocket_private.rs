use crypto_botters::{bitflyer::BitFlyerOption, Client};
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
    let key = env::var("BITFLYER_API_KEY").expect("no API key found");
    let secret = env::var("BITFLYER_API_SECRET").expect("no API secret found");
    let mut client = Client::new();
    client.update_default_option(BitFlyerOption::Key(key));
    client.update_default_option(BitFlyerOption::Secret(secret));

    let connection = client
        .websocket(
            "/json-rpc",
            |message| println!("{:?}", message),
            [
                BitFlyerOption::WebSocketChannels(vec!["child_order_events".to_owned()]),
                BitFlyerOption::WebSocketAuth(true),
            ],
        )
        .await
        .expect("failed to connect websocket");

    // receive messages
    tokio::time::sleep(Duration::from_secs(10)).await;

    // reconnect
    connection.reconnect_state().request_reconnect();

    // we should still see private channel messages
    tokio::time::sleep(Duration::from_secs(10)).await;
}
