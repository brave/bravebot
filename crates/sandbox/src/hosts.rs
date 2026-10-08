//! Which host names a confined program that has egress may reach.
//!
//! A backend filters by address and port and never by name, so a name list is applied by a proxy
//! the session runs ([`crate::proxy`]) and this module is the list it asks. The rules are read
//! from a person's settings and the table of defaults below, never from what a program printed.
//! `docs/specs/sandboxing.md` ([SANDBOX-24]) decides what a list means.
//!
//! [SANDBOX-24]: ../../../docs/specs/sandboxing.md

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
    pub fn decide(&self, host: &str) -> Verdict {
        let host = normalise(host);
        if let Some(rule) = self.denied.iter().find(|rule| rule.covers(&host)) {
            return Verdict::Refused(Refusal::Denied(rule.clone()));
        }
        match self.allowed.iter().find(|rule| rule.covers(&host)) {
            Some(rule) => Verdict::Allowed(rule.clone()),
            None => Verdict::Refused(Refusal::NotListed),
        }
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
        Toolchain::Maven | Toolchain::Gradle => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
