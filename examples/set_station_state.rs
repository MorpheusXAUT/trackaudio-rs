use std::time::Duration;
use trackaudio::messages::commands::SetStationState;
use trackaudio::{Frequency, TrackAudioClient};

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    let client = TrackAudioClient::connect_default().await?;
    println!("Connected to TrackAudio");

    let state = client
        .request(
            SetStationState {
                frequency: Frequency::from_mhz(121.500),
                rx: Some(true.into()),
                tx: None,
                xca: None,
                headset: None,
                is_output_muted: None,
            },
            Some(Duration::from_secs(1)),
        )
        .await?;
    println!("Station state: {state:?}");

    Ok(())
}
