#[cfg(test)]
mod tests {
    use rkyv::{access, from_bytes, rancor::Error, to_bytes};
    use sad_chrono::{DateTime, NaiveDate, Offset, TimeZone, Utc};
    use sad_chrono_tz::{
        America::New_York, ArchivedTz, ArchivedTzOffset, Etc::GMTPlus1, Europe::London,
        OffsetComponents, OffsetName, Tz, TzOffset, TZ_VARIANTS, UTC,
    };

    #[test]
    fn roundtrip_tz_offset_and_datetime() {
        let samples = [
            London.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap(),
            London.with_ymd_and_hms(2024, 7, 15, 12, 0, 0).unwrap(),
            New_York
                .with_ymd_and_hms(2024, 11, 3, 1, 30, 0)
                .earliest()
                .unwrap(),
            GMTPlus1.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap(),
            UTC.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        ];

        for dt in samples {
            let tz = dt.timezone();
            let tz_bytes = to_bytes::<Error>(&tz).unwrap();
            assert_eq!(access::<ArchivedTz, Error>(&tz_bytes).unwrap(), &tz);
            assert_eq!(from_bytes::<Tz, Error>(&tz_bytes).unwrap(), tz);

            let off = *dt.offset();
            let off_bytes = to_bytes::<Error>(&off).unwrap();
            assert_eq!(access::<ArchivedTzOffset, Error>(&off_bytes).unwrap(), &off);
            let decoded_off = from_bytes::<TzOffset, Error>(&off_bytes).unwrap();
            assert_eq!(decoded_off, off);
            assert_eq!(decoded_off.valid_starting(), off.valid_starting());
            assert_eq!(decoded_off.valid_until(), off.valid_until());

            let dt_bytes = to_bytes::<Error>(&dt).unwrap();
            let archived_dt = access::<rkyv::Archived<DateTime<Tz>>, Error>(&dt_bytes).unwrap();
            assert_eq!(archived_dt, &dt);
            let decoded_dt = from_bytes::<DateTime<Tz>, Error>(&dt_bytes).unwrap();
            assert_eq!(decoded_dt, dt);
            assert_eq!(decoded_dt.offset(), dt.offset());
            assert_eq!(
                decoded_dt.offset().abbreviation(),
                dt.offset().abbreviation()
            );
            assert_eq!(decoded_dt.offset().fix(), dt.offset().fix());
            assert_eq!(
                decoded_dt.offset().base_utc_offset(),
                dt.offset().base_utc_offset()
            );
            assert_eq!(decoded_dt.offset().dst_offset(), dt.offset().dst_offset());
            assert_eq!(decoded_dt.to_string(), dt.to_string());
        }
    }

    #[test]
    fn datetime_tz_rejects_corrupted_tz_span_and_local_range() {
        let dt = London.with_ymd_and_hms(2024, 7, 15, 12, 0, 0).unwrap();
        let bytes = to_bytes::<Error>(&dt).unwrap();
        assert_eq!(bytes.len(), 16);

        // ArchivedDateTime<Tz> layout: NaiveDateTime (12 bytes) + ArchivedTzOffset (4 bytes: tz at 12..14, index at 14..16).
        // 1. Out-of-range Tz discriminant inside DateTime<Tz>
        for bad_disc in [TZ_VARIANTS.len() as u16, u16::MAX] {
            let mut tampered = bytes.clone();
            tampered[12..14].copy_from_slice(&bad_disc.to_le_bytes());
            assert!(access::<rkyv::Archived<DateTime<Tz>>, Error>(&tampered).is_err());
            assert!(from_bytes::<DateTime<Tz>, Error>(&tampered).is_err());
        }

        // 2. Out-of-bounds TzOffset span index inside DateTime<Tz>
        let oob_span = (London.timespans_len_for_test() + 1) as u16;
        for bad_idx in [oob_span, u16::MAX] {
            let mut tampered = bytes.clone();
            tampered[14..16].copy_from_slice(&bad_idx.to_le_bytes());
            assert!(access::<rkyv::Archived<DateTime<Tz>>, Error>(&tampered).is_err());
            assert!(from_bytes::<DateTime<Tz>, Error>(&tampered).is_err());
        }

        // 3. Invalid DateTime<Tz> local-time range (MAX_UTC + positive offset overflows NaiveDateTime)
        let max_utc_naive = DateTime::<Utc>::MAX_UTC.naive_utc();
        let positive_off = *NaiveDate::from_ymd_opt(2024, 7, 15)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc()
            .with_timezone(&London)
            .offset();
        assert!(positive_off.fix().local_minus_utc() > 0);
        let out_of_range = DateTime::<Tz>::from_naive_utc_and_offset(max_utc_naive, positive_off);
        let oor_bytes = to_bytes::<Error>(&out_of_range).unwrap();
        assert!(access::<rkyv::Archived<DateTime<Tz>>, Error>(&oor_bytes).is_err());
        assert!(from_bytes::<DateTime<Tz>, Error>(&oor_bytes).is_err());
    }

    #[test]
    fn golden_wire_bytes() {
        // Tz::Africa__Abidjan == 0 -> [0x00, 0x00]
        assert_eq!(
            to_bytes::<Error>(&Tz::Africa__Abidjan).unwrap().as_slice(),
            &[0x00, 0x00]
        );
        // Tz::UTC -> explicit little-endian u16 discriminant
        let utc_disc = (Tz::UTC as u16).to_le_bytes();
        assert_eq!(to_bytes::<Error>(&Tz::UTC).unwrap().as_slice(), &utc_disc);
        // UTC TzOffset has only span index 0 -> [disc_lo, disc_hi, 0x00, 0x00]
        let utc_off = *UTC.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap().offset();
        assert_eq!(
            to_bytes::<Error>(&utc_off).unwrap().as_slice(),
            &[utc_disc[0], utc_disc[1], 0x00, 0x00]
        );
    }

    trait TimespansCountExt {
        fn timespans_len_for_test(&self) -> usize;
    }

    impl TimespansCountExt for Tz {
        fn timespans_len_for_test(&self) -> usize {
            DateTime::<Utc>::MIN_UTC
                .with_timezone(self)
                .offset()
                .iter()
                .count()
        }
    }
}
