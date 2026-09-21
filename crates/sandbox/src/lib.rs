//! OS-level confinement for untrusted subprocesses.
//!
//! One boundary, several backends. The confined process is whatever acts on model
//! output: a processor sub-agent, or a stdio MCP server we launch. Trusted code that
//! performs already-authorised effects is guarded by information-flow gates instead;
//! sandboxing it would confine our own code while leaving the untrusted part free.
//!
//! # Fail closed
//!
//! If confinement cannot be established, [`Sandbox::spawn`] refuses rather than
//! running the process unconfined. Silently degrading is worse than an error: the
//! caller believes it has a guarantee it does not have, and the audit trail records a
//! sandbox that was never applied.

#![deny(unsafe_code)]

#[cfg(target_os = "linux")]
pub mod linux;
// Compiled under test on any Unix as well as on the platform it confines, so what this
// backend decides before a process starts, the argument vector that carries the caller's
// environment past `sandbox-exec`, is pinned by every job that runs the suite rather
// than by the one job that has Seatbelt. The tests that start a confined process are
// gated to macOS inside the module. Unix rather than every platform, because the spawn
// this backend reaches is itself a Unix one.
#[cfg(any(target_os = "macos", all(test, unix)))]
pub mod macos;
pub mod policy;
pub mod process;
// Compiled under test on every platform as well as on the one it confines, so what this
// backend decides before a process starts is pinned by every job that runs the suite
// rather than by the one job that lints this target: the capability the policy asks for,
// what each grant permits, which policies are refused, and how an argument is written onto
// a command line. The Win32 calls applying those decisions are compiled only where they
// exist.
#[cfg(test)]
mod testutil;
#[cfg(any(windows, test))]
pub mod windows;

use policy::{Capabilities, ConfinementLevel, SandboxPolicy};
pub use process::{
    ConfinedChild, ConfinedStderr, ConfinedStdin, ConfinedStdout, Environment, Stream, Streams,
};
use std::fmt;

#[derive(Debug)]
pub enum SandboxError {
    /// No confinement mechanism is available on this platform or kernel.
    ///
    /// A refusal, not a warning: the process does not run.
    Unavailable {
        platform: &'static str,
        detail: String,
    },
    /// A mechanism exists but could not be applied.
    SetupFailed {
        mechanism: &'static str,
        detail: String,
    },
    /// The policy would not confine anything.
    PolicyTooPermissive,
    /// The process could not be started.
    SpawnFailed(std::io::Error),
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { platform, detail } => write!(
                f,
                "no confinement available on {platform} ({detail}); refusing to run \
                 untrusted code unconfined"
            ),
            Self::SetupFailed { mechanism, detail } => write!(
                f,
                "{mechanism} could not be applied ({detail}); refusing to run untrusted \
                 code unconfined"
            ),
            Self::PolicyTooPermissive => f.write_str(
                "the requested policy would not confine anything; refusing to present it \
                 as a sandbox",
            ),
            Self::SpawnFailed(e) => write!(f, "failed to spawn the confined process: {e}"),
        }
    }
}

impl std::error::Error for SandboxError {}

/// A platform confinement backend.
pub trait Sandbox {
    /// What this backend can enforce here, on this kernel.
    fn capabilities(&self) -> Capabilities;

    /// Start a confined process, or refuse.
    ///
    /// The backend starts the process rather than handing back a command for the caller
    /// to spawn, because confinement can be carried by an argument to the call that
    /// creates the process: there is no command a caller could spawn itself on such a
    /// platform and still be confined. The stdio and the environment are therefore
    /// decided here, and [`process`] applies both, so they mean the same thing whichever
    /// backend this is.
    fn spawn(
        &self,
        program: &str,
        args: &[String],
        policy: &SandboxPolicy,
        streams: Streams,
        environment: Environment,
    ) -> Result<ConfinedChild, SandboxError>;
}

/// The backend for the current platform.
///
/// Returns [`SandboxError::Unavailable`] where no backend is implemented, so an
/// unsupported platform is a refusal rather than an unconfined process.
pub fn for_current_platform() -> Result<Box<dyn Sandbox>, SandboxError> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::SeatbeltSandbox::new()?))
    }

    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(linux::LandlockSandbox::new()?))
    }

    #[cfg(windows)]
    {
        Ok(Box::new(windows::AppContainerSandbox::new()?))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        Err(SandboxError::Unavailable {
            platform: std::env::consts::OS,
            detail: "no confinement backend is implemented for this platform yet".into(),
        })
    }
}

/// A backend that always refuses.
///
/// Not a fallback: it exists so tests can assert that callers propagate a refusal
/// rather than continuing without confinement.
#[derive(Debug, Default)]
pub struct Unavailable;

impl Sandbox for Unavailable {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            level: ConfinementLevel::None,
            mechanisms: Vec::new(),
            network_denial_enforced: false,
            // Nothing is granted here at all, so claiming a kind of grant it installs
            // would be a claim about a process this backend never starts.
            grants_paths_that_do_not_exist: false,
        }
    }

    fn spawn(
        &self,
        _program: &str,
        _args: &[String],
        _policy: &SandboxPolicy,
        _streams: Streams,
        _environment: Environment,
    ) -> Result<ConfinedChild, SandboxError> {
        Err(SandboxError::Unavailable {
            platform: std::env::consts::OS,
            detail: "confinement is unavailable".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::nothing_attached;

    #[test]
    fn an_unavailable_backend_refuses_to_spawn() {
        let sandbox = Unavailable;
        let result = sandbox.spawn(
            "echo",
            &[],
            &SandboxPolicy::strict(),
            nothing_attached(),
            Environment::Inherited,
        );
        assert!(matches!(result, Err(SandboxError::Unavailable { .. })));
    }

    /// The property the whole module exists for: no confinement means no process, not
    /// an unconfined one.
    #[test]
    fn refusal_is_not_a_silent_fallback() {
        let sandbox = Unavailable;
        let err = sandbox
            .spawn(
                "echo",
                &[],
                &SandboxPolicy::strict(),
                nothing_attached(),
                Environment::Inherited,
            )
            .expect_err("must refuse");
        assert!(err.to_string().contains("refusing to run"));
    }

    #[test]
    fn an_unavailable_backend_reports_no_confinement() {
        let caps = Unavailable.capabilities();
        assert_eq!(caps.level, ConfinementLevel::None);
        assert!(!caps.network_denial_enforced);
        assert!(!caps.grants_paths_that_do_not_exist);
    }

    /// Either a real backend is returned, or the lookup refuses. It must never hand
    /// back something that reports no confinement.
    #[test]
    fn the_platform_lookup_never_returns_an_unconfined_backend() {
        match for_current_platform() {
            Ok(sandbox) => assert_ne!(
                sandbox.capabilities().level,
                ConfinementLevel::None,
                "a backend was returned that confines nothing"
            ),
            Err(SandboxError::Unavailable { .. }) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn errors_explain_the_refusal() {
        let err = SandboxError::PolicyTooPermissive;
        assert!(err.to_string().contains("would not confine anything"));
    }
}
