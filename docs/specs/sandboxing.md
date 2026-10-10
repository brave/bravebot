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
  - crates/sandbox/src/scope.rs
  - crates/sandbox/src/mode.rs
  - crates/config/src/sandbox.rs
  - crates/config/src/sandbox_network.rs
  - crates/agent/src/confine.rs
  - crates/agent/src/reach.rs
  - crates/sandbox/src/rules.rs
  - crates/sandbox/src/programs.rs
  - crates/tui/src/status.rs
documented-by: docs/website/docs/security/security.md
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
[SANDBOX-29](#SANDBOX-29) says.

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
a path the policy names for writing, and not under one it names for reading. On macOS a connect to
a socket counts as egress, so there a write row reaches a socket only while egress is granted, and
egress also reaches the resolver's socket, which every host name lookup goes through. On Linux the
socket rule holds on a kernel carrying Landlock ABI version 9, and a socket in the abstract
namespace, which has no path, is reachable whatever the policy names. A backend that cannot
enforce the network denial refuses the policy instead, so the guarantee never degrades into one
that is not in force. Which programs a session asks egress for is [SANDBOX-20](#SANDBOX-20)'s.

**Why.** A connect to a socket reaches whatever serves it, with that server's authority. A program
that reaches a Docker daemon's socket can start a container with the home directory mounted, which
is every file the profile withheld. Landlock counts the connect as a write, so both backends apply
one rule to a connect.

`verified-by: bravebot_sandbox::macos::network_is_only_allowed_when_requested`
`verified-by: bravebot_sandbox::macos::a_confined_process_cannot_reach_the_network`
`verified-by: bravebot_sandbox::macos::a_confined_process_granted_egress_cannot_reach_a_socket_outside_its_grants`
`verified-by: bravebot_sandbox::macos::a_confined_process_granted_egress_can_reach_the_resolver`
`verified-by: bravebot_sandbox::macos::a_confined_process_cannot_write_outside_its_grants`
`verified-by: bravebot_sandbox::macos::a_confined_process_runs`
`verified-by: bravebot_sandbox::linux::a_policy_requiring_network_denial_is_refused`
`verified-by: bravebot_sandbox::linux::a_confined_process_runs`
`verified-by: bravebot_sandbox::linux::a_confined_process_can_write_inside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_cannot_write_outside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_cannot_read_outside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_cannot_truncate_a_file_outside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_can_truncate_a_file_inside_its_grants`
`verified-by: bravebot_sandbox::linux::a_confined_process_cannot_drive_a_device_it_was_granted_for_reading`
`verified-by: bravebot_sandbox::linux::the_ruleset_handles_every_right_this_crate_knows_of`
`verified-by: bravebot_sandbox::windows::a_policy_that_did_not_ask_for_the_network_asks_for_no_capability`
`verified-by: bravebot_sandbox::windows::a_policy_that_asked_for_the_network_asks_for_the_internet_client_capability`
`verified-by: bravebot_sandbox::windows::a_policy_withholding_the_network_is_applied_rather_than_refused`
`verified-by: bravebot_sandbox::windows::no_grant_lets_a_confined_process_rewrite_an_access_list`

<a id="SANDBOX-4"></a>
### SANDBOX-4: content cannot inject into the syntax a backend writes it into

Grants are paths and arguments are content. A backend writes both into a syntax: a profile the
kernel parses, or the single command line a platform's process creation takes instead of a vector.
Either is quoted rather than interpreted, so a confinement built from a hostile path still applies
and a program is asked for what the caller asked for.

**Why.** A grant list assembled from paths would otherwise be a place where a filename decides what
the sandbox permits, and a command line assembled from arguments a place where one decides what the
confined program is told to do.

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

<a id="SANDBOX-5"></a>
### SANDBOX-5: capabilities report what the kernel actually enforces, and never more

A backend says what it can enforce rather than what it was asked for, and a policy demanding
something the backend cannot deliver is refused. Which paths it can name is reported the same way.
`bravebot doctor` reports the level this platform can enforce.

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
`verified-by: bravebot_sandbox::windows::capabilities_report_what_a_container_enforces`
`verified-by: bravebot_sandbox::windows::a_policy_requiring_subprocess_denial_is_refused`
`verified-by: bravebot_sandbox::windows::each_run_confines_through_a_profile_of_its_own`
`verified-by: bravebot_sandbox::windows::a_reused_process_identifier_and_sequence_still_get_a_profile_of_their_own`
`verified-by: bravebot_sandbox::windows::a_profile_name_fits_what_the_platform_accepts`

<a id="SANDBOX-6"></a>
### SANDBOX-6: every path a policy names is granted, or the policy is refused

A policy is granted as written, apart from the `.git` writes [SANDBOX-14](#SANDBOX-14) withholds
on macOS. A backend that cannot install a grant for one of the paths refuses
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
`verified-by: bravebot_sandbox::windows::a_path_that_is_not_on_disk_is_named_rather_than_left_out`
`verified-by: bravebot_sandbox::windows::a_policy_whose_paths_are_all_there_names_none`
`verified-by: bravebot_sandbox::windows::each_path_is_granted_what_the_list_it_was_named_in_asks_for`
`verified-by: bravebot_sandbox::windows::a_path_granted_for_reading_is_not_granted_writing_or_deleting`
`verified-by: bravebot_sandbox::windows::a_path_named_for_reading_and_for_writing_is_granted_once_for_writing`
`verified-by: bravebot_sandbox::windows::a_grant_reaches_what_is_under_the_directory_it_is_written_on`
`verified-by: bravebot_sandbox::windows::a_program_is_refused_a_file_outside_its_grants_and_reads_one_inside`
`verified-by: bravebot_sandbox::windows::a_program_writes_inside_its_grants_and_not_outside`

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

The directory a process starts in is the caller's as well. A policy may name one, and otherwise the
process starts in this one's. Naming it grants nothing: a process started in a directory it may not
read is refused its first read of it, so a caller meaning the process to work there grants the
directory too.

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
`verified-by: bravebot_sandbox::macos::a_confined_process_starts_in_the_directory_its_policy_names`
`verified-by: bravebot_sandbox::linux::a_confined_process_starts_in_the_directory_its_policy_names`

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
nothing otherwise. Under a sandbox mode other than `standard` ([SANDBOX-22](#SANDBOX-22)) both
lines say so beside the level: `strict` names itself, and `off` says that programs run unconfined.

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
`verified-by: bravebot_cli::main::the_confinement_line_names_the_mode_unless_it_is_standard`

<a id="SANDBOX-11"></a>
### SANDBOX-11: a write row says what is at the path it names, and one that is not there is created

A row granting a write says whether the path it names is a file, a directory, or neither. Before a
policy is resolved against a backend that cannot name a path which does not exist
([SANDBOX-9](#SANDBOX-9)), every write row saying which of the two it is and not on disk is created:
a file empty, with the directory holding it, and a directory empty, each reachable by the account
that owns it and by nobody else. A row saying neither is not created. A path already there is left
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
or configuration file adds to it or removes from it. In the home directory it holds the program's own state
directory `~/.bravebot`, where the gateway keys, the premium token and the server list are, and then `~/.ssh`,
`~/.aws`, `~/.kube`, `~/.docker`, `~/.azure`, `~/.config/gcloud` and `~/.gnupg`; on macOS also
`~/Library/Keychains`, `/Library/Keychains`, the browser profiles under
`~/Library/Application Support` (`BraveSoftware`, `Google/Chrome`, `Firefox`), `~/Library/Cookies`
and `~/Library/Safari`; on Linux also `~/.local/share/keyrings`, `~/.password-store`,
`~/.config/BraveSoftware`, `~/.config/google-chrome`, `~/.config/chromium` and `~/.mozilla`. Three
kinds of file in `~/.ssh` hold no secret and are read: `config`, `known_hosts` and the default
public keys (`id_rsa.pub`, `id_dsa.pub`, `id_ecdsa.pub`, `id_ecdsa_sk.pub`, `id_ed25519.pub`,
`id_ed25519_sk.pub`). A public key at another name is read by a stage that carries the remote
scope when `config` names it, and the one `user.signingkey` names by a stage that signs
([SANDBOX-16](#SANDBOX-16)). On macOS the file `~/Library/Keychains/login.keychain-db` is also read, with
the directory around it and every other file in it still refused, because `gh` and git's
`osxkeychain` helper keep their tokens in that keychain and open its database file themselves.
Opening a keychain also makes the Security framework write its framework database, which is the
`mds` directory under the per-user cache directory that `confstr` names for the account
(`_CS_DARWIN_USER_CACHE_DIR`, resolved by the host with its links followed and handed in by the
caller), and that directory is written by a stage that reads the machine and by no other. Without
the read row, `gh` reports a failed login and `osxkeychain` fails with `-67674`; with only the read
row they still do. The files a program reads by name to do what it was started for, which are
`~/.config/gh`, `~/.git-credentials`, `~/.netrc`, `~/.npmrc`, `~/.cargo/credentials.toml` and
`~/.pypirc`, are not in the table: the network is where they could leave, and the plan decides that
([SANDBOX-3](#SANDBOX-3)). A path is refused by where it leads, so a link from a readable directory
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
account holding the right to change those directories could write.

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
`verified-by: bravebot_sandbox::base::the_run_base_lifts_the_ssh_files_that_hold_no_secret_and_only_those`
`verified-by: bravebot_sandbox::base::the_run_base_leaves_the_token_files_readable`
`verified-by: bravebot_sandbox::base::the_run_base_writes_only_the_temporary_directory_and_the_null_device`
`verified-by: bravebot_sandbox::base::the_run_base_on_macos_reads_the_login_keychain_file_and_no_other_keychain_file`
`verified-by: bravebot_sandbox::base::the_security_cache_row_is_the_one_directory_under_the_user_cache`
`verified-by: bravebot_agent::confine::the_security_cache_is_written_by_a_macos_stage_that_reads_the_machine_only`
`verified-by: bravebot_sandbox::macos::a_stage_under_the_run_base_writes_the_security_cache_and_no_other_cache`
`verified-by: bravebot_sandbox::macos::the_user_cache_directory_is_an_existing_path_with_its_links_followed`
`verified-by: bravebot_sandbox::base::the_run_base_on_windows_is_the_keyed_base`
`verified-by: bravebot_sandbox::base::the_run_base_without_a_home_refuses_only_the_machine_wide_keychains`
`verified-by: bravebot_sandbox::macos::a_stage_under_the_run_base_is_refused_each_credential_location_and_reads_the_rest`
`verified-by: bravebot_sandbox::linux::a_stage_under_the_run_base_is_refused_each_credential_location_and_reads_the_rest`
`verified-by: bravebot_agent::confine::a_stage_reads_the_machine_and_is_refused_the_credential_locations`
`verified-by: bravebot_agent::home::the_state_directory_is_the_one_the_sandbox_refuses`
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

`verified-by: bravebot_sandbox::macos::a_confined_process_can_look_at_any_path_and_read_or_list_only_its_grants`

<a id="SANDBOX-14"></a>
### SANDBOX-14: on macOS a write row does not reach a `.git` beneath it

On macOS a policy that grants any write refuses a write to every path with a `.git` component, in
any case: creating, changing, renaming into or removing a file or directory named `.git` or inside
one, under a row the policy writes as under any other. Every other write a row grants is granted.
A policy can lift the refusal, as the profile of a program a person asked for does so that `git`
works in the directories it was given ([SANDBOX-18](#SANDBOX-18)), and then a write row reaches a
`.git` as it does on Linux. On Linux Landlock grants a directory with everything beneath it and has no way to hold one
subdirectory back, so this clause does not hold there, which is
[mcp-servers.md](mcp-servers.md)'s known cost. The Windows backend withholds nothing of the kind,
and no server is started there ([SERVERS-10](mcp-servers.md#SERVERS-10)).

**Why.** git runs the commands a repository's configuration and hooks name, with nothing confining
them, the next time anybody runs git in it. A confined process that may write a `.git` may
therefore run code outside its confinement. Seatbelt lets the last rule matching a path decide, so
the refusal follows the rows it narrows. The default macOS volume opens `.GIT` when git asks for `.git`, which is why
the case of the name does not matter.

`verified-by: bravebot_sandbox::macos::a_write_row_does_not_reach_a_git_directory_beneath_it`
`verified-by: bravebot_sandbox::macos::a_policy_allowing_git_directory_writes_reaches_a_git_directory`
`verified-by: bravebot_sandbox::macos::the_git_refusal_is_in_the_profile_only_for_a_policy_that_writes_and_has_not_lifted_it`

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
What decides a row is the table and the platform: nothing on the machine is read, so `CARGO_HOME`,
`GOCACHE` and `XDG_CACHE_HOME` move no row. A list also says whether its program fetches, which is
the egress a closed network leaves it ([SANDBOX-20](#SANDBOX-20)): cargo, npm and the other node
package managers, pip, go, mvn and gradle fetch, and `node`, `python3` and a program no list knows
do not. `run` adds the list for each stage it starts
([SANDBOX-18](#SANDBOX-18)).

**Why.** The paths a build resolves through belong to that build and not to every program that runs,
and the file a stage resolved to is the part of the plan a person read, where a name the model wrote
or a configuration file's contents is not. One list shared by every toolchain lets a postinstall
script leave something in `~/.cargo/registry` for a later `cargo build` to read. A package manager
keeps its token beside its cache, so a row naming the directory holding the cache hands the token to
every build in that ecosystem. An install written by a confined stage is the `cargo` a later stage
resolves to, and a configuration file written by one is a `build.rustc-wrapper` every later build
runs. Cargo's configuration is read at all because cargo fails every invocation on a configuration
file it cannot open, which a machine with a `~/.cargo/config.toml` otherwise meets on every build.

`verified-by: bravebot_sandbox::toolchain::a_list_is_keyed_on_the_file_a_program_resolved_to`
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
given `--kubeconfig` or `docker` is given `--config`. A stage reaches its own scope and no other.
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

A signed commit runs `ssh-keygen -Y sign` with the file `user.signingkey` names. The tool loads that
public key and asks the agent for the signature, so a stage that signs needs the one file and the
agent's socket, and never the private key. The signing scope reads that file and the stage may write
the socket `SSH_AUTH_SOCK` names, as the remote scope may. It is carried only where the person's own
`~/.gitconfig` or `~/.config/git/config` sets `gpg.format` to `ssh`, so an account that does not sign
with ssh sees no difference, the network under a closed setting included. The later file and the last
assignment win, section and variable names are matched without regard to case, a quoted value is
unquoted and an unquoted `#` or `;` starts a comment. A `[gpg "ssh"]` or `[user "name"]` section is
another section, a value holding a backslash is not read, and a repository's configuration, an
`[include]` and `GIT_CONFIG_GLOBAL` are not followed. The key is read where `user.signingkey` is a
key written out (`ssh-` or `key::`), which needs no file, where it is unset, which signs with the
agent's first key and needs no file, or where it is an absolute path or one beginning `~/` that
resolves to an existing regular file named `*.pub` inside the home directory and outside every
credential location other than `~/.ssh`. A private key, a name nothing is at, a relative path, a path
holding `..`, a file outside the home and a link that leads out of those places are refused, and
the stage is told so in the profile line without the value, since the signature then fails. `git
pull` merges or rebases and signs the commit it makes, so the remote scope reads the same file. The
scope is not on the menu of [SANDBOX-26](#SANDBOX-26): a line carries it by its argv and the planner
never names it. The configuration limits are these: `-S<key>` and `--gpg-sign=<key>` in the argv do
not move the key that is read, and a hook or a `gpg.ssh.program` the plan wrote into `.git` runs
with the scope, as one runs with the remote scope.

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
standard mode cannot write, and a line in it can add only a file named `*.pub`, so reading it adds no
private key. A repository's `core.sshCommand` is not read because it decides what runs. Signing is a scope of its
own so that the agent and the network are not lent to a `git commit` of an account that does not sign
with ssh, and so that a commit does not read `~/.git-credentials`; the key it needs is one file in
the person's home that their configuration names, and is read for the same reason `IdentityFile`
keys are. A configuration of the person's is read for it and a repository's is not, because the
repository's is a file the plan may write. `~/.bravebot` is refused the same way, since a
variable that named it would lift the refusal of the gateway keys ([SANDBOX-12](#SANDBOX-12)).

`verified-by: bravebot_sandbox::scope::an_operation_that_talks_to_a_remote_carries_the_remote_scope`
`verified-by: bravebot_sandbox::scope::a_git_operation_that_talks_to_no_remote_carries_none`
`verified-by: bravebot_sandbox::scope::a_gh_argv_that_runs_a_program_gh_did_not_write_carries_none`
`verified-by: bravebot_sandbox::scope::an_option_in_front_of_the_operation_carries_none`
`verified-by: bravebot_sandbox::scope::an_operation_option_naming_a_program_carries_none`
`verified-by: bravebot_sandbox::scope::a_stage_with_an_assignment_in_front_of_it_carries_none`
`verified-by: bravebot_sandbox::scope::an_option_that_runs_nothing_keeps_the_scope`
`verified-by: bravebot_sandbox::scope::a_tool_pointed_at_another_configuration_file_carries_none`
`verified-by: bravebot_sandbox::scope::a_program_no_scope_knows_carries_none`
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
`verified-by: bravebot_sandbox::scope::signing_cannot_be_named`
`verified-by: bravebot_sandbox::scope::the_signing_scope_reads_the_public_key_and_nothing_else`
`verified-by: bravebot_sandbox::scope::the_signing_scope_reads_nothing_where_the_key_is_refused_or_signing_is_off`
`verified-by: bravebot_sandbox::scope::the_remote_scope_reads_the_signing_key_as_well`
`verified-by: bravebot_sandbox::signing::a_public_key_in_the_home_is_the_file_a_stage_that_signs_reads`
`verified-by: bravebot_sandbox::signing::a_format_other_than_ssh_reads_no_key`
`verified-by: bravebot_sandbox::signing::a_literal_key_and_no_key_read_no_file`
`verified-by: bravebot_sandbox::signing::a_value_that_is_not_a_public_key_in_the_home_is_refused`
`verified-by: bravebot_sandbox::signing::the_last_value_of_the_right_section_is_the_one_read`
`verified-by: bravebot_sandbox::signing::a_value_with_a_backslash_is_refused`
`verified-by: bravebot_agent::confine::a_stage_that_signs_is_lent_the_key_and_the_agent_where_the_person_signs_with_ssh`
`verified-by: bravebot_agent::confine::a_stage_that_signs_carries_nothing_where_the_person_does_not_sign_with_ssh`
`verified-by: bravebot_agent::confine::a_stage_that_does_not_sign_is_not_lent_the_agent`
`verified-by: bravebot_agent::confine::a_signing_key_no_scope_reads_is_explained_to_the_planner_without_its_path`
`verified-by: bravebot_sandbox::macos::a_stage_that_signs_loads_the_public_key_and_reaches_the_agent_never_the_private_key`
`verified-by: bravebot_sandbox::scope::the_ssh_configuration_adds_a_bounded_number_of_keys_to_the_remote_scope_alone`
`verified-by: bravebot_sandbox::macos::a_remote_stage_reads_a_public_key_its_configuration_names_and_never_the_private_one`
`verified-by: bravebot_agent::confine::the_prompt_the_line_and_the_policy_agree_on_which_credential_a_stage_lifts`

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

Confinement is a property of the session and not of the plan: the terminal, desktop, plain and
one-shot front ends each ask for it when they open a session. A caller that does not ask, which is a
test driving the executor directly, starts stages as the user's own shell would.

On Windows the stage is created in a container instead of being handed back as a command, since
confinement there is an argument to the call that creates the process: the executor gives the
backend the three standard streams it has made and keeps the sandbox alive for as long as the
process, because the entries the backend wrote are removed as it is dropped. A batch file (`.bat`
or `.cmd`) is refused there, because the command interpreter reads its argument line by its own
rules and a quoting that is safe for every other program is not safe for it. Any other program the
platform cannot confine is refused as it is elsewhere.

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
([trust-map.md](trust-map.md)). On Linux and macOS a stage reads the machine except the credential
table ([SANDBOX-12](#SANDBOX-12)), and writes only the session's directories, so what it costs is a
write outside them and a read of a credential location its plan does not carry. On Linux each
stage start lists every directory above a refused location, and the time that takes grows with the
number of entries in those directories, the home directory among them. On macOS the refusal of a `.git` write is lifted for these stages
([SANDBOX-14](#SANDBOX-14)), which gives them the reach Linux gives.

`verified-by: bravebot_agent::confine::a_confined_program_cannot_read_a_file_outside_the_session`
`verified-by: bravebot_agent::confine::a_confined_program_reads_the_machine_and_not_a_credential_location`
`verified-by: bravebot_agent::confine::a_file_created_between_two_stages_is_readable_by_the_second`
`verified-by: bravebot_agent::confine::a_link_to_a_credential_is_judged_by_where_it_leads`
`verified-by: bravebot_agent::confine::a_confined_program_reads_and_writes_inside_the_session`
`verified-by: bravebot_agent::confine::a_confined_program_cannot_write_outside_the_session`
`verified-by: bravebot_agent::confine::a_program_left_running_is_confined_as_well`
`verified-by: bravebot_agent::confine::a_stage_that_cannot_be_confined_is_refused_with_no_stage_left_running`
`verified-by: bravebot_agent::confine::a_job_with_a_stage_that_cannot_be_confined_is_refused_with_no_stage_left_running`
`verified-by: bravebot_agent::turn::a_delegate_of_a_confining_turn_cannot_write_outside_the_session`
`verified-by: bravebot_agent::tools::a_turn_that_confines_runs_is_confined_to_its_workspace_and_a_turn_that_does_not_is_not`
`verified-by: bravebot_sandbox::linux::a_command_handed_back_is_confined_when_the_caller_spawns_it`
`verified-by: bravebot_sandbox::macos::a_command_handed_back_is_confined_when_the_caller_spawns_it`
`verified-by: bravebot_sandbox::windows::a_batch_file_is_refused_and_an_executable_is_not`
`verified-by: bravebot_agent::confine::variable_names_match_without_case_only_when_folding`

<a id="SANDBOX-18"></a>
### SANDBOX-18: the profile a stage runs under is composed from the plan and the session's directories

On Linux and macOS a stage of a session that names a home directory has as its policy the run base
([SANDBOX-12](#SANDBOX-12)), with `.git` writes allowed, plus: the cache rows of every toolchain, written, since the machine is already read and a
cache is where any build may write ([SANDBOX-15](#SANDBOX-15)); the credential scope its argv names
([SANDBOX-16](#SANDBOX-16)); each directory the session was opened on, to read and write; the
session's scratch directory, to read and write; and the plan's directory as the place it starts.
No row names the directories its program is read from or the two files the step names, since the
machine is read. Where the stage carries the remote scope, the socket `SSH_AUTH_SOCK` names in its
own environment is a write row ([SANDBOX-3](#SANDBOX-3)). Nothing else is in it: no write outside
those, no credential location the stage's scope does not name, and no socket for a stage without
the remote scope.

On Windows, and on Linux and macOS in a session that names no home directory, since the credential
rows are rows under it and a read of the whole machine with none to subtract is the one grant that
must not be made, a stage's policy is the keyed base ([SANDBOX-12](#SANDBOX-12)), with `.git`
writes allowed, plus:
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
since a socket is reached through a write ([SANDBOX-3](#SANDBOX-3)). Nothing else is in it: no
credential directory, no directory above a session directory, and no socket for a stage without the
remote scope.

On Windows a stage is also refused when a directory the session was opened on is, holds or lies
inside one of the credential locations under the home directory ([SANDBOX-12](#SANDBOX-12)): the
state directory `~/.bravebot`, which holds `settings.json`, `~/.ssh`, `~/.aws` and the rest of the
shared table. The row would be a read and write grant, and a Windows container cannot be refused a
path inside a grant, so the refusal is made before the policy is built and no grant is written. The
paths are compared by component without regard to case, and a credential location is compared in
the form it is named and in its resolved form. A session opened on a project directory is not
affected. Linux and macOS grant such a directory and refuse the locations inside it, so the
refusal does not apply there.

Every input is the compiled step a person read, the session's own directories or the process's own
environment. Nothing a program printed, and no value the model supplied beyond the plan, reaches a
row. Rows that name an absent path are created or left out as [SANDBOX-9](#SANDBOX-9) and
[SANDBOX-11](#SANDBOX-11) say, by what the backend can grant.

**Why.** The grant is the plan, so a line nobody is asked about gets the same profile as one
somebody answered. The `.git` hold-back ([SANDBOX-14](#SANDBOX-14)) is lifted because a stage
started in a directory it may write is where a person runs `git commit`, and a profile that refused
it would be one somebody turns off; it is the same reach Linux gives. A program installed inside the
home is read as the file a person read and not as its directory, so that granting a program does not
open the home.

`verified-by: bravebot_agent::confine::the_session_directories_are_read_and_written_and_nothing_else_of_the_persons`
`verified-by: bravebot_agent::confine::the_description_follows_the_platform_it_describes`
`verified-by: bravebot_agent::confine::a_step_whose_plan_names_no_credential_reaches_nothing_in_the_home`
`verified-by: bravebot_agent::confine::a_toolchains_cache_is_granted_to_its_own_binary_only`
`verified-by: bravebot_agent::confine::a_session_with_no_home_is_not_granted_the_machine`
`verified-by: bravebot_agent::confine::a_push_reaches_the_remote_scope_and_a_status_does_not`
`verified-by: bravebot_agent::confine::the_agent_socket_goes_to_a_remote_step_and_to_no_other`
`verified-by: bravebot_agent::confine::an_assignment_in_front_of_a_push_removes_its_scope`
`verified-by: bravebot_agent::confine::a_program_at_the_top_of_the_home_is_granted_as_a_file_and_not_as_the_home`
`verified-by: bravebot_agent::confine::the_path_outside_the_home_is_read_and_the_homes_own_bin_brings_no_parent`
`verified-by: bravebot_sandbox::rules::a_verbatim_drive_path_loses_its_prefix_and_nothing_else_does`
`verified-by: bravebot_sandbox::windows::what_every_container_reads_is_the_machines_own_directories`
`verified-by: bravebot_sandbox::windows::nothing_else_is_taken_to_be_readable_by_every_container`
`verified-by: bravebot_sandbox::windows::a_write_grant_under_a_system_directory_still_needs_its_entry`
`verified-by: bravebot_agent::confine::a_windows_session_on_the_home_directory_is_refused_for_the_state_directory`
`verified-by: bravebot_agent::confine::a_windows_session_inside_a_credential_location_is_refused`
`verified-by: bravebot_agent::confine::a_windows_session_is_compared_to_the_credential_locations_without_regard_to_case`
`verified-by: bravebot_agent::confine::a_name_that_is_not_text_is_not_taken_for_its_lossy_lookalike`
`verified-by: bravebot_agent::confine::a_windows_session_beside_the_credential_locations_is_not_refused`
`verified-by: bravebot_agent::confine::a_windows_session_with_one_root_on_the_home_directory_is_refused_whatever_the_others_are`
`verified-by: bravebot_sandbox::base::the_credential_locations_are_the_rows_the_run_base_refuses`

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
`--add-dir`). On a platform with no base the description says programs are not confined. Under the mode `strict`
([SANDBOX-22](#SANDBOX-22)) it gives the Windows form's account of what is reachable, since the
machine is not read there either. On a turn
that does not confine runs, and under the mode `off`, it says nothing about confinement, since a planner told of a boundary its
programs do not have stops reaching for paths they can reach.

A result whose step did not exit zero, from a run or from a job left running, carries one more
sentence from the driver after the account of how it ended ([RUN-13](tools/run.md#RUN-13)): the
directories the programs could read and write, that beyond those they reached only what a
toolchain list or credential scope added for the steps that named one, the toolchain lists the plan
brought by name and the credential scopes it brought by name, or `none`, and whether the network
was open or closed and, if closed, the reasons that kept it for the steps that had one
([SANDBOX-20](#SANDBOX-20)). Under the modes `strict` and `standard`, where the session has a profile directory, the description
and this sentence also name the menu a `run` call may ask from ([SANDBOX-26](#SANDBOX-26)); under
`off` they name none, since none is accepted. The sentence is the same words for exit 1 and exit 2.
A result whose steps all
exited zero carries no such sentence. The sentence is composed from the same two decisions the policy is
([SANDBOX-18](#SANDBOX-18)), so the two cannot name different lists.

The sentence ends with fixed text, in every mode that produces it, saying that a credential
location such as `~/.ssh`, `~/.aws` or `~/.kube` cannot be added with `/add-dir` or `--add-dir`,
and that where the mode accepts a request, a credential scope is how a line reaches one. It then
says that any other path a command needs is asked for with the `request_path` tool
([SANDBOX-28](#SANDBOX-28)), the person is asked, and a yes lasts for the session. A planner
that meets a refusal on such a location otherwise has no other account of it, and asks the person
for an `/add-dir` that does nothing. The text is the same for every failure and for exit 1 and
exit 2. The menu sentence ends the same way, and the `run` description's
confinement statement names the tool.

**Why.** A refused step reports `exited 1` and its standard error is quarantined, so the planner
cannot see that the sandbox refused it. Without this sentence it cannot tell a sandbox refusal from
a fault in the machine, so it chases causes that do not apply and sends the person to run
diagnostics that cannot show the boundary.

**It branches on no output.** The inputs are the session's directories, the compiled step and the
exit status the result already reports ([RUN-13](tools/run.md#RUN-13)). Nothing a program printed
reaches the sentence, and it names no path a program chose, so the same sentence follows a refusal,
a failing test and a typo. It adds no branch on the exit status beyond the one that already
chooses `It failed` over `It exited 0`.

`verified-by: bravebot_agent::confine::a_confining_turn_tells_the_planner_what_a_refusal_means`
`verified-by: bravebot_agent::confine::a_turn_that_does_not_confine_says_nothing_of_confinement`
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
`verified-by: bravebot_agent::turn::a_turn_that_does_not_confine_runs_says_nothing_of_it_in_the_description_or_a_failure`
`verified-by: bravebot_agent::turn::a_failed_job_on_a_confining_turn_says_what_it_ran_under`
`verified-by: bravebot_agent::tools::the_statement_follows_the_sandbox_mode`
`verified-by: bravebot_agent::confine::a_strict_stage_does_not_read_the_machine`
`verified-by: bravebot_agent::confine::the_failure_sentence_names_the_menu_where_a_request_is_accepted`
`verified-by: bravebot_agent::confine::the_planner_is_told_of_the_menu_in_strict_and_standard`
`verified-by: bravebot_agent::turn::the_failure_sentence_is_the_same_for_exit_1_and_exit_2_and_names_the_menu`
`verified-by: bravebot_agent::turn::the_failure_sentence_names_the_menu_under_standard`

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

The setting is read from `--run-network <open|closed>`, from `run.network` in a settings file, and
from a managed pin, in that order of strength: a managed pin is final, the flag outranks a file, and
a file outranks the default. A managed pin that is neither word is read as `closed`, not ignored,
because a mistyped restriction must not open the network. A file in the person's home, or one named by `--settings` from outside
the workspace, may set either word. A file a checkout carries, which includes a named file inside
the workspace, may set `closed` and never `open`, and `doctor` says that it was ignored. A word that
is neither is reported and not obeyed. The desktop's bridge takes no flag: it settles the answer once,
when it starts, from the person's home layer, the file `--settings` named, when it exists, the settings file of the
directory it started in and the managed pin, and every window's turns read that answer. A project a
window opens later does not close it, and neither does a settings file a window selects afterwards.

Where the backend cannot deny the network (Landlock, [SANDBOX-5](#SANDBOX-5)) a stage that would
lose egress is not started, and the result says that the network is closed and the platform cannot
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
`verified-by: bravebot_config::run_network::a_managed_pin_that_is_neither_word_closes_the_network`
`verified-by: bravebot_agent::confine::the_policy_the_line_and_the_description_agree_on_which_stages_keep_the_network`
`verified-by: bravebot_agent::confine::the_fetch_bit_is_keyed_on_the_resolved_file`
`verified-by: bravebot_agent::confine::a_closed_network_is_not_reopened_by_what_a_stage_is_started_with`
`verified-by: bravebot_agent::confine::a_closed_network_is_told_to_the_planner_and_an_open_one_is_not`
`verified-by: bravebot_agent::confine::a_backend_that_cannot_deny_the_network_refuses_a_closed_stage`
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
`verified-by: bravebot_tui::confirm::a_run_prompt_says_the_network_is_closed_and_which_stage_keeps_it`
`verified-by: bravebot_tui::logo::a_closed_network_is_named_on_the_opening_screen_and_an_open_one_is_not`
`verified-by: bravebot_tui::status::a_closed_network_is_reported_with_who_closed_it_and_an_open_one_is_not`

<a id="SANDBOX-21"></a>
### SANDBOX-21: the programs people run every day are run under the default, and a change that breaks one fails

A suite runs the everyday workflows under the confinement a `run` stage is given on Linux and macOS
([SANDBOX-17](#SANDBOX-17)), each in a session directory and a scratch home of its own: `git` (init,
add, commit, branch, merge, stash, rebase, worktree, a commit hook, and push, clone, fetch and pull
against a local remote), `gh`, a wrapper script that starts `git` and `make`, `make`, `cargo`
(check, build, run, test, and a build script), `npm` and `node`, `python3`, `go`, `find`, `grep`,
`sed`, `awk`, `diff` and `patch`, and `vim` and `less`. A workflow works when every stage exits zero.
The home holds a credential of each kind and has the cache directories of an account that has used
each toolchain, so a first run on an account that never ran the tool is not what is measured
([SANDBOX-15](#SANDBOX-15)). A stage is started as `run` starts it: from the step, with the process
environment overlaid by the stage's own and the withheld names removed, and a host variable that
moves a read row (`GH_CONFIG_DIR`, `GIT_CONFIG_GLOBAL`, `XDG_CONFIG_HOME`, `CARGO_HOME`) or names
an agent socket removed.

Rows that must stay refused run beside them: reading a private key in `~/.ssh`, reading
`~/.aws/credentials`, writing to a directory the session was not opened on, and writing to the
home directory. Such a row passes when its last stage is refused and the same workflow, run again
without the sandbox, works, so a path that was never there is not counted as a refusal. A row that
is let through fails, and so does one whose earlier stage is refused.

A workflow whose program is not installed is skipped, named, and counted apart from the passes; it
is not a failure. A workflow that fails without the sandbox as well is reported as that and not as
the sandbox's. A failing row fails the suite, which runs in CI on Linux and macOS and reports the
number skipped. `bravebot doctor --sandbox-check` runs the same suite on this machine, from a directory
under the state directory, and prints each row, and for a row that failed the stage, the fix for
that kind of failure and where the stage's output went. It exits with the failed ending
([CLI-6](cli.md#CLI-6)) when a row failed. It runs nothing on Windows, where programs are confined
by container.

`bravebot doctor --sandbox-check` adds one row to the suite for the login `gh` keeps in the real
home, which the scratch home cannot hold. It runs `gh auth token` with the person's home and sends
the output to the null device, first outside the sandbox and then under it, held to the person's
filesystem lists as a session is ([SANDBOX-25](#SANDBOX-25)). The row appears only
when `gh` is installed and the run outside the sandbox exits zero. It passes when the run under the
sandbox exits zero and fails otherwise, with a fix that names `sandbox.filesystem.allowRead`
([SANDBOX-25](#SANDBOX-25)) and `gh auth login --insecure-storage`. The CI suite does not run it,
since it depends on the host's login. Token variables are removed from the environment of both
runs, so the row measures the stored login.

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
`verified-by: bravebot_cli::sandbox_check::doctor_sandbox_prints_a_failed_row_with_its_stage_and_its_fix`
`verified-by: bravebot_cli::sandbox_check::doctor_sandbox_gives_each_kind_of_failure_its_own_fix`
`verified-by: bravebot_cli::sandbox_check::doctor_sandbox_tells_a_refused_login_to_allow_the_read`
`verified-by: bravebot_cli::sandbox_check::doctor_sandbox_names_a_skipped_workflow_and_does_not_fail_on_it`
`verified-by: bravebot_cli::sandbox_check::doctor_sandbox_counts_passed_failed_and_skipped_apart`
`verified-by: bravebot_cli::running::doctor_sandbox_skips_what_is_not_installed_and_cleans_up_after_a_clean_run`
`verified-by: bravebot_cli::running::doctor_sandbox_refuses_a_further_argument`

<a id="SANDBOX-22"></a>
### SANDBOX-22: a person chooses how much a program `run` starts is held to, and a checkout can only ask for more

There are three sandbox modes. `strict` starts every stage under the deny-by-default profile
[SANDBOX-18](#SANDBOX-18) composes where the machine cannot be read: the base, the list its program's
binary brings, the credential scope its argv names and the session's directories, so a program reads
nothing else on the machine. `standard` is what [SANDBOX-17](#SANDBOX-17) and
[SANDBOX-18](#SANDBOX-18) describe, and is the mode when nothing chose another. `off` starts the
stage with no profile. The network is not part of a mode: whether a stage keeps egress is the setting
[SANDBOX-20](#SANDBOX-20) describes, and `strict` and `standard` honour it the same way. `off` starts
no profile, so under it nothing holds a network shut; the floor below is what stops a managed pin
from meeting that.

The mode is read, in this order, from `--sandbox <mode>` (the last one wins), from the `sandbox.mode`
key of the settings, and from the managed file's pin; the default is `standard`. In the settings, the home file and a `--settings` file outside
the workspace may name any of the three, the later of the two winning. The checkout's project and
local files, and a `--settings` file inside the workspace, may name only `strict`; a `standard` or
`off` there is not obeyed and `doctor` names the file and the word. A `strict` from any of them
holds over what the home file chose. A value that is not exactly one of the three words chooses
nothing and is reported with its file. Only `mode` is read from the `sandbox` block, and a key
beside it is reported as unread, so a block written for another tool does not look configured.

The managed file's `sandbox.mode` is a floor and not a default. A flag or a settings file that names
a mode looser than it refuses the session before it starts, naming both files; the same mode or a
stricter one is kept; with nothing asked, the pin is the mode. A managed file that pins `run.network`
to `closed` ([SANDBOX-20](#SANDBOX-20)) is a floor of `standard` as well, because `off` starts a
program with no profile and nothing would hold its network shut; the refusal says so. `doctor`,
`--help`, `--version` and the commands that start no program (`auth`, `sessions`, `attach`,
`completion` and the two `import-` commands) run past a refusal, so that the person can read what it
says and sign in.

The desktop and the bridge have no flag, and read the mode from the settings and the pin. `off` is
read there as `standard`, because a window has no line that shows its programs are unconfined. A
subagent's programs run under the mode of the session that started it, and the mode is not in the
session record, so a resumed session takes the mode its start-up chose and not the one it was
saved under. The mode is chosen separately from the permission mode ([MODE-1](permission-modes.md#MODE-1))
and from `--dangerously-skip-permissions`: neither widens it. One line may start with no profile
where a person approves it ([SANDBOX-29](#SANDBOX-29)), which is the mode `off` for that line only.

Under `off` the opening screen and `/status` do not report a closed network, since nothing then holds
a program to it.

Each `run` on a session that confines records a gate, `sandbox`, whose detail names the mode
the programs ran in ([TRACE-1](trace.md#TRACE-1)), or `unconfined` for a line that asked to start
with no profile ([SANDBOX-29](#SANDBOX-29)).

**Why.** A program that is refused a read or a write is a program the person wanted to run, and
having no way to say "not under this" sends them to a wrapper script outside the tool that confines
nothing and reports nothing. A mode they name, whose opening line reports it, is the same
choice with the report kept. It is the person's to make and not the checkout's: a line in a
repository that picked `off` would unconfine whoever cloned it, so the checkout may only ask for the
stricter mode, which takes nothing from them, on the footing the `permissions` keys that only
refuse already have ([PERM-18](permissions.md#PERM-18)). The pin is a floor because it is the
administrator's rule about the machine, and a flag that overrode it would be the setting they wrote
the pin to prevent. A pin looser than what a person asks for is not applied, so a person may be
stricter than the machine requires.

**What it costs.** `strict` fails any program that reads a place its binary's list and its argv do
not name; the person reads the refusal sentence ([SANDBOX-19](#SANDBOX-19)) and chooses `standard` or
adds the directory. `off` is the whole of the confinement removed: a window never offers it, and
the command line prints it on its opening line.

`verified-by: bravebot_sandbox::mode::a_mode_is_read_only_from_its_exact_word`
`verified-by: bravebot_sandbox::mode::strict_is_the_tightest_and_off_the_loosest`
`verified-by: bravebot_config::sandbox::a_mode_is_read_from_the_block_and_nothing_near_it_is`
`verified-by: bravebot_config::settings::the_home_layer_and_a_named_file_outside_the_workspace_choose_the_mode`
`verified-by: bravebot_config::settings::a_checkout_may_only_tighten_the_sandbox`
`verified-by: bravebot_config::settings::a_word_that_is_not_a_mode_chooses_nothing_and_is_reported`
`verified-by: bravebot_config::settings::the_flag_beats_the_files_and_the_default_is_standard`
`verified-by: bravebot_config::settings::a_managed_pin_refuses_a_looser_request_and_keeps_a_stricter_one`
`verified-by: bravebot_config::settings::a_pin_that_is_not_a_mode_pins_nothing`
`verified-by: bravebot_config::settings::a_closed_network_pin_refuses_off_and_leaves_the_rest`
`verified-by: bravebot_config::settings::a_key_beside_the_sandbox_mode_is_reported_as_unread`
`verified-by: bravebot_tui::logo::a_closed_network_is_not_claimed_for_programs_nothing_confines`
`verified-by: bravebot_cli::main::a_refused_mode_stops_what_runs_programs_and_not_what_does_not`
`verified-by: bravebot_config::settings::a_window_never_runs_unconfined_and_keeps_the_pin`
`verified-by: bravebot_cli::main::the_sandbox_flag_takes_one_of_three_words_and_the_last_wins`
`verified-by: bravebot_cli::main::the_sandbox_flag_refuses_a_word_that_is_not_a_mode`
`verified-by: bravebot_cli::main::doctor_names_the_sandbox_mode_and_where_it_came_from`
`verified-by: bravebot_cli::main::doctor_reports_a_mode_the_managed_file_refuses`
`verified-by: bravebot_cli::running::the_sandbox_flag_is_read_before_the_run_starts`
`verified-by: bravebot_cli::running::the_sandbox_flag_reaches_what_the_planner_is_told_of_a_run`
`verified-by: bravebot_agent::turn::a_run_under_off_starts_with_no_profile_and_says_nothing_of_one`
`verified-by: bravebot_agent::turn::a_run_under_strict_cannot_read_what_standard_reads`
`verified-by: bravebot_agent::turn::a_delegate_runs_its_programs_under_the_mode_of_the_turn_that_spawned_it`
`verified-by: bravebot_agent::turn::a_delegate_of_a_strict_turn_cannot_read_what_a_standard_one_reads`
`verified-by: bravebot_core::policy::the_trail_says_which_sandbox_mode_the_programs_ran_in`
`verified-by: bravebot_ui_bridge::permission_mode::a_window_reads_off_as_standard`

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
  absolute, no `..`, existing, not the home or above it, not `~/.ssh` or `~/.bravebot` or inside either, and a link by
  where it leads. A directory later replaced by a link to `~/.ssh` is dropped from the profile and
  from the plan.
- Write is a directory's. A scope is never written by a grant, and `/reach` refuses one.
- A stage with an assignment in front of it is covered by no grant, and a line with one is refused,
  for the reason an assignment removes a scope ([SANDBOX-16](#SANDBOX-16)).
- A stage that starts with an option (`sh -c ...`, `git -C dir push`) has no operation to key on,
  so `/reach` refuses the line, and a grant for a command given no arguments covers it only while
  it is given none. Otherwise a grant made for one script would follow every script.
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

The proxy decides from the destination in the `CONNECT` request line and from nothing else: not a
header, not a reply, not a byte the tunnel carries. TLS is not terminated, so an allowed tunnel is
two byte streams copied into each other unread, and a request that is not a `CONNECT` is refused.
A tunnel is opened only to port 443 or 80. A refused request is answered with the same bytes for
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
path to the rest. Deciding from the request line alone keeps the proxy from being a second reader
of content, and a fixed refusal keeps a name the program chose out of a sentence the planner reads.

The list is named by `sandbox.network.allowedHosts`, `deniedHosts` and `onUnlisted` (`ask` or
`refuse`). `deniedHosts` and `onUnlisted: refuse` only take reach away, so any layer may write them.
`allowedHosts` and `onUnlisted: ask` give it back, so they are read from the person's own settings,
the file `--settings` names outside the workspace and nothing a clone brings, as the allowances of
`sandbox.filesystem` are; a project or local layer's is named by `doctor` and not read. A key that
is not there is no list. A list that is set, even an empty one, is one, and a layer whose list was
not read does not make one. A value that is not a list of strings, or an `onUnlisted` that is
neither word, is reported and read as absent.

The managed layer pins the keys it writes. Its `allowedHosts` is the whole allowed set and nothing a
person wrote is added to it, so a person's entry cannot widen it and `doctor` lists the pinned keys.
An empty one pins a list that refuses every host, and filters a session that set none. Its
`deniedHosts` is added to the person's and nothing lifts it. Its `onUnlisted` is the answer whatever
a person said. A managed value that is not a list of strings, or an `onUnlisted` that is neither
word, pins nothing and `doctor` names it.

A stage with egress under a set list is started with those variables, and a stage with no egress
is not, because it reaches nothing. A stage's list is the person's allowed set and the defaults its
own reasons bring, the remote scope's hosts and its toolchain's registries, so two stages can hold
different lists. One proxy is started for each distinct list the first time a stage needs it, and
is kept for the rest of the process. A denied entry that is not a rule fails the stage, since
dropping it would let the host through, and so does a proxy that cannot be started: a stage is
never started without the filter its list asks for.

A stage held to a proxy has a policy that allows outbound TCP to that proxy's loopback port and to
no other address, and reaches no name resolver, since the proxy resolves the name it tunnels to. A
program that ignores the variables and connects to a host, or to another loopback port, is refused
by the operating system. That costs a stage the loopback services of its own, a local database or
a test server, and a program that does not send its connections through the variables: `ssh`
reads none of them, so a remote reached over ssh cannot connect while a list is set; an HTTPS
remote can. A unix socket a write row already names is still reached. macOS applies
this in the profile. Linux and Windows cannot limit egress to one port (the Landlock rules of a
kernel this build supports filter by port and leave UDP open, and an AppContainer holds the internet
capability or does not), so a stage is refused there while a list is set, with
`sandbox.network.allowedHosts` named, as a stage is refused where the network cannot be closed
([SANDBOX-20](#SANDBOX-20)). The stage is not started with the variables alone. The sentence the
planner reads under SANDBOX-19 says the network is limited by that setting and names no host.

Half built. The list, its defaults, the proxy, the settings that carry the list through the
layers, the managed pin, the environment injection and the port-limited policy on macOS are
written and tested. Unbuilt: a backend on Linux and Windows that limits egress to the port, the
prompt for an unlisted host, the trace record and the `/status` line.

`verified-by: bravebot_sandbox::policy::the_port_egress_is_limited_to_is_carried_and_goes_with_the_network`
`verified-by: bravebot_sandbox::macos::egress_limited_to_a_port_names_the_port_and_no_other_address`
`verified-by: bravebot_sandbox::macos::a_process_limited_to_a_port_reaches_that_port_and_not_another`
`verified-by: bravebot_sandbox::windows::a_policy_limiting_egress_to_one_port_is_refused`
`verified-by: bravebot_sandbox::linux::a_policy_limiting_egress_to_one_port_is_refused`
`verified-by: bravebot_agent::confine::a_platform_that_cannot_hold_a_program_to_the_proxy_is_refused_naming_the_setting`
`verified-by: bravebot_agent::confine::a_stage_under_a_host_list_is_prepared_limited_to_the_proxy_port`
`verified-by: bravebot_agent::confine::the_profile_line_names_the_host_list_setting_and_no_host`
`verified-by: bravebot_agent::confine::a_stage_under_a_host_list_cannot_connect_around_the_proxy`
`verified-by: bravebot_agent::confine::a_platform_that_cannot_hold_a_stage_to_the_proxy_refuses_it`
`verified-by: bravebot_sandbox::hosts::an_exact_entry_covers_that_name_and_no_other`
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
and a program that is given its scope is still a program. A refusal with no grant above it holds back
nothing and is not added.

`denyRead` and `denyWrite` are read from every layer, since they only take reach away. `allowRead`
and `allowWrite` are read from the person's own file, the file `--settings` names from outside the
workspace, the command line and the managed file, and never from a project layer, a local layer or a
named file inside the workspace, so a cloned repository cannot widen its own sandbox. A layer that
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
which is where a private key is ([SANDBOX-16](#SANDBOX-16)), nor to `~/.bravebot` or anything inside it, which is
where the gateway keys are ([SANDBOX-12](#SANDBOX-12)), and no list can set the `.git` writes
[SANDBOX-14](#SANDBOX-14) withholds, since none of them reaches that setting. A write row of the
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
`verified-by: bravebot_cli::main::doctor_reports_a_denial_the_backend_cannot_subtract_as_not_in_force`
`verified-by: bravebot_agent::confine::a_refusal_of_the_persons_is_not_lifted_by_the_scope_a_stage_carries`
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
`verified-by: bravebot_ui_bridge::rules::the_filesystem_lists_are_reported_with_the_file_and_what_became_of_each`

<a id="SANDBOX-26"></a>
### SANDBOX-26: a `run` call may ask for a credential scope or a toolchain list by name, and the person is asked every time

`run` takes an optional `scopes`: an array of names from the fixed menu `remote`, `aws`,
`kubernetes`, `docker`, `cargo`, `node`, `python`, `go`, `maven` and `gradle`. It is for the line
whose argv shows no operation to key a scope on ([SANDBOX-16](#SANDBOX-16)), a script that runs
`gh` or `cargo` inside it. Each name is the scope or toolchain list of the same name, with the rows,
the egress under a closed network ([SANDBOX-20](#SANDBOX-20)) and the ssh agent socket that scope
has when a stage's argv names it, and the places the environment moves its tool's configuration to
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
- Accepted in `strict` and `standard` ([SANDBOX-22](#SANDBOX-22)). `standard` reads the machine
  except the credential table ([SANDBOX-12](#SANDBOX-12)), so a scope there lifts the part of that table
  it names.
- Refused, with one sentence that does not say which setting withheld it, in a session whose mode is
  `off`, which has no profile to add to, with no profile directory, and in a workspace the person
  has not trusted ([TRUST-7](trust-map.md#TRUST-7)).
  A run with nobody to ask refuses it, as it refuses any question; the mode that asks nothing
  approves it, as it approves any run.
- A stage with a `NAME=value` in front of it gets no requested scope, for the reason it gets no
  carried one. A person's deny rules still beat the rows it adds ([SANDBOX-25](#SANDBOX-25)).
- The trail records the names and the stage each was added to under `requested_scopes`, built from
  menu words and stage numbers and nothing the plan chose. The desktop sends the names as
  `requestedScopes`, since it draws no confinement, and offers no standing answer for such a line.
- A delegate's `run` has the same argument, the same refusals and the same question.

**Why.** Scopes follow the operation an argv names, so a script that shells out to `gh` reaches
nothing, fails, and leaves the planner with a refusal it cannot tell from a fault. The remedy is to
let it say what it needs, and to put that to the person in the plan they read, since a name the
planner chose is a credential lent to a program a checkout may have written. Asking every time and
trusting no output is what keeps a name from becoming a standing allowance that the person approved
for a different line.

`verified-by: bravebot_sandbox::scope::a_request_is_a_word_of_the_menu_and_nothing_near_it`
`verified-by: bravebot_sandbox::scope::the_menu_is_every_scope_and_every_toolchain`
`verified-by: bravebot_core::policy::a_vouched_line_that_asks_for_a_scope_is_asked_about_anyway`
`verified-by: bravebot_core::policy::a_remembered_line_that_asks_for_a_scope_is_asked_about_anyway`
`verified-by: bravebot_core::policy::a_requested_scope_leaves_a_trail`
`verified-by: bravebot_agent::confine::a_requested_scope_lifts_its_own_directory_for_a_stage_that_names_none`
`verified-by: bravebot_agent::confine::a_requested_toolchain_brings_its_caches`
`verified-by: bravebot_agent::confine::a_stage_with_an_assignment_gets_no_requested_scope`
`verified-by: bravebot_agent::confine::a_requested_scope_or_toolchain_keeps_a_closed_network_for_its_stage`
`verified-by: bravebot_agent::confine::a_request_is_kept_once_in_the_menus_order`
`verified-by: bravebot_agent::confine::the_description_names_each_requested_scope_and_the_stage_it_is_for`
`verified-by: bravebot_agent::confine::a_strict_stage_that_asked_for_a_scope_reads_that_credential_only`
`verified-by: bravebot_agent::tools::run_takes_one_command_line_and_nothing_else`
`verified-by: bravebot_agent::turn::scopes_are_asked_about_on_a_vouched_line`
`verified-by: bravebot_agent::turn::a_standing_answer_to_a_request_remembers_nothing`
`verified-by: bravebot_agent::turn::scopes_are_refused_under_off`
`verified-by: bravebot_agent::turn::scopes_are_asked_about_and_run_under_standard`
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
- A toolchain list is never remembered, whether it was asked for alone or beside a scope. The prompt
  says so.
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
`verified-by: bravebot_agent::turn::keeping_a_request_writes_nothing_where_it_was_not_offered_or_was_refused`
`verified-by: bravebot_tui::confirm::the_keep_keys_approve_and_remember_the_reach_only`
`verified-by: bravebot_tui::confirm::the_keep_keys_are_unbound_where_the_prompt_does_not_offer_them`
`verified-by: bravebot_tui::confirm::the_keep_keys_wait_for_the_rows_saying_what_they_remember`
`verified-by: bravebot_tui::confirm::a_prompt_offering_to_keep_a_request_names_what_would_be_kept`

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
  `~/.bravebot` and anything inside either, a credential location of the table
  ([SANDBOX-12](#SANDBOX-12)) and any directory that holds one (`~/.config` holds
  `~/.config/gcloud`), any path holding a control character, and any path holding `*` or `?`. The path is judged where it leads,
  through its links. A refused request is not put to the person, and the result says that no answer
  would change it. A path that does not exist is not asked about either.
- **The person's own refusals still hold.** A `denyRead` or `denyWrite` entry covering the path
  refuses the request, and one narrower than a granted path takes its part of the grant back, since
  the person's rows are applied after the session's.
- **It is reach and not trust.** Nothing is marked trusted ([TRUST-9](trust-map.md#TRUST-9)) and no
  file is written; the grant is held by the session and gone with it. Asking again for a path
  already held is asked like the first request, and a yes only ever widens the grant held: a read
  answered beside a write held leaves the write. The person may end a grant with
  `/reach paths remove <number>`.
- **Not accepted** where the mode is `off`, which has no profile to add to, and in a workspace that
  is not trusted ([TRUST-7](trust-map.md#TRUST-7)). The result says so and nothing is asked. No
  settings layer, project file or permission rule grants one in advance, because none names it.
- **Shown and recorded.** `/status` lists each path held, with its access, and the
  trace carries one `path_reach` record for each yes, naming the path and the access.
- **Not delegated.** A delegate is offered no `request_path`, and one it names anyway is answered
  as an unknown name ([DELEGATE-12](delegation.md#DELEGATE-12)), since a path asked for from inside
  a sub-task is reach the person never set up.
- The desktop shows no card for it and refuses, which grants nothing.

**Why.** A stage that cannot write a path the work needs fails with `Operation not permitted`, and
the planner has no way to say which path it needed. Without a per-path request, the only way to let
a build write one file is `/add-dir`, which marks the whole directory trusted. A request for one
path, with the same refusals as the rows a person can write, lets them say yes to that path for a
single session. What the planner supplies decides nothing: the path is routing and is judged by the
rules, and the reason is drawn for the person and recorded.

`verified-by: bravebot_sandbox::rules::a_request_is_refused_where_an_allow_write_entry_is`
`verified-by: bravebot_sandbox::rules::a_request_at_inside_or_above_a_credential_location_is_refused`
`verified-by: bravebot_sandbox::rules::a_request_for_an_existing_path_is_kept_and_a_link_is_judged_where_it_leads`
`verified-by: bravebot_sandbox::rules::a_request_for_a_name_that_is_not_utf8_is_kept_as_those_bytes`
`verified-by: bravebot_sandbox::rules::a_request_under_a_denial_of_the_person_is_refused`
`verified-by: bravebot_sandbox::rules::granted_rules_add_rows_and_a_denial_of_the_person_still_wins`
`verified-by: bravebot_agent::confine::a_path_the_person_let_programs_reach_is_held_and_their_own_deny_still_applies`
`verified-by: bravebot_agent::workspace::path_reach_is_numbered_shared_upgraded_and_ended`
`verified-by: bravebot_agent::workspace::a_new_workspace_holds_no_path_reach`
`verified-by: bravebot_agent::reach::reach_paths_lists_ends_and_leaves_other_words_alone`
`verified-by: bravebot_agent::tools::request_path_is_offered_with_run_and_takes_a_path_a_flag_and_a_reason`
`verified-by: bravebot_agent::tools::a_delegate_that_names_request_path_is_told_no_such_tool_and_nobody_is_asked`
`verified-by: bravebot_agent::turn::a_yes_to_a_path_lets_a_program_write_it_and_a_read_only_yes_does_not`
`verified-by: bravebot_agent::turn::a_yes_to_a_path_marks_nothing_trusted_and_is_recorded`
`verified-by: bravebot_agent::turn::a_path_that_is_refused_as_a_row_is_refused_as_a_request_and_not_asked`
`verified-by: bravebot_agent::turn::a_path_is_not_asked_for_under_off_or_in_an_untrusted_workspace`
`verified-by: bravebot_agent::turn::a_path_needs_a_yes_from_the_person_or_the_mode_that_asks_nothing`
`verified-by: bravebot_agent::turn::the_tool_is_offered_where_runs_are_confined`
`verified-by: bravebot_tui::confirm::a_path_prompt_shows_the_path_the_access_the_reason_and_what_a_yes_does_not_do`
`verified-by: bravebot_tui::confirm::a_path_longer_than_the_box_takes_no_yes_until_the_end_of_it_has_been_drawn`
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
reason ([SANDBOX-28](#SANDBOX-28), [SANDBOX-25](#SANDBOX-25)). A value that is not a boolean is an
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
  ask refuses it.
- **Refused, with one sentence that does not say which setting withheld it,** in `strict`, under
  `off`, in a workspace the person has not trusted ([TRUST-7](trust-map.md#TRUST-7)), under a
  managed `run.network` of `closed`, for the reason the managed file floors `off` there
  ([SANDBOX-22](#SANDBOX-22)), and in a delegate's `run`, since a sub-task asking to run unconfined
  is reach the person never set up. A managed `sandbox.mode` of `strict` is the mode `strict`.
  The sentence does not suggest the argument: the planner learns it from the `run` description.
- **What it prints is quarantined** as an unvouched line's output is ([RUN-4](tools/run.md#RUN-4)).
- **Recorded.** The `sandbox` gate's detail names `unconfined` for that run, where it names the
  session's mode for every other ([TRACE-1](trace.md#TRACE-1)).
- The desktop draws no card for it and refuses, since a window reads `off` as `standard` and has
  no line showing that a program is unconfined ([SANDBOX-22](#SANDBOX-22)).

**Why.** Without it the only way to run `docker login` or `aws sso login` from a session is to start
it with `--sandbox off`, which removes the profile from every program for the whole session. The
routing field is the boolean beside the command line the person already reads, and "run this line
with no sandbox" is a sentence a person can approve on its own. Asking every time and remembering
nothing keeps the request from becoming a standing allowance approved for a different line.

`verified-by: bravebot_agent::tools::an_unconfined_request_is_accepted_only_where_every_condition_holds`
`verified-by: bravebot_agent::tools::only_a_managed_closed_network_refuses_an_unconfined_request`
`verified-by: bravebot_agent::tools::run_takes_one_command_line_and_nothing_else`
`verified-by: bravebot_agent::turn::an_approved_unconfined_line_runs_with_no_profile_and_the_next_line_is_confined_again`
`verified-by: bravebot_agent::turn::an_unconfined_line_is_asked_about_on_a_vouched_line_and_remembers_nothing`
`verified-by: bravebot_agent::turn::the_mode_that_asks_nothing_does_not_approve_an_unconfined_line`
`verified-by: bravebot_agent::turn::an_unconfined_line_is_refused_where_the_session_does_not_accept_it`
`verified-by: bravebot_agent::turn::an_unconfined_value_that_is_not_a_boolean_is_an_error`
`verified-by: bravebot_tui::confirm::a_run_asking_to_be_unconfined_says_so_and_binds_no_lasting_key`
`verified-by: bravebot_ui_bridge::refusal::the_window_refuses_an_unconfined_run_without_asking`

<a id="SANDBOX-30"></a>
### SANDBOX-30: a program bravebot starts itself is never one a confined stage could have written

The programs bravebot starts on its own account, unconfined and with the person's access to `~/.ssh`,
`~/.aws` and `~/.bravebot`, are the AWS CLI (`aws`), the clipboard tools, `osascript`, and the
inhibitor `/caffeinate` starts. Each is resolved to an absolute path before it is started, from the
absolute entries of `PATH` that lie outside every directory a confined stage may write.

- **Those directories are the session's.** Each directory the session was opened on, added, or
  moved to, its scratch directory, the system temporary directory, and every row a stage's policy
  grants writes in ([SANDBOX-18](#SANDBOX-18)), whatever granted it. A directory is added to the
  set when it is opened or first granted, and is never removed from it, because closing a
  directory does not remove a file already written there.
- **A link is judged where it leads.** A `PATH` entry that links into one of them is left out,
  and so is a file in another directory that links into one.
- **An entry that is not absolute is not searched,** and neither is an empty one, which a shell
  reads as the current directory. A name carrying a separator is not looked up, and an absolute
  path is started as given.
- **Nothing found is not installed.** A person whose only `aws` is in such a directory, an
  activated virtualenv's `bin` among them, is told it is not installed
  (Known costs in [backends.md](backends.md#known-costs)); the clipboard tool reads as absent and the next tool is
  tried.
- **The stage's grants are not narrowed.** Withholding `PATH` directories from the write grants
  would stop `pip install` in an activated environment, which is what the grants are for.

**Why.** A stage can write `.venv/bin/aws`, and an activated environment puts that directory first
on `PATH`. The name `aws` then resolves to the stage's file, which bravebot starts outside any
confinement ([CRED-14](credential-protection.md#CRED-14)): the stage has chosen what runs with
the person's access, which the confinement exists to deny it.

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
`verified-by: bravebot_sandbox::programs::a_registered_directory_is_kept_out_of_the_search`
`verified-by: bravebot_sandbox::programs::an_absolute_path_is_the_callers_own_choice`
`verified-by: bravebot_bedrock::credentials::an_aws_a_confined_stage_wrote_at_the_front_of_path_is_not_the_one_run`
`verified-by: bravebot_bedrock::credentials::an_aws_only_in_a_directory_a_stage_may_write_is_reported_as_not_installed`
`verified-by: bravebot_agent::workspace::every_directory_a_stage_may_write_is_kept_out_of_the_search_for_programs`
`verified-by: bravebot_agent::confine::a_directory_a_policy_grants_writes_in_is_kept_out_of_the_search_for_programs`
`verified-by: bravebot_tui::clipboard::a_copy_tool_found_only_in_a_directory_a_stage_may_write_is_not_started`
`verified-by: bravebot_tui::clipboard::a_copy_tool_installed_outside_every_writable_directory_is_started`
`verified-by: bravebot_tui::clipboard::a_paste_tool_found_only_in_a_directory_a_stage_may_write_is_not_started`
`verified-by: bravebot_tui::clipboard::a_paste_tool_installed_outside_every_writable_directory_is_read`
`verified-by: bravebot_tui::clipboard::the_clipboard_entry_points_start_no_tool_found_only_in_a_directory_a_stage_may_write`

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
readable and the table is what the account would not want a script to read at all. The rest of this
section describes the keyed lists, which are what a stage on Windows and an MCP server start from.

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

On Windows the base holds the last two rows only, since a container reads the system directories without an entry of its own.

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

**A toolchain and its caches are the list a program brings.** The paths a build resolves through
belong to that build rather than to every program that runs, so they are keyed on the resolved
binary a stage of the plan names and are shown in the prompt with the rest of the plan. An `npm ci`
is held to the npm rows and a `cargo build` in the same session to the cargo rows, so a postinstall
script cannot leave something in `~/.cargo/registry` for a later `cargo build` to read. The key is
the binary the plan already resolved and a person already read, never a name the model supplied and
never a value a configuration file holds.

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
ecosystem's cache for a later build in that ecosystem to read. Holding that to one ecosystem is what
keying on the binary buys, since the write a `cargo build` is trusted with is one an `npm ci` never
receives. An install is read-only for the opposite reason. A build that would install a toolchain
fails, and a line somebody then runs unconfined costs less than letting a confined stage replace the
`cargo` or the `node` a later stage in the same pipeline resolves to. `npx` meets that rule: a
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
rather than through the directory they sit in. `$HOME` itself is in no row, so a file in it that no
row names, `~/.npmrc` and `~/.pypirc` among them, is unreachable. What this costs is what those
files say: npm passes over a configuration file it cannot read, so a registry or a proxy set in
`~/.npmrc` is not in force for a confined stage, and naming the file is a person's to do. The same
holds for `~/.m2/settings.xml` and `~/.gradle/gradle.properties`, which hold a credential of their
own. Nor is Gradle's `~/.gradle/daemon` in a row: the registry there is how a client finds a daemon
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

**A command no list knows is asked about, and the answer lasts the session.** Partly built: a
stage no list knows runs under the base and its plan, and a person who knows what it needs types
`/reach` ([SANDBOX-23](#SANDBOX-23)) to attach a scope or a directory to it, for the session or
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
names is a write row for a stage that carries the remote scope and for no other
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
refused rather than adopted. The entry left behind
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
  policy row for a connect alone would narrow it. The row has not been exercised against a running
  agent.
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
- Subprocess denial has no mechanism on Windows or on Linux. A container bounds what a process
  reaches rather than whether it creates children, and a child of a confined process is inside the
  same container rather than outside it, so a policy asking for that denial is refused on both
  rather than applied without it ([SANDBOX-5](#SANDBOX-5)). What it costs is that the two
  platforms confine nothing for a caller whose policy wants a program to have no children, where
  Seatbelt applies one.
- A macOS developer directory outside the two places the platform installs one, an Xcode kept
  under the home or in a folder inside `/Applications`, is in no row, so on that machine every
  `/usr/bin` developer shim is refused under the base. A row for it names a directory of the
  person's on the word of a setting, and nothing has decided that yet.
- A stage's program directory and the `PATH` directories outside the home are read by every stage
  ([SANDBOX-18](#SANDBOX-18)), so a program installed under `/opt/homebrew` or `/usr/local` starts.
  That makes every directory on a person's `PATH` outside the home readable to every stage, which is
  wider than the stage's own program needs and is the price of a runner finding the interpreter it
  names.
- A `run` argument that names a path outside the directories the session was opened on is refused
  ([SANDBOX-17](#SANDBOX-17)). Whether the compiler should read such an argument as a path and put
  it in the plan is not settled.
- Seatbelt profiles here allow every `mach-lookup`, so the keychain service is reachable from every
  stage and not only from one carrying the remote scope. A profile holding the keychain to that
  scope has to name the service instead, the same step the socket above needs. Since the base reads
  the login keychain file, a lookup through that service from any stage can reach the login
  keychain's database.
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
  listener run on Unix only, so whether a container reaches a loopback listener, and with it the
  denial of egress on Windows, is shown by the capability the token is built with and not by a
  connection.
