use super::*;
use bravebot_config::keys::{self, Keys};
use bravebot_config::{Settings, brave_key, env_var};

fn scratch(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch/ui-bridge-key-entry")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create scratch");
    path
}

fn configured(settings: &str) -> Result<Config, ConfigError> {
    let mut config = Config::from_lookup(|name| match name {
        env_var::USE_BEDROCK => Some("1".into()),
        env_var::AWS_REGION => Some("us-east-1".into()),
        _ => None,
    })
    .expect("a configuration");
    config.providers = Settings::parse(settings).providers().to_vec();
    Ok(config)
}

fn before(directory: Option<&Path>) -> Before {
    Before {
        incognito: false,
        directory: directory.map(Path::to_path_buf),
        config: Err(ConfigError::Missing("a variable")),
        managed: Managed::default(),
        lookup: Box::new(|_| None),
    }
}

fn openrouter() -> Target {
    Target::Gateway("openrouter".into())
}

/// Each refusal comes before the window asks for a key, so nobody types a key that would not be
/// kept.
#[test]
fn a_key_is_refused_in_order_before_it_is_typed() {
    let directory = scratch("order");
    let incognito = Before {
        incognito: true,
        ..before(Some(&directory))
    };
    for target in [Target::Brave, openrouter()] {
        assert_eq!(
            ready(&target, &incognito).err(),
            Some(t!(desktop_accounts_key_incognito))
        );
        assert_eq!(
            ready(&target, &before(None)).err(),
            Some(t!(desktop_accounts_key_nowhere)),
        );
    }
    let misconfigured = Before {
        config: Err(ConfigError::InvalidEndpoint {
            value: "not an endpoint".into(),
        }),
        ..before(Some(&directory))
    };
    assert_eq!(
        ready(&openrouter(), &misconfigured).err(),
        Some(t!(desktop_accounts_key_misconfigured))
    );
    assert_eq!(
        ready(
            &Target::Gateway("nobody-knows".into()),
            &before(Some(&directory))
        )
        .err(),
        Some(t!(desktop_accounts_key_not_offered))
    );
}

/// Rewriting a key file that can't be read would lose the keys in it.
#[test]
fn an_unreadable_file_is_refused_and_left_as_it_is() {
    let directory = scratch("unreadable");
    std::fs::write(brave_key::file(&directory), "not a key file").expect("written");
    std::fs::write(keys::file(&directory), "not a key file").expect("written");
    let before = before(Some(&directory));
    assert_eq!(
        store(&Target::Brave, &before, Secret::new("k"), true).err(),
        Some(t!(desktop_accounts_key_brave_unreadable))
    );
    assert_eq!(
        store(&openrouter(), &before, Secret::new("k"), true).err(),
        Some(t!(desktop_accounts_keys_unreadable))
    );
    for file in [brave_key::file(&directory), keys::file(&directory)] {
        assert_eq!(
            std::fs::read_to_string(file).expect("read"),
            "not a key file"
        );
    }
}

/// The key replaced still works at the service until revoked there, and is gone from here once the
/// file is written, so it is replaced only on a yes.
#[test]
fn a_brave_key_is_stored_and_one_stored_is_replaced_only_when_asked() {
    let directory = scratch("brave");
    let before = before(Some(&directory));
    assert_eq!(
        store(&Target::Brave, &before, Secret::new(""), false).err(),
        Some(t!(desktop_accounts_key_empty))
    );
    assert!(!brave_key::file(&directory).exists());
    assert_eq!(
        store(&Target::Brave, &before, Secret::new("first"), false),
        Ok(None)
    );
    assert_eq!(
        answer_before(&Target::Brave, &before)["stored"],
        json!(true)
    );
    assert_eq!(
        store(&Target::Brave, &before, Secret::new("second"), false).err(),
        Some(t!(desktop_accounts_key_stored_already))
    );
    let held = |directory: &Path| {
        brave_key::read(directory)
            .expect("readable")
            .map(|key| key.expose().to_string())
    };
    assert_eq!(held(&directory).as_deref(), Some("first"));
    assert_eq!(
        store(&Target::Brave, &before, Secret::new("second"), true),
        Ok(None)
    );
    assert_eq!(held(&directory).as_deref(), Some("second"));
}

/// A key with no block naming its service would never be sent, and the key itself must not end up
/// in the settings file.
#[test]
fn a_known_service_gets_its_block_recorded_with_its_key() {
    let directory = scratch("openrouter");
    let before = before(Some(&directory));
    let asked = answer_before(&openrouter(), &before);
    assert_eq!(asked["ready"], json!(true));
    assert_eq!(asked["stored"], json!(false));
    assert_eq!(asked["records"], json!(true));
    assert_eq!(asked["host"], json!("openrouter.ai"));
    assert_eq!(
        store(&openrouter(), &before, Secret::new("a-key"), false),
        Ok(None)
    );
    let stored = Keys::read(&directory).expect("readable");
    assert_eq!(stored.get("openrouter").map(Secret::expose), Some("a-key"));
    let settings = std::fs::read_to_string(user_settings_file(&directory)).expect("recorded");
    assert!(settings.contains("openrouter"), "{settings}");
    assert!(
        !settings.contains("a-key"),
        "the key is not in the settings"
    );
}

/// Refused before the key is typed, so nobody types one for a service no session would send it to.
#[test]
fn a_block_that_cannot_be_recorded_is_refused_before_the_key() {
    let directory = scratch("cannot-record");
    std::fs::write(user_settings_file(&directory), "[\"not a document\"]").expect("written");
    let before = before(Some(&directory));
    assert_eq!(
        answer_before(&openrouter(), &before),
        json!({ "ready": false, "sentence": t!(desktop_accounts_key_cannot_record) })
    );
    assert!(!keys::file(&directory).exists());
}

/// The page is told where the key goes and which variable overrides it, and only the variable's
/// name may leave the bridge, never its value.
#[test]
fn a_configured_gateway_says_its_host_and_the_variable_that_wins() {
    let directory = scratch("configured");
    let before = Before {
        config: configured(
            r#"{"provider": {"work": {"options": {"baseURL": "https://llm.example.com/v1"}, "env": ["WORK_KEY"]}}}"#,
        ),
        lookup: Box::new(|name| (name == "WORK_KEY").then(|| "set".to_string())),
        ..before(Some(&directory))
    };
    let target = Target::Gateway("work".into());
    let asked = answer_before(&target, &before);
    assert_eq!(asked["host"], json!("llm.example.com"));
    assert_eq!(asked["variable"], json!("WORK_KEY"));
    assert_eq!(asked["records"], json!(false));
    assert!(
        !asked.to_string().contains("set\""),
        "the variable's value is not said"
    );
    assert_eq!(store(&target, &before, Secret::new("k"), false), Ok(None));
    assert!(!user_settings_file(&directory).exists());
}

/// The page names what a key is for, and anything else is refused rather than guessed at.
#[test]
fn a_request_names_brave_or_a_gateway_with_its_id() {
    let request = |params: Value| {
        Request::parse(
            &json!({"id": 1, "method": "accounts.storeKey", "params": params}).to_string(),
        )
        .unwrap_or_else(|_| panic!("a request"))
    };
    assert_eq!(
        target(&request(json!({"way": "brave"}))).ok(),
        Some(Target::Brave)
    );
    assert_eq!(
        target(&request(json!({"way": "gateway", "id": "openrouter"}))).ok(),
        Some(openrouter())
    );
    for refused in [
        json!({"way": "gateway"}),
        json!({"way": "gateway", "id": ""}),
        json!({"way": "chatgpt"}),
    ] {
        assert!(target(&request(refused.clone())).is_err(), "{refused}");
    }
}
