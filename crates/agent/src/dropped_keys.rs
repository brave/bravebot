//! The sentences that name a settings key nothing reads, a key a file wrote twice, and a managed
//! `retrieval` value that pins nothing.
//!
//! Each of the three is a restriction, or what reads as one, that its reader dropped or combined
//! without refusing the file (BACKEND-36, RETRIEVE-23). `doctor`, the session in lines, the
//! terminal interface and the desktop window all say them, and they say the same words because they
//! all take the sentence from here, as the sentence for a dropped rule comes from
//! [`crate::permissions::describe`] (PERM-11).
//!
//! A front end names the keys its session dropped through [`at_session_start_of_this_machine`],
//! which reads the machine's managed layer itself and takes no layer. A front end therefore cannot
//! hand the sentences an empty layer or one it read from somewhere else, and the layer-taking form
//! stays private to this module, where it is tested with a managed file of its own. `doctor` loads
//! the layer once for the whole report and calls the pieces below with it.
//!
//! A sentence is built from the key's path and the file's path alone, never from what the key was
//! set to. Both are text a checkout or the administrator chose, so both are spelled for the surface
//! that shows them ([`Spelling`]): a terminal draws each control character as its picture and writes
//! out each character that draws as nothing (LAYER-3), and the desktop window quotes the name as a
//! prompt does (PROMPT-1). The one comparison on a key is against Claude Code's
//! `permissions.disableBypassPermissionsMode`, a key fixed in this source, which picks the sentence
//! that also names the key this build reads.

use bravebot_config::{Managed, Settings};
use bravebot_i18n::t;
use std::path::Path;

/// How the surface that shows a sentence spells the key and the file in it.
///
/// Chosen by the caller from the surface it is, and never from what a key or a path holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spelling {
    /// A terminal: control characters as their pictures and characters that draw as nothing
    /// written out (LAYER-3). `doctor`, the session in lines and the terminal interface.
    Drawn,
    /// The desktop window: a name quoted where it holds a character that draws as nothing or as a
    /// control, as [`crate::confirm::shown_name`] spells one (PROMPT-1).
    Shown,
}

impl Spelling {
    fn key(self, key: &str) -> String {
        match self {
            Self::Drawn => crate::confirm::printable_spelled(key),
            Self::Shown => crate::confirm::shown_text(key),
        }
    }

    fn path(self, path: &Path) -> String {
        match self {
            Self::Drawn => crate::confirm::printable_spelled(&path.display().to_string()),
            Self::Shown => crate::confirm::shown_path(path),
        }
    }
}

/// The sentence for a key nothing reads, naming the file and not the value.
///
/// Claude Code's key for the bypass switch is the one unread key whose writer believes a
/// restriction is in force, so it is said with the key that does the job (BACKEND-36).
#[must_use]
pub fn unread_key(path: &Path, key: &str, spelling: Spelling) -> String {
    let bypass = format!(
        "permissions.{}",
        bravebot_config::Narrowing::CLAUDE_DISABLE_BYPASS
    );
    match key == bypass {
        true => t!(
            doctor_settings_unread_bypass,
            key = spelling.key(key),
            path = spelling.path(path)
        ),
        false => t!(
            doctor_settings_unread_key,
            key = spelling.key(key),
            path = spelling.path(path)
        ),
    }
}

/// One sentence for each key the layers set that nothing reads, weakest layer first.
#[must_use]
pub fn unread_keys(settings: &Settings, spelling: Spelling) -> Vec<String> {
    settings
        .unread_keys()
        .map(|(path, key)| unread_key(path, key, spelling))
        .collect()
}

/// One sentence for each key a settings file, or the managed file, wrote twice in one object: the
/// key's path and the file, never a value (BACKEND-36). The managed file is named as the layer read
/// it, and as the default location where the layer holds no path.
#[must_use]
pub fn repeated_keys(settings: &Settings, managed: &Managed, spelling: Spelling) -> Vec<String> {
    let managed_file = managed
        .path()
        .map_or_else(bravebot_config::managed_file, Path::to_path_buf);
    settings
        .repeated_keys()
        .map(|(path, key)| {
            t!(
                doctor_settings_repeated_key,
                key = spelling.key(key),
                path = spelling.path(path)
            )
        })
        .chain(managed.repeated_keys().map(|key| {
            t!(
                doctor_settings_repeated_key,
                key = spelling.key(key),
                path = spelling.path(&managed_file)
            )
        }))
        .collect()
}

/// The sentence for a managed `retrieval` value that is not a block holding a boolean `enabled`,
/// which pins nothing, or `None` where the file holds none (RETRIEVE-23).
#[must_use]
pub fn managed_retrieval(managed: &Managed, spelling: Spelling) -> Option<String> {
    let path = managed.path().filter(|_| managed.retrieval_unreadable())?;
    Some(t!(
        doctor_managed_retrieval_unreadable,
        path = spelling.path(path)
    ))
}

/// What a session says before the first prompt about the keys and values of the settings in force
/// that its readers dropped or combined: the unread keys, the repeated keys and the managed
/// `retrieval` value that pins nothing, each once, from the managed layer this machine holds.
///
/// The one call a front end makes. It takes no managed layer, so a front end cannot pass an empty one
/// and lose every managed key. A test of a front end puts a managed file in front of this call with
/// `bravebot_config::managed_file_on_this_thread`.
///
/// The managed file's `mcp.allow` and `mcp.deny` entries that match no server are said when the
/// servers are reached ([`crate::mcp::managed_entries_unusable`]), and a dropped permission rule
/// is said from [`crate::permissions::said_in_a_session`], so neither is repeated here.
#[must_use]
pub fn at_session_start_of_this_machine(settings: &Settings, spelling: Spelling) -> Vec<String> {
    at_session_start(settings, &Managed::load(), spelling)
}

/// [`at_session_start_of_this_machine`] for the layer `managed`, which is private so that no front
/// end can choose the layer.
fn at_session_start(settings: &Settings, managed: &Managed, spelling: Spelling) -> Vec<String> {
    let mut said = unread_keys(settings, spelling);
    said.extend(repeated_keys(settings, managed, spelling));
    said.extend(managed_retrieval(managed, spelling));
    said
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory named by the process and the test, removed when dropped.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "bravebot-dropped-keys-{}-{name}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }

        fn file(&self, name: &str, text: &str) -> std::path::PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, text).expect("a file");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn settings_in(path: &Path) -> Settings {
        Settings::layered(None, None, Some(path))
    }

    /// A key nothing reads is named by its path in the block and the file that wrote it, and what
    /// it was set to is not said. A sentence built from the value, or from the key alone, passes
    /// every test of the settings crate and fails this one.
    #[test]
    fn an_unread_key_is_said_with_its_key_path_and_file_and_never_its_value() {
        let scratch = Scratch::new("unread");
        let file = scratch.file(
            "settings.json",
            r#"{"permissions": {"denyy": ["Read(a-rule-nobody-prints)"]},
                "statusLine": "a-command-nobody-prints"}"#,
        );

        let said = unread_keys(&settings_in(&file), Spelling::Drawn);

        assert_eq!(said.len(), 2, "one sentence for each key: {said:?}");
        for key in ["permissions.denyy", "statusLine"] {
            let named: Vec<&String> = said
                .iter()
                .filter(|sentence| sentence.starts_with(&format!("{key} in ")))
                .collect();
            assert_eq!(named.len(), 1, "the key {key} is not named once: {said:?}");
            assert!(
                named[0].contains(&file.display().to_string()),
                "the key {key} is not named with its file: {}",
                named[0]
            );
        }
        let all = said.join("\n");
        assert!(
            !all.contains("a-rule-nobody-prints") && !all.contains("a-command-nobody-prints"),
            "a value is said: {all}"
        );
    }

    /// Claude Code's bypass key is said with the key this build reads for the same purpose, and a
    /// key beside it is not. A sentence that named the pointer for every key, or for none, fails.
    #[test]
    fn the_bypass_key_claude_code_writes_is_said_with_the_key_that_does_its_job() {
        let file = Path::new("settings.json");
        let pointed = unread_key(
            file,
            "permissions.disableBypassPermissionsMode",
            Spelling::Drawn,
        );
        let plain = unread_key(file, "permissions.denyy", Spelling::Drawn);

        assert!(
            pointed.contains("permissions.bypassUnreachable"),
            "{pointed}"
        );
        assert!(!plain.contains("permissions.bypassUnreachable"), "{plain}");
    }

    /// The key and the file of a checkout are drawn on a terminal: an escape in either is a picture
    /// and no control character reaches the terminal (LAYER-3). A sentence that wrote the key as it
    /// was read would hold the escape.
    #[test]
    fn the_key_and_the_file_are_drawn_without_a_control_character() {
        let scratch = Scratch::new("drawn");
        let file = scratch.file("settings.json", "{\"a\\u001b[8mkey\": 1}");
        let settings = settings_in(&file);
        assert_eq!(settings.unread_keys().count(), 1);

        let said = unread_keys(&settings, Spelling::Drawn).join("\n");
        assert!(
            !said.chars().any(|c| c.is_control() && c != '\n'),
            "a control character is in the sentence: {said:?}"
        );
        assert!(
            said.contains('␛'),
            "the escape is not drawn as its picture: {said}"
        );

        let odd = Path::new("dir/a\u{1b}[2Jname.json");
        let said = unread_key(odd, "key", Spelling::Drawn);
        assert!(
            !said.chars().any(char::is_control) && said.contains('␛'),
            "the file's name is not drawn: {said:?}"
        );
    }

    /// The desktop spells a name as a prompt does: a character that draws as nothing in the key or
    /// the file is written out, so two names that draw alike are not said alike (PROMPT-1). A
    /// sentence that quoted nothing would draw the key with the zero-width character invisible.
    #[test]
    fn the_desktop_spells_a_key_and_a_file_that_hold_a_character_that_draws_as_nothing() {
        let said = unread_key(
            Path::new("dir/s\u{200b}ettings.json"),
            "perm\u{200b}issions.denyy",
            Spelling::Shown,
        );

        assert!(!said.contains('\u{200b}'), "{said:?}");
        assert!(said.matches("\\u{200b}").count() >= 2, "{said}");
    }

    /// AUTO-1: `permissions.startInAuto` is read where it stands under `permissions` and is not
    /// named, and the same key written at the top level of the file is a key nothing reads and is
    /// named with the file. A sentence for the first, or none for the second, fails.
    #[test]
    fn a_start_in_auto_key_outside_permissions_is_named_and_the_one_inside_is_not() {
        let scratch = Scratch::new("auto");
        let inside = scratch.file("inside.json", r#"{"permissions": {"startInAuto": true}}"#);
        let outside = scratch.file("outside.json", r#"{"startInAuto": true}"#);

        assert!(unread_keys(&settings_in(&inside), Spelling::Drawn).is_empty());
        let said = unread_keys(&settings_in(&outside), Spelling::Drawn);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].starts_with("startInAuto in ")
                && said[0].contains(&outside.display().to_string()),
            "{said:?}"
        );
    }

    /// A file whose keys are all read says nothing, so a sentence means a key and not a file.
    #[test]
    fn a_file_whose_keys_are_all_read_says_nothing() {
        let scratch = Scratch::new("quiet");
        let file = scratch.file(
            "settings.json",
            r#"{"permissions": {"deny": ["Read(.env)"]}}"#,
        );

        assert!(
            at_session_start(&settings_in(&file), &Managed::default(), Spelling::Drawn).is_empty()
        );
    }

    /// A key a settings file wrote twice and a key the managed file wrote twice are each named with
    /// their own file, and no value is said.
    #[test]
    fn a_repeated_key_is_said_with_the_file_that_wrote_it_and_no_value() {
        let scratch = Scratch::new("repeated");
        let managed_file = scratch.file(
            "managed.json",
            r#"{"models": {"deny": ["a-model-nobody-prints"]}, "models": {}}"#,
        );
        let settings_file = scratch.file(
            "settings.json",
            r#"{"permissions": {"deny": ["Read(a-rule-nobody-prints)"], "deny": []}}"#,
        );

        let said = repeated_keys(
            &settings_in(&settings_file),
            &Managed::at(&managed_file),
            Spelling::Drawn,
        );

        assert_eq!(said.len(), 2, "one sentence for each repeat: {said:?}");
        assert!(
            said[0].contains("permissions.deny")
                && said[0].contains(&settings_file.display().to_string()),
            "{said:?}"
        );
        assert!(
            said[1].contains("models") && said[1].contains(&managed_file.display().to_string()),
            "{said:?}"
        );
        let all = said.join("\n");
        assert!(
            !all.contains("a-model-nobody-prints") && !all.contains("a-rule-nobody-prints"),
            "a value is said: {all}"
        );
    }

    /// A managed `retrieval` value that pins nothing is said with the managed file. The pin
    /// `{"enabled": false}` is not, and neither is a block that says nothing about the switch.
    #[test]
    fn a_managed_retrieval_value_that_pins_nothing_is_said_and_a_pin_is_not() {
        let scratch = Scratch::new("retrieval");
        for (text, said) in [
            (r#"{"retrieval": {"enabled": "false"}}"#, true),
            (r#"{"retrieval": false}"#, true),
            (r#"{"retrieval": {"enabled": false}}"#, false),
            (r#"{"retrieval": {}}"#, false),
        ] {
            let file = scratch.file("managed.json", text);
            let sentence = managed_retrieval(&Managed::at(&file), Spelling::Drawn);
            assert_eq!(sentence.is_some(), said, "{text}: {sentence:?}");
            if let Some(sentence) = sentence {
                assert!(sentence.contains(&file.display().to_string()), "{sentence}");
            }
        }
    }

    /// What a session says at its start is the three kinds in order, each once, and the same
    /// sentences the functions above give, so a front end that calls this says what `doctor` says.
    #[test]
    fn a_session_start_says_each_kind_once_in_doctors_sentences() {
        let scratch = Scratch::new("start");
        let managed_file = scratch.file(
            "managed.json",
            r#"{"retrieval": false, "models": {"deny": []}, "models": {}}"#,
        );
        let settings_file = scratch.file("settings.json", r#"{"statusLine": 1}"#);
        let settings = settings_in(&settings_file);
        let managed = Managed::at(&managed_file);

        let said = at_session_start(&settings, &managed, Spelling::Drawn);

        let mut expected = unread_keys(&settings, Spelling::Drawn);
        expected.extend(repeated_keys(&settings, &managed, Spelling::Drawn));
        expected.extend(managed_retrieval(&managed, Spelling::Drawn));
        assert_eq!(said, expected);
        assert_eq!(said.len(), 3, "{said:?}");
    }
}
