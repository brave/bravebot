//! Whether the programs `run` starts may reach the network, and who decided.
//!
//! Four things can answer, strongest first: a file the administrator pinned ([`Managed`]), the flag
//! on this invocation, the settings layers ([`Settings::run_network`], which already keeps a
//! checkout from opening what a person closed), and the default, which is `open` so nothing that
//! worked before stops working. A pin is final: it outranks the flag, since that is what makes it
//! a pin.

use crate::{Managed, Settings};
use bravebot_sandbox::network::Network;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Who decided [`RunNetwork::network`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decided {
    /// Nobody did.
    Default,
    /// The flag on this invocation.
    Flag,
    /// A settings file, where the layers could name one.
    Settings(Option<PathBuf>),
    /// The administrator's file.
    Managed(Option<PathBuf>),
}

/// What the programs `run` starts are allowed of the network in this session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunNetwork {
    pub network: Network,
    pub decided: Decided,
}

/// The answer for a flag, the settings layers and the managed layer, none of them read from the
/// machine here, so every ordering rule is checked without one in force.
pub fn resolve(flag: Option<Network>, settings: &Settings, managed: &Managed) -> RunNetwork {
    // A pin the administrator wrote and mistyped is read as the stricter word: it was written to
    // restrict, and a typo that opened the network would do so for every user without a word.
    if let Some(network) = managed
        .network()
        .or_else(|| managed.network_unreadable().then_some(Network::Closed))
    {
        return RunNetwork {
            network,
            decided: Decided::Managed(managed.path().map(PathBuf::from)),
        };
    }
    if let Some(network) = flag {
        return RunNetwork {
            network,
            decided: Decided::Flag,
        };
    }
    if let Some(network) = settings.run_network() {
        return RunNetwork {
            network,
            decided: Decided::Settings(settings.run_network_by().map(PathBuf::from)),
        };
    }
    RunNetwork {
        network: Network::Open,
        decided: Decided::Default,
    }
}

static SETTLED: OnceLock<RunNetwork> = OnceLock::new();

/// Read the layers on this machine and settle the answer for the rest of this process.
///
/// Called once, from the entry point, after the settings file the command line named has been
/// registered and before a session is assembled. First call wins, for the reason
/// [`crate::name_a_settings_file`] does: half the process would otherwise have read the other
/// answer.
pub fn settle_run_network(flag: Option<Network>) -> &'static RunNetwork {
    SETTLED.get_or_init(|| resolve(flag, &Settings::load(), &Managed::load()))
}

/// The answer [`settle_run_network`] settled, or `open` where nothing did, which is every test that
/// does not start from the entry point.
pub fn run_network() -> Network {
    SETTLED
        .get()
        .map_or(Network::Open, |settled| settled.network)
}

/// The settled answer with who decided it, for a report.
pub fn settled_run_network() -> Option<&'static RunNetwork> {
    SETTLED.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(text: &str) -> Settings {
        Settings::parse(text)
    }

    fn managed(name: &str, text: &str) -> Managed {
        crate::managed::scratch(name, text)
    }

    #[test]
    fn nobody_deciding_leaves_the_network_open() {
        let answer = resolve(None, &Settings::default(), &Managed::default());
        assert_eq!(answer.network, Network::Open);
        assert_eq!(answer.decided, Decided::Default);
    }

    #[test]
    fn the_flag_beats_the_settings() {
        let answer = resolve(
            Some(Network::Open),
            &settings(r#"{"run": {"network": "closed"}}"#),
            &Managed::default(),
        );
        assert_eq!(answer.network, Network::Open);
        assert_eq!(answer.decided, Decided::Flag);
        let answer = resolve(
            None,
            &settings(r#"{"run": {"network": "closed"}}"#),
            &Managed::default(),
        );
        assert_eq!(answer.network, Network::Closed);
        assert!(matches!(answer.decided, Decided::Settings(_)));
    }

    #[test]
    fn a_managed_pin_beats_the_flag_and_the_settings() {
        let pin = managed("run-network-pin", r#"{"run": {"network": "closed"}}"#);
        let answer = resolve(
            Some(Network::Open),
            &settings(r#"{"run": {"network": "open"}}"#),
            &pin,
        );
        assert_eq!(answer.network, Network::Closed);
        assert!(matches!(answer.decided, Decided::Managed(_)));
    }

    #[test]
    fn a_managed_pin_that_is_neither_word_closes_the_network() {
        let typo = managed("run-network-typo", r#"{"run": {"network": "Closed"}}"#);
        let answer = resolve(
            Some(Network::Open),
            &settings(r#"{"run": {"network": "open"}}"#),
            &typo,
        );
        assert_eq!(answer.network, Network::Closed);
        assert!(matches!(answer.decided, Decided::Managed(_)));
    }
}
