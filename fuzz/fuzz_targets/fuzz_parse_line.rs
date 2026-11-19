#![no_main]
use libfuzzer_sys::fuzz_target;
use sad_parse_zoneinfo::line::{DaySpec, Line, Month};
use sad_parse_zoneinfo::table::{Format, RuleInfo, ZoneInfo};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };

    for line in text.lines() {
        let parsed = match Line::new(line) {
            Ok(parsed) => parsed,
            Err(err) => {
                let _ = err.to_string();
                continue;
            }
        };
        match parsed {
            Line::Rule(rule) => {
                let info = RuleInfo::from(rule);
                for year in [1800, 1970, 2000, 2023, 2026, 2099, i64::MIN, i64::MAX] {
                    let _ = info.applies_to_year(year);
                    for (utc_off, dst_off) in [(0, 0), (-18_000, 3_600), (19_800, 0)] {
                        let _ = info.try_absolute_datetime_i128(year, utc_off, dst_off);
                        let _ = info.absolute_datetime(year, utc_off, dst_off);
                    }
                }
                let _ = info.day.try_to_concrete_day(2023, Month::February);
                let _ = info.day.try_to_concrete_day(2024, Month::February);
            }
            Line::Zone(zone) => {
                let info = ZoneInfo::from(zone.info);
                let _ = info.format.try_format_constant();
                let _ = info.format.try_format(info.offset, 3_600, None, 64);
                if let Some(end) = info.end_time {
                    let _ = end.try_to_timestamp(info.offset, 3_600);
                    let _ = end.to_timestamp(info.offset, 0);
                    let _ = end.year();
                }
            }
            Line::Continuation(cont) => {
                let info = ZoneInfo::from(cont);
                let _ = info.format.try_format_constant();
                let _ = info.format.try_format(info.offset, 0, None, 64);
                if let Some(end) = info.end_time {
                    let _ = end.try_to_timestamp(info.offset, 0);
                    let _ = end.to_timestamp(info.offset, 0);
                    let _ = end.year();
                }
            }
            Line::Link(_) | Line::Space => {}
        }
    }

    let _ = DaySpec::Ordinal(29).try_to_concrete_day(1900, Month::February);
    let _ = Format::new(text).try_format(5 * 3600 + 30 * 60, 0, None, 64);
});
