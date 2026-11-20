#!/usr/bin/env bash

set -euxo pipefail

export RUST_BACKTRACE=1

CHRONO_TZ_TIMEZONE_FILTER='^(Europe/(London|Vaduz)|America/Argentina/Buenos_Aires|Etc/GMT\+1)$' \
    cargo test --manifest-path chrono-tz/tests/check-regex-filtering/Cargo.toml --color=always -- --color=always

CHRONO_TZ_TIMEZONE_FILTER='^Europe/London$' \
    cargo test -p sad-chrono-tz --doc --features filter-by-regex --color=always

CHRONO_TZ_TIMEZONE_FILTER='^Europe/London$' \
    cargo test -p sad-chrono-tz --lib --features filter-by-regex,rkyv --color=always -- --color=always

CHRONO_TZ_TIMEZONE_FILTER='^Europe/Vaduz$' \
    cargo check -p sad-chrono-tz --features filter-by-regex --color=always

if zero_match_output=$(CHRONO_TZ_TIMEZONE_FILTER='^NoSuch/Zone$' cargo check -p sad-chrono-tz --features filter-by-regex 2>&1); then
    echo "expected zero-match CHRONO_TZ_TIMEZONE_FILTER build to fail" >&2
    exit 1
fi
echo "$zero_match_output" | grep -q 'matched zero timezones'
