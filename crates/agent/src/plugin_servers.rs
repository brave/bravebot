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
//!
//! # Listing
//!
//! A listing reads a plugin's servers once with [`PluginServers::read`], names the plugins that are
//! turned off with [`PluginServers::disabled`], takes the aliases a session requests from the same
//! value with [`PluginServers::requested`], and asks what a session would decide about each server
//! with [`crate::servers::held_by_plugins`], which hashes each plugin's directory once ([`may_start`]).
//! Between them these read the records of the installed plugins, the manifest and the servers file
//! of each enabled plugin, every file of a plugin's installed directory to hash it, and the
//! approvals file. They start no server, ask no question and write nothing.

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

/// Whether `<plugin>_<alias>` is not an alias only because it is longer than an alias may be.
///
/// It is false where the alias the plugin's file gave is not an alias on its own: it holds a
/// character an alias lacks, or it is itself longer than an alias may be. That entry is unusable in
/// its own right and is told as not an alias. It is also false where the plugin's name spoils the
/// join another way, as a name starting with `-` does.
pub fn is_too_long(plugin: &str, alias: &str) -> bool {
    let joined = alias_of(plugin, alias);
    mcp::is_alias(alias)
        && !mcp::is_alias(&joined)
        && joined.starts_with(|c: char| c.is_ascii_alphanumeric())
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
    /// Whether this entry cannot be used because `<plugin>_<alias>` is longer than an alias may be,
    /// as against an alias the plugin's file wrote that is not one or a declaration that is wrong.
    pub fn joined_too_long(&self) -> bool {
        is_too_long(&self.plugin, &self.named)
    }

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
    /// It reads the records of the installed plugins, and for each enabled plugin its manifest and
    /// the servers file the manifest names, in the plugin's installed directory. It hashes no
    /// directory, reads no approvals file, starts no server, asks no question and writes nothing.
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
        Self::read(home).requested(already)
    }

    /// [`Self::requests`] from a value already read, so a listing that also names the plugins that
    /// are turned off reads the files once.
    pub fn requested(&self, already: &[(PathBuf, String)]) -> Vec<(PathBuf, String)> {
        self.servers
            .iter()
            .filter(|server| already.iter().all(|(_, alias)| *alias != server.alias))
            .map(|server| (server.install_dir.join(plugin::FILE), server.alias.clone()))
            .collect()
    }

    /// The installed plugins that are turned off, in name order. Nothing of them was read
    /// (PLUGIN-11), so no server of theirs is among [`Self::servers`].
    pub fn disabled(&self) -> &[String] {
        &self.disabled
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

    /// A joined alias is too long only where the alias the plugin wrote is an alias on its own, the
    /// plugin's name does not otherwise spoil the join, and the join is over the limit. The limit is
    /// the longest alias [`mcp::is_alias`] accepts, found here and not written down, and a joined
    /// alias of exactly that length is not too long.
    ///
    /// Rejects an `is_too_long` that is true for any alias problem, which would call an alias with a
    /// `:` in it, an alias over the limit by itself, or a plugin name starting with `-` too long, and
    /// one that is off by one at the limit, which would call a usable joined alias too long or a
    /// joined alias one character over the limit usable.
    #[test]
    fn a_joined_alias_is_too_long_only_for_a_valid_alias_that_the_join_makes_too_long() {
        let longest = (1..=1024)
            .rev()
            .find(|length| mcp::is_alias(&"x".repeat(*length)))
            .expect("some length is an alias");
        assert!(!mcp::is_alias(&"x".repeat(longest + 1)), "the limit found");
        let room = longest - alias_of("demo", "").len();
        let at_the_limit = "x".repeat(room);
        let over_by_one = "x".repeat(room + 1);
        assert_eq!(alias_of("demo", &at_the_limit).len(), longest);
        assert!(is_reachable("demo", &at_the_limit));
        assert!(!is_too_long("demo", &at_the_limit), "at the limit");
        assert_eq!(alias_of("demo", &over_by_one).len(), longest + 1);
        assert!(is_too_long("demo", &over_by_one), "one over the limit");
        assert!(!is_too_long("demo", "weather"));
        assert!(!is_too_long("demo", "a:b"), "not an alias at all");
        assert!(
            !is_too_long("demo", &"x".repeat(70)),
            "an alias the plugin wrote that is over the limit by itself is not an alias"
        );
        assert!(!is_too_long("-a", "x"), "a name starting with - spoils it");
    }

    /// The plugins turned off are named beside the servers of the others, and the aliases a session
    /// requests come from the same read.
    ///
    /// Rejects an accessor that lists an enabled plugin as turned off, and a request that differs
    /// from what `requests` gives a session.
    #[test]
    fn the_turned_off_plugins_are_named_beside_the_requests_of_the_others() {
        let root = fresh("plugin-servers-listing");
        let home = install(&root, "home", "demo", SERVERS);
        install(&root, "home", "other", SERVERS);
        plugins::set_enabled(&home, "other", false).expect("disable");

        let read = PluginServers::read(&home);

        assert_eq!(read.disabled(), ["other".to_string()]);
        let asked = read.requested(&[]);
        assert_eq!(asked, PluginServers::requests(&home, &[]));
        let aliases: Vec<&str> = asked.iter().map(|(_, alias)| alias.as_str()).collect();
        assert_eq!(aliases, ["demo_b_c", "demo_weather"]);
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
