// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What a client of the local host may ask for.
//!
//! The bridge serves a window that owns the machine it runs on: it lists every session, opens
//! any directory and edits settings. A client of the host gets far less. Every bridge method is
//! either offered here with the parameters it may carry, or named in [`DENIED`]; a method in
//! neither is refused too, so a method added to the bridge is unreachable through the host until
//! someone decides it should not be. `tests/host_allowlist.rs` fails when the bridge has a method
//! this file does not classify.
//!
//! The parameters are checked as well as the method. A parameter the bridge would read but the
//! list does not name is refused, not ignored: `turn.send` carries `prompt` and nothing that
//! attaches a file, and `run.reply` carries no `remember`.

use crate::protocol::{ErrorCode, Failure, Request};

/// A method the host offers, and the parameters a request for it may carry.
pub struct Operation {
    pub method: &'static str,
    pub fields: &'static [&'static str],
}

/// The methods a client of the host may call.
pub const OFFERED: &[Operation] = &[
    Operation {
        method: "agent.info",
        fields: &["session"],
    },
    Operation {
        method: "instance.info",
        fields: &[],
    },
    Operation {
        method: "session.new",
        fields: &["directory"],
    },
    Operation {
        method: "session.view.start",
        fields: &["session", "version"],
    },
    Operation {
        method: "session.close",
        fields: &["session"],
    },
    // The one answer that widens what a session may read that the host's client gives. The client
    // is the program the person named on the host's command line, started over a channel it shares
    // with the host alone, so its answer is the person's own (see the client contract: startup
    // trust is a local-client capability). A client that is not that, such as one reached over a
    // network, is not offered it.
    Operation {
        method: "trust.reply",
        fields: &["session", "trusted"],
    },
    Operation {
        method: "turn.send",
        fields: &["session", "prompt"],
    },
    Operation {
        method: "turn.cancel",
        fields: &["session", "target"],
    },
    Operation {
        method: "confirm.reply",
        fields: &["session", "request", "decision"],
    },
    Operation {
        method: "run.reply",
        fields: &["session", "request", "decision"],
    },
    Operation {
        method: "fetch.reply",
        fields: &["session", "request", "decision"],
    },
    Operation {
        method: "ask.reply",
        fields: &["session", "request", "answers"],
    },
];

/// The offered methods the host answers itself, which the bridge has no method for.
pub const BY_THE_HOST: &[&str] = &["instance.info"];

/// The bridge methods the host does not offer.
///
/// Each group names why. None of them is a method the host cannot support, only one that no
/// client of it needs yet or that acts outside the workspaces the host was given.
pub const DENIED: &[&str] = &[
    // Reach saved sessions, every project's list, and other directories.
    "session.list",
    "session.open",
    "session.fork",
    "session.delete",
    "session.rewind",
    "models.list",
    // Change how the host's own sessions behave, which a person at the host decides.
    "session.mode",
    "session.sandbox",
    "settings.inspect",
    "settings.select",
    "permissions.list",
    "permissions.revoke",
    "hooks.inspect",
    "connectors.list",
    "connectors.preview",
    "connectors.connect",
    "connectors.disconnect",
    "connectors.remove",
    "bot.define",
    "bot.migrate",
    "bot.redefine",
    "doctor",
    // Read files or attach them to a turn.
    "mentions.offer",
    "mentions.named",
    // Work that runs without a request, or a plan walked as one run.
    "watches.list",
    "watches.add",
    "watches.stop",
    "watches.poll",
    "manifest.run",
    "manifest.read",
    "manifest.reply",
    // Answers that widen what a session may do or read beyond `trust.reply` above, which are not
    // offered to any client yet.
    "output.reply",
    "vouch.reply",
    "vet.reply",
    "server.reply",
    "exposure.reply",
    "host.reply",
    "mcp-server.reply",
    "mcp-tools.reply",
    "mcp-call.reply",
    "mcp-move.reply",
];

fn refused(message: String) -> Failure {
    Failure::new(ErrorCode::NotAllowed, message)
}

/// The operation `request` asks for, if the host offers it with these parameters.
pub fn check(request: &Request) -> Result<&'static Operation, Failure> {
    let Some(operation) = OFFERED
        .iter()
        .find(|operation| operation.method == request.method)
    else {
        return Err(refused(format!(
            "the host does not offer `{}`",
            request.method
        )));
    };
    let Some(params) = request.params.as_object() else {
        return Err(refused("the parameters must be an object".into()));
    };
    if let Some(extra) = params
        .keys()
        .find(|name| !operation.fields.contains(&name.as_str()))
    {
        return Err(refused(format!(
            "`{}` does not take `{extra}` on the host",
            request.method
        )));
    }
    Ok(operation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(method: &str, params: serde_json::Value) -> Request {
        Request {
            id: 1,
            method: method.into(),
            params,
        }
    }

    #[test]
    fn an_offered_method_with_its_own_parameters_is_accepted() {
        let asked = request("turn.send", json!({"session": "s1", "prompt": "hi"}));
        assert_eq!(check(&asked).expect("offered").method, "turn.send");
    }

    #[test]
    fn a_method_the_host_does_not_name_is_refused_whether_or_not_the_bridge_has_it() {
        for method in ["session.list", "settings.select", "no.such.method", ""] {
            let failure = check(&request(method, json!({}))).err().expect("refused");
            assert_eq!(failure.code, ErrorCode::NotAllowed, "{method}");
        }
    }

    #[test]
    fn a_parameter_the_method_does_not_name_is_refused() {
        for (method, extra) in [
            ("turn.send", "files"),
            ("turn.send", "model"),
            ("run.reply", "remember"),
            ("trust.reply", "remember"),
            ("session.new", "settings"),
        ] {
            let failure = check(&request(method, json!({ extra: true })))
                .err()
                .expect("refused");
            assert_eq!(failure.code, ErrorCode::NotAllowed, "{method} with {extra}");
        }
    }

    #[test]
    fn parameters_that_are_not_an_object_are_refused() {
        let failure = check(&request("turn.send", json!(["session"])))
            .err()
            .expect("refused");
        assert_eq!(failure.code, ErrorCode::NotAllowed);
    }

    #[test]
    fn no_method_is_both_offered_and_denied() {
        for operation in OFFERED {
            assert!(!DENIED.contains(&operation.method), "{}", operation.method);
        }
    }
}
