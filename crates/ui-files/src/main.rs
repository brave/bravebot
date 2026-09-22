//! UI-only file access. Never executes commands or sends content to the agent.
//! Every path component is opened relative to a pinned directory, without following links.
//!
//! The guarantee rests on `openat` with `NOFOLLOW` per component, so no path can leave the
//! directory it was handed even while the tree changes underneath the walk. That is a POSIX
//! mechanism; Windows reaches the same property by other means, which is not written here. The
//! desktop app this serves is packaged for macOS and Linux only, so rather than compile a weaker
//! walk for a platform nothing ships to, the Windows build refuses every request. An
//! unimplemented boundary is a refusal, the way an unimplemented confinement backend is.
#![forbid(unsafe_code)]

/// Everything below is POSIX. See the note above for why there is no Windows arm.
#[cfg(unix)]
mod posix {

    use rustix::fs::{self as fd_fs, AtFlags, Mode, OFlags};
    use serde::Deserialize;
    use serde_json::{Value, json};
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::path::{Component, Path};

    #[derive(Deserialize)]
    struct Request {
        root: String,
        path: String,
        operation: String,
        limit: Option<usize>,
        text: Option<String>,
        expected: Option<String>,
        /// What to put in `.bravebot-ui/.gitignore` when the grounding walk has to make one.
        ///
        /// A second body rather than a second request, because the directory holding the memory
        /// and the file saying that directory ignores itself are made by the same walk: two
        /// requests would mean two walks, and the second one would start again from `/` with the
        /// first one's descriptors already closed.
        ignore: Option<String>,
    }

    fn invalid(message: &str) -> io::Error {
        io::Error::other(message)
    }
    fn open_at(parent: &File, part: &str, flags: OFlags) -> io::Result<File> {
        // OwnedFd transfers ownership to File without raw descriptor handling.
        fd_fs::openat(
            parent,
            part,
            flags | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::RUSR | Mode::WUSR,
        )
        .map(File::from)
        .map_err(Into::into)
    }
    fn directory_at(parent: &File, part: &str, create: bool) -> io::Result<File> {
        if create {
            match fd_fs::mkdirat(parent, part, Mode::RWXU) {
                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                Err(error) => return Err(error.into()),
            }
        }
        open_at(parent, part, OFlags::RDONLY | OFlags::DIRECTORY)
    }
    fn components(path: &Path) -> io::Result<Vec<&str>> {
        path.components()
            .map(|part| match part {
                Component::Normal(value) => value.to_str().ok_or_else(|| invalid("Invalid path")),
                _ => Err(invalid("Only relative file names are allowed")),
            })
            .collect()
    }
    fn root_directory(root: &str) -> io::Result<File> {
        let root = Path::new(root);
        if !root.is_absolute() {
            return Err(invalid("Project root must be absolute"));
        }
        let mut directory = File::open("/")?;
        for part in components(
            root.strip_prefix("/")
                .map_err(|_| invalid("Invalid root"))?,
        )? {
            directory = directory_at(&directory, part, false)?;
        }
        Ok(directory)
    }
    fn parent_directory(root: &str, path: &str, create: bool) -> io::Result<(File, String)> {
        let parts = components(Path::new(path))?;
        let (leaf, parents) = parts
            .split_last()
            .ok_or_else(|| invalid("A file name is required"))?;
        let mut directory = root_directory(root)?;
        for part in parents {
            directory = directory_at(&directory, part, create)?;
        }
        Ok((directory, (*leaf).to_string()))
    }
    fn read_at(parent: &File, leaf: &str, limit: usize) -> io::Result<(String, bool)> {
        let file = open_at(parent, leaf, OFlags::RDONLY)?;
        if !file.metadata()?.is_file() {
            return Err(invalid("Not a regular file"));
        }
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.contains(&0) {
            return Err(invalid("Not a text file"));
        }
        let truncated = bytes.len() > limit;
        bytes.truncate(limit);
        Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
    }
    /// The largest file this helper will compare, replace, or judge the contents of.
    const TEXT_MAX: usize = 65536;

    /// The one directory inside a checkout that belongs to the front end.
    const HOME: &str = ".bravebot-ui";

    /// What an editor saving its own text is told when the file moved under it.
    const STALE: &str =
        "File changed since this editor opened. Reopen it to review the latest version.";

    /// Whether a regular file is already at `leaf`, for a seed deciding whether to write one.
    ///
    /// The contents are not read and not judged. A seed writes when there is nothing there and
    /// never otherwise: what is in a bot's memory is its own, an unreadable one costs it a
    /// paragraph rather than a turn now that no path a turn names depends on it, and a helper that
    /// decided a file was broken and replaced it would be destroying somebody's data on a
    /// heuristic.
    ///
    /// A link, a directory, a socket: each of those is an error rather than an occupied path, so a
    /// seed refuses instead of renaming over it. The path belongs to this app, so something else at
    /// it is somebody arranging for a write to land where they chose, and silently displacing it
    /// would be the same mistake in the other direction.
    fn occupied(parent: &File, leaf: &str) -> io::Result<bool> {
        let file = match open_at(parent, leaf, OFlags::RDONLY) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if !file.metadata()?.is_file() {
            return Err(invalid("Not a regular file"));
        }
        Ok(true)
    }

    /// The leaf of a bot's memory path, or an error if the path is not one.
    ///
    /// Shared by the channel that replaces a memory and the one that seeds it, so widening either
    /// is one edit rather than two that could drift apart.
    fn memory_leaf(path: &str) -> io::Result<&str> {
        let parts = components(Path::new(path))?;
        match parts.as_slice() {
            [home, bots, leaf] if *home == HOME && *bots == "bots" && leaf.ends_with(".md") => {
                Ok(leaf)
            }
            _ => Err(invalid("Only bot memory can be written")),
        }
    }

    fn replace_at(
        parent: &File,
        leaf: &str,
        text: &str,
        expected: Option<&str>,
    ) -> io::Result<Option<String>> {
        let previous = match read_at(parent, leaf, TEXT_MAX) {
            Ok((_, true)) => return Err(invalid("File exceeds 64 KB")),
            Ok((text, false)) => Some(text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if previous.as_deref() != expected {
            return Err(invalid(STALE));
        }
        // O_EXCL prevents a pre-existing file or link from becoming our temporary file.
        let temporary = format!(
            ".memory-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| invalid("Invalid clock"))?
                .as_nanos()
        );
        let mut file = open_at(
            parent,
            &temporary,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
        )?;
        let result = (|| {
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            fd_fs::renameat(parent, temporary.as_str(), parent, leaf)?;
            Ok(previous)
        })();
        if result.is_err() {
            let _ = fd_fs::unlinkat(parent, temporary.as_str(), AtFlags::empty());
        }
        result
    }
    fn handle(request: Request) -> io::Result<Value> {
        match request.operation.as_str() {
            "read" => {
                let limit = request.limit.unwrap_or(131072).min(262144);
                let (parent, leaf) = parent_directory(&request.root, &request.path, false)?;
                let (text, truncated) = read_at(&parent, &leaf, limit)?;
                Ok(json!({"text": text, "truncated": truncated}))
            }
            // Make a bot's memory file exist, without reading a byte of it back to the caller.
            //
            // The grounding path used to do this walk in TypeScript with `node:fs`, which follows
            // a link at every component: a link at the memory file was read through, and the
            // contents of whatever it pointed at were copied into a briefing the front end then
            // vouched for. The answer is not a second walk that is careful, it is this walk, which
            // is the one the memory's own editor already goes through.
            //
            // It answers whether it wrote, and nothing else. What the memory *says* is the
            // planner's own writing and has no business in a file the front end composes, so this
            // channel has no way to hand it over.
            "memory.seed" => {
                let leaf = memory_leaf(&request.path)?;
                let text = request
                    .text
                    .as_deref()
                    .ok_or_else(|| invalid("Missing memory"))?;
                let ignore = request
                    .ignore
                    .as_deref()
                    .ok_or_else(|| invalid("Missing ignore file"))?;
                for body in [text, ignore] {
                    if body.len() > TEXT_MAX || body.contains('\0') {
                        return Err(invalid("Seeded text must be under 64 KB"));
                    }
                }
                let root = root_directory(&request.root)?;
                let home = directory_at(&root, HOME, true)?;
                let bots = directory_at(&home, "bots", true)?;
                // Written only when absent, and a failure here is not the seed's. The file says
                // the folder ignores itself, which no read of a memory depends on, so a link or a
                // directory where it belongs is left alone rather than refusing every turn this
                // bot has. Never written *through*: that is what `occupied` refusing a link buys.
                if occupied(&home, ".gitignore").is_ok_and(|there| !there) {
                    let _ = replace_at(&home, ".gitignore", ignore, None);
                }
                // `None` is "must still be absent", so a memory that appeared between the look and
                // the write is a refusal rather than a file this replaced.
                let seeded = !occupied(&bots, leaf)?;
                if seeded {
                    replace_at(&bots, leaf, text, None)?;
                }
                Ok(json!({"seeded": seeded}))
            }
            "hooks.read" | "hooks.replace" => {
                if request.path != "hooks.json" {
                    return Err(invalid("Only the hooks file is allowed"));
                }
                let (parent, leaf) = parent_directory(&request.root, "hooks.json", false)?;
                if request.operation == "hooks.read" {
                    return match read_at(&parent, &leaf, TEXT_MAX) {
                        Ok((_, true)) => Err(invalid("Hooks exceed 64 KB")),
                        Ok((text, false)) => Ok(json!({"text": text})),
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {
                            Ok(json!({"text": null}))
                        }
                        Err(error) => Err(error),
                    };
                }
                let text = request.text.ok_or_else(|| invalid("Missing hooks"))?;
                if text.len() > TEXT_MAX || text.contains('\0') {
                    return Err(invalid("Hooks must be text under 64 KB"));
                }
                let previous = replace_at(&parent, &leaf, &text, request.expected.as_deref())?;
                Ok(json!({"previous": previous}))
            }
            "replace" => {
                // This channel writes only bot memory, never an arbitrary project file.
                memory_leaf(&request.path)?;
                let text = request.text.ok_or_else(|| invalid("Missing memory"))?;
                if text.len() > TEXT_MAX || text.contains('\0') {
                    return Err(invalid("Memory must be text under 64 KB"));
                }
                let (parent, leaf) = parent_directory(&request.root, &request.path, true)?;
                let previous = replace_at(&parent, &leaf, &text, request.expected.as_deref())?;
                Ok(json!({"previous": previous}))
            }
            _ => Err(invalid("Unknown file operation")),
        }
    }
    pub fn main() {
        let result = (|| {
            let mut input = String::new();
            io::stdin()
                .take(2 * 1024 * 1024)
                .read_to_string(&mut input)?;
            let request = serde_json::from_str(&input).map_err(|_| invalid("Invalid request"))?;
            handle(request)
        })();
        let response = match result {
            Ok(value) => json!({"ok": value}),
            Err(error) => json!({"error": error.to_string()}),
        };
        println!("{response}");
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;
        use std::os::unix::fs::{PermissionsExt, symlink};
        use std::path::PathBuf;
        struct Fixture {
            path: PathBuf,
            _directory: tempfile::TempDir,
        }
        impl Fixture {
            fn new() -> Self {
                let directory = tempfile::Builder::new()
                    .prefix("bravebot-safe-files-")
                    .permissions(fs::Permissions::from_mode(0o700))
                    .tempdir()
                    .unwrap();
                // Normalize macOS's system temp alias before no-follow traversal.
                let path = fs::canonicalize(directory.path()).unwrap();
                Self {
                    path,
                    _directory: directory,
                }
            }
            fn root(&self) -> &str {
                self.path.to_str().unwrap()
            }
        }

        /// One `memory.seed` request, with both bodies fixed so a test asserts on which landed.
        fn seed(root: &str, path: &str) -> io::Result<Value> {
            handle(Request {
                root: root.to_string(),
                path: path.to_string(),
                operation: "memory.seed".into(),
                limit: None,
                text: Some("seed body".into()),
                expected: None,
                ignore: Some("ignore body".into()),
            })
        }

        #[test]
        fn preview_stays_on_the_pinned_directory_when_its_parent_is_replaced() {
            let f = Fixture::new();
            fs::create_dir(f.path.join("folder")).unwrap();
            fs::create_dir(f.path.join("outside")).unwrap();
            fs::write(f.path.join("folder/file"), "inside").unwrap();
            fs::write(f.path.join("outside/file"), "private outside").unwrap();
            let (parent, leaf) = parent_directory(f.root(), "folder/file", false).unwrap();
            fs::rename(f.path.join("folder"), f.path.join("original")).unwrap();
            symlink(f.path.join("outside"), f.path.join("folder")).unwrap();
            assert_eq!(read_at(&parent, &leaf, 100).unwrap().0, "inside");
            assert!(parent_directory(f.root(), "folder/file", false).is_err());
        }

        #[test]
        fn memory_replace_cannot_follow_a_swapped_parent_to_an_outside_file() {
            let f = Fixture::new();
            let path = ".bravebot-ui/bots/test.md";
            let (parent, leaf) = parent_directory(f.root(), path, true).unwrap();
            replace_at(&parent, &leaf, "before", None).unwrap();
            fs::create_dir(f.path.join("outside")).unwrap();
            fs::write(f.path.join("outside/test.md"), "outside sentinel").unwrap();
            fs::rename(f.path.join(".bravebot-ui/bots"), f.path.join("original")).unwrap();
            symlink(f.path.join("outside"), f.path.join(".bravebot-ui/bots")).unwrap();
            replace_at(&parent, &leaf, "edited", Some("before")).unwrap();
            assert_eq!(
                fs::read_to_string(f.path.join("outside/test.md")).unwrap(),
                "outside sentinel"
            );
            assert_eq!(
                fs::read_to_string(f.path.join("original/test.md")).unwrap(),
                "edited"
            );
            assert!(parent_directory(f.root(), path, true).is_err());
        }

        /// A link at the memory path is refused, in both directions, rather than followed.
        ///
        /// The fault this rejects is the grounding walk this channel replaced: `node:fs` against
        /// a path built by string concatenation, which reads a link's target and writes through
        /// it. Both halves are asserted, because the seed does a read and a write and either one
        /// leaving the checkout is the whole bug: the outside file must keep its bytes, and the
        /// seed must not come back saying it wrote.
        #[test]
        fn seeding_a_memory_cannot_follow_a_link_at_the_memory_path() {
            let f = Fixture::new();
            fs::create_dir(f.path.join("outside")).unwrap();
            fs::write(f.path.join("outside/secret.md"), "outside sentinel").unwrap();
            fs::create_dir_all(f.path.join(".bravebot-ui/bots")).unwrap();
            symlink(
                f.path.join("outside/secret.md"),
                f.path.join(".bravebot-ui/bots/test.md"),
            )
            .unwrap();

            assert!(seed(f.root(), ".bravebot-ui/bots/test.md").is_err());
            assert_eq!(
                fs::read_to_string(f.path.join("outside/secret.md")).unwrap(),
                "outside sentinel"
            );
        }

        /// The same for the directory above it, swapped after the walk pinned it.
        ///
        /// The pinned descriptor is what the seed writes through, so a link put where the name
        /// used to be reaches nothing: the seed lands in the directory that was opened, and a
        /// fresh walk by name is refused outright.
        #[test]
        fn seeding_a_memory_cannot_follow_a_swapped_parent_to_an_outside_file() {
            let f = Fixture::new();
            let path = ".bravebot-ui/bots/test.md";
            let root = root_directory(f.root()).unwrap();
            let home = directory_at(&root, HOME, true).unwrap();
            let bots = directory_at(&home, "bots", true).unwrap();
            fs::create_dir(f.path.join("outside")).unwrap();
            fs::write(f.path.join("outside/test.md"), "outside sentinel").unwrap();
            fs::rename(f.path.join(".bravebot-ui/bots"), f.path.join("original")).unwrap();
            symlink(f.path.join("outside"), f.path.join(".bravebot-ui/bots")).unwrap();

            assert!(!occupied(&bots, "test.md").unwrap());
            replace_at(&bots, "test.md", "seeded", None).unwrap();
            assert_eq!(
                fs::read_to_string(f.path.join("outside/test.md")).unwrap(),
                "outside sentinel"
            );
            assert_eq!(
                fs::read_to_string(f.path.join("original/test.md")).unwrap(),
                "seeded"
            );
            assert!(seed(f.root(), path).is_err());
        }

        /// A seed writes what is absent and never what is there, whatever is in it.
        ///
        /// The fault this rejects is a seed that writes every time it is called: grounding runs on
        /// the way into every send, so that would erase the memory on each one. `seeded` is the
        /// answer, so the second call has to come back false with the text still there.
        ///
        /// The last case is the one worth being deliberate about. A memory whose bytes are not
        /// text is left exactly as it is: a seed that judged contents and replaced what it did not
        /// like would be destroying somebody's data on a heuristic, and an unreadable memory now
        /// costs a paragraph rather than a turn, since no path a turn names depends on it.
        #[test]
        fn a_seed_creates_what_is_missing_and_replaces_nothing() {
            let f = Fixture::new();
            let path = ".bravebot-ui/bots/test.md";
            assert_eq!(seed(f.root(), path).unwrap()["seeded"], true);
            assert_eq!(fs::read_to_string(f.path.join(path)).unwrap(), "seed body");
            assert_eq!(
                fs::read_to_string(f.path.join(".bravebot-ui/.gitignore")).unwrap(),
                "ignore body"
            );

            fs::write(f.path.join(path), "what the bot remembered").unwrap();
            fs::write(f.path.join(".bravebot-ui/.gitignore"), "*").unwrap();
            assert_eq!(seed(f.root(), path).unwrap()["seeded"], false);
            assert_eq!(
                fs::read_to_string(f.path.join(path)).unwrap(),
                "what the bot remembered"
            );
            assert_eq!(
                fs::read_to_string(f.path.join(".bravebot-ui/.gitignore")).unwrap(),
                "*"
            );

            fs::write(f.path.join(path), b"a\0b").unwrap();
            assert_eq!(seed(f.root(), path).unwrap()["seeded"], false);
            assert_eq!(fs::read(f.path.join(path)).unwrap(), b"a\0b");
        }

        /// A link where the `.gitignore` belongs costs the bot nothing and is not written through.
        ///
        /// The fault this rejects is the refusal being the whole seed's: nothing a memory read
        /// depends on is in that file, so failing every grounded turn in the checkout over it
        /// would be a denial of service dressed as a boundary. The other fault is the obvious one,
        /// writing the ignore body through the link, which is what the old walk did.
        #[test]
        fn a_link_where_the_ignore_file_belongs_neither_stops_a_seed_nor_takes_its_write() {
            let f = Fixture::new();
            fs::create_dir_all(f.path.join(".bravebot-ui")).unwrap();
            fs::write(f.path.join("outside.txt"), "outside sentinel").unwrap();
            symlink(
                f.path.join("outside.txt"),
                f.path.join(".bravebot-ui/.gitignore"),
            )
            .unwrap();

            assert_eq!(
                seed(f.root(), ".bravebot-ui/bots/test.md").unwrap()["seeded"],
                true
            );
            assert_eq!(
                fs::read_to_string(f.path.join("outside.txt")).unwrap(),
                "outside sentinel"
            );
        }

        /// The seed channel writes bot memory and nothing else.
        #[test]
        fn a_seed_refuses_every_path_that_is_not_a_bot_memory() {
            let f = Fixture::new();
            for path in [
                ".bravebot-ui/bots/test.txt",
                ".bravebot-ui/test.md",
                ".bravebot-ui/bots/nested/test.md",
                "bots/test.md",
                "../outside/test.md",
                "/etc/passwd",
            ] {
                assert!(seed(f.root(), path).is_err(), "a seed must refuse {path}");
            }
            assert!(!f.path.join(".bravebot-ui").exists());
        }

        #[test]
        fn a_symlink_replacing_the_root_or_leaf_is_refused() {
            let f = Fixture::new();
            fs::create_dir(f.path.join("project")).unwrap();
            fs::write(f.path.join("secret"), "outside").unwrap();
            symlink(f.path.join("secret"), f.path.join("project/file")).unwrap();
            let (parent, leaf) = parent_directory(f.root(), "project/file", false).unwrap();
            assert!(read_at(&parent, &leaf, 100).is_err());
            assert!(replace_at(&parent, &leaf, "changed", None).is_err());
            symlink(&f.path, f.path.join("linked-root")).unwrap();
            assert!(root_directory(f.path.join("linked-root").to_str().unwrap()).is_err());
            assert_eq!(
                fs::read_to_string(f.path.join("secret")).unwrap(),
                "outside"
            );
        }

        #[test]
        fn replacement_checks_expected_text_and_creates_a_private_regular_file() {
            let f = Fixture::new();
            let (parent, leaf) =
                parent_directory(f.root(), ".bravebot-ui/bots/test.md", true).unwrap();
            replace_at(&parent, &leaf, "before", None).unwrap();
            assert!(replace_at(&parent, &leaf, "wrong", None).is_err());
            assert_eq!(
                replace_at(&parent, &leaf, "after", Some("before")).unwrap(),
                Some("before".into())
            );
            assert_eq!(read_at(&parent, &leaf, 100).unwrap().0, "after");
            assert_eq!(
                fs::metadata(f.path.join(".bravebot-ui/bots/test.md"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                fs::read_dir(f.path.join(".bravebot-ui/bots"))
                    .unwrap()
                    .count(),
                1
            );
        }

        #[test]
        fn traversal_nonregular_binary_and_oversized_reads_are_bounded() {
            let f = Fixture::new();
            for path in ["../secret", "/etc/passwd", "", "a/../../secret"] {
                assert!(parent_directory(f.root(), path, false).is_err());
            }
            fs::write(f.path.join("binary"), b"a\0b").unwrap();
            fs::write(f.path.join("large"), "123456").unwrap();
            let parent = root_directory(f.root()).unwrap();
            assert!(read_at(&parent, "binary", 100).is_err());
            assert_eq!(read_at(&parent, "large", 3).unwrap(), ("123".into(), true));
            fs::create_dir(f.path.join("directory")).unwrap();
            assert!(read_at(&parent, "directory", 100).is_err());
            assert!(read_at(&parent, "bad\0name", 100).is_err());
        }

        /// A root that is not absolute is refused rather than resolved against anything.
        ///
        /// The walk starts at `/` and descends by name, so a relative root has no directory to
        /// start from that anybody named: taking it against the working directory would put the
        /// boundary wherever this process happens to have been started, which is not a place a
        /// person opened. A root that merely looks absolute after a `..` is refused too, since
        /// the components are taken as written.
        #[test]
        fn a_root_that_is_not_an_absolute_path_is_refused() {
            let f = Fixture::new();
            fs::create_dir(f.path.join("project")).unwrap();
            for root in ["project", "./project", "", "relative/project"] {
                assert!(
                    root_directory(root).is_err(),
                    "a relative root must not resolve: {root}"
                );
            }
            assert!(root_directory(&format!("{}/../..", f.root())).is_err());
            // The absolute spelling of the same tree is what does work, so the refusals above
            // are about the spelling rather than about an unreadable directory.
            assert!(root_directory(f.path.join("project").to_str().unwrap()).is_ok());
        }

        #[test]
        fn exclusive_temporary_creation_refuses_existing_files_and_links() {
            let f = Fixture::new();
            fs::write(f.path.join("existing"), "sentinel").unwrap();
            symlink(f.path.join("existing"), f.path.join("link")).unwrap();
            let parent = root_directory(f.root()).unwrap();
            for leaf in ["existing", "link"] {
                assert!(
                    open_at(
                        &parent,
                        leaf,
                        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL
                    )
                    .is_err()
                );
            }
            assert_eq!(
                fs::read_to_string(f.path.join("existing")).unwrap(),
                "sentinel"
            );
        }
    }

    #[cfg(test)]
    mod hooks_tests {
        use super::*;
        use std::fs;
        #[test]
        fn hooks_channel_only_replaces_its_fixed_file_and_preserves_concurrent_edits() {
            let mut builder = tempfile::Builder::new();
            builder.prefix("bravebot-hook-helper-");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                builder.permissions(std::fs::Permissions::from_mode(0o700));
            }
            let directory = builder.tempdir().unwrap();
            // Normalize the system temp alias before the helper's no-follow traversal.
            let root = fs::canonicalize(directory.path()).unwrap();
            let call = |operation: &str, path: &str, text: Option<&str>, expected: Option<&str>| {
                handle(Request {
                    root: root.display().to_string(),
                    path: path.into(),
                    operation: operation.into(),
                    limit: None,
                    text: text.map(str::to_string),
                    expected: expected.map(str::to_string),
                    ignore: None,
                })
            };
            assert!(call("hooks.read", "hooks.json", None, None).unwrap()["text"].is_null());
            assert!(call("hooks.replace", "other.json", Some("{}"), None).is_err());
            assert!(call("hooks.replace", "hooks.json", Some("first"), None).is_ok());
            assert!(call("hooks.replace", "hooks.json", Some("overwrite"), None).is_err());
            assert_eq!(
                fs::read_to_string(root.join("hooks.json")).unwrap(),
                "first"
            );
        }
    }
}

#[cfg(unix)]
fn main() {
    posix::main()
}

#[cfg(not(unix))]
fn main() {
    // The same envelope shape the POSIX build answers with, so one caller reads both.
    println!(
        "{}",
        serde_json::json!({
            "error": "File access is not implemented on this platform. Brave Bot is packaged \
                      for macOS and Linux."
        })
    );
}
