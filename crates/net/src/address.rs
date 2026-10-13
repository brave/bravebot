//! Which addresses a fetched host may resolve to.
//!
//! A name is the part of a URL a person reads and an address is the part a socket uses, and
//! whoever answers the lookup decides how the first becomes the second. So the policy's check of
//! the host as written (`docs.example.com`) says nothing about whether the connection ends on a
//! loopback port, a router or a cloud metadata service. This module asks the second question, and
//! because it asks it inside the lookup the client makes, the address checked is the address
//! connected to: ureq connects to what its resolver returns and to nothing else, so there is no
//! second lookup for an answer to differ in.

use std::net::IpAddr;
use ureq::config::Config;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::NextTimeout;

pub(crate) use bravebot_sandbox::address::is_public;

/// What a lookup that found a non-public address fails with.
///
/// Carries nothing, so no layer above can quote the address back: it is how the egress path tells
/// this refusal from a lookup that failed, and the sentence it reports is its own.
#[derive(Debug)]
pub(crate) struct NotPublic;

impl std::fmt::Display for NotPublic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the host resolved to an address that is not public")
    }
}

impl std::error::Error for NotPublic {}

/// Whether a ureq failure is [`NotPublic`].
pub(crate) fn is_refusal(error: &ureq::Error) -> bool {
    matches!(error, ureq::Error::Other(inner) if inner.is::<NotPublic>())
}

/// The resolver a fetch in flight is sent with.
///
/// Refuses when any address the name resolved to is not public, rather than filtering them out:
/// an answer that mixes the two is a name pointing somewhere it should not, and connecting to the
/// public half of it would be trusting the server that wrote the other half.
///
/// A host written as an address is passed through. Nothing was resolved, so there is no name for
/// an answer to stand behind, and the person approved exactly that host.
#[derive(Debug)]
pub(crate) struct GuardedResolver {
    inner: Box<dyn Resolver>,
    admits: fn(IpAddr) -> bool,
}

impl GuardedResolver {
    pub(crate) fn new() -> Self {
        Self {
            inner: Box::new(DefaultResolver::default()),
            admits: is_public,
        }
    }

    /// As [`GuardedResolver::new`], over an answer a test chooses and a classification it chooses.
    #[cfg(test)]
    pub(crate) fn over(inner: impl Resolver, admits: fn(IpAddr) -> bool) -> Self {
        Self {
            inner: Box::new(inner),
            admits,
        }
    }
}

impl Resolver for GuardedResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let addresses = self.inner.resolve(uri, config, timeout)?;
        if uri.host().is_some_and(is_address_literal) {
            return Ok(addresses);
        }
        if addresses.iter().all(|address| (self.admits)(address.ip())) {
            Ok(addresses)
        } else {
            Err(ureq::Error::Other(Box::new(NotPublic)))
        }
    }
}

/// Whether `host`, as a URL writes it, is an IP address and not a name.
///
/// Only the forms that are unambiguous. `2130706433` and `0x7f.1` are names to this check, so the
/// lookup that turns them into an address is classified like any other.
fn is_address_literal(host: &str) -> bool {
    host.strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host)
        .parse::<IpAddr>()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_written_as_an_address_is_not_a_name() {
        assert!(is_address_literal("127.0.0.1"));
        assert!(is_address_literal("169.254.169.254"));
        assert!(is_address_literal("[::1]"));
        assert!(!is_address_literal("localhost"));
        assert!(!is_address_literal("docs.example.com"));
        assert!(!is_address_literal("2130706433"));
        assert!(!is_address_literal("0x7f.1"));
    }
}
