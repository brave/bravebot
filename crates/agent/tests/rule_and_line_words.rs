//! A `deny` rule read as a typed line, and the same text compiled as a line (PERM-5).
//!
//! `typed_line` in `crates/core/src/permissions.rs` resolves quotes, backslash escapes and runs of
//! white space the way the line compiler in `crates/agent/src/cmdline.rs` does. `bravebot-core`
//! cannot call `bravebot-agent`, so the two are separate implementations and nothing but this file
//! builds a rule and compiles a line from one text. Each test here takes the argv the compiler
//! produced from a text, which it hands to the rules before the program is looked up, and asks the
//! rule written with that text whether it covers it.
//!
//! The first two tests are the cases where the two readers have to agree. The rest are the cases
//! where they differ by design. Those are asserted as differences, so a change to either reader
//! that moves a difference fails here and is not silent.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_agent::cmdline::{Reason, Stopped, compile};
use bravebot_core::permissions::{Anchors, Decision, Permissions, Rejected, Ruling, Unreadable};
use std::fs;
use std::path::Path;

/// A permissions list holding one `deny` rule written as `Bash(<text>)`, which has to parse
/// without a report.
fn deny_rule(text: &str) -> Permissions {
    let (permissions, rejected) =
        Permissions::parse(&[format!("Bash({text})")], &[], &[], &Anchors::none());
    assert!(
        rejected.is_empty(),
        "`Bash({text})` should parse without a report: {rejected:?}"
    );
    permissions
}

/// What the line compiler did with a line.
struct Compiled {
    /// The program word and arguments of each step, in the order the compiler asked the rules
    /// about them. The rules are asked once the argv is final and before the program is looked
    /// up, so this holds the argv of a program that does not exist on this machine too.
    argvs: Vec<Vec<String>>,
    /// Why the compile stopped, where it did.
    stopped: Option<Stopped>,
}

impl Compiled {
    /// The reason the compiler refused the line, where it refused for one.
    fn refusal(&self) -> Option<&Reason> {
        match &self.stopped {
            Some(Stopped::Compile(refused)) => Some(&refused.reason),
            _ => None,
        }
    }
}

fn compiled(line: &str, directory: &Path, home: Option<&Path>) -> Compiled {
    let mut argvs = Vec::new();
    let outcome = compile(
        line,
        directory,
        home,
        &mut |program: &str, args: &[String]| {
            let mut argv = vec![program.to_string()];
            argv.extend(args.iter().cloned());
            argvs.push(argv);
            Ok(())
        },
    );
    Compiled {
        argvs,
        stopped: outcome.err(),
    }
}

fn argv_of(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_string()).collect()
}

fn denies(rule: &Permissions, argv: &[String]) -> bool {
    rule.for_command(argv) == Decision::Ruled(Ruling::Deny)
}

/// `argv` with its last word made a different word, which a rule written without a `*` must not
/// cover.
fn with_another_last_word(argv: &[String]) -> Vec<String> {
    let mut other = argv.to_vec();
    if let Some(last) = other.last_mut() {
        last.push_str("-other");
    }
    other
}

/// The line is one command that compiled to this argv, for a program that may not be installed.
fn assert_compiled_to(text: &str, built: &Compiled, expected: &[String]) {
    assert_eq!(
        built.argvs,
        [expected.to_vec()],
        "the argv the line compiler built from `{text}`"
    );
    assert!(
        matches!(built.refusal(), None | Some(Reason::NotFound)),
        "`{text}` should compile, or stop only at a program this machine does not have: {:?}",
        built.stopped
    );
}

/// A rule written the way a line is typed covers the argv that line compiles to.
///
/// A rule that read a quote, an escape or a run of spaces differently from the compiler would
/// cover some other command than the one the compiler runs, and the rule would stop nothing it was
/// written for. An implementation that keeps the characters of the rule as typed (the quotes, the
/// backslash before a space, every space of a run) passes the unit tests that name a few shapes
/// and fails the row it reads differently. The last column of each row is the argv the compiler
/// has to produce, so a change on the compiler's side fails on its own row and not only on the
/// comparison. The rule must not cover the same argv with its last word changed, which rejects a
/// reading that turns every shape into a wildcard.
#[test]
fn a_deny_rule_covers_the_argv_its_text_compiles_to_as_a_line() {
    let rows: Vec<(&str, Vec<&str>)> = vec![
        ("git push origin", vec!["git", "push", "origin"]),
        (r"tool 'x y'", vec!["tool", "x y"]),
        (r#"tool "x y""#, vec!["tool", "x y"]),
        (r#"echo "a\"b""#, vec!["echo", r#"a"b"#]),
        (r#"echo "a\\b""#, vec!["echo", r"a\b"]),
        (r#"echo "a\$b""#, vec!["echo", "a$b"]),
        (r#"echo "a\b""#, vec!["echo", r"a\b"]),
        (r"echo '\'", vec!["echo", r"\"]),
        (r"git\ push origin", vec!["git push", "origin"]),
        (r"echo a\ b", vec!["echo", "a b"]),
        (r#"echo \"x"#, vec!["echo", r#""x"#]),
        (r#"echo 'a'"b"c"#, vec!["echo", "abc"]),
        ("git   push   origin", vec!["git", "push", "origin"]),
        ("git\tpush\torigin", vec!["git", "push", "origin"]),
        ("git \t push", vec!["git", "push"]),
        ("tool ''", vec!["tool", ""]),
        (r#"tool """#, vec!["tool", ""]),
        (r#""my tool" x"#, vec!["my tool", "x"]),
        (r#"sh -c "rm x""#, vec!["sh", "-c", "rm x"]),
        ("caf\u{e9} x", vec!["caf\u{e9}", "x"]),
        ("cafe\u{301} x", vec!["cafe\u{301}", "x"]),
        ("echo '$HOME'", vec!["echo", "$HOME"]),
        (r#"echo "$""#, vec!["echo", "$"]),
        ("echo '`id`'", vec!["echo", "`id`"]),
        (r#"echo "hi!""#, vec!["echo", "hi!"]),
        ("echo 'hi!'", vec!["echo", "hi!"]),
        ("ls a~b", vec!["ls", "a~b"]),
        ("ls [", vec!["ls", "["]),
    ];
    for (text, expected) in rows {
        let expected = argv_of(&expected);
        let directory = tempfile::tempdir().unwrap();
        let built = compiled(text, directory.path(), None);
        assert_compiled_to(text, &built, &expected);

        let rule = deny_rule(text);
        assert!(
            denies(&rule, &expected),
            "the rule `Bash({text})` should cover the argv the line compiled to: {expected:?}"
        );
        let other = with_another_last_word(&expected);
        assert!(
            !denies(&rule, &other),
            "the rule `Bash({text})` should not cover {other:?}"
        );
    }
}

/// A rule covers the program word in either Unicode spelling and in any case, and the compiler
/// keeps the spelling the line used.
///
/// The permissions spec says a `deny` rule covers the program word in either normalisation form
/// and without regard to case, because the volume starts one file under every one of them. A rule
/// that compared the program word as written would let `cafe\u{301} x` through a rule typed
/// `caf\u{e9} x`, and one that folded case only for ASCII programs would let `TOUCH x` through.
/// Each row is the rule text, the line, and the argv the compiler has to build from the line.
#[test]
fn a_deny_rule_covers_the_program_the_line_spells_in_another_form() {
    let rows: Vec<(&str, &str, Vec<&str>)> = vec![
        ("caf\u{e9} x", "cafe\u{301} x", vec!["cafe\u{301}", "x"]),
        ("cafe\u{301} x", "caf\u{e9} x", vec!["caf\u{e9}", "x"]),
        ("touch x", "TOUCH x", vec!["TOUCH", "x"]),
        ("TOUCH x", "touch x", vec!["touch", "x"]),
        ("touch x", "Touch x", vec!["Touch", "x"]),
    ];
    for (rule_text, line, expected) in rows {
        let expected = argv_of(&expected);
        let directory = tempfile::tempdir().unwrap();
        let built = compiled(line, directory.path(), None);
        assert_compiled_to(line, &built, &expected);

        let rule = deny_rule(rule_text);
        assert!(
            denies(&rule, &expected),
            "the rule `Bash({rule_text})` should cover the line `{line}`: {expected:?}"
        );
        let other = with_another_last_word(&expected);
        assert!(
            !denies(&rule, &other),
            "the rule `Bash({rule_text})` should not cover {other:?}"
        );
    }
}

/// A shell operator ends a word in a line and is a character of the word in a rule.
///
/// `echo a|b` is two commands to the compiler, and `typed_line` keeps `a|b` as one word. The
/// compiler ends a word at `|`, `||`, `;`, `&&`, `&>`, `<` and `>`. A rule is not a pipeline, so it
/// covers the word and does not cover the stages the compiler built. The test pins both sides, so an
/// implementation that made the rule end a word at `|` (a rule that then matched only one stage's
/// words) fails on the first assertion, and one that made the compiler read `|` as text fails on the
/// stages. A single `&` and a `(` or `)` written onto a word, or after a space, are not word ends:
/// the compiler refuses the line, and the rule keeps them as text. An implementation that made the
/// compiler split `a&b` at the `&` fails on the refusal.
///
/// The rule also must not cover the argv that reading the operator as white space would give, which
/// is what rejects a rule reader that turns an operator into a gap: the rule `Bash(echo a|b)` still
/// covers the one word `a|b`, because a rule keeps the text as written as well, so only the argv of
/// two words tells a gap from a character. Each of `| & ( ) ; < >` has a row.
#[test]
fn a_rule_keeps_a_shell_operator_as_text_where_the_line_ends_a_word_at_it() {
    let stages: Vec<(&str, Vec<Vec<&str>>)> = vec![
        ("echo a|b", vec![vec!["echo", "a"], vec!["b"]]),
        ("echo a||b", vec![vec!["echo", "a"], vec!["b"]]),
        ("echo a;b", vec![vec!["echo", "a"], vec!["b"]]),
        ("echo a&&b", vec![vec!["echo", "a"], vec!["b"]]),
        ("echo a<b", vec![vec!["echo", "a"]]),
        ("echo a>b", vec![vec!["echo", "a"]]),
        ("echo a&>b", vec![vec!["echo", "a"]]),
    ];
    for (text, expected) in stages {
        let directory = tempfile::tempdir().unwrap();
        let built = compiled(text, directory.path(), None);
        let expected: Vec<Vec<String>> = expected.iter().map(|argv| argv_of(argv)).collect();
        assert_eq!(
            built.argvs, expected,
            "the steps the line compiler built from `{text}`"
        );

        let rule = deny_rule(text);
        let word = &text["echo ".len()..];
        assert!(
            denies(&rule, &argv_of(&["echo", word])),
            "the rule `Bash({text})` should cover `echo` with the one word `{word}`"
        );
        for step in &built.argvs {
            assert_eq!(
                rule.for_command(step),
                Decision::Unmatched,
                "the rule `Bash({text})` should not cover the step {step:?} the line built"
            );
        }
    }

    let refused: Vec<(&str, Reason, &str)> = vec![
        ("echo a&b", Reason::Background, "a&b"),
        ("echo a(b", Reason::Parenthesis('('), "a(b"),
        ("echo a)b", Reason::Parenthesis(')'), "a)b"),
    ];
    for (text, reason, word) in refused {
        let directory = tempfile::tempdir().unwrap();
        let built = compiled(text, directory.path(), None);
        assert_eq!(
            built.refusal(),
            Some(&reason),
            "why the line compiler refused `{text}`"
        );
        assert!(
            built.argvs.is_empty(),
            "a refused line reaches the rules with no step: {:?}",
            built.argvs
        );
        assert!(
            denies(&deny_rule(text), &argv_of(&["echo", word])),
            "the rule `Bash({text})` should cover `echo` with the one word `{word}`"
        );
    }

    // A `(` or `)` after a space is refused as well, with the general refusal and not the one that
    // says to quote the word, since the compiler cannot tell it from a group. The rule reads it as
    // the first character of a word.
    for (text, words) in [
        ("echo a (b)", ["echo", "a", "(b)"]),
        ("echo a )", ["echo", "a", ")"]),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let built = compiled(text, directory.path(), None);
        assert_eq!(
            built.refusal(),
            Some(&Reason::Syntax("nothing this could belong to")),
            "why the line compiler refused `{text}`"
        );
        assert!(
            built.argvs.is_empty(),
            "a refused line reaches the rules with no step: {:?}",
            built.argvs
        );
        assert!(
            denies(&deny_rule(text), &argv_of(&words)),
            "the rule `Bash({text})` should cover the words {words:?}"
        );
    }

    // Reading an operator as white space would give the argv of the same words written with a space,
    // which the rule does not cover. One row for each of the seven characters, so a reader that turns
    // any one of them into a gap fails on its row.
    for operator in ['|', '&', '(', ')', ';', '<', '>'] {
        let text = format!("echo a{operator}b");
        let as_a_gap = argv_of(&["echo", "a", "b"]);
        let rule = deny_rule(&text);
        assert!(
            denies(&rule, &argv_of(&["echo", &format!("a{operator}b")])),
            "the rule `Bash({text})` should cover the one word `a{operator}b`"
        );
        assert!(
            !denies(&rule, &as_a_gap),
            "the rule `Bash({text})` reads `{operator}` as white space: it covers {as_a_gap:?}"
        );
        let spaced = deny_rule("echo a b");
        assert!(
            denies(&spaced, &as_a_gap),
            "the control rule `Bash(echo a b)` should cover {as_a_gap:?}"
        );
    }
}

/// `$`, a backtick and `!` are refused in a line and are characters in a rule, and a quote or
/// `\$` makes them text in a line as well.
///
/// A rule such as `Bash(echo $HOME)` is written about the text, and no line can be that command,
/// so the rule has to keep the characters and cover the argv that holds them. An implementation
/// that refused the rule for holding a `$` would drop the protection it was written for, and one
/// that let the compiler pass `$HOME` through would run a command the rule cannot name. The quoted
/// forms are in the first table, where both readers produce the same word.
#[test]
fn a_rule_keeps_dollar_backtick_and_bang_as_text_where_the_line_refuses_them() {
    let rows: Vec<(&str, Reason, &str)> = vec![
        ("echo $HOME", Reason::Variable, "$HOME"),
        ("echo $(id)", Reason::CommandSubstitution, "$(id)"),
        ("echo `id`", Reason::CommandSubstitution, "`id`"),
        ("echo hi!", Reason::HistoryExpansion, "hi!"),
    ];
    for (text, reason, word) in rows {
        let directory = tempfile::tempdir().unwrap();
        let built = compiled(text, directory.path(), None);
        assert_eq!(
            built.refusal(),
            Some(&reason),
            "why the line compiler refused `{text}`"
        );
        assert!(
            built.argvs.is_empty(),
            "a refused line reaches the rules with no step: {:?}",
            built.argvs
        );
        assert!(
            denies(&deny_rule(text), &argv_of(&["echo", word])),
            "the rule `Bash({text})` should cover `echo` with the one word `{word}`"
        );
    }
}

/// `*` is the wildcard of a rule, and `? [ { ~` are text in a rule where the line expands them.
///
/// The directory holds `a.md` and `b.md`. `ls ?.md`, `ls [ab].md` and `ls {a,b}.md` compile to
/// `ls a.md b.md`, and the rule written with the same text names the characters, so it covers
/// the literal word and not the names. `ls *.md` compiles to the same names and the rule covers
/// them, since a `*` in a rule covers any text. A quoted `*` is an asterisk to the compiler and
/// still the wildcard of the rule, so the rule covers more than the line spelled, which is the
/// direction PERM-4 allows for a rule that restricts. A quoted `?` is text in both. A rule that
/// read `?` as a one-character wildcard would cover the names and fail the first assertion of the
/// `?` row. A rule that kept `*` as text would fail on the `*` row. A leading `~` is the home
/// directory to the compiler and the characters `~/x` to the rule.
#[test]
fn a_rule_keeps_pattern_characters_as_text_where_the_line_expands_them() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("a.md"), "").unwrap();
    fs::write(directory.path().join("b.md"), "").unwrap();
    let names = argv_of(&["ls", "a.md", "b.md"]);

    for (text, word) in [
        ("ls ?.md", "?.md"),
        ("ls [ab].md", "[ab].md"),
        ("ls {a,b}.md", "{a,b}.md"),
    ] {
        let built = compiled(text, directory.path(), None);
        assert_compiled_to(text, &built, &names);
        let rule = deny_rule(text);
        assert!(
            denies(&rule, &argv_of(&["ls", word])),
            "the rule `Bash({text})` should cover the literal word `{word}`"
        );
        assert!(
            !denies(&rule, &names),
            "the rule `Bash({text})` keeps `{word}` as text and should not cover {names:?}"
        );
    }

    let built = compiled("ls *.md", directory.path(), None);
    assert_compiled_to("ls *.md", &built, &names);
    let star = deny_rule("ls *.md");
    assert!(
        denies(&star, &names),
        "the rule `Bash(ls *.md)` should cover the names the line expanded to"
    );
    assert!(
        denies(&star, &argv_of(&["ls", "*.md"])),
        "the rule `Bash(ls *.md)` should cover the literal word"
    );

    let built = compiled("ls '*.md'", directory.path(), None);
    assert_compiled_to("ls '*.md'", &built, &argv_of(&["ls", "*.md"]));
    let quoted_star = deny_rule("ls '*.md'");
    assert!(
        denies(&quoted_star, &argv_of(&["ls", "*.md"])),
        "the rule `Bash(ls '*.md')` should cover the asterisk word"
    );
    assert!(
        denies(&quoted_star, &argv_of(&["ls", "c.md"])),
        "a `*` keeps its meaning in a rule when it is quoted"
    );

    let built = compiled("ls '?.md'", directory.path(), None);
    assert_compiled_to("ls '?.md'", &built, &argv_of(&["ls", "?.md"]));
    let quoted_mark = deny_rule("ls '?.md'");
    assert!(
        denies(&quoted_mark, &argv_of(&["ls", "?.md"])),
        "the rule `Bash(ls '?.md')` should cover the word with a question mark"
    );
    assert!(
        !denies(&quoted_mark, &argv_of(&["ls", "a.md"])),
        "a `?` is text in a rule"
    );

    let home = tempfile::tempdir().unwrap();
    let tilde = deny_rule("ls ~/x");
    let at_home = format!("{}/x", home.path().display());
    let built = compiled("ls ~/x", directory.path(), Some(home.path()));
    assert_compiled_to("ls ~/x", &built, &argv_of(&["ls", &at_home]));
    assert!(
        denies(&tilde, &argv_of(&["ls", "~/x"])),
        "the rule `Bash(ls ~/x)` should cover the word `~/x`"
    );
    assert!(
        !denies(&tilde, &argv_of(&["ls", &at_home])),
        "the rule `Bash(ls ~/x)` keeps `~` as text and should not cover {at_home}"
    );
    let without_home = compiled("ls ~/x", directory.path(), None);
    assert_eq!(
        without_home.refusal(),
        Some(&Reason::NoHome),
        "a `~` with no home directory is refused by the line compiler"
    );
}

/// A quote that is never closed and a backslash with nothing after it are refused by the compiler
/// and reported for a rule, which stays in force as its characters are written.
///
/// Neither text is a line, so the rule has no typed reading. `Permissions::parse` reports it with
/// the reason and keeps the rule, so the person finds out and the rule still stops the command
/// spelled with those characters. An implementation that dropped the rule would leave the
/// protection off, one that guessed the missing quote would cover `echo x` as well, and one that
/// kept the rule without reporting it would leave the writer believing it was read as a line.
#[test]
fn an_unclosed_quote_and_a_trailing_backslash_are_refused_by_the_line_and_reported_for_the_rule() {
    let rows: Vec<(&str, Reason, Unreadable, &str)> = vec![
        (
            r#"echo "x"#,
            Reason::Unclosed('"'),
            Unreadable::UnclosedQuote,
            r#""x"#,
        ),
        (
            "echo 'x",
            Reason::Unclosed('\''),
            Unreadable::UnclosedQuote,
            "'x",
        ),
        (
            r"echo x\",
            Reason::Syntax("a `\\` with nothing after it"),
            Unreadable::TrailingBackslash,
            r"x\",
        ),
    ];
    for (text, reason, unreadable, word) in rows {
        let directory = tempfile::tempdir().unwrap();
        let built = compiled(text, directory.path(), None);
        assert_eq!(
            built.refusal(),
            Some(&reason),
            "why the line compiler refused `{text}`"
        );
        assert!(
            built.argvs.is_empty(),
            "a refused line reaches the rules with no step: {:?}",
            built.argvs
        );

        let written = format!("Bash({text})");
        let (permissions, rejected) =
            Permissions::parse(std::slice::from_ref(&written), &[], &[], &Anchors::none());
        assert_eq!(
            rejected,
            [Rejected {
                text: written.clone(),
                reason: unreadable,
                file: None,
            }],
            "what `Permissions::parse` reports for `{written}`"
        );
        assert!(
            unreadable.keeps_the_rule(),
            "{unreadable:?} is a warning, and the rule stays in force"
        );
        assert!(
            denies(&permissions, &argv_of(&["echo", word])),
            "the rule `{written}` stays in force as its characters are written"
        );
        assert!(
            !denies(&permissions, &argv_of(&["echo", "x"])),
            "the rule `{written}` has no reading that closes the quote or drops the backslash"
        );
    }
}
