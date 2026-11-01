//! The file a call of `choose_file` names, resolved before anything is sent to the extension.
//!
//! The call names the file by a `path` the planner wrote and a person reads in the question and in
//! the window. The server started for `upload-trusted` turns it into the file it leads to, inside
//! the directory `uploads` of the directory its declaration names with `--dir`, and sends the
//! extension the file's canonical path, its size and its name in place of the text. The browser
//! reads the file itself, so no byte of it passes through this program.
//!
//! The path is the planner's own word, written in a context that has met nothing untrusted, and the
//! directory is the one the person declared. No byte of a page or of a result is looked at, and a
//! file's content is never opened: the file is looked at (its type, its size and its name) and
//! nothing else. The outcomes are a request to send or one sentence of the closed list below.

use serde_json::{Map, Value};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};

/// The directory, inside the one the server was declared with, that a file has to be in.
///
/// The declared directory also holds the secret and the socket of the relay, so a file is not
/// chosen from it directly. A person puts the files the model may choose in this directory, and it
/// has to be a directory of its own and not a link, so that the directory a person granted is the
/// one that holds the file.
pub const DIRECTORY: &str = "uploads";

/// The largest file that is chosen, in bytes: 10 MiB, as the other agents that upload bound it.
pub const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// The most characters of a path, as written or as resolved. The window shows the resolved path,
/// the file's name and its size in one text of at most 2,000 characters, so the path is held to
/// less than half of that.
pub const PATH_LIMIT: usize = 900;

/// The call has no path, or it is not text.
pub const NO_PATH: &str = "choose_file needs a path, as text: the name of a file in the uploads \
     directory. Nothing was sent to the browser.";

/// The directory `uploads` of the server's directory is missing, or is not a directory of its own.
pub const NO_DIRECTORY: &str = "There is no uploads directory to choose a file from. Make a \
     directory named uploads, not a link, in the directory this server was declared with, and \
     put the file in it. Nothing was sent to the browser.";

/// The path does not lead to a regular file inside the uploads directory. One sentence says it
/// for every reason (the path does not exist, it leaves the directory, a link in it leads out, it
/// is a directory, the file has more than one hard link, its name holds a character a person
/// cannot see), so the answer does not tell what is outside the directory.
pub const NOT_A_FILE: &str = "That path is not a regular file inside the uploads directory, so \
     it was not chosen. Nothing was sent to the browser.";

/// The name of the file, or of a directory above it in the uploads directory, looks like a
/// credential.
pub const A_CREDENTIAL: &str = "The name of that file looks like a credential, a key or a secret, \
     so it is never given to a page. Nothing was sent to the browser.";

/// The file is larger than [`MAX_BYTES`].
pub const TOO_BIG: &str = "That file is larger than 10 MiB, so it was not chosen. Nothing was \
     sent to the browser.";

/// A file that was chosen: what the extension is sent in place of the planner's path.
#[derive(Debug, PartialEq, Eq)]
pub struct Chosen {
    /// The canonical absolute path, which the browser is asked to read.
    pub path: String,
    /// The size in bytes when the file was looked at.
    pub size: u64,
    /// The name of the file, the last part of `path`.
    pub name: String,
}

/// The parts of a path that are the names of credentials, as `extension/files.js` has them, which a
/// test of this crate holds equal to these. A part that is one of [`CREDENTIAL_EXACT`], starts with
/// one of [`CREDENTIAL_PREFIXES`] or ends with one of [`CREDENTIAL_SUFFIXES`] is the name of a file
/// or a directory that holds keys, tokens or passwords by the conventions of the tools that write
/// them: `.env` and `.env.*`, `.netrc`, `.npmrc`, `.pgpass`, `.git-credentials`; the directories
/// `.ssh`, `.aws` and `.gnupg`; `id_rsa*`, `id_dsa*`, `id_ecdsa*` and `id_ed25519*`;
/// `credentials*`; and `*.pem`, `*.key`, `*.p12`, `*.pfx` and `*.kdbx`.
pub const CREDENTIAL_EXACT: [&str; 8] = [
    ".env",
    ".netrc",
    ".npmrc",
    ".pgpass",
    ".git-credentials",
    ".ssh",
    ".aws",
    ".gnupg",
];
pub const CREDENTIAL_PREFIXES: [&str; 6] = [
    ".env.",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    "credentials",
];
pub const CREDENTIAL_SUFFIXES: [&str; 5] = [".pem", ".key", ".p12", ".pfx", ".kdbx"];

/// Whether one part of a path is the name of a credential, compared without regard to case.
pub fn looks_like_a_credential(part: &str) -> bool {
    let part = part.to_lowercase();
    CREDENTIAL_EXACT.contains(&part.as_str())
        || CREDENTIAL_PREFIXES
            .iter()
            .any(|prefix| part.starts_with(prefix))
        || CREDENTIAL_SUFFIXES
            .iter()
            .any(|suffix| part.ends_with(suffix))
}

/// Whether any normal part of `path` looks like a credential. A part that is not text is not a
/// name this list can speak for, so the path is refused with the other paths that cannot be shown.
fn names_a_credential(path: &Path) -> bool {
    path.components().any(|part| match part {
        Component::Normal(part) => part.to_str().is_some_and(looks_like_a_credential),
        _ => false,
    })
}

/// Whether `c` is a character a person cannot see in the question or the window: a control (Cc), a
/// format character (Cf), the line separator (Zl), the paragraph separator (Zp) or a private-use
/// character (Co). `hasUnseen` in `extension/files.js` is the same set, and a test reads one list
/// of file names with the result for each in both languages. The ranges are those of Unicode 17.
fn is_unseen(c: char) -> bool {
    matches!(
        c,
        '\u{0}'..='\u{1f}'
            | '\u{7f}'..='\u{9f}'
            | '\u{ad}'
            | '\u{600}'..='\u{605}'
            | '\u{61c}'
            | '\u{6dd}'
            | '\u{70f}'
            | '\u{890}'..='\u{891}'
            | '\u{8e2}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{e000}'..='\u{f8ff}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{110bd}'
            | '\u{110cd}'
            | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0001}'
            | '\u{e0020}'..='\u{e007f}'
            | '\u{f0000}'..='\u{ffffd}'
            | '\u{100000}'..='\u{10fffd}'
    )
}

/// The path of `file` and its name as the text the extension is sent. A path or a name that is not
/// text, a path past [`PATH_LIMIT`] and a path with a character a person cannot see get the sentence
/// of a path that is not a file. The bytes that are not text are not replaced with U+FFFD, since
/// the name that results can be that of another file in the directory.
fn as_text(file: &Path) -> Result<(&str, &str), &'static str> {
    let (Some(path), Some(name)) = (
        file.to_str(),
        file.file_name().and_then(std::ffi::OsStr::to_str),
    ) else {
        return Err(NOT_A_FILE);
    };
    if path.chars().count() > PATH_LIMIT || path.chars().any(is_unseen) {
        return Err(NOT_A_FILE);
    }
    Ok((path, name))
}

/// The file `typed` names, looked at from `directory`, the directory the server was declared with.
///
/// A relative path is taken from the uploads directory, which is the only place a file can be, and
/// an absolute path has to lead into it. Either way the file is the one the path leads to with
/// every link followed, and it has to be inside the uploads directory with every link followed, a
/// regular file with one hard link, within [`MAX_BYTES`], and not named like a credential. A
/// relative path is looked at as written, every part of it, and an absolute one by its last part,
/// before the file system is asked about it, so a name that looks like a credential is refused
/// whether or not the file exists. The parts of the path the file leads to below the uploads
/// directory are looked at too.
pub fn choose(directory: &Path, typed: &str) -> Result<Chosen, &'static str> {
    if typed.is_empty() || typed.chars().count() > PATH_LIMIT {
        return Err(NO_PATH);
    }
    let uploads = directory.join(DIRECTORY);
    let written = Path::new(typed);
    let named = if written.is_absolute() {
        written.file_name().map(Path::new).unwrap_or(written)
    } else {
        written
    };
    if names_a_credential(named) {
        return Err(A_CREDENTIAL);
    }
    let home = match std::fs::symlink_metadata(&uploads) {
        Ok(metadata) if metadata.is_dir() => {
            std::fs::canonicalize(&uploads).map_err(|_| NO_DIRECTORY)?
        }
        _ => return Err(NO_DIRECTORY),
    };
    let file = std::fs::canonicalize(uploads.join(written)).map_err(|_| NOT_A_FILE)?;
    let Ok(inside) = file.strip_prefix(&home) else {
        return Err(NOT_A_FILE);
    };
    if names_a_credential(inside) {
        return Err(A_CREDENTIAL);
    }
    let metadata = std::fs::metadata(&file).map_err(|_| NOT_A_FILE)?;
    // A file with more than one hard link is another name for a file that may be anywhere, and a
    // name here says nothing about what it is, so it is not looked at as a file of this directory.
    if !metadata.is_file() || metadata.nlink() > 1 {
        return Err(NOT_A_FILE);
    }
    if metadata.len() > MAX_BYTES {
        return Err(TOO_BIG);
    }
    let (path, name) = as_text(&file)?;
    Ok(Chosen {
        path: path.to_string(),
        size: metadata.len(),
        name: name.to_string(),
    })
}

/// The arguments of a call of `choose_file` with the file it names in place of the planner's path:
/// every argument as it was, except that `path` is the canonical path of the file and `size` and
/// `file_name` are the size and the name of that file. A `size` or a `file_name` the planner wrote
/// is replaced, so what the extension reads of these is what was looked at here.
pub fn with_file(directory: &Path, arguments: &Value) -> Result<Value, &'static str> {
    let Some(arguments) = arguments.as_object() else {
        return Err(NO_PATH);
    };
    let Some(typed) = arguments.get("path").and_then(Value::as_str) else {
        return Err(NO_PATH);
    };
    let chosen = choose(directory, typed)?;
    let mut sent: Map<String, Value> = arguments.clone();
    sent.insert("path".into(), Value::String(chosen.path));
    sent.insert("size".into(), Value::from(chosen.size));
    sent.insert("file_name".into(), Value::String(chosen.name));
    Ok(Value::Object(sent))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::symlink;

    /// A server directory holding an `uploads` directory, and a file `outside` beside it.
    fn declared() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join(DIRECTORY)).unwrap();
        std::fs::write(directory.path().join("outside.txt"), b"not for a page").unwrap();
        directory
    }

    fn put(directory: &Path, name: &str, bytes: &[u8]) {
        let path = directory.join(DIRECTORY).join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// A file in the uploads directory is chosen by a relative name or by its absolute path, and the
    /// extension is sent its canonical path, its size and its name. A server that sent the path as
    /// the planner wrote it would send `..` and links as they are.
    #[test]
    fn a_file_in_the_uploads_directory_is_resolved_to_its_canonical_path_size_and_name() {
        let directory = declared();
        put(directory.path(), "report.txt", b"twelve bytes");
        let canonical = std::fs::canonicalize(directory.path())
            .unwrap()
            .join(DIRECTORY)
            .join("report.txt");
        let expected = Chosen {
            path: canonical.to_str().unwrap().into(),
            size: 12,
            name: "report.txt".into(),
        };
        assert_eq!(choose(directory.path(), "report.txt"), Ok(expected));
        let absolute = directory.path().join(DIRECTORY).join("report.txt");
        let chosen = choose(directory.path(), absolute.to_str().unwrap()).unwrap();
        assert_eq!(chosen.path, canonical.to_str().unwrap());
        let roundabout = choose(directory.path(), "./sub/../report.txt");
        assert_eq!(roundabout, Err(NOT_A_FILE), "sub does not exist");
        put(directory.path(), "sub/inner.txt", b"x");
        let roundabout = choose(directory.path(), "sub/../report.txt").unwrap();
        assert_eq!(roundabout.path, canonical.to_str().unwrap());
    }

    /// A path that leaves the uploads directory is refused whatever the way out, and with the one
    /// sentence a path that does not exist gets, so the answer says nothing about what is outside.
    /// A server that checked the text of the path and not the file it leads to would let `..` out.
    #[test]
    fn a_path_that_leaves_the_uploads_directory_is_refused_with_the_sentence_of_a_missing_file() {
        let directory = declared();
        put(directory.path(), "report.txt", b"x");
        let outside = directory.path().join("outside.txt");
        for typed in [
            "../outside.txt",
            "sub/../../outside.txt",
            outside.to_str().unwrap(),
            "/etc/hosts",
            "/",
            "..",
            "missing.txt",
        ] {
            assert_eq!(
                choose(directory.path(), typed),
                Err(NOT_A_FILE),
                "{typed:?}"
            );
        }
    }

    /// A link inside the directory that leads out of it is refused, and a link that leads to
    /// another file inside it is the file it leads to. A server that looked at the link's own type
    /// and not at where it leads would hand the browser a file the person never put there.
    #[test]
    fn a_link_that_leaves_the_directory_is_refused_and_one_inside_it_is_followed() {
        let directory = declared();
        put(directory.path(), "report.txt", b"abc");
        let uploads = directory.path().join(DIRECTORY);
        symlink(
            directory.path().join("outside.txt"),
            uploads.join("out.txt"),
        )
        .unwrap();
        symlink("report.txt", uploads.join("alias.txt")).unwrap();
        symlink(directory.path(), uploads.join("up")).unwrap();
        assert_eq!(choose(directory.path(), "out.txt"), Err(NOT_A_FILE));
        assert_eq!(choose(directory.path(), "up/outside.txt"), Err(NOT_A_FILE));
        let followed = choose(directory.path(), "alias.txt").unwrap();
        assert_eq!(followed.name, "report.txt");
        assert_eq!(followed.size, 3);
    }

    /// A directory, and the uploads directory itself, are not files.
    #[test]
    fn a_directory_is_not_a_file() {
        let directory = declared();
        put(directory.path(), "sub/inner.txt", b"x");
        for typed in ["sub", ".", "./"] {
            assert_eq!(
                choose(directory.path(), typed),
                Err(NOT_A_FILE),
                "{typed:?}"
            );
        }
    }

    /// A file of 10 MiB is chosen and one byte more is refused. The bound is on the file's size
    /// and not on a count of something else.
    #[test]
    fn a_file_over_ten_mebibytes_is_refused_and_one_of_ten_is_chosen() {
        let directory = declared();
        let uploads = directory.path().join(DIRECTORY);
        let exact = std::fs::File::create(uploads.join("exact.bin")).unwrap();
        exact.set_len(MAX_BYTES).unwrap();
        let over = std::fs::File::create(uploads.join("over.bin")).unwrap();
        over.set_len(MAX_BYTES + 1).unwrap();
        assert_eq!(
            choose(directory.path(), "exact.bin").unwrap().size,
            10 << 20
        );
        assert_eq!(choose(directory.path(), "over.bin"), Err(TOO_BIG));
        assert_eq!(MAX_BYTES, 10_485_760);
    }

    /// A name that looks like a credential is refused as one, by the name as written and by the
    /// name of the file a link leads to, and the file system is not asked about a name written
    /// that way: `/etc/id_rsa` is a credential whether or not it exists.
    #[test]
    fn a_name_that_looks_like_a_credential_is_refused_as_written_and_as_resolved() {
        let directory = declared();
        for name in [
            ".env",
            ".ENV",
            ".env.local",
            "server.pem",
            "SERVER.PEM",
            "id_rsa",
            "id_rsa.pub",
            "id_ed25519",
            "api.key",
            "credentials",
            "credentials.json",
            ".netrc",
            ".npmrc",
            "vault.kdbx",
            "cert.p12",
            "cert.pfx",
            ".git-credentials",
            ".pgpass",
            ".ssh/config",
            ".aws/config",
            ".gnupg/pubring.kbx",
            "keys/id_rsa",
        ] {
            put(directory.path(), name, b"secret");
            assert_eq!(
                choose(directory.path(), name),
                Err(A_CREDENTIAL),
                "{name:?}"
            );
        }
        for typed in ["/etc/id_rsa", "../.env", "nowhere/x.pem", "nowhere/.ssh/x"] {
            assert_eq!(
                choose(directory.path(), typed),
                Err(A_CREDENTIAL),
                "{typed:?}"
            );
        }
        // A harmless name that is a link to one is refused by where it leads.
        let uploads = directory.path().join(DIRECTORY);
        symlink(".env", uploads.join("notes.txt")).unwrap();
        assert_eq!(choose(directory.path(), "notes.txt"), Err(A_CREDENTIAL));
        // Names that only share letters with the list are files.
        for name in [
            "environment.txt",
            "keyboard.txt",
            "monkey.txt",
            "pem.txt",
            "id.txt",
        ] {
            put(directory.path(), name, b"plain");
            assert!(choose(directory.path(), name).is_ok(), "{name:?}");
        }
    }

    /// Only the parts of the path inside the uploads directory are looked at for a credential, and
    /// the last part of an absolute path as it is written: a declared directory that is itself
    /// under a directory named `.aws` is still a place to choose from.
    #[test]
    fn the_directories_above_the_uploads_directory_are_not_looked_at_for_a_credential() {
        let outer = tempfile::tempdir().unwrap();
        let directory = outer.path().join(".aws").join("relay");
        std::fs::create_dir_all(directory.join(DIRECTORY)).unwrap();
        std::fs::write(directory.join(DIRECTORY).join("a.txt"), b"fine").unwrap();
        assert!(choose(&directory, "a.txt").is_ok());
        let canonical = std::fs::canonicalize(&directory).unwrap().join(DIRECTORY);
        let absolute = canonical.join("a.txt");
        assert!(choose(&directory, absolute.to_str().unwrap()).is_ok());
    }

    /// The names of `tests/file-names.json` get the result the list gives each. `extension/tests/
    /// upload-trusted.test.mjs` reads the same list and holds `problemWithFile` of `files.js` to the
    /// same results, so the relay and the extension agree on which names are files, which are
    /// refused as a path that is not a file, and which are credentials, including for the
    /// directories above `uploads`. A relay that counted only controls as unseen, or looked for a
    /// credential in the directories above `uploads`, refuses and accepts differently from the
    /// extension and fails here.
    #[test]
    fn the_names_of_the_list_both_halves_read_get_the_result_the_list_gives() {
        let list: Value = serde_json::from_str(include_str!("../tests/file-names.json")).unwrap();
        let files = list["files"].as_array().unwrap();
        assert!(files.len() > 100, "the list is {} names long", files.len());
        for entry in files {
            let (above, file, result) = (
                entry["above"].as_str().unwrap(),
                entry["file"].as_str().unwrap(),
                entry["result"].as_str().unwrap(),
            );
            let outer = tempfile::tempdir().unwrap();
            let directory = outer.path().join(above);
            std::fs::create_dir_all(directory.join(DIRECTORY)).unwrap();
            put(&directory, file, b"x");
            let expected = match result {
                "chosen" => Ok(()),
                "refused" => Err(NOT_A_FILE),
                "credential" => Err(A_CREDENTIAL),
                other => panic!("{other}: not a result"),
            };
            assert_eq!(
                choose(&directory, file).map(|_| ()),
                expected,
                "{above:?} {file:?}: {}",
                entry["note"]
            );
        }
    }

    /// A character is unseen for exactly the code points of the ranges in `tests/file-names.json`,
    /// which `extension/tests/upload-trusted.test.mjs` holds equal to what the JavaScript engine
    /// takes `hasUnseen` of `files.js` to be. A table that lost a range lets a character through
    /// that the extension refuses, and one with a range too many refuses a name it accepts.
    #[test]
    fn a_character_is_unseen_for_exactly_the_code_points_of_the_list_both_halves_read() {
        let list: Value = serde_json::from_str(include_str!("../tests/file-names.json")).unwrap();
        let ranges: Vec<(u32, u32)> = list["unseen_ranges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|range| {
                (
                    range[0].as_u64().unwrap() as u32,
                    range[1].as_u64().unwrap() as u32,
                )
            })
            .collect();
        for point in 0..=0x10_FFFF_u32 {
            let Some(c) = char::from_u32(point) else {
                continue;
            };
            let listed = ranges
                .iter()
                .any(|(first, last)| (*first..=*last).contains(&point));
            assert_eq!(is_unseen(c), listed, "U+{point:04X}");
        }
    }

    /// A file with more than one hard link is refused with the sentence of a path that is not a
    /// file, whichever of its names is given, and it is chosen again once it has one. Its other
    /// name can be anywhere, so the name in `uploads` says nothing about the file. A server that
    /// looked at the name only would hand the browser a credential that was linked in under a
    /// harmless name. A link that points at a file with one hard link is still followed.
    #[test]
    fn a_file_with_another_hard_link_is_refused_and_a_symbolic_link_to_a_single_one_is_followed() {
        let directory = declared();
        let uploads = directory.path().join(DIRECTORY);
        put(directory.path(), "report.txt", b"abc");
        std::fs::hard_link(uploads.join("report.txt"), uploads.join("copy.txt")).unwrap();
        std::fs::hard_link(directory.path().join("outside.txt"), uploads.join("in.txt")).unwrap();
        for typed in ["report.txt", "copy.txt", "in.txt"] {
            assert_eq!(choose(directory.path(), typed), Err(NOT_A_FILE), "{typed}");
        }
        symlink("report.txt", uploads.join("alias.txt")).unwrap();
        assert_eq!(choose(directory.path(), "alias.txt"), Err(NOT_A_FILE));
        std::fs::remove_file(uploads.join("copy.txt")).unwrap();
        assert_eq!(choose(directory.path(), "report.txt").unwrap().size, 3);
        assert_eq!(choose(directory.path(), "alias.txt").unwrap().size, 3);
        std::fs::remove_file(directory.path().join("outside.txt")).unwrap();
        assert_eq!(choose(directory.path(), "in.txt").unwrap().size, 14);
    }

    /// A server directory with no `uploads`, one that is a file, and one that is a link are each
    /// refused with the sentence that says to make the directory. A link would let the directory
    /// that is granted be one the person never named in the declaration.
    #[test]
    fn an_uploads_directory_that_is_missing_a_file_or_a_link_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(choose(directory.path(), "a.txt"), Err(NO_DIRECTORY));
        std::fs::write(directory.path().join(DIRECTORY), b"x").unwrap();
        assert_eq!(choose(directory.path(), "a.txt"), Err(NO_DIRECTORY));
        std::fs::remove_file(directory.path().join(DIRECTORY)).unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(elsewhere.path().join("a.txt"), b"x").unwrap();
        symlink(elsewhere.path(), directory.path().join(DIRECTORY)).unwrap();
        assert_eq!(choose(directory.path(), "a.txt"), Err(NO_DIRECTORY));
    }

    /// A link can lead to a file whose name is not UTF-8. It is refused, and a lossy rendering of
    /// that name would be the name of a different file, which is chosen only by its own name.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_name_that_is_not_utf8_is_refused_and_is_not_its_lossy_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let directory = declared();
        let uploads = directory.path().join(DIRECTORY);
        let invalid = uploads.join(OsStr::from_bytes(b"report-\xff.txt"));
        std::fs::write(&invalid, b"not utf-8").unwrap();
        put(directory.path(), "report-\u{FFFD}.txt", b"lookalike");
        symlink(&invalid, uploads.join("bad.txt")).unwrap();
        assert_eq!(choose(directory.path(), "bad.txt"), Err(NOT_A_FILE));
        let chosen = choose(directory.path(), "report-\u{FFFD}.txt").unwrap();
        assert_eq!(chosen.name, "report-\u{FFFD}.txt");
        assert_eq!(chosen.size, 9);
    }

    /// A path that is empty, too long, not text or holds a character a person cannot see is
    /// refused, and the arguments that carry it are checked too.
    #[test]
    fn a_missing_empty_or_overlong_path_is_refused() {
        let directory = declared();
        put(directory.path(), "a.txt", b"x");
        assert_eq!(choose(directory.path(), ""), Err(NO_PATH));
        assert_eq!(
            choose(directory.path(), &"a".repeat(PATH_LIMIT + 1)),
            Err(NO_PATH)
        );
        assert_eq!(choose(directory.path(), "a\0.txt"), Err(NOT_A_FILE));
        put(directory.path(), "line\nbreak.txt", b"x");
        assert_eq!(choose(directory.path(), "line\nbreak.txt"), Err(NOT_A_FILE));
        for arguments in [
            json!({}),
            json!({"path": 7}),
            json!({"path": null}),
            json!([]),
            json!("a.txt"),
        ] {
            assert_eq!(
                with_file(directory.path(), &arguments),
                Err(NO_PATH),
                "{arguments}"
            );
        }
    }

    /// A path or a name that is not text is refused with the sentence of a path that is not a file,
    /// and the path that its rendering with U+FFFD spells is text and is sent as it is. A relay that
    /// rendered the bytes that are not text would name the second file for the first, which can be a
    /// different file of the directory. The names are made in memory, since the file system of a Mac
    /// cannot hold the first. The next test reaches the same refusal through `choose` on Linux.
    #[test]
    fn a_path_or_name_that_is_not_text_is_refused_and_the_one_spelled_like_it_is_sent_as_itself() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let invalid_name = Path::new(OsStr::from_bytes(b"/uploads/report-\xff.txt"));
        let invalid_directory = Path::new(OsStr::from_bytes(b"/uploads/\xff/report.txt"));
        let lookalike_name = Path::new("/uploads/report-\u{FFFD}.txt");
        let lookalike_directory = Path::new("/uploads/\u{FFFD}/report.txt");
        assert_eq!(as_text(invalid_name), Err(NOT_A_FILE));
        assert_eq!(as_text(invalid_directory), Err(NOT_A_FILE));
        assert_eq!(
            as_text(lookalike_name),
            Ok(("/uploads/report-\u{FFFD}.txt", "report-\u{FFFD}.txt"))
        );
        assert_eq!(
            as_text(lookalike_directory),
            Ok(("/uploads/\u{FFFD}/report.txt", "report.txt"))
        );
    }

    /// The arguments sent on are the planner's with the path replaced by the canonical one and the
    /// size and the name added from the file, and a size or a name the planner wrote is not sent.
    /// A server that kept the planner's `size` would let it say 1 for a file of 10 MiB.
    #[test]
    fn the_request_carries_the_files_own_path_size_and_name_and_none_the_planner_wrote() {
        let directory = declared();
        put(directory.path(), "report.txt", b"twelve bytes");
        let canonical = std::fs::canonicalize(directory.path())
            .unwrap()
            .join(DIRECTORY)
            .join("report.txt");
        let sent = with_file(
            directory.path(),
            &json!({
                "url": "http://127.0.0.1/upload",
                "role": "button",
                "name": "Resume",
                "path": "report.txt",
                "size": 1,
                "file_name": "innocent.txt",
            }),
        )
        .unwrap();
        assert_eq!(
            sent,
            json!({
                "url": "http://127.0.0.1/upload",
                "role": "button",
                "name": "Resume",
                "path": canonical.to_str().unwrap(),
                "size": 12,
                "file_name": "report.txt",
            })
        );
    }
}
