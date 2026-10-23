//! Whether every `#[allow]` and `#[expect]` in this workspace gives a reason.
//!
//! `clippy::allow_attributes_without_reason` refuses one, but only in code the host compiles. A
//! `#[cfg(windows)]` function is never read on macOS, and a Linux backend is never read on
//! Windows, so an `#[allow(unsafe_code)]` with no reason in either reaches the Windows or Linux lint
//! job after the push. This reads every source file as text, so the platform the check runs on does
//! not decide what it sees. A reason is the last item of the list, `reason = "..."`, and has a
//! word in it.
//!
//! The property belongs to the workspace rather than to any one crate, and it lives here because
//! this is the crate the shipped binary is built from.

use std::path::Path;
use std::path::PathBuf;

/// A piece of an attribute, enough to tell a lint attribute from any other and to find the last
/// item of its list.
#[derive(Debug, PartialEq)]
enum Token<'a> {
    Ident(&'a str),
    Punct(char),
    /// A string, a character or a byte string, with its quotes.
    Literal(&'a str),
}

/// What starts at a position that is not code, and the position just past it.
enum Skip {
    Comment(usize),
    Literal(usize),
}

/// The position just past the string literal whose closing quote is the first unescaped one at or
/// after `from`.
fn past_quote(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

/// The comment, string or character literal that starts at `at`, if one does. A lifetime such as
/// `'a` is code, and is told from a character literal by the quote that closes the literal.
fn skip(source: &str, at: usize) -> Option<Skip> {
    let bytes = source.as_bytes();
    match bytes[at] {
        b'/' if bytes.get(at + 1) == Some(&b'/') => {
            let end = source[at..].find('\n').map_or(bytes.len(), |n| at + n);
            Some(Skip::Comment(end))
        }
        b'/' if bytes.get(at + 1) == Some(&b'*') => {
            let (mut depth, mut end) = (1, at + 2);
            while end < bytes.len() && depth > 0 {
                if bytes[end..].starts_with(b"/*") {
                    depth += 1;
                    end += 2;
                } else if bytes[end..].starts_with(b"*/") {
                    depth -= 1;
                    end += 2;
                } else {
                    end += 1;
                }
            }
            Some(Skip::Comment(end))
        }
        b'"' => Some(Skip::Literal(past_quote(bytes, at + 1))),
        b'r' => {
            let hashes = bytes[at + 1..].iter().take_while(|&&b| b == b'#').count();
            if bytes.get(at + 1 + hashes) != Some(&b'"') {
                return None;
            }
            let closing = format!("\"{}", "#".repeat(hashes));
            let end = source[at + 2 + hashes..]
                .find(&closing)
                .map_or(bytes.len(), |n| at + 2 + hashes + n + closing.len());
            Some(Skip::Literal(end))
        }
        b'\'' => {
            let rest = &source[at + 1..];
            let mut chars = rest.chars();
            let first = chars.next()?;
            if first == '\\' {
                let end = rest
                    .get(2..)
                    .and_then(|tail| tail.find('\''))
                    .map_or(bytes.len(), |n| at + 3 + n + 1);
                return Some(Skip::Literal(end));
            }
            let after = at + 1 + first.len_utf8();
            (bytes.get(after) == Some(&b'\'')).then_some(Skip::Literal(after + 1))
        }
        _ => None,
    }
}

/// The position of the `]` that closes the attribute whose `#` is at `hash`, and of its `[`.
fn brackets(source: &str, hash: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut at = hash + 1;
    let blank = |at: usize| bytes.get(at).is_some_and(u8::is_ascii_whitespace);
    while blank(at) {
        at += 1;
    }
    if bytes.get(at) == Some(&b'!') {
        at += 1;
    }
    while blank(at) {
        at += 1;
    }
    if bytes.get(at) != Some(&b'[') {
        return None;
    }
    let open = at;
    let mut depth = 0;
    while at < bytes.len() {
        if let Some(Skip::Comment(end) | Skip::Literal(end)) = skip(source, at) {
            at = end;
            continue;
        }
        match bytes[at] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some((open, at));
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// Each attribute in `source`, as the byte position of its `#` and the text between its brackets.
/// Text that only looks like one, in a string or a comment, is not read.
fn attributes(source: &str) -> Vec<(usize, &str)> {
    let bytes = source.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if let Some(Skip::Comment(end) | Skip::Literal(end)) = skip(source, at) {
            at = end;
        } else if let Some((open, close)) =
            (bytes[at] == b'#').then(|| brackets(source, at)).flatten()
        {
            found.push((at, &source[open + 1..close]));
            at = close + 1;
        } else {
            at += 1;
        }
    }
    found
}

fn tokens(text: &str) -> Vec<Token<'_>> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match skip(text, at) {
            Some(Skip::Comment(end)) => at = end,
            Some(Skip::Literal(end)) => {
                found.push(Token::Literal(&text[at..end]));
                at = end;
            }
            None if bytes[at].is_ascii_whitespace() => at += 1,
            None if bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_' => {
                let end = (at..bytes.len())
                    .find(|&i| !(bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_'))
                    .unwrap_or(bytes.len());
                found.push(Token::Ident(&text[at..end]));
                at = end;
            }
            None => {
                let c = text[at..].chars().next().expect("a character");
                found.push(Token::Punct(c));
                at += c.len_utf8();
            }
        }
    }
    found
}

/// `tokens` cut at each comma that is not inside a bracket, with the empty piece a trailing comma
/// leaves dropped.
fn items<'a, 'b>(tokens: &'b [Token<'a>]) -> Vec<&'b [Token<'a>]> {
    let mut pieces = Vec::new();
    let (mut depth, mut start) = (0_i32, 0);
    for (at, token) in tokens.iter().enumerate() {
        match token {
            Token::Punct('(' | '[' | '{') => depth += 1,
            Token::Punct(')' | ']' | '}') => depth -= 1,
            Token::Punct(',') if depth == 0 => {
                pieces.push(&tokens[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    pieces.push(&tokens[start..]);
    pieces.retain(|piece| !piece.is_empty());
    pieces
}

/// Whether a string literal holds a word, so that `reason = ""`, a blank one and one that is only a
/// line continuation are not reasons.
fn says_something(literal: &str) -> bool {
    literal
        .split_once('"')
        .and_then(|(_, rest)| rest.rsplit_once('"'))
        .is_some_and(|(inside, _)| inside.chars().any(char::is_alphanumeric))
}

/// Whether an attribute, from its name, is an `allow` or `expect` whose list does not end in
/// `reason = "..."`, or a `cfg_attr` that applies one. Other attributes, `deny` and `warn` among
/// them, are not asked for a reason.
fn lacks_a_reason(attribute: &[Token<'_>]) -> bool {
    let [
        Token::Ident(name),
        Token::Punct('('),
        inner @ ..,
        Token::Punct(')'),
    ] = attribute
    else {
        return false;
    };
    let list = items(inner);
    match *name {
        "allow" | "expect" => !list.last().is_some_and(|last| {
            matches!(
                last,
                [Token::Ident("reason"), Token::Punct('='), Token::Literal(text)]
                    if says_something(text)
            )
        }),
        // The first item is the condition, and the rest are the attributes it applies.
        "cfg_attr" => list.iter().skip(1).any(|item| lacks_a_reason(item)),
        _ => false,
    }
}

/// The 1-based lines of `source` where an `allow` or `expect` gives no reason.
fn without_a_reason(source: &str) -> Vec<usize> {
    attributes(source)
        .into_iter()
        .filter(|(_, text)| lacks_a_reason(&tokens(text)))
        .map(|(at, _)| source[..at].matches('\n').count() + 1)
        .collect()
}

fn rust_files(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a directory under crates/") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                rust_files(&path, found);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// An `#[allow]` or `#[expect]` is a decision to leave a lint unsatisfied, and the next person to
/// read it has nothing but the attribute to learn why. It is checked in every file whatever
/// platform the file is for, because the lint cannot read a file the host does not compile.
#[test]
fn every_allow_and_expect_in_the_crates_gives_a_reason() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("this crate is two directories below the workspace root");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    assert!(
        files.len() > 100,
        "the crates were not found: {}",
        files.len()
    );
    let mut bare = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file).expect("a Rust source file");
        for line in without_a_reason(&source) {
            bare.push(format!(
                "{}:{line}",
                file.strip_prefix(root)
                    .expect("a path under the root")
                    .display()
            ));
        }
    }
    assert!(
        bare.is_empty(),
        "these `allow` and `expect` attributes give no reason. End the list with \
         `reason = \"...\"`:\n{}",
        bare.join("\n")
    );
}

/// The forms an attribute takes that a check reading only `#[allow(` would miss: a crate-level
/// `#!`, a condition around it, one wrapped inside another, a list over several lines and one
/// that names several lints.
#[test]
fn an_allow_or_expect_with_no_reason_is_found_in_every_form_it_is_written() {
    let bare = [
        "#[allow(dead_code)]\nfn f() {}",
        "#![allow(dead_code)]",
        "#[expect(unsafe_code)]\nfn f() {}",
        "#[cfg_attr(not(unix), allow(dead_code))]\nfn f() {}",
        "#![cfg_attr(windows, expect(dead_code))]",
        "#[cfg_attr(a, cfg_attr(b, allow(dead_code)))]\nfn f() {}",
        // A bracket inside a string is not the end of the attribute.
        "#[cfg_attr(feature = \"a]b\", allow(dead_code))]\nfn f() {}",
        "#[allow(\n    unsafe_code,\n    clippy::too_many_arguments\n)]\nfn f() {}",
        "#[allow(clippy::a, clippy::b)]\nfn f() {}",
        "#[ allow ( dead_code ) ]\nfn f() {}",
        "#[allow()]\nfn f() {}",
        // The reason has to end the list: lints after it leave it for none of them to read.
        "#[allow(dead_code, reason = \"x\", unused_mut)]\nfn f() {}",
        // A `reason` that is not a string is not one, and neither is a string with nothing in it.
        "#[allow(dead_code, reason)]\nfn f() {}",
        "#[allow(dead_code, reason = \"\")]\nfn f() {}",
        "#[allow(dead_code, reason = \"   \")]\nfn f() {}",
        "#[allow(dead_code, reason = \"\\\n   \")]\nfn f() {}",
        "#[allow(dead_code, reason = r#\"  \"#)]\nfn f() {}",
    ];
    for source in bare {
        assert_eq!(
            without_a_reason(source).len(),
            1,
            "a missing reason was not found in:\n{source}"
        );
    }
}

/// The forms that give a reason, which must not be reported: the same shapes as above, and a
/// reason that contains the brackets and parentheses a reader of attributes would stop at.
#[test]
fn an_allow_or_expect_that_gives_a_reason_is_not_reported() {
    let reasoned = [
        "#[allow(dead_code, reason = \"x\")]\nfn f() {}",
        "#![allow(dead_code, reason = \"x\")]",
        "#[expect(unsafe_code, reason = \"x\")]\nfn f() {}",
        "#[cfg_attr(not(unix), allow(dead_code, reason = \"x\"))]\nfn f() {}",
        "#[cfg_attr(a, cfg_attr(b, allow(dead_code, reason = \"x\")))]\nfn f() {}",
        "#[allow(\n    unsafe_code,\n    clippy::too_many_arguments,\n    reason = \"x\"\n)]\nfn f() {}",
        "#[allow(dead_code, reason = \"x\",)]\nfn f() {}",
        "#[allow(dead_code, reason = \"see #[allow(x)] and ) ] here\")]\nfn f() {}",
        "#[allow(dead_code, reason = r#\"a \"quoted\" ] word\"#)]\nfn f() {}",
    ];
    for source in reasoned {
        assert_eq!(
            without_a_reason(source),
            Vec::<usize>::new(),
            "a reason was not read in:\n{source}"
        );
    }
}

/// Attributes that are not asked for a reason: the lint is about silencing one, so `deny`, `warn`
/// and `forbid` are not, and neither is anything that is not a lint attribute.
#[test]
fn an_attribute_that_does_not_silence_a_lint_is_not_reported() {
    let other = [
        "#[derive(Debug)]\nstruct S;",
        "#[cfg(test)]\nmod tests {}",
        "#[cfg_attr(test, derive(Debug))]\nstruct S;",
        "#[deny(dead_code)]\nfn f() {}",
        "#![warn(missing_docs)]",
        "#![forbid(unsafe_code)]",
        "#[doc = \"allow(dead_code)\"]\nfn f() {}",
        "let x = vec![1];\nlet y = x[0];",
    ];
    for source in other {
        assert_eq!(
            without_a_reason(source),
            Vec::<usize>::new(),
            "reported:\n{source}"
        );
    }
}

/// Text that looks like an attribute but is a string or a comment is not one. A character literal
/// holding a quote does not start a string, and a lifetime does not start a character literal, so
/// the attributes after them are still read.
#[test]
fn text_that_only_looks_like_an_attribute_is_not_read_and_what_follows_it_still_is() {
    let looks_like = "let a = \"#[allow(dead_code)]\";\n\
        // #[allow(unused_mut)]\n\
        /* #[allow(unused_variables)] /* nested */ #[allow(unused)] */\n\
        let b = r#\"#[allow(dead_code)] \"quoted\"\"#;\n\
        let c = b\"#[allow(dead_code)]\";\n";
    assert_eq!(without_a_reason(looks_like), Vec::<usize>::new());

    let after_a_quote = "let q = '\"';\n#[allow(dead_code)]\nfn f() {}\nlet s = \"x\";";
    assert_eq!(without_a_reason(after_a_quote), [2]);

    let after_an_escape =
        "let q = '\\'';\nlet r = '\\u{22}';\nlet s = '\\\"';\n#[allow(dead_code)]\nfn f() {}\n";
    assert_eq!(without_a_reason(after_an_escape), [4]);

    let after_a_lifetime =
        "fn f<'a>(x: &'a str) -> &'a str { x }\n#[allow(dead_code)]\nfn g() {}\nlet s = \"y\";";
    assert_eq!(without_a_reason(after_a_lifetime), [2]);
}

/// A line number is where the attribute starts, which is where a person fixing it looks.
#[test]
fn a_missing_reason_is_reported_at_the_line_the_attribute_starts_on() {
    let source =
        "fn a() {}\n\n#[allow(\n    dead_code\n)]\nfn b() {}\n#[expect(unused)]\nfn c() {}\n";
    assert_eq!(without_a_reason(source), [3, 7]);
}
