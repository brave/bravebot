// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! `opener.judge`: which of the two session-bus names the desktop app's main process holds lead to
//! a socket a stage may write ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)).
//!
//! The opener Electron starts on Linux (`xdg-open`, and `gio open` under GNOME) finds the session
//! bus from `DBUS_SESSION_BUS_ADDRESS` and otherwise from `XDG_RUNTIME_DIR/bus`. The main process
//! cannot judge them: the directories a stage may write are recorded in this process, where the
//! sessions run. The main process sends the value of each of the two names it holds, and this
//! answers which to delete before it hands an address, a file or a folder to the system. The check
//! is [`bravebot_sandbox::programs::may_be_run`], the one the terminal client's opener and the
//! AWS, gcloud and push-identity starts apply, so the desktop app does not keep a second copy of
//! it.

use bravebot_sandbox::programs::{self, Names, selectors};
use serde_json::{Value, json};
use std::path::Path;

use crate::protocol::{Failure, Request};

/// The names the main process may ask about, with the kind each names.
pub const NAMES: [(&str, Names); 2] = [
    selectors::XDG_RUNTIME_DIR,
    selectors::DBUS_SESSION_BUS_ADDRESS,
];

/// Which names the opener must not be handed, from the values the request carries.
///
/// A name the request does not carry or carries as `null` is not set, and is not in the answer. A
/// value that is not text is a `bad_request`.
pub fn judge(request: &Request) -> Result<Value, Failure> {
    judge_with(request, &programs::outside_stage_writable)
}

/// [`judge`] against a given judgment of which files are outside the directories a stage may write.
fn judge_with(
    request: &Request,
    outside_stage_writable: &impl Fn(&Path) -> bool,
) -> Result<Value, Failure> {
    let mut remove = Vec::new();
    for (name, names) in NAMES {
        match request.param(name) {
            Value::Null => {}
            Value::String(value) => {
                // A Node process reads an environment value that is not UTF-8 with each bad byte
                // replaced by U+FFFD, so a value holding one may be a rendering of other bytes and
                // is not judged as the path it spells (PATH-001).
                if value.contains('\u{FFFD}')
                    || !programs::may_be_run(names, Path::new(value), outside_stage_writable)
                {
                    remove.push(name);
                }
            }
            _ => {
                return Err(Failure::bad_request(format!(
                    "`{name}` must be a string or null"
                )));
            }
        }
    }
    Ok(json!({ "remove": remove }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(params: Value) -> Request {
        Request {
            id: 1,
            method: "opener.judge".to_string(),
            params,
        }
    }

    /// Which names are to be deleted, with only a path under `/safe` outside the directories a
    /// stage may write.
    fn removed(params: Value) -> Vec<String> {
        let answer = judge_with(&asked(params), &|path: &Path| path.starts_with("/safe")).unwrap();
        serde_json::from_value(answer["remove"].clone()).unwrap()
    }

    /// The regressions it rejects: neither name judged (a start that keeps both whatever they
    /// hold), both removed whatever they hold, which takes the person's own bus from Electron, and
    /// the address judged as a path, which no address is.
    #[test]
    fn a_runtime_directory_or_bus_address_that_leads_to_a_socket_a_stage_may_write_is_to_be_deleted()
     {
        assert_eq!(
            removed(json!({
                "XDG_RUNTIME_DIR": "/stage/run",
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=/stage/run/bus",
            })),
            ["XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"]
        );
        assert_eq!(
            removed(json!({
                "XDG_RUNTIME_DIR": "/safe/run",
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=/safe/run/bus;unix:path=/stage/run/bus",
            })),
            ["DBUS_SESSION_BUS_ADDRESS"]
        );
        assert_eq!(
            removed(json!({
                "XDG_RUNTIME_DIR": "run",
                "DBUS_SESSION_BUS_ADDRESS": "unix:abstract=/tmp/dbus-x",
            })),
            ["XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"]
        );
    }

    /// The regression it rejects: a value outside every directory a stage may write deleted, which
    /// takes the session bus from Electron's own portal and file manager calls for a person whose
    /// session was never opened near it.
    #[test]
    fn a_runtime_directory_and_bus_address_outside_every_directory_a_stage_may_write_are_kept() {
        assert!(
            removed(json!({
                "XDG_RUNTIME_DIR": "/safe/run",
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=/safe/run/bus",
            }))
            .is_empty()
        );
        assert!(removed(json!({ "DBUS_SESSION_BUS_ADDRESS": "unix:runtime=yes" })).is_empty());
        assert!(removed(json!({})).is_empty());
        assert!(removed(json!({ "XDG_RUNTIME_DIR": null })).is_empty());
    }

    /// The regression it rejects: a value Node rendered lossily judged as the path it spells, so
    /// `/safe/run\xff` arrives as `/safe/run\u{FFFD}` and is kept as a directory under `/safe`.
    #[test]
    fn a_value_holding_a_replacement_character_is_deleted_whatever_it_spells() {
        assert_eq!(
            removed(json!({
                "XDG_RUNTIME_DIR": "/safe/run\u{FFFD}",
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=/safe/run/bus\u{FFFD}",
            })),
            ["XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"]
        );
    }

    #[test]
    fn a_value_that_is_not_text_is_refused() {
        for bad in [json!(7), json!(["/safe/run"]), json!(true)] {
            let failure = judge_with(&asked(json!({ "XDG_RUNTIME_DIR": bad })), &|_: &Path| true)
                .unwrap_err();
            assert_eq!(failure.code, crate::protocol::ErrorCode::BadRequest);
        }
    }

    /// The regression it rejects: a judgment of this process's own that does not see the directories
    /// the sessions here may write, such as one made against an empty set, so a runtime directory a
    /// session was opened on is kept, and one that treats every directory as writable, so a runtime
    /// directory no session was opened near is deleted. The session directory is one this test
    /// records with `keep_out`. The other is a system directory that no session of this test binary
    /// is opened in or under: a directory under the repository or the temporary directory is one a
    /// stage may write whenever another test of the same process opened a session on the
    /// repository, since the record of such directories is only added to.
    #[test]
    fn the_directories_a_session_here_may_write_are_the_ones_judged() {
        let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch/opener-names");
        let _ = std::fs::remove_dir_all(&scratch);
        let session = scratch.join("session");
        std::fs::create_dir_all(&session).unwrap();
        let session_path = session.canonicalize().unwrap();
        programs::keep_out(&session_path);
        let elsewhere_path = if cfg!(windows) {
            std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap_or("C:\\Windows".into()))
        } else {
            std::path::PathBuf::from("/usr")
        };
        assert!(elsewhere_path.is_dir(), "{elsewhere_path:?}");

        let answer = judge(&asked(json!({
            "XDG_RUNTIME_DIR": session_path.to_str().unwrap(),
        })))
        .unwrap();
        assert_eq!(answer, json!({ "remove": ["XDG_RUNTIME_DIR"] }));
        let answer = judge(&asked(json!({
            "XDG_RUNTIME_DIR": elsewhere_path.to_str().unwrap(),
        })))
        .unwrap();
        assert_eq!(answer, json!({ "remove": [] }));
    }
}
