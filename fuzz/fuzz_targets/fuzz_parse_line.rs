#![no_main]
use libfuzzer_sys::fuzz_target;
use sad_parse_zoneinfo::line::Line;
use sad_parse_zoneinfo::table::{RuleInfo, ZoneInfo};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };

    for line in text.lines() {
        let Ok(parsed) = Line::new(line) else {
            continue;
        };
        match parsed {
            Line::Rule(rule) => {
                let info = RuleInfo::from(rule);
                let _ = info.applies_to_year(2026);
                let _ = info.absolute_datetime(2026, 0, 0);
            }
            Line::Zone(zone) => {
                let info = ZoneInfo::from(zone.info);
                if let Some(end) = info.end_time {
                    let _ = end.to_timestamp(info.offset, 0);
                    let _ = end.year();
                }
            }
            Line::Continuation(cont) => {
                let info = ZoneInfo::from(cont);
                if let Some(end) = info.end_time {
                    let _ = end.to_timestamp(info.offset, 0);
                    let _ = end.year();
                }
            }
            Line::Link(_) | Line::Space => {}
        }
    }
});
