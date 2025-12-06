pub mod commands;
pub mod events;

pub use commands::Command;
pub use events::{ClientEvent, ConnectionState, DisconnectReason, Event};

use serde::{Deserialize, Serialize};

/// A typed request that can be sent through a [`crate::TrackAudioClient`].
///
/// The `Request` trait defines how a [`Command`] defined the in TrackAudio WebSocket API ties to
/// a matching [`Event`] response sent asynchronously.
///
/// # Overview
///
/// A `Request` consists of:
///
/// - An associated [`Response`](Request::Response) type representing the expected result for this request.
/// - A method to convert the request into a concrete [`Command`] that can be sent to the TrackAudio instance.
/// - A method to extract (match/decode) the corresponding response value from an incoming [`Event`] stream.
///
/// This trait powers the generic [`TrackAudioClient::request`](crate::TrackAudioClient::request) API, where the client automatically:
///
/// 1. Sends the command created by [`into_command`](Request::into_command)
/// 2. Listens for incoming events
/// 3. Passes each event to [`extract`](Request::extract)
/// 4. Resolves when `extract` returns `Some(Response)`
///
/// # Associated Types
///
/// ## `type Response`
///
/// The typed value returned when a matching response event is received.
/// This can be any type that represents the decoded successful result of
/// the request (e.g. `StationState`, `Vec<StationState>`, `()`, etc.).
///
/// # Required Methods
///
/// ## `fn into_command(self) -> Command`
///
/// Converts this request into its corresponding outgoing [`Command`].
/// This defines what is actually sent over the WebSocket connection.
///
/// ## `fn extract(event: &Event, cmd: &Command) -> Option<Self::Response>`
///
/// Attempts to decode a matching response from the given [`Event`].
///
/// - Return `Some(response)` if this event is the response for the
///   given request.
/// - Return `None` if the event is not relevant.
///
/// The `cmd` parameter is provided in case the request carries context
/// (e.g., contains a callsign) that needs to be matched against the event.
///
/// # Notes
///
/// - `extract` is called for **all** incoming events until a match is found.
/// - If multiple events could match a command, `extract` should choose the
///   definitive response (e.g., the first state update, or a dedicated
///   acknowledgment event).
/// - The trait does not prescribe error semantics; if the protocol emits
///   explicit error events, they should be converted into `Err` inside
///   [`TrackAudioClient::request`](crate::TrackAudioClient::request), not handled here.
///
/// This trait abstracts the low-level protocol into a clean-typed API,
/// enabling the higher-level request/response mechanism used throughout
/// the library.
pub trait Request: std::fmt::Debug {
    type Response;

    fn into_command(self) -> Command;
    fn extract(event: &Event, cmd: &Command) -> Option<Self::Response>;
}

/// A radio frequency in Hertz.
///
/// Frequencies are stored and serialized as Hertz (Hz) as required by TrackAudio,
/// but can be constructed from Megahertz (MHz) or Kilohertz (kHz) for convenience.
///
/// # Examples
/// ```rust
/// use trackaudio::Frequency;
///
/// // Construct from MHz
/// let freq = Frequency::from_mhz(132.600);
/// assert_eq!(freq, Frequency(132_600_000));
/// assert_eq!(freq.as_hz(), 132_600_000);
/// assert_eq!(freq.as_khz(), 132_600);
/// assert_eq!(freq.as_mhz(), 132.600);
///
/// // Construct from kHz
/// let freq = Frequency::from_khz(132_600);
/// assert_eq!(freq, Frequency(132_600_000));
/// assert_eq!(freq.as_hz(), 132_600_000);
/// assert_eq!(freq.as_khz(), 132_600);
/// assert_eq!(freq.as_mhz(), 132.600);
///
/// // Construct from Hz directly
/// let freq = Frequency::from_hz(132_600_000);
/// assert_eq!(freq, Frequency(132_600_000));
/// assert_eq!(freq.as_hz(), 132_600_000);
/// assert_eq!(freq.as_khz(), 132_600);
/// assert_eq!(freq.as_mhz(), 132.600);
///
/// // Convert from u64 (Hertz)
/// let freq: Frequency = 132_600_000.into();
/// assert_eq!(freq, Frequency(132_600_000));
/// assert_eq!(freq.as_hz(), 132_600_000);
/// assert_eq!(freq.as_khz(), 132_600);
/// assert_eq!(freq.as_mhz(), 132.600);
///
/// // Convert to u64 (Hertz)
/// let hz: u64 = Frequency::from_mhz(132.600).into();
/// assert_eq!(hz, 132_600_000);
///
/// // Convert from f64 (Megahertz)
/// let freq: Frequency = 132.6f64.into();
/// assert_eq!(freq, Frequency(132_600_000));
/// assert_eq!(freq.as_hz(), 132_600_000);
/// assert_eq!(freq.as_khz(), 132_600);
/// assert_eq!(freq.as_mhz(), 132.600);
///
/// // Convert to f64 (Megahertz)
/// let mhz: f64 = Frequency::from_mhz(132.600).into();
/// assert_eq!(mhz, 132.600f64);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Frequency(pub u64);

impl Frequency {
    /// Creates a [`Frequency`] from Hertz (Hz).
    ///
    /// This is the same as constructing the type directly, as the frequency is stored in Hertz internally.
    ///
    /// # Examples
    /// ```rust
    /// use trackaudio::Frequency;
    ///
    /// let freq = Frequency::from_hz(132_600_000);
    /// assert_eq!(freq, Frequency(132_600_000));
    /// assert_eq!(freq.as_hz(), 132_600_000);
    /// assert_eq!(freq.as_khz(), 132_600);
    /// assert_eq!(freq.as_mhz(), 132.600);
    /// ```
    #[must_use]
    pub const fn from_hz(hz: u64) -> Self {
        Self(hz)
    }

    /// Creates a [`Frequency`] from Kilohertz (kHz).
    ///
    /// # Examples
    /// ```rust
    /// use trackaudio::Frequency;
    ///
    /// let freq = Frequency::from_khz(132_600);
    /// assert_eq!(freq, Frequency(132_600_000));
    /// assert_eq!(freq.as_hz(), 132_600_000);
    /// assert_eq!(freq.as_khz(), 132_600);
    /// assert_eq!(freq.as_mhz(), 132.600);
    /// ```
    #[must_use]
    pub const fn from_khz(khz: u64) -> Self {
        Self(khz * 1_000)
    }

    /// Creates a [`Frequency`] from Megahertz (MHz).
    ///
    /// # Examples
    /// ```rust
    /// use trackaudio::Frequency;
    ///
    /// let freq = Frequency::from_mhz(132.600);
    /// assert_eq!(freq, Frequency(132_600_000));
    /// assert_eq!(freq.as_hz(), 132_600_000);
    /// assert_eq!(freq.as_khz(), 132_600);
    /// assert_eq!(freq.as_mhz(), 132.600);
    /// ```
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    #[must_use]
    pub fn from_mhz(mhz: f64) -> Self {
        Self((mhz * 1_000_000.0f64).round() as u64)
    }

    /// Returns the frequency in Hertz (Hz).
    ///
    /// This is the same as accessing the underlying type, as the frequency is stored in Hertz internally.
    #[must_use]
    pub const fn as_hz(&self) -> u64 {
        self.0
    }

    /// Returns the frequency in Kilohertz (kHz).
    #[must_use]
    pub const fn as_khz(&self) -> u64 {
        self.0 / 1_000
    }

    /// Returns the frequency in Megahertz (MHz).
    #[allow(clippy::cast_precision_loss)]
    #[must_use]
    pub fn as_mhz(&self) -> f64 {
        self.0 as f64 / 1_000_000.0f64
    }
}

impl std::fmt::Debug for Frequency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Frequency")
            .field(&format_args!("{:.3} MHz", self.as_mhz()))
            .finish()
    }
}

impl std::fmt::Display for Frequency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display as MHz, as most common for aviation purposes
        write!(f, "{:.3} MHz", self.as_mhz())
    }
}

impl From<u64> for Frequency {
    fn from(hz: u64) -> Self {
        Self::from_hz(hz)
    }
}

impl From<Frequency> for u64 {
    fn from(freq: Frequency) -> Self {
        freq.as_hz()
    }
}

impl From<f64> for Frequency {
    fn from(mhz: f64) -> Self {
        Self::from_mhz(mhz)
    }
}

impl From<Frequency> for f64 {
    fn from(freq: Frequency) -> Self {
        freq.as_mhz()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod frequency {
        use super::Frequency;

        const EPSILON: f64 = 1e-6f64;

        #[test]
        fn from_hz() {
            let f = Frequency::from_hz(132_600_000);
            assert_eq!(f.as_hz(), 132_600_000);
            assert_eq!(f.as_khz(), 132_600);
            assert!((f.as_mhz() - 132.600f64).abs() < EPSILON);
        }

        #[test]
        fn from_khz() {
            let f = Frequency::from_khz(132_600);
            assert_eq!(f.as_hz(), 132_600_000);
            assert_eq!(f.as_khz(), 132_600);
            assert!((f.as_mhz() - 132.600f64).abs() < EPSILON);
        }

        #[test]
        fn from_mhz() {
            let f = Frequency::from_mhz(132.600f64);
            assert_eq!(f.as_hz(), 132_600_000);
            assert_eq!(f.as_khz(), 132_600);
            assert!((f.as_mhz() - 132.600f64).abs() < EPSILON);
        }

        #[test]
        fn from_mhz_rounding() {
            let f = Frequency::from_mhz(132.600_000_1f64);
            assert_eq!(f.as_hz(), 132_600_000);

            let f = Frequency::from_mhz(132.600_000_5f64);
            assert_eq!(f.as_hz(), 132_600_001);

            let f = Frequency::from_mhz(132.600_000_9f64);
            assert_eq!(f.as_hz(), 132_600_001);

            let f = Frequency::from_mhz(132.600_001_0f64);
            assert_eq!(f.as_hz(), 132_600_001);
        }

        #[test]
        fn from_u64() {
            let f = Frequency::from_hz(132_600_000);
            assert_eq!(Frequency::from(132_600_000u64), f);
        }

        #[test]
        fn from_f64() {
            let f = Frequency::from_hz(132_600_000);
            assert_eq!(Frequency::from(132.600f64), f);
        }

        #[test]
        fn u64_from() {
            let f = Frequency::from_hz(132_600_000);
            assert_eq!(u64::from(f), 132_600_000);
        }

        #[test]
        fn f64_from() {
            let f = Frequency::from_hz(132_600_000);
            assert!((f64::from(f) - 132.600f64).abs() < EPSILON);
        }

        #[test]
        fn debug() {
            let f = Frequency::from_hz(132_600_000);
            assert_eq!(format!("{f:?}"), "Frequency(132.600 MHz)");
        }

        #[test]
        fn display() {
            let f = Frequency::from_hz(132_600_000);
            assert_eq!(format!("{f}"), "132.600 MHz");
        }
    }
}
