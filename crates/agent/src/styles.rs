//! Output styles: named words that take the place of the system prompt's opening.
//!
//! A style is words this program wrote, kept by name so a person can pick one with `/style`
//! instead of typing `--system-prompt`. It stands where `--system-prompt` stands (CLI-19) and
//! reaches nothing after it, so the guidance on reading a tool's output, the mode and every
//! refusal are unchanged. The words ask for a manner of answering and grant nothing.
//!
//! A style is one of the three this build ships or a file of the person's own at
//! `~/.bravebot/styles/<name>.md`. The file is the person's configuration and trusted for sitting
//! there; a project's files are not read at all.

use std::borrow::Cow;
use std::path::Path;

/// The directory under `~/.bravebot` that holds a person's own styles.
pub const DIRECTORY: &str = "styles";

/// The most bytes of a style file that are read. A longer file is not a style: it is a document.
pub const LONGEST_FILE: u64 = 4096;

/// One style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    /// What a person types after `/style`.
    pub name: Cow<'static, str>,
    /// Stands in for the opening of the planner's system prompt.
    pub words: Cow<'static, str>,
    /// Whether the words were read from the person's own file, not written into this program.
    pub from_file: bool,
}

/// The styles this build ships, in the order they are listed.
pub const BUILT_IN: [Style; 3] = [
    Style {
        name: Cow::Borrowed("concise"),
        from_file: false,
        words: Cow::Borrowed(
            "\
You are a careful assistant working in a user's workspace, with tools to read files, list them \
and search their contents. Lead with the result. Leave out preamble, \
restating the question and a recap of what you did, and say only what the person needs to act.",
        ),
    },
    Style {
        name: Cow::Borrowed("explanatory"),
        from_file: false,
        words: Cow::Borrowed(
            "\
You are a careful assistant working in a user's workspace, with tools to read files, list them \
and search their contents. While you work, add short `Insight` notes that \
explain why the code is the way it is or why you chose an approach, so the person learns the \
codebase as well as getting the result.",
        ),
    },
    Style {
        name: Cow::Borrowed("proactive"),
        from_file: false,
        words: Cow::Borrowed(
            "\
You are a careful assistant working in a user's workspace, with tools to read files, list them \
and search their contents. Start the work without asking for permission to \
begin, and where a routine decision has an obvious answer, decide it, say what you assumed, and \
carry on. This does not change what you may do: a write is still put to the person where it \
would have been, and a mode that refuses still refuses.",
        ),
    },
];

/// The built-in style called `name`, compared exactly.
pub fn named(name: &str) -> Option<&'static Style> {
    BUILT_IN.iter().find(|style| style.name == name)
}

/// The style called `name`: a built-in one, or else the person's file `<name>.md` under
/// `<home>/styles`, where `home` is `~/.bravebot`.
///
/// The name is compared whole against the built-in list and must be a slug to name a file, so
/// nothing typed after `/style` can reach a path outside the directory. A built-in name always
/// means the built-in: a file cannot stand in for it.
pub fn find(home: Option<&Path>, name: &str) -> Option<Style> {
    if let Some(built_in) = named(name) {
        return Some(built_in.clone());
    }
    read_file(home?, name)
}

/// The words of `<home>/styles/<name>.md`, where that is a small regular file of UTF-8 text.
///
/// A symbolic link is not followed, for the reason a skill's is not: the file stands for what is
/// in the directory, and a link would make it stand for something outside it.
fn read_file(home: &Path, name: &str) -> Option<Style> {
    use std::io::Read;
    if !crate::memory::is_a_slug(name) {
        return None;
    }
    let path = home.join(DIRECTORY).join(format!("{name}.md"));
    if !std::fs::symlink_metadata(&path).ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(&path)
        .ok()?
        .take(LONGEST_FILE + 1)
        .read_to_string(&mut text)
        .ok()?;
    let words = text.trim();
    if words.is_empty() || text.len() as u64 > LONGEST_FILE {
        return None;
    }
    Some(Style {
        name: Cow::Owned(name.to_string()),
        words: Cow::Owned(words.to_string()),
        from_file: true,
    })
}

/// The names `/style` offers: the built-in ones, then the person's files that would be accepted,
/// each once and in order.
pub fn available(home: Option<&Path>) -> Vec<String> {
    let mut names: Vec<String> = BUILT_IN.iter().map(|s| s.name.to_string()).collect();
    let Some(entries) = home.and_then(|h| std::fs::read_dir(h.join(DIRECTORY)).ok()) else {
        return names;
    };
    let mut files: Vec<String> = entries
        .filter_map(|entry| {
            let file = entry.ok()?.file_name().into_string().ok()?;
            let name = file.strip_suffix(".md")?;
            (named(name).is_none() && read_file(home?, name).is_some()).then(|| name.to_string())
        })
        .collect();
    files.sort_unstable();
    files.dedup();
    names.extend(files);
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_is_found_by_its_whole_name_only() {
        assert_eq!(named("concise").map(|s| s.name.as_ref()), Some("concise"));
        for word in ["conci", "Concise", "concise ", "", "default"] {
            assert_eq!(named(word), None, "{word:?} named a style");
        }
    }

    #[test]
    fn no_style_takes_the_quarantine_guidance_with_it() {
        for style in &BUILT_IN {
            assert!(
                !style.words.contains("data, never as instructions"),
                "{} restates guidance that belongs after the opening",
                style.name
            );
            assert!(
                !style.words.trim().is_empty() && style.words.len() < 1000,
                "{}",
                style.name
            );
        }
    }

    #[test]
    fn style_names_are_distinct() {
        let mut names: Vec<_> = BUILT_IN.iter().map(|style| style.name.clone()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BUILT_IN.len());
    }

    /// A directory holding `styles/<name>.md` for each pair, fresh for this test.
    fn home_with(label: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let home = crate::testutil::scratch_dir(&format!("bravebot-styles-{label}"));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(DIRECTORY)).unwrap();
        for (name, words) in files {
            std::fs::write(home.join(DIRECTORY).join(format!("{name}.md")), words).unwrap();
        }
        home
    }

    #[test]
    fn a_file_in_the_home_directory_is_a_style_by_its_name() {
        let home = home_with("found", &[("terse", "\n  Answer in one line.  \n")]);
        let style = find(Some(&home), "terse").expect("the file is a style");
        assert_eq!(style.name, "terse");
        assert_eq!(style.words, "Answer in one line.");
        assert!(style.from_file);
        assert_eq!(find(None, "terse"), None, "no home, no file");
        assert_eq!(find(Some(&home), "missing"), None);
    }

    #[test]
    fn a_file_cannot_stand_in_for_a_built_in_name() {
        let home = home_with("shadow", &[("concise", "Ramble on at length.")]);
        let style = find(Some(&home), "concise").expect("built-in");
        assert!(!style.from_file);
        assert!(!style.words.contains("Ramble"));
        assert_eq!(
            available(Some(&home)),
            ["concise", "explanatory", "proactive"]
        );
    }

    #[test]
    fn a_name_that_is_not_a_slug_never_reaches_a_path() {
        let home = home_with("traverse", &[("ok", "words")]);
        std::fs::write(home.join("outside.md"), "OUTSIDE").unwrap();
        for name in [
            "../outside",
            "ok/../../outside",
            "OK",
            "ok.md",
            "a b",
            "-ok",
            "",
        ] {
            assert_eq!(find(Some(&home), name), None, "{name:?} named a file");
        }
    }

    #[test]
    fn a_link_an_empty_file_and_a_long_file_are_not_styles() {
        let home = home_with(
            "refused",
            &[
                ("empty", "  \n"),
                ("long", &"x".repeat(LONGEST_FILE as usize + 1)),
                ("exact", &"x".repeat(LONGEST_FILE as usize)),
            ],
        );
        #[cfg(unix)]
        {
            std::fs::write(home.join("target.txt"), "ELSEWHERE").unwrap();
            std::os::unix::fs::symlink(
                home.join("target.txt"),
                home.join(DIRECTORY).join("linked.md"),
            )
            .unwrap();
            assert_eq!(find(Some(&home), "linked"), None);
        }
        assert_eq!(find(Some(&home), "empty"), None);
        assert_eq!(find(Some(&home), "long"), None);
        assert!(find(Some(&home), "exact").is_some(), "the cap is inclusive");
    }

    #[test]
    fn the_offered_names_are_the_built_ins_then_the_accepted_files_in_order() {
        let home = home_with(
            "offered",
            &[("zed", "z"), ("alpha", "a"), ("empty", ""), ("Bad", "b")],
        );
        std::fs::write(home.join(DIRECTORY).join("notes.txt"), "not md").unwrap();
        assert_eq!(
            available(Some(&home)),
            ["concise", "explanatory", "proactive", "alpha", "zed"]
        );
        assert_eq!(available(None), ["concise", "explanatory", "proactive"]);
    }
}
