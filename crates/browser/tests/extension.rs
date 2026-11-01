//! The extension in `extension/` held to the relay's side of the contract, by reading its files.
//!
//! The extension is JavaScript and its behaviour is tested by Node, in `extension/tests/`. What is
//! checked here is what the two halves have to agree on: the id `install` records, and the names of
//! the tools the server offers and the extension answers. The second extension, in
//! `extension-trusted/`, is held to the ids the two extensions name each other by and to the
//! permission and the listener it is allowed.

#![cfg(unix)]
#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use base64::Engine;
use bravebot_browser::install::{self, EXTENSION_ID};
use bravebot_browser::relay;
use bravebot_browser::tools::{self, TOOLS};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn extension_file(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../extension")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn extension_manifest() -> Value {
    serde_json::from_str(&extension_file("manifest.json")).unwrap()
}

/// The directory of the second extension, which holds the browser's debugger for the first.
fn trusted_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extension-trusted")
}

fn trusted_file(name: &str) -> String {
    let path = trusted_directory().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn trusted_manifest() -> Value {
    serde_json::from_str(&trusted_file("manifest.json")).unwrap()
}

/// The id a browser derives from the key of an unpacked extension's manifest: the first 16 bytes of
/// the key's SHA-256, each half-byte written as a letter from `a`.
fn id_of(key: &str) -> String {
    let der = base64::engine::general_purpose::STANDARD
        .decode(key)
        .unwrap();
    let digest = Sha256::digest(&der);
    digest[..16]
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .map(|half| char::from(b'a' + half))
        .collect()
}

/// The string of `export const <name> = "...";` in a script.
fn exported_string(source: &str, name: &str) -> String {
    let opening = format!("export const {name} = \"");
    let start = source
        .find(&opening)
        .unwrap_or_else(|| panic!("the script has no `{opening}`"))
        + opening.len();
    let length = source[start..]
        .find("\";")
        .unwrap_or_else(|| panic!("the script: `{name}` has no end"));
    source[start..start + length].to_string()
}

/// The string of `export const <name> = "...";` in `extension-trusted/pair.js`.
fn trusted_pair(name: &str) -> String {
    exported_string(&trusted_file("pair.js"), name)
}

/// The text of `const <name> = {` ... `};` in an extension file, from the line after the opening to
/// the line before the end, which is the nearer of the two ways an object ends since a later `});`
/// belongs to other code.
fn object_body<'a>(source: &'a str, object: &str) -> &'a str {
    let opening = format!("const {object} = ");
    let start = source
        .find(&opening)
        .unwrap_or_else(|| panic!("the source has no `{opening}`"));
    let body = &source[start..];
    let end = ["\n});", "\n};"]
        .iter()
        .filter_map(|ending| body.find(ending))
        .min()
        .unwrap_or_else(|| panic!("`{opening}` has no end"));
    &body[..end]
}

/// The names inside `const <name> = {` ... `};` in an extension file, each being the word before a
/// `:` or a `(`, one member to a line.
fn members(source: &str, object: &str) -> Vec<String> {
    object_body(source, object)
        .lines()
        .skip(1)
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| {
            let line = line.trim().trim_start_matches("async ");
            let name: String = line
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
                .collect();
            let rest = &line[name.len()..];
            (!name.is_empty() && (rest.starts_with(':') || rest.starts_with('('))).then_some(name)
        })
        .collect()
}

/// A group of `extension/groups.js`: its id, the optional permissions its tools call, whether they
/// read pages, whether it is limited to the allowed sites whatever its scope, and its tools.
struct Group {
    id: String,
    permissions: Vec<String>,
    hosts: bool,
    allowed_only: bool,
    tools: Vec<String>,
}

/// The rows of `const <object> = {` ... `};`, in order. Each row is one line of `"id": ...,` whose
/// text is JSON, so the line is read as a JSON object with the one member the id names.
fn rows(source: &str, object: &str) -> Vec<(String, Value)> {
    object_body(source, object)
        .lines()
        .skip(1)
        .filter(|line| line.starts_with("  \""))
        .map(|line| {
            let row: Value =
                serde_json::from_str(&format!("{{{}}}", line.trim().trim_end_matches(',')))
                    .unwrap_or_else(|error| panic!("{line}: {error}"));
            let (id, value) = row.as_object().unwrap().iter().next().unwrap();
            (id.clone(), value.clone())
        })
        .collect()
}

/// The rows of the group table, in order.
fn groups(source: &str) -> Vec<Group> {
    rows(source, "GROUPS")
        .into_iter()
        .map(|(id, group)| {
            let strings = |name: &str| -> Vec<String> {
                group[name]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|entry| entry.as_str().unwrap().to_string())
                    .collect()
            };
            Group {
                id,
                permissions: strings("permissions"),
                hosts: group["hosts"].as_bool().unwrap(),
                allowed_only: group["allowedOnly"].as_bool().unwrap_or(false),
                tools: strings("tools"),
            }
        })
        .collect()
}

/// The rows of the preset table, in order: each preset's name and the groups it names.
fn presets(source: &str) -> Vec<(String, Vec<String>)> {
    rows(source, "PRESETS")
        .into_iter()
        .map(|(name, groups)| {
            let ids = groups
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap().to_string())
                .collect();
            (name, ids)
        })
        .collect()
}

/// The names are read from where the object opens to where it ends, and a closing `});` further
/// down belongs to other code: reading on to it would take that code's names for tools.
#[test]
fn an_objects_members_end_where_the_object_does() {
    let source = "export const TOOLS = {\n  async first(chrome) {},\n};\n\nlistener(() => {\n  connect();\n});\n";
    assert_eq!(members(source, "TOOLS"), ["first"]);
    let frozen = "export const DEFAULTS = Object.freeze({\n  on: true,\n});\n\nconst other = {\n  off: false,\n};\n";
    assert_eq!(members(frozen, "DEFAULTS"), ["on"]);
}

/// A browser derives an unpacked extension's id from the key in its manifest: the first 16 bytes
/// of the key's SHA-256, each half-byte written as a letter from `a`. `install` records the id it
/// is given none, so that id has to be the one this key gives, or the host refuses the extension.
#[test]
fn the_id_install_records_is_the_one_the_extensions_key_gives() {
    let manifest = extension_manifest();
    let key = manifest["key"].as_str().expect("the manifest pins a key");
    assert_eq!(id_of(key), EXTENSION_ID);
}

/// The two extensions name each other by id. The second answers requests from the first alone, so
/// the one id its manifest lets message it and the one its script compares a sender with are the
/// first extension's, and the id the first sends to, in `extension/trusted.js`, is the one the
/// second's own key gives, as is the id the second names itself by. A mismatch fails closed, with
/// each refusing the other, and a wrong id in only one place leaves the browser's check and the
/// script's check disagreeing.
#[test]
fn each_extensions_id_is_the_one_its_key_gives_and_the_second_names_the_first() {
    let first = id_of(extension_manifest()["key"].as_str().unwrap());
    let second_manifest = trusted_manifest();
    let second = id_of(
        second_manifest["key"]
            .as_str()
            .expect("the second extension pins a key"),
    );
    assert_eq!(first, EXTENSION_ID);
    assert_ne!(first, second, "the two extensions have one id");
    assert_ne!(
        extension_manifest()["key"],
        second_manifest["key"],
        "the two extensions have one key"
    );
    assert_eq!(trusted_pair("FIRST_ID"), first);
    assert_eq!(trusted_pair("SECOND_ID"), second);
    assert_eq!(
        exported_string(&extension_file("trusted.js"), "SECOND_ID"),
        second,
        "the id the first extension sends to"
    );
    assert_eq!(
        second_manifest["externally_connectable"],
        serde_json::json!({ "ids": [first] }),
    );
}

/// The second extension holds the browser's debugger and nothing else, so what a person is told when
/// it is installed is two lines, and nothing it can do is hidden in another key. It declares no
/// host, no page of its own the web can reach, no content script and no way to talk to the host. The
/// first extension does not hold the debugger, in its required or optional permissions.
#[test]
fn the_second_extension_holds_the_debugger_and_the_first_extensions_id_alone() {
    let manifest = trusted_manifest();
    assert_eq!(manifest["permissions"], serde_json::json!(["debugger"]));
    let keys: BTreeSet<&str> = manifest
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "action",
            "background",
            "content_security_policy",
            "description",
            "externally_connectable",
            "key",
            "manifest_version",
            "minimum_chrome_version",
            "name",
            "permissions",
            "version",
        ]),
        "the manifest has a key that is not one of the allowed"
    );
    for key in [
        "optional_permissions",
        "host_permissions",
        "optional_host_permissions",
        "content_scripts",
        "web_accessible_resources",
    ] {
        assert!(manifest.get(key).is_none(), "the manifest has {key}");
    }
    assert_eq!(
        manifest["externally_connectable"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["ids"],
        "externally_connectable names a page or a connection besides an extension"
    );

    let first = extension_manifest();
    for key in ["permissions", "optional_permissions"] {
        assert!(
            !first[key]
                .as_array()
                .unwrap()
                .iter()
                .any(|permission| permission == "debugger"),
            "the first extension lists debugger in {key}"
        );
    }
}

/// Every script of the second extension, outside its tests, with its path and text.
fn trusted_scripts() -> Vec<(PathBuf, String)> {
    let mut scripts = Vec::new();
    for entry in std::fs::read_dir(trusted_directory()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "js") {
            continue;
        }
        let code = std::fs::read_to_string(&path).unwrap();
        scripts.push((path, code));
    }
    scripts
}

/// A request from another extension reaches only a script that registers `onMessageExternal`, and
/// the second extension registers it once, in its service worker. The one other thing it listens
/// for is a connection from a page of its own, the confirmation window and the dialog panel, which
/// the service worker hands to the window it belongs to. The browser gives such a connection to
/// `onConnect` for the extension's own pages alone, since the manifest has no content script, no
/// web page that may connect and no id but the first extension's. Only those two pages open one,
/// no script sends a message, and no script registers a listener for a message of any other kind,
/// so no web page, content script or other extension has a way to send it a request.
#[test]
fn the_second_extension_takes_a_request_from_the_first_extension_and_a_connection_from_its_own_windows_and_nothing_else()
 {
    let scripts = trusted_scripts();
    assert!(
        scripts.len() >= 12,
        "only {} files of the extension were read",
        scripts.len()
    );
    let mut external = Vec::new();
    let mut connections = Vec::new();
    let mut connecting = Vec::new();
    for (path, code) in &scripts {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        for (number, line) in code.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            let at = format!("{}:{}", path.display(), number + 1);
            for (position, _) in line.match_indices("onMessage") {
                let after = &line[position + "onMessage".len()..];
                if after.starts_with("External") {
                    external.push(file.clone());
                    continue;
                }
                // A port's own event, or the name of one in an object that stands in for a port.
                assert!(
                    line[..position].ends_with("port.") || after.starts_with(':'),
                    "{at} registers a listener for a message other than an external one"
                );
            }
            if line.contains("onConnect") {
                assert!(
                    !line.contains("onConnectExternal"),
                    "{at} registers a listener for a connection from another extension"
                );
                connections.push(file.clone());
            }
            if line.contains("runtime.connect(") {
                connecting.push(file.clone());
            }
            for word in [
                "sendMessage",
                "onUserScriptMessage",
                "onUserScriptConnect",
                "onmessage",
                "addEventListener(\"message\"",
                "addEventListener('message'",
            ] {
                assert!(!line.contains(word), "{at} uses {word}");
            }
        }
    }
    assert_eq!(external, ["background.js"]);
    // The router in windows.js, and the shared confirmation it hands the connections to.
    connections.sort();
    assert_eq!(
        connections,
        ["confirmation.js", "windows.js", "windows.js",],
        "a script other than the two that route a connection mentions onConnect"
    );
    connecting.sort();
    assert_eq!(
        connecting,
        ["confirm.js", "dialog.js"],
        "a script other than the two windows' pages opens a connection"
    );
}

/// Each tool the server offers calls the extension method of the same name, so the extension
/// answers exactly those, and the groups hold exactly those, each in one group, so that a person has
/// a switch for each of them in its options and the browser is asked for what each needs.
#[test]
fn the_extension_answers_every_tool_the_server_offers_and_no_other() {
    let offered: Vec<&str> = TOOLS.iter().map(|tool| tool.name).collect();
    assert_eq!(members(&extension_file("tools.js"), "TOOLS"), offered);

    let grouped: Vec<String> = groups(&extension_file("groups.js"))
        .into_iter()
        .flat_map(|group| group.tools)
        .collect();
    let mut sorted_grouped = grouped.clone();
    sorted_grouped.sort();
    sorted_grouped.dedup();
    assert_eq!(
        sorted_grouped.len(),
        grouped.len(),
        "a tool is in two groups"
    );
    let mut sorted_offered = offered.clone();
    sorted_offered.sort_unstable();
    assert_eq!(sorted_grouped, sorted_offered);
}

/// The options page looks up each setting's title and note by name, so a tool with a switch and no
/// entry makes the page throw and leaves that tool and every one after it without a switch. The
/// tools it labels are those the server offers and those the group table holds, each once.
#[test]
fn the_options_page_labels_every_tool_the_server_offers_and_no_other() {
    let offered: Vec<&str> = TOOLS.iter().map(|tool| tool.name).collect();
    let labelled = members(&extension_file("options.js"), "LABELS");
    assert_eq!(labelled, offered);

    let mut grouped: Vec<String> = groups(&extension_file("groups.js"))
        .into_iter()
        .flat_map(|group| group.tools)
        .collect();
    grouped.sort();
    let mut labelled = labelled;
    labelled.sort();
    assert_eq!(
        labelled, grouped,
        "the options page labels a tool that is in no group, or lacks one that is in a group"
    );
}

/// The manifest requires only what the extension itself uses and lists the rest as optional, so a
/// fresh install holds no permission that reads a tab, a page, history or bookmarks. Each group
/// lists the optional permissions its tools call, which include `webNavigation` to list a tab's
/// frames, and a group that reads pages needs the two host patterns. A permission a group lists that
/// the manifest does not offer could never be granted, and one the manifest offers that no group
/// lists is held by nobody's request. A permission a tool calls that no group lists would make the
/// browser refuse the call when the group is on.
#[test]
fn the_extension_has_each_permission_its_tools_require() {
    let manifest = extension_manifest();
    let strings = |key: &str| -> BTreeSet<String> {
        manifest[key]
            .as_array()
            .unwrap_or_else(|| panic!("the manifest has no {key}"))
            .iter()
            .map(|entry| entry.as_str().unwrap().to_string())
            .collect()
    };
    let owned = |names: &[&str]| -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    };
    let required = strings("permissions");
    let optional = strings("optional_permissions");
    let hosts = strings("optional_host_permissions");
    assert_eq!(required, owned(&["nativeMessaging", "storage", "alarms"]));
    assert!(
        manifest.get("host_permissions").is_none(),
        "a host the extension holds at install is one no group asked for"
    );
    assert_eq!(hosts, owned(&["https://*/*", "http://*/*", "<all_urls>"]));
    assert!(
        required.is_disjoint(&optional),
        "a permission is either held at install or asked for, not both: {:?}",
        required.intersection(&optional).collect::<Vec<_>>()
    );

    let table = groups(&extension_file("groups.js"));
    for group in &table {
        for permission in &group.permissions {
            assert!(
                optional.contains(permission),
                "{} lists {permission}, which the manifest does not offer as optional",
                group.id
            );
        }
    }
    let used: BTreeSet<String> = table
        .iter()
        .flat_map(|group| group.permissions.iter().cloned())
        .collect();
    assert!(used.contains("webNavigation"));
    assert_eq!(optional, used);
    assert!(
        table.iter().any(|group| group.hosts),
        "no group reads pages, so the hosts the manifest offers are asked for by nobody"
    );

    // The namespaces the tools call, other than the four that need no permission, are each
    // one the manifest requires or one a group lists. A permission such as `system.cpu` names a
    // namespace, `system`, with the others of its family.
    let listed: BTreeSet<&str> = required
        .iter()
        .chain(&used)
        .map(|name| name.split('.').next().unwrap())
        .chain(["runtime", "permissions", "windows", "extension"])
        .collect();
    let code = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}",
        extension_file("tools.js"),
        extension_file("requests.js"),
        extension_file("awake.js"),
        extension_file("held.js"),
        extension_file("screen.js"),
        extension_file("capture.js"),
        extension_file("scripts.js")
    );
    let called: BTreeSet<String> = code
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .flat_map(|line| line.split("chrome.").skip(1))
        .map(|rest| {
            rest.chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect::<String>()
        })
        .filter(|namespace| !namespace.is_empty())
        .collect();
    assert!(called.contains("history") && called.contains("webNavigation"));
    for namespace in [
        "tabGroups",
        "sessions",
        "topSites",
        "readingList",
        "webRequest",
        "privacy",
        "contentSettings",
        "fontSettings",
        "idle",
        "power",
        "search",
        "cookies",
        "browsingData",
        "downloads",
        "contextMenus",
        "pageCapture",
        "desktopCapture",
        "userScripts",
    ] {
        assert!(
            called.contains(namespace),
            "a group lists {namespace} and no tool in tools.js, requests.js, awake.js, held.js, screen.js, capture.js or scripts.js calls chrome.{namespace}"
        );
    }
    // The four `system` permissions each have an API of their own.
    for permission in used.iter().filter(|name| name.starts_with("system.")) {
        assert!(
            code.contains(&format!("chrome.{permission}.")),
            "a group lists {permission} and no tool calls chrome.{permission}"
        );
    }
    for namespace in &called {
        assert!(
            listed.contains(namespace.as_str()),
            "tools.js, requests.js, awake.js, held.js, screen.js, capture.js or scripts.js calls chrome.{namespace}, which no group lists and the manifest does not require"
        );
    }
}

/// Every script of the extension, outside its tests, with its path and text.
fn extension_scripts() -> Vec<(PathBuf, String)> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extension");
    let mut scripts = Vec::new();
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "js") {
            continue;
        }
        let code = std::fs::read_to_string(&path).unwrap();
        scripts.push((path, code));
    }
    scripts
}

/// The browser gives a listener of `webRequest` a request's headers, cookies or body only when the
/// listener names that information as it is registered, and `extraHeaders` is what adds `Cookie`,
/// `Authorization` and `Set-Cookie` among the headers. The extension keeps a request's URL, type
/// and status, so no line of its code outside a comment names any of the four.
#[test]
fn the_extension_never_asks_the_browser_for_a_requests_headers_or_body() {
    let scripts = extension_scripts();
    for (path, code) in &scripts {
        for (number, line) in code.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for word in [
                "extraHeaders",
                "requestHeaders",
                "responseHeaders",
                "requestBody",
            ] {
                assert!(
                    !line.contains(word),
                    "{}:{} asks the browser for {word}",
                    path.display(),
                    number + 1
                );
            }
        }
    }
    assert!(
        scripts.len() >= 6,
        "only {} files of the extension were read",
        scripts.len()
    );
}

/// A web page can send an extension a message only if the manifest's `externally_connectable`
/// names the page, and a message from another extension reaches only a script that registers
/// `onMessageExternal` or `onConnectExternal`. The extension is driven by its native messaging port
/// alone, so it declares the first of these nowhere and registers neither listener.
#[test]
fn the_extension_accepts_no_message_from_a_web_page_or_another_extension() {
    assert!(
        extension_manifest().get("externally_connectable").is_none(),
        "the manifest declares externally_connectable"
    );
    let scripts = extension_scripts();
    for (path, code) in &scripts {
        for (number, line) in code.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for word in ["onMessageExternal", "onConnectExternal"] {
                assert!(
                    !line.contains(word),
                    "{}:{} registers {word}",
                    path.display(),
                    number + 1
                );
            }
        }
    }
    assert!(
        scripts.len() >= 6,
        "only {} files of the extension were read",
        scripts.len()
    );
}

/// The first extension sends to the second and takes nothing from it. `runtime.sendMessage` with
/// the id of another extension is called in `trusted.js` alone, once, to the id the second
/// extension's key gives, so the one door that receives a message from another extension is the
/// second's (BROWSER-12). The test above holds that no script of the first extension registers a
/// listener for one. The capture window of `screen` calls `runtime.sendMessage` once with a message
/// alone, which names no extension and so reaches this extension's own worker (BROWSER-42).
#[test]
fn the_extension_sends_a_message_to_another_extension_from_one_call_in_trusted_js() {
    let mut sends = Vec::new();
    let mut to_itself = Vec::new();
    for (path, code) in extension_scripts() {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        for line in code.lines() {
            if line.trim_start().starts_with("//") || !line.contains("sendMessage") {
                continue;
            }
            if line.contains("sendMessage({") {
                to_itself.push(file.clone());
            } else {
                sends.push((file.clone(), line.trim().to_string()));
            }
        }
    }
    assert_eq!(
        sends
            .iter()
            .map(|(file, _)| file.as_str())
            .collect::<Vec<_>>(),
        ["trusted.js"],
        "a script besides trusted.js, or more than one call in it, sends a message: {sends:?}"
    );
    assert!(
        sends[0]
            .1
            .contains("chrome.runtime.sendMessage(SECOND_ID, "),
        "{}",
        sends[0].1
    );
    assert_eq!(
        to_itself,
        ["capture.js"],
        "a script besides capture.js, or more than one call in it, sends a message to this extension"
    );
}

/// The toolbar action opens a popup page the extension has, and the keyboard command that does
/// what its button does is declared with no key, since a default key can collide with one the
/// person or Brave already uses. The person gives it one in the browser's shortcut settings.
#[test]
fn the_toolbar_action_opens_a_popup_and_the_keyboard_command_has_no_default_key() {
    let manifest = extension_manifest();
    let popup = manifest["action"]["default_popup"]
        .as_str()
        .expect("the manifest's action names no popup");
    let page = extension_file(popup);
    assert!(
        page.contains("Turn everything off"),
        "{popup} has no Turn everything off"
    );
    let commands = manifest["commands"]
        .as_object()
        .expect("the manifest declares no commands");
    assert_eq!(
        commands.keys().collect::<Vec<_>>(),
        ["turn-everything-off"],
        "the commands are the one that turns everything off"
    );
    let command = &commands["turn-everything-off"];
    assert!(
        command["description"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
    assert!(
        command.get("suggested_key").is_none(),
        "the command has a default key"
    );
    let worker = extension_file("background.js");
    assert!(
        worker.contains("\"turn-everything-off\""),
        "the service worker does not listen for the command the manifest declares"
    );
}

/// Brave merged the side panel into its Sidebar, which has no control to open a side-panel
/// extension (brave-browser #31334), and `sidePanel.open` from a popup window crashes the browser
/// (#59256). The extension declares no `sidePanel` permission and no `side_panel` key, and no
/// script names the API.
#[test]
fn the_extension_does_not_use_the_side_panel() {
    let manifest = extension_manifest();
    assert!(
        manifest.get("side_panel").is_none(),
        "the manifest declares side_panel"
    );
    for key in ["permissions", "optional_permissions"] {
        assert!(
            !manifest[key]
                .as_array()
                .unwrap()
                .iter()
                .any(|name| name == "sidePanel"),
            "{key} lists sidePanel"
        );
    }
    let scripts = extension_scripts();
    for (path, code) in &scripts {
        for (number, line) in code.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            assert!(
                !line.contains("sidePanel"),
                "{}:{} names the side panel",
                path.display(),
                number + 1
            );
        }
    }
    assert!(
        scripts.len() >= 6,
        "only {} files of the extension were read",
        scripts.len()
    );
}

/// BROWSER-29: a video of a tab waits for a ruling, so nothing of it exists yet. `capture_tab_video`
/// would list `tabCapture`. No group lists `offscreen`, which the `screen` group cannot use
/// (BROWSER-42), and no script uses an offscreen document. `input` is built on `scripting` alone, so
/// no group lists `debugger` and no script of this extension calls it; the second extension holds
/// `debugger` for `input-trusted`. `screenshot_page` is built ([BROWSER-35]) and calls
/// `captureVisibleTab`, `save_page_archive` is built ([BROWSER-40]) and calls `pageCapture`, `screen`
/// is built ([BROWSER-42]) on `desktopCapture`, and `run_script` is built ([BROWSER-46]) on
/// `userScripts`; the tests after this one hold the first two to one call in one tool. The group
/// lines, the manifest, the relay's table and the extension's code are each read for the first sign
/// of one that is not built. The first tool to land fails this, and the clause is replaced in the
/// same change.
#[test]
fn the_entries_that_wait_for_a_ruling_have_no_tool_permission_or_call() {
    const APIS: [&str; 3] = ["debugger", "tabCapture", "offscreen"];
    const NOT_BUILT: [&str; 1] = ["capture_tab_video"];

    let table = groups(&extension_file("groups.js"));
    for group in &table {
        for permission in &group.permissions {
            assert!(
                !APIS.contains(&permission.as_str()),
                "{} lists {permission}",
                group.id
            );
        }
    }
    let manifest = extension_manifest();
    for key in ["permissions", "optional_permissions"] {
        for entry in manifest[key].as_array().unwrap() {
            let name = entry.as_str().unwrap();
            assert!(!APIS.contains(&name), "the manifest declares {name}");
        }
    }
    for tool in TOOLS {
        assert!(
            !NOT_BUILT.contains(&tool.name),
            "the relay offers {}",
            tool.name
        );
    }

    let calls: Vec<String> = APIS.iter().map(|api| format!("chrome.{api}")).collect();
    let scripts = extension_scripts();
    for (path, code) in &scripts {
        for (number, line) in code.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for call in &calls {
                assert!(
                    !line.contains(call.as_str()),
                    "{}:{} calls {call}",
                    path.display(),
                    number + 1
                );
            }
        }
    }
    assert!(
        scripts.len() >= 6,
        "only {} files of the extension were read",
        scripts.len()
    );
}

/// The product `a * b * c` an assignment holds, as the number it writes.
fn product_after(source: &str, start: &str) -> u64 {
    let line = source
        .lines()
        .find(|line| line.contains(start))
        .unwrap_or_else(|| panic!("no line holds {start}"));
    line.split('=')
        .nth(1)
        .unwrap()
        .trim_end_matches(';')
        .split('*')
        .map(|factor| factor.trim().parse::<u64>().unwrap())
        .product()
}

/// BROWSER-35: the browser lets an extension capture a tab only with `<all_urls>`, so the manifest
/// offers that pattern as an optional host and `groups.js` names the one tool that needs it. The
/// extension calls `captureVisibleTab` once, in `tools.js`, and in no other file. The largest
/// picture it returns is the largest the MCP client takes from a tool, so a picture the client
/// would leave out and count is refused by the extension first, saying why.
#[test]
fn the_extension_captures_a_tab_in_one_call_with_all_urls_and_within_the_clients_cap() {
    let groups_js = extension_file("groups.js");
    assert!(groups_js.contains("export const ALL_URLS = \"<all_urls>\";"));
    assert!(groups_js.contains(
        "export const NEEDS_ALL_URLS = Object.freeze([\"screenshot_page\", \"record_gif\"]);"
    ));
    let hosts: Vec<String> = extension_manifest()["optional_host_permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|host| host.as_str().unwrap().to_string())
        .collect();
    assert_eq!(hosts, ["https://*/*", "http://*/*", "<all_urls>"]);
    assert!(extension_manifest().get("host_permissions").is_none());

    let mut lines = Vec::new();
    for (path, code) in extension_scripts() {
        for (number, line) in code.lines().enumerate() {
            if !line.trim_start().starts_with("//") && line.contains("captureVisibleTab") {
                lines.push(format!(
                    "{}:{}",
                    path.file_name().unwrap().to_string_lossy(),
                    number + 1
                ));
            }
        }
    }
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("tools.js:"), "{lines:?}");

    let cap = product_after(
        &extension_file("pictures.js"),
        "export const MAX_PICTURE_BYTES =",
    );
    // The second extension holds the same number, for the picture it makes.
    assert_eq!(
        cap,
        product_after(
            &std::fs::read_to_string(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../extension-trusted/capture.js")
            )
            .unwrap(),
            "export const MAX_PICTURE_BYTES =",
        )
    );
    let client = product_after(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../mcp/src/protocol.rs"),
        )
        .unwrap(),
        "pub const MAX_PICTURE_BYTES: usize =",
    );
    assert_eq!(cap, client);
    assert_eq!(cap, 8 * 1024 * 1024);
}

/// BROWSER-40: `save_page_archive` is in `downloads`, which lists `pageCapture` for it, `downloads`
/// to save the file and `tabs` to find the tab by its address (Brave leaves the address out of a
/// tab for an extension that holds neither `tabs` nor a host covering it), reads no host, and no
/// other group lists `pageCapture`. No preset names `downloads`, so the Read and Browse presets ask
/// the browser for neither `downloads` nor `pageCapture`. The tool is limited to the allowed sites
/// by name. The extension makes the archive in one call, in `tools.js`, and starts a download in
/// two places (`download_file` and this tool), each with no dialog and no replacing. The largest
/// archive it saves is 16 MiB.
#[test]
fn the_extension_archives_a_tab_in_one_call_and_saves_it_through_downloads_with_no_dialog() {
    let table = groups(&extension_file("groups.js"));
    let listing = |permission: &str| -> Vec<&str> {
        table
            .iter()
            .filter(|group| group.permissions.iter().any(|name| name == permission))
            .map(|group| group.id.as_str())
            .collect()
    };
    assert_eq!(listing("pageCapture"), ["downloads"]);
    assert_eq!(listing("downloads"), ["downloads", "remove-data"]);
    let downloads = table.iter().find(|group| group.id == "downloads").unwrap();
    assert!(!downloads.hosts && !downloads.allowed_only);
    assert!(downloads.permissions.iter().any(|name| name == "tabs"));
    assert!(
        downloads
            .tools
            .iter()
            .any(|name| name == "save_page_archive")
    );
    for (name, ids) in presets(&extension_file("groups.js")) {
        for id in &ids {
            let group = table.iter().find(|group| &group.id == id).unwrap();
            for permission in ["pageCapture", "downloads"] {
                assert!(
                    !group.permissions.iter().any(|held| held == permission),
                    "the {name} preset names {id}, which lists {permission}"
                );
            }
        }
    }
    assert!(
        extension_file("groups.js")
            .contains("export const ALLOWED_ONLY_TOOLS = Object.freeze([\"save_page_archive\"]);")
    );
    assert!(extension_file("tools.js").contains("ALLOWED_ONLY_TOOLS.includes(method)"));
    let optional = extension_manifest()["optional_permissions"].clone();
    assert!(
        optional
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "pageCapture")
    );

    let mut archives = Vec::new();
    let mut downloads = Vec::new();
    for (path, code) in extension_scripts() {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        for (number, line) in code.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if line.contains("chrome.pageCapture") {
                archives.push(format!("{file}:{}", number + 1));
            }
            if line.contains("chrome.downloads.download(") {
                downloads.push(format!("{file}:{}", number + 1));
            }
        }
    }
    assert_eq!(archives.len(), 1, "{archives:?}");
    assert!(archives[0].starts_with("tools.js:"), "{archives:?}");
    assert_eq!(downloads.len(), 2, "{downloads:?}");
    assert!(downloads.iter().all(|at| at.starts_with("tools.js:")));

    let tools_js = extension_file("tools.js");
    for option in ["saveAs: false,", "conflictAction: \"uniquify\","] {
        assert_eq!(tools_js.matches(option).count(), 2, "{option}");
    }
    assert_eq!(
        product_after(&tools_js, "export const MAX_ARCHIVE_BYTES ="),
        16 * 1024 * 1024
    );
}

/// The relay offers the tools of a group from its own table and the extension runs them from
/// `groups.js`, and the two have to say the same thing: the same groups in the same order, and in
/// each the same tools. A tool the relay offers under a group that the extension has in another
/// would be approved by a person under one name and refused by the extension for the permissions
/// of the other, and a group the extension lists that the relay does not know could not be declared.
#[test]
fn the_relays_group_table_is_the_one_the_extension_holds() {
    let table = groups(&extension_file("groups.js"));
    let ids: Vec<&str> = table.iter().map(|group| group.id.as_str()).collect();
    assert_eq!(tools::ids().collect::<Vec<_>>(), ids);

    for group in &table {
        let offered: Vec<&str> = tools::offered(Some(group.id.as_str()))
            .map(|tool| tool.name)
            .collect();
        let held: Vec<&str> = group.tools.iter().map(String::as_str).collect();
        assert_eq!(offered, held, "the tools of {}", group.id);
        let described: Vec<&str> = TOOLS
            .iter()
            .filter(|tool| tool.group == group.id)
            .map(|tool| tool.name)
            .collect();
        assert_eq!(
            described, group.tools,
            "the tools of {} in the table",
            group.id
        );
    }
    for tool in TOOLS {
        let holding: Vec<&str> = table
            .iter()
            .filter(|group| group.tools.iter().any(|name| name == tool.name))
            .map(|group| group.id.as_str())
            .collect();
        assert_eq!(holding, [tool.group], "the group of {}", tool.name);
    }
}

/// `install --preset` declares the groups a preset names in the relay, and the options page turns on
/// the groups the same preset names in `groups.js`. A preset that names other groups in one of them
/// would have a person approve one set of servers and the extension grant the permissions of another.
#[test]
fn the_relays_presets_are_the_ones_the_extension_holds() {
    let held = presets(&extension_file("groups.js"));
    let relay: Vec<(String, Vec<String>)> = install::PRESETS
        .iter()
        .map(|(name, ids)| {
            (
                (*name).to_string(),
                ids.iter().map(|id| (*id).to_string()).collect(),
            )
        })
        .collect();
    assert_eq!(relay, held);
}

/// The design document, which says what the tools are and gives the group table.
fn design_document() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/browser-extension.md");
    std::fs::read_to_string(&path).unwrap()
}

/// The design document says what the tools are, so a tool the server offers that it does not name
/// is one whose reasons and limits are written nowhere a reviewer reads.
#[test]
fn the_design_document_names_every_tool_the_server_offers() {
    let document = design_document();
    for tool in TOOLS {
        assert!(
            document.contains(&format!("`{}`", tool.name)),
            "{} is not named in docs/design/browser-extension.md",
            tool.name
        );
    }
}

/// The cells of each row of the tables under `### <heading>` of the design document, in order. The
/// header of a table and the rule under it are left out.
fn document_rows(document: &str, heading: &str) -> Vec<Vec<String>> {
    let opening = format!("### {heading}");
    let mut lines = document.lines().skip_while(|line| *line != opening).skip(1);
    let mut rows = Vec::new();
    let mut in_body = false;
    for line in lines.by_ref().take_while(|line| !line.starts_with('#')) {
        if !line.starts_with('|') {
            in_body = false;
            continue;
        }
        let cells: Vec<String> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(|cell| cell.trim().to_string())
            .collect();
        if cells.iter().all(|cell| cell.chars().all(|c| c == '-')) {
            in_body = true;
        } else if in_body {
            rows.push(cells);
        }
    }
    assert!(
        !rows.is_empty(),
        "the design document has no table under `{opening}`"
    );
    rows
}

/// The text between pairs of backticks in `cell`, in order.
fn code_spans(cell: &str) -> Vec<&str> {
    cell.split('`').skip(1).step_by(2).collect()
}

/// The quoted words of `const <name> = ...([ ... ]);` in an extension file.
fn quoted_list(source: &str, name: &str) -> Vec<String> {
    let start = source
        .find(&format!("const {name} = "))
        .unwrap_or_else(|| panic!("the source has no `const {name}`"));
    let body = &source[start..];
    let end = body
        .find("]);")
        .unwrap_or_else(|| panic!("`{name}` has no end"));
    body[..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// The lines of an extension script outside comments, joined.
fn code_of(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The design document has a row for each group with what `groups.js`, `lifetimes.js`, the presets
/// and the relay's table say of it, a row for each tool under the group that holds it, and the
/// Chrome methods each tool calls. A tool dropped from the document, listed under another group or
/// added without a row, a value changed in one file only, and a method the code stopped calling or
/// began to call would each leave the document saying something the extension does not do.
#[test]
fn the_design_documents_group_tables_are_the_ones_the_extension_and_the_relay_hold() {
    let document = design_document();
    let table = groups(&extension_file("groups.js"));
    let held_presets = presets(&extension_file("groups.js"));
    let until_restart = quoted_list(&extension_file("lifetimes.js"), "UNTIL_RESTART_BY_DEFAULT");

    let listed = document_rows(&document, "The groups");
    let ids: Vec<&str> = table.iter().map(|group| group.id.as_str()).collect();
    let documented: Vec<&str> = listed.iter().map(|row| code_spans(&row[0])[0]).collect();
    assert_eq!(documented, ids, "the groups of the first table");
    for (group, row) in table.iter().zip(&listed) {
        let id = &group.id;
        assert_eq!(row.len(), 7, "the columns of {id}");
        let words = |cell: &str| -> Vec<String> {
            code_spans(cell).into_iter().map(str::to_string).collect()
        };
        assert_eq!(words(&row[1]), group.permissions, "the permissions of {id}");
        if group.permissions.is_empty() {
            assert_eq!(row[1], "none", "{id} lists no permission");
        }
        assert_eq!(
            row[2],
            if group.hosts { "yes" } else { "no" },
            "the hosts of {id}"
        );
        assert_eq!(
            row[3],
            if until_restart.contains(id) {
                "until Brave restarts"
            } else {
                "until turned off"
            },
            "the default lifetime of {id}"
        );
        let named: Vec<&str> = held_presets
            .iter()
            .filter(|(_, members)| members.contains(id))
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(words(&row[4]), named, "the presets of {id}");
        if named.is_empty() {
            assert_eq!(row[4], "none", "{id} is in no preset");
        }
        let relay = tools::GROUPS.iter().find(|held| held.id == id).unwrap();
        assert_eq!(
            row[5],
            if relay.ask_every_call { "yes" } else { "no" },
            "whether {id} is asked about every call"
        );
        if group.tools.is_empty() {
            assert!(
                row[6].starts_with("none, not built") && row[6].contains("BROWSER-29"),
                "{id} has no tool, so its row says it is not built and cites BROWSER-29: {}",
                row[6]
            );
        } else {
            assert_eq!(
                row[6],
                group.tools.len().to_string(),
                "the tool count of {id}"
            );
        }
    }

    // The tools, each under the group that holds it.
    let methods = document_rows(&document, "The Chrome methods behind each tool");
    let rows: Vec<(String, String)> = methods
        .iter()
        .filter(|row| row.len() == 3)
        .map(|row| {
            (
                code_spans(&row[0])[0].to_string(),
                code_spans(&row[1])[0].to_string(),
            )
        })
        .collect();
    for group in &table {
        let documented: Vec<&str> = rows
            .iter()
            .filter(|(id, _)| *id == group.id)
            .map(|(_, tool)| tool.as_str())
            .collect();
        assert_eq!(
            documented, group.tools,
            "the tools of {} in the document",
            group.id
        );
    }
    let expected: Vec<(String, String)> = table
        .iter()
        .flat_map(|group| {
            group
                .tools
                .iter()
                .map(|tool| (group.id.clone(), tool.clone()))
        })
        .collect();
    assert_eq!(
        rows, expected,
        "a row of the methods table names no group of groups.js"
    );

    // The methods. A name in the document is one the code calls, and a call in the code of the
    // tools is one the document names. A call is written `chrome.<namespace>.<method>`, except the
    // few the code reaches by another spelling, which are listed with it.
    let tokens: Vec<&str> = methods
        .iter()
        .flat_map(|row| code_spans(row.last().unwrap()))
        .filter(|span| span.contains('.'))
        .collect();
    let scripts: String = extension_scripts()
        .iter()
        .map(|(_, source)| code_of(source))
        .collect::<Vec<_>>()
        .join("\n");
    let by_another_spelling = [
        ("dom.openOrClosedShadowRoot", "dom?.openOrClosedShadowRoot("),
        ("power.releaseKeepAwake", "chrome.power?.releaseKeepAwake("),
        ("fontSettings.getDefaultFontSize", "\"getDefaultFontSize\""),
        (
            "fontSettings.getDefaultFixedFontSize",
            "\"getDefaultFixedFontSize\"",
        ),
        ("fontSettings.getMinimumFontSize", "\"getMinimumFontSize\""),
    ];
    for token in &tokens {
        let call = if let Some((_, spelling)) =
            by_another_spelling.iter().find(|(name, _)| name == token)
        {
            (*spelling).to_string()
        } else if token.contains('*') {
            // The settings and kinds are read by name from a table in tools.js.
            format!("chrome.{}", token.split('.').next().unwrap())
        } else {
            format!("chrome.{token}")
        };
        assert!(
            scripts.contains(&call),
            "the document names {token}, and no script of the extension has `{call}`"
        );
    }
    let named: Vec<Vec<&str>> = tokens
        .iter()
        .map(|token| token.split('.').collect())
        .collect();
    let tool_code = [
        "tools.js",
        "requests.js",
        "awake.js",
        "held.js",
        "screen.js",
        "capture.js",
        "scripts.js",
    ]
    .map(|name| code_of(&extension_file(name)))
    .join("\n");
    for rest in tool_code.split("chrome.").skip(1) {
        let call: Vec<&str> = rest
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '.')
            .next()
            .unwrap()
            .trim_end_matches('.')
            .split('.')
            .collect();
        let agrees = |name: &Vec<&str>| {
            call.iter()
                .zip(name)
                .all(|(a, b)| a == b || *a == "*" || *b == "*")
        };
        assert!(
            named.iter().any(agrees),
            "tools.js, requests.js, awake.js, held.js, screen.js, capture.js or scripts.js calls chrome.{}, which the methods tables of the design document do not name",
            call.join(".")
        );
    }
}

/// The names of the arguments a tool takes, sorted.
fn argument_names(tool: &tools::Tool) -> Vec<String> {
    let mut names: Vec<String> = tool.listed()["inputSchema"]["properties"]
        .as_object()
        .map(|properties| properties.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// The design document states, for every tool, its routing field and the arguments it takes, and
/// says what the question shows for every argument any tool takes. A tool added without its row,
/// a row left behind by a tool that went, a routing field that is not the alias of the tool's group
/// and its name, an argument added to a tool and not to its row, and an argument that no row of
/// the argument table describes would each leave the document claiming something about what a
/// person approves that the relay does not offer.
#[test]
fn the_design_document_states_the_routing_field_and_the_arguments_of_every_tool() {
    let document = design_document();
    let table = groups(&extension_file("groups.js"));

    let heading = "The routing field and arguments of each tool";
    let rows = document_rows(&document, heading);
    let expected: Vec<(String, String)> = table
        .iter()
        .flat_map(|group| {
            group
                .tools
                .iter()
                .map(|tool| (group.id.clone(), tool.clone()))
        })
        .collect();
    let documented: Vec<(String, String)> = rows
        .iter()
        .map(|row| {
            (
                code_spans(&row[0])[0].to_string(),
                code_spans(&row[1])[0].to_string(),
            )
        })
        .collect();
    let missing: Vec<&(String, String)> = expected
        .iter()
        .filter(|pair| !documented.contains(pair))
        .collect();
    let extra: Vec<&(String, String)> = documented
        .iter()
        .filter(|pair| !expected.contains(pair))
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "`### {heading}` in docs/design/browser-extension.md has no row for {missing:?} and has \
         rows for {extra:?}, which extension/groups.js does not list. A new tool needs a row \
         giving its routing field and its arguments"
    );
    assert_eq!(
        documented, expected,
        "the rows of `### {heading}` are in the order of extension/groups.js, one to a tool"
    );
    for (row, (group, name)) in rows.iter().zip(&documented) {
        assert_eq!(row.len(), 4, "the columns of {name}");
        let tool = TOOLS
            .iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("the document has a row for {name}, which the relay lacks"));
        assert_eq!(tool.group, group, "the group of {name} in the relay");
        assert_eq!(
            row[2],
            format!("`brave-{group}:{name}`"),
            "the routing field of {name} is the alias of its group and its name (ROUTE-7)"
        );
        let taken = argument_names(tool);
        let spans: Vec<String> = code_spans(&row[3])
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_eq!(
            spans, taken,
            "the arguments of {name} in the document, in alphabetical order"
        );
        if taken.is_empty() {
            assert_eq!(row[3], "none", "{name} takes no argument");
        }
    }
    assert_eq!(
        rows.len(),
        TOOLS.len(),
        "the document has a row for each tool the relay offers and no other"
    );

    let described = document_rows(&document, "Tool surface");
    let in_document: BTreeSet<String> = described
        .iter()
        .flat_map(|row| code_spans(&row[0]).into_iter().map(str::to_string))
        .collect();
    let in_use: BTreeSet<String> = TOOLS.iter().flat_map(argument_names).collect();
    assert_eq!(
        in_document, in_use,
        "the arguments the table of `### Tool surface` describes, and the arguments the tools take"
    );
    for row in &described {
        assert!(
            row.len() == 2 && !row[1].is_empty(),
            "{} has no description of what the question shows",
            row[0]
        );
    }
}

/// The table lists the twenty-one groups, one to a line and in this order. Only the platform group
/// and the groups a tool of which is done by another extension ask the browser for nothing, and
/// the tools are in the groups that name what they use.
#[test]
fn the_group_table_lists_the_groups_and_each_tool_in_one() {
    let table = groups(&extension_file("groups.js"));
    let ids: Vec<&str> = table.iter().map(|group| group.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "platform",
            "tabs-read",
            "history-read",
            "bookmarks-read",
            "page-read",
            "tabs-manage",
            "bookmarks-write",
            "navigate",
            "downloads",
            "cookies",
            "network-watch",
            "browser-info",
            "remove-data",
            "scripts",
            "input",
            "input-trusted",
            "capture-trusted",
            "console-trusted",
            "upload-trusted",
            "screen",
            "held",
        ]
    );
    let row = |id: &str| table.iter().find(|group| group.id == id).unwrap();
    let tools = |id: &str| -> Vec<&str> { row(id).tools.iter().map(String::as_str).collect() };
    assert_eq!(tools("platform"), ["get_platform_info"]);
    assert_eq!(
        tools("tabs-read"),
        [
            "list_tabs",
            "list_frames",
            "list_windows",
            "list_tab_groups",
            "list_recently_closed"
        ]
    );
    assert_eq!(
        tools("history-read"),
        [
            "search_history",
            "get_visits",
            "list_top_sites",
            "list_synced_tabs"
        ]
    );
    let permissions =
        |id: &str| -> Vec<&str> { row(id).permissions.iter().map(String::as_str).collect() };
    assert_eq!(
        permissions("tabs-read"),
        ["tabs", "tabGroups", "webNavigation", "sessions"]
    );
    assert_eq!(
        permissions("history-read"),
        ["history", "topSites", "sessions"]
    );
    assert_eq!(
        tools("bookmarks-read"),
        [
            "search_bookmarks",
            "list_bookmark_folder",
            "list_recent_bookmarks",
            "list_reading_list"
        ]
    );
    assert_eq!(permissions("bookmarks-read"), ["bookmarks", "readingList"]);
    assert_eq!(
        tools("page-read"),
        [
            "read_page",
            "read_page_links",
            "read_page_forms",
            "list_page_elements",
            "screenshot_page"
        ]
    );
    assert_eq!(
        permissions("page-read"),
        ["tabs", "scripting", "webNavigation"]
    );
    assert!(row("page-read").hosts);
    assert_eq!(
        tools("tabs-manage"),
        [
            "focus_tab",
            "pin_tab",
            "mute_tab",
            "move_tab",
            "group_tabs",
            "ungroup_tabs",
            "update_tab_group",
            "discard_tab",
            "duplicate_tab",
            "set_zoom",
            "focus_window",
            "resize_window"
        ]
    );
    assert_eq!(
        tools("bookmarks-write"),
        [
            "create_bookmark",
            "create_bookmark_folder",
            "move_bookmark",
            "rename_bookmark",
            "add_reading_list_entry",
            "update_reading_list_entry"
        ]
    );
    assert_eq!(permissions("bookmarks-write"), ["bookmarks", "readingList"]);
    assert!(!row("bookmarks-write").hosts);
    assert_eq!(
        tools("navigate"),
        [
            "open_tab",
            "navigate_tab",
            "reload_tab",
            "go_back",
            "go_forward",
            "open_window",
            "restore_closed",
            "search_web",
            "wait_for_load"
        ]
    );
    assert_eq!(
        permissions("navigate"),
        ["tabs", "sessions", "search", "webNavigation"]
    );
    assert!(!row("navigate").hosts);
    assert_eq!(tools("cookies"), ["list_cookie_names", "delete_cookie"]);
    assert_eq!(permissions("cookies"), ["cookies"]);
    assert!(row("cookies").hosts);
    assert_eq!(
        tools("downloads"),
        [
            "save_page_archive",
            "download_file",
            "list_downloads",
            "pause_download",
            "resume_download",
            "cancel_download",
            "show_download"
        ]
    );
    assert_eq!(
        permissions("downloads"),
        ["downloads", "pageCapture", "tabs"]
    );
    assert!(!row("downloads").hosts);
    assert_eq!(tools("network-watch"), ["list_requests"]);
    assert_eq!(permissions("network-watch"), ["webRequest"]);
    assert_eq!(
        tools("browser-info"),
        [
            "get_privacy_settings",
            "get_content_settings",
            "get_font_settings",
            "get_system_info",
            "get_idle_state",
            "get_access_flags",
            "keep_awake"
        ]
    );
    assert_eq!(
        permissions("browser-info"),
        [
            "privacy",
            "contentSettings",
            "fontSettings",
            "system.cpu",
            "system.memory",
            "system.display",
            "system.storage",
            "idle",
            "power"
        ]
    );
    assert!(!row("browser-info").hosts);
    assert_eq!(
        tools("remove-data"),
        [
            "close_tab",
            "close_window",
            "delete_history_entry",
            "delete_history_range",
            "clear_browsing_data",
            "remove_bookmark",
            "remove_bookmark_folder",
            "remove_reading_list_entry",
            "delete_downloaded_file"
        ]
    );
    assert_eq!(
        permissions("remove-data"),
        [
            "tabs",
            "history",
            "bookmarks",
            "readingList",
            "browsingData",
            "downloads"
        ]
    );
    assert!(!row("remove-data").hosts);
    assert_eq!(
        tools("input"),
        [
            "click_element",
            "type_text",
            "select_option",
            "press_key",
            "scroll_page",
            "wait_for_element"
        ]
    );
    assert_eq!(permissions("input"), ["tabs", "scripting"]);
    // Code the planner wrote is run with the browser's userScripts API in a page the extension holds
    // the host of, and only in the sites the person allowed (BROWSER-46).
    assert_eq!(tools("scripts"), ["run_script"]);
    assert_eq!(permissions("scripts"), ["userScripts"]);
    assert!(row("scripts").hosts && row("scripts").allowed_only);
    let relay_scripts = tools::GROUPS
        .iter()
        .find(|group| group.id == "scripts")
        .unwrap();
    assert!(
        relay_scripts.ask_every_call && relay_scripts.asks_in_browser,
        "the relay declares the server of scripts to ask every call, and its calls wait for a person"
    );
    assert!(row("input").hosts);
    let relay_group = tools::GROUPS
        .iter()
        .find(|group| group.id == "input")
        .unwrap();
    assert!(
        relay_group.ask_every_call && relay_group.asks_in_browser,
        "the relay declares the server of input to ask every call, and its calls wait for a person"
    );
    assert_eq!(
        tools("input-trusted"),
        [
            "click_element_trusted",
            "type_text_trusted",
            "press_key_trusted",
            "hover_element",
            "drag_element"
        ]
    );
    // The second extension holds `debugger`, so this one lists no permission and reads no host
    // for the group, and the group reaches the allowed sites only.
    assert!(permissions("input-trusted").is_empty());
    assert!(!row("input-trusted").hosts);
    let relay_trusted = tools::GROUPS
        .iter()
        .find(|group| group.id == "input-trusted")
        .unwrap();
    assert!(
        relay_trusted.ask_every_call && relay_trusted.asks_in_browser,
        "the relay declares the server of input-trusted to ask every call, and its calls wait for a person"
    );
    // The one tool that takes a picture of a whole page is a group of its own, so that turning it
    // on says what it reads and says nothing about input.
    assert_eq!(tools("capture-trusted"), ["screenshot_full_page"]);
    assert!(permissions("capture-trusted").is_empty());
    assert!(!row("capture-trusted").hosts);
    let relay_capture = tools::GROUPS
        .iter()
        .find(|group| group.id == "capture-trusted")
        .unwrap();
    assert!(
        relay_capture.ask_every_call && relay_capture.asks_in_browser,
        "the relay declares the server of capture-trusted to ask every call, and its calls wait for a person"
    );
    assert_eq!(tools("console-trusted"), ["read_console"]);
    assert!(permissions("console-trusted").is_empty());
    assert!(!row("console-trusted").hosts);
    let relay_console = tools::GROUPS
        .iter()
        .find(|group| group.id == "console-trusted")
        .unwrap();
    assert!(
        relay_console.ask_every_call && relay_console.asks_in_browser,
        "the relay declares the server of console-trusted to ask every call, and its calls wait for a person"
    );
    assert_eq!(tools("upload-trusted"), ["choose_file"]);
    assert!(permissions("upload-trusted").is_empty());
    assert!(!row("upload-trusted").hosts);
    let relay_upload = tools::GROUPS
        .iter()
        .find(|group| group.id == "upload-trusted")
        .unwrap();
    assert!(
        relay_upload.ask_every_call && relay_upload.asks_in_browser,
        "the relay declares the server of upload-trusted to ask every call, and its calls wait for a person"
    );
    for group in &table {
        assert_eq!(
            group.allowed_only,
            group.id == "scripts"
                || group.id == "input-trusted"
                || group.id == "capture-trusted"
                || group.id == "console-trusted"
                || group.id == "upload-trusted",
            "{} is limited to the allowed sites whatever its scope",
            group.id
        );
    }
    assert_eq!(tools("held"), ["take_held"]);
    assert_eq!(permissions("held"), ["contextMenus"]);
    assert!(!row("held").hosts);
    assert!(row("platform").permissions.is_empty() && !row("platform").hosts);
    assert!(row("page-read").hosts);
    assert_eq!(row("tabs-manage").permissions, ["tabs", "tabGroups"]);
    assert!(!row("tabs-manage").hosts);
    assert!(row("network-watch").hosts);
}

/// What a person holds out is given to the extension by the browser, in the entries it adds to the
/// context menu, and the extension reads nothing from a page to make an item (BROWSER-39). Its code
/// for the group calls no tab, script, navigation or window API, sends and receives no message,
/// asks the browser for no permission, and builds no markup; the manifest has no content script, no
/// page that a web page can load and no way for a web page or another extension to message it; the
/// group lists the one permission and no host; and its server is declared like any other, not about
/// every call, and in no preset.
#[test]
fn what_a_person_holds_out_is_given_by_the_browser_and_read_from_no_page() {
    let code = code_of(&extension_file("held.js"));
    for forbidden in [
        "chrome.tabs",
        "chrome.scripting",
        "chrome.webNavigation",
        "chrome.windows",
        "chrome.runtime.sendMessage",
        "chrome.runtime.onMessage",
        "chrome.runtime.connect",
        "chrome.permissions",
        "executeScript",
        "fetch(",
        "document",
        "innerHTML",
        "eval(",
    ] {
        assert!(
            !code.contains(forbidden),
            "held.js uses `{forbidden}`, which reads a page or takes a message"
        );
    }
    let manifest = extension_manifest();
    for key in [
        "content_scripts",
        "web_accessible_resources",
        "externally_connectable",
        "host_permissions",
    ] {
        assert!(manifest.get(key).is_none(), "the manifest has {key}");
    }
    assert!(
        manifest["optional_permissions"]
            .as_array()
            .unwrap()
            .contains(&Value::from("contextMenus"))
            && !manifest["permissions"]
                .as_array()
                .unwrap()
                .contains(&Value::from("contextMenus")),
        "contextMenus is an optional permission and not a required one"
    );

    let table = groups(&extension_file("groups.js"));
    let held = table.iter().find(|group| group.id == "held").unwrap();
    assert_eq!(held.permissions, ["contextMenus"]);
    assert!(!held.hosts);
    assert_eq!(held.tools, ["take_held"]);
    let relay = tools::GROUPS
        .iter()
        .find(|group| group.id == "held")
        .unwrap();
    assert!(!relay.ask_every_call && !relay.asks_in_browser);
    for (name, members) in presets(&extension_file("groups.js")) {
        assert!(
            !members.iter().any(|id| id == "held"),
            "the preset {name} names held"
        );
    }
}

/// The `input` tools name a role, a key, a direction and a button from lists, and the extension
/// refuses a word that is not on its own lists before it touches the page. The relay's schemas
/// carry the same lists, so a planner is offered no word the extension would refuse and none that
/// the extension accepts and a person was not shown. Every tool of the group takes the URL of the
/// tab it acts in, and none takes a selector, a node id or a coordinate.
#[test]
fn the_input_tools_offer_the_lists_of_words_the_extension_accepts() {
    let source = extension_file("words.js");
    let schema = |name: &str| -> Value {
        TOOLS
            .iter()
            .find(|tool| tool.name == name)
            .unwrap()
            .listed()["inputSchema"]
            .clone()
    };
    let words = |schema: &Value, argument: &str| -> Vec<String> {
        schema["properties"][argument]["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("{argument} has no list of words"))
            .iter()
            .map(|word| word.as_str().unwrap().to_string())
            .collect()
    };
    for name in [
        "click_element",
        "type_text",
        "select_option",
        "wait_for_element",
    ] {
        assert_eq!(
            words(&schema(name), "role"),
            quoted_list(&source, "ROLES"),
            "the roles of {name}"
        );
    }
    assert_eq!(
        words(&schema("press_key"), "key"),
        quoted_list(&source, "KEYS")
    );
    assert_eq!(
        words(&schema("scroll_page"), "direction"),
        quoted_list(&source, "DIRECTIONS")
    );
    assert_eq!(
        words(&schema("click_element"), "button"),
        quoted_list(&source, "BUTTONS")
    );
    for tool in TOOLS.iter().filter(|tool| tool.group == "input") {
        let names = argument_names(tool);
        assert!(
            names.contains(&"url".to_string()),
            "{} takes no url",
            tool.name
        );
        for forbidden in ["selector", "id", "node", "x", "y", "tab", "coordinate"] {
            assert!(
                !names.iter().any(|name| name == forbidden),
                "{} takes {forbidden}",
                tool.name
            );
        }
    }
}

/// The relay states the bounds of the list the extension makes: an `element_number` goes up to
/// the most elements a list holds, and the description of `list_page_elements` gives the length
/// of a listed name and how long a list is good for. A bound that drifted would offer a number
/// the extension refuses or promise a list that has been forgotten.
#[test]
fn the_relay_states_the_bounds_of_the_list_the_extension_makes() {
    let source = extension_file("numbering.js");
    let most = exported_number(&source, "MAX_LISTED");
    let name = exported_number(&source, "LISTED_NAME_LIMIT");
    let minutes = exported_number(&source, "MAX_AGE_MS") / 60_000;
    for tool in TOOLS.iter().filter(|tool| tool.group == "input") {
        let schema = tool.listed()["inputSchema"].clone();
        let number = &schema["properties"]["element_number"];
        if number.is_null() {
            continue;
        }
        assert_eq!(number["minimum"], 1, "{}", tool.name);
        assert_eq!(number["maximum"], most, "{}", tool.name);
    }
    let listing = TOOLS
        .iter()
        .find(|tool| tool.name == "list_page_elements")
        .unwrap();
    for phrase in [
        format!("at most {most}"),
        format!("{name} characters"),
        format!("within {minutes} minutes"),
    ] {
        assert!(
            listing.description.contains(&phrase),
            "{phrase}: {}",
            listing.description
        );
    }
}

/// The number an extension file exports as `export const <name> = <digits>;`.
fn exported_number(source: &str, name: &str) -> u64 {
    let line = source
        .lines()
        .find(|line| line.starts_with(&format!("export const {name} = ")))
        .unwrap_or_else(|| panic!("the source does not export {name}"));
    let digits: String = line
        .split('=')
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    digits.parse().unwrap()
}

/// The quoted words an extension file exports as `export const <name> = Object.freeze([...]);`.
fn exported_words(source: &str, name: &str) -> Vec<String> {
    let line = source
        .lines()
        .find(|line| line.starts_with(&format!("export const {name} = Object.freeze([")))
        .unwrap_or_else(|| panic!("the source does not export {name}"));
    let list = line.split("([").nth(1).unwrap().split("])").next().unwrap();
    list.split(',')
        .map(|word| word.trim().trim_matches('"').to_string())
        .collect()
}

/// BROWSER-45: the relay's server looks at the file a `choose_file` call names, and each extension
/// checks the file it is sent, with the same limits and the same list of credential names. A list
/// in one place that lost a name the others have would let the file through where one of them
/// still refused it, and a bound that differed would refuse a file the relay chose. The two
/// extensions hold the same `files.js`, byte for byte.
#[test]
fn the_relay_and_both_extensions_hold_the_same_credential_names_and_the_same_limits_for_a_file() {
    use bravebot_browser::uploads;
    for source in [extension_file("files.js"), trusted_file("files.js")] {
        assert_eq!(
            exported_words(&source, "CREDENTIAL_EXACT"),
            uploads::CREDENTIAL_EXACT
        );
        assert_eq!(
            exported_words(&source, "CREDENTIAL_PREFIXES"),
            uploads::CREDENTIAL_PREFIXES
        );
        assert_eq!(
            exported_words(&source, "CREDENTIAL_SUFFIXES"),
            uploads::CREDENTIAL_SUFFIXES
        );
        assert_eq!(
            exported_number(&source, "PATH_LIMIT"),
            uploads::PATH_LIMIT as u64
        );
        assert!(source.contains("export const MAX_FILE_BYTES = 10 * 1024 * 1024;"));
    }
    assert_eq!(uploads::MAX_BYTES, 10 * 1024 * 1024);
    assert_eq!(extension_file("files.js"), trusted_file("files.js"));
}

/// BROWSER-46 and ROUTE-11: the code of `run_script` starts once and what it sets going in the page
/// keeps running, and the window, the options page, the tool's description and both clauses say it
/// in the same words, since each is where a person decides. One of them left saying the code runs
/// once, or that its world is gone after the call, would tell a person that a timer or a listener
/// the code set stops with the call.
#[test]
fn the_words_that_say_the_code_starts_once_and_what_it_sets_going_stays_are_the_same_where_the_person_decides()
 {
    let stays = "Timers, listeners and observers it sets keep running in the page until the page is \
                 closed or reloaded";
    let normal = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let spec = |name: &str| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/specs")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    };
    let described = tools::named("run_script", Some("scripts"))
        .unwrap()
        .description
        .to_string();
    for (place, text) in [
        ("the window (actions.js)", extension_file("actions.js")),
        (
            "the options page (options.js)",
            extension_file("options.js"),
        ),
        ("the tool's description", described),
        ("BROWSER-46", spec("browser.md")),
        ("ROUTE-11", spec("routing.md")),
    ] {
        let text = normal(&text);
        assert!(text.contains(stays), "{place} does not say: {stays}");
        assert!(
            text.contains("without asking again"),
            "{place} does not say that each can act without asking again"
        );
        for old in ["It runs once", "gone after the call", "spent by one run"] {
            assert!(!text.contains(old), "{place} still says: {old}");
        }
    }
}

/// BROWSER-34: the person has `CONFIRMATION_WAIT_MS` to answer the window of the tools that act on
/// an element, and `wait_for_element` waits up to `MAX_WAIT_SECONDS`. The server of `input` waits
/// for the extension's reply longer than either, with room for finding the element, bringing the
/// tab forward and acting, and the bound the declaration puts on a call is longer than that wait.
/// A relay that gave up first would fail the call while the window still waits, and an action
/// allowed late would then be done after the session was told it failed. A bound the agent
/// reached first would stop the server before it gave its own answer. Both waits are fixed
/// numbers, below the largest bound `bravebot mcp add` takes.
#[test]
fn the_wait_for_input_is_longer_than_the_persons_and_the_declared_bound_is_longer_still() {
    let person = std::time::Duration::from_millis(exported_number(
        &extension_file("approval.js"),
        "CONFIRMATION_WAIT_MS",
    ));
    assert_eq!(
        person.as_secs(),
        60,
        "BROWSER-34 and the options page say 60 seconds"
    );
    let longest_wait = std::time::Duration::from_secs(exported_number(
        &extension_file("input.js"),
        "MAX_WAIT_SECONDS",
    ));
    let room = std::time::Duration::from_secs(15);

    let wait = relay::reply_timeout(Some("input"));
    assert!(
        wait >= person + room,
        "the relay waits {wait:?} for a call to input, which is not {room:?} more than the \
         {person:?} the person has to answer the window"
    );
    assert!(
        wait >= longest_wait + room,
        "the relay waits {wait:?}, which is not {room:?} more than the {longest_wait:?} a wait_for_element may take"
    );
    let bound = relay::IN_BROWSER_TOOL_TIMEOUT;
    assert!(
        bound >= wait + room,
        "a call is bounded at {bound:?} and the relay waits {wait:?}"
    );
    assert!(
        bound <= std::time::Duration::from_secs(3600),
        "{bound:?} is more than `bravebot mcp add --tool-timeout` takes"
    );
    for id in tools::ids().filter(|id| {
        ![
            "scripts",
            "input",
            "input-trusted",
            "capture-trusted",
            "console-trusted",
            "upload-trusted",
            "screen",
        ]
        .contains(id)
    }) {
        assert!(
            relay::reply_timeout(Some(id)) < person,
            "the relay waits as long as a person for {id}"
        );
    }
    // The picker of `screen` is answered in the time the capture window waits, which is the same
    // 60 seconds, and the relay waits as long for it as for `input`.
    let picker = std::time::Duration::from_millis(exported_number(
        &extension_file("screen.js"),
        "CAPTURE_WAIT_MS",
    ));
    assert_eq!(picker, person);
    assert_eq!(relay::reply_timeout(Some("screen")), wait);
    assert!(
        wait >= picker + room,
        "the relay waits {wait:?} for a call to screen, which is not {room:?} more than the \
         {picker:?} the person has to choose"
    );

    let installed = install::Installed {
        manifest: PathBuf::from("/m/com.brave.bravebot.json"),
        directory: PathBuf::from("/d"),
        program: PathBuf::from("/p/bravebot-browser"),
    };
    // `run_script` waits for the person and then for the script, which the extension gives up on
    // after `PAGE_WAIT_MS`, so the relay outwaits both.
    let script = std::time::Duration::from_millis(exported_number(
        &extension_file("tools.js"),
        "PAGE_WAIT_MS",
    ));
    assert_eq!(relay::reply_timeout(Some("scripts")), wait);
    assert!(
        wait >= person + script + room,
        "the relay waits {wait:?} for a call to scripts, which is not {room:?} more than the \
         {person:?} the person has to answer the window and the {script:?} the script may take"
    );
    for group in ["input", "screen", "scripts"] {
        let line = installed.declaration(Some(group));
        let line = line.to_string_lossy();
        assert!(line.contains(" --ask-every-call "), "{line}");
        assert!(
            line.contains(&format!(" --tool-timeout {} ", bound.as_secs())),
            "{line}"
        );
    }
}

/// A call of `input-trusted` passes through four waits, each longer than the one inside it: the
/// person's 60 seconds to answer the window of the second extension, the 80 seconds this
/// extension waits for the second's reply, the 90 seconds the relay waits for this extension's
/// reply, and the 120 seconds the declaration bounds a call at. A wait that ended before the one
/// inside it would report a failure for a call still running, and a window answered late would
/// then act after the session was told it failed. A first extension that waited as long as the
/// relay would leave the relay to answer, with a message that says nothing of which
/// extension was slow.
#[test]
fn the_waits_of_a_trusted_input_call_grow_from_the_persons_window_to_the_declared_bound() {
    let seconds = |milliseconds: u64| std::time::Duration::from_millis(milliseconds);
    let person = seconds(exported_number(
        &trusted_file("approval.js"),
        "CONFIRMATION_WAIT_MS",
    ));
    let first_extension_waits = seconds(exported_number(&extension_file("trusted.js"), "WAIT_MS"));
    let relay_waits = relay::reply_timeout(Some("input-trusted"));
    assert_eq!(relay::reply_timeout(Some("console-trusted")), relay_waits);
    assert_eq!(relay::reply_timeout(Some("upload-trusted")), relay_waits);
    let bound = relay::IN_BROWSER_TOOL_TIMEOUT;
    assert_eq!(
        [
            person.as_secs(),
            first_extension_waits.as_secs(),
            relay_waits.as_secs(),
            bound.as_secs()
        ],
        [60, 80, 90, 120]
    );
    let room = std::time::Duration::from_secs(10);
    assert!(
        person + room < first_extension_waits,
        "the first extension waits {first_extension_waits:?}, which is not {room:?} more than the \
         {person:?} the person has"
    );
    assert!(
        first_extension_waits + room <= relay_waits,
        "the relay waits {relay_waits:?}, which is not {room:?} more than the \
         {first_extension_waits:?} the first extension waits"
    );
    assert!(
        relay_waits + room < bound,
        "a call is bounded at {bound:?} and the relay waits {relay_waits:?}"
    );
    // The picture tool of `capture-trusted` waits as the others of the second extension do, and
    // the second extension's work after the person's answer, which it ends at its own bound, fits
    // in what is left of the 80 seconds.
    assert_eq!(relay::reply_timeout(Some("capture-trusted")), relay_waits);
    let capture = seconds(exported_number(
        &trusted_file("capture.js"),
        "CAPTURE_WAIT_MS",
    ));
    assert_eq!(capture.as_secs(), 10);
    assert!(
        person + capture + std::time::Duration::from_secs(5) <= first_extension_waits,
        "the second extension may take {capture:?} after the {person:?} of the window, and the \
         first extension waits {first_extension_waits:?}"
    );
    // `read_console` reads for at most 10 seconds once the person has allowed it, and that fits
    // in what is left of the 80 seconds with room before the first extension gives up. The
    // picture's wait and this one are each inside the second extension's time after the answer,
    // which is checked below.
    let read = seconds(exported_number(
        &trusted_file("consoles.js"),
        "READ_WAIT_MS",
    ));
    assert_eq!(read.as_secs(), 10);
    assert!(
        person + read + std::time::Duration::from_secs(5) <= first_extension_waits,
        "the first extension waits {first_extension_waits:?}, which is not 5s more than the \
         {person:?} the person has and the {read:?} a read of the console may take"
    );
    // The inputs of the five tools of `input-trusted`: once the person has answered, an input the
    // browser does not answer ends the call after the wait for a dialog and the wait for the
    // answer, and the release sent after that failure waits for a dialog and for a shorter answer.
    // The longest a call whose browser stops answering input can run is the person's window and
    // those four waits, and it leaves 5 seconds before the first extension gives up.
    let dialog = seconds(exported_number(&trusted_file("cdp.js"), "DIALOG_WAIT_MS"));
    let input = seconds(exported_number(&trusted_file("tools.js"), "INPUT_WAIT_MS"));
    let release = seconds(exported_number(&trusted_file("cdp.js"), "RELEASE_WAIT_MS"));
    assert_eq!(
        [dialog.as_secs(), input.as_secs(), release.as_secs()],
        [2, 5, 1]
    );
    assert!(
        release < input,
        "the release after a failure waits {release:?}, which is not less than the {input:?} of an input"
    );
    let retried = person + dialog + input + dialog + release;
    assert_eq!(retried.as_secs(), 70);
    assert!(
        retried + std::time::Duration::from_secs(5) <= first_extension_waits,
        "an input the browser stops answering, and its release, end a call {retried:?} after its \
         window opened, and the first extension waits {first_extension_waits:?}"
    );
    // The steps before the first window have their own time, counted from the start of the call.
    // Whatever the second extension is doing once the person has answered, the call ends at its
    // own deadline, and the end of the call, the release it still owes and the detach, adds the
    // wait for their answers. The waits above each fit inside the deadline, so it cuts none of
    // them short, and the time before the window, the person's window, the deadline and the end
    // of the call leave 2 seconds before the first extension gives up.
    let before = seconds(exported_number(&trusted_file("cdp.js"), "BEFORE_WINDOW_MS"));
    let call = seconds(exported_number(&trusted_file("cdp.js"), "CALL_WAIT_MS"));
    let detach = seconds(exported_number(&trusted_file("cdp.js"), "DETACH_WAIT_MS"));
    assert_eq!(
        [before.as_secs(), call.as_secs(), detach.as_secs()],
        [5, 11, 2]
    );
    assert!(
        release < detach,
        "the release owed at the end of a call waits {release:?}, which leaves the detach none of the {detach:?}"
    );
    assert!(
        dialog + input + dialog + release < call,
        "the deadline of {call:?} is not longer than an input and its release"
    );
    assert!(
        capture < call && read < call,
        "the deadline of {call:?} is not longer than a picture or a read of the console"
    );
    assert!(
        before + person + call + detach + std::time::Duration::from_secs(2)
            <= first_extension_waits,
        "the {before:?} before the window, the window of {person:?}, the {call:?} of a call and the \
         {detach:?} of its end leave the first extension, which waits {first_extension_waits:?}, \
         under 2s before it gives up"
    );
    // The first extension's own window of the person is the same 60 seconds, so a call of `input`
    // and a call of `input-trusted` give the person the same time.
    assert_eq!(
        person,
        seconds(exported_number(
            &extension_file("approval.js"),
            "CONFIRMATION_WAIT_MS"
        ))
    );

    let installed = install::Installed {
        manifest: PathBuf::from("/m/com.brave.bravebot.json"),
        directory: PathBuf::from("/d"),
        program: PathBuf::from("/p/bravebot-browser"),
    };
    for group in ["input-trusted", "capture-trusted", "upload-trusted"] {
        let line = installed.declaration(Some(group));
        let line = line.to_string_lossy();
        assert!(line.contains(" --ask-every-call "), "{group}: {line}");
        assert!(
            line.contains(&format!(" --tool-timeout {} ", bound.as_secs())),
            "{group}: {line}"
        );
    }
}

/// The Chrome Web Store shows the manifest's description and limits it to 132 characters. It says
/// what the extension does, in groups of tools that read, organize, navigate, download and
/// delete, each off until the person turns it on, and not that it only reads.
#[test]
fn the_manifest_description_says_what_the_extension_does_within_the_store_limit() {
    let manifest = extension_manifest();
    let description = manifest["description"].as_str().unwrap();
    assert!(
        description.chars().count() <= 132,
        "{} characters",
        description.chars().count()
    );
    for word in [
        "read",
        "organize",
        "navigate",
        "download",
        "delete",
        "groups of tools",
        "off until you turn it on",
    ] {
        assert!(description.contains(word), "the description lacks {word}");
    }
    let readme = extension_file("README.md");
    let first_paragraph = readme.split("\n\n").nth(1).unwrap();
    for word in [
        "organize",
        "navigate",
        "download",
        "delete",
        "Each group is off until you turn it on",
    ] {
        assert!(
            first_paragraph.contains(word),
            "the README opens without {word}"
        );
    }
}

/// The Chrome Web Store shows the second extension's description too, within 132 characters. It
/// names each kind of tool the extension answers, which are input, a picture of a whole page, the
/// console of a page and a chosen file, and the first extension, whose tools it answers.
#[test]
fn the_second_extensions_description_names_each_kind_of_tool_within_the_store_limit() {
    let manifest = trusted_manifest();
    let description = manifest["description"].as_str().unwrap();
    assert!(
        description.chars().count() <= 132,
        "{} characters",
        description.chars().count()
    );
    for word in [
        "real mouse and keyboard input",
        "picture of a whole page",
        "its console",
        "a chosen file",
        "BraveBot",
        "allow",
    ] {
        assert!(description.contains(word), "the description lacks {word}");
    }
    let readme = trusted_file("README.md");
    let first_paragraph = readme.split("\n\n").nth(1).unwrap();
    for word in [
        "mouse and keyboard input",
        "picture",
        "console",
        "file input",
    ] {
        assert!(
            first_paragraph.contains(word),
            "the README opens without {word}"
        );
    }
}

/// BROWSER-30: a cookie value is a session token, so no tool of the extension reads one or sets a
/// cookie until a maintainer rules that it may. The extension calls two methods of `cookies`,
/// `getAll` to list the cookies a URL is sent and `remove` to delete one, and the two tools that
/// call them name no value, in their bodies or in the relay's table of tools.
#[test]
fn no_tool_reads_a_cookie_value_or_sets_a_cookie() {
    let mut methods = BTreeSet::new();
    for (path, code) in extension_scripts() {
        for line in code
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
        {
            for rest in line.split("chrome.cookies.").skip(1) {
                let method: String = rest
                    .chars()
                    .take_while(|character| character.is_ascii_alphanumeric())
                    .collect();
                assert!(
                    !method.is_empty() && ["getAll", "remove"].contains(&method.as_str()),
                    "{} calls chrome.cookies.{method}",
                    path.display()
                );
                methods.insert(method);
            }
        }
    }
    assert_eq!(
        methods.into_iter().collect::<Vec<_>>(),
        ["getAll", "remove"]
    );

    let source = extension_file("tools.js");
    for tool in ["list_cookie_names", "delete_cookie"] {
        let start = source
            .find(&format!("  async {tool}("))
            .unwrap_or_else(|| panic!("tools.js has no {tool}"));
        let body = &source[start..];
        let body = &body[..body.find("\n  },\n").unwrap()];
        assert!(
            !body
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .any(|word| word == "value"),
            "{tool} names a value"
        );
    }

    let table = groups(&extension_file("groups.js"));
    let cookies = table.iter().find(|group| group.id == "cookies").unwrap();
    assert_eq!(cookies.tools, ["list_cookie_names", "delete_cookie"]);
    for tool in TOOLS {
        let schema = tool.listed()["inputSchema"].clone();
        for argument in schema["properties"].as_object().unwrap().keys() {
            assert!(
                !["value", "cookie", "cookies"].contains(&argument.as_str()),
                "{} takes {argument}",
                tool.name
            );
        }
        assert_eq!(
            tool.name.contains("cookie"),
            cookies.tools.iter().any(|name| name == tool.name),
            "{}",
            tool.name
        );
    }
}

/// The extension asks the browser for the host by the name install gives its manifest.
#[test]
fn the_extension_connects_to_the_host_install_names() {
    let source = extension_file("background.js");
    let named = format!("const HOST = \"{}\";", bravebot_browser::install::HOST_NAME);
    assert!(
        source.contains(&named),
        "background.js does not say {named}"
    );
}

/// The bound the relay's schemas state on a list argument is the one the extension refuses a longer
/// list at (BROWSER-27), so a client is not told a bound the extension does not hold.
#[test]
fn the_bound_the_relay_states_on_a_list_is_the_one_the_extension_enforces() {
    let source = extension_file("tools.js");
    let limit: usize = source
        .lines()
        .find_map(|line| line.strip_prefix("export const LIST_LIMIT = "))
        .and_then(|rest| rest.trim_end_matches(';').parse().ok())
        .expect("extension/tools.js has `export const LIST_LIMIT = <number>;`");
    assert_eq!(tools::LIST_LIMIT, limit);
}
