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
  - crates/sandbox/src/scope.rs
  - crates/agent/src/confine.rs
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
([commands.md](commands.md#CMD-12)), so it is not confined.

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

The one place a program starts without a backend is a platform with no base to build a profile on,
which is Windows: a program `run` starts there is not confined until a base exists
([SANDBOX-17](#SANDBOX-17)). No other caller is carved out, and on Linux and macOS a program `run`
starts is refused when the backend will not apply its profile.

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
that is not in force.

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
nothing otherwise.

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

The rows a confined program reaches before its own plan is read are the same for every program
and are decided in code: what a dynamic executable needs in order to start, the temporary
directory the session resolved as it opened, on macOS the developer directory resolved with it,
and the git configuration a stage reads for an identity. Nothing a program prints, no value a model
supplied, no argument vector and no configuration file adds a row, and the developer directory is
granted only where the platform installs one. The home directory is in no row, no directory a
credential sits in is in one, and the only paths granted for writing are the temporary directory
and the null device. Egress and children are left where they were, since what the base bounds is
the filesystem. A platform whose prelude is not written down has no base, and so nothing to
assemble a profile from.

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
`verified-by: bravebot_sandbox::linux::a_program_starts_under_the_base_this_machine_resolved`
`verified-by: bravebot_sandbox::linux::a_program_under_the_base_can_name_the_account_it_runs_as`
`verified-by: bravebot_sandbox::linux::a_program_under_the_base_reads_the_machine_and_not_a_private_key`
`verified-by: bravebot_sandbox::macos::a_program_linked_against_the_platforms_tls_library_starts_under_the_base`
`verified-by: bravebot_sandbox::macos::a_developer_tool_the_platform_ships_as_a_shim_starts_under_the_base`

<a id="SANDBOX-13"></a>
### SANDBOX-13: a confined process can look at any path, and reads and lists only its grants

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
`GOCACHE` and `XDG_CACHE_HOME` move no row. `run` adds the list for each stage it starts
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
program resolved to `aws`, `kubectl` or `docker`. It carries none where a `NAME=value` assignment is
written in front of it; where a `git` argv names a program for git to run, which is any abbreviation
git accepts of `--upload-pack`, `--receive-pack`, `--exec`, `--template`, `--config` or
`--strategy`, a `-u` or `-c` to `clone`, a `-s` to `pull`, an address holding `::`, or one written
`<scheme>://` whose scheme is not `ssh`, `git`, `file`, `http`, `https`, `ftp`, `ftps`, `git+ssh`
or `ssh+git`, before a `--` or after one; where a `gh` argv holds `--`; and where `kubectl` is
given `--kubeconfig` or `docker` is given `--config`. A stage reaches its own scope and no other.
No scope names a private key or `~/.ssh` as a directory. The remote scope reads `~/.ssh/config`,
`~/.ssh/known_hosts`, the public key at each name ssh looks for by default, `~/.gitconfig`,
`~/.git-credentials`, `~/.config/git/credentials`, `~/.netrc` and `~/.config/gh`, and writes
`~/.ssh/known_hosts` alone, as a file. For a `gh` stage the last row is the directory `gh` itself
would use: `GH_CONFIG_DIR` if the environment the stage starts with sets it, else
`$XDG_CONFIG_HOME/gh`, else `~/.config/gh`, and it is read and never written. That directory is
refused, and the stage keeps `~/.config/gh`, where it is relative or holds `..`, is the home or
above it, is `~/.ssh` or inside it, or is `~/.config`, `~/.cache` or `~/Library`; a link is judged by where it leads. A tool's directory is read and never written. A scope is
added to the policy it is given and takes nothing from it. `run` adds the scope for each stage it
starts ([SANDBOX-18](#SANDBOX-18)).

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
program writes and this row is the location of a file the person's own tool is about to open. Only
the environment the stage starts with counts: an assignment written in front of the line removes
the scope, so a model-written `GH_CONFIG_DIR=` moves nothing.

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
`verified-by: bravebot_sandbox::scope::the_one_row_a_scope_writes_is_the_hosts_ssh_has_verified`
`verified-by: bravebot_sandbox::scope::the_remote_scope_reaches_both_transports`
`verified-by: bravebot_sandbox::scope::a_scope_leaves_the_policy_it_is_added_to_as_it_was`
`verified-by: bravebot_sandbox::scope::gh_reads_the_configuration_directory_its_environment_names`
`verified-by: bravebot_sandbox::scope::a_gh_directory_that_is_too_wide_or_not_a_path_is_refused`
`verified-by: bravebot_sandbox::scope::a_gh_directory_that_is_a_link_is_judged_by_where_it_leads`
`verified-by: bravebot_agent::confine::a_gh_stage_reads_the_configuration_directory_its_environment_names`
`verified-by: bravebot_sandbox::macos::a_remote_stage_reads_what_ssh_reads_and_never_a_private_key`

<a id="SANDBOX-17"></a>
### SANDBOX-17: a program `run` starts is started under its plan's profile, or not started

On Linux and macOS every stage of a plan `run` starts is started under the profile
[SANDBOX-18](#SANDBOX-18) composes for it: each stage of a pipeline, a stage of a line left running
in the background, and a stage of a plan a subagent runs. The profile is applied after the stage's
environment is set and scrubbed and before its standard streams are connected, so it is the process
that reads its arguments that is confined. A stage the platform cannot confine, because the backend
is unavailable or will not apply the policy, is not started, the pipeline's other stages are
stopped, and the turn is told the program was not started since it could not be confined. No setting
turns this off.

Confinement is a property of the session and not of the plan: the terminal, desktop, plain and
one-shot front ends each ask for it when they open a session. A caller that does not ask, which is a
test driving the executor directly, starts stages as the user's own shell would.

A platform with no base to build a profile on starts a stage unconfined, as the user's own shell would. That is Windows, where
the decision is to confine Linux and macOS first and leave coverage of the third to
[brave/bravebot#1632](https://github.com/brave/bravebot/issues/1632). The carve-out is the absence
of a base and no other reason: a platform that has one and cannot apply a policy refuses.

**Why.** A profile that is applied when it can be and skipped when it cannot is the silent
degradation [SANDBOX-1](#SANDBOX-1) exists to forbid, and the person who endorsed a line believes it
ran under what the plan accounts for. An off switch would be the setting a person reaches for after
one refusal and forgets, and every session after it would run unconfined. Starting the
process already confined, through a command the caller spawns, keeps the stage's pipes and process
group its own, so the executor's cancellation and job handling are unchanged.

**What it costs.** A program argument that names a path outside the directories the session was
opened on is refused by the kernel, since only redirections are opened by this process on the
stage's behalf. A `cat ~/notes.txt` fails, and a person adds the directory
([trust-map.md](trust-map.md)). On macOS the refusal of a `.git` write is lifted for these stages
([SANDBOX-14](#SANDBOX-14)), which gives them the reach Linux gives.

`verified-by: bravebot_agent::confine::a_confined_program_cannot_read_a_file_outside_the_session`
`verified-by: bravebot_agent::confine::a_confined_program_reads_and_writes_inside_the_session`
`verified-by: bravebot_agent::confine::a_confined_program_cannot_write_outside_the_session`
`verified-by: bravebot_agent::confine::a_program_left_running_is_confined_as_well`
`verified-by: bravebot_agent::confine::a_stage_that_cannot_be_confined_is_refused_with_no_stage_left_running`
`verified-by: bravebot_agent::confine::a_job_with_a_stage_that_cannot_be_confined_is_refused_with_no_stage_left_running`
`verified-by: bravebot_agent::turn::a_delegate_of_a_confining_turn_cannot_write_outside_the_session`
`verified-by: bravebot_agent::tools::a_turn_that_confines_runs_is_confined_to_its_workspace_and_a_turn_that_does_not_is_not`
`verified-by: bravebot_sandbox::linux::a_command_handed_back_is_confined_when_the_caller_spawns_it`
`verified-by: bravebot_sandbox::macos::a_command_handed_back_is_confined_when_the_caller_spawns_it`

<a id="SANDBOX-18"></a>
### SANDBOX-18: the profile a stage runs under is composed from the plan and the session's directories

A stage's policy is the fixed base ([SANDBOX-12](#SANDBOX-12)), with `.git` writes allowed, plus:
the toolchain list its resolved binary brings ([SANDBOX-15](#SANDBOX-15)); the credential scope its
argv names ([SANDBOX-16](#SANDBOX-16)); the directories its program is read from, which are the
directories on the `PATH` it starts with and the directory it resolved into, each with its links
followed, the parent of a `bin` directory among them, and none that is the home directory, above it
or a parent inside it; the two files the step names as read, the one it was started as and the one
it resolved to; each directory the session was opened on, to read and write; the session's scratch
directory, to read and write; and the plan's directory as the place it starts. Where the stage
carries the remote scope, the socket `SSH_AUTH_SOCK` names in its own environment is a write row,
since a socket is reached through a write ([SANDBOX-3](#SANDBOX-3)). Nothing else is in it: no
credential directory, no directory above a session directory, and no socket for a stage without the
remote scope.

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
`verified-by: bravebot_agent::confine::a_step_whose_plan_names_no_credential_reaches_nothing_in_the_home`
`verified-by: bravebot_agent::confine::a_toolchains_cache_is_granted_to_its_own_binary_only`
`verified-by: bravebot_agent::confine::a_push_reaches_the_remote_scope_and_a_status_does_not`
`verified-by: bravebot_agent::confine::the_agent_socket_goes_to_a_remote_step_and_to_no_other`
`verified-by: bravebot_agent::confine::an_assignment_in_front_of_a_push_removes_its_scope`
`verified-by: bravebot_agent::confine::a_program_at_the_top_of_the_home_is_granted_as_a_file_and_not_as_the_home`
`verified-by: bravebot_agent::confine::the_path_outside_the_home_is_read_and_the_homes_own_bin_brings_no_parent`

## Programs a person asked for

A program `run` ([tools/run.md](tools/run.md)) starts is confined on Linux and macOS
([SANDBOX-17](#SANDBOX-17), [SANDBOX-18](#SANDBOX-18)), and the clauses above are what the profile
is held to. What confinement bounds is the filesystem: a program is held to the paths the plan a
person endorsed accounts for, and not to whatever else it could open. The programs somebody might
ask for cannot be listed in advance, and a `git push` needs the credentials under `~/.ssh`, so the
remote scope above is what lends those to a stage whose argv names the operation. Windows has no
base yet, so a program there is unconfined. Each part of the decision that is not built is marked
where it appears.

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

**A command no list knows is asked about, and the answer lasts the session.** Not built: a stage no
list knows runs under the base and its plan and nothing asks about it, so a build inside it that
needs a cache fails and stays failed. The rest of this paragraph is the decision. A wrapper is the
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
`~/.aws`, and no row reaches a private key, `~/.ssh` as a directory, or the keychain database on
disk. The remote scope is `~/.ssh/config` and `~/.ssh/known_hosts` to read with `known_hosts` also
to write, that write row naming a file rather than a directory so that an account with no
`known_hosts` gets one rather than a push that fails ([SANDBOX-11](#SANDBOX-11)), the public key at
each name ssh looks for by default, `~/.gitconfig`, and the stores an https helper reads:
`~/.git-credentials`, `~/.config/git/credentials`, `~/.netrc` and `~/.config/gh`. The login keychain
is in no row and is reached through the system service that holds it, whatever scope a stage
carries (the last section's list of what has to exist first). The agent socket `$SSH_AUTH_SOCK`
names is a write row for a stage that carries the remote scope and for no other
([SANDBOX-18](#SANDBOX-18)), and on macOS it is reached only while the profile leaves egress open. On macOS, where the backend creates
nothing, the file is made by ssh, which can do so only into a `~/.ssh` already there, so an account
without one records no host a confined push meets. A push signs through the agent and needs no
private key, and the public half is in the scope because that is what ssh reads to name an identity
to the agent where a configuration file pins one. What that costs is a key kept at a name of a
person's own: ssh without `IdentitiesOnly` offers every key the agent holds and signs anyway, and a
host pinned with `IdentitiesOnly` to such a key leaves ssh neither half to name it by. A push does
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
`~/.npmrc` among them, still needs somebody to name a directory: `/add-dir` in a session,
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

**The network stays open to it.** A profile gates egress as a whole, so it cannot tell an approved
`git push` or `gh api` from an exfiltration, and the endorsed argv already can. What confinement
narrows is what a program may read and write, not what it may send: a confined one still sends
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
- What a program needs in order to start on Windows is not written down, so there is no base
  there and nothing to assemble a profile from. The rows above are the Unix ones, and what a
  Windows base has to settle first is whether a container reaches the system directories through
  an access entry the platform already wrote, or whether the base names them and every run writes
  an entry of its own onto a directory of the machine's. Until it does, a program `run` starts there is
  unconfined ([SANDBOX-17](#SANDBOX-17)).
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
  scope has to name the service instead, the same step the socket above needs.
- The cold path of a macOS developer shim has not been exercised. With the lookup cache the shims
  keep empty, a shim asks `xcodebuild`, which refuses every invocation until the Xcode licence
  is accepted, confined or not, so a machine in that state cannot show whether that path starts,
  and the kernel test runs with the cache the account already has. That cache is kept in the
  account's own temporary directory, which is the session's only while `TMPDIR` names it. On Linux
  the kernel tests start `true`, `id` and `cat` under the base, and no TLS program or compiler.
- No Maven or Gradle build has been run under its list. Each reads a settings file no list names,
  and whether a build on a machine that holds one runs without it or stops is not settled.
- The suite does not run on Windows. The decisions this backend makes before a process starts are
  pure and are run by every job that runs the suite: which capability a policy asks for, what each
  grant permits, which policies are refused, and how an argument is written onto a command line.
  The Win32 calls that apply them are compiled and linted by the
  `x86_64-pc-windows-gnu` clippy job and run by nothing, so [SANDBOX-3](#SANDBOX-3)'s guarantee
  there is argued rather than exercised.
