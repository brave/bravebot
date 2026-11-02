//! Storing a key a person typed: the Brave key ([PREM-16](../../../docs/specs/premium-credentials.md#PREM-16))
//! and a gateway key, with the block a known service needs before a session sends its key.
//!
//! One implementation for `bravebot auth login` and the desktop window's own key entry. Nothing here
//! is worded: each caller says what a result means in its own words, and no result carries a key.

use crate::accounts::{Offer, SettingsNotWritten, write_keys, write_settings};
use crate::home;
use bravebot_config::import::{Destination, Gateway, Unwritable};
use bravebot_config::keys::{self, Keys};
use bravebot_config::provider::Provider;
use bravebot_config::{Secret, brave_key};
use std::io::Write;
use std::path::Path;

/// Write `key` as the Brave key in `directory`, in place of any there.
///
/// Written beside the file and renamed over it, so a write that stops part way leaves the key that
/// was there, and created private to this account (STATE-1), because the key is a bearer token.
pub fn store_brave_key(directory: &Path, key: &Secret) -> std::io::Result<()> {
    let path = brave_key::file(directory);
    home::create_directory(directory)?;
    let temporary = path.with_file_name(format!("{}.{}.tmp", brave_key::FILE, std::process::id()));
    // Closed at the end of the closure, before the rename, which Windows refuses while a handle is
    // still open on either name.
    let written = bravebot_skus::store::create_private(&temporary)
        .and_then(|mut file| {
            file.write_all(brave_key::to_text(key).expose().as_bytes())?;
            // On disk before the rename, or a crash just after it can leave the name on an empty
            // file where a filesystem orders the rename first.
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temporary, &path));
    if written.is_err() {
        std::fs::remove_file(&temporary).unwrap_or_default();
    }
    written
}

/// Why a gateway key was not stored.
#[derive(Debug)]
pub enum StoreError {
    /// The file of gateway keys cannot be read, so it is left as it is.
    KeysUnreadable,
    /// The file could not be written.
    NotStored(std::io::Error),
}

/// Store `key` for `id` among the gateway keys in `directory`, keeping every other key.
///
/// The file is read again here rather than taken from the caller: a copy read before the person
/// typed is as old as that took, and writing it back would restore a key another command forgot.
pub fn store_gateway_key(directory: &Path, id: &str, key: Secret) -> Result<(), StoreError> {
    let mut stored =
        Keys::read(directory).map_err(|keys::Unreadable| StoreError::KeysUnreadable)?;
    stored.insert(id, key);
    write_keys(directory, &stored).map_err(StoreError::NotStored)
}

/// Why no block naming a known service's id can be added to the person's settings.
#[derive(Debug)]
pub enum BlockError {
    Unwritable(Unwritable),
    /// The file names the id already, as something a block is not: the entry is the person's to write.
    NamesTheId,
}

/// The person's settings at `settings` with a block naming `offer`'s id added, or what stops one.
pub fn with_block(offer: &Offer<'_>, settings: &Path) -> Result<Destination, BlockError> {
    let mut destination = Destination::open(settings).map_err(BlockError::Unwritable)?;
    if destination.holds_gateway(offer.id()) {
        return Err(BlockError::NamesTheId);
    }
    destination.add_gateway(Gateway::bare(offer.id(), &offer.provider.base_url));
    // Past the size the settings reader reads, which a destination says only once it is asked.
    destination.text().map_err(BlockError::Unwritable)?;
    Ok(destination)
}

/// Why the block for a known service was not recorded.
#[derive(Debug)]
pub enum RecordError {
    Block(BlockError),
    Write(SettingsNotWritten),
}

/// Record a block naming `offer`'s id, as `"provider": {"<id>": {}}`, which is the reading that
/// reaches the host compiled in for the id and names no variable and no key.
pub fn record_block(offer: &Offer<'_>, settings: &Path) -> Result<(), RecordError> {
    let destination = with_block(offer, settings).map_err(RecordError::Block)?;
    write_settings(&destination).map_err(RecordError::Write)
}

/// The variable in the block's `env` a session would send instead of a stored key, as BACKEND-16
/// orders them: the first set to something other than space. Only its name leaves here.
pub fn variable_in_force(
    provider: &Provider,
    lookup: impl Fn(&str) -> Option<String>,
) -> Option<&str> {
    provider.env.iter().map(String::as_str).find(|name| {
        lookup(name).is_some_and(|value| {
            let set = !value.trim().is_empty();
            let mut value = value.into_bytes();
            bravebot_config::scrub_bytes(&mut value);
            set
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::provider::known_ids;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch/key-entry")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    fn openrouter() -> Offer<'static> {
        assert!(known_ids().any(|id| id == "openrouter"));
        Offer {
            provider: std::borrow::Cow::Owned(Provider::known("openrouter").expect("known")),
            unconfigured: true,
        }
    }

    #[test]
    fn a_brave_key_is_stored_private_replacing_the_one_there() {
        let directory = scratch("brave");
        store_brave_key(&directory, &Secret::new("first")).expect("stored");
        store_brave_key(&directory, &Secret::new("second")).expect("replaced");
        let read = brave_key::read(&directory)
            .expect("readable")
            .expect("held");
        assert_eq!(read.expose(), "second");
        let leftovers: Vec<_> = std::fs::read_dir(&directory)
            .expect("listed")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "no temporary file is left behind");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(brave_key::file(&directory))
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn a_gateway_key_is_added_beside_the_others_and_an_unreadable_file_is_left() {
        let directory = scratch("gateway");
        store_gateway_key(&directory, "a", Secret::new("one")).expect("stored");
        store_gateway_key(&directory, "b", Secret::new("two")).expect("stored");
        let stored = Keys::read(&directory).expect("readable");
        assert_eq!(stored.get("a").map(Secret::expose), Some("one"));
        assert_eq!(stored.get("b").map(Secret::expose), Some("two"));

        let unreadable = scratch("gateway-unreadable");
        std::fs::write(keys::file(&unreadable), "not json").expect("written");
        assert!(matches!(
            store_gateway_key(&unreadable, "a", Secret::new("one")),
            Err(StoreError::KeysUnreadable)
        ));
        assert_eq!(
            std::fs::read_to_string(keys::file(&unreadable)).expect("read"),
            "not json"
        );
    }

    #[test]
    fn a_block_is_recorded_once_and_not_over_an_entry_the_person_wrote() {
        let directory = scratch("block");
        let settings = directory.join("settings.json");
        let offer = openrouter();
        record_block(&offer, &settings).expect("recorded");
        let text = std::fs::read_to_string(&settings).expect("read");
        assert!(text.contains("openrouter"), "{text}");
        assert!(matches!(
            with_block(&offer, &settings),
            Err(BlockError::NamesTheId)
        ));

        std::fs::write(&settings, "[1, 2]").expect("written");
        assert!(matches!(
            record_block(&offer, &settings),
            Err(RecordError::Block(BlockError::Unwritable(
                Unwritable::NotADocument
            )))
        ));
    }

    #[test]
    fn only_a_variable_set_to_something_is_in_force() {
        let mut provider = Provider::known("openrouter").expect("known");
        provider.env = vec!["EMPTY".into(), "SET".into()];
        let lookup = |name: &str| match name {
            "EMPTY" => Some("  ".to_string()),
            "SET" => Some("value".to_string()),
            _ => None,
        };
        assert_eq!(variable_in_force(&provider, lookup), Some("SET"));
        assert_eq!(variable_in_force(&provider, |_| None), None);
    }
}
