//! Where a name lands: expanding `~`, following a relative name up or down from the root,
//! confining the result, and refusing a spelling that is not the file it appears to be. These work
//! on typed names and the file system's answers about where a name lands (`canonicalize`,
//! `symlink_metadata`, `read_link`). None of it reads a file's bytes or gives anything a label.

use super::{Remedy, Workspace, WorkspaceError, names};
use bravebot_core::spelling::to_key;
use bravebot_core::trust::is_absolute_key;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// Whether the first component of `named` is exactly `~`.
pub(super) fn named_home(named: &str) -> bool {
    Path::new(named).components().next() == Some(Component::Normal(std::ffi::OsStr::new("~")))
}

/// `named` with a leading `~` replaced by `home`, or `None` where it does not start with one.
///
/// Only a whole first component counts, so `~notes/x` stays a relative path (CMD-5). A machine
/// naming no home refuses the `~` rather than reading it as a directory named `~` (CMDLINE-4).
pub(super) fn expand_home(
    named: &str,
    home: Option<&Path>,
) -> Result<Option<PathBuf>, &'static str> {
    if !named_home(named) {
        return Ok(None);
    }
    let path = Path::new(named);
    let home = home.ok_or("`~` stands for the home directory and this user has none")?;
    let rest = path.components().skip(1).collect::<PathBuf>();
    Ok(Some(match rest.as_os_str().is_empty() {
        true => home.to_path_buf(),
        false => home.join(rest),
    }))
}

/// Refuse a directory whose resolved name the trust map cannot key a rule under.
///
/// A directory is opened by handing the map the name it resolved to, and the map reads a name
/// without a leading slash as a path under the working directory (TRUST-18), where the rule
/// covering the project covers it. So a rule about a directory named any other way would be keyed
/// inside the project, and the answer given about the project at startup would decide files in a
/// directory nobody vouched for. Refusing is the fail-closed half of that clause, and it is here
/// rather than in the map because canonicalising a name is filesystem work.
///
/// Every door that opens a directory by name has to refuse it, `/cd` as much as `/add-dir`: the
/// working directory a map reads its relative names under is replaced by the destination, so one
/// the map cannot key would read every one of them under a name inside the project instead.
///
/// The name is spelled the way [`key_of`] spells it for the caller building the key, so the two
/// cannot disagree about whether the directory has one. A drive letter has one and a share does
/// not ([`bravebot_core::spelling::to_key`]).
pub(super) fn refuse_unkeyable(
    canonical: &Path,
    named: &str,
    backslash_separates: bool,
) -> Result<(), WorkspaceError> {
    match is_absolute_key(&to_key(&canonical.to_string_lossy(), backslash_separates)) {
        true => Ok(()),
        false => Err(WorkspaceError::Invalid {
            path: named.to_string(),
            reason: "is not spelled from '/', so no trust rule can be keyed under it",
        }),
    }
}

/// The most symlinks one path may be followed through by name before it is treated as a cycle.
///
/// A link the operating system can resolve is resolved by it, under a limit of its own. Only the
/// ones it will not resolve are followed here, and two of those in a row is already pathological.
pub(super) const MAX_LINKS_FOLLOWED: usize = 8;

/// Refuse a name Windows would read as something other than the one file it spells, on a host
/// that does: a stream, a device, a trailing dot or space, an 8.3 short name.
///
/// Asked about each name as typed, so the rules see the file the operation will open and not a
/// spelling of it. A short name is accepted in a root, which is a directory a person chose. The
/// host is a parameter so the refusal is exercised everywhere.
pub(super) fn refuse_misleading_names(
    path: &Path,
    named: &str,
    root: bool,
    windows: bool,
) -> Result<(), WorkspaceError> {
    if !windows {
        return Ok(());
    }
    let misleading = path.components().any(|component| match component {
        Component::Normal(name) => names::misleads(&name.to_string_lossy(), root),
        _ => false,
    });
    match misleading {
        true => Err(WorkspaceError::Invalid {
            path: named.to_string(),
            reason: "names something other than the file it spells on Windows",
        }),
        false => Ok(()),
    }
}

/// Where an operation on `path` would land, with every symlink on the way already followed.
///
/// A path that does not exist cannot be canonicalised, so the deepest ancestor that can is, and
/// the components that do not exist yet are appended to it. Those components are created by the
/// operation itself, so nothing can redirect them; the ones that already exist are resolved
/// before the comparison rather than after, which is what catches a directory symlink leaving
/// the tree. A link the operating system will not resolve is followed by name instead, since a
/// write follows a dangling one too and creates its target.
///
/// `None` where no destination can be named: a cycle, or an entry that is there and will not
/// resolve, including one whose directory cannot be searched. A caller confining an operation
/// refuses that, because it cannot say where the operation would go.
///
/// Says where an operation goes now, not where it goes when it happens; the window between the
/// two is a known cost against the clause this serves.
pub(crate) fn destination(path: &Path) -> Option<PathBuf> {
    let mut missing: Vec<OsString> = Vec::new();
    let mut existing = path.to_path_buf();
    let mut followed = 0usize;

    loop {
        // Asked about the entry rather than what it points at, so that a link with nothing at
        // the other end is not taken for a name nothing occupies.
        match existing.symlink_metadata() {
            Ok(entry) => {
                if let Ok(canonical) = existing.canonicalize() {
                    let mut resolved = canonical;
                    resolved.extend(missing.iter().rev());
                    return Some(resolved);
                }
                if !entry.is_symlink() || followed >= MAX_LINKS_FOLLOWED {
                    return None;
                }
                followed += 1;
                let target = existing.read_link().ok()?;
                existing = if target.is_absolute() {
                    target
                } else {
                    existing.parent()?.join(target)
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                missing.push(existing.file_name()?.to_os_string());
                existing = existing.parent()?.to_path_buf();
            }
            // A component whose directory cannot be searched is not a free name either, and
            // treating it as one would name a destination for a link this cannot see.
            Err(_) => return None,
        }
    }
}

/// The part of `named` written below `opened`, for a name that reaches it through an ancestor.
///
/// The ancestor is matched on where it lands, so any spelling of the open directory is found, and
/// the shallowest one is taken: as much of the name as possible is left as it was written, since
/// what is under it is a name the map may hold a rule of its own about. Walking `ancestors` rather
/// than counted components is what keeps a Windows prefix whole, since `C:` alone names the current
/// directory on that drive and not its root.
///
/// `None` where no ancestor lands there, which is a link straight into the middle of the tree. Such
/// a name has no spelling under the recorded one, and taking the destination's instead is the
/// resolution [`Workspace::trust_key`] rules out.
pub(super) fn written_below(named: &Path, opened: &Path) -> Option<PathBuf> {
    let mut ancestors: Vec<&Path> = named.ancestors().collect();
    ancestors.reverse();
    for walked in ancestors {
        // A prefix that cannot be resolved is skipped rather than ending the search, since it is
        // this walk's own question and not the caller's.
        let Some(reached) = destination(walked) else {
            continue;
        };
        if reached == opened {
            return named.strip_prefix(walked).ok().map(PathBuf::from);
        }
    }
    None
}

impl Workspace {
    /// Whether a resolved path lands in a directory reached by its absolute name.
    ///
    /// The directories the user opened and the session's own. They are reached the same way and
    /// differ in who asked for them, so every test of where a path lands has to cover both or the
    /// session would be handed a directory it cannot write to.
    /// A directory a person opened counts for nothing where the settings keep the tools inside the
    /// root (PERM-16). Resolution is where that has to hold rather than only the door a directory is
    /// opened at: one already open when the restriction was read, which a resume reopening its own
    /// record is, would otherwise stay reachable for the rest of the session.
    pub(super) fn is_opened(&self, resolved: &Path) -> bool {
        let added = !self.reads_stay_inside
            && self
                .added_directories()
                .iter()
                .any(|dir| resolved.starts_with(dir));
        added || self.scratch().is_some_and(|dir| resolved.starts_with(dir))
    }

    /// Whether a resolved path lands where this workspace's file tools may write: the root or a
    /// directory reached by its absolute name.
    pub(crate) fn lands_in_reach(&self, resolved: &Path) -> bool {
        resolved.starts_with(&self.root) || self.is_opened(resolved)
    }

    /// Whether an absolute path is one this workspace may touch.
    ///
    /// For a destination something else opens, which is what a redirection in a command line is:
    /// the plan names the file and the run opens it directly, so the confinement every other write
    /// goes through has to be applied to the path rather than to a call through here.
    ///
    /// The file need not exist yet, so the destination is what is tested rather than the name:
    /// the same test every write through this workspace gets, so the two cannot come apart.
    pub fn confines(&self, path: &Path) -> Result<(), WorkspaceError> {
        let escapes = || WorkspaceError::Escapes {
            path: path.display().to_string(),
            remedy: Remedy::Nothing,
        };
        let resolved = destination(path).ok_or_else(escapes)?;
        if self.lands_in_reach(&resolved) {
            return Ok(());
        }
        Err(escapes())
    }

    /// Other names of the file something else opens at the absolute `path`, spelled as
    /// [`Workspace::relative_display`] spells them.
    ///
    /// [`Workspace::landing`] resolves a name as the file tools do, which refuses a `..` inside
    /// the root, so it gives no second name for `config/sub/../app.env`. A rule is about the file
    /// (PERM-7), and it can be written in either of two names here. One is the spelling with each
    /// `..` taken out ([`Workspace::climbed`]), which is the rule's own name where a link is on
    /// the way (`config` a link, or `app.env` one). The other is where the path lands once
    /// everything is followed, as [`Workspace::confines`] tests it, which is the only name there is
    /// where a `..` leaves a link.
    pub(crate) fn names_reached(&self, path: &Path) -> Vec<String> {
        self.climbed(path)
            .into_iter()
            .chain(destination(path))
            .map(|reached| self.relative_display(&reached))
            .collect()
    }

    /// Refuse an absolute path something else opens where Windows would read a name in it as
    /// another file (PERM-7), the names [`Workspace::resolve`] refuses in a file tool's path.
    ///
    /// A redirection in a command line is such a path, read or written. The open directory it
    /// lands in may be spelled by a short name, which names that one directory, and the rules are
    /// asked about the file under the name it reaches as well ([`Workspace::names_reached`]). What
    /// is written below that directory may not be, and a path that reaches no open directory
    /// through an ancestor of its own, because it lands in none or arrives through a link into the
    /// middle of one, is exempt nowhere.
    pub(crate) fn refuse_misleading(&self, path: &Path) -> Result<(), WorkspaceError> {
        self.refuse_misleading_on(path, self.windows_names)
    }

    /// [`Workspace::refuse_misleading`] with the host supplied, so either answer is exercised.
    pub(super) fn refuse_misleading_on(
        &self,
        path: &Path,
        windows: bool,
    ) -> Result<(), WorkspaceError> {
        if !windows {
            return Ok(());
        }
        let named = path.display().to_string();
        let below = destination(path)
            .and_then(|resolved| self.landed_in(&resolved))
            .and_then(|opened| written_below(path, &opened));
        let Some(below) = below else {
            return refuse_misleading_names(path, &named, false, windows);
        };
        refuse_misleading_names(path, &named, true, windows)?;
        refuse_misleading_names(&below, &named, false, windows)
    }

    /// Resolve a path against the workspace.
    ///
    /// A relative path always means the primary root. An absolute path is legal only inside a
    /// directory the user added by name, and is refused otherwise: an absolute path was refused
    /// outright before `/add-dir` existed, and naming a directory is what makes one reachable.
    ///
    /// A `..` is resolved only into a directory that is open, and then as the absolute path it
    /// lands on, which is checked as any other ([`Workspace::climb_lands`]). A `..` through a link,
    /// or that lands inside the root, is refused: taking it out there would admit a path that
    /// climbs out of the root and back in by its own name, which lands inside and would pass a
    /// test on where it lands.
    ///
    /// Containment is then decided against where the path lands and not against how it is spelled,
    /// so it holds for a path that does not exist yet: a write creates what it names, and a
    /// directory symlink on the way out of the tree would otherwise carry the bytes outside a
    /// lexical test that saw nothing wrong. What comes back is that destination, so a caller that
    /// needs the file rather than the name has it.
    pub(crate) fn resolve(&self, relative: &str) -> Result<PathBuf, WorkspaceError> {
        self.resolve_with_home(relative, self.home.as_deref())
    }

    /// `named` with a leading `~` spelled out as the home directory, and as given otherwise,
    /// including where no home is known, which [`Workspace::resolve`] refuses.
    ///
    /// For a caller that hands a path to something that does not resolve it as the file tools do:
    /// a `deny` rule is anchored at the home directory and never matches the spelling `~/x`, and
    /// a watch is looked at later from a working directory that may have moved, where `~/x` reads
    /// as relative and the watch is ended.
    pub(crate) fn expanded(&self, named: &str) -> String {
        match self.spelled(named, self.home.as_deref()) {
            Ok(Some(path)) => path.to_string_lossy().into_owned(),
            _ => named.to_string(),
        }
    }

    /// `named` as the absolute path it is resolved as, where that is not how it was written: a
    /// leading `~` spelled out, or the `..` of a path that climbs out of the root taken out
    /// ([`Workspace::climb_lands`]). `None` where the name stands as written.
    pub(super) fn spelled(
        &self,
        named: &str,
        home: Option<&Path>,
    ) -> Result<Option<PathBuf>, &'static str> {
        let expanded = expand_home(named, home)?;
        let candidate = expanded.as_deref().unwrap_or_else(|| Path::new(named));
        Ok(self.climb_lands(candidate).or(expanded))
    }

    /// `named` as the absolute path of the open directory a `..` in it lands in, where the call
    /// is to be reported under that spelling, and `None` for any other name, including a `~`
    /// with no `..` (whose messages keep the spelling that was written) and a `..` that is
    /// refused.
    pub(crate) fn climbed_to(&self, named: &str) -> Option<String> {
        let expanded = expand_home(named, self.home.as_deref()).ok()?;
        let candidate = expanded.as_deref().unwrap_or_else(|| Path::new(named));
        let lands = self.climb_lands(candidate)?;
        self.is_opened(&destination(&lands)?)
            .then(|| lands.to_string_lossy().into_owned())
    }

    /// Stand in for the environment's home directory, so a test does not depend on whose machine
    /// it runs on.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_home(mut self, home: Option<PathBuf>) -> Self {
        self.home = home;
        self
    }

    /// [`Workspace::resolve`] with the home directory a leading `~` stands for supplied, so the
    /// rule is testable without whose machine the test runs on.
    ///
    /// A first component that is exactly `~` is the person's home directory, expanded before any
    /// check is made so what follows sees an absolute path like any other: reachable only inside
    /// an opened directory, and refused otherwise. A `..` that lands in an opened directory is
    /// taken out the same way. Messages keep the spelling that was written.
    pub(super) fn resolve_with_home(
        &self,
        relative: &str,
        home: Option<&Path>,
    ) -> Result<PathBuf, WorkspaceError> {
        let expanded = self
            .spelled(relative, home)
            .map_err(|reason| WorkspaceError::Invalid {
                path: relative.to_string(),
                reason,
            })?;
        let candidate = expanded.as_deref().unwrap_or_else(|| Path::new(relative));

        if candidate.is_absolute() {
            return self.resolve_added(candidate, relative);
        }

        for component in candidate.components() {
            match component {
                Component::ParentDir => {
                    return Err(Self::refuse_climb(relative));
                }
                Component::Prefix(_) | Component::RootDir => {
                    return Err(WorkspaceError::Invalid {
                        path: relative.to_string(),
                        reason: "must not name a root or drive",
                    });
                }
                Component::CurDir | Component::Normal(_) => {}
            }
        }

        refuse_misleading_names(candidate, relative, false, cfg!(windows))?;

        let escapes = || WorkspaceError::Escapes {
            path: relative.to_string(),
            remedy: Remedy::Nothing,
        };
        let resolved = destination(&self.root.join(candidate)).ok_or_else(escapes)?;
        if !resolved.starts_with(&self.root) {
            return Err(escapes());
        }
        Ok(resolved)
    }

    /// The name `named` lands on, spelled as [`Workspace::relative_display`] spells it, where that
    /// is not the name it was typed as.
    ///
    /// A permission rule is written about a file (PERM-7), and a name reaches the file it lands on
    /// by more routes than its own spelling: a symbolic link, a case variant on a volume that
    /// ignores case, an 8.3 short name. `None` where the name lands on itself, and where
    /// [`Workspace::resolve`] refuses it. A file tool resolves the name and so refuses it on its
    /// own; a path something else opens has to be held to [`Workspace::confines`] and
    /// [`Workspace::refuse_misleading`] instead.
    ///
    /// Asked of the file system, not of any file's contents, and it is only ever used to ask the
    /// rules a second question, so the answer can add a refusal or an approval and can never
    /// remove one.
    pub(crate) fn landing(&self, named: &str) -> Option<String> {
        let landed = self.relative_display(&self.resolve(named).ok()?);
        let typed = Path::new(named)
            .components()
            .filter(|component| !matches!(component, Component::CurDir));
        (!Path::new(&landed).components().eq(typed)).then_some(landed)
    }

    /// `named` spelled relative to the workspace root, without asking the file system: `./docs/a.md`
    /// and `<root>/docs/a.md` are `docs/a.md`, and a link is not followed.
    ///
    /// `None` for an absolute name that is not under the root as written, which has no
    /// workspace-relative spelling; [`Workspace::landing`] is the name it reaches.
    pub(crate) fn spelled_in_workspace(&self, named: &str) -> Option<String> {
        let expanded = self.expanded(named);
        let path = Path::new(&expanded);
        if path.is_absolute() && !path.starts_with(&self.root) {
            return None;
        }
        let plain: PathBuf = path
            .components()
            .filter(|component| !matches!(component, Component::CurDir))
            .collect();
        Some(self.relative_display(&plain))
    }

    /// Resolve an absolute path, which is legal only inside a directory reached by its absolute
    /// name: one the user added, or the session's own.
    ///
    /// The containment test is against where the path lands, so a symlink inside an added
    /// directory pointing elsewhere is refused exactly as one in the primary root is, whether or
    /// not the file it names exists yet.
    fn resolve_added(&self, candidate: &Path, named: &str) -> Result<PathBuf, WorkspaceError> {
        if candidate
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(Self::refuse_climb(named));
        }

        refuse_misleading_names(candidate, named, true, cfg!(windows))?;

        let escapes = |remedy| WorkspaceError::Escapes {
            path: named.to_string(),
            remedy,
        };
        let resolved = destination(candidate).ok_or_else(|| escapes(Remedy::Nothing))?;
        if !self.is_opened(&resolved) {
            return Err(escapes(self.remedy_outside(&resolved)));
        }
        if let Some(below) = self
            .landed_in(&resolved)
            .and_then(|opened| written_below(candidate, &opened))
        {
            refuse_misleading_names(&below, named, false, cfg!(windows))?;
        }
        Ok(resolved)
    }

    /// [`Workspace::resolve`], and for a read alone also a file under the directory of a skill of the
    /// user's own that was loaded this turn.
    ///
    /// Only where the name is absolute and lands, once links are followed, inside one of `skills`:
    /// a file beside the loaded `SKILL.md`, which is the reach `load_skill` offers. Never for a
    /// write, an edit or a listing, which resolve through [`Workspace::resolve`] and refuse the
    /// directory as before, since a file there is the user's own configuration and an instruction
    /// the next session trusts. Never where a settings layer kept the file tools inside the
    /// workspace (PERM-16). The directories are the driver's, named by a load that already
    /// happened, so nothing a file or a planner said widens this.
    pub(super) fn resolve_for_reading(
        &self,
        relative: &str,
        skills: &[PathBuf],
    ) -> Result<PathBuf, WorkspaceError> {
        let refused = match self.resolve(relative) {
            Ok(resolved)
                if Self::leaves_loaded_skill(relative, &resolved, skills, self.home.as_deref()) =>
            {
                WorkspaceError::Escapes {
                    path: relative.to_string(),
                    remedy: Remedy::Nothing,
                }
            }
            Ok(resolved) => return Ok(resolved),
            Err(refused) => refused,
        };
        if self.reads_stay_inside || skills.is_empty() {
            return Err(refused);
        }
        let Ok(expanded) = expand_home(relative, self.home.as_deref()) else {
            return Err(refused);
        };
        let candidate = expanded.as_deref().unwrap_or_else(|| Path::new(relative));
        if !candidate.is_absolute()
            || candidate
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(refused);
        }
        let Some(resolved) = destination(candidate) else {
            return Err(refused);
        };
        let inside = skills.iter().any(|directory| {
            destination(directory).is_some_and(|landed| resolved.starts_with(landed))
        });
        if inside { Ok(resolved) } else { Err(refused) }
    }

    /// Whether `named`, spelled inside the directory of a loaded skill, lands outside it once links
    /// are followed.
    ///
    /// The policy labels such a name by provenance, from its spelling alone (LOAD-5), so a link
    /// from the directory into one this workspace opened would hand a file the trust map has not
    /// vouched for to the planner as the user's own. `resolved` is where [`Workspace::resolve`]
    /// says the name lands, and it accepts any opened directory.
    fn leaves_loaded_skill(
        named: &str,
        resolved: &Path,
        skills: &[PathBuf],
        home: Option<&Path>,
    ) -> bool {
        let Ok(expanded) = expand_home(named, home) else {
            return false;
        };
        let candidate = expanded.as_deref().unwrap_or_else(|| Path::new(named));
        if !candidate.is_absolute()
            || candidate
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return false;
        }
        let mut spelled_in = skills
            .iter()
            .filter(|directory| candidate.starts_with(directory))
            .peekable();
        spelled_in.peek().is_some()
            && !spelled_in.any(|directory| {
                destination(directory).is_some_and(|landed| resolved.starts_with(landed))
            })
    }

    /// What a person can do so that `resolved`, which no opened directory holds, is reached.
    /// Inside the root a directory cannot be opened, and the relative path reaches the file.
    ///
    /// The directory to open is `resolved` itself where it is a directory, and the directory it is
    /// in for a file or for a name nothing is at yet. A directory that is or lies inside a credential
    /// location is refused by [`Workspace::resolve_directory`], so none is offered. The warning
    /// about checkouts is asked of that directory.
    fn remedy_outside(&self, resolved: &Path) -> Remedy {
        if resolved.starts_with(&self.root) {
            return Remedy::Inside;
        }
        if self.reads_stay_inside {
            return Remedy::Kept;
        }
        let itself = resolved.is_dir();
        let to_open = if itself {
            Some(resolved)
        } else {
            resolved.parent()
        };
        if let (Some(directory), Some(home)) = (to_open, self.home.as_deref())
            && bravebot_sandbox::rules::inside_a_credential_location(directory, home)
        {
            return if itself {
                Remedy::Nothing
            } else {
                Remedy::Credential
            };
        }
        let ends_checkouts = to_open.is_some_and(|directory| self.ends_checkouts(directory));
        match (itself, ends_checkouts) {
            (false, false) => Remedy::Open,
            (false, true) => Remedy::OpenEndsCheckouts,
            (true, false) => Remedy::OpenItself,
            (true, true) => Remedy::OpenItselfEndsCheckouts,
        }
    }

    /// Where a path with a `..` in it lands once each `..` is taken out, for a path that leaves
    /// the root: the absolute spelling its file is reached by, which is then resolved and confined
    /// as any other absolute path, so this decides how a name is spelled and nothing more.
    ///
    /// `None` for a path [`Workspace::climbed`] leaves alone, for one that lands inside the root,
    /// which the relative spelling reaches, and for one that lands on a name that is not UTF-8.
    /// The key, the rules and the result carry the absolute spelling as text, and a lossy one would
    /// name a different file.
    fn climb_lands(&self, candidate: &Path) -> Option<PathBuf> {
        let lands = self.climbed(candidate)?;
        (!lands.starts_with(&self.root) && lands.to_str().is_some()).then_some(lands)
    }

    /// `candidate`, against the root, with each `..` taken out of the text against the entry
    /// before it, which is what the kernel does with it wherever that entry is not a link.
    ///
    /// `None` for a path with no `..`, for one whose `..` leaves a link (where the kernel would
    /// follow the link and the text would not), and for one that climbs off the top of the file
    /// system.
    fn climbed(&self, candidate: &Path) -> Option<PathBuf> {
        if !candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return None;
        }
        let mut lands = PathBuf::new();
        for component in self.root.join(candidate).components() {
            match component {
                Component::ParentDir => {
                    let through_a_link = lands
                        .symlink_metadata()
                        .is_ok_and(|found| found.file_type().is_symlink());
                    if through_a_link || !lands.pop() {
                        return None;
                    }
                }
                Component::CurDir => {}
                other => lands.push(other.as_os_str()),
            }
        }
        Some(lands)
    }

    /// The refusal of a path with a `..` that [`Workspace::climb_lands`] does not take out.
    /// Nothing is offered, since no directory a person opens makes such a path reach its file.
    fn refuse_climb(named: &str) -> WorkspaceError {
        WorkspaceError::Escapes {
            path: named.to_string(),
            remedy: Remedy::Nothing,
        }
    }
}
