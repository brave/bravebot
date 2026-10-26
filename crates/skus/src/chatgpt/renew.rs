//! Replacing an access token before it expires
//! ([CHATGPT-7](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-7)).
//!
//! The renewer is the one [`crate::oauth`] states for any sign-in, given the ChatGPT account and the
//! refresh of [`super::exchange`]. The stored sign-in is read, and where its access token is within
//! a minute of its expiry the refresh token is presented to the token endpoint under the claim on the
//! file, the reply is written, and only then is the new access token handed out. Every process that
//! signs in or renews takes the same claim and reads the file again once it holds it, so two of them
//! never present one refresh token, which a server that replaces it on use would kill for the second.
//!
//! The file is written there whatever the session is doing: this crate knows no mode that declines
//! a write, and a refresh that was not written would end the sign-in at the next process
//! ([CHATGPT-14](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-14)). The tests below run that
//! renewer against the ChatGPT account and a stand-in token endpoint.

use super::account::ChatGpt;

/// Hands out access tokens for the stored ChatGPT sign-in, replacing one that is about to expire.
pub type Renewer = crate::oauth::Renewer<ChatGpt>;

#[cfg(test)]
mod tests {
    use super::super::account::{self, Account};
    use super::super::{Class, Hosts};
    use super::*;
    use crate::Secret;
    use crate::oauth::encoding::{now, now_at};
    use crate::oauth::stand_in::{Received, Reply, named, serve};
    use crate::oauth::{AccountError, TokenError};
    use std::path::{Path, PathBuf};

    const CLIENT: &str = "issued-client";

    fn scratch(name: &str) -> PathBuf {
        let directory = crate::testutil::scratch_dir(&format!("bravebot-chatgpt-renew-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        directory.join("chatgpt-account.json")
    }

    /// A stored sign-in whose access token expires `lasts` seconds from now.
    fn stored(access: &str, refresh: &str, lasts: i64) -> Account {
        Account {
            access_token: Secret::new(access),
            refresh_token: Secret::new(refresh),
            expires_at: now().saturating_add_signed(lasts),
            client_id: CLIENT.to_string(),
            host_id: "urn:uuid:stored-host".to_string(),
        }
    }

    fn renewer(base: &str, path: &Path) -> Renewer {
        Renewer::at(
            Hosts::at(base),
            ureq::Agent::config_builder(),
            path.to_path_buf(),
        )
    }

    fn reply(access: &str, refresh: Option<&str>, lifetime: u64) -> Reply {
        let refresh = refresh.map_or_else(String::new, |token| {
            format!(",\"refresh_token\":\"{token}\"")
        });
        Reply::json(
            200,
            &format!("{{\"access_token\":\"{access}\"{refresh},\"expires_in\":{lifetime}}}"),
        )
    }

    fn on_disk(path: &Path) -> Account {
        account::load_at(path)
            .expect("a read")
            .expect("a stored sign-in")
    }

    /// Run `act` against a renewer over the stored `before` and a token server giving `replies`,
    /// and return what it returned and what the token server was asked.
    fn run<T>(
        name: &str,
        before: Option<&Account>,
        replies: Vec<Reply>,
        act: impl FnOnce(&mut Renewer) -> T,
    ) -> (T, Vec<Received>, PathBuf) {
        let path = scratch(name);
        if let Some(before) = before {
            account::save_at(&path, before).expect("a stored account");
        }
        let server = serve(replies);
        let outcome = act(&mut renewer(&server.base, &path));
        (outcome, server.finish(), path)
    }

    /// A token more than a minute from its expiry is sent as it is, and nothing is asked of the
    /// token endpoint. An implementation that renewed every time would spend the refresh token on
    /// each request.
    #[test]
    fn a_token_with_more_than_a_minute_left_is_used_and_nothing_is_asked() {
        let (outcome, asked, _) = run(
            "fresh",
            Some(&stored("still-good", "r0", 120)),
            vec![],
            Renewer::access_token,
        );

        assert_eq!(outcome.expect("a token").token.expose(), "still-good");
        assert!(asked.is_empty(), "{asked:?}");
    }

    /// A token within the minute is replaced, and one that has already expired is. An
    /// implementation that compared the expiry with the clock alone would send a token that
    /// expires on the way to the host.
    #[test]
    fn a_token_within_a_minute_of_its_expiry_or_past_it_is_replaced() {
        for lasts in [40_i64, 0, -3600] {
            let (outcome, asked, _) = run(
                &format!("near-{}", lasts.unsigned_abs()),
                Some(&stored("old", "r0", lasts)),
                vec![reply("new", None, 3600)],
                Renewer::access_token,
            );

            assert_eq!(outcome.expect("a token").token.expose(), "new", "{lasts}");
            assert_eq!(asked.len(), 1, "{lasts}");
        }
    }

    /// The refresh presents the stored refresh token under the stored client, and what it gives is
    /// in the file before the token is handed out, with the client and the host the sign-in had.
    /// A renewal that did not write would present a refresh token the server had replaced.
    #[test]
    fn a_renewal_presents_the_stored_refresh_token_and_writes_what_it_is_given() {
        let (outcome, asked, path) = run(
            "writes",
            Some(&stored("old", "r0", -5)),
            vec![reply("new", Some("r1"), 1800)],
            Renewer::access_token,
        );

        let access = outcome.expect("a token");
        assert_eq!(access.token.expose(), "new");
        assert!(access.unwritten.is_none());
        let form = asked[0].form();
        assert_eq!(named(&form, "grant_type"), "refresh_token");
        assert_eq!(named(&form, "client_id"), CLIENT);
        assert_eq!(named(&form, "refresh_token"), "r0");
        let file = on_disk(&path);
        assert_eq!(file.access_token.expose(), "new");
        assert_eq!(file.refresh_token.expose(), "r1");
        assert_eq!(file.client_id, CLIENT);
        assert_eq!(file.host_id, "urn:uuid:stored-host");
        let wanted = now() + 1800;
        assert!(
            (wanted - 10..=wanted + 10).contains(&file.expires_at),
            "{} against {wanted}",
            file.expires_at
        );
    }

    /// The known cost of CHATGPT-7: under a clock set before 1970 every expiry looks far off, a
    /// token the service refuses is replaced, and the renewal stores an `expires_at` of the reply's
    /// lifetime past 1970. The next start under a correct clock finds that expired and renews it
    /// again. A renewal that stored another expiry through any path would leave the file as the
    /// start under a correct clock finds it.
    #[test]
    fn a_renewal_under_a_clock_before_1970_stores_an_expiry_the_next_start_renews_again() {
        use std::time::{Duration, UNIX_EPOCH};

        fn before_1970() -> u64 {
            now_at(UNIX_EPOCH - Duration::from_secs(86_400))
        }

        let path = scratch("clock-before-1970");
        account::save_at(&path, &stored("old", "r0", 3600)).expect("a stored account");
        let server = serve(vec![
            reply("new", Some("r1"), 1800),
            reply("newer", Some("r2"), 1800),
        ]);

        let mut under_that_clock = renewer(&server.base, &path).clocked(before_1970);
        let unexpired = under_that_clock.access_token().expect("a token");
        assert_eq!(
            unexpired.token.expose(),
            "old",
            "a token an hour from its expiry is sent"
        );
        let renewed = under_that_clock
            .replace_refused(&Secret::new("old"))
            .expect("a token");
        assert_eq!(renewed.token.expose(), "new");
        assert_eq!(
            on_disk(&path).expires_at,
            1800,
            "the expiry is the reading of the clock, which is 0, plus the reply's lifetime"
        );

        let next = renewer(&server.base, &path)
            .access_token()
            .expect("a token");
        assert_eq!(
            next.token.expose(),
            "newer",
            "the next start did not renew it"
        );
        let asked = server.finish();
        assert_eq!(asked.len(), 2, "{asked:?}");
        assert_eq!(named(&asked[1].form(), "refresh_token"), "r1");
    }

    /// A reply that grants no refresh token keeps the one presented: replacing it with nothing
    /// would sign the person out an hour later.
    #[test]
    fn a_reply_with_no_refresh_token_leaves_the_stored_one() {
        let (outcome, _, path) = run(
            "kept",
            Some(&stored("old", "r0", -5)),
            vec![reply("new", None, 3600)],
            Renewer::access_token,
        );

        outcome.expect("a token");
        assert_eq!(on_disk(&path).refresh_token.expose(), "r0");
    }

    /// Nothing stored is not signed in, and nothing is asked.
    #[test]
    fn no_stored_sign_in_is_not_signed_in() {
        let (outcome, asked, _) = run("none", None, vec![], Renewer::access_token);

        assert!(matches!(outcome, Err(TokenError::NotSignedIn)));
        assert!(asked.is_empty());
    }

    /// A refresh the server refuses as the refresh token no longer being valid ends the sign-in:
    /// the file is removed, so every later turn says so and none presents a token that cannot work.
    #[test]
    fn a_refresh_refused_as_ended_removes_the_sign_in() {
        let (outcome, asked, path) = run(
            "ended",
            Some(&stored("old", "r0", -5)),
            vec![Reply::json(400, "{\"error\":\"invalid_grant\"}")],
            Renewer::access_token,
        );

        assert!(matches!(outcome, Err(TokenError::Ended)), "{outcome:?}");
        assert_eq!(asked.len(), 1);
        assert!(!path.exists());
    }

    /// A renewer that found the sign-in end says so to every later ask, whichever kind, and asks the
    /// token endpoint nothing more. The file is gone by then, and an implementation that read a
    /// missing file as a sign-in nobody made would say so to the turn that asks after a picker's
    /// listing found the end. The end is this renewer's: another over the same missing file has not
    /// seen it and says nothing is stored.
    #[test]
    fn a_renewer_that_found_the_sign_in_end_says_so_to_every_later_ask() {
        let path = scratch("ended-kept");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![Reply::json(400, "{\"error\":\"invalid_grant\"}")]);
        let mut found = renewer(&server.base, &path);

        let first = found.access_token();
        assert!(matches!(first, Err(TokenError::Ended)), "{first:?}");
        assert!(!path.exists());
        let later = [
            found.access_token().map(|_| ()),
            found.replace_refused(&Secret::new("old")).map(|_| ()),
            found.access_token().map(|_| ()),
        ];
        for outcome in later {
            assert!(matches!(outcome, Err(TokenError::Ended)), "{outcome:?}");
        }
        let other = renewer(&server.base, &path).access_token();
        assert!(matches!(other, Err(TokenError::NotSignedIn)), "{other:?}");
        assert_eq!(
            server.finish().len(),
            1,
            "the token endpoint was asked again"
        );
    }

    /// A sign-in stored after the end is used, and the end is forgotten: it is not the sign-in that
    /// ended, so it is not told the end, and a file removed afterwards is a sign-in nobody holds.
    /// An implementation that answered the end without reading the file would refuse the new
    /// sign-in, and one that never forgot it would say the new sign-in ended when the person logged
    /// out.
    #[test]
    fn a_sign_in_stored_after_the_end_is_used_and_the_end_is_forgotten() {
        let path = scratch("ended-forgotten");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![Reply::json(400, "{\"error\":\"invalid_grant\"}")]);
        let mut found = renewer(&server.base, &path);
        assert!(matches!(found.access_token(), Err(TokenError::Ended)));

        account::save_at(&path, &stored("signed-in-again", "r9", 3000)).expect("a new sign-in");
        let again = found.access_token().expect("the new sign-in's token");
        assert_eq!(again.token.expose(), "signed-in-again");

        std::fs::remove_file(&path).expect("a logout");
        let after = found.access_token();
        assert!(matches!(after, Err(TokenError::NotSignedIn)), "{after:?}");
        assert_eq!(server.finish().len(), 1);
    }

    /// Any other failure leaves the file as it was. A busy server, a refusal with another code and
    /// a connection that failed must not delete a sign-in that still works.
    #[test]
    fn a_refresh_that_fails_otherwise_leaves_the_file_and_says_why() {
        for (reply, class) in [
            (Reply::json(503, "{}"), Class::Unavailable),
            (Reply::json(429, "{}"), Class::RateLimited),
            (
                Reply::json(400, "{\"error\":\"invalid_request\"}"),
                Class::Refused,
            ),
            (Reply::json(200, "{\"nothing\":1}"), Class::Incomplete),
        ] {
            let (outcome, _, path) = run(
                &format!("failed-{class}"),
                Some(&stored("old", "r0", -5)),
                vec![reply],
                Renewer::access_token,
            );

            assert!(
                matches!(outcome, Err(TokenError::Refresh(got)) if got == class),
                "{class}: {outcome:?}"
            );
            let file = on_disk(&path);
            assert_eq!(file.access_token.expose(), "old");
            assert_eq!(file.refresh_token.expose(), "r0");
        }
    }

    /// A redirect in answer to a refresh is a failure and is not followed, since the request carries
    /// the refresh token and a followed redirect would send it to a host nobody compiled in. It ends
    /// nothing: the file keeps the access token and the refresh token that were presented. A renewer
    /// whose client followed redirects would pass the exchange's test, which builds its own client.
    #[test]
    fn a_redirect_in_answer_to_a_refresh_is_not_followed_and_the_sign_in_is_left_alone() {
        let elsewhere = serve(vec![reply("never-sent", Some("never-sent"), 3600)]);

        let (outcome, asked, path) = run(
            "redirect",
            Some(&stored("old", "r0", -5)),
            vec![Reply::redirect(&format!("{}/token", elsewhere.base))],
            Renewer::access_token,
        );

        let reached = elsewhere.finish();
        assert!(reached.is_empty(), "the redirect was followed: {reached:?}");
        assert_eq!(asked.len(), 1);
        assert!(
            matches!(outcome, Err(TokenError::Refresh(Class::Refused))),
            "{outcome:?}"
        );
        let file = on_disk(&path);
        assert_eq!(file.access_token.expose(), "old");
        assert_eq!(file.refresh_token.expose(), "r0");
    }

    /// Where the refresh fails and the access token has not yet passed its expiry, the request is
    /// sent with it: it was within the margin, not dead. One past its expiry is never sent.
    #[test]
    fn a_failed_refresh_sends_a_token_that_has_not_expired_and_never_one_that_has() {
        let (outcome, _, _) = run(
            "margin",
            Some(&stored("last-minute", "r0", 30)),
            vec![Reply::json(503, "{}")],
            Renewer::access_token,
        );
        assert_eq!(outcome.expect("a token").token.expose(), "last-minute");

        let (outcome, _, _) = run(
            "past",
            Some(&stored("dead", "r0", -1)),
            vec![Reply::json(503, "{}")],
            Renewer::access_token,
        );
        assert!(matches!(
            outcome,
            Err(TokenError::Refresh(Class::Unavailable))
        ));
    }

    /// A token the host refused is replaced though its expiry is far off, and when the refresh then
    /// fails, the refused token is not handed back. Replacing only by the clock would send the same
    /// refused token again.
    #[test]
    fn a_refused_token_is_replaced_and_never_returned_when_the_refresh_fails() {
        let before = stored("refused-one", "r0", 3000);
        let refused = Secret::new("refused-one");

        let (outcome, asked, path) = run(
            "refused",
            Some(&before),
            vec![reply("replacement", Some("r1"), 3600)],
            |renewer| renewer.replace_refused(&refused),
        );
        assert_eq!(outcome.expect("a token").token.expose(), "replacement");
        assert_eq!(asked.len(), 1);
        assert_eq!(on_disk(&path).access_token.expose(), "replacement");

        let (outcome, _, _) = run(
            "refused-fails",
            Some(&before),
            vec![Reply::json(503, "{}")],
            |renewer| renewer.replace_refused(&refused),
        );
        assert!(matches!(
            outcome,
            Err(TokenError::Refresh(Class::Unavailable))
        ));
    }

    /// A token another process made after the refusal is sent when the refresh fails and it has not
    /// expired, though it is inside the margin: it is not the refused token, and refusing to send
    /// it would fail a request that a usable token could carry.
    #[test]
    fn a_token_made_after_the_refusal_is_sent_when_the_refresh_fails() {
        let (outcome, _, _) = run(
            "after-refusal",
            Some(&stored("made-after", "r1", 30)),
            vec![Reply::json(503, "{}")],
            |renewer| renewer.replace_refused(&Secret::new("refused-one")),
        );

        assert_eq!(outcome.expect("a token").token.expose(), "made-after");
    }

    /// Where the file already holds a token other than the refused one, another process replaced
    /// it, and that one is used and nothing is presented: a second refresh would present a refresh
    /// token a rotating server has already replaced.
    #[test]
    fn a_refused_token_another_process_already_replaced_is_not_replaced_again() {
        let (outcome, asked, _) = run(
            "already",
            Some(&stored("the-other-process-made-this", "r1", 3000)),
            vec![],
            |renewer| renewer.replace_refused(&Secret::new("refused-one")),
        );

        assert_eq!(
            outcome.expect("a token").token.expose(),
            "the-other-process-made-this"
        );
        assert!(asked.is_empty());
    }

    /// A process that finds the claim held waits, reads the file again once it has it, and uses a
    /// token the holder wrote, presenting nothing. Without the second read, both would present
    /// the one refresh token.
    #[test]
    fn a_renewal_made_while_waiting_for_the_claim_is_used_and_not_made_again() {
        let path = scratch("claimed");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![]);
        let held = account::claim(&path).expect("a claim");
        let other = std::thread::spawn({
            let path = path.clone();
            move || {
                std::thread::sleep(Duration::from_millis(300));
                account::write_held(&path, &stored("made-meanwhile", "r1", 3000)).expect("a write");
                drop(held);
            }
        });

        let outcome = renewer(&server.base, &path).access_token();
        other.join().expect("the other process");

        assert_eq!(outcome.expect("a token").token.expose(), "made-meanwhile");
        assert!(server.finish().is_empty());
    }

    /// Stands a directory where the renewal's temporary would go, so the write fails and the file
    /// is left as it was. The name is the one `write_through` makes.
    fn block_writes(path: &Path) -> PathBuf {
        let mut name = path.file_name().expect("a name").to_os_string();
        name.push(format!(".{}.tmp", std::process::id()));
        let blocker = path.with_file_name(name);
        std::fs::create_dir(&blocker).expect("a blocker");
        blocker
    }

    use std::time::Duration;

    /// A renewal whose write fails still gives the token and says the write failed. The tokens
    /// are used from memory, and the next request writes them once the file can take them: the
    /// refresh token the file holds is dead, since the server replaced it.
    #[test]
    fn a_write_that_fails_is_reported_and_tried_again_before_the_next_request() {
        let path = scratch("unwritten");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![reply("new", Some("r1"), 3600)]);
        let mut renewer = renewer(&server.base, &path);
        let blocker = block_writes(&path);

        let first = renewer.access_token().expect("a token");
        assert_eq!(first.token.expose(), "new");
        assert!(first.unwritten.is_some());
        assert_eq!(on_disk(&path).refresh_token.expose(), "r0");

        // Still unwritable: the token is still good and the write is still reported.
        let second = renewer.access_token().expect("a token");
        assert_eq!(second.token.expose(), "new");
        assert!(second.unwritten.is_some());

        std::fs::remove_dir(&blocker).expect("a removed blocker");
        let third = renewer.access_token().expect("a token");

        assert_eq!(third.token.expose(), "new");
        assert!(third.unwritten.is_none());
        let file = on_disk(&path);
        assert_eq!(file.access_token.expose(), "new");
        assert_eq!(file.refresh_token.expose(), "r1");
        assert_eq!(server.finish().len(), 1, "the token was renewed once");
    }

    /// Where the file holds another token than the one this process replaced, another process has
    /// signed in or renewed since, and what memory holds is dropped rather than written over it.
    #[test]
    fn tokens_held_in_memory_are_dropped_where_the_file_holds_another() {
        let path = scratch("dropped");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![reply("mine", Some("r1"), 3600)]);
        let mut renewer = renewer(&server.base, &path);
        let blocker = block_writes(&path);
        renewer.access_token().expect("a token");
        std::fs::remove_dir(&blocker).expect("a removed blocker");
        account::save_at(&path, &stored("theirs", "r9", 3000)).expect("another sign-in");

        let access = renewer.access_token().expect("a token");

        assert_eq!(access.token.expose(), "theirs");
        let file = on_disk(&path);
        assert_eq!(file.access_token.expose(), "theirs");
        assert_eq!(file.refresh_token.expose(), "r9");
        assert!(!renewer.holds_unwritten_tokens());
        assert_eq!(server.finish().len(), 1);
    }

    /// A refresh refused as ended from tokens held in memory drops them and removes the file that
    /// still holds the token this process replaced, which the server killed when it gave the
    /// refused one. Leaving it would show a sign-in that the next process finds dead.
    #[test]
    fn an_ended_sign_in_removes_the_file_that_holds_the_token_this_process_replaced() {
        let path = scratch("ended-memory");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![
            reply("mine", Some("r1"), 10),
            Reply::json(400, "{\"error\":\"invalid_grant\"}"),
        ]);
        let mut renewer = renewer(&server.base, &path);
        let _blocker = block_writes(&path);
        renewer.access_token().expect("a token");

        let outcome = renewer.access_token();
        let asked = server.finish();

        assert!(matches!(outcome, Err(TokenError::Ended)), "{outcome:?}");
        assert_eq!(named(&asked[1].form(), "refresh_token"), "r1");
        assert!(!renewer.holds_unwritten_tokens());
        assert!(!path.exists());
    }

    /// The claim of a holder that is slow and alive, aged past the minute after which a claim is taken
    /// to belong to a process that died, so a second process breaks it.
    fn an_aged_claim(path: &Path) {
        std::fs::File::create(path.with_file_name("chatgpt-account.lock"))
            .expect("a claim")
            .set_modified(std::time::SystemTime::now() - Duration::from_secs(120))
            .expect("an aged claim");
    }

    /// CHATGPT-7: a refresh refused as ended removes the file only while it holds a refresh token this
    /// process knows to be spent. The holder whose claim was broken for its age writes the tokens it was
    /// given while the process that broke it is refused for the one the holder already spent, so the
    /// file is read again before it is removed, and the token found is the one used, with nothing asked
    /// of the endpoint for it. This is the shared renewer under the ChatGPT account. The regression it
    /// rejects is the removal of a sign-in another process had just renewed.
    #[test]
    fn a_refusal_of_a_spent_refresh_token_does_not_remove_what_another_process_wrote() {
        let path = scratch("race");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        an_aged_claim(&path);
        let server = serve(vec![
            Reply::json(400, "{\"error\":\"invalid_grant\"}").after({
                let path = path.clone();
                move || {
                    account::write_held(&path, &stored("from-the-holder", "r1", 3000))
                        .expect("the holder's write");
                }
            }),
        ]);

        let outcome = renewer(&server.base, &path).access_token();
        let asked = server.finish();

        assert_eq!(
            outcome.expect("the holder's token").token.expose(),
            "from-the-holder"
        );
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert_eq!(named(&asked[0].form(), "refresh_token"), "r0");
        let file = on_disk(&path);
        assert_eq!(file.access_token.expose(), "from-the-holder");
        assert_eq!(file.refresh_token.expose(), "r1");
    }

    /// CHATGPT-7: where the sign-in another process wrote is itself due, it is replaced once with its
    /// own refresh token and no more than once, and a second process that keeps replacing it is not
    /// chased: the second refusal, with the file changed again, removes nothing and is told as a refresh
    /// that failed. The regressions it rejects are a refresh that presents the token the file held
    /// before the other process wrote, and a loop that asks until the file stops changing.
    #[test]
    fn a_sign_in_another_process_wrote_that_is_due_is_replaced_once_and_not_chased() {
        let path = scratch("race-due");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![
            Reply::json(400, "{\"error\":\"invalid_grant\"}").after({
                let path = path.clone();
                move || {
                    account::write_held(&path, &stored("due-too", "r1", -5)).expect("a write");
                }
            }),
            reply("new", Some("r2"), 3600),
        ]);

        let outcome = renewer(&server.base, &path).access_token();
        let asked = server.finish();

        assert_eq!(outcome.expect("a token").token.expose(), "new");
        let presented: Vec<String> = asked
            .iter()
            .map(|each| named(&each.form(), "refresh_token").to_string())
            .collect();
        assert_eq!(presented, ["r0", "r1"]);
        assert_eq!(on_disk(&path).refresh_token.expose(), "r2");

        let path = scratch("race-chased");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![
            Reply::json(400, "{\"error\":\"invalid_grant\"}").after({
                let path = path.clone();
                move || {
                    account::write_held(&path, &stored("due-too", "r1", -5)).expect("a write");
                }
            }),
            Reply::json(400, "{\"error\":\"invalid_grant\"}").after({
                let path = path.clone();
                move || {
                    account::write_held(&path, &stored("due-as-well", "r2", -5)).expect("a write");
                }
            }),
        ]);

        let outcome = renewer(&server.base, &path).access_token();

        assert!(
            matches!(outcome, Err(TokenError::Refresh(Class::Refused))),
            "{outcome:?}"
        );
        assert_eq!(server.finish().len(), 2, "the file was chased");
        assert_eq!(
            on_disk(&path).refresh_token.expose(),
            "r2",
            "a file another process wrote was removed"
        );
    }

    /// A failure is named by its step and, for a request, its class, and by nothing a reply said:
    /// the words a person reads are chosen from these.
    #[test]
    fn each_failure_is_named_by_its_step_and_class_alone() {
        let named: [(TokenError, &str); 4] = [
            (TokenError::NotSignedIn, "not signed in"),
            (TokenError::Ended, "sign-in ended"),
            (
                TokenError::Refresh(Class::RateLimited),
                "refresh rate-limited",
            ),
            (
                TokenError::Store(AccountError::Malformed(crate::oauth::Defect::Empty)),
                "store malformed (Empty)",
            ),
        ];
        for (failure, wanted) in named {
            assert_eq!(failure.to_string(), wanted);
        }
    }
}
