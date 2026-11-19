//! The settings file the person named on the command line, held as a protected path. Only paths
//! and file identities are read, never a file's bytes.

use std::path::{Component, Path, PathBuf};

/// The names a named settings file is held under: the spelling typed made absolute with `.` and
/// `..` taken out, then the path it lands on once links are followed where that is a different
/// path. The file need not exist: the part that does is resolved and the rest is kept as written.
pub(super) fn named_settings_names(named: &Path) -> Vec<PathBuf> {
    let absolute = if named.is_absolute() {
        named.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(named))
            .unwrap_or_else(|_| named.to_path_buf())
    };
    let absolute = lexically_clean(&absolute);
    let landed = landed_path(&absolute);
    let mut names = vec![absolute];
    if !names.contains(&landed) {
        names.push(landed);
    }
    names
}

/// `path` with `.` removed and each `..` taken out with the component before it.
pub(super) fn lexically_clean(path: &Path) -> PathBuf {
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                cleaned.pop();
            }
            other => cleaned.push(other.as_os_str()),
        }
    }
    cleaned
}

/// The path `path` lands on: its longest existing prefix with every link followed, then the rest
/// as written. A name whose prefix cannot be resolved is returned as it was.
pub(super) fn landed_path(path: &Path) -> PathBuf {
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut head = path.to_path_buf();
    loop {
        if let Ok(real) = head.canonicalize() {
            let mut landed = real;
            for part in rest.iter().rev() {
                landed.push(part);
            }
            return landed;
        }
        match (head.file_name().map(|name| name.to_owned()), head.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                head = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Whether `spelled` and `named` are one path: equal, or equal once case is ignored where the volume
/// holds names differing only in case as one file. Only the text is read, so a file that is not
/// there yet is answered as one that is.
pub(super) fn names_one_path(spelled: &Path, named: &Path, folds_case: bool) -> bool {
    spelled == named
        || (folds_case
            && spelled.to_string_lossy().to_lowercase() == named.to_string_lossy().to_lowercase())
}

/// Whether `first` and `second` are one file on disk, which is what a hard link is. False where
/// either is not there or the platform gives no identity.
#[cfg(unix)]
pub(super) fn same_file(first: &Path, second: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(first), std::fs::metadata(second)) {
        (Ok(first), Ok(second)) => {
            first.is_file() && first.dev() == second.dev() && first.ino() == second.ino()
        }
        _ => false,
    }
}

#[cfg(not(unix))]
pub(super) fn same_file(_first: &Path, _second: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::super::Workspace;
    use bravebot_core::CaseFolding;
    use std::path::Path;

    /// A write to a named file that is not made yet reaches it by another case where the volume folds
    /// case or could not be asked, and not where it holds the spellings apart. Rejects a comparison
    /// that reads the spelling alone, one that folds on a volume that holds two spellings as two
    /// files, and one that reads a volume that could not be asked as one that does not fold.
    #[test]
    fn a_spelling_in_another_case_reaches_a_file_not_made_yet_unless_the_volume_keeps_them_apart() {
        let root = std::env::temp_dir().join(format!("bravebot-named-fold-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("scratch");
        let root = root.canonicalize().expect("canonical scratch");
        let workspace = Workspace::new(&root)
            .expect("workspace")
            .with_named_settings_file(Some(&root.join("Grant.json")));
        let other_case = root.join("GRANT.json");
        let folds = |_: &Path| CaseFolding::Folds;
        let does_not = |_: &Path| CaseFolding::Keeps;
        let unasked = |_: &Path| CaseFolding::Unknown;
        assert!(
            workspace.reaches_the_named_settings_file_where(&root.join("Grant.json"), &does_not)
        );
        assert!(workspace.reaches_the_named_settings_file_where(&other_case, &folds));
        assert!(!workspace.reaches_the_named_settings_file_where(&other_case, &does_not));
        assert!(workspace.reaches_the_named_settings_file_where(&other_case, &unasked));
        assert!(
            !workspace.reaches_the_named_settings_file_where(&root.join("Other.json"), &unasked)
        );
        assert!(!workspace.reaches_the_named_settings_file_where(&root.join("Other.json"), &folds));
        let _ = std::fs::remove_dir_all(&root);
    }
}
