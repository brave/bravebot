//! The library must stay transport-agnostic.
//!
//! A grep, which is worth more than a convention nobody re-checks. If the library starts
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
                    offences.push(format!(
                        "{}:{}: {forbidden}",
                        path.display(),
                        number + 1
                    ));
                }
            }
        }
    });

    assert!(offences.is_empty(), "the library must not print:\n{}", offences.join("\n"));
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
