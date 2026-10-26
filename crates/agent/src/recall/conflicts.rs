//! Rules that disagree across layers (RECALL-4).
//!
//! The layers are the ones RECALL-2 names, in the order they win: the home `AGENTS.md`, the
//! person's `rules.md` for the project, and the project's own file. A term-overlap pass picks
//! candidate pairs of lines from different layers, a tool-less request decides whether each pair
//! really disagrees, and each `yes` becomes a notice and a line in the project's `disagreements`
//! file. Nothing is edited.
//!
//! **Every layer is trusted, so the request reads only trusted content.** The layers arrive here
//! as the text the preamble admitted, past the same gates the planner's own copy went through, and
//! the answer goes through the gate [`crate::goal`] puts a verdict through: a reply that comes back
//! quarantined is not read.

use super::rank::term_share;
use super::store::Store;
use super::{RULE_PAIRS_JUDGED, SIMILAR_ABOVE, project::ProjectId};
use crate::home;
use bravebot_aichat::protocol::{ChatRequest, Message};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::Sink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::reference::Presentation;
use bravebot_core::slot::SlotId;
use bravebot_i18n::t;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// One rule line of a layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The layer's file, as the session names it.
    pub file: String,
    /// The line's number in that file, from 1.
    pub number: usize,
    /// The rule, without a leading `- `.
    pub text: String,
}

/// The rule lines of one layer: every line with something on it, but headings, code fences and
/// front-matter fences.
fn lines_of(file: &str, text: &str) -> Vec<Line> {
    text.lines()
        .enumerate()
        .filter_map(|(at, line)| {
            let line = line.trim();
            if line.starts_with('#') || line.starts_with("```") || line.starts_with("---") {
                return None;
            }
            let line = line.strip_prefix("- ").unwrap_or(line).trim();
            (!line.is_empty()).then(|| Line {
                file: file.to_string(),
                number: at + 1,
                text: line.to_string(),
            })
        })
        .collect()
}

/// Lines from different layers whose terms overlap, the earlier layer's line first. A pair is a
/// candidate when either line's share of terms in the other is above [`SIMILAR_ABOVE`]. `layers`
/// are `(file, text)` in the order they win, the last winning.
pub fn candidate_pairs(layers: &[(String, String)]) -> Vec<(Line, Line)> {
    let lines: Vec<Vec<Line>> = layers
        .iter()
        .map(|(file, text)| lines_of(file, text))
        .collect();
    let mut pairs = Vec::new();
    for (at, earlier) in lines.iter().enumerate() {
        for later in &lines[at + 1..] {
            for a in earlier {
                for b in later {
                    if term_share(&a.text, &b.text) > SIMILAR_ABOVE
                        || term_share(&b.text, &a.text) > SIMILAR_ABOVE
                    {
                        pairs.push((a.clone(), b.clone()));
                    }
                }
            }
        }
    }
    // The likeliest clashes first, so the cap on pairs judged cuts the weakest.
    pairs.sort_by_key(|(a, b)| {
        std::cmp::Reverse(term_share(&a.text, &b.text).max(term_share(&b.text, &a.text)))
    });
    pairs
}

/// `sha256  file` for each layer, in the form the `checked` file holds them.
fn hashes(layers: &[(String, String)]) -> BTreeMap<String, String> {
    layers
        .iter()
        .map(|(file, text)| {
            let digest = Sha256::digest(text.as_bytes());
            (
                file.clone(),
                digest.iter().map(|byte| format!("{byte:02x}")).collect(),
            )
        })
        .collect()
}

/// Whether any layer differs from what `checked` recorded. No layers and no record is nothing to
/// check. A record that cannot be read is no record.
pub fn layers_changed(checked: &Path, layers: &[(String, String)]) -> bool {
    let held: BTreeMap<String, String> = std::fs::read_to_string(checked)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once("  "))
        .map(|(hash, file)| (file.to_string(), hash.to_string()))
        .collect();
    hashes(layers) != held
}

/// Record the layers as checked, one `sha256  file` line each, replacing what was there.
pub fn mark_checked(checked: &Path, layers: &[(String, String)]) -> io::Result<()> {
    let body: String = hashes(layers)
        .iter()
        .map(|(file, hash)| format!("{hash}  {file}\n"))
        .collect();
    home::write_atomically(checked, body.as_bytes())
}

/// The disagreements last found, as `(earlier layer's line, later layer's line, winner's file)`.
/// The file holds no rule text, so each [`Line::text`] is empty. Missing or unreadable is none.
pub fn read_disagreements(path: &Path) -> Vec<(Line, Line, String)> {
    fn place(at: &str) -> Option<Line> {
        let (file, number) = at.rsplit_once(':')?;
        Some(Line {
            file: file.to_string(),
            number: number.parse().ok()?,
            text: String::new(),
        })
    }
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut parts = line.split("  ");
            let (first, second, winner) = (parts.next()?, parts.next()?, parts.next()?);
            Some((
                place(first)?,
                place(second)?,
                winner.strip_prefix("winner=")?.to_string(),
            ))
        })
        .collect()
}

/// The request could not be answered, so nothing was decided.
#[derive(Debug, PartialEq, Eq)]
pub struct Unanswerable;

const SYSTEM_PROMPT: &str = "\
You compare two rules that a person wrote for a coding agent, each from a different file. You \
have no tools and nothing else to read. Two rules disagree when following one means breaking the \
other. Rules that merely cover the same subject, or that say the same thing, do not disagree.

Answer with one word on the first line: yes if they disagree, no if they do not.";

/// The verdict in what the judge wrote: the first word of the first line, with anything that is not
/// a letter or digit stripped from both ends of the line. `yes` and `no` are answers and anything
/// else is not, so a reply that cannot be read is retried rather than taken as an agreement.
fn read_answer(answer: &str) -> Result<bool, Unanswerable> {
    let first = answer.trim().lines().next().unwrap_or_default();
    let first = first.trim_matches(|c: char| !c.is_alphanumeric());
    let word = first
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or_default();
    if word.eq_ignore_ascii_case("yes") {
        Ok(true)
    } else if word.eq_ignore_ascii_case("no") {
        Ok(false)
    } else {
        Err(Unanswerable)
    }
}

/// Ask whether `earlier` and `later` disagree, of a model with no tools.
///
/// Sent the way [`crate::goal::assess`] sends its judge, under a policy of its own whose only
/// inputs are the two trusted lines, recording to the session's `sink`, so what the session has taken in does not reach a request that
/// reads only trusted text and cannot quarantine its answer. The answer only picks which fixed
/// notice is shown. It still goes through `present`, as a verdict does.
pub fn ask_whether_they_disagree<S: Sink>(
    sink: &mut S,
    chat: &mut crate::processor::Chat<'_>,
    earlier: &Line,
    later: &Line,
) -> Result<bool, Unanswerable> {
    let model = crate::compact::side_request_model(chat.config, chat.egress, chat.model)
        .map_err(|_| Unanswerable)?;
    let model = model.as_ref();
    let asking = format!(
        "Do these two rules disagree? Answer yes or no. Put the answer on the first line.\n\nRule 1, from {}:\n{}\n\nRule 2, from {}:\n{}",
        earlier.file, earlier.text, later.file, later.text
    );
    let request = ChatRequest::new(
        model,
        vec![Message::system(SYSTEM_PROMPT), Message::user(asking)],
    );
    let mut routing = Routing::new();
    routing.insert_trusted("task", "check whether two rules disagree");
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        sink,
    )
    .map_err(|_| Unanswerable)?;
    let mut client = chat.backend(model);
    let completion = client
        .complete_streaming(&mut policy, &request, |_| {})
        .map_err(|_| Unanswerable)?;
    let written = policy
        .adopt_model_output("rules", completion.content)
        .map_err(|_| Unanswerable)?;
    let mut only_for_this = bravebot_core::slot::SlotStore::new();
    let answer = match policy.present(
        "rules",
        SlotId::new("rules"),
        "a judge's answer on whether two rules disagree",
        &written,
        &mut only_for_this,
    ) {
        Ok(Presentation::Visible(body)) => read_answer(&body),
        _ => Err(Unanswerable),
    };
    // A gate that refused anything leaves no answer to act on.
    match policy.finish() {
        true => answer,
        false => Err(Unanswerable),
    }
}

/// Layers judged to agree for a project that has no directory to record it in, kept for the
/// process: `checked` path to the hashes judged. A check makes no `projects/<id>/` directory only
/// to say nothing was found there (RECALL-1), and without this record every turn would pay for the
/// same requests again.
static AGREED_UNRECORDED: std::sync::Mutex<BTreeMap<std::path::PathBuf, BTreeMap<String, String>>> =
    std::sync::Mutex::new(BTreeMap::new());

/// Check the layers if any differs from the last check, and say what was found.
///
/// `judge` decides whether two lines disagree. Returns the notices to show, none where nothing
/// ran. Does nothing where nothing may be written under the state directory (an incognito
/// session), and nothing with fewer than two layers, which cannot disagree. Where the judge cannot
/// answer, nothing is marked checked, so a later turn tries again, and the session says so once:
/// `said` is whether it already has.
///
/// Only a disagreement makes the project's directory. A check that finds none writes `checked`
/// and clears an old `disagreements` only where the directory is already there, so a turn in a
/// directory leaves no record that the person worked there (RECALL-1).
pub fn check(
    state: &Path,
    project: &ProjectId,
    layers: &[(String, String)],
    said: &AtomicBool,
    judge: &mut dyn FnMut(&Line, &Line) -> Result<bool, Unanswerable>,
) -> Vec<String> {
    if home::writable().is_none() || layers.len() < 2 {
        return Vec::new();
    }
    let store = Store::new(state, project.clone());
    if !layers_changed(&store.checked_path(), layers) {
        return Vec::new();
    }
    let agreed = || {
        AGREED_UNRECORDED
            .lock()
            .ok()
            .and_then(|agreed| agreed.get(&store.checked_path()).cloned())
    };
    if agreed() == Some(hashes(layers)) {
        return Vec::new();
    }
    let pairs = candidate_pairs(layers);
    let mut notices = Vec::new();
    let mut found = String::new();
    for (earlier, later) in pairs.iter().take(RULE_PAIRS_JUDGED) {
        match judge(earlier, later) {
            Ok(true) => {
                notices.push(
                    t!(
                        agent_rules_disagree,
                        first_file = earlier.file.as_str(),
                        first_line = earlier.number,
                        first_rule = earlier.text.as_str(),
                        second_file = later.file.as_str(),
                        second_line = later.number,
                        second_rule = later.text.as_str(),
                        winner = later.file.as_str()
                    )
                    .to_string(),
                );
                found.push_str(&format!(
                    "{}:{}  {}:{}  winner={}\n",
                    earlier.file, earlier.number, later.file, later.number, later.file
                ));
            }
            Ok(false) => {}
            Err(Unanswerable) => {
                if !said.swap(true, Ordering::Relaxed) {
                    notices.push(t!(agent_rules_check_skipped).to_string());
                }
                return notices;
            }
        }
    }
    if pairs.len() > RULE_PAIRS_JUDGED {
        notices.push(t!(agent_rules_check_capped, limit = RULE_PAIRS_JUDGED).to_string());
    }
    if found.is_empty() && !store.has_project_directory() {
        if let Ok(mut agreed) = AGREED_UNRECORDED.lock() {
            agreed.insert(store.checked_path(), hashes(layers));
        }
        return notices;
    }
    let recorded = match found.is_empty() {
        true => match std::fs::remove_file(store.disagreements_path()) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        },
        false => store
            .make_project_directory()
            .and_then(|()| home::write_atomically(&store.disagreements_path(), found.as_bytes())),
    }
    .and_then(|()| mark_checked(&store.checked_path(), layers));
    if recorded.is_err() {
        // Not marked, so the next turn looks again rather than trusting a record that was not made.
        let _ = std::fs::remove_file(store.checked_path());
    }
    notices
}

/// Where the `said` flag lives for a process: a session says the check was skipped once.
pub static SKIPPED_SAID: AtomicBool = AtomicBool::new(false);

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::backend::scripted;
    use bravebot_aichat::Completion;
    use bravebot_core::label::Label;
    use bravebot_core::value::Labelled;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create scratch");
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn project() -> ProjectId {
        ProjectId::named("github.com/o/r")
    }

    fn layer(file: &str, text: &str) -> (String, String) {
        (file.to_string(), text.to_string())
    }

    fn two_layers() -> Vec<(String, String)> {
        vec![
            layer(
                "home/AGENTS.md",
                "- Use tabs for indentation in every file\n",
            ),
            layer("AGENTS.md", "Use spaces for indentation in every file\n"),
        ]
    }

    fn said_yes(_: &Line, _: &Line) -> Result<bool, Unanswerable> {
        Ok(true)
    }

    #[test]
    fn lines_from_one_layer_are_never_paired() {
        let layers = vec![layer(
            "AGENTS.md",
            "Use tabs for indentation\nUse spaces for indentation\n",
        )];
        assert!(candidate_pairs(&layers).is_empty());
    }

    #[test]
    fn similar_lines_from_two_layers_are_candidates() {
        let pairs = candidate_pairs(&two_layers());
        assert_eq!(pairs.len(), 1);
        let (earlier, later) = &pairs[0];
        assert_eq!(
            (earlier.file.as_str(), earlier.number),
            ("home/AGENTS.md", 1)
        );
        assert_eq!(earlier.text, "Use tabs for indentation in every file");
        assert_eq!((later.file.as_str(), later.number), ("AGENTS.md", 1));

        let unrelated = vec![
            layer("a", "Prefer small commits\n"),
            layer("b", "Never touch the database migrations\n"),
        ];
        assert!(candidate_pairs(&unrelated).is_empty());
    }

    #[test]
    fn an_unchanged_set_of_layers_is_not_checked_again() {
        let scratch = Scratch::new("conflicts-unchanged");
        let checked = scratch.0.join("checked");
        let layers = two_layers();
        assert!(layers_changed(&checked, &layers), "never checked");
        mark_checked(&checked, &layers).expect("mark");
        assert!(!layers_changed(&checked, &layers));
        assert!(
            std::fs::read_to_string(&checked)
                .unwrap()
                .lines()
                .all(|line| line
                    .split_once("  ")
                    .is_some_and(|(hash, _)| hash.len() == 64))
        );

        let mut edited = layers.clone();
        edited[1].1.push_str("Keep lines short\n");
        assert!(layers_changed(&checked, &edited));
        assert!(
            !layers_changed(&scratch.0.join("none"), &[]),
            "nothing to check"
        );
    }

    #[test]
    fn a_disagreement_is_noticed_naming_the_winner() {
        let scratch = Scratch::new("conflicts-noticed");
        let said = AtomicBool::new(false);
        let notices = check(&scratch.0, &project(), &two_layers(), &said, &mut said_yes);
        assert_eq!(notices.len(), 1, "{notices:?}");
        let notice = &notices[0];
        for named in ["home/AGENTS.md", "AGENTS.md", "tabs", "spaces", "1"] {
            assert!(notice.contains(named), "{named} missing: {notice}");
        }
        let directory = scratch.0.join("projects").join(&project().dir);
        let found = read_disagreements(&directory.join("disagreements"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.file, "home/AGENTS.md");
        assert_eq!(found[0].1.file, "AGENTS.md");
        assert_eq!(found[0].2, "AGENTS.md", "the later layer wins");
        let mode = std::fs::metadata(directory.join("disagreements"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(!layers_changed(&directory.join("checked"), &two_layers()));
        // Nothing was edited.
        assert!(!scratch.0.join("AGENTS.md").exists());
    }

    #[test]
    fn an_agreement_says_nothing() {
        let scratch = Scratch::new("conflicts-agree");
        let directory = scratch.0.join("projects").join(&project().dir);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("disagreements"), "a:1  b:2  winner=b\n").unwrap();
        let said = AtomicBool::new(false);
        let notices = check(&scratch.0, &project(), &two_layers(), &said, &mut |_, _| {
            Ok(false)
        });
        assert!(notices.is_empty(), "{notices:?}");
        assert!(
            !directory.join("disagreements").exists(),
            "an empty result removes the file"
        );
        assert!(!layers_changed(&directory.join("checked"), &two_layers()));
    }

    /// RECALL-1, RECALL-4: a check that finds nothing makes no project directory, and one that
    /// has fewer than two layers asks nothing and writes nothing. The fault this rejects is a
    /// `projects/<id>/` directory left behind in every directory a turn runs in.
    #[test]
    fn a_check_that_finds_nothing_makes_no_project_directory() {
        let scratch = Scratch::new("conflicts-nothing-made");
        let said = AtomicBool::new(false);
        let mut asked = 0;
        let alone = vec![layer("home/AGENTS.md", "- Use tabs for indentation\n")];
        assert!(
            check(&scratch.0, &project(), &alone, &said, &mut |_, _| {
                asked += 1;
                Ok(true)
            })
            .is_empty()
        );
        let unlike = vec![
            layer("home/AGENTS.md", "- Use tabs for indentation\n"),
            layer("AGENTS.md", "Release on Fridays only\n"),
        ];
        assert!(check(&scratch.0, &project(), &unlike, &said, &mut said_yes).is_empty());
        let agreeing = ProjectId::named("github.com/o/agreeing");
        let mut agree = |_: &Line, _: &Line| {
            asked += 1;
            Ok(false)
        };
        assert!(check(&scratch.0, &agreeing, &two_layers(), &said, &mut agree).is_empty());
        assert!(check(&scratch.0, &agreeing, &two_layers(), &said, &mut agree).is_empty());
        assert_eq!(asked, 1, "an agreement is judged once in a process");
        assert!(
            !scratch.0.join("projects").exists(),
            "a check that found nothing made a project directory"
        );
    }

    #[test]
    fn unchanged_layers_ask_the_judge_nothing() {
        // An incognito session has no writable state directory (tests/incognito.rs holds the
        // process-wide half). Here the layers are unchanged, which asks nothing either.
        let scratch = Scratch::new("conflicts-quiet");
        let directory = scratch.0.join("projects").join(&project().dir);
        std::fs::create_dir_all(&directory).unwrap();
        mark_checked(&directory.join("checked"), &two_layers()).unwrap();
        let mut asked = 0;
        let said = AtomicBool::new(false);
        let notices = check(&scratch.0, &project(), &two_layers(), &said, &mut |_, _| {
            asked += 1;
            Ok(true)
        });
        assert!(notices.is_empty() && asked == 0);
    }

    #[test]
    fn a_check_with_no_answer_is_skipped_and_said_once() {
        let scratch = Scratch::new("conflicts-unanswerable");
        let said = AtomicBool::new(false);
        let mut cannot = |_: &Line, _: &Line| Err(Unanswerable);
        let first = check(&scratch.0, &project(), &two_layers(), &said, &mut cannot);
        assert_eq!(first.len(), 1, "{first:?}");
        let second = check(&scratch.0, &project(), &two_layers(), &said, &mut cannot);
        assert!(second.is_empty(), "{second:?}");
        assert!(
            !scratch.0.join("projects").exists(),
            "an unanswered check records nothing"
        );
    }

    #[test]
    fn the_pairs_judged_in_one_check_are_capped_and_the_notice_says_so() {
        let scratch = Scratch::new("conflicts-capped");
        let many = |tool: &str| {
            (0..RULE_PAIRS_JUDGED + 5)
                .map(|n| format!("Use {tool} for indentation in module{n} files\n"))
                .collect::<String>()
        };
        let layers = vec![layer("a", &many("tabs")), layer("b", &many("spaces"))];
        let mut asked = 0;
        let said = AtomicBool::new(false);
        let notices = check(&scratch.0, &project(), &layers, &said, &mut |_, _| {
            asked += 1;
            Ok(false)
        });
        assert_eq!(asked, RULE_PAIRS_JUDGED);
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(notices[0].contains(&RULE_PAIRS_JUDGED.to_string()));
    }

    fn judged_with_a_trail(answer: &str) -> (Result<bool, Unanswerable>, Vec<String>, String) {
        let config = bravebot_config::Config::from_lookup(|key| match key {
            "SERVICES_KEY_AICHAT" => Some("test-key".into()),
            "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
            "BRAVE_AI_CHAT_ENDPOINT" => Some("http://127.0.0.1:9/never-asked".into()),
            _ => None,
        })
        .expect("configured");
        let egress = bravebot_net::Egress::new();
        let mut sink = bravebot_core::event::RecordingSink::new();
        let pair = candidate_pairs(&two_layers()).remove(0);
        scripted::script(vec![Ok(Completion {
            content: Labelled::new(answer.to_string(), Label::untrusted_public()),
            model: "test-model".into(),
            calls: Vec::new(),
            context_tokens: 10,
            usage: Default::default(),
            cut_off: None,
        })]);
        let mut chat =
            crate::processor::Chat::new(&config, &egress, &crate::chatgpt::AccountSession::begin());
        let said = ask_whether_they_disagree(&mut sink, &mut chat, &pair.0, &pair.1);
        let asked = scripted::asked();
        scripted::clear();
        (said, asked, format!("{:?}", sink.events()))
    }

    fn judged_by_the_real_request(answer: &str) -> (Result<bool, Unanswerable>, Vec<String>) {
        let (said, asked, _) = judged_with_a_trail(answer);
        (said, asked)
    }

    #[test]
    fn the_judges_gates_are_recorded_in_the_trail_it_is_given() {
        let (said, _, trail) = judged_with_a_trail("yes");
        assert_eq!(said, Ok(true));
        assert!(
            trail.contains("rules"),
            "nothing reached the session's trail: {trail}"
        );
    }

    #[test]
    fn the_judging_request_has_no_tools_and_reads_a_yes() {
        let (said, asked) = judged_by_the_real_request("Yes.");
        assert_eq!(said, Ok(true));
        assert_eq!(asked.len(), 1);
        assert!(asked[0].contains("Do these two rules disagree? Answer yes or no."));
        assert!(asked[0].contains("tabs") && asked[0].contains("spaces"));
        let request: serde_json::Value = serde_json::from_str(&asked[0]).unwrap();
        assert!(
            request
                .get("tools")
                .is_none_or(|tools| tools.as_array().is_none_or(Vec::is_empty)),
            "{request}"
        );
        assert_eq!(judged_by_the_real_request("no").0, Ok(false));
        for yes in ["Yes, they clash", "**Yes**", "Yes\n\nreason", "\"yes\""] {
            assert_eq!(judged_by_the_real_request(yes).0, Ok(true), "{yes}");
        }
        assert_eq!(judged_by_the_real_request("**No**").0, Ok(false));
        assert_eq!(judged_by_the_real_request("maybe").0, Err(Unanswerable));
    }

    #[test]
    fn a_removed_layer_counts_as_a_change() {
        let scratch = Scratch::new("conflicts-removed");
        let checked = scratch.0.join("checked");
        mark_checked(&checked, &two_layers()).unwrap();
        assert!(layers_changed(&checked, &two_layers()[..1]));
    }

    #[test]
    fn headings_and_fences_are_not_rules() {
        let layers = vec![
            layer("a", "# Use tabs for indentation\n```\n---\n"),
            layer("b", "Use spaces for indentation\n"),
        ];
        assert!(candidate_pairs(&layers).is_empty());
    }

    #[test]
    fn the_likeliest_pairs_come_first() {
        let layers = vec![
            layer(
                "a",
                "Use tabs for indentation in files\nUse tabs for indentation in every single file\n",
            ),
            layer("b", "Use spaces for indentation in every single file\n"),
        ];
        let pairs = candidate_pairs(&layers);
        assert_eq!(pairs[0].0.number, 2, "{pairs:?}");
    }
}
