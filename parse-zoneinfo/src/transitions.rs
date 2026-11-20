//! Generating timespan sets from a built Table.
//!
//! Once a table has been fully built, it needs to be turned into several
//! *fixed timespan sets*: a series of spans of time where the local time
//! offset remains the same throughout. One set is generated for each named
//! time zone. These timespan sets can then be iterated over to produce
//! *transitions*: when the local time changes from one offset to another.
//!
//! These sets are returned as `FixedTimespanSet` values, rather than
//! iterators, because the generation logic does not output the timespans
//! in any particular order, meaning they need to be sorted before they’re
//! returned—so we may as well just return the vector, rather than an
//! iterator over the vector.
//!
//! Similarly, there is a fixed set of years that is iterated over
//! (currently 1800..2100), rather than having an iterator that produces
//! timespans indefinitely. Not only do we need a complete set of timespans
//! for sorting, but it is not necessarily advisable to rely on offset
//! changes so far into the future!
//!
//! ### Example
//!
//! The complete definition of the `Indian/Mauritius` time zone, as
//! specified in the `africa` file in my version of the tz database, has
//! two Zone definitions, one of which refers to four Rule definitions:
//!
//! ```tz
//! # Rule      NAME    FROM    TO      TYPE    IN      ON      AT      SAVE    LETTER/S
//! Rule Mauritius      1982    only    -       Oct     10      0:00    1:00    S
//! Rule Mauritius      1983    only    -       Mar     21      0:00    0       -
//! Rule Mauritius      2008    only    -       Oct     lastSun 2:00    1:00    S
//! Rule Mauritius      2009    only    -       Mar     lastSun 2:00    0       -
//!
//! # Zone      NAME            GMTOFF  RULES   FORMAT  [UNTIL]
//! Zone Indian/Mauritius       3:50:00 -       LMT     1907   # Port Louis
//!                             4:00 Mauritius  MU%sT          # Mauritius Time
//! ```
//!
//! To generate a fixed timespan set for this timezone, we examine each of the
//! Zone definitions, generating at least one timespan for each definition.
//!
//! * The first timespan describes the *local mean time* (LMT) in Mauritius,
//!   calculated by the geographical position of Port Louis, its capital.
//!   Although it’s common to have a timespan set begin with a city’s local mean
//!   time, it is by no means necessary. This timespan has a fixed offset of
//!   three hours and fifty minutes ahead of UTC, and lasts until the beginning
//!   of 1907, at which point the second timespan kicks in.
//! * The second timespan has no ‘until’ date, so it’s in effect indefinitely.
//!   Instead of having a fixed offset, it refers to the set of rules under the
//!   name “Mauritius”, which we’ll have to consult to compute the timespans.
//!     * The first two rules refer to a summer time transition that began on
//!       the 10th of October 1982, and lasted until the 21st of March 1983. But
//!       before we get onto that, we need to add a timespan beginning at the
//!       time the last one ended (1907), up until the point Summer Time kicks
//!       in (1982), reflecting that it was four hours ahead of UTC.
//!     * After this, we add another timespan for Summer Time, when Mauritius
//!       was an extra hour ahead, bringing the total offset for that time to
//!       *five* hours.
//!     * The next (and last) two rules refer to another summer time
//!       transition from the last Sunday of October 2008 to the last Sunday of
//!       March 2009, this time at 2am local time instead of midnight. But, as
//!       before, we need to add a *standard* time timespan beginning at the
//!       time Summer Time ended (1983) up until the point the next span of
//!       Summer Time kicks in (2008), again reflecting that it was four hours
//!       ahead of UTC again.
//!     * Next, we add the Summer Time timespan, again bringing the total
//!       offset to five hours. We need to calculate when the last Sundays of
//!       the months are to get the dates correct.
//!     * Finally, we add one last standard time timespan, lasting from 2009
//!       indefinitely, as the Mauritian authorities decided not to change to
//!       Summer Time again.
//!
//! All this calculation results in the following six timespans to be added:
//!
//! | Timespan start            | Abbreviation | UTC offset         | DST? |
//! |:--------------------------|:-------------|:-------------------|:-----|
//! | *no start*                | LMT          | 3 hours 50 minutes | No   |
//! | 1906-11-31 T 20:10:00 UTC | MUT          | 4 hours            | No   |
//! | 1982-09-09 T 20:00:00 UTC | MUST         | 5 hours            | Yes  |
//! | 1983-02-20 T 19:00:00 UTC | MUT          | 4 hours            | No   |
//! | 2008-09-25 T 22:00:00 UTC | MUST         | 5 hours            | Yes  |
//! | 2009-02-28 T 21:00:00 UTC | MUT          | 4 hours            | No   |
//!
//! There are a few final things of note:
//!
//! Firstly, this library records the times that timespans *begin*, while
//! the tz data files record the times that timespans *end*. Pay attention to
//! this if the timestamps aren’t where you expect them to be! For example, in
//! the data file, the first zone rule has an ‘until’ date and the second has
//! none, whereas in the list of timespans, the last timespan has a ‘start’
//! date and the *first* has none.
//!
//! Secondly, although local mean time in Mauritius lasted until 1907, the
//! timespan is recorded as ending in 1906! Why is this? It’s because the
//! transition occurred at midnight *at the local time*, which in this case,
//! was three hours fifty minutes ahead of UTC. So that time has to be
//! *subtracted* from the date, resulting in twenty hours and ten minutes on
//! the last day of the year. Similar things happen on the rest of the
//! transitions, being either four or five hours ahead of UTC.
//!
//! The logic in this file is based off of `zic.c`, which comes with the
//! zoneinfo files and is in the public domain.

use std::fmt;

use crate::line::{self, TimeType};
use crate::table::{Format, RuleInfo, Saving, Table, ZoneInfo};

/// Maximum number of rules permitted in a single ruleset during transition generation.
pub const MAX_RULES_PER_RULESET: usize = 4_096;

/// Maximum number of rules permitted to apply to a single calendar year.
pub const MAX_ACTIVE_RULES_PER_YEAR: usize = 256;

/// Maximum number of continuation segments permitted in a single zone definition.
pub const MAX_ZONE_SEGMENTS: usize = 1_024;

/// Maximum cumulative rule-evaluation steps permitted when generating a single zone.
pub const MAX_RULE_EVALUATIONS_PER_ZONE: usize = 500_000;

/// Maximum number of transitions permitted in a single timezone's generated table.
pub const MAX_TRANSITIONS_PER_ZONE: usize = 10_000;

/// Maximum byte length permitted for a formatted timezone abbreviation.
pub const MAX_FORMATTED_NAME_LEN: usize = 64;

/// Errors that can occur when computing a timezone's transition table.
#[non_exhaustive]
#[derive(PartialEq, Debug, Clone)]
pub enum TransitionError {
    /// The requested timezone (or a link target) was not found, or links form a cycle.
    ZoneNotFound,
    /// The timezone's zoneset contained no segments.
    EmptyZoneset,
    /// The timezone's zoneset exceeded [`MAX_ZONE_SEGMENTS`].
    TooManySegments { count: usize, limit: usize },
    /// A zone segment referenced an undefined ruleset.
    UnknownRuleset(String),
    /// A non-final zone segment was missing an `UNTIL` timestamp.
    MissingUntil,
    /// The final zone segment unexpectedly specified an `UNTIL` timestamp.
    UnexpectedTerminalUntil,
    /// A zone segment's `UNTIL` timestamp did not strictly increase over the previous segment.
    NonIncreasingUntil { previous: i64, current: i64 },
    /// A zone segment's `UNTIL` timestamp exceeded `i64` Unix-second bounds.
    TimestampOutOfRange,
    /// A zone or rule date was invalid for its month/year (e.g. Feb 29 in a common year).
    InvalidDate(line::Error),
    /// A ruleset exceeded [`MAX_RULES_PER_RULESET`].
    TooManyRules { count: usize, limit: usize },
    /// Too many rules applied in a single calendar year ([`MAX_ACTIVE_RULES_PER_YEAR`]).
    TooManyActiveRules {
        year: i64,
        count: usize,
        limit: usize,
    },
    /// Cumulative rule evaluation work exceeded [`MAX_RULE_EVALUATIONS_PER_ZONE`].
    WorkBudgetExceeded { limit: usize },
    /// Too many transitions were generated for one zone ([`MAX_TRANSITIONS_PER_ZONE`]).
    TooManyTransitions { limit: usize },
    /// A formatted timezone abbreviation exceeded [`MAX_FORMATTED_NAME_LEN`].
    FormattedNameTooLong { limit: usize },
    /// No initial or recurring timespan could be produced for the zone.
    EmptyTimespanSet,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransitionError::ZoneNotFound => write!(f, "timezone not found or broken/cyclic link"),
            TransitionError::EmptyZoneset => write!(f, "zoneset has no segments"),
            TransitionError::TooManySegments { count, limit } => {
                write!(f, "zoneset has {count} segments, exceeding limit of {limit}")
            }
            TransitionError::UnknownRuleset(name) => write!(f, "unknown ruleset `{name}`"),
            TransitionError::MissingUntil => write!(f, "non-final zone segment missing UNTIL"),
            TransitionError::UnexpectedTerminalUntil => {
                write!(f, "final zone segment must not have an UNTIL timestamp")
            }
            TransitionError::NonIncreasingUntil { previous, current } => write!(
                f,
                "zone UNTIL timestamp ({current}) must be strictly greater than previous ({previous})"
            ),
            TransitionError::TimestampOutOfRange => {
                write!(f, "zone UNTIL timestamp is outside i64 range")
            }
            TransitionError::InvalidDate(err) => write!(f, "invalid calendar date: {err}"),
            TransitionError::TooManyRules { count, limit } => {
                write!(f, "ruleset has {count} rules, exceeding limit of {limit}")
            }
            TransitionError::TooManyActiveRules { year, count, limit } => write!(
                f,
                "year {year} has {count} active rules, exceeding limit of {limit}"
            ),
            TransitionError::WorkBudgetExceeded { limit } => {
                write!(f, "zone exceeded rule evaluation work budget of {limit}")
            }
            TransitionError::TooManyTransitions { limit } => {
                write!(f, "zone exceeded limit of {limit} transitions")
            }
            TransitionError::FormattedNameTooLong { limit } => {
                write!(f, "formatted timezone abbreviation exceeded {limit} bytes")
            }
            TransitionError::EmptyTimespanSet => write!(f, "zone produced no timespans"),
        }
    }
}

impl std::error::Error for TransitionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TransitionError::InvalidDate(err) => Some(err),
            _ => None,
        }
    }
}

/// A timespan set preserving `%z` vs explicit abbreviation provenance for codegen.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct CodegenTimespanSet {
    /// The initial timespan before the first transition.
    pub first: CodegenTimespan,
    /// Subsequent `(transition_unix_seconds, timespan)` pairs.
    pub rest: Vec<(i64, CodegenTimespan)>,
}

/// An individual timespan where `%z`-derived numeric abbreviations are represented as `None`.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct CodegenTimespan {
    /// Standard UTC offset in seconds.
    pub utc_offset: i64,
    /// Additional DST offset in seconds.
    pub dst_offset: i64,
    /// Explicit abbreviation (`Some`), or `None` when derived from `%z` (`Format::Offset`).
    pub name: Option<String>,
}

/// A set of timespans, separated by the instances at which the timespans
/// change over. There will always be one more timespan than transitions.
///
/// This mimics the `FixedTimespanSet` struct in `datetime::cal::zone`,
/// except it uses owned `Vec`s instead of slices.
#[derive(PartialEq, Debug, Clone)]
pub struct FixedTimespanSet {
    /// The first timespan, which is assumed to have been in effect up until
    /// the initial transition instant (if any). Each set has to have at
    /// least one timespan.
    pub first: FixedTimespan,

    /// The rest of the timespans, as a vector of tuples, each containing:
    ///
    /// 1. A transition instant at which the previous timespan ends and the
    ///    next one begins, stored as a Unix timestamp;
    /// 2. The actual timespan to transition into.
    pub rest: Vec<(i64, FixedTimespan)>,
}

/// An individual timespan with a fixed offset.
///
/// This mimics the `FixedTimespan` struct in `datetime::cal::zone`, except
/// instead of “total offset” and “is DST” fields, it has separate UTC and
/// DST fields. Also, the name is an owned `String` here instead of a slice.
#[derive(PartialEq, Debug, Clone)]
pub struct FixedTimespan {
    /// The number of seconds offset from UTC during this timespan.
    pub utc_offset: i64,

    /// The number of *extra* daylight-saving seconds during this timespan.
    pub dst_offset: i64,

    /// The abbreviation in use during this timespan.
    pub name: String,
}

impl FixedTimespan {
    /// The total offset in effect during this timespan.
    pub fn total_offset(&self) -> i64 {
        self.utc_offset.saturating_add(self.dst_offset)
    }
}

/// Trait to put the `timespans` method on Tables.
pub trait TableTransitions {
    /// Computes a fixed timespan set for the timezone with the given name.
    /// Returns `None` if the table doesn’t contain a time zone with that name.
    fn timespans(&self, zone_name: &str) -> Option<FixedTimespanSet>;
}

impl Table {
    /// Computes a fixed timespan set for `zone_name` with bounded resource limits
    /// and strict calendar/segment validation.
    pub fn try_timespans(&self, zone_name: &str) -> Result<FixedTimespanSet, TransitionError> {
        let internal = self.build_internal_timespans(zone_name)?;
        Ok(FixedTimespanSet {
            first: internal.first.into_fixed(),
            rest: internal
                .rest
                .into_iter()
                .map(|(t, span)| (t, span.into_fixed()))
                .collect(),
        })
    }

    /// Computes a codegen timespan set for `zone_name` preserving `%z` provenance
    /// (`name: None` for `Format::Offset`) under bounded resource limits.
    pub fn try_codegen_timespans(
        &self,
        zone_name: &str,
    ) -> Result<CodegenTimespanSet, TransitionError> {
        let internal = self.build_internal_timespans(zone_name)?;
        Ok(CodegenTimespanSet {
            first: internal.first.into_codegen(),
            rest: internal
                .rest
                .into_iter()
                .map(|(t, span)| (t, span.into_codegen()))
                .collect(),
        })
    }

    fn build_internal_timespans(
        &self,
        zone_name: &str,
    ) -> Result<InternalTimespanSet, TransitionError> {
        let mut builder = FixedTimespanSetBuilder::default();

        let zoneset = self
            .get_zoneset(zone_name)
            .ok_or(TransitionError::ZoneNotFound)?;
        if zoneset.is_empty() {
            return Err(TransitionError::EmptyZoneset);
        }
        if zoneset.len() > MAX_ZONE_SEGMENTS {
            return Err(TransitionError::TooManySegments {
                count: zoneset.len(),
                limit: MAX_ZONE_SEGMENTS,
            });
        }

        for (i, zone_info) in zoneset.iter().enumerate() {
            builder.consume_work(1)?;
            let mut dst_offset = 0;
            let use_until = i != zoneset.len() - 1;
            let utc_offset = zone_info.offset;

            let mut insert_start_transition = i > 0;
            let mut start_zone_id = None;
            let mut start_utc_offset = zone_info.offset;
            let mut start_dst_offset = 0;

            match zone_info.saving {
                Saving::NoSaving => {
                    builder.add_fixed_saving(
                        zone_info,
                        0,
                        &mut dst_offset,
                        utc_offset,
                        &mut insert_start_transition,
                        &mut start_zone_id,
                    )?;
                }

                Saving::OneOff(amount) => {
                    builder.add_fixed_saving(
                        zone_info,
                        amount,
                        &mut dst_offset,
                        utc_offset,
                        &mut insert_start_transition,
                        &mut start_zone_id,
                    )?;
                }

                Saving::Multiple(ref rules) => {
                    let rules = self
                        .rulesets
                        .get(rules)
                        .ok_or_else(|| TransitionError::UnknownRuleset(rules.clone()))?;
                    builder.add_multiple_saving(
                        zone_info,
                        rules,
                        &mut dst_offset,
                        use_until,
                        utc_offset,
                        &mut insert_start_transition,
                        &mut start_zone_id,
                        &mut start_utc_offset,
                        &mut start_dst_offset,
                    )?;
                }
            }

            if insert_start_transition {
                if let Some((name, is_offset)) = start_zone_id.clone() {
                    let start_time = builder.start_time.ok_or(TransitionError::MissingUntil)?;
                    builder.push_transition(
                        start_time,
                        InternalTimespan {
                            utc_offset: start_utc_offset,
                            dst_offset: start_dst_offset,
                            name,
                            is_offset,
                        },
                    )?;
                }
            }

            if use_until {
                let end_time = zone_info.end_time.ok_or(TransitionError::MissingUntil)?;
                let next_start_i128 = end_time
                    .try_to_timestamp_i128(utc_offset, dst_offset)
                    .map_err(TransitionError::InvalidDate)?;
                if let Some(prev_start_i128) = builder.start_time_i128 {
                    if next_start_i128 <= prev_start_i128 {
                        return Err(TransitionError::NonIncreasingUntil {
                            previous: prev_start_i128.clamp(i64::MIN as i128, i64::MAX as i128)
                                as i64,
                            current: next_start_i128.clamp(i64::MIN as i128, i64::MAX as i128)
                                as i64,
                        });
                    }
                }
                let next_start = i64::try_from(next_start_i128)
                    .map_err(|_| TransitionError::TimestampOutOfRange)?;
                builder.start_time_i128 = Some(next_start_i128);
                builder.start_time = Some(next_start);
            } else if zone_info.end_time.is_some() {
                return Err(TransitionError::UnexpectedTerminalUntil);
            }
        }

        builder.build()
    }
}

impl TableTransitions for Table {
    fn timespans(&self, zone_name: &str) -> Option<FixedTimespanSet> {
        self.try_timespans(zone_name).ok()
    }
}

#[derive(PartialEq, Debug, Clone)]
struct InternalTimespan {
    utc_offset: i64,
    dst_offset: i64,
    name: String,
    is_offset: bool,
}

impl InternalTimespan {
    fn total_offset(&self) -> i128 {
        self.utc_offset as i128 + self.dst_offset as i128
    }

    fn into_fixed(self) -> FixedTimespan {
        FixedTimespan {
            utc_offset: self.utc_offset,
            dst_offset: self.dst_offset,
            name: self.name,
        }
    }

    fn into_codegen(self) -> CodegenTimespan {
        CodegenTimespan {
            utc_offset: self.utc_offset,
            dst_offset: self.dst_offset,
            name: if self.is_offset {
                None
            } else {
                Some(self.name)
            },
        }
    }
}

struct InternalTimespanSet {
    first: InternalTimespan,
    rest: Vec<(i64, InternalTimespan)>,
}

fn format_bounded(
    format: &Format,
    utc_offset: i64,
    dst_offset: i64,
    letters: Option<&String>,
) -> Result<(String, bool), TransitionError> {
    let formatted = format
        .try_format(utc_offset, dst_offset, letters, MAX_FORMATTED_NAME_LEN)
        .ok_or(TransitionError::FormattedNameTooLong {
            limit: MAX_FORMATTED_NAME_LEN,
        })?;
    Ok((formatted, format.is_offset()))
}

#[derive(Debug, Default)]
struct FixedTimespanSetBuilder {
    first: Option<InternalTimespan>,
    rest: Vec<(i64, InternalTimespan)>,

    start_time: Option<i64>,
    start_time_i128: Option<i128>,
    until_time_i128: Option<i128>,
    work_steps: usize,
}

impl FixedTimespanSetBuilder {
    fn consume_work(&mut self, steps: usize) -> Result<(), TransitionError> {
        self.work_steps = self.work_steps.saturating_add(steps);
        if self.work_steps > MAX_RULE_EVALUATIONS_PER_ZONE {
            return Err(TransitionError::WorkBudgetExceeded {
                limit: MAX_RULE_EVALUATIONS_PER_ZONE,
            });
        }
        Ok(())
    }

    fn push_transition(&mut self, at: i64, span: InternalTimespan) -> Result<(), TransitionError> {
        if self.rest.len() >= MAX_TRANSITIONS_PER_ZONE {
            return Err(TransitionError::TooManyTransitions {
                limit: MAX_TRANSITIONS_PER_ZONE,
            });
        }
        self.rest.push((at, span));
        Ok(())
    }

    fn add_fixed_saving(
        &mut self,
        timespan: &ZoneInfo,
        amount: i64,
        dst_offset: &mut i64,
        utc_offset: i64,
        insert_start_transition: &mut bool,
        start_zone_id: &mut Option<(String, bool)>,
    ) -> Result<(), TransitionError> {
        *dst_offset = amount;
        let (name, is_offset) = format_bounded(&timespan.format, utc_offset, *dst_offset, None)?;
        *start_zone_id = Some((name.clone(), is_offset));

        if *insert_start_transition {
            let time = self.start_time.ok_or(TransitionError::MissingUntil)?;
            let span = InternalTimespan {
                utc_offset: timespan.offset,
                dst_offset: *dst_offset,
                name,
                is_offset,
            };

            self.push_transition(time, span)?;
            *insert_start_transition = false;
        } else {
            self.first = Some(InternalTimespan {
                utc_offset,
                dst_offset: *dst_offset,
                name,
                is_offset,
            });
        }
        Ok(())
    }

    #[allow(unused_results)]
    #[allow(clippy::too_many_arguments)]
    fn add_multiple_saving(
        &mut self,
        timespan: &ZoneInfo,
        rules: &[RuleInfo],
        dst_offset: &mut i64,
        use_until: bool,
        utc_offset: i64,
        insert_start_transition: &mut bool,
        start_zone_id: &mut Option<(String, bool)>,
        start_utc_offset: &mut i64,
        start_dst_offset: &mut i64,
    ) -> Result<(), TransitionError> {
        if rules.len() > MAX_RULES_PER_RULESET {
            return Err(TransitionError::TooManyRules {
                count: rules.len(),
                limit: MAX_RULES_PER_RULESET,
            });
        }

        let end_time = if use_until {
            Some(timespan.end_time.ok_or(TransitionError::MissingUntil)?)
        } else {
            None
        };

        let max_year = match end_time {
            Some(et) => et.year().min(2099),
            None => 2099,
        };
        if max_year < 1800 {
            return Ok(());
        }

        struct ActiveRule<'a> {
            rule: &'a RuleInfo,
            base_ts: i128,
            used: bool,
        }

        for year in 1800..=max_year {
            self.consume_work(1)?;
            let mut activated_rules = Vec::new();
            for rule in rules {
                self.consume_work(1)?;
                if !rule.applies_to_year(year) {
                    continue;
                }
                if activated_rules.len() >= MAX_ACTIVE_RULES_PER_YEAR {
                    return Err(TransitionError::TooManyActiveRules {
                        year,
                        count: activated_rules.len() + 1,
                        limit: MAX_ACTIVE_RULES_PER_YEAR,
                    });
                }
                let base_ts = rule
                    .try_absolute_datetime_i128(year, utc_offset, 0)
                    .map_err(TransitionError::InvalidDate)?;
                activated_rules.push(ActiveRule {
                    rule,
                    base_ts,
                    used: false,
                });
            }

            if activated_rules.is_empty() {
                if let Some(end_time) = end_time {
                    let ts = end_time
                        .try_to_timestamp_i128(utc_offset, *dst_offset)
                        .map_err(TransitionError::InvalidDate)?;
                    self.until_time_i128 = Some(ts);
                }
                continue;
            }

            loop {
                if let Some(end_time) = end_time {
                    let ts = end_time
                        .try_to_timestamp_i128(utc_offset, *dst_offset)
                        .map_err(TransitionError::InvalidDate)?;
                    self.until_time_i128 = Some(ts);
                }

                self.consume_work(activated_rules.len())?;

                // Find the minimum unused rule and its start time based on the current
                // UTC and DST offsets. Wall-time rules depend on `dst_offset`.
                let mut earliest: Option<(usize, i128)> = None;
                for (i, active) in activated_rules.iter().enumerate() {
                    if active.used {
                        continue;
                    }
                    let ts_i128 = if matches!(active.rule.time_type, TimeType::Wall) {
                        active.base_ts - *dst_offset as i128
                    } else {
                        active.base_ts
                    };
                    if earliest.is_none_or(|(_, best)| ts_i128 < best) {
                        earliest = Some((i, ts_i128));
                    }
                }

                let (pos, earliest_at_i128) = match earliest {
                    Some(pair) => pair,
                    None => break,
                };

                activated_rules[pos].used = true;
                let earliest_rule = activated_rules[pos].rule;

                if let Some(until_time_i128) = self.until_time_i128 {
                    if use_until && earliest_at_i128 >= until_time_i128 {
                        break;
                    }
                }

                let earliest_at = i64::try_from(earliest_at_i128)
                    .map_err(|_| TransitionError::TimestampOutOfRange)?;

                *dst_offset = earliest_rule.time_to_add;

                if *insert_start_transition {
                    let start_time = self.start_time.ok_or(TransitionError::MissingUntil)?;
                    if earliest_at == start_time {
                        *insert_start_transition = false;
                    } else if earliest_at < start_time {
                        *start_utc_offset = timespan.offset;
                        *start_dst_offset = *dst_offset;
                        let formatted = format_bounded(
                            &timespan.format,
                            utc_offset,
                            *dst_offset,
                            earliest_rule.letters.as_ref(),
                        )?;
                        let _ = start_zone_id.replace(formatted);
                        continue;
                    } else if start_zone_id.is_none()
                        && (*start_utc_offset as i128 + *start_dst_offset as i128)
                            == (timespan.offset as i128 + *dst_offset as i128)
                    {
                        let formatted = format_bounded(
                            &timespan.format,
                            utc_offset,
                            *dst_offset,
                            earliest_rule.letters.as_ref(),
                        )?;
                        let _ = start_zone_id.replace(formatted);
                    }
                }

                let (name, is_offset) = format_bounded(
                    &timespan.format,
                    timespan.offset,
                    earliest_rule.time_to_add,
                    earliest_rule.letters.as_ref(),
                )?;
                let span = InternalTimespan {
                    utc_offset: timespan.offset,
                    dst_offset: earliest_rule.time_to_add,
                    name,
                    is_offset,
                };
                self.push_transition(earliest_at, span)?;
            }
        }
        Ok(())
    }

    fn build(mut self) -> Result<InternalTimespanSet, TransitionError> {
        self.rest.sort_by_key(|a| a.0);

        let first = match self.first {
            Some(ft) => ft,
            None => self
                .rest
                .iter()
                .find(|t| t.1.dst_offset == 0)
                .or_else(|| self.rest.first())
                .ok_or(TransitionError::EmptyTimespanSet)?
                .1
                .clone(),
        };

        let mut zoneset = InternalTimespanSet {
            first,
            rest: self.rest,
        };
        optimise_internal(&mut zoneset);
        Ok(zoneset)
    }
}

#[allow(unused_results)]
fn optimise_internal(transitions: &mut InternalTimespanSet) {
    let mut from_i = 0;
    let mut to_i = 0;

    while from_i < transitions.rest.len() {
        if to_i > 1 {
            let from = transitions.rest[from_i].0;
            let to = transitions.rest[to_i - 1].0;
            if (from as i128 + transitions.rest[to_i - 1].1.total_offset())
                <= (to as i128 + transitions.rest[to_i - 2].1.total_offset())
            {
                transitions.rest[to_i - 1].1 = transitions.rest[from_i].1.clone();
                from_i += 1;
                continue;
            }
        }

        if to_i == 0 || transitions.rest[to_i - 1].1 != transitions.rest[from_i].1 {
            transitions.rest[to_i] = transitions.rest[from_i].clone();
            to_i += 1;
        }

        from_i += 1
    }

    transitions.rest.truncate(to_i);

    if !transitions.rest.is_empty() && transitions.first == transitions.rest[0].1 {
        transitions.rest.remove(0);
    }
}

#[cfg(test)]
#[allow(unused_results)] // for remove
fn optimise(transitions: &mut FixedTimespanSet) {
    let mut from_i = 0;
    let mut to_i = 0;

    while from_i < transitions.rest.len() {
        if to_i > 1 {
            let from = transitions.rest[from_i].0;
            let to = transitions.rest[to_i - 1].0;
            if (from as i128 + transitions.rest[to_i - 1].1.total_offset() as i128)
                <= (to as i128 + transitions.rest[to_i - 2].1.total_offset() as i128)
            {
                transitions.rest[to_i - 1].1 = transitions.rest[from_i].1.clone();
                from_i += 1;
                continue;
            }
        }

        if to_i == 0 || transitions.rest[to_i - 1].1 != transitions.rest[from_i].1 {
            transitions.rest[to_i] = transitions.rest[from_i].clone();
            to_i += 1;
        }

        from_i += 1
    }

    transitions.rest.truncate(to_i);

    if !transitions.rest.is_empty() && transitions.first == transitions.rest[0].1 {
        transitions.rest.remove(0);
    }
}

#[cfg(test)]
mod test {
    use super::optimise;
    use super::*;
    use crate::line::{ChangeTime, DaySpec, Month, Year};
    use crate::table::Format;

    // Allow unused results in test code, because the only ‘results’ that
    // we need to ignore are the ones from inserting and removing from
    // tables and vectors. And as we set them up ourselves, they’re bound
    // to be correct, otherwise the tests would fail!
    #[test]
    #[allow(unused_results)]
    fn optimise_macquarie() {
        let mut transitions = FixedTimespanSet {
            first: FixedTimespan {
                utc_offset: 0,
                dst_offset: 0,
                name: "zzz".to_owned(),
            },
            rest: vec![
                (
                    -2_214_259_200,
                    FixedTimespan {
                        utc_offset: 36000,
                        dst_offset: 0,
                        name: "AEST".to_owned(),
                    },
                ),
                (
                    -1_680_508_800,
                    FixedTimespan {
                        utc_offset: 36000,
                        dst_offset: 3600,
                        name: "AEDT".to_owned(),
                    },
                ),
                (
                    -1_669_892_400,
                    FixedTimespan {
                        utc_offset: 36000,
                        dst_offset: 3600,
                        name: "AEDT".to_owned(),
                    },
                ), // gets removed
                (
                    -1_665_392_400,
                    FixedTimespan {
                        utc_offset: 36000,
                        dst_offset: 0,
                        name: "AEST".to_owned(),
                    },
                ),
                (
                    -1_601_719_200,
                    FixedTimespan {
                        utc_offset: 0,
                        dst_offset: 0,
                        name: "zzz".to_owned(),
                    },
                ),
                (
                    -687_052_800,
                    FixedTimespan {
                        utc_offset: 36000,
                        dst_offset: 0,
                        name: "AEST".to_owned(),
                    },
                ),
                (
                    -94_730_400,
                    FixedTimespan {
                        utc_offset: 36000,
                        dst_offset: 0,
                        name: "AEST".to_owned(),
                    },
                ), // also gets removed
                (
                    -71_136_000,
                    FixedTimespan {
                        utc_offset: 36000,
                        dst_offset: 3600,
                        name: "AEDT".to_owned(),
                    },
                ),
                (
                    -55_411_200,
                    FixedTimespan {
                        utc_offset: 36000,
                        dst_offset: 0,
                        name: "AEST".to_owned(),
                    },
                ),
                (
                    -37_267_200,
                    FixedTimespan {
                        utc_offset: 36000,
                        dst_offset: 3600,
                        name: "AEDT".to_owned(),
                    },
                ),
                (
                    -25_776_000,
                    FixedTimespan {
                        utc_offset: 36000,
                        dst_offset: 0,
                        name: "AEST".to_owned(),
                    },
                ),
                (
                    -5_817_600,
                    FixedTimespan {
                        utc_offset: 36000,
                        dst_offset: 3600,
                        name: "AEDT".to_owned(),
                    },
                ),
            ],
        };

        let mut result = transitions.clone();
        result.rest.remove(6);
        result.rest.remove(2);

        optimise(&mut transitions);
        assert_eq!(transitions, result);
    }

    #[test]
    fn timespans_malformed_table_no_panic() {
        // Empty zoneset
        let mut table = Table::default();
        table.zonesets.insert("Empty".to_owned(), Vec::new());
        assert!(table.timespans("Empty").is_none());

        // Missing ruleset
        table.zonesets.insert(
            "MissingRule".to_owned(),
            vec![ZoneInfo {
                offset: 0,
                format: Format::new("TST"),
                saving: Saving::Multiple("NoSuchRule".to_owned()),
                end_time: None,
            }],
        );
        assert!(table.timespans("MissingRule").is_none());

        // Non-final zone line missing end_time
        table.zonesets.insert(
            "MissingUntil".to_owned(),
            vec![
                ZoneInfo {
                    offset: 0,
                    format: Format::new("TST"),
                    saving: Saving::NoSaving,
                    end_time: None,
                },
                ZoneInfo {
                    offset: 3600,
                    format: Format::new("TST2"),
                    saving: Saving::NoSaving,
                    end_time: None,
                },
            ],
        );
        assert!(table.timespans("MissingUntil").is_none());

        // Empty ruleset / no standard rule when first is None
        table.rulesets.insert("EmptyRules".to_owned(), Vec::new());
        table.zonesets.insert(
            "NoStandardRule".to_owned(),
            vec![ZoneInfo {
                offset: 0,
                format: Format::new("TST"),
                saving: Saving::Multiple("EmptyRules".to_owned()),
                end_time: None,
            }],
        );
        assert!(table.timespans("NoStandardRule").is_none());

        // Extreme timestamps in optimise() do not overflow
        let mut extreme = FixedTimespanSet {
            first: FixedTimespan {
                utc_offset: 0,
                dst_offset: 0,
                name: "A".to_owned(),
            },
            rest: vec![
                (
                    i64::MAX - 10,
                    FixedTimespan {
                        utc_offset: i64::MAX,
                        dst_offset: i64::MAX,
                        name: "B".to_owned(),
                    },
                ),
                (
                    i64::MAX,
                    FixedTimespan {
                        utc_offset: i64::MAX,
                        dst_offset: i64::MAX,
                        name: "C".to_owned(),
                    },
                ),
            ],
        };
        optimise(&mut extreme);
    }

    #[test]
    fn adversarial_bounds_reject_cpu_and_memory_abuse() {
        // 1. Too many active rules in one year (> MAX_ACTIVE_RULES_PER_YEAR)
        let mut table = Table::default();
        let rules: Vec<RuleInfo> = (0..=MAX_ACTIVE_RULES_PER_YEAR)
            .map(|i| RuleInfo {
                from_year: Year::Number(2000),
                to_year: Some(Year::Number(2000)),
                month: Month::January,
                day: DaySpec::Ordinal(1),
                time: i as i64,
                time_type: TimeType::UTC,
                time_to_add: (i as i64 % 2) * 3600,
                letters: None,
            })
            .collect();
        table.rulesets.insert("Flood".to_owned(), rules);
        table.zonesets.insert(
            "Test/Flood".to_owned(),
            vec![ZoneInfo {
                offset: 0,
                saving: Saving::Multiple("Flood".to_owned()),
                format: Format::new("T"),
                end_time: None,
            }],
        );
        assert!(matches!(
            table.try_timespans("Test/Flood"),
            Err(TransitionError::TooManyActiveRules { year: 2000, .. })
        ));
        assert!(table.timespans("Test/Flood").is_none());

        // 2. Too many transitions across 300 years (> MAX_TRANSITIONS_PER_ZONE)
        let mut table2 = Table::default();
        let many_rules: Vec<RuleInfo> = (0i64..40)
            .map(|i| RuleInfo {
                from_year: Year::Minimum,
                to_year: Some(Year::Maximum),
                month: Month::January,
                day: DaySpec::Ordinal(1),
                time: i * 60,
                time_type: TimeType::UTC,
                time_to_add: (i % 2) * 3600,
                letters: None,
            })
            .collect();
        table2.rulesets.insert("Many".to_owned(), many_rules);
        table2.zonesets.insert(
            "Test/Many".to_owned(),
            vec![ZoneInfo {
                offset: 0,
                saving: Saving::Multiple("Many".to_owned()),
                format: Format::new("T%s"),
                end_time: None,
            }],
        );
        assert_eq!(
            table2.try_timespans("Test/Many"),
            Err(TransitionError::TooManyTransitions {
                limit: MAX_TRANSITIONS_PER_ZONE
            })
        );

        // 3. Long %s expansion (> MAX_FORMATTED_NAME_LEN)
        let mut table3 = Table::default();
        table3.rulesets.insert(
            "Long".to_owned(),
            vec![RuleInfo {
                from_year: Year::Number(2000),
                to_year: Some(Year::Number(2000)),
                month: Month::January,
                day: DaySpec::Ordinal(1),
                time: 0,
                time_type: TimeType::UTC,
                time_to_add: 0,
                letters: Some("ABCD".repeat(5)),
            }],
        );
        table3.zonesets.insert(
            "Test/Long".to_owned(),
            vec![ZoneInfo {
                offset: 0,
                saving: Saving::Multiple("Long".to_owned()),
                format: Format::new("%s%s%s%s"),
                end_time: None,
            }],
        );
        assert_eq!(
            table3.try_timespans("Test/Long"),
            Err(TransitionError::FormattedNameTooLong {
                limit: MAX_FORMATTED_NAME_LEN
            })
        );

        // 4. Too many segments (> MAX_ZONE_SEGMENTS)
        let mut table4 = Table::default();
        let mut segs: Vec<ZoneInfo> = (0..=MAX_ZONE_SEGMENTS)
            .map(|i| ZoneInfo {
                offset: 0,
                saving: Saving::NoSaving,
                format: Format::new("T"),
                end_time: Some(ChangeTime::UntilYear(Year::Number(1900 + i as i64))),
            })
            .collect();
        segs.last_mut().unwrap().end_time = None;
        table4.zonesets.insert("Test/ManySegs".to_owned(), segs);
        assert_eq!(
            table4.try_timespans("Test/ManySegs"),
            Err(TransitionError::TooManySegments {
                count: MAX_ZONE_SEGMENTS + 1,
                limit: MAX_ZONE_SEGMENTS,
            })
        );

        // 5. High-rule, zero-emission adversarial zone hits cumulative work budget
        let mut table5 = Table::default();
        let non_matching_rules: Vec<RuleInfo> = (0..1_000)
            .map(|i| RuleInfo {
                from_year: Year::Number(1700),
                to_year: Some(Year::Number(1700 + (i % 10))),
                month: Month::January,
                day: DaySpec::Ordinal(1),
                time: 0,
                time_type: TimeType::UTC,
                time_to_add: 0,
                letters: None,
            })
            .collect();
        table5
            .rulesets
            .insert("Dormant".to_owned(), non_matching_rules);
        let mut dormant_segs: Vec<ZoneInfo> = (0..5)
            .map(|i| ZoneInfo {
                offset: 0,
                saving: Saving::Multiple("Dormant".to_owned()),
                format: Format::new("D"),
                end_time: Some(ChangeTime::UntilYear(Year::Number(2000 + i))),
            })
            .collect();
        dormant_segs.push(ZoneInfo {
            offset: 0,
            saving: Saving::NoSaving,
            format: Format::new("D"),
            end_time: None,
        });
        table5
            .zonesets
            .insert("Test/DormantWork".to_owned(), dormant_segs);
        assert_eq!(
            table5.try_timespans("Test/DormantWork"),
            Err(TransitionError::WorkBudgetExceeded {
                limit: MAX_RULE_EVALUATIONS_PER_ZONE
            })
        );
    }

    #[test]
    fn rejects_invalid_calendar_and_until_ordering() {
        // Terminal UNTIL rejected
        let mut table = Table::default();
        table.zonesets.insert(
            "TermUntil".to_owned(),
            vec![ZoneInfo {
                offset: 0,
                saving: Saving::NoSaving,
                format: Format::new("TST"),
                end_time: Some(ChangeTime::UntilYear(Year::Number(2000))),
            }],
        );
        assert_eq!(
            table.try_timespans("TermUntil"),
            Err(TransitionError::UnexpectedTerminalUntil)
        );

        // Non-increasing UNTIL rejected
        table.zonesets.insert(
            "OutOfOrder".to_owned(),
            vec![
                ZoneInfo {
                    offset: 0,
                    saving: Saving::NoSaving,
                    format: Format::new("A"),
                    end_time: Some(ChangeTime::UntilYear(Year::Number(2000))),
                },
                ZoneInfo {
                    offset: 3600,
                    saving: Saving::NoSaving,
                    format: Format::new("B"),
                    end_time: Some(ChangeTime::UntilYear(Year::Number(1999))),
                },
                ZoneInfo {
                    offset: 7200,
                    saving: Saving::NoSaving,
                    format: Format::new("C"),
                    end_time: None,
                },
            ],
        );
        assert!(matches!(
            table.try_timespans("OutOfOrder"),
            Err(TransitionError::NonIncreasingUntil { .. })
        ));

        // Two distinct extreme UNTIL years exceeding i64::MAX are rejected cleanly
        // with TimestampOutOfRange (and non-increasing extreme UNTIL values are distinguished in i128).
        table.zonesets.insert(
            "ExtremeIncreasing".to_owned(),
            vec![
                ZoneInfo {
                    offset: 0,
                    saving: Saving::NoSaving,
                    format: Format::new("A"),
                    end_time: Some(ChangeTime::UntilYear(Year::Number(300_000_000_000))),
                },
                ZoneInfo {
                    offset: 3600,
                    saving: Saving::NoSaving,
                    format: Format::new("B"),
                    end_time: Some(ChangeTime::UntilYear(Year::Number(400_000_000_000))),
                },
                ZoneInfo {
                    offset: 7200,
                    saving: Saving::NoSaving,
                    format: Format::new("C"),
                    end_time: None,
                },
            ],
        );
        assert_eq!(
            table.try_timespans("ExtremeIncreasing"),
            Err(TransitionError::TimestampOutOfRange)
        );

        // First segment at top of i64 range, second segment strictly larger (> i64::MAX):
        // must report TimestampOutOfRange rather than falsely reporting NonIncreasingUntil.
        table.zonesets.insert(
            "BorderToOutOfRange".to_owned(),
            vec![
                ZoneInfo {
                    offset: 0,
                    saving: Saving::NoSaving,
                    format: Format::new("A"),
                    end_time: Some(ChangeTime::UntilDay(
                        Year::Number(292_277_026_596),
                        Month::January,
                        DaySpec::Ordinal(1),
                    )),
                },
                ZoneInfo {
                    offset: 3600,
                    saving: Saving::NoSaving,
                    format: Format::new("B"),
                    end_time: Some(ChangeTime::UntilYear(Year::Number(292_277_026_597))),
                },
                ZoneInfo {
                    offset: 7200,
                    saving: Saving::NoSaving,
                    format: Format::new("C"),
                    end_time: None,
                },
            ],
        );
        assert_eq!(
            table.try_timespans("BorderToOutOfRange"),
            Err(TransitionError::TimestampOutOfRange)
        );

        // Multi-year rule hitting Feb 29 in a non-leap year rejected
        table.rulesets.insert(
            "BadFeb".to_owned(),
            vec![RuleInfo {
                from_year: Year::Number(2020),
                to_year: Some(Year::Number(2025)),
                month: Month::February,
                day: DaySpec::Ordinal(29),
                time: 0,
                time_type: TimeType::UTC,
                time_to_add: 3600,
                letters: None,
            }],
        );
        table.zonesets.insert(
            "BadFebZone".to_owned(),
            vec![ZoneInfo {
                offset: 0,
                saving: Saving::Multiple("BadFeb".to_owned()),
                format: Format::new("B"),
                end_time: None,
            }],
        );
        assert!(matches!(
            table.try_timespans("BadFebZone"),
            Err(TransitionError::InvalidDate(_))
        ));
    }

    #[test]
    fn codegen_timespans_preserves_percent_z_provenance() {
        let mut table = Table::default();
        table.zonesets.insert(
            "Mixed".to_owned(),
            vec![
                ZoneInfo {
                    offset: -3600,
                    saving: Saving::NoSaving,
                    format: Format::new("-01"),
                    end_time: Some(ChangeTime::UntilYear(Year::Number(2000))),
                },
                ZoneInfo {
                    offset: -3600,
                    saving: Saving::NoSaving,
                    format: Format::new("%z"),
                    end_time: None,
                },
            ],
        );
        let cg = table.try_codegen_timespans("Mixed").unwrap();
        assert_eq!(cg.first.name.as_deref(), Some("-01"));
        assert_eq!(cg.rest.len(), 1);
        assert_eq!(cg.rest[0].1.name, None);
    }
}
