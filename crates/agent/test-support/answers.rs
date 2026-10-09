//! Prompt answers used by caller retention tests.
use bravebot_agent::confirm::*;

pub struct Answers {
    pub runs: Vec<RunRequest>,
    pub run: RunDecision,
    pub exposures: usize,
}
impl Answers {
    pub fn new(run: RunDecision) -> Self {
        Self {
            runs: Vec::new(),
            run,
            exposures: 0,
        }
    }
}
impl Confirmer for Answers {
    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        self.runs.push(request.clone());
        self.run
    }
    fn confirm_exposing_read(&mut self, _: &ExposureRequest) -> Decision {
        self.exposures += 1;
        Decision::Approve
    }
    fn confirm_write(&mut self, request: &WriteRequest) -> WriteDecision {
        ApproveWrites.confirm_write(request)
    }
    fn confirm_read_output(&mut self, request: &OutputRequest) -> Decision {
        ApproveWrites.confirm_read_output(request)
    }
    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        ApproveWrites.confirm_vetted_read(request)
    }
    fn confirm_fetch(&mut self, request: &FetchRequest) -> Decision {
        ApproveWrites.confirm_fetch(request)
    }
    fn confirm_server(&mut self, request: &ServerRequest) -> Decision {
        ApproveWrites.confirm_server(request)
    }
    fn confirm_manifest(&mut self, request: &ManifestRequest) -> Decision {
        ApproveWrites.confirm_manifest(request)
    }
    fn confirm_vouch(&mut self, request: &VouchRequest) -> Decision {
        ApproveWrites.confirm_vouch(request)
    }
    fn confirm_tool_list(&mut self, request: &ToolListRequest) -> Decision {
        ApproveWrites.confirm_tool_list(request)
    }
    fn confirm_mcp_call(&mut self, _: &McpCallRequest) -> CallDecision {
        CallDecision::reject()
    }

    /// Refuses. This double answers no question about reach.
    fn confirm_path(
        &mut self,
        _request: &bravebot_agent::confirm::PathRequest,
    ) -> bravebot_agent::confirm::Decision {
        bravebot_agent::confirm::Decision::Reject
    }

    fn confirm_move(
        &mut self,
        _request: &bravebot_agent::confirm::MoveRequest,
    ) -> bravebot_agent::confirm::Decision {
        bravebot_agent::confirm::Decision::Reject
    }
    fn ask_user(&mut self, _: &bravebot_core::ask::Asking) -> Vec<bravebot_core::ask::Answer> {
        Vec::new()
    }
    fn interjection(&mut self) -> Option<String> {
        None
    }
}
