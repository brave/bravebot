//! Prompt files: `~/.bravebot/prompts/<name>.md`, expanded into the box by `/name args` (CMD-17).
//!
//! The expansion writes text into the box and sends nothing, so what is sent is a line a person
//! saw. Only the person's own directory is read: a workspace is never asked for a `prompts/`
//! directory, and no name is listed from disk. A name is looked up, once, when Enter is pressed on a
//! line that starts with it.

use std::io::Read;
use std::path::{Path, PathBuf};

use bravebot_agent::skills::{body_after_frontmatter, declarations};

/// The directory inside the user's own `~/.bravebot` that holds the files.
const DIRECTORY: &str = "prompts";

/// The only extension a prompt file has.
const EXTENSION: &str = "md";

/// The most a file may hold, so a mistaken link cannot put a log into the box.
pub const LIMIT: u64 = 64 * 1024;

/// Where the person's prompt files live, or `None` where the platform names no home.
pub fn directory() -> Option<PathBuf> {
    bravebot_agent::home::directory().map(|home| home.join(DIRECTORY))
}

/// What a line turns out to be, once its first word has been looked up.
#[derive(Debug, PartialEq, Eq)]
pub enum Expansion {
    /// No file is named by the line, so it is a prompt and is sent as typed.
    NotOne,
    /// The text that replaces the line in the box.
    Expanded(String),
    /// A file is named and cannot be used. The line stays as typed and nothing is sent.
    Refused { name: String, why: Why },
}

/// Why a named file was not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Not a regular file, not readable, or not text.
    Unreadable,
    /// Longer than [`LIMIT`].
    TooLarge,
    /// Nothing but front matter and blank lines.
    Empty,
    /// The `agent` key is not one word.
    AgentNotAName,
    /// The text would begin with a word a command claims, so the next Enter would carry out a
    /// command that came out of a file (CMD-1).
    BeginsWithACommand,
}

/// Look the line's first word up in `directory` and expand the file it names.
///
/// `line` is what the box held with any folded paste put back to its words.
pub fn expand(directory: &Path, line: &str) -> Expansion {
    let Some((name, arguments)) = split(line) else {
        return Expansion::NotOne;
    };
    let path = directory.join(format!("{name}.{EXTENSION}"));
    // Checked before opening: opening a pipe blocks until something writes to it. A path that
    // cannot be examined at all (no directory, a name too long for the platform) names no file,
    // so the line is a prompt like any other.
    let Ok(metadata) = std::fs::metadata(&path) else {
        return Expansion::NotOne;
    };
    if !metadata.is_file() {
        return refused(name, Why::Unreadable);
    }
    let mut bytes = Vec::new();
    let read =
        std::fs::File::open(&path).and_then(|file| file.take(LIMIT + 1).read_to_end(&mut bytes));
    if read.is_err() {
        return refused(name, Why::Unreadable);
    }
    if bytes.len() as u64 > LIMIT {
        return refused(name, Why::TooLarge);
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return refused(name, Why::Unreadable);
    };

    let body = substitute(body_after_frontmatter(&text), arguments);
    let body = body.trim();
    if body.is_empty() {
        return refused(name, Why::Empty);
    }
    let agent = declarations(&text)
        .and_then(|declared| declared.get("agent").cloned())
        .map(|agent| agent.trim().to_string())
        .filter(|agent| !agent.is_empty());
    match agent {
        None if begins_with_a_command(body) => refused(name, Why::BeginsWithACommand),
        None => Expansion::Expanded(body.to_string()),
        Some(agent) if is_one_word(&agent) => {
            Expansion::Expanded(format!("{} {agent} {body}", crate::app::AGENT_COMMAND))
        }
        Some(_) => refused(name, Why::AgentNotAName),
    }
}

fn refused(name: &str, why: Why) -> Expansion {
    Expansion::Refused {
        name: name.to_string(),
        why,
    }
}

/// The name a line starts with and what follows it, or `None` where the first word cannot name a
/// file.
///
/// A name is letters, digits, `-` and `_`, so it cannot climb out of the directory or name a
/// hidden file. A word a command claims is never a name: `/init the project` is a prompt (CMD-2),
/// and it stays one however `init.md` reads.
fn split(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim().strip_prefix('/')?;
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let (name, arguments) = rest.split_at(end);
    let well_formed = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    (well_formed && !claimed_by_a_command(name)).then(|| (name, arguments.trim()))
}

fn claimed_by_a_command(word: &str) -> bool {
    crate::app::commands()
        .iter()
        .any(|command| command.name.strip_prefix('/') == Some(word))
}

fn begins_with_a_command(text: &str) -> bool {
    text.strip_prefix('/').is_some_and(|rest| {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        claimed_by_a_command(&rest[..end])
    })
}

fn is_one_word(word: &str) -> bool {
    !word.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// `$ARGUMENTS` becomes everything after the name, and `$1` to `$9` the whitespace-separated words
/// of it, or nothing where the line has fewer.
///
/// One pass over the template, so text that came from the line is never read for another `$`.
fn substitute(template: &str, arguments: &str) -> String {
    let words: Vec<&str> = arguments.split_whitespace().collect();
    let mut out = String::with_capacity(template.len() + arguments.len());
    let mut rest = template;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        if let Some(tail) = after.strip_prefix("ARGUMENTS") {
            out.push_str(arguments);
            rest = tail;
        } else if let Some(digit) = after.chars().next().filter(|c| ('1'..='9').contains(c)) {
            let place = digit as usize - '1' as usize;
            out.push_str(words.get(place).copied().unwrap_or(""));
            rest = &after[1..];
        } else {
            out.push('$');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory = crate::testutil::scratch_dir(name);
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn expanded(directory: &Path, line: &str) -> String {
        match expand(directory, line) {
            Expansion::Expanded(text) => text,
            other => panic!("{line:?} did not expand: {other:?}"),
        }
    }

    /// The whole tail is `$ARGUMENTS`, the words are `$1` to `$9`, and a word the line lacks is
    /// nothing rather than the placeholder.
    #[test]
    fn arguments_and_positions_are_filled_from_what_followed_the_name() {
        let directory = scratch("prompt-files-fill");
        std::fs::write(
            directory.join("triage.md"),
            "Review $1 for $2.\nAll: $ARGUMENTS\nThird: [$3]\n",
        )
        .unwrap();

        assert_eq!(
            expanded(&directory, "/triage the diff  races"),
            "Review the for diff.\nAll: the diff  races\nThird: [races]"
        );
        assert_eq!(
            expanded(&directory, "/triage"),
            "Review  for .\nAll: \nThird: []"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// What the person typed is data: a `$1` or `$ARGUMENTS` inside it is not read again, and the
    /// words of the template that sit beside a `$` are left alone.
    #[test]
    fn text_that_came_from_the_line_is_not_read_for_placeholders() {
        assert_eq!(
            substitute("A $1 B $2", "$2 $ARGUMENTS"),
            "A $2 B $ARGUMENTS"
        );
        assert_eq!(substitute("$ARGUMENTS and $1", "x $1"), "x $1 and x");
        assert_eq!(
            substitute("cost $5.00 or $0 or $", ""),
            "cost .00 or $0 or $"
        );
    }

    /// The file is the template and the front matter is not part of it.
    #[test]
    fn front_matter_is_not_put_in_the_box() {
        let directory = scratch("prompt-files-front-matter");
        std::fs::write(
            directory.join("plain.md"),
            "---\ndescription: Look at a diff\nmodel: some-model\n---\nLook at $ARGUMENTS\n",
        )
        .unwrap();

        assert_eq!(expanded(&directory, "/plain this"), "Look at this");
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// `agent` fills the `/agent` form with the body as its task, so the line is not sent until
    /// the person presses Enter on a form they can see.
    #[test]
    fn an_agent_in_the_front_matter_fills_the_agent_form() {
        let directory = scratch("prompt-files-agent");
        std::fs::write(
            directory.join("audit.md"),
            "---\nagent: auditor\n---\nAudit $ARGUMENTS\n",
        )
        .unwrap();

        assert_eq!(
            expanded(&directory, "/audit the parser"),
            "/agent auditor Audit the parser"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// An `agent` of several words would put part of itself into the task.
    #[test]
    fn an_agent_that_is_not_one_word_is_refused() {
        let directory = scratch("prompt-files-agent-words");
        std::fs::write(
            directory.join("audit.md"),
            "---\nagent: two words\n---\nAudit\n",
        )
        .unwrap();

        assert_eq!(
            expand(&directory, "/audit"),
            Expansion::Refused {
                name: "audit".to_string(),
                why: Why::AgentNotAName
            }
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A first word is a name only when it is plain, so it cannot reach a file outside the
    /// directory, and it names nothing where the file is not there.
    #[test]
    fn only_a_plain_name_is_looked_up_and_only_inside_the_directory() {
        let outer = scratch("prompt-files-names");
        let directory = outer.join("prompts");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(outer.join("secret.md"), "outside").unwrap();
        std::fs::write(directory.join(".hidden.md"), "hidden").unwrap();
        std::fs::write(directory.join("here.md"), "inside").unwrap();

        for line in [
            "/../secret",
            "/..%2fsecret",
            "/sub/../../secret",
            "/.hidden",
            "/",
            "/ here",
            "here",
            "/missing words",
        ] {
            assert_eq!(expand(&directory, line), Expansion::NotOne, "{line:?}");
        }
        assert_eq!(expanded(&directory, "/here"), "inside");
        let _ = std::fs::remove_dir_all(&outer);
    }

    /// A word a command claims is that command's, so a file by its name cannot turn a line the
    /// table calls a prompt into a different prompt.
    #[test]
    fn a_name_a_command_claims_is_never_a_file() {
        let directory = scratch("prompt-files-claimed");
        for command in crate::app::commands() {
            let name = command.name.trim_start_matches('/');
            std::fs::write(directory.join(format!("{name}.md")), "from a file").unwrap();
            assert_eq!(
                expand(&directory, &format!("/{name} and words")),
                Expansion::NotOne,
                "/{name}"
            );
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A command is dispatched only from a line a person typed (CMD-1), and text read out of a file
    /// is not that, so a body that opens with a command word is not put in the box.
    #[test]
    fn a_body_that_opens_with_a_command_is_refused() {
        let directory = scratch("prompt-files-body-command");
        std::fs::write(directory.join("wipe.md"), "/clear\n").unwrap();
        std::fs::write(directory.join("arg.md"), "/$1\n").unwrap();
        std::fs::write(directory.join("skill.md"), "/not-a-command here\n").unwrap();

        let why = |line: &str| match expand(&directory, line) {
            Expansion::Refused { why, .. } => Some(why),
            _ => None,
        };
        assert_eq!(why("/wipe"), Some(Why::BeginsWithACommand));
        assert_eq!(
            why("/arg clear"),
            Some(Why::BeginsWithACommand),
            "an argument can spell the command"
        );
        assert_eq!(why("/skill"), None);
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A prompt that starts with a slash word is a prompt wherever the directory is not usable.
    #[test]
    fn a_directory_that_is_not_one_leaves_the_line_a_prompt() {
        let outer = scratch("prompt-files-not-a-directory");
        let not_a_directory = outer.join("prompts");
        std::fs::write(&not_a_directory, "a file where the directory belongs").unwrap();

        assert_eq!(
            expand(&not_a_directory, "/tmp is full, why?"),
            Expansion::NotOne
        );
        assert_eq!(
            expand(&outer, &format!("/{}", "a".repeat(5000))),
            Expansion::NotOne
        );
        let _ = std::fs::remove_dir_all(&outer);
    }

    /// A device is not a file, and a name linked to one is refused rather than read to the limit.
    #[cfg(unix)]
    #[test]
    fn a_name_linked_to_a_device_is_not_read() {
        let directory = scratch("prompt-files-device");
        std::os::unix::fs::symlink("/dev/zero", directory.join("zero.md")).unwrap();

        assert_eq!(
            expand(&directory, "/zero"),
            Expansion::Refused {
                name: "zero".to_string(),
                why: Why::Unreadable
            }
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A file that exists and cannot be used is said so, and is not the same as no file.
    #[test]
    fn a_file_that_cannot_be_used_is_refused_rather_than_ignored() {
        let directory = scratch("prompt-files-refused");
        std::fs::create_dir_all(directory.join("folder.md")).unwrap();
        std::fs::write(directory.join("binary.md"), [0xff, 0xfe, 0x00]).unwrap();
        std::fs::write(
            directory.join("blank.md"),
            "---\ndescription: x\n---\n\n  \n",
        )
        .unwrap();
        std::fs::write(directory.join("exact.md"), "a".repeat(LIMIT as usize)).unwrap();
        std::fs::write(directory.join("long.md"), "a".repeat(LIMIT as usize + 1)).unwrap();

        let why = |line: &str| match expand(&directory, line) {
            Expansion::Refused { why, .. } => Some(why),
            _ => None,
        };
        assert_eq!(why("/folder"), Some(Why::Unreadable));
        assert_eq!(why("/binary"), Some(Why::Unreadable));
        assert_eq!(why("/blank"), Some(Why::Empty));
        assert_eq!(why("/long"), Some(Why::TooLarge));
        assert_eq!(why("/exact"), None, "the limit itself is allowed");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
