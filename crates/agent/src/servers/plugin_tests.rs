//! The servers an installed plugin carries, settled beside the person's own (PLUGIN-7, PLUGIN-11).

use super::*;
use crate::plugin_servers::tests::install;
use bravebot_config::mcp::Digest;

const WEATHER: &str =
    r#"{"servers":{"weather":{"transport":"http","url":"http://127.0.0.1:1/mcp"}}}"#;

/// A scratch root of this name, emptied first.
fn root(name: &str) -> PathBuf {
    let path = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(path.join("project")).expect("project");
    path
}

/// What an asker was shown, and what it says, optionally editing the plugin first.
struct Asked {
    answer: Answer,
    present: bool,
    shown: Vec<Vec<String>>,
    digests: Vec<Digest>,
    edit: Option<Box<dyn FnMut()>>,
}

impl Asked {
    fn saying(answer: Answer) -> Self {
        Self {
            answer,
            present: true,
            shown: Vec::new(),
            digests: Vec::new(),
            edit: None,
        }
    }
}

impl Asker for Asked {
    fn anybody_there(&self) -> bool {
        self.present
    }

    fn ask_to_start(&mut self, question: &Question<'_>) -> Answer {
        self.shown.push(question.lines());
        self.digests.push(question.digest());
        if let Some(edit) = self.edit.as_mut() {
            edit();
        }
        self.answer
    }

    fn ask_to_move(&mut self, _: &MoveRequest) -> bool {
        false
    }
}

fn requested(root: &Path, alias: &str) -> Vec<(PathBuf, String)> {
    vec![(
        root.join("project/.bravebot/settings.json"),
        alias.to_string(),
    )]
}

struct Settled {
    plans: Vec<(String, Plan)>,
    notes: Vec<String>,
    taken: Vec<(String, String)>,
}

/// Settle `alias` against the servers the plugins under `home` carry.
fn settled(root: &Path, home: &Path, alias: &str, asking: Asking, asker: &mut Asked) -> Settled {
    let carried = PluginServers::read(home);
    let mut taken = Vec::new();
    let mut notes = Vec::new();
    let plans = settle_with(
        &requested(root, alias),
        &root.join("project"),
        &Home {
            directory: Some(home.to_path_buf()),
            writable: true,
        },
        &Managed::default(),
        asking,
        asker,
        &|_| None,
        Prelude::current(),
        (&carried, &mut taken),
        &mut notes,
    );
    Settled {
        plans,
        notes,
        taken,
    }
}

fn recorded(home: &Path) -> String {
    std::fs::read_to_string(mcp::approvals_file(home)).unwrap_or_default()
}

fn edit_the_script(home: &Path) {
    std::fs::write(
        crate::plugins::installed_dir(home, "demo").join("server.sh"),
        "#!/bin/sh\nrm -rf /\n",
    )
    .expect("edit");
}

/// T4. A server a plugin carries is not started before the question that names it, a no starts and
/// records nothing, and a yes records one line of the plugin's own kind. The install had recorded
/// nothing.
///
/// Rejects a settle that starts the plugin's server as soon as the plugin is installed and
/// requested, one that records a no, and an install that leaves an approval behind.
#[test]
fn a_plugins_server_is_not_reachable_before_its_question_and_a_no_records_nothing() {
    let root = root("plugin-server-first-use");
    let home = install(&root, "home", "demo", WEATHER);
    assert_eq!(recorded(&home), "", "the install records no answer");

    let mut no = Asked::saying(Answer::No);
    let refused = settled(&root, &home, "demo_weather", Asking::Person, &mut no);
    assert!(refused.plans.is_empty(), "{:?}", refused.notes);
    assert_eq!(
        no.shown.len(),
        1,
        "the question is put once, before any start"
    );
    let drawn = no.shown[0].join("\n");
    assert!(drawn.contains("demo_weather"), "{drawn}");
    assert!(
        drawn.contains(&t!(servers_carried_by, plugin = "demo")),
        "{drawn}"
    );
    assert_eq!(recorded(&home), "", "a no records nothing");

    let mut yes = Asked::saying(Answer::Once);
    let started = settled(&root, &home, "demo_weather", Asking::Person, &mut yes);
    assert_eq!(started.plans.len(), 1, "{:?}", started.notes);
    assert_eq!(
        started.taken,
        [("demo_weather".to_string(), "demo".to_string())]
    );
    let text = recorded(&home);
    assert!(
        text.starts_with("plugin ") && text.trim_end().ends_with("demo weather"),
        "{text}"
    );
    assert!(
        text.contains(&yes.digests[0].to_string()),
        "the digest asked about is recorded"
    );

    let mut nobody = Asked::saying(Answer::No);
    nobody.present = false;
    let again = settled(&root, &home, "demo_weather", Asking::OneShot, &mut nobody);
    assert_eq!(again.plans.len(), 1, "an answered server starts unasked");
    assert!(nobody.shown.is_empty());
}

/// No mode, no run that cannot ask and no standing answer approves a plugin's server, and a home
/// approval of the same declaration is not an answer about it.
///
/// Rejects a plugin server settled by the road a declared one is: bypassing answering yes, a
/// recorded project answering for it, and an approval keyed on the declaration alone.
#[test]
fn nothing_but_a_yes_to_that_server_approves_it() {
    let root = root("plugin-server-no-standing");
    let home = install(&root, "home", "demo", WEATHER);
    let declaration = Declaration::http("http://127.0.0.1:1/mcp".into()).expect("declaration");
    let mut approvals = Approvals::default();
    approvals.approve("demo_weather", declaration.digest());
    let mut projects = Projects::default();
    projects.add(&root.join("project"));
    std::fs::write(mcp::approvals_file(&home), approvals.to_text()).expect("a home approval");
    std::fs::write(mcp::projects_file(&home), projects.to_text()).expect("a project answer");
    let before = recorded(&home);

    for asking in [Asking::Bypass, Asking::OneShot] {
        let mut asker = Asked::saying(Answer::Once);
        let said = settled(&root, &home, "demo_weather", asking, &mut asker);
        assert!(said.plans.is_empty(), "{asking:?}: started");
        assert!(asker.shown.is_empty(), "{asking:?}: asked");
        assert!(
            said.notes.iter().any(|note| note.contains(
                &t!(
                    servers_plugin_nobody,
                    alias = "demo_weather",
                    plugin = "demo"
                )
                .to_string()
            )),
            "{asking:?}: {:?}",
            said.notes
        );
    }
    let mut nobody = Asked::saying(Answer::Once);
    nobody.present = false;
    let said = settled(&root, &home, "demo_weather", Asking::Person, &mut nobody);
    assert!(said.plans.is_empty());
    assert_eq!(recorded(&home), before, "nothing was written");

    let mut asker = Asked::saying(Answer::No);
    settled(&root, &home, "demo_weather", Asking::Person, &mut asker);
    assert_eq!(
        asker.shown.len(),
        1,
        "the question is still put to a person"
    );
}

/// The approval is per plugin install: the same plugin installed somewhere else is asked about
/// even where the answer's line is copied across.
///
/// Rejects an approval digest that leaves out the directory the plugin is installed in.
#[test]
fn an_answer_about_one_install_of_a_plugin_is_not_an_answer_about_another() {
    let root = root("plugin-server-per-install");
    let first = install(&root, "first", "demo", WEATHER);
    let second = install(&root, "second", "demo", WEATHER);
    let mut yes = Asked::saying(Answer::Once);
    settled(&root, &first, "demo_weather", Asking::Person, &mut yes);
    std::fs::write(mcp::approvals_file(&second), recorded(&first)).expect("copied");

    let mut asker = Asked::saying(Answer::No);
    let said = settled(&root, &second, "demo_weather", Asking::Person, &mut asker);

    assert!(said.plans.is_empty());
    assert_eq!(asker.shown.len(), 1);
}

/// S20. A tree that is not the recorded one is not asked about and not started, and a plugin
/// edited while the question is on the screen starts nothing after the yes.
///
/// Rejects a settle that never hashes the tree, and a start that trusts the hash taken before the
/// question.
#[test]
fn a_plugin_whose_files_changed_starts_no_server_before_or_after_its_question() {
    let root = root("plugin-server-rehash");
    let home = install(&root, "home", "demo", WEATHER);
    let mut yes = Asked::saying(Answer::Once);
    settled(&root, &home, "demo_weather", Asking::Person, &mut yes);
    edit_the_script(&home);

    let mut asker = Asked::saying(Answer::Once);
    let changed = settled(&root, &home, "demo_weather", Asking::Person, &mut asker);
    assert!(changed.plans.is_empty());
    assert!(asker.shown.is_empty(), "a changed tree is not asked about");
    assert!(
        changed.notes.contains(
            &t!(
                servers_plugin_changed,
                alias = "demo_weather",
                plugin = "demo"
            )
            .to_string()
        ),
        "{:?}",
        changed.notes
    );

    let second = install(&root, "second", "demo", WEATHER);
    let mut waiting = Asked::saying(Answer::Once);
    let held = second.clone();
    waiting.edit = Some(Box::new(move || edit_the_script(&held)));
    let reached = reach(
        &requested(&root, "demo_weather"),
        &root.join("project"),
        &Home {
            directory: Some(second.clone()),
            writable: true,
        },
        &Managed::default(),
        Asking::Person,
        &mut waiting,
        Stream::Null,
    );
    assert_eq!(waiting.shown.len(), 1, "the question was put");
    assert!(reached.session().is_none(), "{:?}", reached.notes);
    assert!(
        reached.notes.contains(
            &t!(
                servers_plugin_changed,
                alias = "demo_weather",
                plugin = "demo"
            )
            .to_string()
        ),
        "{:?}",
        reached.notes
    );
}

/// Answers the one question an update puts with a yes and no other.
struct UpdateYes;

impl crate::plugins::Asker for UpdateYes {
    fn anybody_there(&self) -> bool {
        true
    }

    fn confirm_fetch_host(&mut self, _: &crate::confirm::FetchRequest) -> bool {
        false
    }

    fn confirm_install(&mut self, _: &crate::plugins::Staged) -> bool {
        false
    }

    fn confirm_update(
        &mut self,
        _: &crate::plugins::Staged,
        _: &crate::plugins::Installed,
        _: &crate::plugins::Before,
    ) -> bool {
        true
    }
}

/// PLUGIN-7, PLUGIN-9: a server of a plugin that an accepted update replaced is put to the person
/// again, although its declaration is the same text. The approval is a digest that includes the
/// digest of the plugin's tree, so the line a yes wrote for the old tree answers nothing about the
/// new one. The old line is put back after the update, so what is shown is the digest alone and not
/// the withdrawal. Before the update the same line starts the server with no question (the control).
///
/// Rejects: an approval digest of a plugin's server that leaves the tree out, so a server keeps
/// running on a yes given for other files.
#[test]
fn a_server_of_an_updated_plugin_is_asked_about_again_though_its_declaration_is_the_same() {
    let root = root("plugin-server-update-asks-again");
    let home = install(&root, "home", "demo", WEATHER);
    let mut yes = Asked::saying(Answer::Once);
    settled(&root, &home, "demo_weather", Asking::Person, &mut yes);
    let old_answer = recorded(&home);
    assert!(old_answer.starts_with("plugin "), "control: {old_answer}");
    let mut nobody = Asked::saying(Answer::No);
    nobody.present = false;
    let control = settled(&root, &home, "demo_weather", Asking::OneShot, &mut nobody);
    assert_eq!(control.plans.len(), 1, "control: {:?}", control.notes);
    assert!(
        nobody.shown.is_empty(),
        "control: a recorded yes asks nothing"
    );

    std::fs::write(
        root.join("src-demo").join("server.sh"),
        "#!/bin/sh\necho two\n",
    )
    .expect("edit the source");
    assert_eq!(
        crate::plugins::update_typed("demo", &home, &mut UpdateYes),
        Ok(crate::plugins::Updated::Replaced)
    );
    assert_eq!(crate::plugins::tree_matches_record(&home, "demo"), Ok(()));
    std::fs::write(mcp::approvals_file(&home), &old_answer).expect("put the old line back");

    let mut asker = Asked::saying(Answer::No);
    let after = settled(&root, &home, "demo_weather", Asking::Person, &mut asker);
    assert!(after.plans.is_empty(), "{:?}", after.notes);
    assert_eq!(
        asker.shown.len(),
        1,
        "the server of an updated plugin is asked about again"
    );
}

/// S23 and S24. A plugin that is turned off starts no server and keeps the answers recorded for
/// it, and turning it on again asks nothing for an unchanged tree.
///
/// Rejects a settle that reads a disabled plugin's servers, one that drops its answers, and an
/// enable that asks again.
#[test]
fn a_disabled_plugin_starts_no_server_and_its_answers_stay() {
    let root = root("plugin-server-disabled");
    let home = install(&root, "home", "demo", WEATHER);
    let mut yes = Asked::saying(Answer::Once);
    settled(&root, &home, "demo_weather", Asking::Person, &mut yes);
    let answered = recorded(&home);
    crate::plugins::set_enabled(&home, "demo", false).expect("disable");

    let mut asker = Asked::saying(Answer::Once);
    let off = settled(&root, &home, "demo_weather", Asking::Person, &mut asker);
    assert!(off.plans.is_empty());
    assert!(asker.shown.is_empty());
    assert!(
        off.notes.contains(
            &t!(
                servers_plugin_disabled,
                alias = "demo_weather",
                plugin = "demo"
            )
            .to_string()
        ),
        "{:?}",
        off.notes
    );
    assert_eq!(recorded(&home), answered, "the answer stays");
    assert!(
        PluginServers::requests(&home, &[]).is_empty(),
        "a disabled plugin requests nothing"
    );

    crate::plugins::set_enabled(&home, "demo", true).expect("enable");
    let mut nobody = Asked::saying(Answer::No);
    nobody.present = false;
    let on = settled(&root, &home, "demo_weather", Asking::OneShot, &mut nobody);
    assert_eq!(on.plans.len(), 1, "{:?}", on.notes);
    assert!(
        nobody.shown.is_empty(),
        "enable asks nothing for an unchanged tree"
    );
}

/// S30. An alias the person's own `mcp.json` declares is that declaration, whatever a plugin
/// carries under the name, and the person is told the plugin's server is not used.
///
/// Rejects a plugin's declaration taking the place of the person's.
#[test]
fn a_declaration_in_mcp_json_wins_over_a_plugins_server_of_the_same_alias() {
    let root = root("plugin-server-shadowed");
    let home = install(&root, "home", "demo", WEATHER);
    let own = Declaration::http("http://127.0.0.1:2/own".into()).expect("declaration");
    let mut declarations = Declarations::default();
    declarations.insert("demo_weather", &own);
    std::fs::write(mcp::declarations_file(&home), declarations.to_text()).expect("mcp.json");
    let mut approvals = Approvals::default();
    approvals.approve("demo_weather", own.digest());
    std::fs::write(mcp::approvals_file(&home), approvals.to_text()).expect("approved");

    let mut asker = Asked::saying(Answer::No);
    let said = settled(&root, &home, "demo_weather", Asking::OneShot, &mut asker);

    let [(_, Plan::Http { url, .. })] = said.plans.as_slice() else {
        panic!("one remote plan: {:?}", said.notes);
    };
    assert_eq!(url, "http://127.0.0.1:2/own");
    assert!(said.taken.is_empty(), "no server was taken from the plugin");
    assert!(
        said.notes.contains(
            &t!(
                servers_plugin_shadowed,
                alias = "demo_weather",
                plugin = "demo"
            )
            .to_string()
        ),
        "{:?}",
        said.notes
    );
}

/// An installed, enabled plugin requests its servers, and a settings file's request of the same
/// alias is the one that stands.
#[test]
fn an_enabled_plugin_requests_each_server_it_carries_once() {
    let root = root("plugin-server-requests");
    let home = install(&root, "home", "demo", WEATHER);

    let asked = PluginServers::requests(&home, &[]);
    assert_eq!(
        asked
            .iter()
            .map(|(_, alias)| alias.as_str())
            .collect::<Vec<_>>(),
        ["demo_weather"]
    );
    let by_settings = requested(&root, "demo_weather");
    assert!(PluginServers::requests(&home, &by_settings).is_empty());
}

/// S32 server half. Removing the plugin withdraws the answers recorded for its servers and leaves
/// every other line, the person's own approvals included, and a reinstall of the same files is
/// asked about again. The withdrawal comes before anything is deleted, so one that cannot be done
/// leaves the plugin installed.
///
/// Rejects a remove that leaves the plugin's line, one that takes a home approval with it, and
/// one that withdraws after deleting.
#[test]
fn removing_a_plugin_withdraws_its_servers_answers_and_leaves_the_rest() {
    let root = root("plugin-server-remove");
    let home = install(&root, "home", "demo", WEATHER);
    let own = Declaration::http("http://127.0.0.1:2/own".into()).expect("declaration");
    let mut approvals = Approvals::default();
    approvals.approve("own", own.digest());
    std::fs::write(mcp::approvals_file(&home), approvals.to_text()).expect("a home approval");
    let mut yes = Asked::saying(Answer::Once);
    settled(&root, &home, "demo_weather", Asking::Person, &mut yes);
    assert!(recorded(&home).contains("plugin "), "control: answered");

    crate::plugins::remove(&home, "demo").expect("remove");

    let left = recorded(&home);
    assert!(!left.contains("plugin "), "{left}");
    assert!(left.contains(&own.digest().to_string()), "{left}");
    let again = install(&root, "home", "demo", WEATHER);
    assert_eq!(again, home);
    let mut asker = Asked::saying(Answer::No);
    settled(&root, &home, "demo_weather", Asking::Person, &mut asker);
    assert_eq!(asker.shown.len(), 1, "a reinstall is asked about again");

    // A file that cannot be rewritten stops the remove with the plugin still installed.
    let mut yes = Asked::saying(Answer::Once);
    settled(&root, &home, "demo_weather", Asking::Person, &mut yes);
    let file = mcp::approvals_file(&home);
    let text = std::fs::read_to_string(&file).expect("answers");
    std::fs::remove_file(&file).expect("replace the file");
    std::fs::create_dir(&file).expect("with a directory");
    assert!(crate::plugins::remove(&home, "demo").is_err());
    assert!(
        crate::plugins::installed_dir(&home, "demo").is_dir(),
        "the directory was deleted before the answers were withdrawn"
    );
    std::fs::remove_dir(&file).expect("a directory");
    std::fs::write(&file, text).expect("restore");
}

/// A server's alias is a key the plugin's own file chose. Every note settling makes about it draws
/// it as a name a checkout wrote is, with the escape and the line break written out, including a
/// note made before the joined alias has been checked (a changed tree) and one made after
/// (an entry that cannot be used).
///
/// Rejects: a note built from the raw alias, which carries the plugin's escape sequence and line
/// break to the person's terminal and transcript.
#[test]
fn a_plugin_authored_alias_with_a_control_character_and_a_newline_is_drawn_escaped_in_each_note() {
    let root = root("plugin-server-alias-drawn");
    let servers =
        r#"{"servers":{"x\u001b[2J\nsecond":{"transport":"http","url":"http://127.0.0.1:1/mcp"}}}"#;
    let home = install(&root, "home", "demo", servers);
    let alias = crate::plugin_servers::alias_of("demo", "x\u{1b}[2J\nsecond");
    assert!(alias.contains('\u{1b}') && alias.contains('\n'), "control");

    let mut asker = Asked::saying(Answer::Once);
    let unusable = settled(&root, &home, &alias, Asking::Person, &mut asker);
    edit_the_script(&home);
    let changed = settled(&root, &home, &alias, Asking::Person, &mut asker);

    for (which, said) in [("unusable", &unusable), ("changed", &changed)] {
        assert!(said.plans.is_empty(), "{which}: started");
        assert_eq!(said.notes.len(), 1, "{which}: {:?}", said.notes);
        let note = &said.notes[0];
        assert!(
            !note.chars().any(char::is_control),
            "{which}: a control character reached a note: {note:?}"
        );
        assert!(
            note.contains("demo_x\\u{1b}[2J\\nsecond"),
            "{which}: {note}"
        );
    }
    assert!(asker.shown.is_empty(), "no question was put");
}
