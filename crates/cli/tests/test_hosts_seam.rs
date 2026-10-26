//! CHATGPT-2 and XAI-1: the `test-hosts` seam, which points the ChatGPT account and the xAI sign-in
//! at a server of a test's own, is on in no build that ships.
//!
//! A build with the seam reads a variable that names where a token goes. So every Cargo file and
//! every command that builds or releases the program is read for a way to turn the seam on, and a
//! release build refuses it where debug assertions are off or a panic aborts. The release source
//! is read for who names the hosts the account is sent to, who makes a host and who holds the
//! token, and each of those is pinned to the callers the clause lists.
//!
//! The readers belong to the workspace rather than to any one crate, and live here for the reason
//! the other guards of the workspace do: this is the crate the shipped binary is built from. The
//! tests that hold each reader to the shapes it has to see are beside it.
//!
//! [CHATGPT-2]: ../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2
//! [XAI-1]: ../../../docs/specs/xai-sign-in.md#XAI-1

mod shipped;

use shipped::{clause, declared_test_modules, module_files, relative, rust_files, workspace};
use std::path::PathBuf;

/// Every file a release build compiles, with the code it contributes.
fn release_sources() -> Vec<(PathBuf, String)> {
    shipped::sources()
        .into_iter()
        .map(|source| (source.path, source.without_test_modules))
        .collect()
}

/// The code of a source file text as a release build reads it.
fn production_code(source: &str) -> String {
    shipped::code_of(source).without_test_modules
}

/// The features that exist so a test can point a flow at a server of its own
/// ([CHATGPT-2](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2)). A build with one reads a variable
/// that names where a token goes, so no build that ships may have it.
const TEST_SEAMS: [&str; 1] = ["test-hosts"];

/// What a Cargo file says about the test seams: the lines that name one anywhere but in a table of
/// dev-dependencies or the seam's own declaration, `<seam> = []` under `[features]`, and the lines
/// that name one in a table of dev-dependencies. The escapes of a TOML string that write a character
/// by its number are decoded first, since the workspace has no TOML parser to do it (the crate of
/// this test depends on none).
///
/// A seam a crate's `[dependencies]`, its build dependencies, the workspace's catalogue, a feature of
/// any crate (`default` included) or a Cargo configuration file enables is on in the build that ships,
/// where cargo builds a crate's dev-dependencies only for its tests. A declaration that enables
/// something else would turn that on with it.
fn seam_mentions(manifest: &str) -> (Vec<String>, Vec<String>) {
    let (mut outside, mut inside) = (Vec::new(), Vec::new());
    let mut table: Vec<String> = Vec::new();
    // A TOML string may write a character by its number, so `"test\u002dhosts"` is the feature's
    // name, and no parser here decodes it.
    let manifest = with_escapes_decoded(manifest);
    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            table = line
                .trim_matches(|c| c == '[' || c == ']')
                .split('.')
                .map(|part| {
                    part.trim()
                        .trim_matches(|c| c == '"' || c == '\'')
                        .to_string()
                })
                .collect();
            if !TEST_SEAMS.iter().any(|seam| line.contains(seam)) {
                continue;
            }
        }
        let Some(seam) = TEST_SEAMS.iter().find(|seam| line.contains(*seam)) else {
            continue;
        };
        let for_tests = table.iter().any(|part| part == "dev-dependencies");
        let declared = table == ["features"]
            && line
                .split_once('=')
                .is_some_and(|(key, value)| key.trim() == *seam && value.trim() == "[]");
        if for_tests {
            inside.push(line.to_string());
        } else if !declared {
            outside.push(line.to_string());
        }
    }
    (outside, inside)
}

/// Directories no walk of the repository goes into: built output, installed packages, git's own
/// files, a sibling checkout kept under `.worktrees` and a bytecode cache.
const NOT_WALKED: [&str; 5] = [
    "target",
    "node_modules",
    ".git",
    ".worktrees",
    "__pycache__",
];

/// Every file under `directory` that `keep` accepts, by path, depth first.
fn files_where(
    directory: &std::path::Path,
    keep: &impl Fn(&std::path::Path) -> bool,
) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if !NOT_WALKED.iter().any(|skipped| name == *skipped) {
                found.extend(files_where(&path, keep));
            }
        } else if keep(&path) {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// Every manifest in the repository, and every Cargo configuration file, whichever workspace it
/// belongs to: the fuzz package is a workspace of its own and the root's member list does not name
/// it.
fn cargo_files() -> Vec<(String, String)> {
    files_where(&workspace(), &|path| {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let in_cargo_directory = path
            .parent()
            .and_then(std::path::Path::file_name)
            .is_some_and(|directory| directory == ".cargo");
        name == "Cargo.toml" || (in_cargo_directory && name.starts_with("config"))
    })
    .into_iter()
    .map(|path| {
        let text = std::fs::read_to_string(&path).expect("a Cargo file");
        (relative(&path), text)
    })
    .collect()
}

/// The manifests whose `[dev-dependencies]` turn a seam on, which are the crates whose tests run a
/// flow against a stand-in: the command's, for the built binary, and the two front ends', for what
/// each writes of a failed turn. A crate added to this list is a decision about who may name a host
/// in a test.
const ENABLE_A_SEAM_FOR_TESTS: [&str; 3] = [
    "crates/cli/Cargo.toml",
    "crates/tui/Cargo.toml",
    "crates/ui-bridge/Cargo.toml",
];

/// CHATGPT-2: a test seam is turned on by dev-dependencies and by nothing a release build reads.
/// Cargo builds a crate's dev-dependencies, and the features they ask for, only for that crate's
/// tests, so a seam enabled there is in the binary `cargo test` runs and in no binary a release
/// builds. A manifest that enabled it under `[dependencies]`, in `default` or in another feature
/// would put a variable that chooses where a token goes into the shipped binary. Every `Cargo.toml`
/// in the repository is read, the fuzz package's included, and so is every Cargo configuration file,
/// which could pass the feature as a flag. The seams are declared and enabled somewhere, so the test
/// is not satisfied by a tree that dropped them.
#[test]
fn a_test_seam_is_enabled_by_dev_dependencies_alone() {
    let files = cargo_files();
    let names: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
    assert!(
        files.len() > 20
            && names.contains(&"Cargo.toml")
            && names.contains(&"fuzz/Cargo.toml")
            && names.contains(&"crates/skus/Cargo.toml"),
        "{names:?}, so this test would pass by reading almost nothing"
    );

    let mut outside: Vec<(String, String)> = Vec::new();
    let mut enabling: Vec<&str> = Vec::new();
    for (path, manifest) in &files {
        let (named_outside, named_inside) = seam_mentions(manifest);
        for line in named_outside {
            outside.push((path.clone(), line));
        }
        if !named_inside.is_empty() {
            enabling.push(path);
        }
    }
    assert!(
        outside.is_empty(),
        "a test seam is named where a build that ships would read it: {outside:?}"
    );
    // And no manifest or configuration file turns debug assertions on, makes a panic unwind,
    // overrides the release profile or defines a profile cargo does not have of its own.
    let mut bypassing = Vec::new();
    for (path, manifest) in &files {
        for spelling in bypasses_of_the_refusal(manifest) {
            bypassing.push((path.as_str(), spelling.to_string()));
        }
        for name in custom_profiles(path, manifest) {
            bypassing.push((path.as_str(), format!("the profile {name}")));
        }
    }
    assert!(
        bypassing.is_empty(),
        "a Cargo file undoes what makes a release build refuse the test-hosts seam: {bypassing:?}"
    );
    assert_eq!(
        enabling, ENABLE_A_SEAM_FOR_TESTS,
        "the crates whose tests turn a seam on are the ones that run a flow against a stand-in, \
         and a crate added to them is a decision about who may name a host in a test"
    );

    for seam in TEST_SEAMS {
        let mut declaring: Vec<&str> = files
            .iter()
            .filter(|(_, manifest)| {
                manifest
                    .lines()
                    .any(|line| line.trim().replace(' ', "") == format!("{seam}=[]"))
            })
            .map(|(path, _)| path.as_str())
            .collect();
        declaring.sort_unstable();
        assert_eq!(
            declaring,
            ["crates/config/Cargo.toml", "crates/skus/Cargo.toml"],
            "{seam}"
        );
    }
}

/// The reader of the test seams finds one in every place a shipping build would read it, and none
/// in a table of dev-dependencies or the seam's own declaration.
#[test]
fn the_seam_reader_finds_a_seam_wherever_a_build_that_ships_reads_it() {
    let manifest = "\
[workspace.dependencies]
bravebot-skus = { path = \"crates/skus\", features = [\"test-hosts\"] }

[features]
test-hosts = []
default = [\"test-hosts\"]
everything = [\"bravebot-skus/test-hosts\"]

[dependencies]
bravebot-config = { path = \"../config\", features = [\"test-hosts\"] }

[dependencies.bravebot-skus]
features = ['test-hosts']

[target.'cfg(unix)'.build-dependencies]
bravebot-net = { path = \"../net\", features = [\"test-hosts\"] }

[dev-dependencies]
bravebot-skus = { path = \"../skus\", features = [\"test-hosts\"] }

[target.'cfg(unix)'.dev-dependencies.bravebot-config]
features = [\"test-hosts\"]
";
    let (outside, inside) = seam_mentions(manifest);
    assert_eq!(
        outside,
        [
            "bravebot-skus = { path = \"crates/skus\", features = [\"test-hosts\"] }",
            "default = [\"test-hosts\"]",
            "everything = [\"bravebot-skus/test-hosts\"]",
            "bravebot-config = { path = \"../config\", features = [\"test-hosts\"] }",
            "features = ['test-hosts']",
            "bravebot-net = { path = \"../net\", features = [\"test-hosts\"] }",
        ]
    );
    assert_eq!(
        inside,
        [
            "bravebot-skus = { path = \"../skus\", features = [\"test-hosts\"] }",
            "features = [\"test-hosts\"]",
        ]
    );
    assert_eq!(
        seam_mentions("[features]\ntest-hosts = [\"bravebot-net/proxy\"]\n").0,
        ["test-hosts = [\"bravebot-net/proxy\"]"]
    );
    // A feature written with the escapes of a TOML string is the feature.
    for manifest in [
        "[dependencies]\nbravebot-skus = { path = \"../skus\", features = [\"test\\u002dhosts\"] }\n",
        "[dependencies]\nbravebot-skus = { path = \"../skus\", features = [\"\\u0074est-hosts\"] }\n",
        "[features]\ndefault = [\"bravebot-skus/test\\U0000002dhosts\"]\n",
        "[dependencies]\nbravebot-skus = { path = \"../skus\", features = [\"test\\x2dhosts\"] }\n",
    ] {
        assert_eq!(seam_mentions(manifest).0.len(), 1, "{manifest}");
    }
    // The same in a table of dev-dependencies is the use that is allowed.
    let (outside, inside) = seam_mentions(
        "[dev-dependencies]\nbravebot-skus = { path = \"../skus\", features = [\"test\\u002dhosts\"] }\n",
    );
    assert!(outside.is_empty() && inside.len() == 1);
    // A Cargo configuration file has no dependency tables, so a flag in one is a use.
    assert_eq!(
        seam_mentions("[build]\nrustflags = [\"--cfg\", \"feature=\\\"test-hosts\\\"\"]\n")
            .0
            .len(),
        1
    );
    assert_eq!(
        seam_mentions("[alias]\nb = \"build --features bravebot-skus/test-hosts\"\n")
            .0
            .len(),
        1
    );
}

/// The attribute and the `compile_error!` under it that refuse a test seam, as a crate's `lib.rs`
/// writes them: from the `#[cfg(` line above the macro to the line that ends it.
fn seam_refusal(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let refusal = lines
        .iter()
        .position(|line| line.trim_start().starts_with("compile_error!("))
        .expect("a compile_error! that refuses the seam");
    let attribute = (0..refusal)
        .rev()
        .find(|at| lines[*at].starts_with("#[cfg("))
        .expect("a cfg above the compile_error!");
    let end = (refusal..lines.len())
        .find(|at| lines[*at].trim_end().ends_with(");"))
        .expect("the end of the compile_error!");
    lines[attribute..=end].join("\n")
}

/// Whether the compiler refuses `refusal` under `flags`, with the feature on or off: it is built as a
/// crate of its own, so only the attribute's own condition decides. Returns what the compiler said
/// where it did, and `None` where the crate compiled.
#[allow(
    clippy::disallowed_methods,
    reason = "the test runs the compiler the tree is built with, to see what the attribute does under flags a test cannot set on itself"
)]
fn refused_under(refusal: &str, flags: &[&str], feature_on: bool) -> Option<String> {
    let directory = workspace().join("target/test-scratch/chatgpt-seam-refusal");
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    let source = directory.join("refusal.rs");
    std::fs::write(&source, format!("{refusal}\nfn main() {{}}\n")).expect("the refusal");
    let mut command =
        std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
    command
        .args([
            "--edition",
            "2024",
            "--crate-type",
            "bin",
            "--emit=metadata",
        ])
        .arg("--out-dir")
        .arg(&directory)
        .args(flags);
    if feature_on {
        command.args(["--cfg", "feature=\"test-hosts\""]);
    }
    let output = command
        .arg(&source)
        .output()
        .expect("the compiler runs: it is the one that builds this test");
    (!output.status.success()).then(|| String::from_utf8_lossy(&output.stderr).into_owned())
}

/// CHATGPT-2: a release build refuses the seam. Rust has no `cfg` for the release profile, so the
/// refusal reads the two settings that profile has and a test build does not: debug assertions off,
/// and a panic that aborts. A build with debug assertions on and a panic that aborts is the release
/// profile given `debug-assertions = true`, and a build with them off that unwinds is
/// `cargo test --release`; the first is refused by the abort and the second by the assertions. The
/// attribute is compiled under each combination, with the feature on and off, and the root manifest's
/// release profile is read for the setting the abort rests on.
#[test]
fn a_build_that_aborts_on_a_panic_or_asserts_nothing_refuses_the_seam() {
    let read = |path: &str| std::fs::read_to_string(workspace().join(path)).expect("a source");
    let refusal = seam_refusal(&read("crates/skus/src/lib.rs"));
    assert_eq!(
        refusal,
        seam_refusal(&read("crates/config/src/lib.rs")),
        "the two crates that hold the seam refuse it under different conditions"
    );

    let cases: [(&str, &[&str], bool, bool); 7] = [
        (
            "a test or dev build",
            &["-C", "debug-assertions=on"],
            true,
            false,
        ),
        (
            "a dev build whose dependencies are optimised",
            &["-C", "debug-assertions=on", "-C", "opt-level=2"],
            true,
            false,
        ),
        (
            "the release profile",
            &[
                "-C",
                "debug-assertions=off",
                "-C",
                "panic=abort",
                "-C",
                "opt-level=3",
            ],
            true,
            true,
        ),
        (
            "the release profile with debug assertions turned on",
            &[
                "-C",
                "debug-assertions=on",
                "-C",
                "panic=abort",
                "-C",
                "opt-level=3",
            ],
            true,
            true,
        ),
        (
            "a build with debug assertions on and a panic that aborts",
            &["-C", "debug-assertions=on", "-C", "panic=abort"],
            true,
            true,
        ),
        (
            "tests built with the release profile",
            &["-C", "debug-assertions=off", "-C", "opt-level=3"],
            true,
            true,
        ),
        (
            "a release build without the feature",
            &["-C", "debug-assertions=off", "-C", "panic=abort"],
            false,
            false,
        ),
    ];
    for (build, flags, feature_on, refused) in cases {
        let said = refused_under(&refusal, flags, feature_on);
        assert_eq!(
            said.is_some(),
            refused,
            "{build}: expected the seam to be {}, and the compiler said {said:?}",
            if refused { "refused" } else { "accepted" }
        );
        if let Some(said) = said {
            assert!(
                said.contains("the test-hosts feature is for tests"),
                "{build}: the compiler refused for another reason: {said}"
            );
        }
    }

    // The abort the refusal reads as the release profile is the manifest's.
    let manifest = read("Cargo.toml");
    let mut in_release = false;
    let mut release: Vec<&str> = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_release = line == "[profile.release]";
        } else if in_release && !line.is_empty() && !line.starts_with('#') {
            release.push(line);
        }
    }
    assert!(
        release.contains(&"panic = \"abort\""),
        "the release profile does not abort on a panic, which is how the test-hosts refusal tells \
         it from a test build: {release:?}"
    );
    assert!(
        !release
            .iter()
            .any(|line| line.starts_with("debug-assertions")),
        "the release profile states debug assertions: {release:?}"
    );
}

/// The directories the walk for commands does not go into, beyond [`NOT_WALKED`]: the output of a
/// build, which a checkout may hold and a clean one does not.
const NOT_WALKED_FOR_COMMANDS: [&str; 4] = ["dist", "out", "build", ".docusaurus"];

/// The directory the walk for commands leaves out because it says in words what the guards look for.
const SPECS: &str = "docs/specs";

/// The files that list what a lock pins, which a tool writes and which hold no command. Named in
/// full: a name that only contains `lock` would leave out a script called `block.sh`.
const LOCK_FILES: [&str; 5] = [
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "Podfile.lock",
];

/// Whether `path`, relative to the workspace, is a file the walk for commands reads: every file
/// there is, whatever it is called and whatever it ends in, but a Rust source, a lock file, a
/// `Cargo.toml` and what is under [`SPECS`].
///
/// A Rust source is read for who may name a host by the tests of what a release compiles, and a
/// `Cargo.toml` by the manifest guard, which reads its tables. A file that is not text is left out
/// where it is read, in [`command_files`]. The name of a file says nothing of whether it holds a
/// command, so no other name is left out: a script that ends in `.bash`, a `Justfile` and a
/// `Makefile.local` are read as a `Makefile` is.
fn holds_a_command(path: &std::path::Path) -> bool {
    let relative = relative(path);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    !(relative.starts_with(&format!("{SPECS}/"))
        || name == "Cargo.toml"
        || LOCK_FILES.contains(&name)
        || path.extension().is_some_and(|extension| extension == "rs"))
}

/// Every file the walk for commands reads, by path relative to the workspace and its text. A file
/// with a NUL byte in it is an image, an archive or a font, and holds no command.
fn command_files() -> Vec<(String, String)> {
    fn walk(directory: &std::path::Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                let skipped = NOT_WALKED
                    .iter()
                    .chain(NOT_WALKED_FOR_COMMANDS.iter())
                    .any(|skipped| name == *skipped);
                if !skipped {
                    walk(&path, found);
                }
            } else if holds_a_command(&path) {
                found.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(&workspace(), &mut paths);
    paths.sort();
    paths
        .iter()
        .filter_map(|path| {
            let bytes = std::fs::read(path).ok()?;
            (!bytes.contains(&0))
                .then(|| (relative(path), String::from_utf8_lossy(&bytes).into_owned()))
        })
        .collect()
}

/// The commands in the text of a file, one per logical line: a line that ends in a backslash goes on
/// with the next, as a shell and a Makefile read it.
fn logical_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut joined = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        match trimmed.strip_suffix('\\') {
            Some(continued) => {
                joined.push_str(continued.trim_end());
                joined.push(' ');
            }
            None => {
                joined.push_str(trimmed);
                lines.push(std::mem::take(&mut joined));
            }
        }
    }
    if !joined.is_empty() {
        lines.push(joined);
    }
    lines
}

/// `text` with the escapes of a TOML or a JSON string that write a character by its number
/// (`\u002d`, `\U0000002d`, `\x2d`) written out as the character. A manifest has no TOML parser here
/// to do it, and a `package.json` command is a JSON string: either could spell a feature's name with
/// one. A backslash before a backslash is a backslash, so `\\u002d` is left as it is.
fn with_escapes_decoded(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut decoded = String::with_capacity(text.len());
    let mut at = 0;
    while at < characters.len() {
        if characters[at] == '\\' && at + 1 < characters.len() {
            let digits = match characters[at + 1] {
                'u' => 4,
                'U' => 8,
                'x' => 2,
                '\\' => {
                    decoded.push_str("\\\\");
                    at += 2;
                    continue;
                }
                _ => 0,
            };
            let end = at + 2 + digits;
            if digits > 0 && end <= characters.len() {
                let number: String = characters[at + 2..end].iter().collect();
                if let Some(character) = u32::from_str_radix(&number, 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    decoded.push(character);
                    at = end;
                    continue;
                }
            }
        }
        decoded.push(characters[at]);
        at += 1;
    }
    decoded
}

/// `text` with what separates the pieces of one word in a command taken out: white space, quotes,
/// backslashes, commas and plus signs, in lower case with `_` read as `-`, so that a name split over
/// lines, written as a list of arguments, quoted in any way, written with the escapes of a string or
/// spelled as an environment variable is the same text as the name.
fn squeezed(text: &str) -> String {
    with_escapes_decoded(text)
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '`' | '\\' | ',' | '+'))
        .map(|c| {
            if c == '_' {
                '-'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

/// Whether `text` names a test seam in any way a command could: the name is not a word of any
/// command a release runs, so a file that holds it, in a line or beside the flag or on a line of its
/// own, is one a person has to look at.
fn names_a_test_seam(text: &str) -> bool {
    let squeezed = squeezed(text);
    TEST_SEAMS
        .iter()
        .chain(SEAM_VARIABLE_WORDS.iter())
        .any(|word| squeezed.contains(word))
}

/// What the variable that names a stand-in is called, in the form `squeezed` gives it, which is also
/// the part of its name no other variable has. A command that sets it names the seam as a flag does.
const SEAM_VARIABLE_WORDS: [&str; 2] = ["chatgpt-stand-in", "xai-stand-in"];

/// The logical lines of `text` that ask for every feature, which enables each seam for every crate
/// the build selects: `--all-features`, cargo-deny's `all-features = true` and the spellings
/// `squeezed` makes of them.
fn all_features_lines(text: &str) -> Vec<String> {
    logical_lines(text)
        .into_iter()
        .filter(|line| squeezed(line).contains("all-features"))
        .collect()
}

/// The spellings, in the form [`squeezed`] gives them, of the settings that let a release build
/// compile the seam. The refusal reads two things a release has and a test build does not: debug
/// assertions off and a panic that aborts. Turning the assertions on, making a panic unwind, or
/// overriding the release profile from the environment undoes one or the other, with the feature on
/// or without it. Each is the spelling of a flag (`-C debug-assertions`, `-C panic=unwind`), of a
/// profile setting (`debug-assertions = true`, `panic = "unwind"`) or of a variable
/// (`CARGO_PROFILE_RELEASE_*`), in a command, a workflow or a configuration file.
const BYPASSES_OF_THE_REFUSAL: [(&str, &str); 3] = [
    (
        "debug-assertions",
        "turns debug assertions on or states them",
    ),
    ("panic=unwind", "makes a panic unwind"),
    (
        "cargo-profile-release",
        "overrides a setting of the release profile from the environment",
    ),
];

/// Each of [`BYPASSES_OF_THE_REFUSAL`] that `text` spells, however it is split, quoted or capitalised.
fn bypasses_of_the_refusal(text: &str) -> Vec<&'static str> {
    let squeezed = squeezed(text);
    BYPASSES_OF_THE_REFUSAL
        .iter()
        .filter(|(spelling, _)| squeezed.contains(spelling))
        .map(|(spelling, _)| *spelling)
        .collect()
}

/// The profiles cargo has of its own. A profile of another name states what it inherits, and one that
/// inherits `dev` or `release` and is built with `--all-targets` or `--profile <name>` has the
/// settings of one under a name no refusal reads.
const BUILT_IN_PROFILES: [&str; 4] = ["dev", "release", "test", "bench"];

/// What `--profile` names in a file that is not cargo's, by the file and the word after it: the
/// install profile of `rustup`, and a profile of the AWS CLI in a message.
const NOT_A_CARGO_PROFILE: [(&str, &str); 3] = [
    ("Makefile", "minimal"),
    ("Makefile", "<name>"),
    ("docs/development/fuzzing.md", "minimal"),
];

/// Whether `path` is a file cargo reads its settings from: a manifest, or a configuration file.
fn is_a_cargo_file(path: &str) -> bool {
    let (directory, name) = path.rsplit_once('/').unwrap_or(("", path));
    name == "Cargo.toml" || (directory.ends_with(".cargo") && name.starts_with("config"))
}

/// The profiles a cargo file defines, by name: the second part of a `[profile.<name>]` table or of a
/// dotted key that starts `profile.`, and the first part of a key under `[profile]`. A `profile` table
/// written as one inline value cannot be read here, so it is named as what it is.
fn profile_tables(text: &str) -> Vec<String> {
    let unquoted = |part: &str| {
        part.trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string()
    };
    let mut found = Vec::new();
    // Before the first table header a key is at the root of the file, where `profile` is the table.
    let mut at_root = true;
    let mut under_profile = false;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            at_root = false;
            let parts: Vec<String> = line
                .trim_matches(|c| c == '[' || c == ']')
                .split('.')
                .map(unquoted)
                .collect();
            under_profile = parts == ["profile"];
            if parts.first().is_some_and(|first| first == "profile") && parts.len() > 1 {
                found.push(parts[1].clone());
            }
        } else if let Some(key) = line.split('=').next().map(unquoted) {
            if at_root && key == "profile" {
                found.push("an inline table".to_string());
            } else if let Some(rest) = key.strip_prefix("profile.").filter(|_| at_root) {
                found.push(unquoted(rest.split('.').next().unwrap_or_default()));
            } else if under_profile {
                found.push(unquoted(key.split('.').next().unwrap_or_default()));
            }
        }
    }
    found
}

/// The profiles `text`, the file at `path`, asks for or defines that cargo does not have of its own:
/// the word after `--profile`, the profile of a `--config profile.<name>`, the name in a
/// `CARGO_PROFILE_<NAME>_` variable and, in a file cargo reads, the profile a table defines. A
/// comment is not a command, so a line that starts `#` is not read for a flag.
fn custom_profiles(path: &str, text: &str) -> Vec<String> {
    let decoded = with_escapes_decoded(text);
    let commands: String = decoded
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let tokens: Vec<&str> = commands
        .split(|c: char| c.is_whitespace() || "\"'`,=()[]{};".contains(c))
        .filter(|token| !token.is_empty())
        .collect();
    let mut found = Vec::new();
    for (at, token) in tokens.iter().enumerate() {
        let next = tokens.get(at + 1).copied().unwrap_or_default();
        match *token {
            "--profile" => found.push(next.to_string()),
            "--config" => {
                if let Some(rest) = next.strip_prefix("profile.") {
                    found.push(rest.split('.').next().unwrap_or_default().to_string());
                }
            }
            _ => {}
        }
    }
    let squeezed = squeezed(text);
    for (at, _) in squeezed.match_indices("cargo-profile-") {
        let name = squeezed[at + "cargo-profile-".len()..]
            .split('-')
            .next()
            .unwrap_or_default();
        found.push(name.to_string());
    }
    if is_a_cargo_file(path) {
        found.extend(profile_tables(&decoded));
    }
    found.retain(|name| {
        !BUILT_IN_PROFILES.contains(&name.as_str())
            && !NOT_A_CARGO_PROFILE.contains(&(path, name.as_str()))
    });
    found
}

/// The commands that may ask for every feature, each by the file, the text that names it, how many
/// times that text occurs in the file, and why it ships nothing. A command added to a build or a
/// release is not in this list and is refused, so a new use is added here by name, with the reason.
const MAY_ASK_FOR_ALL_FEATURES: [(&str, &str, usize, &str); 13] = [
    (
        "Makefile",
        "cargo clippy --all-targets --all-features -- -D warnings",
        2,
        "a lint run",
    ),
    (
        "Makefile",
        "cargo clippy --target x86_64-pc-windows-gnu --all-targets --all-features --locked",
        1,
        "a lint run",
    ),
    (
        ".github/workflows/ci.yml",
        "cargo clippy --all-targets --all-features -- -D warnings",
        2,
        "a lint run",
    ),
    (
        ".github/workflows/ci.yml",
        "cargo clippy --target x86_64-pc-windows-gnu --all-targets --all-features --locked",
        1,
        "a lint run",
    ),
    (
        ".githooks/pre-commit",
        "cargo clippy --all-targets --all-features -- -D warnings",
        1,
        "a lint run",
    ),
    (
        "contrib/check-lint-increase.py",
        "\"cargo\", \"clippy\", \"--workspace\", \"--lib\", \"--bins\", \"--all-features\"",
        1,
        "a lint run",
    ),
    (
        "agents/pr_worktree.py",
        "\"cargo\", \"clippy\", *flags, \"--all-targets\", \"--all-features\"",
        1,
        "a lint run",
    ),
    (
        "agents/skills/maintenance/maintenance.py",
        "\"cargo\", \"clippy\", \"--workspace\", \"--lib\", \"--bins\", \"--all-features\"",
        1,
        "a lint run",
    ),
    (
        "agents/pr_worktree_selftest.py",
        "\"--all-targets\", \"--all-features\", \"--\", \"-D\", \"warnings\"",
        1,
        "the plan that script is checked against, which runs nothing",
    ),
    (
        "ui/docs/testing.md",
        "cargo clippy --all-targets --all-features -- -D warnings",
        2,
        "a lint run, described",
    ),
    (
        "ui/docs/development.md",
        "cargo clippy --all-targets --all-features -- -D warnings",
        1,
        "a lint run, described",
    ),
    (
        "ui/docs/v0.9.0-implementation.md",
        "cargo clippy --all-targets --all-features -- -D warnings",
        1,
        "a lint run, described",
    ),
    (
        "deny.toml",
        "all-features = true",
        1,
        "cargo-deny reads the dependency graph with every feature on and compiles nothing",
    ),
];

/// A release is made by `make` targets that run `cargo build --release` and `cargo zigbuild
/// --release`, and CHATGPT-2 lets no flag turn on a feature that names a host. A manifest cannot
/// say what a build command passes, so this reads every text file of the repository
/// ([`holds_a_command`]). None may name a seam at all, in a line, a list of arguments, a
/// variable or a line of its own, and `all-features`, which would enable each, is asked for only
/// by the runs [`MAY_ASK_FOR_ALL_FEATURES`] names, and by no spelling that hides it from the lines.
/// None turns debug assertions on, makes a panic unwind or overrides the release profile
/// ([`BYPASSES_OF_THE_REFUSAL`]), and none builds with a profile cargo does not have
/// ([`custom_profiles`]).
#[test]
fn no_build_or_release_command_passes_a_seam_or_all_features_but_the_lint_runs_by_name() {
    let texts = command_files();

    // The files that build a release, and each place a command is kept, are among those read.
    let paths: Vec<&str> = texts.iter().map(|(path, _)| path.as_str()).collect();
    for needed in [
        "Makefile",
        "Dockerfile.cross",
        "deny.toml",
        "install.sh",
        "install.ps1",
        "package.json",
        "README.md",
        ".github/workflows/ci.yml",
        ".githooks/pre-commit",
        "ui/scripts/build-bridge.sh",
        "ui/package.json",
        "ui/README.md",
        "mobile/package.json",
        "npm/scripts/stage-platform-package.js",
        "extension/package.json",
        "packages/agent-client/package.json",
        "packages/agent-client/README.md",
        "docs/development/releasing.md",
        "docs/website/package.json",
        "docs/website/docs/development.md",
        "contrib/check-lint-increase.py",
        ".envrc.example",
        "crates/i18n/locales/en-US.ftl",
        "ui/src/renderer/App.tsx",
    ] {
        assert!(paths.contains(&needed), "{needed} was not read: {paths:?}");
    }
    let has = |file: &str, command: &str| {
        texts
            .iter()
            .filter(|(path, _)| path == file)
            .any(|(_, text)| {
                logical_lines(text)
                    .iter()
                    .any(|line| line.contains(command))
            })
    };
    assert!(
        has("Makefile", "cargo build --release --locked")
            && has("Dockerfile.cross", "cargo zigbuild --release --locked"),
        "the commands that build a release were not found, so this test would pass by reading none"
    );

    let mut naming = Vec::new();
    for (path, text) in &texts {
        if names_a_test_seam(text) {
            naming.push(path.as_str());
        }
    }
    assert!(
        naming.is_empty(),
        "{naming:?} name a test seam, which no command a release runs does: a flag, a variable or a \
         list of arguments with it would put a way to name a host into whatever the command builds"
    );

    // Nothing turns debug assertions on, makes a panic unwind or overrides the release profile, any of
    // which would let a release build compile the seam, and nothing builds with a profile that cargo
    // does not have of its own, which can inherit the settings of `dev`.
    let mut bypassing = Vec::new();
    let mut profiles = Vec::new();
    for (path, text) in &texts {
        for spelling in bypasses_of_the_refusal(text) {
            bypassing.push((path.as_str(), spelling));
        }
        for name in custom_profiles(path, text) {
            profiles.push((path.as_str(), name));
        }
    }
    assert!(
        bypassing.is_empty(),
        "{bypassing:?} turn off what makes a release build refuse the test-hosts seam: {BYPASSES_OF_THE_REFUSAL:?}"
    );
    assert!(
        profiles.is_empty(),
        "{profiles:?} name a profile that cargo does not have of its own, which may inherit `dev` and \
         is built without the refusal that a release has"
    );

    let mut allowed = 0;
    for (file, needle, times, why) in MAY_ASK_FOR_ALL_FEATURES {
        allowed += times;
        let found = texts
            .iter()
            .filter(|(path, _)| path == file)
            .flat_map(|(_, text)| all_features_lines(text))
            .filter(|line| line.contains(needle))
            .count();
        assert_eq!(
            found, times,
            "{file}: `{needle}` ({why}) was expected {times} times and found {found}"
        );
    }
    let mut every: Vec<(String, usize)> = Vec::new();
    for (path, text) in &texts {
        let lines = all_features_lines(text).len();
        // The word where it is written over lines or in pieces is a use that the lines do not show.
        let hidden = squeezed(text).matches("all-features").count();
        assert_eq!(
            lines, hidden,
            "{path} asks for every feature in a form the lines of it do not show"
        );
        if lines > 0 {
            every.push((path.clone(), lines));
        }
    }
    assert_eq!(
        every.iter().map(|(_, lines)| lines).sum::<usize>(),
        allowed,
        "a command asks for every feature that is not named in MAY_ASK_FOR_ALL_FEATURES, and that \
         enables each test seam for every crate the build selects: {every:#?}"
    );
}

/// The command reader has to see each way a build can be given a seam, in a line, a list of
/// arguments, a variable, a line of its own and a word split over lines, and has to see a request
/// for every feature however it is spelled, or the test above passes on a command it cannot read.
/// It has to pass over a command that names neither.
#[test]
fn the_command_reader_sees_a_seam_and_every_feature_however_a_command_spells_them() {
    for (command, spelling) in [
        (
            "cargo build --release --features bravebot-skus/test-hosts",
            "a feature of a crate",
        ),
        (
            "cargo zigbuild --release --target x --features test-hosts",
            "a feature of the package",
        ),
        (
            "cargo build --release -p bravebot-config -F test-hosts",
            "the short flag",
        ),
        (
            "RUSTFLAGS='--cfg feature=\"test-hosts\"' cargo build --release",
            "a cfg of the feature",
        ),
        (
            "cargo build --release \\\n    -p bravebot-cli \\\n    --features=bravebot-skus/test-hosts",
            "a command over three lines",
        ),
        (
            "cargo\", \"build\", \"--features\", \"bravebot-skus/test-hosts\"",
            "a list of arguments",
        ),
        (
            "args:\n  - --features\n  - test-hosts\n",
            "a list of arguments in a workflow, the name on a line of its own",
        ),
        (
            "FEATURES=(\n  --features\n  \"test-hosts\"\n)\n",
            "a shell array, the name quoted on a line of its own",
        ),
        (
            "features = [\n  \"bravebot-skus/test-hosts\",\n]\n",
            "a list in a manifest",
        ),
        (
            "export CARGO_FEATURE_TEST_HOSTS=1",
            "the variable cargo sets for a feature",
        ),
        (
            "cargo build --features test_hosts",
            "the name written with an underscore",
        ),
        ("cargo build --features TEST-HOSTS", "the name in capitals"),
        (
            "{\"scripts\": {\"build\": \"cargo build --release --features test\\u002dhosts\"}}",
            "the name written with a JSON escape",
        ),
        (
            "cargo build --features test\\U0000002dhosts",
            "the name written with a long escape",
        ),
        (
            "cargo build --features test\\x2dhosts",
            "the name written with a byte escape",
        ),
        (
            "cargo build --features \\u0074est-hosts",
            "a letter of the name written with an escape",
        ),
        (
            "cargo build --features test-\\\nhosts",
            "the name split by a continuation",
        ),
        (
            "cargo build --features 'test-' 'hosts'",
            "the name quoted in two pieces",
        ),
        (
            "BRAVEBOT_TEST_CHATGPT_STAND_IN=http://127.0.0.1:1 bravebot auth login chatgpt",
            "the variable that names the ChatGPT stand-in",
        ),
        (
            "BRAVEBOT_TEST_XAI_STAND_IN=http://127.0.0.1:1 bravebot auth login xai",
            "the variable that names the xAI stand-in",
        ),
        (
            "env BRAVEBOT_TEST_XAI_\\\nSTAND_IN=x bravebot",
            "the xAI variable split by a continuation",
        ),
    ] {
        assert!(
            names_a_test_seam(command),
            "{spelling} was not seen: {command}"
        );
    }
    for command in [
        "cargo build --release --locked",
        "cargo zigbuild --release --locked --target x86_64-unknown-linux-musl",
        "pnpm run test",
        // A backslash before a backslash is a backslash, so this is not an escape.
        "cargo build --features test\\\\u002dhosts",
    ] {
        assert!(!names_a_test_seam(command), "{command}");
        assert!(all_features_lines(command).is_empty(), "{command}");
    }
    for (command, spelling) in [
        (
            "cargo build --release --workspace --all-features",
            "the flag",
        ),
        (
            "cargo build --release \\\n  --workspace \\\n  --all-features",
            "the flag over three lines",
        ),
        ("[graph]\nall-features = true\n", "cargo-deny's setting"),
        ("args:\n  - --all-features\n", "a list of arguments"),
        ("cargo build --all_features", "an underscore"),
        ("cargo build --ALL-FEATURES", "capitals"),
    ] {
        assert!(
            !all_features_lines(command).is_empty(),
            "{spelling} was not seen: {command}"
        );
    }
    let (joined, _) = (
        all_features_lines("cargo build --release \\\n  --workspace \\\n  --all-features"),
        (),
    );
    assert_eq!(joined, ["cargo build --release --workspace --all-features"]);
    // A word split by a line continuation is one logical line that holds it once, so the lines show
    // it as often as the whole text does, which is what the test above compares.
    let split = "cargo build --all-\\\nfeatures";
    assert_eq!(squeezed(split).matches("all-features").count(), 1);
    assert_eq!(all_features_lines(split).len(), 1);
}

/// The reader of what undoes the refusal and of the profiles cargo does not have has to see each way
/// a command, a workflow or a configuration file spells them, and has to pass over what is not one.
/// Each is a way past a guard that reads only the feature's name.
#[test]
fn the_command_reader_sees_what_undoes_the_refusal_and_a_profile_of_its_own() {
    for (text, spelling) in [
        (
            "RUSTFLAGS=\"-C panic=unwind\" cargo build --release",
            "RUSTFLAGS with an unwinding panic",
        ),
        (
            "RUSTFLAGS='-Cdebug-assertions=on' cargo build --release",
            "RUSTFLAGS with the short form of the flag",
        ),
        (
            "env:\n  CARGO_ENCODED_RUSTFLAGS: \"-Cdebug-assertions\"",
            "CARGO_ENCODED_RUSTFLAGS in a workflow",
        ),
        (
            "export CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true",
            "the release profile's variable",
        ),
        (
            "export CARGO_PROFILE_RELEASE_PANIC=unwind",
            "the release profile's panic",
        ),
        (
            "cargo build --release --config 'profile.release.panic=\"unwind\"'",
            "a --config of the release profile",
        ),
        (
            "[profile.release]\npanic = \"unwind\"\n",
            "a table of the release profile",
        ),
        (
            "[build]\nrustflags = [\"-C\", \"debug-assertions=on\"]\n",
            "a configuration file's rustflags",
        ),
        (
            "cargo build --release \\\n  -C debug\\\n-assertions",
            "a word split over lines",
        ),
        (
            "RUSTFLAGS=-C DEBUG_ASSERTIONS=1",
            "capitals and an underscore",
        ),
    ] {
        assert!(
            !bypasses_of_the_refusal(text).is_empty(),
            "{spelling} was not seen: {text}"
        );
    }
    for text in [
        "cargo build --release --locked",
        "RUSTFLAGS=\"-D warnings\" cargo build --release",
        "[profile.release]\npanic = \"abort\"\nlto = true\n",
        "env:\n  CARGO_PROFILE_DEV_DEBUG: line-tables-only",
    ] {
        assert!(bypasses_of_the_refusal(text).is_empty(), "{text}");
    }

    let profiles = |path: &str, text: &str| custom_profiles(path, text);
    for (path, text, spelling) in [
        (
            "build.sh",
            "cargo build --profile shipping --all-targets",
            "--profile",
        ),
        ("build.sh", "cargo build --profile=shipping", "--profile="),
        (
            "ci.yml",
            "args:\n  - cargo\n  - build\n  - --profile\n  - shipping\n",
            "a list of arguments",
        ),
        (
            "build.sh",
            "cargo build --config 'profile.shipping.inherits=\"dev\"'",
            "--config",
        ),
        (
            "build.sh",
            "export CARGO_PROFILE_SHIPPING_INHERITS=dev",
            "a variable",
        ),
        (
            "crates/x/Cargo.toml",
            "[profile.shipping]\ninherits = \"dev\"\n",
            "a table",
        ),
        (
            ".cargo/config.toml",
            "[profile.shipping.package.\"*\"]\nopt-level = 3\n",
            "a table of a package of a profile",
        ),
        (
            "Cargo.toml",
            "[profile]\nshipping.inherits = \"dev\"\n",
            "a key under [profile]",
        ),
        (
            "Cargo.toml",
            "profile.shipping.inherits = \"dev\"\n",
            "a dotted key",
        ),
        (
            "Cargo.toml",
            "profile = { shipping = { inherits = \"dev\" } }\n",
            "an inline table",
        ),
        (
            "other.sh",
            "rustup toolchain install stable --profile minimal",
            "a profile of another tool, in a file that does not name it",
        ),
    ] {
        assert!(
            !profiles(path, text).is_empty(),
            "{spelling} was not seen: {text}"
        );
    }
    for (path, text) in [
        ("build.sh", "cargo build --release --locked"),
        ("build.sh", "cargo build --profile release"),
        ("build.sh", "cargo test --profile dev"),
        ("build.sh", "# cargo build --profile shipping"),
        (
            "ci.yml",
            "env:\n  CARGO_PROFILE_DEV_DEBUG: line-tables-only",
        ),
        ("Cargo.toml", "[profile.release]\npanic = \"abort\"\n"),
        ("Cargo.toml", "[profile.dev.package.\"*\"]\nopt-level = 2\n"),
        ("Cargo.toml", "[dependencies]\nprofile = \"1\"\n"),
        (
            "Makefile",
            "rustup toolchain install nightly --profile minimal",
        ),
        (
            "crates/a/src/profile.json",
            "{\"profile.json\": 1, \"user.profile.name\": 2}",
        ),
    ] {
        // A key named `profile` under `[dependencies]` is not a profile table.
        let found = profiles(path, text);
        assert!(found.is_empty(), "{path}: {text}: {found:?}");
    }
}

/// CHATGPT-2 says what the guards read, and the lists they read are the lists the clause names:
/// every directory the walk leaves out, every kind of file it leaves out, the spellings it refuses
/// and the profiles it allows.
#[test]
fn the_clause_names_what_the_command_guard_reads_and_leaves_out() {
    let spec = std::fs::read_to_string(workspace().join("docs/specs/chatgpt-sign-in.md"))
        .expect("the spec");
    let body = clause(&spec, "CHATGPT-2");
    let mut missing = Vec::new();
    for name in NOT_WALKED
        .iter()
        .chain(NOT_WALKED_FOR_COMMANDS.iter())
        .chain(LOCK_FILES.iter())
        .chain(BUILT_IN_PROFILES.iter())
        .chain([SPECS, "Cargo.toml", ".rs"].iter())
    {
        if !body.contains(&format!("`{name}`")) {
            missing.push((*name).to_string());
        }
    }
    for (spelling, _) in BYPASSES_OF_THE_REFUSAL {
        // The spelling is in the form `squeezed` gives it, and the clause names the flag or the
        // setting, so what is looked for is the part of it a person would write.
        let written = spelling.replace("cargo-profile-release", "CARGO_PROFILE_RELEASE");
        if !body.contains(&written) {
            missing.push(written);
        }
    }
    for (file, word) in NOT_A_CARGO_PROFILE {
        if !body.contains(&format!("`{file}`")) || !body.contains(&format!("`{word}`")) {
            missing.push(format!("{file}: {word}"));
        }
    }
    assert!(
        missing.is_empty(),
        "CHATGPT-2 does not name what the guard reads or leaves out: {missing:?}"
    );
    // And what the clause allows `--all-features` to is what the test allows.
    for (file, _, _, _) in MAY_ASK_FOR_ALL_FEATURES {
        assert!(
            body.contains(&format!("`{file}`")),
            "CHATGPT-2 does not name `{file}`, which may ask for every feature"
        );
    }
}

/// The walk for commands reads a file whatever it is called: a script that ends in `.bash`, a
/// `Justfile`, a `Makefile.local`. It leaves out the kinds of file the clause names and no other.
#[test]
fn the_command_walk_reads_a_file_whatever_it_is_called_and_leaves_out_what_the_clause_names() {
    let at = |path: &str| workspace().join(path);
    for read in [
        "build.bash",
        "Justfile",
        "Makefile.local",
        "scripts/release.zsh",
        "deploy",
        "ci/build.cmd",
        "build.sh.in",
        "block.sh",
        "clock.py",
        "build.lock",
        "tools/Taskfile.yml",
        "fuzz/run",
    ] {
        assert!(
            holds_a_command(&at(read)),
            "{read} is a file the walk does not read"
        );
    }
    for left_out in [
        "crates/x/src/lib.rs",
        "crates/x/Cargo.toml",
        "Cargo.lock",
        "fuzz/Cargo.lock",
        "ui/pnpm-lock.yaml",
        "docs/specs/chatgpt-sign-in.md",
        "docs/specs/tools/run.md",
    ] {
        assert!(
            !holds_a_command(&at(left_out)),
            "{left_out} is a file the walk reads"
        );
    }
    // And what the walk found is most of the repository's text: the files of every kind a command
    // could be kept in.
    let paths: Vec<String> = command_files().into_iter().map(|(path, _)| path).collect();
    assert!(
        paths.len() > 600,
        "{} files were read, so this walk would pass by reading almost nothing",
        paths.len()
    );
    for extension in ["css", "svg", "html", "ftl", "txt", "properties", "xml"] {
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with(&format!(".{extension}"))),
            "no .{extension} file was read"
        );
    }
    assert!(
        !paths.iter().any(|path| path.ends_with(".rs")),
        "a Rust source was read"
    );
}

/// Where `needle` appears in the source a release build compiles, by file relative to the
/// workspace and how many times, for every file under a crate's `src`.
fn release_callers(needle: &str) -> Vec<(String, usize)> {
    let mut found: Vec<(String, usize)> = release_sources()
        .into_iter()
        .filter_map(|(file, code)| {
            let count = code.matches(needle).count();
            (count > 0).then(|| (relative(&file), count))
        })
        .collect();
    found.sort();
    found
}

/// The text of each argument of each call that starts at `needle` in `code`, split at the commas
/// that are not inside a bracket.
fn call_arguments(code: &str, needle: &str) -> Vec<Vec<String>> {
    let mut calls = Vec::new();
    for (at, _) in code.match_indices(needle) {
        let mut depth = 1;
        let mut arguments = vec![String::new()];
        for character in code[at + needle.len()..].chars() {
            match character {
                '(' | '[' | '{' | '<' => depth += 1,
                ')' | ']' | '}' | '>' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                break;
            }
            match (character, depth) {
                (',', 1) => arguments.push(String::new()),
                (other, _) => arguments.last_mut().expect("an argument").push(other),
            }
        }
        calls.push(
            arguments
                .into_iter()
                .map(|argument| argument.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|argument| !argument.is_empty())
                .collect(),
        );
    }
    calls
}

/// [`call_arguments`] over the source a release build compiles.
fn release_call_arguments(needle: &str) -> Vec<Vec<String>> {
    release_sources()
        .into_iter()
        .flat_map(|(_, code)| call_arguments(&code, needle))
        .collect()
}

/// The functions in `code` that take `word` as the type of a parameter, by name: the name after
/// `fn` whose parenthesised parameter list holds `word` as a whole identifier.
fn functions_taking(code: &str, word: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (at, _) in code.match_indices("fn ") {
        let before = code[..at].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = &code[at + "fn ".len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let Some(open) = rest.find('(') else {
            continue;
        };
        let mut depth = 0;
        let mut parameters = String::new();
        for character in rest[open..].chars() {
            match character {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            parameters.push(character);
            if depth == 0 {
                break;
            }
        }
        let holds = parameters
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|identifier| identifier == word);
        if holds && !name.is_empty() {
            found.push(name);
        }
    }
    found
}

/// CHATGPT-2 says a release build sends the account's tokens only to the hosts compiled in. What
/// makes that so is who may name a host, and that is pinned here as the callers of what can. `Hosts`
/// is the sign-in's destinations: the compiled-in ones are made by one constructor, a stand-in's by
/// `Hosts::at`, which exists only with the seam, and the functions that take a `Hosts` are those of
/// the sign-in's own module and of the module of the renewer. The account's serving host is the base address of a `Provider`, which
/// `Provider::chatgpt_account` builds from a constant, and `ResponsesClient::for_account` and
/// `list_from_account` send the access token to whatever provider they are given, so each has one
/// caller, and the argument at that call is the account's provider, which one function of the agent
/// holds. A second caller of any of these, or a function that takes a `Hosts` that is not named
/// here, is a second place that can say where a token goes, and this fails until the clause says why.
#[test]
fn only_the_agent_names_the_host_the_account_is_sent_to_and_no_release_calls_the_seam() {
    let chatgpt_agent = "crates/agent/src/chatgpt.rs".to_string();
    let xai_agent = "crates/agent/src/xai.rs".to_string();
    let backend = "crates/agent/src/backend.rs".to_string();
    let chatgpt_command = "crates/cli/src/chatgpt.rs".to_string();
    // What is called, and the files and counts allowed to call it.
    let allowed: [(&str, Vec<(String, usize)>); 8] = [
        // The constructor that is not a constant exists with the seam, and nothing a release
        // compiles calls it.
        ("Hosts::at(", vec![]),
        // The compiled-in hosts are asked for by each renewer's source and by each command.
        (
            "Hosts::compiled_in",
            vec![
                (chatgpt_agent.clone(), 1),
                (xai_agent.clone(), 1),
                (chatgpt_command.clone(), 1),
                ("crates/cli/src/xai.rs".to_string(), 1),
            ],
        ),
        // The one renewer of the process for each account.
        (
            "Renewer::new(",
            vec![(chatgpt_agent.clone(), 1), (xai_agent.clone(), 1)],
        ),
        // The account's own client and its listing, the two calls that send the access token.
        ("ResponsesClient::for_account(", vec![(backend.clone(), 1)]),
        ("list_from_account(", vec![(chatgpt_agent.clone(), 1)]),
        ("account_models_from(", vec![(chatgpt_agent.clone(), 1)]),
        // The account's provider is held by one function, which the agent calls where it needs the
        // account's address: the definition, the roster, the client and the address of the
        // listing's route.
        (
            "account_provider()",
            vec![(backend.clone(), 2), (chatgpt_agent.clone(), 2)],
        ),
        // Where the provider is built: the agent's one holder, and no front end.
        (
            "Provider::chatgpt_account",
            vec![(chatgpt_agent.clone(), 1)],
        ),
    ];
    for (needle, expected) in allowed {
        assert_eq!(
            release_callers(needle),
            expected,
            "`{needle}` has a caller that CHATGPT-2 does not name. A release build sends the \
             account's tokens to the hosts it names, so a new place that names or reads one is a \
             change to that clause"
        );
    }

    // The provider each of the two calls that send the token is given is the account's.
    let clients = release_call_arguments("ResponsesClient::for_account(");
    assert_eq!(clients.len(), 1);
    assert_eq!(
        clients[0].get(1).map(String::as_str),
        Some("crate::chatgpt::account_provider()"),
        "{clients:?}"
    );
    let listings = release_call_arguments("list_from_account(");
    assert_eq!(listings.len(), 1);
    assert_eq!(listings[0].get(1).map(String::as_str), Some("provider"));
    let rosters = release_call_arguments("account_models_from(");
    assert_eq!(rosters.len(), 1);
    assert_eq!(
        rosters[0].get(2).map(String::as_str),
        Some("account_provider()"),
        "the roster of the account is asked of a provider that is not the account's: {rosters:?}"
    );

    // The functions that take the sign-in's destinations are the sign-in's own.
    let mut taking: Vec<(String, String)> = release_sources()
        .into_iter()
        .filter(|(file, code)| {
            relative(file).starts_with("crates/skus/src/chatgpt/")
                || relative(file).starts_with("crates/skus/src/oauth/")
                || code.contains("bravebot_skus::chatgpt")
        })
        .flat_map(|(file, code)| {
            let file = relative(&file);
            functions_taking(&code, "Hosts")
                .into_iter()
                .map(move |name| (file.clone(), name))
        })
        .collect();
    taking.sort();
    let skus =
        |file: &str, name: &str| (format!("crates/skus/src/chatgpt/{file}"), name.to_string());
    let oauth =
        |file: &str, name: &str| (format!("crates/skus/src/oauth/{file}"), name.to_string());
    assert_eq!(
        taking,
        [
            skus("account.rs", "renew"),
            skus("authorize.rs", "url"),
            skus("exchange.rs", "exchange"),
            skus("exchange.rs", "refresh"),
            skus("id_token.rs", "key_set"),
            skus("id_token.rs", "verify"),
            skus("mod.rs", "run"),
            skus("mod.rs", "sign_in"),
            oauth("mod.rs", "renew"),
            oauth("renew.rs", "at"),
            oauth("renew.rs", "new"),
        ],
        "a function takes the sign-in's destinations that CHATGPT-2 does not name"
    );
}

/// The gate on every item that reads the variable naming a stand-in or makes a host from it.
const SEAM_GATE: &str = "#[cfg(all(feature = \"test-hosts\", debug_assertions))]";

/// The text of the item in `code` that starts at `marker`, to the brace that closes its body, with
/// white space collapsed so that a change of wrapping is not a change of the item.
fn item_text(code: &str, marker: &str) -> String {
    assert_eq!(
        code.matches(marker).count(),
        1,
        "`{marker}` is not in the source exactly once"
    );
    let rest = &code[code.find(marker).expect("the marker")..];
    let open = rest.find('{').expect("an item with a body");
    let mut depth = 0;
    let mut end = rest.len();
    for (at, character) in rest[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + at + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    rest[..end].split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The line of `code` that holds `declaration` and the line above it, white space collapsed.
fn declared_under(code: &str, declaration: &str) -> (String, String) {
    let lines: Vec<&str> = code.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.contains(declaration))
        .unwrap_or_else(|| panic!("no `{declaration}`"));
    let squeeze = |line: &str| line.split_whitespace().collect::<Vec<_>>().join(" ");
    (squeeze(lines[at.saturating_sub(1)]), squeeze(lines[at]))
}

/// Where `name` is used as a whole identifier in the source a release build compiles, by file and
/// how many times.
fn identifier_uses(name: &str) -> Vec<(String, usize)> {
    let mut found: Vec<(String, usize)> = release_sources()
        .into_iter()
        .filter_map(|(file, code)| {
            let count = code
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .filter(|word| *word == name)
                .count();
            (count > 0).then(|| (relative(&file), count))
        })
        .collect();
    found.sort();
    found
}

/// CHATGPT-2: what reads the variable that names a stand-in, what makes a host, and what holds an
/// access token are pinned where a release compiles them. The callers of the functions are pinned
/// above, which leaves a read inside one of them: the bodies of `Hosts::compiled_in`,
/// `chatgpt_serving_host` and `Provider::chatgpt_account` are compared to the text the clause is
/// written about, so an environment read that is not behind the gate, or a host that is not one of
/// the four constants, is a change to a text this test holds. The variable's name is a literal in two
/// gated constants and an identifier in two gated reads, and in nothing else a release compiles or a
/// build script or command holds. The access token is asked for, and put in a request, in the places
/// the clause names.
#[test]
fn the_bodies_that_make_a_host_and_every_reader_of_the_stand_in_variable_and_the_token_are_pinned()
{
    let sources = release_sources();
    let code_of = |file: &str| {
        sources
            .iter()
            .find(|(path, _)| relative(path) == file)
            .map(|(_, code)| code.clone())
            .unwrap_or_else(|| panic!("{file} is not a release source"))
    };
    let skus = code_of("crates/skus/src/chatgpt/mod.rs");
    let provider = code_of("crates/config/src/provider.rs");

    assert_eq!(
        item_text(&skus, "pub fn compiled_in()"),
        format!(
            "pub fn compiled_in() -> Self {{ {SEAM_GATE} if let Some(base) = \
             std::env::var(STAND_IN).ok().filter(|base| !base.is_empty()) {{ return Self::at(&base); }} \
             Self {{ authorize: \"https://auth.openai.com/api/accounts/authorize\".to_string(), \
             token: \"https://auth.openai.com/api/accounts/oauth/token\".to_string(), \
             keys: \"https://auth.openai.com/.well-known/jwks.json\".to_string(), }} }}"
        ),
        "Hosts::compiled_in is not the body CHATGPT-2 is written about"
    );
    assert_eq!(
        item_text(&provider, "fn chatgpt_serving_host()"),
        format!(
            "fn chatgpt_serving_host() -> String {{ {SEAM_GATE} if let Some(base) = \
             std::env::var(CHATGPT_STAND_IN) .ok() .filter(|base| !base.is_empty()) \
             {{ return format!(\"{{base}}/v1\"); }} CHATGPT_SERVING_HOST.to_string() }}"
        ),
        "chatgpt_serving_host is not the body CHATGPT-2 is written about"
    );
    assert_eq!(
        item_text(&provider, "pub fn chatgpt_account()"),
        "pub fn chatgpt_account() -> Self { let entry = serde_json::json!({ \"name\": \"ChatGPT\", \
         \"npm\": RESPONSES_SDK, \"options\": {\"baseURL\": chatgpt_serving_host()}, }); \
         let serde_json::Value::Object(entry) = entry else { unreachable!(\"a JSON object literal\"); }; \
         match Self::one(CHATGPT_ID, &entry) { Some(account) => account, \
         None => unreachable!(\"an entry that states its endpoint is reachable\"), } }",
        "Provider::chatgpt_account is not the body CHATGPT-2 is written about"
    );
    let (_, constant) = declared_under(&provider, "const CHATGPT_SERVING_HOST");
    assert_eq!(
        constant,
        "const CHATGPT_SERVING_HOST: &str = \"https://api.openai.com/v1\";"
    );

    // The variable's name is a literal in two constants and nowhere else a release compiles, and each
    // constant is under the gate.
    assert_eq!(
        release_callers("BRAVEBOT_TEST_CHATGPT_STAND_IN"),
        [
            ("crates/config/src/provider.rs".to_string(), 1),
            ("crates/skus/src/chatgpt/mod.rs".to_string(), 1),
        ],
        "a source a release compiles holds the name of the variable that names a stand-in"
    );
    for (code, constant) in [
        (&skus, "pub const STAND_IN:"),
        (&provider, "pub const CHATGPT_STAND_IN:"),
    ] {
        let (above, _) = declared_under(code, constant);
        assert_eq!(above, SEAM_GATE, "{constant} is not under the gate");
    }
    // The constants are read in the two bodies above and nowhere else.
    assert_eq!(
        identifier_uses("STAND_IN"),
        [("crates/skus/src/chatgpt/mod.rs".to_string(), 2)]
    );
    assert_eq!(
        identifier_uses("CHATGPT_STAND_IN"),
        [("crates/config/src/provider.rs".to_string(), 2)]
    );
    // A build script is code a release runs at build time, and reads neither the variable nor the
    // feature.
    let mut scripts = 0;
    for krate in std::fs::read_dir(workspace().join("crates")).expect("the crates directory") {
        let script = krate.expect("a crate").path().join("build.rs");
        if let Ok(text) = std::fs::read_to_string(&script) {
            scripts += 1;
            assert!(
                !names_a_test_seam(&text),
                "{} names the variable or the feature of a test seam",
                script.display()
            );
        }
    }
    assert!(scripts >= 4, "{scripts} build scripts were read");

    // The access token is asked for in the account's client and the agent, which are the two callers
    // of the trait that gives it, and the renewer is the one thing that implements the call.
    assert_eq!(
        release_callers(".access_token()"),
        [
            ("crates/agent/src/chatgpt.rs".to_string(), 1),
            ("crates/agent/src/xai.rs".to_string(), 1),
            ("crates/aichat/src/responses.rs".to_string(), 1),
        ]
    );
    assert_eq!(
        release_callers("fn access_token("),
        [
            ("crates/agent/src/chatgpt.rs".to_string(), 1),
            ("crates/agent/src/xai.rs".to_string(), 2),
            ("crates/aichat/src/lib.rs".to_string(), 1),
            ("crates/skus/src/oauth/renew.rs".to_string(), 1),
        ]
    );
    assert_eq!(
        release_callers("Renewer::access_token"),
        [
            ("crates/agent/src/chatgpt.rs".to_string(), 1),
            ("crates/agent/src/xai.rs".to_string(), 1),
        ]
    );
    // The token that goes to the listing is the one the renewer made, as text, and no other.
    let listings = release_call_arguments("list_from_account(");
    assert_eq!(
        listings[0].get(2).map(String::as_str),
        Some("token.expose()")
    );
}

/// XAI-1: the xAI sign-in sends its tokens to the two endpoints of one issuer, and the access token
/// goes to one serving host. What makes the hosts is pinned where a release compiles it. The bodies of
/// `Hosts::compiled_in`, `xai_serving_host` and `Provider::xai_account` are compared to the text the
/// clause is written about, so an environment read that is not behind the gate, or a host that is not
/// one of the three constants, is a change to a text this test holds. The variable that names a
/// stand-in is a literal in two gated constants and an identifier in four places, and in nothing else
/// a release compiles. The functions of the sign-in that take the destinations are those the clause
/// names.
#[test]
fn the_xai_hosts_are_three_constants_and_the_variable_that_names_a_stand_in_is_read_in_two_gated_places()
 {
    let sources = release_sources();
    let xai: Vec<(String, String)> = sources
        .iter()
        .map(|(file, code)| (relative(file), code.clone()))
        .filter(|(file, _)| file.starts_with("crates/skus/src/xai/"))
        .collect();
    let module = xai
        .iter()
        .find(|(file, _)| file == "crates/skus/src/xai/mod.rs")
        .map(|(_, code)| code.clone())
        .expect("the xAI module is a release source");

    assert_eq!(
        item_text(&module, "pub fn compiled_in()"),
        format!(
            "pub fn compiled_in() -> Self {{ {SEAM_GATE} if let Some(base) = \
             std::env::var(XAI_STAND_IN) .ok() .filter(|base| !base.is_empty()) \
             {{ return Self::at(&base); }} \
             Self {{ device: \"https://auth.x.ai/oauth2/device/code\".to_string(), \
             token: \"https://auth.x.ai/oauth2/token\".to_string(), }} }}"
        ),
        "xai::Hosts::compiled_in is not the body XAI-1 is written about"
    );
    let provider = sources
        .iter()
        .find(|(file, _)| relative(file) == "crates/config/src/provider.rs")
        .map(|(_, code)| code.clone())
        .expect("the provider module is a release source");
    assert_eq!(
        item_text(&provider, "fn xai_serving_host()"),
        format!(
            "fn xai_serving_host() -> String {{ {SEAM_GATE} if let Some(base) = \
             std::env::var(XAI_STAND_IN) .ok() .filter(|base| !base.is_empty()) \
             {{ return format!(\"{{base}}/v1\"); }} XAI_SERVING_HOST.to_string() }}"
        ),
        "xai_serving_host is not the body XAI-1 is written about"
    );
    assert_eq!(
        item_text(&provider, "pub fn xai_account()"),
        "pub fn xai_account() -> Self { let entry = serde_json::json!({ \"name\": \"xAI\", \
         \"options\": {\"baseURL\": xai_serving_host()}, }); \
         let serde_json::Value::Object(entry) = entry else { unreachable!(\"a JSON object literal\"); }; \
         match Self::one(XAI_ID, &entry) { Some(account) => account, \
         None => unreachable!(\"an entry that states its endpoint is reachable\"), } }",
        "Provider::xai_account is not the body XAI-1 is written about"
    );
    let (_, constant) = declared_under(&provider, "const XAI_SERVING_HOST");
    assert_eq!(
        constant,
        "const XAI_SERVING_HOST: &str = \"https://api.x.ai/v1\";"
    );
    assert_eq!(
        release_callers("BRAVEBOT_TEST_XAI_STAND_IN"),
        [
            ("crates/config/src/provider.rs".to_string(), 1),
            ("crates/skus/src/xai/mod.rs".to_string(), 1),
        ],
        "a source a release compiles holds the name of the variable that names a stand-in"
    );
    for (code, constant) in [
        (&module, "pub const XAI_STAND_IN:"),
        (&provider, "pub const XAI_STAND_IN:"),
    ] {
        let (above, _) = declared_under(code, constant);
        assert_eq!(above, SEAM_GATE, "{constant} is not under the gate");
    }
    assert_eq!(
        identifier_uses("XAI_STAND_IN"),
        [
            ("crates/config/src/provider.rs".to_string(), 2),
            ("crates/skus/src/xai/mod.rs".to_string(), 2),
        ]
    );

    // The functions that take the destinations are the sign-in's own.
    let mut taking: Vec<(String, String)> = xai
        .iter()
        .flat_map(|(file, code)| {
            functions_taking(code, "Hosts")
                .into_iter()
                .map(move |name| (file.clone(), name))
        })
        .collect();
    taking.sort();
    let own = |file: &str, name: &str| (format!("crates/skus/src/xai/{file}"), name.to_string());
    assert_eq!(
        taking,
        [
            own("account.rs", "renew"),
            own("exchange.rs", "code"),
            own("exchange.rs", "poll"),
            own("exchange.rs", "refresh"),
            own("mod.rs", "run"),
            own("mod.rs", "sign_in"),
        ],
        "a function takes the xAI sign-in's destinations that XAI-1 does not name"
    );
}

/// XAI-1 and XAI-16: the access token is put in a request in one place, to a provider built from
/// constants and held by one function, and the places that ask the renewer for it are the agent's own.
/// The provider is made in one place and read in two, the roster's listing and the build of the
/// client. The client that is given it follows no redirect, and the one function that presents the
/// token is called by the two dispatches of a request and no other. A second place that builds the account's provider, hands the
/// client another one, or presents the token is a place that can say where it goes, and this fails
/// until the clause says why.
#[test]
fn the_xai_token_is_presented_through_one_client_given_the_provider_of_constants() {
    assert_eq!(
        release_callers("Provider::xai_account"),
        [("crates/agent/src/xai.rs".to_string(), 1)],
        "the xAI account's provider is built outside the agent's one holder"
    );
    assert_eq!(
        release_callers("serving_provider()"),
        [
            ("crates/agent/src/backend.rs".to_string(), 1),
            ("crates/agent/src/xai.rs".to_string(), 2),
        ],
        "the provider the token is sent with is read outside the client and the roster that present it"
    );
    // Both clients a gateway's requests are made with are built from a provider in this one function
    // each: the configured gateway's, and the account's, which is the one of constants.
    let mut gateways: Vec<String> = release_call_arguments(".for_gateway(")
        .into_iter()
        .map(|arguments| arguments.first().cloned().unwrap_or_default())
        .collect();
    gateways.sort();
    assert_eq!(
        gateways,
        ["crate::xai::serving_provider()", "provider"],
        "a gateway client is built from a provider other than the two XAI-16 names"
    );
    let backend = release_sources()
        .into_iter()
        .find(|(file, _)| relative(file) == "crates/agent/src/backend.rs")
        .map(|(_, code)| code)
        .expect("the backend is a release source");
    let client = item_text(&backend, "fn xai_client<");
    assert!(
        client.contains(".refusing_redirects()"),
        "the account's client follows a redirect: {client}"
    );
    assert_eq!(
        release_callers("xai::presenting("),
        [("crates/agent/src/backend.rs".to_string(), 2)],
        "the access token is presented outside the two dispatches of a request"
    );
    assert_eq!(
        release_callers("xai_client("),
        [("crates/agent/src/backend.rs".to_string(), 2)],
        "the account's client is built outside the two dispatches of a request"
    );
}

/// The scan that pins the callers has to see a call in code and has to pass over one in a test
/// module or a comment, or the test above passes on a source it cannot read.
#[test]
fn the_caller_scan_reads_code_and_leaves_out_tests_and_comments() {
    let source = "\
// Hosts::at(x) is only said here
fn live() { Hosts::at(base); }
#[cfg(test)]
mod tests {
    fn in_a_test() { Hosts::at(base); Hosts::at(base); }
}
fn after() { for_account(h); }
";
    let code = production_code(source);
    assert_eq!(code.matches("Hosts::at(").count(), 1, "{code}");
    assert_eq!(code.matches("for_account(").count(), 1, "{code}");
    assert!(!code.contains("only said here"), "{code}");

    // A module declared without a body is the declaration alone, and the code after it is read.
    for declaration in [
        "#[cfg(test)]\nmod tests;",
        "#[cfg(test)]\npub(crate) mod tests;",
        "#[cfg(test)]\n#[path = \"tests_of_it.rs\"]\nmod tests;",
        "#[cfg(test)]\nmod tests {}",
        "#[cfg(all(test, unix))]\n#[path = \"tests_of_it.rs\"]\nmod tests;",
    ] {
        let code = production_code(&format!(
            "{declaration}\nfn live() {{ Hosts::at(base); }}\n"
        ));
        assert_eq!(
            code.matches("Hosts::at(").count(),
            1,
            "{declaration}: {code}"
        );
        assert!(!code.contains("mod tests"), "{declaration}: {code}");
    }
    // A gate on something that is not a module leaves the item and its attributes in.
    let code = production_code(
        "#[cfg(test)]\n#[allow(dead_code)]\nfn only_a_test() { Hosts::at(base); }\n",
    );
    assert_eq!(code.matches("Hosts::at(").count(), 1, "{code}");
}

/// The reader of a call's arguments and of the functions that take a type splits where Rust does,
/// so a nested call or a generic is not taken for two arguments or a parameter.
#[test]
fn the_call_and_signature_readers_split_where_rust_splits() {
    let code = "\
fn make() {
    ResponsesClient::for_account(egress, crate::chatgpt::account_provider(), model.into(), sign_in, Map::<A, B>::new());
}
";
    let calls = call_arguments(code, "ResponsesClient::for_account(");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].len(), 5, "{calls:?}");
    assert_eq!(calls[0][1], "crate::chatgpt::account_provider()");

    let signatures = "\
pub fn sign_in(hosts: &Hosts, transport: T) {}
fn run<S>(a: S, hosts: Hosts) -> u8 {}
fn hosts_of(a: &HostsOfSomethingElse) {}
pub(super) fn quiet(a: u8) {}
fn nested(callback: impl Fn(&Hosts)) {}
";
    assert_eq!(
        functions_taking(signatures, "Hosts"),
        ["sign_in", "run", "nested"]
    );
}

/// A file only a test module declares is a test, and the scan of what a release compiles has to
/// leave it out or a test that names the seam is taken for a caller of it.
#[test]
fn a_file_only_a_test_module_declares_is_not_a_release_source() {
    let source = "\
mod live;
#[cfg(test)]
mod plain;
#[cfg(test)]
#[path = \"elsewhere_tests.rs\"]
pub(crate) mod renamed;
    #[cfg(test)]
    mod nested;
#[cfg(test)]
mod inline {
}
#[cfg(all(test, unix))]
mod unix_only;
#[cfg(not(test))]
mod other;
";
    assert_eq!(
        declared_test_modules(source),
        [
            (None, "plain".to_string()),
            (
                Some("elsewhere_tests.rs".to_string()),
                "renamed".to_string()
            ),
            (None, "nested".to_string()),
            (None, "unix_only".to_string()),
        ]
    );
    let at = std::path::Path::new("crates/x/src");
    assert_eq!(
        module_files(&at.join("lib.rs"), None, "plain"),
        [at.join("plain.rs"), at.join("plain/mod.rs")]
    );
    assert_eq!(
        module_files(&at.join("app.rs"), None, "plain"),
        [at.join("app/plain.rs"), at.join("app/plain/mod.rs")]
    );
    assert_eq!(
        module_files(&at.join("app.rs"), Some("elsewhere_tests.rs"), "renamed"),
        [at.join("elsewhere_tests.rs")]
    );

    let files: Vec<String> = release_sources()
        .iter()
        .map(|(file, _)| relative(file))
        .collect();
    // A file a test module declares, which this test reads from the tree: any that exists is left
    // out, and one that is declared by a test module and found among the sources is a fault.
    for (file, text) in std::fs::read_dir(workspace().join("crates"))
        .expect("the crates directory")
        .flatten()
        .map(|krate| krate.path().join("src"))
        .filter(|source| source.is_dir())
        .flat_map(|source| rust_files(&source))
        .map(|file| {
            let text = std::fs::read_to_string(&file).expect("a source file");
            (file, text)
        })
    {
        for (path, name) in declared_test_modules(&text) {
            for candidate in module_files(&file, path.as_deref(), &name) {
                if candidate.is_file() {
                    assert!(
                        !files.contains(&relative(&candidate)),
                        "{} is declared by a test module and was taken for a source a release \
                         compiles",
                        relative(&candidate)
                    );
                }
            }
        }
    }
    for release in ["crates/cli/src/chatgpt.rs", "crates/agent/src/chatgpt.rs"] {
        assert!(
            files.iter().any(|file| file == release),
            "{release} was left out, so the scan reads less than a release compiles"
        );
    }
}
