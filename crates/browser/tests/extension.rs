//! The extension in `extension/` held to the relay's side of the contract, by reading its files.
//!
//! The extension is JavaScript and its behaviour is tested by Node, in `extension/tests/`. What is
//! checked here is what the two halves have to agree on: the id `install` records, and the names of
//! the tools the server offers and the extension answers.

#![cfg(unix)]
#![forbid(unsafe_code)]

use base64::Engine;
use bravebot_browser::install::EXTENSION_ID;
use bravebot_browser::tools::TOOLS;
use serde_json::Value;
use sha2::{Digest, Sha256};
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

/// The names inside `export const <name> = {` ... `};` in tools.js, each being the word before a
/// `:` or a `(`, one member to a line.
fn members(source: &str, object: &str) -> Vec<String> {
    let opening = format!("export const {object} = ");
    let start = source
        .find(&opening)
        .unwrap_or_else(|| panic!("tools.js has no `{opening}`"));
    let body = &source[start..];
    // The nearer of the two ways an object ends, since a later `});` belongs to other code.
    let end = ["\n});", "\n};"]
        .iter()
        .filter_map(|ending| body.find(ending))
        .min()
        .unwrap_or_else(|| panic!("`{opening}` has no end"));
    body[..end]
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
    let der = base64::engine::general_purpose::STANDARD
        .decode(key)
        .unwrap();
    let digest = Sha256::digest(&der);
    let id: String = digest[..16]
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .map(|half| char::from(b'a' + half))
        .collect();
    assert_eq!(id, EXTENSION_ID);
}

/// Each tool the server offers calls the extension method of the same name, so the extension
/// answers exactly those, and a person has a switch for each of them in its options.
#[test]
fn the_extension_answers_every_tool_the_server_offers_and_no_other() {
    let source = extension_file("tools.js");
    let offered: Vec<&str> = TOOLS.iter().map(|tool| tool.name).collect();
    assert_eq!(members(&source, "TOOLS"), offered);
    assert_eq!(members(&source, "DEFAULT_SETTINGS"), offered);
}

/// A tool that lists frames needs the browser's frame-navigation records, so the extension asks for
/// that capability alongside the capabilities its other tools use.
#[test]
fn the_extension_has_each_permission_its_tools_require() {
    let manifest = extension_manifest();
    let permissions = manifest["permissions"].as_array().unwrap();
    for permission in [
        "nativeMessaging",
        "tabs",
        "scripting",
        "history",
        "bookmarks",
        "storage",
        "alarms",
        "webNavigation",
    ] {
        assert!(permissions.contains(&Value::String(permission.into())));
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
