use crypto_botters::{bitbank::BitbankOption, Client};
use log::LevelFilter;
use serde::Deserialize;
use rust_decimal::prelude::*;
use std::sync::{Arc, Mutex};
use std::fmt;
use std::time::Duration;

use std::collections::BTreeMap;

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct DepthDiff {
    a: Vec<serde_json::Value>, // ask, amount
    b: Vec<serde_json::Value>, // bid, amount
    ao: Option<String>, // optional. The quantity of asks over the highest price of asks orders. If there is no change in quantity, it will not be included in the message.
    bu: Option<String>, // optional. The quantity of bids under the lowest price of bids orders. If there is no change in quantity, it will not be included in the message.
    au: Option<String>, // optional. The quantity of asks under the lowest price of bids orders. If there is no change in quantity, it will not be included in the message.
    bo: Option<String>, // optional. The quantity of bids over the highest price of asks orders. If there is no change in quantity, it will not be included in the message.
    am: Option<String>, // optional. The quantity of market sell orders. If there is no change in quantity, it will not be included in the message.
    bm: Option<String>, // optional. The quantity of market buy orders. If there is no change in quantity, it will not be included in the message.

    t: i64,    // unixtime in milliseconds
    s: String, // sequence id. increasing-order but not necessarily consecutive
}

#[allow(dead_code)]
#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct DepthWhole {
    asks: Vec<serde_json::Value>,
    bids: Vec<serde_json::Value>,
    asks_over: String, // asks sum s.t. its price is higher than asks_highest value.
    // Without Circut Breaker, 200 offers from best-bid are sent via websocket.
    // So, asks_over is the sum of the rest of the offers.
    bids_under: String, // bids sum s.t. its price is lower than bids_lowest value.

    // these four values are 0 in non-CB mode.
    asks_under: String, // asks sum s.t. its price is lower than bids_lowest. (so low price)
    bids_over: String,  // bids sum s.t. its price is higher than asks_highest. (so high price)
    ask_market: String, // the quantity of market sell orders. Usually "0" in NORMAL mode.
    bid_market: String, // the quantity of market buy orders. Usually "0" in NORMAL mode.

    timestamp: i64,
    sequenceId: String,
}

struct DepthData {
    diff_buffer : BTreeMap<String, DepthDiff>,
    asks : BTreeMap<String, Decimal>, // price, amount
    bids : BTreeMap<String, Decimal>,

    is_complete : bool,
}

impl fmt::Display for DepthData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        assert!(self.is_complete);

        write!(f, "\n")?; 

        for (price, amount) in self.asks.iter().take(20).rev() {
            write!(f, "{}\t{:.4}\n", price, amount)?;
        }
        write!(f, "asks\n")?;
        write!(f, "mid\n")?;

        write!(f, "bids\n")?;
        for (price, amount) in self.bids.iter().rev().take(20) {
            write!(f, "{}\t{:.4}\n", price, amount)?;
        }


        Ok(())
    }
}

impl DepthData {
    fn new() -> Self {
        DepthData {
            diff_buffer : BTreeMap::new(),
            asks : BTreeMap::new(),
            bids : BTreeMap::new(),
            is_complete : false,
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn insert_diff(&mut self, diff: &DepthDiff) {
        for ask in &diff.a {
            let price = ask[0].as_str().unwrap();
            let amount : Decimal = ask[1].as_str().unwrap().parse().unwrap();

            if amount == Decimal::zero() {
                if self.asks.contains_key(price) {
                    self.asks.remove(price);
                }
            }

            else {
                self.asks.insert(price.to_string(), amount);
            }
        }

        for bid in &diff.b {
            let price = bid[0].as_str().unwrap();
            let amount : Decimal = bid[1].as_str().unwrap().parse().unwrap();

            if amount == Decimal::zero() {
                if self.bids.contains_key(price) {
                    self.bids.remove(price);
                }
            }

            else {
                self.bids.insert(price.to_string(), amount);
            }
        }

    }

    fn update_whole(&mut self, whole : DepthWhole) {
        let seq = whole.sequenceId.clone();


        let keys_to_remove : Vec<String> = self.diff_buffer
            .iter()
            .filter(|(key, _)| key < &&seq)
            .map(|(key, _)| key.clone())
            .collect();

        for key in keys_to_remove {
            self.diff_buffer.remove(&key);
        }
        
        self.asks.clear();
        self.bids.clear();

        for ask in whole.asks {
            let price = ask[0].as_str().unwrap();
            let amount : Decimal = ask[1].as_str().unwrap().parse().unwrap();

            assert_ne!(amount, Decimal::zero());
            self.asks.insert(price.to_string(), amount);
        }

        for bid in whole.bids {
            let price = bid[0].as_str().unwrap();
            let amount : Decimal = bid[1].as_str().unwrap().parse().unwrap();

            assert_ne!(amount, Decimal::zero());
            self.bids.insert(price.to_string(), amount);
        }


        self.process_diff_buffer();
        self.is_complete = true;
    }


    fn process_diff_buffer(&mut self) {

        for depth_diff in self.diff_buffer.values() {
            for ask in &depth_diff.a {
                let price = ask[0].as_str().unwrap();
                let amount : Decimal = ask[1].as_str().unwrap().parse().unwrap();

                if amount == Decimal::zero() {
                    self.asks.remove(price);
                }
                else {
                    self.asks.insert(price.to_string(), amount);
                }

            }

            for bid in &depth_diff.b {
                let price = bid[0].as_str().unwrap();
                let amount : Decimal = bid[1].as_str().unwrap().parse().unwrap();

                if amount == Decimal::zero() {
                    self.bids.remove(price);
                } 
                else {
                    self.bids.insert(price.to_string(), amount);
                }
            }
        }


        self.diff_buffer.clear();

        assert!(self.diff_buffer.is_empty());
    }
}


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

    let  btc_board = Arc::<Mutex::<DepthData>>::new(Mutex::new(DepthData::new()));
    let btc_board_clone = Arc::clone(&btc_board);

    let closure = move |message: serde_json::Value| {
        let sio_message: SocketioMessageData = serde_json::from_value(message[1].clone())
            .expect("failed to parse SocketioMessageData data");

        let mut board = btc_board_clone.lock().unwrap();

        if sio_message.room_name.starts_with("depth_diff") {
            log::debug!("diff");

            let depth_data: DepthDiff = serde_json::from_value(sio_message.message.data)
                .expect("failed to parse (diff) depth data");

            board.insert_diff(&depth_data);

            log::debug!("{:?}", depth_data);


        } else if sio_message.room_name.starts_with("depth_whole") {
            log::debug!("whole");
            let depth_data: DepthWhole = serde_json::from_value(sio_message.message.data)
                .expect("failed to parse (whole) depth data");
            log::debug!("{:?}", depth_data);
            //log::debug!("{:?}", depth_data);
            board.update_whole(depth_data);

        } else {
            log::debug!("unknown room name: {}", sio_message.room_name);
        }

        if sio_message.room_name.starts_with("depth") && board.is_complete() {
            //log::debug!("{}", board);
        }

    };

    log::debug!("start websocket connection");

    let connection = client
        .websocket(
            "",
            closure,
            [BitbankOption::WebSocketChannels(vec![
                "depth_diff_btc_jpy".to_owned(),
                "depth_whole_btc_jpy".to_owned(),
            ])],
        )
        .await
        .expect("failed to connect websocket");

    for _ in 0..100 {
        tokio::time::sleep(Duration::from_secs(30)).await;
    }

    drop(connection);
}
