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
    /// Set for a run a stored schedule fired: the token that ends the turn when the planner asks a
    /// question, since nobody is there to answer it (SCHEDULE-2).
    stop_at_a_question: Option<bravebot_core::cancel::Cancel>,
    /// The first question the planner asked a run that stops at one, kept whole to be written down
    /// for `schedule pending`. Carried and written, never read: nothing here looks inside it.
    asked: Option<Asking>,
    /// Whether the person typed `--approve-input-lines`, so that a write, a run and a fetch are put
    /// to the program on the other end of `lines` and approved only on its yes (CLI-36).
    ///
    /// A fact about the command line. Nothing in a request sets it.
    approving: bool,
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
            stop_at_a_question: None,
            asked: None,
            approving: false,
        }
    }

    /// End the turn with `cancel` at the first question the planner asks, and keep the question.
    ///
    /// For a run a stored schedule fired. Declining the question and carrying on would let the
    /// planner act on the lack of an answer; stopping leaves the question for a person to read.
    pub(crate) fn stopping_at_a_question(mut self, cancel: bravebot_core::cancel::Cancel) -> Self {
        self.stop_at_a_question = Some(cancel);
        self
    }

    /// The question a run stopped at, where it stopped at one.
    pub(crate) fn asked(&self) -> Option<&Asking> {
        self.asked.as_ref()
    }

    /// Put each write, run and fetch to the program sending the lines, as well as its questions.
    ///
    /// A no change where no lines are being read, since there is then nobody to put them to.
    pub(crate) fn approving_over_lines(mut self) -> Self {
        self.approving = true;
        self
    }

    /// The channel to put a permission to, where the person let the program answer them.
    fn approver(&self) -> Option<&crate::lines::Channel> {
        self.lines.as_ref().filter(|_| self.approving)
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

    /// Put to the program sending the lines where the person let it answer (CLI-36), with the path
    /// and the complete body that would be written. A write to a protected path, or one the scan
    /// found a credential in, is not put: the driver derived those two facts before the body was
    /// released, and neither is something a program's yes stands in for (MODE-2, CRED-13).
    fn confirm_write(&mut self, request: &WriteRequest) -> WriteDecision {
        let decision = match self.approver() {
            Some(lines) if !request.protected && request.credentials.is_empty() => {
                let said = lines.ask_permission(
                    "write",
                    &[
                        ("path", request.path.clone()),
                        ("contents", request.contents.clone()),
                    ],
                );
                WriteDecision::from(if said {
                    Decision::Approve
                } else {
                    Decision::Reject
                })
            }
            _ => self.refusing.confirm_write(request),
        };
        if !decision.approved() {
            self.declined.push(Declined {
                gate: "file_write",
                reason: "the write was not approved, and nobody was there to approve it",
                in_the_trail: true,
            });
        }
        decision
    }

    /// Put to the program sending the lines where the person let it answer (CLI-36), with the line
    /// as the plan renders it. A line that asks for credential scopes or to run unconfined is not
    /// put, for the reason accepting edits does not answer one (SANDBOX-26).
    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        let decision = match self.approver() {
            Some(lines) if !request.asks_for_scopes() && !request.unconfined => {
                let said = lines.ask_permission(
                    "run",
                    &[
                        ("command", request.plan.display()),
                        ("directory", request.shown_directory()),
                    ],
                );
                if said {
                    RunDecision::approve()
                } else {
                    RunDecision::reject()
                }
            }
            _ => self.refusing.confirm_run(request),
        };
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

    /// Put to the program sending the lines where the person let it answer (CLI-36), with the URL
    /// and the host it names.
    fn confirm_fetch(&mut self, request: &FetchRequest) -> Decision {
        let decision = match self.approver() {
            Some(lines) => {
                if lines.ask_permission(
                    "fetch",
                    &[("url", request.url.clone()), ("host", request.host.clone())],
                ) {
                    Decision::Approve
                } else {
                    Decision::Reject
                }
            }
            None => self.refusing.confirm_fetch(request),
        };
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
        if let Some(cancel) = &self.stop_at_a_question {
            cancel.cancel();
            // The first question is the one the run stopped at; a later one in the same turn is
            // not kept over it.
            self.asked.get_or_insert_with(|| asking.clone());
            self.declined.push(Declined {
                gate: "scheduled_question",
                reason: "a scheduled run cannot be asked a question, so it stopped",
                in_the_trail: false,
            });
            return self.refusing.ask_user(asking);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines::Channel;
    use bravebot_agent::confirm::Confined;
    use bravebot_sandbox::scope::{Requested, Scope};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::time::Duration;

    /// Where the run's event stream goes, so a test reads back what was put to the program.
    #[derive(Clone, Default)]
    struct Said(Arc<Mutex<Vec<u8>>>);

    impl Write for Said {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Said {
        fn permissions(&self) -> Vec<serde_json::Value> {
            String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner))
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .filter(|event| event["event"] == "permission")
                .collect()
        }
    }

    /// The program on the other end of the lines: it picks `position` for every permission put to
    /// it, as soon as it is put. A call that was put and approved therefore comes back approved,
    /// and a call that came back refused was never put.
    struct Program {
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Program {
        fn picking(position: usize, channel: Channel, said: Said) -> Self {
            let stop = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&stop);
            let thread = std::thread::spawn(move || {
                let mut answered = 0;
                while !flag.load(Ordering::SeqCst) {
                    let asked = said.permissions();
                    for event in asked.iter().skip(answered) {
                        let line = format!(
                            r#"{{"type":"answer","id":{},"choices":[[{position}]]}}"#,
                            event["id"]
                        );
                        channel.deliver(answered + 1, Ok(line));
                        answered += 1;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            });
            Self {
                stop,
                thread: Some(thread),
            }
        }

        fn leaves(mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(thread) = self.thread.take() {
                thread.join().expect("the program thread ends");
            }
        }
    }

    type Approving = OneShot<&'static [u8], Vec<u8>>;

    /// A confirmer that puts permissions to a program which picks `position` for each of them.
    fn approving_over_lines(position: usize) -> (Approving, Said, Program) {
        let said = Said::default();
        let channel = Channel::new(false, crate::json::Stream::new(said.clone()), 1 << 20);
        let one_shot = OneShot::new(&b""[..], Vec::new(), false)
            .with_lines(channel.clone())
            .approving_over_lines();
        let program = Program::picking(position, channel, said.clone());
        (one_shot, said, program)
    }

    fn keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn a_write(path: &str) -> WriteRequest {
        WriteRequest {
            written_since_checkout: false,
            path: path.to_string(),
            contents: "text".to_string(),
            existing: None,
            diff: bravebot_agent::diff::Diff::compute("", "text"),
            intent: bravebot_agent::Intent::Create,
            untrusted: false,
            remark: None,
            protected: false,
            credentials: Vec::new(),
            may_always: false,
            record: None,
            may_edit: false,
        }
    }

    fn a_run() -> RunRequest {
        RunRequest::from_pipeline(
            &bravebot_core::Pipeline::new(vec![bravebot_core::Stage::new(
                "git",
                vec!["log".into()],
            )]),
            &["/usr/bin/git".into()],
            "/tmp/project",
        )
    }

    fn a_fetch() -> FetchRequest {
        FetchRequest {
            url: "https://example.invalid/a".to_string(),
            host: "example.invalid".to_string(),
            redirected_from: None,
            classified: Default::default(),
        }
    }

    fn confined_asking_for(requested: Vec<(String, Requested)>) -> Confined {
        Confined {
            reads_the_machine: false,
            directories: Vec::new(),
            network: bravebot_sandbox::network::Network::Open,
            filesystem: Default::default(),
            requested,
            requested_reaches: Vec::new(),
            carried: Vec::new(),
            writes_git_directories: false,
        }
    }

    fn labels(event: &serde_json::Value) -> Vec<String> {
        event["choices"]
            .as_array()
            .expect("choices")
            .iter()
            .map(|choice| choice["label"].as_str().expect("a label").to_string())
            .collect()
    }

    /// CLI-36: a run is put to the program as its command line and directory with the two fixed
    /// choices and nothing that asks for more than this one call, and it proceeds only on the
    /// approving position. A fault that added a field offering to remember the answer, that
    /// approved whatever the program picked, or that put the run under another kind would fail.
    #[test]
    fn a_run_is_put_to_the_program_with_its_command_and_directory_and_runs_only_on_approval() {
        let request = a_run();

        let (mut one_shot, said, program) = approving_over_lines(1);
        let decision = one_shot.confirm_run(&request);
        program.leaves();
        let asked = said.permissions();
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert_eq!(
            keys(&asked[0]),
            ["call", "choices", "event", "id", "kind", "schema"]
        );
        assert_eq!(asked[0]["kind"], "run");
        assert_eq!(keys(&asked[0]["call"]), ["command", "directory"]);
        assert_eq!(asked[0]["call"]["command"], request.plan.display());
        assert_eq!(asked[0]["call"]["directory"], request.shown_directory());
        assert_eq!(labels(&asked[0]), ["reject", "approve"]);
        assert!(decision.approved(), "an approved run did not proceed");
        assert!(one_shot.declined().is_empty());

        let (mut one_shot, said, program) = approving_over_lines(0);
        let decision = one_shot.confirm_run(&request);
        program.leaves();
        assert_eq!(said.permissions().len(), 1);
        assert!(!decision.approved(), "a rejected run proceeded");
        let gates: Vec<_> = one_shot.declined().iter().map(|d| d.gate).collect();
        assert_eq!(gates, ["run"]);
    }

    /// CLI-36: a fetch is put to the program as the URL and the host it names with the two fixed
    /// choices, and proceeds only on the approving position. A fault that approved whatever the
    /// program picked, or that dropped the host or added a remembering field, would fail.
    #[test]
    fn a_fetch_is_put_to_the_program_with_its_url_and_host_and_proceeds_only_on_approval() {
        let request = a_fetch();

        let (mut one_shot, said, program) = approving_over_lines(1);
        let decision = one_shot.confirm_fetch(&request);
        program.leaves();
        let asked = said.permissions();
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert_eq!(
            keys(&asked[0]),
            ["call", "choices", "event", "id", "kind", "schema"]
        );
        assert_eq!(asked[0]["kind"], "fetch");
        assert_eq!(keys(&asked[0]["call"]), ["host", "url"]);
        assert_eq!(asked[0]["call"]["url"], "https://example.invalid/a");
        assert_eq!(asked[0]["call"]["host"], "example.invalid");
        assert_eq!(labels(&asked[0]), ["reject", "approve"]);
        assert_eq!(decision, Decision::Approve);
        assert!(one_shot.declined().is_empty());

        let (mut one_shot, said, program) = approving_over_lines(0);
        let decision = one_shot.confirm_fetch(&request);
        program.leaves();
        assert_eq!(said.permissions().len(), 1);
        assert_eq!(decision, Decision::Reject, "a rejected fetch proceeded");
        let gates: Vec<_> = one_shot.declined().iter().map(|d| d.gate).collect();
        assert_eq!(gates, ["web_fetch"]);
    }

    /// CLI-36, MODE-2: a write to a protected path is never put to the program, so its approving
    /// answer cannot let one through. An ordinary write is put first as the control, which shows
    /// the program here does approve what it is asked. A fault that put every write would have the
    /// program approve a write into `.git/hooks`.
    #[test]
    fn a_write_to_a_protected_path_is_not_put_to_a_program_that_approves_everything() {
        let (mut one_shot, said, program) = approving_over_lines(1);
        let ordinary = one_shot.confirm_write(&a_write("notes.md"));
        let mut protected = a_write(".git/hooks/pre-commit");
        protected.protected = true;
        let refused = one_shot.confirm_write(&protected);
        program.leaves();

        assert!(ordinary.approved(), "the control was not approved");
        assert!(!refused.approved(), "a write to a protected path proceeded");
        let asked = said.permissions();
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert_eq!(asked[0]["call"]["path"], "notes.md");
        let gates: Vec<_> = one_shot.declined().iter().map(|d| d.gate).collect();
        assert_eq!(gates, ["file_write"]);
    }

    /// CLI-36, CRED-13: a write the credential scan found a value in is never put to the program,
    /// so an approving answer cannot write it. A fault that put it would show a program the bytes
    /// of a credential in an event and let its yes write it.
    #[test]
    fn a_write_the_credential_scan_found_a_value_in_is_not_put_to_a_program_that_approves_everything()
     {
        let (mut one_shot, said, program) = approving_over_lines(1);
        let mut request = a_write("config/master.key");
        request.credentials = vec!["a 40 character hex value".to_string()];
        let refused = one_shot.confirm_write(&request);
        program.leaves();

        assert!(
            !refused.approved(),
            "a write holding a credential proceeded"
        );
        let asked = said.permissions();
        assert!(
            asked.is_empty(),
            "the write was put to the program: {asked:?}"
        );
        let gates: Vec<_> = one_shot.declined().iter().map(|d| d.gate).collect();
        assert_eq!(gates, ["file_write"]);
    }

    /// CLI-36, SANDBOX-26: a run that asks for credential scopes, and one that asks to run
    /// unconfined, are never put to the program, so its approving answer cannot grant either. A
    /// plain run is put first as the control. A fault that put every run would let a program that
    /// approves everything hand a command a credential or the whole machine.
    #[test]
    fn a_run_asking_for_scopes_or_to_run_unconfined_is_not_put_to_a_program_that_approves_everything()
     {
        let mut with_scopes = a_run();
        with_scopes.confined = Some(confined_asking_for(vec![(
            "git".to_string(),
            Requested::Scope(Scope::Remote),
        )]));
        assert!(with_scopes.asks_for_scopes());
        let mut unconfined = a_run();
        unconfined.unconfined = true;

        let (mut one_shot, said, program) = approving_over_lines(1);
        let control = one_shot.confirm_run(&a_run());
        let scoped = one_shot.confirm_run(&with_scopes);
        let loose = one_shot.confirm_run(&unconfined);
        program.leaves();

        assert!(control.approved(), "the control was not approved");
        assert!(!scoped.approved(), "a run asking for scopes proceeded");
        assert!(
            !loose.approved(),
            "a run asking to run unconfined proceeded"
        );
        let asked = said.permissions();
        assert_eq!(asked.len(), 1, "{asked:?}");
        let gates: Vec<_> = one_shot.declined().iter().map(|d| d.gate).collect();
        assert_eq!(gates, ["run", "run"]);
    }

    /// CLI-36: an approval answers a write, a run or a fetch and no other question. The question a
    /// vouch puts, which would write a rule into the trust map, is refused without being put, so
    /// a program that approves everything vouches for no path. A fault that routed it through the
    /// permission would write a rule on a program's yes.
    #[test]
    fn an_approving_program_cannot_answer_the_question_that_vouches_for_a_path() {
        let (mut one_shot, said, program) = approving_over_lines(1);
        let decision = one_shot.confirm_vouch(&VouchRequest {
            path: "notes.md".to_string(),
            rule: "notes.md".to_string(),
            preview: "text".to_string(),
            truncated: false,
            verdict: bravebot_core::vetting::Verdict::Safe,
            reason: None,
            label: String::new(),
            reason_label: None,
        });
        program.leaves();

        assert_eq!(decision, Decision::Reject, "a program vouched for a path");
        let asked = said.permissions();
        assert!(
            asked.is_empty(),
            "the vouch was put to the program: {asked:?}"
        );
    }
}
