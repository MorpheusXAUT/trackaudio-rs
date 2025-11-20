use trackaudio::TrackAudioClient;

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    let client = TrackAudioClient::connect_default().await?;
    println!("Connected to TrackAudio");

    let api = client.api();

    let volume = api.change_main_volume(-50, None).await?;
    println!("Volume changed to: {volume}");

    Ok(())
}
