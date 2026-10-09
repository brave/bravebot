//! Task execution.
//!
//! Holds the label-aware tools and the turn loop. Tools take their routing arguments
//! from precommitted routing, never from model output, so a turn cannot be redirected
//! by the content it processes.

#![deny(unsafe_code)]

pub mod advisor;
pub mod agents;
pub mod aside;
pub mod attached;
pub mod backend;
pub mod cmdline;
pub mod compact;
pub mod confine;
pub mod confirm;
pub mod conversation;
pub mod delegate;
pub mod diff;
pub mod exec;
pub mod findings;
pub mod git;
pub mod glob;
pub mod goal;
pub mod granted;
pub mod home;
pub mod hooks;
pub mod host_proxy;
pub mod lsp;
pub mod manifest;
pub mod mcp;
pub mod memory;
pub mod mode;
pub mod outcome;
pub mod permission_mode;
pub mod permissions;
pub mod preamble;
pub mod processor;
pub mod programs;
pub mod reach;
pub mod regex;
pub mod remembered;
pub mod replace;
pub mod report;
pub mod request_view;
pub mod rewind;
pub mod scratch;
pub mod scrub;
pub mod servers;
pub mod shared;
pub mod shell;
pub mod skills;
pub mod spend_limit;
pub mod styles;
pub mod subscription;
#[cfg(test)]
mod testutil;
pub mod timing;
pub mod tools;
pub mod trusted;
pub mod turn;
#[cfg(unix)]
pub mod usability;
pub mod vet;
pub mod watch;
pub mod workspace;

/// What a crash of this process may leave behind, re-exported for the front ends.
///
/// A rule about a process that holds a credential is addressed to every such process, and the
/// graphical front end's transport binary is one of them. Its crate reaches the sandbox only
/// through this one, and a manifest edit to give it a second path to the same function would be a
/// widening of what that crate depends on for no gain.
pub use bravebot_sandbox::crash;

pub use confirm::{
    Carried, Confined, Confirmer, Decision, Intent, Remark, RunDecision, RunRequest, Unattended,
    WriteDecision, WriteRequest,
};
pub use conversation::Conversation;
pub use delegate::Delegated;
pub use mode::Mode;
pub use outcome::{Category, Diagnosis, Ending, Spent};
pub use permission_mode::{Confining, PermissionMode};
pub use processor::ProcessorError;
pub use report::{Activity, IgnoreReports, Reporter};
pub use scratch::SessionScratch;
pub use spend_limit::SpendLimit;
pub use subscription::{Discovery, ImportedSubscription};
pub use turn::{Outcome, Task, TurnError};
pub use workspace::{Workspace, WorkspaceError};
