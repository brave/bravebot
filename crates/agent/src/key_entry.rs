//! Storing a key a person typed: the Brave key ([PREM-16](../../../docs/specs/premium-credentials.md#PREM-16))
//! and a gateway key, with the block a known service needs before a session sends its key.
//!
//! One implementation for `bravebot auth login` and the desktop window's own key entry. Nothing here
//! is worded: each caller says what a result means in its own words, and no result carries a key.
//!
//! The checks made before a key is typed are decided here too, so the terminal and the window
//! refuse the same keys in the same order. A caller adds only checks of its own interaction, such
//! as the terminal's need for one.

use crate::accounts::{Offer, SettingsNotWritten, offered, write_keys, write_settings};
use crate::home;
use bravebot_config::import::{Destination, Gateway, Unwritable};
use bravebot_config::keys::{self, Keys};
use bravebot_config::provider::Provider;
use bravebot_config::{Config, ConfigError, Managed, Secret, brave_key, user_settings_file};
use std::path::Path;

/// Why a key is refused before it is typed.
#[derive(Debug)]
pub enum Refusal {
    /// INCOG-7: a session that leaves nothing behind stores no key either.
    Incognito,
    /// The configuration cannot be read, for a reason other than lacking the Brave credentials.
    Misconfigured,
    /// No service a gateway key can be stored for is offered.
    NoneOffered,
    /// The service named is not one offered; these are the ids that are.
    NotOffered(Vec<String>),
    /// There is no state directory to store the key in.
    Nowhere,
    BraveUnreadable,
    /// The file of gateway keys cannot be read, so it is left as it is.
    KeysUnreadable,
    /// A known service no block names needs one recorded, and none can be.
    CannotRecord(BlockError),
    /// The block to record states a project and a location ([`Offer::takes_a_project`]), which only
    /// a caller that asks for them can record. [`ready`] asks for neither, so it refuses.
    NeedsProject,
}

/// The gateways `config` names. A configuration that lacks only the Brave credentials is one that
/// names no gateway and no AWS account, since either makes them optional, so it names none here.
/// Refusing it would stop the person a gateway key is for, who has configured nothing else.
pub fn configured_gateways(
    config: &Result<Config, ConfigError>,
) -> Result<&[Provider], &ConfigError> {
    match config {
        Ok(config) => Ok(&config.providers),
        Err(ConfigError::Missing(_) | ConfigError::Empty(_)) => Ok(&[]),
        Err(other) => Err(other),
    }
}

/// The services a gateway key can be stored for, and which of them `named` is.
pub fn gateway_offers<'c>(
    providers: &'c [Provider],
    managed: &Managed,
    named: Option<&str>,
) -> Result<(Vec<Offer<'c>>, Option<usize>), Refusal> {
    let offers = offered(providers, managed);
    if offers.is_empty() {
        return Err(Refusal::NoneOffered);
    }
    let at = named.map(|id| position(&offers, id)).transpose()?;
    Ok((offers, at))
}

fn position(offers: &[Offer<'_>], id: &str) -> Result<usize, Refusal> {
    offers
        .iter()
        .position(|offer| offer.id() == id)
        .ok_or_else(|| Refusal::NotOffered(offers.iter().map(|o| o.id().to_string()).collect()))
}

/// The directory a Brave key is stored in, and whether one is stored there already.
pub fn brave_place(directory: Option<&Path>) -> Result<(&Path, bool), Refusal> {
    let directory = directory.ok_or(Refusal::Nowhere)?;
    match brave_key::read(directory) {
        Ok(held) => Ok((directory, held.is_some())),
        Err(brave_key::Unreadable) => Err(Refusal::BraveUnreadable),
    }
}

/// The directory a gateway key is stored in, and the keys stored there.
pub fn gateway_place(directory: Option<&Path>) -> Result<(&Path, Keys), Refusal> {
    let directory = directory.ok_or(Refusal::Nowhere)?;
    let stored = Keys::read(directory).map_err(|keys::Unreadable| Refusal::KeysUnreadable)?;
    Ok((directory, stored))
}

/// Whether the block `offer` needs can be recorded in the person's settings under `directory`,
/// checked before the key is typed so nobody types one for a service no session would send it to.
pub fn recordable(offer: &Offer<'_>, directory: &Path) -> Result<(), Refusal> {
    if offer.unconfigured {
        with_block(offer, &user_settings_file(directory)).map_err(Refusal::CannotRecord)?;
    }
    Ok(())
}

/// What a key is entered for.
pub enum Target<'n> {
    Brave,
    Gateway(&'n str),
}

/// What the checks before a key is typed read.
pub struct Inputs<'i> {
    pub incognito: bool,
    /// The state directory, `None` where nothing may be written.
    pub directory: Option<&'i Path>,
    pub config: &'i Result<Config, ConfigError>,
    pub managed: &'i Managed,
}

/// What the checks before a key is typed found.
pub struct Ready<'i> {
    pub directory: &'i Path,
    /// Whether a key is stored already, which a new one replaces only where the person said so.
    pub stored: bool,
    /// For a gateway, the one it is for.
    pub offer: Option<Offer<'i>>,
}

/// Every check before a key is typed, in the order `bravebot auth login` makes them.
///
/// A service whose block states a project and a location is refused last, with
/// [`Refusal::NeedsProject`]: `bravebot auth login` asks for them after these checks and records the
/// block they name, and a caller with no such questions would record the block of express mode in
/// place of the person's choice.
pub fn ready<'i>(target: &Target<'_>, inputs: &Inputs<'i>) -> Result<Ready<'i>, Refusal> {
    if inputs.incognito {
        return Err(Refusal::Incognito);
    }
    match *target {
        Target::Brave => {
            let (directory, stored) = brave_place(inputs.directory)?;
            Ok(Ready {
                directory,
                stored,
                offer: None,
            })
        }
        Target::Gateway(id) => {
            let providers =
                configured_gateways(inputs.config).map_err(|_| Refusal::Misconfigured)?;
            let (mut offers, _) = gateway_offers(providers, inputs.managed, None)?;
            let offer = offers.swap_remove(position(&offers, id)?);
            let (directory, stored) = gateway_place(inputs.directory)?;
            recordable(&offer, directory)?;
            if offer.takes_a_project() {
                return Err(Refusal::NeedsProject);
            }
            Ok(Ready {
                directory,
                stored: stored.get(id).is_some(),
                offer: Some(offer),
            })
        }
    }
}

/// Write `key` as the Brave key in `directory`, in place of any there.
///
/// Written beside the file and renamed over it, so a write that stops part way leaves the key that
/// was there, and created private to this account (STATE-1), because the key is a bearer token.
pub fn store_brave_key(directory: &Path, key: &Secret) -> std::io::Result<()> {
    home::replace_private(directory, brave_key::FILE, brave_key::to_text(key).expose())
}

/// Write `key` as the custom embeddings endpoint's key in `directory`, as [`store_brave_key`]
/// writes Brave's, or remove the one there for `None`.
pub fn store_embeddings_key(directory: &Path, key: Option<&Secret>) -> std::io::Result<()> {
    use bravebot_config::embeddings_key;
    let path = embeddings_key::file(directory);
    let Some(key) = key else {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    };
    home::replace_private(
        directory,
        embeddings_key::FILE,
        embeddings_key::to_text(key).expose(),
    )
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

/// Why Vertex AI cannot be reached with the project and location typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexFault {
    /// The project is not one an endpoint can be built from.
    Project,
    /// The location is not one a host can be built from.
    Location,
}

/// Whether the project a person typed is one a Vertex AI endpoint can be built from, which is
/// checked before the location is asked for. Nothing typed is express mode and is always usable.
pub fn check_vertex_project(project: &str) -> Result<(), VertexFault> {
    match project.is_empty() || bravebot_config::provider::is_vertex_project(project) {
        true => Ok(()),
        false => Err(VertexFault::Project),
    }
}

/// The Vertex AI offer for the `project` and `location` a person typed, each trimmed by the caller
/// and empty where nothing was typed.
///
/// No project is express mode, which needs the key alone and reads no location. A project is
/// checked as [`bravebot_config::provider::vertex_endpoint`] checks it, so a value that would
/// configure no service is refused here before a key is asked for and nothing is written. The
/// refusal names which of the two is wrong and holds neither value.
pub fn google_vertex_offer(project: &str, location: &str) -> Result<Offer<'static>, VertexFault> {
    use bravebot_config::provider::is_vertex_location;
    check_vertex_project(project)?;
    if project.is_empty() {
        return Offer::google_vertex(None, None).ok_or(VertexFault::Project);
    }
    if !location.is_empty() && !is_vertex_location(location) {
        return Err(VertexFault::Location);
    }
    let location = (!location.is_empty()).then_some(location);
    Offer::google_vertex(Some(project), location).ok_or(VertexFault::Project)
}

/// The Vertex AI offer for a project the Google Cloud CLI signs in
/// ([BACKEND-65](../../../docs/specs/backends.md#BACKEND-65)), which no key goes with.
///
/// The project and location are held to the rules the endpoint is built under
/// ([`google_vertex_offer`]), and the project has to be given: no project is express mode, which
/// takes a key and not this sign-in. Nothing typed for the location is `global`, which is where a
/// block that names none is reached.
pub fn google_cloud_offer(project: &str, location: &str) -> Result<Offer<'static>, VertexFault> {
    if project.is_empty() {
        return Err(VertexFault::Project);
    }
    google_vertex_offer(project, location)
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
    destination.add_gateway(Gateway::stating(
        offer.id(),
        &offer.provider.base_url,
        offer.entry.clone(),
    ));
    // Past the size the settings reader reads, which a destination says only once it is asked.
    destination.text().map_err(BlockError::Unwritable)?;
    Ok(destination)
}

/// What taking the block `bravebot auth login google` recorded out of the person's settings did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoogleBlock {
    /// The block was there as sign-in writes it, and is gone.
    Removed,
    /// No block names `google-vertex`.
    NotThere,
    /// A block names it that sign-in did not write as it is, so it is the person's and is left.
    Left,
    /// The block is as sign-in writes it and a key is stored for the id, so the block is what a
    /// session sends that key to and is left.
    LeftForTheKey,
}

/// Whether `entry` is what `bravebot auth login google` records: `options` holding the project and
/// `"location": "global"` and nothing else, so no `env`, `apiKey`, `keyFile`, `baseURL` or header.
/// A block with anything else in it, or another location, is one a person wrote or edited.
pub fn is_recorded_google_block(entry: &serde_json::Map<String, serde_json::Value>) -> bool {
    let Some(options) = entry
        .get("options")
        .and_then(serde_json::Value::as_object)
        .filter(|_| entry.len() == 1)
    else {
        return false;
    };
    options.len() == 2
        && options.get("location").and_then(serde_json::Value::as_str) == Some("global")
        && options
            .get("project")
            .and_then(serde_json::Value::as_str)
            .is_some_and(bravebot_config::provider::is_vertex_project)
}

/// Take the block sign-in with the Google Cloud CLI recorded out of the settings at `settings`,
/// where it is still as it was recorded and no key is stored for it (`key_stored`).
///
/// Nothing is written where there is no block or where it is left, and a missing file is not made.
pub fn forget_google_block(
    settings: &Path,
    key_stored: bool,
) -> Result<GoogleBlock, SettingsNotWritten> {
    use bravebot_config::provider::GOOGLE_VERTEX_ID;
    let mut destination = Destination::open(settings).map_err(SettingsNotWritten::Unwritable)?;
    if !destination.holds_gateway(GOOGLE_VERTEX_ID) {
        return Ok(GoogleBlock::NotThere);
    }
    let recorded = destination
        .gateway_entry(GOOGLE_VERTEX_ID)
        .is_some_and(is_recorded_google_block);
    if !recorded {
        return Ok(GoogleBlock::Left);
    }
    if key_stored {
        return Ok(GoogleBlock::LeftForTheKey);
    }
    destination.remove_gateway(GOOGLE_VERTEX_ID);
    write_settings(&destination)?;
    Ok(GoogleBlock::Removed)
}

/// Why the block for a known service was not recorded.
#[derive(Debug)]
pub enum RecordError {
    Block(BlockError),
    Write(SettingsNotWritten),
}

/// Record the block that names `offer`'s id, which is `"provider": {"<id>": {}}` for a known service,
/// the reading that reaches the host compiled in for the id and names no variable and no key, and
/// for Vertex AI the entry stating the project and location its host is built from.
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
        let path = crate::testutil::scratch_dir("key-entry").join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        path
    }

    fn openrouter() -> Offer<'static> {
        assert!(known_ids().any(|id| id == "openrouter"));
        Offer {
            provider: std::borrow::Cow::Owned(Provider::known("openrouter").expect("known")),
            unconfigured: true,
            entry: serde_json::Map::new(),
        }
    }

    /// The key is a bearer token (STATE-1), so it is kept private to this account, and a write that
    /// stops part way must leave the key that was there.
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

    /// RETRIEVE-35: the custom embeddings key is kept as Brave's is, replaced in place, and removed
    /// by `None`, which is no failure where there is none.
    #[test]
    fn an_embeddings_key_is_stored_private_and_removed() {
        use bravebot_config::embeddings_key;
        let directory = scratch("embeddings");
        store_embeddings_key(&directory, Some(&Secret::new("first"))).expect("stored");
        store_embeddings_key(&directory, Some(&Secret::new("second"))).expect("replaced");
        let read = embeddings_key::read(&directory)
            .expect("readable")
            .expect("held");
        assert_eq!(read.expose(), "second");
        assert!(
            std::fs::read_dir(&directory)
                .expect("listed")
                .filter_map(Result::ok)
                .all(|entry| !entry.file_name().to_string_lossy().ends_with(".tmp")),
            "no temporary file is left behind"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(embeddings_key::file(&directory))
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        store_embeddings_key(&directory, None).expect("removed");
        assert!(!embeddings_key::file(&directory).exists());
        store_embeddings_key(&directory, None).expect("nothing to remove");
    }

    /// One file holds every gateway's key, so adding one must keep the rest, and a file that can't
    /// be read is not rewritten over keys that would be lost.
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

    /// BACKEND-49 and BACKEND-65: the offer for a project the Google Cloud CLI signs in states the
    /// project and `global` for an empty location, and is refused for no project, which is express
    /// mode and takes a key. It refuses what the endpoint refuses, so no second copy of the rule can
    /// accept a value the endpoint cannot be built from.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn a_google_cloud_offer_needs_a_project_the_endpoint_can_be_built_from() {
        let offer = google_cloud_offer("example-project-1", "").expect("offered");
        assert_eq!(
            offer.entry,
            *serde_json::json!({"options": {"project": "example-project-1", "location": "global"}})
                .as_object()
                .expect("an object")
        );
        assert_eq!(offer.provider.host(), "aiplatform.googleapis.com");
        assert!(offer.provider.needs_google_cloud_token(|_| None));
        let regional = google_cloud_offer("example-project-1", "us-east5").expect("offered");
        assert_eq!(
            regional.provider.host(),
            "us-east5-aiplatform.googleapis.com"
        );
        assert_eq!(google_cloud_offer("", "").err(), Some(VertexFault::Project));
        for project in ["a/b", "-p", "p q", "p@q", "p?x", "p#x"] {
            assert_eq!(
                google_cloud_offer(project, "").err(),
                Some(VertexFault::Project),
                "{project:?}"
            );
        }
        assert_eq!(
            google_cloud_offer("example-project-1", "US").err(),
            Some(VertexFault::Location)
        );
    }

    /// The settings file is the person's own, so a block is added only where nothing names the
    /// service, and a file that is not a document is left for them to fix.
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

    /// A session sends the variable rather than the stored key only where it holds a value, so a
    /// blank one must not be said to win.
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

    fn configured(settings: &str) -> Result<Config, ConfigError> {
        let mut config = Config::from_lookup(|name| match name {
            bravebot_config::env_var::USE_BEDROCK => Some("1".into()),
            bravebot_config::env_var::AWS_REGION => Some("us-east-1".into()),
            _ => None,
        })
        .expect("a configuration");
        config.providers = bravebot_config::Settings::parse(settings)
            .providers()
            .to_vec();
        Ok(config)
    }

    fn refusal(target: &Target<'_>, inputs: &Inputs<'_>) -> String {
        match ready(target, inputs) {
            Ok(_) => "ready".into(),
            Err(refused) => format!("{refused:?}"),
        }
    }

    /// The window and `bravebot auth login` must refuse the same key with the same reason.
    #[test]
    fn a_gateway_key_is_refused_in_the_order_the_command_refuses_it() {
        let managed = Managed::default();
        let misconfigured = Err(ConfigError::InvalidEndpoint {
            value: "not an endpoint".into(),
        });
        let lacking = Err(ConfigError::Missing("a variable"));
        let unreadable = scratch("order-unreadable");
        std::fs::write(keys::file(&unreadable), "not json").expect("written");
        let unrecordable = scratch("order-unrecordable");
        std::fs::write(user_settings_file(&unrecordable), "[\"not a document\"]").expect("written");
        let inputs = |incognito, config, directory| Inputs {
            incognito,
            directory,
            config,
            managed: &managed,
        };
        let openrouter = Target::Gateway("openrouter");
        let nobody = Target::Gateway("nobody-knows");
        // Each refusal holds over every one after it.
        assert_eq!(
            refusal(&nobody, &inputs(true, &misconfigured, None)),
            "Incognito"
        );
        assert_eq!(
            refusal(&nobody, &inputs(false, &misconfigured, None)),
            "Misconfigured"
        );
        assert!(refusal(&nobody, &inputs(false, &lacking, None)).starts_with("NotOffered("));
        assert_eq!(
            refusal(&openrouter, &inputs(false, &lacking, None)),
            "Nowhere"
        );
        assert_eq!(
            refusal(&openrouter, &inputs(false, &lacking, Some(&unreadable))),
            "KeysUnreadable"
        );
        assert!(
            refusal(&openrouter, &inputs(false, &lacking, Some(&unrecordable)))
                .starts_with("CannotRecord(")
        );
        let directory = scratch("order-ready");
        let found = ready(&openrouter, &inputs(false, &lacking, Some(&directory))).expect("ready");
        assert!(!found.stored);
        assert!(found.offer.is_some_and(|offer| offer.unconfigured));
    }

    /// Someone with only a gateway configured lacks the Brave credentials, and refusing them would
    /// stop exactly the person the key is for.
    #[test]
    fn a_configuration_lacking_only_credentials_names_no_gateway_and_another_fault_is_refused() {
        for lacking in [
            ConfigError::Missing("a variable"),
            ConfigError::Empty("a variable"),
        ] {
            assert_eq!(
                configured_gateways(&Err(lacking)).map(<[Provider]>::len),
                Ok(0)
            );
        }
        let invalid = || ConfigError::InvalidEndpoint {
            value: "ftp://elsewhere".to_string(),
        };
        assert_eq!(
            configured_gateways(&Err(invalid())).map(<[Provider]>::len),
            Err(&invalid())
        );
        let config = configured(
            r#"{"provider": {"work": {"options": {"baseURL": "https://llm.example.com/v1"}}}}"#,
        );
        let ids: Vec<&str> = configured_gateways(&config)
            .expect("readable")
            .iter()
            .map(|provider| provider.id.as_str())
            .collect();
        assert_eq!(ids, ["work"]);
    }

    /// A gateway the settings already name needs no block recorded, and the window is told a key is
    /// stored for it so it asks before replacing it.
    #[test]
    fn a_configured_gateway_is_offered_with_the_key_stored_for_it() {
        let managed = Managed::default();
        let config = configured(
            r#"{"provider": {"work": {"options": {"baseURL": "https://llm.example.com/v1"}}}}"#,
        );
        let directory = scratch("configured");
        store_gateway_key(&directory, "work", Secret::new("k")).expect("stored");
        let inputs = Inputs {
            incognito: false,
            directory: Some(&directory),
            config: &config,
            managed: &managed,
        };
        let found = ready(&Target::Gateway("work"), &inputs).expect("ready");
        assert!(found.stored);
        assert!(found.offer.is_some_and(|offer| !offer.unconfigured));
    }

    /// A Brave key needs no gateway, so a configuration fault must not refuse it, and the window
    /// and the command must refuse it for the same reason.
    #[test]
    fn a_brave_key_is_refused_in_the_order_the_command_refuses_it() {
        let managed = Managed::default();
        let config = Err(ConfigError::InvalidEndpoint {
            value: "not an endpoint".into(),
        });
        let unreadable = scratch("brave-order-unreadable");
        std::fs::write(brave_key::file(&unreadable), "not a key file").expect("written");
        let inputs = |incognito, directory| Inputs {
            incognito,
            directory,
            config: &config,
            managed: &managed,
        };
        // A configuration that cannot be read stops no Brave key, which is what makes one readable.
        assert_eq!(refusal(&Target::Brave, &inputs(true, None)), "Incognito");
        assert_eq!(refusal(&Target::Brave, &inputs(false, None)), "Nowhere");
        assert_eq!(
            refusal(&Target::Brave, &inputs(false, Some(&unreadable))),
            "BraveUnreadable"
        );
        let directory = scratch("brave-order-ready");
        assert!(
            !ready(&Target::Brave, &inputs(false, Some(&directory)))
                .expect("ready")
                .stored
        );
        store_brave_key(&directory, &Secret::new("k")).expect("stored");
        assert!(
            ready(&Target::Brave, &inputs(false, Some(&directory)))
                .expect("ready")
                .stored
        );
    }

    /// A machine policy that names no gateway leaves nowhere a key could be sent, while with no
    /// policy the known services are still offered.
    #[test]
    fn nothing_is_offered_where_the_machine_names_no_gateway() {
        let directory = scratch("managed-none");
        let file = directory.join("managed.json");
        std::fs::write(&file, r#"{"provider": {}}"#).expect("written");
        let managed = Managed::at(&file);
        assert!(matches!(
            gateway_offers(&[], &managed, None),
            Err(Refusal::NoneOffered)
        ));
        let (offers, at) =
            gateway_offers(&[], &Managed::default(), Some("openrouter")).expect("offered");
        assert_eq!(
            at.and_then(|at| offers.get(at)).map(Offer::id),
            Some("openrouter")
        );
    }

    /// A caller that asks for no project cannot record the block Vertex AI states one in, so it is
    /// refused where no block names the service, and ready where one does. Without the refusal the
    /// desktop window would record the express block for a person who wanted a project. The check
    /// comes after the ones that need no answer, so a file of keys that cannot be read is still the
    /// reason given where both hold.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn a_service_whose_block_states_a_project_is_not_ready_for_a_caller_that_asks_for_none() {
        let managed = Managed::default();
        let lacking = Err(ConfigError::Missing("a variable"));
        let directory = scratch("vertex-needs-a-project");
        let inputs = |config, directory| Inputs {
            incognito: false,
            directory,
            config,
            managed: &managed,
        };
        let vertex = Target::Gateway("google-vertex");
        assert_eq!(
            refusal(&vertex, &inputs(&lacking, Some(&directory))),
            "NeedsProject"
        );
        assert_eq!(refusal(&vertex, &inputs(&lacking, None)), "Nowhere");
        let unreadable = scratch("vertex-needs-a-project-unreadable");
        std::fs::write(keys::file(&unreadable), "not json").expect("written");
        assert_eq!(
            refusal(&vertex, &inputs(&lacking, Some(&unreadable))),
            "KeysUnreadable"
        );
        assert!(!user_settings_file(&directory).exists());

        let named = configured(
            r#"{"provider": {"google-vertex": {"options": {"project": "example-project-1"}}}}"#,
        );
        let found = ready(&vertex, &inputs(&named, Some(&directory))).expect("ready");
        assert!(found.offer.is_some_and(|offer| !offer.unconfigured));
    }

    /// CLI-18: taking out the block `auth login google` recorded removes it where it is still as
    /// written, and only then. A block with a key variable, another host, another location, a header
    /// or any other field is the person's, and so is one a key is stored for; each is left
    /// byte for byte, as is a settings file with no block, and a file that is not a document is
    /// refused untouched. The regression it rejects is removing whatever block names `google-vertex`,
    /// which deletes what a person wrote.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn the_block_google_sign_in_recorded_is_taken_out_only_where_it_is_still_as_recorded() {
        let recorded = r#"{"model": "opus", "provider": {"other": {}, "google-vertex": {"options": {"project": "example-project-1", "location": "global"}}}}"#;
        let directory = scratch("forget-google");
        let file = user_settings_file(&directory);
        std::fs::write(&file, recorded).expect("written");
        assert_eq!(
            forget_google_block(&file, false).expect("removed"),
            GoogleBlock::Removed
        );
        let left: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).expect("read")).expect("json");
        assert_eq!(
            left,
            serde_json::json!({"model": "opus", "provider": {"other": {}}})
        );
        assert_eq!(
            forget_google_block(&file, false).expect("nothing"),
            GoogleBlock::NotThere
        );

        for (how, entry) in [
            (
                "a key variable",
                r#"{"env": ["GOOGLE_API_KEY"], "options": {"project": "example-project-1", "location": "global"}}"#,
            ),
            (
                "another location",
                r#"{"options": {"project": "example-project-1", "location": "us-east5"}}"#,
            ),
            (
                "another host",
                r#"{"options": {"project": "example-project-1", "location": "global", "baseURL": "https://proxy.example.invalid/v1"}}"#,
            ),
            (
                "a key file",
                r#"{"options": {"project": "example-project-1", "location": "global", "keyFile": "/k"}}"#,
            ),
            ("express mode", "{}"),
            (
                "a project an endpoint cannot be built from",
                r#"{"options": {"project": "a/b", "location": "global"}}"#,
            ),
            ("not a block", "5"),
        ] {
            let text = format!(r#"{{"provider": {{"google-vertex": {entry}}}}}"#);
            std::fs::write(&file, &text).expect("written");
            assert_eq!(
                forget_google_block(&file, false).expect(how),
                GoogleBlock::Left,
                "{how}"
            );
            assert_eq!(std::fs::read_to_string(&file).expect("read"), text, "{how}");
        }

        std::fs::write(&file, recorded).expect("written");
        assert_eq!(
            forget_google_block(&file, true).expect("left"),
            GoogleBlock::LeftForTheKey
        );
        assert_eq!(std::fs::read_to_string(&file).expect("read"), recorded);

        std::fs::write(&file, "[1, 2]").expect("written");
        assert!(forget_google_block(&file, false).is_err());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "[1, 2]");
        let missing = directory.join("nothing").join("settings.json");
        assert_eq!(
            forget_google_block(&missing, false).expect("nothing"),
            GoogleBlock::NotThere
        );
        assert!(!missing.exists(), "a file was made to take nothing out of");
    }
}
