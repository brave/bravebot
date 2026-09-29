//! The skills a turn would advertise, offered in the box as a slash word is typed.
//!
//! Taking one writes `/name` into the line and nothing else: the line is still a prompt, sent as
//! typed, and the planner is what loads the skill it names.

use bravebot_agent::Workspace;
use bravebot_agent::skills::Source;
use bravebot_core::trust::TrustStore;
use bravebot_session::audit::Trail;

/// One skill as the box offers it: its name, when to use it, and where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// What follows the name when it is typed, drawn after it as the line stands ready for it.
    pub argument_hint: Option<String>,
    pub source: Source,
}

/// The skills a turn starting now would advertise, read the way the turn reads them.
///
/// An untrusted project's skills are dropped here as they are there, so a name the planner would
/// never be shown is never drawn either.
pub fn resolved(workspace: &Workspace, trust: TrustStore) -> Vec<Skill> {
    bravebot_agent::skills::resolved(
        workspace,
        bravebot_agent::home::directory().as_deref(),
        trust,
        &mut Trail::new(),
    )
    .iter()
    .map(|skill| Skill {
        name: skill.name.clone(),
        description: skill.description.clone(),
        argument_hint: skill.argument_hint.clone(),
        source: skill.source,
    })
    .collect()
}

/// The half-typed skill name at the end of the line, without its slash, or `None`.
///
/// The last word, so a skill can be named mid-sentence the way a file can. Never on a command
/// line: what follows a command is its argument, taken verbatim, and completing inside one would
/// change what the command is given.
pub fn typed(line: &str) -> Option<&str> {
    if line.ends_with(char::is_whitespace) {
        return None;
    }
    let name = line.split_whitespace().next_back()?.strip_prefix('/')?;
    if crate::app::command_typed(line).is_some() {
        return None;
    }
    Some(name)
}

/// Whether any word of the line begins with a slash, so a skill could be named in it.
pub fn mentioned(line: &str) -> bool {
    line.split_whitespace().any(|word| word.starts_with('/'))
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
        && !crate::app::commands()
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
            source: Source::Home,
        }
    }

    fn names(skills: &[Skill]) -> Vec<&str> {
        skills.iter().map(|skill| skill.name.as_str()).collect()
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
        assert_eq!(typed("/model"), None, "the bare command");
        assert_eq!(typed("/renamed /rel"), Some("rel"));
    }

    /// Narrowed by prefix and listed by name, so the name typed in full sorts above every longer
    /// one sharing it.
    #[test]
    fn what_matches_is_every_name_starting_with_the_word_in_name_order() {
        let held = [skill("review-pr"), skill("release-notes"), skill("review")];
        assert_eq!(
            names(&matching(&held, "re")),
            ["release-notes", "review", "review-pr"]
        );
        assert_eq!(names(&matching(&held, "review")), ["review", "review-pr"]);
        assert!(matching(&held, "zzz").is_empty());
        assert!(matching(&held, "Review").is_empty(), "the other case");
    }

    /// Taking a row writes `/name ` into the box. Where the name is a command's, or carries a
    /// space, that line would be a command rather than a prompt naming a skill, so no such row is
    /// offered.
    #[test]
    fn no_offered_skill_completes_to_a_command_line() {
        let held: Vec<Skill> = crate::app::commands()
            .iter()
            .map(|command| skill(command.name.trim_start_matches('/')))
            .chain([skill("add-dir /etc"), skill("bell\u{7}"), skill("ordinary")])
            .collect();

        let offered = matching(&held, "");
        assert_eq!(names(&offered), ["ordinary"], "the control");
        for skill in &offered {
            let line = format!("/{} ", skill.name);
            assert_eq!(
                crate::app::command_typed(&line),
                None,
                "{line:?} is a command"
            );
        }
    }
}
