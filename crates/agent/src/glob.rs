//! Glob matching for workspace paths.
//!
//! Listings and searches need narrowing, say "just the Rust files under crates", or a model
//! spends a step reading a listing it mostly does not want.
//!
//! A glob rather than a regular expression, and hand-written rather than a dependency, for
//! the same reason `search` matches literally: patterns arrive through a turn, and a
//! backtracking engine turns a pattern into a denial-of-service vector. The matcher below
//! runs in time proportional to the path length times the pattern length, with no
//! backtracking and no recursion, so a hostile pattern costs nothing unusual.
//!
//! Supported, and nothing else:
//!
//! - `?` matches one character within a segment
//! - `*` matches any run of characters within a segment
//! - `**` matches across segment boundaries
//! - `{a,b}` matches either alternative, and may be nested
//!
//! A pattern with no `/` matches against the file name alone, so `*.rs` finds Rust files at
//! any depth, which is the reading a person intends when they type it.
//!
//! Brace groups are expanded before matching rather than matched, which is what keeps the
//! guarantee above: each alternative is an ordinary pattern walked once, so a group costs a
//! multiple of the work and never a power of it. [`expand`] is separate from [`matches`]
//! because a walk applies one pattern to thousands of paths, and expanding once at the top
//! is the difference between one allocation and one per file.

/// How many patterns one expansion may produce.
///
/// Groups multiply: `{a,b}{c,d}{e,f}` is eight patterns and ten such groups are a thousand.
/// Past the cap the pattern is matched literally, which finds nothing and is *reported* as
/// finding nothing, rather than quietly becoming a walk nobody asked for.
const MAX_EXPANSIONS: usize = 64;

/// Whether `path` matches `pattern`.
///
/// `path` is expected to use `/` separators, as workspace-relative paths do.
///
/// Expands on every call. Use [`expand`] with [`matches_any`] where the same pattern is
/// applied to more than a handful of paths.
pub fn matches(pattern: &str, path: &str) -> bool {
    matches_any(&expand(pattern), path)
}

/// Whether `path` matches any of `patterns`, as produced by [`expand`].
pub fn matches_any(patterns: &[String], path: &str) -> bool {
    patterns.iter().any(|pattern| matches_single(pattern, path))
}

/// Whether a file beneath the directory `directory` could match any of `patterns`, as produced by
/// [`expand`]. A walk that gets `false` need not open that directory.
///
/// Decided from the patterns and the directory's own relative name alone. It answers `false` only
/// where no path that begins with `directory/` can match, so it never hides a file `matches_any`
/// would have kept. Where it cannot tell it answers `true`:
///
/// - a pattern with no `/` is about the file name at any depth;
/// - a pattern whose first segment holds a wildcard (`*`, `?` or `**`) is not anchored at a
///   directory the walk can name, so it prunes nothing;
/// - a pattern still holding a brace is the literal an oversized expansion falls back to;
/// - no pattern at all is an empty pattern, which selects nothing and is left to the walk.
///
/// Where the first segment is literal, the segments after it are followed exactly: a segment with
/// a `*` or `?` matches the directory names it can match and no others, and `**` matches any
/// number of them.
pub fn could_match_beneath(patterns: &[String], directory: &str) -> bool {
    patterns.is_empty()
        || patterns
            .iter()
            .any(|pattern| could_match_beneath_single(pattern, directory))
}

/// [`could_match_beneath`] for one already-expanded pattern.
fn could_match_beneath_single(pattern: &str, directory: &str) -> bool {
    if !pattern.contains('/') || pattern.contains(['{', '}']) {
        return true;
    }
    let segments: Vec<&str> = pattern.split('/').collect();
    if segments
        .first()
        .is_none_or(|first| first.contains(['*', '?']))
    {
        return true;
    }

    // The pattern positions the directory's segments so far can have reached. After a position's
    // `**` the next segment's position is reached too, because `**` may match nothing.
    let mut reached = vec![false; segments.len() + 1];
    reached[0] = true;
    close_over_double_stars(&segments, &mut reached);
    for name in directory.split('/') {
        let mut next = vec![false; segments.len() + 1];
        for (at, segment) in segments.iter().enumerate() {
            if !reached[at] {
                continue;
            }
            if *segment == "**" {
                next[at] = true;
            } else if matches_one(segment, name) {
                next[at + 1] = true;
            }
        }
        close_over_double_stars(&segments, &mut next);
        reached = next;
    }

    // Some position is short of the end of the pattern and has only segments a name can satisfy
    // left, so something beneath the directory can still finish the match. A path segment is never
    // empty, so an empty pattern segment is one nothing can satisfy. `tail_ok[at]` is whether every
    // segment from `at` on is satisfiable, computed once from the end so the check is linear.
    let mut tail_ok = vec![true; segments.len() + 1];
    for at in (0..segments.len()).rev() {
        tail_ok[at] = (segments[at] == "**" || !segments[at].is_empty()) && tail_ok[at + 1];
    }
    reached
        .iter()
        .enumerate()
        .any(|(at, reached)| *reached && at < segments.len() && tail_ok[at])
}

fn close_over_double_stars(segments: &[&str], reached: &mut [bool]) {
    for at in 0..segments.len() {
        if reached[at] && segments[at] == "**" {
            reached[at + 1] = true;
        }
    }
}

/// Expand brace groups into the plain patterns they stand for.
///
/// An empty pattern expands to nothing, so it matches nothing. A pattern with no group, an
/// unbalanced one, or one that would exceed [`MAX_EXPANSIONS`] comes back as itself: a
/// half-expanded pattern would match a set nobody wrote, and matching the literal is at
/// least a result the writer can recognise as wrong.
pub fn expand(pattern: &str) -> Vec<String> {
    if pattern.is_empty() {
        return Vec::new();
    }

    let mut done: Vec<String> = Vec::new();
    let mut pending: Vec<String> = vec![pattern.to_string()];

    while let Some(current) = pending.pop() {
        let Some((prefix, alternatives, suffix)) = split_first_group(&current) else {
            done.push(current);
            continue;
        };

        // Counted before anything is pushed, so the cap bounds what is built rather than
        // catching it afterwards.
        if done.len() + pending.len() + alternatives.len() > MAX_EXPANSIONS {
            return vec![pattern.to_string()];
        }

        // A nested group is left in place and handled when the result comes back around.
        for alternative in alternatives {
            pending.push(format!("{prefix}{alternative}{suffix}"));
        }
    }

    done
}

/// Split out the first brace group, as (before, alternatives, after).
///
/// `None` where there is no group or the braces do not balance. Nesting is counted rather
/// than assumed away, so the `}` that closes `{a,{b,c}}` is the last one and not the first.
fn split_first_group(pattern: &str) -> Option<(&str, Vec<&str>, &str)> {
    let open = pattern.find('{')?;

    let mut depth = 0usize;
    let mut close = None;
    // Byte indices, and both braces are ASCII, so slicing on them lands on a boundary.
    for (index, character) in pattern[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + index);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;

    let alternatives = split_alternatives(&pattern[open + 1..close]);
    Some((&pattern[..open], alternatives, &pattern[close + 1..]))
}

/// Split a group's interior on the commas that belong to it, ignoring nested ones.
///
/// `{a,{b,c}}` has two alternatives, not three: the inner comma is the inner group's.
fn split_alternatives(interior: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (index, character) in interior.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&interior[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    out.push(&interior[start..]);
    out
}

/// Whether `path` matches one already-expanded pattern.
fn matches_single(pattern: &str, path: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }

    // A bare name pattern is about the file, not its location. Without this, `*.rs` would
    // match nothing in a tree of any depth, which is never what a person means by it.
    let subject = if pattern.contains('/') {
        path
    } else {
        path.rsplit('/').next().unwrap_or(path)
    };

    matches_segments(pattern, subject)
}

/// Match segment by segment, so `**` is the only thing that can cross a `/`.
///
/// Matching per segment rather than over the whole string is what keeps the two kinds of
/// wildcard from interfering: a single `*` never sees a separator to begin with, so it
/// cannot be tempted across one, and `**` is resolved at the segment level where its
/// meaning is defined.
fn matches_segments(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();

    let (mut pi, mut si) = (0usize, 0usize);
    // Where to resume if the current `**` guess turns out to be too short.
    let mut star: Option<(usize, usize)> = None;

    while si < path.len() {
        if pi < pattern.len() && pattern[pi] == "**" {
            // Try consuming nothing first, and remember to consume more on failure.
            star = Some((pi, si));
            pi += 1;
            continue;
        }

        if pi < pattern.len() && matches_one(pattern[pi], path[si]) {
            pi += 1;
            si += 1;
            continue;
        }

        match star {
            Some((star_pi, star_si)) => {
                // Let the `**` swallow one more segment and retry from just after it.
                star = Some((star_pi, star_si + 1));
                pi = star_pi + 1;
                si = star_si + 1;
            }
            None => return false,
        }
    }

    // A trailing `**` may match no segments at all.
    while pi < pattern.len() && pattern[pi] == "**" {
        pi += 1;
    }

    pi == pattern.len()
}

/// Match one path segment against one pattern segment, where `*` and `?` cannot escape it.
///
/// The two-pointer walk resumes after the last `*` on a mismatch rather than branching, so
/// a pattern crafted to backtrack stays linear in the product of the lengths.
fn matches_one(pattern: &str, segment: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = segment.chars().collect();

    let (mut pi, mut si) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;

    while si < s.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some((pi, si));
            pi += 1;
            continue;
        }
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
            continue;
        }
        match star {
            Some((star_pi, star_si)) => {
                star = Some((star_pi, star_si + 1));
                pi = star_pi + 1;
                si = star_si + 1;
            }
            None => return false,
        }
    }

    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }

    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_name_matches_itself() {
        assert!(matches("main.rs", "main.rs"));
        assert!(!matches("main.rs", "other.rs"));
    }

    /// The reading a person intends: a bare pattern is about the file name, so it finds
    /// matches at any depth.
    #[test]
    fn a_bare_pattern_matches_at_any_depth() {
        assert!(matches("*.rs", "main.rs"));
        assert!(matches("*.rs", "crates/agent/src/tools.rs"));
        assert!(!matches("*.rs", "crates/agent/Cargo.toml"));
    }

    /// A pattern with a separator is about the path, so it anchors.
    #[test]
    fn a_path_pattern_anchors_at_the_root() {
        assert!(matches("src/*.rs", "src/main.rs"));
        assert!(!matches("src/*.rs", "other/main.rs"));
        // A single star must not leap a directory boundary.
        assert!(!matches("src/*.rs", "src/deep/main.rs"));
    }

    #[test]
    fn a_double_star_crosses_directories() {
        assert!(matches("src/**/*.rs", "src/a/b/c.rs"));
        assert!(matches("**/*.rs", "a/b/c.rs"));
        assert!(matches("crates/**/tools.rs", "crates/agent/src/tools.rs"));
    }

    /// `**` must also match no directories at all, or `src/**/x.rs` surprises by missing
    /// `src/x.rs`.
    #[test]
    fn a_double_star_matches_nothing_too() {
        assert!(matches("src/**/x.rs", "src/x.rs"));
        assert!(matches("**/x.rs", "x.rs"));
    }

    /// A trailing `**` names a subtree.
    #[test]
    fn a_trailing_double_star_matches_a_subtree() {
        assert!(matches("src/**", "src/a/b.rs"));
        assert!(matches("src/**", "src/a.rs"));
        assert!(!matches("src/**", "other/a.rs"));
    }

    #[test]
    fn a_question_mark_matches_one_character() {
        assert!(matches("a?.rs", "ab.rs"));
        assert!(!matches("a?.rs", "abc.rs"));
        // But never a separator, which would let it escape its segment. Asked of a path pattern
        // as well as a bare one: a bare pattern is matched against the file name, which holds no
        // separator for a `?` to reach in the first place.
        assert!(!matches("a?b", "a/b"));
        assert!(!matches("src/a?b", "src/a/b"));
    }

    /// The spelling everybody reaches for. It used to be matched literally, which found
    /// nothing and read as proof the files were absent.
    #[test]
    fn a_brace_group_matches_each_alternative() {
        assert!(matches("*.{ts,tsx}", "a.ts"));
        assert!(matches("*.{ts,tsx}", "a.tsx"));
        assert!(!matches("*.{ts,tsx}", "a.js"));
        assert!(matches("**/*.{cc,h,mm}", "ios/browser/policy/map.h"));
    }

    /// Groups multiply rather than add, and a group may sit anywhere in the pattern.
    #[test]
    fn groups_combine() {
        assert!(matches("{src,tests}/**/*.{rs,toml}", "src/a/b.rs"));
        assert!(matches("{src,tests}/**/*.{rs,toml}", "tests/Cargo.toml"));
        assert!(!matches("{src,tests}/**/*.{rs,toml}", "docs/a/b.rs"));
    }

    #[test]
    fn a_nested_group_expands_from_the_inside() {
        let mut expanded = expand("a{b,c{d,e}}f");
        expanded.sort();
        assert_eq!(expanded, ["abf", "acdf", "acef"]);
    }

    /// An alternative may be empty, which is how an optional piece is written.
    #[test]
    fn an_empty_alternative_stands_for_nothing() {
        assert!(matches("a{,-test}.rs", "a.rs"));
        assert!(matches("a{,-test}.rs", "a-test.rs"));
    }

    /// Half-expanding is worse than not expanding: the pattern would match a set nobody
    /// wrote. Matching the literal at least fails in a way its author can recognise.
    #[test]
    fn an_unbalanced_brace_is_matched_literally() {
        assert_eq!(expand("*.{ts"), ["*.{ts"]);
        assert!(matches("*.{ts", "a.{ts"));
        assert!(!matches("*.{ts", "a.ts"));
    }

    /// The cap is what stops a pattern from becoming a walk of its own.
    #[test]
    fn an_oversized_expansion_falls_back_to_the_literal() {
        // Seven groups of two is 128, past the cap of 64.
        let pattern = "{a,b}{a,b}{a,b}{a,b}{a,b}{a,b}{a,b}";
        assert_eq!(expand(pattern), [pattern]);

        // Six is 64, which fits, and is the boundary worth pinning.
        let ok = "{a,b}{a,b}{a,b}{a,b}{a,b}{a,b}";
        assert_eq!(expand(ok).len(), 64);
    }

    #[test]
    fn a_pattern_without_a_group_expands_to_itself() {
        assert_eq!(expand("**/*.rs"), ["**/*.rs"]);
        assert!(expand("").is_empty());
    }

    #[test]
    fn an_empty_pattern_matches_nothing() {
        assert!(!matches("", "a.rs"));
        assert!(!matches("", ""));
    }

    #[test]
    fn a_star_matches_an_empty_run() {
        assert!(matches("a*", "a"));
        assert!(matches("*a", "a"));
        assert!(matches("*", "anything"));
    }

    /// The property that makes this safe to expose: a pattern built to cause backtracking
    /// must still return promptly. Without the two-pointer walk this is exponential.
    #[test]
    fn a_pathological_pattern_does_not_blow_up() {
        let pattern = "*a*a*a*a*a*a*a*a*a*a*a*a*a*a*a*a*b";
        let path = "a".repeat(2_000);
        assert!(!matches(pattern, &path));
    }

    /// The directory a pattern's literal prefix names is entered and a sibling is not.
    #[test]
    fn a_literal_prefix_rules_out_a_sibling_directory() {
        let patterns = expand(".somefolder/**");
        assert!(could_match_beneath(&patterns, ".somefolder"));
        assert!(could_match_beneath(&patterns, ".somefolder/deep/er"));
        assert!(!could_match_beneath(&patterns, "other"));
        assert!(!could_match_beneath(&patterns, "other/.somefolder"));
        // A name that only starts like the prefix is another directory.
        assert!(!could_match_beneath(&patterns, ".somefolder2"));
    }

    /// A segment holding a wildcard matches the names it can match and no others, so a directory is
    /// pruned only when its name cannot be matched by that segment.
    #[test]
    fn a_wildcard_segment_is_followed_exactly() {
        let patterns = expand("src/*/lib.rs");
        assert!(could_match_beneath(&patterns, "src"));
        assert!(could_match_beneath(&patterns, "src/anything"));
        // `lib.rs` is the last segment, which names a file, and `*` takes one level only.
        assert!(!could_match_beneath(&patterns, "src/anything/else"));
        assert!(!could_match_beneath(&patterns, "tests"));

        let patterns = expand("crates/agent-*/src/**");
        assert!(could_match_beneath(&patterns, "crates"));
        assert!(could_match_beneath(&patterns, "crates/agent-core"));
        assert!(could_match_beneath(&patterns, "crates/agent-core/src/deep"));
        assert!(!could_match_beneath(&patterns, "crates/tui"));
        assert!(!could_match_beneath(&patterns, "crates/agent-core/tests"));

        let patterns = expand("x/a?/b/**");
        assert!(could_match_beneath(&patterns, "x/ab"));
        assert!(!could_match_beneath(&patterns, "x/abc"));
        // A first segment with a wildcard is not a prefix the walk can name, so nothing is pruned.
        assert!(could_match_beneath(&expand("a?/b/**"), "abc"));
    }

    /// `**` in the middle stands for any number of directories, including none.
    #[test]
    fn a_double_star_keeps_every_depth_below_its_prefix() {
        let patterns = expand("docs/**/*.md");
        assert!(could_match_beneath(&patterns, "docs"));
        assert!(could_match_beneath(&patterns, "docs/a/b/c"));
        assert!(!could_match_beneath(&patterns, "src"));
    }

    /// A pattern that does not start with a literal directory can match under any directory, and
    /// the walk is not cut for it.
    #[test]
    fn a_pattern_with_no_literal_directory_prefix_prunes_nothing() {
        for pattern in [
            "*.rs",
            "readme*",
            "*/profile.json",
            "**/*.rs",
            "?/x/y.rs",
            "{a,b}",
        ] {
            assert!(
                could_match_beneath(&expand(pattern), "anywhere/at/all"),
                "{pattern} was used to skip a directory"
            );
        }
        // Past the cap on expansions the pattern is the literal, braces included, and nothing is
        // concluded from it.
        let oversized = "{a,b}{a,b}{a,b}{a,b}{a,b}{a,b}{a,b}/x";
        assert!(could_match_beneath(&expand(oversized), "z"));
        // An empty pattern expands to nothing and decides nothing.
        assert!(could_match_beneath(&expand(""), "z"));
    }

    /// Each brace alternative is a pattern of its own, and a directory one of them can reach is
    /// kept.
    #[test]
    fn every_brace_alternative_may_keep_a_directory() {
        let patterns = expand("{src,tests}/**/*.rs");
        assert!(could_match_beneath(&patterns, "src/a"));
        assert!(could_match_beneath(&patterns, "tests"));
        assert!(!could_match_beneath(&patterns, "docs"));
        // One alternative is bare of a directory, so it may match under any.
        let mixed = expand("{src/**,*.rs}");
        assert!(could_match_beneath(&mixed, "docs"));
    }

    /// A pattern that ends where the directory does leaves nothing for a file to be.
    #[test]
    fn a_pattern_whose_last_segment_is_a_directory_name_has_no_file_beneath_it() {
        let patterns = expand("a/b");
        assert!(could_match_beneath(&patterns, "a"));
        assert!(!could_match_beneath(&patterns, "a/b"));
    }

    #[test]
    fn matching_is_case_sensitive() {
        assert!(!matches("*.RS", "main.rs"));
        assert!(matches("*.rs", "main.rs"));
    }

    const SAMPLE_FILES: &[&str] = &[
        "README.md",
        "Cargo.toml",
        "main.rs",
        "lib.rs",
        "src/lib.rs",
        "src/main.rs",
        "src/util/mod.rs",
        "src/util/deep/x.rs",
        "src/a/lib.rs",
        "src/b/lib.rs",
        "src/b/other.rs",
        "crates/agent/Cargo.toml",
        "crates/agent/src/lib.rs",
        "crates/agent/src/glob.rs",
        "crates/agent/src/deep/er/x.rs",
        "crates/agent-core/src/lib.rs",
        "crates/agent-core/src/deep/x.rs",
        "crates/agent-core/tests/t.rs",
        "crates/tui/Cargo.toml",
        "crates/tui/src/lib.rs",
        "crates/tui/src/ui/view.rs",
        "docs/readme.md",
        "docs/specs/labels.md",
        "docs/specs/tools/run.md",
        "docs/specs/tools/notes.txt",
        "docs/guide/a/b/c/d.md",
        "tests/a.rs",
        "tests/Cargo.toml",
        "tests/deep/er/b.rs",
        "a/b",
        "a/b/c.rs",
        "a/x/b/c.rs",
        "a/x/b",
        "a/x/c.rs",
        "a/x/y/b",
        "a/x/y/c.rs",
        "a/x/y/z/c.rs",
        "a/x/y/z/b",
        "x/ab/b/f.rs",
        "x/ab/b/g/h.rs",
        "x/ab/c/f.rs",
        "x/abc/b/f.rs",
        "x/a/b/f.rs",
        "x/ax/b/k",
        "x/ax/b/deep/k",
        "other/lib.rs",
        "other/src/lib.rs",
        "other/docs/a.md",
    ];

    /// Patterns whose first segment is a literal directory, so a walk can prune by them.
    const LITERAL_PREFIX_PATTERNS: &[&str] = &[
        "src/lib.rs",
        "src/*/lib.rs",
        "src/**",
        "crates/agent-*/src/**",
        "crates/agent/src/*.rs",
        "x/a?/b/**",
        "docs/**/*.md",
        "docs/specs/*",
        "a/**/b",
        "a/**/**/c.rs",
        "a/**/b/*.rs",
        "a/b",
        "{src,tests}/**/*.rs",
        "crates/{agent,tui}/src/*.rs",
        "{a,docs}/**/{b,*.md}",
    ];

    /// Patterns with an empty segment, which select nothing and so keep no directory.
    const EMPTY_SEGMENT_PATTERNS: &[&str] = &["src/", "a/", "a//b", "a/**//b"];

    /// Patterns that name no literal directory first, or none at all, so a walk prunes nothing.
    const OTHER_PATTERNS: &[&str] = &[
        "*.rs",
        "Cargo.toml",
        "lib.rs",
        "**/lib.rs",
        "**/specs/*.md",
        "**",
        "*/src/lib.rs",
        "?/b",
    ];

    /// The `(pruned, kept)` counts of `(pattern, directory)` pairs, after asserting that no file
    /// beneath a pruned directory matches the pattern.
    fn check_pruning_is_sound(patterns: &[&str]) -> (usize, usize) {
        let mut directories = std::collections::BTreeSet::new();
        for file in SAMPLE_FILES {
            let segments: Vec<&str> = file.split('/').collect();
            for depth in 1..segments.len() {
                directories.insert(segments[..depth].join("/"));
            }
        }
        let (mut pruned, mut kept) = (0, 0);
        for pattern in patterns {
            let expanded = expand(pattern);
            for directory in &directories {
                if could_match_beneath(&expanded, directory) {
                    kept += 1;
                    continue;
                }
                pruned += 1;
                let beneath = format!("{directory}/");
                for file in SAMPLE_FILES {
                    assert!(
                        !file.starts_with(&beneath) || !matches_any(&expanded, file),
                        "{pattern} skipped {directory}, which holds the match {file}"
                    );
                }
            }
        }
        (pruned, kept)
    }

    /// Pruning a directory must never hide a file the pattern selects.
    #[test]
    fn pruning_never_hides_a_matching_file() {
        let (literal_pruned, literal_kept) = check_pruning_is_sound(LITERAL_PREFIX_PATTERNS);
        let (other_pruned, other_kept) = check_pruning_is_sound(OTHER_PATTERNS);
        assert!(
            literal_pruned > 0,
            "no directory was pruned for a literal prefix"
        );
        assert!(literal_kept > 0, "every directory was pruned");
        assert_eq!(other_pruned, 0, "a pattern with no literal prefix pruned");
        assert!(other_kept > 0);
        let (empty_pruned, empty_kept) = check_pruning_is_sound(EMPTY_SEGMENT_PATTERNS);
        assert!(empty_pruned > 0);
        assert_eq!(
            empty_kept, 0,
            "a pattern with an empty segment kept a directory"
        );

        // Each literal-prefix pattern prunes something and keeps something, so each was exercised
        // in both directions.
        for pattern in LITERAL_PREFIX_PATTERNS {
            let (pruned, kept) = check_pruning_is_sound(&[pattern]);
            assert!(pruned > 0, "{pattern} pruned no directory");
            assert!(kept > 0, "{pattern} kept no directory");
        }
    }

    /// A path segment is never empty, so a pattern with an empty segment selects no file and no
    /// directory has anything beneath it that could finish the match.
    #[test]
    fn an_empty_pattern_segment_can_never_be_satisfied() {
        let trailing = expand("src/");
        assert!(!could_match_beneath(&trailing, "src"));
        assert!(!could_match_beneath(&trailing, "src/deep"));
        let inner = expand("a//b");
        assert!(!could_match_beneath(&inner, "a"));
        assert!(!could_match_beneath(&inner, "a/x"));
        // The same pattern without the empty segment keeps the directory.
        assert!(could_match_beneath(&expand("a/b"), "a"));
        // An empty segment after a `**` is still unsatisfiable.
        assert!(!could_match_beneath(&expand("a/**//b"), "a/x"));

        for file in SAMPLE_FILES {
            assert!(!matches_any(&trailing, file), "src/ matched {file}");
            assert!(!matches_any(&inner, file), "a//b matched {file}");
        }
    }

    /// A pattern with thousands of `**` segments is answered in one pass over its segments.
    #[test]
    fn a_pattern_with_many_double_stars_is_answered() {
        let pattern = format!("a/{}x", "**/".repeat(2_000));
        assert!(could_match_beneath(&expand(&pattern), "a/b/c"));
    }
}
