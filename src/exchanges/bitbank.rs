//! A module for communicating with the [Bitbank API](https://github.com/bitbankinc/bitbank-api-docs/blob/master/README.md)

use std::{marker::PhantomData, time::SystemTime};

use crate::traits::*;
use generic_api_client::{http::*, websocket::*};
use header::HeaderValue;
use hmac::{Hmac, Mac};
use serde::{de::DeserializeOwned, Serialize};
use sha2::Sha256;

/// The type returned by [Client::request()].
pub type BitbankRequestResult<T> = Result<T, BitbankRequestError>;
pub type BitbankRequestError = RequestError<&'static str, BitbankHandleError>;

/// Options that can be set when creating handlers
pub enum BitbankOption {
    /// [Default] variant, does nothing
    Default,
    /// API key
    Key(String),
    /// API secret
    Secret(String),
    /// Base url for HTTP requests
    HttpUrl(BitbankHttpUrl),
    /// Whether [BitbankRequestHandler] should perform authentication
    HttpAuth(bool),
    /// [RequestConfig] used when sending requests.
    /// `url_prefix` will be overridden by [HttpUrl](Self::HttpUrl) unless `HttpUrl` is [BitbankHttpUrl::None].
    RequestConfig(RequestConfig),
    /// Base url for WebSocket connections.
    WebSocketUrl(BitbankWebSocketUrl),
    /// The channels to be subscribed by [WebSocketHandler].
    WebSocketChannels(Vec<String>),
    /// [WebSocketConfig] used for creating [WebSocketConnection]s.
    /// `url_prefix` will be overridden by [WebsocketUrl](Self::WebsocketUrl) unless `WebsocketUrl` is [BitbankWebSocketUrl::None].
    WebSocketConfig(WebSocketConfig),
}

/// A `struct` that represents a set of [BitbankOption]s.
#[derive(Clone, Debug)]
pub struct BitbankOptions {
    /// see [BitbankOption::Key]
    pub key: Option<String>,
    /// see [BitbankOption::Secret]
    pub secret: Option<String>,
    /// see [BitbankOption::HttpUrl]
    pub http_url: BitbankHttpUrl,
    /// see [BitbankOption::HttpAuth]
    pub http_auth: bool,
    /// see [BitbankOption::RequestConfig]
    pub request_config: RequestConfig,
    /// see [BitbankOption::WebsocketUrl]
    pub websocket_url: BitbankWebSocketUrl,
    /// see [BitbankOption::WebSocketChannels]
    pub websocket_channels: Vec<String>,
    /// see [BitbankOption::WebSocketConfig]
    pub websocket_config: WebSocketConfig,
}

/// A `enum` that represents the base url of the Bitbank HTTP API.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum BitbankHttpUrl {
    /// `https://api.bitbank.cc/v1`
    Private,
    /// `https://public.bitbank.cc/`
    Public,
    /// The url will not be modified by [BitbankRequestHandler]
    None,
}

/// A `enum` that represents the base url of the Bitbank WebSocket API.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[non_exhaustive]
pub enum BitbankWebSocketUrl {
    /// `wss://stream.bitbank.cc`
    Default,
    /// The url will not be modified by [BitbankWebSocketHandler]
    None,
}

#[derive(Debug)]
pub enum BitbankHandleError {
    ApiError(serde_json::Value),
    ReuqestLimitExceeded(serde_json::Value),
    ParseError,
}

/// A `struct` that implements [RequestHandler]
pub struct BitbankRequestHandler<'a, R: DeserializeOwned> {
    options: BitbankOptions,
    _phantom: PhantomData<&'a R>,
}

pub struct BitbankWebSocketHandler {
    message_handler: Box<dyn FnMut(serde_json::Value) -> () + Send>,
    options: BitbankOptions,
}

impl<'a, B, R> RequestHandler<B> for BitbankRequestHandler<'a, R>
where
    B: Serialize,
    R: DeserializeOwned,
{
    type Successful = R;
    type Unsuccessful = BitbankHandleError;
    type BuildError = &'static str;

    fn request_config(&self) -> RequestConfig {
        let mut config = self.options.request_config.clone();
        if self.options.http_url != BitbankHttpUrl::None {
            config.url_prefix = self.options.http_url.as_str().to_owned();
        }

        config
    }

    fn build_request(
        &self,
        mut builder: RequestBuilder,
        request_body: &Option<B>,
        _: u8,
    ) -> Result<Request, Self::BuildError> {
        if let Some(body) = request_body {
            let encoded = serde_json::to_string(&body).or(Err(
                "Could not serialize body as application/x-www-form-urlencoded",
            ))?;

            builder = builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(encoded);
        }

        // this gonna be mutable when self.options.http_auth is implemented
        let mut request = builder.build().or(Err("failed to build request"))?;

        if self.options.http_auth {
            // add authentication info to header
            // cf: https://github.com/bitbankinc/bitbank-api-docs/blob/master/rest-api.md
            // ACCESS-TIME-WINDOW method

            let mut path = request.url().path().to_owned();

            if let Some(query) = request.url().query() {
                path.push('?');
                path.push_str(query);
            }

            let body = request
                .body()
                .and_then(|body| body.as_bytes())
                .map(String::from_utf8_lossy)
                .unwrap_or_default();

            let access_request_time = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            let access_time_window = 1000;

            let sign_latter;
            
            // GET method
            if body == "" {
                sign_latter = path;
            }
            // POST method
            else {
                sign_latter = format!("{}", body);
            }


            let sign_content = format!(
                "{}{}{}",
                access_request_time, access_time_window, sign_latter
            );

            let secret = self
                .options
                .secret
                .as_deref()
                .ok_or("API secret not found")?;
            let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();

            hmac.update(sign_content.as_bytes());
            let signature = hex::encode(hmac.finalize().into_bytes());

            let key =
                HeaderValue::from_str(self.options.key.as_deref().ok_or("API key not found")?)
                    .or(Err("invalid character in API key"))?;

            let headers = request.headers_mut();
            headers.insert("ACCESS-KEY", key);
            headers.insert("ACCESS-REQUEST-TIME", HeaderValue::from(access_request_time));
            headers.insert("ACCESS-TIME-WINDOW", HeaderValue::from(access_time_window));
            headers.insert("ACCESS-SIGNATURE", HeaderValue::from_str(&signature).unwrap());
        }

        Ok(request)
    }

    fn handle_response(
        &self,
        status: StatusCode,
        _: HeaderMap,
        response_body: Bytes,
    ) -> Result<Self::Successful, Self::Unsuccessful> {
        if status.is_success() {
            let res = serde_json::from_slice::<R>(&response_body).map_err(|error| {
                log::debug!("Failed to parse response: {:?}", error);
                log::debug!(
                    "Response body: {:?}",
                    String::from_utf8_lossy(&response_body)
                );
                BitbankHandleError::ParseError
            });

            match res {
                Err(err) => {
                    Err(err)
                }

                Ok(res) => {
                    let res_val = serde_json::from_slice::<serde_json::Value>(&response_body).unwrap();
                    if res_val["success"].as_i64() == Some(0) {
                        // Errer code is written in res_val["code"]
                        // cf: https://github.com/bitbankinc/bitbank-api-docs/blob/master/errors.md
                        Err(BitbankHandleError::ApiError(res_val))
                    }
                    else {
                        Ok(res)
                    }
                }
            }

        } else {
            // error brace
            let error = match serde_json::from_slice(&response_body) {
                Ok(parsed_error) => {
                    log::debug!("API error: {:?}", parsed_error);
                    BitbankHandleError::ApiError(parsed_error)
                }

                Err(error) => {
                    log::debug!("Failed to parse error response due to an error: {}", error);
                    BitbankHandleError::ParseError
                }
            };

            Err(error)
        }
    }
}

impl WebSocketHandler for BitbankWebSocketHandler {
    fn websocket_config(&self) -> WebSocketConfig {
        // TODO
        let mut config = self.options.websocket_config.clone();

        if self.options.websocket_url != BitbankWebSocketUrl::None {
            config.url_prefix = self.options.websocket_url.as_str().to_owned();
        }

        config
    }

    fn handle_start(&mut self) -> Vec<WebSocketMessage> {
        // send a handshake packet
        let msg = "40".to_string();
        log::debug!("sending a handshake packet: {}", msg);
        vec![WebSocketMessage::Text(msg)]
    }

    // the first handshake response : `0{"sid":"vOPoe2650oydu3DWHAEg","upgrades":[],"pingInterval":25000,"pingTimeout":20000,"maxPayload":1000000}`
    //
    fn handle_message(&mut self, message: WebSocketMessage) -> Vec<WebSocketMessage> {
        match message {
            WebSocketMessage::Text(message) => {
                // cf: https://socket.io/docs/v4/engine-io-protocol/
                let engine_packet_type = message.chars().nth(0).unwrap();

                match engine_packet_type {
                    // open
                    '0' => {
                        match serde_json::from_str::<serde_json::Value>(&message[1..]) {
                            Ok(message) => {
                                log::debug!("Engine.io's OPEN packet: {:?}", message);
                            }
                            Err(_) => {
                                log::debug!("Invalid JSON message received, processing Engine.io's OPEN packet: {}", message);
                            }
                        };
                    }

                    // close
                    '1' => {
                        log::debug!("Engine.io's CLOSE packet: {}", message);
                    }

                    // ping
                    '2' => {
                        let res_pong_se: Vec<WebSocketMessage> =
                            vec![WebSocketMessage::Text("3".to_string())];
                        log::debug!("got a ping packet: {}", message);
                        log::debug!("sending pong packet: 3");
                        return res_pong_se;
                    }

                    // message
                    '4' => {
                        // socket.io packet
                        // cf: https://socket.io/docs/v4/socket-io-protocol/
                        let socket_packet_type = message.chars().nth(1).unwrap();

                        match socket_packet_type {
                            // CONNECT
                            '0' => {
                                match serde_json::from_str::<serde_json::Value>(&message[2..]) {
                                    Ok(message) => {
                                        log::debug!(
                                            "Handshake packet received. Socket.io packet: {:?}",
                                            message
                                        );

                                        // join process here:
                                        // join rooms
                                        let join_messages: Vec<WebSocketMessage> = self
                                            .options
                                            .websocket_channels
                                            .clone()
                                            .into_iter()
                                            .map(|channel| {
                                                let msg =
                                                    format!("42[\"join-room\", \"{}\"]", channel);
                                                log::debug!("sending join message: {}", msg);
                                                WebSocketMessage::Text(msg)
                                            })
                                            .collect();

                                        return join_messages;
                                    }
                                    Err(_) => {
                                        log::debug!("Invalid JSON message received, processing Socket.io's CONNECT packet: {}", message);
                                    }
                                };
                            }

                            // EVENT
                            '2' => {
                                match serde_json::from_str(&message[2..]) {
                                    Ok(message) => (self.message_handler)(message),
                                    Err(_) => {
                                        log::debug!("Invalid JSON message received, processing Socket.io's EVENT packet: {}", message);
                                    }
                                };
                            }
                            _ => {
                                log::debug!(
                                    "Invalid socket.io packet received: {}",
                                    socket_packet_type
                                );
                            }
                        }
                    }
                    _ => {
                        log::debug!("Invalid Engine.io's packet received: {}", message);
                    }
                }
            }

            WebSocketMessage::Binary(_) => {
                assert!(false);
                log::debug!("Binary message received")
            }
            WebSocketMessage::Ping(_) | WebSocketMessage::Pong(_) => {
                assert!(false);
                ()
            }
        }

        vec![]
    }

    fn handle_close(&mut self, reconnect: bool) {
        log::debug!(
            "Bitbank WebSocket connection closed; reconnect: {}",
            reconnect
        );
    }
}

impl BitbankHttpUrl {
    /// The base URL that this variant represents.
    #[inline(always)]
    fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "https://public.bitbank.cc",
            Self::Private => "https://api.bitbank.cc/v1",
            Self::None => "",
        }
    }
}

impl BitbankWebSocketUrl {
    /// The base URL that this variant represents.
    #[inline(always)]
    fn as_str(&self) -> &'static str {
        match self {
            // Since bitbank's stream API is implemented with socket.io, it becomes complicated without using socket.io library.
            Self::Default => "wss://stream.bitbank.cc/socket.io/?EIO=4&transport=websocket",
            Self::None => "",
        }
    }
}

impl HandlerOptions for BitbankOptions {
    type OptionItem = BitbankOption;

    fn update(&mut self, option: Self::OptionItem) {
        match option {
            BitbankOption::Default => (),
            BitbankOption::Key(v) => self.key = Some(v),
            BitbankOption::Secret(v) => self.secret = Some(v),
            BitbankOption::HttpUrl(v) => self.http_url = v,
            BitbankOption::HttpAuth(v) => self.http_auth = v,
            BitbankOption::RequestConfig(v) => self.request_config = v,
            BitbankOption::WebSocketUrl(v) => self.websocket_url = v,
            BitbankOption::WebSocketChannels(v) => self.websocket_channels = v,
            BitbankOption::WebSocketConfig(v) => self.websocket_config = v,
        }
    }
}

impl Default for BitbankOptions {
    fn default() -> Self {
        let mut websocket_config = WebSocketConfig::new();
        websocket_config.ignore_duplicate_during_reconnection = true;

        Self {
            key: None,
            secret: None,
            http_url: BitbankHttpUrl::None,
            http_auth: false,
            request_config: RequestConfig::default(),
            websocket_url: BitbankWebSocketUrl::Default,
            websocket_channels: vec![],
            websocket_config: WebSocketConfig::default(),
        }
    }
}

impl<'a, R, B> HttpOption<'a, R, B> for BitbankOption
where
    R: DeserializeOwned + 'a,
    B: Serialize,
{
    type RequestHandler = BitbankRequestHandler<'a, R>;

    #[inline(always)]
    fn request_handler(options: Self::Options) -> Self::RequestHandler {
        BitbankRequestHandler::<'a, R> {
            options,
            _phantom: PhantomData,
        }
    }
}

impl<H: FnMut(serde_json::Value) + Send + 'static> WebSocketOption<H> for BitbankOption {
    type WebSocketHandler = BitbankWebSocketHandler;

    #[inline(always)]
    fn websocket_handler(handler: H, options: Self::Options) -> Self::WebSocketHandler {
        BitbankWebSocketHandler {
            message_handler: Box::new(handler),
            options,
        }
    }
}

impl HandlerOption for BitbankOption {
    type Options = BitbankOptions;
}

impl Default for BitbankOption {
    fn default() -> Self {
        Self::Default
    }
}
