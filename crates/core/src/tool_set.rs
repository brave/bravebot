//! The tools a run is limited to by its command line.
//!
//! `--tools a,b,c` offers the planner those tools and no others, and `--no-shell` takes away the
//! three that run or read the output of a program. Both only remove. A name the session would not
//! have offered anyway is still not offered, so the flags cannot widen a run, and a call to a tool
//! taken away is answered as a name nobody offered is.
//!
//! # One way
//!
//! Settled once, from the entry point, before any session is assembled, and never cleared, for the
//! reason [`crate::safe`] has no way to be turned off. An unsettled process, which is every test that
//! does not start from the entry point, is limited to nothing.

use std::sync::OnceLock;

/// The tools `--no-shell` takes away: the one that starts programs, and the two that read what they
/// printed. The same three the shell-execution capability gates for a delegate.
pub const SHELL: [&str; 3] = ["run", "read_output", "job_output"];

/// What a command line limited a run to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Limit {
    /// The only tools offered, where `--tools` was given. `None` is every tool.
    pub only: Option<Vec<String>>,
    /// Whether `--no-shell` was given.
    pub no_shell: bool,
}

impl Limit {
    /// Whether a tool of this name may be offered and called.
    pub fn allows(&self, tool: &str) -> bool {
        if self.no_shell && SHELL.contains(&tool) {
            return false;
        }
        self.only
            .as_ref()
            .is_none_or(|named| named.iter().any(|name| name == tool))
    }

    /// Whether the limit takes away anything at all.
    pub fn is_none(&self) -> bool {
        self.only.is_none() && !self.no_shell
    }
}

static SETTLED: OnceLock<Limit> = OnceLock::new();

/// Limit every session of this process to what the command line named.
///
/// Called once, from the entry point, before any session is assembled. The first call wins.
pub fn settle(limit: Limit) {
    let _ = SETTLED.set(limit);
}

/// Whether a tool of this name may be offered and called in this process.
pub fn allows(tool: &str) -> bool {
    SETTLED.get().is_none_or(|limit| limit.allows(tool))
}

/// Whether `--tools` named the tools, which leaves no room for the ones a server offers.
pub fn names_only() -> bool {
    SETTLED.get().is_some_and(|limit| limit.only.is_some())
}

/// The limit in force, for a report of what a run was started with.
pub fn settled() -> Limit {
    SETTLED.get().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(names: &[&str]) -> Limit {
        Limit {
            only: Some(names.iter().map(|name| name.to_string()).collect()),
            no_shell: false,
        }
    }

    /// An allow-list offers what it names and nothing else, and an empty limit offers everything.
    #[test]
    fn an_allow_list_allows_what_it_names_and_no_other_tool() {
        let limit = only(&["read_file", "search"]);
        assert!(limit.allows("read_file"));
        assert!(limit.allows("search"));
        assert!(!limit.allows("run"));
        assert!(!limit.allows("write_file"));
        assert!(Limit::default().allows("run"));
        assert!(Limit::default().is_none());
    }

    /// `--no-shell` takes away the program tools and leaves the file ones, and it wins over an
    /// allow-list that names `run`, since the flags only remove.
    #[test]
    fn no_shell_removes_the_program_tools_even_where_an_allow_list_names_them() {
        let limit = Limit {
            only: None,
            no_shell: true,
        };
        for tool in SHELL {
            assert!(!limit.allows(tool), "{tool}");
        }
        assert!(limit.allows("read_file"));
        assert!(limit.allows("write_file"));

        let both = Limit {
            only: Some(vec!["run".to_string(), "read_file".to_string()]),
            no_shell: true,
        };
        assert!(!both.allows("run"));
        assert!(both.allows("read_file"));
    }
}
