//! The window's sign-in to one AWS account
//! ([BACKEND-70](../../../docs/specs/backends.md#BACKEND-70)): `aws sso login` for the profile the
//! row names, as `bravebot auth login bedrock` runs it for each account.
//!
//! The URL and the code the AWS CLI writes go to the main process as they are written, which shows
//! them only in the window it owns. The page that asked is told that the sign-in waits and how it
//! ended, in a sentence chosen from the value of the failure alone, so nothing the CLI wrote is
//! repeated to it.

use super::{Running, SignIns};
use crate::emit::Emitter;
use crate::protocol::{ErrorCode, Event, Failure, Request};
use bravebot_agent::accounts::{self, BedrockRecorded, BedrockSwitch};
use bravebot_agent::backend::{AwsStop, AwsStopped, Backend, CredentialError};
use bravebot_agent::home;
use bravebot_config::bedrock::{Bedrock, Tier};
use bravebot_config::{Config, ConfigError, Managed, env_var};
use bravebot_i18n::t;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

const WAY: &str = "bedrock";

/// The longest URL passed on. The CLI's is a few hundred characters; a longer one is not its.
const LONGEST_URL: usize = 2048;

/// The account a sign-in is for, read before anything starts.
pub(super) struct Planned {
    profile: Option<String>,
    account: Bedrock,
    /// Whether it is the tier variables' account, whose sign-in is a reason to record the switch.
    tier: bool,
    switch: BedrockSwitch,
}

/// The account `id` names among those the configuration names, or the sentence the sign-in is
/// refused with before the AWS CLI is started.
pub(super) fn plan(
    config: &Result<Config, ConfigError>,
    switch: BedrockSwitch,
    inherited: Option<&str>,
    id: Option<&str>,
) -> Result<Planned, String> {
    let Ok(config) = config else {
        return Err(t!(desktop_accounts_sign_in_misconfigured).into());
    };
    let named = accounts::aws_accounts(config, inherited);
    if named.is_empty() {
        return Err(match switch {
            BedrockSwitch::PinnedOff => t!(desktop_accounts_sign_in_bedrock_pinned_off).into(),
            BedrockSwitch::Off => t!(
                desktop_accounts_sign_in_bedrock_off,
                switch = env_var::USE_BEDROCK
            ),
            _ => t!(
                desktop_accounts_sign_in_no_aws_account,
                region = env_var::AWS_REGION,
                tiers = Tier::ALL
                    .into_iter()
                    .map(Tier::env_var)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    let Some((profile, account)) = named
        .into_iter()
        .find(|(profile, _)| profile.as_deref() == id)
    else {
        return Err(t!(desktop_accounts_sign_in_aws_gone).into());
    };
    Ok(Planned {
        tier: config
            .bedrock
            .as_ref()
            .is_some_and(|tier| std::ptr::eq(tier, account)),
        profile,
        account: account.clone(),
        switch,
    })
}

/// The URL and the code among the lines the AWS CLI wrote: the first `https://` word, and a line
/// that is a code of the shape the AWS sign-in page asks for, `ABCD-EFGH`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Shown {
    pub(super) url: Option<String>,
    pub(super) code: Option<String>,
}

impl Shown {
    /// Take what `line` holds that is not held yet. True where something was.
    pub(super) fn take(&mut self, line: &str) -> bool {
        let mut changed = false;
        if self.url.is_none()
            && let Some(url) = line
                .split_whitespace()
                .find(|word| word.starts_with("https://") && word.len() <= LONGEST_URL)
        {
            self.url = Some(url.to_string());
            changed = true;
        }
        if self.code.is_none() && is_code(line.trim()) {
            self.code = Some(line.trim().to_string());
            changed = true;
        }
        changed
    }
}

fn is_code(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 9
        && bytes[4] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(at, b)| at == 4 || b.is_ascii_uppercase() || b.is_ascii_digit())
}

impl SignIns {
    /// `accounts.signIn` for `bedrock`. The request names the row by its `id`, the profile, `null`
    /// being the default chain.
    pub(super) fn start_bedrock(
        &mut self,
        request: &Request,
        project: Option<&Path>,
        selected: Option<&Path>,
        emitter: &Emitter,
    ) -> Result<Value, Failure> {
        let id = match request.params.get("id") {
            Some(Value::String(id)) => Some(id.clone()),
            Some(Value::Null) => None,
            _ => {
                return Err(Failure::bad_request(
                    "an AWS sign-in needs the row's `id`, a profile or null",
                ));
            }
        };
        if let Some(path) = selected {
            crate::settings::validate(path)?;
        }
        let mut running = self
            .running
            .lock()
            .map_err(|_| Failure::new(ErrorCode::Internal, "the sign-in state is unavailable"))?;
        if running.is_some() {
            return Err(Failure::bad_request("a sign-in is already running"));
        }
        self.next += 1;
        let attempt = self.next.to_string();
        let layers = crate::settings::layers(project, selected);
        let managed = Managed::load();
        let planned = plan(
            &accounts::bedrock_config(&layers, &managed),
            accounts::bedrock_switch_now(&layers, &managed),
            accounts::inherited_profile().as_deref(),
            id.as_deref(),
        );
        let planned = match planned {
            Ok(planned) => planned,
            Err(refused) => {
                emitter.send(ended(&attempt, id.as_deref(), "failed", Some(&refused)));
                return Ok(json!({ "attempt": attempt }));
            }
        };
        let aws = Arc::new(AwsStop::new());
        *running = Some(Running {
            attempt: attempt.clone(),
            stop: Arc::new(AtomicBool::new(false)),
            aws: Some(Arc::clone(&aws)),
        });
        drop(running);
        let slot: Arc<Mutex<Option<Running>>> = Arc::clone(&self.running);
        let emitter = emitter.clone();
        let number = attempt.clone();
        std::thread::spawn(move || {
            let (outcome, sentence) = run(&number, &planned, &aws, &emitter);
            if let Ok(mut running) = slot.lock()
                && running
                    .as_ref()
                    .is_some_and(|running| running.attempt == number)
            {
                *running = None;
            }
            emitter.send(ended(
                &number,
                planned.profile.as_deref(),
                outcome,
                sentence.as_deref(),
            ));
        });
        Ok(json!({ "attempt": attempt }))
    }
}

fn ended(attempt: &str, id: Option<&str>, outcome: &str, sentence: Option<&str>) -> Event {
    Event::global(
        "accounts.signIn.ended",
        json!({
            "attempt": attempt,
            "way": WAY,
            "id": id,
            "outcome": outcome,
            "sentence": sentence,
        }),
    )
}

/// The sign-in itself, then the check a turn would make, then the switch recorded where the
/// command would record it.
fn run(
    attempt: &str,
    planned: &Planned,
    stop: &AwsStop,
    emitter: &Emitter,
) -> (&'static str, Option<String>) {
    let mut shown = Shown::default();
    let mut announced = false;
    let signed_in = Backend::sign_in_to_until(
        &planned.account,
        |line| {
            if !shown.take(&line) {
                return;
            }
            let name = if announced {
                "accounts.signIn.updated"
            } else {
                "accounts.signIn.started"
            };
            announced = true;
            emitter.send(Event::global(
                name,
                json!({
                    "attempt": attempt,
                    "way": WAY,
                    "id": planned.profile,
                    "url": shown.url,
                    "code": shown.code,
                }),
            ));
        },
        stop,
    );
    match signed_in {
        Err(AwsStopped::Cancelled) => ("cancelled", None),
        Err(AwsStopped::Failed(_)) if stop.stopped() => ("cancelled", None),
        Err(AwsStopped::Failed(failure)) => ("failed", Some(said_of_aws(&failure).into())),
        // Asked again, because `aws sso login` can finish for a profile whose credentials still
        // cannot be exported, and the export is what a turn signs with.
        Ok(()) if !Backend::signed_in_to(&planned.account) => (
            "failed",
            Some(t!(desktop_accounts_aws_still_signed_out).into()),
        ),
        Ok(()) => (
            "signedIn",
            recorded(
                planned,
                bravebot_core::incognito::engaged(),
                home::writable().as_deref(),
            ),
        ),
    }
}

/// What became of [`env_var::USE_BEDROCK`] once the tier variables' account signed in, where there
/// is something to say. An account a provider block names is used without the switch.
pub(super) fn recorded(
    planned: &Planned,
    incognito: bool,
    directory: Option<&Path>,
) -> Option<String> {
    if !planned.tier {
        return None;
    }
    let switch = env_var::USE_BEDROCK;
    match planned.switch {
        BedrockSwitch::Pinned | BedrockSwitch::PinnedOff | BedrockSwitch::Off => None,
        BedrockSwitch::Overruled => Some(t!(desktop_accounts_bedrock_overruled, switch = switch)),
        // INCOG-7: a session that leaves nothing behind records nothing either.
        BedrockSwitch::Recordable if incognito => Some(t!(
            desktop_accounts_bedrock_not_recorded_incognito,
            switch = switch
        )),
        BedrockSwitch::Recordable => {
            let Some(directory) = directory else {
                return Some(t!(desktop_accounts_bedrock_not_recorded, switch = switch));
            };
            match accounts::record_bedrock(directory) {
                Ok(BedrockRecorded::Already) => None,
                Ok(BedrockRecorded::Left) => {
                    Some(t!(desktop_accounts_bedrock_left, switch = switch))
                }
                Ok(BedrockRecorded::Written) => {
                    Some(t!(desktop_accounts_bedrock_recorded, switch = switch))
                }
                Err(_) => Some(t!(desktop_accounts_bedrock_not_recorded, switch = switch)),
            }
        }
    }
}

/// Why the AWS CLI did not sign in, chosen from the kind of failure, so it repeats nothing the CLI
/// wrote, which can name an account or a role.
pub(super) fn said_of_aws(failure: &CredentialError) -> &'static str {
    match failure {
        CredentialError::NotInstalled => t!(desktop_accounts_aws_no_cli),
        CredentialError::NoSuchProfile { .. } => t!(desktop_accounts_aws_no_profile),
        CredentialError::Refused { .. } => t!(desktop_accounts_aws_not_completed),
        CredentialError::Undecodable { .. } => t!(desktop_accounts_aws_undecodable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::Settings;

    const PROVIDER_ACCOUNT: &str = r#"{"provider": {"amazon-bedrock": {"options": {"region": "us-west-2", "profile": "other"}}}}"#;

    fn configured(tier: bool, settings: &str) -> Result<Config, ConfigError> {
        let mut config = Config::from_lookup(|name| match name {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-east-1".into()),
            name if name == Tier::Sonnet.env_var() => Some("a-model".into()),
            _ => None,
        })
        .expect("a configuration");
        if !tier {
            config.bedrock = None;
        }
        config.providers = Settings::parse(settings).providers().to_vec();
        Ok(config)
    }

    fn refused(
        config: &Result<Config, ConfigError>,
        switch: BedrockSwitch,
        id: Option<&str>,
    ) -> String {
        match plan(config, switch, None, id) {
            Ok(_) => panic!("planned"),
            Err(said) => said,
        }
    }

    /// A sign-in with nothing to sign in to is refused in a sentence that says why, before the AWS
    /// CLI is started: misconfigured settings, Bedrock pinned off or switched off with no provider
    /// block, no account at all, and a row the settings no longer name.
    #[test]
    fn a_sign_in_with_no_account_to_sign_in_to_is_refused_with_why() {
        let none = configured(false, "{}");
        assert_eq!(
            refused(
                &Err(ConfigError::Missing("X")),
                BedrockSwitch::Recordable,
                None
            ),
            t!(desktop_accounts_sign_in_misconfigured)
        );
        assert_eq!(
            refused(&none, BedrockSwitch::PinnedOff, None),
            t!(desktop_accounts_sign_in_bedrock_pinned_off)
        );
        let off = refused(&none, BedrockSwitch::Off, None);
        assert!(off.starts_with(env_var::USE_BEDROCK), "{off}");
        let nothing = refused(&none, BedrockSwitch::Recordable, None);
        assert!(nothing.contains(env_var::AWS_REGION), "{nothing}");
        assert!(nothing.contains(Tier::Sonnet.env_var()), "{nothing}");
        assert_eq!(
            refused(
                &configured(false, PROVIDER_ACCOUNT),
                BedrockSwitch::Off,
                Some("gone")
            ),
            t!(desktop_accounts_sign_in_aws_gone)
        );
    }

    /// The tier variables' account is the one whose sign-in records the switch; a provider block's
    /// account is used without it, so its sign-in records nothing.
    #[test]
    fn only_the_tier_account_is_a_reason_to_record_the_switch() {
        let both = configured(true, PROVIDER_ACCOUNT);
        let tier = plan(&both, BedrockSwitch::Recordable, None, None).expect("the tier account");
        assert!(tier.tier);
        assert_eq!(tier.profile, None);
        let block =
            plan(&both, BedrockSwitch::Recordable, None, Some("other")).expect("the block's");
        assert!(!block.tier);
        assert_eq!(block.profile.as_deref(), Some("other"));
        assert_eq!(recorded(&block, false, None), None);
        let pinned_off = configured(false, PROVIDER_ACCOUNT);
        assert!(plan(&pinned_off, BedrockSwitch::PinnedOff, None, Some("other")).is_ok());
    }

    /// An incognito session records nothing and says so; elsewhere the switch is written to the
    /// user's settings once, and a settings file that overrules it is reported rather than edited.
    #[test]
    fn signing_in_records_the_switch_except_in_incognito() {
        let directory = std::env::temp_dir().join(format!("bedrock-record-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let config = configured(true, "{}");
        let tier = plan(&config, BedrockSwitch::Recordable, None, None).unwrap();
        let switch = env_var::USE_BEDROCK;

        assert_eq!(
            recorded(&tier, true, Some(&directory)),
            Some(t!(
                desktop_accounts_bedrock_not_recorded_incognito,
                switch = switch
            ))
        );
        assert!(!directory.join("settings.json").exists());
        assert_eq!(
            recorded(&tier, false, None),
            Some(t!(desktop_accounts_bedrock_not_recorded, switch = switch))
        );
        assert_eq!(
            recorded(&tier, false, Some(&directory)),
            Some(t!(desktop_accounts_bedrock_recorded, switch = switch))
        );
        let written = std::fs::read_to_string(directory.join("settings.json")).unwrap();
        assert!(written.contains(switch), "{written}");
        assert_eq!(recorded(&tier, false, Some(&directory)), None);

        let overruled = plan(&config, BedrockSwitch::Overruled, None, None).unwrap();
        assert_eq!(
            recorded(&overruled, false, Some(&directory)),
            Some(t!(desktop_accounts_bedrock_overruled, switch = switch))
        );
        for quiet in [BedrockSwitch::Pinned, BedrockSwitch::Off] {
            let planned = plan(&config, quiet, None, None).unwrap();
            assert_eq!(recorded(&planned, false, Some(&directory)), None);
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The URL is the first `https://` word the CLI wrote and the code the first line of the AWS
    /// shape, each taken once; nothing else it wrote is kept.
    #[test]
    fn the_url_and_the_code_are_taken_from_what_the_cli_wrote() {
        let mut shown = Shown::default();
        assert!(!shown.take("Attempting to automatically open the SSO authorization page"));
        assert!(!shown.take("http://not-secure.example/"));
        assert!(shown.take("    https://device.sso.us-west-2.amazonaws.com/  "));
        assert!(!shown.take("https://second.example/"));
        assert!(!shown.take("abcd-efgh"));
        assert!(!shown.take("ABCD-EFGHI"));
        assert!(shown.take("  WXYZ-1234 "));
        assert!(!shown.take("QRST-5678"));
        assert_eq!(
            shown,
            Shown {
                url: Some("https://device.sso.us-west-2.amazonaws.com/".into()),
                code: Some("WXYZ-1234".into()),
            }
        );
        let mut long = Shown::default();
        assert!(!long.take(&format!("https://{}", "a".repeat(LONGEST_URL))));
    }

    /// Each failure is told in a sentence of its own kind, which repeats nothing the CLI wrote.
    #[test]
    fn a_failure_is_told_by_its_kind_alone() {
        let cases = [
            (
                CredentialError::NotInstalled,
                t!(desktop_accounts_aws_no_cli),
            ),
            (
                CredentialError::NoSuchProfile {
                    profile: "secret-profile".into(),
                    available: vec!["secret-other".into()],
                },
                t!(desktop_accounts_aws_no_profile),
            ),
            (
                CredentialError::Refused {
                    detail: "arn:aws:iam::123:role/secret".into(),
                },
                t!(desktop_accounts_aws_not_completed),
            ),
            (
                CredentialError::Undecodable {
                    detail: "secret".into(),
                },
                t!(desktop_accounts_aws_undecodable),
            ),
        ];
        for (failure, said) in cases {
            assert_eq!(said_of_aws(&failure), said);
            assert!(!said.contains("secret"));
        }
    }
}
