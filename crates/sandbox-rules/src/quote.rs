//! How a value is written into the syntax a backend hands a platform, so a path or an argument
//! that holds that syntax's own characters is read as one value (SANDBOX-4).

/// `value` as a Seatbelt string literal, for the profile `sandbox-exec` is given on macOS.
///
/// Escapes backslashes and quotes so a path containing either cannot terminate the
/// literal early and inject profile syntax.
pub fn seatbelt_literal(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
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
pub fn is_written_quoted(arg: &str) -> bool {
    arg.is_empty()
        || arg.contains([
            ' ', '\t', '"', '&', '|', '<', '>', '^', '%', '(', ')', '!', '*', '?',
        ])
}

/// One argument, written so `CommandLineToArgvW` gives it back unchanged.
pub fn quoted(arg: &str) -> String {
    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('"');
    let mut backslashes = 0usize;
    for character in arg.chars() {
        match character {
            '\\' => {
                backslashes += 1;
                quoted.push('\\');
            }
            '"' => {
                // The run before a quotation mark is doubled so it is read as backslashes
                // rather than as escapes, and the quotation mark is escaped so it is read
                // as content rather than as the end of the argument.
                quoted.extend(std::iter::repeat_n('\\', backslashes + 1));
                backslashes = 0;
                quoted.push('"');
            }
            other => {
                backslashes = 0;
                quoted.push(other);
            }
        }
    }
    // The run before the closing quotation mark is doubled for the same reason.
    quoted.extend(std::iter::repeat_n('\\', backslashes));
    quoted.push('"');
    quoted
}
