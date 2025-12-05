# trackaudio

[![Crates.io](https://img.shields.io/crates/v/trackaudio.svg)](https://crates.io/crates/trackaudio)
[![Documentation](https://docs.rs/trackaudio/badge.svg)](https://docs.rs/trackaudio)
[![License: Apache-2.0 OR MIT](https://img.shields.io/badge/License-Apache--2.0%20OR%20MIT-blue.svg)](#license)

A Rust client library for [TrackAudio](https://github.com/pierr3/TrackAudio), a modern voice communication application for [VATSIM](https://vatsim.ent) air traffic controllers.

This crate provides a high-level, async API for controlling TrackAudio programmatically via its [WebSocket interface](https://github.com/pierr3/TrackAudio/wiki/SDK-documentation), enabling custom integrations, automation tools, and alternative user interfaces.

## Features

- 🚀 **Async/await API** – Built on Tokio for efficient async I/O
- 🔒 **Type-safe** – Strongly-typed commands and events with full deserialization
- 🔄 **Request-response pattern** – High-level API for commands that expect responses
- 📡 **Event streaming** – Subscribe to real-time events from TrackAudio
- 🔌 **Automatic reconnection** – Resilient WebSocket connections with exponential backoff
- 🧵 **Thread-safe** – Client can be safely shared across threads
- 🔍 **Tracing support** – Optional integration with the `tracing` crate

## Quick start

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
trackaudio = "0.1"
```

Connect to the default instance locally and listen for events:

```rust
use trackaudio::{Command, Event, TrackAudioClient};

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    // Connect to the local TrackAudio instance (ws://127.0.0.1:49080/ws)
    let client = TrackAudioClient::connect_default().await?;

    // Subscribe to event stream
    let mut events = client.subscribe();

    // Send a command
    client.send(Command::PttPressed).await?;

    // Handle incoming events
    while let Ok(event) = events.recv().await {
        match event {
            Event::TxBegin(_) => println!("Started transmitting"),
            Event::RxBegin(rx) => println!("Receiving from {}", rx.callsign),
            _ => {}
        }
    }

    Ok(())
}
```

## High-level API

For common operations, use the `TrackAudioApi` wrapper:

```rust
use std::time::Duration;
use trackaudio::TrackAudioClient;

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    let client = TrackAudioClient::connect_default().await?;
    let api = client.api();

    // Add a station and wait for TrackAudio's response
    let station = api.add_station("LOVV_CTR", Some(Duration::from_secs(5))).await?;
    println!("Added station: {station:?}");

    // Change main volume
    let vol = api.change_main_volume(-20, None).await?;
    println!("Volume changed to {vol}");

    Ok(())
}
```

- Use `TrackAudioClient` if you want full control (raw events, raw commands).
- Use `TrackAudioApi` if you want convenience with timeout-guarded request/response helpers.

For more detailed examples and API docs, check out the [documentation](https://docs.rs/trackaudio).

## Examples

### Adding and configuring stations

```rust
use trackaudio::{Command, Frequency, TrackAudioClient};
use trackaudio::messages::commands::{BoolOrToggle, SetStationState};
use std::time::Duration;

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    let client = TrackAudioClient::connect_default().await?;
    let api = client.api();

    // Add a station
    let station = api.add_station("LOVV_CTR", Some(Duration::from_secs(5))).await?;

    if station.is_available {
        println!("✓ Station available on {}", station.frequency.unwrap());

        // Enable RX and TX on the station
        client.send(Command::SetStationState(SetStationState {
            frequency: station.frequency.unwrap(),
            rx: Some(BoolOrToggle::Bool(true)),
            tx: Some(BoolOrToggle::Bool(true)),
            xca: None,
            is_output_muted: None,
            headset: None,
        })).await?;
    } else {
        println!("✗ Station not available");
    }

    Ok(())
}
```

### Working with frequencies

```rust
use trackaudio::Frequency;

// Create frequencies in different units
let freq_hz = Frequency::from_hz(132_600_000);
let freq_khz = Frequency::from_khz(132_600);
let freq_mhz = Frequency::from_mhz(132.600);

assert_eq!(freq_hz, freq_khz);
assert_eq!(freq_khz, freq_mhz);

// Convert between units
println!("Frequency: {} MHz", freq_hz.as_mhz());
println!("Frequency: {} kHz", freq_hz.as_khz());
println!("Frequency: {} Hz", freq_hz.as_hz());

// Convenient conversions
let freq: Frequency = 132.600_f64.into();  // from MHz
let freq: Frequency = 132_600_000_u64.into();  // from Hz
```

### Monitoring RX/TX activity

```rust
use trackaudio::{Event, TrackAudioClient};

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    let client = TrackAudioClient::connect_default().await?;
    let mut events = client.subscribe();

    println!("Monitoring radio activity...");

    while let Ok(event) = events.recv().await {
        match event {
            Event::RxBegin(rx) => {
                println!("📻 RX Start: {} on {}", rx.callsign, rx.frequency);
            }
            Event::RxEnd(rx) => {
                println!("📻 RX End: {} on {}", rx.callsign, rx.frequency);
                if let Some(active) = rx.active_transmitters {
                    if !active.is_empty() {
                        println!("   Still transmitting: {}", active.join(", "));
                    }
                }
            }
            Event::TxBegin(_) => {
                println!("🎙️  TX Start");
            }
            Event::TxEnd(_) => {
                println!("🎙️  TX End");
            }
            Event::StationStateUpdate(state) => {
                println!("📡 Station update: {}", state.callsign);
            }
            _ => {}
        }
    }

    Ok(())
}
```

### Request-Response pattern

```rust
use trackaudio::TrackAudioClient;
use trackaudio::messages::commands::{GetStationState, GetStationStates};
use std::time::Duration;

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    let client = TrackAudioClient::connect_default().await?;

    // Request a specific station's state
    let station = client
        .request(
            GetStationState {
                callsign: "LOVV_CTR".to_string(),
            },
            Some(Duration::from_secs(5)),
        )
        .await?;

    println!("Station: {:?}", station);

    // Request all station states
    let states = client
        .request(GetStationStates, Some(Duration::from_secs(5)))
        .await?;

    println!("Total stations: {}", states.len());

    Ok(())
}
```

### Custom Configuration

```rust
use trackaudio::{TrackAudioClient, TrackAudioConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    let config = TrackAudioConfig::new("192.168.1.69:49080")?
        .with_capacity(512, 512)  // Increase channel capacities
        .with_ping_interval(Duration::from_secs(10));  // Adjust keepalive

    let client = TrackAudioClient::connect(config).await?;

    // Use client...

    Ok(())
}
```

### Reconnection Configuration

The client automatically reconnects on connection loss with exponential backoff:

```rust
use trackaudio::{TrackAudioClient, TrackAudioConfig, ConnectionState, ClientEvent, Event};
use std::time::Duration;

#[tokio::main]
async fn main() -> trackaudio::Result<()> {
    let config = TrackAudioConfig::default()
        .with_auto_reconnect(true)             // Enabled by default
        .with_max_reconnect_attempts(Some(10)) // None = infinite retries
        .with_backoff_config(
            Duration::from_secs(1),            // Initial backoff
            Duration::from_secs(60),           // Max backoff
            2.0,                               // Multiplier
        );

    let client = TrackAudioClient::connect(config).await?;

    // Monitor connection state changes
    let mut events = client.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if let Event::Client(ClientEvent::ConnectionStateChanged(state)) = event {
                match state {
                    ConnectionState::Disconnected { reason } => {
                        println!("Disconnected: {reason}");
                    }
                    ConnectionState::Reconnecting { attempt, next_delay } => {
                        println!("Reconnecting (attempt {attempt}) in {next_delay:?}...");
                    }
                    ConnectionState::Connected => {
                        println!("Connected!");
                    }
                    _ => {}
                }
            }
        }
    });

    // Manually trigger reconnection if needed
    client.reconnect().await?;

    Ok(())
}
```

## Supported URL Formats

The client supports flexible URL formats for connecting to TrackAudio:

- Full WebSocket URL: `ws://127.0.0.1:49080/ws` or `ws://192.168.1.69/ws`
- Host only: `127.0.0.1` or `localhost` (uses default port 49080 and `/ws` path)
- Host and port: `127.0.0.1:12345` (uses `/ws` path)

**Note:** TrackAudio currently only supports IPv4 connections and the `ws://` scheme (no TLS/`wss://` support).

## API Overview

### Core Types

- **`TrackAudioClient`** – The core WebSocket client for connection management
- **`TrackAudioApi`** – High-level API wrapper with convenient methods
- **`TrackAudioConfig`** – Configuration for connection parameters
- **`Command`** – Commands to send to TrackAudio
- **`Event`** – Events emitted by TrackAudio
- **`Frequency`** – Radio frequency with convenient unit conversions

## Error Handling

All operations return a `Result<T, TrackAudioError>`:

```rust
use trackaudio::{TrackAudioClient, TrackAudioError};

match TrackAudioClient::connect_default().await {
    Ok(client) => {
        // Use client
    }
    Err(TrackAudioError::WebSocket(e)) => {
        eprintln!("Connection failed: {}", e);
    }
    Err(TrackAudioError::Timeout) => {
        eprintln!("Operation timed out");
    }
    Err(TrackAudioError::InvalidUrl(msg)) => {
        eprintln!("Invalid URL: {}", msg);
    }
    Err(e) => {
        eprintln!("Error: {}", e);
    }
}
```

## Features

- **`tracing`** (enabled by default) – Integrate with the [`tracing`](https://docs.rs/tracing) crate for structured logging
- **`reconnect-jitter`** (enabled by default) – Add jitter to reconnection backoff to prevent thundering herd

To disable default features:

```toml
[dependencies]
trackaudio = { version = "0.1", default-features = false }
```

## Minimum Supported Rust Version (MSRV)

This crate requires Rust 1.85 or later.

## License

The `trackaudio` project and all its crates and packages are dual-licensed as

- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE) or https://opensource.org/license/apache-2-0)
- **MIT license** ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

This means you can choose to use `trackaudio` under either the Apache-2.0 license or the MIT license.

### Contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Contributing

Contributions are welcome! Whether you've found a bug, have a feature request, or want to improve the documentation, your input is valued.

### Reporting Issues

If you encounter a bug or have a feature request, please [open an issue](https://github.com/MorpheusXAUT/trackaudio-rs/issues) on GitHub. When reporting a bug, please include:

- A clear description of the problem
- Steps to reproduce the issue
- The version of this crate used
- Your TrackAudio version
- Your Rust version (`rustc --version`)
- Your operating system
- Any relevant error messages or logs

### Submitting Pull Requests

1. Fork the repository and create a feature branch
2. Write tests for new functionality (if applicable)
3. Ensure `cargo test`, `cargo fmt`, and `cargo clippy` pass
4. Update documentation for API changes
5. Submit your PR with a clear description of the changes

Note that this project uses [Conventional Commits](https://www.conventionalcommits.org) in combination with [release-plz](https://release-plz.dev/) for automatic [semantic versioning](https://semver.org/) and release automation.  
Please ensure your commits follow this format when submitting a PR.

For significant changes, please open an issue first to discuss your proposal.

## Resources

- [TrackAudio GitHub](https://github.com/pierr3/TrackAudio)
- [TrackAudio SDK Documentation](https://github.com/pierr3/TrackAudio/wiki/SDK-documentation)
- [VATSIM](https://www.vatsim.net/)
- [Documentation](https://docs.rs/trackaudio)

## Acknowledgments

This is an unofficial client library. TrackAudio is developed and maintained by [Pierre](https://github.com/pierr3).
