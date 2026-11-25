//! The sentences `/plugin` says an install, an update and a refusal in, shared by the terminal
//! interface and the desktop bridge so the two say them alike (PLUGIN-1, PLUGIN-9).
//!
//! Only the shape of a refusal is read, never a plugin's own text. Every name, path and key is
//! drawn through the `printable` each front end passes, since the terminal pictures a tab and the
//! window does not.

use crate::plugin_source::NotASource;
use crate::plugins::{ManageRefusal, Refusal, Updated};
use bravebot_config::plugin as manifest;
use bravebot_i18n::t;

/// How a front end draws text a plugin or the person wrote.
pub type Printable = fn(&str) -> String;

/// What an add typed as `source` did: the plugin's name it installed, or why it was refused.
pub fn added(source: &str, result: &Result<String, Refusal>, printable: Printable) -> String {
    let shown = printable(source);
    match result {
        Ok(name) => t!(plugin_added, name = printable(name), path = shown).to_string(),
        Err(refusal) => t!(
            plugin_add_refused,
            path = shown,
            why = refused(refusal, printable)
        )
        .to_string(),
    }
}

/// What an update of the plugin typed as `name` did.
pub fn updated(name: &str, result: &Result<Updated, Refusal>, printable: Printable) -> String {
    let shown = printable(name);
    match result {
        Ok(Updated::Replaced) => t!(plugin_update_done, name = shown).to_string(),
        Ok(Updated::Unchanged {
            newer_commit: false,
        }) => t!(plugin_update_unchanged, name = shown).to_string(),
        Ok(Updated::Unchanged { newer_commit: true }) => {
            t!(plugin_update_unchanged_newer_commit, name = shown).to_string()
        }
        Err(Refusal::Manage(ManageRefusal::BadName)) => {
            t!(plugin_not_a_name, name = shown).to_string()
        }
        Err(Refusal::Manage(ManageRefusal::NotInstalled(_))) => {
            t!(plugin_not_installed, name = shown).to_string()
        }
        Err(Refusal::Declined) => t!(plugin_update_declined, name = shown).to_string(),
        Err(Refusal::NobodyToAsk) => t!(plugin_update_nobody_to_ask, name = shown).to_string(),
        Err(refusal) => t!(
            plugin_update_refused,
            name = shown,
            why = refused(refusal, printable)
        )
        .to_string(),
    }
}

/// Why a remove, enable or disable of the plugin typed as `name` was refused.
pub fn manage_refused(name: &str, refusal: &ManageRefusal, printable: Printable) -> String {
    let shown = printable(name);
    match refusal {
        ManageRefusal::BadName => t!(plugin_not_a_name, name = shown).to_string(),
        ManageRefusal::NotInstalled(_) => t!(plugin_not_installed, name = shown).to_string(),
        ManageRefusal::Io(_) => t!(plugin_manage_failed, name = shown).to_string(),
    }
}

/// The words a refused install or update is said in.
pub fn refused(refusal: &Refusal, printable: Printable) -> String {
    match refusal {
        Refusal::Manifest(manifest::Refusal::NoManifest | manifest::Refusal::NoPluginToml(_)) => {
            t!(plugin_refused_no_manifest).to_string()
        }
        Refusal::Manifest(other) => match other.named_key() {
            Some(key) => t!(plugin_refused_manifest_key, key = printable(&key)).to_string(),
            None => t!(plugin_refused_manifest).to_string(),
        },
        Refusal::PartMissing(part) => t!(
            plugin_refused_part_missing,
            key = manifest::Field::Part(*part).key()
        )
        .to_string(),
        Refusal::NotASource(why) => match why {
            NotASource::Archive => t!(plugin_refused_archive).to_string(),
            NotASource::NpmPackage => t!(plugin_refused_npm_package).to_string(),
            NotASource::PlainUrl => t!(plugin_refused_plain_url).to_string(),
            NotASource::FileUrl => t!(plugin_refused_file_url).to_string(),
            NotASource::UnsupportedScheme => t!(plugin_refused_unsupported_scheme).to_string(),
            NotASource::BadUrl => t!(plugin_refused_bad_url).to_string(),
            NotASource::BadRef => t!(plugin_refused_bad_ref).to_string(),
        },
        Refusal::GitNotInstalled => t!(plugin_refused_git_not_installed).to_string(),
        Refusal::CloneFailed => t!(plugin_refused_clone_failed).to_string(),
        Refusal::Source(_) | Refusal::SourceNotText | Refusal::Io(_) => {
            t!(plugin_refused_unreadable).to_string()
        }
        Refusal::AlreadyInstalled { name, .. } => {
            t!(plugin_refused_already_installed, name = printable(name)).to_string()
        }
        Refusal::SameSource { name, .. } => {
            t!(plugin_refused_same_source, name = printable(name)).to_string()
        }
        Refusal::NameOffered {
            offered,
            new_plugin,
            existing_plugin,
        } => t!(
            plugin_refused_name_offered,
            offered = printable(offered),
            new = printable(new_plugin),
            existing = printable(existing_plugin)
        )
        .to_string(),
        Refusal::Manage(ManageRefusal::NotInstalled(name)) => {
            t!(plugin_not_installed, name = printable(name)).to_string()
        }
        // A bad name holds no text to name, and `updated` says it itself with the name that was
        // typed, so it is not reached from a typed command. An update reports a failed write as
        // `Refusal::Io`, never as `ManageRefusal::Io`.
        Refusal::Manage(ManageRefusal::BadName | ManageRefusal::Io(_)) => {
            t!(plugin_refused_unreadable).to_string()
        }
        Refusal::RecordUnreadable { .. } => t!(plugin_refused_record_unreadable).to_string(),
        Refusal::NothingToUpdateTo { .. } => t!(plugin_refused_nothing_to_update_to).to_string(),
        Refusal::UpdateRenamed { found, .. } => {
            t!(plugin_refused_update_renamed, found = printable(found)).to_string()
        }
        Refusal::NobodyToAsk => t!(plugin_refused_nobody_to_ask).to_string(),
        Refusal::Declined => t!(plugin_refused_declined).to_string(),
    }
}
