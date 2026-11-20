#![no_main]
use libfuzzer_sys::fuzz_target;
use rkyv::{
    api::low::{access, from_bytes},
    rancor::Failure,
    util::Align,
};
use sad_chrono::Offset;
use sad_chrono_tz::{ArchivedTz, ArchivedTzOffset, OffsetComponents, OffsetName, Tz, TzOffset};

fn exercise_tz(tz: Tz) {
    let _ = tz.name();
}

fn exercise_offset(off: TzOffset) {
    let _ = off.fix();
    let _ = off.tz_id();
    let _ = off.abbreviation();
    let _ = off.base_utc_offset();
    let _ = off.dst_offset();
    let _ = off.valid_starting();
    let _ = off.valid_until();
    let _ = off.next_offset();
    let _ = off.prev_offset();
    let _ = off.to_string();
}

fuzz_target!(|data: &[u8]| {
    // 1. Direct slice access (exercises alignment + length + bytecheck validation)
    if let Ok(archived) = access::<ArchivedTz, Failure>(data) {
        let _ = archived.to_tz();
        if let Ok(tz) = from_bytes::<Tz, Failure>(data) {
            exercise_tz(tz);
        }
    }
    if let Ok(archived) = access::<ArchivedTzOffset, Failure>(data) {
        let _ = archived.to_tz_offset();
        if let Ok(off) = from_bytes::<TzOffset, Failure>(data) {
            exercise_offset(off);
        }
    }

    // 2. Aligned fixed-size access (reaches discriminant/span-index checks even on unaligned slices)
    if data.len() >= 2 {
        let mut aligned2 = Align([0u8; 2]);
        aligned2.0.copy_from_slice(&data[..2]);
        if let Ok(tz) = from_bytes::<Tz, Failure>(&aligned2.0) {
            exercise_tz(tz);
        }
    }
    if data.len() >= 4 {
        let mut aligned4 = Align([0u8; 4]);
        aligned4.0.copy_from_slice(&data[..4]);
        if let Ok(off) = from_bytes::<TzOffset, Failure>(&aligned4.0) {
            exercise_offset(off);
        }
    }
});
