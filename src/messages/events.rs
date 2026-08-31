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
use std::time::Duration;

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
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Voice connection state changed.
    ///
    /// Emitted when the connection to the voice server is established or lost.
    VoiceConnectedState(VoiceConnectedState),

    /// Station added.
    ///
    /// Emitted when a new station is successfully added to TrackAudio, e.g., as a response to
    /// [`Command::AddStation`].
    StationAdded(StationAdded),

    /// A (monitored) station's state has been updated.
    ///
    /// Emitted when any property of a station changes (e.g., rx/tx/xc state, volume, etc.), or
    /// after a station is added or removed from the instance. Emitted as a response to
    /// [`Command::AddStation`], including the info whether the station was found.
    StationStateUpdate(StationState),

    /// An (unassociated) Frequency has been removed.
    ///
    /// Emitted when a manually tuned frequency (without a station) is removed from TrackAudio.
    FrequencyRemoved(FrequencyRemoved),

    /// Full state snapshot of all stations.
    ///
    /// Emitted as a response to [`Command::GetStationState`], containing a list of all stations
    /// currently monitored by TrackAudio.
    StationStates(StationStates),

    /// Transmission started on one or more frequencies.
    ///
    /// Emitted when the user begins transmitting (either by pressing their PTT button or as
    /// a response to a [`Command::PttPressed`]).
    TxBegin(TxBegin),

    /// Transmission ended on one or more frequencies.
    ///
    /// Emitted when the user finishes transmitting (either by releasing their PTT button or as
    /// a response to [`Command::PttReleased`]).
    TxEnd(TxEnd),

    /// Started receiving transmission on one or more frequencies.
    ///
    /// Emitted when another station begins transmitting on a monitored frequency.
    RxBegin(RxBegin),

    /// Stopped receiving transmission on one or more frequencies.
    ///
    /// Emitted when another station stops transmitting on a monitored frequency. Contains a list of
    /// stations still transmitting on frequency (in the case of simultaneous transmissions).
    RxEnd(RxEnd),

    /// The main volume level changed.
    ///
    /// Emitted when the user adjusts the main volume (either by using the volume slider in the
    /// client or as a response to [`Command::ChangeMainVolume`]).
    MainVolumeChange(MainVolumeChange),

    /// Frequency state update (deprecated).
    ///
    /// # Deprecated
    ///
    /// This event is deprecated by TrackAudio and only emitted for backwards
    /// compatibility. Use [`Event::StationStateUpdate`] instead.
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
    Client(ClientEvent),

    /// Unknown or unrecognized event type.
    ///
    /// Used as a fallback for forward compatibility: an event whose `type` this crate does not
    /// know is captured here rather than failing to deserialize, so a newer TrackAudio release
    /// cannot break an older client. The raw message is preserved so consumers can log or
    /// inspect it.
    ///
    /// Note that this only covers *unrecognized* event types. An event with a known type but a
    /// malformed payload is still a deserialization error, surfaced as
    /// [`ClientEvent::EventDeserializationFailed`].
    Unknown {
        /// The raw `type` field of the message, e.g. `"kSomeFutureEvent"`.
        msg_type: String,

        /// The raw `value` field of the message, or [`serde_json::Value::Null`] if absent.
        value: serde_json::Value,
    },
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// The `{ "type": ..., "value": ... }` envelope every TrackAudio message uses.
        #[derive(Deserialize)]
        struct Envelope {
            #[serde(rename = "type")]
            msg_type: String,

            #[serde(default)]
            value: serde_json::Value,
        }

        /// Decodes an event's `value` payload, tagging the error with the message type so a
        /// malformed payload is distinguishable from an unrelated parse failure.
        fn payload<T, E>(msg_type: &str, value: serde_json::Value) -> Result<T, E>
        where
            T: serde::de::DeserializeOwned,
            E: serde::de::Error,
        {
            // TrackAudio sends `"value": {}` for payload-less events such as `kTxBegin`, but a
            // stricter sender might omit it entirely. Treat that as an empty object so those
            // events still decode; payloads that do need fields still fail with a missing-field
            // error rather than a confusing type error.
            let value = if value.is_null() {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                value
            };

            serde_json::from_value(value)
                .map_err(|err| E::custom(format_args!("invalid `value` for {msg_type}: {err}")))
        }

        let Envelope { msg_type, value } = Envelope::deserialize(deserializer)?;

        Ok(match msg_type.as_str() {
            "kVoiceConnectedState" => Self::VoiceConnectedState(payload(&msg_type, value)?),
            "kStationAdded" => Self::StationAdded(payload(&msg_type, value)?),
            "kStationStateUpdate" => Self::StationStateUpdate(payload(&msg_type, value)?),
            "kFrequencyRemoved" => Self::FrequencyRemoved(payload(&msg_type, value)?),
            "kStationStates" => Self::StationStates(payload(&msg_type, value)?),
            "kTxBegin" => Self::TxBegin(payload(&msg_type, value)?),
            "kTxEnd" => Self::TxEnd(payload(&msg_type, value)?),
            "kRxBegin" => Self::RxBegin(payload(&msg_type, value)?),
            "kRxEnd" => Self::RxEnd(payload(&msg_type, value)?),
            "kMainVolumeChange" => Self::MainVolumeChange(payload(&msg_type, value)?),
            #[allow(deprecated)]
            "kFrequencyStateUpdate" => Self::FrequencyStateUpdate(payload(&msg_type, value)?),
            _ => Self::Unknown { msg_type, value },
        })
    }
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
#[serde(rename_all = "camelCase")]
pub struct StationAdded {
    /// The callsign of the station.
    ///
    /// # Notes
    ///
    /// - TrackAudio 1.4.0 renamed the station monitoring 122.800 MHz from `UNICOM` to `ADVISORY`.
    ///   Consumers matching on this callsign need to handle both spellings to stay compatible
    ///   across TrackAudio versions.
    pub callsign: String,

    /// The frequency the station is tuned to.
    pub frequency: Frequency,

    /// The station's alias frequency, if it has one.
    ///
    /// Alias frequencies are used for HF stations, where the frequency the controller is tuned to
    /// differs from the one published to pilots. Only sent by TrackAudio when the station actually
    /// has an alias.
    #[serde(default)]
    pub frequency_alias: Option<Frequency>,
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
    ///
    /// # Notes
    ///
    /// - TrackAudio 1.4.0 renamed the station monitoring 122.800 MHz from `UNICOM` to `ADVISORY`.
    ///   Consumers matching on this callsign need to handle both spellings to stay compatible
    ///   across TrackAudio versions.
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

    /// List of callsigns currently transmitting on this frequency, if any.
    ///
    /// Used to handle cases of simultaneous transmissions on the same frequency. Includes the
    /// station reported by [`callsign`](Self::callsign).
    ///
    /// # Notes
    ///
    /// - TrackAudio's SDK documentation omits this field for `kRxBegin`, but it has been sent
    ///   alongside `kRxEnd` since at least TrackAudio 1.3.3. As of 1.4.0, `kRxBegin` is only
    ///   emitted when TrackAudio has the list available, so this is expected to be `Some` in
    ///   practice.
    #[serde(default, rename = "activeTransmitters")]
    pub active_transmitters: Option<Vec<String>>,
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

/// Reason for disconnection from TrackAudio.
#[derive(Debug, Clone, PartialEq)]
pub enum DisconnectReason {
    /// User requested shutdown.
    Shutdown,

    /// User requested manual reconnection.
    ManualReconnect,

    /// Failed to send ping to keep connection alive.
    PingFailed(String),

    /// Failed to send command over WebSocket.
    CommandSendFailed(String),

    /// Failed to send pong response.
    PongFailed(String),

    /// WebSocket connection was closed by the peer.
    ClosedByPeer {
        /// Close frame code and reason, if provided.
        code: Option<u16>,
        reason: Option<String>,
    },

    /// WebSocket error occurred.
    WebSocketError(String),

    /// WebSocket stream ended unexpectedly.
    StreamEnded,

    /// Initial connection failed.
    ConnectionFailed(String),
}

impl std::fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shutdown => write!(f, "Shutdown requested"),
            Self::ManualReconnect => write!(f, "Manual reconnection requested"),
            Self::PingFailed(err) => write!(f, "Failed to send ping: {err}"),
            Self::CommandSendFailed(err) => write!(f, "Failed to send command: {err}"),
            Self::PongFailed(err) => write!(f, "Failed to send pong: {err}"),
            Self::ClosedByPeer { code, reason } => {
                write!(f, "WebSocket closed by peer")?;
                if let Some(code) = code {
                    write!(f, " (code: {code})")?;
                }
                if let Some(reason) = reason {
                    if !reason.is_empty() {
                        write!(f, ": {reason}")?;
                    }
                }
                Ok(())
            }
            Self::WebSocketError(err) => write!(f, "WebSocket error: {err}"),
            Self::StreamEnded => write!(f, "WebSocket stream ended unexpectedly"),
            Self::ConnectionFailed(err) => write!(f, "Connection failed: {err}"),
        }
    }
}

/// Connection state of the TrackAudio client.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    /// The client is attempting to connect to TrackAudio.
    Connecting {
        /// The connection attempt number (1-indexed).
        attempt: usize,
    },

    /// The client has successfully connected to TrackAudio.
    Connected,

    /// The client has been disconnected from TrackAudio.
    Disconnected {
        /// The reason for the disconnection.
        reason: DisconnectReason,
    },

    /// The client is attempting to reconnect to TrackAudio.
    Reconnecting {
        /// The reconnection attempt number (1-indexed).
        attempt: usize,

        /// The delay before the next reconnection attempt.
        next_delay: Duration,
    },

    /// The client has exhausted all reconnection attempts.
    ReconnectFailed {
        /// The total number of reconnection attempts made.
        attempts: usize,
    },
}

/// Client-side event variants.
///
/// These events are generated locally by the TrackAudio client and do not originate
/// from the TrackAudio instance. They represent client-side state changes or errors.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    /// The connection state has changed.
    ConnectionStateChanged(ConnectionState),

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

#[cfg(test)]
mod tests {
    //! Deserialization tests using payloads taken verbatim from TrackAudio's `backend/src/sdk.cpp`.
    //!
    //! TrackAudio emits an identical WebSocket wire format on 1.3.x and 1.4.0, so the samples below
    //! apply to both. Fields TrackAudio only sends conditionally are modelled as `Option`, and no
    //! payload struct uses `deny_unknown_fields`, so older instances (omitting a field) and newer
    //! ones (adding a field) both keep deserializing.

    use super::*;

    mod rx_begin {
        use super::{Event, Frequency};

        #[test]
        fn with_active_transmitters() {
            let json = r#"{"type":"kRxBegin","value":{"callsign":"AFR001","pFrequencyHz":123000000,"activeTransmitters":["AFR001","DLH456"]}}"#;

            let Event::RxBegin(rx) = serde_json::from_str(json).unwrap() else {
                panic!("expected Event::RxBegin");
            };

            assert_eq!(rx.callsign, "AFR001");
            assert_eq!(rx.frequency, Frequency::from_hz(123_000_000));
            assert_eq!(
                rx.active_transmitters,
                Some(vec!["AFR001".to_string(), "DLH456".to_string()])
            );
        }

        #[test]
        fn without_active_transmitters() {
            let json =
                r#"{"type":"kRxBegin","value":{"callsign":"AFR001","pFrequencyHz":123000000}}"#;

            let Event::RxBegin(rx) = serde_json::from_str(json).unwrap() else {
                panic!("expected Event::RxBegin");
            };

            assert_eq!(rx.callsign, "AFR001");
            assert_eq!(rx.active_transmitters, None);
        }
    }

    mod station_added {
        use super::{Event, Frequency};

        #[test]
        fn with_frequency_alias() {
            let json = r#"{"type":"kStationAdded","value":{"callsign":"EDDF_S_TWR","frequency":118775000,"frequencyAlias":128775000}}"#;

            let Event::StationAdded(station) = serde_json::from_str(json).unwrap() else {
                panic!("expected Event::StationAdded");
            };

            assert_eq!(station.callsign, "EDDF_S_TWR");
            assert_eq!(station.frequency, Frequency::from_hz(118_775_000));
            assert_eq!(
                station.frequency_alias,
                Some(Frequency::from_hz(128_775_000))
            );
        }

        #[test]
        fn without_frequency_alias() {
            let json =
                r#"{"type":"kStationAdded","value":{"callsign":"ADVISORY","frequency":122800000}}"#;

            let Event::StationAdded(station) = serde_json::from_str(json).unwrap() else {
                panic!("expected Event::StationAdded");
            };

            assert_eq!(station.callsign, "ADVISORY");
            assert_eq!(station.frequency, Frequency::from_hz(122_800_000));
            assert_eq!(station.frequency_alias, None);
        }
    }

    /// Guards the properties that keep a single client working across TrackAudio versions.
    mod version_compatibility {
        use super::{Event, Frequency, StationState};

        /// A station monitoring 122.800 MHz is called `UNICOM` up to 1.3.x and `ADVISORY` from
        /// 1.4.0 onwards. Both must deserialize; the callsign is passed through untouched so
        /// consumers can match on either.
        #[test]
        fn unicom_and_advisory_callsigns_both_deserialize() {
            for callsign in ["UNICOM", "ADVISORY"] {
                let json = format!(
                    r#"{{"type":"kStationStateUpdate","value":{{"callsign":"{callsign}","isAvailable":true,"frequency":122800000}}}}"#
                );

                let Event::StationStateUpdate(state) = serde_json::from_str(&json).unwrap() else {
                    panic!("expected Event::StationStateUpdate");
                };

                assert_eq!(state.callsign.as_deref(), Some(callsign));
                assert_eq!(state.frequency, Some(Frequency::from_hz(122_800_000)));
            }
        }

        /// Older instances omit fields newer ones send. Every such field is an `Option`, so a
        /// minimal payload must still deserialize rather than erroring.
        #[test]
        fn missing_optional_fields_deserialize_as_none() {
            let json = r#"{"type":"kStationStateUpdate","value":{"isAvailable":false}}"#;

            let Event::StationStateUpdate(state) = serde_json::from_str(json).unwrap() else {
                panic!("expected Event::StationStateUpdate");
            };

            assert_eq!(
                state,
                StationState {
                    callsign: None,
                    is_available: false,
                    frequency: None,
                    headset: None,
                    is_output_muted: None,
                    output_volume: None,
                    rx: None,
                    tx: None,
                    xc: None,
                    xca: None,
                }
            );
        }

        /// Newer instances may add fields this crate does not model yet. No payload struct uses
        /// `deny_unknown_fields`, so they must be ignored rather than failing the whole event.
        #[test]
        fn unknown_fields_are_ignored() {
            let json = r#"{"type":"kRxBegin","value":{"callsign":"AFR001","pFrequencyHz":123000000,"activeTransmitters":["AFR001"],"somethingNew":{"rco":true}}}"#;

            let Event::RxBegin(rx) = serde_json::from_str(json).unwrap() else {
                panic!("expected Event::RxBegin");
            };

            assert_eq!(rx.callsign, "AFR001");
            assert_eq!(rx.frequency, Frequency::from_hz(123_000_000));
        }

        /// Message types this crate does not know fall back to [`Event::Unknown`] instead of
        /// erroring, so a newer TrackAudio cannot break an older client. The raw message is
        /// preserved so consumers can log what they received.
        #[test]
        fn unknown_event_types_fall_back() {
            let json = r#"{"type":"kSomeFutureEvent","value":{"whatever":1}}"#;

            assert_eq!(
                serde_json::from_str::<Event>(json).unwrap(),
                Event::Unknown {
                    msg_type: "kSomeFutureEvent".to_string(),
                    value: serde_json::json!({"whatever": 1}),
                }
            );
        }

        /// Payload-less events decode even when `value` is absent or null, not just when it is
        /// the empty object TrackAudio actually sends.
        #[test]
        fn payload_less_events_tolerate_missing_value() {
            for json in [
                r#"{"type":"kTxBegin","value":{}}"#,
                r#"{"type":"kTxBegin","value":null}"#,
                r#"{"type":"kTxBegin"}"#,
            ] {
                assert_eq!(
                    serde_json::from_str::<Event>(json).unwrap(),
                    Event::TxBegin(super::TxBegin {}),
                    "failed for {json}"
                );
            }
        }

        /// A payload that genuinely needs fields still reports them as missing rather than as a
        /// confusing type error when `value` is absent.
        #[test]
        fn missing_value_for_a_real_payload_reports_missing_field() {
            let err = serde_json::from_str::<Event>(r#"{"type":"kRxBegin"}"#)
                .unwrap_err()
                .to_string();
            assert!(err.contains("missing field"), "unexpected error: {err}");
        }

        /// An unknown event without a `value` is still recognized, with a null payload.
        #[test]
        fn unknown_event_types_without_value() {
            for json in [
                r#"{"type":"kSomeFutureEvent"}"#,
                r#"{"type":"kSomeFutureEvent","value":null}"#,
            ] {
                assert_eq!(
                    serde_json::from_str::<Event>(json).unwrap(),
                    Event::Unknown {
                        msg_type: "kSomeFutureEvent".to_string(),
                        value: serde_json::Value::Null,
                    }
                );
            }
        }

        /// A *known* event type with a malformed payload must still be an error, so real protocol
        /// breakage surfaces as [`ClientEvent::EventDeserializationFailed`] rather than being
        /// silently swallowed into [`Event::Unknown`].
        #[test]
        fn known_event_with_bad_payload_still_errors() {
            let json = r#"{"type":"kRxBegin","value":{"callsign":42}}"#;

            let err = serde_json::from_str::<Event>(json).unwrap_err().to_string();
            assert!(
                err.contains("kRxBegin"),
                "error should name the message type: {err}"
            );
        }
    }
}
