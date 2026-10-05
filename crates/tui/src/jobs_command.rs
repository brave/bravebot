//! The argument to `/jobs`.
//!
//! A background job is started, read and stopped by the driver (RUN-26, RUN-27). What is here is
//! the half that is a terminal thing: the words a person types to see the turn's jobs and to ask
//! for one to be stopped.

/// The word that stops a job, rather than naming one.
const STOP: &str = "stop";

/// What the argument to `/jobs` asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The bare word: list the jobs of the turn in flight, or of the last one.
    List,
    /// Stop the job with this name, spelled `job:N`, which is the turn's own where `delegate` is
    /// `None` and that delegate's where it names one, spelled `d1` or `d1.2`.
    Stop {
        name: String,
        delegate: Option<String>,
    },
    /// Anything else, answered by saying what the command takes.
    Unreadable,
}

/// Read the argument to `/jobs`.
///
/// A job is named as the list names it, `job:2`, or by its bare number. Each delegate numbers its
/// own jobs from one, so a delegate's job also takes the delegate's number. Anything else is
/// unreadable rather than a guess, since stopping the wrong job loses a build.
pub fn parse(argument: &str) -> Asked {
    let mut words = argument.split_whitespace();
    let Some(first) = words.next() else {
        return Asked::List;
    };
    if first != STOP {
        return Asked::Unreadable;
    }
    let Some(name) = words.next().and_then(job_name) else {
        return Asked::Unreadable;
    };
    let delegate = match words.next() {
        None => None,
        Some(word) => match delegate_name(word) {
            Some(delegate) => Some(delegate),
            None => return Asked::Unreadable,
        },
    };
    if words.next().is_some() {
        return Asked::Unreadable;
    }
    Asked::Stop { name, delegate }
}

/// `job:N` or `N`, as `job:N` with the number as the driver writes it.
fn job_name(word: &str) -> Option<String> {
    let digits = word.strip_prefix("job:").unwrap_or(word);
    Some(format!("job:{}", number(digits)?))
}

/// `dN` or `dN.M`, as [`bravebot_core::delegate::DelegateId`] displays it.
fn delegate_name(word: &str) -> Option<String> {
    let path = word.strip_prefix('d')?;
    let positions = path.split('.').map(number).collect::<Option<Vec<_>>>()?;
    let spelled: Vec<String> = positions.iter().map(u32::to_string).collect();
    Some(format!("d{}", spelled.join(".")))
}

/// A number counting from one, in ASCII digits only.
fn number(digits: &str) -> Option<u32> {
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok().filter(|number| *number > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(name: &str, delegate: Option<&str>) -> Asked {
        Asked::Stop {
            name: name.to_string(),
            delegate: delegate.map(str::to_string),
        }
    }

    #[test]
    fn the_bare_word_lists_the_jobs() {
        assert_eq!(parse(""), Asked::List);
        assert_eq!(parse("   "), Asked::List);
    }

    #[test]
    fn stop_and_a_name_stops_the_turns_own_job() {
        assert_eq!(parse("stop job:2"), stop("job:2", None));
        assert_eq!(parse("stop 2"), stop("job:2", None));
        assert_eq!(parse("  stop   job:007  "), stop("job:7", None));
    }

    #[test]
    fn a_delegates_number_after_the_name_stops_that_delegates_job() {
        assert_eq!(parse("stop job:1 d2"), stop("job:1", Some("d2")));
        assert_eq!(parse("stop 1 d2.01"), stop("job:1", Some("d2.1")));
    }

    /// Answered by saying what the command takes rather than by guessing which job was meant:
    /// stopping the wrong one is the mistake a guess makes here.
    #[test]
    fn anything_else_is_answered_by_saying_what_the_command_takes() {
        for argument in [
            "stop",
            "stop all",
            "stop job:",
            "stop job:0",
            "stop job:x",
            "stop job:1 2",
            "stop job:1 d",
            "stop job:1 d1.",
            "stop job:1 d1 d2",
            "2",
            "list",
            "kill job:1",
        ] {
            assert_eq!(parse(argument), Asked::Unreadable, "{argument}");
        }
    }
}
