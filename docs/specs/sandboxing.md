---
id: SANDBOX
title: Confining subprocesses
status: normative
governs:
  - crates/sandbox/src/lib.rs
  - crates/sandbox/src/base.rs
  - crates/sandbox/src/policy.rs
  - crates/sandbox/src/linux.rs
  - crates/sandbox/src/macos.rs
  - crates/sandbox/src/windows.rs
  - crates/sandbox/src/windows/appcontainer.rs
  - crates/sandbox/src/process.rs
  - crates/sandbox/src/toolchain.rs
  - crates/sandbox/src/hosts.rs
  - crates/sandbox/src/proxy.rs
  - crates/sandbox/src/address.rs
  - crates/sandbox/src/scope.rs
  - crates/cli/src/push_identity.rs
  - crates/sandbox/src/mode.rs
  - crates/config/src/sandbox.rs
  - crates/config/src/sandbox_network.rs
  - crates/config/src/git_identities.rs
  - crates/agent/src/confine.rs
  - crates/agent/src/reach.rs
  - crates/sandbox/src/rules.rs
  - crates/sandbox/src/programs.rs
  - crates/sandbox/src/openshell.rs
  - crates/tui/src/status.rs
  - crates/tui/src/opener.rs
  - crates/tui/src/powershell.rs
  - ui/scripts/host-card.test.mjs
  - ui/scripts/shown.test.mjs
guards:
  - symbol: find_in
    sites:
      - crates/agent/src/confine.rs: 1
      - crates/agent/src/programs.rs: 6
      - crates/agent/src/servers.rs: 2
      - crates/agent/src/workspace.rs: 1
      - crates/agent/tests/workspace.rs: 1
      - crates/sandbox/src/programs.rs: 19
      - crates/sandbox/tests/checkout_in_search.rs: 7
      - crates/tui/src/clipboard.rs: 9
      - crates/tui/src/editor.rs: 2
  - symbol: find_in_with
    sites:
      - crates/sandbox/src/programs.rs: 11
      - crates/tui/src/clipboard.rs: 4
documented-by:
  - docs/website/docs/security/security.md
  - docs/website/docs/security/sandbox.md
---

## Scope

Operating-system confinement for processes that run code we did not write, which today means the
stdio servers in [mcp.md](mcp.md). What this is *not* for is our own code: a processor is a model
call made by our own code, and confining that would fence in the trusted half and leave the
untrusted half free. A program the user asked for is the one case of our own that this does cover,
because the code it runs is not ours: `run` starts it under the profile its plan accounts for on
Linux and macOS ([SANDBOX-17](#SANDBOX-17)), and the last section here is what that profile is. The inhibitor `/caffeinate` starts
is neither: its program and arguments are fixed in our code
([commands.md](commands.md#CMD-12)), so it is not confined, and it is found as
[SANDBOX-30](#SANDBOX-30) says.

Confinement is an operating-system boundary. Everywhere else in these specs the boundary is the
capability set and the label on a value, which is a different mechanism answering a different
question.

On Linux the boundary needs the kernel right that governs moving a file, which arrived in
Landlock's second version rather than its first, so a kernel carrying Landlock without that right
is one where confinement is unavailable and a process is refused rather than run under a policy
that cannot be applied in full ([SANDBOX-7](#SANDBOX-7)). What that costs is the kernels between
5.13 and 5.19, which a long-term distribution release still ships, and on which nothing runs
confined at all.

That version is the floor a kernel is refused below and not the set of rights the confinement
covers. What a Landlock ruleset restricts is the rights it handles, and a right it does not handle
is checked nowhere: the kernel passes every caller in that domain. So the confinement handles every
right this version of Landlock knows of, narrowed to the rights the kernel in front of it carries.
Handling only the rights the floor requires would leave each right Landlock has gained since
outside the boundary, unrestricted and unnameable by any policy, which is how a process confined to
a few directories empties a file anywhere the account can reach.

Each right above the floor is one Landlock counts as a write, so a grant for reading does not
carry it: emptying a file, driving a device rather than reading it, and reaching a socket by its
path are what naming a path for writing permits. A read grant that carried any of them would be a
policy's two lists saying one thing.

**A right younger than the kernel restricts nothing there.** The narrowing is the kernel's and
cannot be argued with: the right that governs emptying a file arrived in Landlock's third version,
so on a kernel carrying only the second (5.19 up to 6.2) a confined process can empty a file
outside its grants, without reading it or opening it for writing. Refusing every kernel below the
newest right would refuse every kernel, since each version adds one, so the floor is where a
*missing* right would deny operations *inside* the grants and the rights above it are enforced
wherever the kernel has them. The gap that leaves is a kernel to upgrade.

On Windows the boundary is an AppContainer: a lowbox token is denied every securable object whose
access-control list does not name the container, so a grant is an entry written onto the directory
the policy names, and egress is a capability the token either carries or does not. A grant is
therefore a change to the filesystem rather than to the process, which is the one thing the other
two backends do not cost, and what follows from it is below.

Where a clause here is not built, or has an unbuilt half, the clause says so, and that sentence is
what keeps a reader from taking the present tense for a claim about what runs today.
[SANDBOX-11](#SANDBOX-11) is the one that says it.

## Clauses

<a id="SANDBOX-1"></a>
### SANDBOX-1: confinement fails closed

If confinement cannot be established the process does not run. An unavailable backend refuses to
spawn rather than falling back, and the platform lookup never hands back a backend that would
confine nothing.

The one place a program starts without a backend is a platform with no base to build a profile on.
There is none today: a program `run` starts is refused on every platform when the backend will not
apply its profile ([SANDBOX-17](#SANDBOX-17)).

**Why.** Silently degrading is worse than an error: the caller believes it has a guarantee it does
not have, and the audit trail records a sandbox that was never applied.

A backend that confines a program the caller names (Landlock, Seatbelt, AppContainer) refuses a
program that is not an absolute path, with `SetupFailed`, before it builds a command, writes a grant
or starts a process. A bare or relative name would be found after the confinement is installed, by
a search the caller's `PATH` and working directory decide. The agent starts each step from the
absolute path it resolved, so its own path never meets the refusal. A local server whose program
name is not text is not started: `start_under` in `crates/agent/src/servers.rs` refuses it before
the launch, with a note that shows the path lossily. `resolve` produces only programs that are
text, so a declaration does not reach it.

The OpenShell backend does not make this check. It does not search the
person's `PATH` after a confinement is installed on this machine: the program runs inside the image
the sandbox was made from, where the image's own `PATH` finds a bare name (`real_path` finds it with
`command -v`), and an absolute path on this machine names nothing there. The planner passes the
program as the declaration spells it, so requiring an absolute path would refuse every declaration.

`verified-by: bravebot_sandbox::lib::only_an_absolute_program_passes_the_check_every_backend_makes`
`verified-by: bravebot_sandbox::linux::a_program_that_is_not_an_absolute_path_is_refused_before_a_command_exists`
`verified-by: bravebot_sandbox::macos::a_program_that_is_not_an_absolute_path_is_refused_before_a_command_exists`
`verified-by: bravebot_sandbox::windows::a_program_that_is_not_an_absolute_path_is_refused_before_a_container_exists`
`verified-by: bravebot_agent::servers::a_local_server_whose_program_name_is_not_text_is_not_started`
`verified-by: bravebot_agent::servers::an_openshell_declaration_is_planned_with_its_program_as_named_in_the_image`
`verified-by: bravebot_sandbox::lib::an_unavailable_backend_refuses_to_spawn`
`verified-by: bravebot_sandbox::lib::refusal_is_not_a_silent_fallback`
`verified-by: bravebot_sandbox::lib::the_platform_lookup_never_returns_an_unconfined_backend`
`verified-by: bravebot_sandbox::lib::errors_explain_the_refusal`

<a id="SANDBOX-2"></a>
### SANDBOX-2: a policy that would confine nothing is refused

A profile starts denying everything, grants accumulate onto it, and a fully permissive policy is
rejected rather than applied. Granting everything is not a confinement decision. A write row
grants everything when it resolves to the root of a filesystem, whatever its spelling: `/..` is the
same grant as `/`, and on Windows a drive root such as `C:\` is one too.

`verified-by: bravebot_sandbox::policy::strict_permits_nothing`
`verified-by: bravebot_sandbox::policy::allowances_accumulate`
`verified-by: bravebot_sandbox::policy::a_strict_policy_is_meaningful`
`verified-by: bravebot_sandbox::policy::granting_everything_is_not_meaningful`
`verified-by: bravebot_sandbox::policy::granting_everything_spelled_another_way_is_not_meaningful`
`verified-by: bravebot_sandbox::policy::a_grant_below_the_root_remains_meaningful`
`verified-by: bravebot_sandbox::policy::granting_a_drive_root_is_not_meaningful`
`verified-by: bravebot_sandbox::policy::network_alone_remains_meaningful`
`verified-by: bravebot_sandbox::macos::a_fully_permissive_policy_is_refused`
`verified-by: bravebot_sandbox::linux::a_fully_permissive_policy_is_refused`
`verified-by: bravebot_sandbox::windows::a_fully_permissive_policy_is_refused`

<a id="SANDBOX-3"></a>
### SANDBOX-3: the network is denied unless it was asked for

A confined process reaches neither the network nor the filesystem outside its grants. Egress,
where a policy asks for it, is to IP addresses. A unix socket that has a path is reached only under
a path the policy names for writing, and not under one it names for reading. A refusal of a read
or of a write inside a write row refuses a connect beneath it as well. A narrower write row beneath
the refusal reaches the sockets under it, and so does a write row at a refusal of a read. A write
row at a refusal of a write is withdrawn and reaches none. On macOS a connect to
a socket counts as egress, so there a write row reaches a socket only while egress is granted, and
egress also reaches the resolver's socket, which every host name lookup goes through. On Linux the
socket rule holds on a kernel carrying Landlock ABI version 9, which is Linux 7.1. Below it, which
is most Linux machines in use, every socket that has a path is reachable whatever the policy names,
which includes the user's D-Bus (where `systemd-run --user` starts a program outside the sandbox),
the agent `SSH_AUTH_SOCK` names (every key it holds) and `docker.sock`. The Linux backend reports
whether the kernel governs these sockets. Where it does not, a stage under sandbox mode `strict` or `read-only`
([SANDBOX-22](#SANDBOX-22)) is refused with the kernel it needs named, and a stage under `workspace-write`
runs while `doctor`, `/status` and the opening screen say the sockets are reachable. Under `off`
`/status` and the opening screen say nothing of them, since the confinement line already says
nothing is confined. The mode does not apply to a local MCP server, which below version 9 reaches
these sockets under every mode, as `doctor` says. A connect to a socket in the abstract namespace,
which has no path, is refused when the socket was made outside the confined process's Landlock domain, on a kernel
carrying ABI version 6, and is reachable below it. The domain is that of one stage, so a socket made
by the session or by another stage is refused too. On Linux signals are not scoped. On macOS a
confined process signals the processes its own profile started and no other: not the session, not
another stage, and no other process of the account. A backend that cannot
enforce the network denial refuses the policy instead, so the guarantee never degrades into one
that is not in force. On Linux the network is denied by a new user and network namespace, in which
the stage has its own loopback and nothing else, whatever the address, port or protocol. A stage
held to a proxy ([SANDBOX-24](#SANDBOX-24)) has one more process in its namespace, the forwarder,
which no ruleset confines. It relays only to the proxy's own listener, so it is not a way to a
socket the stage's rows refuse. Landlock's network rules are not used: they filter a TCP connect
by port and not by address, and leave UDP open. A kernel that does not let an unprivileged user
make the namespace (Ubuntu 24.04 restricts it by default, and many containers block it) cannot
deny the network, and `doctor` says so. On Windows the network is the container's internet client
capability, and a container without it is refused a host name lookup as well as a connection.
Which programs a session asks egress for is [SANDBOX-20](#SANDBOX-20)'s.

On macOS the profile also refuses binding a port and accepting on it, so a confined stage cannot
listen on the loopback interface, whether or not it has egress, and a server it starts, such as the
one `sccache` runs, or a test that binds `127.0.0.1:0`, fails with `Operation not permitted`. A
stage that has egress and is held to no proxy's port connects to a port of this machine without
loopback, since its egress rule matches the loopback address, and a stage that has none is refused
the same connection. A stage the plan lends loopback ([SANDBOX-26](#SANDBOX-26)) may bind and accept
on this machine's own addresses and connect to them, and reaches no other address and no resolver.
That grant is separate from egress: a closed network stays closed for it, so a stage with no egress
that is lent loopback connects to a port of this machine and to no other address, and to a stage
that has egress it adds only listening. A stage held to a proxy's port
([SANDBOX-24](#SANDBOX-24)) may listen when lent loopback and connects to that port alone, with or
without loopback, since another local port could be a service that carries it past the host list.
A host list holds a stage to the proxy only when the stage has egress. A stage with no egress is
held to no proxy, so loopback lets it connect to a port of this machine whether or not a host list
is set.

On Linux and Windows nothing in the backend reads a request for loopback, so it changes nothing
there. On Linux a stage with the network open shares the host's network and connects to a port of
this machine, and this clause says nothing of whether it can listen, since no test shows it. A
stage with no network, or held to a proxy's port, is in the network namespace described above,
which holds no listener of the host's loopback, and a request for loopback does not take a stage
with no network out of it. On Windows a policy that names loopback asks for the same capabilities,
makes the same grants and is refused or applied as the same policy without it. One test connects
to a listener of this machine's loopback from a container granted the network
(`a_container_granted_the_network_cannot_connect_to_this_hosts_loopback`). It passed in the
`Sandbox on Windows` job of CI run 42972425516 (job 130363809021), where the probe printed `loopback
127.0.0.1:56331 from a container granted the network: refused: connection timed out`, so the
connect did not reach the listener and ended by timing out and not by an immediate refusal. No
test listens on a port from a container, so this clause says nothing of listening.

**Why.** A connect to a socket reaches whatever serves it, with that server's authority. A program
that reaches a Docker daemon's socket can start a container with the home directory mounted, which
is every file the profile withheld. Landlock counts the connect as a write, so both backends apply
one rule to a connect. A program that cannot signal a child it started cannot stop it, so a test
runner or a `timeout` that gives up on one waits until it ends by itself.

**What it costs.** The temporary directory is a write row of every stage
([SANDBOX-12](#SANDBOX-12)), and other programs of the account keep sockets there, a browser's
single-instance socket among them. A stage reaches each of those with that server's authority, on
macOS while it has egress and on Linux whatever its network.

`verified-by: bravebot_sandbox::macos::network_is_only_allowed_when_requested`
`verified-by: bravebot_sandbox::macos::a_confined_process_cannot_reach_the_network`
`verified-by: bravebot_sandbox::macos::loopback_is_granted_by_its_own_rows_and_opens_no_other_address`
`verified-by: bravebot_sandbox::macos::a_process_granted_loopback_connects_to_a_local_port_and_one_without_it_cannot`
`verified-by: bravebot_sandbox::macos::a_process_granted_loopback_can_listen_and_one_without_it_cannot`
`verified-by: bravebot_sandbox::macos::an_open_network_connects_to_a_local_port_without_loopback_and_a_closed_one_needs_it`
`verified-by: bravebot_sandbox::macos::a_process_with_an_open_network_listens_only_when_granted_loopback`
`verified-by: bravebot_sandbox::macos::loopback_leaves_a_stage_held_to_a_port_with_that_port_alone`
`verified-by: bravebot_sandbox::macos::a_process_held_to_a_port_and_granted_loopback_reaches_no_other_local_port`
`verified-by: bravebot_agent::confine::only_a_stage_with_egress_is_pointed_at_the_proxy_and_only_under_a_list`
`verified-by: bravebot_agent::confine::a_requested_loopback_lends_loopback_and_no_network`
`verified-by: bravebot_sandbox::macos::a_confined_process_granted_egress_cannot_reach_a_socket_outside_its_grants`
`verified-by: bravebot_sandbox::macos::a_write_grant_reaches_no_socket_inside_a_refusal`
`verified-by: bravebot_sandbox::macos::a_confined_process_granted_egress_can_reach_the_resolver`
`verified-by: bravebot_sandbox::macos::a_refusal_over_the_resolvers_directory_leaves_egress_the_resolver`
`verified-by: bravebot_sandbox::macos::a_confined_process_cannot_write_outside_its_grants`
`verified-by: bravebot_sandbox::macos::a_confined_process_runs`
`verified-by: bravebot_sandbox::macos::a_confined_process_can_signal_a_process_it_started`
`verified-by: bravebot_sandbox::macos::a_confined_process_cannot_signal_a_process_outside_its_sandbox`
`verified-by: bravebot_sandbox::linux::a_policy_requiring_network_denial_is_refused_where_no_namespace_can_be_made`
`verified-by: bravebot_sandbox::linux::a_kernel_that_refuses_the_namespace_refuses_the_stage`
`verified-by: bravebot_sandbox::linux::a_stage_with_no_network_cannot_connect_to_anything`
`verified-by: bravebot_sandbox::linux::a_stage_held_to_the_proxy_cannot_connect_around_it`
`verified-by: bravebot_agent::confine::a_closed_network_stops_a_program_with_no_reason_to_reach_it`
`verified-by: bravebot_agent::confine::a_requested_loopback_reaches_a_listener_from_a_closed_network_on_macos_only`
`verified-by: bravebot_sandbox::linux::a_stage_that_replaces_the_proxys_socket_with_a_link_is_not_relayed_to_what_it_leads_to`
`verified-by: bravebot_sandbox::linux::a_confined_process_runs`
`verified-by: bravebot_sandbox::linux::a_confined_process_can_write_inside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_cannot_write_outside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_cannot_read_outside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_cannot_truncate_a_file_outside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_can_truncate_a_file_inside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_cannot_drive_a_device_it_was_granted_for_reading`
`verified-by: bravebot_sandbox::linux::the_ruleset_handles_every_right_this_crate_knows_of`
`verified-by: bravebot_sandbox::linux::a_confined_process_cannot_connect_to_an_abstract_socket_made_outside_it`
`verified-by: bravebot_sandbox::linux::a_socket_with_a_path_is_reached_under_a_write_grant_and_not_under_a_read_grant_or_a_refusal`
`verified-by: bravebot_sandbox::linux::sockets_with_a_path_are_reported_governed_from_landlock_abi_9`
`verified-by: bravebot_agent::confine::strict_and_read_only_are_refused_where_sockets_with_a_path_are_not_governed`
`verified-by: bravebot_agent::confine::a_strict_or_read_only_stage_is_prepared_only_where_sockets_with_a_path_are_governed`
`verified-by: bravebot_cli::doctor::doctor_says_when_sockets_with_a_path_are_reachable_and_what_that_costs`
`verified-by: bravebot_tui::status::reachable_sockets_with_a_path_are_reported_under_workspace_write_alone`
`verified-by: bravebot_tui::logo::reachable_sockets_with_a_path_are_named_on_the_opening_screen_under_workspace_write_alone`
`verified-by: bravebot_sandbox::windows::a_policy_that_did_not_ask_for_the_network_asks_for_no_capability`
`verified-by: bravebot_sandbox::windows::a_policy_that_asked_for_the_network_asks_for_the_internet_client_capability`
`verified-by: bravebot_sandbox::windows::a_policy_withholding_the_network_is_applied_rather_than_refused`
`verified-by: bravebot_sandbox::windows::a_policy_granted_loopback_asks_for_what_the_same_policy_without_it_asks_for`
`verified-by: bravebot_sandbox::windows::no_grant_lets_a_confined_process_rewrite_an_access_list`
`verified-by: bravebot_sandbox::windows::a_container_connects_out_only_when_its_policy_asks_for_the_network`
`verified-by: bravebot_sandbox::windows::a_container_without_the_network_cannot_look_up_a_name`
`verified-by: bravebot_sandbox::windows::a_container_granted_the_network_cannot_connect_to_this_hosts_loopback`

<a id="SANDBOX-4"></a>
### SANDBOX-4: content cannot inject into the syntax a backend writes it into

Grants are paths and arguments are content. A backend writes both into a syntax: a profile the
kernel parses, or the single command line a platform's process creation takes instead of a vector.
Either is quoted rather than interpreted, so a confinement built from a hostile path still applies
and a program is asked for what the caller asked for.

On Windows an argument holding `*` or `?` is quoted, since a program built with a wildcard runtime
expands one left unquoted. `cmd.exe` reads its line by rules of its own rather than the ones the
quoting is written for, so a `cmd.exe` line is refused where the two readings differ: an argument
holds a quotation mark, an operator, `%`, `!`, a line end or one of the delimiters `,` `;` `=`, or
the first argument after `/c`, `/k` or `/r` is written quoted and another follows it. One argument
after the switch is the line `cmd.exe` is asked to run, and it starts.

Windows reads a program's file name differently from the text it is written in: it drops a dot or a
space that ends the name and reads what follows a `:` as a stream of the file. A program whose final
path component ends in a dot or a space, or holds a `:`, is refused before the batch-file and
`cmd.exe` tests are made, so those tests are never made on a name other than the one that is
started. A drive's colon and a directory ending in a dot are not in the final component and do not
refuse.

**Why.** A grant list assembled from paths would otherwise be a place where a filename decides what
the sandbox permits, and a command line assembled from arguments a place where one decides what the
confined program is told to do. `cmd.exe` drops the outer quotation marks of a line that starts with
one and acts on what it finds outside the rest, so no quoting holds for it.

**What it costs.** The Windows refusals judge the final component of the program as it is written.
A name that Windows resolves to the interpreter or to a batch file by another spelling is refused
when it ends in a dot or a space or holds a `:`, which covers `build.bat.`, `cmd.exe.`, `cmd.exe `
and `cmd.exe::$DATA`. Because those are refused outright, whether `CreateProcessW` starts the
interpreter for such a name is not needed and has not been checked on Windows. An 8.3 short name
(`CMD~1.EXE`, `BUILD~1.BAT`) is not recognised and is started with the quoting a program is given.
`is_cmd` also goes by the final file name alone, so a copy of `cmd.exe` called `tool.exe` that a
stage can write and run is started with the quoting a program is given, not the refusals `cmd.exe`
gets.

No test of the file is made, for these reasons. A copy has a file identity of its own, so identity
matches only a hard link. A version resource or a hash matches an unmodified copy, and the stage that
writes the file can change either while the program still reads its line as `cmd.exe` does. No test
of one program finds another that reads its line by rules of its own. A test that opened the file
would also run before `CreateProcessW` locates the program by its own search, so the file examined
need not be the file started, and the copy can be replaced between the two. So the decision reads
the program string only. It is made when the stage is prepared, before a profile exists and before
`CreateProcessW` is called, in `refusal_for_program` for both the agent and the backend, and it has
no failure path for a file that is missing or unreadable. The container still limits what the copy
can reach.

`verified-by: bravebot_sandbox::macos::paths_cannot_inject_profile_syntax`
`verified-by: bravebot_sandbox::macos::a_profile_containing_a_hostile_path_still_applies`
`verified-by: bravebot_sandbox::macos::granted_paths_appear_as_subpath_rules`
`verified-by: bravebot_sandbox::windows::a_path_containing_a_space_reaches_the_program_as_one_argument`
`verified-by: bravebot_sandbox::windows::a_quotation_mark_in_an_argument_does_not_end_it`
`verified-by: bravebot_sandbox::windows::a_path_ending_in_a_separator_does_not_swallow_the_argument_after_it`
`verified-by: bravebot_sandbox::windows::quoting_a_path_leaves_the_path_it_names_alone`
`verified-by: bravebot_sandbox::windows::a_backslash_before_a_quotation_mark_does_not_escape_the_escape`
`verified-by: bravebot_sandbox::macos::a_backslash_in_a_path_cannot_cancel_the_escape_of_the_quote_after_it`
`verified-by: bravebot_sandbox::windows::an_empty_argument_is_still_an_argument`
`verified-by: bravebot_sandbox::windows::an_argument_that_needs_no_quoting_is_written_as_it_is`
`verified-by: bravebot_sandbox::windows::an_argument_cmd_would_act_on_stays_quoted`
`verified-by: bravebot_sandbox::windows::an_argument_holding_a_wildcard_reaches_the_program_unexpanded`
`verified-by: bravebot_sandbox::windows::cmd_is_refused_an_argument_it_would_read_as_its_own_syntax`
`verified-by: bravebot_sandbox::windows::cmd_is_refused_a_quoted_first_argument_after_its_switch_with_another_after_it`
`verified-by: bravebot_sandbox::windows::cmd_starts_where_it_reads_the_line_as_it_was_written`
`verified-by: bravebot_sandbox::windows::a_program_name_windows_would_normalise_is_refused`
`verified-by: bravebot_sandbox::windows::a_program_path_whose_file_name_is_ordinary_is_not_refused_for_its_directories`
`verified-by: bravebot_sandbox::windows::cmd_spelled_with_a_trailing_dot_a_space_or_a_stream_starts_nothing`
`verified-by: bravebot_sandbox::windows::a_batch_file_spelled_with_a_trailing_dot_starts_nothing`
`verified-by: bravebot_sandbox::windows::only_the_final_file_name_makes_a_program_cmd`
`verified-by: bravebot_sandbox::windows::a_copy_of_cmd_under_another_name_is_started_as_a_program`
`verified-by: bravebot_agent::confine::a_cmd_line_cmd_would_read_otherwise_is_refused_before_its_redirection_is_opened`

<a id="SANDBOX-5"></a>
### SANDBOX-5: capabilities report what the kernel actually enforces, and never more

A backend says what it can enforce rather than what it was asked for, and a policy demanding
something the backend cannot deliver is refused. Which paths it can name is reported the same way.
`bravebot doctor` reports the level this platform can enforce.

On Windows the level is kernel-enforced, as on macOS. A container bounds the filesystem and the
network, and the job a stage is created in ([SANDBOX-17](#SANDBOX-17)) holds a limit of one active
process where the policy denies subprocesses, so a policy asking for that denial is applied there
and not refused. Linux is the platform that still refuses it.

**Why.** An overstated capability is the same failure as a silent fallback, reached by a different
road. An understated one costs the caller a grant it did not mean: a caller that cannot ask whether
a path missing from the disk is nameable reads the platform instead, and then creates a file nothing
asked for, or names the directory holding it, on the platform where neither was necessary.

`verified-by: bravebot_sandbox::macos::capabilities_report_kernel_enforcement`
`verified-by: bravebot_sandbox::linux::capabilities_report_the_network_only_where_a_namespace_can_be_made`
`verified-by: bravebot_sandbox::linux::a_path_that_does_not_exist_is_granted_exactly_where_the_capability_says_so`
`verified-by: bravebot_sandbox::macos::a_path_that_does_not_exist_is_granted_exactly_where_the_capability_says_so`
`verified-by: bravebot_sandbox::linux::a_policy_requiring_network_denial_is_refused_where_no_namespace_can_be_made`
`verified-by: bravebot_sandbox::linux::a_policy_requiring_subprocess_denial_is_refused`
`verified-by: bravebot_sandbox::lib::an_unavailable_backend_reports_no_confinement`
`verified-by: bravebot_sandbox::policy::confinement_levels_render_for_the_audit_trail`
`verified-by: bravebot_cli::doctor::doctor_names_the_confinement_level_in_force`
`verified-by: bravebot_cli::doctor::doctor_says_whether_the_kernel_enforces_network_denial`
`verified-by: bravebot_cli::doctor::confinement_that_could_not_be_established_fails_the_run`
`verified-by: bravebot_sandbox::windows::capabilities_report_what_a_container_enforces`
`verified-by: bravebot_sandbox::windows::a_policy_requiring_subprocess_denial_is_applied`
`verified-by: bravebot_sandbox::windows::the_job_bounds_processes_only_where_the_policy_denies_subprocesses`
`verified-by: bravebot_sandbox::windows::a_stage_cannot_start_a_child_where_its_policy_denies_subprocesses`
`verified-by: bravebot_sandbox::windows::each_run_confines_through_a_profile_of_its_own`
`verified-by: bravebot_sandbox::windows::a_reused_process_identifier_and_sequence_still_get_a_profile_of_their_own`
`verified-by: bravebot_sandbox::windows::a_profile_name_fits_what_the_platform_accepts`

<a id="SANDBOX-6"></a>
### SANDBOX-6: every path a policy names is granted, or the policy is refused

A policy is granted as written, apart from the `.git` writes [SANDBOX-14](#SANDBOX-14) withholds
on macOS. A backend that cannot install a grant for one of the paths refuses
the policy and names that path, rather than confining the process to the rest of them. Where a
backend can grant a path that does not exist yet, the profile carries that grant as named; where it
cannot, the refusal arrives before the process starts, and again where a path goes away, or is
replaced by another file, between that refusal and the exec. Which of the two a backend does is in its capabilities
([SANDBOX-5](#SANDBOX-5)).

A row that is an entry of a directory the caller listed is not a path the caller named, and
neither is an entry a backend lists to grant a row around a refusal beneath it. A caller
that grants a directory except what is refused beneath it passes the entries the directory had
when it was listed, marked as listed. A backend that opens each row before the process starts
leaves out a listed row whose name is no longer there, or is under something that is no longer a
directory, and refuses one that is there and cannot be opened. A backend that opens each row once,
before the process starts, and looks every name up again at the exec, decides a listed row and an
entry it listed the same way there: a name that is gone is left out, and one that is there and
cannot be looked up refuses. The same path named by the caller is refused as before, in a policy
that lists as in one that does not, and when a listed row names it too. A row the caller named is
refused at the exec when its name is gone or leads to another file than the one that was opened,
so a rule is never attached to a file the policy did not name at the time it was checked. A row
the caller named that a backend grants as its entries is refused when the backend cannot list it,
and at the exec when it has gone or is no longer a directory.

On Windows a grant is an entry that what is beneath the directory inherits, and it reaches that and
not what a junction beneath the directory leads to. That is shown for a junction made from outside a
stage. The backend makes no reparse-point check when it writes an entry, and whether a grant crosses
a junction a stage made inside a directory it was granted for writing has not been observed on
Windows: in run 42972425516 (job 130363809021) no stage could make one, by `mklink /J` or by a
program calling `FSCTL_SET_REPARSE_POINT`. It is listed with the known costs.

**Why.** A policy is the list a caller decided a program may reach, so a process running under
fewer of those paths than the policy names is the silent degradation [SANDBOX-1](#SANDBOX-1)
forbids, reached one grant at a time: the process runs, the record says the policy was applied,
and the program is refused a path somebody granted it. Which paths a backend can name is a
platform difference a caller can work with, and a grant that vanished without being reported is
not. A name gone from a directory since it was listed is no longer among the entries the caller
meant, so there is no grant to lose, and refusing the stage over it would make starting a program
depend on whatever else was changing that directory.

`verified-by: bravebot_sandbox::linux::a_path_that_cannot_be_opened_is_refused_rather_than_dropped`
`verified-by: bravebot_sandbox::linux::a_ruleset_is_not_built_with_a_path_missing_from_it`
`verified-by: bravebot_sandbox::linux::a_listed_row_whose_name_went_away_is_left_out_and_the_ones_still_there_are_granted`
`verified-by: bravebot_sandbox::linux::a_row_the_caller_named_is_refused_when_it_went_away_in_a_policy_that_lists`
`verified-by: bravebot_sandbox::linux::a_listed_row_that_is_there_and_cannot_be_opened_is_refused`
`verified-by: bravebot_sandbox::linux::a_named_row_gone_between_the_check_and_the_exec_refuses_the_exec`
`verified-by: bravebot_sandbox::linux::an_entry_listed_before_the_exec_and_gone_by_it_is_left_out`
`verified-by: bravebot_sandbox::linux::an_entry_listed_before_the_exec_that_cannot_be_opened_by_it_refuses_the_exec`
`verified-by: bravebot_sandbox::linux::a_named_row_above_a_refusal_that_cannot_be_listed_is_refused`
`verified-by: bravebot_sandbox::linux::a_named_row_replaced_by_another_directory_between_the_check_and_the_exec_refuses_the_exec`
`verified-by: bravebot_sandbox::policy::a_named_row_above_a_refusal_that_cannot_be_listed_is_an_error_naming_it`
`verified-by: bravebot_sandbox::policy::an_enumeration_tells_the_rows_the_caller_named_from_the_entries_it_listed`
`verified-by: bravebot_agent::confine::the_rows_a_bypass_stage_holds_without_a_request_are_marked_as_listed`
`verified-by: bravebot_sandbox::macos::a_path_that_is_not_there_yet_is_granted_as_named`
`verified-by: bravebot_sandbox::windows::a_path_that_is_not_on_disk_is_named_rather_than_left_out`
`verified-by: bravebot_sandbox::windows::a_policy_whose_paths_are_all_there_names_none`
`verified-by: bravebot_sandbox::windows::each_path_is_granted_what_the_list_it_was_named_in_asks_for`
`verified-by: bravebot_sandbox::windows::a_path_granted_for_reading_is_not_granted_writing_or_deleting`
`verified-by: bravebot_sandbox::windows::a_path_named_for_reading_and_for_writing_is_granted_once_for_writing`
`verified-by: bravebot_sandbox::windows::a_grant_reaches_what_is_under_the_directory_it_is_written_on`
`verified-by: bravebot_sandbox::windows::a_program_is_refused_a_file_outside_its_grants_and_reads_one_inside`
`verified-by: bravebot_sandbox::windows::a_program_writes_inside_its_grants_and_not_outside`
`verified-by: bravebot_sandbox::windows::a_grant_does_not_reach_through_a_junction_to_where_it_leads`
`verified-by: bravebot_sandbox::windows::a_grant_does_not_reach_through_a_junction_a_stage_made`

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
`verified-by: bravebot_sandbox::windows::a_path_granted_for_writing_can_be_written_and_moved_within`

<a id="SANDBOX-8"></a>
### SANDBOX-8: the environment a confined process receives is the caller's

A confined process starts with the environment the calling process holds, with none at all, or
with the variables the caller names and no others, and the caller says which as the process is
started. Confinement applies that answer itself, so a caller asking for nothing is handed nothing
whichever backend confines the process, a caller asking for its own is handed all of it, and a
caller naming variables is handed those with the values it gave. The debug form of what a caller
hands over names each variable and never shows its value, so a log line written from it publishes no
credential. A backend that reaches the program through a second program
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

What this process was itself started with is not a stage's to read either. On Linux the terminal
entry point marks itself non-dumpable before it reads its command line, so a program a stage starts,
which is a process of the same user, is refused `/proc/<pid>/environ` of this one and finds no
gateway or signing key there that a person exported. The mark is cleared by `execve`, so it does
not touch the programs started. Where the kernel refuses to set it, the refusal is said on standard
error and the run goes on. On macOS every Seatbelt profile refuses another process's exec-time
arguments and environment through `sysctl(KERN_PROCARGS2)`. It allows only named machine-information
sysctls and denies `process-info*`, with a `process-info-pidinfo` exception for the process itself
because system tools need that query during startup. Both restrictions are required: either a broad
`sysctl-read` grant or the default process-info access permits the environment read. CPU count,
page size, the physical memory size (`hw.memsize`, which `sort` reads), the fields of `uname` and the longest argument list a program may be given
(`kern.argmax`, which `xargs`, `find -exec {} +` and `getconf ARG_MAX` read) remain readable for
ordinary tools ([SANDBOX-21](#SANDBOX-21)). The argument-list limit is one system-wide number and
holds nothing about any process. This protects against confined stages, including a child given an
empty environment; it does not stop an unconfined same-user process reading the exec-time copy,
erase that copy, or withhold a variable the caller deliberately hands to a child. Windows is not
covered.

On macOS the same profile makes the programs that list processes fail, under the base and under
the run base ([SANDBOX-12](#SANDBOX-12)), which a `run` stage gets and which reads the whole
machine. `pgrep`, `pkill`, `killall`, `lsof` and `ps` each fail whether or not the program they
look for is running. The causes differ: `pgrep` and `pkill` look up the `com.apple.sysmond`
service, which the profile does not name, and exit 3; `killall` reads the process table through a
sysctl the profile does not name; `lsof` needs the `process-info*` refusal lifted; and `ps` is not
started, because `sandbox-exec` cannot start a setuid platform program under any profile. The
refusal above is unchanged. After a failed step whose program is one of the five, the planner is
told that the sandbox refuses listing processes, that the failure is not an answer about the
program, and that a local service is found by connecting to its port, such as
`curl http://127.0.0.1:9222/json/version` for a browser started with
`--remote-debugging-port=9222` ([SANDBOX-19](#SANDBOX-19)). The sentence does not attribute the
failure to the `process-info*` refusal, which is the cause for `lsof` alone. It is chosen from the
file the step resolved to, and nothing the step printed reaches it.

The directory a process starts in is the caller's as well. A policy may name one, and otherwise the
process starts in this one's. Naming it grants nothing: a process started in a directory it may not
read is refused its first read of it, so a caller meaning the process to work there grants the
directory too.

`verified-by: bravebot_sandbox::exec_environment::a_confined_child_cannot_read_its_parents_exec_time_environment`
`verified-by: bravebot_sandbox::macos::process_inspection_is_refused_and_sysctls_are_named_machine_queries`
`verified-by: bravebot_sandbox::macos::a_program_that_asks_for_the_longest_argument_list_runs_under_the_base`
`verified-by: bravebot_sandbox::macos::process_listing_programs_fail_under_both_bases_whether_or_not_what_they_seek_runs`
`verified-by: bravebot_agent::turn::a_real_pgrep_refused_by_the_macos_profile_is_told_to_be_the_sandbox_and_to_probe_the_port`
`verified-by: bravebot_agent::usability::every_workflow_works_under_the_default`
`verified-by: bravebot_sandbox::linux::the_environment_a_confined_process_receives_is_the_callers`
`verified-by: bravebot_sandbox::macos::the_environment_a_confined_process_receives_is_the_callers`
`verified-by: bravebot_sandbox::linux::a_confined_process_given_an_empty_environment_receives_none_of_this_processes_variables`
`verified-by: bravebot_sandbox::macos::a_confined_process_given_an_empty_environment_receives_none_of_this_processes_variables`
`verified-by: bravebot_sandbox::macos::a_variable_stripped_from_the_wrapper_still_reaches_the_confined_process`
`verified-by: bravebot_sandbox::macos::a_variable_the_platform_strips_from_the_wrapper_is_carried_to_the_program_as_an_argument`
`verified-by: bravebot_sandbox::macos::a_caller_holding_nothing_the_platform_strips_reaches_its_program_directly`
`verified-by: bravebot_sandbox::macos::a_confined_process_asked_to_receive_no_variables_is_handed_none_as_an_argument`
`verified-by: bravebot_sandbox::macos::a_program_path_the_wrapper_would_read_as_a_variable_is_refused`
`verified-by: bravebot_sandbox::macos::a_caller_naming_its_variables_has_only_the_named_loader_variables_carried_as_arguments`
`verified-by: bravebot_sandbox::process::a_process_handed_named_variables_receives_those_and_no_others`
`verified-by: bravebot_sandbox::process::a_variables_value_is_not_in_its_debug_form`
`verified-by: bravebot_sandbox::windows::named_variables_are_written_as_a_sorted_block_of_those_alone`
`verified-by: bravebot_sandbox::windows::an_empty_environment_is_an_empty_block_and_an_inherited_one_is_none`
`verified-by: bravebot_sandbox::windows::a_variable_the_platform_would_misread_is_refused`
`verified-by: bravebot_sandbox::windows::a_stage_started_with_only_the_system_root_and_one_variable_sees_that_variable`
`verified-by: bravebot_sandbox::macos::a_confined_process_starts_in_the_directory_its_policy_names`
`verified-by: bravebot_sandbox::linux::a_confined_process_starts_in_the_directory_its_policy_names`
`verified-by: bravebot_cli::running::a_program_of_the_same_user_cannot_read_the_environment_bravebot_was_started_with`

<a id="SANDBOX-9"></a>
### SANDBOX-9: a path that is not on disk is left out before the policy is built, and named

A caller assembling a policy out of paths whose existence is not its own to decide resolves it
against the backend before handing it over. Where the backend grants a path that does not exist
([SANDBOX-5](#SANDBOX-5)), the policy is what was wanted and nothing is left out. Where it does
not, a wanted path that is not on disk is left out and reported back, each such path once, and
every path that is there is kept in the list it was named in. Resolution adds no path, moves none
between the two lists, and carries the network and subprocess grants as they were, so what it
produces is a subset of what was wanted.

Neither of the other two answers to an absent path is taken here. Naming the directory holding it
grants over every other file in that directory, and the path this would fire on first is
`~/.ssh/known_hosts`, whose directory holds the private key no scope reaches. Creating one is a
write, and this decides what a policy names rather than what is on disk, so a row a caller means a
program to create is created before this runs, by the caller that said what it is
([SANDBOX-11](#SANDBOX-11)). What is still absent when this runs is left out.

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
`verified-by: bravebot_agent::servers::a_server_started_without_paths_its_backend_cannot_name_has_a_note_naming_each_one`
`verified-by: bravebot_agent::servers::a_launch_under_a_backend_that_cannot_name_an_absent_path_leaves_the_note`

<a id="SANDBOX-10"></a>
### SANDBOX-10: a session reports the confinement this platform offers, not one it is under

The opening screen and `/status` name the level this platform can enforce over a process running
code we did not write. Neither reports the session as running inside it, and `/status` says beside
the level what the session confines: the MCP servers it started, where it started a local one, and
nothing otherwise. Under a sandbox mode other than `workspace-write` ([SANDBOX-22](#SANDBOX-22)) both
lines say so beside the level: `strict` and `read-only` name themselves, and `off` says that programs run unconfined.

**Why.** The level is a fact about the machine, read before the session opens. What it bounds is a
process started to run somebody else's code, and a session that starts none of those is inside no
boundary at all: the agent's own reads and writes are held by the capability set and the label on a
value, and a program a person asks for runs with the access their own shell would give it. A level
drawn with nothing beside it is read as a guarantee over all of that, which is
[SANDBOX-5](#SANDBOX-5)'s overstated capability told to a person instead of to a caller, and the
person is the one with no backend to check it against.

**The line is a statement, not a count.** The one process a session starts for confinement to
bound is a local MCP server ([mcp-servers.md](mcp-servers.md)), and the line beneath it names each
one started. So the confinement line says which kind of process it covers and leaves the names to
that one, rather than restating a list. A remote server is not a process here and is not confined,
so a session that reached only remote ones still confines nothing. What it costs is a line that
whatever next gives a session such a process has to revisit.

`verified-by: bravebot_tui::status::the_confinement_is_reported_as_available_rather_than_in_force`
`verified-by: bravebot_tui::status::the_servers_a_session_started_are_named_and_what_is_confined_follows_them`
`verified-by: bravebot_tui::logo::the_mark_names_the_agent_its_confinement_and_its_tier`
`verified-by: bravebot_tui::logo::a_narrow_pane_still_reports_the_confinement_and_the_tier`
`verified-by: bravebot_tui::status::the_confinement_line_names_the_mode_unless_it_is_workspace_write`
`verified-by: bravebot_tui::logo::the_opening_screen_names_a_sandbox_mode_other_than_workspace_write`

<a id="SANDBOX-11"></a>
### SANDBOX-11: a write row says what is at the path it names, and one that is not there is created

A row granting a write says whether the path it names is a file, a directory, or neither. Before a
policy is resolved against a backend that cannot name a path which does not exist
([SANDBOX-9](#SANDBOX-9)), every write row saying which of the two it is and not on disk is created:
a file empty, with the directory holding it, and a directory empty, each reachable by the account
that owns it and by nobody else. A row saying neither is not created, and a row a listing produced is one of those
([SANDBOX-6](#SANDBOX-6)). A path already there is left
as it is, contents and all. A row that could not be created is absent still, so resolution leaves it
out and names it. Nothing is created where the backend names an absent path, and nothing where it
confines nothing at all, since a process that will be refused reaches no path made for it.

**Why.** Leaving an absent write row out costs a program the write the row granted it, and the rows
this fires on are the ones a program would have created for itself: a toolchain cache on a machine
that has not run that toolchain, and `~/.ssh/known_hosts` on a fresh account, each of which is a
build or a push that fails rather than a program that does not start. The other answer, naming the
directory holding the path, grants over every other file there, which for `known_hosts` is the
private key. Creating it is open only to a caller that knows which of the two the row means: the
guess is wrong half the time, and a program that finds a directory where it expects a file fails on
a path it was granted as surely as on one that is absent. A read row says nothing and needs to say
nothing, since there is nothing at an absent path to read. Creating on a backend that grants an
absent path would write on the platform where no write was necessary, which is the cost a capability
reported per backend exists to avoid ([SANDBOX-5](#SANDBOX-5)). What is created is narrower than
what the program would have made for itself, because the account gains a path nobody asked it to
have: the directory this fires on first holds a private key, a umask most accounts leave at its
default would make it listable by everybody, and nothing tightens a directory that already exists
afterwards.

Half built. A write row says which of the three it is, and every row a run builds says neither. The
creating is written and tested, and nothing calls it before a policy is resolved, so no run creates
anything. A call put in today would create nothing either: the session temporary directory, the null
device, a server's own directory and the directory a declaration named are each there by the time
the policy is assembled, and none is a row somebody meant a program to create. The rows this fires
on, a toolchain cache and `known_hosts`, would come from a per-program write list, and that list is
not built.

`verified-by: bravebot_sandbox::policy::a_row_naming_a_directory_that_is_not_there_is_created_as_a_directory`
`verified-by: bravebot_sandbox::policy::a_row_naming_a_file_that_is_not_there_is_created_as_a_file`
`verified-by: bravebot_sandbox::policy::a_file_row_is_created_with_the_directory_holding_it`
`verified-by: bravebot_sandbox::policy::a_row_that_does_not_say_what_it_names_is_not_created`
`verified-by: bravebot_sandbox::policy::a_row_a_listing_produced_is_not_created`
`verified-by: bravebot_sandbox::policy::a_backend_that_grants_an_absent_path_has_nothing_created_for_it`
`verified-by: bravebot_sandbox::policy::a_backend_that_confines_nothing_has_nothing_created_for_it`
`verified-by: bravebot_sandbox::policy::what_is_created_is_reachable_by_its_owner_and_nobody_else`
`verified-by: bravebot_sandbox::policy::what_is_created_is_reachable_by_its_owner_and_nobody_else_on_windows`
`verified-by: bravebot_sandbox::windows::the_access_list_of_a_created_row_names_only_its_owner`
`verified-by: bravebot_sandbox::policy::a_row_that_is_already_there_keeps_what_is_in_it`
`verified-by: bravebot_sandbox::policy::a_row_created_first_is_in_the_policy_the_backend_is_handed`
`verified-by: bravebot_sandbox::policy::a_row_that_could_not_be_created_is_left_out_and_named`

<a id="SANDBOX-12"></a>
### SANDBOX-12: the base every program starts from is fixed, and names no credential

A stage of `run` on Linux and macOS starts from the **run base**: it reads the whole machine
except a fixed table of credential locations, and the only paths it writes are the temporary
directory the session resolved as it opened, where the platform has one as a file, the null
device, and on macOS the `mds` directory in the account's per-user cache directory (below). The
table is code, the same for every stage, and no value, argument vector, printed output
or configuration file adds to it or removes from it. These are its rows, each refused with
everything beneath it:

| Where | Refused |
| --- | --- |
| every platform | the program's own state directory `~/.bravebot`, where the gateway keys, the premium key, the ChatGPT and xAI sign-ins and the server list are, and where a `leo-premium.json` that an earlier version wrote and this one does not is refused with the rest, so a file left from an earlier version stays protected (a directory `BRAVEBOT_HOME` moves it to is refused in the same way, with the same `sessions` exception, unless it is the home directory, a place above it or a filesystem root, which would refuse the home itself); the browser relay's default directory `~/.bravebot-browser`, where the secret that drives the browser is (a relay told to use another directory is not covered); `~/.ssh`, `~/.aws`, `~/.kube`, `~/.docker`, `~/.azure`, `~/.config/gcloud`, `~/.gnupg`, sops' age keys `~/.config/sops/age` and `pass`'s `~/.password-store`; the histories `~/.bash_history`, `~/.zsh_history`, `~/.zsh_sessions`, `~/.bash_sessions`, `~/.local/share/fish/fish_history`, `~/.psql_history`, `~/.mysql_history`, `~/.node_repl_history`, `~/.python_history`, `~/.rediscli_history`, `~/.sqlite_history`, `~/.dbshell` and `~/.mongodb/mongosh/mongosh_repl_history`; another agent's login and transcripts, `~/.claude`, `~/.claude.json`, `~/.codex`, `~/.gemini` and `~/.local/share/opencode` (a directory `CLAUDE_CONFIG_DIR` or the like moves is not covered); `~/.config/github-copilot`, `~/.config/openconnect`, `~/.gitlab-runner`, `~/.pgadmin` and `~/Postman` |
| Windows, also | `~/AppData/Roaming/gcloud` and `~/AppData/Roaming/gnupg`, where `gcloud` and `gpg` keep what the unix spellings hold; the browser profiles `~/AppData/Local/BraveSoftware`, `~/AppData/Local/Google/Chrome`, `~/AppData/Local/Chromium`, `~/AppData/Local/Microsoft/Edge` and `~/AppData/Roaming/Mozilla`; and the platform's own stores, Credential Manager's files `~/AppData/Local/Microsoft/Credentials` and `~/AppData/Roaming/Microsoft/Credentials`, the keys the data protection API encrypts with `~/AppData/Roaming/Microsoft/Protect`, the account's private keys `~/AppData/Roaming/Microsoft/Crypto`, and the vault `~/AppData/Local/Microsoft/Vault` and `~/AppData/Roaming/Microsoft/Vault` (an application data directory moved out of the home is not covered) |
| macOS, also | `~/Library/Keychains` and `/Library/Keychains`; the browser profiles under `~/Library/Application Support` (`BraveSoftware`, `Google/Chrome`, `Google/Chrome Beta`, `Google/Chrome Canary`, `Google/Chrome Dev`, `Chromium`, `Firefox`, `Microsoft Edge`, `Microsoft Edge Beta`, `Microsoft Edge Dev`, `Microsoft Edge Canary`, `Arc`, `Vivaldi`), `~/Library/Cookies` and `~/Library/Safari`; `~/Library/Messages`, `~/Library/Mail` and `~/Library/Thunderbird`; 1Password's group container `~/Library/Group Containers/2BUA8C4S2C.com.1password`; under `~/Library/Application Support` also `Slack`, `discord`, `Signal`, `Code`, `Code - Insiders`, `VSCodium`, `Cursor`, `Claude`, `Insomnia`, `Bruno`, `Postman` and `sops/age`; and `~/Library/DBeaverData` |
| Linux, also | `~/.local/share/keyrings`, `~/.local/share/kwalletd` and the X display's cookie `~/.Xauthority` (a cookie `XAUTHORITY` names elsewhere is not covered); the browser profiles `~/.config/BraveSoftware`, `~/.config/google-chrome`, `~/.config/google-chrome-beta`, `~/.config/google-chrome-unstable`, `~/.config/chromium`, `~/.config/microsoft-edge`, `~/.config/microsoft-edge-beta`, `~/.config/microsoft-edge-dev`, `~/.config/vivaldi`, `~/.mozilla`, `~/snap/firefox/common/.mozilla`, `~/snap/chromium` and `~/snap/brave`, and every Flatpak application's data, `~/.var/app`; `~/.thunderbird`; under `~/.config` also `Slack`, `discord`, `Signal`, `1Password`, `Code`, `Code - Insiders`, `VSCodium`, `Cursor`, `JetBrains`, `Insomnia`, `Bruno` and `Postman`; and `~/.local/share/DBeaverData` |

Beside the program's own directories, the cloud and container tools' directories and the private
keys, a row is another application's data (a browser's, a mail or chat client's, a password
manager's, an editor's, a database or API client's, a VPN client's, a CI runner's or another
agent's), what a person typed at a shell or a client, or a desktop session's secret store or
display cookie. A build, a test or a deploy does not read one, and a shell or client a stage starts does not read its
history, since a stage's standard input is never a terminal; the programs that do read a row are
named under what a row costs, below. A browser's channels keep separate directories, so a row for
one channel holds nothing of another's. 1Password's ssh agent keeps its socket in the group
container, and a socket is reached through a write row and never through a read
([SANDBOX-3](#SANDBOX-3)), so a stage that is lent that socket still reaches it. JetBrains keeps
its settings on macOS in `~/Library/Application Support/JetBrains`, beside the scripts its Toolbox
puts on the search path, so that directory is not a row. Three
kinds of file in `~/.ssh` hold no secret and are read: `config`, `known_hosts` and the default
public keys (`id_rsa.pub`, `id_dsa.pub`, `id_ecdsa.pub`, `id_ecdsa_sk.pub`, `id_ed25519.pub`,
`id_ed25519_sk.pub`). One subtree of `~/.bravebot` is read: the session records, `~/.bravebot/sessions`,
so that a script can read past sessions (`session-review` is one). It is lifted for reading only, and
the rest of the directory stays refused with everything beneath it, including a file a later
version adds, since the refusal covers the whole directory and no list of files has to be kept in
step with what the program writes. Nothing under `~/.bravebot` is written by a stage. Every refusal
of `~/.bravebot` in this clause, and in the clauses that cite it, is of the state directory in force:
`~/.bravebot`, and the directory `BRAVEBOT_HOME` puts elsewhere ([STATE-2](state-directory.md#STATE-2)),
the home directory, a directory above it and a filesystem root excepted, since a refusal of one of
those would refuse the account's own files. A link is judged where it leads, for the moved directory
as for the default. A transcript
can hold what a person pasted, so a stage that reads the records is one the plan has to be willing
to let read them; the network is where they could leave, and the plan decides that
([SANDBOX-3](#SANDBOX-3)). A public key at another name is read by a stage that carries the remote
scope when `config` names it, and the one `user.signingkey` names by a stage that signs
([SANDBOX-16](#SANDBOX-16)). On macOS the file `~/Library/Keychains/login.keychain-db` is also read, with
the directory around it and every other file in it still refused, because `gh` and git's
`osxkeychain` helper keep their tokens in that keychain and open its database file themselves.
Opening a keychain also makes the Security framework write its framework database, which is the
`mds` directory under the per-user cache directory that `confstr` names for the account
(`_CS_DARWIN_USER_CACHE_DIR`, resolved by the host with its links followed and handed in by the
caller), and that directory is written by a stage that reads the machine and by no other. Without
the read row, `gh` reports a failed login and `osxkeychain` fails with `-67674`; with only the read
row they still do. The files a program reads by name to do what it was started for are not in the
table, and neither are the files a shell reads as it starts or the person's own files: the network
is where they could leave, and the plan decides that ([SANDBOX-3](#SANDBOX-3)). Of the stores
[credential-protection.md](credential-protection.md) lists as already on the machine, a stage in
the mode `workspace-write` reads these on purpose:

| What reads it | Read |
| --- | --- |
| `ssh` and the keychain lookup, through the lifts above | `~/.ssh/config`, `~/Library/Keychains/login.keychain-db` |
| a script that reads past sessions | `~/.bravebot/sessions` |
| a shell, as it starts | `~/.bashrc`, `~/.zshrc`, `~/.zshenv`, `~/.profile`, `~/.bash_profile` |
| `git`, `gh`, `glab` and `hub` | `~/.gitconfig`, `~/.git-credentials`, `~/.netrc`, `~/.config/gh`, `~/.config/glab-cli`, `~/.config/hub` |
| a package manager | `~/.npmrc`, `~/.yarnrc.yml`, `~/.pypirc`, `~/.config/pip/pip.conf`, `~/.m2/settings.xml`, `~/.gradle/gradle.properties`, `~/.cargo/credentials.toml`, `~/.nuget/NuGet/NuGet.Config`, `~/.composer/auth.json`, `~/.gem/credentials`, `~/.cache/huggingface/token` |
| a client no scope covers, for a deploy, a cloud, a cluster or a secret | `~/.config/vercel`, `~/.config/netlify`, `~/.fly`, `~/.config/doctl`, `~/.supabase`, `~/.config/rclone`, `~/.config/containers`, `~/.config/helm`, `~/.terraformrc`, `~/.terraform.d`, `~/.pulumi`, `~/.vault-token`, `~/.config/op` |
| a database client | `~/.pgpass`, `~/.my.cnf`, `~/.mylogin.cnf` |
| a build that signs | `~/.android/debug.keystore`, `~/Library/MobileDevice/Provisioning Profiles` |
| a program handed a file of the person's own | `~/Downloads`, `~/Desktop` |

A path is refused by where it leads, so a link from a readable directory
to a refused one reaches nothing. On macOS the profile states the refusals after the grants they
narrow and the lifts after the refusals, since the last matching rule wins. On Linux, where a
directory is granted with everything beneath it and no subdirectory can be held back, the
directories above each refused location are listed when the stage starts and granted entry by
entry, with the refused one left out, a link in a listing is not followed, and what is compared is
the names in a directory against the table and where a link leads, never what a file holds. A
write row above a refused location is listed the same way, since a Landlock write grant carries
the right to read beneath it, so a session opened on the home directory cannot make an entry
directly in it. A refused location that is not on disk is still left out of the listing, so a directory made there
during the run is outside every grant. A refusal with no grant above it is a policy that holds back
nothing it was ever given, and is refused as one that confines nothing ([SANDBOX-2](#SANDBOX-2)). The
Windows backend has no way to subtract from a container and refuses a policy that carries a
refusal.

The base below is the **keyed base**, which an MCP server starts from on every platform and a stage
of `run` starts from on Windows, where a container reaches only what it is granted. The rows it
reaches before its own plan is read are the same for every program
and are decided in code: what a dynamic executable needs in order to start, the temporary
directory the session resolved as it opened, on macOS the developer directory resolved with it,
and the git configuration a stage reads for an identity. Nothing a program prints, no value a model
supplied, no argument vector and no configuration file adds a row, and the developer directory is
granted only where the platform installs one. The home directory is in no row, no directory a
credential sits in is in one, and the only paths granted for writing are the temporary directory
and, where the platform has one as a file, the null device. Egress and children are left where they were, since what the base bounds is
the filesystem; a session may take egress from a stage afterwards ([SANDBOX-20](#SANDBOX-20)). A platform whose prelude is not written down has no base, and so nothing to
assemble a profile from. The Windows prelude is empty: an AppContainer reads `C:\Windows` and the
Program Files directories without an entry of its own, and a row there would be an entry only an
account holding the right to change those directories could write. A Windows session with a
directory of its own is granted a `temp` directory inside it in place of the temporary-directory
row, and a stage is handed `TEMP` and `TMP` naming it ([trust-map.md](trust-map.md#TRUST-14)). On
the CI runner the stage's `TEMP` showed the container's own `AC\Temp` instead (Known costs).

**Why.** A profile denies everything and then names what may be reached, so something has to
carry what every program needs before any plan is read: a dynamic executable without its loader
does not start, and a program that cannot open a temporary file fails outright. Those rows are
shown by no prompt, because a row shown on every run teaches a person to approve without reading.
What an invisible list has to be is small, fixed, and reviewed as code, since a row added to it
reaches every program at once and a row a value could add is reach the model chose. What it buys
is what it leaves out: the key a push signs with and the token a publish uses sit under a home
directory, so a base naming that directory, or naming the configuration directory the second git
spelling sits in, hands both to a program whose plan named neither, which is the whole of what
confining a program was for.

The login keychain file is the one credential location the base reads, and what that costs is
that a stage can read a database holding every password the account keeps there. Its items are
encrypted under the login password, and the file can leave the machine wherever egress is open, so
it is open to an offline guess of that password. The system service that holds the keychain was
already reachable from every stage (the known costs below), and because the base reads the login
keychain file, a lookup through it can reach that database. Which items such a lookup returns, and which raise the
system's own prompt, is the platform's and is not tested here. The `mds` write lets a stage change
the framework's cache files, which are rebuilt on demand. The read row names the file and not the
directory so that `aws-vault.keychain-db` and every other keychain file stay refused.

A row costs the program that does read the location: `claude`, `codex`, `gemini` or `opencode`
started by a stage, `sops` decrypting with an age key kept where it looks by default, `pass`, a
graphical program on an X display that checks the cookie, and a stage of a session opened inside
one of the rows ([SANDBOX-18](#SANDBOX-18)). A person who wants one of those programs to work names
the location in `allowRead`, or in `allowWrite` where the program also writes there, as the agents
do ([SANDBOX-25](#SANDBOX-25)). The table names its locations one at a time, so a browser channel,
an application or a history it does not name is read. The stores read on purpose cost the reverse:
a stage that reads one can send what it holds wherever the network is open, which a session closes
with [SANDBOX-20](#SANDBOX-20).

`verified-by: bravebot_sandbox::base::the_base_reaches_no_credential`
`verified-by: bravebot_sandbox::base::the_only_rows_under_a_home_directory_are_the_git_configuration`
`verified-by: bravebot_sandbox::base::the_git_configuration_is_read_and_never_written`
`verified-by: bravebot_sandbox::base::a_machine_with_no_home_directory_gets_the_rest_of_the_base`
`verified-by: bravebot_sandbox::base::the_temporary_directory_is_the_one_the_caller_resolved`
`verified-by: bravebot_sandbox::base::only_the_temporary_directory_and_the_null_device_are_written`
`verified-by: bravebot_sandbox::base::the_base_asks_for_nothing_to_be_created`
`verified-by: bravebot_sandbox::base::the_base_leaves_egress_and_children_to_the_plan`
`verified-by: bravebot_sandbox::base::the_base_names_no_filesystem_root`
`verified-by: bravebot_sandbox::base::each_platform_starts_a_program_out_of_its_own_directories`
`verified-by: bravebot_sandbox::base::a_macos_base_names_what_its_tls_library_and_developer_tools_start_from`
`verified-by: bravebot_sandbox::base::a_developer_directory_anywhere_else_is_in_no_row`
`verified-by: bravebot_sandbox::base::a_prelude_names_the_machines_directories_and_none_of_a_persons`
`verified-by: bravebot_sandbox::base::a_platform_with_no_prelude_written_down_has_no_base`
`verified-by: bravebot_sandbox::base::a_windows_base_names_no_system_directory`
`verified-by: bravebot_sandbox::linux::a_program_starts_under_the_base_this_machine_resolved`
`verified-by: bravebot_sandbox::linux::a_program_under_the_base_can_name_the_account_it_runs_as`
`verified-by: bravebot_sandbox::linux::a_program_under_the_base_reads_the_machine_and_not_a_private_key`
`verified-by: bravebot_sandbox::windows::cmd_and_a_compiler_start_under_the_empty_base`
`verified-by: bravebot_sandbox::base::the_run_base_reads_the_machine_and_refuses_each_credential_location`
`verified-by: bravebot_sandbox::base::the_credential_locations_are_the_rows_the_run_base_refuses`
`verified-by: bravebot_sandbox::base::the_run_base_lifts_the_ssh_files_that_hold_no_secret_and_only_those`
`verified-by: bravebot_sandbox::base::the_run_base_reads_the_session_records_and_no_other_file_in_the_state_directory`
`verified-by: bravebot_sandbox::base::the_state_directory_entries_a_stage_reads_are_the_session_records_alone`
`verified-by: bravebot_sandbox::base::the_run_base_leaves_the_token_files_readable`
`verified-by: bravebot_sandbox::base::every_store_the_catalog_names_is_refused_or_read_on_purpose`
`verified-by: bravebot_sandbox::base::the_run_base_writes_only_the_temporary_directory_and_the_null_device`
`verified-by: bravebot_sandbox::base::the_run_base_on_macos_reads_the_login_keychain_file_and_no_other_keychain_file`
`verified-by: bravebot_sandbox::base::the_security_cache_row_is_the_one_directory_under_the_user_cache`
`verified-by: bravebot_agent::confine::the_security_cache_is_written_by_a_macos_stage_that_reads_the_machine_only`
`verified-by: bravebot_sandbox::macos::a_stage_under_the_run_base_writes_the_security_cache_and_no_other_cache`
`verified-by: bravebot_sandbox::macos::the_user_cache_directory_is_an_existing_path_with_its_links_followed`
`verified-by: bravebot_sandbox::macos::a_refusal_of_a_link_is_written_where_the_link_leads_as_well`
`verified-by: bravebot_sandbox::rules::a_request_reaching_a_linked_credential_directory_is_refused_where_it_leads`
`verified-by: bravebot_sandbox::base::the_run_base_on_windows_is_the_keyed_base`
`verified-by: bravebot_sandbox::base::the_windows_credential_locations_name_the_browser_profiles_and_the_platforms_stores`
`verified-by: bravebot_sandbox::base::the_run_base_without_a_home_refuses_only_the_machine_wide_keychains`
`verified-by: bravebot_sandbox::macos::a_stage_under_the_run_base_is_refused_each_credential_location_and_reads_the_rest`
`verified-by: bravebot_sandbox::linux::a_stage_under_the_run_base_is_refused_each_credential_location_and_reads_the_rest`
`verified-by: bravebot_agent::confine::a_stage_reads_the_machine_and_is_refused_the_credential_locations`
`verified-by: bravebot_agent::home::the_state_directory_is_the_one_the_sandbox_refuses`
`verified-by: bravebot_sandbox::base::the_run_base_refuses_a_state_directory_that_is_moved_as_it_refuses_the_default`
`verified-by: bravebot_sandbox::base::a_moved_state_directory_that_would_refuse_more_than_itself_adds_no_row`
`verified-by: bravebot_sandbox::base::a_moved_state_directory_that_leads_to_the_default_adds_no_row`
`verified-by: bravebot_sandbox::base::a_moved_state_directory_that_is_a_link_to_the_home_or_above_adds_no_row`
`verified-by: bravebot_sandbox::base::the_windows_table_holds_a_moved_state_directory_and_its_base_is_unchanged`
`verified-by: bravebot_sandbox::base::the_run_base_without_a_home_refuses_a_state_directory_that_is_moved`
`verified-by: bravebot_sandbox::rules::a_request_for_the_state_directory_that_is_moved_is_refused_as_the_default_is`
`verified-by: bravebot_sandbox::rules::a_state_directory_that_is_a_link_is_refused_where_it_leads`
`verified-by: bravebot_sandbox::rules::a_state_directory_that_is_a_link_reads_its_session_records_where_it_leads`
`verified-by: bravebot_sandbox::rules::a_directory_added_inside_or_above_the_state_directory_that_is_moved_is_refused`
`verified-by: bravebot_sandbox::rules::a_windows_row_over_a_directory_holding_the_state_directory_that_is_moved_is_refused`
`verified-by: bravebot_sandbox::rules::the_rows_written_without_a_request_leave_out_the_state_directory_that_is_moved`
`verified-by: bravebot_sandbox::scope::a_name_inside_the_state_directory_that_is_moved_is_refused_as_the_default_is`
`verified-by: bravebot_sandbox::scope::a_name_inside_a_state_directory_that_is_moved_through_a_link_is_refused`
`verified-by: bravebot_agent::confine::a_session_inside_a_state_directory_that_is_moved_is_refused_as_the_default_is`
`verified-by: bravebot_agent::confine::a_scratch_directory_inside_a_state_directory_that_is_moved_gets_no_row`
`verified-by: bravebot_agent::confine::a_stage_cannot_read_a_state_directory_that_is_moved_out_of_the_default`
`verified-by: bravebot_agent::confine::a_stage_cannot_read_a_state_directory_that_lies_inside_its_session_directory`
`verified-by: bravebot_agent::tools::a_turns_confinement_refuses_the_state_directory_the_turn_runs_under`
`verified-by: bravebot_agent::tools::a_turns_confinement_refuses_a_list_entry_on_the_state_directory_it_runs_under`
`verified-by: bravebot_agent::servers::a_servers_confinement_does_not_read_a_state_directory_that_is_moved`
`verified-by: bravebot_sandbox::policy::a_refusal_is_recorded_and_changes_nothing_else`
`verified-by: bravebot_sandbox::policy::a_policy_made_nameable_keeps_its_refusals`
`verified-by: bravebot_sandbox::policy::a_refusal_nothing_grants_is_not_meaningful`
`verified-by: bravebot_sandbox::policy::without_a_refusal_the_enumeration_is_the_read_rows`
`verified-by: bravebot_sandbox::policy::the_entries_above_a_refusal_are_each_a_row_and_the_refusal_is_not`
`verified-by: bravebot_sandbox::policy::a_row_beneath_a_refusal_is_kept_as_a_lift`
`verified-by: bravebot_sandbox::policy::a_link_is_not_granted`
`verified-by: bravebot_sandbox::policy::a_refusal_that_is_a_link_is_refused_where_it_leads_as_well`
`verified-by: bravebot_sandbox::policy::a_refusal_that_is_not_on_disk_still_shapes_the_rows`
`verified-by: bravebot_sandbox::policy::a_write_row_above_a_refusal_is_spread_around_it`
`verified-by: bravebot_sandbox::policy::without_a_refusal_the_write_enumeration_is_the_write_rows`
`verified-by: bravebot_sandbox::linux::a_write_row_over_the_home_does_not_hand_back_a_credential_location`
`verified-by: bravebot_sandbox::windows::a_policy_refusing_a_read_is_refused_rather_than_applied`
`verified-by: bravebot_sandbox::linux::a_stage_under_the_run_base_is_refused_each_credential_location_and_reads_the_rest`
`verified-by: bravebot_sandbox::linux::a_stage_under_the_run_base_runs_programs_and_writes_only_where_it_was_given`
`verified-by: bravebot_sandbox::macos::a_stage_under_the_run_base_is_refused_each_credential_location_and_reads_the_rest`
`verified-by: bravebot_sandbox::macos::the_profile_states_a_refusal_after_the_grant_it_narrows_and_a_lift_after_the_refusal`
`verified-by: bravebot_agent::confine::a_stage_reads_the_machine_and_is_refused_the_credential_locations`
`verified-by: bravebot_agent::confine::a_stage_writes_only_the_session_the_temporary_directory_and_the_caches`
`verified-by: bravebot_agent::confine::a_windows_stage_is_given_a_temporary_directory_of_the_sessions_and_not_the_accounts`
`verified-by: bravebot_agent::confine::a_stage_is_given_the_sessions_temp_and_cannot_write_the_accounts`
`verified-by: bravebot_sandbox::macos::a_program_linked_against_the_platforms_tls_library_starts_under_the_base`
`verified-by: bravebot_sandbox::macos::a_developer_tool_the_platform_ships_as_a_shim_starts_under_the_base`

<a id="SANDBOX-13"></a>
### SANDBOX-13: a confined process can look at any path, and reads and lists only its grants

A stage of `run` on Linux and macOS has the machine for a grant and the credential table of
[SANDBOX-12](#SANDBOX-12) for a refusal, so what it is refused opening and listing is a place in
that table and, for writing, anything outside its write rows. The rest of this clause is about a
policy whose grants are rows, which is every other.

On Linux and macOS a look at a path is not bounded by a grant: whether something is there, what
kind of thing it is, its size, when it changed, and where a link points. Opening a file for what it
holds and listing a directory's entries are bounded, and outside the grants both are refused. The
one exception is the root directory on macOS: its entries can be listed, because the loader opens
`/` as the process starts and Seatbelt has no operation that separates opening a directory from
listing it. The names in `/` are the names of the system's top-level directories, and no file's
contents are readable through that row.

**Why.** Landlock bounds no look, so on Linux this is the kernel's and not a choice. On macOS a
profile that refused a look outside the grants refused the walk to them: node resolves its own
script through each directory above it, and a search of `PATH` stops at an entry it is refused
rather than told is missing, which a link outside the grants on the way to a granted directory is.
Under that profile no node program started, which is most of what a runner runs. A look gives the
shape of what is at a name a process had to know already; what it withholds is every byte a file
holds and every name a directory lists, which is where a credential is.

**What it costs.** A look reaches inside a refused location too, and a link's target is held in the
link, so where a link there points is readable: a Chromium browser's profile directory keeps
`SingletonLock`, `SingletonCookie` and `SingletonSocket` as links. Landlock has no right for a look,
so on Linux nothing refuses it. On macOS a rule naming `file-read-metadata` decides a look before a
`file-read*` rule does, wherever each stands in the profile, so the refusals do not narrow the look
whether they come before it or after it. A refusal that named `file-read-metadata` would narrow it,
and would also refuse a look at the refused directory itself, which `ls -la` of the home directory
fails on unless that one path is allowed again; a search of `PATH` would then stop with
`Operation not permitted` at a program that exists inside a refused location, such as Docker
Desktop's `~/.docker/bin`.

`verified-by: bravebot_sandbox::macos::a_confined_process_can_look_at_any_path_and_read_or_list_only_its_grants`

<a id="SANDBOX-14"></a>
### SANDBOX-14: on macOS a write row does not reach a `.git` or any other protected name beneath it

On macOS a policy that grants any write refuses a write to every path with a `.git` component, in
any case: creating, changing, renaming into or removing a file or directory named `.git` or inside
one, under a row the policy writes as under any other. That covers a repository's hooks and
configuration, a submodule's under `.git/modules`, a linked work tree's under `.git/worktrees`, and a
`.git` that is a file naming a gitdir.

The refusal also covers every other name [PERM-9](permissions.md#PERM-9) puts to the person for a
write: a `.bravebot` or `.husky` directory and what is inside it, and a `.gitmodules`, `.gitconfig`
or `.pre-commit-config.yaml` file. These are read beneath each write row and not in the row's own
path, so a row that is itself inside such a directory, such as a server's home under
`~/.bravebot/mcp-home`, is written as any other, and a protected name beneath it is refused. A name
matches in any letter case, with `\u{17f}` read as `s`, `\u{131}` as `i` and the Kelvin sign as `k`,
as `is_protected` reads it. A name spelled with a trailing dot or space, or with a `\` separator,
is another name on macOS and is granted. One list in `bravebot_sandbox` supplies the names, and a test
in `bravebot-agent` holds it equal to the list `is_protected` reads.

Every other write a row grants is granted.
A policy can lift the refusal, as the profile of a `run` stage whose program is git does so that
`git` works in the directories it was given ([SANDBOX-18](#SANDBOX-18)), and then a write row
reaches a `.git` and each other protected name as it does on Linux. Every other stage keeps the
refusal, and so does a stage whose program is a file named `git` under a directory the plan may write
to, the checkout holding a session directory among them ([SANDBOX-20](#SANDBOX-20)). On Linux Landlock grants a directory with everything beneath it and has no way to hold one
subdirectory back, so this clause does not hold there, which is
[mcp-servers.md](mcp-servers.md)'s known cost and a known cost of `run` ([SANDBOX-18](#SANDBOX-18)).
The Windows backend withholds nothing of the kind, and no server is started there
([SERVERS-10](mcp-servers.md#SERVERS-10)).

**Why.** git runs the commands a repository's configuration and hooks name, with nothing confining
them, the next time anybody runs git in it. A hook that `core.hooksPath` or `.pre-commit-config.yaml`
names, a `.husky` script and the project's own `.bravebot` settings are run or loaded with the
person's rights the same way. A confined process that may write one of these may therefore run code
outside its confinement. Seatbelt lets the last rule matching a path decide, so the refusal follows
the rows it narrows. The default macOS volume opens `.GIT` when git asks for `.git`, which is why
the case of the name does not matter.

**What it costs.** A gitdir that is not named `.git`, such as one `--separate-git-dir` made, is
written as any other directory, and so is a directory a process lays out as a repository itself, a
`HEAD` file beside `objects` and `refs` directories, which git started inside it opens. A stage
that has to write a `.husky` or `.bravebot` file under a row, such as an installer of hooks, is
refused, and the person makes that edit through a tool that asks.

A person's trust of a directory lets a plain line run there without asking only where every stage of it
is held back from a protected name by this clause, so on macOS and for a stage that is not git ([RUN-5](tools/run.md#RUN-5)).
The auto mode's classifier is kept from a line with a git stage for the same reason, on every platform
([AUTO-3](auto-mode.md#AUTO-3)).

`verified-by: bravebot_sandbox::macos::a_write_row_does_not_reach_a_git_directory_beneath_it`
`verified-by: bravebot_sandbox::macos::a_write_row_does_not_reach_the_other_protected_names_beneath_it`
`verified-by: bravebot_sandbox::macos::a_write_row_inside_a_protected_directory_is_writable_under_itself`
`verified-by: bravebot_sandbox::macos::every_name_in_the_shared_list_is_in_the_profile`
`verified-by: bravebot_agent::protected_names::the_sandbox_and_the_file_tools_hold_back_the_same_names`
`verified-by: bravebot_sandbox::macos::a_policy_allowing_git_directory_writes_reaches_a_git_directory`
`verified-by: bravebot_sandbox::macos::the_git_refusal_is_in_the_profile_only_for_a_policy_that_writes_and_has_not_lifted_it`
`verified-by: bravebot_agent::confine::only_a_step_whose_program_is_git_writes_a_git_directory`
`verified-by: bravebot_agent::confine::a_git_above_the_session_directory_in_its_checkout_writes_no_git_directory`
`verified-by: bravebot_agent::confine::only_git_writes_a_repositorys_hooks_and_configuration_on_macos`

<a id="SANDBOX-15"></a>
### SANDBOX-15: a toolchain's list is keyed on the file its program resolved to, and names a cache and never the token beside it

A stage whose program resolved to a file one of the toolchain lists below knows gets that
toolchain's rows, and a stage whose program no list knows gets none. The key is the name of the file
the plan resolved, after its links are followed, so rustup's `cargo`, npm's own scripts and a
versioned `python3.12` are each known, and a name that only resembles one, `pnpm`,
`python3-config` or `cargo-deny`, is not. An install and a configuration file are read and never
written. A cache is read and written, and is named as the directory or file it is rather than
through the directory holding it, so no list reaches the file a tool keeps its token in, the home
directory, `~/.config`, `~/.cache`, `~/Library` or `~/Library/Caches`. Each write row says what it
names, so a backend that cannot name an absent path has the cache created as the directory or file
the toolchain expects there. A list is added to the policy it is given and takes nothing from it.
A cache row is judged again where its name leads each time a stage starts: a name with a link
below the home directory is given at the place it leads to, and is given no row, with the stage
still started, where that place is, holds or lies inside a credential or persistence location or the
state directory in force ([SANDBOX-12](#SANDBOX-12)), or
where it is a file for a directory row or a directory for a file row ([SANDBOX-18](#SANDBOX-18)).
What decides a row is the table, the platform and where its name leads: the environment is not
read, so `CARGO_HOME`,
`GOCACHE` and `XDG_CACHE_HOME` move no row. A list also says whether its program fetches, which is
the egress a closed network leaves it ([SANDBOX-20](#SANDBOX-20)): cargo, npm and the other node
package managers, pip, go, mvn and gradle fetch, and `node`, `python3` and a program no list knows
do not. `run` adds the list for each stage it starts where the machine is not read: on Windows
and under the mode `strict` ([SANDBOX-18](#SANDBOX-18), [SANDBOX-22](#SANDBOX-22)). Where the machine is read, which is the
default on Linux and macOS, no stage brings a list, and every stage writes the caches of every
toolchain instead ([SANDBOX-18](#SANDBOX-18)).

**Why.** The paths a build resolves through belong to that build and not to every program that runs,
and the file a stage resolved to is the part of the plan a person read, where a name the model wrote
or a configuration file's contents is not. One list shared by every toolchain would let a postinstall
script leave something in `~/.cargo/registry` for a later `cargo build` to read. Keying the lists
prevents that where they apply and does nothing about it where the machine is read, since every
stage there writes every cache and a postinstall script can leave that file. A package manager
keeps its token beside its cache, so a row naming the directory holding the cache hands the token to
every build in that ecosystem. An install written by a confined stage is the `cargo` a later stage
resolves to, and a configuration file written by one is a `build.rustc-wrapper` every later build
runs. Cargo's configuration is read at all because cargo fails every invocation on a configuration
file it cannot open, which a machine with a `~/.cargo/config.toml` otherwise meets on every build.

`verified-by: bravebot_sandbox::toolchain::a_list_is_keyed_on_the_file_a_program_resolved_to`
`verified-by: bravebot_sandbox::toolchain::a_cache_linked_into_a_state_directory_that_is_moved_is_given_no_row`
`verified-by: bravebot_sandbox::toolchain::a_list_is_keyed_on_the_file_without_a_windows_executable_extension`
`verified-by: bravebot_sandbox::programs::a_windows_executable_extension_is_not_part_of_a_programs_name`
`verified-by: bravebot_sandbox::toolchain::a_program_no_list_knows_brings_none`
`verified-by: bravebot_sandbox::toolchain::a_list_names_a_cache_and_never_the_directory_holding_it`
`verified-by: bravebot_sandbox::toolchain::an_install_is_read_and_never_written`
`verified-by: bravebot_sandbox::toolchain::a_cargo_list_reads_the_configuration_cargo_cannot_start_without`
`verified-by: bravebot_sandbox::toolchain::windows_writes_the_cache_its_toolchain_uses_there`
`verified-by: bravebot_sandbox::toolchain::a_list_writes_its_own_ecosystems_cache_and_no_other`
`verified-by: bravebot_sandbox::toolchain::each_platform_writes_the_cache_its_toolchain_uses_there`
`verified-by: bravebot_sandbox::toolchain::no_list_names_a_directory_that_holds_other_programs_files`
`verified-by: bravebot_sandbox::toolchain::a_missing_cache_is_created_as_what_the_toolchain_expects_there`
`verified-by: bravebot_sandbox::toolchain::a_list_leaves_the_policy_it_is_added_to_as_it_was`
`verified-by: bravebot_sandbox::toolchain::a_cache_that_has_become_a_link_to_somewhere_refused_is_given_no_row`
`verified-by: bravebot_sandbox::toolchain::a_cache_that_is_the_other_kind_of_thing_is_given_no_row`
`verified-by: bravebot_sandbox::toolchain::a_cache_linked_to_somewhere_ordinary_is_granted_there_and_held_to_that_name`
`verified-by: bravebot_sandbox::toolchain::a_cache_that_is_a_link_to_nowhere_is_given_no_row`
`verified-by: bravebot_sandbox::toolchain::a_cache_that_is_a_link_to_the_other_kind_of_thing_is_given_no_row`
`verified-by: bravebot_sandbox::toolchain::an_install_or_configuration_that_has_become_a_link_to_a_refused_place_is_given_no_row`
`verified-by: bravebot_sandbox::macos::a_cargo_stage_writes_its_registry_and_reaches_neither_its_token_nor_its_install`

<a id="SANDBOX-16"></a>
### SANDBOX-16: a credential scope follows the operation the argv names, and reaches no private key

A stage carries the remote scope where its program resolved to `git` and its argv is `push`,
`fetch`, `pull`, `clone` or `ls-remote` with nothing in front of the operation but `-C <directory>`,
and where its program resolved to `gh` and its argv starts with one of the commands gh has that
talk to the host: `api`, `attestation`, `auth`, `cache`, `gist`, `gpg-key`, `issue`, `label`,
`org`, `pr`, `project`, `release`, `repo`, `ruleset`, `run`, `search`, `secret`, `ssh-key`,
`status`, `variable` or `workflow`. It carries `~/.aws`, `~/.kube` or `~/.docker` where its
program resolved to `aws`, `kubectl` or `docker`. It carries the signing scope where its program
resolved to `git` and its argv is `commit`, `merge`, `rebase`, `cherry-pick`, `revert`, `am` or
`tag`, with nothing in front of the operation but `-C <directory>`, and the person's own git
configuration signs with ssh. It carries none where a `NAME=value` assignment is
written in front of it; where a `git` argv names a program for git to run, which is any abbreviation
git accepts of `--upload-pack`, `--receive-pack`, `--exec`, `--template`, `--config` or
`--strategy`, a `-u` or `-c` to `clone`, a `-s` to `pull`, an address holding `::`, or one written
`<scheme>://` whose scheme is not `ssh`, `git`, `file`, `http`, `https`, `ftp`, `ftps`, `git+ssh`
or `ssh+git`, before a `--` or after one; where a signing `git` argv holds any abbreviation of
`--exec` or `--strategy`, or a `-x` or `-s` to `rebase` or a `-s` to `merge`, which run a command or
a `git-merge-<name>` from the search path; where a `gh` argv holds `--`; and where `kubectl` is
given `--kubeconfig` or `docker` is given `--config`. None is carried by a program whose resolved
file is under a directory the plan may write to ([SANDBOX-20](#SANDBOX-20) says which those are,
toolchain caches and the checkout holding a session directory among them), which could have been named `aws` or `git` by the plan itself, as
[SANDBOX-20](#SANDBOX-20) holds for the network. A stage reaches its own scope and no other.
No scope names a private key or `~/.ssh` as a directory. The remote scope reads `~/.ssh/config`,
`~/.ssh/known_hosts`, the public key at each name ssh looks for by default, the public key file
each `IdentityFile` line of `~/.ssh/config` names, `~/.gitconfig`, `~/.git-credentials`,
`~/.config/git/credentials`, `~/.netrc` and `~/.config/gh`, and writes `~/.ssh/known_hosts` alone,
as a file. A tool's directory is read and never written.

A line `IdentityFile <name>` in the user's own `~/.ssh/config` adds the file `<name>.pub`, and one
whose name already ends in `.pub` adds that file. The keyword is matched without regard to case
and its value may follow `=`. A name is absolute or begins `~/` or `%d/`. A name holding another `%`
token or a `$`, or a relative one, adds nothing, and neither does a line that is not valid UTF-8,
which is skipped whole and never rewritten into a name it did not spell. A file is added only where it exists as a regular
file, where the file a link leads to is named `*.pub`, and where that file is not in a credential
location other than `~/.ssh` ([SANDBOX-12](#SANDBOX-12)), and the row is the file a link leads to.
The `.pub` name is the only thing added, so a line that names a private key adds the public file
beside it and never the key. `Include`, `Match` and a repository's `core.sshCommand` are not
followed, `~/.ssh/config` is read up to 1 MiB, and at most 32 files are added. A file that cannot
be read adds nothing. Only the remote scope reads the configuration. A key that `-i` names in a
`core.sshCommand` is read only if the configuration also lists it as an `IdentityFile`.

A person chooses the key a push offers the host for one owner's repositories with `git.identities`
in their settings, an object whose names are `<host>/<owner>` and whose values name a key file:
`{"git": {"identities": {"github.com/netzenbot": "~/.ssh/netzenbot.pub"}}}`. It applies to a stage
whose program resolved to `git` and that carries the remote scope by its argv. An argument after the
operation that does not start with `-` is an ssh address where it is written
`ssh://[user@]host[:port]/<owner>/...`, the same with `git+ssh://` or `ssh+git://`, or
`user@host:<owner>/...` with no `/` before the first `:`; a scp-like address needs the user, so a
refspec such as `HEAD:refs/heads/main` is not one. Where the argv names one such address, or names
the same host and owner more than once, the entry for that host and owner decides, both compared
without regard to ASCII case. The stage then starts with `GIT_SSH_COMMAND` set to
`ssh -i '<key>' -o IdentitiesOnly=yes` and reads that one file. ssh then offers that key first and
after it only the keys an `IdentityFile` line of the person's ssh configuration names for the host,
and no other key the agent holds. The value is judged as an `IdentityFile` name is: absolute or
beginning `~/`, `.pub` added to a name that does not end in it, a regular file named `*.pub` once a
link is followed, and outside every credential location other than `~/.ssh`. It must also lie
outside every directory the plan may write ([SANDBOX-20](#SANDBOX-20)). A path holding `'` is refused, since git hands the variable to a shell, and so
is one holding `%` or `$`, which ssh expands in a key's name, and one that is not valid UTF-8. A
refused key, an address with no entry, an argv naming two different addresses and an argv naming
none leave the stage as it was. The address is read from the argv and nothing else: a remote given
by name, such as `origin`, has its URL in the repository's configuration, which is workspace content
a clone or an earlier stage may have written, so it selects no entry. The run prompt names the key
on the stage it applies to. Only the person's own settings file and a `--settings` file outside the
workspace set the block, as for `sandbox.mode` ([SANDBOX-22](#SANDBOX-22)). A checkout's project and
local files and a `--settings` file inside the workspace or such a checkout are not obeyed, and `doctor` names the
file, as it names an entry whose name is not `<host>/<owner>` or whose value is not a string, a
`git` block or an `identities` value that is not an object, and an entry whose key the rules above
refuse, all but the one about the directories the plan may write, which it has no session to know.
A key beside `identities` in the block is reported as unread.

Where no entry names the owner, the person chooses the key at the run prompt of the terminal
interface. The prompt numbers the public keys ssh could offer, those at its default names and then
those an `IdentityFile` line of the person's ssh configuration names, each judged as an entry's key
is, outside every directory the plan may write included. It does so for a line whose `git` stages
that carry the remote scope by their argv and are decided by nothing all name one host and owner,
where there are two to nine such keys, and in a session that may write to its state directory. A
stage is decided by nothing where no entry names its owner and no choice was made for it. `1` to the
last number runs the line with that key offered first, and `0` runs it with the key left to ssh.
Either writes the choice for the host and owner to `reach.jsonl` in the state directory, and it then
decides, as an entry would, every later stage whose argv names an ssh address of that owner, in
every session and checkout, without asking again. `y` runs the line as it was and remembers nothing.
The digits wait for the rows naming the keys and the file to have been on the screen, a digit past
the last number answers nothing, and the layer that acts on the answer reads the digit against the
list it made rather than against what was drawn, so no front end can choose a key it was not shown.
An entry decides over a choice, and a chosen key that is no longer one a stage may be given, or that
ssh could no longer offer, leaves the owner decided by nothing, so the prompt asks again. The run
prompt names a chosen key, or that the key was left to ssh, with the owner and the day it was
chosen. `/reach keys` lists the choices and `/reach keys remove <number>` forgets one. The plain
interface and the desktop number no keys. A choice is read wherever the session has a state
directory, a session with nobody to ask included: it adds no row, since a choice applies only to a
key ssh could offer, which the remote scope already reads, and it only changes which of them ssh
offers first.

A line whose `run` call asks for `ssh-key` ([SANDBOX-26](#SANDBOX-26)) is asked about the key again
for an owner a choice was made for at an earlier prompt, whether that choice was a key or ssh's own
order, on the same terms: the owner is the one every such stage names, and the answer replaces the
earlier choice. It changes nothing for an owner an entry names, which decides over any choice and
is changed in the settings, nor for a stage with a `NAME=value` in front of it, which carries no
scope. The request lends no path, adds no row and names no key, so the planner chooses nothing: it
only has the person asked.

`bravebot doctor --push-identity [<host>/<owner>]` asks which account each key pushes as. It takes
every public key a stage with the remote scope could be offered, the ones at ssh's default names and
the ones an `IdentityFile` line of the person's ssh configuration names, and runs `ssh -T -i <key>
-o IdentitiesOnly=yes git@<host>` once for each, so the host answers for that key alone. Each key
is printed with its fingerprint and the login the greeting names, or that no account on the host
has it (`Permission denied (publickey)`), or that the host did not answer, and a login is compared
with the owner, which is the argument or the address of `origin` in the directory it ran in. It
runs only when asked, since a hardware key waits for a touch, and the host is `github.com` where
neither names another. The greeting is bytes from a remote host: it is read for a login spelled as
GitHub spells one, letters, digits and `-` up to 39, and nothing else of it is kept. The login and
the comparison are shown on the person's screen and nowhere else. They never choose a key, never
reach a stage, a prompt, the planner or the trace, and the key marked as chosen is the one
`git.identities` names, or failing an entry the one the person chose at a run prompt, whatever any
host said.

A signed commit runs `ssh-keygen -Y sign` with the file `user.signingkey` names. The tool loads that
public key and asks the agent for the signature, so a stage that signs needs the one file and the
agent's socket, and never the private key. The signing scope reads that file and the stage may write
the socket `SSH_AUTH_SOCK` names, as the remote scope may, judged as the remote scope's is
([SANDBOX-18](#SANDBOX-18)). It is carried only where the person's own
`~/.gitconfig` or `~/.config/git/config` sets `gpg.format` to `ssh`, so an account that does not sign
with ssh sees no difference, the network under a closed setting included. The later file and the last
assignment win, section and variable names are matched without regard to case, a quoted value is
unquoted and an unquoted `#` or `;` starts a comment. A `[gpg "ssh"]` or `[user "name"]` section is
another section, a value holding a backslash is not read, and a repository's configuration, an
`[include]`, an `[includeIf]` of any condition but `gitdir:` and `gitdir/i:`, and `GIT_CONFIG_GLOBAL` are
not followed. The key is read where `user.signingkey` is a
key written out (`ssh-` or `key::`), which needs no file, where it is unset, which signs with the
agent's first key and needs no file, or where it is an absolute path or one beginning `~/` that
resolves to an existing regular file named `*.pub` inside the home directory and outside every
credential location other than `~/.ssh`. A private key, a name nothing is at, a relative path, a path
holding `..`, a file outside the home and a link that leads out of those places are refused, and
the stage is told so in the profile line without the value, since the signature then fails. `git
pull` merges or rebases and signs the commit it makes, so the remote scope reads the same file. The
scope is also on the menu of [SANDBOX-26](#SANDBOX-26), for a script that runs `git commit` or `git
rebase` inside it and so shows no operation of its own: a request for `signing` reads the same file
and lends the same agent socket, to a person whose git signs with ssh only, and it is the only scope `/reach` does not name and
[SANDBOX-27](#SANDBOX-27) does not remember. The configuration limits are these: `-S<key>` and `--gpg-sign=<key>` in the argv do
not move the key that is read, and a hook or a `gpg.ssh.program` the plan wrote into `.git` runs
with the scope, as one runs with the remote scope.

An `[includeIf "gitdir:<pattern>"]` or `[includeIf "gitdir/i:<pattern>"]` in either of those two files
is followed, in the place of the file where it stands, so that a person who signs with one key for the
work under `~/work/` and another elsewhere is lent the key git signs with there. The condition is
matched as git matches it, against the `.git` inside the first directory of the session: `~/` is the
home, `./` is the directory of the file that holds the line, a pattern with none of `/`, `~/` and `./`
before it is preceded by `**/`, a trailing `/` adds `**`, `*` and `?` stay inside one part of a path,
`**` is any number of parts, and `gitdir/i` ignores case. A pattern holding `[` or `\` matches
nothing, and so does a condition when the home, the directory of the file that holds the line or
the session's first directory is not valid UTF-8. A `hasconfig:` and an `onbranch:` condition are
not followed, because the first reads the repository's configuration and the second its state, and
a linked worktree is matched by its own
`.git` and not by the directory it links to. The file the `path` names, beside the including file
where it is relative, is read only where it resolves to a regular file inside the home, outside every
credential location other than `~/.ssh`, and outside every directory the session may write, which
are its directories and its scratch directory, since the plan can write those and the key would then
be one the plan chose. A directory the person lets the session write later, by approving a path or a
grant, is not counted. An included file's own `[includeIf]` is not followed. The repository's own
configuration is still not read, whatever it holds: a plan can write it, and the file it names is
read on the strength of it. The profile line of a stage that carries the signing scope, or was asked
to carry it, always says, whether or not the person's git signs with ssh, that the key comes from
`~/.gitconfig` or `~/.config/git/config` or a file an `[includeIf]` of theirs includes for the
directory, and that a `user.signingkey` set in a repository's own configuration is not read. The text is fixed: it holds no value from any
configuration and does not change with the repository, so that a signature which fails with "Couldn't
load public key" is traced to the setting by the planner without reading anything it was not lent.
It also says that a step that signs never reads a private key and signs through the person's ssh
agent, and whether the agent's socket was lent to the step, decided from the stage's environment and
the person's own configuration and never from what a program printed. Where the socket was lent, a
signature failing with "No private key found" means the configured key was lent but the agent does
not hold it, and the person solves it by loading the key with `ssh-add`. Where the step asked for a
scope that lends it and the session has no socket, the line says so and that the person solves it by
starting an agent that holds the key. Where no scope of the step lends it because `gpg.format` is
not `ssh` in the person's own configuration, the line says that the sandbox hid the agent, names
that setting, and names the `remote` scope, which lends the socket whatever the configuration says.
Where
the session would accept `'unconfined': true` for the line (the condition under which a refused
write names it, [SANDBOX-29](#SANDBOX-29)) it adds that the one commit line may be run with it, the
person being asked to approve it; otherwise it names no such argument.

Where a variable moves what a tool reads, the stage also reads the place the variable names, as that
place and read only: for `gh`, `GH_CONFIG_DIR` if the environment the stage starts with sets it, else
`$XDG_CONFIG_HOME/gh`, a directory; for `git`, `GIT_CONFIG_GLOBAL` and
`$XDG_CONFIG_HOME/git/credentials`, files; for `aws`, `AWS_CONFIG_FILE` and
`AWS_SHARED_CREDENTIALS_FILE`, files; for `kubectl`, each file `KUBECONFIG` lists, which is split as
`PATH` is and judged one entry at a time; and for `docker`, `DOCKER_CONFIG`, a directory. The fixed rows
stay. A value is refused where it is relative or holds `..`; a directory also where it is the home or
above it, is `~/.ssh` or inside it, or is `~/.config`, `~/.cache` or `~/Library`; a file where it is
inside `~/.ssh`, is the home or above it, or is a directory; a link is judged by where it leads. A refused entry leaves the others, and the stage
keeps its fixed rows. A value that lands inside a row the scope already holds adds nothing, and one named twice counts once. The run
prompt says each place taken from the environment, with the variable that named it and the path it
led to, beside the sentence for the scope. A scope a `run` call asked for by name
([SANDBOX-26](#SANDBOX-26)) follows the variables of every program the scope has, since the stage
that carries it is not one of them: `gh` and `git` for `remote`. The planner names the scope and
never a variable or a path, and the driver reads the value from the process environment.

A scope is added to the policy it is given and takes nothing from it. `run` adds the scope for each stage it
starts ([SANDBOX-18](#SANDBOX-18)). Where the stage's base refuses a credential location
([SANDBOX-12](#SANDBOX-12)), a scope's read row for that location lifts the refusal for that stage
and for no other, so `aws`, `kubectl` and `docker` read `~/.aws`, `~/.kube` and `~/.docker` where
their scope is carried and are refused them where it is not. The remote scope's read rows are
lifts of the same kind, and no row it names is a private key. A stage whose argv carries no scope
reads no credential location, whatever it is run beside. The places a variable names are added
only where the stage's read is a list of paths. Where the stage reads the machine, a place a
variable names is read already unless it lies in a refused location, and it is refused there.

**Why.** A push is how most sessions end, so a profile that refuses one is one somebody turns off.
`git` runs whatever its argv or its environment names, so a scope granted on the first word of a
line is a credential lent to a program the plan never showed: `--upload-pack=` runs one, `ext::` or
`a-helper://` an address, `GIT_SSH_COMMAND=` a variable. `gh` hands what follows `--` to `git`
unread. The operation is matched exactly for `git` and for `gh`, and each runs a command of its own
ahead of an alias or an extension of the same name, so an alias or an extension carries nothing
whatever the configuration says it runs. `aws`, `kubectl` and `docker` carry their scope on the
program alone, so it reaches what one of them hands a command to: a `kubectl` plugin found on the
search path, a docker CLI plugin, an `aws` alias beginning `!`, and the credential plugin a
kubeconfig names, each able to read that tool's directory. A push signs through the agent and ssh
reads the public half of a key to name an identity to it, so the private half is never needed. A
write to a tool's directory is a program the person's own shell runs later: a `credential_process`,
an exec plugin, a `credsStore` helper. The `gh` directory follows the environment where
[SANDBOX-15](#SANDBOX-15) does not follow `CARGO_HOME`, because a toolchain cache is a place the
program writes and this row is the location of a file the person's own tool is about to open; the
same holds for each variable above, and without it a person whose configuration lives elsewhere gets
`Operation not permitted` from a stage the plan said was fine. A place taken from the environment is
shown in the prompt because a credential scope is what the prompt says it is, and a row nobody saw
is not one anybody approved. Only the environment the stage starts with counts: an assignment
written in front of the line removes the scope, so a model-written `GH_CONFIG_DIR=` moves nothing.
A file is judged where it is rather than where its directory is: what is granted is the one file the
person named, and `~/.ssh` is where a key would be. ssh reads the `.pub` file of an identity to choose
the key the agent offers, so a key the person's configuration pins by name needs the same read as one
at a default name; with `IdentitiesOnly yes` and no such read, ssh offers nothing and the host answers
`Permission denied (publickey)`. The configuration is the person's own file, which a stage in
workspace-write mode cannot write, and a line in it can add only a file named `*.pub`, so reading it adds no
private key. A repository's `core.sshCommand` is not read because it decides what runs. Signing is a scope of its
own so that the agent and the network are not lent to a `git commit` of an account that does not sign
with ssh, and so that a commit does not read `~/.git-credentials`; the key it needs is one file in
the person's home that their configuration names, and is read for the same reason `IdentityFile`
keys are. A configuration of the person's is read for it and a repository's is not, because the
repository's is a file the plan may write. `~/.bravebot` is refused the same way, including the session records the run base reads, since a
variable that named it would lift the refusal of the gateway keys ([SANDBOX-12](#SANDBOX-12)).
ssh offers an agent's keys in order and GitHub accepts the first one registered to any account, so a
person whose agent holds two accounts' keys pushes every repository as one account unless a key is
chosen. Nothing else on a stage chooses it, since an assignment in front of the line removes the
scope and a key a repository's `core.sshCommand` names is read only where the person's configuration
names it too. `git.identities` is the person's choice, for an
owner they named, selected by an address the planner wrote and the person approved, so no byte a
program or a repository wrote picks the credential. `IdentitiesOnly=yes` stops ssh offering the
agent's keys the configuration does not name, and the agent still signs, so the private key is not
read. A person who has not written the setting meets the rejected push first, so the prompt for the
push that needs the answer asks for it once per owner, from keys the person's own files name. The
answer is kept in the state directory with the reach a prompt remembers rather than written into a
settings file the person edits by hand.

A repository can set `user.signingkey` to a key the person's own configuration does not name, and
the repository's configuration is not read, so a stage that signs with it cannot load that public key
even though the agent it is lent holds the key. A stage that signs, and a stage with the remote scope
for a person whose git signs with ssh, is therefore also lent every `*.pub` file directly inside
`~/.ssh` whose key the agent holds. The agent named by the `SSH_AUTH_SOCK` the process started with, which a step that carries the scope
cannot assign, is asked for
its keys with the identities request of the agent protocol, and a file is lent where its first line
holds one of them. The agent already signs with any key it holds for a stage that reaches its socket,
so a file that only names one lends nothing the socket does not. What is lent does not depend on the
repository's configuration, which stays unread. Each file gets the checks a file named by an
`IdentityFile` gets: a regular file named `*.pub` once a link is followed, outside every credential
location other than `~/.ssh`, never a private key, and at most 32 files. Where there is no
`SSH_AUTH_SOCK`, nothing answers there in full within two seconds, or the answer is not an identities
list,
only the configured key is lent. Where `gpg.format` is not `ssh` in the person's own configuration
nothing is lent. This adds no dependency and is not done on Windows. The prompt says so in the fixed
sentence about the signing key, so that a signature that fails for a key the agent does not hold can
be told from one the scope never reached.

**What it costs.** git takes `GIT_SSH_COMMAND` over `GIT_SSH` and every `core.sshCommand`, so on
the stages an entry applies to it replaces those of the person's environment and their own git
configuration, and the ssh options they carried are not passed there. The variable is the stage's,
so every ssh connection the stage makes offers the key first, a submodule of another owner's
included. git rewrites the argv's address by the repository's `url.<base>.insteadOf` and
`pushInsteadOf`, so a repository can send the push, with the key chosen for the owner the argv
named, to another address; a repository's configuration already acts within the remote scope, as
its hooks do. A path a person lent with `request_path`, an `allowWrite` row and the rows a bypass
stage holds without a request are not counted among the directories a key must lie outside, as
they are not for a program's file below; a stage lent the agent can sign with any key it holds
whatever file names one. A push to a remote given by name gets no entry, so the planner writes the
ssh address in the argv for the person's choice to apply.

The directories counted against a program's file are the plan's own. A path a
person lent with `request_path` ([SANDBOX-28](#SANDBOX-28)), an `allowWrite` row
([SANDBOX-25](#SANDBOX-25)) and the rows a bypass stage holds without a request are not counted, so a
program an earlier stage wrote into one of them carries the scope its name has. A session opened on
the home directory carries no scope for a program installed under it, such as `~/.local/bin/aws`.

The one row a scope writes, `~/.ssh/known_hosts`, is a file the person's own ssh trusts, so a stage
that carries the remote scope can add a line to it that makes every later ssh of theirs accept a
host it would have refused: a host key of the stage's choosing, or a `@cert-authority` line naming
a key of the stage's, after which ssh accepts any host whose certificate that key signed. The row is there because ssh records a host
the first time it meets one. Without it, under `StrictHostKeyChecking accept-new`, a host met first
inside a stage is never recorded, so each later connection to it accepts whatever key it is offered,
and under the default `ask` a person at a terminal is asked again on every push.

`verified-by: bravebot_sandbox::scope::an_operation_that_talks_to_a_remote_carries_the_remote_scope`
`verified-by: bravebot_sandbox::scope::a_git_operation_that_talks_to_no_remote_carries_none`
`verified-by: bravebot_sandbox::scope::a_gh_argv_that_runs_a_program_gh_did_not_write_carries_none`
`verified-by: bravebot_sandbox::scope::an_option_in_front_of_the_operation_carries_none`
`verified-by: bravebot_sandbox::scope::an_operation_option_naming_a_program_carries_none`
`verified-by: bravebot_sandbox::scope::a_stage_with_an_assignment_in_front_of_it_carries_none`
`verified-by: bravebot_sandbox::scope::an_option_that_runs_nothing_keeps_the_scope`
`verified-by: bravebot_sandbox::scope::a_tool_pointed_at_another_configuration_file_carries_none`
`verified-by: bravebot_sandbox::scope::a_program_no_scope_knows_carries_none`
`verified-by: bravebot_sandbox::scope::a_scope_is_keyed_on_the_file_without_a_windows_executable_extension`
`verified-by: bravebot_agent::confine::a_program_the_plan_could_have_written_carries_no_scope_by_its_name`
`verified-by: bravebot_agent::confine::a_program_above_the_session_directory_in_its_checkout_carries_no_scope_by_its_name`
`verified-by: bravebot_sandbox::scope::a_stage_reaches_its_own_scope_and_no_other`
`verified-by: bravebot_sandbox::scope::no_scope_reaches_a_private_key_or_the_directory_holding_one`
`verified-by: bravebot_sandbox::scope::a_name_inside_the_state_directory_is_refused_whatever_spells_it`
`verified-by: bravebot_sandbox::scope::the_one_row_a_scope_writes_is_the_hosts_ssh_has_verified`
`verified-by: bravebot_sandbox::scope::the_remote_scope_reaches_both_transports`
`verified-by: bravebot_sandbox::scope::a_scope_leaves_the_policy_it_is_added_to_as_it_was`
`verified-by: bravebot_sandbox::scope::gh_reads_the_configuration_directory_its_environment_names`
`verified-by: bravebot_sandbox::scope::a_gh_directory_that_is_too_wide_or_not_a_path_is_refused`
`verified-by: bravebot_sandbox::scope::a_gh_directory_that_is_a_link_is_judged_by_where_it_leads`
`verified-by: bravebot_sandbox::scope::a_variable_that_moves_a_tools_configuration_moves_its_row`
`verified-by: bravebot_sandbox::scope::a_variable_moves_the_row_of_the_tool_that_reads_it_and_no_other`
`verified-by: bravebot_sandbox::scope::a_kubeconfig_list_is_judged_one_entry_at_a_time`
`verified-by: bravebot_sandbox::scope::a_value_that_is_relative_or_reaches_a_key_is_refused_for_every_tool`
`verified-by: bravebot_sandbox::scope::a_file_that_is_a_link_to_a_private_key_is_refused`
`verified-by: bravebot_sandbox::scope::an_entry_a_list_names_twice_is_reached_once`
`verified-by: bravebot_sandbox::scope::a_variable_pointing_at_a_row_the_scope_already_holds_adds_none`
`verified-by: bravebot_sandbox::scope::a_reach_names_the_variable_it_came_from`
`verified-by: bravebot_agent::confine::a_gh_stage_reads_the_configuration_directory_its_environment_names`
`verified-by: bravebot_agent::confine::a_stage_reads_the_configuration_its_variable_moves`
`verified-by: bravebot_agent::confine::the_prompt_names_a_location_the_environment_moved`
`verified-by: bravebot_sandbox::scope::a_requested_scope_follows_the_variables_of_every_program_it_has`
`verified-by: bravebot_sandbox::scope::a_requested_scope_refuses_the_values_a_carried_one_refuses`
`verified-by: bravebot_agent::confine::a_requested_scope_reads_the_configuration_its_variable_moves`
`verified-by: bravebot_agent::confine::the_prompt_names_a_location_the_environment_moved_for_a_requested_scope`
`verified-by: bravebot_sandbox::macos::a_remote_stage_reads_what_ssh_reads_and_never_a_private_key`
`verified-by: bravebot_sandbox::scope::an_identity_file_in_the_ssh_configuration_adds_its_public_key`
`verified-by: bravebot_sandbox::scope::an_identity_file_that_is_not_a_public_key_adds_nothing`
`verified-by: bravebot_sandbox::scope::a_named_public_key_behind_a_link_is_read_where_it_leads`
`verified-by: bravebot_sandbox::scope::an_identity_file_line_that_is_not_text_adds_nothing_and_its_lossy_lookalike_is_not_read`
`verified-by: bravebot_sandbox::scope::an_identity_file_whose_name_is_not_text_adds_neither_it_nor_its_lookalike`
`verified-by: bravebot_sandbox::scope::an_operation_that_signs_carries_the_signing_scope`
`verified-by: bravebot_sandbox::scope::a_signing_operation_that_names_a_program_carries_nothing`
`verified-by: bravebot_sandbox::scope::signing_does_not_change_the_other_scopes`
`verified-by: bravebot_sandbox::scope::signing_can_be_requested_and_not_remembered`
`verified-by: bravebot_sandbox::scope::the_signing_scope_reads_the_public_key_and_nothing_else`
`verified-by: bravebot_sandbox::scope::the_signing_scope_reads_nothing_where_the_key_is_refused_or_signing_is_off`
`verified-by: bravebot_sandbox::scope::the_remote_scope_reads_the_signing_key_as_well`
`verified-by: bravebot_sandbox::signing::a_public_key_in_the_home_is_the_file_a_stage_that_signs_reads`
`verified-by: bravebot_sandbox::signing::a_format_other_than_ssh_reads_no_key`
`verified-by: bravebot_sandbox::signing::a_literal_key_and_no_key_read_no_file`
`verified-by: bravebot_sandbox::signing::a_value_that_is_not_a_public_key_in_the_home_is_refused`
`verified-by: bravebot_sandbox::signing::the_last_value_of_the_right_section_is_the_one_read`
`verified-by: bravebot_sandbox::signing::a_value_with_a_backslash_is_refused`
`verified-by: bravebot_sandbox::signing::an_include_if_that_matches_the_runs_directory_supplies_the_key`
`verified-by: bravebot_sandbox::signing::an_include_if_is_read_where_it_stands_in_the_file`
`verified-by: bravebot_sandbox::signing::a_gitdir_condition_matches_as_git_matches_it`
`verified-by: bravebot_sandbox::signing::a_gitdir_pattern_with_a_class_or_an_escape_matches_no_directory`
`verified-by: bravebot_sandbox::signing::a_relative_include_path_is_beside_the_file_that_holds_it`
`verified-by: bravebot_sandbox::signing::a_directory_that_is_not_utf8_matches_no_gitdir_pattern`
`verified-by: bravebot_sandbox::signing::an_include_if_whose_file_is_inside_the_workspace_is_refused`
`verified-by: bravebot_sandbox::signing::an_include_if_whose_file_is_not_a_plain_file_in_the_home_is_refused`
`verified-by: bravebot_sandbox::signing::an_include_inside_an_included_file_and_a_plain_include_are_not_followed`
`verified-by: bravebot_sandbox::signing::a_repository_key_is_not_read_when_a_global_key_overrides_it`
`verified-by: bravebot_sandbox::agent::an_agent_that_holds_two_keys_lists_both`
`verified-by: bravebot_sandbox::agent::an_agent_that_cannot_be_asked_holds_nothing`
`verified-by: bravebot_sandbox::agent::an_agent_that_never_answers_holds_nothing_after_the_wait`
`verified-by: bravebot_sandbox::agent::an_answer_cut_short_holds_no_key`
`verified-by: bravebot_sandbox::signing::a_public_key_in_ssh_that_the_agent_holds_is_lent_beside_the_configured_key`
`verified-by: bravebot_sandbox::signing::a_public_key_in_ssh_that_the_agent_does_not_hold_is_not_lent`
`verified-by: bravebot_sandbox::signing::with_no_agent_that_answers_only_the_configured_key_is_lent`
`verified-by: bravebot_sandbox::signing::the_repositorys_signing_key_does_not_change_which_files_are_lent`
`verified-by: bravebot_sandbox::signing::a_private_key_beside_a_matching_public_key_is_never_lent`
`verified-by: bravebot_sandbox::signing::nothing_is_lent_where_git_does_not_sign_with_ssh`
`verified-by: bravebot_sandbox::signing::a_link_into_another_credential_location_and_a_nested_file_are_not_lent`
`verified-by: bravebot_sandbox::signing::no_more_than_thirty_two_files_are_lent_for_held_keys`
`verified-by: bravebot_sandbox::signing::base64_pads_the_way_a_public_key_file_does`
`verified-by: bravebot_sandbox::scope::the_scopes_that_sign_read_a_public_key_in_ssh_that_the_agent_holds`
`verified-by: bravebot_sandbox::macos::a_stage_that_signs_loads_a_public_key_the_agent_holds_and_no_other`
`verified-by: bravebot_agent::confine::a_stage_that_signs_is_lent_a_public_key_in_ssh_that_its_agent_holds`
`verified-by: bravebot_agent::confine::every_way_to_carry_the_signing_scope_lends_the_public_key_the_agent_holds`
`verified-by: bravebot_sandbox::signing::a_glob_matches_by_path_part`
`verified-by: bravebot_agent::confine::a_stage_that_signs_is_lent_the_key_and_the_agent_where_the_person_signs_with_ssh`
`verified-by: bravebot_agent::confine::a_stage_that_signs_carries_nothing_where_the_person_does_not_sign_with_ssh`
`verified-by: bravebot_agent::confine::a_stage_that_does_not_sign_is_not_lent_the_agent`
`verified-by: bravebot_agent::confine::a_signing_key_no_scope_reads_is_explained_to_the_planner_without_its_path`
`verified-by: bravebot_agent::confine::a_stage_that_signs_is_always_told_where_the_key_comes_from`
`verified-by: bravebot_agent::confine::a_failed_signature_says_whether_the_agent_socket_was_lent`
`verified-by: bravebot_agent::confine::a_stage_that_asked_for_signing_is_lent_the_key_and_the_agent`
`verified-by: bravebot_agent::confine::a_stage_that_signs_is_lent_the_key_an_include_if_supplies_for_its_directory`
`verified-by: bravebot_sandbox::macos::a_stage_that_signs_loads_the_public_key_and_reaches_the_agent_never_the_private_key`
`verified-by: bravebot_sandbox::scope::the_ssh_configuration_adds_a_bounded_number_of_keys_to_the_remote_scope_alone`
`verified-by: bravebot_sandbox::macos::a_remote_stage_reads_a_public_key_its_configuration_names_and_never_the_private_one`
`verified-by: bravebot_agent::confine::the_prompt_the_line_and_the_policy_agree_on_which_credential_a_stage_lifts`
`verified-by: bravebot_sandbox::scope::an_ssh_address_in_the_argv_names_its_host_and_owner`
`verified-by: bravebot_sandbox::scope::a_remote_name_a_refspec_or_another_transport_names_no_address`
`verified-by: bravebot_sandbox::scope::an_identity_key_is_a_public_key_file_and_never_a_private_one`
`verified-by: bravebot_sandbox::scope::the_ssh_command_offers_one_key_and_refuses_a_path_it_cannot_quote`
`verified-by: bravebot_sandbox::scope::the_ssh_command_refuses_a_key_whose_path_is_not_text`
`verified-by: bravebot_config::git_identities::an_entry_is_read_by_host_and_owner_and_found_without_regard_to_case`
`verified-by: bravebot_config::git_identities::a_misshapen_entry_is_reported_and_the_rest_are_read`
`verified-by: bravebot_config::git_identities::a_key_beside_identities_is_reported_as_unread`
`verified-by: bravebot_config::settings::only_the_persons_own_files_choose_a_key_for_an_owner`
`verified-by: bravebot_cli::running::doctor_names_a_checkouts_key_choice_and_an_entry_it_cannot_read`
`verified-by: bravebot_sandbox::scope::candidate_keys_are_the_default_names_and_the_named_ones_once_each`
`verified-by: bravebot_cli::push_identity::a_greeting_and_a_refusal_are_told_apart`
`verified-by: bravebot_cli::push_identity::a_login_that_is_not_spelled_as_one_is_not_shown`
`verified-by: bravebot_cli::push_identity::each_key_reports_the_login_its_own_connection_got`
`verified-by: bravebot_cli::push_identity::a_key_no_account_has_is_shown_as_not_recognised`
`verified-by: bravebot_cli::push_identity::the_chosen_key_is_the_one_the_settings_named_not_the_one_that_greeted`
`verified-by: bravebot_cli::push_identity::a_key_chosen_at_a_run_prompt_is_marked_as_chosen_there`
`verified-by: bravebot_cli::running::doctor_push_identity_marks_the_key_chosen_at_a_run_prompt`
`verified-by: bravebot_cli::running::doctor_push_identity_reports_the_login_each_key_gets`
`verified-by: bravebot_cli::running::doctor_push_identity_refuses_an_argument_that_is_not_a_host_and_owner`
`verified-by: bravebot_cli::running::a_configured_git_identity_reaches_only_the_matching_run_stage`
`verified-by: bravebot_agent::confine::an_entry_for_one_owner_gives_its_key_to_that_owners_address_and_no_other`
`verified-by: bravebot_agent::confine::no_entry_leaves_a_remote_stage_as_it_was`
`verified-by: bravebot_agent::confine::a_key_the_plan_could_have_written_or_that_is_not_a_public_key_is_not_offered`
`verified-by: bravebot_tui::confirm::run::a_run_prompt_says_what_a_confined_run_is_held_to`
`verified-by: bravebot_agent::confine::a_key_chosen_at_a_prompt_is_offered_for_its_owner_and_an_entry_in_the_settings_wins`
`verified-by: bravebot_agent::confine::the_prompt_offers_a_key_choice_only_for_one_owner_left_to_ssh_with_two_keys_or_more`
`verified-by: bravebot_agent::reach::a_chosen_key_is_read_back_by_host_and_owner_and_the_latest_choice_wins`
`verified-by: bravebot_agent::reach::grant_lines_and_key_lines_are_each_read_by_their_own_reader_only`
`verified-by: bravebot_agent::reach::reach_keys_lists_the_chosen_keys_and_remove_forgets_one`
`verified-by: bravebot_tui::confirm::run::the_key_digits_choose_only_the_keys_the_prompt_numbered`
`verified-by: bravebot_tui::confirm::run::the_key_digits_wait_for_the_rows_naming_the_keys`
`verified-by: bravebot_tui::confirm::run::a_prompt_offering_a_key_choice_names_each_key_and_where_the_choice_is_written`
`verified-by: bravebot_agent::turn::a_key_chosen_at_the_run_prompt_is_offered_and_remembered_and_one_not_numbered_is_not`
`verified-by: bravebot_cli::running::a_key_chosen_at_a_run_prompt_reaches_the_matching_run_stage`

<a id="SANDBOX-17"></a>
### SANDBOX-17: a program `run` starts is started under its plan's profile, or not started

On every platform every stage of a plan `run` starts is started under the profile
[SANDBOX-18](#SANDBOX-18) composes for it: each stage of a pipeline, a stage of a line left running
in the background, and a stage of a plan a subagent runs. The profile is applied after the stage's
environment is set and scrubbed and before its standard streams are connected, so it is the process
that reads its arguments that is confined. A stage the platform cannot confine, because the backend
is unavailable or will not apply the policy, is not started, the pipeline's other stages are
stopped, and the turn is told the program was not started since it could not be confined. The one way a
program is started with no profile is the mode `off` ([SANDBOX-22](#SANDBOX-22)), which a person
chose by name in their own settings or on the command line, and which a window never reads.

On Linux every program the backend starts, a stage among them, is made the leader of a session of
its own before the policy is applied, and is not started if that fails. A process with no controlling
terminal is refused `TIOCSTI` on the terminal the session runs in, and Landlock governs an ioctl on a
device only from ABI 5 (kernel 6.10), so on an older kernel the session is what keeps a program from
typing into the terminal. A terminal's interrupt and hangup do not reach such a program.

Confinement is a property of the session and not of the plan: the terminal, desktop, plain and
one-shot front ends each ask for it when they open a session. A caller that does not ask, which is a
test driving the executor directly, starts stages as the user's own shell would.

On Windows the stage is created in a container instead of being handed back as a command, since
confinement there is an argument to the call that creates the process: the executor gives the
backend the three standard streams it has made and keeps the sandbox alive for as long as the
process, because the entries the backend wrote are removed as it is dropped. A batch file (`.bat`
or `.cmd`) is refused there, because the command interpreter reads its argument line by its own
rules and a quoting that is safe for every other program is not safe for it. Any other program the
platform cannot confine is refused as it is elsewhere. The stage is created in a job of its own,
which the creation call names, so it is in the job before it runs and every program it starts is in
that job too. The job also keeps the stage from the clipboard (read and write), the global atom
table, creating or switching desktops and the USER handles of processes outside the job, whatever
the policy. Where the policy denies subprocesses the job holds a limit of one active process, which
is the stage itself, so the program it started is the only one in it and a second is refused by the
platform; where the policy allows subprocesses there is no such limit. Killing the stage
ends the job, and the job also ends when its one handle is closed, which the backend does when the
process is let go of and the platform does when this process ends, a crash included. A process
killed while it holds a stage ends the programs in the job and leaves the container profile
registered ([SANDBOX-12](#SANDBOX-12) says what bounds that; the test that looks for the profile
passed in run 42972425516, job 130363809021). The executor
kills each stage of a line when the line is done, so a program a stage started and left running
ends then too, which is when the grants it ran under are taken off. A stage the platform will not put in a job carrying those limits is not started, and the refusal
names what the platform reported, including when this process is itself in a job the platform will
not nest them under. When this process is itself in a job, as it is under a terminal or a CI
runner that puts its children in one, the stage is created in a job of its own that holds every
limit above, or it is not started. `a_stage_is_limited_whether_or_not_this_process_is_in_a_job`
checks both: it reads the job of a stage started from the test process as it is, and again from a
copy of the test binary that has put itself in a job. The stage is created under the gate every start of a program takes
([RPCVIEW-8](session-view.md#RPCVIEW-8)), so a program started at the same moment holds none of its
streams, and it inherits the three standard streams it is given and no other handle this process
holds. The stage is given a console of its own with no
window rather than the one this process runs in, since any process attached to a console can write
input into it, which on the terminal's console is typing an answer to a prompt. The programs it
starts share that console, so none of them opens a window.

**Why.** A profile that is applied when it can be and skipped when it cannot is the silent
degradation [SANDBOX-1](#SANDBOX-1) exists to forbid, and the person who endorsed a line believes it
ran under what the plan accounts for. A switch that turns confinement off is one a person reaches for
after one refusal and forgets, so it is a named mode that the opening line of every session it
governs reports ([SANDBOX-22](#SANDBOX-22)), and it is not one a checkout can set. Starting the
process already confined, through a command the caller spawns or, on Windows, through the call
that creates it, keeps the stage's pipes its own, so the executor's cancellation and job handling
are unchanged.

**What it costs.** On Windows a program argument that names a path outside the directories the
session was opened on is refused by the kernel, since only redirections are opened by this process
on the stage's behalf: a `cat ~/notes.txt` fails, and a person adds the directory
([trust-map.md](trust-map.md)). A Windows stage's console is one nobody can type into, so a program
that asks its question on the console rather than on its input waits until the line is cancelled
or runs out of time, and the console host the platform adds did not stop a stage whose policy denies
subprocesses from starting under a limit of one active process: a `cmd.exe` built-in ran to exit 0
there in the `Sandbox on Windows` job (see Known costs). The console's code
page is the system's rather than one a person set with
`chcp` in the terminal bravebot runs in. On Linux and macOS a stage reads the machine except the credential
table ([SANDBOX-12](#SANDBOX-12)), and writes only the session's directories, so what it costs is a
write outside them and a read of a credential location its plan does not carry. On Linux each
stage start lists every directory above a refused location, and the time that takes grows with the
number of entries in those directories, the home directory among them. On macOS the refusal of a `.git` write is lifted only for a stage
whose program is git ([SANDBOX-14](#SANDBOX-14), [SANDBOX-18](#SANDBOX-18)), which gives that
stage the reach Linux gives every stage.

`verified-by: bravebot_agent::confine::a_confined_program_cannot_read_a_file_outside_the_session`
`verified-by: bravebot_agent::confine::a_confined_program_reads_the_machine_and_not_a_credential_location`
`verified-by: bravebot_agent::confine::a_file_created_between_two_stages_is_readable_by_the_second`
`verified-by: bravebot_agent::confine::a_link_to_a_credential_is_judged_by_where_it_leads`
`verified-by: bravebot_agent::confine::a_confined_program_reads_and_writes_inside_the_session`
`verified-by: bravebot_agent::confine::a_confined_program_cannot_write_outside_the_session`
`verified-by: bravebot_agent::confine::a_program_left_running_is_confined_as_well`
`verified-by: bravebot_agent::confine::a_stage_that_cannot_be_confined_is_refused_with_no_stage_left_running`
`verified-by: bravebot_agent::confine::a_job_with_a_stage_that_cannot_be_confined_is_refused_with_no_stage_left_running`
`verified-by: bravebot_sandbox::windows::the_job_carries_every_ui_limit_whatever_the_policy`
`verified-by: bravebot_sandbox::windows::a_job_that_cannot_be_limited_refuses_the_stage_naming_the_cause`
`verified-by: bravebot_sandbox::windows::the_job_bounds_processes_only_where_the_policy_denies_subprocesses`
`verified-by: bravebot_sandbox::windows::a_stage_is_in_a_job_carrying_every_ui_limit`
`verified-by: bravebot_sandbox::windows::a_stage_is_in_a_job_bounding_processes_only_where_its_policy_denies_subprocesses`
`verified-by: bravebot_sandbox::windows::a_stage_cannot_start_a_child_where_its_policy_denies_subprocesses`
`verified-by: bravebot_sandbox::windows::a_stage_starts_a_child_where_its_policy_allows_subprocesses`
`verified-by: bravebot_sandbox::windows::a_stage_starting_no_child_runs_under_a_policy_that_denies_subprocesses`
`verified-by: bravebot_sandbox::windows::a_stage_is_limited_whether_or_not_this_process_is_in_a_job`
`verified-by: bravebot_agent::turn::a_delegate_of_a_confining_turn_cannot_write_outside_the_session`
`verified-by: bravebot_agent::tools::a_turn_that_confines_runs_is_confined_to_its_workspace_and_a_turn_that_does_not_is_not`
`verified-by: bravebot_sandbox::linux::a_command_handed_back_is_confined_when_the_caller_spawns_it`
`verified-by: bravebot_sandbox::linux::a_confined_process_leads_a_session_of_its_own`
`verified-by: bravebot_sandbox::macos::a_command_handed_back_is_confined_when_the_caller_spawns_it`
`verified-by: bravebot_sandbox::windows::a_batch_file_is_refused_and_an_executable_is_not`
`verified-by: bravebot_sandbox::windows::killing_a_stage_ends_the_programs_it_started`
`verified-by: bravebot_sandbox::windows::a_stage_let_go_of_while_running_leaves_no_program_behind`
`verified-by: bravebot_sandbox::windows::killing_the_process_holding_a_stage_ends_every_program_in_it`
`verified-by: bravebot_sandbox::windows::a_holder_killed_while_it_holds_a_stage_leaves_its_container_profile_registered`
`verified-by: bravebot_sandbox::windows::a_stage_inherits_no_handle_but_the_streams_it_is_given`
`verified-by: bravebot_sandbox::windows::a_stage_start_waits_for_a_channel_being_created`
`verified-by: bravebot_sandbox::windows::a_stage_is_not_attached_to_the_console_this_process_runs_in`
`verified-by: bravebot_agent::confine::variable_names_match_without_case_only_when_folding`

<a id="SANDBOX-18"></a>
### SANDBOX-18: the profile a stage runs under is composed from the plan and the session's directories

On Linux and macOS a stage of a session that names a home directory has as its policy the run base
([SANDBOX-12](#SANDBOX-12)), with `.git` writes allowed where the stage's program is git (below),
plus: the cache rows of every toolchain, written, since the machine is already read and a
cache is where any build may write ([SANDBOX-15](#SANDBOX-15)), each judged where its name leads
as each stage starts and opened on Linux without following a link in its last component, so a
cache relinked to `~/.ssh` or a startup file gets no row; the credential scope its argv names
([SANDBOX-16](#SANDBOX-16)); each directory the session was opened on, to read and write; the
session's scratch directory, to read and write; and the plan's directory as the place it starts.
No row names the directories its program is read from or the two files the step names, since the
machine is read. A local MCP server, which is not a stage, also reads the installation above its
program's `bin` directory when that sits deeper in the home ([SERVERS-10](mcp-servers.md#SERVERS-10)).
Where the stage carries the remote scope, the socket `SSH_AUTH_SOCK` names in its
own environment is a write row ([SANDBOX-3](#SANDBOX-3)), and the row is the path the name leads
to once its links are followed, so a name is granted as the file it leads to when the row is made. The row is made only where that path is a socket, is not or inside a credential
location or a persistence location, and passes the refusals a path in a variable gets (not
`~/.ssh` or the state directory, and not the home directory or above it). Where it is refused the
stage still starts, with no row, and its profile line says no agent was lent. The same judgement
is made for the signing scope and for a requested scope that lends the agent, and for the sentence
that says whether the agent was lent, and is made as each stage starts. A name relinked after
that check and before the sandbox opens the row is not caught. A session that names no home
directory has no location to compare with, so it lends no agent socket. Nothing else is in it: no write outside
those, no credential location the stage's scope does not name, and no socket for a stage without
the remote scope.

On Windows, and on Linux and macOS in a session that names no home directory, since the credential
rows are rows under it and a read of the whole machine with none to subtract is the one grant that
must not be made, a stage's policy is the keyed base ([SANDBOX-12](#SANDBOX-12)), with `.git`
writes allowed where the stage's program is git, plus:
the toolchain list its resolved binary brings ([SANDBOX-15](#SANDBOX-15)); the credential scope its
argv names ([SANDBOX-16](#SANDBOX-16)); the directories its program is read from, which are the
directories on the `PATH` it starts with and the directory it resolved into, each with its links
followed, the parent of a `bin` directory among them, and none that is the home directory, above it
or a parent inside it; the two files the step names as read, the one it was started as and the one
it resolved to; each directory the session was opened on, to read and write; the session's scratch
directory, to read and write; and the plan's directory as the place it starts. On Windows a read row
under a directory every container already reads, `C:\Windows` and the Program Files directories
(bar the `Temp` and `WindowsApps` directories directly under them, which keep their own lists),
is not written, since the entry is one only an administrator can add and the container has the
access without it; a write row there is written, and fails for an account that cannot. A drive path is
named without the `\\?\` prefix a canonical form carries there, so it compares with the rows, unless
the plain spelling would be 260 characters or more. Where the stage
carries the remote scope, the socket `SSH_AUTH_SOCK` names in its own environment is a write row,
since a socket is reached through a write ([SANDBOX-3](#SANDBOX-3)), judged as on Linux and macOS
except that the path is not checked to be a socket, since the agent there can be a named pipe, and
that a session that names no home directory lends no agent socket, having no location to compare
the path with. Nothing else is in it: no
credential directory, no directory above a session directory, and no socket for a stage without the
remote scope.

On Windows a stage is also refused when a directory the session was opened on is, holds or lies
inside one of the credential locations under the home directory ([SANDBOX-12](#SANDBOX-12)): the
state directory `~/.bravebot`, which holds `settings.json`, `~/.ssh`, `~/.aws` and the rest of the
table, which on Windows also holds `gcloud`, `gpg`, the browser profiles and the platform's own
stores under `~/AppData`, so a session opened on `~/AppData/Local` is refused. The row would be a
read and write grant, and a Windows container cannot be refused a
path inside a grant, so the refusal is made before the policy is built and no grant is written. The
paths are compared by component without regard to case, and a credential location is compared in
the form it is named and in its resolved form. A session opened on a project directory is not
affected.

On Linux and macOS a stage is refused when a directory the session was opened on is or lies inside a
credential location, and not when it holds one. On macOS a row at a refused path lifts the refusal,
because an allow follows a deny at the same depth, so the directory `~/.ssh` as a session directory
would let every program the stage starts read and write the keys. The refusal on those platforms
covers the table of [SANDBOX-12](#SANDBOX-12) and, on macOS, `/Library/Keychains`. Names are
compared without regard to case on macOS and exactly on Linux. A directory above a location, the
home directory among them, is granted and the location inside it stays refused. On macOS Seatbelt's
refusal of a read is not a refusal of a write, so each location the stage is refused reading that
lies inside a directory it may write is refused writing too, as is each place under the home
directory where a write becomes code that runs later outside the profile: the shell startup and
logout files `~/.zshenv`, `~/.zprofile`, `~/.zshrc`, `~/.zlogin`, `~/.zlogout`, `~/.bash_profile`,
`~/.bash_login`, `~/.bashrc`, `~/.bash_aliases` (which Debian's `~/.bashrc` reads),
`~/.bash_logout` and `~/.profile`, the directory `~/.config/fish`, the files an X session reads as
it starts, `~/.xprofile` and `~/.xsessionrc`, the per-user job directories `~/Library/LaunchAgents`,
`~/.config/systemd/user` and `~/.config/autostart`, the directory `~/.config/environment.d` whose
variables systemd sets for every program of the account, and git's own configuration `~/.gitconfig`
and `~/.config/git/config`. These persistence locations are named at those fixed paths, so a
`ZDOTDIR` or an `XDG_CONFIG_HOME` that moves one leaves the file it moved it to unrefused. They
are refused for writing only and are read as the rest of the home directory is. A session
opened on the home directory or on a directory above it therefore writes beneath it except the
credential locations and the persistence locations, and a session opened on a directory that holds
none of them has nothing to refuse. `/add-dir`,
`--add-dir` and `additionalDirectories` refuse the same directories before they are added, so a
person is not shown a session directory the stage would be refused for.

A stage is refused, on every platform that has a scratch directory, once the name of the session's
scratch directory leads to a link or a file. Every stage may write the temporary directory the
scratch directory is made in, so the name can be made to lead somewhere else, and a row written from
it then grants wherever it leads. The session records the device and inode of the directory it
locked as it made it ([TRUST-14](trust-map.md#TRUST-14)), and the scratch directory is a row only
while its name leads to that directory. A name that leads to another directory, or to nothing,
because the directory was removed or made again, is given no row, and the stages that follow still
start. The check is made as each stage starts, so a stage of a line that starts after an earlier
stage of the same line replaced the directory is refused too, and a name already changed when a
line's confinement is built is given no row. The scratch directory is also compared with the
credential locations on the terms a session directory is, and where it is or lies inside one it is
given no row, since the stages would otherwise all be refused by a temporary directory a new session
is given in the same place. Where no identity is recorded, as on Windows, the name is held to being
a directory and not a link, so a directory made in place of the original is given the row. The
refusal lasts for the session, and a new session makes a new scratch directory.

Each directory the session was opened on, the working directory and every directory added with
`/add-dir`, `--add-dir` or `additionalDirectories` or reopened by a resume, is held the same way.
The workspace records its device and inode as it is opened, added or reopened
([TRUST-14](trust-map.md#TRUST-14)), the stage's confinement is built from those records, and a
directory is a row only while its name leads to that directory. Every stage may write the temporary
directory, so a stage can move a session directory that lies directly under it and put a link in
its place, and a row written from the name would then grant wherever the link leads. The name is
resolved once, the resolved path is compared with the record, and that same path is the row. A name
that leads to another directory, or to nothing, is given no row, and the stages that follow still
start. A name that leads to a link or a file refuses the stage, and the refusal names the
directory. The check is made as each stage starts, so a later stage of one line is held too, and a
name already changed when a line's confinement is built is given no row. A directory that lies in
or is a credential location is judged as before, by where its name leads. Where no identity is
recorded, as on Windows, the name is held to being a directory and not a link, so a directory made
in place of the original is given the row. The file tools hold the name the workspace stored and
do not follow a link made later. The interval between the check and the sandbox opening the row
remains: a name relinked in it is not caught, as for the scratch directory.

A stage's program is git where the file it resolved to, with its links followed, is named `git`, or
`git.exe` in any case on Windows, and lies beneath none of the rows the stage writes and none of the
checkouts holding a session directory or a directory the session left ([SANDBOX-20](#SANDBOX-20)
says which). The rows the mode bypass gives without a request
([SANDBOX-28](#SANDBOX-28)) are left out of that comparison, since they are the whole machine and
would leave no git anywhere. A script, `make`, `npm`, `gh` or any other program is not git, and git
started by one runs under that stage's profile. The lift changes a profile only on macOS, the one
platform that holds a `.git` back ([SANDBOX-14](#SANDBOX-14)).

Every input is the compiled step a person read, the session's own directories or the process's own
environment. Nothing a program printed, and no value the model supplied beyond the plan, reaches a
row. Rows that name an absent path are created or left out as [SANDBOX-9](#SANDBOX-9) and
[SANDBOX-11](#SANDBOX-11) say, by what the backend can grant.

**Why.** The grant is the plan, so a line nobody is asked about gets the same profile as one
somebody answered. The `.git` hold-back ([SANDBOX-14](#SANDBOX-14)) is lifted for a stage whose
program is git because a stage started in a directory it may write is where a person runs `git
commit`, and a profile that refused it would be one somebody turns off. It is kept for every other
stage because an install script, a `make` target or a test runs code the person did not read, and
a hook it writes, or a `core.fsmonitor`, `core.hooksPath`, `core.sshCommand`, `!` alias or
`credential.helper` it sets in `.git/config`, runs with the person's whole access the next time
they run git there, in a terminal, an editor, or a later stage that carries the remote scope and
the agent socket ([SANDBOX-16](#SANDBOX-16)). The auto mode's classifier is therefore not put a line
with a git stage ([AUTO-3](auto-mode.md#AUTO-3)). The hold-back is kept beneath every write row and not only the
session's directories, because a repository made in the temporary directory or a cache can be moved
into a session directory whole. The program is judged by the file it resolved to and by whether a
stage could have put it there, so a `git` a stage wrote earns nothing by its name. A git stage keeps
the hooks and the configuration too: `git init` and `git clone` write both from the templates, and
`git remote add`, `git push -u` and a checkout that tracks a remote branch write the configuration,
so a refusal there is a git that does not work. A program installed inside the
home is read as the file a person read and not as its directory, so that granting a program does not
open the home. A local MCP server also reads the installation above its program's `bin` directory when
that sits deeper in the home ([SERVERS-10](mcp-servers.md#SERVERS-10)). The persistence locations are refused for writing on macOS because a program started
later by a shell, by launchd, systemd or a desktop session, or by git (`core.fsmonitor`,
`core.pager`, a `!` alias, a credential helper) runs as the account with no profile around it, so a
write there reaches past the stage that made it. The scratch directory is held to the directory the session
made, and not to the credential table alone, because every stage writes the temporary directory and
so the name can be made to lead to any directory the account can reach, most of which are no
credential location. The cost is that a stage of a session opened on the
home directory cannot run `git config --global`.

Every cache is written by every stage on Linux and macOS in the mode `workspace-write` because a stage is as often a script or a
wrapper (`make check`, a `just` recipe) as one program, and the binary it resolves to is not the
build it drives. The cost is that the separation between ecosystems that
[SANDBOX-15](#SANDBOX-15) describes does not hold there: a stage that runs `npm ci` can write
`~/.cargo/registry`, and a later `cargo build` reads what it left. The token files and the installs
stay out of the write rows, so the reach is to caches only. A person who wants each ecosystem held
to its own cache uses the mode `strict` ([SANDBOX-22](#SANDBOX-22)). Under `read-only` no cache is
written.

A cache row is judged where its name leads, so a person who moved `~/.cargo/registry` aside and linked
it to a directory inside a credential location, or to a file, gets no write row for it: the stage
starts and a build that writes that cache fails at the write.

**What the scratch directory check costs.** The check and the start of a stage are two steps. A
program that is already running when another stage starts, such as an earlier stage of a pipeline or
one left running, can change the name between them, and on Linux the row is then opened by the name
the start finds. A name changed before the check is refused, so what remains is a change made in
that interval.

**What the `.git` rule costs.** What git starts runs under the git stage's profile: a hook, a
filter, the command `git bisect run` is given, an editor, a pager and a credential helper may each
write the repository's hooks and configuration, so a pre-commit hook that runs the project's own
linters reaches them, and so does a program the line's own arguments or assignments name for git to
start (`GIT_EDITOR`, `--exec-path`, `rebase -x`). A program that makes or changes a repository
itself is refused the write on macOS: `gh pr checkout`, `gh repo clone`, `npm version`, a `make`
target or a script that commits, `cargo new` outside a repository, a `cargo` fetch of a `git`
dependency, an npm `github:` package, a pip `git+` requirement, SwiftPM's checkouts and a test that
makes a fixture repository fail there. So does git reached through another program, `timeout`, `env` or a version manager's
shim that resolves to that manager. The planner is told to run git as a step of its own
([SANDBOX-19](#SANDBOX-19)), and a person can let one line start with no sandbox
([SANDBOX-29](#SANDBOX-29)). A redirection on the line is opened by this process rather than by the
stage, so `> .git/config` written on the line itself is not refused, except under `read-only`, where
any redirection into a file is ([SANDBOX-22](#SANDBOX-22)). A `core.hooksPath` that points into the work tree makes files every stage writes into hooks,
and a bare repository in a toolchain cache, cargo's under `~/.cargo/git/db` and Go's under its module
cache, has a configuration every stage writes and a later fetch reads. On Linux and Windows nothing holds a `.git` back, so every stage there may
write a repository's hooks and configuration, and the confirmation of a line with a stage whose
program is not git says so wherever that stage writes more than the temporary directory
([PROMPT-2](prompting.md#PROMPT-2)). Landlock holds
a path back from a write row only by listing the directory above it entry by entry, which leaves
that directory without the right to make an entry directly in it: held back from a session
directory, that is every new file at the top of the project, the lockfile `npm install` renames
into place among them, and held back from git's own files it is the `index.lock` every commit
makes. A Windows container refuses a policy that carries a refusal at all.

`verified-by: bravebot_agent::confine::the_session_directories_are_read_and_written_and_nothing_else_of_the_persons`
`verified-by: bravebot_agent::confine::only_a_step_whose_program_is_git_writes_a_git_directory`
`verified-by: bravebot_agent::confine::a_git_above_the_session_directory_in_its_checkout_writes_no_git_directory`
`verified-by: bravebot_agent::confine::only_git_writes_a_repositorys_hooks_and_configuration_on_macos`
`verified-by: bravebot_agent::confine::a_prompt_says_a_program_other_than_git_can_write_a_git_directory_where_none_is_held_back`
`verified-by: bravebot_agent::confine::the_description_follows_the_platform_it_describes`
`verified-by: bravebot_agent::confine::a_step_whose_plan_names_no_credential_reaches_nothing_in_the_home`
`verified-by: bravebot_agent::confine::a_toolchains_cache_is_granted_to_its_own_binary_only`
`verified-by: bravebot_agent::confine::a_stage_writes_only_the_session_the_temporary_directory_and_the_caches`
`verified-by: bravebot_agent::confine::where_the_machine_is_read_every_stage_writes_every_toolchains_cache`
`verified-by: bravebot_agent::confine::a_session_with_no_home_is_not_granted_the_machine`
`verified-by: bravebot_agent::confine::a_windows_program_keeps_its_list_and_scope_with_its_extension`
`verified-by: bravebot_agent::confine::the_keyed_base_reads_the_developer_directory_the_session_resolved`
`verified-by: bravebot_agent::confine::a_macos_session_with_no_home_is_refused_a_directory_in_the_system_keychains`
`verified-by: bravebot_agent::confine::a_push_reaches_the_remote_scope_and_a_status_does_not`
`verified-by: bravebot_agent::confine::the_agent_socket_goes_to_a_remote_step_and_to_no_other`
`verified-by: bravebot_agent::confine::an_agent_socket_name_is_granted_only_where_it_leads_to_a_socket_outside_the_credential_locations`
`verified-by: bravebot_agent::confine::a_session_with_no_home_lends_no_agent_socket`
`verified-by: bravebot_agent::confine::an_assignment_in_front_of_a_push_removes_its_scope`
`verified-by: bravebot_agent::confine::a_program_at_the_top_of_the_home_is_granted_as_a_file_and_not_as_the_home`
`verified-by: bravebot_agent::confine::the_path_outside_the_home_is_read_and_the_homes_own_bin_brings_no_parent`
`verified-by: bravebot_agent::confine::a_program_deep_in_the_home_brings_no_installation_row_to_a_stage_but_does_to_a_server`
`verified-by: bravebot_sandbox::rules::a_verbatim_drive_path_loses_its_prefix_and_nothing_else_does`
`verified-by: bravebot_sandbox::windows::what_every_container_reads_is_the_machines_own_directories`
`verified-by: bravebot_sandbox::windows::nothing_else_is_taken_to_be_readable_by_every_container`
`verified-by: bravebot_sandbox::windows::a_write_grant_under_a_system_directory_still_needs_its_entry`
`verified-by: bravebot_agent::confine::a_windows_session_on_the_home_directory_is_refused_for_the_state_directory`
`verified-by: bravebot_agent::confine::a_windows_session_inside_a_credential_location_is_refused`
`verified-by: bravebot_agent::confine::a_windows_session_on_the_application_data_of_gcloud_or_gpg_is_refused`
`verified-by: bravebot_agent::confine::a_windows_session_on_local_application_data_or_a_browser_profile_is_refused`
`verified-by: bravebot_agent::confine::a_session_inside_the_browser_relays_directory_is_refused_on_every_platform`
`verified-by: bravebot_agent::confine::a_windows_session_is_compared_to_the_credential_locations_without_regard_to_case`
`verified-by: bravebot_agent::confine::a_name_that_is_not_text_is_not_taken_for_its_lossy_lookalike`
`verified-by: bravebot_agent::confine::a_windows_session_beside_the_credential_locations_is_not_refused`
`verified-by: bravebot_agent::confine::a_windows_session_with_one_root_on_the_home_directory_is_refused_whatever_the_others_are`
`verified-by: bravebot_sandbox::base::the_credential_locations_are_the_rows_the_run_base_refuses`
`verified-by: bravebot_agent::confine::a_session_inside_a_credential_location_is_refused_on_every_platform`
`verified-by: bravebot_agent::confine::a_session_beside_or_above_the_credential_locations_is_not_refused_outside_windows`
`verified-by: bravebot_agent::confine::a_macos_session_inside_the_system_keychains_is_refused`
`verified-by: bravebot_agent::confine::a_macos_session_at_or_above_home_cannot_write_where_it_cannot_read`
`verified-by: bravebot_agent::confine::a_scratch_directory_inside_a_credential_location_gets_no_row_on_every_platform`
`verified-by: bravebot_agent::confine::a_stage_is_refused_once_the_scratch_directory_leads_to_a_link_or_a_file`
`verified-by: bravebot_agent::confine::a_scratch_directory_that_is_replaced_or_gone_gets_no_row_and_stops_no_stage`
`verified-by: bravebot_agent::confine::a_scratch_directory_linked_before_the_confinement_is_built_gets_no_row`
`verified-by: bravebot_agent::confine::the_scratch_directory_the_session_made_is_a_row_and_starts_a_stage`
`verified-by: bravebot_agent::confine::a_stage_that_relinks_the_scratch_directory_hands_the_next_stage_nothing`
`verified-by: bravebot_agent::confine::a_line_that_relinks_the_scratch_directory_is_refused_at_the_stage_after`
`verified-by: bravebot_agent::confine::a_scratch_directory_made_again_by_a_program_does_not_stop_the_stages_after`
`verified-by: bravebot_agent::tools::a_turns_confinement_holds_the_scratch_directory_to_the_one_the_session_made`
`verified-by: bravebot_agent::confine::a_session_directory_whose_name_leads_elsewhere_gets_no_row`
`verified-by: bravebot_agent::confine::a_session_directory_as_recorded_is_a_row_and_starts_a_stage`
`verified-by: bravebot_agent::confine::a_working_directory_that_leads_elsewhere_is_not_replaced_by_an_added_one_as_the_place`
`verified-by: bravebot_agent::workspace::the_session_directories_are_the_directories_recorded_as_they_were_opened`
`verified-by: bravebot_agent::workspace::a_delegates_checkout_records_its_own_root_and_carries_its_parents_records`
`verified-by: bravebot_agent::confine::a_stage_that_relinks_an_added_directory_hands_the_next_stage_nothing`
`verified-by: bravebot_agent::confine::a_line_that_relinks_an_added_directory_is_refused_at_the_stage_after`
`verified-by: bravebot_agent::confine::a_line_that_relinks_the_primary_root_is_refused_at_the_stage_after`
`verified-by: bravebot_agent::confine::an_added_directory_made_again_by_a_program_does_not_stop_the_stages_after`
`verified-by: bravebot_agent::tools::a_turns_confinement_holds_each_session_directory_to_the_one_the_workspace_recorded`
`verified-by: bravebot_tui::sessions::a_resumed_session_holds_the_directory_it_reopened_to_the_directory_it_reopened`
`verified-by: bravebot_agent::confine::a_linux_session_is_compared_to_the_credential_locations_with_case`
`verified-by: bravebot_agent::workspace::a_directory_at_or_inside_a_credential_location_is_refused`
`verified-by: bravebot_sandbox::rules::a_directory_at_or_inside_a_credential_location_is_inside_one_and_its_neighbours_are_not`
`verified-by: bravebot_sandbox::rules::a_credential_location_is_compared_where_it_and_the_home_lead`
`verified-by: bravebot_sandbox::rules::a_credential_location_under_a_home_that_is_not_text_is_not_matched_by_its_lookalike`
`verified-by: bravebot_sandbox::toolchain::a_cache_that_has_become_a_link_to_somewhere_refused_is_given_no_row`
`verified-by: bravebot_sandbox::toolchain::a_cache_that_is_the_other_kind_of_thing_is_given_no_row`
`verified-by: bravebot_sandbox::toolchain::a_cache_that_is_a_link_to_nowhere_is_given_no_row`
`verified-by: bravebot_sandbox::toolchain::a_cache_that_is_a_link_to_the_other_kind_of_thing_is_given_no_row`
`verified-by: bravebot_sandbox::toolchain::a_cache_linked_to_somewhere_ordinary_is_granted_there_and_held_to_that_name`
`verified-by: bravebot_agent::confine::a_cache_relinked_to_a_credential_or_a_startup_file_is_given_no_row`
`verified-by: bravebot_agent::confine::a_cache_relinked_between_two_stages_is_judged_again_on_every_route_to_its_row`
`verified-by: bravebot_sandbox::policy::a_row_marked_as_judged_stays_marked_through_the_resolution`
`verified-by: bravebot_sandbox::policy::a_judged_row_above_a_refusal_that_became_a_link_is_an_error_and_one_not_judged_is_listed`
`verified-by: bravebot_sandbox::linux::a_judged_row_that_became_a_link_is_refused_and_one_not_judged_is_followed`
`verified-by: bravebot_sandbox::linux::a_cache_moved_aside_and_linked_to_the_keys_by_one_stage_reaches_nothing_in_the_next`

<a id="SANDBOX-19"></a>
### SANDBOX-19: the planner is told its programs are confined, and a failed step says what it ran under

On a turn that confines `run` ([SANDBOX-17](#SANDBOX-17)) on Linux and macOS, the `run` tool's
description says that the programs it starts may read the machine except the places that hold a
credential, may write only the session's directories, the scratch directory, the temporary
directory and the toolchain caches, and read a credential location only where the command's scope
names it. The sentence after a failed step names the same, and the confirmation's heading names it, with the known-hosts sentence for the remote scope saying what it adds and not what
the stage already read. On Windows the description says that the programs it starts are confined:
to the directories the session was opened on, the scratch
directory and the temporary directory, the system and program directories and git's configuration
files for reading, the caches of the toolchain a program belongs to and the credential scope its
command names. It says that a path outside those is refused by the operating system as `Operation
not permitted` or `Permission denied`, and that only the person widens the reach (`/add-dir`,
`--add-dir`). On macOS, in every mode that confines, the description and the sentence after a failed
step also say that a step whose program is not git cannot write any `.git`, so a program that runs
git to commit, check out or clone is refused that write and git is run as a step of its own
([SANDBOX-18](#SANDBOX-18)); under `read-only` they say so of the places that mode writes, such as a
clone in the temporary directory, since a git step cannot write the session's `.git` either. On a platform with no base the description says programs are not confined. Under the mode `strict`
([SANDBOX-22](#SANDBOX-22)) it gives the Windows form's account of what is reachable, since the
machine is not read there either. Under the mode `read-only` it says that programs cannot write the
project, and that outside the paths the person's own write grants name, a build, a formatter or
`git commit` that writes the directories the session was opened on fails whichever step runs it,
git on its own included. It names what they write: the temporary directory, the paths the person's
own `allowWrite` entries ([SANDBOX-25](#SANDBOX-25)) and remembered write grants
([SANDBOX-23](#SANDBOX-23)) name, and `~/.ssh/known_hosts` for a command with the remote scope. It
says that only the person widens it: that `/add-dir` and `--add-dir` add a directory to read, and
that choosing another sandbox mode lets programs write the project. It also says that the file tools
write nothing in that mode and that a line redirecting output into a file is refused. On a turn
that does not confine runs, and under the mode `off`, it states no boundary, since a planner told of a boundary its
programs do not have stops reaching for paths they can reach. It says one sentence there instead: "The
`request_path` tool is refused in this session and nobody is asked, so do not call it." That sentence is said only where the turn's list holds `request_path`; a list without it gets no sentence ([TOOL-6](tools/tool-surface.md#TOOL-6)). The words are
the same in every permission mode, in a trusted workspace and in one that is not, because the tool
is refused in all of them ([PATHREQ-5](tools/request-path.md#PATHREQ-5)). They say nothing of the file tools, which
[PATHREQ-7](tools/request-path.md#PATHREQ-7) lets reach outside the workspace under bypass, and nothing of `scopes` or
`unconfined`, which are accepted and dropped there ([SANDBOX-26](#SANDBOX-26)).

A result whose step did not exit zero, from a run or from a job left running, carries one more
sentence from the driver after the account of how it ended ([RUN-13](tools/run.md#RUN-13)): the
directories the programs could read and write, that beyond those they reached only what a
toolchain list or credential scope added for the steps that named one, the toolchain lists the plan
brought by name and the credential scopes it brought by name, or `none`, and whether the network
was open or closed and, if closed, the reasons that kept it for the steps that had one
([SANDBOX-20](#SANDBOX-20)). Under `read-only` it names the temporary directory and the paths the
person's own `allowWrite` entries and remembered write grants name as what the programs could write,
and says they could write nothing else of the directories the session was opened on or the toolchain
caches. Where it lists the directories reachable, because the machine is not read, it names the
session's directories and the scratch directory as read. It also says what the description says of a
build, a formatter and `git commit`. Under the modes `strict`, `read-only` and `workspace-write`, where the session has a profile directory
and would accept a request, the description and this sentence also name the menu a `run` call may ask
from ([SANDBOX-26](#SANDBOX-26)). Under `off`, and on a turn that confines no run, the description names no menu and says only the sentence above, and the sentence after a failed step is not written. A session
accepts a request only in a workspace the person has trusted ([TRUST-7](trust-map.md#TRUST-7),
[PATHREQ-5](tools/request-path.md#PATHREQ-5)), so in any other the description and this sentence
name neither the menu nor `request_path`, and on macOS do not tell the planner to ask for
`loopback`. They say instead that `run` accepts no `scopes` argument
and, where the list holds `request_path`, that `request_path` is refused, that nobody is asked, and that the person can trust the workspace or
add a directory with `/add-dir` or `--add-dir`, a directory to read under `read-only`. Under `off`, or
on a turn that confines no run, an untrusted workspace gets the sentence above and nothing more, so the
two never say different things. The description
reads that fact as each turn starts and this sentence reads it when the result is composed. The
sentence is the same words for exit 1 and exit 2.
A result whose steps all
exited zero carries no such sentence. The sentence is composed from the same two decisions the policy is
([SANDBOX-18](#SANDBOX-18)), so the two cannot name different lists.

The sentence ends with fixed text, in every mode that produces it, saying that a credential
location such as `~/.ssh`, `~/.aws` or `~/.kube` cannot be added with `/add-dir` or `--add-dir`,
and that where the mode accepts a request, a credential scope is how a line reaches one. It then
says that any other path a command needs is asked for with the `request_path` tool
([SANDBOX-28](#SANDBOX-28)), the person is asked, and a yes lasts for the session. Under
`read-only` it says that of a path a command needs to read, and that a request for a path to write
is refused in that mode before anyone is asked ([SANDBOX-22](#SANDBOX-22)). On a turn whose list
lacks `request_path` the text stops after the sentence on credential scopes
([TOOL-6](tools/tool-surface.md#TOOL-6)). A planner
that meets a refusal on such a location otherwise has no other account of it, and asks the person
for an `/add-dir` that does nothing. The text is the same for every failure and for exit 1 and
exit 2. The menu sentence ends the same way, and the `run` description's
confinement statement names the tool. Where the session would accept no request, the fixed text
says that a credential location cannot be added with `/add-dir` or `--add-dir` and then says what
the description says of the routes the person has, in place of the credential scope and
`request_path` clauses.

Where a step of the line is `git` carrying the remote scope by its argv, the sentence also says,
before that fixed text, which key such a step offered over ssh ([SANDBOX-16](#SANDBOX-16)): first
the one the person's `git.identities` names for the owner of the ssh address its argv named, then
only keys their ssh configuration names; first the one the person chose for that owner at a run
prompt, then only those; whichever key ssh tried first, as the person chose for that owner at a run
prompt; whichever key ssh tried first, since neither an entry nor a choice names a readable public
key for that owner; or no key from `git.identities`, since the step named its remote by name or by
an address that is not ssh, and over ssh whichever key ssh tried first. Each case present in the
line is said once. It then says whether the session's ssh agent socket was lent to the step, which
it was where the session has one, how many public key files the step could read among those ssh
looks for by default and those the person's ssh configuration names, and how many files an
`IdentityFile` line names that it could not read. It then says that where GitHub refused a push for permission the key offered may
belong to an account that cannot write to the repository, and where it refused the login no key
offered is registered, that a retry offers the same key, and that only the person chooses the key:
at the run prompt of the terminal interface, where ssh could offer from two to nine keys and an
owner the step names has no entry or choice, or, for one already chosen, where the planner's retry
names `ssh-key` in `scopes`, or with the entry shape and the file it goes in. Where no run prompt
will list keys, because ssh could offer fewer than two or more than nine, the settings entry is the
only way it names. Where the session would accept an `unconfined` request, it ends by saying the
push can be run as its own line with `'unconfined': true`, which the person is asked to approve and
which runs with no sandbox, on the condition that applies to the signing sentence. It says that in the terminal
interface `/reach keys` lists the keys chosen at a prompt and `/reach keys remove <number>` forgets
one, so that the next prompt asks again, and that in either case the planner asks the person rather
than retrying. The text names no host, owner, address or key path and reads nothing a
program printed, so the same words follow a rejected push and a typo, which is why each part of it
says the case it is about. A line with no such step says none of it.

On macOS, where the session would accept an `unconfined` request, a failed result for a line with a
step that runs `open` or `osascript` also says that the default profile refuses LaunchServices and
Apple Events, so launching an application, opening a URL in one or scripting one needs the line run
again with `'unconfined': true`, naming the refusal in `why`. The step is chosen by the file its
program resolved to, so a different program given the name `open` does not earn it and `open`
reached under another name does. The sentence is not said on Linux or Windows, and not where the
session would refuse the request.

On macOS, where the session would accept a request, the description also says that a program
cannot listen on a port of this machine unless its line asks for `loopback`, so `cargo test` over
tests that bind `127.0.0.1:0` and a build through `sccache` fail with `Operation not permitted`
without it, and that `loopback` is asked for to connect to a port of this machine only from a line
that has no network access, since it adds nothing to a connect from a line that has network access
([SANDBOX-3](#SANDBOX-3)). Where the network is closed, and under `read-only`, which closes it, the
description adds that a line that asks for `loopback` may still connect to a port of this machine.
A session that would accept no request is told none of this. On Linux and Windows the description
does not say when to ask for `loopback` and names it only in the list of names a line may ask for,
and the `scopes` entry of the tool says that it changes nothing there
([SANDBOX-26](#SANDBOX-26)). The sentence after a failed step says what a step that asked for
`loopback` could do, in its account of the network. On macOS with the network closed and no host
list it could listen on and connect to this machine's own ports. On macOS with the network closed
and a host list it could listen on them, and connect to them if it has no network access, since a
host list holds only the steps that have network access to the proxy's port. On macOS with the
network open it could listen on them: without a host list a step connects without the request, and
with one every step is held to the proxy's port. On Linux and Windows the sentence says nothing of
`loopback`.

On macOS, a failed result for a line with a step that runs `pgrep`, `pkill`, `killall`, `lsof` or
`ps`, from a run or from a job left running, also says that the sandbox refuses listing processes,
so those programs fail whether or not the program they look for is running, that such a failure is
not an answer about the program, and that a local service is found by connecting to its port, such
as `curl http://127.0.0.1:9222/json/version` for a browser started with
`--remote-debugging-port=9222` ([SANDBOX-8](#SANDBOX-8)). It does not name the rule of the profile
that refuses each program, since they differ. Where the session would accept a request it ends with
the rule above about when `loopback` is asked for to connect. Where it would not, the sentence says
to connect to the port and does not tell the planner to ask for `loopback`. The step is chosen by
the file its program resolved to, as for `open`, so a different program given the name `pgrep` does
not earn it and `pgrep` reached under another name does. The words are the same for any exit status
and whatever the step printed, and are said once for a line with several such steps. They are not
said on Linux or Windows, in a result whose steps all exited zero, under the mode `off`, or on a
turn that confines no run.

**Why.** A refused step reports `exited 1` and its standard error is quarantined, so the planner
cannot see that the sandbox refused it. Without this sentence it cannot tell a sandbox refusal from
a fault in the machine, so it chases causes that do not apply and sends the person to run
diagnostics that cannot show the boundary. A refused process listing is the same: `pgrep` exits 3
under the profile whether or not the program it looks for is running, so a planner without the
sentence takes the failure for an answer and asks the person about a program that is running. A
description that says `loopback` lets a line connect to a port of this machine can lead the planner
to ask for it on a line that only connects to one, and each such line is put to the person
([SANDBOX-26](#SANDBOX-26)).

**It branches on no output.** The inputs are the session's directories, the compiled step and the
exit status the result already reports ([RUN-13](tools/run.md#RUN-13)). Nothing a program printed
reaches the sentence, and it names no path a program chose, so the same sentence follows a refusal,
a failing test and a typo. It adds no branch on the exit status beyond the one that already
chooses `It failed` over `It exited 0`. The sentences about `loopback` and about listing processes
are chosen from the platform, the network, whether a host list holds the stages that have network
access, the menu names a step asked for and the file a step resolved to, and are the same words for
every exit status. They name no tool, since a delegate and a turn a person addressed to a definition
read them and are offered fewer tools than the lead ([TOOL-6](tools/tool-surface.md#TOOL-6)).

`verified-by: bravebot_agent::confine::a_confining_turn_tells_the_planner_what_a_refusal_means`
`verified-by: bravebot_agent::confine::a_turn_that_does_not_confine_states_no_boundary_in_the_description`
`verified-by: bravebot_agent::confine::a_platform_with_no_base_says_its_programs_are_not_confined`
`verified-by: bravebot_agent::confine::the_profile_line_names_the_session_directories_the_lists_and_the_scope`
`verified-by: bravebot_agent::confine::a_step_with_no_list_and_no_scope_says_none_for_both`
`verified-by: bravebot_agent::confine::the_profile_line_is_the_same_whatever_paths_the_step_was_given`
`verified-by: bravebot_agent::confine::the_profile_line_agrees_with_the_policy_on_the_lists_and_the_scope`
`verified-by: bravebot_agent::confine::the_profile_line_ends_with_the_fixed_sentence_about_credential_locations`
`verified-by: bravebot_agent::turn::a_failed_run_says_a_credential_location_cannot_be_added_whatever_the_mode_and_exit`
`verified-by: bravebot_agent::turn::a_run_that_worked_or_was_not_confined_does_not_say_a_credential_location_cannot_be_added`
`verified-by: bravebot_agent::tools::the_confinement_statement_is_appended_to_run_on_a_confining_turn_only`
`verified-by: bravebot_agent::turn::a_failed_run_on_a_confining_turn_says_what_it_ran_under`
`verified-by: bravebot_agent::confine::a_path_the_person_let_programs_reach_is_held_and_their_own_deny_still_applies`
`verified-by: bravebot_agent::turn::a_run_that_succeeded_on_a_confining_turn_carries_no_profile_line`
`verified-by: bravebot_agent::turn::a_turn_that_does_not_confine_runs_states_no_boundary_in_the_description_or_a_failure`
`verified-by: bravebot_agent::turn::a_failed_job_on_a_confining_turn_says_what_it_ran_under`
`verified-by: bravebot_agent::tools::the_statement_follows_the_sandbox_mode`
`verified-by: bravebot_agent::confine::a_strict_stage_does_not_read_the_machine`
`verified-by: bravebot_agent::confine::the_failure_sentence_names_the_menu_where_a_request_is_accepted`
`verified-by: bravebot_agent::confine::the_planner_is_told_of_the_menu_in_every_mode_but_off`
`verified-by: bravebot_agent::confine::the_description_says_request_path_is_refused_under_off_and_where_runs_are_not_confined`
`verified-by: bravebot_agent::confine::the_description_says_nothing_of_the_refusal_where_requests_are_accepted`
`verified-by: bravebot_agent::turn::the_run_description_says_request_path_is_refused_under_off`
`verified-by: bravebot_agent::turn::the_run_description_says_request_path_is_refused_where_runs_are_not_confined`
`verified-by: bravebot_agent::confine::the_planner_is_told_a_read_only_stage_cannot_write_the_project`
`verified-by: bravebot_agent::confine::under_read_only_the_description_offers_no_remedy_that_cannot_write`
`verified-by: bravebot_agent::confine::under_read_only_the_failure_sentence_claims_no_write_and_offers_no_remedy_that_cannot_write`
`verified-by: bravebot_agent::confine::a_session_that_accepts_no_request_is_told_of_no_menu_and_no_request_path_after_a_failure`
`verified-by: bravebot_agent::confine::a_session_that_accepts_no_request_is_told_of_no_menu_and_no_request_path_in_the_description`
`verified-by: bravebot_agent::turn::in_an_untrusted_workspace_the_description_and_a_failed_run_name_no_menu_and_no_request_path`
`verified-by: bravebot_agent::turn::in_an_untrusted_workspace_a_failed_job_names_no_request_path`
`verified-by: bravebot_agent::tools::the_confinement_statement_names_request_path_only_where_the_list_holds_it`
`verified-by: bravebot_agent::confine::no_sentence_names_request_path_where_the_list_lacks_it`
`verified-by: bravebot_agent::confine::the_profile_line_names_request_path_only_where_the_list_holds_it`
`verified-by: bravebot_agent::turn::a_turn_not_offered_request_path_never_names_it`
`verified-by: bravebot_agent::turn::a_run_under_read_only_reads_the_machine_and_cannot_write_the_session`
`verified-by: bravebot_agent::confine::the_planner_is_told_on_macos_that_only_git_writes_a_git_directory`
`verified-by: bravebot_agent::confine::a_failed_open_or_osascript_step_on_macos_names_the_unconfined_line_only_where_it_is_accepted`
`verified-by: bravebot_agent::turn::the_failure_sentence_is_the_same_for_exit_1_and_exit_2_and_names_the_menu`
`verified-by: bravebot_agent::turn::the_failure_sentence_names_the_menu_under_workspace_write`
`verified-by: bravebot_agent::confine::a_failed_git_remote_step_says_which_key_it_offered_and_that_the_person_chooses_it`
`verified-by: bravebot_agent::confine::a_failed_step_says_where_the_key_was_chosen_at_a_prompt`
`verified-by: bravebot_agent::confine::a_failed_push_says_whether_an_agent_was_lent_which_keys_were_readable_and_the_way_out`
`verified-by: bravebot_agent::confine::a_failed_push_promises_a_key_prompt_only_where_one_will_be_shown`
`verified-by: bravebot_agent::confine::the_planner_is_told_on_macos_that_listening_needs_loopback`
`verified-by: bravebot_agent::confine::the_planner_is_told_of_loopback_only_what_the_sandbox_does_on_its_platform`
`verified-by: bravebot_agent::confine::a_failed_process_listing_step_on_macos_says_the_sandbox_refused_it_and_to_connect_to_the_port`
`verified-by: bravebot_agent::turn::a_real_pgrep_refused_by_the_macos_profile_is_told_to_be_the_sandbox_and_to_probe_the_port`
`verified-by: bravebot_agent::turn::the_process_listing_sentence_is_the_same_words_for_any_exit_code`
`verified-by: bravebot_agent::turn::the_process_listing_sentence_is_chosen_from_the_program_and_not_from_what_it_printed`
`verified-by: bravebot_agent::turn::the_process_listing_sentence_follows_a_failure_under_a_profile_and_nothing_else`
`verified-by: bravebot_agent::turn::a_failed_job_that_lists_processes_is_told_what_a_failed_run_of_it_is_told`
`verified-by: bravebot_agent::confine::the_sentences_about_loopback_and_listing_processes_name_no_tool`
`verified-by: bravebot_sandbox::macos::process_listing_programs_fail_under_both_bases_whether_or_not_what_they_seek_runs`

<a id="SANDBOX-20"></a>
### SANDBOX-20: a session may close the network, and a stage keeps it only for a reason it carries

A session carries a network setting, `open` or `closed`, and `open` is the default. Under `open`
every stage keeps the egress the base leaves it ([SANDBOX-12](#SANDBOX-12)). Under `closed` a stage
has none unless it carries one of three reasons: its toolchain list fetches ([SANDBOX-15](#SANDBOX-15)),
it carries a credential scope ([SANDBOX-16](#SANDBOX-16): the remote, cloud, cluster and container
scopes alike, since each lends a credential to something reached over a socket or the network, and
the signing scope for an account that signs with ssh, since the agent socket is reached through the
same egress), or
its program resolved to the file `curl` or `ssh`. Each is read from the compiled step, never from
what a program printed, and none is granted to a file under a directory the plan may write to, which
it could have named `curl` itself. A stage that carries one is also granted the resolver ([SANDBOX-3](#SANDBOX-3)) and keeps the unix socket rule
it already had, so the agent socket still reaches a remote stage. A stage started with `NAME=value`
assignments carries no scope ([SANDBOX-16](#SANDBOX-16)), and so none of the third kind.
A directory the plan may write to is one of the session's directories, the checkout holding each of
them, the scratch directory or the temporary directory, or a row some stage of the session may be
given for writing, whatever that stage's program: each toolchain's caches, in every mode, since a
file left there outlives a switch to `read-only`; a directory a person remembered for writing
([SANDBOX-23](#SANDBOX-23)); a path a `request_path` for writing was granted
([SANDBOX-28](#SANDBOX-28)); and an `allowWrite` entry ([SANDBOX-25](#SANDBOX-25)), less what a
`denyWrite` or `denyRead` entry takes back. A stage that
runs a file is often not given the row it lies under, and the file counts all the same, since
another stage of the plan could have written it. The checkout is the one
[SANDBOX-30](#SANDBOX-30) finds for a directory: the nearest of the directory and its ancestors with
a `.git`, a file as well as a directory. It counts for each directory the session opened, added or
moved to, and for each it has since closed or left, since closing a directory removes nothing a
stage wrote there. The `bin` of a checkout can be first on `PATH` and hold a file a contributor's
commit added, above the directory the session opened in, so a file there is as much the plan's as
one inside that directory. A checkout that is the account's home directory, or holds it, such as a
dotfiles repository, does not count, since it would take the reason from every program the account
installed under the home, `~/.local/bin` among them. [PERM-14](permissions.md#PERM-14) finds the same
checkout for a `--settings` file and does not leave that one out: a file under a dotfiles repository
at the home directory is the checkout's there, which fails closed (Known costs in
[permissions.md](permissions.md#known-costs)). The checkout is read from the file system each
time a stage is built. The rows the mode bypass gives without a request
([SANDBOX-28](#SANDBOX-28)) are left out, as they are for `git` ([SANDBOX-18](#SANDBOX-18)): they are
every path the account can write, so counting them would take the reason from every program the
account installed, rustup's `cargo` and a Homebrew `git` among them. Under bypass a stage can
therefore still write a file the account owns, name it `curl`, and have a later stage keep the
network by running it.

A local MCP server is not a stage and carries none of these reasons, so a closed session starts it with
no egress, a runner's fetch included, and a platform that cannot deny the network does not start it
([SERVERS-10](mcp-servers.md#SERVERS-10)).

A person's trust of a directory lets a line run there without asking only where no stage of the line keeps the
network for one of the three reasons above ([RUN-5](tools/run.md#RUN-5)). The line is then started with the
network closed for that line alone: under `open` its stages are built as `closed` builds them, which is the
decision above read for one line with no reason carried, and the session's setting is not changed. A line that was
asked about and approved, or answered by another road, has the setting it was approved under.

The setting is read from `--run-network <open|closed>`, from `run.network` in a settings file, and
from a managed pin, in that order of strength: a managed pin is final, the flag outranks a file, and
a file outranks the default. A managed pin that is neither word is read as `closed`, not ignored,
because a mistyped restriction must not open the network. A file in the person's home, or one named by `--settings` from outside
the workspace and outside every checkout the session has been in ([PERM-14](permissions.md#PERM-14)), may set either word. A file a checkout carries, which includes a named file inside
the workspace or such a checkout, may set `closed` and never `open`, and `doctor` says that it was ignored. A word that
is neither is reported and not obeyed. The desktop's bridge takes no flag: it settles the answer once,
when it starts, from the person's home layer, the file `--settings` named, when it exists, the settings file of the
directory it started in and the managed pin, and every window's turns read that answer. A project a
window opens later does not close it, and neither does a settings file a window selects afterwards.

Where the backend cannot deny the network ([SANDBOX-5](#SANDBOX-5): Linux on a kernel that refuses
the namespace [SANDBOX-3](#SANDBOX-3) describes) a stage that would lose egress is not started, and the result says that the network is closed and the platform cannot
deny it, the refusal [SANDBOX-17](#SANDBOX-17) already gives. The planner is told once, in the `run`
description, that the network is closed and which kinds of stage keep it. The failure sentence
([SANDBOX-19](#SANDBOX-19)) names the setting beside the directories. A closed network is named on
the opening screen, in `/status` with the layer that closed it, in `doctor`, in the run prompt once, with each
stage that keeps it named, and in the trail once per run with each such stage by its place
and a fixed reason, and never by a name the plan chose.

**Why.** A profile gates egress as a whole, so with it open a confined program that read a file
inside its grants can send the file anywhere. Most programs a plan names have no use for the
network: `cat`, `grep`, `make`, a test run. Those that do are few and are told apart by what the
step compiled to. Letting a checkout narrow the setting and never widen it is the rule the other
narrowing settings follow: a repository the person did not write cannot give its own programs
reach, but can ask for less of it.

`verified-by: bravebot_sandbox::network::the_setting_is_read_from_exactly_its_two_words`
`verified-by: bravebot_sandbox::network::curl_and_ssh_are_known_by_the_file_and_nothing_else_is`
`verified-by: bravebot_sandbox::toolchain::only_the_programs_that_fetch_are_known_to_fetch`
`verified-by: bravebot_agent::confine::a_program_the_plan_could_have_written_keeps_no_network_by_its_name`
`verified-by: bravebot_agent::confine::a_program_in_a_directory_a_person_let_programs_write_keeps_no_network_by_its_name`
`verified-by: bravebot_agent::confine::a_program_above_the_session_directory_in_its_checkout_keeps_no_network_by_its_name`
`verified-by: bravebot_agent::confine::a_directory_the_session_left_keeps_its_checkout_in_the_set_the_plan_writes`
`verified-by: bravebot_agent::confine::an_outer_repository_or_a_home_that_is_a_repository_is_not_the_checkout_of_a_session_directory`
`verified-by: bravebot_config::run_network::a_managed_pin_that_is_neither_word_closes_the_network`
`verified-by: bravebot_agent::confine::the_policy_the_line_and_the_description_agree_on_which_stages_keep_the_network`
`verified-by: bravebot_agent::confine::the_fetch_bit_is_keyed_on_the_resolved_file`
`verified-by: bravebot_agent::confine::a_closed_network_is_not_reopened_by_what_a_stage_is_started_with`
`verified-by: bravebot_agent::confine::a_closed_network_is_told_to_the_planner_and_an_open_one_is_not`
`verified-by: bravebot_agent::confine::a_backend_that_cannot_deny_the_network_refuses_a_closed_stage`
`verified-by: bravebot_sandbox::linux::a_stage_with_no_network_cannot_connect_to_anything`
`verified-by: bravebot_agent::confine::the_trail_names_the_stages_that_kept_a_closed_network_by_place_and_reason`
`verified-by: bravebot_agent::confine::the_agent_socket_goes_to_a_remote_step_and_to_no_other`
`verified-by: bravebot_agent::confirm::a_closed_network_is_said_once_and_each_stage_that_keeps_it_is_named`
`verified-by: bravebot_config::settings::the_home_layer_may_set_the_network_either_way`
`verified-by: bravebot_config::settings::a_project_layer_may_close_the_network_and_never_open_it`
`verified-by: bravebot_config::settings::the_local_layer_cannot_open_the_network_either`
`verified-by: bravebot_config::settings::a_named_file_speaks_for_the_person_only_outside_the_workspace`
`verified-by: bravebot_config::settings::an_unreadable_network_word_is_reported_and_not_obeyed`
`verified-by: bravebot_config::managed::the_network_is_pinnable`
`verified-by: bravebot_config::run_network::nobody_deciding_leaves_the_network_open`
`verified-by: bravebot_config::run_network::the_flag_beats_the_settings`
`verified-by: bravebot_config::run_network::a_managed_pin_beats_the_flag_and_the_settings`
`verified-by: bravebot_cli::main::the_run_network_flag_is_taken_out_with_its_word_and_refuses_any_other`
`verified-by: bravebot_ui_bridge::run_network::a_closed_network_in_the_settings_reaches_a_windows_turns`
`verified-by: bravebot_ui_bridge::run_network::a_closed_network_in_the_settings_reaches_an_editors_turns`
`verified-by: bravebot_tui::confirm::run::a_run_prompt_says_the_network_is_closed_and_which_stage_keeps_it`
`verified-by: bravebot_tui::logo::a_closed_network_is_named_on_the_opening_screen_and_an_open_one_is_not`
`verified-by: bravebot_tui::status::a_closed_network_is_reported_with_who_closed_it_and_an_open_one_is_not`

<a id="SANDBOX-21"></a>
### SANDBOX-21: the programs people run every day are run under the default, and a change that breaks one fails

A suite runs the everyday workflows under the confinement a `run` stage is given on Linux and macOS
([SANDBOX-17](#SANDBOX-17)), each in a session directory and a scratch home of its own: `git` (init,
add, commit, branch, merge, stash, rebase, worktree, a commit hook, and push, clone, fetch and pull
against a local remote), `gh`, a wrapper script that starts `git` and `make`, `make`, `cargo`
(check, build, run, test, and a build script), `npm` and `node`, `python3`, `go`, `find`, `xargs`
and `find -exec {} +` passing a list of files to one program, `sort` and `uniq` counting lines, `grep`, `sed`, `awk`, `diff` and
`patch`, and `vim` and `less`. A workflow works when every stage exits zero.
The home holds a credential of each kind and has the cache directories of an account that has used
each toolchain, so a first run on an account that never ran the tool is not what is measured
([SANDBOX-15](#SANDBOX-15)). A stage is started as `run` starts it: from the step, with the withheld
names removed from the process environment, and a host variable that
moves a read row (`GH_CONFIG_DIR`, `GIT_CONFIG_GLOBAL`, `XDG_CONFIG_HOME`, `CARGO_HOME`) or names
an agent socket removed. The suite starts its programs as a checked start
([SANDBOX-30](#SANDBOX-30)), so the generic table applies to a stage, and the suite adds the
variables the programs it starts read to choose code: `GIT_TEMPLATE_DIR`, `GIT_EXEC_PATH`,
`GIT_COMMON_DIR`, `MAKEFILES`, `DEVELOPER_DIR`, `GOROOT` and `GOENV` are removed whatever they hold,
and so is `CARGO_HOME` (a host variable that moves a read row, above); `GOENV` names a file that sets
`GOFLAGS`, `GOTOOLCHAIN` and more, and `CARGO_HOME` holds the `config.toml` that names a wrapper, a
linker or a credential provider. `GIT_SSH`,
`GIT_EXTERNAL_DIFF`, `RUSTC`, `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`, `RUSTDOC`,
`CARGO_BUILD_RUSTC`, `CARGO_BUILD_RUSTC_WRAPPER`, `CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER`,
`CARGO_BUILD_RUSTDOC`, `RUSTUP_TOOLCHAIN`, `PKG_CONFIG` and `PYTHONEXECUTABLE` are kept only when they name a
program outside every directory a stage may write, `BROWSER` (the commands `cargo doc --open` runs)
only when each command is one such program, and `RUSTUP_HOME` and `NODE_PATH` only when they
name a directory (`NODE_PATH` a list of them) outside every one; `GIT_SSH_COMMAND`,
`GIT_PROXY_COMMAND`, `GIT_SEQUENCE_EDITOR`, `MAKEFLAGS`, `GNUMAKEFLAGS`, `MFLAGS`, `RUSTFLAGS`,
`CARGO_ENCODED_RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS`, `RUSTDOCFLAGS`, `CARGO_ENCODED_RUSTDOCFLAGS`,
`CARGO_BUILD_RUSTDOCFLAGS`, `CARGO_REGISTRY_CREDENTIAL_PROVIDER`,
`CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS`, `GOFLAGS`, `GOCACHEPROG`, `CC`, `CXX`, `CGO_LDFLAGS`,
`CGO_CFLAGS`, `CGO_CPPFLAGS`, the make variables `MAKE`, `RM`, `AR`, `AS`, `FC`, `PC`, `M2C`, `LEX`,
`YACC`, `LINT`, `TEXI2DVI`, `MAKEINFO`, `GET` and `CO`, and the make flag variables `CFLAGS`,
`CPPFLAGS`, `CXXFLAGS`, `LDFLAGS`, `LDLIBS` and `TARGET_ARCH` are removed, as
are `GIT_CONFIG_KEY_<n>`, `GIT_CONFIG_VALUE_<n>`, `CARGO_TARGET_<triple>_LINKER`,
`CARGO_TARGET_<triple>_RUNNER`, `CARGO_TARGET_<triple>_RUSTFLAGS`,
`CARGO_TARGET_<triple>_RUSTDOCFLAGS`, `CARGO_REGISTRIES_<name>_CREDENTIAL_PROVIDER` and every name that begins `npm_config_` in any case (this includes `npm_config_prefix`). A
`RUSTUP_HOME` that is removed, or that the environment checked does not hold, is set to the account's `.rustup`, since the `cargo` proxy finds
its toolchains there. `PYTHONPATH`, `PYTHONUSERBASE`, `PYTHONHOME`, `PYTHONPYCACHEPREFIX` and
`NODE_OPTIONS` are in the generic table ([SANDBOX-30](#SANDBOX-30)), which a stage is given too.

A session records the directories a stage may write when its first confined stage starts, which is
after the setup stages, so a setup stage's start has none of them. The tests that plant a file under
a variable that names a directory or a program therefore record the directory the helper is in as
one a stage wrote, as the session would. Every such test also runs the program with the variable as
the person set it. Where the helper does not run, the test prints that it skipped, since a helper
the program never reaches through that variable does not make the test pass. `RUSTC_WRAPPER` and `PYTHONPATH` each have a second test that a value
outside every directory a stage may write is kept. `GOENV`, `GOCACHEPROG`, `PKG_CONFIG`, `CXX`,
`CGO_LDFLAGS`, `GOTOOLCHAIN`, `NODE_OPTIONS`, `NODE_PATH`, `BROWSER`, `npm_config_` names, `PYTHONHOME`,
`PYTHONPYCACHEPREFIX`, `PYTHONEXECUTABLE`, `PYTHONSTARTUP`, `MAKEFLAGS` with `-I`, the make rule
variables and linker flags, `CGO_CFLAGS`, `CGO_CPPFLAGS` and the cargo credential providers each have a
test of the same kind against the real program.

The variables below were examined for the programs the suite starts and are not removed, because
planting a file under each and running the program as a stage runs it ran nothing. `xcrun` and the
`/usr/bin` shims did not run a file planted under any value of `TOOLCHAINS`, `SDKROOT` or
`XCODE_DEVELOPER_DIR_PATH` (`SDKROOT` makes `xcrun` fail to read a property list when it names a
directory without one, and `TOOLCHAINS` and `XCODE_DEVELOPER_DIR_PATH` change nothing it finds), while
the same planted file ran under `DEVELOPER_DIR`. `vim -es -u NONE`, which the editor workflow
starts, did not run the command in `VIMINIT`, and `vim` without `-es` did, so a workflow that starts
an interactive `vim` needs `VIMINIT` added. `python3 -c`, `python3 -m unittest` and `python3 -m venv` did not run the
file in `PYTHONSTARTUP`, which only an interactive interpreter reads (`python3 -i` with stdin
closed did, on 3.15.0), and `sh -c` and a shell script
did not run the file in `ENV`, which only an interactive `sh` reads. No workflow starts an
interactive `python3` or `sh`, so none of the three is removed. `GOROOT` is removed, since
`go vet` ran a `vet` planted under it. `GOENV` is removed, since `go vet` ran the program named by a
`GOFLAGS=-toolexec=` line in the file `GOENV` names (go 1.27.3). Planting a file under `GOPATH`,
`GOBIN`, `GOWORK`, `GOPROXY`, `PYTHONBREAKPOINT`,
`npm_config_node_options`, and `GNUMAKEFLAGS` and `MFLAGS` on GNU Make 3.81 ran nothing
([SANDBOX-30](#SANDBOX-30) has the versions); the two make names and `GOTOOLCHAIN` stay listed or
set by the suite because other versions read them.

The other names in git(1) (Apple Git 2.54.0), GNU Make 3.81's manual and xcrun(1) that can name
code were each tested with a script that appends a marker file, planted under the name and run
against the git, `make` and `xcrun` commands of the workflows (init, add, commit, branch, merge,
stash, worktree, diff, status, push, clone, fetch, pull, `make`). It ran only under `DEVELOPER_DIR`,
`GIT_EXTERNAL_DIFF` (`git diff`), `MAKEFILES` and `MAKEFLAGS` with `SHELL=` or `-I`, which are
handled above. It did not run under
`GIT_ASKPASS`, `SSH_ASKPASS`, `GIT_PAGER`, `PAGER`, `GIT_EDITOR`,
`EDITOR`, `VISUAL`, `GIT_SEQUENCE_EDITOR`, `GIT_ATTR_SOURCE`, `GIT_CEILING_DIRECTORIES`, `GIT_SSH`,
`GIT_SSH_COMMAND`, `GIT_PROXY_COMMAND`, `GIT_TRACE`, `GIT_TRACE2`, `GIT_TRACE2_EVENT`,
`GIT_TRACE_CURL`, `GIT_DIFF_OPTS`, `GIT_SSL_NO_VERIFY`, `GIT_ALLOW_PROTOCOL`, `GIT_SSH_VARIANT`,
`GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_ALTERNATE_OBJECT_DIRECTORIES`, `GIT_NAMESPACE`,
`GIT_REDIRECT_STDERR`, `GIT_NO_REPLACE_OBJECTS`, `TOOLCHAINS`, `SDKROOT`, `XCODE_DEVELOPER_DIR_PATH`,
or, for `make`, `MFLAGS`, `GNUMAKEFLAGS`, `MAKEOVERRIDES`, `MAKELEVEL`, `SHELL` and
`MAKEFILE_LIST`. Those commands start no rebase, so they do not settle `GIT_SEQUENCE_EDITOR`, which
stays in the suite's list, as do `GIT_SSH`, `GIT_SSH_COMMAND` and `GIT_PROXY_COMMAND`, which only a
remote transport reads and a local remote does not use. `GIT_ASKPASS` and `SSH_ASKPASS` are removed
as host variables. The pager and editor names are
read only when git has a terminal to page on or an editor to start, and no stage has a terminal or
starts a commit without its message. A name in neither list reaches the programs as set, so a
workflow that starts a terminal or an editor, or reaches a remote, needs the names it reads added.

Rows that must stay refused run beside them: reading a private key in `~/.ssh`, reading
`~/.aws/credentials`, writing to a directory the session was not opened on, writing to the
home directory, and on macOS a script writing a hook into a repository's `.git`
([SANDBOX-14](#SANDBOX-14)). Such a row passes when its last stage is refused and the same workflow, run again
without the sandbox, works, so a path that was never there is not counted as a refusal. A row that
is let through fails, and so does one whose earlier stage is refused.

A workflow whose program is not installed is skipped, named, and counted apart from the passes; it
is not a failure. A program found only in a directory a stage may write is not installed, since the
programs the suite starts are found as [SANDBOX-30](#SANDBOX-30) says. A workflow that fails without
the sandbox as well is reported as that and not as the sandbox's. A failing row fails the suite,
which runs in CI on Linux and macOS and reports the number skipped.
`bravebot doctor --sandbox-check` runs the same suite on this machine, from a directory
under the state directory, and prints each row, and for a row that failed the stage, the fix for
that kind of failure and where the stage's output went. It exits with the failed ending
([CLI-6](cli.md#CLI-6)) when a row failed. It runs nothing on Windows, where programs are confined
by container.

`bravebot doctor --sandbox-check` adds one row to the suite for the login `gh` keeps in the real
home, which the scratch home cannot hold. It runs `gh auth token` with the person's home and sends
the output to the null device, first outside the sandbox and then under it, held to the person's
filesystem lists as a session is ([SANDBOX-25](#SANDBOX-25)). The row appears only
when `gh` is installed, found as [SANDBOX-30](#SANDBOX-30) says, and the run outside the sandbox
exits zero. It passes when the run under the
sandbox exits zero and fails otherwise, with a fix that names `sandbox.filesystem.allowRead`
([SANDBOX-25](#SANDBOX-25)) and `gh auth login --insecure-storage`. The CI suite does not run it,
since it depends on the host's login. Token variables are removed from the environment of both
runs, so the row measures the stored login. The `gh` that reads the stored login is also a checked
start with no names of its own, and every variable that begins `DYLD_` or `LD_` is removed from it
whatever it holds, since a library the dynamic loader loads into it runs with the account's home.
The two prefixes are `LOADER_PREFIXES` in `crates/sandbox/src/programs.rs`, removed with
`withhold_prefixed`. They are not in `CODE_SELECTOR_PREFIXES`, which every checked start applies
and which would remove `LD_DEBUG` from the starts that keep it ([SANDBOX-30](#SANDBOX-30)).
`make check-code-selectors` reads `LOADER_PREFIXES` and `HOST_VARIABLES`
([checks.md](../development/checks.md)). It does not apply the prefixes to other starts, so a
reviewer reads any other start that holds the loader's names.

The two loader tests start a copy of the host's shell that keeps the loader's names, since the
`gh` on a host can be built without the protection that ignores them. A host that cannot make that
copy prints `skipped`, and a run with `CI` set fails instead, so a green run on a CI machine means
the check ran. The `LD_` test uses a shell that records the names it is given. It has not been run
against a Linux loader. [SANDBOX-30](#SANDBOX-30) records which Linux runs have not been made.

**Why.** Each clause above is tested on the policy it builds, and a default that is sound by every
one of them can still stop `git commit` from working. That is found by running `git commit` under
it, which no policy test does, and a person finds it first when it is already the default.

**It branches on no output.** A row passes or fails on the exit status of its stages. What a
program printed goes to a log file under the suite's directory, and neither the report nor the
`doctor` output carries any of it, only the path of the log.

`verified-by: bravebot_agent::usability::every_workflow_works_under_the_default`
`verified-by: bravebot_agent::usability::the_suite_covers_each_everyday_program_and_the_rows_that_stay_refused`
`verified-by: bravebot_agent::usability::a_workflow_the_sandbox_refuses_fails_the_suite`
`verified-by: bravebot_agent::usability::a_workflow_that_fails_without_the_sandbox_is_not_blamed_on_it`
`verified-by: bravebot_agent::usability::a_row_expected_to_stay_refused_fails_when_the_program_gets_through`
`verified-by: bravebot_agent::usability::a_refusal_of_a_file_that_is_not_there_is_not_a_refusal`
`verified-by: bravebot_agent::usability::a_refusal_before_the_last_stage_of_a_refused_row_is_not_the_expected_one`
`verified-by: bravebot_agent::usability::a_setup_that_fails_is_named_as_setup`
`verified-by: bravebot_agent::usability::a_program_that_is_not_installed_is_skipped_by_name`
`verified-by: bravebot_agent::usability::a_program_a_shell_line_starts_is_skipped_by_name_when_it_is_not_installed`
`verified-by: bravebot_agent::usability::the_report_holds_no_program_output`
`verified-by: bravebot_agent::usability::the_suite_will_not_run_under_the_temporary_directory`
`verified-by: bravebot_agent::usability::a_login_the_sandbox_refuses_is_reported_as_a_login_failure`
`verified-by: bravebot_agent::usability::a_login_the_sandbox_lets_through_passes`
`verified-by: bravebot_agent::usability::a_login_file_on_the_allow_read_list_passes`
`verified-by: bravebot_agent::usability::no_login_outside_the_sandbox_gives_no_row`
`verified-by: bravebot_agent::usability::the_login_probe_keeps_no_token`
`verified-by: bravebot_agent::usability::the_variables_that_choose_the_code_a_stage_runs_are_removed_whatever_they_hold`
`verified-by: bravebot_agent::usability::git_configuration_given_through_the_environment_is_removed_for_any_index`
`verified-by: bravebot_agent::usability::a_variable_name_that_is_not_text_does_not_break_a_start`
`verified-by: bravebot_agent::usability::a_program_or_option_variable_of_a_workflow_tool_is_removed`
`verified-by: bravebot_agent::usability::the_program_cargo_runs_for_a_target_is_removed_and_its_target_directory_is_not`
`verified-by: bravebot_agent::usability::the_loader_variables_are_removed_from_the_stored_login_gh_whatever_they_hold`
`verified-by: bravebot_agent::usability::the_token_variables_are_removed_from_the_stored_login_gh`
`verified-by: bravebot_agent::usability::the_twelve_host_variables_that_move_a_read_row_or_hand_over_a_socket_are_removed`
`verified-by: bravebot_agent::usability::a_setup_commit_is_not_handed_a_template_directory`
`verified-by: bravebot_agent::usability::a_setup_commit_is_not_handed_configuration_that_names_a_hooks_directory`
`verified-by: bravebot_agent::usability::a_setup_commit_is_not_handed_a_second_configuration_pair_that_names_a_program`
`verified-by: bravebot_agent::usability::a_setup_commit_is_not_handed_configuration_parameters`
`verified-by: bravebot_agent::usability::a_setup_commit_is_not_handed_configuration_that_names_a_signing_program`
`verified-by: bravebot_agent::usability::a_setup_commit_is_not_handed_an_exec_path`
`verified-by: bravebot_agent::usability::a_setup_commit_is_not_handed_a_common_directory`
`verified-by: bravebot_agent::usability::a_setup_make_is_not_handed_makefiles`
`verified-by: bravebot_agent::usability::a_setup_make_is_not_handed_makeflags_that_name_a_shell`
`verified-by: bravebot_agent::usability::a_setup_cargo_is_not_handed_a_wrapper_or_target_flags`
`verified-by: bravebot_agent::usability::a_setup_cargo_keeps_a_wrapper_no_stage_can_write`
`verified-by: bravebot_agent::usability::a_setup_cargo_is_not_handed_a_compiler_or_a_workspace_wrapper`
`verified-by: bravebot_agent::usability::a_setup_cargo_is_not_handed_flags_or_a_target_program_that_name_a_linker_or_runner`
`verified-by: bravebot_agent::usability::a_setup_cargo_is_not_handed_a_rustdoc_or_documentation_flags`
`verified-by: bravebot_agent::usability::a_setup_cargo_doc_open_is_not_handed_a_browser`
`verified-by: bravebot_agent::usability::a_setup_cargo_is_not_handed_a_cargo_home_that_holds_configuration`
`verified-by: bravebot_agent::usability::a_setup_cargo_is_not_handed_a_toolchain_or_a_toolchain_directory`
`verified-by: bravebot_agent::usability::a_setup_npm_is_not_handed_configuration`
`verified-by: bravebot_agent::usability::a_setup_go_is_not_handed_flags_or_a_compiler`
`verified-by: bravebot_agent::usability::a_setup_go_is_not_handed_an_environment_file_or_a_cache_program`
`verified-by: bravebot_agent::usability::a_setup_go_is_not_handed_a_pkg_config_linker_flags_or_a_cxx_compiler`
`verified-by: bravebot_agent::usability::a_setup_go_is_not_handed_a_toolchain_named_in_its_environment`
`verified-by: bravebot_agent::usability::a_setup_node_is_not_handed_options_or_a_module_path`
`verified-by: bravebot_agent::usability::a_setup_npm_is_not_handed_a_configuration_file_or_an_initialiser`
`verified-by: bravebot_agent::usability::a_setup_python_is_not_handed_an_installation_or_a_compiled_module_tree`
`verified-by: bravebot_agent::usability::a_setup_python_venv_is_not_handed_an_executable`
`verified-by: bravebot_agent::usability::a_setup_python_the_workflows_start_does_not_read_a_startup_file`
`verified-by: bravebot_agent::usability::a_setup_make_is_not_handed_an_include_directory`
`verified-by: bravebot_agent::usability::a_setup_make_is_not_handed_the_program_or_linker_flags_of_a_built_in_rule`
`verified-by: bravebot_agent::usability::a_setup_cargo_is_not_handed_a_credential_provider`
`verified-by: bravebot_agent::usability::a_setup_go_is_not_handed_cgo_compiler_flags_that_name_an_assembler`
`verified-by: bravebot_agent::usability::a_setup_python_is_not_handed_a_module_path`
`verified-by: bravebot_agent::usability::a_setup_python_keeps_a_module_path_no_stage_can_write`
`verified-by: bravebot_agent::usability::the_names_found_by_running_cargo_npm_go_and_python_are_removed`
`verified-by: bravebot_agent::usability::a_program_variable_naming_a_file_a_stage_may_write_is_not_run_and_one_outside_is`
`verified-by: bravebot_agent::usability::a_setup_stage_is_not_handed_a_developer_directory`
`verified-by: bravebot_agent::usability::a_setup_go_vet_is_not_handed_a_go_root`
`verified-by: bravebot_agent::usability::a_setup_go_vet_is_not_handed_a_go_settings_file`
`verified-by: bravebot_agent::usability::the_gh_the_login_row_starts_is_not_handed_a_dyld_variable`
`verified-by: bravebot_agent::usability::the_gh_the_login_row_starts_is_not_handed_an_ld_variable`
`verified-by: bravebot_agent::usability_path::a_program_found_only_in_a_directory_a_stage_may_write_is_skipped_as_not_installed`
`verified-by: bravebot_agent::usability_path::a_gh_found_only_in_a_directory_a_stage_may_write_gives_no_login_row`
`verified-by: bravebot_cli::sandbox_check::doctor_sandbox_prints_a_failed_row_with_its_stage_and_its_fix`
`verified-by: bravebot_cli::sandbox_check::doctor_sandbox_gives_each_kind_of_failure_its_own_fix`
`verified-by: bravebot_cli::sandbox_check::doctor_sandbox_tells_a_refused_login_to_allow_the_read`
`verified-by: bravebot_cli::sandbox_check::doctor_sandbox_names_a_skipped_workflow_and_does_not_fail_on_it`
`verified-by: bravebot_cli::sandbox_check::doctor_sandbox_counts_passed_failed_and_skipped_apart`
`verified-by: bravebot_cli::running::doctor_sandbox_skips_what_is_not_installed_and_cleans_up_after_a_clean_run`
`verified-by: bravebot_cli::running::doctor_sandbox_refuses_a_further_argument`

<a id="SANDBOX-22"></a>
### SANDBOX-22: a person chooses how much a program `run` starts is held to, and a checkout can only ask for more

There are four sandbox modes. `strict` starts every stage under the deny-by-default profile
[SANDBOX-18](#SANDBOX-18) composes where the machine cannot be read: the base, the list its program's
binary brings, the credential scope its argv names and the session's directories, so a program reads
nothing else on the machine. On a Linux kernel below Landlock ABI 9, where a stage reaches every
unix socket that has a path, a `strict` or `read-only` stage is refused ([SANDBOX-3](#SANDBOX-3)). `read-only`
starts a stage with the reads of `workspace-write` and without its writes to the session: the
directories the session was opened on, the scratch directory and the toolchain caches can be read and
cannot be written, and the temporary directory and the null device stay writable unless the person
holds the temporary directory back, since compilers, test runners and shells write scratch files there. A toolchain a stage carries or asks for brings
its list and writes no cache, and a request for a path to write ([SANDBOX-28](#SANDBOX-28)) is
refused before anyone is asked, so the mode that answers every question cannot hand the session's
directories back. A person's own `allowWrite` entries ([SANDBOX-25](#SANDBOX-25)) and the write
grants they remembered ([SANDBOX-27](#SANDBOX-27)) still apply. `read-only` also holds back
what a program changes elsewhere: every stage starts with the network closed whatever `run.network`
says ([SANDBOX-20](#SANDBOX-20)), and none keeps it for a toolchain that fetches, a credential scope
or a program that talks to a remote. The remote scope ([SANDBOX-16](#SANDBOX-16)) is lent to no stage,
whether its argv, a `scopes` request or a remembered grant carries it, so `git push`,
`gh pr create`, `gh issue comment` and `curl` fail the way they do under Codex's `read-only`, and
`~/.ssh/known_hosts` is not written. Where the platform cannot deny the network ([SANDBOX-5](#SANDBOX-5)),
a `read-only` stage is not started, as under a closed `run.network`. The `run` description says so,
so the planner does not retry them. It holds back the programs
`run` starts and the file tools below, and no process a person approved outside them: a language
server runs with the person's own access in every mode ([LSP-5](tools/lsp.md#LSP-5)), so the build
tooling it runs, a crate's `build.rs` among them, may write anywhere the person can, the session's
directories included, and a local MCP server writes its own home and the directory its declaration
names in every mode ([SERVERS-10](mcp-servers.md#SERVERS-10)). Under `read-only` the file
tools write nothing either, in every permission mode, accepting edits and bypassing included:
`write_file`, `edit_file`, `download_url` and `apply_checkout` are refused before the tool runs and
nobody is asked, a manifest plan with a step that writes a file is refused before it is put to
anyone, a `run` line that redirects output into a file is refused because this process opens that
file and not the stage ([SANDBOX-18](#SANDBOX-18)), and a spawn asking for a checkout is refused,
since making one writes an entry under the repository's `.git`
([CHECKOUT-5](checkouts.md#CHECKOUT-5)). Redirecting to the null device writes no file and runs.
`/checkouts apply`, which the person types ([CHECKOUT-14](checkouts.md#CHECKOUT-14)), writes the
project and is not held to the mode. `workspace-write` is what [SANDBOX-17](#SANDBOX-17) and
[SANDBOX-18](#SANDBOX-18) describe, and is the mode when nothing chose another. `off` starts the
stage with no profile. On Windows no mode but `off` reads the machine: a `workspace-write` stage
starts under the profile a `strict` one does, so the two grant the same, and `read-only` is that
profile without the writes to the session's directories, the scratch directory and the toolchain
caches. The network is not part
of a mode: whether a stage keeps egress is the setting [SANDBOX-20](#SANDBOX-20) describes, and
`strict`, `read-only` and `workspace-write` honour it the same way. `off` starts no profile, so under
it nothing holds a network shut; the floor below is what stops a managed pin from meeting that.

A mode is looser than another when a program under it can read or write more. `off` is looser than
every other mode, and `workspace-write` is looser than `read-only`, which reads the same machine and
writes less. `strict` reads less than `read-only` and writes the session's directories, which
`read-only` does not, so each of the two is looser than the other. That is the ranking on Linux and
macOS. On Windows no mode but `off` reads the machine, so `read-only` is no looser than `strict`, which a
managed pin of `strict` therefore keeps, and `strict` is still looser than `read-only`.

The mode is read, in this order, from `--sandbox <mode>` (the last one wins), from the `sandbox.mode`
key of the settings, and from the managed file's pin; the default is `workspace-write`. In the settings, the home file and a `--settings` file outside
the workspace and every checkout the session has been in ([PERM-14](permissions.md#PERM-14)) may name any of the four, the later of the two winning. The checkout's project and
local files, and a `--settings` file inside either, may name only `strict`; a `read-only`,
`workspace-write` or `off` there is not obeyed and `doctor` names the file and the word. A `strict` from any of them
holds over what the home file or a `--settings` file outside them chose, unless `strict` is looser than that
choice or than the managed file's pin, which it is only of `read-only`. Such a `strict` is not obeyed and refuses
nothing, and `doctor` names its file and the file that chose `read-only`. A value that is not exactly one of the four words chooses
nothing and is reported with its file, except `standard`, the earlier name of `workspace-write`: it
names `workspace-write` in `--sandbox`, `/sandbox`, a settings file and the managed file, and every
report spells it `workspace-write`. Only `mode`, `holdBackTemporaryDirectory` and the two blocks named below are read from the `sandbox` block, and a key
beside them is reported as unread, so a block written for another tool does not look configured.

A person who wants no stage to leave a file in the temporary directory for a later program to read or
run sets `sandbox.holdBackTemporaryDirectory` to `true`, as Codex's `workspace-write` can exclude `$TMPDIR` and
`/tmp` from its writable roots. The directory is then read as before and written by no stage, in every
mode that confines, and under bypass, whose stages write every path the account can, it is a refusal of
the directory: the scratch directory and a person's own `allowWrite` entries are rows of their own and stay. A program that makes a scratch file there is refused, which is a shell heredoc,
`mktemp` and a compiler's intermediate files, and the `run` description says so. Only the person's own
settings file and a `--settings` file outside the workspace and the checkouts the session has been in are read for the key, as for `sandbox.mode`:
a checkout's project or local file that sets it is not obeyed and is reported as an unread key. Any
value but `true` asks for nothing. A `denyWrite` entry naming the temporary directory refuses it as well,
and is the way to hold back one path of it. On Windows a session with a directory of its own grants no
stage the account's temporary directory, so the setting changes nothing there; the `temp` directory
inside the session's own stays written, like the scratch directory.

The managed file's `sandbox.mode` is a floor and not a default. A flag or a settings file that names
a mode looser than it refuses the session before it starts, naming both files, saying which of
reading and writing the mode does more of than the pin, and naming the pin as the mode to run with,
since no mode is stricter than a pin of `strict` or of `read-only`; a mode no looser than the pin is
kept; with nothing asked, the pin is the mode. On a Linux kernel below Landlock ABI 9, a pin of `strict` or
`read-only` makes every `run` stage refuse with the sentence [SANDBOX-3](#SANDBOX-3) describes. The refusal
is made when a stage is prepared and not when the session starts, so a session that starts no stage still
works, and no looser mode replaces the pin. A managed file that pins `run.network`
to `closed` ([SANDBOX-20](#SANDBOX-20)) is a floor of `workspace-write` as well, because `off` starts a
program with no profile and nothing would hold its network shut; the refusal says so. `doctor`,
`--help`, `--version` and the commands that start no program (`auth`, `sessions`, `attach`,
`completion` and the two `import-` commands) run past a refusal, so that the person can read what it
says and sign in.

The desktop and the bridge have no flag, and read the mode from the settings and the pin. `off` is
read there as `workspace-write`, because a window has no line that shows its programs are unconfined. A
subagent's programs run under the mode of the session that started it, and the mode is not in the
session record, so a resumed session takes the mode its start-up chose and not the one it was
saved under. `/sandbox` alone says the mode in force and the flag or file that chose it, in the words `doctor`
uses. `/sandbox strict`, `/sandbox read-only`, `/sandbox workspace-write` and `/sandbox off` set the mode for the rest of the
process, and only a person's typed word does ([CMD-1](commands.md#CMD-1)): no tool, skill, hook or
`AGENTS.md` line sets it, and the planner is told the mode in force and nothing about the command.
A mode named while a turn runs is set as it is typed, `off` included ([CMD-8](commands.md#CMD-8)), and the
turn follows it from then on, as it follows a permission mode ([MODE-8](permission-modes.md#MODE-8)). A call
reads the mode when it begins and is decided under that one mode throughout: its `run` failure
sentence, its `scopes` and `request_path` refusals, a refusal of a file tool under `read-only`, and
the profile its programs start under. A call whose question is on screen when the mode is named
began before it and keeps the mode it began under. A delegate holds the same handle on the mode as
the turn that spawned it, so one running when the mode is named follows it too, and so does the
question whether bypassing answers a language server's start ([LSP-5](tools/lsp.md#LSP-5)). A
program already started keeps the profile it started under, whichever way the mode moved. The `run`
description the planner received states the mode the turn began in, so before its next request the
planner is told of the change in words the driver writes from the two modes: for a mode that
confines, the sentences the description would have carried under it, and for `off`, that programs
now start with no sandbox profile, so the limits the description states do not apply to them, and that
the `request_path` tool is now refused and nobody is asked, where the turn's list holds the tool
and nothing of it otherwise ([TOOL-6](tools/tool-surface.md#TOOL-6)). It is told once for each change, and a turn that confines none of
its runs is told nothing. A move to `off` is made when it is typed, like a move to any other mode,
with no question; the reply says that programs then start with no profile, and a reply made while a
turn runs says that the turn uses the mode for each program it starts from then on. A background job keeps the profile it started under ([RUN-15](tools/run.md#RUN-15)), so a
move to `read-only` while jobs are running says which ones keep the earlier mode, by the names `/jobs`
lists, and that `/jobs stop` ends them; with none running it says nothing of jobs
`verified-by: bravebot_tui::sandbox_command::a_move_to_read_only_names_the_jobs_that_keep_the_earlier_mode`.
The managed floor holds: a mode looser than the managed file's `sandbox.mode`
pin, and `off` under a managed `run.network` of `closed`, is refused with a sentence naming the
file, before anything is asked, and under the pin the sentence says which of reading and writing
the mode does more of. Otherwise the command ranks as `--sandbox` does, so
`/sandbox workspace-write` replaces a checkout's `strict`. The mode is not in the session record, so a
resumed session takes the mode its start-up chose, and the command's choice lasts until the process
ends, across `/clear` and `/resume`. `/bg` is refused while the command's choice differs from the mode
start-up chose, since the background process chooses again ([BG-2](background-sessions.md#BG-2)).
The opening screen, `/status` and the trace's `sandbox` gate
name the mode in force after the command.

The desktop window offers the sandbox mode as the Sandbox group of the session controls menu in the
composer, below the Approvals group that holds the permission mode. The open menu marks the mode in
force and offers `strict`, `read-only` and `workspace-write`. The closed button names the mode in
force after the permission mode when it is not `workspace-write`. It offers no `off`, and the bridge refuses a
request for `off`, or for any word that is not one of the three or `standard`, whatever the window
sends. A mode the kernel cannot hold ([SANDBOX-3](#SANDBOX-3)) is still offered and accepted, and a `run`
stage under it is refused with the sentence that clause describes. A choice is
the session's own and applies from the next turn or manifest run; the one running keeps the mode it
started in, which `/sandbox` in the terminal does not. The managed floor holds: a mode looser than the pin is refused with a sentence naming the
file and saying which of reading and writing the mode does more of, and a choice is met with the
floor again when a turn starts. A session that is opened, resumed
or forked starts from the settings and the pin, not from the choice of the session it came from, and
the settings' `off` is still read there as `workspace-write`.

The mode is chosen separately from the permission mode ([MODE-1](permission-modes.md#MODE-1))
and from `--dangerously-skip-permissions`. Neither changes which mode is in force, and neither
widens a mode, with one exception. Under bypass ([MODE-4](permission-modes.md#MODE-4)) and
`workspace-write`, on macOS and Linux, in a workspace the person has trusted, the stages of the lead
session's `run` may write every path a `request_path` for writing would be granted
([SANDBOX-28](#SANDBOX-28)) without the planner asking.
This widens `workspace-write`'s writes and nothing else: no read, no network access, nothing under
`strict`, nothing for a delegate, nothing in a workspace that is not trusted, and nothing on Windows.
One line may start with no profile where a
person approves it ([SANDBOX-29](#SANDBOX-29)), which is the mode `off` for that line only.

Under `off` the opening screen and `/status` do not report a closed network, since nothing then holds
a program to it.

Each `run` on a session that confines records a gate, `sandbox`, whose detail names the mode
the programs ran in ([TRACE-1](trace.md#TRACE-1)), or `unconfined` for a line that asked to start
with no profile ([SANDBOX-29](#SANDBOX-29)). The entry is written once the line is approved and
about to start, so a line the person does not approve has none. A run whose stages were given
what a `request_path` for writing would be granted ([SANDBOX-28](#SANDBOX-28)) records a second
`sandbox` entry saying so.

**Why.** A program that is refused a read or a write is a program the person wanted to run, and
having no way to say "not under this" sends them to a wrapper script outside the tool that confines
nothing and reports nothing. A mode they name, whose opening line reports it, is the same
choice with the report kept. It is the person's to make and not the checkout's: a line in a
repository that picked `off` would unconfine whoever cloned it, so the checkout may only ask for
`strict`, and only where that takes nothing from them, on the footing the `permissions` keys that only
refuse already have ([PERM-18](permissions.md#PERM-18)). Under a `read-only` the person or the managed
file chose, `strict` would let the checkout's programs write the session's directories, which is the
access `read-only` withholds. The pin is a floor because it is the
administrator's rule about the machine, and a flag that overrode it would be the setting they wrote
the pin to prevent. A pin looser than what a person asks for is not applied, so a person may be
stricter than the machine requires. Bypass may widen `workspace-write`'s writes because under bypass every
`request_path` a session accepts is already granted without asking, so a stage starts with what the
planner could get with one request instead of failing first. `strict` is not widened, since a
person who chose it asked for less than the default. `read-only` is not widened either: bypass
answers a question and does not change which mode is in force. A mode typed during a turn applies
to the rest of it because a person who types `/sandbox off` over a running turn is usually answering
a program the profile just refused, and a mode that waited for the turn to end would leave the
planner retrying that line, or giving up on it, until then. A move to a stricter mode applies at
once for the same reason in the other direction: the person who types it is stopping what the turn
is about to start. The planner is told because its `run` description would otherwise go on stating
a boundary that has ended, or omitting one that has begun. The file tools are held to
`read-only` because they write the same directories its programs cannot, and accepting edits and
bypassing answer the write prompt unasked, so a mode that held back programs alone would hand the
project back through `write_file` in the two permission modes a person picks to stop being asked,
and a redirection written on a `run` line would hand it back to a planner the file tools had
refused. The refusal comes before the
permission mode is reached, as a deny rule's does ([MODE-6](permission-modes.md#MODE-6)), so no
mode answers it.

**What it costs.** A turn that follows the mode can be decided under more than one: its `run`
description states the mode it began in until the notice that follows the change, and a program
already started keeps its profile, so a move to `strict` does not stop a program that is running. `strict` fails any program that reads a place its binary's list and its argv do
not name; the person reads the refusal sentence ([SANDBOX-19](#SANDBOX-19)) and chooses `workspace-write` or
adds the directory. `off` is the whole of the confinement removed: a window never offers it, and
the command line prints it on its opening line. On Windows, where `read-only` reads no more than
`strict` and writes less, a pin of `strict` keeps `read-only`, while a pin of `read-only` still
refuses `strict`, which writes the session's directories. On a Linux kernel below Landlock ABI 9 a pin of
`strict` or `read-only` refuses every `run` stage ([SANDBOX-3](#SANDBOX-3)), and nothing is relaxed. Under
bypass in a trusted workspace, a `workspace-write` stage of the lead
session writes every path outside the refusals [SANDBOX-28](#SANDBOX-28) lists, which includes
shell start-up files and other repositories on the machine. Under `read-only`, a program that writes
a session directory is refused with `Operation not permitted`. Builds, formatters and `git commit`
fail there, and the `run` description says so. A workspace inside the temporary directory stays
writable, since that directory is writable in every mode unless the person holds it back. A person who wants them to run chooses
`workspace-write`. The file tools refuse a write to that workspace as well, and a delegate defined
with `isolation: checkout` does not start. A line that redirects output into a file is refused
there too, so `make > build.log` is. A program the stage runs opens its own redirections under the
profile, so a person's `allowWrite` entry lets `sh -c 'make > build/log'` write where it lists the
directory, and the profile refuses it elsewhere.

`verified-by: bravebot_sandbox::mode::a_mode_is_read_only_from_its_exact_word`
`verified-by: bravebot_sandbox::mode::off_is_the_loosest_and_workspace_write_writes_more_than_read_only`
`verified-by: bravebot_sandbox::mode::strict_and_read_only_are_each_looser_than_the_other`
`verified-by: bravebot_sandbox::mode::a_looser_mode_reads_more_writes_more_or_both`
`verified-by: bravebot_sandbox::mode::on_windows_read_only_is_no_looser_than_strict`
`verified-by: bravebot_agent::confine::under_read_only_a_toolchain_writes_no_cache`
`verified-by: bravebot_agent::confine::read_only_lends_no_network_and_no_remote_scope`
`verified-by: bravebot_agent::turn::under_read_only_a_request_to_write_is_refused_and_one_to_read_is_not`
`verified-by: bravebot_agent::turn::under_read_only_the_file_tools_write_nothing_in_any_permission_mode`
`verified-by: bravebot_agent::manifest::under_read_only_a_plan_that_writes_is_refused_and_one_that_writes_nothing_runs`
`verified-by: bravebot_agent::turn::under_read_only_a_spawn_asking_for_a_checkout_makes_none`
`verified-by: bravebot_agent::turn::under_read_only_a_delegate_cannot_write_the_session`
`verified-by: bravebot_agent::turn::under_read_only_a_line_redirecting_into_a_file_is_refused_and_one_into_dev_null_runs`
`verified-by: bravebot_sandbox::mode::the_default_mode_is_workspace_write`
`verified-by: bravebot_config::sandbox::a_mode_is_read_from_the_block_and_nothing_near_it_is`
`verified-by: bravebot_sandbox::mode::the_word_workspace_write_replaced_still_names_it`
`verified-by: bravebot_config::settings::a_file_written_before_the_rename_still_names_workspace_write`
`verified-by: bravebot_tui::sandbox_command::the_earlier_word_for_workspace_write_still_moves_the_mode`
`verified-by: bravebot_config::settings::the_home_layer_and_a_named_file_outside_the_workspace_choose_the_mode`
`verified-by: bravebot_config::settings::a_checkout_may_only_tighten_the_sandbox`
`verified-by: bravebot_config::sandbox::the_temporary_directory_is_held_back_only_by_a_true`
`verified-by: bravebot_config::settings::only_the_persons_own_files_hold_the_temporary_directory_back`
`verified-by: bravebot_agent::confine::holding_the_temporary_directory_back_drops_its_write_row_alone`
`verified-by: bravebot_agent::confine::a_bypass_stage_is_refused_the_temporary_directory_a_person_held_back`
`verified-by: bravebot_agent::confine::the_planner_is_told_the_temporary_directory_a_person_held_back`
`verified-by: bravebot_agent::confine::holding_the_temporary_directory_back_refuses_a_write_there_and_nothing_else`
`verified-by: bravebot_agent::held_back_temporary_directory::the_settled_setting_holds_the_temporary_directory_back_from_run_and_is_told_to_the_planner`
`verified-by: bravebot_config::settings::a_checkouts_strict_does_not_lift_the_persons_read_only`
`verified-by: bravebot_config::settings::a_checkouts_strict_under_a_read_only_pin_starts_at_the_pin`
`verified-by: bravebot_cli::running::doctor_says_a_checkouts_strict_does_not_lift_the_persons_read_only`
`verified-by: bravebot_config::settings::a_word_that_is_not_a_mode_chooses_nothing_and_is_reported`
`verified-by: bravebot_config::settings::the_flag_beats_the_files_and_the_default_is_workspace_write`
`verified-by: bravebot_config::settings::a_managed_pin_refuses_a_looser_request_and_keeps_a_stricter_one`
`verified-by: bravebot_config::settings::a_pin_of_strict_and_a_pin_of_read_only_each_refuse_the_other`
`verified-by: bravebot_agent::confine::a_read_only_stage_reads_the_session_and_writes_neither_it_nor_the_scratch_directory_nor_a_cache`
`verified-by: bravebot_agent::turn::a_run_under_read_only_reads_the_machine_and_cannot_write_the_session`
`verified-by: bravebot_config::settings::a_pin_that_is_not_a_mode_pins_nothing`
`verified-by: bravebot_config::settings::a_closed_network_pin_refuses_off_and_leaves_the_rest`
`verified-by: bravebot_config::settings::a_network_pin_that_is_neither_word_refuses_off_as_a_closed_one_does`
`verified-by: bravebot_config::settings::a_key_beside_the_sandbox_mode_is_reported_as_unread`
`verified-by: bravebot_tui::logo::a_closed_network_is_not_claimed_for_programs_nothing_confines`
`verified-by: bravebot_agent::confine::strict_and_read_only_are_refused_where_sockets_with_a_path_are_not_governed`
`verified-by: bravebot_cli::main::a_refused_mode_stops_what_runs_programs_and_not_what_does_not`
`verified-by: bravebot_config::settings::a_window_never_runs_unconfined_and_keeps_the_pin`
`verified-by: bravebot_cli::main::the_sandbox_flag_takes_one_of_four_words_and_the_last_wins`
`verified-by: bravebot_cli::main::the_sandbox_flag_refuses_a_word_that_is_not_a_mode`
`verified-by: bravebot_cli::doctor::doctor_names_the_sandbox_mode_and_where_it_came_from`
`verified-by: bravebot_cli::doctor::doctor_names_a_sandbox_flag_as_the_choice_in_force`
`verified-by: bravebot_cli::doctor::doctor_reports_a_sandbox_flag_the_managed_file_refuses`
`verified-by: bravebot_cli::doctor::doctor_reports_a_mode_the_managed_file_refuses`
`verified-by: bravebot_cli::main::a_refused_mode_says_what_it_does_more_of_and_suggests_a_mode_the_pin_keeps`
`verified-by: bravebot_cli::running::the_sandbox_flag_is_read_before_the_run_starts`
`verified-by: bravebot_cli::running::the_sandbox_flag_reaches_what_the_planner_is_told_of_a_run`
`verified-by: bravebot_agent::turn::a_run_under_off_starts_with_no_profile_and_says_nothing_of_one`
`verified-by: bravebot_agent::turn::a_run_under_strict_cannot_read_what_workspace_write_reads`
`verified-by: bravebot_agent::turn::a_delegate_runs_its_programs_under_the_mode_of_the_turn_that_spawned_it`
`verified-by: bravebot_agent::turn::a_delegate_of_a_strict_turn_cannot_read_what_a_workspace_write_one_reads`
`verified-by: bravebot_core::policy::the_trail_says_which_sandbox_mode_the_programs_ran_in`
`verified-by: bravebot_agent::turn::a_bypass_run_writes_beside_the_session_without_a_request_and_the_trail_says_so`
`verified-by: bravebot_ui_bridge::permission_mode::a_window_reads_off_as_workspace_write`
`verified-by: bravebot_config::settings::a_session_moving_its_mode_meets_the_floor_start_up_applies`
`verified-by: bravebot_config::settings::a_windows_choice_outranks_the_settings_and_meets_the_pin`
`verified-by: bravebot_ui_bridge::wire::a_window_may_name_strict_read_only_or_workspace_write_and_no_other_sandbox_mode`
`verified-by: bravebot_ui_bridge::wire::a_refused_sandbox_mode_names_the_file_and_the_floor`
`verified-by: bravebot_ui_bridge::wire::a_refused_sandbox_mode_says_whether_it_reads_or_writes_more_than_the_pin`
`verified-by: bravebot_ui_bridge::sandbox_mode::a_window_chooses_the_mode_the_next_turn_runs_under`
`verified-by: bravebot_ui_bridge::sandbox_mode::a_read_only_window_refuses_file_writes_before_accepting_edits`
`verified-by: bravebot_ui_bridge::sandbox_mode::a_read_only_window_refuses_a_writing_manifest_before_approval`
`verified-by: bravebot_ui_bridge::sandbox_mode::a_window_cannot_choose_to_turn_the_sandbox_off`
`verified-by: bravebot_ui_bridge::sandbox_mode::another_session_opens_under_the_settings_and_not_the_choice`
`verified-by: bravebot_ui_bridge::sandbox_mode::a_projects_strict_does_not_lift_the_persons_read_only`
`verified-by: bravebot_ui_bridge::sandbox_mode::a_choice_made_in_one_session_does_not_reach_another_open_beside_it`
`verified-by: bravebot_ui_bridge::sandbox_mode::a_chosen_mode_is_what_the_next_turns_program_is_held_to`
`verified-by: bravebot_tui::sandbox_command::a_named_mode_is_the_mode_the_next_turn_is_built_with`
`verified-by: bravebot_tui::sandbox_command::a_session_started_from_the_held_choice_keeps_the_commands_mode`
`verified-by: bravebot_tui::sandbox_command::a_move_to_off_is_made_when_typed_and_asks_nothing`
`verified-by: bravebot_tui::sandbox_command::the_managed_floor_refuses_a_looser_mode_naming_the_file`
`verified-by: bravebot_tui::sandbox_command::a_refusal_says_which_of_reading_and_writing_the_mode_does_more_of`
`verified-by: bravebot_agent::sandbox_reason::each_way_a_mode_is_looser_than_the_pin_has_its_own_sentence`
`verified-by: bravebot_tui::sandbox_command::a_word_that_is_not_a_mode_changes_nothing_and_the_bare_word_only_reports`
`verified-by: bravebot_tui::sandbox_command::the_report_names_what_chose_the_mode`
`verified-by: bravebot_tui::app::the_sandbox_command_carries_its_word_unparsed`
`verified-by: bravebot_tui::app::a_sandbox_mode_named_mid_turn_is_set_as_it_is_typed_and_the_turn_follows_it`
`verified-by: bravebot_tui::app::moving_the_sandbox_to_off_mid_turn_is_carried_out_when_typed`
`verified-by: bravebot_agent::live_sandbox::a_mode_set_through_one_handle_is_read_through_its_clones`
`verified-by: bravebot_agent::live_sandbox::handles_made_apart_do_not_share_a_mode`
`verified-by: bravebot_agent::turn::a_sandbox_mode_chosen_during_a_turn_applies_to_the_calls_after_it`
`verified-by: bravebot_agent::turn::a_delegate_follows_a_sandbox_mode_chosen_while_it_runs`
`verified-by: bravebot_agent::confine::the_planner_is_told_of_a_sandbox_mode_chosen_while_its_turn_runs`
`verified-by: bravebot_agent::confine::the_change_notice_for_a_move_to_off_says_request_path_is_refused`
`verified-by: bravebot_agent::turn::a_mode_change_to_off_names_request_path_only_where_the_turn_was_offered_it`
`verified-by: bravebot_agent::permission_mode::a_confirmer_follows_the_sandbox_mode_chosen_after_it_was_made`

<a id="SANDBOX-23"></a>
### SANDBOX-23: reach a person remembered for a command is attached to that command's stage, and nothing else makes it

`/reach <scope or directory> [write] [always] -- <command line>` records, for each stage of the line,
that the stage reaches one more thing: a credential scope of the closed table
([SANDBOX-16](#SANDBOX-16)), or a directory the person named, read unless they said `write`. A
record is keyed on the file the stage's program resolved to and its operation word (its first
argument, unless that is an option), so a grant made for `git push` is not one for `git pull` or
`git -C dir push`, and one made for `/usr/bin/make` is not one for another `make` earlier on the
path. It lasts the session that made it, a `--resume` of it included, or every session with
`always`, and is in force only in the workspace root it was typed in (below). `/reach` alone lists
the grants in force there and `/reach remove <n>` removes the one numbered. `/status` lists the same
rows, numbered the same way, under a heading of their own, and a session with none in force lists
none. A grant is attached when the plan is composed ([SANDBOX-18](#SANDBOX-18)): its rows are
in the stage's profile, the plan the person endorses names it with the day it was allowed, and the
failure line ([SANDBOX-19](#SANDBOX-19)) names a remembered scope as it names any other.

The inputs to a grant are a person's typed words, the compiled step, the closed table and the
process environment, and nothing else, except that the remote scope also reads the `IdentityFile`
lines of the person's `~/.ssh/config` ([SANDBOX-16](#SANDBOX-16)). In particular:

- A directory is judged as `--add-dir` judges one, when it is allowed and again when it is used:
  absolute, no `..`, existing, not the home or above it, not a credential location of the table
  ([SANDBOX-12](#SANDBOX-12)) or inside one, and a link by where it leads. A directory later replaced by a link to `~/.ssh` is dropped from the profile and
  from the plan.
- Write is a directory's. A scope is never written by a grant, and `/reach` refuses one.
- A stage with an assignment in front of it is covered by no grant, and a line with one is refused,
  for the reason an assignment removes a scope ([SANDBOX-16](#SANDBOX-16)).
- A stage that starts with an option (`sh -c ...`, `git -C dir push`) has no operation to key on,
  so `/reach` refuses the line, and a grant for a command given no arguments covers it only while
  it is given none. Otherwise a grant made for one script would follow every script.
- A remembered scope for a program the table names is attached only to a stage whose whole argv
  the table gives that scope ([SANDBOX-16](#SANDBOX-16)), so `git push --receive-pack=<file>
  origin`, `git pull -s <name>`, `kubectl get pods --kubeconfig=<file>` and `docker ps --config=<dir>`
  carry neither the scope's rows nor the egress it keeps, whatever grant was remembered for the
  program and its operation word. A scope for a program the table does not name
  (`/reach aws -- make check`) is keyed on the shape alone, since the table reads no argv for it.
- A grant is for the workspace root it was typed in, and the record keeps that root with the
  identity of the directory there (its creation time and inode, as the kept answer of
  [TRUST-23](trust-map.md#TRUST-23) does). It is read only where the session's workspace root is
  that directory, so a row typed in one checkout is not in force after `/cd` into another or in a
  new session started there, and `always` means always in that directory. A different directory
  made at the same path is not the one the row was typed in, and a directory whose identity cannot
  be read takes no row. The one exception is a credential scope for a program the table above
  already gives that scope to (`git` and `gh` for the remote scope, `aws`, `kubectl`, `docker`),
  which is for every checkout because that tool's configuration directory is the same in all of
  them; no second list of programs is kept. A scope typed for any other program
  (`/reach aws -- make check`) is bound like a directory. A line with no root that is not such a
  scope grants nothing.
- The record is `reach.jsonl` in the state directory. A checkout's files are not read for it, a
  session grant is read only by the session whose id it carries, and a line that is not a grant
  this build understands grants nothing.
- A session with no profile directory to judge against, and a turn with no session to show the row
  to, read and add none. An incognito session reads the record and adds nothing to it
  ([INCOG-5](incognito.md#INCOG-5)).
- A remembered scope is a credential scope for a closed network ([SANDBOX-20](#SANDBOX-20)), so the
  stage keeps the egress it needs to use it. A remembered directory is not a reason to keep it.
- What a program printed and how it exited are not inputs. A refused run leaves no record and the
  same line planned again is held to the same profile.

**Why.** The decision under *Widening happens before the run* is that nothing widens in answer to a
refusal, because the path a refusal names is chosen by the program, and a repository chooses the
program's output. A person typing the path is the other route. Without a way to keep it, a build
that needs a directory once needs it again in every session and the person is asked, or told to
type `--add-dir`, for the same command each time. The grant is still the plan: it is shown before the
run, in the words of the other reach, so there is nothing new to trust.

**What is not built.** A path the planner proposes, and a reach attached to a command the planner
has never run. Each puts a path or a shape the planner chose in front of a person to approve, and
neither is decided here. The one key at the confirmation that remembers a reach is for a credential
scope the planner asked a line for ([SANDBOX-27](#SANDBOX-27)); none remembers a directory.

`verified-by: bravebot_agent::reach::a_grant_covers_the_file_and_the_operation_it_was_made_for`
`verified-by: bravebot_agent::reach::an_assignment_in_front_of_a_step_removes_every_grant`
`verified-by: bravebot_agent::reach::a_grant_for_a_program_the_table_names_covers_only_an_argv_the_table_gives_the_scope`
`verified-by: bravebot_agent::reach::a_grant_is_read_back_by_the_sessions_it_was_made_for`
`verified-by: bravebot_agent::reach::a_revoked_grant_is_gone_until_it_is_allowed_again`
`verified-by: bravebot_agent::reach::a_line_that_is_not_a_grant_grants_nothing`
`verified-by: bravebot_agent::reach::a_grant_made_for_one_command_is_made_for_that_command_only`
`verified-by: bravebot_agent::reach::the_status_rows_are_the_grants_in_force_here_and_no_others`
`verified-by: bravebot_tui::status::the_report_lists_the_reach_remembered_for_commands`
`verified-by: bravebot_agent::reach::a_pipeline_gets_one_grant_for_each_distinct_stage`
`verified-by: bravebot_agent::reach::a_command_that_starts_with_an_option_carries_no_grant`
`verified-by: bravebot_agent::reach::a_directory_is_read_unless_the_person_said_write`
`verified-by: bravebot_agent::reach::a_directory_that_holds_a_key_or_does_not_exist_is_refused`
`verified-by: bravebot_agent::reach::a_directory_replaced_by_a_link_to_the_keys_is_refused_at_use`
`verified-by: bravebot_agent::reach::a_scope_is_never_written_and_an_assignment_is_never_granted`
`verified-by: bravebot_agent::reach::a_line_without_a_command_is_told_the_usage`
`verified-by: bravebot_agent::reach::remove_takes_away_the_row_the_list_numbers`
`verified-by: bravebot_agent::reach::another_sessions_grant_is_not_listed`
`verified-by: bravebot_agent::reach::a_session_with_no_profile_grants_nothing`
`verified-by: bravebot_agent::reach::a_directory_row_is_in_force_only_in_the_checkout_it_was_typed_in`
`verified-by: bravebot_agent::reach::a_scope_for_a_program_the_table_names_is_in_force_in_every_checkout`
`verified-by: bravebot_agent::reach::a_scope_for_a_program_the_table_does_not_name_is_bound_to_the_checkout`
`verified-by: bravebot_agent::reach::a_row_bound_to_another_directory_at_the_same_path_is_dropped`
`verified-by: bravebot_agent::reach::remove_reaches_only_the_rows_of_this_checkout`
`verified-by: bravebot_agent::confine::a_remembered_scope_reaches_the_command_it_was_made_for_and_no_other`
`verified-by: bravebot_agent::confine::a_remembered_scope_is_not_attached_to_an_argv_the_table_refuses`
`verified-by: bravebot_agent::confine::a_remembered_directory_is_read_and_written_only_where_the_grant_says`
`verified-by: bravebot_agent::confine::a_remembered_reach_is_withheld_where_the_step_or_the_machine_has_changed`
`verified-by: bravebot_agent::confine::the_plan_and_the_failure_line_name_a_remembered_scope`
`verified-by: bravebot_agent::confine::a_remembered_scope_keeps_a_closed_network_and_a_remembered_directory_does_not`
`verified-by: bravebot_agent::tools::remembered_reach_comes_from_the_state_directory_for_the_session_that_has_one`
`verified-by: bravebot_agent::turn::a_refused_run_whose_stderr_names_a_path_adds_no_row`
`verified-by: bravebot_agent::turn::a_remembered_reach_is_in_the_plan_and_the_failure_line_of_its_session_only`
`verified-by: bravebot_agent::incognito::no_remembered_reach_is_written_down`
`verified-by: bravebot_agent::incognito::a_reach_an_earlier_session_remembered_is_still_honoured`

<a id="SANDBOX-24"></a>
### SANDBOX-24: a list of hosts is applied by a proxy the session runs

A stage that has egress may be held to a list of host names. A backend filters by address and port
and never by name ([SANDBOX-3](#SANDBOX-3)), so the list is applied by a proxy on a loopback port:
the profile allows that one port, and the proxy decides on the name.

A list is an allowed set and a denied set. An entry is a host name or `*.` and a domain, which
covers every name below the domain and not the domain itself. Names are compared in lower case
without a trailing dot. An entry that is anything else, `*` alone and `a.*.com` included, is not a
rule and is returned to the caller to report. A denied entry wins over an allowed one in either
order, and a name no allowed entry covers is refused, so an empty list refuses every host. No list
means no proxy and no filtering. The defaults a caller adds to a list are code: `github.com`,
`api.github.com` and `*.githubusercontent.com` for a stage carrying the remote scope, and for a
toolchain its registry hosts (`crates.io`, `static.crates.io` and `index.crates.io` for cargo,
`registry.npmjs.org` for node, `pypi.org` and `files.pythonhosted.org` for python,
`proxy.golang.org` for go).

The proxy decides from the destination in the `CONNECT` request line and the addresses that name
resolves to, and from nothing else: not a header, not a reply, not a byte the tunnel carries. TLS
is not terminated, so an allowed tunnel is two byte streams copied into each other unread, and a
request that is not a `CONNECT` is refused. A tunnel is opened only to port 443 or 80.

A name the list allows is resolved once, and the tunnel is opened to an address that lookup gave
and to no other, so an answer that differs the next time is never connected to. If any address in
the answer is not public, in the classes [FETCH-8](tools/fetch-url.md#FETCH-8) names, with an IPv4
address inside an IPv6 one classified as the IPv4 address, the request is refused unless an exact
entry of the allowed list is that address, an IPv4-mapped one compared as its IPv4 address, and no
denied entry covers it. A wildcard names no address, so a server under `*.github.io` cannot answer
with loopback, a private network or the metadata address and be tunnelled to, and `*.0.0.1` does
not list `127.0.0.1`. A request that names an address that is not public, read as the list reads a
name so a trailing dot changes nothing, and that no entry covers, is refused as not public rather
than as not listed. An address written another way, such as `2130706433`, is a name to that check;
a yes to it reaches nothing that is not public, since what it resolves to is checked like any
answer.

A refused request is answered with the same bytes for
every host and every reason, which name neither the host the program asked for nor the rule, so the
program learns nothing from it and nothing a program chose reaches the planner. Every decision is
recorded with the host and the rule that decided it, for the trace, and never the traffic. The
proxy decides on the name the client sent, so a host on an allowed content network can still reach
other tenants of it. That is a known cost and not something the list can close.

The variables `HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY`, in both cases, name the proxy, and a
caller sets them after the person's environment so an assignment a model wrote cannot point a
stage elsewhere. The proxy lives as long as the value that started it, and is not listening once
that is dropped.

**Why.** With the network open, a stage that can read a credential can send it anywhere, and
closing the network wholesale ([SANDBOX-20](#SANDBOX-20)) takes the registries from a build.
Letting a person name the hosts a program may reach keeps the build and removes the exfiltration
path to the rest. A name resolves to whatever its server answers, so without the address check a
listed name, or one under a wildcard whose server another person runs, reaches the services on
this machine's loopback, the network it sits inside and the metadata address. Deciding from the
request line alone keeps the proxy from being a second reader of content, and a fixed refusal
keeps a name the program chose out of a sentence the planner reads.

The list is named by `sandbox.network.allowedHosts`, `deniedHosts` and `onUnlisted` (`ask` or
`refuse`). `deniedHosts` and `onUnlisted: refuse` only take reach away, so any layer may write them.
`allowedHosts` and `onUnlisted: ask` give it back, so they are read from the person's own settings,
the file `--settings` names outside the workspace and the checkouts the session has been in, and nothing a clone brings, as the allowances of
`sandbox.filesystem` are; a project or local layer's is named by `doctor` and not read. A key that
is not there is no list. A list that is set, even an empty one, is one, and a layer whose list was
not read does not make one. A value that is not a list of strings, or an `onUnlisted` that is
neither word, is reported and read as absent.

The managed layer pins the keys it writes. Its `allowedHosts` is the whole allowed set and nothing a
person wrote is added to it, so a person's entry cannot widen it and `doctor` lists the pinned keys.
An empty one pins a list that refuses every host, and filters a session that set none. Its
`deniedHosts` is added to the person's and nothing lifts it. Its `onUnlisted` is the answer whatever
a person said. A managed `allowedHosts` also drops the person's `onUnlisted: ask` unless the managed
file sets `onUnlisted` itself, since a yes to the prompt below adds to the allowed set, which is the
widening the pin exists to prevent. A managed value that is not a list of strings, or an
`onUnlisted` that is neither word, pins nothing and `doctor` names it.

A local MCP server is held to the same list ([SERVERS-10](mcp-servers.md#SERVERS-10)): it is started
with those variables, its policy reaches the proxy's port alone, the registry hosts of its program's
toolchain are added to the list, and a platform that cannot hold it to the proxy does not start it.
A server whose declaration lists `allowed_hosts` is held to that list, narrowed to what the
session's allowed list covers.

A stage with egress under a set list is started with those variables, and a stage with no egress
is not, because it reaches nothing. A stage's list is the person's allowed set and the defaults its
own reasons bring, the remote scope's hosts and its toolchain's registries, so two stages can hold
different lists. One proxy is started for each distinct list the first time a stage needs it, and
is kept for the rest of the process. A denied entry that is not a rule fails the stage, since
dropping it would let the host through, and so does a proxy that cannot be started: a stage is
never started without the filter its list asks for. An `accept` that fails, as one does with no
descriptor left, is retried after a pause that doubles from 5 milliseconds up to a second and starts
again at the next connection accepted, so the proxy does not hold a core while the descriptors stay
spent. A connection its client abandoned before it was accepted is passed over with no pause, so a
stage that resets connections cannot slow the proxy for the other stages sharing it.

A stage held to a proxy has a policy that allows outbound TCP to that proxy's loopback port and to
no other address, and reaches no name resolver, since the proxy resolves the name it tunnels to. A
program that ignores the variables and connects to a host, or to another loopback port, is refused
by the operating system, and so is one whose line asked for loopback ([SANDBOX-3](#SANDBOX-3)),
which lets it listen and adds no port to connect to. That costs a stage the loopback services of its own, a local database or
a test server, and a program that does not send its connections through the variables: `ssh`
reads none of them, so a remote reached over ssh cannot connect while a list is set; an HTTPS
remote can. A unix socket a write row already names is still reached. macOS applies
this in the profile. Linux applies it with a network namespace ([SANDBOX-3](#SANDBOX-3)): the
proxy listens on a unix socket as well as its loopback port, in a directory only the account can
enter, and a forwarder started in the namespace listens on the proxy's port on the namespace's
loopback and relays each connection to that socket. The directory is made under the account's
runtime directory (`XDG_RUNTIME_DIR`) where that is an absolute path outside the temporary
directory that can be written, which is in none of a stage's rows unless the person names it,
and under the temporary directory otherwise. The stage reaches no other address and no other protocol, and holds the loopback
services it starts itself, since they live in the namespace. The forwarder is started before the
stage's filesystem rules are installed, so the stage needs no grant to the socket, cannot attach
to it, and it ends with the stage. The forwarder is not confined, so it reaches whatever the
socket's path leads to, and a stage that can write where the path is can make it lead to the
session bus, an agent, a container runtime or another proxy. After each connect the forwarder
reads what the kernel recorded of the socket that answered, and relays only if the process that
listens on it is the one that runs the proxy and the name it is bound to is the proxy's path. It
closes the stage's connection and that one, having sent and read nothing, when either differs.
Where the kernel will not make the namespace, a stage is refused while a list is set, with
`sandbox.network.allowedHosts` named, as a stage is refused where the network cannot be closed
([SANDBOX-20](#SANDBOX-20)), and `doctor` says why. Windows keeps that refusal: letting an
AppContainer reach loopback takes a loopback exemption, which is machine-wide and needs
administrator rights, and so do the filtering rules of the Windows Filtering Platform, and a
sandbox that changes the machine's network policy is a worse trade than refusing the stage. The
stage is not started with the variables alone. The sentence the
planner reads under SANDBOX-19 says the network is limited by that setting and names no host.

After a foreground `run` has stopped, the trail records what the list decided for the hosts its
programs asked each proxy for, under `hosts`: the host, `allowed` or `refused`, the entry that
decided it or `not listed`, `port not carried` or `not public`, and the stages that proxy serves
in that line.
Repeats are counted. A program chooses the name it asks for, so the record carries a name only
where it is a host name or an IP address, and a request for anything else is recorded as a name
that is not a host name, with none of its bytes. A proxy keeps at most 256 decisions until they are
taken and counts the rest, which the record states. A proxy is shared by every stage with the same
list, so a decision taken for a job left running is recorded with the next foreground line that
shares its proxy, and a decision taken after a line was moved to the background is recorded the
same way. `/status` shows a line with how many entries the allowed and denied lists hold and the
files that wrote them, and no entry, for a session with an `allowedHosts` list that is not run
with the sandbox off.

Under `onUnlisted: ask`, the hosts a foreground run was refused for want of an entry are put to the
person once that run has stopped. Nothing waits on the answer: the program was refused already, and
a yes lets the lines that follow reach the hosts, so the person runs it again. The question names
each host on a row of its own, up to eight at a time, and says that a yes holds until the session
ends and writes nothing to disk. `/clear`, `/branch` and `/resume` begin another session in the same
process, and a host answered before, a no included, is asked about again. A yes adds the hosts to the allowed set of every stage
from the next line on, which starts another proxy because the list is another list. A host the denied list
names stays refused, and a granted name is held to the addresses it resolves to as a listed one
is. A no is kept for the session, so the same host is not asked about again. The hosts asked about
are those the proxy refused as not listed and that are host names or addresses; a host refused for
its port, by a denied entry or as not public, or a name that is not a host name, is not put to
anyone. A delegate's lines are not asked about, and the mode that asks nothing refuses, so
`ask` behaves as `refuse` under bypass and for a session nobody is at. Bypass does not keep the
refusal, so a session that leaves it asks about the same host. The answer is recorded under
`host_grant` with the hosts. The hosts are a program's own bytes: they go to the person's screen
and the trail, the planner is told nothing about them, and nothing branches on one.

**What it costs.** A proxy holds no secret: it serves whatever connects to its port or its socket,
so a process that reaches one is held to that proxy's list and not to its own. That is any process
of another user that can reach the loopback port; on Linux, where the account has no runtime
directory outside the temporary directory that can be written, a stage of any list, or one with
no network, since the socket's directory is then under the temporary directory every stage writes
and a write row reaches a socket beneath it, and a stage that can write there can also remove the
socket, which leaves that list's proxy out of reach of every later stage; and on macOS a stage
lent loopback and held to no proxy, which connects to every loopback port. The system removes the
runtime directory when the account's last login session ends and nothing lingers, and a session
that outlives it has no socket for its proxies: a stage held to a list then reaches nothing, and
the proxy is not made again. The forwarder connects to whatever the socket's path leads to before
it can tell that it is not the proxy's, so a stage that makes the path lead to a service a
connection starts, as socket activation does, starts it, and nothing is sent to it. A proxy starts a thread for each connection with no
limit on how many it holds, and keeps a tunnel open until one side closes it, so a stage can open
connections until the session process has no thread or descriptor left to give. No entry can spell an IPv6
address, so a name whose answer holds one that is not public is refused whatever the list holds:
`localhost` resolves to `::1` as well as `127.0.0.1` on most machines, so a program reaches a
loopback service through the proxy as `127.0.0.1`, with that address listed.

The tests that hold a stage to the proxy through the namespace need a kernel that lets an
unprivileged user make one. They say so and do not run where it does not, so a green run there
says nothing about them.

`verified-by: bravebot_sandbox::policy::the_port_egress_is_limited_to_is_carried_and_goes_with_the_network`
`verified-by: bravebot_sandbox::proxy::a_failing_accept_is_retried_after_a_pause_that_doubles`
`verified-by: bravebot_sandbox::proxy::a_connection_abandoned_before_it_was_accepted_is_passed_over_at_once`
`verified-by: bravebot_sandbox::macos::egress_limited_to_a_port_names_the_port_and_no_other_address`
`verified-by: bravebot_sandbox::macos::a_process_limited_to_a_port_reaches_that_port_and_not_another`
`verified-by: bravebot_sandbox::macos::a_process_held_to_a_port_and_granted_loopback_reaches_no_other_local_port`
`verified-by: bravebot_sandbox::windows::a_policy_limiting_egress_to_one_port_is_refused`
`verified-by: bravebot_sandbox::linux::a_policy_limiting_egress_to_one_port_without_the_proxys_socket_is_refused`
`verified-by: bravebot_sandbox::linux::a_kernel_that_refuses_the_namespace_refuses_the_stage`
`verified-by: bravebot_sandbox::linux::a_stage_held_to_the_proxy_reaches_it_through_the_forwarder`
`verified-by: bravebot_sandbox::linux::a_stage_held_to_the_proxy_cannot_connect_around_it`
`verified-by: bravebot_sandbox::linux::a_stage_with_no_network_cannot_connect_to_anything`
`verified-by: bravebot_cli::doctor::doctor_says_when_network_namespaces_are_unavailable_and_what_that_costs`
`verified-by: bravebot_sandbox::netns::the_forwarder_relays_to_the_proxy_and_ends_with_the_process_that_started_it`
`verified-by: bravebot_sandbox::netns::the_forwarder_relays_to_no_listener_that_another_process_made`
`verified-by: bravebot_sandbox::netns::the_forwarder_relays_to_no_socket_a_link_at_the_proxys_path_leads_to`
`verified-by: bravebot_sandbox::linux::a_stage_that_replaces_the_proxys_socket_with_a_link_is_not_relayed_to_what_it_leads_to`
`verified-by: bravebot_sandbox::proxy::the_socket_is_made_under_the_runtime_directory_and_the_temporary_one_is_the_fallback`
`verified-by: bravebot_agent::confine::a_platform_that_cannot_hold_a_program_to_the_proxy_is_refused_naming_the_setting`
`verified-by: bravebot_agent::confine::a_stage_under_a_host_list_is_prepared_limited_to_the_proxy_port`
`verified-by: bravebot_agent::confine::the_profile_line_names_the_host_list_setting_and_no_host`
`verified-by: bravebot_agent::confine::a_stage_under_a_host_list_cannot_connect_around_the_proxy`
`verified-by: bravebot_agent::confine::a_platform_that_cannot_hold_a_stage_to_the_proxy_refuses_it`
`verified-by: bravebot_sandbox::hosts::an_exact_entry_covers_that_name_and_no_other`
`verified-by: bravebot_agent::workspace::a_session_ending_in_this_process_takes_its_reach_and_host_answers_with_it`
`verified-by: bravebot_sandbox::hosts::a_wildcard_covers_names_below_the_domain_and_not_the_domain`
`verified-by: bravebot_sandbox::hosts::a_denied_entry_wins_over_an_allowed_one_in_either_order`
`verified-by: bravebot_sandbox::hosts::an_empty_list_refuses_every_host`
`verified-by: bravebot_sandbox::hosts::an_entry_that_is_not_a_name_or_a_leading_wildcard_is_returned_and_not_read`
`verified-by: bravebot_sandbox::hosts::the_defaults_are_the_hosts_the_remote_scope_and_each_registry_need`
`verified-by: bravebot_sandbox::proxy::a_listed_host_is_tunnelled_and_an_unlisted_one_is_refused`
`verified-by: bravebot_sandbox::proxy::a_denied_host_is_refused_although_an_allowed_entry_covers_it`
`verified-by: bravebot_sandbox::proxy::a_listed_host_is_refused_on_a_port_the_proxy_does_not_carry`
`verified-by: bravebot_sandbox::proxy::a_default_config_carries_tunnels_to_ports_443_and_80_only`
`verified-by: bravebot_sandbox::proxy::a_refusal_is_the_same_bytes_whatever_host_was_asked_for`
`verified-by: bravebot_sandbox::proxy::the_host_is_read_from_the_request_line_and_not_from_a_header`
`verified-by: bravebot_sandbox::proxy::a_request_that_is_not_a_connect_is_refused_without_a_decision`
`verified-by: bravebot_sandbox::proxy::every_decision_is_recorded_with_the_host_and_the_rule_that_decided_it`
`verified-by: bravebot_sandbox::hosts::only_a_host_name_or_an_address_may_be_recorded`
`verified-by: bravebot_sandbox::proxy::a_decision_is_handed_over_once`
`verified-by: bravebot_sandbox::proxy::decisions_beyond_the_cap_are_counted_and_not_kept`
`verified-by: bravebot_sandbox::proxy::a_request_for_something_that_is_not_a_host_name_is_recorded_without_it`
`verified-by: bravebot_sandbox::proxy::a_listed_name_that_resolves_to_loopback_is_refused`
`verified-by: bravebot_sandbox::proxy::a_name_the_list_allows_is_refused_when_it_resolves_to_an_address_that_is_not_public`
`verified-by: bravebot_sandbox::proxy::one_address_that_is_not_public_among_public_ones_refuses_the_name`
`verified-by: bravebot_sandbox::proxy::an_address_an_exact_entry_names_is_reached_although_it_is_not_public`
`verified-by: bravebot_sandbox::proxy::a_request_naming_an_address_that_is_not_public_is_refused_as_not_public`
`verified-by: bravebot_sandbox::proxy::the_tunnel_is_opened_to_the_address_that_was_checked`
`verified-by: bravebot_sandbox::hosts::an_address_is_named_only_by_an_exact_entry_no_denied_one_covers`
`verified-by: bravebot_sandbox::address::every_class_the_issue_names_is_not_public`
`verified-by: bravebot_sandbox::address::an_ipv4_address_in_ipv6_clothes_is_classified_as_the_ipv4_address`
`verified-by: bravebot_agent::confine::the_trail_names_the_hosts_asked_for_and_what_the_list_decided`
`verified-by: bravebot_agent::confine::the_trail_carries_no_byte_of_a_name_that_is_not_a_host`
`verified-by: bravebot_agent::confine::the_trail_names_the_entry_that_decided_and_a_session_without_a_list_records_nothing`
`verified-by: bravebot_cli::running::a_host_a_run_asked_for_is_recorded_with_what_the_list_decided`
`verified-by: bravebot_tui::status::host_lists_are_reported_by_count_and_file_and_never_by_host`
`verified-by: bravebot_config::sandbox_network::a_pinned_allowed_list_is_not_widened_by_the_persons_ask`
`verified-by: bravebot_agent::workspace::host_answers_are_shared_remembered_and_lifted_by_a_yes`
`verified-by: bravebot_agent::workspace::a_new_workspace_holds_no_host_answers`
`verified-by: bravebot_agent::confine::only_a_host_refused_for_want_of_an_entry_is_put_to_the_person`
`verified-by: bravebot_agent::confine::only_on_unlisted_ask_under_a_list_asks_about_hosts`
`verified-by: bravebot_agent::confine::a_granted_host_changes_the_list_the_next_line_is_held_to`
`verified-by: bravebot_agent::confine::a_granted_host_is_carried_by_the_proxy_and_a_denied_one_still_is_not`
`verified-by: bravebot_agent::tools::hosts_put_to_the_person_are_asked_once_and_the_answer_is_kept`
`verified-by: bravebot_agent::tools::a_refused_host_is_put_to_the_person_under_ask_and_not_for_a_delegate_or_under_refuse`
`verified-by: bravebot_agent::permission_mode::a_host_question_is_asked_in_every_mode_but_bypass_which_refuses`
`verified-by: bravebot_core::policy::a_host_answer_leaves_a_trail_that_tells_a_yes_from_a_no`
`verified-by: bravebot_tui::confirm::grants::a_host_prompt_shows_each_host_and_what_a_yes_does`
`verified-by: bravebot_tui::confirm::grants::a_host_list_longer_than_the_box_takes_no_yes_until_the_end_of_it_has_been_drawn`
`verified-by: bravebot_tui::remote_confirm::hosts_cross_with_every_name_and_only_their_own_yes_comes_back`
`verified-by: bravebot_cli::plain::hosts_are_asked_in_lines_and_only_a_yes_lets_programs_reach_them`
`verified-by: bravebot_ui_bridge::refusal::a_host_request_is_put_to_the_window_and_takes_no_answer_but_its_own`
`verified-by: bravebot_ui_bridge::refusal::a_host_answer_answers_no_other_question`
`verified-by: bravebot_ui_bridge::refusal::an_unanswerable_host_request_refuses`
`verified-by: bravebot_ui_bridge::dispatch::a_host_answer_for_no_waiting_question_is_refused_as_an_answer_to_nothing`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so the card is pinned instead by ui/scripts/host-card.test.mjs, which asserts that it lists every host as text, says how long a yes lasts and what deniedHosts still refuses, offers no standing answer, takes only a host answer, and that the main process lets host.reply through; ui/scripts/shown.test.mjs asserts that an approval waits on the hosts being on screen; make check-ui runs both)`
`verified-by: bravebot_ui_bridge::wire::a_host_prompt_carries_the_hosts_and_nothing_else`
`verified-by: bravebot_ui_bridge::acp::a_host_request_is_shown_with_each_host_on_a_row_and_its_escapes_pictured`
`verified-by: bravebot_ui_bridge::acp::a_host_request_offers_the_session_not_a_single_use`
`verified-by: bravebot_sandbox::proxy::the_environment_points_every_proxy_variable_at_the_loopback_port`
`verified-by: bravebot_sandbox::proxy::dropping_the_proxy_stops_it_listening`
`verified-by: bravebot_agent::host_proxy::a_stage_holds_the_defaults_its_own_reasons_bring_and_no_others`
`verified-by: bravebot_agent::host_proxy::a_denied_host_is_refused_although_a_default_covers_it`
`verified-by: bravebot_agent::host_proxy::a_denied_entry_that_is_no_rule_fails_and_an_allowed_one_is_dropped`
`verified-by: bravebot_agent::host_proxy::a_list_is_served_by_one_proxy_and_another_list_by_another`
`verified-by: bravebot_agent::confine::only_a_stage_with_egress_is_pointed_at_the_proxy_and_only_under_a_list`
`verified-by: bravebot_agent::confine::a_stage_with_a_host_list_is_pointed_at_the_proxy_that_applies_it`
`verified-by: bravebot_agent::confine::an_assignment_in_the_line_cannot_point_a_stage_elsewhere`
`verified-by: bravebot_agent::confine::no_allowed_list_means_no_proxy`
`verified-by: bravebot_agent::confine::a_stage_with_no_egress_is_given_no_proxy`
`verified-by: bravebot_config::settings::no_allowed_hosts_is_no_list_and_an_empty_one_is_a_list`
`verified-by: bravebot_config::settings::the_home_layer_may_write_every_host_key`
`verified-by: bravebot_config::settings::a_checkout_may_deny_a_host_and_never_allow_one`
`verified-by: bravebot_config::settings::a_checkouts_list_alone_does_not_start_filtering`
`verified-by: bravebot_config::settings::a_named_file_may_list_hosts_only_outside_the_workspace`
`verified-by: bravebot_config::settings::a_misshapen_host_key_is_reported_and_not_read`
`verified-by: bravebot_config::sandbox_network::a_pinned_allowed_list_replaces_the_persons`
`verified-by: bravebot_config::sandbox_network::a_pinned_empty_list_filters_a_session_that_set_none`
`verified-by: bravebot_config::sandbox_network::a_pinned_denial_is_added_and_a_pinned_answer_wins`
`verified-by: bravebot_config::sandbox_network::a_misshapen_pin_pins_nothing_and_is_reported`
`verified-by: bravebot_config::sandbox_network::no_managed_file_leaves_the_settings_as_they_are`

<a id="SANDBOX-25"></a>
### SANDBOX-25: a person may add to and take from what a stage reads and writes, with four lists of paths

A session carries four lists, each of paths or globs, in the settings file under `sandbox.filesystem`
and on the command line as `--sandbox-allow-read <path>`, `--sandbox-deny-read <path>`,
`--sandbox-allow-write <path>` and `--sandbox-deny-write <path>`, each repeatable and each adding to
what the settings say. `allowRead` lifts a refusal, one of the table in
[SANDBOX-12](#SANDBOX-12) included. `denyRead` adds a refusal to every stage. `allowWrite` adds a write
row that is read as well. `denyWrite` takes write access away under a path, a directory the
session was opened on included, and leaves what is read as it was. All four are empty by default and
a session that writes none is held to exactly the profile of [SANDBOX-18](#SANDBOX-18).

A narrower path beats a wider one, so an `allowWrite` beneath a `denyWrite` is a row that stands and
a `denyRead` beneath an `allowRead` is a refusal that holds. At one path a refusal wins over the
person's own allowance. A refusal also wins over the rows a stage brings for itself at or beneath it,
a credential scope's and a toolchain cache's among them: the person said no program reads `~/.config/gh`,
and a program that is given its scope is still a program. It wins over the rows of the run base and
the mode `workspace-write` as well, so a `denyRead` of `~/Library/Keychains/login.keychain-db` refuses the
file the base reads on macOS ([SANDBOX-12](#SANDBOX-12)), and a `denyWrite` of a cache refuses a row
every stage is given ([SANDBOX-18](#SANDBOX-18)). A refusal with no grant above it holds back
nothing and is not added.

`denyRead` and `denyWrite` are read from every layer, since they only take reach away. `allowRead`
and `allowWrite` are read from the person's own file, the file `--settings` names from outside the
workspace and the checkouts the session has been in, the command line and the managed file, and never from a project layer, a local layer or a
named file inside either, so a cloned repository cannot widen its own sandbox. A layer that
tried names the file in `doctor`. The managed file may pin each list: a pinned `allowRead` or
`allowWrite` is the whole list and the person's own entries are not read, and a pinned refusal is
added to the person's and cannot be lifted by an entry at or beneath it.

A path is absolute, `~/`-prefixed or relative to the first directory the session was opened on. An
entry is resolved to, and compared with a row in, one spelling, so that a Windows drive path with
the `\\?\` prefix the file system returns is the same path as the one without it. An entry that
climbs out of the directory it is read from, holds `..` where it is absolute, or starts with `~` in
a session that names no home directory is refused. Each path is judged where it leads: the
part of it that is on disk is resolved through its links. `~`, `/`, a drive root and the home directory
or any directory above it are refused as `allowWrite` rows, since a stage that wrote there would be
confined to nothing ([SANDBOX-2](#SANDBOX-2)). No list adds reach to `~/.ssh` or anything inside it,
which is where a private key is ([SANDBOX-16](#SANDBOX-16)), nor to `~/.bravebot` (or the directory `BRAVEBOT_HOME` puts elsewhere) or anything inside it, which is
where the gateway keys are ([SANDBOX-12](#SANDBOX-12)), except that an `allowRead` entry may name
`~/.bravebot/sessions` or a path inside it, which the run base reads already, and no list can set the `.git` writes
[SANDBOX-14](#SANDBOX-14) withholds, since none of them reaches that setting. On Windows, where a
grant cannot be narrowed, an `allowRead` or `allowWrite` entry that is a directory holding one of
the locations of [SANDBOX-12](#SANDBOX-12) beneath it (the home directory, a directory above it, or
`~/.config`, which holds `~/.config/gcloud`) is refused too, since a stage given that directory
would reach `~/.ssh`, `~/.aws` and `~/.bravebot` with it; a wildcard drops those matches and keeps the
rest. An entry at or inside such a location other than `~/.ssh` and `~/.bravebot` is not affected. A write row of the
person's above a location of the table does not write it, since Seatbelt's refusal of a read is not
a refusal of a write. `*` and `?` in an entry of `allowRead` or `denyRead` are expanded when a call's
confinement is built, by listing each directory once without following a link, within a bound of
entries and of depth; a glob that matches nothing is not an error, and one that reaches the bound
names nothing known and is refused. A file made after the listing is outside what a glob named. A
wildcard in a write list is refused.

An entry that is refused is not in force. A refused allowance leaves reach where it was. A refused
`denyRead` or `denyWrite` is a path the person meant to hold back, so a stage is not started while one
is, and the result names the key and the entry. Where the backend cannot subtract from a grant
(Windows) a stage the refusal would reach is refused as [SANDBOX-1](#SANDBOX-1) requires, and a
`denyRead` or `denyWrite` inside the directory the session was opened on, or inside an `allowRead` or
`allowWrite` entry, is reported as not in force, with why, by `doctor` and by the window, since a stage there is refused and none runs with the path
held back. Where it
grants a directory with everything beneath it (Landlock) a directory above a refused write is listed
and granted entry by entry as [SANDBOX-12](#SANDBOX-12) does for a read, which leaves that directory
without the right to make an entry directly in it: a program cannot create a new file beside a file
the person denied it writing.

Every stage of every line a turn starts is built by one function and so holds the lists, a stage
of a pipeline, of a line left running and of a delegate's turn alike, in the terminal and in a
window. `doctor` lists every entry with the file or flag that wrote it and each entry that is not in
force with why. `/status` gives how many entries each list holds and the files that wrote them. The
opening screen says only that the person's rules are in force. The run prompt says the counts once,
and the sentence a failed step carries says them. None of them names a path, since a glob's matches
are the machine's and a path is the person's.

The desktop window shows the four lists and edits nothing: the permissions view lists each entry
under the list it is in with the file that wrote it, or the command line, and an entry that is not
in force with the sentence `doctor` gives; the notice above a conversation counts an entry that is
not in force and an allowance a project file wrote, as it does a permission rule. It names paths
where the terminal's `/status` does not, since it is the person's own settings it is showing back
and `doctor` does the same. The person edits the settings file, and a change applies to the next
conversation. A command that adds an allowance from a session is not built; a planner's way to ask
for reach to one path for the session is `request_path` ([SANDBOX-28](#SANDBOX-28)), which adds no
entry to a list.

**Why.** The lists are how a person changes what a stage reads and writes without `/add-dir`, the
only other way to move that reach, which also marks the directory trusted
([TRUST-9](trust-map.md#TRUST-9)). Without them, a person with a file the program should not change,
or a directory it should not read, can only keep the program out of the session or trust more than
they meant to. Reach a checkout adds is reach the person did not choose, so only the lists that take
reach away are read from one, which is the rule `run.network` follows. A refusal that does not
outrank a stage's own rows is lifted by any stage that carries a scope for the path, a `gh` stage for
`~/.config/gh`, so a refusal wins over those rows. A refusal that cannot be applied is not dropped,
because the path it names is the one the person was protecting.

`verified-by: bravebot_sandbox::rules::a_name_is_met_by_a_star_and_a_question_mark_and_nothing_looser`
`verified-by: bravebot_sandbox::rules::an_entry_is_read_from_where_its_spelling_says`
`verified-by: bravebot_sandbox::rules::a_link_is_judged_by_where_it_leads`
`verified-by: bravebot_sandbox::rules::a_write_row_over_the_home_or_the_root_is_refused`
`verified-by: bravebot_sandbox::rules::an_entry_that_adds_reach_inside_ssh_is_refused`
`verified-by: bravebot_sandbox::rules::an_entry_that_adds_reach_inside_the_state_directory_is_refused`
`verified-by: bravebot_sandbox::rules::only_a_read_of_the_session_records_is_granted_in_the_state_directory`
`verified-by: bravebot_agent::reach::the_session_records_may_be_reached_for_reading_and_not_for_writing`
`verified-by: bravebot_sandbox::rules::a_row_over_a_directory_holding_a_credential_location_is_refused_on_windows_only`
`verified-by: bravebot_sandbox::rules::a_pattern_on_windows_leaves_out_the_matches_that_hold_a_credential_location`
`verified-by: bravebot_agent::confine::an_allow_read_over_the_home_directory_is_not_a_row_on_windows`
`verified-by: bravebot_sandbox::rules::a_glob_is_expanded_by_listing_and_applies_to_reads`
`verified-by: bravebot_sandbox::rules::a_glob_over_a_tree_deeper_than_the_walk_goes_is_refused_and_not_cut_short`
`verified-by: bravebot_sandbox::rules::a_match_is_judged_where_it_leads_and_the_directory_it_is_read_from_is_not_a_pattern`
`verified-by: bravebot_sandbox::rules::a_denial_decides_the_path_it_names_and_a_pinned_one_what_is_beneath`
`verified-by: bravebot_sandbox::rules::a_denial_beats_the_stages_own_rows_and_a_narrower_row_of_the_persons_stands`
`verified-by: bravebot_sandbox::rules::a_denial_with_no_grant_above_it_is_not_added`
`verified-by: bravebot_sandbox::rules::a_write_row_above_a_credential_location_does_not_write_it`
`verified-by: bravebot_sandbox::rules::a_denial_inside_a_grant_is_added_whichever_way_each_is_spelled`
`verified-by: bravebot_sandbox::rules::a_denial_outside_every_grant_is_not_added_in_either_spelling`
`verified-by: bravebot_sandbox::rules::a_row_at_or_under_a_refusal_is_dropped_whichever_way_each_is_spelled`
`verified-by: bravebot_sandbox::rules::an_entry_the_file_system_resolves_to_a_verbatim_path_is_kept_in_the_ordinary_spelling`
`verified-by: bravebot_sandbox::rules::a_verbatim_drive_path_loses_its_prefix_and_nothing_else_does`
`verified-by: bravebot_sandbox::rules::a_verbatim_path_that_is_not_text_is_not_taken_for_its_lossy_lookalike`
`verified-by: bravebot_sandbox::rules::a_stage_with_a_denial_inside_its_grant_is_refused_by_the_windows_backend`
`verified-by: bravebot_sandbox::policy::a_write_row_above_a_write_refusal_is_spread_around_it`
`verified-by: bravebot_sandbox::linux::a_stage_is_refused_a_write_the_policy_refuses_and_keeps_the_rest`
`verified-by: bravebot_sandbox::macos::the_profile_orders_every_row_from_the_widest_path_to_the_narrowest`
`verified-by: bravebot_sandbox::windows::a_policy_refusing_a_write_is_refused_rather_than_applied`
`verified-by: bravebot_sandbox::windows::a_policy_that_refuses_a_read_or_a_write_starts_no_program`
`verified-by: bravebot_sandbox::windows::capabilities_report_what_a_container_enforces`
`verified-by: bravebot_sandbox::rules::a_denial_beneath_a_grant_is_found_whichever_way_each_is_spelled`
`verified-by: bravebot_sandbox::rules::a_denial_beneath_a_persons_allowance_is_found_for_the_kind_of_row_it_cuts`
`verified-by: bravebot_agent::permissions::a_denial_the_backend_cannot_subtract_is_not_in_force_and_nothing_else_changes`
`verified-by: bravebot_agent::permissions::a_denial_beneath_an_allowance_is_not_in_force_where_the_backend_cannot_subtract`
`verified-by: bravebot_cli::doctor::doctor_reports_a_denial_the_backend_cannot_subtract_as_not_in_force`
`verified-by: bravebot_agent::confine::a_refusal_of_the_persons_is_not_lifted_by_the_scope_a_stage_carries`
`verified-by: bravebot_agent::confine::what_the_default_leaves_every_stage_is_withheld_by_the_persons_lists`
`verified-by: bravebot_agent::confine::every_kind_of_stage_holds_the_lists`
`verified-by: bravebot_agent::confine::the_counts_reach_the_description_and_the_profile_and_no_path_does`
`verified-by: bravebot_agent::confine::a_refusal_spelled_with_a_home_the_session_lacks_is_unapplied`
`verified-by: bravebot_agent::confirm::the_persons_filesystem_lists_are_said_once_and_only_where_they_exist`
`verified-by: bravebot_config::settings::the_home_layer_may_write_every_filesystem_list`
`verified-by: bravebot_config::settings::a_checkout_may_add_a_refusal_and_never_an_allowance`
`verified-by: bravebot_config::settings::a_named_file_may_widen_only_outside_the_workspace`
`verified-by: bravebot_config::settings::a_misshapen_filesystem_list_is_reported_and_a_blank_entry_is_left_out`
`verified-by: bravebot_config::settings::the_sandbox_block_is_unread_unless_it_holds_only_the_filesystem_lists`
`verified-by: bravebot_config::managed::the_filesystem_lists_are_pinnable`
`verified-by: bravebot_config::sandbox_filesystem::nobody_deciding_leaves_every_list_empty`
`verified-by: bravebot_config::sandbox_filesystem::a_flag_adds_to_the_settings`
`verified-by: bravebot_config::sandbox_filesystem::a_pinned_allow_list_replaces_the_persons`
`verified-by: bravebot_config::sandbox_filesystem::a_pinned_refusal_is_added_to_the_persons_and_marked`
`verified-by: bravebot_cli::main::the_sandbox_flags_are_taken_out_with_their_paths_into_their_lists`
`verified-by: bravebot_tui::status::filesystem_rules_are_reported_by_count_and_file_and_never_by_path`
`verified-by: bravebot_tui::logo::filesystem_rules_are_named_on_the_opening_screen_and_none_is_not`
`verified-by: bravebot_agent::confine::a_denied_read_is_refused_by_name_and_by_glob_and_its_neighbour_is_not`
`verified-by: bravebot_agent::confine::a_denied_read_holds_back_a_directory_the_base_reads`
`verified-by: bravebot_agent::confine::an_allowed_read_lifts_the_table_and_never_reaches_a_private_key`
`verified-by: bravebot_agent::confine::the_narrower_of_an_allowed_and_a_denied_write_decides`
`verified-by: bravebot_agent::confine::a_denied_write_holds_back_a_file_inside_a_session_directory`
`verified-by: bravebot_agent::confine::a_denied_read_written_through_a_link_holds_where_the_link_leads`
`verified-by: bravebot_agent::confine::a_denial_that_cannot_be_applied_stops_the_stage`
`verified-by: bravebot_cli::running::doctor_names_each_filesystem_rule_with_where_it_came_from`
`verified-by: bravebot_cli::running::a_denied_write_reaches_the_programs_a_run_starts`
`verified-by: bravebot_ui_bridge::sandbox_filesystem::a_denied_write_in_the_settings_reaches_the_programs_a_window_runs`
`verified-by: bravebot_ui_bridge::sandbox_filesystem::a_denied_write_in_the_settings_reaches_the_programs_an_editor_runs`
`verified-by: bravebot_ui_bridge::rules::the_filesystem_lists_are_reported_with_the_file_and_what_became_of_each`

<a id="SANDBOX-26"></a>
### SANDBOX-26: a `run` call may ask for a credential scope, a toolchain list or loopback by name, and the person is asked every time

`run` takes an optional `scopes`: an array of names from the fixed menu `remote`, `signing`, `aws`,
`kubernetes`, `docker`, `cargo`, `node`, `python`, `go`, `maven`, `gradle`, `loopback` and `ssh-key`. It is
for the line whose argv shows no operation to key a scope on ([SANDBOX-16](#SANDBOX-16)), a script
that runs `gh` or `cargo` inside it, or `git rebase` that signs what it rewrites, and for a line
that listens on a port of this machine ([SANDBOX-3](#SANDBOX-3)), `cargo test` over tests that
bind one or a build through `sccache`. `loopback` is the loopback grant and nothing else: it reads
no path, and a closed network ([SANDBOX-20](#SANDBOX-20)) stays closed for it. The planner asks for
it to listen, and to connect to a port of this machine only from a line that has no network access.
A line that has network access gains no connect from the grant: it connects without it, or is held
to a host list's proxy port with it or without it, and the grant gives it listening
([SANDBOX-3](#SANDBOX-3)). On Linux and Windows nothing reads the request, so it changes nothing
there, and it is still accepted and asked about. The `scopes` entry of the tool, which is the same
text on every platform, says this, and says when to connect in the words the description uses
([SANDBOX-19](#SANDBOX-19)). `ssh-key` is a
request for the person to choose again which ssh key a push offers its owner, for the line whose
push was refused for permission ([SANDBOX-16](#SANDBOX-16)): it lends nothing, reads no path, and
only has the run prompt number the keys for an owner already chosen. It is never remembered. Every other name is
the scope or toolchain list of the same name, with the rows, the egress under a closed network
([SANDBOX-20](#SANDBOX-20)) and the ssh agent socket that scope has when a stage's argv names it
(`remote` and `signing` both have one), and the places the environment moves its tool's configuration to
([SANDBOX-16](#SANDBOX-16)), each named in the prompt with its variable. It is added to every stage
of the line that has no `NAME=value` in front of it. A name outside the menu, compared exactly, is an error and nothing runs: `root`,
`Remote` and the empty string are not requests for less.

- Asked about every time. A line that asks is put to a person before any vouched entry, remembered
  line or rule is read ([RUN-5](tools/run.md#RUN-5)), and the prompt shows the names and the stage
  each was added to. It offers no `a` or `r`, records no entry and vouches for no program, since an
  entry is a program, its arguments and a tree, and holds none of what the line was lent. The only
  answers that last past the line are the two that remember the scope as reach ([SANDBOX-27](#SANDBOX-27)).
- What it prints is not trusted on a vouch's account: a line that asked for a scope has its output
  quarantined as an unvouched line's is ([RUN-4](tools/run.md#RUN-4)).
- Accepted in `strict`, `read-only` and `workspace-write` ([SANDBOX-22](#SANDBOX-22)). `workspace-write` reads the machine
  except the credential table ([SANDBOX-12](#SANDBOX-12)), so a scope there lifts the part of that table
  it names.
- Accepted and dropped where the stages of the line start with no profile whatever is asked: in a
  session whose mode is `off`, and in a turn that confines none of its runs. The line already runs
  with the person's own access, so there is nothing to lend. The `run` description does not mention
  `scopes` or `unconfined` there. It says that `request_path` is refused and nobody is asked
  ([SANDBOX-19](#SANDBOX-19)). The arguments are still checked, so a
  name outside the menu, a value that is not an array, and `scopes` with `unconfined` are errors in
  those sessions as in every other. The line is then put to the person as a line that asked for
  nothing is, with no names shown, its output is quarantined as an unvouched line's is, and the
  trail holds no refusal.
- Refused, with one sentence that does not say which setting withheld it, with no profile directory
  and in a workspace the person has not trusted ([TRUST-7](trust-map.md#TRUST-7)). The trail records
  the refusal under `sandbox`, naming `scopes` and each cause that held ([TRACE-1](trace.md#TRACE-1)).
  The person watching is told the same causes, in a note beside the planner's own sentence, which
  stays as it is; the two are written from the one list, so neither can name a cause the other does
  not. A run with nobody to ask refuses it, as it refuses any question; the mode that asks nothing
  approves it, as it approves any run.
- A stage with a `NAME=value` in front of it gets no requested scope, for the reason it gets no
  carried one. A person's deny rules still beat the rows it adds ([SANDBOX-25](#SANDBOX-25)).
- The trail records the names and the stage each was added to under `requested_scopes`, built from
  menu words and stage numbers and nothing the plan chose. The desktop sends the names as
  `requestedScopes`, beside the confinement it draws, and offers no standing answer for such a line.
- A delegate's `run` has the same argument, the same refusals and the same question.

**Why.** Scopes follow the operation an argv names, so a script that shells out to `gh` reaches
nothing, fails, and leaves the planner with a refusal it cannot tell from a fault. The remedy is to
let it say what it needs, and to put that to the person in the plan they read, since a name the
planner chose is a credential lent to a program a checkout may have written. Asking every time and
trusting no output is what keeps a name from becoming a standing allowance that the person approved
for a different line.

A request is asked about every time, remembered nowhere ([SANDBOX-27](#SANDBOX-27)) and kept from
the auto-mode classifier ([AUTO-3](auto-mode.md#AUTO-3)), whatever it lends. One that lends the line
nothing it needs, such as `loopback` for a connect from a line that has network access, costs the
person a question for each line. The description therefore says when `loopback` is asked for to
connect, and that it is not asked for from a line that has network access
([SANDBOX-19](#SANDBOX-19)).

`verified-by: bravebot_sandbox::scope::a_request_is_a_word_of_the_menu_and_nothing_near_it`
`verified-by: bravebot_sandbox::scope::the_menu_is_every_scope_and_every_toolchain`
`verified-by: bravebot_sandbox::scope::signing_can_be_requested_and_not_remembered`
`verified-by: bravebot_agent::confine::a_request_for_ssh_key_asks_again_for_an_owner_already_chosen_and_for_no_entry`
`verified-by: bravebot_agent::turn::a_request_for_ssh_key_asks_again_and_the_new_answer_replaces_the_old_choice`
`verified-by: bravebot_core::policy::a_vouched_line_that_asks_for_a_scope_is_asked_about_anyway`
`verified-by: bravebot_core::policy::a_remembered_line_that_asks_for_a_scope_is_asked_about_anyway`
`verified-by: bravebot_core::policy::a_requested_scope_leaves_a_trail`
`verified-by: bravebot_agent::tools::a_refused_scope_request_is_recorded_with_each_cause_that_held`
`verified-by: bravebot_agent::confine::a_requested_scope_lifts_its_own_directory_for_a_stage_that_names_none`
`verified-by: bravebot_agent::confine::a_requested_toolchain_brings_its_caches`
`verified-by: bravebot_agent::confine::a_stage_with_an_assignment_gets_no_requested_scope`
`verified-by: bravebot_agent::confine::a_requested_scope_or_toolchain_keeps_a_closed_network_for_its_stage`
`verified-by: bravebot_agent::confine::a_requested_loopback_lends_loopback_and_no_network`
`verified-by: bravebot_agent::confine::a_requested_loopback_sorts_last_and_the_prompt_names_its_stage`
`verified-by: bravebot_agent::confine::the_planner_is_told_on_macos_that_listening_needs_loopback`
`verified-by: bravebot_agent::confine::the_planner_is_told_of_loopback_only_what_the_sandbox_does_on_its_platform`
`verified-by: bravebot_agent::turn::in_auto_a_line_that_only_connects_to_a_local_port_needs_no_loopback_and_one_that_asks_goes_to_the_person`
`verified-by: bravebot_agent::confine::a_request_is_kept_once_in_the_menus_order`
`verified-by: bravebot_agent::confine::the_description_names_each_requested_scope_and_the_stage_it_is_for`
`verified-by: bravebot_agent::confine::a_strict_stage_that_asked_for_a_scope_reads_that_credential_only`
`verified-by: bravebot_agent::confine::a_stage_that_asked_for_signing_is_lent_the_key_and_the_agent`
`verified-by: bravebot_agent::confine::a_request_for_signing_over_a_key_no_scope_reads_is_explained`
`verified-by: bravebot_agent::tools::run_takes_one_command_line_and_nothing_else`
`verified-by: bravebot_agent::turn::scopes_are_asked_about_on_a_vouched_line`
`verified-by: bravebot_agent::turn::a_request_on_an_audited_line_quarantines_what_it_prints_and_the_trail_says_so`
`verified-by: bravebot_core::policy::a_line_that_asked_for_a_request_is_untrusted_by_every_road_and_the_trail_says_so`
`verified-by: bravebot_agent::turn::a_standing_answer_to_a_request_remembers_nothing`
`verified-by: bravebot_agent::turn::scopes_are_accepted_and_dropped_under_off`
`verified-by: bravebot_agent::tools::a_request_where_no_profile_applies_is_accepted_and_dropped`
`verified-by: bravebot_agent::turn::scopes_are_asked_about_and_run_under_workspace_write`
`verified-by: bravebot_agent::turn::scopes_are_refused_in_a_workspace_the_person_declined_to_trust`
`verified-by: bravebot_agent::turn::scopes_are_refused_unattended_unless_the_mode_that_asks_nothing_was_given`
`verified-by: bravebot_agent::turn::a_scope_outside_the_menu_is_an_error`
`verified-by: bravebot_ui_bridge::wire::a_run_prompt_carries_what_the_planner_asked_the_line_to_be_lent`

<a id="SANDBOX-27"></a>
### SANDBOX-27: a person may answer a request for a credential scope by remembering it as reach for the programs it was added to

At the confirmation for a line that asked for a credential scope ([SANDBOX-26](#SANDBOX-26)), `m`
runs the line and remembers each requested scope, read only, for the session, and `e` does the same
for every session started in this checkout. What is remembered is what `/reach` writes
([SANDBOX-23](#SANDBOX-23)): one grant for each distinct program and operation word among the line's
stages and each requested scope, so the next plan for those programs carries the scope as a row the
person reads, and a session that asks for nothing is not asked about it.

- Offered only where a record of reach can be written (a session that keeps state and is not
  incognito) and every stage that was given the scope can be keyed: it has an operation word, or no
  argument. A stage with a `NAME=value` in front of it was given nothing and is left out; a stage that
  starts with an option has no operation to key on, so the line is offered neither key. The keys are
  unbound where they are not drawn, and the acting layer works the shapes out again from the plan and
  the closed scope table rather than from what was drawn.
- A toolchain list, loopback and `ssh-key` are never remembered as reach, whether asked for alone or beside a scope,
  and nor is the `signing` scope, which `/reach` has no name for. The prompt says so.
- The line that asked is still asked about every time, with the remembered scope shown on it, and
  neither key vouches for a program, records the line or stops the asking; what it prints is
  quarantined as it was ([SANDBOX-26](#SANDBOX-26)).
- A grant for a program the scope table gives that scope to (`gh` and the remote scope) is for
  every checkout. Every other grant is bound to the checkout the answer was given in
  ([SANDBOX-23](#SANDBOX-23)). The prompt names the programs, the scopes, the lifetimes and the file
  the record is written to, and the keys wait for those rows to have been on the screen. A refusal
  or an interrupt remembers nothing.
- The desktop offers neither key. `/reach remove <number>` forgets one.

**Why.** A build whose script shells out to `gh` needs the scope in every session, and without a way
to keep it the planner asks again each time and the person reads the same prompt each time. Writing
the grant the person could have typed with `/reach`, and nothing wider, makes the answer the same
act as the command, shown in the same words, and keeps the planner's name for a scope from becoming
more than the person read.

`verified-by: bravebot_agent::reach::a_kept_request_leaves_out_an_assigned_stage_and_refuses_an_option_first_stage`
`verified-by: bravebot_agent::reach::a_kept_request_binds_to_a_checkout_unless_the_table_gives_the_program_the_scope`
`verified-by: bravebot_agent::turn::keeping_a_request_for_the_session_remembers_the_scope_for_that_session_only`
`verified-by: bravebot_agent::turn::keeping_a_request_for_every_session_is_read_by_another_session`
`verified-by: bravebot_agent::turn::keeping_a_request_leaves_a_toolchain_out`
`verified-by: bravebot_agent::turn::a_request_for_signing_is_asked_about_and_never_remembered`
`verified-by: bravebot_agent::turn::keeping_a_request_writes_nothing_where_it_was_not_offered_or_was_refused`
`verified-by: bravebot_tui::confirm::run::the_keep_keys_approve_and_remember_the_reach_only`
`verified-by: bravebot_tui::confirm::run::the_keep_keys_are_unbound_where_the_prompt_does_not_offer_them`
`verified-by: bravebot_tui::confirm::run::the_keep_keys_wait_for_the_rows_saying_what_they_remember`
`verified-by: bravebot_tui::confirm::run::a_prompt_offering_to_keep_a_request_names_what_would_be_kept`

<a id="SANDBOX-28"></a>
### SANDBOX-28: a planner may ask for reach to one path, and the person is asked every time it is new

`request_path` ([tools/request-path.md](tools/request-path.md)) is how a planner whose stage fails
for want of a path outside the session's directories asks for it. It takes the `path`, a `write`
flag (write implies read) and the `why` every tool takes. The person is shown the path as it
resolves, whether it is to be read or written, and the reason, and answers. A yes lets every program
a later `run` starts read the path, or read and write it, for the rest of the session. A no, an
interrupt and a run with nobody to ask grant nothing. The mode that answers every permission
question answers this one with a yes, as it does a line's ([SANDBOX-26](#SANDBOX-26)).

- **The same refusals as an `allowWrite` row** ([SANDBOX-25](#SANDBOX-25)), for a read as for a
  write: `~`, `/`, a drive root, the home directory and any directory above it, `~/.ssh`,
  `~/.bravebot` (or the directory `BRAVEBOT_HOME` puts elsewhere) and anything inside either (a request to read `~/.bravebot/sessions` or a path
  inside it is not refused), a credential location of the table
  ([SANDBOX-12](#SANDBOX-12)) and any directory that holds one (`~/.config` holds
  `~/.config/gcloud`), any path holding a control character, and any path holding `*` or `?`. A
  request for writing is also refused at, inside or above a persistence location
  ([SANDBOX-18](#SANDBOX-18)); a request to read one is not. The path is judged where it leads,
  through its links. A refused request is not put to the person, and the result says that no answer
  would change it. A path that does not exist is not asked about either.
- **The person's own refusals still hold.** A `denyRead` or `denyWrite` entry covering the path
  refuses the request, and one narrower than a granted path takes its part of the grant back, since
  the person's rows are applied after the session's.
- **It is reach and not trust.** Nothing is marked trusted ([TRUST-9](trust-map.md#TRUST-9)) and no
  file is written; the grant is held by the session and gone with it. `/clear`, `/branch` and
  `/resume` begin another session in the same process, and a run started after one is asked again
  for a path granted before it. Asking again for a path
  already held is asked like the first request, and a yes only ever widens the grant held: a read
  answered beside a write held leaves the write. The person may end a grant with
  `/reach paths remove <number>`.
- **Not accepted** where the mode is `off`, which has no profile to add to, and in a workspace that
  is not trusted ([TRUST-7](trust-map.md#TRUST-7)). A request to write is not accepted under
  `read-only` ([SANDBOX-22](#SANDBOX-22)), whatever the permission mode, and a request to read is.
  The result says so and nothing is asked. A grant for writing held when the mode becomes `read-only` gives its read and not its
  write for as long as the mode lasts, and gives the write again when the mode returns to
  `workspace-write`, since it is still held. No
  settings layer, project file or permission rule grants one in advance, because none names it.
- **Shown and recorded.** `/status` lists each path held, with its access, and the
  trace carries one `path_reach` record for each yes, naming the path and the access.
- **Judged again when a stage starts.** The grant records the device and inode of what was at
  the path, read without following a link. Each time a stage's policy is built the row is judged
  again, and it is given no row, with the stage still started, when something is at the path and
  the name is a link, a parent of it is a link, it is not the device and inode approved, or it is
  refused as a new request for the same access would be: at, inside or above a credential location
  ([SANDBOX-12](#SANDBOX-12)), `~/.ssh`, `~/.bravebot` (or the directory `BRAVEBOT_HOME` puts elsewhere), a persistence location for a write
  ([SANDBOX-18](#SANDBOX-18)), or the home directory and any directory above it. A name with
  nothing at it keeps its row, since a row opens nothing. A grant made while nothing was at the
  name has no identity to compare, so the directory made there later is judged as each stage
  starts, as a new request for the same access would be. The grant is still held and listed by
  `/status`, so a name restored to the directory approved gives its row again, and the count the
  profile states is of the rows given. The comparison reads file system metadata and the path
  alone. A directory replaced by another ordinary directory, or by a link to one, loses its row
  until the person asks again. The check is made when the policy is built, so a name changed after
  that and before the program opens it is not caught by this clause; the backend opens a row by the
  path it was given. On Linux that is a handle that follows links, so a row opened after its name
  became a link is attached to the link's target, except a toolchain row (a cache, an install or a configuration file), which the policy
  marks as judged and the backend opens without following a link in its last component
  ([SANDBOX-15](#SANDBOX-15)); a link above the last component is still followed. On macOS the profile matches the path a process
  ends up opening, so a row whose name became a link gives the target nothing. Both are recorded
  by tests in `crates/sandbox`, the Linux one built and run only on Linux. The Windows backend
  does not refuse a link at a judged row. It writes its entry through the name, which the platform
  follows, so a name that became a link after the policy was built and before the entry is written
  is given the entry at its new target. The judgement made as each stage's policy is built is the
  only check there, and a test of this on Windows does not exist. This is the same judgement a directory given by
  `/reach` receives ([SANDBOX-23](#SANDBOX-23)). The session holds the grant only, so a `git
  switch` that changes what is at the path leaves the grant in place and the row follows the
  directory approved.
  The comparison with the credential and persistence locations folds case where the platform's
  names are not case sensitive (macOS and Windows), so a name spelled `.SSH` is refused as `.ssh`
  is, and does not where they are (Linux).
  `verified-by: bravebot_agent::confine::a_path_let_reach_is_given_no_row_once_its_name_leads_elsewhere`
  `verified-by: bravebot_agent::confine::a_path_let_reach_is_compared_with_the_credential_locations_as_the_platform_does`
  `verified-by: bravebot_agent::confine::a_path_let_reach_does_not_reach_its_parent_or_a_sibling`
  `verified-by: bravebot_agent::confine::a_path_let_reach_made_before_it_existed_is_judged_when_a_stage_starts`
  `verified-by: bravebot_sandbox::macos::a_granted_row_replaced_by_a_link_gives_its_target_nothing`
  `verified-by: bravebot_sandbox::linux::a_granted_row_replaced_by_a_link_is_attached_to_what_the_name_leads_to_when_opened`
  `verified-by: bravebot_sandbox::linux::a_judged_row_that_became_a_link_is_refused_and_one_not_judged_is_followed`
  `verified-by: bravebot_sandbox::policy::a_row_marked_as_judged_stays_marked_through_the_resolution`
  `verified-by: bravebot_agent::confine::a_cache_relinked_to_a_credential_or_a_startup_file_is_given_no_row`
  `verified-by: bravebot_agent::confine::a_cache_relinked_between_two_stages_is_judged_again_on_every_route_to_its_row`
  `verified-by: bravebot_sandbox::linux::a_cache_moved_aside_and_linked_to_the_keys_by_one_stage_reaches_nothing_in_the_next`
- **Not delegated.** A delegate is offered no `request_path`, and one it names anyway is answered
  as an unknown name ([DELEGATE-12](delegation.md#DELEGATE-12)), since a path asked for from inside
  a sub-task is reach the person never set up.
- The desktop shows no card for it and refuses, which grants nothing.
- **Under bypass, nothing needs asking for.** Where the permission mode answers every question
  ([MODE-4](permission-modes.md#MODE-4)), the mode is `workspace-write`, the workspace is trusted
  ([TRUST-7](trust-map.md#TRUST-7), [TRUST-26](trust-map.md#TRUST-26)) and the platform is macOS or
  Linux, the stages of a `run` are given at the start what a request for writing
  would be granted, those of a delegate included: every path the account can write, except `/`, a drive root, the home
  directory and any directory above it as a whole (the entries already in them stay writable, and
  no new entry is made directly in one), `~/.ssh`, `~/.bravebot` (or the directory `BRAVEBOT_HOME` puts elsewhere) and everything inside either, each
  credential location of the table ([SANDBOX-12](#SANDBOX-12)) and any directory that holds one,
  each persistence location ([SANDBOX-18](#SANDBOX-18)) and any directory that holds one, and every
  path the person's `denyRead` or `denyWrite` covers. Paths are judged where they lead,
  through links. The file tools are given the same reach by the mode itself
  ([PATHREQ-7](tools/request-path.md#PATHREQ-7)) and a `request_path` under bypass still opens a
  directory for them. A delegate is lent the mode and is offered no `request_path`
  ([DELEGATE-12](delegation.md#DELEGATE-12)), and its stages are given the rows as the lead
  session's are, less the directories its checkout holds it out of: a delegate in a checkout is given
  no row for the working directory or for another checkout ([CHECKOUT-7](checkouts.md#CHECKOUT-7)).
  Under `strict` each path is
  still requested and a delegate has no way to request one, under `off` there is no profile, in a
  workspace that is not trusted no request is accepted
  ([PATHREQ-5](tools/request-path.md#PATHREQ-5)) and the stages keep the session's directories, and
  on Windows, where a grant is an
  access-control entry written onto each directory, each path is still requested. Nothing a
  program wrote or printed decides which rows exist. The trace's `sandbox` gate says the stages
  were given what a request would be granted, `/status` says it in its `Programs write` line, and
  the profile line a failed step carries names it. Where the stages are given none of the rows,
  none of the three says anything about what a request would be granted, and `/status` says
  that the running turn holds the answer while a turn holds the trust map.

**Why.** A stage that cannot write a path the work needs fails with `Operation not permitted`, and
the planner has no way to say which path it needed. Without a per-path request, the only way to let
a build write one file is `/add-dir`, which marks the whole directory trusted. A request for one
path, with the same refusals as the rows a person can write, lets them say yes to that path for a
single session. What the planner supplies decides nothing: the path is routing and is judged by the
rules, and the reason is drawn for the person and recorded.

Under bypass nobody is there to answer, and a run that fails with `Operation not permitted` on a
sibling directory costs the planner a turn to learn what the profile already knows it will be
granted. Giving those stages the rows a yes would have given them removes the failed run and gives
them nothing a request would not: the same refusals hold, and the permission mode already answers
the request with a yes. In a workspace that is not trusted a request is refused, so there is no yes
for the rows to stand in for and the stages are given none of them. It is the one way the permission
mode widens a sandbox mode ([SANDBOX-22](#SANDBOX-22)).

**What it costs.** A program that writes anywhere the account can reaches more than the session's
directories, including a unix socket under such a row on macOS, where a network-outbound row
follows a write row. A program directory the account owns or can write, such as a Homebrew prefix, is
left out of the places bravebot finds its own helpers ([SANDBOX-30](#SANDBOX-30)) once a stage's
policy holds it, and stays left out for the life of the process, after the permission mode
changes. The rows are read from the directories as they are when a stage's policy is built, so a
directory made inside a directory spread around a refusal by an earlier stage of the same run is
not covered until the next. A program directory on the account's search path, such as
`~/.local/bin`, `~/.cargo/bin` or a Homebrew prefix, is written like any other, so a program a stage
leaves there runs unconfined when the person's shell finds it before the one they meant; refusing
them would refuse `pip install --user`, `cargo install` and `brew` under bypass. A stage cannot run
`git config --global`, since git's configuration is a persistence location. A row of a `request_path`
grant is also withheld while its name leads somewhere other than where the person approved
(above), and the grant stays listed. This and the refusals above are the whole of what is held
back.

`verified-by: bravebot_sandbox::rules::a_request_is_refused_where_an_allow_write_entry_is`
`verified-by: bravebot_sandbox::rules::a_request_at_inside_or_above_a_credential_location_is_refused`
`verified-by: bravebot_sandbox::rules::a_request_for_an_existing_path_is_kept_and_a_link_is_judged_where_it_leads`
`verified-by: bravebot_sandbox::rules::a_request_for_a_name_that_is_not_utf8_is_kept_as_those_bytes`
`verified-by: bravebot_sandbox::rules::a_request_under_a_denial_of_the_person_is_refused`
`verified-by: bravebot_sandbox::rules::granted_rules_add_rows_and_a_denial_of_the_person_still_wins`
`verified-by: bravebot_agent::confine::a_path_the_person_let_programs_reach_is_held_and_their_own_deny_still_applies`
`verified-by: bravebot_agent::workspace::path_reach_is_numbered_shared_upgraded_and_ended`
`verified-by: bravebot_agent::workspace::a_new_workspace_holds_no_path_reach`
`verified-by: bravebot_agent::workspace::a_session_ending_in_this_process_takes_its_reach_and_host_answers_with_it`
`verified-by: bravebot_agent::confine::a_path_let_reach_does_not_reach_its_parent_or_a_sibling`
`verified-by: bravebot_agent::confine::a_write_grant_held_before_read_only_writes_nothing_until_the_mode_returns`
`verified-by: bravebot_agent::reach::reach_paths_lists_ends_and_leaves_other_words_alone`
`verified-by: bravebot_agent::tools::request_path_is_offered_with_run_and_takes_a_path_a_flag_and_a_reason`
`verified-by: bravebot_agent::tools::a_delegate_that_names_request_path_is_told_no_such_tool_and_nobody_is_asked`
`verified-by: bravebot_agent::turn::a_yes_to_a_path_lets_a_program_write_it_and_a_read_only_yes_does_not`
`verified-by: bravebot_agent::turn::under_read_only_a_request_to_write_is_refused_and_one_to_read_is_not`
`verified-by: bravebot_sandbox::rules::what_is_written_without_a_request_leaves_out_the_home_directory_and_what_holds_a_credential`
`verified-by: bravebot_sandbox::rules::what_is_written_without_a_request_leaves_out_where_a_write_runs_later`
`verified-by: bravebot_sandbox::rules::what_is_written_without_a_request_gives_a_link_beside_the_home_no_row`
`verified-by: bravebot_sandbox::rules::a_request_to_write_where_a_write_runs_later_is_refused_and_one_to_read_it_is_not`
`verified-by: bravebot_agent::confine::a_bypass_stage_holds_the_rows_a_request_would_be_granted_and_not_the_home_directory`
`verified-by: bravebot_agent::confine::a_stage_outside_workspace_write_bypass_on_the_lead_session_is_given_no_extra_row`
`verified-by: bravebot_agent::confine::a_bypass_stage_keeps_the_persons_denied_write_and_says_what_it_writes`
`verified-by: bravebot_agent::confine::a_bypass_stage_cannot_write_where_the_person_denied_a_read`
`verified-by: bravebot_agent::tools::a_session_in_bypass_is_given_what_a_request_would_be_and_so_is_its_delegate`
`verified-by: bravebot_agent::confine::a_bypass_stage_of_a_delegate_in_a_checkout_is_given_no_row_for_the_working_directory`
`verified-by: bravebot_agent::turn::a_worker_of_a_bypass_turn_reaches_a_sibling_directory_and_one_of_an_asking_turn_does_not`
`verified-by: bravebot_agent::confine::a_bypass_stage_writes_beside_the_working_directory_without_a_request`
`verified-by: bravebot_agent::confine::a_bypass_stage_still_cannot_write_a_credential_a_file_run_later_or_a_new_entry_in_home`
`verified-by: bravebot_agent::confine::a_bypass_stage_keeps_a_denied_write_and_strict_keeps_the_request`
`verified-by: bravebot_agent::turn::a_bypass_run_writes_beside_the_session_without_a_request_and_the_trail_says_so`
`verified-by: bravebot_agent::turn::in_an_untrusted_workspace_a_bypass_run_is_given_no_write_a_request_would_add`
`verified-by: bravebot_agent::turn::a_delegate_of_a_confining_turn_cannot_write_outside_the_session`
`verified-by: bravebot_core::policy::the_trail_says_the_programs_were_given_what_a_request_would_be`
`verified-by: bravebot_tui::status::the_report_says_programs_write_what_a_request_would_be_granted_only_where_they_do`
`verified-by: bravebot_agent::turn::a_yes_to_a_path_marks_nothing_trusted_and_is_recorded`
`verified-by: bravebot_agent::turn::a_path_that_is_refused_as_a_row_is_refused_as_a_request_and_not_asked`
`verified-by: bravebot_agent::turn::a_path_is_not_asked_for_under_off_or_in_an_untrusted_workspace`
`verified-by: bravebot_agent::turn::a_path_needs_a_yes_from_the_person_or_the_mode_that_asks_nothing`
`verified-by: bravebot_agent::turn::the_tool_is_offered_where_runs_are_confined`
`verified-by: bravebot_tui::confirm::grants::a_path_prompt_shows_the_path_the_access_the_reason_and_what_a_yes_does_not_do`
`verified-by: bravebot_tui::confirm::grants::a_path_longer_than_the_box_takes_no_yes_until_the_end_of_it_has_been_drawn`
`verified-by: bravebot_tui::status::the_report_lists_the_paths_programs_were_let_reach`
`verified-by: bravebot_cli::plain::a_path_is_asked_in_lines_and_only_a_yes_lets_programs_reach_it`
`verified-by: bravebot_ui_bridge::refusal::a_request_for_a_path_is_refused_with_no_card_whatever_the_window_would_say`

<a id="SANDBOX-29"></a>
### SANDBOX-29: a `run` call may ask for one line to start with no sandbox, and the person is asked every time

`run` takes an optional `unconfined`, a boolean. With `true`, the stages of that one line start with
no profile, as under the mode `off` ([SANDBOX-22](#SANDBOX-22)), and the next line is built from the
session's mode again. It is for the line that has to write where a credential scope reads and never
writes ([SANDBOX-16](#SANDBOX-16)), such as `docker login` into `~/.docker/config.json` or
`aws sso login` into `~/.aws/sso/cache`, which `request_path` and `allowWrite` refuse for the same
reason ([SANDBOX-28](#SANDBOX-28), [SANDBOX-25](#SANDBOX-25)), and for the line that redirects its
output into a file outside the workspace, which a person asked for and the file tools refuse. A value that is not a boolean is an
error and nothing runs. `unconfined` with `scopes` is an error, since a line with no profile has
none to add to.

- **Asked every time.** The line is put to a person before any vouched entry, remembered line or
  rule is read ([RUN-5](tools/run.md#RUN-5)). The prompt says the line runs with no sandbox and
  offers no answer that lasts: not `a`, `r` or `f`, and not `m` or `e` ([SANDBOX-27](#SANDBOX-27)).
  An answer that names one anyway is not acted on, so the line is not vouched for and not recorded.
- **Not approved by the mode that asks nothing.** `--dangerously-skip-permissions` approves a
  `scopes` request ([SANDBOX-26](#SANDBOX-26)) and does not approve this one, since the permission
  mode never widens the sandbox mode ([SANDBOX-22](#SANDBOX-22)) and an approved request is `off`
  for its line. It goes to the confirmer in that mode as in every other, and a run with nobody to
  ask refuses it. A redirection into a directory the permission mode reaches
  ([PATHREQ-7](tools/request-path.md#PATHREQ-7)) needs no such request.
- **Accepted and dropped where the line starts with no profile anyway:** under `off` and in a turn
  that confines none of its runs. Nothing is removed that was there, so the line is put to the
  person as an ordinary line is, its output is quarantined, and no refusal is recorded. The value is
  still checked, so one that is not a boolean, or `unconfined` with `scopes`, is an error there too.
- **Refused, with one sentence that does not say which setting withheld it,** in `strict`, in
  `read-only`, in a workspace the person has not trusted ([TRUST-7](trust-map.md#TRUST-7)), under a
  managed `run.network` of `closed`, for the reason the managed file floors `off` there
  ([SANDBOX-22](#SANDBOX-22)), and in a delegate's `run`, since a sub-task asking to run unconfined
  is reach the person never set up. A managed `sandbox.mode` of `strict` is the mode `strict`.
  The sentence does not suggest the argument: the planner learns it from the `run` description. The
  trail records the refusal under `sandbox`, naming `unconfined` and each cause that held
  ([TRACE-1](trace.md#TRACE-1)), and the person watching is told the same causes in a note beside
  the planner's unchanged sentence, so they can tell which setting to change without reading the
  trail.
- **A line not approved is recorded as not run.** The `sandbox` entry saying the programs ran
  `unconfined` is written only once the person has approved the line. A line they do not approve
  leaves a refusal under `approval` instead ([TRACE-1](trace.md#TRACE-1)).
- **What it prints is quarantined** as an unvouched line's output is ([RUN-4](tools/run.md#RUN-4)).
- **A redirection may leave the workspace.** `printf 'howdy\n' > ../hello.txt` writes the file when
  the person approves the line. The confinement that keeps a redirection's destination inside the
  workspace and the directories opened beside it ([CMDLINE-5](tools/command-line.md#CMDLINE-5))
  stands for a line that has a profile, since this process opens the destination in place of a
  confined stage; a line with none has nothing for it to hold back. The prompt lists the
  destination, a `deny` rule on it still refuses the line, including through a link an earlier part
  of the line left ([CMDLINE-5](tools/command-line.md#CMDLINE-5)), and the file tools stay confined
  ([TRUST-10](trust-map.md#TRUST-10)). Under `off` the same holds for every line, since the mode has
  no profile to remove and so cannot accept this request.
- **The planner is told, where the session accepts it.** A write the file tools refuse for leaving
  the workspace, a redirection refused for the same reason, and a `request_path` for writing that
  is refused unasked each name this argument as the route, and say the person is asked. They name
  it only where the request would be accepted and the front end can ask about it, which the
  terminal interface and a session in lines can, and the desktop and a run with nobody to ask
  cannot. So a mode that keeps the profile, or a window that refuses the line, is never pointed at
  an argument it would turn down. A `request_path` refusal also says why the path cannot be lent.
  The `run` description of the argument makes it a second attempt: every line starts without it, and
  the planner asks for it only after the same line failed because the sandbox refused it access, not
  after a failure of another kind, saying in `why` what was refused. It also says that the person is
  asked every time, and that a session which does not accept it refuses the call with a sentence
  saying so. Each sentence after a failed step that names the argument says the same: it is the way to
  run the line again, with the refusal given in `why`. On macOS that is the sentence after a failed
  step that says only git writes a `.git`, which adds that a script which runs git itself can be run
  again with the argument, in the same sessions. It is also a sentence, said in the same sessions
  after a failed line with a step that runs `open` or `osascript`, that the default profile refuses
  LaunchServices and Apple Events, so launching an application, opening a URL in one or scripting
  one needs the line run again with the argument.
- **Accepted after a failure.** The driver accepts the request only while a line has run with the
  sandbox in this turn and exited non-zero, and each such failure earns one request: accepting one
  spends it, so a request the person declines, or a plan the policy then refuses, leaves it. The
  failed line need not be the line the request names. A request with none earned is refused before
  anybody is asked, with a sentence that says to run the line with the sandbox first, and the trail
  records the refusal under `sandbox` naming that cause, which the person watching is also told in a
  note beside the sentence. The count comes from
  exit codes and whether the deadline stopped the line, and nothing a program printed is read. A line
  stopped at its deadline or by the person, a line that ran with no sandbox and a line moved to the
  background earn nothing. A line whose own redirection leaves the workspace is accepted without one, since the
  sandbox refuses it before anything runs and so no run can fail at it.
- **Recorded.** The `sandbox` gate's detail names `unconfined` for that run, where it names the
  session's mode for every other ([TRACE-1](trace.md#TRACE-1)).
- The desktop draws no card for it and refuses, since a window reads `off` as `workspace-write` and has
  no line showing that a program is unconfined ([SANDBOX-22](#SANDBOX-22)).

**Why.** Without it the only way to run `docker login` or `aws sso login` from a session is to start
it with `--sandbox off`, which removes the profile from every program for the whole session. The
routing field is the boolean beside the command line the person already reads, and "run this line
with no sandbox" is a sentence a person can approve on its own. Asking every time and remembering
nothing keeps the request from becoming a standing allowance approved for a different line.

A file outside the workspace is the same request. The person sees the line and the file it writes.
Refusing the redirection would protect nothing, since a program in the same approved line can write
the file, and would leave a person who asks for `../hello.txt` with no route that ends in a prompt.
The planner is pointed at the argument because it has no other way to learn it when the file tools
refuse the path.

`verified-by: bravebot_agent::tools::an_unconfined_request_is_accepted_only_where_every_condition_holds`
`verified-by: bravebot_agent::tools::a_refused_unconfined_request_is_recorded_with_each_cause_that_held`
`verified-by: bravebot_agent::tools::a_request_for_no_sandbox_is_accepted_only_after_a_confined_line_failed`
`verified-by: bravebot_agent::tools::each_confined_failure_earns_one_request_for_no_sandbox`
`verified-by: bravebot_agent::tools::a_request_for_no_sandbox_the_person_did_not_accept_leaves_the_earned_one`
`verified-by: bravebot_agent::tools::a_redirection_out_of_the_workspace_is_accepted_with_no_failure_counted`
`verified-by: bravebot_agent::turn::a_line_stopped_at_its_deadline_earns_no_request_for_no_sandbox`
`verified-by: bravebot_agent::turn::a_request_for_no_sandbox_before_a_confined_line_failed_is_refused_unasked`
`verified-by: bravebot_agent::tools::only_a_managed_closed_network_refuses_an_unconfined_request`
`verified-by: bravebot_agent::tools::run_takes_one_command_line_and_nothing_else`
`verified-by: bravebot_agent::confine::a_failed_step_on_macos_names_the_unconfined_line_for_git_only_where_it_is_accepted`
`verified-by: bravebot_agent::confine::a_failed_open_or_osascript_step_on_macos_names_the_unconfined_line_only_where_it_is_accepted`
`verified-by: bravebot_agent::confine::each_failed_step_sentence_naming_unconfined_makes_it_the_second_attempt`
`verified-by: bravebot_agent::tools::a_redirection_may_leave_the_workspace_only_where_the_line_has_no_profile`
`verified-by: bravebot_agent::turn::an_approved_unconfined_line_may_redirect_into_a_file_outside_the_workspace`
`verified-by: bravebot_agent::turn::a_line_under_off_may_redirect_into_a_file_outside_the_workspace`
`verified-by: bravebot_agent::turn::a_refused_write_outside_the_workspace_names_the_unconfined_line_only_where_it_is_accepted`
`verified-by: bravebot_agent::turn::a_refused_path_request_says_why_and_points_a_write_at_the_unconfined_line`
`verified-by: bravebot_agent::turn::an_approved_unconfined_line_runs_with_no_profile_and_the_next_line_is_confined_again`
`verified-by: bravebot_agent::turn::an_unconfined_line_is_asked_about_on_a_vouched_line_and_remembers_nothing`
`verified-by: bravebot_agent::turn::a_request_on_an_audited_line_quarantines_what_it_prints_and_the_trail_says_so`
`verified-by: bravebot_core::policy::a_line_that_asked_for_a_request_is_untrusted_by_every_road_and_the_trail_says_so`
`verified-by: bravebot_agent::confine::each_failed_step_sentence_naming_unconfined_makes_it_the_second_attempt`
`verified-by: bravebot_agent::tools::run_takes_one_command_line_and_nothing_else`
`verified-by: bravebot_agent::turn::the_mode_that_asks_nothing_does_not_approve_an_unconfined_line`
`verified-by: bravebot_agent::turn::an_unconfined_line_is_refused_where_the_session_does_not_accept_it`
`verified-by: bravebot_agent::turn::an_unconfined_value_that_is_not_a_boolean_is_an_error`
`verified-by: bravebot_tui::confirm::run::a_run_asking_to_be_unconfined_says_so_and_binds_no_lasting_key`
`verified-by: bravebot_ui_bridge::refusal::the_window_refuses_an_unconfined_run_without_asking`

<a id="SANDBOX-32"></a>
### SANDBOX-32: a person may remember a reviewed prefix for an unconfined line, for programs on a hand-written list only

Not built. Until it is, [SANDBOX-29](#SANDBOX-29) holds as written and every unconfined request is
asked about. Once built, this clause amends SANDBOX-29's "asked every time", the list of answers
[RUN-5](tools/run.md#RUN-5) names, and the exception [RUN-20](tools/run.md#RUN-20) makes, and those
three change in the same pull request that builds it.

A person who approves the same kind of line repeatedly, such as `git pull` in each of several
checkouts, may answer the prompt for an `unconfined` line with `p`, which remembers a prefix: one
program and the leading arguments the person read. A later `unconfined` line is covered when every
condition below holds, and in no other case it is asked about as [SANDBOX-29](#SANDBOX-29) says.

- **The key is bytes.** The binary the program name resolved to and the path that started it, each as
  the path's own bytes ([RUN-8](tools/run.md#RUN-8)); the name that started it; the leading arguments,
  each its own field; and the tree the answer was given in, by its path's bytes and the identity the
  filesystem gives the directory ([TRUST-23](trust-map.md#TRUST-23)). An entry holding a rendering of a
  path covers nothing, as [RUN-19](tools/run.md#RUN-19) says of a line.
- **The line is a single stage with nothing else the exact key refuses.** One program with no
  assignment, no redirection, no `stdin_ref`, no private input, no credential scope or toolchain list
  and no other tree ([RUN-5](tools/run.md#RUN-5)). A pipeline, join or group is not covered.
- **Nothing follows the prefix unless the entry names it.** The entry is a row in a table in the
  source, written by hand and never built from a line a person answered or from anything a program
  printed. A row names a program, its leading arguments and every further argument it covers, each as
  a literal. Any other argument after the prefix makes the line ask again, a positional one as well as
  a flag: `git pull --upload-pack=<command>` and `git pull ext::<command>` are not covered by
  `git pull`. A row is checked by hand against the program's full option list, and the program has no
  flag, environment variable or configuration key that makes it run another program, short of ones the
  row excludes by naming the whole line it covers.
- **The program is on the list, and the list is an allowance.** A program that runs other programs
  (`env`, `xargs`, `find -exec`, `make`, `uv run`, `git -c`, a shell, an interpreter of any version) is
  not on it, and a program absent from the table is asked about. A program whose behaviour reads a file
  a confined stage could have written, as `git` reads `.git/config` and `.git/hooks`, is listed only on
  a platform that holds that file back from every stage ([SANDBOX-14](#SANDBOX-14)), which is macOS.
  No stage's program is a file under a directory the plan may write to, the checkout holding a session
  directory among them ([RUN-5](tools/run.md#RUN-5)).
- **Every condition [SANDBOX-29](#SANDBOX-29) puts on accepting `unconfined` still holds.** The mode
  is not `strict`, `read-only` or `off`, the workspace is trusted, a managed `run.network` is not
  `closed`, and the caller is not a delegate. A rule that says ask, or deny, decides first
  ([PERM-14](permissions.md#PERM-14)). A front end that cannot ask refuses the request, so a prefix on file is never read by a run with nobody to ask or by the desktop. The mode that
  asks nothing does not accept the request either.
- **The answer is for one tree.** A prefix remembered for the session is forgotten by `/cd`, `/clear`
  and `/branch`. A prefix remembered past the session is written to the record of that directory, in a
  file apart from [RUN-19](tools/run.md#RUN-19)'s so that a build that does not know it reads no
  exact line out of one, and is read only where the directory's path and identity both match: `/cd`
  into another checkout, a resume at a path that holds another checkout, and a copy or a re-clone ask
  again ([KEPT-001](../best-practices/carried-grants.md#KEPT-001)).

**What the prompt shows.** The prompt draws the prefix as it will be held, one argument per field, and
a sentence saying that a later line starting with it runs with no sandbox and no question in this
tree, that nothing after the prefix is covered, and where the answer is recorded. `p` is offered next
to `y` and `n` only for a line the table covers, and takes no answer until both the prefix and the
sentence are on the screen. A prompt whose prefix or sentence does not fit is not offered `p`.
`a`, `r`, `f`, `m` and `e` are not offered for an `unconfined` line ([SANDBOX-29](#SANDBOX-29)).

**What it prints is quarantined** as the output of a line the person did not read as written:
untrusted and private ([RUN-4](tools/run.md#RUN-4)), and the transcript says the line ran under a
remembered prefix. `p` vouches for no line and does not make a later line's output readable.

**Recorded.** The `sandbox` gate's detail names `unconfined` and says the line was covered by a prefix
([TRACE-1](trace.md#TRACE-1)).

**Why.** The exact key asks for a line a person read whole. A prefix is a looser key, and a looser key
covers more than the line the person read whenever it leaves something out. A prefix that ignores a
redirection, `stdin_ref`, an assignment or another tree runs a line whose input or output the person
never saw, which the single-stage condition rules out. One that ignores the directory covers the same
words in a checkout the person did not read, which the one-tree condition rules out. One that ignores
a step's environment, a credential scope or a toolchain list runs with access the person did not
grant, which the same single-stage condition rules out. One that holds a reference, or a path
rendered lossily, covers whatever the reference or the rendering later stands for, which the
bytes-only key rules out. One that matches an open-ended list of spellings covers the spelling nobody
listed, which the literal table rules out, as it does an argument after the prefix that makes the
program run another program. One whose deciding rows are pushed off the screen by a long value is
approved unread, which the rule that `p` takes no answer until the prefix and the sentence are shown
rules out. What a person gets in return is one answer for the line they repeat.

**What it costs.** A covered line runs with the person's own access and no sandbox, and with no
question, as often as the planner asks, including at a moment the person did not expect. The prefix
is a standing permission for that program in that tree. A program on the list that a later version
makes run something by another route is covered until the row is changed. A binary replaced at the
same path is not told from the original, which is also true of [RUN-19](tools/run.md#RUN-19). A
person who wants every unconfined line asked about again writes `Bash(*)` in `permissions.ask`.

**Not decided.** Which rows the first table holds, and whether `git pull` is among them given that a
workspace stage on Linux and Windows can write `.git/config`; whether the first build offers `p` for
the session only; and whether a covered line is announced in the transcript before it runs.

`verified-by: none`

<a id="SANDBOX-30"></a>
### SANDBOX-30: a program bravebot starts itself is never one a confined stage could have written

The programs bravebot starts on its own account and looks up by name, unconfined and with the
person's access to `~/.ssh`, `~/.aws` and `~/.bravebot`, are the AWS CLI (`aws`), the clipboard
tools, `osascript`, the inhibitor `/caffeinate` starts, a hook's program, the `crontab`, `launchctl` and `systemctl` that `bravebot schedule` and `/schedule` run and the destination program of a scheduled entry ([SCHEDULE-7](schedule.md#SCHEDULE-7), [SCHEDULE-8](schedule.md#SCHEDULE-8)), a language server, the
editor Ctrl-G opens, which `v` uses as well and a ctrl-click on a file path starts without this agent's own credentials ([CRED-14](credential-protection.md#CRED-14)), the browser opener that `auth login chatgpt` and a click on a link in the terminal
interface start (`open`, `xdg-open` or `rundll32`, [CHATGPT-13](chatgpt-sign-in.md#CHATGPT-13)), the credential authority ([CONNECTOR-2](connectors.md#CONNECTOR-2)), the `ssh-keygen`, `ssh` and `git` that `doctor --push-identity` runs
([SANDBOX-16](#SANDBOX-16)), and what `doctor --sandbox-check` runs with no sandbox
([SANDBOX-21](#SANDBOX-21)): the `gh` of the stored-login row, the programs of a workflow's setup
stages, and the programs of a workflow run again without the sandbox as the control, `git`, `sh`,
`cat` and `touch` among them. Each is resolved to an absolute path before it is started, from the
absolute entries of `PATH` that lie outside every directory a confined stage may write.

`clippy.toml` refuses every call the tree starts a program with, `std::process::Command::new` and
`CreateProcessW`, whatever names the program: a string literal, a variable, an alias, or a helper
that passes it on. A program is started with `bravebot_sandbox::programs::start(program, inherits,
path, selectors)`. `program` is a `Resolved` and not a path or a string. Only the lookup (`Resolved::find`,
`Resolved::find_on`) and a named constructor build one, and each constructor says who chose the
program: `own_executable`, `written_in_source`, `named_by_person`, `found_earlier` and `to_confine`.
`inherits` is `Inherits::All`, `Inherits::Without` the names
[CRED-14](credential-protection.md#CRED-14) removes, or `Inherits::Only` the names it passes on, and
`path` is `SearchPath::Reduced` or `SearchPath::Inherited`, and `selectors` is
`CodeSelectors::Kept` or `CodeSelectors::Checked` (below). None of the three has a default and
`start` applies all of them, so a new spawn picks a constructor, what the program inherits, which
`PATH` it is given and whether its code-selecting variables are checked in the diff, and does not
compile until it has. `start` holds the one `Command::new` the tree
has outside a build script and a test. `make check-security` fails a
`#[allow(clippy::disallowed_methods, reason = "...")]` mark in a production file other than the three
it lists (`start`, the one `CreateProcessW` and a build script) or a second mark in one of those, a
mark whose reason is empty, a mark that covers more than one start, and a second clippy
configuration that hides the refusal from a crate. Tests are allowed at the root of their crate or
file. A new spawn that looks a name up through
`PATH` is added to the list above, in the same change, or resolved first.

- **Those directories are the session's and the process's.** Each directory the session was opened on, added, or
  moved to and the checkout holding it (next bullet), the directory the process ran in and the
  checkout holding it (next bullet, recorded for every command), its scratch directory, the system temporary
  directory, every row a stage's policy
  grants writes in ([SANDBOX-18](#SANDBOX-18)), whatever granted it, and every row the policy a
  local MCP server is launched under grants writes in, its declared directory and its own home
  among them ([SERVERS-10](mcp-servers.md#SERVERS-10)). A row that came with the
  whole machine ([SANDBOX-28](#SANDBOX-28), under bypass) counts only where the account itself
  owns or can write the program, or a directory from the `PATH` entry up to the row, so `/usr/bin`
  stays searched. Ownership counts as well as the write permission because a stage creates files as
  the account and can set their modes, so a program it wrote and made read-only, with the
  directories above it, is still one it wrote. A directory is added to the
  set when it is opened, first granted to a stage, or granted to a server as it is launched, and
  not when a prompt describes the stage or the server, and is never removed from it, because
  closing a directory does not remove a file already written there. A server's rows are recorded
  from the policy it is launched under, after the rows its backend cannot name are dropped and
  before the launch.
- **The checkout holding a directory is left out with it.** The `bin` of a checkout can be first on
  `PATH` and hold a file a contributor added, which a session opened in a subdirectory of the
  checkout would start in place of the installed program, unconfined. The same function,
  `bravebot_sandbox::programs::keep_out_with_checkout`, records a directory and its checkout for a
  directory a session opens, adds or moves to, and for every command that opens no session.
  Every command is a new process, which knows of no directory a stage wrote in an earlier session,
  so `main` records the directory the process ran in, and the checkout holding it, once, before the
  command line is read and before any command is dispatched. A command added later is covered
  without a call of its own, and `bravebot schedule`, `bravebot auth login chatgpt`,
  `bravebot auth login bedrock`, `bravebot auth status` and both `doctor` commands start the
  programs they look up under that record. A deep link that moves the process to another directory
  records that directory as well. Where the working directory cannot be read nothing is recorded.
  The checkout is the nearest of the directory itself and its ancestors with a `.git`, a file
  as well as a directory, so a linked worktree or a submodule is the checkout holding the
  directory. A directory that holds a `.git` is its own checkout and nothing above it is added.
  Only the nearest is taken. Taking every ancestor with a `.git` would also leave out an outer repository
  that merely contains the checkout, such as a dotfiles repository at the account's home or the
  repository a worktree is nested in, and with it every installed `bin` under it. A directory under
  no checkout is recorded by itself. The confinement finds the checkout of a session directory with
  the same function, `bravebot_sandbox::programs::with_checkout`, to decide which files count as the
  plan's ([SANDBOX-20](#SANDBOX-20)), and leaves out a checkout that is the account's home directory
  or holds it, which this lookup does not. A virtualenv or tool directory a stage was granted elsewhere
  is not known to a command that opens no session. A program either doctor command starts is also
  started without this agent's own credentials ([CRED-14](credential-protection.md#CRED-14)).
- **A link is judged where it leads.** A `PATH` entry that links into one of them is left out,
  and so is a file in another directory that links into one. An entry that does not exist yet leads
  where the part of it that exists does, since a stage can make the rest.
- **A directory is judged by the directory it reaches, not only by its name.** A volume can open two
  spellings of a name as one directory: ext4 or tmpfs with `casefold`, vfat, exFAT, and on macOS a
  firmlink such as `/System/Volumes/Data/Users` for `/Users`. Resolving a path there returns the
  spelling it was given, so an entry written another way than a recorded directory shares no name
  with it. On Unix an entry, and the file the lookup resolves to, is also left out when a
  directory above where it leads is a recorded directory by device and inode. The names are
  compared first, so a recorded directory that no longer exists is still left out under the name
  it was recorded under. The `PATH` given to the editor, a hook and the suite's unconfined
  programs, and a row held where the account can write, are judged the same way.
- **An entry that is not absolute is not searched,** and neither is an empty one, which a shell
  reads as the current directory. A name carrying a separator is not looked up, and an absolute
  path is started as given. A hook's relative path is the person's own and runs from the directory
  the hook runs in ([HOOK-4](hooks.md#HOOK-4)).
- **Nothing found is not installed.** A person whose only `aws` is in such a directory, an
  activated virtualenv's `bin` among them, or in the `bin` of the checkout a session was opened in
  or a command ran in, is told it is not installed
  (Known costs in [backends.md](backends.md#known-costs)); the clipboard tool reads as absent and the next tool is
  tried.
- **The editor's, a hook's, the credential authority's, the inhibitor's, a clipboard tool's, the
  browser opener's, `doctor --push-identity`'s and the AWS CLI's own lookups search only those
  entries.** A started program looks up the names it starts itself through the `PATH` it inherits.
  The editor Ctrl-G opens, a hook's program, the credential authority, the inhibitor `/caffeinate`
  starts, a clipboard tool or `osascript`, the browser opener that `auth login chatgpt` and a
  click on a link start, the
  `ssh-keygen`, `ssh` and `git` of `doctor --push-identity`, and the AWS CLI are started with
  bravebot's `PATH` reduced to the entries the lookup searches, in the order
  they were written, so a `#!/usr/bin/env` shebang in the editor, the hook, a clipboard tool, the
  browser opener or an `aws` that is a script, a launcher script, an `$EDITOR` of `env vim`, a `ProxyCommand` in the
  person's ssh configuration that names a bare program finds the program it starts only among
  them. The authority is always an absolute path
  ([CONNECTOR-2](connectors.md#CONNECTOR-2)). A hook named by a path gets the same `PATH` as one
  named bare. Where no entry is searched the program is started with no `PATH` variable. An empty
  `PATH` would be read as the current directory. A program the editor, a hook, a clipboard tool,
  the browser opener, `ssh` or the AWS CLI starts by name that is found only in a directory left out
  is not found by it, such as a bare `credential_process` command in the person's `~/.aws/config`.
  `systemd-inhibit` and `systemctl --user` are given no `DBUS_SYSTEM_BUS_ADDRESS` or
  `DBUS_SESSION_BUS_ADDRESS` at all (SANDBOX-30, "What a checked start does not cover"), so no
  `unixexec:` address reaches them from the environment.
  A `PATH` names directories, so a file in a searched entry that links into a directory left out
  is still found by those lookups.
- **The suite's programs started with no sandbox search only those entries as well.** The `gh` of
  the stored-login row and every stage a workflow starts with no confinement are given a `PATH`
  reduced as the editor's is, so a `sh` that starts `mkdir`, or a `gh` that starts `git`, finds it
  only among them. A stage started under the sandbox keeps the `PATH` a session's stage is given.
  A program found only in a directory left out is not installed: its workflow is skipped
  ([SANDBOX-21](#SANDBOX-21)), and a `gh` found only there gives no login row.
- **A program named in another started program's arguments is resolved first.** A started program
  looks up a name in its own arguments through the `PATH` it was started with. Bravebot's own `PATH`
  can hold those directories, and a reduced one does not judge where a link in a searched entry
  leads, so a program that bravebot names in the arguments it gives a started program is passed as
  an absolute path, found the same way. The inhibitor `/caffeinate` starts on Linux is given `cat`
  that way ([CMD-12](commands.md#CMD-12)). Where `cat` is found only in such a directory the
  inhibitor is not started, and `/caffeinate` reports it as it does an inhibitor that is not
  installed.
- **`start` settles the environment, and a caller that then sets variables from input it does not
  control withholds again.** It removes the names it is given, or clears the environment and passes
  the listed names back, and then reduces `PATH`. A variable the caller sets on the command
  afterwards replaces what `start` removed, so the `run` tool calls `Inherits::withhold` again after
  a line's own assignments, and a `NAME=value` in front of a program does not bring a withheld
  `NAME` back ([RUN-12](tools/run.md#RUN-12)). `start` cannot tell which argument of a program names
  a program for it to start, so that argument is still passed as an absolute path by the caller, as
  the inhibitor's `cat` is, and PATH-005 has a reviewer read it.
- **A checked start has the variables that make a program run code taken off.** The fourth
  argument of `start` is `CodeSelectors::Kept` or `CodeSelectors::Checked(names)`. Checked removes
  from the environment each variable in `CODE_SELECTORS` in `crates/sandbox/src/programs.rs`, every
  variable whose name begins `GIT_CONFIG_` or `BASH_FUNC_`, every variable named `ASDF_<tool>_VERSION`
  (`ASDF_` and `_VERSION` around at least one byte), and each variable in `names`, unless its value is one
  the program may be left to run. `names` is `&[]` for a program with no variables of its own in the
  list. Windows matches a name without regard to case. A variable the `inherits` rule withheld stays
  withheld, and one it passes on is judged. A start is checked or kept as the last bullet of this
  list states. The generic table holds, by what each variable names:

  | Variable | Names |
  |---|---|
  | `LD_PRELOAD`, `LD_LIBRARY_PATH`, `LD_AUDIT`, `GCONV_PATH`, `LOCPATH`, `MUSL_LOCPATH`, `DYLD_INSERT_LIBRARIES`, `PYTHONPATH`, `XDG_DATA_DIRS`, `PYENV_HOOK_PATH` | a list of directories (`Names::Directories`), split as `PATH` is |
  | `LD_ORIGIN_PATH`, `PYTHONUSERBASE`, `PYTHONPYCACHEPREFIX`, `PYTHONHOME`, `DEVELOPER_DIR`, `GIT_EXEC_PATH`, `ASDF_DATA_DIR`, `PYENV_ROOT` | one directory (`Names::Directory`) |
  | `BASH_ENV`, `ASDF_CONFIG_FILE` | one file (`Names::File`) |
  | `NODE_OPTIONS`, `RUBYOPT`, `PERL5OPT`, `JAVA_TOOL_OPTIONS`, `CLOUDSDK_PYTHON_ARGS`, `NLSPATH`, `SHELLOPTS`, `PS4`, `PYENV_VERSION`, `PYTHONPLATLIBDIR`, every `GIT_CONFIG_` name, every `BASH_FUNC_` name and every `ASDF_<tool>_VERSION` name | options or configuration (`Names::Options`), which can name code whatever they hold |

  `SHELLOPTS`, `PS4`, `BASH_FUNC_` and the asdf and pyenv names are in the generic table and not in
  one program's list because an asdf or pyenv shim is a `#!/usr/bin/env bash` script that is found
  under the name of the program it stands for (`aws`, `python3`), so any checked start can run one,
  and a bash script runs the code `PS4` and `BASH_FUNC_` carry whatever program it stands for. What
  each name was shown to run is in the last bullet but one of "What a checked start does not cover".
  The pattern `ASDF_<tool>_VERSION` is matched by its shape because the tool is part of the name,
  and `ASDF_DATA_DIR` is judged by its value, so a prefix of `ASDF_` is not used.

  `HOME` is not in the generic table, because gcloud, `ssh` and `git` need it, so a checked start
  judges it only for a program that is an asdf or pyenv shim: a file directly in a `shims`
  directory that is directly in a directory named `.asdf` or `.pyenv`, or in the directory
  `ASDF_DATA_DIR` or `PYENV_ROOT` names in the environment being checked (a directory name is
  compared without regard to case). The shim is recognised by the path the lookup returned, and
  nothing is read from the file. For such a program `HOME` is removed unless it is an existing
  absolute directory outside every directory a stage may write and neither `.asdf` nor `.pyenv`
  under it leads into one; one that does not exist is judged by the nearest existing directory above
  it below `HOME`, as the browser opener judges its own paths. A kept start is not judged. The cost
  is that a shim whose `HOME` is a directory a stage may write runs with no `HOME`, so the program
  it stands for finds no configuration there. A shim found outside those directories, or one
  that a person has copied elsewhere, is not recognised and is started with the `HOME` it was given.

  A value is kept when `may_be_run` accepts it for what the variable names, as the last bullet of
  this list states. A list of commands, which `BROWSER` is (`Names::Commands`), is accepted when it
  is not empty and every entry, split as `PATH` is, is one program (accepted as a program is) alone
  or followed by ` %s`, with no whitespace, control character or shell character in it, so a value
  with arguments, a second command or a shell line is removed whole. A variable in no list is left
  as it is, so `GIT_CONFIG` and `LD_DEBUG` reach the program, and so does `HOME` for every program
  but the AWS CLI and the browser opener, whose lists hold it (the browser opener judges it as
  described below), and an asdf or pyenv shim (the paragraph above). Every checked start is given
  the generic table, which
  `programs::withhold_code_selectors_with` applies together with the
  program's own list: gcloud's is in `crates/agent/src/gcloud.rs`
  ([BACKEND-65](backends.md#BACKEND-65)), the AWS CLI's in `crates/bedrock/src/credentials.rs`,
  `doctor --push-identity`'s in `crates/cli/src/push_identity.rs`, the browser opener's in
  `crates/tui/src/opener.rs`, the Windows `powershell` starts' in `crates/tui/src/powershell.rs` and
  the sandbox usability suite's in `crates/agent/src/usability.rs`.
  The browser opener adds `BROWSER`, the four `XDG_` directories that `xdg-open` reads to choose
  the handler it runs (`XDG_DATA_DIRS` is also in the generic table), `GIO_LAUNCH_DESKTOP`, `HOME`,
  `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`
  and the GIO, Qt and KDE names in its table below. `GIO_LAUNCH_DESKTOP` is one file
  (`Names::File`): `gio open` executes the wrapper it names
  with the whole command line of the handler it starts (GLib 2.88.4, `gdesktopappinfo.c` line
  3100, read only when the process is not setuid), so a value is removed unless it is an existing
  file outside every directory a stage may write. `XDG_RUNTIME_DIR` and `DBUS_SESSION_BUS_ADDRESS`
  name the session bus that `gio open` connects to: it uses the address when it is set and
  otherwise `$XDG_RUNTIME_DIR/bus`. A service that a stage listens for on a socket it can create
  answers there as the bus, and what it answers to the calls that choose or start a handler is not
  known to be harmless, so the opener keeps each only when the socket it leads to is outside every
  directory a stage may write. `XDG_RUNTIME_DIR` is one directory (`Names::Directory`), removed
  unless it is an existing directory outside every such directory. `DBUS_SESSION_BUS_ADDRESS` is a
  D-Bus address (`Names::BusAddress`): entries joined with `;`, each of which must be
  `unix:path=<absolute path>` with the path outside every such directory (the `guid` key is
  allowed), or `unix:runtime=yes`. A path is judged where it leads, and one that does not exist is
  removed, since whoever can write the directory it would be in makes it first. The file at the
  path is not required to be a socket. A value that is not text is removed, since its path is
  not read from a lossy rendering; `XDG_RUNTIME_DIR` is judged by its bytes.
  An abstract socket, a `tcp:` or other transport, an unknown or repeated key and a `%` in a value
  fail the whole value. `HOME` is a
  directory: `gio open`, which `xdg-open` runs under a GNOME session, reads
  `$HOME/.config/mimeapps.list` and `$HOME/.local/share/applications` when `XDG_CONFIG_HOME` and
  `XDG_DATA_HOME` are not set. The opener removes `HOME` when it does not name an existing directory
  outside every directory a stage may write, and also when `$HOME/.config` or
  `$HOME/.local/share/applications` leads to one (a path that does not exist is judged by the
  nearest directory above it that does, below `HOME`). A session opened on `~/.config` or
  `~/.local/share/applications` has no row for `HOME` itself, which is why those two are judged. The
  generic table does not list `HOME`. The AWS CLI lists it as a directory (below), and gcloud, `ssh`
  and `git` keep it. The AWS CLI, the gcloud launcher and the `ssh-keygen`, `ssh` and `git` of
  `doctor --push-identity` list `BROWSER` and the four `XDG_` directories,
  since `aws sso login` and `gcloud auth application-default login` open the sign-in page through
  Python's `webbrowser`, which runs each entry of `BROWSER` and then `xdg-open`, and a credential
  helper that git's configuration names can do the same. `xdg-open` itself reads `BROWSER` only in
  its generic path, so under GNOME with `gio` installed it is not read. The desktop application
  deletes `BROWSER`, `GIO_LAUNCH_DESKTOP`, the four `XDG_` directories and the GIO, Qt and KDE names
  of the opener's table below from its own environment before it asks
  Electron to open an address or a file, since
  it starts the opener through Electron and not through this start. It does not delete `HOME`,
  `XDG_RUNTIME_DIR` or `DBUS_SESSION_BUS_ADDRESS`: the
  application reads `HOME` itself, Electron reaches the session bus through the other two, the
  agent's allowed-hosts proxy makes its socket directory under `XDG_RUNTIME_DIR`
  ([SANDBOX-24](#SANDBOX-24)), `process.env` is shared with the agent it starts, and a value
  put back after the call cannot be shown to follow the moment Electron starts `xdg-open`. On Linux
  under a GNOME session the application's opener therefore still reads a `HOME`, a runtime directory
  or a bus address that a stage may write.
  [#3904](https://github.com/brave/bravebot/issues/3904) tracks `HOME` and
  [#3940](https://github.com/brave/bravebot/issues/3940) tracks the other two, which are recorded
  in the bullet "The starts that keep `XDG_RUNTIME_DIR` and `DBUS_SESSION_BUS_ADDRESS`" under
  "What a checked start does not cover". The names another program
  reads are added to its start when that program is audited. The usability suite's list
  (`CODE_SELECTORS` in `crates/agent/src/usability.rs`, 77 names) adds the names `git`, `make`
  (the programs and flags of its built-in rules, `MAKE_COMMAND`, and `COMPILER_PATH` and
  `GCC_EXEC_PREFIX` of the compiler driver it starts), `cargo` (with `rustc`, `rustdoc`, `rustup`,
  the `BROWSER` and `CARGO_DOC_BROWSER` of `cargo doc --open`, and the credential providers),
  `npm` (`NODE_PATH`), `python3` (`PYTHONEXECUTABLE`, and `OPENSSL_CONF`, `OPENSSL_MODULES` and
  `OPENSSL_ENGINES` through the constants in `programs::selectors`) and `go` (with the C
  compiler, `pkg-config`, `GOAUTH` and the linker and compiler flags a cgo build reads) read to
  choose code, and it matches by form `CARGO_TARGET_<triple>_LINKER`, `_RUNNER`, `_RUSTFLAGS` and
  `_RUSTDOCFLAGS`, `CARGO_CREDENTIAL_ALIAS_<name>`, make's `COMPILE.<suffix>`, `LINK.<suffix>`,
  `PREPROCESS.<suffix>`, `LEX.<suffix>`, `YACC.<suffix>` and `LINT.<suffix>`, and every
  `npm_config_` name in any case. `python3` is also given the generic table, which holds
  `PYTHONPLATLIBDIR` beside `PYTHONHOME`. `CL`, `_CL_`, `LINK` and `_LINK_`, which the Visual C++
  compiler and linker read, are not listed: the suite starts `make`, `cargo`, `go` and `python3`
  only on Unix (`crates/agent/src/usability.rs` is compiled under `cfg(unix)`), the generic table
  applies to every checked start (`gcloud`, `aws`, `ssh` and `git` among them), none of which reads
  them, and no list in `crates/sandbox/src/programs.rs` is selected by `cfg(windows)`. That row of
  [#3811](https://github.com/brave/bravebot/issues/3811) stays open.
  `CC`, `CXX` and `PKG_CONFIG` are in this list and not in the generic table, because the generic
  table is applied to every checked start and only these starts compile or link. A run with a
  planted helper on the host recorded in the last bullet of "What a checked start does not cover"
  found: go cgo ran `CC`, `CXX` and `PKG_CONFIG`; make's
  built-in rules ran `CC` (compile and link) and `CXX` and never run `pkg-config`; the `cc` crate
  1.2.62 in a cargo build script ran `CC`, `CXX` and `AR`, and the `pkg-config` crate 0.3.33 ran
  `PKG_CONFIG`. `CC` and `CXX` hold a program and arguments, so they are options and removed whatever
  they hold; `PKG_CONFIG` is a program and is kept when it names one outside every directory a stage
  may write. The `cc` crate also ran the helper through `CC_<triple>`, `CXX_<triple>`, `AR_<triple>`,
  `HOST_CC`, `CFLAGS_<triple>`, `CXXFLAGS_<triple>` and `HOST_CFLAGS`, and the suite does not remove those:
  the names are a family that this list does not match yet. No workflow of the suite builds a crate
  with a build script that uses the `cc` crate, so the gap is not reached by a workflow that exists.
  It is tracked by #3859. Configuration files that can name code, such as `.npmrc` and
  `.cargo/config.toml` in a directory a stage may write, are not examined by the suite or by the
  lists; that is tracked by #3858.
  The stored-login `gh` removes every `DYLD_` and `LD_` name ([SANDBOX-21](#SANDBOX-21)). The suite
  also removes `GIT_TEMPLATE_DIR`, `GIT_EXEC_PATH`, `GIT_COMMON_DIR`, `MAKEFILES`, `DEVELOPER_DIR`,
  `GOROOT`, `GOENV` and `CARGO_HOME` whatever they hold, and records the names it examined and
  left (`TOOLCHAINS`, `SDKROOT`,
  `XCODE_DEVELOPER_DIR_PATH`, `VIMINIT`, `PYTHONSTARTUP`, `ENV`) with the reason. The `DYLD_` and
  `LD_` prefixes are `LOADER_PREFIXES` in `crates/sandbox/src/programs.rs`, kept out of
  `CODE_SELECTOR_PREFIXES`, which applies to every checked start and would remove `LD_DEBUG` from
  gcloud and the browser opener. `make check-code-selectors` runs
  `contrib/check-code-selectors.py` ([checks.md](../development/checks.md)). It fails a call of `start` or `start_against`
  whose fourth argument is not written at the call as `CodeSelectors::Kept` or
  `CodeSelectors::Checked(&names)`, a kept start in production code with no comment giving its
  reason, a start in production code that passes `CodeSelectors::Checked(&[])` with no comment at
  the call or whose file has no row in the table below, and a name in any `CODE_SELECTORS` list that lacks a test that keeps a safe value or a
  test that removes a value a stage may write. A list of options needs only the second. The check
  cannot tell that a list is complete, which [PATH-008](../best-practices/paths.md#PATH-008) leaves
  to a reviewer.
- **A kept start leaves the environment as it is.** The program is one the person chose, or it
  runs what the planner chose or a project carries, so removing a variable would change what the
  person asked for. The check covers the programs bravebot starts for its own purposes. It does not
  change the environment of a program the person chose to run, and it does not protect this
  process: a variable already in bravebot's environment applies when bravebot is launched, as
  `LD_PRELOAD` does on Linux, before any start removes it from a child.
- **What each start inherits is stated here.** The programs a `run` stage, a background job, a hook, a
  language server, the AWS CLI, the browser opener, `doctor --push-identity` and `doctor --sandbox-check`
  start get this process's environment without this agent's own credentials
  ([CRED-14](credential-protection.md#CRED-14)). Those of a `run` stage, a background job, a hook, a
  language server and `doctor --sandbox-check` are also given none of the names a person's
  `run.scrubEnv` lists. The credential authority gets the names
  [CONNECTOR-2](connectors.md#CONNECTOR-2) lists and nothing else. The editor, the clipboard tools,
  `osascript`, the inhibitor, a line typed at the `!` prompt, the client the host starts, and this
  program started again (the background supervisor and host, `doctor` for a bug report) get all of
  it: each is the person's own program or holds no credential to hand on. `xcode-select`,
  `sandbox-exec` and the program a sandbox backend starts confined are started with all of it, and
  the environment a confined program has is the one its caller asks the backend for.
- **Each start is checked or kept.** These are checked: the AWS CLI, the gcloud launcher, the
  browser opener, the `ssh-keygen`, `ssh` and `git` of `doctor --push-identity`, the clipboard
  tools and `osascript`, the inhibitor `/caffeinate` starts, the `crontab`, `launchctl` and
  `systemctl` of the scheduler, the tool a workflow is run again with in `doctor --sandbox-check`,
  the stored-login `gh`, `xcode-select` and the OpenShell command line. These are kept: a line
  typed at the `!` prompt, a hook, the editor, a language server, a `run` stage, a background job,
  the program a sandbox backend confines, which a local MCP server is, and `sandbox-exec`, the
  credential authority, the client the host starts, the delivery program of a scheduled entry, the
  `sysctl` that reads a machine identifier with an empty environment, and this program started
  again. A start added to either list is added here in the same change.
- **A dependency starts no program.** crossterm 0.29.0 started `tput` by bare name, with the whole
  environment and `PATH`, when a terminal-size query failed. That start did not go through
  `programs::start`, which `clippy.toml` cannot see into a dependency. The tree keeps crossterm in
  `third_party/crossterm` with that fallback removed, so a failed query is an error and nothing is
  started: the resize event the reader raises on SIGWINCH and the size ratatui asks for on every
  draw both end there. A dependency is audited for a bare-name start when it is added or
  raised. `ratatui-image` 11.1.0 (`src/picker.rs`) starts `tmux` with the whole environment when
  `TERM` or `TERM_PROGRAM` names tmux. It is tracked by #3853.
- **No caller reaches the starts in `gix-path`.** `gix-path` 0.13.0 is compiled in through
  `gix-features`, `gix-fs`, `gix-glob`, `gix-ignore`, `gix-odb`, `gix-pack` and `gix-ref`, and the
  crate has no feature that removes its `env` module. That module has three starts, none of them
  through `programs::start`, so none has its `PATH` reduced and none has its `CodeSelectors`
  checked, and each is given this program's environment (the `git config` query removes or sets
  some `GIT_*` variables and passes the rest on). `shell_command` starts `/bin/sh` on Unix, by
  that absolute path. On Windows it starts the `sh.exe` that Git for Windows installs, by the full path found from
  the output of `git --exec-path` (`bin/sh.exe`, then `usr/bin/sh.exe`), and falls back to the bare
  name `sh.exe`, found through the `PATH`, when that path is not found (`src/env/mod.rs`,
  `src/env/auxiliary.rs`). `core_dir` starts
  `git --exec-path` on every platform; `core_dir_program` and `installation_program` reach it, and
  on Windows so do `shell`, `shell_command` and `system_prefix`. `installation_config`,
  `installation_config_is_system` and `installation_config_prefix` can start `git config` on every
  platform, and `system_config` can on Windows (`src/env/git/mod.rs`). `git` is the bare name, so
  it is found through the `PATH` the process has, and a `git` in a directory a stage may write,
  such as the `bin` of an activated virtualenv, would run with the environment this program has.
  Nothing in the tree reaches any of them. The tree holds that with a test that reads source: no
  `.rs` or `.toml` file under `crates/`, `fuzz/` or `third_party/`, and no `Cargo.toml` at the
  workspace root, names `gix-path` (other file kinds, such as `Cargo.lock`, are not read); no package in the resolved
  graph of the workspace or of `fuzz/` that depends on `gix-path` names its `env` module in any of
  its Rust source (a path through it, an import group or a glob that includes it, or the crate's
  name with no path after it), other than `home_dir`, `var` and `xdg_config`, which start nothing;
  and `gix-path` starts a program in no file outside `src/env`. A fourth check lists the functions
  of `gix-path` that name `Command` (`shell_command`, `git_cmd`, the initialiser of `GIT_CORE_DIR`
  and the import in `src/env/git/mod.rs`) in `AUDITED_STARTS`, and fails when the set the source
  gives differs, so `home_dir`, `var` and `xdg_config` are read as starting nothing only while
  they name no `Command`, and a raise of `gix-path` that changes the set of (file, function) pairs that name
  `Command` needs a new audit before it merges. A raise that leaves the set the same is not
  stopped, so a start added inside a function that already names `Command` is not seen. That check
  attributes each use to the function it is in by counting braces, and does not see a start made
  through a macro or by a function that another function calls. The test reads the graph with
  `cargo metadata --locked` on every run. Cargo needs the manifest of every package the lock names, including
  packages of other platforms and the crates of `fuzz/Cargo.lock` that the test jobs never build, and downloads
  any that `CARGO_HOME` lacks, so the test needs the network on a machine with a cold `CARGO_HOME`. A graph
  that cannot be read is skipped on a developer machine with the cause printed and fails the test where `CI`
  is set. CI runs the workspace's tests on each
  change, so a change that raises a gix crate or `gix-path` in `Cargo.lock` is read by the test
  before it merges. A package whose source is not on disk is named in the test's output as
  skipped. The test reads source and does not run the program: it does not see a call made through
  a macro, a trait object or a name the source computes, and it cannot show that the list of
  entry points is complete ([PATH-008](../best-practices/paths.md#PATH-008)). The starts are
  accepted until a caller appears, and the test fails when one does.
- **Each start's `PATH` is stated here.** A program that starts others by name through the `PATH` it
  inherits is given one of three, and a start added to the list at the head of this clause is added
  to this list with its answer in the same change:

  | Start | `PATH` it is given |
  |---|---|
  | The editor Ctrl-G opens, `v` opens the transcript in, and a click on a path opens a file in | reduced to the entries the lookup searches |
  | A hook's program, named bare or by a path | reduced to the entries the lookup searches |
  | The AWS CLI | reduced to the entries the lookup searches |
  | `gh`, `sh` and the programs of a workflow's setup stages, and a workflow run again without the sandbox, in `doctor --sandbox-check` | reduced to the entries the lookup searches |
  | `ssh-keygen`, `ssh` and `git` in `doctor --push-identity` | reduced to the entries the lookup searches |
  | The connector authority | reduced to the entries the lookup searches |
  | A language server | bravebot's own, accepted: it runs code the project carries, and pyright is meant to use the virtualenv's `python3` ([LSP-5](tools/lsp.md#LSP-5), [Known costs](tools/lsp.md#known-costs)) |
  | The browser opener | reduced to the entries the lookup searches, on every platform: an opener that is a script starts its interpreter by name |
  | The clipboard tools and `osascript` | reduced to the entries the lookup searches, on every platform: a tool that is a script or a launcher starts its interpreter by name |
  | The inhibitor `/caffeinate` starts | reduced to the entries the lookup searches, on every platform. On Linux the one name it is given, `cat`, is resolved first. `DBUS_SYSTEM_BUS_ADDRESS` is removed whatever it holds, so no program named by a `unixexec:` address is started. |
  | A `run` stage or background job, a line typed at the `!` prompt and the client the host starts | not applicable: the program is the one the planner or the person named, and what it starts is theirs. A stage under the sandbox is given the `PATH` a session's stage is given. |
  | `xcode-select` and `sandbox-exec` | not applicable: each is an absolute path written in the source that starts no other program by name |
  | The background supervisor, the host and a bug report | not applicable: each starts this program's own executable or a path stored by it |

  A row that says unreduced is a known gap, not a decision, and it comes out of the table when the
  start is given `SearchPath::Reduced` and a test plants an interpreter in a directory left out and
  checks it is not run.
- **The variables a started program reads to choose code are withheld unless they name something
  outside every directory a stage may write.** `programs::withhold_code_selectors_with` takes each
  variable of a program's `CODE_SELECTORS` list off the environment `start` built, unless its value
  passes `programs::may_be_run`: an absolute path that exists outside every such directory, links
  followed, and for a variable that names a file ([`Names::File`](#SANDBOX-30)) a regular file. A
  variable that names a program ([`Names::Program`](#SANDBOX-30)) also keeps a bare name, since the
  program's `PATH` leaves those directories out. A relative path, a path that does not exist and a
  value inside such a directory are taken off. A variable that holds a list of directories
  (`Names::Directories`, joined with the platform's path separator) is kept only when it has an
  entry and every entry passes as a directory, so one empty, relative, missing or stage-writable
  entry takes the whole variable off. A variable that holds options (`Names::Options`) names no
  path to check and is always taken off, as is a variable that names a program the start has no
  use for. A variable not listed is left as set. A variable that
  more than one list names is defined once in `programs::selectors`. The lists are `CODE_SELECTORS`
  in `crates/agent/src/gcloud.rs` ([BACKEND-65](backends.md#BACKEND-65)), in
  `crates/bedrock/src/credentials.rs`, in `crates/cli/src/push_identity.rs`, in
  `crates/tui/src/opener.rs`, in `crates/tui/src/powershell.rs` (with `OSASCRIPT_CODE_SELECTORS` for `osascript`) and in
  `crates/agent/src/usability.rs`:

  | `gcloud` variable | Names | Why it selects code |
  |---|---|---|
  | `CLOUDSDK_PYTHON` | a program | the interpreter the launcher executes |
  | `CLOUDSDK_CONFIG` | a directory | the launcher sources `virtenv/bin/activate` from it when it has no interpreter of its own |
  | `CLOUDSDK_PYTHON_ARGS` | options | placed before `gcloud.py` on the interpreter's command line, where `-c <code>` and `-m <module>` run |
  | `PYTHONPATH` | a list of directories | the launcher passes neither `-E` nor `-I`, so the interpreter imports `sitecustomize.py` and any module it shadows from these |
  | `PYTHONHOME` | a directory | where the interpreter loads its standard library from |
  | `PYTHONUSERBASE` | a directory | the user site directory, whose `usercustomize.py` and `.pth` files run |
  | `PYTHONPYCACHEPREFIX` | a directory | where the interpreter reads compiled modules from without checking them against the source |
  | `GOOGLE_APPLICATION_CREDENTIALS` | a file | an `external_account` credential in it names a command to run, or a file to read and send to a host it names |
  | `CLOUDSDK_CONTEXT_AWARE_CERTIFICATE_CONFIG_FILE_PATH`, `CLOUDSDK_CONTEXT_AWARE_AUTO_DISCOVERY_FILE_PATH` | a file | name a binary or a certificate provider command the CLI runs when client certificates are in use |
  | `BROWSER` | a program | the command `gcloud auth login` opens the sign-in page with |
  | `OPENSSL_CONF` | a file | configuration of the OpenSSL Python loads, which can load a provider module |

  The AWS CLI's list is:

  | AWS CLI variable | Names | Why it selects code |
  |---|---|---|
  | `AWS_CONFIG_FILE` | a file | replaces `~/.aws/config`, whose profile can carry a `credential_process` command, and whose `[plugins]` section loads a Python module |
  | `AWS_SHARED_CREDENTIALS_FILE` | a file | replaces `~/.aws/credentials`, whose profile can carry a `credential_process` command |
  | `BROWSER` | a program | the command `aws sso login` opens the sign-in page with |
  | `OPENSSL_CONF` | a file | configuration of the OpenSSL the CLI bundles, which can load an engine or a provider module |
  | `OPENSSL_CONF_INCLUDE` | a directory | where that configuration's `.include` lines are read from |
  | `OPENSSL_ENGINES`, `OPENSSL_MODULES` | a directory | where the bundled OpenSSL loads engines and provider modules from |
  | `HOME` | a directory | where `~/.aws/config` (a `credential_process`) and `~/.aws/cli/alias` (an alias that starts with `!` runs a shell command) are read from when `AWS_CONFIG_FILE` is not set |
  | `PAGER`, `MANPAGER` | a list of commands | the program an `aws ... help` page is piped to (`awscli/help.py`), run through a shell; `MANPAGER` is preferred |
  | `PYTHONWARNINGS` | options | the interpreter imports the module a `-W` filter names, and `PYTHONPATH` can place one |
  | `BOTOCORE_EXPERIMENTAL__PLUGINS` | options | read by version 1 of the CLI (`awscli/botocore/plugin.py`) for the modules a client loads as plug-ins. No run loaded a module through it, and it is listed because it is read |

  The installed CLI of version 2 from AWS is a frozen executable, and a probe of 2.37.12
  (`/usr/local/bin/aws`) with `PYTHONPATH` naming a `sitecustomize.py`, and with `PYTHONSTARTUP`,
  did not run it. An `aws` that is a script is a Python program started without `-E` or `-I`, and
  runs the interpreter's variables. That is the `aws` of a pip install and of the Homebrew
  `awscli` formula. Its 2.39.7 `Formula/a/awscli.rb` in `Homebrew/homebrew-core` calls
  `virtualenv_create(libexec, python3, system_site_packages: false)`, `pip_install` for the
  resources and `pip_install_and_link` for the CLI. The installed `/opt/homebrew/bin/aws` is a link
  to `Cellar/awscli/2.39.7/bin/aws`, and the entry script `libexec/bin/aws` starts with
  `#!/opt/homebrew/Cellar/awscli/2.39.7/libexec/bin/python`. What was run and what was read:

  - **Run, Homebrew**: the `awscli` 2.39.7 bottle that `brew install awscli` installs
    (`aws --version` printed `aws-cli/2.39.7 Python/3.15.2 Darwin/25.6.0 source/arm64`), on macOS
    26.6, arm64, with `env -i`, a scratch `HOME` and `PATH=/usr/bin:/bin`, and a helper that
    appends to a marker file planted under each name. A control with nothing planted wrote no
    marker. The helper ran under `AWS_CONFIG_FILE` (a `credential_process`, and a `[plugins]`
    section with `cli_legacy_plugin_path`), `AWS_SHARED_CREDENTIALS_FILE` (a `credential_process`),
    `HOME` (the same `~/.aws/config` entries, and a `!` alias in `~/.aws/cli/alias`), `PAGER` and
    `MANPAGER` (for `aws s3 help` with output piped), `PYTHONPATH` (a `sitecustomize`, also with
    `PYTHONSAFEPATH=1`) and `PYTHONWARNINGS` (`ignore::<module>.<Class>` imported the module that
    `PYTHONPATH` placed). It ran nothing under `PYTHONWARNINGS` alone, `PYTHONHOME`,
    `PYTHONEXECUTABLE`, `PYTHONUSERBASE`, `PYTHONBREAKPOINT`, `PYTHONSTARTUP` with piped streams,
    `BOTOCORE_EXPERIMENTAL__PLUGINS`, `AWS_PAGER` with piped streams, or `EDITOR`, `VISUAL` and
    `BROWSER` for `aws configure list`. Under a pseudo-terminal, with a local HTTP endpoint,
    `AWS_PAGER` and `PAGER` ran for the output of `aws sts get-caller-identity`, and
    `--no-cli-pager` stopped them. `PYTHONSTARTUP` ran only with `PYTHONINSPECT=1` and a
    pseudo-terminal. Every `aws` this program starts has its streams piped, so neither is listed.
  - **Run, pip**: `awscli` 1.46.3 from pip in a virtualenv of Python 3.15.0 (Homebrew), with the
    same method. The results match the Homebrew run, with these differences: version 1 ignores
    `cli_legacy_plugin_path`, and loads a `[plugins]` entry `name = module` only when `PYTHONPATH`
    names the directory of the module; it ran no pager for `AWS_PAGER` or `PAGER` for the output of
    `aws sts get-caller-identity` under a pseudo-terminal. `PYTHONDEVMODE`, `PYTHONTRACEMALLOC` and
    `PYTHONSAFEPATH` each let it exit 0 and ran nothing.
  - **Run, frozen 2.37.12**: the config-file results above (a `credential_process`, a `!` alias, a
    `[plugins]` entry, and `PAGER` and `MANPAGER` for `aws s3 help`) hold for it too, and nothing
    ran under `PYTHONPATH` or `PYTHONSTARTUP` with piped streams. `AWS_PAGER` and `PAGER` ran for
    the output of a command under a pseudo-terminal and not when the streams were piped.
  - **`BOTOCORE_EXPERIMENTAL__PLUGINS`** is read at `awscli/botocore/plugin.py` of 1.46.3. With
    `p=<module>` and `PYTHONPATH` naming the module, neither 1.46.3 nor Homebrew 2.39.7 loaded it.
    The CLI creates its own clients with `PluginContext(plugins="DISABLED")`
    (`create_nested_client` in `awscli/botocore/utils.py`). Source of 2.39.7 (commit
    1db1c87dcc3a560dfbad7ea07bb37d0ab6ed010b of `aws/aws-cli`) holds no read of the name in
    `awscli/*.py` and no `awscli/botocore/plugin.py`. It stays in the list because 1.46.3 reads it.
  - **Read from source**: in 2.39.7 `awscli/clidriver.py` reads `AWS_PAGER` (line 390) and `PAGER`
    (line 398), and `awscli/help.py` reads `MANPAGER` (line 135). In 1.46.3 `awscli/help.py` reads
    `MANPAGER` and then `PAGER` (lines 85 to 88) and starts the pager with `shell=True`, and
    `awscli/customizations/globalargs.py` reads `AWS_PAGER` only to warn about it.

  The rest of the variables the CLI documents (`AWS_PROFILE`, `AWS_CA_BUNDLE`,
  `AWS_WEB_IDENTITY_TOKEN_FILE`, `AWS_DATA_PATH`, `SSL_CERT_FILE`, `SSL_CERT_DIR`) name data the CLI
  parses and does not run. This list was read from the CLI's own topic pages, the strings of the
  executable and its bundled `libcrypto`, and the runs above, and cannot show that no other variable
  selects code ([PATH-008](../best-practices/paths.md#PATH-008)). `gcloud` is a Python program
  started without `-E` or `-I`, which is why its list holds the `PYTHON*` variables. The AWS list
  does not, since the generic table holds those the script run reached (`PYTHONPATH`) and a probe of
  the frozen executable reached none.
  The test that runs the installed CLI with a `credential_process` helper is skipped on a machine
  with no `aws` in a directory the lookup accepts, so its `verified-by` line holds only where the
  CLI is installed.

  The browser opener's list is:

  | Variable | Names | Why it selects code |
  |---|---|---|
  | `BROWSER` | a list of commands | the commands `xdg-open` and Python's `webbrowser` run |
  | `XDG_DATA_HOME`, `XDG_CONFIG_HOME` | a directory | where `xdg-open` and `gio open` read `mimeapps.list` and the `.desktop` entry of the handler |
  | `XDG_DATA_DIRS`, `XDG_CONFIG_DIRS` | a list of directories | the same, searched in order |
  | `HOME` | a directory | the base of the four above when they are not set, so a changed `HOME` selects the handler. `$HOME/.config` and `$HOME/.local/share/applications` are judged with it |
  | `GIO_EXTRA_MODULES` | a list of directories | modules `gio` loads |
  | `GIO_MODULE_DIR` | a directory | the directory `gio` loads modules from |
  | `GIO_LAUNCH_DESKTOP` | a file | the wrapper `gio open` executes with the command line of the handler it starts |
  | `XDG_RUNTIME_DIR` | a directory | holds the session bus socket `gio open` connects to when `DBUS_SESSION_BUS_ADDRESS` is not set |
  | `DBUS_SESSION_BUS_ADDRESS` | a D-Bus address | the session bus `gio open` connects to, which answers the calls that choose or start a handler |
  | `QT_PLUGIN_PATH` | a list of directories | where Qt, and so `kde-open`, loads plug-ins from |
  | `QT_QPA_PLATFORM_PLUGIN_PATH` | a directory | where Qt loads the platform plug-in from |
  | `QT_QPA_PLATFORMTHEME`, `QT_QPA_GENERIC_PLUGINS`, `QT_QPA_PLATFORM`, `QT_STYLE_OVERRIDE` | options | name the plug-in Qt loads from the directories above |
  | `KDESYCOCA` | a file | the service cache `kde-open` reads the `Exec` line of a handler from |

  What was run and what was read for these names is in "What a checked start does not cover".

  `osascript` is given `OSASCRIPT_CODE_SELECTORS` in `crates/tui/src/powershell.rs`, the generic
  table, and no other list:

  | Variable | Names | Why it selects code |
  |---|---|---|
  | `CFFIXED_USER_HOME` | a directory | the home directory `osascript` takes its user-domain `Library/Script Libraries` and `Library/ScriptingAdditions` from, where a compiled script library it is asked to use runs its handlers |

  `HOME` is not listed for `osascript`: it does not follow `HOME` (the runs are under "What a checked
  start does not cover").

  The `ssh-keygen`, `ssh` and `git` that `doctor --push-identity` starts are all built by `started`
  in `crates/cli/src/push_identity.rs`, so one list covers the three:

  | Variable | Names | Why it selects code |
  |---|---|---|
  | `SSH_ASKPASS` | options | the program `ssh` runs to ask for a passphrase. This command asks for none from the environment |
  | `SSH_ASKPASS_REQUIRE` | options | `force` makes `ssh` run `SSH_ASKPASS` even with a terminal |
  | `SSH_SK_PROVIDER` | options | the library `ssh` and `ssh-keygen` load for a hardware key |
  | `SSH_SK_HELPER` | options | the program `ssh` runs for a hardware key |
  | `DEVELOPER_DIR` | a directory | where the macOS `/usr/bin/git` shim finds the toolchain it runs, such as `usr/bin/xcrun` |

  `started` also sets `SHELL` to `/bin/sh` whatever the person's is, because `ssh` runs a
  `ProxyCommand` or a `LocalCommand` with `$SHELL -c`. `SHELL` is set and not checked, since the
  person's shell has no use here. The tests in `crates/cli/src/push_identity.rs` hand `started` an
  environment and read the one it gives the program, so they start no real `ssh`.

  The `powershell` that the clipboard tool and `/caffeinate` start on Windows is started by
  `powershell::start` in `crates/tui/src/powershell.rs`, which both call. It gives a program whose
  name is `powershell` (with or without `.exe`, in any case) this list, a program whose name is
  `osascript` the `osascript` list below, and every other program the generic table alone:

  | Variable | Names | Why it selects code |
  |---|---|---|
  | `PSModulePath` | a list of directories | the directories PowerShell searches for modules, which it loads and runs when a command names a module found in one of them |
  | `COR_ENABLE_PROFILING`, `COR_PROFILER` | options | switch on, and name by class id, a profiler that the .NET Framework runtime of Windows PowerShell 5.1 loads into the process. They name no path and the starts have no use for a profiler |
  | `CORECLR_ENABLE_PROFILING`, `CORECLR_PROFILER` | options | the same for the .NET runtime of PowerShell 7 |
  | `COR_PROFILER_PATH`, `COR_PROFILER_PATH_32`, `COR_PROFILER_PATH_64` | a file | the library the .NET Framework runtime loads as the profiler, for any process or one of that width |
  | `CORECLR_PROFILER_PATH`, `CORECLR_PROFILER_PATH_32`, `CORECLR_PROFILER_PATH_64` | a file | the same for the .NET runtime |

  This list was read from PowerShell's `about_PSModulePath` topic and from the .NET documentation
  of the profiling environment variables. It was not shown by running `powershell` on Windows that
  any of these variables changes what runs, and it cannot show that no other variable does
  ([PATH-008](../best-practices/paths.md#PATH-008)). Both starts pass `-NoProfile`, so no variable
  that names a profile script is listed. `gcloud.cmd` and `APPDATA` are not listed, since nothing
  read shows that either selects code for these starts. Windows matches the names without regard to
  case. The tests in `crates/tui/src/powershell.rs` hand the start an
  environment and read the one it gives the program, so they run on every platform and start no real
  `powershell`. One of them reads `clipboard.rs` and `caffeinate.rs` as text and fails when either
  starts a program other than through `powershell::start`, because no test on a host without
  Windows runs those starts.

  Every other start that passes `CodeSelectors::Checked(&[])` is given the generic table and no
  names of its own. Each file with one has a row here that says what the program was read for, and
  `make check-code-selectors` fails when a production start passes an empty list with no comment at
  the call or with no row for its file. A row is not a finding that no other variable selects code:
  the basis column says how each program was read, and none of these was shown by running the
  program with a name planted.

  <!-- empty-list-starts -->

  | File | Start | Names read | Basis |
  |---|---|---|---|
  | `crates/sandbox/src/macos.rs` | `xcode-select -p`, which `DEVELOPER_DIR` redirects | `DEVELOPER_DIR`, which is in the generic table and is also removed at the call | the binary's strings on macOS 26.6 hold no other upper-case variable name |
  | `crates/schedule/src/schedule.rs` | `crontab -l` and `crontab -`, `launchctl load` and `unload` | `crontab` names `EDITOR` and `VISUAL` for `crontab -e`, which no start runs. `launchctl` names `DYLD_INSERT_LIBRARIES` (in the generic table), `DYLD_LIBRARY_PATH` and `DYLD_IMAGE_SUFFIX`. `systemctl --user` is not an empty-list start. Its four starts (`--user daemon-reload` twice, `--user enable --now` and `--user disable --now`, each through `succeeds`, which sends stdin, stdout and stderr to `/dev/null`) are given `SYSTEMCTL_CODE_SELECTORS`: `DBUS_SESSION_BUS_ADDRESS` (`Names::Options`) and `SYSTEMD_UNIT_PATH` (`Names::Directories`). The bullet on `systemd-inhibit` and `systemctl --user` under "What a checked start does not cover" has the basis. None of the four passes `edit`, `-H` or `-M`. No Linux run was made | the strings of the macOS binaries for `crontab` and `launchctl`. `DYLD_LIBRARY_PATH` and `DYLD_IMAGE_SUFFIX` are not in the generic table and were not shown to change what `launchctl` loads |
  | `crates/sandbox/src/openshell.rs` | the `openshell` CLI, for each `sandbox` subcommand and for `sandbox exec` | none. The start passes `Inherits::Only(["HOME"])`, so no other variable reaches the CLI | the start's own argument. A changed `HOME` is not checked |
  | `crates/tui/src/powershell.rs` | `xclip` and `xsel`, the clipboard tools that are not `powershell`, `osascript`, `systemd-inhibit` or `wl-copy`, with `pbcopy`, `clip` and `/usr/bin/caffeinate`, which are started through the same function | `xclip` 0.13 and `xsel` 1.2.1 read no variable of their own that selects code, and the libraries they use read names that select a socket, a credential file or data only (`DISPLAY`, `XAUTHORITY`, `XENVIRONMENT`, `XLOCALEDIR`, `XCOMPOSEFILE`). On macOS `pbcopy` and `caffeinate` hold no upper-case variable name in their strings. `systemd-inhibit` and `wl-copy` have lists of their own and are not empty-list starts. No Linux run was made | `xclip.c`, `xclib.c` of xclip 0.13, `xsel.c` of xsel 1.2.1, libX11 1.8.7, libxcb 1.16 and libXau 1.0.11, read for `getenv`, `exec*`, `dlopen` and `popen`; the strings of the macOS binaries for the other two |
  | `crates/agent/src/usability.rs` | the stored-login `gh auth token` | `GH_TOKEN` and the three other token names, which the start removes. Other variables `gh` reads were not looked for in the program | the call's own removals |

  The unconfined `make`, `cargo`, `npm`, `go` and `python3` starts of `doctor --sandbox-check` are
  not in this table, because they pass a list of their own.
- **The stage's grants are not narrowed.** Withholding `PATH` directories from the write grants
  would stop `pip install` in an activated environment, which is what the grants are for.

**Why.** A stage can write `.venv/bin/aws`, and an activated environment puts that directory first
on `PATH`. The name `aws` then resolves to the stage's file, which bravebot starts outside any
confinement ([CRED-14](credential-protection.md#CRED-14)): the stage has chosen what runs with
the person's access, which the confinement exists to deny it. A started program looks up a name in
its own arguments through the `PATH` it was started with, not through the lookup above, which also
judges where a link leads. `systemd-inhibit` starts `cat` that way, so its argument is the resolved
path and not the bare name.
An editor, a hook, a credential authority, a clipboard tool or an `aws` that is a script starts its
interpreter the same way, and `ssh` starts a `ProxyCommand`'s program so, so each is given the
`PATH` the lookup would have searched.

**What it costs.** A directory is compared by device and inode, so a filesystem that reports the
same pair for two directories (some FUSE, 9p and network mounts) leaves out an entry that should be
searched, and the program reads as not installed. Off Unix there is no comparison by identity,
because the file index is not in the standard library, so only names are compared there, after a
path is resolved as far as it exists. A recorded directory that does not exist has no identity, so
another spelling of it is compared by name only until it exists.

Under bypass a program the account owns is left out of the lookup whatever its mode, so a person's
own read-only program under a row such as `~/.cargo/bin` reads as not installed, as one in a
directory a stage may write does. A program in directories the account neither owns nor can write,
such as `/usr/bin` on a system where root owns it, is found. Where the account owns the whole tree a
program is in, as when bravebot runs as root, none is found under a row of that tree.

A program a person keeps in a `bin` of the nearest checkout holding a directory a session opens,
adds or moves to reads as not installed to every lookup above for the rest of the process, whether
the session was opened at the checkout's root or below it, and every command, whether or not it
opens a session, leaves out the one holding the directory it ran in. Where the account's home directory holds a `.git`, such as a dotfiles
repository, a session opened in a directory below the home that has no nearer `.git` leaves out the
home as its checkout, so `~/bin` and every other directory under the home that is on `PATH` is not
searched and a program kept in one reads as not installed. A session opened below a nearer checkout
leaves out that checkout only, and the repository at the home stays searched. The record only
grows, so a checkout recorded for one session stays left out when the session moves elsewhere.

A directory a person declares for a local MCP server is left out from the launch on, as a whole, so
a program they keep in it or below it, and every program under a home directory declared as one,
reads as not installed to every lookup above, and the next clipboard tool is tried. The lookup of a
server's own program by name ([SERVERS-5](mcp-servers.md#SERVERS-5)) reads the same record, so a
program named by a bare name that lies in a directory a started server may write is found by the
launch that records the directory and not by a later resolution in the same process, such as a
reconnect. A program named by an absolute path is not judged by any lookup and starts as before.

**What a checked start does not cover.** A checked start removes the generic names and the names its
caller lists, and a name nobody listed reaches the program. Each bullet says what was read from a
program's documentation or launcher, what was read from source and what was run. These were not
shown by running the program, or not entirely:

- The Windows `powershell` starts of the clipboard tool (`crates/tui/src/clipboard.rs`) and of the
  inhibitor (`crates/tui/src/caffeinate.rs`) are given the generic table and the list in
  `crates/tui/src/powershell.rs`. That list was written from documentation. It has not been shown by
  running `powershell` on Windows that any variable in it changes what runs, or that no other
  variable does. `gcloud.cmd` and `APPDATA` are not in it, and have not been checked on Windows. It
  is tracked by #3808.
- The dynamic-loader names (`LD_*`, `GCONV_PATH`, `LOCPATH`, `NLSPATH`, `MUSL_LOCPATH`) have three
  tests in `crates/sandbox/tests/linux_loader.rs`, which is compiled only on Linux. None has been
  run on a Linux host or loader. The file was compiled and linted for `x86_64-unknown-linux-gnu` on
  macOS. The first,
  `a_loader_variable_naming_a_library_a_stage_may_write_is_removed_and_a_safe_one_still_loads`,
  builds libraries that write a marker file when the loader runs them. For `LD_PRELOAD`,
  `LD_AUDIT`, `LD_LIBRARY_PATH` and `GCONV_PATH` (a converter module named by a `gconv-modules`
  file) it starts a program through a kept start and a checked start, and asserts only whether a
  marker is present, since the count of constructor runs depends on the loader. The converter
  module exports no converter, so glibc aborts the `GCONV_PATH` program after
  the module's constructor has run, and the test does not assert that program's exit status. The second,
  `locpath_and_nlspath_reach_a_process_only_when_they_name_a_directory_a_stage_cannot_write`,
  starts a program that reports its environment. It shows what the start hands the process and
  runs no loader or locale code. With no `cc` a test prints `skipped`, and with `CI` set it fails.
  The third,
  `ld_origin_path_and_musl_locpath_reach_a_process_only_when_they_name_a_directory_a_stage_cannot_write`,
  is of the same kind for `LD_ORIGIN_PATH` and `MUSL_LOCPATH`. It does not show what the loader
  does with `LD_ORIGIN_PATH`, which glibc reads only when it cannot read `/proc/self/exe`
  (see below). That run was not made.
  The names come from ld.so(8) on man7.org, which was read. Its secure-execution section says that
  in that mode "the effects of some environment variables are voided or modified, and furthermore
  those environment variables are stripped from the environment", and lists `GCONV_PATH`,
  `GETCONF_DIR`, `HOSTALIASES`, `LOCALDOMAIN`, `LD_AUDIT`, `LD_DEBUG`, `LD_DEBUG_OUTPUT`,
  `LD_DYNAMIC_WEAK`, `LD_HWCAP_MASK`, `LD_LIBRARY_PATH`, `LD_ORIGIN_PATH`, `LD_PRELOAD`,
  `LD_PROFILE`, `LD_SHOW_AUXV`, `LOCPATH`, `MALLOC_TRACE`, `NIS_PATH`, `NLSPATH`,
  `RESOLV_HOST_CONF`, `RES_OPTIONS`, `TMPDIR` and `TZDIR`. Its entry for `LD_ORIGIN_PATH` says it
  is ignored in secure-execution mode since glibc 2.4. glibc's `sysdeps/generic/unsecvars.h`
  (fetched from sourceware.org, version not recorded) lists these and also `GLIBC_TUNABLES`,
  `LD_BIND_NOT`, `LD_BIND_NOW`, `LD_PROFILE_OUTPUT`, `LD_VERBOSE`, `LD_WARN` and eight `MALLOC_`
  names. Both files show which names glibc distrusts under privilege. Neither says which of the names
  outside the table select code, so the source was read as well: glibc 2.40 (the release tarball
  from ftp.gnu.org, with `raw.githubusercontent.com/bminor/glibc/glibc-2.40` for single files and
  `dl-tunables.list` of glibc 2.41 and 2.42) and musl 1.2.5 (musl.libc.org release tarball). Every
  `getenv` and `secure_getenv` call in the library code of both was listed, and each name below was
  decided from the code that reads it. No Linux host was available, so no loader or library was
  run, and every decision rests on that reading.
  The table holds `GCONV_PATH`, `LD_AUDIT`, `LD_LIBRARY_PATH`, `LD_ORIGIN_PATH` and `LD_PRELOAD`,
  which name a library, a module or a directory searched for libraries (glibc `elf/rtld.c`
  `process_envvars` lines 2565 to 2728, `elf/dl-load.c` line 833, `iconv/gconv_cache.c` line 56 with
  `iconv/gconv_conf.c` lines 372 to 410, which load a converter module from each directory).
  `GCONV_PATH` is the only environment variable the `.c` and `.h` files of glibc's `iconv/`
  directory read: a search of that directory at tag glibc-2.40 (read from source, not run) for
  `getenv`, `secure_getenv` and `__libc_enable_secure` finds one call, `getenv ("GCONV_PATH")` in
  `iconv/gconv_cache.c` line 56, which also turns the converter cache off when the variable is set.
  `iconvdata/`, the modules themselves, was not searched, so a variable a converter module reads
  is not examined. The table also holds `LOCPATH` (`Names::Directories`; `locale/newlocale.c`
  reads locale data from each
  entry), `NLSPATH` (`Names::Options`, since it is a list of templates; `catgets/catgets.c` lines 45
  to 65 and musl `src/locale/catopen.c` line 37) and `MUSL_LOCPATH` (`Names::Directories`; musl
  `src/locale/locale_map.c` line 66). `LOCPATH`, `NLSPATH` and `MUSL_LOCPATH` name locale and message
  data the library maps and parses, not code, and are kept because the data selects what the library
  interprets and #3810 lists them. A maintainer who holds that data is not code can remove them.
  `LD_ORIGIN_PATH` is read only when glibc cannot read `/proc/self/exe`
  (`sysdeps/unix/sysv/linux/dl-origin.c`), so it has an effect only in a process with no `/proc`.
  musl reads `LD_LIBRARY_PATH` and `LD_PRELOAD` only when the process is not secure
  (`ldso/dynlink.c` lines 1819 to 1825); a checked start removes them by name for either loader. musl
  reads no other loader variable. musl `ldso/dynlink.c` `path_open` splits `LD_LIBRARY_PATH` on `:`
  and newline and `load_preload` splits `LD_PRELOAD` on whitespace and `:`; glibc splits
  `LD_LIBRARY_PATH` on `:` and `;` and `LD_PRELOAD` on space and `:`. A value that joins entries
  with a separator `PATH` does not use is read as one nonexistent entry and removed.
  Names left out, with the code that was read:
  - `GLIBC_TUNABLES`. In glibc 2.40, 2.41 and 2.42 the only tunables of type STRING are
    `glibc.cpu.hwcaps`, `glibc.cpu.x86_ibt`, `glibc.cpu.x86_shstk` and `glibc.cpu.name`
    (`sysdeps/x86`, `sysdeps/aarch64`, `sysdeps/powerpc`, `sysdeps/s390`, `sysdeps/loongarch`
    `dl-tunables.list`). They select CPU features and hwcaps subdirectory names. No tunable names a
    file or directory, including `glibc.rtld.*`. Other releases and other architectures were not
    read.
  - `GETCONF_DIR`. `posix/getconf.c` executes a program from it only when not every `_SC_V*` and
    `_SC_XBS5_*` environment constant is defined (line 510 against lines 527 to 625). They are
    defined in glibc 2.40 `bits/confname.h`, so that branch is not compiled there.
    `sysdeps/posix/sysconf.c` line 1225 only calls `stat` on a name under it. No start in `crates/`
    runs `getconf` on Linux.
  - `TZDIR` (`time/tzfile.c` line 150), `RESOLV_HOST_CONF` (`resolv/res_hconf.c` line 277) and
    `HOSTALIASES` (`resolv/res_context_hostalias.c` line 103) name a time zone file, a resolver
    configuration file and an alias file, which are parsed as data. `TZDIR` is the same kind of name
    as `LOCPATH`, and is left out because the brief of #3810 does not list it as selecting code.
  - `MALLOC_TRACE` (`malloc/mtrace-impl.c` line 179) names a file that is opened for writing. The
    other `MALLOC_*` names are numbers (`elf/dl-tunables.list` `env_alias`). The malloc debugging
    hooks load only through `LD_PRELOAD` of `libc_malloc_debug.so` (glibc `NEWS` for 2.34), which
    the table covers.
  - `LD_PROFILE`, `LD_PROFILE_OUTPUT` and `LD_DEBUG_OUTPUT` name a library to profile or a place
    to write; `LD_BIND_NOW`, `LD_BIND_NOT`, `LD_DEBUG`, `LD_DYNAMIC_WEAK`, `LD_SHOW_AUXV`,
    `LD_VERBOSE` and `LD_WARN` are flags. `LD_HWCAP_MASK` is not read by glibc 2.40. None is
    loaded or run. `LOCALDOMAIN`, `RES_OPTIONS`, `NIS_PATH` and `TMPDIR` are strings and not paths
    to code.
  `ci.yml` has no musl or Alpine job. `linux_loader.rs` uses a glibc converter module and
  `RUNPATH`, so it is written for glibc.
- `systemd-inhibit` and `systemctl --user` are given the generic table and one or two names of their
  own. No Linux run was made for either; the decisions rest on systemd v255 source (v257 for a
  cross-check of the names), and the manual pages systemctl(1) and systemd-inhibit(1) from
  freedesktop.org (latest) were read as well. Not covered: no `systemd-inhibit` or `systemctl` was
  started with any of these names, so that sd-bus runs the program a `unixexec:` address names, and
  that `systemctl` ignores `DBUS_SYSTEM_BUS_ADDRESS` for user scope, are read from source and not
  observed. A distribution's systemd differs from the v255 tag in patches and build options, and
  was not read. Whether `systemctl enable` reads `SYSTEMD_UNIT_PATH` on a booted host is the third
  item of the not-settled bullet below.
  - `systemd-inhibit` (`crates/tui/src/caffeinate.rs`, `start_program`, through
    `crates/tui/src/powershell.rs`) is given `DBUS_SYSTEM_BUS_ADDRESS` as `Names::Options`.
    `sd_bus_default_system` reads it (`src/login/inhibit.c` line 268, `src/libsystemd/sd-bus/sd-bus.c`
    lines 1280 to 1290) and accepts a `unixexec:` address, for which `bus_socket_exec`
    (`src/libsystemd/sd-bus/bus-socket.c` lines 1066 to 1069) calls `execvp` on its `path=`.
    `secure_getenv` ignores the variable only in a privileged process, which this start is not.
    No `Names` kind judges an address, so the variable is removed whatever it holds, including a
    plain `unix:path=` address. sd-bus then uses `unix:path=/run/dbus/system_bus_socket`. A person
    whose system bus is elsewhere gets an inhibitor that exits at once, which `/caffeinate`
    reports as no hold. A rule that kept a `unix:path=` address outside every stage-writable
    directory and refused every other transport would keep that case working. It needs a new
    `Names` kind and a test, for a variable few people set, and was not built.
    `DBUS_STARTER_ADDRESS` and `DBUS_STARTER_BUS_TYPE` are read only by `sd_bus_default` and
    `sd_bus_open`, which `inhibit.c` does not call. `SYSTEMD_PAGER` and `PAGER` are read by
    `pager_open`, which `inhibit.c` line 62 calls only for `--list`, which no start passes. The
    start sends stdout and stderr to `/dev/null`.
  - `systemctl --user` (`crates/schedule/src/schedule.rs`: `--user daemon-reload` twice,
    `--user enable --now` and `--user disable --now`, all through `succeeds`) is given
    `DBUS_SESSION_BUS_ADDRESS` as `Names::Options` and `SYSTEMD_UNIT_PATH` as `Names::Directories`.
    `bus_connect_user_systemd` (`src/shared/bus-util.c` lines 234 to 268) falls back to
    `sd_bus_default_user` when `XDG_RUNTIME_DIR` is unset or its private socket cannot be
    connected to, and that reads `DBUS_SESSION_BUS_ADDRESS` (`sd-bus.c` line 1343) with the same
    `unixexec:` handling as above. Removing it leaves `unix:path=$XDG_RUNTIME_DIR/bus`. A person
    on another session bus address has the scheduler's `systemctl` calls fail, which `succeeds`
    reports as false. `SYSTEMD_UNIT_PATH` (`src/basic/path-lookup.c` line 588) is a list of
    directories searched first for unit files by a client-side `enable`
    (`src/systemctl/systemctl-util.c` lines 873 to 894). `DBUS_SYSTEM_BUS_ADDRESS` is read only for
    system scope. `SYSTEMD_PAGER`, `PAGER`, `SYSTEMD_EDITOR`, `EDITOR` and `VISUAL` are reached only
    by `list-*`, `show`, `status`, `edit` and help, which no start passes. `SYSTEMD_LIB_DIR` is not
    in v255 or v257. `XDG_CONFIG_HOME` and `XDG_DATA_HOME` are the values `schedule.rs` itself
    uses to place the unit, and are left as set.
- `xclip` and `xsel` are given the generic table and no names of their own. No Linux run was made.
  Not covered: that neither tool loads a library or runs a program under any variable was not
  observed, and a libX11 built with `--enable-loadable-i18n` was not read (the fourth item of the
  not-settled bullet below).
  xclip 0.13 (`xclip.c`, `xclib.c`) has no `getenv`, `exec*`, `dlopen`, `popen` or `system(`, and
  xsel 1.2.1 (`xsel.c`) reads only `XDG_CACHE_HOME` and `HOME` (lines 354 and 355), to place a
  state directory it writes to. libX11 1.8.7, libxcb 1.16 and libXau 1.0.11 were read for every
  `getenv`: `DISPLAY` selects a socket or a TCP peer, `XAUTHORITY`, `HOME` and `USERNAME` select
  the cookie file (`AuFileName.c`), `XENVIRONMENT` a resource file, and `XLOCALEDIR`,
  `XLOCALELIBDIR` (`src/xlibi18n/lcFile.c`), `XCOMPOSEFILE` and `XCOMPOSECACHE` locale and
  compose data that is parsed. `XLOCALEDIR` is read only on the locale path, which neither tool
  reaches. A server a stage wrote can read what is copied, which this entry does not decide.
  The starts are in `crates/tui/src/clipboard.rs`.
- `wl-copy` is given `GIO_EXTRA_MODULES`, `GIO_MODULE_DIR`, `QT_PLUGIN_PATH`, `PERL5LIB` and
  `PERLLIB` (`WL_COPY_CODE_SELECTORS` in `crates/tui/src/powershell.rs`). No Linux run was made.
  Not covered: which of the programs `xdg-mime` starts runs on a given desktop was not observed,
  the source of `qtxdg-mat` (LXQt) was not read, and whether the Perl `mimetype` runs in taint mode
  was not settled (the first and second items of the not-settled bullet below), so the list can be
  short.
  The start passes no `--type`, so `wl-copy` runs `xdg-mime query filetype` on the content
  (wl-clipboard v2.2.1 `src/wl-copy.c` lines 266 to 267), and `xdg-mime` (xdg-utils v1.2.1
  `scripts/xdg-mime.in` lines 63 to 129 and 684 to 700) runs `gio info`, `kmimetypefinder`,
  `qtxdg-mat` or `mimetype` by name, chosen by the desktop. `GIO_EXTRA_MODULES` (a list of
  directories) and `GIO_MODULE_DIR` (one directory) are loaded from by GLib 2.80.0
  (`gio/giomodule.c` lines 1310 and 1249). `QT_PLUGIN_PATH` is a list of plug-in directories (Qt
  5.15.12 `qcoreapplication.cpp` lines 2728 and 2939), kept as a precaution since it was not shown
  that `kmimetypefinder` loads one. `PERL5LIB` and `PERLLIB` are added to `@INC` when Perl is not
  in taint mode (perl v5.38.0 `perl.c` lines 4823 to 4830), for the Perl `mimetype`. `wl-copy`
  itself reads `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`, `XDG_ACTIVATION_TOKEN` and
  `DESKTOP_STARTUP_ID`, which select a socket or are tokens. It does not read `WL_SEAT`; `--seat`
  is an argument. `GDBus` has no `unixexec:` transport (`gio/gdbusaddress.c`), so
  `DBUS_SESSION_BUS_ADDRESS` is not a code selector for `gio info`. `XDG_CURRENT_DESKTOP`,
  `DESKTOP_SESSION`, `KDE_SESSION_VERSION` and `GNOME_DESKTOP_SESSION_ID` choose which of those
  programs runs and are not code selectors, but a stage that sets them changes which of the names
  above matter. Passing `--type text/plain` so that `xdg-mime` never runs would remove the
  exposure; it was not done because it declares a type the content may not have.
- Not settled for the five programs above. Each needs a Linux host:
  - `xdg-mime` under LXQt runs `qtxdg-mat mimetype` (`xdg-mime.in` lines 114 to 128), whose source
    was not read. Run `XDG_CURRENT_DESKTOP=LXQt strace -f -e trace=execve,openat xdg-mime query
    filetype /tmp/x 2>&1 | grep -E 'execve|\.so'`.
  - Whether the Perl `mimetype` runs in taint mode, which ignores `PERL5LIB`, was not settled; the
    module source could not be fetched. Run `head -1 $(command -v mimetype)` and
    `PERL5LIB=/tmp/p strace -f -e trace=openat mimetype /tmp/x`.
  - Whether `systemctl enable` reads `SYSTEMD_UNIT_PATH` on a booted host, where
    `install_client_side()` is false, was not traced in `src/shared/install.c`. Run
    `SYSTEMD_UNIT_PATH=/tmp/u strace -f -e trace=openat systemctl --user enable --now <timer>`. If
    nothing under `/tmp/u` is opened, the name can be dropped.
  - Distribution builds of libX11 with `--enable-loadable-i18n` were not read. Run
    `ldd $(command -v xclip)` and `strings $(readlink -f /usr/lib/x86_64-linux-gnu/libX11.so.6) |
    grep -c xi18n`, or `strace -f -e trace=openat,execve env XLOCALEDIR=/tmp/loc xclip -selection
    clipboard -o` and check that nothing under `/tmp/loc` is opened.
- A Homebrew or pip `aws` script, an asdf or pyenv shim, `gio open`, `kde-open` and `osascript` were
  examined for the variables they read to choose code, by running each where it could be run, and
  the names are in the generic table or in the list of the start that runs the program (the AWS
  CLI's, the browser opener's, `osascript`'s). The runs were on macOS 26.6 (arm64) with an
  environment holding only the variable under test, `PATH=/usr/bin:/bin` and a scratch `HOME`, and
  each name below is marked **run** (a planted helper ran) or **source** (read in the file named,
  not run). A name that ran nothing is recorded as such. No run found every variable a program
  reads, so each list can be incomplete ([PATH-008](../best-practices/paths.md#PATH-008)).
  - **Bash scripts** (every asdf and pyenv shim is `#!/usr/bin/env bash`). Run under bash 3.2.57
    (`/usr/bin`) and 5.3.20 (Homebrew) with the same results: `SHELLOPTS=xtrace` with a `PS4` that
    holds a command substitution ran the command; an exported function `BASH_FUNC_<name>%%` ran
    when the script called a command of that name (`BASH_FUNC_pyenv-version-name%%` and
    `BASH_FUNC_cp%%` ran; `BASH_FUNC_readlink%%` did not);
    `BASH_ENV` ran a file. `BASHOPTS` ran nothing. These are in the generic table, since they apply
    to any bash script. The test `a_bash_script_runs_the_code_in_shellopts_ps4_and_an_exported_function`
    runs a planted bash script that way.
  - **pyenv** (commit 9e40c078 of `pyenv/pyenv`, with the shims `pyenv rehash` writes). Run:
    `PYENV_HOOK_PATH` (a `.bash` file under `<dir>/exec/` was sourced), `PYENV_ROOT` (`pyenv exec`
    sourced a hook from `<root>/pyenv.d/exec`; an installed shim sets its own `PYENV_ROOT`, so the
    name matters when `pyenv` itself is started), `PYENV_VERSION=../../../evil` (resolved to
    `$PYENV_ROOT/versions/../../../evil/bin/python3`, which ran; an absolute path did not).
    `HOME` selects `$HOME/.pyenv` when `PYENV_ROOT` is unset, and a hook under it ran with
    `pyenv exec`. A checked start of a pyenv shim removes `HOME` when a stage may write what it
    reaches (the paragraph after the generic table). Nothing ran under `PYENV_DIR`, `_PYENV_SHIM_PATH`, `PYENV_NATIVE_EXT` or
    `PYTHONPATH` (the planted Python never
    started). `PYENV_DEBUG` was run only together with a `PS4` that holds a command substitution,
    which ran it, as the `PS4` row above does. A plug-in under
    `PYENV_ROOT/plugins/*/bin` was not run by `pyenv version-name` and is not claimed.
  - **asdf** (release binary v0.20.2, commit cb72590, darwin-arm64, whose shims are
    `exec asdf exec "<tool>" "$@"`). Run: `ASDF_DATA_DIR` (a plug-in `exec-env` and an installed
    tool ran from the directory), `ASDF_CONFIG_FILE` (a `pre_<tool>_<command>` line of the file
    ran), `ASDF_<TOOL>_VERSION=path:<dir>` (ran `<dir>/bin/<tool>`; `ASDF_FOO_VERSION=2.0.0` ran
    nothing; the name holds the tool, so it is matched by shape in `CODE_SELECTOR_PATTERNS`) and
    `HOME` (with a `.tool-versions`, `~/.asdf/installs/<tool>/<version>/bin/<tool>` ran). Nothing
    ran under `ASDF_DIR`, `ASDF_CONCURRENCY` or `ASDF_TOOL_VERSIONS_FILENAME`. `HOME` is in
    the AWS CLI's list and the opener's and in no other list, because gcloud, `ssh` and `git` need it,
    so a checked start judges it for an asdf or pyenv shim alone, as the paragraph after the generic
    table states.
  - **`aws` script**: the names, and the versions, are in the AWS table above. Run: Homebrew
    `awscli` 2.39.7 and pip `awscli` 1.46.3 (scripts), and the frozen 2.37.12 executable. Version
    2.39.7 was also read from source.
  - **`gio open`**. Run on macOS with Homebrew `gio` 2.90.1: a planted library whose constructor
    writes a marker ran under `GIO_EXTRA_MODULES` and under `GIO_MODULE_DIR`, for `gio open
    x-probe://a` and for `gio open /etc/hosts`, and did not run without them. Source, GLib 2.86.0
    (commit e33be08bda99): `GIO_EXTRA_MODULES` and `GIO_MODULE_DIR` in `gio/giomodule.c` (both
    ignored in a setuid program), `GIO_LAUNCH_DESKTOP` in `gio/gdesktopappinfo.c`, the four `XDG_`
    names and `HOME` in `glib/gutils.c`. Handler selection through `.desktop` entries is not
    observable on macOS, where `gio/gio-tool-open.c` skips it, and was run on Linux with 2.86.0
    for the four `XDG_` names (the browser opener bullet below). Read in the files named, with no
    load of code found: `GSETTINGS_SCHEMA_DIR`,
    `GSETTINGS_BACKEND` and `GIO_USE_VFS` (which choose among modules already loaded). The session
    bus is reached through `DBUS_SESSION_BUS_ADDRESS` and `XDG_RUNTIME_DIR`, which the opener
    judges by where the socket is, because what a bus a stage runs answers to a handler lookup was
    not run or read.
  - **`kde-open`** was not run: it cannot run on macOS and was not run on Linux. A test added with
    #3884 runs it once per name with a marker handler, and a CI step runs that test under
    `dbus-run-session`; no name has been observed yet. Its names are
    from source only, read at these commits, none of which is a release tag: kde-cli-tools
    aa1f650a282c (`kioclient/CMakeLists.txt` builds `kde-open` from `kioclient.cpp` with
    `KIOCLIENT_AS_KDEOPEN`), KIO a80751d2, KService 2bd4461c and KCoreAddons e96958b2, and Qt 6.10.0
    (tag v6.10.0, commit 5a8637e4516b). `QT_PLUGIN_PATH` (`qcoreapplication.cpp`) and
    `QT_QPA_PLATFORM_PLUGIN_PATH` (`qguiapplication.cpp`) name directories plug-ins load from,
    `QT_QPA_PLATFORMTHEME`, `QT_QPA_GENERIC_PLUGINS` and `QT_QPA_PLATFORM` (`qguiapplication.cpp`)
    and `QT_STYLE_OVERRIDE` (`qapplication.cpp`) name the plug-in loaded from them, and
    `KDESYCOCA` (`ksycoca.cpp` line 698) names the cache the `Exec` line of a handler is read from.
    The directories the handler is looked for in come from Qt's `QStandardPaths`, which reads
    `XDG_DATA_HOME` (`qstandardpaths_unix.cpp` line 227), `XDG_CONFIG_HOME` (line 247),
    `XDG_DATA_DIRS` (line 394) and `XDG_CONFIG_DIRS` (line 406), and KService's `KMimeAssociations`
    (`kmimeassociations.cpp` line 71) and `KSycoca` (`ksycoca.cpp` lines 695 and 707) take their
    `mimeapps.list` and `.desktop` directories and the cache from it.
    `KSycoca` puts the cache file under the generic cache directory, which is `XDG_CACHE_HOME`
    (`qstandardpaths_unix.cpp` line 189) or `HOME/.cache`, when `KDESYCOCA` is not set.
    `XDG_CACHE_HOME` is not in the opener's list, and whether it should be is not decided here.
    `XDG_CURRENT_DESKTOP` is read by `KService::showInCurrentDesktop` (`kservice.cpp` line 445)
    and by `KMimeAssociations` (`kmimeassociations.cpp` line 47), to choose which entries are shown
    and the name `<desktop>-mimeapps.list`. These reads are from source and none was observed in a
    run. Which release of KDE Frameworks or Plasma these names hold for is not recorded. `KDEHOME`,
    `KDEDIRS`, `KDE_FULL_SESSION`, `KDE_SESSION_VERSION`, `KDE_FORK_SLAVES` and the `QML*_IMPORT_PATH`
    names have no read in the files searched, which does not show they are never read. Handler
    selection was read at the default-branch commits kio ad78db8a, KService 2bd4461c and KConfig
    eb4dec9c, and qtbase `dev` 7d402fc0: `KIO::OpenUrlJob` (`openurljob.cpp`) asks
    `KApplicationTrader::preferredService` for the scheme or the MIME type, which reads
    `mimeapps.list` and the service cache. `QStandardPaths` (`qstandardpaths_unix.cpp`) reads
    `XDG_DATA_HOME`, `XDG_DATA_DIRS`, `XDG_CONFIG_HOME` and `XDG_CONFIG_DIRS` and falls back on
    `HOME`. `KMimeAssociations` (`kmimeassociations.cpp`) searches the config directories, then the
    `applications` directories, for `mimeapps.list` and for `<desktop>-mimeapps.list`, where
    `<desktop>` comes from `XDG_CURRENT_DESKTOP`. `KBuildSycoca` (`kbuildsycoca.cpp`) reads the
    `.desktop` files from the `applications` directories, and the cache file name is built from
    the cache location and a hash of the data directories unless `KDESYCOCA` names it
    (`ksycoca.cpp`, `KSycoca::absoluteFilePath`). `XDG_CURRENT_DESKTOP` also filters entries by
    `OnlyShowIn` and `NotShowIn` (`kservice.cpp`, `showInCurrentDesktop`) where the caller asks for
    that. So the four `XDG_` directory names select the program `kde-open` runs, and
    `XDG_CURRENT_DESKTOP` selects among `mimeapps.list` files. `kprocessrunner.cpp` starts the
    program through systemd on the session bus when the bus reports systemd, and otherwise forks it;
    read so, a session bus may not be required, and the child gets the process environment. This is
    from source and was not run, and whether `kde-open` needs a session bus is not established. The
    `ci.yml` step that runs `kde-open` does so under `dbus-run-session`, so a bus exists there. A run
    of `kde-open` on Linux is tracked by #3822 and #3884.
- The browser opener's `BROWSER` and `XDG_*` names ([CHATGPT-13](chatgpt-sign-in.md#CHATGPT-13))
  were run on one Linux host against `xdg-open` and `xdg-mime` 1.2.1 and `gio` 2.86.0, for the
  cells marked run below. Every test added after that run is not run: the host here is macOS,
  the opener tests are Linux-only and print `skipped` elsewhere, and CI has not yet run them. In the
  `check` job on `ubuntu-latest`, `ci.yml` installs `xdg-utils`, `libglib2.0-bin` and
  `kde-cli-tools`. On Linux with `CI` set, the generic-mode `XDG_*`, GNOME and KDE tests fail when
  `xdg-open`, `gio` or `kde-open` is missing, and the generic-mode and GNOME tests also fail when
  the installed handler does not start. The KDE test skips a name whose installed handler does not
  start, since `kde-open` was run through `xdg-open` on the CI runner, which has no session bus, and ran
  no installed handler. `kde_open_run_with_each_name_set_to_a_marker_runs_the_marker_only_for_names_the_opener_removes`
  runs `kde-open` itself under a bus with `HOME` and the `XDG_` names pointing at scratch
  directories, once without a planted name and once per name in `KDE_OPEN_MARKER_NAMES`, and the
  `ci.yml` step that runs it under `dbus-run-session` sets `BRAVEBOT_OBSERVE_KDE_OPEN`, which makes
  a missing `kde-open`, a baseline that starts no handler, or a name that runs the planted handler
  and is not removed by the opener fail the test. Names with no marker form (the plug-in
  directories and names, `QT_STYLE_OVERRIDE`) are printed as not exercised. Without the variable
  it prints `skipped`, as it did on this macOS host. The test that puts a stand-in `kde-open` first on `PATH` and reads what
  the `xdg-open` starts it with fails on Linux with `CI` set when `xdg-open` does not start the
  stand-in, so the opener's names are checked there against a started `kde-open`. The
  generic-mode `BROWSER` test through the installed `xdg-open` is older and prints `skipped` in
  those cases on every platform. On macOS every opener test prints `skipped` even when `CI` is set.
  With no `DISPLAY` or `WAYLAND_DISPLAY`, `xdg-open` 1.2.1 does not look up a scheme handler in
  generic mode, so the generic test sets `DISPLAY`. The generic-mode test failed on the CI runner
  with the installed handler not run; its run now prints `xdg-open`'s debug output when that
  happens.
  What is covered, per name and per desktop mode:

  | Name | Generic desktop | `XDG_CURRENT_DESKTOP=GNOME` (`gio open`) | `XDG_CURRENT_DESKTOP=KDE` and `KDE_FULL_SESSION=true` (`kde-open`) |
  |---|---|---|---|
  | `BROWSER` | Test, run on Linux: the planted helper is not run and an installed one is. | No test. `xdg-open` reaches `BROWSER` in `open_generic`, which `open_gnome` reaches only without `gio` (per the 1.1.x script; not checked against 1.2.1), and the GNOME test needs `gio`. Not run. | Test, not run: the planted helper must not run if an installed `BROWSER` helper runs. If it does not, the test prints that `kde-open` does not read `BROWSER` and asserts nothing. |
  | `XDG_CONFIG_DIRS`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_DATA_DIRS` | Test, not run: the planted handler is not run and an installed one is. `xdg-mime query default` was run on Linux and returned a handler planted under each. | Test, run on Linux: `gio open` started a handler planted under each, so each selects the program that opens the address. The test was changed after that run and its new form is not run. | Test, not run: the planted handler is not run and an installed one is. |

  The opener tests are `a_browser_helper_a_stage_may_write_is_not_run_by_the_installed_xdg_open`
  (generic, `BROWSER`),
  `an_xdg_directory_a_stage_may_write_is_not_read_by_the_installed_xdg_open_in_generic_mode`,
  `an_xdg_directory_a_stage_may_write_is_not_read_by_the_installed_xdg_open_under_gnome` and
  `a_browser_or_xdg_name_a_stage_may_write_is_not_read_by_the_program_xdg_open_hands_the_address_to_under_kde`.
  The KDE test sets `KDE_SESSION_VERSION=5` where `kde-open5` is found and `6` otherwise, since
  `xdg-open` 1.2.1 runs `kfmclient exec` when it is unset, `kde-open5` for 5 and `kde-open` for 4
  and 6. The GNOME test opens an address with the scheme
  `bravebot-test`, since with `XDG_DATA_DIRS` removed `gio open` would start the host's browser
  for `https`. It sets `XDG_CURRENT_DESKTOP` on a host with no GNOME session.

  Electron 44.9.3 on Linux, run under Xvfb, was made to call `openExternal` and `openPath` of
  `ui/src/main/external-open.ts` with all five names set to directories and a script a stage could
  have written. It started `xdg-open` for each call with none of the five in its environment. The
  same two calls made on Electron's `shell` without the deletion started `xdg-open` with all five,
  so Electron hands its environment to `xdg-open` and the deletion is what removes them. The
  by-hand run was made before `GIO_LAUNCH_DESKTOP` was listed, so it set five names; the test sets
  fifteen. `ui/scripts/external-open-electron.test.mjs` repeats that run as the test case `electron
  on linux starts xdg-open without the fifteen names for openExternal and openPath through
  external-open.ts and with them without it`, which expects exactly two starts in each run. It
  needed `--no-sandbox` and `--disable-dev-shm-usage` on the host that ran it. The case `electron
  on linux starts any xdg-open for showItemInFolder without the fifteen names through
  external-open.ts and with them without it` asserts no start count, since how many times Electron
  starts `xdg-open` is Electron's choice, and fails when either run starts none. Each start through
  `external-open.ts` must carry none of the names and each control start all fifteen. It sets
  `DBUS_SESSION_BUS_ADDRESS` to a missing socket so that Electron falls back to `xdg-open`. In
  Electron v44.9.3 (`shell/common/platform_util_linux.cc`) `showItemInFolder` first checks the
  portal and `org.freedesktop.FileManager1` over the session bus, which libdbus finds through
  `DBUS_SESSION_BUS_ADDRESS`; where neither is running, or a call fails, it runs `xdg-open .` in
  the folder that holds the file, and `openPath` and `openExternal` start `xdg-open` through the
  same helper. That helper sets only the working directory, `MM_NOTTTY`, and the activation token,
  so the child gets Electron's environment. This was read, not run, and the source read does not
  show what a missing socket does to the bus connection itself: a null connection returns without
  starting `xdg-open`, which the case reports as a run that started none. Both cases print `skipped` where
  there is no Linux, `xvfb-run` or Electron binary, and on Linux with `CI` set a missing `xvfb-run`
  or Electron fails them. Neither has been run on Linux since the test was changed. The
  `showItemInFolder` case has never been run on Linux.

  The variables `xdg-open` 1.2.1 and `xdg-mime` 1.2.1 use were listed from their source. Besides
  the names in the opener's list, they read `XDG_CURRENT_DESKTOP`, `DESKTOP_SESSION`, `KDE_FULL_SESSION`,
  `GNOME_DESKTOP_SESSION_ID`, `MATE_DESKTOP_SESSION_ID` and `LXQT_SESSION_CONFIG` to choose among
  launchers whose names are fixed, `KDE_SESSION_VERSION` to complete the name `kde-open5`,
  `XDG_UTILS_REALPATH_BACKEND` to choose between `realpath` and `readlink`. They also read
  `XDG_RUNTIME_DIR`, which is in the opener's list, for a `flatpak-info` file that switches to
  `gdbus`. None of the names in this sentence names a file to run.
  `xdg-mime` 1.2.1 also reads `XDG_CURRENT_DESKTOP` and `XDG_MENU_PREFIX` as file-name prefixes
  (`<desktop>-mimeapps.list`, `<prefix>defaults.list`, `<prefix>mimeinfo.cache`) and
  `KDE_SESSION_VERSION` as a suffix of program names (`kde-open5`, `kbuildsycoca<version>`,
  `ktraderclient<version>`, `kde<version>-config`). None of the three is in the opener's list. The
  prefixes are joined to directories the opener already judges, and a file read that way supplies
  only a desktop-entry id, which is looked up in those directories. `xdg-mime` does not validate
  either prefix, so a value containing `/` can select a file outside them, and what that file
  supplies is limited in the same way. The program names built from `KDE_SESSION_VERSION` are
  resolved on the reduced `PATH`. The `eval` at `xdg-mime.in` line 37 and the `kde<version>-config`
  start at line 832 are reached only by the install, uninstall and makedefault actions, and
  `xdg-open` runs `xdg-mime query` only. These readings are from the source and were not run.
  `gio open` was read in GLib 2.88.4 (`gio-tool-open.c`, `gappinfo.c`, `gdesktopappinfo.c`,
  `gportalsupport.c`, `gio-launch-desktop.c`, `glib/gutils.c`); the runs above used `gio` 2.86.0.
  `gio open` never reads `BROWSER`. It reads `XDG_DATA_HOME`, `XDG_DATA_DIRS`, `XDG_CONFIG_HOME`,
  `XDG_CONFIG_DIRS` and `HOME` for the directories that hold `mimeapps.list` and the `.desktop`
  files whose `Exec` line it runs, `PATH` to find that program, and `XDG_CURRENT_DESKTOP` only as a
  file-name prefix, accepted only when it is letters, digits, `-` and `_`. It also reads
  `GIO_LAUNCH_DESKTOP`, the path of the wrapper it executes with the handler's whole command line
  (`gdesktopappinfo.c` line 3100; ignored when the process is setuid). `GIO_LAUNCH_DESKTOP` is in
  the opener's list, and
  `gio_launch_desktop_is_removed_for_a_wrapper_a_stage_may_write_and_kept_for_an_installed_one`
  holds it on the disk. The other names it reads do not choose a program or directory:
  `GIO_USE_PORTALS` (first character only), `GIO_LAUNCHED_DESKTOP_FILE` (a journal identifier),
  `PARENT_WINDOW_ID` (read from a launch context, which `gio open` does not pass).
  `gio open` connects to the session bus before it chooses a handler. A listener bound to
  `<dir>/bus` received the bytes `\x00AUTH\r\n` from a `gio` 2.86.0 run on Linux with
  `XDG_RUNTIME_DIR=<dir>`, and the same from a run with
  `DBUS_SESSION_BUS_ADDRESS=unix:path=<dir>/bus` and no `XDG_RUNTIME_DIR`. That Linux run is
  unreproduced on this host: it is the measurement in
  [#3932](https://github.com/brave/bravebot/issues/3932), and no Linux host ran it for this
  change. The same program on macOS with `gio`
  installed through Homebrew did not connect to either socket and exited with `Operation not
  supported`. With `GIO_USE_PORTALS=1`, which makes `gio` use the desktop portal, it connected, and a
  listener written in Python that
  answered the D-Bus authentication, `Hello` and `GetNameOwner` received, for
  `XDG_RUNTIME_DIR=<dir>` and for `DBUS_SESSION_BUS_ADDRESS=unix:path=<dir>/bus` alike, a call of
  `org.freedesktop.portal.OpenURI.OpenURI` carrying the address `gio open` was given. The listener
  answered that call with an empty reply, `gio` called `GetNameOwner` once more, and the run was ended by
  the harness's 8 second timeout, with exit status -15 and no message from `gio`. So a
  service a stage listens with learns what the person opens through the opener, and both names
  are in the opener's list for that reason, and
  `the_opener_removes_the_runtime_directory_and_bus_address_that_lead_to_a_socket_a_stage_may_write`
  and `a_bus_socket_is_judged_by_where_it_leads_on_the_disk` hold the judgment. Whether a service
  can make `gio open` run code or choose a handler was not shown, and is not recorded as
  impossible: the listener did not complete the portal call, the run forced the portal path and
  not the desktop handler lookup a GNOME session uses, and no Linux host ran it. The names are
  removed and not recorded as harmless. When
  `DBUS_SESSION_BUS_ADDRESS` is removed and `XDG_RUNTIME_DIR` is kept, `gio` finds the bus at
  `$XDG_RUNTIME_DIR/bus`; when both are removed it has no session bus to find, may start one with
  `dbus-launch`, and otherwise opens the address without the services a bus provides. That is the
  cost of removing them. It applies to every value the judgment removes: one that leads into a
  directory a stage may write or that does not exist, a runtime directory that is empty or
  relative, and an address that is abstract, `tcp:`, `launchd:`, repeated-key, contains `%` or is
  otherwise refused, whatever it points to.
  `kde-open` was read in `kioclient.cpp` of kde-cli-tools (commit `aa1f650`) and in `openurljob.cpp`,
  `kprocessrunner.cpp` and `desktopexecparser.cpp` of KIO (commit `608d332`), taken from the GitHub
  mirror of the KDE repositories. Those files read `PATH` directly (`desktopexecparser.cpp`
  line 296), reference no `BROWSER`, and read no `XDG_*` or `KDE_*` variable by name.
  `kprocessrunner.cpp` sets `XDG_ACTIVATION_TOKEN` in the environment of the process it starts and
  does not read it. The libraries they call do read `XDG_*` names, as the `kde-open` entry above
  records (`QStandardPaths`, `KMimeAssociations`, `KSycoca`; KService 2bd4461c and Qt 6.10.0), so
  reading nothing by name in those files does not mean `kde-open` ignores the `XDG_` names. The
  terminal emulator comes from the KDE configuration file and not from the environment.
  `HOME` is in the opener's list and is removed when it names a directory a stage may write. With
  `XDG_CURRENT_DESKTOP=GNOME` and no other name set, an
  `xdg-open` run with `HOME` naming a directory that held a `mimeapps.list` and a `.desktop` entry
  started the planted handler through `gio open`.
  `a_home_a_stage_may_write_is_not_read_by_the_installed_xdg_open_under_gnome` runs that, and
  `a_home_is_judged_with_the_paths_the_opener_reads_under_it_on_the_disk` runs the judgment on the
  disk. What `gio` does with `HOME` removed was not run: the test removes nothing it can see
  `gio` fall back from, and does not run on a host where the account has handlers of its own.
  These were not run: a D-Bus service that answers `gio open` after the portal call it receives, so
  that what it can make `gio open` start is unobserved; the Windows `rundll32` start, which was read, which no test on another
  system can stand in for and which is tracked by #3860; `kde-open` and a KDE session
  (`XDG_CURRENT_DESKTOP=KDE` with `kde-open`), because `kde-open` is not installed on the host that
  ran the rest and installing it there with apt would add 152 packages, so what it reads is
  unobserved (the marker test and CI step added with #3884 are meant to observe it); a logged-in GNOME session, since the GNOME runs set `XDG_CURRENT_DESKTOP` on a host
  with none; `XDG_DATA_DIRS` through the installed `xdg-open` in generic mode, since removing a
  planted value there lets `xdg-mime` and `gio open` use the system's handlers and start a real
  browser, so `xdg_data_dirs_a_stage_may_write_is_not_passed_by_the_opener_in_generic_mode` runs a
  stand-in for `xdg-open` instead; Qt's `QDesktopServices` and
  `QStandardPaths` and KDE's `KService`, `KSycoca` and `KConfig`, which were read later (see the
  `kde-open` entry above), so that `kde-open` chooses its handler through `XDG_DATA_HOME`,
  `XDG_DATA_DIRS`, `XDG_CONFIG_HOME`, `XDG_CONFIG_DIRS`, `XDG_CACHE_HOME` and
  `XDG_CURRENT_DESKTOP` is supported by source and unobserved, so it is unverified by a run; the
  parts of GLib not
  read (the `gcontenttype` implementations, `gsandbox.c`, the splitting of `PATH` in `gspawn.c` and
  the passwd fallback for `HOME`); and a run of `gio open` or `kde-open` with `GIO_LAUNCH_DESKTOP`
  set, which is shown only by a test of the opener's judgment of the value. The list of variables
  the launchers read is therefore complete only for `xdg-open` and `xdg-mime` 1.2.1 and, for the
  files named above, `gio open` and `kde-open`.
  They are tracked by #3822.
- **The starts that keep `XDG_RUNTIME_DIR` and `DBUS_SESSION_BUS_ADDRESS`.** The desktop
  application, the AWS CLI (`crates/bedrock/src/credentials.rs`), the gcloud launcher
  (`crates/agent/src/gcloud.rs`) and the programs of `doctor --push-identity`
  (`crates/cli/src/push_identity.rs`) do not judge either name. The application cannot delete
  them: Electron reaches the session bus through them, the agent's proxy directory is made under
  `XDG_RUNTIME_DIR` and the agent is started with the application's environment. Deleting them for
  the span of one call is not shown to reach the moment Electron starts `xdg-open`. The other three
  start `xdg-open` through Python's `webbrowser` or a credential helper, so under a GNOME session
  `gio open` connects to the bus the value names, and the run in the `gio open` text above shows
  that a service there receives the address being opened, which for a sign-in is the sign-in page's.
  The code gives one reason per start. The tests of the AWS start (`credentials.rs`) and the
  gcloud start (`gcloud.rs`) say the sign-in or login is run to read `DISPLAY` and
  `XDG_RUNTIME_DIR`, and the code does not say which program reads `XDG_RUNTIME_DIR` or why. The
  test of `doctor --push-identity` (`push_identity.rs`) says `HOME`, `SSH_AUTH_SOCK`,
  `XDG_RUNTIME_DIR` and `DISPLAY` are variables the programs started there are run to read, and
  gives no reason specific to `XDG_RUNTIME_DIR`. None of the three judges
  `DBUS_SESSION_BUS_ADDRESS` or says why it is passed on. The sandbox usability suite
  (`crates/agent/src/usability.rs`) lists neither name, and its list has `BROWSER` for the
  commands `cargo doc --open` opens the documentation with. The person's own runtime directory is
  normally outside every directory a stage may write. The cost is that a person whose value leads
  into such a directory
  has the address received by a stage that listens there, and whether more than that can happen
  was not shown. [#3940](https://github.com/brave/bravebot/issues/3940) tracks removing this
  cost. The tests
  `the_aws_start_keeps_the_variables_a_sign_in_is_run_to_read`,
  `the_gcloud_start_keeps_the_display_and_runtime_directory_a_login_reads` and
  `the_variables_the_programs_started_here_are_run_to_read_are_kept` hold the kept names.
- `osascript` was run on macOS 26.6 (arm64) with `/usr/bin/osascript` and `env -i
  PATH=/usr/bin:/bin`. A compiled script library `Marker3809.scpt` (built with `osacompile`) in
  `<home>/Library/Script Libraries`, whose handler appends to a marker file, was used with
  `osascript -e 'use lib : script "Marker3809"' -e "return lib's ping()"`. With `HOME=<home>` it
  failed with `Can't get script "Marker3809" (-1728)` and wrote no marker. With
  `CFFIXED_USER_HOME=<home>`, whatever `HOME` was, it printed `1` and wrote the marker. `path to
  scripting additions from user domain`, `path to library folder from user domain`, `path to home
  folder` and `path to application support from user domain` all resolved to the account's home
  under a changed `HOME` and to `<home>` under `CFFIXED_USER_HOME`. So `osascript` follows
  `CFFIXED_USER_HOME` and not `HOME`, and `CFFIXED_USER_HOME` is its list. A plain-text
  `.applescript` in `Script Libraries` was not loaded. An `.osax` bundle in `ScriptingAdditions`
  was not built, so only the resolved path, and not the load of an `.osax`, is shown to follow
  `CFFIXED_USER_HOME`. `strings -a /usr/bin/osascript` holds no upper-case variable name, and
  `fs_usage` and `dtruss` were not run (they need privileges), so another name may exist.
  `defaults read` did not follow `CFFIXED_USER_HOME`, so the name is not shown to apply to other
  programs and is not in the generic table.
  `a_script_library_under_a_planted_user_home_does_not_run_through_the_checked_start` runs this
  with the checked start and prints `skipped` where `osascript` or `osacompile` is absent.
- The unconfined `make`, `cargo`, `npm`, `go` and `python3` of the `doctor --sandbox-check` re-run
  are given the generic table and the suite's own list ([SANDBOX-21](#SANDBOX-21)). Each name was
  found by running the program with a helper planted under it on macOS 26 (arm64) with GNU Make
  3.81, cargo 1.99.0, npm 11.19.1, go 1.27.3 and python3 3.15.0, and the names added for #3811 with
  python3 3.15.2, Apple clang, and GNU Make 4.0, 4.2.1, 4.3, 4.4 and 4.4.1 built from the
  ftp.gnu.org tarballs with `./configure --without-guile`, with 3.81 as the control. The helper ran under
  `MAKEFLAGS` (`SHELL=` and `-I`), `MAKEFILES`, `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`, `RUSTC`,
  `RUSTDOC`, the `CARGO_BUILD_` forms of those, `CARGO_ENCODED_RUSTFLAGS`, `RUSTFLAGS`,
  `RUSTDOCFLAGS`, `CARGO_TARGET_<triple>_LINKER`, `_RUNNER`, `_RUSTFLAGS` and `_RUSTDOCFLAGS`,
  `CARGO_HOME`, `RUSTUP_TOOLCHAIN` and `RUSTUP_HOME`, every `npm_config_` name in any case
  (`script_shell`, `userconfig`, `globalconfig`, `prefix`, `init_module`), `NODE_OPTIONS`,
  `NODE_PATH`, `GOFLAGS`, `GOENV`, `GOCACHEPROG`, `GOTOOLCHAIN=path`, `CC`, `CXX`,
  `PKG_CONFIG`, `CGO_LDFLAGS` (`-fuse-ld=`), `CGO_CFLAGS` and `CGO_CPPFLAGS` (`-fno-integrated-as -B<dir>`
  with an `as` in the directory; the go build cache hides this unless `-a` is given), the make rule
  variables `AS`, `FC`, `PC`, `M2C`, `LEX`, `YACC`, `LINT`, `TEXI2DVI`, `MAKEINFO`, `GET`, `CO`, `AR`, `RM`
  and `MAKE`, make's `CFLAGS`, `CPPFLAGS`, `CXXFLAGS`, `LDFLAGS`, `LDLIBS` and `TARGET_ARCH` with
  `-B<dir>` and a linker in the directory, `CARGO_REGISTRY_CREDENTIAL_PROVIDER`,
  `CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS` and `CARGO_REGISTRIES_<name>_CREDENTIAL_PROVIDER` (for
  `cargo logout`), `CARGO_ENCODED_RUSTDOCFLAGS` and `CARGO_BUILD_RUSTDOCFLAGS` (`cargo test --doc` with
  `--test-runtool=`), `BROWSER` (`cargo doc --open`, in the suite's list as well as the browser opener's),
  `CARGO_DOC_BROWSER` (`cargo doc --open --offline`, cargo 1.99.0), `CARGO_CREDENTIAL_ALIAS_<name>`
  (cargo 1.99.0 `cargo --config registry.global-credential-providers=["<name>"] logout` ran the
  program the variable names and did not run it without the variable), `GOAUTH` (`go list -m
  example.com/x@latest` with `GOPROXY=https://127.0.0.1:9` ran the helper before the connection
  failed), `CGO_CXXFLAGS` (a package with a `.cc` file, `-fno-integrated-as -B<dir>/` and an `as` in
  the directory), `PYTHONPLATLIBDIR` (a relative value that climbs from `sys.prefix` to a copy of the
  standard library holding a `sitecustomize.py`), `OPENSSL_CONF` with `OPENSSL_MODULES` (python3
  `ssl.create_default_context()` loaded a provider library from the directory and its constructor
  wrote a marker; `OPENSSL_CONF` alone loaded nothing), `COMPILER_PATH` (a directory holding `ld`),
  and these make variables on 3.81 and on 4.0, 4.2.1, 4.3, 4.4 and 4.4.1 with the same result:
  `MAKE_COMMAND` (through a `+$(MAKE)` recipe line), `OUTPUT_OPTION` (only when it holds
  `-fno-integrated-as -B<dir>/ -o x.o`; a bare program name did not run), `ASFLAGS` and `CFLAGS` with
  that form, `COMPILE.c`, `COMPILE.S`, `LINK.c`, `LINK.o`, `LEX.l`, `YACC.y`, `LINT.c` and
  `PREPROCESS.r`. `OBJC` and `OBJCFLAGS` ran on 4.0 to 4.4.1 and not on 3.81, which has no
  Objective-C rule. `TEX`, `TANGLE`, `CTANGLE`, `WEAVE` and `CWEAVE` ran on 4.4.1 and on 3.81. The
  other suffixes of the dotted names are listed by analogy with those run.
  `PYTHONPATH` and `PYTHONUSERBASE` (a `sitecustomize`
  under the first and a `usercustomize` under the second), `PYTHONHOME` (a `sitecustomize` in the
  standard library it names), `PYTHONPYCACHEPREFIX` (an unchecked-hash `.pyc`), `PYTHONEXECUTABLE`
  (`-m venv`) and `PYTHONSTARTUP` (`python3 -i` only), and `GOTOOLCHAIN=go1.99.0+path` ran a
  `go1.99.0` found on `PATH`. It did not run under
  `GNUMAKEFLAGS` or `MFLAGS` with `-f`, `-I` or `SHELL=` on 3.81, and `MAKEFLAGS` with `-f` in any
  spelling (`-f <file>`, `-f<file>`, `--file=`, `--makefile=`, `f <file>`, `-s -f <file>`), `-C`,
  `--directory=` or `-o`, none of which read the file or change directory when they come from
  `MAKEFLAGS` on 3.81 (the same file passed on the command line does run, so the control holds).
  On 4.0, 4.2.1, 4.3 and 4.4 the `-f` cases from `MAKEFLAGS` and `GNUMAKEFLAGS` also ran nothing; on
  4.4.1 alone they ran the file, `--file=` and `f <file>` did too, and `-C /nonexistent` was
  honoured. The 4.4 tag's `decode_switches` in `src/main.c` has the earlier condition, and 4.4.1's
  adds `cs->env` to it, which reads from source as the cause. `--eval`, `-E`, `-- SHELL=<helper>`,
  `SHELL=<helper>` as a variable, `MAKEFILES` and `CC` ran the helper on every version run.
  `SHELL` in the environment, `MAKESHELL`, `CONFIG_SHELL`, `.SHELLFLAGS`, `MAKE_TERMOUT`,
  `MAKE_TERMERR`, `-L`, `-l`, `-W` and the `getenv("SHELL")` fallback of an unexecutable recipe ran
  nothing on 3.81 and on 4.0 to 4.4.1 (`make` does not export `SHELL`). GNU Make 4.1 was not run,
  and no Guile-enabled build was run (`GUILE_LOAD_PATH` is read by libguile and a `$(guile)` call
  needs makefile text, so it is reachable only through routes already withheld).
  `make` implicit-rule variables `ARFLAGS`, `LD`, `F77`, `SHELL` and `CPP` (nothing from a
  built-in rule; a recipe that uses `$(CPP)` runs it), the `cc` crate's `TARGET_CC`, `TARGET_CXX`, `TARGET_AR`, `RANLIB`, `PKG_CONFIG_PATH`,
  `PKG_CONFIG_LIBDIR`, `CARGO_PROFILE_*`, `CARGO_ALIAS_*` (an alias to a program on the `PATH` runs only
  through a `cargo-<name>` file found on `PATH`, which `PATH` handling covers),
  `CARGO_NET_GIT_FETCH_WITH_CLI`, `CARGO_HTTP_*`, `CARGO_BUILD_BUILD_DIR`, `CARGO_BUILD_TARGET`,
  `CARGO_TARGET_DIR`, `CARGO_INSTALL_ROOT`, `CARGO`, `RUSTFMT`,
  `CARGO_REGISTRIES_CRATES_IO_CREDENTIAL_PROVIDER` for `cargo login`, `GOPATH` or `GOMODCACHE` holding a
  toolchain (`go` tried to download it), `GOBIN`, `GOWORK`, `GOPROXY`, `PYTHONSTARTUP` (for `-c`, `-m unittest` and `-m venv`),
  `__PYVENV_LAUNCHER__`, `PYTHONBREAKPOINT`, `npm_config_node_options`,
  `npm_config_cache`, `npm_config_git` or `npm_config_prefix` (set to a directory holding the helper as
  `bin/npm` and `bin/node`, for `npm run`, `root -g`, `bin -g`, `prefix -g`, `config get prefix`, `ls`,
  `pack --dry-run`, `install --offline` and `-v`; it is removed anyway as an `npm_config_` name).
  `GNUMAKEFLAGS` and `MFLAGS` are in the suite's list because other versions of `make` read them.
  `FFLAGS` and `CGO_FFLAGS` are listed from the documentation alone: no Fortran compiler was
  installed on the host, so neither was run. `GCC_EXEC_PREFIX` is listed from the gcc manual
  (`invoke.texi`) and ran nothing under Apple clang. `OPENSSL_ENGINES` is listed from the OpenSSL 3.0
  `openssl-env` manual and was not run; `OPENSSL_MODULES` and `OPENSSL_CONF` are the constants
  `crates/bedrock/src/credentials.rs` already shared. `PREPROCESS.S` was not run because the host's
  file system ignores case, so `x.s` and `x.S` are one file; `PREPROCESS.r` was run in its place.
  `GOENV` and `CARGO_HOME` are removed from every start by the suite's `HOST_VARIABLES`, and
  `GOTOOLCHAIN` is set to `local`, so none of the three is handed a stage as the person set it.
  No Linux or Windows run was made. The names a Linux build of `make`, `cargo`, `npm`, `go` and
  `python3` reads, and their versions, are not recorded. The added names were read from the gcc
  13.2.0 manual, openssl 3.0 `openssl-env`, the Node v24 command-line documentation, CPython
  `Modules/main.c`, the go1.27.3 `cmd/go` source and the cargo 1.90.0 documentation, and none is
  documented as read on Linux only. Windows was not run: the suite runs nothing there. A Linux run
  of them is tracked by #3811 and #3863.
  Each test runs the program with the variable as the person set it first. Where the program does
  not run the helper, the test prints that it skipped and why, and asserts nothing further: a helper
  the program never reaches proves nothing. Where the control passes, the strict check runs on every
  host. A test also skips, and says so, in these cases: the program is not installed (a `go` that a
  stage starts through `sh` included); the `CARGO_TARGET_<triple>_` cases when the host is not
  macOS or Linux on aarch64 or x86_64, the hosts whose triple the tests know; and the toolchain and
  `RUSTUP_HOME` cases on a host whose triple is not known or where `rustup` is not installed.
  The `PYTHONUSERBASE` test takes
  the directory of `usercustomize.py` from `site.getusersitepackages()`, since the directory under a
  base is `lib/python/site-packages` for a macOS framework build and
  `lib/python3.<minor>/site-packages` for other builds. A skipped name stays in the lists and is
  removed on every host. The tests of `OBJC` and `OBJCFLAGS` print `skipped` where the `make` on
  `PATH` is 3.81, which has no Objective-C rule. No test covers `FFLAGS`, `CGO_FFLAGS`,
  `GCC_EXEC_PREFIX`, `PYTHONPLATLIBDIR` through a start or `OPENSSL_ENGINES`: unit tests pin their
  kind and value rules. The results for the `cc` and `pkg-config`
  crates, for GNU Make 3.81, and for the names that ran nothing (`GOPATH`, `GOBIN`, `GOWORK`,
  `GOPROXY`, `CARGO_PROFILE_*`, `CARGO_ALIAS_*`, `RANLIB`,
  `PYTHONBREAKPOINT`, `npm_config_cache` and the others above) were observed by hand, and no test
  repeats them.
- Names and effects of the make, cargo, npm, go and python3 starts that are not settled, each with the
  run that would settle it:
  - Whether a `-fplugin=<library>` value in `FFLAGS`, `ASFLAGS`, `OBJCFLAGS`, `CGO_CXXFLAGS` or
    `CGO_FFLAGS` loads the library. The five are removed whatever they hold, so the question is
    whether the removal was needed. A Linux host with gcc and a `.f` rule or a cgo package with a
    `.cc` file would settle it.
  - `GCC_EXEC_PREFIX` with a prefix holding a fake `cc1`, on a Linux gcc.
  - `PYTHONINSPECT=1` with `PYTHONSTARTUP`. With python3 3.15.2, stdin from `/dev/null` and no
    terminal the startup file did not run. That is weak evidence, since the interpreter had no
    terminal. It is not listed.
  - How far node trusts a cache entry a stage wrote under `NODE_COMPILE_CACHE`, and how far go
    trusts one under `GOCACHE` or `GOMODCACHE`. None is listed. That is a question about the
    integrity checks of those caches, which a probe of one host does not settle.
  - Other `npm` names besides `npm_config_`: the public page of npm's configuration could not be
    retrieved, so the list for npm is `npm_config_` and `NODE_OPTIONS` and `NODE_PATH` only.
  - On Windows, `CL`, `_CL_` (which the MSVC compiler prepends and appends to its command line, and
    which can hold `/analyze:plugin <dll>`), `LINK` and `_LINK_`, and the `VCINSTALLDIR`, `VSCMD_ARG_TGT_ARCH` and
    `VSCMD_ARG_VCVARS_SPECTRE` that the `cc` crate reads to locate a toolset, are not listed and
    were not run. make's use of `COMSPEC` and `MAKESHELL` on Windows is unread. The `ComSpec` of
    Python's `subprocess` is used only with `shell=True`.
  - A GNU Make built with Guile, and GNU Make 4.1.

`verified-by: bravebot_sandbox::programs::a_program_in_a_directory_a_stage_may_write_is_not_found`
`verified-by: bravebot_sandbox::programs::the_program_behind_a_writable_directory_is_the_one_found`
`verified-by: bravebot_sandbox::programs::a_program_is_found_when_no_writable_directory_is_involved`
`verified-by: bravebot_sandbox::programs::a_relative_path_entry_is_never_searched`
`verified-by: bravebot_sandbox::programs::an_empty_path_entry_is_never_searched`
`verified-by: bravebot_sandbox::programs::the_system_temp_directory_counts_as_writable`
`verified-by: bravebot_sandbox::programs::a_link_in_a_safe_directory_to_a_writable_file_is_not_found`
`verified-by: bravebot_sandbox::programs::a_link_inside_a_writable_directory_to_a_safe_program_is_not_found`
`verified-by: bravebot_sandbox::programs::a_path_entry_that_links_into_a_writable_directory_is_not_searched`
`verified-by: bravebot_sandbox::programs::a_sibling_directory_sharing_a_name_prefix_is_not_writable`
`verified-by: bravebot_sandbox::programs::a_program_in_a_row_the_account_can_write_is_not_found`
`verified-by: bravebot_sandbox::programs::a_program_in_a_row_the_account_neither_owns_nor_can_write_is_still_found`
`verified-by: bravebot_sandbox::programs::a_program_the_account_made_read_only_in_a_held_row_is_not_found`
`verified-by: bravebot_sandbox::programs::an_entry_reached_through_a_directory_the_account_owns_is_not_searched`
`verified-by: bravebot_agent::confine::a_program_a_bypass_stage_made_read_only_is_not_found_and_so_never_started`
`verified-by: bravebot_sandbox::programs::a_registered_directory_is_kept_out_of_the_search`
`verified-by: bravebot_sandbox::programs::a_search_of_a_given_path_leaves_out_a_row_held_where_the_account_can_write`
`verified-by: bravebot_sandbox::programs::a_search_of_a_given_path_leaves_out_a_read_only_program_the_account_owns_in_a_held_row`
`verified-by: bravebot_sandbox::programs::an_absolute_path_is_the_callers_own_choice`
`verified-by: bravebot_sandbox::programs::start_removes_the_names_it_is_given_and_no_others`
`verified-by: bravebot_sandbox::programs::start_hands_a_program_only_the_names_it_is_given`
`verified-by: bravebot_sandbox::programs::a_start_given_only_some_names_still_reduces_its_path`
`verified-by: bravebot_sandbox::programs::a_start_given_only_some_names_reports_the_removal_of_the_rest_through_get_envs`
`verified-by: bravebot_sandbox::programs::withhold_takes_a_name_set_after_start_off_again`
`verified-by: bravebot_sandbox::programs::start_gives_a_reduced_path_only_where_it_is_asked_for`
`verified-by: bravebot_sandbox::programs::a_started_programs_path_holds_exactly_the_entries_the_lookup_searches`
`verified-by: bravebot_sandbox::programs::a_started_program_with_no_searched_entry_is_given_no_path_at_all`
`verified-by: bravebot_sandbox::programs::a_started_programs_path_carries_an_entry_that_is_not_utf8_as_its_bytes`
`verified-by: bravebot_sandbox::programs::a_path_entry_the_volume_opens_as_a_directory_a_stage_may_write_is_not_searched`
`verified-by: bravebot_sandbox::programs::a_started_programs_path_leaves_out_an_entry_the_volume_opens_as_a_writable_directory`
`verified-by: bravebot_sandbox::programs::a_program_the_volume_opens_in_a_row_held_where_the_account_can_write_is_not_found`
`verified-by: bravebot_sandbox::programs::a_started_programs_path_leaves_out_a_link_into_another_spelling_of_a_writable_directory`
`verified-by: bravebot_sandbox::programs::a_link_in_a_safe_directory_to_a_program_under_another_spelling_is_not_found`
`verified-by: bravebot_sandbox::programs::a_path_is_within_a_directory_a_volume_opens_under_another_name`
`verified-by: bravebot_sandbox::programs::a_started_programs_path_leaves_out_an_entry_not_yet_made_below_a_link_into_a_writable_directory`
`verified-by: bravebot_sandbox::programs::a_directory_that_is_not_text_is_judged_by_what_it_is_and_not_by_its_lossy_lookalike`
`verified-by: bravebot_sandbox::programs::a_recorded_directory_that_does_not_exist_is_still_compared_by_name`
`verified-by: bravebot_agent::workspace::each_alias_in_the_table_that_spells_a_directory_is_left_out_of_a_program_lookup_unless_it_is_a_listed_gap`
`verified-by: bravebot_bedrock::credentials::an_aws_a_confined_stage_wrote_at_the_front_of_path_is_not_the_one_run`
`verified-by: bravebot_bedrock::credentials::an_aws_where_a_stage_given_the_whole_machine_may_write_is_not_the_one_run`
`verified-by: bravebot_bedrock::credentials::an_aws_that_starts_another_program_by_name_never_runs_one_a_stage_may_have_written`
`verified-by: bravebot_bedrock::credentials::an_aws_config_file_a_stage_may_write_is_not_the_one_the_cli_is_told_to_read`
`verified-by: bravebot_bedrock::credentials::an_aws_credentials_file_a_stage_may_write_is_not_the_one_the_cli_is_told_to_read`
`verified-by: bravebot_bedrock::credentials::an_openssl_configuration_a_stage_may_write_is_not_the_one_the_cli_is_told_to_read`
`verified-by: bravebot_bedrock::credentials::an_openssl_directory_a_stage_may_write_is_not_the_one_the_cli_is_told_to_load_from`
`verified-by: bravebot_bedrock::credentials::a_browser_a_stage_may_write_is_not_the_one_the_sign_in_opens`
`verified-by: bravebot_bedrock::credentials::an_aws_value_that_selects_no_code_is_left_as_set`
`verified-by: bravebot_bedrock::credentials::the_cli_is_handed_the_files_outside_a_stage_and_none_inside_one`
`verified-by: bravebot_bedrock::credentials::the_installed_cli_runs_a_credential_process_outside_a_stage_and_not_one_inside`
`verified-by: bravebot_sandbox::programs::a_file_that_names_code_must_exist_as_a_file_outside_every_directory_a_stage_may_write`
`verified-by: bravebot_sandbox::programs::a_code_selector_is_taken_off_unless_its_value_passes`
`verified-by: bravebot_sandbox::programs::a_list_of_directories_passes_only_when_every_entry_does_and_options_never_do`
`verified-by: bravebot_agent::gcloud::a_python_path_with_an_entry_a_stage_may_write_is_not_the_one_gcloud_imports_from`
`verified-by: bravebot_agent::gcloud::a_python_path_outside_every_directory_a_stage_may_write_still_reaches_gcloud`
`verified-by: bravebot_agent::gcloud::a_python_directory_a_stage_may_write_is_not_the_one_gcloud_is_told_to_load_from`
`verified-by: bravebot_agent::gcloud::a_python_directory_outside_every_directory_a_stage_may_write_still_reaches_gcloud`
`verified-by: bravebot_agent::gcloud::a_credential_or_certificate_file_a_stage_may_write_is_not_the_one_gcloud_is_told_to_read`
`verified-by: bravebot_agent::gcloud::a_credential_or_certificate_file_outside_every_directory_a_stage_may_write_still_reaches_gcloud`
`verified-by: bravebot_agent::gcloud::an_openssl_configuration_a_stage_may_write_is_not_the_one_gcloud_is_told_to_read`
`verified-by: bravebot_agent::gcloud::an_openssl_configuration_outside_every_directory_a_stage_may_write_still_reaches_gcloud`
`verified-by: bravebot_cli::terminal_size::a_failed_size_query_starts_no_tput`
`verified-by: bravebot_cli::gix_path_starts::nothing_in_this_repository_names_gix_path`
`verified-by: bravebot_cli::gix_path_starts::no_crate_that_depends_on_gix_path_names_its_env_module`
`verified-by: bravebot_cli::gix_path_starts::gix_path_starts_a_program_only_in_its_env_module`
`verified-by: bravebot_cli::gix_path_starts::the_scan_sees_each_way_to_reach_the_env_module_and_not_the_paths_that_start_nothing`
`verified-by: bravebot_cli::gix_path_starts::the_scan_of_gix_path_sees_a_command_under_any_name_and_not_a_comment`
`verified-by: bravebot_cli::gix_path_starts::a_dependency_on_gix_path_under_another_name_is_found_and_reported`
`verified-by: bravebot_cli::gix_path_starts::the_functions_of_gix_path_that_start_a_program_are_the_audited_ones`
`verified-by: bravebot_cli::gix_path_starts::the_audit_of_gix_path_passes_on_the_audited_shape`
`verified-by: bravebot_cli::gix_path_starts::the_audit_of_gix_path_fails_when_a_new_function_starts_a_program`
`verified-by: bravebot_cli::gix_path_starts::the_audit_of_gix_path_fails_when_an_audited_non_starting_function_starts_a_program`
`verified-by: bravebot_agent::gcloud::a_browser_a_stage_may_write_is_not_the_one_the_sign_in_opens`
`verified-by: bravebot_agent::gcloud::a_browser_outside_every_directory_a_stage_may_write_still_reaches_gcloud`
`verified-by: bravebot_agent::gcloud::python_options_are_not_passed_to_gcloud_whatever_they_hold`
`verified-by: bravebot_agent::hooks::a_hook_named_bare_never_starts_a_file_found_only_where_a_stage_may_write`
`verified-by: bravebot_agent::hooks::a_hook_named_by_a_path_is_not_looked_up`
`verified-by: bravebot_agent::hooks::a_hook_that_starts_another_program_by_name_never_runs_one_a_stage_may_have_written`
`verified-by: bravebot_cli::push_identity::a_program_ssh_starts_by_name_is_never_one_a_stage_may_have_written`
`verified-by: bravebot_cli::push_identity::the_ssh_keygen_ssh_and_git_starts_all_go_through_started`
`verified-by: bravebot_cli::push_identity::an_askpass_program_is_never_handed_to_the_start`
`verified-by: bravebot_cli::push_identity::the_switch_that_forces_the_askpass_program_is_never_handed_to_the_start`
`verified-by: bravebot_cli::push_identity::a_security_key_provider_is_never_handed_to_the_start`
`verified-by: bravebot_cli::push_identity::a_security_key_helper_is_never_handed_to_the_start`
`verified-by: bravebot_cli::push_identity::the_start_is_given_bin_sh_for_shell_whatever_the_person_has`
`verified-by: bravebot_cli::push_identity::a_developer_directory_a_stage_may_write_is_not_handed_to_the_start`
`verified-by: bravebot_cli::push_identity::a_developer_directory_outside_every_directory_a_stage_may_write_still_reaches_the_start`
`verified-by: bravebot_connector::lib::an_authority_that_starts_another_program_by_name_never_runs_one_a_stage_may_have_written`
`verified-by: bravebot_agent::lsp::a_server_found_only_where_a_stage_may_write_is_not_installed`
`verified-by: bravebot_tui::caffeinate::the_inhibitor_runs_the_installed_cat_and_never_one_a_stage_may_have_written`
`verified-by: bravebot_agent::servers::a_directory_a_started_server_may_write_is_kept_out_of_the_search_for_programs`
`verified-by: bravebot_agent::servers::describing_a_server_keeps_nothing_out_of_the_search_for_programs`
`verified-by: bravebot_agent::servers::a_server_directory_that_is_not_text_is_kept_out_as_it_is_and_not_as_its_lookalike`
`verified-by: bravebot_tui::caffeinate::an_inhibitor_with_no_cat_but_a_planted_one_is_not_started`
`verified-by: bravebot_tui::caffeinate::a_program_the_inhibitor_starts_by_name_is_never_one_a_stage_may_have_written`
`verified-by: bravebot_tui::editor::an_editor_found_only_in_a_directory_a_stage_may_write_is_not_started`
`verified-by: bravebot_tui::editor::an_editor_named_only_by_a_planted_file_is_not_found`
`verified-by: bravebot_tui::editor::an_editor_that_runs_another_program_by_name_never_runs_one_a_stage_may_have_written`
`verified-by: bravebot_tui::editor::an_editor_configured_as_env_and_a_name_never_runs_one_a_stage_may_have_written`
`verified-by: bravebot_tui::editor::an_editor_with_no_searched_path_entry_left_does_not_search_the_current_directory`
`verified-by: bravebot_bedrock::credentials::an_aws_only_in_a_directory_a_stage_may_write_is_reported_as_not_installed`
`verified-by: bravebot_agent::workspace::every_directory_a_stage_may_write_is_kept_out_of_the_search_for_programs`
`verified-by: bravebot_tui::opener::the_opener_is_not_handed_this_agents_own_credentials`
`verified-by: bravebot_tui::opener::a_browser_opener_in_a_directory_a_stage_may_write_is_not_the_one_started`
`verified-by: bravebot_tui::opener::a_program_the_opener_starts_by_name_is_never_one_a_stage_may_have_written`
`verified-by: bravebot_sandbox::programs::a_checked_start_removes_each_generic_name_that_names_a_file_a_stage_may_write`
`verified-by: bravebot_sandbox::programs::a_checked_start_keeps_each_generic_name_that_names_a_file_outside_every_writable_directory`
`verified-by: bravebot_sandbox::programs::a_kept_start_leaves_every_generic_name_set`
`verified-by: bravebot_sandbox::programs::a_checked_start_removes_each_generic_name_whose_value_is_relative_empty_or_missing`
`verified-by: bravebot_sandbox::programs::a_value_that_links_into_a_directory_a_stage_may_write_is_removed`
`verified-by: bravebot_sandbox::programs::a_relative_or_missing_value_is_removed_and_a_bare_program_name_is_kept`
`verified-by: bravebot_sandbox::programs::an_absolute_path_that_exists_outside_every_writable_directory_is_kept`
`verified-by: bravebot_sandbox::programs::a_value_that_is_not_text_is_judged_as_the_bytes_it_is`
`verified-by: bravebot_sandbox::programs::a_list_is_removed_when_any_entry_fails_or_is_empty_and_kept_when_every_entry_is_safe`
`verified-by: bravebot_sandbox::programs::an_option_variable_is_removed_whatever_it_holds`
`verified-by: bravebot_sandbox::programs::a_variable_that_selects_no_code_is_left_as_set`
`verified-by: bravebot_sandbox::programs::a_callers_own_names_are_judged_on_top_of_the_generic_ones`
`verified-by: bravebot_sandbox::programs::a_checked_start_judges_a_value_against_the_directories_recorded_for_the_lookup`
`verified-by: bravebot_sandbox::programs::the_table_holds_each_name_sandbox_30_lists`
`verified-by: bravebot_sandbox::programs::the_table_holds_each_name_that_makes_a_bash_script_or_a_shim_run_code`
`verified-by: bravebot_sandbox::programs::an_exported_function_and_an_asdf_tool_version_are_removed_by_the_shape_of_the_name`
`verified-by: bravebot_sandbox::programs::an_exported_function_or_tool_version_whose_name_is_not_text_is_removed`
`verified-by: bravebot_sandbox::programs::a_bash_script_runs_the_code_in_shellopts_ps4_and_an_exported_function`
`verified-by: bravebot_sandbox::programs::home_is_removed_for_an_asdf_or_pyenv_shim_when_a_stage_may_write_it`
`verified-by: bravebot_sandbox::programs::home_is_removed_for_a_shim_when_what_it_reads_under_home_leads_into_a_stage_directory`
`verified-by: bravebot_sandbox::programs::home_is_kept_for_a_program_that_is_not_an_asdf_or_pyenv_shim`
`verified-by: bravebot_bedrock::credentials::a_pager_the_cli_runs_for_its_help_is_kept_only_when_it_is_an_installed_program`
`verified-by: bravebot_bedrock::credentials::a_home_a_stage_may_write_is_not_the_one_the_cli_reads_its_configuration_from`
`verified-by: bravebot_bedrock::credentials::a_python_warning_filter_or_botocore_plugin_is_removed_whatever_it_holds`
`verified-by: bravebot_tui::opener::the_opener_removes_a_glib_or_qt_name_that_selects_code_a_stage_may_write`
`verified-by: bravebot_tui::opener::the_opener_judges_a_kde_service_cache_by_the_file_it_names`
`verified-by: bravebot_tui::powershell::osascript_has_a_user_home_it_would_load_script_libraries_from_removed_when_a_stage_may_write_it`
`verified-by: bravebot_tui::powershell::a_script_library_under_a_planted_user_home_does_not_run_through_the_checked_start`
`verified-by: bravebot_sandbox::programs::a_checked_start_keeps_what_the_inherits_rule_withheld`
`verified-by: bravebot_sandbox::programs::the_loader_prefixes_are_removed_by_name_and_not_by_a_checked_start`
`verified-by: bravebot_sandbox::linux_loader::a_loader_variable_naming_a_library_a_stage_may_write_is_removed_and_a_safe_one_still_loads`
`verified-by: bravebot_sandbox::linux_loader::locpath_and_nlspath_reach_a_process_only_when_they_name_a_directory_a_stage_cannot_write`
`verified-by: bravebot_sandbox::programs::may_be_run_holds_for_nothing_in_an_option_variable_and_nothing_in_an_empty_list`
`verified-by: bravebot_sandbox::programs::a_list_of_commands_holds_only_when_every_entry_is_one_program_alone_or_with_a_placeholder`
`verified-by: bravebot_tui::opener::the_opener_removes_a_browser_or_xdg_directory_that_names_a_file_a_stage_may_write`
`verified-by: bravebot_tui::opener::a_browser_value_is_kept_only_when_every_command_in_it_is_one_program_alone_or_with_a_placeholder`
`verified-by: bravebot_tui::opener::the_opener_keeps_an_unrelated_variable_and_a_safe_value_of_its_own_names`
`verified-by: bravebot_tui::opener::an_xdg_directory_is_judged_by_where_it_leads_on_the_disk`
`verified-by: bravebot_tui::opener::a_browser_entry_that_is_not_text_is_removed_and_not_judged_by_its_lossy_rendering`
`verified-by: bravebot_tui::opener::a_browser_helper_a_stage_may_write_is_not_run_by_an_opener_that_runs_browser`
`verified-by: bravebot_tui::opener::a_browser_helper_a_stage_may_write_is_not_run_by_the_installed_xdg_open`
`verified-by: bravebot_tui::opener::an_xdg_directory_a_stage_may_write_is_not_read_by_the_installed_xdg_open_under_gnome`
`verified-by: bravebot_tui::opener::an_xdg_directory_a_stage_may_write_is_not_read_by_the_installed_xdg_open_in_generic_mode`
`verified-by: bravebot_tui::opener::a_home_a_stage_may_write_is_not_read_by_the_installed_xdg_open_under_gnome`
`verified-by: bravebot_tui::opener::a_home_is_judged_with_the_paths_the_opener_reads_under_it_on_the_disk`
`verified-by: bravebot_tui::opener::gio_launch_desktop_is_removed_for_a_wrapper_a_stage_may_write_and_kept_for_an_installed_one`
`verified-by: bravebot_tui::opener::xdg_data_dirs_a_stage_may_write_is_not_passed_by_the_opener_in_generic_mode`
`verified-by: bravebot_tui::opener::a_name_a_stage_may_write_is_not_passed_to_the_kde_open_the_installed_xdg_open_starts`
`verified-by: bravebot_tui::opener::the_windows_launch_is_rundll32_with_the_entry_point_and_the_address_as_two_arguments`
`verified-by: by-construction (the desktop main process is not a crate this workspace compiles, so ui/scripts/external-open-electron.test.mjs starts Electron under xvfb-run with a logging xdg-open first on PATH and asserts that openExternal and openPath of external-open.ts start it with none of the fifteen names set and the same calls on Electron's shell start it with all fifteen; it runs only on Linux with Electron and xvfb-run installed and prints skipped elsewhere; make check-ui-build runs it; make check-code-selectors holds its list and OPENER_SELECTORS to the opener's apart from HOME, XDG_RUNTIME_DIR and DBUS_SESSION_BUS_ADDRESS, which the application keeps)`
`verified-by: bravebot_sandbox::programs::a_bus_address_holds_only_when_every_entry_is_a_socket_outside_the_stage`
`verified-by: bravebot_tui::opener::the_opener_removes_the_runtime_directory_and_bus_address_that_lead_to_a_socket_a_stage_may_write`
`verified-by: bravebot_tui::opener::a_bus_socket_is_judged_by_where_it_leads_on_the_disk`
`verified-by: bravebot_tui::opener::a_bus_address_that_is_not_text_is_removed_and_not_judged_by_its_lossy_rendering`
`verified-by: bravebot_sandbox::programs::a_bus_address_that_is_not_text_is_not_judged_by_its_lossy_rendering`
`verified-by: by-construction (the desktop main process is not a crate this workspace compiles, so ui/scripts/external-open.test.mjs asserts that HOME, XDG_RUNTIME_DIR and DBUS_SESSION_BUS_ADDRESS are not in OPENER_SELECTORS and are still set after the deletion; make check-ui runs it)`
`verified-by: bravebot_tui::opener::a_browser_or_xdg_name_a_stage_may_write_is_not_read_by_the_program_xdg_open_hands_the_address_to_under_kde`
`verified-by: bravebot_tui::opener::kde_open_run_with_each_name_set_to_a_marker_runs_the_marker_only_for_names_the_opener_removes`
`verified-by: bravebot_tui::powershell::a_module_path_with_a_directory_a_stage_may_write_is_removed_whole_and_a_safe_one_is_kept`
`verified-by: bravebot_tui::powershell::a_profiler_library_a_stage_may_write_is_removed_and_a_safe_file_is_kept`
`verified-by: bravebot_tui::powershell::a_profiler_switch_or_class_id_is_removed_whatever_it_holds`
`verified-by: bravebot_tui::powershell::a_variable_that_selects_no_code_is_left_as_set_for_powershell`
`verified-by: bravebot_tui::powershell::a_program_other_than_powershell_is_not_given_the_powershell_names`
`verified-by: bravebot_tui::powershell::powershell_is_known_by_its_name_whatever_extension_or_case_the_lookup_found`
`verified-by: bravebot_tui::powershell::the_clipboard_tool_and_the_inhibitor_start_powershell_through_the_checked_start`
`verified-by: bravebot_bedrock::credentials::the_aws_start_removes_a_browser_or_xdg_directory_that_names_a_file_a_stage_may_write`
`verified-by: bravebot_bedrock::credentials::the_aws_start_keeps_the_variables_a_sign_in_is_run_to_read`
`verified-by: bravebot_bedrock::credentials::an_aws_browser_entry_that_is_not_text_is_removed`
`verified-by: bravebot_agent::gcloud::a_browser_or_xdg_directory_a_stage_may_write_is_not_the_one_the_gcloud_login_is_told_to_use`
`verified-by: bravebot_agent::gcloud::a_gcloud_browser_entry_that_is_not_text_is_removed`
`verified-by: bravebot_agent::gcloud::the_gcloud_start_keeps_the_display_and_runtime_directory_a_login_reads`
`verified-by: bravebot_cli::push_identity::a_browser_or_xdg_directory_a_stage_may_write_is_not_handed_to_the_programs_started_here`
`verified-by: bravebot_cli::push_identity::the_variables_the_programs_started_here_are_run_to_read_are_kept`
`verified-by: bravebot_cli::push_identity::a_browser_entry_that_is_not_text_is_removed_from_the_programs_started_here`
`verified-by: bravebot_cli::push_identity::a_list_of_xdg_directories_outside_every_directory_a_stage_may_write_is_kept_for_the_programs_started_here`
`verified-by: bravebot_bedrock::credentials::an_aws_xdg_list_of_directories_outside_every_directory_a_stage_may_write_is_kept`
`verified-by: by-construction (the desktop main process is not a crate this workspace compiles, so the deletion of the fifteen names before each call of electron shell is pinned instead by ui/scripts/external-open.test.mjs, which asserts that the fifteen names are the ones the opener scripts read, that each of openExternal, openPath and showItemInFolder reaches electron with them already deleted, that PATH and EDITOR are left alone, and that no other file in ui/src/main calls the opening functions of shell; make check-ui runs it)`
`verified-by: by-construction (the same deletion is pinned against Electron itself on Linux by ui/scripts/external-open-electron.test.mjs, which asserts that openExternal, openPath and showItemInFolder each start xdg-open without the fifteen names where Electron's own shell starts it with all fifteen; its showItemInFolder case is a test case of its own and has never been run on Linux; the ui job of CI runs it through make check-ui-build on ubuntu-latest and it prints skipped elsewhere)`
`verified-by: bravebot_agent::workspace::each_alias_in_the_table_that_spells_a_directory_is_removed_from_a_browser_or_xdg_value_unless_it_is_a_listed_gap`
`verified-by: bravebot_agent::confine::a_directory_a_policy_grants_writes_in_is_kept_out_of_the_search_for_programs`
`verified-by: bravebot_agent::confine::describing_a_line_keeps_nothing_out_of_the_search_for_programs`
`verified-by: bravebot_tui::clipboard::a_copy_tool_found_only_in_a_directory_a_stage_may_write_is_not_started`
`verified-by: bravebot_tui::clipboard::a_copy_tool_installed_outside_every_writable_directory_is_started`
`verified-by: bravebot_tui::clipboard::a_paste_tool_found_only_in_a_directory_a_stage_may_write_is_not_started`
`verified-by: bravebot_tui::clipboard::a_paste_tool_installed_outside_every_writable_directory_is_read`
`verified-by: bravebot_tui::clipboard::a_copy_tool_the_account_made_read_only_in_a_held_row_is_not_started`
`verified-by: bravebot_tui::clipboard::a_paste_tool_the_account_made_read_only_in_a_held_row_is_not_started`
`verified-by: bravebot_tui::clipboard::the_clipboard_entry_points_start_no_tool_found_only_in_a_directory_a_stage_may_write`
`verified-by: bravebot_tui::clipboard::a_copy_tool_that_starts_another_program_by_name_never_runs_one_a_stage_may_have_written`
`verified-by: bravebot_tui::clipboard::a_paste_tool_that_starts_another_program_by_name_never_runs_one_a_stage_may_have_written`
`verified-by: bravebot_tui::clipboard::the_reads_of_the_clipboard_start_no_tool_found_only_in_a_directory_a_stage_may_write`
`verified-by: bravebot_sandbox::checkout_in_search::the_checkout_holding_the_directory_is_left_out_with_it`
`verified-by: bravebot_sandbox::checkout_in_search::a_directory_under_no_checkout_is_left_out_by_itself`
`verified-by: bravebot_sandbox::checkout_in_search::a_directory_that_holds_its_own_git_leaves_out_only_itself`
`verified-by: bravebot_sandbox::checkout_in_search::the_nearest_checkout_is_the_one_left_out_with_the_directory`
`verified-by: bravebot_sandbox::checkout_in_search::the_checkout_root_is_listed_once`
`verified-by: bravebot_sandbox::checkout_in_search::a_program_in_the_bin_of_the_checkout_above_the_directory_is_not_found`
`verified-by: bravebot_sandbox::checkout_in_search::a_program_in_a_bin_outside_the_checkout_is_still_found`
`verified-by: bravebot_sandbox::checkout_in_search::the_checkout_root_and_a_directory_below_it_leave_out_the_same_bin`
`verified-by: bravebot_sandbox::checkout_in_search::keeping_a_directory_out_keeps_the_checkout_holding_it_out`
`verified-by: bravebot_agent::workspace::the_checkout_holding_a_directory_a_session_opens_is_kept_out_of_the_search_for_programs`
`verified-by: bravebot_agent::servers::an_approved_name_does_not_start_a_program_in_the_bin_of_the_checkout_holding_the_session`
`verified-by: bravebot_cli::running::doctor_push_identity_starts_no_program_from_the_checkout_it_runs_in`
`verified-by: bravebot_cli::running::doctor_push_identity_reports_a_program_found_only_in_the_checkout_as_absent`
`verified-by: bravebot_cli::running::doctor_sandbox_check_starts_no_program_from_the_checkout_it_runs_in`
`verified-by: bravebot_cli::running::schedule_remove_starts_no_crontab_from_the_checkout_it_runs_in`
`verified-by: bravebot_cli::running::auth_login_bedrock_starts_no_aws_from_the_checkout_it_runs_in`
`verified-by: bravebot_cli::running::auth_status_bedrock_starts_no_aws_from_the_checkout_it_runs_in`
`verified-by: bravebot_cli::chatgpt_account::auth_login_chatgpt_opens_the_address_with_no_program_from_the_checkout_it_runs_in`
`verified-by: bravebot_cli::running::schedule_add_and_list_start_no_crontab_from_the_checkout_it_runs_in`
`verified-by: bravebot_cli::running::a_command_run_in_the_checkouts_root_starts_no_program_from_its_bin`
`verified-by: bravebot_cli::running::the_auth_way_list_asks_no_aws_from_the_checkout_it_runs_in`
`verified-by: bravebot_sandbox::checkout_in_search::a_command_that_recorded_its_directory_does_not_find_the_checkouts_bin`
`verified-by: bravebot_agent::usability_path::a_program_found_only_in_a_directory_a_stage_may_write_is_skipped_as_not_installed`
`verified-by: bravebot_agent::usability_path::a_setup_stage_starts_the_program_behind_a_directory_a_stage_may_write`
`verified-by: bravebot_agent::usability_path::a_setup_stage_looks_up_the_programs_it_starts_only_behind_the_directory`
`verified-by: bravebot_agent::usability_path::a_gh_found_only_in_a_directory_a_stage_may_write_gives_no_login_row`
`verified-by: bravebot_agent::usability_path::the_login_is_asked_of_the_gh_behind_a_directory_a_stage_may_write`
`verified-by: bravebot_agent::usability_path::the_gh_asked_for_the_login_looks_up_the_programs_it_starts_only_behind_the_directory`
`verified-by: bravebot_sandbox::process_start_lint::a_start_is_refused_whatever_names_its_program`
`verified-by: bravebot_sandbox::process_start_lint::the_same_starts_with_a_mark_and_a_reason_pass`
`verified-by: by-construction (make check, make check-linux and make check-windows run clippy over every target, so a start with no mark fails them; make check-security fails a clippy.toml that no longer lists std::process::Command::new, a second clippy configuration inside a crate, which clippy reads in place of the root one, and a mark with no reason, and its selftest pins each)`

<a id="SANDBOX-31"></a>
### SANDBOX-31: on macOS a confined process looks up a fixed list of Mach services and no others

A Seatbelt profile allows a Mach lookup of the services in a fixed list and of nothing else: the
account and group lookups a program makes as it starts (`opendirectoryd`), the notification and log
services of libSystem, the per-user directory helper, the network configuration services, and the
keychain service `SecurityServer` ([SANDBOX-12](#SANDBOX-12)) for every policy, and certificate
trust (`trustd`, `trustd.agent`) only for a policy that allows network egress, to every host or to
a proxy's port. A stage whose network is closed ([SANDBOX-20](#SANDBOX-20)) is refused both. The
list is code. No value, argument vector, printed
output or configuration file adds to it, and it is not written as a pattern. A lookup of any other
service is refused, which includes LaunchServices (`com.apple.coreservices.launchservicesd` and
`com.apple.lsd.*`), the pasteboard (`com.apple.pasteboard.1`) and the other services that open a
file or URL, read or write the clipboard, or start a program for the caller. So `open` and
`osascript` fail under the profile, and the planner is told so after a failed step where the session
accepts a request to run a line with no sandbox.

**Why.** A Mach service acts with the account's access whatever the filesystem rows say, so a
profile that allows every lookup lets a confined program read the clipboard, ask LaunchServices to
open a file or URL in another application, and use any other service the session registers.
Neither a read row nor the absence of network egress limits them. The list is the services the
everyday programs need: `gh` fails to verify a certificate without `trustd.agent`, and
`osxkeychain` fails with `-50` without `SecurityServer`. A program that looks up another service
is refused the lookup.

The trust services are the exception to "answers a question about the machine": evaluating a
certificate chain makes them fetch the issuer from the certificate's Authority Information Access
URL, from their own process and outside the profile, so a stage that held them with no network
could make a request to any URL it names, with data in it. A program with no network has no
certificate to verify, so it loses nothing. A stage held to a proxy's port
([SANDBOX-24](#SANDBOX-24)) does need verification, and for it the host list does not stop that
fetch on macOS.

`verified-by: bravebot_sandbox::macos::the_profile_allows_a_lookup_of_named_services_and_not_of_every_service`
`verified-by: bravebot_sandbox::macos::the_trust_services_are_looked_up_only_where_the_network_is_allowed`
`verified-by: bravebot_sandbox::macos::a_confined_process_finds_the_services_programs_need_and_none_that_act_for_the_person`

<a id="SANDBOX-33"></a>
### SANDBOX-33: an OpenShell sandbox is created with no network, given its host rules once it exists, and refused where they cannot be confirmed

The `openshell` backend starts one program inside an NVIDIA OpenShell sandbox through the `openshell`
program, found as every program this one starts is found ([SANDBOX-30](#SANDBOX-30)), with only `HOME`
from this process's environment. A sandbox's name is up to 19 lower-case letters, digits and `-`,
starting with a letter, and its image a reference with no whitespace or control character. It is set
up in this order, and the program is started only at the end:

1. The sandbox is created with the filesystem OpenShell gives every sandbox, read-only system
   directories with `/tmp` writable, Landlock as a hard requirement so a kernel that cannot enforce
   it refuses the sandbox rather than running it unconfined, and no network rule.
2. It is used only once it reports `Ready`, within 60 seconds.
3. Where the program may reach hosts, its real path inside the image is read with `readlink -f`, and
   one enforced network rule per allowed host, port 443 only, for that path, is applied with
   `policy set --wait`. The rule cannot be written earlier, since the path exists only inside a
   sandbox that does. A path that is not UTF-8 cannot be named in the rule, which is text, and is
   refused rather than converted. With no host the program reaches nothing.
4. The program is started once OpenShell reports the rule loaded. Any step that fails
refuses the start, and a sandbox already created is deleted. A variable the caller would pass other
than `PATH` and `HOME` is refused before anything is created, since nothing of this machine's
environment goes into the sandbox. The program's standard streams are carried by `openshell sandbox
exec`, so a long-lived MCP connection runs over them as it does over a local child's pipes.

OpenShell inspects the TLS of what the program sends, through a certificate authority the sandbox
trusts by `SSL_CERT_FILE` and the variables like it. That is how its rules see the host of an HTTPS
request. It differs from the proxy of [SANDBOX-24](#SANDBOX-24), which never decrypts, and a program
that pins certificates or ignores those variables fails to connect.

**Why.** A policy loaded after the program starts leaves it running under a policy that is not yet
the one approved, and OpenShell applies a live change about a second later. A rule naming a symlink
such as `/usr/local/bin/python3` matches nothing, since OpenShell matches the binary that runs.

`verified-by: bravebot_sandbox::openshell::a_policy_with_no_hosts_reaches_nothing_and_requires_landlock`
`verified-by: bravebot_sandbox::openshell::each_host_is_granted_to_the_one_binary_and_enforced`
`verified-by: bravebot_sandbox::openshell::a_quote_or_backslash_cannot_end_a_scalar_early`
`verified-by: bravebot_sandbox::openshell::a_host_that_is_not_a_name_or_a_domain_is_refused`
`verified-by: bravebot_sandbox::openshell::a_name_or_image_a_command_line_could_misread_is_refused`
`verified-by: bravebot_sandbox::openshell::a_path_from_the_image_must_be_absolute_and_plain`
`verified-by: bravebot_sandbox::openshell::the_phase_is_read_from_its_own_line`
`verified-by: bravebot_sandbox::openshell::a_variable_other_than_path_or_home_is_refused_before_anything_is_created`
`verified-by: bravebot_sandbox::openshell::a_start_whose_sandbox_or_policy_cannot_be_confirmed_is_refused_and_the_sandbox_deleted`
`verified-by: bravebot_sandbox::openshell::a_program_name_that_is_not_utf8_reaches_sandbox_exec_as_its_bytes`
`verified-by: bravebot_sandbox::openshell::a_real_path_that_is_not_utf8_is_refused_before_any_rule_is_set`
`verified-by: bravebot_mcp::openshell::a_server_in_an_openshell_sandbox_answers_over_stdio_and_reaches_only_its_host`
`verified-by: bravebot_mcp::openshell::a_program_missing_from_the_image_is_refused_and_the_sandbox_removed`
`verified-by: bravebot_mcp::openshell::a_long_lived_connection_carries_many_calls_a_pause_and_a_long_reply`

<a id="SANDBOX-34"></a>
### SANDBOX-34: on Linux the step between the fork and the exec of a confined program allocates nothing

A program is confined on Linux by a closure that runs in the forked child before the exec: it leads
a session, enters the network namespace ([SANDBOX-24](#SANDBOX-24)), looks the rows up again, and
restricts the thread with the Landlock ruleset ([SANDBOX-5](#SANDBOX-5)). This process has other
threads, and a child made by `fork` holds a copy of any allocator lock another thread held at that
moment, with nothing in the child to release it, so the closure must not call the allocator or code
that does. Everything it needs is built before the fork and held by the closure: the ruleset, as a
descriptor with every rule in it, the id maps and the socket address of the namespace, and each
row's path as a C string with what was opened for it. The closure makes system calls on those and
returns an operating-system error number, which is not built into a string there.

Descriptors the closure holds are close-on-exec, so the program does not inherit them, and the
ruleset is not consumed, so one `Command` can be spawned again. Nothing about the confinement
changes: a ruleset the kernel would not enforce, a row that cannot be opened, and a row that fails
the look-up refuse the start before the process exists or in the child before the exec, and the
program is never started unconfined. The look-up of a row the caller named compares the device and
inode of what its name leads to with those of the file the rule was built on, which is the guarantee
of [SANDBOX-6](#SANDBOX-6): the name is not re-opened in the child, since opening allocates, and a
rule is never left on a file the name no longer leads to.

**Why.** A child that blocks in the allocator never reaches the exec, so the program does not start
and the caller waits for it. The block happens only when another thread was inside the allocator at
the fork, so it passes a test run and appears under load.

A test harness forks a child, arms the thread, runs the closure's work and reads the child's exit
code. The harness installs an allocator that ends the process with a code of its own when an armed
thread calls it, and it kills a child that has not ended at a deadline, so a regression fails the
test in place of hanging the run. The tests of the harness run on every Unix. The tests of the work
need Linux and a kernel with Landlock or user namespaces, and say so when the kernel has neither.

`verified-by: bravebot_sandbox::alloc_probe::a_call_to_the_allocator_by_an_armed_thread_ends_the_child`
`verified-by: bravebot_sandbox::alloc_probe::a_call_to_the_allocator_while_disarmed_is_allowed_and_work_without_one_reports_its_result`
`verified-by: bravebot_sandbox::alloc_probe::a_child_that_blocks_is_killed_at_the_deadline`
`verified-by: bravebot_sandbox::alloc_probe::the_work_between_the_fork_and_the_exec_allocates_nothing`
`verified-by: bravebot_sandbox::alloc_probe::a_refusal_between_the_fork_and_the_exec_allocates_nothing`
`verified-by: bravebot_sandbox::alloc_probe::entering_a_namespace_allocates_nothing`

## Programs a person asked for

A program `run` ([tools/run.md](tools/run.md)) starts is confined on Linux, macOS and Windows
([SANDBOX-17](#SANDBOX-17), [SANDBOX-18](#SANDBOX-18)), and the clauses above are what the profile
is held to. What confinement bounds is the filesystem. The programs somebody might ask for cannot
be listed in advance, and a list that names `gh`, `git` and `cargo` refuses a script that starts a
fourth program for lack of a scope. So on Linux and macOS a stage reads the machine except the
credential table ([SANDBOX-12](#SANDBOX-12)) and writes only the session's directories, the
temporary directory and the toolchain caches. A `git push` needs the public half of a key, which
the table leaves readable, and the agent socket, which the remote scope above lends to a stage
whose argv names the operation. On Windows a program is held to the paths the plan a person
endorsed accounts for, and not to whatever else it could open. Each part of the decision that is
not built is marked where it appears.

**What a stage reads on Linux and macOS is the machine except where a credential sits.** The
network is unchanged, so a token a program can read is a token it can send to any host the plan
lets it reach, which is why the files a tool reads by name to do its job (`~/.config/gh`,
`~/.git-credentials`, `~/.netrc`, `~/.npmrc`, `~/.cargo/credentials.toml`, `~/.pypirc`) are
readable and the table is what the account would not want a script to read at all.

**The default keeps four things in reach of every stage on Linux and macOS:** those token files
and any other a tool keeps outside the table, such as `~/.m2/settings.xml` and
`~/.gradle/gradle.properties`; the environment, less this agent's own credentials
([RUN-12](tools/run.md#RUN-12)), so `GITHUB_TOKEN`, `NPM_TOKEN` and `AWS_*` reach a stage; on macOS
the login keychain file, through which a lookup of the keychain service reaches the items in it
([SANDBOX-12](#SANDBOX-12), [SANDBOX-31](#SANDBOX-31)); and in the mode `workspace-write` the write rows
of every toolchain's cache, which a build started later outside the sandbox runs code from
([SANDBOX-18](#SANDBOX-18)). With the network `open`, the default ([SANDBOX-20](#SANDBOX-20)), a
stage can send what it read to any host. Each stays because `git`, `gh`, `npm` and `cargo` need it
when a script starts them (`make`, a `just` recipe), and the binary that script's stage resolves to
does not name the program it starts, so reach keyed on the program is withheld from the script.
Keying would not cover the install script either: an `npm install` runs a package's scripts inside
its own stage, so a lift of `~/.npmrc` keyed on `npm` reaches each of them. A person withholds each
with a setting: a list of hosts bounds where a stage can send what it read
([SANDBOX-24](#SANDBOX-24)), `denyRead` refuses a token file or the keychain file and `denyWrite` a
cache ([SANDBOX-25](#SANDBOX-25)), `run.scrubEnv` withholds a variable
([RUN-12](tools/run.md#RUN-12)), and the mode `strict` reads a credential only where a stage's
credential scope names it, reads no keychain file, and writes only the cache of a toolchain the
stage resolved to or asked for ([SANDBOX-22](#SANDBOX-22)). None of them closes the sockets a Linux
kernel below Landlock ABI version 9 leaves reachable ([SANDBOX-3](#SANDBOX-3)).

**A stage can leave a file that runs when somebody opens it.** A stage writes the session's
directories, so it can leave a `.command` or `.terminal` file or an application bundle on macOS, a
`.desktop` file on Linux, or a `.bat`, `.cmd`, `.lnk` or `.exe` file on Windows. The system runs
such a file outside the profile when it is opened, and no mark tells it the file came from a
confined program. The sandbox does not mark the files a stage creates: on macOS a program carrying
the download mark waits at its first start for an answer to the system's question, so a stage that
builds a program and a later stage that starts it would wait for an answer nobody gives. The
desktop app's Open in default app hands the system a text, PDF, image, audio or video file at once
and asks the person first about any other file ([ui/docs/security.md](../../ui/docs/security.md)). Opening the
file from the file manager or a terminal is not asked about by bravebot, and stays a cost on every
platform.

The rest of this section describes the keyed lists, which are what a stage on Windows and an MCP
server start from.

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
adds to it, and it changes only in a diff somebody reviews. What may never be in it is
[SANDBOX-12](#SANDBOX-12). What it buys is that it holds no
credential directory, so `~/.ssh/id_rsa` and `~/.aws/credentials` are out of reach of a program
whose plan never named them.

| The base holds | To |
|---|---|
| the loader, the system libraries, the system binary directories, the locale data, terminfo, the time zone data, the CA bundle and the certificate directory beside it, `/etc/hosts`, `/etc/resolv.conf`, `/etc/nsswitch.conf`, `/etc/passwd`, `/etc/group`, the machine's git configuration `/etc/gitconfig`, `/dev/null`, `/dev/zero`, `/dev/random` and `/dev/urandom`, and on macOS the TLS configuration `/private/etc/ssl/openssl.cnf` | read, and write for `/dev/null` |
| the system temporary directory this process resolved as the session opened | read and write |
| on macOS, the developer directory `xcode-select -p` names as the session opens, where it is `/Library/Developer/CommandLineTools` or an application bundle's directly in `/Applications`, which is then the bundle whole | read |
| the git configuration any stage may read for an identity: `~/.gitconfig` and `~/.config/git/config` | read |

On Windows the base holds the last two rows only, since a container reads the system directories without an entry of its own. A Windows session that has a directory of its own is also granted a `temp` directory inside it, read and written, in place of the temporary-directory row, and `TEMP` and `TMP` are handed to a stage naming it ([trust-map.md](trust-map.md#TRUST-17)). A container's `TEMP` showed its own `AC\Temp` on the CI runner instead ([trust-map.md](trust-map.md#TRUST-14)). The account's temporary directory is then granted to no stage. Without a directory of its own the row is the account's temporary directory.

Four rows is the whole of what stays invisible, and each one is here because every program needs it
and none of it sits beside a token. The prelude is what a dynamic executable needs to start at all.
A lookup reads `/etc/group` as well as `/etc/passwd`, and a program stamping a time reads the time
zone data, so both are in it on the same ground: what a program without them produces is wrong
rather than absent, a listing naming a number where a group belongs, and neither sits beside a
token either. The machine's git configuration is in it because git stops on a configuration file
that is there and that it is refused. On macOS the TLS library the platform ships aborts every
program linked against it, `curl` and rustup's `cargo` among them, when it cannot read its
configuration file, so that file is in it too. And `git`, `cc`, `make` and `python3` in `/usr/bin`
are shims that run the real program out of the active developer directory, which differs by
machine, so it is resolved as the temporary directory is and not from a stage's own
`DEVELOPER_DIR=` assignment. The Command Line Tools' is granted as it is. An application bundle's
is granted as the bundle whole, whatever the bundle is called, because its developer directory
loads frameworks from beside it, and a shim whose lookup cache is empty asks `xcodebuild`, which
loads from further across the bundle. A developer directory anywhere else is in no row.
The git configuration is in the base rather than in one program's list because a stage that never
mentions `git` still shells out to it for an identity, a `cargo` fetching a git dependency among
them, and it can name a credential store without holding one. The remote scope below naming
`~/.gitconfig` as well costs nothing. What keeps the base to four rows is that a row shown on every
run is a row that teaches a person to approve without reading, and a row shown on no run is one
nobody audits: neither is free, so the split is by whether every program needs it.

The temporary directory is the one this process resolved as the session opened, and not one a
stage's own environment assignment names, so a line carrying a `TMPDIR=` assignment changes where a
program writes without changing what the profile allows. A program that cannot open a temporary file
fails outright, and that directory is one every process on the machine already reaches, so the base
names it. What it costs is that this session's own scratch directory sits inside it
([trust-map.md](trust-map.md)): an intermediate file a turn left there is readable by a program whose
plan never named it, and the mode that directory is created with does not separate two processes
running as the same account. The row is a write row, so it also reaches a unix socket another
program keeps in that directory ([SANDBOX-3](#SANDBOX-3)).

**A toolchain and its caches are the list a program brings, where the machine is not read.** The
paths a build resolves through belong to that build rather than to every program that runs, so on
Windows and under the mode `strict` they are keyed on the resolved binary a stage of the plan names
and are shown in the prompt with the rest of the plan.
There an `npm ci` is held to the npm rows and a `cargo build` in the same session to the cargo rows,
so a postinstall script cannot leave something in `~/.cargo/registry` for a later `cargo build` to
read. The key is the binary the plan already resolved and a person already read, never a name the
model supplied and never a value a configuration file holds.

On Linux and macOS in the mode `workspace-write` no list is keyed. The machine is read, so the installs and
the cargo configuration need no row, and every stage writes the caches in the table below, since a
stage is often a script that starts a build and not the build's own binary. A postinstall script can
therefore leave something in `~/.cargo/registry` for a later `cargo build` to read, and the keying
that follows does not prevent it. The token files and the installs are in no write row either way.

| The list for | Read | Read and write |
|---|---|---|
| `cargo`, `rustup` | `~/.rustup`, `~/.cargo/bin`, `~/.cargo/config.toml`, `~/.cargo/config`, `~/.asdf` | `~/.cargo/registry`, `~/.cargo/git`, `~/.cargo/.package-cache` |
| `node`, `npm`, `npx`, `npm-cli.js`, `npx-cli.js` | `~/.nvm`, `~/.asdf` | `~/.npm/_cacache` |
| `python`, `pip`, and either followed by a version | `~/.pyenv`, `~/.asdf` | `~/.cache/pip`, which on macOS is `~/Library/Caches/pip` |
| `go` | `~/.asdf` | `~/.cache/go-build`, which on macOS is `~/Library/Caches/go-build`, `~/go/pkg/mod` and `~/go/pkg/sumdb` |
| `mvn` | `~/.asdf` | `~/.m2/repository` |
| `gradle` | `~/.asdf` | `~/.gradle/caches`, `~/.gradle/wrapper`, `~/.gradle/native` |

The first column is the name of the file a stage resolved to once its links are followed, which is
why rustup's proxy and npm's own scripts are in it: a `cargo` rustup installed resolves to `rustup`,
and an `npm` a version manager installed resolves to `npm-cli.js`.

A cache is writable because a build that cannot write one fetches everything again or fails
outright, and the price is a write no plan accounted for: a program can leave something in its own
ecosystem's cache for a later build in that ecosystem to read. Where the lists are keyed, keying on
the binary holds that to one ecosystem, since the write a `cargo build` is trusted with is one an
`npm ci` never receives. Where the machine is read it holds nothing, and a program can leave
something in any ecosystem's cache. An install is read-only for the opposite reason. A build that
would install a toolchain fails, and a line somebody then runs unconfined costs less than letting a
confined stage replace the `cargo` or the `node` a later stage in the same pipeline resolves to. `npx` meets that rule: a
package the project does not hold is installed under `~/.npm/_npx`, which is an install and not a
cache, so an `npx` that would fetch one fails.

On macOS a cache is granted where it will be rather than created first
([SANDBOX-11](#SANDBOX-11)), and the grant is the cache and not the directory above it. So the first
build of an ecosystem on a machine, one whose cache's parent is not there yet, fails: an `npm ci` on
an account that never ran npm is refused `~/.npm`. Linux creates the row before the run, so a first
build there starts with its cache in place.

**A list names a cache, never the directory holding it.** A package manager keeps its token beside
its cache, so naming the parent would grant the token with it: `~/.cargo/credentials.toml` sits in
`~/.cargo`, `~/.m2/settings.xml` holds a server password, and `~/.gradle/gradle.properties` holds a
signing key. Each row above names the subdirectory a build reads, and a token file is out of every
list by never being named. Where a tool keeps state at the top of its home directory rather than in
a subdirectory, that file is named on its own, which is why the cargo lock `~/.cargo/.package-cache`
is in a row beside the registry and the token file next to it is not. The same rule keeps
`~/.config` and `~/Library/Caches` out, since `~/.config/gh` is a credential store the remote scope
below grants, so the XDG git configuration and a cache on macOS are named one directory at a time
rather than through the directory they sit in. Where the lists are keyed, `$HOME` itself is in no
row, so a file in it that no row names, `~/.npmrc` and `~/.pypirc` among them, is unreachable. What
this costs is what those files say: npm passes over a configuration file it cannot read, so a
registry or a proxy set in `~/.npmrc` is not in force for a confined stage, and naming the file is a
person's to do. The same holds for `~/.m2/settings.xml` and `~/.gradle/gradle.properties`, which
hold a credential of their own. On Linux and macOS in the mode `workspace-write` a stage reads all four,
since they are not in the table ([SANDBOX-12](#SANDBOX-12)).
Nor is Gradle's `~/.gradle/daemon` in a row: the registry there is how a client finds a daemon
already running, and a build handed to one the person's own shell started runs unconfined. Cargo's
configuration is the one a list names, because cargo fails every invocation on a configuration file
it cannot open, and it is read and never written, since a `build.rustc-wrapper` written there is a
program every later build runs. What reading it costs is a token kept in it: cargo takes
`registry.token` from that file as well as from `credentials.toml`, so a person who wrote one there
has it read by every cargo stage.

No list serves the editor `git commit` opens when it is given no message. A stage's standard input
is never the terminal ([tools/run.md](tools/run.md)), so a terminal editor has nobody to read from,
confined or not. A graphical one such as `code --wait` needs no terminal, and it is a program the
plan never showed, so no list is keyed on it.

**A command no list knows is asked about, and the answer lasts the session.** Partly built. This
applies where the lists are keyed; on Linux and macOS in the mode `workspace-write` a wrapper already
writes every cache, though under a closed network it still has no egress, since that is decided by
its own binary ([SANDBOX-20](#SANDBOX-20)). A stage no list knows runs under the base and its plan, and a person who knows
what it needs types `/reach` ([SANDBOX-23](#SANDBOX-23)) to attach a scope or a directory to it, for the session or
for good. Nothing asks about it at the prompt yet, so a build inside it that needs a cache and was
not given one fails and stays failed. The rest of this paragraph is the decision. A wrapper is the
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
| is `git push`, `fetch`, `pull`, `clone` or `ls-remote`, with no option in front of it but `-C`, or is one of `gh`'s own commands | the remote scope below |
| is `aws`, `kubectl` or `docker` | that one tool's credential directory |
| is anything else, a build or a test in the same pipeline included | none of it |

A stage reaches its own row and no other: a `docker` stage reaches neither the remote scope nor
`~/.aws`, and no row reaches a private key or `~/.ssh` as a directory, and the keychain database on disk is
reached only as the one login file the base reads ([SANDBOX-12](#SANDBOX-12)). The remote scope is `~/.ssh/config` and `~/.ssh/known_hosts` to read with `known_hosts` also
to write, that write row naming a file rather than a directory so that an account with no
`known_hosts` gets one rather than a push that fails ([SANDBOX-11](#SANDBOX-11)), the public key at
each name ssh looks for by default and each `IdentityFile` of `~/.ssh/config` names, `~/.gitconfig`,
and the stores an https helper reads:
`~/.git-credentials`, `~/.config/git/credentials`, `~/.netrc` and `~/.config/gh`. The login keychain
file is read by the base whatever scope a stage carries ([SANDBOX-12](#SANDBOX-12)), and the
system service that holds it is reached whatever scope a stage carries (the last section's list of
what has to exist first). The agent socket `$SSH_AUTH_SOCK`
names is a write row for a stage that carries the remote scope and for no other, and only where
the path it leads to is a socket outside the credential and persistence locations
([SANDBOX-18](#SANDBOX-18)), and on macOS it is reached only while the profile leaves egress open. On macOS, where the backend creates
nothing, the file is made by ssh, which can do so only into a `~/.ssh` already there, so an account
without one records no host a confined push meets. A push signs through the agent and needs no
private key, and the public half is in the scope because that is what ssh reads to name an identity
to the agent where a configuration file pins one, and a key pinned at a name of the person's own is
read through the `IdentityFile` line that pins it. A key named only by a repository's
`core.sshCommand` is not read: ssh without `IdentitiesOnly` offers every key the agent holds and
signs anyway, and one with it needs the key listed as an `IdentityFile`. A push does
need `known_hosts`, since a host it cannot verify is a push that fails. Both transports are in one
scope because which one a remote uses is written in a configuration file, and no file's contents
decide a scope.

What the scope costs is that a repository's own hooks and its own `.git/config` are read by the
`git` it is lent to, so a push somebody asked for can read a token store while it runs, and a
`core.sshCommand` or a `url.<base>.insteadOf` a clone wrote there decides what runs and where it
goes. A push to a path on disk carries the scope too, since the operation is what is matched, and
runs the other repository's receiving hooks under it. The agent socket is the narrower shape of the
same thing, and is why the ssh half of the scope names no private key: a hook that reaches the
socket can sign with the key for as long as the push lasts, and cannot take it.

**A scope is keyed to the operation, and to no file's contents.** `git` is an arbitrary command
runner given the right argv, since `-c core.sshCommand=`, `-c alias.x=!`, `-c credential.helper=`
and `--exec-path` each turn it into a way to run something else. A scope therefore follows the
operation the endorsed argv names rather than the first word of it: `git status` in a pipeline gets
none, and so does an argv with any option in front of the operation but `-C`, an operation given a
program of its own to run, and a stage with an environment assignment in front of it, because what
would run is not what the plan says would run ([SANDBOX-16](#SANDBOX-16)). A variable the session
inherited, a `GIT_SSH_COMMAND` exported in the person's own shell, is theirs as `~/.gitconfig` is
and keeps the scope. A `kubectl --kubeconfig`
and a `docker --config` get none on the same ground, since the file each is pointed at can name a
program to run. Deriving a scope from a configuration file instead would let a cloned
repository's own `.git/config` choose which secret becomes reachable. The price of reading none of
them is a scope naming stores a given machine does not use, and an `include.path` in `~/.gitconfig`
naming a file no row covers, which git treats as fatal: every `git` a confined stage runs then exits
128, scope or none, since the base names `~/.gitconfig` and nothing it includes.

A tool's directory is read and never written, because a write there is a program the person's own
shell runs later: a `credential_process`, an exec plugin, a `credsStore` helper. What that costs is
every command that writes its own directory, `docker login` storing a token and `aws sso login`
filling its cache among them, which fails under the scope and is a person's to run unconfined.

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
`~/.npmrc` among them, still needs somebody to name a directory: `/reach` for one command
([SANDBOX-23](#SANDBOX-23)), `/add-dir` in a session,
`--add-dir` on the command line, and `additionalDirectories` in a settings file, which is put as a
question of its own when the session opens ([trust-map.md](trust-map.md), [cli.md](cli.md),
[permissions.md](permissions.md)). A rule about which commands to ask about is not one of them,
because such a rule stops a question rather than extending reach. Only this route needs a path to be
nameable without being vouched for, since a scope the compiler adds is a grant in a profile and
records nothing about what anybody trusts.

**Turning the scopes off.** Not built: neither setting exists, and every plan carries the scopes its
stages name. The rest of this paragraph is the decision. `--credential-scopes` takes `planned` or `withheld`, and
`run.credentialScopes` in a settings file takes the same two values. `withheld` leaves every scope
out of every plan, so a stage reaches a credential only where a person named its directory. What it
is for is a machine whose secrets are not this session's to lend: a shared build host, or one an
administrator sets up for somebody else to work on. `planned` is the default, because almost every
session needs a scope and a default people cannot work with is one they switch off wholesale. Either
way the record of the run names each scope and the stage it was added for ([trace.md](trace.md)).

**The network stays open to it, unless the session closed it.** A profile gates egress as a whole, so it cannot tell an approved
`git push` or `gh api` from an exfiltration, and the endorsed argv already can. What confinement
narrows is what a program may read and write, and by default not what it may send
([SANDBOX-20](#SANDBOX-20) is the setting that closes it): a confined one still sends
anything inside its grants. The label on what a program prints is untouched, and no grant makes an
output trusted.

**A grant on Windows is a change to a directory.** Seatbelt reads a profile as the process starts
and Landlock installs a ruleset on the process itself, so neither leaves anything behind. An
access-control entry is on the directory when the confined process starts and is still there
afterwards unless something removes it, so confinement there writes to paths a person owns, and two
runs holding different scopes over one directory are two sets of entries on one list. The backend
removes the entries it wrote and deletes the profile it created as it is dropped, and a run ending
without reaching that leaves them.

What bounds that is a container profile per run rather than per installation. The profile name
carries a value chosen when the backend is created, so a process identifier reused after a crash
does not give a later run the name of the earlier one, and a profile that already exists is
refused rather than adopted.
`a_holder_killed_while_it_holds_a_stage_leaves_its_container_profile_registered` kills a process
holding a stage and looks for a folder `bravebot-<pid>-` under `%LOCALAPPDATA%\Packages`, then
deletes the profile it finds so the runner is left clean. It passed in the `Sandbox on Windows`
job of CI run 42972425516 (job 130363809021), which printed `profiles of holder 5804: left
["bravebot-5804-0-8829290d23592f7f"]; deleted [("bravebot-5804-0-8829290d23592f7f", 0)]; remaining
after the delete []`: the killed holder left one profile and the delete removed it. The test ties a
folder to the holder by process identifier alone, so an earlier leftover of a reused
identifier would be found as well. The entry left behind
names a security identifier no other run holds, so the residue is an entry for a container that no
longer exists rather than a standing grant to something still running, and the next run's grants are
its own. Removal is not atomic either way, which is why the cost is written here rather than treated
as a failure mode that does not arise. [SANDBOX-5](#SANDBOX-5) is where what a backend achieves is
reported as what it is.

**What has to exist first.**

- A path has to be nameable for a profile without being vouched for, on the route a person takes.
  Opening a directory in a session records it as trusted as well as reachable, and the two are
  deliberately one grant there because either half alone is no use to a tool. A confinement scope
  wants the reach and not the vouching: naming `~/.ssh` so a push can sign must not make a key
  file's contents trusted content. The command-line form already separates them, and the two session
  forms do not. A scope the compiler adds needs none of this, since it grants reach inside a profile
  and writes nothing to the record of what a person vouched for.
- The agent socket is a write row because a socket is reached only through the second list
  ([SANDBOX-3](#SANDBOX-3)), which is wider than a row that carried a connect and no write. A
  policy row for a connect alone would narrow it. The row is the socket the name leads to, judged
  as each stage starts, and the check is not repeated when the sandbox opens it, so a link swapped
  in between is not caught. The row has not been exercised against a running agent.
- The Windows base is empty, on the ground that a container reads the system directories through
  an entry the platform already wrote. The Windows job starts `cmd.exe /c` and the compiler on
  its `PATH` under it, and `git`. `git` opens `/dev/null`, which is `NUL`, before it reads its
  arguments, so it starts only where the device lets a container open it. On Windows Server 2025
  (build 26100, the runner image) the device's access list names no ALL APPLICATION PACKAGES
  entry, a container is denied it, and every stage that runs `git` ends with "could not open
  '/dev/null'" until an administrator adds the entry. No grant of this backend can add it, since
  the device is not a path under a directory the account owns, so the job adds it before the tests
  and a machine that runs `git` confined needs the same. Whether another build of Windows names
  the entry is not shown. No other program is started, and whether a `PATH` or session directory
  the account cannot change refuses the whole program is argued and not shown. A program that needs
  a file outside those directories is refused by the kernel until its plan names the file.
- A grant on a single file is written without inheritance, which is the entry a file takes. The
  program started is granted as a file this way, and that is also unexercised.
- Whether a grant crosses a junction a stage made inside a directory it was granted for writing is
  not settled. `set_entry` makes no reparse-point check, so an entry written on the directory may be
  inherited by what the junction leads to. The test
  `bravebot_sandbox::windows::a_grant_does_not_reach_through_a_junction_a_stage_made` reads the
  target's access list while a second stage runs, prints `junction grant crossed` or `junction
  grant did not cross`, and fails if the second stage can read the target. The first stage tries
  `mklink /J`, which the Windows runner refused, and then a copy of the test binary run in the
  stage that sets the mount point reparse point itself with `FSCTL_SET_REPARSE_POINT` and prints
  `junction-probe junction made` or `junction-probe junction not made: <error>`. Both routes were
  run in the `Sandbox on Windows` job of CI run 42972425516 (job 130363809021) and both were
  refused: the test printed `junction not made by a stage by either route: mklink:  [stderr:
  Access is denied.]; probe: junction not made: Access is denied. (os error 5)` and returned. Neither
  `junction grant crossed` nor `junction grant did not cross` was printed, so no run has shown
  whether a grant crosses a junction a stage made, and no clause is claimed from it. The probe's
  output does not say which of its calls was refused. Where the grant crosses,
  `set_entry` has to skip reparse points. It is not changed here, because no run has shown the
  grant crossing. The session's own `temp` directory, which a stage can replace by a
  link, is checked by name as the scratch directory is, and the junction case of that check is
  reasoned and not run on Windows.
- The job limits of [SANDBOX-17](#SANDBOX-17) were observed on the `Sandbox on Windows` job of the
  CI run of main at `d72819914` (run 42944872516, windows-2025), and passed there:
  `a_stage_starting_no_child_runs_under_a_policy_that_denies_subprocesses` (a `cmd.exe` built-in
  exits 0 under a limit of one active process, so the console host the platform adds is not
  counted against it or does not stop the start),
  `a_stage_cannot_start_a_child_where_its_policy_denies_subprocesses`,
  `a_stage_starts_a_child_where_its_policy_allows_subprocesses`,
  `a_stage_is_in_a_job_carrying_every_ui_limit` and
  `a_stage_is_in_a_job_bounding_processes_only_where_its_policy_denies_subprocesses`, the last two
  reading the limits from a program inside the stage. Whether the runner process is itself in a
  job, and so whether those runs nested the stage's job under another, was not recorded in that
  run. `a_stage_is_limited_whether_or_not_this_process_is_in_a_job` records it (it prints `this
  process is in a job: true` or `false`) and repeats the checks from a process that has put itself
  in a job. The five tests above and that one passed in the `Sandbox on Windows` job of CI run
  42972425516 (job 130363809021), which printed `this process is in a job: true`: the test process
  was in a job there, so those stages were created in a job of their own.
- The `Sandbox on Windows` job of CI, on the branch of PR 3886 at `0d87ef50f` (run 42856028633,
  job 129969265030), showed two facts about a Windows stage, each from a failing test and not from
  a passing one. A stage with write access to a directory was refused `mklink /J` there
  ("Access is denied"), so a junction made by a stage by that route has not been seen to exist.
  The test now also tries a program in the stage calling `DeviceIoControl` with
  `FSCTL_SET_REPARSE_POINT`. Run 42972425516 (job 130363809021) ran that route and it was refused
  with `Access is denied. (os error 5)`, so whether a grant crosses a
  junction a stage made is still not observed. A program started with an environment naming only some variables
  failed to start with error 203 ("The system could not find the environment option that was
  entered") in the two job-probe tests; why is not known, and those tests start the probe with
  the inherited environment. The agent's own start of a stage names its environment
  (`Environment::Only`), and the stage tests of `bravebot-agent --test confine` that start through it
  passed in the run of main above, so a named environment is not refused as such. Which variable or
  ordering of the probe's two made the difference is not known.
  `a_stage_started_with_only_the_system_root_and_one_variable_sees_that_variable` starts a stage
  with a named `SystemRoot` and one other variable. It failed in run 42972425516 (job
  130363809021) with `SetupFailed { mechanism: "appcontainer", detail: "`cmd.exe` reads its
  command line by rules of its own, and it reads '%' in an argument as its own syntax ... refusing
  rather than starting it" }`. The start was refused by [SANDBOX-4](#SANDBOX-4) before any process
  ran, because the test's line was `echo %BRAVEBOT_NAMED%`, so that run does not show whether the
  named-environment shape starts. The test now runs `cmd.exe /c set BRAVEBOT_NAMED`, which holds no
  `%`, and expects `BRAVEBOT_NAMED=named`. That form has not been run on Windows.
- The output of a `cmd.exe` line that starts a second `cmd.exe` ended with a closing quote on the
  Windows runner (`child-ran"`). The tests that start a child compare the output with that quote
  removed. Whether the quote comes from `cmd.exe` reading its `/c` line or from the line the backend
  writes ([`command_line`](../../crates/sandbox/src/windows.rs)) is not established.
- A container's `TEMP` and `TMP` on the Windows runner named its own `AC\Temp` under
  `%LOCALAPPDATA%\Packages\<profile>`, and not the `temp` directory the session handed over
  (run 42861370999, job 129988187163, branch of PR 3886 at `f3d07e6a9`). So a program that writes
  to `%TEMP%` writes to a directory that is not the session's, and the session's `temp` directory
  is written only by a program that is told its path. Why the handed-over value is not followed
  is not known. `a_stage_is_given_the_sessions_temp_and_cannot_write_the_accounts` asserts that
  the variables are set and are not the account's, and it passes in the run of main at
  `d72819914` (run 42944872516) ([TRUST-14](trust-map.md#TRUST-14)).
- Subprocess denial has no mechanism on Linux. A container bounds what a process reaches rather
  than whether it creates children, so a policy asking for that denial is refused there rather
  than applied without it ([SANDBOX-5](#SANDBOX-5)). What it costs is that Linux confines nothing
  for a caller whose policy wants a program to have no children, where Seatbelt and a Windows job
  apply one.
- A macOS developer directory outside the two places the platform installs one, an Xcode kept
  under the home or in a folder inside `/Applications`, is in no row, so on that machine every
  `/usr/bin` developer shim is refused under the base. A row for it names a directory of the
  person's on the word of a setting, and nothing has decided that yet.
- A stage's program directory and the `PATH` directories outside the home are read by every stage
  ([SANDBOX-18](#SANDBOX-18)), so a program installed under `/opt/homebrew` or `/usr/local` starts.
  That makes every directory on a person's `PATH` outside the home readable to every stage, which is
  wider than the stage's own program needs and is the price of a runner finding the interpreter it
  names. On Windows each of those rows outside the system and Program Files directories is an
  entry the platform writes onto every file and directory beneath it as a stage starts and
  removes as the stage ends, so the time a stage takes to start grows with what those directories
  hold, and the parent of a `bin` directory is a whole installation. With the `PATH` of the Windows
  CI runner a stage takes minutes to start, so the agent's tests that start a program there keep
  only the `PATH` directories under the system and Program Files directories.
- A `run` argument that names a path outside the directories the session was opened on is refused
  ([SANDBOX-17](#SANDBOX-17)). Whether the compiler should read such an argument as a path and put
  it in the plan is not settled.
- The keychain service is in the list of [SANDBOX-31](#SANDBOX-31), so it is reachable from every
  stage and not only from one carrying the remote scope. A profile holding the keychain to that
  scope has to leave the service out of the list for the stages without it, the same step the
  socket above needs. Since the base reads the login keychain file, a lookup through that service
  from any stage can reach the login keychain's database.
- A stage held to a proxy's port has the certificate trust services of [SANDBOX-31](#SANDBOX-31),
  and trustd fetches a certificate's issuer from its own process, so the host list of
  [SANDBOX-24](#SANDBOX-24) does not bound that request on macOS. No way to keep verification
  without the fetch is known. The seatbelt test of the closed case runs on macOS only; the AIA
  fetch itself was observed with a hand-built profile and not through `SeatbeltSandbox::profile`.
- A program that looks up a Mach service outside the list of [SANDBOX-31](#SANDBOX-31) has the
  lookup refused. The usability suite ([SANDBOX-21](#SANDBOX-21)) passes with the list; a program
  that needs another service (a file watcher through `com.apple.FSEvents`, Spotlight through
  `com.apple.metadata.mds`) loses that feature. Whether any program fails outright for it is not
  shown.
- The cold path of a macOS developer shim has not been exercised. With the lookup cache the shims
  keep empty, a shim asks `xcodebuild`, which refuses every invocation until the Xcode licence
  is accepted, confined or not, so a machine in that state cannot show whether that path starts,
  and the kernel test runs with the cache the account already has. That cache is kept in the
  account's own temporary directory, which is the session's only while `TMPDIR` names it. On Linux
  the kernel tests start `true`, `id` and `cat` under the base, and no TLS program or compiler.
- No Maven or Gradle build has been run under its list. Each reads a settings file no list names,
  and whether a build on a machine that holds one runs without it or stops is not settled.
- Only part of the suite runs on Windows. The decisions this backend makes before a process starts
  are pure and are run by every job that runs the suite: which capability a policy asks for, what
  each grant permits, which policies are refused, and how an argument is written onto a command
  line. The Windows job also runs the tests of the backend module, `crates/sandbox/tests/windows.rs`
  and `crates/agent/tests/confine.rs`, which start real processes in a container. The rest of the
  sandbox crate's tests are written against Unix paths and are not run on Windows, and neither are
  the agent's other tests that start a confined process. The two tests that watch a loopback
  listener run on Unix only. The Windows test that does connect to a loopback listener from a
  container, described below, passed in run 42972425516 (job 130363809021), where the connect from
  a container granted the network to a listener on `127.0.0.1` ended `refused: connection timed
  out`. That is one connection to one loopback listener, so the denial of egress on Windows is
  still shown by the capability the token is built with and not by a connection to an outside address. No test connects from a container granted the network to a private-network address
  either, so that a granted container reaches neither a private network nor, on the strength of a
  connection, the loopback of this machine is not shown by a run.
  `a_container_granted_the_network_cannot_connect_to_this_hosts_loopback` binds a listener on
  `127.0.0.1`, connects to it from a probe in a stage granted the network and asserts the connect
  was refused. It prints `connect-probe connected` or `connect-probe refused: <error>`. The assumption that a container's default network isolation refuses
  loopback is the one it tests, and a runner that connects fails it.
  The Windows tests that connect out (`a_container_connects_out_only_when_its_policy_asks_for_the_network`
  and `a_container_without_the_network_cannot_look_up_a_name`) need `github.com` reachable from the
  machine directly, without a proxy, and fail where it is not, as a machine that cannot make a
  container fails them, and do not skip.
