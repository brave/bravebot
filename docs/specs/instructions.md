---
id: INSTR
title: Resolving standing instructions
status: normative
governs:
  - crates/agent/src/preamble.rs
  - crates/agent/src/agents.rs
  - crates/agent/src/home.rs
documented-by: docs/website/docs/customize/instructions.md
---

## Scope

Before the planner is asked anything, its context is pre-filled with instructions nobody typed
this turn: `AGENTS.md`, which says how work is done somewhere, and the name and description of
every skill on offer. This spec is about **resolution**: which files are looked for, where, in
what order, and where what they say ends up.

It does not cover what a skill file looks like or what any source is trusted for, which is
[skills.md](skills.md), nor what a label means once assigned, which is [labels.md](labels.md).

## The sources

<a id="INSTR-1"></a>
### INSTR-1: ten sources kept as files, and no others

| File | Applies to |
|---|---|
| `~/.bravebot/AGENTS.md` | every project |
| `~/.bravebot/skills/<name>/SKILL.md` | every project |
| `~/.bravebot/agents/<name>.md` | every project |
| `<workspace>/AGENTS.md`, else `CLAUDE.md`, else `.claude/CLAUDE.md` | this project |
| `<workspace>/.bravebot/skills/<name>/SKILL.md` | this project |
| `<workspace>/.claude/skills/<name>/SKILL.md` | this project |
| `<workspace>/.agents/skills/<name>/SKILL.md` | this project |
| `<workspace>/.bravebot/agents/<name>.md` | this project |
| `<workspace>/<directory>/AGENTS.md` | this project, once the session has worked in that directory |
| `<workspace>/<directory>/.bravebot/skills/<name>/SKILL.md` | this project, once the session has worked in that directory |

The two roots are spelled differently on purpose: the user's own directory is already `.bravebot`,
so its skills and definitions sit directly beneath it, while a project keeps its own out of the
way in a dotted directory rather than at the root where `AGENTS.md` sits.

`.claude/skills` and `.agents/skills` are read for the same reason `CLAUDE.md` is: a project that
wrote its skills for another agent should not have them ignored over the spelling, and copying or
symlinking each one into `.bravebot/skills` is work the project should not have to do.
[INSTR-13](#INSTR-13) is their order and the root they are read at. The user's own
`~/.claude/skills` and `~/.agents/skills` are not sources, because [INSTR-2](#INSTR-2) tries no
user directory but `~/.bravebot`.

The command line is a ninth source that is not a file, and [INSTR-10](#INSTR-10) is the whole of it.

A skill is a directory because it has other material to keep beside its instructions. A delegate
definition is one file, so `agents/` is flat: there is nothing for the directory to hold. What a
definition is, and what makes one admissible at all, is
[DELEGATE-19](delegation.md#DELEGATE-19) and [DELEGATE-20](delegation.md#DELEGATE-20).

The project's instructions are looked for under more than one name, in the order above, and the
first that exists is the source. Not all of them: a repository holding two of these holds one set
of instructions under two names, and reading both would state everything twice.

**Why more than one name.** More than one is in use, and a project that wrote its conventions down
should not have them ignored over the spelling. This is still one source, resolved by name.

There is no search of parent directories. A nested file is a source only under
[INSTR-14](#INSTR-14). A file at any other path is an ordinary file, read only when something asks for it by name, or when the source names it,
which is [INSTR-8](#INSTR-8).

A definition's memory, `<workspace>/.bravebot/memory/<name>.md`, is one of those ordinary files.
[MEMORY-4](definition-memory.md#MEMORY-4) has the driver tell a run under the definition where it
is, and puts none of it in the prompt.

**Why no walking upwards.** A rule that walked upwards would pick up instructions from whatever
happened to be above a project on this machine, which is a different set of instructions on the
next machine.

`verified-by: bravebot_agent::preamble::the_home_agents_file_is_read_before_the_project_one`
`verified-by: bravebot_agent::preamble::the_project_file_may_be_named_claude_md`
`verified-by: bravebot_agent::preamble::only_the_first_project_file_that_exists_is_read`
`verified-by: bravebot_agent::skills::a_workspace_skill_shadows_a_home_skill_of_the_same_name`
`verified-by: bravebot_agent::skills::a_skill_in_a_foreign_project_directory_is_offered`
`verified-by: bravebot_agent::agents::a_definition_in_the_users_own_directory_is_selectable`
`verified-by: bravebot_agent::agents::a_definition_in_a_vouched_for_project_is_selectable`

<a id="INSTR-2"></a>
### INSTR-2: `~/.bravebot` is the directory the environment names, and there is no fallback

It is `.bravebot` inside the home directory the environment gives. When there is no home, or the
name is empty, there is no user directory and everything kept there is simply absent. Nothing is
guessed and no other location is tried.

**Why.** A fallback would read instructions from a directory the user never chose, and this is
the one place whose contents are trusted for being the user's own. Daemons and containers run
without a home, and everything kept there is optional, so absence is a case to do without rather
than a reason to refuse to start.

`verified-by: bravebot_agent::home::the_home_directory_is_the_one_the_environment_names`
`verified-by: bravebot_agent::home::an_absent_home_is_not_an_error`
`verified-by: bravebot_agent::home::an_empty_home_is_treated_as_no_home_at_all`
`verified-by: bravebot_agent::skills::no_home_directory_is_not_an_error`

<a id="INSTR-3"></a>
### INSTR-3: only the project's own tree is a source, never a directory opened alongside it

A directory opened by name during a session widens where files may be read from. It adds no
standing instructions and no skills, whatever it contains.

**Why.** Opening a directory to read one file out of it would otherwise change how every later
turn behaves, which is not what the person opening it asked for. The project root is what
relative paths mean and what the session is keyed on, and having one answer to "which project is
this" is what keeps that unambiguous.

`verified-by: bravebot_agent::preamble::an_added_directory_contributes_no_standing_instructions`

## What wins

<a id="INSTR-4"></a>
### INSTR-4: sources are read least specific first, so the project has the last word

The user's own directory is read before the project. Both `AGENTS.md` files are read and both
reach the planner, in that order, and a project skill replaces a global one of the same name.
This is the same "most specific wins" rule the trust map uses for paths.

**Why.** A habit carried between projects should hold until the project says otherwise. Shadowing
by name rather than merging is what lets a project override one skill without restating the rest.

`verified-by: bravebot_agent::preamble::the_home_agents_file_is_read_before_the_project_one`
`verified-by: bravebot_agent::skills::a_workspace_skill_shadows_a_home_skill_of_the_same_name`
`verified-by: bravebot_agent::agents::a_workspace_definition_shadows_a_home_one_of_the_same_name`

<a id="INSTR-5"></a>
### INSTR-5: what is resolved goes into the system prompt, never into the conversation

Standing instructions and the catalogue of skill names are put in front of each request as part
of the system prompt. They are not appended to the stored conversation, so a session running many
turns carries one copy of them however long it runs.

**Why.** The system prompt belongs to the build rather than to the conversation. Sending the same
instructions as a message each turn would accumulate a copy per turn, crowding out the task and
paying for the same text repeatedly, and it would leave the planner reading its own conventions
as though a person had just said them.

`verified-by: bravebot_agent::turn::the_preamble_is_not_stored_in_the_conversation`
`verified-by: bravebot_agent::turn::a_trusted_workspace_agents_file_reaches_the_system_prompt`
`verified-by: bravebot_agent::turn::an_untrusted_workspace_agents_file_never_reaches_the_system_prompt`

<a id="INSTR-6"></a>
### INSTR-6: a source that is not there is not an error

No `AGENTS.md`, no skills directory, no user directory at all: each is the ordinary case, costs
no notice and no refusal, and offers nothing.

**Why.** Nothing is assumed from silence, so an absent source and an empty one say the same thing.
A warning for the common case is a warning people learn to scroll past, and the times a source
really was dropped are the times that has to be read.

`verified-by: bravebot_agent::turn::a_missing_agents_file_is_not_an_error`
`verified-by: bravebot_agent::skills::a_skills_directory_that_does_not_exist_is_not_an_error`

<a id="INSTR-7"></a>
### INSTR-7: the sources are resolved afresh every turn

Discovery runs per turn rather than once at startup. Writing an `AGENTS.md` or a skill mid-session
takes effect on the next turn, including when the agent wrote it itself. There is nothing to
reload and no session to restart.

**Why.** Reading once at startup would make the file just written the one instruction the planner
cannot see, and the fix for that would be to restart, which loses the conversation.

`verified-by: bravebot_agent::preamble::a_file_written_after_one_turn_is_read_by_the_next`

<a id="INSTR-8"></a>
### INSTR-8: an instructions file that only names another one is followed, once

Where the project's instructions are under [`POINTER_BYTES`] and name a markdown file in the
workspace, that file is read instead and is what reaches the planner. Once only: what it names in
turn is not followed.

Length is the whole test. A document is not a pointer however many files it cites, so anything
longer is read as itself and its citations are left alone. An explicit `@path` import is a different signal, and
[INSTR-11](#INSTR-11) expands it.

The pointer is resolved by the same `workspace.read` that governs every other path, so confinement
and the trust map decide whether the named file may be opened. A pointer naming something outside
the workspace is refused there, and an untrusted directory's instructions never reach this rule at
all: they are a notice and no text. A pointer naming a file a `deny` rule covers is read as itself,
the file it names is not read, and the person is told why ([PERM-7](permissions.md#PERM-7)).

**Why.** Repositories that support several agents keep one real document and point the other names
at it. Handed the pointer, a planner spends a call reading what it was about to be given anyway:
a whole round trip, which is the expensive part of a turn, to learn nothing. This was measured: a
project whose `AGENTS.md` read "Refer to canonical agent instructions in `.claude/CLAUDE.md`." cost
exactly that.

`verified-by: bravebot_agent::preamble::a_project_file_that_only_names_another_is_followed`
`verified-by: bravebot_agent::preamble::a_project_file_that_merely_cites_another_is_read_as_itself`
`verified-by: bravebot_agent::preamble::a_pointer_that_names_nothing_readable_leaves_the_file_standing`
`verified-by: bravebot_agent::preamble::a_one_line_file_naming_another_is_a_pointer`
`verified-by: bravebot_agent::preamble::a_document_that_merely_mentions_a_file_is_not_a_pointer`
`verified-by: bravebot_agent::preamble::a_file_pointing_at_itself_is_not_followed`
`verified-by: bravebot_agent::preamble::punctuation_around_the_name_is_not_part_of_it`
`verified-by: bravebot_agent::preamble::a_short_file_naming_nothing_is_not_a_pointer`
`verified-by: bravebot_agent::turn::a_denied_file_an_agents_file_points_at_does_not_reach_the_system_prompt`

<a id="INSTR-9"></a>
### INSTR-9: where the planner is working is stated, and is not read through the trust gate

The system prompt says the working directory, whether the tree is a git repository, whether the
GitHub CLI is on the path, the platform, the OS version, the shell, today's date, and the directory
this session has to itself ([trust-map.md](trust-map.md)) where it has one, with what that directory
is for and the name a program started from a command line reads its path from. A session that has
none has nothing said about one. These are facts about the machine, and the prompt says so.

One of them carries an imperative, and those are the terms on which one is admitted here: gated on
what the probe found, and naming the one case rather than stating a preference. The session's own
directory is stated with what to put in it. The rest are facts alone and nothing is asked of the
planner on their account.

What an installed GitHub CLI is for is said on the same terms, and to the turn a person is watching
rather than in this block: the commands that read a pull request or an issue, with fetching the URL
named as the fallback for a CLI nobody has logged in with. A delegate reads this block word for word
and is offered neither road, no `fetch_url` at all and no `run` without the capability for it
([DELEGATE-4](delegation.md#DELEGATE-4)), so an imperative choosing between them is stated where
those tools are.

They do not pass `read_trusted_content`, and that is the difference between them and every other
source here. There is no file behind any of them. The root is where the user pointed the session,
and the rest comes from the kernel and this process's own environment, which is the provenance
[ROUTE-*](routing.md) relies on for a command the user typed. So there is nothing to vouch for and
no label to refuse. Nothing read out of the workspace may be added to this block, because the whole
argument for skipping the gate is that no source in the tree contributes to it.

Composed per turn like everything else here, so `/cd` ([TRUST-13](trust-map.md#TRUST-13)) is
followed: the next turn states where the session went.

**Why.** Each of these otherwise costs a `run` to discover, and a run costs two prompts, not one:
the plan is approved, and then the output comes back quarantined so the planner has to ask to be
shown it. A planner that does not know its own working directory reaches for `pwd`, which is that
whole exchange for a value the driver has had since startup. The date is stated for a different
reason: a model's sense of it comes from its training and is wrong by however long ago that was.

The session's own directory is stated for a third reason: no `run` could discover it. Nothing names
it but this process, so a planner never told of it has nowhere it knows of to put a file that is not
part of the project and writes one into the project instead, which is the file a build, a commit and
a reviewer each have to deal with. Saying nothing where there is no such directory is the same
argument the other way: a path to a directory that is not there costs a turn the run that finds out.

The GitHub CLI is the one whose fact would change nothing on its own, which is why the imperative
goes with it. A URL arrives, there is a tool for URLs, and taking it fetches a whole page to read a
fraction of it. The saving is the bytes and not the round: unvouched `run` output is `(U,priv)` just
as a fetched body is ([labels.md](labels.md)), so either road spends the round that reads it back
out, and what the page adds is its own size. The `.diff` address adds a hop the approval does not
cover, which [FETCH-4](tools/fetch-url.md#FETCH-4) refuses unless a rule names the host it lands on.
Naming the commands is what makes the fact worth its line, and naming GitHub rather than local tools
in general is what makes it decide anything: a preference that holds for every service is a line on
every turn that settles no case.

The fact is still stated to a delegate, because whether a program is installed is true of the machine
whoever is asking, and a delegate that may run one can use it. What is withheld there is only the
sentence choosing between two tools, one of which a delegate never has: a delegate told to fall back
to `fetch_url` spends a round on a name that resolves to nothing.

`verified-by: bravebot_agent::preamble::the_working_directory_is_stated_so_nothing_has_to_run_pwd`
`verified-by: bravebot_agent::preamble::the_environment_is_stated_even_with_no_instructions_to_read`
`verified-by: bravebot_agent::preamble::whether_the_tree_is_a_git_repository_is_said_either_way`
`verified-by: bravebot_agent::preamble::moving_the_working_directory_restates_it`
`verified-by: bravebot_agent::preamble::the_sessions_own_directory_is_stated_so_a_turn_can_write_in_it`
`verified-by: bravebot_agent::preamble::a_session_with_no_directory_of_its_own_is_told_of_none`
`verified-by: bravebot_agent::preamble::a_windows_version_is_the_three_numbers_a_build_is_named_by`
`verified-by: bravebot_agent::preamble::whether_the_github_cli_is_installed_is_said_either_way`
`verified-by: bravebot_agent::preamble::a_github_url_is_sent_to_the_cli_only_where_the_probe_found_one`
`verified-by: bravebot_agent::preamble::an_installed_github_cli_still_names_fetch_url_as_the_fallback`
`verified-by: bravebot_agent::preamble::the_road_a_github_url_takes_is_not_in_the_block_a_delegate_reads`
`verified-by: bravebot_agent::turn::the_road_for_a_github_url_goes_out_with_the_fact_it_rests_on`

<a id="INSTR-10"></a>
### INSTR-10: words from the command line are the last standing source, and a replacement reaches the opening alone

`--append-system-prompt` ([CLI-19](cli.md#CLI-19)) adds its words to the standing instructions under
the heading "From the command line", after the project's instructions, so the order of INSTR-4
continues one place further and the words have the last word. They are trimmed, and they are in the
text a delegate reads as well as the one a person's turn does.

`--system-prompt` replaces the opening of the system prompt, which is the paragraph saying what kind
of assistant this is, and leaves the rest of the prompt as it is: the instructions for reading a
tool's output, the facts of INSTR-9, the mode, the goal and every source above.

Both are held by the running process and resolved into each turn's system prompt like every other
source, so INSTR-5 holds for them: they are never put in the stored conversation, and a session
running many turns carries one copy. A recorded session stores no system prompt, so a resume
without the flag runs without the words.

They are not read through the trust gate, for the reason INSTR-9's facts are not. There is no file
behind them, and the person who typed them is the one the planner works for. Nothing read out of the
workspace may be added to them. This is also why a file's bytes passed in with
`--append-system-prompt "$(cat notes.md)"` take on the person's authority: the gate was never asked
about the file.

**Why.** A standing instruction a person wants for one run should not have to be written into a file
in the project first. Last place is the most specific source, as INSTR-4 orders them. Only the
opening is replaceable because what follows it is what the other clauses rely on.

`verified-by: bravebot_agent::preamble::words_from_the_command_line_follow_the_projects_file_in_what_a_delegate_reads`
`verified-by: bravebot_agent::turn::appended_words_come_after_the_projects_instructions`
`verified-by: bravebot_agent::turn::a_replaced_opening_takes_the_place_of_the_opening_alone`
`verified-by: bravebot_agent::turn::system_prompt_words_reach_every_turn_and_are_not_stored`
`verified-by: bravebot_agent::turn::a_delegate_reads_the_appended_words_and_not_the_replaced_opening`

<a id="INSTR-11"></a>
### INSTR-11: an `@path` import in the project's instructions is replaced by the file's text

In the project's instructions, after they have passed the trust gate, a whitespace-delimited token
that begins with `@` and names a markdown file is replaced by that file's text, trimmed, where the
token stands. Punctuation that ends the token, as in `@docs/style.md.`, stays after the text. The
path is resolved against the directory of the file the token is written in, and nested imports are
expanded the same way, to four hops from the instructions file. A token inside a code span or a
fenced code block is not an import. A token naming no file, or not a markdown file, is ordinary
text.

Each import is read through the same `workspace.read` and trust gate as [INSTR-8](#INSTR-8)'s
pointer, after a `deny` rule is asked ([PERM-7](permissions.md#PERM-7)), so confinement and the
trust map decide. An import that cannot be expanded stays as written and the person is told which
file and why: a `deny` rule covers it, it is outside the workspace or not trusted, a file above it
is already importing it, or the nesting or the count (64 for one instructions file) is past the
limit. An absolute path, or one reaching above the workspace, is never read and is not reported, because
it names no file the project holds. The same refusal is reported once, however many times the
file is named. An untrusted
directory's instructions never reach this rule ([INSTR-5](#INSTR-5)).

Only the project's file is expanded. `~/.bravebot/AGENTS.md` is not, because its imports would
have to name files outside the workspace.

**Why.** Claude Code and Gemini CLI expand these, so a CLAUDE.md written for either arrives intact
here, and a long file can be split or composed from shared parts. Handed the literal token, a
planner spends a read on it or never reads it. The driver branches only on text that passed the
gate and reads each import through it, so nothing untrusted decides what is read.

`verified-by: bravebot_agent::preamble::an_at_path_import_is_expanded_in_place_and_resolved_beside_its_file`
`verified-by: bravebot_agent::preamble::an_import_nested_past_four_hops_is_left_as_written`
`verified-by: bravebot_agent::preamble::imports_past_the_count_are_left_as_written`
`verified-by: bravebot_agent::preamble::an_import_in_a_code_fence_or_a_code_span_is_not_followed`
`verified-by: bravebot_agent::preamble::an_import_cycle_is_cut_where_it_closes`
`verified-by: bravebot_agent::preamble::an_import_back_to_the_file_a_pointer_was_read_from_is_a_cycle`
`verified-by: bravebot_agent::preamble::a_shorter_fence_inside_a_longer_one_does_not_end_it`
`verified-by: bravebot_agent::preamble::an_import_outside_the_workspace_is_left_as_written`
`verified-by: bravebot_agent::preamble::an_untrusted_project_loads_no_import`
`verified-by: bravebot_agent::turn::a_denied_file_an_agents_file_imports_does_not_reach_the_system_prompt`

<a id="INSTR-12"></a>
### INSTR-12: a built-in output style stands where `--system-prompt` stands, and yields to it

`/style <name>` picks one of the styles this build ships (`concise`, `explanatory` and `proactive`),
`/style off` clears the pick and `/style` alone says which is in force and lists the names. The
pick lasts for the session and is not recorded, so a resumed session starts with none. A name is
compared whole against the built-in list: nothing a person types after `/style` reaches the prompt,
and no style is read from a file.

A style's words stand in for the opening of the system prompt, as `--system-prompt` does
([CLI-19](cli.md#CLI-19), [INSTR-10](#INSTR-10)), and for nothing after it. Where both are present
the words of `--system-prompt` are used and the style's are not, because a person who typed the flag
for this run has given the more specific instruction. Every turn of the session carries the style
in force when it begins, `/loop` ticks and `/goal` rounds included. A delegate is not given one,
and nothing else reads it: not the aside, the summariser, the goal judge, the classifier that vets
a slot, a processor or the planner of a manifest run. A pick made under `--system-prompt` is held and
the person is told it does not show.

A style grants nothing. A write is still put to the person where it would have been, plan mode still
refuses it, and the guidance on reading a tool's output is not in the opening, so no style can
remove it. `proactive` says so in its own words and does not change the mode.

**Why.** Wording for tone and format is a thing a person wants to switch between tasks, and
`--system-prompt` can only be given when the program starts. Fixing the set at build time keeps the
rule INSTR-10 relies on: the words in the opening are written by this program or typed by the person,
and nothing read from a project reaches that place.

**Known costs.** Only the terminal interface has `/style`. Styles are not read from `~/.bravebot/styles` or from a project, there is no
`style` setting, and the choice is not written to the trace. Each needs the trust map or the
settings layers to say who may name a style, and none is built.

`verified-by: bravebot_agent::turn::a_style_takes_the_place_of_the_opening_alone_and_yields_to_system_prompt`
`verified-by: bravebot_agent::styles::a_style_is_found_by_its_whole_name_only`
`verified-by: bravebot_agent::styles::no_style_takes_the_quarantine_guidance_with_it`
`verified-by: bravebot_tui::app::the_style_command_takes_a_name_or_nothing`
`verified-by: bravebot_tui::app::a_prompt_containing_the_style_command_or_a_longer_word_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_style_picked_under_a_replaced_opening_says_it_does_not_show`
`verified-by: bravebot_tui::app::a_style_named_to_the_command_reaches_the_next_turn_and_only_a_known_name_does`

<a id="INSTR-13"></a>
### INSTR-13: a project's skill directories are read least specific first, at the root only

`.agents/skills`, then `.claude/skills`, then `.bravebot/skills`, so a skill in `.bravebot/skills`
shadows one of the same name under either of the others, by the shadowing of
[INSTR-4](#INSTR-4). A project that ships its own version of a ported skill means it.

The project root only. A `.claude/skills` or `.agents/skills` inside a subdirectory is an ordinary
directory and no source, whatever the session has worked in, which is [INSTR-1](#INSTR-1)'s refusal to walk upward read downward as
well: what a turn is advertised would otherwise depend on how deep the checkout is.

Each is read through the trust map like `.bravebot/skills`, so an untrusted project offers none of
them, each directory is checked for trust before it is enumerated at all, and what was skipped is
counted with the directory it was skipped from named and no skill inside it
([SKILL-4](skills.md#SKILL-4), [SKILL-6](skills.md#SKILL-6)).

The count is what a more specific root did not go on to offer. A skill of the same name under a
vouched root is on the list a person can see, so counting it as not loaded would contradict what is
in front of them. A root whose every skill was covered that way is not reported at all.

**Why not the user's own `~/.claude/skills` and `~/.agents/skills`.** [INSTR-2](#INSTR-2) tries no
user directory but `~/.bravebot`, and reading instructions from a directory the person never chose
here is the fallback that clause refuses. Adding them is a separate decision and is not settled
here.

`verified-by: bravebot_agent::skills::a_bravebot_skill_shadows_a_foreign_one_of_the_same_name`
`verified-by: bravebot_agent::skills::a_foreign_skills_directory_below_the_root_is_not_a_source`
`verified-by: bravebot_agent::skills::a_foreign_skill_in_an_untrusted_project_is_counted_and_not_named`
`verified-by: bravebot_agent::skills::a_skill_a_vouched_root_offers_is_not_also_reported_as_not_loaded`

<a id="INSTR-14"></a>
### INSTR-14: a directory the session has worked in adds its own `AGENTS.md` and `.bravebot/skills`

For each directory below the project root that a file tool has read from or written to in this
session, and each directory above it down to the root's own children, the `AGENTS.md` in it is a
source after the project's, and so is `.bravebot/skills` in it. Shallower directories come first and
a deeper one last, so the most specific has the last word ([INSTR-4](#INSTR-4)). A skill of the
same name replaces the project's, and a shallower directory's.

Which directories is the driver's record of the names the planner typed to `read_file` and to the
writing tools, placed by spelling alone, so a link is not followed to where it lands and nothing a
file holds is consulted. A name outside the root records nothing, and the record keeps at most 64
directories. What is recorded is the planner's choice of paths and is never read out of a file.

Each file is read through the trust map and the `deny` rules as the project's `AGENTS.md` and
`.bravebot/skills` are, so a distrusted one never reaches the prompt and the person is told which
file, or which skills directory with a count, and never what it holds ([SKILL-4](skills.md#SKILL-4),
[SKILL-6](skills.md#SKILL-6)). A nested `AGENTS.md` is taken as it stands: a pointer
([INSTR-8](#INSTR-8)) and an `@path` import ([INSTR-11](#INSTR-11)) in it are ordinary text. Only
`.bravebot/skills` is read below the root, as [INSTR-13](#INSTR-13) says. A safe session reads none.

Composed per turn like every other source ([INSTR-7](#INSTR-7)): a file read in a turn takes effect
from the next turn, and the turn that read it keeps the instructions it started with. The record is
the session's, so it is empty again in a resumed session and after `/cd`.

**Why.** A package in a monorepo has conventions the project's root file cannot state without
every turn carrying every package's. Reading downward only into directories the planner chose, and
only through the trust map, keeps what [INSTR-1](#INSTR-1) refused when it refused to walk upward:
what a turn is told does not depend on what sits above the project, and nothing untrusted decides
which file is read.

`verified-by: bravebot_agent::preamble::a_nested_agents_file_is_in_the_prompt_only_after_a_path_under_it_is_touched`
`verified-by: bravebot_agent::preamble::nested_agents_files_follow_the_root_one_and_the_deeper_is_last`
`verified-by: bravebot_agent::preamble::a_distrusted_nested_agents_file_never_reaches_the_prompt`
`verified-by: bravebot_agent::preamble::a_nested_agents_file_a_deny_rule_covers_never_reaches_the_prompt`
`verified-by: bravebot_agent::preamble::a_path_outside_the_root_does_not_make_its_directory_a_source`
`verified-by: bravebot_agent::skills::a_nested_skill_is_offered_after_the_session_works_in_its_directory`
`verified-by: bravebot_agent::skills::a_distrusted_nested_skill_is_counted_and_not_named`
`verified-by: bravebot_agent::turn::a_nested_agents_file_reaches_the_turn_after_a_file_beside_it_is_read`
`verified-by: bravebot_agent::turn::a_nested_agents_file_reaches_the_turn_after_a_file_beside_it_is_written`
`verified-by: bravebot_agent::workspace::moving_the_working_directory_forgets_the_directories_worked_in`
`verified-by: bravebot_agent::safe::a_safe_session_offers_no_nested_skill`
`verified-by: bravebot_agent::safe::a_safe_session_reads_no_nested_agents_file`

## Known costs

Accepted deliberately. Do not "fix" one without changing this spec first.

- **Each recorded directory costs one existence check and one directory listing every turn**, up
  to the 64 the record keeps. There is no cache, because one would have to be invalidated when a
  file appears or changes mid-session. A directory only listed, searched or named in a shell
  command is not recorded: only a read or a write is.
- **Resolution costs up to three directory listings and up to three file reads every turn.** Cheap
  next to the model call it precedes, and the alternative is a cache that has to be invalidated by
  something, which is a second thing to be wrong about how the filesystem looks. A project keeping
  skills in one directory pays two listings of directories that are not there, which is two failed
  `read_dir` calls.
- **Looking for the GitHub CLI walks `$PATH` every turn**, for the same reason and against the same
  alternative: the block is composed per turn, so a value read once would be the one that goes stale
  when something is installed mid-session. The machine that pays most is the one without the CLI,
  which walks every entry to the end to say so, and it is still a handful of directory reads in front
  of a network round trip.
- **On the path is not logged in.** Whether the CLI can reach GitHub cannot be told without a
  request, and before the first turn is the wrong place to make one, so the imperative carries its
  own fallback rather than resting on a stronger probe. A machine holding a CLI nobody has logged in
  with spends one run finding that out, which is the trade: one run against every fetch of a page
  for the part of it being read.
- **A pointer that points at a pointer is not followed twice.** A chain is a mistake in the project
  rather than a layout to support, and the second read is where a cycle would become a hang.
- **The date is UTC, not local.** The offset is not knowable without a timezone database, and a
  dependency for one line of the prompt is the worse trade. A planner near midnight may be a day
  out, which matters to nothing that is not already asking the user.
- **A project cannot turn off a global `AGENTS.md`.** The project's file has the last word, but
  the global one is still in front of the planner and can still be followed where the project
  says nothing that contradicts it. Deleting the global file, or narrowing it, is the only way to
  remove it, since a project is not the right place to be granted power over the user's own
  configuration.
