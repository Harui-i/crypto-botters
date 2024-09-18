use crypto_botters::{bitbank::BitbankOption, Client};
use log::LevelFilter;
use rust_decimal::prelude::*;
use serde::Deserialize;
use std::time::Duration;

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(LevelFilter::Debug)
        .init();
    let client = Client::new();

    #[allow(dead_code)]
    #[derive(Deserialize, Debug)]
    struct SocketioMessageData {
        message: WebSocketMessage,
        room_name: String,
    }

    #[allow(dead_code)]
    #[derive(Deserialize, Debug)]
    struct WebSocketMessage {
        data: serde_json::Value,
    }

    #[allow(dead_code)]
    #[derive(Deserialize, Debug)]
    struct TransactionMessage {
        transactions: Vec<TransactionData>,
    }

    #[allow(dead_code)]
    #[derive(Deserialize, Debug)]
    struct TransactionData {
        #[serde(with = "rust_decimal::serde::float")]
        amount: Decimal,
        executed_at: i64,
        #[serde(with = "rust_decimal::serde::float")]
        price: Decimal,
        side: String,
        transaction_id: i64,
    }

    let closure = |message: serde_json::Value| {
        //log::debug!("plane message: {:?}", message);
        let message_data: SocketioMessageData =
            serde_json::from_value(message[1].clone()).expect("failed to parse message data");
        let room_name = message_data.room_name;

        if room_name.starts_with("transactions") {
            let transaction_msg: TransactionMessage =
                serde_json::from_value(message_data.message.data.clone())
                    .expect("failed to parse transaction message");

            for transaction in transaction_msg.transactions {
                log::debug!("{:?}", transaction);
            }
        } else {
            let websocket_message = message_data.message.data;
            log::debug!("message from `{}`: {:?}", room_name, websocket_message);
        }
    };

    let connection = client
        .websocket(
            "",
            closure,
            [BitbankOption::WebSocketChannels(vec![
                "transactions_btc_jpy".to_owned(),
                "transactions_eth_jpy".to_owned(),
                "transactions_xrp_jpy".to_owned(),
                //"depth_diff_btc_jpy".to_owned(),
                //"depth_whole_btc_jpy".to_owned(),
            ])],
        )
        .await
        .expect("failed to connect websocket");
    // receive messages
    tokio::time::sleep(Duration::from_secs(5)).await;

    // manually reconnect
    connection.reconnect_state().request_reconnect();

    // receive messages. we should see no missing message during reconnection
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_secs(15)).await;
    }
    
    // close the connection
    drop(connection);

    // wait for the "close" message to be logged
    tokio::time::sleep(Duration::from_secs(1)).await;
}
