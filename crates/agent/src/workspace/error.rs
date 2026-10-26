//! What a file tool reports when it cannot do what was asked: the error type and the wording of its
//! remedies. These are sentences about a name and what the file system said, with no [`Policy`], no
//! [`Labelled`] and no file content in them.

use super::named_home;
use bravebot_core::policy::Denial;
use std::fmt;
use std::path::Path;

/// What a failed read says, where a missing file also says what a relative path was joined to.
///
/// "No such file or directory" alone reads as the file being absent, when the writer may have
/// meant another directory than the one the path was joined to.
///
/// Names the directory and not the path joined to it. [`WorkspaceError::describe`] passes the
/// detail through as it is, and a read through a reference was made on a filename the planner is
/// never told (LIST-2).
pub(super) fn io_detail(error: &std::io::Error, named: &str, root: &Path) -> String {
    if error.kind() == std::io::ErrorKind::NotFound
        && !Path::new(named).is_absolute()
        && !named_home(named)
    {
        return format!(
            "{error}; a relative path is looked up under the working directory, {}",
            root.display()
        );
    }
    error.to_string()
}

#[derive(Debug)]
pub enum WorkspaceError {
    /// The policy refused the operation.
    Denied(Denial),
    /// The path resolved outside the workspace root.
    Escapes { path: String, remedy: Remedy },
    /// The path was not usable as a relative workspace path.
    Invalid { path: String, reason: &'static str },
    /// The operation failed on disk.
    Io { path: String, detail: String },
    /// The file changed after it was read, so the approved change no longer applies.
    Stale { path: String },
    /// Another effect already holds the path, so this write was refused rather than interleaved.
    Contended { path: String },
    /// The file is not text, so there is nothing useful to return.
    Binary { path: String },
    /// The file is text in an encoding other than UTF-8. [`Workspace::read_page`] shows it
    /// decoded; a read whose text is written back or handed on as the file's own, an edit's or a
    /// slot's, refuses it.
    NotUtf8 { path: String },
    /// The attachment is larger than a request should carry.
    TooLarge { path: String, limit: usize },
    /// The search pattern is not a regular expression this engine can match.
    Pattern { detail: String },
    /// `read_git` did not answer, for the reason carried.
    Git {
        path: String,
        declined: crate::git::Declined,
    },
    /// No checkout was made, for the reason carried.
    Checkout {
        path: String,
        refused: crate::git::checkout::Refused,
    },
    /// The working directory was not changed because the session keeps the checkouts carried, by
    /// their numbers (CHECKOUT-16).
    KeepsCheckouts { ids: Vec<String> },
}

impl WorkspaceError {
    /// Word this failure about `named`, which is the name the caller may say out loud.
    ///
    /// The path a failure carries is the one the call was made on, and a call made through a
    /// reference was made on a filename out of a directory nobody vouched for. A filename is
    /// content, which is LIST-1, and the reference exists so that the planner is never told
    /// one, which is LIST-2. A tool result that interpolated the name would put bytes an
    /// attacker chose into the planner's context inside a sentence the driver signs for, which
    /// is what LABEL-3 forbids. So every caller that reports one of these to the planner words
    /// it from here and passes the name it is entitled to say: `ref:1` for a reference, the
    /// path the planner typed otherwise.
    ///
    /// Written as a match over the whole enum rather than a substitution over what
    /// [`fmt::Display`] produced, so a variant added later with a path of its own has to say
    /// here which name it reports instead of inheriting a wording that leaks.
    ///
    /// [`WorkspaceError::Denied`] is the arm `named` does not reach: a denial carries a sentence
    /// the policy layer composed, and what may be in one is that layer's question rather than
    /// this one's.
    pub fn describe(&self, named: &str) -> String {
        match self {
            Self::Denied(d) => d.to_string(),
            Self::Escapes {
                remedy: Remedy::Nothing,
                ..
            } => format!("'{named}' resolves outside the workspace; refusing to touch it"),
            Self::Escapes {
                remedy: Remedy::Inside,
                ..
            } => format!(
                "'{named}' is inside the workspace, and a path there is named relative to the \
                 workspace root; refusing to touch it as typed. Name it without the workspace's \
                 own directory in front, or as '.' for the root itself"
            ),
            // What the person can do is part of the sentence, since the planner cannot do it and has
            // otherwise been seen to reach the same file through `run` instead.
            Self::Escapes {
                remedy: Remedy::Open,
                ..
            } => format!(
                "'{named}' resolves outside the workspace; refusing to touch it. The person can \
                 open the directory it is in, with /add-dir in the terminal or --add-dir when \
                 starting bravebot, after which this path reaches it"
            ),
            Self::Escapes {
                remedy: Remedy::OpenOrDrop,
                ..
            } => format!(
                "'{named}' resolves outside the workspace; refusing to touch it. The person can \
                 open the directory it is in, with /add-dir in the terminal or --add-dir when \
                 starting bravebot, after which this path reaches it, or drop the file on the \
                 window to have it read with their next message"
            ),
            Self::Escapes {
                remedy: Remedy::OpenItself,
                ..
            } => format!(
                "'{named}' resolves outside the workspace; refusing to touch it. The person can \
                 open the directory this path names, with /add-dir in the terminal or --add-dir \
                 when starting bravebot, after which this path reaches it"
            ),
            Self::Escapes {
                remedy: Remedy::OpenItselfEndsCheckouts,
                ..
            } => format!(
                "'{named}' resolves outside the workspace; refusing to touch it. The person can \
                 open the directory this path names, with /add-dir in the terminal or --add-dir \
                 when starting bravebot, after which this path reaches it. That directory holds \
                 the working directory, and {ENDS_CHECKOUTS}"
            ),
            Self::Escapes {
                remedy: Remedy::OpenEndsCheckouts,
                ..
            } => format!(
                "'{named}' resolves outside the workspace; refusing to touch it. The person can \
                 open the directory it is in, with /add-dir in the terminal or --add-dir when \
                 starting bravebot, after which this path reaches it. That directory holds the \
                 working directory, and {ENDS_CHECKOUTS}"
            ),
            Self::Escapes {
                remedy: Remedy::DropOrOpenEndsCheckouts,
                ..
            } => format!(
                "'{named}' resolves outside the workspace; refusing to touch it. The person can \
                 drop the file on the window to have it read with their next message. Opening the \
                 directory it is in, with /add-dir in the terminal or --add-dir when starting \
                 bravebot, would also make this path reach it, but that directory holds the \
                 working directory, and {ENDS_CHECKOUTS}"
            ),
            Self::Escapes {
                remedy: Remedy::Credential,
                ..
            } => format!(
                "'{named}' resolves outside the workspace, in a place that holds credentials, \
                 which cannot be opened to the file tools; refusing to touch it"
            ),
            Self::Escapes {
                remedy: Remedy::CredentialDrop,
                ..
            } => format!(
                "'{named}' resolves outside the workspace, in a place that holds credentials, \
                 which cannot be opened to the file tools; refusing to touch it. The person can \
                 drop the file on the window to have it read with their next message"
            ),
            Self::Escapes {
                remedy: Remedy::Kept,
                ..
            } => format!(
                "'{named}' resolves outside the workspace, and permissions.readsStayInWorkspace \
                 keeps the file tools inside it; refusing to touch it"
            ),
            Self::Escapes {
                remedy: Remedy::Drop,
                ..
            } => format!(
                "'{named}' resolves outside the workspace, and permissions.readsStayInWorkspace \
                 keeps the file tools inside it; refusing to touch it. The person can drop the \
                 file on the window to have it read with their next message"
            ),
            Self::Invalid { reason, .. } => format!("'{named}' is not usable: {reason}"),
            Self::Io { detail, .. } => format!("'{named}': {detail}"),
            Self::Stale { .. } => {
                format!("'{named}' changed after it was read; read it again before editing")
            }
            Self::Contended { .. } => {
                format!("another write to '{named}' is still in progress, so nothing was written")
            }
            Self::Binary { .. } => {
                format!("'{named}' is a binary file, so it cannot be read as text")
            }
            Self::NotUtf8 { .. } => format!(
                "'{named}' is text in an encoding other than UTF-8, which this call does not \
                 handle: writing it back would change the file's encoding. read_file shows such \
                 a file decoded"
            ),
            Self::TooLarge { limit, .. } => format!(
                "'{named}' is larger than the {} MiB an attachment may be",
                limit / (1024 * 1024)
            ),
            Self::Pattern { detail } => format!("the search pattern is not usable: {detail}"),
            Self::Git { declined, .. } => declined.describe(named),
            Self::Checkout { refused, .. } => refused.describe(named),
            Self::KeepsCheckouts { ids } => {
                let (noun, them) = if ids.len() == 1 {
                    ("checkout", "it")
                } else {
                    ("checkouts", "them")
                };
                format!(
                    "the session keeps {noun} {}; moving would leave {them} keyed under the \
                     directory the session is in, so remove {them} with /checkouts remove first",
                    ids.join(", "),
                )
            }
        }
    }

    /// The path this failure carries, which is the one the call was made on.
    ///
    /// Only [`fmt::Display`] reads it, and what [`fmt::Display`] writes is for a log, a trail or
    /// a person: all three are entitled to the real path, and a trail saying a file could not be
    /// read without saying which file would be the opposite problem. Every sentence the planner
    /// reads is worded through [`WorkspaceError::describe`] instead, for the reason written
    /// there.
    /// This refusal as a call that reads the file reports it, which may also name a drop.
    pub(super) fn for_a_read(self) -> Self {
        let Self::Escapes { path, remedy } = self else {
            return self;
        };
        let remedy = match remedy {
            Remedy::Open => Remedy::OpenOrDrop,
            Remedy::OpenEndsCheckouts => Remedy::DropOrOpenEndsCheckouts,
            Remedy::Kept => Remedy::Drop,
            Remedy::Credential => Remedy::CredentialDrop,
            other => other,
        };
        Self::Escapes { path, remedy }
    }

    fn carried_path(&self) -> &str {
        match self {
            Self::Denied(_) | Self::Pattern { .. } | Self::KeepsCheckouts { .. } => "",
            Self::Escapes { path, .. }
            | Self::Invalid { path, .. }
            | Self::Io { path, .. }
            | Self::Stale { path }
            | Self::Contended { path }
            | Self::Binary { path }
            | Self::NotUtf8 { path }
            | Self::TooLarge { path, .. }
            | Self::Git { path, .. }
            | Self::Checkout { path, .. } => path,
        }
    }
}

/// What opening a directory that holds the working directory costs, in the words every sentence
/// the planner reads uses for it (CHECKOUT-7).
pub(super) const ENDS_CHECKOUTS: &str = "while one is open no delegate is given a checkout";

/// What a person can do so that a path refused for leaving the workspace reaches its file.
///
/// Chosen where the refusal is made, since only there is it known whether opening a directory would
/// make the same path work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remedy {
    /// None is offered: the path climbs with `..` through a link, off the top of the file system
    /// or to a place inside the root; a link carries it out; it names a credential location, which
    /// cannot be opened; or what was refused is a command's redirection or a rewind.
    Nothing,
    /// The path lands inside the root and in no opened directory, where a directory cannot be
    /// opened. It is named in some other way than relative to the root: from the top of the file
    /// system, through a link to the root or through `~`. The relative spelling reaches the file,
    /// so the planner is told that and not that the path leaves the workspace, which it does not.
    Inside,
    /// Opening the directory a file is in, or a name nothing is at yet would be created in, makes
    /// the same path reach it, for whatever the call does to it.
    Open,
    /// [`Remedy::Open`] for a call that reads the file, where dropping it is a second way to have
    /// it read. A drop only ever reads (DROP-3), so no other call is told of it.
    OpenOrDrop,
    /// [`Remedy::Open`] where the directory holds the working directory, so opening it leaves no
    /// delegate a checkout (CHECKOUT-7).
    OpenEndsCheckouts,
    /// [`Remedy::OpenOrDrop`] where the directory holds the working directory. The drop is named
    /// first, since it reaches the file and costs nothing.
    DropOrOpenEndsCheckouts,
    /// The path names a directory, which is opened as itself rather than through the directory it
    /// is in. This remedy names no drop for any call, since a directory cannot be dropped.
    OpenItself,
    /// [`Remedy::OpenItself`] where the directory holds the working directory, so opening it leaves
    /// no delegate a checkout (CHECKOUT-7).
    OpenItselfEndsCheckouts,
    /// The directory a file is in, or a name nothing is at yet would be created in, is or lies
    /// inside a credential location, which `/add-dir` and `request_path` both refuse (SANDBOX-18),
    /// so no directory is offered. A write is still pointed at the unconfined line where the
    /// session accepts one. A path that names such a directory is [`Remedy::Nothing`].
    Credential,
    /// [`Remedy::Credential`] for a call that reads the file, where a drop is the one way to have
    /// it read.
    CredentialDrop,
    /// `permissions.readsStayInWorkspace` refuses opening a directory, and the call is not a read,
    /// which a drop would not serve (PERM-16).
    Kept,
    /// [`Remedy::Kept`] for a call that reads the file, where a drop is the one way to have it read.
    Drop,
}

impl Remedy {
    /// The remedy as the trail records it, beside the refusal the planner was told of.
    pub fn offered(self) -> &'static str {
        match self {
            Self::Nothing => "none",
            Self::Inside => "name it relative to the workspace root",
            Self::Open => "open its directory",
            Self::OpenOrDrop => "open its directory, or drop the file",
            Self::OpenEndsCheckouts => {
                "open its directory, which holds the working directory and so ends checkouts"
            }
            Self::DropOrOpenEndsCheckouts => {
                "drop the file, or open its directory, which holds the working directory and so \
                 ends checkouts"
            }
            Self::OpenItself => "open the directory itself",
            Self::OpenItselfEndsCheckouts => {
                "open the directory itself, which holds the working directory and so ends \
                 checkouts"
            }
            Self::Credential => "none, since its directory is or lies inside a credential location",
            Self::CredentialDrop => {
                "drop the file, since its directory is or lies inside a credential location"
            }
            Self::Kept => {
                "none, since permissions.readsStayInWorkspace refuses opening its directory"
            }
            Self::Drop => {
                "drop the file, since permissions.readsStayInWorkspace refuses opening its directory"
            }
        }
    }
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.describe(self.carried_path()))
    }
}

impl std::error::Error for WorkspaceError {}

impl From<Denial> for WorkspaceError {
    fn from(value: Denial) -> Self {
        Self::Denied(value)
    }
}
