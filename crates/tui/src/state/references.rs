//! The issues and pull requests the session has met, which `/issues` and `/prs` print and the info
//! panel lists (PANEL-16).
//!
//! The list holds only values [`Reference::new`] built, from three sources: the lines the person
//! typed, which [`Session::remember`] reads, and the planner's own tool-call arguments and the
//! results labelled trusted, which the turn reads in `bravebot-agent` and hands over one call at a
//! time. Nothing here reads a reply or a tool result, and no code here branches on what a
//! reference says beyond the kind it names.

#[cfg_attr(
    not(test),
    expect(
        clippy::wildcard_imports,
        reason = "a child of state.rs, which holds Session and the types every sibling shares"
    )
)]
use super::*;
use bravebot_agent::links::{self, LinkKind, Reference};

/// How many references a session keeps. The first met are kept and later ones are dropped, so the
/// list does not grow with a log that names every number it has seen (PANEL-16).
pub(crate) const PER_SESSION: usize = 500;

impl Session {
    /// Every issue and pull request the session has met, in the order first met.
    pub fn references(&self) -> &[Reference] {
        &self.references
    }

    /// Add what a turn found to the list, keeping the order first met (PANEL-16).
    ///
    /// Called with what the turn reported, which may name something the list already holds: the
    /// next turn starts its own list and so reports an item again. An item already held keeps its
    /// place and its spelling, and a later sighting that names its kind settles a guessed one.
    pub fn add_references(&mut self, found: &[Reference]) {
        links::merge(&mut self.references, found, PER_SESSION);
    }

    /// Read the references out of a line the person sent, whether a prompt, a command or a line
    /// that waits for the turn.
    ///
    /// `line` is the person's own text with a folded paste put back to words. Where it came from
    /// is the keyboard, so there is no label to read.
    pub(crate) fn note_typed(&mut self, line: &str) {
        let found = links::references_in_text(line);
        self.add_references(&found);
    }

    /// Take the list a resumed record held. Called once, where the record is read (SESSION-3).
    pub fn restore_references(&mut self, held: &[Reference]) {
        self.references.clear();
        self.add_references(held);
    }

    /// Forget the list, as `/clear` begins a session that has met nothing.
    pub(super) fn forget_references(&mut self) {
        self.references.clear();
    }

    /// The references of `kind` in the order the panel and `/issues` and `/prs` draw them: the
    /// session's own link of that kind first, where it is a GitHub address of that kind, and then
    /// each reference the session met that is not that item, in the order first met.
    ///
    /// One function for both so the command and the panel cannot disagree.
    pub fn listed_references(&self, kind: LinkKind) -> Vec<Reference> {
        let own = self.own_reference(kind);
        let met = self.references.iter().filter(|reference| {
            reference.kind() == kind && !own.as_ref().is_some_and(|own| own.same_item(reference))
        });
        own.iter().chain(met).cloned().collect()
    }

    /// The item the session's own link of `kind` names, where that link is a GitHub address whose
    /// path names that kind. A link of any other shape names no reference (PANEL-12).
    pub(crate) fn own_reference(&self, kind: LinkKind) -> Option<Reference> {
        let own_link = match kind {
            LinkKind::Issue => self.identity.issue.as_deref(),
            LinkKind::PullRequest => self.identity.pull_request.as_deref(),
        };
        own_link
            .and_then(|link| links::references_in_text(link).into_iter().next())
            .filter(|reference| reference.kind() == kind)
    }

    /// Print the issues (`/issues`) or the pull requests (`/prs`) the session has met, or say there
    /// are none.
    ///
    /// Costs no token and starts no turn: it reads the list the session holds and nothing else, so
    /// it answers while a turn runs (CMD-31).
    pub fn report_references(&mut self, kind: LinkKind) {
        let listed = self.listed_references(kind);
        if listed.is_empty() {
            self.note(match kind {
                LinkKind::Issue => t!(references_none_issues),
                LinkKind::PullRequest => t!(references_none_prs),
            });
            return;
        }
        self.note(match kind {
            LinkKind::Issue => t!(references_heading_issues, count = listed.len()),
            LinkKind::PullRequest => t!(references_heading_prs, count = listed.len()),
        });
        for reference in listed {
            self.note(format!(
                "{}  {}",
                reference.shorthand(),
                reference.address()
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(number: u64) -> Reference {
        Reference::new("brave", "bravebot", number, LinkKind::Issue).expect("valid")
    }

    fn pull(number: u64) -> Reference {
        Reference::new("brave", "bravebot", number, LinkKind::PullRequest).expect("valid")
    }

    fn shorthands(references: &[Reference]) -> Vec<String> {
        references.iter().map(Reference::shorthand).collect()
    }

    fn type_line(session: &mut Session, line: &str) {
        for c in line.chars() {
            session.type_char(c);
        }
    }

    /// What the transcript says after the entries it held before, one string each.
    fn said_since(session: &Session, before: usize) -> Vec<String> {
        session.transcript[before..]
            .iter()
            .map(|entry| entry.text.clone())
            .collect()
    }

    /// PANEL-16. A prompt, a line that waits for the turn, a command and a shell line each reach the
    /// list, because each of them leaves the box through `remember`, and a bare number names no
    /// repository so it reads as none.
    ///
    /// The fault it rejects: reading in `submit` alone, which leaves a queued line, a command and a
    /// shell line unread.
    #[test]
    fn every_line_the_box_sends_is_read_for_what_it_names() {
        let mut session = Session::new("none");

        type_line(&mut session, "look at brave/bravebot#10 and fix #99");
        session.submit().expect("sent");
        assert_eq!(shorthands(session.references()), ["brave/bravebot#10"]);

        session.status = Status::Working;
        type_line(
            &mut session,
            "and https://github.com/brave/bravebot/pull/11",
        );
        assert!(session.queue());

        session.status = Status::Idle;
        type_line(&mut session, "/issue https://github.com/o/r/issues/12");
        let commanded = session.take_command();
        assert_eq!(commanded.line, "/issue https://github.com/o/r/issues/12");

        session.shell = true;
        type_line(&mut session, "echo brave/bravebot#13");
        session.submit_command().expect("run");

        assert_eq!(
            shorthands(session.references()),
            [
                "brave/bravebot#10",
                "brave/bravebot#11",
                "o/r#12",
                "brave/bravebot#13"
            ]
        );
        assert_eq!(session.references()[1].kind(), LinkKind::PullRequest);
        assert!(session.references()[1].kind_named());
        assert!(!session.references()[0].kind_named());
    }

    /// PANEL-16. An item met again is the same item, in the place it first had, and a later address
    /// that names its kind settles the kind the first reading guessed. A line sent a second time
    /// adds nothing.
    ///
    /// The faults it rejects: a list that appends each sighting, one that compares the text and not
    /// the item, and one that keeps the guess.
    #[test]
    fn an_item_met_again_keeps_its_place_and_a_named_kind_settles_a_guess() {
        let mut session = Session::new("none");
        for line in [
            "see Brave/Bravebot#10",
            "and brave/bravebot#20",
            "see Brave/Bravebot#10",
            "the address is https://github.com/brave/bravebot/pull/10/files",
            "the address is https://github.com/brave/bravebot/pull/10/files",
        ] {
            type_line(&mut session, line);
            session.submit().expect("sent");
            session.status = Status::Idle;
        }

        assert_eq!(
            shorthands(session.references()),
            ["Brave/Bravebot#10", "brave/bravebot#20"],
            "the first spelling and the first place are kept"
        );
        assert_eq!(session.references()[0].kind(), LinkKind::PullRequest);
        assert!(session.references()[0].kind_named());
        assert_eq!(session.references()[1].kind(), LinkKind::Issue);
    }

    /// PANEL-16. The session keeps the first 500 it met, in order, and an item already held is not
    /// dropped when the list is full.
    ///
    /// The faults it rejects: no cap, a cap that keeps the last 500, and a cap that refuses a
    /// sighting that would only settle a kind.
    #[test]
    fn the_session_keeps_the_first_five_hundred_it_met_in_order() {
        let mut session = Session::new("none");
        let first: Vec<Reference> = (1..=300).map(issue).collect();
        let second: Vec<Reference> = (301..=600).map(issue).collect();
        session.add_references(&first);
        session.add_references(&second);
        assert_eq!(session.references().len(), 500);
        assert_eq!(session.references()[0].number(), 1);
        assert_eq!(session.references()[499].number(), 500);

        session.add_references(&[pull(3)]);
        assert_eq!(session.references().len(), 500);
        assert_eq!(session.references()[2].kind(), LinkKind::Issue);
        assert!(
            session.references()[2].kind_named(),
            "a named kind was replaced"
        );
    }

    /// PANEL-16. `/issues` and `/prs` list the references of their own kind, the session's own link
    /// of that kind first and not twice, and a link that is not a GitHub address of that kind is not
    /// a reference.
    ///
    /// The faults it rejects: a list that mixes the kinds, one that draws the own link again where
    /// it was also met, one that puts it after the others, and one that reads a link from another
    /// host as an item.
    #[test]
    fn the_list_of_a_kind_starts_with_the_own_link_and_leaves_out_what_is_not_one() {
        let mut session = Session::new("none");
        session.add_references(&[issue(5), pull(6), issue(7), pull(8)]);
        session.link(
            Some("https://github.com/brave/bravebot/issues/7"),
            Some("https://example.test/brave/bravebot/pull/6"),
        );

        assert_eq!(
            shorthands(&session.listed_references(LinkKind::Issue)),
            ["brave/bravebot#7", "brave/bravebot#5"]
        );
        assert_eq!(
            shorthands(&session.listed_references(LinkKind::PullRequest)),
            ["brave/bravebot#6", "brave/bravebot#8"],
            "a link on another host changed the list"
        );
    }

    /// CMD-31. `/issues` and `/prs` say how many, then each item on a line of its own as the
    /// shorthand and the address, in the order first met, and say there are none when there are
    /// none.
    ///
    /// The faults it rejects: lines in another order, a line with the shorthand alone, a list of
    /// both kinds under either word, and a silence for an empty list.
    #[test]
    fn the_commands_print_each_item_with_its_address_or_say_there_are_none() {
        let mut session = Session::new("none");
        let before = session.transcript.len();
        session.report_references(LinkKind::Issue);
        session.report_references(LinkKind::PullRequest);
        assert_eq!(
            said_since(&session, before),
            [
                t!(references_none_issues).to_string(),
                t!(references_none_prs).to_string()
            ]
        );

        session.add_references(&[issue(30), pull(2), issue(4)]);
        let before = session.transcript.len();
        session.report_references(LinkKind::Issue);
        assert_eq!(
            said_since(&session, before),
            [
                t!(references_heading_issues, count = 2).to_string(),
                "brave/bravebot#30  https://github.com/brave/bravebot/issues/30".to_string(),
                "brave/bravebot#4  https://github.com/brave/bravebot/issues/4".to_string(),
            ]
        );
        let before = session.transcript.len();
        session.report_references(LinkKind::PullRequest);
        assert_eq!(
            said_since(&session, before),
            [
                t!(references_heading_prs, count = 1).to_string(),
                "brave/bravebot#2  https://github.com/brave/bravebot/pull/2".to_string(),
            ]
        );
    }

    /// SESSION-3. A cleared conversation has met nothing, and what a record held is the list a
    /// resume starts with, taken in order and without repeating an item.
    ///
    /// The faults it rejects: a `clear` that leaves the list, and a restore that appends to what
    /// the session held or repeats an item the record held twice.
    #[test]
    fn a_clear_forgets_the_list_and_a_restore_replaces_it() {
        let mut session = Session::new("none");
        session.add_references(&[issue(1), issue(2)]);
        session.clear();
        assert!(session.references().is_empty());

        session.add_references(&[issue(9)]);
        session.restore_references(&[pull(3), issue(1), pull(3)]);
        assert_eq!(
            shorthands(session.references()),
            ["brave/bravebot#3", "brave/bravebot#1"]
        );
    }
}
