use std::time::Duration;
use trackaudio::{ClientEvent, ConnectionState, Event, TrackAudioClient, TrackAudioConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TrackAudioConfig::default()
        .with_auto_reconnect(true)
        .with_max_reconnect_attempts(Some(10))
        .with_backoff_config(
            Duration::from_secs(1),  // Initial backoff
            Duration::from_secs(30), // Max backoff
            2.0,                     // Multiplier
        );

    println!("Connecting to TrackAudio with auto-reconnect enabled...");
    let client = TrackAudioClient::connect(config).await?;

    let mut events = client.subscribe();

    let event_task = tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if let Event::Client(ClientEvent::ConnectionStateChanged(state)) = event {
                match state {
                    ConnectionState::Connecting { attempt } => {
                        println!("[Event] Connecting (attempt {attempt})...");
                    }
                    ConnectionState::Connected => {
                        println!("[Event] Connected to TrackAudio!");
                    }
                    ConnectionState::Disconnected { reason } => {
                        println!("[Event] Disconnected: {reason}");
                    }
                    ConnectionState::Reconnecting {
                        attempt,
                        next_delay,
                    } => {
                        println!("[Event] Reconnecting (attempt {attempt}) in {next_delay:?}...");
                    }
                    ConnectionState::ReconnectFailed { attempts } => {
                        println!("[Event] Reconnection failed after {attempts} attempts");
                    }
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_secs(2)).await;

    println!("\nTo test reconnection:");
    println!("1. Stop TrackAudio and watch automatic reconnection attempts");
    println!("2. Restart TrackAudio to see successful reconnection");
    println!("3. Press Ctrl+C to exit\n");

    tokio::time::sleep(Duration::from_secs(5)).await;

    println!("Triggering manual reconnection...");
    client.reconnect().await?;

    event_task.await?;

    Ok(())
}
