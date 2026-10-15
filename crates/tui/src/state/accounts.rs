//! What a session has spent and how full its context is.

use super::*;

/// How the context currently stands in a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Occupancy {
    /// No request has been measured yet.
    Unmeasured,
    /// Compaction shortened the conversation and the next turn has not measured yet.
    ///
    /// `won_back` is what the compaction gave back, in tokens, and `budget` the budget it was
    /// compacted at. The pair outlives a budget adopted later, because what a compaction won back
    /// is a fact about the exchange it shortened rather than about the window in force now. Zero
    /// where there is no figure to state.
    Compacted { won_back: u64, budget: u64 },
    /// Measured token count against the budget, and whether the budget was guessed.
    Measured {
        used: u64,
        budget: u64,
        guessed: bool,
    },
}

/// The sections `/context` divides a request into, as indexes into the byte counts.
const CONTEXT_SYSTEM: usize = 0;
const CONTEXT_INSTRUCTIONS: usize = 1;
const CONTEXT_SKILLS: usize = 2;
const CONTEXT_TOOLS: usize = 3;
const CONTEXT_TYPED: usize = 4;
const CONTEXT_PLANNER: usize = 5;
const CONTEXT_RESULTS: usize = 6;
const CONTEXT_OTHER: usize = 7;

/// Divide `total` in proportion to `weights`, in whole numbers that add up to exactly `total`.
///
/// Each part is rounded down and what is left over goes one apiece to the parts with the largest
/// remainders, so the shares of a measured count always sum to it. All zeros where the weights
/// are, since nothing says what the total is made of.
pub(crate) fn apportion(total: u64, weights: &[u64]) -> Vec<u64> {
    let sum: u128 = weights.iter().map(|weight| u128::from(*weight)).sum();
    if sum == 0 {
        return vec![0; weights.len()];
    }
    let mut shares: Vec<u64> = weights
        .iter()
        .map(|weight| (u128::from(total) * u128::from(*weight) / sum) as u64)
        .collect();
    let mut left = total - shares.iter().sum::<u64>();
    let mut by_remainder: Vec<usize> = (0..weights.len()).collect();
    by_remainder.sort_by_key(|index| {
        std::cmp::Reverse(u128::from(total) * u128::from(weights[*index]) % sum)
    });
    for index in by_remainder {
        if left == 0 {
            break;
        }
        shares[index] += 1;
        left -= 1;
    }
    shares
}

impl Occupancy {
    /// How full the context is, as a percentage, or `None` where nothing has been measured.
    pub fn percent(&self) -> Option<u64> {
        match *self {
            Occupancy::Measured { used, budget, .. } if used > 0 && budget > 0 => {
                Some((used.saturating_mul(100) / budget).min(100))
            }
            _ => None,
        }
    }

    /// How much room a compaction won back, as a percentage of the budget, or `None` where there
    /// is no figure to state.
    pub fn won_back(&self) -> Option<u64> {
        match *self {
            Occupancy::Compacted { won_back, budget } if won_back > 0 && budget > 0 => {
                Some((won_back.saturating_mul(100) / budget).min(100))
            }
            _ => None,
        }
    }
}

impl Session {
    /// Take on what an earlier session spent.
    ///
    /// The counter answers "what has this cost me", and that answer does not become smaller
    /// because the process restarted. Set rather than added to: this is a session being picked
    /// up, not a second one being merged into it.
    pub fn restore_spend(&mut self, tokens: u64, by_turn: std::collections::BTreeMap<usize, u64>) {
        self.tokens = tokens;
        self.spend = by_turn;
    }

    /// Take on the model an earlier session's record says answered.
    ///
    /// Separate from [`Session::served`] because that records a request made in this process, with
    /// the tier it ran on, and nothing has been asked yet.
    pub fn restore_model(&mut self, model: Option<String>) {
        self.recorded_model = model;
    }

    /// Take on what a record says about what answered and what it cost: the spend, the timing and
    /// the model, which are the figures a save writes back whole.
    pub fn restore_accounts(&mut self, record: &bravebot_session::sessions::Record) {
        self.restore_spend(record.tokens, record.spend.clone());
        self.restore_timing(record.timing.clone());
        self.restore_model(record.model.clone());
    }

    /// Take on how an earlier session's turns spent their time.
    ///
    /// Separate from [`Session::restore_spend`] because a record written before timing was kept has
    /// the one and not the other, and a resume must not have to choose between restoring both or
    /// neither.
    pub fn restore_timing(
        &mut self,
        by_turn: std::collections::BTreeMap<usize, bravebot_agent::timing::Timing>,
    ) {
        self.timing = by_turn;
    }

    /// What each turn cost, by turn number.
    ///
    /// Read for writing the session down, for the per-turn block an export carries, and for what
    /// [`Session::report_spend`] draws.
    pub fn spend_by_turn(&self) -> &std::collections::BTreeMap<usize, u64> {
        &self.spend
    }

    /// Where each turn's wall clock went, by turn number, for writing the session down.
    pub fn timing_by_turn(
        &self,
    ) -> &std::collections::BTreeMap<usize, bravebot_agent::timing::Timing> {
        &self.timing
    }

    /// Every turn's timing added together.
    pub fn timing_total(&self) -> bravebot_agent::timing::Timing {
        let mut total = bravebot_agent::timing::Timing::default();
        for timing in self.timing.values() {
            total.add(*timing);
        }
        total
    }

    /// Record how the turn just finished divided its time up.
    ///
    /// Beside [`Session::complete`] rather than inside it, in the same way the context measurement
    /// is: both are facts the worker brings back about the turn, and neither is knowable from the
    /// transcript. The wall figure is not taken from here, because this side has the better one.
    ///
    /// Added to rather than set, for the reason the token breakdown is: a turn that compacted part
    /// way through has already charged that wait to the same number.
    pub fn spent_time(&mut self, timing: bravebot_agent::timing::Timing) {
        let entry = self.timing.entry(self.turns).or_default();
        entry.inference_ms += timing.inference_ms;
        entry.tools_ms += timing.tools_ms;
        entry.stalled_ms += timing.stalled_ms;
    }

    /// Cumulative usage from the worker, retained until this turn ends.
    pub fn progressed(&mut self, spent: bravebot_agent::Spent) {
        self.progress = spent;
    }

    pub(super) fn charge_progress(&mut self) -> u64 {
        let spent = std::mem::take(&mut self.progress);
        self.tokens += spent.tokens;
        *self.spend.entry(self.turns).or_default() += spent.tokens;
        self.spent_time(spent.timing);
        self.served_from_cache(
            spent.cached,
            spent.tokens.saturating_sub(spent.output_tokens),
        );
        spent.tokens
    }

    /// Record how full the context is, against the budget it is compacted at.
    pub fn measured(&mut self, used: u64, budget: u64, guessed: bool) {
        if used == 0 {
            self.occupancy = Occupancy::Unmeasured;
        } else {
            self.occupancy = Occupancy::Measured {
                used,
                budget,
                guessed,
            };
        }
    }

    /// Record that compaction shortened the conversation underneath the session, and by how much.
    ///
    /// `won_back` is in tokens, against the budget the conversation is compacted at. Zero stands
    /// for no figure rather than for a compaction that gave nothing back: a summary that saved
    /// nothing is worth no more to a reader than a server that reported nothing.
    pub fn compacted(&mut self, won_back: u64, budget: u64) {
        self.occupancy = Occupancy::Compacted { won_back, budget };
    }

    /// The occupancy state of the session context.
    pub fn occupancy(&self) -> Occupancy {
        self.occupancy
    }

    /// Update the context budget and guessed status against which occupancy is calculated,
    /// preserving the measured token count.
    pub fn update_budget(&mut self, budget: u64, guessed: bool) {
        if let Occupancy::Measured { used, .. } = self.occupancy {
            self.occupancy = Occupancy::Measured {
                used,
                budget,
                guessed,
            };
        }
    }

    /// Record what the last turn asked for, what the server answered with, and which tier it ran on.
    ///
    /// `comparable` says whether the two names are drawn from one roster, and so whether a difference
    /// between them means anything. The caller knows which backend answered; this cannot tell.
    pub fn served(
        &mut self,
        requested: impl Into<String>,
        served: impl Into<String>,
        premium: bool,
        comparable: bool,
    ) {
        self.served = Some((requested.into(), served.into()));
        self.premium = Some(premium);
        self.served_names_are_comparable = comparable;
    }

    /// Whether the last turn spent a subscription credential, or `None` before one has run.
    pub fn premium(&self) -> Option<bool> {
        self.premium
    }

    /// Record how much of the turn just finished the backend did not have to read.
    pub fn served_from_cache(
        &mut self,
        cached: bravebot_aichat::protocol::Cached,
        prompt_tokens: u64,
    ) {
        self.cached = Some(cached);
        self.cached_prompt_tokens = Some(prompt_tokens);
    }

    /// Put back what an earlier turn read, or forget the figure with `None`.
    ///
    /// The figure is the last turn's, so anything that changes which turn that is has to say so:
    /// undoing a turn puts back the one before it.
    pub fn restore_cache(
        &mut self,
        cached: Option<bravebot_aichat::protocol::Cached>,
        prompt_tokens: Option<u64>,
    ) {
        self.cached = cached;
        self.cached_prompt_tokens = prompt_tokens;
    }

    /// What the last turn read out of the cache and wrote into it, or `None` before one has run.
    pub fn cached(&self) -> Option<bravebot_aichat::protocol::Cached> {
        self.cached
    }

    /// Prompt tokens behind the last turn's cache figures.
    pub fn cached_prompt_tokens(&self) -> Option<u64> {
        self.cached_prompt_tokens
    }

    /// The model the server last reported using, or `None` before any turn has run.
    ///
    /// In a resumed session that has not yet had a turn, the model its record names.
    pub fn served_model(&self) -> Option<&str> {
        match &self.served {
            Some((_, served)) => Some(served.as_str()),
            None => self.recorded_model.as_deref(),
        }
    }

    /// The model the last turn asked for, where the server answered with something else.
    ///
    /// A turn always asks for a name: the one picked with `/model` where there is one, and the
    /// configured default otherwise, which is why the recorded half is not optional. Making it so
    /// left every session running its configuration's model unable to report a substitution at all.
    ///
    /// `None` where they agree, so a caller has nothing to report on the ordinary path. Compared
    /// exactly, which works because both names come from the same roster: what was asked for is a
    /// name off a list the endpoint gave, and what answered is a name from the same list.
    ///
    /// Also `None` where the two were never comparable. A request may name an opaque handle instead
    /// of a model, and a handle standing for a different name is the indirection working rather than a
    /// substitution, so comparing them would put a warning on every turn. Whether they are comparable
    /// is recorded by whoever knew which backend answered, rather than guessed at from the spelling.
    ///
    /// `None` for the automatic entry as well. That name asks the server to choose a model per
    /// request, so a concrete one coming back is the entry doing its job rather than something
    /// served in place of what was asked for.
    pub fn substituted_model(&self) -> Option<&str> {
        let (requested, served) = self.served.as_ref()?;
        if !self.served_names_are_comparable || requested == bravebot_config::DEFAULT_MODEL {
            return None;
        }
        (requested != served).then_some(requested.as_str())
    }

    /// How full the context is, as a percentage, or `None` where nothing has been measured.
    ///
    /// Capped at a hundred rather than allowed past it. The budget is a guess at a window nobody
    /// reports, so a request larger than it is a session that will be compacted next round, not a
    /// context that is a hundred and forty per cent full.
    pub fn fullness(&self) -> Option<u64> {
        self.occupancy.percent()
    }

    /// How much room the last compaction won back, as a percentage of the budget, or `None`
    /// where there is no figure to state.
    pub fn won_back(&self) -> Option<u64> {
        self.occupancy.won_back()
    }

    /// Tokens the turn in flight has spent so far, which are charged to the session when it ends.
    pub fn running_tokens(&self) -> u64 {
        if self.a_turn_is_running() {
            self.progress.tokens
        } else {
            0
        }
    }

    /// Tokens the session has spent, the figure `/cost` opens with: what earlier turns were charged
    /// and what the turn in flight has spent so far.
    pub fn spent_tokens(&self) -> u64 {
        self.tokens + self.running_tokens()
    }

    /// Put what each turn has spent in the transcript.
    ///
    /// What `/cost` answers, and the question the session total cannot: a total tells twenty even
    /// turns and one turn that ran away apart not at all, and those want different fixes. The
    /// share is beside each figure because that is the comparison a reader would otherwise do in
    /// their head, one row at a time.
    ///
    /// Tokens rather than money, because nothing here knows what a token is charged at. No model
    /// listing carries a price, and a prompt the service answered out of its own cache is billed
    /// at a fraction of a fresh one while the record keeps no cache split per turn, so a figure in
    /// money would be composed here rather than measured.
    pub fn report_spend(&mut self) {
        // A turn still running is charged when it ends, so what it has spent so far is added here:
        // asked mid-turn (CMD-8), the count of turns already includes it.
        let mut spend = self.spend.clone();
        let running = self.running_tokens();
        if running > 0 {
            *spend.entry(self.turns).or_default() += running;
        }
        let total = self.spent_tokens();
        let mut lines = vec![crate::status::Line::new(
            t!(status_this_session),
            if total == 0 && spend.is_empty() {
                t!(cost_nothing_spent).to_string()
            } else {
                format!(
                    "{} · {}",
                    t!(count_turns, count = self.turns),
                    crate::status::tokens(total)
                )
            },
        )];

        for (turn, spent) in &spend {
            let label = match turn {
                0 => t!(cost_before_the_first_turn).to_string(),
                number => t!(cost_turn, number = number),
            };
            let line = crate::status::Line::new(&label, crate::status::tokens(*spent));
            lines.push(match total {
                0 => line,
                total => line.with_note(t!(cost_share, percent = spent * 100 / total)),
            });
        }

        // What the total holds that the rows do not account for. A record written before turns
        // were charged separately keeps the whole of it here, and a session resumed from one keeps
        // the part it spent before the resume, so an empty breakdown is the far end of this case
        // rather than a case of its own. Left out, the rows would read as an account of the total
        // that quietly does not add up to it.
        let unattributed = total.saturating_sub(spend.values().sum());
        if unattributed > 0 {
            lines.push(
                crate::status::Line::new("", crate::status::tokens(unattributed))
                    .with_note(t!(cost_unattributed)),
            );
        }

        self.report(crate::status::Report { lines });
    }

    /// Put what the last request was made of in the transcript, by category (`/context`).
    ///
    /// Read from the view of the request the driver built and the count the server reported for
    /// it, and never from the transcript. Each section is a length in bytes of something the
    /// driver composed or carried, scaled to the measured total, so no row is chosen by reading
    /// what a result says and the section names are fixed words.
    pub fn report_context(&mut self) {
        let used = match self.occupancy {
            Occupancy::Measured { used, budget, .. } if budget > 0 => used,
            Occupancy::Compacted { .. } => {
                self.note(t!(context_compacted));
                return;
            }
            _ => {
                self.note(t!(context_not_measured));
                return;
            }
        };
        let Some(view) = self.last_request.as_ref() else {
            self.note(t!(context_no_request));
            return;
        };

        let mut bytes = [0u64; 8];
        bytes[CONTEXT_TOOLS] = view.tools_bytes;
        for span in &view.spans {
            use bravebot_agent::request_view::Provenance;
            let section = match (span.role, &span.provenance) {
                ("system", Provenance::TrustedFile(_)) => CONTEXT_INSTRUCTIONS,
                ("system", Provenance::Trusted("skill list")) => CONTEXT_SKILLS,
                ("system", _) => CONTEXT_SYSTEM,
                ("user", Provenance::Typed | Provenance::TypedWithFiles) => CONTEXT_TYPED,
                ("assistant", _) => CONTEXT_PLANNER,
                ("tool", _) => CONTEXT_RESULTS,
                _ => CONTEXT_OTHER,
            };
            bytes[section] += span.text.len() as u64;
        }
        let shares = apportion(used, &bytes);
        let names = [
            t!(context_section_system),
            t!(context_section_instructions),
            t!(context_section_skills),
            t!(context_section_tools),
            t!(context_section_typed),
            t!(context_section_planner),
            t!(context_section_results),
            t!(context_section_other),
        ];

        let mut lines = vec![crate::status::Line::new(
            t!(context_total),
            format!(
                "{} · {}",
                crate::status::tokens(used),
                crate::render::context_reading(self)
            ),
        )];
        for (name, share) in names.iter().zip(shares) {
            if share == 0 {
                continue;
            }
            lines.push(
                crate::status::Line::new(name, crate::status::tokens(share))
                    .with_note(t!(cost_share, percent = share * 100 / used)),
            );
        }
        self.report(crate::status::Report { lines });
        self.note(t!(context_approximate));
    }
}
