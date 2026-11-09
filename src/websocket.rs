use futures_util::{
    sink::SinkExt,
    stream::{SplitSink, StreamExt},
};
use parking_lot::Mutex as SyncMutex;
use std::{
    collections::hash_map::{Entry, HashMap},
    mem,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    net::TcpStream,
    sync::{mpsc as tokio_mpsc, Mutex as AsyncMutex, Notify},
    task::JoinHandle,
    time::{timeout, MissedTickBehavior},
};
use tokio_tungstenite::{tungstenite, MaybeTlsStream};
use tracing::{instrument::WithSubscriber, Instrument};
pub use tungstenite::Error as TungsteniteError;

type WebSocketStream = tokio_tungstenite::WebSocketStream<MaybeTlsStream<TcpStream>>;
type WebSocketSplitSink = SplitSink<WebSocketStream, tungstenite::Message>;

/// A `struct` that holds a websocket connection.
///
/// Dropping this `struct` terminates the connection.
///
/// # Reconnecting
/// `WebSocketConnection` automatically reconnects when an [TungsteniteError] occurs.
/// Note, that during reconnection, it is **possible** that the [WebSocketHandler] receives multiple identical messages
/// even though the message was sent only once by the server, or receives only one message even though
/// multiple identical messages were sent by the server, because there could be a time difference in the new connection and
/// the old connection.
///
/// You can use the [reconnect_state()][Self::reconnect_state()] method to check if the connection is under
/// a reconnection, or manually request a reconnection.
#[derive(Debug)]
#[must_use = "dropping WebSocketConnection closes the connection"]
pub struct WebSocketConnection<H: WebSocketHandler> {
    task_reconnect: JoinHandle<()>,
    sink: Arc<AsyncMutex<WebSocketSplitSink>>,
    inner: Arc<ConnectionInner<H>>,
    reconnect_state: ReconnectState,
}

// Two ways connections end:
// - User drops WebSocketConnection
//     1. feed_handler receives a message and closes the connection, then terminates
//     2. start_connection notices that the connection is closed, and attempts to notify feed_handler, then terminates
// - Reconnection
//     This happens when:
//     - the user requests so
//     - message timeout
//     - the server closes the connection
//     - some kind of error occurs while receiving the message
//
//     1. task_reconnect starts a new connection
//     2. task_reconnect closes the old connection
//     3. start_connection (old) notices that the connection is closed, and notifies feed_handler, then terminates
//     4. feed_handler receives the message, but ignores it because it is from the old connection
#[derive(Debug)]
struct ConnectionInner<H: WebSocketHandler> {
    url: String,
    span: tracing::Span,
    handler: Arc<SyncMutex<H>>,
    message_tx: tokio_mpsc::UnboundedSender<(bool, FeederMessage)>,
    next_connection_id: AtomicBool,
}

enum FeederMessage {
    Message(tungstenite::Result<tungstenite::Message>),
    ConnectionClosed,
    DropConnectionRequest,
}

// Preserve the existing transport error type.
#[allow(clippy::result_large_err)]
impl<H: WebSocketHandler> WebSocketConnection<H> {
    /// Starts a new `WebSocketConnection` to the given url using the given [handler][WebSocketHandler].
    pub async fn new(url: &str, handler: H) -> Result<Self, TungsteniteError> {
        static NEXT_CONNECTION_ID: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(1);
        let connection_id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
        let span = tracing::info_span!("websocket", connection_id, host = tracing::field::Empty);
        let config = handler.websocket_config();
        let handler = Arc::new(SyncMutex::new(handler));
        let url = config.url_prefix.clone() + url;
        if let Ok(parsed) = reqwest::Url::parse(&url) {
            if let Some(host) = parsed.host_str() {
                span.record("host", host);
            }
        }

        let (message_tx, message_rx) = tokio_mpsc::unbounded_channel();
        let reconnect_manager = ReconnectState::new();

        let connection = Arc::new(ConnectionInner {
            url,
            span: span.clone(),
            handler: Arc::clone(&handler),
            message_tx,
            next_connection_id: AtomicBool::new(false),
        });

        async fn feed_handler(
            connection: Arc<ConnectionInner<impl WebSocketHandler>>,
            mut message_rx: tokio_mpsc::UnboundedReceiver<(bool, FeederMessage)>,
            reconnect_manager: ReconnectState,
            config: WebSocketConfig,
            sink: Arc<AsyncMutex<WebSocketSplitSink>>,
        ) {
            let mut messages: HashMap<WebSocketMessage, isize> = HashMap::new();

            let timeout_duration = if config.message_timeout.is_zero() {
                Duration::MAX
            } else {
                config.message_timeout
            };

            loop {
                match timeout(timeout_duration, message_rx.recv()).await {
                    // message successfully received
                    Ok(Some((id, FeederMessage::Message(Ok(message))))) => {
                        // message successfully received
                        if let Some(message) = WebSocketMessage::from_message(message) {
                            if reconnect_manager.is_reconnecting() {
                                // reconnecting
                                let id_sign: isize = if id { 1 } else { -1 };
                                let entry = messages.entry(message.clone());
                                match entry {
                                    Entry::Occupied(mut occupied) => {
                                        if config.ignore_duplicate_during_reconnection {
                                            tracing::trace!(event = "ws_duplicate_dropped");
                                            continue;
                                        }

                                        *occupied.get_mut() += id_sign;
                                        if id_sign != occupied.get().signum() {
                                            // same message which comes from different connections, so we assume it's a duplicate.
                                            tracing::trace!(event = "ws_duplicate_dropped");
                                            continue;
                                        }
                                        // comes from the same connection, which means the message was sent twice.
                                    }
                                    Entry::Vacant(vacant) => {
                                        // new message
                                        vacant.insert(id_sign);
                                    }
                                }
                            } else {
                                messages.clear();
                            }
                            let messages = connection.handler.lock().handle_message(message);
                            let mut sink_lock = sink.lock().await;
                            for message in messages {
                                if let Err(_error) = sink_lock.send(message.into_message()).await {
                                    tracing::warn!(
                                        event = "ws_send_failed",
                                        error_kind = error_kind(&_error)
                                    );
                                };
                            }
                            if let Err(_error) = sink_lock.flush().await {
                                tracing::warn!(
                                    event = "ws_transport_failed",
                                    error_kind = error_kind(&_error)
                                );
                            }
                        }
                    }
                    // failed to receive message
                    Ok(Some((_, FeederMessage::Message(Err(_error))))) => {
                        tracing::warn!(
                            event = "ws_transport_failed",
                            error_kind = error_kind(&_error)
                        );
                        if reconnect_manager.request_reconnect() {
                            tracing::warn!(
                                event = "ws_reconnect_requested",
                                reason = "receive_error"
                            );
                        }
                    }
                    // timeout
                    Err(_) => {
                        tracing::debug!("WebSocket message timeout");
                        if reconnect_manager.request_reconnect() {
                            tracing::warn!(
                                event = "ws_reconnect_requested",
                                reason = "message_timeout"
                            );
                        }
                    }
                    // connection was closed
                    Ok(Some((id, FeederMessage::ConnectionClosed))) => {
                        let current_id = !connection.next_connection_id.load(Ordering::SeqCst);
                        if id != current_id {
                            // old connection, ignore
                            continue;
                        }
                        tracing::debug!("WebSocket connection closed by server");
                        if reconnect_manager.request_reconnect() {
                            tracing::warn!(
                                event = "ws_reconnect_requested",
                                reason = "server_closed"
                            );
                        }
                    }
                    // the connection is no longer needed because WebSocketConnection was dropped
                    Ok(Some((_, FeederMessage::DropConnectionRequest))) => {
                        if let Err(_error) = sink.lock().await.close().await {
                            tracing::warn!(
                                event = "ws_transport_failed",
                                error_kind = error_kind(&_error)
                            );
                        }
                        break;
                    }
                    // message_tx has been dropped, which should never happen because it's always accessible by connection.message_tx.
                    Ok(None) => unreachable!("message_rx should never be closed"),
                }
            }
            tracing::info!(event = "ws_closed");
            connection.handler.lock().handle_close(false);
        }

        async fn reconnect<H: WebSocketHandler>(
            interval: Duration,
            cooldown: Duration,
            connection: Arc<ConnectionInner<H>>,
            sink: Arc<AsyncMutex<WebSocketSplitSink>>,
            reconnect_manager: ReconnectState,
            no_duplicate: bool,
            wait: Duration,
        ) {
            let mut attempt = 0u64;
            let mut generation = 0u64;
            let mut started = std::time::Instant::now();
            let mut cooldown = tokio::time::interval(cooldown);
            cooldown.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                let timer = if interval.is_zero() {
                    // never completes
                    tokio::time::sleep(Duration::MAX)
                } else {
                    tokio::time::sleep(interval)
                };
                tokio::select! {
                    _ = reconnect_manager.inner.reconnect_notify.notified() => { tracing::debug!(event = "ws_reconnect_trigger", reason = "requested"); },
                    _ = timer => { tracing::info!(event = "ws_reconnect_trigger", reason = "scheduled_refresh"); },
                }
                tracing::debug!("Reconnection requested");
                cooldown.tick().await;
                reconnect_manager
                    .inner
                    .reconnecting
                    .store(true, Ordering::SeqCst);

                // reconnect_notify might have been notified while waiting the cooldown,
                // so we consume any existing permits on reconnect_notify
                reconnect_manager.inner.reconnect_notify.notify_one();
                // this completes immediately because we just added a permit
                reconnect_manager.inner.reconnect_notify.notified().await;

                if attempt == 0 {
                    started = std::time::Instant::now();
                }
                attempt += 1;
                tracing::info!(event = "ws_reconnect_started", attempt);
                if no_duplicate {
                    tokio::time::sleep(wait).await;
                }

                // start a new connection
                match WebSocketConnection::<H>::start_connection(Arc::clone(&connection)).await {
                    Ok(new_sink) => {
                        // replace the sink with the new one
                        let mut old_sink = mem::replace(&mut *sink.lock().await, new_sink);
                        tracing::debug!("New connection established");

                        if no_duplicate {
                            tokio::time::sleep(wait).await;
                        }

                        if let Err(_error) = old_sink.close().await {
                            tracing::debug!(event = "ws_old_connection_close_failed");
                        }
                        connection.handler.lock().handle_close(true);
                        generation += 1;
                        tracing::info!(
                            event = "ws_reconnect_succeeded",
                            generation,
                            attempt,
                            elapsed_ms = started.elapsed().as_millis() as u64
                        );
                        attempt = 0;
                    }
                    Err(_error) => {
                        // try reconnecting again
                        tracing::warn!(
                            event = "ws_reconnect_failed",
                            error_kind = error_kind(&_error),
                            attempt,
                            elapsed_ms = started.elapsed().as_millis() as u64
                        );
                        reconnect_manager.inner.reconnect_notify.notify_one();
                    }
                }

                if no_duplicate {
                    tokio::time::sleep(wait).await;
                }

                reconnect_manager
                    .inner
                    .reconnecting
                    .store(false, Ordering::SeqCst);
            }
        }

        let sink_inner = Self::start_connection(Arc::clone(&connection))
            .instrument(span.clone())
            .await?;
        let sink = Arc::new(AsyncMutex::new(sink_inner));

        tokio::spawn(
            feed_handler(
                Arc::clone(&connection),
                message_rx,
                reconnect_manager.clone(),
                config.clone(),
                Arc::clone(&sink),
            )
            .instrument(span.clone())
            .with_current_subscriber(),
        );

        let task_reconnect = tokio::spawn(
            reconnect(
                config.refresh_after,
                config.connect_cooldown,
                Arc::clone(&connection),
                Arc::clone(&sink),
                reconnect_manager.clone(),
                config.ignore_duplicate_during_reconnection,
                config.reconnection_wait,
            )
            .instrument(span.clone())
            .with_current_subscriber(),
        );

        Ok(Self {
            task_reconnect,
            sink,
            inner: connection,
            reconnect_state: reconnect_manager,
        })
    }

    async fn start_connection(
        connection: Arc<ConnectionInner<impl WebSocketHandler>>,
    ) -> Result<WebSocketSplitSink, TungsteniteError> {
        tracing::info!(event = "ws_connect_started");
        let (websocket_stream, _) = tokio_tungstenite::connect_async(connection.url.clone())
            .await
            .inspect_err(|_error| {
                tracing::warn!(event = "ws_connect_failed", error_kind = error_kind(_error));
            })?;
        tracing::info!(event = "ws_connected");
        let (mut sink, mut stream) = websocket_stream.split();

        let messages = connection.handler.lock().handle_start();
        for message in messages {
            sink.send(message.into_message())
                .await
                .inspect_err(|error| {
                    tracing::warn!(
                        event = "ws_initial_send_failed",
                        error_kind = error_kind(error)
                    );
                })?;
        }
        sink.flush().await.inspect_err(|error| {
            tracing::warn!(
                event = "ws_initial_flush_failed",
                error_kind = error_kind(error)
            );
        })?;

        // fetch_not is unstable so we use fetch_xor
        let id = connection
            .next_connection_id
            .fetch_xor(true, Ordering::SeqCst);

        // pass messages to task_feed_handler
        let span = connection.span.clone();
        tokio::spawn(
            async move {
                while let Some(message) = stream.next().await {
                    // send the received message to the task running feed_handler
                    if connection
                        .message_tx
                        .send((id, FeederMessage::Message(message)))
                        .is_err()
                    {
                        // the channel is closed. we can't disconnect because we don't have the sink
                        tracing::debug!("WebSocket message receiver is closed; abandon connection");
                        return;
                    }
                }
                // the underlying WebSocket connection was closed

                drop(
                    connection
                        .message_tx
                        .send((id, FeederMessage::ConnectionClosed)),
                ); // this may be Err
                tracing::debug!("WebSocket stream closed");
            }
            .instrument(span.clone())
            .with_current_subscriber(),
        );
        Ok(sink)
    }

    /// Sends a message to the connection.
    pub async fn send_message(&self, message: WebSocketMessage) -> Result<(), TungsteniteError> {
        let mut sink_lock = self.sink.lock().await;
        sink_lock
            .send(message.into_message())
            .await
            .inspect_err(|_error| {
                tracing::warn!(parent: &self.inner.span, event = "ws_send_failed", error_kind = error_kind(_error));
            })?;
        sink_lock.flush().await.inspect_err(|_error| {
            tracing::warn!(parent: &self.inner.span, event = "ws_flush_failed", error_kind = error_kind(_error));
        })
    }

    /// Returns a [ReconnectState] for this connection.
    ///
    /// See [ReconnectState] for more information.
    pub fn reconnect_state(&self) -> ReconnectState {
        self.reconnect_state.clone()
    }
}

impl<H: WebSocketHandler> Drop for WebSocketConnection<H> {
    fn drop(&mut self) {
        self.task_reconnect.abort();
        // sending None tells the feeder to close
        let current_id = !self.inner.next_connection_id.load(Ordering::SeqCst);
        self.inner
            .message_tx
            .send((current_id, FeederMessage::DropConnectionRequest))
            .ok();
    }
}

/// A `struct` to request the [WebSocketConnection] to perform a reconnect.
///
/// This `struct` uses an [Arc] internally, so you can obtain multiple
/// `ReconnectState`s for a single [WebSocketConnection] by [cloning][Clone].
#[derive(Debug, Clone)]
pub struct ReconnectState {
    inner: Arc<ReconnectMangerInner>,
}

#[derive(Debug)]
struct ReconnectMangerInner {
    reconnect_notify: Notify,
    reconnecting: AtomicBool,
}

impl ReconnectState {
    fn new() -> Self {
        Self {
            inner: Arc::new(ReconnectMangerInner {
                reconnect_notify: Notify::new(),
                reconnecting: AtomicBool::new(false),
            }),
        }
    }

    /// Returns `true` iff the [WebSocketConnection] is undergoing a reconnection process.
    pub fn is_reconnecting(&self) -> bool {
        self.inner.reconnecting.load(Ordering::SeqCst)
    }

    /// Request the [WebSocketConnection] to perform a reconnect.
    ///
    /// Will return `false` if it is already in a reconnection process.
    pub fn request_reconnect(&self) -> bool {
        if self.is_reconnecting() {
            false
        } else {
            self.inner.reconnect_notify.notify_one();
            true
        }
    }
}

/// An enum that represents a websocket message.
///
/// See also [tungstenite::Message].
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum WebSocketMessage {
    /// A text message
    Text(String),
    /// A binary message
    Binary(Vec<u8>),
    /// A ping message
    Ping(Vec<u8>),
    /// A pong message
    Pong(Vec<u8>),
}

impl WebSocketMessage {
    fn from_message(message: tungstenite::Message) -> Option<Self> {
        match message {
            tungstenite::Message::Text(text) => Some(Self::Text(text)),
            tungstenite::Message::Binary(data) => Some(Self::Binary(data)),
            tungstenite::Message::Ping(data) => Some(Self::Ping(data)),
            tungstenite::Message::Pong(data) => Some(Self::Pong(data)),
            tungstenite::Message::Close(_) | tungstenite::Message::Frame(_) => None,
        }
    }

    fn into_message(self) -> tungstenite::Message {
        match self {
            WebSocketMessage::Text(text) => tungstenite::Message::Text(text),
            WebSocketMessage::Binary(data) => tungstenite::Message::Binary(data),
            WebSocketMessage::Ping(data) => tungstenite::Message::Ping(data),
            WebSocketMessage::Pong(data) => tungstenite::Message::Pong(data),
        }
    }
}

/// A `trait` which is used to handle events on the [WebSocketConnection].
///
/// The `struct` implementing this `trait` is required to be [Send] and `'static` because
/// it will be sent between threads.
pub trait WebSocketHandler: Send + 'static {
    /// Returns a [WebSocketConfig] that will be applied for all WebSocket connections handled by this handler.
    fn websocket_config(&self) -> WebSocketConfig {
        WebSocketConfig::default()
    }

    /// Called when a new connection has been started, and returns messages that should be sent to the server.
    ///
    /// This could be called multiple times because the connection can be reconnected.
    fn handle_start(&mut self) -> Vec<WebSocketMessage> {
        tracing::debug!("WebSocket connection started");
        vec![]
    }

    /// Called when the [WebSocketConnection] received a message, returns messages to be sent to the server.
    fn handle_message(&mut self, message: WebSocketMessage) -> Vec<WebSocketMessage>;

    /// Called when a websocket connection is closed.
    ///
    /// If the parameter `reconnect` is:
    /// - `true`, it means that the connection is being reconnected for some reason.
    /// - `false`, it means that the connection will not be reconnected, because the [WebSocketConnection] was dropped.
    #[allow(unused_variables)]
    fn handle_close(&mut self, reconnect: bool) {
        tracing::debug!("WebSocket connection closed; reconnect: {}", reconnect);
    }
}

/// Configuration for [WebSocketHandler].
///
/// Should be returned by [WebSocketHandler::websocket_config()].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct WebSocketConfig {
    /// Duration that should elapse between each attempt to start a new connection.
    ///
    /// This matters because the [WebSocketConnection] reconnects on error. If the error
    /// continues to happen, it could spam the server if `connect_cooldown` is too short. [Default]s to 3000ms.
    pub connect_cooldown: Duration,
    /// The [WebSocketConnection] will automatically reconnect when `refresh_after` has elapsed since
    /// the last connection started. If you don't want this feature, set it to [Duration::ZERO]. [Default]s to [Duration::ZERO].
    pub refresh_after: Duration,
    /// Prefix which will be used for connections that started using this `WebSocketConfig`. [Default]s to `""`.
    ///
    /// Example usage: `"wss://example.com"`
    pub url_prefix: String,
    /// During reconnection, [WebSocketHandler] might receive two identical messages
    /// even though the server sent only one message. By setting this to `true`, [WebSocketConnection]
    /// will not send duplicate messages to the [WebSocketHandler]. You should set this option to `true`
    /// when messages contain some sort of ID and are distinguishable.
    ///
    /// Note, that [WebSocketConnection] will **not** check duplicate messages when it is not under reconnection
    /// even this option is set to `true`.
    pub ignore_duplicate_during_reconnection: bool,
    /// When `ignore_duplicate_during_reconnection` is set to `true`, [WebSocketConnection] will wait for a
    /// certain amount of time to make sure no message is lost. [Default]s to 300ms
    pub reconnection_wait: Duration,
    /// A reconnection will be triggered if no messages are received within this amount of time.
    /// [Default]s to [Duration::ZERO], which means no timeout will be applied.
    pub message_timeout: Duration,
}

impl WebSocketConfig {
    /// Constructs a new `WebSocketConfig` with its fields set to [default][WebSocketConfig::default()].
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            connect_cooldown: Duration::from_millis(3000),
            refresh_after: Duration::ZERO,
            url_prefix: String::new(),
            ignore_duplicate_during_reconnection: false,
            reconnection_wait: Duration::from_millis(300),
            message_timeout: Duration::ZERO,
        }
    }
}

fn error_kind(error: &TungsteniteError) -> &'static str {
    match error {
        TungsteniteError::Io(_) => "io",
        TungsteniteError::Tls(_) => "tls",
        TungsteniteError::ConnectionClosed => "connection_closed",
        TungsteniteError::AlreadyClosed => "already_closed",
        TungsteniteError::Protocol(_) => "protocol",
        TungsteniteError::Capacity(_) => "capacity",
        TungsteniteError::Utf8 => "utf8",
        TungsteniteError::Http(_) => "http_handshake",
        _ => "websocket",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Output(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    struct Handler;
    impl WebSocketHandler for Handler {
        fn websocket_config(&self) -> WebSocketConfig {
            WebSocketConfig {
                connect_cooldown: Duration::from_millis(1),
                ..Default::default()
            }
        }
        fn handle_message(&mut self, _: WebSocketMessage) -> Vec<WebSocketMessage> {
            vec![]
        }
    }

    #[tokio::test]
    async fn reconnect_logs_failure_and_recovery_in_background_span() {
        // Keep callsites enabled while other tests run without a scoped collector.
        let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut first = tokio_tungstenite::accept_async(stream).await.unwrap();
            first.close(None).await.unwrap();
            drop(first);
            // Fail the first reconnect during the HTTP upgrade.
            let (stream, _) = listener.accept().await.unwrap();
            drop(stream);
            let (stream, _) = listener.accept().await.unwrap();
            let _recovered = tokio_tungstenite::accept_async(stream).await.unwrap();
            let _ = shutdown_rx.await;
        });
        let output = Output(Arc::new(std::sync::Mutex::new(Vec::new())));
        let writer = output.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish();
        let _subscriber_guard = tracing::subscriber::set_default(subscriber);
        let connection = WebSocketConnection::new(
            &format!("ws://{address}/secret-path?key=secret-query"),
            Handler,
        )
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if String::from_utf8_lossy(&output.0.lock().unwrap())
                    .contains("ws_reconnect_succeeded")
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|error| {
            panic!(
                "{error}: {}",
                String::from_utf8_lossy(&output.0.lock().unwrap())
            )
        });
        let log = String::from_utf8(output.0.lock().unwrap().clone()).unwrap();
        assert!(log.contains("ws_reconnect_failed"));
        assert!(log.contains("connection_id"));
        assert!(log.contains("generation=1"));
        assert!(log.contains("attempt=2"));
        assert!(!log.contains("secret-"));
        drop(connection);
        shutdown_tx.send(()).unwrap();
        server.await.unwrap();
    }
}
