//! The argument to `/watch`.
//!
//! The mechanism a watch is, and every bound on it, is [`bravebot_agent::watch`], because a watch
//! is not a terminal thing and a front end that draws nothing keeps them on the same terms. What
//! is here is the half that *is* a terminal thing: the words a person types at this interface to
//! see what is live, to end one, and to arm one on a path they name.

/// The word that ends a watch, rather than naming one.
const STOP: &str = "stop";

/// What the argument to `/watch` asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The bare word: list the live watches, or say there are none.
    List,
    /// End the watch with this number.
    Stop(usize),
    /// Arm a watch on the path the person typed, with one leading `@` taken off.
    Arm(String),
    /// Anything else, answered by saying what the command takes.
    Unreadable,
}

/// Read the argument to `/watch`.
///
/// Three forms. No argument lists the live watches, `stop <n>` ends one, and any other argument
/// is a path to arm a watch on. The path is the whole of the rest of the line, so a name with
/// spaces in it works, and one leading `@` is taken off so the file completer's form is accepted.
///
/// A bare number is not read as a path: it is almost always `stop <n>` with the word left out,
/// and a watch on a file named `3` would be a guess. A path with a control character in it is
/// refused, as the desktop refuses one.
pub fn parse(argument: &str) -> Asked {
    let argument = argument.trim();
    if argument.is_empty() {
        return Asked::List;
    }
    let first = argument
        .split_once(char::is_whitespace)
        .map_or(argument, |(first, _)| first);
    if first == STOP {
        return match argument[STOP.len()..].trim().parse::<usize>() {
            Ok(number) => Asked::Stop(number),
            Err(_) => Asked::Unreadable,
        };
    }
    if argument.parse::<usize>().is_ok() {
        return Asked::Unreadable;
    }
    let path = argument.strip_prefix('@').unwrap_or(argument);
    if path.is_empty() || path.chars().any(char::is_control) {
        return Asked::Unreadable;
    }
    Asked::Arm(path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bare_word_lists_what_is_live() {
        assert_eq!(parse(""), Asked::List);
        assert_eq!(parse("   "), Asked::List);
    }

    #[test]
    fn stop_and_a_number_ends_that_watch() {
        assert_eq!(parse("stop 3"), Asked::Stop(3));
        assert_eq!(parse("stop   12  "), Asked::Stop(12));
    }

    #[test]
    fn a_path_arms_a_watch_on_it() {
        assert_eq!(parse("src/main.rs"), Asked::Arm("src/main.rs".to_string()));
        assert_eq!(parse("  @notes.md "), Asked::Arm("notes.md".to_string()));
        assert_eq!(parse("my notes.md"), Asked::Arm("my notes.md".to_string()));
        assert_eq!(
            parse("stopwatch.md"),
            Asked::Arm("stopwatch.md".to_string())
        );
    }

    /// Answered by saying what the command takes rather than by guessing which watch was meant:
    /// ending the wrong one is the mistake a guess makes here.
    #[test]
    fn anything_else_is_answered_by_saying_what_the_command_takes() {
        for argument in ["stop", "stop all", "3", "stop three", "@", "a\u{7}b.md"] {
            assert_eq!(parse(argument), Asked::Unreadable, "{argument}");
        }
    }
}
