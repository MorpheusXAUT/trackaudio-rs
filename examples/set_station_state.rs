use std::time::Duration;
use trackaudio::messages::commands::SetStationState;
use trackaudio::{Frequency, TrackAudioClient};

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    let client = TrackAudioClient::connect_default().await?;
    println!("Connected to TrackAudio");

    let state = client
        .api()
        .set_station_state(
            SetStationState::new(Frequency::from_mhz(121.500)).rx(true),
            Some(Duration::from_secs(1)),
        )
        .await?;
    println!("Station state: {state:?}");

    Ok(())
}
