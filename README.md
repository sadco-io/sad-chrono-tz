# sad-chrono-tz

[![Build Status][gh-image]][gh-checks]
[![crates.io][cratesio-image]][cratesio]
[![Documentation][docsrs-image]][docsrs]
![Apache2/MIT licensed][license-image]
![MSRV][msrv-image]

[gh-image]: https://github.com/sadco-io/sad-chrono-tz/actions/workflows/test.yml/badge.svg?branch=main
[gh-checks]: https://github.com/sadco-io/sad-chrono-tz/actions/workflows/test.yml?query=branch%3Amain
[cratesio-image]: https://img.shields.io/crates/v/sad-chrono-tz.svg
[cratesio]: https://crates.io/crates/sad-chrono-tz
[docsrs-image]: https://docs.rs/sad-chrono-tz/badge.svg
[docsrs]: https://docs.rs/sad-chrono-tz
[license-image]: https://img.shields.io/badge/license-Apache2.0/MIT-blue.svg
[msrv-image]: https://img.shields.io/badge/rustc-1.66+-blue.svg

IANA timezone implementations for [`sad-chrono`](https://github.com/sadco-io/sad-chrono): a
**maintained, API-compatible fork of [chrono-tz][upstream]** with current IANA tzdb data (`2026f`)
and security and robustness fixes across the runtime, build-time codegen, and zoneinfo parser.

Upstream `chronotope/chrono-tz` has been archived and unmaintained
([RUSTSEC-2026-0555][rustsec-ctz], [RUSTSEC-2026-0559][rustsec-ctzb]), leaving its last release
(`0.10.4`) seven IANA tzdb releases behind (`2025b`). `sad-chrono-tz` is maintained by the Sadco
Security Team alongside [sad-chrono](https://github.com/sadco-io/sad-chrono) and
[sad-rsa](https://github.com/sadco-io/sad-rsa).

## Workspace Crates

| Crate | Version | License | Purpose |
|---|---|---|---|
| `sad-chrono-tz` | `0.10.5` | `MIT OR Apache-2.0` | Precompiled IANA `Tz` enum & `TimeZone` impls for `sad-chrono 0.4` |
| `sad-chrono-tz-build` | `0.5.1` | `MIT OR Apache-2.0` | Build script codegen (used by `filter-by-regex` and `case-insensitive`) |
| `sad-parse-zoneinfo` | `0.5.1` | `MIT` | Textual IANA zoneinfo parser and transition table builder |

## Usage

### Drop-in renamed usage (recommended for migrating existing `chrono-tz` code)

To switch existing `chrono` + `chrono-tz` code without changing `use chrono::...` or
`use chrono_tz::...` paths, rename both dependencies:

```toml
[dependencies]
chrono = { package = "sad-chrono", version = "0.4" }
chrono-tz = { package = "sad-chrono-tz", version = "0.10" }
```

### Direct usage

```toml
[dependencies]
sad-chrono = "0.4"
sad-chrono-tz = "0.10"
```

```rust
use sad_chrono::{TimeZone, Utc};
use sad_chrono_tz::US::Pacific;

let pacific_time = Pacific.with_ymd_and_hms(1990, 5, 6, 12, 30, 45).unwrap();
let utc_time = pacific_time.with_timezone(&Utc);
assert_eq!(utc_time, Utc.with_ymd_and_hms(1990, 5, 6, 19, 30, 45).unwrap());
```

### Type identity (`sad-chrono 0.4` vs `0.5` vs upstream `chrono`)

`sad-chrono 0.4`, `sad-chrono 0.5`, and upstream `chrono 0.4` are **three distinct crates** to
`rustc`. A `TimeZone` implementation must depend on the exact same `sad-chrono` major/minor line
that your application uses:

* **`sad-chrono-tz 0.10.x`** targets **`sad-chrono = "0.4"`** (MSRV `1.66.0`) and preserves
  `chrono-tz 0.10.4`'s exact public API (`impl Default for Tz`, `pub trait OffsetComponents`,
  `OffsetName::abbreviation(&self) -> Option<&str>`).
* If you depend on upstream `chrono` instead, `sad_chrono_tz::Tz` will not satisfy
  `chrono::TimeZone` directly (the compiler will report `` `Tz` implements similarly named trait
  `sad_chrono::offset::TimeZone` ``).

### Transitive dependencies (`[patch.crates-io]`)

Cargo `[patch]` matches both package name and compatible semver version, so renaming
`chrono-tz = { package = "sad-chrono-tz", ... }` in `[patch.crates-io]` is ignored. Applications
that pull upstream `chrono-tz` transitively through third-party crates (and cannot patch every
dependency's `Cargo.toml`) can vendor a release tag and set its package names to match upstream,
alongside vendoring `sad-chrono 0.4.x` as `chrono`. Note that `[patch.crates-io]` only applies at
the top-level binary workspace.

## Compatibility with `chrono-tz 0.10.4`

`sad-chrono-tz 0.10.5` is a strict non-breaking drop-in for `chrono-tz 0.10.4` targeting
`sad-chrono 0.4`:

* **Public API**: Identical to `chrono-tz 0.10.4` (`Tz`, `TZ_VARIANTS`, `IANA_TZDB_VERSION`,
  `ParseError`, `TzOffset`, `OffsetName`, `OffsetComponents`, `Default for Tz`, `GapInfo`, and
  all continent/region module constants).
* **IANA tzdb `2026f`**: Updated from `2025b` to `2026f` (same 597 variants: 0 added, 0 removed),
  including 2026 rule updates for Cuba (`America/Havana`), Manitoba (`America/Winnipeg`), Alberta
  (`America/Edmonton`), British Columbia (`America/Vancouver`), Northwest Territories
  (`America/Inuvik`), Morocco & Western Sahara (`Africa/Casablanca`, `Africa/El_Aaiun`), Moldova
  (`Europe/Chisinau`), and POSIX `USback` rules (`EST5EDT`, `CST6CDT`, `MST7MDT`, `PST8PDT`).
* **Non-breaking upstream improvements included**: `%z` numeric offset support in
  `sad-parse-zoneinfo` (`0.5.1`), link deduplication in `sad-chrono-tz-build`, `CHRONO_TZ_TIMEZONE_FILTER`
  regex-filtering fix (no longer pulls in unrelated `Etc/GMT*`/`UTC` zones when filtering for a
  single zone), `serde_core` dependency (`serde` feature), `phf 0.13`, MSRV `1.66.0`, and optional
  `defmt` (`1.0.1`) support.
* **Packaging**: Symlinked `LICENSE` files replaced with real `LICENSE-MIT` and `LICENSE-APACHE`
  files in every crate.
* **Environment variable**: `CHRONO_TZ_TIMEZONE_FILTER` works identically when `filter-by-regex`
  is enabled.

## Security and Robustness Fixes

| Crate | Issue Class | Upstream Behavior | `sad-chrono-tz` Fix |
|---|---|---|---|
| `sad-parse-zoneinfo` | Large year CPU DoS & `i64` overflow | `O(\|year - 1970\|)` loop in `ChangeTime::to_timestamp` hangs on extreme years | Closed-form `O(1)` `i128` Gregorian calculation with saturating `i64` conversion |
| `sad-parse-zoneinfo` | Empty / out-of-range `DaySpec` panics | `.unwrap()` panic on empty strings, integer overflow, or `u8 as i8` wrap | Checked `i8` parsing enforcing `1..=31` via `Result::Err` |
| `sad-parse-zoneinfo` | `DaySpec` boundary & `Weekday` panics | `.unwrap()` panic at Jan/Dec boundaries and `% 7` panic on negative years | Checked `i16` year-boundary resolution and `i128` Euclidean arithmetic |
| `sad-parse-zoneinfo` | `Year::Minimum`/`Maximum` panics | `unreachable!()` panics in `ChangeTime` and `RuleInfo::applies_to_year` | Rejected in `Zone` `UNTIL` and handled gracefully in `RuleInfo`/`ChangeTime` |
| `sad-parse-zoneinfo` | Malformed `TimeSpec` signs & ranges | Accepted mixed sign combos and minutes/seconds `>= 60` | Sign restricted to leading `-`; minutes/seconds validated in `00..=59` |
| `sad-parse-zoneinfo` | Broken / cyclic / chained link panics | Unchecked map indexing panicked on broken or chained links | Iterative cycle-safe link resolution returning `Option::None` |
| `sad-parse-zoneinfo` | `%z` fractional-minute & overflow panics | `assert!(secs == 0)`, `-i64::MIN` overflow, and `u8` hour truncation | `i128` `unsigned_abs()` formatting supporting seconds and unbounded hours |
| `sad-parse-zoneinfo` | Missing ruleset / empty zoneset panics | Unchecked indexing and `.unwrap()`/`.expect()` in `transitions.rs` | Fallible `Option` propagation and `i128` / saturating arithmetic in `optimise()` |
| `sad-parse-zoneinfo` | Inline `#` comment & `>= 4`-segment bugs | `Rule::from_str` used `continue` on `#`; `structure.rs` emitted `/` in names | `break` on `#`; hierarchical slash-free tree building with empty-segment guards |
| `sad-chrono-tz-build` | Regex over-inclusion & codegen invariants | Single-zone filter pulled 47 zones; no offset bounds check; single-hop links | Exact regex filtering (`chrono-tz#218`), `|offset| < 86_400` & `i32` assertions, cycle-safe transitive links, `{:?}` escaping |
| `sad-chrono-tz` | Local span overflow & unbounded serde error | Unchecked `i64` add in `local_span`; unbounded input reflection in `SerdeError` | `saturating_add` in `local_span`; `SerdeError` input capped at `128` bytes (UTF-8 boundary safe) |

## Known Limitations

* Precomputed timezone transitions in `sad-parse-zoneinfo` cover years `1800..2100`; timestamps at
  or after `2100-01-01` use the final 2099 offset indefinitely.

## Crate Features

* `default`: Enables `std`.
* `std`: Enables `std::error::Error` for `ParseError`. Disable (`default-features = false`) for
  `#![no_std]` with zero `alloc` usage.
* `serde`: Enables `Serialize` and `Deserialize` for `Tz` via `serde_core`.
* `case-insensitive`: Enables `Tz::from_str_insensitive` (invokes `sad-chrono-tz-build` at build time).
* `filter-by-regex`: Filters compiled zones via `CHRONO_TZ_TIMEZONE_FILTER` (invokes `sad-chrono-tz-build`).
* `arbitrary`: Derives `arbitrary::Arbitrary` for `Tz`.
* `defmt`: Implements `defmt::Format` for `Tz`, `ParseError`, `TzOffset`, and `FixedTimespan`.

## Rust Version Requirements

MSRV is Rust **`1.66.0`** across `sad-chrono-tz`, `sad-chrono-tz-build`, and `sad-parse-zoneinfo`.

## License

* `sad-chrono-tz` and `sad-chrono-tz-build` are licensed under either of
  [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT), at your option.
* `sad-parse-zoneinfo` is licensed under the [MIT License](parse-zoneinfo/LICENSE-MIT).
* The IANA Time Zone Database files in `chrono-tz/tz/` are in the public domain.

See [NOTICE](NOTICE) for attribution and a summary of modifications, and [SECURITY.md](SECURITY.md)
to report a vulnerability.

[upstream]: https://github.com/chronotope/chrono-tz
[rustsec-ctz]: https://rustsec.org/advisories/RUSTSEC-2026-0555
[rustsec-ctzb]: https://rustsec.org/advisories/RUSTSEC-2026-0559
