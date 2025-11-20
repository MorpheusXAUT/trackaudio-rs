use trackaudio::{ClientEvent, Event, TrackAudioClient};

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    let client = TrackAudioClient::connect_default().await?;
    println!("Connected to TrackAudio");

    let mut events = client.subscribe();

    println!("Listening for events...");
    loop {
        match events.recv().await {
            Ok(Event::Client(ClientEvent::Disconnected { reason })) => {
                println!("Disconnected from TrackAudio: {reason}");
                break;
            }
            Ok(event) => println!("TrackAudio event: {event:?}"),
            Err(_) => {
                println!("Disconnected from TrackAudio");
                break;
            }
        }
    }

    Ok(())
}
