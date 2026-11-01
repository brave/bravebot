//! Loading the saved cases of a skill: read through the trust the skill itself is read with, and a
//! file nobody vouched for is counted and never held.

use bravebot_agent::skill_eval::{self, CaseProblem};
use bravebot_agent::skills;
use bravebot_agent::workspace::Workspace;
use bravebot_core::event::RecordingSink;
use bravebot_core::trust::TrustStore;
use std::path::{Path, PathBuf};

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("bravebot-skill-eval-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self { path }
    }

    fn project(&self) -> PathBuf {
        let dir = self.path.join("project");
        std::fs::create_dir_all(&dir).expect("create project");
        dir
    }

    fn home(&self) -> PathBuf {
        let dir = self.path.join("home");
        std::fs::create_dir_all(&dir).expect("create home");
        dir
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

const GOOD: &str =
    r#"{"prompt":"tidy the imports","graders":[{"check":{"tool_called":"edit_file"}}]}"#;

/// A skill under `<root>/skills/<name>` with its case files under `evals/`.
fn skill_with_cases(root: &Path, name: &str, cases: &[(&str, &str)]) {
    let at = root.join("skills").join(name);
    std::fs::create_dir_all(at.join("evals")).expect("create directories");
    std::fs::write(
        at.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: a skill for tests\n---\n\nbody"),
    )
    .expect("write skill");
    for (file, text) in cases {
        std::fs::write(at.join("evals").join(file), text).expect("write case");
    }
}

fn trusting(paths: &[&str], distrusting: &[&str]) -> TrustStore {
    let mut store = TrustStore::new("/work");
    for path in paths {
        store.trust(path);
    }
    for path in distrusting {
        store.distrust(path);
    }
    store
}

fn catalogue(workspace: &Workspace, home: &Path, trust: TrustStore) -> skills::Catalogue {
    skills::resolved(
        workspace,
        Some(home),
        trust,
        Default::default(),
        &mut RecordingSink::new(),
    )
}

fn load(
    catalogue: &skills::Catalogue,
    name: &str,
    workspace: &Workspace,
    trust: &TrustStore,
) -> skill_eval::Loaded {
    skill_eval::load_cases(
        catalogue.get(name).expect("the skill is offered"),
        workspace,
        trust,
        &Default::default(),
        &mut RecordingSink::new(),
    )
}

/// A user's own skill and a vouched project's skill both give up their case files, in name order,
/// and a file that is not a valid case is reported as refused beside the ones that are. Rejects a
/// loader that reads only one place, ignores the `evals` directory, or drops a bad file silently
/// (a case that fails to parse would then leave the author with fewer cases and no word).
#[test]
fn cases_of_a_users_skill_and_of_a_vouched_project_skill_are_loaded() {
    let scratch = Scratch::new("loaded");
    let (home, project) = (scratch.home(), scratch.project());
    skill_with_cases(
        &home,
        "mine",
        &[("b.json", GOOD), ("a.json", GOOD), ("c.json", "not json")],
    );
    skill_with_cases(&project.join(".bravebot"), "ours", &[("one.json", GOOD)]);
    std::fs::write(
        home.join("skills/mine/evals/notes.txt"),
        "not a case, only a note",
    )
    .unwrap();
    let workspace = Workspace::new(&project).expect("workspace");
    let trust = trusting(&["."], &[]);
    let found = catalogue(&workspace, &home, trust.clone());

    let mine = load(&found, "mine", &workspace, &trust);
    let names: Vec<_> = mine.cases.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["evals/a.json", "evals/b.json"]);
    assert_eq!(mine.cases[0].1.prompt, "tidy the imports");
    assert_eq!(mine.refused.len(), 1);
    assert_eq!(mine.refused[0].0, "evals/c.json");
    assert!(matches!(mine.refused[0].1, CaseProblem::Malformed(_)));
    assert_eq!(mine.not_read, 0);

    let ours = load(&found, "ours", &workspace, &trust);
    assert_eq!(ours.cases.len(), 1);
    assert_eq!(ours.cases[0].0, "evals/one.json");
}

/// A case file in a vouched project's skill that the trust map does not vouch for is counted and
/// never read, and the vouched one beside it still loads. Rejects a loader that reads every file in
/// the directory because the skill was trusted, and one that lists the distrusted name.
#[test]
fn a_case_file_nobody_vouched_for_is_counted_and_never_read() {
    let scratch = Scratch::new("distrusted");
    let (home, project) = (scratch.home(), scratch.project());
    skill_with_cases(
        &project.join(".bravebot"),
        "ours",
        &[("good.json", GOOD), ("planted.json", GOOD)],
    );
    let workspace = Workspace::new(&project).expect("workspace");
    let trust = trusting(&["."], &[".bravebot/skills/ours/evals/planted.json"]);
    let found = catalogue(&workspace, &home, trust.clone());

    let loaded = load(&found, "ours", &workspace, &trust);

    let names: Vec<_> = loaded.cases.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["evals/good.json"]);
    assert!(loaded.refused.is_empty());
    assert_eq!(loaded.not_read, 1, "the distrusted file was not counted");
}

/// A project nobody vouched for offers no skill at all, so there is nothing to load cases from and
/// the skill is not found by name. Rejects an evaluation that reads a project's cases on the
/// strength of the directory existing.
#[test]
fn a_project_nobody_vouched_for_offers_no_skill_to_evaluate() {
    let scratch = Scratch::new("untrusted-project");
    let (home, project) = (scratch.home(), scratch.project());
    skill_with_cases(&project.join(".bravebot"), "ours", &[("one.json", GOOD)]);
    let workspace = Workspace::new(&project).expect("workspace");
    let found = catalogue(&workspace, &home, trusting(&[], &[]));

    assert!(found.get("ours").is_none());
}

/// A name that is not among the files the catalogue listed reaches nothing, whatever the name
/// spells: a path out of the skill's directory is not read. Rejects a read that joins the name onto
/// the directory without checking it against the listing.
#[test]
fn a_name_the_catalogue_did_not_list_is_not_read() {
    let scratch = Scratch::new("unlisted");
    let (home, project) = (scratch.home(), scratch.project());
    skill_with_cases(&home, "mine", &[("a.json", GOOD)]);
    std::fs::write(home.join("secret.json"), GOOD).unwrap();
    let workspace = Workspace::new(&project).expect("workspace");
    let trust = trusting(&["."], &[]);
    let found = catalogue(&workspace, &home, trust.clone());
    let skill = found.get("mine").expect("offered");

    for name in ["../../secret.json", "evals/missing.json", "SKILL.md"] {
        assert!(
            skills::read_beside(
                skill,
                name,
                &workspace,
                trust.clone(),
                Default::default(),
                &mut RecordingSink::new()
            )
            .is_none(),
            "{name} was read"
        );
    }
}
