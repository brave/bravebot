//! The argument to `/schedule`, cut into the words `bravebot schedule` would have received.
//!
//! A client that runs [`crate::in_session`] cuts the typed line with [`split`] and decides with
//! [`reads_only`] whether it may be carried out while a turn runs (CMD-8), so the terminal and the
//! desktop cut a line the same way and wait for the same words.

/// The subcommands that read what is stored and change nothing, so they are carried out as typed
/// while a turn runs (CMD-8). Every other subcommand writes the entries or starts a run, and waits.
const READS: [&str; 3] = ["list", "pending", "history"];

/// Every subcommand `bravebot schedule` takes, in the order a session offers them when Tab
/// completes the word after `/schedule`. Words of this program's own, so completing one reads
/// nothing a person did not type.
pub const SUBCOMMANDS: [&str; 9] = [
    "list", "add", "remove", "pause", "resume", "pending", "clear", "history", "run-now",
];

/// Whether the argument names a subcommand that only reads. No word at all is also one: the
/// answer is the usage sentence and nothing is read or changed. A line with an unclosed quote
/// waits, because what it would do is not known until the quote is closed.
pub fn reads_only(argument: &str) -> bool {
    split(argument).is_some_and(|words| match words.first() {
        None => true,
        Some(first) => READS.contains(&first.as_str()),
    })
}

/// The words of the argument as a shell would have passed them to `bravebot schedule`: split at
/// white space, with a pair of single or double quotes holding white space inside one word and
/// disappearing. There is no escape character and nothing is expanded, so `$HOME` and `*` stay as
/// typed. `None` where a quote is opened and not closed, because a guess at where it should end
/// could store a prompt the person did not write.
pub fn split(argument: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    for c in argument.chars() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, c) => {
                word.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if started {
        words.push(word);
    }
    Some(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(argument: &str) -> Vec<String> {
        split(argument).expect("closed quotes")
    }

    /// Rejects a splitter that cuts at every space: the cron expression and the prompt would
    /// reach `add` as many words and be refused or stored wrongly.
    #[test]
    fn a_quoted_cron_expression_and_prompt_are_one_word_each() {
        assert_eq!(
            words(r#"add --cron "0 9 * * 1-5" --prompt 'summarize the news'"#),
            [
                "add",
                "--cron",
                "0 9 * * 1-5",
                "--prompt",
                "summarize the news"
            ]
        );
    }

    /// Rejects a splitter that drops an empty quoted word, which would turn `--prompt ""` into a
    /// missing value and shift the words after it.
    #[test]
    fn an_empty_pair_of_quotes_is_an_empty_word() {
        assert_eq!(words(r#"add --prompt "" x"#), ["add", "--prompt", "", "x"]);
    }

    /// Rejects a splitter that expands anything, or reads a backslash as an escape.
    #[test]
    fn nothing_is_expanded_and_there_is_no_escape() {
        assert_eq!(words(r"add --dir $HOME/*"), ["add", "--dir", "$HOME/*"],);
        assert_eq!(words(r"a\ b"), [r"a\", "b"]);
    }

    /// Rejects a splitter that closes an open quote at the end of the line and stores what it
    /// guessed the person meant.
    #[test]
    fn an_unclosed_quote_is_not_guessed_at() {
        assert_eq!(split(r#"add --prompt "never ends"#), None);
        assert_eq!(split("add --prompt 'x"), None);
    }

    /// Rejects a splitter that keeps leading or repeated white space as empty words.
    #[test]
    fn white_space_between_words_is_not_a_word() {
        assert_eq!(words("  list   "), ["list"]);
        assert_eq!(words(""), Vec::<String>::new());
    }

    /// Rejects a completion list that leaves out a reader or repeats a word: every subcommand
    /// carried out as typed while a turn runs is one Tab offers, and each word is offered once.
    #[test]
    fn every_reader_is_offered_for_completion_once() {
        for read in READS {
            assert!(SUBCOMMANDS.contains(&read), "{read}");
        }
        let mut sorted = SUBCOMMANDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), SUBCOMMANDS.len());
    }

    /// Rejects a classifier that lets a writing subcommand run mid-turn, or one that lets a
    /// quoted word spelled like a reader through, or one that queues the bare word, which only
    /// says what the command takes.
    #[test]
    fn only_the_three_readers_and_the_bare_word_are_carried_out_while_a_turn_runs() {
        for argument in [
            "",
            "  ",
            "list",
            "pending",
            "pending 3",
            "history",
            "history 2",
            "  list",
        ] {
            assert!(reads_only(argument), "{argument}");
        }
        for argument in [
            "add --cron '* * * * *' --prompt x",
            "remove 1",
            "pause 1",
            "resume 1",
            "clear 1",
            "run-now 1",
            "listing",
            "list'",
        ] {
            assert!(!reads_only(argument), "{argument}");
        }
    }
}
