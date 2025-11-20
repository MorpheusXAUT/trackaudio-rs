use std::time::Duration;
use trackaudio::TrackAudioClient;
use trackaudio::messages::commands::AddStation;

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    let client = TrackAudioClient::connect_default().await?;
    println!("Connected to TrackAudio");

    let state = client
        .request(
            AddStation {
                callsign: "LOVV_CTR".to_string(),
            },
            Some(Duration::from_secs(1)),
        )
        .await?;
    println!("Station state: {state:?}");

    Ok(())
}
