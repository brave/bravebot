//! The hooks file as the agent reads it, for a front end that shows somebody their own.
//!
//! A projection and nothing more. What a hook is, which entries this build can use and whether the
//! file held anything the reader passed over are all answered by the crate a turn fires hooks out
//! of, so an interface offering to edit the file agrees with the turn by construction rather than by
//! two parsers being kept in step.

use crate::protocol::{ErrorCode, Failure};
use bravebot_config::hooks::{self, Declarations};
use serde_json::{Value, json};
use std::path::Path;

/// Where the declarations live, what they say, and whether all of the file was read.
pub fn inspect() -> Result<Value, Failure> {
    // The directory a turn fires hooks out of, so what is shown is what would run.
    let home = bravebot_agent::home::directory().ok_or_else(|| {
        Failure::new(
            ErrorCode::NoHome,
            "this machine names no state directory, so it declares no hooks",
        )
    })?;
    Ok(report(
        &hooks::hooks_file(&home),
        &Declarations::read(&home),
    ))
}

fn report(path: &Path, read: &Declarations) -> Value {
    json!({
        "path": path.display().to_string(),
        "text": read.text(),
        "entire": read.entire(),
        "hooks": read.hooks().declared().iter().map(|hook| json!({
            "on": hook.moment().as_str(),
            "tool": hook.tool(),
            "run": hook.run(),
            "firesForNothing": hook.fires_for_nothing(),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(text: &str) -> (tempfile::TempDir, Value) {
        let home = tempfile::Builder::new()
            .prefix("bravebot-bridge-hooks-")
            .tempdir()
            .expect("a scratch state directory");
        std::fs::write(hooks::hooks_file(home.path()), text).expect("write a hooks file");
        let report = report(
            &hooks::hooks_file(home.path()),
            &Declarations::read(home.path()),
        );
        (home, report)
    }

    /// HOOK-8: the entries a front end draws are the ones the agent read, said in the agent's own
    /// terms including which of them is about a call that never happens.
    #[test]
    fn a_front_end_is_told_the_entries_the_agent_read() {
        let (home, report) = scratch(
            r#"{"hooks": [
                {"on": "tool-finished", "tool": "write_file", "run": ["fmt", "a b"]},
                {"on": "turn-started", "tool": "write_file", "run": ["begin"]}
            ]}"#,
        );

        assert_eq!(
            report["path"],
            json!(hooks::hooks_file(home.path()).display().to_string())
        );
        assert_eq!(report["entire"], json!(true));
        assert_eq!(
            report["hooks"],
            json!([
                {"on": "tool-finished", "tool": "write_file", "run": ["fmt", "a b"], "firesForNothing": false},
                {"on": "turn-started", "tool": "write_file", "run": ["begin"], "firesForNothing": true},
            ])
        );
    }

    /// HOOK-8: a file holding more than the agent read says so, so an editor knows that composing
    /// it back from these entries would drop something somebody wrote.
    #[test]
    fn a_file_the_agent_did_not_wholly_read_says_so() {
        let text = r#"{"hooks": [{"on": "turn-started", "run": ["x"]}], "later": true}"#;
        let (_home, report) = scratch(text);

        assert_eq!(report["entire"], json!(false));
        assert_eq!(report["text"], json!(text), "shown as it is written");
        assert_eq!(report["hooks"].as_array().map(Vec::len), Some(1));
    }
}
