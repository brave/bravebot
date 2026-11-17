---
id: PLUGIN
title: Plugins
status: proposed
governs:
  - crates/config/src/hooks.rs
  - crates/config/src/mcp.rs
documented-by:
  - none (gap: a page on installing a plugin, owed until the design is built)
---

## Scope

One person-run command that installs a bundle of skills, delegate definitions, hooks and MCP server
declarations from one source, what the person is shown before it does, where each part lands, what a
plugin can never carry, and how an installed plugin is updated, turned off and removed.

Nothing here is built. A clause in this spec reads `verified-by: none` until the work it describes
lands. The points where it changes a built spec are named in
[What this changes elsewhere](#what-this-changes-elsewhere), and none of those specs is edited until
the work lands.

A **plugin** is a directory with a `plugin.toml` at its root. A **source** is a git URL or a local path
that holds one. The **install** is `/plugin add`, which a person types. There is no marketplace, no
catalog and no index of plugins: a person names the source.

Where a skill, a definition, a hook or a server is read from, and what each is trusted for, is
[skills.md](skills.md), [delegation.md](delegation.md), [hooks.md](hooks.md) and
[mcp-servers.md](mcp-servers.md). What the trust map records is [trust-map.md](trust-map.md), and what a
fetch asks is [tools/fetch-url.md](tools/fetch-url.md) and [network-egress.md](network-egress.md). The
prompts a person is asked are in [prompting.md](prompting.md). The rule that untrusted content never
reaches the driver or the planner is [AGENTS.md](../../agents/AGENTS.md).

## What exists today

A person installs each piece by hand: a skill directory under `~/.bravebot/skills`, a definition at
`~/.bravebot/agents/<name>.md`, entries in `~/.bravebot/hooks.json`, and `bravebot mcp add` for a
server. Each is trusted on its own terms. Nothing names a bundle, a version or an update, and
[import.md](import.md) lists opencode's `plugin` key as not imported.

## Clauses

<a id="PLUGIN-1"></a>
### PLUGIN-1: a plugin is installed only by a command a person types

`/plugin add <source>` is the only way a plugin is installed. `/plugin update`, `/plugin remove`,
`/plugin enable`, `/plugin disable` and `/plugin list` are the others. No tool the planner calls
installs, updates, removes, enables or disables one, no checkout file requests one, and a line
arriving through a quarantined source (a pasted block, a fetched page, a file in a workspace) is not
the typed command even where it reads as one.

**Why.** An install puts programs and instructions into the directory every later session trusts by
provenance. Whether that happens is a decision about where bytes land and what runs afterwards, and
only a person's own keystrokes are a source for it that no content can author.

`verified-by: none`

<a id="PLUGIN-2"></a>
### PLUGIN-2: `plugin.toml` names its parts, and a part it does not name is not installed

The manifest holds a `name` (letters, digits and hyphens, the directory name under
`~/.bravebot/plugins`), a `version`, a `description`, and optional paths for `skills`, `agents`,
`hooks` and `servers`. `skills` and `agents` are directories, `hooks` is one file and `servers` is one
file. These four are the only kinds of part. Every path is relative to the plugin's root and stays
inside it: an absolute path, a `..` and a link leaving the directory are refused, and the install
stops, naming the manifest key.

A directory the manifest does not name is not scanned for parts. A key this build does not read is
reported and ignored. A manifest with a key that names a permission rule, an allow list, a grant, a
trusted path or a setting is refused whole ([PLUGIN-6](#PLUGIN-6)).

`plugin.toml` is the only manifest read. A directory holding another tool's manifest and no
`plugin.toml` is refused, and the refusal names `plugin.toml` as the missing file.

**Why.** A reader that finds parts by folder name installs whatever the author left lying in the
checkout. A part the person was shown has to be one the manifest named. A second manifest format would
need a translation from another tool's hook moments and server fields into this one's, and the person
would be shown the translation rather than the file.

`verified-by: none`

<a id="PLUGIN-3"></a>
### PLUGIN-3: fetching a source is a network effect, and the host is put to the person before it happens

A git URL is cloned shallow into a staging directory outside `~/.bravebot` and outside every workspace.
The host is shown and asked about through the same prompt a fetch uses, and no request leaves before
the yes. Only `https` and `ssh`-style git URLs and local paths are sources. A redirect to another host
is not followed. A source that is an archive, an npm package or a plain URL is refused.

The clone is made by the `git` program, started the way [SANDBOX-30](sandboxing.md#SANDBOX-30)
requires, with the variables that select code removed from its environment, no submodule fetched and no
hook from the cloned repository run. A local path is read in place, copied into staging and never
executed there.

The staged bytes are untrusted content. Nothing in them is read by the driver or the planner before the
person has seen what [PLUGIN-4](#PLUGIN-4) lists, and the listing is built by a reader that parses the
manifest and the parts as data. No part of the staged tree is executed, loaded or followed to decide what
is listed.

**Why.** The download is the first moment untrusted bytes exist, and the person's yes to the host is
the same decision a fetch of a page asks. A staged tree that something already ran would have decided
for the person.

`verified-by: none`

<a id="PLUGIN-4"></a>
### PLUGIN-4: the install lists every part in full and asks once

Before anything is copied, the person is shown the plugin's name, version, source and resolved commit
([PLUGIN-14](#PLUGIN-14)), and then each part: every skill's name and description, every definition's
name and kind, every hook's moment and argv, every server's transport, argv or url and host, the
variables it names and the files it may read. Anything the part would reach that
[network-egress.md](network-egress.md) or [permissions.md](permissions.md) would otherwise ask about (a
host, a command, a path outside the workspace) is on its own row. A skill or definition that a source
of higher rank would shadow ([PLUGIN-12](#PLUGIN-12)) is marked as shadowed. The text drawn from the
plugin is drawn inside the margin untrusted content in a prompt is drawn inside, and is cut by what a
person can read, with the rest of a long part shown on request and never dropped.

The answer is yes or no. A yes installs every part, and the prompt says plainly what it does not do:
a hook is not made to run and a server is not made reachable by it, which each reach through their own
first-use question ([PLUGIN-7](#PLUGIN-7)). No mode answers this prompt and no allow rule does. Where
nobody can be asked, such as a one-shot run, nothing is installed.

**Why.** A person cannot vouch for a thing they were not shown, and a prompt listing the plugin's own
summary of itself would let the plugin choose what the person reads. The prompt is built from the
parts, not from a field the manifest wrote.

`verified-by: none`

<a id="PLUGIN-5"></a>
### PLUGIN-5: a plugin installs at user scope, into one directory the person's own, and a workspace plugin is inert

The install copies the staged plugin to `~/.bravebot/plugins/<name>/` and records its source, the
requested and resolved ref ([PLUGIN-14](#PLUGIN-14)), whether it is enabled
([PLUGIN-11](#PLUGIN-11)) and the SHA-256 of the copied tree beside it. Reading that directory is
trusted by provenance, as `~/.bravebot/agents` is. A plugin directory found in a workspace, including
one named `.bravebot/plugins`, offers nothing, and is a file read through the trust map like any other
and therefore not trusted until the map says so.

An install whose name is already installed from another source is refused rather than replaced, naming
both sources. An install of the same source is an update ([PLUGIN-9](#PLUGIN-9)).

There is no project scope and no setting that makes a plugin required for a team.

**Why.** The state directory is the one place a clone cannot reach, which is the property every
single-file spec of the declarations depends on. A project scope would put plugin parts where a checkout
authors them.

`verified-by: none`

<a id="PLUGIN-6"></a>
### PLUGIN-6: a plugin carries no decision

A plugin declares no permission rule, no allow, ask or deny entry, no grant, no trusted path, no mode,
no model, no endpoint and no setting. A part that is a skill, a definition or a hook is held to what
that part already may say: a definition cannot name a capability set beyond its kind, and a hook decides
nothing. Installing a plugin does not change the permission rules in force, the trust map or the mode
of any session.

**Why.** A plugin is outside content a person chose to trust, and the choice was to read what it lists.
A permission rule is a decision taken from something a person wrote, and a plugin is not that person.

`verified-by: none`

<a id="PLUGIN-7"></a>
### PLUGIN-7: installing a part does not approve it

A skill from a plugin is trusted for what a skill in `~/.bravebot/skills` is trusted for, and no more.
A definition from a plugin is one `~/.bravebot/agents` would hold. A server from a plugin is reachable
only after the first-use question that names its digest, as a server added by `bravebot mcp add` is,
and the install records no answer to it.

A hook from a plugin fires only after the person has answered a question naming its moment and its
argv as [PLUGIN-13](#PLUGIN-13) expands it. The question is asked at the first moment the hook would
fire, in a session a person is watching, and at most once in a session. A yes is recorded in
`~/.bravebot/hooks-approved` as a digest of the plugin's name, the hook's moment, its expanded argv and
the plugin's tree digest. A no records nothing, the hook does not fire, and the question is asked again
in a later session. Where nobody can be asked, the hook does not fire.

Before a hook fires or a server starts, the plugin's directory is hashed again. A tree whose digest is
not the one recorded at install stops the hook or the server, and the person is told which plugin no
longer matches its record. Changing a hook's moment or argv, or any file in the plugin, is a question
again after an update the person accepted.

**Why.** The install is one yes to a list. Running a program the first time is a decision about that
program, and it is asked about where it runs, in a session the person is watching. A hook is usually an
argv that starts a script in the plugin, so an approval bound to the argv alone would still hold after
the script changed.

`verified-by: none`

<a id="PLUGIN-8"></a>
### PLUGIN-8: withdrawn, there is no marketplace

Replaced by [PLUGIN-14](#PLUGIN-14) and [PLUGIN-5](#PLUGIN-5). No index of plugins exists and none is
registered. A source is a git URL or a local path the person names, and the pin that an index entry
would carry is the resolved commit and the tree digest those two clauses record.

<a id="PLUGIN-9"></a>
### PLUGIN-9: update, remove and list work from the recorded source

`/plugin update <name>` fetches the recorded source again at the recorded ref
([PLUGIN-3](#PLUGIN-3), [PLUGIN-14](#PLUGIN-14)) and, when the tree's digest differs, puts the
difference to the person the way [PLUGIN-4](#PLUGIN-4) puts a first install: each part added, removed or
changed, in full. A part whose name another installed plugin already offers ([PLUGIN-12](#PLUGIN-12))
refuses the update, naming both plugins. Nothing is replaced before the yes, and an answer given to a
hook or a server before is withdrawn for any part whose digest changed. No plugin updates on its own and
no session start checks for one.

`/plugin remove <name>` deletes the directory and its record, and withdraws what was recorded for its
hooks and servers. `/plugin list` prints each installed plugin's name, version, source, resolved commit,
recorded digest and whether it is enabled, reads nothing but that record and starts no program.

**Why.** An update that applied itself would be a fetch nobody asked for followed by changed programs
nobody saw.

`verified-by: none`

<a id="PLUGIN-10"></a>
### PLUGIN-10: a plugin's text reaches a turn only as the part it is

A skill's name and description reach the prompt as [skills.md](skills.md) says, and its body waits for
`load_skill`. A definition is a delegate's definition. The manifest's `description`, a README, and any
other file in the plugin are not read into a turn. Nothing the plugin says about itself is a source of
instructions.

**Why.** A plugin directory holds files that are not parts, and a reader that put them in a context
would be a second way for the plugin to speak that no prompt showed.

`verified-by: none`

<a id="PLUGIN-11"></a>
### PLUGIN-11: a plugin can be turned off and kept

`/plugin disable <name>` sets the plugin's recorded state to disabled. A disabled plugin offers no
skill, no definition, no hook and no server, and its directory, its record and every answer recorded
for its hooks and servers stay as they were. `/plugin enable <name>` sets the state back and asks
nothing new: a part whose digest equals the one answered for is as approved as before, and any other
part asks as [PLUGIN-7](#PLUGIN-7) says. Both commands are typed ([PLUGIN-1](#PLUGIN-1)). A newly
installed plugin is enabled.

**Why.** A person who wants a plugin out of the way for a while should not have to remove it and answer
every question again.

`verified-by: none`

<a id="PLUGIN-12"></a>
### PLUGIN-12: a plugin's skills and definitions rank last, and two plugins never offer the same name

A skill or definition from a plugin is the least specific source of its kind. A file of the same name
from `~/.bravebot`, a directory the person named ([INSTR-17](instructions.md#INSTR-17)) or a project
shadows it, by the shadowing of [INSTR-4](instructions.md#INSTR-4). A name offered by an installed
plugin that another plugin's part also carries is refused: the install names both plugins and stops,
and a person removes or disables one first. A plugin's skills are not given a prefix naming the plugin.

**Why.** What a person wrote by hand should win over what a stranger's plugin carries under the same
name, and a rule that picks between two plugins would make which one answers depend on the order they
were installed in.

`verified-by: none`

<a id="PLUGIN-13"></a>
### PLUGIN-13: a part may name its own directory with one variable

In the argv of a hook and the command and arguments of a server, the text `${PLUGIN_ROOT}` is replaced
with the absolute path of `~/.bravebot/plugins/<name>`. It is replaced before the digest of
[PLUGIN-7](#PLUGIN-7) is computed and before the install listing is drawn, so the listing and the
approval show the path that will run. No other variable is replaced, the text is not looked up in the
environment, and a `${PLUGIN_ROOT}` anywhere else in a part is left as written. A plugin has no
directory for data of its own.

**Why.** A hook that runs a script the plugin carries cannot be written without a path to the plugin.
Expanding it in the listing keeps what the person approved identical to what runs.

`verified-by: none`

<a id="PLUGIN-14"></a>
### PLUGIN-14: a source may name a ref, and the commit it resolved to is recorded

A git source may end in `#<ref>`, a branch, a tag or a full commit. The install records the ref asked for
and the full commit it resolved to, shows both ([PLUGIN-4](#PLUGIN-4)), and records the tree digest as
[PLUGIN-5](#PLUGIN-5) says. `/plugin update` fetches the recorded ref again and compares the commit and
the digest. A source naming a full commit has nothing to update to. A local path has no commit and is
recorded with its digest.

**Why.** Without an index to pin a source, the commit the person was shown is the pin, and an update is
a different commit the person is shown again.

`verified-by: none`

## What this changes elsewhere

These are the places a built spec says something this spec would need to extend, each decided when the
work lands, in the same commit as the code.

- **Hooks.** The hooks spec says one file in `~/.bravebot` declares hooks and no other file does. A
  plugin's hooks file is read from the plugin's directory under that state directory, and its entries
  fire only after the question in [PLUGIN-7](#PLUGIN-7), recorded in `~/.bravebot/hooks-approved`. The
  hooks spec needs a clause for that question, since today it has none.
- **Skills and definitions.** Each source list gains `~/.bravebot/plugins/<name>/...` as its least
  specific entry ([PLUGIN-12](#PLUGIN-12)), with the trust of the sibling `~/.bravebot` source.
- **MCP servers.** The declaration file is `~/.bravebot/mcp.json` and no other, and a plugin's server
  list is read alongside it with the same digest and first-use question.
- **The trust map.** It does not govern the plugin directory, for the reason it does not govern the rest
  of `~/.bravebot`.
- **Programs bravebot starts.** The `git` that clones a source is added to the programs
  [SANDBOX-30](sandboxing.md#SANDBOX-30) lists.
- **Import.** A conversion of another tool's plugin layout would belong to [import.md](import.md). Nothing
  in this spec reads one.

## Known costs

- A person installing a plugin sees a long listing. The listing is the protection, and a short one would
  be the plugin's own summary.
- A plugin cannot ship a permission rule, so the readme of a plugin that wants a command allowed has to
  tell a person to write the rule.
- No project scope, no "required for a team" setting and no managed list of allowed sources exist. A
  team shares a plugin by sharing its source and every person runs the install.
- Nothing lists plugins to choose from. A person finds a source elsewhere, and a pin says the bytes are
  the ones shown, not that they are good.
- A plugin's own programs have no directory for data and no per-plugin settings.
- A plugin written for another tool does not install here, because only `plugin.toml` is read.
