//! The paths a write asks about whatever the mode or a rule says (MODE-2, PERM-9).
//!
//! A file in one of these places runs code later, with the person's own rights: a git hook the
//! next `git commit` starts, a configuration the next `git` command reads, the project's own
//! `.bravebot/` that a trusted session loads as instructions. A write there is put to the person
//! in accepting-edits mode and under an `Edit` allow rule, and refused where nobody can be asked.
//!
//! The decision reads the routing path alone, the name the planner gave or the name that lands on
//! after links are followed, and never a file's contents.

/// A directory whose every entry is protected, matched on any component of the path.
const DIRECTORIES: [&str; 3] = [".git", ".bravebot", ".husky"];

/// A file protected wherever it is.
const FILES: [&str; 3] = [".gitmodules", ".gitconfig", ".pre-commit-config.yaml"];

/// Whether a write to `path` is one that nothing but a person answers.
///
/// Segments split on `/` and `\`, since a host that separates with the second hands a name back
/// spelled that way, and a name read too widely only costs a question. A trailing dot or space is
/// dropped, because a Windows host reads `.git.` as `.git`. A name is compared the way a volume
/// that folds case and normalizes opens it, whether or not this one does: `.GIT`, `.huſky` and
/// `.husky` spelled with the Kelvin sign for the k are each a protected name there.
pub fn is_protected(path: &str) -> bool {
    path.split(['/', '\\']).any(|segment| {
        let segment = segment.trim_end_matches(['.', ' ']);
        let form = crate::trust::canonical_form(segment, true);
        DIRECTORIES
            .iter()
            .chain(FILES.iter())
            .any(|name| form == *name)
    })
}

#[cfg(test)]
mod tests {
    use super::is_protected;

    #[test]
    fn a_git_component_anywhere_in_the_path_is_protected() {
        for path in [
            ".git",
            ".git/hooks/pre-commit",
            ".git/config",
            "sub/.git/hooks/post-merge",
            "/work/.git/config",
            ".GIT/config",
            ".git./config",
            ".hu\u{17f}ky/pre-commit",
            ".hus\u{212a}y/pre-commit",
            ".GITMODULES",
            ".git \\config",
            "a\\.git\\config",
            ".bravebot/settings.json",
            ".bravebot/skills/x/SKILL.md",
            ".husky/pre-commit",
            ".gitmodules",
            "vendor/.gitconfig",
            ".pre-commit-config.yaml",
        ] {
            assert!(is_protected(path), "{path} was not protected");
        }
    }

    #[test]
    fn an_ordinary_path_and_a_lookalike_are_not_protected() {
        for path in [
            "src/a.rs",
            ".gitignore",
            ".github/workflows/ci.yml",
            "git/config",
            "my.git/config",
            ".gitattributes",
            ".bravebotx/a",
            "docs/.bravebot.md",
            "",
        ] {
            assert!(!is_protected(path), "{path} was protected");
        }
    }
}
