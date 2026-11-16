//! A project's standing MCP answers, as they are listed and forgotten: answer 2 at a server's
//! question, recorded in the projects file, and each tool answer 2 at a call's question stopped
//! asking about, recorded in the tools file. Shared by `bravebot mcp forget` and the window.

use bravebot_config::mcp::{self as records, Projects, Standing};
use std::path::{Path, PathBuf};

/// What is remembered for one project, or what was forgotten for it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Remembered {
    /// Whether every server a checkout there requests is used without asking.
    pub every_server: bool,
    /// The tools no longer asked about there, as `alias:tool`, in order.
    pub tools: Vec<String>,
}

impl Remembered {
    /// Whether nothing is remembered.
    pub fn is_empty(&self) -> bool {
        !self.every_server && self.tools.is_empty()
    }
}

/// A record that is there and cannot be read, so is neither listed nor written back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableRecord {
    pub path: PathBuf,
    pub why: records::Unreadable,
}

/// Why a project's answers were not forgotten. Nothing was written in any case but the last, and
/// there only the file named was not.
#[derive(Debug)]
pub enum Unforgotten {
    /// The session is incognito and writes nothing.
    Incognito,
    /// A record could not be read.
    Unreadable(UnreadableRecord),
    /// A record could not be written.
    NotWritten {
        path: PathBuf,
        error: std::io::Error,
    },
}

/// The project a path names: `typed` taken from `here`, or `here`, as the absolute path it
/// resolves to. A path that no longer resolves is taken as typed, made absolute, since a deleted
/// checkout is the one most worth forgetting and its answers are recorded under that path.
pub fn project_named(here: &Path, typed: Option<&Path>) -> PathBuf {
    let typed = match typed {
        Some(path) => here.join(path),
        None => here.to_path_buf(),
    };
    typed.canonicalize().unwrap_or(typed)
}

/// What the state directory `directory` remembers for `project`.
pub fn remembered(directory: &Path, project: &Path) -> Result<Remembered, UnreadableRecord> {
    let (projects, standing) = records_to_change(directory)?;
    Ok(Remembered {
        every_server: projects.contains(project),
        tools: standing.in_project(project),
    })
}

/// Drop what `directory` remembers for `project`, and nothing recorded for any other, and say what
/// was dropped. Both records are read before either is written, so one that cannot be read leaves
/// both as they are. A record nothing is dropped from is not rewritten.
pub fn forget(directory: &Path, project: &Path) -> Result<Remembered, Unforgotten> {
    if bravebot_core::incognito::engaged() {
        return Err(Unforgotten::Incognito);
    }
    let (mut projects, mut standing) =
        records_to_change(directory).map_err(Unforgotten::Unreadable)?;
    let forgotten = Remembered {
        every_server: projects.remove(project),
        tools: standing.in_project(project),
    };
    standing.forget(project);
    if forgotten.every_server {
        write(&records::projects_file(directory), &projects.to_text())?;
    }
    if !forgotten.tools.is_empty() {
        write(&records::tools_file(directory), &standing.to_text())?;
    }
    Ok(forgotten)
}

fn records_to_change(directory: &Path) -> Result<(Projects, Standing), UnreadableRecord> {
    let projects = Projects::to_change(directory).map_err(|why| UnreadableRecord {
        path: records::projects_file(directory),
        why,
    })?;
    let standing = Standing::to_change(directory).map_err(|why| UnreadableRecord {
        path: records::tools_file(directory),
        why,
    })?;
    Ok((projects, standing))
}

fn write(path: &Path, text: &str) -> Result<(), Unforgotten> {
    crate::home::replace(path, text).map_err(|error| Unforgotten::NotWritten {
        path: path.to_path_buf(),
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory =
            crate::testutil::scratch_dir(&format!("mcp-forget-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create scratch");
        directory
    }

    /// Record answer 2 at a server's question for each of `servers`, and at a call's question for
    /// each `(alias, tool, project)` of `tools`.
    fn answered(directory: &Path, servers: &[&Path], tools: &[(&str, &str, &Path)]) {
        let mut projects = Projects::default();
        for project in servers {
            assert!(projects.add(project));
        }
        std::fs::write(records::projects_file(directory), projects.to_text()).expect("projects");
        let mut standing = Standing::default();
        for (alias, tool, project) in tools {
            assert!(standing.add(alias, tool, project));
        }
        std::fs::write(records::tools_file(directory), standing.to_text()).expect("tools");
    }

    fn texts(directory: &Path) -> (String, String) {
        (
            std::fs::read_to_string(records::projects_file(directory)).unwrap_or_default(),
            std::fs::read_to_string(records::tools_file(directory)).unwrap_or_default(),
        )
    }

    #[test]
    fn forgets_the_servers_alone() {
        let directory = scratch("servers");
        let project = directory.join("project");
        let elsewhere = directory.join("elsewhere");
        answered(
            &directory,
            &[&project],
            &[("weather", "get_alerts", &elsewhere)],
        );
        let tools_before = texts(&directory).1;

        let forgotten = forget(&directory, &project).expect("forgotten");
        assert_eq!(
            forgotten,
            Remembered {
                every_server: true,
                tools: Vec::new()
            }
        );
        assert!(!Projects::read(&directory).contains(&project));
        assert_eq!(texts(&directory).1, tools_before);
    }

    #[test]
    fn forgets_the_tools_alone() {
        let directory = scratch("tools");
        let project = directory.join("project");
        answered(
            &directory,
            &[],
            &[
                ("weather", "get_forecast", &project),
                ("weather", "get_alerts", &project),
            ],
        );

        let forgotten = forget(&directory, &project).expect("forgotten");
        assert_eq!(
            forgotten,
            Remembered {
                every_server: false,
                tools: vec![
                    "weather:get_alerts".to_string(),
                    "weather:get_forecast".to_string()
                ]
            }
        );
        assert!(Standing::read(&directory).in_project(&project).is_empty());
    }

    #[test]
    fn forgets_both_and_leaves_other_projects_as_they_were() {
        let directory = scratch("both");
        let project = directory.join("project");
        let elsewhere = directory.join("elsewhere");
        answered(
            &directory,
            &[&project, &elsewhere],
            &[
                ("weather", "get_forecast", &project),
                ("weather", "get_alerts", &elsewhere),
            ],
        );

        let forgotten = forget(&directory, &project).expect("forgotten");
        assert_eq!(
            forgotten,
            Remembered {
                every_server: true,
                tools: vec!["weather:get_forecast".to_string()]
            }
        );
        assert!(!Projects::read(&directory).contains(&project));
        assert!(Projects::read(&directory).contains(&elsewhere));
        let standing = Standing::read(&directory);
        assert!(!standing.covers("weather", "get_forecast", &project));
        assert!(standing.covers("weather", "get_alerts", &elsewhere));
    }

    #[test]
    fn forgetting_nothing_writes_nothing() {
        let directory = scratch("nothing");
        let project = directory.join("project");
        let elsewhere = directory.join("elsewhere");
        let forgotten = forget(&directory, &project).expect("nothing to forget");
        assert!(forgotten.is_empty());
        assert!(!records::projects_file(&directory).exists());
        assert!(!records::tools_file(&directory).exists());

        answered(
            &directory,
            &[&elsewhere],
            &[("weather", "get_alerts", &elsewhere)],
        );
        let before = texts(&directory);
        assert!(forget(&directory, &project).expect("forgotten").is_empty());
        assert_eq!(texts(&directory), before);
    }

    #[test]
    fn leaves_a_record_it_cannot_read_as_it_is() {
        let directory = scratch("unreadable");
        let project = directory.join("project");
        answered(
            &directory,
            &[&project],
            &[("weather", "get_forecast", &project)],
        );
        let too_large = " ".repeat(64 * 1024 + 1);
        std::fs::write(records::tools_file(&directory), &too_large).unwrap();
        let projects = texts(&directory).0;

        match forget(&directory, &project) {
            Err(Unforgotten::Unreadable(record)) => {
                assert_eq!(record.path, records::tools_file(&directory));
                assert_eq!(record.why, records::Unreadable::TooLarge);
            }
            other => panic!("an unreadable record was forgotten from: {other:?}"),
        }
        assert_eq!(texts(&directory), (projects, too_large));
        assert!(remembered(&directory, &project).is_err());
    }

    #[test]
    fn lists_what_is_remembered_for_one_project_and_changes_nothing() {
        let directory = scratch("list");
        let project = directory.join("project");
        let elsewhere = directory.join("elsewhere");
        assert_eq!(
            remembered(&directory, &project).expect("listed"),
            Remembered::default()
        );
        answered(
            &directory,
            &[&project],
            &[
                ("weather", "get_forecast", &project),
                ("weather", "get_alerts", &elsewhere),
            ],
        );
        let before = texts(&directory);

        assert_eq!(
            remembered(&directory, &project).expect("listed"),
            Remembered {
                every_server: true,
                tools: vec!["weather:get_forecast".to_string()]
            }
        );
        assert_eq!(
            remembered(&directory, &elsewhere).expect("listed"),
            Remembered {
                every_server: false,
                tools: vec!["weather:get_alerts".to_string()]
            }
        );
        assert_eq!(texts(&directory), before);
    }

    #[test]
    fn a_path_that_no_longer_resolves_is_taken_as_typed() {
        let directory = scratch("named");
        let real = directory.join("real");
        std::fs::create_dir_all(&real).unwrap();
        let canonical = real.canonicalize().unwrap();
        assert_eq!(
            project_named(&directory, Some(Path::new("real"))),
            canonical
        );
        assert_eq!(project_named(&real, None), canonical);
        assert_eq!(
            project_named(&directory, Some(Path::new("gone"))),
            directory.join("gone")
        );
    }
}
