//! How much a session asks before it acts.
//!
//! A different axis from [`crate::Mode`], which decides *when* control flow is settled. This decides
//! what happens when a gate has already established that an effect needs a person's consent: whether
//! the question is put to them, answered yes on their behalf, or answered no without asking.
//!
//! Held by the session rather than by a task, because it is a standing answer the person changes
//! while they work: they watch a turn edit files it should not be editing and want the next one to
//! stop and ask, or they have read enough diffs for one afternoon and want the next ten to go
//! through. One key cycles it, and the mode in force is written under the box for as long as it is.
//!
//! # Why plan mode refuses rather than asks
//!
//! [`PermissionMode::Plan`] is not a stricter [`PermissionMode::Ask`]. Its point is that the session
//! is for deciding what to do, so a write is refused outright and the planner is told why: a mode
//! whose only difference was that the person keeps saying no is one where a planner keeps proposing
//! writes and a person keeps declining them, which is the interaction it exists to avoid.
//!
//! Commands are still asked about, and deliberately. Research is most of what planning is, `git log`
//! and a test run are how it is done, and a mode that could not read the tree would write plans from
//! less than the person could see themselves.

use crate::confirm::{
    Confirmer, Decision, OutputRequest, RunDecision, RunRequest, VetRequest, VouchRequest,
    WriteRequest,
};
use bravebot_core::vetting::{Endorsed, Verdict};

/// How much this session asks before it acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionMode {
    /// Every effect is put to the person. What a session has always done, and the default.
    #[default]
    Ask,
    /// Writes go through unasked. Commands are still asked about.
    ///
    /// The two are not the same risk: a write lands in a tree the person can read afterwards, and
    /// `git diff` will show them all of it. A command runs with everything their own shell has and
    /// leaves no diff, so it keeps its prompt.
    AcceptEdits,
    /// Nothing is written. The session is for deciding what to do, not doing it.
    Plan,
    /// Nothing is asked at all, including vouching for files nobody vouched for.
    ///
    /// What `--dangerously-skip-permissions` selects, reachable only where it was given. See
    /// [`Confining::confirm_vouch`] for what the last of those costs, and
    /// [`Confining::confirm_vetted_read`] for the one answer this mode does not give itself.
    Bypass,
}

impl PermissionMode {
    /// The modes one key cycles through, in order, ending back at the first.
    ///
    /// `Bypass` is not among them: it is reachable only where the flag was given, so the ladder
    /// depends on that and is built by [`PermissionMode::cycle`] rather than listed here.
    const LADDER: [Self; 3] = [Self::Ask, Self::AcceptEdits, Self::Plan];

    /// The next mode round the ladder.
    ///
    /// `bypass_available` is whether `--dangerously-skip-permissions` was given. Without it the
    /// fourth rung does not exist: a session that could cycle into bypassing every check would make
    /// the flag decorative, and the flag is the record that somebody accepted what it costs.
    pub fn cycle(self, bypass_available: bool) -> Self {
        // From bypass the ladder is rejoined at the start, so the key remains a cycle rather than a
        // one-way door out of the mode the flag asked for.
        let Some(rung) = Self::LADDER.iter().position(|mode| *mode == self) else {
            return Self::Ask;
        };
        match rung + 1 == Self::LADDER.len() {
            true if bypass_available => Self::Bypass,
            true => Self::Ask,
            false => Self::LADDER[rung + 1],
        }
    }

    /// Whether a write happens without anybody being asked.
    pub fn writes_unasked(self) -> bool {
        matches!(self, Self::AcceptEdits | Self::Bypass)
    }

    /// Whether writing is refused however the person would have answered.
    pub fn refuses_writes(self) -> bool {
        self == Self::Plan
    }

    /// Whether a prompt that would promote quarantined content is worth a check first.
    ///
    /// False in one case. Bypassing draws no such prompt, so a check there is a model call over a
    /// whole slot to produce a word nobody reads, and the run pays for it. Asking for auto-vetting
    /// takes that back: the word is then what answers in the absent person's place, so it is read
    /// after all, and a word that objects is the only thing left that can keep the bytes back.
    pub fn checks_before_promoting(self, auto_vetting: bool) -> bool {
        self != Self::Bypass || auto_vetting
    }

    /// Who the trail is told released one slot's bytes, which is not always who the prompt would
    /// have asked.
    ///
    /// Three answers because a promotion happens three ways, and a record naming the wrong one is
    /// the one entry a reader cannot check. Bypassing with no screening asked for is the mode's
    /// own release: [`Confining::screened`] answers the prompt yes without drawing it, so nobody
    /// read the bytes, and [`PermissionMode::checks_before_promoting`] made no check, so nothing
    /// read them either. Where screening was asked for, a safe verdict is what answered and every
    /// other word refuses before a promotion is reached. Everywhere else the prompt is drawn, so
    /// the answer is a person's.
    ///
    /// The verdict decides nothing here without auto-vetting: off, which is the default, the word
    /// travels to the prompt and a person answers, so it is the person who is credited whatever
    /// the check said.
    pub fn released_by(self, auto_vetting: bool, verdict: Verdict) -> Endorsed {
        match (self, auto_vetting) {
            (Self::Bypass, false) => Endorsed::ByBypassing,
            (_, true) if verdict.is_safe() => Endorsed::ByASafeVerdict,
            _ => Endorsed::ByAPerson,
        }
    }

    /// What the planner is told about the mode, or `None` where there is nothing to say.
    ///
    /// Only plan mode says anything. The others change who answers a question, which is not a fact
    /// about the work and not one the planner is better for knowing: told that its writes are going
    /// through unasked, a model has been handed a reason to be bolder than the person expected.
    ///
    /// Plan mode is different because the refusal is unconditional. Without this the planner spends
    /// the session proposing writes and reading back a refusal it cannot account for, and a planner
    /// that cannot tell a policy from a mistake retries.
    pub fn instruction(self) -> Option<&'static str> {
        match self {
            Self::Plan => Some(
                "\n\nPlan mode. The user is deciding what to do, so writing is refused for this \
                 turn however they would have answered: do not call write or edit, and do not \
                 offer to. Read the code, run what you need to understand it, and answer with what \
                 you would do and why. When the plan is settled they will leave this mode and ask \
                 you to carry it out.",
            ),
            Self::Ask | Self::AcceptEdits | Self::Bypass => None,
        }
    }
}

/// Another confirmer, with the questions this mode does not put to a person already answered.
///
/// A wrapper over whatever the caller already had rather than an implementation of its own, because
/// two of the confirmer's questions are not permission questions and must still reach the person: a question
/// the planner posed asks for information rather than consent, and an interjection is the person
/// speaking unprompted. Answering either here would put words in the mouth of somebody sitting in
/// front of the session.
///
/// Wired unconditionally, with the mode deciding what it does, so there is no second branch of an
/// otherwise identical turn for a permission bug to live in.
pub struct Confining<'a, C: Confirmer> {
    inner: &'a mut C,
    mode: PermissionMode,
    auto_vetting: bool,
}

impl<'a, C: Confirmer> Confining<'a, C> {
    /// `auto_vetting` is whatever [`bravebot_core::vetting::auto`] resolved for this run, and the
    /// same value the task carries. A parameter rather than a default, because the two prompts that
    /// promote quarantined content answer yes without it: a builder call left off at one of the
    /// callers below would be a run that stopped screening and said nothing about it.
    pub fn new(inner: &'a mut C, mode: PermissionMode, auto_vetting: bool) -> Self {
        Self {
            inner,
            mode,
            auto_vetting,
        }
    }

    /// What bypassing answers where a prompt would have promoted one slot's bytes.
    ///
    /// Yes, unless somebody asked for screening. Then the check's own word is what answers in the
    /// place of the person who is not there, and a word that objects is a refusal rather than a
    /// warning drawn beside bytes on a screen nobody is reading: the alternative is promoting
    /// content a check objected to, which is what the screening was asked for to stop.
    ///
    /// Unsafe and a check that did not complete are answered alike, and that is the whole of what
    /// fail closed means here. A check can be made to fail by content that has some influence over
    /// the call, so an answer that promoted on a failure would be an answer an attacker can reach.
    fn screened(&self, verdict: Verdict) -> Decision {
        match self.auto_vetting && !verdict.is_safe() {
            true => Decision::Reject,
            false => Decision::Approve,
        }
    }
}

impl<C: Confirmer> Confirmer for Confining<'_, C> {
    /// The one question the mode may answer either way.
    ///
    /// A refusal is not the same as a person having said no, but the tool reports it the same way,
    /// and that is the right thing to tell the planner: what it needs to know is that the write did
    /// not happen and retrying will not help. Plan mode also says so in the system prompt, which is
    /// where the reason belongs.
    ///
    /// Refusing here is not what enforces plan mode, and cannot be: this is reached only where
    /// something wanted to prompt, and a write into a path the trust map covers or a rule allows
    /// wants no prompt. The write tools refuse on the mode itself, before any of that. What this
    /// arm holds is the mode's answer wherever a write prompt does get raised.
    fn confirm_write(&mut self, request: &WriteRequest) -> Decision {
        match self.mode {
            PermissionMode::AcceptEdits | PermissionMode::Bypass => Decision::Approve,
            PermissionMode::Plan => Decision::Reject,
            PermissionMode::Ask => self.inner.confirm_write(request),
        }
    }

    /// Approves without vouching for the programs and without recording anything past the session,
    /// where it approves at all. Remembering writes a
    /// standing permission into the session record, which outlives the mode: that record can be
    /// resumed in another, and the vouched list would then claim a person approved programs nobody
    /// ever showed them. A line recorded past the session is the same objection over a longer
    /// lifetime: no prompt was drawn for a key to reach, so a record saying somebody chose to
    /// remember a line they were never shown would be a standing permission nobody granted.
    ///
    /// Accepting edits does not accept runs. A write lands in a tree that `git diff` will show in
    /// full afterwards; a command runs with everything the user's shell has and leaves no diff.
    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        match self.mode {
            PermissionMode::Bypass => RunDecision::approve(),
            PermissionMode::Ask | PermissionMode::AcceptEdits | PermissionMode::Plan => {
                self.inner.confirm_run(request)
            }
        }
    }

    /// Asked in every mode but bypass, where the verdict answers where screening was asked for.
    fn confirm_read_output(&mut self, request: &OutputRequest) -> Decision {
        match self.mode {
            PermissionMode::Bypass => self.screened(request.verdict),
            PermissionMode::Ask | PermissionMode::AcceptEdits | PermissionMode::Plan => {
                self.inner.confirm_read_output(request)
            }
        }
    }

    /// Asked in every mode but bypass, as reading a command's output is.
    ///
    /// Accepting edits does not accept this: what that mode grants is writes to this tree, and
    /// this puts bytes nobody vouched for into the planner's context. Plan mode asks rather than
    /// refusing, since reading is how a plan gets written.
    ///
    /// Bypassing answers it, and is the one mode whose answer can be no: a run told to ask nobody
    /// and to screen what it promotes has the check's word and nothing else to go on.
    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        match self.mode {
            PermissionMode::Bypass => self.screened(request.verdict),
            PermissionMode::Ask | PermissionMode::AcceptEdits | PermissionMode::Plan => {
                self.inner.confirm_vetted_read(request)
            }
        }
    }

    /// Asked in every mode but bypass, including plan mode.
    ///
    /// Accepting edits does not accept fetches: the two have nothing to do with each other, and a
    /// person who said yes to writing files in this tree has said nothing about which hosts may be
    /// talked to. Plan mode asks rather than refusing, because reading a page is how a plan gets
    /// written and a fetch changes nothing here; what it does do is leave the machine, which is
    /// the person's to agree to.
    fn confirm_fetch(&mut self, request: &crate::confirm::FetchRequest) -> Decision {
        match self.mode {
            PermissionMode::Bypass => Decision::Approve,
            PermissionMode::Ask | PermissionMode::AcceptEdits | PermissionMode::Plan => {
                self.inner.confirm_fetch(request)
            }
        }
    }

    /// Asked in every mode but bypass, including plan mode and accepting edits.
    ///
    /// Accepting edits does not start servers: what that mode grants is writes to this tree, and a
    /// language server runs the ecosystem's build tooling with the user's own access, which is
    /// nearer to a run than to an edit. Plan mode asks rather than refusing, for the reason a fetch
    /// does: reading the code is how a plan gets written, and a server writes nothing here. What it
    /// does do is execute code out of the dependency tree, which stays the person's to agree to.
    fn confirm_server(&mut self, request: &crate::confirm::ServerRequest) -> Decision {
        match self.mode {
            PermissionMode::Bypass => Decision::Approve,
            PermissionMode::Ask | PermissionMode::AcceptEdits | PermissionMode::Plan => {
                self.inner.confirm_server(request)
            }
        }
    }

    /// Approves a whole plan only where every check is being bypassed, and asks in every other mode
    /// including plan mode.
    ///
    /// Accepting edits does not approve a plan. What that mode grants is the writes in one turn the
    /// person is watching, and a manifest plan is a whole run's worth of effects fixed before any of
    /// it happens: a mode meaning "stop showing me diffs" cannot also mean "run programs I have not
    /// read".
    ///
    /// Plan mode asks rather than refusing. It is not this mode's substitute and not its rival: it
    /// withholds writes for a turn so the planner can research and propose, while this proposes
    /// first and observes afterwards. Refusing here would take one mode's answer for the other's
    /// question, and a person in plan mode who wants a run planned before it touches anything is
    /// asking for exactly what this mode does.
    fn confirm_manifest(&mut self, request: &crate::confirm::ManifestRequest) -> Decision {
        match self.mode {
            PermissionMode::Bypass => Decision::Approve,
            PermissionMode::Ask | PermissionMode::AcceptEdits | PermissionMode::Plan => {
                self.inner.confirm_manifest(request)
            }
        }
    }

    /// Vouches for the file only where every check is being bypassed, which is the part of that mode
    /// that costs the most: the label on those bytes is what keeps a file's contents from being read
    /// as instructions, and this hands it over for every quarantined file the planner asks for.
    ///
    /// Screening does not reach it, and the verdict is not read here. A yes writes a rule about a
    /// path rather than promoting bytes, which is a standing decision and a larger question than the
    /// one a check read, so the answer stays the mode's however the check answered. No check is made
    /// before it either, for the reason it is made before the other two: nothing would read the word.
    fn confirm_vouch(&mut self, request: &VouchRequest) -> Decision {
        match self.mode {
            PermissionMode::Bypass => Decision::Approve,
            PermissionMode::Ask | PermissionMode::AcceptEdits | PermissionMode::Plan => {
                self.inner.confirm_vouch(request)
            }
        }
    }

    /// Asked in every mode but bypass, including accepting edits.
    ///
    /// Accepting edits does not accept this. What that mode grants is writes to this tree, and
    /// this is a disclosure off it: the planner's context goes to whoever performs inference, so
    /// a person who said yes to diffs has said nothing about their `.env` reaching a model. Plan
    /// mode asks rather than refusing, because reading is how a plan gets written and this read
    /// changes nothing here.
    ///
    /// Bypassing answers it yes, as it answers every other prompt, and screening does not reach
    /// it: a check is a model being shown the content, which is the disclosure the question is
    /// about, so asking one would perform the act it was deciding about.
    fn confirm_exposing_read(&mut self, request: &crate::confirm::ExposureRequest) -> Decision {
        match self.mode {
            PermissionMode::Bypass => Decision::Approve,
            PermissionMode::Ask | PermissionMode::AcceptEdits | PermissionMode::Plan => {
                self.inner.confirm_exposing_read(request)
            }
        }
    }

    /// Always the inner confirmer's. A question the planner posed is not a permission, and an answer
    /// invented here would be reported to the model as the user's own words.
    fn ask_user(&mut self, asking: &bravebot_core::ask::Asking) -> Vec<bravebot_core::ask::Answer> {
        self.inner.ask_user(asking)
    }

    /// Always the inner confirmer's, for the same reason: this is the person talking, not being
    /// asked.
    fn interjection(&mut self) -> Option<String> {
        self.inner.interjection()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confirm::{
        ApprovePlans, ApproveRuns, ChoosesFirst, Intent, ManifestRequest, ReadsOutput, Unattended,
        VetsContent,
    };

    fn a_write() -> WriteRequest {
        WriteRequest {
            path: "src/main.rs".to_string(),
            contents: "fn main() {}\n".to_string(),
            existing: None,
            diff: crate::diff::Diff::compute("", "fn main() {}\n"),
            intent: Intent::Create,
            untrusted: false,
            remark: None,
            credentials: Vec::new(),
        }
    }

    fn a_series() -> bravebot_core::ask::Asking {
        bravebot_core::ask::asking(&bravebot_core::ask::Series::new(vec![
            bravebot_core::ask::Question::new(
                "Cache",
                "Which cache layer?",
                vec![
                    bravebot_core::ask::Choice::new("HTTP", None),
                    bravebot_core::ask::Choice::new("Query", None),
                ],
                false,
            ),
            bravebot_core::ask::Question::new("Branch", "Which branch?", Vec::new(), false),
        ]))
    }

    fn a_plan() -> ManifestRequest {
        ManifestRequest {
            task: "tidy the notes".to_string(),
            steps: vec!["1. [act] write summary to summary.md".to_string()],
        }
    }

    fn an_output(verdict: Verdict) -> OutputRequest {
        OutputRequest {
            command: "cat notes.txt".to_string(),
            output: "a line".to_string(),
            lines: 1,
            reference: "ref:1".to_string(),
            verdict,
            reason: None,
        }
    }

    fn a_vetted_read(verdict: Verdict) -> VetRequest {
        VetRequest {
            origin: "https://example.test/page".to_string(),
            expects: "the release notes".to_string(),
            content: "a line".to_string(),
            lines: 1,
            verdict,
            reason: None,
        }
    }

    fn a_vouch(verdict: Verdict) -> VouchRequest {
        VouchRequest {
            path: "notes.txt".to_string(),
            preview: "a line".to_string(),
            truncated: false,
            verdict,
            reason: None,
        }
    }

    fn a_run() -> RunRequest {
        RunRequest::from_pipeline(
            &bravebot_core::Pipeline::new(vec![bravebot_core::Stage::new(
                "git",
                vec!["log".into()],
            )]),
            &["/usr/bin/git".into()],
            "/tmp/project",
        )
    }

    /// Asking is what a session has always done, and what it must still do unless somebody changed
    /// it: the mode nobody chose cannot be one that stops putting writes to a person.
    #[test]
    fn the_default_mode_asks_about_everything() {
        assert_eq!(PermissionMode::default(), PermissionMode::Ask);

        let mut refusing = Unattended;
        let mut confining = Confining::new(&mut refusing, PermissionMode::Ask, false);
        // The inner confirmer's answer, whatever it is, rather than one this decided.
        assert_eq!(confining.confirm_write(&a_write()), Decision::Reject);
        assert!(!confining.confirm_run(&a_run()).approved());
    }

    /// The whole of what accepting edits buys, and the whole of what it does not: a write goes
    /// through and a command still asks. A write lands where `git diff` will show it; a command runs
    /// with everything the user's shell has.
    #[test]
    fn accepting_edits_lets_writes_through_but_not_commands() {
        let mut refusing = Unattended;
        let mut confining = Confining::new(&mut refusing, PermissionMode::AcceptEdits, false);
        assert_eq!(confining.confirm_write(&a_write()), Decision::Approve);
        assert!(
            !confining.confirm_run(&a_run()).approved(),
            "accepting edits must not accept running programs"
        );
    }

    /// Plan mode refuses rather than asks, so a write does not happen even where the wrapped
    /// confirmer would have approved one. Asking is what `Ask` is for.
    #[test]
    fn plan_mode_refuses_a_write_the_person_would_have_approved() {
        let mut approving = ApproveRuns;
        let mut confining = Confining::new(&mut approving, PermissionMode::Plan, false);
        assert_eq!(confining.confirm_write(&a_write()), Decision::Reject);
    }

    /// Research is most of what planning is, and `git log` is how it is done. The prompt stays,
    /// so it is the person's answer rather than the mode's.
    #[test]
    fn plan_mode_still_lets_a_command_be_asked_about() {
        let mut approving = ApproveRuns;
        let mut confining = Confining::new(&mut approving, PermissionMode::Plan, false);
        assert!(confining.confirm_run(&a_run()).approved());
    }

    /// Every consent question answered, which is what the flag selects.
    #[test]
    fn bypassing_answers_every_permission_question() {
        let mut refusing = Unattended;
        let mut confining = Confining::new(&mut refusing, PermissionMode::Bypass, false);
        assert_eq!(confining.confirm_write(&a_write()), Decision::Approve);
        let run = confining.confirm_run(&a_run());
        assert!(run.approved());
        assert!(!run.remember, "a standing permission outlived the mode");
        assert!(
            !run.record,
            "a mode that draws no prompt recorded an answer past the session"
        );
        assert_eq!(confining.confirm_manifest(&a_plan()), Decision::Approve);
    }

    /// A run told to ask nobody and told nothing about screening gets what it asked for, and the
    /// verdict beside the bytes is not read: nothing made it, so an answer that turned on it would
    /// be an answer turning on a placeholder. The double refuses, so the yes is the mode's own.
    #[test]
    fn bypassing_promotes_quarantined_content_where_nothing_screens_it() {
        let mut refusing = Unattended;
        let mut confining = Confining::new(&mut refusing, PermissionMode::Bypass, false);
        assert_eq!(
            confining.confirm_read_output(&an_output(Verdict::Unsafe)),
            Decision::Approve
        );
        assert_eq!(
            confining.confirm_vetted_read(&a_vetted_read(Verdict::Unsafe)),
            Decision::Approve
        );
    }

    /// The point of asking for screening on a run nobody is watching: a word that objects is the
    /// only thing left that can keep a slot's bytes out of the planner, so it has to be able to
    /// refuse. A check that did not complete answers with the unsafe one, because content can reach
    /// the call that makes it and an answer that promoted on a failure is an answer an attacker can
    /// reach. Each double approves its own route, so the refusal is the mode's own.
    #[test]
    fn screening_under_bypass_refuses_what_a_check_would_not_pass() {
        for objection in [Verdict::Unsafe, Verdict::Inconclusive("the call failed")] {
            let mut reading = ReadsOutput;
            assert_eq!(
                Confining::new(&mut reading, PermissionMode::Bypass, true)
                    .confirm_read_output(&an_output(objection)),
                Decision::Reject,
                "{objection} promoted a command's output"
            );

            let mut vetting = VetsContent;
            assert_eq!(
                Confining::new(&mut vetting, PermissionMode::Bypass, true)
                    .confirm_vetted_read(&a_vetted_read(objection)),
                Decision::Reject,
                "{objection} promoted a quarantined slot"
            );
        }
    }

    /// Screening is a screen rather than a wall. A run that refused every promotion would be one
    /// nobody could use the two flags together on, and the flags are documented as composing.
    #[test]
    fn screening_under_bypass_still_promotes_what_a_check_found_nothing_in() {
        let mut refusing = Unattended;
        let mut confining = Confining::new(&mut refusing, PermissionMode::Bypass, true);
        assert_eq!(
            confining.confirm_read_output(&an_output(Verdict::Safe)),
            Decision::Approve
        );
        assert_eq!(
            confining.confirm_vetted_read(&a_vetted_read(Verdict::Safe)),
            Decision::Approve
        );
    }

    /// Vouching writes a standing rule about a path rather than promoting one slot's bytes, which is
    /// a larger question than the one a check read. Bypassing answers it as it always did, whatever
    /// a verdict about today's contents says, so screening cannot be read as having narrowed a grant
    /// it never looked at.
    #[test]
    fn screening_does_not_reach_the_vouch_offer() {
        let mut refusing = Unattended;
        let mut confining = Confining::new(&mut refusing, PermissionMode::Bypass, true);
        assert_eq!(
            confining.confirm_vouch(&a_vouch(Verdict::Unsafe)),
            Decision::Approve
        );
    }

    /// A verdict answers in an absent person's place and nowhere else. Where there is somebody to
    /// ask, a check that found nothing is advice drawn beside the bytes and the yes is still theirs:
    /// a screening flag that approved here would have turned a second model into the person.
    #[test]
    fn screening_answers_nothing_where_somebody_is_there_to_ask() {
        for asks in [
            PermissionMode::Ask,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
        ] {
            let mut refusing = Unattended;
            let mut confining = Confining::new(&mut refusing, asks, true);
            assert_eq!(
                confining.confirm_read_output(&an_output(Verdict::Safe)),
                Decision::Reject,
                "{asks:?} promoted a command's output on a verdict instead of asking"
            );
            assert_eq!(
                confining.confirm_vetted_read(&a_vetted_read(Verdict::Safe)),
                Decision::Reject,
                "{asks:?} promoted a quarantined slot on a verdict instead of asking"
            );

            // The other half of the same rule, and the doubles answer the other way so that each
            // arm's expected decision is one only the mode can have produced. A word that objects
            // is a warning drawn beside the bytes here rather than a refusal, so the person's yes
            // has to reach through.
            let mut output = ReadsOutput;
            assert_eq!(
                Confining::new(&mut output, asks, true)
                    .confirm_read_output(&an_output(Verdict::Unsafe)),
                Decision::Approve,
                "{asks:?} kept a command's output back instead of asking"
            );
            let mut vetting = VetsContent;
            assert_eq!(
                Confining::new(&mut vetting, asks, true)
                    .confirm_vetted_read(&a_vetted_read(Verdict::Unsafe)),
                Decision::Approve,
                "{asks:?} kept a quarantined slot back instead of asking"
            );
        }
    }

    /// What the check is for decides whether it is made. Every mode that draws a prompt has somebody
    /// to read the word, and bypassing with screening asked for has the word answering in their
    /// place; bypassing without it has neither, and a model call whose answer nothing reads is
    /// latency and money the run pays for nothing.
    #[test]
    fn a_check_before_promoting_is_made_wherever_its_word_is_read() {
        for asks in [
            PermissionMode::Ask,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
        ] {
            for screening in [false, true] {
                assert!(
                    asks.checks_before_promoting(screening),
                    "{asks:?} drew a prompt with no verdict beside the bytes"
                );
            }
        }
        assert!(PermissionMode::Bypass.checks_before_promoting(true));
        assert!(!PermissionMode::Bypass.checks_before_promoting(false));
    }

    /// The release nobody was asked about and no check was made about is the mode's own, and the
    /// trail has to say so: crediting a person who was never shown the bytes is the one entry a
    /// reader cannot check, and crediting a check names one that was never placed.
    ///
    /// Every mode is asked with both answers to screening and with all three verdicts, so an
    /// implementation that answered the mode's own name everywhere, or that read the verdict where
    /// nothing turned on it, is told apart from the required one rather than passing on the arm it
    /// happens to be right about.
    #[test]
    fn an_unscreened_unattended_release_is_credited_to_the_mode() {
        let verdicts = [
            Verdict::Safe,
            Verdict::Unsafe,
            Verdict::Inconclusive("the check was not made"),
        ];

        for verdict in verdicts {
            assert_eq!(
                PermissionMode::Bypass.released_by(false, verdict),
                Endorsed::ByBypassing,
                "a release with nobody shown the bytes and no check made was credited elsewhere \
                 on {verdict}"
            );
        }

        // Screening asked for: the word answers in the absent person's place, so a safe one is
        // what released the bytes. Every other word refuses before a promotion is reached, and
        // the value there reaches no trail.
        assert_eq!(
            PermissionMode::Bypass.released_by(true, Verdict::Safe),
            Endorsed::ByASafeVerdict
        );

        for asks in [
            PermissionMode::Ask,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
        ] {
            for verdict in verdicts {
                assert_eq!(
                    asks.released_by(false, verdict),
                    Endorsed::ByAPerson,
                    "{asks:?} draws the prompt, so the person who answered it was not credited \
                     on {verdict}"
                );
            }
            assert_eq!(
                asks.released_by(true, Verdict::Safe),
                Endorsed::ByASafeVerdict,
                "{asks:?} with auto-vetting on credited a person nobody asked"
            );
            assert_eq!(
                asks.released_by(true, Verdict::Unsafe),
                Endorsed::ByAPerson,
                "{asks:?} credited a check that objected with the release"
            );
        }
    }

    /// Every mode but the one that answers everything puts a plan to a person. Accepting edits
    /// accepts a write at a time, and a plan is a run's worth of them settled before anything has
    /// been read; plan mode asks rather than refusing, because planning is what somebody who chose
    /// that mode came for and refusing here would take one mode's answer for the other's question.
    ///
    /// The double approves, so a refusal would be the mode's own rather than the inner confirmer's.
    #[test]
    fn a_plan_is_put_to_a_person_in_every_mode_but_bypass() {
        for asks in [
            PermissionMode::Ask,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
        ] {
            let mut approving = ApprovePlans;
            let mut confining = Confining::new(&mut approving, asks, false);
            assert_eq!(
                confining.confirm_manifest(&a_plan()),
                Decision::Approve,
                "{asks:?} answered a plan itself instead of putting it to somebody"
            );
        }
    }

    /// Only plan mode tells the planner anything. Told that its writes are going through unasked, a
    /// model has been handed a reason to be bolder than the person expected.
    #[test]
    fn only_plan_mode_says_anything_to_the_planner() {
        assert!(PermissionMode::Plan.instruction().is_some());
        for quiet in [
            PermissionMode::Ask,
            PermissionMode::AcceptEdits,
            PermissionMode::Bypass,
        ] {
            assert!(quiet.instruction().is_none(), "{quiet:?} spoke up");
        }
    }

    /// The ladder one key walks. Three rungs without the flag, back to the start from the last: a
    /// key that stopped cycling would be a mode somebody could not leave.
    #[test]
    fn the_key_cycles_three_modes_without_the_flag() {
        let mut mode = PermissionMode::default();
        let mut seen = Vec::new();
        for _ in 0..4 {
            mode = mode.cycle(false);
            seen.push(mode);
        }
        assert_eq!(
            seen,
            vec![
                PermissionMode::AcceptEdits,
                PermissionMode::Plan,
                PermissionMode::Ask,
                PermissionMode::AcceptEdits,
            ]
        );
    }

    /// The fourth rung exists only where the flag was given. Without that, a session could cycle
    /// into bypassing every check and the flag would be decorative.
    #[test]
    fn bypass_is_only_reachable_where_the_flag_was_given() {
        let mut mode = PermissionMode::default();
        let mut seen = Vec::new();
        for _ in 0..4 {
            mode = mode.cycle(true);
            seen.push(mode);
        }
        assert_eq!(
            seen,
            vec![
                PermissionMode::AcceptEdits,
                PermissionMode::Plan,
                PermissionMode::Bypass,
                PermissionMode::Ask,
            ],
            "the ladder must come back round rather than stop at bypass"
        );

        // The rung is unreachable without it, however many times the key is pressed.
        let mut mode = PermissionMode::default();
        for _ in 0..12 {
            mode = mode.cycle(false);
            assert_ne!(mode, PermissionMode::Bypass);
        }
    }

    /// A question the planner posed asks for information rather than consent, and it must reach the
    /// person in every mode: an answer invented here would be reported to the model as theirs.
    #[test]
    fn no_mode_answers_a_question_that_is_not_a_permission() {
        for mode in [
            PermissionMode::Ask,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
            PermissionMode::Bypass,
        ] {
            let mut choosing = ChoosesFirst;
            let asked = Confining::new(&mut choosing, mode, false).ask_user(&a_series());
            assert_eq!(
                asked,
                vec![
                    bravebot_core::ask::Answer::Chosen(vec![0]),
                    bravebot_core::ask::Answer::Declined
                ],
                "{mode:?} answered a question that was not a permission"
            );
        }
    }

    /// A session started in bypass by the flag can still be cycled out of and back into it, so the
    /// key means the same thing wherever the session began.
    #[test]
    fn a_session_started_in_bypass_can_cycle_out_of_it() {
        assert_eq!(PermissionMode::Bypass.cycle(true), PermissionMode::Ask);
        // And back round to it, so nothing is one-way.
        let mut mode = PermissionMode::Bypass;
        for _ in 0..4 {
            mode = mode.cycle(true);
        }
        assert_eq!(mode, PermissionMode::Bypass);
    }
}
