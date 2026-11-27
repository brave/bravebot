use super::*;

use super::arguments::{Scratch as ArgumentsScratch, told, with_tools};

use bravebot_core::capability::{Capability, CapabilitySet};

use bravebot_core::event::RecordingSink;

use bravebot_core::policy::{ReleasePlan, Routing};

use bravebot_core::trust::TrustStore;

const SKILL_FILE_MARKER: &str = "REFERENCE-BESIDE-THE-SKILL";

/// A home holding the skill `notes` with files beside it, a file beside the home skill
/// `other` that nothing loads, and a secret outside every skill directory; and a project
/// beside it. `~` in a path names this home.
struct SkillFixture {
    /// Held for its removal of the directory when the test ends.
    _scratch: ArgumentsScratch,
    home: std::path::PathBuf,
    project: std::path::PathBuf,
}

impl SkillFixture {
    fn new(name: &str) -> Self {
        let scratch = ArgumentsScratch::new(name);
        let home = scratch.path.join("h");
        let project = scratch.path.join("project");
        for (skill, files) in [
            ("notes", &["reference.md", "scripts/helper.sh"][..]),
            ("other", &["unloaded.md"][..]),
        ] {
            let at = home.join(".bravebot/skills").join(skill);
            std::fs::create_dir_all(at.join("scripts")).unwrap();
            std::fs::write(
                at.join("SKILL.md"),
                format!("---\nname: {skill}\ndescription: d\n---\nbody of {skill}\n"),
            )
            .unwrap();
            for file in files {
                std::fs::write(at.join(file), format!("{SKILL_FILE_MARKER} {file}\n")).unwrap();
            }
        }
        std::fs::write(home.join("secret.md"), "OUTSIDE-EVERY-SKILL\n").unwrap();
        std::os::unix::fs::symlink(
            home.join("secret.md"),
            home.join(".bravebot/skills/notes/escape.md"),
        )
        .unwrap();
        std::fs::create_dir_all(&project).unwrap();
        Self {
            _scratch: scratch,
            home,
            project,
        }
    }

    fn workspace(&self) -> Workspace {
        Workspace::new(&self.project)
            .expect("workspace")
            .with_home(Some(self.home.clone()))
    }

    fn catalogue(&self, workspace: &Workspace) -> crate::skills::Catalogue {
        crate::skills::resolved(
            workspace,
            Some(&self.home.join(".bravebot")),
            TrustStore::new("/work"),
            bravebot_core::permissions::Permissions::default(),
            &mut RecordingSink::new(),
        )
    }
}

fn skill_policy(sink: &mut RecordingSink) -> Policy<'_, RecordingSink> {
    let mut routing = Routing::new();
    routing.insert_trusted("task", "read a skill's files");
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead, Capability::FileWrite]),
        sink,
    )
    .expect("policy")
    .with_trust(TrustStore::new("/work"))
}

/// `read_file` or `write_file` through dispatch, the door a planner uses.
fn skill_call(
    policy: &mut Policy<'_, RecordingSink>,
    workspace: &Workspace,
    tool: &str,
    arguments: Value,
) -> (bool, bool, String) {
    let call: ToolCall = serde_json::from_value(json!({
        "id": "a",
        "function": {"name": tool, "arguments": arguments.to_string()}
    }))
    .expect("a call");
    let produced = with_tools(workspace, |tools| {
        dispatch(
            policy,
            tools,
            &mut crate::confirm::ApproveWrites,
            &mut crate::report::IgnoreReports,
            &call,
        )
    });
    let trusted = produced.deferred.is_none() && produced.text.label().is_trusted();
    let said = told(policy, &produced.text);
    let failed = said.starts_with("error:") || said.starts_with("refused:");
    (failed, trusted, said)
}

fn load_named(
    policy: &mut Policy<'_, RecordingSink>,
    catalogue: &crate::skills::Catalogue,
    name: &str,
) -> String {
    let produced = load_skill(policy, catalogue, Beside::every(), &json!({ "name": name }));
    assert!(!produced.failed, "the skill did not load");
    told(policy, &produced.text)
}

/// Exercise the planner's dispatch door, including the catalogue guard and labelled result.
fn dispatched_help(
    policy: &mut Policy<'_, RecordingSink>,
    workspace: &Workspace,
    catalogue: &crate::skills::Catalogue,
    name: &str,
) -> Output {
    let call: ToolCall = serde_json::from_value(json!({
        "id": "help",
        "function": {"name": "load_skill", "arguments": json!({"name": name}).to_string()}
    }))
    .unwrap();
    arguments::with_tools_and_skills(workspace, &[], catalogue, |tools| {
        dispatch(
            policy,
            tools,
            &mut crate::confirm::ApproveWrites,
            &mut crate::report::IgnoreReports,
            &call,
        )
    })
}

/// Runtime checkout docs must not replace the installed version or grant access to any file.
#[test]
fn compiled_help_dispatch_is_trusted_and_opens_no_files_or_directories() {
    let fixture = SkillFixture::new("compiled-help-dispatch");
    let docs = fixture.project.join("docs/website/docs/using");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("sessions.md"), "CHECKOUT-DOC-DECOY").unwrap();
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    let key = workspace.trust_key("docs/website/docs/using/sessions.md");
    assert!(!policy.trusts_path(&key));
    let outside = || json!({"path": "~/.bravebot/skills/notes/reference.md"});
    assert!(skill_call(&mut policy, &workspace, "read_file", outside()).0);

    let mut names = vec![
        "bravebot-help".to_string(),
        "bravebot-help/using/sessions".to_string(),
        "bravebot-help/using/sessions::headings".to_string(),
        "bravebot-help/specs".to_string(),
        "bravebot-help/specs/commands".to_string(),
        // A long page's bare name is its outline, and `#<section>` is one of its sections.
        "bravebot-help/reference/commands".to_string(),
        "bravebot-help/reference/commands#request".to_string(),
    ];
    let list = dispatched_help(&mut policy, &workspace, &catalogue, &names[2]);
    let headings = told(&mut policy, &list.text);
    names.push(
        headings
            .lines()
            .filter_map(|line| line.strip_prefix("- "))
            .find(|name| name.ends_with(":The files the session wrote"))
            .unwrap()
            .to_string(),
    );
    for name in names {
        let output = dispatched_help(&mut policy, &workspace, &catalogue, &name);
        assert!(
            !told(&mut policy, &output.text).starts_with("error:"),
            "{name}"
        );
        assert!(output.text.label().is_trusted(), "{name}");
        assert_eq!(
            output.text.label().confidentiality,
            bravebot_core::Confidentiality::Public
        );
        assert_eq!(output.origin, "built-in", "{name}");
        assert!(
            output.deferred.is_none() && output.loaded.is_none(),
            "{name}"
        );
        let text = told(&mut policy, &output.text);
        assert!(!text.contains("CHECKOUT-DOC-DECOY"), "{name}");
        assert!(policy.loaded_skill_directories().is_empty(), "{name}");
        assert!(!policy.trusts_path(&key), "{name}");
        assert!(!policy.trusts_path("."), "{name}");
    }
    assert!(skill_call(&mut policy, &workspace, "read_file", outside()).0);
    let (failed, trusted, text) = skill_call(
        &mut policy,
        &workspace,
        "read_file",
        json!({"path": "docs/website/docs/using/sessions.md"}),
    );
    assert!(
        !failed && !trusted && !text.contains("CHECKOUT-DOC-DECOY"),
        "{text}"
    );
}

/// An advertised name must load through dispatch; normalising a traversal or a near miss must
/// not reach either compiled text or an on-disk file with that name.
#[test]
fn compiled_help_dispatch_loads_every_advertised_page_and_rejects_near_misses() {
    let fixture = SkillFixture::new("help-exact-dispatch");
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    let output = dispatched_help(&mut policy, &workspace, &catalogue, "bravebot-help");
    let index = told(&mut policy, &output.text);
    assert!(!index.contains("CMD-24:") && !index.contains("::section:14:"));
    let names: Vec<_> = index
        .lines()
        .filter_map(|line| line.strip_prefix("- bravebot-help/"))
        .map(|line| format!("bravebot-help/{}", line.split(": ").next().unwrap()))
        .collect();
    assert!(names.len() > 60);
    for name in names {
        let output = dispatched_help(&mut policy, &workspace, &catalogue, &name);
        assert!(
            !told(&mut policy, &output.text).starts_with("error:"),
            "{name}"
        );
        let page = told(&mut policy, &output.text);
        assert_ne!(page, index, "{name}");
        // Every section name an outline gives loads through dispatch as that section.
        for line in page
            .lines()
            .filter(|line| line.starts_with(&format!("- {name}#")))
        {
            let section = line[2..].split(": ").next().unwrap();
            let output = dispatched_help(&mut policy, &workspace, &catalogue, section);
            let text = told(&mut policy, &output.text);
            assert!(!text.starts_with("error:"), "{section}");
            assert_ne!(text, page, "{section}");
            assert_eq!(output.origin, "built-in", "{section}");
        }
        if name != "bravebot-help/specs" {
            let output = dispatched_help(
                &mut policy,
                &workspace,
                &catalogue,
                &format!("{name}::headings"),
            );
            assert!(
                !told(&mut policy, &output.text).starts_with("error:"),
                "{name}"
            );
            let headings = told(&mut policy, &output.text);
            for selector in headings.lines().filter_map(|line| line.strip_prefix("- ")) {
                let output = dispatched_help(&mut policy, &workspace, &catalogue, selector);
                assert!(
                    !told(&mut policy, &output.text).starts_with("error:"),
                    "{selector}"
                );
            }
        }
    }
    for name in [
        "bravebot-help/using/session::headings",
        "bravebot-help/using/sessions.md::headings",
        "bravebot-help/using/../using/sessions::headings",
        "bravebot-help/using//sessions::headings",
        "bravebot-help/using/sessions::headings ",
        "bravebot-help/using/sessions::section:14:The files the session wrot",
        "bravebot-help/using/sessions::section:014:The files the session wrote",
        "bravebot-help/using/sessions::section:13:The files the session wrote",
        "bravebot-help/using/sessions::section:14:the files the session wrote",
        "bravebot-help/using/sessions::section:999:The files the session wrote",
        "bravebot-help/using/sessions::section:The files the session wrote",
        "bravebot-help/specs::headings",
        "bravebot-help/specs/commands::headings",
        "bravebot-help/reference/commands#",
        "bravebot-help/reference/commands#request-",
        "bravebot-help/reference/commands#Request",
        "bravebot-help/reference/commands#request#request",
        "bravebot-help/reference/commands#no-section-has-this-heading",
        "bravebot-help/using/watches#request",
        "bravebot-help/specs/commands#request",
    ] {
        let output = dispatched_help(&mut policy, &workspace, &catalogue, name);
        assert!(
            told(&mut policy, &output.text).starts_with("error: no skill named"),
            "{name}"
        );
    }
}

/// Replacing the index must remove every built-in route, not only the original page route.
#[test]
fn custom_help_dispatch_shadows_pages_headings_sections_and_specs() {
    let fixture = SkillFixture::new("help-all-shadowed-dispatch");
    let own = fixture.home.join(".bravebot/skills/bravebot-help");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(
        own.join("SKILL.md"),
        "---\nname: bravebot-help\ndescription: mine\n---\nmy own help\n",
    )
    .unwrap();
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    let own = dispatched_help(&mut policy, &workspace, &catalogue, "bravebot-help");
    assert_eq!(told(&mut policy, &own.text).trim(), "my own help");
    for name in [
        "bravebot-help/using/sessions",
        "bravebot-help/using/sessions::headings",
        "bravebot-help/using/sessions::section:14:The files the session wrote",
        "bravebot-help/specs",
        "bravebot-help/specs/commands",
        "bravebot-help/reference/commands",
        "bravebot-help/reference/commands#request",
    ] {
        let output = dispatched_help(&mut policy, &workspace, &catalogue, name);
        assert!(
            told(&mut policy, &output.text).starts_with("error: no skill named"),
            "{name}"
        );
    }
}

/// A removed or person-only help skill must not leave compiled routes reachable, and the
/// section syntax must not become a general skill-body lookup.
#[test]
fn excluded_help_catalogues_refuse_every_compiled_route_and_ordinary_skill_selectors() {
    let fixture = SkillFixture::new("help-excluded-dispatch");
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let curated = catalogue.clone().only(&["notes".to_string()]);
    let without = catalogue.without("bravebot-help");
    let own = fixture.home.join(".bravebot/skills/bravebot-help");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(
        own.join("SKILL.md"),
        "---\nname: bravebot-help\ndescription: private help\ndisable-model-invocation: true\n---\nPRIVATE-HELP-BODY\n",
    )
    .unwrap();
    let person_only = fixture.catalogue(&workspace);
    assert!(person_only.get("bravebot-help").unwrap().person_only);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    for catalogue in [&curated, &without, &person_only] {
        for name in [
            "bravebot-help",
            "bravebot-help/using/sessions",
            "bravebot-help/using/sessions::headings",
            "bravebot-help/using/sessions::section:14:The files the session wrote",
            "bravebot-help/specs",
            "bravebot-help/specs/commands",
            "bravebot-help/reference/commands",
            "bravebot-help/reference/commands#request",
            "notes::headings",
            "notes::section:1:Notes",
            "notes#request",
        ] {
            let output = dispatched_help(&mut policy, &workspace, catalogue, name);
            let text = told(&mut policy, &output.text);
            assert!(text.starts_with("error: no skill named"), "{name}: {text}");
            assert!(policy.loaded_skill_directories().is_empty(), "{name}");
        }
    }
    let output = dispatched_help(&mut policy, &workspace, &curated, "notes");
    assert!(told(&mut policy, &output.text).starts_with("body of notes\n"));
}

/// Counts and titles must survive dispatch; merely preserving one known spec hides omissions.
#[test]
fn compiled_help_dispatch_keeps_every_spec_title_and_clause_count() {
    fn specs(directory: &std::path::Path, paths: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                specs(&path, paths);
            } else if path.extension().is_some_and(|ext| ext == "md")
                && path.file_name().unwrap() != "README.md"
            {
                paths.push(path);
            }
        }
    }
    let fixture = SkillFixture::new("help-spec-counts-dispatch");
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specs");
    let mut paths = Vec::new();
    specs(&root, &mut paths);
    let output = dispatched_help(&mut policy, &workspace, &catalogue, "bravebot-help/specs");
    let list = told(&mut policy, &output.text);
    assert_eq!(
        list.lines()
            .filter(|line| line.starts_with("- bravebot-help/specs/"))
            .count(),
        paths.len()
    );
    for path in paths {
        let spec = crate::spec_headings::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let relative = path.strip_prefix(&root).unwrap().with_extension("");
        let name = format!("bravebot-help/specs/{}", relative.display());
        let count = spec.headings.len();
        let noun = if count == 1 { "clause" } else { "clauses" };
        let proposed = if spec.status == "proposed" {
            ", proposed"
        } else {
            ""
        };
        assert!(
            list.contains(&format!(
                "- {name}: {} ({}, {count} {noun}{proposed})\n",
                spec.title, spec.prefix
            )),
            "{name}"
        );
        let output = dispatched_help(&mut policy, &workspace, &catalogue, &name);
        assert!(
            !told(&mut policy, &output.text).starts_with("error:"),
            "{name}"
        );
    }
}

/// The regression it rejects: the index naming pages that `load_skill` then refuses, or a page
/// loading as the index does, or any name but an exact one reaching a page.
#[test]
fn a_documentation_page_loads_by_the_name_the_index_gives_it() {
    let fixture = SkillFixture::new("help-page-loads");
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);

    let index = load_named(&mut policy, &catalogue, "bravebot-help");
    assert!(
        index.contains("- bravebot-help/using/watches: Watching a file."),
        "{index}"
    );
    let page = load_named(&mut policy, &catalogue, "bravebot-help/using/watches");
    assert!(
        page.starts_with("# Watching a file") && !page.contains("sidebar_position"),
        "{page}"
    );
    assert_ne!(page, index);

    let produced = load_skill(
        &mut policy,
        &catalogue,
        Beside::every(),
        &json!({ "name": "bravebot-help/using/watche" }),
    );
    assert!(produced.failed, "a name one character out loaded a page");
}

/// The regression it rejects: a long page loading whole through `load_skill`, or a section name
/// the outline gives being known to `help` and refused by the tool.
#[test]
fn a_long_documentation_page_loads_by_section_through_the_tool() {
    let fixture = SkillFixture::new("help-section-loads");
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);

    let outline = load_named(&mut policy, &catalogue, "bravebot-help/reference/commands");
    // The outline is the opening text and one line per section, not the page: it names the
    // `/request` section and holds none of that section's text.
    assert!(outline.contains("This page is "), "{outline}");
    assert!(
        outline.contains("- bravebot-help/reference/commands#request: `/request` ("),
        "{outline}"
    );
    assert!(
        !outline.contains("Opens a read-only view of the last request"),
        "the page loaded whole: {} characters",
        outline.chars().count()
    );
    assert!(outline.chars().count() < 14_000, "{outline}");

    let section = load_named(
        &mut policy,
        &catalogue,
        "bravebot-help/reference/commands#request",
    );
    assert!(section.starts_with("## `/request`\n"), "{section}");
    assert!(
        section.contains("Opens a read-only view of the last request")
            && section.contains("`unrecorded`")
            && section.ends_with("`/request` followed by words is a prompt."),
        "{section}"
    );
    // The sections on either side of it are not part of it.
    assert!(
        !section.contains("## `/context`")
            && !section.contains("Reports what the last request was made of")
            && !section.contains("## `/status`")
            && !section.contains("Reports everything the session knows"),
        "{section}"
    );
    assert_ne!(section, outline);

    let produced = load_skill(
        &mut policy,
        &catalogue,
        Beside::every(),
        &json!({ "name": "bravebot-help/reference/commands#no-section-has-this-heading" }),
    );
    assert!(produced.failed, "a section the page lacks loaded");
}

/// A helper-only selector or a full-page fallback would leave real help calls just as large.
#[test]
fn the_files_view_answer_loads_through_headings_and_one_small_section() {
    let fixture = SkillFixture::new("help-headings-load");
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    let index = load_named(&mut policy, &catalogue, "bravebot-help");
    let sessions = index
        .lines()
        .find(|line| line.starts_with("- bravebot-help/using/sessions: "))
        .unwrap();
    assert!(sessions.to_lowercase().contains("files"), "{sessions}");
    let headings = load_named(
        &mut policy,
        &catalogue,
        "bravebot-help/using/sessions::headings",
    );
    assert!(
        !headings.contains("Ctrl-C"),
        "body text leaked into headings"
    );
    let selector = headings
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .find(|name| name.ends_with(":The files the session wrote"))
        .unwrap();
    let section = load_named(&mut policy, &catalogue, selector);
    assert!(section.starts_with("### The files the session wrote\n"));
    assert!(
        section.contains("previous") && section.contains("next"),
        "{section}"
    );
    assert!(
        section.contains("Esc") && section.contains("Ctrl-C"),
        "{section}"
    );
    assert!(!section.contains("\n### ") && !section.contains("\n## "));
    // What the two routes cost after the index, which both start from. The bare loads are what
    // the tool returns for the two candidate pages: an outline for a page over 12000 characters.
    let size_of = |page: &str| -> usize {
        let prefix = format!("- bravebot-help/{page}: ");
        let line = index
            .lines()
            .find(|line| line.starts_with(&prefix))
            .unwrap();
        line.rsplit_once('(')
            .and_then(|(_, size)| size.split_whitespace().next())
            .and_then(|size| size.parse().ok())
            .unwrap_or_else(|| panic!("no size on {line}"))
    };
    let mut bare = 0;
    for page in ["reference/cli", "using/interactive-mode"] {
        let loaded = load_named(&mut policy, &catalogue, &format!("bravebot-help/{page}"));
        let loaded = loaded.chars().count();
        // A loader that returned the whole page would be the size the index gives.
        assert!(
            loaded * 5 < size_of(page),
            "{page} loaded {loaded} of {} characters",
            size_of(page)
        );
        bare += loaded;
    }
    let selected = headings.chars().count() + section.chars().count();
    eprintln!(
        "help payload: index={}, headings={}, section={}, bare loads={bare}",
        index.chars().count(),
        headings.chars().count(),
        section.chars().count()
    );
    assert!(selected < 10_000, "selected {selected} characters");
    assert!(selected < bare, "selected {selected}, bare loads {bare}");
}

/// The regression it rejects: the list of specs or a spec's page being known to `help` and not
/// reachable through `load_skill`, or a spec's page answering as the list does.
#[test]
fn a_spec_page_loads_by_the_name_the_list_gives_it() {
    let fixture = SkillFixture::new("help-spec-loads");
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);

    let index = load_named(&mut policy, &catalogue, "bravebot-help");
    assert!(index.contains("- bravebot-help/specs: "), "{index}");
    let list = load_named(&mut policy, &catalogue, "bravebot-help/specs");
    assert!(
        list.contains("- bravebot-help/specs/commands: Slash commands (CMD, "),
        "{list}"
    );
    let page = load_named(&mut policy, &catalogue, "bravebot-help/specs/commands");
    assert!(page.contains("\nCMD-24: "), "{page}");
    assert_ne!(page, list);

    let produced = load_skill(
        &mut policy,
        &catalogue,
        Beside::every(),
        &json!({ "name": "bravebot-help/specs/command" }),
    );
    assert!(produced.failed, "a name one character out loaded a spec");
}

/// The regression it rejects: a page answering for a skill of the user's own that took the
/// place of the built-in index, which would hand the planner built-in text the user replaced.
#[test]
fn a_users_skill_of_the_help_name_takes_the_pages_with_it() {
    let fixture = SkillFixture::new("help-page-shadowed");
    let own = fixture.home.join(".bravebot/skills/bravebot-help");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(
        own.join("SKILL.md"),
        "---\nname: bravebot-help\ndescription: mine\n---\nmy own help\n",
    )
    .unwrap();
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);

    assert_eq!(
        load_named(&mut policy, &catalogue, "bravebot-help").trim(),
        "my own help"
    );
    let produced = load_skill(
        &mut policy,
        &catalogue,
        Beside::every(),
        &json!({ "name": "bravebot-help/using/watches" }),
    );
    assert!(
        produced.failed,
        "a built-in page answered for the user's skill"
    );
    let produced = load_skill(
        &mut policy,
        &catalogue,
        Beside::every(),
        &json!({ "name": "bravebot-help/specs/commands" }),
    );
    assert!(
        produced.failed,
        "a built-in spec page answered for the user's skill"
    );
    for name in [
        "bravebot-help/reference/commands",
        "bravebot-help/reference/commands#request",
    ] {
        let produced = load_skill(
            &mut policy,
            &catalogue,
            Beside::every(),
            &json!({ "name": name }),
        );
        assert!(
            produced.failed,
            "{name} answered for the user's skill: a long page's outline or section"
        );
    }
}

/// The regression it rejects: a skill's sibling file being readable before the skill is
/// loaded, never being readable, or being read as quarantined (a map answering about a
/// directory it does not govern). A skill that is not loaded is the control, so reach that
/// was granted to every skill in the directory would pass the first assertion only.
#[test]
fn a_home_skills_sibling_file_is_readable_after_it_is_loaded_and_not_before() {
    let fixture = SkillFixture::new("skill-files-read");
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    let read = |policy: &mut Policy<'_, RecordingSink>, path: &str| {
        skill_call(policy, &workspace, "read_file", json!({ "path": path }))
    };

    let (failed, _, before) = read(&mut policy, "~/.bravebot/skills/notes/reference.md");
    assert!(
        failed && !before.contains(SKILL_FILE_MARKER),
        "read before load: {before}"
    );

    let loaded = load_named(&mut policy, &catalogue, "notes");
    assert!(
        loaded.contains("~/.bravebot/skills/notes")
            && loaded.contains("- reference.md\n")
            && loaded.contains("- scripts/helper.sh\n"),
        "the listing is missing: {loaded}"
    );

    let (failed, trusted, after) = read(&mut policy, "~/.bravebot/skills/notes/reference.md");
    assert!(
        !failed && after.contains(SKILL_FILE_MARKER),
        "read after load: {after}"
    );
    assert!(
        trusted,
        "the file was labelled by the map, not by provenance"
    );

    let (failed, _, nested) = read(&mut policy, "~/.bravebot/skills/notes/scripts/helper.sh");
    assert!(
        !failed && nested.contains(SKILL_FILE_MARKER),
        "nested: {nested}"
    );

    let (failed, _, other) = read(&mut policy, "~/.bravebot/skills/other/unloaded.md");
    assert!(
        failed && !other.contains(SKILL_FILE_MARKER),
        "a skill nobody loaded was reached: {other}"
    );
}

/// `search_skills` refuses a missing or blank query, matches by name, and never cuts a
/// description, mirroring `load_skill`'s own argument handling (SKILL-18).
#[test]
fn search_skills_matches_by_name_and_refuses_a_blank_query() {
    let fixture = SkillFixture::new("skill-search");
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);

    let produced = search_skills(&mut policy, &catalogue, &json!({}));
    assert!(produced.failed, "a missing query was not refused");
    assert_eq!(
        told(&mut policy, &produced.text),
        "error: 'query' is required and must be a string"
    );

    let produced = search_skills(&mut policy, &catalogue, &json!({ "query": "   " }));
    assert!(produced.failed, "a blank query was not refused");
    assert_eq!(
        told(&mut policy, &produced.text),
        "error: 'query' must not be blank"
    );

    let produced = search_skills(&mut policy, &catalogue, &json!({ "query": "notes" }));
    assert!(!produced.failed, "a matching query was refused");
    let said = told(&mut policy, &produced.text);
    assert!(
        said.contains("- notes: d") && !said.contains("- other: "),
        "the query matched something other than the skill named notes: {said}"
    );

    let produced = search_skills(&mut policy, &catalogue, &json!({ "query": "no-such-word" }));
    assert!(
        !produced.failed,
        "a search with no match was treated as an error"
    );
    assert_eq!(
        told(&mut policy, &produced.text),
        "No skill matched that search."
    );
}

/// `search_skills` says how many further matches there were past the cap (FINDSKILL-4),
/// through the tool call itself rather than only through `Catalogue::search`, which this
/// line reads but does not render.
#[test]
fn search_skills_says_how_many_more_matched_past_the_cap() {
    let scratch = ArgumentsScratch::new("skill-search-cap");
    let home = scratch.path.join("h");
    for n in 0..30 {
        let at = home.join(".bravebot/skills").join(format!("broad-{n:02}"));
        std::fs::create_dir_all(&at).unwrap();
        std::fs::write(
            at.join("SKILL.md"),
            "---\nname: broad\ndescription: matches a broad query\n---\nbody\n"
                .replace("broad\ndescription", &format!("broad-{n:02}\ndescription")),
        )
        .unwrap();
    }
    let project = scratch.path.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let workspace = Workspace::new(&project)
        .expect("workspace")
        .with_home(Some(home.clone()));
    let catalogue = crate::skills::resolved(
        &workspace,
        Some(&home.join(".bravebot")),
        TrustStore::new("/work"),
        bravebot_core::permissions::Permissions::default(),
        &mut RecordingSink::new(),
    );

    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    let produced = search_skills(&mut policy, &catalogue, &json!({ "query": "broad" }));
    assert!(!produced.failed, "a broad query was refused");
    let said = told(&mut policy, &produced.text);
    assert!(
        said.contains("...and 5 more matched"),
        "the overflow line is missing or wrong: {said}"
    );
}

/// The regression it rejects: the reach being a general opening of the directory, so a
/// planner writes into the user's own configuration or climbs out of the skill's directory.
#[test]
fn reaching_a_loaded_skills_files_is_for_reading_inside_the_directory_only() {
    let fixture = SkillFixture::new("skill-files-confined");
    let workspace = fixture.workspace();
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    load_named(&mut policy, &catalogue, "notes");

    let target = fixture.home.join(".bravebot/skills/notes/reference.md");
    let (failed, _, said) = skill_call(
        &mut policy,
        &workspace,
        "write_file",
        json!({"path": target.to_string_lossy(), "contents": "planted"}),
    );
    assert!(failed, "a write beside a loaded skill was accepted: {said}");
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        format!("{SKILL_FILE_MARKER} reference.md\n"),
        "the file was changed"
    );

    for path in [
        "~/.bravebot/skills/notes/../../../secret.md",
        "~/.bravebot/skills/notes/escape.md",
        "~/secret.md",
    ] {
        let (failed, _, said) = skill_call(
            &mut policy,
            &workspace,
            "read_file",
            json!({ "path": path }),
        );
        assert!(
            failed && !said.contains("OUTSIDE-EVERY-SKILL"),
            "{path} reached a file outside the skill's directory: {said}"
        );
    }
}

/// The regression it rejects: a link in a loaded skill's directory that lands in a directory
/// the person added, or in the session's own, being read under the skill's provenance label.
/// The file is outside the skill's directory, so it is refused. A link that stays inside the
/// directory is the control that keeps the refusal from being a blanket one.
#[test]
fn a_link_from_a_loaded_skill_into_an_opened_directory_is_refused() {
    let fixture = SkillFixture::new("skill-files-link-into-opened");
    let outside = fixture.project.parent().unwrap();
    let added = outside.join("shared");
    let scratch = outside.join("scratch");
    for (directory, file) in [(&added, "untrusted.md"), (&scratch, "note.md")] {
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(directory.join(file), "NOT-THE-SKILLS-OWN\n").unwrap();
    }
    let skill = fixture.home.join(".bravebot/skills/notes");
    for (name, target) in [
        ("added.md", added.join("untrusted.md")),
        ("scratch.md", scratch.join("note.md")),
        ("alias.md", skill.join("reference.md")),
    ] {
        std::os::unix::fs::symlink(target, skill.join(name)).unwrap();
    }
    let mut workspace = fixture.workspace();
    workspace
        .add_directory(&added.to_string_lossy())
        .expect("an added directory");
    workspace.open_scratch(crate::scratch::ScratchDirectory::observed(
        scratch.canonicalize().unwrap(),
    ));
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    load_named(&mut policy, &catalogue, "notes");

    for path in [
        "~/.bravebot/skills/notes/added.md",
        "~/.bravebot/skills/notes/scratch.md",
    ] {
        let (failed, _, said) = skill_call(
            &mut policy,
            &workspace,
            "read_file",
            json!({ "path": path }),
        );
        assert!(
            failed && !said.contains("NOT-THE-SKILLS-OWN"),
            "{path} reached a file outside the skill's directory: {said}"
        );
    }

    let (failed, trusted, said) = skill_call(
        &mut policy,
        &workspace,
        "read_file",
        json!({ "path": "~/.bravebot/skills/notes/alias.md" }),
    );
    assert!(
        !failed && trusted && said.contains(SKILL_FILE_MARKER),
        "a link inside the skill's directory was refused: {said}"
    );
}

/// The regression it rejects: a `write_file` into the session's own directory being counted as
/// a change to the project, so the turn ends saying a change was not built when the project is
/// as it was. The control is a write into the project, which still counts.
///
/// The test is the resolved destination, never the name typed: a link in an added directory
/// into the session's directory is not counted, and a link in the session's directory out to the
/// added directory is. The regression it rejects is a test on the name typed. A link between the
/// project and the session's directory is refused by the workspace before any flag is set. With
/// no session directory every write counts.
#[test]
fn a_write_into_the_session_directory_is_not_a_changed_file() {
    let project = arguments::Scratch::new("changed-flag-project");
    let session = arguments::Scratch::new("changed-flag-session");
    let session_path = session.path.canonicalize().unwrap();
    let mut workspace = Workspace::new(&project.path).expect("workspace");
    workspace.open_scratch(crate::scratch::ScratchDirectory::observed(
        session_path.clone(),
    ));
    let changed_in = |workspace: &Workspace, path: String| {
        let mut sink = RecordingSink::new();
        let mut policy = skill_policy(&mut sink);
        let call: ToolCall = serde_json::from_value(json!({
            "id": "a",
            "function": {
                "name": "write_file",
                "arguments": json!({"path": path, "contents": "x"}).to_string()
            }
        }))
        .expect("a call");
        let produced = with_tools(workspace, |tools| {
            dispatch(
                &mut policy,
                tools,
                &mut crate::confirm::ApproveWrites,
                &mut crate::report::IgnoreReports,
                &call,
            )
        });
        (
            produced.changed_a_file,
            produced.wrote_in_the_session_directory,
        )
    };
    let changed = |path: String| changed_in(&workspace, path).0;
    let in_session = session_path.join("note.txt");
    assert_eq!(
        changed_in(&workspace, in_session.to_string_lossy().into_owned()),
        (false, true),
        "a write in the session directory is marked apart and not as a change"
    );
    assert!(in_session.exists(), "the write did not land");
    assert_eq!(
        changed_in(&workspace, "kept.txt".to_string()),
        (true, false),
        "a project write is a change and is not marked as one in the session directory"
    );

    // A link in an added directory that lands in the session's directory: the name typed is
    // under the added directory, the write lands in the session's, and only the landing counts.
    let extra = arguments::Scratch::new("changed-flag-extra");
    let extra_path = extra.path.canonicalize().unwrap();
    workspace
        .open_directory(extra_path.to_str().unwrap())
        .unwrap();
    std::os::unix::fs::symlink(&session_path, extra_path.join("into-session")).unwrap();
    let by_link = extra_path.join("into-session/via-link.txt");
    assert!(
        !changed(by_link.to_string_lossy().into_owned()),
        "a write through a link into the session directory was counted"
    );
    assert!(session_path.join("via-link.txt").exists(), "did not land");

    // A link in the session's directory that lands in the added directory: typed under the
    // session's directory, landed outside it, so it counts.
    std::os::unix::fs::symlink(&extra_path, session_path.join("into-extra")).unwrap();
    let by_link = session_path.join("into-extra/from-session.txt");
    assert!(
        changed(by_link.to_string_lossy().into_owned()),
        "a write through a link out of the session directory was not counted"
    );
    assert!(extra_path.join("from-session.txt").exists(), "did not land");

    // A session with no directory of its own counts every write.
    let bare = Workspace::new(&project.path).expect("workspace");
    assert!(changed_in(&bare, "plain.txt".to_string()).0);
}

/// GROUND-1. An `edit_file`, a `download_url` and a picture written from a reference each mark
/// a write that landed in the session's own directory apart from a change to the project, as
/// `write_file` does. The control for each is the same call into the project, which is a
/// change and is not marked as a session write.
///
/// The regression it rejects: one of those tools setting the changed-file flag whatever it
/// landed on, or never setting the session flag, so a delegate's finished note leaves out that
/// write or counts it in the project.
#[expect(
    clippy::too_many_lines,
    reason = "one fixture serves three tools and their controls, and the closure that runs a tool shares its setup with the assertions that follow"
)]
#[cfg(unix)]
#[test]
fn an_edit_a_download_and_a_picture_in_the_session_directory_are_marked_as_session_writes() {
    use std::io::{Read, Write};

    let png = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==",
            )
            .unwrap()
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let site = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut head = [0u8; 2048];
            let _read = stream.read(&mut head);
            stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 5\r\nConnection: close\r\n\r\nbytes",
            ).ok();
        }
    });

    let flags = |tool: &str, in_session: bool| {
        let project = arguments::Scratch::new(&format!("marks-project-{tool}-{in_session}"));
        let session = arguments::Scratch::new(&format!("marks-session-{tool}-{in_session}"));
        let session_path = session.path.canonicalize().unwrap();
        let mut workspace = Workspace::new(&project.path).unwrap();
        workspace.open_scratch(crate::scratch::ScratchDirectory::observed(
            session_path.clone(),
        ));
        let home = match in_session {
            true => session_path.clone(),
            false => project.path.clone(),
        };
        let destination = |file: &str| match in_session {
            true => session_path.join(file).to_string_lossy().into_owned(),
            false => file.to_string(),
        };
        let mut trust = TrustStore::new(&project.path);
        if tool != "write_file" {
            trust.trust(".");
            trust.trust(&session_path.to_string_lossy());
        }
        let mut routing = Routing::new();
        routing.insert_trusted("task", "save a file");
        let mut sink = RecordingSink::new();
        let (rules, rejected) = bravebot_core::permissions::Permissions::parse(
            &[],
            &[],
            &["WebFetch(domain:127.0.0.1)".to_string()],
            &bravebot_core::permissions::Anchors::none(),
        );
        assert!(rejected.is_empty(), "the rule did not parse");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([
                Capability::FileRead,
                Capability::FileWrite,
                Capability::WebFetch,
            ]),
            &mut sink,
        )
        .unwrap()
        .with_trust(trust)
        .with_permissions(rules);
        let calls = match tool {
            "edit_file" => {
                std::fs::write(home.join("target.txt"), "old\n").unwrap();
                vec![json!({
                    "name": "edit_file",
                    "arguments": json!({
                        "path": destination("target.txt"),
                        "old_text": "old",
                        "new_text": "new",
                    }),
                })]
            }
            "download_url" => vec![json!({
                "name": "download_url",
                "arguments": json!({
                    "url": format!("{site}/logo.png"),
                    "path": destination("logo.png"),
                }),
            })],
            _ => {
                std::fs::write(project.path.join("shot.png"), &png).unwrap();
                vec![json!({
                    "name": "write_file",
                    "arguments": json!({
                        "path": destination("copy.png"),
                        "contents_ref": "ref:1",
                    }),
                })]
            }
        };
        let last = with_tools(&workspace, |tools| {
            if tool == "write_file" {
                // The name of a listed file, labelled through a call whose use count LABEL
                // does not pin, so this test adds no pinned site.
                let name = policy
                    .label_user_configuration("a listing of the project", "shot.png".to_string());
                policy
                    .defer(
                        "list_files",
                        SlotId::new("ref:1"),
                        "shot.png",
                        &name,
                        png.len(),
                        tools.slots,
                    )
                    .unwrap();
            }
            calls
                .iter()
                .map(|call| {
                    let call: ToolCall = serde_json::from_value(json!({
                        "id": "a",
                        "function": {
                            "name": call["name"],
                            "arguments": call["arguments"].to_string(),
                        }
                    }))
                    .unwrap();
                    dispatch(
                        &mut policy,
                        tools,
                        &mut crate::confirm::ApproveWrites,
                        &mut crate::report::IgnoreReports,
                        &call,
                    )
                })
                .last()
                .unwrap()
        });
        let landed = match tool {
            "edit_file" => "target.txt",
            "download_url" => "logo.png",
            _ => "copy.png",
        };
        let file = std::fs::read(home.join(landed)).unwrap_or_default();
        assert!(
            !file.is_empty() && file != b"old\n",
            "{tool} in_session={in_session}: the write did not land: {}",
            told(&mut policy, &last.text)
        );
        (last.changed_a_file, last.wrote_in_the_session_directory)
    };
    for tool in ["edit_file", "download_url", "write_file"] {
        assert_eq!(
            flags(tool, true),
            (false, true),
            "{tool} into the session directory is not marked as a session write alone"
        );
        assert_eq!(
            flags(tool, false),
            (true, false),
            "{tool} into the project is not marked as a change alone"
        );
    }
}

/// A redirection destination is listed with the file it lands on when that file is not the
/// one named. The regression it rejects: comparing the text shown for the two, which is the
/// same for two names that differ only in bytes that are not UTF-8, so a link to such a name
/// was listed as landing where it is named (PATH-001). The first pair is the control: a link
/// whose target shows differently. The second pair is made on Linux only, since other hosts
/// refuse a name that is not text.
#[test]
fn a_link_to_a_name_that_differs_only_in_bytes_that_are_not_text_is_listed_with_its_landing() {
    use std::os::unix::ffi::OsStrExt;

    let top = crate::testutil::scratch_dir("landing-not-text");
    let _ = std::fs::remove_dir_all(&top);
    std::fs::create_dir_all(&top).unwrap();
    let top = top.canonicalize().unwrap();
    let named = |bytes: &[u8]| top.join(std::ffi::OsStr::from_bytes(bytes));
    let mut pairs = vec![(&b"out-plain"[..], &b"real-plain"[..])];
    if cfg!(target_os = "linux") {
        pairs.push((&b"out-\xff"[..], &b"out-\xfe"[..]));
    }
    for (link, target) in pairs {
        std::fs::write(named(target), "x").unwrap();
        std::os::unix::fs::symlink(named(target), named(link)).unwrap();
        assert_eq!(
            landing_other_than_the_name(&named(link)),
            Some(named(target))
        );
    }
    std::fs::write(named(b"plain-file"), "x").unwrap();
    assert_eq!(landing_other_than_the_name(&named(b"plain-file")), None);
}

/// A redirection into a file that is a hard link to a protected one is protected, and so is
/// one whose own name is not UTF-8, since its lossy rendering is the name of a different file
/// and the walk for its other names would look for that one (PATH-001). The regression it
/// rejects: spelling the name lossily before the lookup, which found no file and judged
/// `out-\xff` unprotected. The plain-named link and the unlinked file are the controls and
/// run on every Unix host; the name that is not text is made on Linux only.
#[test]
fn a_redirection_into_a_hard_link_to_a_protected_file_is_protected_whatever_the_link_is_called() {
    use std::os::unix::ffi::OsStrExt;

    let top = crate::testutil::scratch_dir("redirect-hard-link-not-text");
    let _ = std::fs::remove_dir_all(&top);
    std::fs::create_dir_all(&top).unwrap();
    let top = top.canonicalize().unwrap();
    let workspace = Workspace::new(&top).expect("workspace");
    std::fs::write(top.join(".gitconfig"), "[user]\n").unwrap();
    std::fs::write(top.join("plain.txt"), "plain\n").unwrap();
    let named = |bytes: &[u8]| top.join(std::ffi::OsStr::from_bytes(bytes));
    std::fs::hard_link(top.join(".gitconfig"), named(b"out-plain")).unwrap();
    assert!(redirection_is_protected(&workspace, &named(b"out-plain")));
    assert!(!redirection_is_protected(
        &workspace,
        &top.join("plain.txt")
    ));
    if cfg!(target_os = "linux") {
        std::fs::hard_link(top.join(".gitconfig"), named(b"out-\xff")).unwrap();
        assert!(redirection_is_protected(&workspace, &named(b"out-\xff")));
    }
}

/// The regression it rejects: a comparison of where a link lands with the skill's directory
/// that renders both lossily, so a link into an opened directory whose name is not text is
/// read as inside the skill's directory named with the replacement character (PATH-003).
#[cfg(target_os = "linux")]
#[test]
fn a_link_into_a_directory_whose_name_is_not_text_is_not_taken_for_its_lookalike_skill() {
    use std::os::unix::ffi::OsStrExt;

    let fixture = SkillFixture::new("skill-files-link-not-text");
    let skills = fixture.home.join(".bravebot/skills");
    let lookalike = skills.join("shared-\u{FFFD}");
    std::fs::create_dir_all(&lookalike).unwrap();
    std::fs::write(
        lookalike.join("SKILL.md"),
        "---\nname: lookalike\ndescription: d\n---\nbody\n",
    )
    .unwrap();
    std::fs::write(lookalike.join("own.md"), "THE-SKILLS-OWN\n").unwrap();
    let scratch = skills.join(std::ffi::OsStr::from_bytes(b"shared-\xff"));
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::write(scratch.join("untrusted.md"), "NOT-THE-SKILLS-OWN\n").unwrap();
    std::os::unix::fs::symlink(scratch.join("untrusted.md"), lookalike.join("out.md")).unwrap();
    std::os::unix::fs::symlink(lookalike.join("own.md"), lookalike.join("in.md")).unwrap();
    let mut workspace = fixture.workspace();
    workspace.open_scratch(crate::scratch::ScratchDirectory::observed(
        scratch.canonicalize().unwrap(),
    ));
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    load_named(&mut policy, &catalogue, "lookalike");

    let (failed, _, said) = skill_call(
        &mut policy,
        &workspace,
        "read_file",
        json!({ "path": "~/.bravebot/skills/shared-\u{FFFD}/out.md" }),
    );
    assert!(
        failed && !said.contains("NOT-THE-SKILLS-OWN"),
        "a link into a directory that is not text was read as the skill's own: {said}"
    );
    let (failed, trusted, said) = skill_call(
        &mut policy,
        &workspace,
        "read_file",
        json!({ "path": "~/.bravebot/skills/shared-\u{FFFD}/in.md" }),
    );
    assert!(
        !failed && trusted && said.contains("THE-SKILLS-OWN"),
        "a link inside the skill's directory was refused: {said}"
    );
}

/// The regression it rejects: a setting that keeps the file tools inside the project (PERM-16)
/// being bypassed by the reach a load grants.
#[test]
fn a_load_does_not_widen_a_session_kept_inside_its_project() {
    let fixture = SkillFixture::new("skill-files-kept-inside");
    let workspace = fixture.workspace().with_reads_kept_inside(true);
    let catalogue = fixture.catalogue(&workspace);
    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    load_named(&mut policy, &catalogue, "notes");

    let (failed, _, said) = skill_call(
        &mut policy,
        &workspace,
        "read_file",
        json!({"path": "~/.bravebot/skills/notes/reference.md"}),
    );
    assert!(failed && !said.contains(SKILL_FILE_MARKER), "{said}");
}

/// The regression it rejects: a listing that follows a link out of the directory, names the
/// skill's own file, or names a file a deny rule covers. The skill is listed with the file
/// beside it as the control, since an empty listing would pass every "not named" check.
#[test]
fn the_listing_names_regular_files_inside_the_directory_and_nothing_a_rule_covers() {
    let fixture = SkillFixture::new("skill-files-listing");
    let workspace = fixture.workspace();
    let (rules, rejected) = bravebot_core::permissions::Permissions::parse(
        &["Read(~/.bravebot/skills/notes/scripts/**)".to_string()],
        &[],
        &[],
        &bravebot_core::permissions::Anchors {
            home: Some(fixture.home.to_string_lossy().into_owned()),
            ..bravebot_core::permissions::Anchors::none()
        },
    );
    assert!(rejected.is_empty(), "the rule did not parse: {rejected:?}");
    let all = fixture.catalogue(&workspace);
    let covered = crate::skills::resolved(
        &workspace,
        Some(&fixture.home.join(".bravebot")),
        TrustStore::new("/work"),
        rules,
        &mut RecordingSink::new(),
    );

    let files = |catalogue: &crate::skills::Catalogue| {
        catalogue.get("notes").expect("offered").files.clone()
    };
    assert_eq!(files(&all), ["reference.md", "scripts/helper.sh"]);
    assert_eq!(
        files(&covered),
        ["reference.md"],
        "a file a deny rule covers was named"
    );
}

/// The regression it rejects: a listing that renders a name lossily, so a file whose name is
/// not text is listed under the replacement-character name of a different file (PATH-003).
#[cfg(target_os = "linux")]
#[test]
fn the_listing_leaves_out_a_name_that_is_not_text_and_does_not_merge_it_with_its_lookalike() {
    use std::os::unix::ffi::OsStrExt;

    let fixture = SkillFixture::new("skill-files-not-text");
    let workspace = fixture.workspace();
    let beside = fixture.home.join(".bravebot/skills/notes");
    std::fs::write(beside.join(std::ffi::OsStr::from_bytes(b"x-\xff")), "x").unwrap();
    std::fs::write(beside.join("x-\u{FFFD}"), "x").unwrap();

    let catalogue = fixture.catalogue(&workspace);
    let files = &catalogue.get("notes").expect("offered").files;
    assert_eq!(
        files
            .iter()
            .filter(|name| name.as_str() == "x-\u{FFFD}")
            .count(),
        1,
        "{files:?}"
    );
    assert_eq!(files.len(), 3, "{files:?}");
}

/// The regression it rejects: a skill directory that has had its scripts run listing the
/// bytecode cache first, so a `.pyc` is the file the result offers as the example (LOAD-4).
#[test]
fn the_listing_leaves_out_generated_directories_and_compiled_files() {
    let fixture = SkillFixture::new("skill-files-generated");
    let workspace = fixture.workspace();
    let beside = fixture.home.join(".bravebot/skills/notes");
    std::fs::create_dir_all(beside.join("__pycache__")).unwrap();
    std::fs::create_dir_all(beside.join("node_modules/dep")).unwrap();
    std::fs::write(beside.join("__pycache__/demo.cpython-312.pyc"), "x").unwrap();
    std::fs::write(beside.join("node_modules/dep/index.js"), "x").unwrap();
    std::fs::write(beside.join("stray.pyc"), "x").unwrap();
    std::fs::write(beside.join("demo.py"), "x").unwrap();

    let catalogue = fixture.catalogue(&workspace);
    let files = &catalogue.get("notes").expect("offered").files;
    assert_eq!(files, &["demo.py", "reference.md", "scripts/helper.sh"]);
}

/// The regression it rejects: a listing that names a file whose name holds a control
/// character, so a newline in a name starts a line of its own in the result a planner reads
/// (LOAD-4). The plain file beside them is the control: an empty listing would pass.
#[test]
fn the_listing_leaves_out_a_name_holding_a_control_character() {
    let fixture = SkillFixture::new("skill-files-control");
    let workspace = fixture.workspace();
    let beside = fixture.home.join(".bravebot/skills/notes");
    std::fs::write(beside.join("two\nlines.md"), "x").unwrap();
    std::fs::write(beside.join("tab\there.md"), "x").unwrap();

    let catalogue = fixture.catalogue(&workspace);
    let files = &catalogue.get("notes").expect("offered").files;
    assert_eq!(files, &["reference.md", "scripts/helper.sh"]);
}

/// The regression it rejects: a listing with no count cap, so a skill that keeps thousands
/// of files makes an unbounded `load_skill` result (LOAD-4). The cap keeps the first names
/// in order, which is what makes the same directory read the same every time.
#[test]
fn the_listing_stops_at_two_hundred_files() {
    let fixture = SkillFixture::new("skill-files-count-cap");
    let workspace = fixture.workspace();
    let beside = fixture.home.join(".bravebot/skills/notes");
    for n in 0..205 {
        std::fs::write(beside.join(format!("a{n:03}.md")), "x").unwrap();
    }

    let catalogue = fixture.catalogue(&workspace);
    let files = &catalogue.get("notes").expect("offered").files;
    assert_eq!(files.len(), 200, "{}", files.len());
    assert_eq!(files.first().map(String::as_str), Some("a000.md"));
    assert_eq!(files.last().map(String::as_str), Some("a199.md"));
}

/// The regression it rejects: a listing that searches without a depth bound, or one that
/// stops a level early or late (LOAD-4). A file under six directories is named and one
/// under seven is not.
#[test]
fn the_listing_searches_six_directories_down_and_no_further() {
    let fixture = SkillFixture::new("skill-files-depth-cap");
    let workspace = fixture.workspace();
    let beside = fixture.home.join(".bravebot/skills/notes");
    let six = beside.join("d1/d2/d3/d4/d5/d6");
    std::fs::create_dir_all(six.join("d7")).unwrap();
    std::fs::write(six.join("at-six.md"), "x").unwrap();
    std::fs::write(six.join("d7/at-seven.md"), "x").unwrap();

    let catalogue = fixture.catalogue(&workspace);
    let files = &catalogue.get("notes").expect("offered").files;
    assert_eq!(
        files,
        &[
            "d1/d2/d3/d4/d5/d6/at-six.md",
            "reference.md",
            "scripts/helper.sh"
        ]
    );
}

/// The regression it rejects: a project's skill naming a file the trust map does not vouch
/// for (a name is content), or being handed the provenance reach that is for the user's own.
#[test]
fn a_project_skill_lists_only_the_files_the_trust_map_vouches_for() {
    let fixture = SkillFixture::new("skill-files-project");
    let at = fixture.project.join(".bravebot/skills/lint");
    std::fs::create_dir_all(at.join("vetted")).unwrap();
    std::fs::create_dir_all(at.join("unvetted")).unwrap();
    std::fs::write(
        at.join("SKILL.md"),
        "---\nname: lint\ndescription: d\n---\nbody\n",
    )
    .unwrap();
    std::fs::write(at.join("vetted/a.md"), "a").unwrap();
    std::fs::write(at.join("unvetted/b.md"), "b").unwrap();
    let workspace = fixture.workspace();
    let mut store = TrustStore::new("/work");
    store.trust(".bravebot/skills");
    store.distrust(".bravebot/skills/lint/unvetted");
    let catalogue = crate::skills::resolved(
        &workspace,
        None,
        store,
        bravebot_core::permissions::Permissions::default(),
        &mut RecordingSink::new(),
    );

    let skill = catalogue.get("lint").expect("offered");
    assert_eq!(skill.files, ["vetted/a.md"]);
    assert_eq!(skill.directory.as_deref(), Some(".bravebot/skills/lint"));
    assert!(skill.reach().is_none(), "a project skill was given reach");
}

/// LOAD-4: a project skill whose directory is a link to another directory in the workspace is
/// offered with where the link lands, because a listing of the link's parent shows nothing.
/// The regression it rejects: the files being named under the link's own name only, or a
/// landing outside the workspace being named.
#[test]
fn a_project_skill_reached_through_a_link_names_where_it_lands_inside_the_workspace_only() {
    let fixture = SkillFixture::new("skill-files-link");
    let target = fixture.project.join("shared/lint");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("SKILL.md"),
        "---\nname: lint\ndescription: d\n---\nbody\n",
    )
    .unwrap();
    std::fs::write(target.join("a.md"), "a").unwrap();
    std::fs::create_dir_all(fixture.project.join(".bravebot/skills")).unwrap();
    std::os::unix::fs::symlink(&target, fixture.project.join(".bravebot/skills/lint")).unwrap();

    let elsewhere = fixture.home.join("outside-the-workspace");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(
        elsewhere.join("SKILL.md"),
        "---\nname: away\ndescription: d\n---\nbody\n",
    )
    .unwrap();
    std::fs::write(elsewhere.join("b.md"), "b").unwrap();
    std::os::unix::fs::symlink(&elsewhere, fixture.project.join(".bravebot/skills/away")).unwrap();

    let workspace = fixture.workspace();
    let mut store = TrustStore::new("/work");
    store.trust(".");
    let catalogue = crate::skills::resolved(
        &workspace,
        None,
        store,
        bravebot_core::permissions::Permissions::default(),
        &mut RecordingSink::new(),
    );

    let skill = catalogue.get("lint").expect("offered");
    assert_eq!(skill.landed.as_deref(), Some("shared/lint"));
    assert_eq!(skill.files, ["a.md"]);
    assert!(
        catalogue.get("away").is_none(),
        "a skill whose link lands outside the workspace was offered"
    );

    let mut sink = RecordingSink::new();
    let mut policy = skill_policy(&mut sink);
    let loaded = load_named(&mut policy, &catalogue, "lint");
    assert!(
        loaded.contains("a link to shared/lint") && loaded.contains("shared/lint/a.md"),
        "{loaded}"
    );
    assert!(
        !loaded.contains("outside-the-workspace"),
        "a path outside the workspace was named: {loaded}"
    );
}

/// LOAD-4: where a project skill's directory is a link, each file is judged by the trust map
/// under the name the link gives it and under the name where it lands. The regression it
/// rejects: the files being judged under one of the two names only, so a file the map
/// distrusts under the other name is still offered. `ok.md` is the control: it is trusted
/// under both names.
#[test]
fn a_file_of_a_linked_skill_is_judged_under_both_the_link_name_and_the_landed_name() {
    let fixture = SkillFixture::new("skill-files-link-both-names");
    let target = fixture.project.join("shared/lint");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("SKILL.md"),
        "---\nname: lint\ndescription: d\n---\nbody\n",
    )
    .unwrap();
    for file in ["ok.md", "distrusted-landed.md", "distrusted-link.md"] {
        std::fs::write(target.join(file), "x").unwrap();
    }
    std::fs::create_dir_all(fixture.project.join(".bravebot/skills")).unwrap();
    std::os::unix::fs::symlink(&target, fixture.project.join(".bravebot/skills/lint")).unwrap();

    let workspace = fixture.workspace();
    let mut store = TrustStore::new("/work");
    store.trust(".");
    store.distrust("shared/lint/distrusted-landed.md");
    store.distrust(".bravebot/skills/lint/distrusted-link.md");
    let catalogue = crate::skills::resolved(
        &workspace,
        None,
        store,
        bravebot_core::permissions::Permissions::default(),
        &mut RecordingSink::new(),
    );

    let skill = catalogue.get("lint").expect("offered");
    assert_eq!(skill.landed.as_deref(), Some("shared/lint"));
    assert_eq!(skill.files, ["ok.md"]);
}

/// LOAD-4: a link that lands where the trust map distrusts the directory is named no landing,
/// and the skill is still offered. A link that lands where a deny rule covers the directory is
/// not offered at all, since the rule covers the `SKILL.md` in it (PERM-3). The regression it
/// rejects: a landing path named to the planner without the rules and the trust map having
/// been asked about it. The first skill is the control: it lands where both allow.
#[test]
fn a_landing_a_rule_denies_or_the_trust_map_distrusts_is_not_named() {
    let fixture = SkillFixture::new("skill-files-link-refused");
    std::fs::create_dir_all(fixture.project.join(".bravebot/skills")).unwrap();
    for name in ["open", "denied", "distrusted"] {
        let target = fixture.project.join(format!("shared/{name}"));
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            target.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: d\n---\nbody\n"),
        )
        .unwrap();
        std::fs::write(target.join("a.md"), "a").unwrap();
        std::os::unix::fs::symlink(
            &target,
            fixture.project.join(format!(".bravebot/skills/{name}")),
        )
        .unwrap();
    }

    let workspace = fixture.workspace();
    let mut store = TrustStore::new("/work");
    store.trust(".");
    store.distrust("shared/distrusted");
    store.trust("shared/distrusted/SKILL.md");
    let (rules, rejected) = bravebot_core::permissions::Permissions::parse(
        &["Read(shared/denied)".to_string()],
        &[],
        &[],
        &bravebot_core::permissions::Anchors::none(),
    );
    assert!(rejected.is_empty(), "the rule did not parse: {rejected:?}");
    let catalogue =
        crate::skills::resolved(&workspace, None, store, rules, &mut RecordingSink::new());

    let landed = |name: &str| catalogue.get(name).map(|skill| skill.landed.clone());
    assert_eq!(landed("open"), Some(Some("shared/open".to_string())));
    assert_eq!(
        landed("denied"),
        None,
        "a skill in a directory a deny rule covers was offered"
    );
    assert_eq!(
        landed("distrusted"),
        Some(None),
        "a distrusted landing was named"
    );
}

/// LOAD-4: a link that lands in a directory whose name is not text is named no landing, and a
/// rule about the directory whose name spells the replacement-character rendering of it decides
/// that directory only (PATH-003). The regression it rejects: the landing being judged and named
/// on a lossy rendering of its path, so the invalid name is handed to the planner as its
/// lookalike, or is allowed or denied by what the rules say about the lookalike. Linux only,
/// because the name has to exist on disk.
#[cfg(target_os = "linux")]
#[test]
fn a_landing_whose_name_is_not_text_is_not_named_as_its_lookalike() {
    use std::os::unix::ffi::OsStrExt;

    let fixture = SkillFixture::new("skill-files-link-not-text");
    std::fs::create_dir_all(fixture.project.join(".bravebot/skills")).unwrap();
    let invalid = std::ffi::OsStr::from_bytes(b"lint-\xff");
    for (name, directory) in [
        ("invalid", fixture.project.join("shared").join(invalid)),
        ("lookalike", fixture.project.join("shared/lint-\u{FFFD}")),
    ] {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: d\n---\nbody\n"),
        )
        .unwrap();
        std::fs::write(directory.join("a.md"), "a").unwrap();
        std::os::unix::fs::symlink(
            &directory,
            fixture.project.join(format!(".bravebot/skills/{name}")),
        )
        .unwrap();
    }

    let workspace = fixture.workspace();
    let landings = |rules: bravebot_core::permissions::Permissions| {
        let mut store = TrustStore::new("/work");
        store.trust(".");
        let catalogue =
            crate::skills::resolved(&workspace, None, store, rules, &mut RecordingSink::new());
        let landed = |name: &str| catalogue.get(name).map(|skill| skill.landed.clone());
        (landed("invalid"), landed("lookalike"))
    };

    let (invalid_landed, lookalike_landed) =
        landings(bravebot_core::permissions::Permissions::default());
    assert_eq!(
        invalid_landed,
        Some(None),
        "a landing that is not text was named as its lookalike"
    );
    assert_eq!(
        lookalike_landed,
        Some(Some("shared/lint-\u{FFFD}".to_string())),
        "the control: a landing that is text and allowed is named"
    );

    let (rules, rejected) = bravebot_core::permissions::Permissions::parse(
        &["Read(shared/lint-\u{FFFD})".to_string()],
        &[],
        &[],
        &bravebot_core::permissions::Anchors::none(),
    );
    assert!(rejected.is_empty(), "the rule did not parse: {rejected:?}");
    let (invalid_landed, lookalike_landed) = landings(rules);
    assert_eq!(
        lookalike_landed, None,
        "a rule about the lookalike did not deny it"
    );
    assert_eq!(
        invalid_landed,
        Some(None),
        "a rule about the lookalike dropped the skill whose landing is not text, or let the \
         landing be named"
    );
}
