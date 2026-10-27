//! Fact files on disk, under the state directory (RECALL-4).

use super::fact::{Fact, Scope, is_a_key};
use super::project::ProjectId;
use crate::home;
use crate::preamble::{AGENTS_FILE, RULES_FILE};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Open `path` for reading only where it is a regular file.
///
/// Opened without waiting, then asked what it is, so a pipe at the path, or one put there between
/// a look and the open, is never waited on (RECALL-2, RECALL-11).
pub(crate) fn open_regular(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use rustix::fs::OFlags;
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(OFlags::NONBLOCK.bits().cast_signed());
    }
    let file = options.open(path)?;
    match file.metadata()?.is_file() {
        true => Ok(file),
        false => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        )),
    }
}

/// The facts for one project and for every project, kept under one state directory.
pub struct Store {
    home: PathBuf,
    project: ProjectId,
}

/// What a read found: the facts, and a line for each file it had to leave out.
#[derive(Debug, Default)]
pub struct Loaded {
    pub facts: Vec<(Scope, Fact)>,
    pub notices: Vec<String>,
}

impl Store {
    /// A store over `home`, the state directory that holds `projects/` and `facts/`.
    pub fn new(home: &Path, project: ProjectId) -> Store {
        Store {
            home: home.to_path_buf(),
            project,
        }
    }

    fn project_directory(&self) -> PathBuf {
        self.home.join("projects").join(&self.project.dir)
    }

    fn facts_directory(&self, scope: Scope) -> PathBuf {
        match scope {
            Scope::Project => self.project_directory().join("facts"),
            Scope::Everywhere => self.home.join("facts"),
        }
    }

    /// Where the fact with `key` lives in `scope`.
    pub fn fact_path(&self, scope: Scope, key: &str) -> PathBuf {
        self.facts_directory(scope).join(format!("{key}.md"))
    }

    /// The file beside a project's facts that says which project the directory is for.
    pub fn id_path(&self) -> PathBuf {
        self.project_directory().join("id")
    }

    /// The file a rule for `scope` is appended to: `AGENTS.md` for every project, and the project's
    /// `rules.md` for this one (RECALL-2).
    pub fn rules_path(&self, scope: Scope) -> PathBuf {
        match scope {
            Scope::Everywhere => self.home.join(AGENTS_FILE),
            Scope::Project => self.project_directory().join(RULES_FILE),
        }
    }

    /// Every fact in both scopes, in key order. Makes no directory and fails on none: a file that
    /// cannot be read as a fact is named in `notices` and left out.
    pub fn load(&self) -> Loaded {
        let mut loaded = Loaded::default();
        for scope in [Scope::Everywhere, Scope::Project] {
            let root = self.facts_directory(scope);
            collect(&root, &root, scope, &mut loaded);
        }
        loaded
    }

    /// Write `fact` to its file, replacing one with the same key whole. Refuses while the session
    /// may not write under the state directory.
    pub fn write(&self, scope: Scope, fact: &Fact) -> io::Result<PathBuf> {
        if home::writable().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "nothing is written under the state directory",
            ));
        }
        if !is_a_key(&fact.key) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{:?} is not a key", fact.key),
            ));
        }
        if scope == Scope::Project {
            self.make_project_directory()?;
        }
        let path = self.fact_path(scope, &fact.key);
        if let Some(parent) = path.parent() {
            home::create_directory(parent)?;
        }
        home::replace(&path, fact.render())?;
        Ok(path)
    }

    /// Add `line` to the rules file for `scope`, on a line of its own, and say which file it went
    /// to. The line is one line: the caller has already flattened it. Refuses while the session may
    /// not write under the state directory.
    pub fn append_rule(&self, scope: Scope, line: &str) -> io::Result<PathBuf> {
        if home::writable().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "nothing is written under the state directory",
            ));
        }
        if scope == Scope::Project {
            self.make_project_directory()?;
        }
        let path = self.rules_path(scope);
        // A file the person edited by hand may end mid-line, and a rule appended to that line
        // would be part of it.
        let ends_mid_line = std::fs::read(&path)
            .map(|held| held.last().is_some_and(|last| *last != b'\n'))
            .unwrap_or(false);
        let mut file = home::append_to_file(&path)?;
        let lead = if ends_mid_line { "\n" } else { "" };
        file.write_all(format!("{lead}{line}\n").as_bytes())?;
        Ok(path)
    }

    /// Make the project's directory and the `id` file that says which project it is for.
    pub(super) fn make_project_directory(&self) -> io::Result<()> {
        home::create_directory(&self.project_directory())?;
        home::write_file(&self.id_path(), self.project.id.as_bytes())
    }
}

/// Add the facts under `directory` to `loaded`, descending into the directories a key's slashes
/// make. A missing directory holds nothing.
fn collect(root: &Path, directory: &Path, scope: Scope, loaded: &mut Loaded) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".md") else {
            // Slug directories are keys' prefixes; anything else here, such as a temporary file
            // left by an interrupted write, is not a fact.
            if kind.is_dir() {
                collect(root, &path, scope, loaded);
            }
            continue;
        };
        if !kind.is_file() {
            loaded.notices.push(format!(
                "{} is not a file, so it was skipped",
                path.display()
            ));
            continue;
        }
        let key = path
            .strip_prefix(root)
            .ok()
            .and_then(Path::to_str)
            .and_then(|relative| relative.strip_suffix(".md"))
            .unwrap_or(stem)
            .replace(std::path::MAIN_SEPARATOR, "/");
        if !is_a_key(&key) {
            loaded.notices.push(format!(
                "{} is not named like a key, so it was skipped",
                path.display()
            ));
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => loaded.facts.push((scope, Fact::parse(&key, &text))),
            Err(error) => loaded.notices.push(format!(
                "{} could not be read as text ({error}), so it was skipped",
                path.display()
            )),
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::recall::fact::Source;
    use bravebot_core::label::Confidentiality;
    use std::os::unix::fs::PermissionsExt;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create scratch");
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn store(home: &Path) -> Store {
        Store::new(
            home,
            ProjectId {
                id: "github.com/o/r".to_string(),
                dir: "0123456789abcdef".to_string(),
            },
        )
    }

    fn fact(key: &str) -> Fact {
        Fact {
            key: key.to_string(),
            source: Source::Person,
            label: Confidentiality::Public,
            created: None,
            verified: None,
            superseded_by: None,
            session: None,
            tags: Vec::new(),
            body: "text\n".to_string(),
        }
    }

    #[test]
    fn a_fact_is_written_where_its_scope_says() {
        let scratch = Scratch::new("recall-store-write");
        let store = store(&scratch.0);

        let project = store.write(Scope::Project, &fact("k")).expect("write");
        assert_eq!(
            project,
            scratch.0.join("projects/0123456789abcdef/facts/k.md")
        );
        let everywhere = store.write(Scope::Everywhere, &fact("k")).expect("write");
        assert_eq!(everywhere, scratch.0.join("facts/k.md"));

        for path in [&project, &everywhere] {
            let mode = std::fs::metadata(path)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{}", path.display());
        }
        let id = scratch.0.join("projects/0123456789abcdef/id");
        assert_eq!(std::fs::read_to_string(id).expect("id"), "github.com/o/r");

        let loaded = store.load();
        assert_eq!(loaded.facts.len(), 2);
        assert!(loaded.notices.is_empty());
    }

    #[test]
    fn a_nested_key_is_written_and_loaded() {
        let scratch = Scratch::new("recall-store-nested");
        let store = store(&scratch.0);
        store
            .write(Scope::Everywhere, &fact("release/forge-poll"))
            .expect("write");
        let loaded = store.load();
        assert_eq!(loaded.facts[0].1.key, "release/forge-poll");
    }

    #[test]
    fn loading_makes_no_directory() {
        let scratch = Scratch::new("recall-store-read");
        let loaded = store(&scratch.0).load();
        assert!(loaded.facts.is_empty() && loaded.notices.is_empty());
        assert_eq!(std::fs::read_dir(&scratch.0).expect("read").count(), 0);
    }

    #[test]
    fn a_file_that_is_not_text_is_named_and_skipped() {
        let scratch = Scratch::new("recall-store-skip");
        let facts = scratch.0.join("facts");
        std::fs::create_dir_all(facts.join("dir.md")).expect("directory");
        std::fs::write(facts.join("binary.md"), [0xff, 0xfe, 0x00]).expect("binary");

        let loaded = store(&scratch.0).load();
        assert!(loaded.facts.is_empty());
        assert_eq!(loaded.notices.len(), 2, "{:?}", loaded.notices);
        assert!(loaded.notices.iter().any(|n| n.contains("binary.md")));
        assert!(loaded.notices.iter().any(|n| n.contains("dir.md")));
    }
}
