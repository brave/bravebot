//! The skills a turn would advertise, offered in the box as a slash word is typed, and the prompts
//! of the MCP servers the session reached.
//!
//! Taking one writes `/name` into the line and nothing else: the line is still a prompt, sent as
//! typed, and the planner is what loads the skill it names, except one its file keeps from the
//! planner, which the turn supplies for the line that names it (SKILL-20). A prompt of a server is a
//! row of the same kind, and what makes the line run it is the turn that is sent, never the box
//! (CMD-26). Taking one also writes the names of the arguments it cannot run without, so they are
//! not typed.

use bravebot_agent::Workspace;
use bravebot_agent::skills::Source;
use bravebot_core::permissions::Permissions;
use bravebot_core::trust::TrustStore;
use bravebot_session::audit::Trail;

/// Where a row the box offers came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A skill, from one of the three places a skill is found.
    Skill(Source),
    /// A prompt of the MCP server the person declared as this alias, and the names of the arguments
    /// it cannot run without.
    Server {
        alias: String,
        required: Vec<String>,
    },
}

impl From<Source> for Origin {
    fn from(source: Source) -> Self {
        Self::Skill(source)
    }
}

/// One row the box offers after a slash: a skill's name, or the word of an MCP server's prompt,
/// with when to use it and where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// What follows the name when it is typed, drawn after it as the line stands ready for it.
    pub argument_hint: Option<String>,
    pub source: Origin,
    /// Whether the skill's file keeps it from the planner, so only the person's own `/name` loads it
    /// (SKILL-20). Drawn on the row, since a person choosing a skill to name wants to know it is one
    /// only they can.
    pub person_only: bool,
}

impl Skill {
    /// Whether the row is a server's prompt, which runs only where it begins the line, since the
    /// arguments after it are the whole of what it takes (CMD-26).
    pub fn is_a_server_prompt(&self) -> bool {
        matches!(self.source, Origin::Server { .. })
    }

    /// What taking the row writes: the name after a slash and a space, and for a server's prompt
    /// each required argument as `name=`, so the person gives it a value and does not retype what
    /// the row drew (CMD-26). The line is read back as the words it holds, with nothing compared
    /// with what the server listed.
    pub fn taken_as(&self) -> String {
        let mut line = format!("/{} ", self.name);
        if let Origin::Server { required, .. } = &self.source {
            let stubs: Vec<String> = required.iter().map(|name| format!("{name}=")).collect();
            line.push_str(&stubs.join(" "));
        }
        line
    }
}

/// The skills a turn starting now would resolve, read the way the turn reads them.
///
/// An untrusted project's skills are dropped here as they are there, and so is one `permissions`
/// denies reading, so a name the turn would refuse is never drawn. A skill its file keeps from the
/// planner is drawn, marked, since the box is where a person finds the name to type (SKILL-20).
pub fn resolved(workspace: &Workspace, trust: TrustStore, permissions: Permissions) -> Vec<Skill> {
    bravebot_agent::skills::offered(
        workspace,
        bravebot_agent::home::directory().as_deref(),
        trust,
        permissions,
        &mut Trail::new(),
    )
    .iter()
    .map(|skill| Skill {
        name: skill.name.clone(),
        description: skill.description.clone(),
        argument_hint: skill.argument_hint.clone(),
        source: skill.source.into(),
        person_only: skill.person_only,
    })
    .collect()
}

/// The prompts of the MCP servers the session reached, as rows of the box (CMD-26).
///
/// Drawn from what each server offered when it started, through the same display release a tool
/// list takes when it is put to a person. The words are the server's own, so a row is for a
/// person to read, and what taking one writes is only words it drew. The line that results is read
/// as the person's own, never against the list. The policy is a scratch one: the release is the
/// only thing it is for, and its trail is not a turn's.
pub fn from_servers(servers: Option<&bravebot_agent::mcp::Session>) -> Vec<Skill> {
    use bravebot_core::capability::CapabilitySet;
    use bravebot_core::policy::{Policy, ReleasePlan, Routing};
    let Some(servers) = servers else {
        return Vec::new();
    };
    let mut routing = Routing::new();
    routing.insert_trusted("task", "show the prompts the MCP servers offer");
    let mut trail = Trail::new();
    let Ok(mut policy) = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::none(),
        &mut trail,
    ) else {
        return Vec::new();
    };
    servers
        .prompts(&mut policy)
        .into_iter()
        .map(|prompt| Skill {
            name: prompt.word,
            description: prompt.description.unwrap_or_default(),
            argument_hint: argument_hint(&prompt.arguments),
            source: Origin::Server {
                alias: prompt.alias,
                required: prompt
                    .arguments
                    .iter()
                    .filter(|argument| argument.required)
                    .map(|argument| argument.name.clone())
                    .collect(),
            },
            person_only: false,
        })
        .collect()
}

/// What follows a prompt's word: each required argument as `name=<value>` and then each optional
/// one in brackets, or nothing where it takes none.
fn argument_hint(arguments: &[bravebot_agent::mcp::ListedArgument]) -> Option<String> {
    let (required, optional): (Vec<_>, Vec<_>) =
        arguments.iter().partition(|argument| argument.required);
    let hint = required
        .iter()
        .map(|argument| format!("{}=<value>", argument.name))
        .chain(
            optional
                .iter()
                .map(|argument| format!("[{}=<value>]", argument.name)),
        )
        .collect::<Vec<_>>()
        .join(" ");
    (!hint.is_empty()).then_some(hint)
}

/// The half-typed skill name at the end of the line, without its slash, or `None`.
///
/// The last word, so a skill can be named mid-sentence the way a file can. Never after the word
/// of a command line: what follows a command is its argument, taken verbatim, and completing
/// inside one would change what the command is given. The command word alone is still being
/// typed, since a longer skill name may start with it.
pub fn typed(line: &str) -> Option<&str> {
    if line.ends_with(char::is_whitespace) {
        return None;
    }
    let mut words = line.split_whitespace();
    let name = words.clone().next_back()?.strip_prefix('/')?;
    if words.nth(1).is_some() && crate::command_table::command_typed(line).is_some() {
        return None;
    }
    Some(name)
}

/// Whether any word of the line begins with a slash, so a skill could be named in it.
pub fn mentioned(line: &str) -> bool {
    line.split_whitespace().any(|word| word.starts_with('/'))
}

/// The rows the line could still be completed to: those [`matching`] finds for the word being
/// typed, less the prompts of servers where that word does not begin the line, since a line runs a
/// prompt only from its first word (CMD-26).
pub fn offered_for(held: &[Skill], line: &str) -> Vec<Skill> {
    let Some(typed) = typed(line) else {
        return Vec::new();
    };
    let at_start = !line.trim_start().contains(char::is_whitespace);
    matching(held, typed)
        .into_iter()
        .filter(|row| at_start || !row.is_a_server_prompt())
        .collect()
}

/// The held skills whose names start with what was typed, in name order.
///
/// A name with a space or a control character in it is left out, since taking one would write a
/// line that reads as something other than a skill's name. So is a name a command already claims:
/// `/loop` at the start of a line is the command, and completing a skill onto it would write a
/// line that runs something.
pub fn matching(held: &[Skill], typed: &str) -> Vec<Skill> {
    let mut found: Vec<Skill> = held
        .iter()
        .filter(|skill| skill.name.starts_with(typed) && offerable(&skill.name))
        .cloned()
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// The held skill a word names in full, or `None` where it names nothing offered.
///
/// The word as typed, with its slash. Exact where [`matching`] is by prefix: a half-typed name
/// is still being completed and has not yet named anything.
pub fn named<'a>(held: &'a [Skill], word: &str) -> Option<&'a Skill> {
    let name = word.strip_prefix('/')?;
    held.iter()
        .find(|skill| skill.name == name && offerable(&skill.name))
}

fn offerable(name: &str) -> bool {
    !name.is_empty()
        && !name.chars().any(|c| c.is_whitespace() || c.is_control())
        && !crate::command_table::commands()
            .iter()
            .any(|command| command.name.strip_prefix('/') == Some(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(name: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: format!("what {name} is for"),
            argument_hint: None,
            source: Source::Home.into(),
            person_only: false,
        }
    }

    fn names(skills: &[Skill]) -> Vec<&str> {
        skills.iter().map(|skill| skill.name.as_str()).collect()
    }

    /// A server's prompt row with `required` as the names it cannot run without.
    fn prompt(name: &str, required: &[&str]) -> Skill {
        Skill {
            source: Origin::Server {
                alias: "weather".to_string(),
                required: required.iter().map(|name| name.to_string()).collect(),
            },
            ..skill(name)
        }
    }

    /// Taking a skill's row writes its name and a space, and so does a prompt's that needs no
    /// argument, while a prompt that needs some gets each as `name=` in the order listed, with no
    /// space after the last, so the caret sits where the last value is typed.
    #[test]
    fn taking_a_row_writes_the_names_of_the_arguments_a_prompt_cannot_run_without() {
        assert_eq!(skill("review-pr").taken_as(), "/review-pr ");
        assert_eq!(prompt("tidy", &[]).taken_as(), "/tidy ");
        assert_eq!(prompt("forecast", &["city"]).taken_as(), "/forecast city=");
        assert_eq!(
            prompt("compare", &["before", "after"]).taken_as(),
            "/compare before= after="
        );
    }

    /// The box offers the set a turn would resolve with the mark its file gave each skill, so the row
    /// can say which ones only the person can have loaded (SKILL-20). A project's skill is there once
    /// the project is vouched for.
    #[test]
    fn a_skill_kept_from_the_planner_is_offered_marked_beside_one_that_is_not() {
        let project = tempfile::tempdir().expect("project");
        for (name, extra) in [
            ("ship-it", "disable-model-invocation: true\n"),
            ("ship-notes", ""),
        ] {
            let at = project.path().join(".bravebot/skills").join(name);
            std::fs::create_dir_all(&at).expect("skill directory");
            std::fs::write(
                at.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: d\n{extra}---\n\nbody\n"),
            )
            .expect("write skill");
        }
        let workspace = Workspace::new(project.path()).expect("workspace");
        let mut trust = TrustStore::new(project.path());
        trust.trust(".");

        let offered = resolved(&workspace, trust, Permissions::new());

        let marked = |name: &str| {
            offered
                .iter()
                .find(|skill| skill.name == name)
                .unwrap_or_else(|| panic!("{name} was not offered"))
                .person_only
        };
        assert!(marked("ship-it"), "the mark was not carried to the box");
        assert!(
            !marked("ship-notes"),
            "the mark was put on a skill that has none"
        );
    }

    /// The word being typed is the last one, at the start of the line or after a sentence, and a
    /// space after it means it is finished.
    #[test]
    fn the_word_being_typed_is_the_last_one_on_the_line() {
        assert_eq!(typed("/rel"), Some("rel"));
        assert_eq!(typed("/"), Some(""));
        assert_eq!(typed("this is /release-no"), Some("release-no"));
        assert_eq!(typed("this is /release-notes "), None, "finished");
        assert_eq!(typed("release-no"), None, "no slash");
        assert_eq!(typed("see /rel and more"), None, "not the last word");
        assert_eq!(typed(""), None);
    }

    /// A command's argument is taken verbatim, so nothing inside one is completed. A longer word
    /// starting with a command is a prompt, and is completed like any other.
    #[test]
    fn nothing_is_offered_inside_a_command_line() {
        assert_eq!(typed("/add-dir /rel"), None);
        assert_eq!(typed("/btw what does /rel"), None);
        assert_eq!(typed("/model"), Some("model"), "the bare command");
        assert_eq!(typed("/pr /rel"), None, "an argument");
        assert_eq!(typed("/renamed /rel"), Some("rel"));
    }

    /// Narrowed by prefix and listed by name, so the name typed in full sorts above every longer
    /// one sharing it.
    #[test]
    fn what_matches_is_every_name_starting_with_the_word_in_name_order() {
        let held = [skill("retro-pr"), skill("release-notes"), skill("retro")];
        assert_eq!(
            names(&matching(&held, "re")),
            ["release-notes", "retro", "retro-pr"]
        );
        assert_eq!(names(&matching(&held, "retro")), ["retro", "retro-pr"]);
        assert!(matching(&held, "zzz").is_empty());
        assert!(matching(&held, "Review").is_empty(), "the other case");
    }

    /// Taking a row writes `/name ` into the box. Where the name is a command's, or carries a
    /// space, that line would be a command rather than a prompt naming a skill, so no such row is
    /// offered.
    #[test]
    fn no_offered_skill_completes_to_a_command_line() {
        let held: Vec<Skill> = crate::command_table::commands()
            .iter()
            .map(|command| skill(command.name.trim_start_matches('/')))
            .chain([skill("add-dir /etc"), skill("bell\u{7}"), skill("ordinary")])
            .collect();

        let offered = matching(&held, "");
        assert_eq!(names(&offered), ["ordinary"], "the control");
        for skill in &offered {
            let line = format!("/{} ", skill.name);
            assert_eq!(
                crate::command_table::command_typed(&line),
                None,
                "{line:?} is a command"
            );
        }
    }
}
