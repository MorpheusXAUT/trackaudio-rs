//! # TrackAudio client library
//!
//! `trackaudio` is a Rust client library for interacting with [TrackAudio](https://github.com/pierr3/TrackAudio),
//! a modern voice communication application for [VATSIM](https://vatsim.net/) air traffic controllers.
//!
//! This crate provides a high-level, async API for controlling TrackAudio programmatically via
//! its [WebSocket interface](https://github.com/pierr3/TrackAudio/wiki/SDK-documentation), allowing
//! you to build custom integrations, automation tools, or alternative user interfaces.
//!
//! ## Features
//!
//! - **Async/await API**: Built on Tokio for efficient async I/O
//! - **Type-safe commands and events**: Strongly-typed message protocol
//! - **Request-response pattern**: High-level API for commands that expect responses
//! - **Event streaming**: Subscribe to real-time events from TrackAudio
//! - **Thread-safe**: Client can be safely shared across threads
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use trackaudio::{Command, Event, TrackAudioClient};
//!
//! #[tokio::main]
//! async fn main() -> trackaudio::Result<()> {
//!     // Connect to TrackAudio (defaults to ws://127.0.0.1:49080/ws)
//!     let client = TrackAudioClient::connect_default().await?;
//!
//!     // Subscribe to events
//!     let mut events = client.subscribe();
//!
//!     // Send a command
//!     client.send(Command::PttPressed).await?;
//!
//!     // Listen for events
//!     while let Ok(event) = events.recv().await {
//!         match event {
//!             Event::TxBegin(_) => println!("Started transmitting"),
//!             Event::RxBegin(rx) => println!("Receiving from {}", rx.callsign),
//!             _ => {}
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## High-level API
//!
//! For common operations you can use [`TrackAudioApi`], which wraps [`TrackAudioClient`] and
//! provides typed convenience methods based on the [`Request`](messages::Request) pattern:
//!
//! ```rust,no_run
//! use std::time::Duration;
//! use trackaudio::TrackAudioClient;
//!
//! #[tokio::main]
//! async fn main() -> trackaudio::Result<()> {
//!     let client = TrackAudioClient::connect_default().await?;
//!     let api = client.api();
//!
//!     // Add a station and wait for its initial state
//!     let station = api.add_station("LOVV_CTR", Some(Duration::from_secs(5))).await?;
//!     println!("Added station: {station:?}");
//!
//!     // Change the main output volume
//!     let volume = api.change_main_output_volume(-20, None).await?;
//!     println!("Changed main output volume to {volume}");
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Choosing between low-level and high-level APIs
//!
//! - Use [`TrackAudioClient`] when you want **full control**:
//!   - subscribe to the raw [`Event`] stream
//!   - send arbitrary [`Command`]s
//!   - implement your own request/response logic
//!
//! - Use [`TrackAudioApi`] for **common operations** with less boilerplate:
//!   - add/remove stations
//!   - query station states
//!   - adjust volumes and other simple settings
//!   - transmit on all active frequencies
//!
//! ## Architecture
//!
//! The library is organized around three main parts:
//!
//! ### [`TrackAudioClient`]
//!
//! The core WebSocket client that handles connection management, command sending,
//! and event distribution. Use this for low-level control or when you need to
//! work directly with the command/event protocol.
//!
//! ### [`TrackAudioApi`]
//!
//! A high-level API wrapper that provides convenient methods for common operations
//! with built-in request-response matching and timeout handling.
//!
//! ### Messages
//!
//! - [`Command`]: Commands you send to TrackAudio (PTT, add station, set volume, etc.)
//! - [`Event`]: Events emitted by TrackAudio (station updates, RX/TX state changes, etc.)
//! - [`Request`](messages::Request): Trait for typed request-response patterns
//!
//! ## Configuration
//!
//! Use [`TrackAudioConfig`] to control connection parameters and internal behavior:
//!
//! IPv4 and `ws://` are currently supported; IPv6 and `wss://` are not. Check the documentation of
//! [`TrackAudioConfig`] for more details on supported URL variants.
//!
//! If you're simply trying to connect to the default TrackAudio instance on your local machine,
//! you can use [`TrackAudioClient::connect_default`] to create a client with the default configuration:
//!
//! ```rust,no_run
//! use trackaudio::TrackAudioClient;
//!
//! #[tokio::main]
//! async fn main() -> trackaudio::Result<()> {
//!     let client = TrackAudioClient::connect_default().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! If you need to customize the connection parameters, you can use [`TrackAudioClient::connect`].
//!
//! Connecting to a remote TrackAudio instance without modifying additional config values can be
//! performed via [`TrackAudioClient::connect_url`]:
//!
//! ```rust,no_run
//! use trackaudio::TrackAudioClient;
//!
//! #[tokio::main]
//! async fn main() -> trackaudio::Result<()> {
//!     // See [`TrackAudioConfig`] for supported URL variants.
//!     let client = TrackAudioClient::connect_url("192.168.1.69").await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Request/response pattern
//!
//! Some commands support a request/response style interaction by implementing the [`Request`](messages::Request) trait.
//! You can send these requests directly via [`TrackAudioClient::request`]:
//!
//! ```rust,no_run
//! use std::time::Duration;
//! use trackaudio::TrackAudioClient;
//! use trackaudio::messages::commands::GetStationStates;
//!
//! #[tokio::main]
//! async fn main() -> trackaudio::Result<()> {
//!     let client = TrackAudioClient::connect_default().await?;
//!
//!     // Get a snapshot of all station states
//!     let states = client
//!         .request(GetStationStates, Some(Duration::from_secs(5)))
//!         .await?;
//!     println!("We have {} stations", states.len());
//!
//!     Ok(())
//! }
//! ```
//!
//! The [`Request`](messages::Request) implementation ties a concrete [`Command`] (e.g., [`GetStationStates`](messages::commands::GetStationStates)
//! to the matching [`Event`] returned by TrackAudio, and converts them into a typed response
//! (e.g., `Vec<StationState>`).
//!
//! ## Working with frequencies
//!
//! TrackAudio returns and expects all frequencies to be in Hertz, however that seem a bit cumbersome
//! to work with. The [`Frequency`] type stores values in Hertz internally but offers helpers for
//! Hz, kHz, and MHz:
//!
//! ```rust
//! use trackaudio::Frequency;
//!
//! let a = Frequency::from_hz(132_600_000);
//! let b = Frequency::from_khz(132_600);
//! let c = Frequency::from_mhz(132.600);
//!
//! assert_eq!(a, b);
//! assert_eq!(b, c);
//! assert_eq!(a.as_mhz(), 132.600);
//!
//! // Convenient conversion to/from MHz as f64
//! let freq: Frequency = 132.600_f64.into();
//! let mhz: f64 = freq.into();
//! assert_eq!(freq.as_mhz(), mhz);
//!
//! // Convenient conversion to/from Hz as u64
//! let freq: Frequency = 132_600_000_u64.into();
//! let hz: u64 = freq.into();
//! assert_eq!(freq.as_hz(), hz);
//! ```
//!
//! ## Error Handling
//!
//! All operations return a [`Result<T>`](Result) type with [`TrackAudioError`] variants:
//!
//! ```rust,no_run
//! use trackaudio::{TrackAudioClient, TrackAudioError};
//!
//! async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!     match TrackAudioClient::connect_default().await {
//!         Ok(client) => { /* use client */ },
//!         Err(TrackAudioError::WebSocket(e)) => {
//!             eprintln!("Connection failed: {}", e);
//!         },
//!         Err(TrackAudioError::Timeout) => {
//!             eprintln!("Connection timed out");
//!         },
//!         Err(e) => {
//!             eprintln!("Other error: {}", e);
//!         }
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Client-emitted events
//!
//! In addition to events originating from TrackAudio itself, the client can inject internal events
//! via the [`Event::Client`] variant. These carry a [`ClientEvent`] and are used to report issues such as:
//!
//! - Connection loss
//! - Failed command transmission
//! - Event deserialization errors
//!
//! You receive them on the same broadcast channel as all other events.
//!
//! ## Features
//!
//! - `tracing`: integrate with the [`tracing`](https://docs.rs/tracing) crate
//!   and emit spans for key operations (connect, send, request, etc.). Enabled by default.
//!
//! ## External Resources
//!
//! - [TrackAudio GitHub](https://github.com/pierr3/TrackAudio)
//! - [TrackAudio SDK Documentation](https://github.com/pierr3/TrackAudio/wiki/SDK-documentation)
//! - [VATSIM](https://vatsim.net/)

mod api;
mod client;
mod config;
pub mod messages;

pub use api::TrackAudioApi;
pub use client::TrackAudioClient;
pub use config::TrackAudioConfig;
pub use messages::{ClientEvent, Command, Event, Frequency};

/// The crate's `Result` type, used throughout the library to indicate success or failure.
///
/// This type is a convenient alias for `std::result::Result` where the error type is
/// [`TrackAudioError`] and is used by all public API functions as relevant.
///
/// # Type Parameters
/// - `T`: The type of the success value contained in the `Result`.
pub type Result<T> = std::result::Result<T, TrackAudioError>;

/// The `TrackAudioError` enum represents various errors that can occur while using this crate.
#[derive(Debug, thiserror::Error)]
pub enum TrackAudioError {
    /// The TrackAudio WebSocket API URL in the configuration is invalid.
    ///
    /// This is returned when a URL given to [`TrackAudioConfig`] or
    /// [`TrackAudioClient`] cannot be parsed or normalized into a valid
    /// TrackAudio WebSocket endpoint (for example, due to an unsupported
    /// scheme, missing host, or otherwise malformed URL).
    ///
    /// The inner `String` contains a human-readable description of why the
    /// URL was rejected.
    #[error("invalid TrackAudio URL: {0}")]
    InvalidUrl(String),

    /// A WebSocket-level error occurred while talking to the TrackAudio instance.
    ///
    /// This typically indicates failures during connection establishment
    /// (DNS problems, refused connection), protocol violations, or I/O errors
    /// while sending or receiving WebSocket frames.
    ///
    /// The wrapped [`tokio_tungstenite::tungstenite::Error`] provides more
    /// detailed context about the exact failure.
    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    /// JSON (de)serialization failed for a TrackAudio message.
    ///
    /// This is returned when converting between raw WebSocket text
    /// data and high-level types such as [`Command`] or [`Event`] fails,
    /// for example, due to malformed JSON, an unexpected message shape,
    /// or incompatible schema changes.
    ///
    /// The wrapped [`serde_json::Error`] contains the underlying parser or
    /// serializer error.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// Sending a [`Command`] towards TrackAudio failed.
    ///
    /// This can happen if the internal channel used by the client to
    /// forward commands is closed, or if the underlying WebSocket send
    /// loop has already terminated and can no longer accept new messages.
    ///
    /// The inner `String` provides a brief description of the send failure.
    #[error("send error: {0}")]
    Send(String),

    /// Receiving a [`ClientEvent`] from TrackAudio failed.
    ///
    /// This can occur if the internal event channel has been closed,
    /// the WebSocket receive loop has terminated unexpectedly, or an
    /// otherwise unrecoverable receive error occurred.
    ///
    /// The inner `String` provides a brief description of the receive failure.
    #[error("receive error: {0}")]
    Receive(String),

    /// An operation exceeded its configured timeout.
    ///
    /// This variant is used for operations that are guarded by a timeout,
    /// such as connecting the client or waiting for a specific response
    /// or event. It indicates that no successful result was produced
    /// within the allotted time span.
    #[error("timeout")]
    Timeout,

    /// The internal client task has terminated unexpectedly.
    ///
    /// This usually indicates that the background task driving the
    /// WebSocket connection and event loop has exited (for example, due
    /// to an unrecoverable error or a panic), while the caller still
    /// attempted to interact with the client.
    ///
    /// Callers seeing this error should typically recreate the
    /// [`TrackAudioClient`] instance.
    #[error("client task terminated")]
    ClientTaskTerminated,
}
