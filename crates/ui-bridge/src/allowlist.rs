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
pub use bravebot_core::device::Operation as Access;

/// A method the host offers, the parameters a request for it may carry, and whether it reads a
/// session or changes one.
pub struct Operation {
    pub method: &'static str,
    pub fields: &'static [&'static str],
    /// What a device's grant has to cover for the device to ask it. A `read` grant covers only
    /// [`Access::Read`].
    pub access: Access,
}

/// The methods a client of the host may call.
pub const OFFERED: &[Operation] = &[
    Operation {
        method: "agent.info",
        fields: &["session"],
        access: Access::Read,
    },
    Operation {
        method: "instance.info",
        fields: &[],
        access: Access::Read,
    },
    // The release notes compiled into this program. Reads nothing and needs no session.
    Operation {
        method: "changelog.read",
        fields: &[],
        access: Access::Read,
    },
    Operation {
        method: "session.new",
        fields: &["directory"],
        access: Access::Control,
    },
    Operation {
        method: "session.view.start",
        fields: &["session", "version"],
        access: Access::Read,
    },
    // A new snapshot of a session's view, for a client that missed an update.
    Operation {
        method: "session.view.resync",
        fields: &["session"],
        access: Access::Read,
    },
    Operation {
        method: "session.close",
        fields: &["session"],
        access: Access::Control,
    },
    // Joins a session another connection opened. The host answers it itself.
    Operation {
        method: "session.attach",
        fields: &["session"],
        access: Access::Read,
    },
    // The one answer that widens what a session may read that a client of the host gives, and only
    // a client the person gave `trust` on the host's command line ([`Answer::Trust`]). Such a client
    // is a program the person named, started over a channel it shares with the host alone, so its
    // answer is the person's own (see the client contract: startup trust is a local-client
    // capability). A client that is not that, such as one reached over a network, is not offered it.
    Operation {
        method: "trust.reply",
        fields: &["session", "trusted"],
        access: Access::Control,
    },
    Operation {
        method: "turn.send",
        fields: &["session", "prompt", "messageId"],
        access: Access::Control,
    },
    Operation {
        method: "turn.lookup",
        fields: &["session", "messageId"],
        access: Access::Read,
    },
    Operation {
        method: "turn.cancel",
        fields: &["session", "target"],
        access: Access::Control,
    },
    Operation {
        method: "confirm.reply",
        fields: &["session", "request", "decision"],
        access: Access::Control,
    },
    Operation {
        method: "run.reply",
        fields: &["session", "request", "decision"],
        access: Access::Control,
    },
    Operation {
        method: "fetch.reply",
        fields: &["session", "request", "decision"],
        access: Access::Control,
    },
    Operation {
        method: "ask.reply",
        fields: &["session", "request", "answers"],
        access: Access::Control,
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
    "background.list",
    // Totals what every project's records spent (CLI-37).
    "session.stats",
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
    "session.limit",
    "session.variant",
    "settings.inspect",
    "settings.select",
    "permissions.list",
    "permissions.check",
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
    // The level of this process's diagnostic log (DIAG-11). The log is written for the person at
    // the host to read and attach to a bug report, so only that person chooses how much it holds.
    "diagnostic_log.set",
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
    // The text of the desktop window's own title (PANEL-19), which a client of the host has no
    // window to show.
    "window.title",
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
    "pastes.clean",
    // Work that runs without a request, or a plan walked as one run.
    "watches.list",
    "watches.add",
    "watches.stop",
    "watches.poll",
    // The condition a chat works towards is a line a person typed into the window's own composer
    // (CMD-1, GOAL-1). A client of the host is a program, and a program never types that line.
    "session.goal",
    // Puts that goal to its judge and sends the work back, which the window asks for once it has
    // sent the prompts the person queued, so a goal never gets ahead of the person.
    "session.goal.continue",
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
    // window's Chat menu (BG-2, CMD-34), and stopping one, which a person asks for from the chat
    // list's menu (BG-11). The only client of the host today has no menu to ask from.
    "session.background",
    "session.background.stop",
    // A question asked beside the work, for the `/btw` or `/recap` line a person typed into the
    // window's own composer (CMD-1, CMD-8, CMD-16). A client of the host is a program, and a
    // program never types it.
    "session.btw",
    "session.recap",
    "session.aside.cancel",
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

/// What a device's grant has to cover for it to ask `method`. A method the host does not offer is
/// [`Access::Control`], so a method nobody has classified is never taken for a read.
pub fn access_of(method: &str) -> Access {
    OFFERED
        .iter()
        .find(|operation| operation.method == method)
        .map_or(Access::Control, |operation| operation.access)
}

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
            "background.list",
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

    /// Rejects `session.goal` for a client of the host: the condition is a line a person typed
    /// (CMD-1, GOAL-1), so no program may set, pause or clear it.
    #[test]
    fn the_goal_command_is_denied_to_a_client_of_the_host() {
        assert!(DENIED.contains(&"session.goal"));
        let refused = check(&request(
            "session.goal",
            json!({"session": "s1", "argument": "cargo test exits 0"}),
        ))
        .err()
        .map(|failure| failure.code);
        assert_eq!(refused, Some(ErrorCode::NotAllowed));
        assert!(DENIED.contains(&"session.goal.continue"));
        let refused = check(&request("session.goal.continue", json!({"session": "s1"})))
            .err()
            .map(|failure| failure.code);
        assert_eq!(refused, Some(ErrorCode::NotAllowed));
    }

    /// Rejects `session.btw`, `session.recap` and `session.aside.cancel` for a client of the host:
    /// a program never types the `/btw` or `/recap` line (CMD-1), so the methods are the window's
    /// alone.
    #[test]
    fn asking_beside_the_work_is_denied_to_a_client_of_the_host() {
        for method in ["session.btw", "session.recap", "session.aside.cancel"] {
            assert!(DENIED.contains(&method), "{method}");
            let failure = check(&request(method, json!({"session": "s1"})))
                .err()
                .expect("refused");
            assert_eq!(failure.code, ErrorCode::NotAllowed, "{method}");
        }
    }

    /// Each method the host offers reads or controls as written here. Opening, closing, sending,
    /// cancelling and every reply change a session, so a `read` device may ask none of them
    /// (decision 16 of `docs/design/mobile/decisions-and-tradeoffs.md`). A method added to the offer
    /// fails this until its access is written here, and a control moved to read fails it too.
    #[test]
    fn each_offered_method_reads_or_controls_as_written_here() {
        let written = [
            ("agent.info", Access::Read),
            ("instance.info", Access::Read),
            ("changelog.read", Access::Read),
            ("session.new", Access::Control),
            ("session.view.start", Access::Read),
            ("session.view.resync", Access::Read),
            ("session.close", Access::Control),
            ("session.attach", Access::Read),
            ("trust.reply", Access::Control),
            ("turn.send", Access::Control),
            ("turn.lookup", Access::Read),
            ("turn.cancel", Access::Control),
            ("confirm.reply", Access::Control),
            ("run.reply", Access::Control),
            ("fetch.reply", Access::Control),
            ("ask.reply", Access::Control),
        ];
        let offered: Vec<_> = OFFERED
            .iter()
            .map(|operation| (operation.method, operation.access))
            .collect();
        assert_eq!(offered, written);
        for operation in OFFERED {
            assert_eq!(access_of(operation.method), operation.access);
        }
    }

    /// A method the host does not offer needs a `control` grant, whether the bridge has it or not.
    #[test]
    fn a_method_the_host_does_not_offer_needs_control() {
        for method in DENIED
            .iter()
            .copied()
            .chain(["no.such.method", "", "pair.begin"])
        {
            assert_eq!(access_of(method), Access::Control, "{method}");
        }
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

    /// DIAG-11: the host's diagnostic log is for the person at the host, so a client of the host
    /// may not change its level. The faults this rejects are the method moved to the offered list,
    /// which would let a client raise the log to debug, and the method dropped from both lists.
    #[test]
    fn diagnostic_log_set_is_denied_to_a_client_of_the_host() {
        assert!(DENIED.contains(&"diagnostic_log.set"));
        assert!(
            OFFERED
                .iter()
                .all(|operation| operation.method != "diagnostic_log.set"),
            "diagnostic_log.set is offered"
        );
        assert_eq!(
            check(&request("diagnostic_log.set", json!({"level": "debug"})))
                .err()
                .map(|failure| failure.code),
            Some(ErrorCode::NotAllowed)
        );
    }
}
