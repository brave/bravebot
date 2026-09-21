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

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use ureq::Proxy;
use ureq::ProxyProtocol;
use ureq::config::ConfigBuilder;
use ureq::tls::{Certificate, PemItem, RootCerts, TlsConfig, parse_pem};
use ureq::typestate::AgentScope;

/// The variable naming a file of certificates to trust instead of the built-in set.
pub const CERTIFICATE_FILE: &str = "SSL_CERT_FILE";

/// The variable naming a directory of them.
pub const CERTIFICATE_DIRECTORY: &str = "SSL_CERT_DIR";

/// The variables a proxy is named in, in the order the transport reads them. Each is read in lower
/// case as well, which is the form some networks hand out, and the first one set wins.
pub const PROXY_VARIABLES: &[&str] = &["ALL_PROXY", "HTTPS_PROXY", "HTTP_PROXY"];

/// The variable naming the hosts a proxy is not used for.
pub const NO_PROXY: &str = "NO_PROXY";

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

/// What every HTTP client in this process trusts, and what it goes through.
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
    /// What `NO_PROXY` held, for the report. The transport library reads it for itself.
    no_proxy: Option<String>,
}

impl Transport {
    /// What the environment asks for, read once for the whole process.
    ///
    /// Once, because a certificate directory is every file in it and a client is built for each
    /// turn, each model roster and each update check; and because two clients resolving the
    /// environment at different moments could disagree about what this process trusts.
    pub fn shared() -> &'static Self {
        static SHARED: OnceLock<Transport> = OnceLock::new();
        SHARED.get_or_init(Self::from_env)
    }

    fn from_env() -> Self {
        let mut transport = Self::resolve(
            TrustRoots::from_env(|name| std::env::var_os(name)),
            // The transport library's own reader, so the variables it looks at and the `NO_PROXY`
            // exceptions it honours are the ones every other client of it honours. Called here
            // rather than left to its default, so a release that stopped calling it changes
            // nothing about this program and the tests below fail rather than the proxy quietly
            // going away.
            Proxy::try_from_env(),
        );
        transport.no_proxy = [NO_PROXY, "no_proxy"]
            .iter()
            .find_map(|name| std::env::var(name).ok())
            .filter(|value| !value.is_empty());
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
            no_proxy: None,
        }
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

    /// The hosts `NO_PROXY` excludes from the proxy, verbatim.
    ///
    /// Worth reporting because it decides whether a proxy in force applies to the host that is
    /// failing, and `NO_PROXY=*` leaves one configured and used for nothing.
    pub fn no_proxy(&self) -> Option<&str> {
        self.no_proxy.as_deref()
    }

    /// An agent configuration that trusts what this says and goes where this says.
    ///
    /// The one place either is set. A caller adds its own timeouts and builds.
    pub fn agent_config_builder(&self) -> ConfigBuilder<AgentScope> {
        ureq::Agent::config_builder()
            .tls_config(TlsConfig::builder().root_certs(self.root_certs()).build())
            .proxy(self.proxy.clone())
    }

    fn root_certs(&self) -> RootCerts {
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
            .field("no_proxy", &self.no_proxy)
            .finish()
    }
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
}
