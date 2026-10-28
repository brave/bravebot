//! Presenting the premium key on a turn's requests.
//!
//! The adapter between the configuration and the chat client. The key lives in the configuration
//! ([`bravebot_config::Config::premium_key`]) and the client asks for it once per request, so that
//! the caller that holds it can count each request against a session's `limit` in credits
//! ([TURN-8](../../../docs/specs/turns.md#TURN-8)).
//!
//! A key is only ever handed to the client for a premium host that is one of Brave's. A premium
//! endpoint that is not, a local model server or a private deployment, is somebody's own choice of
//! where requests go, and no key belongs near it.

use bravebot_aichat::{Subscription, SubscriptionCredential};
use bravebot_config::{Config, Secret};

/// The premium key, ready to be presented on each premium request.
///
/// No `Debug`, deliberately. It holds a live key, and the obvious debugging reflex of printing this
/// would put it in a log. The same reasoning as [`bravebot_aichat::SubscriptionCredential`], which
/// redacts for it.
pub struct PremiumKey {
    key: Secret,
}

impl PremiumKey {
    /// The key this configuration holds, where the premium host it goes with is one of Brave's.
    ///
    /// `None` where there is no key, which is the ordinary case for a build that was not given one,
    /// and where the premium host is not Brave's. The second is a destination somebody chose, so it
    /// is no fault and not worth a line on every run.
    pub fn held(config: &Config) -> Option<Self> {
        let key = config.premium_key.as_ref()?;
        is_a_brave_endpoint(config.premium_endpoint.as_deref()?).then(|| Self { key: key.clone() })
    }
}

impl Subscription for PremiumKey {
    fn next_credential(&mut self) -> Result<SubscriptionCredential, String> {
        Ok(SubscriptionCredential {
            key: self.key.clone(),
        })
    }
}

/// Whether an aichat endpoint is one of Brave's own deployments.
///
/// This decides where the premium key may be sent, so the host it reads is the one the transport
/// connects to, which ends at the first `/`, `?` or `#`, with any userinfo and port left out. A
/// suffix test against anything longer reads `ai-chat.bsg.brave.com:443` as somebody else's host,
/// and reads `attacker.example#.brave.com` as Brave's while the request goes to `attacker.example`.
///
/// False for a host in none of the three suffixes, which is somebody's own deployment: a local
/// service, a private one, a proxy in front of either. Nobody is handed a configuration pointing at
/// one, so it is a destination that was chosen.
pub fn is_a_brave_endpoint(endpoint: &str) -> bool {
    let Some(host) = bravebot_core::url::host_of(endpoint) else {
        return false;
    };
    [".brave.com", ".brave.software", ".bravesoftware.com"]
        .iter()
        .any(|suffix| host.ends_with(suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A configuration whose premium host is `premium` where one is named and the compiled-in one
    /// where not, with `key` stored as `bravebot auth login brave` leaves it.
    fn configured(premium: Option<&str>, key: Option<&str>) -> Config {
        let mut config = Config::from_lookup(|name| match name {
            "SERVICES_KEY_AICHAT" => Some("test-signing-key".into()),
            "BRAVE_SERVICES_KEY_ID" => Some("test-key-id".into()),
            "BRAVE_AI_CHAT_ENDPOINT" => Some("https://ai-chat.bsg.brave.software".into()),
            "BRAVE_AI_CHAT_PREMIUM_ENDPOINT" => premium.map(str::to_string),
            _ => None,
        })
        .expect("a configuration");
        config.premium_key = key.map(bravebot_config::Secret::new);
        config
    }

    /// PREM-12: the key a configuration holds is the one presented, every time it is asked for. A
    /// source that spent it, as the single-use credential it replaces was, would fail the second
    /// request of a turn.
    #[test]
    fn the_key_is_presented_on_every_request() {
        let config = configured(
            Some("https://ai-chat-premium.bsg.brave.software"),
            Some("the-premium-key"),
        );
        let mut held = PremiumKey::held(&config).expect("a key to present");
        for _ in 0..3 {
            let credential = held.next_credential().expect("a key");
            assert_eq!(credential.key.expose(), "the-premium-key");
        }
    }

    /// PREM-2: a key is presented only to a premium host that is Brave's. With none stored there is
    /// nothing to present, the host compiled in is Brave's, and a host somebody named in its place
    /// that is not Brave's is sent nothing.
    #[test]
    fn a_key_is_presented_only_to_a_premium_host_that_is_braves() {
        assert!(
            PremiumKey::held(&configured(None, None)).is_none(),
            "no key"
        );
        assert!(
            PremiumKey::held(&configured(
                Some("https://ai-chat-premium.bsg.brave.software"),
                None
            ))
            .is_none(),
            "a host with no key"
        );
        assert!(
            PremiumKey::held(&configured(None, Some("the-premium-key"))).is_some(),
            "the host compiled in is not Brave's"
        );
        for theirs in ["http://127.0.0.1:8080", "https://premium.example"] {
            assert!(
                PremiumKey::held(&configured(Some(theirs), Some("the-premium-key"))).is_none(),
                "{theirs} was sent the key"
            );
        }

        // A value that a transport would split two ways is not used by the configuration, so the host
        // compiled in stands and it is that host the key goes to (PREM-11).
        let split = configured(
            Some("https://attacker.example#.brave.com"),
            Some("the-premium-key"),
        );
        assert!(PremiumKey::held(&split).is_some());
        assert_eq!(
            split.premium_endpoint.as_deref(),
            Some(bravebot_config::DEV_PREMIUM_ENDPOINT)
        );
    }

    /// A turn whose model is served by another backend is not handed the key.
    ///
    /// The model decides which service answers, and only Brave's endpoint has a notion of a premium
    /// key at all. Bedrock and a gateway were set up with credentials of their own, and the key is
    /// not one they would accept, so presenting it there is sending it somewhere it does not belong.
    /// The Brave half is asserted too, since a gate that handed out nothing would pass the other.
    #[test]
    fn a_turn_on_another_backend_is_not_handed_the_premium_key() {
        let mut config = Config::from_lookup(|name| {
            match name {
                "SERVICES_KEY_AICHAT" => Some("test-signing-key"),
                "BRAVE_SERVICES_KEY_ID" => Some("test-key-id"),
                "BRAVE_AI_CHAT_ENDPOINT" => Some("https://ai-chat.bsg.brave.software"),
                "BRAVEBOT_USE_BEDROCK" => Some("1"),
                "AWS_REGION" => Some("us-west-2"),
                "ANTHROPIC_DEFAULT_OPUS_MODEL" => Some("opus-arn"),
                _ => None,
            }
            .map(str::to_string)
        })
        .expect("a configuration");
        config.premium_key = Some(bravebot_config::Secret::new("the-premium-key"));
        let egress = bravebot_net::Egress::new();

        assert!(
            crate::turn::discover_subscription(
                &config,
                &egress,
                Some(bravebot_config::DEFAULT_MODEL)
            )
            .is_some(),
            "a Brave turn was not handed the key"
        );
        assert!(
            crate::turn::discover_subscription(&config, &egress, Some("opus-arn")).is_none(),
            "a Bedrock turn was handed the premium key"
        );
    }

    /// A port is not part of a host's name, so a deployment written with one is the same
    /// deployment. Read against the whole authority, every Brave host with a port in it came back
    /// as somebody else's, which is a key withheld from the host it was issued for and a
    /// configuration that names Brave's own endpoint read as a service the person chose.
    #[test]
    fn a_port_is_not_part_of_the_host() {
        assert!(is_a_brave_endpoint("https://ai-chat.bsg.brave.com:443"));
        assert!(is_a_brave_endpoint(
            "https://ai-chat.bsg.brave.software:8443/v1"
        ));

        // And nothing is cut off an authority whose last colon starts no port.
        assert!(!is_a_brave_endpoint("https://example.invalid:"));
        assert!(!is_a_brave_endpoint("http://[::1]:8080"));
    }

    /// PREM-2: the host a key is sent to is the one the transport connects to, which ends at the
    /// first `/`, `?` or `#`. Read to the first `/` alone, `attacker.example#.brave.com` ends with a
    /// Brave suffix while the request goes to `attacker.example`, so the suffix test would pass for
    /// a host that is not Brave's.
    #[test]
    fn a_suffix_after_a_hash_or_a_question_mark_does_not_make_a_host_braves() {
        for theirs in [
            "https://attacker.example#.brave.com",
            "https://attacker.example?.brave.com",
            "https://attacker.example:8443#.brave.com",
            "https://attacker.example:8443?.brave.com",
            "https://attacker.example#.brave.com/v1",
            "https://attacker.example?.brave.com/v1",
            "https://attacker.example#.brave.software",
            "https://attacker.example?.bravesoftware.com",
        ] {
            assert!(!is_a_brave_endpoint(theirs), "{theirs}");
        }
    }

    /// Userinfo is not the host either: the host is what follows the last `@`, so a Brave name in
    /// front of it does not make the endpoint Brave's, and one after it does.
    #[test]
    fn a_brave_name_in_the_userinfo_does_not_make_a_host_braves() {
        assert!(!is_a_brave_endpoint(
            "https://ai-chat.bsg.brave.com@attacker.example"
        ));
        assert!(is_a_brave_endpoint(
            "https://attacker.example@ai-chat.bsg.brave.com"
        ));
    }

    /// Brave's own deployments are Brave's, whichever channel they are, and everything else is a
    /// host somebody chose. A local service read as Brave's would be refused as having no service
    /// configured, which is every development build.
    #[test]
    fn braves_own_deployments_are_told_from_a_host_somebody_chose() {
        for braves in [
            "https://ai-chat.bsg.brave.com",
            "https://ai-chat-premium.bsg.brave.com/v1/chat/completions",
            "https://ai-chat.bsg.bravesoftware.com",
            "https://ai-chat.bsg.brave.software",
        ] {
            assert!(is_a_brave_endpoint(braves), "{braves}");
        }
        for theirs in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:8000",
            "https://ai.example.internal/v1",
            "https://notbrave.com",
        ] {
            assert!(!is_a_brave_endpoint(theirs), "{theirs}");
        }
    }
}
