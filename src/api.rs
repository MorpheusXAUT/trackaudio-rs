use crate::messages::commands::{
    AddStation, ChangeMainVolume, ChangeStationVolume, GetStationState, GetStationStates,
    GetVoiceConnectedState,
};
use crate::messages::events::StationState;
use crate::{Command, Event, TrackAudioClient};
use std::fmt::Display;
use std::time::Duration;

/// A high-level client for interacting with the TrackAudio WebSocket API.
///
/// [`TrackAudioApi`] acts as a wrapper around the [`TrackAudioClient`], providing a higher-level
/// interface for interacting with TrackAudio, abstracting some of the asynchronous command- and
/// event-based architecture of the WebSocket API.
pub struct TrackAudioApi<'a> {
    client: &'a TrackAudioClient,
}

impl TrackAudioClient {
    /// Creates a higher-level [`TrackAudioApi`] from the current [`TrackAudioClient`] instance.
    ///
    /// # Returns
    /// - `TrackAudioApi`: API instance tied to the lifetime of `self`.
    pub fn api(&self) -> TrackAudioApi<'_> {
        TrackAudioApi::new(self)
    }
}

impl<'a> TrackAudioApi<'a> {
    /// Creates a higher-level [`TrackAudioApi`] from the given [`TrackAudioClient`] instance.
    /// - `TrackAudioApi`: API instance tied to the lifetime of `client`.
    pub fn new(client: &'a TrackAudioClient) -> Self {
        Self { client }
    }

    /// Transmits a Push-To-Talk (PTT) command and awaits a corresponding event as a confirmation.
    ///
    /// This function sends a [`Command::PttPressed`]/[`Command::PttReleased`] command depending on
    /// the provided `active` flag to activate or deactivate transmission of audio and waits for a
    /// corresponding [`Event::TxBegin`] or [`Event::TxEnd`] event as confirmation.
    ///
    /// # Parameters
    /// - `active`: A boolean indicating whether to activate or deactivate transmission
    /// - `timeout`: An optional `Duration` to specify the maximum time to wait
    ///   for the confirmation event. If `None`, the function will wait indefinitely.
    ///
    /// # Returns
    /// - `Ok(())`: After receiving the expected confirmation event.
    /// - `Err(TrackAudioError)`: An error if the operation failed or exceeded the provided timeout.
    ///
    /// # Errors
    /// - [`TrackAudioError::Timeout`](crate::TrackAudioError::Timeout): If the operation times out.
    /// - [`TrackAudioError::Send`](crate::TrackAudioError::Send): If an error occurs while sending the command.
    /// - [`TrackAudioError::Receive`](crate::TrackAudioError::Receive): If an error occurs while receiving events.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self), err))]
    pub async fn transmit(&self, active: bool, timeout: Option<Duration>) -> crate::Result<()> {
        let cmd = if active {
            Command::PttPressed
        } else {
            Command::PttReleased
        };
        self.client
            .send_and_await(cmd, timeout, move |event| match event {
                Event::TxBegin(_) if active => Some(()),
                Event::TxEnd(_) if !active => Some(()),
                _ => None,
            })
            .await
    }

    /// Adds a new station with the provided callsign and returns its state.
    ///
    /// This function sends a [`Command::AddStation`] command to request the addition of
    /// a new station identified by the given `callsign`. It waits for a corresponding
    /// [`Event::StationStateUpdate`] event that matches the callsign and retrieves
    /// the updated station state.
    ///
    /// # Parameters
    /// - `callsign`: Callsign of the station to be added, e.g. `"LOVV_CTR"`.
    /// - `timeout`: An optional `Duration` to specify the maximum time to wait
    ///   for the response. If `None`, the function will wait indefinitely.
    ///
    /// # Returns
    /// - `Ok(StationState)`: The state of the newly added station if the operation
    ///   was successful.
    /// - `Err(TrackAudioError)`: An error if the operation failed or exceeded the provided timeout.
    ///
    /// # Errors
    /// - [`TrackAudioError::Timeout`](crate::TrackAudioError::Timeout): If the operation times out.
    /// - [`TrackAudioError::Send`](crate::TrackAudioError::Send): If an error occurs while sending the command.
    /// - [`TrackAudioError::Receive`](crate::TrackAudioError::Receive): If an error occurs while receiving events.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self), fields(callsign = %callsign), err))]
    pub async fn add_station(
        &self,
        callsign: impl Into<String> + Display,
        timeout: Option<Duration>,
    ) -> crate::Result<StationState> {
        self.client
            .request(
                AddStation {
                    callsign: callsign.into(),
                },
                timeout,
            )
            .await
    }

    /// Changes the volume of a station and returns its updated state.
    ///
    /// This function sends a [`Command::ChangeStationVolume`] command to modify the volume of
    /// an existing station identified by the given `frequency_hz`. It waits for a corresponding
    /// [`Event::StationStateUpdate`] event that matches the frequency and retrieves
    /// the updated station state.
    ///
    /// # Parameters
    /// - `frequency_hz`: Callsign of the station to be added, e.g. `132_600_000`.
    /// - `amount`: The amount to adjust the volume by, relative to the current value. This amount
    ///   is in the range -100..=100, the resulting volume will be clamped to 0..=100.
    /// - `timeout`: An optional `Duration` to specify the maximum time to wait
    ///   for the response. If `None`, the function will wait indefinitely.
    ///
    /// # Returns
    /// - `Ok(StationState)`: The state of the updated station if the operation was successful.
    /// - `Err(TrackAudioError)`: An error if the operation failed or exceeded the provided timeout.
    ///
    /// # Errors
    /// - [`TrackAudioError::Timeout`](crate::TrackAudioError::Timeout): If the operation times out.
    /// - [`TrackAudioError::Send`](crate::TrackAudioError::Send): If an error occurs while sending the command.
    /// - [`TrackAudioError::Receive`](crate::TrackAudioError::Receive): If an error occurs while receiving events.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self), err))]
    pub async fn change_station_volume(
        &self,
        frequency_hz: u64,
        amount: i8,
        timeout: Option<Duration>,
    ) -> crate::Result<StationState> {
        self.client
            .request(ChangeStationVolume::new(frequency_hz, amount), timeout)
            .await
    }

    /// Changes the main volume and returns its updated state.
    ///
    /// This function sends a [`Command::ChangeMainVolume`] command to modify the volume of
    /// the main audio output. It waits for a corresponding [`Event::MainVolumeChange`] event
    /// and retrieves the updated main output volume.
    ///
    /// # Parameters
    /// - `amount`: The amount to adjust the volume by, relative to the current value. This amount
    ///   is in the range -100..=100, the resulting volume will be clamped to 0..=100.
    /// - `timeout`: An optional `Duration` to specify the maximum time to wait
    ///   for the response. If `None`, the function will wait indefinitely.
    ///
    /// # Returns
    /// - `Ok(f32)`: The updated main volume if the operation was successful.
    /// - `Err(TrackAudioError)`: An error if the operation failed or exceeded the provided timeout.
    ///
    /// # Errors
    /// - [`TrackAudioError::Timeout`](crate::TrackAudioError::Timeout): If the operation times out.
    /// - [`TrackAudioError::Send`](crate::TrackAudioError::Send): If an error occurs while sending the command.
    /// - [`TrackAudioError::Receive`](crate::TrackAudioError::Receive): If an error occurs while receiving events.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self), err))]
    pub async fn change_main_volume(
        &self,
        amount: i8,
        timeout: Option<Duration>,
    ) -> crate::Result<f32> {
        self.client
            .request(ChangeMainVolume::new(amount), timeout)
            .await
    }

    /// Retrieves the current state of a specific station.
    ///
    /// This function sends a [`Command::GetStationState`] command to request the current status of
    /// a station identified by the given `callsign`. It waits for a corresponding
    /// [`Event::StationStateUpdate`] event that matches the callsign and retrieves
    /// the updated station state.
    ///
    /// # Parameters
    /// - `callsign`: Callsign of the station to retrieve the state for, e.g. `"LOVV_CTR"`.
    /// - `timeout`: An optional `Duration` to specify the maximum time to wait
    ///   for the response. If `None`, the function will wait indefinitely.
    ///
    /// # Returns
    /// - `Ok(StationState)`: The state of the station if the operation was successful.
    /// - `Err(TrackAudioError)`: An error if the operation failed or exceeded the provided timeout.
    ///
    /// # Errors
    /// - [`TrackAudioError::Timeout`](crate::TrackAudioError::Timeout): If the operation times out.
    /// - [`TrackAudioError::Send`](crate::TrackAudioError::Send): If an error occurs while sending the command.
    /// - [`TrackAudioError::Receive`](crate::TrackAudioError::Receive): If an error occurs while receiving events.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self), fields(callsign = %callsign), err))]
    pub async fn get_station_state(
        &self,
        callsign: impl Into<String> + Display,
        timeout: Option<Duration>,
    ) -> crate::Result<StationState> {
        self.client
            .request(
                GetStationState {
                    callsign: callsign.into(),
                },
                timeout,
            )
            .await
    }

    /// Retrieves the current state of all active stations.
    ///
    /// This function sends a [`Command::GetStationStates`] command to request the current status of
    /// all active stations. It waits for a corresponding [`Event::StationStates`] event
    /// and returns the updated station state.
    ///
    /// # Parameters
    /// - `timeout`: An optional `Duration` to specify the maximum time to wait
    ///   for the response. If `None`, the function will wait indefinitely.
    ///
    /// # Returns
    /// - `Ok(Vec<StationState>)`: The state of all active stations if the operation was successful.
    /// - `Err(TrackAudioError)`: An error if the operation failed or exceeded the provided timeout.
    ///
    /// # Errors
    /// - [`TrackAudioError::Timeout`](crate::TrackAudioError::Timeout): If the operation times out.
    /// - [`TrackAudioError::Send`](crate::TrackAudioError::Send): If an error occurs while sending the command.
    /// - [`TrackAudioError::Receive`](crate::TrackAudioError::Receive): If an error occurs while receiving events.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self), err))]
    pub async fn get_station_states(
        &self,
        timeout: Option<Duration>,
    ) -> crate::Result<Vec<StationState>> {
        self.client.request(GetStationStates, timeout).await
    }

    /// Retrieves the current voice connection state.
    ///
    /// This function sends a [`Command::GetVoiceConnectedState`] command to request the current
    /// voice connection state. It waits for a corresponding [`Event::VoiceConnectedState`] event
    /// and returns the current state.
    ///
    /// # Parameters
    /// - `timeout`: An optional `Duration` to specify the maximum time to wait
    ///   for the response. If `None`, the function will wait indefinitely.
    ///
    /// # Returns
    /// - `Ok(bool)`: The current voice connection state if the operation was successful.
    /// - `Err(TrackAudioError)`: An error if the operation failed or exceeded the provided timeout.
    ///
    /// # Errors
    /// - [`TrackAudioError::Timeout`](crate::TrackAudioError::Timeout): If the operation times out.
    /// - [`TrackAudioError::Send`](crate::TrackAudioError::Send): If an error occurs while sending the command.
    /// - [`TrackAudioError::Receive`](crate::TrackAudioError::Receive): If an error occurs while receiving events.
    pub async fn get_voice_connected_state(
        &self,
        timeout: Option<Duration>,
    ) -> crate::Result<bool> {
        self.client.request(GetVoiceConnectedState, timeout).await
    }
}
