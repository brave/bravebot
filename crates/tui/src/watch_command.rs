//! The argument to `/watch`.
//!
//! The mechanism a watch is, and every bound on it, is [`bravebot_agent::watch`], because a watch
//! is not a terminal thing and a front end that draws nothing keeps them on the same terms. What
//! is here is the half that *is* a terminal thing: the words a person types at this interface to
//! see what is live and to end one.

/// The word that ends a watch, rather than naming one.
const STOP: &str = "stop";

/// What the argument to `/watch` asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// The bare word: list the live watches, or say there are none.
    List,
    /// End the watch with this number.
    Stop(usize),
    /// Anything else, answered by saying what the command takes.
    Unreadable,
}

/// Read the argument to `/watch`.
///
/// Two forms and nothing else. A watch is armed by asking for one in a prompt, so there is no
/// form here that arms one: what a person needs from a command is to see what is live and to end
/// one of them, which are the two things a transcript cannot tell them.
pub fn parse(argument: &str) -> Asked {
    let argument = argument.trim();
    if argument.is_empty() {
        return Asked::List;
    }
    match argument.split_once(char::is_whitespace) {
        Some((STOP, number)) => match number.trim().parse::<usize>() {
            Ok(number) => Asked::Stop(number),
            Err(_) => Asked::Unreadable,
        },
        _ => Asked::Unreadable,
    }
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

    /// Answered by saying what the command takes rather than by guessing which watch was meant:
    /// ending the wrong one is the mistake a guess makes here.
    #[test]
    fn anything_else_is_answered_by_saying_what_the_command_takes() {
        for argument in ["stop", "stop all", "3", "stop three", "list", "src/main.rs"] {
            assert_eq!(parse(argument), Asked::Unreadable, "{argument}");
        }
    }
}
