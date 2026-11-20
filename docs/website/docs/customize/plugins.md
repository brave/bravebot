---
sidebar_position: 9
title: Plugins
description: Install a bundle of skills and agent definitions from a folder on this machine or a git URL with /plugin add, see what it shows you first, list, turn off and remove plugins, and what a plugin can never carry.
---

# Plugins

A plugin is a folder with a `plugin.toml` at its root that bundles skills and agent definitions
under one name. `/plugin add <path>` installs one from a folder on this machine, and
`/plugin add <git url>` or `/plugin add <git url>#<ref>` installs one from a git repository. You type
the command, and you are shown every skill and definition in the plugin before anything is copied.

`/plugin update`, hooks, servers and the `${PLUGIN_ROOT}` variable are not
built. A `hooks` or `servers` path in a manifest installs nothing.

## The commands

| Command | What it does |
|---|---|
| `/plugin add <path>` | Reads the folder at `<path>`, shows what it holds and installs it if you say yes |
| `/plugin add <git url>[#<ref>]` | Asks about the host, fetches the repository, shows what it holds and installs it if you say yes |
| `/plugin list` | Prints each installed plugin's name, version, source, resolved commit, digest and whether it is enabled |
| `/plugin remove <name>` | Deletes the plugin's folder and its record |
| `/plugin disable <name>` | Keeps the plugin installed and stops it offering anything |
| `/plugin enable <name>` | Turns a disabled plugin back on, and asks nothing new |

These are commands you type and nothing else runs them. No tool the model is offered installs or
changes a plugin, and a `/plugin add` line that arrives inside pasted text is text in the box, not
the command. A one-shot run has no `/plugin` command and installs nothing, since nobody can be
asked there. See [`/plugin`](../reference/commands.md) for the forms.

A relative `<path>` is read from the session's workspace folder. An installed plugin is yours and
is offered in every project.

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
```

`name`, `version` and `description` are required. The name is letters, digits and hyphens only,
because it is the folder name the plugin is installed under. `skills` names a folder of skills
(`skills/<name>/SKILL.md`, as in [Skills](skills.md)) and `agents` names a folder of definitions
(`agents/<name>.md`, as in [Agents](agents.md)). Both are optional, and a folder the manifest does
not name is not read for parts, whatever it is called.

A key this build does not read is reported and ignored.

## What you are shown

`/plugin add` copies the folder to a staging directory first and reads it there as data. Nothing in
it is run. It then asks once, in a box that shows:

- the plugin's name, version and source (a path, or the git URL)
- for a git source, the ref you asked for and the full commit it resolved to
- each skill's name and description
- each definition's name and kind
- a mark on any skill or definition that another source already offers under the same name, saying
  which one, since that one is what a session gets and not the plugin's
- a sentence saying that yes does not make a hook run or a server reachable

All of it is text the plugin's author wrote, so it is drawn inside the margin used for untrusted
content, with control and invisible characters made visible. A long description is cut after a few
lines and shown in full when you press `e`, and you can answer yes only once nothing is cut. `y`
installs. `n` or `Esc` installs nothing and copies nothing. The box is the same in every
[permission mode](../security/permission-modes.md): no mode and no allow rule answers it for you, and
`skip_permissions` does not either.

## What an install does

A yes copies the plugin to `~/.bravebot/plugins/<name>/` and writes a record beside it, at
`~/.bravebot/plugins/<name>.record`. The record holds the source, for a git source the ref you asked
for and the commit it resolved to, the version, whether the
plugin is enabled and the SHA-256 of the copied tree. `/plugin list` reads that record, prints the
commit of a git source, and starts no program. A new plugin is enabled.

An install is refused, and nothing is copied, when:

- a plugin of that name is already installed from another source, or from the same one (the same
  URL and ref for a git source), since updating is not built
- the manifest has a key that names a decision: a permission rule, an allow list, a grant, a trusted
  path, a mode, a model, an endpoint or a setting. The refusal names the key, and the manifest is
  refused whole
- a path for `skills` or `agents` is absolute, holds `..`, names nothing in the plugin, or is or
  contains a link that leaves the plugin. The refusal names the manifest key. The copy never follows
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

Only the skills and definitions reach a turn. The manifest's description, a README and any other
file in the plugin are not read into one.

## Trust

A plugin skill is trusted for what a skill in `~/.bravebot/skills` is trusted for, and a definition
is held as one in `~/.bravebot/agents` is. A plugin carries no decision: installing one does not
change a permission rule, a trusted path or a mode, and a definition cannot name more than its
kind allows.

:::caution
A plugin's skills and definitions are instructions your sessions follow, and what you were shown is
what you are trusting. Read the plugin before you say yes, in the same way you would read a skill
you copied into `~/.bravebot/skills`.
:::
