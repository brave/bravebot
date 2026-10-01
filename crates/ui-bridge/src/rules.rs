//! The permission rules a session opened under, from the `permissions` block of the settings
//! files (docs/specs/permissions.md).
//!
//! The rules are read once, when a session opens, and held for its turns and its manifest runs
//! (PERM-12). A file edited afterwards describes the next session.
//!
//! Which rules take effect is the agent's to decide, and this module asks it:
//!
//! - `deny` and `ask` rules are read from every layer, since both only narrow.
//! - `allow` rules are read from the person's own file, and from a selected settings file
//!   outside the workspace (PERM-14).
//! - An `allow` rule a checkout wrote is proposed and not granted (PERM-14). The terminal puts
//!   the proposed rules to the person in one question (PERM-15). This front end has no such
//!   question yet, so it grants none of them and reads no grant the terminal recorded. Each is
//!   reported, so the person knows the rule is not in force.
//! - A directory named in `additionalDirectories` is not opened (PERM-10), for the same reason.
//!   Each is reported.

use bravebot_config::Settings;
use bravebot_core::permissions::Permissions;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The rules in force for one session, and what is reported about the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsRules {
    /// What a turn and a run are given. Empty where no file wrote a rule, which changes nothing
    /// about what a session asks (PERM-12).
    pub permissions: Permissions,
    /// The rules in force, as the files spelled them, for a front end to show.
    pub deny: Vec<String>,
    pub ask: Vec<String>,
    pub allow: Vec<String>,
    /// Entries nothing could read as a rule, each with the agent's sentence about why (PERM-11).
    pub unreadable: Vec<Unreadable>,
    /// `allow` rules a checkout wrote, which are not in force, with the file each was written in.
    pub proposed: Vec<Proposed>,
    /// Directories a file asked to have opened, which are not open.
    pub directories: Vec<String>,
}

/// An entry that is not a rule, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    /// The entry as the file spelled it.
    pub rule: String,
    /// The agent's sentence about what is wrong with it, in the reader's language.
    pub said: String,
}

/// An `allow` rule a checkout wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposed {
    pub rule: String,
    pub file: PathBuf,
}

impl SettingsRules {
    /// Read the rules out of the settings a session opens under.
    ///
    /// There is a person at this front end, so an `allow` rule in their own file holds, as it
    /// does in a terminal session. The reading for a run nobody watches drops every `allow`
    /// rule, and is not this one.
    ///
    /// `workspace` is the directory the session works in. Its volume answers whether two
    /// spellings of a path that differ only in case name one file, which a path rule has to know.
    pub fn read(settings: &Settings, workspace: &Path) -> Self {
        let profile = bravebot_agent::home::profile();
        let (permissions, rejected) =
            bravebot_agent::permissions::from_settings(settings, profile.as_deref(), workspace);
        let lists = settings.permissions();
        // A list in force leaves out what could not be read, so a front end does not show a
        // mistyped rule as one that holds.
        let readable = |lines: &[String]| -> Vec<String> {
            lines
                .iter()
                .filter(|line| !rejected.iter().any(|reject| &reject.text == *line))
                .cloned()
                .collect()
        };
        Self {
            permissions,
            deny: readable(&lists.deny),
            ask: readable(&lists.ask),
            allow: readable(&lists.allow),
            unreadable: rejected
                .iter()
                .map(|reject| Unreadable {
                    rule: reject.text.clone(),
                    said: bravebot_agent::permissions::describe(reject),
                })
                .collect(),
            proposed: bravebot_agent::permissions::proposed(settings, profile.as_deref())
                .into_iter()
                .map(|proposed| Proposed {
                    rule: proposed.rule,
                    file: proposed.path,
                })
                .collect(),
            directories: bravebot_agent::permissions::additional_directories(settings).to_vec(),
        }
    }

    /// The rules as a front end reads them.
    ///
    /// Rule text is what a person wrote in a settings file, so none of it is untrusted content.
    pub fn json(&self) -> Value {
        json!({
            "deny": self.deny,
            "ask": self.ask,
            "allow": self.allow,
            "unreadable": self.unreadable.iter()
                .map(|entry| json!({ "rule": entry.rule, "said": entry.said }))
                .collect::<Vec<_>>(),
            "proposed": self.proposed.iter()
                .map(|entry| json!({ "rule": entry.rule, "file": entry.file.display().to_string() }))
                .collect::<Vec<_>>(),
            "directories": self.directories,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::permissions::{Decision, Ruling};

    /// A home layer and a project layer, each holding the text given, and the settings read
    /// from the two.
    fn layers(
        home_text: Option<&str>,
        project_text: Option<&str>,
    ) -> (tempfile::TempDir, Settings) {
        let mut builder = tempfile::Builder::new();
        builder.prefix("bravebot-rules-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir().unwrap();
        let home = directory.path().join("home");
        let project = directory.path().join("project");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(project.join(".bravebot")).unwrap();
        if let Some(text) = home_text {
            std::fs::write(home.join("settings.json"), text).unwrap();
        }
        if let Some(text) = project_text {
            std::fs::write(project.join(".bravebot/settings.json"), text).unwrap();
        }
        let settings = Settings::layered(Some(home), Some(&project), None);
        (directory, settings)
    }

    /// What the rules say about fetching from `host`, or `None` where no rule matched.
    fn fetching(rules: &SettingsRules, host: &str) -> Option<Ruling> {
        match rules.permissions.for_host(host) {
            Decision::Ruled(ruling) => Some(ruling),
            Decision::Unmatched => None,
        }
    }

    /// PERM-12: a session with no `permissions` block is given no rules and reports nothing.
    #[test]
    fn no_block_is_no_rules_and_nothing_to_report() {
        let (dir, settings) = layers(None, None);
        let rules = SettingsRules::read(&settings, dir.path());
        // The rules are compared by what they hold rather than to the default, since what they
        // hold also says whether the workspace's volume folds case, which is no rule.
        assert!(rules.permissions.is_empty());
        assert_eq!(
            rules.json(),
            json!({ "deny": [], "ask": [], "allow": [], "unreadable": [], "proposed": [], "directories": [] })
        );
    }

    /// An `allow` rule in the person's own file holds, since there is a person at this front end.
    #[test]
    fn an_allow_rule_in_the_persons_own_file_is_in_force() {
        let (dir, settings) = layers(
            Some(r#"{"permissions": {"allow": ["WebFetch(domain:example.com)"]}}"#),
            None,
        );
        let rules = SettingsRules::read(&settings, dir.path());
        assert_eq!(fetching(&rules, "example.com"), Some(Ruling::Allow));
        assert_eq!(rules.allow, ["WebFetch(domain:example.com)"]);
        assert!(rules.proposed.is_empty());
    }

    /// PERM-14: an `allow` rule a checkout wrote answers no prompt. It is reported with the file
    /// it was written in, and is not listed as in force.
    #[test]
    fn an_allow_rule_a_checkout_wrote_is_reported_and_not_in_force() {
        let (dir, settings) = layers(
            None,
            Some(r#"{"permissions": {"allow": ["WebFetch(domain:example.com)"]}}"#),
        );
        let rules = SettingsRules::read(&settings, dir.path());
        assert_eq!(fetching(&rules, "example.com"), None);
        assert!(rules.allow.is_empty(), "{:?}", rules.allow);
        let [proposed] = rules.proposed.as_slice() else {
            panic!(
                "one rule was proposed and {:?} were reported",
                rules.proposed
            );
        };
        assert_eq!(proposed.rule, "WebFetch(domain:example.com)");
        assert!(
            proposed.file.ends_with("project/.bravebot/settings.json"),
            "{}",
            proposed.file.display()
        );
        drop(dir);
    }

    /// `deny` and `ask` rules only narrow, so a checkout's hold as the person's own do.
    #[test]
    fn a_checkouts_deny_and_ask_rules_are_in_force() {
        let (dir, settings) = layers(
            None,
            Some(
                r#"{"permissions": {"deny": ["WebFetch(domain:denied.test)"],
                    "ask": ["WebFetch(domain:asked.test)"]}}"#,
            ),
        );
        let rules = SettingsRules::read(&settings, dir.path());
        assert_eq!(fetching(&rules, "denied.test"), Some(Ruling::Deny));
        assert_eq!(fetching(&rules, "asked.test"), Some(Ruling::Ask));
        assert_eq!(rules.deny, ["WebFetch(domain:denied.test)"]);
        assert_eq!(rules.ask, ["WebFetch(domain:asked.test)"]);
    }

    /// PERM-2: a checkout's deny rule beats an allow rule in the person's own file.
    #[test]
    fn a_checkouts_deny_rule_beats_the_persons_own_allow_rule() {
        let (dir, settings) = layers(
            Some(r#"{"permissions": {"allow": ["WebFetch(domain:example.com)"]}}"#),
            Some(r#"{"permissions": {"deny": ["WebFetch(domain:example.com)"]}}"#),
        );
        let rules = SettingsRules::read(&settings, dir.path());
        assert_eq!(fetching(&rules, "example.com"), Some(Ruling::Deny));
    }

    /// PERM-11: an entry that is not a rule is reported with the agent's sentence, is not listed
    /// as in force, and takes no other rule with it.
    #[test]
    fn an_unreadable_rule_is_reported_and_the_rest_still_hold() {
        let (dir, settings) = layers(
            Some(
                r#"{"permissions": {"deny": ["Fetchh(domain:denied.test)",
                    "WebFetch(domain:denied.test)"]}}"#,
            ),
            None,
        );
        let rules = SettingsRules::read(&settings, dir.path());
        assert_eq!(rules.deny, ["WebFetch(domain:denied.test)"]);
        assert_eq!(fetching(&rules, "denied.test"), Some(Ruling::Deny));
        let [unreadable] = rules.unreadable.as_slice() else {
            panic!(
                "one entry was unreadable and {:?} were reported",
                rules.unreadable
            );
        };
        assert_eq!(unreadable.rule, "Fetchh(domain:denied.test)");
        assert!(
            unreadable.said.contains("Fetchh(domain:denied.test)"),
            "{}",
            unreadable.said
        );
    }

    /// PERM-14: a settings file the person selected may write an `allow` rule where it is outside
    /// the workspace. One inside the workspace is the checkout's file under another name, so
    /// its `allow` rule is proposed and not in force.
    #[test]
    fn a_selected_file_grants_only_from_outside_the_workspace() {
        let mut builder = tempfile::Builder::new();
        builder.prefix("bravebot-rules-selected-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let home = root.join("home");
        let project = root.join("project");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        let text = r#"{"permissions": {"allow": ["WebFetch(domain:example.com)"]}}"#;

        let outside = root.join("override.json");
        std::fs::write(&outside, text).unwrap();
        let rules = SettingsRules::read(
            &Settings::layered(Some(home.clone()), Some(&project), Some(&outside)),
            &project,
        );
        assert_eq!(fetching(&rules, "example.com"), Some(Ruling::Allow));
        assert!(rules.proposed.is_empty(), "{:?}", rules.proposed);

        let inside = project.join("override.json");
        std::fs::write(&inside, text).unwrap();
        let rules = SettingsRules::read(
            &Settings::layered(Some(home), Some(&project), Some(&inside)),
            &project,
        );
        assert_eq!(fetching(&rules, "example.com"), None);
        assert_eq!(rules.proposed.len(), 1, "{:?}", rules.proposed);
    }

    /// PERM-10: a directory a file named is reported and is not something this module opens.
    #[test]
    fn a_directory_a_file_named_is_reported() {
        let (dir, settings) = layers(
            None,
            Some(r#"{"permissions": {"additionalDirectories": ["../other"]}}"#),
        );
        let rules = SettingsRules::read(&settings, dir.path());
        assert_eq!(rules.directories, ["../other"]);
        assert_eq!(rules.json()["directories"], json!(["../other"]));
    }
}
