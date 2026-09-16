---
id: SANDBOX
title: Confining subprocesses
status: normative
governs:
  - crates/sandbox/src/lib.rs
  - crates/sandbox/src/policy.rs
  - crates/sandbox/src/linux.rs
  - crates/sandbox/src/macos.rs
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
something the backend cannot deliver is refused. `bravebot doctor` reports the level in force.

**Why.** An overstated capability is the same failure as a silent fallback, reached by a different
road.

`verified-by: bravebot_sandbox::macos::capabilities_report_kernel_enforcement`
`verified-by: bravebot_sandbox::linux::capabilities_do_not_overstate_network_denial`
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
that refusal and the exec.

**Why.** A policy is the list a caller decided a program may reach, so a process running under
fewer of those paths than the policy names is the silent degradation [SANDBOX-1](#SANDBOX-1)
forbids, reached one grant at a time: the process runs, the record says the policy was applied,
and the program is refused a path somebody granted it. Which paths a backend can name is a
platform difference a caller can work with, and a grant that vanished without being reported is
not.

`verified-by: bravebot_sandbox::linux::a_path_that_cannot_be_opened_is_refused_rather_than_dropped`
`verified-by: bravebot_sandbox::linux::a_ruleset_is_not_built_with_a_path_missing_from_it`
`verified-by: bravebot_sandbox::macos::a_path_that_is_not_there_yet_is_granted_as_named`

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

**The base is reviewed as code.** The loader, the system directories and a toolchain are shown by no
prompt, so they are a fixed part of the profile rather than a grant. That base holds no credential
directory, which is the whole of what this buys: `~/.ssh/id_rsa` and `~/.aws/credentials` are out of
reach of a program whose plan never named them. It does hold the caches and version manager
directories a build resolves through, and not the token files beside them: `~/.cargo/registry` and
`~/.npm/_cacache` are in it, `~/.cargo/credentials.toml` and `~/.npmrc` are not. The configuration
of the editor `git commit` opens is in it too. Leaving a cache out breaks every build and protects
nothing, so a cache is part of the base rather than an exception somebody is shown.

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
- A profile has to name paths that are already there, on the backend that cannot name any other
  kind. A path that does not exist yet goes into a macOS profile and the file can then be created,
  while on Linux a grant is a right on an open descriptor, so such a path cannot be named in one at
  all and the policy is refused ([SANDBOX-6](#SANDBOX-6)). A scope naming `~/.ssh/known_hosts` on a
  fresh account is therefore a `run` that does not start rather than a push that fails for no stated
  reason, and what the compiler owes the profile is to create the file, name the directory holding
  it, or leave the scope out.
- The base has to be written down. A profile as generated here denies everything and then names what
  a program may reach, on both backends, so "everything except this key" is not expressible and the
  base has to carry what an ordinary build reads.
- The macOS backend clears the environment of a process it wraps. A stage receives the environment
  this process holds, less the credentials this agent authenticates with, so a `run` profile needs a
  backend that leaves the rest of that environment alone. The ssh half of the remote scope depends
  on this one: a program that cannot see `$SSH_AUTH_SOCK` cannot use the socket, whatever a profile
  allows.
- The Linux backend targets the first Landlock ABI, which carries no right for renaming or linking a
  file into another directory, and a ruleset that does not handle that right denies the operation
  outright. Writing a temporary file and renaming it into place is what a compiler and a package
  manager do. The ABI carrying the right raises the oldest kernel this backend runs on from 5.13 to
  5.19, and asking for it best-effort on an older one drops it again silently, so which kernels are
  covered is part of this rather than a detail of it.
- Windows has published binaries and no backend, and [SANDBOX-1](#SANDBOX-1) refuses to run a
  process it cannot confine, so confinement there refuses every program until that platform has one
  (issue #88). Running unconfined where no backend exists is the degradation that clause forbids.
