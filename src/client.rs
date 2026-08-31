use crate::messages::{Command, Event, Request};
use crate::{
    ClientEvent, ConnectionState, DisconnectReason, Result, TrackAudioConfig, TrackAudioError,
};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::{Bytes, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

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
    connection_state: watch::Receiver<ConnectionState>,
    shutdown: CancellationToken,
    reconnect_tx: mpsc::Sender<()>,
    task: JoinHandle<()>,
}

impl TrackAudioClient {
    /// Returns a new [`TrackAudioClient`] using the provided configuration and starts connecting
    /// to the configured TrackAudio instance.
    ///
    /// After connecting, you can send commands to TrackAudio using [`TrackAudioClient::send()`]
    /// and subscribe to the (raw) stream of events emitted using [`TrackAudioClient::subscribe()`].
    ///
    /// # Connection is established in the background
    ///
    /// This method returns as soon as the client task is spawned; it does **not** wait for the
    /// WebSocket connection to come up, and a returned client is therefore not yet known to be
    /// connected. Commands sent before the connection is up are queued, not rejected.
    ///
    /// To wait for the connection, use [`wait_connected`](Self::wait_connected); to check it
    /// without waiting, [`connection_state`](Self::connection_state). To follow every transition,
    /// subscribe with [`TrackAudioClient::subscribe()`] and watch for
    /// [`ClientEvent::ConnectionStateChanged`], which reports [`ConnectionState::Connected`] once
    /// the connection succeeds and [`ConnectionState::Disconnected`] (with a
    /// [`DisconnectReason`]) when it fails.
    ///
    /// # Returns
    /// - `Ok(Self)`: The client, with its background task spawned.
    ///
    /// # Errors
    /// This method is currently infallible and always returns `Ok`; connection failures are
    /// reported as events rather than returned here. It returns [`Result`] so that failures
    /// detectable up front can be surfaced without a breaking change.
    // Connecting happens in the spawned task, but `async` is public API here.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    #[cfg_attr(feature = "tracing", tracing::instrument(err))]
    pub async fn connect(config: TrackAudioConfig) -> Result<Self> {
        let (command_tx, command_rx) = mpsc::channel::<Command>(config.command_channel_capacity);
        let (event_tx, _) = broadcast::channel::<Event>(config.event_channel_capacity);
        let (reconnect_tx, reconnect_rx) = mpsc::channel::<()>(1);
        let (connection_state_tx, connection_state) =
            watch::channel(ConnectionState::Connecting { attempt: 1 });
        let shutdown = CancellationToken::new();

        #[cfg(feature = "tracing")]
        tracing::trace!("Spawning client task");
        let task = tokio::runtime::Handle::current().spawn(Self::run_client_with_reconnect(
            command_rx,
            event_tx.clone(),
            connection_state_tx,
            reconnect_rx,
            shutdown.clone(),
            config,
        ));

        Ok(Self {
            inner: Arc::new(Inner {
                command_tx,
                event_tx,
                connection_state,
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
    /// As with [`connect`](Self::connect), this returns before the connection is established;
    /// see that method for how to observe the real connection state.
    ///
    /// # Returns
    /// - `Ok(Self)`: The client, with its background task spawned.
    ///
    /// # Errors
    /// This method is currently infallible and always returns `Ok`; connection failures are
    /// reported as events rather than returned here.
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
    /// As with [`connect`](Self::connect), this returns before the connection is established;
    /// see that method for how to observe the real connection state.
    ///
    /// # Returns
    /// - `Ok(Self)`: The client, with its background task spawned.
    ///
    /// # Errors
    /// - [`TrackAudioError::InvalidUrl`]: If the provided URL is invalid. This is the only
    ///   failure reported here; connection failures are reported as events.
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
    ///
    /// # Correlation
    ///
    /// TrackAudio's protocol has no request identifiers, so a response cannot be tied back to the
    /// command that caused it. This method resolves on the **first** event the `filter` accepts,
    /// whoever caused it. Events TrackAudio broadcasts to every connected client, such as a volume
    /// change the user makes with the on-screen slider, can therefore resolve a pending
    /// call, as can the response to a concurrent identical request from this same client. Filters
    /// that can match on a callsign or frequency narrow this considerably; those for commands
    /// carrying no such key (e.g. [`Command::ChangeMainVolume`]) cannot. Avoid issuing
    /// several indistinguishable requests concurrently on one connection.
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
            #[cfg(feature = "tracing")]
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
    ///   of the protocol. It resolves on the first matching event, which is not necessarily a
    ///   response to *this* request; see the `Correlation` section on
    ///   [`send_and_await`](Self::send_and_await).
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

    /// Returns the client's current connection state.
    ///
    /// This is a cheap, non-blocking snapshot of the same state reported by
    /// [`ClientEvent::ConnectionStateChanged`], for callers that need to poll rather than
    /// subscribe. It is always up to date by the time that event is observed.
    ///
    /// [`ConnectionState::Disconnected`] does not imply a retry is pending; use
    /// [`wait_connected`](Self::wait_connected) to tell a client that is still trying from one
    /// that has stopped.
    ///
    /// The exception is a client that is no longer running, after
    /// [`shutdown`](Self::shutdown) or [`terminate`](Self::terminate) or if its task ended
    /// unexpectedly. [`ConnectionState::Disconnected`] is then reported directly, so it can
    /// arrive before the matching event, or without one at all.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::time::Duration;
    /// use trackaudio::{ConnectionState, TrackAudioClient};
    ///
    /// # async fn example() -> trackaudio::Result<()> {
    /// let client = TrackAudioClient::connect_default().await?;
    /// client.wait_connected(Some(Duration::from_secs(5))).await?;
    ///
    /// // Later, to re-check without waiting:
    /// if client.connection_state() != ConnectionState::Connected {
    ///     println!("connection dropped");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn connection_state(&self) -> ConnectionState {
        let state = self.inner.connection_state.borrow().clone();

        // A task that was aborted by `terminate` or that unwound on a panic publishes no final
        // state, so a live-looking one can outlive the client that produced it.
        let client_gone = self.inner.shutdown.is_cancelled()
            || self.inner.connection_state.has_changed().is_err();
        if client_gone
            && !matches!(
                state,
                ConnectionState::Disconnected { .. } | ConnectionState::ReconnectFailed { .. }
            )
        {
            return ConnectionState::Disconnected {
                reason: DisconnectReason::Shutdown,
            };
        }

        state
    }

    /// Waits until the client is connected to the TrackAudio instance.
    ///
    /// [`connect`](Self::connect) returns before the connection is established, so this is the
    /// way to wait for one. Returns immediately if the client is already connected.
    ///
    /// A lost connection does not end the wait: with auto-reconnect enabled a
    /// [`ConnectionState::Disconnected`] is transient, and this keeps waiting for the retry to
    /// succeed. That is what makes it safe to call before TrackAudio has been started.
    ///
    /// # Parameters
    /// - `timeout`: How long to wait. If `None`, waits indefinitely. Prefer a timeout unless the
    ///   caller genuinely wants to block until TrackAudio appears.
    ///
    /// # Returns
    /// - `Ok(())`: The client is connected.
    /// - `Err(TrackAudioError)`: The wait timed out or the connection can no longer be made.
    ///
    /// # Errors
    /// - [`TrackAudioError::Timeout`]: If the timeout elapsed before connecting.
    /// - [`TrackAudioError::ClientTaskTerminated`]: If the client gave up (reconnection attempts
    ///   exhausted, auto-reconnect disabled after a failure, or the client was shut down).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::time::Duration;
    /// use trackaudio::TrackAudioClient;
    ///
    /// # async fn example() -> trackaudio::Result<()> {
    /// let client = TrackAudioClient::connect_default().await?;
    /// client.wait_connected(Some(Duration::from_secs(5))).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(skip(self), err(level = "debug"))
    )]
    pub async fn wait_connected(&self, timeout: Option<Duration>) -> Result<()> {
        let mut rx = self.inner.connection_state.clone();
        let shutdown = self.inner.shutdown.clone();

        // Checked before the state, which a dead client can leave stale at `Connected`.
        if shutdown.is_cancelled() || rx.has_changed().is_err() {
            return Err(TrackAudioError::ClientTaskTerminated);
        }

        // An already-connected client publishes no further transition, so awaiting one hangs.
        if *rx.borrow_and_update() == ConnectionState::Connected {
            return Ok(());
        }

        let fut = async move {
            loop {
                let changed = tokio::select! {
                    () = shutdown.cancelled() => return Err(TrackAudioError::ClientTaskTerminated),
                    res = rx.changed() => res,
                };
                if changed.is_err() {
                    // The sender lives in the client task, so this means the task is gone.
                    return Err(TrackAudioError::ClientTaskTerminated);
                }

                match &*rx.borrow_and_update() {
                    ConnectionState::Connected => return Ok(()),
                    #[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
                    ConnectionState::ReconnectFailed { attempts } => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(?attempts, "Giving up waiting for connection");
                        return Err(TrackAudioError::ClientTaskTerminated);
                    }
                    _ => {}
                }
            }
        };

        match timeout {
            Some(timeout) => tokio::time::timeout(timeout, fut)
                .await
                .map_err(|_| TrackAudioError::Timeout)?,
            None => fut.await,
        }
    }

    /// Gracefully shuts down the client and disconnects from the TrackAudio instance.
    pub fn shutdown(&self) {
        #[cfg(feature = "tracing")]
        tracing::debug!("Shutdown requested");
        self.inner.shutdown.cancel();
    }

    /// Forcefully terminates the client task and disconnects from the TrackAudio instance.
    ///
    /// # Notes
    /// - [`TrackAudioClient`] is [`Clone`] and every clone shares one background task. Taking
    ///   `self` by value therefore does *not* imply exclusive ownership: terminating through one
    ///   clone aborts the task for all of them, and any surviving clone will fail every
    ///   subsequent [`send`](Self::send). Prefer [`shutdown`](Self::shutdown) unless you
    ///   specifically need to abort the task without letting it close the connection cleanly.
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
        connection_state_tx: watch::Sender<ConnectionState>,
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

            Self::set_connection_state(
                &event_tx,
                &connection_state_tx,
                ConnectionState::Connecting { attempt },
            );

            match Self::establish_connection(&config.url).await {
                Ok(ws_stream) => {
                    #[cfg(feature = "tracing")]
                    tracing::info!(?attempt, "Connected to TrackAudio");
                    attempt = 0;

                    Self::set_connection_state(
                        &event_tx,
                        &connection_state_tx,
                        ConnectionState::Connected,
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

                    Self::set_connection_state(
                        &event_tx,
                        &connection_state_tx,
                        ConnectionState::Disconnected {
                            reason: disconnect_reason.clone(),
                        },
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
                            Self::set_connection_state(
                                &event_tx,
                                &connection_state_tx,
                                ConnectionState::ReconnectFailed { attempts: attempt },
                            );
                            should_reconnect = false;
                            continue;
                        }
                    }

                    Self::set_connection_state(
                        &event_tx,
                        &connection_state_tx,
                        ConnectionState::Disconnected {
                            reason: DisconnectReason::ConnectionFailed(err.to_string()),
                        },
                    );

                    should_reconnect = Self::should_attempt_reconnect(&shutdown, &config);
                }
            }

            if should_reconnect {
                (attempt, should_reconnect) = Self::await_backoff(
                    &event_tx,
                    &connection_state_tx,
                    &mut reconnect_rx,
                    &shutdown,
                    &config,
                    attempt,
                )
                .await;
            }
        }

        #[cfg(feature = "tracing")]
        tracing::debug!("Client task with reconnection completed");
    }

    /// Returns the attempt counter, which a manual reconnect resets to retry immediately, and
    /// whether the loop should keep going.
    async fn await_backoff(
        event_tx: &broadcast::Sender<Event>,
        connection_state_tx: &watch::Sender<ConnectionState>,
        reconnect_rx: &mut mpsc::Receiver<()>,
        shutdown: &CancellationToken,
        config: &TrackAudioConfig,
        mut attempt: usize,
    ) -> (usize, bool) {
        let mut should_reconnect = true;
        let backoff = Self::calculate_backoff(attempt, config);

        #[cfg(feature = "tracing")]
        tracing::debug!(?attempt, ?backoff, "Waiting before attempting reconnect");

        Self::set_connection_state(
            event_tx,
            connection_state_tx,
            ConnectionState::Reconnecting {
                attempt: attempt + 1,
                next_delay: backoff,
            },
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
                Self::set_connection_state(
                    event_tx,
                    connection_state_tx,
                    ConnectionState::Disconnected {
                        reason: DisconnectReason::Shutdown,
                    },
                );
                should_reconnect = false;
            }
        }

        (attempt, should_reconnect)
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
                    #[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
                    if let Err(err) = ws_tx.send(Message::Close(None)).await {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(?err, "Failed to send Close message");
                    }
                    return DisconnectReason::Shutdown;
                }

                Some(()) = reconnect_rx.recv() => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Manual reconnection requested, closing connection");
                    #[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
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
                                    #[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
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
                        #[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
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

    /// Updates the watch before broadcasting, so a consumer reacting to the event never reads
    /// the previous state back out of `connection_state`.
    fn set_connection_state(
        event_tx: &broadcast::Sender<Event>,
        connection_state_tx: &watch::Sender<ConnectionState>,
        state: ConnectionState,
    ) {
        connection_state_tx.send_replace(state.clone());
        Self::send_client_event(event_tx, ClientEvent::ConnectionStateChanged(state));
    }

    #[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
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
    use crate::{
        ClientEvent, ConnectionState, DisconnectReason, Event, TrackAudioClient, TrackAudioConfig,
        TrackAudioError,
    };
    use assert_matches::assert_matches;
    use std::time::Duration;
    use test_log::test;
    use tokio::sync::broadcast;

    /// Port 1 on loopback: reserved, never bound, so connecting reliably fails fast.
    fn dead_endpoint() -> TrackAudioConfig {
        TrackAudioConfig::new("ws://127.0.0.1:1/ws").expect("valid URL")
    }

    /// Reads the event stream up to the next connection state transition.
    async fn next_state(events: &mut broadcast::Receiver<Event>) -> ConnectionState {
        loop {
            let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
                .await
                .expect("client should keep emitting")
                .expect("event stream should stay open");
            if let Event::Client(ClientEvent::ConnectionStateChanged(state)) = event {
                return state;
            }
        }
    }

    /// A client that has not reached TrackAudio yet must never report itself as connected.
    #[test(tokio::test)]
    async fn connection_state_is_not_connected_before_connecting() {
        let client = TrackAudioClient::connect(dead_endpoint())
            .await
            .expect("connect should not fail");

        assert_ne!(client.connection_state(), ConnectionState::Connected);
    }

    /// `wait_connected` must give up on the timeout rather than hang when nothing is listening.
    #[test(tokio::test)]
    async fn wait_connected_times_out_when_nothing_is_listening() {
        let client = TrackAudioClient::connect(dead_endpoint())
            .await
            .expect("connect should not fail");

        let err = client
            .wait_connected(Some(Duration::from_millis(150)))
            .await
            .expect_err("should not connect");
        assert_matches!(err, TrackAudioError::Timeout);
    }

    /// Once reconnection attempts are exhausted the client task ends, so a pending wait has to
    /// resolve instead of hanging until its timeout.
    #[test(tokio::test)]
    async fn wait_connected_gives_up_once_reconnects_are_exhausted() {
        let config = dead_endpoint()
            .with_max_reconnect_attempts(Some(1))
            .with_backoff_config(Duration::from_millis(10), Duration::from_millis(10), 1.0);
        let client = TrackAudioClient::connect(config)
            .await
            .expect("connect should not fail");

        let err = client
            .wait_connected(Some(Duration::from_secs(10)))
            .await
            .expect_err("should give up");
        assert_matches!(err, TrackAudioError::ClientTaskTerminated);
    }

    /// `terminate` aborts the task mid-flight, so the last published state outlives the client
    /// and must not still be reported as live.
    #[test(tokio::test)]
    async fn connection_state_is_not_live_after_terminate() {
        let client = TrackAudioClient::connect(dead_endpoint())
            .await
            .expect("connect should not fail");
        client.clone().terminate();

        assert_matches!(
            client.connection_state(),
            ConnectionState::Disconnected {
                reason: DisconnectReason::Shutdown
            }
        );
        assert_matches!(
            client.wait_connected(Some(Duration::from_millis(50))).await,
            Err(TrackAudioError::ClientTaskTerminated)
        );
    }

    /// Asserted on the event stream, not `connection_state`, which synthesizes a terminal state
    /// for any shut-down client and would pass with the broadcast removed.
    #[test(tokio::test)]
    async fn shutdown_during_backoff_broadcasts_a_final_state() {
        let config = dead_endpoint().with_backoff_config(
            Duration::from_secs(30),
            Duration::from_secs(30),
            1.0,
        );
        let client = TrackAudioClient::connect(config)
            .await
            .expect("connect should not fail");

        // Polled rather than read off the stream, so the test does not depend on subscribing
        // before the task has had a chance to run.
        tokio::time::timeout(Duration::from_secs(5), async {
            while !matches!(
                client.connection_state(),
                ConnectionState::Reconnecting { .. }
            ) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("client should reach backoff");

        let mut events = client.subscribe();
        client.shutdown();

        assert_matches!(
            next_state(&mut events).await,
            ConnectionState::Disconnected {
                reason: DisconnectReason::Shutdown
            }
        );
    }

    #[test(tokio::test)]
    async fn connect_url_empty() {
        let err = TrackAudioClient::connect_url("  ")
            .await
            .expect_err("config should be invalid");
        assert_matches!(err, TrackAudioError::InvalidUrl(err) if err == "empty URL");
    }
}
