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
        if let Ok(parsed) = Line::new(line) {
            let _ = builder.add_line(parsed);
        }
    }

    let table = builder.build();
    let _ = table.structure().into_iter().count();
    for name in table.zonesets.keys().chain(table.links.keys()) {
        let _ = table.get_zoneset(name);
        let _ = table.timespans(name);
    }
});
