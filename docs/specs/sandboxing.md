---
id: SANDBOX
title: Confining subprocesses
status: normative
governs:
  - crates/sandbox/src/lib.rs
  - crates/sandbox/src/policy.rs
  - crates/sandbox/src/linux.rs
  - crates/sandbox/src/macos.rs
  - crates/sandbox/src/process.rs
documented-by: docs/website/docs/security/security.md
---

## Scope

Operating-system confinement for processes that run code we did not write, which today means the
stdio servers in [mcp.md](mcp.md). What this is *not* for is our own code: a processor is a model
call made by our own code, and confining that would fence in the trusted half and leave the
untrusted half free. A program the user asked for runs with the access their own shell would give
it, and what confining one would mean is the last section here.

Confinement is an operating-system boundary. Everywhere else in these specs the boundary is the
capability set and the label on a value, which is a different mechanism answering a different
question.

On Linux the boundary needs the kernel right that governs moving a file, which arrived in
Landlock's second version rather than its first, so a kernel carrying Landlock without that right
is one where confinement is unavailable and a process is refused rather than run under a policy
that cannot be applied in full ([SANDBOX-7](#SANDBOX-7)). What that costs is the kernels between
5.13 and 5.19, which a long-term distribution release still ships, and on which nothing runs
confined at all.

## Clauses

<a id="SANDBOX-1"></a>
### SANDBOX-1: confinement fails closed

If confinement cannot be established the process does not run. An unavailable backend refuses to
spawn rather than falling back, and the platform lookup never hands back a backend that would
confine nothing.

**Why.** Silently degrading is worse than an error: the caller believes it has a guarantee it does
not have, and the audit trail records a sandbox that was never applied.

`verified-by: bravebot_sandbox::lib::an_unavailable_backend_refuses_to_spawn`
`verified-by: bravebot_sandbox::lib::refusal_is_not_a_silent_fallback`
`verified-by: bravebot_sandbox::lib::the_platform_lookup_never_returns_an_unconfined_backend`
`verified-by: bravebot_sandbox::lib::errors_explain_the_refusal`

<a id="SANDBOX-2"></a>
### SANDBOX-2: a policy that would confine nothing is refused

A profile starts denying everything, grants accumulate onto it, and a fully permissive policy is
rejected rather than applied. Granting everything is not a confinement decision.

`verified-by: bravebot_sandbox::policy::strict_permits_nothing`
`verified-by: bravebot_sandbox::policy::allowances_accumulate`
`verified-by: bravebot_sandbox::policy::a_strict_policy_is_meaningful`
`verified-by: bravebot_sandbox::policy::granting_everything_is_not_meaningful`
`verified-by: bravebot_sandbox::policy::network_alone_remains_meaningful`
`verified-by: bravebot_sandbox::macos::a_fully_permissive_policy_is_refused`
`verified-by: bravebot_sandbox::linux::a_fully_permissive_policy_is_refused`

<a id="SANDBOX-3"></a>
### SANDBOX-3: the network is denied unless it was asked for

A confined process reaches neither the network nor the filesystem outside its grants. A backend
that cannot enforce the network denial refuses the policy instead, so the guarantee never degrades
into one that is not in force.

`verified-by: bravebot_sandbox::macos::network_is_only_allowed_when_requested`
`verified-by: bravebot_sandbox::macos::a_confined_process_cannot_reach_the_network`
`verified-by: bravebot_sandbox::macos::a_confined_process_cannot_write_outside_its_grants`
`verified-by: bravebot_sandbox::macos::a_confined_process_runs`
`verified-by: bravebot_sandbox::linux::a_policy_requiring_network_denial_is_refused`
`verified-by: bravebot_sandbox::linux::a_confined_process_runs`
`verified-by: bravebot_sandbox::linux::a_confined_process_can_write_inside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_cannot_write_outside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_cannot_read_outside_its_grants`

<a id="SANDBOX-4"></a>
### SANDBOX-4: a path cannot inject profile syntax

Grants are paths, and a path is content: one containing profile syntax is quoted rather than
interpreted, and a profile built from a hostile path still applies.

**Why.** A grant list assembled from paths would otherwise be a place where a filename decides what
the sandbox permits.

`verified-by: bravebot_sandbox::macos::paths_cannot_inject_profile_syntax`
`verified-by: bravebot_sandbox::macos::a_profile_containing_a_hostile_path_still_applies`
`verified-by: bravebot_sandbox::macos::granted_paths_appear_as_subpath_rules`

<a id="SANDBOX-5"></a>
### SANDBOX-5: capabilities report what the kernel actually enforces, and never more

A backend says what it can enforce rather than what it was asked for, and a policy demanding
something the backend cannot deliver is refused. Which paths it can name is reported the same way.
`bravebot doctor` reports the level in force.

**Why.** An overstated capability is the same failure as a silent fallback, reached by a different
road. An understated one costs the caller a grant it did not mean: a caller that cannot ask whether
a path missing from the disk is nameable reads the platform instead, and then creates a file nothing
asked for, or names the directory holding it, on the platform where neither was necessary.

`verified-by: bravebot_sandbox::macos::capabilities_report_kernel_enforcement`
`verified-by: bravebot_sandbox::linux::capabilities_do_not_overstate_network_denial`
`verified-by: bravebot_sandbox::linux::a_path_that_does_not_exist_is_granted_exactly_where_the_capability_says_so`
`verified-by: bravebot_sandbox::macos::a_path_that_does_not_exist_is_granted_exactly_where_the_capability_says_so`
`verified-by: bravebot_sandbox::linux::a_policy_requiring_network_denial_is_refused`
`verified-by: bravebot_sandbox::linux::a_policy_requiring_subprocess_denial_is_refused`
`verified-by: bravebot_sandbox::lib::an_unavailable_backend_reports_no_confinement`
`verified-by: bravebot_sandbox::policy::confinement_levels_render_for_the_audit_trail`
`verified-by: bravebot_cli::main::doctor_names_the_confinement_level_in_force`
`verified-by: bravebot_cli::main::doctor_says_whether_the_kernel_enforces_network_denial`
`verified-by: bravebot_cli::main::confinement_that_could_not_be_established_fails_the_run`

<a id="SANDBOX-6"></a>
### SANDBOX-6: every path a policy names is granted, or the policy is refused

A policy is granted as written. A backend that cannot install a grant for one of the paths refuses
the policy and names that path, rather than confining the process to the rest of them. Where a
backend can grant a path that does not exist yet, the profile carries that grant as named; where it
cannot, the refusal arrives before the process starts, and again where a path goes away between
that refusal and the exec. Which of the two a backend does is in its capabilities
([SANDBOX-5](#SANDBOX-5)).

**Why.** A policy is the list a caller decided a program may reach, so a process running under
fewer of those paths than the policy names is the silent degradation [SANDBOX-1](#SANDBOX-1)
forbids, reached one grant at a time: the process runs, the record says the policy was applied,
and the program is refused a path somebody granted it. Which paths a backend can name is a
platform difference a caller can work with, and a grant that vanished without being reported is
not.

`verified-by: bravebot_sandbox::linux::a_path_that_cannot_be_opened_is_refused_rather_than_dropped`
`verified-by: bravebot_sandbox::linux::a_ruleset_is_not_built_with_a_path_missing_from_it`
`verified-by: bravebot_sandbox::macos::a_path_that_is_not_there_yet_is_granted_as_named`

<a id="SANDBOX-7"></a>
### SANDBOX-7: a write grant covers moving a file within it

A grant to write a path covers moving a file from anywhere under it to anywhere else under it, not
only creating and removing one. Where the kernel has no right governing that move, confinement is
unavailable there and names the kernel that carries it, rather than applying a policy without it.

**Why.** Writing a temporary file and renaming it into place is how a compiler, a package manager
and an editor write anything, so a confinement denying the move holds a program to less than the
paths its policy granted while the record says the policy was applied, which is the degradation
[SANDBOX-1](#SANDBOX-1) forbids in an operation rather than in a path. It is also the shape of that
degradation hardest to see from outside: the tool that moves a file for a living answers a refused
move by copying the file and unlinking the original, so the work appears to succeed and has quietly
stopped being atomic.

`verified-by: bravebot_sandbox::linux::a_confined_process_can_rename_a_file_between_two_granted_directories`
`verified-by: bravebot_sandbox::linux::a_kernel_that_cannot_govern_a_move_is_refused_rather_than_confining_without_it`
`verified-by: bravebot_sandbox::macos::a_confined_process_can_rename_a_file_between_two_granted_directories`

<a id="SANDBOX-8"></a>
### SANDBOX-8: the environment a confined process receives is the caller's

A confined process starts with either the environment the calling process holds or none at all, and
the caller says which as the process is started. Confinement applies that answer itself, so a
caller asking for nothing is handed nothing whichever backend confines the process, and a caller
asking for its own is handed all of it. A backend that reaches the program through a second program
answers for what that one loses on the way, and where it cannot restore what was lost it refuses
rather than starting the process with less than was asked for. What it restores is what would
otherwise be lost and no more: a command line is readable by every user of the machine and another
user's environment is not, so a variable carried on one is a variable disclosed to carry it.

**Why.** A variable carries what no grant over paths can withhold or hand over: a credential this
process authenticates with sits in one, and so does the agent socket a push signs through. A
backend deciding that for the caller decides it once per platform, so the same policy hands a
program everything on one and nothing on the other, and which of those a consumer was written
against is the difference between a credential withheld and a credential handed to code we did not
write. Leaving the emptying to each caller costs the same thing one step further out, since a
caller that forgets is a program handed everything with nothing saying so.

`verified-by: bravebot_sandbox::linux::the_environment_a_confined_process_receives_is_the_callers`
`verified-by: bravebot_sandbox::macos::the_environment_a_confined_process_receives_is_the_callers`
`verified-by: bravebot_sandbox::linux::a_confined_process_given_an_empty_environment_receives_none_of_this_processes_variables`
`verified-by: bravebot_sandbox::macos::a_confined_process_given_an_empty_environment_receives_none_of_this_processes_variables`
`verified-by: bravebot_sandbox::macos::a_variable_stripped_from_the_wrapper_still_reaches_the_confined_process`
`verified-by: bravebot_sandbox::macos::a_variable_the_platform_strips_from_the_wrapper_is_carried_to_the_program_as_an_argument`
`verified-by: bravebot_sandbox::macos::a_caller_holding_nothing_the_platform_strips_reaches_its_program_directly`
`verified-by: bravebot_sandbox::macos::a_confined_process_asked_to_receive_no_variables_is_handed_none_as_an_argument`
`verified-by: bravebot_sandbox::macos::a_program_path_the_wrapper_would_read_as_a_variable_is_refused`

<a id="SANDBOX-9"></a>
### SANDBOX-9: a path that is not on disk is left out before the policy is built, and named

A caller assembling a policy out of paths whose existence is not its own to decide resolves it
against the backend before handing it over. Where the backend grants a path that does not exist
([SANDBOX-5](#SANDBOX-5)), the policy is what was wanted and nothing is left out. Where it does
not, a wanted path that is not on disk is left out and reported back, each such path once, and
every path that is there is kept in the list it was named in. Resolution adds no path, moves none
between the two lists, and carries the network and subprocess grants as they were, so what it
produces is a subset of what was wanted.

Neither of the other two answers to an absent path is taken. Naming the directory holding it
grants over every other file in that directory, and the path this would fire on first is
`~/.ssh/known_hosts`, whose directory holds the private key no scope reaches. Creating the file
writes where nothing asked for a write, and a policy names a path without saying whether it is a
file or a directory, so a caller creating one guesses between an empty file and an empty directory,
and the wrong guess is a program that fails on a path it was granted.

**Why.** [SANDBOX-6](#SANDBOX-6) refuses a policy naming a path the backend cannot grant, which is
the right answer to a caller that named a path wrongly and the wrong answer to a machine that does
not carry a toolchain some list knows: a profile is assembled from lists naming more paths than any
one machine has, so without this a machine with no `~/.pyenv` is a machine where every program is
refused. This decides what the policy names and is therefore not the backend dropping a grant that
SANDBOX-6 forbids: the caller is told which paths went, so the difference between the grant somebody
decided on and the grant a program got is visible where it can be acted on.

`verified-by: bravebot_sandbox::policy::a_backend_that_grants_an_absent_path_is_asked_for_the_policy_as_wanted`
`verified-by: bravebot_sandbox::policy::a_path_that_is_not_on_disk_is_left_out_and_named`
`verified-by: bravebot_sandbox::policy::a_path_that_is_there_stays_in_the_list_it_was_named_in`
`verified-by: bravebot_sandbox::policy::resolution_carries_the_network_and_subprocess_grants_unchanged`
`verified-by: bravebot_sandbox::policy::a_path_wanted_for_reading_and_for_writing_is_named_once_when_it_is_left_out`
`verified-by: bravebot_sandbox::linux::a_policy_refused_over_an_absent_path_is_one_this_backend_installs_once_it_is_resolved`

## Programs a person asked for

A program `run` ([tools/run.md](tools/run.md)) starts is unconfined: it gets the access the user's
own shell would give it, and the reason `run` gives is that `git push` needs `~/.ssh` and the
programs somebody might ask for cannot be listed in advance. The decision is that confinement is
added, and that what it bounds is the filesystem: a program is held to the paths the plan a person
endorsed accounts for, and not to whatever else it could open. Nothing in this section is in force.
A `run` profile is what puts it in force, and the clauses above are what such a profile is then held
to.

**The grant is the plan, not the prompt.** A command line compiles to a plan carrying its read set,
its write set and each stage's resolved binary, and that plan is what a person is shown and what an
endorsement binds to ([tools/command-line.md](tools/command-line.md)). The profile is built from the
plan, so a line nobody is asked about gets the same one as a line somebody answered: a proven line,
a command already answered this session, one a settings rule stops the asking for, and a line under
the mode that answers every permission question are each held to what their own plan accounts for.
Nothing a program prints reaches the profile, and no value the model supplied chooses one beyond the
plan a person could read.

**The base is reviewed as code.** The loader, the system directories and the temporary directory are
shown by no prompt, so they are a fixed part of the profile rather than a grant. A profile denies
everything and then names what may be reached, so "everything except this key" is not a profile
anybody can write, and something has to carry what every program needs before any plan is read. That
list is code: it is the same for every plan, nothing the model supplied and nothing an argv carries
adds to it, and it changes only in a diff somebody reviews. What it buys is that it holds no
credential directory, so `~/.ssh/id_rsa` and `~/.aws/credentials` are out of reach of a program
whose plan never named them.

| The base holds | To |
|---|---|
| the loader, the system libraries, the system binary directories, the locale data, terminfo, the CA bundle and the certificate directory beside it, `/etc/hosts`, `/etc/resolv.conf`, `/etc/nsswitch.conf`, `/etc/passwd`, `/dev/null`, `/dev/zero`, `/dev/random` and `/dev/urandom` | read, and write for `/dev/null` |
| the system temporary directory this process resolved as the session opened | read and write |
| the git configuration any stage may read for an identity: `~/.gitconfig` and `~/.config/git/config` | read |

Three rows is the whole of what stays invisible, and each one is here because every program needs it
and none of it sits beside a token. The prelude is what a dynamic executable needs to start at all.
The git configuration is in the base rather than in one program's list because a stage that never
mentions `git` still shells out to it for an identity, a `cargo` fetching a git dependency among
them, and it can name a credential store without holding one. The remote scope below naming
`~/.gitconfig` as well costs nothing. What keeps the base to three rows is that a row shown on every
run is a row that teaches a person to approve without reading, and a row shown on no run is one
nobody audits: neither is free, so the split is by whether every program needs it.

The temporary directory is the one this process resolved as the session opened, and not one a
stage's own environment assignment names, so a line carrying a `TMPDIR=` assignment changes where a
program writes without changing what the profile allows. A program that cannot open a temporary file
fails outright, and that directory is one every process on the machine already reaches, so the base
names it. What it costs is that this session's own scratch directory sits inside it
([trust-map.md](trust-map.md)): an intermediate file a turn left there is readable by a program whose
plan never named it, and the mode that directory is created with does not separate two processes
running as the same account.

**A toolchain and its caches are the list a program brings.** The paths a build resolves through
belong to that build rather than to every program that runs, so they are keyed on the resolved
binary a stage of the plan names and are shown in the prompt with the rest of the plan. An `npm ci`
is held to the npm rows and a `cargo build` in the same session to the cargo rows, so a postinstall
script cannot leave something in `~/.cargo/registry` for a later `cargo build` to read. The key is
the binary the plan already resolved and a person already read, never a name the model supplied and
never a value a configuration file holds.

| The list for | Read | Read and write |
|---|---|---|
| `cargo` | `~/.rustup`, `~/.cargo/bin`, `~/.asdf` | `~/.cargo/registry`, `~/.cargo/git`, `~/.cargo/.package-cache` |
| `node`, `npm`, `npx` | `~/.nvm`, `~/.asdf` | `~/.npm/_cacache` |
| `python`, `pip` | `~/.pyenv`, `~/.asdf` | `~/.cache/pip` |
| `go` | `~/.asdf` | `~/.cache/go-build`, `~/go/pkg/mod` |
| `mvn` | `~/.asdf` | `~/.m2/repository` |
| `gradle` | `~/.asdf` | `~/.gradle/caches`, `~/.gradle/wrapper`, `~/.gradle/native` |
| `git` | the configuration directory of each editor this row names, which is what the editor `git commit` opens reads | that editor's own state directory |

A cache is writable because a build that cannot write one fetches everything again or fails
outright, and the price is a write no plan accounted for: a program can leave something in its own
ecosystem's cache for a later build in that ecosystem to read. Holding that to one ecosystem is what
keying on the binary buys, since the write a `cargo build` is trusted with is one an `npm ci` never
receives. An install is read-only for the opposite reason. A build that would install a toolchain
fails, and a line somebody then runs unconfined costs less than letting a confined stage replace the
`cargo` or the `node` a later stage in the same pipeline resolves to.

**A list names a cache, never the directory holding it.** A package manager keeps its token beside
its cache, so naming the parent would grant the token with it: `~/.cargo/credentials.toml` sits in
`~/.cargo`, `~/.m2/settings.xml` holds a server password, and `~/.gradle/gradle.properties` holds a
signing key. Each row above names the subdirectory a build reads, and a token file is out of every
list by never being named. Where a tool keeps state at the top of its home directory rather than in
a subdirectory, that file is named on its own, which is why the cargo lock `~/.cargo/.package-cache`
is in a row beside the registry and the token file next to it is not. The same rule keeps
`~/.config` out, since `~/.config/gh` is a credential store the remote scope below grants, so the
XDG git configuration and an editor's configuration are named one directory at a time rather than
through the directory they sit in. Which editors the `git` row knows is in that row and is not read
from `core.editor`, since no configuration file's contents decide what a list holds. `$HOME` itself
is in no row, so a file in it that no row names, `~/.npmrc` and `~/.pypirc` among them, is
unreachable. What this costs is an editor no row names and an editor installed outside the system
binary directories: the first opens without its configuration, the second cannot start at all, and
naming the directory is a person's to do in either case.

**A command no list knows is asked about, and the answer lasts the session.** A wrapper is the
common case rather than the edge one: `make check` here, a `just` recipe or an `npm run` target
elsewhere, and the binary such a stage resolves is `make` or `just` and not the build it goes on to
drive. That stage gets the base and its plan and nothing else, so a build inside it that needs a
cache fails. The run that failed stays failed; what follows it is a question naming the lists above,
and a person attaches the ones that command turns out to need. Those lists are the whole of what the
question can offer, so a build made to fail in a chosen way puts no path of its own in front of
anybody, which is what keeps this on the right side of nothing widening in answer to a refusal
below.

The answer lasts the session, a `--resume` carries it and `/status` lists it, on the terms
`/add-dir` already sets ([trust-map.md](trust-map.md)). Keeping it past a `/clear` is a person
writing it in `permissions` ([permissions.md](permissions.md)), since an attachment outliving the
answer that allowed it is the durable reach a session-scoped answer exists to avoid leaving behind.
What this costs is a wrapper answered once in every session that runs one, and a person who attaches
every list to a single command has one shared list back, though only for that command and only by an
answer somebody gave.

**A credential is a scope the plan carries.** A push is how most sessions end, so a profile that
refuses one is a profile somebody turns off, and a scope a person has to go and find first is that
refusal with a step in front of it. A plan therefore carries a third thing beside its read set and
its write set: the credential scope each stage needs, shown in the prompt with the rest of the plan.
A scope grants a stage no more than the same stage reaches when it is not confined, no scope grants
a private key, and what confinement buys on disk is what every other stage in the pipeline loses.

A scope bounds files and nothing else. A stage receives the environment this process holds
([tools/run.md](tools/run.md)), so a token sitting in it, `CARGO_REGISTRY_TOKEN` or `GITHUB_TOKEN`,
reaches every stage of a pipeline whatever scope each one carries, and no filesystem grant is what
put it there or can take it away.

| A stage whose operation | Reaches |
|---|---|
| is `git push`, `fetch`, `pull`, `clone` or `ls-remote`, or is `gh` | the remote scope below |
| is `aws`, `kubectl` or `docker` | that one tool's credential directory |
| is anything else, a build or a test in the same pipeline included | none of it |

A stage reaches its own row and no other: a `docker` stage reaches neither the remote scope nor
`~/.aws`, and no row reaches a private key, `~/.ssh` as a directory, or the keychain database on
disk. The remote scope is the agent socket `$SSH_AUTH_SOCK`, `~/.ssh/config` and
`~/.ssh/known_hosts` to read with `known_hosts` also to write, the public keys in that directory,
`~/.gitconfig`, and the stores an https helper reads: `~/.git-credentials`, `~/.netrc`,
`~/.config/gh`, and the login keychain through the system service that holds it. A push signs
through the agent and needs no private key, and the public half is in the scope because that is what
ssh reads to name an identity to the agent where a configuration file pins one. It does need
`known_hosts`, since a host it cannot verify is a push that fails. Both transports are in one scope
because which one a remote uses is written in a configuration file, and no file's contents decide a
scope.

What the scope costs is that a repository's own hooks run under `git`, so a push somebody asked for
can read a token store while it runs. The agent socket is the narrower shape of the same thing, and
is why the ssh half of the scope names no private key: a hook that reaches the socket can sign with
the key for as long as the push lasts, and cannot take it.

**A scope is keyed to the operation, and to no file's contents.** `git` is an arbitrary command
runner given the right argv, since `-c core.sshCommand=`, `-c alias.x=!`, `-c credential.helper=`
and `--exec-path` each turn it into a way to run something else. A scope therefore follows the
operation the endorsed argv names rather than the first word of it: `git status` in a pipeline gets
none, and an argv carrying one of those overrides gets none either, because what would run is not
what the plan says would run. Deriving a scope from a configuration file instead would let a cloned
repository's own `.git/config` choose which secret becomes reachable. The price of reading none of
them is a scope naming stores a given machine does not use, and an `include.path` in `~/.gitconfig`
naming a file no scope covers.

**Widening happens before the run, and nothing widens after a refusal.** The compiler adds the
scope, a person sees it in the plan they endorse, and there is nothing new to trust, because the
grant is still the plan. Widening in answer to a refusal is rejected: a denial reaches this process
as an exit status and no backend here hands it a path, so the only place the wanted path appears is
what the program printed, which is content a repository controls. A build made to fail in a chosen
way would otherwise become a prompt asking a person for `~/.ssh` with a plausible reason. A backend
that did report the path would not settle it, since the path it reports is still the one the program
chose to touch. Nothing grants what a program just failed to reach.

**A person is the other route, and the only route to the key.** A session where no agent holds the
key, or one wanting a store no scope names, a publish reading `~/.cargo/credentials.toml` or
`~/.npmrc` among them, still needs somebody to name a directory: `/add-dir` in a session,
`--add-dir` on the command line, and `additionalDirectories` in a settings file, which is put as a
question of its own when the session opens ([trust-map.md](trust-map.md), [cli.md](cli.md),
[permissions.md](permissions.md)). A rule about which commands to ask about is not one of them,
because such a rule stops a question rather than extending reach. Only this route needs a path to be
nameable without being vouched for, since a scope the compiler adds is a grant in a profile and
records nothing about what anybody trusts.

**Turning the scopes off.** `--credential-scopes` takes `planned` or `withheld`, and
`run.credentialScopes` in a settings file takes the same two values. `withheld` leaves every scope
out of every plan, so a stage reaches a credential only where a person named its directory. What it
is for is a machine whose secrets are not this session's to lend: a shared build host, or one an
administrator sets up for somebody else to work on. `planned` is the default, because almost every
session needs a scope and a default people cannot work with is one they switch off wholesale. Either
way the record of the run names each scope and the stage it was added for ([trace.md](trace.md)).

**The network stays open to it.** A profile gates egress as a whole, so it cannot tell an approved
`git push` or `gh api` from an exfiltration, and the endorsed argv already can. What confinement
narrows is what a program may read and write, not what it may send: a confined one still sends
anything inside its grants. The label on what a program prints is untouched, and no grant makes an
output trusted.

**What has to exist first.**

- A path has to be nameable for a profile without being vouched for, on the route a person takes.
  Opening a directory in a session records it as trusted as well as reachable, and the two are
  deliberately one grant there because either half alone is no use to a tool. A confinement scope
  wants the reach and not the vouching: naming `~/.ssh` so a push can sign must not make a key
  file's contents trusted content. The command-line form already separates them, and the two session
  forms do not. A scope the compiler adds needs none of this, since it grants reach inside a profile
  and writes nothing to the record of what a person vouched for.
- A policy names paths, in a list to read and a list to write, and a socket is neither. On macOS a
  connect to a unix socket is a network operation, so the agent socket is reachable exactly while a
  `run` profile leaves egress open, and the first profile narrowing egress has to be able to name
  the socket instead. On Linux the right that governs connecting to a pathname socket arrives many
  ABI versions after the one this backend targets, so a connect there is neither granted nor
  deniable, and a profile meaning to bound one needs that ABI and a kernel carrying it.
- A profile has to say, for a path it means a program to create, whether that path is a file or a
  directory. A wanted path that is not on disk is left out of the policy on the backend that cannot
  name one ([SANDBOX-9](#SANDBOX-9)), which is the whole answer for a row that is only read, since
  there is nothing at such a path to read either way, and which is why a machine with no `~/.pyenv`
  is not a machine where every `run` is refused. What it costs is a row a program is meant to write into
  and would have created for itself: a toolchain cache directory on a machine that has not run that
  toolchain, and `~/.ssh/known_hosts` on a fresh account, each of which becomes a push or a build
  that fails rather than a `run` that does not start. Keeping those means creating the path before
  the policy is built, which a profile cannot ask for while a row says only a path, and naming the
  directory holding it instead is not open to `known_hosts`, whose directory holds the key no scope
  reaches.
- Windows has published binaries and no backend, and [SANDBOX-1](#SANDBOX-1) refuses to run a
  process it cannot confine, so confinement there refuses every program until that platform has one
  (issue #88). Running unconfined where no backend exists is the degradation that clause forbids.
  Windows is a platform this project supports, so this is a defect being carried rather than the
  price of a platform nobody ships to.
