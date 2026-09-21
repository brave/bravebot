//! Reading the working tree for credentials, before anybody has vouched for it.
//!
//! The directory a session is pointed at may already hold somebody's keys. Nobody put them
//! through a gate and nothing recorded what they reach, and the moment the tree is vouched for
//! its contents are readable by a turn and disclosed to whoever performs inference. So the walk
//! runs first, and what it found is on the screen while the question about trusting the directory
//! is still unanswered.
//!
//! # What this can and cannot say
//!
//! It finds what [`bravebot_core::credentials`] recognises, in the part of the tree it reached
//! inside its budget. Silence means nothing matched, which is not the same as nothing being
//! there, and a report that says how far it got is the only honest form for a partial answer to
//! take: a walk that stopped at three per cent and one that stopped at ninety-seven are different
//! answers.
//!
//! # Why this decides nothing
//!
//! A finding is shown and that is all. Nothing here refuses a read, moves a value anywhere, or
//! answers the trust question on anybody's behalf, which is what
//! [CRED-17](../../../docs/specs/credential-protection.md) requires of a scan: a program running
//! as the person can write a credential in a form no layer recognises, so a scan that gated
//! anything would be a gate that can be lied to. What it defends against is the person not
//! knowing what is in their own repository.
//!
//! That is also why reading the tree here is not a decision taken from untrusted content. The
//! walk runs before a session exists, the bytes it reads reach no model and no turn, and the one
//! thing a match produces is a line on a terminal. Nothing branches on the result.

use bravebot_core::credentials::{self, Finding};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long the walk may run before it reports what it reached.
///
/// It blocks the trust question, so it is bounded rather than thorough. Two seconds is long
/// enough for an ordinary checkout and short enough that nobody waits on a large one, and a tree
/// too large to finish inside it is a fact the report carries rather than a reason to wait.
pub const BUDGET: Duration = Duration::from_secs(2);

/// How much of one file is read.
///
/// A dump, a bundle or a database file can be gigabytes, and reading one whole would spend the
/// whole budget on a single path. The head is where configuration and armoured keys are written,
/// so it is the part worth having when only a part can be had.
const HEAD_OF_A_FILE: u64 = 256 * 1024;

/// How much of the head is examined for the byte that says a file is not text.
const SNIFFED: usize = 8 * 1024;

/// Directories nobody in this tree wrote, which is why they are not read.
///
/// Build output is deliberately absent: `dist` and `target` hold what this repository produced,
/// and a key baked into a bundle at build time is in the tree and about to ship. So is anything
/// vendored or submoduled, because vouching for the tree covers those too. `.git` is here because
/// the objects under it are a scan of their own, expensive and off by default, rather than
/// because the history does not matter.
const NOT_READ: &[&str] = &[".git", "node_modules", "venv", ".venv"];

/// What one pass over a tree found, and how much of the tree it got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeScan {
    findings: Vec<Finding>,
    read: usize,
    everything_was_read: bool,
}

impl Default for TreeScan {
    /// A scan that has found nothing and has read everything there was, which is what a walk
    /// over an empty directory ends as. The walk only ever lowers the last of those.
    fn default() -> Self {
        Self {
            findings: Vec::new(),
            read: 0,
            everything_was_read: true,
        }
    }
}

impl TreeScan {
    /// What was found, the ones a provider's own format declared first.
    ///
    /// Ranked rather than in walk order, because only the first few are read: a report that led
    /// with an inferred match from a name would push a declared key off the screen, and the
    /// declared one is the finding nobody has to argue about.
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// How many files were read.
    ///
    /// The denominator of the coverage question, and the only one available: how many files the
    /// tree holds is not known without walking it, which is the thing that ran out of time.
    pub fn read(&self) -> usize {
        self.read
    }

    /// Whether the whole tree was read.
    ///
    /// False where the budget ran out with the walk unfinished, and false where a file or a
    /// directory could not be read at all. The two are one answer because they are one fact to
    /// whoever reads the report: part of this directory was not looked at, so its silence covers
    /// less than the tree.
    pub fn everything_was_read(&self) -> bool {
        self.everything_was_read
    }
}

/// Read `root`, inside [`BUDGET`].
pub fn scan_tree(root: &Path) -> TreeScan {
    scan_tree_within(root, BUDGET)
}

/// Read `root`, inside `budget`.
///
/// Separated from [`scan_tree`] so a test can state the deadline it is testing rather than
/// waiting out the real one.
///
/// Files in a directory are read before any of its subdirectories are descended into, and both
/// are sorted, so a walk that stopped early kept a part of the tree that is the same part on the
/// next machine. The filesystem's own order would make a partial answer a different partial
/// answer each time.
pub fn scan_tree_within(root: &Path, budget: Duration) -> TreeScan {
    let deadline = Instant::now() + budget;
    let salt = credentials::run_salt();
    let mut scan = TreeScan::default();
    // An explicit stack rather than recursion: a tree's depth is whatever somebody's checkout
    // happens to be, and a walk that overflows the stack takes the session with it.
    let mut left = vec![root.to_path_buf()];

    while let Some(directory) = left.pop() {
        if Instant::now() >= deadline {
            return finish(scan, false);
        }
        let Some((files, directories)) = entries_of(&directory) else {
            // A directory that cannot be listed is part of the tree nothing looked at, and
            // reporting silence over it would be the clean result this scan must never invent.
            scan.everything_was_read = false;
            continue;
        };
        for file in files {
            if Instant::now() >= deadline {
                return finish(scan, false);
            }
            match text_of(&file) {
                Reading::Text(text) => {
                    scan.read += 1;
                    scan.findings
                        .extend(credentials::scan(&named(root, &file), &text, salt));
                }
                Reading::NotText => continue,
                Reading::Refused => scan.everything_was_read = false,
            }
        }
        // Reversed, because the stack hands back what went on last and the sorted order is the
        // one the walk is supposed to take.
        left.extend(directories.into_iter().rev());
    }

    finish(scan, true)
}

/// Rank what was found and record whether the walk got to the end of the tree.
///
/// `finished` is false only where the budget ran out. A file or a directory that could not be
/// read has already been recorded during the walk, so it is kept rather than overwritten.
fn finish(mut scan: TreeScan, finished: bool) -> TreeScan {
    // Stable, so paths stay in the order the walk read them within each half.
    scan.findings
        .sort_by_key(|finding| !finding.kind.is_declared());
    scan.everything_was_read &= finished;
    scan
}

/// The files and the subdirectories of one directory, each sorted, or nothing if it cannot be
/// read.
///
/// A symlink is neither. Following one would take the walk outside the tree being vouched for,
/// and a link is also the one way a walk that follows nothing else can be made to loop.
fn entries_of(directory: &Path) -> Option<(Vec<PathBuf>, Vec<PathBuf>)> {
    let reading = std::fs::read_dir(directory).ok()?;
    let mut files = Vec::new();
    let mut directories = Vec::new();

    for entry in reading.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if !NOT_READ.contains(&entry.file_name().to_string_lossy().as_ref()) {
                directories.push(entry.path());
            }
        } else if kind.is_file() {
            files.push(entry.path());
        }
    }

    files.sort();
    directories.sort();
    Some((files, directories))
}

/// What came of trying to read one file.
///
/// Three answers rather than two, because a file the scan could not open and a file with nothing
/// in it worth reading are the same silence to whoever reads the report, and only one of them is
/// a gap in what was covered.
enum Reading {
    /// The head of the file, as text.
    Text(String),
    /// Not text, so no layer could say anything about it either way.
    NotText,
    /// The scan was not allowed to read it, or it went away while the walk was running.
    Refused,
}

/// The head of a file as text, or why there is none.
///
/// A file holding a zero byte early on is a binary: an object file, an image, a database. Reading
/// one as text finds nothing a person would act on and costs the budget that the rest of the tree
/// needed, and the lossy conversion below would turn its bytes into something a layer could
/// match on by accident. The zero byte is looked for in a first small read, so a binary costs
/// that much rather than the whole cap.
fn text_of(path: &Path) -> Reading {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Reading::Refused;
    };
    let mut head = Vec::new();
    if (&mut file)
        .take(SNIFFED as u64)
        .read_to_end(&mut head)
        .is_err()
    {
        return Reading::Refused;
    }
    if head.contains(&0) {
        return Reading::NotText;
    }
    if file
        .take(HEAD_OF_A_FILE - SNIFFED as u64)
        .read_to_end(&mut head)
        .is_err()
    {
        return Reading::Refused;
    }
    // Lossy, because the head of a large file is cut wherever the cap fell, which is as likely to
    // be inside a character as between two.
    Reading::Text(String::from_utf8_lossy(&head).into_owned())
}

/// A path as the person reading the report sees it, relative to the tree it was found in.
fn named(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::credentials::Kind;

    /// A tree built for one test, removed first so a previous run leaves nothing behind.
    fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = crate::testutil::scratch_dir(&format!("bravebot-credential-scan-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("make the root");
        for (path, contents) in files {
            let at = root.join(path);
            std::fs::create_dir_all(at.parent().expect("a parent")).expect("make the directory");
            std::fs::write(&at, contents).expect("write the file");
        }
        root
    }

    /// An AWS key id, which is a shape rather than a guess.
    const A_DECLARED_KEY: &str = "AKIAIOSFODNN7EXAMPLE";

    /// The case the clause exists for: a key is sitting in an untracked file, and the person is
    /// about to be asked whether to vouch for the directory holding it. A walk that read only
    /// what git tracks would find nothing here, and `.gitignore` hides a file from git rather
    /// than from a turn.
    #[test]
    fn a_credential_in_an_ignored_file_is_found_before_anybody_is_asked() {
        let root = tree(
            "ignored",
            &[
                (".gitignore", ".env\n"),
                (".env", &format!("AWS_ACCESS_KEY_ID={A_DECLARED_KEY}\n")),
            ],
        );

        let scan = scan_tree(&root);

        let found: Vec<&Finding> = scan.findings().iter().collect();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, Kind::AwsAccessKey);
        assert_eq!(found[0].path, ".env");
        assert_eq!(found[0].line, 1);
    }

    /// The report is read from the top and only the first few lines are shown, so the order is
    /// what decides which findings a person actually sees. A value a provider's own format
    /// declared is the one nobody has to argue about, and an inference from a name that sounds
    /// like a secret must not push one off the screen.
    #[test]
    fn a_declared_key_is_reported_ahead_of_an_inferred_one() {
        let root = tree(
            "ranked",
            &[
                (
                    "a-inferred.yml",
                    // The fixture is a name that sounds like a secret beside a value that looks
                    // rare, which is the whole of what the inferred layer matches on. The scan
                    // that reads it is the thing under test, so it cannot be written any other
                    // way, and the value is thirty-two characters of nothing.
                    // nosemgrep: generic.secrets.security.detected-generic-api-key.detected-generic-api-key
                    "API_KEY: 8f14e45fceea167a5a36dedd4bea2543\n",
                ),
                ("z-declared.env", &format!("KEY={A_DECLARED_KEY}\n")),
            ],
        );

        let kinds: Vec<Kind> = scan_tree(&root)
            .findings()
            .iter()
            .map(|finding| finding.kind)
            .collect();

        assert_eq!(
            kinds,
            vec![Kind::AwsAccessKey, Kind::Assigned],
            "the inferred finding was reported first, so it is what a short report would show"
        );
    }

    /// Nothing the scan reports may repeat the value, whatever surface it came from. A report
    /// that quoted what it found would be a second copy of every credential in the tree, written
    /// by the thing that was checking for copies.
    #[test]
    fn nothing_the_report_says_repeats_the_value_that_was_found() {
        let root = tree(
            "quiet",
            &[("config.env", &format!("KEY={A_DECLARED_KEY}\n"))],
        );

        let scan = scan_tree(&root);
        let said = scan.findings()[0].describe();

        assert!(!said.contains(A_DECLARED_KEY), "{said}");
        assert!(
            !said.contains(&A_DECLARED_KEY[..8]),
            "a prefix of the value is most of what identifies it: {said}"
        );
        assert!(
            !said.contains(&A_DECLARED_KEY[A_DECLARED_KEY.len() - 8..]),
            "a suffix of the value is the same disclosure as a prefix: {said}"
        );
    }

    /// A walk that stopped has to say so. A partial result reported as a whole one is a clean
    /// bill of health over a tree nothing looked at, which is worse than no scan at all: the
    /// person reads silence as an answer.
    #[test]
    fn a_walk_that_runs_out_of_time_says_it_did_not_reach_the_end() {
        let root = tree(
            "budget",
            &[("one.txt", "nothing"), ("deep/two.txt", "nothing")],
        );

        let stopped = scan_tree_within(&root, Duration::ZERO);
        assert!(
            !stopped.everything_was_read(),
            "a walk with no time at all claimed to have covered the tree"
        );
        assert_eq!(stopped.read(), 0);

        let whole = scan_tree(&root);
        assert!(whole.everything_was_read());
        assert_eq!(whole.read(), 2);
    }

    /// A file or a directory the scan cannot read is part of the tree nothing looked at.
    /// Reported as a finished walk it would be a clean result over files nobody read, which is
    /// the one answer this scan must never invent: the person is about to vouch for them.
    ///
    /// Both halves in one test, because the fixture is a mode and the one environment that
    /// cannot express it, a session running as root, cannot express either of them.
    #[test]
    #[cfg(unix)]
    fn what_the_walk_could_not_read_is_not_reported_as_covered() {
        use std::os::unix::fs::PermissionsExt;

        for (name, shut) in [
            ("locked-dir", "shut/inside.env"),
            ("locked-file", "shut.env"),
        ] {
            let root = tree(
                name,
                &[("readme.md", "nothing here\n"), (shut, "nothing\n")],
            );
            // The directory in one case and the file itself in the other, which are the two
            // ways a path in the tree comes back unreadable.
            let at = match shut.contains('/') {
                true => root.join("shut"),
                false => root.join(shut),
            };
            std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o000)).expect("shut it");

            // A mode is not enforced against root, so the fixture cannot express the fault
            // there. Probed rather than assumed, because the container this suite is also run
            // in is root, and a test that failed there would be failing on the environment.
            let unenforced = match at.is_dir() {
                true => std::fs::read_dir(&at).is_ok(),
                false => std::fs::File::open(&at).is_ok(),
            };
            let scan = scan_tree(&root);

            // Restored before the assertion, so a failure does not leave something nothing can
            // remove behind for the next run.
            std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o755)).expect("open it");
            if unenforced {
                continue;
            }
            assert!(
                !scan.everything_was_read(),
                "{name}: what the walk was locked out of was reported as covered"
            );
        }
    }

    /// The directories nobody in the tree wrote are the bulk of a large checkout, and reading
    /// them would spend the budget the person's own files needed. Build output is the other
    /// half of the same rule and is read, because a key baked into a bundle is in the tree and
    /// about to ship.
    #[test]
    fn a_dependency_directory_is_not_read_and_build_output_is() {
        let root = tree(
            "skipping",
            &[
                ("node_modules/pkg/.env", &format!("KEY={A_DECLARED_KEY}\n")),
                (".git/config", &format!("KEY={A_DECLARED_KEY}\n")),
                (
                    "dist/bundle.js",
                    &format!("const k = \"{A_DECLARED_KEY}\";\n"),
                ),
            ],
        );

        let scan = scan_tree(&root);

        let paths: Vec<&str> = scan
            .findings()
            .iter()
            .map(|finding| finding.path.as_str())
            .collect();
        assert_eq!(paths, vec!["dist/bundle.js"], "{paths:?}");
    }

    /// A link is a name in this tree for a file outside it. Following one would read a home
    /// directory the person never pointed the session at, and report its contents as something
    /// found in the repository.
    #[test]
    #[cfg(unix)]
    fn a_link_out_of_the_tree_is_not_followed() {
        let outside = tree(
            "outside",
            &[("secrets.env", &format!("KEY={A_DECLARED_KEY}\n"))],
        );
        let root = tree("linking", &[("readme.md", "nothing here\n")]);
        std::os::unix::fs::symlink(outside.join("secrets.env"), root.join("linked.env"))
            .expect("make the link");

        let scan = scan_tree(&root);

        assert!(
            scan.findings().is_empty(),
            "the link was followed out of the tree: {:?}",
            scan.findings()
        );
    }

    /// An object file or an image is most of a build tree by count. Read as text it finds
    /// nothing anybody would act on, and the bytes of one converted to text are exactly the
    /// shape a layer matches on by accident.
    #[test]
    fn a_binary_file_is_not_read_as_text() {
        let root = tree("binary", &[("readme.md", "nothing here\n")]);
        let mut bytes = b"API_KEY: ".to_vec();
        bytes.push(0);
        bytes.extend_from_slice(b"8f14e45fceea167a5a36dedd4bea2543\n");
        std::fs::write(root.join("object.bin"), bytes).expect("write the binary");

        let scan = scan_tree(&root);

        assert!(scan.findings().is_empty(), "{:?}", scan.findings());
        assert_eq!(scan.read(), 1, "only the text file was read");
    }
}
