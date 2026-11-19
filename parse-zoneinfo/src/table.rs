//! Collecting parsed zoneinfo data lines into a set of time zone data.
//!
//! This module provides the `Table` struct, which is able to take parsed
//! lines of input from the `line` module and coalesce them into a single
//! set of data.
//!
//! It’s not as simple as it seems, because the zoneinfo data lines refer to
//! each other through strings: lines of the form “link zone A to B” could be
//! *parsed* successfully but still fail to be *interpreted* successfully if
//! “B” doesn’t exist. So it has to check every step of the way—nothing wrong
//! with this, it’s just a consequence of reading data from a text file.
//!
//! This module only deals with constructing a table from data: any analysis
//! of the data is done elsewhere.
//!
//!
//! ## Example
//!
//! ```
//! use sad_parse_zoneinfo::line::{Zone, Line, Link};
//! use sad_parse_zoneinfo::table::TableBuilder;
//!
//! let mut builder = TableBuilder::new();
//!
//! let zone = "Zone  Pacific/Auckland  11:39:04  -  LMT  1868  Nov  2";
//! let link = "Link  Pacific/Auckland  Antarctica/McMurdo";
//!
//! for line in [zone, link] {
//!     builder.add_line(Line::new(&line)?).unwrap();
//! }
//!
//! let table = builder.build();
//!
//! assert!(table.get_zoneset("Pacific/Auckland").is_some());
//! assert!(table.get_zoneset("Antarctica/McMurdo").is_some());
//! assert!(table.get_zoneset("UTC").is_none());
//! # Ok::<(), sad_parse_zoneinfo::line::Error>(())
//! ```

use std::collections::hash_map::{Entry, HashMap};
use std::collections::HashSet;
use std::fmt::{self, Write};

use crate::line::{self, ChangeTime, DaySpec, Line, Month, TimeType, Year};

/// A **table** of all the data in one or more zoneinfo files.
#[derive(PartialEq, Debug, Default)]
pub struct Table {
    /// Mapping of ruleset names to rulesets.
    pub rulesets: HashMap<String, Vec<RuleInfo>>,

    /// Mapping of zoneset names to zonesets.
    pub zonesets: HashMap<String, Vec<ZoneInfo>>,

    /// Mapping of link timezone names, to the names they link to.
    pub links: HashMap<String, String>,
}

impl Table {
    /// Tries to find the zoneset with the given name by looking it up in
    /// either the zonesets map or the links map.
    ///
    /// Resolves chained links and returns `None` on broken or cyclic links.
    pub fn get_zoneset(&self, zone_name: &str) -> Option<&[ZoneInfo]> {
        let mut current = zone_name;
        let mut visited = HashSet::new();

        loop {
            if let Some(zones) = self.zonesets.get(current) {
                return Some(zones.as_slice());
            }
            if !visited.insert(current) {
                return None;
            }
            current = self.links.get(current)?.as_str();
        }
    }
}

/// An owned rule definition line.
///
/// This mimics the `Rule` struct in the `line` module, only its uses owned
/// Strings instead of string slices, and has had some pre-processing
/// applied to it.
#[derive(PartialEq, Debug)]
pub struct RuleInfo {
    /// The year that this rule *starts* applying.
    pub from_year: Year,

    /// The year that this rule *finishes* applying, inclusive, or `None` if
    /// it applies up until the end of this timespan.
    pub to_year: Option<Year>,

    /// The month it applies on.
    pub month: Month,

    /// The day it applies on.
    pub day: DaySpec,

    /// The exact time it applies on.
    pub time: i64,

    /// The type of time that time is.
    pub time_type: TimeType,

    /// The amount of time to save.
    pub time_to_add: i64,

    /// Any extra letters that should be added to this time zone’s
    /// abbreviation, in place of `%s`.
    pub letters: Option<String>,
}

impl<'line> From<line::Rule<'line>> for RuleInfo {
    fn from(info: line::Rule) -> RuleInfo {
        RuleInfo {
            from_year: info.from_year,
            to_year: info.to_year,
            month: info.month,
            day: info.day,
            time: info.time.0.as_seconds(),
            time_type: info.time.1,
            time_to_add: info.time_to_add.as_seconds(),
            letters: info.letters.map(str::to_owned),
        }
    }
}

impl RuleInfo {
    /// Returns whether this rule is in effect during the given year.
    pub fn applies_to_year(&self, year: i64) -> bool {
        use line::Year::*;

        let from = match self.from_year {
            Minimum => i64::MIN,
            Maximum => i64::MAX,
            Number(y) => y,
        };
        let to = match self.to_year {
            None => from,
            Some(Minimum) => i64::MIN,
            Some(Maximum) => i64::MAX,
            Some(Number(y)) => y,
        };
        year >= from && year <= to
    }

    /// Computes the rule's transition instant in `i128` seconds without clamping,
    /// validating calendar dates (such as February 29 in a non-leap year).
    pub fn try_absolute_datetime_i128(
        &self,
        year: i64,
        utc_offset: i64,
        dst_offset: i64,
    ) -> Result<i128, line::Error> {
        let offset: i128 = match self.time_type {
            TimeType::UTC => 0,
            TimeType::Standard => utc_offset as i128,
            TimeType::Wall => utc_offset as i128 + dst_offset as i128,
        };

        let changetime = ChangeTime::UntilDay(Year::Number(year), self.month, self.day);
        let day_ts = changetime.try_to_timestamp_i128(0, 0)?;
        Ok(day_ts + self.time as i128 - offset)
    }

    pub fn absolute_datetime(&self, year: i64, utc_offset: i64, dst_offset: i64) -> i64 {
        let ts = match self.try_absolute_datetime_i128(year, utc_offset, dst_offset) {
            Ok(ts) => ts,
            Err(_) => {
                let offset: i128 = match self.time_type {
                    TimeType::UTC => 0,
                    TimeType::Standard => utc_offset as i128,
                    TimeType::Wall => utc_offset as i128 + dst_offset as i128,
                };
                let changetime = ChangeTime::UntilDay(Year::Number(year), self.month, self.day);
                changetime.to_timestamp(0, 0) as i128 + self.time as i128 - offset
            }
        };
        ts.clamp(i64::MIN as i128, i64::MAX as i128) as i64
    }
}

/// An owned zone definition line.
///
/// This struct mimics the `ZoneInfo` struct in the `line` module, *not* the
/// `Zone` struct, which is the key name in the map—this is just the value.
///
/// As with `RuleInfo`, this struct uses owned Strings rather than string
/// slices.
#[derive(PartialEq, Debug)]
pub struct ZoneInfo {
    /// The number of seconds that need to be added to UTC to get the
    /// standard time in this zone.
    pub offset: i64,

    /// The name of all the rules that should apply in the time zone, or the
    /// amount of daylight-saving time to add.
    pub saving: Saving,

    /// The format for time zone abbreviations.
    pub format: Format,

    /// The time at which the rules change for this time zone, or `None` if
    /// these rules are in effect until the end of time (!).
    pub end_time: Option<ChangeTime>,
}

impl<'line> From<line::ZoneInfo<'line>> for ZoneInfo {
    fn from(info: line::ZoneInfo) -> ZoneInfo {
        ZoneInfo {
            offset: info.utc_offset.as_seconds(),
            saving: match info.saving {
                line::Saving::NoSaving => Saving::NoSaving,
                line::Saving::Multiple(s) => Saving::Multiple(s.to_owned()),
                line::Saving::OneOff(t) => Saving::OneOff(t.as_seconds()),
            },
            format: Format::new(info.format),
            end_time: info.time,
        }
    }
}

/// The amount of daylight saving time (DST) to apply to this timespan. This
/// is a special type for a certain field in a zone line, which can hold
/// different types of value.
///
/// This is the owned version of the `Saving` type in the `line` module.
#[derive(PartialEq, Debug)]
pub enum Saving {
    /// Just stick to the base offset.
    NoSaving,

    /// This amount of time should be saved while this timespan is in effect.
    /// (This is the equivalent to there being a single one-off rule with the
    /// given amount of time to save).
    OneOff(i64),

    /// All rules with the given name should apply while this timespan is in
    /// effect.
    Multiple(String),
}

/// The format string to generate a time zone abbreviation from.
#[non_exhaustive]
#[derive(PartialEq, Debug, Clone)]
pub enum Format {
    /// A constant format, which remains the same throughout both standard
    /// and DST timespans.
    Constant(String),

    /// An alternate format, such as “PST/PDT”, which changes between
    /// standard and DST timespans.
    Alternate {
        /// Abbreviation to use during Standard Time.
        standard: String,

        /// Abbreviation to use during Summer Time.
        dst: String,
    },

    /// A format with a placeholder `%s`, which uses the `letters` field in
    /// a `RuleInfo` to generate the time zone abbreviation.
    Placeholder(String),

    /// The special %z placeholder that gets formatted as a numeric offset.
    Offset,
}

impl Format {
    /// Convert the template into one of the `Format` variants. This can’t
    /// fail, as any syntax that doesn’t match one of the two formats will
    /// just be a ‘constant’ format.
    pub fn new(template: &str) -> Format {
        if let Some(pos) = template.find('/') {
            Format::Alternate {
                standard: template[..pos].to_owned(),
                dst: template[pos + 1..].to_owned(),
            }
        } else if template.contains("%s") {
            Format::Placeholder(template.to_owned())
        } else if template == "%z" {
            Format::Offset
        } else {
            Format::Constant(template.to_owned())
        }
    }

    /// Returns `true` if this format is [`Format::Offset`] (`%z`).
    pub fn is_offset(&self) -> bool {
        matches!(*self, Format::Offset)
    }

    /// Formats the timezone abbreviation if the resulting byte length does not
    /// exceed `max_len`, returning `None` if it would exceed `max_len` without
    /// allocating an oversized buffer.
    pub fn try_format(
        &self,
        utc_offset: i64,
        dst_offset: i64,
        letters: Option<&String>,
        max_len: usize,
    ) -> Option<String> {
        let letters = match letters {
            Some(l) => &**l,
            None => "",
        };

        match *self {
            Format::Constant(ref s) => (s.len() <= max_len).then(|| s.clone()),
            Format::Placeholder(ref s) => {
                let matches = s.matches("%s").count();
                let out_len = s
                    .len()
                    .checked_sub(matches * 2)?
                    .checked_add(matches.checked_mul(letters.len())?)?;
                if out_len > max_len {
                    return None;
                }
                let mut out = String::with_capacity(out_len);
                for (idx, chunk) in s.split("%s").enumerate() {
                    if idx > 0 {
                        out.push_str(letters);
                    }
                    out.push_str(chunk);
                }
                Some(out)
            }
            Format::Alternate { ref standard, .. } if dst_offset == 0 => {
                (standard.len() <= max_len).then(|| standard.clone())
            }
            Format::Alternate { ref dst, .. } => (dst.len() <= max_len).then(|| dst.clone()),
            Format::Offset => {
                let offset = utc_offset as i128 + dst_offset as i128;
                let sign = if offset < 0 { '-' } else { '+' };
                let off = offset.unsigned_abs();

                let minutes = off / 60;
                let secs = off % 60;
                let mins = minutes % 60;
                let hours = minutes / 60;

                let mut f = String::with_capacity(16);
                f.push(sign);
                let _ = write!(&mut f, "{hours:02}");
                if secs != 0 {
                    let _ = write!(&mut f, "{mins:02}{secs:02}");
                } else if mins != 0 {
                    let _ = write!(&mut f, "{mins:02}");
                }
                (f.len() <= max_len).then_some(f)
            }
        }
    }

    pub fn format(&self, utc_offset: i64, dst_offset: i64, letters: Option<&String>) -> String {
        self.try_format(utc_offset, dst_offset, letters, usize::MAX)
            .expect("unbounded format length")
    }

    /// Returns the constant abbreviation string if this format is
    /// [`Format::Constant`], or `None` for dynamic formats (`Alternate`,
    /// `Placeholder`, or `Offset`).
    pub fn try_format_constant(&self) -> Option<&str> {
        match *self {
            Format::Constant(ref s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns the constant abbreviation string.
    ///
    /// # Panics
    ///
    /// Panics if `self` is not [`Format::Constant`]. Prefer
    /// [`Format::try_format_constant`] for fallible callers.
    pub fn format_constant(&self) -> String {
        self.try_format_constant()
            .expect("Expected a constant formatting string")
            .to_owned()
    }
}

/// A builder for `Table` values based on various line definitions.
#[derive(PartialEq, Debug)]
pub struct TableBuilder {
    /// The table that’s being built up.
    table: Table,

    /// If the last line was a zone definition, then this holds its name.
    /// `None` otherwise. This is so continuation lines can be added to the
    /// same zone as the original zone line.
    current_zoneset_name: Option<String>,
}

impl Default for TableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TableBuilder {
    /// Creates a new builder with an empty table.
    pub fn new() -> TableBuilder {
        TableBuilder {
            table: Table::default(),
            current_zoneset_name: None,
        }
    }

    pub fn add_line<'line>(&mut self, line: Line<'line>) -> Result<(), Error<'line>> {
        match line {
            Line::Zone(zone) => self.add_zone_line(zone),
            Line::Continuation(cont) => self.add_continuation_line(cont),
            Line::Rule(rule) => self.add_rule_line(rule),
            Line::Link(link) => self.add_link_line(link),
            Line::Space => Ok(()),
        }
    }

    /// Adds a new line describing a zone definition.
    ///
    /// Returns an error if there’s already a zone with the same name, or the
    /// zone refers to a ruleset that hasn’t been defined yet.
    pub fn add_zone_line<'line>(
        &mut self,
        zone_line: line::Zone<'line>,
    ) -> Result<(), Error<'line>> {
        if let line::Saving::Multiple(ruleset_name) = zone_line.info.saving {
            if !self.table.rulesets.contains_key(ruleset_name) {
                return Err(Error::UnknownRuleset(ruleset_name));
            }
        }

        if self.table.links.contains_key(zone_line.name) {
            return Err(Error::DuplicateZone);
        }

        let zoneset = match self.table.zonesets.entry(zone_line.name.to_owned()) {
            Entry::Occupied(_) => return Err(Error::DuplicateZone),
            Entry::Vacant(e) => e.insert(Vec::new()),
        };

        zoneset.push(zone_line.info.into());
        self.current_zoneset_name = Some(zone_line.name.to_owned());
        Ok(())
    }

    /// Adds a new line describing the *continuation* of a zone definition.
    ///
    /// Returns an error if the builder wasn’t expecting a continuation line
    /// (meaning, the previous line wasn’t a zone line). Forward references to
    /// rulesets defined later in the zoneinfo data (such as `Rule EU` in
    /// IANA `europe`) are allowed here and validated during transition building.
    pub fn add_continuation_line<'line>(
        &mut self,
        continuation_line: line::ZoneInfo<'line>,
    ) -> Result<(), Error<'line>> {
        let zoneset = match self.current_zoneset_name {
            Some(ref name) => self
                .table
                .zonesets
                .get_mut(name)
                .ok_or(Error::SurpriseContinuationLine)?,
            None => return Err(Error::SurpriseContinuationLine),
        };

        zoneset.push(continuation_line.into());
        Ok(())
    }

    /// Adds a new line describing one entry in a ruleset, creating that set
    /// if it didn’t exist already.
    pub fn add_rule_line<'line>(
        &mut self,
        rule_line: line::Rule<'line>,
    ) -> Result<(), Error<'line>> {
        let ruleset = self
            .table
            .rulesets
            .entry(rule_line.name.to_owned())
            .or_default();

        ruleset.push(rule_line.into());
        self.current_zoneset_name = None;
        Ok(())
    }

    /// Adds a new line linking one zone to another.
    ///
    /// Returns an error if there was already a link with that name.
    pub fn add_link_line<'line>(
        &mut self,
        link_line: line::Link<'line>,
    ) -> Result<(), Error<'line>> {
        if self.table.zonesets.contains_key(link_line.new) {
            return Err(Error::DuplicateLink(link_line.new));
        }

        match self.table.links.entry(link_line.new.to_owned()) {
            Entry::Occupied(_) => Err(Error::DuplicateLink(link_line.new)),
            Entry::Vacant(e) => {
                let _ = e.insert(link_line.existing.to_owned());
                self.current_zoneset_name = None;
                Ok(())
            }
        }
    }

    /// Returns the table after it’s finished being built.
    pub fn build(self) -> Table {
        self.table
    }
}

/// Something that can go wrong while constructing a `Table`.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Error<'line> {
    /// A continuation line was passed in, but the previous line wasn’t a zone
    /// definition line.
    SurpriseContinuationLine,

    /// A zone definition referred to a ruleset that hadn’t been defined.
    UnknownRuleset(&'line str),

    /// A link line was passed in, but there’s already a link with that name.
    DuplicateLink(&'line str),

    /// A zone line was passed in, but there’s already a zone with that name.
    DuplicateZone,
}

impl<'line> fmt::Display for Error<'line> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::SurpriseContinuationLine => {
                write!(
                    f,
                    "continuation line follows line that isn't a zone definition line"
                )
            }
            Error::UnknownRuleset(_) => {
                write!(f, "zone definition refers to a ruleset that isn't defined")
            }
            Error::DuplicateLink(_) => write!(f, "link line with name that already exists"),
            Error::DuplicateZone => write!(f, "zone line with name that already exists"),
        }
    }
}

impl<'line> std::error::Error for Error<'line> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line::{DaySpec, Line, Month, TimeType, Year};

    #[test]
    fn get_zoneset_chained_cyclic_and_broken_links() {
        let mut builder = TableBuilder::new();
        builder
            .add_line(Line::new("Zone Real/Zone 1:00 - RZT").unwrap())
            .unwrap();
        builder
            .add_line(Line::new("Link Real/Zone Link/One").unwrap())
            .unwrap();
        builder
            .add_line(Line::new("Link Link/One Link/Two").unwrap())
            .unwrap();
        builder
            .add_line(Line::new("Link Missing/Target Broken/Link").unwrap())
            .unwrap();
        builder
            .add_line(Line::new("Link Cycle/B Cycle/A").unwrap())
            .unwrap();
        builder
            .add_line(Line::new("Link Cycle/A Cycle/B").unwrap())
            .unwrap();
        builder
            .add_line(Line::new("Link Self/Link Self/Link").unwrap())
            .unwrap();
        let table = builder.build();

        assert!(table.get_zoneset("Real/Zone").is_some());
        assert_eq!(
            table.get_zoneset("Link/Two"),
            table.get_zoneset("Real/Zone")
        );
        assert!(table.get_zoneset("Broken/Link").is_none());
        assert!(table.get_zoneset("Cycle/A").is_none());
        assert!(table.get_zoneset("Self/Link").is_none());
    }

    #[test]
    fn continuation_unknown_ruleset_does_not_panic_on_timespans() {
        use crate::transitions::TableTransitions;

        let mut builder = TableBuilder::new();
        builder
            .add_line(Line::new("Zone Test/Zone 0 - TZ 1970").unwrap())
            .unwrap();
        builder
            .add_line(Line::new(" 0 NoSuchRule T%sT").unwrap())
            .unwrap();
        let table = builder.build();
        assert!(table.timespans("Test/Zone").is_none());
    }

    #[test]
    fn format_offset_percent_z_edge_cases() {
        let fmt = Format::new("%z");
        assert_eq!(fmt.format(0, 0, None), "+00");
        assert_eq!(fmt.format(-3 * 3600, 0, None), "-03");
        assert_eq!(fmt.format(5 * 3600 + 30 * 60, 0, None), "+0530");
        assert_eq!(fmt.format(3600 + 2 * 60 + 3, 0, None), "+010203");
        assert_eq!(fmt.format(-(60 + 15), 0, None), "-000115");
        assert_eq!(fmt.format(300 * 3600, 0, None), "+300");
        assert!(!fmt.format(i64::MIN, 0, None).is_empty());
    }

    #[test]
    fn rule_info_min_max_years_no_panic() {
        let rule_min_max = RuleInfo {
            from_year: Year::Minimum,
            to_year: Some(Year::Maximum),
            month: Month::January,
            day: DaySpec::Ordinal(1),
            time: 0,
            time_type: TimeType::UTC,
            time_to_add: 0,
            letters: None,
        };
        assert!(rule_min_max.applies_to_year(2026));
        assert!(rule_min_max.applies_to_year(i64::MIN));

        let rule_inverted = RuleInfo {
            from_year: Year::Maximum,
            to_year: Some(Year::Number(2000)),
            month: Month::January,
            day: DaySpec::Ordinal(1),
            time: 0,
            time_type: TimeType::Wall,
            time_to_add: 0,
            letters: None,
        };
        assert!(!rule_inverted.applies_to_year(2000));
        let _ = rule_inverted.absolute_datetime(i64::MAX, i64::MAX, i64::MAX);
    }

    #[test]
    fn rule_info_single_clamp_boundary() {
        // Year where Jan 1 00:00 UTC is just above i64::MAX, and a large positive UTC
        // offset brings the resulting UTC timestamp back below i64::MAX.
        // 2^63 / 31_556_952 ~= 292_277_026_596 -> let's pick year 292_277_026_597
        let rule = RuleInfo {
            from_year: Year::Minimum,
            to_year: Some(Year::Maximum),
            month: Month::January,
            day: DaySpec::Ordinal(1),
            time: 0,
            time_type: TimeType::Standard,
            time_to_add: 0,
            letters: None,
        };
        let jan1 = ChangeTime::UntilDay(
            Year::Number(292_277_026_597),
            Month::January,
            DaySpec::Ordinal(1),
        )
        .try_to_timestamp_i128(0, 0)
        .unwrap();
        assert!(jan1 > i64::MAX as i128);
        let delta = (jan1 - i64::MAX as i128) as i64 + 100;
        let resolved = rule.absolute_datetime(292_277_026_597, delta, 0);
        assert_eq!(resolved, i64::MAX - 100);

        // And with offset = 0, clamps to i64::MAX; with negative offset = -3600, clamps to i64::MAX
        // (not i64::MAX - 3600).
        let rule_wall = RuleInfo {
            time_type: TimeType::Wall,
            ..rule
        };
        assert_eq!(
            rule_wall.absolute_datetime(292_277_026_597, 3600, -7200),
            i64::MAX
        );
    }

    #[test]
    fn format_try_format_and_try_format_constant() {
        let constant = Format::new("EST");
        assert_eq!(constant.try_format_constant(), Some("EST"));
        assert_eq!(constant.try_format(0, 0, None, 64), Some("EST".to_owned()));
        assert_eq!(constant.try_format(0, 0, None, 2), None);

        let offset = Format::new("%z");
        assert_eq!(offset.try_format_constant(), None);
        assert!(offset.is_offset());

        let placeholder = Format::new("%s%s%s%s");
        assert_eq!(placeholder.try_format_constant(), None);
        let long_letters = "X".repeat(20);
        assert_eq!(placeholder.try_format(0, 0, Some(&long_letters), 64), None);
        assert_eq!(
            Format::new("A%sB").try_format(0, 0, Some(&"S".to_owned()), 64),
            Some("ASB".to_owned())
        );
    }

    #[test]
    fn table_builder_rejects_zone_link_collisions() {
        let mut b1 = TableBuilder::new();
        b1.add_line(Line::new("Zone Foo/Bar 0 - FB").unwrap())
            .unwrap();
        assert_eq!(
            b1.add_line(Line::new("Link Foo/Bar Foo/Bar").unwrap()),
            Err(Error::DuplicateLink("Foo/Bar"))
        );

        let mut b2 = TableBuilder::new();
        b2.add_line(Line::new("Link Foo/Baz Foo/Bar").unwrap())
            .unwrap();
        assert_eq!(
            b2.add_line(Line::new("Zone Foo/Bar 0 - FB").unwrap()),
            Err(Error::DuplicateZone)
        );
    }
}
