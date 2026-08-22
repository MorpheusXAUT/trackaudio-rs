# Security policy

## Supported versions

`trackaudio` is pre-1.0. Only the latest published version receives fixes; there are no maintained
release branches. Older versions are yanked from crates.io when a vulnerability affects them.

## Reporting a vulnerability

Please report security issues privately through
[GitHub's private vulnerability reporting](https://github.com/MorpheusXAUT/trackaudio-rs/security/advisories/new)
rather than opening a public issue.

Include what the issue is, which crate version and feature flags are affected, which TrackAudio
version you tested against, and how to reproduce it. You should get an initial response within a
week.

## Scope

Worth reporting:

- Anything a malicious or compromised WebSocket peer could exploit: panics, unbounded allocation,
  or infinite loops while deserializing TrackAudio messages or handling the event stream.
- Reconnect handling that a misbehaving endpoint can drive into a hot loop, bypassing the
  exponential backoff or the `reconnect-jitter` feature.
- A message sequence that makes the request-response correlation hand one caller's response to a
  different caller.
- Supply chain problems with the released crate: a published `.crate` whose contents do not match
  the tagged source.

Out of scope:

- **The TrackAudio SDK protocol has no authentication or encryption.** This library connects over
  plain `ws://` to a local instance, because that is all TrackAudio offers. Anything that can reach
  the SDK port can already control TrackAudio directly, without this crate. Keeping that port off
  untrusted networks is the deployment's job; do not report the protocol's properties as
  vulnerabilities of this crate.
- Denial of service against a TrackAudio instance by an application using this library. Any
  WebSocket client can do the same.
- Advisories in dependencies with no reachable path from this crate. Those are tracked by
  `cargo deny` in CI; open a normal issue instead.

## Release integrity

The crate is published to crates.io by GitHub Actions from a tagged commit, through Trusted
Publishing over OIDC, so no long-lived registry token exists. No binary artifacts are released.
