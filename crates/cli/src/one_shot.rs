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
}

impl<R: Read, W: Write> OneShot<R, W> {
    pub(crate) fn new(input: R, output: W, attended: bool) -> Self {
        Self {
            input: std::io::BufReader::new(input),
            output,
            attended,
            refusing: bravebot_agent::Unattended,
        }
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
    /// The one question this confirmer may answer yes to, and only where somebody is there.
    fn confirm_manifest(&mut self, request: &ManifestRequest) -> Decision {
        if !self.attended {
            return self.refusing.confirm_manifest(request);
        }
        // A question that could not be written is a plan nobody saw, so it is declined rather than
        // taken as unanswered and waved through.
        self.put_the_plan(request).unwrap_or(Decision::Reject)
    }

    fn confirm_write(&mut self, request: &WriteRequest) -> WriteDecision {
        self.refusing.confirm_write(request)
    }

    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        self.refusing.confirm_run(request)
    }

    fn confirm_read_output(&mut self, request: &OutputRequest) -> Decision {
        self.refusing.confirm_read_output(request)
    }

    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        self.refusing.confirm_vetted_read(request)
    }

    fn confirm_fetch(&mut self, request: &FetchRequest) -> Decision {
        self.refusing.confirm_fetch(request)
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
        self.refusing.confirm_mcp_call(request)
    }

    fn confirm_path(&mut self, request: &bravebot_agent::confirm::PathRequest) -> Decision {
        self.refusing.confirm_path(request)
    }

    fn confirm_host(&mut self, request: &bravebot_agent::confirm::HostRequest) -> Decision {
        self.refusing.confirm_host(request)
    }

    fn confirm_move(&mut self, request: &bravebot_agent::confirm::MoveRequest) -> Decision {
        self.refusing.confirm_move(request)
    }

    /// Declined rather than answered, as everywhere nobody can be asked: a reply invented here would
    /// be reported to the planner as the person's own words.
    fn ask_user(&mut self, asking: &Asking) -> Vec<Answer> {
        self.refusing.ask_user(asking)
    }

    /// Nothing. Nobody is typing between steps: the only line this reads is an answer to a question
    /// it just asked.
    fn interjection(&mut self) -> Option<String> {
        self.refusing.interjection()
    }
}
