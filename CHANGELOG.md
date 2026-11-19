ChangeLog for sad-chrono-tz
===========================

`sad-chrono-tz` is a maintained fork of
[chrono-tz](https://github.com/chronotope/chrono-tz). Entries reference the
upstream pull request or issue each change came from as `chrono-tz#NNNN`.

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
  `sad-parse-zoneinfo` (`0.5.1`), preserving `%z` provenance in codegen so
  `sad-chrono-tz` matches `chrono-tz 0.10.4` `OffsetName::abbreviation() == None`
  for `%z`-derived names while `Display` outputs the numeric offset.
* `chrono-tz#214` (Robert Bastian): link deduplication in
  `sad-chrono-tz-build`, substantially reducing `prebuilt/timezones.rs`.
* `chrono-tz#218` (Robert Bastian): fix `CHRONO_TZ_TIMEZONE_FILTER` regex
  filtering so single-zone filters select only matching zones rather than
  pulling in unrelated `Etc/GMT*` and `UTC` zones (`chrono-tz#215`), adapted
  for MSRV `1.66` and conditional `Default for Tz` when `UTC` is excluded.
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
    `InvalidDaySpec` or `FailedWeekdayParse`), and reject invalid ordinal
    calendar dates (such as February 29 in a common year or April 31) in
    `DaySpec::try_to_concrete_day`, `ZoneInfo`/`Rule` parsing, and transition
    building while keeping valid weekday-relative spillover.
  * Harden `DaySpec` resolution and `Weekday` calculation with `i16`/`i128`
    Euclidean arithmetic and January/December year-boundary handling.
  * Return `FailedYearParse` for `Year::Minimum`/`Maximum` in `Zone` `UNTIL`
    columns and handle them without `unreachable!()` panics in `RuleInfo` and
    `ChangeTime`.
  * Compute `RuleInfo::absolute_datetime` in `i128` with a single final clamp
    to `i64::MIN..=i64::MAX`.
  * Accept optional leading `+` or `-` on `TimeSpec` hours while rejecting
    mixed component signs and minutes/seconds `>= 60` via
    `InvalidTimeSpecAndType`.
  * Resolve chained links iteratively with cycle detection in
    `Table::get_zoneset`, returning `None` on broken or cyclic links, and reject
    zone/link name collisions in `TableBuilder`.
  * Support fractional-minute `%z` formatting and unbounded hours via `i128`
    `unsigned_abs()`; add fallible `Format::try_format` (length-checked before
    allocation) and `Format::try_format_constant` (while documenting
    `# Panics` on `Format::format_constant`).
  * Add bounded fallible `Table::try_timespans` and `Table::try_codegen_timespans`
    (delegated to by `TableTransitions::timespans` returning `Option`): cap
    rulesets (`4_096` rules), active rules per year (`256`), transitions per
    zone (`10_000`), and formatted abbreviations (`64` bytes); precompute rule
    base timestamps in `O(R)` per year; and reject terminal `UNTIL` segments and
    non-increasing `UNTIL` timestamps across continuation lines.
  * Fix `Rule::from_str` inline `#` comment handling (`continue` -> `break`)
    and `structure::build_tree` multi-level / empty-segment handling.
* **`sad-chrono-tz-build 0.5.1`**:
  * Validate Rust identifier grammar, normalization/uppercase/case-insensitive
    collisions, zone/link collisions, broken/cyclic links, hierarchy depth
    `<= 3`, non-empty filtered zone sets, `%z` provenance (`None` for `%z`),
    and `i32` / `|offset| < 86_400` bounds; generate both `timezones.rs` and
    `directory.rs` completely in memory before writing to disk so invalid inputs
    fail the build cleanly without partial files.
  * Emit `MAX_TZ_NAME_LEN` pre-checks before PHF lookups in `Tz::from_str` and
    `Tz::from_str_insensitive`, and escape emitted string literals with `{:?}`.
* **`sad-chrono-tz 0.10.5`**:
  * Compute `FixedTimespanSet::local_span` in `i128` with a single final clamp
    to `i64`, use `saturating_add` in `FixedTimespan::fix`, and handle
    fractional-minute offsets in `FixedTimespan` `Display`.
  * Escape control characters and quotes via `char::escape_debug()` in
    `SerdeError` while capping reflected diagnostic output at `128` bytes on a
    UTF-8 boundary and appending `...` for longer inputs.
  * Make `tests/codegen.rs` compare in a temporary directory with LF
    normalization (enforced via `.gitattributes`) without mutating tracked
    prebuilt files, expand `check-regex-filtering` (3-segment zone, alias,
    excluded siblings, `%z` `abbreviation() == None`), and add transition-edge
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
