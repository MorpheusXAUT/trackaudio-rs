# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/MorpheusXAUT/trackaudio-rs/compare/v0.2.2...v0.3.0)

### Breaking changes


- [**breaking**] Carry the raw message in Event::Unknown - ([a6b6e8a](https://github.com/MorpheusXAUT/trackaudio-rs/commit/a6b6e8aebf1a84315ad2092382057a1d0f491096))

### Features


- Let callers observe and await the connection state ([#70](https://github.com/MorpheusXAUT/trackaudio-rs/pull/70)) - ([cda9e59](https://github.com/MorpheusXAUT/trackaudio-rs/commit/cda9e591978fb5f5842d6eea5dd9b8c8e8c6501c))
- Add activeTransmitters and frequencyAlias to event payloads - ([76a69f7](https://github.com/MorpheusXAUT/trackaudio-rs/commit/76a69f7cb9f0854b3bccb6ca981191012478b04e))

### Bug Fixes


- Tolerate an absent value on payload-less events - ([2e26a7f](https://github.com/MorpheusXAUT/trackaudio-rs/commit/2e26a7fbc8156a72dc2316350f9bad585dd02403))
- Resolve add_station from kStationAdded - ([98118bf](https://github.com/MorpheusXAUT/trackaudio-rs/commit/98118bfd995dc59ae41167f4cc86d51388400a14))
- Compile with the tracing feature disabled - ([834ec69](https://github.com/MorpheusXAUT/trackaudio-rs/commit/834ec692af81a8d2c2e667d3bc897bd7e7a2b76c))

### Documentation


- Correct and tighten docs, drop the README version pin - ([a4279b9](https://github.com/MorpheusXAUT/trackaudio-rs/commit/a4279b9e7f053e5faa47b9c34af7d97c8b91cce1))
- Add security policy ([#67](https://github.com/MorpheusXAUT/trackaudio-rs/pull/67)) - ([269938a](https://github.com/MorpheusXAUT/trackaudio-rs/commit/269938a5f076435f41709bc1a4d7d66fe2667bb5))


## [0.2.2](https://github.com/MorpheusXAUT/trackaudio-rs/compare/v0.2.1...v0.2.2)

### Features


- Add set_station_state to API ([#34](https://github.com/MorpheusXAUT/trackaudio-rs/pull/34)) - ([41e325c](https://github.com/MorpheusXAUT/trackaudio-rs/commit/41e325c239e34328f28719ef892f3d57fdd8261c))


## [0.2.1](https://github.com/MorpheusXAUT/trackaudio-rs/compare/v0.2.0...v0.2.1)

### Bug Fixes


- Fix multiple reconnects blocking in case of connection failures ([#11](https://github.com/MorpheusXAUT/trackaudio-rs/pull/11)) - ([31de86b](https://github.com/MorpheusXAUT/trackaudio-rs/commit/31de86bebe21c605e015eebfd7c3601ebb8e8b5b))


## [0.2.0](https://github.com/MorpheusXAUT/trackaudio-rs/compare/v0.1.0...v0.2.0)

### Features


- Add auto reconnect ([#9](https://github.com/MorpheusXAUT/trackaudio-rs/pull/9)) - ([20acf6f](https://github.com/MorpheusXAUT/trackaudio-rs/commit/20acf6fbe39105226f43eb087117b188484d2d49))

