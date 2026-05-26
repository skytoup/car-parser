use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct FixtureGroup {
    pub name: &'static str,
    pub root: PathBuf,
    pub requires_full: bool,
}

impl FixtureGroup {
    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub fn is_enabled(&self) -> bool {
        !self.requires_full || require_full_fixture()
    }
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../car-tests/data")
}

pub fn fixture_path(name: &str) -> PathBuf {
    fixture_root().join(name)
}

pub fn fixture_group(name: &str) -> FixtureGroup {
    match name {
        "full" => FixtureGroup {
            name: "full",
            root: fixture_root(),
            requires_full: true,
        },
        _ => FixtureGroup {
            name: "smoke",
            root: fixture_root(),
            requires_full: false,
        },
    }
}

pub fn temp_output_dir(test_name: &str) -> PathBuf {
    let pid = std::process::id();
    let seq = TEMP_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("carparser-{test_name}-{pid}-{seq}"))
}

pub fn require_full_fixture() -> bool {
    matches!(
        std::env::var("CAR_TEST_FULL").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}
