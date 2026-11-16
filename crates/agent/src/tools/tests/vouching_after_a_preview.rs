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
/// holds an authority to start one on. Counts the questions it was asked. Every other
/// question is Unattended's.
struct SaysYes {
    starts_a_program_on: Option<FileAuthority>,
    asked: usize,
}

impl Confirmer for SaysYes {
    fn confirm_vouch(&mut self, _request: &VouchRequest) -> Decision {
        self.asked += 1;
        if let Some(authority) = self.starts_a_program_on.take() {
            authority.note_program_started();
        }
        Decision::Approve
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

/// Reads `note.txt`, which nobody vouched for, `reads` times in one turn with a person who
/// answers yes, in bypass so that no confined check is made. Returns what the planner was
/// told by each read, whether the file is still quarantined afterwards, and how many times
/// the person was asked, and the trail.
fn read_note(starts_a_program: bool, reads: usize) -> (Vec<String>, bool, usize, String) {
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
        starts_a_program_on: starts_a_program.then(|| policy.file_authority()),
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
