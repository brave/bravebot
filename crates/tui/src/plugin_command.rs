//! `/plugin`: install a plugin from a local path, list the installed plugins, and remove, enable or
//! disable one (PLUGIN-1).
//!
//! A person's word and nobody else's: the line is dispatched from the input box like every other
//! command (CMD-1), and no tool the planner calls reaches this module. What a plugin says is never
//! read to decide anything here: the manifest reader and the install in `bravebot_agent::plugins`
//! decide, and this module only says what they answered.
//!
//! Nothing is installed unless [`confirm_install`] answers yes. It is the one place the install
//! question is put, and no permission mode, allow rule or `skip_permissions` answers it.

use bravebot_agent::plugin_said;
use bravebot_agent::plugins::{self, Asker, Installed, ManageRefusal, Staged};
use bravebot_agent::skills::Catalogue;
use bravebot_core::delegate::Definitions;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use std::path::Path;

use crate::command_table::{
    PLUGIN_ADD, PLUGIN_DISABLE, PLUGIN_ENABLE, PLUGIN_LIST, PLUGIN_REMOVE, PLUGIN_UPDATE,
};
use crate::render::printable;
use crate::state::Session;

/// The words typed after `/plugin`, read once so the command and its tests read the same forms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Typed<'a> {
    Add(&'a str),
    List,
    Remove(&'a str),
    Enable(&'a str),
    Disable(&'a str),
    Update(&'a str),
    /// A word that needs a name or a path and was given none.
    NeedsAnArgument(&'a str),
    Usage,
}

/// Read `argument`, the text typed after `/plugin`. Only the person's own words are read.
pub(crate) fn parse(argument: &str) -> Typed<'_> {
    let argument = argument.trim();
    let (word, rest) = match argument.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim()),
        None => (argument, ""),
    };
    match word {
        PLUGIN_LIST if rest.is_empty() => Typed::List,
        PLUGIN_ADD | PLUGIN_REMOVE | PLUGIN_ENABLE | PLUGIN_DISABLE | PLUGIN_UPDATE
            if rest.is_empty() =>
        {
            Typed::NeedsAnArgument(match word {
                PLUGIN_ADD => PLUGIN_ADD,
                PLUGIN_REMOVE => PLUGIN_REMOVE,
                PLUGIN_ENABLE => PLUGIN_ENABLE,
                PLUGIN_UPDATE => PLUGIN_UPDATE,
                _ => PLUGIN_DISABLE,
            })
        }
        PLUGIN_ADD => Typed::Add(rest),
        PLUGIN_REMOVE if !rest.contains(char::is_whitespace) => Typed::Remove(rest),
        PLUGIN_ENABLE if !rest.contains(char::is_whitespace) => Typed::Enable(rest),
        PLUGIN_DISABLE if !rest.contains(char::is_whitespace) => Typed::Disable(rest),
        PLUGIN_UPDATE if !rest.contains(char::is_whitespace) => Typed::Update(rest),
        _ => Typed::Usage,
    }
}

/// Put the install question to the person: the plugin name, version and source, and the parts it
/// offers. Returns whether the person said yes.
///
/// Nothing answers it but a person: no permission mode, no allow rule and no `skip_permissions`
/// reach here (PLUGIN-4). The prompt is `confirm::ask_install`, which has no request type the
/// confirmer's modes could name, and it answers no wherever it cannot be drawn or read.
/// `catalogue` and `definitions` are what this session discovered, so a name a higher-ranked
/// source takes is marked as shadowed.
pub fn confirm_install<B: Backend>(
    terminal: &mut Terminal<B>,
    staged: &Staged,
    catalogue: &Catalogue,
    definitions: &Definitions,
) -> bool {
    let request = crate::confirm::InstallRequest::from_staged(staged, catalogue, definitions);
    crate::confirm::ask_install(terminal, &request)
}

/// The terminal's side of the install question.
pub struct TerminalAsker<'t, B: Backend> {
    pub terminal: &'t mut Terminal<B>,
    pub catalogue: &'t Catalogue,
    pub definitions: &'t Definitions,
}

impl<B: Backend> Asker for TerminalAsker<'_, B> {
    fn anybody_there(&self) -> bool {
        true
    }

    fn confirm_fetch_host(&mut self, request: &bravebot_agent::confirm::FetchRequest) -> bool {
        matches!(
            crate::confirm::ask_fetch(self.terminal, request),
            crate::confirm::Answer::Approve
        )
    }

    fn confirm_install(&mut self, staged: &Staged) -> bool {
        confirm_install(self.terminal, staged, self.catalogue, self.definitions)
    }

    fn confirm_update(
        &mut self,
        staged: &Staged,
        installed: &Installed,
        before: &plugins::Before,
    ) -> bool {
        let request = crate::confirm::UpdateRequest::from_parts(
            staged,
            installed,
            before,
            self.catalogue,
            self.definitions,
        );
        crate::confirm::ask_update(self.terminal, &request)
    }
}

/// Carry out `argument`, the words typed after `/plugin`, saying what happened in the transcript.
///
/// `home` is the state directory (`~/.bravebot`) and `root` the directory a relative path is read
/// from. Where there is no state directory nothing is read or written.
pub fn run<A: Asker + ?Sized>(
    session: &mut Session,
    home: Option<&Path>,
    root: &Path,
    argument: &str,
    asker: &mut A,
) {
    let typed = parse(argument);
    if let Typed::Usage = typed {
        return session.note(t!(plugin_command_usage));
    }
    if let Typed::NeedsAnArgument(word) = typed {
        return session.note(t!(plugin_command_needs_an_argument, command = word));
    }
    let Some(home) = home else {
        return session.note(t!(plugin_command_no_home));
    };
    match typed {
        Typed::Add(path) => add(session, home, root, path, asker),
        Typed::List => list(session, home),
        Typed::Update(name) => update(session, home, name, asker),
        Typed::Remove(name) => managed(session, name, &plugins::remove(home, name), |name| {
            t!(plugin_removed, name = name)
        }),
        Typed::Enable(name) => managed(
            session,
            name,
            &plugins::set_enabled(home, name, true),
            |name| t!(plugin_enabled, name = name),
        ),
        Typed::Disable(name) => managed(
            session,
            name,
            &plugins::set_enabled(home, name, false),
            |name| t!(plugin_disabled, name = name),
        ),
        Typed::Usage | Typed::NeedsAnArgument(_) => {}
    }
}

fn add<A: Asker + ?Sized>(
    session: &mut Session,
    home: &Path,
    root: &Path,
    path: &str,
    asker: &mut A,
) {
    let result = plugins::install_typed(path, root, home, asker);
    session.note(plugin_said::added(path, &result, printable));
}

/// `/plugin update <name>` (PLUGIN-9): says what the update did. The refusal is said in words that
/// carry no byte of the source: a name is drawn through [`printable`].
fn update<A: Asker + ?Sized>(session: &mut Session, home: &Path, name: &str, asker: &mut A) {
    let result = plugins::update_typed(name, home, asker);
    session.note(plugin_said::updated(name, &result, printable));
}

fn list(session: &mut Session, home: &Path) {
    let installed = plugins::list(home);
    if installed.is_empty() {
        return session.note(t!(plugin_list_none));
    }
    for plugin in &installed {
        session.note(row(plugin));
    }
}

/// One installed plugin as a row: name, version, source, the resolved commit of a git source,
/// digest and whether it is enabled (R44, PLUGIN-9). Every field but the digest is text the plugin
/// or the person wrote, so each is drawn through [`printable`].
pub(crate) fn row(plugin: &Installed) -> String {
    let state = match plugin.enabled {
        true => t!(plugin_list_enabled),
        false => t!(plugin_list_disabled),
    };
    match plugin.source.commit() {
        Some(commit) => t!(
            plugin_list_row_git,
            name = printable(&plugin.name),
            version = printable(&plugin.version),
            source = printable(&plugin.source.to_string()),
            commit = printable(commit.as_str()),
            digest = printable(&plugin.digest),
            state = state
        )
        .to_string(),
        None => t!(
            plugin_list_row,
            name = printable(&plugin.name),
            version = printable(&plugin.version),
            source = printable(&plugin.source.to_string()),
            digest = printable(&plugin.digest),
            state = state
        )
        .to_string(),
    }
}

fn managed(
    session: &mut Session,
    name: &str,
    result: &Result<(), ManageRefusal>,
    done: impl FnOnce(String) -> String,
) {
    session.note(match result {
        Ok(()) => done(printable(name)),
        Err(refusal) => plugin_said::manage_refused(name, refusal, printable),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Speaker;
    use bravebot_agent::plugins::Nobody;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::path::PathBuf;

    /// The asker `TerminalAsker` is, with the keys of a person scripted in place of the terminal.
    /// It draws the real prompt, so a test reads what was shown.
    struct Scripted {
        keys: std::collections::VecDeque<KeyCode>,
        terminal: Terminal<TestBackend>,
        asked: usize,
        updates_asked: usize,
        hosts_asked: usize,
        screen: String,
        with: Option<(Catalogue, Definitions)>,
    }

    impl Scripted {
        fn new(keys: &[KeyCode]) -> Self {
            Self {
                keys: keys.iter().copied().collect(),
                terminal: Terminal::new(TestBackend::new(100, 60)).unwrap(),
                asked: 0,
                updates_asked: 0,
                hosts_asked: 0,
                screen: String::new(),
                with: None,
            }
        }
    }

    impl Asker for Scripted {
        fn anybody_there(&self) -> bool {
            true
        }

        fn confirm_fetch_host(&mut self, request: &bravebot_agent::confirm::FetchRequest) -> bool {
            self.hosts_asked += 1;
            let keys = &mut self.keys;
            let answer = crate::confirm::ask_fetch_reading(&mut self.terminal, request, || {
                match keys.pop_front() {
                    Some(code) => {
                        crate::confirm::Next::Press(KeyEvent::new(code, KeyModifiers::NONE))
                    }
                    None => crate::confirm::Next::Lost,
                }
            });
            let buffer = self.terminal.backend().buffer().clone();
            self.screen = (0..buffer.area.height)
                .map(|y| {
                    (0..buffer.area.width)
                        .map(|x| buffer[(x, y)].symbol().to_string())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            matches!(answer, crate::confirm::Answer::Approve)
        }

        fn confirm_install(&mut self, staged: &Staged) -> bool {
            self.asked += 1;
            let (catalogue, definitions) = self.with.clone().unwrap_or_default();
            let request =
                crate::confirm::InstallRequest::from_staged(staged, &catalogue, &definitions);
            let keys = &mut self.keys;
            let answer = crate::confirm::ask_install_reading(&mut self.terminal, &request, || {
                match keys.pop_front() {
                    Some(code) => {
                        crate::confirm::Next::Press(KeyEvent::new(code, KeyModifiers::NONE))
                    }
                    None => crate::confirm::Next::Lost,
                }
            });
            let buffer = self.terminal.backend().buffer().clone();
            self.screen = (0..buffer.area.height)
                .map(|y| {
                    (0..buffer.area.width)
                        .map(|x| buffer[(x, y)].symbol().to_string())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            answer
        }

        fn confirm_update(
            &mut self,
            staged: &Staged,
            installed: &Installed,
            before: &plugins::Before,
        ) -> bool {
            self.updates_asked += 1;
            let (catalogue, definitions) = self.with.clone().unwrap_or_default();
            let request = crate::confirm::UpdateRequest::from_parts(
                staged,
                installed,
                before,
                &catalogue,
                &definitions,
            );
            let keys = &mut self.keys;
            let answer = crate::confirm::ask_update_reading(&mut self.terminal, &request, || {
                match keys.pop_front() {
                    Some(code) => {
                        crate::confirm::Next::Press(KeyEvent::new(code, KeyModifiers::NONE))
                    }
                    None => crate::confirm::Next::Lost,
                }
            });
            let buffer = self.terminal.backend().buffer().clone();
            self.screen = (0..buffer.area.height)
                .map(|y| {
                    (0..buffer.area.width)
                        .map(|x| buffer[(x, y)].symbol().to_string())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            answer
        }
    }

    /// An asker that answers as told and counts the questions.
    struct Answering {
        yes: bool,
        asked: usize,
    }

    impl Asker for Answering {
        fn anybody_there(&self) -> bool {
            true
        }

        fn confirm_fetch_host(&mut self, _request: &bravebot_agent::confirm::FetchRequest) -> bool {
            false
        }

        fn confirm_install(&mut self, _staged: &Staged) -> bool {
            self.asked += 1;
            self.yes
        }

        fn confirm_update(
            &mut self,
            _staged: &Staged,
            _installed: &Installed,
            _before: &plugins::Before,
        ) -> bool {
            self.asked += 1;
            self.yes
        }
    }

    fn said(session: &Session) -> Vec<String> {
        session
            .transcript
            .iter()
            .filter(|entry| entry.speaker == Speaker::System)
            .map(|entry| entry.text.clone())
            .collect()
    }

    /// A plugin called `name` offering one skill, in `parent`.
    fn plugin_named(parent: &Path, name: &str) -> PathBuf {
        let root = parent.join(format!("src-{name}"));
        std::fs::create_dir_all(root.join("skills").join("hello")).unwrap();
        std::fs::write(
            root.join("plugin.toml"),
            format!(
                "name = \"{name}\"\nversion = \"2.3.4\"\ndescription = \"d\"\nskills = \"skills\"\n"
            ),
        )
        .unwrap();
        std::fs::write(
            root.join("skills").join("hello").join("SKILL.md"),
            "---\nname: hello\ndescription: Says hello.\n---\n\nBody.\n",
        )
        .unwrap();
        root
    }

    fn state_is_untouched(home: &Path) -> bool {
        !home.exists()
    }

    /// R19: the subcommand words are read from what the person typed and from nothing else.
    ///
    /// Rejects: a word that carries out a subcommand it does not name, and a name with spaces
    /// in it reaching remove, enable or disable.
    #[test]
    fn the_words_after_the_command_read_as_the_six_forms_and_nothing_else() {
        assert_eq!(parse("add ./demo"), Typed::Add("./demo"));
        assert_eq!(
            parse("add   a path with spaces"),
            Typed::Add("a path with spaces")
        );
        assert_eq!(parse("list"), Typed::List);
        assert_eq!(parse("remove demo"), Typed::Remove("demo"));
        assert_eq!(parse("enable demo"), Typed::Enable("demo"));
        assert_eq!(parse("disable demo"), Typed::Disable("demo"));
        assert_eq!(parse("update demo"), Typed::Update("demo"));
        assert_eq!(parse("update"), Typed::NeedsAnArgument("update"));
        assert_eq!(parse("add"), Typed::NeedsAnArgument("add"));
        assert_eq!(parse("remove"), Typed::NeedsAnArgument("remove"));
        for other in [
            "",
            "list demo",
            "remove a b",
            "install demo",
            "ADD x",
            "update a b",
            "UPDATE demo",
        ] {
            assert_eq!(parse(other), Typed::Usage, "{other:?}");
        }
    }

    /// R20, R21: where nobody can be asked nothing is installed, and the person is told why.
    ///
    /// Rejects: an install that goes ahead with no asker, and a refusal that says nothing.
    #[test]
    fn add_where_nobody_can_be_asked_installs_nothing_and_says_so() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        plugin_named(scratch.path(), "demo");
        let mut session = Session::new("none");
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "add src-demo",
            &mut Nobody,
        );
        assert!(state_is_untouched(&home));
        let said = said(&session);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].contains("nobody can be asked"), "{said:?}");
    }

    /// R22: `/plugin add` is answered by a person and by nothing else. The asker is handed no
    /// permission mode, no rule and no `skip_permissions`, so each mode, the ladder's bypass
    /// included, still puts the prompt, and a prompt that is not answered installs nothing.
    ///
    /// Rejects: a mode answering the prompt for the person, and a lost terminal that installs.
    #[test]
    fn every_permission_mode_still_puts_the_prompt_to_the_person() {
        let mut modes_seen = Vec::new();
        let mut session = Session::new("none");
        for _ in 0..5 {
            modes_seen.push(session.permission_mode());
            let scratch = tempfile::tempdir().unwrap();
            let home = scratch.path().join("home");
            plugin_named(scratch.path(), "demo");
            let mut asker = Scripted::new(&[KeyCode::Char('n')]);
            run(
                &mut session,
                Some(&home),
                scratch.path(),
                "add src-demo",
                &mut asker,
            );
            assert_eq!(
                asker.asked,
                1,
                "{:?} did not put the prompt",
                session.permission_mode()
            );
            assert!(state_is_untouched(&home));
            // A prompt nobody answers installs nothing either.
            let mut lost = Scripted::new(&[]);
            run(
                &mut session,
                Some(&home),
                scratch.path(),
                "add src-demo",
                &mut lost,
            );
            assert!(state_is_untouched(&home), "{:?}", session.permission_mode());
            session.cycle_permission_mode();
        }
        use bravebot_agent::PermissionMode::*;
        for mode in [Ask, AcceptEdits, Plan, Auto, Bypass] {
            assert!(
                modes_seen.contains(&mode),
                "{mode:?} not exercised: {modes_seen:?}"
            );
        }
    }

    /// R22: a yes typed by the person installs, after one question.
    ///
    /// Rejects: a y that does not install.
    #[test]
    fn a_typed_yes_installs_after_one_prompt() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        plugin_named(scratch.path(), "demo");
        let mut session = Session::new("none");
        let mut asker = Scripted::new(&[KeyCode::Char('y')]);
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "add src-demo",
            &mut asker,
        );
        assert_eq!(asker.asked, 1);
        assert!(home.join("plugins").join("demo").exists());
        assert!(asker.screen.contains("2.3.4"), "{}", asker.screen);
    }

    /// R29: a plugin skill named like a skill in the user's own directory is installed, is marked
    /// shadowed in the prompt, and the home skill is the one a session offers afterwards.
    ///
    /// Rejects: a missing mark, a plugin skill that wins, and a shadowed skill that is not
    /// installed.
    #[test]
    fn a_plugin_skill_named_like_a_home_skill_is_marked_and_the_home_one_is_offered() {
        use bravebot_agent::skills::{self, Source};
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let own = home.join("skills").join("hello");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(
            own.join("SKILL.md"),
            "---\nname: hello\ndescription: The home one.\n---\n\nHome body.\n",
        )
        .unwrap();
        plugin_named(scratch.path(), "demo");
        let project = scratch.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let workspace = bravebot_agent::workspace::Workspace::new(&project).unwrap();
        let offered = |home: &Path| {
            skills::resolved(
                &workspace,
                Some(home),
                bravebot_core::trust::TrustStore::new(workspace.root()),
                bravebot_core::permissions::Permissions::default(),
                &mut bravebot_session::audit::Trail::new(),
            )
        };
        let before = offered(&home);
        let mut asker = Scripted::new(&[KeyCode::Char('y')]);
        asker.with = Some((before.clone(), Definitions::default()));
        let mut session = Session::new("none");
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "add src-demo",
            &mut asker,
        );

        assert!(asker.screen.contains("Says hello."), "{}", asker.screen);
        assert!(asker.screen.contains("Shadowed by"), "{}", asker.screen);
        assert!(home.join("plugins").join("demo").exists());
        let after = offered(&home);
        let skill = after.get("hello").expect("a skill named hello is offered");
        assert_eq!(skill.source, Source::Home);
        assert_eq!(skill.description, "The home one.");
    }

    /// A no copies nothing and records nothing, and the prompt is not a place Enter answers.
    ///
    /// Rejects: a no that still copies the plugin or writes the record.
    #[test]
    fn a_no_copies_and_records_nothing() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        plugin_named(scratch.path(), "demo");
        let mut session = Session::new("none");
        let mut asker = Scripted::new(&[KeyCode::Enter, KeyCode::Char('n')]);
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "add src-demo",
            &mut asker,
        );
        assert!(state_is_untouched(&home));
        assert!(
            said(&session)[0].contains("declined"),
            "{:?}",
            said(&session)
        );
    }

    /// A yes installs once after one question, and a no installs nothing.
    ///
    /// Rejects: an answer that is ignored, and a question put twice.
    #[test]
    fn add_installs_only_on_a_yes_after_one_question() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        plugin_named(scratch.path(), "demo");
        let mut session = Session::new("none");
        let mut no = Answering {
            yes: false,
            asked: 0,
        };
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "add src-demo",
            &mut no,
        );
        assert_eq!((no.asked, state_is_untouched(&home)), (1, true));
        let mut yes = Answering {
            yes: true,
            asked: 0,
        };
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "add src-demo",
            &mut yes,
        );
        assert_eq!(yes.asked, 1);
        assert!(bravebot_agent::plugins::read_record(&home, "demo").is_some());
        assert!(
            said(&session)[1].contains("Installed demo"),
            "{:?}",
            said(&session)
        );
    }

    /// R44: list prints name, version, source, digest and whether the plugin is enabled.
    ///
    /// Rejects: a row that leaves one of the five out.
    #[test]
    fn list_prints_name_version_source_digest_and_enabled() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_named(scratch.path(), "demo");
        let mut session = Session::new("none");
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "list",
            &mut Nobody,
        );
        assert!(said(&session)[0].contains("No plugin is installed"));
        bravebot_agent::plugins::install(&source, &home).unwrap();
        let record = bravebot_agent::plugins::read_record(&home, "demo").unwrap();
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "list",
            &mut Nobody,
        );
        let row = said(&session).pop().unwrap();
        for part in [
            "demo",
            "2.3.4",
            &record.source.to_string(),
            &record.digest,
            "enabled",
        ] {
            assert!(row.contains(part), "{part} missing from {row}");
        }
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "disable demo",
            &mut Nobody,
        );
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "list",
            &mut Nobody,
        );
        assert!(said(&session).pop().unwrap().contains("disabled"));
    }

    /// PLUGIN-9: a git source's row prints the full resolved commit beside the URL, and a local
    /// path's row prints no commit.
    ///
    /// Rejects: a row that leaves the commit out, one that cuts it short, and a commit label on a
    /// local row.
    #[test]
    fn list_prints_the_resolved_commit_of_a_git_source_and_none_for_a_path() {
        use bravebot_agent::plugin_source::{Commit, Source};
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let mut plugin = Installed {
            name: "demo".into(),
            source: Source::Git {
                url: "https://example.com/demo.git".into(),
                requested_ref: Some("main".into()),
                commit: Commit::parse(commit).unwrap(),
            },
            version: "1.0.0".into(),
            enabled: true,
            digest: "d1".into(),
        };
        let git = row(&plugin);
        for part in ["https://example.com/demo.git", commit, "d1", "enabled"] {
            assert!(git.contains(part), "{part} missing from {git}");
        }
        plugin.source = Source::Local("/a/b".into());
        let local = row(&plugin);
        assert!(
            local.contains("/a/b") && !local.contains("commit"),
            "{local}"
        );
    }

    /// PLUGIN-3: text that is not a source is refused in words, naming the kind, before any
    /// question.
    ///
    /// Rejects: a refusal that is silent or says only that a directory was unreadable, and an
    /// asker reached for a refused source.
    #[test]
    fn add_refuses_an_archive_an_npm_name_and_a_plain_url_in_words_without_asking() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let mut session = Session::new("none");
        let mut asker = Answering {
            yes: true,
            asked: 0,
        };
        for typed in [
            "add https://example.com/x.tar.gz",
            "add @scope/pkg",
            "add https://example.com/docs",
            "add file:///tmp/p",
            "add https://example.com/x.git#-x",
        ] {
            run(&mut session, Some(&home), scratch.path(), typed, &mut asker);
        }
        let said = said(&session);
        for (note, expected) in said.iter().zip([
            "an archive",
            "an npm package",
            "not a git url",
            "file://",
            "starts with -",
        ]) {
            assert!(note.contains(expected), "{expected} missing from {note}");
        }
        assert_eq!(said.len(), 5, "{said:?}");
        assert_eq!(asker.asked, 0);
        assert!(state_is_untouched(&home));
    }

    /// PLUGIN-3, FETCH-2: a git url is put to the person in the prompt a fetch uses, with the host
    /// the parser read on its own row above the url, and a no installs nothing, starts nothing and
    /// says the install was declined. The install question is never put.
    ///
    /// Rejects: a host question that draws the host from the text before the first slash (the
    /// userinfo `example.com` of the url below is not the host), a url missing from the prompt, and
    /// a no that goes on to fetch or to ask the install question.
    #[test]
    fn a_git_url_is_put_in_the_fetch_prompt_with_its_host_apart_and_a_no_ends_it() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let mut session = Session::new("none");
        let mut asker = Scripted::new(&[KeyCode::Char('n')]);
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "add ssh://example.com@evil.test/x.git#main",
            &mut asker,
        );
        let lines: Vec<&str> = asker.screen.lines().map(str::trim_end).collect();
        let host_row = lines
            .iter()
            .position(|line| line.contains("talking to evil.test"))
            .unwrap_or_else(|| panic!("no host row in {}", asker.screen));
        let url_row = lines
            .iter()
            .position(|line| line.contains("Fetch ssh://example.com@evil.test/x.git"))
            .unwrap_or_else(|| panic!("no url row in {}", asker.screen));
        assert!(host_row < url_row, "{}", asker.screen);
        assert!(!lines[host_row].contains("example.com"), "{}", asker.screen);
        assert_eq!((asker.hosts_asked, asker.asked), (1, 0));
        assert!(state_is_untouched(&home));
        assert!(
            said(&session)[0].contains("declined"),
            "{:?}",
            said(&session)
        );
    }

    /// PLUGIN-3, PLUGIN-4: every permission mode, the bypass among them, still puts the host
    /// question to the person, and a question nobody answers lets nothing leave.
    ///
    /// Rejects: a mode that answers the host question (`asked` would stay at zero and the state
    /// directory would be written), and a lost terminal that counts as a yes.
    #[test]
    fn every_permission_mode_still_puts_the_host_question_and_a_lost_terminal_is_a_no() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let mut session = Session::new("none");
        for _ in 0..5 {
            let mut asker = Scripted::new(&[KeyCode::Char('n')]);
            run(
                &mut session,
                Some(&home),
                scratch.path(),
                "add https://example.com/x.git",
                &mut asker,
            );
            assert_eq!(asker.hosts_asked, 1, "{:?}", session.permission_mode());
            let mut lost = Scripted::new(&[]);
            run(
                &mut session,
                Some(&home),
                scratch.path(),
                "add https://example.com/x.git",
                &mut lost,
            );
            assert_eq!(lost.hosts_asked, 1);
            assert!(state_is_untouched(&home), "{:?}", session.permission_mode());
            session.cycle_permission_mode();
        }
    }

    /// R14 to R16: remove, enable and disable act on the installed plugin without any question,
    /// and say so when the name is not one.
    ///
    /// Rejects: a question put for enable, and a silent miss.
    #[test]
    fn remove_enable_and_disable_act_without_a_question_and_miss_in_words() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_named(scratch.path(), "demo");
        bravebot_agent::plugins::install(&source, &home).unwrap();
        let mut session = Session::new("none");
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "disable demo",
            &mut Nobody,
        );
        assert!(
            !bravebot_agent::plugins::read_record(&home, "demo")
                .unwrap()
                .enabled
        );
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "enable demo",
            &mut Nobody,
        );
        assert!(
            bravebot_agent::plugins::read_record(&home, "demo")
                .unwrap()
                .enabled
        );
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "remove demo",
            &mut Nobody,
        );
        assert!(bravebot_agent::plugins::read_record(&home, "demo").is_none());
        assert!(!bravebot_agent::plugins::installed_dir(&home, "demo").exists());
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "remove demo",
            &mut Nobody,
        );
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "remove ../x",
            &mut Nobody,
        );
        let said = said(&session);
        assert!(said[3].contains("No plugin named demo"), "{said:?}");
        assert!(said[4].contains("not the name of a plugin"), "{said:?}");
    }

    /// With no state directory nothing is read or written.
    #[test]
    fn without_a_state_directory_nothing_is_read_or_written() {
        let mut session = Session::new("none");
        run(&mut session, None, Path::new("."), "add x", &mut Nobody);
        assert!(said(&session)[0].contains("no state directory"));
    }

    /// PLUGIN-1, PLUGIN-9: `/plugin update <name>` is a typed command that is dispatched, not the
    /// usage line and not a refusal that says update is not built. A bare `update` asks for a name,
    /// a miss and a bad name are said in words, nobody to ask updates nothing, an unchanged source
    /// says so and asks nothing, and a changed one puts one question that a no leaves unanswered
    /// for the state directory.
    ///
    /// Rejects: an update that falls to the usage line, one that says it is not built, one that
    /// asks about a source whose files did not change, and one that writes anything on a no.
    #[test]
    fn update_is_dispatched_and_says_what_it_did_in_words() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_named(scratch.path(), "demo");
        bravebot_agent::plugins::install(&source, &home).unwrap();
        let recorded = std::fs::read_to_string(home.join("plugins").join("demo.record")).unwrap();
        let mut session = Session::new("none");
        let mut asker = Answering {
            yes: true,
            asked: 0,
        };
        for line in [
            "update",
            "update absent",
            "update ../x",
            "update demo",
            "update a b",
        ] {
            run(&mut session, Some(&home), scratch.path(), line, &mut asker);
        }
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "update demo",
            &mut Nobody,
        );
        let notes = said(&session);
        assert!(notes[0].contains("/plugin update needs"), "{notes:?}");
        assert!(notes[1].contains("No plugin named absent"), "{notes:?}");
        assert!(notes[2].contains("not the name of a plugin"), "{notes:?}");
        assert!(notes[3].contains("up to date"), "{notes:?}");
        assert!(notes[4].contains("/plugin takes"), "{notes:?}");
        assert!(notes[5].contains("nobody can be asked"), "{notes:?}");
        assert!(notes.iter().all(|note| !note.contains("not built")));
        assert_eq!(asker.asked, 0);

        std::fs::write(
            source.join("skills").join("hello").join("SKILL.md"),
            "---\nname: hello\ndescription: Says more.\n---\n\nBody.\n",
        )
        .unwrap();
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "update demo",
            &mut Nobody,
        );
        assert!(
            said(&session)
                .pop()
                .unwrap()
                .contains("nobody can be asked"),
            "{:?}",
            said(&session)
        );
        assert_eq!(
            std::fs::read_to_string(home.join("plugins").join("demo.record")).unwrap(),
            recorded
        );
        assert_eq!(
            bravebot_agent::plugins::tree_matches_record(&home, "demo"),
            Ok(())
        );
    }

    /// PLUGIN-1, PLUGIN-5: `/plugin add` of a source that is installed does not say that update is
    /// not built.
    ///
    /// Rejects: an add of the installed source that answers with the old text, which said update
    /// was not built.
    #[test]
    fn adding_an_installed_source_points_at_update_and_does_not_say_it_is_not_built() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_named(scratch.path(), "demo");
        bravebot_agent::plugins::install(&source, &home).unwrap();
        let mut session = Session::new("none");
        let mut asker = Answering {
            yes: true,
            asked: 0,
        };
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "add src-demo",
            &mut asker,
        );
        let notes = said(&session);
        assert!(notes[0].contains("/plugin update demo"), "{notes:?}");
        assert!(!notes[0].contains("not built"), "{notes:?}");
    }

    /// A source `plugin_named` made, edited so that `hello` reads differently, a skill `bye` is
    /// added and the version is higher.
    fn edit_the_source(source: &Path) {
        std::fs::write(
            source.join("skills").join("hello").join("SKILL.md"),
            "---\nname: hello\ndescription: Says more.\n---\n\nBody.\n",
        )
        .unwrap();
        std::fs::create_dir_all(source.join("skills").join("bye")).unwrap();
        std::fs::write(
            source.join("skills").join("bye").join("SKILL.md"),
            "---\nname: bye\ndescription: Says bye.\n---\n\nBody.\n",
        )
        .unwrap();
        std::fs::write(
            source.join("plugin.toml"),
            "name = \"demo\"\nversion = \"2.4.0\"\ndescription = \"d\"\nskills = \"skills\"\n",
        )
        .unwrap();
    }

    /// PLUGIN-9, PLUGIN-4: a changed source puts one prompt that names the plugin, both versions and
    /// the source and shows the added and the changed skill behind the margin. A typed y replaces
    /// the plugin and the person is told. Any other answer, and a terminal nobody answers, leave the
    /// directory and the record as they were.
    ///
    /// Rejects: a prompt that lacks the version change or the source, a part of the difference that
    /// is not drawn, plugin text drawn without the margin, an `n` or a lost terminal that updates,
    /// and a y that does not.
    #[test]
    fn a_changed_source_puts_the_difference_and_only_a_typed_yes_updates() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_named(scratch.path(), "demo");
        bravebot_agent::plugins::install(&source, &home).unwrap();
        let recorded = std::fs::read_to_string(home.join("plugins").join("demo.record")).unwrap();
        edit_the_source(&source);
        let mut session = Session::new("none");

        for keys in [vec![KeyCode::Char('n')], vec![KeyCode::Enter], vec![]] {
            let mut asker = Scripted::new(&keys);
            run(
                &mut session,
                Some(&home),
                scratch.path(),
                "update demo",
                &mut asker,
            );
            assert_eq!(asker.updates_asked, 1, "{keys:?}");
            assert_eq!(
                std::fs::read_to_string(home.join("plugins").join("demo.record")).unwrap(),
                recorded,
                "{keys:?}"
            );
        }
        assert!(
            said(&session)
                .iter()
                .all(|note| note.contains("declined") || note.contains("Nothing was updated")),
            "{:?}",
            said(&session)
        );

        let mut asker = Scripted::new(&[KeyCode::Char('y')]);
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "update demo",
            &mut asker,
        );
        assert_eq!(asker.updates_asked, 1);
        for shown in [
            "Update plugin",
            "demo",
            "2.3.4 to 2.4.0",
            &source.canonicalize().unwrap().display().to_string(),
            "Skills: 1 added, 0 removed, 1 changed",
            "Added",
            "bye",
            "Says bye.",
            "Changed",
            "Says hello.",
            "Says more.",
            "Update this plugin?",
            "Yes, update it",
            "No, keep the installed copy",
            "changed plugin.toml",
            "added skills/bye/SKILL.md",
        ] {
            assert!(asker.screen.contains(shown), "{shown}\n{}", asker.screen);
        }
        let row = asker
            .screen
            .lines()
            .find(|row| row.contains("Says bye."))
            .expect("the added skill is drawn");
        assert!(row.contains('\u{2503}'), "{row}");
        let record = bravebot_agent::plugins::read_record(&home, "demo").unwrap();
        assert_eq!(record.version, "2.4.0");
        assert_eq!(
            bravebot_agent::plugins::tree_matches_record(&home, "demo"),
            Ok(())
        );
        assert!(
            said(&session).pop().unwrap().contains("Updated demo"),
            "{:?}",
            said(&session)
        );
    }

    /// PLUGIN-4, PLUGIN-9: no permission mode, the bypass among them, answers the update question.
    /// Each puts the prompt, and a prompt nobody answers updates nothing.
    ///
    /// Rejects: a mode that answers for the person (the question would not be put and the plugin
    /// would change, or stay unasked).
    #[test]
    fn every_permission_mode_still_puts_the_update_prompt_to_the_person() {
        let mut session = Session::new("none");
        for _ in 0..5 {
            let scratch = tempfile::tempdir().unwrap();
            let home = scratch.path().join("home");
            let source = plugin_named(scratch.path(), "demo");
            bravebot_agent::plugins::install(&source, &home).unwrap();
            edit_the_source(&source);
            let mut asker = Scripted::new(&[KeyCode::Char('n')]);
            run(
                &mut session,
                Some(&home),
                scratch.path(),
                "update demo",
                &mut asker,
            );
            assert_eq!(
                asker.updates_asked,
                1,
                "{:?} did not put the prompt",
                session.permission_mode()
            );
            let mut lost = Scripted::new(&[]);
            run(
                &mut session,
                Some(&home),
                scratch.path(),
                "update demo",
                &mut lost,
            );
            assert_eq!(
                bravebot_agent::plugins::read_record(&home, "demo")
                    .unwrap()
                    .version,
                "2.3.4",
                "{:?}",
                session.permission_mode()
            );
            session.cycle_permission_mode();
        }
    }

    /// PLUGIN-9: an update that would offer a skill another plugin offers is refused in words naming
    /// both plugins, with the prompt never put.
    ///
    /// Rejects: a refusal that names one plugin, and a prompt put for an update that cannot be made.
    #[test]
    fn an_update_into_a_name_another_plugin_offers_is_said_naming_both_and_asks_nothing() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = plugin_named(scratch.path(), "demo");
        bravebot_agent::plugins::install(&source, &home).unwrap();
        let other = plugin_named(scratch.path(), "other");
        std::fs::rename(
            other.join("skills").join("hello"),
            other.join("skills").join("review"),
        )
        .unwrap();
        std::fs::write(
            other.join("skills").join("review").join("SKILL.md"),
            "---\nname: review\ndescription: Reviews.\n---\n\nBody.\n",
        )
        .unwrap();
        bravebot_agent::plugins::install(&other, &home).unwrap();
        std::fs::rename(
            source.join("skills").join("hello"),
            source.join("skills").join("review"),
        )
        .unwrap();
        std::fs::write(
            source.join("skills").join("review").join("SKILL.md"),
            "---\nname: review\ndescription: Also reviews.\n---\n\nBody.\n",
        )
        .unwrap();
        let mut session = Session::new("none");
        let mut asker = Scripted::new(&[KeyCode::Char('y')]);
        run(
            &mut session,
            Some(&home),
            scratch.path(),
            "update demo",
            &mut asker,
        );
        assert_eq!(asker.updates_asked, 0);
        let note = said(&session).pop().unwrap();
        for part in ["Nothing was updated for demo", "review", "demo", "other"] {
            assert!(note.contains(part), "{part} missing from {note}");
        }
        assert!(
            bravebot_agent::plugins::installed_dir(&home, "demo")
                .join("skills")
                .join("hello")
                .is_dir()
        );
    }
}
