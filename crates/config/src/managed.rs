//! The machine-level layer: what an administrator pinned, above everything a person can set.
//!
//! Every other source of configuration is ultimately the individual's. The process environment
//! outranks the build and the settings files, so a value exported for one session is the last word
//! on where a request goes, which is the right answer while the machine belongs to the person using
//! it. It is the wrong answer where two parties have a legitimate say: an organisation that
//! requires inference traffic to reach an approved endpoint, or that will not have models reached
//! through somebody's personal cloud account, has nothing to say it with.
//!
//! So one file sits above the environment. Its authority is the filesystem's rather than this
//! program's: it lives in the directory the platform reserves for administration, and nothing here
//! checks who owns it or what the permissions on it are. A person who can write that path is a
//! person who can replace this binary, so a check would settle nothing and add a thing to get
//! wrong. What each platform actually grants that directory differs, and how far it can be relied
//! on is under Known costs in `docs/specs/backends.md`.
//!
//! Only the names that decide *where a request goes* may be pinned, and no credential is read from
//! here at all. A layer that could pin the theme is a layer somebody will use to pin the theme, and
//! a preference is not the thing two parties disagree about.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::env_var;
use crate::provider::Provider;
use crate::settings::Settings;

/// The file, inside whichever directory the platform reserves for an administrator.
const MANAGED_FILE: &str = "managed.json";

/// Where that directory is.
///
/// A literal rather than a path built from a variable. `%ProgramData%` and the rest are stated in
/// the environment of the process, which is the environment of the person this layer binds, so
/// reading one would let them choose which file answers for them. What that costs on a machine
/// whose system drive is not `C:` is under Known costs in `docs/specs/backends.md`.
#[cfg(target_os = "macos")]
const MANAGED_DIR: &str = "/Library/Application Support/bravebot";
#[cfg(windows)]
const MANAGED_DIR: &str = r"C:\ProgramData\bravebot";
#[cfg(all(unix, not(target_os = "macos")))]
const MANAGED_DIR: &str = "/etc/bravebot";

/// The names a managed layer may pin, which are the ones that decide where a request goes.
///
/// The endpoints and the AWS block, so that an approved host can be made the only host and the
/// switch that reaches somebody's own AWS account can be turned off. Not the signing key or the key
/// id: a layer that names a destination grants nothing, and a credential is the one value here that
/// would.
const PINNABLE: [&str; 8] = [
    env_var::ENDPOINT,
    env_var::PREMIUM_ENDPOINT,
    env_var::USE_BEDROCK,
    env_var::AWS_REGION,
    env_var::AWS_PROFILE,
    env_var::BEDROCK_OPUS_MODEL,
    env_var::BEDROCK_SONNET_MODEL,
    env_var::BEDROCK_HAIKU_MODEL,
];

/// The gateway block's name, which is pinned whole rather than a name at a time.
const PROVIDER_BLOCK: &str = "provider";

/// What the managed layer pinned, or nothing where there is no such file.
///
/// Not comparable, because a gateway it pinned may carry a token and [`crate::Secret`] refuses
/// equality. What a test wants of one of these is a field of it rather than the whole.
#[derive(Debug, Clone, Default)]
pub struct Managed {
    /// The pinnable variables the file set, blank ones left out.
    pins: BTreeMap<String, String>,
    /// The gateways it decided, or `None` where it said nothing about them.
    ///
    /// `Some` of an empty list is a file saying there are none, which is how an organisation
    /// refuses a gateway somebody configured for themselves. Absence cannot say that, so the
    /// presence of the block is kept rather than only what was under it.
    gateways: Option<Vec<Provider>>,
    /// The file, where there is one there at all.
    ///
    /// Recorded for a file that exists rather than for one that was understood, so that a report can
    /// distinguish a file nobody wrote from one that was found and honoured nothing. Those are
    /// different problems and only the second has a remedy.
    path: Option<PathBuf>,
}

/// Where the managed layer is read from on this platform, whether or not anything is there.
///
/// Public so that a report explaining a pin can name the file, and so an administrator reading the
/// documentation is told one path rather than three.
pub fn managed_file() -> PathBuf {
    Path::new(MANAGED_DIR).join(MANAGED_FILE)
}

impl Managed {
    /// Every pin in force on this machine.
    pub fn load() -> Self {
        Self::at(&managed_file())
    }

    /// As [`Managed::load`], for a named file, so a test needs no machine-level one.
    ///
    /// Not reachable from a variable, deliberately: see [`MANAGED_DIR`].
    pub fn at(path: &Path) -> Self {
        let found = path.exists().then(|| path.to_path_buf());
        let Some(root) = crate::settings::read(path) else {
            return Self {
                path: found,
                ..Self::default()
            };
        };
        let layer = Settings::from_map(&root);
        Self {
            pins: PINNABLE
                .iter()
                .filter_map(|name| {
                    let value = layer.get(name)?.trim();
                    match value.is_empty() {
                        // A blank is how somebody comments a line out, the same reading the rest of
                        // this configuration gives one, so it pins nothing rather than pinning the
                        // empty string.
                        true => None,
                        false => Some(((*name).to_string(), value.to_string())),
                    }
                })
                .collect(),
            // A block, and not merely a name. Anything else under `provider` is a mistyped file
            // rather than a decision about gateways, and reading it as one would take away every
            // gateway on the machine on the strength of a stray `null`.
            gateways: matches!(root.get(PROVIDER_BLOCK), Some(serde_json::Value::Object(_)))
                .then(|| approved(layer.providers())),
            path: found,
        }
    }

    /// What the managed layer pinned this name to, if it pinned it.
    ///
    /// Answering is the whole of the pin: a caller that gets a value here consults nothing else for
    /// that name.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.pins.get(name).map(String::as_str)
    }

    /// The gateways in force, or `None` where the managed layer said nothing about them.
    pub fn gateways(&self) -> Option<&[Provider]> {
        self.gateways.as_deref()
    }

    /// The names it pinned, for `doctor` to report.
    ///
    /// Names rather than values, for the reason the settings report gives: everyone on the machine
    /// can read this file, so the value is not the interesting half. A name in this list is the
    /// answer to why nothing a person sets is changing it.
    pub fn pinned(&self) -> impl Iterator<Item = &str> {
        self.pins
            .keys()
            .map(String::as_str)
            .chain(self.gateways.is_some().then_some(PROVIDER_BLOCK))
    }

    /// The file, where there is one there at all, read or not.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether anything is pinned at all.
    pub fn is_empty(&self) -> bool {
        self.pinned().next().is_none()
    }
}

/// The gateways as this layer may state them, which is without a token written into the file.
///
/// A gateway entry has a field for a credential and this file does not get to use it. Everyone on
/// the machine can read the managed layer, by design, so a token in it is a token handed to every
/// account rather than one held by its owner. Naming a variable still works and is the form the
/// gateway block recommends anyway.
///
/// Dropped rather than refused, because an entry that names a host is worth honouring: what a
/// stripped token costs is a gateway that has to be given its credential another way, and the
/// service says so on the first request.
fn approved(gateways: &[Provider]) -> Vec<Provider> {
    gateways
        .iter()
        .map(|gateway| Provider {
            api_key: None,
            ..gateway.clone()
        })
        .collect()
}

/// A managed layer written into a scratch directory, the real path being root's.
///
/// Outside the test module below so the resolution tests in [`crate`] can write one without a
/// second spelling of the file's name.
#[cfg(test)]
pub(crate) fn scratch(name: &str, text: &str) -> Managed {
    let dir = crate::testutil::scratch_dir(name);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let path = dir.join(MANAGED_FILE);
    std::fs::write(&path, text).expect("a managed file");
    Managed::at(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layer exists to make an approved host the only host, so the name that decides which host
    /// is reached has to be one it can answer for.
    #[test]
    fn an_endpoint_is_pinnable() {
        let managed = scratch(
            "managed-endpoint",
            r#"{"env": {"BRAVE_AI_CHAT_ENDPOINT": "https://approved.example"}}"#,
        );
        assert_eq!(
            managed.get(env_var::ENDPOINT),
            Some("https://approved.example")
        );
        assert_eq!(
            managed.pinned().collect::<Vec<_>>(),
            vec![env_var::ENDPOINT]
        );
    }

    /// A layer that can pin anything is a layer somebody uses to pin a preference. What an
    /// organisation and an individual can disagree about is where a request goes, so nothing else
    /// is read, however the file spells it.
    #[test]
    fn a_name_outside_the_pinnable_set_pins_nothing() {
        let managed = scratch(
            "managed-unpinnable",
            r#"{
                "model": "a-model",
                "effort": "max",
                "editorMode": "vim",
                "keybindings": {"stash": "ctrl-s"},
                "env": {
                    "SERVICES_KEY_AICHAT": "a-key",
                    "BRAVE_SERVICES_KEY_ID": "a-key-id",
                    "BRAVEBOT_CONTEXT_BUDGET": "4096",
                    "BRAVEBOT_SUBPROCESS_ENV_SCRUB": "0"
                }
            }"#,
        );
        assert!(managed.is_empty(), "nothing in that file may be pinned");
        for name in [
            env_var::SIGNING_KEY,
            env_var::KEY_ID,
            env_var::CONTEXT_BUDGET,
            env_var::SUBPROCESS_ENV_SCRUB,
            "model",
            // How hard a model thinks is what it costs and how long it takes, not where the request
            // goes, so it is a preference on the footing the theme is one (BACKEND-43).
            "effort",
        ] {
            assert_eq!(managed.get(name), None, "{name} is not pinnable");
        }
    }

    /// Half of "our account or nothing": the switch that reaches somebody's own AWS account is one
    /// of the names an organisation can answer for. The other half is the gateway block, since an
    /// entry there can name an AWS account too.
    #[test]
    fn the_switch_that_reaches_a_personal_account_is_pinnable() {
        let managed = scratch(
            "managed-no-bedrock",
            r#"{"env": {"BRAVEBOT_USE_BEDROCK": "0"}}"#,
        );
        assert_eq!(managed.get(env_var::USE_BEDROCK), Some("0"));
    }

    /// A gateway is a block rather than a variable, and an empty one is the only way to say there
    /// are none: an approved endpoint pins nothing while somebody can add a destination beside it.
    #[test]
    fn an_empty_gateway_block_says_there_are_no_gateways() {
        let managed = scratch("managed-no-gateways", r#"{"provider": {}}"#);
        assert!(
            managed.gateways().is_some_and(<[_]>::is_empty),
            "an empty block is a block saying there are none"
        );
        assert_eq!(
            managed.pinned().collect::<Vec<_>>(),
            vec![PROVIDER_BLOCK],
            "a block that was there is reported, empty or not"
        );
    }

    /// Saying nothing about gateways has to be distinguishable from saying there are none, or a
    /// file pinning only an endpoint would silently take away every gateway somebody configured.
    #[test]
    fn a_file_with_no_gateway_block_leaves_the_gateways_alone() {
        let managed = scratch(
            "managed-gateways-unmentioned",
            r#"{"env": {"BRAVE_AI_CHAT_ENDPOINT": "https://approved.example"}}"#,
        );
        assert!(managed.gateways().is_none());
    }

    /// The gateways an organisation approves are the ones in force, so the block is read as a
    /// settings file's is.
    #[test]
    fn a_gateway_block_names_the_gateways_in_force() {
        let managed = scratch(
            "managed-gateways",
            r#"{"provider": {"approved": {"options": {"baseURL": "https://gateway.example/v1"}}}}"#,
        );
        let gateways = managed.gateways().expect("a block");
        assert_eq!(gateways.len(), 1);
        assert_eq!(gateways[0].id, "approved");
    }

    /// The case every machine is in. Nothing is pinned, nothing is reported, and the report says
    /// nothing about a file nobody wrote.
    #[test]
    fn an_absent_file_pins_nothing_and_is_not_reported() {
        let managed = Managed::at(&crate::testutil::scratch_dir("managed-absent").join("no.json"));
        assert!(managed.is_empty());
        assert_eq!(managed.path(), None);
    }

    /// A half-typed file is absence rather than a refusal to start: the rest of the configuration
    /// still describes a working backend. It is still named, because a file found and understood by
    /// nothing is a different problem from one nobody wrote, and only the first has a remedy.
    #[test]
    fn an_unparseable_file_pins_nothing_and_is_still_named() {
        let managed = scratch("managed-unparseable", "{not json");
        assert!(managed.is_empty());
        assert!(
            managed.path().is_some(),
            "a file that is there has to be reportable"
        );
    }

    /// A stray value under `provider` is a mistyped file, not a decision that there are to be no
    /// gateways. Reading it as one would take every gateway on the machine away on the strength of a
    /// `null` somebody left behind.
    #[test]
    fn a_provider_key_that_is_not_a_block_decides_nothing() {
        for spelling in [
            r#"{"provider": []}"#,
            r#"{"provider": null}"#,
            r#"{"provider": 1}"#,
        ] {
            let managed = scratch("managed-provider-mistyped", spelling);
            assert!(managed.gateways().is_none(), "{spelling} decided something");
        }
    }

    /// Everyone on the machine can read this file, so a token in it is a token handed to every
    /// account. The host it names is still worth honouring.
    #[test]
    fn a_token_written_into_the_file_is_not_read() {
        let managed = scratch(
            "managed-gateway-token",
            r#"{"provider": {"approved": {"options": {
                "baseURL": "https://gateway.example/v1",
                "apiKey": "a-token"
            }}}}"#,
        );
        let gateways = managed.gateways().expect("a block");
        assert_eq!(gateways.len(), 1);
        assert!(gateways[0].api_key.is_none());
        assert_eq!(gateways[0].base_url, "https://gateway.example/v1");
    }

    /// A blank is how a line is commented out everywhere else in this configuration, so it cannot
    /// mean "pinned to nothing" here, which would make a placeholder an outage.
    #[test]
    fn a_blank_value_pins_nothing() {
        let managed = scratch(
            "managed-blank",
            r#"{"env": {"BRAVE_AI_CHAT_ENDPOINT": "  "}}"#,
        );
        assert!(managed.is_empty());
        assert_eq!(managed.get(env_var::ENDPOINT), None);
    }

    /// A file that exists is worth reporting even when nothing in it could be pinned, because the
    /// person who wrote it is otherwise told nothing about why it did nothing.
    #[test]
    fn a_file_that_was_read_is_named_even_where_it_pins_nothing() {
        let managed = scratch(
            "managed-named",
            r#"{"env": {"BRAVEBOT_CONTEXT_BUDGET": "4"}}"#,
        );
        assert!(managed.is_empty());
        assert!(managed.path().is_some());
    }
}
