---
sidebar_position: 4
title: The run tool
description: How a command line is compiled, approved and run, what each answer grants, and how its output is labelled.
---

# The run tool

Runs a command line. **You approve the compiled plan before anything runs.**

| Parameter | |
|---|---|
| `command` | one command line; a newline is refused, since this is a line and not a script |
| `directory` | where to run, inside the workspace or a directory you added ([below](#the-directory-carries-over-and-nothing-else-does)) |
| `deadline_seconds` | how long to wait, defaulting to 300 seconds ([below](#a-line-has-a-deadline)) |
| `background` | start the line and hand back a job name instead of waiting ([below](#leaving-a-pipeline-running)) |
| `stdin_ref` | a reference whose contents are fed to the first program ([below](#filtering-something-the-agent-may-not-read)) |
| `read` | ask for the output in this result; honoured only when [bypassing with no screening](#reading-the-output-in-the-same-result) |

```
git log --oneline -50 | head -20
```

The line is **compiled, never interpreted**. No shell sees it at any point: bravebot's own grammar is
the only thing that reads it. What comes out is an ordered plan, each step a resolved binary and a
literal argument vector, together with every file the line would write. That plan is what runs. A
`;` or `|` inside quotes is part of an argument and stays part of it.

A name is looked up on `PATH`; a path is taken relative to the workspace.

**A program is started by the path its name was found by**, and a link on that path is not followed
first. That keeps `venv/bin/python` finding its virtual environment. The prompt shows the path and
the file it leads to, and your answer covers the file only when it is started by that same path.
Just before the pipeline starts, each path is resolved again, and one that no longer leads to the
file you approved refuses the whole pipeline before any step runs.

Every step's program is found when the line is compiled, before anything runs. A line therefore
cannot run a program an earlier step in it creates: `python3 -m venv v && v/bin/python x.py` is
refused, and the refusal says the step that creates the program has to run on its own first.

## The directory carries over, and nothing else does

A call may name a `directory` to run in, inside the workspace or inside a directory you added. Without
one, a line runs where the last one ran, and the first line of a turn runs at the workspace root. The
carrying lasts the turn: the next turn starts at the root again.

**Shell state does not carry.** `NAME=value` on one line has no effect on the next, because there is
no shell process between calls to hold it. A line that needs a variable set puts it on that line. A
directory is a routing field, shown at every prompt and endorsed with the plan, so carrying it is
visible; an environment that accumulated invisibly would change what a later plan does without
appearing in that plan.

A directory that is not the workspace root **is asked about**, and what the line prints is quarantined,
unless something you answered names that exact tree. A directory that does not exist is an error and
moves nothing, and a refused directory does not become the one the next line runs in.

## What the grammar takes

| | |
|---|---|
| pipelines and sequencing | <code>\|</code>, `&&`, <code>\|\|</code>, `;`, and `( … )` to group |
| redirection | `>`, `>>`, `<`, `2>`, `2>>`, `2>&1`, `&>`, each naming one literal file |
| patterns | `*`, `?`, `[…]`, `**` |
| brace expansion | `{a,b}`, `{1..9}` |
| home | a leading `~`, and only a leading one |
| per-command environment | `NAME=literal cmd` |

A leading `~` is **your home directory**, the one `~/.bravebot` sits inside, and never that directory
itself: a home-relative path the planner writes names a file of your own rather than one among this
program's settings, credentials and session records. A `~` you type in the input box and a `~` the
planner writes stand for the same directory, and a machine naming no home refuses the `~` rather than
inventing one.

Everything else is refused, as an error naming the part of the line that caused it, and **a refusal
runs nothing**. There is no falling back to a shell and no running the prefix that did compile.

| Refused | Why |
|---|---|
| `$(…)`, backticks | a command whose text is computed is a destination nobody saw |
| `$VAR`, `${…}` | the value is not in the line, so the plan is not in the line |
| `$((…))` | arithmetic needs an interpreter |
| `<(…)`, `>(…)` | the same, plus a file descriptor nobody named |
| `&` | backgrounding is a parameter of a call, not a token in a line |
| `<<`, `<<<` | a here-document is content, not syntax |
| `eval`, `source`, `.`, `exec`, `trap` | they put an interpreter back in the plan |
| `if`, `while`, `for`, `case`, `function` | control flow is a program |
| `!` | history expansion is text you typed reaching a line the planner wrote |
| a pattern in program position | a program worked out from what is on disk changes when the tree does |
| anything that would open `/dev/tty` | the terminal is not this program's to hand over |

Quoted, every one of them is an ordinary argument: `'$HOME'` is six characters that reach the program
as one word.

## Your answer binds to the plan, not to the line

The prompt draws the plan: each step as the line wrote it, the binary that will actually run
underneath, the directory it runs in, and **every file the line would create or replace**, listed
rather than left to be worked out from the steps above. The line the planner wrote is shown above it
and marked as context. Compare the two to catch a compiler that read the line wrong, but the line is
not what you are agreeing to.

Two lines that compile alike are one thing to agree to. One approval cannot be reused for the same
steps joined differently, for the same steps writing somewhere else, or for the same steps in another
directory.

**Every branch is endorsed before anything runs.** `a && b` may run `b`, `a || b` may run `b`, and
`a ; b` will, so all of them are in the plan and all of them are approved up front. Nothing is put to
you part-way through a running line, where you could not tell what state the first half had left
behind.

## The answers, and how long each one lasts

```
  y run it    a always this session    r remember it    n don't    ctrl-c stop the turn
```

| Key | Lasts | Grants |
|---|---|---|
| `y` | this call | the line runs once |
| `a` | this session | the line runs unasked, **and** what it prints becomes readable |
| `r` | past the session | the line runs unasked, and what it prints stays quarantined |
| `n` | nothing | the line does not run |

`a` is [vouching](../security/permissions.md#vouching-for-a-command), and it is the only answer that
makes output readable, because that is an assertion about the command that only somebody looking at it
can make.

**`r` records that one exact command line** under `~/.bravebot`, keyed by the directory you were in.
Every session begun in that directory honours it from then on, including the one you pressed it in, and
the prompt shows what would be recorded and where before you agree. It stops the asking and nothing
else: a covered line still runs with its side effects, and what it prints is still quarantined. Press
`a` if you want to read the output.

What is recorded is the line and never a pattern: the program's name, the binary that name resolved
to, each argument as its own field, and where the output was sent. A later line is covered only when
every one of those is the same and the name still resolves to the same binary. Sending the errors
somewhere else makes a different line. Nothing in the record can mean "any text", so no answer here can
reach a second line.

A record is read only where a prompt could have been drawn. A [one-shot run](../using/headless.md) reads
none, and puts a covered line where it puts every other one. A tick of a
[`/loop`](commands.md#loop-interval-prompt) does draw its prompts to a live session, so pressing `r`
and then leaving a loop running overnight grants more than pressing it and staying.

## `a` is withheld where an entry would cover a line you did not read

At three kinds of prompt the `a` key is not offered at all, and an answer given there vouches for
nothing. A vouched entry records a program and its exact arguments, so wherever the line's meaning sits
outside those, an entry would cover a line nobody was shown.

| Withheld for | Because an entry would otherwise cover |
|---|---|
| a line reading a file in, `< secrets.txt` | the same program fed any other file |
| a line writing a file, `> out.txt` | the same program with the redirection gone |
| a line carrying `NAME=value` | the same program under no assignment at all |

The assignment is the sharpest of the three: it decides what a program loads before its own arguments
are read, so `LD_PRELOAD=./evil.so git log` is asked about however often `git log` was vouched for, and
what it prints is quarantined.

**A known cost.** `NO_COLOR=1 cargo test` and `RUST_LOG=debug ./demo` are ordinary work, and they are
asked about every time, in this session and the next. The spelling that can be remembered puts the
assignment where you can read it in the argv: `env NO_COLOR=1 cargo test` is a program called `env`
with three arguments, so vouching for it covers that line and no other. What is refused is a line whose
meaning is not in its argv, not the setting of a variable.

## A line that only reads what you vouched for does not ask

`wc -l Cargo.toml` in a directory you trusted runs unasked, and its output comes back as text rather
than as a reference. It cannot write anything, and it reads one file you already answered about, so
there was nothing for a prompt to decide.

That holds where every step of the plan is one of the audited programs (`wc`, `head`, `tail`, `cut`,
`tr`, `grep`, `basename`, `dirname`, `pwd`, `ls`, `cat`, `sort`, `uniq`, `diff`, `stat`, `du`), called with options that program's entry lists, and the
plan writes nothing and reads only paths the trust map answers for. The output's label is the map's
answer about what the line read. Where any path is one nobody vouched for, the output is untrusted and
private and the prompt stands, exactly as for every other run.

`sort` and `uniq` are in the list because the entry refuses what writes: `sort -o`, `--output`,
`-T` and `--compress-program` leave the step unproven, and so does a second operand to `uniq`, which
is the file it writes. `diff` compares two paths and `-r` is refused, since GNU `diff -r` follows a
link to a directory. `ls`, `stat` and `du` must name a path, because with none they report on the
working directory or, for BSD `stat`, on standard input, and `-L` and `-H` are refused because they
follow a link to a place the line does not name.

A recursive search answers for the whole tree it walks, so `grep -r` takes its label from the
directory it was pointed at and everything beneath it. A directory you refused inside a project you
vouched for decides the answer about that project.

The audit is per option rather than per program, and an entry lists the options a call may use rather
than the ones to reject. A list to reject fails open: `-S` makes BSD `grep` follow every symlink it
meets while walking, and `grep -A 1 -r TODO` walks the working directory while appearing to name a
path. An option the entry does not list leaves the step unproven, so the line is asked about as usual.

BSD and GNU versions of `head`, `tail`, `wc`, `cut` and `grep` read a word after the first operand
differently: BSD opens it as a file, GNU takes it as an option. A word spelled like an option after an
operand therefore leaves the step unproven, so `grep -r TODO src -n` is asked about, and
`grep -r -n TODO src` is not.

Five other things leave a step unproven. Naming the program by path rather than by name, since a file
called `wc` in the directory the line runs in would otherwise answer as the audited one. A name that
resolves to a file inside the workspace, for the same reason: a `PATH` entry in the project can put
one there. An
environment assignment, which decides what a program loads before its own arguments are read. A
redirection, which opens a file the argument list does not name. And running anywhere but the
directory the trust map's rules are written against.

**`git` is not audited and will not be.** A repository's own config can define an alias that runs a
command and a pager that runs a command, so `git log` is an interpreter whose program is a file in the
tree being inspected. `sed` and `awk` are out for the same reason: their program is an argument, and
`awk` can reach the shell this design excludes. All three stay on the vouching road, where a person
answers for them.

**This is not an allowlist.** A program that is not audited is asked about, never refused. A `deny` or
`ask` rule you wrote still decides, because proof removes the default prompt and does not overrule a
rule you wrote down. Private input still asks.

## Patterns become the files they match

Expansion happens against the tree at approval time, so what you read at the prompt is the file list
rather than the pattern. A pattern matching nothing is an error rather than an argument passed
through unchanged. `**` steps over the directories a listing steps over, so it does not descend into
`.git` or `node_modules`.

Expansion is bounded in both directions: a word standing for more than 100 arguments is refused with
the count, and the walk gives up after 4,096 directories. An approval prompt nobody reads grants
everything and asks nothing.

## A redirection is a write

`> out.txt` is a file this line creates or truncates. It appears in the plan's write set, it is shown
at the prompt, and it takes every rule a write takes: the permission rules, the trust map's answer
for that path, and the confinement that keeps a write inside the workspace. `>>` is a write, and
`2>&1` renames a stream and touches no file.

**`> /dev/null` discards, so it is not a write.** Sending a stream to `/dev/null` (with `>`, `>>`,
`2>`, `2>>` or `&>`) writes no file, so the path joins no write set and no write rule is asked about
it; the prompt still shows where the stream went. Only that exact spelling counts: `/dev/../dev/null`,
or a link to the device, is an ordinary path and is refused as one outside the workspace.

**A redirection also records what it wrote.** Where the line's output is untrusted, every file the line
opened for writing becomes untrusted, which is what stops a program's output being read back as
trusted. Where the output is trusted the map is left as it was, because `>>` keeps whatever the file
already held and vouching for a program answers for the program rather than for one file's contents, so
a line never raises a path's trust. What is recorded is what the line actually opened rather than the
write set, so a branch the line decided against leaves its destination alone.

The one direction has a cost: a file a vouched-for line overwrote stays untrusted until you say
otherwise, so reading it back is quarantined and costs a prompt.

**`< secrets.txt` is standard input, so that run asks every time.** The file's bytes go into a
program, which releases your data somewhere this policy stops governing, so a `<` meets the
private-input gate whatever the trust map says about the path, whatever you have vouched for, and
whatever a rule in the settings file allows. Any step's redirection counts, since a step in the middle
of a pipeline is handed the file the same way.

A target must compile to exactly one literal path. A pattern is refused even where it matches one
file today, because a destination worked out from what is on disk moves when the tree does, and the
plan would stop saying where the bytes go.

**A line that writes is put to you every time**, whatever you have vouched for and whatever you have
remembered. Vouching is keyed on a program and its arguments, and a destination is neither, so a
remembered command cannot pick one up unseen. Both redirections are among the prompts that
[offer no `a`](#a-is-withheld-where-an-entry-would-cover-a-line-you-did-not-read).

## Something that wants a terminal is refused before it starts

`git rebase -i`, `git add -i`, an editor, a pager without `--no-pager`: refused when the line is
compiled, with the thing to do instead. The list is a convenience rather than a guarantee. Something
interactive that is not on it reaches [the deadline](#a-line-has-a-deadline) and comes back with
what it printed.

Standard input is empty unless the line redirects a file into it or the planner named a reference, so a
step that reads it gets nothing rather than the terminal.

## The output

| | Label |
|---|---|
| the plan (programs, arguments, and the files it writes) | `(T,pub)`, because a person approves the compiled plan |
| standard input | may be untrusted; a person approves when it is private |
| standard output and error | `(U,priv)`, quarantined |
| …for a line every step of which a person vouched for, fed nothing untrusted | `(T,priv)` |
| …for a line that [proves what it read](#a-line-that-only-reads-what-you-vouched-for-does-not-ask) | the trust map's answer about what it read, private |

**Output nobody vouched for is not shown to the planner.** It comes back as a reference, like a file
it may not read, and can be passed to `spawn_processor` or written to a file with `write_file`. It is
not capped, since none of it enters the conversation.

That result also tells the planner what would lift the quarantine, naming three things in the order
they apply: **this** result through [`read_output`](tools.md#read_output), the next one once a person has
vouched for every stage of the exact command, and a file through `read_file`. Only where a command
produced it, so a quarantined *read* carries no advice about vouching for a command nobody ran.
Without those a planner reads one quarantined result as proof that programs are unreadable and stops
running them, which is not what happened: the label is about who answered for the command.
[Bypassing with no screening](../security/permissions.md#bypassing) changes the advice, since nobody is
shown the output and a run the mode approved vouches for nothing: the planner is told that
`read_output` hands it back as text it can read, and, on a run's result, that `read: true` returns it
in the same result. The advice does not name the mode, which the planner is told only in plan mode.

A command that printed nothing, such as a `mkdir`, has nothing to keep back. The planner is told it
printed nothing, with no reference and no advice, and the transcript does not mark it as kept from
the planner. That is decided from the size of the output alone.

**Every result says how the run ended**, in front of what the program printed: that every step
exited zero, which step did not and with what code, or that the line outstayed
[its deadline](#a-line-has-a-deadline) and was stopped. It is said for quarantined output too,
where the planner holds a reference it may not read. The verdict is read off the processes and the
clock, never out of a byte the program printed, so it is structure exactly as a line count is and
puts nothing in the planner's context that a program chose.

Output the planner **may** read comes back as text, capped at 16 KiB unless
[`run.maxOutput`](../customize/configuration.md#runmaxoutput) names another figure. Past the cap the
head and the tail are kept and the middle dropped, with a line in between saying how much went and
the byte it starts at. The cap is on what enters the conversation rather than on what the command
printed, and the whole of it stays available as a reference, which
[`read_output`](tools.md#read_output) reads back a page at a time without asking you.

## Reading the output in the same result

`read: true` asks for what the line printed in the result that ran it. When you
[bypass permissions](../security/permissions.md#bypassing) and have not asked for screening,
`read_output` is always answered yes and nobody is shown anything, so the answer is given at once: the
output comes back as text the planner may read, with the same label and the same entry in the audit
trail that `read_output` would have written, and the reference it was kept under beside it. That saves
the model round a `read_output` call would have cost. Output longer than the
[`run.maxOutput`](../customize/configuration.md#runmaxoutput) cap is the exception: the planner asked
before it could see the size, so it gets the reference, which states the size, and is told the output
was too long for one result. It is told where the rest is too: `read_output` hands back the whole of
it, and a filter such as `tail` given the reference as `stdin_ref` reads part of it without the line
being run again.

In every other mode `read` changes nothing: the output is quarantined as usual, and you are asked, or
a check reads it, only when the planner calls `read_output`. The same holds for an
[agent definition](../customize/agents.md) whose `tools` leave out `read_output`. `read` must be
`true` or `false`, and it is refused beside `background: true`, since the result that starts a job
holds nothing the job has printed.

## Filtering something the agent may not read

`stdin_ref` names a reference, and its contents are fed to the first program's standard input. That
is how `sed`, `awk`, `grep` or `jq` are run over a fetched page or a quarantined result: the agent
names the reference it was given, the bytes go into the program, and nothing along that path reads
them. The answer comes back quarantined the same way any other run's output does, so it can be
written to a file, handed to a processor, or shown to you with
[`read_output`](tools.md#read_output).

The run prompt names the reference beside the plan, because what a program is fed is as much a part
of what you are approving as what it runs. Where those bytes are **yours** (the output of an
earlier command, a file out of your workspace) feeding them to a program releases them somewhere
this policy stops governing, so the prompt says so and asks every time, whatever is on the vouched
list ([`a` is withheld](#a-is-withheld-where-an-entry-would-cover-a-line-you-did-not-read)).

**Vouching for a filter does not make a page trustworthy.** A program prints what it was given, so
a line fed content nobody vouched for comes back quarantined even where every step of it is a
command you vouched for. Otherwise a single `a` at a `sed` prompt would be a way to read any
quarantined document as trusted text, which is the thing the split exists to prevent.

`stdin_ref` and a `<` redirection are two ways to fill one standard input, so a line may have one
or the other and not both, and a background line may have neither: nothing is waited for there and
nothing is fed either.

## What a program is handed

A step gets the environment bravebot is running in, **less the credentials bravebot authenticates to
its own backend with**: `SERVICES_KEY_AICHAT` and `BRAVE_SERVICES_KEY_ID`. Every step, not only the
first, and removed rather than blanked, so a program that tells an unset variable from an empty one
sees what a machine that never held the credential sees. You approve the plan, the resolved binaries
and the directory. The environment is not among them.

**The rest of your environment stays, and that is not an oversight.** `run aws s3 ls` and `run gh pr
list` are ordinary requests, and no rule matching variable names can tell one of those from an
exfiltration, so `AWS_PROFILE`, `GITHUB_TOKEN` and `NPM_TOKEN` are left where they are. What the run
prompt tells you about the remainder is the truth: a run has the access your own shell has. Name
anything of your own you want withheld in
[`run.scrubEnv`](../customize/configuration.md#runscrubenv).

:::note
**This is not confinement.** A program that reaches the network is unpoliced and can send anything it
can read: a file, the workspace, a credential of your own. What closes here is the narrow part of the
gap, the credentials you could not have been shown at the prompt and had no way to withhold. Nothing
is established about what the program then does, and the label on its output is unaffected.
:::

A line you typed yourself in [shell mode](../using/shell-mode.md) is not this and keeps your whole
environment, since it is meant to behave as your own terminal does.

## A line has a deadline

Every line is given 300 seconds unless the call names its own. `deadline_seconds` raises or lowers
that, and is held between 1 and 600: a value outside those becomes the nearer of the two, and one that
is not a whole number of seconds is refused before anything runs. It has no effect with
`background: true`, which is not waited for at all.

**Both figures are yours to name.** `run.defaultSeconds` and `run.maxSeconds` in a settings file say
what a line with no deadline of its own gets and the most one may ask for; see
[configuration](../customize/configuration.md#rundefaultseconds-and-runmaxseconds). Raise the first
where your build takes longer than five minutes, so a line the agent did not think to put a deadline
on is not stopped partway through. The agent is told whichever figures are in force, so a ceiling you
raise is one it knows it may ask for.

When the deadline runs out the steps are killed, and **what they printed before that comes back exactly
as it would from a line that ended on its own**, under the same label. Reaching the deadline ends a run
rather than failing it, so a program that never exits, like a server told to serve a page, still gives
you everything it printed. The result says it was stopped, which is how you tell the two apart.

Being cut short neither raises nor lowers the label on the output. Finishing inside the deadline says
nothing about what a program did.

See [Vouching for a command](../security/permissions.md#vouching-for-a-command) for what `a` grants.

## Leaving a pipeline running

`background: true` starts the line without waiting for it and hands back a job name, which
[`job_output`](tools.md#job_output) reads. This is for the program no deadline can serve: a
server told to serve serves, prints as it goes, and never exits, so waiting for one and killing it at
the limit leaves no moment at which it is up and can be talked to.

**Every gate is the one a foreground run passes, at the same point.** The rules, your approval, and
the label the output will carry are all settled before anything starts. Being left running is not a
reason to ask for less.

`deadline_seconds` does nothing here, since nothing is waited for.

Anything a job wrote to standard error is marked as such wherever it is reported, so an error message
is not read as though the program had printed it as output. [`/jobs`](commands.md#jobs-stop-name-delegate)
lists the jobs a turn has started and stops one.

**One pipeline, and no redirection.** A line with `&&` or `||` decides where to go next by waiting on
the part before it, and nothing waits here; a redirection names a destination nothing is reading.
Both are refused rather than half-honoured.

**A job cannot outlive the turn that started it.** The turn owns the pipeline and ending the turn
kills it. A background program still running afterwards would be an effect nobody is watching,
nobody is being asked about, and nobody can stop. The terminal shows each job while it runs and says
when the turn stopped one ([Moving a command to the
background](../using/interactive-mode.md#moving-a-command-to-the-background)).

## Moving a running command to the background

A line the turn is waiting on can be moved to the background by the person at the terminal, with the
key the hint line names (`ctrl-b` unless you [moved it](../using/interactive-mode.md#moving-a-key)).
Only the interactive terminal client has the key: one-shot mode and the desktop app wait as before.
The command is not stopped or started again. It becomes a job exactly as if it had been started with
`background: true`, and the agent's result for the call says the user moved it, after how many
seconds, and under which job name. Nothing it printed is in that result, including what it printed
before the move: [`job_output`](tools.md#job_output) reads all of it, under the label the line was
always going to carry.

**Only a line a job can hold is offered.** That is one pipeline with no redirection, the same lines
`background: true` accepts. A line that reads its input from an earlier result and one a delegate is
running are waited for to the end, and the hint does not name the key while they run. A line that
asked for its output with `read: true` can be moved: the result for the call is then the move, and
[`job_output`](tools.md#job_output) returns the output.

**The deadline goes with the wait.** A moved line is not killed when its `deadline_seconds` would
have run out, because a deadline is how long the turn will wait and the turn is no longer waiting.

**The turn still owns it.** Ending the turn kills it, as it kills every job. Stopping the turn with
Ctrl-C at the moment of the press stops the command: a stop always wins over a move.

The audit trail records the move as a `handoff` entry naming the job and when it happened, so a
command that went on without a deadline is shown to be your choice and not the agent's.
