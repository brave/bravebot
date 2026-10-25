//! What the window says of the ChatGPT account when a turn failed on it or its models could not be
//! listed ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8),
//! [CHATGPT-9](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9)).
//!
//! The window draws one sentence in sentence case that says what happened and what to do, with a
//! command in a code element where one is the remedy. The terminal's sentences are lower case, take
//! the status of a refusal after them and write the command between backticks, none of which a window
//! can draw as it is, so the window has messages of its own in the catalog. They are chosen from the
//! condition alone, as the terminal's are, and repeat nothing a service, a file or a settings file
//! wrote. Which conditions name a command is the agent's to decide
//! ([`bravebot_agent::chatgpt::Remedy`]), so this module keeps only the words.
//!
//! The command is passed into the message and sent beside the sentence, so the window can draw the
//! words that are the command as code without reading the sentence to find them.

use bravebot_agent::chatgpt::{AccountModels, Remedy, remedy_of_failure, remedy_of_listing};
use bravebot_aichat::responses::protocol::Refusal;
use bravebot_aichat::{AccountFailure, RenewalFailure};
use bravebot_i18n::t;

/// A sentence for the window, and the command it names where one is the remedy. A failure's card and a
/// picker's warning are both this.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Said {
    /// One sentence in sentence case, with the command in it as plain words.
    pub(crate) sentence: String,
    /// The words of `sentence` that are the command, for the window to draw as code.
    pub(crate) command: Option<&'static str>,
}

impl Said {
    /// A sentence that names no command.
    pub(crate) fn plain(sentence: impl Into<String>) -> Self {
        Self {
            sentence: sentence.into(),
            command: None,
        }
    }

    /// A sentence that names the command that is its remedy.
    #[cfg(test)]
    pub(crate) fn naming(command: &'static str, sentence: impl Into<String>) -> Self {
        Self {
            sentence: sentence.into(),
            command: Some(command),
        }
    }
}

/// The sentence for a failure of the account.
pub(crate) fn of_failure(failure: AccountFailure) -> Said {
    let command = remedy_of_failure(failure).map(Remedy::command);
    let shown = command.unwrap_or_default();
    let said = |sentence: String| Said { sentence, command };
    match failure {
        AccountFailure::NotSignedIn => said(t!(desktop_chatgpt_not_signed_in, command = shown)),
        AccountFailure::Ended => said(t!(desktop_chatgpt_ended, command = shown)),
        AccountFailure::Refused => said(t!(desktop_chatgpt_token_refused, command = shown)),
        AccountFailure::Unusable => said(t!(desktop_chatgpt_unusable, command = shown)),
        // The wait a refusal asked for at a 429 is said for either refusal, as the terminal says it.
        AccountFailure::Named {
            refusal: Refusal::UsageLimit,
            ..
        } => Said::plain(match failure.wait() {
            Some(wait) => t!(
                desktop_chatgpt_usage_limit_wait,
                minutes = bravebot_agent::outcome::minutes_to_wait(wait)
            ),
            None => t!(desktop_chatgpt_usage_limit).to_string(),
        }),
        AccountFailure::Named {
            refusal: Refusal::NotPermitted,
            ..
        } => Said::plain(match failure.wait() {
            Some(wait) => t!(
                desktop_chatgpt_not_permitted_wait,
                minutes = bravebot_agent::outcome::minutes_to_wait(wait)
            ),
            None => t!(desktop_chatgpt_not_permitted).to_string(),
        }),
        AccountFailure::Renewal(RenewalFailure::Refused) => {
            Said::plain(t!(desktop_chatgpt_renewal_refused))
        }
        AccountFailure::Renewal(RenewalFailure::RateLimited) => {
            Said::plain(t!(desktop_chatgpt_renewal_rate_limited))
        }
        AccountFailure::Renewal(RenewalFailure::Unavailable) => {
            Said::plain(t!(desktop_chatgpt_renewal_unavailable))
        }
        AccountFailure::Renewal(RenewalFailure::Incomplete) => {
            Said::plain(t!(desktop_chatgpt_renewal_incomplete))
        }
        AccountFailure::Renewal(RenewalFailure::Transport) => {
            Said::plain(t!(desktop_chatgpt_renewal_transport))
        }
    }
}

/// What the window narrates where a summary of the conversation could not be made and the account was
/// what failed: that the turn goes on without one, and then the account's sentence, one sentence
/// each.
pub(crate) fn of_summary_failure(failure: AccountFailure) -> [Said; 2] {
    [
        Said::plain(t!(desktop_compaction_failed)),
        of_failure(failure),
    ]
}

/// What the window narrates where a turn moved off its model for the person's fallback model and
/// the account was what failed. Only a renewal that was rate-limited or met an unavailable service
/// reaches it, since every other failure of the account is a category no fallback answers for.
pub(crate) fn of_fall_back(failure: AccountFailure, from: &str, to: &str) -> Said {
    Said::plain(match failure {
        AccountFailure::Renewal(RenewalFailure::RateLimited) => {
            t!(desktop_chatgpt_fell_back_rate_limited, from = from, to = to)
        }
        AccountFailure::Renewal(RenewalFailure::Unavailable) => {
            t!(desktop_chatgpt_fell_back_unavailable, from = from, to = to)
        }
        _ => t!(desktop_chatgpt_fell_back, from = from, to = to),
    })
}

/// The warning the model picker shows where the account's models could not be listed, or `None`
/// where there is nothing to warn of: nobody signed in, or a listing that was read, empty or not.
pub(crate) fn of_listing(account: &AccountModels) -> Option<Said> {
    let command = remedy_of_listing(account).map(Remedy::command);
    let shown = command.unwrap_or_default();
    let said = |sentence: String| Said { sentence, command };
    match account {
        AccountModels::Absent | AccountModels::Offered(_) => None,
        AccountModels::Unreadable => {
            Some(said(t!(desktop_chatgpt_models_unreadable, command = shown)))
        }
        AccountModels::Compiled(_) => {
            Some(said(t!(desktop_chatgpt_models_compiled, command = shown)))
        }
        AccountModels::Refused(Refusal::UsageLimit) => {
            Some(Said::plain(t!(desktop_chatgpt_models_usage_limit)))
        }
        AccountModels::Refused(Refusal::NotPermitted) => {
            Some(Said::plain(t!(desktop_chatgpt_models_not_permitted)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Every condition the account can fail in, with the wait a usage limit carries and without.
    fn every_failure() -> Vec<AccountFailure> {
        let refusal = |refusal, status, retry_after| AccountFailure::Named {
            refusal,
            status,
            retry_after,
        };
        vec![
            AccountFailure::NotSignedIn,
            AccountFailure::Ended,
            AccountFailure::Refused,
            AccountFailure::Unusable,
            refusal(Refusal::UsageLimit, None, None),
            refusal(
                Refusal::UsageLimit,
                Some(429),
                Some(Duration::from_secs(600)),
            ),
            refusal(Refusal::UsageLimit, Some(429), Some(Duration::from_secs(5))),
            refusal(Refusal::NotPermitted, Some(403), None),
            refusal(
                Refusal::NotPermitted,
                Some(429),
                Some(Duration::from_secs(61)),
            ),
            AccountFailure::Renewal(RenewalFailure::Refused),
            AccountFailure::Renewal(RenewalFailure::RateLimited),
            AccountFailure::Renewal(RenewalFailure::Unavailable),
            AccountFailure::Renewal(RenewalFailure::Incomplete),
            AccountFailure::Renewal(RenewalFailure::Transport),
        ]
    }

    /// What every sentence of the window is held to: one sentence in sentence case, with no backtick
    /// that a paragraph would print as it is, and its command in it once so the window finds the
    /// words to draw as code, where it names one.
    fn assert_one_sentence_naming(said: &Said, what: &str) {
        let sentence = &said.sentence;
        let first = sentence.chars().next().expect("a sentence");
        assert!(first.is_uppercase(), "{what}: {sentence}");
        assert!(sentence.ends_with('.'), "{what}: {sentence}");
        assert_eq!(
            sentence.matches('.').count(),
            1,
            "{what} is not one sentence: {sentence}"
        );
        assert!(!sentence.contains('`'), "{what}: {sentence}");
        assert!(
            !sentence.contains('{') && !sentence.contains('}'),
            "{what}: {sentence}"
        );
        match said.command {
            Some(command) => assert_eq!(
                sentence.matches(command).count(),
                1,
                "{what} does not carry its command once: {sentence}"
            ),
            None => assert!(!sentence.contains("bravebot "), "{what}: {sentence}"),
        }
    }

    /// The design skill's microcopy for an error: what happened and what to do, in one sentence, in
    /// sentence case, and a command in a code element, which the window draws from `command` and
    /// not from backticks that a paragraph would print as they are. The window's sentences differ
    /// from one another and name a command only where one is the remedy. The regressions it rejects
    /// are the terminal's lower case or two-sentence wording reaching the window, backticks in a
    /// paragraph, and a command named in words the window cannot find.
    #[test]
    fn the_windows_sentence_is_one_sentence_in_sentence_case_and_names_its_command_once() {
        let mut sentences = Vec::new();
        for failure in every_failure() {
            let said = of_failure(failure);
            assert_one_sentence_naming(&said, &format!("{failure:?}"));
            sentences.push(said.sentence);
        }
        let mut distinct = sentences.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), sentences.len(), "{sentences:#?}");
    }

    /// Where signing in again, or looking at the stored sign-in, is the remedy the sentence names that
    /// command, and nowhere else does: a usage limit, a refusal that says the account may not be used
    /// this way and every failed renewal are not mended by signing in again, as
    /// [CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8) lists them.
    #[test]
    fn the_command_is_named_where_it_is_the_remedy() {
        let command_of = |failure| of_failure(failure).command;
        assert_eq!(
            command_of(AccountFailure::NotSignedIn),
            Some(Remedy::SignIn.command())
        );
        assert_eq!(
            command_of(AccountFailure::Ended),
            Some(Remedy::SignIn.command())
        );
        assert_eq!(
            command_of(AccountFailure::Refused),
            Some(Remedy::SignIn.command())
        );
        assert_eq!(
            command_of(AccountFailure::Unusable),
            Some(Remedy::Status.command())
        );
        for failure in every_failure() {
            if matches!(
                failure,
                AccountFailure::Named { .. } | AccountFailure::Renewal(_)
            ) {
                assert_eq!(command_of(failure), None, "{failure:?}");
            }
        }
    }

    /// A wait a refusal asked for at a 429 is said for either refusal, in minutes rounded up and in
    /// the singular for one, as the terminal and the one-shot run say it, and one that came with
    /// another status or on a stream says none. The rule is
    /// [CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)'s: the service's `Retry-After` is
    /// told to the person wherever the failure is, and which refusal it was does not decide it.
    #[test]
    fn a_wait_asked_for_at_a_429_is_said_in_minutes_for_either_refusal() {
        let refusal = |refusal, status, wait: Option<u64>| {
            of_failure(AccountFailure::Named {
                refusal,
                status,
                retry_after: wait.map(Duration::from_secs),
            })
            .sentence
        };
        for kind in [Refusal::UsageLimit, Refusal::NotPermitted] {
            let eleven = refusal(kind, Some(429), Some(601));
            assert!(eleven.contains("about 11 minutes"), "{kind:?}: {eleven}");
            let one = refusal(kind, Some(429), Some(5));
            assert!(
                one.contains("about 1 minute") && !one.contains("1 minutes"),
                "{kind:?}: {one}"
            );
            let none = refusal(kind, Some(429), None);
            assert!(!none.contains("minute"), "{kind:?}: {none}");
            for status in [Some(403), Some(401), None] {
                let unasked = refusal(kind, status, Some(600));
                assert!(
                    !unasked.contains("minute"),
                    "{kind:?} {status:?}: {unasked}"
                );
            }
        }
        // The two refusals stay told apart whatever the wait.
        assert!(refusal(Refusal::UsageLimit, Some(429), Some(600)).contains("usage limit"));
        assert!(!refusal(Refusal::NotPermitted, Some(429), Some(600)).contains("usage limit"));
    }

    /// What the window narrates where a summary could not be made and the account failed is two
    /// sentences, each one line: that the turn goes on, and the account's own sentence with its
    /// command. A move to the fallback model names no command and says which model it moves from and
    /// to, and a failure that cannot reach it is still said in a sentence.
    #[test]
    fn the_notices_of_a_turn_that_goes_on_after_the_account_failed_are_sentences_of_the_window() {
        for failure in every_failure() {
            let [goes_on, why] = of_summary_failure(failure);
            assert_one_sentence_naming(&goes_on, "the line that the turn goes on");
            assert!(goes_on.command.is_none());
            assert_eq!(why, of_failure(failure), "{failure:?}");
        }
        for failure in every_failure() {
            let said = of_fall_back(failure, "chatgpt/gpt-a", "chatgpt/gpt-b");
            assert_one_sentence_naming(&said, &format!("{failure:?}"));
            assert!(said.command.is_none(), "{failure:?}");
            assert!(
                said.sentence.contains("chatgpt/gpt-a") && said.sentence.contains("chatgpt/gpt-b"),
                "{failure:?}: {}",
                said.sentence
            );
        }
        let sentence = |failure| of_fall_back(failure, "a", "b").sentence;
        assert!(
            sentence(AccountFailure::Renewal(RenewalFailure::RateLimited))
                .contains("limiting requests")
        );
        assert!(
            sentence(AccountFailure::Renewal(RenewalFailure::Unavailable)).contains("unavailable")
        );
    }

    /// The model picker's warnings are held to the same rule, are told apart, and say nothing where
    /// there is nothing to warn of: a machine nobody signed in on, and a listing that was read, empty
    /// or not. The compiled list's warning says that the list is built in, and none of them names the
    /// terminal's `/model`, which is a command of another front end.
    #[test]
    fn each_warning_of_the_picker_is_one_sentence_and_says_nothing_where_nothing_failed() {
        use bravebot_aichat::models::Model;
        let none: Vec<Model> = Vec::new();
        assert_eq!(of_listing(&AccountModels::Absent), None);
        assert_eq!(of_listing(&AccountModels::Offered(none)), None);

        let all = [
            AccountModels::Unreadable,
            AccountModels::Compiled(Vec::new()),
            AccountModels::Refused(Refusal::UsageLimit),
            AccountModels::Refused(Refusal::NotPermitted),
        ];
        let mut sentences = Vec::new();
        for account in &all {
            let said = of_listing(account).expect("a warning");
            assert_one_sentence_naming(&said, &format!("{account:?}"));
            assert!(!said.sentence.contains("/model"), "{}", said.sentence);
            sentences.push(said.sentence);
        }
        assert!(sentences[1].contains("built in"), "{}", sentences[1]);
        assert!(!sentences[0].contains("built in"), "{}", sentences[0]);
        let mut distinct = sentences.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), sentences.len(), "{sentences:#?}");
        // Signing in again changes neither refusal, so neither warning sends the person to do it.
        for refusal in [&all[2], &all[3]] {
            let said = of_listing(refusal).expect("a warning");
            assert_eq!(said.command, None, "{refusal:?}");
        }
    }
}
