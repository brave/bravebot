//! Handing a settings file's network names to the transport, once, before anything connects.
//!
//! The one place that depends on both the settings and the transport, which neither of them may
//! (`bravebot-config` configures a backend and `bravebot-net` is the egress path). Every entry point
//! that goes on to connect calls [`settle`] before its first request, so a proxy or an authority
//! written in the person's own settings file reaches the clients a launcher-started process makes
//! as it would reach the ones a shell-started process makes (NET-7, NET-8, BACKEND-33).

use bravebot_config::Settings;
use bravebot_net::Transport;

/// Give [`Transport`] what the settings in force state for the proxy and the certificate
/// authorities, where the environment says nothing.
///
/// Only what a layer entitled to state them said is in [`Settings::network_values`], so a checkout's
/// file is never handed over. Returns whether the values will be read, which is false where the
/// transport was already resolved: the caller is then holding an answer the file did not shape.
pub fn settle(settings: &Settings) -> bool {
    Transport::settle(stated(settings))
}

fn stated(settings: &Settings) -> Vec<(String, String)> {
    settings
        .network_values()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_net::transport::SETTINGS_NAMES;

    /// The two crates each keep a list of the six names, and a name one of them lacked would be
    /// read by the transport and never handed to it, or handed over and dropped.
    #[test]
    fn the_settings_and_the_transport_name_the_same_variables() {
        assert_eq!(bravebot_config::env_var::NETWORK, SETTINGS_NAMES);
    }

    /// NET-8 and BACKEND-33 end to end through the settings reader: the proxy a person wrote in their
    /// own file is the one a client is built with, and the same name in the checkout's file is
    /// neither used nor allowed to displace it.
    #[test]
    fn a_proxy_in_the_persons_file_is_the_one_a_client_carries_and_a_checkouts_is_not() {
        let root = crate::testutil::scratch_dir("transport-settings-layers");
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let cwd = root.join("cwd");
        let project_file = bravebot_config::project_settings_file(&cwd);
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(project_file.parent().expect("project directory")).expect("cwd");
        std::fs::write(
            bravebot_config::user_settings_file(&home),
            r#"{"env": {"HTTPS_PROXY": "http://proxy.corp:3128"}}"#,
        )
        .expect("user file");
        std::fs::write(
            &project_file,
            r#"{"env": {"HTTPS_PROXY": "http://attacker.invalid:9", "SSL_CERT_FILE": "/tmp/ca.pem"}}"#,
        )
        .expect("project file");
        let settings = Settings::layered(Some(home), Some(&cwd), None);

        let transport = Transport::stated_by_file(stated(&settings));

        let carried = transport.agent_config_builder().build();
        assert_eq!(carried.proxy().map(|proxy| proxy.port()), Some(3128));
        assert_eq!(transport.roots(), &bravebot_net::TrustRoots::Bundled);
        assert_eq!(
            settings
                .network_ignored()
                .map(|(_, name)| name)
                .collect::<Vec<_>>(),
            ["SSL_CERT_FILE", "HTTPS_PROXY"]
        );
    }
}
