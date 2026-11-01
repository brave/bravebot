//! The Brave profile of the extension's own: `profile`, run as the program it is, with HOME and the
//! relay's directory in scratch directories. A fake Brave is a script that records the words it was
//! started with, so no browser is run.

#![cfg(unix)]
#![forbid(unsafe_code)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_browser::profile;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, PoisonError};

const PROGRAM: &str = env!("CARGO_BIN_EXE_bravebot-browser");

/// Held while a script is written and while a process is created, so that no child of another test
/// holds the script open for writing as this one runs it.
static CREATING: Mutex<()> = Mutex::new(());

/// A scratch directory and the places the commands are pointed at inside it.
struct Setup {
    /// Keeps the scratch directory until the test ends.
    _scratch: tempfile::TempDir,
    /// The scratch directory with its links resolved, which is what a command run in it sees.
    work: PathBuf,
    home: PathBuf,
    relay: PathBuf,
}

impl Setup {
    fn new() -> Self {
        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().canonicalize().unwrap();
        let home = work.join("home");
        let relay = work.join("relay");
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(&relay).unwrap();
        Setup {
            _scratch: scratch,
            work,
            home,
            relay,
        }
    }

    /// Runs the relay program in the scratch directory with the words, as a person would.
    fn run(&self, words: &[&str]) -> Output {
        self.run_with(words, |_| {})
    }

    fn run_with(&self, words: &[&str], adjust: impl FnOnce(&mut Command)) -> Output {
        let mut command = Command::new(PROGRAM);
        command
            .args(words)
            .current_dir(&self.work)
            .env("HOME", &self.home)
            .env_remove("BRAVEBOT_HOME")
            .env("BRAVEBOT_BROWSER_DIR", &self.relay)
            .env_remove("XDG_CONFIG_HOME")
            .stdin(Stdio::null());
        adjust(&mut command);
        output(&mut command)
    }

    /// Runs `line` in a shell in the scratch directory, as a person would paste it, with
    /// `RECORDED` naming the file a fake Brave writes to.
    fn shell(&self, line: &OsStr, recorded: &Path) -> Output {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(line)
            .current_dir(&self.work)
            .env("RECORDED", recorded)
            .stdin(Stdio::null());
        output(&mut command)
    }

    /// A fake Brave that writes each word it was started with, and a NUL after it, to the file
    /// `RECORDED` names. It is at `name` in the scratch directory.
    fn fake_brave(&self, name: &str) -> PathBuf {
        let path = self.work.join(name);
        let _creating = CREATING.lock().unwrap_or_else(PoisonError::into_inner);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "#!/bin/sh\nfor word in \"$@\"; do printf '%s\\0' \"$word\"; done > \"$RECORDED\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// A directory the extension could be loaded from.
    fn extension(&self, name: &str) -> PathBuf {
        let path = self.work.join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("manifest.json"), "{}").unwrap();
        path
    }

    /// The scratch directory's own copy of a path named relative to it.
    fn at(&self, name: &str) -> PathBuf {
        self.work.join(name)
    }
}

fn output(command: &mut Command) -> Output {
    let _creating = CREATING.lock().unwrap_or_else(PoisonError::into_inner);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    command.output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Every path under `root`, relative to it, so that two states of a directory can be compared.
fn tree(root: &Path) -> Vec<PathBuf> {
    fn visit(root: &Path, directory: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            found.push(path.strip_prefix(root).unwrap().to_path_buf());
            if std::fs::symlink_metadata(&path).unwrap().is_dir() {
                visit(root, &path, found);
            }
        }
    }
    let mut found = Vec::new();
    visit(root, root, &mut found);
    found.sort();
    found
}

/// Where the person's own Brave reads a host manifest on this platform.
fn platform_manifests(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts")
    } else {
        home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts")
    }
}

/// The words a fake Brave recorded.
fn recorded(file: &Path) -> Vec<OsString> {
    let bytes = std::fs::read(file).unwrap();
    let mut words: Vec<OsString> = bytes
        .split(|byte| *byte == 0)
        .map(|word| OsStr::from_bytes(word).to_os_string())
        .collect();
    assert_eq!(words.pop().as_deref(), Some(OsStr::new("")));
    words
}

/// `profile` prints one line, and running it in a shell starts the Brave it names with the
/// profile and the extension as one word each, whatever the directories are called: a space, a
/// quote and a leading dash among them. The line is built from absolute paths. Printing it starts
/// nothing, makes the profile private to this account with the marker in it and nothing else, and
/// writes nothing in HOME, where a manifest of the person's own Brave and a neighbour stay as they
/// were, or in the relay's directory.
#[test]
fn profile_prints_the_line_that_starts_brave_on_a_directory_of_its_own_with_the_extension_loaded() {
    let setup = Setup::new();
    let theirs = platform_manifests(&setup.home);
    std::fs::create_dir_all(&theirs).unwrap();
    std::fs::write(theirs.join("com.brave.bravebot.json"), "theirs").unwrap();
    std::fs::write(theirs.join("com.example.other.json"), "{}").unwrap();
    let (before_home, before_relay) = (tree(&setup.home), tree(&setup.relay));
    let extension = setup.extension("-an extension");
    setup.fake_brave("brave dir/Brave Browser");
    let profile = setup.at("-it's a profile");
    assert!(!profile.exists());

    let printed = setup.run(&[
        "profile",
        "--extension-dir",
        "-an extension",
        "--brave",
        "brave dir/Brave Browser",
        "--profile-dir",
        "-it's a profile",
    ]);
    assert!(printed.status.success(), "{}", text(&printed.stderr));
    assert!(printed.stderr.is_empty(), "{}", text(&printed.stderr));
    let stdout = printed.stdout.clone();
    assert_eq!(stdout.iter().filter(|byte| **byte == b'\n').count(), 1);
    assert_eq!(stdout.last(), Some(&b'\n'));

    assert_eq!(tree(&profile), [PathBuf::from(profile::MARKER)]);
    assert_eq!(
        std::fs::metadata(&profile).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(tree(&setup.home), before_home);
    assert_eq!(tree(&setup.relay), before_relay);
    assert_eq!(
        std::fs::read_to_string(theirs.join("com.brave.bravebot.json")).unwrap(),
        "theirs"
    );

    let record = setup.at("recorded");
    assert!(!record.exists(), "printing the line started Brave");
    let ran = setup.shell(OsStr::from_bytes(&stdout), &record);
    assert!(ran.status.success(), "{}", text(&ran.stderr));
    let expected: Vec<OsString> = vec![
        format!("--user-data-dir={}", profile.display()).into(),
        format!("--load-extension={}", extension.display()).into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
    ];
    assert_eq!(recorded(&record), expected);
}

/// A directory a Brave has run on is not made the extension's profile: no command is printed,
/// nothing is written into it, and the person is told why. Either file Brave leaves there is
/// enough.
#[test]
fn profile_refuses_a_directory_that_holds_someone_elses_profile_and_writes_nothing() {
    for held in ["Default", "Local State"] {
        let setup = Setup::new();
        let extension = setup.extension("ext");
        let brave = setup.fake_brave("brave");
        let profile = setup.at("used");
        std::fs::create_dir_all(&profile).unwrap();
        if held == "Default" {
            std::fs::create_dir(profile.join(held)).unwrap();
        } else {
            std::fs::write(profile.join(held), "{}").unwrap();
        }
        let before = tree(&profile);

        let refused = setup.run(&[
            "profile",
            "--profile-dir",
            "used",
            "--extension-dir",
            extension.to_str().unwrap(),
            "--brave",
            brave.to_str().unwrap(),
        ]);
        assert_eq!(refused.status.code(), Some(1), "{held}");
        let said = text(&refused.stderr);
        assert!(
            said.contains(held) && said.contains("did not make"),
            "{said}"
        );
        assert!(refused.stdout.is_empty(), "{held}");
        assert_eq!(tree(&profile), before, "{held}");
    }
}

/// A profile `profile` made is used again after Brave has run on it, which leaves `Default` and
/// `Local State` there, and the same line is printed.
#[test]
fn profile_runs_again_on_a_directory_it_made_after_a_browser_has_used_it() {
    let setup = Setup::new();
    let extension = setup.extension("ext");
    let brave = setup.fake_brave("brave");
    let words = [
        "profile",
        "--profile-dir",
        "p",
        "--extension-dir",
        extension.to_str().unwrap(),
        "--brave",
        brave.to_str().unwrap(),
    ];
    let first = setup.run(&words);
    assert!(first.status.success(), "{}", text(&first.stderr));
    let profile = setup.at("p");
    std::fs::create_dir(profile.join("Default")).unwrap();
    std::fs::write(profile.join("Local State"), "{}").unwrap();

    let again = setup.run(&words);
    assert!(again.status.success(), "{}", text(&again.stderr));
    assert_eq!(again.stdout, first.stdout);
}

/// A directory that Brave would not load as the extension, and a Brave that is not a file, are
/// refused with nothing printed and the profile directory not made. The first has no manifest, and
/// the second has a comma, which `--load-extension` would read as two directories.
#[test]
fn profile_makes_no_directory_for_an_extension_brave_would_not_load_or_a_brave_that_is_not_there() {
    let setup = Setup::new();
    let brave = setup.fake_brave("brave");
    std::fs::create_dir_all(setup.at("empty")).unwrap();
    let with_comma = setup.extension("a,b");

    for (extension, brave, expected) in [
        (setup.at("empty"), brave.clone(), "manifest.json"),
        (with_comma, brave.clone(), "comma"),
        (
            setup.extension("ext"),
            setup.at("no brave"),
            "is not a file",
        ),
    ] {
        let refused = setup.run(&[
            "profile",
            "--profile-dir",
            "p",
            "--extension-dir",
            extension.to_str().unwrap(),
            "--brave",
            brave.to_str().unwrap(),
        ]);
        assert_eq!(refused.status.code(), Some(1), "{expected}");
        assert!(refused.stdout.is_empty(), "{expected}");
        let said = text(&refused.stderr);
        assert!(said.contains(expected), "{expected}: {said}");
        assert!(!setup.at("p").exists(), "{expected}");
    }
}

/// The line gives a shell back the words it was built from, for the names that a shell would
/// split, expand, run or end a command at, and for bytes that are not text.
#[test]
fn the_line_is_the_same_words_to_a_shell() {
    let setup = Setup::new();
    let names: Vec<OsString> = [
        &b"plain"[..],
        b"two words",
        b"it's",
        b"say \"hi\"",
        b"$HOME and `date` and $(date)",
        b"~/x",
        b"* ? [a]",
        b"a;b && c | d > e < f & g",
        b"back\\slash",
        b"line\nbreak",
        b"",
        b"-n",
        b"--user-data-dir=/a b",
        b"bytes-\xff-\xfe",
        "bytes-\u{FFFD}".as_bytes(),
    ]
    .iter()
    .map(|name| OsString::from_vec(name.to_vec()))
    .collect();
    let mut words = vec![OsString::from("printf"), OsString::from("%s\\0")];
    words.extend(names.iter().cloned());
    let line = profile::line(&words);

    let ran = setup.shell(&line, &setup.at("unused"));
    assert!(ran.status.success(), "{}", text(&ran.stderr));
    let mut received: Vec<&[u8]> = ran.stdout.split(|byte| *byte == 0).collect();
    assert_eq!(received.pop(), Some(&b""[..]));
    let expected: Vec<&[u8]> = names.iter().map(|name| name.as_bytes()).collect();
    assert_eq!(received, expected);
}

/// Runs `profile` with the three flags, where the one named is given `value` and the others name
/// the extension `ext` and the Brave `brave` of the scratch directory and a profile `p`.
fn profile_with(setup: &Setup, flag: &str, value: &[u8]) -> Output {
    let mut words = vec!["profile"];
    for (name, given) in [
        ("--profile-dir", "p"),
        ("--extension-dir", "ext"),
        ("--brave", "brave"),
    ] {
        if name != flag {
            words.extend([name, given]);
        }
    }
    words.push(flag);
    setup.run_with(&words, |command| {
        command.arg(OsStr::from_bytes(value));
    })
}

/// A directory whose name is not UTF-8 is still the directory Brave is to be started on, so
/// `profile` takes it as the bytes it has: it does not stop at the argument, and does not make or
/// start Brave on the directory its replacement characters spell. macOS refuses to make such a
/// name, which is how the bytes are seen to arrive, and elsewhere the line holds them.
#[test]
fn profile_given_a_directory_whose_name_is_not_text_takes_the_bytes_it_has() {
    let setup = Setup::new();
    let extension = setup.extension("ext");
    setup.fake_brave("brave");
    let printed = profile_with(&setup, "--profile-dir", b"p-\xff");
    let said = text(&printed.stderr);
    assert!(!said.contains("panicked"), "{said}");
    assert!(
        !setup.at("p-\u{FFFD}").exists(),
        "a directory was made for the lookalike"
    );
    if cfg!(target_os = "macos") {
        assert_eq!(printed.status.code(), Some(1), "{said}");
        assert!(printed.stdout.is_empty());
        return;
    }
    assert!(printed.status.success(), "{said}");

    let profile = setup.work.join(OsStr::from_bytes(b"p-\xff"));
    assert_eq!(tree(&profile), [PathBuf::from(profile::MARKER)]);
    let record = setup.at("recorded");
    let ran = setup.shell(OsStr::from_bytes(&printed.stdout), &record);
    assert!(ran.status.success(), "{}", text(&ran.stderr));
    let expected: Vec<OsString> = vec![
        OsString::from_vec([b"--user-data-dir=", profile.as_os_str().as_bytes()].concat()),
        format!("--load-extension={}", extension.display()).into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
    ];
    assert_eq!(recorded(&record), expected);
}

/// An extension directory or a Brave whose name is not UTF-8 and is not there is refused as one
/// with a name that is text would be, with the exit code 1 and nothing made.
#[test]
fn profile_given_an_extension_or_a_brave_whose_name_is_not_text_refuses_it_and_makes_nothing() {
    let setup = Setup::new();
    setup.extension("ext");
    setup.fake_brave("brave");
    for (flag, expected) in [
        ("--extension-dir", "manifest.json"),
        ("--brave", "is not a file"),
    ] {
        let refused = profile_with(&setup, flag, b"absent-\xff");
        let said = text(&refused.stderr);
        assert_eq!(refused.status.code(), Some(1), "{flag}: {said}");
        assert!(refused.stdout.is_empty(), "{flag}");
        assert!(said.contains(expected), "{flag}: {said}");
        assert!(!setup.at("p").exists(), "{flag}");
    }
}

/// The native host, `mcp` and `install` take text, so a word that is not UTF-8 is answered with a
/// sentence saying so and the exit code 2, and nothing is written. It does not stop the process
/// with a panic.
#[test]
fn a_subcommand_that_takes_text_refuses_a_word_that_is_not_text_and_writes_nothing() {
    let setup = Setup::new();
    let (before_home, before_relay) = (tree(&setup.home), tree(&setup.relay));
    for (words, bad) in [
        (&["install", "--manifest-dir"][..], &b"m-\xff"[..]),
        (&["mcp"], b"\xff"),
        (&["mcp", "--group"], b"platform-\xff"),
        (&[], b"\xff"),
        (&[], b"chrome-extension://\xff"),
    ] {
        let refused = setup.run_with(words, |command| {
            command.arg(OsStr::from_bytes(bad));
        });
        let said = text(&refused.stderr);
        assert_eq!(refused.status.code(), Some(2), "{words:?}: {said}");
        assert!(refused.stdout.is_empty(), "{words:?}");
        assert!(said.contains("not valid UTF-8"), "{words:?}: {said}");
        assert!(!said.contains("panicked"), "{words:?}: {said}");
    }
    assert_eq!(tree(&setup.home), before_home);
    assert_eq!(tree(&setup.relay), before_relay);
}

/// With no `--brave` and nothing named `brave-browser` or `brave` on `PATH`, a Brave is not
/// found, and the person is told how to name one. macOS has Brave's app path instead of a search,
/// so this is run elsewhere.
#[cfg(not(target_os = "macos"))]
#[test]
fn profile_says_how_to_name_a_brave_when_none_is_on_path() {
    let setup = Setup::new();
    let extension = setup.extension("ext");
    let refused = setup.run_with(
        &[
            "profile",
            "--profile-dir",
            "p",
            "--extension-dir",
            extension.to_str().unwrap(),
        ],
        |command| {
            command.env("PATH", "");
        },
    );
    assert_eq!(refused.status.code(), Some(1));
    assert!(refused.stdout.is_empty());
    assert!(
        text(&refused.stderr).contains("--brave"),
        "{}",
        text(&refused.stderr)
    );
}

/// A profile that holds the link a running Brave keeps is said to maybe have one running, and the
/// command is still printed, since the link outlasts a Brave that stopped without removing it. A
/// profile that does not hold it gets no such remark.
#[test]
fn profile_says_a_brave_may_already_be_running_where_the_profile_holds_its_lock() {
    let setup = Setup::new();
    let extension = setup.extension("ext");
    let brave = setup.fake_brave("brave");
    let words = [
        "profile",
        "--profile-dir",
        "p",
        "--extension-dir",
        extension.to_str().unwrap(),
        "--brave",
        brave.to_str().unwrap(),
    ];

    let quiet = setup.run(&words);
    assert!(quiet.status.success(), "{}", text(&quiet.stderr));
    assert!(quiet.stderr.is_empty(), "{}", text(&quiet.stderr));

    std::os::unix::fs::symlink("some-host-123", setup.at("p").join(profile::SINGLETON_LOCK))
        .unwrap();
    let running = setup.run(&words);
    assert!(running.status.success());
    assert_eq!(running.stdout, quiet.stdout);
    let said = text(&running.stderr);
    assert!(
        said.contains("may already be running") && said.contains("does not start a second browser"),
        "{said}"
    );
}

/// Words `profile` does not take are answered with the usage, which names every flag, and print
/// nothing.
#[test]
fn profile_given_words_it_does_not_take_prints_the_usage() {
    let setup = Setup::new();
    for words in [
        &["profile"][..],
        &["profile", "--profile-dir", "p"],
        &["profile", "--extension-dir", "e"],
        &["profile", "--profile-dir", "p", "--extension-dir"],
        &[
            "profile",
            "--profile-dir",
            "p",
            "--extension-dir",
            "e",
            "extra",
        ],
        &[
            "profile",
            "--profile-dir",
            "p",
            "--profile-dir",
            "q",
            "--extension-dir",
            "e",
        ],
    ] {
        let refused = setup.run(words);
        assert_eq!(refused.status.code(), Some(2), "{words:?}");
        assert!(refused.stdout.is_empty(), "{words:?}");
        let said = text(&refused.stderr);
        assert!(said.contains("usage: bravebot-browser install"), "{said}");
        for flag in ["profile", "--profile-dir", "--extension-dir", "--brave"] {
            assert!(said.contains(flag), "{flag}: {said}");
        }
    }
}
