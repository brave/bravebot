//! UI-only file access. Never executes commands or sends content to the agent.
//! Every path component is opened relative to a pinned directory, without following links.
#![forbid(unsafe_code)]

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
fn replace_at(
    parent: &File,
    leaf: &str,
    text: &str,
    expected: Option<&str>,
) -> io::Result<Option<String>> {
    let previous = match read_at(parent, leaf, 65536) {
        Ok((_, true)) => return Err(invalid("File exceeds 64 KB")),
        Ok((text, false)) => Some(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if previous.as_deref() != expected {
        return Err(invalid(
            "File changed since this editor opened. Reopen it to review the latest version.",
        ));
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
        "hooks.read" | "hooks.replace" => {
            if request.path != "hooks.json" { return Err(invalid("Only the hooks file is allowed")); }
            let (parent, leaf) = parent_directory(&request.root, "hooks.json", false)?;
            if request.operation == "hooks.read" {
                return match read_at(&parent, &leaf, 65536) {
                    Ok((_, true)) => Err(invalid("Hooks exceed 64 KB")),
                    Ok((text, false)) => Ok(json!({"text": text})),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(json!({"text": null})),
                    Err(error) => Err(error),
                };
            }
            let text = request.text.ok_or_else(|| invalid("Missing hooks"))?;
            if text.len() > 65536 || text.contains('\0') { return Err(invalid("Hooks must be text under 64 KB")); }
            let previous = replace_at(&parent, &leaf, &text, request.expected.as_deref())?;
            Ok(json!({"previous": previous}))
        }
        "replace" => {
            // This channel writes only bot memory, never an arbitrary project file.
            let parts = components(Path::new(&request.path))?;
            if parts.len() != 3
                || parts[0] != ".bravebot-ui"
                || parts[1] != "bots"
                || !parts[2].ends_with(".md")
            {
                return Err(invalid("Only bot memory can be replaced"));
            }
            let text = request.text.ok_or_else(|| invalid("Missing memory"))?;
            if text.len() > 65536 || text.contains('\0') {
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
        let (parent, leaf) = parent_directory(f.root(), ".bravebot-ui/bots/test.md", true).unwrap();
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
        let call = |operation: &str, path: &str, text: Option<&str>, expected: Option<&str>| handle(Request {
            root: root.display().to_string(), path: path.into(), operation: operation.into(),
            limit: None, text: text.map(str::to_string), expected: expected.map(str::to_string),
        });
        assert!(call("hooks.read", "hooks.json", None, None).unwrap()["text"].is_null());
        assert!(call("hooks.replace", "other.json", Some("{}"), None).is_err());
        assert!(call("hooks.replace", "hooks.json", Some("first"), None).is_ok());
        assert!(call("hooks.replace", "hooks.json", Some("overwrite"), None).is_err());
        assert_eq!(fs::read_to_string(root.join("hooks.json")).unwrap(), "first");
    }
}
