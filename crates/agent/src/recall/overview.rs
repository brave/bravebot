//! What `/memory` shows of the rules and facts kept (RECALL-11). It reads and writes nothing.

use super::fact::{Fact, Scope};
use super::project::project_id;
use super::store::Store;
use std::path::{Path, PathBuf};

/// The most lines of a rules file that are counted.
const RULE_LINES_COUNTED: usize = 100_000;
/// The most bytes of a rules file that are read to count them.
const RULE_BYTES_READ: u64 = 8 * 1024 * 1024;

/// A rules file that exists, and how many lines it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFile {
    pub path: PathBuf,
    pub lines: usize,
    /// Whether the file holds more lines than were counted.
    pub more: bool,
}

/// The lines of the regular file at `path`, counted from at most [`RULE_BYTES_READ`] bytes and
/// stopping after [`RULE_LINES_COUNTED`]. A link is followed; a pipe, a device or a missing file is
/// none.
fn count_rule_file(path: &Path) -> Option<RuleFile> {
    use std::io::{BufRead, Read};
    // Opened without waiting, then asked what it is, so a pipe put in the file's place between a
    // look and the open is not waited on.
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use rustix::fs::OFlags;
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(OFlags::NONBLOCK.bits().cast_signed());
    }
    let file = options.open(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() {
        return None;
    }
    // A file longer than the bytes read has more lines than were counted, whatever they hold.
    let cut = meta.len() > RULE_BYTES_READ;
    let mut lines = 0;
    for _ in std::io::BufReader::new(file.take(RULE_BYTES_READ)).split(b'\n') {
        if lines == RULE_LINES_COUNTED {
            return Some(RuleFile {
                path: path.to_path_buf(),
                lines,
                more: true,
            });
        }
        lines += 1;
    }
    Some(RuleFile {
        path: path.to_path_buf(),
        lines,
        more: cut,
    })
}

/// The rules and facts `/memory` lists besides the definitions' memories (RECALL-11).
#[derive(Debug, Default)]
pub struct Overview {
    /// The rule layers that exist, in the order they win from the last: the one for every
    /// project, this project's, then the working directory's own (RECALL-2).
    pub rules: Vec<RuleFile>,
    /// The facts in scope, in key order.
    pub facts: Vec<(Scope, Fact)>,
}

/// The rule layers and the facts kept for the project at `workspace`'s root. Reads the front
/// matter of fact files and counts the lines of rule files, and makes no directory. The state
/// rule files are the ones a turn admits ([`crate::preamble::state_rule_files`]), and `own` is the
/// working directory's own file a turn reads, as [`crate::agents::project_rules_file`] chooses it.
pub fn overview(
    home: &Path,
    workspace: &crate::workspace::Workspace,
    trusted: bool,
    own: Option<&Path>,
) -> Overview {
    let project = project_id(workspace.root(), trusted);
    let mut rules: Vec<RuleFile> = crate::preamble::state_rule_files(&[workspace], home, &project)
        .iter()
        .filter_map(|file| count_rule_file(file))
        .collect();
    let store = Store::new(home, project);
    rules.extend(own.and_then(count_rule_file));
    Overview {
        rules,
        facts: store.load().facts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RECALL-11: `/memory` lists the state rule files a turn reads. A dotfiles-style link to a
    /// file elsewhere is followed and listed, as the turn follows it; a link that leads into the
    /// workspace is not, since the turn refuses it; and a pipe is never opened.
    #[cfg(unix)]
    #[test]
    fn a_linked_rules_file_is_listed_as_a_turn_reads_it_and_a_pipe_is_not_read() {
        let home = crate::testutil::scratch_dir("recall-tidy-rules-links");
        let _ = std::fs::remove_dir_all(&home);
        let root = home.join("work");
        std::fs::create_dir_all(&root).expect("root");
        let workspace = crate::workspace::Workspace::new(&root).expect("workspace");
        let store = Store::new(&home, project_id(&root, false));
        let dotfiles = home.join("dotfiles");
        std::fs::create_dir_all(&dotfiles).expect("dotfiles");
        let real = dotfiles.join("AGENTS.md");
        std::fs::write(&real, "one\ntwo\n").expect("real");
        // The global file is a link to a real file; the project's is a pipe, which a read would
        // wait on for ever.
        let global = store.rules_path(Scope::Everywhere);
        std::os::unix::fs::symlink(&real, &global).expect("link");
        let project = store.rules_path(Scope::Project);
        std::fs::create_dir_all(project.parent().expect("parent")).expect("directory");
        let made = std::process::Command::new("mkfifo")
            .arg(&project)
            .status()
            .expect("mkfifo");
        assert!(made.success());
        let listed = overview(&home, &workspace, false, None).rules;
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(
            (listed[0].path.as_path(), listed[0].lines),
            (global.as_path(), 2)
        );

        // A link into the workspace is refused by the turn, so it is not listed either.
        std::fs::remove_file(&global).expect("unlink");
        std::fs::write(root.join("planted.md"), "x\n").expect("planted");
        std::os::unix::fs::symlink(root.join("planted.md"), &global).expect("link");
        assert!(overview(&home, &workspace, false, None).rules.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_long_rules_file_is_counted_up_to_a_cap_and_the_working_directorys_own_is_listed() {
        let home = crate::testutil::scratch_dir("recall-tidy-rules-cap");
        let _ = std::fs::remove_dir_all(&home);
        let root = home.join("work");
        std::fs::create_dir_all(&root).expect("root");
        let store = Store::new(&home, project_id(&root, true));
        std::fs::write(
            store.rules_path(Scope::Everywhere),
            "x\n".repeat(RULE_LINES_COUNTED + 5),
        )
        .expect("long");
        std::fs::write(root.join("AGENTS.md"), "a\nb\nc\n").expect("own");
        let workspace = crate::workspace::Workspace::new(&root).expect("workspace");
        let listed = overview(&home, &workspace, true, Some(&root.join("AGENTS.md"))).rules;
        assert_eq!(listed.len(), 2);
        assert_eq!(
            (listed[0].lines, listed[0].more),
            (RULE_LINES_COUNTED, true)
        );
        assert_eq!((listed[1].lines, listed[1].more), (3, false));
        assert_eq!(listed[1].path, root.join("AGENTS.md"));
        // No file chosen, none listed.
        assert_eq!(overview(&home, &workspace, true, None).rules.len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }
}
