//! Lending one confirmer, one reporter, one audit trail and one wallet to several runs at once.
//!
//! A turn and the delegates it spawned run at the same time and each of the four is single: one
//! person answers the questions, one screen shows the lines, one trail records the decisions, one
//! subscription pays for the requests. What they need is not a copy each but a turn each, which is
//! what this is.
//!
//! # Why a lock and not a copy
//!
//! Copying is wrong for all four. Two trails leave a hole in the record exactly over the part of
//! the turn nobody watched. Two reporters interleave half-written lines. Two confirmers put two
//! questions on one screen, and a person cannot answer either without reading both. Two wallets
//! over one batch hand out the same credential twice, which is the one thing a credential may
//! never be.
//!
//! Holding the lock for the whole of one question is deliberate rather than a cost. A person is
//! asked one thing at a time, and a delegate that wants an answer while somebody is reading
//! another delegate's diff waits for them to finish reading it.
//!
//! # What this does not do
//!
//! It carries no content and reads none. Every method here takes what it was handed, takes the
//! lock, and passes it through. Usage totals stay with the delegate until collection; labelled
//! content keeps the label it arrived with.

use crate::confirm::{
    Confirmer, Decision, OutputRequest, RunDecision, RunRequest, VetRequest, VouchRequest,
    WriteRequest,
};
use crate::report::{
    Activity, DelegateId, Delegation, Landing, Phase, Printed, Reported, Reporter, Returned, Shown,
};
use bravebot_aichat::{Subscription, SubscriptionCredential};
use bravebot_core::ask::{Answer, Asking};
use bravebot_core::event::{Event, Sink};
use bravebot_core::todo::Row;
use std::sync::{Mutex, MutexGuard};

/// One thing several runs take turns with.
///
/// Holds the borrow for as long as the turn does, and hands out [`Borrowed`] handles that take
/// the lock for one call each.
pub struct Lent<'a, T: ?Sized> {
    inner: Mutex<&'a mut T>,
}

impl<'a, T: ?Sized> Lent<'a, T> {
    pub fn new(inner: &'a mut T) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }

    /// A handle for the turn's own work.
    pub fn turn(&self) -> Borrowed<'_, 'a, T> {
        Borrowed {
            lent: self,
            from: None,
            relayed: None,
            spent: Default::default(),
            inference: Vec::new(),
        }
    }

    /// A handle for one delegate's work, which says whose every report through it is.
    pub fn delegate(&self, id: DelegateId) -> Borrowed<'_, 'a, T> {
        Borrowed {
            lent: self,
            from: Some(id),
            relayed: None,
            spent: Default::default(),
            inference: Vec::new(),
        }
    }

    /// Take the lock.
    ///
    /// A delegate that panicked leaves it poisoned, and the turn is still running and still owns
    /// the screen. What is behind the lock is a confirmer, a reporter, a trail or a wallet, and a
    /// panic leaves none of the four half written: each method here is one call that either
    /// happened or did not, and a spend is recorded before the credential it produced is handed
    /// back. So the turn carries on with what it was lent rather than dying of somebody else's
    /// failure.
    fn hold(&self) -> MutexGuard<'_, &'a mut T> {
        self.inner.lock().unwrap_or_else(|held| held.into_inner())
    }

    /// Take the lock if nothing else holds it.
    ///
    /// For the one question nobody is waiting on the answer to. See [`Borrowed::interjection`].
    fn try_hold(&self) -> Option<MutexGuard<'_, &'a mut T>> {
        match self.inner.try_lock() {
            Ok(held) => Some(held),
            Err(std::sync::TryLockError::Poisoned(held)) => Some(held.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }
}

/// The one credential store a run spends from.
///
/// A trait rather than [`Lent`] itself, because `Lent` is invariant in the lifetime of the borrow
/// it holds: a run that has to carry either the wallet it opened or the one it was handed cannot
/// name both in a single type, and every signature between the turn and its delegates would
/// otherwise state the turn's own borrow.
///
/// Spending, and nothing else. A run is told what the next credential is and never which batch it
/// came from, how many are left, or whether the wallet has been written back: those belong to the
/// run that opened it, which is the only one that closes it.
pub trait Spends: Sync {
    /// Take the lock and spend the next credential.
    fn spend_one(&self) -> Result<SubscriptionCredential, String>;
}

impl<T: Subscription + Send + ?Sized> Spends for Lent<'_, T> {
    fn spend_one(&self) -> Result<SubscriptionCredential, String> {
        self.hold().next_credential()
    }
}

/// One run's handle on the wallet.
///
/// A handle rather than a copy, for the reason a credential exists at all: it is single-use, so
/// a second wallet over the same batch hands the next run a credential the first has already
/// presented. Every handle reaches the one wallet, and a spend made through any of them is a
/// spend every other one can see (PREM-5).
pub struct Spending<'a>(&'a dyn Spends);

impl<'a> Spending<'a> {
    pub fn new(wallet: &'a dyn Spends) -> Self {
        Self(wallet)
    }
}

impl Subscription for Spending<'_> {
    fn next_credential(&mut self) -> Result<SubscriptionCredential, String> {
        self.0.spend_one()
    }
}

/// One run's handle on something lent.
///
/// Each handle retains its own progress until the parent collects it.
pub struct Borrowed<'m, 'a, T: ?Sized> {
    lent: &'m Lent<'a, T>,
    /// Whose work goes through this handle, where it is a delegate's.
    from: Option<DelegateId>,
    /// A delegate beneath `from` whose work the next call forwards, as the handle that call came
    /// through said. Only ever one of `from`'s own descendants, and spent by the call it names.
    relayed: Option<DelegateId>,
    spent: crate::outcome::Spent,
    inference: Vec<crate::timing::Interval>,
}

impl<T: Sink + ?Sized> Sink for Borrowed<'_, '_, T> {
    /// Both under one lock, so a record and the run it belongs to cannot be separated by another
    /// run recording in between. The same reason the reports below announce whose they are.
    fn emit(&mut self, event: Event) {
        let whose = self.whose();
        let mut held = self.lent.hold();
        held.recording_for(whose);
        held.emit(event);
    }

    /// Kept for one call only where it names a delegate beneath this handle's own, so a handle
    /// for a delegate cannot be talked into recording as the turn or as a sibling. A delegate's
    /// own turn lends this handle onward and hands its own work a handle for the turn, which says
    /// `None` here and so records as this delegate; a delegate that turn spawned says its own
    /// number, and the trail names it rather than the delegate above it.
    fn recording_for(&mut self, delegate: Option<DelegateId>) {
        self.relay(delegate);
    }
}

/// Forward one report, saying whose it is first.
///
/// Both under one lock, so a line and the run it belongs to cannot be separated by another run
/// reporting in between.
macro_rules! reports {
    ($( fn $name:ident(&mut self $(, $arg:ident: $ty:ty)* $(,)?); )*) => {
        $(
            fn $name(&mut self $(, $arg: $ty)*) {
                let whose = self.whose();
                let mut held = self.lent.hold();
                held.reporting_for(whose);
                held.$name($($arg),*);
            }
        )*
    };
}

impl<T: Reporter + ?Sized> Reporter for Borrowed<'_, '_, T> {
    fn inference_interval(&mut self, interval: crate::timing::Interval) {
        if self.from.is_some() {
            self.inference.push(interval);
        }
        let whose = self.whose();
        let mut held = self.lent.hold();
        held.reporting_for(whose);
        held.inference_interval(interval);
    }

    fn prompt_recorded(&mut self, at: usize) {
        // A delegate has its own conversation; its offsets do not describe the parent turn.
        if self.from.is_none() {
            self.lent.hold().prompt_recorded(at);
        }
    }

    reports! {
        fn todos(&mut self, rows: Vec<Row>);
        fn output_tokens(&mut self, written: u64);
        fn phase(&mut self, phase: Phase);
        fn narration(&mut self, text: String);
        fn streaming(&mut self, text: String);
        fn notice(&mut self, text: String);
        fn quarantined(&mut self, shown: Shown);
        fn printed(&mut self, output: Printed);
        fn returned(&mut self, returned: Returned);
        fn landed(&mut self, landing: Landing);
        fn tool_started(&mut self, activity: Activity);
        fn tool_finished(&mut self, activity: Activity);
        fn check_started(&mut self, lines: usize);
        fn check_finished(&mut self);
        fn interjected(&mut self, said: String);
        fn delegate_started(&mut self, delegation: Delegation);
        fn delegate_waiting(&mut self, delegate: DelegateId);
    }

    /// Retain delegate totals until collection. Forwarding them would replace the parent's
    /// cumulative total with one delegate's smaller total.
    fn spent(&mut self, spent: crate::outcome::Spent) {
        self.spent = spent;
        if self.from.is_some() {
            return;
        }
        let mut held = self.lent.hold();
        held.reporting_for(None);
        held.spent(spent);
    }

    /// Not through the macro: whose report this is was settled when the handle was made, and a
    /// delegate finishing is the turn's news rather than the delegate's own.
    fn delegate_finished(
        &mut self,
        delegate: DelegateId,
        note: String,
        failed: bool,
        reported: Option<Reported>,
    ) {
        let mut held = self.lent.hold();
        held.reporting_for(None);
        held.delegate_finished(delegate, note, failed, reported);
    }

    /// Kept for one call only where it names a delegate beneath this handle's own, so a handle
    /// for a delegate cannot be talked into reporting as the turn or as a sibling. See
    /// [`Borrowed::recording_for`](Sink::recording_for).
    fn reporting_for(&mut self, delegate: Option<DelegateId>) {
        self.relay(delegate);
    }
}

impl<T: Confirmer + ?Sized> Confirmer for Borrowed<'_, '_, T> {
    fn confirm_write(&mut self, request: &WriteRequest) -> Decision {
        self.lent.hold().confirm_write(request)
    }

    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        self.lent.hold().confirm_run(request)
    }

    fn confirm_read_output(&mut self, request: &OutputRequest) -> Decision {
        self.lent.hold().confirm_read_output(request)
    }

    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        self.lent.hold().confirm_vetted_read(request)
    }

    fn confirm_fetch(&mut self, request: &crate::confirm::FetchRequest) -> Decision {
        self.lent.hold().confirm_fetch(request)
    }

    fn confirm_vouch(&mut self, request: &VouchRequest) -> Decision {
        self.lent.hold().confirm_vouch(request)
    }

    fn confirm_exposing_read(&mut self, request: &crate::confirm::ExposureRequest) -> Decision {
        self.lent.hold().confirm_exposing_read(request)
    }

    fn confirm_tool_list(
        &mut self,
        request: &crate::confirm::ToolListRequest,
    ) -> crate::confirm::Decision {
        self.lent.hold().confirm_tool_list(request)
    }

    fn confirm_mcp_call(
        &mut self,
        request: &crate::confirm::McpCallRequest,
    ) -> crate::confirm::CallDecision {
        self.lent.hold().confirm_mcp_call(request)
    }

    fn confirm_server(&mut self, request: &crate::confirm::ServerRequest) -> Decision {
        self.lent.hold().confirm_server(request)
    }

    fn confirm_manifest(&mut self, request: &crate::confirm::ManifestRequest) -> Decision {
        self.lent.hold().confirm_manifest(request)
    }

    fn ask_user(&mut self, asking: &Asking) -> Vec<Answer> {
        self.lent.hold().ask_user(asking)
    }

    /// The one method here nobody is waiting on, so it never waits.
    ///
    /// Everything else is a question, and a question is asked because a run has stopped until it
    /// is answered. This is a poll: the turn asks between rounds whether anything was typed. With
    /// somebody part way through answering a delegate's question the lock is held for as long as
    /// they take to read it, and a turn that blocked here would stop for that long over a
    /// question it did not ask. What was typed keeps until the next poll.
    fn interjection(&mut self) -> Option<String> {
        self.lent.try_hold()?.interjection()
    }
}

impl<T: ?Sized> Borrowed<'_, '_, T> {
    /// Remember whose the next call is, where the handle it came through named one of this
    /// handle's descendants, and forget any earlier one either way.
    ///
    /// The numbers are the kernel's, so a descendant's is this handle's own with more after it
    /// and nothing a model wrote reaches the comparison.
    fn relay(&mut self, delegate: Option<DelegateId>) {
        self.relayed = delegate.filter(|id| self.from.is_some_and(|from| id.is_beneath(from)));
    }

    /// Whose the call being forwarded is: the descendant it was relayed for, or this handle's own.
    fn whose(&mut self) -> Option<DelegateId> {
        self.relayed.take().or(self.from)
    }

    /// Keep request intervals available on successful, failed and cancelled returns alike.
    pub fn take_inference(&mut self) -> Vec<crate::timing::Interval> {
        std::mem::take(&mut self.inference)
    }

    /// Keep completed usage available when a delegate returns an error without an outcome.
    pub fn last_spent(&self) -> crate::outcome::Spent {
        self.spent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::Spent;
    use crate::report::RecordingReporter;

    fn a_total(tokens: u64) -> Spent {
        Spent {
            tokens,
            ..Default::default()
        }
    }

    /// A delegate must not overwrite parent progress while cancellation is still possible.
    #[test]
    fn what_a_delegate_has_spent_is_not_reported_as_what_the_turn_has() {
        let mut recording = RecordingReporter::default();
        {
            let lent = Lent::new(&mut recording);
            let mut turn = lent.turn();
            let mut delegate = lent.delegate(DelegateId::nth(1));
            turn.spent(a_total(1_000));
            delegate.spent(a_total(5));
            delegate.spent(a_total(10));
            turn.spent(a_total(1_200));
        }
        assert_eq!(
            recording.spent,
            vec![a_total(1_000), a_total(1_200)],
            "a delegate's own total was reported as the turn's"
        );
    }

    /// PREM-5, over the wallet a run actually spends rather than a stand-in for one: two runs
    /// holding handles on one wallet are never offered the same credential.
    ///
    /// The turn and its delegates are the two runs, and a credential is single-use. What makes
    /// that hold is the lock and the single wallet behind it: a spend is recorded in memory until
    /// the wallet is written back (PREM-6), so two wallets over one batch (a copy each, or a
    /// second read of the same file) would each hand out the first unspent credential they can
    /// see, which is the same one.
    #[test]
    fn two_runs_holding_one_wallet_are_never_offered_the_same_credential() {
        use bravebot_aichat::Subscription;

        let batch = bravebot_skus::StoredCredentials {
            order_id: "order".to_string(),
            environment: bravebot_skus::Environment::Production,
            item_id: "item".to_string(),
            issuer: "brave.com?sku=brave-leo-premium".to_string(),
            // Two, so running out is not what makes the second answer differ from the first, and
            // real ones, since a credential that cannot be presented never reaches a cookie.
            credentials: std::iter::repeat_with(|| bravebot_skus::store::Credential {
                unblinded: bravebot_skus::device::test_credential(),
                valid_from: "2000-01-01T00:00:00".to_string(),
                valid_to: "2999-01-01T00:00:00".to_string(),
                spent: false,
                rfc: true,
            })
            .take(2)
            .collect(),
        };

        let mut wallet = crate::ImportedSubscription::detached(batch);
        let lent = Lent::new(&mut wallet);
        let turns = Spending::new(&lent)
            .next_credential()
            .expect("the turn spends one");
        let delegates = Spending::new(&lent)
            .next_credential()
            .expect("the delegate spends the next");

        assert_ne!(
            turns.cookie_value, delegates.cookie_value,
            "a credential the turn had already presented was offered to a delegate"
        );
    }

    /// Nested delegates cannot replace the parent prompt's position with their own offset.
    #[test]
    fn only_the_parent_reports_its_prompt_position() {
        let mut recording = RecordingReporter::default();
        {
            let lent = Lent::new(&mut recording);
            lent.turn().prompt_recorded(7);
            let mut delegate = lent.delegate(DelegateId::nth(1));
            let nested = Lent::new(&mut delegate);
            nested.turn().prompt_recorded(2);
        }
        assert_eq!(recording.prompts, [7]);
    }

    fn a_gate(gate: &'static str) -> Event {
        Event::GatePassed {
            gate,
            detail: String::new(),
        }
    }

    fn named(sink: &bravebot_core::event::RecordingSink) -> Vec<Option<String>> {
        sink.recorded()
            .map(|(from, _)| from.map(|id| id.to_string()))
            .collect()
    }

    fn d(path: &[u32]) -> DelegateId {
        path[1..].iter().fold(DelegateId::nth(path[0]), |at, &n| {
            at.child(n).expect("within the depth")
        })
    }

    /// A delegate's own turn lends the handle it was given onward, so every record its delegates
    /// make passes through two handles, and the trail has to name the run that took the decision
    /// rather than the delegate above it. Otherwise a grandchild's approvals read as its parent's,
    /// and two grandchildren of one delegate read as one run.
    #[test]
    fn a_nested_delegates_records_name_it_rather_than_the_delegate_above_it() {
        let mut sink = bravebot_core::event::RecordingSink::new();
        {
            let lent = Lent::new(&mut sink);
            lent.turn().emit(a_gate("the turn's"));
            let mut first = lent.delegate(d(&[1]));
            {
                let within = Lent::new(&mut first);
                within.turn().emit(a_gate("the first delegate's"));
                let mut nested = within.delegate(d(&[1, 2]));
                nested.emit(a_gate("its second delegate's"));
                {
                    let deeper = Lent::new(&mut nested);
                    deeper
                        .delegate(d(&[1, 2, 1]))
                        .emit(a_gate("that one's first"));
                    deeper.turn().emit(a_gate("its second delegate's again"));
                }
                within.turn().emit(a_gate("the first delegate's again"));
            }
        }
        let expected = ["", "d1", "d1.2", "d1.2.1", "d1.2", "d1"];
        assert_eq!(
            named(&sink),
            expected
                .iter()
                .map(|name| (!name.is_empty()).then(|| name.to_string()))
                .collect::<Vec<_>>()
        );
    }

    /// The relay takes only a number beneath the handle's own, and only for the next call. A
    /// handle that could be told it was the turn, itself, or a sibling would let one run's records
    /// be written down as another's, which is the whole of what attribution is for.
    #[test]
    fn a_handle_relays_only_its_own_descendants_and_only_once() {
        let mut sink = bravebot_core::event::RecordingSink::new();
        {
            let lent = Lent::new(&mut sink);
            let mut turn = lent.turn();
            turn.recording_for(Some(d(&[1])));
            turn.emit(a_gate("the turn claimed by a delegate"));

            let mut first = lent.delegate(d(&[1]));
            for claimed in [None, Some(d(&[1])), Some(d(&[2])), Some(d(&[2, 1]))] {
                first.recording_for(claimed);
                first.emit(a_gate("a claim that is not a descendant"));
            }
            first.recording_for(Some(d(&[1, 3])));
            first.emit(a_gate("a descendant's"));
            first.emit(a_gate("the delegate's own after it"));
        }
        let expected = [
            None,
            Some("d1"),
            Some("d1"),
            Some("d1"),
            Some("d1"),
            Some("d1.3"),
            Some("d1"),
        ];
        assert_eq!(
            named(&sink),
            expected
                .iter()
                .map(|name| name.map(str::to_string))
                .collect::<Vec<_>>()
        );
    }

    /// Lines, and what each was attributed to when it arrived.
    #[derive(Default)]
    struct Lines {
        attributed_to: Option<DelegateId>,
        lines: Vec<(Option<String>, String)>,
        requests: usize,
    }

    impl Reporter for Lines {
        fn reporting_for(&mut self, delegate: Option<DelegateId>) {
            self.attributed_to = delegate;
        }

        fn narration(&mut self, text: String) {
            self.lines
                .push((self.attributed_to.map(|id| id.to_string()), text));
        }

        fn inference_interval(&mut self, _interval: crate::timing::Interval) {
            self.requests += 1;
        }

        fn todos(&mut self, _rows: Vec<bravebot_core::todo::Row>) {}
    }

    /// A screen puts a line under the block of the run whose it is, by the number it was told,
    /// so a grandchild's lines have to arrive under the grandchild's number or they land in its
    /// parent's block.
    #[test]
    fn a_nested_delegates_lines_are_reported_as_its_own() {
        let mut screen = Lines::default();
        {
            let lent = Lent::new(&mut screen);
            let mut first = lent.delegate(d(&[1]));
            let within = Lent::new(&mut first);
            within.turn().narration("the first delegate's".to_string());
            within
                .delegate(d(&[1, 1]))
                .narration("its first delegate's".to_string());
            within
                .turn()
                .narration("the first delegate's again".to_string());
        }
        assert_eq!(
            screen.lines,
            [
                (Some("d1".to_string()), "the first delegate's".to_string()),
                (Some("d1.1".to_string()), "its first delegate's".to_string()),
                (
                    Some("d1".to_string()),
                    "the first delegate's again".to_string()
                ),
            ]
        );
    }

    /// The turn above a delegate counts the requests made inside its wait on that delegate, and a
    /// request a grandchild made is one of them: the time went on the person's model whichever run
    /// asked. Kept by every handle on the way up, not only the one it was made through.
    #[test]
    fn a_nested_delegates_requests_reach_every_delegate_above_it() {
        let interval = crate::timing::Interval::since(std::time::Instant::now());
        let mut screen = Lines::default();
        let (kept_by_first, kept_by_nested) = {
            let lent = Lent::new(&mut screen);
            let mut first = lent.delegate(d(&[1]));
            let kept_by_nested = {
                let within = Lent::new(&mut first);
                let mut nested = within.delegate(d(&[1, 1]));
                nested.inference_interval(interval);
                within.turn().inference_interval(interval);
                nested.take_inference().len()
            };
            (first.take_inference().len(), kept_by_nested)
        };
        assert_eq!(
            kept_by_nested, 1,
            "the nested delegate lost its own request"
        );
        assert_eq!(
            kept_by_first, 2,
            "the delegate above lost its own request or its delegate's"
        );
        assert_eq!(
            screen.requests, 2,
            "the screen was not told of both requests"
        );
    }
}
