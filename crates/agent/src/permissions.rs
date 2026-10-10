//! Reading the `permissions` block out of the settings file.
//!
//! The kernel owns the rule language and this owns finding the file, because reading a rule needs
//! to know where `~` and the settings file are, which is I/O the kernel does not do. What crosses
//! the boundary is rule text and two directory names.
//!
//! # Why the anchors are what they are
//!
//! A `~/` rule points at the user's home directory. A `/` rule points at the directory the
//! settings file sits in, which is `~/.bravebot`: Claude Code anchors a single leading slash at
//! the settings source rather than at the filesystem root, so a rule written in a user-level file
//! means something inside that file's own directory. That is a trap worth reproducing rather than
//! improving on, because somebody who has read those docs will write `//` when they mean the root,
//! and quietly meaning something else here would be worse than agreeing.

use crate::granted::Proposed;
use bravebot_config::{PermissionLists, Settings};
use bravebot_core::permissions::{Anchors, Permissions, Rejected, Rule, Unreadable};
use bravebot_i18n::t;

/// The rules a settings file carried, and any of its lines that were not rules.
///
/// The rejects are returned rather than logged so a caller can report them where a person will
/// read them. A rule nobody can act on is worth saying out loud: a misspelled deny rule reads as
/// protection that is not there.
/// `profile` is the user's home directory ([`crate::home::profile`]) and never the state
/// directory: [`anchors`] joins `.bravebot` onto it itself to reach the settings file, and a
/// caller that passed the state directory would anchor a `~/` rule one segment too deep and a
/// `/` rule two.
/// `workspace` is the directory the session works in, whose volume answers whether two spellings of
/// a path that differ only in case are one file ([`crate::workspace::volume_folds_case`]).
pub fn from_settings(
    settings: &Settings,
    profile: Option<&std::path::Path>,
    workspace: &std::path::Path,
) -> (Permissions, Vec<Rejected>) {
    with_granted(settings, &[], profile, workspace)
}

/// The entries the settings layer could not hand over as rule text, as rejects.
///
/// An entry that is not a line never reaches [`Permissions::parse`], so the list that call builds
/// cannot hold it, and this is where the two halves of one report are put back together. Here
/// rather than in the settings crate because `Rejected` belongs to the kernel, which layering.md
/// puts above the crate that reads the file.
fn entries_that_are_not_lines(lists: &PermissionLists) -> impl Iterator<Item = Rejected> + '_ {
    lists
        .unreadable
        .iter()
        .map(|text| Rejected::not_a_line(text))
}

/// The `permissions` blocks and rule lists a layer spelled as another shape, as rejects that name
/// the file each came from.
///
/// None of them set or removed a rule, so nothing in [`Settings::permissions`] holds them: the
/// weaker layers' lists stand in their place, and only the layer that wrote one can say it was
/// ignored ([PERM-11]).
///
/// [PERM-11]: ../../../docs/specs/permissions.md
fn values_that_are_not_lists(settings: &Settings) -> impl Iterator<Item = Rejected> + '_ {
    settings
        .misshapen_rule_lists()
        .map(|(path, text)| Rejected::not_a_list(text, &path.display().to_string()))
}

/// One dropped entry as the person reading about it sees it, in their own language.
///
/// The kernel names what was wrong and this says it, because a person reads it and what a person
/// reads comes from a catalog (LOCALE-1), which the kernel neither holds nor prints through. Every
/// reporting site goes through here, so `doctor` and a session cannot come to word it differently.
///
/// The entry is quoted in the spelling the file used: a line of three spaces and a line of none
/// are two entries somebody has to find, and a report that trimmed both names neither.
pub fn describe(rejected: &Rejected) -> String {
    let problem = problem(rejected.reason);
    match &rejected.file {
        Some(path) => t!(
            permission_rule_unreadable_in,
            rule = &rejected.text,
            path = path,
            problem = problem
        ),
        None => t!(
            permission_rule_unreadable,
            rule = &rejected.text,
            problem = problem
        ),
    }
}

/// Why an entry of one of the four filesystem lists is not in force, in the words `doctor` and a
/// window share.
pub fn filesystem_reason(reason: bravebot_sandbox::rules::Reason) -> &'static str {
    use bravebot_sandbox::rules::Reason;
    match reason {
        Reason::NoHome => t!(sandbox_rule_no_home),
        Reason::Climbs => t!(sandbox_rule_climbs),
        Reason::GlobOnAWrite => t!(sandbox_rule_glob_on_a_write),
        Reason::ConfinesNothing => t!(sandbox_rule_confines_nothing),
        Reason::PrivateKey => t!(sandbox_rule_private_key),
        Reason::StateDirectory => t!(sandbox_rule_state_directory),
        Reason::TooBroad => t!(sandbox_rule_too_broad),
        Reason::Overridden => t!(sandbox_rule_overridden),
    }
}

/// Every entry of the four filesystem lists with why it is not in force, or `None` where it is.
///
/// An entry that resolved is still not in force where the backend cannot subtract from a grant and
/// the entry is a denial beneath `directory`, which every stage is granted, or beneath an allowance
/// of the person's: each such stage is refused. `capabilities` is `None` where no backend is available, and then nothing starts at all.
pub fn filesystem_standing<'a>(
    rules: &'a bravebot_sandbox::rules::Rules,
    directory: &std::path::Path,
    capabilities: Option<&bravebot_sandbox::policy::Capabilities>,
) -> Vec<(&'a bravebot_sandbox::rules::Item, Option<&'static str>)> {
    use bravebot_sandbox::rules::State;
    let unsubtractable = match capabilities {
        Some(capabilities) if !capabilities.subtracts_from_a_grant => {
            rules.denials_beneath_a_grant(directory)
        }
        _ => Vec::new(),
    };
    rules
        .items()
        .iter()
        .map(|item| {
            let why = match item.state {
                State::Refused(reason) => Some(filesystem_reason(reason)),
                State::InForce(_)
                    if unsubtractable.iter().any(|held| std::ptr::eq(*held, item)) =>
                {
                    Some(t!(sandbox_rule_cannot_subtract))
                }
                State::InForce(_) => None,
            };
            (item, why)
        })
        .collect()
}

/// What was wrong, as a clause to follow the entry.
///
/// One arm per reason and no catch-all, so a reason added to the kernel does not compile until it
/// has words: a rule silently reported as nothing is the drop PERM-11 exists to prevent.
fn problem(reason: Unreadable) -> &'static str {
    match reason {
        Unreadable::NotALine => t!(permission_rule_not_a_line),
        Unreadable::Empty => t!(permission_rule_empty),
        Unreadable::UnclosedBracket => t!(permission_rule_unclosed_bracket),
        Unreadable::UnknownFamily => t!(permission_rule_unknown_family),
        Unreadable::EmptyBrackets => t!(permission_rule_empty_brackets),
        Unreadable::Unanchored => t!(permission_rule_unanchored),
        Unreadable::NotADomainRule => t!(permission_rule_not_a_domain_rule),
        Unreadable::NoDomainNamed => t!(permission_rule_no_domain_named),
        Unreadable::NotAToolRule => t!(permission_rule_not_a_tool_rule),
        Unreadable::NotAList => t!(permission_rule_not_a_list),
    }
}

/// The same rules for a run with nobody at it, which is every list but the one that allows.
///
/// An allow rule answers a prompt in advance. Where nobody can be asked there is no prompt for it
/// to answer, so honouring one would not be saving somebody a keystroke: it would be letting a
/// line in a settings file write, run a program or fetch a URL in a run nobody is watching, beside
/// the command-line flag that is meant to be the only way that happens.
///
/// The other two carry over, because both still say something such a run can act on. A deny rule
/// refuses before there is anything to prompt about, and an ask rule turns a write that would have
/// gone through silently into one there is nobody to approve.
/// `profile` is the user's home directory, for the reason [`from_settings`] states, and `workspace`
/// is the directory whose volume decides whether case is folded.
pub fn for_an_unattended_run(
    settings: &Settings,
    profile: Option<&std::path::Path>,
    workspace: &std::path::Path,
) -> (Permissions, Vec<Rejected>) {
    let lists = settings.permissions();
    let anchors = anchors(profile, crate::workspace::volume_folds_case(workspace));
    let (mut permissions, mut rejected) =
        Permissions::parse(&lists.deny, &lists.ask, &[], &anchors);
    follow_links(&mut permissions, workspace);
    // Read for its rejects and then dropped, rather than not read at all. A line the person
    // believes is in force and that nothing can act on is worth saying out loud wherever it was
    // written, and an allow rule that is silently unreadable here reads to them as one that holds.
    let (_, unreadable) = Permissions::parse(&[], &[], &lists.allow, &anchors);
    rejected.extend(unreadable);
    rejected.extend(entries_that_are_not_lines(lists));
    rejected.extend(values_that_are_not_lists(settings));
    // And the entries a layer that cannot grant wrote, for the same reason again: an unreadable
    // one is not a rule anybody could have granted, so it is named here rather than left to the
    // report about grants (PERM-14) that this run has none of.
    let (_, unreadable) = dropped_allow_entries(settings, &anchors);
    rejected.extend(unreadable);
    (permissions, rejected)
}

/// What a leading `~` and a leading `/` in a rule are resolved against.
///
/// Both come from the user's home directory, the settings one by joining `.bravebot` onto it. So
/// what this takes is the home directory itself and not the state directory that sits inside it
/// (PERM-3), which is the same distinction a `~` in a command line turns on (CMDLINE-4).
fn anchors(profile: Option<&std::path::Path>, folds_case: bool) -> Anchors {
    let home = profile.map(|profile| profile.display().to_string());
    Anchors {
        // The settings file lives in the global state directory, so a `/` rule is anchored there.
        settings_dir: home.as_ref().map(|home| format!("{home}/.bravebot")),
        home,
        // The host's answer, which this crate is the lowest one that may ask for: the kernel has
        // no filesystem and takes it as data (PERM-3).
        backslash_separates: crate::workspace::BACKSLASH_SEPARATES,
        // Asked of the workspace's volume by the caller and false where that could not be told, so
        // a failed probe compares bytes and never widens a rule.
        folds_case,
    }
}

/// Have the `deny` and `ask` path rules cover the file each one's spelling reaches (PERM-7).
///
/// The links are followed as the rules are read, so a link made or repointed later in the session
/// is not followed until they are read again (PERM-12). Where `workspace` cannot be resolved, a
/// rule about the workspace is left as written. So is a rule whose spelling passes through no link:
/// a full path to a project file names it in the other namespace, which PERM-3 keeps apart.
fn follow_links(permissions: &mut Permissions, workspace: &std::path::Path) {
    let root = workspace.canonicalize().ok();
    permissions.follow_links(|prefix| {
        let named = match bravebot_core::trust::is_absolute_key(prefix) {
            true => std::path::PathBuf::from(
                host_spelling(prefix, crate::workspace::BACKSLASH_SEPARATES).as_ref(),
            ),
            false => root.as_ref()?.join(prefix),
        };
        let through_a_link = named.ancestors().any(|place| {
            place
                .symlink_metadata()
                .is_ok_and(|entry| entry.is_symlink())
        });
        if !through_a_link {
            return None;
        }
        let reached = crate::workspace::destination(&named)?;
        // Named as a gate holds it: relative to the root for a file in the project, in full for one
        // outside (PERM-3).
        let held = match root.as_ref().map(|root| reached.strip_prefix(root)) {
            Some(Ok(relative)) => relative.to_path_buf(),
            _ => reached,
        };
        Some(held.to_string_lossy().into_owned())
    });
}

/// A full path keyed from `/` spelled as the host opens it: `/C:/x` is `C:/x` where a backslash
/// separates, and every other key is already a name the host opens. `/C:` is `C:/`, since `C:` alone
/// is wherever the process last was on that drive.
fn host_spelling(key: &str, backslash_separates: bool) -> std::borrow::Cow<'_, str> {
    let from_the_drive = key.strip_prefix('/').filter(
        |rest| matches!(rest.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic()),
    );
    match from_the_drive {
        Some(drive) if backslash_separates && drive.len() == 2 => format!("{drive}/").into(),
        Some(rest) if backslash_separates => rest.into(),
        _ => key.into(),
    }
}

/// The rules a checkout proposed and a person granted, added to the ones a settings file could
/// write on its own.
///
/// [`from_settings`] is every rule that took effect on being read, which is `deny` and `ask` from
/// every layer and `allow` from the person's own file alone ([PERM-14]). A checkout's `allow` entry
/// is dropped there and reported, and this is the other half of the route: an entry the person was
/// shown and accepted is a rule they wrote, so it is parsed with the rest and decides what any of
/// them decides.
///
/// `granted` is the rule text as the file spelled it, which is what the question showed. So a rule
/// nobody can read is dropped and reported here exactly as one in the person's own file is
/// ([PERM-11]): granting it was granting a line, and what a line means is still the rule language's
/// to say.
///
/// Takes the text rather than a [`crate::granted::Proposed`] because what reaches the rule parser is
/// a line: which file proposed it decided whether to ask, and that question is answered by the time
/// this is called. `profile` is the user's home directory, for the reason [`from_settings`] states,
/// and `workspace` is the directory whose volume decides whether case is folded.
///
/// [PERM-11]: ../../../docs/specs/permissions.md
/// [PERM-14]: ../../../docs/specs/permissions.md
pub fn with_granted(
    settings: &Settings,
    granted: &[String],
    profile: Option<&std::path::Path>,
    workspace: &std::path::Path,
) -> (Permissions, Vec<Rejected>) {
    let lists = settings.permissions();
    // Appended rather than prepended, and it decides nothing either way: PERM-2 puts `deny` before
    // `ask` before `allow` and the first match in a list wins, so a granted rule cannot outrank a
    // narrowing one however the list is ordered. The order the person read them in is the order they
    // were proposed in, which is the order to report a bad one in.
    let allow: Vec<String> = lists.allow.iter().chain(granted).cloned().collect();
    let anchors = anchors(profile, crate::workspace::volume_folds_case(workspace));
    let (mut permissions, mut rejected) =
        Permissions::parse(&lists.deny, &lists.ask, &allow, &anchors);
    follow_links(&mut permissions, workspace);
    rejected.extend(entries_that_are_not_lines(lists));
    rejected.extend(values_that_are_not_lists(settings));
    let (_, unreadable) = dropped_allow_entries(settings, &anchors);
    rejected.extend(unreadable);
    (permissions, rejected)
}

/// The `allow` entries a layer that could not grant them wrote, as the rules among them and the
/// entries that are not rules at all.
///
/// Both halves are reported and under different clauses. A rule is [PERM-14]'s: dropped, named
/// with the file it was written in, and put to the person at the question [PERM-15] describes. An
/// entry that is not a rule is [PERM-11]'s whatever layer wrote it, and is not also reported as a
/// grant that was withheld, because a rule nothing can act on is nobody's grant: being told to
/// grant a line that can never decide anything sends somebody to the wrong fix, where the same
/// line in their own file is named for what is wrong with it.
///
/// Split here rather than in the settings crate for [`entries_that_are_not_lines`]'s reason: what
/// a line means is the kernel's to say, and layering.md puts the kernel above the crate that reads
/// the file.
///
/// [PERM-11]: ../../../docs/specs/permissions.md
/// [PERM-14]: ../../../docs/specs/permissions.md
/// [PERM-15]: ../../../docs/specs/permissions.md
fn dropped_allow_entries(settings: &Settings, anchors: &Anchors) -> (Vec<Proposed>, Vec<Rejected>) {
    let mut proposed = Vec::new();
    let mut rejected = Vec::new();
    for (path, rule) in settings.allow_ignored() {
        match Rule::parse(rule, anchors) {
            Ok(_) => proposed.push(Proposed::new(path, rule)),
            Err(reject) => rejected.push(reject),
        }
    }
    (proposed, rejected)
}

/// The `allow` rules a checkout proposed: every dropped entry that is a rule at all.
///
/// What the question [PERM-15] describes offers, and what [PERM-14] reports as dropped or as
/// granted. The entries left out of it are reported as unreadable instead, by whichever of
/// [`from_settings`], [`with_granted`] and [`for_an_unattended_run`] the same caller builds its
/// rules with, so no surface has to remember the other half of the report.
///
/// `profile` is the user's home directory, for the reason [`from_settings`] states.
///
/// [PERM-14]: ../../../docs/specs/permissions.md
/// [PERM-15]: ../../../docs/specs/permissions.md
pub fn proposed(settings: &Settings, profile: Option<&std::path::Path>) -> Vec<Proposed> {
    dropped_allow_entries(settings, &anchors(profile, false)).0
}

/// The directories a settings file asked to have opened, in the order it named them.
///
/// Names only, and a name is a request: a file that arrived with a checkout must not be able to
/// make a path outside the project reachable and vouched for. Putting each to the person and
/// opening what they accept is the caller's to do, through the same path `/add-dir` takes, so that
/// a directory a file named and a directory a person typed are reachable on identical terms and
/// neither has a route the other lacks.
pub fn additional_directories(settings: &Settings) -> &[String] {
    &settings.permissions().additional_directories
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::permissions::{Decision, Ruling, Subject};
    use std::path::{Path, PathBuf};

    /// A backend that reports whether it can hold a path back from a directory it grants.
    fn backend(subtracts_from_a_grant: bool) -> bravebot_sandbox::policy::Capabilities {
        bravebot_sandbox::policy::Capabilities {
            level: bravebot_sandbox::policy::ConfinementLevel::Partial,
            mechanisms: vec!["a mechanism"],
            network_denial_enforced: true,
            grants_paths_that_do_not_exist: false,
            subtracts_from_a_grant,
        }
    }

    /// A denial inside the session directory is not in force where the backend cannot hold it
    /// back, since every stage there is refused. Where it can, and for every entry the backend
    /// has no part in, the answer is what the entry resolved to.
    #[test]
    fn a_denial_the_backend_cannot_subtract_is_not_in_force_and_nothing_else_changes() {
        use bravebot_sandbox::rules::{Entry, Lists, resolve};
        let root = crate::testutil::scratch_dir("Permissions-Cannot-Subtract");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("inside")).unwrap();
        std::fs::create_dir_all(root.join("beside")).unwrap();
        let root = root.canonicalize().unwrap();
        let session = root.join("inside");
        let entry = |path: &str| Entry {
            path: path.to_string(),
            by: None,
            pinned: false,
        };
        let beside = root.join("beside").join("secret.env");
        let beside = beside.to_str().unwrap();
        let lists = Lists {
            allow_read: vec![entry("notes.txt")],
            deny_read: vec![entry("secret.env"), entry(beside)],
            deny_write: vec![entry("secret.env"), entry("../../outside-the-root")],
            ..Lists::default()
        };
        let rules = resolve(&lists, None, &session);
        let cannot = t!(sandbox_rule_cannot_subtract);
        let climbs = filesystem_reason(bravebot_sandbox::rules::Reason::Climbs);
        let standing = |capabilities: Option<&bravebot_sandbox::policy::Capabilities>| {
            filesystem_standing(&rules, &session, capabilities)
                .into_iter()
                .map(|(item, why)| (item.list.key(), item.entry.path.as_str(), why))
                .collect::<Vec<_>>()
        };

        assert_eq!(
            standing(Some(&backend(false))),
            [
                ("allowRead", "notes.txt", None),
                ("denyRead", "secret.env", Some(cannot)),
                ("denyRead", beside, None),
                ("denyWrite", "secret.env", Some(cannot)),
                ("denyWrite", "../../outside-the-root", Some(climbs)),
            ]
        );
        for held in [standing(Some(&backend(true))), standing(None)] {
            assert_eq!(
                held,
                [
                    ("allowRead", "notes.txt", None),
                    ("denyRead", "secret.env", None),
                    ("denyRead", beside, None),
                    ("denyWrite", "secret.env", None),
                    ("denyWrite", "../../outside-the-root", Some(climbs)),
                ]
            );
        }
    }

    /// The denial is judged against the allowances too: one beneath an `allowWrite` entry is not in
    /// force where the backend cannot subtract, though the session directory is somewhere else.
    #[test]
    fn a_denial_beneath_an_allowance_is_not_in_force_where_the_backend_cannot_subtract() {
        use bravebot_sandbox::rules::{Entry, Lists, resolve};
        let root = crate::testutil::scratch_dir("Permissions-Cannot-Subtract-Allowance");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("inside")).unwrap();
        std::fs::create_dir_all(root.join("shared")).unwrap();
        let root = root.canonicalize().unwrap();
        let session = root.join("inside");
        let shared = root.join("shared");
        let secret = shared.join("secret.env");
        let entry = |path: &std::path::Path| Entry {
            path: path.to_str().unwrap().to_string(),
            by: None,
            pinned: false,
        };
        let lists = Lists {
            allow_write: vec![entry(&shared)],
            deny_write: vec![entry(&secret)],
            ..Lists::default()
        };
        let rules = resolve(&lists, None, &session);
        let why = |capabilities: &bravebot_sandbox::policy::Capabilities| {
            filesystem_standing(&rules, &session, Some(capabilities))
                .into_iter()
                .map(|(item, why)| (item.list.key(), why))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            why(&backend(false)),
            [
                ("allowWrite", None),
                ("denyWrite", Some(t!(sandbox_rule_cannot_subtract)))
            ]
        );
        assert_eq!(
            why(&backend(true)),
            [("allowWrite", None), ("denyWrite", None)]
        );
    }

    /// The rules take the answer of the volume the workspace is on, and a workspace that cannot be
    /// read gets byte comparison: a rule about `Docs` covers `docs` only where the two are one file.
    #[test]
    fn rules_take_the_case_answer_of_the_volume_the_workspace_is_on() {
        let root = crate::testutil::scratch_dir("Permissions-Volume-Case");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        // The oracle asks the same filesystem the other way, with a file of the test's own.
        std::fs::write(root.join("probe-file"), b"").unwrap();
        let folds = root.join("PROBE-FILE").exists();

        let settings = Settings::parse(r#"{"permissions": {"deny": ["Read(Docs/**)"]}}"#);
        let (on_it, _) = from_settings(&settings, None, &root);
        let (nowhere, _) = from_settings(&settings, None, &root.join("does-not-exist"));

        let expected = match folds {
            true => Decision::Ruled(Ruling::Deny),
            false => Decision::Unmatched,
        };
        assert_eq!(on_it.for_path(Subject::Read, "docs/a.md"), expected);
        assert_eq!(
            nowhere.for_path(Subject::Read, "docs/a.md"),
            Decision::Unmatched,
            "a workspace that could not be read folded case"
        );
        assert_eq!(
            on_it.for_path(Subject::Read, "Docs/a.md"),
            Decision::Ruled(Ruling::Deny)
        );
    }

    /// The block from Claude Code's own documentation, read out of a settings file and into rules
    /// that decide something. This is the whole point of the module.
    #[test]
    fn a_settings_file_block_becomes_rules_that_decide() {
        let settings = Settings::parse(
            r#"{
              "permissions": {
                "allow": ["Bash(git diff *)"],
                "ask": ["Bash(git push *)"],
                "deny": ["Read(./.env)"]
              }
            }"#,
        );
        let (permissions, rejected) = from_settings(
            &settings,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert!(rejected.is_empty());
        assert_eq!(
            permissions.for_command(&["git", "diff", "--stat"]),
            Decision::Ruled(Ruling::Allow)
        );
        assert_eq!(
            permissions.for_command(&["git", "push", "origin", "main"]),
            Decision::Ruled(Ruling::Ask)
        );
        assert_eq!(
            permissions.for_path(Subject::Read, ".env"),
            Decision::Ruled(Ruling::Deny)
        );
    }

    /// No settings file means no rules, which is the state every session was in before this
    /// existed and must stay indistinguishable from it.
    #[test]
    fn no_block_is_no_rules() {
        let (permissions, rejected) =
            from_settings(&Settings::default(), None, Path::new("/nonexistent"));
        assert!(permissions.is_empty());
        assert!(rejected.is_empty());
    }

    /// A line that is not a rule is handed back rather than dropped in silence, so a person can be
    /// told which of their rules is doing nothing.
    #[test]
    fn a_line_that_is_not_a_rule_is_reported() {
        let settings = Settings::parse(r#"{"permissions": {"deny": ["Read(.env)", "Nonsense"]}}"#);
        let (permissions, rejected) = from_settings(
            &settings,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert_eq!(permissions.len(), 1);
        assert_eq!(rejected.len(), 1);
        assert!(describe(&rejected[0]).contains("Nonsense"));
    }

    /// A run with nobody at it keeps the two lists it can act on and loses the one that answers a
    /// prompt, because there is no prompt for that one to answer.
    #[test]
    fn a_run_nobody_is_watching_keeps_every_rule_but_the_ones_that_allow() {
        let settings = Settings::parse(
            r#"{
              "permissions": {
                "allow": ["Bash(git diff *)"],
                "ask": ["Bash(git push *)"],
                "deny": ["Read(./.env)"]
              }
            }"#,
        );
        let (permissions, rejected) = for_an_unattended_run(
            &settings,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert!(rejected.is_empty());
        assert_eq!(
            permissions.for_command(&["git", "diff", "--stat"]),
            Decision::Unmatched
        );
        assert_eq!(
            permissions.for_command(&["git", "push", "origin", "main"]),
            Decision::Ruled(Ruling::Ask)
        );
        assert_eq!(
            permissions.for_path(Subject::Read, ".env"),
            Decision::Ruled(Ruling::Deny)
        );
    }

    /// An allow rule that cannot be read is still named, though nothing would have acted on it
    /// here. A rule reported nowhere reads to the person who wrote it as one that is in force, and
    /// the same file is read by a session where it decides something.
    #[test]
    fn an_unreadable_allow_rule_is_reported_to_a_run_nobody_is_watching() {
        let settings = Settings::parse(r#"{"permissions": {"allow": ["Bash(git diff *"]}}"#);
        let (permissions, rejected) = for_an_unattended_run(
            &settings,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert!(permissions.is_empty());
        assert_eq!(rejected.len(), 1);
        assert!(describe(&rejected[0]).contains("git diff"));
    }

    /// A deny rule nested one array too deep, which is the ordinary way this key is mistyped. It
    /// is not a line, so the rule parser never sees it and cannot be the thing that names it: what
    /// the settings layer could not hand over has to arrive in the same report, or the person is
    /// told the file carries no rules while they believe `.env` is denied.
    #[test]
    fn an_entry_that_is_not_a_line_is_reported() {
        let settings =
            Settings::parse(r#"{"permissions": {"deny": [["Read(./.env)"], "Read(./notes)"]}}"#);
        let (permissions, rejected) = from_settings(
            &settings,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert_eq!(permissions.len(), 1);
        assert_eq!(
            rejected.iter().map(describe).collect::<Vec<_>>(),
            [r#"'["Read(./.env)"]' is not a rule; a rule is written as a line of text"#]
        );
        // What the dropped rule was meant to stop, still not stopped. The report is the whole of
        // what stands between that and somebody believing otherwise.
        assert_eq!(
            permissions.for_path(Subject::Read, ".env"),
            Decision::Unmatched
        );
    }

    /// A run with nobody at it reads the same file and reports the same entry. Its rules are built
    /// from two passes rather than one, so an entry that belongs to neither pass is the one a fix
    /// made in the first of them would lose.
    #[test]
    fn an_entry_that_is_not_a_line_is_reported_to_a_run_nobody_is_watching() {
        let settings = Settings::parse(r#"{"permissions": {"allow": [["Bash(git diff *)"]]}}"#);
        let (permissions, rejected) = for_an_unattended_run(
            &settings,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert!(permissions.is_empty());
        assert_eq!(rejected.len(), 1);
        assert!(describe(&rejected[0]).contains("Bash(git diff *)"));
    }

    /// A checkout's `permissions` block that is not an object sets no rule and takes none of the
    /// person's away, and is named with its file and a reason of its own. Every layer can spell the
    /// same block, and the advice for an entry in a list, to write it as a line, is the wrong fix.
    /// Both callers, because each builds its rejects on its own.
    #[test]
    fn a_rule_list_that_is_not_a_list_is_reported_with_its_file() {
        let settings = layered_settings(
            "not-a-list",
            Some(r#"{"permissions": null}"#),
            Some(r#"{"permissions": {"deny": ["Read(./.env)"]}}"#),
        );
        let file = std::path::Path::new(".bravebot").join("settings.json");
        let reason = r#"sets no rules and removes none; rules go in a list, such as "deny": ["Read(./.env)"]"#;
        for (caller, (permissions, rejected)) in [
            (
                "a session",
                from_settings(
                    &settings,
                    Some(&PathBuf::from("/home/x")),
                    std::path::Path::new("."),
                ),
            ),
            (
                "a run nobody is watching",
                for_an_unattended_run(
                    &settings,
                    Some(&PathBuf::from("/home/x")),
                    std::path::Path::new("."),
                ),
            ),
        ] {
            assert_eq!(
                permissions.for_path(Subject::Read, ".env"),
                Decision::Ruled(Ruling::Deny),
                "{caller} lost the person's own deny rule"
            );
            let said: Vec<String> = rejected.iter().map(describe).collect();
            assert_eq!(said.len(), 1, "{caller}: {said:?}");
            assert!(
                said[0].starts_with(r#"'{"permissions":null}' in "#),
                "{caller}: {}",
                said[0]
            );
            assert!(
                said[0].ends_with(&format!("{} {reason}", file.display())),
                "{caller}: {}",
                said[0]
            );
        }
    }

    /// Blank text is a line, so it reaches the rule parser and comes back with the parser's own
    /// word for it. A filter over the list is what stopped that rejection ever being reached.
    ///
    /// Reported in the spelling the file used, which for a blank line is the whole of what
    /// distinguishes it: two blank entries reported as `''` are two lines a person cannot find.
    #[test]
    fn a_blank_rule_is_reported_as_empty() {
        let settings = Settings::parse(r#"{"permissions": {"deny": ["   ", ""]}}"#);
        let (permissions, rejected) = from_settings(
            &settings,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert!(permissions.is_empty());
        assert_eq!(
            rejected.iter().map(describe).collect::<Vec<_>>(),
            ["'   ' is empty", "'' is empty"]
        );
    }

    /// PERM-11: two unreadable lines that differ only in surrounding space are reported as two
    /// different sentences, each quoting its line as the file spelled it.
    #[test]
    fn a_padded_unreadable_rule_is_reported_in_the_spelling_the_file_used() {
        let settings = Settings::parse(r#"{"permissions": {"deny": ["Read(.env", " Read(.env"]}}"#);
        let (permissions, rejected) = from_settings(
            &settings,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert!(permissions.is_empty());
        let said = rejected.iter().map(describe).collect::<Vec<_>>();
        assert_eq!(said.len(), 2);
        assert_ne!(said[0], said[1]);
        assert!(said[0].starts_with("'Read(.env' "), "{}", said[0]);
        assert!(said[1].starts_with("' Read(.env' "), "{}", said[1]);
    }

    /// Every reason a rule can be dropped for has words of its own. An arm is easy to copy and
    /// leave pointing at the message above it, and a report that gave a mistyped bracket the
    /// wording for a misspelled family would send somebody looking at the wrong part of their
    /// line. Which reasons exist is the kernel's to say and the match has no catch-all, so one
    /// added there does not build until it has been given words.
    #[test]
    fn every_reason_a_rule_is_dropped_for_says_something_of_its_own() {
        let said = [
            Unreadable::NotALine,
            Unreadable::Empty,
            Unreadable::UnclosedBracket,
            Unreadable::UnknownFamily,
            Unreadable::EmptyBrackets,
            Unreadable::Unanchored,
            Unreadable::NotADomainRule,
            Unreadable::NoDomainNamed,
            Unreadable::NotAToolRule,
            Unreadable::NotAList,
        ]
        .map(problem);

        let mut seen = std::collections::BTreeSet::new();
        for words in said {
            assert!(!words.is_empty(), "a reason with nothing to say");
            assert!(seen.insert(words), "two reasons both read '{words}'");
        }
    }

    /// PERM-14 at the boundary that turns lists into rules: a `Bash` allow rule a checkout wrote
    /// decides nothing, and the same rule in the person's own file decides. Both directions in one
    /// test, because a fix that dropped every allow rule would pass the first assertion alone.
    ///
    /// Here rather than only in `bravebot-config` because the lists cross a crate boundary before
    /// anything acts on them, and this is the call every interactive session makes. The three
    /// families share these rules, so an entry that never reaches `Permissions::parse` reaches no
    /// gate: what each gate then does with an `Allow` ruling is pinned where that gate is.
    #[test]
    fn a_checkout_cannot_write_a_rule_that_answers_a_prompt() {
        let block = r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"]}}"#;
        let checkout = layered_settings("checkout-allow", Some(block), None);
        let (permissions, rejected) = from_settings(
            &checkout,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert!(rejected.is_empty(), "{rejected:?}");
        assert_eq!(
            permissions.for_command(&["bash", "scripts/check.sh"]),
            Decision::Unmatched,
            "a checkout's allow rule answered the run prompt"
        );

        let own = layered_settings("own-allow", None, Some(block));
        let (permissions, rejected) = from_settings(
            &own,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert!(rejected.is_empty(), "{rejected:?}");
        assert_eq!(
            permissions.for_command(&["bash", "scripts/check.sh"]),
            Decision::Ruled(Ruling::Allow),
            "the person's own allow rule stopped deciding"
        );
    }

    /// PERM-15 at the same boundary: the rule a checkout proposed decides once the person has
    /// granted it, and the one they did not grant still decides nothing. Both directions in one
    /// test, because a fix that installed every proposed rule would pass the first assertion alone,
    /// which is the defect this whole route exists to close.
    #[test]
    fn a_rule_the_person_granted_answers_the_prompt_and_one_they_did_not_does_not() {
        let block = r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"]}}"#;
        let checkout = layered_settings("granted-allow", Some(block), None);
        let profile = PathBuf::from("/home/x");

        let (permissions, rejected) = with_granted(
            &checkout,
            &["Bash(bash scripts/check.sh)".to_string()],
            Some(&profile),
            Path::new("/nonexistent"),
        );
        assert!(rejected.is_empty(), "{rejected:?}");
        assert_eq!(
            permissions.for_command(&["bash", "scripts/check.sh"]),
            Decision::Ruled(Ruling::Allow),
            "a rule the person granted did not answer the run prompt"
        );

        let (ungranted, rejected) =
            with_granted(&checkout, &[], Some(&profile), Path::new("/nonexistent"));
        assert!(rejected.is_empty(), "{rejected:?}");
        assert_eq!(
            ungranted.for_command(&["bash", "scripts/check.sh"]),
            Decision::Unmatched,
            "a rule nobody granted answered the run prompt"
        );
    }

    /// PERM-2, PERM-15: a granted rule cannot outrank a narrowing one. Granting is the person
    /// answering a prompt in advance, and a `deny` rule is the refusal that comes before there is a
    /// prompt to answer, so the list a granted entry joins is still consulted last.
    #[test]
    fn a_granted_rule_does_not_beat_a_deny_rule() {
        let block = r#"{"permissions": {"deny": ["Bash(bash scripts/check.sh)"]}}"#;
        let checkout = layered_settings("granted-loses-to-deny", Some(block), None);

        let (permissions, _) = with_granted(
            &checkout,
            &["Bash(bash scripts/check.sh)".to_string()],
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert_eq!(
            permissions.for_command(&["bash", "scripts/check.sh"]),
            Decision::Ruled(Ruling::Deny),
            "a granted rule overrode a deny rule"
        );
    }

    /// PERM-11, PERM-15: granting a line that is not a rule grants nothing and is reported, exactly
    /// as an unreadable rule in the person's own file is. What the person accepted was the text on
    /// the screen, and what a text means is still the rule language's to decide: a line reported
    /// nowhere reads to whoever wrote it as one in force.
    #[test]
    fn a_granted_line_that_is_not_a_rule_is_reported_and_decides_nothing() {
        let (permissions, rejected) = with_granted(
            &Settings::default(),
            &["Bash(bash scripts/check.sh".to_string()],
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert!(
            permissions.is_empty(),
            "an unreadable rule decided something"
        );
        assert_eq!(rejected.len(), 1);
        assert!(describe(&rejected[0]).contains("scripts/check.sh"));
    }

    /// PERM-11, PERM-14: a checkout's `allow` entry is a grant that was withheld where it is a
    /// rule, and an unreadable entry wherever it was written. Both directions from one file,
    /// because each half alone is passed by a wrong implementation: offering every entry reports a
    /// line that can never decide anything as a rule to grant, and offering none reports a rule the
    /// person could have granted as a typo.
    #[test]
    fn an_allow_entry_a_checkout_wrote_is_proposed_only_where_it_is_a_rule() {
        let block = r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)", "Nonsense"]}}"#;
        let checkout = layered_settings("checkout-allow-readability", Some(block), None);
        let profile = PathBuf::from("/home/x");

        let offered: Vec<String> = proposed(&checkout, Some(&profile))
            .into_iter()
            .map(|rule| rule.rule)
            .collect();
        assert_eq!(
            offered,
            ["Bash(bash scripts/check.sh)"],
            "the entries offered as grants are not the ones that are rules"
        );

        let (_, rejected) = from_settings(&checkout, Some(&profile), Path::new("/nonexistent"));
        let said: Vec<String> = rejected.iter().map(describe).collect();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].contains("Nonsense"),
            "the entry that is not a rule was not named for what is wrong with it: {said:?}"
        );
    }

    /// PERM-11: the same entry in a run nobody is watching. Such a run installs no allow rule at
    /// all and puts no question, so this report is the only one it has: an entry named nowhere
    /// reads to whoever wrote it as a rule in force, which is the same failure in a surface that
    /// has no grant to explain it away.
    #[test]
    fn a_checkouts_unreadable_allow_entry_is_named_to_a_run_nobody_is_watching() {
        let block = r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)", "Nonsense"]}}"#;
        let checkout = layered_settings("unattended-checkout-allow", Some(block), None);

        let (_, rejected) = for_an_unattended_run(
            &checkout,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        let said: Vec<String> = rejected.iter().map(describe).collect();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].contains("Nonsense"),
            "the entry that is not a rule was not named: {said:?}"
        );
    }

    /// A scratch home and working directory with the layers the arguments name, read the way a
    /// session reads them.
    ///
    /// Settings written to files rather than parsed from text, because which file a rule was in is
    /// the whole of what this test turns on and [`bravebot_config::Settings::parse`] has no layers.
    fn layered_settings(
        name: &str,
        project: Option<&str>,
        home: Option<&str>,
    ) -> bravebot_config::Settings {
        let root = crate::testutil::scratch_dir(&format!("bravebot-permission-layers-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let home_dir = root.join("home");
        let cwd = root.join("cwd");
        std::fs::create_dir_all(&home_dir).expect("scratch home");
        std::fs::create_dir_all(cwd.join(".bravebot")).expect("scratch project");
        if let Some(text) = project {
            std::fs::write(cwd.join(".bravebot").join("settings.json"), text).expect("project");
        }
        if let Some(text) = home {
            std::fs::write(home_dir.join("settings.json"), text).expect("home");
        }
        bravebot_config::Settings::layered(Some(home_dir), Some(&cwd), None)
    }

    /// A single leading slash is anchored at the settings file's own directory, which is the
    /// documented behaviour and the one somebody is most likely to get wrong.
    #[test]
    fn a_single_slash_rule_is_anchored_at_the_settings_directory() {
        let settings = Settings::parse(r#"{"permissions": {"deny": ["Read(/secrets/**)"]}}"#);
        let (permissions, _) = from_settings(
            &settings,
            Some(&PathBuf::from("/home/x")),
            Path::new("/nonexistent"),
        );
        assert_eq!(
            permissions.for_path(Subject::Read, "/home/x/.bravebot/secrets/key"),
            Decision::Ruled(Ruling::Deny)
        );
        assert_eq!(
            permissions.for_path(Subject::Read, "/secrets/key"),
            Decision::Unmatched
        );
    }

    /// PERM-3 end to end: a `/x` rule in a checkout's own settings file covers the directory beside
    /// that file, and not the one beside the global file.
    #[test]
    fn a_checkouts_slash_rule_is_anchored_beside_the_checkouts_file() {
        let name = "slash-rule-in-a-checkout";
        let block = r#"{"permissions": {"deny": ["Read(/secrets/**)"]}}"#;
        let settings = layered_settings(name, Some(block), None);
        let root = crate::testutil::scratch_dir(&format!("bravebot-permission-layers-{name}"));
        let beside_the_file = root
            .join("cwd")
            .join(".bravebot")
            .join("secrets")
            .join("key");
        let (permissions, _) =
            from_settings(&settings, Some(&root.join("profile")), &root.join("cwd"));
        assert_eq!(
            permissions.for_path(Subject::Read, &beside_the_file.display().to_string()),
            Decision::Ruled(Ruling::Deny)
        );
        let beside_the_global = root
            .join("profile")
            .join(".bravebot")
            .join("secrets")
            .join("key");
        assert_eq!(
            permissions.for_path(Subject::Read, &beside_the_global.display().to_string()),
            Decision::Unmatched
        );
    }

    /// A scratch directory holding a project with `real/secret.txt` and `real/notes.md` in it and a
    /// `linked` link to `real`, canonicalized as a session's root is.
    #[cfg(unix)]
    fn project_with_a_linked_directory(name: &str) -> PathBuf {
        let scratch = crate::testutil::scratch_dir(name);
        let _ = std::fs::remove_dir_all(&scratch);
        let root = scratch.join("project");
        std::fs::create_dir_all(root.join("real")).unwrap();
        std::fs::write(root.join("real").join("secret.txt"), "hunter2").unwrap();
        std::fs::write(root.join("real").join("notes.md"), "notes").unwrap();
        std::os::unix::fs::symlink("real", root.join("linked")).unwrap();
        root.canonicalize().unwrap()
    }

    /// PERM-7: a `deny` or `ask` rule spelled through a link covers the file the link reaches, under
    /// the name a gate holds it by, and still covers the spelling it was written with.
    #[cfg(unix)]
    #[test]
    fn a_restricting_rule_spelled_through_a_link_covers_the_file_it_reaches() {
        let root = project_with_a_linked_directory("permissions-rule-through-a-link");
        let settings = Settings::parse(
            r#"{"permissions": {"deny": ["Read(linked/**)"], "ask": ["Edit(linked/notes.md)"]}}"#,
        );
        let (permissions, rejected) = from_settings(&settings, None, &root);
        assert!(rejected.is_empty(), "{rejected:?}");

        assert_eq!(
            permissions.for_path(Subject::Read, "real/secret.txt"),
            Decision::Ruled(Ruling::Deny),
            "a deny rule spelled through a link missed the file it names"
        );
        assert_eq!(
            permissions.for_path(Subject::Edit, "real/notes.md"),
            Decision::Ruled(Ruling::Ask),
            "an ask rule spelled through a link missed the file it names"
        );
        assert_eq!(
            permissions.for_path(Subject::Read, "linked/secret.txt"),
            Decision::Ruled(Ruling::Deny),
            "the spelling the rule was written with stopped being covered"
        );
        assert_eq!(
            permissions.len(),
            2,
            "a followed link was counted as a rule"
        );
    }

    /// A run nobody is watching reads the same `deny` and `ask` rules, so it follows them the same way.
    #[cfg(unix)]
    #[test]
    fn an_unattended_run_follows_a_rule_spelled_through_a_link_too() {
        let root = project_with_a_linked_directory("permissions-unattended-rule-through-a-link");
        let settings = Settings::parse(
            r#"{"permissions": {"deny": ["Read(linked/**)"], "ask": ["Edit(linked/notes.md)"]}}"#,
        );
        let (permissions, rejected) = for_an_unattended_run(&settings, None, &root);
        assert!(rejected.is_empty(), "{rejected:?}");

        assert_eq!(
            permissions.for_path(Subject::Read, "real/secret.txt"),
            Decision::Ruled(Ruling::Deny),
            "a run nobody is watching read a file a deny rule names through a link"
        );
        assert_eq!(
            permissions.for_path(Subject::Edit, "real/notes.md"),
            Decision::Ruled(Ruling::Ask),
            "a run nobody is watching wrote a file an ask rule names through a link"
        );
    }

    /// The name a link reaches is one place, so the rule added under it does not float the way the
    /// written one does (PERM-4): a link to `real` is not a rule about every `real` in the tree.
    #[cfg(unix)]
    #[test]
    fn the_name_a_link_reaches_is_covered_at_that_place_only() {
        let root = project_with_a_linked_directory("permissions-link-landing-does-not-float");
        let settings = Settings::parse(r#"{"permissions": {"deny": ["Read(linked/**)"]}}"#);
        let (permissions, _) = from_settings(&settings, None, &root);

        assert_eq!(
            permissions.for_path(Subject::Read, "vendor/real/secret.txt"),
            Decision::Unmatched,
            "a rule about one linked directory covered every directory of its target's name"
        );
        assert_eq!(
            permissions.for_path(Subject::Read, "vendor/linked/secret.txt"),
            Decision::Ruled(Ruling::Deny),
            "the written spelling stopped floating"
        );
    }

    /// A full path key is opened from its drive where a backslash separates, a bare drive from its
    /// root, and as written on every other host, where `/C:` is a directory like any other.
    #[test]
    fn a_full_path_key_is_opened_from_its_drive_only_where_a_backslash_separates() {
        assert_eq!(host_spelling("/C:/x", true), "C:/x");
        assert_eq!(
            host_spelling("/C:", true),
            "C:/",
            "a bare drive was opened as wherever the process last was on it"
        );
        assert_eq!(host_spelling("/C:/x", false), "/C:/x");
        assert_eq!(host_spelling("/C:", false), "/C:");
        assert_eq!(host_spelling("/home/x", true), "/home/x");
    }

    /// PERM-7: a name is a place at the top of the workspace as well as at any depth, so a rule
    /// naming one that is a link covers the file the link reaches.
    #[cfg(unix)]
    #[test]
    fn a_rule_naming_a_link_covers_the_file_it_reaches() {
        let root = project_with_a_linked_directory("permissions-name-that-is-a-link");
        std::os::unix::fs::symlink("real/secret.txt", root.join(".env")).unwrap();
        let settings = Settings::parse(r#"{"permissions": {"deny": ["Read(.env)"]}}"#);
        let (permissions, _) = from_settings(&settings, None, &root);

        assert_eq!(
            permissions.for_path(Subject::Read, "real/secret.txt"),
            Decision::Ruled(Ruling::Deny),
            "a rule naming a link missed the file the link reaches"
        );
        assert_eq!(
            permissions.for_path(Subject::Read, "sub/.env"),
            Decision::Ruled(Ruling::Deny),
            "the name stopped matching at any depth"
        );
    }

    /// PERM-3: a full path with no link in it names a project file in the other namespace, so it is
    /// not added under the name the project holds the file by.
    #[cfg(unix)]
    #[test]
    fn a_full_path_rule_with_no_link_in_it_says_nothing_about_a_project_file() {
        let root = project_with_a_linked_directory("permissions-full-path-without-a-link");
        let settings = Settings::parse(&format!(
            r#"{{"permissions": {{"deny": ["Read(/{}/real/**)"]}}}}"#,
            root.display()
        ));
        let (permissions, rejected) = from_settings(&settings, None, &root);
        assert!(rejected.is_empty(), "{rejected:?}");

        assert_eq!(
            permissions.for_path(Subject::Read, "real/secret.txt"),
            Decision::Unmatched,
            "a full path with no link in it reached into the project's namespace"
        );
        assert_eq!(
            permissions.for_path(
                Subject::Read,
                &root.join("real").join("secret.txt").display().to_string()
            ),
            Decision::Ruled(Ruling::Deny)
        );
    }

    /// An `allow` rule grants only the spelling the person approved, so a link in it is not a
    /// second place it grants (PERM-7).
    #[cfg(unix)]
    #[test]
    fn an_allow_rule_spelled_through_a_link_grants_only_the_spelling_it_names() {
        let root = project_with_a_linked_directory("permissions-allow-through-a-link");
        let settings = layered_settings(
            "allow-through-a-link",
            None,
            Some(r#"{"permissions": {"allow": ["Edit(linked/**)"]}}"#),
        );
        let (permissions, _) = from_settings(&settings, None, &root);

        assert_eq!(
            permissions.for_path(Subject::Edit, "real/notes.md"),
            Decision::Unmatched,
            "an allow rule was followed through a link"
        );
        assert_eq!(
            permissions.for_path(Subject::Edit, "linked/notes.md"),
            Decision::Ruled(Ruling::Allow)
        );
    }

    /// A link that leaves the project reaches a file a gate holds in full, so that is the name the
    /// rule covers it by. The same holds the other way: a full path spelled through a link into the
    /// project covers the file by its name in the project.
    #[cfg(unix)]
    #[test]
    fn a_rule_through_a_link_covers_its_target_on_whichever_side_of_the_root_it_is() {
        let root = project_with_a_linked_directory("permissions-link-across-the-root");
        let scratch = root.parent().unwrap();
        std::fs::create_dir_all(scratch.join("outside")).unwrap();
        std::os::unix::fs::symlink(scratch.join("outside"), root.join("out")).unwrap();
        std::os::unix::fs::symlink(&root, scratch.join("project-link")).unwrap();
        let into_the_project = format!("Read(/{}/project-link/real/**)", scratch.display());
        let settings = Settings::parse(&format!(
            r#"{{"permissions": {{"deny": ["Read(out/**)", "{into_the_project}"]}}}}"#
        ));
        let (permissions, rejected) = from_settings(&settings, None, &root);
        assert!(rejected.is_empty(), "{rejected:?}");

        let outside = scratch.join("outside").join("key");
        assert_eq!(
            permissions.for_path(Subject::Read, &outside.display().to_string()),
            Decision::Ruled(Ruling::Deny),
            "a rule through a link out of the project missed the full path it reaches"
        );
        assert_eq!(
            permissions.for_path(Subject::Read, "real/secret.txt"),
            Decision::Ruled(Ruling::Deny),
            "a full path through a link into the project missed the file it reaches"
        );
    }

    /// The home and settings directories a `~/` and a `/` rule start at are followed the same way,
    /// so a home directory reached through a link is not a way round a rule written under it.
    #[cfg(unix)]
    #[test]
    fn a_rule_under_a_linked_home_directory_covers_the_directory_it_reaches() {
        let root = project_with_a_linked_directory("permissions-linked-home");
        let scratch = root.parent().unwrap();
        let home = scratch.join("home");
        std::fs::create_dir_all(home.join(".bravebot")).unwrap();
        std::os::unix::fs::symlink(&home, scratch.join("home-link")).unwrap();
        let settings = Settings::parse(
            r#"{"permissions": {"deny": ["Read(~/secrets/**)", "Read(/keys/**)"]}}"#,
        );
        let (permissions, _) = from_settings(&settings, Some(&scratch.join("home-link")), &root);

        for path in [
            home.join("secrets").join("key"),
            home.join(".bravebot").join("keys").join("key"),
        ] {
            assert_eq!(
                permissions.for_path(Subject::Read, &path.display().to_string()),
                Decision::Ruled(Ruling::Deny),
                "{} was not covered through the linked home directory",
                path.display()
            );
        }
    }

    /// The directories a file asked for come back in the order it named them, since a caller
    /// opens them one at a time and reports each.
    #[test]
    fn the_directories_a_file_named_come_back_in_order() {
        let settings = Settings::parse(
            r#"{"permissions": {"additionalDirectories": ["../shared", "/opt/other"]}}"#,
        );
        assert_eq!(
            additional_directories(&settings),
            ["../shared", "/opt/other"]
        );
    }
}
