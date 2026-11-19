/// This test is compiled by `bin/test-regex-filtering.sh` with the filter regex
/// set to:
/// `CHRONO_TZ_TIMEZONE_FILTER="^(Europe/(London|Vaduz)|America/Argentina/Buenos_Aires|Etc/GMT\+1)$"`
///
/// We use it to check:
/// 1) That the compiled chrono-tz contains the expected 2-segment, 3-segment,
///    alias (`Europe/Vaduz`), and `%z` (`Etc/GMT+1`) timezones.
/// 2) That excluded zones (including siblings of 3-segment zones) are missing.
/// 3) That `OffsetName::abbreviation()` returns `None` for `%z` numeric names
///    and `Some(...)` for explicit abbreviations in the generated `filter-by-regex` path.

#[cfg(test)]
mod tests {
    use sad_chrono::offset::TimeZone;
    use sad_chrono_tz::{America, Etc, Europe, OffsetName, Tz, TZ_VARIANTS};
    use std::str::FromStr;

    #[test]
    fn filtered_zones_compile_and_preserve_abbreviations() {
        // 2-segment canonical zone
        let london_time = Europe::London.with_ymd_and_hms(2013, 12, 25, 14, 0, 0).unwrap();
        assert_eq!("Europe/London", Europe::London.name());
        assert_eq!(london_time.offset().abbreviation(), Some("GMT"));
        assert_eq!(london_time.offset().to_string(), "GMT");
        assert_eq!(Tz::from_str("Europe/London"), Ok(Europe::London));

        // Vaduz is a link to Europe/Zurich (which is excluded from TZ_VARIANTS).
        assert_eq!(Tz::from_str("Europe/Vaduz"), Ok(Europe::Vaduz));

        // 3-segment zone
        let _ba = America::Argentina::Buenos_Aires
            .with_ymd_and_hms(2026, 6, 1, 12, 0, 0)
            .unwrap();
        assert_eq!(
            Tz::from_str("America/Argentina/Buenos_Aires"),
            Ok(America::Argentina::Buenos_Aires)
        );

        // H1: %z-derived numeric offset returns None for abbreviation() while Display outputs "-01".
        let gmt_plus_1 = Etc::GMTPlus1.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        assert_eq!(gmt_plus_1.offset().abbreviation(), None);
        assert_eq!(gmt_plus_1.offset().to_string(), "-01");
        assert_eq!(Tz::from_str("Etc/GMT+1"), Ok(Etc::GMTPlus1));
    }

    #[test]
    fn excluded_things_are_missing() {
        // Timezones from outside selected regions should not be included.
        assert!(Tz::from_str("Australia/Melbourne").is_err());
        assert!(Tz::from_str("Indian/Maldives").is_err());
        assert!(Tz::from_str("Mexico/BajaSur").is_err());
        assert!(Tz::from_str("Pacific/Kwajalein").is_err());
        assert!(Tz::from_str("US/Central").is_err());

        // Excluded siblings inside Europe, America/Argentina, and Etc.
        assert!(Tz::from_str("Europe/Isle_Of_Man").is_err());
        assert!(Tz::from_str("Europe/Belfast").is_err());
        assert!(Tz::from_str("Europe/Zurich").is_err());
        assert!(Tz::from_str("Europe/Brussels").is_err());
        assert!(Tz::from_str("America/Argentina/Cordoba").is_err());
        assert!(Tz::from_str("Europe/Dublin").is_err());
        assert!(Tz::from_str("Europe/Warsaw").is_err());
        assert!(Tz::from_str("Etc/GMT+2").is_err());

        // Top level zones, including UTC and GMT should also be excluded.
        assert!(Tz::from_str("UTC").is_err());
        assert!(Tz::from_str("GMT").is_err());
        assert!(Tz::from_str("EST5EDT").is_err());

        // Exactly the four filtered zones should be present.
        assert_eq!(TZ_VARIANTS.len(), 4);
    }
}
