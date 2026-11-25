//! The install listing and the update difference, as JSON the window draws (PLUGIN-4, PLUGIN-9,
//! PLUGIN-13).
//!
//! A projection of what `bravebot_agent::plugins` staged and read as data, and nothing more. It
//! decides nothing: it puts no question, answers none and starts nothing. The rows and the update
//! pairing are `bravebot_agent::plugin_listing`'s, which the terminal draws from too, so the two
//! front ends list the same parts.
//!
//! Every string the plugin wrote sits under a key named `untrusted`, as a string, a list or an
//! object, and the window draws everything under such a key inside the margin that marks untrusted
//! content, with control and invisible characters written out. Nothing outside an `untrusted` key
//! is the plugin's text: those fields are counts, booleans, the kind of a change, a definition's
//! kind as this build parsed it, a fault in this build's words, the tree digest, and `shadowedBy`,
//! which names another source's origin and is drawn behind the margin as the terminal draws it.
//! Text is carried as read, never cut, so every word of every argv reaches the window.

use bravebot_agent::plugin_listing::{
    self as shared, DefinitionRow, Delta, HookRow, InstallRequest, Listed, PartDeltas, Reach,
    ServerRow, SkillRow, UpdateRequest,
};
use bravebot_agent::plugin_servers;
use bravebot_agent::plugin_source::Source;
use bravebot_agent::plugins::{Before, Change, Installed, Staged};
use bravebot_agent::skills::Catalogue;
use bravebot_config::hooks::Unkept;
use bravebot_config::plugin::FileFault;
use bravebot_core::delegate::Definitions;
use serde_json::{Value, json};

/// The listing the install question puts: the plugin's name, version and source, the ref and full
/// commit of a git source, every skill and definition with what shadows it, every hook and server,
/// and the paths the staging passed over.
///
/// `catalogue` and `definitions` are what discovery found for the session the shadow marks are
/// checked against.
pub fn install(staged: &Staged, catalogue: &Catalogue, definitions: &Definitions) -> Value {
    let request = InstallRequest::from_staged(staged, catalogue, definitions);
    let git = request.git.as_ref();
    json!({
        "git": git.is_some(),
        "untrusted": {
            "name": request.name,
            "version": request.version,
            "source": request.source,
            "requestedRef": git.and_then(|git| git.requested_ref.clone()),
            "commit": git.map(|git| git.commit.clone()),
            "skipped": skipped(staged),
        },
        "skills": request.skills.iter().map(skill_row).collect::<Vec<_>>(),
        "definitions": request.definitions.iter().map(definition_row).collect::<Vec<_>>(),
        "hooks": hooks_of(request.hooks.as_ref()),
        "servers": servers_of(&request.name, request.servers.as_ref()),
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
    let request = UpdateRequest::from_parts(staged, installed, before, catalogue, definitions);
    let (old_skills, old_kinds) = shared::offer_rows(&before.offers, catalogue, definitions);
    let old_hooks = shared::hooks_from(before.hooks.as_ref());
    let old_servers = shared::servers_from(&installed.name, before.servers.as_ref());
    let (requested_ref, new_commit) = git_of(&staged.source);
    let (_, installed_commit) = git_of(&installed.source);
    let new_name = &staged.manifest.name;
    json!({
        "git": request.git.is_some(),
        "enabled": installed.enabled,
        "installedDigest": installed.digest,
        "newDigest": staged.digest,
        "untrusted": {
            "name": request.name,
            "installedVersion": request.installed_version,
            "newVersion": request.new_version,
            "source": request.source,
            "installedSource": installed.source.to_string(),
            "requestedRef": requested_ref,
            "installedCommit": installed_commit,
            "newCommit": new_commit,
            "skipped": skipped(staged),
        },
        "before": {
            "readable": request.installed_readable,
            "skills": old_skills.iter().map(skill_row).collect::<Vec<_>>(),
            "definitions": old_kinds.iter().map(definition_row).collect::<Vec<_>>(),
            "hooks": hooks_of(old_hooks.as_ref()),
            "servers": servers_of(&installed.name, old_servers.as_ref()),
        },
        "skills": deltas_of(&request.skills, skill_row),
        "definitions": deltas_of(&request.definitions, definition_row),
        "hooks": request.hooks.as_ref().map_or(Value::Null, |part| {
            let passed = staged.hooks.as_ref().map(|_| passed_over(part.new_unkept));
            part_deltas_of(part, hook_row, passed.as_ref())
        }),
        "servers": request.servers.as_ref().map_or(Value::Null, |part| {
            // A row of the installed copy names the installed plugin, and a row of the staged tree
            // the staged one, so each is reached under its own name.
            let delta_rows = part.deltas.iter().map(|delta| match delta {
                Delta::Added(now) => json!({"change": "added", "now": server_row(new_name, now)}),
                Delta::Removed(was) => {
                    json!({"change": "removed", "was": server_row(&installed.name, was)})
                }
                Delta::Changed { was, now } => json!({
                    "change": "changed",
                    "was": server_row(&installed.name, was),
                    "now": server_row(new_name, now),
                }),
            });
            json!({
                "deltas": delta_rows.collect::<Vec<_>>(),
                "installedFault": part.installed_fault.map(fault_word),
                "newFault": part.new_fault.map(fault_word),
                "passedOver": Value::Null,
            })
        }),
        "files": request.files.as_ref().map(|files| {
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

/// The paths the staging passed over.
fn skipped(staged: &Staged) -> Vec<String> {
    staged
        .skipped
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
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

fn skill_row(row: &SkillRow) -> Value {
    json!({
        "untrusted": {"name": row.name, "description": row.description},
        "shadowedBy": row.shadowed_by,
    })
}

fn definition_row(row: &DefinitionRow) -> Value {
    json!({
        "kind": row.kind,
        "untrusted": {"name": row.name},
        "shadowedBy": row.shadowed_by,
    })
}

/// The hooks section, `null` where the manifest names no hooks file. A file that could not be read
/// is a `fault` with no rows, never an empty list of hooks.
fn hooks_of(listed: Option<&Listed<HookRow>>) -> Value {
    listed.map_or(Value::Null, |listed| {
        json!({
            "rows": listed.rows.iter().map(hook_row).collect::<Vec<_>>(),
            "fault": listed.fault.map(fault_word),
            "passedOver": passed_over(listed.unkept),
        })
    })
}

/// One hook, its argv as it will run: each word on its own, `${PLUGIN_ROOT}` already the installed
/// directory (PLUGIN-13).
fn hook_row(hook: &HookRow) -> Value {
    json!({
        "timeoutSecs": hook.timeout_secs,
        "untrusted": {
            "moment": hook.moment,
            "tool": hook.tool,
            "watches": hook.watches,
            "argv": hook.argv,
        },
        "reaches": hook
            .argv
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
fn servers_of(plugin: &str, listed: Option<&Listed<ServerRow>>) -> Value {
    listed.map_or(Value::Null, |listed| {
        json!({
            "rows": listed
                .rows
                .iter()
                .map(|row| server_row(plugin, row))
                .collect::<Vec<_>>(),
            "fault": listed.fault.map(fault_word),
        })
    })
}

/// One server of the plugin called `plugin`. An entry that states no declaration that can be used
/// lists its alias and nothing else.
fn server_row(plugin: &str, row: &ServerRow) -> Value {
    json!({
        "usable": row.usable,
        "reachable": row.reachable,
        "tooLong": row.usable && plugin_servers::is_too_long(plugin, &row.alias),
        "neverStarted": !row.reachable,
        "untrusted": {
            "alias": row.alias,
            "transport": row.transport,
            "argv": row.argv,
            "url": row.url,
            "variables": row.variables,
            "stored": row.stored,
        },
        "reaches": row.reaches.iter().map(reach_row).collect::<Vec<_>>(),
    })
}

/// The program a declaration would start, a host it would reach, a file it may read or the
/// directory it runs in.
fn reach_row(reach_of: &Reach) -> Value {
    match reach_of {
        Reach::Command(program) => reach("command", program),
        Reach::Host(host) => reach("host", host),
        Reach::File(file) => reach("file", file),
        Reach::Directory(directory) => reach("directory", directory),
    }
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

/// The deltas `bravebot_agent::plugin_listing` paired, each row drawn by `row`.
fn deltas_of<T>(deltas: &[Delta<T>], row: impl Fn(&T) -> Value) -> Vec<Value> {
    deltas
        .iter()
        .map(|delta| match delta {
            Delta::Added(now) => json!({"change": "added", "now": row(now)}),
            Delta::Removed(was) => json!({"change": "removed", "was": row(was)}),
            Delta::Changed { was, now } => {
                json!({"change": "changed", "was": row(was), "now": row(now)})
            }
        })
        .collect()
}

/// The difference between two readings of a hooks file. A side that could not be read is said as
/// its fault, so its rows are not taken for none.
fn part_deltas_of<T>(
    part: &PartDeltas<T>,
    row: impl Fn(&T) -> Value,
    passed_over: Option<&Value>,
) -> Value {
    json!({
        "deltas": deltas_of(&part.deltas, row),
        "installedFault": part.installed_fault.map(fault_word),
        "newFault": part.new_fault.map(fault_word),
        "passedOver": passed_over,
    })
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use bravebot_agent::confirm::FetchRequest;
    use bravebot_agent::plugins::{self, Asker};
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
