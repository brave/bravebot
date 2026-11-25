//! Plugins: the installed list, turning one on or off, and removing one, from the window.
//!
//! A projection over `bravebot_agent::plugins` and nothing more. What is installed is read from the
//! records alone, so listing hashes nothing (PLUGIN-9), and turning a plugin on or off or removing
//! it asks nothing (PLUGIN-11). A refusal is said in the words `/plugin` says it in.

// The install listing and the update difference the add and update job puts to the window. The job
// comes in a later change; until then only the listing's tests use it.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the add and update job that puts the listing is not wired yet"
    )
)]
mod listing;

use crate::protocol::{ErrorCode, Failure, Request};
use bravebot_agent::confirm::printable_spelled;
use bravebot_agent::plugin_source::Source;
use bravebot_agent::plugins::{self, Installed, ManageRefusal};
use bravebot_i18n::t;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Every installed plugin, as its record says it.
pub fn list() -> Result<Value, Failure> {
    let home = state()?;
    Ok(listed(&home, bravebot_agent::home::writable().is_some()))
}

/// Turn the plugin `name` on or off, and say what is installed afterwards.
pub fn set_enabled(request: &Request, enabled: bool) -> Result<Value, Failure> {
    let home = writable_state()?;
    let name = request.string("name")?;
    enabled_in(&home, &name, enabled)
}

/// Delete the plugin `name` and its record, and say what is installed afterwards.
pub fn remove(request: &Request) -> Result<Value, Failure> {
    let home = writable_state()?;
    let name = request.string("name")?;
    removed_in(&home, &name)
}

fn listed(home: &Path, writable: bool) -> Value {
    json!({
        "state": home.display().to_string(),
        "writable": writable,
        "plugins": plugins::list(home).iter().map(drawn).collect::<Vec<_>>(),
    })
}

fn enabled_in(home: &Path, name: &str, enabled: bool) -> Result<Value, Failure> {
    plugins::set_enabled(home, name, enabled).map_err(|refusal| refused(name, &refusal))?;
    Ok(listed(home, true))
}

fn removed_in(home: &Path, name: &str) -> Result<Value, Failure> {
    plugins::remove(home, name).map_err(|refusal| refused(name, &refusal))?;
    Ok(listed(home, true))
}

/// One record as the window draws it. Every field but `enabled` is text the plugin or the person
/// wrote; the window draws it as such.
fn drawn(plugin: &Installed) -> Value {
    let (requested_ref, commit) = match &plugin.source {
        Source::Local(_) => (None, None),
        Source::Git {
            requested_ref,
            commit,
            ..
        } => (requested_ref.clone(), Some(commit.as_str().to_string())),
    };
    json!({
        "name": plugin.name,
        "version": plugin.version,
        "source": plugin.source.to_string(),
        "requestedRef": requested_ref,
        "commit": commit,
        "digest": plugin.digest,
        "enabled": plugin.enabled,
    })
}

/// The sentence `/plugin remove`, `enable` and `disable` say a refusal in. The name is the one the
/// window sent, drawn through [`printable_spelled`].
fn refused(name: &str, refusal: &ManageRefusal) -> Failure {
    let shown = printable_spelled(name);
    match refusal {
        ManageRefusal::BadName => {
            Failure::bad_request(t!(plugin_not_a_name, name = shown).to_string())
        }
        ManageRefusal::NotInstalled(_) => {
            Failure::bad_request(t!(plugin_not_installed, name = shown).to_string())
        }
        ManageRefusal::Io(_) => Failure::new(
            ErrorCode::Internal,
            t!(plugin_manage_failed, name = shown).to_string(),
        ),
    }
}

fn state() -> Result<PathBuf, Failure> {
    bravebot_agent::home::directory()
        .ok_or_else(|| Failure::new(ErrorCode::NoHome, t!(plugin_command_no_home).to_string()))
}

/// The state directory, where this run may write it, which an incognito one may not.
fn writable_state() -> Result<PathBuf, Failure> {
    let home = state()?;
    bravebot_agent::home::writable()
        .map(|_| home)
        .ok_or_else(|| Failure::bad_request(t!(plugin_manage_not_writable).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn scratch() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("bravebot-bridge-plugins-")
            .tempdir()
            .expect("a scratch state directory")
    }

    /// An installed plugin as `commit_install` leaves one: its directory and the record beside it.
    fn installed(home: &Path, name: &str, record: &str) {
        let plugins = home.join("plugins");
        std::fs::create_dir_all(plugins.join(name).join("skills")).unwrap();
        std::fs::write(plugins.join(name).join("plugin.toml"), "name = \"x\"\n").unwrap();
        std::fs::write(plugins.join(format!("{name}.record")), record).unwrap();
    }

    fn local(home: &Path, name: &str) {
        installed(
            home,
            name,
            "source = /src/local\nversion = 1.0.0\nenabled = true\ndigest = abc\n",
        );
    }

    fn git(home: &Path, name: &str) {
        installed(
            home,
            name,
            &format!(
                "source = https://example.com/p.git\nrequested-ref = v2\ncommit = {COMMIT}\n\
                 version = 2.3.4\nenabled = false\ndigest = def\n"
            ),
        );
    }

    /// PLUGIN-9: the list is the records' fields, and only a git source has a commit or a ref.
    #[test]
    fn the_list_is_what_each_record_says_and_only_git_has_a_commit() {
        let home = scratch();
        local(home.path(), "alpha");
        git(home.path(), "beta");

        let shown = listed(home.path(), true);

        assert_eq!(shown["state"], json!(home.path().display().to_string()));
        assert_eq!(shown["writable"], json!(true));
        assert_eq!(
            shown["plugins"],
            json!([
                {"name": "alpha", "version": "1.0.0", "source": "/src/local", "requestedRef": null,
                 "commit": null, "digest": "abc", "enabled": true},
                {"name": "beta", "version": "2.3.4", "source": "https://example.com/p.git",
                 "requestedRef": "v2", "commit": COMMIT, "digest": "def", "enabled": false},
            ])
        );
    }

    #[test]
    fn nothing_installed_is_an_empty_list() {
        let home = scratch();
        assert_eq!(listed(home.path(), false)["plugins"], json!([]));
    }

    /// PLUGIN-11: disabling then enabling changes the enabled state and nothing else in the record.
    #[test]
    fn disabling_then_enabling_flips_only_enabled() {
        let home = scratch();
        git(home.path(), "beta");
        let record = home.path().join("plugins").join("beta.record");
        let before = listed(home.path(), true)["plugins"][0].clone();

        let enabled = enabled_in(home.path(), "beta", true).expect("enabled");
        let mut expected = before.clone();
        expected["enabled"] = json!(true);
        assert_eq!(enabled["plugins"], json!([expected]));

        let disabled = enabled_in(home.path(), "beta", false).expect("disabled");
        assert_eq!(disabled["plugins"], json!([before]));
        assert!(home.path().join("plugins").join("beta").is_dir());
        assert!(
            std::fs::read_to_string(&record)
                .unwrap()
                .contains(&format!("commit = {COMMIT}"))
        );
    }

    /// PLUGIN-9: removing deletes the directory and the record, and the answer lists the rest.
    #[test]
    fn removing_deletes_the_directory_and_the_record() {
        let home = scratch();
        local(home.path(), "alpha");
        git(home.path(), "beta");

        let after = removed_in(home.path(), "alpha").expect("removed");

        let plugins = home.path().join("plugins");
        assert!(!plugins.join("alpha").exists());
        assert!(!plugins.join("alpha.record").exists());
        assert!(plugins.join("beta").is_dir());
        assert_eq!(after["plugins"].as_array().map(Vec::len), Some(1));
        assert_eq!(after["plugins"][0]["name"], json!("beta"));
    }

    /// A name that is not a plugin's, or one not installed, is refused in `/plugin`'s sentence and
    /// changes nothing. The name is drawn printable, so a control character in it is pictured.
    #[test]
    fn a_bad_or_missing_name_is_refused_in_a_sentence() {
        let home = scratch();
        local(home.path(), "alpha");

        let bad = enabled_in(home.path(), "../alpha\u{1b}", false).expect_err("bad name");
        assert_eq!(bad.code, ErrorCode::BadRequest);
        assert_eq!(
            bad.message,
            t!(
                plugin_not_a_name,
                name = printable_spelled("../alpha\u{1b}")
            )
            .to_string()
        );
        assert!(!bad.message.contains('\u{1b}'));

        let missing = removed_in(home.path(), "gamma").expect_err("not installed");
        assert_eq!(missing.code, ErrorCode::BadRequest);
        assert_eq!(
            missing.message,
            t!(plugin_not_installed, name = "gamma").to_string()
        );
        let missing = enabled_in(home.path(), "gamma", true).expect_err("not installed");
        assert_eq!(
            missing.message,
            t!(plugin_not_installed, name = "gamma").to_string()
        );

        assert_eq!(
            listed(home.path(), true)["plugins"][0]["enabled"],
            json!(true)
        );
    }
}
