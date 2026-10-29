//! The sandbox mode a session holds, readable by a turn that is running while the person changes it
//! (SANDBOX-22).

use bravebot_sandbox::SandboxMode;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The sandbox mode a session is in.
///
/// Cloning gives another handle on the same mode, so the command that sets it, the tools that
/// start programs under it, the confirmer that answers under it and a delegate's turn all read what
/// the person chose last. A value made from a [`SandboxMode`] is a handle nobody else holds, which
/// is what a front end that has no way to change the mode while a turn runs uses.
///
/// Read once per decision: two reads in one decision can disagree.
#[derive(Debug, Clone)]
pub struct LiveSandbox {
    /// The position of the mode in [`SandboxMode::ALL`].
    mode: Arc<AtomicUsize>,
}

impl LiveSandbox {
    pub fn new(mode: SandboxMode) -> Self {
        Self {
            mode: Arc::new(AtomicUsize::new(position(mode))),
        }
    }

    /// The mode in force now.
    pub fn get(&self) -> SandboxMode {
        let held = self.mode.load(Ordering::Acquire);
        SandboxMode::ALL.get(held).copied().unwrap_or_default()
    }

    pub fn set(&self, mode: SandboxMode) {
        self.mode.store(position(mode), Ordering::Release);
    }
}

fn position(mode: SandboxMode) -> usize {
    SandboxMode::ALL
        .iter()
        .position(|candidate| *candidate == mode)
        .unwrap_or_default()
}

impl Default for LiveSandbox {
    fn default() -> Self {
        Self::new(SandboxMode::default())
    }
}

impl From<SandboxMode> for LiveSandbox {
    fn from(mode: SandboxMode) -> Self {
        Self::new(mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SANDBOX-22: a mode set through one handle is the mode every clone reads, for each of the
    /// four modes. The regression it rejects is a clone that copies the value it was made with,
    /// which leaves a running turn on the mode it began with.
    #[test]
    fn a_mode_set_through_one_handle_is_read_through_its_clones() {
        for mode in SandboxMode::ALL {
            let held = LiveSandbox::new(SandboxMode::WorkspaceWrite);
            let clone = held.clone();

            held.set(mode);

            assert_eq!(clone.get(), mode, "set to {mode:?}");
            assert_eq!(held.get(), mode, "set to {mode:?}");
        }
    }

    /// SANDBOX-22: two handles made from a mode each hold their own. The regression it rejects is
    /// a handle that shares one global, which would let a window's choice move another session's.
    #[test]
    fn handles_made_apart_do_not_share_a_mode() {
        let one = LiveSandbox::from(SandboxMode::Strict);
        let other = LiveSandbox::from(SandboxMode::Strict);

        one.set(SandboxMode::Off);

        assert_eq!(other.get(), SandboxMode::Strict);
        assert_eq!(LiveSandbox::default().get(), SandboxMode::default());
    }
}
