//! The caps on a walk, a search and a page, and the plain structs the file tools hand back. They
//! hold what was found and how much of it there was, and read no file themselves.

use bravebot_filetype::Decoding;
use std::time::Duration;

/// Caps on directory walks, so a large tree cannot stall a turn or flood the model's
/// context. Truncation is size hygiene, not filtering: nothing is inspected to decide
/// what to drop.
pub(crate) const MAX_ENTRIES: usize = 2_000;

/// How many files a *search* may walk before it gives up on the rest.
///
/// Far above [`MAX_ENTRIES`] because the two caps guard different things. A listing's paths
/// are the answer and every one of them is spent on context, so a listing is capped at what
/// is worth reading. A search's paths are never shown: only matching lines are, and those
/// have their own cap in [`MAX_MATCHES`]. Holding a search to a listing's budget bought no
/// context back and cost whole subtrees, which is the failure that matters: a search that
/// stops after a couple of thousand files reports nothing for a needle it never looked for,
/// and nothing reads as an answer.
///
/// Walking is cheap: a path is a stat and a string. Reading is not, which is what
/// [`MAX_SEARCH_TIME`] is for.
///
/// Past what a tree a person works in usually holds, which is not the same as past every tree:
/// a monorepo or a checkout of generated sources reaches this, and
/// [`Workspace::with_search_caps`] is how one says so.
pub const MAX_SEARCH_FILES: usize = 100_000;

/// The largest source file a repository map reads, in bytes. Past it a file is generated or data.
pub(super) const MAX_MAP_FILE_BYTES: usize = 512 * 1024;

/// The most source a repository map reads in all, in bytes.
pub(super) const MAX_MAP_BYTES: usize = 32 * 1024 * 1024;

pub(super) const MAX_MATCHES: usize = 200;

pub(super) const MAX_MATCH_LINE: usize = 500;

/// The most lines a search shows on each side of a match, whatever it asked for.
pub const MAX_SEARCH_CONTEXT: usize = 10;

/// How many context lines one search returns in all, beside the matches.
///
/// A cap of its own because the match cap cannot bound them: two hundred matches with the most
/// context each would be four thousand lines, a result the planner pays for in every later round.
/// Reached, the search keeps the matches and stops adding the lines around them, and says so.
pub(super) const MAX_CONTEXT_LINES: usize = 1_000;

/// How many files a `files` or `count` result lists.
///
/// The match cap does not bound them, since neither shape returns a match, and a common word can
/// name a hundred thousand files. A count's total is exact past it; the listing is what stops.
pub(super) const MAX_TALLIED_FILES: usize = 200;

/// How long a search may spend opening files, where nothing configured otherwise.
///
/// The match cap already stops a *productive* search early. This is for the other one: a
/// pattern that matches nothing is read to the end of the tree, so on a large repository the
/// worst case is every file. A wall-clock budget bounds that without bounding the useful
/// case, and stopping is reported the same way the entry cap is, since the answer is partial
/// either way, and what the reader must not do is take it for complete.
///
/// Ten seconds is a guess about a filesystem rather than a fact about one, which is why
/// [`Workspace::with_search_caps`] exists: a network mount reads an order of magnitude slower
/// than a local disk, and nothing here can tell which it is on.
pub(super) const MAX_SEARCH_TIME: Duration = Duration::from_secs(10);

/// Caps on a single paged read.
///
/// A turn re-sends the whole message history each round, so the cost of one oversized read
/// is paid repeatedly. These bound a page rather than the file: the rest stays reachable by
/// asking for a later offset.
///
/// The line cap counts characters, not bytes, for the reason READ-2 gives: a cap in bytes is
/// a different cap for every script, since 2000 bytes is 2000 characters of English and 666
/// of Japanese, so a line the clause allows in full comes back cut to a third and reported as
/// shortened. The price is that a page of four-byte characters costs up to four times the
/// bytes a page of ASCII does.
pub(super) const MAX_PAGE_LINES: usize = 500;

pub(super) const MAX_LINE: usize = 2_000;

/// Characters in a whole page, counting a newline after each line. Without it the worst page is
/// `MAX_PAGE_LINES` x `MAX_LINE`, a million characters, which a minified bundle reaches in one read.
pub(super) const MAX_PAGE_CHARS: usize = 100_000;

// A shortened line and its notice always fit, so a page is never empty because of the budget.
const _: () = assert!(MAX_LINE + 64 < MAX_PAGE_CHARS);

/// A bounded window of a file's lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// The lines in this window, each capped at [`MAX_LINE`] characters and all together at
    /// [`MAX_PAGE_CHARS`], so a page can end before the line limit does.
    pub lines: Vec<String>,
    /// Whether the file ends with a newline.
    ///
    /// Lost otherwise: the lines are joined back together with newlines between them and none
    /// after, so a file that went through a slot came back a byte shorter than it went in. That
    /// is a change to every file processed this way, and it shows up in the next diff somebody
    /// reads as "no newline at end of file".
    pub ends_with_newline: bool,
    /// 1-based number of the first line returned.
    pub first_line: usize,
    /// Lines in the whole file, so a caller can tell there is more to ask for.
    pub total_lines: usize,
    /// How many returned lines were individually shortened.
    pub long_lines: usize,
    /// What to compare against the next look to see whether the file was written.
    ///
    /// See [`change_token`]. Of the whole file rather than of this window, so a paged read and a
    /// whole one describe the same file the same way: what is being watched is the file, and a
    /// window of it changing is not a different question.
    pub change_token: String,
    /// How the file's bytes became these lines. Anything but [`Decoding::Utf8`] means the lines are
    /// a reading of the file and not its bytes.
    pub decoding: Decoding,
}

impl Page {
    /// 1-based line number just past this window, when the file continues.
    pub fn next_line(&self) -> Option<usize> {
        let past = self.first_line + self.lines.len();
        (past <= self.total_lines).then_some(past)
    }
}

/// One grep hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Workspace-relative path.
    pub path: String,
    /// 1-based line number.
    pub line: usize,
    /// The matching line, truncated to [`MAX_MATCH_LINE`].
    pub text: String,
}

/// What shape a search's result takes.
///
/// A choice about the call and never about what was read, so it is routing: the walk and the match
/// loop are the same for all three, and what differs is what is kept from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchOutput {
    /// The matching lines, as a search always returned them.
    #[default]
    Lines,
    /// Each file holding a match, once, and no line.
    Files,
    /// How many lines match in each file, and in all.
    Count,
}

impl SearchOutput {
    /// The mode a name asks for, or `None` for a name that is not one.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "lines" => Some(Self::Lines),
            "files" => Some(Self::Files),
            "count" => Some(Self::Count),
            _ => None,
        }
    }

    /// Whether this result is a summary of the matches rather than the matches themselves.
    pub fn summarises(self) -> bool {
        self != Self::Lines
    }
}

/// A file holding matches, and how many lines in it match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTally {
    /// Workspace-relative path.
    pub path: String,
    /// How many lines of the file match. 1 in a `files` result, which stops reading the file at the
    /// first.
    pub lines: usize,
}

/// A line shown beside a match because it is near one, and not because it matched.
///
/// Kept apart from [`Match`] so that it counts toward neither [`MAX_MATCHES`] nor an offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextLine {
    /// Workspace-relative path.
    pub path: String,
    /// 1-based line number.
    pub line: usize,
    /// The line, truncated to [`MAX_MATCH_LINE`].
    pub text: String,
}

/// The result of a directory listing.
///
/// Carries whether a cap was reached, because a model shown exactly [`MAX_ENTRIES`] paths
/// with no notice will reason as though it saw the whole tree. Silent truncation is worse
/// than a short answer: it looks like completeness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub files: Vec<String>,
    /// Directories the walk stopped at because it had reached the depth it was given.
    ///
    /// Empty when the walk was unbounded, where every directory is descended into and the files
    /// beneath it are the listing. A bounded walk reports them because a listing of names with
    /// nothing said about the directories beside them describes a tree with no branches, and a
    /// planner reading one concludes the project has no source directory.
    pub directories: Vec<String>,
    /// Whether files were left out because a cap was reached.
    pub truncated: bool,
    /// Whether a directory beneath the one named could not be opened and was left out.
    ///
    /// A fact about the shape of the walk and never the directory's name: the error that opening
    /// it produced spells that name, which is a name out of the tree and would reach the planner
    /// as a sentence the driver wrote (LIST-2).
    pub unreadable: bool,
    /// How many symbolic links the walk met and did not enter.
    ///
    /// A number and never a name, for the same reason as `unreadable`. A directory made only of
    /// links lists as empty without it, and a planner reads an empty listing as an empty
    /// directory (LIST-6).
    pub links: usize,
}

/// The result of a repository map.
///
/// Labelled by the files it was built from, which are only ever files the trust map vouches for,
/// so a map is trusted whenever it is returned. What it says of the files it left out is a count
/// and never a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoMap {
    /// Files with their declarations, most referenced first. Empty when none was found.
    pub body: String,
    /// Declarations found in the files read.
    pub found: usize,
    /// Declarations the body shows.
    pub shown: usize,
    /// Source files read.
    pub files: usize,
    /// Source files left out because the trust map does not vouch for them.
    pub unvouched: usize,
    /// Source files left out for another reason: unreadable, over the size caps, or holding what
    /// looks like a credential. One count for the three, so the planner is not told which file
    /// held one.
    pub skipped: usize,
    /// Whether a cap on files or bytes stopped the walk or the reading short.
    pub truncated: bool,
    /// Whether a directory beneath the one named could not be opened.
    pub unreadable: bool,
}

/// The result of a content search. Reports truncation for the same reason as [`Listing`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matches {
    pub matches: Vec<Match>,
    /// The lines around the matches that were asked for, in file and line order, without a line
    /// that is itself one of `matches`. Empty for a search that asked for no context.
    pub context: Vec<ContextLine>,
    /// Whether the cap on context lines stopped the lines around some match being added.
    ///
    /// The matches are all there; it is the lines beside the later ones that are missing, which
    /// reads as those matches having nothing near them unless it is said.
    pub context_truncated: bool,
    /// What this result is: the lines in `matches`, or a summary in `tallies`.
    pub output: SearchOutput,
    /// Each file holding a match, in walk order, for a `files` or `count` search. Empty for lines.
    pub tallies: Vec<FileTally>,
    /// Whether the cap on listed files cut `tallies` short. `matched` still counts every match the
    /// walk read, so a `count` total stays whole.
    pub tallies_truncated: bool,
    /// Whether matches were left out because the match cap was reached.
    pub truncated: bool,
    /// Whether files were left unopened because the walk hit its entry cap.
    ///
    /// A separate fact from `truncated` and the more dangerous of the two: a search that stopped
    /// short of the tree reports no matches for a needle it never looked for, which reads exactly
    /// like the needle not being there.
    pub unvisited: bool,
    /// Whether reading stopped because [`MAX_SEARCH_TIME`] ran out.
    ///
    /// Dangerous in the same way as `unvisited`, and for the same reason: what was not read
    /// cannot have matched.
    pub timed_out: bool,
    /// How many files the `include` glob selected.
    ///
    /// The number that separates the two ways a search comes back empty. Zero means the glob
    /// picked no files at all, so nothing was ever read and the result says nothing about
    /// whether the needle is in the tree: a broken query, not evidence. Anything above zero
    /// means files were read and the needle was not in them, which is evidence. Rendered as
    /// one message, those two are indistinguishable, and a reader who cannot tell them apart
    /// treats a typo as proof of absence.
    pub considered: usize,
    /// How many of the selected files were actually opened.
    ///
    /// Below `considered` when a cap or the clock stopped the read early. Reported so the
    /// question "was this search complete?" has an answer in the result rather than in
    /// another call.
    pub searched: usize,
    /// Whether a permission rule kept files out of the walk.
    ///
    /// The third reason a search read nothing, and the one the other two must not be mistaken
    /// for: a glob that selected no files is a query to rewrite, and a rule is not. Whether, never
    /// which: the names are what the rule is keeping back.
    pub withheld: bool,
    /// 1-based position of the first match returned, within all the matches found.
    ///
    /// 1 for a search that started at the beginning, and whatever offset was asked for otherwise.
    pub first_match: usize,
    /// What was returned plus what an offset passed over.
    ///
    /// Where `truncated` is set this is how far the search counted rather than how many matches the
    /// tree holds. It answers the one question a page cannot: an offset past the last match returns
    /// nothing, and nothing reads as the pattern being absent unless the count says otherwise.
    pub matched: usize,
    /// How many symbolic links the walk met and did not enter.
    ///
    /// A number and never a name. A search whose files are behind links reads nothing there, and
    /// without the count that reads as the pattern being absent or the glob being wrong (SEARCH-13).
    pub links: usize,
}

impl Matches {
    /// Whether the search returned nothing to show, in whichever shape it was asked for.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty() && self.tallies.is_empty()
    }

    /// Where a further search continues, or that this page fell past the last match.
    ///
    /// `None` for an ordinary complete answer, and for one a cap other than the cap on matches
    /// stopped: a walk that gave up on the tree or ran out of time reached neither the end of the
    /// matches nor a count of them, so it has no page to offer and nothing to say about the end.
    /// Narrowing the search is the only advice left there.
    pub fn paging(&self) -> Option<Paging> {
        if self.unvisited || self.timed_out {
            return None;
        }
        if self.truncated {
            return Some(Paging::Continue(self.first_match + self.matches.len()));
        }
        (self.first_match > 1 && self.matches.is_empty() && self.matched > 0).then_some(
            Paging::PastTheEnd {
                found: self.matched,
            },
        )
    }
}

/// What a paged result has to say about the matches it did not return.
///
/// Either reaches the planner beside the reference rather than only inside the body, for the reason
/// a truncation notice does: by default the body is quarantined and the planner reads a reference
/// instead, so a sentence written into it reaches nobody who could act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paging {
    /// The match cap cut the result short, and this offset asks for the rest.
    Continue(usize),
    /// The offset asked for is past the last match, and this many were found in all.
    ///
    /// Carried so that an empty page cannot be read as the pattern having gone from the tree
    /// between two calls. The count discloses no more than the body's own line count already does.
    PastTheEnd { found: usize },
}
