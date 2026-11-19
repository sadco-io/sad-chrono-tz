# sad-parse-zoneinfo

[![Build Status][gh-image]][gh-checks]
[![crates.io][cratesio-image]][cratesio]
[![Documentation][docsrs-image]][docsrs]

[gh-image]: https://github.com/sadco-io/sad-chrono-tz/actions/workflows/test.yml/badge.svg?branch=main
[gh-checks]: https://github.com/sadco-io/sad-chrono-tz/actions/workflows/test.yml?query=branch%3Amain
[cratesio-image]: https://img.shields.io/crates/v/sad-parse-zoneinfo.svg
[cratesio]: https://crates.io/crates/sad-parse-zoneinfo
[docsrs-image]: https://docs.rs/sad-parse-zoneinfo/badge.svg
[docsrs]: https://docs.rs/sad-parse-zoneinfo

Rust library for reading the text files comprising the [zoneinfo database], which records time zone
changes and offsets across the world from multiple sources.

`sad-parse-zoneinfo` is a maintained, hardened fork of [`parse-zoneinfo`] (itself a fork of
[`zoneinfo_parse`] by Benjamin Sago). It is used by [`sad-chrono-tz`].

[iana]: https://www.iana.org/time-zones
[tz]: https://github.com/eggert/tz
[zoneinfo database]: https://en.wikipedia.org/wiki/Tz_database
[`zoneinfo_parse`]: https://crates.io/crates/zoneinfo_parse
[`parse-zoneinfo`]: https://crates.io/crates/parse-zoneinfo
[`sad-chrono-tz`]: https://crates.io/crates/sad-chrono-tz

## License

Licensed under the [MIT License](LICENSE-MIT). See [NOTICE](NOTICE) for attribution and
modifications.
