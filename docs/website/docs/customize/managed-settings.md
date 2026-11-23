---
sidebar_position: 15
title: Settings pinned by an administrator
description: Administrator-managed settings, configuration precedence, pinned request destinations, model and MCP server restrictions, and action policy.
---

# Settings pinned by an administrator

The other settings files and how they combine are described under
[`settings.json`](configuration.md#settingsjson).

One file answers **above the process environment**, and therefore above every other source:

| Platform | Path |
|---|---|
| Linux | `/etc/bravebot/managed.json` |
| macOS | `/Library/Application Support/bravebot/managed.json` |
| Windows | `C:\ProgramData\bravebot\managed.json` |

The path is a literal, and no variable names it. `%ProgramData%` and the rest are stated in the
environment of the person this layer binds, so reading one would let them choose which file answers for
them.

This inverts the rule that the environment wins, deliberately. That rule exists so a released binary
can be pointed at a local backend without rebuilding it, and a pin an exported variable outranked
would pin nothing. An organisation requiring that inference traffic reach an approved endpoint, or
refusing to have models reached through somebody's personal cloud account, would otherwise have no way
to say so.

**Only these names may be pinned**, being the ones that decide where a request goes:
`BRAVE_AI_CHAT_ENDPOINT`, `BRAVE_AI_CHAT_PREMIUM_ENDPOINT`, `BRAVEBOT_USE_BEDROCK`, `AWS_REGION`,
`AWS_PROFILE`, the three `ANTHROPIC_DEFAULT_*_MODEL` tiers, and the `provider` block. A project or
local settings file in a checkout cannot name any of these variables, so what a pin outranks is the
person's own file, the environment and the build. Every other name
in the file decides nothing, the signing key and key id included, save the two pairs of lists and the
two refusals: `"mcp": { "allow": [...], "deny": [...] }` name the
[MCP servers](mcp-servers.md#refused-by-an-administrator)
a session on the machine may start and may not, by host or by command. The file can keep a server
from starting and never add one. A name it does not pin resolves exactly as it would with no such
file.

`"models": { "allow": [...], "deny": [...] }` name the models a machine may request and may not, read
on the same footing:

```json
{
  "env": { "BRAVEBOT_USE_BEDROCK": "1", "AWS_PROFILE": "the-org-account" },
  "models": {
    "allow": [
      "arn:aws:bedrock:us-west-2:000000000000:application-inference-profile/approved-opus",
      "arn:aws:bedrock:us-west-2:000000000000:application-inference-profile/approved-sonnet"
    ]
  }
}
```

Pinning the account without this says where the traffic goes and not what it spends: every other model
name still reaches the pinned account and is billed to it. A name here is the model as a request
carries it, an inference-profile ARN or a gateway slug such as `z-ai/glm-4.6`, compared exactly, and a
tier word is checked as the model that tier names. A gateway model has two spellings, such as
`openrouter/z-ai/glm-4.6` and the `z-ai/glm-4.6` the gateway is sent, and an entry naming either one
answers for the model however it was asked for. Without an `allow` list every model not denied is
requested as before; with one, a model no entry names is refused, and `"allow": []` refuses every
model. A deny entry wins over an allow entry naming the same model.

This can only refuse. There is nowhere in the file to hold a roster, so an entry permits a name some
service already offers rather than adding one, and the file cannot say which model is the default.
`/model` lists only what the machine may request. A model you had picked that the lists refuse is
ignored, your `~/.bravebot/model` is left as it is, and the session opens on the configured model and
says so; where that model is refused too the start is refused and names the file. An `AGENTS.md`
definition naming a refused model is refused when it is read, when the planner starts it as a delegate
and when you address it. A skill naming a refused model leaves the turn on the session's model and
says which file refused it.

`"permissions": { "readsStayInWorkspace": true, "bypassUnreachable": true }` are read here on the same
reasoning: each can only take capability away, never add any, so keeping the file tools inside the
working directory or putting the mode that asks about nothing out of reach is something two parties can
have a legitimate say in. Neither is pinned in the sense the variables are. A name in the list above is
answered by this file and nothing else is consulted for it; these two take the strictest answer any
file gave, so somebody may ask for either where this file did not, and no file of theirs can lift it
where it did.

`"permissions": { "startInAuto": true }` is not read here. The key opens sessions in the auto mode, so
only the file in a person's own directory and the file `--settings` names can write it, and a copy in
this file opens nothing. `bravebot doctor` says so where this file names it. The key that would let an
administrator remove the mode for a machine is not built yet
([#3568](https://github.com/brave/bravebot/issues/3568)).

`"crashReports": false` turns crash reports off on the machine, whatever the user's own
`~/.bravebot/settings.json` says. It can only turn them off: `true` is not read, and a value that is
not a boolean is ignored. See [`crashReports`](configuration.md#crashreports).

`"usagePing": false` turns the daily usage ping off on the machine, whatever the user's own settings or
the switch in `/config` say, and the switch is shown as turned off by an administrator. It can only
turn the ping off: `true` is not read. See [`usagePing`](configuration.md#usageping).

`"permissions": { "deny": [...], "ask": [...] }` are rules, written as they are in any settings file, and
they are added to the rules every other file wrote. A deny rule here refuses on every session on the
machine in every mode, and nothing in a checkout or in a person's own file removes it:

```json
{
  "permissions": {
    "deny": ["Read(.env)", "Bash(git push *)"],
    "ask": ["WebFetch"]
  }
}
```

A refusal names this file. A line that is not a rule is reported by `doctor` and in the session, and the
rest of the list still applies. `allow` is never read from here, because a grant is the person's own.

A key written twice in one object of this file, at any depth, is read so that the second copy erases
nothing the first one refused. Two blocks under one key are combined, and a `deny` or `ask` list written
twice keeps the entries of both, as do `models.deny`, `mcp.deny` and the other lists that only refuse. A
repeated `models.allow` or `mcp.allow` keeps the entries both lists name. `doctor` names each repeated
key with this file's path, and never what it was set to.

A layer that can pin a preference is a layer somebody uses to pin one. What two parties have a
legitimate say in is where a request goes, whose account pays for it, and what a session on the machine
may reach; which theme is on and which keys do what are neither.

**No credential is read from this file.** A gateway entry's `apiKey` and `options.keyFile` are dropped
and the entry's host, models and variable names are honoured without them. Everyone on the machine can
read this file, so a token in it is a token handed to every account rather than one held by its owner,
and the layer names the host a token goes to, so it does not also choose which file's content is sent
there. A key stored with `bravebot auth login gateway` and a variable the entry names still supply the
credential. An entry that names only an `options.keyFile` therefore names no credential once the key
file is dropped, as one that names only an `apiKey` does. The service says what
is missing on the first request. A `headers` block is kept, because a header is how this layer names a
tenant or a route. Writing a value as `{env:NAME}` keeps it out of a file every account can read.

**The `provider` block is pinned whole** rather than a name at a time, because pinning an endpoint
pins nothing while anybody may add a destination beside it. A block that is present and empty says
there are no gateways. A file without the block, or one spelling it as anything but a block, leaves the
gateways a person configured in force: taking every gateway on the machine away on the strength of a
stray `null` is the one reading nobody would intend.

Refusing every account but the organisation's therefore takes **both** halves, the switch pinned off
and the `provider` block pinned, since a gateway entry can name an AWS account too.

The authority here is the filesystem's rather than this program's. Nothing checks who owns the file or
what its permissions are: somebody who can write that path can replace the binary. It fails softly like
any other layer, so a file that is missing, over 64 KB or unparseable pins nothing, and a blank or
non-string value pins nothing under that name.
