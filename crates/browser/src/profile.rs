//! A Brave profile of its own for the extension, so that a session can use a browser that holds
//! nothing of a person's signed-in profile.
//!
//! The profile is a user data directory that `profile` makes and marks, and the command it builds
//! starts Brave on that directory with the extension loaded unpacked. Brave reads the host manifest
//! `install` writes from one place whatever user data directory it runs on, so the profile has no
//! manifest of its own and its extension reaches the relay `install` set up.
//!
//! Nothing here starts a process. Only `bravebot-sandbox` starts programs (SANDBOX-30) and this
//! crate links no workspace crate, so the command is printed for the person to run in their own
//! terminal. No tool offers it and nothing in BraveBot runs it.

use crate::paths;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The file that says `profile` made this directory.
pub const MARKER: &str = "bravebot-profile";

/// What the marker says, for a person who finds it.
const MARKER_TEXT: &str = "A Brave profile made by bravebot-browser profile, for the BraveBot \
                           extension. It is not a profile of yours.\n";

/// What Brave creates in a user data directory the first time it runs on it. A directory that holds
/// either and no marker belongs to a Brave someone has used.
const HELD_BY_A_BROWSER: [&str; 2] = ["Default", "Local State"];

/// The link Brave keeps in a user data directory while it runs on it.
pub const SINGLETON_LOCK: &str = "SingletonLock";

/// Where Brave's stable channel is on macOS.
pub const MACOS_BRAVE: &str = "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser";

/// The names Brave's stable channel has on `PATH` elsewhere, in the order they are preferred.
const LINUX_NAMES: [&str; 2] = ["brave-browser", "brave"];

/// The file in the extension's directory that makes it one Brave can load.
const EXTENSION_MANIFEST: &str = "manifest.json";

fn io_error(message: String) -> io::Error {
    io::Error::other(message)
}

/// Whether anything is at `path`, a dangling link included.
fn present(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Whether `profile` carries the marker as a file of its own, which a link in its place does not.
fn is_marked(profile: &Path) -> bool {
    std::fs::symlink_metadata(profile.join(MARKER)).is_ok_and(|marker| marker.is_file())
}

/// Refuses a directory that is not one this command may use: a link, something that is not a
/// directory, or a directory with no marker that holds what a Brave leaves behind. The extension is
/// not loaded into the profile of a browser someone uses.
pub fn refuse_foreign(profile: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(profile) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(io_error(format!(
                "{} is a link, and a profile has to be a directory of its own",
                profile.display()
            )));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(io_error(format!(
                "{} is not a directory",
                profile.display()
            )));
        }
        _ => {}
    }
    if is_marked(profile) {
        return Ok(());
    }
    match HELD_BY_A_BROWSER
        .iter()
        .find(|held| present(&profile.join(held)))
    {
        Some(held) => Err(io_error(format!(
            "{} holds {held}, so it is a profile that bravebot-browser profile did not make; name a \
             directory that is empty or does not exist",
            profile.display()
        ))),
        None => Ok(()),
    }
}

/// Makes `profile` if it is not there, narrows it to this account, and writes the marker that says
/// this command made it, unless the marker is there already.
fn mark(profile: &Path) -> io::Result<()> {
    paths::prepare(profile)?;
    if is_marked(profile) {
        return Ok(());
    }
    paths::write_private(profile, MARKER, MARKER_TEXT.as_bytes())
}

/// What the words after `profile` ask for.
#[derive(Debug, PartialEq, Eq)]
pub struct Arguments<'a> {
    /// The user data directory to start Brave on, which is made if it is not there.
    pub profile: &'a OsStr,
    /// The directory of the extension, to load unpacked.
    pub extension: &'a OsStr,
    /// The Brave to start, where it is not the one found.
    pub brave: Option<&'a OsStr>,
}

/// Reads the words after `profile`, or `None` for words it does not take. The profile and the
/// extension are both needed, and each flag is given once and takes the word after it, whatever
/// that word starts with. A value is kept as the bytes it is, since a directory whose name is not
/// UTF-8 is still the directory Brave is to be started on.
pub fn arguments<'a>(words: &[&'a OsStr]) -> Option<Arguments<'a>> {
    let (mut profile, mut extension, mut brave) = (None, None, None);
    let mut rest = words;
    while let [flag, value, tail @ ..] = rest {
        let slot = match flag.to_str() {
            Some("--profile-dir") => &mut profile,
            Some("--extension-dir") => &mut extension,
            Some("--brave") => &mut brave,
            _ => return None,
        };
        if slot.replace(*value).is_some() {
            return None;
        }
        rest = tail;
    }
    if !rest.is_empty() {
        return None;
    }
    Some(Arguments {
        profile: profile?,
        extension: extension?,
        brave,
    })
}

/// Whether `path` is a file anyone could run.
fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

/// The Brave to start. The one named is used as it is. Otherwise it is `app`, where the platform
/// installs it, or on a platform with no `app` the first of [`LINUX_NAMES`] found in an absolute
/// entry of `path`. Nothing else is looked at.
fn find_brave(
    explicit: Option<&Path>,
    app: Option<&Path>,
    path: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(explicit) = explicit {
        return std::path::absolute(explicit)
            .ok()
            .filter(|explicit| explicit.is_file());
    }
    if let Some(app) = app {
        return Some(app.to_path_buf()).filter(|app| app.is_file());
    }
    let directories: Vec<PathBuf> = std::env::split_paths(path?)
        .filter(|directory| directory.is_absolute())
        .collect();
    LINUX_NAMES.iter().find_map(|name| {
        directories
            .iter()
            .map(|directory| directory.join(name))
            .find(|candidate| is_executable(candidate))
    })
}

/// The directory of the extension as an absolute path, if Brave can load it from there. Brave reads
/// `--load-extension` as a list separated by commas, so a path holding one would load two others.
fn extension_directory(directory: &Path) -> io::Result<PathBuf> {
    let directory = std::path::absolute(directory)?;
    if directory.as_os_str().as_bytes().contains(&b',') {
        return Err(io_error(format!(
            "{} has a comma in its name, which Brave reads as the end of one extension and the \
             start of another; name a directory without one",
            directory.display()
        )));
    }
    if !directory.join(EXTENSION_MANIFEST).is_file() {
        return Err(io_error(format!(
            "{} holds no {EXTENSION_MANIFEST}; name the directory the extension is in",
            directory.display()
        )));
    }
    Ok(directory)
}

/// A word that is `name` followed by `value`, as Brave reads a switch.
fn switch(name: &str, value: &Path) -> OsString {
    let mut word = OsString::from(name);
    word.push(value);
    word
}

/// The words of the command that starts `brave` on the user data directory `profile` with the
/// extension in `extension` loaded. The switches carry their values after an `=`, so a path that
/// starts with a dash or holds a space is still one word. `--no-first-run` skips the first-run
/// tasks of Brave, among them the offer to import another browser's data into the new profile.
pub fn command(brave: &Path, profile: &Path, extension: &Path) -> Vec<OsString> {
    vec![
        brave.as_os_str().to_os_string(),
        switch("--user-data-dir=", profile),
        switch("--load-extension=", extension),
        OsString::from("--no-first-run"),
        OsString::from("--no-default-browser-check"),
    ]
}

/// `word` as one word of a POSIX shell. A word of letters, digits and `_ - . / : , + @ =` is left
/// as it is. Any other is put in single quotes, with each `'` in it written `'\''`, so a space, a
/// quote, `$`, a backtick or a `~` at the start reaches the command as part of the word. The bytes
/// are copied as they are, since a name that is not UTF-8 is still the name the shell is given.
fn quoted(word: &OsStr) -> Vec<u8> {
    let plain = |byte: &u8| byte.is_ascii_alphanumeric() || b"_-./:,+@=".contains(byte);
    let bytes = word.as_bytes();
    if !bytes.is_empty() && bytes.iter().all(plain) {
        return bytes.to_vec();
    }
    let mut quoted = vec![b'\''];
    for (at, part) in bytes.split(|byte| *byte == b'\'').enumerate() {
        if at > 0 {
            quoted.extend_from_slice(b"'\\''");
        }
        quoted.extend_from_slice(part);
    }
    quoted.push(b'\'');
    quoted
}

/// `words` as one line to paste into a POSIX shell, each word quoted as [`quoted`] says.
pub fn line(words: &[OsString]) -> OsString {
    let quoted: Vec<Vec<u8>> = words.iter().map(|word| quoted(word)).collect();
    OsString::from_vec(quoted.join(&b' '))
}

/// The command that starts Brave on a profile, and whether a Brave may be running on it.
pub struct Start {
    /// The line to run in a POSIX shell.
    pub line: OsString,
    /// Whether the profile holds the link a running Brave keeps. A Brave that stopped without
    /// removing it leaves it, so this says "may".
    pub may_be_running: bool,
}

/// Builds the command for `asked` and makes the profile, or says why there is no command: the
/// directory holds a profile this command did not make, the extension's directory cannot be loaded,
/// or no Brave is found. Nothing is written for any of those. `path` is the `PATH` the Brave is
/// looked for on.
pub fn start_command(asked: &Arguments<'_>, path: Option<&OsStr>) -> io::Result<Start> {
    let profile = std::path::absolute(asked.profile)?;
    refuse_foreign(&profile)?;
    let extension = extension_directory(Path::new(asked.extension))?;
    let explicit = asked.brave.map(Path::new);
    let app = cfg!(target_os = "macos").then(|| Path::new(MACOS_BRAVE));
    let Some(brave) = find_brave(explicit, app, path) else {
        return Err(io_error(match asked.brave {
            Some(named) => format!(
                "{} is not a file; name the Brave to start with --brave",
                Path::new(named).display()
            ),
            None => format!(
                "no Brave was found, at {MACOS_BRAVE} on macOS or named {} on PATH elsewhere; name \
                 it with --brave <path>",
                LINUX_NAMES.join(" or ")
            ),
        }));
    };
    let may_be_running = present(&profile.join(SINGLETON_LOCK));
    mark(&profile)?;
    Ok(Start {
        line: line(&command(&brave, &profile, &extension)),
        may_be_running,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// A file Brave could be, executable or not.
    fn file(path: &Path, executable: bool) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "#!/bin/sh\n").unwrap();
        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    /// Each switch is one word with its value after an `=`, so a name with a space, or one that
    /// starts with a dash, reaches Brave whole, and the name of a directory that is not UTF-8 is not
    /// merged with the one its replacement characters spell.
    #[test]
    fn the_command_is_one_word_for_the_program_and_each_switch() {
        let words = command(
            Path::new("/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"),
            Path::new("/p/-profile dir"),
            Path::new("/e/ext dir"),
        );
        let expected: Vec<&OsStr> = [
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "--user-data-dir=/p/-profile dir",
            "--load-extension=/e/ext dir",
            "--no-first-run",
            "--no-default-browser-check",
        ]
        .iter()
        .map(OsStr::new)
        .collect();
        assert_eq!(words, expected);

        let invalid = Path::new(OsStr::from_bytes(b"/p/dir-\xff"));
        let lookalike = Path::new("/p/dir-\u{FFFD}");
        let of = |profile| command(Path::new("/b"), profile, Path::new("/e"));
        assert_eq!(
            of(invalid)[1].as_bytes(),
            b"--user-data-dir=/p/dir-\xff".as_slice()
        );
        assert_ne!(of(invalid), of(lookalike));
    }

    /// A word of ordinary characters is left alone, and any other is one single-quoted word, with
    /// a quote of its own closed around, so the line is the same words to a shell. The words of the
    /// shell test in `tests/clean_profile.rs` run these through `sh`.
    #[test]
    fn a_word_is_quoted_only_where_a_shell_would_split_or_expand_it() {
        let of = |word: &str| quoted(OsStr::new(word));
        assert_eq!(
            of("--user-data-dir=/a/b_c-d.e:f,g+h@i"),
            b"--user-data-dir=/a/b_c-d.e:f,g+h@i"
        );
        assert_eq!(of("a b"), b"'a b'");
        assert_eq!(of("it's"), b"'it'\\''s'");
        assert_eq!(of("$HOME"), b"'$HOME'");
        assert_eq!(of("~"), b"'~'");
        assert_eq!(of(""), b"''");
        assert_eq!(of("a*"), b"'a*'");
        let invalid = OsStr::from_bytes(b"a\xff b");
        assert_eq!(quoted(invalid), b"'a\xff b'");
        assert_ne!(quoted(invalid), of("a\u{FFFD} b"));
    }

    /// A directory is one this command may use when it is empty, absent, or marked, and not when it
    /// holds what a Brave leaves behind with no marker, since the extension and a manifest would
    /// then go into a profile someone uses.
    #[test]
    fn a_directory_holding_a_profile_this_command_did_not_make_is_refused() {
        let scratch = directory();
        let at = |name: &str| scratch.path().join(name);

        assert!(refuse_foreign(&at("absent")).is_ok());

        std::fs::create_dir(at("empty")).unwrap();
        assert!(refuse_foreign(&at("empty")).is_ok());

        std::fs::create_dir_all(at("used/Default")).unwrap();
        let said = refuse_foreign(&at("used")).unwrap_err().to_string();
        assert!(said.contains("Default"), "{said}");

        std::fs::create_dir(at("settings")).unwrap();
        std::fs::write(at("settings/Local State"), "{}").unwrap();
        assert!(refuse_foreign(&at("settings")).is_err());

        std::fs::create_dir_all(at("made/Default")).unwrap();
        std::fs::write(at("made").join(MARKER), "").unwrap();
        assert!(refuse_foreign(&at("made")).is_ok());

        std::os::unix::fs::symlink(at("empty"), at("link")).unwrap();
        let said = refuse_foreign(&at("link")).unwrap_err().to_string();
        assert!(said.contains("is a link"), "{said}");
        std::fs::write(at("file"), "").unwrap();
        let said = refuse_foreign(&at("file")).unwrap_err().to_string();
        assert!(said.contains("is not a directory"), "{said}");
    }

    /// A marker that is a link to a file is not the marker, so a profile cannot be marked by
    /// pointing at one.
    #[test]
    fn a_link_in_place_of_the_marker_does_not_mark_a_profile() {
        let scratch = directory();
        let profile = scratch.path().join("used");
        std::fs::create_dir_all(profile.join("Default")).unwrap();
        std::fs::write(scratch.path().join("elsewhere"), "").unwrap();
        std::os::unix::fs::symlink(scratch.path().join("elsewhere"), profile.join(MARKER)).unwrap();
        assert!(refuse_foreign(&profile).is_err());
    }

    /// With no `--brave`, macOS has the one app path, and elsewhere the first of the two names
    /// found in an absolute entry of `PATH`: `brave-browser` in a later entry before `brave` in
    /// an earlier one. A file that is not executable, a directory, and a relative entry are not it.
    #[test]
    fn brave_is_found_at_the_places_each_platform_keeps_it_and_nowhere_else() {
        let scratch = directory();
        let at = |name: &str| scratch.path().join(name);
        file(&at("first/brave"), true);
        file(&at("second/brave-browser"), true);
        let path = std::env::join_paths([at("first"), at("second")]).unwrap();
        assert_eq!(
            find_brave(None, None, Some(&path)),
            Some(at("second/brave-browser"))
        );

        file(&at("plain/brave-browser"), false);
        std::fs::create_dir_all(at("dir/brave")).unwrap();
        let path = std::env::join_paths([at("plain"), at("dir")]).unwrap();
        assert_eq!(find_brave(None, None, Some(&path)), None);

        // An entry that reaches an executable `brave` by a relative path, which a shell would run.
        file(&at("near/brave"), true);
        let up = "../".repeat(std::env::current_dir().unwrap().components().count());
        let relative = PathBuf::from(format!(
            "{up}{}",
            at("near").strip_prefix("/").unwrap().display()
        ));
        assert!(relative.join("brave").is_file());
        let path = std::env::join_paths([relative.as_path(), Path::new("")]).unwrap();
        assert_eq!(find_brave(None, None, Some(&path)), None);
        assert_eq!(find_brave(None, None, None), None);

        file(&at("App/Brave Browser"), true);
        let app = at("App/Brave Browser");
        let path = std::env::join_paths([at("first")]).unwrap();
        assert_eq!(find_brave(None, Some(&app), Some(&path)), Some(app.clone()));
        assert_eq!(find_brave(None, Some(&at("App/absent")), Some(&path)), None);
    }

    /// A Brave named with `--brave` is used wherever one would have been found, if it is a file.
    #[test]
    fn a_brave_that_is_named_is_the_one_used() {
        let scratch = directory();
        let at = |name: &str| scratch.path().join(name);
        file(&at("first/brave"), true);
        file(&at("mine/brave beta"), false);
        let path = std::env::join_paths([at("first")]).unwrap();
        assert_eq!(
            find_brave(Some(&at("mine/brave beta")), None, Some(&path)),
            Some(at("mine/brave beta"))
        );
        assert_eq!(
            find_brave(Some(&at("mine/absent")), None, Some(&path)),
            None
        );
    }

    /// The extension's directory needs a manifest, or Brave loads nothing without saying so, and
    /// a comma in its name, which Brave reads as a separator, would load two other directories.
    #[test]
    fn an_extension_directory_needs_a_manifest_and_no_comma() {
        let scratch = directory();
        let at = |name: &str| scratch.path().join(name);
        std::fs::create_dir_all(at("empty")).unwrap();
        assert!(extension_directory(&at("empty")).is_err());

        std::fs::create_dir_all(at("a,b")).unwrap();
        std::fs::write(at("a,b").join(EXTENSION_MANIFEST), "{}").unwrap();
        let said = extension_directory(&at("a,b")).unwrap_err().to_string();
        assert!(said.contains("comma"), "{said}");

        std::fs::create_dir_all(at("ext dir")).unwrap();
        std::fs::write(at("ext dir").join(EXTENSION_MANIFEST), "{}").unwrap();
        assert_eq!(extension_directory(&at("ext dir")).unwrap(), at("ext dir"));
    }

    /// `profile` needs the profile and the extension, once each, and takes no other word. A value
    /// is the word after its flag even where it starts with a dash, and is kept as the bytes it
    /// has, so a directory whose name is not UTF-8 is not the one its replacement characters spell.
    #[test]
    fn profile_takes_the_two_directories_and_a_brave_and_no_other_word() {
        fn of(words: &[&'static str]) -> Option<Arguments<'static>> {
            let words: Vec<&OsStr> = words.iter().map(|word| OsStr::new(*word)).collect();
            arguments(&words)
        }
        assert_eq!(
            of(&["--extension-dir", "-e", "--profile-dir", "-p"]),
            Some(Arguments {
                profile: OsStr::new("-p"),
                extension: OsStr::new("-e"),
                brave: None
            })
        );
        assert_eq!(
            of(&["--profile-dir", "p", "--extension-dir", "e", "--brave", "b"])
                .and_then(|asked| asked.brave),
            Some(OsStr::new("b"))
        );
        for words in [
            &[][..],
            &["--profile-dir", "p"],
            &["--extension-dir", "e"],
            &["--profile-dir", "p", "--extension-dir"],
            &[
                "--profile-dir",
                "p",
                "--profile-dir",
                "q",
                "--extension-dir",
                "e",
            ],
            &[
                "--profile-dir",
                "p",
                "--extension-dir",
                "e",
                "--brave",
                "a",
                "--brave",
                "b",
            ],
            &["--profile-dir", "p", "--extension-dir", "e", "extra"],
            &[
                "--profile-dir",
                "p",
                "--extension-dir",
                "e",
                "--unknown",
                "x",
            ],
        ] {
            assert_eq!(of(words), None, "{words:?}");
        }

        let invalid = OsStr::from_bytes(b"p-\xff");
        let lookalike = OsStr::new("p-\u{FFFD}");
        let with = |profile| {
            let words = [
                OsStr::new("--profile-dir"),
                profile,
                OsStr::new("--extension-dir"),
                OsStr::new("e"),
            ];
            arguments(&words).map(|asked| asked.profile)
        };
        assert_eq!(with(invalid), Some(invalid));
        assert_ne!(with(invalid), with(lookalike));
        let unknown = [OsStr::from_bytes(b"--profile-dir\xff"), OsStr::new("p")];
        assert_eq!(arguments(&unknown), None);
    }
}
