//! `/init`: the turn that drafts a project's `AGENTS.md`, for a project that has none.
//!
//! The words are the driver's own and fixed when the program is built, so they are planner-facing
//! text and stay out of the message catalog. Nothing in them is read from the project, and they
//! name no path with `@`, so sending them vouches for nothing. What the turn may read is whatever
//! the trust map lets any turn read, and the file is written with `write_file`, so it is shown and
//! asked about like every other write (CMD-13).

use std::path::Path;

/// The file `/init` writes, and the only name it looks for.
pub const FILE: &str = "AGENTS.md";

/// What the turn is asked.
///
/// A read of a file the person did not vouch for comes back to the planner as a reference rather
/// than lines (READ-1), which is the cue to ask the person instead of drawing on it.
pub const PROMPT: &str = "Draft an AGENTS.md for this project: a short contributor guide that \
tells a coding agent how to work here.\n\
\n\
Write it to AGENTS.md in the working directory with write_file. Title it \"Repository \
Guidelines\" and keep it between 200 and 400 words of Markdown. Cover, as far as you can \
establish them: project structure and module organisation; build, test and development \
commands; coding style and naming; testing guidelines; and commit and pull request \
conventions. Leave out a section you cannot support rather than inventing it.\n\
\n\
Draw only on what you can see. List and read the project's files to learn these things. If a \
read returns a reference instead of the lines, you may not see that file: do not try to reach \
it another way and do not guess what it says. Where you cannot see enough, ask the person \
short questions with ask_user and write from their answers.\n\
\n\
Write the file once, when you have what you need. The person is shown the write and decides \
about it. Do not create, change or delete any other file.";

/// Whether `root` already holds something called `AGENTS.md`.
///
/// A check on the name and nothing else: the bytes are not read, so what is in an existing file
/// cannot decide anything, and a link with no target still counts, since writing through it would
/// create a file somewhere the person did not choose.
pub fn already_there(root: &Path) -> bool {
    std::fs::symlink_metadata(root.join(FILE)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_of_the_name_is_there_whatever_it_holds() {
        let scratch = crate::testutil::scratch_dir("init-command-there");
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        assert!(!already_there(&scratch));
        std::fs::write(scratch.join(FILE), "").unwrap();
        assert!(already_there(&scratch));
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn the_prompt_names_no_path_for_the_person_to_vouch_for() {
        assert!(!PROMPT.contains('@'));
    }
}
