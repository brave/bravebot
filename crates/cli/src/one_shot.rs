//! The confirmer a one-shot run answers its questions with.

use bravebot_agent::confirm::{
    Confirmer, Decision, FetchRequest, ManifestRequest, OutputRequest, RunDecision, RunRequest,
    ServerRequest, VetRequest, VouchRequest, WriteDecision, WriteRequest,
};
use bravebot_core::ask::{Answer, Asking};
use bravebot_i18n::t;
use std::io::{BufRead, Read, Write};

/// What a one-shot run answers its questions with: [`bravebot_agent::Unattended`], except that a
/// plan is put to whoever typed the command.
///
/// Every other question is refused, as CLI-1 has it. Those are due in the middle of a run, over a
/// path or a program nobody undertook to watch for, and there is nothing about typing a command that
/// says somebody will still be there. A plan is asked at one known moment instead: once, before the
/// first step, while the command that raised it has printed nothing but its own progress. The person
/// who typed it is the person reading that, so there is somebody to ask.
///
/// Generic over both ends, as [`piped_input`] is over its source, so a test answers without a
/// terminal.
pub(crate) struct OneShot<R: Read, W: Write> {
    /// Where the answer is read from, and only ever after the question was written.
    ///
    /// Buffered here rather than taken buffered, because the whole confirmer travels to the thread
    /// the turn runs on and a held stdin lock cannot.
    input: std::io::BufReader<R>,
    /// Where the plan and the question go. stderr in a real run, which is what keeps stdout the
    /// reply alone (CLI-5).
    output: W,
    /// Whether there is anybody to ask.
    ///
    /// Both ends being a terminal, not stdin alone. A plan written into a redirected stderr is a
    /// plan nobody read, and running a program nobody was shown is the one thing this question
    /// cannot mean.
    attended: bool,
    /// Every question but the plan. Delegated rather than restated so there is one account of what
    /// a one-shot refuses, and adding a question here cannot quietly start approving it.
    refusing: bravebot_agent::Unattended,
    /// What to call just before the plan is put to the person (HOOK-2).
    alert: Option<bravebot_agent::PromptAlert>,
    /// The effects that went unapproved because nobody was there to approve them, in order.
    ///
    /// Kept here, at the one place that knows the answer was a refusal for want of anybody to ask,
    /// so the run can end on the status that says a gate refused something (CLI-6). A question the
    /// tool answers with a problem for the planner without telling the policy leaves the run's own
    /// clean flag true, and this is then the only record of it. A write, a run, a fetch or a call
    /// to a server's tool is told to the policy, so it is kept for the status alone
    /// (`Declined::in_the_trail`).
    declined: Vec<Declined>,
    /// The input lines a run given `--input-format lines` reads (CLI-36), where it was.
    ///
    /// The planner's questions are put to the program on the other end of it and a steer line is
    /// what an interjection takes. Nothing else changes: a write, a run, a fetch and every other
    /// question is still refused here, because a line sent on a pipe approves none of them.
    lines: Option<crate::lines::Channel>,
}

/// One question a one-shot run could not put to anybody, and so answered no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Declined {
    /// The gate the question belongs to, named as a result object names a refusal's.
    pub(crate) gate: &'static str,
    /// Why, as a sentence a script can print.
    pub(crate) reason: &'static str,
    /// Whether the audit trail already holds this refusal, which the policy recorded when the tool
    /// answered the planner. It still ends the run on the refused status, since a delegate's
    /// policy keeps its own refusals and this confirmer is shared, and a result object lists it
    /// once, from the trail.
    pub(crate) in_the_trail: bool,
}

impl<R: Read, W: Write> OneShot<R, W> {
    pub(crate) fn new(input: R, output: W, attended: bool) -> Self {
        Self {
            input: std::io::BufReader::new(input),
            output,
            attended,
            refusing: bravebot_agent::Unattended,
            alert: None,
            declined: Vec::new(),
            lines: None,
        }
    }

    /// Put the planner's questions to the program sending these lines, and take its steers.
    pub(crate) fn with_lines(mut self, lines: crate::lines::Channel) -> Self {
        self.lines = Some(lines);
        self
    }

    /// What was refused for want of anybody to ask, in the order it was asked.
    pub(crate) fn declined(&self) -> &[Declined] {
        &self.declined
    }

    /// Write the question out, then read the answer back.
    ///
    /// The steps are not printed here. The run narrates the frozen plan a line per step immediately
    /// before asking, from the same renderer this request was built with, so on a terminal they are
    /// the lines directly above the question and are still on screen while it is answered. Printing
    /// them again would put the same list twice under two different headings, which reads as two
    /// plans rather than one. The panel a session draws does repeat them, and has to: it covers the
    /// transcript that showed them.
    fn put_the_plan(&mut self, request: &ManifestRequest) -> std::io::Result<Decision> {
        writeln!(self.output, "{}", t!(plan_title))?;
        writeln!(
            self.output,
            "{} {}  {}",
            t!(plan_verb),
            t!(plan_steps, count = request.steps.len()),
            t!(plan_goal, task = &request.task)
        )?;
        writeln!(self.output)?;
        for sentence in [
            t!(plan_explained),
            t!(plan_not_its_writes),
            t!(plan_nothing_yet),
        ] {
            writeln!(self.output, "{sentence}")?;
        }
        write!(self.output, "{} ", t!(plan_answer))?;
        self.output.flush()?;

        // A closed stdin reads nothing, which is nobody answering, which is a no. So is any other
        // line: the affirmative is the only answer that runs a program, and a person who typed
        // something else did not type that.
        let mut answer = String::new();
        self.input.read_line(&mut answer)?;
        Ok(match answer.trim().to_lowercase() == t!(line_answer_yes) {
            true => Decision::Approve,
            false => Decision::Reject,
        })
    }
}

impl<R: Read, W: Write> Confirmer for OneShot<R, W> {
    fn alert_when_asking(&mut self, alert: Option<bravebot_agent::PromptAlert>) {
        self.alert = alert;
    }

    /// False, because every question but the plan is refused unasked. The plan is the one that is
    /// put, and it fires the alert itself.
    fn asks_a_person(&self) -> bool {
        false
    }

    /// The one question this confirmer may answer yes to, and only where somebody is there.
    fn confirm_manifest(&mut self, request: &ManifestRequest) -> Decision {
        let decision = if self.attended {
            if let Some(alert) = &self.alert {
                alert();
            }
            // A question that could not be written is a plan nobody saw, so it is declined rather
            // than taken as unanswered and waved through.
            self.put_the_plan(request).unwrap_or(Decision::Reject)
        } else {
            self.refusing.confirm_manifest(request)
        };
        if decision == Decision::Reject {
            self.declined.push(Declined {
                gate: "manifest_plan",
                reason: "the plan was not approved",
                in_the_trail: false,
            });
        }
        decision
    }

    fn confirm_write(&mut self, request: &WriteRequest) -> WriteDecision {
        let decision = self.refusing.confirm_write(request);
        if !decision.approved() {
            self.declined.push(Declined {
                gate: "file_write",
                reason: "the write was not approved, and nobody was there to approve it",
                in_the_trail: true,
            });
        }
        decision
    }

    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        let decision = self.refusing.confirm_run(request);
        if !decision.approved() {
            self.declined.push(Declined {
                gate: "run",
                reason: "the program was not approved, and nobody was there to approve it",
                in_the_trail: true,
            });
        }
        decision
    }

    fn confirm_read_output(&mut self, request: &OutputRequest) -> Decision {
        self.refusing.confirm_read_output(request)
    }

    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        self.refusing.confirm_vetted_read(request)
    }

    fn confirm_fetch(&mut self, request: &FetchRequest) -> Decision {
        let decision = self.refusing.confirm_fetch(request);
        if decision == Decision::Reject {
            self.declined.push(Declined {
                gate: "web_fetch",
                reason: "the fetch was not approved, and nobody was there to approve it",
                in_the_trail: true,
            });
        }
        decision
    }

    fn confirm_server(&mut self, request: &ServerRequest) -> Decision {
        self.refusing.confirm_server(request)
    }

    fn confirm_vouch(&mut self, request: &VouchRequest) -> Decision {
        self.refusing.confirm_vouch(request)
    }

    fn confirm_exposing_read(
        &mut self,
        request: &bravebot_agent::confirm::ExposureRequest,
    ) -> Decision {
        self.refusing.confirm_exposing_read(request)
    }

    fn confirm_tool_list(
        &mut self,
        request: &bravebot_agent::confirm::ToolListRequest,
    ) -> bravebot_agent::confirm::Decision {
        self.refusing.confirm_tool_list(request)
    }

    fn confirm_mcp_call(
        &mut self,
        request: &bravebot_agent::confirm::McpCallRequest,
    ) -> bravebot_agent::confirm::CallDecision {
        let decision = self.refusing.confirm_mcp_call(request);
        if decision.decision == Decision::Reject {
            self.declined.push(Declined {
                gate: "mcp_call",
                reason: "the call was not approved, and nobody was there to approve it",
                in_the_trail: true,
            });
        }
        decision
    }

    fn confirm_path(&mut self, request: &bravebot_agent::confirm::PathRequest) -> Decision {
        self.refusing.confirm_path(request)
    }

    fn confirm_host(&mut self, request: &bravebot_agent::confirm::HostRequest) -> Decision {
        self.refusing.confirm_host(request)
    }

    fn confirm_message(
        &mut self,
        request: &bravebot_agent::confirm::MessageRequest,
    ) -> bravebot_agent::confirm::MessageDecision {
        self.refusing.confirm_message(request)
    }

    fn confirm_move(&mut self, request: &bravebot_agent::confirm::MoveRequest) -> Decision {
        self.refusing.confirm_move(request)
    }

    /// Declined rather than answered, as everywhere nobody can be asked: a reply invented here would
    /// be reported to the planner as the person's own words. Where lines are being read the question
    /// is put to the program sending them, and what comes back is a pick among the choices the
    /// planner offered (CLI-36).
    fn ask_user(&mut self, asking: &Asking) -> Vec<Answer> {
        match &self.lines {
            Some(lines) => lines.ask(asking),
            None => self.refusing.ask_user(asking),
        }
    }

    /// Nothing, except a steer line from a channel a person vouched for. Nobody is typing between
    /// steps: the only line otherwise read is an answer to a question it just asked.
    fn interjection(&mut self) -> Option<String> {
        match &self.lines {
            Some(lines) => lines.steer(),
            None => self.refusing.interjection(),
        }
    }
}
