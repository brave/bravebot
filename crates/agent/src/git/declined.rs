//! The refusals this reader gives, and the sentences that word them for the planner.

use super::Query;

/// Why a repository or a question was not answered.
///
/// Each sentence is worded here and carries only what the planner wrote, never text read out of
/// the repository, so a refusal cannot be a route for a repository's bytes into the planner's
/// context without a label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declined {
    Untrusted,
    Fenced,
    NoRepository,
    LinkedGitDir,
    Linked,
    Alternates,
    CommonDir,
    Replaced,
    Include,
    Worktree,
    Format,
    Unreadable,
    NoCommits,
    TooSlow,
    Unknown(String),
    Ambiguous(String),
    Unsupported(String, &'static str),
    Kind {
        revision: String,
        wanted: &'static str,
    },
    NoSuchPath(String),
    PathInvalid(String),
    Withheld(String),
    ShowNeedsPath(String),
    TakesOne(Query, String),
    DiffNeedsTwo,
    SearchNeedsPattern,
    PatternIsForSearch(Query),
    TagsTakeNoPath,
    PairIsForDiff(String),
    UntrustedTree,
    StatusTakesNoRevision,
    SplitIndex,
    SparseIndex,
    Bare,
    Elsewhere,
}

impl Declined {
    /// The sentence the planner reads, for a repository it called `named`.
    ///
    /// What to do instead is not in it. git answers nearly everything this reader declines, `run` is
    /// how git is asked, and a turn is not always offered `run`, so that half is
    /// [`Declined::instead`] and the caller that knows the turn puts the two together.
    pub fn describe(&self, named: &str) -> String {
        match self {
            Declined::Untrusted => format!(
                "{named}/.git is not a directory this session trusts in full, so read_git does \
                 not open it: reading history means following what its files say."
            ),
            Declined::Fenced => format!(
                "{named}/.git holds a file a deny rule covers, and read_git reads every file \
                 there or none, so it does not open it."
            ),
            Declined::NoRepository => format!(
                "{named} has no .git directory holding a repository. read_git reads a repository \
                 whose .git is a directory."
            ),
            Declined::LinkedGitDir => format!(
                "{named}/.git is a file pointing elsewhere, as a linked worktree or a submodule \
                 has, and read_git does not follow it."
            ),
            Declined::Linked => {
                format!("{named}/.git holds a symbolic link, and read_git does not follow one.")
            }
            Declined::Alternates => format!(
                "{named}/.git borrows objects from another repository through \
                 objects/info/alternates, which read_git does not read."
            ),
            Declined::CommonDir => format!(
                "{named}/.git shares its refs and objects with another repository through \
                 commondir, which read_git does not follow."
            ),
            Declined::Replaced => format!(
                "{named}/.git replaces objects or grafts parents, through refs/replace or \
                 info/grafts, and read_git does not apply either."
            ),
            Declined::Include => format!(
                "{named}/.git/config includes configuration from another file, which read_git \
                 does not read."
            ),
            Declined::Worktree => {
                format!("{named}/.git/config sets core.worktree, which read_git does not follow.")
            }
            Declined::Format => format!(
                "{named}/.git/config names a repository format or extension read_git does not \
                 read, or is not configuration read_git can parse."
            ),
            Declined::Unreadable => format!(
                "{named}/.git could not be read as a repository: something it names is missing \
                 or damaged."
            ),
            Declined::NoCommits => format!("{named} has no commits yet."),
            Declined::TooSlow => {
                format!("Reading {named}/.git took longer than read_git allows.")
            }
            Declined::Unknown(revision) => {
                format!("{revision} names no commit, tag, branch or object in {named}.")
            }
            Declined::Ambiguous(revision) => format!(
                "{revision} is the start of more than one object id in {named}; give more of it."
            ),
            Declined::Unsupported(revision, what) => {
                format!("{revision} uses {what}, which read_git does not support.")
            }
            Declined::Kind { revision, wanted } => {
                format!("{revision} does not name a {wanted} in {named}.")
            }
            Declined::NoSuchPath(path) => {
                format!("{path} is not in that revision of {named}.")
            }
            Declined::PathInvalid(path) => format!(
                "{path} is not a path inside the repository: write it relative to the \
                 repository's root, without . or .. components."
            ),
            Declined::Withheld(path) => format!(
                "{path} in {named} is a path this session may not read, so read_git does not \
                 show it."
            ),
            Declined::ShowNeedsPath(revision) => format!(
                "{revision} names a tree or file by id alone, so where it sits in the repository \
                 is unknown and cannot be checked against what may be read. Name it as \
                 <commit>:<path>."
            ),
            Declined::TakesOne(query, revision) => format!(
                "{} takes one revision and {revision} is a range; use log to list its commits \
                 or diff to compare its ends.",
                query.word()
            ),
            Declined::DiffNeedsTwo => "diff compares two commits, written as A..B or as \"A B\". \
                 To see what one commit changed, use show."
                .to_owned(),
            Declined::SearchNeedsPattern => "search needs a pattern: the regular expression to \
                 look for in the files at the revision."
                .to_owned(),
            Declined::PatternIsForSearch(query) => format!(
                "a pattern is what search looks for in the files at a revision, and {} takes \
                 none.",
                query.word()
            ),
            Declined::TagsTakeNoPath => "tags lists the repository's tags and takes no path; to \
                 find the commits that changed a file, use log with the path."
                .to_owned(),
            Declined::PairIsForDiff(revision) => format!(
                "{revision} names two revisions, which only diff takes; for a range of commits \
                 write A..B."
            ),
            Declined::UntrustedTree => format!(
                "{named} is not a working tree this session trusts in full, so read_git does not \
                 read its status: status compares every file there with the index."
            ),
            Declined::StatusTakesNoRevision => "status compares the index and the working tree \
                 with HEAD and takes no revision; to compare commits, use diff."
                .to_owned(),
            Declined::SplitIndex => format!(
                "{named}/.git/index is split into a shared index, which read_git does not read."
            ),
            Declined::SparseIndex => {
                format!("{named}/.git/index is a sparse index, which read_git does not read.")
            }
            Declined::Bare => format!(
                "{named}/.git/config sets core.bare, so the repository has no working tree to \
                 read a status from."
            ),
            Declined::Elsewhere => format!(
                "{named}/.git/config names an ignore or attributes file outside the repository, \
                 through core.excludesFile, core.attributesFile or attr.tree, which read_git does \
                 not read."
            ),
        }
    }

    /// What answers the question instead, where `run` is the thing that would and the caller says
    /// the turn is offered one. Nothing where no other tool answers it.
    ///
    /// Written as a match over the whole enum rather than a list of the ones that have an answer, so
    /// a variant added later says here whether anything else reads what this reader would not.
    ///
    /// The two refusals a deny rule causes are the ones that stay silent although git would print
    /// the bytes: a file under `.git` a rule covers, and a path in the repository a rule covers. A
    /// `Read` rule does not bar a command line, so the sentence would read as how to get past a rule
    /// somebody wrote, and whether advice may point there is the permission rules' question rather
    /// than this reader's.
    pub fn instead(&self) -> Option<&'static str> {
        let read = "Use run to read it with git instead.";
        match self {
            Declined::Untrusted
            | Declined::NoRepository
            | Declined::LinkedGitDir
            | Declined::Linked
            | Declined::Alternates
            | Declined::CommonDir
            | Declined::Replaced
            | Declined::Include
            | Declined::Worktree
            | Declined::Format
            | Declined::Unreadable
            | Declined::TooSlow
            | Declined::UntrustedTree
            | Declined::SplitIndex
            | Declined::SparseIndex
            | Declined::Bare
            | Declined::Elsewhere => Some(read),
            Declined::Unsupported(..) => Some("Use run to ask git for it."),
            Declined::DiffNeedsTwo => Some("To compare with the working tree, use run."),
            // A deny rule's two refusals, for the reason above.
            Declined::Fenced | Declined::Withheld(_) => None,
            // Questions this reader answers, asked about something that is not there or asked the
            // wrong way round. The sentence already says what to ask instead, and git would answer
            // no better.
            Declined::NoCommits
            | Declined::Unknown(_)
            | Declined::Ambiguous(_)
            | Declined::Kind { .. }
            | Declined::NoSuchPath(_)
            | Declined::PathInvalid(_)
            | Declined::ShowNeedsPath(_)
            | Declined::TakesOne(..)
            | Declined::SearchNeedsPattern
            | Declined::PatternIsForSearch(_)
            | Declined::TagsTakeNoPath
            | Declined::PairIsForDiff(_)
            | Declined::StatusTakesNoRevision => None,
        }
    }
}
