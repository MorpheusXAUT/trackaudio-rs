use crate::messages::{Command, Event, Request};
use crate::{ClientEvent, Result, TrackAudioConfig, TrackAudioError};
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
    /// - [`TrackAudioError::Websocket`](crate::TrackAudioError::WebSocket): If the WebSocket connection failed
    #[cfg_attr(feature = "tracing", tracing::instrument(err))]
    pub async fn connect(config: TrackAudioConfig) -> Result<Self> {
        #[cfg(feature = "tracing")]
        tracing::info!("Connecting to TrackAudio");
        let (ws_stream, _) = tokio_tungstenite::connect_async(config.url).await?;

        #[cfg(feature = "tracing")]
        tracing::debug!("Successfully connected to TrackAudio");
        let (ws_tx, ws_rx) = ws_stream.split();

        let (command_tx, command_rx) = mpsc::channel::<Command>(config.command_channel_capacity);
        let (event_tx, _) = broadcast::channel::<Event>(config.event_channel_capacity);
        let shutdown = CancellationToken::new();

        #[cfg(feature = "tracing")]
        tracing::trace!("Spawning client task");
        let task = tokio::runtime::Handle::current().spawn(Self::run_client(
            ws_tx,
            ws_rx,
            command_rx,
            event_tx.clone(),
            shutdown.clone(),
            config.ping_interval,
        ));

        Ok(Self {
            inner: Arc::new(Inner {
                command_tx,
                event_tx,
                shutdown,
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
                                return Err(TrackAudioError::Send(error.to_string()));
                            }
                        }

                        if let Some(result) = filter(&event) {
                            return Ok(result);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        #[cfg(feature = "tracing")]
                        tracing::trace!(skipped = ?skipped, "Lagged while receiving events");
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

    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
    async fn run_client(
        mut ws_tx: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
        mut ws_rx: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        mut command_rx: mpsc::Receiver<Command>,
        event_tx: broadcast::Sender<Event>,
        shutdown: CancellationToken,
        ping_interval: Duration,
    ) {
        #[cfg(feature = "tracing")]
        tracing::debug!("Client task started");
        let mut ping_interval = tokio::time::interval(ping_interval);

        loop {
            tokio::select! {
                biased;

                _ = shutdown.cancelled() => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Shutdown requested, sending Close message");
                    if let Err(err) = ws_tx.send(Message::Close(None)).await {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(?err, "Failed to send Close message");
                    }
                    break;
                }

                _ = ping_interval.tick() => {
                    if let Err(err) = ws_tx.send(Message::Ping(Bytes::new())).await {
                        #[cfg(feature = "tracing")]
                        tracing::error!(?err, "Failed to send ping");
                        Self::send_client_event(
                            &event_tx,
                            ClientEvent::Disconnected {
                                reason: format!("Failed to send ping: {err}"),
                            }
                        );
                        break;
                    }
                }

                Some(cmd) = command_rx.recv() => {
                    match serde_json::to_string(&cmd) {
                        Ok(json) => {
                            if let Err(err) = ws_tx.send(Message::text(json)).await{
                                #[cfg(feature = "tracing")]
                                tracing::error!(?err, "Failed to send WebSocket message");
                                Self::send_client_event(
                                    &event_tx,
                                    ClientEvent::CommandSendFailed {
                                        command: cmd,
                                        error: err.to_string(),
                                    }
                                );
                                break;
                            }
                        }
                        Err(err) => {
                            #[cfg(feature = "tracing")]
                            tracing::error!(?err, "Failed to serialize command");
                            Self::send_client_event(
                                &event_tx,
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
                                        &event_tx,
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
                                Self::send_client_event(
                                    &event_tx,
                                    ClientEvent::Disconnected {
                                        reason: format!("Failed to send pong: {err}"),
                                    }
                                );
                                break;
                            }
                        }
                        Some(Ok(Message::Close(frame))) => {
                            #[cfg(feature = "tracing")]
                            tracing::info!(?frame, "WebSocket connection closed");
                            Self::send_client_event(
                                &event_tx,
                                ClientEvent::Disconnected {
                                    reason: format!("WebSocket closed by peer: {frame:?}"),
                                }
                            );
                            break;
                        },
                        Some(Ok(other)) => {
                            #[cfg(feature = "tracing")]
                            tracing::trace!(?other, "Received unexpected WebSocket message");
                            continue;
                        },
                        Some(Err(err)) => {
                            #[cfg(feature = "tracing")]
                            tracing::error!(?err, "Failed to receive WebSocket message");
                            Self::send_client_event(
                                &event_tx,
                                ClientEvent::Disconnected {
                                    reason: format!("WebSocket error: {err}"),
                                }
                            );
                            break;
                        }
                        None => {
                            #[cfg(feature = "tracing")]
                            tracing::error!("WebSocket stream ended unexpectedly");
                            Self::send_client_event(
                                &event_tx,
                                ClientEvent::Disconnected {
                                    reason: "WebSocket stream ended unexpectedly".to_string(),
                                }
                            );
                            break;
                        },
                    }
                }
            }
        }

        #[cfg(feature = "tracing")]
        tracing::debug!("Client task completed");
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
