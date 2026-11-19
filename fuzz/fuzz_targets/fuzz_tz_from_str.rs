#![no_main]
use core::str::FromStr;
use libfuzzer_sys::fuzz_target;
use sad_chrono_tz::Tz;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = core::str::from_utf8(data) else {
        return;
    };

    if let Ok(tz) = Tz::from_str(s) {
        assert_eq!(Tz::from_str(tz.name()), Ok(tz));
    }
    if let Ok(tz) = Tz::from_str_insensitive(s) {
        assert_eq!(Tz::from_str_insensitive(tz.name()), Ok(tz));
    }
});
