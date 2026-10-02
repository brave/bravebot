//! The argument to `/checkouts`.
//!
//! A checkout is made, kept and removed by [`bravebot_agent::workspace`]. What is here is the half
//! that is a terminal thing: the words a person types to see the checkouts the session keeps and to
//! remove one.

/// The word that removes a checkout, rather than naming one.
const REMOVE: &str = "remove";

/// What the argument to `/checkouts` asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The bare word: list the checkouts the session keeps, or say there are none.
    List,
    /// Remove the checkout with this number, spelled the way the list spells it.
    Remove(String),
    /// Anything else, answered by saying what the command takes.
    Unreadable,
}

/// Read the argument to `/checkouts`.
///
/// A number is taken as the list prints it, `c2`, or bare, `2`. Anything else is unreadable rather
/// than a guess, since removing the wrong checkout deletes work.
pub fn parse(argument: &str) -> Asked {
    let argument = argument.trim();
    if argument.is_empty() {
        return Asked::List;
    }
    let Some((REMOVE, named)) = argument.split_once(char::is_whitespace) else {
        return Asked::Unreadable;
    };
    let named = named.trim();
    let digits = named.strip_prefix('c').unwrap_or(named);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Asked::Unreadable;
    }
    match digits.parse::<u64>() {
        Ok(number) => Asked::Remove(format!("c{number}")),
        Err(_) => Asked::Unreadable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bare_word_lists_the_checkouts() {
        assert_eq!(parse(""), Asked::List);
        assert_eq!(parse("   "), Asked::List);
    }

    #[test]
    fn remove_and_a_number_removes_that_checkout() {
        assert_eq!(parse("remove c3"), Asked::Remove("c3".to_string()));
        assert_eq!(parse("remove   12  "), Asked::Remove("c12".to_string()));
        assert_eq!(parse("remove c007"), Asked::Remove("c7".to_string()));
    }

    #[test]
    fn anything_else_is_answered_by_saying_what_the_command_takes() {
        for argument in [
            "remove",
            "remove all",
            "remove c",
            "remove c-1",
            "remove +1",
            "remove c1 c2",
            "c1",
            "list",
            "delete c1",
            "remove 99999999999999999999999",
        ] {
            assert_eq!(parse(argument), Asked::Unreadable, "{argument}");
        }
    }
}
