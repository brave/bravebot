//! Configuration inspection uses the linked agent, never a separately installed CLI.
use crate::protocol::{ErrorCode, Failure};
use bravebot_config::{Config, Managed, Narrowing, NotADocument, Settings};
use serde_json::{Value, json};
use std::path::Path;

pub fn layers(project: Option<&Path>, selected: Option<&Path>) -> Settings {
    Settings::layered(bravebot_agent::home::directory(), project, selected)
}

/// Whether a safe verdict releases quarantined content with nobody asked, resolved as the terminal does.
pub fn auto_vetting(settings: &Settings) -> bool {
    resolve_vetting(settings, bravebot_session::store::load_vetting())
}

fn resolve_vetting(settings: &Settings, chosen: Option<bool>) -> bool {
    bravebot_core::vetting::auto(
        bravebot_core::vetting::asked_for(),
        chosen,
        settings.auto_vetting(),
    )
}

pub fn config(project: Option<&Path>, selected: Option<&Path>) -> Result<Config, Failure> {
    if let Some(path) = selected {
        validate(path)?;
    }
    Config::from_env_and_settings(&layers(project, selected), &Managed::load())
        .map_err(|e| Failure::new(ErrorCode::Config, e.to_string()))
}

pub fn validate(path: &Path) -> Result<(), Failure> {
    let meta = std::fs::metadata(path)
        .map_err(|_| Failure::bad_request("The settings file cannot be opened."))?;
    if !meta.is_file() || meta.len() > 64 * 1024 {
        return Err(Failure::bad_request(
            "Choose a JSON settings file no larger than 64 KB.",
        ));
    }
    // Through the agent's own reader rather than a parse here. The file may state a gateway token,
    // and a parse that is dropped rather than cleared leaves the token for the allocator, which is
    // what CRED-23 in docs/specs/credential-protection.md is about.
    bravebot_config::check_document(path).map_err(|reason| match reason {
        NotADocument::Unreadable => Failure::bad_request("The settings file cannot be read."),
        NotADocument::NotJson => Failure::bad_request("The settings file is not valid JSON."),
        NotADocument::NotAnObject => Failure::bad_request("Settings must be a JSON object."),
    })
}

/// The keys a file above the person's own home directory named and the agent did not obey, with
/// the file each came from.
fn ignored(settings: &Settings) -> Vec<Value> {
    let named = |key: &'static str, paths: Vec<&Path>| -> Vec<Value> {
        paths
            .into_iter()
            .map(|path| json!({ "name": key, "path": path }))
            .collect()
    };
    let mut ignored = named("model", settings.model_ignored().collect());
    ignored.extend(named("provider", settings.providers_ignored().collect()));
    ignored.extend(named("advisorModel", settings.advisor_ignored().collect()));
    ignored.extend(named(
        "fallbackModel",
        settings.fallback_ignored().collect(),
    ));
    ignored.extend(named("summaryModel", settings.summary_ignored().collect()));
    ignored.extend(named("agent", settings.agent_ignored().collect()));
    ignored
}

/// What the settings in force say about the two keys that only refuse and the three run limits.
///
/// A limit nobody named is `null`: the built-in figure belongs to the crate that applies it, and
/// answering with one here would be a second copy. A refusal in force is `true` and names the file
/// that asked for it, or `managed` where the administrator's file did.
fn limits(settings: &Settings, managed: &Managed) -> Value {
    let narrowing = settings.narrowing().strictest(managed.narrowing());
    let refusal = |key: &str, asked: Option<bool>| {
        json!({
            "value": asked,
            "path": settings.narrowed_by(key),
            "managed": managed_named(managed, key),
        })
    };
    let deadlines = settings.run_deadlines();
    json!({
        "readsStayInWorkspace": refusal(Narrowing::READS_STAY_IN_WORKSPACE, narrowing.reads_stay_in_workspace),
        "bypassUnreachable": refusal(Narrowing::BYPASS_UNREACHABLE, narrowing.bypass_unreachable),
        "unreadable": settings
            .narrowing_unreadable()
            .map(|(path, key)| json!({ "name": key, "path": path }))
            .collect::<Vec<_>>(),
        "run": {
            "defaultSeconds": deadlines.default.map(|d| d.as_secs()),
            "maxSeconds": deadlines.ceiling.map(|d| d.as_secs()),
            "maxOutput": settings.run_output_cap(),
        },
    })
}

fn managed_named(managed: &Managed, key: &str) -> bool {
    let narrowing = managed.narrowing();
    match key {
        Narrowing::READS_STAY_IN_WORKSPACE => narrowing.reads_stay_in_workspace == Some(true),
        Narrowing::BYPASS_UNREACHABLE => narrowing.bypass_unreachable == Some(true),
        _ => false,
    }
}

pub fn report(project: Option<&Path>, selected: Option<&Path>) -> Value {
    let settings = layers(project, selected);
    let managed = Managed::load();
    let configured = config(project, selected);
    let transport = bravebot_net::transport::Transport::shared();
    let providers = configured
        .as_ref()
        .map(|c| {
            c.providers.iter().map(|p| json!({
        "name": p.display_name(), "credential": match p.credential(|key| std::env::var(key).ok()) {
            bravebot_config::provider::Credential::Token(_) => "configured",
            bravebot_config::provider::Credential::Absent => "missing",
            bravebot_config::provider::Credential::NotNeeded => "not required",
        }
    })).collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "build": crate::agent_build(), "configured": configured.is_ok(),
        "problem": configured.as_ref().err().map(|_| "No usable model service is configured. Choose a gateway, AWS Bedrock, or a configured Brave build."),
        "model": configured.as_ref().ok().map(|c| &c.default_model),
        "bedrock": configured.as_ref().is_ok_and(|c| c.bedrock.is_some()),
        "brave": configured.as_ref().is_ok_and(|c| c.serves_aichat()), "providers": providers,
        "selected": selected, "layers": settings.layers().collect::<Vec<_>>(),
        "overrides": settings.overridden().map(|(name, path)| json!({"name": name, "path": path})).collect::<Vec<_>>(),
        "ignored": ignored(&settings),
        "limits": limits(&settings, &managed),
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
        assert_eq!(
            Settings::layered(None, None, Some(&file)).model(),
            Some("selected/model")
        );
        text.push(' ');
        std::fs::write(&file, text).unwrap();
        assert!(
            validate(&file).is_err(),
            "do not accept a file the agent silently ignores"
        );
    }

    #[test]
    fn a_project_files_model_and_provider_are_reported_as_ignored() {
        let (directory, settings) = vetting_layers(
            Some(r#"{"model":"home/model"}"#),
            Some(r#"{"model":"project/model","provider":{}}"#),
        );
        let project = directory.path().join("project/.bravebot/settings.json");
        assert_eq!(
            ignored(&settings),
            vec![
                json!({ "name": "model", "path": project }),
                json!({ "name": "provider", "path": project }),
            ]
        );
        assert_eq!(
            settings.model(),
            Some("home/model"),
            "the home file's model was not the one obeyed"
        );

        let (_, settings) = vetting_layers(Some(r#"{"model":"home/model"}"#), None);
        assert!(
            ignored(&settings).is_empty(),
            "the person's own file was reported"
        );
    }

    #[test]
    fn a_project_files_advisor_is_reported_as_ignored() {
        let (directory, settings) = vetting_layers(
            Some(r#"{"advisorModel":"home/advisor"}"#),
            Some(r#"{"advisorModel":"project/advisor"}"#),
        );
        let project = directory.path().join("project/.bravebot/settings.json");
        assert_eq!(
            ignored(&settings),
            vec![json!({ "name": "advisorModel", "path": project })]
        );
        assert_eq!(settings.advisor_model(), Some("home/advisor"));
    }

    fn vetting_layers(
        home_text: Option<&str>,
        project_text: Option<&str>,
    ) -> (tempfile::TempDir, Settings) {
        let mut builder = tempfile::Builder::new();
        builder.prefix("bravebot-settings-vetting-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir().unwrap();
        let home = directory.path().join("home");
        let project = directory.path().join("project");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(project.join(".bravebot")).unwrap();
        if let Some(text) = home_text {
            std::fs::write(home.join("settings.json"), text).unwrap();
        }
        if let Some(text) = project_text {
            std::fs::write(project.join(".bravebot/settings.json"), text).unwrap();
        }
        let settings = Settings::layered(Some(home), Some(&project), None);
        (directory, settings)
    }

    #[test]
    fn the_report_names_each_limit_and_the_file_that_asked_for_a_refusal() {
        let (directory, settings) = vetting_layers(
            Some(r#"{"run": {"defaultSeconds": 90, "maxOutput": 4096}}"#),
            Some(
                r#"{"permissions": {"readsStayInWorkspace": true, "bypassUnreachable": "yes"},
                    "run": {"maxSeconds": 120}}"#,
            ),
        );
        let project = directory.path().join("project/.bravebot/settings.json");
        let report = limits(&settings, &Managed::default());
        assert_eq!(
            report["readsStayInWorkspace"],
            json!({ "value": true, "path": project, "managed": false })
        );
        assert_eq!(
            report["bypassUnreachable"],
            json!({ "value": null, "path": null, "managed": false }),
            "a value that is not a boolean is absence, not a refusal in force"
        );
        assert_eq!(
            report["unreadable"],
            json!([{ "name": "bypassUnreachable", "path": project }])
        );
        assert_eq!(
            report["run"],
            json!({ "defaultSeconds": 90, "maxSeconds": 120, "maxOutput": 4096 })
        );

        let (_, none) = vetting_layers(None, None);
        let report = limits(&none, &Managed::default());
        assert_eq!(
            report["run"],
            json!({ "defaultSeconds": null, "maxSeconds": null, "maxOutput": null }),
            "an unnamed limit was reported as a figure"
        );
        assert_eq!(report["readsStayInWorkspace"]["value"], json!(null));
    }

    #[test]
    fn a_managed_false_does_not_take_credit_for_a_refusal_another_file_asked_for() {
        let (directory, settings) = vetting_layers(
            None,
            Some(r#"{"permissions": {"readsStayInWorkspace": true}}"#),
        );
        let managed_path = directory.path().join("home/managed.json");
        std::fs::write(
            &managed_path,
            r#"{"permissions": {"readsStayInWorkspace": false}}"#,
        )
        .unwrap();
        let managed = Managed::at(&managed_path);
        let project = directory.path().join("project/.bravebot/settings.json");
        assert_eq!(
            limits(&settings, &managed)["readsStayInWorkspace"],
            json!({ "value": true, "path": project, "managed": false }),
            "the administrator was named for a refusal their file did not ask for"
        );
    }

    #[test]
    fn auto_vetting_is_off_until_somebody_turns_it_on() {
        let (_dir, settings) = vetting_layers(None, None);
        assert!(!resolve_vetting(&settings, None));
    }

    #[test]
    fn the_home_settings_file_turns_auto_vetting_on() {
        let (_dir, settings) = vetting_layers(Some(r#"{"vetting": {"auto": true}}"#), None);
        assert!(resolve_vetting(&settings, None));
    }

    #[test]
    fn a_checkout_cannot_turn_auto_vetting_on() {
        let (_dir, settings) = vetting_layers(None, Some(r#"{"vetting": {"auto": true}}"#));
        assert!(!resolve_vetting(&settings, None));
    }

    #[test]
    fn a_recorded_choice_outranks_the_home_settings_file() {
        let (_dir, settings) = vetting_layers(Some(r#"{"vetting": {"auto": true}}"#), None);
        assert!(!resolve_vetting(&settings, Some(false)));
        let (_dir, settings) = vetting_layers(Some(r#"{"vetting": {"auto": false}}"#), None);
        assert!(resolve_vetting(&settings, Some(true)));
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
        std::fs::write(
            project.join(".bravebot/settings.json"),
            r#"{"model":"project/model"}"#,
        )
        .unwrap();
        std::fs::write(
            project.join(".bravebot/settings.local.json"),
            r#"{"model":"local/model"}"#,
        )
        .unwrap();
        let selected = root.join("override.json");
        std::fs::write(&selected, r#"{"model":"selected/model"}"#).unwrap();
        validate(&selected).unwrap();
        let settings = Settings::layered(Some(home.clone()), Some(&project), Some(&selected));
        assert_eq!(settings.model(), Some("selected/model"));
        assert_eq!(settings.layers().count(), 4);
        // The checkout's files cannot pick a model (BACKEND-24): both of their words are dropped
        // whole, and the home layer's own answers.
        assert_eq!(
            Settings::layered(Some(home), Some(&project), None).model(),
            Some("home/model")
        );
    }
}
