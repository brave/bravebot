//! Code that two crates each hold a copy of, because neither may depend on the other, read through
//! the public face of each.
//!
//! `bravebot-agent` depends on both `bravebot-core` and `bravebot-sandbox`, so it is the one place
//! a test can hold the two copies together. Two copies are checked:
//!
//! - The IPv4 form of an address. `carried_ipv4` in `bravebot_core::ambient` and `ipv4_form` in
//!   `bravebot_sandbox::address` both reduce an IPv4 address inside an IPv6 one to that IPv4
//!   address. Neither function is public, so core is read through `canonical_host` and the sandbox
//!   through `HostList::decide`, `HostList::denying` and `is_public`
//!   ([SANDBOX-24](../../../docs/specs/sandboxing.md)).
//! - The fold of a name. `fold_case` and `canonical_key` in `bravebot_core::trust` are the fold the
//!   trust map compares names under ([TRUST-2](../../../docs/specs/trust-map.md)). `matches_folded`
//!   in `bravebot_sandbox::rules` applies the same fold to a glob, and is read through
//!   `rules::resolve` ([SANDBOX-25](../../../docs/specs/sandboxing.md)).
//!
//! Each table holds the expected value as well as comparing the two sides, so a change that moves
//! both copies the same way fails too.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_core::ambient::{CanonicalHost, canonical_host};
use bravebot_core::trust::canonical_key;
use bravebot_sandbox::address::is_public;
use bravebot_sandbox::hosts::{HostList, Refusal, Verdict};
use bravebot_sandbox::rules::{Entry, Lists, State, resolve};
use std::fs;
use std::net::{IpAddr, Ipv4Addr};

/// The list that denies exactly the entries given.
fn denying(entries: &[&str]) -> HostList {
    let (list, invalid) = HostList::parse([], entries.iter().copied());
    assert!(invalid.is_empty(), "{invalid:?} are not entries");
    list
}

/// Whether a list that denies exactly `denied` refuses `host` as a denial, and not as a host that is
/// merely unlisted.
fn refuses(denied: &str, host: &str) -> bool {
    matches!(
        denying(&[denied]).decide(host),
        Verdict::Refused(Refusal::Denied(_))
    )
}

/// The address a spelling writes, without the brackets or the trailing dot a host may carry.
fn bare(spelling: &str) -> IpAddr {
    spelling
        .trim_end_matches('.')
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .unwrap_or_else(|_| panic!("{spelling} is not an address"))
}

/// A spelling, the address it reads as, and whether that address is on the public internet.
///
/// The address is the IPv4 address the spelling carries where it is an IPv4-mapped (`::ffff:0:0/96`)
/// or a NAT64 (`64:ff9b::/96`) address, and the IPv6 address itself where it is not. The rows that
/// are not carried are the ones a wrong prefix test would carry: the deprecated IPv4-compatible
/// form, a NAT64 prefix with another third segment, a prefix with a non-zero segment among the
/// four after `64:ff9b`, and the IPv4-translated form `::ffff:0:a.b.c.d`.
const SPELLINGS: &[(&str, &str, bool)] = &[
    ("93.184.216.34", "93.184.216.34", true),
    ("::ffff:93.184.216.34", "93.184.216.34", true),
    ("::ffff:5db8:d822", "93.184.216.34", true),
    ("64:ff9b::5db8:d822", "93.184.216.34", true),
    ("64:ff9b::93.184.216.34", "93.184.216.34", true),
    ("0:0:0:0:0:ffff:5db8:d822", "93.184.216.34", true),
    ("::FFFF:5DB8:D822", "93.184.216.34", true),
    ("64:FF9B::5DB8:D822", "93.184.216.34", true),
    ("[::ffff:93.184.216.34]", "93.184.216.34", true),
    ("[64:ff9b::5db8:d822]", "93.184.216.34", true),
    ("93.184.216.34.", "93.184.216.34", true),
    ("[::ffff:93.184.216.34].", "93.184.216.34", true),
    ("::93.184.216.34", "::93.184.216.34", true),
    ("64:ff9b:1::5db8:d822", "64:ff9b:1::5db8:d822", true),
    ("64:ff9b::1:0:5db8:d822", "64:ff9b::1:0:5db8:d822", true),
    ("64:ff9b::1:5db8:d822", "64:ff9b::1:5db8:d822", true),
    ("::ffff:0:93.184.216.34", "::ffff:0:93.184.216.34", true),
    ("2606:4700:4700::1111", "2606:4700:4700::1111", true),
    ("::1", "::1", false),
    ("127.0.0.1", "127.0.0.1", false),
    ("::ffff:127.0.0.1", "127.0.0.1", false),
    ("::ffff:169.254.169.254", "169.254.169.254", false),
    ("64:ff9b::a9fe:a9fe", "169.254.169.254", false),
    ("64:ff9b:1::a9fe:a9fe", "64:ff9b:1::a9fe:a9fe", true),
    ("::127.0.0.1", "::127.0.0.1", true),
];

/// Core and the host list read every spelling of an address as the same address, and as the one the
/// table names.
///
/// Core is asked through `canonical_host`, which gives the address a spelling is read as. The host
/// list is asked whether a denied entry that is the expected IPv4 address refuses the spelling,
/// since the list reduces an address to its IPv4 form and compares that text with its entries. No
/// entry can spell an IPv6 address, so an address that is not carried is asked about the other way:
/// the IPv4 address its last 32 bits spell must not refuse it. A denied entry for another address
/// refuses no spelling.
///
/// The regressions it rejects: a NAT64 address left as IPv6 on either side, so a denied
/// `93.184.216.34` misses `64:ff9b::5db8:d822` or core reads it as another host than the list does;
/// a prefix tested on five segments or fewer, which carries `64:ff9b::1:5db8:d822`; the
/// IPv4-compatible form `::93.184.216.34` or the translated form `::ffff:0:93.184.216.34` read as
/// IPv4; and a bracket, a trailing dot or upper case that one side reads as a name.
#[test]
fn an_ipv4_address_inside_an_ipv6_one_is_read_as_that_ipv4_address_by_core_and_by_the_host_list() {
    for (spelling, expected, _) in SPELLINGS {
        let expected: IpAddr = expected.parse().unwrap();
        assert_eq!(
            canonical_host(spelling),
            CanonicalHost::Address(expected),
            "core reads {spelling}"
        );
        assert!(
            !refuses("192.0.2.1", spelling),
            "a denied 192.0.2.1 refuses {spelling}"
        );
        match expected {
            IpAddr::V4(carried) => {
                let carried = carried.to_string();
                assert!(
                    refuses(&carried, spelling),
                    "a denied {carried} does not refuse {spelling}"
                );
                assert!(
                    denying(&[&carried]).denying(bare(spelling)).is_some(),
                    "a denied {carried} does not cover the address {spelling} spells"
                );
            }
            IpAddr::V6(not_carried) => {
                let [.., a, b, c, d] = not_carried.octets();
                let last_32_bits = Ipv4Addr::new(a, b, c, d).to_string();
                assert!(
                    !refuses(&last_32_bits, spelling),
                    "a denied {last_32_bits} refuses {spelling}, which does not carry it"
                );
                assert!(
                    denying(&[&last_32_bits]).denying(bare(spelling)).is_none(),
                    "a denied {last_32_bits} covers the address {spelling} spells"
                );
                let (_, invalid) = HostList::parse([], [not_carried.to_string().as_str()]);
                assert_eq!(
                    invalid.len(),
                    1,
                    "an entry spells the IPv6 address {not_carried}"
                );
            }
        }
    }
}

/// The classification of an address is the classification of the address it carries.
///
/// Core has no classification of an address, so this holds the sandbox's `is_public` to the table
/// alone: `::ffff:127.0.0.1` and `64:ff9b::a9fe:a9fe` are the loopback and the metadata address and
/// are not public, while `64:ff9b:1::a9fe:a9fe` and `::127.0.0.1` carry nothing and are public
/// IPv6 addresses.
///
/// The regressions it rejects: a classification that reads the IPv6 spelling, so the mapped
/// loopback is public, and one that carries too much, so an address in another NAT64 prefix or the
/// IPv4-compatible form is read as the IPv4 address it ends in and is not public.
#[test]
fn an_address_is_classified_as_the_address_it_carries() {
    for (spelling, expected, public) in SPELLINGS {
        let expected: IpAddr = expected.parse().unwrap();
        assert_eq!(is_public(bare(spelling)), *public, "{spelling}");
        assert_eq!(
            is_public(bare(spelling)),
            is_public(expected),
            "{spelling} and {expected}"
        );
    }
}

/// A number written another way than dotted decimal or an IPv6 address is read differently by the
/// two crates, on purpose.
///
/// Core reads each of `2130706433`, `0x7f000001`, `0177.0.0.1` and `127.1` as the address a
/// resolver reads, since a rule about an address has to cover every spelling a connection can use.
/// The host list reads them as names: `address_of` in `bravebot_sandbox::hosts` gives an address
/// only for the spellings `std` parses. A denied `127.0.0.1` therefore does not refuse them by
/// text, and a denied entry spelling one refuses it as the name it is. What the proxy then connects
/// to is the answer of a lookup, which it compares with the denied entries and classifies
/// ([SANDBOX-24](../../../docs/specs/sandboxing.md)).
///
/// The regressions it rejects: the host list reading a dword or a short form as an address, which
/// changes the verdict on a name an allowed wildcard covers, and core reading one of them as a
/// name, which leaves a rule about `127.0.0.1` unmatched by it.
#[test]
fn a_number_written_another_way_is_an_address_to_core_and_a_name_to_the_host_list() {
    let loopback = IpAddr::from([127, 0, 0, 1]);
    assert!(refuses("127.0.0.1", "127.0.0.1"));
    for spelling in ["2130706433", "0x7f000001", "0177.0.0.1", "127.1"] {
        assert_eq!(
            canonical_host(spelling),
            CanonicalHost::Address(loopback),
            "core reads {spelling}"
        );
        assert!(
            !refuses("127.0.0.1", spelling),
            "the host list reads {spelling} as the address 127.0.0.1"
        );
        assert!(
            refuses(spelling, spelling),
            "the host list does not read {spelling} as a name"
        );
    }
}

/// Whether a `denyRead` glob that is `typed` and a `*` names a file made as `made`, in a directory
/// that holds that file alone.
///
/// The glob folds, so it names the file when the fold of `made` starts with the fold of `typed`.
/// The volume is not asked: the walk lists the directory, and the file keeps the spelling it was
/// made with, so the answer is the same on a volume that folds case and on one that does not. The
/// `*` is the only wildcard, so neither name may hold one.
fn glob_names(typed: &str, made: &str) -> bool {
    assert!(
        !typed.contains(['*', '?']) && !made.contains(['*', '?']),
        "a name with a wildcard in it is not a name"
    );
    let base = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    fs::write(base.path().join(made), "").unwrap();
    let lists = Lists {
        deny_read: vec![Entry {
            path: format!("{typed}*"),
            by: None,
            pinned: false,
            from_a_checkout: false,
        }],
        ..Lists::default()
    };
    let rules = resolve(&lists, Some(home.path()), None, base.path());
    let State::InForce(paths) = &rules.items()[0].state else {
        panic!("the glob {typed:?}* was refused");
    };
    let root = fs::canonicalize(base.path()).unwrap();
    for path in paths {
        assert_eq!(path.parent(), Some(root.as_path()), "{path:?}");
    }
    !paths.is_empty()
}

/// Two names and whether the trust map's fold joins them, which is
/// `canonical_key(a, true) == canonical_key(b, true)`: canonical decomposition, then `fold_case`,
/// which takes upper case first and then lower case, and decomposes again.
///
/// The Turkish dotted capital I folds to `i` and a combining dot, so it joins `i\u{307}` and not
/// `i`. The dotless i, the long s, the Kelvin sign, the final sigma and the ligature `\u{fb01}` join
/// the letters they fold to through upper case. The sharp s joins `ss` in either case. The
/// full-width `s`, `\u{e9}` and `e`, and `resume` and `r\u{e9}sume` are held apart: no compatibility
/// form, accent or space is folded. The capital sharp s `\u{1e9e}` lowercases to `\u{df}`, which
/// `fold_case` does not take on to `ss`, so it joins neither `\u{df}` nor `ss`; the glob holds them
/// apart too.
const NAMES: &[(&str, &str, bool)] = &[
    ("SERVER.PEM", "server.pem", true),
    ("stra\u{df}e", "STRASSE", true),
    ("stra\u{df}e", "strasse", true),
    ("\u{212a}ey", "key", true),
    ("\u{17f}ecret", "secret", true),
    ("\u{130}x", "ix", false),
    ("\u{130}x", "i\u{307}x", true),
    ("\u{131}x", "ix", true),
    ("\u{d55c}", "\u{1112}\u{1161}\u{11ab}", true),
    ("R\u{e9}sum\u{e9}", "Re\u{301}sume\u{301}", true),
    ("\u{212b}", "\u{c5}", true),
    ("e\u{301}", "\u{c9}", true),
    ("\u{3c2}", "\u{3c3}", true),
    ("\u{fb01}", "fi", true),
    ("resume", "r\u{e9}sume", false),
    ("\u{e9}", "e", false),
    ("\u{ff53}x", "sx", false),
    ("\u{1e9e}", "\u{df}", false),
    ("\u{1e9e}", "ss", false),
];

/// The trust map's fold joins the names the table says it joins and no others.
///
/// The two other tests read the sandbox's copy against this table, so this one holds the table to
/// the trust map. Without it a table that drifts from `canonical_key` would make a glob fold that
/// drifts the same way pass.
///
/// The regression it rejects: a change to `fold_case` or `canonical_key` that no longer joins a
/// name the table lists, or joins one it does not.
#[test]
fn the_trust_maps_fold_joins_the_names_the_table_lists() {
    for (a, b, joined) in NAMES {
        assert_eq!(
            canonical_key(a, true) == canonical_key(b, true),
            *joined,
            "the trust map's fold of {a:?} and {b:?}"
        );
    }
}

/// A deny glob names a file wherever the trust map's fold joins the two names, in both directions.
///
/// The sandbox cannot depend on the trust map, so `matches_folded` holds its own copy of the fold
/// of `canonical_key`. A glob that missed a join would leave readable a file a volume opens under
/// the name the person denied, which is what the fold is for.
///
/// The regressions it rejects: a glob fold that lowercases only, so it misses the Kelvin sign, the
/// long s and the sharp s; and one that does not decompose, so it misses the Angstrom sign, the
/// Hangul syllable and the two forms of `R\u{e9}sum\u{e9}`.
#[test]
fn a_deny_glob_names_a_file_wherever_the_trust_maps_fold_joins_the_two_names() {
    for (a, b, joined) in NAMES.iter().filter(|(_, _, joined)| *joined) {
        assert!(joined);
        assert!(
            glob_names(b, a),
            "the trust map joins {a:?} and {b:?}, and the glob {b:?}* misses a file made as {a:?}"
        );
        assert!(
            glob_names(a, b),
            "the trust map joins {a:?} and {b:?}, and the glob {a:?}* misses a file made as {b:?}"
        );
    }
}

/// A deny glob names no file the trust map's fold holds apart from the glob's name.
///
/// The sandbox compares under one fixed fold, which a volume may join less than, so a glob may name
/// more than a volume opens and that errs toward holding back. No row of the table is such an extra
/// match today. This test makes one visible when it appears, as a row to add to the table with a
/// reason, and fails on a fold that joins a name the trust map holds apart.
///
/// The regressions it rejects: a glob fold that strips accents, so `resume` meets `r\u{e9}sume`;
/// one that reads compatibility forms, so the full-width `s` meets `s`; and one that does not keep
/// the dotted capital I apart from `i`.
#[test]
fn a_deny_glob_names_no_file_the_trust_maps_fold_holds_apart() {
    for (a, b, _) in NAMES.iter().filter(|(_, _, joined)| !*joined) {
        assert!(
            !glob_names(b, a),
            "the trust map holds {a:?} and {b:?} apart, and the glob {b:?}* names a file made as {a:?}"
        );
        assert!(
            !glob_names(a, b),
            "the trust map holds {a:?} and {b:?} apart, and the glob {a:?}* names a file made as {b:?}"
        );
    }
}
