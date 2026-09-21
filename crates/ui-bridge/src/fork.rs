//! Cutting a conversation in front of something the user said.
//!
//! A fork is a session that begins with part of another one. What decides "part" is a place in
//! the transcript, and a transcript is [`Conversation::recounted`] — a *projection* of the
//! stored messages, not the messages themselves. So the front-end names a place in the
//! projection, an ordinal over the prompts it drew, and this module turns that back into an
//! index into the messages.
//!
//! # Why the alignment is exact
//!
//! `recounted` drops a `Role::User` message only when its text starts with one of three markers
//! the agent writes itself: a tool result, a resume note, a compaction summary. Every one of
//! those decisions is a function of the role and the text and nothing else. A message whose text
//! *equals* the text of a prompt the transcript showed therefore cannot have been one of the
//! dropped ones — had it started with a marker, its own line would have been dropped too.
//!
//! So walking the messages and consuming the drawn prompts in order is not a guess and not a
//! second copy of upstream's filter rules. It matches on what upstream published rather than on
//! how upstream decided it, which is the same discipline the rest of this crate follows with
//! tags. If a future build filters on something the text does not carry, the walk runs out of
//! matches and this returns `None` — a refusal rather than a cut in the wrong place.
//!
//! Two upstream messages are worth knowing about here: a context file arrives as a user message
//! (`Contents of …`), and a turn that spends its tool budget is nudged with one. Neither is
//! filtered, so both are prompts as far as a transcript is concerned, and both shift the
//! ordinals of everything after them. A window that has drawn neither can therefore count
//! differently from the conversation — which is why the caller checks the prompt's *text*
//! against the ordinal before cutting anything.

use bravebot_agent::conversation::{Said, Snapshot};
use bravebot_aichat::protocol::Role;

/// A conversation cut in front of a prompt, and the prompt it was cut in front of.
pub struct Cut {
    /// Everything said before it, ready for `Conversation::restored`.
    pub before: Snapshot,
    /// What the user had asked there, for the composer to open with.
    pub prompt: String,
}

/// The prompts a transcript drew, in the order it drew them.
pub fn prompts(said: &[Said]) -> Vec<&str> {
    said.iter()
        .filter_map(|entry| match entry {
            Said::User(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Cut in front of the `ordinal`-th thing the user said.
///
/// `None` when there is no such prompt, or when the walk cannot find the message it came from.
/// Both are refusals rather than approximations: a fork taken a few messages away from where
/// somebody pointed is worse than one that did not happen.
pub fn cut(snapshot: &Snapshot, said: &[Said], ordinal: usize) -> Option<Cut> {
    let prompts = prompts(said);
    let wanted = (*prompts.get(ordinal)?).to_string();

    // The archive first, because that is the order `recounted` walks and the order the
    // transcript is drawn in: what compaction took out of the request is still the session.
    let mut seen = 0usize;
    let mut cut_at = None;
    for (index, message) in snapshot
        .archive
        .iter()
        .chain(snapshot.messages.iter())
        .enumerate()
    {
        if message.role != Role::User {
            continue;
        }
        // Anything that does not match the prompt we are expecting next is one of the user-role
        // messages the transcript never showed. Skipped rather than counted.
        if message.content.text() != *prompts[seen] {
            continue;
        }
        if seen == ordinal {
            cut_at = Some(index);
            break;
        }
        seen += 1;
    }
    let cut_at = cut_at?;

    // A cut inside the archive takes the whole of the child's history out of it: there is no
    // request left for a compaction summary to stand in for, so what compaction removed goes
    // back in and the archive empties. A cut after it keeps both, and the summary that stands in
    // for the archive is still at the head of the messages where it was.
    let archived = snapshot.archive.len();
    let (messages, archive) = if cut_at <= archived {
        (snapshot.archive[..cut_at].to_vec(), Vec::new())
    } else {
        (
            snapshot.messages[..cut_at - archived].to_vec(),
            snapshot.archive.clone(),
        )
    };

    Some(Cut {
        before: Snapshot {
            messages,
            // Carried verbatim, and never raised. Integrity is met over a session's whole life
            // and no message records its own, so it cannot be recomputed for a prefix — and the
            // only direction it may be wrong in is downwards.
            context: snapshot.context.clone(),
            // Carried too, though the child may name fewer slots than the counter has handed
            // out. It exists so a name is never handed out twice, and lowering it would let the
            // child mint a name a message it kept already used.
            references: snapshot.references,
            archive,
            // Dropped, for the reason upstream drops it after compacting: the figure described a
            // conversation that no longer exists, and a child that inherited it would open by
            // trying to compact a history it has not sent yet.
            measured: 0,
        },
        prompt: wanted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::Conversation;
    use bravebot_agent::conversation::{
        COMPACTED_PREFIX, RESUMED_PREFIX, TOOL_RESULT_PREFIX,
    };
    use bravebot_aichat::protocol::{Message, ToolCallRequest, ToolCallRequestFunction};

    fn snapshot(messages: Vec<Message>) -> Snapshot {
        Snapshot {
            messages,
            context: "trusted".into(),
            references: 3,
            archive: Vec::new(),
            measured: 4096,
        }
    }

    /// What a transcript would have drawn for a snapshot, which is what a front-end counts.
    fn drawn(snapshot: &Snapshot) -> Vec<Said> {
        Conversation::restored(snapshot.clone()).recounted()
    }

    fn call(id: &str) -> ToolCallRequest {
        ToolCallRequest {
            id: id.into(),
            kind: "function".into(),
            function: ToolCallRequestFunction {
                name: "read_file".into(),
                arguments: "{}".into(),
            },
        }
    }

    fn texts(messages: &[Message]) -> Vec<String> {
        messages.iter().map(|m| m.content.text()).collect()
    }

    #[test]
    fn cutting_in_front_of_a_prompt_keeps_everything_before_it() {
        let before = snapshot(vec![
            Message::user("first"),
            Message::assistant("one"),
            Message::user("second"),
            Message::assistant("two"),
        ]);
        let cut = cut(&before, &drawn(&before), 1).expect("a second prompt");

        assert_eq!(cut.prompt, "second");
        assert_eq!(texts(&cut.before.messages), vec!["first", "one"]);
    }

    #[test]
    fn a_cut_never_separates_a_call_from_its_result() {
        let before = snapshot(vec![
            Message::user("first"),
            Message::assistant_calling("looking", vec![call("c1")]),
            Message::tool_result("c1", "the contents"),
            Message::assistant("one"),
            Message::user("second"),
        ]);
        let cut = cut(&before, &drawn(&before), 1).expect("a second prompt");

        let request = Conversation::restored(cut.before).with_system("system");
        assert!(
            !request.iter().any(|m| m.content.text().contains("did not run")),
            "a cut in front of a prompt left nothing for the filler to answer",
        );
    }

    #[test]
    fn a_call_the_cut_left_unanswered_is_answered_before_it_is_sent() {
        // The converse, and the reason no pairing logic is written here: even a history that
        // ends mid-round assembles into a well-formed request, because the agent fills the gap.
        let before = snapshot(vec![
            Message::user("first"),
            Message::assistant_calling("looking", vec![call("c1")]),
        ]);
        let request = Conversation::restored(before).with_system("system");
        assert!(request.iter().any(|m| m.content.text().contains("did not run")));
    }

    #[test]
    fn a_prompt_the_transcript_never_showed_is_not_a_fork_point() {
        let before = snapshot(vec![
            Message::user("first"),
            Message::user(format!("{TOOL_RESULT_PREFIX}read_file: contents")),
            Message::user(format!("{RESUMED_PREFIX} references are dead")),
            Message::assistant("one"),
            Message::user("second"),
        ]);
        let said = drawn(&before);
        assert_eq!(prompts(&said), vec!["first", "second"]);

        let cut = cut(&before, &said, 1).expect("a second prompt");
        assert_eq!(cut.prompt, "second");
        // Everything before it, the agent's own messages included: they were never the user's
        // to cut at, but they are still what the model was working from.
        assert_eq!(cut.before.messages.len(), 4);
    }

    #[test]
    fn a_fork_inside_the_archive_puts_what_compaction_took_back_into_the_request() {
        let mut before = snapshot(vec![
            Message::user(format!("{COMPACTED_PREFIX}\n\nearlier, in short")),
            Message::user("third"),
        ]);
        before.archive = vec![
            Message::user("first"),
            Message::assistant("one"),
            Message::user("second"),
            Message::assistant("two"),
        ];
        let said = drawn(&before);
        assert_eq!(prompts(&said), vec!["first", "second", "third"]);

        let cut = cut(&before, &said, 1).expect("the archived second prompt");
        assert_eq!(cut.prompt, "second");
        assert_eq!(texts(&cut.before.messages), vec!["first", "one"]);
        assert!(cut.before.archive.is_empty(), "nothing is left standing in for");
    }

    #[test]
    fn the_summary_standing_in_for_the_archive_survives_a_later_cut() {
        let mut before = snapshot(vec![
            Message::user(format!("{COMPACTED_PREFIX}\n\nearlier, in short")),
            Message::user("third"),
            Message::assistant("three"),
            Message::user("fourth"),
        ]);
        before.archive = vec![Message::user("first"), Message::assistant("one")];
        let said = drawn(&before);

        let cut = cut(&before, &said, 2).expect("the fourth prompt");
        assert_eq!(cut.prompt, "fourth");
        assert_eq!(cut.before.archive.len(), 2, "the archive is untouched");
        assert!(
            cut.before.messages[0].content.text().starts_with(COMPACTED_PREFIX),
            "the summary is still at the head of the request",
        );
        assert_eq!(cut.before.messages.len(), 3);
    }

    #[test]
    fn the_measurement_does_not_survive_a_cut() {
        let before = snapshot(vec![Message::user("first"), Message::user("second")]);
        let cut = cut(&before, &drawn(&before), 1).expect("a second prompt");

        assert_eq!(cut.before.measured, 0);
        assert_eq!(cut.before.references, 3, "but the names handed out are remembered");
    }

    #[test]
    fn an_untrusted_conversation_forks_untrusted() {
        let mut before = snapshot(vec![Message::user("first"), Message::user("second")]);
        before.context = "untrusted".into();
        let cut = cut(&before, &drawn(&before), 1).expect("a second prompt");

        assert_eq!(cut.before.context, "untrusted");
    }

    #[test]
    fn an_ordinal_past_the_last_prompt_is_no_cut_at_all() {
        let before = snapshot(vec![Message::user("first")]);
        assert!(cut(&before, &drawn(&before), 1).is_none());
        assert!(cut(&snapshot(Vec::new()), &[], 0).is_none());
    }

    #[test]
    fn two_identical_prompts_fork_at_the_one_that_was_asked_for() {
        let before = snapshot(vec![
            Message::user("again"),
            Message::assistant("one"),
            Message::user("again"),
            Message::assistant("two"),
        ]);
        let said = drawn(&before);

        assert_eq!(cut(&before, &said, 0).unwrap().before.messages.len(), 0);
        assert_eq!(cut(&before, &said, 1).unwrap().before.messages.len(), 2);
    }
}
