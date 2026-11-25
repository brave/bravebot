//! The install listing and the update difference, as JSON the window draws (PLUGIN-4, PLUGIN-9,
//! PLUGIN-13).
//!
//! A projection of what `bravebot_agent::plugins` staged and read as data, and nothing more. It
//! decides nothing: it puts no question, answers none and starts nothing. The rows are the ones
//! `crates/tui/src/confirm/plugin.rs` and `confirm/plugin/update.rs` draw, so the two front ends
//! list the same parts.
//!
//! Every string the plugin wrote sits under a key named `untrusted`, as a string, a list or an
//! object, and the window draws everything under such a key inside the margin that marks untrusted
//! content, with control and invisible characters written out. Nothing outside an `untrusted` key
//! is the plugin's text: those fields are counts, booleans, the kind of a change, a definition's
//! kind as this build parsed it, a fault in this build's words, the tree digest, and `shadowedBy`,
//! which names another source's origin and is drawn behind the margin as the terminal draws it.
//! Text is carried as read, never cut, so every word of every argv reaches the window.

use bravebot_agent::plugin_servers;
use bravebot_agent::plugin_source::Source;
use bravebot_agent::plugins::{self, Before, Change, Installed, Offer, Offered, Staged};
use bravebot_agent::skills::Catalogue;
use bravebot_config::hooks::{Hook, Unkept};
use bravebot_config::mcp::Declaration;
use bravebot_config::plugin::{FileFault, HookList, ServerList};
use bravebot_core::delegate::Definitions;
use serde_json::{Value, json};

/// The listing the install question puts: the plugin's name, version and source, the ref and full
/// commit of a git source, every skill and definition with what shadows it, every hook and server,
/// and the paths the staging passed over.
///
/// `catalogue` and `definitions` are what discovery found for the session the shadow marks are
/// checked against.
pub fn install(staged: &Staged, catalogue: &Catalogue, definitions: &Definitions) -> Value {
    let (skills, kinds) = offer_rows(&plugins::staged_offers(staged), catalogue, definitions);
    let (requested_ref, commit) = git_of(&staged.source);
    json!({
        "git": commit.is_some(),
        "untrusted": {
            "name": staged.manifest.name,
            "version": staged.manifest.version,
            "source": staged.source.to_string(),
            "requestedRef": requested_ref,
            "commit": commit,
            "skipped": staged
                .skipped
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        },
        "skills": skills,
        "definitions": kinds,
        "hooks": hooks_of(staged.hooks.as_ref()),
        "servers": servers_of(&staged.manifest.name, staged.servers.as_ref()),
    })
}

/// The difference the update question puts: the installed record, what the installed copy held as
/// `before` read it, each part added, removed or changed, and every file that differs.
pub fn update(
    staged: &Staged,
    installed: &Installed,
    before: &Before,
    catalogue: &Catalogue,
    definitions: &Definitions,
) -> Value {
    let (old_skills, old_kinds) = offer_rows(&before.offers, catalogue, definitions);
    let (new_skills, new_kinds) =
        offer_rows(&plugins::staged_offers(staged), catalogue, definitions);
    let old_hooks = hooks_of(before.hooks.as_ref());
    let new_hooks = hooks_of(staged.hooks.as_ref());
    let old_servers = servers_of(&installed.name, before.servers.as_ref());
    let new_servers = servers_of(&staged.manifest.name, staged.servers.as_ref());
    let (requested_ref, new_commit) = git_of(&staged.source);
    let (_, installed_commit) = git_of(&installed.source);
    let git = installed_commit.is_some() && new_commit.is_some();
    json!({
        "git": git,
        "enabled": installed.enabled,
        "installedDigest": installed.digest,
        "newDigest": staged.digest,
        "untrusted": {
            "name": staged.manifest.name,
            "installedVersion": installed.version,
            "newVersion": staged.manifest.version,
            "source": staged.source.to_string(),
            "installedSource": installed.source.to_string(),
            "requestedRef": requested_ref,
            "installedCommit": installed_commit,
            "newCommit": new_commit,
            "skipped": staged
                .skipped
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        },
        "before": {
            "readable": before.readable,
            "skills": old_skills,
            "definitions": old_kinds,
            "hooks": old_hooks,
            "servers": old_servers,
        },
        "skills": deltas(&old_skills, &new_skills, offer_key),
        "definitions": deltas(&old_kinds, &new_kinds, offer_key),
        "hooks": part_deltas(&old_hooks, &new_hooks, hook_key),
        "servers": part_deltas(&old_servers, &new_servers, server_key),
        "files": plugins::file_changes(staged, before).map(|files| {
            files
                .iter()
                .map(|file| {
                    json!({
                        "change": change_word(file.change),
                        // The path's bytes, decoded for display only (PATH-001 keeps the bytes).
                        "untrusted": String::from_utf8_lossy(&file.path),
                    })
                })
                .collect::<Vec<_>>()
        }),
    })
}

/// The ref the person named and the full commit of a git source, or two nulls for a local path,
/// which has no commit.
fn git_of(source: &Source) -> (Option<String>, Option<String>) {
    match source {
        Source::Local(_) => (None, None),
        Source::Git {
            requested_ref,
            commit,
            ..
        } => (requested_ref.clone(), Some(commit.to_string())),
    }
}

/// The skill rows and the definition rows of `offers`, each carrying the origin of a source above a
/// plugin that offers the same name.
fn offer_rows(
    offers: &[Offer],
    catalogue: &Catalogue,
    definitions: &Definitions,
) -> (Vec<Value>, Vec<Value>) {
    let mut skills = Vec::new();
    let mut kinds = Vec::new();
    for mark in plugins::shadow_marks(offers, catalogue, definitions) {
        match mark.offer.what {
            Offered::Skill { description } => skills.push(json!({
                "untrusted": {"name": mark.offer.name, "description": description},
                "shadowedBy": mark.by,
            })),
            Offered::Definition { kind } => kinds.push(json!({
                "kind": kind.to_string(),
                "untrusted": {"name": mark.offer.name},
                "shadowedBy": mark.by,
            })),
        }
    }
    (skills, kinds)
}

/// The hooks section, `null` where the manifest names no hooks file. A file that could not be read
/// is a `fault` with no rows, never an empty list of hooks.
fn hooks_of(read: Option<&Result<HookList, FileFault>>) -> Value {
    match read {
        None => Value::Null,
        Some(Ok(list)) => json!({
            "rows": list.hooks.declared().iter().map(hook_row).collect::<Vec<_>>(),
            "fault": Value::Null,
            "passedOver": passed_over(list.unkept),
        }),
        Some(Err(fault)) => json!({
            "rows": [],
            "fault": fault_word(*fault),
            "passedOver": passed_over(Unkept::default()),
        }),
    }
}

/// One hook, its argv as it will run: each word on its own, `${PLUGIN_ROOT}` already the installed
/// directory (PLUGIN-13).
fn hook_row(hook: &Hook) -> Value {
    json!({
        "timeoutSecs": hook.timeout().map(|bound| bound.as_secs()),
        "untrusted": {
            "moment": hook.moment().as_str(),
            "tool": hook.tool(),
            "watches": hook.path(),
            "argv": hook.run(),
        },
        "reaches": hook
            .run()
            .first()
            .map(|program| vec![reach("command", program)])
            .unwrap_or_default(),
    })
}

/// What the hook reader passed over: whole entries, and keys it does not read.
fn passed_over(unkept: Unkept) -> Value {
    json!({
        "entries": unkept.dropped,
        "keys": unkept.root_keys + unkept.entry_keys,
    })
}

/// The servers section, `null` where the manifest names no servers file. An entry that states no
/// usable declaration is a row marked never started, as is one whose `<plugin>_<alias>` is longer
/// than an alias may be.
fn servers_of(plugin: &str, read: Option<&Result<ServerList, FileFault>>) -> Value {
    match read {
        None => Value::Null,
        Some(Ok(list)) => json!({
            "rows": list
                .entries
                .iter()
                .map(|entry| match &entry.declaration {
                    Ok(declaration) => server_row(plugin, &entry.alias, declaration),
                    Err(_) => unusable_row(&entry.alias),
                })
                .collect::<Vec<_>>(),
            "fault": Value::Null,
        }),
        Some(Err(fault)) => json!({"rows": [], "fault": fault_word(*fault)}),
    }
}

fn server_row(plugin: &str, alias: &str, declaration: &Declaration) -> Value {
    let reachable = plugin_servers::is_reachable(plugin, alias);
    let mut variables: Vec<String> = declaration.variables().to_vec();
    variables.extend(declaration.bearer_env().map(str::to_string));
    variables.extend(
        declaration
            .env_http_headers()
            .map(|(header, variable)| format!("{variable} ({header})")),
    );
    let stored: Vec<String> = declaration
        .stored_values()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    json!({
        "usable": true,
        "reachable": reachable,
        "tooLong": plugin_servers::is_too_long(plugin, alias),
        "neverStarted": !reachable,
        "untrusted": {
            "alias": alias,
            "transport": declaration.transport().to_string(),
            "argv": declaration.argv(),
            "url": declaration.url(),
            "variables": variables,
            "stored": stored,
        },
        "reaches": reaches(declaration),
    })
}

/// An entry that states no declaration that can be used. Its alias is listed and nothing else.
fn unusable_row(alias: &str) -> Value {
    json!({
        "usable": false,
        "reachable": false,
        "tooLong": false,
        "neverStarted": true,
        "untrusted": {
            "alias": alias,
            "transport": "",
            "argv": [],
            "url": Value::Null,
            "variables": [],
            "stored": [],
        },
        "reaches": [],
    })
}

/// The program a declaration would start, the hosts it would reach, the files it may read and the
/// directory it runs in, one row each.
fn reaches(declaration: &Declaration) -> Vec<Value> {
    let mut found = Vec::new();
    if let Some(program) = declaration.argv().first() {
        found.push(reach("command", program));
    }
    if let Some(host) = declaration.url().and_then(bravebot_core::url::host_of) {
        found.push(reach("host", &host));
    }
    for host in declaration.allowed_hosts().unwrap_or_default() {
        found.push(reach("host", host));
    }
    for file in declaration.reads() {
        found.push(reach("file", file));
    }
    if let Some(directory) = declaration.directory() {
        found.push(reach("directory", directory));
    }
    found
}

fn reach(kind: &str, value: &str) -> Value {
    json!({"kind": kind, "untrusted": value})
}

/// Why a part file could not be read, in this build's words, never the file's.
fn fault_word(fault: FileFault) -> &'static str {
    match fault {
        FileFault::Unreadable => "unreadable",
        FileFault::NotARegularFile => "not-a-regular-file",
        FileFault::TooLarge => "too-large",
        FileFault::NotText => "not-text",
        FileFault::NotHooks => "not-hooks",
        FileFault::NotServers => "not-servers",
        FileFault::PathNotText => "path-not-text",
    }
}

fn change_word(change: Change) -> &'static str {
    match change {
        Change::Added => "added",
        Change::Removed => "removed",
        Change::Changed => "changed",
    }
}

fn offer_key(row: &Value) -> Value {
    row["untrusted"]["name"].clone()
}

fn hook_key(row: &Value) -> Value {
    let words = &row["untrusted"];
    json!([words["moment"], words["tool"], words["watches"]])
}

fn server_key(row: &Value) -> Value {
    row["untrusted"]["alias"].clone()
}

/// The rows turning `old` into `new`, as the terminal's update difference pairs them. A row equal
/// on both sides is left out. Of the rest, a row of `new` with the `key` of a row of `old` is
/// changed, one with no partner is added, and a row of `old` left over is removed. Added and changed
/// come in the order of `new`, then removed in the order of `old`.
fn deltas(old: &[Value], new: &[Value], key: impl Fn(&Value) -> Value) -> Vec<Value> {
    let mut left: Vec<Option<&Value>> = old.iter().map(Some).collect();
    let mut unmatched = Vec::new();
    for row in new {
        match left.iter().position(|held| *held == Some(row)) {
            Some(at) => left[at] = None,
            None => unmatched.push(row),
        }
    }
    let mut found = Vec::new();
    for row in unmatched {
        let partner = left
            .iter()
            .position(|held| held.is_some_and(|held| key(held) == key(row)));
        found.push(match partner.and_then(|at| left[at].take()) {
            Some(was) => json!({"change": "changed", "was": was, "now": row}),
            None => json!({"change": "added", "now": row}),
        });
    }
    found.extend(
        left.into_iter()
            .flatten()
            .map(|was| json!({"change": "removed", "was": was})),
    );
    found
}

/// The difference between two readings of a hooks or servers file, `null` where neither side names
/// one. A side that could not be read is said as its fault, so its rows are not taken for none.
fn part_deltas(old: &Value, new: &Value, key: impl Fn(&Value) -> Value) -> Value {
    if old.is_null() && new.is_null() {
        return Value::Null;
    }
    let rows = |part: &Value| part["rows"].as_array().cloned().unwrap_or_default();
    json!({
        "deltas": deltas(&rows(old), &rows(new), key),
        "installedFault": old["fault"],
        "newFault": new["fault"],
        "passedOver": new["passedOver"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::confirm::FetchRequest;
    use bravebot_agent::plugins::Asker;
    use bravebot_agent::workspace::Workspace;
    use std::path::Path;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn write_skill(root: &Path, name: &str, description: &str) {
        write(
            &root.join("skills").join(name).join("SKILL.md"),
            &format!("---\nname: {name}\ndescription: {description}\n---\n\nbody"),
        );
    }

    /// A plugin `demo` at `source` with a skill, a definition, a hooks file and a servers file,
    /// the parts the terminal's install listing test stages.
    fn plugin_at(source: &Path, version: &str, description: &str) {
        write(
            &source.join("plugin.toml"),
            &format!(
                "name = \"demo\"\nversion = \"{version}\"\ndescription = \"d\"\nskills = \"skills\"\nagents = \"agents\"\nhooks = \"h.json\"\nservers = \"s.json\"\n"
            ),
        );
        write_skill(source, "hello", description);
        write(
            &source.join("agents").join("helper.md"),
            "---\nname: helper\ndescription: d\nkind: worker\n---\n\nprompt",
        );
        write(
            &source.join("h.json"),
            r#"{"hooks":[{"on":"turn-started","run":["${PLUGIN_ROOT}/hook.sh","--flag","two words"]},{"on":"nope","run":["x"]}]}"#,
        );
        write(
            &source.join("s.json"),
            r#"{"servers":{"local":{"transport":"stdio","argv":["${PLUGIN_ROOT}/server.py","-v"],"variables":["API_KEY"],"env":{"HOME_X":"${PLUGIN_ROOT}/x"},"reads":["/etc/hosts"]},"web":{"transport":"http","url":"https://mcp.example.org/v1"},"broken":{"transport":"stdio"}}}"#,
        );
    }

    fn listed(staged: &Staged) -> Value {
        install(staged, &Catalogue::default(), &Definitions::default())
    }

    /// PLUGIN-4, PLUGIN-13: every skill, definition, hook and server is in the listing, each argv
    /// word on its own with `${PLUGIN_ROOT}` already the installed directory, every reach on its
    /// own row, and each string of the plugin under an `untrusted` key.
    ///
    /// Rejects: a part left out, an argv joined, cut or left unexpanded or with the staging path,
    /// a stored value expanded, and plugin text outside the `untrusted` keys.
    #[test]
    fn every_part_and_every_argv_word_is_listed_expanded() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = scratch.path().join("src");
        plugin_at(&source, "2.3.4", "Says hello.");
        let staged = plugins::stage(&source, &home).unwrap();
        let listing = listed(&staged);
        let root = plugins::installed_dir(&home, "demo").display().to_string();

        assert_eq!(listing["untrusted"]["name"], "demo");
        assert_eq!(listing["untrusted"]["version"], "2.3.4");
        assert_eq!(listing["untrusted"]["source"], source.display().to_string());
        assert_eq!(listing["git"], false);
        assert!(listing["untrusted"]["commit"].is_null());
        assert_eq!(
            listing["skills"],
            json!([{"untrusted": {"name": "hello", "description": "Says hello."}, "shadowedBy": null}])
        );
        assert_eq!(listing["definitions"][0]["untrusted"]["name"], "helper");
        assert_eq!(listing["definitions"][0]["kind"], "worker");

        let hooks = &listing["hooks"];
        assert!(hooks["fault"].is_null(), "{hooks}");
        assert_eq!(hooks["passedOver"]["entries"], 1);
        assert_eq!(hooks["rows"].as_array().unwrap().len(), 1);
        let hook = &hooks["rows"][0];
        assert_eq!(hook["untrusted"]["moment"], "turn-started");
        assert_eq!(
            hook["untrusted"]["argv"],
            json!([format!("{root}/hook.sh"), "--flag", "two words"])
        );
        assert_eq!(hook["reaches"][0]["untrusted"], format!("{root}/hook.sh"));

        let servers = listing["servers"]["rows"].as_array().unwrap();
        assert_eq!(servers.len(), 3);
        let by_alias = |alias: &str| {
            servers
                .iter()
                .find(|row| row["untrusted"]["alias"] == alias)
                .unwrap_or_else(|| panic!("{alias} listed"))
        };
        let local = by_alias("local");
        assert_eq!(local["untrusted"]["transport"], "stdio");
        assert_eq!(
            local["untrusted"]["argv"],
            json!([format!("{root}/server.py"), "-v"])
        );
        assert_eq!(local["untrusted"]["variables"], json!(["API_KEY"]));
        assert_eq!(
            local["untrusted"]["stored"],
            json!(["HOME_X=${PLUGIN_ROOT}/x"])
        );
        assert!(local["reaches"].as_array().unwrap().contains(&json!(
            {"kind": "file", "untrusted": "/etc/hosts"}
        )));
        assert_eq!(local["neverStarted"], false);
        let web = by_alias("web");
        assert_eq!(web["untrusted"]["url"], "https://mcp.example.org/v1");
        assert!(web["reaches"].as_array().unwrap().contains(&json!(
            {"kind": "host", "untrusted": "mcp.example.org"}
        )));
        let broken = by_alias("broken");
        assert_eq!(
            (&broken["usable"], &broken["neverStarted"]),
            (&json!(false), &json!(true))
        );

        let text = listing.to_string();
        assert!(!text.contains(".plugin-staging"), "{text}");
        assert!(!text.contains("${PLUGIN_ROOT}/hook.sh"), "{text}");
        assert!(!text.contains("${PLUGIN_ROOT}/server.py"), "{text}");
        assert_untrusted_only_under_untrusted(
            &listing,
            &[
                "demo",
                "hello",
                "Says hello.",
                "two words",
                "--flag",
                "local",
            ],
        );
    }

    /// Each of `words` is found only under an `untrusted` key of `value`.
    fn assert_untrusted_only_under_untrusted(value: &Value, words: &[&str]) {
        fn outside(value: &Value, found: &mut Vec<String>) {
            match value {
                Value::String(text) => found.push(text.clone()),
                Value::Array(items) => items.iter().for_each(|item| outside(item, found)),
                Value::Object(map) => map
                    .iter()
                    .filter(|(key, _)| key.as_str() != "untrusted")
                    .for_each(|(_, item)| outside(item, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        outside(value, &mut found);
        for word in words {
            assert!(
                !found.iter().any(|text| text == word),
                "{word} outside an untrusted key: {found:?}"
            );
        }
    }

    /// PLUGIN-4: a server whose `<plugin>_<alias>` is longer than an alias may be is listed in full
    /// and marked never started, while the same server one character shorter is not.
    ///
    /// Rejects: a listing that marks usable a server a session can never reach.
    #[test]
    fn a_joined_alias_that_is_too_long_reads_as_never_started() {
        let server = |alias: &str| {
            let scratch = tempfile::tempdir().unwrap();
            let source = scratch.path().join("src");
            write(
                &source.join("plugin.toml"),
                "name = \"demo\"\nversion = \"1\"\ndescription = \"d\"\nservers = \"s.json\"\n",
            );
            write(
                &source.join("s.json"),
                &format!(
                    r#"{{"servers":{{"{alias}":{{"transport":"http","url":"https://mcp.example.org/v1"}}}}}}"#
                ),
            );
            let staged = plugins::stage(&source, &scratch.path().join("home")).unwrap();
            listed(&staged)["servers"]["rows"][0].clone()
        };
        let long = server(&"x".repeat(60));
        assert_eq!(long["untrusted"]["url"], "https://mcp.example.org/v1");
        assert_eq!(long["usable"], true);
        assert_eq!(long["reachable"], false);
        assert_eq!(long["tooLong"], true);
        assert_eq!(long["neverStarted"], true);
        let short = server(&"x".repeat(59));
        assert_eq!(short["neverStarted"], false);
        assert_eq!(short["tooLong"], false);
    }

    /// PLUGIN-4: a hooks file that cannot be read is a fault, not a plugin with no hooks, and a
    /// manifest that names no hooks file is `null`.
    ///
    /// Rejects: an unusable file listed as an empty list with no fault.
    #[test]
    fn an_unreadable_hooks_file_is_a_fault_and_not_empty() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = scratch.path().join("src");
        write(
            &source.join("plugin.toml"),
            "name = \"demo\"\nversion = \"1\"\ndescription = \"d\"\nhooks = \"h.json\"\n",
        );
        write(&source.join("h.json"), "{ not json");
        let staged = plugins::stage(&source, &home).unwrap();
        let hooks = &listed(&staged)["hooks"];
        assert_eq!(hooks["fault"], "not-hooks", "{hooks}");
        assert_eq!(hooks["rows"], json!([]));

        write(
            &source.join("plugin.toml"),
            "name = \"demo\"\nversion = \"1\"\ndescription = \"d\"\n",
        );
        let staged = plugins::stage(&source, &home).unwrap();
        let listing = listed(&staged);
        assert!(listing["hooks"].is_null(), "{listing}");
        assert!(listing["servers"].is_null(), "{listing}");
    }

    /// PLUGIN-12: a skill a source above a plugin already offers carries that source's origin as
    /// `shadowedBy`, from the catalogue discovery resolves, and one nobody else offers carries none.
    ///
    /// Rejects: a shadow mark dropped from the listing.
    #[test]
    fn a_shadowed_skill_carries_shadowed_by() {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let work = scratch.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        let source = scratch.path().join("src");
        write(
            &source.join("plugin.toml"),
            "name = \"demo\"\nversion = \"1\"\ndescription = \"d\"\nskills = \"skills\"\n",
        );
        write_skill(&source, "hello", "from the plugin");
        write_skill(&source, "free", "nobody else has it");
        write_skill(&home, "hello", "the person's own");
        let staged = plugins::stage(&source, &home).unwrap();
        let workspace = Workspace::new(&work).unwrap();
        let catalogue = bravebot_agent::skills::resolved(
            &workspace,
            Some(&home),
            bravebot_core::trust::TrustStore::new(&work),
            bravebot_core::permissions::Permissions::default(),
            &mut bravebot_session::audit::Trail::default(),
        );
        let listing = install(&staged, &catalogue, &Definitions::default());
        let skill = |name: &str| {
            listing["skills"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["untrusted"]["name"] == name)
                .cloned()
                .unwrap_or_else(|| panic!("{name} listed\n{listing}"))
        };
        let by = skill("hello")["shadowedBy"].clone();
        assert!(
            by.as_str().is_some_and(|by| by.contains("skills")),
            "{listing}"
        );
        assert!(skill("free")["shadowedBy"].is_null(), "{listing}");
    }

    /// An asker that answers yes to an install and keeps the update listing it is shown, answering
    /// no to it.
    #[derive(Default)]
    struct Keeping {
        shown: Option<Value>,
    }

    impl Asker for Keeping {
        fn anybody_there(&self) -> bool {
            true
        }

        fn confirm_fetch_host(&mut self, _request: &FetchRequest) -> bool {
            false
        }

        fn confirm_install(&mut self, _staged: &Staged) -> bool {
            true
        }

        fn confirm_update(
            &mut self,
            staged: &Staged,
            installed: &Installed,
            before: &Before,
        ) -> bool {
            self.shown = Some(update(
                staged,
                installed,
                before,
                &Catalogue::default(),
                &Definitions::default(),
            ));
            false
        }
    }

    /// The update listing for the plugin installed from `source`, after `edit` changes it and
    /// `spoil` changes the installed copy.
    fn update_listing(edit: impl Fn(&Path), spoil: impl Fn(&Path)) -> Value {
        let scratch = tempfile::tempdir().unwrap();
        let home = scratch.path().join("home");
        let source = scratch.path().join("src");
        plugin_at(&source, "2.3.4", "Says hello.");
        let mut asker = Keeping::default();
        plugins::install_asked(&source, &home, &mut asker).unwrap();
        edit(&source);
        spoil(&plugins::installed_dir(&home, "demo"));
        assert_eq!(
            plugins::update_typed("demo", &home, &mut asker),
            Err(plugins::Refusal::Declined)
        );
        asker.shown.expect("the update question was put")
    }

    /// PLUGIN-9: the update listing pairs a skill whose description changed as one change with
    /// both sides, leaves out the parts that are equal, gives both versions and the files that
    /// differ, and where the installed manifest cannot be read says `before.readable` is false.
    ///
    /// Rejects: a change shown as a removal and an addition, an unchanged part listed, a file
    /// difference left out, and an unreadable installed copy shown as a readable empty one.
    #[test]
    fn the_update_view_pairs_a_change_and_reports_an_unreadable_before() {
        let listing = update_listing(
            |source| {
                plugin_at(source, "2.4.0", "Says hello again.");
                write(&source.join("extra.sh"), "echo");
            },
            |_| {},
        );
        assert_eq!(listing["untrusted"]["installedVersion"], "2.3.4");
        assert_eq!(listing["untrusted"]["newVersion"], "2.4.0");
        assert_eq!(listing["before"]["readable"], true);
        assert_eq!(
            listing["skills"],
            json!([{
                "change": "changed",
                "was": {"untrusted": {"name": "hello", "description": "Says hello."}, "shadowedBy": null},
                "now": {"untrusted": {"name": "hello", "description": "Says hello again."}, "shadowedBy": null},
            }])
        );
        assert_eq!(listing["definitions"], json!([]));
        assert_eq!(listing["hooks"]["deltas"], json!([]));
        assert_eq!(listing["servers"]["deltas"], json!([]));
        let files = listing["files"]
            .as_array()
            .expect("the trees were compared");
        for (path, change) in [("extra.sh", "added"), ("plugin.toml", "changed")] {
            assert!(
                files.contains(&json!({"change": change, "untrusted": path})),
                "{path} {change}\n{listing}"
            );
        }
        assert!(
            !files.iter().any(|file| file["untrusted"] == "h.json"),
            "{listing}"
        );

        let unread = update_listing(
            |source| plugin_at(source, "2.4.0", "Says hello."),
            |installed| write(&installed.join("plugin.toml"), "not a manifest ["),
        );
        assert_eq!(unread["before"]["readable"], false, "{unread}");
        assert_eq!(unread["before"]["skills"], json!([]));
        assert_eq!(unread["skills"][0]["change"], "added", "{unread}");
    }
}
