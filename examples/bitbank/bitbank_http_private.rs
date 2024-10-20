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

  client.update_default_option(BitbankOption::HttpUrl(BitbankHttpUrl::Private));
  client.update_default_option(BitbankOption::HttpAuth(true));


  // get assets information
  let res: serde_json::Value = client.get_no_query(
    "/user/assets",
    [BitbankOption::Default]
  ).await.unwrap();
  println!("Assets: {:?}", res);


  // place limit order 0.001BTC @ 10 BTC/JPY

  let res2: serde_json::Value = client.post(
    "/user/spot/order",
    Some(&serde_json::json!({
      "pair":"btc_jpy",
      "amount":"0.001",
      "price":"10",
      "side":"buy",
      "type":"limit",
      "post_only":true,
    })),
    [BitbankOption::Default],
  ).await.unwrap();

  println!("Order result: {:?}", res2);

  // fetch active orders
  let res3: serde_json::Value = client.get(
    "/user/spot/active_orders",
    Some(&serde_json::json!({"pair":"btc_jpy"})),
    [BitbankOption::Default],
  ).await.unwrap();

  println!("Active orders: {:?}", res3);

}