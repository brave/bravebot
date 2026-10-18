//! Recalling earlier prompts.
//!
//! Every submitted prompt is kept, and Up walks backwards through them the way a shell does.
//! Retyping a long question because it needed one word changed is the kind of friction that makes
//! an interface tiring.
//!
//! Browsing is a mode rather than an edit: while it is active the box shows a stored prompt and
//! reports the position, and leaving the mode restores whatever was being typed before. That way
//! pressing Up out of curiosity cannot destroy a half-written line.
//!
//! Walking back one at a time is no way to reach the hundredth prompt, so each entry also carries
//! when it was sent and which workspace it was sent from. Both are for
//! [`crate::history_search`], which is the other way in: a list a person reads and narrows, where
//! an age says which of two similar prompts is the one they mean and the workspace says whether a
//! prompt belongs to what they are doing now.

use bravebot_session::store::Entry;

/// One submission's claim on a recall entry. Kept only while this process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ticket(usize);

/// Which prompts a walk or a search offers. Up and the search share it, so what each scope holds
/// is decided once.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Every stored prompt, from every session and workspace.
    #[default]
    All,
    /// The prompts sent from the workspace the session runs in. An entry recorded with no
    /// workspace belongs to none, so it is in [`Scope::All`] only.
    Project,
    /// The prompts this session sent, and the ones its record holds when it was resumed.
    Session,
}

impl Scope {
    /// The scope the chord moves to from this one, in the order everywhere, this project, this
    /// session, and back.
    pub fn next(self) -> Self {
        match self {
            Scope::All => Scope::Project,
            Scope::Project => Scope::Session,
            Scope::Session => Scope::All,
        }
    }

    /// Whether `entry` is in this scope. `mine` says it is this session's, and `project` is the
    /// workspace the session runs in.
    pub fn contains(self, entry: &Entry, mine: bool, project: Option<&str>) -> bool {
        match self {
            Scope::All => true,
            Scope::Project => project
                .filter(|project| !project.is_empty())
                .is_some_and(|project| entry.project.as_deref() == Some(project)),
            Scope::Session => mine,
        }
    }
}

/// Prompts already sent, and where the user is looking.
#[derive(Debug, Default)]
pub struct History {
    /// Oldest first, so the newest is at the end.
    entries: Vec<Entry>,
    /// Stable identities and submission counts, aligned with entries. Duplicates share a claim.
    claims: Vec<(Ticket, usize)>,
    /// Whether each entry belongs to this session, aligned with entries. The stored file does not
    /// say, so the prompts this process sent and the ones a resumed record holds are marked here.
    mine: Vec<bool>,
    scope: Scope,
    /// The workspace the session runs in, which [`Scope::Project`] compares each entry against.
    project: Option<String>,
    next_ticket: usize,
    /// How far back the user has walked. `None` means they are editing, not browsing.
    ///
    /// Counted from the newest entry of the scope: 1 is the most recent prompt, and 0 is the kept draft, which
    /// sits in front of the walk. Stored as a distance rather than an index so appending an entry
    /// cannot silently move what is being viewed.
    back: Option<usize>,
    /// The line last cleared with Escape or Ctrl-C. It is not a sent prompt, so it is not in
    /// `entries`, is never written to the history file and is not offered to the search.
    draft: Option<String>,
    /// What was in the input box before browsing started, to restore on the way out.
    stashed: String,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start from prompts stored by earlier sessions, oldest first.
    pub fn from_entries(entries: Vec<Entry>) -> Self {
        Self {
            claims: (0..entries.len()).map(|id| (Ticket(id), 1)).collect(),
            mine: vec![false; entries.len()],
            scope: Scope::All,
            project: None,
            next_ticket: entries.len(),
            entries,
            back: None,
            draft: None,
            stashed: String::new(),
        }
    }

    /// Every stored prompt, oldest first, for writing back to disk and for searching.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Keep a line that was cleared, replacing any draft kept before it.
    pub fn keep_draft(&mut self, line: String) {
        self.draft = Some(line);
    }

    /// Say which workspace the session runs in, for [`Scope::Project`].
    pub fn set_project(&mut self, project: Option<String>) {
        self.project = project;
        if !self.holds(self.scope) {
            self.scope = Scope::All;
        }
    }

    /// Whether the entry at `at` is in `scope`.
    pub fn contains(&self, at: usize, scope: Scope) -> bool {
        scope.contains(&self.entries[at], self.mine[at], self.project.as_deref())
    }

    /// Whether any stored prompt is in `scope`.
    pub fn holds(&self, scope: Scope) -> bool {
        (0..self.entries.len()).any(|at| self.contains(at, scope))
    }

    /// The scope the chord moves to from `from`: the next of everywhere, this project and this
    /// session that holds a prompt, skipping the ones that hold none. `None` when no other scope
    /// holds one.
    pub fn following(&self, from: Scope) -> Option<Scope> {
        let mut scope = from;
        for _ in 0..2 {
            scope = scope.next();
            if self.holds(scope) {
                return Some(scope);
            }
        }
        None
    }

    /// The entries Up walks now, as indexes into `entries`, oldest first.
    fn visible(&self) -> Vec<usize> {
        (0..self.entries.len())
            .filter(|at| self.contains(*at, self.scope))
            .collect()
    }

    /// Whether Up has anything to bring back: a sent prompt in the scope or a kept draft.
    pub fn can_recall(&self) -> bool {
        self.draft.is_some() || self.holds(self.scope)
    }

    /// Which prompts Up is walking.
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// The scope the chord would move the walk to: a sent prompt is on screen and another scope
    /// holds a prompt. With none, the chord is not on offer.
    pub fn next_scope(&self) -> Option<Scope> {
        self.on_a_sent_prompt()
            .then(|| self.following(self.scope))
            .flatten()
    }

    /// Move the walk to the next scope, staying on the prompt on screen where it is in the new
    /// scope, otherwise landing on the newest prompt of the new scope before it, and failing that
    /// the oldest. Returns the prompt now on screen.
    pub fn cycle(&mut self) -> Option<String> {
        let scope = self.next_scope()?;
        let back = self.back.filter(|back| *back > 0)?;
        let list = self.visible();
        let shown = list[list.len() - back];
        self.scope = scope;
        let list = self.visible();
        let place = list.iter().rposition(|each| *each <= shown).unwrap_or(0);
        self.back = Some(list.len() - place);
        Some(self.entries[list[place]].prompt.clone())
    }

    /// Count the prompt a resumed record holds as this session's: the newest stored entry with
    /// these words that is not already counted.
    pub fn adopt(&mut self, prompt: &str) {
        if let Some(at) = (0..self.entries.len())
            .rev()
            .find(|at| !self.mine[*at] && self.entries[*at].prompt == prompt)
        {
            self.mine[at] = true;
        }
    }

    /// Start a new session's own prompts, as `/clear` does.
    pub fn forget_session(&mut self) {
        self.mine.iter_mut().for_each(|mine| *mine = false);
        self.scope = Scope::All;
        self.leave();
    }

    /// Whether the box shows a sent prompt, as opposed to a line being typed or the kept draft.
    pub fn on_a_sent_prompt(&self) -> bool {
        self.back.is_some_and(|back| back > 0)
    }

    /// Record a submitted prompt, sent now from `project`.
    ///
    /// Sending drops the kept draft, since the line it was kept from has been superseded.
    ///
    /// Consecutive duplicates are collapsed: sending the same thing twice is usually a retry, and
    /// two identical entries make walking back slower without adding anything. The kept entry is
    /// the older one, since the prompt is the same and the first time it was asked is when the
    /// question was new.
    pub fn push(&mut self, prompt: impl Into<String>, project: Option<String>) -> Option<&Entry> {
        let entry = Entry::sent(prompt, project);
        self.leave();
        self.draft = None;
        if self
            .entries
            .last()
            .is_some_and(|last| last.prompt == entry.prompt)
        {
            self.claims.last_mut().expect("an existing entry").1 += 1;
            *self.mine.last_mut().expect("an existing entry") = true;
            return None;
        }
        self.mine.push(true);
        self.claims.push((Ticket(self.next_ticket), 1));
        self.next_ticket += 1;
        self.entries.push(entry);
        self.entries.last()
    }

    /// The entry just submitted, including a submission collapsed into its predecessor.
    pub(crate) fn ticket(&self) -> Ticket {
        self.claims.last().expect("a submitted prompt").0
    }

    /// Cancel one submission without removing other submissions that share its words. Returns the
    /// entry that left the list, if one did.
    pub(crate) fn withdraw(&mut self, ticket: Ticket) -> Option<Entry> {
        self.leave();
        let at = self.claims.iter().position(|(id, _)| *id == ticket)?;
        self.claims[at].1 -= 1;
        if self.claims[at].1 == 0 {
            self.claims.remove(at);
            self.mine.remove(at);
            let entry = self.entries.remove(at);
            if !self.holds(self.scope) {
                self.scope = Scope::All;
            }
            return Some(entry);
        }
        None
    }

    /// How many prompts are stored.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether the user is currently looking at a stored prompt.
    pub fn is_browsing(&self) -> bool {
        self.back.is_some()
    }

    /// The position to show, as `(index, total)`, counting oldest first.
    ///
    /// `None` when not browsing. The index is the entry's ordinal rather than its distance back,
    /// because "History 78/83" reads as a place in a list.
    pub fn position(&self) -> Option<(usize, usize)> {
        let back = self.back.filter(|back| *back > 0)?;
        let total = self.visible().len();
        Some((total + 1 - back, total))
    }

    /// Step one prompt further back, returning what to show.
    ///
    /// `current` is what is in the input box now, kept so it can be restored on the way out.
    /// Returns `None` at the oldest entry, leaving the view where it is rather than wrapping:
    /// wrapping to the newest would look like the key had stopped working.
    pub fn older(&mut self, current: &str) -> Option<String> {
        let list = self.visible();
        let back = match self.back {
            None => {
                if let Some(draft) = &self.draft {
                    self.stashed = current.to_string();
                    self.back = Some(0);
                    return Some(draft.clone());
                }
                if list.is_empty() {
                    return None;
                }
                self.stashed = current.to_string();
                1
            }
            Some(back) if back < list.len() => back + 1,
            // Already at the oldest.
            Some(_) => return None,
        };

        self.back = Some(back);
        Some(self.entries[list[list.len() - back]].prompt.clone())
    }

    /// Step one prompt forward, returning what to show.
    ///
    /// Stepping forward from the newest entry leaves browsing and restores the line that was
    /// being typed, which is what makes Up safe to press speculatively.
    pub fn newer(&mut self) -> Option<String> {
        let list = self.visible();
        match self.back {
            None => None,
            Some(1) if self.draft.is_some() => {
                self.back = Some(0);
                self.draft.clone()
            }
            Some(0) | Some(1) => {
                self.back = None;
                Some(std::mem::take(&mut self.stashed))
            }
            Some(back) => {
                self.back = Some(back - 1);
                Some(self.entries[list[list.len() - (back - 1)]].prompt.clone())
            }
        }
    }

    /// Stop browsing without changing the input.
    pub fn leave(&mut self) {
        self.back = None;
        self.stashed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(prompts: &[&str]) -> History {
        let mut history = History::new();
        for prompt in prompts {
            history.push(*prompt, None);
        }
        history
    }

    #[test]
    fn a_new_history_is_empty_and_not_browsing() {
        let history = History::new();
        assert!(history.is_empty());
        assert!(!history.is_browsing());
        assert_eq!(history.position(), None);
    }

    /// Up with nothing stored must do nothing rather than clearing the line.
    #[test]
    fn an_empty_history_has_nothing_to_recall() {
        let mut history = History::new();
        assert_eq!(history.older("typing"), None);
        assert!(!history.is_browsing());
    }

    #[test]
    fn up_recalls_the_most_recent_prompt_first() {
        let mut history = with(&["first", "second", "third"]);
        assert_eq!(history.older("").as_deref(), Some("third"));
    }

    #[test]
    fn up_keeps_walking_backwards() {
        let mut history = with(&["first", "second", "third"]);
        assert_eq!(history.older("").as_deref(), Some("third"));
        assert_eq!(history.older("").as_deref(), Some("second"));
        assert_eq!(history.older("").as_deref(), Some("first"));
    }

    /// Wrapping round to the newest would look like the key had stopped working, so the view
    /// stays put at the oldest entry.
    #[test]
    fn up_stops_at_the_oldest_entry() {
        let mut history = with(&["only"]);
        assert_eq!(history.older("").as_deref(), Some("only"));
        assert_eq!(history.older(""), None);
        assert_eq!(history.position(), Some((1, 1)));
    }

    #[test]
    fn down_walks_forwards_again() {
        let mut history = with(&["first", "second", "third"]);
        history.older("");
        history.older("");
        assert_eq!(history.newer().as_deref(), Some("third"));
    }

    /// The reason Up is safe to press speculatively: whatever was being typed comes back.
    #[test]
    fn leaving_the_newest_entry_restores_the_typed_line() {
        let mut history = with(&["stored"]);
        assert_eq!(history.older("half typed").as_deref(), Some("stored"));
        assert_eq!(history.newer().as_deref(), Some("half typed"));
        assert!(!history.is_browsing());
    }

    #[test]
    fn down_does_nothing_when_not_browsing() {
        let mut history = with(&["stored"]);
        assert_eq!(history.newer(), None);
    }

    /// The position is what the interface shows, so it must match the screenshot's reading: an
    /// ordinal in a list, oldest first.
    #[test]
    fn the_position_counts_from_the_oldest() {
        let mut history = History::new();
        for n in 1..=83 {
            history.push(format!("prompt {n}"), None);
        }

        // One press shows the newest, which is the 83rd of 83.
        history.older("");
        assert_eq!(history.position(), Some((83, 83)));

        // Walking back to the 78th, as in the screenshot.
        for _ in 0..5 {
            history.older("");
        }
        assert_eq!(history.position(), Some((78, 83)));
    }

    /// Sending the same prompt twice is usually a retry, and a duplicate only makes walking back
    /// slower.
    #[test]
    fn consecutive_duplicates_are_collapsed() {
        let history = with(&["same", "same", "other", "same"]);
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn submitting_leaves_browsing() {
        let mut history = with(&["first", "second"]);
        history.older("");
        assert!(history.is_browsing());
        history.push("third", None);
        assert!(!history.is_browsing(), "still browsing after a submission");
    }

    /// A cancelled prompt goes back in the input box, so it must leave history too rather than
    /// being offered from two places.
    #[test]
    fn withdrawing_removes_the_cancelled_entry() {
        let mut history = with(&["first", "second"]);
        history.withdraw(history.ticket());
        assert_eq!(history.len(), 1);
        assert_eq!(history.older("").as_deref(), Some("first"));
    }

    /// Cancelling the only prompt this session sent while narrowed would leave Up with nothing to
    /// walk, and no prompt on screen to offer the scope chord from, so the walk goes back to every
    /// stored prompt.
    #[test]
    fn withdrawing_the_last_prompt_of_this_session_widens_the_scope() {
        let mut history = stored(&["old"]);
        history.push("new", None);
        history.older("");
        assert_eq!(history.cycle().as_deref(), Some("new"));
        assert_eq!(history.scope(), Scope::Session);

        assert!(history.withdraw(history.ticket()).is_some());

        assert_eq!(history.scope(), Scope::All);
        assert_eq!(history.next_scope(), None);
        assert!(history.can_recall());
        assert_eq!(history.older("").as_deref(), Some("old"));
    }

    /// Browsing then submitting a new prompt must not corrupt the position, which is why the
    /// distance is stored rather than an index.
    #[test]
    fn appending_while_browsing_does_not_shift_the_view() {
        let mut history = with(&["a", "b"]);
        history.older("");
        history.push("c", None);

        // No longer browsing, and a fresh walk back sees the new entry first.
        assert!(!history.is_browsing());
        assert_eq!(history.older("").as_deref(), Some("c"));
        assert_eq!(history.position(), Some((3, 3)));
    }

    /// The kept draft is the front of the walk, ahead of the newest sent prompt, and the sent
    /// prompts are still behind it.
    #[test]
    fn up_brings_the_draft_back_before_the_newest_prompt() {
        let mut history = with(&["first", "second"]);
        history.keep_draft("cleared".to_string());
        assert_eq!(history.older("").as_deref(), Some("cleared"));
        assert_eq!(history.older("").as_deref(), Some("second"));
        assert_eq!(history.older("").as_deref(), Some("first"));
        assert_eq!(history.older(""), None);
    }

    /// Down from the newest sent prompt passes through the draft, and leaving it restores the
    /// line that was being typed.
    #[test]
    fn down_walks_forward_through_the_draft_to_the_typed_line() {
        let mut history = with(&["sent"]);
        history.keep_draft("cleared".to_string());
        history.older("typing");
        history.older("typing");
        assert_eq!(history.newer().as_deref(), Some("cleared"));
        assert_eq!(history.newer().as_deref(), Some("typing"));
        assert!(!history.is_browsing());
    }

    /// With nothing sent yet, the draft is all Up has to bring back.
    #[test]
    fn a_draft_is_recalled_when_nothing_has_been_sent() {
        let mut history = History::new();
        assert!(!history.can_recall());
        history.keep_draft("cleared".to_string());
        assert!(history.can_recall());
        assert_eq!(history.older("").as_deref(), Some("cleared"));
        assert_eq!(history.older(""), None);
        assert_eq!(history.newer().as_deref(), Some(""));
    }

    /// One slot: a second cleared line takes the first one's place.
    #[test]
    fn a_second_draft_replaces_the_first() {
        let mut history = with(&["sent"]);
        history.keep_draft("one".to_string());
        history.keep_draft("two".to_string());
        assert_eq!(history.older("").as_deref(), Some("two"));
        assert_eq!(history.older("").as_deref(), Some("sent"));
        assert_eq!(history.newer().as_deref(), Some("two"));
    }

    /// Sending any prompt drops the draft, including a prompt collapsed into the one before it.
    #[test]
    fn sending_drops_the_draft() {
        let mut history = with(&["sent"]);
        history.keep_draft("cleared".to_string());
        history.push("sent", None);
        assert_eq!(history.older("").as_deref(), Some("sent"));
        assert_eq!(history.newer().as_deref(), Some(""));
    }

    /// The draft is not a sent prompt, so it is not among the entries written to disk and searched.
    #[test]
    fn the_draft_is_not_a_stored_entry() {
        let mut history = with(&["sent"]);
        history.keep_draft("cleared".to_string());
        assert_eq!(history.len(), 1);
        assert!(
            history
                .entries()
                .iter()
                .all(|entry| entry.prompt != "cleared")
        );
    }

    /// The border names a place in the list of sent prompts, and the draft is not in it.
    #[test]
    fn the_draft_has_no_position_in_the_list_of_sent_prompts() {
        let mut history = with(&["sent"]);
        history.keep_draft("cleared".to_string());
        history.older("");
        assert!(history.is_browsing());
        assert!(!history.on_a_sent_prompt());
        assert_eq!(history.position(), None);
        history.older("");
        assert!(history.on_a_sent_prompt());
        assert_eq!(history.position(), Some((1, 1)));
    }

    fn stored(prompts: &[&str]) -> History {
        History::from_entries(
            prompts
                .iter()
                .map(|prompt| Entry::sent(*prompt, None))
                .collect(),
        )
    }

    /// Up walks every stored prompt, this session's and earlier ones alike.
    #[test]
    fn up_walks_every_stored_prompt_by_default() {
        let mut history = stored(&["old"]);
        assert!(history.can_recall());
        assert_eq!(history.scope(), Scope::All);
        assert_eq!(history.older("").as_deref(), Some("old"));
        history.newer();
        history.push("new", None);
        assert_eq!(history.older("").as_deref(), Some("new"));
        assert_eq!(history.older("").as_deref(), Some("old"));
        assert_eq!(history.older(""), None);
        assert_eq!(history.position(), Some((1, 2)));
    }

    /// The switch to this session's prompts is on offer only where this session has some, and
    /// only from a prompt on screen.
    #[test]
    fn this_sessions_scope_is_offered_only_from_a_prompt_when_the_session_has_sent_one() {
        let mut history = stored(&["old"]);
        assert_eq!(history.next_scope(), None);
        history.older("");
        assert_eq!(history.next_scope(), None, "offered with nothing sent");
        history.newer();
        history.push("new", None);
        assert_eq!(
            history.next_scope(),
            None,
            "offered with no prompt on screen"
        );
        history.older("");
        assert_eq!(history.next_scope(), Some(Scope::Session));
        history.cycle();
        assert_eq!(history.next_scope(), Some(Scope::All));
    }

    /// A prompt a resumed record holds is counted, and clearing the conversation uncounts it and
    /// puts the walk back on every stored prompt.
    #[test]
    fn a_resumed_prompt_is_this_sessions_until_the_conversation_is_cleared() {
        let mut history = stored(&["a", "b", "c"]);
        history.adopt("b");
        assert_eq!(history.older("").as_deref(), Some("c"));
        assert_eq!(history.next_scope(), Some(Scope::Session));
        assert_eq!(history.cycle().as_deref(), Some("b"));
        assert_eq!(history.position(), Some((1, 1)));
        history.forget_session();
        assert_eq!(history.scope(), Scope::All);
        assert_eq!(history.older("").as_deref(), Some("c"));
        assert_eq!(history.next_scope(), None);
    }

    /// Switching keeps the prompt, and where the prompt is not this session's it lands on the
    /// nearest of this session's before it, and failing that the oldest of them.
    #[test]
    fn narrowing_from_a_prompt_of_another_session_lands_on_the_one_before_it() {
        let mut history = stored(&["a", "b", "c", "d"]);
        history.adopt("b");
        history.adopt("d");
        assert_eq!(history.older("").as_deref(), Some("d"));
        assert_eq!(history.position(), Some((4, 4)));
        assert_eq!(history.cycle().as_deref(), Some("d"));
        assert_eq!(history.position(), Some((2, 2)));
        assert_eq!(history.next_scope(), Some(Scope::All));
        assert_eq!(history.cycle().as_deref(), Some("d"));
        assert_eq!(history.position(), Some((4, 4)));
        history.older("");
        history.older("");
        history.older("");
        assert_eq!(history.cycle().as_deref(), Some("b"));
        assert_eq!(history.position(), Some((1, 2)));

        let mut history = stored(&["a", "b", "c"]);
        history.adopt("b");
        history.older("");
        assert_eq!(history.cycle().as_deref(), Some("b"));
        history.cycle();
        assert_eq!(history.older("").as_deref(), Some("a"));
        assert_eq!(history.cycle().as_deref(), Some("b"));
    }

    /// Ending the walk, and sending, leave the scope where it was put, so the next Up walks the
    /// same prompts.
    #[test]
    fn the_scope_stays_where_it_was_put_when_the_walk_ends() {
        let mut history = stored(&["a"]);
        history.push("b", None);
        history.older("");
        assert_eq!(history.cycle().as_deref(), Some("b"));
        history.newer();
        assert_eq!(history.scope(), Scope::Session);
        assert_eq!(history.older("").as_deref(), Some("b"));
        assert_eq!(history.older(""), None);
        history.newer();
        history.push("c", None);
        assert_eq!(history.scope(), Scope::Session);
        assert_eq!(history.older("").as_deref(), Some("c"));
        assert_eq!(history.older("").as_deref(), Some("b"));
        assert_eq!(history.older(""), None);
        history.newer();
        history.older("");
        history.cycle();
        assert_eq!(history.scope(), Scope::All);
    }

    /// Not the oldest of this session's: the newest one before the prompt on screen.
    #[test]
    fn narrowing_picks_the_nearest_earlier_prompt_not_the_oldest() {
        let mut history = stored(&["a", "b", "c", "d", "e", "f"]);
        for prompt in ["b", "d", "f"] {
            history.adopt(prompt);
        }
        history.older("");
        history.older("");
        assert_eq!(history.older("").as_deref(), Some("d"));
        assert_eq!(history.older("").as_deref(), Some("c"));
        assert_eq!(history.cycle().as_deref(), Some("b"));
        history.cycle();
        history.newer();
        assert_eq!(history.newer().as_deref(), Some("d"));
        history.newer();
        assert_eq!(history.cycle().as_deref(), Some("d"));
        assert_eq!(history.position(), Some((2, 3)));
    }

    const HERE: &str = "/work/here";

    /// Entries in a mix of workspaces, `b` and `e` from this process, running in [`HERE`].
    fn spread() -> History {
        let sent =
            |prompt: &str, project: Option<&str>| Entry::sent(prompt, project.map(str::to_string));
        let mut history = History::from_entries(vec![
            sent("a", Some(HERE)),
            sent("b", Some("/work/elsewhere")),
            sent("c", None),
            sent("d", Some(HERE)),
            sent("e", Some("/work/elsewhere")),
        ]);
        history.set_project(Some(HERE.to_string()));
        history.adopt("b");
        history.adopt("e");
        history
    }

    /// Everywhere, this project, this session, and back, the walk staying on the prompt on screen
    /// where it is in the new scope and otherwise on the newest one before it.
    #[test]
    fn the_scope_cycles_everywhere_project_session_and_back() {
        let mut history = spread();
        assert_eq!(history.older("").as_deref(), Some("e"));
        assert_eq!(history.older("").as_deref(), Some("d"));

        // `d` is this project's, so it stays on screen.
        assert_eq!(history.next_scope(), Some(Scope::Project));
        assert_eq!(history.cycle().as_deref(), Some("d"));
        assert_eq!(history.scope(), Scope::Project);
        assert_eq!(history.position(), Some((2, 2)));

        // `d` is not this session's, so the newest of this session's before it is `b`.
        assert_eq!(history.next_scope(), Some(Scope::Session));
        assert_eq!(history.cycle().as_deref(), Some("b"));
        assert_eq!(history.scope(), Scope::Session);
        assert_eq!(history.position(), Some((1, 2)));

        assert_eq!(history.next_scope(), Some(Scope::All));
        assert_eq!(history.cycle().as_deref(), Some("b"));
        assert_eq!(history.scope(), Scope::All);
        assert_eq!(history.position(), Some((2, 5)));
    }

    /// Where no earlier prompt of the new scope exists, the walk lands on the oldest of it.
    #[test]
    fn a_scope_switch_before_every_prompt_of_the_scope_lands_on_the_oldest() {
        let mut history = spread();
        for _ in 0..5 {
            history.older("");
        }
        assert_eq!(history.position(), Some((1, 5)));
        assert_eq!(history.cycle().as_deref(), Some("a"));
        assert_eq!(history.cycle().as_deref(), Some("b"));
        assert_eq!(history.position(), Some((1, 2)));
    }

    /// An entry stored with no workspace is in everywhere only, and a session with no workspace has
    /// no project scope at all: guessing would file a whole history under the next checkout opened.
    #[test]
    fn a_prompt_with_no_recorded_project_is_in_the_everywhere_scope_only() {
        let history = spread();
        assert!(history.contains(2, Scope::All));
        assert!(!history.contains(2, Scope::Project));
        assert!(!history.contains(2, Scope::Session));

        let mut unplaced = spread();
        unplaced.set_project(None);
        assert!(!unplaced.holds(Scope::Project));
    }

    /// A scope that holds nothing is not offered, and the cycle goes past it.
    #[test]
    fn a_scope_with_nothing_in_it_is_skipped() {
        // This session has sent nothing: everywhere, then this project, then back.
        let mut history = spread();
        history.forget_session();
        history.older("");
        assert_eq!(history.next_scope(), Some(Scope::Project));
        assert_eq!(history.cycle().as_deref(), Some("d"));
        assert_eq!(history.scope(), Scope::Project);
        assert_eq!(history.next_scope(), Some(Scope::All));

        // No stored prompt belongs to this workspace: everywhere, then this session.
        let mut history = spread();
        history.set_project(Some("/work/nowhere".to_string()));
        history.older("");
        assert_eq!(history.next_scope(), Some(Scope::Session));
        assert_eq!(history.cycle().as_deref(), Some("e"));
        assert_eq!(history.scope(), Scope::Session);
        assert_eq!(history.next_scope(), Some(Scope::All));
    }

    /// With one scope left the chord is not offered and does nothing.
    #[test]
    fn with_one_scope_left_the_chord_is_not_offered_and_does_nothing() {
        let mut history = stored(&["a", "b"]);
        history.older("");
        assert_eq!(history.next_scope(), None);
        assert_eq!(history.cycle(), None);
        assert_eq!(history.scope(), Scope::All);
        assert_eq!(history.position(), Some((2, 2)));
    }

    /// The scope holds across ending the walk, and `/clear` puts it back on everywhere.
    #[test]
    fn the_project_scope_holds_across_the_walk_and_clearing_puts_it_back() {
        let mut history = spread();
        history.older("");
        history.cycle();
        history.newer();
        history.newer();
        assert!(!history.is_browsing());
        assert_eq!(history.scope(), Scope::Project);
        assert_eq!(history.older("").as_deref(), Some("d"));

        history.forget_session();
        assert_eq!(history.scope(), Scope::All);
    }

    /// Withdrawing the last prompt of the project scope in force leaves nothing to walk there, so
    /// the walk goes back to everywhere.
    #[test]
    fn withdrawing_the_last_prompt_of_this_project_puts_the_scope_back_on_everywhere() {
        let mut history = stored(&["old"]);
        history.set_project(Some(HERE.to_string()));
        history.push("new", Some(HERE.to_string()));
        history.older("");
        assert_eq!(history.cycle().as_deref(), Some("new"));
        assert_eq!(history.scope(), Scope::Project);

        assert!(history.withdraw(history.ticket()).is_some());

        assert_eq!(history.scope(), Scope::All);
        assert_eq!(history.older("").as_deref(), Some("old"));
    }

    /// Moving to a workspace that has no stored prompt leaves nothing to walk on the project
    /// scope, so the walk goes back to everywhere rather than recalling nothing.
    #[test]
    fn moving_to_a_workspace_with_no_prompts_puts_the_scope_back_on_everywhere() {
        let mut history = stored(&["old"]);
        history.set_project(Some(HERE.to_string()));
        history.push("new", Some(HERE.to_string()));
        history.older("");
        assert_eq!(history.cycle().as_deref(), Some("new"));
        assert_eq!(history.scope(), Scope::Project);

        history.set_project(Some("/elsewhere".to_string()));

        assert_eq!(history.scope(), Scope::All);
        assert_eq!(history.older("").as_deref(), Some("old"));
    }
}
