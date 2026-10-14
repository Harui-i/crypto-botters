# Examples

Examples for `crypto-botters-bitbank`, a fork of crypto-botters.
Enable the exchange feature when running an example:

```sh
cargo run --example bitbank_http_public --features bitbank
cargo run --example bitbank_http_private --features bitbank
```

bitbankのprivate RESTサンプルは資産・注文一覧の参照のみです。
`BITBANK_API_KEY`と`BITBANK_API_SECRET`を環境変数に設定してください。
注文の送信やレート制限を意図的に起こす処理は含みません。
他の取引所のprivateサンプルは実行前に内容を確認してください。
