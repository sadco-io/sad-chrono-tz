# Security Policy

## Supported Versions

| Crate | Release Line | Supported Versions | Unsupported Versions | Notes |
|---|---|---|---|---|
| `sad-chrono-tz` | `0.11.x` (`main`) | `>= 0.11.0` | `< 0.10.5` | Targets `sad-chrono` `0.5` (`0.5.1+`), MSRV `1.85.0`, `phf 0.14`, `defmt 1.2`, `ParseError: core::error::Error` independent of `std`, API-compatible for the verified runtime surface of `chrono-tz` `0.10.4` with IANA tzdb `2026f`. |
| `sad-chrono-tz` | `0.10.x` | `>= 0.10.5, < 0.11.0` | `< 0.10.5` | Security and tzdb fixes only; targets `sad-chrono` `0.4` (`0.4.48+`), MSRV `1.66.0` (`defmt` feature requires Rust `1.81+`), `phf 0.13`. |
| `sad-chrono-tz-build` | `0.6.x` / `0.5.x` | `>= 0.6.0` (`0.6.x`), `>= 0.5.1` (`0.5.x`) | `< 0.5.1` | Build-time codegen for `sad-chrono-tz 0.11.x` (MSRV `1.85.0`) and `0.10.x` (MSRV `1.66.0`). |
| `sad-parse-zoneinfo` | `0.6.x` / `0.5.x` | `>= 0.6.0` (`0.6.x`), `>= 0.5.1` (`0.5.x`) | `< 0.5.1` | Textual zoneinfo parser and bounded transition builder (MSRV `1.85.0` on `0.6.x`, `1.66.0` on `0.5.x`). |

## Scope and Exposure Boundaries

This policy covers all three crates in the workspace, with distinct threat models:

- **`sad-chrono-tz`** (runtime): Callers may supply untrusted timezone strings (`FromStr`,
  `Tz::from_str_insensitive`, `serde::Deserialize`), untrusted `rkyv` archived bytes via checked
  access and checked decoding (`sad-chrono-tz/rkyv` enables checked archives for `Tz` and
  `TzOffset`; checked `sad_chrono::DateTime<Tz>` archives additionally require a
  `sad-chrono 0.5.1+` `rkyv*` feature and validate the representable local-time range), or
  dates/timestamps (`TimeZone`, `Offset`, and `GapInfo` lookups against precompiled or
  regex-filtered tables).
- **`sad-chrono-tz-build`** (build-time): Exposure requires control over the
  `CHRONO_TZ_TIMEZONE_FILTER` environment variable or custom/replaced `tz/` zoneinfo inputs;
  validation errors detected before writing starts fail code generation cleanly without writing
  output files (output files are written sequentially, so an I/O error during writing can leave one
  file updated).
- **`sad-parse-zoneinfo`** (parser): Direct callers may pass untrusted zoneinfo text (`Line::new`,
  `TableBuilder`) and compute transition tables (`Table::try_timespans`,
  `TableTransitions::timespans`).

Panics, integer overflows, unbounded CPU or memory resource use, or incorrect timezone lookups
reachable from untrusted input (or in fallible APIs returning `Option` / `Result`) are in scope.

## Advisories

For each security fix we publish a GitHub Security Advisory for `sad-chrono-tz`
and request a RustSec advisory. We watch upstream `chrono-tz`, `chrono-tz-build`,
and `parse-zoneinfo` advisories and issues, and mirror any that affect shared
code. Because `cargo audit` and `cargo deny` match advisories by package name,
an upstream `chrono-tz`, `chrono-tz-build`, or `parse-zoneinfo` advisory will not
flag `sad-chrono-tz`, `sad-chrono-tz-build`, or `sad-parse-zoneinfo` on its own.

## Reporting a Vulnerability

If you have discovered a security vulnerability in this project, please report
it privately. **Do not disclose it as a public issue.** This gives us time to
work with you to fix the issue before public exposure, reducing the chance that
the exploit will be used before a patch is released.

Please disclose it at [security advisory](https://github.com/sadco-io/sad-chrono-tz/security/advisories/new).

If the issue also affects upstream [chrono-tz](https://github.com/chronotope/chrono-tz),
note that `chronotope/chrono-tz` is archived upstream; we will still coordinate
with affected downstream ecosystems where appropriate.

This project is maintained by a team of volunteers on a reasonable-effort basis.
As such, please give us at least 90 days to work on a fix before public exposure.
