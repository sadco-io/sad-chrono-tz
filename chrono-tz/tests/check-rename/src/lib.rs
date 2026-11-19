pub fn convert_to_utc(dt: chrono::DateTime<chrono_tz::Tz>) -> chrono::DateTime<chrono::Utc> {
    dt.with_timezone(&chrono::Utc)
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use chrono::{Duration, Offset, TimeZone, Utc};
    use chrono_tz::{
        America, Europe, GapInfo, OffsetComponents, OffsetName, Tz, IANA_TZDB_VERSION, TZ_VARIANTS,
    };
    use serde_test::{assert_tokens, Token};

    #[test]
    fn rename_drop_in_api_works() {
        assert_eq!(IANA_TZDB_VERSION, "2026f");
        assert_eq!(TZ_VARIANTS.len(), 597);
        assert_eq!(Tz::default(), Tz::UTC);

        let dt = Europe::London
            .with_ymd_and_hms(2026, 5, 10, 12, 0, 0)
            .unwrap();
        assert_eq!(dt.offset().tz_id(), "Europe/London");
        assert_eq!(dt.offset().abbreviation(), Some("BST"));
        assert_eq!(dt.offset().base_utc_offset(), Duration::hours(0));
        assert_eq!(dt.offset().dst_offset(), Duration::hours(1));
        assert_eq!(dt.offset().fix().local_minus_utc(), 3600);
        assert_eq!(
            super::convert_to_utc(dt),
            Utc.with_ymd_and_hms(2026, 5, 10, 11, 0, 0).unwrap()
        );

        assert_eq!(Tz::from_str("America/New_York"), Ok(America::New_York));
        assert_eq!(
            Tz::from_str_insensitive("america/new_york"),
            Ok(America::New_York)
        );

        let gap_dt = chrono::NaiveDate::from_ymd_opt(2026, 3, 8)
            .unwrap()
            .and_hms_opt(2, 30, 0)
            .unwrap();
        let gap = GapInfo::new(&gap_dt, &America::New_York).unwrap();
        assert!(gap.begin.is_some());
        assert!(gap.end.is_some());

        assert_tokens(&America::New_York, &[Token::String("America/New_York")]);
    }
}
