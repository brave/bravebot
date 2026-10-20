---
sidebar_position: 4
title: Security
description: What Brave Bot defends against, what it does not, and what leaves the process.
---

# Security

:::caution[The logins and caches a program the agent runs can reach by default]
On Linux and macOS, under the default sandbox mode, a program the agent runs, and every program it
starts, such as the install scripts an `npm install` runs, can:

- read the files tools keep a login in outside the locations the sandbox refuses, such as
  `~/.config/gh`, `~/.git-credentials`, `~/.config/git/credentials`, `~/.netrc`, `~/.npmrc`,
  `~/.cargo/credentials.toml`, `~/.pypirc`, `~/.m2/settings.xml` and `~/.gradle/gradle.properties`;
- read every environment variable Brave Bot was started with except its own signing key and the
  variables a gateway you configured names as holding its key, so `GITHUB_TOKEN`, `NPM_TOKEN` and
  `AWS_SECRET_ACCESS_KEY` among them;
- on macOS, read the login keychain file `~/Library/Keychains/login.keychain-db`, which lets a
  lookup through the keychain service reach the items in it, where macOS decides which items it
  answers with and which it asks you about first;
- write the toolchain caches, among them `~/.cargo/registry`, `~/.cargo/git`, `~/go/pkg/mod`,
  `~/.m2/repository` and `~/.gradle/wrapper`, which the next `cargo build`, `go build`, `mvn` or
  `./gradlew` you run yourself runs code from, outside the sandbox;
- with the network open, which is the default, send what it read to any host.

These are the default because `git`, `gh`, `npm` and `cargo` read their logins and write their
caches there, including when `make` or another script starts them, and the program the sandbox sees
then is the script. Each is withheld by a setting in `~/.bravebot/settings.json`, and the programs
that use what it withholds then fail under the sandbox:

- `{"sandbox": {"network": {"allowedHosts": ["registry.npmjs.org"]}}}` holds a program to the hosts
  listed, with GitHub added for a `git` or `gh` command that talks to a remote and its own registry
  added for `cargo`, `npm`, `pip` and `go`. Other hosts are refused, and so is a listed name that
  resolves to this machine, a private network or the cloud metadata address, unless each such
  address is an entry of its own. An entry cannot be an IPv6 address, so `localhost`, which most
  machines also resolve to `::1`, is refused; list `127.0.0.1` and ask for that.
  An allowed host can still
  receive what a program read, such as a package it publishes to that registry, and on macOS a
  certificate check can fetch from the address the certificate names.
- `{"sandbox": {"filesystem": {"denyRead": ["~/.npmrc", "~/Library/Keychains/login.keychain-db"]}}}`
  refuses a token file or the keychain file, to `gh` and git's `osxkeychain` helper as well.
- `{"run": {"scrubEnv": ["GITHUB_TOKEN", "NPM_TOKEN", "AWS_*"]}}` withholds those variables.
- `{"sandbox": {"mode": "strict"}}` reads a credential only for a command that is lent one (a `git`
  or `gh` command that talks to a remote, `aws`, `kubectl` or `docker`, or a scope you approved for
  the line), reads no keychain file, and lets a program reach only the cache of its own toolchain or
  one you approved for the line, so a `cargo build` that `make` starts has no cache to read or
  write. A `denyWrite` entry takes one cache from every program instead.

`{"run": {"network": "closed"}}` takes the network from a program that has no reason for it. A
package manager keeps it, and so do the install scripts it runs.

`{"git": {"identities": {"github.com/your-account": "~/.ssh/your-account.pub"}}}` has a `git`
command whose arguments name an ssh address of that owner, such as
`git push git@github.com:your-account/repo.git main`, offer GitHub that key first, and after it only
keys your `~/.ssh/config` names for the host. ssh otherwise offers the agent's keys in order and
GitHub takes the first one it knows, so with keys of two accounts in one agent every push goes out
as one of them. A remote given by name, such as `origin`, is not matched, because its address is in
the repository's own configuration. On those commands it replaces a `GIT_SSH_COMMAND`, `GIT_SSH` or
`core.sshCommand` of yours. Only your own settings file and a `--settings` file outside the
workspace can set it.

Without an entry for the owner, the run prompt in the terminal interface numbers the public keys
your own ssh files name and asks which one to offer, once per owner, and keeps the answer in
`~/.bravebot/reach.jsonl`. An entry still decides over a kept answer.

On Linux, on a kernel whose Landlock is older than its ninth version, a program also reaches every
unix socket that has a path, whatever these settings say: the ssh agent's, with every key it holds,
`docker.sock`, and the session bus, through which `systemd-run --user` starts a program outside the
sandbox. On Linux, signals are not scoped. On macOS a stage signals only processes its own profile started. A connection to an abstract unix socket made outside the stage (the
X server, the session bus on some systems) is refused only on a kernel with Landlock's sixth version
or later, and is reachable below it.

Under the default mode on Linux and macOS every program writes every toolchain's cache, since a
program is as often a script or a `make` target as one build. A postinstall script run by `npm ci`
can therefore leave something in `~/.cargo/registry` for a later `cargo build`. `strict` holds each
program to the cache of its own toolchain.
:::

## What this defends against

**Indirect prompt injection.** Text that arrives from somewhere nobody vouched for (a web page, a
dependency's README, a build log, a program's output, a file in an untrusted directory) never reaches
the model deciding what to do next, and never reaches a decision in the Rust code either.

This is structural rather than instructional. The model is not asked to be careful with such content,
because it never has it: content is quarantined and the planner is handed a reference. What an
attacker can write into a file, they cannot turn into an instruction, because nothing in the pipeline
will read it as one.

The four things that would break it, and are therefore what a code review looks for:

1. **A branch on untrusted bytes.** The driver may carry untrusted content and hand it to an effect. It
   may not branch on it: no `if`, `match`, comparison or early return whose condition derives from
   untrusted bytes. A "careful refusal" computed from attacker-controlled text is still a decision an
   attacker took.
2. **The same branch, moved into the kernel.** Relocating a decision is not removing it, and "it is
   only for a message to the model" does not help, because a message to the model *is* the planner's
   context.
3. **A declassification outside the three gates.** Reading untrusted bytes is allowed only where they
   were already going: a filesystem write, an HTTP body, or a person's screen.
4. **A label built by hand.** Never construct a value with a better label than its inputs had. If a
   value derived from untrusted input has to be trusted for something to work, the design is wrong.

## What it does not defend against

Stated plainly, because an unlisted exception is indistinguishable from a violation.

- **Anything you vouch for.** `@a-file-you-have-not-read.md`, a dropped file, `! cat notes.md`, and
  answering yes to a directory all put content into the planner's context on the strength of your
  gesture. Nothing inspects the bytes, and nothing could.
- **A vouched-for command's output.** `a` at a run prompt makes what that command prints trusted. `git
  log` prints commit messages whoever contributed wrote.
- **What lands in a trusted directory afterwards.** A rule is about a path, not about the files that
  were in it. See [Trusted directories](trust.md#known-costs).
- **A program the agent was allowed to run.** On Linux and macOS a program can read the machine
  except the places that hold a credential (ssh private keys, cloud and container logins,
  keychains other than the macOS login keychain file that `gh` and `git` use, browser profiles and
  password stores, shell and database histories, the logins and transcripts of other coding agents
  (`~/.claude`, `~/.codex`, `~/.gemini`), mail, chat and editor data, and `~/.bravebot`, where the
  gateway keys, the Leo Premium token and the MCP server list are, which no
  `allowRead` entry, scope or `/reach` grant lifts, except that a program reads, and never writes,
  the session records in `~/.bravebot/sessions`) and can write only the directories the session
  was opened on, its scratch directory, the system temporary directory (unless you [hold it back](../customize/configuration.md#sandboxholdbacktemporarydirectory)) and the toolchain caches. A
  command that names `aws`, `kubectl` or `docker` is lent that tool's directory, and a `git push` the
  agent socket, never a private key. The files a tool reads by name to do its job, such as the `gh`
  login, `~/.npmrc` and `~/.netrc`, are readable. So are the shell startup files, `~/.gitconfig`,
  `~/.pgpass`, `~/.my.cnf`, `~/.vault-token` and the configuration of tools such as `helm`, `glab`,
  `doctl` and `rclone`.

  A transcript can hold what you pasted, and a program that can read the session records could send
  them out wherever the network is open. Close the network with `--run-network closed` if that
  matters for a run.

  A stage that starts `claude`, `codex`, `gemini`, `opencode`, `sops` (with an age key in its
  default place) or `pass` cannot read the location that program keeps its data in. The same holds
  for a graphical program that checks the X display's cookie, and for a session opened inside a
  refused location. Name the location in `sandbox.filesystem.allowRead`, or in `allowWrite` where
  the program also writes there.

  The refused locations are listed one at a time, so a browser channel or application the list does
  not name is readable.

  On macOS a program cannot listen on a port of the
  machine unless the planner asks for `loopback` on that line and you approve it, and only `git`
  itself writes a `.git`, so a script, `make`, `npm` or `gh` that runs git to commit, check out or
  clone is refused. On Windows a program runs in a container that reaches only the directories the
  session was opened on, its scratch directory, the system temporary directory, the system and
  program directories, git's configuration files, its toolchain's caches and the credential scope
  its command names, so in no sandbox mode but `off` does it read the rest of the machine. A
  program's own network requests do not go through the one way out described below, so an approved
  `curl`, `git push` or package install can read what its profile reaches and send it with nothing
  here routing or inspecting the traffic.
- **The model being wrong.** Approval prompts exist because the planner can propose something you do
  not want, and reviewing the diff is the mechanism that catches it.

## Seven deliberate exceptions

Brave Bot looks at untrusted bytes to decide something in seven places, three in the policy layer
and four in the agent, the search, the diagnostics sentence, reading a repository's history and
telling a text file from a binary one, all written down rather than left to be found.

**Splitting a processor's answer.** A processor returns one piece of text holding two things: a remark
for the person watching, and the document to be written. It marks where the document begins, and the
policy layer searches for that mark to find where to cut. The mark is not a boundary and cannot be
forged, because there is nothing to forge. The processor writes the whole answer and may put the mark
wherever it likes, and the first one counts. An attacker who owns the file gains: the ability to make
the write be refused, the ability to shift where the cut lands within content that was already
theirs, and the ability to put words in a remark that reaches your screen and stops there. What they
cannot do is choose *which* file is written, which stays the planner's choice plus your approval from
a diff.

**A trailing newline.** Before a file is written back, the code checks whether the file being replaced
ended in a newline, so the new one can end the same way.

**Reading a verdict out of a check.** A second model can be shown quarantined content and asked to
answer in one word whether it looks like an attempt to give instructions. Reading that word is a
decision taken from a reply that is a function of untrusted content, so the word is untrusted too.
That check is off until you turn it on, and while it is off the only thing its answer decides is
which banner the approval prompt carries: the bytes are drawn below it either way, the keys that
answer the question are offered either way, and nothing is promoted until you say so. An attacker who
owns the content can force the reassuring word, and what that buys them is a quieter sentence above
content you are still reading for yourself.

**Searching files nobody vouched for.** A search tests every line of every file it walks against the
pattern, including files you did not vouch for, so which lines match, how many, and whether the search
reports itself incomplete are decided by those files' bytes. The lines never reach the model: one
unvouched file makes the whole result untrusted, and the model is given a reference to it. What
reaches the model is the result's size, whether it was cut short and which limit cut it, the offset
for the next page, and, when it asks for a page past the last match, how many there were. An attacker
who owns one of the files can set those numbers, make the model spend searches paging through a large
result, and make one search read a very large file.
They cannot choose which files are searched, how the result is labelled, or any destination, and the
model never sees a line of text from the file.

**Counting a language server's diagnostics.** The `lsp` tool's `diagnostics` operation tells the model
how many errors a language server found, the line each starts on (up to 20, then how many more), and
a count of warnings and of other notes. The server computes those numbers from the file asked about
and from the files it indexed that file against, including files you did not vouch for. A file's
message, code and source are never carried: the model gets a reference it cannot read, and every word
of the sentence is Brave Bot's own. An attacker who owns such a file can set the counts, choose which
line numbers are named, and choose whether the sentence says no errors were reported yet, which is
worded as nothing stronger than that. They cannot choose the path, any destination, or put a word of
their own in front of the model.

**Searching a repository's history.** A repository's history can hold files from paths you did not
vouch for, and reading it searches or compares their bytes. Which lines match and whether the
answer is cut short are decided by those bytes, so one such file makes the whole answer untrusted,
and the model is given a reference to it. What reaches the model is the answer's size and whether
it was cut short. An attacker who owns one of the files can set those, and tell the model by a
count whether the pattern it chose matched. A page boundary that earlier files' matching lines
decided can stop the walk before later files, but every path is recorded before its file is read, so
those bytes cannot steer the label. They cannot choose how the answer is labelled or any
destination, and the model never sees a line of text from the file.

**Telling a text file from a binary one.** When the model reads a file you did not vouch for, Brave
Bot does not hand back text. It checks the start of the file for a NUL byte or a heap of control
characters, and either refuses in its own words that the file is binary, or gives the model a
reference with the file's size. An attacker who owns the file chooses which of those two results the
model sees, and can put the file out of reach of the component that reads it later. That is one bit
beside a size they already set. They cannot choose how the result is labelled, any destination, or
put a word of their own in front of the model.

## What leaves the process

There is **one way out**, and it is not optional: every outbound request carrying labelled content
goes through a single call, and the HTTP client behind it is private to that module, so nothing else
can use it to open a second path. One component keeps a client of its own; the end of this section
says which, and what it carries.

- **Redirects are revalidated on every hop.** They are followed by hand and each new URL is put to the
  gate before it is fetched, so a permitted host cannot hand off to a denied one. The chain is bounded.
- **A redirect may not leave `https` for cleartext.** A hop from an `https` URL to a plain `http` one
  is refused, whatever asked for the request. Every hop re-sends the whole request, headers and body
  alike, so a chain that lost TLS would put in the clear what the hop before it carried under TLS. A
  chain that had no TLS to begin with continues, and one that picks TLS up part way through cannot put
  it down again: what is refused is leaving `https`, not a hop that changes scheme.
- **Only `http` and `https` ever reach the network.** Any other scheme is refused before a connection
  is attempted, rather than handed to a library to interpret.
- **A body is capped, and a truncated one says so.** A body that stops partway is a failure rather than
  a short success. This is resource hygiene, not content inspection. The bytes are never parsed to
  decide anything.
- **Each phase is bounded separately.** Connecting, starting to reply and continuing to reply are timed
  apart, so a slow answer is not confused with a dead connection. A model streaming from this
  machine, with no proxy in between, is waited on for as long as it takes wherever you can stop it,
  since a local model can go quiet for minutes while it writes a tool call.
- **Only "not now" is retried.** A connection that gave out is worth another attempt; a refusal is not.

One crate opens a socket of its own: the subscription client, for [Leo
Premium](../customize/premium.md). That traffic carries credentials and an order id, never workspace
content or model output, so no labelled value escapes the gate. It follows its own redirects through
its library's default rather than through the loop above, so the refusals in that list, the cleartext
one included, do not reach it.

## Certificates and proxies

Both are read from the environment on purpose and set on every client explicitly, rather than left to
a library's default, so what is in force is a decision here and not a property of a dependency's
version. `bravebot doctor` reports both, which is where to look first when a connection fails.

A process started from a launcher or a service has no shell profile, so the six names below
(`SSL_CERT_FILE`, `SSL_CERT_DIR`, `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY` and `NO_PROXY`) may also be
set in the `env` block of `~/.bravebot/settings.json` or of a `--settings` file outside the project, in
upper case. A name exported in the environment wins, and the certificate pair and the proxy each
come whole from one place, so a bundle in a file is never added to an exported directory. `NO_PROXY`
is the one name read on its own. A `.bravebot/settings.json` or `.bravebot/settings.local.json` arrives
with a checkout, and a proxy or an authority it chose would decide where conversation content goes and
who can read it, so one named there decides nothing and `bravebot doctor` names the file. `doctor`
also names the file that supplied the roots or the proxy, without the proxy's value.

**Certificate authorities.** A build ships a set of them, and `SSL_CERT_FILE` (a bundle) or
`SSL_CERT_DIR` (a directory of them) names others **in their place**. What they name replaces the
shipped set rather than adding to it, because that is what every other client on the machine does with
these variables: somebody pinning a private authority has ruled the public ones out on purpose. The
two are read independently, so setting one that turns out to be empty does not discard what the other
held. Where neither holds anything, nothing is trusted rather than the shipped set quietly coming
back, and `doctor` names the path that yielded nothing.

This is what to set on a network that inspects TLS, where the certificate presented comes from an
authority whoever set the machine up installed. The **platform trust store is not read**: on a machine
where neither variable is set, an authority installed into the system store is invisible here, and a
line in a shell profile is the remedy.

**A proxy.** `ALL_PROXY`, `HTTPS_PROXY` and `HTTP_PROXY` are read in that order and in either case,
with `NO_PROXY` naming the hosts that bypass it. `doctor` names the proxy by protocol, host and port,
the hosts it is not used for, and whether it requires a credential; the credential itself is never
printed, because a diagnostic is something people paste into issues. A proxy whose protocol this build
cannot connect through is not used at all, and `doctor` says that one was named and is not the route.
A variable that is set but whose value cannot be read as a proxy address (not a URL, no host, or a
scheme no proxy speaks) is named by `doctor` on its own line, without its value. The next variable in
the order above is still tried, and `doctor` ends on a configuration error for it.

A proxy carries request bodies, which here means conversation content. It can read them only where it
also terminates TLS, which takes an authority this process trusts and therefore one of the two
variables above: a second statement by the same person, on the same machine, that every other client
on it is held to as well. A proxy nobody stated sees nothing, and one somebody stated sees what they
already let it see.

## Confinement

`bravebot doctor` reports the operating-system confinement available on your platform and whether
the kernel enforces the network denial, printed rather than assumed, because the guarantee differs
by platform and kernel.

The opening screen and `/status` name that same platform level, and `/status` says beside it what
the session confines: the [MCP servers](../customize/mcp-servers.md) it started, or nothing.
Confinement bounds a process started to run code Brave Bot did not write, so a session that starts
none of those is inside no such boundary, and the level is what your machine offers rather than
something holding the session back.

**Sandbox modes.** The programs the agent runs start under one of four modes. On Linux and macOS,
`read-only` reads the machine as above and writes nothing in the session's directories, the scratch
directory or the toolchain caches. `workspace-write` reads the same and also writes the session's
directories, the scratch directory and the toolchain caches. `strict` reads nothing on the machine
beyond what the program's binary needs, the session's directories and the credential scope its
command names. On Windows no mode but `off` reads the machine: `strict` and `workspace-write` give a
program the same container, and `read-only` gives it that container without its writes to the
session's directories, the scratch directory and the toolchain caches. `off` starts programs with no
profile. The default is `workspace-write`. Choose one with `--sandbox <mode>` or with
`{"sandbox": {"mode": "strict"}}` in `~/.bravebot/settings.json`. The
earlier name of `workspace-write`, `standard`, is still read in the
flag, `/sandbox`, the settings files and the managed file, and is reported as `workspace-write`. A repository's `.bravebot/settings.json` can ask for `strict` and
nothing looser, so a checkout cannot unconfine whoever opens it. Its `strict` is not obeyed where your
settings or the managed file chose `read-only`, since `strict` writes the session's directories. A managed file that pins a mode is a
floor: a flag or a setting looser than it stops the session from starting, and `bravebot doctor`
says which files disagree. A pin of `strict` also refuses `read-only` (on Windows, where `read-only` reads no more than `strict`, a pin of `strict` keeps it), and a pin of `read-only` also
refuses `strict`, since `strict` writes the session's directories. A refusal of a mode looser than a pin says which of reading and writing the mode does more of, and names the pinned mode to use. `strict` is refused on a Linux kernel below Landlock ABI 9, which is Linux 7.1, because a program there reaches every unix socket that has a path, such as the session bus, the ssh agent and `docker.sock`. The refusal says so. Under `workspace-write` and `read-only` on such a kernel, the opening screen, `/status` and `bravebot doctor` each show a line saying that programs reach sockets with a path. A local MCP server reaches these sockets under every mode. `read-only` also closes the network
for every program whatever `run.network` says, and lends none the remote credentials, so `git push`
and `gh pr create` fail. Under it the agent's file tools write nothing either,
a manifest run that writes a file is refused, a delegate is not given a checkout of its own, a
command line that redirects output into a file is refused, and a request from the agent to write a
path is refused, all of these even in accept-edits mode and with `--dangerously-skip-permissions`.
Redirecting to `/dev/null` still runs. Under `read-only`, `/add-dir` and `--add-dir` add a directory to read and never to write, a build, a formatter or `git commit` in the session's directories fails whichever step runs it, and programs can write the temporary directory and the paths your own `allowWrite` entries and remembered write grants name. A write grant held before the switch to `read-only` gives its read and not its write until the mode returns to `workspace-write`. A request to read a path is accepted; choose another sandbox mode to let programs write the project. `/checkouts apply`, which you type, is not held to the mode. A program that a stage runs opens its own redirections under its profile, so with an `allowWrite` entry `sh -c 'make > build/log'` can write there. The desktop app reads `off` as
`workspace-write`. When the mode is not `workspace-write`, the opening screen and `/status` say so,
and `bravebot doctor` and `/sandbox` say what chose it.

Where confinement is used, it **fails closed**: if it cannot be established the process does not run,
rather than running unconfined. A profile starts denying everything and grants accumulate onto it, and
a policy that would confine nothing is rejected rather than applied, and a write grant on the root
directory, or on a drive root on Windows, confines nothing and is rejected the same way. The network
is denied unless it was asked for. On macOS, a process that is given the network reaches a unix
socket only under a directory it may write, so a Docker daemon's socket elsewhere stays out of reach. A refusal of a read or write inside such a directory also refuses a connect beneath it, so a socket under a `denyWrite` path, or under `~/.docker` in a session opened on the home directory, stays out of reach. A backend reports what the kernel actually enforces rather than what it was asked for,
and a policy demanding something it cannot deliver is refused.

**A policy is granted as written, or it is refused.** A backend that cannot install a grant for one of
the paths refuses the whole policy and names that path, rather than confining the process to the rest
of them. Running under fewer paths than the policy names while the record says the policy was applied
is the same silent degradation, reached one grant at a time.

**A directory inside a credential location is refused.** A stage is not started when a directory the
session was opened on is, or lies inside, a place that holds a credential such as `~/.ssh`. On macOS
a read and write grant at such a path would lift the refusal, so the directory would hand the keys to
every program. A directory above a credential location, your home directory among them, is still
granted and the location inside it stays refused. `/add-dir`, `--add-dir` and
`additionalDirectories` refuse the same directories before they are added. A file tool refused for a
path inside such a location does not point at `/add-dir` or `request_path`, which would refuse it
too. For a read it suggests dropping the file on the window, for a write it suggests the unconfined
line where the session accepts one, and for the location itself it suggests nothing. When a confined step fails,
the profile line printed with the failure ends with a fixed sentence, the same in every mode and for
exit 1 and exit 2: a credential location such as `~/.ssh`, `~/.aws` or `~/.kube` cannot be added with
`/add-dir` or `--add-dir`, and where the mode accepts a request, a credential scope is how a line
reaches one. On Windows a container
cannot be refused a path inside a grant, so a stage is also refused when a session directory holds a
credential location, and `bravebot doctor` reports a `denyRead` or `denyWrite` entry inside a granted
directory as not in force.

**More places a stage cannot reach.** The browser relay's default directory `~/.bravebot-browser`,
which holds the secret that drives the browser, is refused like `~/.bravebot` (a relay told to use
another directory is not covered). On Windows `gcloud` and `gpg` keep their data under
`~/AppData/Roaming/gcloud` and `~/AppData/Roaming/gnupg`, and those are refused too, with the browser
profiles under `~/AppData/Local` (`BraveSoftware`, `Google/Chrome`, `Chromium`, `Microsoft/Edge`) and
`~/AppData/Roaming/Mozilla`, and the Windows stores for credentials, keys and vaults under
`Microsoft` in `~/AppData/Local` and `~/AppData/Roaming` (`Credentials`, `Protect`, `Crypto` and
`Vault`). A session opened on `~/AppData/Local` or on `~/AppData/Roaming/Microsoft` is refused. On macOS a
credential directory that is a link (a dotfile manager's `~/.ssh`, say) is refused where it stands and
where it leads. On macOS, when the session is opened on your home directory or above it, a stage also
cannot write the places it cannot read, nor the shell startup files (`~/.zshenv`, `~/.zprofile`,
`~/.zshrc`, `~/.zlogin`, `~/.bash_profile`, `~/.bash_login`, `~/.bashrc`, `~/.profile`) or
`~/Library/LaunchAgents`, because what is written there runs later outside the sandbox. They stay
readable.

In bypass mode, and for a path a `request_path` call asks to write, a stage is also refused writes to
the persistence locations: the shell startup and logout files (`~/.zshenv`, `~/.zprofile`,
`~/.zshrc`, `~/.zlogin`, `~/.zlogout`, `~/.bash_profile`, `~/.bash_login`, `~/.bashrc`,
`~/.bash_aliases`, `~/.bash_logout` and `~/.profile`), `~/.config/fish`, `~/.xprofile`,
`~/.xsessionrc`, `~/Library/LaunchAgents`, `~/.config/systemd/user`, `~/.config/autostart`,
`~/.config/environment.d`, `~/.gitconfig` and `~/.config/git/config`. Reads are not affected. A stage
therefore cannot run `git config --global`. The locations are fixed paths, so a `ZDOTDIR` or
`XDG_CONFIG_HOME` that moves one leaves the moved file writable, and a `PATH` directory such as
`~/.local/bin` stays writable.

**On macOS a stage reaches only a fixed list of system services.** The pasteboard and LaunchServices
(opening a file or URL in another application) are refused. A program that needs another service,
such as a file watcher built on FSEvents or Spotlight, loses that feature. The certificate trust
services are in the list only for a stage allowed to reach the network. A stage held to a host list
has them, and macOS fetches a certificate's issuer from its own process, so the host list does not
bound that one request.

**On Linux every program starts in a session of its own**, so it has no controlling terminal and
cannot type into yours, which kernels before 6.10 would otherwise allow. A terminal's interrupt and
hangup do not reach it.

**A `PATH=` assignment on a `run` line chooses nothing a stage may read.** The directories a stage's
programs are read from come from Brave Bot's own `PATH`, and a program location that is, holds or lies
inside a credential location is left out.

**A program Brave Bot starts itself is never one a stage wrote.** The AWS CLI, the clipboard tools,
`osascript` and the inhibitor `/caffeinate` start outside the sandbox, so each is looked up only in
absolute `PATH` entries that lie outside every directory a stage may write. Someone whose only `aws`
is in an activated virtualenv, or in the session's own directory, is told it is not installed until
it is installed elsewhere. In bypass mode a stage may write every path your account can write, so an
`aws` in a directory your account owns (a Homebrew prefix, say) is found until the first stage runs
and reads as not installed after that, until Brave Bot exits. [Hooks](../customize/hooks.md),
language servers and the editor that Ctrl-G opens are looked up the same way when named without a path.
The editor, a hook and the AWS CLI are also started with a `PATH` of those same entries only, in the
order written, so a program any of them starts by name (an `env` shebang, an `$EDITOR` of `env vim`,
or a bare `credential_process` command) is not found in a directory a stage may write. A program the
editor, a hook or the CLI needs that lives only in such a directory,
for example a tool run from a project's `node_modules/.bin`, is not found by it. `bravebot doctor --push-identity` looks up
`ssh-keygen`, `ssh` and `git` the same way, leaving out the directory it ran in and the checkout
holding it, reports one found only there as absent, and starts them without Brave Bot's own credentials and
with a `PATH` of those same entries only, so a `ProxyCommand` in your ssh configuration that names a bare
program does not find one a stage wrote. The inhibitor `/caffeinate` starts is given a `PATH` of those
same entries only as well, so a program `systemd-inhibit` starts by name is not found in a directory a
stage may write.

**The remote scope can write `~/.ssh/known_hosts`.** A stage that carries it can add a host key or a
`@cert-authority` line to that file, and your own ssh trusts the line afterwards. Read the
command before you approve a stage with that scope.

**Signing a commit needs the `signing` scope.** A confined `git commit`, `rebase` or `merge` that
signs with ssh is lent the one public key your `~/.gitconfig` or `~/.config/git/config` names, found
through an `[includeIf "gitdir:..."]` block that matches the session's directory, and the ssh agent
socket, never a private key. A `run` can ask for it by name with `scopes: ["signing"]`; you are asked
every time and it is not remembered. The key is also searched among the `*.pub` files in `~/.ssh`
whose key the agent holds, up to 32. A `user.signingkey` set in the repository's own configuration is
not read, and a failed step's profile line says so.

A signing step never reads a private key, so "No private key found" means the ssh agent does not hold
the configured key. It does not mean the key file is missing or that the sandbox hides it. Run
`ssh-add` on the key and try again. Where the session accepts it, the agent is also told the one commit
line can be run with `unconfined: true`, which you are asked to approve.

**On macOS a write grant does not reach a `.git`.** git runs the commands a repository's
configuration and hooks name, unconfined, the next time anybody runs git there, so a confined process
that could write one could run code outside its confinement. A grant to write a directory refuses a
write to any `.git` beneath it, in any case. A program `run` starts is held to that unless the
program is `git` itself, so an install script, a build or a test cannot plant a hook or a setting
for git to run later, and a build that clones a git dependency fails there. On Linux and Windows
nothing holds a `.git` back from a program `run` starts.
On every platform an MCP server is also refused a
directory inside a repository. Linux's Landlock cannot hold one directory back from a grant, so there
a `.git` beneath a server's directory, its home or the temporary directory is one it may write.

**A write grant covers moving a file inside it.** Writing a temporary file and renaming it into place
is how a compiler, a package manager and an editor write anything, so a grant to write a path covers
moving a file from anywhere under it to anywhere else under it. On Linux this needs a kernel right that
arrived in Landlock's second version, so on kernels between 5.13 and 5.19, which a long-term
distribution release still ships, confinement is unavailable and a process is refused rather than run
under a policy that cannot be applied in full. A confinement that denied the move would leave a tool
that renames files for a living copying and unlinking instead, so the work appears to succeed and has
quietly stopped being atomic.

**A path that is not on disk is left out, and named.** Profiles are assembled from lists naming more
paths than any one machine has, so refusing every policy that mentions an absent path would mean a
machine with no `~/.pyenv` is a machine where nothing runs. Where the platform can grant a path that
does not exist, the policy is what was wanted. Where it cannot, an absent path is left out before the
policy is built and reported back, so the difference between the grant that was decided on and the
grant a program got stays visible. Naming the parent directory in its place is not done, since that
would grant every other file in it. A path is created in its place only where the list naming it says
whether it is a file or a directory, and only for a program meant to create it, which is how a fresh
account with no `~/.ssh/known_hosts` would get one rather than a push that fails; what is created is
reachable by your account and nobody else. The lists a confined process runs under today say neither,
so nothing is created for one.

**Looking at a path is not reading it.** A confined process can learn whether something is at any
path, what kind of thing it is, its size, when it changed and where a link points. It cannot open a
file outside its grants or list a directory outside them, except that on macOS it can list the names
in `/`, which are the system's top-level directories. Linux's Landlock bounds no look at all,
and on macOS a profile that refused one refused the walk to a grant too, which no node program
survives.

**The environment is the caller's.** A confined process starts with the environment this process
holds. A grant over paths can neither withhold nor hand over what sits in a variable, and a credential
or an agent socket does, so which of its own variables a program is trusted with stays the caller's
decision rather than something that differs per platform. On macOS a confined process cannot use
`sysctl(KERN_PROCARGS2)` to read another process's exec-time environment, including bravebot's.
Machine-information queries needed by ordinary tools remain available, including the system-wide
limit on argument size that `xargs` and `find -exec {} +` read. This does not erase the
kernel's saved environment or protect it from an unconfined process of the same user. On Linux the
terminal interface also marks itself non-dumpable before it reads its command line, so a stage cannot
read its environment (and the keys you exported) from `/proc`. If the kernel refuses the mark, a
message is printed and the run goes on.

Confinement does not cover the rest of the system. A processor is a model call made by our own code,
and a program you asked for runs with the access your own shell would give it. Everywhere else, the
boundary is the capability set and the label on a value.

**Windows confines a process in a container, and a grant there is a change to your directories.**
Each path a policy grants gets an access entry written onto the directory, and the entries are
removed when the run ends. A run that is killed before it gets that far can leave an entry behind.
Every run uses a container profile of its own, so a leftover entry names a container that no longer
exists and never a standing grant to something still running. A file or directory created so a
write grant has something to name is reachable by your account and nobody else. A policy that
requires denying subprocesses is refused there, because the container cannot enforce it.

A stage and its children run in a job object, so a cancel, a timeout or Brave Bot exiting ends them,
and a stage that cannot be put in a job does not start. Each stage gets a console of its own with no
window. A program that asks a question on the console rather than on its input waits until it is
cancelled or times out, and the code page is the system's, not one set with `chcp`.

A stage running `cmd` or `cmd.exe` is refused where `cmd.exe` would read the line differently from
how Brave Bot quotes it. That is when an argument holds a quotation mark, `&`, `|`, `<`, `>`, `^`,
`(`, `)`, `%`, `!`, `,`, `;`, `=` or a line end, or when the first argument after `/c`, `/k` or
`/r` is quoted and another follows it. Run the program directly instead. One argument after the
switch still starts.

## Data collection, usage, and retention

Brave does not use your data and does not store it. Prompts and used file contents are sent to Brave's
endpoint to produce a reply and are discarded once it has been produced. Nothing is retained and
nothing is used for training.

:::caution[Six lines about your machine go out with every request]
The system prompt states your working directory, whether it is a git repository, the platform, the OS
version, the shell and today's date. The working directory is an absolute path, so on most machines it
contains your username, and the OS version names your kernel build. There is no setting that withholds
them.

Nothing else about the machine is added: no environment variables beyond `$SHELL`, no hostname, no
username on its own, no file contents, no directory listing. See
[Where you are working](../customize/instructions.md#where-you-are-working) for why each is there.
:::

Local state is stored in `~/.bravebot` on your own machine: session records, prompt history and the
model you chose. Session records hold what the planner was allowed to hold, which means whatever file
content it was shown. **Nothing untrusted is ever written down**, by construction rather than by
filtering, and quarantined content is not written at all. A pasted picture is written, because it was
part of your own message. Deleting a session record removes that session; the prompt history is a
separate file holding every prompt you have submitted across all runs, and deleting a session does not
touch it.

Leo Premium credentials live in a mode-0600 file under `~/.bravebot`, readable only by you. They are
not encrypted at rest, which is what the browser they are imported from does with the same secret.
See [Leo Premium](../customize/premium.md#where-they-are-kept).

The credentials Brave Bot reaches a model with are overwritten in memory when the value holding
them goes, rather than handed back to the allocator with the bytes still in them, and the process
turns off its own core dump before it reads the first one, so a crash leaves no file with a key in
it. The single-use credentials of an imported Leo Premium subscription are held the same way, and
so is the text of the file they are read out of. A key held in a settings file is also kept out of
swap on macOS and Linux, unless the kernel refuses the lock, and on Windows it is held in ordinary
memory. Three things are not covered: swap for the buffers a credential passes through on its way in,
the copy in the process's environment when you exported it as a variable, and the one-shot
presentation a premium request carries, which is derived from a credential rather than being one and
lives only as long as the request.

## Reporting a problem

Brave Bot is experimental and developed in the open. Please report security issues through the
[repository](https://github.com/brave/bravebot/issues), and see the
[mini-specs](https://github.com/brave/bravebot/tree/main/docs/specs) for the clause-level
statement of everything on this page.
