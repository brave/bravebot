//! The key a person's own git configuration signs with, as far as a stage that signs may read it.
//!
//! `git commit`, `git rebase` and the other operations that write a signed commit run
//! `ssh-keygen -Y sign` with the file `user.signingkey` names. ssh-keygen reads that public key and
//! asks the agent to sign with the private half, so the stage needs the one `.pub` file and the
//! agent's socket, and never the private key. `docs/specs/sandboxing.md` (SANDBOX-16) decides what
//! is read; this reads the two configuration files that can set it.
//!
//! Only the person's own files are read, `~/.gitconfig` and `~/.config/git/config`, as the
//! `IdentityFile` lines of `~/.ssh/config` are. A repository's configuration, an `[include]` and an
//! environment variable that moves the files are not followed, because each is a place a plan can
//! write or a value a plan can set, and the file named here is read on the strength of it.

use crate::base::under;
use std::path::{Path, PathBuf};

/// The most configuration bytes read from one file.
const CONFIGURATION_LIMIT: u64 = 1 << 20;

/// What `user.signingkey` names, as far as a stage may read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// No key is configured, so git signs with the first key the agent lists.
    Unset,
    /// The key is written into the configuration (`ssh-ed25519 AAAA...` or `key::...`), so there
    /// is no file to read.
    Literal,
    /// The public key file to read, as the sandbox will be told it.
    File(PathBuf),
    /// A value that names no file a stage may read: a private key, a file that is not named
    /// `*.pub`, one that does not exist, one outside the home directory or inside another
    /// credential location, a relative path, or text this cannot read as a path.
    Refused,
}

/// How the person's own git configuration signs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signing {
    /// Whether `gpg.format` is `ssh`. Any other format signs with a program that needs a credential
    /// location no scope reaches, and an unset one does not sign with a key this reads.
    pub enabled: bool,
    pub key: Key,
}

impl Signing {
    /// The one file a stage that signs reads, where signing is on and the key is a file.
    pub fn file(&self) -> Option<&Path> {
        match (&self.key, self.enabled) {
            (Key::File(file), true) => Some(file),
            _ => None,
        }
    }
}

/// The signing configuration of the account whose home is `home`.
///
/// The files are read in the order git reads them, so the later one wins, and within a file the
/// last assignment wins. A file that cannot be read sets nothing.
pub fn read(home: &Path) -> Signing {
    let mut format = None;
    let mut key = None;
    for file in [".config/git/config", ".gitconfig"] {
        for entry in entries(&configuration(&under(home, file))) {
            match (entry.section.as_str(), entry.name.as_str()) {
                ("gpg", "format") => format = Some(entry.value),
                ("user", "signingkey") => key = Some(entry.value),
                _ => {}
            }
        }
    }
    Signing {
        enabled: format.flatten().is_some_and(|format| format == "ssh"),
        key: match key {
            None => Key::Unset,
            Some(None) => Key::Refused,
            Some(Some(value)) if value.is_empty() => Key::Unset,
            Some(Some(value)) => judged(&value, home),
        },
    }
}

fn configuration(path: &Path) -> String {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut bytes = Vec::new();
    if file
        .take(CONFIGURATION_LIMIT)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return String::new();
    }
    // A lossy decode could spell a key name that is not in the file.
    String::from_utf8(bytes).unwrap_or_default()
}

/// One assignment of a configuration file.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    /// The section, lowercased. A section with a subsection (`[gpg "ssh"]`) has no entries here:
    /// it is a different section than the one asked about.
    section: String,
    /// The variable, lowercased.
    name: String,
    /// What it is set to, or `None` where the value holds a backslash. git reads a backslash as an
    /// escape or a line continuation, and what follows then is not the text on this line.
    value: Option<String>,
}

fn entries(contents: &str) -> Vec<Entry> {
    let mut section: Option<String> = None;
    let mut found = Vec::new();
    for line in contents.lines() {
        let mut line = line.trim();
        if let Some(header) = line.strip_prefix('[') {
            let Some(end) = header.find(']') else {
                section = None;
                continue;
            };
            let name = header[..end].trim();
            let plain = !name.is_empty()
                && name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-');
            section = plain.then(|| name.to_ascii_lowercase());
            line = header[end + 1..].trim();
        }
        let Some(section) = &section else {
            continue;
        };
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            continue;
        }
        found.push(Entry {
            section: section.clone(),
            name: name.to_ascii_lowercase(),
            value: value_of(value),
        });
    }
    found
}

/// The text of a value: quotes removed, an unquoted `#` or `;` starting a comment.
fn value_of(raw: &str) -> Option<String> {
    if raw.contains('\\') {
        return None;
    }
    let mut value = String::new();
    let mut quoted = false;
    for character in raw.chars() {
        match character {
            '"' => quoted = !quoted,
            '#' | ';' if !quoted => break,
            _ => value.push(character),
        }
    }
    Some(value.trim().to_string())
}

fn judged(value: &str, home: &Path) -> Key {
    if value.starts_with("key::") || value.starts_with("ssh-") {
        return Key::Literal;
    }
    match crate::scope::judged_public_key(Path::new(value), home).filter(|file| inside(file, home))
    {
        Some(file) => Key::File(file),
        None => Key::Refused,
    }
}

/// Whether `file`, already resolved, is beneath the home directory.
fn inside(file: &Path, home: &Path) -> bool {
    let real_home = std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    file.starts_with(&real_home) && file != real_home
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::testutil::scratch_dir;

    /// A home with `.gitconfig` holding `gitconfig` and each of `files` present.
    fn a_home_signing_with(name: &str, gitconfig: &str, files: &[&str]) -> PathBuf {
        let home = scratch_dir(name);
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        for file in files {
            let path = home.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "a key").unwrap();
        }
        std::fs::write(home.join(".gitconfig"), gitconfig).unwrap();
        std::fs::canonicalize(home).unwrap()
    }

    /// SANDBOX-16: the key a signed commit is made with is read where the person's own
    /// configuration names a public key in their home, however the path is spelled.
    #[test]
    fn a_public_key_in_the_home_is_the_file_a_stage_that_signs_reads() {
        let home = a_home_signing_with(
            "signing-key-named",
            "[gpg]\n\tformat = ssh\n[user]\n\tsigningkey = ~/keys/work.pub\n",
            &["keys/work.pub"],
        );
        let signing = read(&home);
        assert!(signing.enabled);
        assert_eq!(signing.file(), Some(home.join("keys/work.pub").as_path()));

        let spelled_out = format!(
            "[gpg]\nformat=ssh\n[user]\nsigningkey=\"{}/keys/work.pub\"\n",
            home.display()
        );
        std::fs::write(home.join(".gitconfig"), spelled_out).unwrap();
        assert_eq!(
            read(&home).file(),
            Some(home.join("keys/work.pub").as_path())
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// Only `gpg.format = ssh` signs with the agent. Any other format names a program this does not
    /// read a key for, so no file is read whatever `user.signingkey` says.
    #[test]
    fn a_format_other_than_ssh_reads_no_key() {
        for gitconfig in [
            "[user]\n\tsigningkey = ~/keys/work.pub\n",
            "[gpg]\n\tformat = openpgp\n[user]\n\tsigningkey = ~/keys/work.pub\n",
            "[gpg \"ssh\"]\n\tformat = ssh\n[user]\n\tsigningkey = ~/keys/work.pub\n",
            "[gpg]\n\tformat = SSH\n[user]\n\tsigningkey = ~/keys/work.pub\n",
        ] {
            let home = a_home_signing_with("signing-format", gitconfig, &["keys/work.pub"]);
            let signing = read(&home);
            assert!(!signing.enabled, "{gitconfig}");
            assert_eq!(signing.file(), None, "{gitconfig}");
            std::fs::remove_dir_all(&home).unwrap();
        }
    }

    /// A key written into the configuration needs no file, and none set means the agent's first key.
    #[test]
    fn a_literal_key_and_no_key_read_no_file() {
        for (value, expected) in [
            ("ssh-ed25519 AAAAC3Nza", Key::Literal),
            ("key::ssh-ed25519 AAAAC3Nza", Key::Literal),
            ("", Key::Unset),
        ] {
            let home = a_home_signing_with(
                "signing-literal",
                &format!("[gpg]\nformat = ssh\n[user]\nsigningkey = {value}\n"),
                &[],
            );
            let signing = read(&home);
            assert!(signing.enabled);
            assert_eq!(signing.key, expected, "{value}");
            assert_eq!(signing.file(), None, "{value}");
            std::fs::remove_dir_all(&home).unwrap();
        }
        let home = a_home_signing_with("signing-unset", "[gpg]\nformat = ssh\n", &[]);
        assert_eq!(read(&home).key, Key::Unset);
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// The file is read on the strength of one line of configuration, so it must be a public key
    /// in the home. A private key, a name nothing is at, a relative path, `..`, a file outside the
    /// home and a link that leads out of it or into another credential location are refused.
    #[test]
    fn a_value_that_is_not_a_public_key_in_the_home_is_refused() {
        let outside = scratch_dir("signing-outside");
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("other.pub"), "a key").unwrap();
        let outside = std::fs::canonicalize(outside).unwrap();

        let home = a_home_signing_with(
            "signing-refused",
            "",
            &[".ssh/id_work", "keys/work.pub", ".aws/key.pub"],
        );
        std::os::unix::fs::symlink(outside.join("other.pub"), home.join("keys/out.pub")).unwrap();
        std::os::unix::fs::symlink(home.join(".aws/key.pub"), home.join("keys/aws.pub")).unwrap();
        std::os::unix::fs::symlink(home.join(".ssh/id_work"), home.join("keys/private.pub"))
            .unwrap();

        let refused = [
            "~/.ssh/id_work".to_string(),
            "~/keys/missing.pub".to_string(),
            "keys/work.pub".to_string(),
            "~/keys/../keys/work.pub".to_string(),
            outside.join("other.pub").display().to_string(),
            "~/keys/out.pub".to_string(),
            "~/.aws/key.pub".to_string(),
            "~/keys/aws.pub".to_string(),
            "~/keys/private.pub".to_string(),
            "~".to_string(),
        ];
        for value in refused {
            std::fs::write(
                home.join(".gitconfig"),
                format!("[gpg]\nformat = ssh\n[user]\nsigningkey = {value}\n"),
            )
            .unwrap();
            let signing = read(&home);
            assert_eq!(signing.key, Key::Refused, "{value}");
            assert_eq!(signing.file(), None, "{value}");
        }
        std::fs::remove_dir_all(&home).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    /// git takes the last value, and its own two global files in the order `~/.config/git/config`
    /// then `~/.gitconfig`. A section, a subsection or a variable of another name is not the one.
    #[test]
    fn the_last_value_of_the_right_section_is_the_one_read() {
        let home = a_home_signing_with(
            "signing-order",
            "[User]\n  SigningKey = ~/keys/first.pub # a comment\n\
             signingkey = ~/keys/last.pub\n[GPG]\nFormat = ssh ; a comment\n\
             [core]\nsigningkey = ~/keys/core.pub\n[user \"work\"]\nsigningkey = ~/keys/sub.pub\n\
             [gpg \"ssh\"]\nformat = openpgp\n",
            &[
                "keys/core.pub",
                "keys/sub.pub",
                "keys/first.pub",
                "keys/last.pub",
                "keys/xdg.pub",
            ],
        );
        std::fs::create_dir_all(home.join(".config/git")).unwrap();
        std::fs::write(
            home.join(".config/git/config"),
            "[user]\nsigningkey = ~/keys/xdg.pub\n",
        )
        .unwrap();
        let signing = read(&home);
        assert!(signing.enabled);
        assert_eq!(signing.file(), Some(home.join("keys/last.pub").as_path()));

        std::fs::write(home.join(".gitconfig"), "[gpg]\nformat = ssh\n").unwrap();
        assert_eq!(
            read(&home).file(),
            Some(home.join("keys/xdg.pub").as_path())
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// A backslash in a value is an escape or a continuation, so the text on the line is not the
    /// path git reads and the key is refused rather than guessed. A file that is not UTF-8 sets
    /// nothing.
    #[test]
    fn a_value_with_a_backslash_is_refused() {
        let home = a_home_signing_with(
            "signing-backslash",
            "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/keys/work\\\n.pub\n",
            &["keys/work.pub"],
        );
        assert_eq!(read(&home).key, Key::Refused);
        std::fs::write(home.join(".gitconfig"), b"[gpg]\nformat = ssh\n\xff\n").unwrap();
        assert!(!read(&home).enabled);
        std::fs::remove_dir_all(&home).unwrap();
    }
}
