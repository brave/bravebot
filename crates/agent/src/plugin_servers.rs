//! The MCP servers an installed plugin carries, as aliases a session resolves (PLUGIN-7).
//!
//! A plugin's servers file is read from its installed directory as `mcp.json` is, with
//! `${PLUGIN_ROOT}` already replaced ([`crate::plugins::installed_servers`]). Each server is then
//! reached under `<plugin>_<alias>` and goes through the same road as a server added by
//! `bravebot mcp add`: a request names it, the first-use question names its digest, and only a
//! person's yes approves it ([`crate::servers`]).
//!
//! # What the plugin's files decide
//!
//! Nothing but the declaration that is listed and asked about. The driver hashes the plugin's tree
//! and compares it with the digest the install recorded ([`may_start`]) only to refuse. No byte of
//! a plugin's files chooses whether a server is asked about or approved.
//!
//! # The alias
//!
//! An alias holds letters, digits, `-` and `_` and starts with a letter or a digit, and a plugin's
//! name holds letters, digits and `-`. So the first `_` of `<plugin>_<alias>` ends the plugin's
//! name and no two servers share a name: `a-b_c` is plugin `a-b`'s `c`, and `a_b_c` is plugin `a`'s
//! `b_c`. A name `:`, `/` or `.` would have been plainer to read, and none of them is an alias.
//! Where the person's own `mcp.json` declares the same alias the person's declaration is the one
//! that is read: a plugin cannot take the place of a server the person wrote.

use crate::plugins;
use bravebot_config::mcp::{self, Declaration, Digest, Problem};
use bravebot_config::plugin;
use std::path::{Path, PathBuf};

/// The character between a plugin's name and the alias its servers file gave a server.
pub const SEPARATOR: char = '_';

/// The alias a session reaches the server `alias` of the plugin `plugin` under.
pub fn alias_of(plugin: &str, alias: &str) -> String {
    format!("{plugin}{SEPARATOR}{alias}")
}

/// Whether a session can reach the server `alias` of the plugin `plugin`: the joined alias is one
/// [`mcp::is_alias`] accepts, which includes its length. A server that fails it is listed and never
/// started.
pub fn is_reachable(plugin: &str, alias: &str) -> bool {
    mcp::is_alias(&alias_of(plugin, alias))
}

/// One server of an enabled plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    /// The alias a session reaches it under.
    pub alias: String,
    /// The plugin that carries it.
    pub plugin: String,
    /// The alias the plugin's own file gave it, which the approval records beside the plugin.
    pub named: String,
    /// What it states, with `${PLUGIN_ROOT}` replaced in its command and arguments, or what is
    /// wrong with the entry.
    pub declaration: Result<Declaration, Problem>,
    install_dir: PathBuf,
    /// The digest of the plugin's tree the install recorded.
    tree: String,
}

impl Server {
    /// What the first-use question and the approval are about: the declaration as it now stands,
    /// with the plugin's name and the directory it is installed in (PLUGIN-13).
    pub fn digest(&self) -> Option<Digest> {
        let declaration = self.declaration.as_ref().ok()?;
        Some(declaration.plugin_digest(
            &self.plugin,
            &self.install_dir.to_string_lossy(),
            &self.tree,
        ))
    }
}

/// What a requested alias resolves to among the plugins.
#[derive(Debug)]
pub enum Listed<'a> {
    /// A server of an enabled plugin.
    Server(&'a Server),
    /// The alias begins with the name of an installed plugin that is turned off, so nothing of it
    /// was read.
    Disabled(&'a str),
}

/// The servers of every enabled installed plugin, read once.
#[derive(Debug, Default)]
pub struct PluginServers {
    servers: Vec<Server>,
    disabled: Vec<String>,
}

impl PluginServers {
    /// The servers of each enabled plugin under the state directory `home`, in plugin name order.
    ///
    /// None in a safe session, which reads no declaration, and for a plugin whose servers file
    /// cannot be read. A plugin that is turned off is named and nothing of it is read (PLUGIN-11).
    pub fn read(home: &Path) -> Self {
        let mut read = Self::default();
        if bravebot_core::safe::engaged() {
            return read;
        }
        for installed in plugins::list(home) {
            if !installed.enabled {
                read.disabled.push(installed.name);
                continue;
            }
            let Some(Ok(list)) = plugins::installed_servers(home, &installed.name) else {
                continue;
            };
            for entry in list.entries {
                let alias = alias_of(&installed.name, &entry.alias);
                let declaration = match is_reachable(&installed.name, &entry.alias) {
                    true => entry.declaration,
                    false => Err(Problem::Alias),
                };
                read.servers.push(Server {
                    alias,
                    plugin: installed.name.clone(),
                    named: entry.alias,
                    declaration,
                    install_dir: plugins::installed_dir(home, &installed.name),
                    tree: installed.digest.clone(),
                });
            }
        }
        read
    }

    /// What a session asks for from the installed, enabled plugins: each server they carry, once,
    /// with the plugin's manifest as the file that asked, since installing and enabling the plugin
    /// is the person's own request for what its listing showed (PLUGIN-4, PLUGIN-11).
    ///
    /// A checkout cannot add to this: only the state directory is read. An alias a settings file
    /// also requests is requested once, under the settings file.
    pub fn requests(home: &Path, already: &[(PathBuf, String)]) -> Vec<(PathBuf, String)> {
        Self::read(home)
            .servers
            .into_iter()
            .filter(|server| already.iter().all(|(_, alias)| *alias != server.alias))
            .map(|server| (server.install_dir.join(plugin::FILE), server.alias))
            .collect()
    }

    /// What `alias` is among the plugins' servers, if anything.
    pub fn find(&self, alias: &str) -> Option<Listed<'_>> {
        if let Some(server) = self.servers.iter().find(|server| server.alias == alias) {
            return Some(Listed::Server(server));
        }
        let (plugin, _) = alias.split_once(SEPARATOR)?;
        self.disabled
            .iter()
            .find(|name| *name == plugin)
            .map(|name| Listed::Disabled(name))
    }

    /// Every server read, for a listing.
    pub fn servers(&self) -> &[Server] {
        &self.servers
    }
}

/// Why a plugin's server is not started now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// The plugin is turned off. Its recorded answers stay (PLUGIN-11).
    Disabled,
    /// Its directory does not hash to the digest the install recorded, cannot be hashed, or has no
    /// record to be held to (PLUGIN-7).
    Changed,
}

/// Whether the plugin called `plugin` may have a server started now: it is enabled and its
/// directory, hashed again, is the one the install recorded.
///
/// Asked before a server is put to the person and again just before it starts, so a file changed
/// while the question waited is seen. The answer is used only to refuse.
pub fn may_start(home: &Path, plugin: &str) -> Result<(), Stop> {
    let Some(record) = plugins::read_record(home, plugin) else {
        return Err(Stop::Changed);
    };
    if !record.enabled {
        return Err(Stop::Disabled);
    }
    plugins::tree_matches_record(home, plugin).map_err(|_| Stop::Changed)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testutil::scratch_dir;

    const SERVERS: &str = r#"{"servers":{"weather":{"transport":"stdio","argv":["${PLUGIN_ROOT}/server.sh","--root","${PLUGIN_ROOT}"],"env":{"KEY":"${PLUGIN_ROOT}/key"}},"b_c":{"transport":"http","url":"http://127.0.0.1:1/mcp"}}}"#;

    /// Installs a plugin called `name` whose servers file is `servers`, into `<scratch>/<home>`.
    pub(crate) fn install(scratch: &Path, home: &str, name: &str, servers: &str) -> PathBuf {
        let source = scratch.join(format!("src-{name}"));
        std::fs::create_dir_all(&source).expect("source");
        std::fs::write(
            source.join("plugin.toml"),
            format!(
                "name = \"{name}\"\nversion = \"1.0.0\"\ndescription = \"A demo\"\nservers = \"servers.json\"\n"
            ),
        )
        .expect("manifest");
        std::fs::write(source.join("servers.json"), servers).expect("servers");
        std::fs::write(source.join("server.sh"), "#!/bin/sh\n").expect("script");
        let home = scratch.join(home);
        plugins::install(&source, &home).expect("install");
        home
    }

    fn fresh(name: &str) -> PathBuf {
        let root = scratch_dir(name);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("scratch");
        root
    }

    /// A server is reached as `<plugin>_<alias>`, and the first `_` ends the plugin's name because
    /// a plugin's name holds no `_` and an alias starts with a letter or a digit.
    ///
    /// Rejects a separator that is also a character of a plugin's name, which would let two
    /// plugins' servers share one alias.
    #[test]
    fn a_server_is_reached_as_its_plugin_and_its_alias_joined_by_an_underscore() {
        let root = fresh("plugin-servers-alias");
        let home = install(&root, "home", "demo", SERVERS);

        let read = PluginServers::read(&home);

        let aliases: Vec<&str> = read.servers().iter().map(|s| s.alias.as_str()).collect();
        assert_eq!(aliases, ["demo_b_c", "demo_weather"]);
        let Some(Listed::Server(first)) = read.find("demo_b_c") else {
            panic!("the server whose own alias holds an underscore is listed");
        };
        assert_eq!(
            (first.plugin.as_str(), first.named.as_str()),
            ("demo", "b_c")
        );
        assert!(
            read.find("weather").is_none(),
            "the plugin's own alias is not one"
        );
        assert!(read.servers().iter().all(|s| mcp::is_alias(&s.alias)));
    }

    /// The digest the approval is keyed on is over the declaration with `${PLUGIN_ROOT}` replaced,
    /// and with the plugin and the directory it is installed in.
    ///
    /// Rejects a digest taken over the unexpanded text, and one that leaves out the install
    /// directory or the plugin's name.
    #[test]
    fn the_digest_is_over_the_expanded_declaration_and_follows_the_plugin_and_its_directory() {
        let root = fresh("plugin-servers-digest");
        let home = install(&root, "home", "demo", SERVERS);
        let elsewhere = install(&root, "elsewhere", "demo", SERVERS);
        let other = install(&root, "other-home", "other", SERVERS);
        let dir = plugins::installed_dir(&home, "demo");
        let dir = dir.to_string_lossy().to_string();

        let digest_in = |home: &Path| {
            let read = PluginServers::read(home);
            let Some(Listed::Server(server)) =
                read.find(&format!("{}_weather", plugins::list(home)[0].name))
            else {
                panic!("listed");
            };
            server.digest().expect("a digest")
        };

        let expanded = Declaration::stdio(
            vec![format!("{dir}/server.sh"), "--root".into(), dir.clone()],
            Vec::new(),
            None,
        )
        .expect("declaration")
        .storing([("KEY".to_string(), "${PLUGIN_ROOT}/key".to_string())].into())
        .expect("env");
        let tree = plugins::read_record(&home, "demo").expect("record").digest;
        assert_eq!(
            digest_in(&home),
            expanded.plugin_digest("demo", &dir, &tree)
        );
        assert_ne!(digest_in(&home), expanded.digest(), "not a home digest");
        assert_ne!(digest_in(&home), digest_in(&elsewhere), "another directory");
        assert_ne!(digest_in(&home), digest_in(&other), "another plugin");
    }

    /// A plugin that is turned off has nothing read, and its servers are named as turned off.
    ///
    /// Rejects reading a disabled plugin's servers file and offering what it holds.
    #[test]
    fn a_plugin_that_is_turned_off_offers_no_server_and_is_named() {
        let root = fresh("plugin-servers-disabled");
        let home = install(&root, "home", "demo", SERVERS);
        plugins::set_enabled(&home, "demo", false).expect("disable");

        let read = PluginServers::read(&home);

        assert!(read.servers().is_empty());
        assert!(matches!(
            read.find("demo_weather"),
            Some(Listed::Disabled("demo"))
        ));
        assert!(read.find("other_weather").is_none());
    }

    /// An alias that would be longer than an alias may be is an entry that cannot be used.
    #[test]
    fn a_joined_alias_that_is_too_long_is_not_used() {
        let root = fresh("plugin-servers-long");
        let long = "x".repeat(60);
        let servers = format!(
            r#"{{"servers":{{"{long}":{{"transport":"http","url":"http://127.0.0.1:1/mcp"}}}}}}"#
        );
        let home = install(&root, "home", "demo", &servers);

        let read = PluginServers::read(&home);

        let [server] = read.servers() else {
            panic!("one server listed");
        };
        assert_eq!(server.declaration, Err(Problem::Alias));
    }

    /// Hashing again refuses a tree that is not the recorded one in every way it can differ, and a
    /// plugin turned off.
    #[test]
    fn a_server_may_start_only_while_the_plugin_is_enabled_and_its_tree_is_the_recorded_one() {
        let root = fresh("plugin-servers-may-start");
        let home = install(&root, "home", "demo", SERVERS);
        assert_eq!(may_start(&home, "demo"), Ok(()));

        std::fs::write(
            plugins::installed_dir(&home, "demo").join("server.sh"),
            "#!/bin/sh\nx\n",
        )
        .expect("edit");
        assert_eq!(may_start(&home, "demo"), Err(Stop::Changed));
        assert_eq!(may_start(&home, "nothing"), Err(Stop::Changed));

        let other = install(&root, "second", "demo", SERVERS);
        plugins::set_enabled(&other, "demo", false).expect("disable");
        assert_eq!(may_start(&other, "demo"), Err(Stop::Disabled));
    }
}
