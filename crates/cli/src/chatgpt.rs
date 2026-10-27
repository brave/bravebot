//! `bravebot auth login chatgpt`, `logout chatgpt` and what `status` and the list say about it
//! ([CHATGPT-1](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-1),
//! [CHATGPT-11](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-11),
//! [CHATGPT-12](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-12) and
//! [CHATGPT-13](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-13)).
//!
//! The flow itself is in `bravebot_skus::chatgpt`. This is what a person at a terminal sees of it:
//! the refusals that need no answer, the question about replacing a sign-in, the address to open,
//! and the browser.

use crate::exit::{Ending, fail};
use crate::plain::Prompting;
use bravebot_agent::chatgpt::AccountModels;
use bravebot_agent::confirm::Decision;
use bravebot_config::Config;
use bravebot_i18n::t;
use bravebot_net::Transport;
use bravebot_skus::chatgpt::{
    Account, AccountError, Announcement, Class, Defect, Hosts, Limits, Pages, SignInError, SignedIn,
};
use std::io::IsTerminal;
use std::process::ExitCode;

/// The id of the provider block that would be taken for this service's own name.
const NAME: &str = "chatgpt";

/// What decides whether `bravebot auth login chatgpt` may start, none of which needs an answer.
struct Facts {
    incognito: bool,
    /// A provider block with the id `chatgpt` states an endpoint.
    named_by_a_gateway: bool,
    stderr_is_a_terminal: bool,
    stdin_is_a_terminal: bool,
    /// A sign-in is stored and readable.
    stored: bool,
}

/// Why the command stops before it starts, with the ending that says so, or `None`.
///
/// The order is the order the checks are made in: the mode first, since the sign-in is refused there
/// whatever else is true, then the name, then the two terminals.
fn refusal(facts: &Facts) -> Option<(Ending, String)> {
    // An import of a subscription is refused in this mode for the same reason: the sign-in is a
    // write to `~/.bravebot` that the mode exists to decline. Doing it and discarding the result
    // would leave a session that was signed in once and never again.
    if facts.incognito {
        return Some((Ending::Failed, t!(chatgpt_not_while_incognito).into()));
    }
    // A block of this name that states an endpoint is a gateway and keeps the name, so one name
    // cannot select two services.
    if facts.named_by_a_gateway {
        return Some((Ending::Configuration, t!(chatgpt_name_is_a_gateway).into()));
    }
    // The address is shown on stderr, so a run that cannot show it to anyone cannot sign in.
    if !facts.stderr_is_a_terminal {
        return Some((Ending::Argument, t!(chatgpt_needs_a_terminal).into()));
    }
    // A stored sign-in is replaced only on a yes, and a run nobody is watching cannot give one.
    if facts.stored && !facts.stdin_is_a_terminal {
        return Some((Ending::Argument, t!(chatgpt_held_needs_a_terminal).into()));
    }
    None
}

/// `bravebot auth login chatgpt`.
///
/// Everything that needs no answer is checked before the listener binds and before a browser opens,
/// and each refusal writes nothing.
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
        .is_some_and(|config| config.providers.iter().any(|provider| provider.id == NAME));
    let stored = matches!(Account::load(), Ok(Some(_)));
    let facts = Facts {
        incognito,
        named_by_a_gateway,
        stderr_is_a_terminal: std::io::stderr().is_terminal(),
        stdin_is_a_terminal: std::io::stdin().is_terminal(),
        stored,
    };
    if let Some((ending, refused)) = refusal(&facts) {
        return fail(ending, refused);
    }
    after_asking(
        stored,
        &mut Prompting::new(std::io::stdin().lock(), std::io::stderr()),
        || sign_in(config.as_ref()),
    )
}

/// Run `then`, the sign-in, unless a sign-in is stored and the person does not say yes to replacing
/// it.
///
/// This is the gate on the stored sign-in: anything but a yes, the end of the input included,
/// leaves the file as it is, binds no listener and asks for no browser, since all three are what
/// `then` does.
fn after_asking<R: std::io::BufRead, W: std::io::Write>(
    stored: bool,
    asking: &mut Prompting<R, W>,
    then: impl FnOnce() -> ExitCode,
) -> ExitCode {
    if stored {
        asking.say(t!(chatgpt_held));
        if asking.ask(&[], t!(chatgpt_replace)) != Decision::Approve {
            return ExitCode::SUCCESS;
        }
    }
    then()
}

/// The sign-in itself: the listener, the address, the browser, the exchange and the file, and then
/// what the account reaches.
///
/// `config` is `None` only in an incognito session, which [`refusal`] stops before this runs.
fn sign_in(config: Option<&Config>) -> ExitCode {
    let signed_in = bravebot_skus::chatgpt::sign_in(
        &Hosts::compiled_in(),
        Transport::shared().agent_config_builder(),
        &Limits::default(),
        &Pages {
            done: t!(chatgpt_page_done),
            not_found: t!(chatgpt_page_not_found),
        },
        announce,
    );
    let signed_in = match signed_in {
        Ok(signed_in) => signed_in,
        Err(stopped) => return fail(Ending::Failed, said_of_sign_in(&stopped)),
    };
    // A standard output that cannot be written to has nobody to tell, and the sign-in is stored
    // whatever was said. Read once the sign-in is stored, since whether a start would ask depends on it.
    let asks = config.is_some_and(crate::model::a_start_would_ask);
    finish(
        &signed_in,
        &mut std::io::stdout(),
        || match config {
            Some(config) => bravebot_agent::chatgpt::account_models(
                config,
                &mut bravebot_session::audit::Trail::new(),
            ),
            None => AccountModels::Absent,
        },
        asks,
    )
    .unwrap_or(());
    ExitCode::SUCCESS
}

/// Say that the sign-in is stored, then ask what the account reaches and say that
/// ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
///
/// The sign-in is said before the listing is asked, so a service slow to answer leaves a person
/// knowing it was kept. What the listing says never changes the ending: the sign-in is stored
/// whatever the account reaches today.
///
/// `asks` is whether a start would ask which of the account's models to use, which decides whether
/// the sentence sends the person to run `bravebot`.
fn finish(
    signed_in: &SignedIn,
    shown: &mut impl std::io::Write,
    listing: impl FnOnce() -> AccountModels,
    asks: bool,
) -> std::io::Result<()> {
    writeln!(
        shown,
        "{}",
        t!(
            chatgpt_stored,
            expires = signed_in.expires.clone(),
            path = signed_in.stored_in.display().to_string()
        )
    )?;
    shown.flush()?;
    writeln!(shown, "{}", said_of_listing(&listing(), asks))
}

/// The sentence for what the listing answered: how to choose a model, that the account reaches none,
/// or that the listing could not be had and why.
///
/// A listing that was read and is empty is the service's answer, and is not told as a failure. One
/// refused in a code that names the account is said in that clause's sentence, which does not send
/// the person to sign in again.
///
/// Each sentence that names a way to use the account names a command that works as typed: `bravebot`
/// only where `asks` says a start would ask which model to use, or a command that names one together
/// with the prompt or the task that starts something. A person with a model in force that something
/// serves is not asked, and one with a Bedrock or gateway block is asked about that service's models and
/// not the account's, so neither is told the account's question will be asked. `/model` is
/// named only as what lists the models once a session is open, since the sign-in does not open one.
pub(crate) fn said_of_listing(listing: &AccountModels, asks: bool) -> String {
    match listing {
        AccountModels::Offered(models) if models.is_empty() => t!(chatgpt_reaches_no_model).into(),
        AccountModels::Unreadable => t!(chatgpt_listing_unreadable).into(),
        AccountModels::Compiled(_) => t!(chatgpt_listing_compiled).into(),
        AccountModels::Refused(refusal) => {
            t!(
                chatgpt_listing_refused,
                refusal = bravebot_agent::chatgpt::said_of_refusal(*refusal)
            )
        }
        AccountModels::Offered(_) | AccountModels::Absent => match asks {
            true => t!(chatgpt_use_by_asking).into(),
            false => t!(chatgpt_use_by_naming).into(),
        },
    }
}

/// Show the address and the port, then ask for the browser.
fn announce(announcement: &Announcement<'_>) {
    announce_to(announcement, &mut std::io::stderr(), |address| {
        // Not reported: the address has been shown, and the sign-in carries on waiting whether or
        // not a browser opened.
        bravebot_tui::opener::open_address(address);
    });
}

/// [`announce`], with the stream the lines go to and what asks for the browser stated.
///
/// The address is written before the browser is asked to open it, whether or not one can be opened:
/// an opener that exits successfully and opens nothing is what a machine reached over SSH may have.
/// The sign-in waits for the browser either way.
fn announce_to(
    announcement: &Announcement<'_>,
    shown: &mut impl std::io::Write,
    open: impl FnOnce(&str),
) {
    let _ = writeln!(shown, "{}", t!(chatgpt_open_this));
    let _ = writeln!(shown, "{}", announcement.url);
    let _ = writeln!(shown, "{}", t!(chatgpt_waiting, port = announcement.port));
    let _ = shown.flush();
    open(announcement.url);
}

/// `bravebot auth logout chatgpt`: remove the sign-in stored here.
///
/// Allowed in an incognito session, since it leaves less behind, and not an error where nothing is
/// stored. What it removes is the copy: the credential is OpenAI's, and ends there.
pub(crate) fn logout() -> ExitCode {
    match Account::forget() {
        Ok(true) => {
            println!("{}", t!(chatgpt_forgotten));
            ExitCode::SUCCESS
        }
        Ok(false) => {
            println!("{}", t!(chatgpt_nothing_to_forget));
            ExitCode::SUCCESS
        }
        Err(refused) => fail(Ending::Failed, said_of_account(&refused)),
    }
}

/// The sentence for why the sign-in stopped, chosen from the step and the class of a status.
///
/// None quotes a reply, a request, a callback or a settings file
/// ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)): the variants hold none, and a
/// path and what the operating system said are all a store failure adds.
fn said_of_sign_in(stopped: &SignInError) -> String {
    let class = |class: &Class| match class {
        Class::Refused => t!(chatgpt_class_refused),
        Class::RateLimited => t!(chatgpt_class_rate_limited),
        Class::Unavailable => t!(chatgpt_class_unavailable),
        Class::Incomplete => t!(chatgpt_class_incomplete),
        Class::Transport => t!(chatgpt_class_transport),
    };
    match stopped {
        SignInError::Listener => t!(chatgpt_failed_listener).into(),
        SignInError::TimedOut => t!(chatgpt_failed_timed_out).into(),
        SignInError::NoCode => t!(chatgpt_failed_no_code).into(),
        SignInError::NoClient => t!(chatgpt_failed_no_client).into(),
        SignInError::ClientChanged => t!(chatgpt_failed_client_changed).into(),
        SignInError::Exchange(failed) => t!(chatgpt_failed_exchange, reason = class(failed)),
        SignInError::NoUseScope => t!(chatgpt_failed_no_use_scope).into(),
        SignInError::KeySet(failed) => t!(chatgpt_failed_key_set, reason = class(failed)),
        SignInError::IdToken => t!(chatgpt_failed_id_token).into(),
        SignInError::Clock => t!(chatgpt_failed_clock).into(),
        SignInError::Store(failure) => said_of_account(failure),
    }
}

/// The sentence for why the stored sign-in could not be read, written or removed, with the remedy
/// where signing in again or forgetting it is one.
fn said_of_account(failure: &AccountError) -> String {
    match failure {
        AccountError::Nowhere { detail } => t!(
            chatgpt_account_nowhere,
            detail = crate::progress::printable(detail)
        ),
        AccountError::Unusable { detail } => t!(
            chatgpt_account_unusable,
            detail = crate::progress::printable(detail)
        ),
        AccountError::Malformed(defect) => t!(
            chatgpt_account_malformed,
            defect = match defect {
                Defect::Empty => t!(chatgpt_defect_empty),
                Defect::NotJson => t!(chatgpt_defect_not_json),
                Defect::OtherVersion => t!(chatgpt_defect_other_version),
                Defect::MissingField => t!(chatgpt_defect_missing_field),
                Defect::NoExpiry => t!(chatgpt_defect_no_expiry),
            }
        ),
        AccountError::Unwritable { detail } => t!(
            chatgpt_account_unwritable,
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

    fn can_start() -> Facts {
        Facts {
            incognito: false,
            named_by_a_gateway: false,
            stderr_is_a_terminal: true,
            stdin_is_a_terminal: true,
            stored: false,
        }
    }

    /// CHATGPT-12: each condition that stops the command stops it with the ending its sentence
    /// belongs to, and a run where nothing is wrong is not stopped. A refusal missing for one
    /// condition would bind a port, open a browser and write a token where the clause says it
    /// cannot.
    #[test]
    fn each_condition_that_cannot_be_answered_stops_the_command_with_its_own_ending() {
        assert!(refusal(&can_start()).is_none());

        let incognito = refusal(&Facts {
            incognito: true,
            ..can_start()
        })
        .expect("incognito is refused");
        assert_eq!(incognito.0, Ending::Failed);
        assert_eq!(incognito.1, t!(chatgpt_not_while_incognito));

        let gateway = refusal(&Facts {
            named_by_a_gateway: true,
            ..can_start()
        })
        .expect("a gateway of that name is refused");
        assert_eq!(gateway.0, Ending::Configuration);
        assert_eq!(gateway.1, t!(chatgpt_name_is_a_gateway));

        let no_stderr = refusal(&Facts {
            stderr_is_a_terminal: false,
            ..can_start()
        })
        .expect("no terminal to show the address on is refused");
        assert_eq!(no_stderr.0, Ending::Argument);
        assert_eq!(no_stderr.1, t!(chatgpt_needs_a_terminal));
    }

    /// CHATGPT-12: a stored sign-in is replaced only on a yes, so where stdin is not a terminal the
    /// command is refused, and where nothing is stored a stdin that is not a terminal is no matter.
    /// Refusing a first sign-in for want of stdin would turn away a run that asks nothing.
    #[test]
    fn a_stored_sign_in_needs_a_terminal_to_be_asked_about_and_a_first_one_does_not() {
        let held_and_unattended = refusal(&Facts {
            stored: true,
            stdin_is_a_terminal: false,
            ..can_start()
        })
        .expect("a replacement nobody can answer is refused");
        assert_eq!(held_and_unattended.0, Ending::Argument);
        assert_eq!(held_and_unattended.1, t!(chatgpt_held_needs_a_terminal));

        assert!(
            refusal(&Facts {
                stored: true,
                ..can_start()
            })
            .is_none(),
            "a replacement somebody can answer was refused"
        );
        assert!(
            refusal(&Facts {
                stdin_is_a_terminal: false,
                ..can_start()
            })
            .is_none(),
            "a first sign-in was refused for want of stdin"
        );
    }

    /// CHATGPT-12 states the mode refuses before anything else: an incognito session with a
    /// gateway of the name and no terminal is refused as incognito, which is the sentence that says
    /// why no other answer would have changed it.
    #[test]
    fn the_mode_is_the_refusal_given_where_several_apply() {
        let refused = refusal(&Facts {
            incognito: true,
            named_by_a_gateway: true,
            stderr_is_a_terminal: false,
            stdin_is_a_terminal: false,
            stored: true,
        })
        .expect("a refusal");
        assert_eq!(refused.1, t!(chatgpt_not_while_incognito));
    }

    /// A stream that records what was written to it, in a buffer the test can also read while the
    /// code under test is still running.
    #[derive(Clone, Default)]
    struct Shown(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

    impl std::io::Write for Shown {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// CHATGPT-13: the address and the port are written before the browser is asked to open it, and
    /// the browser is asked for that address alone. An opener that failed or opened nothing would
    /// leave a person with no address if it came first, which is what a machine reached over SSH
    /// can do.
    #[test]
    fn the_address_and_the_port_are_shown_before_the_browser_is_asked_for() {
        let announcement = Announcement {
            url: "https://auth.example/authorize?a=1&b=2",
            port: 40123,
        };
        let mut shown = Shown::default();
        let by_then = Shown::default();
        let mut asked_for = None;

        announce_to(&announcement, &mut shown.clone(), |url| {
            // What had been shown at the moment the browser was asked for.
            by_then.0.borrow_mut().extend(shown.0.borrow().iter());
            asked_for = Some(url.to_string());
        });

        let before = String::from_utf8(by_then.0.borrow().clone()).expect("text");
        let lines: Vec<&str> = before.lines().collect();
        assert_eq!(lines.len(), 3, "{before}");
        assert_eq!(lines[0], t!(chatgpt_open_this));
        assert_eq!(lines[1], "https://auth.example/authorize?a=1&b=2");
        assert!(lines[2].contains("40123"), "{before}");
        assert_eq!(
            asked_for.as_deref(),
            Some("https://auth.example/authorize?a=1&b=2")
        );
        let _ = &mut shown;
    }

    /// A sign-in as the flow reports it, with a path that names nothing real.
    fn signed_in() -> SignedIn {
        SignedIn {
            expires: "2026-10-22T21:05:27Z".to_string(),
            stored_in: std::path::PathBuf::from("/placeholder/chatgpt-account.json"),
        }
    }

    /// CHATGPT-8, CHATGPT-9: once the sign-in is stored the command says what the account reaches,
    /// and each answer of the listing has its own sentence. An account that reaches no model is told
    /// as that and not as a failure, and a listing that could not be had is not told as the account
    /// reaching none, since the person would then stop looking for a model the account has. Where
    /// the compiled list stands in for it the sentence says so and that a row may be refused, and
    /// where nothing is offered it says that. A refusal in a code that names the account is said in
    /// its own sentence and does not send the person to sign in again. Only a listing with a model
    /// in it sends the person to run `bravebot` to choose one.
    #[test]
    fn each_answer_of_the_listing_has_its_own_sentence_and_only_models_send_the_person_to_choose() {
        use bravebot_agent::chatgpt::{Model, Refusal};

        let reaches_none = said_of_listing(&AccountModels::Offered(Vec::new()), true);
        let unreadable = said_of_listing(&AccountModels::Unreadable, true);
        let compiled = said_of_listing(&AccountModels::Compiled(vec![Model::automatic()]), true);
        let offered = said_of_listing(&AccountModels::Offered(vec![Model::automatic()]), true);
        assert_eq!(reaches_none, t!(chatgpt_reaches_no_model));
        assert_eq!(unreadable, t!(chatgpt_listing_unreadable));
        assert_eq!(compiled, t!(chatgpt_listing_compiled));
        assert_eq!(offered, t!(chatgpt_use_by_asking));
        assert_eq!(said_of_listing(&AccountModels::Absent, true), offered);

        let mut distinct = vec![
            reaches_none.clone(),
            unreadable.clone(),
            compiled.clone(),
            offered.clone(),
        ];
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), 4, "two answers share a sentence");
        assert!(
            unreadable.contains("--model chatgpt/<model>"),
            "no way out for an unreadable listing: {unreadable}"
        );
        assert!(
            offered.starts_with("Run bravebot"),
            "the sentence does not send the person to the command that asks: {offered}"
        );
        assert!(
            compiled.contains("built-in list") && compiled.contains("cannot reach"),
            "the compiled list was not said to be one that may be refused: {compiled}"
        );
        assert!(
            compiled.contains("--model chatgpt/<model>"),
            "no way out for a model the account cannot reach: {compiled}"
        );
        assert!(
            !compiled.contains("reaches no model") && !unreadable.contains("built-in"),
            "an answer was told as another: {compiled} / {unreadable}"
        );
        assert!(
            !reaches_none.contains("could not"),
            "an empty listing was told as a failure: {reaches_none}"
        );
        assert!(
            !unreadable.contains("reaches no model"),
            "a failed listing was told as the account having nothing: {unreadable}"
        );

        for refusal in [Refusal::UsageLimit, Refusal::NotPermitted] {
            let said = said_of_listing(&AccountModels::Refused(refusal), true);
            assert_eq!(
                said,
                t!(
                    chatgpt_listing_refused,
                    refusal = bravebot_agent::chatgpt::said_of_refusal(refusal)
                ),
                "{refusal:?}"
            );
            assert!(
                !said.contains("sign in again") && !said.contains("auth login"),
                "a refusal sent the person to sign in again: {said}"
            );
        }
    }

    /// CHATGPT-8: each sentence the sign-in ends with that names a way to use the account names the
    /// two commands that work as typed, one that opens a session and one that runs a task, each with
    /// a model, and names `/model` only after saying a session is open. The regressions it rejects
    /// are `bravebot --model chatgpt/<model>`, which is refused for want of a task, and a
    /// sentence that sends a person to `/model` before any session can be open, which is what the
    /// sign-in ended with before a start asked which model to use.
    #[test]
    fn every_command_the_sign_in_ends_by_naming_is_one_that_works_as_typed() {
        use bravebot_agent::chatgpt::Model;
        const WORKING: [&str; 2] = [
            r#"bravebot -i "<prompt>" --model chatgpt/<model>"#,
            r#"bravebot "<task>" --model chatgpt/<model>"#,
        ];

        for asks in [true, false] {
            for listing in [
                AccountModels::Offered(vec![Model::automatic()]),
                AccountModels::Unreadable,
                AccountModels::Compiled(vec![Model::automatic()]),
            ] {
                let said = said_of_listing(&listing, asks);
                let named: Vec<&str> = said.split('`').skip(1).step_by(2).collect();
                assert_eq!(named, WORKING, "{said}");
                assert!(!said.contains("bravebot --model"), "{said}");
                if let Some(at) = said.find("/model") {
                    assert!(
                        said[..at].contains("session is open"),
                        "/model is named before a session is open: {said}"
                    );
                }
            }
        }
    }

    /// CHATGPT-8, BACKEND-39: the sign-in promises the question only where a start would ask it. The
    /// sentence that sends a person to run `bravebot` for it is said where one would, and where it
    /// would not ask about the account, because a service of the person's own is configured, whose models
    /// it asks about instead, or a model is in force that something serves, the sentence names the
    /// commands and says nothing of being asked. No other
    /// sentence the command ends with promises it: where the listing could not be had nothing can be
    /// asked, whoever the person is. The regression it rejects is the promise made to everybody.
    #[test]
    fn the_sign_in_promises_the_question_only_where_a_start_would_ask_it() {
        use bravebot_agent::chatgpt::Model;
        let models = || vec![Model::automatic()];

        let asked = said_of_listing(&AccountModels::Offered(models()), true);
        let not_asked = said_of_listing(&AccountModels::Offered(models()), false);
        assert_eq!(asked, t!(chatgpt_use_by_asking));
        assert_eq!(not_asked, t!(chatgpt_use_by_naming));
        assert!(asked.contains("Run bravebot"), "{asked}");
        for said in [
            &not_asked,
            &said_of_listing(&AccountModels::Unreadable, true),
            &said_of_listing(&AccountModels::Compiled(models()), true),
            &said_of_listing(&AccountModels::Unreadable, false),
            &said_of_listing(&AccountModels::Compiled(models()), false),
        ] {
            assert!(
                !said.contains("Run bravebot") && !said.contains("asked which"),
                "the sentence promises a question: {said}"
            );
        }
        for listing in [AccountModels::Unreadable, AccountModels::Compiled(models())] {
            assert_eq!(
                said_of_listing(&listing, true),
                said_of_listing(&listing, false),
                "a sentence for a listing that could not be had changed with who is asked"
            );
        }
    }

    /// CHATGPT-8: the sign-in is said before the listing is asked, so a service slow to answer
    /// leaves a person knowing the sign-in was kept, and the listing is asked once. What the listing
    /// says follows, and the line that says the sign-in is stored names its expiry and where it is
    /// kept. The regression it rejects is asking first, which holds the confirmation back for as
    /// long as the request takes.
    #[test]
    fn the_sign_in_is_said_before_the_listing_is_asked_and_the_listing_is_asked_once() {
        let shown = Shown::default();
        let by_then = std::cell::RefCell::new(None);
        let mut asked = 0;

        finish(
            &signed_in(),
            &mut shown.clone(),
            || {
                asked += 1;
                *by_then.borrow_mut() =
                    Some(String::from_utf8(shown.0.borrow().clone()).expect("text"));
                AccountModels::Offered(Vec::new())
            },
            false,
        )
        .expect("written");

        assert_eq!(asked, 1);
        let stored = t!(
            chatgpt_stored,
            expires = "2026-10-22T21:05:27Z",
            path = "/placeholder/chatgpt-account.json"
        );
        assert_eq!(
            by_then.into_inner().expect("the listing was asked"),
            format!("{stored}\n"),
            "the sign-in was not said before the listing was asked"
        );
        let all = String::from_utf8(shown.0.borrow().clone()).expect("text");
        assert_eq!(all, format!("{stored}\n{}\n", t!(chatgpt_reaches_no_model)));
    }

    /// CHATGPT-8: each step that stops the sign-in has its own sentence, the sentences of the
    /// failed requests differ by the class of the status, and each names the command that is the
    /// remedy where signing in again is one. The sentence for a refused scope names the gateway
    /// that is the other road, and the one for the time bound names forwarding the port.
    #[test]
    fn each_stop_of_the_sign_in_has_its_own_sentence_and_names_the_remedy_where_there_is_one() {
        let sentences: Vec<String> = [
            SignInError::Listener,
            SignInError::TimedOut,
            SignInError::NoCode,
            SignInError::NoClient,
            SignInError::ClientChanged,
            SignInError::NoUseScope,
            SignInError::Exchange(Class::Refused),
            SignInError::Exchange(Class::RateLimited),
            SignInError::Exchange(Class::Unavailable),
            SignInError::Exchange(Class::Incomplete),
            SignInError::Exchange(Class::Transport),
            SignInError::IdToken,
            SignInError::KeySet(Class::Refused),
            SignInError::KeySet(Class::Transport),
            SignInError::Clock,
        ]
        .iter()
        .map(said_of_sign_in)
        .collect();

        let mut distinct = sentences.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), sentences.len(), "{sentences:#?}");
        assert!(sentences[1].contains("forwarded"), "{}", sentences[1]);
        for needing_the_command in [2, 3, 5] {
            assert!(
                sentences[needing_the_command].contains("bravebot auth login chatgpt"),
                "{}",
                sentences[needing_the_command]
            );
        }
        assert!(
            sentences[4].contains("bravebot auth logout chatgpt"),
            "{}",
            sentences[4]
        );
        assert!(sentences[5].contains("openai gateway"), "{}", sentences[5]);
        assert!(sentences[6].contains("refused"), "{}", sentences[6]);
        assert!(sentences[7].contains("limiting"), "{}", sentences[7]);
        assert!(sentences[8].contains("unavailable"), "{}", sentences[8]);
        assert!(
            sentences[11].contains("bravebot auth login chatgpt"),
            "{}",
            sentences[11]
        );
        assert!(
            sentences[13].contains("could not be reached"),
            "{}",
            sentences[13]
        );
        assert!(
            sentences[14].contains("clock") && sentences[14].contains("1970"),
            "{}",
            sentences[14]
        );
        assert!(
            sentences[14].contains("bravebot auth login chatgpt"),
            "{}",
            sentences[14]
        );
    }

    /// CHATGPT-8: a failure to store is worded with the path and what the system said, made safe to
    /// print, and a file that cannot be used names both remedies. A detail with an escape in it
    /// would otherwise move the cursor of the terminal it is printed on.
    #[test]
    fn a_failure_of_the_file_is_worded_with_its_detail_made_printable_and_the_remedy() {
        let detail = "/home/a/.bravebot/chatgpt-account.json: \u{1b}[2Jdenied".to_string();

        for (failure, remedy) in [
            (
                AccountError::Nowhere {
                    detail: detail.clone(),
                },
                false,
            ),
            (
                AccountError::Unusable {
                    detail: detail.clone(),
                },
                true,
            ),
            (
                AccountError::Unwritable {
                    detail: detail.clone(),
                },
                false,
            ),
        ] {
            let said = said_of_account(&failure);
            assert!(said.contains("chatgpt-account.json"), "{said}");
            assert!(!said.contains('\u{1b}'), "{said:?}");
            assert_eq!(
                said.contains("bravebot auth logout chatgpt"),
                remedy,
                "{said}"
            );
        }
        let defects: Vec<String> = [
            Defect::Empty,
            Defect::NotJson,
            Defect::OtherVersion,
            Defect::MissingField,
            Defect::NoExpiry,
        ]
        .into_iter()
        .map(|defect| said_of_account(&AccountError::Malformed(defect)))
        .collect();
        let mut distinct = defects.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), defects.len(), "{defects:#?}");
        for said in &defects {
            assert!(said.contains("bravebot auth login chatgpt"), "{said}");
            assert!(said.contains("bravebot auth logout chatgpt"), "{said}");
        }
    }

    /// CHATGPT-12: a stored sign-in is replaced only on a yes. Anything else, a no, an empty line or
    /// the end of the input, stops before the sign-in runs, which is what binds the listener, asks
    /// for the browser and writes the file; a first sign-in asks nothing.
    #[test]
    fn a_stored_sign_in_is_replaced_only_on_a_yes_and_a_first_one_asks_nothing() {
        let ran_after = |stored: bool, answer: &str| {
            let ran = std::cell::Cell::new(false);
            let mut asking = Prompting::new(std::io::Cursor::new(answer.to_string()), Vec::new());
            after_asking(stored, &mut asking, || {
                ran.set(true);
                ExitCode::SUCCESS
            });
            ran.get()
        };

        for refused in ["n\n", "\n", "", "no\n", "maybe\n"] {
            assert!(
                !ran_after(true, refused),
                "{refused:?} replaced the sign-in"
            );
        }
        assert!(ran_after(true, "y\n"), "a yes did not replace it");
        assert!(ran_after(false, ""), "a first sign-in asked a question");
    }

    fn signed_in_account() -> Account {
        Account {
            access_token: Secret::new("an-access-token"),
            refresh_token: Secret::new("a-refresh-token"),
            expires_at: 1_792_703_127,
            client_id: "issued-client".to_string(),
            host_id: "urn:uuid:a-host".to_string(),
        }
    }

    /// CHATGPT-11: what the file holds is stated as the instant the access token expires and
    /// nothing that could be used to sign a request or to say whose account it is.
    #[test]
    fn a_stored_sign_in_is_reported_by_its_expiry_alone() {
        let Standing::SignedIn { expires } = standing_of(Ok(Some(signed_in_account()))) else {
            panic!("a stored sign-in was not signed in");
        };

        assert_eq!(expires, "2026-10-22T21:05:27Z");
    }

    /// CHATGPT-11: an expiry that has passed is still signed in, since the next request replaces
    /// the token, and nothing stored is not signed in. A status that read a past expiry as signed
    /// out would end every sign-in at the first hour.
    #[test]
    fn a_token_past_its_expiry_is_still_signed_in_and_nothing_stored_is_not() {
        let mut lapsed = signed_in_account();
        lapsed.expires_at = 1;

        assert!(matches!(
            standing_of(Ok(Some(lapsed))),
            Standing::SignedIn { .. }
        ));
        assert!(matches!(standing_of(Ok(None)), Standing::NotSignedIn));
    }

    /// CHATGPT-11: a file that cannot be used is unusable with the remedy, and a machine with no
    /// directory to keep one in is told so rather than that the file is unusable.
    #[test]
    fn a_file_that_cannot_be_used_is_unusable_and_no_place_to_keep_one_is_said_apart() {
        let Standing::Unusable(reason) =
            standing_of(Err(AccountError::Malformed(Defect::MissingField)))
        else {
            panic!("a malformed file was not unusable");
        };
        assert!(reason.contains("bravebot auth login chatgpt"), "{reason}");
        assert!(reason.contains("bravebot auth logout chatgpt"), "{reason}");

        assert!(matches!(
            standing_of(Err(AccountError::Nowhere {
                detail: "HOME names nothing".to_string()
            })),
            Standing::Nowhere
        ));
    }
}
