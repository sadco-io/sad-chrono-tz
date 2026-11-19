#![no_main]
use libfuzzer_sys::fuzz_target;
use sad_chrono_tz::Tz;

fuzz_target!(|data: &[u8]| {
    match serde_json::from_slice::<Tz>(data) {
        Ok(tz) => {
            let encoded = serde_json::to_string(&tz).unwrap();
            let decoded: Tz = serde_json::from_str(&encoded).unwrap();
            assert_eq!(tz, decoded);
        }
        Err(err) => {
            let _ = err.to_string();
        }
    }
});
