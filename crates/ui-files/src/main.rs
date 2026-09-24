//! UI-only file access. Never executes commands or sends content to the agent.
//! Every path component is opened relative to a pinned directory, without following links.
//!
//! No path can leave the directory it was handed, even while the tree changes underneath the
//! walk, because each component is opened relative to the handle of the one above it and a link
//! there is refused rather than resolved. The platform modules hold that one open: `openat` with
//! `O_NOFOLLOW` on POSIX, `NtCreateFile` with `OBJ_DONT_REPARSE` on Windows. Which paths are
//! accepted, and what is read and written, is decided here once for both.
#![deny(unsafe_code)]
// `deny` rather than `forbid` for the Windows calls, each of which names itself in `windows.rs`.
#![cfg_attr(not(windows), forbid(unsafe_code))]

#[cfg(any(windows, test))]
mod names;
#[cfg(unix)]
mod posix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use posix as sys;
#[cfg(windows)]
use windows as sys;

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
    /// A second body rather than a second request, because the directory holding the memory and
    /// the file saying that directory ignores itself are made by the same walk: two requests would
    /// mean two walks, and the second one would start again from the volume root with the first
    /// one's handles already closed.
    ignore: Option<String>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}

/// The names in a path, refusing any component that is not a plain name.
///
/// `root` is whether the path is the directory a person opened rather than one inside it, which
/// matters on Windows only: see `names::misleads`.
fn components(path: &Path, root: bool) -> io::Result<Vec<&str>> {
    path.components()
        .map(|part| match part {
            Component::Normal(value) => {
                let value = value.to_str().ok_or_else(|| invalid("Invalid path"))?;
                if sys::misleads(value, root) {
                    return Err(invalid("Not a plain file name on Windows"));
                }
                Ok(value)
            }
            _ => Err(invalid("Only relative file names are allowed")),
        })
        .collect()
}
fn root_directory(root: &str) -> io::Result<File> {
    let (mut directory, rest) = sys::volume(Path::new(root))?;
    for part in components(rest, true)? {
        directory = sys::directory_at(&directory, part, false)?;
    }
    Ok(directory)
}
fn parent_directory(root: &str, path: &str, create: bool) -> io::Result<(File, String)> {
    let parts = components(Path::new(path), false)?;
    let (leaf, parents) = parts
        .split_last()
        .ok_or_else(|| invalid("A file name is required"))?;
    let mut directory = root_directory(root)?;
    for part in parents {
        directory = sys::directory_at(&directory, part, create)?;
    }
    Ok((directory, (*leaf).to_string()))
}
fn read_at(parent: &File, leaf: &str, limit: usize) -> io::Result<(String, bool)> {
    let file = sys::open_at(parent, leaf)?;
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
/// The contents are not read and not judged. A seed writes when there is nothing there and never
/// otherwise: what is in a bot's memory is its own, an unreadable one costs it a paragraph rather
/// than a turn now that no path a turn names depends on it, and a helper that decided a file was
/// broken and replaced it would be destroying somebody's data on a heuristic.
///
/// A link, a directory, a socket: each of those is an error rather than an occupied path, so a
/// seed refuses instead of renaming over it. The path belongs to this app, so something else at it
/// is somebody arranging for a write to land where they chose, and silently displacing it would be
/// the same mistake in the other direction.
fn occupied(parent: &File, leaf: &str) -> io::Result<bool> {
    let file = match sys::open_at(parent, leaf) {
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
/// Shared by the channel that replaces a memory and the one that seeds it, so widening either is
/// one edit rather than two that could drift apart. The match is exact on every platform, so a
/// spelling Windows would open as the same file in another case is refused rather than normalised.
fn memory_leaf(path: &str) -> io::Result<&str> {
    let parts = components(Path::new(path), false)?;
    match parts.as_slice() {
        [home, bots, leaf] if *home == HOME && *bots == "bots" && leaf.ends_with(".md") => Ok(leaf),
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
    let temporary = format!(
        ".memory-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| invalid("Invalid clock"))?
            .as_nanos()
    );
    let mut file = sys::create_new_at(parent, &temporary)?;
    let result = (|| {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        sys::rename_at(parent, &file, &temporary, leaf)?;
        Ok(previous)
    })();
    if result.is_err() {
        sys::remove_at(parent, &file, &temporary);
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
        // The grounding path used to do this walk in TypeScript with `node:fs`, which follows a
        // link at every component: a link at the memory file was read through, and the contents of
        // whatever it pointed at were copied into a briefing the front end then vouched for. The
        // answer is not a second walk that is careful, it is this walk, which is the one the
        // memory's own editor already goes through.
        //
        // It answers whether it wrote, and nothing else. What the memory *says* is the planner's
        // own writing and has no business in a file the front end composes, so this channel has no
        // way to hand it over.
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
            let home = sys::directory_at(&root, HOME, true)?;
            let bots = sys::directory_at(&home, "bots", true)?;
            // Written only when absent, and a failure here is not the seed's. The file says the
            // folder ignores itself, which no read of a memory depends on, so a link or a directory
            // where it belongs is left alone rather than refusing every turn this bot has. Never
            // written *through*: that is what `occupied` refusing a link buys.
            if occupied(&home, ".gitignore").is_ok_and(|there| !there) {
                let _ = replace_at(&home, ".gitignore", ignore, None);
            }
            // `None` is "must still be absent", so a memory that appeared between the look and the
            // write is a refusal rather than a file this replaced.
            let seeded = !occupied(&bots, leaf)?;
            if seeded {
                replace_at(&bots, leaf, text, None)?;
            }
            Ok(json!({"seeded": seeded}))
        }
        // Writing only. What the file says is asked of the agent, which is the reader a turn fires
        // hooks out of, so nothing here decides what one is.
        "hooks.replace" => {
            if request.path != "hooks.json" {
                return Err(invalid("Only the hooks file is allowed"));
            }
            let (parent, leaf) = parent_directory(&request.root, "hooks.json", false)?;
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
fn main() {
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
    use std::path::PathBuf;

    /// A kind of link a walk could be sent through.
    #[derive(Clone, Copy, Debug)]
    enum Link {
        Symlink,
        /// A directory junction, which on Windows needs no privilege to make.
        #[cfg(windows)]
        Junction,
    }

    #[cfg(unix)]
    const LINKS: [Link; 1] = [Link::Symlink];
    #[cfg(windows)]
    const LINKS: [Link; 2] = [Link::Symlink, Link::Junction];

    fn link(target: &Path, at: &Path, kind: Link) {
        match kind {
            #[cfg(unix)]
            Link::Symlink => std::os::unix::fs::symlink(target, at).unwrap(),
            #[cfg(windows)]
            Link::Symlink if target.is_dir() => {
                std::os::windows::fs::symlink_dir(target, at).unwrap()
            }
            #[cfg(windows)]
            Link::Symlink => std::os::windows::fs::symlink_file(target, at).unwrap(),
            #[cfg(windows)]
            Link::Junction => {
                // `mklink` reads a `/` as the start of a switch.
                let native = |path: &Path| path.to_string_lossy().replace('/', "\\");
                let made = std::process::Command::new("cmd")
                    .args(["/C", "mklink", "/J"])
                    .arg(native(at))
                    .arg(native(target))
                    .stdout(std::process::Stdio::null())
                    .status()
                    .unwrap();
                assert!(made.success(), "mklink /J {}", at.display());
            }
        }
    }

    /// `canonicalize` on Windows answers in the `\\?\` form, which the helper refuses as a root.
    fn plain(path: PathBuf) -> PathBuf {
        #[cfg(windows)]
        if let Some(rest) = path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
            return PathBuf::from(rest);
        }
        path
    }

    struct Fixture {
        path: PathBuf,
        _directory: tempfile::TempDir,
    }
    impl Fixture {
        fn new() -> Self {
            let mut builder = tempfile::Builder::new();
            builder.prefix("bravebot-safe-files-");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                builder.permissions(fs::Permissions::from_mode(0o700));
            }
            let directory = builder.tempdir().unwrap();
            // Normalize the system temp alias before no-follow traversal.
            let path = plain(fs::canonicalize(directory.path()).unwrap());
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

    /// One request of any other kind, writing `text` where it writes.
    fn call(root: &str, operation: &str, path: &str) -> io::Result<Value> {
        handle(Request {
            root: root.to_string(),
            path: path.to_string(),
            operation: operation.into(),
            limit: None,
            text: Some("written".into()),
            expected: None,
            ignore: None,
        })
    }

    #[test]
    fn preview_stays_on_the_pinned_directory_when_its_parent_is_replaced() {
        for kind in LINKS {
            let f = Fixture::new();
            fs::create_dir(f.path.join("folder")).unwrap();
            fs::create_dir(f.path.join("outside")).unwrap();
            fs::write(f.path.join("folder/file"), "inside").unwrap();
            fs::write(f.path.join("outside/file"), "private outside").unwrap();
            let (parent, leaf) = parent_directory(f.root(), "folder/file", false).unwrap();
            fs::rename(f.path.join("folder"), f.path.join("original")).unwrap();
            link(&f.path.join("outside"), &f.path.join("folder"), kind);
            assert_eq!(
                read_at(&parent, &leaf, 100).unwrap().0,
                "inside",
                "{kind:?}"
            );
            assert!(
                parent_directory(f.root(), "folder/file", false).is_err(),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn memory_replace_cannot_follow_a_swapped_parent_to_an_outside_file() {
        for kind in LINKS {
            let f = Fixture::new();
            let path = ".bravebot-ui/bots/test.md";
            let (parent, leaf) = parent_directory(f.root(), path, true).unwrap();
            replace_at(&parent, &leaf, "before", None).unwrap();
            fs::create_dir(f.path.join("outside")).unwrap();
            fs::write(f.path.join("outside/test.md"), "outside sentinel").unwrap();
            fs::rename(f.path.join(".bravebot-ui/bots"), f.path.join("original")).unwrap();
            link(
                &f.path.join("outside"),
                &f.path.join(".bravebot-ui/bots"),
                kind,
            );
            replace_at(&parent, &leaf, "edited", Some("before")).unwrap();
            assert_eq!(
                fs::read_to_string(f.path.join("outside/test.md")).unwrap(),
                "outside sentinel",
                "{kind:?}"
            );
            assert_eq!(
                fs::read_to_string(f.path.join("original/test.md")).unwrap(),
                "edited",
                "{kind:?}"
            );
            assert!(parent_directory(f.root(), path, true).is_err(), "{kind:?}");
        }
    }

    /// A link at the memory path is refused, in both directions, rather than followed.
    ///
    /// The fault this rejects is the grounding walk this channel replaced: `node:fs` against a
    /// path built by string concatenation, which reads a link's target and writes through it. Both
    /// halves are asserted, because the seed does a read and a write and either one leaving the
    /// checkout is the whole bug: the outside file must keep its bytes, and the seed must not come
    /// back saying it wrote.
    #[test]
    fn seeding_a_memory_cannot_follow_a_link_at_the_memory_path() {
        for kind in LINKS {
            let f = Fixture::new();
            fs::create_dir(f.path.join("outside")).unwrap();
            fs::write(f.path.join("outside/secret.md"), "outside sentinel").unwrap();
            fs::create_dir_all(f.path.join(".bravebot-ui/bots")).unwrap();
            link(
                &f.path.join("outside/secret.md"),
                &f.path.join(".bravebot-ui/bots/test.md"),
                kind,
            );

            assert!(
                seed(f.root(), ".bravebot-ui/bots/test.md").is_err(),
                "{kind:?}"
            );
            assert_eq!(
                fs::read_to_string(f.path.join("outside/secret.md")).unwrap(),
                "outside sentinel",
                "{kind:?}"
            );
        }
    }

    /// The same for the directory above it, swapped after the walk pinned it.
    ///
    /// The pinned handle is what the seed writes through, so a link put where the name used to be
    /// reaches nothing: the seed lands in the directory that was opened, and a fresh walk by name
    /// is refused outright.
    #[test]
    fn seeding_a_memory_cannot_follow_a_swapped_parent_to_an_outside_file() {
        for kind in LINKS {
            let f = Fixture::new();
            let path = ".bravebot-ui/bots/test.md";
            let root = root_directory(f.root()).unwrap();
            let home = sys::directory_at(&root, HOME, true).unwrap();
            let bots = sys::directory_at(&home, "bots", true).unwrap();
            fs::create_dir(f.path.join("outside")).unwrap();
            fs::write(f.path.join("outside/test.md"), "outside sentinel").unwrap();
            fs::rename(f.path.join(".bravebot-ui/bots"), f.path.join("original")).unwrap();
            link(
                &f.path.join("outside"),
                &f.path.join(".bravebot-ui/bots"),
                kind,
            );

            assert!(!occupied(&bots, "test.md").unwrap(), "{kind:?}");
            replace_at(&bots, "test.md", "seeded", None).unwrap();
            assert_eq!(
                fs::read_to_string(f.path.join("outside/test.md")).unwrap(),
                "outside sentinel",
                "{kind:?}"
            );
            assert_eq!(
                fs::read_to_string(f.path.join("original/test.md")).unwrap(),
                "seeded",
                "{kind:?}"
            );
            assert!(seed(f.root(), path).is_err(), "{kind:?}");
        }
    }

    /// A link at any directory the walk passes through is refused, and nothing beyond it moves.
    ///
    /// The fault this rejects is a walk that opens a directory component by following what is
    /// there: `openat` without `O_NOFOLLOW`, or `NtCreateFile` without `OBJ_DONT_REPARSE` and
    /// `FILE_OPEN_REPARSE_POINT`. The leaf and the root's last component are
    /// `a_symlink_replacing_the_root_or_leaf_is_refused`; these are the components between, and
    /// the two directories the memory channels create.
    #[test]
    fn a_link_at_any_component_is_refused_and_nothing_outside_changes() {
        for kind in LINKS {
            let f = Fixture::new();
            fs::create_dir_all(f.path.join("real/project/folder")).unwrap();
            fs::write(f.path.join("real/project/folder/file"), "outside").unwrap();
            fs::create_dir_all(f.path.join("outside/bots")).unwrap();
            fs::create_dir(f.path.join("project")).unwrap();

            // A root whose middle component is a link.
            link(&f.path.join("real"), &f.path.join("linked"), kind);
            let through = f.path.join("linked/project");
            assert!(
                root_directory(through.to_str().unwrap()).is_err(),
                "{kind:?}"
            );
            assert!(
                call(through.to_str().unwrap(), "read", "folder/file").is_err(),
                "{kind:?}"
            );

            // A path whose middle component is a link.
            let project = f.path.join("project");
            link(
                &f.path.join("real/project/folder"),
                &project.join("folder"),
                kind,
            );
            assert!(
                call(project.to_str().unwrap(), "read", "folder/file").is_err(),
                "{kind:?}"
            );

            // The front end's own directory, and the one inside it, as links.
            link(&f.path.join("outside"), &project.join(HOME), kind);
            for operation in ["memory.seed", "replace"] {
                let refused = if operation == "memory.seed" {
                    seed(project.to_str().unwrap(), ".bravebot-ui/bots/test.md")
                } else {
                    call(
                        project.to_str().unwrap(),
                        operation,
                        ".bravebot-ui/bots/test.md",
                    )
                };
                assert!(refused.is_err(), "{operation} {kind:?}");
            }
            // A link to a directory is a file on POSIX and a directory on Windows.
            fs::remove_file(project.join(HOME))
                .or_else(|_| fs::remove_dir(project.join(HOME)))
                .unwrap();
            fs::create_dir(project.join(HOME)).unwrap();
            link(
                &f.path.join("outside/bots"),
                &project.join(".bravebot-ui/bots"),
                kind,
            );
            for operation in ["memory.seed", "replace"] {
                let refused = if operation == "memory.seed" {
                    seed(project.to_str().unwrap(), ".bravebot-ui/bots/test.md")
                } else {
                    call(
                        project.to_str().unwrap(),
                        operation,
                        ".bravebot-ui/bots/test.md",
                    )
                };
                assert!(refused.is_err(), "{operation} {kind:?}");
            }

            assert_eq!(
                fs::read_dir(f.path.join("outside/bots")).unwrap().count(),
                0,
                "{kind:?}"
            );
            assert_eq!(
                fs::read_dir(f.path.join("outside")).unwrap().count(),
                1,
                "{kind:?}"
            );
            assert_eq!(
                fs::read_to_string(f.path.join("real/project/folder/file")).unwrap(),
                "outside"
            );
        }
    }

    /// A seed writes what is absent and never what is there, whatever is in it.
    ///
    /// The fault this rejects is a seed that writes every time it is called: grounding runs on the
    /// way into every send, so that would erase the memory on each one. `seeded` is the answer, so
    /// the second call has to come back false with the text still there.
    ///
    /// The last case is the one worth being deliberate about. A memory whose bytes are not text is
    /// left exactly as it is: a seed that judged contents and replaced what it did not like would be
    /// destroying somebody's data on a heuristic, and an unreadable memory now costs a paragraph
    /// rather than a turn, since no path a turn names depends on it.
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
    /// The fault this rejects is the refusal being the whole seed's: nothing a memory read depends
    /// on is in that file, so failing every grounded turn in the checkout over it would be a denial
    /// of service dressed as a boundary. The other fault is the obvious one, writing the ignore
    /// body through the link, which is what the old walk did.
    #[test]
    fn a_link_where_the_ignore_file_belongs_neither_stops_a_seed_nor_takes_its_write() {
        for kind in LINKS {
            let f = Fixture::new();
            fs::create_dir_all(f.path.join(".bravebot-ui")).unwrap();
            fs::write(f.path.join("outside.txt"), "outside sentinel").unwrap();
            link(
                &f.path.join("outside.txt"),
                &f.path.join(".bravebot-ui/.gitignore"),
                kind,
            );

            assert_eq!(
                seed(f.root(), ".bravebot-ui/bots/test.md").unwrap()["seeded"],
                true,
                "{kind:?}"
            );
            assert_eq!(
                fs::read_to_string(f.path.join("outside.txt")).unwrap(),
                "outside sentinel",
                "{kind:?}"
            );
        }
    }

    /// The seed channel writes bot memory and nothing else.
    ///
    /// The spellings in another case are the ones Windows and macOS would open as the real memory
    /// directory, so an accepted-path check that folded case would pass them there.
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
            ".BRAVEBOT-UI/bots/test.md",
            ".bravebot-ui/Bots/test.md",
            ".bravebot-ui/bots/test.MD",
        ] {
            assert!(seed(f.root(), path).is_err(), "a seed must refuse {path}");
        }
        assert_eq!(fs::read_dir(&f.path).unwrap().count(), 0);
    }

    #[test]
    fn a_symlink_replacing_the_root_or_leaf_is_refused() {
        for kind in LINKS {
            let f = Fixture::new();
            fs::create_dir(f.path.join("project")).unwrap();
            fs::write(f.path.join("secret"), "outside").unwrap();
            link(&f.path.join("secret"), &f.path.join("project/file"), kind);
            let (parent, leaf) = parent_directory(f.root(), "project/file", false).unwrap();
            assert!(read_at(&parent, &leaf, 100).is_err(), "{kind:?}");
            assert!(
                replace_at(&parent, &leaf, "changed", None).is_err(),
                "{kind:?}"
            );
            link(&f.path, &f.path.join("linked-root"), kind);
            assert!(
                root_directory(f.path.join("linked-root").to_str().unwrap()).is_err(),
                "{kind:?}"
            );
            assert_eq!(
                fs::read_to_string(f.path.join("secret")).unwrap(),
                "outside",
                "{kind:?}"
            );
        }
    }

    #[test]
    fn replacement_checks_expected_text_and_creates_a_private_regular_file() {
        let f = Fixture::new();
        let (parent, leaf) = parent_directory(f.root(), ".bravebot-ui/bots/test.md", true).unwrap();
        replace_at(&parent, &leaf, "before", None).unwrap();
        assert!(replace_at(&parent, &leaf, "wrong", None).is_err());
        assert_eq!(
            replace_at(&parent, &leaf, "after", Some("before")).unwrap(),
            Some("before".into())
        );
        assert_eq!(read_at(&parent, &leaf, 100).unwrap().0, "after");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(f.path.join(".bravebot-ui/bots/test.md"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        // Private on Windows is a list naming this account alone, protected so the directory's
        // inheritable entries do not join it. A directory's entry is inherited by what is made in
        // it, which is how `0700` reads for a file someone else puts there.
        #[cfg(windows)]
        {
            let account = sys::current_account().unwrap();
            let split = |list: String| {
                let (flags, entries) = list
                    .strip_prefix("D:")
                    .and_then(|rest| rest.split_once('('))
                    .unwrap_or_else(|| panic!("{list}"));
                (flags.to_string(), format!("({entries}"))
            };
            for (path, entry) in [
                (".bravebot-ui/bots/test.md", ""),
                (".bravebot-ui/bots", "OICI"),
                (".bravebot-ui", "OICI"),
            ] {
                let (flags, entries) = split(sys::list_at(&f.path.join(path)).unwrap());
                assert!(flags.contains('P'), "{path} inherits: {flags}{entries}");
                let expected = sys::written(&format!("D:P(A;{entry};FA;;;{account})")).unwrap();
                assert_eq!(entries, split(expected).1, "{path}");
            }
        }
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
    /// The walk starts at the volume root and descends by name, so a relative root has no
    /// directory to start from that anybody named: taking it against the working directory would
    /// put the boundary wherever this process happens to have been started, which is not a place a
    /// person opened. A root that merely looks absolute after a `..` is refused too, since the
    /// components are taken as written.
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
        // The absolute spelling of the same tree is what does work, so the refusals above are
        // about the spelling rather than about an unreadable directory.
        assert!(root_directory(f.path.join("project").to_str().unwrap()).is_ok());
    }

    #[test]
    fn exclusive_temporary_creation_refuses_existing_files_and_links() {
        for kind in LINKS {
            let f = Fixture::new();
            fs::write(f.path.join("existing"), "sentinel").unwrap();
            link(&f.path.join("existing"), &f.path.join("link"), kind);
            let parent = root_directory(f.root()).unwrap();
            for leaf in ["existing", "link"] {
                assert!(
                    sys::create_new_at(&parent, leaf).is_err(),
                    "{leaf} {kind:?}"
                );
            }
            assert_eq!(
                fs::read_to_string(f.path.join("existing")).unwrap(),
                "sentinel",
                "{kind:?}"
            );
        }
    }

    /// No caller passes a name with a separator, but the open resolves no link inside one either:
    /// the fault is an open that refuses a link only as its last component.
    #[cfg(windows)]
    #[test]
    fn a_relative_open_follows_no_link_before_its_last_component() {
        for kind in LINKS {
            let f = Fixture::new();
            fs::create_dir(f.path.join("real")).unwrap();
            fs::write(f.path.join("real").join("secret"), "outside").unwrap();
            link(&f.path.join("real"), &f.path.join("link"), kind);
            let parent = root_directory(f.root()).unwrap();
            assert!(sys::open_at(&parent, r"real\secret").is_ok(), "{kind:?}");
            assert!(sys::open_at(&parent, r"link\secret").is_err(), "{kind:?}");
        }
    }

    /// Every spelling Windows reads as a name other than the one written is refused by the walk,
    /// before any channel's own check of which path it accepts.
    ///
    /// The fault this rejects is a walk that hands each name to `NtCreateFile` as it is: `:stream`
    /// then writes a stream of a memory rather than a memory, and `CON.md` or a trailing dot makes
    /// a file ordinary Windows tools cannot name. The error is asserted, not just refusal, since
    /// the order is the point: the accepted-path check would refuse some of these for a reason of
    /// its own and pass the rest.
    #[cfg(windows)]
    #[test]
    fn windows_name_forms_are_refused_before_the_accepted_path_check() {
        let f = Fixture::new();
        let names = "Not a plain file name on Windows";
        let relative = "Only relative file names are allowed";
        for (path, error) in [
            (r".bravebot-ui\bots\test.md:stream", names),
            (".bravebot-ui/bots/test.md::$DATA", names),
            (".bravebot-ui/bots/CON.md", names),
            (".bravebot-ui/bots/nul.md", names),
            (".bravebot-ui/bots/COM1.md", names),
            (".bravebot-ui/bots/LPT¹.md", names),
            (".bravebot-ui/bots./test.md", names),
            (".bravebot-ui /bots/test.md", names),
            (".bravebot-ui/bots/TEST~1.md", names),
            ("BRAVEB~1/bots/test.md", names),
            (r"C:.bravebot-ui\bots\test.md", relative),
            (r"C:\.bravebot-ui\bots\test.md", relative),
            (r"\\?\C:\.bravebot-ui\bots\test.md", relative),
            (r"\\server\share\.bravebot-ui\bots\test.md", relative),
            (r"\\.\C:\.bravebot-ui\bots\test.md", relative),
            (r"\.bravebot-ui\bots\test.md", relative),
        ] {
            for operation in ["memory.seed", "replace", "read"] {
                let answer = if operation == "memory.seed" {
                    seed(f.root(), path)
                } else {
                    call(f.root(), operation, path)
                };
                assert_eq!(
                    answer.err().map(|refused| refused.to_string()).as_deref(),
                    Some(error),
                    "{operation} {path}"
                );
            }
        }
        assert_eq!(fs::read_dir(&f.path).unwrap().count(), 0);

        // The same forms as the directory a person opened. A short name is the one accepted, since
        // a root is the directory somebody chose and its short spelling names that one directory.
        let root = f.root();
        let drive = &root[..2];
        for (root, error) in [
            (
                format!("{drive}{}", &root[3..]),
                "Project root must be absolute",
            ),
            (
                format!(r"\\?\{root}"),
                "Project root must be on a drive letter",
            ),
            (
                format!(r"\\localhost\{}${}", &root[..1], &root[2..]),
                "Project root must be on a drive letter",
            ),
            (format!(r"{root}\CON"), names),
            (format!(r"{root}:stream"), names),
            (format!(r"{root}."), names),
            (format!("{root}\\bad\u{1}name"), names),
        ] {
            assert_eq!(
                root_directory(&root)
                    .err()
                    .map(|refused| refused.to_string())
                    .as_deref(),
                Some(error),
                "{root}"
            );
        }

        assert_eq!(
            seed(root, r".bravebot-ui\bots\test.md").unwrap()["seeded"],
            true,
            "a backslash is a separator"
        );
        assert_eq!(
            fs::read_to_string(f.path.join(".bravebot-ui/bots/test.md")).unwrap(),
            "seed body"
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
        #[cfg(windows)]
        let root = std::path::PathBuf::from(root.to_str().unwrap().trim_start_matches(r"\\?\"));
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
        // This channel writes hooks and does not read them: a second reader of that file is a
        // second answer to what a hook is.
        assert!(call("hooks.read", "hooks.json", None, None).is_err());
        assert!(call("hooks.replace", "other.json", Some("{}"), None).is_err());
        // The spelling Windows and macOS would open as the same file is a different path here.
        assert!(call("hooks.replace", "HOOKS.JSON", Some("{}"), None).is_err());
        assert!(call("hooks.replace", "hooks.json", Some("first"), None).is_ok());
        assert!(call("hooks.replace", "hooks.json", Some("overwrite"), None).is_err());
        assert_eq!(
            fs::read_to_string(root.join("hooks.json")).unwrap(),
            "first"
        );
    }
}
