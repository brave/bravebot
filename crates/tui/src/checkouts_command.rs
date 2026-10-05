//! The argument to `/checkouts`, and whether a checkout's commit was pushed.
//!
//! A checkout is made, kept and removed by [`bravebot_agent::workspace`]. What is here is the half
//! that is a terminal thing: the words a person types to see the checkouts the session keeps and to
//! remove one, and whether a remote branch is at the commit a checkout is at. That is read here and
//! not in the driver, since a program in the checkout writes those files: what they say is shown to
//! the person and decides nothing.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// The word that removes a checkout, rather than naming one.
const REMOVE: &str = "remove";
/// The word that brings a checkout's files back into the working directory.
const APPLY: &str = "apply";

/// What the argument to `/checkouts` asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The bare word: list the checkouts the session keeps, or say there are none.
    List,
    /// Remove the checkout with this number, spelled the way the list spells it.
    Remove(String),
    /// Bring back the files written in the checkout with this number, spelled the same way.
    Apply(String),
    /// Anything else, answered by saying what the command takes.
    Unreadable,
}

/// Read the argument to `/checkouts`.
///
/// A number is taken as the list prints it, `c2`, or bare, `2`. Anything else is unreadable rather
/// than a guess, since removing the wrong checkout deletes work and applying the wrong one writes
/// into the working directory.
pub fn parse(argument: &str) -> Asked {
    let argument = argument.trim();
    if argument.is_empty() {
        return Asked::List;
    }
    let Some((word, named)) = argument.split_once(char::is_whitespace) else {
        return Asked::Unreadable;
    };
    let named = named.trim();
    let digits = named.strip_prefix('c').unwrap_or(named);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Asked::Unreadable;
    }
    let Ok(number) = digits.parse::<u64>() else {
        return Asked::Unreadable;
    };
    match word {
        REMOVE => Asked::Remove(format!("c{number}")),
        APPLY => Asked::Apply(format!("c{number}")),
        _ => Asked::Unreadable,
    }
}

/// Where a checkout's HEAD is against the repository's remote branches (CHECKOUT-15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pushed {
    /// A remote branch, named as `origin/fix`, is at the commit HEAD is at.
    At {
        /// The branch HEAD names, or `None` where HEAD names a commit.
        branch: Option<String>,
        remote: String,
    },
    /// No remote branch is at that commit. One at a later commit is not looked for.
    Nowhere { branch: Option<String> },
    /// HEAD, the branch it names, `packed-refs` or the remote branches could not be read.
    Unread,
}

/// The longest ref file read. A ref is one line.
const LONGEST_REF: u64 = 4096;
const LONGEST_PACKED_REFS: u64 = 64 << 20;
/// A checkout shares the repository's refs, so a program in it can fill `refs/remotes`. The
/// walk stops here so that cannot stall the listing.
const MOST_REMOTE_ENTRIES: usize = 100_000;

/// The refs of one `.git` directory, read once for every checkout made from it.
///
/// Nothing is read through a link, and only a plain file is opened, so a link or a pipe a program
/// in a checkout put in place of one reads as unread rather than leading elsewhere or leaving the
/// listing waiting.
pub struct Branches {
    repository: PathBuf,
    /// `None` where `packed-refs` or `refs/remotes` could not be read in full.
    refs: Option<Refs>,
}

struct Refs {
    /// Each ref `packed-refs` lists, by its full name.
    packed: BTreeMap<String, String>,
    /// Each remote branch, as `origin/fix`, and the commit it is at.
    remotes: BTreeMap<String, String>,
}

impl Branches {
    pub fn read(repository: &Path) -> Branches {
        let packed = match read_under(repository, &["packed-refs"], LONGEST_PACKED_REFS) {
            Found::Absent => Some(BTreeMap::new()),
            Found::Bytes(bytes) => Some(packed_refs(&bytes)),
            Found::Unreadable => None,
        };
        let refs = packed.and_then(|packed| {
            let remotes = remote_tips(repository, &packed)?;
            Some(Refs { packed, remotes })
        });
        Branches {
            repository: repository.to_path_buf(),
            refs,
        }
    }

    /// Where the HEAD of checkout `id` is against the remote branches. HEAD is the one in the
    /// `worktrees/<id>/` entry the driver wrote.
    pub fn pushed(&self, id: &str) -> Pushed {
        let Some(refs) = &self.refs else {
            return Pushed::Unread;
        };
        let Found::Bytes(head) =
            read_under(&self.repository, &["worktrees", id, "HEAD"], LONGEST_REF)
        else {
            return Pushed::Unread;
        };
        let Ok(head) = String::from_utf8(head) else {
            return Pushed::Unread;
        };
        let head = head.trim_end();
        let (branch, commit) = match head.strip_prefix("ref: ") {
            Some(name) => {
                let (Some(branch), Some(commit)) =
                    (name.strip_prefix("refs/heads/"), self.tip(refs, name))
                else {
                    return Pushed::Unread;
                };
                (Some(branch.to_owned()), commit)
            }
            None if is_id(head) => (None, head.to_owned()),
            None => return Pushed::Unread,
        };
        let at: Vec<&str> = refs
            .remotes
            .iter()
            .filter(|(_, tip)| **tip == commit)
            .map(|(name, _)| name.as_str())
            .collect();
        let same_name = |name: &&str| {
            branch.is_some() && name.split_once('/').map(|(_, rest)| rest) == branch.as_deref()
        };
        match at.iter().copied().find(same_name).or(at.first().copied()) {
            Some(remote) => Pushed::At {
                branch,
                remote: remote.to_owned(),
            },
            None => Pushed::Nowhere { branch },
        }
    }

    /// The commit the ref `name` is at: its loose file, or `packed-refs` where there is none. A
    /// name git would not take is not looked for, so none leads outside `refs` or onto a second
    /// line of the listing.
    fn tip(&self, refs: &Refs, name: &str) -> Option<String> {
        if !bravebot_agent::git::plausible_ref(name) {
            return None;
        }
        let parts: Vec<&str> = name.split('/').collect();
        match read_under(&self.repository, &parts, LONGEST_REF) {
            Found::Absent => refs.packed.get(name).cloned(),
            Found::Bytes(bytes) => commit_in(&bytes),
            Found::Unreadable => None,
        }
    }
}

/// What reading one file beneath the repository found.
enum Found {
    Absent,
    Bytes(Vec<u8>),
    /// Something other than a plain file, one reached through a link, one too long, or one that
    /// could not be read.
    Unreadable,
}

/// What stands at a path, not following a link.
enum Stands {
    Directory,
    Absent,
    Other,
}

fn stands(path: &Path) -> Stands {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => Stands::Directory,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Stands::Absent,
        _ => Stands::Other,
    }
}

/// The file `parts` name beneath `repository`, each part but the last a directory and not a link.
fn read_under(repository: &Path, parts: &[&str], longest: u64) -> Found {
    let mut path = repository.to_path_buf();
    for (at, part) in parts.iter().enumerate() {
        if at > 0 {
            match stands(&path) {
                Stands::Directory => {}
                Stands::Absent => return Found::Absent,
                Stands::Other => return Found::Unreadable,
            }
        }
        path.push(part);
    }
    read_plain(&path, longest)
}

/// The bytes of a plain file no longer than `longest`.
fn read_plain(path: &Path, longest: u64) -> Found {
    let file = match open_plain(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Found::Absent,
        Err(_) => return Found::Unreadable,
    };
    // What was opened, not what the path named a moment before.
    if !file.metadata().is_ok_and(|meta| meta.is_file()) {
        return Found::Unreadable;
    }
    let mut bytes = Vec::new();
    match file.take(longest + 1).read_to_end(&mut bytes) {
        Ok(read) if read as u64 <= longest => Found::Bytes(bytes),
        _ => Found::Unreadable,
    }
}

/// Open `path` without following a link, and without waiting for a writer where it is a pipe.
#[cfg(unix)]
fn open_plain(path: &Path) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags};
    let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK;
    Ok(std::fs::File::from(rustix::fs::open(
        path,
        flags,
        Mode::empty(),
    )?))
}

#[cfg(not(unix))]
fn open_plain(path: &Path) -> std::io::Result<std::fs::File> {
    if !std::fs::symlink_metadata(path)?.is_file() {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    std::fs::File::open(path)
}

/// Each remote branch, as `origin/fix`, and the commit it is at, or `None` where `refs/remotes`
/// could not be read in full. A loose ref takes the place of a packed one of the same name, as git
/// reads them, and a symbolic one such as `origin/HEAD` is left out.
fn remote_tips(
    repository: &Path,
    packed: &BTreeMap<String, String>,
) -> Option<BTreeMap<String, String>> {
    let mut tips: BTreeMap<String, String> = packed
        .iter()
        .filter(|(name, _)| bravebot_agent::git::plausible_ref(name))
        .filter_map(|(name, id)| Some((name.strip_prefix("refs/remotes/")?.to_owned(), id.clone())))
        .collect();
    let refs = repository.join("refs");
    let remotes = refs.join("remotes");
    match (stands(&refs), stands(&remotes)) {
        (Stands::Absent, _) | (Stands::Directory, Stands::Absent) => return Some(tips),
        (Stands::Directory, Stands::Directory) => {}
        _ => return None,
    }
    let mut looked_at = 0;
    let mut pending = vec![(remotes, String::new())];
    while let Some((directory, prefix)) = pending.pop() {
        for entry in std::fs::read_dir(&directory).ok()? {
            looked_at += 1;
            if looked_at > MOST_REMOTE_ENTRIES {
                return None;
            }
            let entry = entry.ok()?;
            let kind = entry.file_type().ok()?;
            let name = format!("{prefix}{}", entry.file_name().into_string().ok()?);
            if kind.is_dir() {
                pending.push((entry.path(), format!("{name}/")));
                continue;
            }
            // Such as the `.lock` file git writes beside a ref it is changing.
            if !bravebot_agent::git::plausible_ref(&format!("refs/remotes/{name}")) {
                continue;
            }
            tips.remove(&name);
            match read_plain(&entry.path(), LONGEST_REF) {
                Found::Bytes(bytes) => {
                    if let Some(id) = commit_in(&bytes) {
                        tips.insert(name, id);
                    }
                }
                Found::Absent => {}
                Found::Unreadable => return None,
            }
        }
    }
    Some(tips)
}

/// The commit a loose ref names, where it names one rather than another ref.
fn commit_in(bytes: &[u8]) -> Option<String> {
    let id = std::str::from_utf8(bytes).ok()?.trim_end();
    is_id(id).then(|| id.to_owned())
}

/// Each ref `packed-refs` lists, by name. The header and the peeled lines name no ref, and a line
/// that is not UTF-8 is passed over rather than the file.
fn packed_refs(bytes: &[u8]) -> BTreeMap<String, String> {
    bytes
        .split(|byte| *byte == b'\n')
        .filter_map(|line| std::str::from_utf8(line).ok())
        .filter_map(|line| line.split_once(' '))
        .filter(|(id, _)| is_id(id))
        .map(|(id, name)| (name.to_owned(), id.to_owned()))
        .collect()
}

/// A commit id as git writes one in a ref: forty hexadecimal digits, or sixty-four.
fn is_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bare_word_lists_the_checkouts() {
        assert_eq!(parse(""), Asked::List);
        assert_eq!(parse("   "), Asked::List);
    }

    #[test]
    fn apply_and_a_number_brings_back_that_checkouts_files() {
        assert_eq!(parse("apply c3"), Asked::Apply("c3".to_string()));
        assert_eq!(parse("apply 12"), Asked::Apply("c12".to_string()));
        assert_eq!(parse("apply"), Asked::Unreadable);
        assert_eq!(parse("apply all"), Asked::Unreadable);
    }

    #[test]
    fn remove_and_a_number_removes_that_checkout() {
        assert_eq!(parse("remove c3"), Asked::Remove("c3".to_string()));
        assert_eq!(parse("remove   12  "), Asked::Remove("c12".to_string()));
        assert_eq!(parse("remove c007"), Asked::Remove("c7".to_string()));
    }

    #[test]
    fn anything_else_is_answered_by_saying_what_the_command_takes() {
        for argument in [
            "remove",
            "remove all",
            "remove c",
            "remove c-1",
            "remove +1",
            "remove c1 c2",
            "c1",
            "list",
            "delete c1",
            "remove 99999999999999999999999",
        ] {
            assert_eq!(parse(argument), Asked::Unreadable, "{argument}");
        }
    }

    const AT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const ELSEWHERE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    /// A `.git` directory with nothing in it but what a test puts there.
    fn repository(name: &str) -> std::path::PathBuf {
        let root = crate::testutil::scratch_dir(&format!("pushed-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let git = root.join(".git");
        std::fs::create_dir_all(&git).expect("scratch");
        git
    }

    fn put(git: &Path, name: &str, text: &str) {
        let path = git.join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("parent");
        std::fs::write(path, text).expect("written");
    }

    fn pushed(git: &Path, id: &str) -> Pushed {
        Branches::read(git).pushed(id)
    }

    fn at(branch: Option<&str>, remote: &str) -> Pushed {
        Pushed::At {
            branch: branch.map(str::to_owned),
            remote: remote.to_owned(),
        }
    }

    /// CHECKOUT-15. A checkout on a branch a remote branch is at reads as pushed, whether the refs
    /// are loose files or in `packed-refs`, and one at a commit no remote branch is at does not.
    #[test]
    fn a_checkout_reads_as_pushed_where_a_remote_branch_is_at_its_commit() {
        let git = repository("loose");
        put(&git, "worktrees/c1/HEAD", "ref: refs/heads/fix\n");
        put(&git, "refs/heads/fix", &format!("{AT}\n"));
        put(&git, "refs/remotes/origin/fix", &format!("{AT}\n"));
        assert_eq!(pushed(&git, "c1"), at(Some("fix"), "origin/fix"));

        put(&git, "refs/remotes/origin/fix", &format!("{ELSEWHERE}\n"));
        assert_eq!(
            pushed(&git, "c1"),
            Pushed::Nowhere {
                branch: Some("fix".into())
            }
        );

        let git = repository("packed");
        put(&git, "worktrees/c1/HEAD", "ref: refs/heads/fix\n");
        put(
            &git,
            "packed-refs",
            &format!(
                "# pack-refs with: peeled fully-peeled sorted \n{AT} refs/heads/fix\n{AT} refs/remotes/origin/fix\n^{ELSEWHERE}\n"
            ),
        );
        assert_eq!(pushed(&git, "c1"), at(Some("fix"), "origin/fix"));

        // A loose ref is newer than the packed one of the same name, as git reads them.
        put(&git, "refs/remotes/origin/fix", &format!("{ELSEWHERE}\n"));
        assert_eq!(
            pushed(&git, "c1"),
            Pushed::Nowhere {
                branch: Some("fix".into())
            }
        );
        put(&git, "refs/heads/fix", &format!("{ELSEWHERE}\n"));
        assert_eq!(pushed(&git, "c1"), at(Some("fix"), "origin/fix"));
        // One that names no commit hides the packed one too, rather than leaving it to be read.
        put(&git, "refs/heads/fix", &format!("{AT}\n"));
        put(&git, "refs/remotes/origin/fix", "not a commit\n");
        assert_eq!(
            pushed(&git, "c1"),
            Pushed::Nowhere {
                branch: Some("fix".into())
            }
        );
    }

    /// CHECKOUT-15. Of the remote branches at the commit, the one named as the checkout's branch
    /// is the one named, and a checkout on no branch is matched by its commit alone.
    #[test]
    fn the_remote_branch_named_is_the_one_of_the_same_name() {
        let git = repository("named");
        put(&git, "worktrees/c1/HEAD", "ref: refs/heads/fix\n");
        put(&git, "refs/heads/fix", &format!("{AT}\n"));
        put(&git, "refs/remotes/aaa/other", &format!("{AT}\n"));
        put(&git, "refs/remotes/origin/fix", &format!("{AT}\n"));
        put(
            &git,
            "refs/remotes/origin/HEAD",
            "ref: refs/remotes/origin/fix\n",
        );
        assert_eq!(pushed(&git, "c1"), at(Some("fix"), "origin/fix"));

        put(&git, "worktrees/c1/HEAD", &format!("{AT}\n"));
        assert_eq!(pushed(&git, "c1"), at(None, "aaa/other"));
        put(&git, "worktrees/c1/HEAD", &format!("{ELSEWHERE}\n"));
        assert_eq!(pushed(&git, "c1"), Pushed::Nowhere { branch: None });
    }

    /// CHECKOUT-15. A HEAD that is missing, is not a commit or a branch, names a branch outside
    /// `refs/heads` or one that does not exist reads as unread rather than as not pushed.
    #[test]
    fn a_head_that_cannot_be_followed_reads_as_unread() {
        let git = repository("unread");
        put(&git, "refs/remotes/origin/fix", &format!("{AT}\n"));
        // So that `refs/heads/..` would lead somewhere if it were followed.
        put(&git, "refs/heads/main", &format!("{AT}\n"));
        assert_eq!(pushed(&git, "c1"), Pushed::Unread);
        for head in [
            "not a commit\n",
            "ref: refs/heads/gone\n",
            "ref: refs/remotes/origin/fix\n",
            "ref: refs/heads/../remotes/origin/fix\n",
            "ref: refs/heads/a\\b\n",
        ] {
            put(&git, "worktrees/c1/HEAD", head);
            assert_eq!(pushed(&git, "c1"), Pushed::Unread, "{head}");
        }
        put(&git, "worktrees/c1/HEAD", &format!("{AT}\n"));
        assert_eq!(pushed(&git, "c1"), at(None, "origin/fix"));
        std::fs::create_dir_all(git.join("packed-refs")).expect("a directory");
        assert_eq!(pushed(&git, "c1"), Pushed::Unread);
    }

    /// CHECKOUT-15. A branch or a remote branch whose name git would not take is not read, so a
    /// name with a line break or a terminal escape in it cannot add a line to the listing or
    /// redraw the screen.
    #[cfg(unix)]
    #[test]
    fn a_name_git_would_not_take_is_not_read() {
        let git = repository("names");
        put(&git, "refs/heads/fix\nc1: pushed", &format!("{AT}\n"));
        put(
            &git,
            "refs/remotes/origin/fix\nc1: pushed",
            &format!("{AT}\n"),
        );
        put(&git, "refs/remotes/origin/fix.lock", &format!("{AT}\n"));
        put(
            &git,
            "worktrees/c1/HEAD",
            "ref: refs/heads/fix\nc1: pushed\n",
        );
        assert_eq!(pushed(&git, "c1"), Pushed::Unread);
        put(&git, "worktrees/c1/HEAD", &format!("{AT}\n"));
        assert_eq!(pushed(&git, "c1"), Pushed::Nowhere { branch: None });
        put(
            &git,
            "packed-refs",
            &format!("{AT} refs/remotes/origin/fix\x1b[2J\n"),
        );
        assert_eq!(pushed(&git, "c1"), Pushed::Nowhere { branch: None });
    }

    /// CHECKOUT-15. A line of `packed-refs` that is not UTF-8 is passed over, and the rest of the
    /// file is still read.
    #[test]
    fn a_packed_ref_that_is_not_utf8_leaves_the_others_read() {
        let git = repository("packed-bytes");
        put(&git, "worktrees/c1/HEAD", "ref: refs/heads/fix\n");
        let mut packed = format!("{AT} refs/remotes/origin/caf").into_bytes();
        packed.extend_from_slice(b"\xe9\n");
        packed.extend_from_slice(
            format!("{AT} refs/heads/fix\n{AT} refs/remotes/origin/fix\n").as_bytes(),
        );
        std::fs::write(git.join("packed-refs"), packed).expect("written");
        assert_eq!(pushed(&git, "c1"), at(Some("fix"), "origin/fix"));
    }

    /// CHECKOUT-15. A link in place of HEAD, a ref, `refs/remotes` or any directory on the way to
    /// one is not followed, since a program in the checkout could point one anywhere.
    #[cfg(unix)]
    #[test]
    fn a_link_is_not_followed() {
        let git = repository("link");
        put(&git, "elsewhere/HEAD", &format!("{AT}\n"));
        put(&git, "elsewhere/fix", &format!("{AT}\n"));
        put(&git, "elsewhere/remotes/origin/fix", &format!("{AT}\n"));
        std::fs::create_dir_all(git.join("worktrees/c1")).expect("entry");
        std::os::unix::fs::symlink(git.join("elsewhere/HEAD"), git.join("worktrees/c1/HEAD"))
            .expect("linked");
        assert_eq!(pushed(&git, "c1"), Pushed::Unread);

        std::fs::remove_file(git.join("worktrees/c1/HEAD")).expect("unlinked");
        put(&git, "worktrees/c1/HEAD", "ref: refs/heads/fix\n");
        std::fs::create_dir_all(git.join("refs/heads")).expect("heads");
        std::os::unix::fs::symlink(git.join("elsewhere/fix"), git.join("refs/heads/fix"))
            .expect("linked");
        assert_eq!(pushed(&git, "c1"), Pushed::Unread);

        std::fs::remove_file(git.join("refs/heads/fix")).expect("unlinked");
        put(&git, "refs/heads/fix", &format!("{AT}\n"));
        std::os::unix::fs::symlink(git.join("elsewhere/remotes"), git.join("refs/remotes"))
            .expect("linked");
        assert_eq!(pushed(&git, "c1"), Pushed::Unread);
        std::fs::remove_file(git.join("refs/remotes")).expect("unlinked");
        assert_eq!(
            pushed(&git, "c1"),
            Pushed::Nowhere {
                branch: Some("fix".into())
            }
        );

        put(&git, "elsewhere/heads/fix", &format!("{AT}\n"));
        put(
            &git,
            "packed-refs",
            &format!("{AT} refs/heads/fix\n{AT} refs/remotes/origin/fix\n"),
        );
        std::fs::remove_dir_all(git.join("refs/heads")).expect("removed");
        std::os::unix::fs::symlink(git.join("elsewhere/heads"), git.join("refs/heads"))
            .expect("linked");
        assert_eq!(pushed(&git, "c1"), Pushed::Unread);
        std::fs::remove_file(git.join("refs/heads")).expect("unlinked");
        put(&git, "refs/heads/fix", &format!("{AT}\n"));
        assert_eq!(pushed(&git, "c1"), at(Some("fix"), "origin/fix"));

        put(&git, "elsewhere/worktrees/c1/HEAD", &format!("{AT}\n"));
        std::fs::remove_dir_all(git.join("worktrees")).expect("removed");
        std::os::unix::fs::symlink(git.join("elsewhere/worktrees"), git.join("worktrees"))
            .expect("linked");
        assert_eq!(pushed(&git, "c1"), Pushed::Unread);
    }

    /// CHECKOUT-15. A ref, or a directory of remote branches, that is there but cannot be read
    /// reads as unread, rather than as whatever `packed-refs` says or as not pushed.
    #[cfg(unix)]
    #[test]
    fn a_ref_that_cannot_be_read_reads_as_unread() {
        use std::os::unix::fs::PermissionsExt;
        let locked = |path: &Path, mode| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("mode");
        };
        let git = repository("locked");
        put(&git, "worktrees/c1/HEAD", "ref: refs/heads/fix\n");
        put(&git, "refs/heads/fix", &format!("{ELSEWHERE}\n"));
        put(&git, "refs/remotes/origin/fix", &format!("{AT}\n"));
        put(
            &git,
            "packed-refs",
            &format!("{AT} refs/heads/fix\n{AT} refs/remotes/origin/fix\n"),
        );
        let heads = git.join("refs/heads");
        locked(&heads, 0o000);
        // A superuser reads it anyway, which leaves nothing to observe.
        if std::fs::read_dir(&heads).is_ok() {
            locked(&heads, 0o755);
            eprintln!("running as a superuser, so a ref that cannot be read is not tried");
            return;
        }
        let heads_locked = pushed(&git, "c1");
        locked(&heads, 0o755);
        assert_eq!(heads_locked, Pushed::Unread);

        let origin = git.join("refs/remotes/origin");
        locked(&origin, 0o000);
        let remotes_locked = pushed(&git, "c1");
        locked(&origin, 0o755);
        assert_eq!(remotes_locked, Pushed::Unread);

        let tip = git.join("refs/remotes/origin/fix");
        locked(&tip, 0o000);
        let tip_locked = pushed(&git, "c1");
        locked(&tip, 0o644);
        assert_eq!(tip_locked, Pushed::Unread);
        assert_eq!(
            pushed(&git, "c1"),
            Pushed::Nowhere {
                branch: Some("fix".into())
            }
        );
    }

    /// CHECKOUT-15. A pipe in place of HEAD or a ref reads as unread, and the listing does not
    /// wait for something to write to it. Skipped, saying so, where `mkfifo` is not installed.
    #[cfg(unix)]
    #[test]
    fn a_pipe_is_not_waited_on() {
        let git = repository("pipe");
        put(&git, "refs/heads/fix", &format!("{AT}\n"));
        put(
            &git,
            "packed-refs",
            &format!("{AT} refs/remotes/origin/fix\n"),
        );
        std::fs::create_dir_all(git.join("worktrees/c1")).expect("entry");
        std::fs::create_dir_all(git.join("refs/remotes/origin")).expect("remotes");
        let made = |path: &Path| {
            std::process::Command::new("mkfifo")
                .arg(path)
                .status()
                .is_ok_and(|status| status.success())
        };
        if !made(&git.join("worktrees/c1/HEAD")) {
            eprintln!("mkfifo is not installed, so a pipe is not tried");
            return;
        }
        let read = |git: &Path| {
            let (sent, got) = std::sync::mpsc::channel();
            let git = git.to_path_buf();
            std::thread::spawn(move || sent.send(pushed(&git, "c1")));
            got.recv_timeout(std::time::Duration::from_secs(10))
                .expect("the listing waited on a pipe")
        };
        assert_eq!(read(&git), Pushed::Unread);

        std::fs::remove_file(git.join("worktrees/c1/HEAD")).expect("removed");
        put(&git, "worktrees/c1/HEAD", "ref: refs/heads/fix\n");
        assert!(made(&git.join("refs/remotes/origin/fix")), "mkfifo");
        assert_eq!(read(&git), Pushed::Unread);
    }

    #[cfg(unix)]
    fn git(dir: &Path, args: &[&str]) -> Option<std::process::Output> {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "a")
            .env("GIT_AUTHOR_EMAIL", "a@example.com")
            .env("GIT_COMMITTER_NAME", "a")
            .env("GIT_COMMITTER_EMAIL", "a@example.com")
            .output()
            .ok()
    }

    #[cfg(unix)]
    fn run(dir: &Path, args: &[&str]) {
        let out = git(dir, args).expect("git ran");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// CHECKOUT-15. What git itself writes reads as pushed after a push, as not pushed after a
    /// commit, and as pushed again once git has packed its refs. Skipped, saying so, where git is
    /// not installed.
    #[cfg(unix)]
    #[test]
    fn a_branch_git_pushed_reads_as_pushed() {
        let root = crate::testutil::scratch_dir("pushed-git");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("scratch");
        if git(&root, &["--version"]).is_none_or(|out| !out.status.success()) {
            eprintln!("git is not installed, so what git writes is not read");
            return;
        }
        let work = root.join("work");
        let remote = root.join("remote.git");
        run(&root, &["init", "-q", "-b", "main", "work"]);
        run(&root, &["init", "-q", "--bare", "remote.git"]);
        run(&work, &["commit", "-q", "--allow-empty", "-m", "first"]);
        run(
            &work,
            &["remote", "add", "origin", &remote.display().to_string()],
        );
        run(&work, &["worktree", "add", "-q", "-b", "fix", "../c1"]);
        let c1 = root.join("c1");
        let git_dir = work.join(".git");
        assert_eq!(
            pushed(&git_dir, "c1"),
            Pushed::Nowhere {
                branch: Some("fix".into())
            }
        );
        run(&c1, &["push", "-q", "origin", "fix"]);
        assert_eq!(pushed(&git_dir, "c1"), at(Some("fix"), "origin/fix"));
        run(&c1, &["commit", "-q", "--allow-empty", "-m", "second"]);
        assert_eq!(
            pushed(&git_dir, "c1"),
            Pushed::Nowhere {
                branch: Some("fix".into())
            }
        );
        run(&c1, &["push", "-q", "origin", "fix"]);
        run(&work, &["pack-refs", "--all", "--prune"]);
        assert!(!git_dir.join("refs/remotes/origin/fix").exists(), "packed");
        assert_eq!(pushed(&git_dir, "c1"), at(Some("fix"), "origin/fix"));
    }
}
