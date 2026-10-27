//! `bravebot auth login xai`, `logout xai` and what `status` and the list say about it
//! ([XAI-7](../../../docs/specs/xai-sign-in.md#XAI-7),
//! [XAI-8](../../../docs/specs/xai-sign-in.md#XAI-8),
//! [XAI-9](../../../docs/specs/xai-sign-in.md#XAI-9),
//! [XAI-10](../../../docs/specs/xai-sign-in.md#XAI-10) and
//! [XAI-11](../../../docs/specs/xai-sign-in.md#XAI-11)).
//!
//! The flow itself is in `bravebot_skus::xai`. This is what a person at a terminal sees of it: the
//! refusals that need no answer, the address and the code, and the sentence for each way it ends.

use crate::exit::{Ending, fail};
use bravebot_agent::xai::said_of_class;
use bravebot_config::Config;
use bravebot_i18n::t;
use bravebot_net::Transport;
use bravebot_skus::xai::{Account, AccountError, Announcement, Defect, Hosts, SignInError};
use std::io::{IsTerminal, Write};
use std::process::ExitCode;

/// What decides whether `bravebot auth login xai` may start, none of which needs an answer.
struct Facts {
    incognito: bool,
    /// A provider block with the id `xai` is a gateway.
    named_by_a_gateway: bool,
    stderr_is_a_terminal: bool,
}

/// Why the command stops before it starts, with the ending that says so, or `None`.
///
/// The mode is checked first, since the sign-in is refused there whatever else is true, then the
/// name, then the terminal.
fn refusal(facts: &Facts) -> Option<(Ending, String)> {
    // The sign-in is a write to `~/.bravebot` that the mode exists to decline. Doing it and
    // discarding the result would leave a session that was signed in once and never again.
    if facts.incognito {
        return Some((Ending::Failed, t!(xai_not_while_incognito).into()));
    }
    // A block of this name is a gateway and keeps the name, so one name cannot select two services
    // ([XAI-13](../../../docs/specs/xai-sign-in.md#XAI-13)).
    if facts.named_by_a_gateway {
        return Some((Ending::Configuration, t!(xai_name_is_a_gateway).into()));
    }
    // The address and the code are shown on stderr, so a run that cannot show them to anyone cannot
    // sign in.
    if !facts.stderr_is_a_terminal {
        return Some((Ending::Argument, t!(xai_needs_a_terminal).into()));
    }
    None
}

/// `bravebot auth login xai`.
///
/// Everything that needs no answer is checked before the code is asked for, and each refusal
/// writes nothing and sends nothing.
pub(crate) fn login() -> ExitCode {
    let incognito = bravebot_core::incognito::engaged();
    // A configuration that cannot be read cannot say whether a block names the service. It is not
    // read in an incognito session, where the answer would not change the refusal.
    let config = match Config::from_env() {
        _ if incognito => None,
        Ok(config) => Some(config),
        Err(problem) => return fail(Ending::Configuration, problem),
    };
    let named_by_a_gateway = config
        .as_ref()
        .is_some_and(|config| !config.offers_xai_account());
    let facts = Facts {
        incognito,
        named_by_a_gateway,
        stderr_is_a_terminal: std::io::stderr().is_terminal(),
    };
    if let Some((ending, refused)) = refusal(&facts) {
        return fail(ending, refused);
    }
    let signed_in = bravebot_skus::xai::sign_in(
        &Hosts::compiled_in(),
        Transport::shared().agent_config_builder(),
        announce,
    );
    match signed_in {
        Ok(signed_in) => {
            // A standard output that cannot be written to has nobody to tell, and the sign-in is
            // stored whatever was said. Whether a start would ask which model to use is read once
            // the sign-in is stored, since it depends on it.
            let asks = config.as_ref().is_some_and(crate::model::a_start_would_ask);
            let mut shown = std::io::stdout();
            writeln!(
                shown,
                "{}",
                t!(
                    xai_stored,
                    expires = signed_in.expires,
                    path = crate::progress::printable_path(&signed_in.stored_in)
                )
            )
            .unwrap_or(());
            writeln!(shown, "{}", what_to_do_next(asks)).unwrap_or(());
            ExitCode::SUCCESS
        }
        Err(stopped) => fail(Ending::Failed, said_of_sign_in(&stopped)),
    }
}

/// What the command says after the sign-in is stored, which names only commands that work as typed:
/// `bravebot` on its own only where a start would ask which of the account's models to use, and
/// otherwise a command that names one together with the prompt or the task that starts something.
/// `/model` is named only as what lists the models once a session is open, since the sign-in does not
/// open one.
fn what_to_do_next(a_start_would_ask: bool) -> &'static str {
    match a_start_would_ask {
        true => t!(xai_use_by_asking),
        false => t!(xai_use_by_naming),
    }
}

fn announce(announcement: &Announcement<'_>) {
    announce_to(announcement, &mut std::io::stderr());
}

/// [`announce`], with the stream the lines go to stated.
///
/// Each line is written and flushed before the next, since the sign-in waits for the code to be
/// used and a person has to read them while it does. The address and the code are the ones the reply
/// gave, which the sign-in showed only after checking that they are printable and that the address
/// is `https`.
fn announce_to(announcement: &Announcement<'_>, shown: &mut impl Write) {
    let minutes = i64::try_from(announcement.expires_in.div_ceil(60)).unwrap_or(i64::MAX);
    for line in [
        t!(xai_open_this).to_string(),
        announcement.url.to_string(),
        t!(xai_enter_code, code = announcement.user_code),
        t!(xai_waiting, minutes = minutes),
    ] {
        // A stream that cannot be written to has nobody to tell, and the sign-in waits all the same.
        writeln!(shown, "{line}")
            .and_then(|()| shown.flush())
            .unwrap_or(());
    }
}

/// `bravebot auth logout xai`: remove the sign-in stored here.
///
/// Allowed in an incognito session, since it leaves less behind, and not an error where nothing is
/// stored. What it removes is the copy: no request is made to xAI, so nothing ends there.
pub(crate) fn logout() -> ExitCode {
    match Account::forget() {
        Ok(true) => {
            println!("{}", t!(xai_forgotten));
            ExitCode::SUCCESS
        }
        Ok(false) => {
            println!("{}", t!(xai_nothing_to_forget));
            ExitCode::SUCCESS
        }
        Err(refused) => fail(Ending::Failed, said_of_account(&refused)),
    }
}

/// The sentence for why the sign-in stopped, chosen from the step and the class of a status.
///
/// None quotes a reply, a request or a file ([XAI-8](../../../docs/specs/xai-sign-in.md#XAI-8)): the
/// variants hold none, and a path and what the operating system said are all a store failure adds.
fn said_of_sign_in(stopped: &SignInError) -> String {
    match stopped {
        SignInError::Code(class) => t!(xai_failed_code, reason = said_of_class(*class)),
        SignInError::AccessDenied => t!(xai_failed_access_denied).into(),
        SignInError::AuthorizationDenied => t!(xai_failed_authorization_denied).into(),
        SignInError::Expired => t!(xai_failed_expired).into(),
        SignInError::TimedOut => t!(xai_failed_timed_out).into(),
        SignInError::Exchange(class) => t!(xai_failed_exchange, reason = said_of_class(*class)),
        SignInError::Clock => t!(xai_failed_clock).into(),
        SignInError::Store(failure) => said_of_account(failure),
    }
}

/// The sentence for why the stored sign-in could not be read, written or removed, with the remedy
/// where signing in again or forgetting it is one.
fn said_of_account(failure: &AccountError) -> String {
    match failure {
        AccountError::Nowhere { detail } => t!(
            xai_account_nowhere,
            detail = crate::progress::printable(detail)
        ),
        AccountError::Unusable { detail } => t!(
            xai_account_unusable,
            detail = crate::progress::printable(detail)
        ),
        AccountError::Malformed(defect) => t!(
            xai_account_malformed,
            defect = match defect {
                Defect::Empty => t!(xai_defect_empty),
                Defect::NotJson => t!(xai_defect_not_json),
                Defect::OtherVersion => t!(xai_defect_other_version),
                Defect::MissingField => t!(xai_defect_missing_field),
                Defect::NoExpiry => t!(xai_defect_no_expiry),
            }
        ),
        AccountError::Unwritable { detail } => t!(
            xai_account_unwritable,
            detail = crate::progress::printable(detail)
        ),
    }
}

/// What the stored sign-in is, for `status` and for the list.
pub(crate) enum Standing {
    /// A sign-in is stored, and its access token expires at the instant stated.
    SignedIn { expires: String },
    /// Nothing is stored.
    NotSignedIn,
    /// This machine has nowhere to keep one.
    Nowhere,
    /// What is stored cannot be read, with the sentence that names the remedy.
    Unusable(String),
}

/// Read the stored sign-in, and print nothing and probe nothing. An access token whose instant has
/// passed is still signed in, since the next request replaces it.
pub(crate) fn standing() -> Standing {
    standing_of(Account::load())
}

fn standing_of(loaded: Result<Option<Account>, AccountError>) -> Standing {
    match loaded {
        Ok(Some(account)) => Standing::SignedIn {
            expires: account.expires(),
        },
        Ok(None) => Standing::NotSignedIn,
        Err(AccountError::Nowhere { .. }) => Standing::Nowhere,
        Err(other) => Standing::Unusable(said_of_account(&other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_skus::Secret;
    use bravebot_skus::xai::Class;

    fn can_start() -> Facts {
        Facts {
            incognito: false,
            named_by_a_gateway: false,
            stderr_is_a_terminal: true,
        }
    }

    /// XAI-10: each condition that stops the command stops it with the ending its sentence belongs
    /// to, a run where nothing is wrong is not stopped, and the mode is the refusal given where both
    /// apply. A refusal missing for one condition would ask xAI for a code and wait for it where
    /// nobody can see the code, or write a token where the mode exists to decline one.
    #[test]
    fn each_condition_that_cannot_be_answered_stops_the_command_with_its_own_ending() {
        assert!(refusal(&can_start()).is_none());

        let incognito = refusal(&Facts {
            incognito: true,
            ..can_start()
        })
        .expect("incognito is refused");
        assert_eq!(incognito.0, Ending::Failed);
        assert_eq!(incognito.1, t!(xai_not_while_incognito));

        let no_stderr = refusal(&Facts {
            stderr_is_a_terminal: false,
            ..can_start()
        })
        .expect("no terminal to show the address on is refused");
        assert_eq!(no_stderr.0, Ending::Argument);
        assert_eq!(no_stderr.1, t!(xai_needs_a_terminal));

        let gateway = refusal(&Facts {
            named_by_a_gateway: true,
            ..can_start()
        })
        .expect("a gateway of that name is refused");
        assert_eq!(gateway.0, Ending::Configuration);
        assert_eq!(gateway.1, t!(xai_name_is_a_gateway));

        let both = refusal(&Facts {
            incognito: true,
            named_by_a_gateway: true,
            stderr_is_a_terminal: false,
        })
        .expect("a refusal");
        assert_eq!(both.1, t!(xai_not_while_incognito));

        // The name is refused before the terminal is asked about, so a person told to rename the
        // block is not then told to find a terminal for a command that will refuse again.
        let name_and_terminal = refusal(&Facts {
            named_by_a_gateway: true,
            stderr_is_a_terminal: false,
            ..can_start()
        })
        .expect("a refusal");
        assert_eq!(name_and_terminal.1, t!(xai_name_is_a_gateway));
    }

    /// A stream that records what was written to it and how often it was flushed, in a buffer the
    /// test can also read while the code under test is still running.
    #[derive(Default)]
    struct Shown {
        written: Vec<u8>,
        /// The text written at each flush, so a line that was written and not flushed shows up as
        /// one that was never shown.
        flushed: Vec<String>,
    }

    impl Write for Shown {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.written.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushed
                .push(String::from_utf8_lossy(&self.written).into_owned());
            Ok(())
        }
    }

    /// XAI-11: the address and the code are written one line at a time, in the order a person acts
    /// on them, and each line is flushed as it is written. The address is the line after the one that
    /// says to open it, the code is in the third, and the wait says how long the code lasts in
    /// minutes, rounded up. An implementation that collected the lines and wrote them at the end
    /// would show them after the code had stopped working.
    #[test]
    fn the_address_and_the_code_are_shown_line_by_line_as_they_are_written() {
        let announcement = Announcement {
            url: "https://accounts.example/oauth2/device?user_code=ABCD-EFGH",
            user_code: "ABCD-EFGH",
            expires_in: 1800,
        };
        let mut shown = Shown::default();

        announce_to(&announcement, &mut shown);

        let text = String::from_utf8(shown.written.clone()).expect("text");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4, "{text}");
        assert_eq!(lines[0], t!(xai_open_this));
        assert_eq!(
            lines[1],
            "https://accounts.example/oauth2/device?user_code=ABCD-EFGH"
        );
        assert!(lines[2].ends_with("ABCD-EFGH"), "{text}");
        assert!(lines[3].contains("30 minutes"), "{text}");
        let sizes: Vec<usize> = shown
            .flushed
            .iter()
            .map(|so_far| so_far.lines().count())
            .collect();
        assert_eq!(
            sizes,
            [1, 2, 3, 4],
            "a line was held back: {shown:?}",
            shown = shown.flushed
        );
    }

    /// A code that lasts a part of a minute is a minute, and one minute is said in the singular. A
    /// wait stated as 0 minutes would tell a person the code had stopped working.
    #[test]
    fn the_time_a_code_lasts_is_whole_minutes_rounded_up() {
        for (seconds, wanted) in [(60, "1 minute."), (61, "2 minutes."), (300, "5 minutes.")] {
            let announcement = Announcement {
                url: "https://accounts.example/d",
                user_code: "A",
                expires_in: seconds,
            };
            let mut shown = Shown::default();

            announce_to(&announcement, &mut shown);

            let text = String::from_utf8(shown.written).expect("text");
            assert!(
                text.lines().last().expect("a line").ends_with(wanted),
                "{seconds}: {text}"
            );
        }
    }

    /// XAI-8: each way the sign-in stops has a sentence of its own, none says more than the step and
    /// the class it was made from, and signing in again is named where it is the remedy. A request that
    /// failed says the class of its status in the sentence of the step it was part of, and
    /// `access_denied`, `authorization_denied`, an expired code and the time passing are four
    /// sentences. That none holds text taken from a reply is held by the values being closed sets, and
    /// by `a_refused_sign_in_says_why_in_the_language_chosen_and_quotes_nothing_of_the_reply`, which
    /// reads what the binary printed.
    #[test]
    fn each_way_the_sign_in_stops_has_its_own_sentence_and_quotes_nothing() {
        let stops = [
            SignInError::Code(Class::Refused),
            SignInError::Code(Class::Unavailable),
            SignInError::AccessDenied,
            SignInError::AuthorizationDenied,
            SignInError::Expired,
            SignInError::TimedOut,
            SignInError::Exchange(Class::Refused),
            SignInError::Exchange(Class::Incomplete),
            SignInError::Clock,
        ];
        let mut said: Vec<String> = stops.iter().map(said_of_sign_in).collect();
        let all = said.clone();
        said.sort();
        said.dedup();
        assert_eq!(
            said.len(),
            stops.len(),
            "two stops share a sentence: {all:?}"
        );

        for (stopped, sentence) in stops.iter().zip(&all) {
            assert!(
                sentence.contains("nothing was stored"),
                "{stopped:?} did not say nothing was stored: {sentence}"
            );
        }
        for stopped in [
            SignInError::AccessDenied,
            SignInError::AuthorizationDenied,
            SignInError::Expired,
            SignInError::TimedOut,
            SignInError::Clock,
        ] {
            let sentence = said_of_sign_in(&stopped);
            assert!(
                sentence.contains("bravebot auth login xai"),
                "no remedy: {sentence}"
            );
        }
        assert!(
            said_of_sign_in(&SignInError::Exchange(Class::Incomplete))
                .contains(said_of_class(Class::Incomplete))
        );
    }

    /// XAI-8: a failure to store the sign-in or to read the stored one is a sentence that names the
    /// remedy where signing in again or forgetting it is one, and prints the path and what the
    /// system said as one line, so a file name with a control sequence in it cannot draw on the
    /// terminal.
    #[test]
    fn a_file_that_cannot_be_used_is_a_sentence_with_the_remedy_and_a_detail_that_is_one_line() {
        let detail = "/home/a/.bravebot/xai-account.json: \u{1b}[2Jdenied".to_string();

        let unusable = said_of_account(&AccountError::Unusable {
            detail: detail.clone(),
        });
        assert!(unusable.contains("bravebot auth login xai"), "{unusable}");
        assert!(unusable.contains("bravebot auth logout xai"), "{unusable}");
        assert!(!unusable.contains('\u{1b}'), "{unusable:?}");
        assert!(unusable.contains("xai-account.json"), "{unusable}");

        let unwritable = said_of_account(&AccountError::Unwritable {
            detail: detail.clone(),
        });
        assert!(!unwritable.contains('\u{1b}'), "{unwritable:?}");
        assert!(unwritable.contains("nothing was changed"), "{unwritable}");
        let nowhere = said_of_account(&AccountError::Nowhere { detail });
        assert!(!nowhere.contains('\u{1b}'), "{nowhere:?}");

        let mut defects: Vec<String> = [
            Defect::Empty,
            Defect::NotJson,
            Defect::OtherVersion,
            Defect::MissingField,
            Defect::NoExpiry,
        ]
        .into_iter()
        .map(|defect| said_of_account(&AccountError::Malformed(defect)))
        .collect();
        defects.sort();
        defects.dedup();
        assert_eq!(defects.len(), 5, "two defects share a sentence");
    }

    /// XAI-9: a stored sign-in is reported by its expiry alone, a token past its expiry is still
    /// signed in since the next request replaces it, nothing stored is not signed in, a machine with
    /// nowhere to keep one is said apart from one with nothing stored, and a file that cannot be used
    /// is unusable with the remedy and does not quote it.
    #[test]
    fn a_stored_sign_in_is_reported_by_its_expiry_alone_and_a_file_that_cannot_be_used_is_unusable()
    {
        let account = |expires_at| Account {
            access_token: Secret::new("an-access-token"),
            refresh_token: Secret::new("a-refresh-token"),
            expires_at,
        };

        let held = standing_of(Ok(Some(account(1_792_703_127))));
        assert!(
            matches!(&held, Standing::SignedIn { expires } if expires == "2026-10-22T21:05:27Z")
        );
        let lapsed = standing_of(Ok(Some(account(1))));
        assert!(
            matches!(&lapsed, Standing::SignedIn { expires } if expires == "1970-01-01T00:00:01Z")
        );
        assert!(matches!(standing_of(Ok(None)), Standing::NotSignedIn));
        assert!(matches!(
            standing_of(Err(AccountError::Nowhere {
                detail: "no home".to_string()
            })),
            Standing::Nowhere
        ));
        match standing_of(Err(AccountError::Malformed(Defect::NotJson))) {
            Standing::Unusable(said) => {
                assert!(said.contains("bravebot auth login xai"), "{said}");
                assert!(!said.contains("access-token"), "{said}");
            }
            _ => panic!("a file that is not JSON was not unusable"),
        }
        for printed in [
            format!("{:?}", account(1)),
            match standing_of(Ok(Some(account(1)))) {
                Standing::SignedIn { expires } => expires,
                _ => String::new(),
            },
        ] {
            assert!(!printed.contains("a-refresh-token"), "{printed}");
            assert!(!printed.contains("an-access-token"), "{printed}");
        }
    }

    /// XAI-9 and XAI-12: the line `auth status` and `doctor` say of a stored sign-in names the host the
    /// access token is presented to, and it is the host the account's requests go to, which
    /// [`bravebot_config::provider::Provider::xai_account`] holds as a constant. A line that named
    /// another host would tell a person where the token goes wrongly, and one that said nothing of it
    /// would leave the two hosts a token goes to unsaid. With the variable that names a stand-in set
    /// the account's host is the stand-in's, so the test fails and says so rather than comparing nothing.
    #[test]
    fn the_host_the_status_names_is_the_one_the_access_token_is_sent_to() {
        assert!(
            std::env::var("BRAVEBOT_TEST_XAI_STAND_IN").is_err(),
            "BRAVEBOT_TEST_XAI_STAND_IN is set, so the compiled-in host cannot be read: unset it"
        );
        let account = bravebot_config::provider::Provider::xai_account();
        let host = account.host();
        assert_eq!(host, "api.x.ai");

        for line in [
            t!(auth_status_xai_expires, expires = "2026-10-22T21:05:27Z"),
            t!(doctor_xai_signed_in, expires = "2026-10-22T21:05:27Z"),
        ] {
            assert!(line.contains(&format!("sent to {host} alone")), "{line}");
            assert!(line.contains("2026-10-22T21:05:27Z"), "{line}");
            assert!(
                line.contains("refresh token is sent to auth.x.ai alone"),
                "{line}"
            );
        }
    }
}
