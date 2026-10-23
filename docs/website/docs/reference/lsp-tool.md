---
sidebar_position: 5
title: The lsp tool
description: Which language server answers, what starting one costs, how a file is checked, and how every answer is labelled.
---

# The lsp tool

Asks a language server about a symbol: where it is defined, what refers to it, what implements it,
what calls it. It also asks what the server makes of a file. **Starting a server is put to you.**
That holds in every permission mode, bypass included, and a start is refused where nobody can be
asked, as in a one-shot run.

| Parameter | |
|---|---|
| `operation` | one of `goToDefinition`, `findReferences`, `hover`, `documentSymbol`, `workspaceSymbol`, `goToImplementation`, `incomingCalls`, `outgoingCalls`, `diagnostics` |
| `path` | workspace-relative file holding the symbol, or the file to check for `diagnostics`; required for every operation but `workspaceSymbol` |
| `line`, `character` | 1-based position of the symbol, as a search or a read reported it; not used by `diagnostics` |
| `query` | for `workspaceSymbol`, the name to look for |

This answers what [`search`](tools.md#search) cannot. A search for a name finds every comment and string that
mentions it; this finds the declaration the compiler agrees on.

**Nothing works out where a name is.** A position is two numbers the planner states, taken from a
line it was already shown. A position that no longer holds the symbol answers with nothing found
rather than an error, since a file changes under an agent and a stale line number is ordinary.

The operation list is closed and every entry on it is a read. LSP is an open protocol and a server
advertises methods of its own, one of which applies edits to your files, so forwarding a name would
make what this tool can reach a property of whichever server you installed.

## Checking a file with `diagnostics`

`diagnostics` takes a path and no position. The agent uses it to learn whether a file still compiles
before running a build. The server is sent the file as it is on disk and the notice it
publishes in reply is the answer:

```text
language server: 2 errors at line 12, 40; 1 warning. Fix them before moving on
```

The agent is told how many errors the server found and the line each starts on, how many warnings
and how many other notes. These are numbers the server reports, so they are given for a file in a
tree you have not vouched for as for one you have. **The messages are not shown to the agent.** A
message is a sentence the server composed out of your files, and it can quote a name defined in
another file, so the file that was checked does not vouch for it. It comes back as a reference the
agent hands to a processor or asks you to show it, as hover text does. While a `deny` rule covers
reading any path, the messages are left out, and the agent is told so. A file that does not exist is
reported as such before any server is started, so you are not asked about one. A location in a file a
`deny` rule covers is left out of any answer, and the answer does not say how many were left out.
While a `deny` rule covers reading any path, hover text is withheld.

A server that publishes nothing about the file in a few seconds is reported as not having reported,
which is not the same as a file with no errors, and a report with no errors says "no errors reported
yet" because a server can refine a quick pass later. A server still indexing says the report may be
short. A server that publishes late, or only when its diagnostics change, can stay silent about an
edit, so a build is still the check that does not depend on the server.

A `write_file` or `edit_file` result already carries the same counts and lines when a server for that
file's language is running, and never starts one. `diagnostics` is the way to ask for them for a file
that was not just written or edited, and it starts the server after you agree to it.

## Which server, and what starting one costs

| Language | Server |
|---|---|
| Rust | `rust-analyzer` |
| TypeScript and JavaScript | `typescript-language-server` |
| Python | `pyright-langserver` |
| Go | `gopls` |

The binary has to be installed and on your `PATH`, in a directory that a program the agent runs cannot
write. A server found only in the session's own directory, such as a `.venv/bin/pyright-langserver`
or a `node_modules/.bin` entry, is reported as not installed. A command written with a path in it is
your own choice and is not looked up. A language not in the table has no server until
you declare one, [below](#declaring-a-server-of-your-own).

**A server runs with the access your own shell would give it, and is not confined.** The prompt says
so in those words: it reads the whole tree and the dependency sources, and starting it runs code
from your project and its dependencies with your own access. For Rust that means `build.rs` and proc
macros out of `Cargo.lock` execute, which is code from your dependency tree running as you. Go's
tooling builds to answer too. The TypeScript server runs the TypeScript your project carries: it
looks for `typescript/lib/tsserver.js` under `node_modules`, `.yarn/sdks`, `.pnpm/sdks` or
`.vscode/pnpify` in the workspace and then in each directory above it, and runs the nearest one
before its own copy. The Python server runs the `python3` on your `PATH`, which runs the `.pth` files
of its environment as it starts: with your project's virtual environment active, those come from the
project's dependencies.

Confinement is not an option withheld here. A server indexes *by* running that build tooling, so a
profile denying it a subprocess and somewhere to write gives you a server whose index never settles,
and every answer from one says it may be short. The choice is a server with your access or no
working tool.

What does not turn on your answer is the label on what comes back, and that is the half that
matters. Hover text is quarantined, and nothing a server reads out of a file reaches the planner as
something bravebot said.

**A filename is content, and a name that is shown is bounded.** An answer is labelled by the files it
names, as [below](#a-position-is-structure-a-name-and-the-text-at-it-are-content) describes. A file
can be named to read like an instruction, so where every file an answer names is one you vouched for,
the name is shown with its control characters replaced by pictures (`␊` for a newline), one location
is always one line, and an answer stops at two hundred locations and says that it did. What that
leaves is a name out of a tree you vouched for, read as a name.

A server is started by the first question that needs it, so a session that asks nothing about a
language starts nothing.

**You are asked once per language per session, not once per call.** The server that first question
starts is kept for the session and answers every later question, and your approval is kept with it.
It is shut down when the session ends, and killed if it does not go quietly. Indexing is the whole
cost of a server, so paying it per request would make each call slower than the search it replaces.

A `checker` or `worker` delegate asks the same servers, so a language you approved is not put to you
again and the tree is not indexed twice. A `reader` delegate is not offered `lsp` at all: starting a
server runs the project's build tooling, and a reader may not run programs. A delegate working in a
[checkout](../customize/agents.md#a-checkout-of-its-own) is not offered it either, since the server
indexes your working tree.

## Declaring a server of your own

A C, C++, Java, Kotlin, Ruby, PHP, Swift, Lua or C# project needs a server the table does not hold.
Declare one in `~/.bravebot/lsp.json`:

```json
{
  "servers": {
    "clangd": {
      "command": "clangd",
      "args": ["--background-index"],
      "extensions": { "c": "c", "h": "c", "cpp": "cpp", "hpp": "cpp" },
      "env": { "CLANGD_FLAGS": "--log=error" },
      "initializationOptions": {}
    }
  }
}
```

| Key | |
|---|---|
| `command` | a program name looked up on your `PATH`, or an absolute path; a relative path is refused |
| `args` | arguments, optional |
| `extensions` | each file extension the server handles, with the language id the protocol wants; required |
| `env` | variables set for it, optional |
| `initializationOptions` | sent to the server as written, optional |

**Only this file is read.** A `lsp.json`, `.lsp.json` or `.github/lsp.json` in a project changes
nothing, because a command in a file a project carries is a command a tool call could have written.

A declared extension wins over the table, so declaring `rs` replaces `rust-analyzer`. Where two
declarations name one extension, the first by name wins. An extension nothing names answers that no
server is configured.

**The prompt names the whole command and says what starting it runs is not known.** It does not
say the server runs nothing, and it does not say it runs your dependencies' code, because
bravebot did not choose the program. It runs with your own access and is not confined. Your
answer is kept for the session against the declaration as it stood: edit the file and the next
question is put to you again.

A declared server is settled once the progress it reported has ended. One that reports no progress
is waited on for twenty seconds once, and every answer after that says it may be short.

## A position is structure; a name and the text at it are content

A location is a path, a line, a column and the kind of symbol. **The line and the column are
structure**, as a line count is for a file the planner may not read: there is nowhere in two integers
for prose to sit. **The name is content.** It is bytes out of the tree the server indexed, so an
answer is labelled by the files it names. Each path is looked up in the trust map. An answer that
names only files you vouched for is read as written. An answer that names any file you did not vouch
for comes back whole as a reference, with none of its locations in front of the planner, so somebody
who owns `vendor/lib.js` cannot use `goToDefinition` to put a sentence in front of the planner
through its name. A `goToDefinition` into a dependency's source, or a `workspaceSymbol` that matches
a vendored file, is such an answer where you have not vouched for that tree, and vouching for it is
the way to see the locations. An answer that names nothing carries no taint.

The **text** at a location takes the ordinary treatment, because it is bytes the file chose. A
signature or a source excerpt from a file nobody vouched for comes back as a reference, so one result
can be a visible list of locations whose text the planner may not read.

**Hover text is a reference whatever you have vouched for.** A hover answer carries the prose and a
position and never says which file wrote it, and a doc comment is written where the symbol is defined
rather than where you asked: hovering over a call in one file shows prose out of another. With no file
named there is no trust map entry to read, and borrowing the queried file's entry would hand the
planner bytes out of a vendor directory you deliberately left untrusted. Unattributed text is
untrusted, on the footing every other output of a server arrives on. Hover is where most of this
tool's value is, which makes this its sharpest cost.

A location is not made routing by having been returned. A read of a path that came back from here is
gated as it would be had the planner guessed the path, and a write to it is put to you as a diff like
any other. What somebody who owns a file in a tree you vouched for gains is a say in which path and
line the planner is told about, by arranging their code so a symbol resolves where they like. That costs at
worst a wasted read of a file the planner could already read.

## An answer says which kind of nothing it is

A definition in a dependency, a toolchain source, or anywhere else outside the working directory
comes back saying it is outside the workspace, with its path not spelled as though `read_file` would
open it. Naming it does not make it readable: a read of it is refused as for any other path outside
the tree.

No server configured for the language, a missing binary, and a server that failed to start are three
different answers, and **none of them is an empty result**. Nothing falls back to searching the tree.
An absent server reported as "no references found" is a false negative that reads as proof, and a
planner that believes nothing calls a function will delete it.

An answer given while the server is still indexing says it may be short, in the same words a
truncated search uses. A request waits for the index up to a bound and then answers from what there
is rather than failing.

## The index is cached, and it is not small

A server keeps its index under `~/.bravebot/lsp/`, keyed by the workspace and never inside your tree,
which is what makes the second session in a workspace fast. A question about a symbol should not
change the tree you are working in.

**Only age prunes it.** A Rust workspace's index runs to a few hundred megabytes, and a machine that
has been in many workspaces holds one for each until no server has started on it for
[`retentionDays`](../customize/configuration.md#retentiondays) days, 30 where you state none.
Deleting the directory at any time costs the next session its indexing time and nothing else.

Nothing in that cache is read as trusted, whatever else `~/.bravebot` is trusted for. The bytes
derive from workspace files, so they carry those files' labels, and the only thing that reads them is
the server. An [incognito session](../using/incognito.md) writes
no cache: its server still runs and still answers, and it re-indexes each time.
