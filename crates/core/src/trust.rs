//! Which paths a user vouches for.
//!
//! The kernel decides *whether* a write needs a person's approval by asking two questions:
//! is the destination somewhere the user vouched for, and is the data trusted? A write of
//! trusted data to a trusted path is ordinary work and needs no interruption. Anything else
//! is shown to a person, because that is where a mistake cannot be undone by a later step.
//!
//! # Most specific rule wins
//!
//! Trust is recorded per path prefix, and the *longest* matching prefix decides. Both
//! polarities are expressible, so a trusted project may contain an untrusted subtree, and
//! that subtree may in turn contain a trusted path again:
//!
//! ```text
//!   .            (unset)
//!   src          trusted     → src/main.rs is trusted
//!   src/vendor   untrusted   → src/vendor/lib.js is untrusted
//!   src/vendor/ours.js trusted → trusted again, being more specific
//! ```
//!
//! # Why writing untrusted data marks the destination untrusted
//!
//! Without that rule the trust store would launder data. A turn could fetch a page, write
//! it into a trusted directory, read it back (now labelled trusted, because the path says
//! so) and use it for routing. Recording the destination as untrusted when untrusted bytes
//! land there closes the loop: what comes out of a file is never more trusted than what
//! went in.
//!
//! This crate performs no I/O; paths here are strings, compared by segment. The caller is
//! responsible for having resolved and confined them first.
//!
//! # One namespace of full paths
//!
//! Every rule is keyed under a full path. A caller may name a path relatively, which is how a
//! file in the project is named, and the store reads that under the working directory it was
//! given when it was made: `src/main.rs` in a session working in `/work` is the rule
//! `/work/src/main.rs`, and the working directory's own rule is `/work`. A name that is already
//! a full path is a key as it stands, which is how a directory opened by name is named.
//!
//! So there is no empty prefix to cover everything, and `/Users/x/notes` simply does not prefix
//! `/Users/x/proj`. One file has one rule whichever way it was spelled, and the longest matching
//! prefix decides among them all.

use crate::label::Integrity;
use std::collections::BTreeMap;
use std::path::Path;

/// A user's decisions about which paths are trustworthy.
///
/// Empty of rules when it is made, which means nothing is trusted and every write is shown to a
/// person. That is the right default: trust has to be granted, never assumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustStore {
    /// The working directory a relative name is read under, as a key.
    ///
    /// An absolute path, which is what [`TrustStore::new`] is given and what the workspace
    /// resolves once. A name that is not one keys this store's relative rules under itself
    /// consistently, and a rule about a directory opened by name does not reach them: wrong in
    /// the direction that trusts nothing.
    root: String,
    /// Full path → whether that subtree is trusted.
    ///
    /// A `BTreeMap` rather than a hash map so iteration order is deterministic, which keeps
    /// the audit trail reproducible.
    rules: BTreeMap<String, Integrity>,
}

impl TrustStore {
    /// A map with no rules, whose relative names mean paths under `root`.
    ///
    /// `root` is the working directory, absolute: the one `Workspace::new` resolves. It is not a
    /// rule and grants nothing, which is what keeps an empty map an empty map.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: normalise(&root.as_ref().to_string_lossy()),
            rules: BTreeMap::new(),
        }
    }

    /// The key `path` is held under: the full path it names.
    ///
    /// A name that is already absolute is one as it stands. A relative one is the working
    /// directory with the name beneath it, and the empty name is the working directory itself,
    /// which is the rule covering the whole project.
    pub(crate) fn key(&self, path: &str) -> String {
        let named = normalise(path);
        if is_absolute_key(&named) {
            return named;
        }
        match (self.root.as_str(), named.as_str()) {
            (_, "") => self.root.clone(),
            ("/", below) => format!("/{below}"),
            // A working directory that is not a path is not one a key can be built on, so the
            // names read under it stay as they were: they reach each other and nothing reaches
            // them, which is the direction that trusts nothing.
            ("", below) => below.to_string(),
            (root, below) => format!("{root}/{below}"),
        }
    }

    /// How a key is spelled back to a caller: relative where it is under the working directory,
    /// and the key itself where it is not.
    ///
    /// The inverse of [`TrustStore::key`], so a name this hands out reaches the rule it came
    /// from. That is what lets a rule be written down and read back (a session record keeps the
    /// relative form, which survives the project being moved) and shown to a person under the
    /// name they would type.
    fn named<'a>(&self, key: &'a str) -> &'a str {
        if key == self.root {
            return "";
        }
        // Nothing is under a working directory that is not a path, so every other key stands as
        // it is: stripping the empty string off one would turn a full path into a relative name
        // that no longer reaches it.
        if self.root.is_empty() {
            return key;
        }
        key.strip_prefix(self.root.as_str())
            .and_then(|below| below.strip_prefix('/'))
            .unwrap_or(key)
    }

    /// Record that `path` and everything beneath it is trusted.
    pub fn trust(&mut self, path: &str) {
        let key = self.key(path);
        self.rules.insert(key, Integrity::Trusted);
    }

    /// Record that `path` and everything beneath it is untrusted.
    ///
    /// Used both for an explicit user decision and for the automatic marking that happens
    /// when untrusted bytes are written somewhere.
    pub fn distrust(&mut self, path: &str) {
        let key = self.key(path);
        self.rules.insert(key, Integrity::Untrusted);
    }

    /// The integrity of `path`, by the longest matching rule.
    ///
    /// `None` when no rule covers it, which the caller should treat as untrusted. This
    /// returns an option rather than defaulting so a caller cannot silently confuse "the
    /// user vouched for this" with "nobody has said".
    pub fn integrity_of(&self, path: &str) -> Option<Integrity> {
        let path = self.key(path);
        self.rules
            .iter()
            .filter(|(prefix, _)| covers(prefix, &path))
            .max_by_key(|(prefix, _)| specificity(prefix))
            .map(|(_, integrity)| *integrity)
    }

    /// The integrity of everything at or beneath `path`, by the meet of every rule that bears on
    /// it.
    ///
    /// `integrity_of` answers about one file, by the longest rule covering it. A caller reading a
    /// whole subtree needs the weakest answer inside that subtree instead, because a trusted
    /// project may hold an untrusted directory and a result derived from both is only as good as
    /// the worse of them.
    ///
    /// `None` when no rule covers `path` at all, which the caller should treat as untrusted, for
    /// the same reason `integrity_of` returns an option: nobody has said.
    pub fn integrity_beneath(&self, path: &str) -> Option<Integrity> {
        let covered = self.integrity_of(path)?;
        Some(self.integrity_beneath_or(path, covered))
    }

    /// [`TrustStore::integrity_beneath`], answering `assumed` where no rule covers `path`.
    ///
    /// For a directory something else made reachable without a rule being written about it: the
    /// caller says what an uncovered path there answers as. The rules recorded inside it still
    /// weaken the answer, so a file a write marked untrusted is not laundered by a read of the tree
    /// around it.
    pub fn integrity_beneath_or(&self, path: &str, assumed: Integrity) -> Integrity {
        let mut answer = self.integrity_of(path).unwrap_or(assumed);
        let path = self.key(path);
        for (prefix, integrity) in self.keyed() {
            if covers(&path, prefix) {
                answer = answer.meet(integrity);
            }
        }
        answer
    }

    /// Whether `path` is trusted. Anything not covered by a rule is not.
    pub fn is_trusted(&self, path: &str) -> bool {
        self.integrity_of(path) == Some(Integrity::Trusted)
    }

    /// Whether any path has been vouched for at all.
    ///
    /// Distinguishes "the user declined" from "the user trusted nothing yet", which callers
    /// report differently.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The rules, most general first, for display and for the audit trail.
    ///
    /// Each spelled the way a caller names that path: relative to the working directory for a
    /// rule inside it, and the full path for one outside. That is the name a person would type
    /// and the name a session record keeps, which is what lets a record survive the project
    /// being moved or renamed.
    pub fn rules(&self) -> impl Iterator<Item = (&str, Integrity)> {
        self.rules
            .iter()
            .map(|(key, integrity)| (self.named(key), *integrity))
    }

    /// The same rules under the keys the map holds them by: full paths, most general first.
    ///
    /// What [`TrustStore::rules`] hands out depends on the working directory, so it is the wrong
    /// thing to copy a rule between two maps with: a relative name re-read under a different
    /// working directory is a rule about a different file. A key means the same file everywhere,
    /// and [`TrustStore::trust`] takes one as readily as a relative name.
    pub fn keyed(&self) -> impl Iterator<Item = (&str, Integrity)> {
        self.rules
            .iter()
            .map(|(key, integrity)| (key.as_str(), *integrity))
    }

    /// The same decisions, said about the same files, once the working directory has moved.
    ///
    /// A rule is keyed on the full path it is about, so a working directory that moves takes
    /// nothing with it: every rule still names the file it always named, and what changes is only
    /// which of them a relative name reaches and how each is spelled back. Nothing is granted and
    /// nothing is withdrawn, which is what makes this a re-spelling rather than a decision.
    ///
    /// `to` is an absolute path, and the caller is responsible for having canonicalised it: a
    /// second spelling of one directory would read the rules written under the first as though
    /// they were about somewhere else.
    pub fn rebased(&self, to: &Path) -> Self {
        Self {
            root: normalise(&to.to_string_lossy()),
            rules: self.rules.clone(),
        }
    }
}

/// Whether a name is a full path rather than one read under a working directory.
///
/// A leading `/` is the whole of the test, and a name spelled any other way is relative. A drive
/// letter is not a root here and a backslash is not a separator: a key arrives spelled from `/`,
/// and a backslash is a legal filename byte where paths are, so a file called `C:\notes` is a file
/// in the project and reading its name as a root would take it out of the project's rules.
/// Resolving a platform's path into a key therefore belongs outside this crate, which does no
/// filesystem work, and the workspace refuses to open a directory whose name is not one rather
/// than handing over a name that would be read under the working directory (TRUST-18).
///
/// Permission patterns keep the two apart as namespaces of their own, which is a rule about what
/// a person wrote rather than about what this map holds (PERM-3).
pub fn is_absolute_key(key: &str) -> bool {
    key.starts_with('/')
}

/// Normalise a path for comparison.
///
/// Every `.` segment and every empty one goes, so `./src/`, `src`, `src/`, `./src/./` and
/// `src//` are one rule rather than five that shadow each other. Anything less is a bypass, not
/// an inconvenience: `Workspace::resolve` accepts a `.` component and opens the same file, so a
/// key that kept one would leave a per-file untrusted rule the planner could spell past, and the
/// page a turn fetched into that file would be read back as trusted.
///
/// A `..` segment is kept as written rather than resolved. Resolving one lexically would be a
/// guess: nothing here has a filesystem, and `a/../b` names `b` only if `a` is a directory and
/// not a symlink somewhere else. A confined path never contains one, since confinement refuses
/// `..` rather than resolving it (TRUST-10), so what this leaves unmerged is the spelling a
/// dropped file arrives under.
///
/// An absolute path **keeps** its leading slash, which is what makes it a key already rather than
/// a name to be read under the working directory.
pub(crate) fn normalise(path: &str) -> String {
    let trimmed = path.trim();
    let segments = trimmed
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>()
        .join("/");
    match is_absolute_key(trimmed) {
        true => format!("/{segments}"),
        false => segments,
    }
}

/// Whether `prefix` covers `path`, matching whole segments only.
///
/// Segment-wise so `src` does not cover `srcfoo`, which a plain string prefix test would
/// wrongly accept, and that mistake would hand trust to a path the user never named.
pub(crate) fn covers(prefix: &str, path: &str) -> bool {
    // The filesystem root covers everything, and is the one key with no segment of its own.
    if prefix == "/" {
        return true;
    }
    // The empty key is not a second one of those. It is what a map made against something that is
    // not a path keys its own root under, and a full path is never under such a root.
    if prefix.is_empty() {
        return !is_absolute_key(path);
    }
    if path == prefix {
        return true;
    }
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// How specific a rule is. Deeper rules win; the filesystem root is least specific.
fn specificity(prefix: &str) -> usize {
    if prefix == "/" || prefix.is_empty() {
        0
    } else {
        prefix.trim_start_matches('/').split('/').count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_store_trusts_nothing() {
        let store = TrustStore::new("/work");
        assert!(store.is_empty());
        assert_eq!(store.integrity_of("src/main.rs"), None);
        assert!(!store.is_trusted("src/main.rs"));
    }

    #[test]
    fn trusting_a_directory_covers_what_is_beneath_it() {
        let mut store = TrustStore::new("/work");
        store.trust("src");
        assert!(store.is_trusted("src"));
        assert!(store.is_trusted("src/main.rs"));
        assert!(store.is_trusted("src/deep/nested.rs"));
        assert!(!store.is_trusted("other/main.rs"));
    }

    /// A search that walks a whole tree is only as trustworthy as the weakest thing in it, so an
    /// untrusted directory inside a trusted project has to show up in the answer about the project.
    #[test]
    fn a_subtree_is_trusted_only_when_nothing_beneath_it_is_not() {
        let mut store = TrustStore::new("/work");
        store.trust("src");
        store.distrust("src/vendor");

        assert_eq!(store.integrity_beneath("src"), Some(Integrity::Untrusted));
        assert_eq!(
            store.integrity_beneath("src/handlers"),
            Some(Integrity::Trusted)
        );
        assert_eq!(
            store.integrity_beneath("src/vendor"),
            Some(Integrity::Untrusted)
        );
        assert_eq!(store.integrity_beneath("elsewhere"), None);
    }

    /// One file's own answer must not be weakened by a rule about a sibling, or vouching for a
    /// project would be undone by any one directory inside it.
    #[test]
    fn a_rule_about_a_sibling_does_not_reach_a_file() {
        let mut store = TrustStore::new("/work");
        store.trust(".");
        store.distrust("vendor");

        assert_eq!(
            store.integrity_beneath("src/main.rs"),
            Some(Integrity::Trusted)
        );
        assert_eq!(store.integrity_beneath("."), Some(Integrity::Untrusted));
    }

    /// Trusting the workspace root is the startup case, so it must cover everything.
    #[test]
    fn trusting_the_root_covers_the_whole_tree() {
        let mut store = TrustStore::new("/work");
        store.trust(".");
        assert!(store.is_trusted("anything/at/all.rs"));
        assert!(store.is_trusted("top.rs"));
    }

    /// The rule the design turns on: a more specific decision overrides a broader one.
    #[test]
    fn an_untrusted_subpath_overrides_a_trusted_parent() {
        let mut store = TrustStore::new("/work");
        store.trust(".");
        store.distrust("vendor");

        assert!(store.is_trusted("src/main.rs"));
        assert!(!store.is_trusted("vendor/lib.js"));
        assert!(!store.is_trusted("vendor"));
    }

    /// And the reverse, which the user asked for explicitly: a trusted path inside an
    /// untrusted directory.
    #[test]
    fn a_trusted_subpath_overrides_an_untrusted_parent() {
        let mut store = TrustStore::new("/work");
        store.distrust("vendor");
        store.trust("vendor/ours");

        assert!(!store.is_trusted("vendor/lib.js"));
        assert!(store.is_trusted("vendor/ours/code.js"));
        assert!(store.is_trusted("vendor/ours"));
    }

    /// Specificity must keep working to arbitrary depth, alternating polarity.
    #[test]
    fn the_deepest_rule_wins_at_any_depth() {
        let mut store = TrustStore::new("/work");
        store.trust(".");
        store.distrust("a");
        store.trust("a/b");
        store.distrust("a/b/c");
        store.trust("a/b/c/d");

        assert!(store.is_trusted("z.rs"));
        assert!(!store.is_trusted("a/x.rs"));
        assert!(store.is_trusted("a/b/x.rs"));
        assert!(!store.is_trusted("a/b/c/x.rs"));
        assert!(store.is_trusted("a/b/c/d/x.rs"));
    }

    /// A prefix must match whole segments: treating `src` as covering `srcfoo` would grant
    /// trust to a path the user never named.
    #[test]
    fn a_rule_matches_whole_segments_only() {
        let mut store = TrustStore::new("/work");
        store.trust("src");
        assert!(!store.is_trusted("srcfoo/main.rs"));
        assert!(!store.is_trusted("srcfoo"));
        assert!(store.is_trusted("src/foo"));
    }

    /// Equivalent spellings of a path must be one rule, or a user could trust `./src` and
    /// be surprised that `src` is still untrusted.
    #[test]
    fn equivalent_path_spellings_are_the_same_rule() {
        let mut store = TrustStore::new("/work");
        store.trust("./src/");
        assert!(store.is_trusted("src/main.rs"));

        store.distrust("src");
        assert!(
            !store.is_trusted("src/main.rs"),
            "a differently spelled path became a second rule"
        );
    }

    /// The laundering round trip this closes: a per-file untrusted rule the planner can spell
    /// past is no rule at all, since `Workspace::resolve` accepts a `.` component and a repeated
    /// separator and opens the very same file. Every spelling of that file has to reach the rule
    /// the write recorded, not the workspace root rule above it.
    #[test]
    fn every_equivalent_spelling_of_a_path_reaches_the_same_rule() {
        let mut store = TrustStore::new("/work");
        store.trust(".");
        store.distrust("src/fetched.json");

        for spelling in [
            "src/fetched.json",
            "./src/fetched.json",
            "src/./fetched.json",
            "src//fetched.json",
            "./src/./fetched.json",
            "src/.//fetched.json",
        ] {
            assert_eq!(
                store.integrity_of(spelling),
                Some(Integrity::Untrusted),
                "{spelling} missed the rule, so the workspace root rule decided instead"
            );
        }
    }

    /// Re-deciding must replace the earlier decision rather than accumulating rules whose
    /// resolution depends on insertion order.
    #[test]
    fn a_later_decision_replaces_an_earlier_one() {
        let mut store = TrustStore::new("/work");
        store.trust("src");
        assert!(store.is_trusted("src/main.rs"));
        store.distrust("src");
        assert!(!store.is_trusted("src/main.rs"));
        store.trust("src");
        assert!(store.is_trusted("src/main.rs"));
    }

    #[test]
    fn integrity_of_reports_which_decision_applied() {
        let mut store = TrustStore::new("/work");
        store.trust(".");
        store.distrust("vendor");

        assert_eq!(store.integrity_of("src/a.rs"), Some(Integrity::Trusted));
        assert_eq!(
            store.integrity_of("vendor/a.js"),
            Some(Integrity::Untrusted)
        );
    }

    /// With no root rule, an unmentioned path stays unknown rather than inheriting from an
    /// unrelated sibling rule.
    #[test]
    fn an_unrelated_path_is_unaffected_by_other_rules() {
        let mut store = TrustStore::new("/work");
        store.trust("src");
        assert_eq!(store.integrity_of("docs/readme.md"), None);
    }

    /// A rule about a directory opened by name decides nothing inside the project. Two files may
    /// be called `src/main.rs`, and the full path each rule is keyed under is what says which one
    /// it means: `Users/me/notes/todo.md` in the project is `/work/Users/me/notes/todo.md`, which
    /// the rule about `/Users/me/notes` does not prefix.
    #[test]
    fn a_rule_outside_the_working_directory_decides_nothing_inside_it() {
        let mut store = TrustStore::new("/work");
        store.trust("/Users/me/notes");

        assert!(store.is_trusted("/Users/me/notes/todo.md"));
        assert_eq!(
            store.integrity_of("Users/me/notes/todo.md"),
            None,
            "a rule outside the project leaked into it"
        );
        assert_eq!(store.integrity_of("src/main.rs"), None);
    }

    /// And the reverse: vouching for the workspace must not vouch for a directory outside it. The
    /// working directory's own rule is its full path, not an empty prefix, so there is nothing for
    /// a path elsewhere to fall under.
    #[test]
    fn trusting_the_workspace_says_nothing_about_an_added_directory() {
        let mut store = TrustStore::new("/work");
        store.trust(".");

        assert!(store.is_trusted("src/main.rs"));
        assert_eq!(
            store.integrity_of("/Users/me/notes/todo.md"),
            None,
            "the workspace rule reached outside it"
        );
    }

    /// One file has one rule however it was named. A project file spelled in full reaches the rule
    /// its relative name wrote, which is what keeps content written as untrusted under one
    /// spelling from being read back as trusted under the other.
    #[test]
    fn a_path_named_in_full_reaches_the_rule_its_relative_name_wrote() {
        let mut store = TrustStore::new("/work");
        store.trust(".");
        store.distrust("src/fetched.json");

        assert!(store.is_trusted("/work/src/main.rs"));
        assert_eq!(
            store.integrity_of("/work/src/fetched.json"),
            Some(Integrity::Untrusted),
            "the full name missed the rule its relative spelling wrote"
        );
        assert_eq!(store.integrity_of("/work"), Some(Integrity::Trusted));
    }

    /// A person who vouches for a directory holding the project has vouched for the project's
    /// files too, since the rule is about the tree and the project is in it. The project's own
    /// rules are more specific and still decide where they exist.
    #[test]
    fn a_directory_vouched_for_above_the_project_covers_the_project() {
        let mut store = TrustStore::new("/Users/me/proj");
        store.trust("/Users/me");
        store.distrust("vendor");

        assert!(store.is_trusted("src/main.rs"));
        assert!(store.is_trusted("/Users/me/notes/todo.md"));
        assert_eq!(
            store.integrity_of("vendor/lib.js"),
            Some(Integrity::Untrusted),
            "a broader answer overrode the project's own rule"
        );
    }

    /// Whether a name is a key already is decided by a leading slash and by nothing else. A key
    /// arrives spelled from `/`, and a backslash is a legal filename byte where paths are, so a
    /// name carrying a drive letter is a path under the project and the project's own rules are
    /// the ones that decide it. Reading such a name as a full path instead would take a file the
    /// project holds out of the rules written about it.
    #[test]
    fn a_name_that_is_a_root_on_another_platform_is_read_under_the_working_directory() {
        let mut store = TrustStore::new("/work");
        store.distrust("C:\\notes");

        assert!(!is_absolute_key("C:\\notes"));
        assert_eq!(normalise("C:\\notes"), "C:\\notes");
        assert_eq!(
            store.integrity_of("C:\\notes"),
            Some(Integrity::Untrusted),
            "a file in the project stopped being decided by a rule about it"
        );
        assert_eq!(
            store.integrity_of("/C:\\notes"),
            None,
            "a name read under the working directory reached a rule keyed in full"
        );
    }

    /// `/` is the one key with no segment of its own, and it covers every path there is,
    /// including the working directory's. Somebody who vouches for the filesystem root has
    /// vouched for everything in it, and the project is in it.
    #[test]
    fn trusting_the_filesystem_root_covers_the_working_directory_too() {
        let mut store = TrustStore::new("/work");
        store.trust("/");

        assert!(store.is_trusted("/etc/hosts"));
        assert!(store.is_trusted("src/main.rs"));
        // Still the least specific rule there is, so anything written about a path wins over it.
        store.distrust("src/vendor");
        assert!(!store.is_trusted("src/vendor/lib.js"));
    }

    /// Most specific still wins outside the project, so an added directory can hold an untrusted
    /// subtree exactly as the workspace can.
    #[test]
    fn the_deepest_absolute_rule_wins() {
        let mut store = TrustStore::new("/work");
        store.trust("/Users/me/notes");
        store.distrust("/Users/me/notes/clipped");

        assert!(store.is_trusted("/Users/me/notes/todo.md"));
        assert!(!store.is_trusted("/Users/me/notes/clipped/from-the-web.md"));
    }

    /// Two added directories are separate rules, so one does not answer for the other.
    #[test]
    fn one_added_directory_does_not_cover_a_sibling() {
        let mut store = TrustStore::new("/work");
        store.trust("/Users/me/notes");

        assert!(store.is_trusted("/Users/me/notes/todo.md"));
        assert_eq!(store.integrity_of("/Users/me/other/todo.md"), None);
        // Whole segments only, here too: `notes` must not cover `notes-backup`.
        assert_eq!(store.integrity_of("/Users/me/notes-backup/todo.md"), None);
    }

    /// A trailing slash is the same rule, as it is for a relative path.
    #[test]
    fn equivalent_absolute_spellings_are_the_same_rule() {
        let mut store = TrustStore::new("/work");
        store.trust("/Users/me/notes/");
        assert!(store.is_trusted("/Users/me/notes/todo.md"));

        store.distrust("/Users/me/notes");
        assert!(
            !store.is_trusted("/Users/me/notes/todo.md"),
            "a differently spelled path became a second rule"
        );
    }

    /// The interior spellings too, outside the project as inside it. An added directory holds
    /// files worth distrusting one at a time (TRUST-9), so a spelling that misses a per-file rule
    /// launders content here exactly as it does in the workspace.
    #[test]
    fn every_equivalent_absolute_spelling_reaches_the_same_rule() {
        let mut store = TrustStore::new("/work");
        store.trust("/Users/me/notes");
        store.distrust("/Users/me/notes/clipped.md");

        for spelling in [
            "/Users/me/notes/clipped.md",
            "/Users/me/./notes/clipped.md",
            "/Users/me/notes//clipped.md",
            "/Users/me/notes/./clipped.md",
        ] {
            assert_eq!(
                store.integrity_of(spelling),
                Some(Integrity::Untrusted),
                "{spelling} missed the rule, so the added directory's own rule decided instead"
            );
        }
    }

    /// A map is made against the working directory, which the workspace resolves to an absolute
    /// path before anything reaches here. A record hand-edited to name something else must not
    /// widen what the map covers: the names read under it reach each other and nothing else
    /// reaches them, so a yes given under such a map vouches for nothing a path can name.
    #[test]
    fn a_map_made_against_something_that_is_not_a_path_reaches_nothing() {
        let mut store = TrustStore::new("");
        store.trust(".");

        assert!(
            store.is_trusted("src/main.rs"),
            "the map lost its own names"
        );
        assert_eq!(store.integrity_of("/etc/hosts"), None);
        assert_eq!(store.integrity_of("/"), None);
    }

    /// Moving the working directory must not carry the yes given for one project into another. The
    /// rule is still about the project it was given for, which is now somewhere else, so that is
    /// where it is written down.
    #[test]
    fn a_yes_for_one_project_does_not_follow_a_move_to_another() {
        let mut store = TrustStore::new("/work");
        store.trust(".");

        let moved = store.rebased(Path::new("/other"));
        assert_eq!(
            moved.integrity_of("src/main.rs"),
            None,
            "the answer given for /work vouched for /other"
        );
        assert!(moved.is_trusted("/work/src/main.rs"));
    }

    /// And a no given inside the project is not forgotten by moving into it. `vendor` was
    /// untrusted before the move and names the same files after it, so it stays untrusted.
    #[test]
    fn a_no_inside_the_new_directory_survives_the_move() {
        let mut store = TrustStore::new("/work");
        store.trust(".");
        store.distrust("src/vendor");

        let moved = store.rebased(Path::new("/work/src"));
        assert_eq!(
            moved.integrity_of("vendor/lib.js"),
            Some(Integrity::Untrusted),
            "an untrusted subtree became trusted by moving into its parent"
        );
    }

    /// The other direction: a rule about a directory the user added by name becomes an ordinary
    /// relative rule once that directory is the working directory.
    #[test]
    fn a_rule_in_an_added_directory_becomes_relative_when_it_is_moved_into() {
        let mut store = TrustStore::new("/work");
        store.trust("/Users/me/notes");
        store.distrust("/Users/me/notes/private");

        let moved = store.rebased(Path::new("/Users/me/notes"));
        assert!(moved.is_trusted("todo.md"));
        assert_eq!(
            moved.integrity_of("private/diary.md"),
            Some(Integrity::Untrusted)
        );
    }

    /// Whole segments decide what is inside the new directory, exactly as they decide what a rule
    /// covers. A prefix test on the characters would move `srcfoo` in as though it were `src`.
    #[test]
    fn a_sibling_with_a_longer_name_is_not_taken_to_be_inside() {
        let mut store = TrustStore::new("/work");
        store.distrust("srcfoo");

        let moved = store.rebased(Path::new("/work/src"));
        assert_eq!(
            moved.integrity_of("foo"),
            None,
            "a sibling directory was rebased as though it were inside the new one"
        );
        assert_eq!(
            moved.integrity_of("/work/srcfoo"),
            Some(Integrity::Untrusted)
        );
    }

    /// Rebasing decides nothing. Every rule that went in comes out, saying the same thing about
    /// the same files, which is what makes it safe to do without asking anybody.
    #[test]
    fn rebasing_neither_grants_nor_withdraws_anything() {
        let mut store = TrustStore::new("/work");
        store.trust(".");
        store.distrust("vendor");
        store.trust("/Users/me/notes");

        let moved = store.rebased(Path::new("/other"));
        assert_eq!(moved.rules().count(), 3);
        assert!(moved.is_trusted("/work/src/main.rs"));
        assert_eq!(
            moved.integrity_of("/work/vendor/lib.js"),
            Some(Integrity::Untrusted)
        );
        assert!(moved.is_trusted("/Users/me/notes/todo.md"));
    }
}
