//! What `/memory` shows of the rules and facts kept, and what `/memory-tidy` proposes (RECALL-12).
//!
//! Everything here reads and nothing writes: a proposal names a fact and what would fix it, and the
//! person acts on it or asks the planner to.

use super::conflicts::{Line, read_disagreements};
use super::fact::{Fact, Scope};
use super::project::project_id;
use super::rank::{links, term_share, terms};
use super::store::Store;
use super::{
    DUPLICATE_AT, OVERSIZED_BYTES, SIGNATURE_SHARED, SIGNATURE_SKIP_OVER, SIGNATURE_TERMS,
    STALE_DAYS,
};
use crate::calendar::{civil_from_days, days_from_civil};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One thing `/memory-tidy` proposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// Two live facts that say nearly the same, by key.
    Duplicate(String, String),
    /// A fact that names code and has been neither verified nor edited for [`STALE_DAYS`].
    Stale(String),
    /// A `[[link]]` in `from` to a key `to` that names no fact.
    BrokenLink { from: String, to: String },
    /// A fact superseded by a key that names no fact.
    MissingReplacement(String),
    /// A fact over [`OVERSIZED_BYTES`].
    Oversized(String),
    /// Two rule lines found to disagree at the last check (RECALL-4), and the file that wins.
    RulesDisagree(Line, Line, String),
}

/// File extensions that make a word a path to code.
const CODE_EXTENSIONS: [&str; 22] = [
    "rs", "toml", "md", "py", "js", "ts", "tsx", "jsx", "json", "yaml", "yml", "go", "c", "h",
    "cpp", "sh", "hoon", "nix", "css", "html", "lock", "sql",
];

/// Extensions that prose also uses for a framework (`Node.js`), so a capitalised name with one and
/// no directory in front of it is not a path.
const FRAMEWORK_EXTENSIONS: [&str; 6] = ["js", "ts", "jsx", "tsx", "py", "go"];

/// Whether `body` names code: a path with an extension, a `::` path, an `fn name(` item, or a commit.
fn names_code(body: &str) -> bool {
    let words: Vec<&str> = body.split_whitespace().collect();
    if body.contains("::")
        || words.windows(2).any(|pair| {
            pair[0] == "fn"
                && pair[1].contains('(')
                && pair[1].starts_with(|c: char| c.is_alphabetic() || c == '_')
        })
    {
        return true;
    }
    words.iter().any(|word| {
        let word = word.trim_matches(|c: char| {
            !(c.is_ascii_alphanumeric() || c == '.' || c == '/' || c == '_' || c == '-')
        });
        // A plain number or date is not a commit: it needs a digit and a letter from a to f.
        let hex = (7..=40).contains(&word.len())
            && word.chars().all(|c| c.is_ascii_hexdigit())
            && word.chars().any(|c| c.is_ascii_digit())
            && word.chars().any(|c| matches!(c, 'a'..='f' | 'A'..='F'));
        let path = word.rsplit_once('.').is_some_and(|(stem, ext)| {
            let prose = !stem.contains('/')
                && stem.starts_with(|c: char| c.is_uppercase())
                && FRAMEWORK_EXTENSIONS.contains(&ext);
            !stem.is_empty() && CODE_EXTENSIONS.contains(&ext) && !prose
        });
        hex || path
    })
}

/// Days since 1970-01-01 of a `YYYY-MM-DD` date.
fn day_count(date: &str) -> Option<i64> {
    let mut parts = date.trim().splitn(3, '-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    ((1..=12).contains(&month) && (1..=31).contains(&day))
        .then(|| days_from_civil(year, month, day))
}

/// The signature of each fact: its [`SIGNATURE_TERMS`] rarest terms across `docs`.
fn signatures(docs: &[&Fact]) -> Vec<BTreeSet<String>> {
    let sets: Vec<BTreeSet<String>> = docs
        .iter()
        .map(|fact| terms(&fact.body).into_iter().collect())
        .collect();
    let mut held: BTreeMap<&str, usize> = BTreeMap::new();
    for set in &sets {
        for term in set {
            *held.entry(term).or_default() += 1;
        }
    }
    let picked: Vec<BTreeSet<String>> = sets
        .iter()
        .map(|set| {
            let mut ranked: Vec<&String> = set.iter().collect();
            ranked.sort_by_key(|term| (held[term.as_str()], term.as_str()));
            ranked.into_iter().take(SIGNATURE_TERMS).cloned().collect()
        })
        .collect();
    let mut on: BTreeMap<&str, usize> = BTreeMap::new();
    for signature in &picked {
        for term in signature {
            *on.entry(term).or_default() += 1;
        }
    }
    picked
        .iter()
        .map(|signature| {
            signature
                .iter()
                .filter(|term| on[term.as_str()] <= SIGNATURE_SKIP_OVER)
                .cloned()
                .collect()
        })
        .collect()
}

/// What would be tidied among `facts`, as of `today` (`YYYY-MM-DD`). `edited` gives the date a
/// fact's file was last changed, and a fact is as old as the later of that and its `verified`.
pub fn tidy(
    facts: &[(Scope, Fact)],
    today: &str,
    edited: &dyn Fn(Scope, &str) -> Option<String>,
) -> Vec<Item> {
    let mut items = Vec::new();
    let keys: BTreeSet<&str> = facts.iter().map(|(_, fact)| fact.key.as_str()).collect();
    let live: Vec<&Fact> = facts
        .iter()
        .map(|(_, fact)| fact)
        .filter(|fact| fact.is_live())
        .collect();

    let signatures = signatures(&live);
    for (at, first) in live.iter().enumerate() {
        for (offset, second) in live[at + 1..].iter().enumerate() {
            let other = at + 1 + offset;
            if signatures[at].intersection(&signatures[other]).count() < SIGNATURE_SHARED {
                continue;
            }
            // Either direction, so a short fact held whole inside a longer one is found too.
            let share =
                term_share(&first.body, &second.body).max(term_share(&second.body, &first.body));
            if share >= DUPLICATE_AT {
                items.push(Item::Duplicate(first.key.clone(), second.key.clone()));
            }
        }
    }

    let now = day_count(today);
    for (scope, fact) in facts {
        if fact.is_live() && names_code(&fact.body) {
            let latest = [fact.verified.clone(), edited(*scope, &fact.key)]
                .into_iter()
                .flatten()
                .filter_map(|date| day_count(&date))
                .max();
            let old = match (now, latest) {
                (Some(now), Some(then)) => now - then > i64::from(STALE_DAYS),
                // Nothing says when it was last looked at.
                (_, None) => true,
                (None, Some(_)) => false,
            };
            if old {
                items.push(Item::Stale(fact.key.clone()));
            }
        }
        for target in links(&fact.body) {
            if !keys.contains(target.as_str()) {
                items.push(Item::BrokenLink {
                    from: fact.key.clone(),
                    to: target,
                });
            }
        }
        if fact
            .superseded_by
            .as_deref()
            .is_some_and(|key| !keys.contains(key))
        {
            items.push(Item::MissingReplacement(fact.key.clone()));
        }
        if fact.body.len() > OVERSIZED_BYTES {
            items.push(Item::Oversized(fact.key.clone()));
        }
    }
    items
}

/// The `YYYY-MM-DD` of a file's last change, or none where it cannot be told.
fn edited_on(path: &Path) -> Option<String> {
    let seconds = std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Everything tidy proposes for the facts kept for the project at `root` and for every project,
/// and the disagreements the last rule check kept. Reads only, and makes no directory.
pub fn proposals(home: &Path, root: &Path, trusted: bool) -> Vec<Item> {
    let store = Store::new(home, project_id(root, trusted));
    let loaded = store.load();
    let mut items = tidy(&loaded.facts, &crate::preamble::today(), &|scope, key| {
        edited_on(&store.fact_path(scope, key))
    });
    items.extend(
        read_disagreements(&store.disagreements_path())
            .into_iter()
            .map(|(first, second, winner)| Item::RulesDisagree(first, second, winner)),
    );
    items
}

/// The most lines of a rules file that are counted.
const RULE_LINES_COUNTED: usize = 100_000;
/// The most bytes of a rules file that are read to count them.
const RULE_BYTES_READ: u64 = 8 * 1024 * 1024;

/// A rules file that exists, and how many lines it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFile {
    pub path: PathBuf,
    pub lines: usize,
    /// Whether the file holds more lines than were counted.
    pub more: bool,
}

/// The lines of the regular file at `path`, counted from at most [`RULE_BYTES_READ`] bytes and
/// stopping after [`RULE_LINES_COUNTED`]. A link is followed; a pipe, a device or a missing file is
/// none.
fn count_rule_file(path: &Path) -> Option<RuleFile> {
    use std::io::{BufRead, Read};
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    // A file longer than the bytes read has more lines than were counted, whatever they hold.
    let cut = meta.len() > RULE_BYTES_READ;
    let mut lines = 0;
    for _ in std::io::BufReader::new(file.take(RULE_BYTES_READ)).split(b'\n') {
        if lines == RULE_LINES_COUNTED {
            return Some(RuleFile {
                path: path.to_path_buf(),
                lines,
                more: true,
            });
        }
        lines += 1;
    }
    Some(RuleFile {
        path: path.to_path_buf(),
        lines,
        more: cut,
    })
}

/// The rules and facts `/memory` lists besides the definitions' memories (RECALL-12).
#[derive(Debug, Default)]
pub struct Overview {
    /// The rule layers that exist, in the order they win from the last: the one for every
    /// project, this project's, then the working directory's own (RECALL-2).
    pub rules: Vec<RuleFile>,
    /// The facts in scope, in key order.
    pub facts: Vec<(Scope, Fact)>,
}

/// The rule layers and the facts kept for the project at `workspace`'s root. Reads the front
/// matter of fact files and counts the lines of rule files, and makes no directory. The state
/// rule files are the ones a turn admits ([`crate::preamble::state_rule_files`]), and `own` is the
/// working directory's own file a turn reads, as [`crate::agents::project_rules_file`] chooses it.
pub fn overview(
    home: &Path,
    workspace: &crate::workspace::Workspace,
    trusted: bool,
    own: Option<&Path>,
) -> Overview {
    let project = project_id(workspace.root(), trusted);
    let mut rules: Vec<RuleFile> = crate::preamble::state_rule_files(&[workspace], home, &project)
        .iter()
        .filter_map(|file| count_rule_file(file))
        .collect();
    let store = Store::new(home, project);
    rules.extend(own.and_then(count_rule_file));
    Overview {
        rules,
        facts: store.load().facts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recall::fact::Source;
    use bravebot_core::label::Confidentiality;

    fn fact(key: &str, body: &str) -> (Scope, Fact) {
        (
            Scope::Project,
            Fact {
                key: key.to_string(),
                source: Source::Person,
                label: Confidentiality::Public,
                created: None,
                verified: Some("2026-10-01".to_string()),
                superseded_by: None,
                session: None,
                tags: Vec::new(),
                body: body.to_string(),
            },
        )
    }

    fn proposed(facts: &[(Scope, Fact)], today: &str) -> Vec<Item> {
        tidy(facts, today, &|_, _| None)
    }

    const POLL: &str = "The release forge is polled every minute by the scheduler job, and a failed poll retries after five minutes";

    #[test]
    fn two_near_copies_are_proposed_as_duplicates() {
        let facts = [
            fact("a", POLL),
            fact("b", &format!("{POLL} again")),
            fact(
                "c",
                "Staging deploys go out on Thursdays after the freeze lifts",
            ),
        ];
        assert_eq!(
            proposed(&facts, "2026-10-02"),
            [Item::Duplicate("a".to_string(), "b".to_string())]
        );
    }

    #[test]
    fn superseded_facts_are_never_proposed() {
        let mut old = fact("a", POLL);
        old.1.superseded_by = Some("b".to_string());
        let facts = [old, fact("b", POLL)];
        assert_eq!(proposed(&facts, "2026-10-02"), []);
    }

    #[test]
    fn a_fact_naming_code_unverified_for_30_days_is_stale() {
        let facts = [fact("a", "The poll lives in crates/agent/src/poll.rs")];
        assert_eq!(proposed(&facts, "2026-10-31"), []);
        assert_eq!(
            proposed(&facts, "2026-11-01"),
            [Item::Stale("a".to_string())]
        );
        // An edit to the file counts as a look at it.
        let edited = |_: Scope, _: &str| Some("2026-10-25".to_string());
        assert_eq!(tidy(&facts, "2026-11-01", &edited), []);
        for body in ["see fn poll(&self)", "Poller::run", "fixed in 1a2b3c4d"] {
            assert_eq!(
                proposed(&[fact("a", body)], "2027-01-01"),
                [Item::Stale("a".to_string())],
                "{body}"
            );
        }
    }

    #[test]
    fn a_fact_naming_no_code_is_never_stale() {
        let facts = [fact(
            "a",
            "Releases go out on Thursdays, version 1.2 first, defaced",
        )];
        assert_eq!(proposed(&facts, "2030-01-01"), []);
    }

    #[test]
    fn a_broken_link_and_a_missing_replacement_are_listed() {
        let mut gone = fact("old", "replaced");
        gone.1.superseded_by = Some("nowhere".to_string());
        let facts = [
            fact("a", "see [[b]] and [[missing|label]]"),
            fact("b", "kept"),
            gone,
        ];
        assert_eq!(
            proposed(&facts, "2026-10-02"),
            [
                Item::BrokenLink {
                    from: "a".to_string(),
                    to: "missing".to_string()
                },
                Item::MissingReplacement("old".to_string()),
            ]
        );
    }

    #[test]
    fn an_oversized_fact_is_listed() {
        let facts = [
            fact("big", &"word ".repeat(OVERSIZED_BYTES)),
            fact("fine", "short"),
        ];
        assert_eq!(
            proposed(&facts, "2026-10-02"),
            [Item::Oversized("big".to_string())]
        );
    }

    #[test]
    fn the_memory_tidy_command_changes_no_file() {
        let home = crate::testutil::scratch_dir("recall-tidy-read-only");
        let _ = std::fs::remove_dir_all(&home);
        let root = home.join("work");
        std::fs::create_dir_all(&root).expect("root");
        let store = Store::new(&home, project_id(&root, false));
        for (scope, key, text) in [
            (
                Scope::Everywhere,
                "a",
                "---\nsource: person\nverified: 2020-01-01\n---\nsee src/lib.rs and [[gone]]\n",
            ),
            (Scope::Project, "b", "plain\n"),
        ] {
            let path = store.fact_path(scope, key);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(&path, text).expect("write");
            let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_577_836_800);
            std::fs::File::options()
                .write(true)
                .open(&path)
                .and_then(|file| file.set_modified(old))
                .expect("age");
        }
        let disagreements = store.disagreements_path();
        std::fs::write(&disagreements, "AGENTS.md:1  rules.md:2  winner=rules.md\n")
            .expect("record");

        let snapshot = |home: &Path| -> Vec<(PathBuf, Vec<u8>)> {
            let mut found = Vec::new();
            let mut pending = vec![home.to_path_buf()];
            while let Some(directory) = pending.pop() {
                for entry in std::fs::read_dir(&directory).expect("read").flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        found.push((path.clone(), Vec::new()));
                        pending.push(path);
                    } else {
                        found.push((path.clone(), std::fs::read(&path).expect("file")));
                    }
                }
            }
            found.sort();
            found
        };
        let before = snapshot(&home);
        let items = proposals(&home, &root, false);
        assert_eq!(snapshot(&home), before);
        assert!(items.contains(&Item::Stale("a".to_string())), "{items:?}");
        assert!(items.contains(&Item::BrokenLink {
            from: "a".to_string(),
            to: "gone".to_string()
        }));
        assert!(
            matches!(items.last(), Some(Item::RulesDisagree(_, _, winner)) if winner == "rules.md")
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_plain_number_a_framework_name_and_a_word_fn_are_not_code() {
        for body in [
            "released on 20261001 with 12345678 rows",
            "we use Node.js and Vue.js here",
            "a fn is a function",
            "the defaced decade",
        ] {
            assert!(!names_code(body), "{body}");
        }
        for body in [
            "see src/Foo.js",
            "edit Cargo.toml",
            "call fn poll()",
            "at 1a2b3c4",
            "a_module.js",
        ] {
            assert!(names_code(body), "{body}");
        }
    }

    /// RECALL-12: `/memory` lists the state rule files a turn reads. A dotfiles-style link to a
    /// file elsewhere is followed and listed, as the turn follows it; a link that leads into the
    /// workspace is not, since the turn refuses it; and a pipe is never opened.
    #[test]
    fn a_linked_rules_file_is_listed_as_a_turn_reads_it_and_a_pipe_is_not_read() {
        let home = crate::testutil::scratch_dir("recall-tidy-rules-links");
        let _ = std::fs::remove_dir_all(&home);
        let root = home.join("work");
        std::fs::create_dir_all(&root).expect("root");
        let workspace = crate::workspace::Workspace::new(&root).expect("workspace");
        let store = Store::new(&home, project_id(&root, false));
        let dotfiles = home.join("dotfiles");
        std::fs::create_dir_all(&dotfiles).expect("dotfiles");
        let real = dotfiles.join("AGENTS.md");
        std::fs::write(&real, "one\ntwo\n").expect("real");
        // The global file is a link to a real file; the project's is a pipe, which a read would
        // wait on for ever.
        let global = store.rules_path(Scope::Everywhere);
        std::os::unix::fs::symlink(&real, &global).expect("link");
        let project = store.rules_path(Scope::Project);
        std::fs::create_dir_all(project.parent().expect("parent")).expect("directory");
        let made = std::process::Command::new("mkfifo")
            .arg(&project)
            .status()
            .expect("mkfifo");
        assert!(made.success());
        let listed = overview(&home, &workspace, false, None).rules;
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(
            (listed[0].path.as_path(), listed[0].lines),
            (global.as_path(), 2)
        );

        // A link into the workspace is refused by the turn, so it is not listed either.
        std::fs::remove_file(&global).expect("unlink");
        std::fs::write(root.join("planted.md"), "x\n").expect("planted");
        std::os::unix::fs::symlink(root.join("planted.md"), &global).expect("link");
        assert!(overview(&home, &workspace, false, None).rules.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_long_rules_file_is_counted_up_to_a_cap_and_the_working_directorys_own_is_listed() {
        let home = crate::testutil::scratch_dir("recall-tidy-rules-cap");
        let _ = std::fs::remove_dir_all(&home);
        let root = home.join("work");
        std::fs::create_dir_all(&root).expect("root");
        let store = Store::new(&home, project_id(&root, true));
        std::fs::write(
            store.rules_path(Scope::Everywhere),
            "x\n".repeat(RULE_LINES_COUNTED + 5),
        )
        .expect("long");
        std::fs::write(root.join("AGENTS.md"), "a\nb\nc\n").expect("own");
        let workspace = crate::workspace::Workspace::new(&root).expect("workspace");
        let listed = overview(&home, &workspace, true, Some(&root.join("AGENTS.md"))).rules;
        assert_eq!(listed.len(), 2);
        assert_eq!(
            (listed[0].lines, listed[0].more),
            (RULE_LINES_COUNTED, true)
        );
        assert_eq!((listed[1].lines, listed[1].more), (3, false));
        assert_eq!(listed[1].path, root.join("AGENTS.md"));
        // No file chosen, none listed.
        assert_eq!(overview(&home, &workspace, true, None).rules.len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }
}
