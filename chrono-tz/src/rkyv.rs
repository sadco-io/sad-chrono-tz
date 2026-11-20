use core::{error::Error, fmt};

use rkyv::{
    bytecheck::CheckBytes,
    munge::munge,
    rancor::{fail, Fallible, Source},
    traits::NoUndef,
    Archive, Archived, Deserialize, Place, Portable, Serialize,
};

use crate::timezone_impl::{TimeSpans, TzOffset};
use crate::timezones::{Tz, TZ_VARIANTS};

/// Error returned when validating or deserializing an out-of-range [`ArchivedTz`].
#[derive(Debug)]
struct InvalidArchivedTzError;

impl fmt::Display for InvalidArchivedTzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("archived Tz discriminant is out of range for TZ_VARIANTS")
    }
}

impl Error for InvalidArchivedTzError {}

/// Error returned when validating or deserializing an out-of-range [`ArchivedTzOffset`].
#[derive(Debug)]
struct InvalidArchivedTzOffsetError;

impl fmt::Display for InvalidArchivedTzOffsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("archived TzOffset span index is out of range for timezone")
    }
}

impl Error for InvalidArchivedTzOffsetError {}

/// Archived representation of [`Tz`].
///
/// `ArchivedTz` stores a 16-bit zone ordinal. `ArchivedTzOffset` also stores a 16-bit span index.
/// Both fields are little-endian by default. Decode with the same rkyv format configuration and
/// matching ordered zone and transition tables. An in-range index can resolve differently after a
/// filter or tzdb change.
///
/// Applications requiring cross-build table rejection should verify a fingerprint of the
/// generated zone and transition tables before decoding ([`IANA_TZDB_VERSION`](crate::IANA_TZDB_VERSION)
/// alone is insufficient because filtered and full builds share the same tzdb version string). For
/// `sad_chrono::DateTime<Tz>`, `CheckBytes` verification checks local-time representability under
/// `sad-chrono`'s stored-offset contract rather than cross-build table identity.
///
/// [`CheckBytes`] fails closed for any discriminant `>= TZ_VARIANTS.len()`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Portable)]
#[repr(transparent)]
pub struct ArchivedTz(Archived<u16>);

// SAFETY: `ArchivedTz` is `#[repr(transparent)]` over `Archived<u16>`, which implements `NoUndef`
// and has no padding bytes.
unsafe impl NoUndef for ArchivedTz {}

impl ArchivedTz {
    /// Returns the native variant index stored in this archived value.
    #[inline]
    pub const fn discriminant(&self) -> u16 {
        self.0.to_native()
    }

    /// Resolves this archived value back to a [`Tz`], returning `None` if the discriminant is
    /// out of range for the compiled [`TZ_VARIANTS`].
    #[inline]
    pub fn to_tz(&self) -> Option<Tz> {
        TZ_VARIANTS.get(usize::from(self.discriminant())).copied()
    }
}

impl fmt::Debug for ArchivedTz {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_tz() {
            Some(tz) => fmt::Debug::fmt(&tz, f),
            None => f
                .debug_tuple("ArchivedTz::Invalid")
                .field(&self.discriminant())
                .finish(),
        }
    }
}

impl PartialEq<Tz> for ArchivedTz {
    #[inline]
    fn eq(&self, other: &Tz) -> bool {
        usize::from(self.discriminant()) == (*other as usize)
    }
}

impl PartialEq<ArchivedTz> for Tz {
    #[inline]
    fn eq(&self, other: &ArchivedTz) -> bool {
        other == self
    }
}

impl Archive for Tz {
    type Archived = ArchivedTz;
    type Resolver = ();

    #[inline]
    fn resolve(&self, _resolver: Self::Resolver, out: Place<Self::Archived>) {
        munge!(let ArchivedTz(disc_out) = out);
        u16::resolve(&(*self as u16), (), disc_out);
    }
}

impl<S: Fallible + ?Sized> Serialize<S> for Tz {
    #[inline]
    fn serialize(&self, _serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        Ok(())
    }
}

impl<D: Fallible + ?Sized> Deserialize<Tz, D> for ArchivedTz
where
    D::Error: Source,
{
    fn deserialize(&self, _deserializer: &mut D) -> Result<Tz, D::Error> {
        self.to_tz()
            .ok_or_else(|| D::Error::new(InvalidArchivedTzError))
    }
}

// SAFETY: `check_bytes` validates the underlying `Archived<u16>` and rejects every discriminant
// `>= TZ_VARIANTS.len()`, failing closed before any `Tz` can be observed.
unsafe impl<C: Fallible + ?Sized> CheckBytes<C> for ArchivedTz
where
    C::Error: Source,
{
    unsafe fn check_bytes(value: *const Self, context: &mut C) -> Result<(), C::Error> {
        // SAFETY: `ArchivedTz` is `#[repr(transparent)]` over `Archived<u16>`, and `value` meets
        // `CheckBytes`'s alignment and readability preconditions for `Self`.
        let disc_ptr = value.cast::<Archived<u16>>();
        // SAFETY: `disc_ptr` has the exact layout and alignment of `Archived<u16>`.
        unsafe { <Archived<u16> as CheckBytes<C>>::check_bytes(disc_ptr, context)? };
        // SAFETY: `Archived<u16>` passed `check_bytes` above, so reading it is valid.
        let disc = usize::from(unsafe { (*disc_ptr).to_native() });
        if disc >= TZ_VARIANTS.len() {
            fail!(InvalidArchivedTzError);
        }
        Ok(())
    }
}

/// Archived representation of [`TzOffset`].
///
/// Stores an [`ArchivedTz`] and the span index as an [`Archived<u16>`]. [`CheckBytes`] fails
/// closed unless both the timezone discriminant is within [`TZ_VARIANTS`] and the span index is
/// within the timezone's transition table (`<= tz.timespans().rest.len()`).
#[derive(Clone, Copy, Eq, Portable)]
#[repr(C)]
pub struct ArchivedTzOffset {
    tz: ArchivedTz,
    index: Archived<u16>,
}

// SAFETY: `ArchivedTzOffset` is `#[repr(C)]` containing two `Archived<u16>`-sized/aligned fields
// with zero padding bytes, each implementing `NoUndef`.
unsafe impl NoUndef for ArchivedTzOffset {}

impl ArchivedTzOffset {
    /// Resolves this archived offset back to a [`TzOffset`], returning `None` if either the
    /// timezone discriminant or the transition span index is out of bounds.
    pub fn to_tz_offset(&self) -> Option<TzOffset> {
        let tz = self.tz.to_tz()?;
        let index = self.index.to_native();
        if usize::from(index) <= tz.timespans().rest.len() {
            Some(TzOffset { tz, index })
        } else {
            None
        }
    }
}

impl PartialEq for ArchivedTzOffset {
    fn eq(&self, other: &Self) -> bool {
        match (self.to_tz_offset(), other.to_tz_offset()) {
            (Some(a), Some(b)) => a == b,
            _ => self.tz == other.tz && self.index == other.index,
        }
    }
}

impl PartialEq<TzOffset> for ArchivedTzOffset {
    fn eq(&self, other: &TzOffset) -> bool {
        self.to_tz_offset().is_some_and(|off| off == *other)
    }
}

impl PartialEq<ArchivedTzOffset> for TzOffset {
    fn eq(&self, other: &ArchivedTzOffset) -> bool {
        other == self
    }
}

impl fmt::Debug for ArchivedTzOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_tz_offset() {
            Some(off) => fmt::Debug::fmt(&off, f),
            None => f
                .debug_struct("ArchivedTzOffset::Invalid")
                .field("tz", &self.tz)
                .field("index", &self.index.to_native())
                .finish(),
        }
    }
}

impl Archive for TzOffset {
    type Archived = ArchivedTzOffset;
    type Resolver = ();

    #[inline]
    fn resolve(&self, _resolver: Self::Resolver, out: Place<Self::Archived>) {
        munge!(let ArchivedTzOffset { tz: tz_out, index: index_out } = out);
        self.tz.resolve((), tz_out);
        u16::resolve(&self.index, (), index_out);
    }
}

impl<S: Fallible + ?Sized> Serialize<S> for TzOffset {
    #[inline]
    fn serialize(&self, _serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        Ok(())
    }
}

impl<D: Fallible + ?Sized> Deserialize<TzOffset, D> for ArchivedTzOffset
where
    D::Error: Source,
{
    fn deserialize(&self, _deserializer: &mut D) -> Result<TzOffset, D::Error> {
        self.to_tz_offset()
            .ok_or_else(|| D::Error::new(InvalidArchivedTzOffsetError))
    }
}

// SAFETY: `check_bytes` validates `tz` (guaranteeing `tz` is in `TZ_VARIANTS`) and `index`, and
// rejects any span index `> tz.timespans().rest.len()`, preventing out-of-bounds span indexing.
unsafe impl<C: Fallible + ?Sized> CheckBytes<C> for ArchivedTzOffset
where
    C::Error: Source,
{
    unsafe fn check_bytes(value: *const Self, context: &mut C) -> Result<(), C::Error> {
        // SAFETY: `value` is aligned and readable for `Self` (`#[repr(C)]`), so projecting to
        // field pointers via `addr_of!` stays in bounds without creating intermediate references.
        let tz_ptr = unsafe { core::ptr::addr_of!((*value).tz) };
        // SAFETY: `tz_ptr` points to properly aligned `ArchivedTz`.
        unsafe { <ArchivedTz as CheckBytes<C>>::check_bytes(tz_ptr, context)? };

        // SAFETY: `index` is a field of `Self` at `value`.
        let index_ptr = unsafe { core::ptr::addr_of!((*value).index) };
        // SAFETY: `index_ptr` points to properly aligned `Archived<u16>`.
        unsafe { <Archived<u16> as CheckBytes<C>>::check_bytes(index_ptr, context)? };

        // SAFETY: Both fields passed `check_bytes` above, so reading their native values is valid.
        let tz =
            unsafe { (*tz_ptr).to_tz() }.ok_or_else(|| C::Error::new(InvalidArchivedTzError))?;
        // SAFETY: `index_ptr` passed `check_bytes` above.
        let span_idx = usize::from(unsafe { (*index_ptr).to_native() });
        if span_idx > tz.timespans().rest.len() {
            fail!(InvalidArchivedTzOffsetError);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::mem::MaybeUninit;

    use rkyv::{
        api::low::{access, from_bytes, to_bytes_in_with_alloc},
        rancor::Failure,
        ser::{allocator::SubAllocator, writer::Buffer},
        util::Align,
    };

    use super::*;
    use crate::{OffsetComponents, OffsetName};

    fn archive_no_alloc<T>(value: &T) -> Align<[u8; 16]>
    where
        T: for<'a> Serialize<rkyv::api::low::LowSerializer<Buffer<'a>, SubAllocator<'a>, Failure>>,
    {
        let mut storage = Align([0u8; 16]);
        let mut alloc = [MaybeUninit::<u8>::uninit(); 64];
        let mut raw = [MaybeUninit::<u8>::uninit(); 16];
        let buf = to_bytes_in_with_alloc::<_, _, Failure>(
            value,
            Buffer::from(&mut raw[..]),
            SubAllocator::new(&mut alloc),
        )
        .unwrap();
        let len = buf.len();
        storage.0[..len].copy_from_slice(&buf);
        storage
    }

    #[test]
    fn no_alloc_roundtrip_and_discriminant_bytes() {
        let len = core::mem::size_of::<ArchivedTz>();
        assert_eq!(len, 2);

        // Exhaustively round-trip every compiled Tz variant via rkyv::api::low (zero alloc)
        for (expected_idx, &tz) in TZ_VARIANTS.iter().enumerate() {
            let buf = archive_no_alloc(&tz);
            let accessed = access::<ArchivedTz, Failure>(&buf.0[..len]).unwrap();
            assert_eq!(*accessed, tz);
            assert_eq!(accessed.discriminant(), expected_idx as u16);
            assert_eq!(from_bytes::<Tz, Failure>(&buf.0[..len]).unwrap(), tz);
        }
    }

    #[test]
    fn reject_out_of_range_tz_discriminant() {
        let len = core::mem::size_of::<ArchivedTz>();
        for bad_disc in [TZ_VARIANTS.len() as u16, u16::MAX] {
            let bad = ArchivedTz(Archived::<u16>::from_native(bad_disc));
            // SAFETY: `ArchivedTz` is `#[repr(transparent)]` over `Archived<u16>` (2 bytes, NoUndef).
            let bytes = unsafe { core::slice::from_raw_parts((&raw const bad).cast::<u8>(), len) };
            assert!(access::<ArchivedTz, Failure>(bytes).is_err());
            assert!(from_bytes::<Tz, Failure>(bytes).is_err());
        }
    }

    #[test]
    fn tz_offset_no_alloc_roundtrip_and_fail_closed() {
        let off_len = core::mem::size_of::<ArchivedTzOffset>();
        assert_eq!(off_len, 4);

        for &tz in TZ_VARIANTS.iter() {
            let first_off = *sad_chrono::DateTime::<sad_chrono::Utc>::MIN_UTC
                .with_timezone(&tz)
                .offset();
            let last_off = *sad_chrono::DateTime::<sad_chrono::Utc>::MAX_UTC
                .with_timezone(&tz)
                .offset();

            for off in [first_off, last_off] {
                let buf = archive_no_alloc(&off);
                let accessed = access::<ArchivedTzOffset, Failure>(&buf.0[..off_len]).unwrap();
                assert_eq!(*accessed, off);
                let decoded = from_bytes::<TzOffset, Failure>(&buf.0[..off_len]).unwrap();
                assert_eq!(decoded, off);
                assert_eq!(decoded.tz_id(), off.tz_id());
                assert_eq!(decoded.abbreviation(), off.abbreviation());
                assert_eq!(decoded.base_utc_offset(), off.base_utc_offset());
                assert_eq!(decoded.dst_offset(), off.dst_offset());
                assert_eq!(decoded.valid_starting(), off.valid_starting());
                assert_eq!(decoded.valid_until(), off.valid_until());
            }

            // Out-of-bounds span index (rest.len() + 1 and u16::MAX) must fail closed
            let oob_idx = u16::try_from(tz.timespans().rest.len() + 1).unwrap_or(u16::MAX);
            for bad_idx in [oob_idx, u16::MAX] {
                let bad = ArchivedTzOffset {
                    tz: ArchivedTz(Archived::<u16>::from_native(tz as u16)),
                    index: Archived::<u16>::from_native(bad_idx),
                };
                // SAFETY: `ArchivedTzOffset` is `#[repr(C)]` with zero padding (4 bytes, NoUndef).
                let bytes =
                    unsafe { core::slice::from_raw_parts((&raw const bad).cast::<u8>(), off_len) };
                assert!(access::<ArchivedTzOffset, Failure>(bytes).is_err());
                assert!(from_bytes::<TzOffset, Failure>(bytes).is_err());
            }
        }

        // Invalid Tz inside ArchivedTzOffset must also fail closed
        for bad_disc in [TZ_VARIANTS.len() as u16, u16::MAX] {
            let bad = ArchivedTzOffset {
                tz: ArchivedTz(Archived::<u16>::from_native(bad_disc)),
                index: Archived::<u16>::from_native(0),
            };
            // SAFETY: `ArchivedTzOffset` is `#[repr(C)]` with zero padding (4 bytes, NoUndef).
            let bytes =
                unsafe { core::slice::from_raw_parts((&raw const bad).cast::<u8>(), off_len) };
            assert!(access::<ArchivedTzOffset, Failure>(bytes).is_err());
            assert!(from_bytes::<TzOffset, Failure>(bytes).is_err());
        }
    }

    #[cfg(not(feature = "filter-by-regex"))]
    #[test]
    fn golden_wire_bytes_pinned() {
        use sad_chrono::TimeZone;

        // Nonzero zone ordinal: America::New_York == 170 -> [0xaa, 0x00] (little-endian u16).
        let ny = Tz::America__New_York;
        assert_eq!(ny as u16, 170);
        let tz_buf = archive_no_alloc(&ny);
        let accessed_tz = access::<ArchivedTz, Failure>(&tz_buf.0[..2]).unwrap();
        assert_eq!(&tz_buf.0[..2], &[0xaa, 0x00]);
        assert_eq!(accessed_tz.discriminant(), 170);
        assert_eq!(*accessed_tz, ny);

        // Nonzero zone ordinal + nonzero span index: New_York 1918-04-01 EDT == span index 2 -> [0xaa, 0x00, 0x02, 0x00].
        let dt = ny.with_ymd_and_hms(1918, 4, 1, 12, 0, 0).unwrap();
        let off = *dt.offset();
        assert_eq!(off.index, 2);
        let off_buf = archive_no_alloc(&off);
        let accessed_off = access::<ArchivedTzOffset, Failure>(&off_buf.0[..4]).unwrap();
        assert_eq!(&off_buf.0[..4], &[0xaa, 0x00, 0x02, 0x00]);
        assert_eq!(accessed_off.tz.discriminant(), 170);
        assert_eq!(accessed_off.index.to_native(), 2);
        assert_eq!(*accessed_off, off);
    }
}
