//! The library must stay transport-agnostic, and must not write the state directory itself.
//!
//! Greps, which are worth more than a convention nobody re-checks. If the library starts
//! printing, a second front-end inherits lines it cannot intercept and a stray `println!`
//! interleaves with the protocol — the same reason the agent's kernel never prints.

use std::path::Path;

#[test]
fn nothing_below_bin_writes_to_stdout_or_ends_the_process() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offences = Vec::new();

    visit(&source, &mut |path, text| {
        // The transport is the one place allowed to do either.
        if path.components().any(|part| part.as_os_str() == "bin") {
            return;
        }
        for (number, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for forbidden in ["println!", "print!", "std::process::exit", "eprintln!"] {
                if code.contains(forbidden) {
                    offences.push(format!("{}:{}: {forbidden}", path.display(), number + 1));
                }
            }
        }
    });

    assert!(
        offences.is_empty(),
        "the library must not print:\n{}",
        offences.join("\n")
    );
}

/// Nothing here writes into `~/.bravebot` itself: every such write goes through the crate
/// that owns the store, which asks for the modes that directory is kept at.
///
/// The bytes there are one surface's prompts, conversations and standing permissions being read
/// by another, so the modes are the whole of what keeps them to one account. A second writer
/// creating a directory or opening a file for itself would land at the process umask, leaving
/// exactly the files a front end writes most readable by every local account. The mode of a
/// directory that already exists is not changed by creating it again either, so the first such
/// write would decide it for every session after.
#[test]
fn the_state_directory_is_written_only_through_the_crate_that_owns_it() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offences = Vec::new();

    visit(&source, &mut |path, text| {
        for (number, line) in outside_tests(text) {
            let code = line.split("//").next().unwrap_or("");
            for forbidden in [
                "fs::write",
                "fs::create_dir",
                "fs::remove_file",
                "fs::remove_dir",
                "File::create",
                "OpenOptions",
            ] {
                if code.contains(forbidden) {
                    offences.push(format!("{}:{number}: {forbidden}", path.display()));
                }
            }
        }
    });

    assert!(
        offences.is_empty(),
        "a write here would land at the umask rather than the store's modes:\n{}",
        offences.join("\n")
    );
}

/// The crate's own lines, numbered, with every `#[cfg(test)]` module left out.
///
/// A fixture writing a scratch file is not a second writer of the store, and a rule that
/// counted one would be a rule kept by writing fixtures somewhere less readable.
fn outside_tests(text: &str) -> Vec<(usize, &str)> {
    outside_modules_marked(text, |line| line.trim() == "#[cfg(test)]")
}

/// As [`outside_tests`], with `marks` saying which attribute line opens an item to leave out.
///
/// The item is whatever the attribute is on: a module or function up to its closing brace, or a
/// `use` or `mod x;` up to its semicolon. Other attributes between the two belong to the item.
fn outside_modules_marked(text: &str, marks: impl Fn(&str) -> bool) -> Vec<(usize, &str)> {
    let mut kept = Vec::new();
    let mut skipping = false;
    let mut opened = false;
    let mut braces = 0i32;
    let mut literal = Literal::default();
    for (index, line) in text.lines().enumerate() {
        if !skipping {
            if marks(line) {
                skipping = true;
                opened = false;
                braces = 0;
            } else {
                kept.push((index + 1, line));
            }
            continue;
        }
        let code = literal
            .strip(line.split("//").next().unwrap_or(""))
            .trim()
            .to_string();
        if code.starts_with("#[") {
            continue;
        }
        braces += code.matches('{').count() as i32 - code.matches('}').count() as i32;
        opened |= code.contains('{');
        let ended = if opened {
            braces <= 0
        } else {
            code.ends_with(';')
        };
        if ended {
            skipping = false;
        }
    }
    kept
}

/// Whether an attribute line makes the item after it exist only in a test build: `#[cfg(test)]`, or
/// `all(...)` with `test` among its arguments. `any`, `not` and every other condition can be true
/// in a build that is not a test build, so what they mark is production code.
fn only_built_for_tests(line: &str) -> bool {
    let line = line.trim();
    let Some(condition) = line
        .strip_prefix("#[cfg(")
        .and_then(|rest| rest.strip_suffix(")]"))
    else {
        return false;
    };
    requires_test(condition.trim())
}

fn requires_test(condition: &str) -> bool {
    if condition == "test" {
        return true;
    }
    let Some(arguments) = condition
        .strip_prefix("all(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    let mut depth = 0;
    let mut start = 0;
    let mut parts = Vec::new();
    for (at, c) in arguments.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(arguments[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(arguments[start..].trim());
    parts.into_iter().any(requires_test)
}

/// Where a scan of source text stands inside a string literal, which can run over several lines.
#[derive(Default)]
struct Literal {
    /// Inside a plain string, after a backslash.
    escaped: bool,
    /// Inside a plain string.
    plain: bool,
    /// Inside a raw string closed by a quote and this many `#`.
    raw: Option<usize>,
}

impl Literal {
    /// `line` with the inside of its string and character literals removed, so a brace in one is
    /// not counted. The state carries over to the next line.
    fn strip(&mut self, line: &str) -> String {
        let chars: Vec<char> = line.chars().collect();
        let mut kept = String::new();
        let mut at = 0;
        while at < chars.len() {
            let c = chars[at];
            if let Some(hashes) = self.raw {
                if c == '"' && chars[at + 1..].iter().take(hashes).all(|h| *h == '#') {
                    let closing = chars[at + 1..].iter().take_while(|h| **h == '#').count();
                    if closing >= hashes {
                        self.raw = None;
                        at += hashes;
                    }
                }
            } else if self.plain {
                if self.escaped {
                    self.escaped = false;
                } else if c == '\\' {
                    self.escaped = true;
                } else if c == '"' {
                    self.plain = false;
                }
            } else if c == '"' {
                self.plain = true;
            } else if c == 'r'
                && !chars[..at]
                    .last()
                    .is_some_and(|p| p.is_alphanumeric() || *p == '_')
            {
                let hashes = chars[at + 1..].iter().take_while(|h| **h == '#').count();
                if chars.get(at + 1 + hashes) == Some(&'"') {
                    self.raw = Some(hashes);
                    at += 1 + hashes;
                } else {
                    kept.push(c);
                }
            } else if c == '\'' {
                // A character literal is `'x'` or `'\x'`; anything else is a lifetime.
                let literal_end = if chars.get(at + 1) == Some(&'\\') {
                    chars[at + 2..]
                        .iter()
                        .position(|q| *q == '\'')
                        .map(|n| at + 2 + n)
                } else if chars.get(at + 2) == Some(&'\'') {
                    Some(at + 2)
                } else {
                    None
                };
                match literal_end {
                    Some(end) => at = end,
                    None => kept.push(c),
                }
            } else {
                kept.push(c);
            }
            at += 1;
        }
        kept
    }
}

fn visit(directory: &Path, each: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            visit(&path, each);
        } else if path.extension().is_some_and(|e| e == "rs")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            each(&path, &text);
        }
    }
}

/// Every place the bridge process starts a program runs under the spawn gate.
///
/// macOS marks a new socket pair close-on-exec in a second step, so a program started on another
/// thread between the two holds both ends of a control channel (RPCVIEW-8). The gate is the one
/// lock that creation of a channel and every start of a program share. A start that skips it is
/// the gap, and nothing at the call site says so, which is why this reads the source: any crate
/// the bridge links, outside its test modules, that starts a `Command` must do it inside
/// `starting_a_program`.
#[test]
fn every_program_the_bridge_process_starts_goes_through_the_spawn_gate() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut offences = Vec::new();

    for krate in linked_crates(&workspace) {
        visit(&workspace.join(&krate).join("src"), &mut |path, text| {
            // A file that is a test module of its own is declared elsewhere with `mod x_tests;`.
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("_tests.rs"))
            {
                return;
            }
            // Windows has no fork, so a handle is not inherited by a program started elsewhere.
            if path.components().any(|part| part.as_os_str() == "windows") {
                return;
            }
            for number in ungated_starts(text) {
                offences.push(format!("{}:{number}", path.display()));
            }
        });
    }

    assert!(
        offences.is_empty(),
        "a program is started outside the spawn gate:\n{}",
        offences.join("\n")
    );
}

/// The line numbers in `text`, outside its test modules, that start a program other than inside
/// a call to `starting_a_program`.
fn ungated_starts(text: &str) -> Vec<usize> {
    // `#[cfg(test)]`, and the forms that add a platform: `#[cfg(all(test, ...))]`. A condition that
    // is the absence of test marks production code.
    let lines = outside_modules_marked(text, only_built_for_tests);
    let mut found = Vec::new();
    for (at, (number, line)) in lines.iter().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        // A descriptor made in two steps on macOS (`pipe` then close-on-exec) is the same hazard as a
        // socket pair: a program started on another thread in between inherits it.
        let Some(start) = [
            ".spawn()",
            ".output()",
            ".status()",
            "io::pipe()",
            "UnixStream::pair()",
            "socketpair(",
        ]
        .iter()
        .filter_map(|call| code.find(call))
        .min() else {
            continue;
        };
        // The statement that starts it: a few lines up to where it is built.
        let from = at.saturating_sub(12);
        let mut statement: String = lines[from..at]
            .iter()
            .map(|(_, l)| format!("{}\n", l.split("//").next().unwrap_or("")))
            .collect();
        statement.push_str(&code[..start]);
        // A `.spawn()` with no arguments is a `Command` in every crate this reads. `output` and
        // `status` are also methods of other things, so those count only beside a command.
        let is_spawn = code[start..].starts_with(".spawn()");
        let is_descriptor = !code[start..].starts_with(".output()")
            && !code[start..].starts_with(".status()")
            && !is_spawn;
        if !is_spawn
            && !is_descriptor
            && !(statement.contains("Command") || statement.contains("command"))
        {
            continue;
        }
        // Inside the gate means a call to either function that is still open where the start is.
        let inside = ["starting_a_program(", "creating_a_channel("]
            .iter()
            .filter_map(|name| statement.rfind(name).map(|at| at + name.len()))
            .max()
            .is_some_and(|after_name| {
                let after = &statement[after_name..];
                after.matches('(').count() >= after.matches(')').count()
            });
        if !inside {
            found.push(*number);
        }
    }
    found
}

#[test]
fn a_start_inside_the_gate_is_not_an_offence() {
    assert_eq!(
        ungated_starts("fn a() { starting_a_program(|| command.spawn()) }\n"),
        Vec::<usize>::new()
    );
    let several = "fn a() {\n    let child = bravebot_sandbox::process::starting_a_program(|| {\n        command\n            .stdin(x)\n            .spawn()\n    });\n}\n";
    assert_eq!(ungated_starts(several), Vec::<usize>::new());
}

#[test]
fn a_start_beside_a_gated_one_is_an_offence() {
    let text = "fn a() {\n    let one = starting_a_program(|| first_command.spawn());\n    let two = second_command.spawn();\n}\n";
    assert_eq!(ungated_starts(text), vec![3]);
}

#[test]
fn code_that_is_not_a_test_module_is_scanned_even_when_its_attribute_mentions_test() {
    let text = "#[cfg(not(test))]\nfn a() {\n    command.spawn();\n}\n";
    assert_eq!(ungated_starts(text), vec![3]);
}

#[test]
fn a_one_line_test_item_does_not_hide_the_item_after_it() {
    let text = "#[cfg(test)]\nuse std::fs;\n\nfn a() {\n    command.spawn();\n}\n";
    assert_eq!(ungated_starts(text), vec![5]);
}

#[test]
fn a_start_is_found_whatever_the_value_is_called() {
    // Nothing near it says what `launcher` is.
    let text = "fn a() {\n    let launcher = build();\n    launcher.spawn();\n}\n";
    assert_eq!(ungated_starts(text), vec![3]);
}

#[test]
fn code_that_is_built_outside_tests_is_scanned_whatever_else_its_cfg_says() {
    for attribute in [
        "#[cfg(any(test, unix))]",
        "#[cfg(feature = \"latest\")]",
        "#[cfg(all(not(windows), test_support))]",
        "#[cfg(not(test))]",
    ] {
        let text = format!("{attribute}\nfn a() {{\n    command.spawn();\n}}\n");
        assert_eq!(ungated_starts(&text), vec![3], "{attribute}");
    }
}

#[test]
fn every_form_of_test_only_code_is_left_out() {
    for attribute in [
        "#[cfg(test)]",
        "#[cfg(all(test, unix))]",
        "#[cfg(all(unix, test))]",
        "#[cfg(all(test, target_os = \"linux\"))]",
    ] {
        let text = format!("{attribute}\nfn a() {{\n    command.spawn();\n}}\n");
        assert_eq!(ungated_starts(&text), Vec::<usize>::new(), "{attribute}");
    }
}

#[test]
fn a_brace_in_a_string_does_not_change_where_a_test_module_ends() {
    let text = "#[cfg(test)]\nmod tests {\n    const OPEN: &str = \"{\";\n}\n\nfn a() {\n    command.spawn();\n}\n";
    assert_eq!(ungated_starts(text), vec![7]);
}

#[test]
fn a_descriptor_made_outside_the_gate_is_an_offence() {
    for creation in [
        "std::io::pipe()",
        "UnixStream::pair()",
        "libc::socketpair(libc::AF_UNIX, 1, 0, fds)",
    ] {
        let text = format!("fn a() {{\n    let made = {creation};\n}}\n");
        assert_eq!(ungated_starts(&text), vec![2], "{creation}");
    }
}

#[test]
fn a_descriptor_made_inside_the_gate_is_not_an_offence() {
    let text = "fn a() {\n    let made = creating_a_channel(|| std::io::pipe());\n}\n";
    assert_eq!(ungated_starts(text), Vec::<usize>::new());
}

#[test]
fn a_test_module_is_left_out() {
    let text = "#[cfg(test)]\nmod tests {\n    fn a() {\n        command.spawn();\n    }\n}\n";
    assert_eq!(ungated_starts(text), Vec::<usize>::new());
}

/// The crates this crate links, directly or not, from the `path` entries of their manifests.
fn linked_crates(workspace: &Path) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut queue = vec!["ui-bridge".to_string()];
    while let Some(name) = queue.pop() {
        if found.contains(&name) {
            continue;
        }
        let manifest = workspace.join(&name).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).unwrap_or_default();
        queue.extend(path_dependencies(&text));
        found.push(name);
    }
    found
}

/// The crates a manifest names by `path` in a table other than `dev-dependencies`, whatever the
/// table is called and wherever in the file it comes.
fn path_dependencies(manifest: &str) -> Vec<String> {
    let mut linked = false;
    let mut found = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            linked = line.contains("dependencies") && !line.contains("dev-dependencies");
            continue;
        }
        if linked
            && let Some(rest) = line.split("path = \"../").nth(1)
            && let Some(name) = rest.split('"').next()
        {
            found.push(name.to_string());
        }
    }
    found
}

#[test]
fn a_crate_named_in_any_dependency_table_but_dev_is_linked() {
    let manifest = "[package]\nname = \"x\"\n\n[dependencies]\na = { path = \"../a\" }\n\n\
        [dev-dependencies]\nd = { path = \"../d\" }\n\n\
        [target.'cfg(unix)'.dependencies]\nb = { path = \"../b\" }\n\n\
        [build-dependencies]\nc = { path = \"../c\" }\n\n\
        [dependencies.e]\npath = \"../e\"\n";
    let mut found = path_dependencies(manifest);
    found.sort();
    assert_eq!(found, vec!["a", "b", "c", "e"]);
}

/// Each binary here speaks over its own standard input and output, which is the control channel
/// when an editor or window launches it. On Linux, a process of the same user could copy those
/// descriptors out of one that has not marked itself non-dumpable (RPCVIEW-8).
#[test]
fn every_binary_that_speaks_over_its_own_stdio_marks_itself_non_dumpable() {
    let binaries = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bin");
    let mut seen = 0;
    let mut unmarked = Vec::new();
    visit(&binaries, &mut |path, text| {
        seen += 1;
        if !marks_itself_before_reading(text) {
            unmarked.push(path.display().to_string());
        }
    });
    assert!(seen > 0, "no binaries found under {binaries:?}");
    assert!(
        unmarked.is_empty(),
        "these binaries do not mark themselves non-dumpable before they read:\n{}",
        unmarked.join("\n")
    );
}

/// Whether `text` calls `refuse_same_user_inspection` on a line of code, before any line that
/// reads standard input.
fn marks_itself_before_reading(text: &str) -> bool {
    let code = |line: &str| line.split("//").next().unwrap_or("").to_string();
    let mark = text
        .lines()
        .position(|line| code(line).contains("refuse_same_user_inspection("));
    let read = text.lines().position(|line| code(line).contains("stdin"));
    match (mark, read) {
        (Some(mark), Some(read)) => mark < read,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

#[test]
fn a_comment_that_names_the_call_does_not_mark_the_process() {
    assert!(!marks_itself_before_reading(
        "fn main() {\n    // See refuse_same_user_inspection().\n}\n"
    ));
}

#[test]
fn a_call_after_standard_input_is_read_is_too_late() {
    let late =
        "fn main() {\n    let line = std::io::stdin();\n    refuse_same_user_inspection();\n}\n";
    assert!(!marks_itself_before_reading(late));
    let early =
        "fn main() {\n    refuse_same_user_inspection();\n    let line = std::io::stdin();\n}\n";
    assert!(marks_itself_before_reading(early));
}
