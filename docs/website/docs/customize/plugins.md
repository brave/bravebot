---
sidebar_position: 9
title: Plugins
description: Install a bundle of skills, agent definitions, hooks and MCP servers from a folder on this machine or a git URL with /plugin add or the desktop app's Plugins settings, see what it shows you first, the question a plugin hook and a plugin server each ask, what stops them when the plugin's folder changes, and what a plugin can never carry.
---

# Plugins

A plugin is a folder with a `plugin.toml` at its root that bundles skills, agent definitions, hooks
and MCP servers under one name. `/plugin add <path>` installs one from a folder on this machine, and
`/plugin add <git url>` or `/plugin add <git url>#<ref>` installs one from a git repository. You type
the command, and you are shown every skill, definition, hook and server in the plugin before anything
is copied. In the desktop app, the Plugins page in Settings does the same
(see [In the desktop app](#in-the-desktop-app)).

A plugin's hooks and servers are installed with it, and each
asks its own question before it first runs or starts. `/plugin update <name>` fetches the source again
and puts what changed to you, and nothing updates on its own.

## The commands

| Command | What it does |
|---|---|
| `/plugin add <path>` | Reads the folder at `<path>`, shows what it holds and installs it if you say yes |
| `/plugin add <git url>[#<ref>]` | Asks about the host, fetches the repository, shows what it holds and installs it if you say yes |
| `/plugin list` | Prints each installed plugin's name, version, source, resolved commit, digest and whether it is enabled |
| `/plugin update <name>` | Fetches the plugin's recorded source again, shows what was added, removed or changed, and replaces the plugin only if you say yes |
| `/plugin remove <name>` | Withdraws the plugin's hook and server answers, then deletes the plugin's folder and its record |
| `/plugin disable <name>` | Keeps the plugin installed and stops it offering anything: no hook fires and no server starts |
| `/plugin enable <name>` | Turns a disabled plugin back on, and asks nothing new for a folder that has not changed |

These are commands you type, and the desktop Plugins page is the only other way to run them. No tool the model is offered installs or
changes a plugin, and a `/plugin add` line that arrives inside pasted text is text in the box, not
the command. A one-shot run has no `/plugin` command and installs nothing, since nobody can be
asked there. See [`/plugin`](../reference/commands.md) for the forms.

A relative `<path>` is read from the session's workspace folder. An installed plugin is yours and
is offered in every project.

## In the desktop app

Open Settings and choose **Plugins**. The page lists each installed plugin with its name, version,
source, ref and commit for a git source, digest, and a switch that turns it on and off. It reads
only the records, as `/plugin list` does.

- **Install** reads the source in the field above the button. Type an absolute folder path or a git
  URL, or use **Choose folder…** to pick a folder. A relative path is refused, because the app has
  no workspace folder to read it from.
- The switch disables or enables the plugin, as `/plugin disable` and `/plugin enable` do, and asks
  nothing.
- **Update** runs `/plugin update` for that plugin, and **Remove** runs `/plugin remove`.

Install and Update put the same questions as the commands, in the window:

- For a git source you are first asked about the host, with the host on its own row. No connection is
  made until you choose **Yes, fetch from this host**.
- The install sheet and the update sheet list the same rows as the boxes described below, with the
  plugin's own text inside the untrusted content margin. A long description is cut, with
  **Show more** beside it, and no word of a hook's or server's program and arguments is cut.
- **Yes, install** and **Yes, update** are enabled only once every row has been on screen. Only those
  buttons install or update. **Don't install**, **Keep it as it is**, `Esc`, closing the sheet and
  leaving the page each answer no.
- The app has no open session for the page, so a skill or definition is marked as shadowed only by
  one in your home directory, and the sheet says so.
- The app cannot approve a plugin hook, so a plugin's hooks do not run in the app's conversations.
  The sheet says so for a plugin that carries hooks.

Only the app's own window can do any of this. A client of the local host cannot install, update,
remove, enable or disable a plugin, and nothing in a conversation or a link can start an install.

## Installing from a git URL

A git source is an `https` URL whose path ends in `.git`, an `ssh://host/path` URL, or the
`user@host:path` form. Add `#<ref>` to name a branch, a tag or a full commit, as in
`/plugin add https://example.com/team/review-kit.git#v1.2`. Without a ref the repository's default
branch is used.

Before anything is fetched you are asked about the host, in the same box a fetch of a web page
uses, with the host on its own row above the URL. No connection is made to it until you answer `y`,
and `n` installs nothing. No permission mode and no allow rule answers this question for you. The
host is the one in the URL, and a source that redirects to another host is not followed.

The repository is fetched one commit deep by the `git` program, which bravebot starts itself, from a
folder on its `PATH` that a sandboxed command could not have written. If the only `git` is in such a
folder, bravebot says git is not installed and asks nothing. The variables that make `git` or `ssh`
run a program of their own choosing, such as `GIT_SSH_COMMAND` and `GIT_ASKPASS`, are removed from its
environment, so put such settings in your git or ssh configuration, which is read. No submodule is
fetched and no hook or file system monitor from the repository runs. The fetch is made in a private
folder in your system's temporary directory, which is removed when the install ends.

These are refused with a notice, and no question is asked and no program is started:

- an archive, such as a URL ending in `.zip`, `.tar.gz` or `.tgz`
- an npm package, such as `@scope/pkg` or `npm:pkg`
- a plain URL, which is any `http` or `https` URL whose path does not end in `.git`
- a `file://` URL, `git://`, `ftp://` and `git+https://` (type a local folder as a path)
- a ref that starts with `-` or holds a control character

The box that lists the plugin then shows the ref you asked for, or says none was given, and the full
40-character commit it resolved to.

## The manifest

```toml
name = "review-kit"
version = "0.1.0"
description = "Review helpers"
skills = "skills"
agents = "agents"
hooks = "hooks.json"
servers = "servers.json"
```

`name`, `version` and `description` are required. The name is letters, digits and hyphens only,
because it is the folder name the plugin is installed under. `skills` names a folder of skills
(`skills/<name>/SKILL.md`, as in [Skills](skills.md)) and `agents` names a folder of definitions
(`agents/<name>.md`, as in [Agents](agents.md)). `hooks` names one file inside the plugin that
declares hooks, in the format of `~/.bravebot/hooks.json` ([Hooks](hooks.md)), and `servers` names one
file that declares MCP servers, in the format of `~/.bravebot/mcp.json`
([MCP servers](mcp-servers.md)). All four are optional, and a folder or file the manifest does not
name is not read for parts, whatever it is called.

Each of the four is a relative path inside the plugin: not absolute, with no `..`, and with no link
in it that leaves the plugin.

A key this build does not read is reported and ignored.

## What you are shown

`/plugin add` copies the folder to a staging directory first and reads it there as data. Nothing in
it is run. It then asks once, in a box that shows:

- the plugin's name, version and source (a path, or the git URL)
- for a git source, the ref you asked for and the full commit it resolved to
- each skill's name and description
- each definition's name and kind
- every hook: its moment, and its program and each argument on a row of its own
- every server: its transport, its command and each argument or its url and host, the variables it
  names, the variables it stores a value for (shown as `NAME=value`, as written), the files it may
  read and its directory
- a mark on any skill or definition that another source already offers under the same name, saying
  which one, since that one is what a session gets and not the plugin's
- a sentence saying that yes does not make a hook run or a server reachable

All of it is text the plugin's author wrote, so it is drawn inside the margin used for untrusted
content, with control and invisible characters made visible. That includes every word of a hook's
program and arguments and of a server's declaration. A long description is cut after a few
lines and shown in full when you press `e`, and you can answer yes only once nothing is cut. `y`
installs. `n` or `Esc` installs nothing and copies nothing. The box is the same in every
[permission mode](../security/permission-modes.md): no mode and no allow rule answers it for you, and
`skip_permissions` does not either.

A yes to this box records no answer for a hook or a server. Each is asked about later, in the
session that first uses it (see [A plugin's hooks](#a-plugins-hooks) and
[A plugin's servers](#a-plugins-servers)).

The listing shows what the reader found, not what you hoped for:

- A hooks file entry the reader cannot use is dropped and counted in the box, not shown.
- A hooks or servers file that cannot be read is said, and nothing is listed for it. Nothing runs
  from it.
- A server entry that states no usable declaration is listed by its alias and marked as never
  started.
- A file the manifest names that is missing refuses the install.

## What an install does

A yes copies the plugin to `~/.bravebot/plugins/<name>/` and writes a record beside it, at
`~/.bravebot/plugins/<name>.record`. The record holds the source, for a git source the ref you asked
for and the commit it resolved to, the version, whether the
plugin is enabled and the SHA-256 of the copied tree. `/plugin list` reads that record, prints the
commit of a git source, and starts no program. A new plugin is enabled. The install itself writes
nothing about a hook or a server: no line goes to `~/.bravebot/hooks-approved` or
`~/.bravebot/mcp-approved`.

An install is refused, and nothing is copied, when:

- a plugin of that name is already installed from another source, or from the same one (the same
  URL and ref for a git source). For the same source the refusal points at `/plugin update <name>`
- the manifest has a key that names a decision: a permission rule, an allow list, a grant, a trusted
  path, a mode, a model, an endpoint or a setting. The refusal names the key, and the manifest is
  refused whole
- a path for `skills`, `agents`, `hooks` or `servers` is absolute, holds `..`, names nothing in the
  plugin, or is or contains a link that leaves the plugin. The refusal names the manifest key. The copy never follows
  a link
- the folder has another tool's manifest, such as `.claude-plugin/plugin.json`, and no
  `plugin.toml`. The refusal says `plugin.toml` is missing
- another installed plugin already offers a skill or definition of the same name. The refusal names
  both plugins, and you remove or disable one first

## Where its skills and definitions rank

Plugin skills and definitions are the least specific source. A skill or definition of the same
name from bravebot itself, `~/.bravebot`, a directory in `sharedSkillDirectories` or a project
shadows the plugin's, so what you wrote by hand wins over what a plugin carries. A plugin's skills
get no prefix naming the plugin. `/skills` shows `(plugin)` beside one.

A disabled plugin offers nothing, and neither does a `plugins` folder inside a project, including
`.bravebot/plugins`: only `~/.bravebot/plugins` is read.

Only the skills and definitions are read into a turn as instructions. The manifest's description, a
README and any other file in the plugin are not read into one.

## A plugin's hooks

A plugin's hooks file is read from its own folder under `~/.bravebot/plugins`, and a hook in it runs
only after you answer a question. The hooks are the same kind as [your own](hooks.md): a program
and its arguments, told only the moment, and they decide nothing.

The question is asked at the first moment the hook would fire, in a session you are watching. It
names the plugin, the moment and the full program and arguments, each word behind the untrusted
content margin, and it is asked at most once per session.

- `y` runs the hook and writes one line to `~/.bravebot/hooks-approved`: a digest of the plugin
  name, the moment, the program and arguments (with `${PLUGIN_ROOT}` already replaced) and the
  digest of the plugin's folder.
- `n` records nothing, the hook does not run, and it is asked again in a later session.

No permission mode answers this question, `--dangerously-skip-permissions` included. In an
[incognito session](../using/incognito.md) it is asked, and a yes lasts only until the session
ends, since nothing is written. Where nobody can be asked, no plugin hook fires. That is a one-shot
run (`-p`), the desktop app, an editor over ACP and the local host. `--safe` reads no plugin hook.

These moments fire a plugin's hook:

| Moment | For a plugin hook |
|---|---|
| `turn-started`, `turn-finished`, `checkout-made`, `tool-finished`, `compacted` | after the question |
| `session-started` | in the full-screen interface and in lines (`--plain`), after the question |
| `file-changed` | in the full-screen interface only, after the question |
| `prompt-shown` | only for a hook you already answered yes to |
| `session-ended` | only for a hook you already answered yes to, and it never asks |

A `file-changed` entry whose `path` is absolute, holds `..`, is empty or has a drive prefix is not
watched. A `file-changed` question waits while you have a draft in the input box, so a key you
type is not taken as the answer.

## A plugin's servers

A server in a plugin's servers file is known as `<plugin>_<alias>`, so `weather` in a plugin named
`demo` is `demo_weather`. Installing and enabling the plugin is the request for it: there is no
`mcp.request` to add. It is asked about the first time it is used, with the same question as a
server you added with `bravebot mcp add` (see [MCP servers](mcp-servers.md)), and with these
differences from a server you declared yourself:

- `--dangerously-skip-permissions`, a one-shot run and a recorded "all future servers in this
  project" answer do not approve it. Only a yes to that server does.
- A server you declared in `mcp.json` under the same alias wins over the plugin's, and a note says
  so.
- A plugin server is not moved if its remote host redirects.
- `bravebot mcp approve` does not approve it. That command reads `~/.bravebot/mcp.json` only. A plugin
  server is approved by a yes the first time a session at a terminal asks about it.

`bravebot mcp list` and `bravebot doctor` list the servers of every enabled plugin as a second block
after the servers you declared, under the heading `carried by installed plugins`. Each row is the
server's `<plugin>_<alias>`, its transport, whether it is approved and the digest a yes is recorded
against. Under it a line names the plugin that carries it, and a line says what a session at a
terminal does: asks first, starts it unasked after an earlier yes, or does not start it, with the
reason. A server nobody has said yes to is listed as unapproved. A yes you gave about a server you
declared yourself, or the answer "all future servers in this project", does not make a plugin server
approved, so neither shows as approved.
`bravebot mcp get demo_weather` shows the declaration in full, names the plugin and shows the whole
digest and whether the server is approved. `/mcp list` names each plugin server that is not running by
its name, with whether it was turned off in this session. `/mcp reconnect demo_weather` starts it,
asking the first-use question where you have not said yes to that server, and
`/mcp disable demo_weather` turns it off for the session. `/mcp remove`
acts on declared servers only: remove a plugin with `/plugin remove demo`.

A plugin you turned off with `/plugin disable` is named on one line, and its servers are not read or
listed. A plugin whose folder no longer matches what was installed is listed with the sentence that
says so under each of its servers, and that does not make `list` or `doctor` fail. A server whose name
is too long once the plugin's name is put before it, or whose entry cannot be used, is listed with what
is wrong and makes `list` and `doctor` fail. Where `mcp.json` declares the same name, only your server
is listed, with a line saying the plugin's is not used. Listing starts no server, asks no question
and writes no file.

A yes is recorded as a `plugin` line in `~/.bravebot/mcp-approved`.

## When the plugin's folder changes

Before a hook fires or a server starts, the plugin's folder is hashed again. If it no longer matches
the digest recorded at install, the hook or server is stopped and no question is put. A notice names
the plugin once per session, in a plain sentence: "A hook of the plugin demo did not run: the
plugin's files no longer match what was installed. Run /plugin update demo to use its hooks again if
its source holds the files you want, or remove the plugin and add it again." The sentence for a
server says the same of a server. To use the plugin's hooks or servers
again, run `/plugin update <name>` if its source holds the files you want (see
[Updating a plugin](#updating-a-plugin)), or remove the plugin and install it again.

`/plugin disable` fires no hook and starts no server, and keeps your answers in `hooks-approved` and
`mcp-approved`. `/plugin enable` asks nothing new for a folder that has not changed.
`/plugin remove` withdraws the plugin's lines from `hooks-approved` and its `plugin` lines from
`mcp-approved` before it deletes the folder and the record, so installing identical files again is
asked about again.

## Updating a plugin

`/plugin update <name>` reads the plugin's record and fetches its source again: a git source at the ref
recorded for it (the default branch where none was named), a local path from the same path. For a git
source you are first asked about the host, as for an install, and no connection is made before you
answer `y`. No permission mode and no allow rule answers it.

- If the source holds the files that are installed, nothing is changed and nothing is asked. If the ref
  now points at a newer commit with the same files, the notice says so and the record keeps the older
  commit.
- If a source was added at a full commit, there is nothing to update to, and the notice says so before
  anything is fetched.
- If the files differ, a box shows the plugin's name, its old and new version, its source and the old
  and new commit, then each skill, definition, hook and server that was added, removed or changed, and
  the files that differ. It is drawn behind the same margin as the install box, and `y` updates. `n`,
  `Esc` and any other key leave the plugin as it was.
- If the update would offer a skill or definition name that another installed plugin offers, it is
  refused, naming both plugins, and nothing is asked.
- Where nobody can be asked, such as a one-shot run, nothing is fetched and nothing is updated.

A yes replaces the plugin's folder and its record. The plugin stays enabled or disabled as it was. The
answers recorded for all of its hooks and servers are withdrawn, so each hook and each server asks
its question again the next time it is used, whether or not its own entry changed. Nothing updates a
plugin on its own, and starting a session makes no request for one.

## `${PLUGIN_ROOT}`

In a hook's program and arguments, and in a server's command and arguments, `${PLUGIN_ROOT}` is
replaced with the absolute path `~/.bravebot/plugins/<name>`. This is done before the install
listing is drawn and before any digest is taken, so you read, and approve, the path the plugin will
have. It is replaced nowhere else: not in an environment value, a url, a file a server may read, a
directory, or a hook's `path`. It is not looked up in your environment.

## Trust

A plugin skill is trusted for what a skill in `~/.bravebot/skills` is trusted for, and a definition
is held as one in `~/.bravebot/agents` is. A plugin hook runs with your own access, as your own hook
does, and only after you say yes to it. A plugin server is held to the confinement of any server
you declared, and starts only after you say yes to it. A plugin carries no decision: installing one does not
change a permission rule, a trusted path or a mode, and a definition cannot name more than its
kind allows.

:::caution
A plugin's skills and definitions are instructions your sessions follow, and what you were shown is
what you are trusting. Read the plugin before you say yes, in the same way you would read a skill
you copied into `~/.bravebot/skills`. A hook is a program, so read its program and arguments, and
the script they name, before you answer its question.
:::

## Known costs and limits

- **A plugin hook declared only on `session-ended` fires only after an earlier yes.** That moment
  never asks, so the hook needs an answer given at another moment or in an earlier session.
- **A `file-changed` question waits while you have a draft.** The hook does not run until the input
  box is empty.
- **The desktop app and ACP cannot approve a plugin hook.** No plugin hook runs there.
- **A notice is a plain sentence.** It names the plugin and is not drawn behind the untrusted
  content margin.
- **A changed plugin folder stops its hooks and servers until it is updated.** Run
  `/plugin update <name>`, or remove the plugin and install it again.
- **Nothing looks for a newer version.** A plugin stays as installed until you type
  `/plugin update <name>`.
