//! What a session decides as it opens, about the `allow` rules a checkout proposed and the
//! directories a settings file named, before anything is put to anybody.
//!
//! Every front asks these questions its own way: the terminal draws a box and waits, the window
//! sends an event and takes a reply later. What is asked, what holds without asking and what a yes
//! records are the same everywhere, so they are decided here and a front only puts the question.
//!
//! Each decision comes in two halves, so nothing here waits on an answer: one says what needs
//! asking ([`rules_to_grant`], [`named_directories`] with [`named_to_open`]), and the other applies
//! the answer once it comes ([`RuleQuestion::answer`]).

use crate::granted::{Proposed, Store};
use crate::{PermissionMode, Workspace, WorkspaceError};
use std::path::Path;

/// Where a session's opening trust map came from, which is what it says about the questions after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whence {
    /// The person answered the startup question just now, or answered yes to it in an earlier
    /// session here and the yes was kept (TRUST-23). Either way they are here, and are asked about
    /// whatever else the tree proposes: what the yes kept was that question and no other.
    Asked,
    /// The record of the session being picked up, so the answer is that session's user's own.
    Resumed,
    /// Nobody was asked, because the mode in force answers this question too.
    Unasked,
}

impl Whence {
    /// How a session that moved to another directory under `mode` stands there: asked, since the
    /// person who named it is here, unless the mode answers. Never resumed, since no record holds
    /// an answer for a directory the session was not in.
    pub fn after_a_move(mode: PermissionMode) -> Self {
        if mode_answers(mode) {
            Self::Unasked
        } else {
            Self::Asked
        }
    }
}

/// Whether `mode` answers the opening questions itself, so none is put to anybody.
///
/// Only bypassing does: it approves every write and every run, so stopping at a question about a
/// workspace would be the one thing it did not answer. Auto still asks here.
pub fn mode_answers(mode: PermissionMode) -> bool {
    match mode {
        PermissionMode::Bypass => true,
        PermissionMode::Ask
        | PermissionMode::AcceptEdits
        | PermissionMode::Plan
        | PermissionMode::Auto => false,
    }
}

/// The `allow` rules a checkout's settings files proposed, in the order the files wrote them.
///
/// A rule in one of those layers grants rather than narrows, so it is dropped where it is read
/// ([PERM-14]) and arrives here as a request instead. Only entries that are rules: one that is not
/// decides nothing whoever grants it, so PERM-15 reports it under PERM-11 rather than offering it.
///
/// [PERM-14]: ../../../docs/specs/permissions.md
pub fn proposed_rules(settings: &bravebot_config::Settings) -> Vec<Proposed> {
    crate::permissions::proposed(settings, crate::home::profile().as_deref())
}

/// What becomes of the `allow` rules a checkout proposed.
#[derive(Debug, PartialEq, Eq)]
pub enum Granting {
    /// In force with nothing put to anybody, because something else answered.
    Granting(Vec<Proposed>),
    /// The rules already granted for this workspace, and the rest put to the person as one question.
    Asking {
        already: Vec<Proposed>,
        asking: Vec<Proposed>,
    },
}

/// What becomes of `proposed` for a session that opened this way, given the rules the record
/// already holds.
///
/// - **Asked.** The person answers, bar the rules their own record already holds for this
///   workspace, which they answered before and are not asked about twice ([PERM-13]'s reasoning).
/// - **Unasked.** The mode answers the whole list for the session it is in force for, and nothing
///   is written to the record: a grant on disk would outlive the flag that made it.
/// - **Resumed.** Nothing is put to it, and what holds is what the record already says ([PERM-10]).
///   A rule the checkout has added since is not one anybody granted.
///
/// [PERM-10]: ../../../docs/specs/permissions.md
/// [PERM-13]: ../../../docs/specs/permissions.md
pub fn granting(whence: Whence, proposed: &[Proposed], already: &[Proposed]) -> Granting {
    match whence {
        Whence::Resumed => Granting::Granting(already.to_vec()),
        Whence::Unasked => Granting::Granting(proposed.to_vec()),
        Whence::Asked => Granting::Asking {
            already: already.to_vec(),
            asking: proposed
                .iter()
                .filter(|rule| !already.contains(rule))
                .cloned()
                .collect(),
        },
    }
}

/// The question about a checkout's proposed rules, settled as far as it can be without an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleQuestion {
    /// The proposed rules this workspace's record already holds, which a front names as granted
    /// before. Empty where the record was not read.
    pub held: Vec<Proposed>,
    /// The rules in force whatever the answer.
    pub granted: Vec<Proposed>,
    /// The rules put to the person as one question, empty where there is nothing to ask.
    pub asking: Vec<Proposed>,
}

impl RuleQuestion {
    /// Whether anything is put to the person.
    pub fn needs_asking(&self) -> bool {
        !self.asking.is_empty()
    }

    /// The rules in force once the person answered `yes` or no, recording a yes in `store`.
    ///
    /// A no leaves only what held without asking. A yes is written through [`Store::grant`], which
    /// adds nothing in an incognito session ([`crate::granted::may_be_added_to`]), so there the
    /// rules hold for this session alone.
    pub fn answer(self, yes: bool, store: Option<&Store>, session: &str) -> Vec<Proposed> {
        let mut granted = self.granted;
        if !yes || self.asking.is_empty() {
            return granted;
        }
        if let Some(store) = store {
            store.grant(&self.asking.iter().collect::<Vec<_>>(), session);
        }
        granted.extend(self.asking);
        granted
    }
}

/// What needs asking about `proposed` for a session that opened this way, reading `store`.
///
/// Nothing proposed is nothing to ask and no record to read (PERM-12). No store, or one that cannot
/// be read, is a record that says nothing, and a record that says nothing is a session that asks.
/// The record is not read where the mode answers, since it cannot change the answer.
pub fn rules_to_grant(
    whence: Whence,
    store: Option<&Store>,
    proposed: &[Proposed],
) -> RuleQuestion {
    if proposed.is_empty() {
        return RuleQuestion {
            held: Vec::new(),
            granted: Vec::new(),
            asking: Vec::new(),
        };
    }
    let held: Vec<Proposed> = match (whence, store) {
        (Whence::Unasked, _) | (_, None) => Vec::new(),
        (_, Some(store)) => store.granted(proposed).into_iter().cloned().collect(),
    };
    let (granted, asking) = match granting(whence, proposed, &held) {
        Granting::Granting(granted) => (granted, Vec::new()),
        Granting::Asking { already, asking } => (already, asking),
    };
    RuleQuestion {
        held,
        granted,
        asking,
    }
}

/// The proposed rules that are not in force, which a front says were dropped.
///
/// Matched on the file as well as the text, since two layers may write one rule and only one of
/// them be granted.
pub fn not_granted<'a>(proposed: &'a [Proposed], granted: &[Proposed]) -> Vec<&'a Proposed> {
    proposed
        .iter()
        .filter(|rule| !granted.contains(rule))
        .collect()
}

/// What becomes of the directories a settings file named.
#[derive(Debug, PartialEq, Eq)]
pub enum Named {
    /// Opened with nothing put to anybody.
    Opening(Vec<String>),
    /// Put to the person, one question per directory.
    Asking(Vec<String>),
}

/// What becomes of the directories a settings file named, for a session that opened this way.
///
/// Whoever answered for the working directory answers for these. A resumed session opens none:
/// the directories it has open are the ones its own record reopened, so a name in a file edited
/// since is not a grant that session's user made. Nothing is resolved for it either.
pub fn named_directories(whence: Whence, named: &[String]) -> Named {
    match whence {
        Whence::Asked => Named::Asking(named.to_vec()),
        Whence::Unasked => Named::Opening(named.to_vec()),
        Whence::Resumed => Named::Opening(Vec::new()),
    }
}

/// [`crate::workspace::named_directory`], with a leading `~` expanded against `home` first
/// (TRUST-9). Where no home is known, the name is read as typed, under the working directory.
pub fn named_directory(
    root: &Path,
    typed: &str,
    home: Option<&Path>,
) -> Result<String, WorkspaceError> {
    let expanded = match crate::workspace::expand_home(typed, home) {
        Ok(Some(path)) => path.to_string_lossy().into_owned(),
        Ok(None) | Err(_) => typed.to_string(),
    };
    crate::workspace::named_directory(root, &expanded)
}

/// The directories the names in a settings file would open, resolved before anything is asked.
#[derive(Debug, Default)]
pub struct NamedToOpen {
    /// What each name opens, in the order named, a path two layers both named listed once.
    pub paths: Vec<String>,
    /// Each name that cannot be opened at all, with why, which is said rather than asked about.
    pub refused: Vec<(String, WorkspaceError)>,
}

/// Resolve `named` against `workspace`, so what a person is shown is what they would be granting:
/// a name is canonicalized when it is opened, so `shared` pointing elsewhere shows where it points.
pub fn named_to_open(workspace: &Workspace, named: &[String], home: Option<&Path>) -> NamedToOpen {
    let mut resolved = NamedToOpen::default();
    for directory in named {
        match named_directory(workspace.root(), directory, home)
            .and_then(|candidate| workspace.resolve_directory(&candidate))
        {
            Ok(path) => {
                let shown = path.display().to_string();
                if !resolved.paths.contains(&shown) {
                    resolved.paths.push(shown);
                }
            }
            Err(problem) => resolved.refused.push((directory.clone(), problem)),
        }
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A state directory of this test's own, removed with it.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch state directory");
            Self { path }
        }

        fn store(&self) -> Store {
            Store::new(&self.path, Path::new("/work"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn proposed(rules: &[&str]) -> Vec<Proposed> {
        rules
            .iter()
            .map(|rule| Proposed::new(Path::new("/work/.bravebot/settings.json"), rule))
            .collect()
    }

    /// PERM-15: the person asked about the working directory is asked about the rules too.
    #[test]
    fn a_person_asked_about_the_workspace_is_asked_about_the_rules_a_checkout_proposed() {
        let rules = proposed(&["Bash(bash scripts/check.sh)", "Edit(src/**)"]);

        assert_eq!(
            granting(Whence::Asked, &rules, &[]),
            Granting::Asking {
                already: Vec::new(),
                asking: rules,
            }
        );
    }

    /// PERM-15: and not twice about a rule the record already holds.
    #[test]
    fn a_rule_granted_in_an_earlier_session_is_in_force_rather_than_asked_about_again() {
        let rules = proposed(&["Bash(bash scripts/check.sh)", "Edit(src/**)"]);
        let already = proposed(&["Bash(bash scripts/check.sh)"]);

        assert_eq!(
            granting(Whence::Asked, &rules, &already),
            Granting::Asking {
                already,
                asking: proposed(&["Edit(src/**)"]),
            }
        );
    }

    /// PERM-15: a grant is for the file that proposed it, so the same text in another layer is
    /// still asked about.
    #[test]
    fn one_rule_text_in_two_files_is_granted_for_the_file_it_was_granted_in() {
        let project = Proposed::new(
            Path::new("/work/.bravebot/settings.json"),
            "Bash(bash scripts/check.sh)",
        );
        let local = Proposed::new(
            Path::new("/work/.bravebot/settings.local.json"),
            "Bash(bash scripts/check.sh)",
        );
        let proposed = vec![project.clone(), local.clone()];

        assert_eq!(
            granting(Whence::Asked, &proposed, std::slice::from_ref(&project)),
            Granting::Asking {
                already: vec![project],
                asking: vec![local.clone()],
            }
        );
        assert_eq!(not_granted(&proposed, &proposed[..1]), [&local]);
    }

    /// PERM-15: the mode that answers every question answers this one too.
    #[test]
    fn bypassing_grants_the_rules_a_checkout_proposed_without_asking() {
        let rules = proposed(&["Bash(bash scripts/check.sh)"]);

        assert_eq!(
            granting(Whence::Unasked, &rules, &[]),
            Granting::Granting(rules)
        );
    }

    /// PERM-15: a resume holds only what the record already said.
    #[test]
    fn a_resume_grants_only_the_rules_the_record_already_held() {
        let rules = proposed(&["Bash(bash scripts/check.sh)", "Edit(src/**)"]);
        let already = proposed(&["Bash(bash scripts/check.sh)"]);

        assert_eq!(
            granting(Whence::Resumed, &rules, &already),
            Granting::Granting(already)
        );
    }

    /// PERM-12: nothing proposed is nothing to ask and nothing in force.
    #[test]
    fn nothing_proposed_asks_nothing() {
        let scratch = Scratch::new("opening-nothing-proposed");
        for whence in [Whence::Asked, Whence::Resumed, Whence::Unasked] {
            let question = rules_to_grant(whence, Some(&scratch.store()), &[]);
            assert!(!question.needs_asking());
            assert!(
                question
                    .answer(true, Some(&scratch.store()), "s")
                    .is_empty()
            );
        }
        assert!(!scratch.store().path().exists(), "a record was written");
    }

    /// PERM-15: an ungranted rule is asked about, and a yes grants it and records it, so the next
    /// session in the workspace holds it without asking.
    #[test]
    fn an_ungranted_rule_is_asked_about_and_a_yes_is_recorded() {
        let scratch = Scratch::new("opening-ungranted");
        let rules = proposed(&["Bash(bash scripts/check.sh)"]);

        let question = rules_to_grant(Whence::Asked, Some(&scratch.store()), &rules);
        assert!(question.held.is_empty());
        assert_eq!(question.asking, rules);
        assert_eq!(
            question.answer(true, Some(&scratch.store()), "first"),
            rules
        );

        let again = rules_to_grant(Whence::Asked, Some(&scratch.store()), &rules);
        assert!(
            !again.needs_asking(),
            "a granted rule was asked about again"
        );
        assert_eq!(again.held, rules);
        assert_eq!(again.granted, rules);
    }

    /// PERM-15: a no grants nothing it asked about and records nothing.
    #[test]
    fn a_no_grants_nothing_and_records_nothing() {
        let scratch = Scratch::new("opening-declined");
        let rules = proposed(&["Bash(bash scripts/check.sh)"]);

        let question = rules_to_grant(Whence::Asked, Some(&scratch.store()), &rules);
        assert!(
            question
                .answer(false, Some(&scratch.store()), "s")
                .is_empty()
        );
        assert!(scratch.store().granted(&rules).is_empty());
    }

    /// PERM-15: a rule edited since it was granted is asked about again, and the rest still hold.
    #[test]
    fn an_edited_rule_is_asked_about_again() {
        let scratch = Scratch::new("opening-edited");
        let kept = proposed(&["Edit(src/**)"]);
        let before = proposed(&["Bash(bash scripts/check.sh)"]);
        scratch
            .store()
            .grant(&kept.iter().chain(&before).collect::<Vec<_>>(), "earlier");

        let now = proposed(&["Edit(src/**)", "Bash(bash scripts/deploy.sh)"]);
        let question = rules_to_grant(Whence::Asked, Some(&scratch.store()), &now);
        assert_eq!(question.granted, kept);
        assert_eq!(question.asking, proposed(&["Bash(bash scripts/deploy.sh)"]));
    }

    /// MODE-4: bypassing grants every rule, asks nothing and writes nothing.
    #[test]
    fn bypassing_asks_nothing_and_writes_nothing() {
        let scratch = Scratch::new("opening-bypass");
        let rules = proposed(&["Bash(bash scripts/check.sh)"]);
        let whence = Whence::after_a_move(PermissionMode::Bypass);
        assert_eq!(whence, Whence::Unasked);

        let question = rules_to_grant(whence, Some(&scratch.store()), &rules);
        assert!(!question.needs_asking());
        assert_eq!(question.answer(true, Some(&scratch.store()), "s"), rules);
        assert!(scratch.store().granted(&rules).is_empty());
        assert_eq!(
            named_directories(whence, &["notes".to_string()]),
            Named::Opening(vec!["notes".to_string()])
        );
    }

    /// Auto answers approvals but not the opening questions, which are still put to the person.
    #[test]
    fn auto_still_asks() {
        for mode in [
            PermissionMode::Auto,
            PermissionMode::Ask,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
        ] {
            assert!(!mode_answers(mode));
            assert_eq!(Whence::after_a_move(mode), Whence::Asked);
        }
        let rules = proposed(&["Edit(src/**)"]);
        assert_eq!(
            rules_to_grant(Whence::after_a_move(PermissionMode::Auto), None, &rules).asking,
            rules
        );
    }

    /// No record is a record that says nothing, so the session asks, and a yes still holds.
    #[test]
    fn no_record_asks_and_a_yes_holds_for_the_session() {
        let rules = proposed(&["Edit(src/**)"]);
        let question = rules_to_grant(Whence::Asked, None, &rules);
        assert_eq!(question.asking, rules);
        assert_eq!(question.answer(true, None, "s"), rules);
    }

    /// Each directory a file named is asked about where the person was asked about the workspace.
    #[test]
    fn a_person_asked_about_the_workspace_is_asked_about_each_named_directory() {
        let named = vec!["/home/me/notes".to_string(), "/home/me/.ssh".to_string()];
        assert_eq!(
            named_directories(Whence::Asked, &named),
            Named::Asking(named)
        );
    }

    /// A resume opens no directory a file named.
    #[test]
    fn a_resume_opens_no_directory_a_file_named() {
        assert_eq!(
            named_directories(Whence::Resumed, &["/home/me/notes".to_string()]),
            Named::Opening(Vec::new())
        );
    }

    /// Names are resolved to what they open, listed once, and a name that opens nothing is refused.
    #[test]
    fn named_directories_are_resolved_before_they_are_asked_about() {
        let root = crate::testutil::scratch_dir("opening-named-to-open");
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("project");
        std::fs::create_dir_all(root.join("shared")).expect("scratch");
        std::fs::create_dir_all(&project).expect("scratch");
        let workspace = Workspace::new(&project).expect("workspace");

        let named = [
            "../shared".to_string(),
            root.join("shared").display().to_string(),
            "../missing".to_string(),
        ];
        let resolved = named_to_open(&workspace, &named, None);
        let canonical = root.join("shared").canonicalize().expect("canonical");
        assert_eq!(resolved.paths, [canonical.display().to_string()]);
        assert_eq!(resolved.refused.len(), 1);
        assert_eq!(resolved.refused[0].0, "../missing");

        std::fs::remove_dir_all(&root).ok();
    }
}
