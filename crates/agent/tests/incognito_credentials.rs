//! An incognito session spends an imported subscription and records the spend.
//!
//! # Why a binary of its own
//!
//! Two things here are process-wide. Engaging the mode is a one-way door for the life of the
//! process, which is the property that makes it trustworthy, and `HOME` is read by whatever asks
//! while it is set. A binary holding only this test has no other thread to race and no other test
//! to leave engaged, so neither has to be locked against anything.

use bravebot_aichat::Subscription;
use std::path::PathBuf;

/// A scratch state directory that removes itself.
struct Scratch {
    home: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let home = std::env::temp_dir().join("bravebot-agent-incognito-credentials");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("create scratch");
        // SAFETY: this binary holds one test, so nothing else is reading the environment.
        unsafe { std::env::set_var("HOME", &home) };
        bravebot_core::incognito::engage();
        Self { home }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// A credential in the shape the store holds one: 64 bytes of preimage and a 32 byte point, as
/// base64. Its value decides nothing here, only that presenting it succeeds so that the spend
/// reaches the wallet.
const A_TOKEN: &str = concat!(
    "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1",
    "Njc4OTo7PD0+PwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
);

/// A batch whose one credential is usable for long enough that the date the suite runs on cannot
/// decide the outcome.
fn a_batch() -> bravebot_skus::StoredCredentials {
    bravebot_skus::StoredCredentials {
        order_id: "aaaaaaaa-1111-4222-8333-444444444444".to_string(),
        environment: bravebot_skus::Environment::Production,
        item_id: "b7114ccc-b3a5-4951-9a5d-8b7a28731111".to_string(),
        issuer: "brave.com?sku=brave-leo-premium".to_string(),
        credentials: vec![bravebot_skus::store::Credential {
            unblinded: bravebot_skus::Secret::new(A_TOKEN),
            valid_from: "2000-01-01T00:00:00".to_string(),
            valid_to: "2099-01-01T00:00:00".to_string(),
            spent: false,
            rfc: true,
        }],
    }
}

/// INCOG-8: a credential spent in a private session is recorded as spent, and the batch it came
/// from is read and spent as it would be in any other session.
///
/// A credential is single use, so a session that presented one and left it looking unspent hands
/// the next session a credential the service has already seen. Both halves are asserted together:
/// a mode that refused the read as well would not be private but broken, since a session that
/// cannot reach a credential cannot reach a premium backend at all.
#[test]
fn a_spent_credential_is_written_back_in_a_private_session() {
    let _scratch = Scratch::new();

    // Seeded as an earlier ordinary session left it, which is the state this mode has to read.
    bravebot_skus::store::save(&a_batch()).expect("a write into the scratch home");

    {
        let mut subscription =
            bravebot_agent::ImportedSubscription::discover("https://ai-chat.bsg.brave.com")
                .found()
                .expect("an incognito session still reads the imported credentials");

        let credential = subscription
            .next_credential()
            .expect("an incognito session still spends a credential");
        assert!(
            !credential.cookie_value.is_empty(),
            "the turn was handed nothing to authenticate with"
        );
    }
    // Dropped by here, and a wallet with a destination flushes as it goes.

    let stored = bravebot_skus::store::load().expect("a read");
    assert!(
        stored.credentials[0].spent,
        "the credential this private session presented is still offered to the next one"
    );
}
