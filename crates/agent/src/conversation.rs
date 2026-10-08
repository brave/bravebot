//! What a session remembers between turns.
//!
//! A turn used to begin with nothing but the prompt it was given, so a session was a row of
//! strangers: the model could not be asked to try again, or to carry on, because it had never
//! heard of what came before. This is the record that fixes that.
//!
//! It changes nothing about the rule the repository rests on. Every string in here has already
//! been past [`bravebot_core::policy::Policy::present`]: either the kernel judged it trusted and
//! showed it to the planner, or what went in is a reference and the content stayed in
//! quarantine. Carrying the record forward therefore carries no untrusted bytes forward, because
//! there were never any in it.
//!
//! Three things travel with the messages:
//!
//! - the **quarantine**, so a reference the planner was given in an earlier turn still names
//!   something. A slot store that died with its turn would leave the conversation full of names
//!   for content that no longer exists.
//! - the **reference counter**, so two turns cannot both hand out `ref:0` and leave the planner
//!   with one name for two things.
//! - the **integrity** the conversation has met, so a later turn cannot label output better than
//!   an earlier turn would have. See [`bravebot_core::policy::Policy::resuming`]. With it goes
//!   whether the planner was shown private content, for the same reason on the other axis
//!   ([`bravebot_core::policy::Policy::holding`]).

use crate::request_view::Provenance;
use bravebot_aichat::protocol::{Message, Role};
#[cfg(test)]
use bravebot_aichat::protocol::{ToolCallRequest, ToolCallRequestFunction};
use bravebot_core::label::{Confidentiality, Integrity, Label};
use bravebot_core::slot::{SlotId, SlotStore};
use serde::{Deserialize, Serialize};

/// Why a user message was composed rather than typed, for the surfaces that draw one.
///
/// A prompt is what a person typed. These are not. Two of them the agent writes into the
/// conversation itself, to put a file somebody named in front of the planner, or to say that a
/// watch fired while no turn was running. The third a front end sends on its own account, to ask
/// a bot to bring its memory up to date after a compaction has taken the detail behind it. A
/// transcript draws each of them as something other than a prompt, and the only thing that can
/// say which one it is is whatever composed it.
///
/// The alternative is reading the prose back, which asks the words inside a message what the
/// message is. The words inside a context file are the file's, so that hands whoever wrote the
/// file the choice of which row it is drawn as, including the rows the interface draws about
/// itself. Recorded here, the choice stays with the composer.
///
/// Not sent. This rides beside the [`Message`] rather than in it, because the messages are the
/// request body: a field here would be a field the backend was asked to accept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Composed {
    /// A file read into the conversation because somebody named it.
    Attached {
        /// The path as it was named, which is what the interface has room to draw.
        path: String,
    },
    /// A watch that fired between turns, as the prompt of the turn it starts.
    Watch {
        /// Which of the session's watches it was.
        number: usize,
        /// The path it was armed on.
        path: String,
    },
    /// A turn a front end sent to bring a bot's memory up to date, rather than a person typing it.
    ///
    /// The one tag a front end may write, and it names what the turn is for rather than merely
    /// saying somebody other than a person composed it. A surface drawing a row of its own for
    /// this is asserting that the conversation was asked to consolidate, and a tag that said only
    /// "not typed" would leave that row standing over any prompt any client composed for any
    /// reason. The agent's own two tags stay the agent's: see `wire::composed` in
    /// `bravebot-ui-bridge` for the word a request may carry and the ones it may not.
    Consolidation,
    /// A picture or a PDF `vet_content` let through, attached for the planner after the round's
    /// results (VET-4).
    ///
    /// The fields are the driver's: the reference is a counter's name and the media type comes
    /// from its table of extensions. Neither is anything the file said, so a surface may draw a
    /// row of its own from them.
    Vetted {
        /// The reference the file was promoted out of.
        reference: String,
        /// Its media type.
        media: String,
    },
    /// A tool result sent as prose rather than in the API's own shape, which is the fallback for
    /// a round whose calls carried no ids or whose own account of itself was quarantined.
    ///
    /// House-keeping for the planner: a transcript draws no row for it, and compaction never cuts
    /// between it and the round above it.
    ToolResult,
    /// The note a resume adds to say the references the planner was handed no longer name
    /// anything.
    ///
    /// House-keeping for the planner. Not written to a snapshot, since every resume adds its own.
    Resumed,
    /// The summary that stands in for the messages a compaction took out of the request.
    ///
    /// House-keeping for the planner: the archive already holds what it stands in for, so a
    /// transcript drawing it would show the session twice.
    Summary,
    /// Words another program kept, copied into the archive by `sessions import` (SESSION-32).
    ///
    /// Said by somebody, so a transcript draws it as the prompt or the answer it was. The tag is
    /// for the one thing that must not happen to it: it is never moved into the request, since
    /// nothing in a copy was written by this session's own planner or checked by its driver.
    Imported,
}

impl Composed {
    /// Whether this is a message written for the planner that no transcript draws.
    ///
    /// Decided from the tag, so a prompt a person typed is never taken for one by what it says.
    fn is_housekeeping(&self) -> bool {
        matches!(self, Self::ToolResult | Self::Resumed | Self::Summary)
    }
}

/// One message as the record holds it: what was sent, and why the agent wrote it.
///
/// A pair rather than two lists kept in step. Everything that walks the exchange would have to
/// index both, and [`Conversation::snapshot`] leaves a message out as it writes, so a second list
/// is one filter away from describing the wrong message for the rest of the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stored {
    /// What was sent.
    ///
    /// Flattened, so a record written before any of this existed reads as a message with nothing
    /// composed about it, which is what it is.
    #[serde(flatten)]
    pub message: Message,
    /// Absent for a prompt, an answer and a result, which is nearly all of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composed: Option<Composed>,
    /// Whose words these are, as the composer recorded it, for the view of a request.
    ///
    /// Not written down: a saved session is a record of the exchange and not of this process's
    /// reading of it, so a message restored from one has no origin until a turn composes a new
    /// one. [`Stored::provenance`] says so rather than guessing.
    #[serde(skip)]
    pub source: Option<Provenance>,
}

impl Stored {
    /// A message nobody composed: a prompt, an answer, or a result.
    pub fn plain(message: Message) -> Self {
        Self {
            message,
            composed: None,
            source: None,
        }
    }

    /// Whose words this message holds.
    ///
    /// What the composer recorded where it recorded anything. Failing that, only what the shape of
    /// the record settles: the tag of a message that holds a file somebody named, a summary the
    /// driver put in, and the planner's own role. A user message with nothing recorded is not taken
    /// for a typed one, since being wrong that way would tell a person that words they never wrote
    /// were theirs.
    pub fn provenance(&self) -> Provenance {
        if let Some(source) = &self.source {
            return source.clone();
        }
        match (&self.composed, self.message.role) {
            (Some(Composed::Attached { path }), _) => Provenance::TrustedFile(path.clone()),
            (Some(Composed::Summary), _) => Provenance::Summary,
            (Some(Composed::Vetted { reference, .. }), _) => Provenance::Vetted(reference.clone()),
            (Some(Composed::Watch { .. } | Composed::Resumed | Composed::Consolidation), _) => {
                Provenance::Driver
            }
            (_, Role::Assistant) => Provenance::Planner,
            _ => Provenance::Unrecorded,
        }
    }
}

/// The record a session carries from one turn to the next.
///
/// Not `Clone`: the quarantine holds the only copy of content nobody may read, and a second
/// store would be a second place for a reference to resolve differently.
#[derive(Debug)]
pub struct Conversation {
    /// The exchange so far, oldest first, without the system prompt.
    ///
    /// The system prompt is left out deliberately: it belongs to the build rather than to the
    /// conversation, so a session that outlives an upgrade should use the new one.
    messages: Vec<Stored>,
    /// Content the kernel would not show the planner, by the name it was given.
    quarantine: SlotStore,
    /// How many references the session has handed out.
    ///
    /// Trusted metadata: a counter, never derived from content.
    references: usize,
    /// What everything this conversation has been shown amounts to.
    context: Integrity,
    /// Whether the planner was shown private content in it. Only ever rises.
    holds: Confidentiality,
    /// What compaction took out of the request, oldest first.
    ///
    /// What compaction shortens is the request, not the record. These messages are no longer
    /// sent, and a summary of them is sent instead, but the person whose session this is still
    /// owns every word of it: [`Conversation::recounted`] reads them back. A transcript with a
    /// hole in it where the user's own earlier prompts were is not a saving worth making.
    archive: Vec<Stored>,
    /// What the last request built from this conversation came to, as the server counted it.
    ///
    /// Kept here rather than in the turn because a session is many turns and the conversation is
    /// the only thing that outlives one of them. A figure that started again at zero each turn
    /// would only ever notice a conversation growing inside a single long turn, and a session of
    /// fifty short ones would fill the context with nothing watching.
    ///
    /// Trusted metadata: a count the server reported, never anything anyone wrote.
    measured: u64,
    /// Whether the planner has asked for a write since a turn last ended with an answer.
    ///
    /// Kept here rather than in the turn because a stopped turn is usually continued rather than
    /// replaced: the next prompt is `continue`, and the change the stopped turn wrote is still
    /// the task's change. An answer is where one task usually ends, so that is where it clears.
    ///
    /// Trusted metadata: from the names of the functions the planner called, never their
    /// arguments or anything a tool returned.
    asked_to_write: bool,
    /// The windows of files the planner has been shown, and the call that showed each.
    ///
    /// Held here for the reason `quarantine` is: a session is many turns and this is the only thing
    /// that outlives one. Not part of a [`Snapshot`], because a resumed conversation owes the
    /// planner nothing about a read it cannot be sure the planner still holds, and showing the lines
    /// again is the safe direction to be wrong in.
    ///
    /// Trusted metadata: a path the planner named, the window it asked for and a change token taken
    /// from the file's size and modification time, never anything a file said.
    reads: ShownReads,
}

/// A window of a file as `read_file` showed it, for telling a repeat of the same read from a new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadWindow {
    /// The workspace key of the file, so two spellings of one path are one file.
    pub file: String,
    /// The first line shown, 1-based.
    pub offset: usize,
    /// The most lines asked for, as the read applied it.
    pub limit: usize,
    /// The file's change token when the window was read.
    pub token: String,
}

/// What `read_file` has shown the planner in this conversation.
///
/// An entry stands for lines that are in a message the planner still has. It names that message by
/// its position in the conversation, not by the id of the call it answered: providers number ids
/// per response, so two rounds can carry one. Compaction drops every entry whose message it removed
/// (COMPACT-6): the lines are then in a summary at best, and a repeat of the read has to send them.
#[derive(Debug, Default)]
pub struct ShownReads {
    shown: Vec<(usize, ReadWindow)>,
}

impl ShownReads {
    /// The change token this window was last shown under, if the planner still has those lines.
    pub fn token_of(&self, file: &str, offset: usize, limit: usize) -> Option<&str> {
        self.shown
            .iter()
            .find(|(_, w)| w.file == file && w.offset == offset && w.limit == limit)
            .map(|(_, w)| w.token.as_str())
    }

    /// Record that the message at `position` put this window in front of the planner, replacing
    /// what was recorded for the same window.
    pub fn record(&mut self, position: usize, window: ReadWindow) {
        self.shown.retain(|(_, w)| {
            !(w.file == window.file && w.offset == window.offset && w.limit == window.limit)
        });
        self.shown.push((position, window));
    }

    /// Forget the messages before `boundary`, and move the rest to where compaction puts them: the
    /// summary takes the first position, so each kept message is one later than its place after
    /// the drain.
    fn compacted(&mut self, boundary: usize) {
        self.shown.retain(|(position, _)| *position >= boundary);
        for (position, _) in &mut self.shown {
            *position = *position - boundary + 1;
        }
    }
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new()
    }
}

impl Conversation {
    /// An empty conversation, which is what a session starts with.
    ///
    /// Trusted, since nothing has been read into it yet.
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            quarantine: SlotStore::new(),
            references: 0,
            context: Integrity::Trusted,
            holds: Confidentiality::Public,
            archive: Vec::new(),
            measured: 0,
            asked_to_write: false,
            reads: ShownReads::default(),
        }
    }

    /// Whether anything has been said yet.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// How many messages the next request would carry.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// The whole exchange with the system prompt in front, as one request's messages.
    ///
    /// Any call left unanswered is answered here, saying it did not run. A turn can end between
    /// a call and its result, by cancellation or by failure, and a round that announced a call
    /// nothing ever answered is a malformed request: the next turn would be refused by the
    /// server rather than merely missing something. Filling the gap keeps the record of what
    /// was attempted, which is the reason the conversation survives a failed turn at all.
    pub fn with_system(&self, system: &str) -> Vec<Message> {
        Self::assembled(system, &self.messages)
    }

    /// The same, and whose words each message after the system prompt holds.
    ///
    /// One list from one pass, so a mark cannot land on a message other than the one it was made
    /// for: a call answered here has the driver's sentence as its answer.
    pub fn with_system_marked(&self, system: &str) -> (Vec<Message>, Vec<Provenance>) {
        let marked = Self::assembled_marked(system, &self.messages);
        let marks = marked
            .iter()
            .skip(1)
            .map(|(_, mark)| mark.clone())
            .collect();
        (
            marked.into_iter().map(|(message, _)| message).collect(),
            marks,
        )
    }

    /// The same, over some prefix of the exchange.
    ///
    /// Shared with [`Conversation::with_system`] rather than written twice, because the gap
    /// filling below is the difference between a well-formed request and one a server refuses,
    /// and a summariser sending a prefix needs it exactly as much as a turn sending the whole.
    fn assembled(system: &str, exchange: &[Stored]) -> Vec<Message> {
        Self::assembled_marked(system, exchange)
            .into_iter()
            .map(|(message, _)| message)
            .collect()
    }

    /// [`Conversation::assembled`] with each message's origin beside it.
    fn assembled_marked(system: &str, exchange: &[Stored]) -> Vec<(Message, Provenance)> {
        let mut messages = Vec::with_capacity(exchange.len() + 1);
        messages.push((Message::system(system), Provenance::Driver));

        for (index, stored) in exchange.iter().enumerate() {
            let message = &stored.message;
            messages.push((message.clone(), stored.provenance()));

            let Some(calls) = &message.tool_calls else {
                continue;
            };
            for call in calls {
                if !answered(exchange, index, &call.id) {
                    messages.push((
                        Message::tool_result(
                            call.id.clone(),
                            "(this call did not run: the turn ended first)",
                        ),
                        Provenance::Driver,
                    ));
                }
            }
        }

        messages
    }

    /// What the conversation has met, for the turn resuming it.
    pub fn context(&self) -> Integrity {
        self.context
    }

    /// What the conversation's planner was shown, for the turn resuming it.
    pub fn holds(&self) -> Confidentiality {
        self.holds
    }

    /// Add a message the kernel has already ruled the planner may hold.
    pub fn push(&mut self, message: Message) {
        self.messages.push(Stored::plain(message));
    }

    /// The same, for a message the agent composed rather than a person.
    ///
    /// Separate from [`Conversation::push`] so the tag is written where the prose is written, by
    /// the one caller that knows what it is composing. A tag applied anywhere else would be a
    /// second reading of the same message.
    pub fn push_composed(&mut self, message: Message, composed: Composed) {
        self.messages.push(Stored {
            message,
            composed: Some(composed),
            source: None,
        });
    }

    /// [`Conversation::push_composed`] for a message whose composer also knows whose words it holds.
    pub fn push_composed_from(&mut self, message: Message, composed: Composed, source: Provenance) {
        self.messages.push(Stored {
            message,
            composed: Some(composed),
            source: Some(source),
        });
    }

    /// The same as [`Conversation::push`], for a message whose composer knows whose words it holds.
    ///
    /// Written where the prose is written, by the one caller that knows, for the reason the tag of
    /// [`Conversation::push_composed`] is. It is shown to a person who wants to see where each span
    /// of a request came from, so it is never worked out from the words.
    pub fn push_from(&mut self, message: Message, source: Provenance) {
        self.messages.push(Stored {
            message,
            composed: None,
            source: Some(source),
        });
    }

    /// Take the next reference name.
    ///
    /// Unique across the session rather than across the turn, which is the difference between a
    /// name and a coincidence.
    pub fn next_reference(&mut self) -> SlotId {
        let slot = SlotId::new(format!("ref:{}", self.references));
        self.references += 1;
        slot
    }

    /// The quarantine, for the kernel to write into and read back out of.
    pub fn quarantine(&mut self) -> &mut SlotStore {
        &mut self.quarantine
    }

    /// The quarantine and the record of what was shown, together, because a tool call borrows both
    /// and a method for each would borrow the conversation twice.
    pub fn for_a_tool(&mut self) -> (&mut SlotStore, &mut ShownReads) {
        (&mut self.quarantine, &mut self.reads)
    }

    /// Record that the message just pushed put a window of a file in front of the planner.
    pub fn shown_read(&mut self, window: ReadWindow) {
        if let Some(last) = self.messages.len().checked_sub(1) {
            self.reads.record(last, window);
        }
    }

    /// Record what the last request built from this conversation came to.
    ///
    /// The server's own figure. There is no tokeniser here, so this is the only measurement of a
    /// conversation's size that exists, and it is one round out of date by construction: it says
    /// what the last request came to, not what the next one will.
    pub fn measured(&mut self, prompt_tokens: u64) {
        self.measured = prompt_tokens;
    }

    /// What the last request came to, or zero where none has been sent yet.
    ///
    /// Zero reads as "not measured", which is the same thing as "not yet too large" for every
    /// purpose here.
    pub fn last_request_tokens(&self) -> u64 {
        self.measured
    }

    /// Record that the planner has asked for a write.
    pub fn write_requested(&mut self) {
        self.asked_to_write = true;
    }

    /// Record that a turn ended with an answer, so the next prompt starts from nothing written.
    pub fn turn_answered(&mut self) {
        self.asked_to_write = false;
    }

    /// Whether a write has been asked for since a turn last ended with an answer.
    pub fn asked_to_write(&self) -> bool {
        self.asked_to_write
    }

    /// Where compaction would cut, or `None` when there is nothing worth summarising.
    ///
    /// Whole exchanges are given up first, since the boundary between two of them is the one a
    /// person would draw. A turn that has gone long on its own has no earlier exchange to give:
    /// its history is one prompt and a great many rounds of tool calls, so the exchange count
    /// never grows and that rule alone would decline exactly when the context is filling
    /// fastest. Where it does, earlier **rounds** are given up instead.
    ///
    /// Both cut where a round is not in progress. That is not a preference:
    /// [`Conversation::with_system`] answers a call nothing answered, and it looks only at the
    /// run of results immediately after the call, so a cut between a call and its results leaves
    /// the head claiming the call never ran and the tail holding an answer to a call that is not
    /// there. Every boundary here is found from roles and call ids, which are the shape of the
    /// exchange rather than anything anyone wrote.
    ///
    /// `None` where the head would be nothing but an earlier summary. There is nothing left to
    /// give up in that case, and saying so is better than spending a request to learn it.
    pub fn compaction_boundary(&self) -> Option<usize> {
        self.by_exchange().or_else(|| self.by_round())
    }

    /// The start of the last [`RECENT_EXCHANGES_KEPT`] exchanges.
    ///
    /// A summary is lossy, and what a session is in the middle of is the part that can least
    /// afford to be paraphrased.
    fn by_exchange(&self) -> Option<usize> {
        self.cut_keeping(&self.boundaries(opens_an_exchange), RECENT_EXCHANGES_KEPT)
    }

    /// The start of the last [`RECENT_ROUNDS_KEPT`] rounds.
    ///
    /// The fallback, for the turn that is long by itself. It may cut away the prompt the turn
    /// began with, which is the point: that prompt is one of the things the summary is required
    /// to carry, and a turn on its fortieth round has more history behind it than the sentence
    /// that started it.
    ///
    /// A round that answered by id is found by the field alone: its results carry one and its
    /// own start does not. A round sent as prose carries no ids anywhere, so the field alone
    /// would take each of its results for the start of a round of its own, which both puts the
    /// cut inside a round and makes [`RECENT_ROUNDS_KEPT`] count results rather than rounds.
    fn by_round(&self) -> Option<usize> {
        self.cut_keeping(
            &self.boundaries(|stored| {
                stored.message.tool_call_id.is_none()
                    && !matches!(stored.composed, Some(Composed::ToolResult))
            }),
            RECENT_ROUNDS_KEPT,
        )
    }

    /// The cut that keeps the last `kept` of these boundaries, if it is worth making.
    ///
    /// **Worth making is the whole of this.** Summarising costs a model call, so a cut that gives
    /// up one round in order to keep twelve has spent a round to save almost nothing, and the
    /// next round is in exactly the same position: a conversation that cannot get under the
    /// budget would then summarise itself once per round for the rest of the turn, doubling the
    /// requests and shortening nothing. That is not hypothetical. A request has a floor it cannot
    /// go below, the system prompt and the tool schemas, and a budget under that floor is
    /// unreachable however much history is given up.
    ///
    /// So at least as much has to be given up as is kept. That pays for the call, and it makes
    /// the next compaction wait until that much has built up again, which is the hysteresis that
    /// stops the loop.
    ///
    /// An earlier summary does not count towards what is given up. It is already the compressed
    /// form of something, so re-summarising it buys nothing and loses a little more of it each
    /// time.
    fn cut_keeping(&self, points: &[usize], kept: usize) -> Option<usize> {
        let head = points.len().checked_sub(kept)?;
        let cut = *points.get(head)?;

        let given_up = points[..head]
            .iter()
            .filter(|&&index| !matches!(self.messages[index].composed, Some(Composed::Summary)))
            .count();
        (given_up >= kept).then_some(cut)
    }

    /// The indices a cut may fall on, by whatever rule is asking.
    fn boundaries(&self, is_one: impl Fn(&Stored) -> bool) -> Vec<usize> {
        self.messages
            .iter()
            .enumerate()
            .filter(|(_, stored)| is_one(stored))
            .map(|(index, _)| index)
            .collect()
    }

    /// The part compaction would replace, as one request's messages.
    ///
    /// The system prompt is the summariser's, so it is passed in the same way a turn passes its
    /// own: the conversation stores neither.
    pub fn to_summarise(&self, boundary: usize, system: &str) -> Vec<Message> {
        Self::assembled(system, &self.messages[..boundary])
    }

    /// Put a summary in place of everything before `boundary`.
    ///
    /// Three things it deliberately leaves alone. The **quarantine**, because it holds the only
    /// copy of the content the surviving references name, and a reference in a retained message
    /// has to still resolve. The **reference counter**, because slots are written once and a name
    /// handed out twice is a collision. The **integrity**, because compaction is not a fresh
    /// session and nothing here has un-read what the conversation read.
    ///
    /// The replaced messages go to the archive rather than into a bin. See the field.
    pub fn compacted(&mut self, boundary: usize, summary: &str) {
        let replaced: Vec<Stored> = self.messages.drain(..boundary).collect();
        self.archive.extend(replaced);
        self.reads.compacted(boundary);

        let mut note = format!("{COMPACTED_PREFIX}\n\n{}", summary.trim());
        if let Some(live) = self.live_references() {
            note.push_str("\n\n");
            note.push_str(&live);
        }
        self.messages.insert(
            0,
            Stored {
                message: Message::user(note),
                composed: Some(Composed::Summary),
                source: None,
            },
        );

        // The figure described a conversation that no longer exists, and nothing has measured
        // this one. Left alone it would say the context is still full: the gauge would show a
        // percentage for an exchange that has been shortened underneath it, and the next turn
        // would open by trying to compact again on the strength of it.
        self.measured = 0;
    }

    /// What to tell the planner about the references it may still use.
    ///
    /// The names are still in the summary and in the retained messages, and nothing in either
    /// says whether they survived, so the planner would find out by being refused. Written from
    /// the counter and the slot store's inventory of ids: no origin, and no byte of anything
    /// quarantined. An origin out of a quarantined listing is content, and this line is going
    /// into the planner's context.
    fn live_references(&self) -> Option<String> {
        let names: Vec<String> = (0..self.references)
            .map(|n| SlotId::new(format!("ref:{n}")))
            .filter(|slot| self.quarantine.label_of(slot).is_some())
            .map(|slot| slot.to_string())
            .collect();

        (!names.is_empty()).then(|| {
            format!(
                "The conversation above was summarised, but nothing was thrown away behind it: \
                 {} still name what they named.",
                names.join(", ")
            )
        })
    }

    /// Record what the turn's context has met.
    ///
    /// Recorded as it happens rather than once the turn is over, because a turn that fails
    /// partway still read what it read, and the next turn has to inherit that.
    ///
    /// One way on both axes: [`Integrity::meet`] cannot raise integrity and
    /// [`Confidentiality::join`] cannot lower confidentiality, so nothing recorded here restores
    /// integrity the conversation has lost or makes a planner that was shown private content
    /// public again.
    pub fn observed(&mut self, context: Label) {
        self.context = self.context.meet(context.integrity);
        self.holds = self.holds.join(context.confidentiality);
    }
}

/// One thing said, for an interface showing a conversation it did not watch happen.
///
/// The exchange a person would recognise, and what the turn did between the two. A tool
/// *result* is left out: it was written for the planner, and a transcript of a resumed session
/// is for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// What the user asked.
    User(String),
    /// What the model answered, where the kernel let it be seen at all.
    Assistant(String),
    /// A call the turn made, in the words it was announced with while it ran.
    ///
    /// What came of it is not here. The record does not say, and inventing an outcome for a call
    /// whose result nobody wrote down would be worse than admitting the line is all there is.
    Tool {
        /// The call, as the line announcing it read.
        line: String,
        /// Why the planner made it, in its own words. Empty where it gave no reason.
        why: String,
    },
    /// A message the agent wrote into the conversation, what it wrote it for, and what it said.
    ///
    /// The tag is what a surface decides from. The text is what a surface draws when it has no row
    /// of its own for that tag, which is a plain message and the same thing an unknown tag gets:
    /// the words are still the conversation, and leaving a message out of a transcript silently is
    /// worse than drawing it plainly. What a surface must never do is read the words to decide
    /// which it is, since for a file those words are the file's own.
    Composed {
        /// What the agent composed it for.
        why: Composed,
        /// The message as the planner was sent it.
        text: String,
    },
}

/// A conversation written down, for a session that outlives the process.
///
/// Two of the four things a conversation carries, and deliberately so.
///
/// The **messages** are safe to write anywhere: every one of them has already been past the
/// present gate, so a stored conversation holds no untrusted bytes. The **integrity** goes with
/// them because it is what a resumed turn must inherit, and dropping it would let a resumed
/// session call trusted what the original would not have. Whether the planner was shown private
/// content goes with them for the same reason: dropping it would let a resumed session call
/// public what the original would have called private.
///
/// The **quarantine** is not stored. Untrusted content would then be sitting in a file, to be
/// read back and relabelled from what that file says, and a label that survives a round trip
/// through an editable file is not a label. A resumed conversation therefore holds references
/// that no longer name anything, which is the honest failure: a name with nothing behind it,
/// rather than bytes with a label nobody checked. [`Conversation::restored`] says so rather than
/// leaving the planner to find out by being refused.
///
/// The **reference counter** goes with them so a resumed session cannot hand out a name an
/// earlier message already used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// The exchange, oldest first, without the system prompt.
    pub messages: Vec<Stored>,
    /// What the conversation had met: `trusted`, or anything else.
    ///
    /// A word rather than a flag, so a person reading the file can see what it says, and an
    /// unreadable value means untrusted rather than a parse error. Everything unrecognised
    /// degrades in the safe direction, which is the only direction this may degrade in.
    pub context: String,
    /// How many references had been handed out.
    #[serde(default)]
    pub references: usize,
    /// What compaction took out of the request, oldest first.
    ///
    /// Written down because it is the person's own transcript and losing it on a resume would
    /// make compaction cost them their history after all. Defaulted, so a session file written
    /// before compaction existed still reads.
    #[serde(default)]
    pub archive: Vec<Stored>,
    /// What the last request built from this conversation came to.
    ///
    /// Stored so a resumed session knows it is already large. Without it the first turn after a
    /// resume sends the whole conversation again to find out what it already knew.
    #[serde(default)]
    pub measured: u64,
    /// Whether a write had been asked for since a turn last ended with an answer.
    ///
    /// Stored so a stopped turn resumed in a later process is continued the way it would have
    /// been in this one. Defaulted to false, which costs a record written without it at most one
    /// needless line to the planner.
    #[serde(default)]
    pub asked_to_write: bool,
    /// Whether the planner had been shown private content: `public`, or anything else.
    ///
    /// A word for the reason `context` is one, and read the same way: anything but `public` is
    /// private. A file written before this was recorded has no word and reads as private, since
    /// nothing says what that session's planner was shown.
    #[serde(default = "unrecorded")]
    pub holds: String,
}

/// The word for an integrity, as it is written down.
const TRUSTED: &str = "trusted";
const UNTRUSTED: &str = "untrusted";

/// The word for what a planner was shown, as it is written down.
const PUBLIC: &str = "public";
const PRIVATE: &str = "private";

fn unrecorded() -> String {
    PRIVATE.to_string()
}

impl Conversation {
    /// The conversation as it can be written down.
    ///
    /// The note [`Conversation::restored`] adds is left out. It is something a resume produces
    /// rather than part of the exchange, and writing it down would stack another copy on every
    /// resume while the one already there named a shorter list than the session had by then.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            messages: self
                .messages
                .iter()
                .filter(|stored| !matches!(stored.composed, Some(Composed::Resumed)))
                .cloned()
                .collect(),
            context: match self.context {
                Integrity::Trusted => TRUSTED.to_string(),
                Integrity::Untrusted => UNTRUSTED.to_string(),
            },
            references: self.references,
            archive: self.archive.clone(),
            measured: self.measured,
            asked_to_write: self.asked_to_write,
            holds: match self.holds {
                Confidentiality::Public => PUBLIC.to_string(),
                Confidentiality::Private => PRIVATE.to_string(),
            },
        }
    }

    /// A conversation read back from one.
    ///
    /// The quarantine starts empty, since a snapshot has none: see [`Snapshot`]. Anything but
    /// the word for trusted is read as untrusted, so a truncated, hand-edited or
    /// newer-than-this-build file resumes with less trust rather than more.
    ///
    /// A conversation that had been handed references is told they are dead. The names are still
    /// in the transcript in front of the planner, and nothing in it says they stopped naming
    /// anything, so without this the first thing a resumed session does with quarantined content
    /// is spend a call finding out. Saying so is cheap and needs no content: the note is written
    /// here, from a counter the kernel kept, and holds no byte of what was quarantined.
    pub fn restored(snapshot: Snapshot) -> Self {
        let mut messages = snapshot.messages;
        if let Some(note) = dead_references(snapshot.references) {
            messages.push(Stored {
                message: Message::user(note),
                composed: Some(Composed::Resumed),
                source: None,
            });
        }

        Self {
            messages,
            quarantine: SlotStore::new(),
            references: snapshot.references,
            context: if snapshot.context == TRUSTED {
                Integrity::Trusted
            } else {
                Integrity::Untrusted
            },
            holds: if snapshot.holds == PUBLIC {
                Confidentiality::Public
            } else {
                Confidentiality::Private
            },
            archive: snapshot.archive,
            measured: snapshot.measured,
            asked_to_write: snapshot.asked_to_write,
            reads: ShownReads::default(),
        }
    }

    /// The exchange, for an interface that wants to show what was said.
    pub fn messages(&self) -> &[Stored] {
        &self.messages
    }

    /// The exchange as a person would read it back.
    ///
    /// Prompts and answers. A round's own account of itself is left out along with the results,
    /// since between them they are the working, and a transcript being shown to whoever resumed
    /// the session is not the place for it.
    ///
    /// A message the agent composed rather than a person is reported as the tag it was recorded
    /// with, so no surface has to ask a message's own words what the message is. See [`Composed`].
    ///
    /// A result sent in the API's own shape is a message of its own and is simply skipped. One
    /// sent as prose, which is what an untrusted context falls back to, is recognised by the
    /// prefix this crate put on it: examining it decides only how a line is drawn, and the text
    /// being examined has already been past the present gate.
    ///
    /// The **calls** are reported as well, since what a turn did is most of what a person wants
    /// back. Only that it happened and what it was about: a call's result is not recounted, so
    /// resuming a session does not put a file's contents on the screen where the live session
    /// showed a one-line summary.
    pub fn recounted(&self) -> Vec<Said> {
        let mut said = Vec::new();
        // The archive first: what compaction took out of the request is still the person's
        // session, and they are the one reading this.
        for stored in self.archive.iter().chain(self.messages.iter()) {
            if let Some(why) = stored
                .composed
                .as_ref()
                .filter(|why| **why != Composed::Imported)
            {
                // Written for the planner and said by no one: a prose tool result, the resume
                // note, and the summary standing in for what the archive above already holds.
                // Drawn as a prompt any of them would look like something the user typed.
                if why.is_housekeeping() {
                    continue;
                }
                said.push(Said::Composed {
                    why: why.clone(),
                    text: stored.message.content.text(),
                });
                continue;
            }
            let message = &stored.message;
            match message.role {
                // `text` rather than the whole content: an attachment is drawn from what the
                // interface recorded about it, and a data URI in the scrollback is not a transcript.
                Role::User => said.push(Said::User(message.content.text())),
                Role::Assistant => {
                    // What the model said on its way to a call, which the live transcript shows
                    // above the call it introduces.
                    let spoken = message.content.text();
                    if !spoken.trim().is_empty() {
                        said.push(Said::Assistant(spoken));
                    }
                    for call in message.tool_calls.iter().flatten() {
                        said.push(Said::Tool {
                            line: crate::tools::describe_stored_call(
                                &call.function.name,
                                &call.function.arguments,
                            ),
                            why: crate::tools::stored_why(&call.function.arguments),
                        });
                    }
                }
                Role::System | Role::Tool => {}
            }
        }
        said
    }
}

/// Whether the round beginning at `index` answered the call with this id.
///
/// Only the run of results immediately after it counts, since that is where the answers to a
/// round belong and where a server looks for them.
fn answered(exchange: &[Stored], index: usize, id: &str) -> bool {
    exchange[index + 1..]
        .iter()
        .take_while(|stored| stored.message.tool_call_id.is_some())
        .any(|stored| stored.message.tool_call_id.as_deref() == Some(id))
}

/// How many of the most recent exchanges compaction leaves word for word.
///
/// Two rather than one, because the exchange a session is in the middle of usually refers to the
/// one before it: "do the same to the other file" is unanswerable from a paraphrase of what the
/// other file was.
const RECENT_EXCHANGES_KEPT: usize = 2;

/// How many of the most recent rounds compaction leaves word for word, where it is giving up
/// rounds rather than exchanges.
///
/// More than the exchange count, and for the opposite reason: a round is a much smaller thing
/// than an exchange, and what a turn is doing on its thirtieth round is usually the work of the
/// last several rather than of the last one.
const RECENT_ROUNDS_KEPT: usize = 6;

/// Whether a message begins an exchange, so compaction may cut in front of it.
///
/// A prompt the user typed, or a summary standing in for the ones before it. A tool result sent
/// as prose is not one, whatever it looks like: it belongs to the round above it, and cutting
/// between the two would separate a call from its answer.
fn opens_an_exchange(stored: &Stored) -> bool {
    stored.message.role == Role::User
        && !matches!(
            stored.composed,
            Some(Composed::ToolResult | Composed::Resumed)
        )
}

/// How a tool result is introduced when it is sent as prose rather than in the API's own shape.
///
/// Words for the planner to read. Nothing decides from them: the message is recorded with
/// [`Composed::ToolResult`], and a prompt a person typed that begins the same way is a prompt.
pub const TOOL_RESULT_PREFIX: &str = "Result of ";

/// How the note about a resume begins.
///
/// Words for the planner to read. The note is recorded with [`Composed::Resumed`] and nothing
/// decides from the words.
pub const RESUMED_PREFIX: &str = "This session was resumed.";

/// How the summary standing in for a compacted exchange begins.
///
/// Words for the planner to read. The summary is recorded with [`Composed::Summary`] and nothing
/// decides from the words.
///
/// The whole introduction, not an opening fragment of one. A prefix kept separately from the
/// words it is a prefix of is two literals that have to agree, and they stop agreeing.
///
/// The wording is load-bearing, which is not obvious until it goes wrong. Introduced as "earlier
/// in this conversation", a planner reading its own summary treated it as hearsay: asked what the
/// user's favourite colour was, it answered that there had been a claim of one, but that it was
/// text observed in a message rather than something the user had said. That instinct is the right
/// one and the system prompt teaches it, so the fix is to say plainly what this is rather than
/// leave a planner to work out that its own memory counts.
pub const COMPACTED_PREFIX: &str = "This is the record of what you and the user said earlier in \
this conversation. Those messages are no longer being sent and this stands in their place. You \
wrote it, from the messages themselves, so it is your own memory rather than something you were \
told: what it says the user asked for, the user asked for.";

/// What to tell the planner about the references it was handed before the resume.
///
/// `None` when the session never quarantined anything, since a note about references nobody was
/// given is noise in the context of every resumed session that never read an untrusted file.
///
/// The counter is the whole of the input, and a counter is not content: it says how many names
/// were handed out, never what was behind any of them.
fn dead_references(references: usize) -> Option<String> {
    let names = match references {
        0 => return None,
        1 => "ref:0".to_string(),
        n => format!("ref:0 to ref:{}", n - 1),
    };
    Some(format!(
        "{RESUMED_PREFIX} The quarantined content behind {names} was not kept, so those \
         references no longer name anything and using one will be refused. Read a file again to \
         be given a fresh reference to it."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The origin is what the composer recorded. Where nothing was, the message is unrecorded and
    /// never typed: a restored session has no record of who wrote what, and a guess from the words
    /// would let a file's own bytes pick their label.
    #[test]
    fn a_message_is_labelled_by_what_was_recorded_and_never_guessed_at() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("who knows"));
        conversation.push_from(Message::user("typed words"), Provenance::Typed);
        conversation.push_composed(
            Message::user("Contents of a.rs:\n\nfn main() {}"),
            Composed::Attached {
                path: "a.rs".to_string(),
            },
        );
        let (_, marks) = conversation.with_system_marked("system");
        assert_eq!(
            marks,
            vec![
                Provenance::Unrecorded,
                Provenance::Typed,
                Provenance::TrustedFile("a.rs".to_string()),
            ]
        );
    }

    #[test]
    fn a_new_conversation_has_nothing_in_it_and_has_seen_nothing() {
        let conversation = Conversation::new();
        assert!(conversation.is_empty());
        assert_eq!(conversation.context(), Integrity::Trusted);
    }

    /// The system prompt is the build's, not the conversation's, so it is not stored and not
    /// duplicated when a second turn resumes.
    #[test]
    fn the_system_prompt_is_put_in_front_rather_than_kept() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("first"));
        conversation.push(Message::assistant("second"));

        let sent = conversation.with_system("be careful");
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0].content.text(), "be careful");
        assert_eq!(sent[1].content.text(), "first");
        assert_eq!(conversation.len(), 2);

        // And again, with the same result rather than two system prompts.
        assert_eq!(conversation.with_system("be careful").len(), 3);
    }

    /// Two turns handing out the same name would leave the planner with one word for two things,
    /// and the second turn's slot would be refused as a repeated write.
    #[test]
    fn reference_names_are_unique_across_the_session() {
        let mut conversation = Conversation::new();
        let first = conversation.next_reference();
        let second = conversation.next_reference();
        assert_ne!(first, second);
    }

    fn a_call(name: &str, arguments: &str) -> ToolCallRequest {
        ToolCallRequest {
            id: "call-1".to_string(),
            kind: "function".to_string(),
            function: ToolCallRequestFunction {
                name: name.to_string(),
                arguments: arguments.to_string(),
            },
            extra_content: None,
        }
    }

    fn said_tool(line: &str, why: &str) -> Said {
        Said::Tool {
            line: line.to_string(),
            why: why.to_string(),
        }
    }

    /// A turn can be cancelled between a call and its result. The round still belongs in the
    /// record, but a call nothing answers makes the whole next request malformed, so the gap is
    /// filled rather than left for a server to refuse.
    #[test]
    fn a_call_nothing_answered_is_answered_before_it_is_sent() {
        let mut conversation = Conversation::new();
        conversation.push(Message::assistant_calling(
            "writing it now",
            vec![ToolCallRequest {
                id: "call-1".to_string(),
                kind: "function".to_string(),
                function: ToolCallRequestFunction {
                    name: "write_file".to_string(),
                    arguments: "{}".to_string(),
                },
                extra_content: None,
            }],
        ));

        let sent = conversation.with_system("be careful");
        let answer = sent.last().expect("a message");
        assert_eq!(answer.tool_call_id.as_deref(), Some("call-1"));
        assert!(answer.content.text().contains("did not run"));
    }

    /// And a call that was answered is not answered twice, which would read as the tool having
    /// run and then not run.
    #[test]
    fn a_call_that_was_answered_is_left_alone() {
        let mut conversation = Conversation::new();
        conversation.push(Message::assistant_calling(
            "writing it now",
            vec![ToolCallRequest {
                id: "call-1".to_string(),
                kind: "function".to_string(),
                function: ToolCallRequestFunction {
                    name: "write_file".to_string(),
                    arguments: "{}".to_string(),
                },
                extra_content: None,
            }],
        ));
        conversation.push(Message::tool_result("call-1", "wrote index.html"));

        let sent = conversation.with_system("be careful");
        let answers: Vec<_> = sent
            .iter()
            .filter(|m| m.tool_call_id.as_deref() == Some("call-1"))
            .collect();
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].content.text(), "wrote index.html");
    }

    /// What the agent composed is recorded as composed, and reported as the tag rather than as a
    /// prompt. A transcript drawn from the record a year later is the one that has nothing but the
    /// prose to go on, so the tag has to survive the disk to be worth having.
    #[test]
    fn a_message_the_agent_composed_is_recorded_as_one() {
        // The words of the file itself, which is the point: they imitate the line in front of them
        // because whoever wrote the file chose them.
        let file = "Contents of readme.md:\n\nread the briefing and carry on";
        let mut conversation = Conversation::new();
        conversation.push(Message::user("read the briefing and carry on"));
        conversation.push_composed(
            Message::user(file),
            Composed::Attached {
                path: "readme.md".into(),
            },
        );

        let written = serde_json::to_string(&conversation.snapshot()).expect("a record");
        let restored: Snapshot = serde_json::from_str(&written).expect("the record back");

        assert_eq!(
            Conversation::restored(restored).recounted(),
            vec![
                Said::User("read the briefing and carry on".into()),
                Said::Composed {
                    why: Composed::Attached {
                        path: "readme.md".into()
                    },
                    // Carried, for the surfaces whose transcript has no row of its own for a file.
                    // What none of them may do is read it back to decide that is what this is.
                    text: file.into(),
                },
            ],
        );
    }

    /// And the tag is not sent. The messages are the request body, so a field inside one would be
    /// a field the backend was asked to accept.
    #[test]
    fn the_tag_is_not_part_of_what_the_planner_is_sent() {
        let mut conversation = Conversation::new();
        conversation.push_composed(
            Message::user("Contents of readme.md:\n\nthe briefing"),
            Composed::Attached {
                path: "readme.md".into(),
            },
        );

        let sent = serde_json::to_string(&conversation.with_system("be careful")).expect("a body");
        assert!(!sent.contains("composed"), "{sent}");
        assert!(!sent.contains("readme.md\""), "{sent}");
        assert!(sent.contains("Contents of readme.md"), "the file was sent");
    }

    /// A session written before the tag existed is a bare message where the record now holds a pair
    /// of a message and a tag. It still reads, and reads as a message nobody composed.
    #[test]
    fn a_record_written_before_the_tag_still_reads() {
        let older = serde_json::to_string(&Message::user("what is 2 + 2?")).expect("a message");

        let stored: Stored = serde_json::from_str(&older).expect("the message as a record entry");
        assert!(stored.composed.is_none());
        assert_eq!(stored.message.content.text(), "what is 2 + 2?");
    }

    /// A session that outlives the process has to come back as what it was, integrity included.
    #[test]
    fn a_conversation_survives_being_written_down() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("what is 2 + 2?"));
        conversation.push(Message::assistant("four"));
        let _ = conversation.next_reference();

        let restored = Conversation::restored(conversation.snapshot());
        assert_eq!(
            restored.messages()[0].message.content.text(),
            "what is 2 + 2?"
        );
        assert_eq!(restored.messages()[1].message.content.text(), "four");
        assert_eq!(restored.context(), Integrity::Trusted);
        // The counter continues rather than starting over, or a resumed session would hand out
        // a name an earlier message already used.
        assert_eq!(
            Conversation::restored(conversation.snapshot()).next_reference(),
            conversation.next_reference()
        );
    }

    /// The quarantine does not survive, so every reference the planner holds is a name for
    /// nothing. Nothing in the transcript says so, which is what left a resumed session to find
    /// out by asking for content that no longer exists.
    #[test]
    fn a_resumed_conversation_is_told_its_references_are_dead() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("summarise notes.md"));
        let _ = conversation.next_reference();
        let _ = conversation.next_reference();
        let _ = conversation.next_reference();

        let restored = Conversation::restored(conversation.snapshot());
        let note = restored
            .messages()
            .last()
            .expect("a note")
            .message
            .content
            .text();
        assert!(note.contains("ref:0"), "{note}");
        assert!(note.contains("ref:2"), "{note}");
        assert!(
            !note.contains("ref:3"),
            "a name that was never handed out: {note}"
        );
    }

    /// One reference is named rather than described as a range, since "ref:0 to ref:0" is a way
    /// of writing one name that invites a reader to look for two.
    #[test]
    fn a_single_dead_reference_is_named_on_its_own() {
        let mut conversation = Conversation::new();
        let _ = conversation.next_reference();

        let restored = Conversation::restored(conversation.snapshot());
        let note = restored
            .messages()
            .last()
            .expect("a note")
            .message
            .content
            .text();
        assert!(note.contains("ref:0"), "{note}");
        assert_eq!(
            note.matches("ref:").count(),
            1,
            "one reference was described as a range: {note}"
        );
    }

    /// A session that never quarantined anything gets no note. It would be in the context of
    /// every resumed session, saying nothing about anything.
    #[test]
    fn a_conversation_that_was_handed_no_references_is_told_nothing() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("what is 2 + 2?"));
        conversation.push(Message::assistant("four"));

        let restored = Conversation::restored(conversation.snapshot());
        assert_eq!(restored.len(), 2, "a note appeared with nothing to say");
    }

    /// What a turn did is most of what a person resumes a session to see. Prose alone left a
    /// transcript that said the model answered and never said it had read anything.
    #[test]
    fn a_recounted_turn_says_what_it_did_and_not_only_what_it_said() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("what is in main.rs?"));
        conversation.push(Message::assistant_calling(
            "let me look",
            vec![a_call(
                "read_file",
                r#"{"path":"src/main.rs","why":"to see the entry point"}"#,
            )],
        ));
        conversation.push(Message::tool_result("call-1", "fn main() {}"));
        conversation.push(Message::assistant("it is a hello world"));

        assert_eq!(
            conversation.recounted(),
            vec![
                Said::User("what is in main.rs?".to_string()),
                Said::Assistant("let me look".to_string()),
                said_tool("Read(src/main.rs)", "to see the entry point"),
                Said::Assistant("it is a hello world".to_string()),
            ]
        );
    }

    /// The result stays out. A live session showed a one-line summary beside the call, and
    /// putting the file's contents there instead would be a resume showing more than the session
    /// it is resuming ever did.
    #[test]
    fn what_a_call_returned_is_not_recounted() {
        let mut conversation = Conversation::new();
        conversation.push(Message::assistant_calling(
            String::new(),
            vec![a_call("read_file", r#"{"path":"secrets.txt"}"#)],
        ));
        conversation.push(Message::tool_result("call-1", "the file's whole contents"));
        // A result sent as prose, which is the fallback in an untrusted context.
        conversation.push_composed(
            Message::user(format!(
                "{TOOL_RESULT_PREFIX}read_file: the file's whole contents"
            )),
            Composed::ToolResult,
        );

        let recounted = conversation.recounted();
        assert_eq!(recounted, vec![said_tool("Read(secrets.txt)", "")]);
    }

    /// A person may type a prompt that begins with the words the agent opens its own notes with.
    /// What a message is comes from the record, so each of these is still a prompt: it is drawn,
    /// numbered, and found again by a fork.
    #[test]
    fn a_typed_prompt_that_opens_like_a_note_is_still_a_prompt() {
        for prefix in [TOOL_RESULT_PREFIX, RESUMED_PREFIX, COMPACTED_PREFIX] {
            let prompt = format!("{prefix}my experiment: please explain it");
            let mut conversation = Conversation::new();
            conversation.push(Message::user(prompt.clone()));
            conversation.push(Message::assistant("done"));

            assert_eq!(
                conversation.recounted(),
                vec![Said::User(prompt.clone()), Said::Assistant("done".into())],
                "prefix {prefix:?}"
            );
            assert_eq!(
                conversation.snapshot().messages.len(),
                2,
                "the saved record dropped a typed prompt that began {prefix:?}"
            );
            assert_eq!(
                conversation.compaction_boundary(),
                None,
                "prefix {prefix:?}"
            );
        }
    }

    /// And a typed prompt that opens like a note still counts as an exchange for compaction, so
    /// the cut may land in front of it.
    #[test]
    fn a_typed_prompt_that_opens_like_a_note_is_a_place_to_cut() {
        let mut conversation = Conversation::new();
        for (prompt, answer) in [
            ("first", "a"),
            ("second", "b"),
            ("third", "c"),
            (
                &format!("{TOOL_RESULT_PREFIX}my experiment: explain it") as &str,
                "d",
            ),
        ] {
            conversation.push(Message::user(prompt));
            conversation.push(Message::assistant(answer));
        }

        // Four exchanges, two kept: the cut is in front of the third.
        assert_eq!(conversation.compaction_boundary(), Some(4));
        conversation.compacted(4, "earlier");
        assert_eq!(
            conversation
                .recounted()
                .iter()
                .filter(|said| matches!(said, Said::User(_)))
                .count(),
            4,
            "the summary was drawn as a prompt or a typed one was dropped"
        );
    }

    /// A round with several calls is several lines, in the order they were asked for, each with
    /// the reason given for it rather than one reason for the round.
    #[test]
    fn every_call_in_a_round_is_recounted() {
        let mut conversation = Conversation::new();
        conversation.push(Message::assistant_calling(
            String::new(),
            vec![
                a_call(
                    "search",
                    r#"{"pattern":"MAX_STEPS","why":"find where the bound is set"}"#,
                ),
                a_call(
                    "list_files",
                    r#"{"directory":"src","why":"see the layout"}"#,
                ),
            ],
        ));

        assert_eq!(
            conversation.recounted(),
            vec![
                said_tool("Search(MAX_STEPS)", "find where the bound is set"),
                said_tool("List(src)", "see the layout"),
            ]
        );
    }

    /// Arguments a turn ended before writing must not take the line with them. A call announced
    /// and never completed is exactly what a killed session leaves behind.
    #[test]
    fn a_call_with_unreadable_arguments_is_still_recounted() {
        let mut conversation = Conversation::new();
        conversation.push(Message::assistant_calling(
            String::new(),
            vec![a_call("read_file", "{\"path\":")],
        ));

        assert_eq!(conversation.recounted(), vec![said_tool("Read", "")]);
    }

    /// The note is for the planner. Drawn in a transcript it would read as a prompt the user
    /// never typed, in a session they are resuming precisely to see what was said.
    #[test]
    fn the_note_is_not_shown_as_something_the_user_said() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("summarise notes.md"));
        let _ = conversation.next_reference();

        let restored = Conversation::restored(conversation.snapshot());
        assert_eq!(
            restored.recounted(),
            vec![Said::User("summarise notes.md".to_string())]
        );
    }

    /// Restoring twice must not stack notes, which is what would happen if the note were saved
    /// with the messages and then added again on the way back in.
    #[test]
    fn resuming_a_resumed_session_does_not_repeat_the_note() {
        let mut conversation = Conversation::new();
        let _ = conversation.next_reference();

        let once = Conversation::restored(conversation.snapshot());
        let twice = Conversation::restored(once.snapshot());
        let notes = twice
            .messages()
            .iter()
            .filter(|message| {
                message
                    .message
                    .content
                    .as_text()
                    .is_some_and(|text| text.starts_with(RESUMED_PREFIX))
            })
            .count();
        assert_eq!(notes, 1, "the note was added again on top of itself");
    }

    /// Four exchanges, the shape most of the compaction tests need.
    ///
    /// Four rather than three because a cut has to give up at least as much as it keeps, and two
    /// are kept. Three exchanges is a conversation with nothing worth summarising in it.
    fn four_exchanges() -> Conversation {
        let mut conversation = Conversation::new();
        for (prompt, answer) in [
            ("first", "a"),
            ("second", "b"),
            ("third", "c"),
            ("fourth", "d"),
        ] {
            conversation.push(Message::user(prompt));
            conversation.push(Message::assistant(answer));
        }
        conversation
    }

    /// A summary is a paraphrase, and the exchange a session is in the middle of is the one that
    /// can least afford to be paraphrased.
    #[test]
    fn compaction_keeps_the_most_recent_exchanges_word_for_word() {
        let mut conversation = four_exchanges();
        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they asked about the first thing");

        let kept: Vec<String> = conversation
            .messages()
            .iter()
            .map(|message| message.message.content.text())
            .collect();
        assert_eq!(kept.len(), 5, "{kept:?}");
        assert!(kept[0].starts_with(COMPACTED_PREFIX), "{kept:?}");
        assert_eq!(&kept[1..], ["third", "c", "fourth", "d"]);
    }

    fn a_window(file: &str, token: &str) -> ReadWindow {
        ReadWindow {
            file: file.to_string(),
            offset: 1,
            limit: 2000,
            token: token.to_string(),
        }
    }

    /// A summary is a paraphrase, so a read whose round compaction removed leaves the planner
    /// without the lines the notice would point at. The record has to go with the round, and only
    /// that round: a read in a round the compaction kept is still in front of the planner.
    #[test]
    fn compaction_forgets_the_reads_whose_rounds_it_removed_and_keeps_the_others() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("first"));
        let mut early = a_call("read_file", r#"{"path":"old.md"}"#);
        early.id = "call".into();
        conversation.push(Message::assistant_calling("looking", vec![early]));
        conversation.push(Message::tool_result("call", "the old notes"));
        conversation.shown_read(a_window("old.md", "aaaa"));
        conversation.push(Message::assistant("done"));
        conversation.push(Message::user("second"));
        conversation.push(Message::assistant("b"));
        conversation.push(Message::user("third"));
        let mut late = a_call("read_file", r#"{"path":"new.md"}"#);
        late.id = "call".into();
        conversation.push(Message::assistant_calling("looking", vec![late]));
        conversation.push(Message::tool_result("call", "the new notes"));
        conversation.shown_read(a_window("new.md", "bbbb"));
        conversation.push(Message::assistant("c"));
        conversation.push(Message::user("fourth"));
        conversation.push(Message::assistant("d"));

        assert_eq!(
            conversation.for_a_tool().1.token_of("old.md", 1, 2000),
            Some("aaaa")
        );

        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they read some notes");

        let (_, reads) = conversation.for_a_tool();
        assert_eq!(
            reads.token_of("old.md", 1, 2000),
            None,
            "a read from a compacted round is still recorded as shown"
        );
        assert_eq!(
            reads.token_of("new.md", 1, 2000),
            Some("bbbb"),
            "a read from a round compaction kept was forgotten"
        );
    }

    /// Reading the same window again replaces the entry, so the record holds the token the planner
    /// saw last rather than the first one: the older of two is a file that has since been written.
    #[test]
    fn a_window_shown_again_is_recorded_under_its_latest_token() {
        let mut reads = ShownReads::default();
        reads.record(1, a_window("a.md", "aaaa"));
        reads.record(2, a_window("a.md", "bbbb"));
        reads.record(3, a_window("b.md", "cccc"));

        assert_eq!(reads.token_of("a.md", 1, 2000), Some("bbbb"));
        assert_eq!(reads.token_of("b.md", 1, 2000), Some("cccc"));
        assert_eq!(reads.token_of("a.md", 2, 2000), None);
        assert_eq!(reads.token_of("a.md", 1, 10), None);
    }

    /// A second compaction runs on positions the first one moved, so the move has to be exact: the
    /// summary takes position 0, which puts the message that was at `boundary` at position 1.
    #[test]
    fn a_kept_read_follows_its_message_to_its_new_position() {
        let mut reads = ShownReads::default();
        reads.record(2, a_window("gone.md", "aaaa"));
        reads.record(3, a_window("first-kept.md", "bbbb"));
        reads.record(6, a_window("later.md", "cccc"));

        reads.compacted(3);

        assert_eq!(
            reads
                .shown
                .iter()
                .map(|(position, w)| (*position, w.file.as_str()))
                .collect::<Vec<_>>(),
            [(1, "first-kept.md"), (4, "later.md")]
        );
    }

    /// The invariant the cut point exists for. `with_system` answers a call nothing answered by
    /// looking only at the run of results immediately after it, so an assistant message parted
    /// from its results leaves a request the server refuses and a record that says a tool both
    /// ran and did not.
    #[test]
    fn compaction_never_separates_a_call_from_its_results() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("first"));
        conversation.push(Message::assistant_calling(
            "looking",
            vec![a_call("read_file", r#"{"path":"notes.md"}"#)],
        ));
        conversation.push(Message::tool_result("call-1", "the notes"));
        conversation.push(Message::assistant("done"));
        conversation.push(Message::user("second"));
        conversation.push(Message::assistant("b"));
        conversation.push(Message::user("third"));
        conversation.push(Message::assistant("c"));
        conversation.push(Message::user("fourth"));
        conversation.push(Message::assistant("d"));

        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they asked about the notes");

        let sent = conversation.with_system("be careful");
        assert!(
            !sent
                .iter()
                .any(|m| m.content.text().contains("did not run")),
            "a round was cut in half: {sent:?}"
        );
        assert!(
            !sent.iter().any(|m| m.tool_call_id.is_some()),
            "an answer outlived the call it answered: {sent:?}"
        );
    }

    /// A turn goes long on its own: one prompt, then round after round of tool calls. The
    /// exchange count never grows while that happens, so a rule that only gave up whole exchanges
    /// would decline at exactly the moment the context is filling fastest.
    #[test]
    fn a_turn_that_is_long_by_itself_gives_up_its_earlier_rounds() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("find and fix the bug"));
        for round in 0..14 {
            conversation.push(Message::assistant_calling(
                "looking",
                vec![ToolCallRequest {
                    id: format!("call-{round}"),
                    kind: "function".to_string(),
                    function: ToolCallRequestFunction {
                        name: "read_file".to_string(),
                        arguments: r#"{"path":"src/main.rs"}"#.to_string(),
                    },
                    extra_content: None,
                }],
            ));
            conversation.push(Message::tool_result(format!("call-{round}"), "some lines"));
        }

        let boundary = conversation
            .compaction_boundary()
            .expect("a long turn has rounds to give up");
        conversation.compacted(boundary, "they are looking for a bug in src/main.rs");

        assert!(
            conversation.len() < 29,
            "nothing was given up: {}",
            conversation.len()
        );
        let sent = conversation.with_system("be careful");
        assert!(
            !sent
                .iter()
                .any(|m| m.content.text().contains("did not run")),
            "a round was cut in half: {sent:?}"
        );
    }

    /// The rule that makes the fallback safe. Cutting between a call and the results that answer
    /// it leaves the head saying the call never ran and the tail holding an answer to a call that
    /// is not there, which is a request the server refuses.
    #[test]
    fn a_round_in_progress_is_never_a_place_to_cut() {
        // A round may answer one call or several, and where the cut lands in a run of answers
        // depends on how many there are. Asserted over a family of shapes rather than one, so the
        // property holds however the counts above are tuned.
        for answers in 1..=5 {
            let mut conversation = Conversation::new();
            conversation.push(Message::user("fix it"));
            for round in 0..14 {
                conversation.push(Message::assistant_calling(
                    "looking",
                    vec![ToolCallRequest {
                        id: format!("call-{round}"),
                        kind: "function".to_string(),
                        function: ToolCallRequestFunction {
                            name: "search".to_string(),
                            arguments: r#"{"pattern":"x"}"#.to_string(),
                        },
                        extra_content: None,
                    }],
                ));
                for _ in 0..answers {
                    conversation.push(Message::tool_result(format!("call-{round}"), "a result"));
                }
            }

            let boundary = conversation
                .compaction_boundary()
                .expect("something to give up");
            assert!(
                conversation.messages()[boundary]
                    .message
                    .tool_call_id
                    .is_none(),
                "with {answers} answers to a call, the cut landed on one of them"
            );

            conversation.compacted(boundary, "they are looking for a bug");
            let sent = conversation.with_system("be careful");
            assert!(
                !sent
                    .iter()
                    .any(|m| m.content.text().contains("did not run")),
                "with {answers} answers to a call, a round was cut in half"
            );
        }
    }

    /// The same rule where the round was sent as prose. A call whose id the server left off
    /// cannot be answered by id, so the whole round falls back to prose: an assistant message
    /// with no calls on it, then each result as a plain message. None of those carries a
    /// `tool_call_id`, so a cut point found by that field alone lands as readily in the middle of
    /// such a round as between two of them, and counting each result as a round of its own keeps
    /// a fraction of the history [`RECENT_ROUNDS_KEPT`] says it keeps.
    #[test]
    fn a_prose_shaped_round_is_as_indivisible_as_an_api_shaped_one() {
        // The same family of shapes as the test above, for the same reason: where a cut lands in
        // a run of answers depends on how many of them there are.
        for answers in 1..=5 {
            let mut conversation = Conversation::new();
            conversation.push(Message::user("fix it"));
            for _ in 0..14 {
                // No `tool_calls` field, which is what the fallback produces.
                conversation.push(Message::assistant("looking"));
                for answer in 0..answers {
                    conversation.push_composed(
                        Message::user(format!("{TOOL_RESULT_PREFIX}search:\n\nresult {answer}")),
                        Composed::ToolResult,
                    );
                }
            }

            let boundary = conversation
                .compaction_boundary()
                .expect("a long prose turn has rounds to give up");
            assert!(
                !conversation.messages()[boundary]
                    .message
                    .content
                    .as_text()
                    .is_some_and(|text| text.starts_with(TOOL_RESULT_PREFIX)),
                "with {answers} prose answers to a call, the cut landed on one of them"
            );

            conversation.compacted(boundary, "they are looking for a bug");
            let kept = conversation.messages();
            assert!(
                !kept[1]
                    .message
                    .content
                    .as_text()
                    .is_some_and(|text| text.starts_with(TOOL_RESULT_PREFIX)),
                "with {answers} prose answers to a call, the tail opens with an answer to a call \
                 that was summarised away"
            );
            assert_eq!(
                kept.iter()
                    .filter(|message| message.message.role == Role::Assistant)
                    .count(),
                RECENT_ROUNDS_KEPT,
                "with {answers} prose answers to a call, each answer counted as a round"
            );
        }
    }

    /// Compacting a conversation that is all recent would trade the exact words of what is being
    /// worked on for a paraphrase of it, and save nothing worth having.
    #[test]
    fn a_conversation_with_nothing_but_recent_exchanges_is_not_compacted() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("first"));
        conversation.push(Message::assistant("a"));
        conversation.push(Message::user("second"));
        assert_eq!(conversation.compaction_boundary(), None);
    }

    /// A session picked up tomorrow is as full as the one that was put down, and the figure saying
    /// so is the server's, taken once and never recomputable here. Dropped on the way to disk, the
    /// only way back to it is to spend a turn finding out.
    #[test]
    fn a_restored_conversation_remembers_what_its_last_request_came_to() {
        let mut conversation = four_exchanges();
        conversation.measured(90_000);

        let restored = Conversation::restored(conversation.snapshot());

        assert_eq!(restored.last_request_tokens(), 90_000);
    }

    /// A stopped turn is as likely to be continued from a later process as from this one, and the
    /// change it asked to write is still the task's. Dropped on the way to disk, a `continue` after
    /// a resume would be told nothing is written yet about a change that is on disk.
    #[test]
    fn a_restored_conversation_remembers_a_write_asked_for_since_the_last_answer() {
        let mut conversation = four_exchanges();
        conversation.write_requested();

        let written = serde_json::to_string(&conversation.snapshot()).expect("a record");
        let restored =
            Conversation::restored(serde_json::from_str(&written).expect("the record back"));

        assert!(
            restored.asked_to_write(),
            "a write asked for before the record was written came back unasked"
        );
    }

    /// The figure said how large the conversation was before it was shortened. Kept, it would
    /// have the next turn open by trying to compact again on the strength of a measurement of
    /// something that no longer exists.
    #[test]
    fn compacting_forgets_a_measurement_of_the_conversation_it_replaced() {
        let mut conversation = four_exchanges();
        conversation.measured(90_000);

        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they asked about the first two things");

        assert_eq!(conversation.last_request_tokens(), 0);
    }

    /// The bound on futility. A request has a floor it cannot go below, the system prompt and the
    /// tool schemas, so a budget under that floor is unreachable however much history is given
    /// up. Without this the turn summarises itself once per round for the rest of its life,
    /// doubling the requests and shortening nothing: measured at 35 summaries in a turn that
    /// should have made none.
    #[test]
    fn a_cut_that_would_give_up_less_than_it_keeps_is_not_worth_a_request() {
        let mut conversation = four_exchanges();
        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they asked about the first two things");

        // One more exchange is not enough to pay for another summary.
        conversation.push(Message::user("fifth"));
        conversation.push(Message::assistant("e"));
        assert_eq!(conversation.compaction_boundary(), None);

        // Two is.
        conversation.push(Message::user("sixth"));
        conversation.push(Message::assistant("f"));
        assert!(conversation.compaction_boundary().is_some());
    }

    /// The same bound on the round fallback, which is where it was actually costing something: a
    /// long turn added one round at a time, so each compaction gave up one round to keep twelve.
    #[test]
    fn a_long_turn_does_not_summarise_itself_once_per_round() {
        let mut conversation = Conversation::new();
        conversation.push(Message::user("find and fix the bug"));

        let mut summaries = 0;
        for round in 0..40 {
            conversation.push(Message::assistant_calling(
                "looking",
                vec![ToolCallRequest {
                    id: format!("call-{round}"),
                    kind: "function".to_string(),
                    function: ToolCallRequestFunction {
                        name: "read_file".to_string(),
                        arguments: r#"{"path":"src/main.rs"}"#.to_string(),
                    },
                    extra_content: None,
                }],
            ));
            conversation.push(Message::tool_result(format!("call-{round}"), "some lines"));

            // What the turn loop does every round once the budget is passed.
            if let Some(boundary) = conversation.compaction_boundary() {
                conversation.compacted(boundary, "they are looking for a bug in src/main.rs");
                summaries += 1;
            }
        }

        assert!(
            summaries <= 40 / RECENT_ROUNDS_KEPT + 1,
            "a summary every {} rounds or so was expected, and there were {summaries}",
            RECENT_ROUNDS_KEPT
        );
        assert!(summaries > 0, "the longest turn there is never compacted");
    }

    /// There is nothing left to give up, and saying so beats spending a request to summarise a
    /// summary into a summary.
    #[test]
    fn a_head_that_is_only_an_earlier_summary_is_not_compacted_again() {
        let mut conversation = four_exchanges();
        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they asked about the first thing");

        assert_eq!(conversation.compaction_boundary(), None);
    }

    /// And it starts again once there is something new behind the summary, or a long session
    /// would compact once and then grow forever.
    #[test]
    fn a_conversation_that_carries_on_after_a_summary_compacts_again() {
        let mut conversation = four_exchanges();
        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they asked about the first thing");

        for (prompt, answer) in [("fifth", "e"), ("sixth", "f")] {
            conversation.push(Message::user(prompt));
            conversation.push(Message::assistant(answer));
        }
        assert!(conversation.compaction_boundary().is_some());
    }

    /// What compaction shortens is the request, not the record. A person resuming a session they
    /// spent an afternoon on must not find their own earlier prompts missing from it.
    #[test]
    fn what_compaction_took_out_of_the_request_is_still_recounted_to_the_person() {
        let mut conversation = four_exchanges();
        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they asked about the first thing");

        assert_eq!(
            conversation.recounted(),
            vec![
                Said::User("first".to_string()),
                Said::Assistant("a".to_string()),
                Said::User("second".to_string()),
                Said::Assistant("b".to_string()),
                Said::User("third".to_string()),
                Said::Assistant("c".to_string()),
                Said::User("fourth".to_string()),
                Said::Assistant("d".to_string()),
            ]
        );
    }

    /// The summary is written for the planner. Drawn in a transcript it would read as a prompt
    /// the user never typed, on top of the exchange it is standing in for.
    #[test]
    fn the_summary_is_not_shown_as_something_the_user_said() {
        let mut conversation = four_exchanges();
        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they asked about the first thing");

        assert!(
            !conversation.recounted().contains(&Said::User(format!(
                "{COMPACTED_PREFIX}\n\nthey asked about the first thing"
            ))),
            "the summary was drawn as a prompt"
        );
    }

    /// Slots are written once, so a name handed out twice is a collision rather than a muddle.
    #[test]
    fn compaction_does_not_rewind_the_reference_counter() {
        let mut conversation = four_exchanges();
        let _ = conversation.next_reference();
        let _ = conversation.next_reference();

        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they read two files");

        assert_eq!(conversation.next_reference(), SlotId::new("ref:2"));
    }

    /// The quarantine holds the only copy of what a surviving reference names. Dropping it to
    /// save room would leave the planner holding names for content that no longer exists, which
    /// is the failure a resume already has and compaction has no excuse for.
    #[test]
    fn a_reference_minted_before_compaction_still_names_its_content_after_it() {
        let mut conversation = four_exchanges();
        let slot = conversation.next_reference();
        conversation
            .quarantine()
            .writer_for(slot.clone(), Label::untrusted_private())
            .expect("a writer")
            .write("what nobody may read")
            .expect("written");

        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they read a file");

        assert!(
            conversation.quarantine().take_for_effect(&slot).is_ok(),
            "compaction dropped the content behind a live reference"
        );
    }

    /// The names are in the summary and in the retained messages, and nothing in either says
    /// whether they still work, so without this the planner finds out by being refused.
    #[test]
    fn compaction_says_which_references_still_work() {
        let mut conversation = four_exchanges();
        let slot = conversation.next_reference();
        conversation
            .quarantine()
            .writer_for(slot, Label::untrusted_private())
            .expect("a writer")
            .write("what nobody may read")
            .expect("written");

        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they read a file");

        let note = conversation.messages()[0].message.content.text();
        assert!(note.contains("ref:0"), "{note}");
    }

    /// A name minted for content the planner turned out to be allowed to read never became a
    /// slot. Claiming it still names something would send the planner after nothing.
    #[test]
    fn a_reference_that_was_never_quarantined_is_not_claimed_to_be_live() {
        let mut conversation = four_exchanges();
        let _ = conversation.next_reference();

        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they read a file they were allowed to see");

        let note = conversation.messages()[0].message.content.text();
        assert!(!note.contains("ref:0"), "{note}");
    }

    /// A session that outlives the process must come back compacted, and with the archive that
    /// makes the shortening free for the person reading it.
    #[test]
    fn a_compacted_conversation_survives_being_written_down() {
        let mut conversation = four_exchanges();
        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they asked about the first thing");

        let restored = Conversation::restored(conversation.snapshot());
        assert_eq!(restored.len(), 5);
        assert!(
            restored.messages()[0]
                .message
                .content
                .as_text()
                .is_some_and(|text| text.starts_with(COMPACTED_PREFIX))
        );
        assert_eq!(restored.recounted().len(), 8);
    }

    /// Session files predate this field. Someone resuming yesterday's work should not be told
    /// their session is unreadable because a newer build wanted a key it had never written.
    #[test]
    fn a_session_file_written_before_compaction_existed_still_reads() {
        let stored = r#"{"messages":[{"role":"user","content":"what is 2 + 2?"}],
                         "context":"trusted","references":0}"#;
        let snapshot: Snapshot = serde_json::from_str(stored).expect("an older session file");
        let restored = Conversation::restored(snapshot);

        assert_eq!(restored.len(), 1);
        assert_eq!(restored.recounted().len(), 1);
    }

    /// Compaction is not a fresh session. Nothing in it has un-read what the conversation read,
    /// and a summary that came back trusted would be the one upgrade this repository forbids.
    #[test]
    fn compaction_does_not_restore_integrity_the_conversation_had_lost() {
        let mut conversation = four_exchanges();
        conversation.observed(Label::untrusted_public());

        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they asked about the first thing");

        assert_eq!(conversation.context(), Integrity::Untrusted);
    }

    /// The same for what the planner was shown: a summary of the person's mail is as private as
    /// the mail.
    #[test]
    fn compaction_does_not_make_a_private_conversation_public() {
        let mut conversation = four_exchanges();
        conversation.observed(Label::trusted_private());

        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        conversation.compacted(boundary, "they asked about the first thing");

        assert_eq!(conversation.holds(), Confidentiality::Private);
    }

    /// Integrity is the one thing here that must never come back better than it went in, and a
    /// file is the easiest place to make that mistake.
    #[test]
    fn an_untrusted_conversation_does_not_come_back_trusted() {
        let mut conversation = Conversation::new();
        conversation.observed(Label::untrusted_public());

        let snapshot = conversation.snapshot();
        assert_eq!(snapshot.context, "untrusted");
        assert_eq!(
            Conversation::restored(snapshot).context(),
            Integrity::Untrusted
        );
    }

    /// Whatever a file says that this build does not recognise, the answer is untrusted. A
    /// truncated write, a hand edit, or a newer build's word all land in the safe direction.
    #[test]
    fn an_unreadable_integrity_is_read_as_untrusted() {
        for word in ["", "TRUSTED", "yes", "somewhat", "trusted-ish"] {
            let restored = Conversation::restored(Snapshot {
                messages: Vec::new(),
                context: word.to_string(),
                references: 0,
                archive: Vec::new(),
                measured: 0,
                asked_to_write: false,
                holds: PUBLIC.to_string(),
            });
            assert_eq!(
                restored.context(),
                Integrity::Untrusted,
                "{word:?} was read as trusted"
            );
        }
    }

    #[test]
    fn what_the_conversation_has_met_only_ever_falls() {
        let mut conversation = Conversation::new();
        conversation.observed(Label::untrusted_public());
        assert_eq!(conversation.context(), Integrity::Untrusted);

        conversation.observed(Label::trusted_public());
        assert_eq!(
            conversation.context(),
            Integrity::Untrusted,
            "a later trusted turn does not un-see what an earlier one read"
        );
    }

    /// The other axis, and the same mistake: a planner that was shown the person's mail comes back
    /// from a file still holding it, so a resumed session's calls to a server still ask (PERM-9).
    #[test]
    fn a_private_conversation_does_not_come_back_public() {
        let mut conversation = Conversation::new();
        conversation.observed(Label::trusted_private());

        let snapshot = conversation.snapshot();
        assert_eq!(snapshot.holds, "private");
        assert_eq!(
            Conversation::restored(snapshot).holds(),
            Confidentiality::Private
        );
    }

    /// And a conversation shown nothing private comes back public, or every resumed session would
    /// ask before every call to a server.
    #[test]
    fn a_public_conversation_comes_back_public() {
        let snapshot = four_exchanges().snapshot();
        assert_eq!(snapshot.holds, "public");
        assert_eq!(
            Conversation::restored(snapshot).holds(),
            Confidentiality::Public
        );
    }

    /// Read the way the integrity word is: anything but the one word is the safe answer.
    #[test]
    fn an_unreadable_holds_word_is_read_as_private() {
        for word in ["", "PUBLIC", "pub", "yes", "public-ish"] {
            let restored = Conversation::restored(Snapshot {
                messages: Vec::new(),
                context: TRUSTED.to_string(),
                references: 0,
                archive: Vec::new(),
                measured: 0,
                asked_to_write: false,
                holds: word.to_string(),
            });
            assert_eq!(
                restored.holds(),
                Confidentiality::Private,
                "{word:?} was read as public"
            );
        }
    }

    /// A file from before the word was written says nothing about what its planner was shown, so
    /// it reads as private. It still reads.
    #[test]
    fn a_session_file_written_before_holds_was_recorded_reads_as_private() {
        let stored = r#"{"messages":[{"role":"user","content":"what is 2 + 2?"}],
                         "context":"trusted","references":0}"#;
        let snapshot: Snapshot = serde_json::from_str(stored).expect("an older session file");
        let restored = Conversation::restored(snapshot);

        assert_eq!(restored.len(), 1);
        assert_eq!(restored.holds(), Confidentiality::Private);
    }

    #[test]
    fn what_the_planner_was_shown_only_ever_rises() {
        let mut conversation = Conversation::new();
        conversation.observed(Label::trusted_private());
        conversation.observed(Label::trusted_public());
        assert_eq!(
            conversation.holds(),
            Confidentiality::Private,
            "a later turn shown nothing private does not un-see what an earlier one was shown"
        );
    }
}
