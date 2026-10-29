---
id: INSTR
title: Resolving standing instructions
status: normative
governs:
  - crates/agent/src/preamble.rs
  - crates/agent/src/rules.rs
  - crates/agent/src/agents.rs
  - crates/agent/src/home.rs
  - crates/agent/src/styles.rs
  - crates/agent/src/workspace/touched.rs
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
### INSTR-1: thirteen sources kept as files, two more only when named, and no others

The table lists the files read without being asked. Two more, `~/.agents/skills/<name>/SKILL.md` and
`~/.claude/skills/<name>/SKILL.md`, apply to every project and are read only for the directory
the person's settings name ([INSTR-17](#INSTR-17)).

| File | Applies to |
|---|---|
| `~/.bravebot/AGENTS.md` | every project |
| `~/.bravebot/projects/<id>/rules.md` | the project [RECALL-1](recall.md#RECALL-1) names |
| `~/.bravebot/skills/<name>/SKILL.md` | every project |
| `~/.bravebot/agents/<name>.md` | every project |
| `~/.bravebot/rules/<name>.md` | every project, from the turn a file its `paths` match is touched, or every turn without `paths` |
| `<workspace>/AGENTS.md`, else `CLAUDE.md`, else `.claude/CLAUDE.md` | this project |
| `<workspace>/.bravebot/skills/<name>/SKILL.md` | this project |
| `<workspace>/.claude/skills/<name>/SKILL.md` | this project |
| `<workspace>/.agents/skills/<name>/SKILL.md` | this project |
| `<workspace>/.bravebot/agents/<name>.md` | this project |
| `<workspace>/.bravebot/rules/<name>.md` | this project, from the turn a file its `paths` match is touched, or every turn without `paths` |
| `<workspace>/<directory>/AGENTS.md` | this project, once the session has worked in that directory |
| `<workspace>/<directory>/.bravebot/skills/<name>/SKILL.md` | this project, once the session has worked in that directory |

The two roots are spelled differently on purpose: the user's own directory is already `.bravebot`,
so its skills and definitions sit directly beneath it, while a project keeps its own out of the
way in a dotted directory rather than at the root where `AGENTS.md` sits.

`.claude/skills` and `.agents/skills` are read for the same reason `CLAUDE.md` is: a project that
wrote its skills for another agent should not have them ignored over the spelling, and copying or
symlinking each one into `.bravebot/skills` is work the project should not have to do.
[INSTR-13](#INSTR-13) is their order and the root they are read at. The user's own
`~/.claude/skills` and `~/.agents/skills` are sources only once the person's own settings name them
([INSTR-17](#INSTR-17)); until then they are not read, because [INSTR-2](#INSTR-2) tries no user
directory but `~/.bravebot` without that agreement.

The command line is a source that is not a file, and [INSTR-10](#INSTR-10) is the whole of it.

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

The one addition is a directory the person has named in `sharedSkillDirectories`
([INSTR-17](#INSTR-17)). That is the person choosing the directory, which is what a fallback lacks.

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
`verified-by: bravebot_agent::preamble::an_added_directory_the_person_vouched_for_still_contributes_no_standing_instructions`
`verified-by: bravebot_agent::skills::a_directory_opened_beside_the_project_adds_no_skills_even_where_it_is_vouched_for`

## What wins

<a id="INSTR-4"></a>
### INSTR-4: sources are read least specific first, so the project has the last word

The user's own directory is read before the project. Both `AGENTS.md` files are read and both
reach the planner, in that order, and a project skill replaces a global one of the same name.
A skill in `~/.bravebot/skills` replaces one of the same name in a shared directory the person
named ([INSTR-17](#INSTR-17)), and a project skill replaces both.
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
`verified-by: bravebot_agent::preamble::a_pointer_naming_a_file_outside_the_workspace_is_not_followed`
`verified-by: bravebot_agent::preamble::a_pointer_in_an_untrusted_directory_is_a_notice_and_no_text`
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

What to do when a program fails because something it needs is missing from this machine is said on
the same terms, to the turn a person is watching and only where the session has a directory of its
own. Check whether something installed can stand in for what is missing, with a line that only
reads and whose exit status is the answer, run by the same interpreter or runtime the program uses.
Where something can, write what connects the two in a new directory inside the session's own, and
run the program again with that directory on a search path the program reads from its environment
in addition to its own, assigned in front of it on that one line, the path written out in full and
in single quotes. Try that before asking the person to install anything or to edit the program.
Where nothing installed can stand in, say what is missing and what provides it. The paragraph names
no language, package or variable. A session with no directory of its own is told none of this, and
neither is a delegate.

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

The stand-in is stated because a planner not told of it asks the person to install what is missing
or to edit the program, and both change the machine or the program to supply what something
installed can often already provide. It describes no case: it says what to do when a program fails
for something missing, and names no language, package or variable, because text naming one of them
covers that one alone. What connects the two, written in the session's own directory, changes
neither the machine nor the program, and goes when the session does. The check runs on the
program's own interpreter or runtime because another one on the same machine can have what that one
lacks. The
search path is one read in addition to the program's own, because an assignment replaces whatever
the variable held and a line cannot name the old value, so a path that stands in for a default would
lose it. The stand-in goes in a directory of its own, and only that directory goes on the path,
because the session's directory also holds whatever else the work put in it. The assignment is on
the one line because nothing carries over between lines. The path is written out because a line
expands no variable, and quoted because a line reads an unquoted backslash as an escape and a
Windows path is made of them. It is gated on the directory because without one the only place to
write the stand-in is the project. It is withheld from a delegate because the road is two `run`s and
a reader is offered none. Nothing here is a decision the driver takes on a program's output: the
check is answered by an exit status ([RUN-13](tools/run.md#RUN-13)), and what the planner does with
it is the planner's.

`verified-by: bravebot_agent::preamble::the_working_directory_is_stated_so_nothing_has_to_run_pwd`
`verified-by: bravebot_agent::preamble::the_environment_is_stated_even_with_no_instructions_to_read`
`verified-by: bravebot_agent::preamble::whether_the_tree_is_a_git_repository_is_said_either_way`
`verified-by: bravebot_agent::preamble::moving_the_working_directory_restates_it`
`verified-by: bravebot_agent::preamble::the_sessions_own_directory_is_stated_so_a_turn_can_write_in_it`
`verified-by: bravebot_agent::preamble::a_session_with_no_directory_of_its_own_is_told_of_none`
`verified-by: bravebot_agent::preamble::a_missing_dependency_is_stood_in_for_from_the_sessions_own_directory`
`verified-by: bravebot_agent::preamble::the_stand_in_road_names_no_language_or_variable`
`verified-by: bravebot_agent::confine::a_confined_python_imports_from_the_sessions_own_directory_only_when_the_line_names_it`
`verified-by: bravebot_agent::preamble::a_windows_version_is_the_three_numbers_a_build_is_named_by`
`verified-by: bravebot_agent::preamble::whether_the_github_cli_is_installed_is_said_either_way`
`verified-by: bravebot_agent::preamble::a_github_url_is_sent_to_the_cli_only_where_the_probe_found_one`
`verified-by: bravebot_agent::preamble::an_installed_github_cli_still_names_fetch_url_as_the_fallback`
`verified-by: bravebot_agent::preamble::the_road_a_github_url_takes_is_not_in_the_block_a_delegate_reads`
`verified-by: bravebot_agent::turn::the_road_for_a_github_url_goes_out_with_the_fact_it_rests_on`

<a id="INSTR-10"></a>
### INSTR-10: words from the command line have the last word on precedence, and a replacement reaches the opening alone

`--append-system-prompt` ([CLI-19](cli.md#CLI-19)) adds its words to the standing instructions under
the heading "From the command line", after the project's instructions, so the order of INSTR-4
continues one place further and the words have the last word on precedence. A rule file a touched
file brings in is appended after them ([INSTR-16](#INSTR-16)), so it comes later in the prompt and
the words stand where it disagrees. They are trimmed, and they are in the text a delegate reads as
well as the one a person's turn does.

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
in the project first. Last place among the sources INSTR-4 orders is the most specific. Only the
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
### INSTR-12: an output style stands where `--system-prompt` stands, and yields to it

`/style <name>` picks one of the styles this build ships (`concise`, `explanatory` and `proactive`),
one of the person's own or one of the project's, `/style off` clears the pick and `/style` alone says which is in force and lists the names. The
pick lasts for the session and is not stored. A `style` key in `~/.bravebot/settings.json`, or in the file `--settings` names, is the style a session starts in, named as `/style` names one: a project's or a local settings file naming it is ignored and reported, since which words open the prompt is the person's to say, and a name that is no style is said and starts none. A resumed session starts in that style or none. Each turn whose prompt opens with a style's words records it in the trail by name and by whether it is built in, the person's own file or the project's, never by its words ([TRACE-7](trace.md#TRACE-7)); a turn where `--system-prompt` stood in its place records none. A name is
compared whole against the built-in list, the person's own files and the project's, and nothing a person types after
`/style` reaches the prompt.

A person's own style is a file `~/.bravebot/styles/<name>.md`. Its name is a slug (lowercase letters
and digits in runs joined by hyphens, as in [MEMORY-3](definition-memory.md#MEMORY-3)) and a name is
never joined to a path before it has been checked as one, so nothing typed can name a file outside
that directory. A file is a style only where it is a regular file, not a symbolic link, of at most
4096 bytes of UTF-8 with some words in it. A file called by a built-in name is ignored, so a file
cannot take the place of one this build ships. Its words are the person's own configuration and
trusted for sitting in `~/.bravebot`, as `~/.bravebot/AGENTS.md` is.

A project's own style is a file `<workspace>/.bravebot/styles/<name>.md`, of the same shape, and it
is read only where the trust map vouches for it ([TRUST-1](trust-map.md#TRUST-1)). Neither the
directory nor the file is opened until the map has been asked, a deny rule covering the file leaves
it out, and its bytes arrive through the read and the gate the project's `AGENTS.md` goes through,
so a file that is not trusted is no style and nothing of it is looked at, listed or counted. A name
a built-in style or one of the person's own files already has is not offered from the project, so a
project cannot take the place of either. `/style` lists the project's names after the others, from
what the map vouched for when the line was typed, and picks one by its name alone: the words are not
held. A turn reads the file again when it begins, under its own map, so vouching withdrawn after
the pick, or a file an earlier turn wrote and so distrusted ([TRUST-4](trust-map.md#TRUST-4)),
takes the style out of that turn. That turn opens with the usual words, records no style, and says
in its notices that the style was not used. A project's style is not a `style` setting's to name:
the setting is read before the map is, so it names a built-in style or one of the person's own.

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
`--system-prompt` can only be given when the program starts. A project's style is the one source
that is not the person's own, so it has the standing the project's `AGENTS.md` has and no more:
vouched for by the map or not read at all, and read again at each turn so the vouching is not one
the pick outlives. The rule INSTR-10 relies on, that nothing read from a project reaches the opening
without the trust map saying so, is the one that holds.

**Known costs.** Only the terminal interface has `/style` and reads the `style` setting. A project's
styles are offered from a list read when a `/style` line is typed, so one picked while a turn runs
is taken on the list last read, and a name that list never held is refused. The trail names a
project's style and not its file, so a reader cannot tell two files of one name apart across a
change of the file.

`verified-by: bravebot_agent::turn::a_style_takes_the_place_of_the_opening_alone_and_yields_to_system_prompt`
`verified-by: bravebot_agent::styles::a_style_is_found_by_its_whole_name_only`
`verified-by: bravebot_agent::styles::no_style_takes_the_quarantine_guidance_with_it`
`verified-by: bravebot_agent::styles::a_file_in_the_home_directory_is_a_style_by_its_name`
`verified-by: bravebot_agent::styles::a_file_cannot_stand_in_for_a_built_in_name`
`verified-by: bravebot_agent::styles::a_name_that_is_not_a_slug_never_reaches_a_path`
`verified-by: bravebot_agent::styles::a_link_an_empty_file_and_a_long_file_are_not_styles`
`verified-by: bravebot_agent::styles::the_offered_names_are_the_built_ins_then_the_accepted_files_in_order`
`verified-by: bravebot_agent::styles::a_project_style_is_read_only_where_the_trust_map_vouches`
`verified-by: bravebot_agent::styles::a_project_file_cannot_take_the_place_of_a_built_in_or_a_home_style`
`verified-by: bravebot_agent::styles::a_project_link_an_empty_file_a_long_file_and_a_bad_name_are_not_styles`
`verified-by: bravebot_agent::styles::a_deny_rule_over_a_project_style_neither_offers_nor_resolves_it`
`verified-by: bravebot_agent::turn::a_project_style_opens_the_prompt_only_where_the_trust_map_vouches`
`verified-by: bravebot_tui::app::a_project_style_is_listed_and_picked_by_name_only_where_one_was_vouched_for`
`verified-by: bravebot_tui::state::the_project_styles_are_read_once_per_style_line_and_held_after_it`
`verified-by: bravebot_agent::turn::a_style_from_a_file_opens_the_prompt_and_yields_to_system_prompt`
`verified-by: bravebot_agent::turn::a_turn_records_the_output_style_that_opened_it`
`verified-by: bravebot_tui::app::a_style_file_in_the_home_directory_is_listed_and_picked_by_name`
`verified-by: bravebot_config::settings::a_project_layer_cannot_name_the_style`
`verified-by: bravebot_config::settings::a_style_that_is_blank_or_not_a_string_names_nothing`
`verified-by: bravebot_tui::app::a_style_the_settings_name_is_in_force_from_the_first_turn_and_a_wrong_name_is_said`
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

The count is what a more specific root did not go on to offer, a more specific root being one read
after the skipped one, including a directory [INSTR-14](#INSTR-14) adds. It leaves out each entry
with the directory name of a skill on the list from such a root. That skill is on the list a person
can see, so counting the entry as not loaded would contradict what is in front of them. The
untrusted root is asked only whether it holds an entry of each such name, and none of its own names
is held or compared. A built-in skill, a skill from `~/.bravebot`, a skill from a shared directory
the person named ([INSTR-17](#INSTR-17)) and a skill from a less specific
root leave the count as it is, and so does a skill that a more specific one of the same name took
off the list. A root whose every skill was covered that way is not reported at all.

**Why not the user's own `~/.claude/skills` and `~/.agents/skills` by default.** [INSTR-2](#INSTR-2)
tries no user directory but `~/.bravebot`, and reading instructions from a directory the person never
chose is the fallback that clause refuses. They are read only when the person names them
([INSTR-17](#INSTR-17)).

`verified-by: bravebot_agent::skills::a_bravebot_skill_shadows_a_foreign_one_of_the_same_name`
`verified-by: bravebot_agent::skills::a_foreign_skills_directory_below_the_root_is_not_a_source`
`verified-by: bravebot_agent::skills::a_foreign_skills_directory_under_a_touched_directory_is_not_a_source`
`verified-by: bravebot_agent::skills::a_foreign_skill_in_an_untrusted_project_is_counted_and_not_named`
`verified-by: bravebot_agent::skills::a_skill_a_vouched_root_offers_is_not_also_reported_as_not_loaded`
`verified-by: bravebot_agent::skills::an_untrusted_entry_named_like_a_vouched_skill_is_still_counted`
`verified-by: bravebot_agent::skills::a_skill_from_no_vouched_root_leaves_an_untrusted_entry_counted`
`verified-by: bravebot_agent::skills::a_vouched_skill_shadowed_off_the_list_leaves_an_untrusted_entry_counted`
`verified-by: bravebot_agent::skills::a_less_specific_vouched_root_leaves_an_untrusted_entry_counted`

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
the session's, so it is empty after `/cd`. The session's record keeps the directories as names, and a
resumed session places each by its spelling under the root again, so a name that now lands outside
the root adds nothing. The text of the files they name is read on the first turn of the resumed
session as on any other, through the trust map. A fork carries the names with the conversation it
copies ([SESSION-3](sessions.md#SESSION-3)). [INSTR-16](#INSTR-16) reads the files of the same
record.

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
`verified-by: bravebot_agent::preamble::the_directories_worked_in_stop_at_sixty_four`
`verified-by: bravebot_agent::preamble::a_link_in_a_touched_name_is_not_followed_to_where_it_lands`
`verified-by: bravebot_agent::preamble::a_nested_agents_file_is_not_followed_to_a_pointer_or_expanded_for_an_import`
`verified-by: bravebot_agent::skills::a_nested_skill_is_offered_after_the_session_works_in_its_directory`
`verified-by: bravebot_agent::skills::a_deeper_nested_skill_shadows_a_shallower_one_of_its_name`
`verified-by: bravebot_agent::skills::a_foreign_skills_directory_under_a_touched_directory_is_not_a_source`
`verified-by: bravebot_agent::skills::a_distrusted_nested_skill_is_counted_and_not_named`
`verified-by: bravebot_agent::skills::a_distrusted_nested_skill_is_counted_beside_a_shallower_one_of_its_name`
`verified-by: bravebot_agent::skills::a_nested_skill_on_the_list_covers_a_distrusted_project_entry_of_its_name`
`verified-by: bravebot_agent::preamble::a_workspace_restored_from_a_record_composes_the_nested_file_and_the_rule_of_the_run_before`
`verified-by: bravebot_agent::workspace::a_workspace_restored_from_a_record_places_each_name_under_the_root_and_keeps_the_limits`
`verified-by: bravebot_session::sessions::the_names_the_file_tools_reached_are_written_and_read_back`
`verified-by: bravebot_session::sessions::a_record_from_before_touched_names_were_kept_reads_as_having_none`
`verified-by: bravebot_session::sessions::a_fork_carries_the_names_the_file_tools_reached`
`verified-by: bravebot_cli::running::a_continued_run_has_the_rule_the_earlier_runs_file_read_loaded`
`verified-by: bravebot_agent::turn::a_nested_agents_file_reaches_the_turn_after_a_file_beside_it_is_read`
`verified-by: bravebot_agent::turn::a_nested_agents_file_reaches_the_turn_after_a_file_beside_it_is_written`
`verified-by: bravebot_agent::workspace::moving_the_working_directory_forgets_the_directories_worked_in`
`verified-by: bravebot_agent::safe::a_safe_session_offers_no_nested_skill`
`verified-by: bravebot_agent::safe::a_safe_session_reads_no_nested_agents_file`

<a id="INSTR-15"></a>
### INSTR-15: a standing source long enough to matter is said out loud

After a source has passed the trust gate, its byte length is compared against two fixed figures.
Over 16384 bytes a notice names the file and its size. The byte lengths of all the sources are
added up, and over 32768 bytes in all one further notice says the total. The combined notice is
said once for the turn, at the source that crosses the figure. Neither figure keeps a file out of
the prompt: every source that passes the trust gate reaches it, whatever its size.

The sources measured are the ones [INSTR-4](#INSTR-4) orders: `~/.bravebot/AGENTS.md`, the
project's file, each nested `AGENTS.md` of [INSTR-14](#INSTR-14), and each rule file of
[INSTR-16](#INSTR-16) that is in the prompt when the turn begins. Words from
`--append-system-prompt` are not measured, for the reason [INSTR-10](#INSTR-10) does not read them
through the gate: a person who typed them for this run has said what this run carries. A source an
untrusted directory holds is already out before this rule is reached, so nothing untrusted is
measured and nothing untrusted is named.

The notices are said before the first request, as [SKILL-11](skills.md#SKILL-11) requires of every
notice, and composed per turn like every other part of this ([INSTR-7](#INSTR-7)), so shortening a
file ends them.

This is a length and nothing else. The driver compares the byte count of text that has already
passed the gate, which is the test [INSTR-8](#INSTR-8) already rests on, and a notice carries a
path from [INSTR-1](#INSTR-1)'s fixed set and a number. Nothing a file says decides whether it is
loaded or what is said about it. A path is named here where [SKILL-6](skills.md#SKILL-6) counts
rather than names, because that rule is about a path an untrusted project chose; these paths are
fixed by [INSTR-1](#INSTR-1) and a nested one passed the trust map before it was measured.

[INSTR-6](#INSTR-6) keeps the absence of a source silent and is untouched: this concerns a file
that is there.

**Why.** [INSTR-5](#INSTR-5) puts every source in every request of the session, so a file nobody
meant to be long is paid for on every turn and nothing today says its size. Codex stops adding
instruction files at 32 KiB in all and Claude Code warns over a recommended length, which is the
same judgement about the same cost: a warning, not a refusal. A file nobody meant to write that
long is still a file somebody wrote on purpose, and a half-read set of instructions is worse than
a whole one that cost a sentence to say so: the planner cannot tell which rules a drop left out,
and a rule cut off mid-sentence still reads as a rule.

**Known costs.** Nothing here accounts for the prompt against the model's context window, and
compaction ([compaction.md](compaction.md)) is where that question belongs.

`verified-by: bravebot_agent::preamble::a_long_instructions_file_loads_and_its_size_is_said`
`verified-by: bravebot_agent::preamble::a_very_large_instructions_file_still_loads`
`verified-by: bravebot_agent::preamble::sources_long_together_are_reported_once`
`verified-by: bravebot_agent::preamble::a_long_rule_file_in_force_is_measured_like_an_instructions_file`

<a id="INSTR-16"></a>
### INSTR-16: a rule file with `paths` is added once a file one of its globs matches is touched

`~/.bravebot/rules/<name>.md` and `<workspace>/.bravebot/rules/<name>.md` are rule files: the
`.md` files directly in each directory, in name order, the person's own before the project's
([INSTR-4](#INSTR-4)). A file whose name is not UTF-8 text is not one, and the person is told how many
were passed over. A rule file with no `paths` key in its frontmatter is standing text like the
project's `AGENTS.md`, in force on every turn, and follows the `AGENTS.md` of its own root. A rule
file with a `paths` list is held back. It is added to the system prompt once a file one of its
globs matches has been touched, and stays for the rest of the session. `paths: []` names no file,
so such a rule never loads. A file that opens a frontmatter block and never closes it, or whose
`paths` list opens a `[` and never closes it, is left out and the person is told, and is not read
as text for every turn. A byte order mark or blank lines before the first marker are not part of
the file. No other key is read.

`paths` is a list of globs, written one `- glob` to a line, as `[a, b]`, which may run over several
lines, or as one glob or a comma-separated run after the colon. A comma inside `{}` or quotes
belongs to the glob, and a `#` after whitespace begins a comment. A glob is the dialect
`list_files` takes: `*`, `**`, `?` and `{a,b}`, matched against a name relative to the project
root. A glob with no `/` is matched against the file's own name at any depth, unless it is written
with a leading `/` or `./`, which names the project root's own files and is otherwise dropped. A
glob ending in `/` names everything below that directory. A `paths` value that is not a list of
globs, such as a block scalar, is left out and the person is told, as an unclosed list is; `null`
names no file, as `[]` does.

A file is touched when `read_file` read it, a tool that writes wrote it, or the person named it
with `@` in the prompt and a file is there. What is compared with a glob is the name the planner typed to the tool, or
the person typed after `@`, placed under the root by its spelling alone as INSTR-14 places a
directory, so a link is not followed to where it lands. No file's content and no tool's output is
compared, so a name a listing, a search, a command or a processor printed is not a touch, and
neither is a file read through a reference, since the planner typed the reference and not the
file's name. A name outside the root, or one a refused call or a `deny` rule kept from being read,
is not a touch either, although a declined credential read still records its directory for
INSTR-14. A file named with `@` is not worked in, so it adds no directory to INSTR-14's record,
and a name that is a directory, or is nothing, is not a file named.

A rule's text comes through the read, the trust map and the `deny` rules of
[INSTR-8](#INSTR-8), and the person's own are trusted for sitting in `~/.bravebot` unless the way
to the directory goes through one a turn can write to ([SKILL-3](skills.md#SKILL-3)). The project's
directory is listed to count its files, and no file in it is opened until the map has been asked. A
directory the map does not vouch for contributes one notice with a count and no name, a file a
`deny` rule covers is left out with a notice naming it ([PERM-7](permissions.md#PERM-7)), and a rule
is taken as it stands: a pointer or an `@path` import in it is ordinary text. A safe session reads
no rule file.

Rule files are read when a turn begins ([INSTR-7](#INSTR-7)). The rules a touched file already
matches, from an earlier turn or from this prompt's `@`, are in the system prompt before the first
request. A file touched while a turn runs adds its rule before the next request of that turn. In
both cases a rule is appended after everything else the system prompt holds, the words about the
mode and the words from the command line ([INSTR-10](#INSTR-10)) included, under one heading, so
the bytes in front of the rule are the ones the last request sent. The heading says that every
instruction above, the mode and the command line's words among them, stands where a rule
disagrees. Rules the session added
before keep the order it added them in, and the rest follow in the order the rule files are read,
so a turn that starts after the last round of another carries the same prompt. The person is told
once for the session which rule was added and which touched file matched it. A rule file written
during a turn takes effect on the next one. A rule in force on every turn, and a rule a touched file
has already brought in when the turn begins, is measured by [INSTR-15](#INSTR-15) as an
`AGENTS.md` is.

The record of touched files is INSTR-14's, kept beside its directories: the session's, empty after
`/cd`, and at most 1024 files, which are compared with a held rule's
globs once when a turn begins and once more for each file touched after. A front end that builds a
workspace for each turn hands it on with the checkouts the session keeps
([CHECKOUT-16](checkouts.md#CHECKOUT-16)), so the record outlives a turn there too.

**Why.** A convention for one kind of file costs a line in every request of a session that never
goes near one, and putting it in the project's `AGENTS.md` makes it share a file with everything
else. Matching only names the planner or the person chose keeps what INSTR-1 refused when it
refused to walk upward: nothing a file holds, and nothing a tool printed, decides which trusted
file is read. Appending rather than inserting at the place the rule would stand if it had been
there from the start leaves the prompt up to the rule as the last request sent it, which is what a
service that reuses a prefix reads again, and the next turn starts with the rule in the same place.

**Known costs.** A glob has no character class, as `list_files` has none: a `[` in one is an
ordinary character. A name is compared as it was typed, so on a volume that folds case a name typed
in another case than the glob does not match. A session that touches more than 1024 files loads no
rule for the 1025th. A rule that arrives while a turn runs changes the system message, which is
where [BACKEND-27](backends.md#BACKEND-27) and [BACKEND-32](backends.md#BACKEND-32) put their first
breakpoint, so the next request reads the system prompt and the history in front of it again at
full price, once per request that adds a rule. A rule that arrives while a turn runs is not
measured by [INSTR-15](#INSTR-15) until the next turn begins. The session's record keeps the names
of the files touched and of the rules told, so a resumed session has the rules its earlier turns
loaded from its first turn, read again through the trust map and the `deny` rules. A name in a
record is placed under the root by its spelling as a typed name is, and the 1024-file limit holds
for a record as for a live session.

`verified-by: bravebot_agent::preamble::a_rule_with_paths_is_in_the_prompt_only_after_a_file_it_matches_is_touched`
`verified-by: bravebot_agent::preamble::a_loaded_rule_goes_after_everything_else_in_the_prompt`
`verified-by: bravebot_agent::preamble::a_rule_with_no_paths_is_in_force_every_turn_and_the_projects_have_the_last_word`
`verified-by: bravebot_agent::preamble::a_rule_whose_paths_list_is_empty_never_loads`
`verified-by: bravebot_agent::preamble::naming_a_file_with_at_loads_its_rule_and_not_its_directorys_instructions`
`verified-by: bravebot_agent::preamble::a_rule_in_the_persons_own_directory_loads_on_a_matching_file`
`verified-by: bravebot_agent::preamble::a_rule_in_an_untrusted_directory_is_a_notice_and_no_text`
`verified-by: bravebot_agent::preamble::a_rule_file_a_deny_rule_covers_is_not_read`
`verified-by: bravebot_agent::preamble::a_rule_whose_frontmatter_or_paths_list_never_closes_is_left_out_with_a_notice`
`verified-by: bravebot_agent::preamble::the_persons_rules_are_not_read_by_a_way_that_goes_through_the_workspace`
`verified-by: bravebot_agent::preamble::a_rule_stays_in_force_for_the_turns_after_the_file_that_loaded_it`
`verified-by: bravebot_agent::preamble::a_workspace_restored_from_a_record_composes_the_nested_file_and_the_rule_of_the_run_before`
`verified-by: bravebot_session::sessions::the_names_the_file_tools_reached_are_written_and_read_back`
`verified-by: bravebot_cli::running::a_continued_run_has_the_rule_the_earlier_runs_file_read_loaded`
`verified-by: bravebot_agent::preamble::a_long_rule_file_in_force_is_measured_like_an_instructions_file`
`verified-by: bravebot_agent::preamble::a_rule_file_whose_name_is_not_text_is_not_read_as_the_one_its_lossy_spelling_names`
`verified-by: bravebot_agent::rules::a_block_list_names_one_glob_a_line`
`verified-by: bravebot_agent::rules::a_list_at_the_keys_own_indent_ends_at_the_next_key`
`verified-by: bravebot_agent::rules::a_comma_inside_braces_or_quotes_belongs_to_the_glob`
`verified-by: bravebot_agent::rules::an_empty_list_names_no_file_and_no_key_names_every_turn`
`verified-by: bravebot_agent::rules::a_comment_after_a_glob_is_not_part_of_it`
`verified-by: bravebot_agent::rules::a_bracket_inside_a_glob_does_not_close_the_list`
`verified-by: bravebot_agent::rules::a_byte_order_mark_or_blank_lines_before_the_frontmatter_are_not_part_of_the_file`
`verified-by: bravebot_agent::rules::a_value_that_is_not_a_list_is_unreadable_and_null_names_no_file`
`verified-by: bravebot_agent::rules::a_glob_ending_in_a_slash_names_what_is_below_it`
`verified-by: bravebot_agent::rules::a_quote_inside_a_word_is_part_of_the_glob`
`verified-by: bravebot_agent::rules::a_list_in_brackets_may_run_over_several_lines`
`verified-by: bravebot_agent::rules::a_glob_written_from_the_root_is_about_the_roots_own_files`
`verified-by: bravebot_agent::rules::rules_added_before_keep_the_order_they_were_added_in`
`verified-by: bravebot_agent::rules::an_indented_paths_line_is_not_the_key`
`verified-by: bravebot_agent::rules::a_glob_with_a_separator_anchors_and_a_bare_one_matches_at_any_depth`
`verified-by: bravebot_agent::rules::a_rule_is_taken_once_and_only_the_first_carries_the_heading`
`verified-by: bravebot_agent::turn::a_rule_a_round_brings_in_is_appended_to_the_system_prompt_of_the_next_round`
`verified-by: bravebot_agent::turn::a_file_a_listing_or_a_search_names_does_not_load_a_rule`
`verified-by: bravebot_agent::turn::a_file_the_person_names_with_at_loads_its_rule_for_the_first_request`
`verified-by: bravebot_agent::turn::a_file_written_or_edited_loads_its_rule_for_the_next_round`
`verified-by: bravebot_agent::turn::a_read_the_person_declined_for_a_credential_does_not_load_a_rule`
`verified-by: bravebot_agent::turn::a_file_named_with_at_that_a_deny_rule_covers_loads_no_rule`
`verified-by: bravebot_agent::turn::a_delegate_started_after_a_read_carries_the_rule_and_leaves_the_telling_to_the_turn`
`verified-by: bravebot_agent::turn::a_delegate_leaves_the_telling_of_a_rule_to_the_turn_that_started_it`
`verified-by: bravebot_agent::turn::a_file_read_through_a_reference_does_not_load_a_rule`
`verified-by: bravebot_agent::turn::rules_added_in_one_order_are_carried_in_that_order_by_the_next_turn`
`verified-by: bravebot_agent::workspace::moving_the_working_directory_forgets_the_files_touched_and_the_rules_told_of`
`verified-by: bravebot_agent::workspace::a_name_typed_with_at_records_the_file_and_not_its_directory`
`verified-by: bravebot_agent::workspace::a_name_typed_with_at_that_is_a_directory_or_nothing_is_not_a_file`
`verified-by: bravebot_agent::workspace::a_touched_file_is_recorded_by_the_spelling_the_planner_typed`
`verified-by: bravebot_agent::workspace::a_workspace_given_what_the_session_kept_has_the_files_it_touched`
`verified-by: bravebot_agent::workspace::the_files_touched_stop_at_a_thousand_and_twenty_four`
`verified-by: bravebot_agent::safe::a_safe_session_reads_no_rule_file`

<a id="INSTR-17"></a>
### INSTR-17: the user's `~/.agents/skills` and `~/.claude/skills` are read only for a directory the person's settings name

`sharedSkillDirectories` is a list of strings in `~/.bravebot/settings.json` or the file `--settings`
names. Each entry is compared, after trimming white space and a trailing `/`, with the two strings
`~/.agents/skills` and `~/.claude/skills`, and a directory that matches is read from the home
directory. An entry matching neither names nothing, and no other directory can be reached through the
key. With the key absent, empty or not a list, neither directory is read. This is the default and it
is the only consent there is: no question is put to the person, and no answer to one is kept.

A project's settings files cannot set it, as they cannot set `style` ([INSTR-12](#INSTR-12)). Naming
a directory puts every skill in it in front of the planner, so a checkout choosing it would be
choosing what the person's home contributes to the prompt. A layer that wrote it is listed by
`doctor` and the Sessions screen as ignored.

A named directory is the person's own, so its skills are labelled from provenance as the skills in
`~/.bravebot/skills` are, with the exception [SKILL-3](skills.md#SKILL-3) makes for a path the
workspace can write to or reach through. They are read before `~/.bravebot/skills`, `.agents/skills`
first, so a skill in `~/.bravebot/skills` shadows one of the same name in either, and a project skill
shadows all of them ([INSTR-4](#INSTR-4)).

`doctor` names a directory of the two that exists and is not named, so a person who keeps skills for
another agent is told they were found and left alone. That is decided from the two fixed paths and
the key, and nothing a directory holds is read to decide it.

**Why.** [INSTR-2](#INSTR-2) refuses a fallback to a directory the person never chose. Reading the
two directories by default would be one, and a person may keep skills there for another agent
without wanting this program to offer them. Naming a directory is the person choosing it.

`verified-by: bravebot_config::settings::a_project_layer_cannot_name_a_shared_skill_directory`
`verified-by: bravebot_agent::skills::a_shared_skill_directory_the_person_did_not_name_is_not_read`
`verified-by: bravebot_agent::skills::a_named_shared_skill_directory_is_read_and_the_other_is_not`
`verified-by: bravebot_agent::skills::a_bravebot_skill_shadows_a_shared_one_and_the_projects_shadows_both`
`verified-by: bravebot_agent::skills::the_agents_shared_directory_is_read_before_the_claude_one`
`verified-by: bravebot_agent::skills::a_named_shared_skill_can_be_kept_by_a_definition_and_reaches_its_own_directory`
`verified-by: bravebot_agent::skills::a_named_shared_skill_directory_reached_through_the_workspace_is_not_read_and_is_said`
`verified-by: bravebot_agent::safe::a_safe_session_reads_no_named_shared_skill_directory`

## Known costs

Accepted deliberately. Do not "fix" one without changing this spec first.

- **Each recorded directory costs one existence check and one directory listing every turn**, up
  to the 64 the record keeps. There is no cache, because one would have to be invalidated when a
  file appears or changes mid-session. A directory only listed, searched or named in a shell
  command is not recorded: only a read or a write is.
- **Every rule file is read and gated on every turn, whether it applies to every turn or is held
  for a path.** Its `paths` are in the file, so there is no knowing which to read without reading
  them all, and a cache would have to be invalidated by a write nobody tells this about
  ([INSTR-7](#INSTR-7)). A project with many rule files pays one read, one trail entry and one parse
  for each, on every turn and every delegate it starts.
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
- **There is no ceiling past which a source of [INSTR-15](#INSTR-15) is left out.** A file of any
  size is paid for in full on every request of the session, because dropping or truncating it is a
  worse cost than carrying it whole.
