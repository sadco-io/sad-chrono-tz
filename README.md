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
[msrv-image]: https://img.shields.io/badge/rustc-1.85+-blue.svg

IANA timezone implementations for [`sad-chrono`](https://github.com/sadco-io/sad-chrono): a
**maintained fork of [chrono-tz][upstream]** (API-compatible for the verified runtime surface) with
current IANA tzdb data (`2026f`) and security and robustness fixes across the runtime, build-time
codegen, and zoneinfo parser.

Upstream `chronotope/chrono-tz` has been archived and unmaintained
([RUSTSEC-2026-0555][rustsec-ctz], [RUSTSEC-2026-0559][rustsec-ctzb]), leaving its last release
(`0.10.4`) seven IANA tzdb releases behind (`2025b`). `sad-chrono-tz` is maintained by the Sadco
Security Team alongside [sad-chrono](https://github.com/sadco-io/sad-chrono) and
[sad-rsa](https://github.com/sadco-io/sad-rsa).

## Which version?

* **`0.11.x`** (recommended, `main`): targets **`sad-chrono 0.5`** (`0.5.1+`), MSRV `1.85.0`,
  `phf 0.14`, `defmt 1.2`, `ParseError` implementing `core::error::Error` independently of `std`,
  IANA `2026f`.
* **`0.10.x`** (`0.10.x` maintenance branch, security & tzdb fixes only): targets
  **`sad-chrono 0.4`** (`0.4.48+`), MSRV `1.66.0` (with `defmt 1.0.1` requiring Rust `1.81+`),
  `phf 0.13`.

`sad-chrono-tz 0.11` is **`chrono-tz 0.10`'s verified runtime API with `sad-chrono 0.5` and
current dependencies**. It is not upstream's unreleased `chrono-tz 0.11` breaking API.

## Workspace Crates

| Crate | Version | License | Purpose |
|---|---|---|---|
| `sad-chrono-tz` | `0.11.1` | `MIT OR Apache-2.0` | Precompiled IANA `Tz` enum & `TimeZone` impls for `sad-chrono 0.5` (`0.5.1+`) |
| `sad-chrono-tz-build` | `0.6.1` | `MIT OR Apache-2.0` | Build script codegen (used by `filter-by-regex` and `case-insensitive`) |
| `sad-parse-zoneinfo` | `0.6.0` | `MIT` | Textual IANA zoneinfo parser and transition table builder |

## Usage

### Drop-in renamed usage (recommended for migrating existing `chrono-tz` code)

To switch existing `chrono` + `chrono-tz` code without changing `use chrono::...` or
`use chrono_tz::...` paths, rename both dependencies:

```toml
[dependencies]
chrono = { package = "sad-chrono", version = "0.5.1" }
chrono-tz = { package = "sad-chrono-tz", version = "0.11.1" }
```

### Direct usage

```toml
[dependencies]
sad-chrono = "0.5.1"
sad-chrono-tz = "0.11.1"
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

* **`sad-chrono-tz 0.11.x`** targets **`sad-chrono = "0.5.1"`** (MSRV `1.85.0`) and preserves
  `chrono-tz 0.10.4`'s verified runtime public API (`Default for Tz` when `UTC` is included,
  `pub trait OffsetComponents`, `OffsetName::abbreviation(&self) -> Option<&str>`).
* **`sad-chrono-tz 0.10.x`** targets **`sad-chrono = "0.4.48"`** (MSRV `1.66.0`, except `defmt`
  requiring Rust `1.81+`) with the same verified runtime API.
* If you depend on upstream `chrono` instead, `sad_chrono_tz::Tz` will not satisfy
  `chrono::TimeZone` directly (the compiler will report `` `Tz` implements similarly named trait
  `sad_chrono::offset::TimeZone` ``).

### Transitive dependencies (`[patch.crates-io]`)

This route uses the `0.10.x` line (MSRV `1.66.0` for non-`defmt` features) alongside
`sad-chrono 0.4.x`, because Cargo `[patch]` matches both package name and compatible semver
version (`[patch.crates-io] chrono-tz = { package = "sad-chrono-tz", ... }` and `0.11.x` copies
are ignored when crates depend on `chrono-tz = "^0.10"`). Applications that pull upstream
`chrono-tz` transitively through third-party crates (and cannot patch every dependency's
`Cargo.toml`) can vendor a `v0.10.*` release tag and set its package names to match upstream,
alongside vendoring `sad-chrono 0.4.x` as `chrono`. Note that `[patch.crates-io]` only applies at
the top-level binary workspace.

## Compatibility with `chrono-tz 0.10.4`

`sad-chrono-tz 0.11.0` is API-compatible with `chrono-tz 0.10.4` for the verified runtime surface
and supported feature configurations targeting `sad-chrono 0.5` (`0.5.1+`):

* **Public runtime API**: Verified against `chrono-tz 0.10.4` (`Tz`, `TZ_VARIANTS`,
  `IANA_TZDB_VERSION`, `ParseError`, `TzOffset`, `OffsetName`, `OffsetComponents`, `GapInfo`, and
  continent/region module constants). `Tz::default()` returns `Tz::UTC` when `UTC` is included in
  the compiled zone set (and is omitted if `filter-by-regex` excludes `UTC`).
* **Numeric `%z` abbreviations**: Preserves `chrono-tz 0.10.4` behavior where `%z`-derived numeric
  offsets (such as `Etc/GMT+1`, `Antarctica/Casey`, or post-2024 `America/Scoresbysund`) return
  `None` from `OffsetName::abbreviation()`, while `Display` formats the signed numeric offset
  (e.g. `"-01"`). Explicit abbreviations (including literal zone abbreviations such as `EST` or
  `-0330`) return `Some(...)`.
* **IANA tzdb `2026f`**: Updated from `2025b` to `2026f` (same 597 variants: 0 added, 0 removed),
  including 2026 rule updates for Cuba (`America/Havana`), Manitoba (`America/Winnipeg`), Alberta
  (`America/Edmonton`), British Columbia (`America/Vancouver`), Northwest Territories
  (`America/Inuvik`), Morocco & Western Sahara (`Africa/Casablanca`, `Africa/El_Aaiun`), Moldova
  (`Europe/Chisinau`), and POSIX `USback` rules (`EST5EDT`, `CST6CDT`, `MST7MDT`, `PST8PDT`).
* **Upstream improvements included**: `%z` formatting in `sad-parse-zoneinfo` (`0.6.0`, where
  `Format::format` takes three arguments as in `chrono-tz#211` / `parse-zoneinfo 0.5.0` vs two in
  `0.4.0`), link deduplication in `sad-chrono-tz-build`, `CHRONO_TZ_TIMEZONE_FILTER` regex filtering
  selecting only zones whose names match the regex (`chrono-tz#218`, no longer pulling in unrelated
  `Etc/GMT*`/`UTC` zones; use anchors such as `^...$` for whole-name matching, and an unset or
  blank value leaves all zones enabled), `serde_core` dependency (`serde` feature), `phf 0.14`, MSRV
  `1.85.0`, `core::error::Error` for `ParseError`, and optional `defmt` (`1.2`) support.
* **Packaging**: Symlinked `LICENSE` files replaced with real `LICENSE-MIT` and `LICENSE-APACHE`
  files (`sad-chrono-tz` and `sad-chrono-tz-build` carry `MIT` and `Apache-2.0` texts;
  `sad-parse-zoneinfo` carries `MIT`).
* **Environment variable**: When `filter-by-regex` is enabled, `CHRONO_TZ_TIMEZONE_FILTER` selects
  only zones whose names match the regex (use anchors such as `^...$` for whole-name matching; an
  unset or blank value leaves all zones enabled), with no implicit `UTC`/`Etc/GMT*` inclusion, and
  fails the build cleanly if zero zones match.

## For library authors

Libraries can switch with the same rename:

```toml
[dependencies]
chrono = { package = "sad-chrono", version = "0.5.1" }
chrono-tz = { package = "sad-chrono-tz", version = "0.11.1" }
```

Because the dependency keys stay `chrono` and `chrono-tz`, your `use chrono::...` and
`use chrono_tz::...` paths and feature names keep working. Enable every feature you use
explicitly: features of `chrono-tz` and `sad-chrono-tz` do not unify.

**Which line?** Depending on `sad-chrono-tz = "0.11"` requires Rust `1.85.0` and pairs with
`sad-chrono = "0.5.1"`. Libraries that must support Rust `< 1.85.0` or `sad-chrono = "0.4"` (for
example `rkyv 0.7` builds) can target `sad-chrono-tz = "0.10"` (`0.10.x` branch), noting that
`sad-chrono-tz 0.10` and `0.11` depend on distinct `sad-chrono` major/minor versions (`0.4` vs
`0.5`) whose types do not interoperate — moving a library's public API from `0.10` to `0.11` is a
breaking change.

**Is it a breaking change?** `sad_chrono_tz::Tz` and `chrono_tz::Tz` (as well as
`sad-chrono-tz 0.10` and `0.11`) implement `TimeZone` for distinct `sad-chrono`/`chrono` crates.
If `Tz`, `TzOffset`, or `chrono::DateTime<Tz>` appear in your public API, switching breaks callers
that pass upstream `chrono-tz` or `0.10` types. Release the switch as a new semver-major version,
or gate separate versions behind Cargo features. If you only use `chrono-tz` internally, switching
is not a breaking change.

## Stability and versioning

`sad-chrono-tz 0.11.x` keeps the verified runtime API surface of `chrono-tz 0.10.4` and will not
make semver-incompatible API changes within `0.11.x`. `0.11.0` deliberately leaves the `0.10`
version sequence because the dependency and MSRV upgrades (`sad-chrono 0.5.1`, `phf 0.14`,
`defmt 1.2`, MSRV `1.85.0`) are breaking changes — it is **not** upstream's unreleased
`chrono-tz 0.11` breaking API.

The `0.10.x` branch remains maintained for security and IANA tzdb fixes only (targeting
`sad-chrono 0.4` `0.4.48+`, MSRV `1.66.0` with `defmt 1.0.1` requiring `1.81+`, `phf 0.13`). Each
release's changelog states which IANA tzdb version and fixes it includes.

MSRV is Rust `1.85.0` for `0.11.x`. Within `0.11.x`, it is raised only with a changelog entry and
only if a dependency's security fix requires it.

## Security and Robustness Fixes

| Crate | Issue Class | Upstream Behavior | `sad-chrono-tz` Fix |
|---|---|---|---|
| `sad-parse-zoneinfo` | Large year CPU DoS & `i64` overflow | `O(\|year - 1970\|)` loop in `ChangeTime::to_timestamp` hangs on extreme years | Constant-time `i128` Gregorian calculation clamping `ChangeTime::to_timestamp()` once to `i64` |
| `sad-parse-zoneinfo` | Day tokens & invalid calendar dates | Unchecked empty strings, `u8 as i8` wraps, and silent `Feb 29` -> `Mar 1` spillover | Day tokens enforce `1..=31`; ordinal dates and `weekday<=`/`>=` anchors validated against month/leap-year (`zic`-aligned `Feb weekday<=29` common-year normalization) while keeping valid spillover |
| `sad-parse-zoneinfo` | `DaySpec` boundary & `Weekday` panics | `.unwrap()` panic at Jan/Dec boundaries and `% 7` panic on negative years | Checked `i16`/`i128` year-boundary resolution and `i128` Euclidean arithmetic |
| `sad-parse-zoneinfo` | `Year::Minimum`/`Maximum` panics | `unreachable!()` panics in `ChangeTime` and `RuleInfo::applies_to_year` | Symbolic `Zone` `UNTIL` years return `FailedYearParse`; handled in `RuleInfo`/`ChangeTime` with single-clamp `i128` `absolute_datetime` fallback |
| `sad-parse-zoneinfo` | Malformed `TimeSpec` signs & ranges | Accepted mixed sign combos and minutes/seconds `>= 60` | Leading `+`/`-` allowed on hours; mixed component signs and mins/secs `>= 60` return `InvalidTimeSpecAndType` |
| `sad-parse-zoneinfo` | Broken / cyclic / chained link & collisions | Unchecked map indexing panicked on broken or chained links; zone/link overlap | Iterative cycle-safe `Table::get_zoneset` returning `None` on broken/cyclic links; `TableBuilder` rejects zone/link collisions |
| `sad-parse-zoneinfo` | `%z` fractional-minute & overflow panics | `assert!(secs == 0)`, `-i64::MIN` overflow, and `u8` hour truncation | `i128` `unsigned_abs()` formatting supporting seconds and unbounded hours; `try_format_constant` added (`format_constant` documents `# Panics`) |
| `sad-parse-zoneinfo` | Unbounded rules/transitions, `%s` expansion, `UNTIL` | Unbounded rule scans, `%s` expansion, and ignored terminal/non-monotonic/out-of-range `UNTIL` | Bounded `try_timespans`/`timespans` (`MAX_RULES_PER_RULESET = 4_096`, `MAX_ACTIVE_RULES_PER_YEAR = 256`, `MAX_ZONE_SEGMENTS = 1_024`, `MAX_RULE_EVALUATIONS_PER_ZONE = 500_000`, `MAX_TRANSITIONS_PER_ZONE = 10_000`, `MAX_FORMAT_TEMPLATE_LEN = 256`, `MAX_FORMATTED_NAME_LEN = 64`), precomputes rule base timestamps per active rule per year, validates `UNTIL` ordering in `i128` with `TimestampOutOfRange`, and rejects terminal/non-increasing `UNTIL` |
| `sad-parse-zoneinfo` | Inline `#` comment & `>= 4`-segment bugs | `Rule::from_str` used `continue` on `#`; `structure.rs` emitted `/` in names | `break` on `#`; hierarchical slash-free tree building with empty-segment guards |
| `sad-chrono-tz-build` | Codegen validation, regex & partial writes | Unchecked identifiers/collisions, empty-regex `Arbitrary` errors, mid-write panics | Validates Rust identifiers, reserved generator declarations/imports, normalization/case/zone-link collisions, depth `<= 3`, non-empty regex match, `%z` provenance, `i32` bounds & `\|offset\| < 86_400` in memory before writing files (validation errors detected before writing starts leave no files written) |
| `sad-chrono-tz` | Single-clamp spans, PHF length & Serde diagnostics | Premature `saturating_add` in `local_span`; full-string PHF hash; raw unbounded Serde errors | Single-clamp `i128` in `local_span`; `MAX_TZ_NAME_LEN` short-circuit before PHF; `SerdeError` escapes control/quote chars and caps reflected input at `128` bytes on a UTF-8 boundary + `...` |

## Known Limitations

* Recurring-rule expansion in `sad-parse-zoneinfo` iterates year inputs `1800..2100` with `2100`
  excluded; explicit zone boundaries are processed separately, and lookups extrapolate the first or
  last generated span outside the table.
* `Format::format_constant()` panics on non-constant formats (`Alternate`, `Placeholder`, `Offset`);
  direct callers should prefer `Format::try_format_constant()`.

## Crate Features

* `default`: Enables `std`.
* `std`: Controls the crate's `#![no_std]` attribute (`#![cfg_attr(not(any(feature = "std", test)), no_std)]`
  at `chrono-tz/src/lib.rs`). Disable (`default-features = false`) for `#![no_std]` with zero
  `alloc` usage; `ParseError` implements `core::error::Error` independently of `std`.
* `serde`: Enables `Serialize` and `Deserialize` for `Tz` via `serde_core`.
* `rkyv`: Enables `rkyv 0.8` (`0.8.18`) zero-copy `Archive`, `Serialize`, `Deserialize`, and
  fail-closed `CheckBytes` validation (`ArchivedTz`, `ArchivedTzOffset`) for `Tz`, `TzOffset`, and
  `sad_chrono::DateTime<Tz>`. Works in `#![no_std]` without `alloc` and does not force a
  pointer-width feature (compatible with `sad-chrono`'s `rkyv-16`, `rkyv-32`, `rkyv-64`, and
  `rkyv`). Note that `ArchivedTz` stores the compiled variant index into `TZ_VARIANTS`, so
  archives must be regenerated if `filter-by-regex` changes the compiled zone set or if a future
  IANA tzdb release adds or removes variants.
* `case-insensitive`: Enables `Tz::from_str_insensitive` (invokes `sad-chrono-tz-build` at build time).
* `filter-by-regex`: Filters compiled zones via `CHRONO_TZ_TIMEZONE_FILTER` (invokes `sad-chrono-tz-build`).
* `arbitrary`: Derives `arbitrary::Arbitrary` for `Tz`.
* `defmt`: Implements `defmt::Format` for `Tz`, `ParseError`, `TzOffset`, and `FixedTimespan`.

## Rust Version Requirements

MSRV is Rust **`1.85.0`** across `sad-chrono-tz` (`0.11.x`), `sad-chrono-tz-build` (`0.6.x`), and
`sad-parse-zoneinfo` (`0.6.x`). The `0.10.x` maintenance branch supports Rust **`1.66.0`** (except
the optional `defmt` feature, which requires Rust **`1.81+`**).

## License

* `sad-chrono-tz` and `sad-chrono-tz-build` are licensed under either of
  [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT), at your option.
* `sad-parse-zoneinfo` is licensed under the [MIT License](https://github.com/sadco-io/sad-chrono-tz/blob/v0.11.1/parse-zoneinfo/LICENSE-MIT).
* The IANA Time Zone Database files in `chrono-tz/tz/` are in the public domain.

See [NOTICE](NOTICE) for attribution and a summary of modifications, and [SECURITY.md](SECURITY.md)
to report a vulnerability.

[upstream]: https://github.com/chronotope/chrono-tz
[rustsec-ctz]: https://rustsec.org/advisories/RUSTSEC-2026-0555.html
[rustsec-ctzb]: https://rustsec.org/advisories/RUSTSEC-2026-0559.html
