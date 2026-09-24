//! Where `~/.bravebot` is.
//!
//! `HOME` is process-wide, so these tests are the only ones in the crate that touch it and they
//! are kept in a file of their own. Everything else the agent reads from the home directory takes
//! the path as an argument, precisely so that no other test depends on the environment.

use std::path::PathBuf;
use std::sync::Mutex;

/// One lock for the whole file, not one per test.
///
/// `HOME` is process-wide, so every test here contends for the same thing. A mutex declared
/// inside each function would be a different mutex, and two tests would then be free to run at
/// once and see each other's HOME.
static HOME_LOCK: Mutex<()> = Mutex::new(());

/// Point HOME at a scratch directory for the duration of the closure.
fn with_temp_home<T>(name: &str, body: impl FnOnce(&PathBuf) -> T) -> T {
    let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let dir = std::env::temp_dir().join(format!("bravebot-agent-home-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch home");

    let previous = std::env::var_os("HOME");
    // SAFETY: single-threaded within the lock, and restored before returning.
    unsafe { std::env::set_var("HOME", &dir) };

    let result = body(&dir);

    match previous {
        Some(value) => unsafe { std::env::set_var("HOME", value) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// The directory is the user's, so it is found where the user's environment says it is, and
/// nowhere else. Guessing would mean reading files from a directory nobody chose.
#[test]
fn the_home_directory_is_the_one_the_environment_names() {
    with_temp_home("named", |home| {
        assert_eq!(
            bravebot_agent::home::directory(),
            Some(home.join(".bravebot"))
        );
    });
}

/// CMDLINE-4: a leading `~` stands for the profile directory itself, and the state directory is
/// one segment below it.
///
/// The two are one `join` apart, which is what makes answering with the wrong one silent: a
/// command line naming `~/notes.txt` would read `~/.bravebot/notes.txt`, a directory holding this
/// program's own settings and credentials rather than the person's files.
#[test]
fn the_profile_directory_is_the_home_itself_rather_than_the_state_directory() {
    with_temp_home("profile", |home| {
        assert_eq!(bravebot_agent::home::profile(), Some(home.clone()));
        assert_eq!(
            bravebot_agent::home::directory(),
            Some(home.join(".bravebot")),
            "the state directory moved out from under the profile directory"
        );
    });
}

/// PERM-3: a `~/` rule in a settings file is anchored at the home directory and a `/` rule at the
/// state directory inside it, when the accessor every caller reads is the one that answers.
///
/// The rule language is right about both and the anchors it is given decide where they land, so
/// the mistake this rejects is a caller handing over `~/.bravebot` where the home directory is
/// wanted: `Read(~/.ssh/**)` then covers `~/.bravebot/.ssh`, a directory nobody has, and the deny
/// rule protects nothing while reading as though it does.
#[test]
fn a_settings_rule_is_anchored_at_the_home_directory_the_accessor_names() {
    use bravebot_core::permissions::{Decision, Ruling, Subject};

    with_temp_home("permission-anchors", |home| {
        let settings = bravebot_config::Settings::parse(
            r#"{"permissions": {"deny": ["Read(~/.ssh/**)", "Read(/secrets/**)"]}}"#,
        );
        let (permissions, rejected) = bravebot_agent::permissions::from_settings(
            &settings,
            bravebot_agent::home::profile().as_deref(),
        );

        assert!(rejected.is_empty(), "{rejected:?}");
        assert_eq!(
            permissions.for_path(
                Subject::Read,
                &home.join(".ssh/id_ed25519").display().to_string()
            ),
            Decision::Ruled(Ruling::Deny),
            "a `~/` rule did not cover the home directory"
        );
        assert_eq!(
            permissions.for_path(
                Subject::Read,
                &home.join(".bravebot/secrets/key").display().to_string()
            ),
            Decision::Ruled(Ruling::Deny),
            "a `/` rule did not cover the directory the settings file is in"
        );
    });
}

/// Clear every variable the platform states a profile directory in, run the closure, and put them
/// back.
///
/// Every one of them, not `HOME` alone: the platform decides which it states a profile directory in,
/// and a test that left another set would be asking what happens when one variable is missing rather
/// than when there is no home at all.
fn with_no_home<T>(body: impl FnOnce() -> T) -> T {
    let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let previous: Vec<_> = bravebot_agent::home::PROFILE_VARIABLES
        .iter()
        .map(|variable| (variable, std::env::var_os(variable)))
        .collect();
    // SAFETY: single-threaded within the lock, and restored before returning.
    for (variable, _) in &previous {
        unsafe { std::env::remove_var(variable) };
    }

    let result = body();

    for (variable, value) in previous {
        match value {
            Some(value) => unsafe { std::env::set_var(variable, value) },
            None => unsafe { std::env::remove_var(variable) },
        }
    }
    result
}

/// Daemons and containers run without a home. That is a case to do without, never a reason to
/// refuse to start, since everything kept there is optional.
#[test]
fn an_absent_home_is_not_an_error() {
    let found = with_no_home(bravebot_agent::home::directory);

    assert_eq!(found, None, "a missing home invented a directory");
}

/// A machine that names no home has no directory for a `~` to stand for either, and inventing one
/// would put a path in front of somebody that is not where the bytes would go.
#[test]
fn an_absent_home_names_no_profile_directory_for_a_tilde() {
    let found = with_no_home(bravebot_agent::home::profile);

    assert_eq!(found, None, "a missing home invented a profile directory");
}

/// An empty HOME is a misconfigured environment, not the filesystem root. Joining onto it would
/// put the user's own files in `/.bravebot`.
#[test]
fn an_empty_home_is_treated_as_no_home_at_all() {
    let found = with_no_home(|| {
        // SAFETY: `with_no_home` holds the lock, and restores this along with the rest.
        unsafe { std::env::set_var("HOME", "") };
        bravebot_agent::home::directory()
    });

    assert_eq!(found, None, "an empty home was joined onto anyway");
}

/// A file left holding nothing must be reported, not read as never having imported.
///
/// A write interrupted partway leaves it that way, since the file is truncated before anything is
/// put in it. Passing over it quietly is the silent downgrade PREM-8 exists to prevent: the turn
/// spends no subscription, the endpoint answers a premium model name with a weaker model rather
/// than an error, and nothing on screen says the subscription needs importing again.
#[test]
fn an_empty_credentials_file_is_reported_rather_than_read_as_absent() {
    with_temp_home("empty-credentials", |_| {
        let path = bravebot_skus::store::path().expect("a scratch home");
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the state directory");
        std::fs::write(&path, "").expect("a file holding nothing");

        let discovery =
            bravebot_agent::ImportedSubscription::discover("https://ai-chat.bsg.brave.com");

        let complaint = discovery
            .complaint()
            .expect("a file that exists and cannot be read must say so");
        // Which cause and what to do about it. Every refusal names the import, so the cause is
        // asserted too: reported as corruption instead would send someone looking for a bad file.
        assert!(complaint.contains("holds nothing"), "{complaint}");
        assert!(complaint.contains("import-leo-creds"), "{complaint}");
        assert!(discovery.found().is_none(), "nothing spendable");
    });
}

/// A batch imported from the wrong channel must be reported, not passed over in silence.
///
/// A credential only verifies against the deployment that signed it, so a staging batch cannot be
/// spent against the production endpoint. Skipping it quietly is the silent downgrade PREM-8 exists
/// to prevent: the request goes out with no credential, the endpoint answers a premium model name
/// with a weaker model rather than an error, and the subscription the user is paying for goes unused
/// with nothing on screen to connect the two.
#[test]
fn a_subscription_imported_for_another_environment_is_reported() {
    with_temp_home("environment-mismatch", |_| {
        let batch = bravebot_skus::StoredCredentials {
            order_id: "aaaaaaaa-1111-4222-8333-444444444444".to_string(),
            environment: bravebot_skus::Environment::Staging,
            item_id: "b7114ccc-b3a5-4951-9a5d-8b7a28731111".to_string(),
            issuer: "brave.com?sku=brave-leo-premium".to_string(),
            credentials: vec![bravebot_skus::store::Credential {
                unblinded: bravebot_skus::Secret::new("token"),
                valid_from: "2026-08-22T00:00:00".to_string(),
                valid_to: "2099-08-23T00:00:00".to_string(),
                spent: false,
                rfc: true,
            }],
        };
        bravebot_skus::store::save(&batch).expect("a write into the scratch home");

        let discovery =
            bravebot_agent::ImportedSubscription::discover("https://ai-chat.bsg.brave.com");

        let complaint = discovery
            .complaint()
            .expect("a staging batch cannot be spent on production, and must say so");
        // Which environment it holds and what to do, since "premium is off" leaves nothing to act on.
        assert!(complaint.contains("staging"), "{complaint}");
        assert!(complaint.contains("import-leo-creds"), "{complaint}");
        assert!(discovery.found().is_none(), "nothing spendable");
    });
}

/// A build that knows the premium host and also names an AWS tier, so the model alone decides which
/// service a turn reaches.
fn premium_and_bedrock() -> bravebot_config::Config {
    bravebot_config::Config::from_lookup(|key| {
        match key {
            bravebot_config::env_var::SIGNING_KEY => Some("test-signing-key"),
            bravebot_config::env_var::KEY_ID => Some("test-key-id"),
            bravebot_config::env_var::ENDPOINT => Some("https://ai-chat.bsg.brave.com"),
            bravebot_config::env_var::PREMIUM_ENDPOINT => {
                Some("https://ai-chat-premium.bsg.brave.com")
            }
            bravebot_config::env_var::USE_BEDROCK => Some("1"),
            bravebot_config::env_var::AWS_REGION => Some("us-west-2"),
            bravebot_config::env_var::BEDROCK_OPUS_MODEL => Some("opus-arn"),
            _ => None,
        }
        .map(str::to_string)
    })
    .expect("configured")
}

/// A turn whose model is served by another backend is told nothing about the credential store.
///
/// The store is not read at all. An imported Leo credential means nothing to Bedrock or to a
/// gateway, so a batch that cannot be spent costs such a turn nothing and there is no downgrade to
/// report. Reported anyway, the line is wrong twice over: it says the turn fell back to the free
/// tier, which is not where a request signed for AWS went, and it sends somebody to re-import a
/// subscription that would have changed nothing about the answer they got.
#[test]
fn a_turn_on_another_backend_is_not_told_about_an_unusable_batch() {
    with_temp_home("another-backend", |_| {
        let path = bravebot_skus::store::path().expect("a scratch home");
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the state directory");
        std::fs::write(&path, "").expect("a file holding nothing");

        let config = premium_and_bedrock();
        let egress = bravebot_net::Egress::new();

        // The Brave roster still reports, the batch being one such a turn would have spent. Asserted
        // here rather than taken on trust, since a gate that silenced everything would pass the half
        // of this test that matters.
        let mut reporter = bravebot_agent::report::RecordingReporter::default();
        bravebot_agent::turn::discover_subscription(
            &config,
            &egress,
            Some(bravebot_config::DEFAULT_MODEL),
            &mut reporter,
        );
        assert_eq!(
            reporter.notices.len(),
            1,
            "a Brave turn lost its downgrade warning: {:?}",
            reporter.notices
        );

        let mut reporter = bravebot_agent::report::RecordingReporter::default();
        let found = bravebot_agent::turn::discover_subscription(
            &config,
            &egress,
            Some("opus-arn"),
            &mut reporter,
        );
        assert!(found.is_none(), "a Bedrock turn took a Leo credential");
        assert!(
            reporter.notices.is_empty(),
            "a Bedrock turn was told about the credential store: {:?}",
            reporter.notices
        );
    });
}

/// PREM-5: a delegate's turn does not read the credential store at all.
///
/// The store is one file and a spend reaches it only when the wallet is written back (PREM-6), so
/// a second wallet opened inside a run reads every credential that run has already spent as
/// unspent. A delegate spends what the turn lent it, which means the turn is the only run that
/// looks: the wallet here is the one there is, and a delegate handed none has none rather than
/// one of its own.
///
/// Asserted through the warning an unreadable batch produces, which is what a read of the store
/// leaves behind. It is also the warning's own rule: the line sends somebody to re-import a
/// subscription, and a turn that started three delegates would say it four times.
#[test]
fn a_delegate_does_not_open_a_wallet_of_its_own() {
    with_temp_home("delegate-reads-no-store", |home| {
        let path = bravebot_skus::store::path().expect("a scratch home");
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the state directory");
        std::fs::write(&path, "").expect("a file holding nothing");

        let config = premium_and_bedrock();
        let egress = bravebot_net::Egress::new();
        let workspace = bravebot_agent::Workspace::new(home).expect("a workspace");

        // The turn's own look, which is the one that reports. Asserted rather than assumed: a
        // store this build could not read at all would leave the delegate's half of this test
        // passing for the wrong reason.
        let mut watching = bravebot_agent::report::RecordingReporter::default();
        bravebot_agent::turn::discover_subscription(
            &config,
            &egress,
            Some(bravebot_config::DEFAULT_MODEL),
            &mut watching,
        );
        assert_eq!(
            watching.notices.len(),
            1,
            "the turn's own look at the store said nothing: {:?}",
            watching.notices
        );

        // Cancelled before it starts, so the delegate's turn gets as far as the store and no
        // further: what is being asked is whether it looks, not what it would send.
        let cancel = bravebot_core::cancel::Cancel::new();
        cancel.cancel();

        let mut trail = bravebot_core::event::RecordingSink::new();
        let mut routing = bravebot_core::Routing::new();
        routing.insert_trusted("task", "ask a delegate");
        let mut policy = bravebot_core::policy::Policy::begin(
            routing,
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([
                bravebot_core::capability::Capability::WebFetch,
                bravebot_core::capability::Capability::FileRead,
            ]),
            &mut trail,
        )
        .expect("a policy");
        let spec = policy
            .before_delegate(
                bravebot_core::delegate::DelegateId::nth(1),
                &bravebot_core::value::Labelled::new(
                    "reader".to_string(),
                    bravebot_core::label::Label::untrusted_public(),
                ),
                &bravebot_core::value::Labelled::new(
                    "look at it".to_string(),
                    bravebot_core::label::Label::untrusted_public(),
                ),
            )
            .expect("a delegate the gate allows");
        let seeded = bravebot_agent::delegate::seed(&policy, spec, None);
        policy.finish();

        let mut reporter = bravebot_agent::report::RecordingReporter::default();
        let mut sink = bravebot_core::event::RecordingSink::new();
        bravebot_agent::delegate::run(
            &seeded,
            &config,
            &egress,
            &workspace,
            None,
            None,
            None,
            bravebot_agent::PermissionMode::Ask,
            false,
            &bravebot_config::Attribution::default(),
            None,
            &cancel,
            &mut bravebot_agent::confirm::ApproveWrites,
            &mut reporter,
            &mut sink,
            // The turn found nothing to lend, which is what an unreadable batch leaves it with.
            None,
        );

        assert!(
            reporter.notices.is_empty(),
            "a delegate read the credential store: {:?}",
            reporter.notices
        );
    });
}
