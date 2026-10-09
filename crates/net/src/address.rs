//! Which addresses a fetched host may resolve to.
//!
//! A name is the part of a URL a person reads and an address is the part a socket uses, and
//! whoever answers the lookup decides how the first becomes the second. So the policy's check of
//! the host as written (`docs.example.com`) says nothing about whether the connection ends on a
//! loopback port, a router or a cloud metadata service. This module asks the second question, and
//! because it asks it inside the lookup the client makes, the address checked is the address
//! connected to: ureq connects to what its resolver returns and to nothing else, so there is no
//! second lookup for an answer to differ in.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use ureq::config::Config;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::NextTimeout;

/// Whether a connection to `address` leaves for the public internet, as opposed to ending on this
/// machine or on a network the machine sits inside.
pub(crate) fn is_public(address: IpAddr) -> bool {
    match address.to_canonical() {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => public_v6(v6),
    }
}

fn public_v4(address: Ipv4Addr) -> bool {
    let [a, b, ..] = address.octets();
    !(address.is_unspecified()
        || address.is_loopback()
        // 10/8, 172.16/12 and 192.168/16.
        || address.is_private()
        // 169.254/16, which holds the metadata address every large cloud serves.
        || address.is_link_local()
        || address.is_broadcast()
        || address.is_multicast()
        // 0/8, "this network".
        || a == 0
        // 100.64/10, carrier-grade NAT: a provider's inside, and some metadata services.
        || (a == 100 && (b & 0b1100_0000) == 64)
        // 240/4, reserved.
        || a >= 240)
}

fn public_v6(address: Ipv6Addr) -> bool {
    let first = address.segments()[0];
    // 64:ff9b::/96 carries an IPv4 address in its last 32 bits, and is what a resolver on a NAT64
    // network answers with for an IPv4-only name, so the address it carries is the one classified.
    if address.segments()[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        let [.., a, b, c, d] = address.octets();
        return public_v4(Ipv4Addr::new(a, b, c, d));
    }
    !(address.is_unspecified()
        || address.is_loopback()
        || address.is_multicast()
        // fc00::/7, unique-local.
        || (first & 0xfe00) == 0xfc00
        // fe80::/10, link-local, and the deprecated site-local fec0::/10.
        || (first & 0xffc0) == 0xfe80
        || (first & 0xffc0) == 0xfec0)
}

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

    fn address(text: &str) -> IpAddr {
        text.parse().expect("an address")
    }

    #[test]
    fn every_class_the_issue_names_is_not_public() {
        for text in [
            "127.0.0.1",
            "127.8.8.8",
            "::1",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "169.254.0.1",
            "fe80::1",
            "fec0::1",
            "fd00:ec2::254",
            "fc00::1",
            "100.64.0.1",
            "100.100.100.200",
            "0.0.0.0",
            "::",
            "255.255.255.255",
            "224.0.0.1",
            "ff02::1",
            "240.0.0.1",
            "0.1.2.3",
        ] {
            assert!(!is_public(address(text)), "{text} is not public");
        }
    }

    #[test]
    fn a_public_address_is_public() {
        for text in [
            "93.184.216.34",
            "8.8.8.8",
            "172.15.255.255",
            "172.32.0.1",
            "100.63.255.255",
            "100.128.0.1",
            "2606:4700:4700::1111",
            "64:ff9b::808:808",
        ] {
            assert!(is_public(address(text)), "{text} is public");
        }
    }

    #[test]
    fn an_ipv4_address_in_ipv6_clothes_is_classified_as_the_ipv4_address() {
        for text in [
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::a9fe:a9fe",
            "64:ff9b::7f00:1",
        ] {
            assert!(!is_public(address(text)), "{text} is not public");
        }
        assert!(is_public(address("::ffff:8.8.8.8")));
    }

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
