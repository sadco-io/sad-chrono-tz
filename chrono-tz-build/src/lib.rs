extern crate parse_zoneinfo;
#[cfg(feature = "filter-by-regex")]
extern crate regex;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::env;
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write as IoWrite};
use std::path::{Path, PathBuf};

use parse_zoneinfo::line::Line;
use parse_zoneinfo::structure::Child;
use parse_zoneinfo::table::{Table, TableBuilder};
use parse_zoneinfo::transitions::CodegenTimespan;
use parse_zoneinfo::FILES;

/// The name of the environment variable which possibly holds the filter regex.
#[cfg(feature = "filter-by-regex")]
pub const FILTER_ENV_VAR_NAME: &str = "CHRONO_TZ_TIMEZONE_FILTER";

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "try", "typeof", "unsized", "virtual", "yield", "gen",
];

/// Identifiers declared, imported, or re-exported at the top level of `directory.rs`
/// and the `sad-chrono-tz` crate root.
const DIRECTORY_ROOT_RESERVED: &[&str] = &[
    "IANA_TZDB_VERSION",
    "Tz",
    "directory",
    "timezones",
    "ParseError",
    "TZ_VARIANTS",
    "GapInfo",
    "OffsetComponents",
    "OffsetName",
    "TzOffset",
    "TzOffsetIter",
    "ArchivedTz",
    "ArchivedTzOffset",
    "prebuilt",
    "binary_search",
    "timezone_impl",
    "rkyv",
    "serde",
];

/// Identifiers imported into `directory.rs` submodules (`use super::*` / `use crate::timezones::Tz`).
const DIRECTORY_SUBMODULE_RESERVED: &[&str] = &["Tz", "IANA_TZDB_VERSION"];

/// Identifiers declared or imported in `timezones.rs` (including inside `TimeSpans::timespans`).
const TIMEZONES_RESERVED_IDENTS: &[&str] = &[
    "ABBREVIATIONS",
    "Tz",
    "ParseError",
    "TZ_VARIANTS",
    "TIMEZONES",
    "TIMEZONES_UNCASED",
    "MAX_TZ_NAME_LEN",
    "TimeSpans",
    "FixedTimespanSet",
    "FixedTimespan",
    "Debug",
    "Display",
    "Formatter",
    "FromStr",
    "UncasedStr",
    "fmt",
    "IANA_TZDB_VERSION",
];

fn matches_reserved(ident: &str, reserved: &[&str]) -> bool {
    let upper = ident.to_ascii_uppercase();
    reserved
        .iter()
        .any(|&r| r == ident || r.to_ascii_uppercase() == upper)
}

fn is_valid_rust_ident(ident: &str) -> bool {
    if ident.is_empty() || ident == "_" || RUST_KEYWORDS.contains(&ident) {
        return false;
    }
    let mut bytes = ident.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

// This function is needed until zoneinfo_parse handles comments correctly.
// Technically a '#' symbol could occur between double quotes and should be
// ignored in this case, however this never happens in the tz database as it
// stands.
fn strip_comments(mut line: String) -> String {
    if let Some(pos) = line.find('#') {
        line.truncate(pos);
    };
    line
}

/// Minimum Unix timestamp representable by `sad_chrono::DateTime<Utc>`.
const MIN_CHRONO_UTC_TIMESTAMP: i64 = -8_334_601_228_800;
/// Maximum Unix timestamp representable by `sad_chrono::DateTime<Utc>`.
const MAX_CHRONO_UTC_TIMESTAMP: i64 = 8_210_266_876_799;
/// Minimum signed 20-bit offset component (`-(1 << 19)`).
const PACKED_OFFSET_MIN: i64 = -(1 << 19);
/// Maximum signed 20-bit offset component (`(1 << 19) - 1`).
const PACKED_OFFSET_MAX: i64 = (1 << 19) - 1;

fn try_validate_timespan(zone: &str, ts: &CodegenTimespan) -> Result<(), String> {
    if i32::try_from(ts.utc_offset).is_err() {
        return Err(format!(
            "zone {zone}: utc_offset {} out of i32 range",
            ts.utc_offset
        ));
    }
    if i32::try_from(ts.dst_offset).is_err() {
        return Err(format!(
            "zone {zone}: dst_offset {} out of i32 range",
            ts.dst_offset
        ));
    }
    let total = ts.utc_offset as i128 + ts.dst_offset as i128;
    if i32::try_from(total).is_err() || total.abs() >= 86_400 {
        return Err(format!(
            "zone {zone}: total offset {total} out of FixedOffset bounds (-86400..86400)"
        ));
    }
    if !(PACKED_OFFSET_MIN..=PACKED_OFFSET_MAX).contains(&ts.utc_offset) {
        return Err(format!(
            "zone {zone}: utc_offset {} out of 20-bit packed range ({PACKED_OFFSET_MIN}..={PACKED_OFFSET_MAX})",
            ts.utc_offset
        ));
    }
    if !(PACKED_OFFSET_MIN..=PACKED_OFFSET_MAX).contains(&ts.dst_offset) {
        return Err(format!(
            "zone {zone}: dst_offset {} out of 20-bit packed range ({PACKED_OFFSET_MIN}..={PACKED_OFFSET_MAX})",
            ts.dst_offset
        ));
    }
    if let Some(ref name) = ts.name {
        if name.len() > 64 {
            return Err(format!(
                "zone {zone}: abbreviation `{name}` length {} exceeds 64 bytes",
                name.len()
            ));
        }
    }
    Ok(())
}

fn pack_timespan(
    zone: &str,
    ts: &CodegenTimespan,
    abbreviations: &str,
) -> Result<([u8; 5], u16, u8), String> {
    try_validate_timespan(zone, ts)?;
    let utc = ts.utc_offset as i32;
    let dst = ts.dst_offset as i32;
    let packed = (((utc as i64 as u64) & 0xF_FFFF) << 20) | ((dst as i64 as u64) & 0xF_FFFF);
    let offset = [
        packed as u8,
        (packed >> 8) as u8,
        (packed >> 16) as u8,
        (packed >> 24) as u8,
        (packed >> 32) as u8,
    ];
    let packed_back = (offset[0] as u64)
        | ((offset[1] as u64) << 8)
        | ((offset[2] as u64) << 16)
        | ((offset[3] as u64) << 24)
        | ((offset[4] as u64) << 32);
    let utc_back = (((packed_back as i64) << 24) >> 44) as i32;
    let dst_back = (((packed_back as i64) << 44) >> 44) as i32;
    if utc_back != utc || dst_back != dst {
        return Err(format!(
            "zone {zone}: packed offset round-trip mismatch for utc={utc}, dst={dst}"
        ));
    }
    let (abbr_start, abbr_len) = match ts.name.as_deref() {
        Some(name) => {
            let start = abbreviations.find(name).ok_or_else(|| {
                format!("zone {zone}: abbreviation `{name}` missing from interned table")
            })?;
            let abbr_start = u16::try_from(start).map_err(|_| {
                format!("zone {zone}: abbreviation start offset {start} exceeds u16::MAX")
            })?;
            let abbr_len = u8::try_from(name.len()).map_err(|_| {
                format!(
                    "zone {zone}: abbreviation length {} exceeds u8::MAX",
                    name.len()
                )
            })?;
            if abbr_len == u8::MAX || &abbreviations[start..start + name.len()] != name {
                return Err(format!(
                    "zone {zone}: invalid packed abbreviation slice for `{name}`"
                ));
            }
            (abbr_start, abbr_len)
        }
        None => (0, u8::MAX),
    };
    Ok((offset, abbr_start, abbr_len))
}

fn try_resolve_link<'a>(table: &'a Table, zone: &'a str) -> Result<&'a str, String> {
    let mut current = zone;
    let mut visited = HashSet::new();
    loop {
        let in_zonesets = table.zonesets.contains_key(current);
        let link_target = table.links.get(current);
        if in_zonesets && link_target.is_some() {
            return Err(format!(
                "zone/link collision for `{current}` while resolving `{zone}`"
            ));
        }
        match link_target {
            Some(target) => {
                if !visited.insert(current) {
                    return Err(format!(
                        "cyclic link detected while resolving zone `{zone}`"
                    ));
                }
                current = target.as_str();
            }
            None if in_zonesets => return Ok(current),
            None => {
                return Err(format!(
                    "broken link for zone `{zone}`: target `{current}` does not exist"
                ));
            }
        }
    }
}

// Generate a list of the time zone periods beyond the first that apply
// to this zone, as a string representation of a static slice.
fn format_rest(
    zone: &str,
    rest: &[(i64, CodegenTimespan)],
    abbreviations: &str,
) -> Result<String, String> {
    let mut ret = "&[\n".to_string();
    for (start, ts) in rest {
        let (offset, abbr_start, abbr_len) = pack_timespan(zone, ts, abbreviations)?;
        let _ = writeln!(
            ret,
            "                ({start}, FixedTimespan {{ \
             offset: {offset:?}, abbr_len: {abbr_len}, abbr_start: {abbr_start} \
             }}),"
        );
    }
    ret.push_str("            ]");
    Ok(ret)
}

// Convert all '/' to '__', all '+' to 'Plus' and '-' followed by a digit to
// 'Minus', removing other '-' hyphens. This is so the names can be used as
// Rust identifiers.
fn convert_bad_chars(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '/' => out.push_str("__"),
            '+' => out.push_str("Plus"),
            '-' => {
                if chars.peek().copied().is_some_and(char::is_numeric) {
                    out.push_str("Minus");
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

fn validate_zones(table: &Table, zones: &BTreeSet<&str>, uncased: bool) -> Result<(), String> {
    if zones.is_empty() {
        return Err("timezone selection is empty; filter matched zero timezones".to_owned());
    }

    for key in table.zonesets.keys() {
        if table.links.contains_key(key) {
            return Err(format!(
                "zone/link collision: `{key}` appears as both a Zone and a Link"
            ));
        }
    }

    let mut variant_idents: HashMap<String, &str> = HashMap::with_capacity(zones.len());
    let mut target_const_idents: HashMap<String, &str> = HashMap::with_capacity(zones.len());
    let mut uncased_names: HashMap<String, &str> = HashMap::new();
    let mut root_idents: HashMap<String, &str> = HashMap::new();
    let mut mod_child_idents: HashMap<String, HashMap<String, String>> = HashMap::new();

    for &zone in zones {
        if zone.is_empty() || zone.split('/').any(str::is_empty) {
            return Err(format!("invalid empty segment in timezone name `{zone}`"));
        }
        let depth = zone.split('/').count();
        if depth > 3 {
            return Err(format!(
                "timezone `{zone}` has {depth} segments; depth > 3 is not supported"
            ));
        }

        let variant = convert_bad_chars(zone);
        if !is_valid_rust_ident(&variant) {
            return Err(format!(
                "timezone `{zone}` normalizes to invalid Rust identifier `{variant}`"
            ));
        }
        if matches_reserved(&variant, TIMEZONES_RESERVED_IDENTS) {
            return Err(format!(
                "timezone `{zone}` normalizes to reserved identifier `{variant}` in `timezones.rs`"
            ));
        }
        if let Some(&prev) = variant_idents.get(&variant) {
            return Err(format!(
                "normalization collision: `{prev}` and `{zone}` both normalize to `Tz::{variant}`"
            ));
        }
        variant_idents.insert(variant, zone);

        if uncased {
            let lower = zone.to_ascii_lowercase();
            if let Some(&prev) = uncased_names.get(&lower) {
                return Err(format!(
                    "case-insensitive collision: `{prev}` and `{zone}` collide under `case-insensitive`"
                ));
            }
            uncased_names.insert(lower, zone);
        }

        let resolved = try_resolve_link(table, zone)?;
        let resolved_upper = convert_bad_chars(resolved).to_uppercase();
        if !is_valid_rust_ident(&resolved_upper) {
            return Err(format!(
                "resolved target `{resolved}` normalizes to invalid Rust const `{resolved_upper}`"
            ));
        }
        if matches_reserved(&resolved_upper, TIMEZONES_RESERVED_IDENTS) {
            return Err(format!(
                "resolved target `{resolved}` normalizes to reserved identifier `{resolved_upper}` in `timezones.rs`"
            ));
        }
        if let Some(&prev) = target_const_idents.get(&resolved_upper) {
            if prev != resolved {
                return Err(format!(
                    "normalization collision: canonical zones `{prev}` and `{resolved}` both map to `const {resolved_upper}`"
                ));
            }
        } else {
            target_const_idents.insert(resolved_upper, resolved);
        }

        let segments: Vec<&str> = zone.split('/').collect();
        for (idx, &seg) in segments.iter().enumerate() {
            let seg_ident = convert_bad_chars(seg);
            if !is_valid_rust_ident(&seg_ident) {
                return Err(format!(
                    "segment `{seg}` of timezone `{zone}` normalizes to invalid Rust identifier `{seg_ident}`"
                ));
            }
            let reserved = if idx == 0 {
                DIRECTORY_ROOT_RESERVED
            } else {
                DIRECTORY_SUBMODULE_RESERVED
            };
            if matches_reserved(&seg_ident, reserved) {
                return Err(format!(
                    "segment `{seg}` of timezone `{zone}` (`{seg_ident}`) collides with reserved generator identifier"
                ));
            }
        }

        let first_ident = convert_bad_chars(segments[0]);
        if let Some(&prev_seg) = root_idents.get(&first_ident) {
            if prev_seg != segments[0] {
                return Err(format!(
                    "directory identifier collision at root: `{prev_seg}` and `{}` both normalize to `{first_ident}`",
                    segments[0]
                ));
            }
        } else {
            root_idents.insert(first_ident.clone(), segments[0]);
        }

        if segments.len() == 1 {
            let entry = mod_child_idents.entry(String::new()).or_default();
            let tag = format!("zone:{zone}");
            if let Some(prev) = entry.get(&first_ident) {
                if prev != &tag {
                    return Err(format!(
                        "directory namespace collision at root between `{prev}` and `{zone}` (`{first_ident}`)"
                    ));
                }
            } else {
                entry.insert(first_ident, tag);
            }
        } else {
            // Ensure no top-level const has the same normalized name as this module.
            let root_entry = mod_child_idents.entry(String::new()).or_default();
            let mod_tag = format!("mod:{}", segments[0]);
            if let Some(prev) = root_entry.get(&first_ident) {
                if prev != &mod_tag {
                    return Err(format!(
                        "directory namespace collision at root between `{prev}` and module `{}` (`{first_ident}`)",
                        segments[0]
                    ));
                }
            } else {
                root_entry.insert(first_ident, mod_tag);
            }

            let second_ident = convert_bad_chars(segments[1]);
            let level1_entry = mod_child_idents.entry(segments[0].to_owned()).or_default();
            if let Some(prev) = level1_entry.get(&second_ident) {
                let expected = if segments.len() == 2 {
                    format!("{}/{}", segments[0], segments[1])
                } else {
                    format!("mod:{}/{}", segments[0], segments[1])
                };
                if prev != &expected {
                    return Err(format!(
                        "directory namespace collision in `{}` between `{prev}` and `{zone}` (`{second_ident}`)",
                        segments[0]
                    ));
                }
            } else {
                let tag = if segments.len() == 2 {
                    format!("{}/{}", segments[0], segments[1])
                } else {
                    format!("mod:{}/{}", segments[0], segments[1])
                };
                level1_entry.insert(second_ident, tag);
            }

            if segments.len() == 3 {
                let parent_key = format!("{}/{}", segments[0], segments[1]);
                let third_ident = convert_bad_chars(segments[2]);
                let level2_entry = mod_child_idents.entry(parent_key.clone()).or_default();
                if let Some(prev) = level2_entry.get(&third_ident) {
                    if prev != zone {
                        return Err(format!(
                            "directory namespace collision in `{parent_key}` between `{prev}` and `{zone}` (`{third_ident}`)"
                        ));
                    }
                } else {
                    level2_entry.insert(third_ident, zone.to_owned());
                }
            }
        }
    }

    Ok(())
}

// The timezone file contains impls of `Timespans` for all timezones in the
// database. The `Wrap` wrapper in the `timezone_impl` module then implements
// TimeZone for any contained struct that implements `Timespans`.
fn render_timezone_file(
    table: &Table,
    zones: &BTreeSet<&str>,
    uncased: bool,
) -> Result<String, String> {
    if zones.len() > u16::MAX as usize {
        return Err(format!(
            "zone count {} exceeds u16::MAX ({})",
            zones.len(),
            u16::MAX
        ));
    }

    let mut targets = BTreeSet::new();
    for &z in zones {
        targets.insert(try_resolve_link(table, z)?);
    }

    let mut target_timespans = Vec::with_capacity(targets.len());
    let mut unique_abbrs: BTreeSet<String> = BTreeSet::new();
    for zone in targets {
        let timespans = table
            .try_codegen_timespans(zone)
            .map_err(|e| format!("failed to build timespans for `{zone}`: {e}"))?;
        if timespans.rest.len() > u16::MAX as usize {
            return Err(format!(
                "zone `{zone}` has {} transition spans, exceeding u16::MAX ({})",
                timespans.rest.len(),
                u16::MAX
            ));
        }
        try_validate_timespan(zone, &timespans.first)?;
        if let Some(ref name) = timespans.first.name {
            unique_abbrs.insert(name.clone());
        }
        for (ts_unix, ts) in &timespans.rest {
            if !(MIN_CHRONO_UTC_TIMESTAMP..=MAX_CHRONO_UTC_TIMESTAMP).contains(ts_unix) {
                return Err(format!(
                    "zone `{zone}`: transition timestamp {ts_unix} out of sad-chrono DateTime<Utc> range ({MIN_CHRONO_UTC_TIMESTAMP}..={MAX_CHRONO_UTC_TIMESTAMP})"
                ));
            }
            try_validate_timespan(zone, ts)?;
            if let Some(ref name) = ts.name {
                unique_abbrs.insert(name.clone());
            }
        }
        target_timespans.push((zone, timespans));
    }

    // Intern unique abbreviations longest-first so shorter suffixes/substrings share bytes.
    let mut sorted_abbrs: Vec<&str> = unique_abbrs.iter().map(String::as_str).collect();
    sorted_abbrs.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    let mut abbreviations_str = String::new();
    for abbr in sorted_abbrs {
        if !abbreviations_str.contains(abbr) {
            abbreviations_str.push_str(abbr);
        }
    }
    if abbreviations_str.len() > u16::MAX as usize {
        return Err(format!(
            "interned abbreviation table length {} exceeds u16::MAX ({})",
            abbreviations_str.len(),
            u16::MAX
        ));
    }

    let mut out = String::with_capacity(64 * 1024);
    let max_tz_name_len = zones.iter().map(|z| z.len()).max().unwrap_or(0);

    writeln!(out, "use core::fmt::{{self, Debug, Display, Formatter}};").unwrap();
    writeln!(out, "use core::str::FromStr;\n").unwrap();
    writeln!(
        out,
        "use crate::timezone_impl::{{TimeSpans, FixedTimespanSet, FixedTimespan}};\n"
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const ABBREVIATIONS: &str = {abbreviations_str:?};\n"
    )
    .unwrap();
    writeln!(out, "const MAX_TZ_NAME_LEN: usize = {max_tz_name_len};\n").unwrap();
    writeln!(
        out,
        "/// TimeZones built at compile time from the tz database
///
/// This implements [`TimeZone`](https://docs.rs/sad-chrono/0.5/sad_chrono/offset/trait.TimeZone.html)
/// so that it may be used in and to construct sad-chrono's `DateTime` type.
/// [`Default`] is implemented for [`Tz`] when `UTC` is included in the compiled zone set.
/// See the root module documentation for details."
    )
    .unwrap();
    writeln!(out, "#[derive(Clone, Copy, PartialEq, Eq, Hash)]").unwrap();
    writeln!(
        out,
        r#"#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]"#
    )
    .unwrap();
    writeln!(out, "pub enum Tz {{").unwrap();
    for &zone in zones {
        let zone_name = convert_bad_chars(zone);
        writeln!(out, "    /// {zone}\n    {zone_name},").unwrap();
    }
    writeln!(out, "}}").unwrap();

    let mut map = phf_codegen::Map::new();
    for &zone in zones {
        map.entry(zone, format!("Tz::{}", convert_bad_chars(zone)));
    }
    writeln!(
        out,
        "static TIMEZONES: ::phf::Map<&'static str, Tz> = \n{};",
        map.build()
    )
    .unwrap();

    #[cfg(feature = "case-insensitive")]
    if uncased {
        writeln!(out, "use uncased::UncasedStr;\n").unwrap();
        let mut map = phf_codegen::Map::new();
        for &zone in zones {
            map.entry(
                uncased::UncasedStr::new(zone),
                format!("Tz::{}", convert_bad_chars(zone)),
            );
        }
        writeln!(
            out,
            "static TIMEZONES_UNCASED: ::phf::Map<&'static uncased::UncasedStr, Tz> = \n{};",
            map.build()
        )
        .unwrap();
    }

    writeln!(
        out,
        "#[cfg_attr(feature = \"defmt\", derive(defmt::Format))]"
    )
    .unwrap();
    writeln!(
        out,
        r#"#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ParseError(());

impl Display for ParseError {{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {{
        f.write_str("failed to parse timezone")
    }}
}}

impl core::error::Error for ParseError {{}}

impl FromStr for Tz {{
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {{
        if s.len() > MAX_TZ_NAME_LEN {{
            return Err(ParseError(()));
        }}
        TIMEZONES.get(s).cloned().ok_or(ParseError(()))
    }}
}}
"#
    )
    .unwrap();

    writeln!(
        out,
        "impl Tz {{
    pub fn name(self) -> &'static str {{
        match self {{"
    )
    .unwrap();
    for &zone in zones {
        let zone_name = convert_bad_chars(zone);
        writeln!(out, "            Tz::{zone_name} => {zone:?},").unwrap();
    }
    writeln!(
        out,
        "        }}
    }}"
    )
    .unwrap();

    if uncased {
        writeln!(
            out,
            r#"
    #[cfg(feature = "case-insensitive")]
    /// Parses a timezone string in a case-insensitive way
    pub fn from_str_insensitive(s: &str) -> Result<Self, ParseError> {{
        if s.len() > MAX_TZ_NAME_LEN {{
            return Err(ParseError(()));
        }}
        TIMEZONES_UNCASED.get(s.into()).cloned().ok_or(ParseError(()))
    }}"#
        )
        .unwrap();
    }

    writeln!(out, "}}").unwrap();

    writeln!(
        out,
        "impl Debug for Tz {{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {{
        f.write_str(self.name().as_ref())
    }}
}}\n"
    )
    .unwrap();
    writeln!(
        out,
        "impl Display for Tz {{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {{
        f.write_str(self.name().as_ref())
    }}
}}\n"
    )
    .unwrap();
    if zones.contains("UTC") {
        writeln!(
            out,
            "/// Returns [`Tz::UTC`] (only implemented when `UTC` is included in the compiled zone set).
impl Default for Tz {{
    fn default() -> Self {{
        Tz::UTC
    }}
}}\n"
        )
        .unwrap();
    }
    writeln!(
        out,
        r#"#[cfg(feature = "defmt")]
impl defmt::Format for Tz {{
    fn format(&self, f: defmt::Formatter) {{
        defmt::write!(f, "{{:?}}", self.name());
    }}
}}
"#
    )
    .unwrap();
    writeln!(
        out,
        "impl TimeSpans for Tz {{
    fn timespans(&self) -> FixedTimespanSet {{"
    )
    .unwrap();

    for (zone, timespans) in target_timespans {
        let zone_name = convert_bad_chars(zone);
        let (offset, abbr_start, abbr_len) =
            pack_timespan(zone, &timespans.first, &abbreviations_str)?;
        let rest = format_rest(zone, &timespans.rest, &abbreviations_str)?;
        writeln!(
            out,
            "        const {zone}: FixedTimespanSet = FixedTimespanSet {{
            first: FixedTimespan {{ offset: {offset:?}, abbr_len: {abbr_len}, abbr_start: {abbr_start} }},
            rest: {rest},
        }};\n",
            zone = zone_name.to_uppercase(),
        )
        .unwrap();
    }

    write!(
        out,
        "
        match *self {{
"
    )
    .unwrap();

    for &zone in zones {
        let zone_name = convert_bad_chars(zone);
        let target_name = convert_bad_chars(try_resolve_link(table, zone)?);
        writeln!(
            out,
            "            Tz::{zone_name} => {target_name},",
            target_name = target_name.to_uppercase(),
        )
        .unwrap();
    }
    write!(
        out,
        "        }}
    }}
}}\n"
    )
    .unwrap();

    let doctest_variant = if zones.contains("UTC") {
        "UTC".to_owned()
    } else {
        convert_bad_chars(zones.iter().next().expect("non-empty zones"))
    };
    write!(
        out,
        "/// An array of every known variant
///
/// Useful for iterating over known timezones:
///
/// ```
/// use sad_chrono_tz::{{TZ_VARIANTS, Tz}};
/// assert!(TZ_VARIANTS.iter().any(|v| *v == Tz::{doctest_variant}));
/// ```
pub static TZ_VARIANTS: [Tz; {num}] = [
",
        num = zones.len()
    )
    .unwrap();
    for &zone in zones {
        writeln!(out, "    Tz::{zone},", zone = convert_bad_chars(zone)).unwrap();
    }
    write!(out, "];").unwrap();
    Ok(out)
}

// Create a file containing nice-looking re-exports such as Europe::London
// instead of having to use chrono_tz::timezones::Europe__London
fn render_directory_file(zones: &BTreeSet<&str>, version: &str) -> Result<String, String> {
    let mut out = String::with_capacity(16 * 1024);

    writeln!(out, "use crate::timezones::Tz;\n").unwrap();

    // expose the underlying IANA TZDB version
    writeln!(out, "pub const IANA_TZDB_VERSION: &str = {version:?};\n").unwrap();

    // add the `loose' zone definitions first
    for &zone in zones.iter().filter(|zone| !zone.contains('/')) {
        let zone = convert_bad_chars(zone);
        writeln!(out, "pub const {zone}: Tz = Tz::{zone};").unwrap();
    }
    writeln!(out).unwrap();

    let tree_entries: Vec<_> = parse_zoneinfo::structure::build_tree(zones.iter().copied())
        .into_iter()
        .collect();
    let by_name: BTreeMap<&str, &[Child]> = tree_entries
        .iter()
        .map(|entry| (entry.name, entry.children.as_slice()))
        .collect();

    // now add the `structured' zone names in submodules
    let mut first = true;
    for entry in &tree_entries {
        if entry.name.contains('/') {
            continue;
        }

        match first {
            true => first = false,
            false => writeln!(out).unwrap(),
        }

        let module_name = convert_bad_chars(entry.name);
        writeln!(out, "pub mod {module_name} {{").unwrap();
        if entry
            .children
            .iter()
            .any(|c| matches!(c, Child::TimeZone(_)))
        {
            writeln!(out, "    use super::*;\n").unwrap();
        }
        for &child in &entry.children {
            let name = match child {
                Child::Submodule(name) => name,
                Child::TimeZone(name) => {
                    let name = convert_bad_chars(name);
                    writeln!(out, "    pub const {name}: Tz = Tz::{module_name}__{name};").unwrap();
                    continue;
                }
            };

            let submodule_name = convert_bad_chars(name);
            writeln!(out, "    pub mod {submodule_name} {{").unwrap();
            writeln!(out, "        use crate::timezones::Tz;\n").unwrap();
            let full_name = format!("{}/{name}", entry.name);
            if let Some(&sub_children) = by_name.get(full_name.as_str()) {
                for &sub_child in sub_children {
                    let child_name = match sub_child {
                        Child::Submodule(sub) => {
                            return Err(format!(
                                "timezone hierarchy `{full_name}/{sub}` exceeds depth 3"
                            ));
                        }
                        Child::TimeZone(name) => name,
                    };

                    let converted_name = convert_bad_chars(child_name);
                    writeln!(
                        out,
                        "        pub const {converted_name}: Tz = Tz::{module_name}__{submodule_name}__{converted_name};"
                    )
                    .unwrap();
                }
            }
            writeln!(out, "    }}\n").unwrap();
        }
        writeln!(out, "}}").unwrap();
    }

    Ok(out)
}

/// Checks the `CHRONO_TZ_TIMEZONE_FILTER` environment variable.
/// Converts it to a regex if set.
#[cfg(feature = "filter-by-regex")]
fn try_get_filter_regex() -> Result<Option<regex::Regex>, String> {
    match std::env::var(FILTER_ENV_VAR_NAME) {
        Ok(val) => {
            let val = val.trim();
            if val.is_empty() {
                return Ok(None);
            }
            regex::Regex::new(val).map(Some).map_err(|err| {
                format!(
                    "The value '{val:?}' for environment variable {FILTER_ENV_VAR_NAME} is not a valid regex, err={err}"
                )
            })
        }
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(s)) => Err(format!(
            "The value '{s:?}' for environment variable {FILTER_ENV_VAR_NAME} is not valid Unicode"
        )),
    }
}

fn detect_iana_db_version(root: &Path) -> Result<String, String> {
    let path = root.join(Path::new("tz/NEWS"));
    let file = File::open(&path).map_err(|e| format!("failed to open {}: {e}", path.display()))?;

    let lines = BufReader::new(file).lines();
    for line in lines {
        let line = line.map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let Some(rest) = line.strip_prefix("Release ") else {
            continue;
        };
        if let Some((version, _)) = rest.split_once(" - ") {
            return Ok(version.to_owned());
        }
    }

    Err(format!("no IANA tzdb version found in {}", path.display()))
}

fn generate_files_in_memory(
    table: &Table,
    zones: &BTreeSet<&str>,
    uncased: bool,
    version: &str,
) -> Result<(String, String), String> {
    validate_zones(table, zones, uncased)?;
    let timezones_rs = render_timezone_file(table, zones, uncased)?;
    let directory_rs = render_directory_file(zones, version)?;
    Ok((timezones_rs, directory_rs))
}

fn write_atomic_checked(dir: &Path, timezones_rs: &str, directory_rs: &str) -> io::Result<()> {
    let timezone_path = dir.join("timezones.rs");
    let directory_path = dir.join("directory.rs");

    let mut timezone_file = File::create(timezone_path)?;
    timezone_file.write_all(timezones_rs.as_bytes())?;
    timezone_file.flush()?;

    let mut directory_file = File::create(directory_path)?;
    directory_file.write_all(directory_rs.as_bytes())?;
    directory_file.flush()?;
    Ok(())
}

fn try_generate_and_write(
    dir: &Path,
    table: &Table,
    zones: &BTreeSet<&str>,
    uncased: bool,
    version: &str,
) -> Result<(), String> {
    let (timezones_rs, directory_rs) = generate_files_in_memory(table, zones, uncased, version)?;
    write_atomic_checked(dir, &timezones_rs, &directory_rs)
        .map_err(|e| format!("failed to write generated files to {}: {e}", dir.display()))
}

fn try_main_from_root(root: &Path, dir: &Path, filter: bool, uncased: bool) -> Result<(), String> {
    let mut table = TableBuilder::new();

    for fname in FILES {
        let path = root.join(format!("tz/{fname}"));
        let file = File::open(&path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        for (line_no, line) in BufReader::new(file).lines().enumerate() {
            let line =
                line.map_err(|e| format!("error reading {}:{}: {e}", path.display(), line_no + 1))?;
            let line = strip_comments(line);
            let parsed = Line::new(&line)
                .map_err(|e| format!("parse error in {}:{}: {e}", path.display(), line_no + 1))?;
            table
                .add_line(parsed)
                .map_err(|e| format!("table error in {}:{}: {e}", path.display(), line_no + 1))?;
        }
    }

    let table = table.build();

    #[cfg(feature = "filter-by-regex")]
    let regex = if filter {
        try_get_filter_regex()?
    } else {
        None
    };
    #[cfg(not(feature = "filter-by-regex"))]
    let _ = filter;

    #[cfg(feature = "filter-by-regex")]
    let matches_filter = |tz: &str| regex.as_ref().is_none_or(|r| r.is_match(tz));
    #[cfg(not(feature = "filter-by-regex"))]
    let matches_filter = |_: &str| true;

    let zones = table
        .zonesets
        .keys()
        .chain(table.links.keys())
        .filter(|s| matches_filter(s))
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    if zones.is_empty() {
        #[cfg(feature = "filter-by-regex")]
        if let Some(ref re) = regex {
            return Err(format!(
                "filter regex `{}` for {FILTER_ENV_VAR_NAME} matched zero timezones",
                re.as_str()
            ));
        }
        return Err("timezone selection is empty; zero timezones to generate".to_owned());
    }

    let version = detect_iana_db_version(root)?;
    try_generate_and_write(dir, &table, &zones, uncased, &version)
}

pub fn try_main(dir: &Path, filter: bool, uncased: bool) -> Result<(), String> {
    let root = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").map_err(|_| "no Cargo build context".to_owned())?,
    );
    try_main_from_root(&root, dir, filter, uncased)
}

pub fn main(dir: &Path, filter: bool, uncased: bool) {
    if let Err(err) = try_main(dir, filter, uncased) {
        panic!("sad-chrono-tz-build failed: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parse_zoneinfo::line::Line;
    use parse_zoneinfo::table::{Format, Saving, ZoneInfo};

    fn parse_table(input: &str) -> Table {
        let mut builder = TableBuilder::new();
        for line in input.lines() {
            let line = strip_comments(line.to_owned());
            builder.add_line(Line::new(&line).unwrap()).unwrap();
        }
        builder.build()
    }

    #[test]
    fn convert_bad_chars_handles_mixed_hyphens_and_digits() {
        assert_eq!(convert_bad_chars("Etc/GMT-1"), "Etc__GMTMinus1");
        assert_eq!(convert_bad_chars("Etc/GMT+12"), "Etc__GMTPlus12");
        assert_eq!(
            convert_bad_chars("America/Port-au-Prince"),
            "America__PortauPrince"
        );
        assert_eq!(
            convert_bad_chars("America/Port-au-Prince-1"),
            "America__PortauPrinceMinus1"
        );
    }

    #[test]
    fn resolve_link_transitive() {
        let table =
            parse_table("Zone Real/Zone 0 - RZ\nLink Real/Zone Link/One\nLink Link/One Link/Two\n");

        assert_eq!(try_resolve_link(&table, "Link/Two").unwrap(), "Real/Zone");
        assert_eq!(try_resolve_link(&table, "Real/Zone").unwrap(), "Real/Zone");
    }

    #[test]
    fn resolve_link_cycle_and_broken_rejected() {
        let table =
            parse_table("Link Cycle/B Cycle/A\nLink Cycle/A Cycle/B\nLink Missing/Z Broken/A\n");
        let err_cycle = try_resolve_link(&table, "Cycle/A").unwrap_err();
        assert!(err_cycle.contains("cyclic link"), "{err_cycle}");

        let err_broken = try_resolve_link(&table, "Broken/A").unwrap_err();
        assert!(err_broken.contains("broken link"), "{err_broken}");
    }

    #[test]
    fn validate_timespan_accepts_valid_bounds() {
        try_validate_timespan(
            "Valid/Zone",
            &CodegenTimespan {
                utc_offset: 86_399,
                dst_offset: 0,
                name: Some("V".to_owned()),
            },
        )
        .unwrap();
        try_validate_timespan(
            "Valid/Neg",
            &CodegenTimespan {
                utc_offset: -86_399,
                dst_offset: 0,
                name: Some("N".to_owned()),
            },
        )
        .unwrap();
    }

    #[test]
    fn validate_timespan_rejects_24h_and_i32_overflow() {
        let err_24h = try_validate_timespan(
            "Bad/Zone",
            &CodegenTimespan {
                utc_offset: 82_800,
                dst_offset: 3_600,
                name: Some("B".to_owned()),
            },
        )
        .unwrap_err();
        assert!(err_24h.contains("FixedOffset bounds"), "{err_24h}");

        let err_i32 = try_validate_timespan(
            "Huge/Zone",
            &CodegenTimespan {
                utc_offset: i64::MAX,
                dst_offset: 0,
                name: Some("H".to_owned()),
            },
        )
        .unwrap_err();
        assert!(err_i32.contains("out of i32 range"), "{err_i32}");

        let err_utc_packed = try_validate_timespan(
            "UtcPacked/Zone",
            &CodegenTimespan {
                utc_offset: 524_288,
                dst_offset: -450_000,
                name: Some("U".to_owned()),
            },
        )
        .unwrap_err();
        assert!(
            err_utc_packed.contains("20-bit packed range"),
            "{err_utc_packed}"
        );

        let err_dst_packed = try_validate_timespan(
            "DstPacked/Zone",
            &CodegenTimespan {
                utc_offset: 450_000,
                dst_offset: -524_289,
                name: Some("D".to_owned()),
            },
        )
        .unwrap_err();
        assert!(
            err_dst_packed.contains("20-bit packed range"),
            "{err_dst_packed}"
        );

        let err_long_abbr = try_validate_timespan(
            "LongAbbr/Zone",
            &CodegenTimespan {
                utc_offset: 0,
                dst_offset: 0,
                name: Some("A".repeat(65)),
            },
        )
        .unwrap_err();
        assert!(
            err_long_abbr.contains("exceeds 64 bytes"),
            "{err_long_abbr}"
        );
    }

    #[test]
    fn pack_timespan_roundtrips_signed_offsets() {
        let abbrs = "-0330EST";
        for (utc, dst, name, expected_start, expected_len) in [
            (-57_368i64, -3_600i64, Some("-0330"), 0u16, 5u8),
            (54_822, 7_200, Some("EST"), 5, 3),
            (-18_000, 0, None, 0, u8::MAX),
            (-18_000, 3_600, Some("ST"), 6, 2),
            (0, 10_800, Some("EST"), 5, 3),
            (-524_288, 524_287, Some("EST"), 5, 3),
        ] {
            let ts = CodegenTimespan {
                utc_offset: utc,
                dst_offset: dst,
                name: name.map(str::to_owned),
            };
            let (offset, start, len) = pack_timespan("Test/Zone", &ts, abbrs).unwrap();
            let packed = (offset[0] as u64)
                | ((offset[1] as u64) << 8)
                | ((offset[2] as u64) << 16)
                | ((offset[3] as u64) << 24)
                | ((offset[4] as u64) << 32);
            assert_eq!((((packed as i64) << 24) >> 44), utc);
            assert_eq!((((packed as i64) << 44) >> 44), dst);
            assert_eq!(start, expected_start);
            assert_eq!(len, expected_len);
        }
    }

    #[test]
    fn custom_zone_with_3h_dst_succeeds_and_decodes() {
        let t = parse_table("Zone Custom/ThreeHourDst 0 3:00 CDST\n");
        let z: BTreeSet<&str> = ["Custom/ThreeHourDst"].into_iter().collect();
        let (tz_rs, _) = generate_files_in_memory(&t, &z, false, "2026f").unwrap();

        let (expected_offset, expected_start, expected_len) = pack_timespan(
            "Custom/ThreeHourDst",
            &CodegenTimespan {
                utc_offset: 0,
                dst_offset: 10_800,
                name: Some("CDST".to_owned()),
            },
            "CDST",
        )
        .unwrap();
        let packed = (expected_offset[0] as u64)
            | ((expected_offset[1] as u64) << 8)
            | ((expected_offset[2] as u64) << 16)
            | ((expected_offset[3] as u64) << 24)
            | ((expected_offset[4] as u64) << 32);
        assert_eq!((((packed as i64) << 24) >> 44), 0);
        assert_eq!((((packed as i64) << 44) >> 44), 10_800);
        assert!(tz_rs.contains(&format!(
            "FixedTimespan {{ offset: {expected_offset:?}, abbr_len: {expected_len}, abbr_start: {expected_start} }}"
        )));
    }

    #[test]
    fn rejects_unrepresentable_finite_transition_timestamp_until_300000() {
        let t = parse_table("Zone Custom/FarFuture 0 - LMT 300000\n 1:00 - FF\n");
        let z: BTreeSet<&str> = ["Custom/FarFuture"].into_iter().collect();
        let err = generate_files_in_memory(&t, &z, false, "2026f").unwrap_err();
        assert!(err.contains("DateTime<Utc> range"), "{err}");
    }

    #[test]
    fn rejects_invalid_rust_identifiers_and_keywords() {
        let t_kw = parse_table("Zone fn 0 - FN\n");
        let z_kw: BTreeSet<&str> = ["fn"].into_iter().collect();
        let err = generate_files_in_memory(&t_kw, &z_kw, false, "2026f").unwrap_err();
        assert!(err.contains("invalid Rust identifier"), "{err}");

        let t_num = parse_table("Zone 123Bad 0 - BAD\n");
        let z_num: BTreeSet<&str> = ["123Bad"].into_iter().collect();
        let err = generate_files_in_memory(&t_num, &z_num, false, "2026f").unwrap_err();
        assert!(err.contains("invalid Rust identifier"), "{err}");

        let t_under = parse_table("Zone _ 0 - U\n");
        let z_under: BTreeSet<&str> = ["_"].into_iter().collect();
        let err = generate_files_in_memory(&t_under, &z_under, false, "2026f").unwrap_err();
        assert!(err.contains("invalid Rust identifier"), "{err}");
    }

    #[test]
    fn rejects_normalization_and_case_collisions() {
        // '-' without digit is stripped, so A-B and AB both become AB.
        let t_norm = parse_table("Zone A-B 0 - A\nZone AB 0 - B\n");
        let z_norm: BTreeSet<&str> = ["A-B", "AB"].into_iter().collect();
        let err = generate_files_in_memory(&t_norm, &z_norm, false, "2026f").unwrap_err();
        assert!(err.contains("normalization collision"), "{err}");

        // Uppercase const collision: Foo/bar and Foo/BAR have distinct variants Foo__bar / Foo__BAR
        // but collide on const FOO__BAR.
        let t_upper = parse_table("Zone Foo/bar 0 - A\nZone Foo/BAR 0 - B\n");
        let z_upper: BTreeSet<&str> = ["Foo/bar", "Foo/BAR"].into_iter().collect();
        let err = generate_files_in_memory(&t_upper, &z_upper, false, "2026f").unwrap_err();
        assert!(err.contains("normalization collision"), "{err}");

        // Case-insensitive collision when uncased = true.
        let t_case = parse_table("Zone Foo/bar 0 - A\nZone Foo/Bar 0 - B\n");
        let z_case: BTreeSet<&str> = ["Foo/bar", "Foo/Bar"].into_iter().collect();
        let err = generate_files_in_memory(&t_case, &z_case, true, "2026f").unwrap_err();
        assert!(err.contains("case-insensitive collision"), "{err}");
    }

    #[test]
    fn rejects_zone_link_collision_and_depth_gt_3() {
        let mut t_coll = Table::default();
        t_coll.zonesets.insert(
            "Dup/Zone".to_owned(),
            vec![ZoneInfo {
                offset: 0,
                saving: Saving::NoSaving,
                format: Format::new("DZ"),
                end_time: None,
            }],
        );
        t_coll
            .links
            .insert("Dup/Zone".to_owned(), "Dup/Zone".to_owned());
        let z_coll: BTreeSet<&str> = ["Dup/Zone"].into_iter().collect();
        let err = generate_files_in_memory(&t_coll, &z_coll, false, "2026f").unwrap_err();
        assert!(err.contains("zone/link collision"), "{err}");

        let t_deep = parse_table("Zone A/B/C/D 0 - DEEP\n");
        let z_deep: BTreeSet<&str> = ["A/B/C/D"].into_iter().collect();
        let err = generate_files_in_memory(&t_deep, &z_deep, false, "2026f").unwrap_err();
        assert!(err.contains("depth > 3"), "{err}");

        // A name cannot be both a zone and a module in directory.rs.
        let t_mod_zone = parse_table("Zone Foo 0 - F\nZone Foo/Bar 0 - FB\n");
        let z_mod_zone: BTreeSet<&str> = ["Foo", "Foo/Bar"].into_iter().collect();
        let err = generate_files_in_memory(&t_mod_zone, &z_mod_zone, false, "2026f").unwrap_err();
        assert!(err.contains("directory namespace collision"), "{err}");
    }

    #[test]
    fn rejects_empty_selection_and_supports_alias_only_and_percent_z() {
        let t = parse_table(
            "Zone Etc/GMT+1 -1:00 - %z\nZone America/St_Johns -3:30 - -0330\nZone America/Argentina/Buenos_Aires -3:00 - -03\nLink Etc/GMT+1 Alias/Only\n",
        );

        let empty: BTreeSet<&str> = BTreeSet::new();
        let err = generate_files_in_memory(&t, &empty, false, "2026f").unwrap_err();
        assert!(err.contains("empty"), "{err}");

        // Alias-only selection (without UTC -> no impl Default for Tz)
        let alias_only: BTreeSet<&str> = ["Alias/Only"].into_iter().collect();
        let (tz_rs, dir_rs) = generate_files_in_memory(&t, &alias_only, false, "2026f").unwrap();
        assert!(tz_rs.contains("MAX_TZ_NAME_LEN: usize = 10;"));
        assert!(tz_rs.contains("if s.len() > MAX_TZ_NAME_LEN"));
        assert!(tz_rs.contains("abbr_len: 255"));
        assert!(!tz_rs.contains("impl Default for Tz"));
        assert!(dir_rs.contains("pub mod Alias"));

        // Explicit numeric abbreviation (-0330) keeps Some("-0330"), while %z keeps None (abbr_len: 255)
        let both: BTreeSet<&str> = [
            "Etc/GMT+1",
            "America/St_Johns",
            "America/Argentina/Buenos_Aires",
        ]
        .into_iter()
        .collect();
        let (tz_rs2, dir_rs2) = generate_files_in_memory(&t, &both, false, "2026f").unwrap();
        assert!(tz_rs2.contains("pub(crate) const ABBREVIATIONS: &str = \"-0330\";"));
        assert!(tz_rs2.contains("abbr_len: 255"));
        assert!(tz_rs2.contains("abbr_len: 5, abbr_start: 0"));
        assert!(dir_rs2.contains("pub mod Argentina"));
        assert!(dir_rs2.contains("pub const Buenos_Aires: Tz"));
    }

    struct TempDirGuard(PathBuf);

    impl TempDirGuard {
        fn new(prefix: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let base = env::temp_dir();
            let pid = std::process::id();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            loop {
                let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
                let candidate = base.join(format!("{prefix}-{pid}-{nanos}-{seq}"));
                match std::fs::create_dir(&candidate) {
                    Ok(()) => return Self(candidate),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("failed to create temp dir {}: {e}", candidate.display()),
                }
            }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rejects_reserved_generator_identifiers_and_writes_no_files() {
        for bad_zone in [
            "IANA_TZDB_VERSION",
            "IANA_TZDB_VERSION-",
            "iana_tzdb_version",
            "Tz",
            "directory",
            "timezones",
            "ParseError",
            "TZ_VARIANTS",
            "Europe/Tz",
            "Europe/IANA_TZDB_VERSION",
        ] {
            let t = parse_table(&format!("Zone {bad_zone} 0 - TEST\n"));
            let z: BTreeSet<&str> = [bad_zone].into_iter().collect();
            let err = generate_files_in_memory(&t, &z, false, "2026f").unwrap_err();
            assert!(err.contains("reserved"), "{bad_zone}: {err}");
        }

        // Also exercise the full tz-reading + codegen + write entry point on real temp directories.
        let tz_root = TempDirGuard::new("sad-chrono-tz-build-reserved-tz");
        let out_dir = TempDirGuard::new("sad-chrono-tz-build-reserved-out");
        let tz_dir = tz_root.0.join("tz");
        std::fs::create_dir(&tz_dir).unwrap();
        std::fs::write(tz_dir.join("NEWS"), "Release 2026f - 2026-01-01\n").unwrap();
        for fname in FILES {
            let contents = if *fname == "africa" {
                "Zone IANA_TZDB_VERSION 0 - TEST\n"
            } else {
                ""
            };
            std::fs::write(tz_dir.join(fname), contents).unwrap();
        }
        let err = try_main_from_root(&tz_root.0, &out_dir.0, false, false).unwrap_err();
        assert!(err.contains("reserved"), "{err}");
        assert!(std::fs::read_dir(&out_dir.0).unwrap().next().is_none());
    }

    #[test]
    fn atomic_in_memory_validation_writes_no_files_on_failure() {
        let dir = TempDirGuard::new("sad-chrono-tz-build-atomic-test");

        let table = parse_table("Link Missing/Target Broken/Link\n");
        let zones: BTreeSet<&str> = ["Broken/Link"].into_iter().collect();
        assert!(try_generate_and_write(&dir.0, &table, &zones, false, "2026f").is_err());
        assert!(std::fs::read_dir(&dir.0).unwrap().next().is_none());
    }

    #[test]
    fn pack_timespan_rejects_missing_and_oversized_abbreviations() {
        let missing = pack_timespan(
            "Test/Missing",
            &CodegenTimespan {
                utc_offset: 0,
                dst_offset: 0,
                name: Some("MISS".to_owned()),
            },
            "EST",
        )
        .unwrap_err();
        assert!(missing.contains("missing from interned table"), "{missing}");

        let mut huge_table = "x".repeat(usize::from(u16::MAX) + 1);
        huge_table.push_str("EST");
        let oob = pack_timespan(
            "Test/Oob",
            &CodegenTimespan {
                utc_offset: 0,
                dst_offset: 0,
                name: Some("EST".to_owned()),
            },
            &huge_table,
        )
        .unwrap_err();
        assert!(oob.contains("exceeds u16::MAX"), "{oob}");
    }

    #[test]
    fn render_timezone_file_rejects_table_overflows_and_empty_zoneset() {
        // Empty zoneset fails try_codegen_timespans inside render_timezone_file.
        let mut empty_table = Table::default();
        empty_table
            .zonesets
            .insert("Empty/Zone".to_owned(), Vec::new());
        let empty_zones: BTreeSet<&str> = ["Empty/Zone"].into_iter().collect();
        let err_empty = render_timezone_file(&empty_table, &empty_zones, false).unwrap_err();
        assert!(
            err_empty.contains("failed to build timespans for `Empty/Zone`"),
            "{err_empty}"
        );

        // > u16::MAX zones rejected.
        let many_names: Vec<String> = (0..=usize::from(u16::MAX))
            .map(|i| format!("Zone{i}"))
            .collect();
        let many_zones: BTreeSet<&str> = many_names.iter().map(String::as_str).collect();
        let err_count = render_timezone_file(&Table::default(), &many_zones, false).unwrap_err();
        assert!(err_count.contains("zone count"), "{err_count}");
        assert!(err_count.contains("exceeds u16::MAX"), "{err_count}");

        // > u16::MAX bytes of unique interned abbreviations rejected.
        let mut abbr_table = Table::default();
        let mut abbr_zone_names = Vec::with_capacity(1200);
        let padding = "x".repeat(55);
        for i in 0..1200 {
            let zname = format!("Abbr/Z{i:04}");
            let abbr = format!("A{i:04}{padding}");
            abbr_table.zonesets.insert(
                zname.clone(),
                vec![ZoneInfo {
                    offset: 0,
                    saving: Saving::NoSaving,
                    format: Format::new(&abbr),
                    end_time: None,
                }],
            );
            abbr_zone_names.push(zname);
        }
        let abbr_zones: BTreeSet<&str> = abbr_zone_names.iter().map(String::as_str).collect();
        let err_abbrs = render_timezone_file(&abbr_table, &abbr_zones, false).unwrap_err();
        assert!(
            err_abbrs.contains("interned abbreviation table length")
                && err_abbrs.contains("exceeds u16::MAX"),
            "{err_abbrs}"
        );
    }
}
