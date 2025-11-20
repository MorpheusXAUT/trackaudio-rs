//! Events emitted by TrackAudio.
//!
//! This module contains all events that are emitted by TrackAudio after state changes or user
//! interaction.
//!
//! # Overview
//!
//! Events are sent by TrackAudio as JSON messages via its WebSocket API and typically either
//! indicate an external change (e.g., a third party starts transmitting on frequency), the result
//! of a user interaction, or a direct response to a [`Command`]. The main [`Event`] enum contains
//! all available events, including associated payload structs.
//!
//! # External documentation
//!
//! For more details on TrackAudio's event protocol, see the
//! [SDK documentation](https://github.com/pierr3/TrackAudio/wiki/SDK-documentation#outgoing-messages)
//! as well as the [respective implementation](https://github.com/pierr3/TrackAudio/blob/main/backend/include/sdkWebsocketMessage.hpp).

use crate::{Command, Frequency};
use serde::Deserialize;

/// Represents an event received from the TrackAudio instance.
///
/// These messages are sent by TrackAudio to all clients connected to the WebSocket API and
/// represent state changes or other events that occur (either due to user interaction or
/// internal changes).
///
/// Additionally, the `ClientEvent` variant can be used to capture events that occur on the
/// [`TrackAudioClient`](crate::TrackAudioClient) side, such as connection failures or errors.
///
/// # Deserialization
///
/// Events are deserialized from JSON strings sent by TrackAudio with a `type` field indicating the
/// variant name and a `value` field containing the variant's data.
///
/// # Notes
///
/// - TrackAudio's outgoing messages SDK documentation can be found on
///   [GitHub](https://github.com/pierr3/TrackAudio/wiki/SDK-documentation#outgoing-messages).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Event {
    /// Voice connection state changed.
    ///
    /// Emitted when the connection to the voice server is established or lost.
    #[serde(rename = "kVoiceConnectedState")]
    VoiceConnectedState(VoiceConnectedState),

    /// Station added.
    ///
    /// Emitted when a new station is successfully added to TrackAudio, e.g., as a response to
    /// [`Command::AddStation`].
    #[serde(rename = "kStationAdded")]
    StationAdded(StationAdded),

    /// A (monitored) station's state has been updated.
    ///
    /// Emitted when any property of a station changes (e.g., rx/tx/xc state, volume, etc.), or
    /// after a station is added or removed from the instance. Emitted as a response to
    /// [`Command::AddStation`], including the info whether the station was found.
    #[serde(rename = "kStationStateUpdate")]
    StationStateUpdate(StationState),

    /// An (unassociated) Frequency has been removed.
    ///
    /// Emitted when a manually tuned frequency (without a station) is removed from TrackAudio.
    #[serde(rename = "kFrequencyRemoved")]
    FrequencyRemoved(FrequencyRemoved),

    /// Full state snapshot of all stations.
    ///
    /// Emitted as a response to [`Command::GetStationState`], containing a list of all stations
    /// currently monitored by TrackAudio.
    #[serde(rename = "kStationStates")]
    StationStates(StationStates),

    /// Transmission started on one or more frequencies.
    ///
    /// Emitted when the user begins transmitting (either by pressing their PTT button or as
    /// a response to a [`Command::PttPressed`]).
    #[serde(rename = "kTxBegin")]
    TxBegin(TxBegin),

    /// Transmission ended on one or more frequencies.
    ///
    /// Emitted when the user finishes transmitting (either by releasing their PTT button or as
    /// a response to [`Command::PttReleased`]).
    #[serde(rename = "kTxEnd")]
    TxEnd(TxEnd),

    /// Started receiving transmission on one or more frequencies.
    ///
    /// Emitted when another station begins transmitting on a monitored frequency.
    #[serde(rename = "kRxBegin")]
    RxBegin(RxBegin),

    /// Stopped receiving transmission on one or more frequencies.
    ///
    /// Emitted when another station stops transmitting on a monitored frequency. Contains a list of
    /// stations still transmitting on frequency (in the case of simultaneous transmissions).
    #[serde(rename = "kRxEnd")]
    RxEnd(RxEnd),

    /// The main volume level changed.
    ///
    /// Emitted when the user adjusts the main volume (either by using the volume slider in the
    /// client or as a response to [`Command::ChangeMainVolume`]).
    #[serde(rename = "kMainVolumeChange")]
    MainVolumeChange(MainVolumeChange),

    /// Frequency state update (deprecated).
    ///
    /// # Deprecated
    ///
    /// This event is deprecated by TrackAudio and only emitted for backwards
    /// compatibility. Use [`Event::StationStateUpdate`] instead.
    #[serde(rename = "kFrequencyStateUpdate")]
    #[deprecated(
        since = "0.1.0",
        note = "This event is deprecated by TrackAudio and only emitted for backwards compatibility. Use StationStateUpdate instead."
    )]
    #[allow(deprecated)]
    FrequencyStateUpdate(FrequencyStateUpdate),

    /// Client-side event not received from TrackAudio.
    ///
    /// These events are generated locally and not deserialized from JSON, but are used to
    /// communicate the [`TrackAudioClient`](crate::TrackAudioClient)'s current (internal) state.
    #[serde(skip)]
    Client(ClientEvent),

    /// Unknown or unrecognized event type.
    ///
    /// Used as a fallback for forward compatibility when new event types are added.
    #[serde(other)]
    Unknown,
}

/// Voice connection state payload.
///
/// Indicates whether TrackAudio is currently connected to the voice server.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VoiceConnectedState {
    /// Whether the voice connection is established.
    pub connected: bool,
}

/// Information about a newly added station.
///
/// Indicates a station was successfully added to TrackAudio.
///
/// Emitted in response to [`Command::AddStation`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StationAdded {
    /// The callsign of the station.
    pub callsign: String,

    /// The frequency the station is tuned to.
    pub frequency: Frequency,
}

/// Station state information.
///
/// Contains the current state of a monitored radio station, including its frequency,
/// transmission/reception status, and audio settings.
///
/// Emitted in response to [`Command::GetStationState`], [`Command::SetStationState`],
/// [`Command::AddStation`] and [`Command::ChangeStationVolume`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationState {
    /// The callsign of the station.
    ///
    /// When adding a station, this will be the callsign added (as for most other requests).
    ///
    /// When manually tuning a frequency (not available via API), `callsign` will be `None`. All
    /// later updates will have the callsign `Some("MANUAL")` for manually tuned frequencies.
    pub callsign: Option<String>,

    /// Whether the station is available (found in the VATSIM audio database). If `false`, all
    /// other information will be `None`.
    pub is_available: bool,

    /// The frequency the station is tuned to.
    ///
    /// When adding a station, this value is only available if the station was found and
    /// successfully added.
    ///
    /// When manually tuning a frequency (not available via API), this will be the frequency added,
    /// but its `callsign` will be `None`.
    pub frequency: Option<Frequency>,

    /// Whether the station is routed to the headset audio device only (`true`) or output to both
    /// speaker and headset (`false`).
    pub headset: Option<bool>,

    /// Whether the station's audio output is muted.
    pub is_output_muted: Option<bool>,

    /// The station's audio output volume level in the range 0..=100.
    pub output_volume: Option<f32>,

    /// Whether the station is set to receive (RX).
    pub rx: Option<bool>,

    /// Whether the station is set to transmit (TX).
    pub tx: Option<bool>,

    /// Whether the station has cross-couple (XC) enabled.
    pub xc: Option<bool>,

    /// Whether the station has cross-couple across (XCA) enabled.
    pub xca: Option<bool>,
}

/// Information about a manually tuned frequency that was removed.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FrequencyRemoved {
    /// The frequency that was removed.
    pub frequency: Frequency,
}

/// Envelope structure for station state updates.
///
/// Used internally by TrackAudio to wrap individual station state updates with type information.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StationStateEnvelope {
    /// The message type identifier (should always be "kStationStateUpdate").
    #[serde(rename = "type")]
    pub msg_type: String,

    /// The station state data.
    pub value: StationState,
}

/// Collection of all monitored station states.
///
/// Emitted in response to [`Command::GetStationState`] queries.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StationStates {
    pub stations: Vec<StationStateEnvelope>,
}

/// Transmission begin event payload.
///
/// Currently contains no additional data. The event itself indicates that
/// the local user has started transmitting.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TxBegin {}

/// Transmission end event payload.
///
/// Currently contains no additional data. The event itself indicates that
/// the local user has stopped transmitting.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TxEnd {}

/// Reception begin event payload.
///
/// Indicates that a remote station has started transmitting on a monitored frequency.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RxBegin {
    /// The callsign of the station that started transmitting.
    pub callsign: String,

    /// The frequency on which the transmission is occurring.
    #[serde(rename = "pFrequencyHz")]
    pub frequency: Frequency,
}

/// Reception end event payload.
///
/// Indicates that a remote station has stopped transmitting on a monitored frequency.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RxEnd {
    /// The callsign of the station that stopped transmitting.
    pub callsign: String,

    /// The frequency on which the transmission was occurring.
    #[serde(rename = "pFrequencyHz")]
    pub frequency: Frequency,

    /// List of callsigns still transmitting on this frequency, if any.
    ///
    /// Used to handle cases of simultaneous transmissions on the same frequency.
    #[serde(default, rename = "activeTransmitters")]
    pub active_transmitters: Option<Vec<String>>,
}

/// Main volume change event payload.
///
/// Indicates that the main volume level has been adjusted.
///
/// Emitted in response to [`Command::ChangeMainVolume`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MainVolumeChange {
    /// The main audio volume level in the range 0..=100.
    pub volume: f32,
}

/// Deprecated frequency state update payload.
///
/// # Deprecated
///
/// This payload is deprecated by TrackAudio. Use [`StationState`] instead.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[allow(dead_code)]
#[deprecated(
    since = "0.1.0",
    note = "This payload is deprecated by TrackAudio. Use StationState instead."
)]
pub struct FrequencyStateUpdate {
    /// Stations currently set to receive.
    #[allow(deprecated)]
    rx: Vec<FrequencyState>,

    /// Stations currently set to transmit.
    #[allow(deprecated)]
    tx: Vec<FrequencyState>,

    /// Stations currently set to cross-couple.
    #[allow(deprecated)]
    xc: Vec<FrequencyState>,
}

/// Deprecated frequency state information.
///
/// # Deprecated
///
/// This payload is deprecated by TrackAudio. Use [`StationState`] instead.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[deprecated(
    since = "0.1.0",
    note = "This payload is deprecated by TrackAudio. Use StationState instead."
)]
pub struct FrequencyState {
    /// The callsign of the station.
    #[serde(rename = "pCallsign")]
    pub callsign: String,

    /// The frequency the station is tuned to.
    #[serde(rename = "pFrequencyHz")]
    pub frequency: Frequency,
}

/// Client-side event variants.
///
/// These events are generated locally by the TrackAudio client and do not originate
/// from the TrackAudio instance. They represent client-side state changes or errors.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    /// The client has been disconnected from TrackAudio.
    Disconnected {
        /// The reason for the disconnection.
        reason: String,
    },

    /// A command failed to send to TrackAudio.
    CommandSendFailed {
        /// The command that failed to send.
        command: Command,

        /// The error message describing the failure.
        error: String,
    },

    /// An event from TrackAudio could not be deserialized.
    EventDeserializationFailed {
        /// The raw JSON string that failed to parse.
        raw: String,

        /// The error message describing the deserialization failure.
        error: String,
    },
}
