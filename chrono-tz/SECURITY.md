# Security Policy

## Supported Versions

| Version | Supported | Notes |
|---------|-----------|-------|
| `0.10.x` (`sad-chrono-tz` >= `0.10.5`, `sad-chrono-tz-build` >= `0.5.1`, `sad-parse-zoneinfo` >= `0.5.1`) | All fixes | Current drop-in release line targeting `sad-chrono` `0.4` (MSRV `1.66.0`), API-compatible with `chrono-tz` `0.10.4` with IANA tzdb `2026f` and parser/codegen/runtime hardening. |
| `< 0.10.5` | No | Upgrade to `0.10.5` or later. |

## Scope

This policy covers all three crates in the workspace:

- **`sad-chrono-tz`**: `Tz::from_str` / `Tz::from_str_insensitive`, `serde::Deserialize`,
  `TimeZone` / `Offset` / `GapInfo` lookups, and generated timezone tables.
- **`sad-chrono-tz-build`**: build-time IANA tzdb parsing, `CHRONO_TZ_TIMEZONE_FILTER`
  regex filtering, link resolution, and generated code invariants.
- **`sad-parse-zoneinfo`**: parsing untrusted IANA zoneinfo text lines, building
  zone/rule/link tables, formatting `%z` abbreviations, and computing transition
  tables.

Panics, integer overflows, unbounded CPU or memory resource use, or incorrect
timezone lookups reachable from untrusted input (or in fallible APIs returning
`Option` / `Result`) are in scope.

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
