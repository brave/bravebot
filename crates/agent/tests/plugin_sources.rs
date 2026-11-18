//! Skills and definitions that come from an installed plugin.
//!
//! The state directory is an argument, as in the other discovery tests, so none of this touches
//! `HOME`.

use bravebot_agent::workspace::Workspace;
use bravebot_agent::{agents, plugins, skills};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::trust::TrustStore;
use std::path::{Path, PathBuf};

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "bravebot-plugin-sources-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self { path }
    }

    fn workspace(&self) -> PathBuf {
        let dir = self.path.join("project");
        std::fs::create_dir_all(&dir).expect("create workspace");
        dir
    }

    fn state(&self) -> PathBuf {
        let dir = self.path.join("state");
        std::fs::create_dir_all(&dir).expect("create state directory");
        dir
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn policy(sink: &mut RecordingSink) -> Policy<'_, RecordingSink> {
    let mut routing = Routing::new();
    routing.insert_trusted("task", "do the work");
    let mut store = TrustStore::new("/work");
    store.trust(".");
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead, Capability::FileWrite]),
        sink,
    )
    .expect("policy")
    .with_trust(store)
}

fn write_skill(root: &Path, name: &str, description: &str, body: &str) {
    let at = root.join("skills").join(name);
    std::fs::create_dir_all(&at).expect("create skill directory");
    std::fs::write(
        at.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}"),
    )
    .expect("write skill");
}

fn write_definition(root: &Path, name: &str, kind: &str, body: &str) {
    let at = root.join("agents");
    std::fs::create_dir_all(&at).expect("create agents directory");
    std::fs::write(
        at.join(format!("{name}.md")),
        format!("---\nname: {name}\ndescription: d\nkind: {kind}\n---\n\n{body}"),
    )
    .expect("write definition");
}

/// A plugin directory offering one skill and one definition, each named `shared`, and installs it.
fn install_plugin(scratch: &Scratch, state: &Path, readme: bool) {
    let source = scratch.path.join("source").join("demo");
    std::fs::create_dir_all(&source).expect("create plugin");
    std::fs::write(
        source.join("plugin.toml"),
        "name = \"demo\"\nversion = \"1.0.0\"\ndescription = \"PLUGIN-DESCRIPTION-MARKER\"\n\
         skills = \"skills\"\nagents = \"agents\"\n",
    )
    .expect("write manifest");
    write_skill(&source, "shared", "from the plugin", "plugin body");
    write_definition(&source, "shared", "worker", "plugin prompt");
    if readme {
        std::fs::write(source.join("README.md"), "PLUGIN-README-MARKER\n").expect("write readme");
    }
    plugins::install(&source, state).expect("install");
}

fn discover_skills(workspace: &Workspace, state: &Path) -> skills::Catalogue {
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    skills::discover(&mut policy, workspace, Some(state)).0
}

fn discover_definitions(
    workspace: &Workspace,
    state: &Path,
) -> bravebot_core::delegate::Definitions {
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    agents::discover(&mut policy, workspace, Some(state)).0
}

#[test]
fn an_enabled_plugin_offers_its_skill_and_definition_and_a_disabled_one_offers_nothing() {
    let scratch = Scratch::new("offers");
    let state = scratch.state();
    let workspace = Workspace::new(scratch.workspace()).unwrap();
    install_plugin(&scratch, &state, false);

    let catalogue = discover_skills(&workspace, &state);
    let skill = catalogue
        .get("shared")
        .expect("the plugin skill is offered");
    assert_eq!(skill.source, skills::Source::Plugin);
    assert!(skill.origin.contains("plugins/demo"), "{}", skill.origin);
    let definitions = discover_definitions(&workspace, &state);
    assert_eq!(definitions.get("shared").unwrap().prompt(), "plugin prompt");

    plugins::set_enabled(&state, "demo", false).unwrap();
    assert!(discover_skills(&workspace, &state).get("shared").is_none());
    assert!(
        discover_definitions(&workspace, &state)
            .get("shared")
            .is_none()
    );
}

/// PLUGIN-12: a plugin skill loses to a built-in, to the user's own, and to the project's.
#[test]
fn a_plugin_skill_is_shadowed_by_every_other_source_of_its_name() {
    let scratch = Scratch::new("skill-shadow");
    let state = scratch.state();
    let project = scratch.workspace();
    let workspace = Workspace::new(&project).unwrap();
    install_plugin(&scratch, &state, false);

    // Alone, the plugin's skill is the one offered.
    assert_eq!(
        discover_skills(&workspace, &state)
            .get("shared")
            .unwrap()
            .source,
        skills::Source::Plugin
    );

    // The user's own skill of that name wins, and the plugin's is not offered beside it.
    write_skill(&state, "shared", "home", "home body");
    let catalogue = discover_skills(&workspace, &state);
    assert_eq!(
        catalogue.get("shared").unwrap().source,
        skills::Source::Home
    );
    assert_eq!(catalogue.iter().filter(|s| s.name == "shared").count(), 1);

    // The project's wins over both.
    write_skill(
        &project.join(".bravebot"),
        "shared",
        "project",
        "project body",
    );
    let catalogue = discover_skills(&workspace, &state);
    assert_eq!(
        catalogue.get("shared").unwrap().source,
        skills::Source::Workspace
    );

    // A plugin skill named like a built-in does not replace it.
    let loop_plugin = scratch.path.join("source").join("loopy");
    std::fs::create_dir_all(&loop_plugin).unwrap();
    std::fs::write(
        loop_plugin.join("plugin.toml"),
        "name = \"loopy\"\nversion = \"1.0.0\"\ndescription = \"d\"\nskills = \"skills\"\n",
    )
    .unwrap();
    write_skill(&loop_plugin, "loop", "from the plugin", "plugin loop");
    plugins::install(&loop_plugin, &state).unwrap();
    assert_eq!(
        discover_skills(&workspace, &state)
            .get("loop")
            .unwrap()
            .source,
        skills::Source::BuiltIn
    );
}

/// PLUGIN-12: a plugin definition loses whole to any other source, with nothing merged from it.
#[test]
fn a_plugin_definition_is_shadowed_by_every_other_source_of_its_name() {
    let scratch = Scratch::new("definition-shadow");
    let state = scratch.state();
    let project = scratch.workspace();
    let workspace = Workspace::new(&project).unwrap();
    install_plugin(&scratch, &state, false);

    write_definition(&state, "shared", "reader", "home prompt");
    let definitions = discover_definitions(&workspace, &state);
    let found = definitions.get("shared").unwrap();
    assert_eq!(found.prompt(), "home prompt");
    assert_eq!(found.kind(), bravebot_core::delegate::Kind::Reader);
    assert_eq!(
        definitions.iter().filter(|d| d.name() == "shared").count(),
        1
    );

    write_definition(
        &project.join(".bravebot"),
        "shared",
        "reader",
        "project prompt",
    );
    let definitions = discover_definitions(&workspace, &state);
    assert_eq!(
        definitions.get("shared").unwrap().prompt(),
        "project prompt"
    );
}

/// PLUGIN-12, SKILL-3: a plugin skill is held and listed as one of the person's own is. Its body is
/// trusted, its origin names the plugin, it is listed to the planner, and a file beside it is read
/// through the home arm, so the plugin is trusted no more and no less than `~/.bravebot/skills`.
/// Rejects a `Source::Plugin` that falls out of the home arm of `read_beside`, and a plugin skill
/// read as untrusted content.
#[test]
fn a_plugin_skill_is_held_and_listed_as_a_home_skill_is() {
    let scratch = Scratch::new("skill-held-as-home");
    let state = scratch.state();
    let workspace = Workspace::new(scratch.workspace()).unwrap();
    install_plugin(&scratch, &state, false);
    let beside = plugins::installed_dir(&state, "demo")
        .join("skills")
        .join("shared");
    std::fs::write(beside.join("notes.md"), "beside the skill\n").unwrap();
    write_skill(&state, "mine", "home", "home body");
    std::fs::write(
        state.join("skills").join("mine").join("notes.md"),
        "beside the skill\n",
    )
    .unwrap();

    let catalogue = discover_skills(&workspace, &state);
    let plugin = catalogue.get("shared").expect("the plugin skill");
    let home = catalogue.get("mine").expect("the home skill");
    assert_eq!(plugin.source, skills::Source::Plugin);
    assert_eq!(home.source, skills::Source::Home);
    assert!(
        plugin
            .origin
            .starts_with("~/.bravebot/plugins/demo/skills/shared/"),
        "{}",
        plugin.origin
    );
    assert_eq!(
        plugin.body().label().is_trusted(),
        home.body().label().is_trusted()
    );
    assert!(plugin.body().label().is_trusted());
    // Found by search as a home skill is, and not listed in the prompt by default as neither is.
    assert!(
        catalogue
            .search("from the plugin")
            .0
            .iter()
            .any(|s| s.name == "shared")
    );
    assert!(catalogue.search("home").0.iter().any(|s| s.name == "mine"));
    assert!(!catalogue.describe_for_prompt().contains("shared"));
    assert!(!catalogue.describe_for_prompt().contains("mine"));
    assert_eq!(plugin.files, home.files);
    for skill in [plugin, home] {
        let read = skills::read_beside(
            skill,
            "notes.md",
            &workspace,
            TrustStore::new("/work"),
            Default::default(),
            &mut RecordingSink::new(),
        );
        assert!(read.is_some(), "{} was not read beside", skill.name);
    }
}

/// PLUGIN-12: a plugin definition is read as one of the person's own is: its origin names the
/// plugin, and what the file says is taken as written, the kind and the prompt.
#[test]
fn a_plugin_definition_is_listed_as_a_home_definition_is() {
    let scratch = Scratch::new("definition-held-as-home");
    let state = scratch.state();
    let workspace = Workspace::new(scratch.workspace()).unwrap();
    install_plugin(&scratch, &state, false);
    write_definition(&state, "mine", "worker", "home prompt");

    let definitions = discover_definitions(&workspace, &state);
    let plugin = definitions.get("shared").expect("the plugin definition");
    let home = definitions.get("mine").expect("the home definition");
    assert!(
        plugin
            .origin()
            .starts_with("~/.bravebot/plugins/demo/agents/"),
        "{}",
        plugin.origin()
    );
    assert!(
        home.origin().starts_with("~/.bravebot/agents/"),
        "{}",
        home.origin()
    );
    assert_eq!(plugin.kind(), home.kind());
    assert_eq!(plugin.prompt(), "plugin prompt");
    assert!(definitions.iter().any(|d| d.name() == "shared"));
}

/// PLUGIN-12: only the parts reach a turn. The manifest description and a README are never read
/// into a skill list or anything else the planner is given.
#[test]
fn nothing_but_the_parts_reaches_a_turn() {
    let scratch = Scratch::new("parts-only");
    let state = scratch.state();
    let workspace = Workspace::new(scratch.workspace()).unwrap();
    install_plugin(&scratch, &state, true);

    let catalogue = discover_skills(&workspace, &state);
    assert!(catalogue.get("shared").is_some());
    let mut seen = catalogue.describe_for_prompt();
    seen.push_str(&catalogue.other_skill_names().join("\n"));
    for skill in catalogue.iter() {
        seen.push_str(&format!(
            "{}\n{}\n{}\n",
            skill.name, skill.description, skill.origin
        ));
    }
    let definitions = discover_definitions(&workspace, &state);
    for definition in definitions.iter() {
        seen.push_str(&format!(
            "{}\n{}\n{}\n",
            definition.name(),
            definition.description(),
            definition.origin()
        ));
    }
    assert!(!seen.contains("PLUGIN-DESCRIPTION-MARKER"), "{seen}");
    assert!(!seen.contains("PLUGIN-README-MARKER"), "{seen}");
    // The copy kept the README on disk: it is there to be left unread.
    assert!(
        plugins::installed_dir(&state, "demo")
            .join("README.md")
            .exists()
    );
}

/// PLUGIN-12 in the install prompt: a staged plugin's names are marked with the source that
/// already offers them, and unmarked where nothing above a plugin does.
#[test]
fn a_staged_plugin_is_marked_with_the_sources_that_shadow_its_names() {
    let scratch = Scratch::new("marks");
    let state = scratch.state();
    let project = scratch.workspace();
    let workspace = Workspace::new(&project).unwrap();
    let source = scratch.path.join("source").join("demo");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("plugin.toml"),
        "name = \"demo\"\nversion = \"1.0.0\"\ndescription = \"d\"\nskills = \"skills\"\nagents = \"agents\"\n",
    )
    .unwrap();
    write_skill(&source, "shared", "d", "b");
    write_skill(&source, "loop", "d", "b");
    write_skill(&source, "free", "d", "b");
    write_definition(&source, "shared", "worker", "p");
    let staged = plugins::stage(&source, &state).unwrap();
    let offers = plugins::staged_offers(&staged);

    write_skill(&state, "shared", "home", "home body");
    let marks = |state: &Path| {
        let catalogue = discover_skills(&workspace, state);
        let definitions = discover_definitions(&workspace, state);
        plugins::shadow_marks(&offers, &catalogue, &definitions)
    };
    let by = |marks: &[plugins::Shadowed], name: &str, skill: bool| {
        marks
            .iter()
            .find(|mark| {
                mark.offer.name == name
                    && matches!(mark.offer.what, plugins::Offered::Skill { .. }) == skill
            })
            .expect("the offer is marked")
            .by
            .clone()
    };
    let marked = marks(&state);
    assert!(by(&marked, "shared", true).unwrap().contains("skills"));
    assert_eq!(by(&marked, "loop", true).as_deref(), Some("built-in"));
    assert_eq!(by(&marked, "free", true), None);
    // A definition of the same name is not shadowed by a skill.
    assert_eq!(by(&marked, "shared", false), None);

    // Once the plugin is installed, its own entries are not marked as shadowing themselves.
    plugins::commit_install(staged, &state).unwrap();
    let marked = marks(&state);
    assert_eq!(by(&marked, "free", true), None);
    assert_eq!(by(&marked, "shared", false), None);
}

/// PLUGIN-5: a plugin directory in a workspace offers nothing, whether it is the installed layout
/// or a bare plugin, and whether or not the workspace is vouched for.
#[test]
fn a_plugin_directory_in_a_workspace_offers_nothing() {
    let scratch = Scratch::new("workspace-offers-nothing");
    let state = scratch.state();
    let project = scratch.workspace();
    let source = scratch.path.join("source").join("demo");
    std::fs::create_dir_all(&source).unwrap();
    install_plugin(&scratch, &scratch.path.join("elsewhere"), false);
    for place in [
        project.join(".bravebot").join("plugins"),
        project.join("plugins"),
    ] {
        let copy = place.join("demo");
        std::fs::create_dir_all(&copy).unwrap();
        let from = plugins::installed_dir(&scratch.path.join("elsewhere"), "demo");
        std::fs::write(
            copy.join("plugin.toml"),
            std::fs::read(from.join("plugin.toml")).unwrap(),
        )
        .unwrap();
        write_skill(
            &copy,
            "shared",
            "from the workspace plugin",
            "workspace plugin",
        );
        write_definition(&copy, "shared", "worker", "workspace plugin");
        // The record an install writes beside a plugin.
        std::fs::write(
            place.join("demo.record"),
            "source = /s\nenabled = true\ndigest = d\n",
        )
        .unwrap();
    }
    let workspace = Workspace::new(&project).unwrap();

    let catalogue = discover_skills(&workspace, &state);
    assert!(catalogue.get("shared").is_none());
    assert!(
        discover_definitions(&workspace, &state)
            .get("shared")
            .is_none()
    );
    assert!(plugins::enabled_roots(&state).is_empty());
}
