use super::*;

/// A glob the matcher cannot read selects no files, and a search over no files reports no
/// matches, which is the sentence a search that read the whole tree and found nothing
/// prints. Saying which syntax was the problem is what keeps the two apart.
#[test]
fn a_glob_leaning_on_missing_syntax_is_named() {
    for pattern in [
        "src/[abc]*.rs",
        "**/*.[ch]",
        "!(vendor)/**",
        "**/+(a|b).rs",
        "!vendor/**",
    ] {
        assert!(
            reads_as_an_unsupported_glob(pattern).is_some(),
            "{pattern} was not recognised"
        );
    }
}

/// The advice is free on a result that found nothing anyway, but a pattern that works must
/// never be lectured at. Brace groups are supported, so they are the first thing that must
/// not fire here.
#[test]
fn a_glob_the_matcher_can_read_is_left_alone() {
    for pattern in [
        "*.rs",
        "**/*.{cc,h,mm}",
        "{src,tests}/**/*.rs",
        "crates/**/tools.rs",
        "a?.rs",
        // A lone bracket in a filename is not a character class.
        "notes[draft.md",
    ] {
        assert!(
            reads_as_an_unsupported_glob(pattern).is_none(),
            "{pattern} was flagged"
        );
    }
}

/// LIST-3: a walk skips a fixed list of directory names without reporting them, so the
/// description names every one of them. A planner that sees no `vendor` in a listing would
/// otherwise read it as absent.
#[test]
fn list_files_names_every_directory_its_walk_skips() {
    let offered = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let described = &offered
        .iter()
        .find(|t| t.function.name == "list_files")
        .expect("list_files is offered")
        .function
        .description;
    let skipped = bravebot_filetype::ignored_directories();
    assert!(skipped.contains(&"vendor") && skipped.contains(&"build"));
    for name in skipped {
        assert!(
            described.contains(&format!("{name},")),
            "list_files skips {name} and does not say so: {described}"
        );
    }
    assert!(described.contains(".claude/worktrees"));
    // LIST-6: and that it does not enter a symbolic link, which it counts and does not name.
    assert!(
        described.contains("symbolic link") && described.contains("says how many"),
        "list_files does not say links are not entered: {described}"
    );
    // LIST-3, LIST-4: and that a walk stops at a time limit and how to narrow it.
    assert!(
        described.contains("time limit") && described.contains("narrows it"),
        "list_files does not say a listing stops at a time limit: {described}"
    );
    let searched = &offered
        .iter()
        .find(|t| t.function.name == "search")
        .expect("search is offered")
        .function
        .description;
    assert!(
        searched.contains("symbolic link") && searched.contains("says how many"),
        "search does not say links are not entered: {searched}"
    );
}

/// A tool schema is the whole of what the planner is told about the matcher, so syntax
/// denied there is syntax nothing ever sends: the brace expansion the matcher performs
/// before the walk is reachable only by a planner willing to try what it was told does not
/// work. `list_files` and `search` run one matcher between them, so one account of it is
/// what they owe the planner, and a second account that disagrees is how a group the matcher
/// expands came to be advertised as missing on one of them.
#[test]
fn both_glob_arguments_describe_the_matcher_the_same_way() {
    let offered = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    // The part of a description that is about the matcher rather than about the argument.
    let syntax = |tool: &str, property: &str| -> String {
        let described = offered
            .iter()
            .find(|t| t.function.name == tool)
            .unwrap_or_else(|| panic!("{tool} is offered"))
            .function
            .parameters["properties"][property]["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{tool}.{property} is described"))
            .to_string();
        let at = described.find("Supports").unwrap_or_else(|| {
            panic!("{tool}.{property} does not say what it supports: {described}")
        });
        described[at..].to_string()
    };

    let mut expanded = crate::glob::expand("**/*.{rs,toml}");
    expanded.sort();
    assert_eq!(
        expanded,
        ["**/*.rs", "**/*.toml"],
        "the matcher no longer expands a brace group, so neither schema may offer one"
    );
    let listing = syntax("list_files", "pattern");
    assert!(
        listing.contains("** and brace groups"),
        "list_files withholds the brace groups the matcher expands: {listing}"
    );
    assert_eq!(
        listing,
        syntax("search", "include"),
        "two arguments over one matcher advertise different syntax"
    );
}
