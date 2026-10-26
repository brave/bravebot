use super::{Ended, Start, Switch};
use std::io;

pub(super) struct Carrying {
    pub workspace: bravebot_agent::Workspace,
    pub sandbox: crate::sandbox_command::Sandboxed,
}

pub(super) enum Phase {
    Session {
        start: Start,
        carrying: Option<Box<Carrying>>,
    },
    Over(io::Result<Ended>),
}

impl Phase {
    /// `--resume` and `--from-pr` begin a session. The Sessions screen opens over it, so they are
    /// not a phase with no session. A manifest record begins none.
    pub(super) fn starting_with(start: Start) -> Self {
        Self::beginning_unless_a_manifest_run(start)
    }

    pub(super) fn after_a_session(ended: io::Result<Ended>, switch: Option<Switch>) -> Self {
        match (ended, switch) {
            (Ok(_), Some(next)) => Self::Session {
                start: Start::Resuming(next.record),
                carrying: Some(Box::new(Carrying {
                    workspace: next.workspace,
                    sandbox: next.sandbox,
                })),
            },
            (ended, _) => Self::Over(ended),
        }
    }

    /// The Sessions screen and `--resume` refuse a manifest record. If one still arrives, loading
    /// its empty conversation as a turn would continue a run that cannot be continued.
    fn beginning_unless_a_manifest_run(start: Start) -> Self {
        match start {
            Start::Resuming(record) if record.manifest.is_some() => {
                Self::Over(Ok(Ended::Left(None)))
            }
            start => Self::Session {
                start,
                carrying: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AskedFor;
    use bravebot_session::sessions::Record;

    fn a_record(id: &str, manifest: bool) -> Box<Record> {
        let mut value = serde_json::json!({
            "id": id,
            "directory": "/tmp/x",
            "title": "an earlier session",
            "started": 1,
            "updated": 1,
            "conversation": {"messages": [], "context": "trusted"},
        });
        if manifest {
            value["manifest"] = serde_json::json!({});
        }
        Box::new(serde_json::from_value(value).expect("a record"))
    }

    fn resuming_id(phase: &Phase) -> Option<&str> {
        match phase {
            Phase::Session {
                start: Start::Resuming(record),
                ..
            } => Some(record.id.as_str()),
            _ => None,
        }
    }

    fn is_fresh(phase: &Phase) -> bool {
        matches!(
            phase,
            Phase::Session {
                start: Start::Fresh,
                carrying: None
            }
        )
    }

    fn is_over_with_nothing_to_resume(phase: &Phase) -> bool {
        matches!(phase, Phase::Over(Ok(Ended::Left(None))))
    }

    #[test]
    fn a_start_begins_a_session_including_one_that_opens_the_sessions_screen() {
        assert!(matches!(
            Phase::starting_with(Start::Choose),
            Phase::Session {
                start: Start::Choose,
                carrying: None
            }
        ));
        assert!(matches!(
            Phase::starting_with(Start::ChooseFromPr("12".into())),
            Phase::Session {
                start: Start::ChooseFromPr(from_pr),
                carrying: None
            } if from_pr == "12"
        ));
        assert!(is_fresh(&Phase::starting_with(Start::Fresh)));
        assert!(matches!(
            Phase::starting_with(Start::Sending("hello".into(), AskedFor::default())),
            Phase::Session {
                start: Start::Sending(..),
                carrying: None
            }
        ));
        assert_eq!(
            resuming_id(&Phase::starting_with(Start::Resuming(a_record(
                "1-2", false
            )))),
            Some("1-2")
        );
    }

    #[test]
    fn a_manifest_record_begins_no_session() {
        assert!(is_over_with_nothing_to_resume(&Phase::starting_with(
            Start::Resuming(a_record("5-6", true))
        )));
    }

    fn typed_sandbox() -> crate::sandbox_command::Sandboxed {
        crate::sandbox_command::Sandboxed::typed(bravebot_sandbox::SandboxMode::ReadOnly)
    }

    fn a_switch(record: Box<Record>, root: &std::path::Path) -> Switch {
        Switch {
            record,
            workspace: bravebot_agent::Workspace::new(root).expect("a workspace"),
            sandbox: typed_sandbox(),
        }
    }

    #[test]
    fn a_switch_begins_the_chosen_record_in_the_workspace_the_session_left() {
        let root = crate::testutil::scratch_dir("phase-switch");
        std::fs::create_dir_all(&root).expect("a directory");
        let moved = bravebot_agent::Workspace::new(&root)
            .expect("a workspace")
            .root()
            .to_path_buf();

        let phase = Phase::after_a_session(
            Ok(Ended::Left(None)),
            Some(a_switch(a_record("7-8", false), &root)),
        );

        assert_eq!(resuming_id(&phase), Some("7-8"));
        let Phase::Session {
            carrying: Some(carried),
            ..
        } = phase
        else {
            panic!("nothing was carried to the next session");
        };
        assert_eq!(carried.workspace.root(), moved);
        assert_eq!(carried.sandbox, typed_sandbox());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_session_that_ends_without_a_switch_ends_the_interface_with_what_it_left() {
        let background = Ended::Refused("no".into());
        assert!(matches!(
            Phase::after_a_session(Ok(background), None),
            Phase::Over(Ok(Ended::Refused(reason))) if reason == "no"
        ));

        let root = crate::testutil::scratch_dir("phase-failed");
        std::fs::create_dir_all(&root).expect("a directory");
        let failed = Phase::after_a_session(
            Err(io::Error::other("the terminal went away")),
            Some(a_switch(a_record("9-10", false), &root)),
        );
        assert!(matches!(
            failed,
            Phase::Over(Err(failure)) if failure.to_string() == "the terminal went away"
        ));
        let _ = std::fs::remove_dir_all(&root);
    }
}
