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

use bravebot_config::{PermissionLists, Settings};
use bravebot_core::permissions::{Anchors, Permissions, Rejected, Unreadable};
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
pub fn from_settings(
    settings: &Settings,
    profile: Option<&std::path::Path>,
) -> (Permissions, Vec<Rejected>) {
    let lists = settings.permissions();
    let (permissions, mut rejected) =
        Permissions::parse(&lists.deny, &lists.ask, &lists.allow, &anchors(profile));
    rejected.extend(entries_that_are_not_lines(lists));
    (permissions, rejected)
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

/// One dropped entry as the person reading about it sees it, in their own language.
///
/// The kernel names what was wrong and this says it, because a person reads it and what a person
/// reads comes from a catalog (LOCALE-1), which the kernel neither holds nor prints through. Every
/// reporting site goes through here, so `doctor` and a session cannot come to word it differently.
///
/// The entry is quoted in the spelling the file used: a line of three spaces and a line of none
/// are two entries somebody has to find, and a report that trimmed both names neither.
pub fn describe(rejected: &Rejected) -> String {
    t!(
        permission_rule_unreadable,
        rule = &rejected.text,
        problem = problem(rejected.reason)
    )
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
/// `profile` is the user's home directory, for the reason [`from_settings`] states.
pub fn for_an_unattended_run(
    settings: &Settings,
    profile: Option<&std::path::Path>,
) -> (Permissions, Vec<Rejected>) {
    let lists = settings.permissions();
    let anchors = anchors(profile);
    let (permissions, mut rejected) = Permissions::parse(&lists.deny, &lists.ask, &[], &anchors);
    // Read for its rejects and then dropped, rather than not read at all. A line the person
    // believes is in force and that nothing can act on is worth saying out loud wherever it was
    // written, and an allow rule that is silently unreadable here reads to them as one that holds.
    let (_, unreadable) = Permissions::parse(&[], &[], &lists.allow, &anchors);
    rejected.extend(unreadable);
    rejected.extend(entries_that_are_not_lines(lists));
    (permissions, rejected)
}

/// What a leading `~` and a leading `/` in a rule are resolved against.
///
/// Both come from the user's home directory, the settings one by joining `.bravebot` onto it. So
/// what this takes is the home directory itself and not the state directory that sits inside it
/// (PERM-3), which is the same distinction a `~` in a command line turns on (CMDLINE-4).
fn anchors(profile: Option<&std::path::Path>) -> Anchors {
    let home = profile.map(|profile| profile.display().to_string());
    Anchors {
        // The settings file lives in the global state directory, so a `/` rule is anchored there.
        settings_dir: home.as_ref().map(|home| format!("{home}/.bravebot")),
        home,
    }
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
    use std::path::PathBuf;

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
        let (permissions, rejected) = from_settings(&settings, Some(&PathBuf::from("/home/x")));
        assert!(rejected.is_empty());
        assert_eq!(
            permissions.for_command("git diff --stat"),
            Decision::Ruled(Ruling::Allow)
        );
        assert_eq!(
            permissions.for_command("git push origin main"),
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
        let (permissions, rejected) = from_settings(&Settings::default(), None);
        assert!(permissions.is_empty());
        assert!(rejected.is_empty());
    }

    /// A line that is not a rule is handed back rather than dropped in silence, so a person can be
    /// told which of their rules is doing nothing.
    #[test]
    fn a_line_that_is_not_a_rule_is_reported() {
        let settings = Settings::parse(r#"{"permissions": {"deny": ["Read(.env)", "Nonsense"]}}"#);
        let (permissions, rejected) = from_settings(&settings, Some(&PathBuf::from("/home/x")));
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
        let (permissions, rejected) =
            for_an_unattended_run(&settings, Some(&PathBuf::from("/home/x")));
        assert!(rejected.is_empty());
        assert_eq!(
            permissions.for_command("git diff --stat"),
            Decision::Unmatched
        );
        assert_eq!(
            permissions.for_command("git push origin main"),
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
        let (permissions, rejected) =
            for_an_unattended_run(&settings, Some(&PathBuf::from("/home/x")));
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
        let (permissions, rejected) = from_settings(&settings, Some(&PathBuf::from("/home/x")));
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
        let (permissions, rejected) =
            for_an_unattended_run(&settings, Some(&PathBuf::from("/home/x")));
        assert!(permissions.is_empty());
        assert_eq!(rejected.len(), 1);
        assert!(describe(&rejected[0]).contains("Bash(git diff *)"));
    }

    /// Blank text is a line, so it reaches the rule parser and comes back with the parser's own
    /// word for it. A filter over the list is what stopped that rejection ever being reached.
    ///
    /// Reported in the spelling the file used, which for a blank line is the whole of what
    /// distinguishes it: two blank entries reported as `''` are two lines a person cannot find.
    #[test]
    fn a_blank_rule_is_reported_as_empty() {
        let settings = Settings::parse(r#"{"permissions": {"deny": ["   ", ""]}}"#);
        let (permissions, rejected) = from_settings(&settings, Some(&PathBuf::from("/home/x")));
        assert!(permissions.is_empty());
        assert_eq!(
            rejected.iter().map(describe).collect::<Vec<_>>(),
            ["'   ' is empty", "'' is empty"]
        );
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
        let (permissions, rejected) = from_settings(&checkout, Some(&PathBuf::from("/home/x")));
        assert!(rejected.is_empty(), "{rejected:?}");
        assert_eq!(
            permissions.for_command("bash scripts/check.sh"),
            Decision::Unmatched,
            "a checkout's allow rule answered the run prompt"
        );

        let own = layered_settings("own-allow", None, Some(block));
        let (permissions, rejected) = from_settings(&own, Some(&PathBuf::from("/home/x")));
        assert!(rejected.is_empty(), "{rejected:?}");
        assert_eq!(
            permissions.for_command("bash scripts/check.sh"),
            Decision::Ruled(Ruling::Allow),
            "the person's own allow rule stopped deciding"
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
        let (permissions, _) = from_settings(&settings, Some(&PathBuf::from("/home/x")));
        assert_eq!(
            permissions.for_path(Subject::Read, "/home/x/.bravebot/secrets/key"),
            Decision::Ruled(Ruling::Deny)
        );
        assert_eq!(
            permissions.for_path(Subject::Read, "/secrets/key"),
            Decision::Unmatched
        );
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
