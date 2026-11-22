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
    settled_under(
        root,
        home,
        alias,
        (&Managed::default(), Prelude::current()),
        asking,
        asker,
    )
}

/// [`settled`] against the managed layer and the confinement given, not the ones this machine has.
fn settled_under(
    root: &Path,
    home: &Path,
    alias: &str,
    (managed, prelude): (&Managed, Option<Prelude>),
    asking: Asking,
    asker: &mut Asked,
) -> Settled {
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
        managed,
        asking,
        asker,
        &|_| None,
        prelude,
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
    edit_a_script(home, "demo");
}

/// Change one file in the installed directory of the plugin `plugin`, and in no other.
fn edit_a_script(home: &Path, plugin: &str) {
    std::fs::write(
        crate::plugins::installed_dir(home, plugin).join("server.sh"),
        "#!/bin/sh\nrm -rf /\n",
    )
    .expect("edit");
}

/// What a listing holds for the server `alias` among the plugins under `home`, read as a session
/// at a terminal (or not) reads them.
fn held(home: &Path, alias: &str, asking: Asking) -> PluginHeld {
    let carried = PluginServers::read(home);
    let mut rows = held_by_plugins(&carried, home, asking, &Managed::default());
    let at = rows
        .iter()
        .position(|(server, _)| server.alias == alias)
        .expect("the server is listed");
    rows.swap_remove(at).1
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

/// A listing calls a plugin's server approved only where the person said yes to that server, and
/// asked about where nothing has been said and a session at a terminal would ask. A home approval
/// of the same declaration and a recorded project do not approve it, as a session settles it
/// (SERVERS-27).
///
/// Rejects a listing that reuses the declared-server decision (`approves` on the declaration's
/// digest, or a recorded project), which would call a server approved that a session still asks
/// about, and one that never reads the person's yes, which would ask again about an answered one.
#[test]
fn a_listing_calls_a_plugins_server_approved_only_for_a_yes_to_that_server() {
    let root = root("plugin-held-approval");
    let home = install(&root, "home", "demo", WEATHER);

    let nobody_answered = held(&home, "demo_weather", Asking::Person);
    assert!(!nobody_answered.approved, "{nobody_answered:?}");
    assert!(nobody_answered.asks, "a session at a terminal asks");
    assert!(nobody_answered.digest.is_some());
    assert!(nobody_answered.stop.is_none() && nobody_answered.unusable.is_none());
    assert!(nobody_answered.unstarted.is_none());

    let declaration = Declaration::http("http://127.0.0.1:1/mcp".into()).expect("declaration");
    let mut approvals = Approvals::default();
    approvals.approve("demo_weather", declaration.digest());
    let mut projects = Projects::default();
    projects.add(&root.join("project"));
    std::fs::write(mcp::approvals_file(&home), approvals.to_text()).expect("a home approval");
    std::fs::write(mcp::projects_file(&home), projects.to_text()).expect("a project answer");
    for asking in [Asking::Person, Asking::Bypass, Asking::OneShot] {
        let said = held(&home, "demo_weather", asking);
        assert!(!said.approved, "{asking:?}: {said:?}");
        assert_eq!(said.asks, asking == Asking::Person, "{asking:?}");
    }

    let mut yes = Asked::saying(Answer::Once);
    let started = settled(&root, &home, "demo_weather", Asking::Person, &mut yes);
    assert_eq!(started.plans.len(), 1, "{:?}", started.notes);
    for asking in [Asking::Person, Asking::Bypass, Asking::OneShot] {
        let said = held(&home, "demo_weather", asking);
        assert!(said.approved && !said.asks, "{asking:?}: {said:?}");
        assert_eq!(said.digest, Some(yes.digests[0]), "{asking:?}");
    }
}

/// A plugin whose files changed holds a stop whatever was answered, and a plugin turned off is
/// named and holds a stop for a server read before it was turned off. Neither is asked about.
///
/// Rejects a listing that never hashes the plugin's directory again, which would call a changed
/// tree startable, and one that reads a turned-off plugin's servers file for display (PLUGIN-11).
#[test]
fn a_listing_holds_a_stop_for_a_changed_tree_and_names_a_plugin_turned_off() {
    let root = root("plugin-held-stop");
    let home = install(&root, "home", "demo", WEATHER);
    let mut yes = Asked::saying(Answer::Once);
    settled(&root, &home, "demo_weather", Asking::Person, &mut yes);
    let control = held(&home, "demo_weather", Asking::Person);
    assert!(control.stop.is_none() && control.approved, "{control:?}");

    edit_the_script(&home);
    let changed = held(&home, "demo_weather", Asking::Person);
    assert_eq!(changed.stop, Some(Stop::Changed), "{changed:?}");
    assert!(!changed.asks, "a changed tree is not asked about");

    let second = install(&root, "second", "demo", WEATHER);
    let carried = PluginServers::read(&second);
    crate::plugins::set_enabled(&second, "demo", false).expect("disable");
    let rows = held_by_plugins(&carried, &second, Asking::Person, &Managed::default());
    let [(_, off)] = rows.as_slice() else {
        panic!("one server");
    };
    assert_eq!(off.stop, Some(Stop::Disabled), "{off:?}");
    assert!(!off.asks);

    let again = PluginServers::read(&second);
    assert_eq!(again.disabled(), ["demo".to_string()]);
    assert!(again.servers().is_empty(), "nothing of it was read");
    assert!(again.requested(&[]).is_empty());
}

/// A joined alias that is too long is told apart from an alias the plugin's file wrote that is not
/// one and from a declaration that is wrong, so a front end can say which: [`plugin_problem`] gives
/// the length sentence for the first and the declaration's own problem for the other two.
///
/// Rejects a `plugin_problem` that answers every `Problem::Alias` with the length sentence, one that
/// never says it, and one that gives the three the same words.
#[test]
fn a_listing_tells_a_joined_alias_that_is_too_long_from_an_unusable_declaration() {
    let root = root("plugin-held-unusable");
    let long = "x".repeat(60);
    let servers = format!(
        r#"{{"servers":{{"{long}":{{"transport":"http","url":"http://127.0.0.1:1/mcp"}},"y\u001b[2J":{{"transport":"http","url":"http://127.0.0.1:1/mcp"}},"bad":{{"transport":"smtp"}}}}}}"#
    );
    let home = install(&root, "home", "demo", &servers);
    let carried = PluginServers::read(&home);
    let rows = held_by_plugins(&carried, &home, Asking::Person, &Managed::default());
    assert_eq!(rows.len(), 3);
    let row = |named: &str| {
        rows.iter()
            .find(|(server, _)| server.named == named)
            .expect("listed")
    };
    let sentence = |named: &str| {
        let (server, held) = row(named);
        plugin_problem(server, held.unusable.as_ref().expect("unusable"))
    };

    assert_eq!(row(&long).1.unusable, Some(Problem::Alias));
    assert!(!row(&long).1.asks);
    assert_eq!(sentence(&long), t!(mcp_plugin_too_long).to_string());
    assert_ne!(sentence(&long), problem(&Problem::Alias));
    assert_eq!(row("y\u{1b}[2J").1.unusable, Some(Problem::Alias));
    assert_eq!(sentence("y\u{1b}[2J"), problem(&Problem::Alias));
    assert_eq!(row("bad").1.unusable, Some(Problem::Transport));
    assert_eq!(sentence("bad"), problem(&Problem::Transport));
}

/// A session's note for an entry a plugin carries that cannot be used is worded as a listing words
/// it: a joined alias that is too long says the joined alias is too long, and an alias that is not
/// one and a declaration that is wrong say what `problem` says of them.
///
/// Rejects a `settle_plugin` that words every unusable entry with `problem(found)`, which tells a
/// person whose plugin's name makes the join too long that the alias is not one, and one that
/// words every `Problem::Alias` as too long.
#[test]
fn a_session_words_an_unusable_plugin_entry_as_a_listing_does() {
    let root = root("plugin-session-unusable-words");
    let long = "x".repeat(60);
    let servers = format!(
        r#"{{"servers":{{"{long}":{{"transport":"http","url":"http://127.0.0.1:1/mcp"}},"y\u001b[2J":{{"transport":"http","url":"http://127.0.0.1:1/mcp"}},"bad":{{"transport":"smtp"}}}}}}"#
    );
    let home = install(&root, "home", "demo", &servers);
    let carried = PluginServers::read(&home);
    let entries = [
        (
            long.as_str(),
            Problem::Alias,
            t!(mcp_plugin_too_long).to_string(),
        ),
        ("y\u{1b}[2J", Problem::Alias, problem(&Problem::Alias)),
        ("bad", Problem::Transport, problem(&Problem::Transport)),
    ];

    for (named, found, wording) in entries {
        let alias = crate::plugin_servers::alias_of("demo", named);
        let server = carried
            .servers()
            .iter()
            .find(|server| server.named == named)
            .expect("listed");
        assert_eq!(plugin_problem(server, &found), wording, "{named:?}");
        let mut asker = Asked::saying(Answer::Once);

        let said = settled(&root, &home, &alias, Asking::Person, &mut asker);

        assert!(said.plans.is_empty(), "{named:?}: started");
        assert!(asker.shown.is_empty(), "{named:?}: asked");
        assert_eq!(
            said.notes,
            [t!(mcp_unusable, alias = shown(&alias), problem = wording).to_string()],
            "{named:?}"
        );
    }
}

/// A session settling a plugin's server and a listing holding it agree on the same states: the
/// server starts exactly where the listing holds nothing back and either approves it or lets a
/// session ask, and a question is put exactly where the listing says `asks`. Where the entry cannot
/// be used because the joined alias is too long, the session's note is the sentence the listing
/// gives, not only a refusal.
///
/// Rejects a second copy of the decision in either, which drifts from the other as soon as one of
/// them changes, and a session that refuses a too-long joined alias in other words than a listing
/// uses.
#[test]
fn a_session_and_a_listing_agree_on_every_state_of_a_plugins_server() {
    let root = root("plugin-held-agree");
    let long = "x".repeat(60);
    let too_long = format!(
        r#"{{"servers":{{"{long}":{{"transport":"http","url":"http://127.0.0.1:1/mcp"}}}}}}"#
    );
    let mut checked = 0;
    for state in [
        "unanswered",
        "answered",
        "home approval and project",
        "changed",
        "too long",
    ] {
        for asking in [Asking::Person, Asking::OneShot, Asking::Bypass] {
            let name = format!("home-{checked}");
            checked += 1;
            let servers = if state == "too long" {
                &too_long
            } else {
                WEATHER
            };
            let alias = if state == "too long" {
                format!("demo_{long}")
            } else {
                "demo_weather".to_string()
            };
            let home = install(&root, &name, "demo", servers);
            match state {
                "answered" => {
                    let mut yes = Asked::saying(Answer::Once);
                    settled(&root, &home, &alias, Asking::Person, &mut yes);
                }
                "home approval and project" => {
                    let declaration =
                        Declaration::http("http://127.0.0.1:1/mcp".into()).expect("declaration");
                    let mut approvals = Approvals::default();
                    approvals.approve("demo_weather", declaration.digest());
                    let mut projects = Projects::default();
                    projects.add(&root.join("project"));
                    std::fs::write(mcp::approvals_file(&home), approvals.to_text()).expect("home");
                    std::fs::write(mcp::projects_file(&home), projects.to_text()).expect("project");
                }
                "changed" => edit_the_script(&home),
                _ => {}
            }

            let listing = held(&home, &alias, asking);
            let mut asker = Asked::saying(Answer::Once);
            let session = settled(&root, &home, &alias, asking, &mut asker);

            let held_back =
                listing.stop.is_some() || listing.unusable.is_some() || listing.unstarted.is_some();
            let starts = !held_back && (listing.approved || listing.asks);
            let context = format!("{state}, {asking:?}: {listing:?} {:?}", session.notes);
            assert_eq!(session.plans.len() == 1, starts, "{context}");
            assert_eq!(
                asker.shown.len() == 1,
                listing.asks && !held_back,
                "{context}"
            );
            if state == "too long" {
                let carried = PluginServers::read(&home);
                let server = carried
                    .servers()
                    .iter()
                    .find(|server| server.alias == alias)
                    .expect("listed");
                let sentence = plugin_problem(
                    server,
                    listing
                        .unusable
                        .as_ref()
                        .expect("the listing holds a problem"),
                );
                assert_eq!(sentence, t!(mcp_plugin_too_long).to_string(), "{context}");
                assert_eq!(
                    session.notes,
                    [t!(mcp_unusable, alias = shown(&alias), problem = sentence).to_string()],
                    "{context}"
                );
            }
        }
    }
    assert_eq!(checked, 15);
}

/// The decision for one server, made by the function a caller that answers for one alias calls, is
/// the decision the whole listing makes for that server, in each state a plugin's server is in:
/// unapproved, approved, a tree that changed, a plugin turned off, and an entry that cannot be used.
///
/// Rejects a single-server function that does not hash the plugin's directory again, which would
/// call a changed tree startable, one that reads no approvals, which would ask about an answered
/// server, and a second copy of the decision that drifts from the listing's.
#[test]
fn the_hold_of_one_server_is_the_hold_the_listing_gives_that_server() {
    let root = root("plugin-held-single");
    let wrong = r#"{"servers":{"bad":{"transport":"smtp"}}}"#;
    let fields = |held: &PluginHeld| {
        (
            held.stop,
            held.unusable.clone(),
            held.unstarted.clone(),
            held.digest,
            held.approved,
            held.asks,
            held.plan.is_some(),
        )
    };
    let mut checked = 0;
    for state in [
        "unapproved",
        "approved",
        "changed",
        "turned off",
        "unusable",
    ] {
        for asking in [Asking::Person, Asking::OneShot] {
            let name = format!("home-{checked}");
            checked += 1;
            let unusable = state == "unusable";
            let home = install(&root, &name, "demo", if unusable { wrong } else { WEATHER });
            let alias = if unusable { "demo_bad" } else { "demo_weather" };
            match state {
                "approved" => {
                    let mut yes = Asked::saying(Answer::Once);
                    settled(&root, &home, alias, Asking::Person, &mut yes);
                }
                "changed" => edit_the_script(&home),
                _ => {}
            }
            let carried = PluginServers::read(&home);
            if state == "turned off" {
                crate::plugins::set_enabled(&home, "demo", false).expect("disable");
            }

            let mut listing = held_by_plugins(&carried, &home, asking, &Managed::default());
            let (server, listed) = listing.pop().expect("one server");
            assert!(listing.is_empty(), "one server");
            let single = held_by_plugin_server(server, &home, asking, &Managed::default());

            let context = format!("{state}, {asking:?}: {single:?} against {listed:?}");
            assert_eq!(fields(&single), fields(&listed), "{context}");
            match state {
                "unapproved" => assert!(!single.approved && single.stop.is_none(), "{context}"),
                "approved" => assert!(single.approved && !single.asks, "{context}"),
                "changed" => assert_eq!(single.stop, Some(Stop::Changed), "{context}"),
                "turned off" => assert_eq!(single.stop, Some(Stop::Disabled), "{context}"),
                _ => assert!(single.unusable.is_some(), "{context}"),
            }
        }
    }
    assert_eq!(checked, 10);
}

/// A held server printed with `{:?}` names every field a caller reads and leaves out what its plan
/// holds: the plan of an HTTP server carries the bearer token read from the environment in its
/// headers, and a test or a log that prints a `PluginHeld` must not print it.
///
/// Rejects a `Debug` derived over the plan, which prints the header `Authorization: Bearer <token>`.
#[test]
fn a_held_server_printed_with_debug_leaves_out_the_bearer_token_its_plan_holds() {
    let root = root("plugin-held-debug");
    let secure = r#"{"servers":{"secure":{"transport":"http","url":"http://127.0.0.1:1/mcp","bearer_env":"DEMO_TOKEN"}}}"#;
    let home = install(&root, "home", "demo", secure);
    let carried = PluginServers::read(&home);
    let [server] = carried.servers() else {
        panic!("one server");
    };
    let token = "secret-token-value-4454";

    let held = hold_plugin_server(
        server,
        Ok(()),
        &Approvals::default(),
        &Managed::default(),
        &|name| (name == "DEMO_TOKEN").then(|| std::ffi::OsString::from(token)),
        Prelude::current(),
        &|| true,
    );

    let Some(Plan::Http { headers, .. }) = &held.plan else {
        panic!("an HTTP plan: {held:?}");
    };
    assert!(
        headers.contains(&("Authorization".to_string(), format!("Bearer {token}"))),
        "control: the plan holds the token"
    );
    for printed in [format!("{held:?}"), format!("{held:#?}")] {
        assert!(!printed.contains(token), "{printed}");
        assert!(printed.contains("plan: Some(..)"), "{printed}");
        assert!(
            printed.contains("asks: true") && printed.contains("approved: false"),
            "{printed}"
        );
    }
}

const TWO_SERVERS: &str = r#"{"servers":{"b":{"transport":"http","url":"http://127.0.0.1:1/mcp"},"c":{"transport":"http","url":"http://127.0.0.1:2/mcp"}}}"#;

/// The plugins `a` and `a-b`, each carrying the servers `b` and `c`, installed under one state
/// directory called `name`. The name of the first is the start of the name of the second, and the
/// servers have the same names, so only the whole plugin name tells their servers apart.
fn two_plugins(root: &Path, name: &str) -> PathBuf {
    let home = install(root, name, "a", TWO_SERVERS);
    assert_eq!(
        install(root, name, "a-b", TWO_SERVERS),
        home,
        "one state directory"
    );
    home
}

/// The stop each server of `carried` holds, by alias in sorted order.
fn stops(carried: &PluginServers, home: &Path, asking: Asking) -> Vec<(String, Option<Stop>)> {
    let mut stops: Vec<(String, Option<Stop>)> =
        held_by_plugins(carried, home, asking, &Managed::default())
            .into_iter()
            .map(|(server, held)| (server.alias.clone(), held.stop))
            .collect();
    stops.sort_by(|left, right| left.0.cmp(&right.0));
    stops
}

/// What [`stops`] gives for the four servers of [`two_plugins`] where the servers of `a` hold `a_stop`
/// and the servers of `a-b` hold `a_b_stop`.
fn expected_stops(a_stop: Option<Stop>, a_b_stop: Option<Stop>) -> Vec<(String, Option<Stop>)> {
    vec![
        ("a-b_b".to_string(), a_b_stop),
        ("a-b_c".to_string(), a_b_stop),
        ("a_b".to_string(), a_stop),
        ("a_c".to_string(), a_stop),
    ]
}

/// S6. A change to the files of one plugin stops the servers of that plugin, both of them, and the
/// servers of the other plugin installed in the same state directory are not stopped, whichever of
/// the two is changed. A listing and a session say the same of each.
///
/// Rejects a `held_by_plugins` that reuses the stop of the first plugin hashed for every server (the
/// changed `a-b` shows no stop, the changed `a` stops `a-b` too), one that keys the stops it has found
/// on a prefix of the plugin name, so `a-b` is given what `a` had, and one that hashes a directory
/// other than the plugin's own. A session that checked the wrong plugin for a server fails the same
/// rows.
#[test]
fn a_change_to_one_plugin_stops_the_servers_of_that_plugin_and_no_other() {
    let root = root("plugin-held-two-changed");
    for (changed, expected) in [
        ("a-b", expected_stops(None, Some(Stop::Changed))),
        ("a", expected_stops(Some(Stop::Changed), None)),
    ] {
        let home = two_plugins(&root, &format!("home-{changed}"));
        let carried = PluginServers::read(&home);
        assert_eq!(carried.servers().len(), 4, "four servers are read");
        assert_eq!(
            stops(&carried, &home, Asking::Person),
            expected_stops(None, None),
            "control, {changed}"
        );

        edit_a_script(&home, changed);

        for asking in [Asking::Person, Asking::OneShot] {
            assert_eq!(
                stops(&carried, &home, asking),
                expected,
                "{changed} changed, {asking:?}"
            );
        }
        for server in carried.servers() {
            let single = held_by_plugin_server(server, &home, Asking::Person, &Managed::default());
            let wanted = (server.plugin == changed).then_some(Stop::Changed);
            assert_eq!(single.stop, wanted, "{changed} changed, {}", server.alias);
        }
        for plugin in ["a", "a-b"] {
            let alias = format!("{plugin}_b");
            let mut asker = Asked::saying(Answer::No);
            let said = settled(&root, &home, &alias, Asking::Person, &mut asker);
            assert!(said.plans.is_empty(), "{changed} changed, {alias}");
            if plugin == changed {
                assert_eq!(
                    said.notes,
                    [t!(
                        servers_plugin_changed,
                        alias = alias.as_str(),
                        plugin = plugin
                    )
                    .to_string()],
                    "{changed} changed, {alias}"
                );
                assert!(asker.shown.is_empty(), "{changed} changed, {alias}: asked");
            } else {
                assert_eq!(
                    said.notes,
                    [t!(servers_declined, alias = alias.as_str()).to_string()],
                    "{changed} changed, {alias}"
                );
                assert_eq!(asker.shown.len(), 1, "{changed} changed, {alias}: asked");
            }
        }
    }
}

/// S6. A plugin turned off stops its own servers with `Stop::Disabled` and not those of a plugin whose
/// name starts with its name, and a request for a server the enabled plugin does not carry is not
/// claimed by the disabled one. Both are asked of a state directory holding `a` and `a-b`.
///
/// Rejects a stop looked up by a prefix of the plugin name (`a` taking `a-b` with it), a
/// `PluginServers::find` that treats an alias starting with a disabled plugin's name as that plugin's
/// (`a-b_zzz` answered as the disabled `a`), and a session that cannot tell `a_b` from `a-b_b`.
#[test]
fn a_plugin_turned_off_stops_its_own_servers_and_not_those_of_a_plugin_named_with_its_name_first() {
    let root = root("plugin-held-two-disabled");
    for (off, on, expected) in [
        ("a", "a-b", expected_stops(Some(Stop::Disabled), None)),
        ("a-b", "a", expected_stops(None, Some(Stop::Disabled))),
    ] {
        let home = two_plugins(&root, &format!("home-{off}"));
        let carried = PluginServers::read(&home);
        crate::plugins::set_enabled(&home, off, false).expect("disable");

        assert_eq!(stops(&carried, &home, Asking::Person), expected, "{off}");

        let again = PluginServers::read(&home);
        assert_eq!(again.disabled(), [off.to_string()], "{off}");
        assert_eq!(again.servers().len(), 2, "{off}: only {on}'s are read");
        assert!(
            matches!(again.find(&format!("{off}_b")), Some(Listed::Disabled(name)) if name == off),
            "{off}"
        );
        assert!(
            matches!(again.find(&format!("{on}_b")), Some(Listed::Server(server)) if server.plugin == on),
            "{on}"
        );
        assert!(
            again.find(&format!("{on}_zzz")).is_none(),
            "{off} is off and {on} does not carry zzz"
        );
        let alias = format!("{off}_b");
        let mut asker = Asked::saying(Answer::Once);
        let said = settled(&root, &home, &alias, Asking::Person, &mut asker);
        assert!(said.plans.is_empty(), "{alias}");
        assert_eq!(
            said.notes,
            [t!(
                servers_plugin_disabled,
                alias = alias.as_str(),
                plugin = off
            )
            .to_string()],
            "{alias}"
        );
        assert!(asker.shown.is_empty(), "{alias}: asked");
        let alias = format!("{on}_b");
        let mut asker = Asked::saying(Answer::Once);
        let said = settled(&root, &home, &alias, Asking::Person, &mut asker);
        assert_eq!(said.plans.len(), 1, "{alias}: {:?}", said.notes);
        assert_eq!(
            said.taken,
            [(alias.clone(), on.to_string())],
            "{alias} is taken from {on}"
        );
        assert_eq!(asker.shown.len(), 1, "{alias}");
    }
}

const NOT_FOUND: &str = r#"{"servers":{"tool":{"transport":"stdio","argv":["weather-mcp"]}}}"#;
const LOCAL: &str =
    r#"{"servers":{"tool":{"transport":"stdio","argv":["${PLUGIN_ROOT}/server.sh"]}}}"#;
const ELSEWHERE: &str =
    r#"{"servers":{"tool":{"transport":"http","url":"https://mcp.elsewhere.example/mcp"}}}"#;

/// [`hold_plugin_server`] for the server `alias`, as a listing holds it, against the managed layer and
/// the confinement given instead of the ones this machine has.
fn held_with(
    home: &Path,
    alias: &str,
    asking: Asking,
    managed: &Managed,
    prelude: Option<Prelude>,
) -> PluginHeld {
    let carried = PluginServers::read(home);
    let Some(Listed::Server(server)) = carried.find(alias) else {
        panic!("{alias} is a server of a plugin");
    };
    hold_plugin_server(
        server,
        crate::plugin_servers::may_start(home, &server.plugin),
        &Approvals::read(home),
        managed,
        &|name| std::env::var_os(name),
        prelude,
        &|| asking == Asking::Person,
    )
}

/// S7. A plugin's server that cannot be started here is held back for the reason a session gives and
/// a listing holds, in each of the three ways: its program is a bare name and nothing says where to
/// look for it (`Unplanned`), the machine's managed layer refuses it (`Refused`), and it is a local
/// server on a host with no confinement (`Unconfined`, given as no confinement). In none a question is
/// put, an answer is recorded or a server starts, in any mode, though a person is there and would say
/// yes. The session's note is the text of the state and the listing holds the state.
///
/// Rejects a session or a listing that asks about the server before it checks where it would start,
/// one that words a state as another (`Unplanned` said as a refusal by the managed layer, or the
/// reverse), one that drops the note for a state so the server is silently absent, and a listing
/// that holds nothing for a state the session refuses.
#[test]
fn a_session_and_a_listing_agree_on_the_states_that_keep_a_plugins_server_from_starting_here() {
    let root = root("plugin-held-unstarted");
    let managed_file = root.join("managed.json");
    std::fs::write(
        &managed_file,
        r#"{"mcp": {"allow": [{"host": "*.corp.example"}]}}"#,
    )
    .expect("managed.json");
    let refusing = Managed::at(&managed_file);
    let open = Managed::default();
    let path = managed_file.display().to_string();
    let alias = "demo_tool";
    let mut checked = 0;
    for state in ["not found", "refused", "unconfined"] {
        for asking in [Asking::Person, Asking::OneShot, Asking::Bypass] {
            let reason = match state {
                "not found" => {
                    t!(servers_program_without_path, program = "weather-mcp").to_string()
                }
                _ => t!(managed_not_allowed, path = path.clone()).to_string(),
            };
            let (servers, managed, prelude, unstarted, note) = match state {
                "not found" => (
                    NOT_FOUND,
                    &open,
                    Prelude::current(),
                    Unstarted::Unplanned(reason.clone()),
                    t!(servers_not_reached, alias = alias, reason = reason).to_string(),
                ),
                "refused" => (
                    ELSEWHERE,
                    &refusing,
                    Prelude::current(),
                    Unstarted::Refused(reason.clone()),
                    t!(servers_refused_by_managed, alias = alias, reason = reason).to_string(),
                ),
                _ => (
                    LOCAL,
                    &open,
                    None,
                    Unstarted::Unconfined,
                    t!(servers_no_confinement_here, alias = alias).to_string(),
                ),
            };
            let name = format!("home-{checked}");
            checked += 1;
            let home = install(&root, &name, "demo", servers);

            let listing = held_with(&home, alias, asking, managed, prelude);
            let mut asker = Asked::saying(Answer::Once);
            let session =
                settled_under(&root, &home, alias, (managed, prelude), asking, &mut asker);

            let context = format!("{state}, {asking:?}: {listing:?} {:?}", session.notes);
            assert_eq!(listing.unstarted, Some(unstarted), "{context}");
            assert!(
                listing.stop.is_none() && listing.unusable.is_none() && listing.plan.is_none(),
                "{context}"
            );
            assert!(!listing.asks && !listing.approved, "{context}");
            assert_eq!(session.notes, [note], "{context}");
            assert!(session.plans.is_empty(), "{context}: started");
            assert!(session.taken.is_empty(), "{context}: taken");
            assert!(asker.shown.is_empty(), "{context}: asked");
            assert_eq!(recorded(&home), "", "{context}: recorded");
            if state != "unconfined" {
                let carried = PluginServers::read(&home);
                let rows = held_by_plugins(&carried, &home, asking, managed);
                let [(_, host)] = rows.as_slice() else {
                    panic!("one server");
                };
                assert_eq!(host.unstarted, listing.unstarted, "{context}");
            }
        }
    }
    assert_eq!(checked, 9);
}

/// S7, the unconfined state as this host gives it. A plugin's local server is `Unconfined` in the
/// listing and in a session where `Prelude::current()` is `None` (a host with no confinement for a
/// local server), and where it is `Some` (macOS and Linux) the server has a plan and a session at a
/// terminal asks the one question. This asserts the state the host has and says which it was in the
/// failure message, so it holds on either kind of host.
///
/// Rejects a `held_by_plugins` or a session that does not read the host's confinement for a plugin's
/// server (one that always starts it, or one that always holds it back).
#[test]
fn a_plugins_local_server_is_unconfined_where_this_host_has_no_confinement_and_asked_about_where_it_has()
 {
    let root = root("plugin-held-host-confinement");
    let home = install(&root, "home", "demo", LOCAL);
    let carried = PluginServers::read(&home);
    let rows = held_by_plugins(&carried, &home, Asking::Person, &Managed::default());
    let [(_, listing)] = rows.as_slice() else {
        panic!("one server");
    };
    let mut asker = Asked::saying(Answer::No);

    let session = settled(&root, &home, "demo_tool", Asking::Person, &mut asker);

    let host = format!("Prelude::current() is {:?}", Prelude::current().is_some());
    assert!(session.plans.is_empty(), "{host}");
    match Prelude::current() {
        None => {
            assert_eq!(listing.unstarted, Some(Unstarted::Unconfined), "{host}");
            assert_eq!(
                session.notes,
                [t!(servers_no_confinement_here, alias = "demo_tool").to_string()],
                "{host}"
            );
            assert!(asker.shown.is_empty(), "{host}: asked");
        }
        Some(_) => {
            assert!(
                listing.unstarted.is_none() && listing.plan.is_some() && listing.asks,
                "{host}: {listing:?}"
            );
            assert_eq!(
                session.notes,
                [t!(servers_declined, alias = "demo_tool").to_string()],
                "{host}"
            );
            assert_eq!(asker.shown.len(), 1, "{host}: the one question");
        }
    }
}
