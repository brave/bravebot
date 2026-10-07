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

/// Which prompts Up walks.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The prompts this session sent, and the ones its record holds when it was resumed.
    #[default]
    Session,
    /// Every stored prompt, from every session and workspace.
    All,
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
            scope: Scope::Session,
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

    /// The entries Up walks now, as indexes into `entries`, oldest first.
    fn visible(&self) -> Vec<usize> {
        match self.scope {
            Scope::All => (0..self.entries.len()).collect(),
            Scope::Session => (0..self.entries.len())
                .filter(|at| self.mine[*at])
                .collect(),
        }
    }

    /// Whether Up has anything to bring back: a sent prompt in the scope or a kept draft.
    pub fn can_recall(&self) -> bool {
        self.draft.is_some() || self.has_visible()
    }

    fn has_visible(&self) -> bool {
        match self.scope {
            Scope::All => !self.entries.is_empty(),
            Scope::Session => self.mine.iter().any(|mine| *mine),
        }
    }

    /// Which prompts Up is walking.
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Whether Up has nothing to bring back only because this session has sent nothing, while
    /// earlier sessions' prompts are stored.
    pub fn only_earlier_sessions(&self) -> bool {
        self.draft.is_none() && !self.has_visible() && !self.entries.is_empty()
    }

    /// Whether Ctrl-Right means "every stored prompt" now. `line_is_empty` is whether the box is
    /// empty, since the key is the caret's word motion in a line being typed.
    ///
    /// While a stored prompt is on screen, or where Up found nothing of this session's and the box
    /// holds nothing to move around in.
    pub fn can_widen(&self, line_is_empty: bool) -> bool {
        self.scope == Scope::Session
            && (self.on_a_sent_prompt()
                || (self.back.is_none() && line_is_empty && self.only_earlier_sessions()))
    }

    /// Whether Ctrl-Left means "this session's prompts" now: a stored prompt is on screen in the
    /// wide scope and this session has some.
    pub fn can_narrow(&self) -> bool {
        self.scope == Scope::All && self.on_a_sent_prompt() && self.mine.iter().any(|mine| *mine)
    }

    /// Walk every stored prompt, staying on the prompt on screen. Returns it.
    pub fn widen(&mut self) -> Option<String> {
        let list = self.visible();
        self.scope = Scope::All;
        let back = self.back.filter(|back| *back > 0)?;
        let at = list[list.len() - back];
        self.back = Some(self.entries.len() - at);
        Some(self.entries[at].prompt.clone())
    }

    /// Walk only this session's prompts again, and return the one now on screen: the one that was
    /// there where it is this session's, otherwise the newest of this session's before it, and
    /// failing that the oldest of this session's.
    pub fn narrow(&mut self) -> Option<String> {
        let back = self.back.filter(|back| *back > 0)?;
        let at = self.entries.len() - back;
        self.scope = Scope::Session;
        let list = self.visible();
        let place = list.iter().rposition(|each| *each <= at).unwrap_or(0);
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

    /// Cancel one submission without removing other submissions that share its words. Reports
    /// whether an entry left the list.
    pub(crate) fn withdraw(&mut self, ticket: Ticket) -> bool {
        self.leave();
        let Some(at) = self.claims.iter().position(|(id, _)| *id == ticket) else {
            return false;
        };
        self.claims[at].1 -= 1;
        if self.claims[at].1 == 0 {
            self.claims.remove(at);
            self.mine.remove(at);
            self.entries.remove(at);
            return true;
        }
        false
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
                self.scope = Scope::Session;
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
        self.scope = Scope::Session;
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

    /// Stored prompts belong to an earlier session, so Up has nothing until one is sent.
    #[test]
    fn stored_prompts_are_not_this_sessions() {
        let mut history = stored(&["old"]);
        assert!(!history.can_recall());
        assert!(history.only_earlier_sessions());
        assert_eq!(history.older(""), None);
        history.push("new", None);
        assert_eq!(history.older("").as_deref(), Some("new"));
        assert_eq!(history.older(""), None);
        assert_eq!(history.position(), Some((1, 1)));
    }

    /// A prompt a resumed record holds is counted, and clearing the conversation uncounts it.
    #[test]
    fn a_resumed_prompt_is_this_sessions_until_the_conversation_is_cleared() {
        let mut history = stored(&["a", "b", "c"]);
        history.adopt("b");
        assert_eq!(history.older("").as_deref(), Some("b"));
        history.forget_session();
        assert!(history.only_earlier_sessions());
    }

    /// Switching keeps the prompt, and where the prompt is not this session's it lands on the
    /// nearest of this session's before it.
    #[test]
    fn narrowing_from_a_prompt_of_another_session_lands_on_the_one_before_it() {
        let mut history = stored(&["a", "b", "c", "d"]);
        history.adopt("b");
        history.adopt("d");
        history.older("");
        assert!(history.can_widen(false));
        assert_eq!(history.widen().as_deref(), Some("d"));
        assert_eq!(history.position(), Some((4, 4)));
        history.older("");
        assert_eq!(history.older("").as_deref(), Some("b"));
        history.older("");
        assert_eq!(history.narrow().as_deref(), Some("b"));
        assert_eq!(history.position(), Some((1, 2)));

        let mut history = stored(&["a", "b", "c"]);
        history.adopt("b");
        history.older("");
        history.widen();
        assert_eq!(history.older("").as_deref(), Some("a"));
        assert_eq!(history.narrow().as_deref(), Some("b"));
    }

    /// Leaving the walk puts the scope back, so the next Up is this session's again.
    #[test]
    fn the_scope_goes_back_to_the_session_when_the_walk_ends() {
        let mut history = stored(&["a"]);
        history.push("b", None);
        history.older("");
        history.widen();
        assert_eq!(history.scope(), Scope::All);
        history.newer();
        assert_eq!(history.scope(), Scope::Session);
    }

    /// Not the oldest of this session's: the newest one before the prompt on screen.
    #[test]
    fn narrowing_picks_the_nearest_earlier_prompt_not_the_oldest() {
        let mut history = stored(&["a", "b", "c", "d", "e", "f"]);
        for prompt in ["b", "d", "f"] {
            history.adopt(prompt);
        }
        history.older("");
        history.widen();
        history.older("");
        assert_eq!(history.older("").as_deref(), Some("d"));
        assert_eq!(history.older("").as_deref(), Some("c"));
        assert_eq!(history.narrow().as_deref(), Some("b"));
        history.widen();
        history.newer();
        assert_eq!(history.newer().as_deref(), Some("d"));
        history.newer();
        assert_eq!(history.narrow().as_deref(), Some("d"));
        assert_eq!(history.position(), Some((2, 3)));
    }
}
