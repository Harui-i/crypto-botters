use crypto_botters::{bybit::BybitOption, Client};
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
    let key = env::var("BYBIT_API_KEY").expect("no API key found");
    let secret = env::var("BYBIT_API_SECRET").expect("no API secret found");
    let mut client = Client::new();
    client.update_default_option(BybitOption::Key(key));
    client.update_default_option(BybitOption::Secret(secret));

    let connection = client
        .websocket(
            "/v5/private",
            |message| println!("{}", message),
            [
                BybitOption::WebSocketTopics(vec!["wallet".to_owned()]),
                BybitOption::WebSocketAuth(true),
            ],
        )
        .await
        .expect("failed to connect websocket");
    // receive messages
    tokio::time::sleep(Duration::from_secs(300)).await;

    // close the connection
    drop(connection);

    // wait for the "close" message to be logged
    tokio::time::sleep(Duration::from_secs(1)).await;
}
