use core::cmp::Ordering;
use core::fmt::{Debug, Display, Error, Formatter, Write};

use sad_chrono::{
    DateTime, Duration, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Offset,
    TimeZone, Utc,
};

use crate::binary_search::binary_search;
use crate::timezones::{Tz, ABBREVIATIONS};

/// An Offset that applies for a period of time
///
/// For example, [`::US::Eastern`] is composed of at least two
/// `FixedTimespan`s: `EST` and `EDT`, that are variously in effect.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) struct FixedTimespan {
    /// Packs `utc_offset` (bits 20..40, 20-bit signed) and `dst_offset` (bits 0..20, 20-bit signed)
    /// into 5 little-endian bytes, keeping `FixedTimespan` at 8 bytes with zero padding.
    pub(crate) offset: [u8; 5],
    /// Byte length into `ABBREVIATIONS`, or `u8::MAX` when no explicit abbreviation exists (`%z`).
    pub(crate) abbr_len: u8,
    /// Byte offset into `ABBREVIATIONS` when `abbr_len != u8::MAX`.
    pub(crate) abbr_start: u16,
}

const _: () = assert!(core::mem::size_of::<FixedTimespan>() == 8);

impl FixedTimespan {
    #[inline]
    const fn packed_u64(&self) -> u64 {
        (self.offset[0] as u64)
            | ((self.offset[1] as u64) << 8)
            | ((self.offset[2] as u64) << 16)
            | ((self.offset[3] as u64) << 24)
            | ((self.offset[4] as u64) << 32)
    }

    #[inline]
    pub(crate) const fn utc_offset(&self) -> i32 {
        (((self.packed_u64() as i64) << 24) >> 44) as i32
    }

    #[inline]
    pub(crate) const fn dst_offset(&self) -> i32 {
        (((self.packed_u64() as i64) << 44) >> 44) as i32
    }

    #[inline]
    pub(crate) fn name(&self) -> Option<&'static str> {
        if self.abbr_len == u8::MAX {
            None
        } else {
            let start = self.abbr_start as usize;
            let len = self.abbr_len as usize;
            Some(&ABBREVIATIONS[start..start + len])
        }
    }

    #[cfg(test)]
    const fn for_test(utc_offset: i32, dst_offset: i32, abbr_start: u16, abbr_len: u8) -> Self {
        let packed = (((utc_offset as i64 as u64) & 0xF_FFFF) << 20)
            | ((dst_offset as i64 as u64) & 0xF_FFFF);
        Self {
            offset: [
                packed as u8,
                (packed >> 8) as u8,
                (packed >> 16) as u8,
                (packed >> 24) as u8,
                (packed >> 32) as u8,
            ],
            abbr_len,
            abbr_start,
        }
    }
}

impl Offset for FixedTimespan {
    fn fix(&self) -> FixedOffset {
        FixedOffset::east_opt(self.utc_offset().saturating_add(self.dst_offset())).unwrap()
    }
}

impl Display for FixedTimespan {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        if let Some(name) = self.name() {
            return write!(f, "{name}");
        }
        let offset = self.utc_offset() as i64 + self.dst_offset() as i64;
        let sign = if offset < 0 { '-' } else { '+' };
        let off = offset.unsigned_abs();

        let minutes = off / 60;
        let secs = off % 60;
        let mins = minutes % 60;
        let hours = minutes / 60;

        f.write_char(sign)?;
        write!(f, "{hours:02}")?;
        if secs != 0 {
            write!(f, "{mins:02}{secs:02}")?;
        } else if mins != 0 {
            write!(f, "{mins:02}")?;
        }
        Ok(())
    }
}

impl Debug for FixedTimespan {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        Display::fmt(self, f)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for FixedTimespan {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(f, "{:?}", self.name());
    }
}

/// Offset for a specific transition span within a [`Tz`] timezone.
///
/// Validity bounds ([`TzOffset::valid_starting`], [`TzOffset::valid_until`]) and span traversal
/// ([`TzOffset::next_offset`], [`TzOffset::prev_offset`], [`TzOffset::iter`]) reflect the
/// generated transition table for the timezone: span starts are inclusive and span ends are
/// exclusive. Recurring rules in `sad-parse-zoneinfo` expand over `1800..2100` (`2100` excluded);
/// explicit zone boundaries are processed separately, and lookups extrapolate the outer spans
/// (for which `valid_starting()` on the first span or `valid_until()` on the final span returns
/// `None`).
///
/// `PartialEq` for `TzOffset` compares the timezone, both offset components, and abbreviation, so
/// equal `TzOffset` values (such as winter offsets from different years) can still have different
/// validity bounds.
#[derive(Copy, Clone, Eq)]
pub struct TzOffset {
    pub(crate) tz: Tz,
    pub(crate) index: u16,
}

impl PartialEq for TzOffset {
    fn eq(&self, other: &Self) -> bool {
        self.tz == other.tz && (self.index == other.index || self.timespan() == other.timespan())
    }
}

/// Detailed timezone offset components that expose any special conditions currently in effect.
///
/// This trait breaks down an offset into the standard UTC offset and any special offset
/// in effect (such as DST) at a given time.
///
/// ```
/// # #[cfg(not(feature = "filter-by-regex"))]
/// # {
/// use sad_chrono::{Duration, Offset, TimeZone};
/// use sad_chrono_tz::Europe::London;
/// use sad_chrono_tz::OffsetComponents;
///
/// let london_time = London.with_ymd_and_hms(2016, 5, 10, 12, 0, 0).unwrap();
///
/// // London typically has zero offset from UTC, but has a 1h adjustment forward
/// // when summer time is in effect.
/// let lon_utc_offset = london_time.offset().base_utc_offset();
/// let lon_dst_offset = london_time.offset().dst_offset();
/// let total_offset = lon_utc_offset + lon_dst_offset;
/// assert_eq!(lon_utc_offset, Duration::hours(0));
/// assert_eq!(lon_dst_offset, Duration::hours(1));
///
/// // As a sanity check, make sure that the total offsets added together are equivalent to the
/// // total fixed offset.
/// assert_eq!(total_offset.num_seconds(), london_time.offset().fix().local_minus_utc() as i64);
/// # }
/// ```
pub trait OffsetComponents {
    /// The base offset from UTC; this usually doesn't change unless the government changes something
    fn base_utc_offset(&self) -> Duration;
    /// The additional offset from UTC that is currently in effect; typically for daylight saving time
    fn dst_offset(&self) -> Duration;
}

/// Timezone offset name information.
///
/// This trait exposes display names that describe an offset in
/// various situations.
///
/// ```
/// # #[cfg(not(feature = "filter-by-regex"))]
/// # {
/// use sad_chrono::TimeZone;
/// use sad_chrono_tz::Europe::London;
/// use sad_chrono_tz::OffsetName;
///
/// let london_time = London.with_ymd_and_hms(2016, 2, 10, 12, 0, 0).unwrap();
/// assert_eq!(london_time.offset().tz_id(), "Europe/London");
/// // London is normally on GMT
/// assert_eq!(london_time.offset().abbreviation(), Some("GMT"));
///
/// let london_summer_time = London.with_ymd_and_hms(2016, 5, 10, 12, 0, 0).unwrap();
/// // The TZ ID remains constant year round
/// assert_eq!(london_summer_time.offset().tz_id(), "Europe/London");
/// // During the summer, this becomes British Summer Time
/// assert_eq!(london_summer_time.offset().abbreviation(), Some("BST"));
/// # }
/// ```
pub trait OffsetName {
    /// The IANA TZDB identifier (ex: America/New_York)
    fn tz_id(&self) -> &str;
    /// The abbreviation to use in a longer timestamp (ex: EST)
    ///
    /// This takes into account any special offsets that may be in effect.
    /// For example, at a given instant, the time zone with ID *America/New_York*
    /// may be either *EST* or *EDT*.
    fn abbreviation(&self) -> Option<&str>;
}

impl TzOffset {
    #[inline]
    fn new(tz: Tz, index: usize) -> Self {
        debug_assert!(u16::try_from(index).is_ok());
        TzOffset {
            tz,
            index: index as u16,
        }
    }

    fn map_localresult(tz: Tz, result: LocalResult<usize>) -> LocalResult<Self> {
        match result {
            LocalResult::None => LocalResult::None,
            LocalResult::Single(s) => LocalResult::Single(TzOffset::new(tz, s)),
            LocalResult::Ambiguous(a, b) => {
                LocalResult::Ambiguous(TzOffset::new(tz, a), TzOffset::new(tz, b))
            }
        }
    }

    #[inline]
    pub(crate) fn timespans(&self) -> FixedTimespanSet {
        self.tz.timespans()
    }

    #[inline]
    pub(crate) fn timespan(&self) -> FixedTimespan {
        self.timespans().get(self.index as usize)
    }

    /// Returns the UTC instant at which this offset became valid, or `None` for the initial
    /// open-ended span of the timezone.
    ///
    /// ```
    /// # #[cfg(not(feature = "filter-by-regex"))]
    /// # {
    /// use sad_chrono::{NaiveDate, TimeZone};
    /// use sad_chrono_tz::America::New_York;
    ///
    /// let dt = New_York.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap();
    /// let offset = dt.offset();
    /// let start = offset.valid_starting().unwrap();
    /// assert_eq!(start, NaiveDate::from_ymd_opt(2024, 3, 10).unwrap().and_hms_opt(7, 0, 0).unwrap().and_utc());
    /// # }
    /// ```
    pub fn valid_starting(&self) -> Option<DateTime<Utc>> {
        let rest_idx = usize::from(self.index).checked_sub(1)?;
        let (ts, _) = self.timespans().rest.get(rest_idx)?;
        DateTime::from_timestamp(*ts, 0)
    }

    /// Returns the exclusive UTC instant at which this offset ceases to be valid, or `None` for
    /// the final open-ended span of the timezone.
    ///
    /// When `Some(until)` is returned, `until` equals `self.next_offset()?.valid_starting()`.
    ///
    /// ```
    /// # #[cfg(not(feature = "filter-by-regex"))]
    /// # {
    /// use sad_chrono::{NaiveDate, TimeZone};
    /// use sad_chrono_tz::America::New_York;
    ///
    /// let dt = New_York.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap();
    /// let offset = dt.offset();
    /// assert_eq!(offset.valid_until(), offset.next_offset().unwrap().valid_starting());
    /// assert_eq!(
    ///     offset.valid_until().unwrap(),
    ///     NaiveDate::from_ymd_opt(2024, 11, 3).unwrap().and_hms_opt(6, 0, 0).unwrap().and_utc()
    /// );
    /// # }
    /// ```
    pub fn valid_until(&self) -> Option<DateTime<Utc>> {
        let (ts, _) = self.timespans().rest.get(usize::from(self.index))?;
        DateTime::from_timestamp(*ts, 0)
    }

    /// Returns the next offset span in this timezone, or `None` if this is the final span.
    ///
    /// ```
    /// # #[cfg(not(feature = "filter-by-regex"))]
    /// # {
    /// use sad_chrono::TimeZone;
    /// use sad_chrono_tz::Europe::London;
    /// use sad_chrono_tz::OffsetName;
    ///
    /// let winter = London.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
    /// let next = winter.offset().next_offset().unwrap();
    /// assert_eq!(next.abbreviation(), Some("BST"));
    /// # }
    /// ```
    pub fn next_offset(&self) -> Option<Self> {
        let next_idx = self.index.checked_add(1)?;
        if usize::from(next_idx) < self.timespans().len() {
            Some(Self {
                tz: self.tz,
                index: next_idx,
            })
        } else {
            None
        }
    }

    /// Returns the previous offset span in this timezone, or `None` if this is the initial span.
    ///
    /// ```
    /// # #[cfg(not(feature = "filter-by-regex"))]
    /// # {
    /// use sad_chrono::TimeZone;
    /// use sad_chrono_tz::Europe::London;
    /// use sad_chrono_tz::OffsetName;
    ///
    /// let summer = London.with_ymd_and_hms(2024, 7, 15, 12, 0, 0).unwrap();
    /// let prev = summer.offset().prev_offset().unwrap();
    /// assert_eq!(prev.abbreviation(), Some("GMT"));
    /// # }
    /// ```
    pub fn prev_offset(&self) -> Option<Self> {
        self.index
            .checked_sub(1)
            .map(|index| Self { tz: self.tz, index })
    }

    /// Returns an iterator over this offset and all remaining offset spans for the timezone.
    ///
    /// ```
    /// # #[cfg(not(feature = "filter-by-regex"))]
    /// # {
    /// use sad_chrono::TimeZone;
    /// use sad_chrono_tz::UTC;
    ///
    /// let dt = UTC.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    /// assert_eq!(dt.offset().iter().count(), 1);
    /// # }
    /// ```
    pub fn iter(&self) -> TzOffsetIter {
        TzOffsetIter(Some(*self))
    }
}

/// Iterator over consecutive [`TzOffset`] spans in a timezone, starting from a given offset.
///
/// `PartialEq` compares the iterator cursor position `(tz, index)` (or exhausted state), unlike
/// [`TzOffset::eq`] which compares offset values.
#[derive(Clone, Copy, Debug, Eq)]
pub struct TzOffsetIter(Option<TzOffset>);

impl PartialEq for TzOffsetIter {
    fn eq(&self, other: &Self) -> bool {
        match (self.0, other.0) {
            (Some(a), Some(b)) => a.tz == b.tz && a.index == b.index,
            (None, None) => true,
            _ => false,
        }
    }
}

impl Iterator for TzOffsetIter {
    type Item = TzOffset;

    fn next(&mut self) -> Option<Self::Item> {
        let offset = self.0?;
        self.0 = offset.next_offset();
        Some(offset)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.0 {
            Some(off) => {
                let remaining = off.timespans().len().saturating_sub(usize::from(off.index));
                (remaining, Some(remaining))
            }
            None => (0, Some(0)),
        }
    }
}

impl ExactSizeIterator for TzOffsetIter {}
impl core::iter::FusedIterator for TzOffsetIter {}

impl OffsetComponents for TzOffset {
    fn base_utc_offset(&self) -> Duration {
        Duration::seconds(self.timespan().utc_offset() as i64)
    }

    fn dst_offset(&self) -> Duration {
        Duration::seconds(self.timespan().dst_offset() as i64)
    }
}

impl OffsetName for TzOffset {
    fn tz_id(&self) -> &str {
        self.tz.name()
    }

    fn abbreviation(&self) -> Option<&str> {
        self.timespan().name()
    }
}

impl Offset for TzOffset {
    fn fix(&self) -> FixedOffset {
        self.timespan().fix()
    }
}

impl Display for TzOffset {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        Display::fmt(&self.timespan(), f)
    }
}

impl Debug for TzOffset {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        Debug::fmt(&self.timespan(), f)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for TzOffset {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(f, "{:?}", self.timespan());
    }
}

/// Represents the span of time that a given rule is valid for.
/// Note that I have made the assumption that all ranges are
/// left-inclusive and right-exclusive - that is to say,
/// if the clocks go forward by 1 hour at 1am, the time 1am
/// does not exist in local time (the clock goes from 00:59:59
/// to 02:00:00). Likewise, if the clocks go back by one hour
/// at 2am, the clock goes from 01:59:59 to 01:00:00. This is
/// an arbitrary choice, and I could not find a source to
/// confirm whether or not this is correct.
struct Span {
    begin: Option<i64>,
    end: Option<i64>,
}

impl Span {
    fn contains(&self, x: i64) -> bool {
        match (self.begin, self.end) {
            (Some(a), Some(b)) if a <= x && x < b => true,
            (Some(a), None) if a <= x => true,
            (None, Some(b)) if b > x => true,
            (None, None) => true,
            _ => false,
        }
    }

    fn cmp(&self, x: i64) -> Ordering {
        match (self.begin, self.end) {
            (Some(a), Some(b)) if a <= x && x < b => Ordering::Equal,
            (Some(a), Some(b)) if a <= x && b <= x => Ordering::Less,
            (Some(_), Some(_)) => Ordering::Greater,
            (Some(a), None) if a <= x => Ordering::Equal,
            (Some(_), None) => Ordering::Greater,
            (None, Some(b)) if b <= x => Ordering::Less,
            (None, Some(_)) => Ordering::Equal,
            (None, None) => Ordering::Equal,
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) struct FixedTimespanSet {
    pub(crate) first: FixedTimespan,
    pub(crate) rest: &'static [(i64, FixedTimespan)],
}

impl FixedTimespanSet {
    pub(crate) fn len(&self) -> usize {
        1 + self.rest.len()
    }

    fn utc_span(&self, index: usize) -> Span {
        debug_assert!(index < self.len());
        Span {
            begin: if index == 0 {
                None
            } else {
                Some(self.rest[index - 1].0)
            },
            end: if index == self.rest.len() {
                None
            } else {
                Some(self.rest[index].0)
            },
        }
    }

    fn local_span(&self, index: usize) -> Span {
        debug_assert!(index < self.len());
        let clamp_local = |ts: i64, span: FixedTimespan| -> i64 {
            (ts as i128 + span.utc_offset() as i128 + span.dst_offset() as i128)
                .clamp(i64::MIN as i128, i64::MAX as i128) as i64
        };
        Span {
            begin: if index == 0 {
                None
            } else {
                let span = self.rest[index - 1];
                Some(clamp_local(span.0, span.1))
            },
            end: if index == self.rest.len() {
                None
            } else if index == 0 {
                Some(clamp_local(self.rest[index].0, self.first))
            } else {
                Some(clamp_local(self.rest[index].0, self.rest[index - 1].1))
            },
        }
    }

    pub(crate) fn get(&self, index: usize) -> FixedTimespan {
        debug_assert!(index < self.len());
        if index == 0 {
            self.first
        } else {
            self.rest[index - 1].1
        }
    }
}

pub(crate) trait TimeSpans {
    fn timespans(&self) -> FixedTimespanSet;
}

impl TimeZone for Tz {
    type Offset = TzOffset;

    fn from_offset(offset: &Self::Offset) -> Self {
        offset.tz
    }

    #[allow(deprecated)]
    fn offset_from_local_date(&self, local: &NaiveDate) -> LocalResult<Self::Offset> {
        let earliest = self.offset_from_local_datetime(&local.and_time(NaiveTime::MIN));
        let latest = self.offset_from_local_datetime(&local.and_hms_opt(23, 59, 59).unwrap());
        // From the chrono docs:
        //
        // > This type should be considered ambiguous at best, due to the inherent lack of
        // > precision required for the time zone resolution. There are some guarantees on the usage
        // > of `Date<Tz>`:
        // > - If properly constructed via `TimeZone::ymd` and others without an error,
        // >   the corresponding local date should exist for at least a moment.
        // >   (It may still have a gap from the offset changes.)
        //
        // > - The `TimeZone` is free to assign *any* `Offset` to the local date,
        // >   as long as that offset did occur in given day.
        // >   For example, if `2015-03-08T01:59-08:00` is followed by `2015-03-08T03:00-07:00`,
        // >   it may produce either `2015-03-08-08:00` or `2015-03-08-07:00`
        // >   but *not* `2015-03-08+00:00` and others.
        //
        // > - Once constructed as a full `DateTime`,
        // >   `DateTime::date` and other associated methods should return those for the original `Date`.
        // >   For example, if `dt = tz.ymd(y,m,d).hms(h,n,s)` were valid, `dt.date() == tz.ymd(y,m,d)`.
        //
        // > - The date is timezone-agnostic up to one day (i.e. practically always),
        // >   so the local date and UTC date should be equal for most cases
        // >   even though the raw calculation between `NaiveDate` and `Duration` may not.
        //
        // For these reasons we return always a single offset here if we can, rather than being
        // technically correct and returning Ambiguous(_,_) on days when the clock changes. The
        // alternative is painful errors when computing unambiguous times such as
        // `TimeZone.ymd(ambiguous_date).hms(unambiguous_time)`.
        use sad_chrono::LocalResult::*;
        match (earliest, latest) {
            (result @ Single(_), _) => result,
            (_, result @ Single(_)) => result,
            (Ambiguous(offset, _), _) => Single(offset),
            (_, Ambiguous(offset, _)) => Single(offset),
            (None, None) => None,
        }
    }

    // First search for a timespan that the local datetime falls into, then, if it exists,
    // check the two surrounding timespans (if they exist) to see if there is any ambiguity.
    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<Self::Offset> {
        let timestamp = local.and_utc().timestamp();
        let timespans = self.timespans();
        let index = binary_search(0, timespans.len(), |i| {
            timespans.local_span(i).cmp(timestamp)
        });
        TzOffset::map_localresult(
            *self,
            match index {
                Ok(0) if timespans.len() == 1 => LocalResult::Single(0),
                Ok(0) if timespans.local_span(1).contains(timestamp) => {
                    LocalResult::Ambiguous(0, 1)
                }
                Ok(0) => LocalResult::Single(0),
                Ok(i) if timespans.local_span(i - 1).contains(timestamp) => {
                    LocalResult::Ambiguous(i - 1, i)
                }
                Ok(i) if i == timespans.len() - 1 => LocalResult::Single(i),
                Ok(i) if timespans.local_span(i + 1).contains(timestamp) => {
                    LocalResult::Ambiguous(i, i + 1)
                }
                Ok(i) => LocalResult::Single(i),
                Err(_) => LocalResult::None,
            },
        )
    }

    #[allow(deprecated)]
    fn offset_from_utc_date(&self, utc: &NaiveDate) -> Self::Offset {
        // See comment above for why it is OK to just take any arbitrary time in the day
        self.offset_from_utc_datetime(&utc.and_time(NaiveTime::MIN))
    }

    // Binary search for the required timespan. Any i64 is guaranteed to fall within
    // exactly one timespan, no matter what (so the `unwrap` is safe).
    fn offset_from_utc_datetime(&self, dt: &NaiveDateTime) -> Self::Offset {
        let timestamp = dt.and_utc().timestamp();
        let timespans = self.timespans();
        let index =
            binary_search(0, timespans.len(), |i| timespans.utc_span(i).cmp(timestamp)).unwrap();
        TzOffset::new(*self, index)
    }
}

/// Represents the information of a gap.
///
/// This returns useful information that can be used when converting a local [`NaiveDateTime`]
/// to a timezone-aware [`DateTime`] with [`TimeZone::from_local_datetime`] and a gap
/// ([`LocalResult::None`]) is found.
pub struct GapInfo {
    /// When available it contains information about the beginning of the gap.
    ///
    /// The time represents the first instant in which the gap starts.
    /// This means that it is the first instant that when used with [`TimeZone::from_local_datetime`]
    /// it will return [`LocalResult::None`].
    ///
    /// The offset represents the offset of the first instant before the gap.
    pub begin: Option<(NaiveDateTime, TzOffset)>,
    /// When available it contains the first instant after the gap.
    pub end: Option<DateTime<Tz>>,
}

impl GapInfo {
    /// Return information about a gap.
    ///
    /// It returns `None` if `local` is not in a gap for the current timezone.
    ///
    /// If `local` is at the limits of the known timestamps the fields `begin` or `end` in
    /// [`GapInfo`] will be `None`.
    pub fn new(local: &NaiveDateTime, tz: &Tz) -> Option<Self> {
        let timestamp = local.and_utc().timestamp();
        let timespans = tz.timespans();
        let index = binary_search(0, timespans.len(), |i| {
            timespans.local_span(i).cmp(timestamp)
        });

        let Err(end_idx) = index else {
            return None;
        };

        let begin = match end_idx {
            0 => None,
            _ => {
                let start_idx = end_idx - 1;

                timespans
                    .local_span(start_idx)
                    .end
                    .and_then(|start_time| DateTime::from_timestamp(start_time, 0))
                    .map(|start_time| (start_time.naive_local(), TzOffset::new(*tz, start_idx)))
            }
        };

        let end = match end_idx {
            _ if end_idx >= timespans.len() => None,
            _ => {
                timespans
                    .local_span(end_idx)
                    .begin
                    .and_then(|end_time| DateTime::from_timestamp(end_time, 0))
                    .and_then(|date_time| {
                        // we create the DateTime from a timestamp that exists in the timezone
                        tz.from_local_datetime(&date_time.naive_local()).single()
                    })
            }
        };

        Some(Self { begin, end })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_span_extreme_no_overflow() {
        const SET_REST: &[(i64, FixedTimespan)] = &[
            (i64::MIN + 10, FixedTimespan::for_test(-3600, 0, 0, u8::MAX)),
            (
                i64::MAX - 10,
                FixedTimespan::for_test(3600, 3600, 0, u8::MAX),
            ),
        ];
        let set = FixedTimespanSet {
            first: FixedTimespan::for_test(-3600, 0, 0, u8::MAX),
            rest: SET_REST,
        };
        assert_eq!(set.local_span(0).end, Some(i64::MIN));
        assert_eq!(set.local_span(1).begin, Some(i64::MIN));
        assert_eq!(set.local_span(1).end, Some(i64::MAX - 3610));
        assert_eq!(set.local_span(2).begin, Some(i64::MAX));

        // Opposite-signed UTC and DST offsets must be combined in i128 before clamping once.
        const OPP_REST: &[(i64, FixedTimespan)] = &[
            (
                i64::MIN + 10,
                FixedTimespan::for_test(-3600, 7200, 0, u8::MAX),
            ),
            (
                i64::MAX - 10,
                FixedTimespan::for_test(3600, -7200, 0, u8::MAX),
            ),
        ];
        let opposite = FixedTimespanSet {
            first: FixedTimespan::for_test(-3600, 7200, 0, u8::MAX),
            rest: OPP_REST,
        };
        assert_eq!(opposite.local_span(0).end, Some(i64::MIN + 3610));
        assert_eq!(opposite.local_span(1).begin, Some(i64::MIN + 3610));
        assert_eq!(opposite.local_span(1).end, Some(i64::MAX));
        assert_eq!(opposite.local_span(2).begin, Some(i64::MAX - 3610));
    }

    #[test]
    fn next_prev_and_iter_validity_bounds() {
        use crate::TZ_VARIANTS;

        // Multi-transition zone (Eastern)
        #[cfg(not(feature = "filter-by-regex"))]
        {
            let eastern = Tz::US__Eastern;
            let edt = eastern.with_ymd_and_hms(2019, 11, 3, 0, 0, 0).unwrap();
            let edt_offset = *edt.offset();
            let next_offset = TzOffset::new(edt_offset.tz, usize::from(edt_offset.index) + 1);
            assert_eq!(edt_offset.next_offset(), Some(next_offset));
            assert_eq!(next_offset.prev_offset(), Some(edt_offset));
            assert_eq!(edt_offset.iter().next(), Some(edt_offset));
            assert_eq!(edt_offset.iter().nth(1), edt_offset.next_offset());
            assert_eq!(edt_offset.valid_until(), next_offset.valid_starting());

            // Check actual transition boundary behavior
            let until = edt_offset.valid_until().unwrap();
            assert_eq!(
                (until - Duration::seconds(1))
                    .with_timezone(&eastern)
                    .offset(),
                &edt_offset
            );
            assert_eq!(until.with_timezone(&eastern).offset(), &next_offset);

            // Havana 2026f skipped 2026-11-01 fallback -> stays CDT until 2027-11-07 05:00:00 UTC
            let havana = Tz::America__Havana;
            let hav_2026 = NaiveDate::from_ymd_opt(2026, 11, 2)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap()
                .and_utc()
                .with_timezone(&havana);
            let expected_until = NaiveDate::from_ymd_opt(2027, 11, 7)
                .unwrap()
                .and_hms_opt(5, 0, 0)
                .unwrap()
                .and_utc();
            assert_eq!(hav_2026.offset().valid_until(), Some(expected_until));

            // Single-span zones (UTC, Etc/GMT+1)
            for single_tz in [Tz::UTC, Tz::Etc__GMTPlus1] {
                let off = *DateTime::<Utc>::MIN_UTC.with_timezone(&single_tz).offset();
                assert_eq!(off.valid_starting(), None);
                assert_eq!(off.valid_until(), None);
                assert_eq!(off.next_offset(), None);
                assert_eq!(off.prev_offset(), None);
                assert_eq!(off.iter().len(), 1);
                assert_eq!(off.iter().collect::<Vec<_>>(), vec![off]);
            }
        }

        // Exhaustive check across all compiled variants and extreme dates: zero panics/overflows
        for &tz in TZ_VARIANTS.iter() {
            let min_dt = DateTime::<Utc>::MIN_UTC.with_timezone(&tz);
            let max_dt = DateTime::<Utc>::MAX_UTC.with_timezone(&tz);
            let _ = NaiveDateTime::MIN.and_local_timezone(tz);
            let _ = NaiveDateTime::MAX.and_local_timezone(tz);

            let first = *min_dt.offset();
            assert_eq!(first.valid_starting(), None);
            assert_eq!(first.prev_offset(), None);

            let timespans = tz.timespans();
            let mut iter = first.iter();
            assert_eq!(iter.len(), timespans.len());

            let mut prev_start: Option<DateTime<Utc>> = None;
            let mut prev_off: Option<TzOffset> = None;
            let mut count = 0usize;
            for off in &mut iter {
                count += 1;
                if let Some(p) = prev_off {
                    assert_eq!(off.prev_offset(), Some(p));
                    assert_eq!(p.valid_until(), off.valid_starting());
                    let start = off.valid_starting().unwrap();
                    if let Some(ps) = prev_start {
                        assert!(start > ps);
                    }
                    prev_start = Some(start);
                }
                prev_off = Some(off);
            }
            assert_eq!(count, timespans.len());
            assert_eq!(iter.len(), 0);
            assert_eq!(iter.next(), None);

            let last = prev_off.unwrap();
            assert_eq!(last.valid_until(), None);
            assert_eq!(last.next_offset(), None);
            assert_eq!(max_dt.offset().valid_until(), None);
        }
    }
}
