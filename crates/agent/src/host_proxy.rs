//! The proxies a session runs so a stage with egress reaches only the hosts its list names
//! (SANDBOX-24).
//!
//! A stage's list is the person's allowed set plus the defaults its own reason for egress brings
//! (the remote scope's hosts, a toolchain's registries), so two stages can hold different lists.
//! One proxy is started for each distinct list and kept for the rest of the process, which is the
//! session: a stage started later with the same list reaches the same port, and nothing is
//! listening once the process ends.

use bravebot_sandbox::hosts::{HostList, REMOTE_SCOPE_HOSTS, registry_hosts};
use bravebot_sandbox::proxy::{Proxy, ProxyConfig};
use bravebot_sandbox::toolchain::Toolchain;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

type Key = (Vec<String>, Vec<String>);

/// The list a stage is held to: the person's entries and the defaults named by its reasons.
///
/// An allowed entry that is not a rule is left out, which narrows the list. A denied one that is
/// not a rule would widen it if it were left out, so it fails the stage with the entry named.
pub fn list_for(
    allowed: &[String],
    denied: &[String],
    remote: bool,
    toolchains: &[Toolchain],
) -> Result<HostList, String> {
    let (mut list, invalid) = HostList::parse(
        allowed.iter().map(String::as_str),
        denied.iter().map(String::as_str),
    );
    if let Some(entry) = invalid
        .iter()
        .find(|entry| denied.iter().any(|kept| kept == *entry))
    {
        return Err(format!(
            "sandbox.network.deniedHosts names `{entry}`, which is not a host name, and a stage \
             started without it would reach the host it holds back"
        ));
    }
    if remote {
        list.allow_all(REMOTE_SCOPE_HOSTS);
    }
    for toolchain in toolchains {
        list.allow_all(registry_hosts(*toolchain));
    }
    Ok(list)
}

/// The proxy that applies `list`, started the first time it is asked for.
pub fn proxy_for(list: &HostList) -> std::io::Result<Arc<Proxy>> {
    static STARTED: OnceLock<Mutex<HashMap<Key, Arc<Proxy>>>> = OnceLock::new();
    let key: Key = (
        list.allowed().iter().map(|rule| rule.spelling()).collect(),
        list.denied().iter().map(|rule| rule.spelling()).collect(),
    );
    let mut started = STARTED
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(proxy) = started.get(&key) {
        return Ok(proxy.clone());
    }
    let proxy = Arc::new(Proxy::start(ProxyConfig::new(list.clone()))?);
    started.insert(key, proxy.clone());
    Ok(proxy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|entry| entry.to_string()).collect()
    }

    /// The remote scope and a toolchain each add their own hosts and no stage gets the other's,
    /// so a plain stage holds only what the person listed.
    #[test]
    fn a_stage_holds_the_defaults_its_own_reasons_bring_and_no_others() {
        let own = names(&["mine.example"]);
        let plain = list_for(&own, &[], false, &[]).unwrap();
        assert!(plain.decide("mine.example").is_allowed());
        assert!(!plain.decide("github.com").is_allowed());
        assert!(!plain.decide("crates.io").is_allowed());
        let remote = list_for(&own, &[], true, &[]).unwrap();
        assert!(remote.decide("api.github.com").is_allowed());
        assert!(!remote.decide("crates.io").is_allowed());
        let cargo = list_for(&own, &[], false, &[Toolchain::Cargo]).unwrap();
        assert!(cargo.decide("static.crates.io").is_allowed());
        assert!(!cargo.decide("github.com").is_allowed());
    }

    /// A denial the person wrote beats a default added after it.
    #[test]
    fn a_denied_host_is_refused_although_a_default_covers_it() {
        let list = list_for(&[], &names(&["api.github.com"]), true, &[]).unwrap();
        assert!(!list.decide("api.github.com").is_allowed());
        assert!(list.decide("github.com").is_allowed());
    }

    /// An allowed entry that is no rule narrows the list; a denied one that is no rule fails the
    /// stage, because dropping it would let the host through.
    #[test]
    fn a_denied_entry_that_is_no_rule_fails_and_an_allowed_one_is_dropped() {
        let kept = list_for(&names(&["*", "mine.example"]), &[], false, &[]).unwrap();
        assert!(kept.decide("mine.example").is_allowed());
        assert!(!kept.decide("anything.example").is_allowed());
        let refused = list_for(&[], &names(&["a.*.com"]), false, &[]).unwrap_err();
        assert!(refused.contains("sandbox.network.deniedHosts"), "{refused}");
    }

    /// The same list is one proxy and a different list is another, so a stage is never handed a
    /// port that decides with someone else's list.
    #[test]
    fn a_list_is_served_by_one_proxy_and_another_list_by_another() {
        let one = list_for(&names(&["one.example"]), &[], false, &[]).unwrap();
        let again = list_for(&names(&["one.example"]), &[], false, &[]).unwrap();
        let two = list_for(&names(&["two.example"]), &[], false, &[]).unwrap();
        let first = proxy_for(&one).unwrap();
        assert_eq!(first.addr(), proxy_for(&again).unwrap().addr());
        assert_ne!(first.addr(), proxy_for(&two).unwrap().addr());
    }
}
