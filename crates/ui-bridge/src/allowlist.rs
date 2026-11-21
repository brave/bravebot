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
//! The list is for a client that is the person's own program, started over a channel it shares with
//! the host alone. It offers answers that grant trust, run a command and fetch a page, so a client
//! reached any other way, such as over a network, needs a list of its own and not this one. Each
//! client gives only the answers the person named for it on the host's command line ([`Answer`]),
//! and a reply of any other kind is refused ([`check_answer`]).
//!
//! The parameters are checked as well as the method. A parameter the bridge would read but the
//! list does not name is refused, not ignored: `turn.send` carries `prompt` and nothing that
//! attaches a file, and `run.reply` carries no `remember`.

use crate::protocol::{ErrorCode, Failure, Request};
use crate::turn::Kind;

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
    // The release notes compiled into this program. Reads nothing and needs no session.
    Operation {
        method: "changelog.read",
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
    // A new snapshot of a session's view, for a client that missed an update.
    Operation {
        method: "session.view.resync",
        fields: &["session"],
    },
    Operation {
        method: "session.close",
        fields: &["session"],
    },
    // Joins a session another connection opened. The host answers it itself.
    Operation {
        method: "session.attach",
        fields: &["session"],
    },
    // The one answer that widens what a session may read that a client of the host gives, and only
    // a client the person gave `trust` on the host's command line ([`Answer::Trust`]). Such a client
    // is a program the person named, started over a channel it shares with the host alone, so its
    // answer is the person's own (see the client contract: startup trust is a local-client
    // capability). A client that is not that, such as one reached over a network, is not offered it.
    Operation {
        method: "trust.reply",
        fields: &["session", "trusted"],
    },
    Operation {
        method: "turn.send",
        fields: &["session", "prompt", "messageId"],
    },
    Operation {
        method: "turn.lookup",
        fields: &["session", "messageId"],
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

/// The offered methods the host answers itself in `Host::dispatch`, which the bridge has no method for.
pub const BY_THE_HOST: &[&str] = &["instance.info", "session.attach"];

/// A kind of answer a client of the host may give. The person names each client's answers on the
/// host's command line (`--answers`), and nothing a client sends changes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Answer {
    Confirm,
    Run,
    Fetch,
    Ask,
    /// The startup question about the session's directory.
    Trust,
}

impl Answer {
    pub const ALL: [Self; 5] = [
        Self::Confirm,
        Self::Run,
        Self::Fetch,
        Self::Ask,
        Self::Trust,
    ];

    /// The word the command line and `instance.info` use.
    pub fn word(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Run => "run",
            Self::Fetch => "fetch",
            Self::Ask => "ask",
            Self::Trust => "trust",
        }
    }

    /// The method that gives this answer.
    fn reply(self) -> &'static str {
        match self {
            Self::Confirm => "confirm.reply",
            Self::Run => "run.reply",
            Self::Fetch => "fetch.reply",
            Self::Ask => "ask.reply",
            Self::Trust => "trust.reply",
        }
    }

    /// The question of a turn this answers. The startup question is not one a turn asks.
    fn question(self) -> Option<Kind> {
        match self {
            Self::Confirm => Some(Kind::Write),
            Self::Run => Some(Kind::Run),
            Self::Fetch => Some(Kind::Fetch),
            Self::Ask => Some(Kind::Ask),
            Self::Trust => None,
        }
    }

    /// The answer `method` gives, where it is one.
    pub fn given_by(method: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|answer| answer.reply() == method)
    }

    /// The answers a comma-separated list names, in the order of [`Answer::ALL`]. `none` alone is
    /// no answer at all. An empty list, an unknown word or one named twice is refused, so a typing
    /// mistake does not quietly give a client fewer or more answers than the person meant.
    pub fn list(words: &str) -> Result<Vec<Self>, String> {
        if words == "none" {
            return Ok(Vec::new());
        }
        let mut answers = Vec::new();
        for word in words.split(',') {
            let Some(answer) = Self::ALL.into_iter().find(|answer| answer.word() == word) else {
                return Err(format!(
                    "`{word}` is not an answer; name some of confirm, run, fetch, ask and trust, \
                     separated by commas, or none"
                ));
            };
            if answers.contains(&answer) {
                return Err(format!("`{word}` is named twice"));
            }
            answers.push(answer);
        }
        answers.sort();
        Ok(answers)
    }
}

/// The kinds of question a client that gives `answers` is asked: the ones whose reply the host
/// offers and that client may give.
///
/// Derived from [`OFFERED`] so that offering a reply method and being asked its question are one
/// decision. `trust.reply` gives none: the startup question is not one a turn asks.
pub fn answerable(answers: &[Answer]) -> Vec<Kind> {
    OFFERED
        .iter()
        .filter_map(|operation| Answer::given_by(operation.method))
        .filter(|answer| answers.contains(answer))
        .filter_map(Answer::question)
        .collect()
}

/// Refuse a reply that gives an answer the connection may not give. Any other request passes.
pub fn check_answer(request: &Request, answers: &[Answer]) -> Result<(), Failure> {
    match Answer::given_by(&request.method) {
        Some(answer) if !answers.contains(&answer) => Err(refused(format!(
            "this client may not give `{}` answers",
            answer.word()
        ))),
        _ => Ok(()),
    }
}

/// The bridge methods the host does not offer.
///
/// Each group names why. None of them is a method the host cannot support, only one that no
/// client of it needs yet or that acts outside the workspaces the host was given.
pub const DENIED: &[&str] = &[
    // Reach saved sessions, every project's list, and other directories.
    "session.list",
    "session.open",
    "session.fork",
    "session.addressed",
    "session.delete",
    "session.purge",
    "session.import",
    "session.rewind",
    "session.note",
    // Open another directory beside the workspace and vouch for it, or close one (TRUST-9). The
    // window adds one only through its main process's own folder picker.
    "session.folders.list",
    "session.folders.add",
    "session.folders.close",
    "models.list",
    // Change how the host's own sessions behave, which a person at the host decides.
    "session.mode",
    "session.sandbox",
    "settings.inspect",
    "settings.select",
    "permissions.list",
    "permissions.revoke",
    "hooks.inspect",
    "language.get",
    "language.set",
    // The machine's usage ping, which the person at the host decides.
    "usage_ping.get",
    "usage_ping.set",
    "usage_ping.notice_due",
    "usage_ping.mark_notice_shown",
    // The machine's crash report switch and the one-time notice about it. The notice belongs to the
    // window, not to a client of the host: such a client has no screen to show it on, and whether
    // reports are on in this process and the switch are the person's at the host.
    "crash_reports.get",
    "crash_reports.set",
    "crash_reports.notice_due",
    "crash_reports.mark_notice_shown",
    // The machine's recorded effort level, which the terminal shares.
    "effort.get",
    "effort.set",
    "connectors.list",
    "connectors.preview",
    "connectors.connect",
    "connectors.disconnect",
    "connectors.remove",
    "bot.define",
    "bot.list",
    "bot.migrate",
    "bot.redefine",
    "bot.servers.read",
    "bot.servers.write",
    "bot.replace",
    "doctor",
    // Say where the machine keeps its state and what the host's settings name, for the person at
    // the host to read and post.
    "bug-report.compose",
    // What the desktop window reports about its own page, which a client of the host has none of.
    "window.fault",
    // Which of the session-bus names the desktop window's main process holds lead to a socket a
    // stage may write (SANDBOX-30). The directories a session may write are those of this process,
    // and a client of the host has no opener to protect.
    "opener.judge",
    "update.status",
    // The machine's sign-ins, which are its person's and not a workspace's, and the sign-in the
    // window starts only after its main process asked that person.
    "accounts.list",
    "accounts.signOut",
    "accounts.signIn",
    "accounts.cancelSignIn",
    // A key typed in the window's own key window, which only its main process reads.
    "accounts.keyBefore",
    "accounts.storeKey",
    // The host's own person's agreement to keep its computer awake (CMD-12).
    "caffeinate.confirmed",
    "caffeinate.confirm",
    // Read files or attach them to a turn.
    "mentions.offer",
    "mentions.named",
    "drops.classify",
    "pastes.check",
    // Work that runs without a request, or a plan walked as one run.
    "watches.list",
    "watches.add",
    "watches.stop",
    "watches.poll",
    // Lists and changes the schedule entries kept on this machine, for a line a person typed into
    // the window's own composer (CMD-1). A client of the host is a program, and a program never
    // types that line.
    "schedule.command",
    "manifest.run",
    "manifest.read",
    "manifest.reply",
    // Words a person types into a turn already running (INPUT-10). The only client of the host
    // today is not a window with a message box that stays live while a turn runs.
    "turn.interject",
    // A request to the session's model on the person's draft, for a message box to put in the box
    // (INPUT-50). The only client of the host today has no box to put it in.
    "session.enhance",
    "session.enhance.cancel",
    // The `/cost`, `/context`, `/request` and `/status` reports the window's Chat menu opens
    // (CMD-19). The only client of the host today has no menu that asks for them, and the last
    // request holds the session's whole system prompt and the paths of its instruction files.
    "session.report.cost",
    "session.report.context",
    "session.report.request",
    "session.report.status",
    // Shortening a conversation, which a person asks for from the window's Chat menu (CMD-34). The
    // only client of the host today has no menu to ask from.
    "session.compact",
    "session.compact.cancel",
    // Asking for a brief to hand a chat off from, which a person asks for from the window's Chat
    // menu (SESSION-34, CMD-34). The only client of the host today has no menu to ask from.
    "session.handoff",
    "session.handoff.cancel",
    // Handing a chat to a background session on this machine, which a person asks for from the
    // window's Chat menu (BG-2, CMD-34). The only client of the host today has no menu to ask from.
    "session.background",
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
    // The questions a session opens with about what its checkout's settings ask for. The host's
    // sessions put none of them (`Bridge::not_putting_project_questions`), so there is nothing to
    // answer.
    "rules.reply",
    "folder.reply",
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
        let asked = request(
            "turn.send",
            json!({"session": "s1", "prompt": "hi", "messageId": "m1"}),
        );
        assert_eq!(check(&asked).expect("offered").method, "turn.send");
    }

    #[test]
    fn a_method_the_host_does_not_name_is_refused_whether_or_not_the_bridge_has_it() {
        for method in [
            "session.list",
            "settings.select",
            "crash_reports.notice_due",
            "crash_reports.mark_notice_shown",
            "no.such.method",
            "",
        ] {
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
            ("changelog.read", "path"),
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

    /// A reply method added to the offer without a kind to ask would be a question the host's
    /// client is never put, and one removed from it would still be asked. Every offered reply is
    /// either the startup answer or maps to a kind, and the kinds are exactly those.
    #[test]
    fn every_offered_reply_but_the_startup_answer_is_a_kind_a_client_is_asked() {
        let replies: Vec<_> = OFFERED
            .iter()
            .map(|operation| operation.method)
            .filter(|method| method.ends_with(".reply") && *method != "trust.reply")
            .collect();
        assert_eq!(
            replies,
            ["confirm.reply", "run.reply", "fetch.reply", "ask.reply"]
        );
        assert_eq!(
            answerable(&Answer::ALL),
            [Kind::Write, Kind::Run, Kind::Fetch, Kind::Ask]
        );
        for answer in Answer::ALL {
            assert!(
                OFFERED
                    .iter()
                    .any(|operation| operation.method == answer.reply()),
                "{} is not offered",
                answer.reply()
            );
        }
    }

    /// A client is asked only the questions whose answers it may give, so one that may not
    /// confirm a write is never shown one.
    #[test]
    fn a_client_is_asked_only_the_kinds_it_may_answer() {
        assert_eq!(answerable(&[Answer::Run, Answer::Trust]), [Kind::Run]);
        assert_eq!(answerable(&[Answer::Trust]), []);
        assert_eq!(answerable(&[]), []);
    }

    #[test]
    fn a_reply_the_client_may_not_give_is_refused_and_any_other_request_passes() {
        let trust = request("trust.reply", json!({"session": "s1", "trusted": true}));
        let failure = check_answer(&trust, &[Answer::Confirm]).expect_err("refused");
        assert_eq!(failure.code, ErrorCode::NotAllowed);
        assert!(check_answer(&trust, &[Answer::Trust]).is_ok());
        let send = request("turn.send", json!({}));
        assert!(check_answer(&send, &[]).is_ok());
    }

    #[test]
    fn a_list_of_answers_names_known_words_once_or_none() {
        assert_eq!(
            Answer::list("trust,confirm"),
            Ok(vec![Answer::Confirm, Answer::Trust])
        );
        assert_eq!(Answer::list("none"), Ok(vec![]));
        for wrong in [
            "",
            "confirm,",
            "write",
            "confirm,confirm",
            "none,run",
            "Confirm",
        ] {
            assert!(Answer::list(wrong).is_err(), "{wrong:?} was accepted");
        }
    }

    /// Rejects an offer of `schedule.command` to a client of the host: a program never types the
    /// line (CMD-1), so the method is the window's alone.
    #[test]
    fn schedule_command_is_denied_to_a_client_of_the_host() {
        assert!(DENIED.contains(&"schedule.command"));
        let failure = check(&request(
            "schedule.command",
            json!({"session": "s1", "argument": "list"}),
        ))
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

    /// CRASH-9: the crash report switch and its notice are the person's at the host, so a client of
    /// the host may call none of the four methods. The faults this rejects are a method moved to
    /// the offered list, which would let a client turn reports off or mark the notice shown, and a
    /// method dropped from both lists.
    #[test]
    fn the_crash_report_methods_are_denied_to_a_client_of_the_host() {
        for method in [
            "crash_reports.get",
            "crash_reports.set",
            "crash_reports.notice_due",
            "crash_reports.mark_notice_shown",
        ] {
            assert!(DENIED.contains(&method), "{method} is not denied");
            assert!(
                OFFERED.iter().all(|operation| operation.method != method),
                "{method} is offered"
            );
            let failure = check(&request(method, json!({"enabled": false})))
                .err()
                .expect("refused");
            assert_eq!(failure.code, ErrorCode::NotAllowed, "{method}");
        }
    }
}
