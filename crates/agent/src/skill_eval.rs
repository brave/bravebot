//! The arithmetic of a skill evaluation: how the grades of a run become its score, how the runs of a
//! case become the case's score, and how the two arms are set side by side. Also the saved case
//! file a person writes, parsed into the shape the runner reads.
//!
//! Every input to the arithmetic is a pass or a fail and a weight, which the driver settled from
//! its own record of a run or from a verdict the gate released. Nothing in this module reads text
//! to decide anything, so no score can depend on what a reply, a skill or a case file said.

use serde::Deserialize;

use crate::skills::Skill;

/// The directory beside a skill's `SKILL.md` that holds its saved cases, one JSON file each.
pub const CASES_DIRECTORY: &str = "evals";

/// The saved cases of one skill, and what was not accepted.
#[derive(Debug, Default)]
pub struct Loaded {
    /// Each accepted case with the name of the file it came from, in the order of the names.
    pub cases: Vec<(String, Case)>,
    /// Files that were read and refused, each with why. The names are of files the catalogue
    /// listed, which only trusted files are.
    pub refused: Vec<(String, CaseProblem)>,
    /// How many files beside the skill were left out of its listing by a rule or by the trust map.
    /// Counted, never named (SKILL-6), and never read.
    pub not_read: usize,
}

/// The saved cases of a skill.
///
/// Only files the catalogue listed are read, and each is read through the same gate as the skill
/// itself (SKILL-3 for the user's own, SKILL-4 for a project's), so a case file nobody vouched for
/// is counted in [`Loaded::not_read`] and its bytes are never held. The file names that decide which
/// files are cases are ones the catalogue listed after those checks.
pub fn load_cases<S: bravebot_core::event::Sink>(
    skill: &Skill,
    workspace: &crate::workspace::Workspace,
    trust: &bravebot_core::trust::TrustStore,
    permissions: &bravebot_core::permissions::Permissions,
    sink: &mut S,
) -> Loaded {
    let prefix = format!("{CASES_DIRECTORY}/");
    let mut loaded = Loaded {
        not_read: crate::skills::unlisted_beside(skill, workspace),
        ..Loaded::default()
    };
    for name in skill
        .files
        .iter()
        .filter(|name| name.starts_with(&prefix) && name.ends_with(".json"))
    {
        let Some(text) = crate::skills::read_beside(
            skill,
            name,
            workspace,
            trust.clone(),
            permissions.clone(),
            sink,
        ) else {
            loaded.not_read += 1;
            continue;
        };
        match parse_case(&text) {
            Ok(case) => loaded.cases.push((name.clone(), case)),
            Err(problem) => loaded.refused.push((name.clone(), problem)),
        }
    }
    loaded
}

/// The most times one case may be repeated, so a typo in a case file cannot start a thousand runs.
pub const MOST_REPEATS: u32 = 20;

/// How many times a case runs when its file does not say.
pub const DEFAULT_REPEATS: u32 = 3;

/// One saved case: a prompt, how often to run it, and how each run is graded.
///
/// Read from a JSON file in the `evals` directory beside a skill's `SKILL.md`. A key this build
/// does not know is refused rather than ignored, so a grader spelled wrongly cannot be dropped and
/// leave a case that passes by checking nothing.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// What each run is asked, as a person typed it.
    pub prompt: String,
    /// How many times each arm runs the case.
    #[serde(default = "default_repeats")]
    pub repeats: u32,
    /// The checks applied to every run, at least one.
    pub graders: Vec<Grader>,
}

fn default_repeats() -> u32 {
    DEFAULT_REPEATS
}

/// One check on a run, with the weight it counts for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grader {
    /// What is checked.
    pub check: Check,
    /// How much it counts, 1 where the file does not say.
    #[serde(default = "one")]
    pub weight: u32,
}

fn one() -> u32 {
    1
}

/// What a grader checks. The first group reads the driver's own record of the run, never the reply.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    /// The run called the tool of that name at least once.
    ToolCalled(String),
    /// The run never called the tool of that name.
    ToolNotCalled(String),
    /// The run used at most this many rounds.
    RoundsAtMost(u32),
    /// The run loaded the skill under test.
    SkillLoaded(bool),
    /// A model with no tools reads the exchange and answers against this rubric. Counted only
    /// where the verdict is released by the gate.
    Rubric(String),
}

/// Why a case file was not accepted. The words are for the person who wrote the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseProblem {
    /// Not JSON of the shape above, with the parser's reason.
    Malformed(String),
    /// The prompt is empty.
    EmptyPrompt,
    /// No grader, so nothing would be checked.
    NoGraders,
    /// `repeats` is 0 or above [`MOST_REPEATS`].
    Repeats(u32),
    /// A grader carries a weight of 0, which makes it count for nothing.
    ZeroWeight,
}

/// Read one case file's text.
///
/// The text is a person's own file, trusted by the caller before it gets here, so this only checks
/// its structure.
pub fn parse_case(text: &str) -> Result<Case, CaseProblem> {
    let case: Case =
        serde_json::from_str(text).map_err(|error| CaseProblem::Malformed(error.to_string()))?;
    if case.prompt.trim().is_empty() {
        return Err(CaseProblem::EmptyPrompt);
    }
    if case.graders.is_empty() {
        return Err(CaseProblem::NoGraders);
    }
    if case.repeats == 0 || case.repeats > MOST_REPEATS {
        return Err(CaseProblem::Repeats(case.repeats));
    }
    if case.graders.iter().any(|grader| grader.weight == 0) {
        return Err(CaseProblem::ZeroWeight);
    }
    Ok(case)
}

/// One grader's result for one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grade {
    /// Whether the grader passed.
    pub passed: bool,
    /// How much it counts. A case that sets no weights gives every grader 1.
    pub weight: u32,
}

/// The score of one run: the weight of the graders that passed over the weight of all of them.
///
/// `None` where the graders carry no weight at all, since there is nothing to take a fraction of.
/// A run that was stopped at its cap is scored by the caller as no run, not as a run that failed
/// every grader, so a stop is never counted as a pass or as a fail.
pub fn run_score(grades: &[Grade]) -> Option<f64> {
    let total: u64 = grades.iter().map(|g| u64::from(g.weight)).sum();
    if total == 0 {
        return None;
    }
    let passed: u64 = grades
        .iter()
        .filter(|g| g.passed)
        .map(|g| u64::from(g.weight))
        .sum();
    Some(passed as f64 / total as f64)
}

/// The score of one case in one arm: the mean of its runs' scores.
///
/// `None` where there is no scored run.
pub fn case_score(runs: &[Option<f64>]) -> Option<f64> {
    let scored: Vec<f64> = runs.iter().flatten().copied().collect();
    if scored.is_empty() {
        return None;
    }
    Some(scored.iter().sum::<f64>() / scored.len() as f64)
}

/// How many times a rubric is put to the judge (SKILLEVAL-7).
pub const VOTES: usize = 3;

/// How many of those verdicts must be a pass for the grader to pass.
pub const PASSES_NEEDED: usize = 2;

/// What one verdict from the goal judge counts for, or `None` where the driver may not hold it.
///
/// [`goal::Verdict::Quarantined`](crate::goal::Verdict::Quarantined) is the variant the gate
/// produces when the exchange the judge read had met something untrusted. It carries no text, and
/// it counts for nothing: no pass and no fail. A verdict the judge wrote that is not one of the
/// three words is a fail, since a rubric the judge could not rule met has not been shown to hold.
/// The match is on the variant the gate released and never on the judge's prose.
pub fn vote_of(verdict: &crate::goal::Verdict) -> Option<bool> {
    use crate::goal::Verdict;
    match verdict {
        Verdict::Met { .. } => Some(true),
        Verdict::NotMet { .. } | Verdict::Impossible { .. } | Verdict::Unreadable => Some(false),
        Verdict::Quarantined => None,
    }
}

/// Whether a rubric passed, from the votes the judge was asked for, or `None` where the grader
/// has no result.
///
/// It passes when at least [`PASSES_NEEDED`] of [`VOTES`] votes are a pass. A vote the driver was
/// not allowed to hold, or one that could not be had because the request failed, is `None`, and
/// one such vote leaves the whole grader without a result: the votes are over one exchange, so the
/// label that withheld one applies to the rest, and a pass counted from the votes left would let
/// the withheld one be chosen by omission. A number of votes other than [`VOTES`] also gives none.
pub fn rubric_passes(votes: &[Option<bool>]) -> Option<bool> {
    if votes.len() != VOTES {
        return None;
    }
    let held: Option<Vec<bool>> = votes.iter().copied().collect();
    let held = held?;
    Some(held.iter().filter(|passed| **passed).count() >= PASSES_NEEDED)
}

/// A case or a whole evaluation in both arms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Comparison {
    /// The score with the skill in the catalogue.
    pub with: f64,
    /// The score with the skill taken out of it.
    pub without: f64,
}

impl Comparison {
    /// How much the skill added: positive where it helped, negative where it hurt.
    pub fn difference(&self) -> f64 {
        self.with - self.without
    }
}

/// Both arms of a case, or `None` unless each arm has a score. A difference between a score and
/// nothing would read as the whole score.
pub fn compare(with: Option<f64>, without: Option<f64>) -> Option<Comparison> {
    Some(Comparison {
        with: with?,
        without: without?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graded(passed: bool, weight: u32) -> Grade {
        Grade { passed, weight }
    }

    fn case(text: &str) -> Result<Case, CaseProblem> {
        parse_case(text)
    }

    /// A case file with every key reads into the shape the runner uses. Rejects a parser that
    /// drops the weight, the repeats or a grader kind.
    #[test]
    fn a_case_file_reads_into_prompt_repeats_and_weighted_graders() {
        let read = case(
            r#"{"prompt":"tidy imports","repeats":5,"graders":[
                {"check":{"tool_called":"edit_file"},"weight":2},
                {"check":{"rounds_at_most":8}},
                {"check":{"skill_loaded":true}},
                {"check":{"rubric":"the imports are sorted"}}]}"#,
        )
        .expect("a well-formed case reads");
        assert_eq!(read.prompt, "tidy imports");
        assert_eq!(read.repeats, 5);
        assert_eq!(read.graders.len(), 4);
        assert_eq!(read.graders[0].check, Check::ToolCalled("edit_file".into()));
        assert_eq!(read.graders[0].weight, 2);
        assert_eq!(read.graders[1].weight, 1, "an unset weight is 1");
        assert_eq!(
            read.graders[3].check,
            Check::Rubric("the imports are sorted".into())
        );
        let unset = case(r#"{"prompt":"p","graders":[{"check":{"rounds_at_most":1}}]}"#).unwrap();
        assert_eq!(unset.repeats, DEFAULT_REPEATS);
    }

    /// A case that would check nothing, or run nothing, or run without limit is refused, and so is
    /// a key this build does not read. Rejects a parser that ignores an unknown key (a misspelt
    /// grader would silently drop and the case pass on what is left) and one that accepts zero
    /// graders, zero or huge repeats, or a zero weight.
    #[test]
    fn a_case_that_checks_nothing_or_runs_without_limit_is_refused() {
        let one = r#"{"check":{"rounds_at_most":1}}"#;
        assert!(matches!(case("not json"), Err(CaseProblem::Malformed(_))));
        assert_eq!(
            case(&format!(r#"{{"prompt":"  ","graders":[{one}]}}"#)),
            Err(CaseProblem::EmptyPrompt)
        );
        assert_eq!(
            case(r#"{"prompt":"p","graders":[]}"#),
            Err(CaseProblem::NoGraders)
        );
        assert_eq!(
            case(&format!(
                r#"{{"prompt":"p","repeats":0,"graders":[{one}]}}"#
            )),
            Err(CaseProblem::Repeats(0))
        );
        assert_eq!(
            case(&format!(
                r#"{{"prompt":"p","repeats":{},"graders":[{one}]}}"#,
                MOST_REPEATS + 1
            )),
            Err(CaseProblem::Repeats(MOST_REPEATS + 1))
        );
        assert_eq!(
            case(r#"{"prompt":"p","graders":[{"check":{"rounds_at_most":1},"weight":0}]}"#),
            Err(CaseProblem::ZeroWeight)
        );
        assert!(matches!(
            case(&format!(
                r#"{{"prompt":"p","graders":[{one}],"retries":2}}"#
            )),
            Err(CaseProblem::Malformed(_))
        ));
        assert!(matches!(
            case(r#"{"prompt":"p","graders":[{"check":{"tool_calld":"x"}}]}"#),
            Err(CaseProblem::Malformed(_))
        ));
        assert!(
            case(&format!(
                r#"{{"prompt":"p","repeats":{MOST_REPEATS},"graders":[{one}]}}"#
            ))
            .is_ok(),
            "the limit itself is allowed"
        );
    }

    /// Two of three equal graders passing is two thirds. Rejects a score that counts a run as
    /// all-or-nothing, and one that divides by the passes.
    #[test]
    fn a_run_scores_the_fraction_of_its_graders_that_passed() {
        let score = run_score(&[graded(true, 1), graded(true, 1), graded(false, 1)]).unwrap();
        assert!((score - 2.0 / 3.0).abs() < 1e-12, "{score}");
        assert_eq!(run_score(&[graded(true, 1)]), Some(1.0));
        assert_eq!(run_score(&[graded(false, 1), graded(false, 1)]), Some(0.0));
    }

    /// A weight changes what a grader is worth: a failed grader of weight 3 outweighs two passed
    /// ones of weight 1. Rejects a score that ignores weights.
    #[test]
    fn weights_change_what_a_grader_is_worth() {
        let score = run_score(&[graded(true, 1), graded(true, 1), graded(false, 3)]).unwrap();
        assert!((score - 2.0 / 5.0).abs() < 1e-12, "{score}");
    }

    /// No graders, or graders weighing nothing, give no score rather than a 0 or a 1. Rejects a
    /// division by zero and a default that would make an ungraded case read as a result.
    #[test]
    fn graders_with_no_weight_give_no_score() {
        assert_eq!(run_score(&[]), None);
        assert_eq!(run_score(&[graded(true, 0)]), None);
    }

    /// A case is the mean of its runs, and a run with no score takes no part in it. Rejects a mean
    /// that counts an unscored run as 0, which would score a run stopped at its cap as a failure.
    #[test]
    fn a_case_is_the_mean_of_its_scored_runs() {
        assert_eq!(case_score(&[Some(1.0), Some(0.0), Some(0.5)]), Some(0.5));
        assert_eq!(case_score(&[Some(1.0), None, Some(0.5)]), Some(0.75));
        assert_eq!(case_score(&[None, None]), None);
        assert_eq!(case_score(&[]), None);
    }

    /// The difference is with minus without, and exists only when both arms scored. Rejects the
    /// opposite sign and a missing arm read as 0.
    #[test]
    fn the_difference_is_with_minus_without_and_needs_both_arms() {
        let both = compare(Some(0.75), Some(0.25)).unwrap();
        assert_eq!(both.difference(), 0.5);
        assert_eq!(compare(Some(0.25), Some(0.75)).unwrap().difference(), -0.5);
        assert_eq!(compare(Some(0.75), None), None);
        assert_eq!(compare(None, Some(0.25)), None);
    }

    /// SKILLEVAL-7: each verdict the gate released counts as a pass only where the judge said MET,
    /// and the verdict it withheld counts as neither. Rejects a mapping that reads a withheld
    /// verdict as a fail (a run that met untrusted content would score 0 instead of no score) or as
    /// a pass, and one that reads NOT MET, IMPOSSIBLE or an unreadable answer as a pass.
    #[test]
    fn a_verdict_counts_as_a_pass_only_where_the_judge_said_met() {
        use crate::goal::Verdict;
        let reason = || "because".to_string();
        assert_eq!(vote_of(&Verdict::Met { reason: reason() }), Some(true));
        assert_eq!(vote_of(&Verdict::NotMet { reason: reason() }), Some(false));
        assert_eq!(
            vote_of(&Verdict::Impossible { reason: reason() }),
            Some(false)
        );
        assert_eq!(vote_of(&Verdict::Unreadable), Some(false));
        assert_eq!(vote_of(&Verdict::Quarantined), None);
    }

    /// SKILLEVAL-7: a rubric passes on two of three votes. Rejects a threshold of three (unanimity),
    /// one that passes on a single vote, and a strict majority counted wrongly as "more passes than
    /// there are votes".
    #[test]
    fn a_rubric_passes_on_two_of_three_votes() {
        let yes = Some(true);
        let no = Some(false);
        assert_eq!(rubric_passes(&[yes, yes, yes]), Some(true));
        assert_eq!(rubric_passes(&[yes, no, yes]), Some(true));
        assert_eq!(rubric_passes(&[no, yes, yes]), Some(true));
        assert_eq!(rubric_passes(&[yes, no, no]), Some(false));
        assert_eq!(rubric_passes(&[no, no, no]), Some(false));
    }

    /// SKILLEVAL-7: a vote the driver could not hold leaves the rubric with no result, even when
    /// the votes left would carry it. Rejects a count over the votes that remain, which would let
    /// the label that withheld a vote choose the outcome by omission.
    #[test]
    fn a_withheld_vote_leaves_the_rubric_without_a_result() {
        let yes = Some(true);
        assert_eq!(rubric_passes(&[yes, yes, None]), None);
        assert_eq!(rubric_passes(&[None, None, None]), None);
        assert_eq!(rubric_passes(&[yes, yes]), None, "too few votes");
        assert_eq!(rubric_passes(&[yes, yes, yes, yes]), None, "too many");
        assert_eq!(rubric_passes(&[]), None);
    }
}
