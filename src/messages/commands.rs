//! Commands for controlling TrackAudio.
//!
//! This module contains all commands that can be sent to TrackAudio to control radio behavior,
//! manage stations, and adjust audio settings.
//!
//! # Overview
//!
//! Commands are sent to TrackAudio as JSON messages via its WebSocket API and typically trigger
//! corresponding [`Event`]s in response. The main [`Command`] enum contains all available commands
//! including associated payload structs for commands that require additional data.
//!
//! # Request pattern
//!
//! Some commands (their respective payloads, to be precise) implement the [`Request`] trait,
//! allowing them to be used in a request-response pattern where you can await the corresponding
//! event response.
//!
//! # Examples
//!
//! ```rust
//! use trackaudio::Command;
//! use trackaudio::messages::commands::AddStation;
//!
//! // Simple command without a payload
//! let cmd = Command::PttPressed;
//!
//! // Command with payload
//! let cmd = Command::AddStation(AddStation {
//!     callsign: "LOVV_CTR".to_string(),
//! });
//! ```
//!
//! # External documentation
//!
//! For more details on TrackAudio's command protocol, see the
//! [SDK documentation](https://github.com/pierr3/TrackAudio/wiki/SDK-documentation#incoming-messages).

use crate::messages::Request;
use crate::messages::events::StationState;
use crate::{Event, Frequency};
use serde::Serialize;

/// Commands sent to the TrackAudio instance.
///
/// These messages are sent to TrackAudio to control its behavior and change the state programmatically.
///
/// # Serialization
///
/// Events are serialized to JSON strings sent to TrackAudio with a `type` field indicating the
/// variant name and a `value` field containing the variant's data.
///
/// # Notes
///
/// - TrackAudio's incoming messages SDK documentation can be found on
///   [GitHub](https://github.com/pierr3/TrackAudio/wiki/SDK-documentation#incoming-messages).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum Command {
    /// Start transmission by pressing the push-to-talk button.
    ///
    /// Initiates transmission on all active (TX/XC enabled) frequencies. Should be followed
    /// by [`Command::PttReleased`] when transmission ends.
    /// TrackAudio emits [`Event::TxBegin`] when the transmission starts.
    #[serde(rename = "kPttPressed")]
    PttPressed,

    /// End transmission by releasing the push-to-talk button.
    ///
    /// Stops a transmission started with [`Command::PttPressed`].
    /// TrackAudio emits [`Event::TxEnd`] when the transmission ends.
    #[serde(rename = "kPttReleased")]
    PttReleased,

    /// Add a new station to monitor.
    ///
    /// Tries to add a station with the specified callsign, allowing the user to receive and transmit
    /// on the associated frequency.
    /// TrackAudio emits [`Event::StationStateUpdate`], indicating success or failure adding the station.
    #[serde(rename = "kAddStation")]
    AddStation(AddStation),

    /// Update an existing station's state.
    ///
    /// Modifies properties of a station such as its rx/tx state, cross-couple mode, or other
    /// configuration settings.
    /// TrackAudio emits [`Event::StationStateUpdate`] with the updated state.
    #[serde(rename = "kSetStationState")]
    SetStationState(SetStationState),

    /// Adjust the volume of a specific station.
    ///
    /// Changes the audio level for a single station's received transmissions.
    /// TrackAudio emits [`Event::StationStateUpdate`] with the updated state.
    #[serde(rename = "kChangeStationVolume")]
    ChangeStationVolume(ChangeStationVolume),

    /// Adjust the main output volume.
    ///
    /// Changes the main volume level for all audio output.
    /// TrackAudio emits [`Event::MainOutputVolumeChange`] with the updated volume.
    #[serde(rename = "kChangeMainOutputVolume")]
    ChangeMainOutputVolume(ChangeMainOutputVolume),

    /// Request the current main output volume.
    ///
    /// Queries TrackAudio for the current main volume level.
    /// TrackAudio emits [`Event::MainOutputVolumeChange`] with the updated volume.
    #[serde(rename = "kGetMainOutputVolume")]
    GetMainOutputVolume,

    /// Request the state of a specific station.
    ///
    /// Queries TrackAudio for the current configuration and station of a station.
    /// TrackAudio emits [`Event::StationStateUpdate`] with the current state.
    #[serde(rename = "kGetStationState")]
    GetStationState(GetStationState),

    /// Request the state of all monitored stations.
    ///
    /// Queries the complete state of all configured stations.
    /// TrackAudio emits [`Event::StationStates`] with all station data.
    #[serde(rename = "kGetStationStates")]
    GetStationStates,
}

/// Payload for adding a new station.
///
/// Specifies the callsign of the station to add. TrackAudio will look up the associated frequency
/// and transmitters for this callsign and monitor it accordingly.
///
/// This command implements the [`Request`] trait and can be used in a request-response pattern.
///
/// Furthermore, the [`TrackAudioApi`](crate::TrackAudioApi) provides a convenience wrapper
/// [`add_station`](crate::TrackAudioApi::add_station), which can be used to easily add a station
/// and await the associated response from TrackAudio.
/// # Examples
///
/// ```rust
/// use trackaudio::messages::commands::AddStation;
///
/// let cmd = AddStation {
///     callsign: "LOVV_CTR".to_string(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AddStation {
    /// Callsign of the station to add (e.g., "LOVV_CTR").
    pub callsign: String,
}

impl Request for AddStation {
    type Response = StationState;

    fn into_command(self) -> Command {
        Command::AddStation(self)
    }

    fn extract(event: &Event, cmd: &Command) -> Option<Self::Response> {
        let callsign = match cmd {
            Command::AddStation(AddStation { callsign }) => callsign,
            _ => return None,
        };
        match event {
            Event::StationStateUpdate(state) if &state.callsign == callsign => Some(state.clone()),
            _ => None,
        }
    }
}

/// Payload for updating a station's state.
///
/// The station to update is identified by its `frequency` (in Hz/Hertz). All other fields are
/// optional and will not be changed if omitted.
/// Use [`BoolOrToggle::Toggle`] to toggle a boolean field without knowing its current state.
///
/// This command implements the [`Request`] trait and can be used in a request-response pattern.
///
/// # Examples
///
/// ```rust
/// use trackaudio::Frequency;
/// use trackaudio::messages::commands::{BoolOrToggle, SetStationState};
///
/// let cmd = SetStationState {
///     frequency: Frequency::from_mhz(132.600), // 132.600 MHz
///     rx: Some(BoolOrToggle::Bool(true)), // enable RX
///     tx: Some(BoolOrToggle::Bool(true)), // enable TX
///     xca: Some(BoolOrToggle::Bool(false)), // disable XCA
///     is_output_muted: None,
///     headset: Some(BoolOrToggle::Toggle), // toggle headset output
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetStationState {
    /// Frequency of station to set state for in Hz (Hertz, e.g., 132_600_000 for 132.600 MHz)
    pub frequency: Frequency,

    /// Whether to mute the audio output of the station.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_output_muted: Option<BoolOrToggle>,

    /// Whether to receive (RX) on this frequency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx: Option<BoolOrToggle>,

    /// Whether to transmit (TX) on this frequency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx: Option<BoolOrToggle>,

    /// Whether cross-coupling (XCA) mode is enabled on this frequency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xca: Option<BoolOrToggle>,

    /// Whether to output audio to the configured headset device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headset: Option<BoolOrToggle>,
}

impl Request for SetStationState {
    type Response = StationState;

    fn into_command(self) -> Command {
        Command::SetStationState(self)
    }

    fn extract(event: &Event, cmd: &Command) -> Option<Self::Response> {
        let frequency = match cmd {
            Command::SetStationState(SetStationState { frequency, .. }) => frequency,
            _ => return None,
        };
        match event {
            Event::StationStateUpdate(state)
                if state.frequency.is_some_and(|f| f == *frequency) =>
            {
                Some(state.clone())
            }
            _ => None,
        }
    }
}

/// Payload for adjusting a station's volume.
///
/// The volume change is relative to the current volume level.
///
/// This command implements the [`Request`] trait and can be used in a request-response pattern.
///
/// Furthermore, the [`TrackAudioApi`](crate::TrackAudioApi) provides a convenience wrapper
/// [`change_station_volume`](crate::TrackAudioApi::change_station_volume), which can be used to
/// easily change an existing station's volume and await the associated response from TrackAudio.
///
/// # Examples
///
/// ```rust
/// use trackaudio::Frequency;
/// use trackaudio::messages::commands::ChangeStationVolume;
///
/// let cmd = ChangeStationVolume {
///     frequency: Frequency::from_mhz(132.600), // 132.600 MHz
///     amount: 50, // 50% volume increase
/// };
///
/// let cmd = ChangeStationVolume {
///     frequency: Frequency::from_mhz(132.600), // 132.600 MHz
///     amount: -100, // -100% volume increase (effectively mute)
/// };
///
/// let cmd = ChangeStationVolume::new(Frequency::from_mhz(132.600), 127);
/// assert_eq!(cmd.amount, 100); // clamped to 100
///
/// let cmd = ChangeStationVolume::new(132_600_000, -127);
/// assert_eq!(cmd.amount, -100); // clamped to -100
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChangeStationVolume {
    /// Frequency of station to change volume for in Hz (Hertz, e.g., 132_600_000 for 132.600 MHz)
    pub frequency: Frequency,

    /// Amount to adjust the volume by, in the range -100..=100.
    ///
    /// This is added to the current volume. The resulting volume will be clamped to the range 0..=100.
    pub amount: i8,
}

impl ChangeStationVolume {
    pub fn new(frequency: impl Into<Frequency>, amount: i8) -> Self {
        Self {
            frequency: frequency.into(),
            amount: amount.clamp(-100, 100),
        }
    }
}

impl Request for ChangeStationVolume {
    type Response = StationState;

    fn into_command(self) -> Command {
        Command::ChangeStationVolume(self)
    }

    fn extract(event: &Event, cmd: &Command) -> Option<Self::Response> {
        let frequency = match cmd {
            Command::ChangeStationVolume(ChangeStationVolume { frequency, .. }) => frequency,
            _ => return None,
        };
        match event {
            Event::StationStateUpdate(state)
                if state.frequency.is_some_and(|f| f == *frequency) =>
            {
                Some(state.clone())
            }
            _ => None,
        }
    }
}

/// Payload for adjusting the main output volume.
///
/// The volume change is relative to the current main volume level.
///
/// This command implements the [`Request`] trait and can be used in a request-response pattern.
///
/// Furthermore, the [`TrackAudioApi`](crate::TrackAudioApi) provides a convenience wrapper
/// [`change_main_output_volume`](crate::TrackAudioApi::change_main_output_volume), which can be
/// used to easily change the main output volume and await the associated response from TrackAudio.
///
/// # Examples
///
/// ```rust
/// use trackaudio::messages::commands::ChangeMainOutputVolume;
///
/// let cmd = ChangeMainOutputVolume {
///     amount: 50, // 50% volume increase
/// };
///
/// let cmd = ChangeMainOutputVolume {
///     amount: -100, // -100% volume increase (effectively mute)
/// };
///
/// let cmd = ChangeMainOutputVolume::new(127);
/// assert_eq!(cmd.amount, 100); // clamped to 100
///
/// let cmd = ChangeMainOutputVolume::new(-127);
/// assert_eq!(cmd.amount, -100); // clamped to -100
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChangeMainOutputVolume {
    /// Amount to adjust the volume by, in the range -100..=100.
    ///
    /// This is added to the current volume. The resulting volume will be clamped to the range 0..=100.
    pub amount: i8,
}

impl ChangeMainOutputVolume {
    pub fn new(amount: i8) -> Self {
        Self {
            amount: amount.clamp(-100, 100),
        }
    }
}

impl Request for ChangeMainOutputVolume {
    type Response = f32;

    fn into_command(self) -> Command {
        Command::ChangeMainOutputVolume(self)
    }

    fn extract(event: &Event, _cmd: &Command) -> Option<Self::Response> {
        match event {
            Event::MainOutputVolumeChange(change) => Some(change.volume),
            _ => None,
        }
    }
}

/// Payload for requesting a specific station's state.
///
/// This command implements the [`Request`] trait and can be used in a request-response pattern.
///
/// Furthermore, the [`TrackAudioApi`](crate::TrackAudioApi) provides a convenience wrapper
/// [`get_station_state`](crate::TrackAudioApi::get_station_state), which can be
/// used to easily request a station's state and await the associated response from TrackAudio.
///
/// # Examples
///
/// ```rust
/// use trackaudio::messages::commands::GetStationState;
///
/// let cmd = GetStationState {
///     callsign: "LOVV_CTR".to_string(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GetStationState {
    /// Callsign of station to request state for.
    pub callsign: String,
}

impl Request for GetStationState {
    type Response = StationState;

    fn into_command(self) -> Command {
        Command::GetStationState(self)
    }

    fn extract(event: &Event, cmd: &Command) -> Option<Self::Response> {
        let callsign = match cmd {
            Command::GetStationState(GetStationState { callsign }) => callsign,
            _ => return None,
        };
        match event {
            Event::StationStateUpdate(state) if &state.callsign == callsign => Some(state.clone()),
            _ => None,
        }
    }
}

/// Payload for requesting all station states.
///
/// This is a unit struct as the command requires no additional data.
///
/// This command implements the [`Request`] trait and can be used in a request-response pattern.
///
/// Furthermore, the [`TrackAudioApi`](crate::TrackAudioApi) provides a convenience wrapper
/// [`get_station_states`](crate::TrackAudioApi::get_station_states), which can be used to easily
/// request the state of all stations and await the associated response from TrackAudio.
///
/// # Examples
///
/// ```rust
/// use trackaudio::messages::commands::GetStationStates;
///
/// let cmd = GetStationStates;
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GetStationStates;

impl Request for GetStationStates {
    type Response = Vec<StationState>;

    fn into_command(self) -> Command {
        Command::GetStationStates
    }

    fn extract(event: &Event, _cmd: &Command) -> Option<Self::Response> {
        match event {
            Event::StationStates(states) => Some(
                states
                    .stations
                    .iter()
                    .map(|s| s.value.clone())
                    .collect::<_>(),
            ),
            _ => None,
        }
    }
}

/// A boolean value or a toggle instruction.
///
/// Used in station state updates to either set a specific boolean value or toggle the current state
/// without needing to know its current value. TrackAudio represents toggles as the string literal
/// `"toggle"` in its JSON messages.
///
/// # Examples
///
/// ```rust
/// use trackaudio::messages::commands::BoolOrToggle;
///
/// // Set to a specific bool value
/// let enabled = BoolOrToggle::Bool(false);
/// assert_eq!(enabled, BoolOrToggle::Bool(false));
///
/// // Toggle the current value
/// let toggle = BoolOrToggle::Toggle;
/// assert_eq!(toggle, BoolOrToggle::Toggle);
///
/// // Convert from bool
/// let from_bool: BoolOrToggle = true.into();
/// assert_eq!(from_bool, BoolOrToggle::Bool(true));
///
/// // Convert from Option<bool>
/// let from_opt_some: BoolOrToggle = Some(true).into();
/// assert_eq!(from_opt_some, BoolOrToggle::Bool(true));
///
/// let from_opt_none: BoolOrToggle = None.into();
/// assert_eq!(from_opt_none, BoolOrToggle::Toggle);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum BoolOrToggle {
    /// A specific boolean value.
    Bool(bool),
    /// Toggle the current state. Serializable as `"toggle"`.
    #[serde(rename = "toggle")]
    Toggle,
}

impl From<bool> for BoolOrToggle {
    fn from(b: bool) -> Self {
        BoolOrToggle::Bool(b)
    }
}

impl From<Option<bool>> for BoolOrToggle {
    fn from(opt: Option<bool>) -> Self {
        match opt {
            Some(b) => BoolOrToggle::Bool(b),
            None => BoolOrToggle::Toggle,
        }
    }
}

impl TryFrom<BoolOrToggle> for bool {
    type Error = ();
    fn try_from(b: BoolOrToggle) -> Result<Self, Self::Error> {
        match b {
            BoolOrToggle::Bool(b) => Ok(b),
            BoolOrToggle::Toggle => Err(()),
        }
    }
}

impl BoolOrToggle {
    pub fn is_toggle(&self) -> bool {
        matches!(self, BoolOrToggle::Toggle)
    }

    pub fn is_true(&self) -> bool {
        matches!(self, BoolOrToggle::Bool(true))
    }

    pub fn is_false(&self) -> bool {
        matches!(self, BoolOrToggle::Bool(false))
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            BoolOrToggle::Bool(b) => Some(*b),
            BoolOrToggle::Toggle => None,
        }
    }

    pub fn unwrap_or(&self, default: bool) -> bool {
        self.as_bool().unwrap_or(default)
    }
}
