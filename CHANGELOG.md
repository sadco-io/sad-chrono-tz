ChangeLog for sad-chrono-tz
===========================

`sad-chrono-tz` is a maintained fork of
[chrono-tz](https://github.com/chronotope/chrono-tz). Entries reference the
upstream pull request or issue each change came from as `chrono-tz#NNNN`.

## sad-chrono-tz 0.11.1 / sad-chrono-tz-build 0.6.1

Additive performance, memory-footprint, transition-API, and `rkyv 0.8` release
on the `0.11.x` line (`sad-parse-zoneinfo` remains `0.6.0` unchanged). Preserves
verified public API compatibility via `cargo-semver-checks` against
`sad-chrono-tz 0.11.0` and identical transition behavior across all 597 bundled
`TZ_VARIANTS`.

### Packed `TzOffset` & `FixedTimespan` (`chrono-tz#165`, pitdicker)

* Pack `FixedTimespan` in generated tables (`offset: [u8; 5]` encoding 20-bit
  signed `utc_offset` and 20-bit signed `dst_offset`, plus `abbr_len: u8` /
  `abbr_start: u16` indexing into an interned `ABBREVIATIONS` table, with
  `abbr_len == u8::MAX` preserving `None` for `%z` numeric offsets; measured
  layout `8` bytes on standard 64-bit / native targets).
* Store `TzOffset` as `(tz: Tz, index: u16)` (measured layout `4` bytes vs `32`
  bytes in `0.11.0` on standard 64-bit / native targets; `sad_chrono::DateTime<Tz>`
  measured layout shrinks from `48` bytes to `16` bytes), preserving `0.11.0`
  `PartialEq` semantics across distinct transition spans with identical offset
  components and abbreviation.
* Intern repeated timezone abbreviations into a shared `ABBREVIATIONS` table and
  regenerate `chrono-tz/src/prebuilt/{directory,timezones}.rs`.
* Add build-time packing and transition bounds checks in
  `sad-chrono-tz-build 0.6.1`: custom input outside the 20-bit signed component
  range (`-524_288..=524_287` seconds per component), combined `|offset| < 86_400`,
  `sad-chrono::DateTime<Utc>` transition timestamp range
  (`-8_334_601_228_800..=8_210_266_876_799`), abbreviation length (`<= 64`,
  `!= u8::MAX`), `ABBREVIATIONS` length (`<= 65_535`), zone count (`<= 65_535`),
  or `65,535` **transition entries** per zone beyond the initial `first` span
  fails generation cleanly (while `sad-parse-zoneinfo` enforces an effective
  `10,000`-transition limit via `MAX_TRANSITIONS_PER_ZONE`).

### `TzOffset` validity bounds & transition iteration (`chrono-tz#199`, gharveymn)

* Add inherent `#![no_std]` methods on `TzOffset`: `valid_starting()`,
  `valid_until()`, `next_offset()`, `prev_offset()`, and `iter()`, plus
  exported `TzOffsetIter` (`ExactSizeIterator`, `FusedIterator`, `Copy`,
  `Clone`, `Debug`, `PartialEq`, `Eq`), preserving `0.11.0` `OffsetComponents`
  trait signatures and dyn-compatibility.
* Validity bounds and span traversal reflect the generated transition table:
  span starts are inclusive and span ends are exclusive; recurring rules expand
  over `1800..2100` (`2100` excluded), explicit zone boundaries are processed
  separately, and lookups extrapolate the outer spans. Equal `TzOffset` values
  can have different validity bounds because `TzOffset::eq` still compares the
  zone, both offset components, and abbreviation (while `TzOffsetIter` compares
  cursor position `(tz, index)`).

### Optional `rkyv 0.8` support

* `sad-chrono-tz/rkyv` (`rkyv = "0.8.18"`, `rkyv/bytecheck`, no `alloc` or
  `pointer_width_*` forced) enables checked archives for `Tz` (`ArchivedTz`)
  and `TzOffset` (`ArchivedTzOffset`); checked `sad_chrono::DateTime<Tz>`
  archives additionally require a `sad-chrono 0.5.1+` `rkyv*` feature
  (`rkyv-16`, `rkyv-32`, `rkyv-64`, or `rkyv`) and validate the representable
  local-time range. Use checked access (`access`) and checked decoding
  (`from_bytes`) for untrusted bytes.
* `CheckBytes` and `Deserialize` fail closed on out-of-range `Tz` discriminants
  (`>= TZ_VARIANTS.len()`, including under `filter-by-regex`) and
  out-of-bounds `TzOffset` span indices (`> tz.timespans().rest.len()`).
  `ArchivedTz` stores a 16-bit zone ordinal. `ArchivedTzOffset` also stores a
  16-bit span index. Both fields are little-endian by default. Decode with the
  same rkyv format configuration and matching ordered zone and transition
  tables. An in-range index can resolve differently after a filter or tzdb
  change. `DateTime<Tz>` verification checks local-time representability under
  `sad-chrono`'s stored-offset contract (verify a fingerprint of the generated
  zone and transition tables before decoding when cross-build table rejection is
  required; `IANA_TZDB_VERSION` alone does not detect `filter-by-regex`
  changes).
* Add `chrono-tz/tests/check-rkyv` integration suite (including no-alloc
  `rkyv::api::low` coverage across `rkyv-16`, `rkyv-32`, and `rkyv-64`),
  `check-nostd` `rkyv` and `rkyv,defmt` target coverage, `filter-by-regex` +
  `rkyv` test coverage (with aligned buffers and misaligned rejection tests),
  and `fuzz_tz_rkyv` fuzz target.

### CI

* Pin `--baseline-version 0.11.0` in `.github/workflows/lint.yml` `semver` job
  and expand `cargo fmt --check` coverage to `check-regex-filtering` and
  `check-nostd`; add Windows `rkyv` / `check-rkyv` runtime tests, `no_std`
  `rkyv,defmt` builds, `w16`/`w32`/`w64` no-alloc `check-rkyv` checks, and
  `fuzz_tz_rkyv` to CI.

## sad-chrono-tz 0.11.0 / sad-chrono-tz-build 0.6.0 / sad-parse-zoneinfo 0.6.0

Current release line targeting `sad-chrono = "0.5.1"` (`0.5.1+`) with MSRV
`1.85.0`. API-compatible with `chrono-tz 0.10.4` for the verified runtime
signatures and supported feature configurations (`Default for Tz` is implemented
when `UTC` is included in the compiled zone set, `OffsetName::abbreviation()`
returns `None` for `%z`-derived numeric offsets, `Tz` exposes all 597 IANA
`2026f` variants, and `CHRONO_TZ_TIMEZONE_FILTER` selects zones whose names
match the regex — whole-name matching requires anchors such as `^...$`, and an
unset or blank filter leaves all zones enabled; in `sad-parse-zoneinfo 0.6.0`,
public `Format::format` takes three arguments — `utc_offset`, `dst_offset`,
`letters` — matching `chrono-tz#211` / `parse-zoneinfo 0.5.0`, whereas
upstream `parse-zoneinfo 0.4.0` took two).

> **Note:** `sad-chrono-tz 0.11.0` pairs `chrono-tz 0.10`'s verified runtime
> API with `sad-chrono 0.5.1+` and current dependencies. It is **not**
> upstream's unreleased `chrono-tz 0.11` breaking API.

### Breaking dependency & MSRV upgrades

* Target **`sad-chrono = "0.5.1"`** (`0.5.1+`) instead of `sad-chrono 0.4`.
* Bump MSRV (`rust-version`) from `1.66.0` to **`1.85.0`** across
  `sad-chrono-tz`, `sad-chrono-tz-build`, and `sad-parse-zoneinfo`.
* Upgrade `phf`, `phf_codegen`, and `phf_shared` from `0.13` to **`0.14`** and
  regenerate `chrono-tz/src/prebuilt/{directory,timezones}.rs`.
* Upgrade `defmt` requirement from `1.0.1` to **`1.2`** (matching `sad-chrono
  0.5.1`) and add `defmt` feature coverage to `check-nostd`.
* Upgrade `arbitrary` requirement from `1.2` to **`1.4`** (resolving to `1.5.0`
  cleanly on Rust `1.85.0`).
* Update `sad-chrono-tz-build` and `sad-parse-zoneinfo` to `0.6.0`.

### `#![no_std]` improvements

* Implement `core::error::Error` for `ParseError` independently of `std` (Rust
  `1.81+`), so `#![no_std]` callers (`default-features = false`) receive
  `Error` without enabling `std`.
* Keep `std = []` (and `default = ["std"]`): the `std` feature still controls
  the crate's `#![cfg_attr(not(any(feature = "std", test)), no_std)]` attribute
  in `chrono-tz/src/lib.rs`, preserving compatibility with downstream feature
  lists.

## sad-chrono-tz 0.10.5 / sad-chrono-tz-build 0.5.1 / sad-parse-zoneinfo 0.5.1

Initial `sadco-io` release, API-compatible with `chrono-tz 0.10.4` for the
verified runtime signatures and supported feature configurations (`Default for
Tz` is implemented when `UTC` is included in the compiled zone set, and
`OffsetName::abbreviation()` returns `None` for `%z`-derived numeric offsets),
targeting `sad-chrono = "0.4.48"` (`0.4.48+`) with MSRV `1.66.0` (optional
`defmt` requires Rust `1.81+`).

### IANA Time Zone Database

* Updated vendored `chrono-tz/tz` submodule from `2025b` to **`2026f`**
  (`0` variants added, `0` removed; `TZ_VARIANTS.len() == 597`), picking up
  2026 rule updates for Cuba (`America/Havana`), Manitoba (`America/Winnipeg`),
  Alberta (`America/Edmonton`), British Columbia (`America/Vancouver`),
  Northwest Territories (`America/Inuvik`), Morocco and Western Sahara
  (`Africa/Casablanca`, `Africa/El_Aaiun`), Moldova (`Europe/Chisinau`), and
  POSIX `USback` zones (`EST5EDT`, `CST6CDT`, `MST7MDT`, `PST8PDT`).

### Upstream backports

* `chrono-tz#211` (Robert Bastian): `%z` numeric offset format support in
  `sad-parse-zoneinfo` (`0.5.1`, where public `Format::format` takes three
  arguments — `utc_offset`, `dst_offset`, `letters` — matching
  `parse-zoneinfo 0.5.0`, whereas upstream `parse-zoneinfo 0.4.0` took two),
  preserving `%z` provenance in codegen so `sad-chrono-tz` matches
  `chrono-tz 0.10.4` `OffsetName::abbreviation() == None` for `%z`-derived
  names while `Display` outputs the numeric offset.
* `chrono-tz#214` (Robert Bastian): link deduplication in
  `sad-chrono-tz-build`, substantially reducing `prebuilt/timezones.rs`.
* `chrono-tz#218` (Robert Bastian): fix `CHRONO_TZ_TIMEZONE_FILTER` regex
  filtering so filters select only zones whose names match the regex rather
  than pulling in unrelated `Etc/GMT*` and `UTC` zones (`chrono-tz#215`;
  whole-name matching requires anchors such as `^...$`, and an unset or blank
  filter leaves all zones enabled), adapted for MSRV `1.66` and conditional
  `Default for Tz` when `UTC` is excluded.
* `chrono-tz#222` (Dirkjan Ochtman): clean up missing lifetime elision warnings
  in `parse-zoneinfo/src/structure.rs`.
* `chrono-tz#223` (Yotam Ofek): update `phf`, `phf_codegen`, and `phf_shared`
  to `0.13`.
* `chrono-tz#224` (Yotam Ofek): depend on `serde_core` instead of `serde` for
  the `serde` feature.
* `chrono-tz#225` (Yotam Ofek): bump MSRV to Rust `1.66.0` across all three
  crates.
* `chrono-tz#226` (Paul Bender): add feature-gated `defmt` (`1.0.1`) support
  (requires Rust `1.81+` due to `defmt-parser`).
* `chrono-tz#227` (Paul Bender): fix clippy warnings in `chrono-tz-build`.

### Security and robustness

* **`sad-parse-zoneinfo 0.5.1`**:
  * Replace `O(|year - 1970|)` year loop in `ChangeTime::to_timestamp` with a
    constant-time `i128` Gregorian calculation clamping once to `i64` bounds.
  * Enforce `1..=31` day tokens in `DaySpec::from_str` (returning
    `InvalidDaySpec` or `FailedWeekdayParse`), validate ordinal calendar dates
    and `weekday<=`/`>=` anchors against month/leap-year lengths in
    `DaySpec::try_to_concrete_day`, `ZoneInfo`/`Rule` parsing, and transition
    building (matching `zic`'s `Feb weekday<=29` common-year anchor
    normalization and rejecting `Feb weekday>=29` in common years) while keeping
    valid weekday-relative spillover.
  * Harden `DaySpec` resolution and `Weekday` calculation with `i16`/`i128`
    Euclidean arithmetic and January/December year-boundary handling.
  * Return `FailedYearParse` for `Year::Minimum`/`Maximum` in `Zone` `UNTIL`
    columns and handle them without `unreachable!()` panics in `RuleInfo` and
    `ChangeTime`.
  * Compute `RuleInfo::absolute_datetime` (including its invalid-ordinal
    fallback via `ChangeTime::to_timestamp_unclamped_i128`) in `i128` with a
    single final clamp to `i64::MIN..=i64::MAX`.
  * Accept optional leading `+` or `-` on `TimeSpec` hours while rejecting
    mixed component signs and minutes/seconds `>= 60` via
    `InvalidTimeSpecAndType`.
  * Resolve chained links iteratively with cycle detection in
    `Table::get_zoneset`, returning `None` on broken or cyclic links, and reject
    zone/link name collisions in `TableBuilder`.
  * Support fractional-minute `%z` formatting and unbounded hours via `i128`
    `unsigned_abs()`; add fallible `Format::try_format` (rejecting complete
    templates over `MAX_FORMAT_TEMPLATE_LEN = 256` bytes and formatted output
    exceeding `max_len`) and `Format::try_format_constant` (while documenting
    `# Panics` on `Format::format_constant`).
  * Add bounded fallible `Table::try_timespans` and `Table::try_codegen_timespans`
    (delegated to by `TableTransitions::timespans` returning `Option`): cap
    rulesets (`4_096` rules), active rules per year (`256`), zone segments
    (`1_024`), cumulative rule evaluations (`500_000` per zone), transitions per
    zone (`10_000`), and formatted abbreviations (`64` bytes); precompute rule
    base timestamps once per active rule per year; compare `UNTIL` instants in
    `i128` before `i64` range checking (`TimestampOutOfRange`); and reject
    terminal `UNTIL` segments and non-increasing `UNTIL` timestamps.
  * Fix `Rule::from_str` inline `#` comment handling (`continue` -> `break`)
    and `structure::build_tree` multi-level / empty-segment handling.
* **`sad-chrono-tz-build 0.5.1`**:
  * Validate Rust identifier grammar, reserved generator declarations and imports
    (`IANA_TZDB_VERSION`, `Tz`, `ParseError`, `TZ_VARIANTS`, `TIMEZONES`,
    `MAX_TZ_NAME_LEN`, etc. in their respective namespaces), normalization/
    uppercase/case-insensitive collisions, zone/link collisions, broken/cyclic
    links, hierarchy depth `<= 3` (rejecting hierarchies deeper than three
    segments), non-empty filtered zone sets, `%z` provenance (`None` for `%z`),
    and `i32` / `|offset| < 86_400` bounds; generate both `timezones.rs` and
    `directory.rs` completely in memory before writing so pre-write validation
    errors fail cleanly without writing output files (distinct from downstream
    Rust compilation and sequential file I/O).
  * Emit `MAX_TZ_NAME_LEN` pre-checks before PHF lookups in `Tz::from_str` and
    `Tz::from_str_insensitive`, and escape emitted string literals with `{:?}`.
* **`sad-chrono-tz 0.10.5`**:
  * Compute `FixedTimespanSet::local_span` in `i128` with a single final clamp
    to `i64`, use `saturating_add` in `FixedTimespan::fix`, and handle
    fractional-minute offsets in `FixedTimespan` `Display`.
  * Escape control characters and quotes via `char::escape_debug()` in
    `SerdeError` while capping reflected diagnostic output at `128` bytes on a
    UTF-8 boundary and appending `...` for longer inputs.
  * Use exclusively created temporary directories with RAII cleanup in
    `tests/codegen.rs` and `chrono-tz-build` tests (with LF normalization via
    `.gitattributes`), gate zone-specific doctests cleanly under
    `filter-by-regex`, expand `bin/test-regex-filtering.sh` (alias-only build,
    zero-match failure, filtered doctests), add embedded `no_std` `defmt` checks,
    and add Havana / Brazil 1965–1966 (`1965-04-01 02:00:00 UTC`) transition-edge
    and fuzz coverage.

### Packaging and governance

* Rename crates to `sad-chrono-tz`, `sad-chrono-tz-build`, and
  `sad-parse-zoneinfo` using default Cargo library names, targeting
  `sad-chrono 0.4` (`0.4.48+`).
* Replace symlinked `LICENSE` files with real `LICENSE-MIT` and
  `LICENSE-APACHE` files (`chrono-tz#164`), exclude `examples/benchmark.rs`
  from `sad-parse-zoneinfo` packaging, add `NOTICE` and `SECURITY.md`, remove
  the unmaintained `tzfile` dev-dependency, add `check-rename` verification
  crate, and set up `sad-chrono`-grade CI workflows and `deny.toml`.
