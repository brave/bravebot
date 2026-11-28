use super::arguments::{Scratch, told, with_tools};
use super::*;
use crate::confirm::{
    Confirmer, RunDecision, RunRequest, Unattended, VouchRequest, WriteDecision, WriteRequest,
};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::file_authority::FileAuthority;
use bravebot_core::policy::{ReleasePlan, Routing};

/// Answers a vouch question yes, after starting a `run` line on the first question when it
/// holds an authority to start one on, and after distrusting `note.txt` on the first question
/// when it holds an authority to do that on. Says the answer was `mode`'s, where that is set,
/// as a permission mode answering for the user does. Counts the questions it was asked. Every
/// other question is Unattended's.
struct SaysYes {
    starts_a_program_on: Option<FileAuthority>,
    distrusts_the_note_on: Option<FileAuthority>,
    mode: Option<&'static str>,
    asked: usize,
}

impl Confirmer for SaysYes {
    fn confirm_vouch(&mut self, _request: &VouchRequest) -> Decision {
        self.asked += 1;
        if let Some(authority) = self.starts_a_program_on.take() {
            authority.note_program_started();
        }
        if let Some(authority) = self.distrusts_the_note_on.take() {
            authority.publish("note.txt", bravebot_core::label::Integrity::Untrusted);
        }
        Decision::Approve
    }
    fn confirm_vouch_noting_the_mode(
        &mut self,
        request: &VouchRequest,
    ) -> (Decision, Option<&'static str>) {
        (self.confirm_vouch(request), self.mode)
    }
    fn confirm_write(&mut self, request: &WriteRequest) -> WriteDecision {
        Unattended.confirm_write(request)
    }
    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        Unattended.confirm_run(request)
    }
    fn confirm_read_output(&mut self, request: &crate::confirm::OutputRequest) -> Decision {
        Unattended.confirm_read_output(request)
    }
    fn confirm_vetted_read(&mut self, request: &crate::confirm::VetRequest) -> Decision {
        Unattended.confirm_vetted_read(request)
    }
    fn confirm_fetch(&mut self, request: &crate::confirm::FetchRequest) -> Decision {
        Unattended.confirm_fetch(request)
    }
    fn confirm_server(&mut self, request: &crate::confirm::ServerRequest) -> Decision {
        Unattended.confirm_server(request)
    }
    fn confirm_manifest(&mut self, request: &crate::confirm::ManifestRequest) -> Decision {
        Unattended.confirm_manifest(request)
    }
    fn confirm_exposing_read(&mut self, request: &crate::confirm::ExposureRequest) -> Decision {
        Unattended.confirm_exposing_read(request)
    }
    fn confirm_tool_list(&mut self, request: &crate::confirm::ToolListRequest) -> Decision {
        Unattended.confirm_tool_list(request)
    }
    fn confirm_mcp_call(
        &mut self,
        request: &crate::confirm::McpCallRequest,
    ) -> crate::confirm::CallDecision {
        Unattended.confirm_mcp_call(request)
    }
    fn confirm_path(&mut self, request: &crate::confirm::PathRequest) -> Decision {
        Unattended.confirm_path(request)
    }
    fn confirm_host(&mut self, request: &crate::confirm::HostRequest) -> Decision {
        Unattended.confirm_host(request)
    }
    fn confirm_message(
        &mut self,
        request: &crate::confirm::MessageRequest,
    ) -> crate::confirm::MessageDecision {
        Unattended.confirm_message(request)
    }
    fn confirm_move(&mut self, request: &crate::confirm::MoveRequest) -> Decision {
        Unattended.confirm_move(request)
    }
    fn ask_user(&mut self, asking: &bravebot_core::ask::Asking) -> Vec<bravebot_core::ask::Answer> {
        Unattended.ask_user(asking)
    }
    fn interjection(&mut self) -> Option<String> {
        Unattended.interjection()
    }
}

/// What happens while the first vouch question is open.
#[derive(Clone, Copy)]
enum Meanwhile {
    Nothing,
    AProgramStarts,
    TheNoteIsDistrusted,
}

/// Reads `note.txt`, which nobody vouched for, `reads` times in one turn with a person who
/// answers yes, in bypass so that no confined check is made. Returns what the planner was
/// told by each read, whether the file is still quarantined afterwards, and how many times
/// the person was asked, and the trail.
fn read_note(starts_a_program: bool, reads: usize) -> (Vec<String>, bool, usize, String) {
    let meanwhile = match starts_a_program {
        true => Meanwhile::AProgramStarts,
        false => Meanwhile::Nothing,
    };
    read_note_answered_by(None, meanwhile, reads)
}

/// [`read_note`], with the answer said to be `mode`'s where that is set, and `meanwhile`
/// happening while the first question is open.
fn read_note_answered_by(
    mode: Option<&'static str>,
    meanwhile: Meanwhile,
    reads: usize,
) -> (Vec<String>, bool, usize, String) {
    let scratch = Scratch::new("vouch-after-program");
    std::fs::write(scratch.path.join("note.txt"), "a line\n").unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut routing = Routing::new();
    routing.insert_trusted("task", "read a note");
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy");
    let mut person = SaysYes {
        starts_a_program_on: matches!(meanwhile, Meanwhile::AProgramStarts)
            .then(|| policy.file_authority()),
        distrusts_the_note_on: matches!(meanwhile, Meanwhile::TheNoteIsDistrusted)
            .then(|| policy.file_authority()),
        mode,
        asked: 0,
    };
    let call: ToolCall = serde_json::from_value(json!({
        "id": "1",
        "function": {
            "name": "read_file",
            "arguments": json!({"path": "note.txt", "why": "to see the note"}).to_string(),
        }
    }))
    .expect("a call");

    let outputs = with_tools(&workspace, |tools| {
        tools.permission_mode.set(crate::PermissionMode::Bypass);
        (0..reads)
            .map(|_| {
                dispatch(
                    &mut policy,
                    tools,
                    &mut person,
                    &mut crate::report::IgnoreReports,
                    &call,
                )
            })
            .collect::<Vec<_>>()
    });
    let said = outputs
        .iter()
        .map(|output| told(&mut policy, &output.text))
        .collect();
    let still_quarantined = policy.read_is_quarantined("note.txt");
    drop(policy);
    (
        said,
        still_quarantined,
        person.asked,
        format!("{:?}", sink.events()),
    )
}

/// One read of `note.txt`: what the planner was told, whether the file is still
/// quarantined, and how many times the person was asked.
fn read_while(starts_a_program: bool) -> (String, bool, usize) {
    let (mut said, still_quarantined, asked, _) = read_note(starts_a_program, 1);
    (said.remove(0), still_quarantined, asked)
}

/// The regression it rejects: a `run` line another delegate started voiding a vouch a mode
/// answered. Nobody read a preview, so no program can have rewritten what somebody was shown,
/// and under bypass with delegates running commands a read that compared the whole stamp would
/// fail on every retry. `a_yes_is_not_spent_when_a_program_started_while_the_person_was_asked`
/// is the control: it answers in bypass with no mode named, so it also rejects deciding who
/// answered from the mode read before the question instead of from the answer (MODE-8).
#[test]
fn a_yes_the_mode_gave_is_spent_when_a_program_started_while_the_question_was_open() {
    let (said, still_quarantined, asked, trail) =
        read_note_answered_by(Some("bypass"), Meanwhile::AProgramStarts, 1);
    let said = &said[0];
    assert_eq!(asked, 1, "{said}");
    assert!(
        !still_quarantined,
        "a mode's yes was thrown away for a program started beside it: {said}"
    );
    assert!(
        !said.contains("changed while the user was being asked about it"),
        "the planner was told a person was asked: {said}"
    );
    assert!(
        trail.contains("bypass mode answered the offer to vouch"),
        "the trail does not credit the mode: {trail}"
    );
}

/// The regression it rejects: a mode's answer spent after the path's own revision moved, which
/// TRUST-4 forbids whoever answered, and a refusal that tells the planner a person was asked
/// when a mode answered. The result says nobody was asked, that the file was written or decided
/// about since the read began, and to read it again, and says nothing about a command starting,
/// which does not void a mode's answer (TRUST-4). The second read rejects keeping the path
/// marked as asked about after a mode's answer was thrown away: it has to offer the path again.
#[test]
fn a_yes_the_mode_gave_is_not_spent_on_a_file_whose_revision_moved_while_the_question_was_open() {
    let (said, still_quarantined, asked, _) =
        read_note_answered_by(Some("bypass"), Meanwhile::TheNoteIsDistrusted, 1);
    let (said_twice, _, asked_twice, _) =
        read_note_answered_by(Some("bypass"), Meanwhile::TheNoteIsDistrusted, 2);
    assert_eq!(
        asked_twice, 2,
        "the second read did not offer the path again: {}",
        said_twice[1]
    );
    let said = &said[0];
    assert_eq!(asked, 1, "{said}");
    assert!(
        still_quarantined,
        "a mode's yes was spent on a file written after it was asked about: {said}"
    );
    assert!(
        !said.contains("being asked"),
        "the planner was told a person was asked: {said}"
    );
    assert!(
        said.contains("nobody was asked"),
        "the planner was not told nobody was asked: {said}"
    );
    assert!(
        said.contains(
            "was written, or a decision about it or a directory above it was made, since this \
             read began"
        ),
        "the planner was not told why the answer was thrown away: {said}"
    );
    assert!(
        said.contains("Read it again"),
        "the planner was not told to read it again: {said}"
    );
    assert!(
        !said.contains("command"),
        "the planner was told a command voided the answer: {said}"
    );
}

/// The regression it rejects: `read_file` passing the vouch only the path's revision, or
/// taking the stamp after the question was answered. A program started while the question
/// was open moves no revision, so either leaves the file trusted on a yes that was about
/// text a program may have rewritten since. The first case is the control: with no
/// program started the same yes is spent, so a read that never vouched would not pass.
#[test]
fn a_yes_is_not_spent_when_a_program_started_while_the_person_was_asked() {
    let (said, still_quarantined, asked) = read_while(false);
    assert_eq!(asked, 1, "{said}");
    assert!(
        !still_quarantined,
        "a yes with nothing changed was not spent: {said}"
    );

    let (said, still_quarantined, asked) = read_while(true);
    assert_eq!(asked, 1, "{said}");
    assert!(
        still_quarantined,
        "a yes was spent on a file a program may have rewritten: {said}"
    );
    assert!(
        said.contains("changed while the user was being asked about it"),
        "the planner was not told the answer was thrown away: {said}"
    );
}

/// The regression it rejects: a read after a thrown-away answer asking nobody. The result of
/// the first read tells the planner to read again and says the person will be asked, so the
/// second read has to put the question, and a yes with no program started in between has to
/// trust the file. An implementation that keeps the path marked as asked about leaves the
/// second read quarantined with `asked` still 1.
#[test]
fn a_read_after_a_thrown_away_answer_asks_the_person_again() {
    let (said, still_quarantined, asked, trail) = read_note(true, 2);
    assert!(
        said[0].contains("changed while the user was being asked about it"),
        "the first answer was not thrown away: {}",
        said[0]
    );
    assert_eq!(
        asked, 2,
        "the second read did not put the question again: {}",
        said[1]
    );
    assert!(
        !still_quarantined,
        "a yes given to the second question was not spent: {}",
        said[1]
    );
    assert!(
        !said[1].contains("changed while the user was being asked about it"),
        "the second answer was thrown away too: {}",
        said[1]
    );
    assert_eq!(
        trail
            .matches("note.txt: quarantined, so the user is offered the chance to vouch for it")
            .count(),
        2,
        "the trail does not show the second offer: {trail}"
    );
}
