//! Whether an address is on the public internet, or on this machine or a network it sits inside.
//!
//! A name resolves to whatever its server answers, so a check on the name says nothing about where
//! a connection to it ends. `docs/specs/tools/fetch-url.md` ([FETCH-8]) and
//! `docs/specs/sandboxing.md` ([SANDBOX-24]) decide what is done with an answer that is not public.
//!
//! [FETCH-8]: ../../../docs/specs/tools/fetch-url.md
//! [SANDBOX-24]: ../../../docs/specs/sandboxing.md

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Whether a connection to `address` leaves for the public internet, as opposed to ending on this
/// machine or on a network the machine sits inside.
pub fn is_public(address: IpAddr) -> bool {
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
}
