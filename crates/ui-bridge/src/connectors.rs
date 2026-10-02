//! Connectors: the MCP servers a person declares, approves and turns on from the window.
//!
//! A connector is a declaration in `~/.bravebot/mcp.json` (SERVERS-1). It is on where its
//! declaration is approved (SERVERS-5) and the home settings file requests it (SERVERS-2), which is
//! what makes every session start it. This module writes the files `bravebot mcp add`, `approve`,
//! `enable -s user`, `disable -s user` and `remove` write, and nothing else.
//!
//! Filling in the form is not the approval (SERVERS-3). `connectors.preview` returns the declaration
//! the form resolves to and its fingerprint; `connectors.connect` takes that fingerprint back, builds
//! the declaration again, and writes nothing unless the two agree. So what is approved is what the
//! person was shown, and a file or an environment that changed between the two is refused rather
//! than approved unseen.

use crate::protocol::{ErrorCode, Failure, Request};
use bravebot_agent::servers;
use bravebot_config::Managed;
use bravebot_config::import::{Destination, Unwritable};
use bravebot_config::mcp::{self, Approvals, Declaration, Declarations, Problem};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Every connector declared, and whether each is on.
pub fn list() -> Result<Value, Failure> {
    let home = bravebot_agent::home::directory();
    let writable = bravebot_agent::home::writable().is_some();
    let user_home = std::env::var_os("HOME").map(PathBuf::from);
    let Some(directory) = home else {
        return Ok(json!({
            "home": user_home.map(|home| home.display().to_string()),
            "state": null,
            "writable": false,
            "unavailable": servers::no_state_directory(),
            "connectors": [],
        }));
    };
    let declarations = Declarations::read(&directory).map_err(|why| {
        Failure::new(
            ErrorCode::Config,
            format!(
                "{} cannot be read: {}",
                mcp::declarations_file(&directory).display(),
                bravebot_agent::mcp::unreadable(&why)
            ),
        )
    })?;
    let approvals = Approvals::read(&directory);
    let requested = requested_here(&directory);
    let managed = Managed::load();
    let environment = |name: &str| std::env::var_os(name);
    let connectors: Vec<Value> = declarations
        .entries()
        .into_iter()
        .map(|entry| match entry.declaration {
            Ok(declaration) => {
                let digest = declaration.digest();
                let approved = approvals.approves(&digest);
                let on = requested.contains(&entry.alias);
                let mut drawn = drawn(&entry.alias, &declaration);
                drawn["approved"] = json!(approved);
                drawn["requested"] = json!(on);
                drawn["connected"] = json!(approved && on);
                drawn["changed"] = json!(approvals.changed(&entry.alias, &digest));
                drawn["refused"] = json!(servers::refused_declaration(
                    &managed,
                    &declaration,
                    &environment
                ));
                drawn
            }
            Err(found) => json!({
                "alias": entry.alias,
                "problem": servers::problem(&found),
                "requested": requested.contains(&entry.alias),
                "connected": false,
            }),
        })
        .collect();
    Ok(json!({
        "home": user_home.map(|home| home.display().to_string()),
        "state": directory.display().to_string(),
        "writable": writable,
        "unavailable": null,
        "connectors": connectors,
    }))
}

/// The declaration a form resolves to, for the person to read before connecting it.
///
/// `fingerprint` is the whole digest, which `connectors.connect` takes back. `exists` says a
/// connector of this name is declared already, and `same` that it is this declaration.
pub fn preview(request: &Request) -> Result<Value, Failure> {
    let directory = state()?;
    let alias = alias(request)?;
    let existing = existing(&directory, &alias)?;
    let declaration = resolve(request, &directory, existing.as_ref())?;
    let mut drawn = drawn(&alias, &declaration);
    drawn["fingerprint"] = json!(declaration.digest().to_string());
    drawn["exists"] = json!(existing.is_some());
    drawn["same"] = json!(existing.as_ref() == Some(&declaration));
    drawn["fetching"] = json!(
        servers::fetching(&alias, &declaration)
            .iter()
            .map(|line| line.trim().to_string())
            .collect::<Vec<_>>()
    );
    drawn["refused"] = json!(servers::refused_declaration(
        &Managed::load(),
        &declaration,
        &|name| std::env::var_os(name)
    ));
    Ok(drawn)
}

/// Declare, approve and turn on a connector, where the declaration is still the one the person was
/// shown.
///
/// `replace` must be set to write over a connector of the same name that declares something else,
/// so adding one cannot quietly change another. The request goes in the home settings file, so every
/// session starts it.
pub fn connect(request: &Request) -> Result<Value, Failure> {
    let directory = writable_state()?;
    let alias = alias(request)?;
    let shown = request.string("fingerprint")?;
    let mut declarations = read(&directory)?;
    let existing = declarations
        .get(&alias)
        .and_then(|entry| entry.declaration.ok());
    let declaration = resolve(request, &directory, existing.as_ref())?;
    if declaration.digest().to_string() != shown {
        return Err(Failure::bad_request(format!(
            "{alias} resolves to something other than what was shown, so nothing was connected. \
             Review it again."
        )));
    }
    let replacing = declarations.get(&alias).is_some() && existing.as_ref() != Some(&declaration);
    if replacing && !request.flag("replace", false) {
        return Err(Failure::bad_request(format!(
            "a connector named {alias} exists already. Choose another name."
        )));
    }
    if let Some(reason) = servers::refused_declaration(&Managed::load(), &declaration, &|name| {
        std::env::var_os(name)
    }) {
        return Err(Failure::bad_request(format!(
            "{alias} is not connected: {reason}"
        )));
    }
    let mut settings = user_settings(&directory)?;
    if !settings.requests(&alias) && !settings.request(&alias) {
        return Err(Failure::new(
            ErrorCode::Config,
            format!(
                "{} holds an mcp block this cannot add to. Edit it by hand.",
                settings.path().display()
            ),
        ));
    }

    declarations.insert(&alias, &declaration);
    bravebot_agent::home::create_directory(&directory)
        .map_err(|error| written(&directory, error))?;
    let file = mcp::declarations_file(&directory);
    bravebot_agent::mcp::replace(&file, declarations.to_text())
        .map_err(|error| written(&file, error))?;
    let mut approvals = approvals_to_change(&directory)?;
    servers::record(
        &directory,
        &declarations,
        &mut approvals,
        &alias,
        &declaration,
    )
    .map_err(|reason| Failure::new(ErrorCode::Internal, reason))?;
    write_settings(&settings)?;
    list()
}

/// Turn a connector off: take it out of the home settings file. Its declaration and approval stay,
/// so connecting it again asks nothing new.
pub fn disconnect(request: &Request) -> Result<Value, Failure> {
    let directory = writable_state()?;
    let alias = request.string("alias")?;
    let mut settings = user_settings(&directory)?;
    if settings.withdraw(&alias) {
        write_settings(&settings)?;
    }
    list()
}

/// Remove a connector: turn it off, and delete its declaration and its approval together, since an
/// approval outliving its declaration is a digest nothing resolves to.
pub fn remove(request: &Request) -> Result<Value, Failure> {
    let directory = writable_state()?;
    let alias = request.string("alias")?;
    let mut settings = user_settings(&directory)?;
    if settings.withdraw(&alias) {
        write_settings(&settings)?;
    }
    let mut declarations = read(&directory)?;
    if declarations.remove(&alias) {
        let mut approvals = approvals_to_change(&directory)?;
        let file = mcp::declarations_file(&directory);
        bravebot_agent::mcp::replace(&file, declarations.to_text())
            .map_err(|error| written(&file, error))?;
        approvals.keep_only(&declarations);
        let file = mcp::approvals_file(&directory);
        bravebot_agent::mcp::replace(&file, approvals.to_text())
            .map_err(|error| written(&file, error))?;
    }
    list()
}

/// The approvals to change and write back, refused where the file is there and cannot be read,
/// since writing over it would lose every approval it holds.
fn approvals_to_change(directory: &Path) -> Result<Approvals, Failure> {
    Approvals::to_change(directory).map_err(|why| {
        Failure::new(
            ErrorCode::Config,
            format!(
                "{} cannot be read: {}",
                mcp::approvals_file(directory).display(),
                bravebot_agent::mcp::unreadable(&why)
            ),
        )
    })
}

/// A declaration as the window draws it. A stored value is named and never sent.
fn drawn(alias: &str, declaration: &Declaration) -> Value {
    let (command, url, directory) = match declaration {
        Declaration::Stdio {
            argv, directory, ..
        } => (Some(argv.clone()), None, directory.clone()),
        Declaration::Http { url } => (None, Some(url.clone()), None),
    };
    let variables: Vec<Value> = declaration
        .stored()
        .map(|name| json!({ "name": name, "stored": true }))
        .chain(
            declaration
                .variables()
                .iter()
                .map(|name| json!({ "name": name, "stored": false })),
        )
        .collect();
    json!({
        "alias": alias,
        "transport": declaration.transport(),
        "command": command,
        "url": url,
        "variables": variables,
        "reads": declaration.reads(),
        "directory": directory,
        "digest": declaration.digest().short(),
        "problem": null,
    })
}

/// The declaration a request's form describes.
///
/// `variables` is a list of `{ name, value?, keep? }`. A value is stored; a name alone is read from
/// the environment at launch; `keep` takes the value `existing` stores under that name, so a
/// settings page can change other fields without the person typing a token again. A program given
/// by a bare name is given `PATH` where nothing names it, as `bravebot mcp add` gives it.
fn resolve(
    request: &Request,
    directory: &Path,
    existing: Option<&Declaration>,
) -> Result<Declaration, Failure> {
    let refused = |found: Problem| Failure::bad_request(servers::problem(&found));
    match request.string("transport")?.as_str() {
        "http" => {
            if request
                .param("variables")
                .as_array()
                .is_some_and(|v| !v.is_empty())
                || request.optional_string("directory").is_some()
            {
                return Err(Failure::bad_request(
                    "a connector reached by URL takes no variables and no directory",
                ));
            }
            Declaration::http(request.string("url")?.trim().to_string()).map_err(refused)
        }
        "stdio" => {
            let argv: Vec<String> = request
                .param("command")
                .as_array()
                .map(|words| {
                    words
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if argv.first().is_none_or(|program| program.trim().is_empty()) {
                return Err(Failure::bad_request(
                    "a local connector needs a program to run",
                ));
            }
            let stored = match existing {
                Some(Declaration::Stdio { env, .. }) => env.clone(),
                _ => BTreeMap::new(),
            };
            let mut variables = Vec::new();
            let mut env = BTreeMap::new();
            for given in request.param("variables").as_array().into_iter().flatten() {
                let name = given["name"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if !mcp::is_variable_name(&name) {
                    return Err(Failure::bad_request(
                        "a variable's name is a letter or _, then letters, digits and _",
                    ));
                }
                let value = match (given["value"].as_str(), given["keep"].as_bool()) {
                    (Some(value), _) => Some(value.to_string()),
                    (None, Some(true)) => Some(stored.get(&name).cloned().ok_or_else(|| {
                        Failure::bad_request(format!("{name} has no stored value to keep"))
                    })?),
                    _ => None,
                };
                let twice = variables.contains(&name) || env.contains_key(&name);
                if twice {
                    return Err(refused(Problem::Twice(name)));
                }
                match value {
                    Some(value) => {
                        env.insert(name, value);
                    }
                    None => variables.push(name),
                }
            }
            if servers::is_a_bare_name(&argv[0])
                && !env.contains_key("PATH")
                && !variables.iter().any(|name| name == "PATH")
            {
                variables.push("PATH".to_string());
            }
            let directory_given = request
                .optional_string("directory")
                .filter(|path| !path.trim().is_empty())
                .map(|path| place(path.trim()))
                .transpose()?;
            let declaration = Declaration::stdio(argv, variables, directory_given)
                .and_then(|declaration| declaration.storing(env))
                .map_err(refused)?;
            servers::with_reads(declaration, directory, &|name| std::env::var_os(name))
                .map_err(refused)
        }
        _ => Err(Failure::bad_request(
            "a connector is reached by URL (http) or runs a local program (stdio)",
        )),
    }
}

/// A directory a local server runs in, as the absolute path it resolves to, and never one inside a
/// repository (SERVERS-10).
fn place(typed: &str) -> Result<String, Failure> {
    let resolved = std::fs::canonicalize(typed)
        .ok()
        .filter(|path| path.is_dir())
        .ok_or_else(|| Failure::bad_request(format!("{typed} is not a directory")))?;
    if let Some(repository) = servers::repository_holding(&resolved) {
        return Err(Failure::bad_request(servers::in_a_repository(
            &resolved,
            &repository,
        )));
    }
    resolved
        .into_os_string()
        .into_string()
        .map_err(|_| Failure::bad_request("the directory's path is not text"))
}

fn alias(request: &Request) -> Result<String, Failure> {
    let alias = request.string("alias")?.trim().to_string();
    if !mcp::is_alias(&alias) {
        return Err(Failure::bad_request(
            "a connector's name is letters, digits, - and _, starting with a letter or a digit",
        ));
    }
    Ok(alias)
}

fn state() -> Result<PathBuf, Failure> {
    bravebot_agent::home::directory()
        .ok_or_else(|| Failure::new(ErrorCode::NoHome, servers::no_state_directory()))
}

/// The state directory, where this run may write it, which an incognito one may not.
fn writable_state() -> Result<PathBuf, Failure> {
    let directory = state()?;
    bravebot_agent::home::writable()
        .map(|_| directory)
        .ok_or_else(|| {
            Failure::bad_request("connectors cannot be changed while nothing is being written")
        })
}

fn read(directory: &Path) -> Result<Declarations, Failure> {
    Declarations::read(directory).map_err(|why| {
        Failure::new(
            ErrorCode::Config,
            format!(
                "{} cannot be read, so it was left as it is: {}",
                mcp::declarations_file(directory).display(),
                bravebot_agent::mcp::unreadable(&why)
            ),
        )
    })
}

fn existing(directory: &Path, alias: &str) -> Result<Option<Declaration>, Failure> {
    Ok(read(directory)?
        .get(alias)
        .and_then(|entry| entry.declaration.ok()))
}

/// The aliases the home settings file requests, which are the ones every session starts.
fn requested_here(directory: &Path) -> Vec<String> {
    let file = bravebot_config::user_settings_file(directory);
    match Destination::open(&file) {
        Ok(settings) => {
            let declared = Declarations::read(directory).unwrap_or_default();
            declared
                .entries()
                .into_iter()
                .map(|entry| entry.alias)
                .filter(|alias| settings.requests(alias))
                .collect()
        }
        Err(_) => Vec::new(),
    }
}

fn user_settings(directory: &Path) -> Result<Destination, Failure> {
    let file = bravebot_config::user_settings_file(directory);
    Destination::open(&file).map_err(|why| unwritable(why, &file))
}

/// Write the home settings file whole, through a link where it is one, as `bravebot mcp enable`
/// writes it.
fn write_settings(settings: &Destination) -> Result<(), Failure> {
    let path = settings.path();
    if settings.changed() {
        return Err(unwritable(Unwritable::Changed, path));
    }
    let text = settings.text().map_err(|why| unwritable(why, path))?;
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(parent) = target.parent() {
        bravebot_agent::home::create_directory(parent).map_err(|error| written(parent, error))?;
    }
    bravebot_agent::mcp::replace(&target, text.expose()).map_err(|error| written(&target, error))
}

fn unwritable(why: Unwritable, path: &Path) -> Failure {
    let reason = match why {
        Unwritable::NotADocument => "it does not hold a settings document",
        Unwritable::TooLarge => "it is larger than the settings reader reads",
        Unwritable::Changed => "it changed while this was open",
    };
    Failure::new(
        ErrorCode::Config,
        format!("{} was left as it is: {reason}", path.display()),
    )
}

fn written(path: &Path, error: std::io::Error) -> Failure {
    Failure::new(
        ErrorCode::Internal,
        format!("{} could not be written: {error}", path.display()),
    )
}
