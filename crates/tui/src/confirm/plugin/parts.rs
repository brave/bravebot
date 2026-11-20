//! The hook and server rows of the install prompt (PLUGIN-4, PLUGIN-13).
//!
//! The rows are built from what `bravebot_agent::plugins::stage` read out of the staged tree as
//! data: each hook's moment and argv and each server's transport, argv or url, variables and files,
//! with `${PLUGIN_ROOT}` already replaced by the directory the plugin will have. Nothing here
//! decides what runs. A hooks or servers file that could not be read, and any hook entry the hook
//! reader passed over, is said in the listing, so the prompt never lists less than the plugin
//! carries.
//!
//! Everything the plugin wrote is drawn behind the margin through [`super::untrusted`]. Rows are
//! never cut: a person answering must have been shown every word of every argv.

use bravebot_agent::plugin_servers;
use bravebot_agent::plugins::Staged;
use bravebot_config::hooks::{Hook, Unkept};
use bravebot_config::mcp::Declaration;
use bravebot_config::plugin::FileFault;
use bravebot_i18n::t;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::super::chrome::indented;
use super::untrusted;

/// One hook the plugin's hooks file declares, argv as it will run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRow {
    /// The moment, in the words the hooks file spells it with.
    pub moment: String,
    /// The tool the hook is about, where the entry named one.
    pub tool: Option<String>,
    /// The file a `file-changed` entry watches.
    pub watches: Option<String>,
    /// The program and its arguments, `${PLUGIN_ROOT}` replaced.
    pub argv: Vec<String>,
    /// The bound the entry states, in seconds.
    pub timeout_secs: Option<u64>,
}

/// Something a hook or a server would reach, which is on its own row (PLUGIN-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// A program that would be started.
    Command(String),
    /// A host that would be connected to.
    Host(String),
    /// A file a local server may read.
    File(String),
    /// The directory a local server runs in.
    Directory(String),
}

/// One server the plugin's servers file declares, command as it will run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRow {
    /// The alias as the file wrote it.
    pub alias: String,
    /// Whether the entry states a declaration that can be used. An entry that does not is listed
    /// as one and is never started.
    pub usable: bool,
    /// Whether a session can reach the server under `<plugin>_<alias>`. A usable entry whose joined
    /// alias is longer than an alias may be is listed in full and is never started.
    pub reachable: bool,
    /// `stdio` or `http`, empty for an entry that cannot be used.
    pub transport: String,
    /// A local server's program and arguments, `${PLUGIN_ROOT}` replaced.
    pub argv: Vec<String>,
    /// A remote server's url as written.
    pub url: Option<String>,
    /// The names of the variables it receives from the environment, or names for a header or a
    /// bearer token.
    pub variables: Vec<String>,
    /// The variables it is given with a value, as `NAME=value`, left as written.
    pub stored: Vec<String>,
    /// What it would reach, one row each.
    pub reaches: Vec<Reach>,
}

/// What one part file held, for the listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed<T> {
    pub rows: Vec<T>,
    /// Why the file could not be read at all, where it could not.
    pub fault: Option<FileFault>,
    /// What the hook reader passed over. Always none for servers, whose unusable entries are rows.
    pub unkept: Unkept,
}

impl<T> Listed<T> {
    /// A file that could not be read.
    fn unread(fault: FileFault) -> Self {
        Self {
            rows: Vec::new(),
            fault: Some(fault),
            unkept: Unkept::default(),
        }
    }
}

fn hook_row(hook: &Hook) -> HookRow {
    HookRow {
        moment: hook.moment().as_str().to_string(),
        tool: hook.tool().map(str::to_string),
        watches: hook.path().map(str::to_string),
        argv: hook.run().to_vec(),
        timeout_secs: hook.timeout().map(|bound| bound.as_secs()),
    }
}

/// The hook rows of `staged`, or `None` where its manifest names no hooks file.
pub(super) fn hooks_of(staged: &Staged) -> Option<Listed<HookRow>> {
    staged.hooks.as_ref().map(|read| match read {
        Ok(list) => Listed {
            rows: list.hooks.declared().iter().map(hook_row).collect(),
            fault: None,
            unkept: list.unkept,
        },
        Err(fault) => Listed::unread(*fault),
    })
}

/// The hosts, files and directory a declaration would reach, and the program it would start.
fn reaches(declaration: &Declaration) -> Vec<Reach> {
    let mut found = Vec::new();
    if let Some(program) = declaration.argv().first() {
        found.push(Reach::Command(program.clone()));
    }
    if let Some(host) = declaration.url().and_then(bravebot_core::url::host_of) {
        found.push(Reach::Host(host));
    }
    for host in declaration.allowed_hosts().unwrap_or_default() {
        found.push(Reach::Host(host.clone()));
    }
    found.extend(declaration.reads().iter().cloned().map(Reach::File));
    found.extend(
        declaration
            .directory()
            .map(|d| Reach::Directory(d.to_string())),
    );
    found
}

fn server_row(plugin: &str, alias: &str, declaration: &Declaration) -> ServerRow {
    let mut variables: Vec<String> = declaration.variables().to_vec();
    variables.extend(declaration.bearer_env().map(str::to_string));
    variables.extend(
        declaration
            .env_http_headers()
            .map(|(header, variable)| format!("{variable} ({header})")),
    );
    ServerRow {
        alias: alias.to_string(),
        usable: true,
        reachable: plugin_servers::is_reachable(plugin, alias),
        transport: declaration.transport().to_string(),
        argv: declaration.argv().to_vec(),
        url: declaration.url().map(str::to_string),
        variables,
        stored: declaration
            .stored_values()
            .map(|(name, value)| format!("{name}={value}"))
            .collect(),
        reaches: reaches(declaration),
    }
}

/// The server rows of `staged`, or `None` where its manifest names no servers file.
pub(super) fn servers_of(staged: &Staged) -> Option<Listed<ServerRow>> {
    let plugin = staged.manifest.name.as_str();
    staged.servers.as_ref().map(|read| match read {
        Ok(list) => Listed {
            rows: list
                .entries
                .iter()
                .map(|entry| match &entry.declaration {
                    Ok(declaration) => server_row(plugin, &entry.alias, declaration),
                    Err(_) => ServerRow {
                        alias: entry.alias.clone(),
                        usable: false,
                        reachable: false,
                        transport: String::new(),
                        argv: Vec::new(),
                        url: None,
                        variables: Vec::new(),
                        stored: Vec::new(),
                        reaches: Vec::new(),
                    },
                })
                .collect(),
            fault: None,
            unkept: Unkept::default(),
        },
        Err(fault) => Listed::unread(*fault),
    })
}

/// What the rows are drawn with: the margin for the plugin's text and the two styles.
pub(super) struct Pen<'a> {
    pub margin: &'a Span<'static>,
    pub bold: Style,
    pub muted: Style,
    pub width: usize,
}

impl Pen<'_> {
    /// A label, then the plugin's text behind the margin, uncut.
    fn labelled(&self, lines: &mut Vec<Line<'static>>, label: String, value: &str) {
        lines.extend(indented(label, self.muted, self.width));
        let (rows, _) = untrusted(value, self.bold, self.margin, self.width, None, false);
        lines.extend(rows);
    }

    /// A label, then each word of an argument vector on its own numbered row.
    fn words(&self, lines: &mut Vec<Line<'static>>, label: String, words: &[String]) {
        lines.extend(indented(label, self.muted, self.width));
        for (number, word) in words.iter().enumerate() {
            let text = format!("{}. {word}", number + 1);
            let (rows, _) = untrusted(&text, self.bold, self.margin, self.width, None, false);
            lines.extend(rows);
        }
    }
}

fn fault_text(fault: FileFault) -> String {
    match fault {
        FileFault::Unreadable => t!(plugin_install_fault_unreadable),
        FileFault::NotARegularFile => t!(plugin_install_fault_not_file),
        FileFault::TooLarge => t!(plugin_install_fault_too_large),
        FileFault::NotText => t!(plugin_install_fault_not_text),
        FileFault::NotHooks => t!(plugin_install_fault_not_hooks),
        FileFault::NotServers => t!(plugin_install_fault_not_servers),
        FileFault::PathNotText => t!(plugin_install_fault_path),
    }
    .to_string()
}

fn reach_row(pen: &Pen<'_>, lines: &mut Vec<Line<'static>>, reach: &Reach) {
    let (label, value) = match reach {
        Reach::Command(value) => (t!(plugin_install_reach_command), value),
        Reach::Host(value) => (t!(plugin_install_reach_host), value),
        Reach::File(value) => (t!(plugin_install_reach_file), value),
        Reach::Directory(value) => (t!(plugin_install_reach_directory), value),
    };
    pen.labelled(lines, label.to_string(), value);
}

/// What a part file said that could not be listed.
fn left_out(
    pen: &Pen<'_>,
    lines: &mut Vec<Line<'static>>,
    listed_fault: Option<FileFault>,
    unkept: Unkept,
) {
    if let Some(fault) = listed_fault {
        lines.extend(indented(fault_text(fault), pen.muted, pen.width));
    }
    if !unkept.is_none() {
        lines.extend(indented(
            t!(
                plugin_install_hooks_unkept,
                dropped = unkept.dropped,
                keys = unkept.root_keys + unkept.entry_keys
            ),
            pen.muted,
            pen.width,
        ));
    }
}

/// The hooks section: a heading and, for each hook, its moment, what it is limited to, its argv
/// and what it would start.
pub(super) fn draw_hooks(
    pen: &Pen<'_>,
    lines: &mut Vec<Line<'static>>,
    listing: Option<&Listed<HookRow>>,
) {
    lines.push(Line::raw(""));
    let Some(listed) = listing else {
        lines.extend(indented(t!(plugin_install_no_hooks), pen.muted, pen.width));
        return;
    };
    lines.extend(indented(
        t!(plugin_install_hooks, count = listed.rows.len()),
        pen.bold,
        pen.width,
    ));
    for hook in &listed.rows {
        lines.push(Line::raw(""));
        lines.extend(indented(
            t!(plugin_install_hook_at, moment = hook.moment.as_str()),
            pen.bold,
            pen.width,
        ));
        if let Some(tool) = &hook.tool {
            pen.labelled(lines, t!(plugin_install_hook_tool).to_string(), tool);
        }
        if let Some(path) = &hook.watches {
            pen.labelled(lines, t!(plugin_install_hook_path).to_string(), path);
        }
        if let Some(seconds) = hook.timeout_secs {
            lines.extend(indented(
                t!(plugin_install_hook_timeout, seconds = seconds),
                pen.muted,
                pen.width,
            ));
        }
        pen.words(lines, t!(plugin_install_argv).to_string(), &hook.argv);
        if let Some(program) = hook.argv.first() {
            reach_row(pen, lines, &Reach::Command(program.clone()));
        }
    }
    left_out(pen, lines, listed.fault, listed.unkept);
}

/// The servers section: a heading and, for each server, its transport, its command or url, the
/// variables it names and everything it would reach.
pub(super) fn draw_servers(
    pen: &Pen<'_>,
    lines: &mut Vec<Line<'static>>,
    listing: Option<&Listed<ServerRow>>,
) {
    lines.push(Line::raw(""));
    let Some(listed) = listing else {
        lines.extend(indented(
            t!(plugin_install_no_servers),
            pen.muted,
            pen.width,
        ));
        return;
    };
    lines.extend(indented(
        t!(plugin_install_servers, count = listed.rows.len()),
        pen.bold,
        pen.width,
    ));
    for server in &listed.rows {
        lines.push(Line::raw(""));
        pen.labelled(lines, t!(plugin_install_server).to_string(), &server.alias);
        if !server.usable {
            lines.extend(indented(
                t!(plugin_install_server_unusable),
                pen.muted,
                pen.width,
            ));
            continue;
        }
        if !server.reachable {
            lines.extend(indented(
                t!(plugin_install_server_unreachable),
                pen.muted,
                pen.width,
            ));
        }
        pen.labelled(
            lines,
            t!(plugin_install_server_transport).to_string(),
            &server.transport,
        );
        if !server.argv.is_empty() {
            pen.words(lines, t!(plugin_install_argv).to_string(), &server.argv);
        }
        if let Some(url) = &server.url {
            pen.labelled(lines, t!(plugin_install_server_url).to_string(), url);
        }
        for variable in &server.variables {
            pen.labelled(
                lines,
                t!(plugin_install_server_variable).to_string(),
                variable,
            );
        }
        for stored in &server.stored {
            pen.labelled(lines, t!(plugin_install_server_stored).to_string(), stored);
        }
        for reach in &server.reaches {
            reach_row(pen, lines, reach);
        }
    }
    left_out(pen, lines, listed.fault, listed.unkept);
}
