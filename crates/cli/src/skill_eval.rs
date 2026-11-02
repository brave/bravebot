//! `bravebot skill eval <name> [--model <model>] [--max-rounds <n>] [--max-tokens <n>]
//! [--trust-workspace]`: run the
//! saved cases of one skill with the skill and again without it, grade both, and report the two
//! scores and their difference (SKILLEVAL-1 to SKILLEVAL-9).
//!
//! Each run is the one-shot turn `bravebot "<task>"` makes, with nobody to ask, so an effect that
//! needs a prompt is refused (CLI-1). The grading reads the driver's record of the run: the tools
//! it called, the rounds it made and which catalogue entry each `load_skill` call resolved to. That
//! entry is the name the catalogue holds the skill under, set where the call is executed against the
//! catalogue and never read from the call's arguments, so whether the skill was loaded is settled
//! by the driver's own fact and not by comparing a word the model typed. Nothing here reads a
//! reply or a call's target.

use crate::exit::{Ending, fail};
use crate::json;
use crate::progress;
use crate::workspace::{current_workspace, rules_for_a_one_shot_run, unattended_trust};
use crate::{Prepared, prepare_the_run};
use bravebot_agent::skill_eval::{self, Case, CaseProblem, Check, Grade};
use bravebot_agent::turn::{self, SystemPrompts, Task};
use bravebot_core::cancel::Cancel;
use bravebot_i18n::t;
use std::process::ExitCode;

/// The rounds one run may make when `--max-rounds` is not given (SKILLEVAL-5).
const DEFAULT_ROUNDS: usize = 12;

/// What was asked for, once the words are read.
#[derive(Debug, PartialEq, Eq)]
struct Request {
    name: String,
    model: Option<String>,
    max_rounds: usize,
    max_tokens: Option<u64>,
    trust_workspace: bool,
}

/// `eval <name>` and the four flags it takes, in any order. A second `eval`, a flag given twice, a
/// count that is not a positive whole number and any other word are refused. Flags that let a run
/// write are not among them (SKILLEVAL-5).
fn read(args: &[String]) -> Option<Request> {
    let (first, rest) = args.split_first()?;
    if first != "eval" {
        return None;
    }
    let (name, mut rest) = rest.split_first()?;
    if name.starts_with('-') || name.trim().is_empty() {
        return None;
    }
    let mut request = Request {
        name: name.clone(),
        model: None,
        max_rounds: DEFAULT_ROUNDS,
        max_tokens: None,
        trust_workspace: false,
    };
    let mut rounds_given = false;
    while let Some((flag, after)) = rest.split_first() {
        rest = after;
        match flag.as_str() {
            "--trust-workspace" if !request.trust_workspace => request.trust_workspace = true,
            "--model" if request.model.is_none() => {
                let (value, after) = rest.split_first()?;
                if value.starts_with('-') || value.trim().is_empty() {
                    return None;
                }
                request.model = Some(value.clone());
                rest = after;
            }
            "--max-rounds" if !rounds_given => {
                let (value, after) = rest.split_first()?;
                request.max_rounds = value.parse().ok().filter(|rounds| *rounds > 0)?;
                rounds_given = true;
                rest = after;
            }
            "--max-tokens" if request.max_tokens.is_none() => {
                let (value, after) = rest.split_first()?;
                request.max_tokens = Some(crate::invocation::limit_count(Some(value))?);
                rest = after;
            }
            _ => return None,
        }
    }
    Some(request)
}

/// What the driver recorded of one run (SKILLEVAL-6).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Record {
    /// Each tool call by the tool's own name, the catalogue entry a `load_skill` call resolved to
    /// (the catalogue's name for it, never the call's argument) and whether it was refused.
    calls: Vec<(String, Option<String>, bool)>,
    /// The rounds the run made.
    rounds: usize,
}

impl Record {
    fn of(calls: &[json::Call], rounds: usize) -> Self {
        Self {
            calls: calls
                .iter()
                .map(|call| (call.tool.clone(), call.skill.clone(), call.refused))
                .collect(),
            rounds,
        }
    }

    fn called(&self, tool: &str) -> bool {
        self.calls.iter().any(|(called, ..)| called == tool)
    }

    /// Whether the run asked the catalogue for skills at all.
    fn searched(&self) -> bool {
        self.called("search_skills")
    }

    /// Whether a `load_skill` call resolved to this catalogue entry and was answered. `skill` is
    /// the entry's name as the catalogue holds it, which is also what each call's resolved entry is
    /// recorded as, so two names the driver holds are compared and no word of the model's is.
    fn loaded(&self, skill: &str) -> bool {
        self.calls.iter().any(|(tool, resolved, refused)| {
            tool == "load_skill" && resolved.as_deref() == Some(skill) && !refused
        })
    }

    fn refused(&self) -> usize {
        self.calls.iter().filter(|(.., refused)| *refused).count()
    }
}

/// The grades of one run, or `None` where a grader has no result. `rubrics` holds, in the order
/// the case lists its rubric graders, whether each passed on at least two of three released
/// verdicts, or `None` where the gate released fewer (SKILLEVAL-7). A `None` leaves the run without
/// a score and is never read as a fail or a pass.
fn grades(
    case: &Case,
    record: &Record,
    skill: &str,
    rubrics: &[Option<bool>],
) -> Option<Vec<Grade>> {
    let mut rubrics = rubrics.iter();
    case.graders
        .iter()
        .map(|grader| {
            let passed = match &grader.check {
                Check::ToolCalled(tool) => record.called(tool),
                Check::ToolNotCalled(tool) => !record.called(tool),
                Check::RoundsAtMost(most) => record.rounds <= *most as usize,
                Check::SkillLoaded(wanted) => record.loaded(skill) == *wanted,
                Check::Rubric(_) => (*rubrics.next()?)?,
            };
            Some(Grade {
                passed,
                weight: grader.weight,
            })
        })
        .collect()
}

/// How one run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum How {
    Finished,
    /// It stopped at a round or token limit and has no score (SKILLEVAL-5).
    Capped,
    /// It ended on an error other than a limit.
    Failed,
}

/// One arm of one case.
#[derive(Debug, Default)]
struct Arm {
    scores: Vec<Option<f64>>,
    searched: usize,
    loaded: usize,
    capped: usize,
    failed: usize,
    refused: usize,
    runs: usize,
    /// Runs with a rubric whose verdicts the gate did not release, so the rubric has no result.
    withheld: usize,
}

impl Arm {
    fn add(&mut self, how: How, record: &Record, score: Option<f64>, skill: &str, withheld: bool) {
        self.runs += 1;
        self.withheld += usize::from(withheld);
        self.scores.push(score);
        self.searched += usize::from(record.searched());
        self.loaded += usize::from(record.loaded(skill));
        self.capped += usize::from(how == How::Capped);
        self.failed += usize::from(how == How::Failed);
        self.refused += record.refused();
    }

    fn score(&self) -> Option<f64> {
        skill_eval::case_score(&self.scores)
    }
}

/// What one run came to: how it ended, the driver's record of it, and for each rubric of the case
/// whether it passed (`None` where the gate released too few verdicts).
struct Ran {
    how: How,
    record: Record,
    rubrics: Vec<Option<bool>>,
}

/// Put each rubric to the goal judge three times over the exchange of a finished run.
///
/// The judge is [`turn::goal`]: a request with no tools, whose answer is read only where the gate
/// releases it to the driver (GOAL-3, GOAL-5). The rubric is the text of a case file read through
/// its skill's trust, and the exchange is the run's own, so what is counted is a verdict over
/// whatever the run met. A verdict over untrusted content comes back quarantined, counts for
/// nothing and is not shown, and a request that failed counts for nothing either (SKILLEVAL-7).
fn judge_rubrics(
    prepared: &Prepared,
    task: &Task,
    completed: &turn::CompletedTurn,
    conversation: &bravebot_agent::Conversation,
    rubrics: &[&str],
) -> Vec<Option<bool>> {
    let mut sink = json::Streaming::new(None);
    rubrics
        .iter()
        .map(|rubric| {
            let votes: Vec<Option<bool>> = (0..skill_eval::VOTES)
                .map(|_| {
                    let check =
                        bravebot_agent::goal::Check::of(conversation, rubric, completed.work);
                    turn::goal(
                        &prepared.config,
                        &prepared.egress,
                        task.account_session(),
                        check,
                        Some(prepared.model.as_str()),
                        &mut sink,
                        completed.decisions.trust.clone(),
                        &Cancel::new(),
                    )
                    .ok()
                    .and_then(|assessed| skill_eval::vote_of(&assessed.verdict))
                })
                .collect();
            skill_eval::rubric_passes(&votes)
        })
        .collect()
}

/// Run one task as a one-shot run with nobody to ask, and record what the driver saw of it.
fn run_once(
    prepared: &Prepared,
    trust: &bravebot_core::trust::TrustStore,
    task: &Task,
    rubrics: &[&str],
) -> Ran {
    let mut one_shot = crate::one_shot::OneShot::new(std::io::empty(), std::io::sink(), false);
    let mut confirmer =
        bravebot_agent::Confining::new(&mut one_shot, prepared.permission_mode, task.auto_vetting)
            .with_sandbox_mode(task.sandbox.clone());
    let mut reporter = progress::Progress::new(std::io::sink());
    let mut sink = json::Streaming::new(None);
    let mut conversation = bravebot_agent::Conversation::default();
    let completed = turn::resume(
        &prepared.config,
        &prepared.egress,
        &prepared.workspace,
        task,
        &mut conversation,
        &mut confirmer,
        &mut reporter,
        &mut sink,
        trust.clone(),
        bravebot_core::programs::TrustedPrograms::new(),
        None,
        &Cancel::new(),
    );
    match &completed.outcome {
        Ok(outcome) => Ran {
            how: How::Finished,
            record: Record::of(reporter.calls(), outcome.steps),
            rubrics: judge_rubrics(prepared, task, &completed, &conversation, rubrics),
        },
        Err(error) => Ran {
            how: match error.limit_reached() {
                Some(_) => How::Capped,
                None => How::Failed,
            },
            record: Record::of(
                reporter.calls(),
                error.limit_reached().map_or(0, |reached| reached.rounds()),
            ),
            rubrics: Vec::new(),
        },
    }
}

/// `bravebot skill ...`, after `skill`.
pub(crate) fn command(args: &[String], bypass_flag: Option<&str>) -> ExitCode {
    let Some(request) = read(args) else {
        return fail(Ending::Argument, t!(skill_eval_usage));
    };
    // The flags that let a run write are settled before dispatch and handed over, so a line that
    // carried one is refused here by the spelling that was typed and not run (SKILLEVAL-5).
    // `--accept-edits` and `--auto` are refused before dispatch, where no one-shot run follows.
    if let Some(flag) = bypass_flag {
        return fail(
            Ending::Argument,
            t!(skill_eval_takes_no_write_flag, flag = flag),
        );
    }
    let settings = bravebot_config::Settings::load();
    let workspace = match current_workspace(&settings, &bravebot_config::Managed::load()) {
        Ok(workspace) => workspace,
        Err(problem) => return crate::workspace::workspace_problem(&problem),
    };
    let (permissions, _rejected) = rules_for_a_one_shot_run(
        &settings,
        bravebot_agent::home::profile().as_deref(),
        workspace.root(),
        false,
    );
    let trust = unattended_trust(workspace.root(), request.trust_workspace);
    let home = bravebot_agent::home::directory();
    let mut events = bravebot_core::event::NullSink;
    let catalogue = bravebot_agent::skills::resolved(
        &workspace,
        home.as_deref(),
        trust.clone(),
        permissions.clone(),
        &mut events,
    );
    // One sentence for a name nothing offers and for one a project nobody vouched for would have
    // offered, naming only what was typed (SKILLEVAL-1).
    let Some(skill) = catalogue.get(&request.name).cloned() else {
        return fail(
            Ending::Argument,
            t!(skill_eval_unknown_name, name = request.name.as_str()),
        );
    };
    if skill.person_only {
        return fail(
            Ending::Argument,
            t!(skill_eval_person_only, name = request.name.as_str()),
        );
    }
    let loaded = skill_eval::load_cases(&skill, &workspace, &trust, &permissions, &mut events);
    if loaded.not_read > 0 {
        eprintln!(
            "{}",
            t!(skill_eval_not_read, count = loaded.not_read as i64)
        );
    }
    if let Some((file, problem)) = loaded.refused.first() {
        return fail(
            Ending::Argument,
            t!(
                skill_eval_case_refused,
                file = progress::printable(file),
                problem = describe(problem)
            ),
        );
    }
    if loaded.cases.is_empty() {
        return fail(
            Ending::Argument,
            t!(skill_eval_no_cases, name = request.name.as_str()),
        );
    }
    drop(workspace);
    run_cases(&request, &loaded, &trust)
}

/// Run each loaded case with the skill and without it, and say the scores (SKILLEVAL-2).
fn run_cases(
    request: &Request,
    loaded: &skill_eval::Loaded,
    trust: &bravebot_core::trust::TrustStore,
) -> ExitCode {
    let mut run_args = vec!["--max-rounds".to_string(), request.max_rounds.to_string()];
    if let Some(tokens) = request.max_tokens {
        run_args.extend(["--max-tokens".to_string(), tokens.to_string()]);
    }
    if let Some(model) = &request.model {
        run_args.extend(["--model".to_string(), model.clone()]);
    }
    if request.trust_workspace {
        run_args.push("--trust-workspace".to_string());
    }
    // Replaced by each case's prompt below. It only has to be a task for the run to be prepared.
    run_args.push(request.name.clone());
    let prepared = match prepare_the_run(&run_args, false, None, None, SystemPrompts::default()) {
        Ok(prepared) => prepared,
        Err(code) => return code,
    };
    let mut with_cases = Vec::new();
    let mut without_cases = Vec::new();
    let mut lines = String::new();
    let mut failed = 0;
    for (file, case) in &loaded.cases {
        eprintln!(
            "{}",
            t!(
                skill_eval_running,
                case = progress::printable(file),
                runs = i64::from(case.repeats)
            )
        );
        let (mut with, mut without) = (Arm::default(), Arm::default());
        for _ in 0..case.repeats {
            for baseline in [false, true] {
                let mut task = prepared.task.clone();
                task.prompt = case.prompt.clone();
                if baseline {
                    task = task.without_skill(Some(request.name.clone()));
                }
                let rubrics: Vec<&str> = case
                    .graders
                    .iter()
                    .filter_map(|grader| match &grader.check {
                        Check::Rubric(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                let ran = run_once(&prepared, trust, &task, &rubrics);
                let score = match ran.how {
                    How::Finished => grades(case, &ran.record, &request.name, &ran.rubrics)
                        .and_then(|graded| skill_eval::run_score(&graded)),
                    How::Capped | How::Failed => None,
                };
                let withheld = ran.rubrics.iter().any(Option::is_none);
                let arm = if baseline { &mut without } else { &mut with };
                arm.add(ran.how, &ran.record, score, &request.name, withheld);
            }
        }
        failed += with.failed + without.failed;
        lines.push_str(&case_line(file, &with, &without));
        with_cases.push(with);
        without_cases.push(without);
    }
    prepared.dismantle();

    let overall = |arms: &[Arm]| {
        let scores: Vec<Option<f64>> = arms.iter().map(Arm::score).collect();
        skill_eval::case_score(&scores)
    };
    lines.push_str(&overall_line(overall(&with_cases), overall(&without_cases)));
    print!("{lines}");
    match failed {
        0 => ExitCode::SUCCESS,
        _ => fail(
            Ending::Failed,
            t!(skill_eval_runs_failed, count = failed as i64),
        ),
    }
}

/// Why a case file was refused, in the person's language. The parser's own reason for a file that
/// is not JSON of the right shape is the one part that is not translated.
fn describe(problem: &CaseProblem) -> String {
    match problem {
        CaseProblem::Malformed(reason) => t!(
            skill_eval_problem_malformed,
            reason = progress::printable(reason)
        )
        .to_string(),
        CaseProblem::EmptyPrompt => t!(skill_eval_problem_empty_prompt).to_string(),
        CaseProblem::NoGraders => t!(skill_eval_problem_no_graders).to_string(),
        CaseProblem::Repeats(repeats) => t!(
            skill_eval_problem_repeats,
            repeats = i64::from(*repeats),
            most = i64::from(skill_eval::MOST_REPEATS)
        )
        .to_string(),
        CaseProblem::ZeroWeight => t!(skill_eval_problem_zero_weight).to_string(),
    }
}

fn shown(score: Option<f64>) -> String {
    match score {
        Some(score) => format!("{score:.2}"),
        None => t!(skill_eval_no_score).to_string(),
    }
}

fn difference(with: Option<f64>, without: Option<f64>) -> String {
    match skill_eval::compare(with, without) {
        Some(comparison) => format!("{:+.2}", comparison.difference()),
        None => t!(skill_eval_no_score).to_string(),
    }
}

/// The lines for one case: its two scores and their difference, then what each arm counted.
fn case_line(file: &str, with: &Arm, without: &Arm) -> String {
    let (scored_with, scored_without) = (with.score(), without.score());
    let mut text = format!(
        "{}\n",
        t!(
            skill_eval_case,
            case = progress::printable(file),
            with = shown(scored_with),
            without = shown(scored_without),
            difference = difference(scored_with, scored_without)
        )
    );
    for (arm, label) in [
        (with, t!(skill_eval_with)),
        (without, t!(skill_eval_without)),
    ] {
        text.push_str(&format!(
            "{}\n",
            t!(
                skill_eval_arm_counts,
                arm = label,
                runs = arm.runs as i64,
                searched = arm.searched as i64,
                loaded = arm.loaded as i64,
                capped = arm.capped as i64,
                failed = arm.failed as i64,
                refused = arm.refused as i64,
                withheld = arm.withheld as i64
            )
        ));
    }
    text
}

fn overall_line(with: Option<f64>, without: Option<f64>) -> String {
    format!(
        "{}\n",
        t!(
            skill_eval_overall,
            with = shown(with),
            without = shown(without),
            difference = difference(with, without)
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::skill_eval::{Check, Grader};

    fn words(typed: &[&str]) -> Vec<String> {
        typed.iter().map(|word| word.to_string()).collect()
    }

    fn call(tool: &str, resolved: Option<&str>, refused: bool) -> (String, Option<String>, bool) {
        (tool.to_string(), resolved.map(str::to_string), refused)
    }

    fn case(checks: Vec<Check>) -> Case {
        Case {
            prompt: "do it".to_string(),
            repeats: 1,
            graders: checks
                .into_iter()
                .map(|check| Grader { check, weight: 1 })
                .collect(),
        }
    }

    /// SKILLEVAL-5. `eval <name>` takes its four flags and no others, because a flag that lets a
    /// run write (`--accept-edits`, `--mode auto`) would let a case file's prompt change the
    /// person's tree. Rejects a reader that passes an unknown word through, or accepts a count of
    /// zero, a repeated flag or a flag with no value.
    #[test]
    fn the_name_and_the_four_flags_are_read_and_nothing_else() {
        let read_it = |typed: &[&str]| read(&words(typed));
        assert_eq!(
            read_it(&["eval", "demo"]),
            Some(Request {
                name: "demo".to_string(),
                model: None,
                max_rounds: DEFAULT_ROUNDS,
                max_tokens: None,
                trust_workspace: false,
            })
        );
        assert_eq!(
            read_it(&[
                "eval",
                "demo",
                "--max-rounds",
                "4",
                "--max-tokens",
                "900",
                "--model",
                "m",
                "--trust-workspace"
            ]),
            Some(Request {
                name: "demo".to_string(),
                model: Some("m".to_string()),
                max_rounds: 4,
                max_tokens: Some(900),
                trust_workspace: true,
            })
        );
        for refused in [
            &[][..],
            &["eval"][..],
            &["run", "demo"][..],
            &["eval", "--trust-workspace"][..],
            &["eval", "demo", "extra"][..],
            &["eval", "demo", "--max-rounds", "0"][..],
            &["eval", "demo", "--max-rounds", "many"][..],
            &["eval", "demo", "--max-rounds"][..],
            &["eval", "demo", "--max-tokens"][..],
            &["eval", "demo", "--max-tokens", "0"][..],
            &["eval", "demo", "--max-tokens", "-5"][..],
            &["eval", "demo", "--max-tokens", "1", "--max-tokens", "2"][..],
            &["eval", "demo", "--model", "--trust-workspace"][..],
            &["eval", "demo", "--trust-workspace", "--trust-workspace"][..],
            &["eval", "demo", "--mode", "auto"][..],
            &["eval", "demo", "--accept-edits"][..],
        ] {
            assert_eq!(read_it(refused), None, "{refused:?}");
        }
    }

    /// SKILLEVAL-6. A deterministic grader is decided by what the driver recorded of the run (the
    /// tools called, whether a call was refused, the rounds, whether the skill loaded), never by
    /// the model's reply, which the model controls. Rejects a grader that reads the reply, or that
    /// counts a refused call as made.
    #[test]
    fn a_deterministic_grader_reads_the_record_and_not_the_reply() {
        let record = Record {
            calls: vec![
                call("search_skills", None, false),
                call("load_skill", Some("demo"), false),
                call("write_file", None, true),
            ],
            rounds: 3,
        };
        let graded = |checks: Vec<Check>| {
            grades(&case(checks), &record, "demo", &[])
                .unwrap()
                .iter()
                .map(|grade| grade.passed)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            graded(vec![
                Check::ToolCalled("search_skills".to_string()),
                Check::ToolCalled("read_file".to_string()),
                Check::ToolNotCalled("read_file".to_string()),
                Check::ToolNotCalled("write_file".to_string()),
                Check::RoundsAtMost(3),
                Check::RoundsAtMost(2),
                Check::SkillLoaded(true),
                Check::SkillLoaded(false),
            ]),
            [true, false, true, false, true, false, true, false]
        );
    }

    /// SKILLEVAL-6. Whether the skill loaded is a fact about the catalogue entry a `load_skill`
    /// call resolved to and was answered for. Rejects a record that counts a refused call, a call
    /// to another tool, a call for another entry, or a call that resolved to no entry.
    #[test]
    fn a_skill_counts_as_loaded_only_when_the_catalogue_entry_it_resolved_to_was_answered() {
        let loaded =
            |calls: Vec<(String, Option<String>, bool)>| Record { calls, rounds: 1 }.loaded("demo");
        assert!(loaded(vec![call("load_skill", Some("demo"), false)]));
        assert!(!loaded(vec![call("load_skill", Some("demo"), true)]));
        assert!(!loaded(vec![call("load_skill", Some("other"), false)]));
        assert!(!loaded(vec![call("search_skills", Some("demo"), false)]));
        // A call that resolved to no entry is not a load of any skill, whatever it was typed as.
        assert!(!loaded(vec![call("load_skill", None, false)]));
    }

    /// SKILLEVAL-6. The record is built from the call's resolved entry and not from the target
    /// the call was drawn with, which is the model's word. Rejects a record that copies the target
    /// and compares it: here the target says `demo` for a call that resolved to nothing, and the
    /// target says something else for a call that resolved to `demo`.
    #[test]
    fn the_record_ignores_the_target_a_call_was_drawn_with() {
        let calls = [
            json::Call {
                tool: "load_skill".to_string(),
                target: "demo".to_string(),
                skill: None,
                refused: false,
            },
            json::Call {
                tool: "load_skill".to_string(),
                target: "DEMO ".to_string(),
                skill: Some("demo".to_string()),
                refused: false,
            },
        ];
        assert!(!Record::of(&calls[..1], 1).loaded("demo"));
        assert!(Record::of(&calls[1..], 1).loaded("demo"));
    }

    /// SKILLEVAL-7. A rubric passes or fails on the verdicts the gate released. Where it released
    /// too few, the rubric has no result and the run has no grade, because the verdict is over
    /// content that may be untrusted and a withheld one must not pick the result. Rejects reading a
    /// withheld rubric as a fail or a pass.
    #[test]
    fn a_rubric_takes_its_result_from_the_released_votes_and_without_one_gives_no_grade() {
        let record = Record::default();
        let rubric = case(vec![Check::Rubric("is it good".to_string())]);
        let graded = |votes: &[Option<bool>]| {
            grades(&rubric, &record, "demo", votes)
                .map(|grades| grades.iter().map(|grade| grade.passed).collect::<Vec<_>>())
        };
        assert_eq!(graded(&[Some(true)]), Some(vec![true]));
        assert_eq!(graded(&[Some(false)]), Some(vec![false]));
        assert_eq!(graded(&[None]), None, "a withheld rubric was graded");
        assert_eq!(graded(&[]), None, "a rubric with no result was graded");
    }

    /// A person who writes a case file is told why it was refused in words. Rejects a message built
    /// with `{:?}`, which shows variant names and quoted strings instead of a sentence.
    #[test]
    fn every_case_problem_is_described_in_words_with_no_debug_syntax() {
        for (problem, wanted) in [
            (
                CaseProblem::Malformed("unknown field `retries`".to_string()),
                "unknown field `retries`",
            ),
            (CaseProblem::EmptyPrompt, "prompt is empty"),
            (CaseProblem::NoGraders, "no grader"),
            (
                CaseProblem::Repeats(0),
                "repeats is 0, which is outside 1 to 20",
            ),
            (CaseProblem::ZeroWeight, "weight of 0"),
        ] {
            let said = describe(&problem);
            assert!(said.contains(wanted), "{said}");
            for debug in [
                "Malformed(",
                "EmptyPrompt",
                "NoGraders",
                "Repeats(",
                "ZeroWeight",
                "\"",
            ] {
                assert!(!said.contains(debug), "{debug} in {said}");
            }
        }
    }

    /// SKILLEVAL-5. A run that stopped at a limit has no score, and the report says so. Rejects
    /// counting it as 0, which would read as the skill failing and skew the difference between the
    /// two arms.
    #[test]
    fn an_arm_with_no_score_is_said_and_not_read_as_zero() {
        assert_eq!(difference(Some(1.0), None), t!(skill_eval_no_score));
        assert_eq!(difference(None, Some(0.0)), t!(skill_eval_no_score));
        assert_eq!(shown(Some(0.0)), "0.00");
        assert_eq!(difference(Some(0.5), Some(1.0)), "-0.50");
        let mut arm = Arm::default();
        arm.add(How::Capped, &Record::default(), None, "demo", false);
        assert_eq!(arm.score(), None);
        assert_eq!((arm.runs, arm.capped), (1, 1));
    }
}
