//! Fixtures the unit tests of more than one module share.

use crate::finish::{Finished, report};
use bravebot_config::Managed;
use std::path::{Path, PathBuf};

/// Reads back what a run would have written, as the two streams it writes to.
pub(crate) fn written(run: &Finished<'_>) -> (String, String) {
    let mut reply = Vec::new();
    let mut beside = Vec::new();
    report(&mut reply, &mut beside, run);
    (
        String::from_utf8(reply).expect("utf-8"),
        String::from_utf8(beside).expect("utf-8"),
    )
}

/// A scratch directory that removes itself, so tests do not leave state behind.
///
/// Under this crate's own build directory rather than the system temporary one, which is
/// shared between users and where a name this predictable is somebody else's to create first.
pub(crate) struct Scratch {
    pub(crate) path: PathBuf,
}

impl Scratch {
    pub(crate) fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self {
            path: path.canonicalize().expect("canonical scratch"),
        }
    }

    /// A directory inside the scratch, made ready to be used.
    pub(crate) fn directory(&self, name: &str) -> PathBuf {
        let path = self.path.join(name);
        std::fs::create_dir_all(&path).expect("create directory");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The managed file a test writes, for the layer an administrator owns.
pub(crate) fn pinned(scratch: &Scratch, text: &str) -> (Managed, PathBuf) {
    let path = scratch.directory("managed").join("managed.json");
    std::fs::write(&path, text).expect("a managed file");
    (Managed::at(&path), path)
}

pub(crate) fn scratch(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    // The pid and the stamp make the name this process's own, and `create_dir` refuses a name
    // already taken, so another slot's copy of this test is never reused.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let dir = std::env::temp_dir().join(format!(
        "bravebot-cli-sandbox-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir(&dir).expect("scratch");
    dir
}

pub(crate) fn pin_file(name: &str, text: &str) -> Managed {
    let dir = scratch(name);
    let file = dir.join("managed.json");
    std::fs::write(&file, text).expect("managed file");
    Managed::at(&file)
}
