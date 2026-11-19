#![no_main]
use libfuzzer_sys::fuzz_target;
use sad_parse_zoneinfo::line::Line;
use sad_parse_zoneinfo::structure::Structure;
use sad_parse_zoneinfo::table::TableBuilder;
use sad_parse_zoneinfo::transitions::TableTransitions;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };

    let mut builder = TableBuilder::new();
    for line in text.lines() {
        match Line::new(line) {
            Ok(parsed) => {
                if let Err(err) = builder.add_line(parsed) {
                    let _ = err.to_string();
                }
            }
            Err(err) => {
                let _ = err.to_string();
            }
        }
    }

    let table = builder.build();
    let _ = table.structure().into_iter().count();
    for name in table.zonesets.keys().chain(table.links.keys()) {
        let _ = table.get_zoneset(name);
        if let Err(err) = table.try_timespans(name) {
            let _ = err.to_string();
        }
        if let Err(err) = table.try_codegen_timespans(name) {
            let _ = err.to_string();
        }
        let _ = table.timespans(name);
    }
});
