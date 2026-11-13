//! Which host names a confined program that has egress may reach.
//!
//! A backend filters by address and port and never by name, so a name list is applied by a proxy
//! the session runs ([`crate::proxy`]) and this module is the list it asks. The rules are read
//! from a person's settings and the table of defaults below, never from what a program printed.
//! `docs/specs/sandboxing.md` ([SANDBOX-24]) decides what a list means.
//!
//! [SANDBOX-24]: ../../../docs/specs/sandboxing.md

use std::net::IpAddr;

use crate::address::ipv4_form;
use crate::toolchain::Toolchain;

/// The hosts a stage carrying the remote scope reaches by default.
pub const REMOTE_SCOPE_HOSTS: &[&str] =
    &["github.com", "api.github.com", "*.githubusercontent.com"];

/// One entry of a list: a host name, or `*.` and a domain for every name below it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRule {
    /// The name in lower case, without a trailing dot and, for a wildcard, without the `*.`.
    name: String,
    wildcard: bool,
}

impl HostRule {
    /// The rule an entry spells, or `None` for one that is not a host name or a `*.` domain.
    ///
    /// A wildcard is the leading `*.` and nothing else, so `*` alone, `a.*.com` and `*example.com`
    /// are refused rather than read as something wider than they look.
    pub fn parse(entry: &str) -> Option<Self> {
        let (wildcard, name) = match entry.trim().strip_prefix("*.") {
            Some(rest) => (true, rest),
            None => (false, entry.trim()),
        };
        let name = normalise(name);
        valid_name(&name).then_some(Self { name, wildcard })
    }

    /// Whether `host`, already normalised, is a name this rule covers. A wildcard covers every
    /// name below its domain and not the domain itself.
    fn covers(&self, host: &str) -> bool {
        if !self.wildcard {
            return host == self.name;
        }
        host.len() > self.name.len() + 1
            && host.ends_with(self.name.as_str())
            && host[..host.len() - self.name.len()].ends_with('.')
    }

    /// The entry as it is written in a list.
    pub fn spelling(&self) -> String {
        if self.wildcard {
            format!("*.{}", self.name)
        } else {
            self.name.clone()
        }
    }
}

/// A host name as it is compared: ASCII lower case and no trailing dot.
pub fn normalise(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// `host` as a record may carry it: the normalised name where it is a host name or an IP address,
/// and `None` for anything else. A program chooses the name it asks a proxy for, so a record that
/// held it verbatim would let a program put any bytes, a newline included, into a trail.
pub fn recordable(host: &str) -> Option<String> {
    let name = normalise(host);
    (valid_name(&name) || name.parse::<std::net::IpAddr>().is_ok()).then_some(name)
}

/// The IP address `host`, already normalised, spells, in brackets or not. An address written
/// another way, such as `2130706433`, is a name and gives `None`.
fn address_of(host: &str) -> Option<IpAddr> {
    let bare = host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host);
    bare.parse().ok()
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 253
        && name
            .split('.')
            .all(|label| !label.is_empty() && label.bytes().all(label_byte))
}

fn label_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

/// Why a host was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A rule in the denied list covers it.
    Denied(HostRule),
    /// No rule in the allowed list covers it.
    NotListed,
    /// The name is allowed, and the port asked for is one the proxy does not carry.
    Port,
    /// The request named an address that is not public, or a name that resolved to one, and no
    /// exact entry of the allowed list is that address.
    NotPublic,
}

/// What the list decided about one host, and the rule that decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allowed(HostRule),
    Refused(Refusal),
}

impl Verdict {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed(_))
    }
}

/// An allowed list and a denied list. A denied entry wins over an allowed one, whichever is
/// written first, and a name no entry of the allowed list covers is refused, so an empty list
/// refuses every host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostList {
    allowed: Vec<HostRule>,
    denied: Vec<HostRule>,
}

impl HostList {
    pub fn new(allowed: Vec<HostRule>, denied: Vec<HostRule>) -> Self {
        Self { allowed, denied }
    }

    /// The list two groups of entries spell. An entry that is not a valid rule is returned
    /// beside the list so a caller can report it, and is never read as a rule: dropping an
    /// allowed entry narrows the list and dropping a denied one would widen it, so a denied
    /// entry that does not parse is the caller's to refuse the whole list over.
    pub fn parse<'a>(
        allowed: impl IntoIterator<Item = &'a str>,
        denied: impl IntoIterator<Item = &'a str>,
    ) -> (Self, Vec<String>) {
        let mut invalid = Vec::new();
        let mut read = |entries: &mut dyn Iterator<Item = &'a str>| {
            entries
                .filter_map(|entry| {
                    HostRule::parse(entry).or_else(|| {
                        invalid.push(entry.to_string());
                        None
                    })
                })
                .collect::<Vec<_>>()
        };
        let allowed = read(&mut allowed.into_iter());
        let denied = read(&mut denied.into_iter());
        (Self { allowed, denied }, invalid)
    }

    /// Adds entries to the allowed list, as a table of defaults does.
    pub fn allow_all(&mut self, entries: &[&str]) {
        self.allowed
            .extend(entries.iter().filter_map(|entry| HostRule::parse(entry)));
    }

    /// The decision for `host`, from the name and nothing else.
    ///
    /// A host that is an IP address, in brackets or not, is compared in one form, an IPv4 address
    /// inside an IPv6 one (IPv4-mapped or NAT64) as that IPv4 address, so the spelling a program
    /// chose cannot pass a denied entry. Only an exact allowed entry names an address, as in
    /// [`Self::names_address`].
    pub fn decide(&self, host: &str) -> Verdict {
        let host = normalise(host);
        let address = address_of(&host).map(|address| ipv4_form(address).to_string());
        let host = address.clone().unwrap_or(host);
        if let Some(rule) = self.denied.iter().find(|rule| rule.covers(&host)) {
            return Verdict::Refused(Refusal::Denied(rule.clone()));
        }
        let names = |rule: &&HostRule| match address {
            Some(_) => !rule.wildcard && rule.name == host,
            None => rule.covers(&host),
        };
        match self.allowed.iter().find(names) {
            Some(rule) => Verdict::Allowed(rule.clone()),
            None => Verdict::Refused(Refusal::NotListed),
        }
    }

    /// Whether the allowed entries cover every name `rule` covers. Denied entries are not read.
    ///
    /// A wildcard rule is covered only by a wildcard on the same domain or on a domain above it, so
    /// a list of `api.example.com` does not cover `*.example.com`.
    ///
    /// A wildcard names no address, as in [`Self::decide`], so an exact rule spelling an address is
    /// covered only by an exact entry spelling the same address.
    pub fn allows_every_name_of(&self, rule: &HostRule) -> bool {
        let address = address_of(&rule.name).is_some();
        self.allowed.iter().any(|allowed| match rule.wildcard {
            false if address => !allowed.wildcard && allowed.name == rule.name,
            false => allowed.covers(&rule.name),
            true => allowed.wildcard && (allowed.name == rule.name || allowed.covers(&rule.name)),
        })
    }

    /// Whether a denied entry covers any name `rule` covers, for a list that can only allow, such as
    /// an OpenShell policy: there a rule that a denied entry overlaps at all has to be left out.
    pub fn denies_any_name_of(&self, rule: &HostRule) -> bool {
        self.denied
            .iter()
            .any(|denied| match (rule.wildcard, denied.wildcard) {
                (false, _) => denied.covers(&rule.name),
                (true, false) => rule.covers(&denied.name),
                (true, true) => {
                    denied.name == rule.name
                        || denied.covers(&rule.name)
                        || rule.covers(&denied.name)
                }
            })
    }

    /// Whether an exact entry of the allowed list is `address` and no denied entry covers it. An
    /// IPv4 address inside an IPv6 one is compared as that IPv4 address. A wildcard names no
    /// address, so `*.0.0.1` does not let a tunnel reach `127.0.0.1`, and no entry can spell an
    /// IPv6 address.
    pub fn names_address(&self, address: IpAddr) -> bool {
        self.denying(address).is_none() && {
            let address = ipv4_form(address).to_string();
            self.allowed
                .iter()
                .any(|rule| !rule.wildcard && rule.name == address)
        }
    }

    /// The denied entry that covers `address`, compared as [`Self::decide`] compares a host that is
    /// an address: an IPv4 address inside an IPv6 one as that IPv4 address.
    pub fn denying(&self, address: IpAddr) -> Option<&HostRule> {
        let address = ipv4_form(address).to_string();
        self.denied.iter().find(|rule| rule.covers(&address))
    }

    pub fn allowed(&self) -> &[HostRule] {
        &self.allowed
    }

    pub fn denied(&self) -> &[HostRule] {
        &self.denied
    }
}

/// The registry hosts a toolchain's programs fetch from by default.
pub fn registry_hosts(toolchain: Toolchain) -> &'static [&'static str] {
    match toolchain {
        Toolchain::Cargo => &["crates.io", "static.crates.io", "index.crates.io"],
        Toolchain::Node => &["registry.npmjs.org"],
        Toolchain::Python => &["pypi.org", "files.pythonhosted.org"],
        Toolchain::Go => &["proxy.golang.org"],
        Toolchain::Maven | Toolchain::Gradle | Toolchain::Gh => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A narrower list is one every name of which the wider list allows. A wildcard is covered only
    /// by a wildcard at or above its domain, and an exact entry by an exact one or a wildcard over it.
    #[test]
    fn a_list_allows_every_name_of_a_rule_only_when_each_name_is_covered() {
        let (list, _) = HostList::parse(["api.example.com", "*.github.io"], []);
        let rule = |entry: &str| HostRule::parse(entry).expect("a rule");
        for covered in [
            "api.example.com",
            "API.example.com.",
            "me.github.io",
            "*.me.github.io",
            "*.github.io",
        ] {
            assert!(list.allows_every_name_of(&rule(covered)), "{covered}");
        }
        for wider in [
            "*.example.com",
            "other.example.com",
            "github.io",
            "*.io",
            "*.com",
        ] {
            assert!(!list.allows_every_name_of(&rule(wider)), "{wider}");
        }
    }

    /// An exact entry naming an address is covered only by an exact entry naming the same address,
    /// since a wildcard names no address. The regression it rejects is a server declaration listing
    /// `1.2.216.34` kept under a session's `*.216.34`, which then reaches an address the session's
    /// own list refuses.
    #[test]
    fn a_wildcard_allows_no_address_a_narrower_list_names() {
        let rule = |entry: &str| HostRule::parse(entry).expect("a rule");
        let wildcard = list(&["*.216.34"], &[]);
        assert!(!wildcard.allows_every_name_of(&rule("1.2.216.34")));
        assert!(wildcard.allows_every_name_of(&rule("0x5d.0xb8.216.34")));
        assert!(wildcard.allows_every_name_of(&rule("a.216.34")));
        let exact = list(&["1.2.216.34"], &[]);
        assert!(exact.allows_every_name_of(&rule("1.2.216.34")));
        assert!(!exact.allows_every_name_of(&rule("5.6.216.34")));
    }

    /// A record carries a host name or an address, normalised, and nothing else. The regression
    /// it rejects is a name with a control character, a space or a bracket reaching a trail.
    #[test]
    fn only_a_host_name_or_an_address_may_be_recorded() {
        assert_eq!(recordable("GitHub.com."), Some("github.com".to_string()));
        assert_eq!(recordable("127.0.0.1"), Some("127.0.0.1".to_string()));
        assert_eq!(recordable("::1"), Some("::1".to_string()));
        for bad in [
            "",
            "a b.example",
            "a\nb.example",
            "x\u{1b}[2J.example",
            "a..b",
            "-.é",
        ] {
            assert_eq!(recordable(bad), None, "{bad:?}");
        }
    }

    fn list(allowed: &[&str], denied: &[&str]) -> HostList {
        let (list, invalid) = HostList::parse(allowed.iter().copied(), denied.iter().copied());
        assert!(invalid.is_empty(), "{invalid:?}");
        list
    }

    #[test]
    fn an_exact_entry_covers_that_name_and_no_other() {
        let list = list(&["github.com"], &[]);
        assert!(list.decide("github.com").is_allowed());
        assert!(list.decide("GitHub.com.").is_allowed());
        for host in [
            "api.github.com",
            "evilgithub.com",
            "github.com.evil.example",
        ] {
            assert_eq!(
                list.decide(host),
                Verdict::Refused(Refusal::NotListed),
                "{host}"
            );
        }
    }

    #[test]
    fn a_wildcard_covers_names_below_the_domain_and_not_the_domain() {
        let list = list(&["*.githubusercontent.com"], &[]);
        for host in ["raw.githubusercontent.com", "a.b.githubusercontent.com"] {
            assert!(list.decide(host).is_allowed(), "{host}");
        }
        for host in [
            "githubusercontent.com",
            "evilgithubusercontent.com",
            "raw.githubusercontent.com.evil.example",
            "xgithubusercontent.com",
        ] {
            assert!(!list.decide(host).is_allowed(), "{host}");
        }
    }

    #[test]
    fn a_denied_entry_wins_over_an_allowed_one_in_either_order() {
        let both = list(&["*.example.com", "api.example.com"], &["api.example.com"]);
        let wide = list(&["api.example.com"], &["*.example.com"]);
        for list in [both, wide] {
            assert!(matches!(
                list.decide("api.example.com"),
                Verdict::Refused(Refusal::Denied(_))
            ));
        }
        let narrow = list(&["*.example.com"], &["api.example.com"]);
        assert!(narrow.decide("www.example.com").is_allowed());
    }

    /// An address is named by an exact entry that spells it, an IPv4-mapped one by the entry
    /// spelling its IPv4 address, and by nothing else. The regressions it rejects: a wildcard whose
    /// domain matches the end of an address's text counting as naming it, an allowed entry winning
    /// over a denied one, and a mapped address missed by the entry that names its IPv4 form.
    #[test]
    fn an_address_is_named_only_by_an_exact_entry_no_denied_one_covers() {
        let ip = |text: &str| text.parse::<IpAddr>().unwrap();
        let exact = list(&["*.github.io", "127.0.0.1"], &[]);
        assert!(exact.names_address(ip("127.0.0.1")));
        assert!(exact.names_address(ip("::ffff:127.0.0.1")));
        assert!(!exact.names_address(ip("127.0.0.2")));
        assert!(!exact.names_address(ip("::1")));

        let wildcard = list(&["*.0.0.1"], &[]);
        assert_eq!(
            wildcard.decide("127.0.0.1"),
            Verdict::Refused(Refusal::NotListed)
        );
        assert!(!wildcard.names_address(ip("127.0.0.1")));

        let denied = list(&["10.0.0.1"], &["10.0.0.1"]);
        assert!(!denied.names_address(ip("10.0.0.1")));
    }

    /// The list reads an address in one form whatever the spelling: an IPv4-mapped address, in
    /// brackets or with a trailing dot, is decided as its IPv4 address, an allowed entry names an
    /// address only exactly, and a denied wildcard still covers the text of an IPv4 address. An
    /// address written another way is a name. The regressions it rejects: a mapped spelling
    /// passing a denied entry, a wildcard allowed entry listing an address, and a denied wildcard
    /// that stops refusing addresses below its domain.
    #[test]
    fn an_address_is_decided_in_its_ipv4_form_and_named_only_by_an_exact_allowed_entry() {
        let denied = HostRule::parse("93.184.216.34").unwrap();
        let both = list(&["*.216.34", "93.184.216.34"], &["93.184.216.34"]);
        for host in [
            "93.184.216.34",
            "93.184.216.34.",
            "::ffff:93.184.216.34",
            "[::ffff:93.184.216.34]",
            "[::FFFF:5db8:d822].",
        ] {
            assert_eq!(
                both.decide(host),
                Verdict::Refused(Refusal::Denied(denied.clone())),
                "{host}"
            );
        }

        let exact = list(&["*.216.34", "1.2.216.34"], &[]);
        let rule = HostRule::parse("1.2.216.34").unwrap();
        for host in ["1.2.216.34", "[::ffff:1.2.216.34]", "::ffff:102:d822"] {
            assert_eq!(exact.decide(host), Verdict::Allowed(rule.clone()), "{host}");
        }
        for host in ["5.6.216.34", "::ffff:5.6.216.34", "[::ffff:5.6.216.34]"] {
            assert_eq!(
                exact.decide(host),
                Verdict::Refused(Refusal::NotListed),
                "{host}"
            );
        }
        assert!(exact.decide("a.216.34").is_allowed(), "a name below it");

        let wide = list(&["5.6.216.34"], &["*.216.34"]);
        for host in ["5.6.216.34", "::ffff:5.6.216.34"] {
            assert!(
                matches!(wide.decide(host), Verdict::Refused(Refusal::Denied(_))),
                "{host}"
            );
        }

        let decimal = list(&["2130706433"], &[]);
        assert!(decimal.decide("2130706433").is_allowed());
        assert!(!decimal.decide("127.0.0.1").is_allowed());
    }

    /// An IPv4 address inside a NAT64 one (`64:ff9b::/96`) is decided as that IPv4 address, as an
    /// IPv4-mapped one is: a denied entry refuses it, a denied wildcard covers it, and an exact
    /// allowed entry names it, in the list's decision and in the check of an answer. The deprecated
    /// IPv4-compatible form `::a.b.c.d` stays an IPv6 address, which no entry can name. The
    /// regressions it rejects: the NAT64 spelling compared as spelled, so it passes a denied entry
    /// and misses the exact entry naming its IPv4 address, and the compatible form read as IPv4,
    /// which `bravebot-core` does not do.
    #[test]
    fn an_ipv4_address_inside_a_nat64_one_is_decided_as_the_ipv4_address() {
        let ip = |text: &str| text.parse::<IpAddr>().unwrap();
        let denied = HostRule::parse("93.184.216.34").unwrap();
        let both = list(&["*.216.34", "93.184.216.34"], &["93.184.216.34"]);
        for host in [
            "64:ff9b::5db8:d822",
            "[64:ff9b::93.184.216.34]",
            "64:FF9B::5DB8:D822.",
        ] {
            assert_eq!(
                both.decide(host),
                Verdict::Refused(Refusal::Denied(denied.clone())),
                "{host}"
            );
        }
        assert_eq!(both.denying(ip("64:ff9b::5db8:d822")), Some(&denied));
        assert_eq!(
            both.decide("::93.184.216.34"),
            Verdict::Refused(Refusal::NotListed)
        );
        assert_eq!(both.denying(ip("::93.184.216.34")), None);

        let wide = list(&["5.6.216.34"], &["*.216.34"]);
        assert!(matches!(
            wide.decide("64:ff9b::506:d822"),
            Verdict::Refused(Refusal::Denied(_))
        ));

        let exact = list(&["1.2.216.34", "127.0.0.1"], &[]);
        assert_eq!(
            exact.decide("[64:ff9b::102:d822]"),
            Verdict::Allowed(HostRule::parse("1.2.216.34").unwrap())
        );
        assert!(exact.names_address(ip("64:ff9b::7f00:1")));
        assert!(!exact.names_address(ip("::127.0.0.1")));
        assert!(!exact.names_address(ip("64:ff9b:1::7f00:1")));
    }

    #[test]
    fn an_empty_list_refuses_every_host() {
        let empty = HostList::default();
        for host in ["github.com", "127.0.0.1", "localhost"] {
            assert_eq!(
                empty.decide(host),
                Verdict::Refused(Refusal::NotListed),
                "{host}"
            );
        }
    }

    #[test]
    fn an_entry_that_is_not_a_name_or_a_leading_wildcard_is_returned_and_not_read() {
        let entries = [
            "*",
            "*example.com",
            "a.*.com",
            "*.",
            "",
            "exa mple.com",
            "https://example.com",
            "example.com:443",
            "example.com/path",
            "a..b",
        ];
        let (list, invalid) = HostList::parse(entries.iter().copied(), std::iter::empty());
        assert_eq!(invalid.len(), entries.len(), "{invalid:?}");
        assert!(list.allowed().is_empty());
        assert!(!list.decide("example.com").is_allowed());
    }

    #[test]
    fn a_rule_is_spelled_as_it_was_written_in_lower_case() {
        for entry in ["github.com", "*.example.com"] {
            assert_eq!(HostRule::parse(entry).unwrap().spelling(), entry);
        }
        assert_eq!(
            HostRule::parse("*.Example.COM.").unwrap().spelling(),
            "*.example.com"
        );
    }

    #[test]
    fn the_defaults_are_the_hosts_the_remote_scope_and_each_registry_need() {
        let mut list = HostList::default();
        list.allow_all(REMOTE_SCOPE_HOSTS);
        for host in [
            "github.com",
            "api.github.com",
            "objects.githubusercontent.com",
        ] {
            assert!(list.decide(host).is_allowed(), "{host}");
        }
        assert!(!list.decide("crates.io").is_allowed());
        let mut cargo = HostList::default();
        cargo.allow_all(registry_hosts(Toolchain::Cargo));
        for host in ["crates.io", "static.crates.io", "index.crates.io"] {
            assert!(cargo.decide(host).is_allowed(), "{host}");
        }
        assert!(!cargo.decide("registry.npmjs.org").is_allowed());
        assert!(registry_hosts(Toolchain::Maven).is_empty());
    }
    #[test]
    fn a_denied_entry_overlapping_a_rule_at_all_denies_some_name_of_it() {
        let (list, invalid) = HostList::parse(
            [],
            ["bad.example.com", "*.private.example.org", "*.corp.example"],
        );
        assert!(invalid.is_empty());
        let denies = |entry: &str| list.denies_any_name_of(&HostRule::parse(entry).unwrap());
        assert!(denies("bad.example.com"));
        assert!(denies("*.example.com"), "a wildcard over a denied name");
        assert!(denies("x.private.example.org"));
        assert!(denies("*.private.example.org"));
        assert!(
            denies("*.a.private.example.org"),
            "a wildcard below a denied one"
        );
        assert!(denies("*.example.org"), "a wildcard above a denied one");
        assert!(!denies("good.example.com"));
        assert!(
            !denies("private.example.org"),
            "a wildcard covers names below it only"
        );
        assert!(!denies("api.example.net"));
    }
}
