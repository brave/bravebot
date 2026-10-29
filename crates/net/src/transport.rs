//! What this process trusts, and what it goes through, for every HTTP client in it.
//!
//! Two things a person configures outside this program and expects every program on the machine
//! to honour: the certificate authorities a TLS handshake is validated against, and the proxy a
//! request is routed through. Both are read here, once, so the egress gate and the subscription
//! client answer the same way rather than each inheriting whatever its own client happened to
//! default to.
//!
//! Neither is inherited. The transport library's own defaults read the environment for a proxy
//! and compile a root set into the binary, and a version that changed either would change what
//! this program trusts with nothing in this repository saying so.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use ureq::Proxy;
use ureq::ProxyProtocol;
use ureq::config::ConfigBuilder;
use ureq::tls::{Certificate, ClientCert, PemItem, PrivateKey, RootCerts, TlsConfig, parse_pem};
use ureq::typestate::AgentScope;

/// The variable naming a file of certificates to trust instead of the built-in set.
pub const CERTIFICATE_FILE: &str = "SSL_CERT_FILE";

/// The variable naming a directory of them.
pub const CERTIFICATE_DIRECTORY: &str = "SSL_CERT_DIR";

/// The variable naming the file that holds the certificate (and its chain) this process presents to a
/// server that asks for one.
pub const CLIENT_CERTIFICATE: &str = "BRAVEBOT_CLIENT_CERT";

/// The variable naming the file that holds that certificate's private key, unencrypted or
/// encrypted as PKCS#8 (`BEGIN ENCRYPTED PRIVATE KEY`).
pub const CLIENT_KEY: &str = "BRAVEBOT_CLIENT_KEY";

/// The variable holding the passphrase of an encrypted [`CLIENT_KEY`]. Read from the environment
/// only, never from a settings file, and never printed.
pub const CLIENT_KEY_PASSPHRASE: &str = "BRAVEBOT_CLIENT_KEY_PASSPHRASE";

/// The variables a proxy is named in, in the order the transport reads them. Each is read in lower
/// case as well, which is the form some networks hand out, and the first one set wins.
pub const PROXY_VARIABLES: &[&str] = &["ALL_PROXY", "HTTPS_PROXY", "HTTP_PROXY"];

/// The variable naming the hosts a proxy is not used for.
pub const NO_PROXY: &str = "NO_PROXY";

/// The names a settings file may state in place of the environment, which is BACKEND-33's reading of
/// these six and no others. A caller that owns the settings hands them to [`Transport::settle`].
pub const SETTINGS_NAMES: [&str; 6] = [
    CERTIFICATE_FILE,
    CERTIFICATE_DIRECTORY,
    "ALL_PROXY",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    NO_PROXY,
];

/// Which certificate authorities a server is validated against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustRoots {
    /// The set compiled into this build.
    Bundled,
    /// The paths the environment named, and nothing else.
    ///
    /// Replacing rather than extending, because that is what every other client on the machine
    /// does with these variables: a bundle is the whole of what to trust, and a person pinning a
    /// private authority would otherwise still be accepting the public ones.
    Named {
        file: Option<PathBuf>,
        directory: Option<PathBuf>,
    },
}

impl TrustRoots {
    /// What the environment asks for.
    ///
    /// Takes a lookup rather than reading the process environment, so what the variables mean can
    /// be pinned by a test without a global that the rest of the suite runs against.
    pub fn from_env(lookup: impl Fn(&str) -> Option<OsString>) -> Self {
        let file = named_path(&lookup, CERTIFICATE_FILE);
        let directory = named_path(&lookup, CERTIFICATE_DIRECTORY);
        match (&file, &directory) {
            (None, None) => Self::Bundled,
            _ => Self::Named { file, directory },
        }
    }

    /// The paths this names, in the order they are read.
    pub fn paths(&self) -> Vec<&Path> {
        match self {
            Self::Bundled => Vec::new(),
            Self::Named { file, directory } => file
                .iter()
                .chain(directory.iter())
                .map(PathBuf::as_path)
                .collect(),
        }
    }
}

/// The passphrase the environment states, where it states one. Empty names none.
fn named_passphrase(lookup: &impl Fn(&str) -> Option<OsString>) -> Option<String> {
    lookup(CLIENT_KEY_PASSPHRASE)
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty())
}

/// A variable that is set but empty names nothing. A shell that exports one unconditionally leaves
/// it empty rather than unset, and reading that as a path would refuse every connection the machine
/// makes for a variable nobody meant to set.
fn named_path(lookup: &impl Fn(&str) -> Option<OsString>, name: &str) -> Option<PathBuf> {
    lookup(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Why the certificates the environment named cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustError {
    /// The path that failed, as the variable gave it.
    pub path: PathBuf,
    pub detail: String,
}

impl fmt::Display for TrustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.detail)
    }
}

impl std::error::Error for TrustError {}

/// What a settings file stated, set once by [`Transport::settle`].
static FROM_FILE: OnceLock<BTreeMap<String, String>> = OnceLock::new();

/// What the environment and that file resolved to, read once for the whole process.
static SHARED: OnceLock<Transport> = OnceLock::new();

/// What every HTTP client in this process trusts, and what it goes through.
#[derive(Clone)]
pub struct Transport {
    roots: TrustRoots,
    /// The certificates [`TrustRoots::Named`] resolved to, empty where nothing named any.
    certificates: Vec<Certificate<'static>>,
    /// Every named path that could not be used, in the order the paths are read.
    trust_problems: Vec<TrustError>,
    /// Only ever a protocol this build can connect through. See [`Transport::resolve`].
    proxy: Option<Proxy>,
    /// The protocol of a proxy that was named and cannot be used.
    unusable_proxy: Option<String>,
    /// The names of the proxy variables that are set and whose value cannot be read as a proxy
    /// address, in the order the variables are read. Names only: a proxy uri carries a credential.
    unparseable_proxies: Vec<String>,
    /// What `NO_PROXY` held, for the report. The transport library reads it for itself.
    no_proxy: Option<String>,
    /// Whether the roots came from a settings file because the environment named none.
    roots_from_file: bool,
    /// Whether the proxy came from a settings file because the environment named none.
    proxy_from_file: bool,
    /// The certificate and key the environment named, once both loaded and agreed with each other.
    client: Option<ClientCert>,
    /// The paths the environment named for them, whether or not they could be used.
    client_paths: ClientPaths,
    /// Every named path that could not be used, in the order the paths are read.
    client_problems: Vec<TrustError>,
    /// The passphrase of the key, kept to open it again when the files are read again (NET-11).
    /// Never printed: [`Transport`]'s `Debug` says only whether one was given.
    client_passphrase: Option<String>,
}

/// The certificate and key the named files held at one reading, or why they could not be used.
///
/// Held by whoever builds clients from it, so a pair read again after a failed handshake replaces
/// the one in use without the process-wide [`Transport`] changing under every other client.
#[derive(Clone)]
pub struct ClientPair {
    cert: Option<ClientCert>,
    problems: Vec<TrustError>,
    /// A hash of the certificate chain and the key, to tell whether a reading found a different
    /// pair. Zero where there is none.
    identity: u64,
}

impl ClientPair {
    fn of(cert: Option<ClientCert>, problems: Vec<TrustError>) -> Self {
        use std::hash::{Hash, Hasher};
        let identity = cert.as_ref().map_or(0, |pair| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for certificate in pair.certs() {
                certificate.der().hash(&mut hasher);
            }
            pair.private_key().der().hash(&mut hasher);
            hasher.finish()
        });
        Self {
            cert,
            problems,
            identity,
        }
    }

    /// Whether there is a pair to present.
    pub fn is_loaded(&self) -> bool {
        self.cert.is_some()
    }

    /// Why the files could not be used, where they could not.
    pub fn problems(&self) -> &[TrustError] {
        &self.problems
    }

    /// Whether both hold the same certificate and key, or both hold none.
    pub fn same_as(&self, other: &ClientPair) -> bool {
        self.identity == other.identity
    }
}

/// What reading the named files again found.
pub struct Reread {
    /// The pair to use from here: the one just read, or the one in use where the new reading could
    /// not be used (NET-11).
    pub pair: ClientPair,
    /// Which of the two files could not be used in this reading, named `certificate` or `key`.
    pub failed: Vec<&'static str>,
}

/// The two files a client certificate is named by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientPaths {
    pub certificate: Option<PathBuf>,
    pub key: Option<PathBuf>,
}

impl ClientPaths {
    fn from_env(lookup: impl Fn(&str) -> Option<OsString>) -> Self {
        Self {
            certificate: named_path(&lookup, CLIENT_CERTIFICATE),
            key: named_path(&lookup, CLIENT_KEY),
        }
    }
}

impl Transport {
    /// What the environment asks for, read once for the whole process.
    ///
    /// Once, because a certificate directory is every file in it and a client is built for each
    /// turn, each model roster and each update check; and because two clients resolving the
    /// environment at different moments could disagree about what this process trusts.
    pub fn shared() -> &'static Self {
        SHARED.get_or_init(Self::from_env)
    }

    /// Hand over what a settings file stated for [`SETTINGS_NAMES`], before the first call to
    /// [`Transport::shared`] resolves them.
    ///
    /// Returns whether they will be read: false where something already resolved the transport, or
    /// where an earlier call settled it, since a value arriving after the answer is a value that
    /// decides nothing and the caller is the one to say so. A blank value names nothing, as it does
    /// in the environment, and a name outside the six is not kept.
    pub fn settle(values: impl IntoIterator<Item = (String, String)>) -> bool {
        FROM_FILE.set(stated_names(values)).is_ok() && SHARED.get().is_none()
    }

    /// What a settings file states and the environment does not, resolved as [`Transport::shared`]
    /// resolves it with an environment that names nothing.
    ///
    /// Exists so a test can hold the route and the roots a file produces to what a client is built
    /// with, which is otherwise reachable only through the process-wide answer every other test in
    /// the suite shares.
    pub fn stated_by_file(values: impl IntoIterator<Item = (String, String)>) -> Self {
        let stated = stated_names(values);
        Self::from_sources(
            &|_| None,
            &|name| stated.get(name).map(OsString::from),
            || None,
        )
    }

    fn from_env() -> Self {
        let stated = FROM_FILE.get();
        Self::from_sources(
            &|name| std::env::var_os(name),
            &|name| {
                stated
                    .and_then(|values| values.get(name))
                    .map(OsString::from)
            },
            // The transport library's own reader, so the variables it looks at and the `NO_PROXY`
            // exceptions it honours are the ones every other client of it honours. Called here
            // rather than left to its default, so a release that stopped calling it changes
            // nothing about this program and the tests below fail rather than the proxy quietly
            // going away.
            Proxy::try_from_env,
        )
    }

    /// The environment and a settings file, the environment first (NET-7, NET-8).
    ///
    /// Each answer comes whole from one of the two: the roots from the environment if it names
    /// either path, and the proxy from the environment if it names any of the three variables,
    /// whether or not they parse. Mixing them name by name would let a file's `ALL_PROXY` outrank an
    /// exported `HTTPS_PROXY`, which is the file beating the environment through the order the
    /// variables are read in.
    fn from_sources(
        environment: &impl Fn(&str) -> Option<OsString>,
        file: &impl Fn(&str) -> Option<OsString>,
        environment_proxy: impl FnOnce() -> Option<Proxy>,
    ) -> Self {
        let (roots, roots_from_file) = match TrustRoots::from_env(environment) {
            TrustRoots::Bundled => match TrustRoots::from_env(file) {
                TrustRoots::Bundled => (TrustRoots::Bundled, false),
                named => (named, true),
            },
            named => (named, false),
        };
        let no_proxy_of = |lookup: &dyn Fn(&str) -> Option<OsString>| {
            [NO_PROXY, "no_proxy"]
                .iter()
                .find_map(|name| lookup(name).and_then(|value| value.into_string().ok()))
                .filter(|value| !value.is_empty())
        };
        let names_a_proxy = |lookup: &dyn Fn(&str) -> Option<OsString>| {
            PROXY_VARIABLES.iter().any(|variable| {
                [variable.to_string(), variable.to_ascii_lowercase()]
                    .iter()
                    .any(|name| lookup(name).is_some_and(|value| !value.is_empty()))
            })
        };

        let from_the_file = !names_a_proxy(environment) && names_a_proxy(file);
        let mut transport = if from_the_file {
            // `NO_PROXY` in the environment still wins over the file's, as every name does, and
            // applies to a route the file stated.
            let no_proxy = no_proxy_of(environment).or_else(|| no_proxy_of(file));
            let proxy = proxy_stated(file, no_proxy.as_deref());
            let mut transport = Self::resolve(roots, proxy);
            transport.unparseable_proxies = unparseable_proxy_variables(file);
            transport.no_proxy = no_proxy;
            transport.roots_from_file = roots_from_file;
            transport.proxy_from_file = true;
            transport
        } else {
            // Which variables the reader below passes over, read for the report alone. It skips a
            // value that is not a uri and goes on to the next variable, so without this the route it
            // chose and the variable somebody set are two different statements and the report makes
            // only the first.
            let unparseable = unparseable_proxy_variables(environment);
            let mut transport = Self::resolve(roots, environment_proxy());
            transport.unparseable_proxies = unparseable;
            transport.no_proxy = no_proxy_of(environment);
            transport.roots_from_file = roots_from_file;
            transport
        };
        // The environment alone: a pair of files is a credential's address, and a settings file is
        // not where NET-11 reads one from.
        transport.use_client_pair(
            ClientPaths::from_env(environment),
            named_passphrase(environment),
        );
        transport
    }

    /// The transport a caller states rather than the environment's.
    ///
    /// Exists so what a report of the transport says can be held to in a test. Nothing in the
    /// product states one: there is a single answer to what this process trusts and where it sends
    /// requests, and it is the environment's. A proxy uri that cannot be parsed names no proxy,
    /// which is what an unusable one in the environment does.
    pub fn stated(roots: TrustRoots, proxy: Option<&str>, no_proxy: Option<&str>) -> Self {
        let mut transport = Self::resolve(roots, proxy.and_then(|uri| Proxy::new(uri).ok()));
        transport.no_proxy = no_proxy.map(str::to_string);
        transport
    }

    /// The transport a caller states, together with the proxy variables that hold something that is
    /// not a uri.
    ///
    /// Exists for the same reason [`Transport::stated`] does: what the report says about a machine
    /// whose proxy variable does not parse is otherwise reachable only by setting that variable in
    /// the process every other test in the suite runs in.
    pub fn stated_with_unparseable_proxies(
        roots: TrustRoots,
        proxy: Option<&str>,
        no_proxy: Option<&str>,
        unparseable: &[&str],
    ) -> Self {
        let mut transport = Self::stated(roots, proxy, no_proxy);
        transport.unparseable_proxies =
            unparseable.iter().map(|name| (*name).to_string()).collect();
        transport
    }

    /// The transport with a client certificate named, and nothing else stated.
    ///
    /// Exists so what a report says about a certificate that loaded, one that did not and a pair
    /// that disagree can be held to in a test, as [`Transport::stated`] does for the roots.
    pub fn stated_with_client_pair(certificate: Option<&Path>, key: Option<&Path>) -> Self {
        let mut transport = Self::resolve(TrustRoots::Bundled, None);
        transport.use_client_pair(
            ClientPaths {
                certificate: certificate.map(Path::to_path_buf),
                key: key.map(Path::to_path_buf),
            },
            None,
        );
        transport
    }

    /// As [`Transport::stated_with_client_pair`], trusting the roots `roots` names, so that a test
    /// can connect to a server of its own that asks for a client certificate.
    pub fn stated_with_roots_and_client_pair(
        roots: TrustRoots,
        certificate: Option<&Path>,
        key: Option<&Path>,
    ) -> Self {
        let mut transport = Self::resolve(roots, None);
        transport.use_client_pair(
            ClientPaths {
                certificate: certificate.map(Path::to_path_buf),
                key: key.map(Path::to_path_buf),
            },
            None,
        );
        transport
    }

    /// As [`Transport::stated_with_client_pair`], with the passphrase of an encrypted key.
    pub fn stated_with_client_pair_and_passphrase(
        certificate: Option<&Path>,
        key: Option<&Path>,
        passphrase: &str,
    ) -> Self {
        let mut transport = Self::resolve(TrustRoots::Bundled, None);
        transport.use_client_pair(
            ClientPaths {
                certificate: certificate.map(Path::to_path_buf),
                key: key.map(Path::to_path_buf),
            },
            Some(passphrase.to_string()),
        );
        transport
    }

    fn use_client_pair(&mut self, paths: ClientPaths, passphrase: Option<String>) {
        (self.client, self.client_problems) = load_client_pair(&paths, passphrase.as_deref());
        self.client_paths = paths;
        self.client_passphrase = passphrase;
    }

    /// The pair the files held when the environment was read.
    pub fn client_pair(&self) -> ClientPair {
        ClientPair::of(self.client.clone(), self.client_problems.clone())
    }

    /// Read the named files again.
    ///
    /// A reading that cannot be used while `previous` holds a pair keeps `previous`: a certificate
    /// replaced before its key is, or the reverse, is a rotation half written, and the next reading
    /// finds the whole of it. Both files are read together and loaded as one pair, so a new
    /// certificate is never presented with the old key.
    pub fn reread_client_pair(&self, previous: &ClientPair) -> Reread {
        let (cert, problems) =
            load_client_pair(&self.client_paths, self.client_passphrase.as_deref());
        let failed = problems
            .iter()
            .filter_map(|problem| {
                if Some(&problem.path) == self.client_paths.certificate.as_ref() {
                    Some("certificate")
                } else if Some(&problem.path) == self.client_paths.key.as_ref() {
                    Some("key")
                } else {
                    None
                }
            })
            .collect();
        let fresh = ClientPair::of(cert, problems);
        let pair = if !fresh.is_loaded() && previous.is_loaded() {
            previous.clone()
        } else {
            fresh
        };
        Reread { pair, failed }
    }

    fn resolve(roots: TrustRoots, proxy: Option<Proxy>) -> Self {
        let (certificates, trust_problems) = match &roots {
            TrustRoots::Bundled => (Vec::new(), Vec::new()),
            TrustRoots::Named { file, directory } => load(file.as_deref(), directory.as_deref()),
        };
        // A protocol the build cannot connect through is not a route, and holding one here would
        // make it one in the report while every request went direct. Worse for a stated proxy: the
        // transport library treats that as an error worth ending the process over, and turns the
        // first connection into a panic.
        let (proxy, unusable_proxy) = match proxy {
            Some(proxy) if !connectable(&proxy) => {
                let named = proxy.protocol().to_string().to_ascii_lowercase();
                (None, Some(named))
            }
            found => (found, None),
        };
        Self {
            roots,
            certificates,
            trust_problems,
            proxy,
            unusable_proxy,
            unparseable_proxies: Vec::new(),
            no_proxy: None,
            roots_from_file: false,
            proxy_from_file: false,
            client: None,
            client_paths: ClientPaths::default(),
            client_problems: Vec::new(),
            client_passphrase: None,
        }
    }

    /// Whether the roots in force are what a settings file named, the environment naming none, so
    /// a report can say which file to edit.
    pub fn roots_from_file(&self) -> bool {
        self.roots_from_file
    }

    /// Whether the proxy in force is what a settings file named, on the same terms.
    pub fn proxy_from_file(&self) -> bool {
        self.proxy_from_file
    }

    /// Where the trust roots come from.
    pub fn roots(&self) -> &TrustRoots {
        &self.roots
    }

    /// Why each named path that could not be used cannot be, in the order the paths are read. A
    /// path that worked is still in force whatever the others did: see [`load`].
    ///
    /// Every one of them, rather than the first: two variables name the two paths, and a report
    /// that stopped at one would leave whoever set the other looking at a machine that says nothing
    /// about it.
    pub fn trust_problems(&self) -> &[TrustError] {
        &self.trust_problems
    }

    /// Whether every handshake is about to be refused, which is what a named path yielding no
    /// certificate at all leaves behind.
    pub fn trusts_nothing(&self) -> bool {
        !matches!(self.roots, TrustRoots::Bundled) && self.certificates.is_empty()
    }

    /// The files the environment named for a client certificate, loaded or not.
    pub fn client_paths(&self) -> &ClientPaths {
        &self.client_paths
    }

    /// Whether the environment named a client certificate or key at all, usable or not. The
    /// egress gate keeps a pair to read again only then.
    pub fn names_client_certificate(&self) -> bool {
        self.client_paths.certificate.is_some() || self.client_paths.key.is_some()
    }

    /// Whether a certificate and key were named, loaded and agree, so that a server asking for one
    /// is given it.
    pub fn presents_client_certificate(&self) -> bool {
        self.client.is_some()
    }

    /// Whether a passphrase was given for the client key. Whether, never what: the value stays in
    /// this transport and is shown nowhere.
    pub fn client_passphrase_given(&self) -> bool {
        self.client_passphrase.is_some()
    }

    /// Why each named path that could not be used cannot be, in the order the paths are read.
    ///
    /// While there is one, every TLS handshake is refused ([`Transport::agent_config_builder`]).
    pub fn client_problems(&self) -> &[TrustError] {
        &self.client_problems
    }

    /// The proxy in force, named by protocol, host and port alone.
    ///
    /// Never the uri: a proxy uri carries a username and password on the networks that require one,
    /// and a diagnostic that printed one is a diagnostic people paste into issues.
    pub fn proxy_summary(&self) -> Option<String> {
        self.proxy.as_ref().map(|proxy| {
            // Lowercased, because the transport names a protocol in capitals and a uri scheme is
            // written in lower case everywhere else a reader will have seen one.
            let protocol = proxy.protocol().to_string().to_ascii_lowercase();
            format!("{protocol}://{}:{}", proxy.host(), proxy.port())
        })
    }

    /// Whether the proxy requires a credential, which is a fact about the network rather than the
    /// credential itself.
    pub fn proxy_is_authenticated(&self) -> bool {
        self.proxy
            .as_ref()
            .is_some_and(|proxy| proxy.username().is_some())
    }

    /// The protocol of a proxy that was named and that this build cannot connect through, so that
    /// a report does not present a route requests are not taking.
    pub fn unusable_proxy(&self) -> Option<&str> {
        self.unusable_proxy.as_deref()
    }

    /// The proxy variables that are set to something that cannot be read as a proxy address, in the
    /// order the variables are read.
    ///
    /// Names alone, never values: a proxy uri carries a username and password on the networks that
    /// require one, and a value that failed to parse is as likely to hold one as a value that did.
    ///
    /// Reported because the alternative is a report that denies the proxy somebody set. The reader
    /// passes over a value it cannot parse and tries the next variable, so a machine whose only
    /// route off it is a mistyped `HTTPS_PROXY` otherwise sees connection failures beside a line
    /// saying no proxy was named.
    pub fn unparseable_proxies(&self) -> &[String] {
        &self.unparseable_proxies
    }

    /// The hosts `NO_PROXY` excludes from the proxy, verbatim.
    ///
    /// Worth reporting because it decides whether a proxy in force applies to the host that is
    /// failing, and `NO_PROXY=*` leaves one configured and used for nothing.
    pub fn no_proxy(&self) -> Option<&str> {
        self.no_proxy.as_deref()
    }

    /// An agent configuration that trusts what this says and goes where this says, and presents no
    /// client certificate.
    ///
    /// The one place either is set. A caller adds its own timeouts and builds. The certificate is
    /// presented only by [`Transport::agent_config_builder_presenting`], which the egress gate uses
    /// for requests to a model provider (NET-11), so a sign-in client built from this one never
    /// offers it.
    pub fn agent_config_builder(&self) -> ConfigBuilder<AgentScope> {
        self.config_builder(&self.client_problems, None)
    }

    /// As [`Transport::agent_config_builder`], presenting `pair` to a server that asks for a
    /// certificate.
    pub fn agent_config_builder_presenting(&self, pair: &ClientPair) -> ConfigBuilder<AgentScope> {
        self.config_builder(&pair.problems, pair.cert.clone())
    }

    fn config_builder(
        &self,
        client_problems: &[TrustError],
        client: Option<ClientCert>,
    ) -> ConfigBuilder<AgentScope> {
        ureq::Agent::config_builder()
            .tls_config(
                TlsConfig::builder()
                    .root_certs(self.root_certs_given(client_problems))
                    .client_cert(client)
                    .build(),
            )
            .proxy(self.proxy.clone())
    }

    #[cfg(test)]
    fn root_certs(&self) -> RootCerts {
        self.root_certs_given(&self.client_problems)
    }

    fn root_certs_given(&self, client_problems: &[TrustError]) -> RootCerts {
        // A certificate that was named and cannot be presented refuses every handshake, the way a
        // named root that held nothing does, rather than letting a request go to a server that
        // would have been given it without one. Nothing trusted is the one answer every client
        // built from this configuration, the subscription client included, already understands.
        if !client_problems.is_empty() {
            return RootCerts::new_with_certs(&[]);
        }
        match self.roots {
            TrustRoots::Bundled => RootCerts::WebPki,
            // Whatever the named paths held, which is nothing where none of them held anything.
            // The built-in set is not the fallback: somebody who named an authority and silently
            // got the public ones back would be trusting a set they had just replaced, and the
            // connection they were trying to fix would go on failing with nothing to say why.
            // Refusing every handshake is the honest reading of "trust these and nothing else",
            // and `doctor` names the path and the reason.
            TrustRoots::Named { .. } => RootCerts::new_with_certs(&self.certificates),
        }
    }
}

impl fmt::Debug for Transport {
    /// No certificates and no proxy uri: the first is bulk that says nothing a path does not, and
    /// the second carries a credential.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transport")
            .field("roots", &self.roots)
            .field("problems", &self.trust_problems)
            .field("proxy", &self.proxy_summary())
            .field("unusable_proxy", &self.unusable_proxy)
            .field("unparseable_proxies", &self.unparseable_proxies)
            .field("no_proxy", &self.no_proxy)
            .field("client_paths", &self.client_paths)
            .field("client_problems", &self.client_problems)
            .field("client_passphrase_given", &self.client_passphrase.is_some())
            .finish()
    }
}

/// The certificate and key two paths name, or what is wrong with them.
///
/// Nothing is named, nothing is loaded and nothing is wrong. One named without the other is a
/// problem on the one that was named: a certificate with no key cannot answer a server, and a
/// request sent without it is the failure this setting exists to prevent.
fn load_client_pair(
    paths: &ClientPaths,
    passphrase: Option<&str>,
) -> (Option<ClientCert>, Vec<TrustError>) {
    let (certificate, key) = match (&paths.certificate, &paths.key) {
        (None, None) => return (None, Vec::new()),
        (Some(certificate), None) => {
            return (None, vec![missing_half(certificate, CLIENT_KEY)]);
        }
        (None, Some(key)) => return (None, vec![missing_half(key, CLIENT_CERTIFICATE)]),
        (Some(certificate), Some(key)) => (certificate, key),
    };

    let certificates = std::fs::read(certificate)
        .map_err(|e| unusable(certificate, e))
        .and_then(|bytes| match from_pem(&bytes) {
            found if found.is_empty() => Err(holds_nothing(certificate)),
            found => Ok(found),
        });
    let private_key = std::fs::read(key)
        .map_err(|e| unusable(key, e))
        .and_then(|bytes| private_key_in(key, &bytes, passphrase));

    match (certificates, private_key) {
        (Ok(certificates), Ok(private_key)) => {
            let pair = ClientCert::new_with_certs(&certificates, private_key);
            if agrees(&pair) {
                (Some(pair), Vec::new())
            } else {
                (None, vec![mismatched(key, certificate)])
            }
        }
        (certificates, private_key) => (
            None,
            [certificates.err(), private_key.err()]
                .into_iter()
                .flatten()
                .collect(),
        ),
    }
}

/// The key in an `ENCRYPTED PRIVATE KEY` section (PKCS#8 with PBES2), opened with `passphrase`.
///
/// The error never carries the passphrase or any byte of the key: a wrong passphrase and a damaged
/// file read the same, because the cipher cannot tell them apart either.
fn decrypted_key_in(
    path: &Path,
    bytes: &[u8],
    passphrase: Option<&str>,
) -> Result<PrivateKey<'static>, TrustError> {
    use pkcs8::{DecodePrivateKey, SecretDocument};
    let refused = |detail: String| TrustError {
        path: path.to_path_buf(),
        detail,
    };
    let Some(passphrase) = passphrase else {
        return Err(refused(format!(
            "is an encrypted key, and {CLIENT_KEY_PASSPHRASE} names no passphrase for it"
        )));
    };
    let text = std::str::from_utf8(bytes)
        .map_err(|_| refused("is an encrypted key that is not readable text".to_string()))?;
    let document = SecretDocument::from_pkcs8_encrypted_pem(text, passphrase).map_err(|_| {
        refused(format!(
            "is an encrypted key that {CLIENT_KEY_PASSPHRASE} does not open, or that is damaged"
        ))
    })?;
    let pem = document
        .to_pem("PRIVATE KEY", pkcs8::LineEnding::LF)
        .map_err(|_| refused("is an encrypted key that holds no usable key".to_string()))?;
    PrivateKey::from_pem(pem.as_bytes())
        .map_err(|_| refused("is an encrypted key that holds no usable key".to_string()))
}

fn private_key_in(
    path: &Path,
    bytes: &[u8],
    passphrase: Option<&str>,
) -> Result<PrivateKey<'static>, TrustError> {
    // The legacy OpenSSL form keeps a `BEGIN RSA PRIVATE KEY` label and says it is encrypted in a
    // header, so the PEM reader would return its ciphertext as if it were a key. It is looked for
    // first, and is never decrypted: only PKCS#8 is.
    if String::from_utf8_lossy(bytes).contains("Proc-Type: 4,ENCRYPTED") {
        return Err(TrustError {
            path: path.to_path_buf(),
            detail:
                "is a key in OpenSSL's legacy encrypted form, which this build cannot decrypt; \
                     convert it with `openssl pkcs8 -topk8 -nocrypt`"
                    .to_string(),
        });
    }
    let found = parse_pem(bytes)
        .filter_map(Result::ok)
        .find_map(|item| match item {
            PemItem::PrivateKey(key) => Some(key),
            _ => None,
        });
    match found {
        Some(key) => Ok(key),
        // The PEM reader skips a section it has no kind for, so an encrypted key reads as a file
        // holding none, which would send somebody looking for a key that is plainly there.
        None if String::from_utf8_lossy(bytes).contains("ENCRYPTED PRIVATE KEY") => {
            decrypted_key_in(path, bytes, passphrase)
        }
        None => Err(TrustError {
            path: path.to_path_buf(),
            detail: "holds no private key".to_string(),
        }),
    }
}

fn missing_half(named: &Path, other: &str) -> TrustError {
    TrustError {
        path: named.to_path_buf(),
        detail: format!("is named without {other}, and a certificate needs its key"),
    }
}

fn mismatched(key: &Path, certificate: &Path) -> TrustError {
    TrustError {
        path: key.to_path_buf(),
        detail: format!(
            "does not match the certificate in {}, or is a kind of key this build cannot use",
            certificate.display()
        ),
    }
}

/// Whether the TLS library accepts this certificate with this key, and the key is a kind it can
/// sign with.
///
/// Asked of the library's own check, which answers with an error. Building a connection's
/// configuration from a pair that disagree panics instead, and the release profile aborts on a
/// panic, so a pair that was only tried by connecting would end the process rather than be refused.
fn agrees(pair: &ClientCert) -> bool {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    let Ok(key) = PrivateKeyDer::try_from(pair.private_key().der()) else {
        return false;
    };
    let Ok(signing) = rustls::crypto::ring::sign::any_supported_type(&key) else {
        return false;
    };
    let chain = pair
        .certs()
        .iter()
        .map(|certificate| CertificateDer::from(certificate.der().to_vec()))
        .collect::<Vec<_>>();
    if chain.is_empty() {
        return false;
    }
    rustls::sign::CertifiedKey::new(chain, signing)
        .keys_match()
        .is_ok()
}

/// Every certificate the named paths hold, and every one of them that could not be used.
///
/// The two paths are read independently. A machine that names both and has only one of them is
/// common enough that discarding the good one would be the worse failure by far: the cost of one
/// unusable path is a line in the report, and the cost of dropping a usable one is every connection
/// this process makes.
///
/// Within a path the opposite holds: an entry that does not parse is skipped, because a certificate
/// directory in the form OpenSSL reads also holds revocation lists, and one unreadable entry among
/// a hundred is not a reason to stop trusting the other ninety-nine.
///
/// Nothing found anywhere leaves nothing trusted, for the reason [`Transport::root_certs`] gives.
fn load(
    file: Option<&Path>,
    directory: Option<&Path>,
) -> (Vec<Certificate<'static>>, Vec<TrustError>) {
    let mut certificates = Vec::new();
    let mut problems = Vec::new();

    for (path, read) in [
        (
            file,
            from_file as fn(&Path) -> Result<Vec<Certificate<'static>>, TrustError>,
        ),
        (directory, from_directory),
    ] {
        let Some(path) = path else { continue };
        match read(path) {
            Ok(found) if found.is_empty() => problems.push(holds_nothing(path)),
            Ok(found) => certificates.extend(found),
            Err(error) => problems.push(error),
        }
    }

    (certificates, problems)
}

/// Every certificate in one file of them.
fn from_file(path: &Path) -> Result<Vec<Certificate<'static>>, TrustError> {
    Ok(from_pem(
        &std::fs::read(path).map_err(|e| unusable(path, e))?,
    ))
}

/// Every certificate in a directory of them, read in name order so that two runs on the same
/// directory trust the same set in the same order.
fn from_directory(path: &Path) -> Result<Vec<Certificate<'static>>, TrustError> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)
        .map_err(|e| unusable(path, e))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();

    Ok(entries
        .iter()
        .filter_map(|entry| std::fs::read(entry).ok())
        .flat_map(|bytes| from_pem(&bytes))
        .collect())
}

/// The certificates in one PEM document. Anything else it holds is not one and is skipped.
fn from_pem(bytes: &[u8]) -> Vec<Certificate<'static>> {
    parse_pem(bytes)
        .filter_map(Result::ok)
        .filter_map(|item| match item {
            PemItem::Certificate(certificate) => Some(certificate),
            _ => None,
        })
        .collect()
}

/// Every proxy variable that is set, non-empty, and holding something that cannot be read as a
/// proxy address, in the order the variables are read, and in either case.
///
/// A value that is not a uri at all, one naming no authority, and one whose scheme no proxy speaks
/// all fail the same parse, which is why neither this nor the report calls the value "not a uri":
/// `gopher://proxy.corp` is one.
///
/// The names alone are collected. A value that does not parse is as likely to hold a credential as
/// one that does, and a report exists to be pasted into an issue.
///
/// The same parse the transport library's own reader applies, so a variable named here is exactly
/// one that reader passed over. That reader stops at the first variable it can parse and this does
/// not stop at all: a variable set after the one in force is reported too, since whoever set it
/// stated a route and is otherwise told nothing about why it is not the one in use.
///
/// Both spellings are read, and a spelling holding a value another spelling of the same variable
/// already answered with is passed over. Windows matches an environment name without regard to
/// case, so there both spellings answer with the one value somebody set, and naming each would
/// print the variable twice and send the reader to correct a spelling nobody wrote.
fn unparseable_proxy_variables(lookup: &impl Fn(&str) -> Option<OsString>) -> Vec<String> {
    let mut named = Vec::new();
    for variable in PROXY_VARIABLES {
        let mut answered: Vec<String> = Vec::new();
        for name in [variable.to_string(), variable.to_ascii_lowercase()] {
            let Some(value) = lookup(&name)
                .and_then(|value| value.into_string().ok())
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if answered.contains(&value) {
                continue;
            }
            answered.push(value.clone());
            if Proxy::new(&value).is_err() {
                named.push(name);
            }
        }
    }
    named
}

/// The names of [`SETTINGS_NAMES`] a file stated with something in them. A blank value names nothing,
/// as it does in the environment, and a name outside the six is not kept.
fn stated_names(values: impl IntoIterator<Item = (String, String)>) -> BTreeMap<String, String> {
    values
        .into_iter()
        .filter(|(name, value)| SETTINGS_NAMES.contains(&name.as_str()) && !value.trim().is_empty())
        .collect()
}

/// The first of the proxy variables a lookup holds that reads as a proxy, carrying `no_proxy`'s
/// exceptions where there are any.
///
/// The environment's own reader attaches `NO_PROXY` to the proxy it builds, and a proxy made from
/// a string does not, so the exceptions are added here through the library's builder from the
/// parts of the parsed proxy rather than by parsing the uri a second way.
fn proxy_stated(
    lookup: &impl Fn(&str) -> Option<OsString>,
    no_proxy: Option<&str>,
) -> Option<Proxy> {
    PROXY_VARIABLES.iter().find_map(|variable| {
        let value = lookup(variable)?.into_string().ok()?;
        let proxy = Proxy::new(&value).ok()?;
        let Some(exceptions) = no_proxy else {
            return Some(proxy);
        };
        let mut builder = Proxy::builder(proxy.protocol())
            .host(proxy.host())
            .port(proxy.port());
        if let Some(username) = proxy.username() {
            builder = builder.username(username);
        }
        if let Some(password) = proxy.password() {
            builder = builder.password(password);
        }
        for entry in exceptions.split(',').map(str::trim) {
            builder = builder.no_proxy(entry);
        }
        Some(builder.build().unwrap_or(proxy))
    })
}

/// Whether this build can actually connect through a proxy of this protocol.
///
/// SOCKS takes a feature this build does not compile, and the transport library's answer to one it
/// cannot use is to warn and connect directly, or, for a proxy set rather than read from the
/// environment, to panic at the first connection. Neither is a thing to discover at the first
/// request, so a proxy that is not one of these is not carried at all.
fn connectable(proxy: &Proxy) -> bool {
    matches!(proxy.protocol(), ProxyProtocol::Http | ProxyProtocol::Https)
}

fn unusable(path: &Path, error: std::io::Error) -> TrustError {
    TrustError {
        path: path.to_path_buf(),
        detail: error.to_string(),
    }
}

fn holds_nothing(path: &Path) -> TrustError {
    TrustError {
        path: path.to_path_buf(),
        detail: "holds no certificate".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A scratch directory under the workspace `target/`, which is per-checkout and already
    /// ignored. The shared system temporary directory is what the security scan flags, and a fixed
    /// name under it collides whenever two checkouts run the suite at once.
    fn scratch(name: &str) -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.push("target");
        path.push("test-scratch");
        path.push(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch directory");
        path
    }

    fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), OsString::from(*value)))
            .collect();
        move |name: &str| map.get(name).cloned()
    }

    /// A PEM certificate to trust. Its contents never reach a handshake here, so what matters is
    /// only that it is one and that the reader finds it.
    const CERTIFICATE: &str = "\
        -----BEGIN CERTIFICATE-----\n\
        MIIDLTCCAhWgAwIBAgIUI3QhU6VvEuw22kr99y0SacWIBwcwDQYJKoZIhvcNAQEL\n\
        BQAwJTEjMCEGA1UEAwwaRXhhbXBsZSBDb3JwIEluc3BlY3Rpb24gQ0EwIBcNMjYw\n\
        OTE2MjAzNTE3WhgPMjEyNjA4MjMyMDM1MTdaMCUxIzAhBgNVBAMMGkV4YW1wbGUg\n\
        Q29ycCBJbnNwZWN0aW9uIENBMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKC\n\
        AQEA1+qE/2NfFRdBWoZDIfQtoSGKkWo5HssPJAtFR6oBuZCJifdLxFa5/0S9sWyf\n\
        faq/GN2wbMJHJ4IrX8QB74PaF1lfG8VNnqWHcxUnyD2lMc9l3Vw8KhEpX4gJM0mW\n\
        V+E/scRTEjnJhlrTnYyeaY1pQ6mZDmy9/WuvmXS8X5Is8zr3KIc+BGsiQhXKg4QU\n\
        QnT4GARDGq2nctO4HZERT5yCsP09kR07xnK1MIaW3qmvkl/9X8oqonVr8ygpVRLZ\n\
        4hUMzu18Bmmag6JkvpmWMbHrr2Rx9g+65GM7a4Lx4lhStvOUS7lHGjYlEEp/60Ot\n\
        7BLVO3ZUA4LCHJ5iRSM0FtN5uwIDAQABo1MwUTAdBgNVHQ4EFgQUeweV1wC95Wlu\n\
        APG8P13pNKyCeNowHwYDVR0jBBgwFoAUeweV1wC95WluAPG8P13pNKyCeNowDwYD\n\
        VR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEABDWsa9BbhSSpP67fqAOt\n\
        kOadMxbNXBsC/ob4JFOzyf4pKWft4OPJWPB9J4Qe3xBQsmO8/TSMtAXLjFTNZbFi\n\
        +zaogurqvWL6vO/c7f6q+AXEMK3l2iHOL26nhOwUdz+vmtj93hjtXWPmc6E/yaz1\n\
        9KUv6Cd0dt33xAwIZ8pMBvQ7/9jhstyf7C3lMRKgwezYwXgcxVG60t5LqfEvqj2t\n\
        6OpcWZ9UnY7Nx+DmAEWGYrnOVoT30hhV5HxsiX6p+/wrWVR7o/oD6yG6V/sa+pXA\n\
        t/CQHQf+4EaQ+kjcow2Pm6NwD/HodxtfMTbRaxEDbxOilrEuUcjH9YzhjiVNEUtx\n\
        RQ==\n\
        -----END CERTIFICATE-----\n\
        ";

    /// Nothing named means nothing changed. A program that consulted the machine's own store the
    /// moment nobody said otherwise would be trusting a different set than the build it shipped as.
    #[test]
    fn an_environment_that_names_no_certificates_leaves_the_built_in_roots_in_force() {
        let roots = TrustRoots::from_env(environment(&[]));

        assert_eq!(roots, TrustRoots::Bundled);
        assert!(roots.paths().is_empty());
    }

    /// The variable every other client on the machine reads is the one that has to work here, or
    /// the authority somebody already installed stays invisible to this program alone.
    #[test]
    fn a_certificate_file_the_environment_names_is_what_is_trusted() {
        let roots = TrustRoots::from_env(environment(&[(CERTIFICATE_FILE, "/etc/corp/ca.pem")]));

        assert_eq!(
            roots,
            TrustRoots::Named {
                file: Some(PathBuf::from("/etc/corp/ca.pem")),
                directory: None,
            }
        );
    }

    /// Both variables, and both read, because a machine that states a directory rather than a
    /// bundle has stated the same thing and would otherwise go unanswered.
    #[test]
    fn a_certificate_directory_is_read_alongside_a_file() {
        let roots = TrustRoots::from_env(environment(&[
            (CERTIFICATE_FILE, "/etc/corp/ca.pem"),
            (CERTIFICATE_DIRECTORY, "/etc/corp/certs"),
        ]));

        assert_eq!(
            roots.paths(),
            vec![Path::new("/etc/corp/ca.pem"), Path::new("/etc/corp/certs")]
        );
    }

    /// A shell that exports a variable unconditionally leaves it empty rather than unset. Read as a
    /// path it would name no certificate, and a set of no certificates refuses every connection the
    /// machine makes, for a variable nobody meant to set.
    #[test]
    fn a_variable_that_is_set_but_empty_names_nothing() {
        let roots = TrustRoots::from_env(environment(&[
            (CERTIFICATE_FILE, ""),
            (CERTIFICATE_DIRECTORY, ""),
        ]));

        assert_eq!(roots, TrustRoots::Bundled);
    }

    /// The certificates in the named file are the ones a handshake is put to, rather than the
    /// built-in set alongside them: somebody pinning a private authority has replaced the public
    /// ones on purpose.
    #[test]
    fn the_named_certificates_replace_the_built_in_roots() {
        let dir = scratch("net-trust-roots-replace");
        let file = dir.join("ca.pem");
        std::fs::write(&file, CERTIFICATE).expect("write");

        let transport = Transport::stated(
            TrustRoots::Named {
                file: Some(file),
                directory: None,
            },
            None,
            None,
        );

        assert!(transport.trust_problems().is_empty());
        assert!(
            matches!(transport.root_certs(), RootCerts::Specific(certs) if certs.len() == 1),
            "the named file is the whole of what is trusted"
        );
    }

    /// A path naming nothing usable refuses every handshake rather than quietly returning to the
    /// built-in roots. Returning to them would have somebody trusting the set they had just
    /// replaced, and would leave the connection they were fixing failing with nothing to say why.
    #[test]
    fn a_certificate_file_that_cannot_be_read_trusts_nothing_and_says_which_path() {
        let dir = scratch("net-trust-roots-unreadable");
        let missing = dir.join("absent.pem");

        let transport = Transport::stated(
            TrustRoots::Named {
                file: Some(missing.clone()),
                directory: None,
            },
            None,
            None,
        );

        let [problem] = transport.trust_problems() else {
            panic!(
                "the one named path is reported: {:?}",
                transport.trust_problems()
            )
        };
        assert_eq!(problem.path, missing);
        assert!(
            matches!(transport.root_certs(), RootCerts::Specific(certs) if certs.is_empty()),
            "nothing is trusted, rather than the built-in set"
        );
    }

    /// A file that exists and holds no certificate is the same refusal: a bundle somebody truncated
    /// or pointed at the wrong path trusts nothing, and says so, rather than silently widening.
    #[test]
    fn a_certificate_file_holding_no_certificate_is_refused() {
        let dir = scratch("net-trust-roots-empty");
        let file = dir.join("ca.pem");
        std::fs::write(&file, "not a certificate\n").expect("write");

        let transport = Transport::stated(
            TrustRoots::Named {
                file: Some(file.clone()),
                directory: None,
            },
            None,
            None,
        );

        let [problem] = transport.trust_problems() else {
            panic!(
                "the one named path is reported: {:?}",
                transport.trust_problems()
            )
        };
        assert_eq!(problem.path, file);
        assert!(problem.detail.contains("no certificate"));
    }

    /// A certificate directory in the form OpenSSL reads also holds revocation lists and whatever
    /// else the machine keeps there. One entry that is not a certificate is not a reason to stop
    /// trusting the rest of the directory.
    #[test]
    fn a_directory_entry_that_is_not_a_certificate_is_skipped() {
        let dir = scratch("net-trust-roots-directory");
        std::fs::write(dir.join("corp.0"), CERTIFICATE).expect("write");
        std::fs::write(dir.join("corp.r0"), "-----BEGIN X509 CRL-----\nnope\n").expect("write");

        let transport = Transport::stated(
            TrustRoots::Named {
                file: None,
                directory: Some(dir),
            },
            None,
            None,
        );

        assert!(transport.trust_problems().is_empty());
        assert!(
            matches!(transport.root_certs(), RootCerts::Specific(certs) if certs.len() == 1),
            "the certificate is trusted and the revocation list is not one"
        );
    }

    /// A directory holding no certificate at all is the refusal a named path always is. Skipping
    /// what is not a certificate must not become skipping the fact that there were none.
    #[test]
    fn a_certificate_directory_holding_none_is_refused() {
        let dir = scratch("net-trust-roots-directory-empty");
        std::fs::write(dir.join("corp.r0"), "-----BEGIN X509 CRL-----\nnope\n").expect("write");

        let transport = Transport::stated(
            TrustRoots::Named {
                file: None,
                directory: Some(dir.clone()),
            },
            None,
            None,
        );

        let [problem] = transport.trust_problems() else {
            panic!(
                "the one named path is reported: {:?}",
                transport.trust_problems()
            )
        };
        assert_eq!(problem.path, dir);
    }

    /// A proxy uri carries a username and password on the networks that require one. Reporting the
    /// uri would put a live password in every diagnostic somebody pastes into an issue.
    #[test]
    fn a_proxy_is_named_without_the_credential_it_carries() {
        let transport = Transport::stated(
            TrustRoots::Bundled,
            Some("http://alice:s3cret@proxy.corp.example:8080"),
            None,
        );

        assert_eq!(
            transport.proxy_summary().as_deref(),
            Some("http://proxy.corp.example:8080")
        );
        assert!(transport.proxy_is_authenticated());
        assert!(!format!("{transport:?}").contains("s3cret"));
    }

    /// A proxy that needs no credential is reported as needing none, since a proxy rejecting an
    /// unauthenticated request is one of the failures the report exists to explain.
    #[test]
    fn a_proxy_without_a_credential_is_not_reported_as_having_one() {
        let transport =
            Transport::stated(TrustRoots::Bundled, Some("http://proxy.corp:3128"), None);

        assert_eq!(
            transport.proxy_summary().as_deref(),
            Some("http://proxy.corp:3128")
        );
        assert!(!transport.proxy_is_authenticated());
    }

    /// The proxy a client uses is the one this states, not one the transport library's defaults
    /// read for themselves. Inherited, a release that changed that default would silently start or
    /// stop routing every request in this process through somebody else's machine.
    #[test]
    fn the_proxy_a_client_gets_is_the_one_stated_rather_than_a_library_default() {
        let stated = Transport::stated(TrustRoots::Bundled, Some("http://proxy.corp:3128"), None)
            .agent_config_builder()
            .build();
        assert_eq!(
            stated.proxy().map(|proxy| proxy.port()),
            Some(3128),
            "what was stated is what the client is configured with"
        );

        let none = Transport::stated(TrustRoots::Bundled, None, None)
            .agent_config_builder()
            .build();
        assert!(
            none.proxy().is_none(),
            "no proxy means none, whatever the library's own default read"
        );
    }

    /// A machine naming both a bundle and a directory commonly has only one of them, and the
    /// certificates it does have are what stand between it and refusing every connection. The cost
    /// of an unusable path is a line in the report; the cost of discarding a usable one is the whole
    /// of this process's network.
    #[test]
    fn a_path_that_yields_nothing_does_not_discard_one_that_does() {
        let dir = scratch("net-trust-roots-one-of-two");
        let file = dir.join("ca.pem");
        std::fs::write(&file, CERTIFICATE).expect("write");
        let absent = dir.join("no-such-directory");

        let transport = Transport::stated(
            TrustRoots::Named {
                file: Some(file),
                directory: Some(absent.clone()),
            },
            None,
            None,
        );

        assert!(!transport.trusts_nothing());
        assert!(
            matches!(transport.root_certs(), RootCerts::Specific(certs) if certs.len() == 1),
            "the readable path is still in force"
        );
        let [problem] = transport.trust_problems() else {
            panic!(
                "only the path that failed is reported: {:?}",
                transport.trust_problems()
            )
        };
        assert_eq!(
            problem.path, absent,
            "and the one that failed is still named"
        );
    }

    /// Two variables name the two paths, and a machine that has neither has set both wrongly. A
    /// report that named one of them would have whoever set the other fixing a path the program had
    /// nothing to say about, and every connection would go on failing.
    #[test]
    fn every_named_path_that_yields_nothing_is_reported_rather_than_the_first() {
        let dir = scratch("net-trust-roots-neither-of-two");
        let absent_file = dir.join("no-such-bundle.pem");
        let absent_directory = dir.join("no-such-directory");

        let transport = Transport::stated(
            TrustRoots::Named {
                file: Some(absent_file.clone()),
                directory: Some(absent_directory.clone()),
            },
            None,
            None,
        );

        assert!(transport.trusts_nothing());
        let paths: Vec<&Path> = transport
            .trust_problems()
            .iter()
            .map(|problem| problem.path.as_path())
            .collect();
        assert_eq!(
            paths,
            [absent_file.as_path(), absent_directory.as_path()],
            "both paths are named, in the order they are read"
        );
    }

    /// A protocol the build cannot connect through is not a route. Carried, every request would go
    /// direct while the report named a proxy, and a proxy stated rather than read from the
    /// environment would end the process at the first connection instead.
    #[test]
    fn a_proxy_protocol_this_build_cannot_connect_through_is_not_the_route() {
        let transport =
            Transport::stated(TrustRoots::Bundled, Some("socks5://proxy.corp:1080"), None);

        assert_eq!(transport.unusable_proxy(), Some("socks5"));
        assert!(transport.proxy_summary().is_none());
        assert!(
            transport.agent_config_builder().build().proxy().is_none(),
            "no client is handed a proxy it cannot connect through"
        );
    }

    /// A variable set to something that is not a uri is passed over by the reader, which then tries
    /// the next variable. Without naming it, the report says no proxy was named on a machine where
    /// somebody named one, and whoever set it has nothing to fix.
    ///
    /// A bare host is not such a value: the reader inserts a default scheme, so `proxy.corp:8080` is
    /// a route. What fails is a value with a space in it, an authority that is not there, or a
    /// scheme naming a protocol no proxy speaks.
    #[test]
    fn a_proxy_variable_that_cannot_be_parsed_is_named() {
        let named =
            unparseable_proxy_variables(&environment(&[("HTTPS_PROXY", "ht tp://proxy.corp")]));

        assert_eq!(named, ["HTTPS_PROXY"]);
    }

    /// Each spelling that actually fails, so a reader of this test knows what the report is about
    /// and a change to the parse that quietly accepted one of them shows up here.
    #[test]
    fn the_values_a_proxy_variable_cannot_hold_are_each_named() {
        for value in ["ht tp://proxy.corp", "http://", "gopher://proxy.corp:70"] {
            let named = unparseable_proxy_variables(&environment(&[("HTTPS_PROXY", value)]));

            assert_eq!(named, ["HTTPS_PROXY"], "{value} is not a proxy uri");
        }
    }

    /// A bare host and a bare host with a port are routes rather than mistakes: the reader inserts
    /// the default scheme. Naming either would send somebody to fix a variable that works.
    #[test]
    fn a_proxy_variable_naming_a_bare_host_is_not_a_problem_to_report() {
        let named = unparseable_proxy_variables(&environment(&[
            ("ALL_PROXY", "proxy.corp"),
            ("HTTPS_PROXY", "proxy.corp:8080"),
        ]));

        assert!(named.is_empty(), "{named:?}");
    }

    /// The value never appears. A proxy uri carries a username and password on the networks that
    /// require one, and a value that failed to parse is as likely to hold one as a value that did.
    #[test]
    fn a_proxy_variable_that_cannot_be_parsed_is_named_without_its_value() {
        let transport = Transport::stated_with_unparseable_proxies(
            TrustRoots::Bundled,
            None,
            None,
            &["HTTPS_PROXY"],
        );

        assert_eq!(transport.unparseable_proxies(), ["HTTPS_PROXY"]);
        let shown = format!("{transport:?}");
        assert!(shown.contains("HTTPS_PROXY"), "{shown}");
        assert!(
            !shown.contains("s3cret") && !shown.contains("alice"),
            "no credential reaches a report: {shown}"
        );
    }

    /// A value that parses is not a problem to report, and a variable that is set but empty names
    /// nothing at all: a shell that exports one unconditionally leaves it empty rather than unset.
    #[test]
    fn a_proxy_variable_that_parses_or_is_empty_is_not_named() {
        let named = unparseable_proxy_variables(&environment(&[
            ("ALL_PROXY", "http://proxy.corp:3128"),
            ("HTTPS_PROXY", ""),
        ]));

        assert!(named.is_empty(), "{named:?}");
    }

    /// Every one of them, rather than the first. The reader stops at the first variable it can
    /// parse, so a later variable that does not parse is still a route somebody stated, and a report
    /// that stopped would leave them fixing nothing.
    #[test]
    fn every_proxy_variable_that_cannot_be_parsed_is_named_in_the_order_they_are_read() {
        let named = unparseable_proxy_variables(&environment(&[
            ("HTTP_PROXY", "also not a uri"),
            ("ALL_PROXY", "not a uri"),
        ]));

        assert_eq!(named, ["ALL_PROXY", "HTTP_PROXY"]);
    }

    /// The lower-case spelling is the form some networks hand out, and the reader honours it, so a
    /// report that read only the upper-case one would deny a proxy somebody set.
    #[test]
    fn a_lower_case_proxy_variable_that_cannot_be_parsed_is_named() {
        let named =
            unparseable_proxy_variables(&environment(&[("https_proxy", "ht tp://proxy.corp")]));

        assert_eq!(named, ["https_proxy"]);
    }

    /// Windows matches an environment name without regard to case, so both spellings answer with the
    /// one value somebody set. NET-8 says a variable is named one line, and a reader told to correct
    /// `https_proxy` on a machine where only `HTTPS_PROXY` exists is sent after a variable that is
    /// not there.
    ///
    /// The lookup folds case rather than the test running on Windows, because the suite runs on
    /// Linux and macOS too and the behaviour under test is the lookup's, not the platform's.
    #[test]
    fn a_proxy_variable_a_case_folding_lookup_answers_twice_is_named_once() {
        let folding = |name: &str| match name.eq_ignore_ascii_case("HTTPS_PROXY") {
            true => Some(OsString::from("ht tp://proxy.corp")),
            false => None,
        };

        let named = unparseable_proxy_variables(&folding);

        assert_eq!(named, ["HTTPS_PROXY"]);
    }

    /// Two spellings of one variable holding two different values are two variables, which is only
    /// reachable where the lookup does not fold case. Both are routes somebody stated and neither is
    /// honoured, so the dedup above must key on the value and not on the variable.
    #[test]
    fn two_spellings_of_a_proxy_variable_holding_different_values_are_both_named() {
        let named = unparseable_proxy_variables(&environment(&[
            ("HTTPS_PROXY", "ht tp://proxy.corp"),
            ("https_proxy", "gopher://proxy.corp"),
        ]));

        assert_eq!(named, ["HTTPS_PROXY", "https_proxy"]);
    }

    /// The built-in roots are what a client gets when nothing named others, so a build that ships
    /// them keeps working on a machine that has said nothing about certificates.
    #[test]
    fn a_client_gets_the_built_in_roots_when_nothing_names_others() {
        let config = Transport::stated(TrustRoots::Bundled, None, None)
            .agent_config_builder()
            .build();

        assert!(matches!(
            config.tls_config().root_certs(),
            RootCerts::WebPki
        ));
    }

    /// The environment and a settings file as the process resolves them, with the exported proxy
    /// reader stood in for by `exported`, which is what the library's own would have built from the
    /// environment lookup.
    fn resolved(
        exported: &[(&str, &str)],
        stated: &[(&str, &str)],
        exported_proxy: Option<&str>,
    ) -> Transport {
        Transport::from_sources(&environment(exported), &environment(stated), || {
            exported_proxy.and_then(|uri| Proxy::new(uri).ok())
        })
    }

    fn proxy_port_of(transport: &Transport) -> Option<u16> {
        transport
            .agent_config_builder()
            .build()
            .proxy()
            .map(Proxy::port)
    }

    fn bypasses(transport: &Transport, host: &str) -> bool {
        let uri: ureq::http::Uri = format!("https://{host}/").parse().expect("a uri");
        transport
            .agent_config_builder()
            .build()
            .proxy()
            .is_some_and(|proxy| proxy.is_no_proxy(&uri))
    }

    /// NET-8: a launcher that hands the process no shell profile is the case this exists for, so a
    /// proxy a settings file states is the route a client is built with.
    #[test]
    fn a_proxy_a_settings_file_states_is_the_one_a_client_carries() {
        let transport = resolved(&[], &[("HTTPS_PROXY", "http://proxy.corp:3128")], None);

        assert_eq!(proxy_port_of(&transport), Some(3128));
        assert_eq!(
            transport.proxy_summary().as_deref(),
            Some("http://proxy.corp:3128")
        );
        assert!(transport.proxy_from_file());
    }

    /// The environment wins as a whole: a file's `ALL_PROXY` is read before an exported
    /// `HTTPS_PROXY` by the order the variables are read in, and has to lose to it all the same.
    #[test]
    fn an_exported_proxy_beats_the_file_whatever_names_each_used() {
        let transport = resolved(
            &[("HTTPS_PROXY", "http://exported.corp:3129")],
            &[("ALL_PROXY", "http://stated.corp:3128")],
            Some("http://exported.corp:3129"),
        );

        assert_eq!(proxy_port_of(&transport), Some(3129));
        assert!(!transport.proxy_from_file());
    }

    /// An exported variable that names no usable route is still the environment speaking, and is
    /// reported as such: falling through to the file would make the report deny what was exported.
    #[test]
    fn an_exported_proxy_that_does_not_parse_is_not_replaced_by_the_file() {
        let transport = resolved(
            &[("HTTPS_PROXY", "ht tp://proxy.corp")],
            &[("ALL_PROXY", "http://stated.corp:3128")],
            None,
        );

        assert_eq!(proxy_port_of(&transport), None);
        assert_eq!(transport.unparseable_proxies(), ["HTTPS_PROXY"]);
        assert!(!transport.proxy_from_file());
    }

    /// A file's value that is not a proxy address is named like the environment's, and is not a
    /// route.
    #[test]
    fn a_proxy_in_the_file_that_does_not_parse_is_named_and_is_not_the_route() {
        let transport = resolved(&[], &[("HTTPS_PROXY", "ht tp://proxy.corp")], None);

        assert_eq!(proxy_port_of(&transport), None);
        assert_eq!(transport.unparseable_proxies(), ["HTTPS_PROXY"]);
    }

    /// The exceptions go with a route the file stated, which the library attaches only to a proxy
    /// it read itself, and the credential in the uri survives the rebuilding that attaches them.
    #[test]
    fn a_file_proxy_honours_the_no_proxy_it_is_given_and_keeps_its_credential() {
        let transport = resolved(
            &[],
            &[
                ("HTTPS_PROXY", "http://alice:s3cret@proxy.corp:3128"),
                ("NO_PROXY", "internal.corp, .lan,"),
            ],
            None,
        );

        assert!(bypasses(&transport, "internal.corp"));
        assert!(bypasses(&transport, "build.lan"));
        assert!(!bypasses(&transport, "api.example"));
        assert_eq!(transport.no_proxy(), Some("internal.corp, .lan,"));
        assert!(transport.proxy_is_authenticated());
        assert_eq!(
            transport.proxy_summary().as_deref(),
            Some("http://proxy.corp:3128")
        );
    }

    /// `NO_PROXY` is the exception to the file's whole-answer rule: an exported one applies to a
    /// route the file stated and the file's own is not consulted beside it.
    #[test]
    fn an_exported_no_proxy_applies_to_a_file_proxy_in_place_of_the_files() {
        let transport = resolved(
            &[("NO_PROXY", "exported.corp")],
            &[
                ("HTTPS_PROXY", "http://proxy.corp:3128"),
                ("NO_PROXY", "stated.corp"),
            ],
            None,
        );

        assert!(bypasses(&transport, "exported.corp"));
        assert!(!bypasses(&transport, "stated.corp"));
        assert_eq!(transport.no_proxy(), Some("exported.corp"));
    }

    /// NET-7: a certificate bundle a settings file names is what is trusted where the environment
    /// names none.
    #[test]
    fn a_certificate_file_a_settings_file_names_is_what_is_trusted() {
        let transport = resolved(&[], &[("SSL_CERT_FILE", "/etc/corp/ca.pem")], None);

        assert_eq!(
            transport.roots(),
            &TrustRoots::Named {
                file: Some(PathBuf::from("/etc/corp/ca.pem")),
                directory: None,
            }
        );
        assert!(transport.roots_from_file());
    }

    /// The pair is taken whole from the environment, as the proxy is: an exported directory beside
    /// a file's bundle is the exported statement, and the bundle is not added to it.
    #[test]
    fn exported_roots_replace_the_files_rather_than_joining_them() {
        let transport = resolved(
            &[("SSL_CERT_DIR", "/etc/exported")],
            &[("SSL_CERT_FILE", "/etc/corp/ca.pem")],
            None,
        );

        assert_eq!(
            transport.roots(),
            &TrustRoots::Named {
                file: None,
                directory: Some(PathBuf::from("/etc/exported")),
            }
        );
        assert!(!transport.roots_from_file());
    }

    /// Nothing stated anywhere changes nothing: the built-in roots, no route, no file named.
    #[test]
    fn nothing_stated_in_either_place_leaves_the_defaults() {
        let transport = resolved(&[], &[], None);

        assert_eq!(transport.roots(), &TrustRoots::Bundled);
        assert_eq!(proxy_port_of(&transport), None);
        assert!(!transport.roots_from_file() && !transport.proxy_from_file());
    }

    /// Only the six names are read from a file, and a blank value names nothing, as in the
    /// environment.
    #[test]
    fn a_settings_file_hands_over_the_six_names_and_nothing_else() {
        let kept = stated_names([
            (
                "HTTPS_PROXY".to_string(),
                "http://proxy.corp:3128".to_string(),
            ),
            ("ALL_PROXY".to_string(), "   ".to_string()),
            ("https_proxy".to_string(), "http://lower.corp:1".to_string()),
            ("AWS_REGION".to_string(), "eu-west-1".to_string()),
        ]);

        assert_eq!(
            kept.keys().map(String::as_str).collect::<Vec<_>>(),
            ["HTTPS_PROXY"]
        );
    }

    const CLIENT_A_KEY: [&str; 3] = [
        "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgNI5exiW0VUAX6Zlh",
        "2xpQpP9Ul/tkSYnXIk0Ew/P6G1qhRANCAAQvxnDQ3AYHDeZGt/rINkorQdR29nsI",
        "WLbY1tX9vPGoIPii3RoRH2ndnu4qv35pTOL5ZPoT6W8ZF4tEue2KzaW7",
    ];
    const CLIENT_B_KEY: [&str; 3] = [
        "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgelZvU4tWD/NVOwfb",
        "DUI00URNX/zgu9ybKgXU2Pkdqj+hRANCAASN8kQiMv+SDHgFjaKHKIt5KF7D8gLJ",
        "SYjcDrZ4lZT8NTj0TPvc+G/EN5tEtwRz/9/RGamARW++a28XqN8cntww",
    ];
    const CLIENT_A_CERTIFICATE: [&str; 10] = [
        "MIIBmTCCAT+gAwIBAgIUGoEm4tLRujtRq+Z9SYoo7mpaVTgwCgYIKoZIzj0EAwIw",
        "ITEfMB0GA1UEAwwWYnJhdmVib3QtdGVzdC1jbGllbnQtYTAgFw0yNjEwMjIxNjQ5",
        "MjJaGA8yMTI2MDkyODE2NDkyMlowITEfMB0GA1UEAwwWYnJhdmVib3QtdGVzdC1j",
        "bGllbnQtYTBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABC/GcNDcBgcN5ka3+sg2",
        "SitB1Hb2ewhYttjW1f288agg+KLdGhEfad2e7iq/fmlM4vlk+hPpbxkXi0S57YrN",
        "pbujUzBRMB0GA1UdDgQWBBSiEIAHwsbVA0ZZsQ1BDSeQ1FBAfjAfBgNVHSMEGDAW",
        "gBSiEIAHwsbVA0ZZsQ1BDSeQ1FBAfjAPBgNVHRMBAf8EBTADAQH/MAoGCCqGSM49",
        "BAMCA0gAMEUCIQDV/pEMcyED9IfcGuf4pke2It4Kj4yubw6uM0lrqz5ymgIgRKEK",
        "slJ1+wh5R7cmgoS2bui3Umrk+r+pU9FokD8FYZM=",
        "",
    ];

    /// PEM armour assembled rather than written, so no literal in this file is a private key to a
    /// scanner. The bodies are throwaway keys made for these tests and used for nothing else.
    fn armoured(kind: &str, body: &[&str]) -> String {
        let mut pem = format!("-----BEGIN {kind}-----\n");
        for line in body.iter().filter(|line| !line.is_empty()) {
            pem.push_str(line);
            pem.push('\n');
        }
        pem.push_str(&format!("-----END {kind}-----\n"));
        pem
    }

    fn client_certificate_a() -> String {
        armoured("CERTIFICATE", &CLIENT_A_CERTIFICATE)
    }

    fn client_key(body: &[&str]) -> String {
        armoured(&format!("{} KEY", "PRIVATE"), body)
    }

    /// The transport the environment makes of two files, written into a scratch directory.
    fn with_client_files(
        name: &str,
        certificate: Option<&str>,
        key: Option<&str>,
    ) -> (Transport, PathBuf, PathBuf) {
        let directory = scratch(name);
        let certificate_path = directory.join("client.crt");
        let key_path = directory.join("client.key");
        let mut pairs = Vec::new();
        if let Some(certificate) = certificate {
            std::fs::write(&certificate_path, certificate).expect("certificate");
            pairs.push((CLIENT_CERTIFICATE, certificate_path.to_str().unwrap()));
        }
        if let Some(key) = key {
            std::fs::write(&key_path, key).expect("key");
            pairs.push((CLIENT_KEY, key_path.to_str().unwrap()));
        }
        let transport = Transport::from_sources(&environment(&pairs), &|_| None, || None);
        (transport, certificate_path, key_path)
    }

    /// Whether a client built from the transport refuses every handshake, which is what trusting
    /// nothing is.
    fn refuses_every_handshake(transport: &Transport) -> bool {
        let config = transport.agent_config_builder().build();
        matches!(config.tls_config().root_certs(), RootCerts::Specific(roots) if roots.is_empty())
    }

    /// NET-11: a certificate and its key, both named, are what a client presents.
    #[test]
    fn a_certificate_and_key_that_agree_are_what_a_client_presents() {
        let (transport, ..) = with_client_files(
            "client-pair-agrees",
            Some(&client_certificate_a()),
            Some(&client_key(&CLIENT_A_KEY)),
        );

        assert!(transport.client_problems().is_empty());
        assert!(transport.presents_client_certificate());
        let config = transport
            .agent_config_builder_presenting(&transport.client_pair())
            .build();
        assert!(config.tls_config().client_cert().is_some());
        // The builder every other client is made from holds none (NET-11).
        let plain = transport.agent_config_builder().build();
        assert!(plain.tls_config().client_cert().is_none());
        assert!(!refuses_every_handshake(&transport));
    }

    /// NET-11: naming nothing presents nothing, and an empty variable names nothing, as it does
    /// for the roots.
    #[test]
    fn naming_no_client_certificate_presents_none() {
        let transport = Transport::from_sources(
            &environment(&[(CLIENT_CERTIFICATE, ""), (CLIENT_KEY, "")]),
            &|_| None,
            || None,
        );

        assert!(!transport.presents_client_certificate());
        assert!(transport.client_problems().is_empty());
        assert_eq!(transport.client_paths(), &ClientPaths::default());
        let config = transport.agent_config_builder().build();
        assert!(config.tls_config().client_cert().is_none());
    }

    /// NET-11: a key that belongs to a different certificate is refused by the key's path, and every
    /// handshake with it. The library would otherwise panic at the first request.
    #[test]
    fn a_key_that_does_not_belong_to_the_certificate_is_refused_with_its_path() {
        let (transport, certificate, key) = with_client_files(
            "client-pair-mismatch",
            Some(&client_certificate_a()),
            Some(&client_key(&CLIENT_B_KEY)),
        );

        assert!(!transport.presents_client_certificate());
        let [problem] = transport.client_problems() else {
            panic!("one problem expected: {:?}", transport.client_problems());
        };
        assert_eq!(problem.path, key);
        assert!(
            problem.detail.contains(&certificate.display().to_string()),
            "{problem}"
        );
        assert!(refuses_every_handshake(&transport));
        let config = transport.agent_config_builder().build();
        assert!(config.tls_config().client_cert().is_none());
    }

    /// NET-11: a path that cannot be read is named with the reason, and the request is refused
    /// rather than sent without the certificate.
    #[test]
    fn a_certificate_file_that_cannot_be_read_is_refused_with_its_path() {
        let directory = scratch("client-certificate-missing");
        let key = directory.join("client.key");
        std::fs::write(&key, client_key(&CLIENT_A_KEY)).expect("key");
        let certificate = directory.join("absent.crt");
        let transport = Transport::from_sources(
            &environment(&[
                (CLIENT_CERTIFICATE, certificate.to_str().unwrap()),
                (CLIENT_KEY, key.to_str().unwrap()),
            ]),
            &|_| None,
            || None,
        );

        let [problem] = transport.client_problems() else {
            panic!("one problem expected: {:?}", transport.client_problems());
        };
        assert_eq!(problem.path, certificate);
        assert!(refuses_every_handshake(&transport));
    }

    /// NET-11: both files are read, so two bad paths are two problems and not the first of them.
    #[test]
    fn every_client_file_that_cannot_be_used_is_reported() {
        let (transport, certificate, key) = with_client_files(
            "client-both-bad",
            Some("not a certificate\n"),
            Some("not a key\n"),
        );

        let paths: Vec<&Path> = transport
            .client_problems()
            .iter()
            .map(|problem| problem.path.as_path())
            .collect();
        assert_eq!(paths, [certificate.as_path(), key.as_path()]);
        assert!(refuses_every_handshake(&transport));
    }

    /// NET-11: a certificate without its key cannot answer a server, and the request that would have
    /// gone without one is refused instead.
    #[test]
    fn a_certificate_named_without_a_key_is_refused_and_so_is_a_key_without_a_certificate() {
        let (transport, certificate, _) =
            with_client_files("client-no-key", Some(&client_certificate_a()), None);
        let [problem] = transport.client_problems() else {
            panic!("one problem expected: {:?}", transport.client_problems());
        };
        assert_eq!(problem.path, certificate);
        assert!(problem.detail.contains(CLIENT_KEY), "{problem}");
        assert!(refuses_every_handshake(&transport));

        let (transport, _, key) = with_client_files(
            "client-no-certificate",
            None,
            Some(&client_key(&CLIENT_A_KEY)),
        );
        let [problem] = transport.client_problems() else {
            panic!("one problem expected: {:?}", transport.client_problems());
        };
        assert_eq!(problem.path, key);
        assert!(problem.detail.contains(CLIENT_CERTIFICATE), "{problem}");
        assert!(refuses_every_handshake(&transport));
    }

    /// NET-11: an encrypted key with no passphrase named is refused by its path, and the refusal
    /// names the variable that would open it. Rejects an implementation that reads the key as
    /// absent, or that keeps the old advice to decrypt the file by hand.
    #[test]
    fn an_encrypted_key_with_no_passphrase_is_refused_naming_the_variable() {
        let (transport, key) = with_encrypted_key("client-encrypted-key", None);

        let [problem] = transport.client_problems() else {
            panic!("one problem expected: {:?}", transport.client_problems());
        };
        assert_eq!(problem.path, key);
        assert!(problem.detail.contains("encrypted"), "{problem}");
        assert!(problem.detail.contains(CLIENT_KEY_PASSPHRASE), "{problem}");
        assert!(refuses_every_handshake(&transport));
    }

    /// NET-11: a wrong passphrase is a refusal that holds neither the passphrase nor any key bytes.
    /// Rejects an implementation whose error forwards the decryption library's message or the
    /// value it was given.
    #[test]
    fn a_wrong_passphrase_is_refused_without_repeating_it() {
        let (transport, key) =
            with_encrypted_key("client-wrong-passphrase", Some("battery-staple"));

        let [problem] = transport.client_problems() else {
            panic!("one problem expected: {:?}", transport.client_problems());
        };
        assert_eq!(problem.path, key);
        assert!(!problem.to_string().contains("battery-staple"), "{problem}");
        assert!(!format!("{transport:?}").contains("battery-staple"));
        assert!(refuses_every_handshake(&transport));
    }

    /// NET-11: the right passphrase opens the key and the pair is presented. Rejects an
    /// implementation that decrypts but then fails to hand the key to the pair check.
    #[test]
    fn the_right_passphrase_opens_an_encrypted_key_and_the_pair_is_presented() {
        let (transport, _) = with_encrypted_key("client-right-passphrase", Some(PASSPHRASE));

        assert!(
            transport.client_problems().is_empty(),
            "{:?}",
            transport.client_problems()
        );
        assert!(transport.presents_client_certificate());
        assert!(!format!("{transport:?}").contains(PASSPHRASE));
    }

    /// NET-11: the passphrase and the two paths are read from the environment only. A settings
    /// file that states all three leaves the transport naming and presenting nothing, and a
    /// passphrase the file states does not open a key the environment names. Rejects a reader that
    /// adds the passphrase or the paths to `SETTINGS_NAMES` or looks them up in the file's names.
    #[test]
    fn a_settings_file_cannot_state_the_client_pair_or_its_passphrase() {
        let directory = scratch("client-pair-from-a-settings-file");
        let certificate = directory.join("client.crt");
        let key = directory.join("client.key");
        std::fs::write(&certificate, certificate_for_encrypted_key()).expect("certificate");
        std::fs::write(&key, encrypted_key()).expect("key");
        let stated = vec![
            (
                CLIENT_CERTIFICATE.to_string(),
                certificate.to_str().unwrap().to_string(),
            ),
            (CLIENT_KEY.to_string(), key.to_str().unwrap().to_string()),
            (CLIENT_KEY_PASSPHRASE.to_string(), PASSPHRASE.to_string()),
        ];

        assert!(stated_names(stated.clone()).is_empty());
        let by_file = Transport::stated_by_file(stated.clone());
        assert!(!by_file.names_client_certificate());
        assert!(!by_file.presents_client_certificate());
        assert!(by_file.client_problems().is_empty());

        let file = stated
            .into_iter()
            .map(|(name, value)| (name, OsString::from(value)))
            .collect::<HashMap<_, _>>();
        let transport = Transport::from_sources(
            &environment(&[
                (CLIENT_CERTIFICATE, certificate.to_str().unwrap()),
                (CLIENT_KEY, key.to_str().unwrap()),
            ]),
            &|name| file.get(name).cloned(),
            || None,
        );
        assert!(!transport.presents_client_certificate());
        let [problem] = transport.client_problems() else {
            panic!("one problem expected: {:?}", transport.client_problems());
        };
        assert!(problem.detail.contains(CLIENT_KEY_PASSPHRASE), "{problem}");
    }

    /// NET-11: a key that opens but belongs to another certificate is refused as any other
    /// mismatched pair is. Rejects an implementation that skips the agreement check for a key it
    /// had to decrypt.
    #[test]
    fn a_decrypted_key_that_does_not_belong_to_the_certificate_is_refused() {
        let directory = scratch("client-decrypted-mismatch");
        std::fs::write(directory.join("client.crt"), client_certificate_a()).expect("certificate");
        std::fs::write(
            directory.join("client.key"),
            encrypted_key_of(&CLIENT_B_KEY),
        )
        .expect("key");
        let transport = Transport::from_sources(
            &environment(&[
                (
                    CLIENT_CERTIFICATE,
                    directory.join("client.crt").to_str().unwrap(),
                ),
                (CLIENT_KEY, directory.join("client.key").to_str().unwrap()),
                (CLIENT_KEY_PASSPHRASE, PASSPHRASE),
            ]),
            &|_| None,
            || None,
        );

        assert!(!transport.presents_client_certificate());
        assert_eq!(transport.client_problems().len(), 1);
        assert!(refuses_every_handshake(&transport));
    }

    const PASSPHRASE: &str = "correct-horse";

    /// `CLIENT_A_KEY` as PKCS#8 encrypted with PBES2 (PBKDF2 with HMAC-SHA256, AES-256-CBC), the
    /// form `openssl pkcs8 -topk8 -v2 aes256 -v2prf hmacWithSHA256` writes.
    fn encrypted_key() -> String {
        encrypted_key_of(&CLIENT_A_KEY)
    }

    fn encrypted_key_of(body: &[&str]) -> String {
        use pkcs8::pkcs5::pbes2::Parameters;
        use pkcs8::{DecodePrivateKey, PrivateKeyInfo};

        let plain = pkcs8::SecretDocument::from_pkcs8_pem(&client_key(body))
            .expect("the fixture key is PKCS#8");
        let info = plain
            .decode_msg::<PrivateKeyInfo>()
            .expect("private key info");
        let parameters =
            Parameters::pbkdf2_sha256_aes256cbc(2048, &[7; 16], &[9; 16]).expect("parameters");
        info.encrypt_with_params(parameters, PASSPHRASE)
            .expect("encrypts")
            .to_pem("ENCRYPTED PRIVATE KEY", pkcs8::LineEnding::LF)
            .expect("pem")
            .to_string()
    }

    /// The certificate `encrypted_key` belongs to.
    fn certificate_for_encrypted_key() -> String {
        client_certificate_a()
    }

    fn with_encrypted_key(name: &str, passphrase: Option<&str>) -> (Transport, PathBuf) {
        let directory = scratch(name);
        let certificate = directory.join("client.crt");
        let key = directory.join("client.key");
        std::fs::write(&certificate, certificate_for_encrypted_key()).expect("certificate");
        std::fs::write(&key, encrypted_key()).expect("key");
        let mut pairs = vec![
            (CLIENT_CERTIFICATE, certificate.to_str().unwrap()),
            (CLIENT_KEY, key.to_str().unwrap()),
        ];
        if let Some(passphrase) = passphrase {
            pairs.push((CLIENT_KEY_PASSPHRASE, passphrase));
        }
        (
            Transport::from_sources(&environment(&pairs), &|_| None, || None),
            key,
        )
    }

    /// NET-11: a key in OpenSSL's legacy encrypted form keeps a plain key label and says it is
    /// encrypted in a header. It is reported as that, apart from "does not match the certificate",
    /// which is what its ciphertext would be reported as if it were read as a key.
    #[test]
    fn a_legacy_openssl_encrypted_key_is_reported_as_encrypted_and_named_by_path() {
        let legacy = armoured(
            &format!("RSA {} KEY", "PRIVATE"),
            &[
                "Proc-Type: 4,ENCRYPTED",
                "DEK-Info: AES-256-CBC,00112233445566778899AABBCCDDEEFF",
                "MIHsMFcGCSqGSIb3DQEFDTBKMCkG",
            ],
        );
        let (transport, _, key) = with_client_files(
            "client-legacy-encrypted-key",
            Some(&client_certificate_a()),
            Some(&legacy),
        );

        let [problem] = transport.client_problems() else {
            panic!("one problem expected: {:?}", transport.client_problems());
        };
        assert_eq!(problem.path, key);
        assert!(problem.detail.contains("legacy encrypted"), "{problem}");
        assert!(problem.detail.contains("openssl pkcs8 -topk8"), "{problem}");
        assert!(!problem.detail.contains("does not match"), "{problem}");
        assert!(refuses_every_handshake(&transport));
    }

    /// NET-11: neither the key nor the certificate reaches a `Debug` print, which is what a trace
    /// or a panic message holds.
    #[test]
    fn a_debug_print_of_the_transport_holds_the_paths_and_none_of_the_key() {
        let (transport, certificate, key) = with_client_files(
            "client-debug",
            Some(&client_certificate_a()),
            Some(&client_key(&CLIENT_A_KEY)),
        );

        let printed = format!("{transport:?}");
        assert!(printed.contains(&key.display().to_string()), "{printed}");
        assert!(printed.contains(&certificate.display().to_string()));
        for line in CLIENT_A_KEY {
            assert!(!printed.contains(line), "key material in {printed}");
        }
        assert!(!printed.contains(CLIENT_A_CERTIFICATE[0]));
        assert!(
            printed.contains("client_passphrase_given: false"),
            "{printed}"
        );
    }

    /// NET-11: a `Debug` print says whether a passphrase was given and never what it was, which is
    /// what a trace or a panic message holds. Rejects an implementation that derives `Debug` over
    /// the field.
    #[test]
    fn a_debug_print_of_the_transport_says_a_passphrase_was_given_and_not_what_it_was() {
        let directory = scratch("client-debug-passphrase");
        std::fs::write(directory.join("client.crt"), client_certificate_a()).expect("certificate");
        std::fs::write(directory.join("client.key"), encrypted_key()).expect("key");
        let transport = Transport::stated_with_client_pair_and_passphrase(
            Some(&directory.join("client.crt")),
            Some(&directory.join("client.key")),
            PASSPHRASE,
        );

        let printed = format!("{transport:?}");

        assert!(
            printed.contains("client_passphrase_given: true"),
            "{printed}"
        );
        assert!(!printed.contains(PASSPHRASE), "{printed}");
        assert!(!printed.contains(&client_certificate_a()), "{printed}");
        assert!(transport.presents_client_certificate());
    }
}
