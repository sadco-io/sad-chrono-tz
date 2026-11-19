# Fuzzing sad-chrono-tz

To fuzz `sad-chrono-tz` and `sad-parse-zoneinfo` we rely on the
[cargo-fuzz](https://rust-fuzz.github.io/) project.

To install `cargo-fuzz`:
```sh
cargo install cargo-fuzz
```

Available fuzz targets:
- `fuzz_parse_line`: `sad_parse_zoneinfo::line::Line::new` and line/rule/zone conversion
- `fuzz_parse_table`: `TableBuilder`, `Table::get_zoneset`, `Table::timespans`, and `Table::structure`
- `fuzz_tz_from_str`: `Tz::from_str` and `Tz::from_str_insensitive`
- `fuzz_tz_serde`: `serde_json` deserialization and bounded `SerdeError` formatting for `Tz`

To run a fuzzer, navigate to the repository root and run `cargo fuzz run <target>`, for example:
```sh
cargo fuzz run fuzz_parse_table
```
