#[cfg(test)]
mod tests {
    use core::mem::MaybeUninit;

    use rkyv::{
        api::low::{access, from_bytes, to_bytes_in_with_alloc, LowSerializer},
        rancor::Error,
        ser::{allocator::SubAllocator, writer::Buffer},
        util::Align,
        Serialize,
    };
    use sad_chrono::{DateTime, NaiveDate, Offset, TimeZone, Utc};
    use sad_chrono_tz::{
        America::New_York, ArchivedTz, ArchivedTzOffset, Etc::GMTPlus1, Europe::London,
        OffsetComponents, OffsetName, Tz, TzOffset, TZ_VARIANTS, UTC,
    };

    fn archive_no_alloc<T>(value: &T) -> (Align<[u8; 32]>, usize)
    where
        T: for<'a> Serialize<LowSerializer<Buffer<'a>, SubAllocator<'a>, Error>>,
    {
        let mut storage = Align([0u8; 32]);
        let mut alloc = [MaybeUninit::<u8>::uninit(); 64];
        let mut raw = [MaybeUninit::<u8>::uninit(); 32];
        let buf = to_bytes_in_with_alloc::<_, _, Error>(
            value,
            Buffer::from(&mut raw[..]),
            SubAllocator::new(&mut alloc),
        )
        .unwrap();
        let len = buf.len();
        storage.0[..len].copy_from_slice(&buf);
        (storage, len)
    }

    fn samples() -> [DateTime<Tz>; 5] {
        [
            London.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap(),
            London.with_ymd_and_hms(2024, 7, 15, 12, 0, 0).unwrap(),
            New_York
                .with_ymd_and_hms(2024, 11, 3, 1, 30, 0)
                .earliest()
                .unwrap(),
            GMTPlus1.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap(),
            UTC.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        ]
    }

    #[test]
    fn no_alloc_low_api_roundtrip_tz_offset_and_datetime() {
        for dt in samples() {
            let tz = dt.timezone();
            let (tz_buf, tz_len) = archive_no_alloc(&tz);
            assert_eq!(tz_len, 2);
            assert_eq!(
                access::<ArchivedTz, Error>(&tz_buf.0[..tz_len]).unwrap(),
                &tz
            );
            assert_eq!(from_bytes::<Tz, Error>(&tz_buf.0[..tz_len]).unwrap(), tz);

            let off = *dt.offset();
            let (off_buf, off_len) = archive_no_alloc(&off);
            assert_eq!(off_len, 4);
            assert_eq!(
                access::<ArchivedTzOffset, Error>(&off_buf.0[..off_len]).unwrap(),
                &off
            );
            let decoded_off = from_bytes::<TzOffset, Error>(&off_buf.0[..off_len]).unwrap();
            assert_eq!(decoded_off, off);
            assert_eq!(decoded_off.valid_starting(), off.valid_starting());
            assert_eq!(decoded_off.valid_until(), off.valid_until());

            let (dt_buf, dt_len) = archive_no_alloc(&dt);
            assert_eq!(dt_len, 16);
            let archived_dt =
                access::<rkyv::Archived<DateTime<Tz>>, Error>(&dt_buf.0[..dt_len]).unwrap();
            assert_eq!(archived_dt, &dt);
            let decoded_dt = from_bytes::<DateTime<Tz>, Error>(&dt_buf.0[..dt_len]).unwrap();
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
    fn no_alloc_low_api_datetime_tz_rejects_corrupted_tz_span_local_range_and_misaligned() {
        let dt = London.with_ymd_and_hms(2024, 7, 15, 12, 0, 0).unwrap();
        let (bytes, len) = archive_no_alloc(&dt);
        assert_eq!(len, 16);

        // Misaligned DateTime<Tz> buffer must be rejected.
        let mut misaligned = Align([0u8; 32]);
        misaligned.0[1..17].copy_from_slice(&bytes.0[..16]);
        assert!(access::<rkyv::Archived<DateTime<Tz>>, Error>(&misaligned.0[1..17]).is_err());
        assert!(from_bytes::<DateTime<Tz>, Error>(&misaligned.0[1..17]).is_err());

        // ArchivedDateTime<Tz> layout: NaiveDateTime (12 bytes) + ArchivedTzOffset (4 bytes: tz at 12..14, index at 14..16).
        // 1. Out-of-range Tz discriminant inside DateTime<Tz>
        for bad_disc in [TZ_VARIANTS.len() as u16, u16::MAX] {
            let mut tampered = bytes;
            tampered.0[12..14].copy_from_slice(&bad_disc.to_le_bytes());
            assert!(access::<rkyv::Archived<DateTime<Tz>>, Error>(&tampered.0[..16]).is_err());
            assert!(from_bytes::<DateTime<Tz>, Error>(&tampered.0[..16]).is_err());
        }

        // 2. Out-of-bounds TzOffset span index inside DateTime<Tz>
        let oob_span = (London.timespans_len_for_test() + 1) as u16;
        for bad_idx in [oob_span, u16::MAX] {
            let mut tampered = bytes;
            tampered.0[14..16].copy_from_slice(&bad_idx.to_le_bytes());
            assert!(access::<rkyv::Archived<DateTime<Tz>>, Error>(&tampered.0[..16]).is_err());
            assert!(from_bytes::<DateTime<Tz>, Error>(&tampered.0[..16]).is_err());
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
        let (oor_bytes, oor_len) = archive_no_alloc(&out_of_range);
        assert!(access::<rkyv::Archived<DateTime<Tz>>, Error>(&oor_bytes.0[..oor_len]).is_err());
        assert!(from_bytes::<DateTime<Tz>, Error>(&oor_bytes.0[..oor_len]).is_err());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn alloc_roundtrip_and_tamper_checks() {
        use rkyv::{access as alloc_access, from_bytes as alloc_from_bytes, to_bytes};

        for dt in samples() {
            let dt_bytes = to_bytes::<Error>(&dt).unwrap();
            let archived_dt =
                alloc_access::<rkyv::Archived<DateTime<Tz>>, Error>(&dt_bytes).unwrap();
            assert_eq!(archived_dt, &dt);
            assert_eq!(
                alloc_from_bytes::<DateTime<Tz>, Error>(&dt_bytes).unwrap(),
                dt
            );
        }

        let dt = London.with_ymd_and_hms(2024, 7, 15, 12, 0, 0).unwrap();
        let bytes = to_bytes::<Error>(&dt).unwrap();
        let mut tampered = bytes.clone();
        tampered[12..14].copy_from_slice(&(TZ_VARIANTS.len() as u16).to_le_bytes());
        assert!(alloc_access::<rkyv::Archived<DateTime<Tz>>, Error>(&tampered).is_err());
        assert!(alloc_from_bytes::<DateTime<Tz>, Error>(&tampered).is_err());
    }

    #[test]
    fn golden_wire_bytes() {
        // Tz::Africa__Abidjan == 0 -> [0x00, 0x00]
        let (abidjan_buf, abidjan_len) = archive_no_alloc(&Tz::Africa__Abidjan);
        assert_eq!(&abidjan_buf.0[..abidjan_len], &[0x00, 0x00]);

        // Nonzero zone ordinal: America::New_York == 170 -> [0xaa, 0x00]
        let (ny_buf, ny_len) = archive_no_alloc(&New_York);
        assert_eq!(&ny_buf.0[..ny_len], &[0xaa, 0x00]);

        // Nonzero zone ordinal + nonzero span index: New_York 1918-04-01 EDT (span index 2) -> [0xaa, 0x00, 0x02, 0x00]
        let ny_1918 = New_York.with_ymd_and_hms(1918, 4, 1, 12, 0, 0).unwrap();
        let (ny_off_buf, ny_off_len) = archive_no_alloc(ny_1918.offset());
        assert_eq!(&ny_off_buf.0[..ny_off_len], &[0xaa, 0x00, 0x02, 0x00]);
        let decoded_ny_off = from_bytes::<TzOffset, Error>(&ny_off_buf.0[..ny_off_len]).unwrap();
        assert_eq!(decoded_ny_off, *ny_1918.offset());

        // Tz::UTC == 592 -> [0x50, 0x02], UTC TzOffset (span index 0) -> [0x50, 0x02, 0x00, 0x00]
        let (utc_buf, utc_len) = archive_no_alloc(&Tz::UTC);
        assert_eq!(&utc_buf.0[..utc_len], &[0x50, 0x02]);
        let utc_off = *UTC.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap().offset();
        let (utc_off_buf, utc_off_len) = archive_no_alloc(&utc_off);
        assert_eq!(&utc_off_buf.0[..utc_off_len], &[0x50, 0x02, 0x00, 0x00]);
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
