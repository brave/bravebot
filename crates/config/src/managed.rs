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
//!
//! It may also keep an MCP server from starting, by the host it reaches or the command it runs, and
//! never add one (SERVERS-12). A layer that could declare a server would install a program on every
//! machine it reaches.

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

/// Where the servers a machine may start are listed, and the name `doctor` reports the list by
/// (SERVERS-12).
const SERVER_ALLOW: &str = "mcp.allow";

/// Where the servers it may not start are listed, and the name `doctor` reports that list by.
const SERVER_DENY: &str = "mcp.deny";

/// A server as the managed layer compares it with an entry (SERVERS-12).
#[derive(Debug, Clone, Copy)]
pub enum Server<'a> {
    /// A local server, by what it starts: the program as the absolute path a session resolved it
    /// to, then its arguments.
    Local(&'a [String]),
    /// A remote server, by the url it was declared with.
    Remote(&'a str),
}

/// One entry of `mcp.allow` or `mcp.deny`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// A host, lowercased and without a trailing dot, whose first label may be `*`.
    Host(String),
    /// An argv whose program is an absolute path.
    Command(Vec<String>),
}

impl Rule {
    /// Whether this entry names `server`, whose url's host, where it has one, is `host`.
    fn names(&self, server: Server<'_>, host: Option<&str>) -> bool {
        match (self, server) {
            (Rule::Command(argv), Server::Local(started)) => argv.as_slice() == started,
            (Rule::Host(entry), Server::Remote(_)) => host.is_some_and(|host| {
                match entry.strip_prefix("*.") {
                    // A label and a dot before the rest, so `*.corp.example` is neither
                    // `corp.example` nor `evilcorp.example`.
                    Some(rest) => host
                        .strip_suffix(rest)
                        .is_some_and(|head| head.len() > 1 && head.ends_with('.')),
                    None => host == entry,
                }
            }),
            _ => false,
        }
    }
}

/// Why the managed layer keeps a server from starting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal<'a> {
    /// `mcp.allow` is in force and no entry in it names the server.
    NotAllowed,
    /// This entry of `mcp.deny` names it.
    Denied(&'a Rule),
    /// `mcp.deny` names hosts, and the server's url spells its host in a way no entry can be
    /// compared with.
    HostUnread,
}

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
    /// The servers it allows, or `None` where it wrote no such list.
    ///
    /// `Some` of an empty list allows none, for the reason an empty gateway block names none.
    /// Neither this nor `denied` can hold a declaration, an approval or a request, so
    /// nothing written in this file can make a server reachable that was not already.
    allowed: Option<Vec<Rule>>,
    /// The servers it denies, which no allow entry brings back.
    denied: Vec<Rule>,
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
            allowed: server_list(&root, "allow").map(rules),
            denied: server_list(&root, "deny").map(rules).unwrap_or_default(),
            // Read, so there, whatever `exists` said a moment ago: a refusal always has a file to
            // name.
            path: Some(path.to_path_buf()),
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

    /// The file that keeps `server` from starting and why, where this layer keeps it from starting
    /// (SERVERS-12).
    ///
    /// The alias is not compared, because the person declaring a server chooses it. A deny entry
    /// is read before the allow list, so a server both name is denied.
    pub fn refuses(&self, server: Server<'_>) -> Option<(&Path, Refusal<'_>)> {
        let host = match server {
            Server::Remote(url) => plain_host(url),
            Server::Local(_) => None,
        };
        let refusal = match self
            .denied
            .iter()
            .find(|rule| rule.names(server, host.as_deref()))
        {
            Some(rule) => Some(Refusal::Denied(rule)),
            None if matches!(server, Server::Remote(_))
                && host.is_none()
                && self.denied.iter().any(|rule| matches!(rule, Rule::Host(_))) =>
            {
                Some(Refusal::HostUnread)
            }
            None => self
                .allowed
                .as_ref()
                .filter(|allowed| {
                    !allowed
                        .iter()
                        .any(|rule| rule.names(server, host.as_deref()))
                })
                .map(|_| Refusal::NotAllowed),
        };
        Some((self.path.as_deref()?, refusal?))
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
            .chain(self.allowed.is_some().then_some(SERVER_ALLOW))
            .chain((!self.denied.is_empty()).then_some(SERVER_DENY))
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

/// The entries of `mcp.allow` or `mcp.deny`, where that key holds a list.
///
/// Anything else there is a mistyped file rather than a decision, on the footing a stray value
/// under `provider` is one.
fn server_list<'a>(
    root: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a [serde_json::Value]> {
    match root.get("mcp").and_then(|block| block.get(key)) {
        Some(serde_json::Value::Array(entries)) => Some(entries),
        _ => None,
    }
}

/// The entries of a list that are one of the two forms, and nothing else.
///
/// An entry is an object with one key, `host` or `command`. Any other is skipped rather than
/// spoiling the list: in an allow list that allows less, and in a deny list it denies nothing, so
/// a deny list of nothing else is not reported as a pin that does nothing.
fn rules(entries: &[serde_json::Value]) -> Vec<Rule> {
    entries
        .iter()
        .filter_map(|entry| {
            let entry = entry.as_object().filter(|entry| entry.len() == 1)?;
            match (entry.get("host"), entry.get("command")) {
                (Some(serde_json::Value::String(host)), None) => host_rule(host).map(Rule::Host),
                (None, Some(serde_json::Value::Array(argv))) => {
                    command_rule(argv).map(Rule::Command)
                }
                _ => None,
            }
        })
        .collect()
}

/// A host entry, where it is a plain host whose first label may be `*`.
///
/// `*` is a whole label and only the first one: an entry that could match inside a label would be
/// one a person could satisfy with a name they registered.
fn host_rule(written: &str) -> Option<String> {
    let written = written.trim();
    match written.strip_prefix("*.") {
        Some(rest) => plain(rest)
            .filter(|name| !name.starts_with('['))
            .map(|name| format!("*.{name}")),
        None => plain(written),
    }
}

/// A command entry, where every word is a string and the program is an absolute path.
///
/// Absolute, because a session compares the path it resolved the program to, and a bare name is
/// no path any session resolves to.
fn command_rule(argv: &[serde_json::Value]) -> Option<Vec<String>> {
    let argv: Vec<String> = argv
        .iter()
        .map(|word| word.as_str().map(str::to_string))
        .collect::<Option<_>>()?;
    argv.first()
        .is_some_and(|program| Path::new(program).is_absolute())
        .then_some(argv)
}

/// The host a remote server's url names, where it is spelled plainly, and `None` where it is not.
///
/// Plain on purpose. The HTTP client reads the url with its own parser, and a host read here that
/// differed from the one it connects to would let a url match an entry it does not go to: a
/// backslash, a percent escape or a letter outside the ones a DNS name is spelled in is where two
/// parsers part. Such a url names no host, which no allow entry matches and no deny list naming a
/// host lets through.
fn plain_host(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let (host, port) = match authority.strip_prefix('[') {
        Some(bracketed) => {
            let (address, port) = bracketed.split_once(']')?;
            (&authority[..address.len() + 2], port)
        }
        None => authority
            .find(':')
            .map_or((authority, ""), |at| authority.split_at(at)),
    };
    let port = match port.strip_prefix(':') {
        Some(digits) => digits.bytes().all(|b| b.is_ascii_digit()),
        None => port.is_empty(),
    };
    port.then(|| plain(host)).flatten()
}

/// `written` lowercased with one trailing dot dropped, where it is a name in the letters a DNS name
/// is spelled in or an address in brackets, and `None` where it is anything else.
fn plain(written: &str) -> Option<String> {
    let lower = written.to_ascii_lowercase();
    if let Some(address) = lower
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        let address = !address.is_empty()
            && address
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.');
        return address.then_some(lower);
    }
    let name = lower.strip_suffix('.').unwrap_or(&lower);
    let labelled = !name.is_empty()
        && name.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        });
    labelled.then(|| name.to_string())
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

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    /// What the layer says about a server, without the file it says it from.
    fn refusal<'a>(managed: &'a Managed, server: Server<'_>) -> Option<Refusal<'a>> {
        managed.refuses(server).map(|(path, refusal)| {
            assert_eq!(Some(path), managed.path(), "a refusal names its file");
            refusal
        })
    }

    const CORP: &str = r#"{"mcp": {"allow": [
        {"host": "*.corp.example"},
        {"host": "[::1]"},
        {"command": ["/usr/local/bin/approved-server", "--stdio"]}
    ]}}"#;

    /// An allow list is the form that holds: the person chooses an alias and cannot choose what
    /// the list names, so a server it does not name is not started whatever it is called.
    #[test]
    fn an_allow_list_starts_only_what_it_names() {
        let managed = scratch("managed-allow", CORP);
        for url in [
            "https://mcp.corp.example/mcp",
            "https://a.b.corp.example:8443/",
            "HTTP://MCP.Corp.Example./mcp",
            "http://[::1]:8931/mcp",
        ] {
            assert_eq!(refusal(&managed, Server::Remote(url)), None, "{url}");
        }
        for url in [
            "https://corp.example/",
            "https://evilcorp.example/",
            "https://corp.example.evil.test/",
            "http://[::2]/",
        ] {
            assert_eq!(
                refusal(&managed, Server::Remote(url)),
                Some(Refusal::NotAllowed),
                "{url}"
            );
        }
        let approved = argv(&["/usr/local/bin/approved-server", "--stdio"]);
        assert_eq!(refusal(&managed, Server::Local(&approved)), None);
        for started in [
            argv(&["/usr/local/bin/approved-server"]),
            argv(&["/usr/local/bin/approved-server", "--stdio", "--more"]),
            argv(&["/opt/other-server", "--stdio"]),
        ] {
            assert_eq!(
                refusal(&managed, Server::Local(&started)),
                Some(Refusal::NotAllowed),
                "{started:?}"
            );
        }
        assert_eq!(managed.pinned().collect::<Vec<_>>(), vec![SERVER_ALLOW]);
    }

    /// An empty allow list allows nothing, which is how a machine says it runs no server at all.
    /// It is reported, because it decides something.
    #[test]
    fn an_empty_allow_list_starts_nothing() {
        let managed = scratch("managed-allow-none", r#"{"mcp": {"allow": []}}"#);
        assert_eq!(
            refusal(&managed, Server::Remote("https://mcp.corp.example/")),
            Some(Refusal::NotAllowed)
        );
        let started = argv(&["/usr/local/bin/approved-server"]);
        assert_eq!(
            refusal(&managed, Server::Local(&started)),
            Some(Refusal::NotAllowed)
        );
        assert_eq!(managed.pinned().collect::<Vec<_>>(), vec![SERVER_ALLOW]);
    }

    /// A deny entry wins over an allow entry naming the same server, so one host can be taken out
    /// of a domain the list allows. Case and a trailing dot are not a way around it.
    #[test]
    fn a_deny_entry_wins_over_an_allow_entry() {
        let managed = scratch(
            "managed-deny-wins",
            r#"{"mcp": {
                "allow": [
                    {"host": "*.corp.example"},
                    {"command": ["/usr/local/bin/approved-server", "--stdio"]}
                ],
                "deny": [
                    {"host": "Staging.Corp.Example."},
                    {"command": ["/usr/local/bin/approved-server", "--stdio"]}
                ]
            }}"#,
        );
        let staging = Rule::Host("staging.corp.example".into());
        for url in [
            "https://staging.corp.example/mcp",
            "https://STAGING.corp.example.:443/",
        ] {
            assert_eq!(
                refusal(&managed, Server::Remote(url)),
                Some(Refusal::Denied(&staging)),
                "{url}"
            );
        }
        assert_eq!(
            refusal(&managed, Server::Remote("https://mcp.corp.example/")),
            None
        );
        let approved = argv(&["/usr/local/bin/approved-server", "--stdio"]);
        assert_eq!(
            refusal(&managed, Server::Local(&approved)),
            Some(Refusal::Denied(&Rule::Command(approved.clone())))
        );
        assert_eq!(
            managed.pinned().collect::<Vec<_>>(),
            vec![SERVER_ALLOW, SERVER_DENY]
        );
    }

    /// Without an allow list the file refuses what it denies and nothing else, so a machine that
    /// denies one host leaves every other server the person's to decide about.
    #[test]
    fn without_an_allow_list_only_what_is_denied_is_refused() {
        let managed = scratch(
            "managed-deny-only",
            r#"{"mcp": {"deny": [{"host": "*.tracker.example"}, {"command": ["/opt/bad"]}]}}"#,
        );
        assert_eq!(
            refusal(&managed, Server::Remote("https://a.tracker.example/")),
            Some(Refusal::Denied(&Rule::Host("*.tracker.example".into())))
        );
        assert_eq!(
            refusal(&managed, Server::Local(&argv(&["/opt/bad"]))),
            Some(Refusal::Denied(&Rule::Command(argv(&["/opt/bad"]))))
        );
        for url in ["https://tracker.example/", "https://docs.example/"] {
            assert_eq!(refusal(&managed, Server::Remote(url)), None, "{url}");
        }
        for started in [argv(&["/opt/bad", "--flag"]), argv(&["/opt/good"])] {
            assert_eq!(
                refusal(&managed, Server::Local(&started)),
                None,
                "{started:?}"
            );
        }
        assert_eq!(managed.pinned().collect::<Vec<_>>(), vec![SERVER_DENY]);
    }

    /// A url whose host another parser could read as a different one is compared with no entry.
    /// `https://evil.test\.corp.example/` goes to `evil.test` for a WHATWG parser and would end in
    /// `.corp.example` for a split on `/`, so it is not allowed by `*.corp.example`, and a deny
    /// list naming a host refuses it rather than letting it past.
    #[test]
    fn a_host_two_parsers_could_read_apart_matches_no_entry() {
        let unplain = [
            r"https://evil.test\.corp.example/",
            "https://evil.test%2f.corp.example/",
            "https://staging%2Ecorp.example/",
            "https://st\u{e4}ging.corp.example/",
            "https://mcp.corp.example:80:90/",
            "https://mcp..corp.example/",
            "https://[::1/",
        ];
        let allowing = scratch("managed-unplain-allow", CORP);
        let denying = scratch(
            "managed-unplain-deny",
            r#"{"mcp": {"deny": [{"host": "staging.corp.example"}]}}"#,
        );
        let commands = scratch(
            "managed-unplain-commands",
            r#"{"mcp": {"deny": [{"command": ["/opt/bad"]}]}}"#,
        );
        for url in unplain {
            assert_eq!(
                refusal(&allowing, Server::Remote(url)),
                Some(Refusal::NotAllowed),
                "{url}"
            );
            assert_eq!(
                refusal(&denying, Server::Remote(url)),
                Some(Refusal::HostUnread),
                "{url}"
            );
            assert_eq!(
                refusal(&commands, Server::Remote(url)),
                None,
                "{url}: a list naming no host has none to compare"
            );
        }
    }

    /// A stray value is a mistyped file, not a decision, and nothing a person declared goes away on
    /// the strength of one. An empty deny list denies nothing, so it is not reported as a pin.
    #[test]
    fn a_list_that_is_not_a_list_decides_nothing() {
        for spelling in [
            r#"{"mcp": {"allow": "*.corp.example"}}"#,
            r#"{"mcp": {"allow": {"host": "*.corp.example"}}}"#,
            r#"{"mcp": {"allow": null}}"#,
            r#"{"mcp": {"deny": {"host": "mcp.corp.example"}}}"#,
            r#"{"mcp": {"deny": null}}"#,
            r#"{"mcp": {"deny": []}}"#,
            r#"{"mcp": ["deny", "weather"]}"#,
        ] {
            let managed = scratch("managed-lists-mistyped", spelling);
            assert_eq!(
                refusal(&managed, Server::Remote("https://mcp.corp.example/")),
                None,
                "{spelling} refused a server"
            );
            assert!(managed.is_empty(), "{spelling} was reported as a pin");
        }
    }

    /// An entry that is neither form is skipped. In a deny list that denies nothing, so a list of
    /// nothing else is not reported; in an allow list it allows nothing, so the list still stands
    /// and allows less. An alias is one of them: the person declaring a server chooses it.
    #[test]
    fn an_entry_in_neither_form_is_skipped() {
        let malformed = r#"[
            "weather",
            {"alias": "weather"},
            {"host": "*"},
            {"host": "a.*.corp.example"},
            {"host": "*corp.example"},
            {"host": "*.[::1]"},
            {"host": ""},
            {"host": "mcp.corp.example/path"},
            {"command": ["approved-server", "--stdio"]},
            {"command": []},
            {"command": ["/usr/local/bin/approved-server", 3]},
            {"host": "mcp.corp.example", "command": ["/usr/local/bin/approved-server"]},
            {"host": "mcp.corp.example", "port": 443}
        ]"#;
        let remote = Server::Remote("https://mcp.corp.example/");
        let program = argv(&["/usr/local/bin/approved-server"]);
        let denying = scratch(
            "managed-deny-malformed",
            &format!(r#"{{"mcp": {{"deny": {malformed}}}}}"#),
        );
        assert!(denying.is_empty(), "a deny list of nothing was reported");
        assert_eq!(refusal(&denying, remote), None);
        assert_eq!(refusal(&denying, Server::Local(&program)), None);
        let allowing = scratch(
            "managed-allow-malformed",
            &format!(r#"{{"mcp": {{"allow": {malformed}}}}}"#),
        );
        assert_eq!(refusal(&allowing, remote), Some(Refusal::NotAllowed));
        assert_eq!(
            refusal(&allowing, Server::Local(&program)),
            Some(Refusal::NotAllowed)
        );
        let spaced = scratch(
            "managed-deny-spaced",
            r#"{"mcp": {"deny": [{"host": "  MCP.corp.example  "}]}}"#,
        );
        assert!(refusal(&spaced, remote).is_some());
    }

    /// The layer may keep a server from starting and never add one. [`Managed`] has nowhere to
    /// hold a server, so a declaration, a request or an approval spelled here is read as nothing,
    /// and an allow entry permits a declaration the person made rather than making one.
    #[test]
    fn a_server_declared_or_requested_here_is_read_as_nothing() {
        let managed = scratch(
            "managed-declares",
            r#"{
                "mcp": {
                    "request": ["weather"],
                    "approve": ["weather"],
                    "weather": {"command": "weather-mcp", "args": ["--stdio"]}
                },
                "mcpServers": {"weather": {"command": "weather-mcp"}}
            }"#,
        );
        assert!(managed.is_empty(), "a server was read out of the file");
        let started = argv(&["/usr/local/bin/weather-mcp", "--stdio"]);
        assert_eq!(refusal(&managed, Server::Local(&started)), None);
    }
}
