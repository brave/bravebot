//! Standalone turns for tests: a fresh conversation, no vouched programs and no language servers.
//!
//! Every front end starts a turn with `turn::resume`, so these call it and take the outcome. They
//! choose arguments and nothing else.
use bravebot_agent::Workspace;
use bravebot_agent::confirm::Confirmer;
use bravebot_agent::conversation::Conversation;
use bravebot_agent::report::{IgnoreReports, Reporter};
use bravebot_agent::turn::{self, Outcome, Task, TurnError};
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::Sink;
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::trust::TrustStore;
use bravebot_net::Egress;

/// One turn with the trust map of `workspace`'s root.
pub fn run<S: Sink + Send, C: Confirmer + Send>(
    config: &Config,
    egress: &Egress,
    workspace: &Workspace,
    task: &Task,
    confirmer: &mut C,
    sink: &mut S,
) -> Result<Outcome, TurnError> {
    run_with_trust(
        config,
        egress,
        workspace,
        task,
        confirmer,
        sink,
        bravebot_agent::workspace::trust_store(workspace.root()),
    )
}

/// One turn with the user's trust decisions, telling nobody about progress.
pub fn run_with_trust<S: Sink + Send, C: Confirmer + Send>(
    config: &Config,
    egress: &Egress,
    workspace: &Workspace,
    task: &Task,
    confirmer: &mut C,
    sink: &mut S,
    trust: TrustStore,
) -> Result<Outcome, TurnError> {
    run_cancellable(
        config,
        egress,
        workspace,
        task,
        confirmer,
        &mut IgnoreReports,
        sink,
        trust,
        &Cancel::new(),
    )
}

/// One turn with a token that stops it and a reporter to tell about progress.
#[allow(clippy::too_many_arguments)]
pub fn run_cancellable<S: Sink + Send, C: Confirmer + Send, R: Reporter + Send>(
    config: &Config,
    egress: &Egress,
    workspace: &Workspace,
    task: &Task,
    confirmer: &mut C,
    reporter: &mut R,
    sink: &mut S,
    trust: TrustStore,
    cancel: &Cancel,
) -> Result<Outcome, TurnError> {
    turn::resume(
        config,
        egress,
        workspace,
        task,
        &mut Conversation::new(),
        confirmer,
        reporter,
        sink,
        trust,
        TrustedPrograms::new(),
        None,
        cancel,
    )
    .outcome
}
