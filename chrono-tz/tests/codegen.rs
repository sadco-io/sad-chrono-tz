use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{env, fs};

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let base = env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        loop {
            let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
            let candidate = base.join(format!("{prefix}-{pid}-{nanos}-{seq}"));
            match fs::create_dir(&candidate) {
                Ok(()) => return Self(candidate),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("failed to create temp dir {}: {e}", candidate.display()),
            }
        }
    }
}

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
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/prebuilt");
    let prebuilt_directory = normalize_lf(&fs::read_to_string(root.join("directory.rs")).unwrap());
    let prebuilt_timezones = normalize_lf(&fs::read_to_string(root.join("timezones.rs")).unwrap());

    let out_dir = TempDirGuard::new("sad-chrono-tz-codegen-test");

    chrono_tz_build::main(&out_dir.0, false, false);
    let generated_directory =
        normalize_lf(&fs::read_to_string(out_dir.0.join("directory.rs")).unwrap());
    let generated_timezones =
        normalize_lf(&fs::read_to_string(out_dir.0.join("timezones.rs")).unwrap());

    assert_eq!(
        prebuilt_directory, generated_directory,
        "src/prebuilt/directory.rs is out of date with codegen"
    );
    assert_eq!(
        prebuilt_timezones, generated_timezones,
        "src/prebuilt/timezones.rs is out of date with codegen"
    );
}
