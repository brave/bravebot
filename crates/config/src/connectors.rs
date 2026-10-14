//! Installed connector definitions, the part of each that this process reads.
//!
//! A definition is one file per service under `connectors/` in the person's own state
//! directory, and in no settings layer a checkout may carry: a checkout that could install one
//! could choose which program receives the text a run sends. The file's name is the connector's
//! name, because a grant is written against it and so it has to be something a person chose.
//!
//! The file holds the connector's `name`, `definition_id`, `authority`, `item` and `surface`, and
//! nothing else: the wire part that says how to call the service is in the authority's own copy of
//! the definition, and a file carrying any other field is refused rather than read in part. The
//! `definition_id` is the SHA-256 the authority's install script computed over the canonical
//! definition, without its client id, client secret and API key. This process does not compute it:
//! it checks its form and sends it on every call, so the authority refuses a call made against a
//! copy other than the one it holds.

pub mod forms;

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Which of the two connector steps a resource belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepKind {
    Fetch,
    Act,
}

/// One resource a connector offers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub step: StepKind,
    /// The fields the resource takes, each required unless it has a default. `a|b` means exactly
    /// one of the two.
    pub uses: Vec<String>,
    /// One line for the planner.
    pub summary: String,
    /// The selector terms a fetch accepts. A trailing `*` stands for any value.
    #[serde(default)]
    pub select: Vec<String>,
    /// For an act naming a resolve in `on`: the selector terms that resolve must use at least one
    /// of, unless it names its item, so the act does not land on whichever item sorted first. For a
    /// fetch: the terms its own `select` must use at least one of.
    #[serde(default)]
    pub narrow: Vec<String>,
    /// For an act whose option is a key here: the selector terms the fetch named in `on`, or the
    /// act's own `select`, must use one of, in place of `narrow`'s; a fetch that named its item
    /// needs none.
    #[serde(default)]
    pub narrow_when: BTreeMap<String, Vec<String>>,
    /// The form each field's value must have, overriding the surface's.
    #[serde(default)]
    pub forms: BTreeMap<String, String>,
    /// The only values each field may take, overriding the surface's.
    #[serde(default)]
    pub values: BTreeMap<String, Vec<String>>,
    /// Whether a fetch of this resource returns a receipt an act may name in `on`.
    #[serde(default)]
    pub receipt: bool,
    /// The values a step gives in `with`, each by name with the form it must have, such as an
    /// event's `start` as a `datetime`. A step that takes `with` gives every one of them.
    #[serde(default)]
    pub with: BTreeMap<String, WithRule>,
    /// Values the authority takes from the item an act lands on, each with the words the plan
    /// line uses for it. The plan never gives them, and the authority refuses a request that does.
    #[serde(default)]
    pub bound: BTreeMap<String, String>,
    /// On a fetch with a receipt, the fields it records that an act in the run may take, each with
    /// the words the planner reads, such as a flight's departure.
    #[serde(default)]
    pub records: BTreeMap<String, String>,
    /// On an act, the `with` values a plan may take in `take`, from a field a fetch earlier in the
    /// run recorded, in place of giving them.
    #[serde(default)]
    pub takes: Vec<String>,
    /// Whether an act carries text the plan wrote or released; one that does not says what it
    /// does by its option alone.
    #[serde(default = "yes")]
    pub text: bool,
    /// An act whose effect reaches further than its text, such as approving a pull request or
    /// pushing code; the plan line says so before the person approves.
    #[serde(default)]
    pub high_impact: bool,
}

/// What a `with` value must be: of a form, or one of a list of values, such as a traveller's
/// title.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum WithRule {
    Form(String),
    OneOf(Vec<String>),
}

impl WithRule {
    /// Whether `value` keeps the rule.
    pub fn check(&self, value: &str) -> Result<(), String> {
        match self {
            WithRule::Form(form) => forms::check(form, value),
            WithRule::OneOf(list) if list.iter().any(|v| v == value) => Ok(()),
            WithRule::OneOf(list) => Err(format!("is not one of {}", list.join(", "))),
        }
    }

    /// The form the rule names, where it names one.
    pub fn form(&self) -> Option<&str> {
        match self {
            WithRule::Form(form) => Some(form),
            WithRule::OneOf(_) => None,
        }
    }
}

fn yes() -> bool {
    true
}

impl Resource {
    /// Each entry of `uses` as the fields it allows, one for a plain field and two for `a|b`.
    pub fn alternatives(&self) -> impl Iterator<Item = Vec<&str>> {
        self.uses.iter().map(|entry| entry.split('|').collect())
    }

    /// Whether `field` is one of the fields the resource takes, alone or as half of an either-or.
    pub fn uses_field(&self, field: &str) -> bool {
        self.alternatives().any(|fields| fields.contains(&field))
    }
}

/// What this process knows about one connector.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Surface {
    pub resources: BTreeMap<String, Resource>,
    /// The only values a field may take, where the definition limits them.
    #[serde(default)]
    pub values: BTreeMap<String, Vec<String>>,
    /// A value an omitted field takes, locked and shown as if the plan had given it.
    #[serde(default)]
    pub defaults: BTreeMap<String, String>,
    /// The named form each field's value must have.
    #[serde(default)]
    pub forms: BTreeMap<String, String>,
    /// A worked example per resource, for the planner.
    #[serde(default)]
    pub examples: BTreeMap<String, String>,
}

/// One installed connector, as its file holds it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connector {
    pub name: String,
    /// The definition's id, 64 lower-case hex characters, as the install script wrote it. Read
    /// with a default so a copy without one is refused with a reason rather than serde's words.
    #[serde(default)]
    pub definition_id: String,
    /// The authority program, an absolute path.
    pub authority: PathBuf,
    /// The credential item in the authority this connector uses.
    pub item: String,
    pub surface: Surface,
}

/// Every definition under `connectors/` in `home`, by name.
///
/// A file that cannot be read is kept as its error rather than dropped, so a plan naming that
/// connector is refused with the reason and a plan naming another one is not affected.
pub fn installed(home: &Path) -> BTreeMap<String, Result<Connector, String>> {
    let mut found = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(home.join("connectors")) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let read = read_one(home, &path, &name);
        found.insert(name, read);
    }
    found
}

/// Refuse a definition that somebody other than the person could have written: the file names the
/// program that is handed a write's text, so a file or folder another account owns, one open to the
/// group or the world, or a link that could point anywhere is a way to swap that program. The
/// person is whoever owns the state directory, which this process made for them. Anything whose
/// owner or mode cannot be read is refused as an open one.
#[cfg(unix)]
fn owner_only_writable(home: &Path, path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let owner = std::fs::metadata(home).map(|m| m.uid()).ok();
    let open = |p: &Path| {
        let found = std::fs::symlink_metadata(p)
            .ok()
            .map(|m| (m.mode(), m.uid(), m.file_type().is_symlink()));
        open_to_others(found, owner)
    };
    if open(path) || path.parent().is_some_and(open) {
        return Err(format!(
            "{} or its folder is a link, belongs to another account, can be written by others or \
             could not be checked; make them the person's own, writable by them only",
            path.display()
        ));
    }
    Ok(())
}

/// Whether a file `found` as its mode, its owner and whether it is a link is open to anyone but
/// `owner`: a link, another account's, or writable by the group or the world. A file or an owner
/// that could not be read is open.
#[cfg(unix)]
fn open_to_others(found: Option<(u32, u32, bool)>, owner: Option<u32>) -> bool {
    match (found, owner) {
        (Some((mode, uid, link)), Some(owner)) => link || uid != owner || mode & 0o022 != 0,
        _ => true,
    }
}

/// Refuse an authority program that anyone but the person or the system could replace: it is
/// handed a write's text, so the program and the folder it is in, followed through any link, and
/// the folder the definition names it in, must each be owned by the person or by root and
/// writable by nobody else. A program that does not exist is refused too.
#[cfg(unix)]
fn authority_closed(home: &Path, authority: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let owner = std::fs::metadata(home).map(|m| m.uid()).ok();
    let real = std::fs::canonicalize(authority).map_err(|e| {
        format!(
            "the authority '{}' cannot be read: {e}",
            authority.display()
        )
    })?;
    let closed = |p: &Path| {
        std::fs::metadata(p)
            .is_ok_and(|m| (Some(m.uid()) == owner || m.uid() == 0) && m.mode() & 0o022 == 0)
    };
    let places = [Some(real.as_path()), real.parent(), authority.parent()];
    if places.into_iter().flatten().all(closed) {
        Ok(())
    } else {
        Err(format!(
            "the authority '{}' or its folder belongs to another account or can be written by \
             others; make them the person's own or the system's, writable by their owner only",
            authority.display()
        ))
    }
}

/// Off Unix every definition is refused before its authority is looked at.
#[cfg(not(unix))]
fn authority_closed(_home: &Path, _authority: &Path) -> Result<(), String> {
    Ok(())
}

/// Off Unix there is no mode to read, so no definition can be shown to be its owner's alone, and
/// every one is refused.
#[cfg(not(unix))]
fn owner_only_writable(_home: &Path, path: &Path) -> Result<(), String> {
    Err(format!(
        "{}: connectors need the owner-only file check this platform does not have",
        path.display()
    ))
}

/// Refuse a surface that names a form [`forms`] does not define, for the surface or for one
/// resource.
fn forms_known(surface: &Surface) -> Result<(), String> {
    let named = surface
        .forms
        .values()
        .map(|form| (None, form.as_str()))
        .chain(surface.resources.iter().flat_map(|(resource, r)| {
            r.forms
                .values()
                .map(String::as_str)
                .chain(r.with.values().filter_map(WithRule::form))
                .map(move |form| (Some(resource), form))
        }));
    for (resource, form) in named {
        if !forms::known(form) {
            return Err(match resource {
                Some(resource) => {
                    format!("the resource '{resource}' names a form nothing defines, '{form}'")
                }
                None => format!("the surface names a form nothing defines, '{form}'"),
            });
        }
    }
    Ok(())
}

/// Refuse a fetch that has to narrow by a term its own selector does not offer, which no plan could
/// meet, or that takes no selector to narrow with.
fn narrow_offered(surface: &Surface) -> Result<(), String> {
    let fetches = surface
        .resources
        .iter()
        .filter(|(_, r)| r.step == StepKind::Fetch && !r.narrow.is_empty());
    for (name, resource) in fetches {
        if !resource.uses_field("select") {
            return Err(format!(
                "the resource '{name}' has to narrow and takes no select"
            ));
        }
        for term in &resource.narrow {
            let offered = resource.select.iter().any(|pattern| {
                pattern == term
                    || pattern
                        .strip_suffix('*')
                        .is_some_and(|prefix| prefix.contains(':') && term.starts_with(prefix))
            });
            if !offered {
                return Err(format!(
                    "the resource '{name}' has to narrow by '{term}', which its select does not offer"
                ));
            }
        }
    }
    Ok(())
}

/// Names a `with` value may not take, because a call's template already uses them for the
/// step's own fields or the engine's.
const NOT_WITH: [&str; 5] = ["place", "item", "path", "base", "id"];

/// Refuse a resource whose `with` values and `uses` disagree, or that names one a selector key
/// could not spell or a template already uses, so every declared value can be given and none
/// stands in for another field.
fn with_declared(surface: &Surface) -> Result<(), String> {
    for (name, resource) in &surface.resources {
        if resource.with.is_empty() == resource.uses_field("with") {
            return Err(format!(
                "the resource '{name}' must declare its with values exactly when it uses with"
            ));
        }
        if resource
            .with
            .values()
            .any(|r| r == &WithRule::OneOf(Vec::new()))
        {
            return Err(format!(
                "the resource '{name}' offers a with value that can be nothing"
            ));
        }
        for key in resource.with.keys() {
            let spelled = !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
            if !spelled || NOT_WITH.contains(&key.as_str()) || resource.bound.contains_key(key) {
                return Err(format!(
                    "the resource '{name}' cannot take a with value named '{key}'"
                ));
            }
        }
        if !resource.records.is_empty() && !(resource.step == StepKind::Fetch && resource.receipt) {
            return Err(format!(
                "the resource '{name}' records fields only as a fetch with a receipt"
            ));
        }
        if !resource.takes.is_empty()
            && !(resource.step == StepKind::Act
                && resource.takes.iter().all(|k| resource.with.contains_key(k)))
        {
            return Err(format!(
                "the resource '{name}' takes only with values it declares, as an act"
            ));
        }
    }
    Ok(())
}

/// The rules the credential authority holds a surface to when it loads the definition, held here
/// too, so a plan is never made against a surface the authority would refuse.
fn surface_rules(surface: &Surface) -> Result<(), String> {
    if let Some(key) = surface.defaults.keys().find(|k| k.contains('|')) {
        return Err(format!(
            "the default '{key}' names an either-or; a default names one field"
        ));
    }
    for (name, resource) in &surface.resources {
        let act = resource.step == StepKind::Act;
        let values = |field: &str| {
            resource
                .values
                .get(field)
                .or_else(|| surface.values.get(field))
        };
        if resource.uses_field("option") && values("option").is_none() {
            return Err(format!(
                "the resource '{name}' takes an option and lists no values for it"
            ));
        }
        if resource
            .alternatives()
            .any(|g| g.len() > 1 && g.contains(&"with"))
        {
            return Err(format!(
                "the resource '{name}' offers with as one of an either-or"
            ));
        }
        let act_only = resource.high_impact
            || !resource.text
            || !resource.narrow_when.is_empty()
            || !resource.bound.is_empty();
        if !act && act_only {
            return Err(format!(
                "the resource '{name}' is a fetch, and only an act is high-impact, takes no text, narrows by option or binds values"
            ));
        }
        if !resource.bound.is_empty() && !resource.uses_field("on") {
            return Err(format!(
                "the resource '{name}' binds values and takes no 'on' to name the item"
            ));
        }
        let offered = values("option").map(Vec::as_slice).unwrap_or_default();
        if let Some(option) = resource.narrow_when.keys().find(|o| !offered.contains(o)) {
            return Err(format!(
                "the resource '{name}' narrows by '{option}', which it does not offer"
            ));
        }
    }
    Ok(())
}

/// What the authority accepts as a connector's name and its credential item, held here too: a name
/// of lower-case letters, digits, `_` and `-`, and an item `svc:<name>:<account>`.
fn name_and_item(file: &Connector) -> Result<(), String> {
    let spelled = !file.name.is_empty()
        && file
            .name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
    if !spelled {
        return Err(format!(
            "'{}' is not a connector name: lower-case letters, digits, '_' and '-'",
            file.name
        ));
    }
    let account = file
        .item
        .strip_prefix(&format!("svc:{}:", file.name))
        .unwrap_or_default();
    if account.is_empty() || account.chars().any(char::is_control) {
        return Err(format!(
            "'{}' is not svc:{}:<account>",
            file.item, file.name
        ));
    }
    Ok(())
}

fn read_one(home: &Path, path: &Path, name: &str) -> Result<Connector, String> {
    owner_only_writable(home, path)?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: Connector =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if file.name != name {
        return Err(format!(
            "{} names itself '{}', and a connector is known by its file name",
            path.display(),
            file.name
        ));
    }
    if file.definition_id.is_empty() {
        return Err(format!(
            "{}: reinstall it: it carries no definition_id",
            path.display()
        ));
    }
    if file.definition_id.len() != 64
        || !file
            .definition_id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(format!(
            "{}: reinstall it: its definition_id is not 64 lower-case hex characters",
            path.display()
        ));
    }
    if !file.authority.is_absolute() {
        return Err(format!(
            "{}: the authority '{}' is not an absolute path",
            path.display(),
            file.authority.display()
        ));
    }
    authority_closed(home, &file.authority).map_err(|why| format!("{}: {why}", path.display()))?;
    forms_known(&file.surface).map_err(|why| format!("{}: {why}", path.display()))?;
    narrow_offered(&file.surface).map_err(|why| format!("{}: {why}", path.display()))?;
    with_declared(&file.surface).map_err(|why| format!("{}: {why}", path.display()))?;
    surface_rules(&file.surface).map_err(|why| format!("{}: {why}", path.display()))?;
    name_and_item(&file).map_err(|why| format!("{}: {why}", path.display()))?;
    Ok(file)
}

#[cfg(all(test, not(unix)))]
mod off_unix {
    #[test]
    fn a_definition_is_refused_where_the_owner_only_check_cannot_run() {
        let path = std::path::Path::new("connectors").join("github.json");
        let refused = super::read_one(std::path::Path::new("."), &path, "github").unwrap_err();
        assert!(refused.contains("owner-only file check"), "{refused}");
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A state directory holding `files` under `connectors/`, each naming an authority program
    /// made beside it. Every mode is set rather than left to the umask, which may let the group
    /// write.
    fn home(files: &[(&str, &str)]) -> scratch::Dir {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch::Dir::new();
        let mode = |path: &Path, mode: u32| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        let authority = dir.path().join("authority");
        std::fs::create_dir_all(&authority).unwrap();
        std::fs::write(authority.join("agent"), "#!/bin/sh\n").unwrap();
        mode(&authority.join("agent"), 0o755);
        mode(&authority, 0o755);
        std::fs::create_dir_all(dir.path().join("connectors")).unwrap();
        for (name, text) in files {
            let named = authority.join("agent").display().to_string();
            let text = text.replace("/opt/authority/agent", &named);
            std::fs::write(dir.path().join("connectors").join(name), text).unwrap();
            mode(&dir.path().join("connectors").join(name), 0o644);
        }
        mode(&dir.path().join("connectors"), 0o755);
        mode(dir.path(), 0o755);
        dir
    }

    const GITHUB: &str = r#"{
        "name": "github",
        "definition_id": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "authority": "/opt/authority/agent",
        "item": "svc:github:octocat",
        "surface": {
            "resources": {
                "issue": { "step": "fetch", "uses": ["place", "item"], "summary": "one issue" }
            },
            "forms": { "place": "owner/name", "item": "positive-integer" }
        }
    }"#;

    #[test]
    fn a_definition_that_is_a_dangling_link_is_refused() {
        let dir = home(&[]);
        std::os::unix::fs::symlink(
            dir.path().join("nowhere.json"),
            dir.path().join("connectors").join("github.json"),
        )
        .unwrap();
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(refused.contains("is a link"), "{refused}");
    }

    /// A file or an owner that could not be read is treated as open, never as the person's own.
    #[test]
    fn a_file_or_owner_that_cannot_be_read_is_open_to_others() {
        assert!(open_to_others(None, Some(501)));
        assert!(open_to_others(Some((0o100644, 501, false)), None));
        assert!(open_to_others(None, None));
    }

    /// The program a definition names is handed a write's text, so one anybody but the person or
    /// the system could replace is refused, and so is one that is not there.
    #[test]
    fn an_authority_others_could_replace_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let set = |path: &Path, mode: u32| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        let dir = home(&[("github.json", GITHUB)]);
        assert!(installed(dir.path())["github"].is_ok());
        let agent = dir.path().join("authority").join("agent");
        set(&agent, 0o777);
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(refused.contains("can be written by others"), "{refused}");
        set(&agent, 0o755);
        set(&dir.path().join("authority"), 0o775);
        assert!(
            installed(dir.path())["github"].is_err(),
            "a folder the group can write"
        );
        set(&dir.path().join("authority"), 0o755);
        std::fs::remove_file(&agent).unwrap();
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(refused.contains("cannot be read"), "{refused}");
    }

    /// A program the system owns, writable by nobody else, is one the person may name.
    #[test]
    fn an_authority_the_system_owns_is_accepted() {
        let dir = home(&[(
            "github.json",
            &GITHUB.replace("/opt/authority/agent", "/bin/sh"),
        )]);
        let read = installed(dir.path())["github"].clone();
        assert!(read.is_ok(), "{read:?}");
    }

    #[test]
    fn a_definition_others_could_have_written_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = home(&[("github.json", GITHUB)]);
        let file = dir.path().join("connectors").join("github.json");
        assert!(installed(dir.path())["github"].is_ok());
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o666)).unwrap();
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(refused.contains("written by others"), "{refused}");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let folder = dir.path().join("connectors");
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(installed(dir.path())["github"].is_err());
    }

    #[test]
    fn a_definition_that_is_a_link_is_refused_though_it_points_at_one_that_reads() {
        let dir = home(&[]);
        std::fs::write(dir.path().join("elsewhere.json"), GITHUB).unwrap();
        std::os::unix::fs::symlink(
            dir.path().join("elsewhere.json"),
            dir.path().join("connectors").join("github.json"),
        )
        .unwrap();
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(refused.contains("is a link"), "{refused}");
    }

    #[test]
    fn only_the_persons_own_file_writable_by_them_alone_is_closed_to_others() {
        let person = Some(501);
        let file = |mode: u32, uid: u32, link: bool| Some((mode, uid, link));
        assert!(!open_to_others(file(0o100644, 501, false), person));
        assert!(!open_to_others(file(0o40700, 501, false), person));
        assert!(
            open_to_others(file(0o100644, 0, false), person),
            "another account's"
        );
        assert!(
            open_to_others(file(0o100664, 501, false), person),
            "the group's"
        );
        assert!(
            open_to_others(file(0o100646, 501, false), person),
            "the world's"
        );
        assert!(open_to_others(file(0o120777, 501, true), person), "a link");
    }

    #[test]
    fn a_definition_carrying_a_field_beside_its_five_is_refused() {
        let wire = GITHUB.replacen('{', r#"{ "wire": { "base": "https://example.com" },"#, 1);
        let dir = home(&[("github.json", &wire)]);
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(refused.contains("unknown field `wire`"), "{refused}");
    }

    #[test]
    fn a_definition_without_a_well_formed_definition_id_is_refused() {
        let id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let missing = GITHUB.replace(&format!(r#""definition_id": "{id}","#), "");
        let dir = home(&[("github.json", &missing)]);
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(
            refused.contains("reinstall it: it carries no definition_id"),
            "{refused}"
        );
        for bad in [
            &id.to_uppercase(),
            &id[1..],
            &format!("{id}0"),
            &id.replace('a', "g"),
        ] {
            let dir = home(&[("github.json", &GITHUB.replace(id, bad))]);
            let refused = installed(dir.path())["github"].clone().unwrap_err();
            assert!(
                refused.contains("not 64 lower-case hex"),
                "{bad}: {refused}"
            );
        }
        let dir = home(&[("github.json", GITHUB)]);
        assert_eq!(
            installed(dir.path())["github"]
                .as_ref()
                .unwrap()
                .definition_id,
            id
        );
    }

    #[test]
    fn a_definition_is_read_by_its_file_name() {
        let dir = home(&[("github.json", GITHUB)]);
        let found = installed(dir.path());
        let github = found["github"].as_ref().expect("reads");
        assert_eq!(github.item, "svc:github:octocat");
        assert_eq!(github.surface.resources["issue"].step, StepKind::Fetch);
        assert_eq!(github.surface.forms["place"], "owner/name");
    }

    #[test]
    fn a_definition_naming_itself_otherwise_or_a_relative_authority_is_kept_as_an_error() {
        let renamed = GITHUB.replace(r#""name": "github""#, r#""name": "gitlab""#);
        let relative = GITHUB
            .replace("/opt/authority/agent", "agent")
            .replace(r#""name": "github""#, r#""name": "other""#);
        let dir = home(&[("github.json", &renamed), ("other.json", &relative)]);
        let found = installed(dir.path());
        assert!(found["github"].as_ref().unwrap_err().contains("file name"));
        assert!(found["other"].as_ref().unwrap_err().contains("absolute"));
    }

    #[test]
    fn a_surface_with_a_key_nothing_knows_is_refused() {
        let unknown = GITHUB.replace(
            r#""summary": "one issue""#,
            r#""summary": "x", "sumary": "y""#,
        );
        let dir = home(&[("github.json", &unknown)]);
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(refused.contains("unknown field `sumary`"), "{refused}");
        let unknown = GITHUB.replace(r#""forms": {"#, r#""colour": "red", "forms": {"#);
        let dir = home(&[("github.json", &unknown)]);
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(refused.contains("unknown field `colour`"), "{refused}");
    }

    #[test]
    fn a_definition_naming_a_form_nothing_defines_is_kept_as_an_error() {
        let surface = GITHUB.replace(r#""item": "positive-integer""#, r#""item": "integer""#);
        let resource = GITHUB
            .replace(
                r#""summary": "one issue""#,
                r#""summary": "one issue", "forms": { "item": "number" }"#,
            )
            .replace(r#""name": "github""#, r#""name": "other""#);
        let dir = home(&[("github.json", &surface), ("other.json", &resource)]);
        let found = installed(dir.path());
        let surface = found["github"].as_ref().unwrap_err();
        assert!(
            surface.contains("the surface names a form nothing defines, 'integer'"),
            "{surface}"
        );
        let resource = found["other"].as_ref().unwrap_err();
        assert!(
            resource.contains("the resource 'issue' names a form nothing defines, 'number'"),
            "{resource}"
        );
    }

    #[test]
    fn a_fetch_narrows_only_by_terms_its_own_select_offers() {
        let search = |narrow: &str| {
            GITHUB.replace(
                r#""issue": { "step": "fetch", "uses": ["place", "item"], "summary": "one issue" }"#,
                &format!(
                    r#""issue": {{ "step": "fetch", "uses": ["place", "item"], "summary": "one issue" }},
                       "search": {{ "step": "fetch", "uses": ["select"], "summary": "across places",
                                   "select": ["text:*", "user:*", "sort:created-*"], "narrow": {narrow} }}"#
                ),
            )
        };
        let offered = search(r#"["user:*", "sort:created-asc"]"#);
        let dir = home(&[("github.json", &offered)]);
        let github = installed(dir.path())["github"].clone().expect("reads");
        assert_eq!(
            github.surface.resources["search"].narrow,
            ["user:*", "sort:created-asc"]
        );

        let dir = home(&[("github.json", &search(r#"["org:*"]"#))]);
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(
            refused.contains(
                "the resource 'search' has to narrow by 'org:*', which its select does not offer"
            ),
            "{refused}"
        );
        let selectless = GITHUB.replace(
            r#""summary": "one issue""#,
            r#""summary": "one issue", "narrow": ["state:*"]"#,
        );
        let dir = home(&[("github.json", &selectless)]);
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(refused.contains("takes no select"), "{refused}");
    }

    #[test]
    fn with_values_are_declared_with_a_form_exactly_when_a_resource_uses_with() {
        let event = |uses: &str, with: &str| {
            GITHUB.replace(
                r#""issue": { "step": "fetch", "uses": ["place", "item"], "summary": "one issue" }"#,
                &format!(
                    r#""issue": {{ "step": "fetch", "uses": ["place", "item"], "summary": "one issue" }},
                       "event": {{ "step": "act", "uses": {uses}, "summary": "add an event", "with": {with} }}"#
                ),
            )
        };
        let fine = event(
            r#"["place", "with"]"#,
            r#"{ "start": "datetime", "end": "datetime" }"#,
        );
        let dir = home(&[("github.json", &fine)]);
        let github = installed(dir.path())["github"].clone().expect("reads");
        assert_eq!(
            github.surface.resources["event"].with["start"],
            WithRule::Form("datetime".into())
        );

        for (uses, with, why) in [
            (
                r#"["place"]"#,
                r#"{ "start": "datetime" }"#,
                "exactly when it uses with",
            ),
            (r#"["place", "with"]"#, "{}", "exactly when it uses with"),
            (
                r#"["place", "with"]"#,
                r#"{ "start": "instant" }"#,
                "a form nothing defines, 'instant'",
            ),
            (
                r#"["place", "with"]"#,
                r#"{ "item": "datetime" }"#,
                "a with value named 'item'",
            ),
            (
                r#"["place", "with"]"#,
                r#"{ "Start": "datetime" }"#,
                "a with value named 'Start'",
            ),
        ] {
            let dir = home(&[("github.json", &event(uses, with))]);
            let refused = installed(dir.path())["github"].clone().unwrap_err();
            assert!(refused.contains(why), "{with}: {refused}");
        }
    }

    /// A surface the credential authority would refuse to load is refused here too, so no plan is
    /// made against it; and so is a name or item the authority would not accept.
    #[test]
    fn a_surface_or_name_the_authority_would_refuse_is_refused() {
        let with_resource = |resource: &str| {
            GITHUB.replace(
                r#""issue": { "step": "fetch", "uses": ["place", "item"], "summary": "one issue" }"#,
                &format!(
                    r#""issue": {{ "step": "fetch", "uses": ["place", "item"], "summary": "one issue" }},
                       "extra": {resource}"#
                ),
            )
        };
        for (resource, why) in [
            (
                r#"{ "step": "act", "uses": ["place", "item", "option"], "summary": "x" }"#,
                "lists no values for it",
            ),
            (
                r#"{ "step": "fetch", "uses": ["place", "item"], "summary": "x", "high_impact": true }"#,
                "only an act is high-impact",
            ),
            (
                r#"{ "step": "act", "uses": ["place", "item"], "summary": "x", "bound": { "to": "who" } }"#,
                "takes no 'on'",
            ),
            (
                r#"{ "step": "act", "uses": ["place", "with|item"], "summary": "x", "with": { "at": "date" } }"#,
                "one of an either-or",
            ),
            (
                r#"{ "step": "act", "uses": ["place", "with"], "summary": "x", "with": { "title": [] } }"#,
                "offers a with value that can be nothing",
            ),
            (
                r#"{ "step": "act", "uses": ["place", "on", "with"], "summary": "x",
                     "with": { "to": "address" }, "bound": { "to": "who" } }"#,
                "cannot take a with value named 'to'",
            ),
            (
                r#"{ "step": "fetch", "uses": ["place", "item"], "summary": "x", "records": { "at": "when" } }"#,
                "records fields only as a fetch with a receipt",
            ),
            (
                r#"{ "step": "fetch", "uses": ["place", "with"], "summary": "x", "with": { "at": "date" },
                     "takes": ["at"] }"#,
                "takes only with values it declares, as an act",
            ),
            (
                r#"{ "step": "act", "uses": ["place", "with"], "summary": "x", "with": { "at": "date" },
                     "takes": ["on"] }"#,
                "takes only with values it declares, as an act",
            ),
            (
                r#"{ "step": "fetch", "uses": ["place", "item"], "summary": "x", "bound": { "to": "who" } }"#,
                "only an act is high-impact, takes no text, narrows by option or binds values",
            ),
            (
                r#"{ "step": "fetch", "uses": ["place", "item"], "summary": "x", "text": false }"#,
                "only an act is high-impact, takes no text, narrows by option or binds values",
            ),
            (
                r#"{ "step": "fetch", "uses": ["place", "item"], "summary": "x",
                     "narrow_when": { "A": ["state:*"] } }"#,
                "only an act is high-impact, takes no text, narrows by option or binds values",
            ),
            (
                r#"{ "step": "act", "uses": ["place", "item", "option"], "summary": "x",
                     "values": { "option": ["A"] }, "narrow_when": { "B": ["state:*"] } }"#,
                "narrows by 'B', which it does not offer",
            ),
        ] {
            let dir = home(&[("github.json", &with_resource(resource))]);
            let refused = installed(dir.path())["github"].clone().unwrap_err();
            assert!(refused.contains(why), "{resource}: {refused}");
        }

        let either_default = GITHUB.replace(
            r#""forms": { "place""#,
            r#""defaults": { "place|item": "x" }, "forms": { "place""#,
        );
        let dir = home(&[("github.json", &either_default)]);
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(refused.contains("names an either-or"), "{refused}");

        let renamed = GITHUB.replace(r#""name": "github""#, r#""name": "Git.Hub""#);
        let dir = home(&[("Git.Hub.json", &renamed)]);
        let refused = installed(dir.path())["Git.Hub"].clone().unwrap_err();
        assert!(refused.contains("not a connector name"), "{refused}");
        let other_item = GITHUB.replace("svc:github:", "svc:gitlab:");
        let dir = home(&[("github.json", &other_item)]);
        let refused = installed(dir.path())["github"].clone().unwrap_err();
        assert!(refused.contains("is not svc:github:<account>"), "{refused}");
    }

    #[test]
    fn no_connectors_directory_means_nothing_installed() {
        let dir = scratch::Dir::new();
        assert!(installed(dir.path()).is_empty());
    }

    /// A directory under the workspace's test scratch, one per call, removed when dropped.
    mod scratch {
        pub struct Dir(std::path::PathBuf);
        impl Dir {
            pub fn new() -> Self {
                use std::sync::atomic::{AtomicU64, Ordering};
                static NEXT: AtomicU64 = AtomicU64::new(0);
                let path = crate::testutil::scratch_dir(&format!(
                    "connectors-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                let _ = std::fs::remove_dir_all(&path);
                std::fs::create_dir_all(&path).unwrap();
                Self(path)
            }
            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
