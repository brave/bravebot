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
//! write or a value a plan can set, and the file named here is read on the strength of it. A
//! `[includeIf "gitdir:..."]` in those files is followed for the run's own directory, and the file
//! it names is read only where it is a file the plan cannot write.

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

/// Where a run happens, which decides the `[includeIf "gitdir:..."]` blocks of the person's own
/// configuration that apply and which files they may name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Place {
    /// The directory the run starts in. A `gitdir:` condition is matched against the `.git` inside
    /// it, so a run with no directory matches none.
    pub directory: Option<PathBuf>,
    /// Every directory the session may write. A file inside one is a file the plan can write, and
    /// is never read as an included configuration.
    pub sessions: Vec<PathBuf>,
}

/// The signing configuration of the account whose home is `home`, for a run at `place`.
///
/// The files are read in the order git reads them, so the later one wins, and within a file the
/// last assignment wins. An `[includeIf]` whose file applies is read where git reads it, in the
/// order of the file that holds it. A file that cannot be read sets nothing.
pub fn read(home: &Path, place: &Place) -> Signing {
    let mut format = None;
    let mut key = None;
    let mut set = |entry: Entry| match (entry.section.as_str(), entry.name.as_str()) {
        ("gpg", "format") => format = Some(entry.value),
        ("user", "signingkey") => key = Some(entry.value),
        _ => {}
    };
    for file in [".config/git/config", ".gitconfig"] {
        let file = under(home, file);
        for entry in entries(&configuration(&file)) {
            if (entry.section.as_str(), entry.name.as_str()) != ("includeif", "path") {
                set(entry);
                continue;
            }
            let included = file
                .parent()
                .and_then(|beside| included_file(&entry, beside, home, place));
            if let Some(included) = included {
                entries(&configuration(&included))
                    .into_iter()
                    .for_each(&mut set);
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

/// The file a `path` line under an `[includeIf]` names, where the condition holds for `place` and
/// the file is one the plan cannot write, as it will be read.
///
/// It is a regular file inside the home and outside every credential location other than
/// `~/.ssh`, and outside every directory the session may write. A relative name is beside the file
/// that holds the line, as git reads it.
fn included_file(entry: &Entry, beside: &Path, home: &Path, place: &Place) -> Option<PathBuf> {
    let condition = entry.condition.as_deref()?;
    if !condition_holds(condition, beside, home, place) {
        return None;
    }
    let named = entry.value.as_deref()?;
    let spelled = match named.strip_prefix("~/") {
        Some(inside) => home.join(inside),
        None => beside.join(named),
    };
    let resolved = std::fs::canonicalize(spelled).ok()?;
    let writable = place.sessions.iter().any(|session| {
        resolved.starts_with(session)
            || std::fs::canonicalize(session).is_ok_and(|real| resolved.starts_with(real))
    });
    (!writable
        && inside(&resolved, home)
        && !crate::scope::in_another_credential_location(&resolved, home)
        && std::fs::metadata(&resolved).is_ok_and(|held| held.is_file()))
    .then_some(resolved)
}

/// Whether an `[includeIf]` condition holds for `place`. Only `gitdir:` and `gitdir/i:` are read:
/// `onbranch:` needs the repository's state and `hasconfig:` reads its configuration, which a plan
/// can write.
fn condition_holds(condition: &str, beside: &Path, home: &Path, place: &Place) -> bool {
    let (pattern, insensitive) = match (
        condition.strip_prefix("gitdir:"),
        condition.strip_prefix("gitdir/i:"),
    ) {
        (Some(pattern), _) => (pattern, false),
        (_, Some(pattern)) => (pattern, true),
        _ => return false,
    };
    // A character class or an escape is not read, so a pattern holding one matches nothing.
    if pattern.is_empty() || pattern.len() > PATTERN_LIMIT || pattern.contains(['[', '\\']) {
        return false;
    }
    let mut pattern = if let Some(inside) = pattern.strip_prefix("~/") {
        format!("{}/{inside}", spelled(home))
    } else if let Some(inside) = pattern.strip_prefix("./") {
        format!("{}/{inside}", spelled(beside))
    } else if pattern.starts_with('/') {
        pattern.to_string()
    } else {
        format!("**/{pattern}")
    };
    if pattern.ends_with('/') {
        pattern.push_str("**");
    }
    let Some(directory) = &place.directory else {
        return false;
    };
    // git compares the git directory as it resolves it, and a link on the way can spell it twice.
    [
        directory.clone(),
        std::fs::canonicalize(directory).unwrap_or_default(),
    ]
    .iter()
    .filter(|directory| !directory.as_os_str().is_empty())
    .any(|directory| glob(&pattern, &spelled(&directory.join(".git")), insensitive))
}

/// The longest `gitdir:` pattern read.
const PATTERN_LIMIT: usize = 1024;

/// `path` as the text git matches a pattern against: `/` between its parts.
fn spelled(path: &Path) -> String {
    let text = path.to_string_lossy().into_owned();
    match cfg!(windows) {
        true => text.replace('\\', "/"),
        false => text,
    }
}

/// Whether `text` matches `pattern` as git matches a `gitdir:` pattern: `*` and `?` stay inside
/// one part of a path, and a part that is `**` is any number of parts. None is allowed, unless the
/// `**` ends the pattern, which is what is inside a directory.
fn glob(pattern: &str, text: &str, insensitive: bool) -> bool {
    let (pattern, text) = match insensitive {
        true => (pattern.to_lowercase(), text.to_lowercase()),
        false => (pattern.to_string(), text.to_string()),
    };
    let pattern: Vec<&str> = pattern.split('/').collect();
    let text: Vec<&str> = text.split('/').collect();
    // reached[i][j]: the first i parts of the pattern match the first j parts of the text.
    let mut reached = vec![vec![false; text.len() + 1]; pattern.len() + 1];
    reached[0][0] = true;
    for (at, part) in pattern.iter().enumerate() {
        for taken in 0..=text.len() {
            if !reached[at][taken] {
                continue;
            }
            if *part == "**" {
                // A `**` that ends the pattern is what is inside a directory, so it needs a part.
                let least = taken + usize::from(at + 1 == pattern.len());
                (least..=text.len()).for_each(|more| reached[at + 1][more] = true);
            } else if taken < text.len() && part_matches(part.as_bytes(), text[taken].as_bytes()) {
                reached[at + 1][taken + 1] = true;
            }
        }
    }
    reached[pattern.len()][text.len()]
}

/// Whether one part of a path matches one part of a pattern, where `*` is any run of characters
/// and `?` is one.
fn part_matches(pattern: &[u8], text: &[u8]) -> bool {
    let (mut at, mut taken) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while taken < text.len() {
        match pattern.get(at) {
            Some(b'*') => {
                star = Some((at, taken));
                at += 1;
            }
            Some(&byte) if byte == b'?' || byte == text[taken] => {
                at += 1;
                taken += 1;
            }
            _ => match star {
                Some((from, since)) => {
                    at = from + 1;
                    taken = since + 1;
                    star = Some((from, since + 1));
                }
                None => return false,
            },
        }
    }
    pattern[at..].iter().all(|byte| *byte == b'*')
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
    /// it is a different section than the one asked about. `[includeIf "..."]` is the exception,
    /// and its entries are in the section `includeif`.
    section: String,
    /// What an `[includeIf]` is conditional on, as written between the quotes.
    condition: Option<String>,
    /// The variable, lowercased.
    name: String,
    /// What it is set to, or `None` where the value holds a backslash. git reads a backslash as an
    /// escape or a line continuation, and what follows then is not the text on this line.
    value: Option<String>,
}

fn entries(contents: &str) -> Vec<Entry> {
    let mut section: Option<(String, Option<String>)> = None;
    let mut found = Vec::new();
    for line in contents.lines() {
        let mut line = line.trim();
        if let Some(header) = line.strip_prefix('[') {
            let (named, rest) = header_of(header);
            section = named;
            let Some(rest) = rest else {
                continue;
            };
            line = rest.trim();
        }
        let Some((section, condition)) = &section else {
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
            condition: condition.clone(),
            name: name.to_ascii_lowercase(),
            value: value_of(value),
        });
    }
    found
}

/// The section a header names, with the condition of an `[includeIf "..."]`, and the text after
/// its closing bracket, or `None` where the header is not one this reads. `header` is the line
/// after its opening bracket.
fn header_of(header: &str) -> (Option<(String, Option<String>)>, Option<&str>) {
    let plain = |name: &str| {
        !name.is_empty()
            && name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
    };
    // A quote before the first bracket opens a subsection, which may hold a bracket itself.
    if let Some(open) = header
        .find('"')
        .filter(|open| header.find(']').is_none_or(|end| open < &end))
    {
        let name = header[..open].trim();
        let Some(close) = header[open + 1..].find('"').map(|close| open + 1 + close) else {
            return (None, None);
        };
        let Some(rest) = header[close + 1..].trim_start().strip_prefix(']') else {
            return (None, None);
        };
        let condition = &header[open + 1..close];
        let named = (name.eq_ignore_ascii_case("includeif") && !condition.contains('\\'))
            .then(|| ("includeif".to_string(), Some(condition.to_string())));
        return (named, Some(rest));
    }
    let Some(end) = header.find(']') else {
        return (None, None);
    };
    let name = header[..end].trim();
    (
        plain(name).then(|| (name.to_ascii_lowercase(), None)),
        Some(&header[end + 1..]),
    )
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
        let signing = read_alone(&home);
        assert!(signing.enabled);
        assert_eq!(signing.file(), Some(home.join("keys/work.pub").as_path()));

        let spelled_out = format!(
            "[gpg]\nformat=ssh\n[user]\nsigningkey=\"{}/keys/work.pub\"\n",
            home.display()
        );
        std::fs::write(home.join(".gitconfig"), spelled_out).unwrap();
        assert_eq!(
            read_alone(&home).file(),
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
            let signing = read_alone(&home);
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
            let signing = read_alone(&home);
            assert!(signing.enabled);
            assert_eq!(signing.key, expected, "{value}");
            assert_eq!(signing.file(), None, "{value}");
            std::fs::remove_dir_all(&home).unwrap();
        }
        let home = a_home_signing_with("signing-unset", "[gpg]\nformat = ssh\n", &[]);
        assert_eq!(read_alone(&home).key, Key::Unset);
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
            let signing = read_alone(&home);
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
        let signing = read_alone(&home);
        assert!(signing.enabled);
        assert_eq!(signing.file(), Some(home.join("keys/last.pub").as_path()));

        std::fs::write(home.join(".gitconfig"), "[gpg]\nformat = ssh\n").unwrap();
        assert_eq!(
            read_alone(&home).file(),
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
        assert_eq!(read_alone(&home).key, Key::Refused);
        std::fs::write(home.join(".gitconfig"), b"[gpg]\nformat = ssh\n\xff\n").unwrap();
        assert!(!read_alone(&home).enabled);
        std::fs::remove_dir_all(&home).unwrap();
    }

    fn read_alone(home: &Path) -> Signing {
        read(home, &Place::default())
    }

    /// A run in `home/work/project`, which is also the directory its session may write.
    fn a_run_in_the_work_project(home: &Path) -> Place {
        let directory = home.join("work/project");
        std::fs::create_dir_all(&directory).unwrap();
        Place {
            directory: Some(directory.clone()),
            sessions: vec![directory],
        }
    }

    /// A home whose `~/.gitconfig` signs with `default.pub` and carries `block` after it, and a
    /// `~/.work-config` that signs with `work.pub`.
    fn a_home_with_a_work_key_behind(name: &str, block: &str) -> PathBuf {
        let home = a_home_signing_with(name, "", &["keys/default.pub", "keys/work.pub"]);
        std::fs::write(
            home.join(".work-config"),
            "[user]\nsigningkey = ~/keys/work.pub\n",
        )
        .unwrap();
        let block = block.replace("{home}", &home.display().to_string());
        std::fs::write(
            home.join(".gitconfig"),
            format!("[gpg]\nformat = ssh\n[user]\nsigningkey = ~/keys/default.pub\n{block}"),
        )
        .unwrap();
        home
    }

    /// The name of the key file read for a run in `home/work/project`.
    fn the_key_read(home: &Path, place: &Place) -> String {
        match read(home, place).file() {
            Some(file) => file.file_name().unwrap().to_string_lossy().into_owned(),
            None => "none".to_string(),
        }
    }

    /// SANDBOX-16: an `[includeIf "gitdir:..."]` of the person's own configuration that matches
    /// the run's directory supplies the key, as it does when git signs there. Run anywhere else, or
    /// with no directory, the key is the one the block does not override.
    #[test]
    fn an_include_if_that_matches_the_runs_directory_supplies_the_key() {
        let home = a_home_with_a_work_key_behind(
            "signing-include-if",
            "[includeIf \"gitdir:~/work/\"]\n\tpath = ~/.work-config\n",
        );
        let place = a_run_in_the_work_project(&home);
        assert_eq!(the_key_read(&home, &place), "work.pub");
        assert_eq!(the_key_read(&home, &Place::default()), "default.pub");

        let elsewhere = home.join("other/project");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let place = Place {
            directory: Some(elsewhere.clone()),
            sessions: vec![elsewhere],
        };
        assert_eq!(the_key_read(&home, &place), "default.pub");
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// git reads an `[includeIf]` where it stands, so a key set after the block wins and one set
    /// before it does not.
    #[test]
    fn an_include_if_is_read_where_it_stands_in_the_file() {
        let home = a_home_with_a_work_key_behind(
            "signing-include-order",
            "[includeIf \"gitdir:~/work/\"]\n\tpath = ~/.work-config\n\
             [user]\n\tsigningkey = ~/keys/last.pub\n",
        );
        std::fs::write(home.join("keys/last.pub"), "a key").unwrap();
        let place = a_run_in_the_work_project(&home);
        assert_eq!(the_key_read(&home, &place), "last.pub");

        std::fs::write(
            home.join(".gitconfig"),
            "[gpg]\nformat = ssh\n[includeIf \"gitdir:~/work/\"]\n\tpath = ~/.work-config\n\
             [user]\n\tsigningkey = ~/keys/last.pub\n",
        )
        .unwrap();
        assert_eq!(the_key_read(&home, &place), "last.pub");

        std::fs::write(
            home.join(".gitconfig"),
            "[gpg]\nformat = ssh\n[user]\n\tsigningkey = ~/keys/last.pub\n\
             [includeIf \"gitdir:~/work/\"]\n\tpath = ~/.work-config\n",
        )
        .unwrap();
        assert_eq!(the_key_read(&home, &place), "work.pub");
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// The conditions that match `home/work/project/.git` and those that do not. A pattern is
    /// anchored the way git anchors it, `*` stays inside one part of a path, `gitdir/i` ignores
    /// case, and a condition this does not read matches nothing.
    #[test]
    fn a_gitdir_condition_matches_as_git_matches_it() {
        let matching = [
            "gitdir:~/work/",
            "gitdir:~/work/**",
            "gitdir:~/work/project/",
            "gitdir:~/work/project/.git",
            "gitdir:~/wo*/",
            "gitdir:~/wo?k/",
            "gitdir:~/*/project/",
            "gitdir:./work/",
            "gitdir:work/",
            "gitdir:project/",
            "gitdir:{home}/work/",
            "gitdir/i:~/WORK/",
            "gitdir/i:~/Work/Project/",
        ];
        let not_matching = [
            "gitdir:~/WORK/",
            "gitdir:~/work",
            "gitdir:~/work/proj",
            "gitdir:~/other/",
            "gitdir:~/*/.git",
            "gitdir:~/w?rk?/",
            "gitdir:~/work/[p]roject/",
            "gitdir:~/work/proj\\ect/",
            "gitdir:",
            "gitdir:/work/",
            "gitdir:other/",
            "gitdir/i:~/other/",
            "hasconfig:remote.*.url:*",
            "onbranch:main",
            "GITDIR:~/work/",
        ];
        for (condition, expected) in matching
            .iter()
            .map(|condition| (condition, "work.pub"))
            .chain(
                not_matching
                    .iter()
                    .map(|condition| (condition, "default.pub")),
            )
        {
            let home = a_home_with_a_work_key_behind(
                "signing-gitdir-patterns",
                &format!("[includeIf \"{condition}\"]\n\tpath = ~/.work-config\n"),
            );
            let place = a_run_in_the_work_project(&home);
            assert_eq!(the_key_read(&home, &place), expected, "{condition}");
            std::fs::remove_dir_all(&home).unwrap();
        }
    }

    /// git reads `[x]` in a pattern as a character class and `\\` as an escape. Neither is read here,
    /// so a pattern holding one matches nothing, not even a directory spelled with the same text.
    #[test]
    fn a_gitdir_pattern_with_a_class_or_an_escape_matches_no_directory() {
        for (directory, condition) in [
            ("work/[x]/project", "gitdir:~/work/[x]/"),
            ("work/a\\b/project", "gitdir:~/work/a\\b/"),
        ] {
            let home = a_home_with_a_work_key_behind(
                "signing-gitdir-class",
                &format!("[includeIf \"{condition}\"]\n\tpath = ~/.work-config\n"),
            );
            let directory = home.join(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let place = Place {
                directory: Some(directory.clone()),
                sessions: vec![directory],
            };
            assert_eq!(the_key_read(&home, &place), "default.pub", "{condition}");
            std::fs::remove_dir_all(&home).unwrap();
        }
    }

    /// A relative `path` is beside the file that holds the line, whichever of the two global files
    /// it is.
    #[test]
    fn a_relative_include_path_is_beside_the_file_that_holds_it() {
        let home = a_home_with_a_work_key_behind("signing-include-relative", "");
        std::fs::create_dir_all(home.join(".config/git")).unwrap();
        std::fs::write(
            home.join(".config/git/work"),
            "[user]\nsigningkey = ~/keys/work.pub\n",
        )
        .unwrap();
        std::fs::write(
            home.join(".config/git/config"),
            "[includeIf \"gitdir:~/work/\"]\n\tpath = work\n",
        )
        .unwrap();
        let place = a_run_in_the_work_project(&home);
        // ~/.gitconfig is read after it and sets the default key, as git does.
        assert_eq!(the_key_read(&home, &place), "default.pub");

        std::fs::write(
            home.join(".gitconfig"),
            "[gpg]\nformat = ssh\n[includeIf \"gitdir:~/work/\"]\n\tpath = .work-config\n",
        )
        .unwrap();
        assert_eq!(the_key_read(&home, &place), "work.pub");
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// The included file is read on the strength of a line the person wrote, but its content must
    /// not be something the plan wrote. A file inside a directory the session may write is refused,
    /// whether it is named directly or through a link from outside it.
    #[test]
    fn an_include_if_whose_file_is_inside_the_workspace_is_refused() {
        let home = a_home_with_a_work_key_behind(
            "signing-include-workspace",
            "[includeIf \"gitdir:~/work/\"]\n\tpath = ~/work/project/.work-config\n",
        );
        let place = a_run_in_the_work_project(&home);
        std::fs::rename(
            home.join(".work-config"),
            home.join("work/project/.work-config"),
        )
        .unwrap();
        assert_eq!(the_key_read(&home, &place), "default.pub");

        std::fs::write(
            home.join(".gitconfig"),
            "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/keys/default.pub\n\
             [includeIf \"gitdir:~/work/\"]\n\tpath = ~/.linked-config\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(
            home.join("work/project/.work-config"),
            home.join(".linked-config"),
        )
        .unwrap();
        assert_eq!(the_key_read(&home, &place), "default.pub");

        let scratch = home.join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::rename(
            home.join("work/project/.work-config"),
            scratch.join("config"),
        )
        .unwrap();
        std::fs::write(
            home.join(".gitconfig"),
            "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/keys/default.pub\n\
             [includeIf \"gitdir:~/work/\"]\n\tpath = ~/scratch/config\n",
        )
        .unwrap();
        assert_eq!(the_key_read(&home, &place), "work.pub");
        let place = Place {
            sessions: vec![place.directory.clone().unwrap(), scratch],
            ..place
        };
        assert_eq!(the_key_read(&home, &place), "default.pub");
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// The file is also refused where it is outside the home, inside another credential location,
    /// not a regular file or not there. `~/.ssh` is the one credential location a configuration
    /// of the person's may sit in, since the key is judged there already.
    #[test]
    fn an_include_if_whose_file_is_not_a_plain_file_in_the_home_is_refused() {
        let outside = scratch_dir("signing-include-outside");
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(
            outside.join("config"),
            "[user]\nsigningkey = ~/keys/work.pub\n",
        )
        .unwrap();
        let outside = std::fs::canonicalize(outside).unwrap();

        let home = a_home_with_a_work_key_behind("signing-include-refused", "");
        let place = a_run_in_the_work_project(&home);
        for directory in [".aws", ".ssh", "directory"] {
            std::fs::create_dir_all(home.join(directory)).unwrap();
        }
        for directory in [".aws", ".ssh"] {
            std::fs::copy(
                home.join(".work-config"),
                home.join(directory).join("config"),
            )
            .unwrap();
        }
        std::os::unix::fs::symlink(outside.join("config"), home.join(".out-config")).unwrap();
        std::os::unix::fs::symlink(home.join(".aws/config"), home.join(".aws-config")).unwrap();

        let cases = [
            (outside.join("config").display().to_string(), "default.pub"),
            ("~/.out-config".to_string(), "default.pub"),
            ("~/.aws/config".to_string(), "default.pub"),
            ("~/.aws-config".to_string(), "default.pub"),
            ("~/directory".to_string(), "default.pub"),
            ("~/missing".to_string(), "default.pub"),
            ("~/.ssh/config".to_string(), "work.pub"),
            ("~/.work-config".to_string(), "work.pub"),
        ];
        for (path, expected) in cases {
            std::fs::write(
                home.join(".gitconfig"),
                format!(
                    "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/keys/default.pub\n\
                     [includeIf \"gitdir:~/work/\"]\n\tpath = {path}\n"
                ),
            )
            .unwrap();
            assert_eq!(the_key_read(&home, &place), expected, "{path}");
        }
        std::fs::remove_dir_all(&home).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    /// An included file is not searched for blocks of its own, and a plain `[include]` is not
    /// followed, so no file but the ones the person's two global files name through a condition
    /// that holds is read.
    #[test]
    fn an_include_inside_an_included_file_and_a_plain_include_are_not_followed() {
        let home = a_home_with_a_work_key_behind(
            "signing-include-nested",
            "[includeIf \"gitdir:~/work/\"]\n\tpath = ~/.nested\n\
             [include]\n\tpath = ~/.work-config\n",
        );
        std::fs::write(
            home.join(".nested"),
            "[includeIf \"gitdir:~/work/\"]\n\tpath = ~/.work-config\n",
        )
        .unwrap();
        let place = a_run_in_the_work_project(&home);
        assert_eq!(the_key_read(&home, &place), "default.pub");
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// SANDBOX-16: the repository's own configuration is never read, because the plan can write it.
    /// A key there that the person's global key overrides is not the one read, and with no global
    /// key it is not read at all.
    #[test]
    fn a_repository_key_is_not_read_when_a_global_key_overrides_it() {
        let home = a_home_with_a_work_key_behind("signing-repository-key", "");
        let place = a_run_in_the_work_project(&home);
        let repository = place.directory.clone().unwrap();
        std::fs::create_dir_all(repository.join(".git")).unwrap();
        std::fs::write(home.join("keys/repo.pub"), "a key").unwrap();
        std::fs::write(
            repository.join(".git/config"),
            "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/keys/repo.pub\n",
        )
        .unwrap();
        assert_eq!(the_key_read(&home, &place), "default.pub");

        std::fs::write(home.join(".gitconfig"), "[gpg]\nformat = ssh\n").unwrap();
        let signing = read(&home, &place);
        assert_eq!(signing.key, Key::Unset);
        assert_eq!(signing.file(), None);
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// `**` is any number of parts and none, `*` and `?` stay inside a part.
    #[test]
    fn a_glob_matches_by_path_part() {
        for (pattern, text, expected) in [
            ("/a/**", "/a/b/c", true),
            ("/a/**", "/a", false),
            ("/a/**/c", "/a/c", true),
            ("/a/**/c", "/a/b/x/c", true),
            ("/a/*", "/a/b/c", false),
            ("/a/*/c", "/a/b/c", true),
            ("/a/*b*/c", "/a/xbx/c", true),
            ("/a/b?", "/a/b", false),
            ("/a/b?", "/a/bc", true),
            ("/a/b?", "/a/bcd", false),
            ("/a/b*", "/a/b", true),
            ("**/c", "/a/c", true),
            ("/a/B", "/a/b", false),
        ] {
            assert_eq!(glob(pattern, text, false), expected, "{pattern} {text}");
        }
        assert!(glob("/a/B", "/a/b", true));
    }
}
