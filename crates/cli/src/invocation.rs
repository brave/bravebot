//! Reading a one-shot invocation off the command line, before anything runs.
//!
//! Everything here reads the person's own arguments, never content.

use crate::continued;
use crate::flags::*;
use bravebot_agent::Mode;
use bravebot_agent::turn::{MAX_TOOL_ROUNDS, RunLimits, SystemPrompts};
use bravebot_i18n::t;

/// The prompt `-i` opens a session on and the model and level named after it, or why the words after
/// it give none.
///
/// One argument, taken as written: a prompt a script assembled holds newlines, and joining words as
/// `--bg` does would flatten them. A missing or blank prompt is refused, for the reason a blank
/// `--model` is: a script whose variable expanded to nothing asked for a task. So is one opening
/// with `-` and holding no whitespace, which is the next flag. `--model` and `--effort` may follow it,
/// read as a one-shot run reads them. Anything else after the prompt is refused by name rather than
/// ignored, because the other options that describe a task are a one-shot run's, and a session that
/// dropped `--file` would run without a file the line asked for.
pub(crate) fn prompt_to_open_on(
    words: &[String],
) -> Result<(String, bravebot_tui::app::AskedFor), String> {
    let prompt = match words.first() {
        Some(prompt) if is_a_value(prompt) => prompt.clone(),
        _ => return Err(t!(cli_interactive_needs_a_prompt).to_string()),
    };
    let mut named = bravebot_tui::app::AskedFor::default();
    let mut index = 1;
    while let Some(word) = words.get(index) {
        match word.as_str() {
            "--model" => named.model = Some(model_name(words.get(index + 1))?),
            "--effort" => named.effort = Some(effort_level(words.get(index + 1))?),
            after => {
                return Err(t!(cli_interactive_takes_nothing_else, argument = after));
            }
        }
        index += 2;
    }
    Ok((prompt, named))
}

/// What a one-shot invocation asked for, before anything runs.
#[derive(Debug)]
pub(crate) struct Invocation {
    pub(crate) prompt: String,
    pub(crate) files: Vec<String>,
    /// The past sessions of this directory named with `--session-ref`, by id (NAME-10).
    pub(crate) session_refs: Vec<String>,
    pub(crate) mode: Mode,
    /// The model the command line named. `None` leaves the configured one in force rather than
    /// standing for a model of its own.
    pub(crate) model: Option<String>,
    /// The model the command line named as the planner's advisor. `None` leaves it to the
    /// `advisorModel` setting.
    pub(crate) advisor: Option<String>,
    /// The model the command line named for processors to run on, as it was written. `None` leaves
    /// it to the `processorModel` setting.
    pub(crate) processor_model: Option<String>,
    /// The level the command line named, which outranks the saved pick and every settings file for
    /// this run alone. `None` leaves those to answer.
    pub(crate) effort: Option<bravebot_session::store::Effort>,
    /// Directories outside the working one that this run may reach into.
    pub(crate) directories: Vec<String>,
    /// Whether the person typed `--trust-workspace`, which gives this run the map a yes to the
    /// startup question writes (TRUST-26).
    pub(crate) trust_workspace: bool,
    pub(crate) trace: bool,
    pub(crate) print: bool,
    /// Whether stdout carries the result object rather than the prose reply.
    pub(crate) json: bool,
    /// Whether stdout also carries an event line for each call, refusal and request as it
    /// finishes, ahead of the result object (CLI-24).
    pub(crate) stream: bool,
    /// The earlier session this run carries on (CLI-25).
    pub(crate) resume: Option<continued::Resume>,
    /// The file holding the JSON Schema the reply must match (CLI-28).
    pub(crate) output_schema: Option<String>,
    /// The condition the run works towards until a judge says it holds (CLI-35).
    pub(crate) goal: Option<String>,
    /// Whether stdin carries lines that start turns, steer them and answer their questions
    /// (CLI-36).
    pub(crate) input_lines: bool,
    /// Whether the person typed `--trust-input-lines`, which makes those lines their own words
    /// where otherwise they are a program's (CLI-36).
    pub(crate) trust_input_lines: bool,
    /// The limits the command line gave the run, which end it where they are reached (CLI-38).
    pub(crate) limits: RunLimits,
}

/// The whole number after `--max-rounds` or `--max-tokens`, or `None` where the word is anything
/// else (CLI-38).
///
/// Digits and nothing else, so a sign, a fraction, an exponent or a space is refused as the
/// mistake in a value a script computed that it is. Zero is refused as well, rather than read as no
/// limit: a limit of nothing would stop the run before its first request, and a script that means
/// no limit leaves the flag off.
pub(crate) fn limit_count(next: Option<&String>) -> Option<u64> {
    next.filter(|word| !word.is_empty() && word.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|word| word.parse::<u64>().ok())
        .filter(|count| *count > 0)
}

/// The name after `--model`, for a one-shot run and for the session `-i` opens.
///
/// A blank name is refused rather than read as no choice. A stored one is allowed to be blank, where
/// it means the file holds nothing and the configured model answers, but a script that computed an
/// empty variable asked for a model and would otherwise be given whatever was configured without
/// being told.
pub(crate) fn model_name(next: Option<&String>) -> Result<String, String> {
    match next.map(|name| name.trim()) {
        Some(name) if !name.is_empty() => Ok(name.to_string()),
        _ => Err(t!(cli_model_needs_a_name).to_string()),
    }
}

/// The level after `--effort`, for a one-shot run and for the session `-i` opens.
///
/// Refused unless it is a level, for the reason a blank `--model` is, and for a stronger one: a model
/// name the service does not know is substituted and reported, where a word that is no level would be
/// dropped without a word and the run sent at another level.
pub(crate) fn effort_level(
    next: Option<&String>,
) -> Result<bravebot_session::store::Effort, String> {
    match next.and_then(|word| bravebot_session::store::Effort::named(word)) {
        Some(level) => Ok(level),
        None => {
            let levels: Vec<&str> = bravebot_session::store::Effort::ALL
                .iter()
                .map(|level| level.as_str())
                .collect();
            Err(t!(cli_effort_needs_a_level, levels = levels.join(", ")))
        }
    }
}

/// Parse `<prompt> [--file path]... [--add-dir path]... [--mode name] [--model name]
/// [--processor-model name] [--effort level] [--trace] [--json] [-p]`.
pub(crate) fn parse_invocation(args: &[String]) -> Result<Invocation, String> {
    let mut prompt = String::new();
    let mut files = Vec::new();
    let mut session_refs: Vec<String> = Vec::new();
    let mut mode = Mode::default();
    let mut model = None;
    let mut advisor = None;
    let mut processor_model = None;
    let mut effort = None;
    let mut directories = Vec::new();
    let mut trust_workspace = false;
    let mut trace = false;
    let mut print = false;
    let mut json = false;
    let mut stream = false;
    let mut resume = None;
    let mut output_schema = None;
    let mut goal = None;
    let mut input_lines = false;
    let mut trust_input_lines = false;
    let mut limits = RunLimits::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--mode" => match args.get(index + 1).map(|name| name.parse::<Mode>()) {
                Some(Ok(chosen)) => {
                    mode = chosen;
                    index += 2;
                }
                Some(Err(complaint)) => return Err(complaint),
                None => {
                    return Err(t!(cli_mode_needs_a_name, names = Mode::NAMES.join(", ")));
                }
            },
            "--model" => {
                model = Some(model_name(args.get(index + 1))?);
                index += 2;
            }
            // Refused when blank for the reason `--model` is: a script that computed an empty
            // variable asked for an advisor and would otherwise run without one, untold.
            "--advisor" => match args.get(index + 1).map(|name| name.trim()) {
                Some(name) if !name.is_empty() => {
                    advisor = Some(name.to_string());
                    index += 2;
                }
                _ => return Err(t!(cli_advisor_needs_a_name).to_string()),
            },
            // Refused when blank for the reason `--advisor` is: a script whose variable expanded to
            // nothing asked for a cheaper model and would otherwise run processors on the dear one.
            "--processor-model" => match args.get(index + 1).map(|name| name.trim()) {
                Some(name) if !name.is_empty() => {
                    processor_model = Some(name.to_string());
                    index += 2;
                }
                _ => return Err(t!(cli_processor_model_needs_a_name).to_string()),
            },
            "--effort" => {
                effort = Some(effort_level(args.get(index + 1))?);
                index += 2;
            }
            "--file" => match args.get(index + 1) {
                Some(path) => {
                    files.push(path.clone());
                    index += 2;
                }
                None => return Err(t!(cli_file_needs_a_path).to_string()),
            },
            "--session-ref" => match args.get(index + 1).map(|id| id.trim()) {
                Some(id) if !id.is_empty() => {
                    // Naming one twice is naming it once, as on a line with the mention twice.
                    if !session_refs.iter().any(|seen| seen == id) {
                        session_refs.push(id.to_string());
                    }
                    index += 2;
                }
                _ => return Err(t!(cli_session_ref_needs_an_id).to_string()),
            },
            // Repeatable, since reaching one sibling checkout is no more natural than reaching
            // two, and a flag that could only be given once would be a rule about typing.
            "--add-dir" => match args.get(index + 1).map(|path| path.trim()) {
                Some(path) if !path.is_empty() => {
                    directories.push(path.to_string());
                    index += 2;
                }
                _ => return Err(t!(cli_add_dir_needs_a_path).to_string()),
            },
            // Repeats are one flag, as with `--incognito`: asking twice for what is already so is
            // no mistake worth refusing a run over.
            "--trust-workspace" => {
                trust_workspace = true;
                index += 1;
            }
            "--trace" => {
                trace = true;
                index += 1;
            }
            // A task carrying on an earlier session. The id is refused when it is missing or is
            // the next flag, for the reason a blank `--model` is: the run would otherwise look for
            // a session called `-p`, or carry on none.
            "--resume" | "-r" => match args.get(index + 1) {
                Some(id) if !id.trim().is_empty() && !id.starts_with('-') => {
                    resume = Some(continued::Resume::Id(id.clone()));
                    index += 2;
                }
                _ => return Err(t!(cli_resume_needs_an_id).to_string()),
            },
            "--continue" | "-c" => {
                resume = Some(continued::Resume::Latest);
                index += 1;
            }
            // Refused when blank for the reason `--model` is: a script that computed an empty
            // variable asked for a shape and would otherwise get free prose, untold.
            "--output-schema" => match args.get(index + 1).map(|path| path.trim()) {
                Some(path) if !path.is_empty() => {
                    output_schema = Some(path.to_string());
                    index += 2;
                }
                _ => return Err(t!(cli_output_schema_needs_a_path).to_string()),
            },
            // Refused when blank or when it is the next flag, as the words of `--system-prompt` are:
            // a script that computed an empty variable asked for a condition and would otherwise
            // run once untold, or take the flag after it for the condition and lose that flag.
            "--goal" => match args.get(index + 1).filter(|text| is_a_value(text)) {
                Some(condition) => {
                    goal = Some(condition.trim().to_string());
                    index += 2;
                }
                None => return Err(t!(cli_goal_needs_a_condition).to_string()),
            },
            // Refused unless it is a whole number of rounds below the built-in bound, for the reason
            // a word that is no level is refused by `--effort`: a script that computed a bad value
            // asked for a cap and would otherwise run without one, untold. At the built-in bound
            // or above it the run is cut off there by that bound, which takes its tools away and
            // has it answer, so the status a script reads is 0 and not this one.
            "--max-rounds" => {
                let rounds = limit_count(args.get(index + 1))
                    .and_then(|count| usize::try_from(count).ok())
                    .filter(|count| *count < MAX_TOOL_ROUNDS);
                limits.rounds =
                    Some(rounds.ok_or_else(|| {
                        t!(cli_max_rounds_needs_a_count, most = MAX_TOOL_ROUNDS - 1)
                    })?);
                index += 2;
            }
            "--max-tokens" => {
                limits.tokens = Some(
                    limit_count(args.get(index + 1))
                        .ok_or_else(|| t!(cli_max_tokens_needs_a_count).to_string())?,
                );
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            // The result object is still the last line, so the stream is a form of `--json`.
            "--json-stream" => {
                json = true;
                stream = true;
                index += 1;
            }
            // `lines` is the one format, and the word is checked rather than assumed so a second
            // format can be added without changing what a script that names this one does. The
            // lines are answered on the stream, so a run that reads them is a `--json-stream` run.
            "--input-format" => match args.get(index + 1).map(String::as_str) {
                Some("lines") => {
                    input_lines = true;
                    json = true;
                    stream = true;
                    index += 2;
                }
                _ => return Err(t!(cli_input_format_unknown).to_string()),
            },
            "--trust-input-lines" => {
                trust_input_lines = true;
                index += 1;
            }
            "-p" | "--print" => {
                print = true;
                index += 1;
            }
            // Reached only behind a task flag, since a leading one opens a session instead. Taken as
            // the prompt, `bravebot --model opus -i` would run the task `-i` with nobody watching.
            "-i" | "--interactive" => return Err(t!(cli_interactive_not_with_a_task).to_string()),
            other if prompt.is_empty() => {
                prompt = other.to_string();
                index += 1;
            }
            other => return Err(t!(cli_unexpected_argument, argument = other)),
        }
    }

    Ok(Invocation {
        prompt,
        files,
        session_refs,
        mode,
        model,
        advisor,
        processor_model,
        effort,
        directories,
        trust_workspace,
        trace,
        print,
        json,
        stream,
        resume,
        output_schema,
        goal,
        input_lines,
        trust_input_lines,
        limits,
    })
}

/// The first of the flags a manifest run cannot take that this invocation gave, as the sentence that
/// refuses it, in the order the checks run: agent, advisor, output schema, goal, tools, system
/// prompts, resume. `None` for any other mode.
pub(crate) fn manifest_conflict(
    mode: Mode,
    has_agent: bool,
    has_advisor: bool,
    has_output_schema: bool,
    has_goal: bool,
    prompts: &SystemPrompts,
    resume: Option<&continued::Resume>,
) -> Option<String> {
    if mode != Mode::Manifest {
        return None;
    }
    // A manifest run's plan is written by the planner before any step runs, and an addressed
    // definition is matched a turn at a time by the turn's own kernel, so the name would be taken
    // and nothing would be addressed to it.
    if has_agent {
        return Some(t!(cli_agent_not_with_a_manifest).to_string());
    }
    // A manifest run's steps are run from the plan, not chosen by a planner that could ask, so an
    // advisor would be named and never consulted.
    if has_advisor {
        return Some(t!(cli_advisor_not_with_a_manifest).to_string());
    }
    // A manifest run has a reply per step and none for the run, so there is no one reply to hold to
    // a schema.
    if has_output_schema {
        return Some(t!(cli_output_schema_not_with_a_manifest).to_string());
    }
    // A manifest run is planned once and run from the plan, so there is no conversation to send
    // the work back into when a judge says the condition is not met.
    if has_goal {
        return Some(t!(cli_goal_not_with_a_manifest).to_string());
    }
    // A manifest run's steps are planned and run from the plan, not chosen from a list of tools, so
    // the flags would be taken and limit nothing.
    if !bravebot_core::tool_set::settled().is_none() {
        return Some(t!(cli_tools_not_with_a_manifest).to_string());
    }
    // Neither reaches the planner of a manifest run, which is given no standing instructions from
    // the command line, and words a run would drop are refused instead (CLI-19).
    if let Some(flag) = flag_named(prompts) {
        return Some(t!(cli_system_prompt_not_with_a_manifest, flag = flag));
    }
    // A manifest run has no conversation to carry a recorded one into.
    resume.map(|asked| {
        let flag = match asked {
            continued::Resume::Id(_) => "--resume",
            continued::Resume::Latest => "--continue",
        };
        t!(cli_resume_not_with_a_manifest, flag = flag)
    })
}

/// Why `--input-format lines` or `--trust-input-lines` cannot go with the rest of the command line,
/// as the sentence that refuses it, or `None` where nothing is wrong (CLI-36).
///
/// `--incognito` is refused because each turn is carried on from the record the one before wrote,
/// and an incognito run writes none.
pub(crate) fn input_lines_conflict(
    input_lines: bool,
    trust_input_lines: bool,
    mode: Mode,
    incognito: bool,
) -> Option<String> {
    if !input_lines {
        return trust_input_lines.then(|| t!(cli_trust_input_lines_needs_lines).to_string());
    }
    if mode == Mode::Manifest {
        return Some(t!(cli_input_lines_not_with_a_manifest).to_string());
    }
    incognito.then(|| t!(cli_input_lines_not_with_incognito).to_string())
}

/// The sentence that refuses a run limit given to a manifest run, which plans before it makes any
/// round of tool calls and has none to count (CLI-38). `None` for a turn, and for a run given none.
///
/// Refused by name rather than taken and ignored: a script that capped a run and was given one
/// with no cap would pay for the cost it asked to bound.
pub(crate) fn limit_not_with_a_manifest(mode: Mode, limits: RunLimits) -> Option<String> {
    let flag = limit_flag(limits)?;
    (mode == Mode::Manifest).then(|| t!(cli_limit_not_with_a_manifest, flag = flag))
}

/// The sentence that refuses a run limit given with `--goal`, whose rounds are each a turn that
/// counts a limit from nothing, so a limit would bound each round and not the run (CLI-38).
/// `None` for a run with no goal, and for a run given no limit.
///
/// Refused by name for the reason a manifest run is: a script that capped a run would be given one
/// that spends the cap in every round, up to eleven times over.
pub(crate) fn limit_not_with_a_goal(has_goal: bool, limits: RunLimits) -> Option<String> {
    let flag = limit_flag(limits)?;
    has_goal.then(|| t!(cli_limit_not_with_a_goal, flag = flag))
}

/// The flag that gave the run a limit, `--max-rounds` before `--max-tokens` where it gave both
/// (CLI-38). `None` for a run given none.
pub(crate) fn limit_flag(limits: RunLimits) -> Option<&'static str> {
    match limits {
        RunLimits {
            rounds: Some(_), ..
        } => Some("--max-rounds"),
        RunLimits {
            tokens: Some(_), ..
        } => Some("--max-tokens"),
        _ => None,
    }
}
