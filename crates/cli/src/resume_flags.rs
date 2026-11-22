//! The flags a session was started with, as the line printed on the way out repeats them
//! ([SESSION-8](../../../docs/specs/sessions.md#SESSION-8)).
//!
//! A record keeps neither the permission mode nor the sandbox mode, the network setting, the lists
//! of paths, the settings file, the strictness, the log level nor the system prompt a run was given,
//! so a resume that omits a flag runs under the rules its settings and defaults give and asks
//! again. The line the person copies therefore carries the flags they typed.
//!
//! Everything here is the person's own command line, read by `settle_flags` and never taken from a
//! record, a checkout or a model. What is recorded is what each scanner in [`crate::flags`] took,
//! so a flag typed twice is the value the scanner resolved to, and nothing is read back from the
//! settings or the environment to decide what to print.

use bravebot_sandbox::SandboxMode;
use bravebot_sandbox::network::Network;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

/// The process-wide flags a command line carried, in the form [`Typed::words`] repeats them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each bool is one flag the person typed or did not"
)]
pub(crate) struct Typed {
    /// The spelling of the flag that answers every question, where one was typed.
    pub(crate) bypass: Option<&'static str>,
    pub(crate) auto: bool,
    /// The `--sandbox` mode as typed, the last of several.
    pub(crate) sandbox: Option<SandboxMode>,
    /// The `--run-network` setting as typed, the last of several.
    pub(crate) run_network: Option<Network>,
    pub(crate) allow_read: Vec<String>,
    pub(crate) deny_read: Vec<String>,
    pub(crate) allow_write: Vec<String>,
    pub(crate) deny_write: Vec<String>,
    /// The `--settings` file, the last of several.
    pub(crate) settings: Option<PathBuf>,
    pub(crate) safe: bool,
    pub(crate) locked: bool,
    pub(crate) vet: bool,
    pub(crate) incognito: bool,
    /// The tools `--tools` named, the last list of several.
    pub(crate) tools: Option<Vec<String>>,
    pub(crate) no_shell: bool,
    /// The level `--log-level` named, the last of several.
    pub(crate) log_level: Option<bravebot_diag::Level>,
    /// The words `--system-prompt` gave, the last of several.
    pub(crate) system_prompt: Option<String>,
    /// The words `--append-system-prompt` gave, the last of several.
    pub(crate) append_system_prompt: Option<String>,
    /// The directory the process was in when the flags were read, which a relative `--settings`
    /// path was read from. The paths of the four `--sandbox-*` flags are read from the session's
    /// directory instead, which the line is given where it is built.
    pub(crate) read_in: Option<PathBuf>,
}

impl Typed {
    /// The words of the flags, unquoted, one flag and its value after another.
    ///
    /// Empty where none was typed. Each list flag is repeated once per path, in the order the
    /// paths were typed, and every other flag appears once.
    pub(crate) fn words(&self) -> Vec<String> {
        let mut words: Vec<String> = Vec::new();
        if let Some(flag) = self.bypass {
            words.push(flag.to_string());
        }
        if self.auto {
            words.push(crate::flags::AUTO.to_string());
        }
        if let Some(mode) = self.sandbox {
            words.extend(["--sandbox".to_string(), mode.name().to_string()]);
        }
        if let Some(network) = self.run_network {
            words.extend(["--run-network".to_string(), network.name().to_string()]);
        }
        for (flag, paths) in [
            ("--sandbox-allow-read", &self.allow_read),
            ("--sandbox-deny-read", &self.deny_read),
            ("--sandbox-allow-write", &self.allow_write),
            ("--sandbox-deny-write", &self.deny_write),
        ] {
            for path in paths {
                words.extend([flag.to_string(), path.clone()]);
            }
        }
        if let Some(path) = &self.settings {
            words.extend([
                "--settings".to_string(),
                path.to_string_lossy().into_owned(),
            ]);
        }
        for (given, flag) in [
            (self.safe, crate::flags::SAFE),
            (self.locked, "--locked"),
            (self.vet, crate::flags::VET),
            (self.incognito, crate::flags::INCOGNITO),
        ] {
            if given {
                words.push(flag.to_string());
            }
        }
        if let Some(tools) = &self.tools {
            words.extend(["--tools".to_string(), tools.join(",")]);
        }
        if self.no_shell {
            words.push("--no-shell".to_string());
        }
        if let Some(level) = self.log_level {
            words.extend(["--log-level".to_string(), level.as_str().to_string()]);
        }
        if let Some(text) = &self.system_prompt {
            words.extend(["--system-prompt".to_string(), text.clone()]);
        }
        if let Some(text) = &self.append_system_prompt {
            words.extend(["--append-system-prompt".to_string(), text.clone()]);
        }
        words
    }

    /// These flags for a session that moved, with each relative value of `--settings` and of the
    /// four `--sandbox-*` path flags written as the absolute path it was read as when the session
    /// started.
    ///
    /// A relative value is read from a directory: the process's own, for `--settings`
    /// ([`settings_path_from`]), and the session's, for a sandbox path ([`sandbox_path_from`]). The
    /// line a moved session prints is run from the session's new directory, where the same words
    /// would name another file or confine another place. `session_directory` is the directory the
    /// session started in. A `--settings` value stays as typed where the process's directory was not
    /// read.
    pub(crate) fn with_paths_made_absolute(&self, session_directory: &Path) -> Typed {
        let from_the_session = |paths: &[String]| -> Vec<String> {
            paths
                .iter()
                .map(|path| sandbox_path_from(path, session_directory))
                .collect()
        };
        Typed {
            allow_read: from_the_session(&self.allow_read),
            deny_read: from_the_session(&self.deny_read),
            allow_write: from_the_session(&self.allow_write),
            deny_write: from_the_session(&self.deny_write),
            settings: match (&self.settings, &self.read_in) {
                (Some(path), Some(directory)) => Some(PathBuf::from(settings_path_from(
                    &path.to_string_lossy(),
                    directory,
                ))),
                _ => self.settings.clone(),
            },
            ..self.clone()
        }
    }

    /// What follows `bravebot --resume <id>` on the line: each word as one word of a shell, drawn
    /// so that no character of it moves the cursor, each with a space in front. Empty where no
    /// flag was typed.
    ///
    /// A map from the person's own words to words. The quoting is [`bravebot_tui::dropped::shell_word`],
    /// which a path pasted into a shell line already goes through, and the drawing is the one every
    /// path on the terminal goes through (LAYER-3).
    pub(crate) fn suffix(&self) -> String {
        self.words()
            .iter()
            .map(|word| {
                format!(
                    " {}",
                    bravebot_agent::confirm::printable_spelled_with_tab_pictured(
                        &bravebot_tui::dropped::shell_word(word)
                    )
                )
            })
            .collect()
    }
}

/// The entry `value` of a `--sandbox-*` list as an absolute entry that the sandbox reads as the
/// place the relative entry was read as from `directory`, or as typed where it needs no directory.
///
/// The sandbox reads a relative entry from the session's directory and resolves its `.` and `..` by
/// name as it goes: a `..` removes a name the entry holds, and one with none left to remove
/// climbs out of the directory, so the entry is refused. It refuses an absolute entry that holds
/// any `..`, because it cannot say where that one climbs from (`one` in
/// `crates/sandbox/src/rules.rs`, which judges `absolute || climbed == 0` as `Reason::Climbs`).
/// A relative entry joined to `directory` with its `..` left in place would therefore be refused
/// where it was in force, and a deny entry that is refused is lost. This resolves `.` and `..` the
/// way the sandbox does, so the entry names the same path whichever directory the line is run
/// from. Wildcards and trailing slashes are left to the sandbox: a name holding one is pushed as it
/// is, and a trailing slash adds no name.
///
/// Returned as typed: an absolute entry, one that starts at the home directory (`~`, `~/` or `~\`,
/// the three spellings the sandbox reads from home), one that is blank, and one that starts with a
/// variable, which the sandbox refuses before it reads a path and which a joined path would turn
/// into a place that exists. An entry that climbs out of the directory (`..`, `../x`,
/// `sub/../../x`) is joined to `directory` with its `..` left in place: the sandbox refused it as
/// typed, so it was never in force, and the absolute entry holding a `..` is refused the same way.
/// Resolving it would turn an entry that was refused into one that is in force. So is an entry
/// whose `..` removes a name that holds a wildcard (`a/*/../x`): the sandbox reads it as a pattern,
/// which a write list refuses and a read list matches nothing with, and the resolved entry would
/// name a path.
///
/// A pure function of its arguments, which read nothing from the filesystem and resolve no link, so
/// a `..` after a name that is a link removes the name, as the sandbox's own reading does.
pub(crate) fn sandbox_path_from(value: &str, directory: &Path) -> String {
    let spelled = value.trim();
    let from_home = spelled
        .strip_prefix('~')
        .is_some_and(|after| after.is_empty() || after.starts_with(['/', '\\']));
    if spelled.is_empty()
        || from_home
        || starts_with_a_variable(spelled)
        || Path::new(spelled).is_absolute()
    {
        return value.to_string();
    }
    let left_unresolved = || directory.join(spelled).to_string_lossy().into_owned();
    let mut path = directory.to_path_buf();
    let mut names: Vec<&std::ffi::OsStr> = Vec::new();
    for part in Path::new(spelled).components() {
        match part {
            Component::Prefix(_) | Component::RootDir => path.push(part.as_os_str()),
            Component::CurDir => {}
            Component::Normal(name) => {
                path.push(name);
                names.push(name);
            }
            Component::ParentDir => match names.pop() {
                Some(name) if !name.to_string_lossy().contains(['*', '?']) => {
                    path.pop();
                }
                _ => return left_unresolved(),
            },
        }
    }
    path.to_string_lossy().into_owned()
}

/// Whether an entry starts with a variable the way the sandbox reads one (`starts_with_a_variable`
/// in `crates/sandbox/src/rules.rs`): `$NAME`, `${`, or `%NAME%`.
fn starts_with_a_variable(spelled: &str) -> bool {
    let name_start = |c: char| c.is_ascii_alphabetic() || c == '_';
    if let Some(after) = spelled.strip_prefix('$') {
        return after.starts_with(name_start) || after.starts_with('{');
    }
    if let Some(after) = spelled.strip_prefix('%') {
        return after.starts_with(name_start) && after.contains('%');
    }
    false
}

/// The `--settings` file `value` as the absolute path it names when read from `directory`, or as
/// typed where it is absolute.
///
/// Settings loading expands no `~`, so `~/x` that a shell did not expand is a relative file named
/// `~` in `directory`, and is joined to it as any other relative value is. The `..` of a joined
/// value is left in place: the operating system resolves a settings path against the directory
/// physically, and the joined path names the same file. A pure function of its arguments.
pub(crate) fn settings_path_from(value: &str, directory: &Path) -> String {
    let spelled = value.trim();
    if spelled.is_empty() || Path::new(spelled).is_absolute() {
        return value.to_string();
    }
    directory.join(spelled).to_string_lossy().into_owned()
}

static TYPED: OnceLock<Typed> = OnceLock::new();

/// Remember the flags the command line carried, for the rest of this process.
///
/// Called once by `settle_flags`, after every scanner has run. The first call wins, as with the
/// other process-wide settings the entry point settles.
pub(crate) fn record(typed: Typed) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the first call wins, as with the other process-wide settings"
    )]
    let _ = TYPED.set(typed);
}

/// The flags [`record`] was given, or none where `settle_flags` did not run to its end.
pub(crate) fn typed() -> Typed {
    TYPED.get().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(typed: Typed) -> Vec<String> {
        typed.words()
    }

    fn spelled(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    /// The flag that answers every question and the sandbox mode are the two the person asked for
    /// by name. Each is repeated as typed, so a line that repeated the other spelling, or the
    /// default mode, would resume under rules that were not the session's.
    #[test]
    fn the_bypass_flag_as_typed_and_the_sandbox_mode_are_repeated() {
        assert_eq!(
            only(Typed {
                bypass: Some(crate::flags::SKIP_PERMISSIONS),
                sandbox: Some(SandboxMode::Strict),
                ..Typed::default()
            }),
            spelled(&["--dangerously-skip-permissions", "--sandbox", "strict"])
        );
        assert_eq!(
            only(Typed {
                bypass: Some(crate::flags::BYPASS_APPROVALS_AND_SANDBOX),
                ..Typed::default()
            }),
            spelled(&["--dangerously-bypass-approvals-and-sandbox"])
        );
        for mode in SandboxMode::ALL {
            assert_eq!(
                only(Typed {
                    sandbox: Some(mode),
                    ..Typed::default()
                }),
                spelled(&["--sandbox", mode.name()])
            );
        }
    }

    /// Each flag a resume does not take back is repeated with its value, each path of a list flag
    /// once per path, and no flag stands in for another. The regression it rejects is a builder
    /// that leaves one of them out or moves a value to the wrong flag.
    #[test]
    fn every_flag_a_resume_does_not_take_back_is_repeated_with_its_value() {
        let cases: Vec<(Typed, Vec<String>)> = vec![
            (
                Typed {
                    auto: true,
                    ..Typed::default()
                },
                spelled(&["--auto"]),
            ),
            (
                Typed {
                    run_network: Some(Network::Closed),
                    ..Typed::default()
                },
                spelled(&["--run-network", "closed"]),
            ),
            (
                Typed {
                    run_network: Some(Network::Open),
                    ..Typed::default()
                },
                spelled(&["--run-network", "open"]),
            ),
            (
                Typed {
                    allow_read: vec!["/r1".into(), "/r2".into()],
                    ..Typed::default()
                },
                spelled(&["--sandbox-allow-read", "/r1", "--sandbox-allow-read", "/r2"]),
            ),
            (
                Typed {
                    deny_read: vec!["/dr".into()],
                    ..Typed::default()
                },
                spelled(&["--sandbox-deny-read", "/dr"]),
            ),
            (
                Typed {
                    allow_write: vec!["/aw".into()],
                    ..Typed::default()
                },
                spelled(&["--sandbox-allow-write", "/aw"]),
            ),
            (
                Typed {
                    deny_write: vec!["/dw1".into(), "/dw2".into()],
                    ..Typed::default()
                },
                spelled(&[
                    "--sandbox-deny-write",
                    "/dw1",
                    "--sandbox-deny-write",
                    "/dw2",
                ]),
            ),
            (
                Typed {
                    settings: Some(PathBuf::from("/s/settings.json")),
                    ..Typed::default()
                },
                spelled(&["--settings", "/s/settings.json"]),
            ),
            (
                Typed {
                    safe: true,
                    ..Typed::default()
                },
                spelled(&["--safe"]),
            ),
            (
                Typed {
                    locked: true,
                    ..Typed::default()
                },
                spelled(&["--locked"]),
            ),
            (
                Typed {
                    vet: true,
                    ..Typed::default()
                },
                spelled(&["--vet"]),
            ),
            (
                Typed {
                    incognito: true,
                    ..Typed::default()
                },
                spelled(&["--incognito"]),
            ),
            (
                Typed {
                    tools: Some(vec!["read_file".into(), "search".into()]),
                    no_shell: true,
                    ..Typed::default()
                },
                spelled(&["--tools", "read_file,search", "--no-shell"]),
            ),
            (
                Typed {
                    system_prompt: Some("be brief".into()),
                    append_system_prompt: Some("end politely".into()),
                    ..Typed::default()
                },
                spelled(&[
                    "--system-prompt",
                    "be brief",
                    "--append-system-prompt",
                    "end politely",
                ]),
            ),
        ];
        for (typed, expected) in cases {
            assert_eq!(typed.words(), expected, "{typed:?}");
        }
    }

    /// `--log-level` is repeated with the word `--log-level` parses, for each level, so a line that
    /// is pasted back is accepted. The regression it rejects is a spelling the scanner does not
    /// parse, which refuses the pasted line.
    #[test]
    fn the_log_level_is_repeated_in_the_spelling_the_flag_parses() {
        for level in [
            bravebot_diag::Level::Error,
            bravebot_diag::Level::Info,
            bravebot_diag::Level::Debug,
        ] {
            let words = Typed {
                log_level: Some(level),
                ..Typed::default()
            }
            .words();

            assert_eq!(words.len(), 2, "{words:?}");
            assert_eq!(words[0], "--log-level");
            assert_eq!(bravebot_diag::Level::parse(&words[1]), Some(level));
        }
    }

    /// A sandbox entry is read from a directory, and the line of a session that moved is run from
    /// another. An entry that starts at the home directory, is absolute or starts with a variable
    /// names the same place from every directory and is left as typed. Any other is the absolute
    /// entry the sandbox reads it as: the directory with the entry's `.` and `..` resolved by name,
    /// leading white space dropped and a trailing slash adding nothing. The sandbox refuses an
    /// absolute entry that holds a `..`, so a `..` that is left in place keeps the refusal an entry
    /// that climbs out of the directory already had, and one that removes a name holding a wildcard
    /// is left unresolved too, since resolving it would name a path where the entry named none. The
    /// regressions it rejects are a relative value left as typed, a home value or an absolute one
    /// joined to a directory, a `..` left in the middle of an entry the sandbox accepted (the entry
    /// is then refused and a deny entry lost), and a `..` of an entry that climbs resolved away.
    #[cfg(unix)]
    #[test]
    fn a_relative_sandbox_entry_is_written_as_the_absolute_entry_the_sandbox_reads_it_as() {
        let from = Path::new("/work");
        for (typed, expected) in [
            ("x.json", "/work/x.json"),
            ("./y", "/work/y"),
            ("sub/../x", "/work/x"),
            ("a/b/../../c", "/work/c"),
            ("x/", "/work/x"),
            ("  padded ", "/work/padded"),
            ("  sub/../x ", "/work/x"),
            ("a/./b", "/work/a/b"),
            ("g/*/x", "/work/g/*/x"),
            ("..", "/work/.."),
            ("../up", "/work/../up"),
            ("sub/../../x", "/work/sub/../../x"),
            ("a/*/../x", "/work/a/*/../x"),
            ("~user/x", "/work/~user/x"),
            ("%dir/x", "/work/%dir/x"),
            ("/abs/p", "/abs/p"),
            ("/abs/../p", "/abs/../p"),
            ("~", "~"),
            ("~/notes", "~/notes"),
            ("~\\notes", "~\\notes"),
            ("$HOME/x", "$HOME/x"),
            ("${HOME}/x", "${HOME}/x"),
            ("%USERPROFILE%/x", "%USERPROFILE%/x"),
        ] {
            assert_eq!(sandbox_path_from(typed, from), expected, "{typed:?}");
        }
    }

    /// The sandbox reads the entry a moved session prints as it read the entry that was typed. Each
    /// value is given to the resolver the sandbox uses (`bravebot_sandbox::rules::resolve`) twice, as
    /// typed from the session's directory and as printed from the directory the session moved to,
    /// in each of the four lists, and the two come to the same path or the same refusal. The table
    /// holds the values that decide it: a `..` that removes a name the entry holds, one that has no
    /// name to remove and so climbs, a `.`, a trailing slash, white space, a glob, a variable, a home
    /// entry and an absolute one. The regression it rejects is a printed entry the sandbox reads as
    /// another path or refuses where the typed one was in force, which is a restriction lost for a
    /// deny entry and a grant lost for an allow entry.
    #[cfg(unix)]
    #[test]
    fn a_moved_sandbox_entry_means_to_the_sandbox_what_the_typed_entry_meant() {
        use bravebot_sandbox::rules::{Entry, List, Lists, Reason, State, resolve};

        let scratch = crate::test_support::Scratch::new("resume-flags-sandbox-entries");
        let session = scratch.directory("session");
        let moved_to = scratch.directory("moved");
        let home = scratch.directory("home");
        std::fs::create_dir_all(session.join("g/one/x")).expect("a directory a glob names");
        let absolute = session.join("abs/p");
        let state = |list: List, spelled: &str, base: &Path| -> State {
            let entry = Entry {
                path: spelled.to_string(),
                by: None,
                pinned: false,
                from_a_checkout: false,
            };
            let mut lists = Lists::default();
            match list {
                List::AllowRead => lists.allow_read.push(entry),
                List::DenyRead => lists.deny_read.push(entry),
                List::AllowWrite => lists.allow_write.push(entry),
                List::DenyWrite => lists.deny_write.push(entry),
            }
            resolve(&lists, Some(&home), None, base).items()[0]
                .state
                .clone()
        };
        let in_force = |path: PathBuf| State::InForce(vec![path]);
        let climbs = State::Refused(Reason::Climbs);
        let typed_values: Vec<(String, State)> = vec![
            ("sub/../x".into(), in_force(session.join("x"))),
            ("./x".into(), in_force(session.join("x"))),
            ("a/b/../../c".into(), in_force(session.join("c"))),
            ("x/".into(), in_force(session.join("x"))),
            ("  sub/../x ".into(), in_force(session.join("x"))),
            ("g/*/x".into(), in_force(session.join("g/one/x"))),
            ("..".into(), climbs.clone()),
            ("../x".into(), climbs.clone()),
            ("sub/../../x".into(), climbs.clone()),
            ("$HOME/x".into(), State::Refused(Reason::NamesAVariable)),
            ("~/x".into(), in_force(home.join("x"))),
            (absolute.display().to_string(), in_force(absolute.clone())),
            (format!("{}/../p", absolute.display()), climbs),
        ];

        for (typed, expected) in typed_values {
            let printed = sandbox_path_from(&typed, &session);
            for list in [
                List::AllowRead,
                List::DenyRead,
                List::AllowWrite,
                List::DenyWrite,
            ] {
                let as_typed = state(list, &typed, &session);
                assert_eq!(
                    state(list, &printed, &moved_to),
                    as_typed,
                    "{list:?} {typed:?} printed as {printed:?}"
                );
                // Wildcards in a write list are refused whichever way they are spelled, so what an
                // entry names is checked in the list that reads.
                if list == List::DenyRead {
                    assert_eq!(as_typed, expected, "{typed:?}");
                }
            }
            let holds_a_parent = Path::new(&printed)
                .components()
                .any(|part| part == Component::ParentDir);
            // An entry in force is written with no `..`, which the sandbox refuses in an absolute
            // one, and an entry it refused as climbing keeps the `..` it was refused for.
            assert_eq!(
                holds_a_parent,
                matches!(expected, State::Refused(Reason::Climbs)),
                "{typed:?} printed as {printed:?}"
            );
        }
    }

    /// A `--settings` file is read by the operating system, from the directory the process was in,
    /// and settings loading expands no `~`, so a value is joined to that directory unless it is
    /// absolute, with a `..` left in place, which names the same file physically. A `~/x` that the
    /// shell did not expand is a relative file named `~` there. The regressions it rejects are a
    /// `~` value printed as typed, which names another file from the new directory, and a `..`
    /// resolved by name, which names another file where a link is involved.
    #[cfg(unix)]
    #[test]
    fn a_settings_path_is_joined_to_the_directory_it_was_read_from() {
        let from = Path::new("/process");
        for (typed, expected) in [
            ("x.json", "/process/x.json"),
            ("./y", "/process/./y"),
            ("sub/../z", "/process/sub/../z"),
            ("../up", "/process/../up"),
            ("  padded ", "/process/padded"),
            ("~/x", "/process/~/x"),
            ("~", "/process/~"),
            ("/abs/p", "/abs/p"),
            ("/abs/../p", "/abs/../p"),
        ] {
            assert_eq!(settings_path_from(typed, from), expected, "{typed:?}");
        }
    }

    /// The line a session that moved prints is run from the directory it moved to, so each relative
    /// value of `--settings` and of the four `--sandbox-*` path flags is the absolute path it was read
    /// as at the start, and no other flag changes. `--settings` is read from the directory the
    /// process was in and a sandbox path from the session's, which are two directories, and each flag
    /// uses its own. The regressions it rejects are the session's directory used for `--settings`, the
    /// process's directory used for a sandbox path, a flag left relative, a path flag dropped from
    /// the rewrite, and a `--settings` value joined where the process's directory was not read.
    #[cfg(unix)]
    #[test]
    fn a_moved_session_names_each_relative_path_as_it_was_read_when_it_started() {
        let typed = Typed {
            allow_read: vec!["a".into(), "/abs".into()],
            deny_read: vec!["~/h".into(), "sub/../k".into()],
            allow_write: vec!["./w".into()],
            deny_write: vec!["../d".into(), "~".into()],
            settings: Some(PathBuf::from("s.json")),
            read_in: Some(PathBuf::from("/process")),
            safe: true,
            tools: Some(vec!["search".into()]),
            system_prompt: Some("relative words".into()),
            ..Typed::default()
        };

        assert_eq!(
            typed
                .with_paths_made_absolute(Path::new("/session"))
                .words(),
            spelled(&[
                "--sandbox-allow-read",
                "/session/a",
                "--sandbox-allow-read",
                "/abs",
                "--sandbox-deny-read",
                "~/h",
                "--sandbox-deny-read",
                "/session/k",
                "--sandbox-allow-write",
                "/session/w",
                "--sandbox-deny-write",
                "/session/../d",
                "--sandbox-deny-write",
                "~",
                "--settings",
                "/process/s.json",
                "--safe",
                "--tools",
                "search",
                "--system-prompt",
                "relative words",
            ])
        );
        // Left as typed where the session did not move.
        assert_eq!(
            typed.words(),
            spelled(&[
                "--sandbox-allow-read",
                "a",
                "--sandbox-allow-read",
                "/abs",
                "--sandbox-deny-read",
                "~/h",
                "--sandbox-deny-read",
                "sub/../k",
                "--sandbox-allow-write",
                "./w",
                "--sandbox-deny-write",
                "../d",
                "--sandbox-deny-write",
                "~",
                "--settings",
                "s.json",
                "--safe",
                "--tools",
                "search",
                "--system-prompt",
                "relative words",
            ])
        );
        // No directory was read for the settings file, so there is none to join it to.
        let unread = Typed {
            read_in: None,
            ..typed
        };
        assert!(
            unread
                .with_paths_made_absolute(Path::new("/session"))
                .words()
                .ends_with(&spelled(&[
                    "--settings",
                    "s.json",
                    "--safe",
                    "--tools",
                    "search",
                    "--system-prompt",
                    "relative words",
                ]))
        );
    }

    /// A session started with none of these flags repeats none, so the line is what it was before
    /// flags were carried.
    #[test]
    fn a_session_started_with_no_flag_repeats_none() {
        assert!(Typed::default().words().is_empty());
        assert_eq!(Typed::default().suffix(), "");
    }

    /// The line is for a person to paste into a shell, so a value is one word of it whatever it
    /// holds, and the terminal draws it without a character that moves the cursor or reorders the
    /// text. Every flag that takes a value is checked, not only the paths and the system prompt: a
    /// `--settings` path and the text of `--append-system-prompt` are as free as those, and the
    /// scanner refuses only a blank value or one that opens with `-` and holds no space. A tool name
    /// is checked against the tools there are, so no scan lets a hostile one through, and the
    /// printer does not rely on that. `--run-network` and `--log-level` take one of a few words the
    /// scanner parses, so a hostile value cannot reach them. The regressions it rejects are a value
    /// joined with a space (a path with a space reads as two words), a quote or `$` left active in
    /// the shell, a control character or a bidirectional control written to the terminal as it is,
    /// and quoting for some flags and not for others.
    #[test]
    fn a_hostile_value_is_one_shell_word_and_is_drawn_without_a_control_character() {
        let typed = Typed {
            deny_write: vec![
                "/my dir/it's $(id) `id`".into(),
                "/x\u{1b}]0;T\u{7}\r\n\ty\u{202e}z".into(),
            ],
            settings: Some(PathBuf::from("/s dir/it's $(id) `id`\nx.json")),
            tools: Some(vec!["read file".into(), "a'b".into()]),
            system_prompt: Some("line one\nline\ttwo".into()),
            append_system_prompt: Some("end it's $(id) `id`\nok".into()),
            ..Typed::default()
        };
        let suffix = typed.suffix();

        assert!(
            suffix.contains(r#" --sandbox-deny-write '/my dir/it'\''s $(id) `id`'"#),
            "{suffix}"
        );
        assert_eq!(suffix.chars().find(|c| c.is_control()), None, "{suffix:?}");
        assert!(!suffix.contains('\u{202e}'), "{suffix:?}");
        assert!(suffix.contains("\u{241b}]0;T\u{2407}"), "{suffix:?}");
        assert!(suffix.contains("\u{2039}U+202E\u{203A}"), "{suffix:?}");
        // The two values stayed two words of two flags.
        assert_eq!(suffix.matches(" --sandbox-deny-write ").count(), 2);
        // Text that holds a line break is drawn as a picture of it, so the system prompt is one word
        // of the line and the line is one line.
        assert!(
            suffix.contains(" --system-prompt 'line one\u{240a}line"),
            "{suffix:?}"
        );
        // Each of the other three is one quoted word with the quote and `$` inert in it.
        for word in [
            " --settings '/s dir/it'\\''s $(id) `id`\u{240a}x.json'",
            " --tools 'read file,a'\\''b'",
            " --append-system-prompt 'end it'\\''s $(id) `id`\u{240a}ok'",
        ] {
            assert!(suffix.contains(word), "{word} is not in {suffix:?}");
        }
    }
}
