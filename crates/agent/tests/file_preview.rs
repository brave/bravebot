//! The file a click on a path shows (MOUSE-42), asked about every name the file system gives a file
//! (TRUST-2).
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[allow(
    dead_code,
    reason = "test-support/aliases.rs is shared with turn.rs and workspace.rs, and this file asks only Consumer::DenyRule"
)]
#[path = "../test-support/aliases.rs"]
mod aliases;

use bravebot_agent::file_preview::{self, FilePreview, Refused};
use bravebot_agent::workspace::{Workspace, trust_store};
use bravebot_core::event::NullSink;
use bravebot_core::permissions::{Anchors, Permissions};
use bravebot_core::trust::TrustStore;
use std::path::PathBuf;

/// A scratch directory that removes itself, so tests do not leave state behind.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "bravebot-file-preview-aliases-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self {
            path: path.canonicalize().expect("a path"),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn preview(
    workspace: &Workspace,
    permissions: &Permissions,
    named: &str,
) -> Result<FilePreview, Refused> {
    file_preview::preview(
        workspace,
        &TrustStore::new(workspace.root()),
        permissions,
        &mut NullSink,
        named,
        1,
    )
}

fn denying_reads_of(name: &str, workspace: &Workspace) -> Permissions {
    let (permissions, rejected) = Permissions::parse(
        &[format!("Read({name})")],
        &[],
        &[],
        &Anchors {
            folds_case: trust_store(workspace.root()).folds_case(),
            ..Anchors::none()
        },
    );
    assert!(rejected.is_empty(), "the rule for {name:?} did not parse");
    permissions
}

/// A deny rule is about the file (PERM-7), so a preview of any name the volume opens it by is
/// refused, as a read of it by the file tools is. The rows are the TRUST-2 table; a row the rules
/// do not cover lists the issue that tracks it, so the table is revised with the fix. A hard link
/// is the row where the name asked about is spelled by no rule and no link leads from it to the
/// name a rule is written about.
#[test]
fn each_alias_in_the_table_is_refused_by_a_deny_rule_unless_it_is_a_listed_gap() {
    for (index, row) in aliases::ALIASES.iter().enumerate() {
        let scratch = Scratch::new(&format!("alias-{index}"));
        if !aliases::prepare(&scratch.path, row) {
            continue;
        }
        let workspace = Workspace::new(&scratch.path).expect("workspace");
        let rule = denying_reads_of(row.name, &workspace);

        let shown = preview(&workspace, &Permissions::new(), row.alias);
        assert_eq!(
            shown.map(|shown| shown.lines).ok(),
            Some(vec![format!("1  {}", aliases::CONTENT)]),
            "{}: {:?} does not show without a rule, so a refusal would show nothing",
            row.kind,
            row.alias
        );
        assert!(
            preview(&workspace, &rule, aliases::CONTROL).is_ok(),
            "{}: a rule refused a file it does not spell",
            row.kind
        );
        assert_eq!(
            preview(&workspace, &rule, row.name),
            Err(Refused::Denied),
            "{}: a rule did not refuse the name it spells",
            row.kind
        );
        let covered = preview(&workspace, &rule, row.alias) == Err(Refused::Denied);
        aliases::expect(row, aliases::Consumer::DenyRule, covered);
    }
}

/// Where the walk for a file's other names gives up, a rule about a path may be about this file, so
/// the preview is refused while a `deny` rule about any path is in force, as a read by the file
/// tools is. Without a rule the file is shown.
#[cfg(unix)]
#[test]
fn a_file_whose_other_names_cannot_be_found_is_refused_while_a_deny_rule_is_in_force() {
    let scratch = Scratch::new("walk-gives-up");
    std::fs::write(scratch.path.join("secret.txt"), aliases::CONTENT).unwrap();
    std::fs::hard_link(
        scratch.path.join("secret.txt"),
        scratch.path.join("copy.txt"),
    )
    .unwrap();
    std::fs::write(scratch.path.join("other.txt"), "other").unwrap();
    let capped = Workspace::new(&scratch.path)
        .expect("workspace")
        .with_search_caps(Some(1), None);
    let rule = denying_reads_of("elsewhere.txt", &capped);

    assert!(
        preview(&capped, &Permissions::new(), "copy.txt").is_ok(),
        "a walk that gives up refused with no rule in force"
    );
    assert_eq!(
        preview(&capped, &rule, "copy.txt"),
        Err(Refused::Denied),
        "a file with a hard link whose other names could not be found was shown under a rule"
    );
    assert!(
        preview(&capped, &rule, "other.txt").is_ok(),
        "a file with one name was refused for a walk it does not need"
    );
}
