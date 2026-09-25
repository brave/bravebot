//! Delegate definitions: the kinds of delegate a person keeps in a file.
//!
//! Three kinds are this program's own. A definition is one more, written down somewhere rather
//! than compiled in, and it looks like a skill because it is the same problem:
//!
//! ```text
//! ---
//! name: rule-reviewer
//! description: Checks a diff against the rule in docs/development/reviewing-for-the-rule.md.
//! kind: reader
//! tools: read_file, list_files
//! ---
//!
//! Read the diff and the four shapes a violation takes. Report the shape and the file.
//! ```
//!
//! **A definition names an enumerated kind. It never describes a capability set.** `kind:` is
//! required and resolves through [`Kind::from_name`], so what a file chooses is which of the
//! three this delegate is. It may then name fewer tools than that kind reaches, and there is no
//! spelling of `tools:` that reaches one the kind does not: delegation redistributes authority
//! and never creates it, so a checked-in file cannot be the author of any.
//!
//! # What this module may and may not read
//!
//! Everything here parses **trusted** text, for the reason [`crate::skills`] gives at more
//! length: a definition's name and description go into a tool schema verbatim, and its body is
//! the whole of what a second planner is told it is. A source that fails
//! `Policy::read_trusted_content` is dropped entirely rather than quarantined, because a
//! reference in place of an instruction is no use to anybody, and a directory nobody vouched for
//! is counted rather than named, because a directory name is content too.
//!
//! The set is fixed before the turn and the kernel holds it. What a planner names is compared
//! against it, which decides nothing an attacker steers only because nothing an attacker wrote
//! ever entered the set.

use crate::skills::{Catalogue, Notice};
use crate::workspace::Workspace;
use bravebot_core::capability::Capability;
use bravebot_core::delegate::{Admitted, Definition, Definitions, Kind, Narrowing};
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_i18n::t;
use std::path::Path;

/// The directory holding definitions, inside the user's own directory and inside a project.
///
/// Spelled the way skills are, and flat rather than a directory each: a definition is one file,
/// so the directory a skill needs for the rest of its material has nothing to hold.
const AGENTS: &str = "agents";
const WORKSPACE_AGENTS: &str = ".bravebot/agents";

/// What a file turned out to be.
enum Read {
    /// A definition, ready to go into the set.
    Definition(Box<Definition>),
    /// Not a definition at all: no `name`, so nothing claimed to be one.
    ///
    /// Silent. A directory of definitions is a place a person also keeps a README, and a note
    /// beside the files is not a mistake to report.
    NotOne,
    /// A file claiming to be a definition and failing to be one, with what it is missing.
    Skipped(&'static str),
}

/// Read one definition out of the text of a file.
///
/// Every refusal here is a narrowing: what comes back is either a definition naming one of the
/// three kinds, or nothing at all.
fn read_definition(text: &str, origin: &str) -> Read {
    let Some(declared) = crate::skills::declarations(text) else {
        return Read::NotOne;
    };
    let Some(name) = declared.get("name").filter(|name| !name.is_empty()) else {
        return Read::NotOne;
    };
    if !is_a_name(name) {
        return Read::Skipped("its name may not begin with '-' or contain a colon");
    }
    if !Definitions::may_be_named(name) {
        return Read::Skipped(
            "reader, checker and worker are the kinds' own names, so a definition cannot take one",
        );
    }
    let Some(description) = declared.get("description").filter(|d| !d.is_empty()) else {
        return Read::Skipped("it needs a description saying when to use it");
    };
    let Some(kind) = declared.get("kind").map(String::as_str) else {
        return Read::Skipped("it needs a kind");
    };
    let Some(kind) = Kind::from_name(kind) else {
        return Read::Skipped("its kind is not one of reader, checker or worker");
    };

    let mut definition = Definition::from_file(
        name,
        description,
        kind,
        declared.get("tools").map(|named| names_in(named)),
        crate::skills::body_after_frontmatter(text),
        origin,
    );

    // `inherit` is how other agents' definitions name no model, so one ported from them keeps
    // meaning that rather than sending the word as a model name.
    if let Some(model) = declared
        .get("model")
        .map(|m| m.trim())
        .filter(|m| !m.is_empty() && !m.eq_ignore_ascii_case("inherit"))
    {
        definition = definition.with_model(model);
    }

    if let Some(skills) = declared.get("skills") {
        definition = definition.with_skills(names_in(skills));
    }

    Read::Definition(Box::new(definition))
}

/// Whether this is a name a definition may go by.
///
/// Two rules, both borrowed from the agent definitions people already write for other tools, so
/// one checked-in directory serves them and this:
///
/// - **No leading `-`.** A name beginning with one reads as a command-line switch wherever one is
///   printed beside other words.
/// - **No colon, and none that normalises to one.** A colon is what separates a namespace from a
///   name everywhere one is written, so it stays reserved even though nothing here namespaces
///   anything yet. The three characters that fold to a colon are written out rather than reached
///   through a normalisation pass, which would be a dependency for four code points; the fourth
///   is the double-colon ligature, which folds to a string containing two. A character Unicode
///   adds to that set later is the known cost, and the test below is where it would be added.
fn is_a_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.chars().any(|c| FOLDS_TO_A_COLON.contains(&c))
}

/// Every character that is a colon or normalises to one.
const FOLDS_TO_A_COLON: [char; 5] = [
    ':',        // U+003A COLON
    '\u{2a74}', // DOUBLE COLON EQUAL, which folds to "::="
    '\u{fe13}', // PRESENTATION FORM FOR VERTICAL COLON
    '\u{fe55}', // SMALL COLON
    '\u{ff1a}', // FULLWIDTH COLON
];

/// The names a `tools:` or a `skills:` value lists.
///
/// A comma and a space both separate, so a YAML scalar (`read_file, list_files`) and a YAML
/// sequence (`- read_file` on its own line) both arrive here as something this splits the same
/// way. Neither separates inside parentheses, so a parenthesised argument written for another
/// agent stays one token and is dropped whole rather than splitting into two tokens that each
/// match nothing.
///
/// `*` is not special. It is a widening spelling, and a definition may not widen anything, so it
/// is a name matching nothing like any other.
fn names_in(value: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;

    for c in value.chars() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            ',' | ' ' | '\t' | '\n' if depth == 0 => {
                if !current.is_empty() {
                    names.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        names.push(current);
    }

    // The bullet of a YAML sequence, which the one dialect joins into the value along with the
    // entry it introduces. Dropped here rather than in the parser, where a `-` opening a line is
    // not always a bullet.
    names.retain(|name| name != "-");
    names
}

/// Find the kinds of delegate available to this turn.
///
/// The three the program wrote, then the user's own directory, then the project, least specific
/// first so the project has the last word. A definition of the same name replaces the one before
/// it, which is how a project narrows a habit without restating it, and how one of a person's own
/// replaces a kind for every project they work in.
pub fn discover<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    home: Option<&Path>,
) -> (Definitions, Vec<Notice>) {
    // The three kinds, which are always here. There is no gate to pass and nothing to vouch for:
    // this is the program's own text rather than a source somebody supplied, so it can neither be
    // refused nor be missing.
    let mut definitions = Definitions::default();
    let mut notices = Vec::new();

    if let Some(home) = home {
        discover_home(policy, &home.join(AGENTS), &mut definitions, &mut notices);
    }
    discover_workspace(policy, workspace, &mut definitions, &mut notices);

    (definitions, notices)
}

/// The set a turn starting now would resolve, for an interface about to start one.
///
/// Read the way a turn reads it, through a policy holding only the read, so a name a person
/// typed is compared against the set the turn will compare it against. The turn resolves the set
/// again and its kernel decides; this is what lets a miss be said before anything starts, where a
/// turn refused later is drawn as a failure whose reason nobody is shown.
pub fn resolved<S: Sink>(
    workspace: &Workspace,
    home: Option<&Path>,
    trust: bravebot_core::trust::TrustStore,
    sink: &mut S,
) -> Definitions {
    let mut routing = bravebot_core::policy::Routing::new();
    routing.insert_trusted("agents", WORKSPACE_AGENTS);
    let Ok(policy) = Policy::begin(
        routing,
        bravebot_core::policy::ReleasePlan::new(),
        bravebot_core::capability::CapabilitySet::from_iter([Capability::FileRead]),
        sink,
    ) else {
        return Definitions::default();
    };
    let mut policy = policy
        .with_trust(trust)
        .with_root(workspace.root())
        .with_backslash_separates(crate::workspace::BACKSLASH_SEPARATES);
    discover(&mut policy, workspace, home).0
}

/// Definitions from `~/.bravebot/agents`, labelled from where they sit.
fn discover_home<S: Sink>(
    policy: &mut Policy<'_, S>,
    root: &Path,
    definitions: &mut Definitions,
    notices: &mut Vec<Notice>,
) {
    if policy.before_capability(Capability::FileRead).is_err() {
        return;
    }

    for file in definition_files(root) {
        let origin = format!("~/.bravebot/{AGENTS}/{file}");

        let Ok(text) = std::fs::read_to_string(root.join(&file)) else {
            continue;
        };

        // Labelled here and gated below rather than used directly, so there is one way into the
        // set and it refuses, and the trail records both halves.
        let labelled = policy.label_user_configuration(&origin, text);
        let Ok(text) = policy.read_trusted_content("agents", &labelled) else {
            continue;
        };

        admit(
            read_definition(&text, &origin),
            &origin,
            definitions,
            notices,
        );
    }
}

/// Definitions from `<workspace>/.bravebot/agents`, labelled by the trust map.
fn discover_workspace<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    definitions: &mut Definitions,
    notices: &mut Vec<Notice>,
) {
    let root = workspace.root().join(WORKSPACE_AGENTS);
    let files = definition_files(&root);
    if files.is_empty() {
        return;
    }

    // Checked before anything is enumerated, and checked on the label rather than on any content.
    // A file name is content too: a definition in a project nobody vouched for could be named to
    // read like an instruction, and it would reach the user's screen in a notice even if it never
    // reached a prompt.
    if !policy.trust().is_trusted(WORKSPACE_AGENTS) {
        let (count, verb) = counted(files.len());
        notices.push(Notice::from_message(format!(
            "{count} in {WORKSPACE_AGENTS} {verb} not loaded: this directory is not trusted"
        )));
        return;
    }

    for file in files {
        let relative = format!("{WORKSPACE_AGENTS}/{file}");

        let Ok(contents) = workspace.read(policy, &Labelled::trusted(relative.clone())) else {
            continue;
        };

        // Asked of the label before it is asked of the gate, for the reason `skills` gives: the
        // gate still runs whenever this proceeds, and what this avoids is recording a denial for
        // a condition that is ordinary and expected.
        if !contents.label().is_trusted() {
            notices.push(Notice::from_message(format!(
                "{relative} was not loaded: it is not trusted"
            )));
            continue;
        }
        let Ok(text) = policy.read_trusted_content("agents", &contents) else {
            continue;
        };

        admit(
            read_definition(&text, &relative),
            &relative,
            definitions,
            notices,
        );
    }
}

/// Put what a file turned out to be into the set, or say why it is not there.
fn admit(read: Read, origin: &str, definitions: &mut Definitions, notices: &mut Vec<Notice>) {
    let why = match read {
        // The kernel keeps its own invariant about the three kinds' names, and `read_definition`
        // asked about it already, so a refusal here is a rule this loader missed rather than a
        // file. Reported rather than dropped: silence would be the one case where somebody's
        // file does nothing and nothing says so.
        Read::Definition(definition) => match definitions.insert(*definition) {
            Admitted::AsWritten => return,
            // A later source narrows a name and never widens it, and what it asked for and did
            // not get is said rather than dropped quietly: a narrowing nobody is told about
            // reads to whoever wrote the file as one still in force. Both files can be named
            // because by here each came from a source somebody vouched for.
            Admitted::Narrowed(narrowing) => {
                notices.push(Notice::from_message(narrowed(origin, &narrowing)));
                return;
            }
            Admitted::Refused => "its name is one of the kinds' own",
        },
        Read::NotOne => return,
        Read::Skipped(why) => why,
    };
    notices.push(Notice::from_message(format!("{origin} was skipped: {why}")));
}

/// What to tell whoever wrote a definition that the one of the same name before it cut down.
///
/// The words are here rather than in the kernel, which hands over which of the two axes moved
/// and nothing about how to say it. Both halves where both moved, because a person told only
/// about the kind would go on believing their `tools:` line was the one in force.
fn narrowed(origin: &str, narrowing: &Narrowing) -> String {
    let mut said = Vec::new();
    if narrowing.named != narrowing.loaded {
        said.push(format!(
            "it names kind {} and is loaded as a {}",
            narrowing.named, narrowing.loaded
        ));
    }
    if let Some(confined_to) = narrowing.confined_to.as_deref() {
        said.push(match confined_to {
            [] => "it is loaded with no tools at all".to_string(),
            tools => format!("it is loaded confined to {}", tools.join(", ")),
        });
    }
    format!(
        "{origin} does not widen {}: {}",
        narrowing.replaced,
        said.join(", and ")
    )
}

/// What to tell whoever wrote a definition naming a skill this turn did not find.
///
/// Such a name selects nothing, as a `tools:` name that is not a tool does, and silence would
/// leave a misspelt one reading to its author as a skill the delegate is offered. Named, because
/// the name is the definition's own words and the definition came from a source somebody vouched
/// for.
pub fn skills_not_found(definitions: &Definitions, skills: &Catalogue) -> Vec<Notice> {
    definitions
        .iter()
        .filter_map(|definition| {
            let mut missing: Vec<&str> = Vec::new();
            for name in definition.skills()? {
                if skills.get(name).is_none() && !missing.contains(&name.as_str()) {
                    missing.push(name);
                }
            }
            if missing.is_empty() {
                return None;
            }
            Some(Notice::from_message(t!(
                delegate_skills_not_found,
                definition = definition.origin(),
                count = missing.len(),
                skills = missing.join(", ")
            )))
        })
        .collect()
}

/// A count of definitions, and the verb that agrees with it.
fn counted(n: usize) -> (String, &'static str) {
    if n == 1 {
        ("1 delegate definition".to_string(), "was")
    } else {
        (format!("{n} delegate definitions"), "were")
    }
}

/// The names of the files under a definitions root, sorted.
///
/// Sorted so a turn offers the same definitions in the same order every time, and so two files
/// resolve against each other the same way on every machine. An order that came from the
/// filesystem would vary by machine, which would make which of two definitions is live vary with
/// it.
fn definition_files(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.ends_with(".md"))
        .collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition_of(text: &str) -> Definition {
        match read_definition(text, "test") {
            Read::Definition(definition) => *definition,
            Read::NotOne => panic!("not read as a definition at all"),
            Read::Skipped(why) => panic!("skipped: {why}"),
        }
    }

    /// The whole shape, so the rest of these tests are about one key at a time.
    #[test]
    fn a_definition_carries_a_name_a_description_a_kind_and_a_body() {
        let definition = definition_of(
            "---\nname: rule-reviewer\ndescription: Checks a diff. Use before a review.\nkind: \
             reader\ntools: read_file, list_files\n---\n\nRead the diff and report the shape.\n",
        );

        assert_eq!(definition.name(), "rule-reviewer");
        assert_eq!(
            definition.description(),
            "Checks a diff. Use before a review."
        );
        assert_eq!(definition.kind(), Kind::Reader);
        assert_eq!(
            definition.tools(),
            Some(["read_file".to_string(), "list_files".to_string()].as_slice())
        );
        assert_eq!(definition.prompt(), "Read the diff and report the shape.\n");
    }

    /// `kind` resolves through the enumerated set or the file is not a definition. A file that
    /// could name a kind of its own would be a checked-in file describing a capability set, which
    /// is the one thing a definition may never do.
    #[test]
    fn a_kind_nobody_enumerated_is_not_a_definition() {
        for kind in [
            "superuser",
            "planner",
            "Reader",
            "file_write",
            "",
            "../worker",
        ] {
            let text =
                format!("---\nname: escalate\ndescription: does everything\nkind: {kind}\n---\n");
            assert!(
                matches!(read_definition(&text, "test"), Read::Skipped(_)),
                "'{kind}' was accepted as a kind"
            );
        }
    }

    /// A definition with no kind describes nothing, so it selects nothing. Left out rather than
    /// defaulted to the narrowest: a default would make the required key optional in practice and
    /// leave a file's author believing they had said something they had not.
    #[test]
    fn a_definition_needs_a_kind_and_a_description() {
        assert!(matches!(
            read_definition("---\nname: a\ndescription: b\n---\n", "test"),
            Read::Skipped(_)
        ));
        assert!(matches!(
            read_definition("---\nname: a\nkind: reader\n---\n", "test"),
            Read::Skipped(_)
        ));
    }

    /// A directory of definitions is a place a person also keeps a README. A file claiming to be
    /// nothing is not a mistake and is not reported as one; a file claiming to be a definition
    /// and failing is.
    #[test]
    fn a_file_with_no_name_is_not_a_definition_and_is_not_an_error() {
        for text in [
            "just some notes about the definitions in here\n",
            "---\nkind: reader\ndescription: no name at all\n---\n",
            "---\nname: rule-reviewer\ndescription: unterminated\nkind: reader\n",
        ] {
            assert!(
                matches!(read_definition(text, "test"), Read::NotOne),
                "a file nobody claimed was a definition was reported: {text}"
            );
        }
    }

    /// A colon separates a namespace from a name everywhere one is written, and a fullwidth one
    /// normalises to the same character. A name carrying one is refused rather than stripped, so
    /// there is no spelling that reaches a name somebody else's definition already has.
    #[test]
    fn a_name_that_is_or_folds_to_a_colon_is_refused() {
        for name in [
            "plugin:reviewer",
            "plugin\u{ff1a}reviewer",
            "plugin\u{fe55}reviewer",
            "plugin\u{fe13}reviewer",
            "plugin\u{2a74}reviewer",
            "-reviewer",
        ] {
            let text =
                format!("---\nname: {name}\ndescription: checks a diff\nkind: reader\n---\n");
            assert!(
                matches!(read_definition(&text, "test"), Read::Skipped(_)),
                "'{name}' was accepted as a name"
            );
        }
        assert!(is_a_name("rule-reviewer"));
        assert!(is_a_name("reviewer-"));
    }

    /// A comma and a space both separate, so the scalar spelling and the sequence spelling both
    /// arrive as the same list. A definition written for another agent parses here, which is what
    /// makes one checked-in directory serve both.
    #[test]
    fn a_tools_list_separates_on_commas_and_on_spaces() {
        for value in [
            "read_file, list_files",
            "read_file,list_files",
            "read_file list_files",
            "- read_file\n- list_files",
        ] {
            assert_eq!(
                names_in(value),
                ["read_file", "list_files"],
                "'{value}' did not read as two tools"
            );
        }
    }

    /// Neither separator applies inside parentheses, so an argument written for an agent whose
    /// tools take them stays one token. It matches no tool here and is dropped whole, where
    /// splitting it would leave two tokens that each match nothing and one that reads like a
    /// tool name.
    #[test]
    fn a_parenthesised_argument_stays_one_token() {
        assert_eq!(
            names_in("read_file, Bash(git log --oneline), list_files"),
            ["read_file", "Bash(git log --oneline)", "list_files"]
        );
        assert_eq!(names_in("Bash(a(b) c)"), ["Bash(a(b) c)"]);
    }

    /// `*` is a widening spelling and a definition may not widen anything, so it is a name
    /// matching no tool rather than a way to ask for all of them.
    #[test]
    fn an_asterisk_is_a_name_and_never_the_whole_list() {
        let definition = definition_of(
            "---\nname: everything\ndescription: asks for it all\nkind: worker\ntools: *\n---\n",
        );

        assert_eq!(definition.tools(), Some(["*".to_string()].as_slice()));
        assert!(
            !definition
                .capabilities()
                .contains(&bravebot_core::capability::Capability::FileWrite),
            "an asterisk widened a definition to everything its kind holds"
        );
    }

    /// A `reader` is a reader wherever a session runs, so a file claiming a kind's own name is
    /// skipped rather than taken. Reported, because a file that silently did nothing would be
    /// the one case where somebody's own configuration is ignored with nothing said.
    #[test]
    fn a_definition_cannot_take_a_kinds_own_name() {
        for name in Kind::NAMES {
            let text = format!(
                "---\nname: {name}\ndescription: pretending to be a kind\nkind: worker\n---\n"
            );
            assert!(
                matches!(read_definition(&text, "test"), Read::Skipped(_)),
                "'{name}' was accepted as a definition's name"
            );
        }
    }

    /// Keys nothing here reads are ignored rather than refused, so a definition written for
    /// another agent loads. That is what lets one checked-in directory serve several of them.
    #[test]
    fn a_key_nothing_here_reads_is_ignored_rather_than_refused() {
        let definition = definition_of(
            "---\nname: rule-reviewer\ndescription: checks a diff\nkind: reader\ntemperature: \
             0.5\ncolor: blue\n---\n\nbody\n",
        );

        assert_eq!(definition.name(), "rule-reviewer");
        assert_eq!(definition.kind(), Kind::Reader);
    }

    /// A definition can name a model to run on.
    #[test]
    fn a_definition_reads_a_model_name() {
        let definition = definition_of(
            "---\nname: cheap-reader\ndescription: reads with haiku\nkind: reader\nmodel: \
             haiku\n---\n\nbody\n",
        );

        assert_eq!(definition.name(), "cheap-reader");
        assert_eq!(definition.model(), Some("haiku"));
    }

    /// An empty or whitespace-only model key is ignored, leaving the model unset.
    #[test]
    fn an_empty_model_name_in_a_definition_is_ignored() {
        let definition = definition_of(
            "---\nname: default-reader\ndescription: reads with parent model\nkind: reader\nmodel: \
             \"   \"\n---\n\nbody\n",
        );

        assert_eq!(definition.name(), "default-reader");
        assert_eq!(definition.model(), None);
    }

    /// `skills:` is read the way `tools:` is, so both spellings of a list arrive as the same
    /// names. An empty line names none, which is a delegate offered no skills, and an absent one
    /// is every skill the turn found: the two have to stay apart, or a definition written to be
    /// told nothing would be told everything.
    #[test]
    fn a_definition_reads_the_skills_it_names() {
        let skills_of = |line: &str| {
            definition_of(&format!(
                "---\nname: reviewer\ndescription: reviews\nkind: reader\n{line}---\n\nbody\n"
            ))
            .skills()
            .map(<[String]>::to_vec)
        };
        let both = Some(vec!["review-style".to_string(), "commit-style".to_string()]);

        assert_eq!(skills_of("skills: review-style, commit-style\n"), both);
        assert_eq!(
            skills_of("skills:\n  - review-style\n  - commit-style\n"),
            both
        );
        assert_eq!(skills_of("skills:\n"), Some(Vec::new()));
        assert_eq!(skills_of(""), None);
    }

    /// One misspelt name written twice is one name nothing found, so it is said once and in the
    /// singular rather than as two skills.
    #[test]
    fn a_skill_named_twice_and_found_nowhere_is_said_once() {
        let mut definitions = Definitions::default();
        definitions.insert(definition_of(
            "---\nname: reviewer\ndescription: reviews\nkind: reader\nskills: rule-reveiw, \
             rule-reveiw\n---\n\nbody\n",
        ));

        let said: Vec<String> = skills_not_found(&definitions, &Catalogue::default())
            .into_iter()
            .map(|notice| notice.message)
            .collect();

        assert_eq!(
            said,
            [
                "test names a skill this session did not find, so its delegate is offered without \
              it: rule-reveiw"
            ]
        );
    }

    #[test]
    fn a_definition_naming_inherit_names_no_model() {
        for written in ["inherit", "Inherit"] {
            let definition = definition_of(&format!(
                "---\nname: ported\ndescription: from elsewhere\nkind: reader\nmodel: \
                 {written}\n---\n\nbody\n"
            ));

            assert_eq!(definition.model(), None, "model: {written}");
        }
    }
}
