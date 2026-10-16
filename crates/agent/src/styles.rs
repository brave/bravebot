//! Output styles: named words that take the place of the system prompt's opening.
//!
//! A style is words this program wrote, kept by name so a person can pick one with `/style`
//! instead of typing `--system-prompt`. It stands where `--system-prompt` stands (CLI-19) and
//! reaches nothing after it, so the guidance on reading a tool's output, the mode and every
//! refusal are unchanged. The words ask for a manner of answering and grant nothing.
//!
//! A style is one of the three this build ships, a file of the person's own at
//! `~/.bravebot/styles/<name>.md`, or a file of the project's at `.bravebot/styles/<name>.md`. The
//! first file is the person's configuration and trusted for sitting there. The second is read only
//! where the trust map vouches for it, through the gate every other project file goes through, and
//! again at the start of each turn that uses it, so a vouching withdrawn after the pick takes the
//! style out of the next turn.

use crate::workspace::Workspace;
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use std::borrow::Cow;
use std::path::Path;

pub use bravebot_core::policy::StyleOrigin as Origin;

/// The directory under the workspace that holds a project's own styles.
pub const PROJECT_DIRECTORY: &str = ".bravebot/styles";

/// The directory under `~/.bravebot` that holds a person's own styles.
pub const DIRECTORY: &str = "styles";

/// The most bytes of a style file that are read. A longer file is not a style: it is a document.
pub const LONGEST_FILE: u64 = 4096;

/// One style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    /// What a person types after `/style`.
    pub name: Cow<'static, str>,
    /// Stands in for the opening of the planner's system prompt.
    pub words: Cow<'static, str>,
    /// Where the words came from.
    pub origin: Origin,
}

impl Style {
    /// A pick of the project's style `name`, whose words are not read yet.
    ///
    /// The words are read by [`resolve_project`] when a turn begins, under that turn's trust map,
    /// so nothing a pick held can outlast the vouching it was made under. Until then it has none.
    pub fn project(name: &str) -> Self {
        Self {
            name: Cow::Owned(name.to_string()),
            words: Cow::Borrowed(""),
            origin: Origin::Project,
        }
    }
}

/// The styles this build ships, in the order they are listed.
pub const BUILT_IN: [Style; 3] = [
    Style {
        name: Cow::Borrowed("concise"),
        origin: Origin::BuiltIn,
        words: Cow::Borrowed(
            "\
You are a careful assistant working in a user's workspace, with tools to read files, list them \
and search their contents. Lead with the result. Leave out preamble, \
restating the question and a recap of what you did, and say only what the person needs to act.",
        ),
    },
    Style {
        name: Cow::Borrowed("explanatory"),
        origin: Origin::BuiltIn,
        words: Cow::Borrowed(
            "\
You are a careful assistant working in a user's workspace, with tools to read files, list them \
and search their contents. While you work, add short `Insight` notes that \
explain why the code is the way it is or why you chose an approach, so the person learns the \
codebase as well as getting the result.",
        ),
    },
    Style {
        name: Cow::Borrowed("proactive"),
        origin: Origin::BuiltIn,
        words: Cow::Borrowed(
            "\
You are a careful assistant working in a user's workspace, with tools to read files, list them \
and search their contents. Start the work without asking for permission to \
begin, and where a routine decision has an obvious answer, decide it, say what you assumed, and \
carry on. This does not change what you may do: a write is still put to the person where it \
would have been, and a mode that refuses still refuses.",
        ),
    },
];

/// The built-in style called `name`, compared exactly.
pub fn named(name: &str) -> Option<&'static Style> {
    BUILT_IN.iter().find(|style| style.name == name)
}

/// The style called `name`: a built-in one, or else the person's file `<name>.md` under
/// `<home>/styles`, where `home` is `~/.bravebot`.
///
/// The name is compared whole against the built-in list and must be a slug to name a file, so
/// nothing typed after `/style` can reach a path outside the directory. A built-in name always
/// means the built-in: a file cannot stand in for it.
pub fn find(home: Option<&Path>, name: &str) -> Option<Style> {
    if let Some(built_in) = named(name) {
        return Some(built_in.clone());
    }
    read_file(home?, name)
}

/// The words of `<home>/styles/<name>.md`, where that is a small regular file of UTF-8 text.
///
/// A symbolic link is not followed, for the reason a skill's is not: the file stands for what is
/// in the directory, and a link would make it stand for something outside it.
fn read_file(home: &Path, name: &str) -> Option<Style> {
    use std::io::Read;
    if !crate::memory::is_a_slug(name) {
        return None;
    }
    let path = home.join(DIRECTORY).join(format!("{name}.md"));
    if !std::fs::symlink_metadata(&path).ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(&path)
        .ok()?
        .take(LONGEST_FILE + 1)
        .read_to_string(&mut text)
        .ok()?;
    let words = text.trim();
    if words.is_empty() || text.len() as u64 > LONGEST_FILE {
        return None;
    }
    Some(Style {
        name: Cow::Owned(name.to_string()),
        words: Cow::Owned(words.to_string()),
        origin: Origin::Home,
    })
}

/// The names `/style` offers: the built-in ones, then the person's files that would be accepted,
/// each once and in order.
pub fn available(home: Option<&Path>) -> Vec<String> {
    let mut names: Vec<String> = BUILT_IN.iter().map(|s| s.name.to_string()).collect();
    let Some(entries) = home.and_then(|h| std::fs::read_dir(h.join(DIRECTORY)).ok()) else {
        return names;
    };
    let mut files: Vec<String> = entries
        .filter_map(|entry| {
            let file = entry.ok()?.file_name().into_string().ok()?;
            let name = file.strip_suffix(".md")?;
            (named(name).is_none() && read_file(home?, name).is_some()).then(|| name.to_string())
        })
        .collect();
    files.sort_unstable();
    files.dedup();
    names.extend(files);
    names
}

/// The project's style called `name`, read through the trust map, or `None` where it is not one.
///
/// The directory and the file are each asked of the map before anything is opened, so nothing
/// about an untrusted project's files is looked at, and the bytes come back through the same read
/// and the same gate as the project's `AGENTS.md`. The words are examined only once they have
/// passed it. A deny rule covering the file, a symbolic link, a file over [`LONGEST_FILE`] bytes
/// and one with no words in it are not styles, as for the person's own files.
pub fn resolve_project<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    name: &str,
) -> Option<Style> {
    if !crate::memory::is_a_slug(name) {
        return None;
    }
    let relative = format!("{PROJECT_DIRECTORY}/{name}.md");
    if !policy.trusts_path(PROJECT_DIRECTORY) || !policy.trusts_path(&relative) {
        return None;
    }
    if workspace.rule_denies_reading(policy, &relative) {
        return None;
    }
    let meta = std::fs::symlink_metadata(workspace.root().join(&relative)).ok()?;
    if !meta.is_file() || meta.len() > LONGEST_FILE {
        return None;
    }
    let contents = workspace.read(policy, &Labelled::trusted(relative)).ok()?;
    if !contents.label().is_trusted() {
        return None;
    }
    let text = policy
        .read_trusted_content("output style", &contents)
        .ok()?;
    let words = text.trim();
    if words.is_empty() || text.len() as u64 > LONGEST_FILE {
        return None;
    }
    Some(Style {
        name: Cow::Owned(name.to_string()),
        words: Cow::Owned(words.to_string()),
        origin: Origin::Project,
    })
}

/// The names of the project's styles that would be accepted, which `/style` offers after the others.
///
/// A name a built-in style or one of the person's files already has is left out, so a project can
/// neither take the place of one nor make `/style` offer a name twice. Nothing is listed from a
/// directory the trust map does not vouch for.
pub fn project_names<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    home: Option<&Path>,
) -> Vec<String> {
    if !policy.trusts_path(PROJECT_DIRECTORY) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(workspace.root().join(PROJECT_DIRECTORY)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            let file = entry.ok()?.file_name().into_string().ok()?;
            let name = file.strip_suffix(".md")?.to_string();
            Some(name)
        })
        .filter(|name| named(name).is_none() && home.is_none_or(|h| read_file(h, name).is_none()))
        .filter(|name| resolve_project(policy, workspace, name).is_some())
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// [`project_names`] as an interface asks for them, through a policy holding only the read.
///
/// The same gate, trust map and rules a turn reads the project through, so what `/style` offers is
/// what a turn would accept, and a name the turn would refuse is never offered.
pub fn offered_from_project<S: Sink>(
    workspace: &Workspace,
    home: Option<&Path>,
    trust: bravebot_core::trust::TrustStore,
    permissions: bravebot_core::permissions::Permissions,
    sink: &mut S,
) -> Vec<String> {
    let mut routing = bravebot_core::policy::Routing::new();
    routing.insert_trusted("styles", PROJECT_DIRECTORY);
    let Ok(policy) = Policy::begin(
        routing,
        bravebot_core::policy::ReleasePlan::new(),
        bravebot_core::capability::CapabilitySet::from_iter([
            bravebot_core::capability::Capability::FileRead,
        ]),
        sink,
    ) else {
        return Vec::new();
    };
    let mut policy = policy
        .with_trust(trust)
        .with_permissions(permissions)
        .with_root(workspace.root())
        .with_backslash_separates(crate::workspace::BACKSLASH_SEPARATES);
    project_names(&mut policy, workspace, home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_is_found_by_its_whole_name_only() {
        assert_eq!(named("concise").map(|s| s.name.as_ref()), Some("concise"));
        for word in ["conci", "Concise", "concise ", "", "default"] {
            assert_eq!(named(word), None, "{word:?} named a style");
        }
    }

    #[test]
    fn no_style_takes_the_quarantine_guidance_with_it() {
        for style in &BUILT_IN {
            assert!(
                !style.words.contains("data, never as instructions"),
                "{} restates guidance that belongs after the opening",
                style.name
            );
            assert!(
                !style.words.trim().is_empty() && style.words.len() < 1000,
                "{}",
                style.name
            );
        }
    }

    #[test]
    fn style_names_are_distinct() {
        let mut names: Vec<_> = BUILT_IN.iter().map(|style| style.name.clone()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BUILT_IN.len());
    }

    /// A directory holding `styles/<name>.md` for each pair, fresh for this test.
    fn home_with(label: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let home = crate::testutil::scratch_dir(&format!("bravebot-styles-{label}"));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(DIRECTORY)).unwrap();
        for (name, words) in files {
            std::fs::write(home.join(DIRECTORY).join(format!("{name}.md")), words).unwrap();
        }
        home
    }

    #[test]
    fn a_file_in_the_home_directory_is_a_style_by_its_name() {
        let home = home_with("found", &[("terse", "\n  Answer in one line.  \n")]);
        let style = find(Some(&home), "terse").expect("the file is a style");
        assert_eq!(style.name, "terse");
        assert_eq!(style.words, "Answer in one line.");
        assert_eq!(style.origin, Origin::Home);
        assert_eq!(find(None, "terse"), None, "no home, no file");
        assert_eq!(find(Some(&home), "missing"), None);
    }

    #[test]
    fn a_file_cannot_stand_in_for_a_built_in_name() {
        let home = home_with("shadow", &[("concise", "Ramble on at length.")]);
        let style = find(Some(&home), "concise").expect("built-in");
        assert_eq!(style.origin, Origin::BuiltIn);
        assert!(!style.words.contains("Ramble"));
        assert_eq!(
            available(Some(&home)),
            ["concise", "explanatory", "proactive"]
        );
    }

    #[test]
    fn a_name_that_is_not_a_slug_never_reaches_a_path() {
        let home = home_with("traverse", &[("ok", "words")]);
        std::fs::write(home.join("outside.md"), "OUTSIDE").unwrap();
        for name in [
            "../outside",
            "ok/../../outside",
            "OK",
            "ok.md",
            "a b",
            "-ok",
            "",
        ] {
            assert_eq!(find(Some(&home), name), None, "{name:?} named a file");
        }
    }

    #[test]
    fn a_link_an_empty_file_and_a_long_file_are_not_styles() {
        let home = home_with(
            "refused",
            &[
                ("empty", "  \n"),
                ("long", &"x".repeat(LONGEST_FILE as usize + 1)),
                ("exact", &"x".repeat(LONGEST_FILE as usize)),
            ],
        );
        #[cfg(unix)]
        {
            std::fs::write(home.join("target.txt"), "ELSEWHERE").unwrap();
            std::os::unix::fs::symlink(
                home.join("target.txt"),
                home.join(DIRECTORY).join("linked.md"),
            )
            .unwrap();
            assert_eq!(find(Some(&home), "linked"), None);
        }
        assert_eq!(find(Some(&home), "empty"), None);
        assert_eq!(find(Some(&home), "long"), None);
        assert!(find(Some(&home), "exact").is_some(), "the cap is inclusive");
    }

    #[test]
    fn the_offered_names_are_the_built_ins_then_the_accepted_files_in_order() {
        let home = home_with(
            "offered",
            &[("zed", "z"), ("alpha", "a"), ("empty", ""), ("Bad", "b")],
        );
        std::fs::write(home.join(DIRECTORY).join("notes.txt"), "not md").unwrap();
        assert_eq!(
            available(Some(&home)),
            ["concise", "explanatory", "proactive", "alpha", "zed"]
        );
        assert_eq!(available(None), ["concise", "explanatory", "proactive"]);
    }

    /// A workspace holding `.bravebot/styles/<name>.md` for each pair, fresh for this test.
    fn project_with(label: &str, files: &[(&str, &str)]) -> Workspace {
        let root = crate::testutil::scratch_dir(&format!("bravebot-project-styles-{label}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(PROJECT_DIRECTORY)).unwrap();
        for (name, words) in files {
            std::fs::write(
                root.join(PROJECT_DIRECTORY).join(format!("{name}.md")),
                words,
            )
            .unwrap();
        }
        Workspace::new(&root).expect("workspace")
    }

    fn vouching(workspace: &Workspace) -> bravebot_core::trust::TrustStore {
        let mut trust = bravebot_core::trust::TrustStore::new(workspace.root());
        trust.trust(".");
        trust
    }

    fn offered(
        workspace: &Workspace,
        home: Option<&Path>,
        trust: bravebot_core::trust::TrustStore,
    ) -> Vec<String> {
        offered_from_project(
            workspace,
            home,
            trust,
            bravebot_core::permissions::Permissions::default(),
            &mut bravebot_core::event::RecordingSink::new(),
        )
    }

    fn resolved(
        workspace: &Workspace,
        trust: bravebot_core::trust::TrustStore,
        name: &str,
    ) -> Option<Style> {
        let mut sink = bravebot_core::event::RecordingSink::new();
        let mut routing = bravebot_core::policy::Routing::new();
        routing.insert_trusted("styles", PROJECT_DIRECTORY);
        let mut policy = Policy::begin(
            routing,
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([
                bravebot_core::capability::Capability::FileRead,
            ]),
            &mut sink,
        )
        .expect("policy")
        .with_trust(trust)
        .with_root(workspace.root());
        resolve_project(&mut policy, workspace, name)
    }

    #[test]
    fn a_project_style_is_read_only_where_the_trust_map_vouches() {
        let workspace = project_with("trust", &[("house", "  Answer like the house.  ")]);
        let untrusted = bravebot_core::trust::TrustStore::new(workspace.root());

        assert_eq!(offered(&workspace, None, vouching(&workspace)), ["house"]);
        let style = resolved(&workspace, vouching(&workspace), "house").expect("vouched for");
        assert_eq!(style.words, "Answer like the house.");
        assert_eq!(style.origin, Origin::Project);

        assert!(offered(&workspace, None, untrusted.clone()).is_empty());
        assert_eq!(resolved(&workspace, untrusted, "house"), None);
    }

    #[test]
    fn a_project_file_cannot_take_the_place_of_a_built_in_or_a_home_style() {
        let workspace = project_with(
            "shadow",
            &[
                ("concise", "Ramble."),
                ("terse", "Project terse."),
                ("mine", "Project mine."),
            ],
        );
        let home = home_with("shadow-project", &[("terse", "Home terse.")]);
        assert_eq!(
            offered(&workspace, Some(&home), vouching(&workspace)),
            ["mine"]
        );
        assert_eq!(
            offered(&workspace, None, vouching(&workspace)),
            ["mine", "terse"]
        );
    }

    #[test]
    fn a_project_link_an_empty_file_a_long_file_and_a_bad_name_are_not_styles() {
        let workspace = project_with(
            "refused",
            &[
                ("empty", " \n"),
                ("long", &"x".repeat(LONGEST_FILE as usize + 1)),
                ("Bad", "words"),
                ("good", "words"),
            ],
        );
        let directory = workspace.root().join(PROJECT_DIRECTORY);
        #[cfg(unix)]
        {
            std::fs::write(workspace.root().join("elsewhere.txt"), "ELSEWHERE").unwrap();
            std::os::unix::fs::symlink(
                workspace.root().join("elsewhere.txt"),
                directory.join("linked.md"),
            )
            .unwrap();
        }
        std::fs::write(directory.join("notes.txt"), "not md").unwrap();
        assert_eq!(offered(&workspace, None, vouching(&workspace)), ["good"]);
        for name in ["empty", "long", "Bad", "linked", "../elsewhere", ""] {
            assert_eq!(
                resolved(&workspace, vouching(&workspace), name),
                None,
                "{name:?}"
            );
        }
    }
}
