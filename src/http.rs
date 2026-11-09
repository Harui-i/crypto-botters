pub use bytes::Bytes;
pub use reqwest::{
    header::{self, HeaderMap},
    Method, Request, RequestBuilder, StatusCode,
};
use serde::Serialize;
use std::time::Duration;
use thiserror::Error;

/// The User Agent string
pub static USER_AGENT: &str = concat!("crypto-botters-bitbank/", env!("CARGO_PKG_VERSION"));

/// Client for communicating with APIs through HTTP/HTTPS.
///
/// When making a HTTP request or starting a websocket connection with this client,
/// a handler that implements [RequestHandler] is required.
#[derive(Debug, Clone)]
pub struct Client {
    client: reqwest::Client,
}

impl Client {
    /// Constructs a new `Client`.
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes an HTTP request with the given [RequestHandler] and returns the response.
    ///
    /// It is recommended to use methods like [get()][Self::get()] because this method takes many type parameters and parameters.
    ///
    /// The request is passed to `handler` before being sent, and the response is passed to `handler` before being returned.
    /// Note, that as stated in the docs for [RequestBuilder::query()], parameter `query` only accepts a **sequence of** key-value pairs.
    pub async fn request<Q, B, H>(
        &self,
        method: Method,
        url: &str,
        query: Option<&Q>,
        body: Option<B>,
        handler: &H,
    ) -> Result<H::Successful, RequestError<H::BuildError, H::Unsuccessful>>
    where
        Q: Serialize + ?Sized,
        H: RequestHandler<B>,
    {
        use tracing::Instrument;
        static NEXT_REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let request_id = NEXT_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let span = tracing::info_span!("http_request", request_id, method = %method, host = tracing::field::Empty);
        async move {
            let started = std::time::Instant::now();
            tracing::debug!(event = "http_request_started");
            let config = handler.request_config();
            config.verify();
            let url = config.url_prefix + url;
            if let Ok(parsed) = reqwest::Url::parse(&url) {
                if let Some(host) = parsed.host_str() {
                    tracing::Span::current().record("host", host);
                }
            }
            let mut count = 1;
            loop {
                // create RequestBuilder
                let mut request_builder = self
                    .client
                    .request(method.clone(), url.clone())
                    .timeout(config.timeout);
                if let Some(query) = query {
                    request_builder = request_builder.query(query);
                }
                let request = handler
                    .build_request(request_builder, &body, count)
                    .map_err(|error| {
                        tracing::warn!(
                            event = "http_request_failed",
                            stage = "build",
                            attempt = count,
                            elapsed_ms = started.elapsed().as_millis() as u64
                        );
                        RequestError::BuildRequestError(error)
                    })?;
                // send the request
                match self.client.execute(request).await {
                    Ok(mut response) => {
                        let status = response.status();
                        let headers = std::mem::take(response.headers_mut());
                        let body = response.bytes().await.map_err(|error| {
                            tracing::warn!(
                                event = "http_request_failed",
                                stage = "receive_body",
                                error_kind = error_kind(&error),
                                attempt = count,
                                elapsed_ms = started.elapsed().as_millis() as u64
                            );
                            RequestError::ReceiveResponse(error)
                        })?;
                        let result = handler
                            .handle_response(status, headers, body)
                            .map_err(RequestError::ResponseHandleError);
                        tracing::debug!(
                            event = "http_request_finished",
                            http_status = status.as_u16(),
                            handler_ok = result.is_ok(),
                            attempt = count,
                            elapsed_ms = started.elapsed().as_millis() as u64
                        );
                        return result;
                    }
                    Err(error) => {
                        if count >= config.max_try {
                            // max retry count
                            tracing::warn!(
                                event = "http_request_failed",
                                stage = "send",
                                error_kind = error_kind(&error),
                                attempt = count,
                                elapsed_ms = started.elapsed().as_millis() as u64
                            );
                            return Err(RequestError::SendRequest(error));
                        }
                        tracing::warn!(
                            event = "http_retry",
                            error_kind = error_kind(&error),
                            next_attempt = count + 1,
                            max_attempts = config.max_try,
                            wait_ms = config.retry_cooldown.as_millis() as u64
                        );
                        // else, continue
                        count += 1;
                        tokio::time::sleep(config.retry_cooldown).await;
                    }
                }
            }
        }
        .instrument(span)
        .await
    }

    /// Makes an GET request with the given [RequestHandler].
    ///
    /// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
    /// This method requires that `handler` can handle a request with a body of type `()`. The actual body passed will be `None`.
    ///
    /// For more information, see [request()][Self::request()].
    #[inline(always)]
    pub async fn get<Q, H>(
        &self,
        url: &str,
        query: Option<&Q>,
        handler: &H,
    ) -> Result<H::Successful, RequestError<H::BuildError, H::Unsuccessful>>
    where
        Q: Serialize + ?Sized,
        H: RequestHandler<()>,
    {
        self.request::<Q, (), H>(Method::GET, url, query, None, handler)
            .await
    }

    /// Makes an GET request with the given [RequestHandler], without queries.
    ///
    /// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
    /// This method requires that `handler` can handle a request with a body of type `()`. The actual body passed will be `None`.
    ///
    /// For more information, see [request()][Self::request()].
    #[inline(always)]
    pub async fn get_no_query<H>(
        &self,
        url: &str,
        handler: &H,
    ) -> Result<H::Successful, RequestError<H::BuildError, H::Unsuccessful>>
    where
        H: RequestHandler<()>,
    {
        self.request::<&[(&str, &str)], (), H>(Method::GET, url, None, None, handler)
            .await
    }

    /// Makes an POST request with the given [RequestHandler].
    ///
    /// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
    ///
    /// For more information, see [request()][Self::request()].
    #[inline(always)]
    pub async fn post<B, H>(
        &self,
        url: &str,
        body: Option<B>,
        handler: &H,
    ) -> Result<H::Successful, RequestError<H::BuildError, H::Unsuccessful>>
    where
        H: RequestHandler<B>,
    {
        self.request::<(), B, H>(Method::POST, url, None, body, handler)
            .await
    }

    /// Makes an POST request with the given [RequestHandler], without a body.
    ///
    /// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
    /// This method requires that `handler` can handle a request with a body of type `()`. The actual body passed will be `None`.
    ///
    /// For more information, see [request()][Self::request()].
    #[inline(always)]
    pub async fn post_no_body<H>(
        &self,
        url: &str,
        handler: &H,
    ) -> Result<H::Successful, RequestError<H::BuildError, H::Unsuccessful>>
    where
        H: RequestHandler<()>,
    {
        self.request::<(), (), H>(Method::POST, url, None, None, handler)
            .await
    }

    /// Makes an PUT request with the given [RequestHandler].
    ///
    /// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
    ///
    /// For more information, see [request()][Self::request()].
    #[inline(always)]
    pub async fn put<B, H>(
        &self,
        url: &str,
        body: Option<B>,
        handler: &H,
    ) -> Result<H::Successful, RequestError<H::BuildError, H::Unsuccessful>>
    where
        H: RequestHandler<B>,
    {
        self.request::<(), B, H>(Method::PUT, url, None, body, handler)
            .await
    }

    /// Makes an PUT request with the given [RequestHandler], without a body.
    ///
    /// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
    /// This method requires that `handler` can handle a request with a body of type `()`. The actual body passed will be `None`.
    ///
    /// For more information, see [request()][Self::request()].
    #[inline(always)]
    pub async fn put_no_body<H>(
        &self,
        url: &str,
        handler: &H,
    ) -> Result<H::Successful, RequestError<H::BuildError, H::Unsuccessful>>
    where
        H: RequestHandler<()>,
    {
        self.request::<(), (), H>(Method::PUT, url, None, None, handler)
            .await
    }

    /// Makes an DELETE request with the given [RequestHandler].
    ///
    /// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
    /// This method requires that `handler` can handle a request with a body of type `()`. The actual body passed will be `None`.
    ///
    /// For more information, see [request()][Self::request()].
    #[inline(always)]
    pub async fn delete<Q, H>(
        &self,
        url: &str,
        query: Option<&Q>,
        handler: &H,
    ) -> Result<H::Successful, RequestError<H::BuildError, H::Unsuccessful>>
    where
        Q: Serialize + ?Sized,
        H: RequestHandler<()>,
    {
        self.request::<Q, (), H>(Method::DELETE, url, query, None, handler)
            .await
    }

    /// Makes an DELETE request with the given [RequestHandler], without queries.
    ///
    /// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
    /// This method requires that `handler` can handle a request with a body of type `()`. The actual body passed will be `None`.
    ///
    /// For more information, see [request()][Self::request()].
    #[inline(always)]
    pub async fn delete_no_query<H>(
        &self,
        url: &str,
        handler: &H,
    ) -> Result<H::Successful, RequestError<H::BuildError, H::Unsuccessful>>
    where
        H: RequestHandler<()>,
    {
        self.request::<&[(&str, &str)], (), H>(Method::DELETE, url, None, None, handler)
            .await
    }
}

impl Default for Client {
    fn default() -> Self {
        let client = reqwest::ClientBuilder::new()
            .user_agent(USER_AGENT)
            .build()
            .unwrap(); // user agent should be valid
        Self { client }
    }
}

/// A `trait` which is used to process requests and responses for the [Client].
pub trait RequestHandler<B> {
    /// The type which is returned to the caller of [Client::request()] when the response was successful.
    type Successful;
    /// The type which is returned to the caller of [Client::request()] when the response was unsuccessful.
    type Unsuccessful;
    /// The type that represents an error occurred in [build_request()][Self::build_request()].
    type BuildError;

    /// Returns a [RequestConfig] that will be used to send a HTTP reqeust.
    fn request_config(&self) -> RequestConfig {
        RequestConfig::default()
    }

    /// Build a HTTP request to be sent.
    ///
    /// Implementors have to decide how to include the `request_body` into the `builder`. Implementors can
    /// also perform other operations (such as authorization) on the request.
    fn build_request(
        &self,
        builder: RequestBuilder,
        request_body: &Option<B>,
        attempt_count: u8,
    ) -> Result<Request, Self::BuildError>;

    /// Handle a HTTP response before it is returned to the caller of [Client::request()].
    ///
    /// You can verify, parse, etc... the response here before it is returned to the caller.
    ///
    /// # Examples
    /// ```
    /// # use bytes::Bytes;
    /// # use reqwest::{StatusCode, header::HeaderMap};
    /// # trait Ignore {
    /// fn handle_response(&self, status: StatusCode, _: HeaderMap, response_body: Bytes) -> Result<String, ()> {
    ///     if status.is_success() {
    ///         let body = std::str::from_utf8(&response_body).expect("body should be valid UTF-8").to_owned();
    ///         Ok(body)
    ///     } else {
    ///         Err(())
    ///     }
    /// }
    /// # }
    /// ```
    fn handle_response(
        &self,
        status: StatusCode,
        headers: HeaderMap,
        response_body: Bytes,
    ) -> Result<Self::Successful, Self::Unsuccessful>;
}

/// Configuration when sending a request using [Client].
///
/// Should be returned by [RequestHandler::request_config()].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RequestConfig {
    /// [Client] will retry sending a request if it failed to send. `max_try` can be used limit the number of attempts.
    ///
    /// Do not set this to `0` or [Client::request()] will **panic**. [Default]s to `1` (which means no retry).
    pub max_try: u8,
    /// Duration that should elapse after retrying sending a request.
    ///
    /// [Default]s to 500ms. See also: `max_try`.
    pub retry_cooldown: Duration,
    /// The timeout set when sending a request. [Default]s to 3s.
    ///
    /// It is possible for the [RequestHandler] to override this in [RequestHandler::build_request()].
    /// See also: [RequestBuilder::timeout()].
    pub timeout: Duration,
    /// The prefix which will be used for requests sent using this configuration. [Default]s to `""`.
    ///
    /// Example usage: `"https://example.com"`
    pub url_prefix: String,
}

impl RequestConfig {
    /// Constructs a new `RequestConfig` with its fields set to [default][RequestConfig::default()].
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    fn verify(&self) {
        assert_ne!(
            self.max_try, 0,
            "RequestConfig.max_try must not be equal to 0"
        );
    }
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            max_try: 1,
            retry_cooldown: Duration::from_millis(500),
            timeout: Duration::from_secs(3),
            url_prefix: String::new(),
        }
    }
}

/// An `enum` that represents errors that could be returned by [Client::request()]
///
/// Type parameter `R` is [RequestHandler::Unsuccessful].
#[derive(Error, Debug)]
pub enum RequestError<E, R> {
    /// An error which occurred while sending a HTTP request.
    #[error("failed to send reqeust")]
    SendRequest(#[source] reqwest::Error),
    /// An error which occurred while receiving a HTTP response.
    #[error("failed to receive response")]
    ReceiveResponse(#[source] reqwest::Error),
    /// Error occurred in [RequestHandler::build_request()].
    #[error("the handler failed to build a request")]
    BuildRequestError(E),
    /// An error which was returned by [RequestHandler].
    #[error("the response handler returned an error")]
    ResponseHandleError(R),
}

fn error_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_body() {
        "body"
    } else if error.is_decode() {
        "decode"
    } else if error.is_builder() {
        "build"
    } else {
        "transport"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing::instrument::WithSubscriber;

    #[derive(Clone)]
    struct Output(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct Handler {
        fail_build: bool,
    }
    impl RequestHandler<()> for Handler {
        type Successful = ();
        type Unsuccessful = ();
        type BuildError = &'static str;
        fn request_config(&self) -> RequestConfig {
            RequestConfig {
                max_try: 2,
                retry_cooldown: Duration::from_millis(1),
                ..Default::default()
            }
        }
        fn build_request(
            &self,
            builder: RequestBuilder,
            _: &Option<()>,
            _: u8,
        ) -> Result<Request, Self::BuildError> {
            if self.fail_build {
                Err("secret-build-error")
            } else {
                builder.build().map_err(|_| "build")
            }
        }
        fn handle_response(&self, _: StatusCode, _: HeaderMap, _: Bytes) -> Result<(), ()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn build_failure_logs_stage_without_secrets() {
        let output = Output(Arc::new(Mutex::new(Vec::new())));
        let writer = output.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish();
        let result = Client::new()
            .get_no_query(
                "http://user:secret-password@localhost/secret-path?key=secret-query",
                &Handler { fail_build: true },
            )
            .with_subscriber(subscriber)
            .await;
        assert!(matches!(result, Err(RequestError::BuildRequestError(_))));
        let log = String::from_utf8(output.0.lock().unwrap().clone()).unwrap();
        assert!(log.contains("http_request_failed"));
        assert!(log.contains("request_id"));
        assert!(log.contains("build"));
        assert!(!log.contains("secret-"));
    }

    #[tokio::test]
    async fn success_without_subscriber() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).unwrap() > 0);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .unwrap();
        });
        Client::new()
            .get_no_query(
                &format!("http://{address}/"),
                &Handler { fail_build: false },
            )
            .await
            .unwrap();
        server.join().unwrap();
    }
    async fn scripted_response(
        responses: Vec<&'static [u8]>,
    ) -> (Result<(), RequestError<&'static str, ()>>, String) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            for response in responses {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(std::time::Instant::now() < deadline);
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = [0; 4096];
                assert!(stream.read(&mut request).unwrap() > 0);
                stream.write_all(response).unwrap();
            }
        });
        let output = Output(Arc::new(Mutex::new(Vec::new())));
        let writer = output.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish();
        let result = Client::new()
            .get_no_query(
                &format!("http://{address}/secret-path?key=secret-query"),
                &Handler { fail_build: false },
            )
            .with_subscriber(subscriber)
            .await;
        server.join().unwrap();
        let log = String::from_utf8(output.0.lock().unwrap().clone()).unwrap();
        assert!(!log.contains("secret-"));
        (result, log)
    }

    #[tokio::test]
    async fn retry_stays_in_request_span() {
        let (result, log) = scripted_response(vec![
            b"",
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        ])
        .await;
        result.unwrap();
        assert!(log.contains("http_retry"));
        assert!(log.contains("next_attempt=2"));
        assert!(log.contains("handler_ok=true"));
        assert!(log.contains("request_id"));
    }

    #[tokio::test]
    async fn partial_body_is_receive_failure_without_retry() {
        let (result, log) = scripted_response(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nsecret-body",
        ])
        .await;
        assert!(matches!(result, Err(RequestError::ReceiveResponse(_))));
        assert!(log.contains("receive_body"));
        assert!(!log.contains("http_retry"));
    }
}
