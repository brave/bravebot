//! Replacing an access token before it expires
//! ([XAI-6](../../../../docs/specs/xai-sign-in.md#XAI-6)).
//!
//! The renewer is the one [`crate::oauth`] states for any sign-in, given the xAI account and the
//! refresh of [`super::exchange`]. Where the access token is within a minute of its expiry, the
//! refresh token is presented to the token endpoint under the claim on the file, the reply is
//! written, and only then is the new access token handed out. The states of that renewal (a claim
//! another process holds, a write the file could not take, a sign-in that ended) are exercised
//! through the ChatGPT account, which is the other kind of it. The tests below are what is particular
//! to this one: what a refresh presents, how the expiry of its reply is read, and the file it writes.

use super::account::Xai;

/// Hands out access tokens for the stored xAI sign-in, replacing one that is about to expire.
pub type Renewer = crate::oauth::Renewer<Xai>;

#[cfg(test)]
mod tests {
    use super::super::Hosts;
    use super::super::account::{self, Account};
    use super::*;
    use crate::Secret;
    use crate::oauth::encoding::{base64url, now};
    use crate::oauth::stand_in::{Reply, named, serve};
    use crate::oauth::{Class, TokenError};
    use std::path::{Path, PathBuf};

    fn scratch(name: &str) -> PathBuf {
        let directory = crate::testutil::scratch_dir(&format!("bravebot-xai-renew-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        directory.join("xai-account.json")
    }

    /// A stored sign-in whose access token expires `lasts` seconds from now.
    fn stored(access: &str, refresh: &str, lasts: i64) -> Account {
        Account {
            access_token: Secret::new(access),
            refresh_token: Secret::new(refresh),
            expires_at: now().saturating_add_signed(lasts),
        }
    }

    fn renewer(base: &str, path: &Path) -> Renewer {
        Renewer::at(
            Hosts::at(base),
            ureq::Agent::config_builder(),
            path.to_path_buf(),
        )
    }

    fn on_disk(path: &Path) -> Account {
        account::load_at(path)
            .expect("a read")
            .expect("a stored sign-in")
    }

    /// A token more than a minute from its expiry is sent as it is, and nothing is asked of the token
    /// endpoint. An implementation that renewed every time would spend the refresh token on each
    /// request.
    #[test]
    fn a_token_with_more_than_a_minute_left_is_used_and_nothing_is_asked() {
        let path = scratch("fresh");
        account::save_at(&path, &stored("still-good", "r0", 120)).expect("a stored account");
        let server = serve(vec![]);

        let access = renewer(&server.base, &path)
            .access_token()
            .expect("a token");

        assert_eq!(access.token.expose(), "still-good");
        assert!(server.finish().is_empty());
    }

    /// The refresh presents the stored refresh token under the client and the grant, in the fields
    /// opencode sends and no other, with no credential in a header, and what it gives is in the file
    /// before the token is handed out. A renewal that did not write would present a refresh token the
    /// server had replaced.
    #[test]
    fn a_refresh_presents_the_stored_refresh_token_under_the_client_and_writes_what_it_is_given() {
        let path = scratch("writes");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![Reply::json(
            200,
            "{\"access_token\":\"new\",\"refresh_token\":\"r1\",\"expires_in\":1800}",
        )]);

        let access = renewer(&server.base, &path)
            .access_token()
            .expect("a token");
        let asked = server.finish();

        assert_eq!(access.token.expose(), "new");
        assert!(access.unwritten.is_none());
        assert_eq!(asked.len(), 1, "{asked:?}");
        let asked = &asked[0];
        assert_eq!(
            (asked.method.as_str(), asked.path.as_str()),
            ("POST", "/token")
        );
        assert_eq!(
            asked.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(asked.header("user-agent"), Some("bravebot"));
        assert_eq!(asked.header("authorization"), None);
        let form = asked.form();
        let names: Vec<&str> = form.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["grant_type", "refresh_token", "client_id"]);
        assert_eq!(named(&form, "grant_type"), "refresh_token");
        assert_eq!(named(&form, "refresh_token"), "r0");
        assert_eq!(
            named(&form, "client_id"),
            "b1a00492-073a-47ea-816f-4c329264a828"
        );
        let file = on_disk(&path);
        assert_eq!(file.access_token.expose(), "new");
        assert_eq!(file.refresh_token.expose(), "r1");
        let wanted = now() + 1800;
        assert!(
            (wanted - 10..=wanted + 10).contains(&file.expires_at),
            "{} against {wanted}",
            file.expires_at
        );
    }

    /// A reply that grants no refresh token keeps the one presented: replacing it with nothing would
    /// sign the person out an hour later. A reply that grants one replaces it.
    #[test]
    fn a_reply_with_no_refresh_token_keeps_the_one_presented() {
        let path = scratch("kept");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![
            Reply::json(200, "{\"access_token\":\"new\",\"expires_in\":3600}"),
            Reply::json(
                200,
                "{\"access_token\":\"newer\",\"refresh_token\":null,\"expires_in\":3600}",
            ),
        ]);
        let mut renewing = renewer(&server.base, &path);

        renewing.access_token().expect("a token");
        let first = on_disk(&path);
        assert_eq!(first.access_token.expose(), "new");
        assert_eq!(first.refresh_token.expose(), "r0");
        // Expired again, so the next ask presents the refresh token that was kept.
        account::save_at(
            &path,
            &Account {
                expires_at: now() - 5,
                ..first
            },
        )
        .expect("an expired account");
        renewing.access_token().expect("a token");
        let asked = server.finish();

        assert_eq!(named(&asked[1].form(), "refresh_token"), "r0");
        assert_eq!(on_disk(&path).refresh_token.expose(), "r0");
    }

    /// Where the reply states no lifetime the expiry is read from the `exp` claim of an access token
    /// that is a JWT, held to between five minutes and a day from the reply, and where there is none, an
    /// hour on. An implementation that ignored the claim would store an hour for a token that states two
    /// hours, and one that did not hold it would store a token that never expires for an `exp` that is
    /// never reached, and one that is replaced on every ask for an `exp` that has passed.
    #[test]
    fn a_reply_with_no_lifetime_is_stored_with_the_tokens_own_expiry_or_an_hour() {
        let day = now() + 86_400;
        let dated = |exp: &str| {
            format!(
                "{}.{}.signature",
                base64url(b"{\"alg\":\"none\"}"),
                base64url(format!("{{\"exp\":{exp}}}").as_bytes())
            )
        };
        let (dated, never, passed) = (
            dated(&(now() + 7200).to_string()),
            dated("18446744073709551615"),
            dated("1"),
        );
        for (access, expected) in [
            (dated.as_str(), now() + 7200),
            (never.as_str(), day),
            (passed.as_str(), now() + 300),
            ("an-opaque-token", now() + 3600),
        ] {
            let path = scratch("expiry");
            account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
            let server = serve(vec![Reply::json(
                200,
                &serde_json::json!({"access_token": access, "refresh_token": "r1"}).to_string(),
            )]);

            renewer(&server.base, &path)
                .access_token()
                .expect("a token");
            server.finish();

            let stored = on_disk(&path).expires_at;
            assert!(
                (expected - 10..=expected + 10).contains(&stored),
                "{stored} against {expected}"
            );
        }
    }

    /// A refresh refused as the refresh token no longer being valid ends the sign-in: the file is
    /// removed, and every later ask of the renewer says it ended without asking the endpoint again.
    /// Any other refusal leaves the file where it is and says which class it was.
    #[test]
    fn a_refresh_refused_as_ended_removes_the_sign_in_and_any_other_leaves_it() {
        let path = scratch("ended");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![Reply::json(400, "{\"error\":\"invalid_grant\"}")]);
        let mut ending = renewer(&server.base, &path);

        assert!(matches!(ending.access_token(), Err(TokenError::Ended)));
        assert!(matches!(ending.access_token(), Err(TokenError::Ended)));
        assert_eq!(server.finish().len(), 1);
        assert!(!path.exists(), "the sign-in that ended was kept");

        for (reply, class) in [
            (
                Reply::json(503, "{\"error\":\"invalid_grant\"}"),
                Class::Unavailable,
            ),
            (
                Reply::json(400, "{\"error\":\"invalid_request\"}"),
                Class::Refused,
            ),
            (Reply::json(429, "{}"), Class::RateLimited),
            (Reply::json(200, "{\"nothing\":1}"), Class::Incomplete),
        ] {
            let path = scratch("failed");
            account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
            let server = serve(vec![reply]);

            let outcome = renewer(&server.base, &path).access_token();
            server.finish();

            assert!(
                matches!(outcome, Err(TokenError::Refresh(found)) if found == class),
                "{outcome:?} where {class:?} was wanted"
            );
            assert_eq!(on_disk(&path).refresh_token.expose(), "r0");
        }
    }

    /// A redirect in answer to a refresh is a failure and is not followed: it would carry the refresh
    /// token to a host nobody compiled in. The stored sign-in is left as it is.
    #[test]
    fn a_redirect_in_answer_to_a_refresh_is_not_followed_and_the_sign_in_is_left_alone() {
        let path = scratch("redirect");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let elsewhere = serve(vec![]);
        let server = serve(vec![Reply::redirect(&format!("{}/token", elsewhere.base))]);

        let outcome = renewer(&server.base, &path).access_token();

        assert!(
            matches!(outcome, Err(TokenError::Refresh(Class::Refused))),
            "{outcome:?}"
        );
        assert_eq!(server.finish().len(), 1);
        let reached = elsewhere.finish();
        assert!(reached.is_empty(), "the redirect was followed: {reached:?}");
        assert_eq!(on_disk(&path).refresh_token.expose(), "r0");
    }

    /// The claim of a holder that is slow and alive, aged past the minute after which a claim is taken to
    /// belong to a process that died, so a second process breaks it.
    fn an_aged_claim(path: &Path) {
        std::fs::File::create(path.with_file_name("xai-account.lock"))
            .expect("a claim")
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(120))
            .expect("an aged claim");
    }

    /// XAI-6: a refresh refused as ended removes the file only while it holds the refresh token that was
    /// refused. The claim of a holder that is slow and alive is judged abandoned by its age and broken,
    /// and the process that broke it presents a refresh token the holder has already spent and is told it
    /// is no longer valid, while the holder writes the tokens it was given. That refusal says nothing
    /// about what the holder wrote, so the file is read again before it is removed, and the token found
    /// there is the one used, with nothing asked of the endpoint for it. The regression it rejects is
    /// the removal of the sign-in another process had just written, which signs a person out of an
    /// account that was renewed.
    #[test]
    fn a_refusal_of_a_spent_refresh_token_does_not_remove_what_another_process_wrote() {
        let path = scratch("race");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        an_aged_claim(&path);
        let server = serve(vec![
            Reply::json(400, "{\"error\":\"invalid_grant\"}").after({
                let path = path.clone();
                move || {
                    crate::oauth::write_held::<Xai>(&path, &stored("from-the-holder", "r1", 3000))
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

    /// XAI-6: where the sign-in another process wrote is itself due, it is replaced once with its own
    /// refresh token, and no more than once. A second process that keeps replacing it is not chased:
    /// the refusal of the second refresh, with the file changed again, removes nothing and is told as a
    /// refresh that failed, which the next ask tries again. The regressions it rejects are a refresh
    /// that presents the token the file held before the other process wrote, and a loop that asks until
    /// the file stops changing.
    #[test]
    fn a_sign_in_another_process_wrote_that_is_due_is_replaced_once_and_not_chased() {
        let path = scratch("race-due");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let server = serve(vec![
            Reply::json(400, "{\"error\":\"invalid_grant\"}").after({
                let path = path.clone();
                move || {
                    crate::oauth::write_held::<Xai>(&path, &stored("due-too", "r1", -5))
                        .expect("a write");
                }
            }),
            Reply::json(
                200,
                "{\"access_token\":\"new\",\"refresh_token\":\"r2\",\"expires_in\":3600}",
            ),
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
                    crate::oauth::write_held::<Xai>(&path, &stored("due-too", "r1", -5))
                        .expect("a write");
                }
            }),
            Reply::json(400, "{\"error\":\"invalid_grant\"}").after({
                let path = path.clone();
                move || {
                    crate::oauth::write_held::<Xai>(&path, &stored("due-as-well", "r2", -5))
                        .expect("a write");
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

    /// XAI-6: a refresh that is made a second time under one claim, because the file another process
    /// changed after the first was refused is due as well, makes the claim as young as a new one first.
    /// The first request may have waited as long as a request may, and a claim is judged abandoned at
    /// sixty seconds, so a claim that kept the age it had could be broken while the second request waits
    /// and a third process could present the token that request is about to spend. The first reply
    /// ages the claim to fifty seconds, which is a first request that waited far past its deadline, and
    /// the second request reads the age of the claim when it arrives. The regression it rejects is a
    /// second request made under a claim a few seconds from being judged abandoned.
    #[test]
    fn a_second_refresh_under_one_claim_is_made_with_the_claim_as_young_as_a_new_one() {
        let path = scratch("claim-age");
        account::save_at(&path, &stored("old", "r0", -5)).expect("a stored account");
        let claim = path.with_file_name("xai-account.lock");
        let age = std::sync::Arc::new(std::sync::Mutex::new(None));
        let server = serve(vec![
            Reply::json(400, "{\"error\":\"invalid_grant\"}").after({
                let (path, claim) = (path.clone(), claim.clone());
                move || {
                    crate::oauth::write_held::<Xai>(&path, &stored("due-too", "r1", -5))
                        .expect("a write");
                    std::fs::OpenOptions::new()
                        .write(true)
                        .open(&claim)
                        .expect("the claim the renewer holds")
                        .set_modified(
                            std::time::SystemTime::now() - std::time::Duration::from_secs(50),
                        )
                        .expect("an aged claim");
                }
            }),
            Reply::json(
                200,
                "{\"access_token\":\"new\",\"refresh_token\":\"r2\",\"expires_in\":3600}",
            )
            .after({
                let age = std::sync::Arc::clone(&age);
                move || {
                    let found = std::fs::metadata(&claim)
                        .and_then(|found| found.modified())
                        .map(|written| written.elapsed().unwrap_or_default());
                    *age.lock().expect("the age") = Some(found);
                }
            }),
        ]);

        let outcome = renewer(&server.base, &path).access_token();
        let asked = server.finish();

        assert_eq!(outcome.expect("a token").token.expose(), "new");
        assert_eq!(asked.len(), 2, "{asked:?}");
        let age = age
            .lock()
            .expect("the age")
            .take()
            .expect("the second request was made")
            .expect("the claim was still there");
        assert!(
            age < std::time::Duration::from_secs(10),
            "the second request was made under a claim {age:?} old"
        );
    }

    /// Nothing stored is not signed in, and is not an ask of the endpoint.
    #[test]
    fn no_stored_sign_in_is_not_signed_in() {
        let path = scratch("none");
        let server = serve(vec![]);

        let outcome = renewer(&server.base, &path).access_token();

        assert!(matches!(outcome, Err(TokenError::NotSignedIn)));
        assert!(server.finish().is_empty());
    }
}
