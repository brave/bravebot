//! The project a fact belongs to.

use crate::git::config::parse_config;
use crate::servers::repository_holding;
use sha2::{Digest, Sha256};
use std::path::Path;

/// A project, by the id that names it and the directory name derived from that id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectId {
    /// What identifies the project, kept in the `id` file so a directory can be traced back.
    pub id: String,
    /// The first 16 hex characters of the SHA-256 of what names it: the name of its directory.
    pub dir: String,
}

impl ProjectId {
    /// The project called `id`, with the directory name its hash gives.
    pub fn named(id: &str) -> ProjectId {
        ProjectId::hashed(id.as_bytes(), id)
    }

    /// The project whose directory name is the hash of `bytes`, shown as `id`.
    fn hashed(bytes: &[u8], id: &str) -> ProjectId {
        let digest = Sha256::digest(bytes);
        let dir = digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        ProjectId {
            id: id.to_string(),
            dir,
        }
    }
}

/// The project `working_directory` belongs to (RECALL-1): named by its `origin` remote once the
/// workspace is `trusted`, and by the canonical path of its repository root until then, or when
/// there is no `origin` or no repository.
pub fn project_id(working_directory: &Path, trusted: bool) -> ProjectId {
    let root = repository_holding(working_directory).unwrap_or_else(|| {
        working_directory
            .canonicalize()
            .unwrap_or_else(|_| working_directory.to_path_buf())
    });
    match trusted.then(|| origin_of(&root)).flatten() {
        Some(origin) => ProjectId::named(&origin),
        // The path's own bytes are hashed, so two roots that differ only in bytes that are not
        // UTF-8 stay two projects. The lossy text is only what the `id` file shows.
        None => ProjectId::hashed(root.as_os_str().as_encoded_bytes(), &root.to_string_lossy()),
    }
}

/// [`project_id`], with what was kept for the project under its path moved to its `origin` name the
/// first time the workspace is trusted (RECALL-1).
///
/// Only where the `origin` name has nothing kept yet, so nothing is merged or written over, and
/// never in a session that writes nothing to the state directory. Where both are kept, the path's
/// stays where it is.
pub fn settled(home: &Path, working_directory: &Path, trusted: bool) -> ProjectId {
    settled_where(
        home,
        working_directory,
        trusted,
        crate::home::writable().is_some(),
    )
}

/// [`settled`], told whether the session may write to the state directory.
fn settled_where(
    home: &Path,
    working_directory: &Path,
    trusted: bool,
    writable: bool,
) -> ProjectId {
    let project = project_id(working_directory, trusted);
    if !trusted || !writable {
        return project;
    }
    let by_path = project_id(working_directory, false);
    let projects = home.join("projects");
    let (from, to) = (projects.join(&by_path.dir), projects.join(&project.dir));
    if by_path.dir != project.dir
        && std::fs::symlink_metadata(&from).is_ok_and(|meta| meta.is_dir())
        && std::fs::symlink_metadata(&to).is_err()
        && std::fs::rename(&from, &to).is_ok()
    {
        // The `id` file names the project the directory is now for.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the id file only labels the directory for a person tracing it back, and the \
                      next save writes it again; the files are already where they belong"
        )]
        let _ = super::store::Store::new(home, project.clone()).make_project_directory();
    }
    project
}

/// The normalised `origin` URL of the repository at `root`, read from its configuration file
/// without starting git.
fn origin_of(root: &Path) -> Option<String> {
    // Only the repository's own git directory is read: `.git/config`, the configuration of the
    // repository a `.git` file names, or `config` in a bare layout that has no `.git`. A tracked
    // `config` beside a `.git` file is content nobody has vouched for.
    let dot_git = root.join(".git");
    let config = if dot_git.symlink_metadata().is_ok_and(|meta| meta.is_file()) {
        linked_git_directory(&dot_git)?.join("config")
    } else if dot_git.symlink_metadata().is_ok() {
        dot_git.join("config")
    } else if root.join("HEAD").is_file()
        && root.join("objects").is_dir()
        && root.join("refs").is_dir()
    {
        root.join("config")
    } else {
        return None;
    };
    let bytes = std::fs::read(config).ok()?;
    let entries = parse_config(&bytes)?;
    // Git fetches from the first `url` an `origin` names, so the first is the project's.
    let url = entries.iter().find(|entry| {
        entry.section == "remote"
            && entry.subsection.as_deref() == Some(b"origin".as_slice())
            && entry.key == "url"
    })?;
    normalise_origin(std::str::from_utf8(url.value.as_deref()?).ok()?.trim())
}

/// The git directory that holds the configuration of a linked worktree or a submodule, whose
/// `.git` is the file `dot_git`: the directory its `gitdir:` line names, relative to the file's
/// own directory or absolute, and then the one that directory's `commondir` names, relative to it
/// or absolute, where it has one. A worktree's own directory under `worktrees/` has no `config`
/// of its own, and its `commondir` leads to the repository's. `None` where either file cannot be
/// read or names nothing.
fn linked_git_directory(dot_git: &Path) -> Option<std::path::PathBuf> {
    /// The first line of a small file, trimmed, or none.
    fn first_line(path: &Path) -> Option<String> {
        use std::io::Read;
        let mut text = String::new();
        std::fs::File::open(path)
            .ok()?
            .take(4096)
            .read_to_string(&mut text)
            .ok()?;
        let line = text.lines().next()?.trim();
        (!line.is_empty()).then(|| line.to_string())
    }
    let named = first_line(dot_git)?;
    let gitdir = named.strip_prefix("gitdir:")?.trim();
    if gitdir.is_empty() {
        return None;
    }
    let gitdir = dot_git.parent()?.join(gitdir);
    if !gitdir.is_dir() {
        return None;
    }
    let common = match std::fs::symlink_metadata(gitdir.join("commondir")) {
        Ok(_) => gitdir.join(first_line(&gitdir.join("commondir"))?),
        Err(_) => gitdir,
    };
    common.is_dir().then_some(common)
}

/// A remote URL as a project id: scheme, credentials and a trailing `.git` taken off, the host in
/// lower case. `None` for a URL that names no host, such as a path on this machine.
pub fn normalise_origin(url: &str) -> Option<String> {
    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (Some(scheme), rest),
        None => (None, url),
    };
    if scheme.is_some_and(|scheme| scheme.eq_ignore_ascii_case("file")) {
        return None;
    }
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    // The scp form `host:path` has no scheme, and its colon is the path's start, not a port's.
    let (host, path) = match (scheme, authority.split_once(':')) {
        (None, Some((host, first))) => (host, format!("{first}/{path}")),
        _ => (authority, path.to_string()),
    };
    let path = path.trim_matches('/');
    let path = path
        .strip_suffix(".git")
        .unwrap_or(path)
        .trim_end_matches('/');
    if host.is_empty() || path.is_empty() || host.starts_with('.') {
        return None;
    }
    Some(format!("{}/{path}", host.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recall::fact::{Fact, Scope, Source};
    use crate::recall::store::Store;
    use bravebot_core::label::Confidentiality;
    use std::path::PathBuf;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create scratch");
            Self(path.canonicalize().expect("canonical scratch"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A repository root holding a `.git/config` that names `origin` as `url`.
    fn repository(scratch: &Scratch, url: &str) -> PathBuf {
        let root = scratch.0.join("repo");
        std::fs::create_dir_all(root.join(".git")).expect("git directory");
        std::fs::write(
            root.join(".git/config"),
            format!("[core]\n\trepositoryformatversion = 0\n[remote \"origin\"]\n\turl = {url}\n"),
        )
        .expect("config");
        root
    }

    /// RECALL-1: what was kept under the path before the trust yes moves to the `origin` name at
    /// the first trusted look, so it keeps loading. Where the `origin` already has its own, nothing
    /// is merged and the path's stays, and neither an untrusted look nor a session that writes
    /// nothing to the state directory moves anything.
    #[test]
    fn what_was_kept_under_the_path_moves_to_the_origin_once_trusted() {
        let scratch = Scratch::new("recall-project-settled");
        let root = repository(&scratch, "git@github.com:x/y.git");
        let home = scratch.0.join("home");
        let projects = home.join("projects");
        let (by_path, by_origin) = (project_id(&root, false), project_id(&root, true));
        let kept = projects.join(&by_path.dir).join("facts");
        std::fs::create_dir_all(&kept).expect("facts");
        std::fs::write(kept.join("k.md"), "KEPT").expect("fact");

        assert_eq!(settled(&home, &root, false), by_path);
        assert!(kept.join("k.md").exists(), "an untrusted look moved it");
        assert_eq!(settled_where(&home, &root, true, false), by_origin);
        assert!(
            kept.join("k.md").exists(),
            "a session that writes nothing to the state directory moved it"
        );

        assert_eq!(settled(&home, &root, true), by_origin);
        let moved = projects.join(&by_origin.dir);
        assert_eq!(
            std::fs::read_to_string(moved.join("facts/k.md")).expect("moved"),
            "KEPT"
        );
        assert_eq!(
            std::fs::read_to_string(moved.join("id")).expect("id"),
            "github.com/x/y"
        );
        assert!(!projects.join(&by_path.dir).exists());

        std::fs::create_dir_all(&kept).expect("facts again");
        std::fs::write(kept.join("j.md"), "LATER").expect("fact");
        settled(&home, &root, true);
        assert!(
            kept.join("j.md").exists(),
            "the origin's own were written over"
        );
        assert!(!moved.join("facts/j.md").exists());
    }

    #[test]
    fn the_two_spellings_of_one_remote_are_one_project() {
        let scp = normalise_origin("git@github.com:brave/bravebot.git");
        let https = normalise_origin("https://github.com/brave/bravebot");
        assert_eq!(scp.as_deref(), Some("github.com/brave/bravebot"));
        assert_eq!(scp, https);
        assert_eq!(
            normalise_origin("ssh://git@GitHub.com/brave/bravebot.git/").as_deref(),
            Some("github.com/brave/bravebot")
        );
    }

    #[test]
    fn a_remote_that_names_no_host_names_no_project() {
        assert_eq!(normalise_origin("/srv/git/bravebot.git"), None);
        assert_eq!(normalise_origin("file:///srv/git/bravebot.git"), None);
        assert_eq!(normalise_origin(""), None);
    }

    #[test]
    fn credentials_in_a_remote_are_dropped() {
        assert_eq!(
            normalise_origin("https://user:tok@github.com/a/b.git").as_deref(),
            Some("github.com/a/b")
        );

        let scratch = Scratch::new("recall-project-credentials");
        let root = repository(&scratch, "https://user:tok@github.com/a/b.git");
        let project = project_id(&root, true);
        assert_eq!(project.id, "github.com/a/b");
        let home = scratch.0.join("state");
        let fact = Fact {
            key: "k".to_string(),
            source: Source::Person,
            label: Confidentiality::Public,
            created: None,
            verified: None,
            superseded_by: None,
            session: None,
            tags: Vec::new(),
            body: "text\n".to_string(),
        };
        Store::new(&home, project)
            .write(Scope::Project, &fact)
            .expect("write");
        let mut files = vec![home];
        while let Some(path) = files.pop() {
            if path.is_dir() {
                files.extend(
                    std::fs::read_dir(&path)
                        .expect("read")
                        .map(|e| e.expect("entry").path()),
                );
            } else {
                let text = std::fs::read_to_string(&path).expect("read file");
                assert!(!text.contains("tok"), "{} holds the token", path.display());
            }
        }
    }

    #[test]
    fn an_untrusted_workspace_is_named_by_its_path() {
        let scratch = Scratch::new("recall-project-untrusted");
        let root = repository(&scratch, "git@github.com:x/y.git");
        let nested = root.join("src");
        std::fs::create_dir_all(&nested).expect("nested");
        assert_eq!(
            project_id(&nested, false).id,
            root.to_string_lossy().into_owned()
        );
    }

    #[test]
    fn a_trusted_workspace_is_named_by_its_origin() {
        let scratch = Scratch::new("recall-project-trusted");
        let root = repository(&scratch, "git@github.com:x/y.git");
        let nested = root.join("src");
        std::fs::create_dir_all(&nested).expect("nested");
        assert_eq!(project_id(&nested, true).id, "github.com/x/y");
    }

    #[test]
    fn a_trusted_repository_with_no_origin_is_named_by_its_path() {
        let scratch = Scratch::new("recall-project-no-origin");
        let root = scratch.0.join("repo");
        std::fs::create_dir_all(root.join(".git")).expect("git directory");
        std::fs::write(root.join(".git/config"), "[core]\n\tbare = false\n").expect("config");
        assert_eq!(
            project_id(&root, true).id,
            root.to_string_lossy().into_owned()
        );
    }

    #[test]
    fn a_tracked_config_beside_a_git_file_does_not_name_the_project() {
        let scratch = Scratch::new("recall-project-git-file");
        let root = scratch.0.join("repo");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::write(root.join(".git"), "gitdir: /elsewhere\n").expect("git file");
        std::fs::write(
            root.join("config"),
            "[remote \"origin\"]\n\turl = git@github.com:victim/private.git\n",
        )
        .expect("tracked config");
        assert_eq!(
            project_id(&root, true).id,
            root.to_string_lossy().into_owned()
        );
    }

    /// RECALL-1: a linked worktree is the repository it was added from. Its `.git` file names
    /// its own directory under the main repository's `worktrees/`, whose `commondir` leads back to
    /// the git directory that holds `config`. Built by hand as `git worktree add` lays it out.
    #[test]
    fn a_linked_worktree_is_named_by_its_repositorys_origin() {
        let scratch = Scratch::new("recall-project-worktree");
        let main = repository(&scratch, "git@github.com:brave/bravebot.git");
        let own = main.join(".git/worktrees/linked");
        std::fs::create_dir_all(&own).expect("worktree's own directory");
        std::fs::write(own.join("commondir"), "../..\n").expect("commondir");
        std::fs::write(own.join("HEAD"), "ref: refs/heads/linked\n").expect("HEAD");
        let linked = scratch.0.join("linked");
        std::fs::create_dir_all(&linked).expect("worktree");
        std::fs::write(linked.join(".git"), format!("gitdir: {}\n", own.display()))
            .expect("git file");
        assert_eq!(project_id(&linked, true).id, "github.com/brave/bravebot");

        // A relative `gitdir:`, as a submodule writes it, and a git directory with no `commondir`.
        let module = main.join(".git/modules/sub");
        std::fs::create_dir_all(&module).expect("module directory");
        std::fs::write(
            module.join("config"),
            "[remote \"origin\"]\n\turl = https://github.com/brave/sub\n",
        )
        .expect("module config");
        let sub = main.join("sub");
        std::fs::create_dir_all(&sub).expect("submodule");
        std::fs::write(sub.join(".git"), "gitdir: ../.git/modules/sub\n").expect("git file");
        assert_eq!(project_id(&sub, true).id, "github.com/brave/sub");

        // Untrusted, the path still names it; and a `gitdir:` that leads nowhere falls back to it.
        assert_eq!(
            project_id(&linked, false).id,
            linked.to_string_lossy().into_owned()
        );
        std::fs::write(linked.join(".git"), "gitdir: ../nowhere\n").expect("git file");
        assert_eq!(
            project_id(&linked, true).id,
            linked.to_string_lossy().into_owned()
        );
    }

    /// Git fetches from the first `url` an `origin` names, so that one names the project.
    #[test]
    fn the_first_origin_url_names_the_project() {
        let scratch = Scratch::new("recall-project-two-urls");
        let root = repository(&scratch, "git@github.com:first/one.git");
        let mut config = std::fs::read_to_string(root.join(".git/config")).expect("config");
        config.push_str("\turl = git@github.com:second/two.git\n");
        std::fs::write(root.join(".git/config"), config).expect("config");
        assert_eq!(project_id(&root, true).id, "github.com/first/one");
    }

    #[test]
    fn a_bare_repository_is_named_by_its_origin() {
        let scratch = Scratch::new("recall-project-bare");
        let root = scratch.0.join("repo.git");
        std::fs::create_dir_all(root.join("objects")).expect("objects");
        std::fs::create_dir_all(root.join("refs")).expect("refs");
        std::fs::write(root.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
        std::fs::write(
            root.join("config"),
            "[remote \"origin\"]\n\turl = git@github.com:x/y.git\n",
        )
        .expect("config");
        assert_eq!(project_id(&root, true).id, "github.com/x/y");
    }

    #[test]
    fn a_directory_outside_a_repository_is_named_by_its_path() {
        let outside = crate::servers::outside_any_repository("recall-project-outside");
        let canonical = outside.canonicalize().expect("canonical");
        for trusted in [false, true] {
            assert_eq!(
                project_id(&outside, trusted).id,
                canonical.to_string_lossy().into_owned()
            );
        }
        let _ = std::fs::remove_dir_all(&outside);
    }

    /// RECALL-1: a root whose name is not UTF-8 and its lossy lookalike are two projects, or one
    /// would load the other's rules and facts.
    #[cfg(unix)]
    #[test]
    fn a_root_that_is_not_utf8_is_not_its_lossy_lookalike() {
        use std::os::unix::ffi::OsStrExt;
        let outside = crate::servers::outside_any_repository("recall-project-not-utf8");
        let raw = outside.join(std::ffi::OsStr::from_bytes(b"repo-\xff"));
        let lookalike = outside.join("repo-\u{FFFD}");
        assert_ne!(
            project_id(&raw, false).dir,
            project_id(&lookalike, false).dir
        );
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn the_directory_name_is_sixteen_hex_of_the_ids_hash() {
        // SHA-256("abc") begins ba7816bf8f01cfea.
        let project = ProjectId::named("abc");
        assert_eq!(project.dir, "ba7816bf8f01cfea");
        assert_eq!(project.dir.len(), 16);
        assert!(
            project
                .dir
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }
}
