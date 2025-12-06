use crate::messages::{Command, Event, Request};
use crate::{
    ClientEvent, ConnectionState, DisconnectReason, Result, TrackAudioConfig, TrackAudioError,
};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;
use tungstenite::{Bytes, Message};

/// [`TrackAudioClient`] is a client for interacting with a TrackAudio instance via WebSockets.
///
/// It supports sending commands to TrackAudio and subscribing to events emitted by the instance.
///
/// The client is thread-safe and can be used concurrently from multiple threads or components.
#[derive(Debug, Clone)]
pub struct TrackAudioClient {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    command_tx: mpsc::Sender<Command>,
    event_tx: broadcast::Sender<Event>,
    shutdown: CancellationToken,
    reconnect_tx: mpsc::Sender<()>,
    task: JoinHandle<()>,
}

impl TrackAudioClient {
    /// Returns a new [`TrackAudioClient`] using the provided configuration.
    /// This method automatically establishes a connection to the configured TrackAudio instance.
    ///
    /// After connecting, you can send commands to TrackAudio using [`TrackAudioClient::send()`]
    /// and subscribe to the (raw) stream of events emitted using [`TrackAudioClient::subscribe()`].
    ///
    /// # Returns
    /// - `Ok(Self)`: Indicates successful connection and initialization of the TrackAudio client.
    /// - `Err(TrackAudioError)`: Returns an error if the connection fails (e.g., network issues or invalid configuration).
    ///
    /// # Errors
    /// - [`TrackAudioError::Websocket`](TrackAudioError::WebSocket): If the WebSocket connection failed
    #[cfg_attr(feature = "tracing", tracing::instrument(err))]
    pub async fn connect(config: TrackAudioConfig) -> Result<Self> {
        let (command_tx, command_rx) = mpsc::channel::<Command>(config.command_channel_capacity);
        let (event_tx, _) = broadcast::channel::<Event>(config.event_channel_capacity);
        let (reconnect_tx, reconnect_rx) = mpsc::channel::<()>(1);
        let shutdown = CancellationToken::new();

        #[cfg(feature = "tracing")]
        tracing::trace!("Spawning client task");
        let task = tokio::runtime::Handle::current().spawn(Self::run_client_with_reconnect(
            command_rx,
            event_tx.clone(),
            reconnect_rx,
            shutdown.clone(),
            config,
        ));

        Ok(Self {
            inner: Arc::new(Inner {
                command_tx,
                event_tx,
                shutdown,
                reconnect_tx,
                task,
            }),
        })
    }

    /// Asynchronously connects to the default TrackAudio URL using the default configuration.
    ///
    /// This function establishes a connection using the default [`TrackAudioConfig`]
    /// parameters, which are retrieved via the [`TrackAudioConfig::default()`] method.
    /// This will attempt to establish a connection to `ws://127.0.0.1:49080/ws`.
    ///
    /// # Returns
    /// - `Ok(Self)`: If connection to TrackAudio was established successfully
    /// - `Err(TrackAudioError)`: If connecting to TrackAudio failed
    ///
    /// # Errors
    /// - [`TrackAudioError::WebSocket`]: If the WebSocket connection failed
    pub async fn connect_default() -> Result<Self> {
        #[cfg(feature = "tracing")]
        tracing::trace!("Connecting to default TrackAudio URL");
        Self::connect(TrackAudioConfig::default()).await
    }

    /// Asynchronously connects to a TrackAudio instance using the provided URL.
    ///
    /// This method is a convenience wrapper around [`TrackAudioConfig::new()`] and [`TrackAudioClient::connect()`].
    ///
    /// After connecting, you can send commands to TrackAudio using [`TrackAudioClient::send()`]
    /// and subscribe to the (raw) stream of events emitted using [`TrackAudioClient::subscribe()`].
    ///
    /// # Returns
    /// - `Ok(Self)`: If the connection to TrackAudio was established successfully
    /// - `Err(TrackAudioError)`: If connecting to TrackAudio failed
    ///
    /// # Errors
    /// - [`TrackAudioError::InvalidUrl`]: If the provided URL is invalid.
    /// - [`TrackAudioError::WebSocket`]: If the WebSocket connection failed.
    pub async fn connect_url(endpoint: impl AsRef<str>) -> Result<Self> {
        Self::connect(TrackAudioConfig::new(endpoint)?).await
    }

    /// Sends a [`Command`] to the TrackAudio instance.
    ///
    /// # Returns
    /// - `Ok(())`: If the command was successfully enqueued for transmission.
    /// - `Err(TrackAudioError)`: If the send operation (to the channel) fails.
    ///
    /// # Errors
    /// - [`TrackAudioError::Send`]: If the send operation (to the channel) fails.
    ///
    /// # Notes
    /// - This method sends the command asynchronously via an internal channel and does not wait
    ///   for it to be transmitted over the WebSocket connection.
    /// - Any errors during command transmission or processing are emitted asynchronously as an [`Event`].
    pub async fn send(&self, cmd: Command) -> Result<()> {
        #[cfg(feature = "tracing")]
        tracing::trace!(?cmd, "Sending command");
        self.inner
            .command_tx
            .send(cmd)
            .await
            .map_err(|e| TrackAudioError::Send(e.to_string()))
    }

    /// Sends a command and waits for a corresponding event that matches a provided filter.
    ///
    /// This asynchronous function allows sending a [`Command`] and waiting for a corresponding
    /// [`Event`] to be returned. Since TrackAudio does not provide a synchronous API, the client
    /// subscribes to the incoming event stream and completes on the first match fulfilling the `filter`.
    /// It provides optional timeout behavior to ensure the operation does not hang indefinitely.
    ///
    /// # Type Parameters
    /// - `R`: The extracted result type from the event that matches the filter.
    /// - `F`: A filter function type that defines the matching condition. This function takes a
    ///   reference to an [`Event`] and returns an `Option<E>`.
    ///
    /// # Parameters
    /// - `cmd`: The command to be sent.
    /// - `timeout`: An optional duration specifying the maximum time to wait for a matching event.
    ///   If `None`, this method waits indefinitely for a matching event.
    /// - `filter`: A filter function used to match and extract the result from an event. This
    ///   function is called for every received event, and if it returns `Some(R)`, the function
    ///   exits and returns `R`. Otherwise, the loop continues to wait for another event (until the
    ///   optional timeout expires).
    ///
    /// # Returns
    /// - `Ok(E)`: If a matching event is received and its result is successfully extracted.
    /// - `Err(TrackAudioError)`: If an error occurs during the process.
    ///
    /// # Errors
    /// - [`TrackAudioError::Timeout`]: If the operation times out.
    /// - [`TrackAudioError::Send`]: If an error occurs while sending the command.
    /// - [`TrackAudioError::Receive`]: If an error occurs while receiving events.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, filter), err))]
    pub async fn send_and_await<R, F>(
        &self,
        cmd: Command,
        timeout: Option<Duration>,
        mut filter: F,
    ) -> Result<R>
    where
        F: FnMut(&Event) -> Option<R>,
    {
        let mut rx = self.subscribe();

        #[cfg(feature = "tracing")]
        tracing::trace!("Sending command");
        self.send(cmd.clone()).await?;

        let fut = async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Event::Client(ClientEvent::CommandSendFailed { command, error }) =
                            &event
                        {
                            if command == &cmd {
                                #[cfg(feature = "tracing")]
                                tracing::trace!(?cmd, ?event, "Command send failed");
                                return Err(TrackAudioError::Send(error.clone()));
                            }
                        }

                        if let Some(result) = filter(&event) {
                            return Ok(result);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        #[cfg(feature = "tracing")]
                        tracing::trace!(?skipped, "Lagged while receiving events");
                        return Err(TrackAudioError::Receive(format!(
                            "lagged by {skipped} events"
                        )));
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        #[cfg(feature = "tracing")]
                        tracing::trace!("Event channel closed");
                        return Err(TrackAudioError::ClientTaskTerminated);
                    }
                }
            }
        };

        if let Some(timeout) = timeout {
            #[cfg(feature = "tracing")]
            tracing::trace!("Waiting for response with timeout");
            tokio::time::timeout(timeout, fut)
                .await
                .map_err(|_| TrackAudioError::Timeout)?
        } else {
            tracing::trace!("Waiting for response");
            fut.await
        }
    }

    /// Sends a typed [`Request`] and asynchronously waits for the corresponding
    /// response event.
    ///
    /// This method provides a high-level typed request/response interface on top of
    /// the TrackAudio protocol. It:
    ///
    /// 1. Converts the given request into a [`Command`] via
    ///    [`Request::into_command`].
    /// 2. Sends that command over the WebSocket connection.
    /// 3. Listens to incoming [`Event`]s from the server.
    /// 4. Passes each event to [`Request::extract`] to determine whether it is the
    ///    matching response.
    /// 5. Resolves when a response is decoded, or when the optional timeout elapses.
    ///
    /// # Parameters
    ///
    /// - `req`: The typed request implementing [`Request`].
    /// - `timeout`: Optional duration after which the request will abort with a
    ///   timeout error. If `None`, the request waits indefinitely.
    ///
    /// # Returns
    ///
    /// - `Ok(T)`: where `T` is the associated [`Request::Response`](Request::Response) type if a
    ///   matching response event is received.
    /// - `Err(TrackAudioError)`: If an error occurs during the process.
    ///
    /// # Errors
    /// - [`TrackAudioError::Timeout`]: If the operation times out.
    /// - [`TrackAudioError::Send`]: If an error occurs while sending the command.
    /// - [`TrackAudioError::Receive`]: If an error occurs while receiving events.
    ///
    /// # Matching Logic
    ///
    /// After sending the command, this method observes **all incoming events** for
    /// this connection until:
    ///
    /// - [`Request::extract`] returns `Some(response)` → the response is returned.
    /// - The timeout (if set) expires.
    /// - The connection is shut down or errored.
    ///
    /// Only the request implementation knows how to interpret which server event
    /// corresponds to this specific request. That logic is entirely encapsulated in
    /// the [`Request`] trait.
    ///
    /// # Notes
    ///
    /// - This method reflects the "request → event stream → matching response" model
    ///   of the protocol.
    /// - For many common protocol operations, you may prefer the convenience methods
    ///   provided by [`TrackAudioApi`](crate::TrackAudioApi) (e.g. `add_station`, `get_station_state`,
    ///   etc.).
    /// - The method does **not** attempt retries; retry policy should be implemented
    ///   at a higher layer if needed.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self), err))]
    pub async fn request<R: Request>(
        &self,
        req: R,
        timeout: Option<Duration>,
    ) -> Result<R::Response> {
        let cmd = req.into_command();
        self.send_and_await(cmd.clone(), timeout, move |event| R::extract(event, &cmd))
            .await
    }

    /// Subscribes to all events emitted by the TrackAudio instance.
    ///
    /// # Returns
    ///
    /// A [`broadcast::Receiver`] of type [`Event`] that can be used to receive events from the
    /// TrackAudio instance. This channel will also receive any errors emitted during command
    /// transmission or processing.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.event_tx.subscribe()
    }

    /// Gracefully shuts down the client and disconnects from the TrackAudio instance.
    pub fn shutdown(&self) {
        #[cfg(feature = "tracing")]
        tracing::debug!("Shutdown requested");
        self.inner.shutdown.cancel();
    }

    /// Forcefully terminates the client task and disconnects from the TrackAudio instance.
    pub fn terminate(self) {
        #[cfg(feature = "tracing")]
        tracing::debug!("Termination requested");
        self.inner.shutdown.cancel();
        self.inner.task.abort();
    }

    /// Manually triggers a reconnection attempt.
    ///
    /// This method can be used to force a reconnection even if the client is currently connected.
    /// It will gracefully close the existing connection and attempt to establish a new one.
    ///
    /// # Returns
    /// - `Ok(())`: If the reconnection request was successfully queued
    /// - `Err(TrackAudioError)`: If the reconnection request could not be sent
    ///
    /// # Errors
    /// - [`TrackAudioError::ClientTaskTerminated`]: If the client task has been terminated
    ///
    /// # Notes
    /// - If a reconnection request is already pending, this method will return `Ok(())` immediately,
    ///   effectively deduplicating the request.
    pub fn reconnect(&self) -> Result<()> {
        #[cfg(feature = "tracing")]
        tracing::debug!("Manual reconnection requested");
        match self.inner.reconnect_tx.try_send(()) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(())) => {
                #[cfg(feature = "tracing")]
                tracing::debug!("Reconnection already pending");
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(())) => {
                Err(TrackAudioError::ClientTaskTerminated)
            }
        }
    }

    fn calculate_backoff(attempt: usize, config: &TrackAudioConfig) -> Duration {
        if attempt == 0 {
            return config.initial_backoff;
        }

        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        let backoff_secs = config.initial_backoff.as_secs_f64()
            * config.backoff_multiplier.powi((attempt - 1) as i32);
        let backoff = Duration::from_secs_f64(backoff_secs.min(config.max_backoff.as_secs_f64()));

        #[cfg(feature = "reconnect-jitter")]
        {
            let jitter = (rand::random::<f64>() * 0.2 - 0.1) * backoff.as_secs_f64();
            Duration::from_secs_f64((backoff.as_secs_f64() + jitter).max(0.0))
        }
        #[cfg(not(feature = "reconnect-jitter"))]
        backoff
    }

    async fn establish_connection(url: &str) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
        #[cfg(feature = "tracing")]
        tracing::debug!("Attempting to establish WebSocket connection");

        let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;

        #[cfg(feature = "tracing")]
        tracing::debug!("Successfully established WebSocket connection");

        Ok(ws_stream)
    }

    fn should_attempt_reconnect(shutdown: &CancellationToken, config: &TrackAudioConfig) -> bool {
        if shutdown.is_cancelled() {
            #[cfg(feature = "tracing")]
            tracing::debug!("Shutdown requested, not reconnecting");
            return false;
        }

        if !config.enable_auto_reconnect {
            #[cfg(feature = "tracing")]
            tracing::debug!("Auto-reconnect disabled, not reconnecting");
            return false;
        }

        true
    }

    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
    async fn run_client_with_reconnect(
        mut command_rx: mpsc::Receiver<Command>,
        event_tx: broadcast::Sender<Event>,
        mut reconnect_rx: mpsc::Receiver<()>,
        shutdown: CancellationToken,
        config: TrackAudioConfig,
    ) {
        #[cfg(feature = "tracing")]
        tracing::debug!("Client task with reconnection started");

        let mut attempt = 0;
        let mut should_reconnect = true;

        while should_reconnect {
            attempt += 1;

            Self::send_client_event(
                &event_tx,
                ClientEvent::ConnectionStateChanged(ConnectionState::Connecting { attempt }),
            );

            match Self::establish_connection(&config.url).await {
                Ok(ws_stream) => {
                    #[cfg(feature = "tracing")]
                    tracing::info!(?attempt, "Connected to TrackAudio");
                    attempt = 0;

                    Self::send_client_event(
                        &event_tx,
                        ClientEvent::ConnectionStateChanged(ConnectionState::Connected),
                    );

                    let (ws_tx, ws_rx) = ws_stream.split();

                    let disconnect_reason = Self::run_client(
                        ws_tx,
                        ws_rx,
                        &mut command_rx,
                        &event_tx,
                        &mut reconnect_rx,
                        &shutdown,
                        config.ping_interval,
                    )
                    .await;

                    #[cfg(feature = "tracing")]
                    tracing::info!(?attempt, "Disconnected from TrackAudio");

                    Self::send_client_event(
                        &event_tx,
                        ClientEvent::ConnectionStateChanged(ConnectionState::Disconnected {
                            reason: disconnect_reason.clone(),
                        }),
                    );

                    should_reconnect = Self::should_attempt_reconnect(&shutdown, &config);
                }
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(?attempt, ?err, "Connection attempt failed");

                    if let Some(max_attempts) = config.max_reconnect_attempts {
                        if attempt >= max_attempts {
                            #[cfg(feature = "tracing")]
                            tracing::error!(
                                ?attempt,
                                ?max_attempts,
                                "Max reconnection attempts reached"
                            );
                            Self::send_client_event(
                                &event_tx,
                                ClientEvent::ConnectionStateChanged(
                                    ConnectionState::ReconnectFailed { attempts: attempt },
                                ),
                            );
                            should_reconnect = false;
                            continue;
                        }
                    }

                    Self::send_client_event(
                        &event_tx,
                        ClientEvent::ConnectionStateChanged(ConnectionState::Disconnected {
                            reason: DisconnectReason::ConnectionFailed(err.to_string()),
                        }),
                    );

                    should_reconnect = Self::should_attempt_reconnect(&shutdown, &config);
                }
            }

            if should_reconnect {
                let backoff = Self::calculate_backoff(attempt, &config);

                #[cfg(feature = "tracing")]
                tracing::debug!(?attempt, ?backoff, "Waiting before attempting reconnect");

                Self::send_client_event(
                    &event_tx,
                    ClientEvent::ConnectionStateChanged(ConnectionState::Reconnecting {
                        attempt: attempt + 1,
                        next_delay: backoff,
                    }),
                );

                tokio::select! {
                    () = tokio::time::sleep(backoff) => {},
                    Some(()) = reconnect_rx.recv() => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Manual reconnection requested during backoff");
                        attempt = 0;
                    }
                    () = shutdown.cancelled() => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Shutdown requested during backoff");
                        should_reconnect = false;
                    }
                }
            }
        }

        #[cfg(feature = "tracing")]
        tracing::debug!("Client task with reconnection completed");
    }

    #[allow(clippy::too_many_lines)]
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
    async fn run_client(
        mut ws_tx: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
        mut ws_rx: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        command_rx: &mut mpsc::Receiver<Command>,
        event_tx: &broadcast::Sender<Event>,
        reconnect_rx: &mut mpsc::Receiver<()>,
        shutdown: &CancellationToken,
        ping_interval: Duration,
    ) -> DisconnectReason {
        #[cfg(feature = "tracing")]
        tracing::debug!("Client task started");
        let mut ping_interval = tokio::time::interval(ping_interval);

        loop {
            tokio::select! {
                biased;

                () = shutdown.cancelled() => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Shutdown requested, sending Close message");
                    if let Err(err) = ws_tx.send(Message::Close(None)).await {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(?err, "Failed to send Close message");
                    }
                    return DisconnectReason::Shutdown;
                }

                Some(()) = reconnect_rx.recv() => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Manual reconnection requested, closing connection");
                    if let Err(err) = ws_tx.send(Message::Close(None)).await {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(?err, "Failed to send Close message");
                    }
                    return DisconnectReason::ManualReconnect;
                }

                _ = ping_interval.tick() => {
                    if let Err(err) = ws_tx.send(Message::Ping(Bytes::new())).await {
                        #[cfg(feature = "tracing")]
                        tracing::error!(?err, "Failed to send ping");
                        return DisconnectReason::PingFailed(err.to_string());
                    }
                }

                Some(cmd) = command_rx.recv() => {
                    match serde_json::to_string(&cmd) {
                        Ok(json) => {
                            if let Err(err) = ws_tx.send(Message::text(json)).await{
                                #[cfg(feature = "tracing")]
                                tracing::error!(?err, "Failed to send WebSocket message");
                                Self::send_client_event(
                                    event_tx,
                                    ClientEvent::CommandSendFailed {
                                        command: cmd,
                                        error: err.to_string(),
                                    }
                                );
                                return DisconnectReason::CommandSendFailed(err.to_string());
                            }
                        }
                        Err(err) => {
                            #[cfg(feature = "tracing")]
                            tracing::error!(?err, "Failed to serialize command");
                            Self::send_client_event(
                                event_tx,
                                ClientEvent::CommandSendFailed {
                                    command: cmd,
                                    error: format!("Failed to serialize command: {err}"),
                                }
                            );
                        }
                    }
                }

                msg = ws_rx.next() => {
                    match msg {
                        Some(Ok(Message::Text(json))) => {
                            match serde_json::from_str::<Event>(&json) {
                                Ok(event) => {
                                    if let Err(err) = event_tx.send(event) {
                                        #[cfg(feature = "tracing")]
                                        tracing::debug!(?err, "Failed to send event");
                                    }
                                }
                                Err(err) => {
                                    #[cfg(feature = "tracing")]
                                    tracing::warn!(?err, ?json, "Failed to deserialize event");
                                    Self::send_client_event(
                                        event_tx,
                                        ClientEvent::EventDeserializationFailed {
                                            raw: json.to_string(),
                                            error: err.to_string(),
                                        }
                                    );
                                }
                            }
                        }
                        Some(Ok(Message::Ping(payload))) => {
                            #[cfg(feature = "tracing")]
                            tracing::trace!(?payload, "Received ping");
                            if let Err(err) = ws_tx.send(Message::Pong(payload)).await {
                                #[cfg(feature = "tracing")]
                                tracing::warn!(?err, "Failed to send pong");
                                return DisconnectReason::PongFailed(err.to_string());
                            }
                        }
                        Some(Ok(Message::Pong(_))) => {},
                        Some(Ok(Message::Close(frame))) => {
                            #[cfg(feature = "tracing")]
                            tracing::info!(?frame, "WebSocket connection closed");
                            let (code, reason) = match frame.as_ref() {
                                Some(f) => {
                                    let reason = if f.reason.is_empty() {
                                        None
                                    } else {
                                        Some(f.reason.to_string())
                                    };
                                    (Some(f.code.into()), reason)
                                }
                                None => (None, None)
                            };
                            return DisconnectReason::ClosedByPeer {code, reason};
                        },
                        Some(Ok(other)) => {
                            #[cfg(feature = "tracing")]
                            tracing::trace!(?other, "Received unexpected WebSocket message");
                        },
                        Some(Err(err)) => {
                            #[cfg(feature = "tracing")]
                            tracing::error!(?err, "Failed to receive WebSocket message");
                            return DisconnectReason::WebSocketError(err.to_string());
                        }
                        None => {
                            #[cfg(feature = "tracing")]
                            tracing::error!("WebSocket stream ended unexpectedly");
                            return DisconnectReason::StreamEnded;
                        },
                    }
                }
            }
        }
    }

    fn send_client_event(event_tx: &broadcast::Sender<Event>, client_event: ClientEvent) {
        if let Err(err) = event_tx.send(Event::Client(client_event)) {
            #[cfg(feature = "tracing")]
            tracing::debug!(?err, "Failed to send client event");
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        #[cfg(feature = "tracing")]
        tracing::info!("Dropping TrackAudioClient");
        self.shutdown.cancel();
    }
}

#[cfg(test)]
mod tests {
    use crate::TrackAudioClient;
    use crate::TrackAudioError;
    use assert_matches::assert_matches;
    use test_log::test;

    #[test(tokio::test)]
    async fn connect_url_empty() {
        let err = TrackAudioClient::connect_url("  ")
            .await
            .expect_err("config should be invalid");
        assert_matches!(err, TrackAudioError::InvalidUrl(err) if err == "empty URL");
    }
}
