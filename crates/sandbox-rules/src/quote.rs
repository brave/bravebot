//! How a value is written into the syntax a backend hands a platform, so a path or an argument
//! that holds that syntax's own characters is read as one value (SANDBOX-4).

#[cfg(verus_keep_ghost)]
use crate::spec::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// The characters of `s`, in order.
#[cfg_attr(verus_keep_ghost, verus_verify(external_body))]
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r@ == s@,
))]
fn chars_of(s: &str) -> Vec<char> {
    s.chars().collect()
}

/// The string of `chars`, in order.
#[cfg_attr(verus_keep_ghost, verus_verify(external_body))]
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r@ == chars@,
))]
fn string_of(chars: Vec<char>) -> String {
    chars.into_iter().collect()
}

/// Push `n` backslashes onto `out`.
#[cfg_attr(verus_keep_ghost, verus_verify(external_body))]
#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures
        final(out)@ == old(out)@ + backslashes(n as nat),
))]
fn push_backslashes(out: &mut Vec<char>, n: usize) {
    out.extend(std::iter::repeat_n('\\', n));
}

/// `value` as a Seatbelt string literal, for the profile `sandbox-exec` is given on macOS.
///
/// Escapes backslashes and quotes so a path containing either cannot terminate the
/// literal early and inject profile syntax.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        read_literal(r@) == Some(value@),
))]
pub fn seatbelt_literal(value: &str) -> String {
    let chars = chars_of(value);
    let mut literal: Vec<char> = Vec::new();
    literal.push('"');
    let mut at = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            at <= chars.len(),
            chars@ == value@,
            literal@ == seq!['"'] + escape(chars@.take(at as int)),
        decreases chars.len() - at,
    ))]
    while at < chars.len() {
        let character = chars[at];
        #[cfg(verus_keep_ghost)]
        proof! {
            lemma_escape_push(chars@.take(at as int), character);
            assert(chars@.take(at as int + 1) =~= chars@.take(at as int).push(character));
        }
        // A line of its own, so the mutation in verify/mutations.txt can rewrite it.
        let escapes = character == '\\' || character == '"';
        if escapes {
            literal.push('\\');
        }
        literal.push(character);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(literal@ =~= seq!['"'] + escape(chars@.take(at as int + 1)));
        }
        at += 1;
    }
    literal.push('"');
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(chars@.take(chars.len() as int) =~= value@);
        a_seatbelt_literal_reads_back_as_its_path(value@);
        assert(literal@.drop_first() =~= escape(value@) + seq!['"']);
    }
    string_of(literal)
}

/// The command line Windows' `CreateProcessW` is given, which is the one thing on this platform
/// that is a string rather than a vector.
///
/// Windows starts a process from a single command line and leaves the program to split
/// it, so an argument containing a space or a quotation mark is two arguments unless it
/// is quoted here. A policy's paths reach this as arguments, so an unquoted one is a path
/// deciding what the program was asked to do ([SANDBOX-4]).
///
/// The rules are the ones `CommandLineToArgvW` parses back, which is what the C runtime
/// and every language runtime on this platform use: an argument [`is_written_quoted`] is
/// quoted, a backslash run immediately before the closing
/// quotation mark is doubled, and a backslash run before an embedded quotation mark is doubled
/// and the quotation mark escaped. A backslash anywhere else is literal, so a path is not
/// rewritten by being quoted. Any other argument is written as it is, since `cmd.exe` reads its
/// own switches from the command line and does not find `/c` once it is quoted. What `cmd.exe`
/// does with the rest of its line is not decided by quoting, so `refusal_for_program` in
/// `bravebot-sandbox` refuses the lines it would read differently.
///
/// [SANDBOX-4]: ../../../docs/specs/sandboxing.md
#[cfg_attr(verus_keep_ghost, verifier::external)]
pub fn command_line(program: &str, args: &[String]) -> String {
    let mut line = quoted(program);
    for arg in args {
        line.push(' ');
        if is_written_quoted(arg) {
            line.push_str(&quoted(arg));
        } else {
            line.push_str(arg);
        }
    }
    line
}

/// Whether [`command_line`] writes `arg` between quotation marks: it is empty, or holds a space,
/// a tab, a quotation mark, a character `cmd.exe` reads as its own (`& | < > ^ % ( ) !`), or a
/// `*` or `?`, which a program built with the MinGW or MSVC wildcard runtime expands into file
/// names where the argument is left unquoted.
#[cfg_attr(verus_keep_ghost, verifier::external)]
pub fn is_written_quoted(arg: &str) -> bool {
    arg.is_empty()
        || arg.contains([
            ' ', '\t', '"', '&', '|', '<', '>', '^', '%', '(', ')', '!', '*', '?',
        ])
}

/// One argument, written so `CommandLineToArgvW` gives it back unchanged.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r@.len() > 0,
        r@[0] == '"',
        read_quoted(r@.drop_first(), 0) == Some((arg@, Seq::<char>::empty())),
))]
pub fn quoted(arg: &str) -> String {
    let chars = chars_of(arg);
    let mut quoted: Vec<char> = Vec::new();
    quoted.push('"');
    let mut backslashes = 0usize;
    let mut at = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            at <= chars.len(),
            chars@ == arg@,
            backslashes as nat == run_after(chars@.take(at as int), 0),
            backslashes <= at,
            quoted@ == seq!['"'] + written(chars@.take(at as int), 0),
        decreases chars.len() - at,
    ))]
    while at < chars.len() {
        let character = chars[at];
        #[cfg(verus_keep_ghost)]
        proof! {
            lemma_written_push(chars@.take(at as int), character, 0);
            lemma_written_of_one(character, backslashes as nat);
            assert(chars@.take(at as int + 1) =~= chars@.take(at as int).push(character));
        }
        if character == '\\' {
            backslashes += 1;
            quoted.push('\\');
        } else if character == '"' {
            // The run before a quotation mark is doubled so it is read as backslashes rather than
            // as escapes, and the quotation mark is escaped so it is read as content rather than
            // as the end of the argument.
            push_backslashes(&mut quoted, backslashes + 1);
            backslashes = 0;
            quoted.push('"');
        } else {
            backslashes = 0;
            quoted.push(character);
        }
        at += 1;
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(quoted@ =~= seq!['"'] + written(chars@.take(at as int), 0));
        }
    }
    // The run before the closing quotation mark is doubled for the same reason.
    push_backslashes(&mut quoted, backslashes);
    quoted.push('"');
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(chars@.take(chars.len() as int) =~= arg@);
        lemma_emit_is_written_and_the_run(arg@, 0);
        a_quoted_argument_reads_back_as_itself(arg@, Seq::<char>::empty());
        assert(quoted@.drop_first() =~= emit(arg@, 0) + seq!['"'] + Seq::<char>::empty());
    }
    string_of(quoted)
}
