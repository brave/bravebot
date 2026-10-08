---
id: SHELLINT
title: Shell integration
status: normative
governs:
  - crates/cli/src/shell_init.rs
documented-by: docs/website/docs/reference/cli.md
---

## Scope

`bravebot shell-init <shell>` prints a hook for a person's own shell. The hook keeps the command
lines the person ran in that terminal and defines `@bravebot`, which asks one question with those
lines as input. It gives bravebot the person's other shell, which the `!` prompt
([shell-mode.md](shell-mode.md)) does not. The lines are text typed into a shell the person owns,
and a line can hold pasted text the hook cannot tell from typed text, so they reach the run as piped
input ([CLI-3](cli.md#CLI-3)) and never as trusted words. Output of those commands is never
recorded.

## Clauses

<a id="SHELLINT-1"></a>
### SHELLINT-1: `shell-init <shell>` prints a hook and does nothing else

`bash`, `zsh` or `fish` writes the hook to stdout, and stderr is empty. The text is fixed: the
command reads nothing under `~/.bravebot`, writes nothing there and makes no request, so the output
is the same wherever it runs. No shell, a shell there is no hook for, or any argument after the
shell is refused with the status for an argument ([CLI-6](cli.md#CLI-6)), prints nothing on stdout
and says why through the catalogue ([LOCALE-2](localization.md#LOCALE-2)). `--agent`,
`--system-prompt` and `--append-system-prompt` are refused with it, as with `completion`
([CLI-20](cli.md#CLI-20)).

**Why.** The output is evaluated by a person's shell at every start, so it must not depend on what
is in a workspace or a state directory, and a refusal must print nothing for the shell to evaluate.

`verified-by: bravebot_cli::running::a_shell_init_script_reads_and_writes_nothing_under_the_home`
`verified-by: bravebot_cli::running::a_shell_init_with_no_script_to_print_is_refused_with_the_argument_status`
`verified-by: bravebot_cli::main::a_definition_is_refused_where_nothing_would_work_under_it`
`verified-by: bravebot_cli::main::the_system_prompt_flags_are_refused_where_nothing_would_use_them`

<a id="SHELLINT-2"></a>
### SHELLINT-2: the hook records the command line and nothing else, privately

Each line the person submits is appended to `~/.bravebot/shell/<terminal id>`, where the terminal
id is the shell's process id. A command that spans lines is recorded as one line, with its
newlines as spaces. The line is recorded as submitted, so what a command printed and whether it
succeeded are not recorded. A line that is `@bravebot`, or begins with `@bravebot` and a space, is
not recorded, so a question is not part of the next one. The state directory, the shell directory
and the file are mode 0700, 0700 and 0600 whatever the shell's umask is and however the state
directory was created ([STATE-1](state-directory.md#STATE-1)).

**Why.** A command line names paths, hosts and branches and can hold a token typed as an argument,
which is what [STATE-1](state-directory.md#STATE-1) protects in the history file. Output is the
larger and less predictable record, and a question that needs it can ask for it with a tool.

**Known costs.** bash records through its history, when the command finishes and the prompt
returns, so a command still running or killed is not in the record, where zsh and fish record it
as it starts and so also record the line that sets `BRAVEBOT_INCOGNITO`. With bash, a line that `HISTCONTROL` or `HISTIGNORE` drops is not recorded,
which is the behaviour a person who set them wants. The tests
run the bash hook in a real `bash`; the zsh and fish hooks are the same design written for those
shells and are not run by the suite.

`verified-by: bravebot_cli::running::a_question_carries_the_commands_since_the_last_one_on_stdin`
`verified-by: bravebot_cli::running::the_recorded_lines_are_private_and_end_with_the_terminal`

<a id="SHELLINT-3"></a>
### SHELLINT-3: `BRAVEBOT_INCOGNITO` set and not empty stops the recording

The variable is read at each command, not once when the hook is sourced, so setting it partway
through a terminal stops recording from the next line. `@bravebot` then starts its run with `--incognito` ([INCOG-3](incognito.md#INCOG-3)).

**Why.** A person who wants a stretch of work left out should not have to edit their shell
startup file, and a run started from a stretch they asked to leave no trace of should not leave
one itself.

`verified-by: bravebot_cli::running::an_incognito_shell_records_nothing_and_asks_incognito`

<a id="SHELLINT-4"></a>
### SHELLINT-4: a question carries the lines since the last one, and the terminal's file ends with it

`@bravebot "question"` runs `bravebot -p "question"` with the last 200 recorded lines, and at most
64 KiB of them, as standard input, and then empties the file, so the next question carries only
what was run after it. A question asked with nothing recorded carries no input. Closing the
terminal removes the file: the hook chains the shell's exit handler where one exists and does not
replace it.

**Why.** The lines answer "what did I just run". A question that carried every line since the
terminal opened would answer about commands that have nothing to do with it and would reach the
input cap ([CLI-4](cli.md#CLI-4)). A file left behind by a closed terminal is a record nobody can
see is still there.

**Known costs.** A terminal that is killed rather than closed runs no exit handler and leaves its
file. The next shell with the same process id empties it as it starts, and until then it is mode 0600.
The file is emptied when the question is sent, so a run that fails to start does not get the lines
back. The first line of a window cut at 64 KiB can begin partway through. Anything piped into
`@bravebot` is ignored: the input is the recorded lines.

`verified-by: bravebot_cli::running::a_question_carries_the_commands_since_the_last_one_on_stdin`
`verified-by: bravebot_cli::running::the_recorded_lines_are_private_and_end_with_the_terminal`

<a id="SHELLINT-5"></a>
### SHELLINT-5: the lines enter the run as piped input and never as an argument

The hook gives the lines to the run on standard input, which [CLI-3](cli.md#CLI-3) labels
untrusted and private and presents to the planner as a reference. A recorded line is never placed
in the argument list, where it would be the prompt and trusted, and the only argument words are
the ones the person typed after `@bravebot`.

**Why.** A recorded line can hold text that came from a web page or a chat window by paste, and
the hook cannot tell it from what was typed. Treating the pessimistic way costs nothing the
person can see, since the planner is given the reference and the lines through the same route as
`cat build-error.txt | bravebot -p ...`.

`verified-by: bravebot_cli::running::a_question_carries_the_commands_since_the_last_one_on_stdin`
`verified-by: bravebot_core::policy::piped_input_is_quarantined_when_presented`
`verified-by: bravebot_agent::turn::piped_input_is_never_shown_to_the_planner`
