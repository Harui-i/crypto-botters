use std::env;

use crypto_botters::{bitbank::{BitbankHttpUrl, BitbankOption}, Client};

#[tokio::main]
async fn main () {

  env_logger::builder()
    .filter_level(log::LevelFilter::Debug)
    .init();

  let key = env::var("BITBANK_API_KEY").expect("no API key found");
  let secret = env::var("BITBANK_API_SECRET").expect("no API secret found");

  let mut client = Client::new();
  client.update_default_option(BitbankOption::Key(key));
  client.update_default_option(BitbankOption::Secret(secret));


  // get assets information
  let res: serde_json::Value = client.get_no_query(
    "/user/assets",
    [BitbankOption::HttpAuth(true), BitbankOption::HttpUrl(BitbankHttpUrl::Private)]
  ).await.unwrap();
  println!("Assets: {:?}", res);

  // fetch active orders
  let res2: serde_json::Value = client.get(
    "/user/spot/active_orders",
    Some(&serde_json::json!({"pair":"btc_jpy"})),
    [BitbankOption::HttpAuth(true), BitbankOption::HttpUrl(BitbankHttpUrl::Private)],
  ).await.unwrap();

  println!("Active orders: {:?}", res2);

}