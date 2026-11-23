//! Which rule decides a call a person names, for `bravebot permissions check` (CLI-30) and any
//! other front end that answers the same question.
//!
//! The call is only matched against the rules: nothing is opened, started or fetched. The words
//! are what a person typed, so the match is on routing alone (PERM-1). Nothing here formats an
//! answer for display; each front end draws the raw strings its own way.

use crate::granted::Proposed;
use bravebot_config::RuleSources;
use bravebot_core::permissions::{Permissions, Rule, Ruling, Subject};
use std::path::{Path, PathBuf};

/// The call a person asked about, in the one spelling each family is matched in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// A file a `Read` or `Edit` call names.
    Path(Subject, String),
    /// A command line, program first.
    Command(Vec<String>),
    /// The host a fetch would reach.
    Host(String),
    /// An MCP server's alias and one of its tools.
    Tool(String, String),
    /// A web search, which has no routing a rule could name (PERM-1).
    Search,
}

impl Call {
    /// What `words` (the family, then its specifier) name, or `None` where they name no call.
    pub fn read(words: &[String]) -> Option<Self> {
        let (family, rest) = words.split_first()?;
        match (Subject::parse(family)?, rest) {
            (subject @ (Subject::Read | Subject::Edit), [path]) => {
                Some(Self::Path(subject, path.clone()))
            }
            (Subject::Bash, argv) if !argv.is_empty() => Some(Self::Command(argv.to_vec())),
            // A URL is reduced to its host, as a fetch is (PERM-1); anything that is not one is
            // taken as the host it spells.
            (Subject::WebFetch, [target]) => Some(Self::Host(
                bravebot_core::url::host_of(target).unwrap_or_else(|| target.clone()),
            )),
            (Subject::WebSearch, []) => Some(Self::Search),
            (Subject::Mcp, [target]) => {
                let (alias, tool) = target.split_once(':')?;
                Some(Self::Tool(alias.to_string(), tool.to_string()))
            }
            _ => None,
        }
    }

    /// Whether a session would refuse this call as an 8.3 short name while a command rule
    /// restricts (PERM-5). Only a command can be.
    fn reads_a_short_name(&self, permissions: &Permissions) -> bool {
        match self {
            Self::Command(argv) => permissions.reads_a_short_name(argv),
            _ => false,
        }
    }

    /// The rule of `permissions` that decides this call, and the list it is in.
    fn decided_by<'a>(&self, permissions: &'a Permissions) -> Option<(Ruling, &'a Rule)> {
        match self {
            Self::Path(subject, path) => permissions.rule_for_path(*subject, path),
            Self::Command(argv) => permissions.rule_for_command(argv),
            Self::Host(host) => permissions.rule_for_host(host),
            Self::Tool(alias, tool) => permissions.rule_for_mcp(alias, tool),
            Self::Search => permissions.rule_for_search(),
        }
    }
}

/// What decides a call, and a rule not in force that would have allowed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explained {
    /// What a session holding these rules would do with the call.
    pub decision: Decision,
    /// A checkout's `allow` rule the person has not granted that would allow the call, where no
    /// `deny` or `ask` rule decided it (PERM-15).
    pub not_in_force: Option<NotInForce>,
}

/// What a session does with a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Refused as an 8.3 short name before any rule is read (PERM-5).
    ShortName,
    /// No rule matches.
    NoRule,
    /// A rule decides.
    Rule {
        /// The list the rule is in.
        ruling: Ruling,
        /// The rule as its file spelled it, or as the grant recorded it.
        written: String,
        /// Where the rule came from, where that is known.
        origin: Option<Origin>,
        /// Why the person wrote it, where their own file said (PERM-1). Display text only.
        reason: Option<String>,
    },
}

/// Where a deciding rule came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A settings layer wrote it.
    File(PathBuf),
    /// The person granted a checkout's rule written in this file at the question (PERM-15).
    Granted(PathBuf),
}

/// A checkout's `allow` rule that is not in force until the person grants it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotInForce {
    /// The rule as the checkout's file spelled it.
    pub rule: String,
    /// The file that wrote it, empty where it could not be found.
    pub file: PathBuf,
}

/// Which rule of `permissions` decides `call`, where it came from and why, and which of the
/// `waiting` rules (proposed and not granted) would allow it once granted.
///
/// `sources` names the file and reason of each rule a layer wrote, and `granted` the checkout
/// rules the person granted, which `permissions` already holds. `profile` and `workspace` anchor
/// the waiting rules as a session would anchor them.
pub fn explain(
    call: &Call,
    permissions: &Permissions,
    sources: &RuleSources,
    granted: &[Proposed],
    waiting: &[Proposed],
    profile: Option<&Path>,
    workspace: &Path,
) -> Explained {
    // A session refuses an 8.3 short name before any rule is read (PERM-5), so the answer says
    // that rather than "no rule matches". The decision is core's, not a second copy of it.
    if call.reads_a_short_name(permissions) {
        return Explained {
            decision: Decision::ShortName,
            not_in_force: None,
        };
    }
    let decided = call.decided_by(permissions);
    let decision = match decided {
        None => Decision::NoRule,
        Some((ruling, rule)) => decision(sources, granted, ruling, rule),
    };
    // A rule that restricts decides before any grant, so one that could not grant yet is only
    // worth naming where nothing restricted the call.
    let not_in_force = if matches!(decided, Some((Ruling::Deny | Ruling::Ask, _))) {
        None
    } else {
        let would_allow = crate::permissions::as_allow_list(waiting, profile, workspace);
        call.decided_by(&would_allow).map(|(_, rule)| NotInForce {
            rule: rule.written().to_string(),
            file: waiting
                .iter()
                .find(|w| w.rule.trim() == rule.written())
                .map(|from| from.path.clone())
                .unwrap_or_default(),
        })
    };
    Explained {
        decision,
        not_in_force,
    }
}

/// The decision `rule` makes, the rule as written, where it came from and why.
fn decision(sources: &RuleSources, granted: &[Proposed], ruling: Ruling, rule: &Rule) -> Decision {
    // The file's own spelling where a layer wrote the rule, and the grant's where the person
    // granted a checkout's rule at the question (PERM-15).
    let (written, origin) = match sources.source(ruling.as_str(), rule.written()) {
        Some((file, written)) => (written.to_string(), Some(Origin::File(file.to_path_buf()))),
        None => (
            rule.written().to_string(),
            granted
                .iter()
                .find(|g| g.rule.trim() == rule.written())
                .map(|from| Origin::Granted(from.path.clone())),
        ),
    };
    Decision::Rule {
        ruling,
        written,
        origin,
        // Only the person's own file carries a reason, and only where they wrote one (PERM-1).
        reason: sources
            .reason(ruling.as_str(), rule.written())
            .map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::Settings;

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    /// A home and a checkout, each with a settings file, read as a session there reads them.
    struct Layers {
        settings: Settings,
        home: PathBuf,
        project: PathBuf,
        cwd: PathBuf,
    }

    fn layers(name: &str, home: &str, project: &str) -> Layers {
        let root = crate::testutil::scratch_dir(&format!("bravebot-rule-check-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let home_dir = root.join("home");
        let cwd = root.join("cwd");
        std::fs::create_dir_all(&home_dir).expect("scratch home");
        std::fs::create_dir_all(cwd.join(".bravebot")).expect("scratch project");
        let project_file = cwd.join(".bravebot").join("settings.json");
        std::fs::write(&project_file, project).expect("project settings");
        std::fs::write(home_dir.join("settings.json"), home).expect("home settings");
        Layers {
            settings: Settings::layered(Some(home_dir.clone()), Some(&cwd), None),
            home: home_dir.join("settings.json"),
            project: project_file,
            cwd,
        }
    }

    /// `explain` for `call` over `layers`, with `granted` of the checkout's proposed rules granted.
    fn explained(layers: &Layers, call: &[&str], granted: &[&str]) -> Explained {
        let profile = PathBuf::from("/home/x");
        let proposed = crate::permissions::proposed(&layers.settings, Some(&profile));
        let (granted, waiting): (Vec<Proposed>, Vec<Proposed>) = proposed
            .into_iter()
            .partition(|rule| granted.contains(&rule.rule.as_str()));
        let (permissions, _) = crate::permissions::with_granted(
            &layers.settings,
            &granted
                .iter()
                .map(|rule| rule.rule.clone())
                .collect::<Vec<_>>(),
            Some(&profile),
            &layers.cwd,
        );
        explain(
            &Call::read(&words(call)).expect("a call"),
            &permissions,
            &layers.settings.rule_sources(),
            &granted,
            &waiting,
            Some(&profile),
            &layers.cwd,
        )
    }

    /// CLI-30: each family takes the number of words it is matched on, and the URL a fetch names
    /// is reduced to its host.
    #[test]
    fn a_call_is_read_from_each_familys_words() {
        assert_eq!(
            Call::read(&words(&["Read", ".env"])),
            Some(Call::Path(Subject::Read, ".env".to_string()))
        );
        assert_eq!(
            Call::read(&words(&["Edit", "src/main.rs"])),
            Some(Call::Path(Subject::Edit, "src/main.rs".to_string()))
        );
        assert_eq!(
            Call::read(&words(&["Bash", "git", "push", "origin"])),
            Some(Call::Command(words(&["git", "push", "origin"])))
        );
        assert_eq!(
            Call::read(&words(&["WebFetch", "https://example.com/a?b"])),
            Some(Call::Host("example.com".to_string()))
        );
        assert_eq!(
            Call::read(&words(&["WebFetch", "example.org"])),
            Some(Call::Host("example.org".to_string()))
        );
        assert_eq!(Call::read(&words(&["WebSearch"])), Some(Call::Search));
        assert_eq!(
            Call::read(&words(&["Mcp", "github:create_issue"])),
            Some(Call::Tool("github".to_string(), "create_issue".to_string()))
        );
    }

    /// CLI-30: words that name no call are refused rather than read as some other call.
    #[test]
    fn words_that_name_no_call_are_refused() {
        for refused in [
            &[][..],
            &["Bash"],
            &["Read"],
            &["Read", "a", "b"],
            &["Edit"],
            &["WebFetch"],
            &["WebFetch", "a.com", "b.com"],
            &["WebSearch", "anything"],
            &["Mcp", "github"],
            &["Mcp", "github:a", "b"],
            &["Mcp"],
            &["Write", "x"],
            &["bash", "ls"],
        ] {
            assert_eq!(Call::read(&words(refused)), None, "{refused:?}");
        }
    }

    /// CLI-30, PERM-2: deny before ask before allow, each named with the file that wrote it and
    /// the reason the person's own file gave.
    #[test]
    fn a_rule_decides_with_its_file_and_reason() {
        let layers = layers(
            "decides",
            r#"{"permissions": {
                 "deny": [{"rule": "Bash(git push *)", "reason": "push by hand"}],
                 "allow": ["Bash(git *)"]
               }}"#,
            r#"{"permissions": {"ask": ["Bash(git commit *)"]}}"#,
        );
        assert_eq!(
            explained(&layers, &["Bash", "git", "push", "origin"], &[]).decision,
            Decision::Rule {
                ruling: Ruling::Deny,
                written: "Bash(git push *)".to_string(),
                origin: Some(Origin::File(layers.home.clone())),
                reason: Some("push by hand".to_string()),
            }
        );
        assert_eq!(
            explained(&layers, &["Bash", "git", "commit", "-m", "x"], &[]).decision,
            Decision::Rule {
                ruling: Ruling::Ask,
                written: "Bash(git commit *)".to_string(),
                origin: Some(Origin::File(layers.project.clone())),
                reason: None,
            }
        );
        let allowed = explained(&layers, &["Bash", "git", "status"], &[]);
        assert_eq!(
            allowed.decision,
            Decision::Rule {
                ruling: Ruling::Allow,
                written: "Bash(git *)".to_string(),
                origin: Some(Origin::File(layers.home.clone())),
                reason: None,
            }
        );
        assert_eq!(allowed.not_in_force, None);
    }

    /// CLI-30: a call no rule matches says so.
    #[test]
    fn a_call_no_rule_matches_has_no_rule() {
        let layers = layers(
            "none",
            r#"{"permissions": {"deny": ["Read(/.env)"]}}"#,
            r#"{}"#,
        );
        assert_eq!(
            explained(&layers, &["Bash", "make", "test"], &[]),
            Explained {
                decision: Decision::NoRule,
                not_in_force: None,
            }
        );
    }

    /// CLI-30, PERM-5: an 8.3 short name is refused while a command rule restricts, before any
    /// rule is read, and is matched as usual where none does.
    #[test]
    fn a_short_name_is_refused_while_a_command_rule_restricts() {
        let restricting = layers(
            "short-name",
            r#"{"permissions": {"deny": ["Bash(powershell *)"]}}"#,
            r#"{}"#,
        );
        assert_eq!(
            explained(&restricting, &["Bash", "POWERS~1", "x"], &[]),
            Explained {
                decision: Decision::ShortName,
                not_in_force: None,
            }
        );
        let allowing = layers(
            "short-name-allow",
            r#"{"permissions": {"allow": ["Bash(git *)"]}}"#,
            r#"{}"#,
        );
        assert_eq!(
            explained(&allowing, &["Bash", "POWERS~1", "x"], &[]).decision,
            Decision::NoRule
        );
    }

    /// CLI-30, PERM-15: a checkout's `allow` rule is named as not in force until it is granted,
    /// and decides as granted once it is.
    #[test]
    fn a_checkouts_allow_rule_decides_only_once_granted() {
        let layers = layers(
            "granted",
            r#"{}"#,
            r#"{"permissions": {"allow": ["Bash(make *)"]}}"#,
        );
        assert_eq!(
            explained(&layers, &["Bash", "make", "test"], &[]),
            Explained {
                decision: Decision::NoRule,
                not_in_force: Some(NotInForce {
                    rule: "Bash(make *)".to_string(),
                    file: layers.project.clone(),
                }),
            }
        );
        let granted = explained(&layers, &["Bash", "make", "test"], &["Bash(make *)"]);
        assert_eq!(
            granted.decision,
            Decision::Rule {
                ruling: Ruling::Allow,
                written: "Bash(make *)".to_string(),
                origin: Some(Origin::Granted(layers.project.clone())),
                reason: None,
            }
        );
        assert_eq!(granted.not_in_force, None);
    }

    /// CLI-30: a rule that restricts decides before any grant, so no waiting rule is named
    /// beside it.
    #[test]
    fn no_waiting_rule_is_named_beside_one_that_restricts() {
        let layers = layers(
            "restricts",
            r#"{"permissions": {"ask": ["Bash(make *)"]}}"#,
            r#"{"permissions": {"allow": ["Bash(make *)"]}}"#,
        );
        let asked = explained(&layers, &["Bash", "make", "test"], &[]);
        assert!(
            matches!(
                asked.decision,
                Decision::Rule {
                    ruling: Ruling::Ask,
                    ..
                }
            ),
            "{asked:?}"
        );
        assert_eq!(asked.not_in_force, None);
    }
}
