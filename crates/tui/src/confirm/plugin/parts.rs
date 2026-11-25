//! The hook and server rows of the install prompt (PLUGIN-4, PLUGIN-13).
//!
//! The rows are built in `bravebot_agent::plugin_listing`, which the desktop bridge lists from too,
//! out of what `bravebot_agent::plugins::stage` read of the staged tree as data: each hook's moment
//! and argv and each server's transport, argv or url, variables and files, with `${PLUGIN_ROOT}`
//! already replaced by the directory the plugin will have. Nothing here decides what runs. A hooks or servers file that could not be read, and any hook entry the hook
//! reader passed over, is said in the listing, so the prompt never lists less than the plugin
//! carries.
//!
//! Everything the plugin wrote is drawn behind the margin through [`super::untrusted`]. Rows are
//! never cut: a person answering must have been shown every word of every argv.

use bravebot_config::hooks::Unkept;
use bravebot_config::plugin::FileFault;
use bravebot_i18n::t;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::super::chrome::indented;
use super::untrusted;

pub use bravebot_agent::plugin_listing::{HookRow, Listed, Reach, ServerRow};

/// What the rows are drawn with: the margin for the plugin's text and the two styles.
pub(super) struct Pen<'a> {
    pub margin: &'a Span<'static>,
    pub bold: Style,
    pub muted: Style,
    pub width: usize,
}

impl Pen<'_> {
    /// A label, then the plugin's text behind the margin, uncut.
    pub(super) fn labelled(&self, lines: &mut Vec<Line<'static>>, label: String, value: &str) {
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

pub(super) fn fault_text(fault: FileFault) -> String {
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
pub(super) fn left_out(
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
        hook_body(pen, lines, hook);
    }
    left_out(pen, lines, listed.fault, listed.unkept);
}

/// One hook: its moment, what it is limited to, its argv and what it would start. Shared by the
/// install listing and the update difference, so a hook reads the same in both.
pub(super) fn hook_body(pen: &Pen<'_>, lines: &mut Vec<Line<'static>>, hook: &HookRow) {
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
        server_body(pen, lines, server);
    }
    left_out(pen, lines, listed.fault, listed.unkept);
}

/// One server: its alias, transport, command or url, variables and everything it would reach.
/// Shared by the install listing and the update difference.
pub(super) fn server_body(pen: &Pen<'_>, lines: &mut Vec<Line<'static>>, server: &ServerRow) {
    pen.labelled(lines, t!(plugin_install_server).to_string(), &server.alias);
    if !server.usable {
        lines.extend(indented(
            t!(plugin_install_server_unusable),
            pen.muted,
            pen.width,
        ));
        return;
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
