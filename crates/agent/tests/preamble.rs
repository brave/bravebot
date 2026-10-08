//! Where standing instructions are read from, and in what order.
//!
//! No test here touches `HOME`. The home root is an argument, which is what keeps the rest of the
//! suite from depending on whatever the developer happens to have installed.

use bravebot_agent::preamble;
use bravebot_agent::skills::Catalogue;
use bravebot_agent::workspace::Workspace;
use bravebot_config::Attribution;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::trust::TrustStore;
use std::path::PathBuf;

/// A scratch directory that removes itself, so tests do not leave state behind.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("bravebot-preamble-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self { path }
    }

    fn directory(&self, name: &str) -> PathBuf {
        let dir = self.path.join(name);
        std::fs::create_dir_all(&dir).expect("create directory");
        dir
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn routing() -> Routing {
    let mut r = Routing::new();
    r.insert_trusted("task", "do the work");
    r
}

fn policy<'s>(sink: &'s mut RecordingSink, trusted: &[&str]) -> Policy<'s, RecordingSink> {
    let mut store = TrustStore::new("/work");
    for path in trusted {
        store.trust(path);
    }
    Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead, Capability::FileWrite]),
        sink,
    )
    .expect("policy")
    .with_trust(store)
}

/// Both files are read, so the question is which one the planner reads last. The project is the
/// more specific of the two, and a convention stated there is the one that has to win when it
/// disagrees with a habit the user carries between projects.
#[test]
fn the_home_agents_file_is_read_before_the_project_one() {
    let scratch = Scratch::new("ordering");
    let home = scratch.directory("home");
    let project = scratch.directory("project");
    std::fs::write(home.join("AGENTS.md"), "GLOBAL-CONVENTION").unwrap();
    std::fs::write(project.join("AGENTS.md"), "PROJECT-CONVENTION").unwrap();
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            Some(&home),
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };

    let global = preamble
        .text
        .find("GLOBAL-CONVENTION")
        .expect("the home file was not read at all");
    let project = preamble
        .text
        .find("PROJECT-CONVENTION")
        .expect("the project file was not read at all");
    assert!(
        global < project,
        "the project file did not have the last word: {}",
        preamble.text
    );
}

/// A directory opened with `/add-dir` is somewhere to read files from, not a second project. Its
/// conventions are not the ones this work is being done under, and treating them as standing
/// instructions would let opening a directory silently change how every later turn behaves.
#[test]
fn an_added_directory_contributes_no_standing_instructions() {
    let scratch = Scratch::new("added");
    let project = scratch.directory("project");
    let other = scratch.directory("other");
    std::fs::write(other.join("AGENTS.md"), "ADDED-CONVENTION").unwrap();
    let mut workspace = Workspace::new(&project).expect("workspace");
    workspace
        .add_directory(other.to_str().expect("path is utf-8"))
        .expect("add the directory");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };

    assert!(
        !preamble.text.contains("ADDED-CONVENTION"),
        "an added directory's AGENTS.md became a standing instruction: {}",
        preamble.text
    );
}

/// Writing an AGENTS.md mid-session works, and so does having the agent write one. Reading the
/// sources once at startup would mean the file that was just written is the one instruction the
/// planner cannot see.
#[test]
fn a_file_written_after_one_turn_is_read_by_the_next() {
    let scratch = Scratch::new("afresh");
    let project = scratch.directory("project");
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let before = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };
    assert!(
        !before.text.contains("LATE-CONVENTION"),
        "the file was found before it existed"
    );

    std::fs::write(project.join("AGENTS.md"), "LATE-CONVENTION").unwrap();

    let after = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };
    assert!(
        after.text.contains("LATE-CONVENTION"),
        "a file written between turns was never picked up: {}",
        after.text
    );
}

/// A project that wrote its conventions down should not have them ignored over the spelling.
#[test]
fn the_project_file_may_be_named_claude_md() {
    for (name, subdirectory) in [("CLAUDE.md", None), ("CLAUDE.md", Some(".claude"))] {
        let scratch = Scratch::new(&format!("claude-md-{}", subdirectory.unwrap_or("root")));
        let project = scratch.directory("project");
        let holder = match subdirectory {
            Some(sub) => {
                let path = project.join(sub);
                std::fs::create_dir_all(&path).unwrap();
                path
            }
            None => project.clone(),
        };
        std::fs::write(holder.join(name), "PROJECT-CONVENTION").unwrap();
        let workspace = Workspace::new(&project).expect("workspace");

        let mut sink = RecordingSink::new();
        let preamble = {
            let mut policy = policy(&mut sink, &["."]);
            preamble::compose(
                &mut policy,
                &workspace,
                None,
                &Catalogue::default(),
                None,
                None,
                &Attribution::default(),
            )
        };

        assert!(
            preamble.text.contains("PROJECT-CONVENTION"),
            "{name} under {subdirectory:?} was not read: {}",
            preamble.text
        );
    }
}

/// One set of instructions under two names is still one set. Reading both would state
/// everything twice, and the planner pays for the whole system prompt on every request.
#[test]
fn only_the_first_project_file_that_exists_is_read() {
    let scratch = Scratch::new("first-wins");
    let project = scratch.directory("project");
    std::fs::write(project.join("AGENTS.md"), "THE-REAL-ONE").unwrap();
    std::fs::write(project.join("CLAUDE.md"), "THE-OTHER-ONE").unwrap();
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };

    assert!(preamble.text.contains("THE-REAL-ONE"));
    assert!(
        !preamble.text.contains("THE-OTHER-ONE"),
        "both files were read: {}",
        preamble.text
    );
}

/// The shape that cost a real turn a whole round trip: `AGENTS.md` holding one sentence naming
/// the document the project actually keeps its conventions in.
#[test]
fn a_project_file_that_only_names_another_is_followed() {
    let scratch = Scratch::new("pointer");
    let project = scratch.directory("project");
    std::fs::create_dir_all(project.join(".claude")).unwrap();
    std::fs::write(
        project.join("AGENTS.md"),
        "Refer to canonical agent instructions in `.claude/CLAUDE.md`.",
    )
    .unwrap();
    std::fs::write(project.join(".claude/CLAUDE.md"), "THE-REAL-CONVENTIONS").unwrap();
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };

    assert!(
        preamble.text.contains("THE-REAL-CONVENTIONS"),
        "the pointer was not followed: {}",
        preamble.text
    );
    // The header names where the instructions came from, not where the pointer was.
    assert!(
        preamble.text.contains("From .claude/CLAUDE.md:"),
        "the source was misnamed: {}",
        preamble.text
    );
}

/// The test that keeps the rule above from eating instructions. A real document cites other
/// files all the time, and swapping the conventions for whatever they mentioned first would be
/// far worse than the round trip this saves.
#[test]
fn a_project_file_that_merely_cites_another_is_read_as_itself() {
    let scratch = Scratch::new("not-a-pointer");
    let project = scratch.directory("project");
    let long = format!(
        "THE-REAL-CONVENTIONS\n\nSee also docs/style.md.\n\n{}",
        "Write a test for everything you change. ".repeat(20)
    );
    std::fs::write(project.join("AGENTS.md"), &long).unwrap();
    std::fs::write(project.join("docs-style-decoy.md"), "DECOY").unwrap();
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };

    assert!(preamble.text.contains("THE-REAL-CONVENTIONS"));
    assert!(!preamble.text.contains("DECOY"));
}

/// A pointer naming something the workspace does not hold changes nothing: the file that was
/// found is still the instructions, and confinement is what refuses the rest.
#[test]
fn a_pointer_that_names_nothing_readable_leaves_the_file_standing() {
    let scratch = Scratch::new("pointer-dangling");
    let project = scratch.directory("project");
    std::fs::write(
        project.join("AGENTS.md"),
        "See ../../../etc/passwd.md for the conventions.",
    )
    .unwrap();
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };

    assert!(
        preamble.text.contains("See ../../../etc/passwd.md"),
        "the file that was actually found did not reach the planner: {}",
        preamble.text
    );
}

/// What `compose` produces for a project holding `files`, with the given paths trusted.
fn composed(name: &str, files: &[(&str, &str)], trusted: &[&str]) -> preamble::Preamble {
    let scratch = Scratch::new(name);
    let project = scratch.directory("project");
    for (path, contents) in files {
        let path = project.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink, trusted);
    preamble::compose(
        &mut policy,
        &workspace,
        None,
        &Catalogue::default(),
        None,
        None,
        &Attribution::default(),
    )
}

/// A long document that splits its conventions across files: the import is replaced where it
/// stands, and a nested import is resolved against the file it is written in, not the project root.
#[test]
fn an_at_path_import_is_expanded_in_place_and_resolved_beside_its_file() {
    let preamble = composed(
        "import-expanded",
        &[
            (
                "AGENTS.md",
                "BEFORE\n\nStyle: @docs/style.md.\n\nAFTER\n\n@docs/other.md\n",
            ),
            ("docs/style.md", "STYLE-RULES @nested.md"),
            ("docs/nested.md", "NESTED-RULES"),
            ("docs/other.md", "OTHER-RULES"),
            ("nested.md", "ROOT-DECOY"),
        ],
        &["."],
    );

    assert!(
        preamble
            .text
            .contains("BEFORE\n\nStyle: STYLE-RULES NESTED-RULES.\n\nAFTER\n\nOTHER-RULES"),
        "the imports were not expanded where they stand: {}",
        preamble.text
    );
    assert!(!preamble.text.contains("ROOT-DECOY"));
    assert!(!preamble.text.contains("@docs/style.md"));
    assert!(preamble.notices.is_empty(), "{:?}", preamble.notices);
}

/// Four hops are followed. The fifth is left as written, and the person is told which file it was.
#[test]
fn an_import_nested_past_four_hops_is_left_as_written() {
    let preamble = composed(
        "import-depth",
        &[
            ("AGENTS.md", "ROOT @l1.md"),
            ("l1.md", "ONE @l2.md"),
            ("l2.md", "TWO @l3.md"),
            ("l3.md", "THREE @l4.md"),
            ("l4.md", "FOUR @l5.md"),
            ("l5.md", "FIVE-MUST-NOT-LOAD"),
        ],
        &["."],
    );

    assert!(
        preamble.text.contains("ROOT ONE TWO THREE FOUR @l5.md"),
        "{}",
        preamble.text
    );
    assert!(!preamble.text.contains("FIVE-MUST-NOT-LOAD"));
    assert_eq!(preamble.notices.len(), 1, "{:?}", preamble.notices);
    assert!(preamble.notices[0].message.contains("l5.md"));
}

/// The 64th import is followed and the 65th is not, however shallow they are. The ones past the
/// count stay as written and the person is told which they were.
#[test]
fn imports_past_the_count_are_left_as_written() {
    let names: Vec<String> = (0..66).map(|n| format!("part-{n:02}.md")).collect();
    let bodies: Vec<String> = (0..66).map(|n| format!("BODY-{n:02}")).collect();
    let root = names
        .iter()
        .map(|name| format!("@{name}"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut files: Vec<(&str, &str)> = vec![("AGENTS.md", root.as_str())];
    files.extend(
        names
            .iter()
            .zip(&bodies)
            .map(|(name, body)| (name.as_str(), body.as_str())),
    );

    let preamble = composed("import-count", &files, &["."]);

    let followed = bodies[..64].join(" ");
    assert!(
        preamble
            .text
            .contains(&format!("{followed} @part-64.md @part-65.md")),
        "{}",
        preamble.text
    );
    assert!(!preamble.text.contains("BODY-64"));
    assert!(!preamble.text.contains("BODY-65"));
    assert_eq!(preamble.notices.len(), 2, "{:?}", preamble.notices);
    assert!(preamble.notices[0].message.contains("part-64.md"));
    assert!(preamble.notices[1].message.contains("part-65.md"));
}

/// A path in a fence or in backticks is documentation about an import, not one.
#[test]
fn an_import_in_a_code_fence_or_a_code_span_is_not_followed() {
    let preamble = composed(
        "import-code",
        &[
            (
                "AGENTS.md",
                "Write `@docs/a.md` or ` @docs/a.md ` to import.\n\n```\n@docs/a.md\n```\n\n~~~\n@docs/a.md\n~~~\n",
            ),
            ("docs/a.md", "MUST-NOT-LOAD"),
        ],
        &["."],
    );

    assert!(
        !preamble.text.contains("MUST-NOT-LOAD"),
        "{}",
        preamble.text
    );
    assert_eq!(preamble.text.matches("@docs/a.md").count(), 4);
}

/// A fence closes on a run at least as long as the one that opened it, so a shorter run inside is
/// part of the block, and the import after it is still documentation.
#[test]
fn a_shorter_fence_inside_a_longer_one_does_not_end_it() {
    let preamble = composed(
        "import-long-fence",
        &[
            (
                "AGENTS.md",
                "````\n```\n@docs/a.md\n````\n\n``Write `@docs/b.md` here``\n",
            ),
            ("docs/a.md", "A-MUST-NOT-LOAD"),
            ("docs/b.md", "B-MUST-NOT-LOAD"),
        ],
        &["."],
    );

    assert!(
        !preamble.text.contains("MUST-NOT-LOAD"),
        "{}",
        preamble.text
    );
}

/// A short file naming another is followed, and when that file imports the first one back the
/// cycle is cut there rather than the pointer sentence being inlined into it.
#[test]
fn an_import_back_to_the_file_a_pointer_was_read_from_is_a_cycle() {
    let preamble = composed(
        "import-pointer-cycle",
        &[
            ("AGENTS.md", "Refer to `CLAUDE.md`."),
            ("CLAUDE.md", "REAL @AGENTS.md"),
        ],
        &["."],
    );

    assert!(
        preamble.text.contains("REAL @AGENTS.md"),
        "{}",
        preamble.text
    );
    assert!(!preamble.text.contains("REAL Refer to"));
    assert_eq!(preamble.notices.len(), 1, "{:?}", preamble.notices);
}

/// Two files importing each other end at the second visit rather than at the depth limit, and the
/// file that would have repeated is left as written.
#[test]
fn an_import_cycle_is_cut_where_it_closes() {
    let preamble = composed(
        "import-cycle",
        &[
            ("AGENTS.md", "ROOT @a.md"),
            ("a.md", "A @b.md"),
            ("b.md", "B @a.md"),
        ],
        &["."],
    );

    assert!(
        preamble.text.contains("ROOT A B @a.md"),
        "{}",
        preamble.text
    );
    assert_eq!(preamble.notices.len(), 1, "{:?}", preamble.notices);
    assert!(preamble.notices[0].message.contains("a.md"));
}

/// An import above the project root is refused, however the path is spelt.
#[test]
fn an_import_outside_the_workspace_is_left_as_written() {
    let preamble = composed(
        "import-outside",
        &[
            ("AGENTS.md", "ROOT @../outside.md @/etc/passwd.md"),
            ("../outside.md", "OUTSIDE-MUST-NOT-LOAD"),
        ],
        &["."],
    );

    assert!(
        preamble
            .text
            .contains("ROOT @../outside.md @/etc/passwd.md"),
        "{}",
        preamble.text
    );
    assert!(!preamble.text.contains("OUTSIDE-MUST-NOT-LOAD"));
}

/// A directory nobody vouched for loads no instructions, so there is nothing to parse an import
/// out of and nothing the import could read.
#[test]
fn an_untrusted_project_loads_no_import() {
    let preamble = composed(
        "import-untrusted",
        &[
            ("AGENTS.md", "ROOT @docs/a.md"),
            ("docs/a.md", "MUST-NOT-LOAD"),
        ],
        &[],
    );

    assert!(
        !preamble.text.contains("MUST-NOT-LOAD"),
        "{}",
        preamble.text
    );
    assert!(!preamble.text.contains("ROOT"), "{}", preamble.text);
}

/// The whole reason the block exists. Without it a planner has to run `pwd` to learn where it is,
/// which costs a prompt to approve the run and a second one to be shown the answer, because a
/// command's output comes back quarantined.
#[test]
fn the_working_directory_is_stated_so_nothing_has_to_run_pwd() {
    let scratch = Scratch::new("cwd");
    let project = scratch.directory("project");
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };

    assert!(
        preamble
            .text
            .contains(&workspace.root().display().to_string()),
        "the working directory is not in the preamble: {}",
        preamble.text
    );
}

/// A project with no AGENTS.md and no skills still gets the block. These are facts about the
/// machine rather than anything read out of the tree, so there is nothing for an empty project to
/// be missing.
///
/// Every fact the clause names, since the two it did not name were the two that went missing: the
/// OS version was dropped on every non-unix build and nothing failed.
#[test]
fn the_environment_is_stated_even_with_no_instructions_to_read() {
    let scratch = Scratch::new("bare");
    let project = scratch.directory("project");
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };

    for fact in [
        "Working directory:",
        "Is a git repository:",
        "GitHub CLI (gh) on PATH:",
        "Platform:",
        "OS version:",
        "Shell:",
        "Today's date:",
    ] {
        assert!(
            preamble.text.contains(fact),
            "`{fact}` is missing from a preamble with no instructions: {}",
            preamble.text
        );
    }
}

/// A delegate's prompt carries this block word for word, and a delegate is offered no `fetch_url`
/// and, without `ShellExec`, no `run`. So what routes between those two is kept out of the block and
/// handed over separately, on the same probe: a machine whose `$PATH` holds the CLI has both, and one
/// without it has neither.
///
/// Both halves read the machine the code reads, so where `gh` is not installed there is nothing for
/// either to say and this observes nothing. What holds on any host is in the unit tests, over the
/// paragraph and over the fact line with the probe's answer passed in.
#[test]
fn the_road_a_github_url_takes_is_not_in_the_block_a_delegate_reads() {
    let scratch = Scratch::new("github-road-preamble");
    let project = scratch.directory("project");
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };

    assert!(
        !preamble.text.contains("gh pr view"),
        "a delegate was told to run a command it was not offered: {}",
        preamble.text
    );
    assert_eq!(
        preamble.text.contains("GitHub CLI (gh) on PATH: true"),
        preamble.for_a_person.contains("gh pr view"),
        "the fact and what it is for disagree about this machine: {} / {}",
        preamble.text,
        preamble.for_a_person
    );
}

/// Said from the tree rather than assumed. A planner told a checkout is not a repository would
/// avoid git in one that is, and the answer has to come from what is on disk.
#[test]
fn whether_the_tree_is_a_git_repository_is_said_either_way() {
    let scratch = Scratch::new("git");
    let plain = scratch.directory("plain");
    let checkout = scratch.directory("checkout");
    std::fs::create_dir_all(checkout.join(".git")).unwrap();

    let mut sink = RecordingSink::new();
    let described = |workspace: &Workspace, sink: &mut RecordingSink| {
        let mut policy = policy(sink, &["."]);
        preamble::compose(
            &mut policy,
            workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
        .text
    };

    let in_checkout = described(&Workspace::new(&checkout).expect("workspace"), &mut sink);
    assert!(
        in_checkout.contains("Is a git repository: true"),
        "a checkout was not reported as one: {in_checkout}"
    );

    let in_plain = described(&Workspace::new(&plain).expect("workspace"), &mut sink);
    assert!(
        in_plain.contains("Is a git repository: false"),
        "a directory with no .git was reported as a repository: {in_plain}"
    );
}

/// `/cd` moves the working directory, and the block is composed per turn, so the next turn states
/// where the session went. A value read once at startup would go stale the moment it moved.
#[test]
fn moving_the_working_directory_restates_it() {
    let scratch = Scratch::new("moved");
    let first = scratch.directory("first");
    let second = scratch.directory("second");

    let mut sink = RecordingSink::new();
    let described = |workspace: &Workspace, sink: &mut RecordingSink| {
        let mut policy = policy(sink, &["."]);
        preamble::compose(
            &mut policy,
            workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
        .text
    };

    let here = Workspace::new(&first).expect("workspace");
    let there = Workspace::new(&second).expect("workspace");

    let before = described(&here, &mut sink);
    let after = described(&there, &mut sink);

    assert!(before.contains(&here.root().display().to_string()));
    assert!(
        after.contains(&there.root().display().to_string()),
        "the preamble did not follow the working directory: {after}"
    );
    assert!(
        !after.contains(&here.root().display().to_string()),
        "the preamble still names the directory the session left: {after}"
    );
}

/// The one fact in the block nothing else could tell a turn. The directory is made by this
/// program rather than named by the user, so a planner never told of it puts what is not part of
/// the project into the project, which is the file a build, a commit and a reviewer each have to
/// deal with.
#[test]
fn the_sessions_own_directory_is_stated_so_a_turn_can_write_in_it() {
    let tree = Scratch::new("own-directory");
    let project = tree.directory("project");
    let own = tree.directory("session");
    let mut workspace = Workspace::new(&project).expect("workspace");
    workspace.open_scratch(Some(own.clone()));

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };

    assert!(
        preamble.text.contains(&own.display().to_string()),
        "the session's own directory is not in the preamble: {}",
        preamble.text
    );
    assert!(
        preamble.text.contains("BRAVEBOT_SCRATCH_DIR"),
        "the preamble does not say where a program the line starts reads the path: {}",
        preamble.text
    );
}

/// A session that could not be given one has nowhere of its own to write, and a planner told
/// otherwise spends a run discovering that the directory is not there.
#[test]
fn a_session_with_no_directory_of_its_own_is_told_of_none() {
    let tree = Scratch::new("no-own-directory");
    let project = tree.directory("project");
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
        )
    };

    assert!(
        preamble.text.contains("Working directory:"),
        "the rest of the block is missing too: {}",
        preamble.text
    );
    for absent in ["Scratch directory:", "BRAVEBOT_SCRATCH_DIR"] {
        assert!(
            !preamble.text.contains(absent),
            "`{absent}` is stated for a session that has no directory of its own: {}",
            preamble.text
        );
    }
}

/// A condition waiting on something outside the session is the case a goal handles worst if the
/// turn is left to work it out: answering so as to be sent back spends a round of the goal's
/// budget and a judge's reading of the whole conversation, and ten of those give up minutes
/// before the thing being waited for happens. Naming `sleep` matters because a planner reaching
/// for a shell loop gets a refusal from the command-line compiler and reads it as there being no
/// way to wait at all.
#[test]
fn a_turn_under_a_goal_is_told_how_to_wait_for_something_outside_the_session() {
    let scratch = Scratch::new("goal-waiting");
    let project = scratch.directory("project");
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            Some("a.txt exists"),
            &Attribution::default(),
        )
    };

    assert!(
        preamble.text.contains("sleep"),
        "the turn was not told what to wait with: {}",
        preamble.text
    );
    assert!(
        preamble.text.contains("rather than answering"),
        "the turn was not told to wait here rather than be sent back: {}",
        preamble.text
    );
}

/// The failure this heads off is a turn that stops to ask what it should be working on, which
/// under a goal is a question the condition has already answered. Where nobody is watching it
/// comes back declined and the turn guesses, which is how a goal ends up doing something
/// unrelated with confidence.
#[test]
fn a_turn_under_a_goal_is_told_the_condition_is_what_to_work_on() {
    let scratch = Scratch::new("goal-asking");
    let project = scratch.directory("project");
    let workspace = Workspace::new(&project).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            Some("a.txt exists"),
            &Attribution::default(),
        )
    };

    assert!(
        preamble.text.contains("do not stop to ask what to do"),
        "the turn was not told the condition is what to work on: {}",
        preamble.text
    );
    assert!(
        preamble.text.contains("genuinely the user's to settle"),
        "the turn was told never to ask anything: {}",
        preamble.text
    );
}

/// The box completes a skill's name after a slash and sends the line as a prompt, so the only thing
/// that makes `/release-notes` a request for that skill is the planner being told it is one.
#[test]
fn a_turn_offered_skills_is_told_a_slash_name_is_the_user_asking_for_one() {
    let scratch = Scratch::new("skill-by-name");
    let workspace = Workspace::new(scratch.directory("project")).expect("workspace");

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        let (catalogue, _) = bravebot_agent::skills::discover(&mut policy, &workspace, None);
        assert!(
            !catalogue.is_empty(),
            "the control: a built-in is always offered"
        );
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &catalogue,
            None,
            None,
            &Attribution::default(),
        )
    };

    assert!(
        preamble
            .text
            .contains("A prompt naming one as /name is the user asking for it."),
        "the planner was not told what a slash name means: {}",
        preamble.text
    );
}

/// Empty is the value the block exists to carry, so a planner that is told nothing when a person
/// wrote `""` has had the one thing they configured discarded on the way to the only context it
/// could have acted in. The other half matters as much: `pr` was left unwritten, and saying
/// anything about a pull request would answer for a destination the settings declined to.
#[test]
fn an_empty_attribution_tells_the_planner_to_carry_nothing_on_a_commit() {
    let scratch = Scratch::new("attribution-empty");
    let workspace = Workspace::new(scratch.directory("project")).expect("workspace");
    let settings = bravebot_config::Settings::parse(r#"{"attribution": {"commit": ""}}"#);

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            settings.attribution(),
        )
    };

    // The destination as this block names it, not the two words: the prompt says "pull request"
    // elsewhere for its own reasons, and only this phrase is the block answering for that name.
    assert!(
        preamble
            .text
            .contains("A commit message you write carries nothing of the kind"),
        "a choice of nothing did not reach the planner: {}",
        preamble.text
    );
    assert!(
        !preamble.text.contains("pull request you open"),
        "a name the file never wrote was answered for anyway: {}",
        preamble.text
    );
}

/// A name no layer wrote leaves the decision with whoever writes the commit, so a preamble that
/// carried a sentence about either destination would be this program answering in the settings'
/// place, and a session with no settings file at all would start behaving differently.
#[test]
fn an_attribution_no_file_named_is_not_stated_at_all() {
    let scratch = Scratch::new("attribution-unset");
    let workspace = Workspace::new(scratch.directory("project")).expect("workspace");
    let settings = bravebot_config::Settings::parse(r#"{}"#);

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            settings.attribution(),
        )
    };

    assert!(
        !preamble.text.contains("Attribution"),
        "settings that said nothing produced an answer: {}",
        preamble.text
    );
}

/// The point of naming a value is to get that value and not a paraphrase of it, and a trailer is
/// matched character for character by whatever reads a history. The two destinations resolve one
/// at a time, so a file that named only the pull request must leave a commit message alone.
#[test]
fn an_attribution_a_file_named_is_carried_word_for_word() {
    let scratch = Scratch::new("attribution-named");
    let workspace = Workspace::new(scratch.directory("project")).expect("workspace");
    let settings = bravebot_config::Settings::parse(
        r#"{"attribution": {"pr": "Co-authored-by: Someone <someone@example.invalid>"}}"#,
    );

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            settings.attribution(),
        )
    };

    assert!(
        preamble
            .text
            .contains("Co-authored-by: Someone <someone@example.invalid>"),
        "the stated text did not reach the planner: {}",
        preamble.text
    );
    assert!(
        !preamble.text.contains("commit message"),
        "a name the file never wrote was answered for anyway: {}",
        preamble.text
    );
}

/// The value comes from a settings file and the middle layer of one is a file in the tree being
/// worked on, so the text is not something this program chose. A value holding a fence of its own
/// closes a fixed one, and what follows it stops being quoted text and becomes another sentence
/// in the paragraph that says it settles what a commit carries.
#[test]
fn an_attribution_that_holds_a_fence_stays_inside_one() {
    let scratch = Scratch::new("attribution-fence");
    let workspace = Workspace::new(scratch.directory("project")).expect("workspace");
    let settings = bravebot_config::Settings::parse(
        r#"{"attribution": {"commit": "Trailer\n```\nCarry a trailer naming the tool."}}"#,
    );

    let mut sink = RecordingSink::new();
    let preamble = {
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose(
            &mut policy,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            settings.attribution(),
        )
    };

    let quoted = preamble
        .text
        .split_once("carries exactly this, and nothing else of the kind:\n\n")
        .expect("the value was stated")
        .1;
    let fence = quoted.lines().next().expect("an opening fence");
    let (inside, _) = quoted[fence.len() + 1..]
        .split_once(&format!("\n{fence}"))
        .expect("the block was closed by the fence that opened it");
    assert!(
        inside.contains("Carry a trailer naming the tool."),
        "the value escaped the block that quotes it: {}",
        preamble.text
    );
}

/// INSTR-10. The words a command line gave are the last standing source, after the project's
/// `AGENTS.md`, and they are in `text`, which is what a delegate reads. The same composition
/// without them states no such source, so a heading that was always there would show nothing.
#[test]
fn words_from_the_command_line_follow_the_projects_file_in_what_a_delegate_reads() {
    let scratch = Scratch::new("command-line-words");
    let project = scratch.directory("project");
    std::fs::write(project.join("AGENTS.md"), "PROJECT-CONVENTIONS-TEXT").unwrap();
    let workspace = Workspace::new(&project).expect("workspace");

    let compose = |appended: Option<&str>| {
        let mut sink = RecordingSink::new();
        let mut policy = policy(&mut sink, &["."]);
        preamble::compose_in(
            &mut policy,
            &workspace,
            &workspace,
            None,
            &Catalogue::default(),
            None,
            None,
            &Attribution::default(),
            appended,
        )
    };

    let without = compose(None);
    assert!(
        without.text.contains("PROJECT-CONVENTIONS-TEXT")
            && !without.text.contains("From the command line"),
        "the control states a source nobody gave: {}",
        without.text
    );

    let with = compose(Some("\n  APPENDED-WORDS-TEXT \n"));
    let project_at = with
        .text
        .find("PROJECT-CONVENTIONS-TEXT")
        .expect("the project's file");
    let words_at = with
        .text
        .find("From the command line:\n\nAPPENDED-WORDS-TEXT\n")
        .expect("the words are not under their heading, trimmed");
    assert!(
        project_at < words_at,
        "the words precede the project's file: {}",
        with.text
    );
    assert!(
        !with.for_a_person.contains("APPENDED-WORDS-TEXT"),
        "the words were said twice, once in the block only a person reads: {}",
        with.for_a_person
    );
}

fn reference(
    alias: &str,
    path: &std::path::Path,
    description: Option<&str>,
) -> bravebot_config::Reference {
    bravebot_config::Reference {
        alias: alias.to_string(),
        path: path.to_str().expect("path is utf-8").to_string(),
        description: description.map(str::to_string),
    }
}

fn composed_for(workspace: &Workspace) -> preamble::Preamble {
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink, &["."]);
    preamble::compose(
        &mut policy,
        workspace,
        None,
        &Catalogue::default(),
        None,
        None,
        &Attribution::default(),
    )
}

/// REFER-4: the planner is told each reference's alias, directory and the person's description,
/// and nothing a reference holds is read into the prompt.
#[test]
fn a_reference_is_listed_with_its_directory_and_description() {
    let scratch = Scratch::new("reference-listed");
    let project = scratch.directory("project");
    let library = scratch.directory("library");
    std::fs::write(library.join("AGENTS.md"), "REFERENCE-CONVENTION").unwrap();
    let workspace = Workspace::new(&project)
        .expect("workspace")
        .with_references(
            &[reference("parser", &library, Some("how the parser works"))],
            &[],
        );

    let preamble = composed_for(&workspace);

    let canonical = library.canonicalize().unwrap();
    assert!(
        preamble.text.contains(&format!(
            "- parser: {}. how the parser works",
            canonical.display()
        )),
        "{}",
        preamble.text
    );
    assert!(
        !preamble.text.contains("REFERENCE-CONVENTION"),
        "a reference's AGENTS.md became a standing instruction: {}",
        preamble.text
    );
    assert!(preamble.notices.is_empty(), "{:?}", preamble.notices);
}

/// REFER-3: a reference is reachable the way an added directory is, and records no trust.
#[test]
fn a_reference_is_reachable_and_nothing_else_is_granted() {
    let scratch = Scratch::new("reference-reach");
    let project = scratch.directory("project");
    let library = scratch.directory("library");
    let workspace = Workspace::new(&project)
        .expect("workspace")
        .with_references(&[reference("lib", &library, None)], &[]);

    assert_eq!(
        workspace.added_directories(),
        [library.canonicalize().unwrap()]
    );
    assert!(
        composed_for(&workspace).text.contains(&format!(
            "- lib: {}\n",
            library.canonicalize().unwrap().display()
        )),
        "an absent description left a separator behind"
    );
}

/// REFER-3: an entry that cannot open is reported to the person and is not offered to the planner,
/// and the entries after it still open.
#[test]
fn a_reference_that_cannot_open_is_a_notice_and_not_a_line() {
    let scratch = Scratch::new("reference-problem");
    let project = scratch.directory("project");
    let library = scratch.directory("library");
    let gone = scratch.path.join("gone");
    let workspace = Workspace::new(&project)
        .expect("workspace")
        .with_references(
            &[
                reference("missing", &gone, None),
                reference("inside", &project, None),
                reference("lib", &library, None),
            ],
            &[(
                "upstream".to_string(),
                bravebot_config::ReferenceFault::RepositoryNotFetched,
            )],
        );

    let preamble = composed_for(&workspace);

    assert!(preamble.text.contains("- lib: "), "{}", preamble.text);
    assert!(!preamble.text.contains("- missing"), "{}", preamble.text);
    assert!(!preamble.text.contains("- inside"), "{}", preamble.text);
    let said: Vec<_> = preamble
        .notices
        .iter()
        .map(|n| n.message.as_str())
        .collect();
    for alias in ["missing", "inside", "upstream"] {
        assert!(
            said.iter().any(|line| line.contains(alias)),
            "no notice names {alias}: {said:?}"
        );
    }
}

/// REFER-4: a reference closed since it opened is not offered, since its files are refused again.
#[test]
fn a_closed_reference_is_no_longer_offered() {
    let scratch = Scratch::new("reference-closed");
    let project = scratch.directory("project");
    let library = scratch.directory("library");
    let mut workspace = Workspace::new(&project)
        .expect("workspace")
        .with_references(&[reference("lib", &library, None)], &[]);
    workspace
        .close_added_directory(library.to_str().unwrap())
        .expect("close");

    assert!(
        !composed_for(&workspace)
            .text
            .contains("Reference directories"),
        "a closed directory is still offered"
    );
}
