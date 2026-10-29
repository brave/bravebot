//! Progress for a run with no interface to draw on.
//!
//! A one-shot run has no spinner and no redraw, so without this it prints nothing at all until
//! the reply arrives. Turns take as many rounds as the work needs, and one that reads a dozen
//! files and edits three of them is a long silence to sit through.
//!
//! Everything here goes to stderr. The reply is the command's output and belongs on stdout,
//! where it can be piped into something else; a progress log that shared that stream would
//! corrupt it.

use crate::json::{self, Call, Stream, Tokens};
use bravebot_agent::report::{Activity, HeardMessage, Reported, Reporter, SentMessage, Shown};
use bravebot_core::todo::Row;
use bravebot_i18n::t;
use bravebot_tui::wrap::display_width;
use std::io::Write;

/// Marks a call, matching the interactive transcript so the two read alike.
const CALL_MARKER: &str = "\u{23fa}";
/// Marks the detail belonging to the line above it.
const DETAIL_MARKER: &str = "\u{23bf}";

pub(crate) use bravebot_approval::QUARANTINE_BAR;

/// The width a shown block is broken at.
///
/// stderr is as often a pipe or a log file as a terminal, so there is no width to ask for, and a
/// previewed line arrives trimmed to 160 characters: wider than most terminals, which would wrap
/// it themselves. A row the terminal wrapped starts at column 0 with no margin in front of it,
/// which is the one thing the marking exists to make impossible, so the rows are the reporter's
/// to make. Eighty is the width assumed of a terminal that has not said otherwise.
const BLOCK_WIDTH: usize = 80;

pub(crate) use bravebot_approval::printable;

/// A file's path as [`printable`] draws it.
pub(crate) fn printable_path(path: &std::path::Path) -> String {
    printable(&path.display().to_string())
}

/// Text of several lines as [`printable`] draws it, each line break kept.
///
/// For words that run over more than one line and may carry a name a checkout chose, such as a
/// notice naming a file or the account of a plan's steps (LAYER-3).
pub(crate) fn printable_lines(text: &str) -> String {
    text.split('\n')
        .map(printable)
        .collect::<Vec<_>>()
        .join("\n")
}

/// One dropped permission entry, said as [`bravebot_agent::permissions::describe`] says it, with
/// the entry and the file it was written in pictured by [`printable`] (LAYER-3).
///
/// Both are text a checkout's settings file chose, and `describe` puts them into the sentence
/// unchanged. The sentence's own words are left alone, so a message that runs over several lines
/// keeps its breaks.
pub(crate) fn described(rejected: &bravebot_core::permissions::Rejected) -> String {
    bravebot_agent::permissions::describe(&bravebot_core::permissions::Rejected {
        text: printable(&rejected.text),
        reason: rejected.reason,
        file: rejected.file.as_deref().map(printable),
    })
}

/// One line of shown content as rows, each beginning with the margin.
fn marked(margin: &str, line: &str) -> Vec<String> {
    // One column at least. A margin wider than the block still gets its margin, and the content
    // is broken a character at a time rather than written outside it.
    let room = BLOCK_WIDTH.saturating_sub(display_width(margin)).max(1);
    let mut rows = Vec::new();
    let mut row = String::new();
    let mut used = 0usize;

    for c in printable(line).chars() {
        let mut buffer = [0u8; 4];
        let reached = display_width(c.encode_utf8(&mut buffer));
        if used > 0 && used + reached > room {
            rows.push(format!("{margin}{row}"));
            row.clear();
            used = 0;
        }
        row.push(c);
        used += reached;
    }

    // Always a row, so an empty line is a marked one rather than a gap in the block.
    rows.push(format!("{margin}{row}"));
    rows
}

/// The token counts of a usage report, as the result object holds them.
fn tokens_of(spent: bravebot_agent::Spent) -> Tokens {
    Tokens {
        total: spent.tokens,
        output: spent.output_tokens,
        context: spent.context_tokens,
        cache_read: spent.cached.read_tokens,
        cache_written: spent.cached.written_tokens,
    }
}

/// Writes progress as it arrives.
///
/// Generic over the sink so a test can read back exactly what a run would have printed.
pub struct Progress<W: Write> {
    out: W,
    /// What the turn running now has cost, as it last reported it. A turn reports from nothing, so
    /// this restarts when the next one begins.
    spent: bravebot_agent::Spent,
    /// What the rounds before this one and the judge's checks cost, for a run that works towards a
    /// goal (CLI-35). Zero for every other run, which is what leaves their figures as the turn
    /// reported them.
    earlier: bravebot_agent::Spent,
    /// Every call the run made, in order, for a result object that has to list them.
    ///
    /// Kept here because this is the one thing told about every call, start and finish, and a
    /// second listener would be a second set of hooks to keep in step with dispatch.
    calls: Vec<Call>,
    /// Every notice the turn said as it ran, in order, for a run that ends without an outcome.
    ///
    /// Kept rather than printed as it arrives, because a turn that produces an outcome carries
    /// these on it and the ending prints them from there: written twice, a run that loaded a skill
    /// would say so once in the middle of its progress and once at the end.
    notices: Vec<String>,
    /// How many of `notices` the rounds before this one said, for a goal run (CLI-35).
    earlier_notices: usize,
    /// Where each call and each finished request is written as it happens, for `--json-stream`.
    stream: Option<Stream>,
    /// Where the turn's prompt entered the recounted conversation, for a run that writes its
    /// session down.
    prompt_at: Option<usize>,
}

impl<W: Write> Progress<W> {
    pub fn new(out: W) -> Self {
        Self {
            out,
            spent: Default::default(),
            earlier: Default::default(),
            calls: Vec::new(),
            notices: Vec::new(),
            earlier_notices: 0,
            stream: None,
            prompt_at: None,
        }
    }

    /// Also write an event line for each finished call and each finished request.
    pub fn streaming(mut self, stream: Option<Stream>) -> Self {
        self.stream = stream;
        self
    }

    /// Where the prompt entered the conversation, once the turn has said.
    pub fn prompt_at(&self) -> Option<usize> {
        self.prompt_at
    }

    /// What the run has cost: every round so far and every check of its goal, with the turn running
    /// now.
    pub fn spent(&self) -> bravebot_agent::Spent {
        self.counted()
    }

    /// [`Progress::spent`], under a name the `Reporter` method of the same name cannot take.
    fn counted(&self) -> bravebot_agent::Spent {
        let mut total = self.spent;
        total.add_spent(&self.earlier);
        // A turn that has not made its first request has no reading yet, and the last one's is
        // still how full the conversation is.
        if total.context_tokens == 0 {
            total.context_tokens = self.earlier.context_tokens;
        }
        total.timing.add(self.earlier.timing);
        total
    }

    /// Start the next round of a goal run. What the finished one cost is kept, so the figures a
    /// stream and a failure report keep rising rather than starting again from nothing.
    pub fn next_round(&mut self) {
        self.earlier = self.counted();
        self.spent = Default::default();
        self.earlier_notices = self.notices.len();
    }

    /// What every round before the one running now, and every check of the goal, cost.
    pub fn earlier(&self) -> bravebot_agent::Spent {
        self.earlier
    }

    /// Add what one check of the goal cost, which no turn reports.
    pub fn charge(&mut self, spent: bravebot_agent::Spent) {
        let before = tokens_of(self.counted());
        self.earlier.add_spent(&spent);
        self.announce(before);
    }

    /// Write a usage event if the cost differs from `before`.
    fn announce(&mut self, before: Tokens) {
        let counted = tokens_of(self.counted());
        if let (true, Some(stream)) = (counted != before, self.stream.as_mut()) {
            stream.line(&json::usage_event(&counted));
        }
    }

    /// What the run called, in the order it called it.
    pub fn calls(&self) -> &[Call] {
        &self.calls
    }

    /// What the turn said as it ran, in the order it said it.
    ///
    /// What a turn that failed has instead of an outcome. A hook that could not be started is said
    /// here as it happens (HOOK-7), and a turn that then fails for its own reasons produces nothing
    /// to carry it, so this is the only copy left.
    ///
    /// More than an outcome carries, since a sentence said before the turn had one still reaches a
    /// reporter: a premium endpoint that cannot be used is the case there is today. So a run that
    /// failed says that too and a run that answered says nothing about it, the latter being a gap
    /// of its own rather than anything this reads too widely.
    pub fn notices(&self) -> &[String] {
        &self.notices
    }

    /// A failed write is dropped. Progress announces and has nothing to refuse with, so a
    /// closed stderr means nobody is reading, not that the turn should stop.
    ///
    /// Every line is drawn through [`printable_lines`] on the way out: a call's target, reason and
    /// note can name a file a checkout chose (LAYER-3), and a sentence added later is covered
    /// without its author having to remember.
    fn say(&mut self, line: &str) {
        let _ = writeln!(self.out, "{}", printable_lines(line));
        let _ = self.out.flush();
    }
}

impl<W: Write> Reporter for Progress<W> {
    fn prompt_recorded(&mut self, at: usize) {
        self.prompt_at = Some(at);
    }

    fn spent(&mut self, spent: bravebot_agent::Spent) {
        // Reported more often than the figure moves: at the start, with nothing, and again for a
        // request already counted. Only a change is an event, since a line repeating the last one
        // tells a caller following the stream nothing and reads as a request that did not happen.
        let before = tokens_of(self.counted());
        self.spent = spent;
        self.announce(before);
    }

    /// Left to the per-call lines. A task list redrawn in place is legible; the same list
    /// reprinted in full after every step is a wall of repetition.
    fn todos(&mut self, _rows: Vec<Row>) {}

    fn narration(&mut self, text: String) {
        // Dropped here rather than by the turn, which cannot look at the text to decide. This
        // side may: it has been released, and a blank line is a presentation question.
        if text.trim().is_empty() {
            return;
        }
        self.say(&format!("\n{text}\n"));
    }

    /// Kept for the ending to print. See [`Progress::notices`] for why it is not said here.
    fn notice(&mut self, text: String) {
        // Every round of a goal run says what a turn says about itself, and a sentence an earlier
        // round already said is the same fact, so a run that fails in round six does not print it
        // six times.
        if self.notices[..self.earlier_notices].contains(&text) {
            return;
        }
        self.notices.push(text);
    }

    /// Printed as the call begins, which is the whole point: a slow call should be visible
    /// while it is slow rather than only once it is over.
    fn tool_started(&mut self, activity: Activity) {
        let why = activity.why.lines().next().unwrap_or_default().trim();
        if why.is_empty() {
            self.say(&format!("{CALL_MARKER} {}", activity.line()));
        } else {
            self.say(&format!("{CALL_MARKER} {why}"));
            self.say(&format!("  {DETAIL_MARKER} {}", activity.line()));
        }
    }

    fn tool_finished(&mut self, activity: Activity) {
        // Recorded as it finishes rather than as it starts, because whether it was refused is
        // half of what a caller wants and is not known until then. A call dispatch named nothing
        // for is left out: an empty name is nothing a program can match on.
        if !activity.tool.is_empty() {
            let call = Call {
                tool: activity.tool.clone(),
                target: activity.target.clone(),
                refused: activity.failed,
            };
            if let Some(stream) = self.stream.as_mut() {
                stream.line(&json::call_event(&call));
            }
            self.calls.push(call);
        }
        if let Some(note) = &activity.note {
            // The same figure the interactive transcript puts here, from the same function, for
            // the same reason: a run watched from a terminal has no other way to tell a slow check
            // from a slow program.
            let waited = bravebot_tui::indicator::format_waited(activity.waited);
            self.say(&format!("  {DETAIL_MARKER} {note}{waited}"));
        }
    }

    /// Written where the interactive transcript would draw the block, and for the same reason:
    /// the person watching owns the workspace and is the one who can tell whether the agent is
    /// working on the right file. Silence here is indistinguishable from having nothing to show.
    ///
    /// On stderr with the rest of the progress, since stdout is the reply.
    fn quarantined(&mut self, shown: Shown) {
        self.draw_quarantined(&shown);
    }

    /// Written apart from the person's prompts: a heading naming the sender as the roster does, then
    /// the words quoted where the planner was given them, and in the marked block every quarantined
    /// thing is drawn in where it was not (MSG-11).
    fn session_message(&mut self, heard: HeardMessage) {
        let from: String = heard
            .from
            .chars()
            .take(bravebot_agent::messages::ID_SHOWN)
            .collect();
        self.say("");
        self.say(&printable(&t!(
            session_message_heading,
            from = from,
            name = heard.name,
            label = heard.label
        )));
        match heard.body {
            Reported::Said(text) => self.draw_quoted(&text),
            Reported::Kept(shown) => self.draw_quarantined(&shown),
        }
    }

    /// Written apart from the person's prompts, as a message that came in is: a heading naming the
    /// receiver as the roster does, then every word quoted. Drawn for every message, so one sent
    /// under a standing answer is seen as it goes (MSG-11).
    fn message_sent(&mut self, sent: SentMessage) {
        let to: String = sent
            .to
            .chars()
            .take(bravebot_agent::messages::ID_SHOWN)
            .collect();
        self.say("");
        self.say(&printable(&t!(
            session_message_sent_heading,
            to = to,
            name = sent.name,
            label = sent.label
        )));
        self.draw_quoted(&sent.text);
    }
}

impl<W: Write> Progress<W> {
    /// The words of a message, one quoted row to each line of them.
    fn draw_quoted(&mut self, text: &str) {
        for line in text.lines() {
            for row in marked("  > ", line) {
                self.say(&row);
            }
        }
        self.say("");
    }

    fn draw_quarantined(&mut self, shown: &Shown) {
        let margin = format!("  {QUARANTINE_BAR} ");

        // The heading is marked like the content, because the origin is not this module's text:
        // it can be a filename read out of a quarantined listing. So it is neutralised too, and a
        // long one continues on another marked row rather than outside the block.
        let mut block = vec![t!(
            quarantined_heading,
            origin = &shown.origin,
            label = &shown.label
        )];
        block.extend(shown.reach.note().map(|note| format!("  {note}")));

        // The preview's colour codes are taken out and counted (VIEW-30). The heading is not
        // touched, since its origin can be a filename and a person is shown every byte of one.
        let mut dropped = 0;
        block.extend(shown.preview.iter().map(|line| {
            let (plain, taken) = bravebot_tui::colour_codes::without(line);
            dropped += taken;
            plain
        }));

        // Said rather than silently dropped: a preview that stops without saying so reads as the
        // whole of what was there.
        if shown.lines > shown.preview.len() {
            block.push(t!(
                transcript_more_lines,
                count = shown.lines - shown.preview.len()
            ));
        }
        block.extend(bravebot_tui::colour_codes::note(dropped));

        self.say("");
        for line in &block {
            for row in marked(&margin, line) {
                self.say(&row);
            }
        }
        self.say("");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::report::Reach;

    fn log(run: impl FnOnce(&mut Progress<&mut Vec<u8>>)) -> String {
        let mut buffer = Vec::new();
        let mut progress = Progress::new(&mut buffer);
        run(&mut progress);
        String::from_utf8(buffer).expect("utf-8")
    }

    /// The line has to appear before the call finishes, or a one-shot run is silent for
    /// exactly as long as the slow part takes.
    #[test]
    fn a_call_is_printed_when_it_begins() {
        let written = log(|p| p.tool_started(Activity::running("Read", "src/main.rs")));
        assert!(written.contains("Read(src/main.rs)"), "got: {written}");
    }

    /// The line says what the call is for as well as what it touches. A run read from a terminal
    /// otherwise shows every call and nothing of why it was made.
    #[test]
    fn a_call_is_printed_with_the_reason_it_was_made() {
        let written = log(|p| {
            p.tool_started(
                Activity::running("Search", "MAX_STEPS")
                    .saying_why("find where the bound is set\nand a second line"),
            )
        });
        let headline = written.find("find where the bound is set");
        let call = written.find("Search(MAX_STEPS)");
        assert!(
            headline.is_some() && call.is_some() && headline < call,
            "the reason should head the call: {written}"
        );
        assert!(!written.contains("a second line"), "got: {written}");
    }

    /// A quarantined listing entry's real file name reaches the call line in place of its
    /// reference (LAYER-3), the way it did for [`the_lines_of_a_call_draw_a_checkouts_names_as_pictures`]
    /// below, but through the `ref:N(flags):name` form `target_of` builds rather than a bare
    /// name, which is the shape #3064 reported (`say` already pictures it through
    /// [`printable_lines`]).
    #[test]
    fn a_call_line_draws_a_targets_control_characters_as_pictures() {
        let written = log(|p| {
            p.tool_started(Activity::running(
                "Read",
                "ref:1(U,priv):x\u{1b}]0;PWNED-TITLE\u{7}\u{1b}[31mred.txt",
            ));
        });
        assert!(
            !written.contains('\u{1b}') && !written.contains('\u{7}'),
            "got: {written:?}"
        );
        assert!(
            written.contains("x\u{241b}]0;PWNED-TITLE\u{2407}"),
            "the escape should be drawn as its picture: {written:?}"
        );
    }

    #[test]
    fn a_finished_call_prints_what_came_of_it() {
        let written = log(|p| {
            p.tool_finished(Activity::running("Search", "todo").done("4 matches"));
        });
        assert!(written.contains("4 matches"), "got: {written}");
    }

    /// A call that ran a model inside itself says how long it waited there. A one-shot run has no
    /// status line and no totals on the screen, so this is the only place it could be said at all.
    #[test]
    fn a_finished_call_prints_what_it_spent_at_a_model() {
        let written = log(|p| {
            p.tool_finished(
                Activity::running("Read output", "ref:1")
                    .done("3 lines, read")
                    .after_waiting(Some(std::time::Duration::from_secs(8))),
            );
        });
        assert!(written.contains("8s at the model"), "got: {written}");
    }

    /// And a call that asked none says nothing about one.
    #[test]
    fn a_finished_call_that_asked_no_model_prints_nothing_about_one() {
        let written = log(|p| {
            p.tool_finished(Activity::running("Search", "todo").done("4 matches"));
        });
        assert!(!written.contains("at the model"), "got: {written}");
    }

    /// TRACE-11, LAYER-3: a call's target, reason and note can name a file a checkout chose, and
    /// the line each is said on is drawn with its control characters pictured.
    #[test]
    fn the_lines_of_a_call_draw_a_checkouts_names_as_pictures() {
        let name = "x\u{1b}]0;T\u{7}\u{1b}[31mred.txt";
        let pictured = "x\u{241b}]0;T\u{2407}\u{241b}[31mred.txt";
        let written = log(|p| {
            p.tool_started(
                Activity::running("Read", name).saying_why(format!("open {name} to check it")),
            );
            p.tool_finished(Activity::running("Read", name).done(format!("read {name}")));
        });
        assert_eq!(
            written.chars().find(|c| c.is_control() && *c != '\n'),
            None,
            "a control character reached the line: {written:?}"
        );
        assert_eq!(
            written.matches(pictured).count(),
            3,
            "a name was dropped or left raw: {written:?}"
        );
    }

    /// A refusal is worth printing too: a run that quietly skipped a write and answered anyway
    /// is the case a user most needs to see.
    #[test]
    fn a_refusal_is_printed() {
        let written = log(|p| {
            p.tool_finished(Activity::running("Write", "a.rs").failed("refused: not approved"));
        });
        assert!(written.contains("refused"), "got: {written}");
    }

    #[test]
    fn narration_is_printed() {
        let written = log(|p| p.narration("Reading the config first.".into()));
        assert!(written.contains("Reading the config first."));
    }

    /// A round with nothing to say still reports, so the blank must not become an empty line
    /// nobody can account for.
    #[test]
    fn empty_narration_prints_nothing() {
        assert!(log(|p| p.narration(String::new())).is_empty());
        assert!(log(|p| p.narration("  \n ".into())).is_empty());
    }

    /// The bar the block is written behind.
    const BAR: char = '\u{2503}';

    fn quarantining(preview: Vec<String>, lines: usize) -> Shown {
        Shown {
            origin: "notes.md".to_string(),
            reach: Reach::NotThePlanner,
            label: "(U,priv)".to_string(),
            preview,
            lines,
        }
    }

    /// The whole of the reported defect: a one-shot run used to print nothing at all where the
    /// interactive transcript draws a block, and silence is indistinguishable from there having
    /// been nothing to show.
    ///
    /// Every row carries the margin, because a row that begins with content is a row the content
    /// could have put a margin of its own in front of.
    #[test]
    fn quarantined_content_is_shown_and_marked_on_every_line() {
        let written =
            log(|p| p.quarantined(quarantining(vec!["api_key = hunter2".to_string()], 1)));

        assert!(written.contains("api_key = hunter2"), "got: {written}");
        assert!(written.contains("notes.md"), "got: {written}");
        for row in written.lines().filter(|row| !row.is_empty()) {
            assert_eq!(
                row.chars().position(|c| !c.is_whitespace()),
                row.chars().position(|c| c == BAR),
                "a row of the block begins with content rather than the margin: {row:?}"
            );
        }
    }

    /// Content kept from the planner is what every block holds, so its heading does not repeat
    /// that on each one. Content in no model's context is the exception and says so.
    #[test]
    fn a_block_states_its_reach_only_where_that_is_the_exception() {
        let ordinary =
            log(|p| p.quarantined(quarantining(vec!["api_key = hunter2".to_string()], 1)));
        assert!(!ordinary.contains("planner"), "got: {ordinary}");
        assert!(!ordinary.contains("no model"), "got: {ordinary}");

        let mut unread = quarantining(vec!["a remark".to_string()], 1);
        unread.reach = Reach::NoModel;
        let written = log(|p| p.quarantined(unread));
        assert!(written.contains("no model's context"), "got: {written}");
    }

    fn heard(body: Reported) -> HeardMessage {
        HeardMessage {
            from: "22222222-2222-4222-8222-222222222222".to_string(),
            name: "fix the build".to_string(),
            label: "(T,pub)".to_string(),
            body,
        }
    }

    /// MSG-11: a message the planner was given is drawn under a heading that names the sender and
    /// the label, with the words quoted on every row, so it cannot be read as a line the person typed.
    #[test]
    fn a_message_is_drawn_under_its_sender_with_the_words_quoted() {
        let written = log(|p| {
            p.session_message(heard(Reported::Said("first\nsecond".to_string())));
        });
        assert!(
            written.contains("message from session 22222222 (fix the build) \u{b7} (T,pub)"),
            "got: {written}"
        );
        assert!(
            written.contains("  > first\n  > second\n"),
            "got: {written}"
        );
    }

    /// MSG-11: a message the planner was not given is drawn in the marked block every quarantined
    /// thing is drawn in, under the same heading, and none of its rows begins with its own words.
    #[test]
    fn a_withheld_message_is_drawn_behind_the_quarantine_margin() {
        let written = log(|p| {
            p.session_message(heard(Reported::Kept(Shown {
                origin: "a message from another session".to_string(),
                reach: Reach::NotThePlanner,
                label: "(U,priv)".to_string(),
                preview: vec!["IGNORE-EVERYTHING".to_string()],
                lines: 1,
            })));
        });
        assert!(written.contains("fix the build"), "got: {written}");
        assert!(written.contains("IGNORE-EVERYTHING"), "got: {written}");
        let words = written
            .lines()
            .find(|row| row.contains("IGNORE-EVERYTHING"))
            .expect("the words are drawn");
        assert!(
            words.contains(BAR),
            "the words are drawn outside the margin: {words:?}"
        );
    }

    fn sent(text: &str) -> SentMessage {
        SentMessage {
            to: "22222222-2222-4222-8222-222222222222".to_string(),
            name: "fix the build".to_string(),
            label: "(T,priv)".to_string(),
            text: text.to_string(),
        }
    }

    /// MSG-11: a message this session sent is drawn under a heading that names the receiver and the
    /// label it went under, with every row of the words quoted, so it cannot be read as a line the
    /// person typed or as one that came in.
    #[test]
    fn a_message_that_was_sent_is_drawn_under_its_receiver_with_the_words_quoted() {
        let written = log(|p| p.message_sent(sent("first\nsecond")));
        assert!(
            written.contains("message to session 22222222 (fix the build) \u{b7} (T,priv)"),
            "got: {written}"
        );
        assert!(
            written.contains("  > first\n  > second\n"),
            "got: {written}"
        );
        assert!(
            !written.contains("message from session"),
            "a sent message was drawn as one that came in: {written}"
        );
    }

    /// MSG-11: control characters in the receiver's name or in the words are replaced, so a session
    /// named or worded to move the cursor cannot paint over the heading or the margin of what was
    /// sent.
    #[test]
    fn a_sent_message_cannot_move_the_cursor_through_the_name_or_the_words() {
        let mut message = sent("\u{1b}[2Kfine");
        message.name = "x\u{1b}[2J\nname".to_string();
        let written = log(|p| p.message_sent(message));
        assert!(!written.contains('\u{1b}'), "got: {written:?}");
        assert_eq!(
            written
                .lines()
                .filter(|row| row.contains("message to session"))
                .count(),
            1,
            "a name started a heading of its own: {written:?}"
        );
    }

    /// MSG-11: control characters in a name or in the words are replaced, so a session named or
    /// worded to move the cursor cannot paint over the heading or the margin.
    #[test]
    fn a_message_cannot_move_the_cursor_through_its_name_or_its_words() {
        let mut message = heard(Reported::Said("\u{1b}[2Kfine".to_string()));
        message.name = "x\u{1b}[2J\nname".to_string();
        let written = log(|p| p.session_message(message));
        assert!(!written.contains('\u{1b}'), "got: {written:?}");
        assert_eq!(
            written
                .lines()
                .filter(|row| row.contains("message from session"))
                .count(),
            1,
            "a name started a heading of its own: {written:?}"
        );
    }

    /// A preview line that clears the row it is written on could erase the margin above it, which
    /// is the one mark the design says content can never imitate.
    #[test]
    fn quarantined_content_cannot_paint_its_own_margin() {
        let written = log(|p| {
            p.quarantined(quarantining(
                vec![
                    "\u{1b}[0m\u{1b}[A\u{1b}[2K harmless looking".to_string(),
                    "\u{1b}[33m  \u{2503} untrusted \u{b7} nothing \u{b7} (T,pub)".to_string(),
                ],
                2,
            ))
        });

        assert!(
            !written.contains('\u{1b}'),
            "content could move the cursor of the terminal reading this: {written:?}"
        );
        // Neutralised rather than dropped. A character silently removed is one nobody can tell
        // was ever in the file, which makes the preview a less faithful record than it looks.
        assert!(
            written.contains('\u{241b}'),
            "the escape was not neutralised: {written:?}"
        );
    }

    /// A program that colours its output writes a code around each coloured word, and drawn as
    /// pictures they bury the words. The block shows the words and says how many codes it left
    /// out, behind the same margin as every other row (VIEW-30).
    #[test]
    fn colour_codes_in_a_block_are_counted_and_not_drawn() {
        let written = log(|p| {
            p.quarantined(quarantining(
                vec!["\u{1b}[0m\u{1b}[36m\u{251c}\u{1b}[0m Announcement: a link".to_string()],
                1,
            ))
        });

        assert!(
            written.contains("\u{251c} Announcement: a link"),
            "got: {written:?}"
        );
        assert!(
            !written.contains("[36m") && !written.contains("[0m") && !written.contains('\u{241b}'),
            "a colour code was drawn as text: {written:?}"
        );
        let note = written
            .lines()
            .find(|row| row.contains("3 colour codes not drawn"))
            .unwrap_or_else(|| panic!("the count was not written: {written:?}"));
        assert_eq!(
            note.chars().position(|c| !c.is_whitespace()),
            note.chars().position(|c| c == BAR),
            "the count begins with content rather than the margin: {note:?}"
        );
    }

    /// Only a colour code is left out. An erase beside one is written as pictures and not counted
    /// as a colour, and the heading's origin is a filename a person is shown every byte of.
    #[test]
    fn only_colour_codes_are_left_out_of_a_block() {
        let mut shown = quarantining(vec!["\u{1b}[31mred\u{1b}[2Jcleared".to_string()], 1);
        shown.origin = "a\u{1b}[8mb.md".to_string();
        let written = log(|p| p.quarantined(shown));

        assert!(
            written.contains("red\u{241b}[2Jcleared"),
            "got: {written:?}"
        );
        assert!(
            written.contains("a\u{241b}[8mb.md"),
            "the origin was changed: {written:?}"
        );
        assert!(
            written.contains("1 colour code not drawn"),
            "got: {written:?}"
        );
        assert!(!written.contains('\u{1b}'), "got: {written:?}");
    }

    /// A block with no colour code says nothing about them.
    #[test]
    fn a_block_without_colour_codes_says_nothing_about_them() {
        let written = log(|p| p.quarantined(quarantining(vec!["plain".to_string()], 1)));

        assert!(!written.contains("colour code"), "got: {written:?}");
    }

    /// The sentence about a dropped rule is the catalog's, and the rule and the file are a
    /// checkout's. Both are drawn as pictures, and the sentence around them keeps its own breaks,
    /// so the replacement is of the pieces rather than of the whole line.
    #[test]
    fn a_dropped_rule_and_the_file_it_was_written_in_are_described_as_pictures() {
        let rejected = bravebot_core::permissions::Rejected {
            text: "\u{1b}[8mrule".to_string(),
            reason: bravebot_core::permissions::Unreadable::NotAList,
            file: Some("\u{1b}[8mfile.json".to_string()),
        };

        let said = described(&rejected);

        assert!(
            !said.chars().any(|c| c.is_control() && c != '\n'),
            "the sentence holds a control character from the checkout: {said:?}"
        );
        for piece in ["\u{241b}[8mrule", "\u{241b}[8mfile.json"] {
            assert!(
                said.contains(piece),
                "{piece:?} was left out of the sentence rather than pictured: {said:?}"
            );
        }
    }

    /// A previewed line arrives trimmed to a width wider than most terminals. Left whole, the
    /// terminal wraps it, and the row it wraps onto starts at column 0 with no margin at all.
    #[test]
    fn a_line_too_wide_for_the_block_is_broken_behind_the_margin() {
        let long = "x".repeat(300);
        let written = log(|p| p.quarantined(quarantining(vec![long], 1)));

        let body: Vec<&str> = written.lines().filter(|row| row.contains('x')).collect();
        assert!(body.len() > 1, "the line was not broken: {written}");
        for row in body {
            assert!(row.starts_with("  \u{2503} "), "{row:?}");
            assert!(
                row.chars().count() <= 80,
                "a row is wider than the block: {row:?}"
            );
        }
    }

    /// A preview that stops without saying so reads as the whole of what was there.
    #[test]
    fn a_trimmed_preview_says_how_much_it_left_out() {
        let written = log(|p| p.quarantined(quarantining(vec!["first".to_string()], 9)));

        assert!(written.contains("8 more lines"), "got: {written}");
    }

    /// Nobody reading is not a failure: there is no return value to refuse with, by design.
    #[test]
    fn a_closed_stream_is_not_an_error() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            }
        }

        let mut progress = Progress::new(Closed);
        progress.tool_started(Activity::running("Read", "a.rs"));
        progress.narration("still fine".into());
    }

    /// A turn that starts again from nothing is one round of a run that works towards a goal
    /// (CLI-35), so what the run has cost is the rounds before it and the checks between them as
    /// well, whether it ends in an answer or in a failure.
    #[test]
    fn what_a_run_has_cost_adds_up_across_its_rounds_and_its_checks() {
        let a_round = |tokens: u64| bravebot_agent::Spent {
            tokens,
            output_tokens: tokens / 10,
            context_tokens: tokens,
            ..Default::default()
        };
        let mut progress = Progress::new(std::io::sink());

        Reporter::spent(&mut progress, a_round(100));
        progress.charge(a_round(30));
        progress.next_round();
        assert_eq!(
            progress.spent().tokens,
            130,
            "the round that finished was forgotten or counted twice"
        );
        Reporter::spent(&mut progress, bravebot_agent::Spent::default());
        assert_eq!(
            progress.spent().tokens,
            130,
            "the round before was forgotten"
        );
        assert_eq!(
            progress.spent().context_tokens,
            100,
            "a round that has made no request reads as an empty conversation"
        );
        Reporter::spent(&mut progress, a_round(40));

        let total = progress.spent();
        assert_eq!(total.tokens, 170);
        assert_eq!(total.output_tokens, 4 + 10 + 3);
        assert_eq!(
            total.context_tokens, 40,
            "the last request is how full it is"
        );
        assert_eq!(progress.earlier().tokens, 130);
    }

    /// A caller following a stream reads the usage as a running total, so the first request of the
    /// next round must not read as the total falling back to its own.
    #[test]
    fn a_streamed_total_never_falls_when_the_next_round_starts() {
        #[derive(Clone, Default)]
        struct Shared(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
        impl Write for Shared {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                let mut held = self
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                held.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let written = Shared::default();
        let mut progress =
            Progress::new(std::io::sink()).streaming(Some(Stream::new(written.clone())));
        let a_round = |tokens: u64| bravebot_agent::Spent {
            tokens,
            ..Default::default()
        };

        Reporter::spent(&mut progress, a_round(100));
        progress.charge(a_round(30));
        progress.next_round();
        Reporter::spent(&mut progress, bravebot_agent::Spent::default());
        Reporter::spent(&mut progress, a_round(40));

        let text = String::from_utf8(written.0.lock().expect("a lock").clone()).expect("utf-8");
        let totals: Vec<u64> = text
            .lines()
            .map(|line| {
                let at = line.find(r#""total":"#).expect(line) + r#""total":"#.len();
                let digits: String = line[at..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                digits.parse().expect(line)
            })
            .collect();
        assert_eq!(totals, [100, 130, 170], "{text}");
    }

    /// Each round of a goal run says what a turn says about itself, so what a run that fails in a
    /// later round has to print is each sentence once, and a sentence only a later round says too.
    #[test]
    fn a_notice_a_later_round_says_again_is_kept_once() {
        let mut progress = Progress::new(std::io::sink());

        progress.notice("loaded AGENTS.md".to_string());
        progress.next_round();
        progress.notice("loaded AGENTS.md".to_string());
        progress.notice("a hook failed".to_string());
        progress.notice("a hook failed".to_string());

        assert_eq!(
            progress.notices(),
            ["loaded AGENTS.md", "a hook failed", "a hook failed"],
            "a sentence one round said twice is that round's to say, and another round's is not"
        );
    }
}
