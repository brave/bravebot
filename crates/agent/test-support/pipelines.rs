//! Running a pipeline of already-resolved argv stages, for tests.
//!
//! Shipped code runs a compiled plan through `exec::run_plan_observed` and starts a background job
//! through `exec::start_steps`. These build the plan or the steps from a pipeline and the paths its
//! stages resolved to, and pass no confinement and no notification, as the functions they replace
//! in `exec` did.
use bravebot_agent::exec::{self, ExecError};
use bravebot_core::Pipeline;
use bravebot_core::cancel::Cancel;
use bravebot_core::command::{Plan, Step, Steps};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// One step per stage, each started as the path it was resolved to.
pub fn steps_of(pipeline: &Pipeline, resolved: &[PathBuf]) -> Result<Vec<Step>, ExecError> {
    if pipeline.is_empty() {
        return Err(ExecError::Io("no stages to run".to_string()));
    }
    if resolved.len() != pipeline.len() {
        return Err(ExecError::Io(
            "every stage must have been resolved to a program before it runs".to_string(),
        ));
    }
    Ok(pipeline
        .stages
        .iter()
        .zip(resolved)
        .map(|(stage, path)| Step {
            program: stage.program.clone(),
            resolved: path.clone(),
            started_as: path.clone(),
            args: stage.args.clone(),
            environment: Vec::new(),
            routes: Vec::new(),
        })
        .collect())
}

/// Run a pipeline in `directory` and collect what it printed, with the limit named.
pub fn run_within(
    pipeline: &Pipeline,
    resolved: &[PathBuf],
    directory: &Path,
    cancel: &Cancel,
    limit: Duration,
    scratch: Option<&Path>,
) -> Result<exec::Ran, ExecError> {
    let plan = Plan {
        line: String::new(),
        directory: directory.to_path_buf(),
        steps: Steps::Pipeline(steps_of(pipeline, resolved)?),
        writes: Vec::new(),
        reads: Vec::new(),
        stdin: None,
    };
    exec::run_plan_observed(&plan, cancel, limit, scratch, None, None, &mut |_| Ok(()))
}

/// [`run_within`] with the default limit.
pub fn run(
    pipeline: &Pipeline,
    resolved: &[PathBuf],
    directory: &Path,
    cancel: &Cancel,
    scratch: Option<&Path>,
) -> Result<exec::Ran, ExecError> {
    run_within(pipeline, resolved, directory, cancel, exec::LIMIT, scratch)
}

/// Start a pipeline and leave it running.
pub fn start(
    pipeline: &Pipeline,
    resolved: &[PathBuf],
    directory: &Path,
    scratch: Option<&Path>,
) -> Result<exec::Background, ExecError> {
    let steps = steps_of(pipeline, resolved)?;
    exec::start_steps(&steps, directory, scratch, None)
}
