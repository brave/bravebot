//! The hooks a person said yes to, `hooks-approved` in the user's own state directory.
//!
//! A hook that comes from a plugin fires only after the person has answered a question naming it
//! (PLUGIN-7). A yes is one line here: a digest of the plugin's name, the hook's moment, the hook's
//! argv with `${PLUGIN_ROOT}` already replaced (PLUGIN-13) and the plugin's tree digest, then the
//! plugin's name so the lines of a removed plugin can be found again.
//!
//! ```text
//! 4f1c9a2e...  demo-plugin
//! ```
//!
//! The file is the person's own, like `mcp-approved`, and a plugin cannot write it. A line that
//! does not read is passed over and approves nothing. A no is never written.

use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The file, inside the state directory.
const APPROVALS_FILE: &str = "hooks-approved";

/// The most of it worth reading. A line is about a hundred bytes, so this is thousands of hooks.
const MAX_BYTES: u64 = 256 * 1024;

/// What is hashed first, so a digest made here is not one made for another purpose.
const DOMAIN: &[u8] = b"bravebot hooks-approved v1";

/// Where the approvals are, for telling the person so.
pub fn approvals_file(home: &Path) -> PathBuf {
    home.join(APPROVALS_FILE)
}

/// The SHA-256 of one hook of one plugin, as the question named it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Digest([u8; 32]);

impl Digest {
    /// The digest of `plugin`'s hook on `moment` that starts `argv`, for the plugin tree whose
    /// digest is `tree`.
    ///
    /// `argv` is the list [`crate::plugin::read_hooks`] returns, so `${PLUGIN_ROOT}` is replaced
    /// before it is hashed. Every field is hashed with its length, so no two lists of fields that
    /// differ in where a word ends hash alike.
    pub fn of(plugin: &str, moment: &str, argv: &[String], tree: &str) -> Self {
        let mut hasher = Sha256::new();
        let mut field = |bytes: &[u8]| {
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        };
        field(DOMAIN);
        field(plugin.as_bytes());
        field(moment.as_bytes());
        field(&(argv.len() as u64).to_be_bytes());
        for word in argv {
            field(word.as_bytes());
        }
        field(tree.as_bytes());
        Self(hasher.finalize().into())
    }

    /// A digest from its sixty-four lowercase hexadecimal digits, or `None` for anything else.
    pub fn parse(hex: &str) -> Option<Self> {
        let bytes = hex.as_bytes();
        if bytes.len() != 64 || !bytes.iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return None;
        }
        let mut out = [0_u8; 32];
        for (place, pair) in out.iter_mut().zip(bytes.chunks(2)) {
            let high = char::from(pair[0]).to_digit(16)?;
            let low = char::from(pair[1]).to_digit(16)?;
            *place = u8::try_from(high * 16 + low).ok()?;
        }
        Some(Self(out))
    }
}

impl std::fmt::Display for Digest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// The lines of the file: each a digest and the name of the plugin it was answered for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Approvals {
    lines: BTreeSet<(Digest, String)>,
}

impl Approvals {
    /// Read the approvals in a state directory. No file, a file too large and a file that cannot
    /// be read approve nothing.
    pub fn read(home: &Path) -> Self {
        let path = approvals_file(home);
        match std::fs::metadata(&path) {
            Ok(found) if found.len() <= MAX_BYTES => {}
            _ => return Self::default(),
        }
        std::fs::read_to_string(&path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Read approvals out of the file's text. A line that is not a digest and a plugin name is
    /// passed over.
    pub fn parse(text: &str) -> Self {
        let mut approvals = Self::default();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            let (Some(hex), Some(plugin), None) = (words.next(), words.next(), words.next()) else {
                continue;
            };
            if let Some(digest) = Digest::parse(hex)
                && crate::plugin::is_valid_name(plugin)
            {
                approvals.lines.insert((digest, plugin.to_string()));
            }
        }
        approvals
    }

    /// Whether the person said yes to this digest.
    pub fn approves(&self, digest: &Digest) -> bool {
        self.lines.iter().any(|(held, _)| held == digest)
    }

    /// The file's text, one line for each approval.
    pub fn to_text(&self) -> String {
        self.lines
            .iter()
            .map(|(digest, plugin)| format!("{digest} {plugin}\n"))
            .collect()
    }
}

/// Add the line for a yes to the file in `home`, creating the directory and the file where they are
/// not there. A digest already in the file is not written again.
pub fn record(home: &Path, plugin: &str, digest: &Digest) -> std::io::Result<()> {
    if Approvals::read(home).approves(digest) {
        return Ok(());
    }
    std::fs::create_dir_all(home)?;
    let path = approvals_file(home);
    let mut line = String::new();
    // A file the person edited by hand may end without a newline, and a line must not join it.
    if std::fs::read_to_string(&path).is_ok_and(|text| !text.is_empty() && !text.ends_with('\n')) {
        line.push('\n');
    }
    line.push_str(&format!("{digest} {plugin}\n"));
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())
}

/// Remove every line recorded for the plugin called `plugin` from the file in `home`, so a later
/// install of that name is asked about again (PLUGIN-9). The other lines are kept exactly as they
/// are, including a line that does not read and a missing final newline. The file is written beside
/// itself and renamed over, so a reader sees the old file or the new one. A file with no line for
/// the plugin, or no file, is left untouched.
pub fn withdraw(home: &Path, plugin: &str) -> std::io::Result<()> {
    let path = approvals_file(home);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let kept: String = text
        .split_inclusive('\n')
        .filter(|line| !is_line_for(line, plugin))
        .collect();
    if kept == text {
        return Ok(());
    }
    let temporary = home.join(format!(".{APPROVALS_FILE}.tmp"));
    std::fs::write(&temporary, kept)
        .and_then(|()| std::fs::rename(&temporary, &path))
        .inspect_err(|_| {
            std::fs::remove_file(&temporary).ok();
        })
}

/// Whether `line` is one [`Approvals::parse`] reads, for the plugin called `plugin`.
fn is_line_for(line: &str, plugin: &str) -> bool {
    let mut words = line.split_whitespace();
    matches!(
        (words.next(), words.next(), words.next()),
        (Some(hex), Some(name), None) if name == plugin && Digest::parse(hex).is_some()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(ToString::to_string).collect()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(format!("config-hooks-approved-{name}"));
        std::fs::remove_dir_all(&dir).ok();
        dir
    }

    /// PLUGIN-7: the digest is SHA-256 over length-prefixed fields: the domain string, the plugin's
    /// name, the moment, the number of argv words, each word, and the tree digest, each field as its
    /// byte length in eight big-endian bytes and then its bytes. The bytes are built here from that
    /// layout, and one value is fixed as a literal computed outside this crate, so a change of the
    /// layout, of the domain string or of a field left out fails here and not only by comparison
    /// with itself.
    ///
    /// Rejects: a digest that leaves out the plugin, the moment, an argv word or the tree, one that
    /// drops the length prefixes, and one that changes the domain string.
    #[test]
    fn the_digest_is_a_sha256_over_the_documented_length_prefixed_fields() {
        fn by_hand(plugin: &str, moment: &str, argv: &[&str], tree: &str) -> String {
            let mut bytes: Vec<u8> = Vec::new();
            let mut field = |content: &[u8]| {
                bytes.extend_from_slice(&(content.len() as u64).to_be_bytes());
                bytes.extend_from_slice(content);
            };
            field(b"bravebot hooks-approved v1");
            field(plugin.as_bytes());
            field(moment.as_bytes());
            field(&(argv.len() as u64).to_be_bytes());
            for word in argv {
                field(word.as_bytes());
            }
            field(tree.as_bytes());
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        }
        let words = ["/p/hook.sh", "--flag"];
        let digest = Digest::of("demo", "turn-started", &argv(&words), "aa");

        assert_eq!(
            digest.to_string(),
            by_hand("demo", "turn-started", &words, "aa")
        );
        assert_eq!(
            digest.to_string(),
            "a9c2a51b0cf189102f1a55885871b6d66f8fc9298b876a1a684784ad7234c743",
            "the value a separate implementation of the layout gives"
        );

        let changed = [
            (
                "plugin",
                Digest::of("other", "turn-started", &argv(&words), "aa"),
            ),
            (
                "moment",
                Digest::of("demo", "turn-finished", &argv(&words), "aa"),
            ),
            (
                "first word",
                Digest::of(
                    "demo",
                    "turn-started",
                    &argv(&["/p/other.sh", "--flag"]),
                    "aa",
                ),
            ),
            (
                "second word",
                Digest::of(
                    "demo",
                    "turn-started",
                    &argv(&["/p/hook.sh", "--other"]),
                    "aa",
                ),
            ),
            (
                "an added word",
                Digest::of(
                    "demo",
                    "turn-started",
                    &argv(&["/p/hook.sh", "--flag", "x"]),
                    "aa",
                ),
            ),
            (
                "tree",
                Digest::of("demo", "turn-started", &argv(&words), "bb"),
            ),
        ];
        for (field, other) in changed {
            assert_ne!(digest, other, "{field} does not change the digest");
        }
    }

    /// PLUGIN-7: the digest is over the plugin's name, the moment, the argv and the tree. A digest
    /// that left one out would keep an approval after that part changed. Each field is changed
    /// alone, and a word boundary moved without changing the bytes.
    #[test]
    fn the_digest_follows_the_plugin_the_moment_the_argv_and_the_tree() {
        let base = Digest::of("demo", "turn-started", &argv(&["/p/hook.sh"]), "aa");
        assert_eq!(
            base,
            Digest::of("demo", "turn-started", &argv(&["/p/hook.sh"]), "aa")
        );
        let others = [
            Digest::of("other", "turn-started", &argv(&["/p/hook.sh"]), "aa"),
            Digest::of("demo", "turn-finished", &argv(&["/p/hook.sh"]), "aa"),
            Digest::of("demo", "turn-started", &argv(&["/p/hook.sh", "-x"]), "aa"),
            Digest::of("demo", "turn-started", &argv(&["/p/hook.sh"]), "bb"),
        ];
        for other in others {
            assert_ne!(base, other);
        }
        assert_ne!(
            Digest::of("demo", "turn-started", &argv(&["a b"]), "aa"),
            Digest::of("demo", "turn-started", &argv(&["a", "b"]), "aa")
        );
        assert_ne!(
            Digest::of("demo", "turn-started", &argv(&["a", ""]), "aa"),
            Digest::of("demo", "turn-started", &argv(&["a"]), "aa")
        );
    }

    /// A line is a digest and a plugin name. The reader passes over anything else and the writer
    /// reads back what it wrote.
    #[test]
    fn a_line_that_is_not_a_digest_and_a_name_approves_nothing() {
        let digest = Digest::of("demo", "turn-started", &argv(&["/p/hook.sh"]), "aa");
        let text = format!(
            "{digest} demo\n\nnot-hex demo\n{digest}\n{digest} demo extra\n{digest} ../x\n{}\n",
            "A".repeat(64)
        );
        let read = Approvals::parse(&text);
        assert!(read.approves(&digest));
        assert_eq!(
            read.lines.iter().filter(|(_, name)| name == "demo").count(),
            1
        );
        assert_eq!(Approvals::parse(&read.to_text()), read);
        assert!(!Approvals::parse(&format!("{digest}\n")).approves(&digest));
        assert!(!Approvals::parse(&format!("{digest} ../x\n")).approves(&digest));
    }

    /// PLUGIN-7: a yes is one line. Recording twice writes one, a hand-edited file without a final
    /// newline keeps its own line, and no file approves nothing.
    #[test]
    fn a_yes_is_one_line_and_is_written_once() {
        let home = scratch("record");
        let digest = Digest::of("demo", "turn-started", &argv(&["/p/hook.sh"]), "aa");
        assert!(!Approvals::read(&home).approves(&digest));
        record(&home, "demo", &digest).expect("record");
        record(&home, "demo", &digest).expect("record again");
        let text = std::fs::read_to_string(approvals_file(&home)).expect("file");
        assert_eq!(text, format!("{digest} demo\n"));
        assert!(Approvals::read(&home).approves(&digest));

        let other = Digest::of("demo", "turn-finished", &argv(&["/p/hook.sh"]), "aa");
        std::fs::write(approvals_file(&home), format!("{digest} demo")).expect("edited");
        record(&home, "demo", &other).expect("record after edit");
        let read = Approvals::read(&home);
        assert!(read.approves(&digest) && read.approves(&other));
        std::fs::remove_dir_all(&home).ok();
    }

    /// PLUGIN-9, S32: withdrawing a plugin removes its lines and no other line, byte for byte,
    /// including a line that does not read and a final line with no newline. A name that is only a
    /// prefix of another plugin's name removes nothing of that plugin's.
    ///
    /// Rejects: a rewrite from the parsed set (which reorders and drops unread lines), a match on a
    /// name prefix, a withdrawal that leaves the plugin's own line, and one that creates a file.
    #[test]
    fn withdrawing_a_plugin_removes_its_lines_and_leaves_the_others_as_they_are() {
        let home = scratch("withdraw");
        let mine = Digest::of("demo", "turn-started", &argv(&["/p/a"]), "aa");
        let mine_too = Digest::of("demo", "turn-finished", &argv(&["/p/a"]), "aa");
        let longer = Digest::of("demo-two", "turn-started", &argv(&["/p/a"]), "aa");
        let other = Digest::of("other", "turn-started", &argv(&["/p/a"]), "aa");
        withdraw(&home, "demo").expect("no file");
        assert!(!approvals_file(&home).exists());

        std::fs::create_dir_all(&home).expect("home");
        let before = format!(
            "{other} other\nnot a line  \n{mine} demo\n\n{longer} demo-two\n{mine_too} demo\n{other} other"
        );
        std::fs::write(approvals_file(&home), &before).expect("file");
        withdraw(&home, "demo").expect("withdraw");
        let after = std::fs::read_to_string(approvals_file(&home)).expect("file");
        assert_eq!(
            after,
            format!("{other} other\nnot a line  \n\n{longer} demo-two\n{other} other")
        );
        let read = Approvals::read(&home);
        assert!(!read.approves(&mine) && !read.approves(&mine_too));
        assert!(read.approves(&longer) && read.approves(&other));
        assert!(!home.join(format!(".{APPROVALS_FILE}.tmp")).exists());

        withdraw(&home, "demo").expect("again");
        assert_eq!(
            std::fs::read_to_string(approvals_file(&home)).expect("file"),
            after
        );
        std::fs::remove_dir_all(&home).ok();
    }
}
