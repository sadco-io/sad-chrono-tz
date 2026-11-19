ChangeLog for sad-chrono-tz
===========================

`sad-chrono-tz` is a maintained, API-compatible fork of
[chrono-tz](https://github.com/chronotope/chrono-tz). Entries reference the
upstream pull request or issue each change came from as `chrono-tz#NNNN`.

## sad-chrono-tz 0.10.5 / sad-chrono-tz-build 0.5.1 / sad-parse-zoneinfo 0.5.1

Initial `sadco-io` release, keeping `chrono-tz 0.10.4`'s exact public API
(`impl Default for Tz`, `OffsetComponents`, and
`OffsetName::abbreviation(&self) -> Option<&str>`) and targeting
`sad-chrono = "0.4"` (`0.4.46+`) with MSRV `1.66.0`.

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
  `sad-parse-zoneinfo` (`0.5.1`).
* `chrono-tz#214` (Robert Bastian): link deduplication in
  `sad-chrono-tz-build`, substantially reducing `prebuilt/timezones.rs`.
* `chrono-tz#218` (Robert Bastian): fix `CHRONO_TZ_TIMEZONE_FILTER` regex
  filtering so single-zone filters no longer pull in unrelated `Etc/GMT*` and
  `UTC` zones (`chrono-tz#215`), adapted for MSRV `1.66` and `Default for Tz`
  compatibility when `UTC` is filtered out.
* `chrono-tz#222` (Dirkjan Ochtman): clean up missing lifetime elision warnings
  in `parse-zoneinfo/src/structure.rs`.
* `chrono-tz#223` (Yotam Ofek): update `phf`, `phf_codegen`, and `phf_shared`
  to `0.13`.
* `chrono-tz#224` (Yotam Ofek): depend on `serde_core` instead of `serde` for
  the `serde` feature.
* `chrono-tz#225` (Yotam Ofek): bump MSRV to Rust `1.66.0` across all three
  crates.
* `chrono-tz#226` (Paul Bender): add feature-gated `defmt` (`1.0.1`) support.
* `chrono-tz#227` (Paul Bender): fix clippy warnings in `chrono-tz-build`.

### Security and robustness

* **`sad-parse-zoneinfo 0.5.1`**:
  * Replace `O(|year - 1970|)` year loop in `ChangeTime::to_timestamp` with a
    closed-form `O(1)` checked `i128` Gregorian calculation (saturating to
    `i64` bounds).
  * Validate `DaySpec::from_str` against empty strings, overflows, `u8 as i8`
    wraps, and days outside `1..=31`.
  * Harden `DaySpec::to_concrete_day` and `Weekday::calculate` with `i16`/`i128`
    Euclidean arithmetic and safe January/December year-boundary handling.
  * Reject `Year::Minimum`/`Maximum` in `Zone` `UNTIL` columns and handle them
    without `unreachable!()` panics in `RuleInfo` and `ChangeTime`.
  * Reject malformed `TimeSpec` sign combinations and minutes/seconds `>= 60`.
  * Resolve chained links iteratively with cycle detection in
    `Table::get_zoneset`, returning `None` on broken or cyclic links.
  * Support fractional-minute `%z` formatting and unbounded hours without
    panicking or `i64::MIN` / `u8` overflow.
  * Eliminate `.unwrap()` / `.expect()` / unchecked indexing panics and
    unchecked `i64` arithmetic in `transitions.rs`.
  * Fix `Rule::from_str` inline `#` comment handling (`continue` -> `break`)
    and `structure::build_tree` multi-level / empty-segment handling.
* **`sad-chrono-tz-build 0.5.1`**:
  * Validate every generated `FixedTimespan` offset fits in `i32` and
    `(-86_400..86_400)` so runtime `TzOffset::fix()` cannot panic.
  * Resolve chained links transitively with cycle detection, use `{:?}` string
    escaping, filter 3-level submodules in `directory.rs`, and harden
    `convert_bad_chars` for mixed hyphens and digits.
* **`sad-chrono-tz 0.10.5`**:
  * Use `saturating_add` in `FixedTimespanSet::local_span` and
    `FixedTimespan::fix`, and avoid fractional-minute panics in
    `FixedTimespan` `Display`.
  * Bound untrusted input length reflected in `SerdeError` to `128` bytes
    (truncating at UTF-8 char boundaries with `...` for longer inputs).
  * Add `fuzz/` workspace (`fuzz_parse_line`, `fuzz_parse_table`,
    `fuzz_tz_from_str`, `fuzz_tz_serde`).

### Packaging and governance

* Rename crates to `sad-chrono-tz`, `sad-chrono-tz-build`, and
  `sad-parse-zoneinfo` using default Cargo library names.
* Replace symlinked `LICENSE` files with real `LICENSE-MIT` and
  `LICENSE-APACHE` files (`chrono-tz#164`), add `NOTICE` and `SECURITY.md`,
  remove the unmaintained `tzfile` dev-dependency, add `check-rename`
  verification crate, and set up `sad-chrono`-grade CI workflows and `deny.toml`.
