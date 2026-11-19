use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{env, fs};

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn normalize_lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[test]
fn codegen() {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/prebuilt");
    let prebuilt_directory = normalize_lf(&fs::read_to_string(root.join("directory.rs")).unwrap());
    let prebuilt_timezones = normalize_lf(&fs::read_to_string(root.join("timezones.rs")).unwrap());

    let out_dir = env::temp_dir().join(format!(
        "sad-chrono-tz-codegen-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _guard = TempDirGuard(out_dir.clone());
    fs::create_dir_all(&out_dir).unwrap();

    chrono_tz_build::main(&out_dir, false, false);
    let generated_directory =
        normalize_lf(&fs::read_to_string(out_dir.join("directory.rs")).unwrap());
    let generated_timezones =
        normalize_lf(&fs::read_to_string(out_dir.join("timezones.rs")).unwrap());

    assert_eq!(
        prebuilt_directory, generated_directory,
        "src/prebuilt/directory.rs is out of date with codegen"
    );
    assert_eq!(
        prebuilt_timezones, generated_timezones,
        "src/prebuilt/timezones.rs is out of date with codegen"
    );
}
