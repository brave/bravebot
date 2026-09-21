//! Configuration inspection uses the linked agent, never a separately installed CLI.
use crate::protocol::{ErrorCode, Failure};
use bravebot_config::{Config, Settings, Managed};
use serde_json::{Value, json};
use std::path::Path;

pub fn layers(project: Option<&Path>, selected: Option<&Path>) -> Settings {
    Settings::layered(bravebot_agent::home::directory(), project, selected)
}

pub fn config(project: Option<&Path>, selected: Option<&Path>) -> Result<Config, Failure> {
    if let Some(path) = selected { validate(path)?; }
    Config::from_env_and_settings(&layers(project, selected), &Managed::load())
        .map_err(|e| Failure::new(ErrorCode::Config, e.to_string()))
}

pub fn validate(path: &Path) -> Result<(), Failure> {
    let meta = std::fs::metadata(path).map_err(|_| Failure::bad_request("The settings file cannot be opened."))?;
    if !meta.is_file() || meta.len() > 64 * 1024 {
        return Err(Failure::bad_request("Choose a JSON settings file no larger than 64 KB."));
    }
    let text = std::fs::read_to_string(path).map_err(|_| Failure::bad_request("The settings file cannot be read."))?;
    let value: Value = serde_json::from_str(&text).map_err(|_| Failure::bad_request("The settings file is not valid JSON."))?;
    if !value.is_object() { return Err(Failure::bad_request("Settings must be a JSON object.")); }
    Ok(())
}

pub fn report(project: Option<&Path>, selected: Option<&Path>) -> Value {
    let settings = layers(project, selected);
    let managed = Managed::load();
    let configured = config(project, selected);
    let transport = bravebot_net::transport::Transport::shared();
    let providers = configured.as_ref().map(|c| c.providers.iter().map(|p| json!({
        "name": p.display_name(), "credential": match p.credential(|key| std::env::var(key).ok()) {
            bravebot_config::provider::Credential::Token(_) => "configured",
            bravebot_config::provider::Credential::Absent => "missing",
            bravebot_config::provider::Credential::NotNeeded => "not required",
        }
    })).collect::<Vec<_>>()).unwrap_or_default();
    json!({
        "build": crate::agent_build(), "configured": configured.is_ok(),
        "problem": configured.as_ref().err().map(|_| "No usable model service is configured. Choose a gateway, AWS Bedrock, or a configured Brave build."),
        "model": configured.as_ref().ok().map(|c| &c.default_model),
        "bedrock": configured.as_ref().is_ok_and(|c| c.bedrock.is_some()),
        "brave": configured.as_ref().is_ok_and(|c| c.serves_aichat()), "providers": providers,
        "selected": selected, "layers": settings.layers().collect::<Vec<_>>(),
        "overrides": settings.overridden().map(|(name, path)| json!({"name": name, "path": path})).collect::<Vec<_>>(),
        "managed": { "path": managed.path(), "keys": managed.pinned().collect::<Vec<_>>() },
        "network": { "roots": transport.roots().paths(),
            "problem": (!transport.trust_problems().is_empty()).then(|| transport.trust_problems().iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")),
            "trustsNothing": transport.trusts_nothing(), "proxy": transport.proxy_summary(),
            "authenticated": transport.proxy_is_authenticated(), "unusableProxy": transport.unusable_proxy(),
            "noProxy": transport.no_proxy() }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_files_cannot_exceed_the_agents_read_limit() {
        let mut builder = tempfile::Builder::new();
        builder.prefix("bravebot-settings-size-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir().unwrap();
        let root = directory.path().to_path_buf();
        let file = root.join("override.json");
        let mut text = r#"{"model":"selected/model"}"#.to_string();
        text.push_str(&" ".repeat(64 * 1024 - text.len()));
        std::fs::write(&file, &text).unwrap();
        validate(&file).unwrap();
        assert_eq!(Settings::layered(None, None, Some(&file)).model(), Some("selected/model"));
        text.push(' ');
        std::fs::write(&file, text).unwrap();
        assert!(validate(&file).is_err(), "do not accept a file the agent silently ignores");
    }

    #[test]
    fn a_selected_override_merges_after_project_files_without_editing_them() {
        let mut builder = tempfile::Builder::new();
        builder.prefix("bravebot-settings-layers-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir().unwrap();
        let root = directory.path().to_path_buf();
        let home = root.join("home");
        let project = root.join("project");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(project.join(".bravebot")).unwrap();
        std::fs::write(home.join("settings.json"), r#"{"model":"home/model"}"#).unwrap();
        std::fs::write(project.join(".bravebot/settings.json"), r#"{"model":"project/model"}"#).unwrap();
        std::fs::write(project.join(".bravebot/settings.local.json"), r#"{"model":"local/model"}"#).unwrap();
        let selected = root.join("override.json");
        std::fs::write(&selected, r#"{"model":"selected/model"}"#).unwrap();
        validate(&selected).unwrap();
        let settings = Settings::layered(Some(home.clone()), Some(&project), Some(&selected));
        assert_eq!(settings.model(), Some("selected/model"));
        assert_eq!(settings.layers().count(), 4);
        assert_eq!(Settings::layered(Some(home), Some(&project), None).model(), Some("local/model"));
    }
}
