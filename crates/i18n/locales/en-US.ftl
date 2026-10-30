# The reference catalog. It owns the set of messages and the name and kind of every
# argument, so a translation can add none of its own and change no call site.
#
# Ids are kebab-case and grouped by the surface they appear on. A message is named for
# what it says, never for where it happens to sit, so moving a line between two panels
# does not rename it.
#
# What is not here: anything the planner reads. A tool's description, the preamble, and
# the sentence a refused tool answers with are interface to a model rather than prose for
# a person, and translating them would change what the agent does.


## Counting

count-turns = { $count ->
    [one] { $count } turn
   *[other] { $count } turns
    }


## Starting up, and the words printed before an interface exists

cli-tagline = bravebot { $version }: a general-purpose agent resistant to prompt injection
cli-usage-heading = Usage:
cli-usage-interactive = Start an interactive session
cli-usage-interactive-prompt = Start an interactive session and send the prompt as its first turn
cli-usage-plain = Start a session in lines, taking nothing from the terminal
cli-usage-task = Run a single task
cli-usage-piped = ...with piped input, never trusted
cli-usage-resume = Choose a session to pick up
cli-usage-from-pr = Choose among the sessions linked to a pull request
cli-usage-continue = Pick up the most recent session in this directory
cli-usage-resume-task = Send a one-shot task as the next turn of a session
cli-usage-continue-task = Send a one-shot task as the next turn of the most recent session
cli-usage-fork = Fork a session and start exploring a different path
cli-usage-doctor = Check configuration and confinement
cli-usage-doctor-sandbox = Run everyday workflows under the sandbox and report which work
cli-usage-update = Print the command that updates this copy
cli-usage-bug-report = Write the version, what doctor reports and the newest log's name to a file to attach to a bug report
# Said when the retired `bravebot import-leo-creds` is typed, in place of starting a task named by that word.
cli-import-leo-creds-retired = bravebot no longer imports a Leo subscription from Brave. Premium requests use a key stored with `bravebot auth login brave`
cli-usage-import-providers = Import a model service Claude Code or opencode configured
cli-usage-auth-login = Sign in to a model service, listing every way when none is named
cli-usage-auth-logout = Forget the stored Brave premium key, a gateway key, or a ChatGPT or xAI sign-in
cli-usage-auth-status = Say whether a sign-in is usable, exiting 0 only if it is
cli-usage-mcp = Declare, list and approve MCP servers
cli-usage-completion = Print a shell completion script
cli-usage-acp = Speak the Agent Client Protocol on stdin and stdout, for an editor
cli-usage-shell-init = Print the shell hook that gives @bravebot the commands you ran
cli-usage-permissions = Report which permission rule decides a tool call
permissions-check-usage = permissions check takes a family and what to ask about: Read <path>, Edit <path>, Bash <program> [arguments], WebFetch <host or URL>, or Mcp <server:tool>
permissions-check-decision = decision
permissions-check-deny = deny
permissions-check-ask = ask
permissions-check-allow = allow
permissions-check-none = no rule matches, so the ordinary gates decide
permissions-check-rule = rule
permissions-check-file = file
permissions-check-reason = reason
permissions-check-granted = granted at the question for this workspace, written in { $path }
permissions-check-not-in-force = not in force: { $rule } in { $path } would allow this once it is granted at the question
cli-usage-sessions = List the sessions that keep running after the terminal closes, grouped by state on a terminal
cli-usage-sessions-stop = Stop one of them
cli-usage-sessions-list = List the sessions recorded for this directory
cli-usage-sessions-delete = Delete one recorded session, its trail and its search index
cli-usage-sessions-purge = Delete every session recorded for this directory
cli-usage-sessions-import = Copy sessions another agent kept for this directory
cli-usage-stats = Total the turns and tokens recorded sessions spent; takes --days <n>, --models and --project [<dir>]
cli-usage-bg = Start a session that keeps running after the terminal closes
cli-usage-attach = Join a background session's terminal
cli-usage-reply = Send a prompt to an idle background session

cli-keys-heading = Interactive keys:
cli-key-send = Send
cli-key-audit = Toggle the audit trail
cli-key-history = Walk back through sent prompts
cli-key-history-search = Search every prompt sent
cli-key-scroll = Scroll the transcript
cli-key-jump = Jump to the start or the latest
cli-key-cancel = Cancel a running turn, clear the input, or leave
cli-key-leave = Leave

cli-commands-heading = Interactive commands:
cli-name-a-file = Include a workspace file as trusted context

## A session in lines: no screen of its own, no colour, nothing repainted

cli-plain-opening =
    bravebot { $version } in lines, { $model }. A line is a prompt; the end of the input
    (Ctrl-D) ends the session.
# Said where `--plain` was given with something other than a terminal on stdin. The lines it reads
# are prompts, and nothing vouches for what a pipe carries.
cli-plain-needs-a-terminal =
    --plain reads what you type, so its input must be a terminal. Use -p to run one task
    with piped input, which is read as quarantined context.
# Said where --plain was given alongside another way of starting. It starts a session rather than
# describing one, so there is nothing for it to combine with.
cli-plain-takes-nothing-else =
    --plain starts a session and takes no other arguments. --incognito,
    --dangerously-skip-permissions, --settings and --agent go with it; everything else is another
    way of starting.
# Said where the startup question was not put because an earlier session here was answered yes and
# the yes was kept (TRUST-23). A session in lines has no slash commands, so the ways to be asked again are
# the ones it can name: the command in the interface that draws, or the lines in the file. The
# lines rather than the file, since a directory whose path is spelled alike shares the file.
# Said on stderr by a one-shot run that opens with a kept answer about its working directory
# or the root of the git worktree around it, naming the directory the answer was about.
cli-trusting-kept =
    trusting { $directory } (you trusted it { $when }; to be asked again, run
    /forget-trust in bravebot, or delete the lines naming it from { $path })

cli-plain-trusting-kept =
    trusting { $directory } (you trusted it { $when }; to be asked again, run
    /forget-trust in bravebot without --plain, or delete the lines naming it from { $path })
cli-plain-trusting-kept-root =
    trusting { $directory }, inside { $root } (you trusted { $root } { $when }; to be asked again, run
    /forget-trust in bravebot without --plain, or delete the lines naming { $root } from { $path })
# Said in a session in lines (CLI-14) after a yes is kept, and in the lines before the question. It
# has no /forget-trust of its own, so both name the command in the interface that draws.
cli-plain-trust-kept =
    trusting { $directory }, and later sessions started here will not ask; to take it back, run
    /forget-trust in bravebot without --plain, or delete the lines naming it from { $path }
cli-plain-trust-remember-where =
    To take it back, run /forget-trust in bravebot without --plain, or delete the lines naming
    this directory from this file:

# Said when a background session is started again and its earlier conversation was read back from
# its record (BG-1).
cli-plain-resumed = Continuing this session's earlier conversation ({ $count }).

## How much a session asks before it acts, drawn under the input box
#
# The markers are Claude Code's, and deliberately: somebody who has used one of these knows what
# ⏵⏵ means at a glance, and inventing our own would make a familiar thing need reading. Asking has
# a line only in a session that was started with the flag that skips permissions: there it is the
# answer to "did it stop skipping them?", and in any other session it is what has always happened.
mode-ask = ◇ asking before it acts
mode-accept-edits = ⏵ accept edits on
mode-plan = ⏸ plan mode on
mode-bypass = ⏵⏵ bypass permissions on
mode-auto = ⏵⏵ auto mode on

cli-options-heading = Options:
cli-option-file = Include a workspace file as context (repeatable)
cli-option-session-ref = Add the newest part of an earlier session of this directory to the task (repeatable)
cli-option-add-dir = Reach into a directory outside the working one (repeatable)
cli-option-trust-workspace = Trust the working directory for this run, as answering yes to the startup question does
cli-option-settings = Read this settings file for this run, above the ones found on disk
cli-option-run-network =
    Whether programs `run` starts may reach the network. closed denies it to every one except a package
    manager's fetch, git or gh with a remote operation, curl, ssh and a stage with a remote scope
cli-option-sandbox-allow-read =
    Let programs `run` starts read this path or glob, lifting a refusal of it. Settings: sandbox.filesystem.allowRead (repeatable)
cli-option-sandbox-deny-read =
    Refuse programs `run` starts reading this path or glob. Settings: sandbox.filesystem.denyRead (repeatable)
cli-option-sandbox-allow-write =
    Let programs `run` starts write this path, which they may read too. Settings: sandbox.filesystem.allowWrite (repeatable)
cli-option-sandbox-deny-write =
    Refuse programs `run` starts writing this path, a session directory included. Settings: sandbox.filesystem.denyWrite (repeatable)
cli-option-log-level =
    How much of a failure's shape is written to the diagnostic log: error (the default), info or debug.
    Hosts, statuses and counts only, never content; an incognito session writes none
cli-option-agent = Address every turn to this definition, as /agent does for one
cli-option-issue = Give the session it opens this issue link, as /issue does
cli-option-pr = Give the session it opens this pull request link, as /pr does
cli-option-system-prompt =
    Replace the opening sentence of the planner's system prompt for every turn. The rest of it stays
cli-option-append-system-prompt =
    Add this text to the planner's standing instructions for every turn, after AGENTS.md
cli-option-mode = turn (default) decides step by step; manifest plans the whole run first
cli-option-model = The model this run asks for, in place of the remembered or configured one
cli-option-advisor = A model the agent may put a question to, offered to it as the advisor tool
cli-option-processor-model = The model processors run on, in place of the processorModel setting. inherit runs them on this run's model
cli-option-tools = Offer the agent only these tools, comma separated. It cannot add one a setting removed
cli-option-no-shell = Offer the agent no tool that runs a program or reads what one printed
cli-option-effort = How hard this run asks the model to think, in place of the remembered or configured level
cli-option-print = Non-interactive. Reads piped stdin as quarantined context
cli-option-interactive = Open the interactive session and send this prompt as its first turn; --model and --effort may follow it
cli-option-trace = Print the audit trail
cli-option-json = Print one result object on stdout instead of the reply
cli-option-json-stream = Print one event per line on stdout as the run goes, then the result object
cli-option-input-format = Read turns from stdin as JSON lines when given lines (implies --json-stream)
cli-option-trust-input-lines = Treat the lines on stdin as your own words and not a program's (needs --input-format lines)
cli-option-output-schema = Require the reply to match the JSON Schema in this file; with --json the value is in "structured"
cli-option-goal = Work towards this condition: after each turn a judge says whether it holds, and the work goes back until it does; exits with status 7 if it never does
cli-option-max-rounds = Stop the run when the agent asks for more than this many rounds of tool calls, ending on status 8
cli-option-max-tokens = Stop the run before a request once this many tokens are spent, ending on status 8
cli-option-incognito = Write nothing to ~/.bravebot: no history, no session record, no preference
cli-option-safe =
    Load none of your own hooks, skills, definitions, MCP servers or AGENTS.md. Sign-in, model and permissions still apply
cli-option-locked =
    Like --safe, and also read no project or local settings file, and refuse --dangerously-skip-permissions
cli-locked-refuses-bypass =
    { $flag } is refused with --locked: a locked run never opens the bypass mode
cli-accept-edits-not-with-bypass =
    --accept-edits is refused with { $flag }: the second answers every question the first answers and more, so say which you mean
cli-accept-edits-needs-a-run =
    --accept-edits is for a one-shot run, such as -p "task". A session has the mode key
cli-auto-not-with-bypass =
    --auto is refused with { $flag }: the second answers every question the first answers and more, so say which you mean
cli-auto-not-with-accept-edits =
    --auto is refused with --accept-edits: auto also answers a write the way accepting edits does, so give only --auto
cli-auto-needs-a-run =
    --auto is for a one-shot run, such as -p "task". A session has the mode key
cli-option-vet =
    For this run, let a check answer: content it finds nothing in is promoted without asking you,
    and where nobody can be asked, anything else is kept back
cli-option-sandbox =
    How far the programs `run` starts may reach: strict, read-only, workspace-write or off. A managed file may set a
    floor this cannot go below
cli-option-dangerously-skip-permissions =
    Bypass all permission checks. Recommended only for sandboxes with no internet access
cli-option-dangerously-bypass-approvals-and-sandbox =
    Bypass all permission checks and confine no program, as --dangerously-skip-permissions with --sandbox off.
    Recommended only for machines you can lose
cli-bypass-and-sandbox-disagree =
    --dangerously-bypass-approvals-and-sandbox turns the sandbox off, and --sandbox { $mode } asks for it on. Give one of them
cli-option-accept-edits =
    For a one-shot run, write files without asking. Commands, fetches and a write that would
    create a credential are still refused
cli-option-auto =
    For a one-shot run, let a classifier answer a command the sandbox confines and the first fetch of
    a URL, and write files without asking. Anything the classifier does not allow is refused, and so
    is a write that would create a credential
cli-option-help = Show this message
cli-option-version = Show the version


## What a command-line run says when it cannot start

cli-unknown-option = unknown option: { $flag }
cli-completion-needs-a-shell = completion takes one of bash, zsh or fish
cli-shell-init-needs-a-shell = shell-init takes one of bash, zsh or fish
cli-acp-takes-nothing-else = acp takes no arguments
cli-acp-refuses-bypass = acp does not take --dangerously-skip-permissions or --dangerously-bypass-approvals-and-sandbox: an editor cannot be made to stop asking
cli-acp-unreadable-line = acp: unreadable line
cli-acp-sessions-still-working = acp: some sessions were still working when the process stopped
cli-bug-report-takes-nothing-else = bug-report takes no arguments
cli-update-takes-nothing-else = update takes no arguments
bug-report-no-state-directory = bug-report writes nothing in an incognito session or where there is no home directory
bug-report-not-written = the bug report was not written: { $problem }
cli-file-needs-a-path = --file requires a path
cli-session-ref-needs-an-id = --session-ref requires a session id
cli-resume-needs-an-id = --resume requires the id of a session when it goes with a task
# The flag is --resume or --continue, as typed.
cli-resume-not-with-a-manifest =
    { $flag } does not go with --mode manifest: a manifest run has no conversation to carry on from
cli-add-dir-needs-a-path = --add-dir requires an absolute path to a directory
cli-directory-ends-checkouts = { $directory } holds the working directory, so no delegate is given a checkout while it is open; start again without --add-dir { $directory } to have one
cli-settings-needs-a-path = --settings requires a path to a settings file
cli-settings-not-a-file = --settings names no file: { $path }
cli-run-network-needs-a-word = --run-network requires open or closed
cli-run-network-unknown = --run-network takes open or closed, not { $word }
# The flag is one of the four --sandbox-* flags, as typed.
cli-sandbox-flag-needs-a-path = { $flag } requires a path
cli-log-level-needs-a-word = --log-level requires error, info or debug
cli-log-level-unknown = --log-level takes error, info or debug, not { $word }
cli-tools-needs-a-list = --tools requires a comma separated list of tool names
cli-tools-unknown = --tools names { $name }, which is no tool. The tools are: { $known }
# The command is the first argument, one of this program's own subcommands.
cli-tools-not-for-a-command =
    --tools and --no-shell limit the tools a session or a task is offered, and { $command } starts neither
cli-tools-not-with-a-manifest =
    --tools and --no-shell do not go with --mode manifest: a manifest run's steps are planned and run from the plan, not chosen from a list of tools
cli-agent-needs-a-name = --agent requires the name of a definition
# The command is the first argument, one of this program's own subcommands.
cli-agent-not-for-a-command =
    --agent names the definition a session or a task works under, and { $command } starts neither
cli-agent-not-with-a-manifest =
    --agent does not go with --mode manifest: a manifest run plans every step before any runs,
    and a definition is addressed a turn at a time
# The flag is --system-prompt or --append-system-prompt, as typed.
cli-system-prompt-needs-text = { $flag } requires the text to use
# The flag is --system-prompt or --append-system-prompt, and the command one of this program's own
# subcommands.
cli-system-prompt-not-for-a-command =
    { $flag } gives words to a session or a task, and { $command } starts neither
cli-system-prompt-not-with-a-manifest =
    { $flag } does not go with --mode manifest: the planner of a manifest run does not read it
# Said where a run with -p is given a --agent name it did not resolve, with the names it did.
cli-agent-no-such-definition = there is no definition called { $name }; this run resolved { $names }
# The same, where the project holds definitions the run did not read. It gives a count and never a
# name, because a file name in an untrusted directory is untrusted content.
cli-agent-no-such-definition-unread =
    { $count ->
        [one] there is no definition called { $name }; this run resolved { $names }. 1 definition in .bravebot/agents was not read: -p asks no trust question, so it reads only ~/.bravebot/agents
       *[other] there is no definition called { $name }; this run resolved { $names }. { $count } definitions in .bravebot/agents were not read: -p asks no trust question, so it reads only ~/.bravebot/agents
    }
# Said where the agent setting names a definition this run or session in lines did not resolve. It
# goes on without one, so the reason follows (ADDRESS-13).
cli-agent-setting-gone =
    the agent setting names { $definition }, which was not resolved: going on without a definition
# Said when a session in lines started with --agent opens (CLI-17). It has no slash commands, so it
# names no way to address another definition. The model is the one the definition names.
cli-plain-working-under = every prompt is addressed to { $definition }
cli-plain-working-under-model = every prompt is addressed to { $definition }, which asks for { $model }
# The flag that asks to be asked about nothing, where a settings layer made that mode unreachable.
# The file is named because the flag is documented and works everywhere else, so a refusal without
# it sends somebody looking for a fault in the program.
cli-bypass-unreachable =
    { $flag } is refused: permissions.bypassUnreachable in { $path } makes
    that mode unreachable here. Remove it there, or run without the flag.
cli-sandbox-needs-a-mode = --sandbox requires one of { $names }
# The file that holds the floor is named, and the file that asked when there was one, because a
# refusal that names neither sends somebody looking for a fault in the program. $looser is one of the
# sandbox-looser- sentences. The pin is the mode suggested, since no mode is stricter than a pin of
# strict or of read-only.
cli-sandbox-refused-flag =
    --sandbox { $asked } is refused: { $pinned_in } sets sandbox.mode to { $pinned }, and a run may
    read and write no more than that. { $looser } Run with --sandbox { $pinned }, or without the flag.
cli-sandbox-refused-file =
    sandbox.mode { $asked } in { $asked_in } is refused: { $pinned_in } sets sandbox.mode to { $pinned },
    and a run may read and write no more than that. { $looser } Change it to { $pinned } there, or
    remove it.
# Why a mode is looser than the managed file's sandbox.mode pin. The modes are not a line, so a
# refusal says which of reading and writing the asked mode does more of.
sandbox-looser-reads = { $asked } reads more of the machine than { $pinned }.
sandbox-looser-writes = { $asked } writes more than { $pinned }.
sandbox-looser-both = { $asked } reads more of the machine and writes more than { $pinned }.
cli-sandbox-refused-network-flag =
    --sandbox { $asked } is refused: { $pinned_in } pins run.network to closed, and a program started
    with no sandbox is not held to that. Run with --sandbox workspace-write or stricter, or without the flag.
cli-sandbox-refused-network-file =
    sandbox.mode { $asked } in { $asked_in } is refused: { $pinned_in } pins run.network to closed, and
    a program started with no sandbox is not held to that. Change it there, or remove it.
cli-mode-needs-a-name = --mode requires one of { $names }
cli-model-needs-a-name = --model requires the name of a model
cli-advisor-needs-a-name = --advisor requires the name of a model
cli-processor-model-needs-a-name = --processor-model requires the name of a model
cli-advisor-not-with-a-manifest = --advisor cannot be used with --mode manifest, which runs its plan without a planner to ask
cli-effort-needs-a-level = --effort requires one of { $levels }
cli-goal-needs-a-condition = --goal requires the condition to work towards
cli-goal-not-with-a-manifest = --goal cannot be used with --mode manifest, which runs its plan once and has no conversation to send the work back into
cli-goal-impossible = the goal cannot be met: { $reason }
cli-goal-unreadable = the check did not answer with a verdict, so there is nothing to act on and the run stops
cli-goal-quarantined =
    this conversation has met untrusted content, so a verdict about it is not something this
    program may act on, and the run stops
cli-goal-stalled =
    the goal has sent the work back without a turn making a tool call, and has stopped sending it
cli-goal-failed = the goal could not be checked: { $problem }
cli-output-schema-needs-a-path = --output-schema requires the path of a JSON Schema file
cli-input-format-unknown = --input-format takes lines
cli-trust-input-lines-needs-lines = --trust-input-lines requires --input-format lines
cli-input-lines-not-with-incognito = --input-format lines does not go with --incognito: each turn is carried on from the session record, and an incognito run writes none
cli-input-lines-not-with-a-manifest = --input-format lines does not go with --mode manifest: a manifest run has one task and no turns to add
cli-input-lines-no-prompt = no prompt line arrived before the input ended
cli-input-lines-session-lost = the session this run carries on from cannot be read back, so the turn was not run: { $problem }
cli-input-lines-session-unsaved = the turn before this one wrote no session record, so there is nothing to carry on from and the turn was not run
cli-output-schema-not-with-a-manifest = --output-schema cannot be used with --mode manifest, which has a reply for each step and none for the run
cli-max-rounds-needs-a-count = --max-rounds requires a whole number from 1 to { $most }
cli-max-tokens-needs-a-count = --max-tokens requires a whole number of tokens, 1 or more
cli-limit-not-with-a-manifest = { $flag } cannot be used with --mode manifest, which plans the whole run first and makes no rounds of tool calls
cli-limit-not-with-a-goal = { $flag } cannot be used with --goal, which sends the work back for further rounds and would count the limit afresh in each of them
cli-output-schema-not-served = --output-schema cannot be used with { $model }, which cannot be asked for a reply that matches a schema
cli-output-schema-unreadable = cannot read the output schema { $path }: { $problem }
cli-output-schema-not-json = the output schema { $path } is not JSON
cli-output-schema-not-an-object = in the output schema { $path }, { $at } is not a JSON object
cli-output-schema-unsupported = the output schema { $path } uses { $keyword } at { $at }, which is not supported
cli-output-schema-malformed = the output schema { $path } gives { $keyword } at { $at } a value it cannot take
cli-output-schema-mismatch = the reply does not match the output schema at { $at }: { $problem }
cli-output-schema-rule-not-json = the reply is not a single JSON value
cli-output-schema-rule-type = the value is of another type
cli-output-schema-rule-enum = the value is not one of those listed
cli-output-schema-rule-const = the value is not the one allowed
cli-output-schema-rule-required = a required property is missing
cli-output-schema-rule-extra = a property the schema does not list is present
cli-output-schema-rule-length = the string is shorter or longer than allowed
cli-output-schema-rule-count = the array has fewer or more items than allowed
cli-output-schema-rule-range = the number is below or above what is allowed
cli-unexpected-argument = unexpected argument: { $argument }
cli-task-required = a task is required
cli-configuration-problem = configuration error: { $problem }
cli-workspace-problem = workspace error: { $problem }
cli-interface-problem = interface error: { $problem }
cli-directory-unknown = cannot tell which directory this is
cli-no-such-session = no session { $id } in this directory
cli-manifest-run = { $id } is a manifest run, so there is nothing to continue; this is what it did
cli-nothing-to-continue = no session to continue in this directory
cli-issue-needs-a-link = --issue requires one http or https address
cli-pr-needs-a-link = --pr requires one http or https address
# Said where --issue or --pr is given to a line that opens no interactive session: a task, a session
# in lines, a background session, or a command that starts none.
cli-links-need-a-session =
    --issue and --pr give the session they open its links, and this command line opens no interactive session
cli-from-pr-needs-a-value = --from-pr requires a pull request number or address
cli-interactive-needs-a-prompt = -i takes the prompt to send as the session's first turn
# Said where something other than --model or --effort follows the prompt of -i. The session it opens
# takes the flags every way of starting takes, which are already out of the line by then; what is
# left describes a one-shot run.
cli-interactive-takes-nothing-else =
    -i takes one prompt, quoted where it has spaces, and then --model and --effort, so '{ $argument }'
    is refused. --file and the other options for a task go with a one-shot run (-p).
# Said where -i came after a task's own options rather than first.
cli-interactive-not-with-a-task =
    -i opens a session and comes first, followed by its prompt and then --model and --effort. --file
    and the other options for a task go with a one-shot run (-p).
# Said where -i came after another way of starting a session, such as --resume or --fork.
cli-interactive-not-with-another-start = -i opens a new session and comes first, so it does not go with { $flag }.
# Said where -i was given with something other than a terminal on stdin or stdout, where there is
# nothing to draw the session on and nobody to watch the task.
cli-interactive-needs-a-terminal =
    -i opens a session on this terminal, so its input and output must be one. Use -p to run the
    task with nobody watching.
cli-fork-needs-a-name = --fork requires a session id
cli-piped-input-unreadable = warning: could not read piped input: { $problem }
cli-piped-input-too-large =
    piped input is larger than { $limit } MiB. Write it to a file and name that instead


## What a run says when no model service is configured

# Said instead of starting work at all. A session opened with nothing configured reads as the agent
# being poor rather than as nothing having been set up yet, so the first run says what to configure
# instead of starting work that has no service to do it.
onboarding-no-model = no model service is configured yet
# Said before the routes where Claude Code or opencode configures a service bravebot can use, or a
# running Ollama, LM Studio or llama-server serves one, and nobody was there to be asked about it: a
# one-shot run, --json, a pipe, or doctor.
onboarding-import-one =
    { $source } configures a model service bravebot can use: run `bravebot auth login import` in a terminal to import it.
# The same, where the one source is a server running on this machine: Ollama, LM Studio or
# llama-server.
onboarding-import-running =
    { $source } is running here with models bravebot can use: run `bravebot auth login import` in a terminal to import it.
# Said where the one source is an API key exported in the environment, which belongs to no program.
onboarding-import-keys =
    An API key for a model service bravebot can use is exported here: run `bravebot auth login import` in a terminal to import it.
onboarding-import-both =
    { $first } and { $second } each have a model service bravebot can use: run `bravebot auth login import` in a terminal to import them.
onboarding-import-three =
    { $first }, { $second } and { $third } each have a model service bravebot can use: run `bravebot auth login import` in a terminal to import them.
# Where more than three have one. The names arrive as one comma-separated list.
onboarding-import-many =
    { $sources } each have a model service bravebot can use: run `bravebot auth login import` in a terminal to import them.
# Said instead, where a service is configured and only the model in force is Brave's own. A
# settings block copied out of another tool names its models and names no default, so this is
# where somebody following that route lands, and what they have to do is name one of their own.
onboarding-name-a-configured-model =
    A service is configured, but the model in force is one of Brave's: name one of your own with the `model` key in ~/.bravebot/settings.json, or with --model on a one-shot run. `bravebot doctor` lists what each configured service offers.
# Said instead of the routes where a ChatGPT sign-in is stored and the model in force is one of
# Brave's. The sign-in is the service that is configured, so what is left is naming one of its
# models, which chatgpt-use-by-name says on the line after.
onboarding-chatgpt-signed-in = A ChatGPT sign-in is stored, but the model in force is one of Brave's.
# The same where an xAI sign-in is stored, which xai-use-by-name follows, and where both are stored, which
# accounts-use-by-name follows.
onboarding-xai-signed-in = An xAI sign-in is stored, but the model in force is one of Brave's.
onboarding-accounts-signed-in = A ChatGPT sign-in and an xAI sign-in are stored, but the model in force is one of Brave's.
onboarding-pick-one = Configure one of these, then run bravebot again:
onboarding-bedrock =
    AWS Bedrock, through your own account: put a `provider` block named `amazon-bedrock` in ~/.bravebot/settings.json, with its region and the models to offer.
onboarding-openrouter =
    OpenRouter, or any other OpenAI-compatible gateway: put a `provider` block named for it in ~/.bravebot/settings.json, with the variable that holds its API key and the models to offer.
# The ChatGPT route: a person types the command and signs in in the browser it opens.
onboarding-chatgpt =
    A ChatGPT Plus or Pro account, if you have one: run `bravebot auth login chatgpt` in a terminal and sign in in the browser it opens. Its models are then offered in /model as chatgpt/<model>.
# The xAI route: a person types the command and signs in at the address it prints, on any device.
onboarding-xai =
    An xAI SuperGrok subscription, if you have one: run `bravebot auth login xai` in a terminal and open the address it prints on any device. Its models are then offered in /model as xai/<model>.
# Last of the five, and said to be last, because these models are reached through Brave's AI
# gateway, which is still in development. It is offered rather than left out because it is the
# route a build made with the key set already takes.
onboarding-premium =
    Brave's premium service, if you have a key for it: run `bravebot auth login brave` in a terminal and type the key. Its AI gateway is still in development, so prefer one of the four above for now.
# Said after either, so it reads after the routes and after the one line alike.
onboarding-where-to-read =
    There are worked examples in https://github.com/brave/bravebot/blob/main/docs/getting-started.md#choosing-a-model-service


## What a finished one-shot run says beside the reply

cli-notice = note: { $notice }
cli-model-used = model: { $model }
cli-something-was-refused = note: a policy gate refused something during this turn
cli-resume-heading = Resume this session with:
# When /cd moved the session, the shell this is printed into is not where the record is, and
# --resume looks an id up under the directory it runs in.
cli-resume-moved = This session moved to { $directory }. Resume it from there with:


## Reporting configuration and confinement

doctor-configuration-ok = configuration OK
doctor-endpoint = endpoint
doctor-premium = premium
doctor-premium-absent = not configured
doctor-premium-keyed = { $url }, with a premium key
doctor-premium-unkeyed = { $url }, without a premium key, so requests go to the endpoint above
doctor-key-id = key id
doctor-model = model
doctor-model-chosen = { $model } (chosen with /model)
doctor-model-default = { $model } (default)
doctor-model-set-aside = { $model } (default, since { $pick }, chosen with /model, is not served by any configured service)
# The other reason a pick is not in force. Not the line above: a model the machine's layer refuses is
# one a configured service would have served, so saying nothing serves it would name the wrong fault.
# The line under this one names the file that refused.
doctor-model-refused = { $model } (default, since { $pick }, chosen with /model, is not requested on this machine)
doctor-key-name = key
doctor-key = { $key } (never transmitted)
# What would end each credential this build holds for itself: who issued it, the surface that
# revokes it, and anything minted from it that revoking it would not reach. Written down here
# because the moment somebody needs it is the moment it is too late to work out, and because the
# disposition people reach for, deleting the local copy, ends this machine's custody and nothing
# else. One line per credential, and both AWS arrangements where an account is configured, since
# which one a profile resolves to is the AWS CLI's answer and this report asks it only whether the
# account is signed in. One per gateway a settings file configured too, so each names the host that
# would end its token.
doctor-ends = ends
doctor-ends-signing-key =
    the signing key: issued by the Brave backend, which derives its copy from a master seed and this key id; ended only by retiring that id there and shipping another build, since one build's key is every install's
doctor-ends-aws-access-key =
    a long-lived access key: issued by AWS IAM to the user the profile names; ended with `aws iam delete-access-key`
doctor-ends-aws-session =
    a session credential: issued by AWS STS for the profile, and this program asks the AWS CLI for another as it builds each request, so the expiry ends that copy rather than this program's access; ended at its issuer, since `aws sso logout` clears this machine's copy rather than the session behind it, and the next one is minted from whatever the profile chains to, for as long as that lasts
doctor-ends-premium-key =
    the premium key: issued by Brave's premium service, which is also the only surface that revokes it; `bravebot auth logout brave` ends this machine's custody and leaves the key live there
doctor-ends-gateway-token =
    a gateway bearer token: issued by { $gateway }, which is also the only surface that revokes it; deleting it from the settings file, unsetting the variable or running `bravebot auth logout gateway` ends this machine's custody and leaves the token live there
doctor-ends-chatgpt-account =
    a ChatGPT sign-in: issued by OpenAI to the account that signed in, and the refresh token replaces the access token without asking anybody, so the access token's expiry ends that copy rather than this program's access; ended at OpenAI by signing out of the account or removing this program's access in its settings, since `bravebot auth logout chatgpt` ends this machine's custody and leaves the tokens live there
doctor-ends-xai-account =
    an xAI sign-in: issued by xAI to the account that signed in, and the refresh token replaces the access token without asking anybody. The access token was issued for 21600 seconds, observed once, and what xAI answers it past that was not observed, so its expiry is taken to end that copy rather than this program's access. The refresh token ends when xAI revokes it, which its revocation endpoint did once on 2026-10-26, a call this program does not make, since `bravebot auth logout xai` ends this machine's custody and ends nothing there
# Which tier the gate walk left a credential on, shown under the account of what would end it. One
# sentence per tier rather than per credential: the tier is where the walk stopped, and what is
# particular to a credential is the line above this one. Delegated is here although nothing stands
# there, so the first credential that does is not reported as the tier below it.
doctor-tier = tier
doctor-tier-delegated =
    delegated: nothing is held here, and something this program cannot impersonate decides each use and can refuse it
doctor-tier-granted =
    granted: a real secret, bounded before it was issued to what the issuer will accept it for, and enforced where this program cannot reach
doctor-tier-held-briefly =
    held briefly: a real secret whose lifetime, rather than its reach, is what its issuer enforces
doctor-tier-held =
    held: a permanent secret, bounded by the surface that revokes it and by nothing else
# How quickly a leak of a credential would be noticed and acted on, shown for the one arrangement
# whose bound is a window rather than a revocation. The figure is this deployment's judgement and
# not a fact about the credential: it decides whether the window is short enough for what the
# credential reaches, and it neither sets the tier nor moves it.
doctor-noticed = noticed
doctor-noticed-aws-session =
    within about { $minutes } minutes, and only where somebody is reading the account's trail: a call made with the session appears there rather than here, nothing on this machine watches for one, and ending it before its expiry is a request at its issuer
# Shown only for a credential something is minted from that ending it would not reach, because
# a line reading "nothing" for the other two is the one people learn to skip.
doctor-outlives = outlives
doctor-outlives-aws-access-key =
    a session credential STS already issued under that access key, which runs to its own expiry: deleting the key does not reach it
doctor-outlives-chatgpt-account =
    an access token OpenAI already issued from the refresh token: it was refused within seconds after the refresh token was revoked at OpenAI's revocation endpoint (one account, one revocation, a call this program does not make), and whether signing out of the account or removing this program's access in its settings ends it has not been observed
doctor-outlives-xai-account =
    an access token xAI already issued from the refresh token: one request made right after the refresh token was revoked at xAI's revocation endpoint, a call this program does not make, was accepted with an access token issued before it (one account, once, on 2026-10-26), and how long such a token keeps working was not observed, so it may work until it expires
# Whether a derived credential is bound to the party that presents it. Shown only for the
# credentials minted from something else. A bearer secret is usable by whatever holds a copy, and the
# two bearer sentences keep apart an issuer that offers no bound form from one nobody has asked.
doctor-binding = binding
doctor-binding-sender-constrained =
    sender-constrained: the issuer checks who presents it, so a copy taken from this machine is of no use elsewhere
doctor-binding-bearer-refused =
    a bearer secret: whatever holds a copy can use it until it expires, and its issuer offers no form that is bound to the presenter
doctor-binding-bearer-not-attempted =
    a bearer secret: whatever holds a copy can use it, and nobody has asked its issuer for a form that is bound to the presenter
# How each credential reached the tier it stands at: one line per gate its walk failed, naming
# the gate, whether the counterparty refused or nobody attempted it, and the condition that was
# not met. A tier on its own says where a credential stands and nothing about whether it could
# have stood anywhere else, and the two answers are what tells a fact about the world from a
# decision made here. The gate number is passed from the record, so a line cannot name a gate the
# walk did not fail. No line names a host: a walk is an account of an arrangement, and the address
# somebody acts on is on the 'ends' line above it.
doctor-dropped = dropped
doctor-dropped-refused = the counterparty refused
doctor-dropped-not-attempted = nobody attempted it
doctor-dropped-signing-key-nothing-decides-each-use =
    gate { $gate }, { $answer }: nothing the agent cannot impersonate decides each use, since the key signs the request digest in this process and nothing else is asked to sign one
doctor-dropped-signing-key-no-bound-fixed-before-issue =
    gate { $gate }, { $answer }: no bound on what the key may do is fixed before it is issued, since the backend derives its copy from a master seed and this key id and is asked for nothing narrower
doctor-dropped-signing-key-not-minted-for-one-step =
    gate { $gate }, { $answer }: it is not minted for one step, since it is baked into the build and one build's key is every install's
doctor-dropped-aws-access-key-nothing-decides-each-use =
    gate { $gate }, { $answer }: nothing the agent cannot impersonate decides each use, since this process signs each request with the key itself
doctor-dropped-aws-access-key-no-bound-fixed-before-issue =
    gate { $gate }, { $answer }: no bound on what the key may do is fixed before it is issued, since STS mints a session bounded by a policy AWS enforces and the agent cannot widen, and nothing here asks for one
doctor-dropped-aws-access-key-not-minted-for-one-step =
    gate { $gate }, { $answer }: it is not minted for one step, since the profile's key is used as the AWS CLI resolved it and IAM ends it only when somebody deletes it
doctor-dropped-aws-session-nothing-decides-each-use =
    gate { $gate }, { $answer }: nothing the agent cannot impersonate decides each use, since this process signs each request with the session credential itself
doctor-dropped-aws-session-no-bound-fixed-before-issue =
    gate { $gate }, { $answer }: no bound on what the session may do is fixed before it is issued, since it carries whatever the profile's role or SSO grant allows and nothing here asks STS to narrow it to this run
doctor-dropped-aws-session-renewable-without-authority =
    gate { $gate }, { $answer }: the agent renews it without further authority, since this program asks the AWS CLI for another session as it builds each request and the CLI mints it from whatever the profile chains to without asking anybody
doctor-dropped-gateway-token-nothing-decides-each-use =
    gate { $gate }, { $answer }: nothing the agent cannot impersonate decides each use, since the token goes in a header this process sends and no performer exists for the request
doctor-dropped-gateway-token-no-bound-fixed-before-issue =
    gate { $gate }, { $answer }: no bound on what the token may do is fixed before it is issued, since the block names a host and a variable and never an issuer, so there is nothing here to ask for a narrower one
doctor-dropped-premium-key-nothing-decides-each-use =
    gate { $gate }, { $answer }: nothing the agent cannot impersonate decides each use, since this process presents the key itself and nothing is asked to authorise the request
doctor-dropped-premium-key-no-bound-fixed-before-issue =
    gate { $gate }, { $answer }: no bound on what the key may do is fixed before it is issued, since the premium service issues one key and nothing here asks it for a narrower one
doctor-dropped-premium-key-not-minted-for-one-step =
    gate { $gate }, { $answer }: it is not minted for one step, since the key is whatever `bravebot auth login brave` stored, and it is held for the whole run
doctor-dropped-gateway-token-not-minted-for-one-step =
    gate { $gate }, { $answer }: it is not minted for one step, since the token is whatever the settings file carries, the variable holds or `bravebot auth login gateway` stored, and it is held for the whole run
doctor-dropped-chatgpt-account-nothing-decides-each-use =
    gate { $gate }, { $answer }: nothing the agent cannot impersonate decides each use, since this process presents the access token itself and nothing is asked to authorise the request
doctor-dropped-chatgpt-account-no-bound-fixed-before-issue =
    gate { $gate }, { $answer }: no bound on what the sign-in may do is fixed before it is issued, since nothing here asks OpenAI to narrow it to this run
doctor-dropped-chatgpt-account-renewable-without-authority =
    gate { $gate }, { $answer }: the agent renews it without further authority, since the refresh token replaces the access token without asking anybody
doctor-dropped-xai-account-nothing-decides-each-use =
    gate { $gate }, { $answer }: nothing the agent cannot impersonate decides each use, since this program holds the access token itself and nothing is asked to authorise a use of it
doctor-dropped-xai-account-no-bound-fixed-before-issue =
    gate { $gate }, { $answer }: no bound on what the sign-in may do is fixed before it is issued, since nothing here asks xAI to narrow it to this run
doctor-dropped-xai-account-renewable-without-authority =
    gate { $gate }, { $answer }: the agent renews it without further authority, since the refresh token xAI issued replaces the access token without asking anybody
# The ChatGPT sign-in, shown only where one is stored. A sign-in that cannot be read is shown on the
# same line with the sentence that names its remedy. The instant is the one the access token expires,
# written as 2026-10-22T21:05:27Z, and is the only part of the file shown: the file also holds the
# tokens and identifiers, which belong in no report people paste. A past instant is still signed in,
# since the token is replaced when it is next used.
doctor-chatgpt = chatgpt
doctor-chatgpt-signed-in = signed in, the access token expires { $expires }
# The xAI sign-in, shown on the same terms as the ChatGPT one: only where one is stored, an unreadable
# one with its remedy, and only the instant of the file. The host is the token endpoint's, which the
# refresh token is sent to and no other.
doctor-xai = xai
doctor-xai-signed-in = signed in, the access token expires { $expires } and is sent to api.x.ai alone, and the refresh token is sent to auth.x.ai alone
# Both are reported when both are reachable, so this names one of the two rather than the backend.
doctor-backend = offers
doctor-backend-bedrock = AWS Bedrock
doctor-backend-aichat = Brave Leo
doctor-backend-gateway = { $gateway } (gateway)
# Whether one was found, never the value: on this path it is a bearer token, and a diagnostic that
# printed one is a diagnostic people paste into issues.
doctor-gateway-token = found (never printed)
doctor-gateway-token-stored = stored by bravebot auth login gateway (never printed)
# The id is the provider block's.
doctor-gateway-token-absent =
    none found (set a variable its `env` names, or run bravebot auth login gateway { $id })
doctor-gateway-token-not-needed = none needed (the block names none)
# A header a gateway block named and this build does not send, with the name the file wrote and the
# reason. The name only: a header a gateway routes on is often a second credential.
doctor-header-reserved = { $name } in the provider block is not sent: this program sets that header itself
doctor-header-not-a-name = { $name } in the provider block is not sent: it is not a header name
doctor-header-not-a-value = { $name } in the provider block is not sent: a header value cannot hold a line ending or a control character
doctor-header-substitution = { $name } in the provider block is not sent: a variable reference is read as the whole value, not inside a longer one
doctor-gateway-keys = gateway keys
doctor-gateway-keys-unreadable =
    { $path } cannot be read, so no key in it is sent (bravebot auth login gateway leaves it as it is)
# The one line `doctor` leaves on stderr when what it printed on stdout ends the run in a failure,
# so the identifier is somewhere a log of the failure holds it (CLI-6).
doctor-ended = the report above holds a problem that ends this run in failure
doctor-gateway-models-absent = none configured (the gateway is asked what it serves)
doctor-gateway-models-compiled = { $models } (built in, since this service has no listing; name any other the same way)
doctor-region = region
doctor-profile = profile
doctor-profile-absent = default credentials
# Whether the AWS CLI hands this account a credential a request can be signed with now. Only the
# answer is shown, never the credential, and the command to run where there is none. The sign-in
# command is named only where a sign-in is what is missing: it cannot add a profile or install the CLI.
doctor-aws-session = session
doctor-aws-signed-in = signed in
doctor-aws-signed-out = not signed in (run `bravebot auth login bedrock`)
# $available is the AWS CLI's own profile names, comma-separated.
doctor-aws-no-profile = no such profile in the AWS CLI (it has { $available })
doctor-aws-no-profiles = no such profile, and the AWS CLI has none (run `aws configure sso`)
doctor-aws-no-cli = unknown (the AWS CLI is not installed)
doctor-aws-undecodable = unknown (the AWS CLI answered with something that is not a credential)
doctor-tiers = models
doctor-tiers-absent = none configured (set ANTHROPIC_DEFAULT_OPUS_MODEL)
doctor-settings = settings
doctor-settings-names = { $names }
doctor-settings-absent = no settings.json
# A pattern in run.scrubEnv and one variable name it selected on this machine, one line each. Names
# only: the values are what the list exists to withhold (RUN-28).
doctor-scrub-pattern = by pattern
doctor-scrub-pattern-matched = { $pattern } selected { $variable }, which no program this agent runs is handed
doctor-permissions = permissions
doctor-permissions-absent = no rules
doctor-permissions-count =
    { $count ->
        [one] { $count } rule
       *[other] { $count } rules
    }
doctor-permissions-unreadable = unreadable rule
# A key a skill file declared that nothing here reads. The skill is its file's path and the keys are
# that file's own words, joined with a comma, both from a source somebody vouched for. A line rather
# than a silence, so the next key somebody writes is not another quiet no-op.
doctor-skill-key-unread = unread key
doctor-skill-keys-unread =
    { $count ->
        [one] { $skill } declares { $keys }, which nothing here reads
       *[other] { $skill } declares { $keys }, none of which anything here reads
    }
# A file that configures a gateway names no variables, and reporting that as an absent file would
# describe a file the person is looking at.
doctor-settings-no-variables = settings.json, naming no variables
doctor-settings-layer = layer
doctor-settings-override = override
# Which file a name finally came from, where more than one set it. Somebody looking at a value they
# did not expect has three files to open otherwise.
doctor-settings-overridden = { $name } from { $path }
# A file that named vetting.auto and was not obeyed. Read from the home layer alone, so a
# checkout cannot stop somebody being asked, and a line that does nothing is worth saying so.
doctor-settings-ignored = ignored
doctor-settings-vetting-ignored =
    vetting.auto in { $path } is not obeyed: it is read from ~/.bravebot/settings.json only
# A provider block or a model key a layer that may not pick a backend wrote, named for the same
# reason: a checkout cannot choose where requests go, and a line that does nothing is worth saying so.
doctor-settings-provider-ignored =
    provider in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
# A gateway whose baseURL writes a ':' that no port a connection can use follows. The client would
# connect to the scheme's default port, so the entry is dropped. The port is the text written there,
# empty where nothing follows the ':'.
doctor-settings-provider-port-refused =
    provider { $id } is not used: its baseURL writes a port that is not a number up to 65535 (found "{ $port }")
doctor-settings-model-ignored =
    model in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-advisor-ignored =
    advisorModel in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-fallback-ignored =
    fallbackModel in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-summary-ignored =
    summaryModel in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-plan-ignored =
    planModel in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-processor-ignored =
    processorModel in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-commit-message-ignored =
    commitMessage in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-agent-ignored =
    agent in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-shared-skill-directories-ignored =
    sharedSkillDirectories in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-style-ignored =
    style in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-language-ignored =
    language in { $path } is not obeyed: it is read from ~/.bravebot/settings.json only
doctor-settings-language-unmatched =
    language { $language } in ~/.bravebot/settings.json names no catalog, so it chooses { $reference }; the catalogs are { $available }
doctor-settings-ask-idle-ignored =
    askIdleSeconds in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-references-ignored =
    references in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
doctor-settings-attribution-ignored =
    attribution in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
# retentionDays is read from the person's own file alone, since a checkout choosing when their
# records are deleted would delete them by being opened. A line that does nothing is worth saying so.
doctor-settings-retention-ignored =
    retentionDays in { $path } is not obeyed: it is read from ~/.bravebot/settings.json only
# The sweep that removes old session records, language server indexes and findings waits while the
# period it was asked for cannot be determined, and removes nothing, so a person who set a period
# is not swept on the built-in one instead.
doctor-retention = retention
doctor-retention-paused =
    nothing is removed for age until { $path } is readable and retentionDays in it is a whole number of days above zero
# A key that only ever refuses, spelled as something other than a boolean. It is read as absence, so
# the session is as permissive as one that named nothing, and nothing else would say so.
doctor-settings-narrowing-ignored =
    { $key } in { $path } is not a boolean, so it is read as absent and refuses nothing
doctor-run-network = run network
doctor-run-network-closed = closed, except for steps that fetch or reach a remote ({ $source })
doctor-settings-network-ignored =
    run.network "open" in { $path } is not obeyed: a checkout can close the network and never open it
doctor-settings-network-unreadable =
    run.network in { $path } is neither open nor closed, so it is read as absent
doctor-managed-network-unreadable =
    run.network in { $path } is neither open nor closed, so the network is closed
doctor-sandbox-filesystem = sandbox filesystem
# The key is allowRead, denyRead, allowWrite or denyWrite, the path is as written, and the source is
# the file that wrote it or the command line.
doctor-sandbox-filesystem-entry = { $key } { $path } ({ $source })
doctor-sandbox-filesystem-refused = { $key } { $path } is not in force: { $reason } ({ $source })
doctor-sandbox-filesystem-source-flag = a command-line flag
doctor-settings-sandbox-filesystem-ignored =
    sandbox.filesystem.{ $key } in { $path } is not obeyed: a checkout can refuse reach and never add it, so it is read from ~/.bravebot/settings.json, the file --settings names and the managed file only
doctor-settings-sandbox-misshapen =
    sandbox.filesystem.{ $key } in { $path } is not a list of strings, so it is read as absent
doctor-settings-sandbox-hosts-ignored =
    sandbox.network.{ $key } in { $path } is not obeyed: a checkout can refuse a host and never allow one, so it is read from ~/.bravebot/settings.json, the file --settings names and the managed file only
doctor-settings-sandbox-hosts-misshapen =
    sandbox.network.{ $key } in { $path } is not a list of strings (onUnlisted: ask or refuse), so it is read as absent
doctor-settings-git-identities-ignored =
    git.identities in { $path } is not obeyed: a checkout cannot choose which key a push offers, so it is read from ~/.bravebot/settings.json and the file --settings names outside the workspace only
doctor-settings-git-identities-misshapen =
    { $key } in { $path } is not read: git.identities maps a name <host>/<owner> to the path of a public key file
doctor-settings-git-identities-key-refused =
    { $key } in { $path } names { $value }, which is not a key a push can be given: it has to be a regular *.pub file outside every credential location but ~/.ssh, such as ~/.aws, with no ', % or $ in its path, so ssh offers its default key for that owner
doctor-managed-sandbox-misshapen =
    sandbox.filesystem.{ $key } in { $path } is not a list of strings, so it pins nothing
doctor-managed-sandbox-unread =
    sandbox.filesystem.{ $key } { $path } is not read: { $managed } pins that list
sandbox-rule-no-home = it starts with ~ and this session names no home directory
sandbox-rule-climbs = it climbs out of the directory it is read from, or holds .. where it cannot be judged
sandbox-rule-glob-on-a-write = a wildcard applies to reads and not to writes
sandbox-rule-confines-nothing = a write row over the home directory or the whole filesystem confines nothing
sandbox-rule-private-key = no list adds reach to ~/.ssh, where a private key is
sandbox-rule-state-directory = no list adds reach to ~/.bravebot, where the gateway keys are, except to read ~/.bravebot/sessions
sandbox-rule-holds-credentials = on this platform a row over a directory reaches everything inside it, so no list adds a directory that holds a credential location such as ~/.ssh, ~/.aws or ~/.bravebot
sandbox-rule-too-broad = its wildcard looked at more of the disk than a pattern may, so what it names is not known
sandbox-rule-overridden = another entry decides that path: a refusal at the same path, or one the managed file wrote
sandbox-rule-cannot-subtract = this platform cannot hold back a path inside a directory every stage is granted, so a stage is refused instead of started with the path reachable
# An allow rule a layer that may not grant one wrote. Named one at a time and with its file, for
# the reason the vetting line gives: a rule that looks like configuration and does nothing is the
# one worth saying out loud.
doctor-settings-allow-ignored =
    the allow rule { $rule } in { $path } is not granted: an allow rule answers a prompt, so a
    project's file proposes one and you grant it when a session starts
# The other answer for the same rule: one somebody granted for this workspace is in force, and saying
# so is what keeps the line above readable as the exception rather than as the only outcome.
doctor-settings-granted = granted
doctor-settings-allow-granted =
    the allow rule { $rule } in { $path } is granted for this directory
# A top-level key beside the ones this build reads. The file is largely another tool's shape, so a
# pasted block holds keys written for that one, and a key that reads as a restriction and is never
# read is the one worth saying out loud. The key and the file, never the value.
doctor-settings-unread = unread key
doctor-settings-unread-key =
    { $key } in { $path } is not read by this build: it configures nothing and restricts nothing
# A settings layer that tried to declare an MCP server. Named by key and file only, since the entry
# may hold an argv and the values of variables.
doctor-settings-mcp-declared =
    { $key } in { $path } declares an MCP server, which only ~/.bravebot/mcp.json may: nothing in it
    is started
# A sandbox.mode a layer that may not loosen the sandbox wrote. Only `strict` is obeyed from a
# project's files, because a mode is a bundle of what a program may reach and a checkout that could
# pick a looser one would loosen it for whoever cloned it.
doctor-settings-sandbox-ignored =
    sandbox.mode { $mode } in { $path } is not obeyed: a project's file may ask for strict only, and the
    other modes are read from ~/.bravebot/settings.json and from the file --settings names only
# A `strict` from a file inside the workspace that is not obeyed, because the person's own file or the
# managed file chose a mode `strict` is looser than. `strict` writes the session's directories and
# `read-only` does not.
doctor-settings-sandbox-strict-ignored =
    sandbox.mode strict in { $path } is not obeyed: strict lets programs write the session's
    directories, which { $mode } from { $held } does not, and a file inside the workspace may not
    loosen the mode
doctor-settings-sandbox-unreadable =
    sandbox.mode in { $path } is not strict, read-only, workspace-write or off, so it is read as absent and the default applies
doctor-sandbox-mode = sandbox mode
doctor-sandbox-default = { $mode } (the default)
doctor-sandbox-from = { $mode } from { $path }
# The machine-level layer, above everything a person can set. The names rather than the values, for
# the reason the settings lines give, and the path because a pin somebody wants lifted is lifted by
# whoever can write that file.
doctor-managed = managed
doctor-managed-pinned = { $names } from { $path }
# A file somebody wrote that holds nothing this layer may pin. Reported, because the alternative
# leaves them unable to tell it from a file that was never found.
doctor-managed-nothing = { $path }, pinning nothing
# The heading over what `bravebot mcp list` reports, which `doctor` reports too.
doctor-mcp-servers = MCP servers declared in { $path }, for a session started in { $project }
doctor-mcp-none = no MCP server is declared in { $path }, for a session started in { $project }
# Which variable answered as well as where the directory is: more than one can name a profile
# directory, and the one that won is what somebody has to change to put the directory elsewhere.
doctor-state-directory = state directory { $path }, from { $variable }
# Where the diagnostic log goes, so a person filing a bug knows what to attach and where to look.
# The session writes nothing there when incognito, which `doctor` cannot know to say.
doctor-state-directory-logs = diagnostic logs { $path }
# What this program asks for as each file is created is a mode no other account can read. Where the
# platform is not told that, the files carry whatever the profile directory grants them instead. A
# prompt history holds every path, branch name and pasted fragment somebody has typed, so which of
# the two they have is theirs to know rather than a detail of the build.
doctor-state-directory-unprotected = not restricted
doctor-state-directory-permissions =
    prompt history, session records and saved choices carry your profile directory's permissions
# A value that is not an absolute path names no directory, so the state directory is the one the
# profile directory gives and the report above says `HOME` named it. Without this line somebody who
# set the variable and sees no effect has nothing to tell them it was read and passed over. Only the
# variable's name is said, as for the proxy variables, since a value may carry a credential.
doctor-state-directory-passed-over = passed over
doctor-state-directory-passed-over-detail =
    { $variable } is not an absolute path, so it names no directory
# What outlives a session is kept in this directory, so a machine without one keeps none of it, and
# nothing else in the report says so. Every variable that was looked at is named, since which ones
# they are is a fact about the platform rather than something the reader should have to know. The
# absence is partial: a checkout's own files are read as usual, and saying which half is lost is
# what stops this reading as "your AGENTS.md is ignored".
doctor-state-directory-absent = no state directory: { $variables } names nothing
doctor-state-directory-not-kept = not kept
doctor-state-directory-forgotten =
    sessions and --resume, prompt history, the model and theme you choose
doctor-state-directory-not-read = not read
doctor-state-directory-your-own =
    settings, skills and standing instructions of your own; a checkout's own still apply
doctor-state-directory-remedy = to keep them
# The remedy names the same variables the line above does. Naming one of them would send somebody on
# a platform that answers with the other to set the variable that was not going to be consulted.
doctor-state-directory-set-profile = set { $variables } to a directory of your own
doctor-confinement = confinement { $level }
# How much confinement was actually achieved. The sandbox reports which of the three it got and
# the interface is what names it, because bravebot-sandbox holds no words for a person.
confinement-kernel = kernel-enforced
confinement-partial = partial
confinement-none = none
# The platform level, then the sandbox mode a person chose. `off` says what it means because nothing
# else on the screen shows that a program `run` starts is not confined.
confinement-with-mode = { $level }, sandbox { $mode }
confinement-with-mode-off = { $level }, sandbox off: programs run unconfined
doctor-mechanisms = mechanisms
doctor-network-denial = network denial
doctor-kernel-enforced = kernel-enforced
doctor-not-enforced = NOT enforced
doctor-confinement-unavailable = confinement unavailable
doctor-no-network-namespace = network namespaces: unavailable ({ $reason }). A stage with no network, or held to sandbox.network.allowedHosts, is refused here. Allow unprivileged user namespaces to use either.
# Said only where the kernel cannot hold a connect to a unix socket that has a path to a stage's
# grants, which is Landlock below ABI 9.
doctor-sockets-with-a-path-reachable = unix sockets with a path: reachable. Landlock on this kernel is below ABI 9 (Linux 7.1), so a stage under sandbox mode workspace-write, and a local MCP server under any mode, reaches every such socket, the user's D-Bus, SSH_AUTH_SOCK and docker.sock among them. A stage under strict or read-only is refused here.
# The network a request actually crosses: which certificate authorities a handshake is validated
# against, and which proxy it is routed through. Both are stated outside this program, and neither
# is visible anywhere else when a connection fails.
doctor-network = network
doctor-trust-roots = trust roots
doctor-trust-roots-bundled = built in ({ $variables } names others)
doctor-trust-roots-named = { $paths }
doctor-trust-roots-none = nothing trusted, so every connection will fail
doctor-trust-roots-unusable = unusable
doctor-proxy = proxy
doctor-proxy-absent = none ({ $variables } names one, in upper case or lower)
doctor-proxy-in-force = { $proxy }
doctor-proxy-authenticated = { $proxy } (with a credential, never printed)
doctor-proxy-unsupported = { $protocol } is not supported by this build, so requests go direct
doctor-proxy-unparseable = not a proxy
doctor-proxy-unparseable-detail = { $variable } is set to something that cannot be read as a proxy address, so it is not the route (its value is never printed)
doctor-no-proxy = not proxied
doctor-client-certificate = client certificate
doctor-client-certificate-loaded = { $certificate }, key { $key } (loaded; the key is never printed)
doctor-client-certificate-refused = named and unusable, so every TLS connection will be refused
doctor-client-certificate-unusable = unusable
doctor-client-passphrase = key passphrase
doctor-client-passphrase-given = given in { $variable } (the value is never printed)
doctor-client-passphrase-absent = none given in { $variable }, so an encrypted key cannot be opened
doctor-network-file = stated in
doctor-network-file-detail = { $names } from { $path }
doctor-network-file-ignored = ignored
doctor-network-file-ignored-detail = { $name } in { $path } is a checkout's file, so it decides nothing


## A permission rule this build could not act on

# Said wherever a dropped rule is reported: by `doctor`, under the label above, and as a note in
# the session that read the file. The entry is quoted as the file spelled it, because finding it
# again is the whole point of being told.
permission-rule-unreadable = '{ $rule }' { $problem }
# The same, for a value that every settings file could have written, so the file is named with it.
permission-rule-unreadable-in = '{ $rule }' in { $path } { $problem }
permission-rule-not-a-line = is not a rule; a rule is written as a line of text
permission-rule-empty = is empty
permission-rule-unclosed-bracket = is missing its closing bracket
permission-rule-unknown-family = names no family of tools this agent has; use Read, Edit, Bash, WebFetch or Mcp
permission-rule-empty-brackets = has empty brackets; drop them to mean every use
permission-rule-empty-quoted-name = has double quotes around nothing, and no file has an empty name; drop the brackets to mean every use, or put the file's name between the quotes
permission-rule-whitespace-at-an-end = has whitespace at the start or end of its path, which would be part of a file's name; remove it, or put the whole path in double quotes to mean the name as written
permission-rule-unanchored = needs a home directory or a settings directory to say where it points
permission-rule-not-a-domain-rule = needs a domain, written WebFetch(domain:example.com)
permission-rule-no-domain-named = names no domain after 'domain:'
permission-rule-not-a-tool-rule = needs a server, or a server and one of its tools, written Mcp(weather) or Mcp(weather:get_forecast)
# A permissions block, or its deny, ask or allow list, written as something other than a list.
# The rules another settings file wrote still apply, which is what "removes none" says.
permission-rule-not-a-list = sets no rules and removes none; rules go in a list, such as "deny": ["Read(./.env)"]


# Said when a background job exits. The line drawn when it started said only that something had
# been started, and nothing else in the transcript ever says it is over, so a build that failed
# while the turn was doing something else would leave nothing on the screen about it. What it
# printed goes to the view a person can open; this is the sentence saying the thing has ended.
background-job-finished = `{ $command }` finished in the background: { $outcome }

# The heading over a message another session sent, where it reaches the planner of this one. The
# sender is named by the start of its id and by its name as the list of background sessions shows
# them, and the label is the one the message was given on arrival, which is the sender's word only
# where it was a label.
session-message-heading = message from session { $from } ({ $name }) · { $label }

# The heading over a message this session's planner sent to another session. The receiver is named
# as the list of background sessions names it, and the label is the one the message was sent under.
session-message-sent-heading = message to session { $to } ({ $name }) · { $label }

# Under the heading of a resumed session's message that the planner was not given the words of: a
# message whose label was not trusted reaches the planner as a reference, and the record keeps no
# content for it.
session-message-kept = its words were kept from the planner and are not in this record

# Said when a reply reached the output limit and the model is asked again rather than the turn
# ending. Which one follows from what the reply was doing when it stopped: a call it was writing
# was never made, so the work is asked for in parts, and a reply that wrote nothing is asked again.
ceiling-stop-in-call = the model reached its output limit of { $tokens } tokens while writing a call to { $tool }, so the call was not made; asking it to do the work in smaller parts
ceiling-stop-in-a-call = the model reached its output limit of { $tokens } tokens while writing a tool call, so the call was not made; asking it to do the work in smaller parts
ceiling-stop-thinking = the model reached its output limit of { $tokens } tokens while thinking, before it wrote anything; asking it again
ceiling-stop-silent = the model reached its output limit of { $tokens } tokens before it wrote anything; asking it again
# What a limit counts, as the noun the sentences below fill in.
limit-unit-tokens = tokens
limit-unit-credits = credits
# The question put when a session has spent as much as its limit allows.
spend-limit-header = Limit
spend-limit-reached = This session has spent { $spent } { $unit }, which reaches its limit of { $limit }.
spend-limit-how-to-raise = To go on under a new limit, answer in your own words with a figure in tokens, such as 2m.
spend-limit-how-to-raise-credits = To go on under a new limit, answer in your own words with a figure in credits, such as 200.
spend-limit-not-a-limit = { $text } is not a limit above the { $spent } { $unit } spent. Use a whole number, with k or m after it for thousands or millions.
spend-limit-stop = Stop here
spend-limit-without-a-limit = Go on without a limit
spend-limit-raised = the session limit is now { $limit } { $unit }
spend-limit-lifted = the session has no limit
spend-limit-stopped = stopped at the session limit: { $spent } { $unit } spent against a limit of { $limit }
# What a run says when a limit its person gave it ended it, before the round or request it would have made.
run-limit-rounds = stopped at the limit on rounds of tool calls ({ $most }): the planner asked for another and it was not run
run-limit-tokens = stopped at the limit on tokens ({ $most }): { $spent } were spent and the next request was not sent
repeated-call-header = Repeated call
repeated-call-reached = The planner has made the same { $tool } call { $count } times in a row. It was not run. Run it again, or choose another way forward.
repeated-call-refuse = Do not run it, and tell the planner to try another approach
repeated-call-run-it = Run it this once
repeated-call-stop = Stop the turn
repeated-call-refused = { $tool } was not run: the same call { $count } times in a row
repeated-call-stopped = stopped at a repeated call: { $tool } made { $count } times in a row
# Said when a turn's model kept failing and the person's fallbackModel takes over for the rest of it.
fallback-model-in-use = { $from } failed as { $category }; this turn moves to { $to }
# The same where the failure was the ChatGPT account's, which is said in its own sentence.
fallback-model-in-use-account = { $from } failed ({ $problem }); this turn moves to { $to }
# Said instead where the turn has no tools left, so the model is asked for an answer rather than
# for the work.
ceiling-stop-answer-now = the model reached its output limit of { $tokens } tokens; asking it for a shorter answer

# Said when a reply reached the output limit a second time with a call part written, so its text
# is the answer. The text arrives looking like the work was done, and the call it was writing is
# the part that was not.
ceiling-stop-ends-in-call = the model reached its output limit of { $tokens } tokens again while writing a call to { $tool }, so the call was not made and this answer stops where it did; raise BRAVEBOT_OUTPUT_BUDGET or ask for less in one turn
ceiling-stop-ends-in-a-call = the model reached its output limit of { $tokens } tokens again while writing a tool call, so the call was not made and this answer stops where it did; raise BRAVEBOT_OUTPUT_BUDGET or ask for less in one turn

# Said when a hook a person attached to a moment did not end well. Three sentences rather than one
# because what to do about each is different: a program that is not there is a path to fix, a
# non-zero status is the hook's own business, and one that was stopped was too slow to be run from
# a turn at all. Nothing a hook prints is read, so this is the whole of what can be said about it.
hook-not-started = the { $moment } hook `{ $program }` could not be started ({ $detail })
hook-failed = the { $moment } hook `{ $program }` did not end well ({ $status })
hook-stopped =
    the { $moment } hook `{ $program }` was still running after { $seconds } seconds and was
    stopped


## Importing a model service Claude Code or opencode configured, or a running Ollama, LM Studio or llama-server serves

# Said above everything an import would write, naming the files it was read from. Every name and
# value follows before the question, because what is written is what the person approves.
import-found = { $source } configures a model service bravebot can use, in { $files }.
# Where no file was read: the setup is exported rather than written down.
import-found-exported =
    { $source } configures a model service bravebot can use, in this process's environment.
# Where the source is a key exported in the environment: no file was read and no program is named.
import-found-keys =
    An API key for a model service bravebot can use is exported in this process's environment.
# Where the source is a server that answered rather than a file: Ollama, LM Studio or llama-server,
# at the address asked.
import-found-running = { $source } is running at { $url }, serving models bravebot can use.
# Under the model, where the running server stated the window it holds that model loaded with.
import-window = the server has it loaded with a context window of { $window } tokens
# Said where that window is below the one a conversation is otherwise budgeted against. The entry
# listed below it carries the figure as contextWindow, so a conversation is kept within it.
import-ollama-window-small = Ollama drops the earliest turns beyond that window, so the entry below records it as contextWindow and a conversation is kept within it rather than within { $assumed } tokens. Set OLLAMA_CONTEXT_LENGTH higher before starting Ollama to keep more.
import-lm-studio-window-small = A conversation longer than that window does not fit the model as LM Studio has loaded it, so the entry below records it as contextWindow and a conversation is kept within it rather than within { $assumed } tokens. Load the model with a longer context length in LM Studio to keep more.
import-adds = Importing it adds these to { $file }:
# One gateway, with the host its requests go to: that host is where a credential is sent, so it is
# the part of the entry the question is really about.
import-adds-gateway = provider.{ $id }, reached at { $endpoint }: { $entry }
import-key-held = provider.{ $id }: a key is held for it, which is asked about on its own
import-key-file =
    provider.{ $id }: its key is read from { $path }, which is not followed, so no key is written
# The command is never named or shown, since it can hold a key itself, and nothing here runs it.
import-key-command =
    provider.{ $id }: its key comes from a command, which is never run: export what it prints as { $variables }
import-kept = Left as they are, since { $file } already sets them:
# A model a Bedrock tier variable names, which the tiers answer for before any entry does.
import-named = Not added, since a tier already names them:
import-named-model = { $model } in provider.{ $id }, named by { $variable }
import-pinned = Not offered, since { $file } sets them for every user of this machine:
# Found and not imported, one line each, by name and reason and never by value.
import-left-heading = Found in { $source } and not imported:
import-left-anthropic-api = a setting for Anthropic's own API, which is not carried over: add a provider.anthropic block that names where the key lives
import-left-vertex = Google Vertex AI without a Google Cloud project, or through Google Cloud credentials, neither of which a service here reaches
import-left-bearer-token =
    a Bedrock API key; bravebot signs Bedrock requests through the AWS credential chain instead
import-left-no-region = Bedrock with no region to sign for
import-left-sign-in = a sign-in that belongs to opencode
import-left-another-sdk = an entry reached through an SDK other than an OpenAI-compatible one
import-left-no-endpoint = no reachable endpoint is stated or known for it
import-left-substitution =
    its key is built from an opencode substitution inside a longer value, which bravebot does not make
import-left-elsewhere = names an address other than this machine's loopback, which is not asked
import-left-no-tool-model = running there, with no model that can call tools
import-left-another-host =
    a key exported beside a host of its own, which is not offered at this service's usual address: add a provider block that states that host and names the variable
# What the environment is called inside a sentence, as the name of a place the import is read from.
import-source-environment = your environment
# A word like sonnet, which Claude Code resolves itself. Written as it stands it would name a model on
# Brave's roster instead of Anthropic's.
import-left-model-word =
    a word Claude Code turns into a model, which bravebot cannot do for Anthropic's API: set the `model` key, or export the key and start bravebot again to be asked which model to use
import-question = Import this from { $source }?
# Asked on its own, after the import is approved, and never showing the key.
import-key-question =
    Write the key for provider.{ $id } into { $file }, where it is kept in plain text, to be sent to { $endpoint }?
import-key-export =
    provider.{ $id } reads its key from { $variables }: export it before starting bravebot.
import-key-none = provider.{ $id } is written with no credential.
import-imported = imported what { $source } configured into { $file }
import-imported-running = imported what { $source } serves into { $file }
import-unset-variable =
    provider.{ $id } in { $file } reads its key from { $variables }, which is not set here: export it, then run bravebot again
# Said where the session opens anyway, because the model it runs on is served by another entry.
import-unset-variable-later =
    provider.{ $id } in { $file } reads its key from { $variables }, which is not set here: its models answer once it is exported
import-not-written = { $file } was not written: { $problem }
import-not-a-document =
    { $file } does not hold a settings document, so nothing can be imported into it without losing what it says
import-too-large =
    { $file } is past what bravebot reads, or would be with the import in it, so it was not written
import-changed =
    { $file } changed while the import was asking, so it was not written: run bravebot import-providers to ask again
import-needs-a-terminal = import-providers asks before it writes anything, so it needs a terminal to ask on
import-not-while-incognito = an import writes settings to disk, which an incognito session will not do
import-no-home = there is no home directory to write settings in
import-nothing-found =
    neither Claude Code nor opencode configures a model service bravebot can use, and no Ollama, LM Studio or llama-server serving one is running here
import-nothing-new = nothing is left to import: every name found is already set, or pinned
import-takes-nothing-else = import-providers takes no arguments


## Signing in to a model service, by any of the ways the program has

# Printed under a refusal that named no command, or one this does not have.
auth-forms-heading = bravebot auth takes one of:
auth-needs-a-command = bravebot auth needs a command
auth-unknown-command = bravebot auth has no command { $command }
auth-unknown-way = there is no way to sign in called { $way }
auth-unexpected-argument = { $command } does not take { $argument }
auth-needs-a-terminal =
    bravebot auth login asks which way to sign in, so it needs a terminal to ask on, or the name of a way
auth-logout-needs-a-way = bravebot auth logout needs the name of the way to sign out of
# The heading over the list. Each line under it starts with a number and a word, bedrock,
# import, gateway, chatgpt or xai, which a script types and which are not translated.
auth-ways-heading = Ways to sign in to a model service:
auth-way-brave = Brave's premium service, with a key typed here and kept by bravebot
auth-way-bedrock = An AWS account, for Amazon Bedrock
auth-way-import = A model service Claude Code, opencode, Ollama, LM Studio or llama-server has, imported into settings
# The services are the ids bravebot knows an endpoint for, such as openrouter, comma separated.
auth-way-gateway = A key for { $services } or a gateway a provider block in settings names, typed here and kept by bravebot
auth-way-chatgpt = A ChatGPT Plus or Pro account, signed in to in a browser
auth-way-xai = An xAI SuperGrok subscription, signed in to with a code entered on any device
# The status of the gateway way where keys are stored. The ids are provider ids.
auth-gateway-held = a key stored for { $ids }
auth-gateway-held-unreadable = the file of gateway keys cannot be read
# A way that is already signed in, with what it holds.
auth-way-held = { $description } ({ $status })
auth-signed-in = signed in
auth-which-way = Which one? Type its number or its name, or nothing to stop:
# Said where the answer to auth-which-way names none of the ways listed.
auth-not-a-listed-way = { $answer } is not one of the ways listed
# The region is AWS_REGION, and the tiers are the variables naming a model, such as
# ANTHROPIC_DEFAULT_OPUS_MODEL.
auth-no-aws-account =
    no AWS account is configured for Bedrock: set { $region } and a model in one of { $tiers }, or run bravebot auth login import where Claude Code or opencode uses one
# In every auth-bedrock message the switch is BRAVEBOT_USE_BEDROCK, and the file is the person's
# own settings file.
auth-bedrock-off =
    { $switch } is set to something other than 1, which turns Bedrock off, and no amazon-bedrock provider block names an AWS account
# The path is the machine-level settings file an administrator writes.
auth-bedrock-pinned-off =
    { $path } sets { $switch } for every user of this machine to something other than 1, which turns Bedrock off, and no amazon-bedrock provider block names an AWS account
auth-bedrock-recorded = { $file } sets { $switch }=1 now, so a session uses Bedrock without it being exported
auth-bedrock-not-recorded =
    { $switch }=1 was not recorded, so it still has to be exported for a session to use Bedrock: { $problem }
auth-bedrock-not-recorded-incognito =
    an incognito session records nothing, so { $switch }=1 still has to be exported for a session to use Bedrock
auth-bedrock-overruled =
    a settings file sets { $switch } to something other than 1, so a session uses Bedrock only where { $switch }=1 is exported
auth-bedrock-left =
    { $file } already names { $switch }, so it was left as it is, and a session uses Bedrock only where { $switch }=1 is exported or a project's settings set it
auth-bedrock-env-not-a-block = env in { $file } is not a block of names, so it was left as it is
auth-bedrock-settings-changed = { $file } changed as it was being written, so it was left as it is
auth-aws-profile-signed-in = the AWS profile { $profile } is signed in
auth-aws-default-signed-in = the default AWS profile is signed in
# The failure is what the AWS CLI or the check after it said.
auth-aws-profile-failed = the AWS profile { $profile } is not signed in: { $failure }
auth-aws-default-failed = the default AWS profile is not signed in: { $failure }
auth-aws-still-signed-out =
    aws sso login finished, and the profile still gives no credentials to sign a request with
auth-logout-bedrock =
    bravebot keeps no AWS session of its own: the AWS CLI keeps it, and aws sso logout ends it
auth-logout-import =
    an import keeps no credential of its own: it wrote entries to the settings file, and removing them there undoes it
# The id is the word typed after gateway. The word after it is not repeated, because it is most
# likely the key.
auth-gateway-key-argument =
    a key is never a command-line argument, where other programs can read it and the shell keeps it: run bravebot auth login gateway { $id } and type the key when asked
auth-gateway-extra-word =
    { $command } takes an id and nothing after it: a word there is not repeated, since it may be a key
auth-gateway-not-while-incognito =
    an incognito session writes nothing to disk, and storing a key is a write: run bravebot auth login gateway without --incognito
# The path is the machine-level settings file an administrator writes, which names no gateway.
auth-gateway-none-configured =
    { $path } names the gateways for every user of this machine and names none, so there is no gateway to store a key for
# The ids are the ones a key can be stored for: the gateways configured, then the services bravebot
# knows that no provider block names.
auth-gateway-not-configured = that is not the id of a gateway a key can be stored for; they are { $ids }
# As auth-gateway-not-configured, where the machine-level settings file names the gateways, so the
# services bravebot knows are not offered. The path is that file.
auth-gateway-not-configured-managed =
    that is not the id of one of the gateways { $path } names for every user of this machine, which are the only ones a key can be stored for; they are { $ids }
auth-gateway-needs-a-terminal =
    a gateway key is typed at a terminal with nothing drawn, so bravebot auth login gateway needs a terminal
auth-gateway-no-home = there is no home directory to store the key in
auth-gateway-keys-unreadable =
    { $path } is not a file of keys bravebot wrote, so it was left as it is and nothing was changed
auth-gateways-heading = Services a key can be stored for:
# As auth-gateways-heading, where the machine-level settings file names the gateways. The path is
# that file.
auth-gateways-heading-managed =
    The gateways { $path } names for every user of this machine, which are the only ones a key can be stored for:
# Beside a gateway in the list under auth-gateways-heading.
auth-gateway-key-stored = a key stored
auth-which-gateway = Which one? Type its number or its id, or nothing to stop:
auth-not-a-listed-gateway = { $answer } is not one of the gateways listed
auth-gateway-key-held = A key is already stored for { $id }.
# Said where no provider block names the id, so no session sends the key. Answering no keeps the
# key and has the block recorded.
auth-gateway-key-held-unrecorded =
    A key is already stored for { $id }, and no provider block names it, so no session sends it. Answering no keeps it and records the block in settings.
auth-gateway-replace = Replace it?
# Asked with echo off, so nothing appears as the key is typed or pasted.
auth-gateway-key-question = Key for { $id }, sent to { $host } (not shown as you type):
auth-gateway-nothing-stored = nothing was stored
auth-gateway-not-read = the key could not be read from the terminal: { $error }
auth-gateway-not-stored = the key was not stored: { $path }: { $error }
auth-gateway-stored =
    the key for { $id } is stored in { $path }, and sessions started from now on send it to { $host }
# Said where no provider block named the id, once the block is recorded for a key already stored.
# The file is the person's own settings file, and the host is the one compiled in for the id.
auth-gateway-recorded =
    { $file } names { $id } under provider now, so sessions started from now on send the key stored for it to { $host }
# Said where no provider block named the id, once its key is stored and the block recorded.
auth-gateway-stored-and-recorded =
    the key for { $id } is stored in { $path }, and { $file } names { $id } under provider now, so sessions started from now on send it to { $host }
# Said before the key is asked for, where a block naming the id would have to be recorded and the
# settings file cannot take one. The problem is one sentence about that file.
auth-gateway-cannot-record =
    a key for { $id } is sent only once a provider block names { $id }, and none can be recorded, so no key was asked for: { $problem }
# Said once the key is stored and the block naming its id could not be recorded. The entry is the
# text to add under provider by hand, and the problem is one sentence about the file.
auth-gateway-not-recorded =
    the key for { $id } is stored, and no session sends it until a provider block names { $id }: add { $entry } to { $file } ({ $problem })
# Where provider holds something that is not a block of entries, or an entry for the id that
# configures no service.
auth-gateway-entry-not-a-service =
    { $file } has a provider that is not a block of entries, or an entry for { $id } that configures no service
# The variable is one the provider block's env names.
auth-gateway-variable-wins =
    { $variable } is set, and while it is, a session sends its value instead of the stored key
auth-logout-gateway-none = no gateway key is stored
auth-logout-gateway-which =
    keys are stored for { $ids }: name the one to forget, as bravebot auth logout gateway <id>
auth-logout-gateway-not-stored = no key is stored for { $id }; keys are stored for { $ids }
auth-logout-gateway-not-written = the key was not forgotten: { $path }: { $error }
auth-logout-gateway-forgotten =
    the key for { $id } is forgotten here, and still works at { $host } until it is revoked there
auth-logout-gateway-forgotten-elsewhere =
    the key for { $id } is forgotten here, and still works at the service that issued it until it is revoked there

# bravebot auth status prints one line per sign-in, as "bedrock: signed in: <detail>". The detail is a
# count or a fixed sentence, never a credential.
auth-status-signed-in = signed in: { $detail }
auth-status-not-signed-in = not signed in: { $detail }
auth-status-unusable = unusable: { $detail }
auth-status-none-usable = no sign-in is usable
auth-status-not-all-usable = a sign-in asked about is not usable
auth-status-brave-held = a premium key is stored and is sent to Brave's premium service alone
auth-status-brave-none = no premium key is stored; run bravebot auth login brave
auth-status-brave-nowhere = this machine has nowhere to keep a key, so none is stored
auth-status-import = bravebot auth login import writes settings and keeps no sign-in of its own, so there is nothing to ask about: name brave, bedrock, gateway, chatgpt or xai
# Said of a gateway when no key is stored for any service and no provider block names a gateway.
auth-status-gateway-none = no gateway key is stored; run bravebot auth login gateway
# The id is a service bravebot knows that no provider block names, so no session sends a key to it.
auth-status-gateway-none-stored = no key is stored for { $id }; run bravebot auth login gateway { $id }
auth-status-gateway-unrecorded =
    a key is stored for { $id } and no provider block names it, so no session sends it; run bravebot auth login gateway { $id } to record the block
# The instant is the one the access token expires, written as 2026-10-22T21:05:27Z. The token is
# replaced when it is next used, so a past instant is still signed in.
auth-status-chatgpt-expires = the access token expires { $expires }
auth-status-chatgpt-none = no ChatGPT sign-in is stored; run bravebot auth login chatgpt
auth-status-chatgpt-nowhere = this machine has nowhere to keep a sign-in, so none is stored
# The instant is the one the access token expires, as for ChatGPT. The token is replaced when it is
# next used, so a past instant is still signed in.
auth-status-xai-expires = the access token expires { $expires } and is sent to api.x.ai alone, and the refresh token is sent to auth.x.ai alone
auth-status-xai-none = no xAI sign-in is stored; run bravebot auth login xai
auth-status-xai-nowhere = this machine has nowhere to keep a sign-in, so none is stored
auth-status-bedrock-good = the AWS session gives credentials to sign a request with
# The next four are fixed sentences because a line of `auth status` carries no text the AWS CLI
# chose: its error text names accounts and roles, and its profile list can embed an account id.
auth-status-bedrock-lapsed = the AWS session has lapsed: run `bravebot auth login bedrock`
auth-status-bedrock-no-profile = the AWS CLI has no profile by this name: name one it has in settings
auth-status-bedrock-no-cli = the AWS CLI is not installed, and Bedrock credentials come from it
auth-status-bedrock-undecodable = the AWS CLI answered with something that is not a credential


## Signing in with a ChatGPT account

chatgpt-not-while-incognito = signing in stores a token on disk, which an incognito session will not do
chatgpt-needs-a-terminal =
    signing in to ChatGPT shows an address to open and waits for the browser, so bravebot auth login chatgpt needs a terminal on standard error
chatgpt-held-needs-a-terminal =
    a ChatGPT sign-in is already stored and replacing it needs an answer, so bravebot auth login chatgpt needs a terminal on standard input
chatgpt-name-is-a-gateway =
    a provider block named chatgpt is a gateway, and one name cannot select two services: rename the block, then run bravebot auth login chatgpt again
chatgpt-held = A ChatGPT sign-in is already stored.
# Beside the way in the list of ways, where the file a sign-in is kept in cannot be read.
chatgpt-held-unusable = a stored sign-in cannot be read
chatgpt-replace = Replace it?
chatgpt-open-this = Open this address in a browser to sign in to ChatGPT:
chatgpt-waiting = Waiting for the browser on 127.0.0.1, port { $port }. A browser on another machine reaches it only through a forwarded port.
chatgpt-stored = signed in to ChatGPT; the access token expires { $expires }. The sign-in is stored in { $path }
# What the sign-in command ends with where the account lists models and a start would ask which one to
# use (account-start-question): running bravebot is the way to choose. The commands name a model together
# with the prompt or the task that starts something.
chatgpt-use-by-asking = Run bravebot to be asked which of the account's models to use, or name one, as in `bravebot -i "<prompt>" --model chatgpt/<model>` or `bravebot "<task>" --model chatgpt/<model>`. Once a session is open, /model lists them.
# The same where a start would not ask about the account, because a service of the person's own is
# configured, whose models a start asks about instead, or a model is already in force: nothing is said of
# being asked.
chatgpt-use-by-naming = Name one of the account's models, as in `bravebot -i "<prompt>" --model chatgpt/<model>` or `bravebot "<task>" --model chatgpt/<model>`. Once a session is open, /model lists them.
# What a refusal says. A refusal is also what a one-shot run, a start that names a model and `doctor` end
# in, and none of those asks, so the last sentence says which start does: one on its own. Both commands
# name a model together with the prompt or the task that starts something, since
# `bravebot --model chatgpt/<model>` alone starts nothing. /model is reached once a session is open.
chatgpt-use-by-name = Name one of the account's models, as in `bravebot -i "<prompt>" --model chatgpt/<model>` or `bravebot "<task>" --model chatgpt/<model>`. Once a session is open, /model lists them. Started on its own, in a terminal outside an incognito session, bravebot asks which one to use when the account lists its models.
# The same for an xAI sign-in, and for both signed in, where each command names a model of one account.
xai-use-by-name = Name one of the account's models, as in `bravebot -i "<prompt>" --model xai/<model>` or `bravebot "<task>" --model xai/<model>`. Once a session is open, /model lists them. Started on its own, in a terminal outside an incognito session, bravebot asks which one to use when the account lists its models.
accounts-use-by-name = Name one of the accounts' models, as in `bravebot -i "<prompt>" --model chatgpt/<model>` or `bravebot "<task>" --model xai/<model>`. Once a session is open, /model lists them. Started on its own, in a terminal outside an incognito session, bravebot asks which one to use when an account lists its models.
# What the command says after the sign-in is stored when the account's listing answered with no model,
# could not be had, or was refused in a code that names the account. The sign-in is stored in each. Of the
# two for a listing that could not be had, the second is for an account whose picker offers the built-in
# list in its place.
chatgpt-reaches-no-model = This ChatGPT account reaches no model through bravebot, so /model offers none. The sign-in stays stored.
chatgpt-listing-unreadable = The account's models could not be listed. Name one, as in `bravebot -i "<prompt>" --model chatgpt/<model>` or `bravebot "<task>" --model chatgpt/<model>`
chatgpt-listing-compiled = The account's models could not be listed, and once a session is open /model offers a built-in list that may name a model the account cannot reach. Name one, as in `bravebot -i "<prompt>" --model chatgpt/<model>` or `bravebot "<task>" --model chatgpt/<model>`
chatgpt-listing-refused = The account's models could not be listed: { $refusal }
chatgpt-forgotten =
    forgot the stored ChatGPT sign-in. It stays valid at OpenAI until it is ended there: sign out of ChatGPT, or remove bravebot's access in the account's settings
chatgpt-nothing-to-forget = no ChatGPT sign-in is stored
# Why the sign-in stopped. Each is chosen from the step that failed and, for a failed request, the
# class of its status. None repeats anything the browser, OpenAI or a settings file said.
chatgpt-failed-listener =
    could not listen for the browser on the loopback address, so no browser was opened and nothing was changed
chatgpt-failed-timed-out =
    the browser did not return to bravebot in time, so the sign-in stopped and nothing was stored; a browser on another machine reaches this machine's loopback port only if the port named above is forwarded to it
chatgpt-failed-no-code =
    the browser returned no authorization code, so the sign-in stopped and nothing was stored; run bravebot auth login chatgpt to try again
chatgpt-failed-no-client =
    the browser returned no usable client for bravebot, so the sign-in stopped and nothing was stored; run bravebot auth login chatgpt to try again
chatgpt-failed-client-changed =
    the browser returned a different client from the one stored, so the sign-in stopped and nothing was changed; run bravebot auth logout chatgpt and sign in again
chatgpt-failed-no-use-scope =
    the sign-in did not allow bravebot to use the subscription's tokens, so nothing was stored; run bravebot auth login chatgpt and allow it, or use an API key with the openai gateway
chatgpt-failed-id-token =
    the ID token the authorization server returned did not verify, so the sign-in stopped and nothing was stored; run bravebot auth login chatgpt to try again
chatgpt-failed-clock =
    the clock of this computer is set before 1970, so the sign-in cannot tell when the ID token expires and nothing was stored; set the clock, then run bravebot auth login chatgpt to try again
chatgpt-failed-key-set = the keys that verify the sign-in could not be fetched: { $reason }; nothing was stored
chatgpt-failed-exchange = the sign-in could not be completed: { $reason }; nothing was stored
chatgpt-class-refused = the authorization server refused the request
chatgpt-class-rate-limited = the authorization server is limiting requests; try again later
chatgpt-class-unavailable = the authorization server is unavailable; try again later
chatgpt-class-incomplete = the authorization server's reply was incomplete
chatgpt-class-transport = the authorization server could not be reached
# The detail is a path and what the operating system said about it.
chatgpt-account-nowhere = there is nowhere to keep a ChatGPT sign-in: { $detail }
chatgpt-account-unusable =
    the stored ChatGPT sign-in could not be read: { $detail }; run bravebot auth login chatgpt to sign in again, or bravebot auth logout chatgpt to forget it
chatgpt-account-malformed =
    the stored ChatGPT sign-in is unusable: { $defect }; run bravebot auth login chatgpt to sign in again, or bravebot auth logout chatgpt to forget it
chatgpt-account-unwritable = the ChatGPT sign-in could not be stored: { $detail }; nothing was changed
# Said once in the turn that renewed the sign-in, where the file could not take the renewal. It names no
# path and quotes nothing the file or the system said.
chatgpt-renewal-unwritten = the ChatGPT sign-in was renewed and could not be saved, so it lasts only until bravebot exits; check that the folder it is kept in has room and can be written to, and if the next start says the sign-in ended, run bravebot auth login chatgpt
chatgpt-defect-empty = the file holds nothing, which an interrupted write leaves behind
chatgpt-defect-not-json = the file is not valid JSON
chatgpt-defect-other-version = it was written by another version, and this build reads version 1
chatgpt-defect-missing-field = a field is missing or empty
chatgpt-defect-no-expiry = it has no expiry
# The two pages the browser is shown: after it returns to bravebot, and for any other request to the
# port. They are shown in the browser, not in the terminal.
chatgpt-page-done = You can close this tab and return to bravebot, which says whether the sign-in worked.
chatgpt-page-not-found = Not found.

## Signing in to Brave's premium service

# `bravebot auth login brave` asks for one key and keeps it in a file of its own. The key is never
# said back, and the premium host it goes to is compiled in.
brave-not-while-incognito = a premium key is kept on disk, which an incognito session will not do
brave-needs-a-terminal = bravebot auth login brave asks for the key without showing it, so it needs a terminal to ask on
brave-no-home = there is nowhere to keep a key: neither BRAVEBOT_HOME nor the profile directory names one
brave-key-unreadable = the premium key file { $path } cannot be read, so it is left as it is; delete it to store a key again
brave-key-held = a premium key is stored already
brave-replace = Replace it?
brave-key-question = Brave premium key (nothing is shown as you type):
brave-nothing-stored = no key was typed, so nothing was stored
brave-not-read = the key could not be read: { $error }
brave-not-stored = the key could not be stored in { $path }: { $error }
brave-stored = stored the premium key in { $path }; premium requests will now use it
brave-forgotten = forgot the stored premium key; it still works at Brave's premium service until it is revoked there
brave-nothing-to-forget = no premium key is stored
brave-not-forgotten = the stored premium key in { $path } could not be removed: { $error }
doctor-brave-key = premium key

## Signing in to xAI with a SuperGrok subscription

xai-not-while-incognito = signing in stores a token on disk, which an incognito session will not do
xai-needs-a-terminal =
    signing in to xAI shows an address and a code and waits for them to be used, so bravebot auth login xai needs a terminal on standard error
# The status of the xAI way in the list of ways, where a sign-in is stored and cannot be read.
xai-name-is-a-gateway =
    a provider block named xai is a gateway, and one name cannot select two services: rename the block, then run bravebot auth login xai again
xai-held-unusable = a stored sign-in cannot be read
# Said in four lines as the code arrives: this, the address, the code and the wait. The address and the
# code are the ones xAI's reply gave, shown only where they are printable and the address is https.
xai-open-this = Open this address on any device to sign in to xAI:
xai-enter-code = The code to enter, if the page asks for one: { $code }
xai-waiting =
    { $minutes ->
        [one] Waiting for the code to be used. It stops working in about 1 minute.
       *[other] Waiting for the code to be used. It stops working in about { $minutes } minutes.
    }
xai-stored = signed in to xAI; the access token expires { $expires }. The sign-in is stored in { $path }
xai-forgotten =
    forgot the stored xAI sign-in. This removes the copy kept here and ends nothing at xAI
xai-nothing-to-forget = no xAI sign-in is stored
# What stops a sign-in. None holds text taken from xAI's reply, and none says more of a failed request
# than the class of its status.
xai-failed-access-denied =
    xAI denied access, so nothing was stored; run bravebot auth login xai to try again
xai-failed-authorization-denied =
    xAI denied the authorization, so nothing was stored; run bravebot auth login xai to try again
xai-failed-expired =
    the code stopped working before it was used, so nothing was stored; run bravebot auth login xai to try again
xai-failed-timed-out =
    the code was not used within the time bravebot waits for one, so the sign-in stopped and nothing was stored; run bravebot auth login xai to try again
xai-failed-code = xAI did not give a code to sign in with: { $reason }; nothing was stored
xai-failed-exchange = the sign-in could not be completed: { $reason }; nothing was stored
xai-failed-clock =
    the clock of this computer is set before 1970, so the sign-in cannot tell when the code stops working and nothing was stored; set the clock, then run bravebot auth login xai to try again
xai-class-refused = xAI refused the request
xai-class-rate-limited = xAI is limiting requests; try again later
xai-class-unavailable = xAI is unavailable; try again later
xai-class-incomplete = xAI's reply could not be read
xai-class-transport = xAI could not be reached
# The detail is a path and what the operating system said.
xai-account-nowhere = there is nowhere to keep an xAI sign-in: { $detail }
xai-account-unusable =
    the stored xAI sign-in could not be read: { $detail }; run bravebot auth login xai to sign in again, or bravebot auth logout xai to forget it
xai-account-malformed =
    the stored xAI sign-in is unusable: { $defect }; run bravebot auth login xai to sign in again, or bravebot auth logout xai to forget it
xai-account-unwritable = the xAI sign-in could not be stored: { $detail }; nothing was changed
xai-defect-empty = the file holds nothing, which an interrupted write leaves behind
xai-defect-not-json = the file is not valid JSON
xai-defect-other-version = it was written by another version, and this build reads version 1
xai-defect-missing-field = a field is missing or empty
xai-defect-no-expiry = it has no expiry
# Why no access token could be had, in the words a turn will say it in. Signing in again is named
# where it is the remedy, and not for a renewal that may pass.
xai-not-signed-in = not signed in to xAI. Run `bravebot auth login xai` to sign in
xai-ended = the xAI sign-in has ended. Run `bravebot auth login xai` to sign in again
xai-renewal-failed = the xAI sign-in could not be renewed: { $reason }
# Said once, in the turn that renewed the sign-in, where the file could not take the renewal. xAI replaced
# the refresh token at a refresh once with one account, so the file may still hold one xAI no longer accepts.
xai-renewal-unwritten = the xAI sign-in was renewed and could not be saved, so it lasts only until bravebot exits; check that the folder it is kept in has room and can be written to, and if the next start says the sign-in ended, run bravebot auth login xai
xai-unusable = the stored xAI sign-in could not be read. Run `bravebot auth status xai` to see why
# Said when a turn's request was refused for its access token and refused again for the new one.
xai-token-refused = xAI refused the xAI sign-in's access token, and a new one as well. Run `bravebot auth login xai` to sign in again
# Said where the serving host refused a request with a 403 after the access token was replaced. A first 403 is
# how it refused a token it could not validate, so a new token is asked for first. No refusal for a plan was
# observed. It names no command, since signing in again does not change a refusal that a new token did not,
# says what to check, and quotes nothing the host wrote.
xai-not-permitted = xAI refused the request for this account, and signing in again will not change that. Check the plan at xAI, or choose another model


## Declaring an MCP server, and approving one

# Printed under a refusal that named no command, or one this does not have.
mcp-forms-heading = bravebot mcp takes one of:
mcp-needs-a-command = bravebot mcp needs a command
mcp-unknown-command = bravebot mcp has no command { $command }
mcp-needs-an-alias = { $command } needs the alias of a server
mcp-unexpected-argument = { $command } does not take { $argument }
mcp-add-stray-argument =
    word { $position } after add is not a flag, and is not repeated since it may be a value: -e
    takes the words up to the next flag, --dir, --http and -s one each, and -- takes the rest
mcp-scope-needs-a-value = -s needs a scope: local, project or user
mcp-not-a-scope = { $scope } is not a scope: -s takes local, project or user
mcp-not-a-scope-unshown =
    the word after -s is not a scope, and is not repeated since it may be a value: -s takes local,
    project or user
mcp-two-scopes = -s is given twice, and a request is written to one file
mcp-not-an-alias =
    { $alias } cannot name a server: an alias is letters, digits, - and _, starts with a letter or
    a digit, and is at most 64 characters
mcp-not-an-alias-unshown =
    word { $position } after add cannot name a server, and is not repeated since it may be a value:
    an alias is letters, digits, - and _, starts with a letter or a digit, and is at most 64
    characters
mcp-needs-a-transport = add needs -- <program> [args...] or --http <url>
mcp-two-transports = add takes a program after -- or --http, not both
mcp-stdio-needs-a-program =
    a program and its arguments come after a bare --, as in -- npx -y weather-mcp
mcp-http-needs-a-url = --http needs a url
mcp-bearer-env-needs-a-name = --bearer-env needs the name of an environment variable
mcp-header-env-needs-a-pair = --header-env needs <HEADER>=<ENVIRONMENT_VARIABLE>
mcp-env-needs-a-name = -e needs NAME=value, or the name of a variable read from your environment
mcp-env-after-the-alias =
    -e and --env come after the alias, as in add weather -e KEY=value -- weather-mcp
mcp-env-word-refused =
    word { $position } after add is not NAME=value, and is not repeated since it may be a value: a
    name alone is read from your environment only as the one word its -e takes
mcp-dir-needs-a-path = --dir needs a directory
mcp-tools-needs-a-list = --tools needs a tool name or a comma-separated list of tool names
mcp-exclude-tools-needs-a-list = --exclude-tools needs a tool name or a comma-separated list of tool names
mcp-dir-not-a-directory = { $path } is not a directory
mcp-dir-not-a-directory-unshown =
    the word after --dir is not a directory, and is not repeated since it may be a value
mcp-dir-not-text = { $path } cannot be written into mcp.json, which holds text
# A server may write its directory, and git runs the commands a repository's configuration names.
mcp-dir-in-a-repository =
    { $path } is in the git repository at { $repository }, and a server may write its directory,
    where git finds commands to run: give it a directory outside any repository
mcp-not-added = { $alias } was not declared: { $problem }
mcp-not-declared = no MCP server is declared as { $alias }
# What is wrong with a declaration, from a flag or from mcp.json. None of these repeats a value: the
# ones that name something name the variable, never what it was set to.
mcp-problem-alias = the alias is not one: letters, digits, - and _, starting with a letter or a digit
mcp-problem-not-an-object = the entry is not an object
mcp-problem-transport = transport is missing, or is neither stdio nor http
mcp-problem-key = { $key } is not a key a declaration has
mcp-problem-program = argv is missing or empty, or holds something that is not a string
mcp-problem-name = a variable is not a name: a letter or _, then letters, digits and _
mcp-problem-env = env is not an object of names and their values
mcp-problem-value = the value env gives { $name } is not text a variable can hold
mcp-problem-twice =
    { $name } is given twice: a variable has one value, stored or read from your environment
mcp-problem-reads = reads is not a list of absolute paths
mcp-problem-directory = the directory is not an absolute path
mcp-problem-url = the url is not http or https with a host and, where it writes a port, a number up to 65535
mcp-problem-credentials =
    the url carries a user or a password, which would keep a credential in plain text
mcp-problem-header = { $name } is not a valid HTTP header token
mcp-problem-remote = a remote server takes no { $key }
mcp-problem-timeout = { $key } is not a whole number of seconds from 1 to { $most }
mcp-problem-tool = { $name } is not a tool name
mcp-problem-tools = { $key } is not a list of tool names
mcp-unreadable = { $path } cannot be read: { $reason }
mcp-unreadable-too-large = it is larger than a declarations file has any reason to be
mcp-unreadable-not-read = it could not be read as text
mcp-unreadable-not-json = it is not JSON
mcp-unreadable-not-an-object = it is not a JSON object
mcp-unreadable-servers = servers is not an object
mcp-unreadable-key = { $key } is not a key it has: it holds servers and nothing else
mcp-not-while-incognito =
    bravebot mcp { $command } writes to disk, which an incognito session will not do
mcp-no-state-directory =
    there is no state directory, so no MCP server is declared: none of { $variables } names a
    profile directory
mcp-not-written = { $path } could not be written ({ $error })
mcp-declared = declared { $alias } in { $path }
mcp-variables = variables: { $names }
# HTTP headers sent from environment variables, shown as "Header: $VAR"
mcp-headers = headers: { $headers }
# A variable given its value at add, which is kept in mcp.json and is never shown.
mcp-variable-stored = { $name } (stored)
# A file a stored value or an argument names, which the server is let read.
mcp-may-read = may read: { $path }
mcp-directory = directory, which it may write: { $path }
# The seconds a server has for its handshake and for each call, where a declaration gives either.
mcp-timeouts = time allowed: { $startup } seconds to start, { $tool } seconds for each call
mcp-declaration-tools = tools: { $names }
mcp-declaration-exclude-tools = exclude tools: { $names }
mcp-digest = digest: { $digest }
# Where a declaration replaced one that was approved, which of its fields differ.
mcp-changed = changed: { $fields }
mcp-question = Use this MCP server?
mcp-already-approved = { $alias } is approved, against digest { $digest }
mcp-recorded = approved { $alias }, against digest { $digest }
mcp-left-unapproved = { $alias } is declared and not approved
mcp-nobody-asked =
    nobody could be asked about { $alias }, so it is declared and not enabled: run { $command } at
    a terminal
mcp-nobody-to-ask =
    nobody can be asked about { $alias }: run bravebot mcp approve { $alias } at a terminal
mcp-declared-not-enabled =
    { $alias } is declared and not approved, so it is not enabled: { $command } asks again
# Said by `bravebot mcp enable` and `disable`, and by `add` once the server is approved. The path
# is the settings file whose mcp.request was written.
mcp-enabled = enabled { $alias } in { $path }
mcp-already-enabled = { $alias } is already enabled in { $path }
mcp-not-enabled = { $alias } is not approved, so it was not enabled
mcp-nobody-to-enable =
    nobody can be asked about { $alias }, so it was not enabled: run { $command } at a terminal
mcp-requested-not-approved =
    { $alias } is not approved, and { $path } still requests it, so the next session that reads it
    asks about { $alias }
# The reason is the managed layer's own, naming its file.
mcp-enabled-not-started = { $alias } is requested and not started: { $reason }
mcp-disabled = disabled { $alias } in { $path }
mcp-enabled-nowhere = { $alias } is not enabled in { $paths }
mcp-settings-not-a-document =
    { $path } does not hold a settings document, so it was left as it is
mcp-settings-too-large =
    { $path } is past what bravebot reads, or would be with the change in it, so it was not written
mcp-settings-changed =
    { $path } changed after bravebot mcp { $command } read it, so it was not written: run it again
mcp-settings-not-a-list =
    mcp.request in { $path } is not a list of aliases, so it was left as it is
mcp-settings-link =
    { $path } is a link, and a checkout's link leads wherever whoever wrote the checkout pointed
    it, so it was not written
mcp-removed = removed { $alias }, and any approval that only it held
# Said by `bravebot mcp forget`, once for each standing answer it dropped.
mcp-forgot-servers = { $path } no longer starts every server it requests without asking
mcp-forgot-tool = { $tool } is asked about again before each call in { $path }
mcp-forgot-nothing = nothing was recorded for { $path }
mcp-no-current-directory = the current directory could not be read: { $error }
mcp-none-declared = no MCP server is declared in { $path }
mcp-list-declared-in = declared in { $path }
mcp-approved = approved
mcp-unapproved = unapproved
mcp-unapproved-run-approve = unapproved: run bravebot mcp approve { $alias }
# The heading `get` puts over the policy a session would start a local server under, which the lines
# beneath it spell out with the labels /status uses.
mcp-get-policy = a session here would confine it to:
mcp-get-policy-home = home
mcp-get-policy-home-throwaway = a new directory in the temporary directory, made when it starts
mcp-refused-by-managed = not started: { $reason }
mcp-cannot-be-used = cannot be used: { $problem }
mcp-unusable = { $alias } cannot be used: { $problem }
mcp-list-unusable =
    { $count ->
        [one] one declaration in { $path } cannot be used
       *[other] { $count } declarations in { $path } cannot be used
    }
# Under the heading of `bravebot mcp list`: the directory whose session the lines under each server
# answer for.
mcp-list-here = for a session started in { $path }
# Under each server in `bravebot mcp list` and `doctor`: which checkout requested it, and whether a
# session started here holds the grant to call it.
mcp-not-requested-here = not requested here, so no session here holds a grant to call it
mcp-requested-held =
    requested by { $file }: a session here starts it unasked, and holds a grant to call it
mcp-requested-asked =
    requested by { $file }: a session at a terminal here asks before starting it, and holds a
    grant to call it only after a yes
mcp-requested-withheld =
    requested by { $file }, and no session here starts it or holds a grant to call it: { $reason }
# Where the server's own line already says why.
mcp-requested-not-started =
    requested by { $file }, and no session here starts it or holds a grant to call it
mcp-requested-undeclared = requested by { $file }, and not declared: bravebot mcp add declares it
mcp-no-confinement-here = this platform has no confinement for a local MCP server yet
# The standing answers recorded for a server in this directory: answer 2 at either question.
mcp-standing-project = answered here: use every MCP server this project requests
mcp-standing-tools = answered here: call { $tools } without asking
mcp-standing-none = nothing is answered for it here


## The MCP servers a session starts with, and why one it was asked for is not among them

servers-none-reached = no requested MCP server was started ({ $aliases }): { $reason }
servers-not-declared =
    { $file } requests the MCP server { $alias }, which is not declared, so nothing was installed
    or run for it: bravebot mcp add declares one
servers-not-reached = { $alias } was not started: { $reason }
servers-refused-by-managed =
    { $alias } was not started, whatever was declared or approved: { $reason }
managed-not-allowed =
    { $path }, which this machine's administrator manages, allows only the servers its mcp.allow
    names, and not this one
managed-denied =
    { $path }, which this machine's administrator manages, denies it with the mcp.deny entry
    { $entry }
managed-host-unread =
    { $path }, which this machine's administrator manages, denies servers by host, and this url
    spells its host in a way no entry can be compared with
servers-nobody-in-a-one-shot =
    { $alias } was not started: a one-shot run asks nobody, so run bravebot mcp approve { $alias }
    at a terminal
servers-nobody-at-a-terminal =
    { $alias } was not started: there is no terminal to ask at, so run bravebot mcp approve
    { $alias } at one
servers-declined = { $alias } is not used in this session
servers-program-relative = { $program } is a relative path, which names a different program in each directory
servers-program-without-path =
    { $program } is found through PATH, which the declaration does not name: declare it with
    --env PATH, or give the program as an absolute path
servers-program-not-found = { $program } is in no directory the PATH it names lists
servers-requested-by = requested by { $file }
servers-program = runs { $path }
servers-changed = changed since it was approved
# A runner resolves a package when it starts, so what it runs is chosen then, not here.
servers-fetches = { $runner } fetches what it runs when it starts
servers-unpinned = { $package } names no exact version, so it runs whatever is published under it
servers-unread =
    { $flag } is not a flag bravebot knows, so which package { $runner } runs, and whether it names an exact version, is not known
servers-answer-once = Yes
servers-answer-project = Yes, and use all future MCP servers in this project
servers-answer-no = No, continue without this server
servers-answer = [1/2/3]
safe-mode-started =
    Safe mode: hooks, skills, definitions, MCP servers and AGENTS.md were not loaded. Sign-in, the model and permissions are as usual
servers-for-this-session-only =
    { $alias } is used in this session only: an incognito session records no answer
servers-not-kept = { $alias } is used, and its approval was not recorded: { $reason }
servers-project-not-kept = { $path } was not recorded as a project whose servers are used
servers-not-confined = { $alias } was not started, since nothing here can confine it: { $reason }
servers-no-confinement-here =
    { $alias } was not started: this platform has no confinement for a local MCP server yet
servers-no-home =
    { $alias } was not started: a directory of its own could not be made in { $path }: { $reason }
servers-paths-left-out =
    { $alias } was started without paths it was granted, since confinement here names no path that
    is not on disk: { $paths }
servers-no-handshake = { $alias } was started and did not complete its handshake: { $reason }
servers-too-slow = { $alias } did not complete its handshake within { $seconds } seconds
servers-prompts-not-listed = { $alias } offers no prompt in this session: { $reason }
servers-prompts-refused = { $count ->
    [one] { $alias } listed { $count } prompt it cannot offer: a name is letters, digits, - and _, listed once, and so is the name of each argument
   *[other] { $alias } listed { $count } prompts it cannot offer: a name is letters, digits, - and _, listed once, and so is the name of each argument
    }

## A model this machine's administrator does not let it ask for

# Said where a run or a session settles on the model, rather than when a request would go out: the
# person reading it cannot write the file that refused, so the file is the only actionable thing in
# it, and a refusal at the moment of the request says nothing about where to look.
managed-model-refused = no request is made for { $model }: { $reason }
managed-model-not-allowed =
    { $path }, which this machine's administrator manages, allows only the models its models.allow
    names
managed-model-denied =
    { $path }, which this machine's administrator manages, denies it with a models.deny entry
# A delegate definition naming a model the layer refuses. Refused where the definition is read, so
# nothing is started for it.
delegate-model-refused =
    { $definition } asks for { $model }, which this machine does not request: { $reason }

# Said among a delegate's own lines when a person stopped it from the delegate view: the turn goes on.
delegate-stopped-narration = you stopped this delegate, so it has to finish with what it has

# Said among a delegate's own lines when it has spent the token budget its definition or the call
# set: it loses its tools and answers with what it has. The figure is a count of tokens.
delegate-budget-narration = this delegate has spent its budget of { $budget } tokens, so it has to finish with what it has

## The tools an MCP server offers, read by the person before any of them is offered to the model

mcp-tools-title = offer these tools to the model?
mcp-tools-offered =
    { $count ->
        [one] { $alias } offers one tool
       *[other] { $alias } offers { $count } tools
    }
mcp-tools-none = { $alias } lists no tool it can offer
mcp-tools-explained =
    The model will read each tool's name, its arguments and what the server says about it, as
    shown here. Every call is still put to you. Say no if a description gives instructions.
mcp-tools-not-listed =
    { $count ->
        [one] one more tool is not listed: its name or its arguments cannot be offered
       *[other] { $count } more tools are not listed: their names or their arguments cannot be offered
    }
mcp-tools-withheld =
    { $count ->
        [one] one tool was withheld by the server's allow or deny list
       *[other] { $count } tools were withheld by the server's allow or deny list
    }
# How one argument's kind reads in a list, as in `city_name (string, required)`. The kinds are
# the server's JSON Schema words and stay as it wrote them.
mcp-tools-argument-list-of = { $kind } of { $items }
mcp-tools-argument-required = required
mcp-tools-yes = Yes, offer them
mcp-tools-no = No, continue without them
mcp-tools-declined = { $alias } offers no tool in this session: its list was not approved
mcp-tools-refused = { $alias } offers no tool in this session: { $reason }
mcp-tools-changed = { $alias } changed its tools: it offers none until the list it gives now is approved
mcp-tools-reread-failed = { $alias } offers no tool in this session: its changed list could not be read ({ $reason })

## One call to a tool of an MCP server

mcp-call-title = call this tool?
mcp-call-kind = (MCP)
mcp-call-no-arguments = no arguments
mcp-call-question = Proceed?
mcp-call-yes = Yes
mcp-call-stand = Yes, and stop asking for { $tool } in this project
mcp-call-cannot-stand = not offered: nothing answered in this session can be recorded
mcp-call-allow-and-remember-explained = Allow and remember records { $tool } in this project: every later call to it, with any arguments, is allowed without asking again.
mcp-call-no = No
mcp-call-expand = (e to expand)
mcp-call-collapse = (e to collapse)
mcp-call-not-recorded =
    { $tool } was called, and your answer to stop asking could not be recorded, so the next call
    asks again: { $error }
mcp-call-path-not-one-line = the project's path cannot be written on one line
# Why a record of answers under the state directory was left as it is rather than written over.
mcp-record-too-large = it is larger than a record of answers has any reason to be, so it was left as it is
mcp-record-not-read = it could not be read as text, so it was left as it is

## A remote MCP server whose reply pointed somewhere it is not declared

mcp-move-title = declare this server where its reply points?
mcp-start-title = start { $alias }?
command-mcp = List MCP servers, start one again, or turn one off
mcp-command-usage = /mcp takes list, reconnect [alias] or disable <alias>
mcp-command-none = No MCP server is requested here, and none is running
mcp-command-running = MCP servers running:
mcp-command-not-running = MCP servers requested here and not running:
mcp-command-not-connected = { $alias }: not connected. /mcp reconnect { $alias } tries it again
mcp-command-off = { $alias }: turned off for this session. /mcp reconnect { $alias } starts it again
mcp-command-already-running = { $alias } is already running
mcp-command-not-requested = { $alias } is not requested by this session's settings
mcp-command-nothing-to-reconnect = Every requested MCP server is running or turned off. /mcp reconnect <alias> starts one that is off.
mcp-command-reconnected = { $alias } is running. Its tools are put to you before the next turn plans.
mcp-command-disabled = { $alias } is turned off for this session. Nothing was written.
mcp-command-already-off = { $alias } is already turned off
mcp-command-needs-an-alias = /mcp { $command } needs the alias of a server
mcp-move-declared = { $alias } is declared at { $url }
mcp-move-destination = and its reply points to { $url }
mcp-move-reaching = reaching { $authority }
mcp-move-explained =
    Nothing was sent there. A yes declares the server at that address and sends it what was being
    sent, and every later request to the server goes there too, in this session and the next. Say
    no unless you know the server moved.
mcp-move-this-session-only = nothing answered in this session is recorded, so a yes lasts until it ends
mcp-move-yes = Yes, it moved there
mcp-move-no = No
mcp-move-declined =
    { $alias } stays where it is declared: its reply pointed somewhere else, and nothing was sent there
mcp-move-not-started =
    { $alias } was not started: its reply to the handshake pointed somewhere it is not declared, and
    nothing was sent there
mcp-move-refused-by-managed = { $alias } was not moved where its reply points: { $reason }
mcp-move-undeclarable = { $alias } was not moved: where its reply points cannot be declared: { $problem }
mcp-move-moved = { $alias } was moved where its reply pointed
mcp-move-edited =
    { $alias } was not moved: its declaration changed while you were asked, so it was left as it is
mcp-move-not-recorded =
    { $alias } is used where its reply pointed in this session only, since the move could not be
    recorded: { $error }
mcp-move-no-handshake = { $alias } did not complete its handshake where its reply pointed: { $reason }
mcp-move-again = { $alias } was redirected again, off where it was just moved, so that was refused

## An MCP server's prompt a person ran by typing its name

mcp-prompt-failed = the prompt { $name } of { $alias } could not be fetched: { $reason }
mcp-prompt-not-a-pair = { $word } is not name=value, so the line was not sent. A prompt of a server takes its arguments as name=value, with a value that holds spaces in double quotes
mcp-prompt-bad-name = { $word } does not start with a name an argument may have, so the line was not sent. A name is letters, digits, _, - and .
mcp-prompt-ambiguous = { $word } begins the alias of more than one server, so it names none of them and the line was not sent
mcp-prompt-sent-as-text = that line names a prompt it cannot run, so it went to the model as it is and the prompt was not fetched
mcp-prompt-twice = { $name } is given more than once, so the line was not sent
mcp-prompt-quote = the value of { $name } has a double quote that does not close, or text straight after it, so the line was not sent

## Programs asking for one more path for the session

path-title = let programs reach another path?
path-reads = programs this session starts may read { $path }
path-writes = programs this session starts may read and write { $path }
path-why = the planner says: { $why }
path-explained =
    Every command the planner runs from now until this session ends gets it. Nothing is written to
    disk, so the next session asks again. /reach paths lists what was allowed and
    /reach paths remove <number> ends one.
path-not-trusted =
    The directory is not marked trusted: what is in it is still labelled as it was, and a file
    there is no more believed than before.
path-yes = Yes, for this session
path-no = No
path-listed = { $number }. every command also { $access } { $path }, for this session
reached-listed = { $path }, first reached by { $tool } ({ $field })
path-none = no path was requested this session
path-removed = ended: commands no longer { $access } { $path }
path-refused-number = no requested path is numbered { $number }
path-usage = /reach paths lists the paths programs were allowed to reach this session. /reach paths remove <number> ends one.

## Programs asking for hosts the allowed-hosts list does not cover

host-title = let programs reach more hosts?
host-asked = { $count ->
    [one] a program this session started asked for a host that sandbox.network.allowedHosts does not list:
   *[other] programs this session started asked for { $count } hosts that sandbox.network.allowedHosts does not list:
    }
host-row = { $host }
host-explained =
    The program was refused, so run it again after a yes. Every command the planner runs from now
    until this session ends may reach these hosts, unless sandbox.network.deniedHosts names them.
    Nothing is written to disk, so the next session asks again.
host-yes = Yes, for this session
host-no = No

## A message to another background session, asked before it is sent

message-title = send a message to another session?
message-to = To the session "{ $name }" ({ $state }), working in { $directory }:
message-label = Sent as { $label }.
message-explained =
    A yes sends this text to that session's planner as advice from another session. It answers
    nothing that session is holding, grants it no permission and starts no turn there. The planner
    wrote the text.
message-explained-turn =
    Answering { $key } sends this one and the later ones to this session until this turn ends.
    Nothing is written down, so the next turn asks again.
# What a line on a terminal accepts: y sends this one, the key named in message-answer-turn sends
# this one and the later ones this turn, and any other line, and the end of the input, sends nothing.
message-answer = [y/t/N]
message-answer-turn = t

## Vouching for a directory, asked once when a session starts somewhere new

trust-directory-title = trust this directory?
trust-directory-question = Trust
trust-directory-explained =
    Files here will be read as trusted, and edits to them will not be shown to you one by
    one. Say no if you did not write this code.
trust-directory-regardless =
    Either way, anything derived from the web or from an untrusted file is still shown
    before it is written.
# What a yes grants beyond the files (RUN-5), above the lines that scroll so that it is on the screen
# whenever the yes can be taken. It names the cases where a line still asks, since a person who
# reads it should not expect more than they get.
trust-directory-confined-lines =
    A yes also lets a command run here without asking when the sandbox confines it, with the
    network closed for that command, which today is on macOS only. Anything that needs more still asks.
# The yes where the answer cannot be kept (TRUST-23): no state directory, a filesystem root, the home
# directory or one holding it, a filesystem that does not say when a directory was made, or an
# incognito session.
trust-directory-yes = trust it
trust-directory-no = ask me about every write
# The yes where it keeps the answer for later sessions (TRUST-23). Its lines say what it covers and
# where it goes, since nobody can endorse a record they were not shown.
trust-directory-remember = trust and remember
trust-directory-remember-explained =
    A yes is kept: later sessions started in this directory, or below it if it is a git root, are not asked.
trust-directory-remember-exact =
    A session started above it is still asked, and so is one in a nested repository or in a directory deleted and made again here.
trust-directory-remember-where = /forget-trust takes it back, and it is written down here:
# Said where a yes cannot be kept, so the person is not left to find out from the next session.
trust-directory-not-remembered =
    This answer cannot be kept here, so a session started in this directory later asks again.
# Above the keys while the lines saying what y keeps and where it writes have not been on the screen
# together, which is when y is not taken. What y does comes first, so a narrow terminal that cuts the
# line off keeps it.
trust-directory-remember-unseen = ↑↓ y trusts nothing yet: what it keeps is not shown yet
# The same where those lines are taller than the box, so no scroll shows them together.
trust-directory-remember-too-small = y trusts nothing: what it keeps is taller than this box
quit = quit
trust-quit-again = again


## Opening a directory a settings file named, asked once for each when a session starts

named-directory-title = open this directory?
named-directory-question = Open
named-directory-explained =
    A settings file asked for this directory to be opened beside the one you are working in.
    Opening it lets files there be read and edited, and read as trusted.
named-directory-regardless =
    A file cannot open a directory on its own. Say no and this session runs without it; /add-dir
    opens one at any time.
named-directory-yes = open it
named-directory-no = leave it closed


## Granting the allow rules a checkout's settings file proposed, asked once for the whole list

# One question for every rule, listing each with the file it came from. The list is what makes the
# answer consent to specific grants rather than a feeling about the tree, and one box per rule would
# be thirty answers nobody reads.
granted-rules-title = grant these permission rules?
granted-rules-question = This project's settings ask to stop you being asked about:
granted-rules-explained =
    Each of these answers an approval prompt you would otherwise see: running a program, writing a
    file, or fetching a URL. They were written by whoever wrote this project, not by you.
granted-rules-regardless =
    A project cannot grant itself these. Say no and this session asks about each action as usual;
    rules in ~/.bravebot/settings.json are your own and always apply.
granted-rules-yes = grant them
granted-rules-no = keep asking me
# Above the keys while a rule has not yet been on the screen with its file, which is when y is not
# taken: a key that did nothing and said nothing would read as a question that had stopped answering.
# What y does comes first, so a narrow terminal that cuts the line off keeps it.
granted-rules-unseen =
    { $count ->
        [one] ↑↓ y grants nothing: { $count } rule not shown yet
       *[other] ↑↓ y grants nothing: { $count } rules not shown yet
    }
# The same where a rule not shown yet is taller than the box, so no scroll shows it whole.
granted-rules-too-small = y grants nothing: a rule is taller than this box


## Choosing a theme, a model, or a session to pick up

theme-picker-title = themes
theme-picker-keys = ↑↓ choose  ·  Enter select  ·  Esc keep current
model-picker-heading = Select model
model-picker-keys = ↑↓ choose  ·  Enter select  ·  type to search  ·  Esc keep current
model-picker-search-placeholder = Search
model-picker-nothing-matches = nothing matches that
model-picker-loading = loading models…
model-picker-listing-lost = the model list was not delivered
model-picker-unknown-name = the prompt file names { $model }, which the model list does not hold
picker-current = current

# Choosing how hard to think. The level names are the words sent and stored, so they are not
# translated; only what each one is for is.
effort-picker-title = effort
effort-picker-keys = ↑↓ choose  ·  Enter select  ·  Esc keep current
effort-unset = default
effort-hint-unset = left to whichever service answers
effort-hint-low = least thinking, for simple work
effort-hint-medium = less thinking, where that holds up
effort-hint-high = the usual amount, for work needing care
effort-hint-xhigh = more thinking, for code and long runs
effort-hint-max = the most thinking, cost aside
# Choosing how the input box edits text. The style names are the words the setting is spelled with, so
# they are not translated; only what each one is for is.
config-picker-title = editor mode
config-picker-keys = ↑↓ choose  ·  Enter select  ·  Esc keep current
config-editing-hint-ordinary = arrows and the readline chords
config-editing-hint-vi = modal editing, with hjkl and the operators
picker-premium = premium
# The heading over the models Brave's own endpoint serves. Named rather than left blank, because a
# list whose other sections name a service reads as though the unlabelled rows came from nowhere.
picker-service-brave = Brave
# What a tier set to an inference-profile ARN is called where no section heading says whose account
# answers: the info panel and /status. The tier word alone would read as the Brave roster's model of
# that name.
model-label-bedrock = { $name } (Bedrock)
# Both rosters are offered at once, and a tier name alone does not say which of the two it is: the
# same model is reachable through either, billed and reached differently. Not "Bedrock" alone, which
# the Brave roster already says of the models it serves through its own account.
picker-service-bedrock-profile = Bedrock, your { $profile } AWS profile
picker-service-bedrock = Bedrock, your AWS account
# Ctrl-R over the prompts already sent.
history-search-title = Search prompts
history-scope-everywhere = everywhere
history-scope-here = this project
history-scope-session = this session
history-search-placeholder = Filter history…
history-search-keys = ↑↓ to move  ·  Enter to use  ·  { $scope } to cycle the scope  ·  Esc to cancel
history-search-nothing-matches = nothing matches that
history-search-more-lines =
    { $count ->
        [one] … +1 line
       *[other] … +{ $count } lines
    }
# An age in a gutter beside every row rather than in a sentence, so it is short enough to leave the
# prompt the width. The session list says the same thing at length, where there is room for it.
history-age-now = now
history-age-minutes = { $count }m ago
history-age-hours = { $count }h ago
history-age-days = { $count }d ago
history-age-months = { $count }mo ago
# In the border of the box while a stored prompt is being walked to: which of the stored prompts is
# in the box, of how many.
input-history-position = { $scope } { $index }/{ $total }
input-history-this-session = This session
input-history-this-project = This project
input-history-all = All
input-history-widen = { $chord } all prompts
input-history-narrow = { $chord } this session
input-history-project = { $chord } project prompts
# At the other end of that same border, the ways in: the search over every prompt, and the search
# narrowed to the ones sent from this project. The narrower one is dropped where the row will not
# hold both beside the position, and then the other one is, so a longer wording is one that fewer
# terminals show at all.
input-history-search = { $chord } to search
input-history-scope = { $chord } this project
resume-manifest-run = that was a manifest run, which cannot be continued; start a new session


## Shared by every question the interface stops to ask

stop-the-turn = stop the turn
# Beside a write, run or call question's keys: the key that opens a line to type a note into.
note-key = note
# The line a note is typed into, opened from a write, run or call question.
note-title = add a note
note-sent-with-no = sent with a no
note-sent-with-yes = sent with a yes
note-explained = the planner reads this beside your answer. it does not change what is approved.
note-send = send
note-switch-to-yes = answer yes
note-switch-to-no = answer no
note-back = back
scroll-more = ↑↓ { $count } more
scroll-back = ↑↓ back
# Under a question's body in place of the scroll hint while rows that decide the question have not
# been drawn yet. No key that approves is taken until they have.
prompt-unseen =
    { $count ->
        [one] ↑↓ { $count } more row to read before a yes
       *[other] ↑↓ { $count } more rows to read before a yes
    }


## Approving a write

write-title = approve this write?
write-create = Create
write-overwrite = Overwrite
write-edit = Edit
write-tally = +{ $added } -{ $removed }
write-too-large-to-show =
    the change is too large to show: { $added } lines replace { $removed }
# Said above the rows of a write's change where some of the lines added or removed hold a character
# that draws as nothing. Unlike the sentence over a command's output, the text is not going to the
# model: it is going into a file, whole.
write-hidden-characters = { $count ->
    [one] 1 character in the lines this changes draws as nothing. It is written out below as ‹U+XXXX›, and a tag character also as the letter it stands for. The file is written with it as it is.
   *[other] { $count } characters in the lines this changes draw as nothing. Each is written out below as ‹U+XXXX›, and a tag character also as the letter it stands for. The file is written with them as they are.
    }
write-untrusted = untrusted: nobody has read this, and the model never saw it
write-remark =
    what the isolated processor said about this change, which nothing has checked against it
write-credentials =
    this looks like it would put a secret in the tree, going by the name beside the value and how
    the value reads. Nothing recognised it as a particular provider's key, so it is a guess and
    yours to settle
write-since-checkout =
    this session wrote to this file in the working directory after the checkout was made, so it
    may hold changes the checkout's copy does not. Read the difference before approving
write-line-endings-kept = line endings: { $ending } kept
write-line-endings-changed = line endings: { $from } to { $to }
write-line-endings-new = line endings: { $ending }
write-unchanged = { $count ->
    [one] … { $count } unchanged line
   *[other] … { $count } unchanged lines
    }
write-always-explained =
    a: stop asking whether this file may hold a secret, for the rest of this session
write-always-this-file = this file only: the same name in another directory is asked about again
write-always-only-the-secret =
    it settles the secret only: a write that would be asked about anyway still is
write-remember-explained = r: stop asking whether this file may hold a secret, from now on
write-remember-every-session = every session started in this directory reads it, not just this one
write-remember-where = it is written down here, and deleting the line is the way back:
write-yes = write it
write-always = always this session
write-remember = remember it
write-edit-first = edit it first
write-no = leave it alone


## Approving a command

run-title = run this?
run-verb = Run
run-stages = { $count ->
    [one] { $count } stage
   *[other] { $count } stages
    }
run-in-directory = in { $directory }
watching-list-command = command
# The same column on a background job's row. The job's name leads the line beside it.
watching-list-job = background
# The row and the view for a question asked beside the work, which is what /btw sends.
watching-list-aside = aside
watching-aside-head = a question asked beside the work
watching-aside-question = you asked
watching-aside-answer = the answer, which the conversation has not read
watching-aside-not-kept = this answer is on your screen only: the conversation had read something untrusted, so the record does not keep it
watching-aside-gone = the record could not keep this answer, so it did not come back with the session
watching-lines = { $count ->
    [one] 1 line
   *[other] { $count } lines
    }
watching-output-head = what this command printed
# The name is the one the driver gave the job, never anything the job printed.
watching-output-job-head = what background { $name } printed
watching-output-read = the model has read this
watching-output-kept = the model has not read this
# Short enough to stand in a column beside a command line. The whole sentence is in the header of
# the view the row opens.
watching-row-read = read
watching-row-kept = not read
# The same column on an aside's row, where the question is not whether the model read the answer
# but whether the record keeps it.
watching-row-kept-answer = kept
watching-row-screen-only = screen only
watching-output-more = { $count ->
    [one] 1 more line was printed and is not kept
   *[other] { $count } more lines were printed and are not kept
    }
watching-output-live-more = { $count ->
    [one] 1 earlier line is not shown here and is still unread
   *[other] { $count } earlier lines are not shown here and are still unread
    }
run-line-sent = the model wrote:
run-writes = it writes these files:
run-is-fed = it is fed the contents of:
run-not-sandboxed = this is not sandboxed: it runs with the access your own shell has
# Said instead of the line above where the turn confines what a run starts, and followed by the
# directories it is held to and by one sentence for each toolchain and credential a stage brings.
run-confined = its files are confined to these directories, the system temporary directory and the system files every program needs:
run-confined-machine = it can read this machine except the places that hold a credential, and write only these directories, the system temporary directory and the toolchain caches:
# The two above, where the platform cannot hold a `.git` back from a program that is not git.
run-confined-git = its files are confined to these directories, the system temporary directory and the system files every program needs, and a program in it other than git can write a repository's hooks and configuration, which git runs later with no sandbox:
run-confined-machine-git = it can read this machine except the places that hold a credential, and write only these directories, the system temporary directory and the toolchain caches, and a program in it other than git can write a repository's hooks and configuration, which git runs later with no sandbox:
run-carries-known-hosts = { $program } also adds to your ssh known hosts and can reach your ssh agent
run-carries-toolchain = { $program } also reaches the install and the cache of the { $toolchain } toolchain
run-carries-loopback = { $program } can also listen on, and connect to, ports of this machine
run-carries-ssh-key = the planner asked that you choose again which ssh key { $program } offers first, where more than one could be offered, and nothing more is lent
run-carries-remote = { $program } also reads your git and gh logins, your ssh configuration and public keys, never a private key, and adds to your ssh known hosts
run-carries-aws = { $program } also reads your aws credentials in ~/.aws
run-carries-kubernetes = { $program } also reads your kubernetes credentials in ~/.kube
run-carries-docker = { $program } also reads your docker credentials in ~/.docker
run-carries-signing = { $program } also reads the public key your git configuration signs with, and signs through your ssh agent, never reading a private key
run-carries-reach = { $program } also reads { $path }, where your { $variable } points
run-carries-identity = { $program } has ssh offer the key { $path } first, which your git.identities names for the owner of this address, and after it only keys your ssh configuration names
run-carries-identity-chosen = { $program } has ssh offer the key { $path } first, which you chose for { $owner } on { $date }, and after it only keys your ssh configuration names
run-carries-identity-chosen-default = { $program } leaves the key to ssh, which offers your keys in order, as you chose for { $owner } on { $date }
run-network-closed = the network is closed for programs this session starts, except for those below
run-keeps-network = { $program } also reaches the network
# Said for reach a person attached to a command with /reach. The date is the day they allowed it,
# so the row is never a surprise: the reach is on the screen every time the command is asked about.
run-carries-requested = { $sentence } (asked for by the planner for this line)
run-carries-remembered = { $sentence } (remembered for this command, allowed { $date })
run-carries-remembered-read = { $program } also reads { $path } (remembered for this command, allowed { $date })
run-carries-remembered-write = { $program } also reads and writes { $path } (remembered for this command, allowed { $date })
# The /reach command: what it lists, what it says it did, and why it did nothing.
reach-usage = /reach lists the reach remembered for commands. /reach <remote|aws|kubernetes|docker|directory> [write] [always] -- <command> remembers it for that command. /reach remove <number> forgets one. /reach paths lists the paths a session was let reach, and /reach paths remove <number> ends one. /reach keys lists the ssh key chosen at a run prompt for each owner, and /reach keys remove <number> forgets one.
reach-none = no reach is remembered for any command
reach-listed = { $number }. { $command } also { $access } { $entry }, allowed { $date }, { $lifetime }
reach-access-reads = reads
reach-access-writes = reads and writes
reach-lifetime-session = for this session
reach-lifetime-always = always
reach-lifetime-session-in = for this session in { $workspace }
reach-lifetime-always-in = always in { $workspace }
reach-added = remembered: { $command } also { $access } { $entry }, { $lifetime }. The next plan for it shows this row.
reach-removed = forgotten: { $command } no longer also { $access } { $entry }
reach-refused-entry = { $entry } is not a credential scope (remote, aws, kubernetes, docker) or a directory that can be reached. It must be an absolute path or start with ~/, exist, and not be your home directory, a directory above it, ~/.ssh, or a credential location such as ~/.aws or ~/.gnupg.
reach-refused-write = a credential scope is read only. Only a directory can be written.
reach-refused-line = that command line cannot be run as written, so there is nothing to remember the reach for
reach-refused-assignment = a command with NAME=value in front of it carries no remembered reach
reach-refused-option = a command that starts with an option, such as `sh -c ...` or `git -C dir push`, cannot carry a remembered reach. Name the operation first, as in `git push`.
reach-refused-workspace = this directory cannot be told apart from another made at the same path, so a reach for this command cannot be tied to it
reach-refused-incognito = this session adds nothing to ~/.bravebot, so nothing was remembered
reach-refused-no-home = this session has no home directory to judge a reach against
reach-refused-number = no reach is numbered { $number }
reach-key-none = no ssh key is chosen for any owner
reach-key-listed = { $number }. { $owner }: { $key }, chosen { $date }
reach-key-default = whichever key ssh tries first
reach-key-usage = /reach keys lists the ssh key chosen at a run prompt for each owner, and /reach keys remove <number> forgets one.
reach-key-refused-number = no key is numbered { $number }
reach-key-removed = forgotten: the key for { $owner }. The next run prompt for a git command that reaches it asks again, where it could offer more than one key.
run-filesystem-rules = your own filesystem rules apply to these programs: { $allow_read } allowRead, { $deny_read } denyRead, { $allow_write } allowWrite, { $deny_write } denyWrite
# Said above the list of what a line reaches that nothing here holds: no credential is handed
# over, nobody is asked at the moment it is used, and nothing here can take the access back. Said
# only where a line reaches one, so the list is never empty and never noise. The line above is
# said either way: this names what is being granted, rather than replacing what confinement there
# is with a list.
run-spends-authority = it also spends access that is yours elsewhere, which nobody is asked for and nothing here takes back:
run-authority-container = { $named }: the container daemon, which runs anything as root on this machine
run-authority-logged-in = { $named }: already logged in, so it acts as you without asking you
run-authority-agent = { $named }: your ssh agent, which signs with keys it never hands over
run-authority-metadata = { $named }: this machine's metadata service, which hands out the credentials of the role it runs as
run-releases-private = it is also being fed your own data, which leaves here with it
run-always-explained = a: trust this exact command for the rest of this session
run-always-means-both = which means both:
run-always-runs-again = it runs again unasked, side effects and all
run-always-output-trusted = what it prints is trusted, and the model reads it
run-always-exact-arguments = these arguments only: git log would not cover git push
run-always-this-directory = this directory only: the same line elsewhere is asked about again
run-allow-and-remember-explained = Allow and remember trusts this exact command, with these exact arguments, in this directory: it runs again unasked, side effects and all, now and in later sessions, and what it prints is trusted so the model reads it.
run-private-not-remembered =
    private input is asked about every time, so this one cannot be remembered
run-assignment-not-remembered =
    an assignment in front of a program is asked about every time, so this one cannot be remembered
run-write-not-remembered =
    a line naming a file to write is asked about every time, so this one cannot be remembered
run-stdin-not-remembered =
    a line fed a reference is asked about every time, so this one cannot be remembered
run-unconfined = the planner asked for this one line to run with no sandbox: no profile holds it, and the next line is held to the session's mode again
run-unconfined-not-remembered =
    a line the planner asked to run with no sandbox is asked about every time, so neither the line nor the request is remembered
run-scopes-not-remembered =
    a line the planner asked to carry a credential scope, toolchain list or loopback is asked every time, so it cannot be remembered
# Said only where the plan asked for a credential scope and a record of reach can be written. The
# lines under it are the programs, each with its operation word, as `/reach` lists them.
run-keep-reach-explained = m: also remember the requested reach for these commands, so the next plan for them carries it
run-keep-reach-scopes = the reach remembered is { $scopes }, read only
run-keep-reach-still-asked = the line is still asked about every time, with that reach shown on it, and nothing it prints is trusted
run-keep-reach-lifetimes = m lasts for this session. e lasts until removed, in this checkout, or in every checkout for a program the scope belongs to, such as git for remote.
run-keep-reach-asked-again = { $names } is not remembered: it is asked for again each time
run-keep-reach-where = it is written down here:
run-keep-reach-undo = /reach lists what is remembered, and /reach remove <number> forgets one
run-keep-reach = remember the reach
run-keep-reach-always = remember it for every session
run-key-choice-explained = { $first }-{ $last }: run it, and from now on have ssh offer that key first whenever git reaches { $owner }:
run-key-choice-default = leave the key to ssh, which offers your keys in order, and stop asking
run-key-choice-yes = y runs it with whichever key ssh tries first, and asks again next time
run-key-choice-undo = /reach keys lists the keys chosen, and /reach keys remove <number> forgets one
run-key-choose = offer that key
run-key-leave = leave it to ssh
run-remember-explained = r: stop asking about this exact line, in this directory, from now on
run-remember-where = it is written down here, and deleting the line is the way back:
run-remember-only-asking = it stops the asking only: what it prints stays quarantined
run-remember-every-session = every session started in this directory reads it, not just this one
# Said only for a line the program's table lists, under the record's own rows. The name under the
# first row is the scope in words, and the two rows after it say what it covers and what stays fixed.
run-remember-scope-explained = s: stop asking about this, in this directory, from now on:
run-remember-scope-read-covers = it covers viewing a pull request or an issue and reading a pull request's diff and checks, each with any number
run-remember-scope-read-fixed = everything else is still asked about: another repository, another flag, and anything that writes
run-remember-scope-one-entry = it is saved as one entry in the same file, and deleting that line takes all of it back
# Said where the person has already answered a prompt for this binary under other arguments, which
# is the only thing a prompt can establish about a line that will be asked about however it is
# answered. No pattern is suggested: which argument carried the message is the person's to decide.
run-pattern-varies =
    these arguments differ from the ones you were asked about before, so no key here ends the asking
run-pattern-where = a pattern for the family is written in a settings file, not answered here:
run-pattern-covers-unread = a pattern covers lines nobody has read, which is more than any key here grants
run-pattern-only-asking = a pattern stops the asking and nothing else: what the line prints stays quarantined
run-yes = run it
run-always = always this session
run-remember = remember it
run-remember-scope-read-pull-requests-and-issues = read pull requests and issues in { $repository }
run-no = don't
# Said above the keys while a row of the plan has not been on the screen. It counts rows, which is
# what the arrows move by, and the count comes first so a narrow box does not cut it off.
run-unseen =
    { $count ->
        [one] ↑↓ { $count } row not shown: no key runs this yet
       *[other] ↑↓ { $count } rows not shown: no key runs this yet
    }
# Said once the plan has been read while the rows saying what `a` or `r` grant besides have not. The
# keys still waiting on them are drawn muted.
run-grant-unseen =
    { $count ->
        [one] ↑↓ { $count } row not shown: a muted key waits on it
       *[other] ↑↓ { $count } rows not shown: a muted key waits on them
    }


## What a check said, at the head of every prompt whose answer would promote content

# Said of a command's output, of a file somebody is being asked to vouch for, and of one slot the
# model asked about, so it says "this" rather than naming what was read: the prompt around it has
# already said which thing that is.
check-safe = the check found no attempt to give instructions in this
check-unsafe = the check says this looks like an attempt to give instructions
check-inconclusive = the check did not complete, so nothing has looked at this


## Characters in a prompt's text that draw as nothing

# Said above text a person is deciding about, where some of it draws as nothing: a zero-width
# character, a bidirectional control, a variation selector or a tag character. The text below
# writes each one out, so the sentence says what those tokens are and that the model reads the
# characters themselves.
hidden-characters = { $count ->
    [one] 1 character in this text draws as nothing. It is written out below as ‹U+XXXX›, and a tag character also as the letter it stands for. The model reads it as it is.
   *[other] { $count } characters in this text draw as nothing. Each is written out below as ‹U+XXXX›, and a tag character also as the letter it stands for. The model reads them as they are.
    }


## Letting the model read what a command printed

output-title = let the model read this?
output-verb = Read
output-lines = { $count ->
    [one] { $count } line
   *[other] { $count } lines
    }
output-printed-by = printed by { $command }
output-unseen =
    the model has not seen this. Approving puts it in its context, and it will act on it.
output-empty = (it printed nothing)
output-yes = let it read this
output-no = keep it back


## Letting the model read one quarantined slot a check has looked at

vet-title = let the model read this?
vet-verb = Read
vet-lines = { $count ->
    [one] { $count } line
   *[other] { $count } lines
    }
vet-from = from { $origin }
vet-unseen =
    the model has not seen this. Approving puts it in its context, and it will act on it.
vet-covers-this-only =
    this covers what is below and nothing else. No path is vouched for, so the next read of
    the same thing asks again.
vet-expected = the model asked for this expecting { $expects }
vet-empty = (there is nothing in it)
vet-yes = let it read this
# What stops is the asking, not the checking: a check runs before this prompt either way, so a
# label about vetting would name the one thing this key does not change, and would read as the
# more cautious choice when it is the looser one.
vet-always = don't ask when safe
vet-no = keep it back
# What the standing key turns on, drawn only where it is offered. Not about these bytes: it says
# that from here on a check finding nothing answers this question, and where that is written down.
vet-always-covers =
    a stops this question wherever a check finds nothing, in this session and the next, until
    you change it. Kept in ~/.bravebot/vetting.

## Letting the model see one quarantined picture or PDF a check has looked at

vet-picture-title = let the model see this?
vet-picture-verb = Show
vet-picture-file = { $bytes ->
    [one] { $media }, { $bytes } byte
   *[other] { $media }, { $bytes } bytes
    }
# Above the path of the copy the prompt wrote, which is how a person sees what a terminal cannot
# draw. The copy is deleted once the question is answered, and the line has to say so.
vet-picture-open = open this copy to see what the model would be shown. It is deleted when you answer:
# The part of the file a person reading it is most likely to miss and a model is not.
vet-picture-words =
    a model reads words in a picture that a person can miss: small, faint, or nearly the colour
    of what is behind them. Look for writing before letting it through.
# Beneath the picture a terminal that draws real pictures shows on the prompt. It says what the
# drawing is, and why the copy is still worth opening.
vet-picture-drawn =
    the picture as this terminal draws it. Small or faint writing may not show at this size: open
    the copy above and zoom in.
vet-pdf-hidden-text =
    a PDF can also hold text that no page draws, and the model is given that text too.
vet-picture-yes = let it see this


## Fetching a URL

fetch-title = fetch this?
fetch-verb = Fetch
fetch-host = talking to { $host }
# Said under the host where the fetch was approved for another host and a redirect led here, so
# the question reads as a hop the person was never shown and not as a fresh fetch.
fetch-redirected = redirected from { $from }, which is the host this fetch was approved for
# Said where the host is the metadata service of the machine this is running on, which is a host
# like any other to everything in between: it asks for no credential and hands out the ones of
# the role this machine runs as. A person shown the address alone has been shown a number.
fetch-authority-metadata =
    this is this machine's own metadata service: it asks nothing of whoever reaches it and
    answers with the credentials of the role this machine runs as.
fetch-explained =
    what comes back stays quarantined however you answer: the model can pass it to a
    processor or write it to a file, and cannot read it or be told what it says.
fetch-yes = fetch it
fetch-no = don't


## Starting a language server

server-title = start a language server?
server-verb = Start
server-workspace = to index { $workspace }
server-build-tooling =
    this runs code from your project and its dependencies with your own access, the way
    building or testing the project does. it stays running for this session.
server-arguments = with { $arguments }
server-declared-unknown =
    you declared this server yourself, so what starting it runs is not known: it may run code
    from your project and its dependencies. it runs with your own access, and stays running for
    this session.
server-reads-only =
    it reads the project and stays running for this session. nothing is written to your
    project.
server-explained =
    what it reports stays on the same footing however you answer: a place in a file is
    shown, and the text at that place is quarantined unless you vouched for the file.
server-yes = start it
server-no = don't


## Approving a plan before it runs

plan-title = run this plan?
plan-verb = Run
plan-steps = { $count ->
    [one] { $count } step
   *[other] { $count } steps
    }
plan-goal = for { $task }
plan-explained =
    the whole program, decided before anything was read. nothing it reads can add a step,
    drop one, or send anything anywhere this plan does not already name.
plan-not-its-writes =
    approving the plan is not approving its writes. each one is still put to you as it
    comes up.
plan-nothing-yet =
    nothing has been read or written yet, so declining leaves everything as it is.
plan-yes = run it
plan-no = don't
# Where a question is a line on a terminal rather than a panel: what a yes looks like, and the one
# answer that approves. Any other line, and the end of the input, refuses. Shared by every question
# put in lines, so one affirmative covers them all.
line-answer = [y/N]
line-answer-yes = y
# The plan's own line, which names what saying yes runs.
plan-answer = run it? [y/N]


## Vouching for a quarantined file

vouch-title = let the model read this file?
vouch-verb = Trust
vouch-explained =
    the model cannot read this file, so it is working blind on it. Vouching lets it read
    this file for the rest of this session, here and in every later read.
vouch-rule = a yes trusts { $rule } and everything beneath it
vouch-nothing = (nothing of this file can be shown)
vouch-yes = trust it
vouch-no = leave it quarantined


## Reading a file that holds what looks like a credential

expose-title = let the model read a file holding a credential?
expose-verb = Send
expose-explained =
    the model may read this file, and what it reads goes to whoever performs inference.
    The scan found something in it that looks like a credential. Sending it discloses
    that value; declining keeps this file's text from the model and changes nothing else.
    An answer covers this file until the session ends or you change directory.
expose-found = what the scan found, without any of the value:
expose-yes = send it anyway
expose-no = keep it back


## Counting the things a session accumulates

count-rules = { $count ->
    [one] { $count } rule
   *[other] { $count } rules
    }
count-commands = { $count ->
    [one] { $count } command
   *[other] { $count } commands
    }
count-remembered = { $count ->
    [one] { $count } entry
   *[other] { $count } entries
    }
count-requested-paths = { $count ->
    [one] { $count } path
   *[other] { $count } paths
    }
count-reached-directories = { $count ->
    [one] { $count } directory
   *[other] { $count } directories
    }
count-reach-grants = { $count ->
    [one] { $count } grant
   *[other] { $count } grants
    }
count-tokens = { $count ->
    [one] { $count } token
   *[other] { $count } tokens
    }
# Thousands, already rounded to one place, because the exact figure is not the point at that size.
count-tokens-thousands = { $thousands }k tokens
# What separates a whole number from its fraction. English writes a point and much of Europe a
# comma, and the interface has one number in it that has a fraction at all.
number-decimal-separator = .


## What /status reports about the session

status-session = Session
status-session-untitled = untitled, nothing sent yet
status-session-id = Session id
status-directory = Directory
status-directory-trusted = trusted
status-directory-untrusted = not trusted, so every write is shown to you
status-directory-kept = remembered { $when }
status-directory-kept-note = later sessions started here trust it without asking
status-directory-kept-where = /forget-trust to be asked again; the answer is kept in { $path }
status-directory-kept-root-note = later sessions started here trust it without asking, because { $root } was remembered
status-also-open = Also open
status-added-directory = added with /add-dir
status-scratch = Scratch
status-scratch-note = this session's own to write in, removed when it ends
status-checkout = Checkout
status-checkout-note = { $id }, of commit { $commit }, made for delegate { $delegate }
status-model = Model
status-model-chosen = chosen with /model
status-model-default = the configured default
status-model-definitions = the one { $definition } asks for
# Under the model line when the line gives the name the configuration chose and not what a request
# names, since an error from the service quotes the latter.
status-model-id = sent to the service as { $id }
status-agent = Agent
status-agent-every-turn = every turn is addressed to it, named with --agent
status-agent-by-setting = every turn is addressed to it, chosen by the agent setting
status-effort = Effort
status-effort-chosen = chosen with /effort
status-effort-default = whatever the service does on its own
status-effort-not-read = chosen with /effort, but this model reads none
# The level shown is the one a ChatGPT session began with, which that account is sent for the whole
# session. The level chosen since is a level word or the word for the default.
status-effort-kept-for-account = the level this ChatGPT session began with; { $chosen } was chosen with /effort and applies from the next session
status-theme = Theme
status-theme-chosen = chosen with /theme
status-served = Answered by
status-served-instead = served instead of { $asked }, which that turn asked for
status-endpoint = Endpoint
# Which tier the last turn actually ran on, not what this build was compiled knowing about.
status-premium-available = premium available, nothing sent yet
status-premium-in-use = premium, the premium key was sent
status-premium-not-spent = the premium key was not sent
status-no-subscription = no premium key configured
status-confinement = Confinement
# Beside the level, because the level alone reads as a boundary the session is inside. The level
# is what this platform can enforce over a process running code we did not write, and the session
# starts none of those.
status-confinement-nothing-confined = this session confines nothing
# Where the session started MCP servers, the level is in force over them and over nothing else.
status-confinement-servers = this session confines the MCP servers it started, and nothing else it runs
status-mcp-servers = MCP servers
status-mcp-servers-none = none
# How one started server's tools stand. A list nobody has read yet is put to the person at the
# start of the next turn, and none of its tools is offered to the model until they say yes.
status-mcp-servers-unread = { $alias }: its tools are put to you before the next turn plans
status-mcp-servers-tools =
    { $count ->
        [one] { $alias }: one tool offered to the model
       *[other] { $alias }: { $count } tools offered to the model
    }
status-mcp-servers-declined = { $alias }: no tool offered, as you answered
# The heading of what one started local server's confinement grants, then a line each for the
# network, subprocesses and where it starts, and one for each path its backend was handed. Each
# path's label is what the server may do at it.
status-server-policy = Server policy
status-server-policy-egress = egress
status-server-policy-egress-closed = none
status-server-policy-egress-loopback = this machine only
status-server-policy-egress-open = any address
status-server-policy-egress-hosts = only through the allowed-hosts proxy
status-server-policy-subprocesses = subprocesses
status-server-policy-subprocesses-allowed = allowed
status-server-policy-subprocesses-refused = refused
status-server-policy-starts-in = starts in
# macOS refuses a write to a .git beneath a write row, so a row labelled write does not reach one.
status-server-policy-git = .git
status-server-policy-git-held-back = not written under a write row
status-server-policy-read = read
status-server-policy-write = write
status-server-policy-read-write = read, write
# The paths that were wanted and are not rows, because this platform grants no path that is not on
# disk and these are not. The paths follow, one to a line.
status-server-policy-left-out = left out
status-server-policy-left-out-reason = not on disk, and this platform grants no path that is not
status-loop = Loop
status-loop-numbered = Loop { $number }
status-loop-every = every { $every }
status-loop-self-paced = paced by each turn
status-loop-next = next in { $next }
status-loop-running = running now
status-loop-unpaced = waiting for the turn to say when
status-loop-once = once
status-after = After
status-after-numbered = After { $number }
status-goal = Goal
status-goal-paused = paused, /goal resume arms it
status-goal-usage = { $note } · { $elapsed } · { $tokens }
status-watch = Watch { $number }
status-watch-armed-by = armed by turn { $turn } · { $left } left
# One line per background job of the last turn. The name is the driver's.
status-job = Background { $name }
status-job-of-delegate = Background { $name } of delegate { $number }
status-job-note = { $standing } · { $origin }
status-job-moved = moved from the foreground after { $after }
status-job-started = started in the background
status-goal-rounds = { $rounds ->
    [one] sent back { $rounds } time, { $left } left
   *[other] sent back { $rounds } times, { $left } left
    }
# Drawn only where a mode other than asking is in force. Asking is the ordinary state and needs no
# line: a panel reporting "permissions: enforced" on every session teaches people to skim past the
# one that says otherwise.
status-permissions = Permissions
status-permissions-cycle = shift-tab to change
status-programs-write = Programs write
status-programs-write-anywhere = anywhere a request_path would be granted
status-programs-write-except = not a credential location, your denyWrite paths, or a new entry in your home directory or beside a credential location
# Said only where auto-vetting is on. The file is named because that is where the answer is kept
# and where it is undone; nothing in the interface turns it back off.
status-vetting = Vetting
status-vetting-auto = a check that finds nothing reads content to the model without asking
status-vetting-where = kept in ~/.bravebot/vetting
# Said only where the network for the programs a command starts is closed. The note says who closed
# it, since that decides how it is opened again.
status-network = Programs' network
status-network-closed = closed, except for steps that fetch or reach a remote
status-network-by-default = closed by default
status-network-by-flag = closed by --run-network
status-network-by-settings = closed by run.network in { $path }
status-network-by-a-setting = closed by run.network in a settings file
status-network-pinned = pinned closed by { $path } and not changeable by a flag or a settings file
status-network-pinned-by-policy = pinned closed by the managed settings and not changeable by a flag or a settings file
# Said only under sandbox mode workspace-write, where the kernel leaves every unix socket that has a path
# open to the programs a command starts. The note says why, and what strict and read-only do there.
status-path-sockets = Programs' unix sockets
status-path-sockets-reachable = every one with a path is reachable: D-Bus, SSH_AUTH_SOCK, docker.sock
status-path-sockets-why = Landlock here is below ABI 9 (Linux 7.1), and strict and read-only refuse programs
# Said only where one of the four sandbox.filesystem lists has an entry. The note names the files.
status-sandbox-filesystem = Filesystem rules
status-sandbox-filesystem-counts = { $allow_read } allowRead, { $deny_read } denyRead, { $allow_write } allowWrite, { $deny_write } denyWrite
status-sandbox-filesystem-files = from { $files }
status-sandbox-filesystem-flags = the command line
status-hosts = Allowed hosts
status-hosts-counts = { $allowed } allowed, { $denied } denied
status-hosts-files = from { $files }; a host no entry covers is refused
status-hosts-managed = the managed settings
status-hosts-refused = a host no entry covers is refused
status-hosts-files-ask = from { $files }; a host no entry covers is asked about
status-hosts-asked = a host no entry covers is asked about
status-this-session = This session
# Where a session's wall clock went. Four figures, because the whole is unactionable: a session
# that took an hour on the model, an hour on subprocesses, and an hour waiting for its user to
# answer a prompt are three different problems with the same total.
status-time = Time
status-time-inference = on the model
status-time-tools = running tools
status-time-stalled = waiting on you
status-time-overhead = unaccounted for
# How much of the last turn's prompt the service did not have to read. Two figures because they are
# priced differently: a read is a fraction of what a fresh token costs and a write is above it, so a
# breakpoint on a prefix nothing reads back is a loss that only the second figure shows.
# The label says which turn because the counts above it are the whole session's, and a reader who
# took this for a session total would divide it by them and conclude the wrong rate.
status-cache = Prompt cache, last turn
status-cache-read = served from the cache
status-cache-written = written to it for the next turn
# The lifetime the settings ask the service to keep a cached prefix for. It is the setting and not a
# measurement: no reply says how long a service actually kept anything, and a model that refuses the
# field is sent the request without it.
status-cache-lifetime = lifetime asked of the service
status-cache-lifetime-five-minutes = 5 minutes
status-cache-lifetime-one-hour = 1 hour
status-cache-lifetime-service-default = none, the service's own
# The latest turn's cache read as a share of its prompt tokens, on the footer.
hint-cache-hit-rate = cache { $rate }%
status-trust = Trust
status-nothing-vouched-for = nothing vouched for
status-held-by-the-turn = held by the running turn, shown once it ends
status-trusted = trusted
status-untrusted = untrusted
status-programs = Programs
status-every-run-is-asked = every run is put to you
status-nothing-vouched-this-session = nothing vouched for this session; the lines below run unasked
status-trusted-commands = Trusted commands
status-trusted-commands-note = run unasked, and their output is trusted
status-command-in = in { $directory }
scope-read-pull-requests-and-issues = Read pull requests and issues in { $repository }
status-remembered = Remembered lines
status-remembered-note = run unasked in this directory, and their output stays quarantined
status-remembered-this-session = remembered in this session
status-remembered-earlier = remembered in an earlier session
status-remembered-where = delete a line from { $path } to be asked again
status-reach = Remembered reach
status-reached-directories = Directories reached
status-reached-directories-note = bypass mode lets the file tools reach these without opening them; they are not trusted, and leaving bypass ends it
status-requested-paths = Requested paths
status-requested-paths-note = programs this session starts may reach these; the directories are not trusted. /reach paths remove <number> ends one
status-reach-note = added to the plan of the command each names; /reach remove <number> forgets one
status-remembered-and-more = { $count ->
    [one] … and 1 more, { $earlier } of them from an earlier session
   *[other] … and { $count } more, { $earlier } of them from an earlier session
    }

# Which deployment is being talked to. Left as they are where a language borrows the English
# abbreviation, which is common for these four.
environment-local = local
environment-dev = dev
environment-prod = prod
environment-custom = custom


## What /cost reports about each turn

# One turn's share of the session, so the turn that ran away is the one that stands out. A figure
# in tokens alone leaves the reader to divide it by the total themselves, and the whole reason to
# ask is that one row is unlike the others.
cost-share = { $percent }%
cost-turn = Turn { $number }
# What an aside or a run asked for before any prompt was sent. It is in the session total either
# way, so it is shown rather than dropped, and it is not a turn, so it does not borrow a turn's
# number.
cost-before-the-first-turn = Before turn 1
cost-nothing-spent = nothing spent yet
session-limit-none = this session has no spend limit
session-limit-in-force = the session limit is { $limit } { $unit }, and { $spent } are spent
session-limit-set = the session limit is { $limit } { $unit }, and { $spent } are spent
session-limit-set-below-spent = the session limit is { $limit } { $unit }, and { $spent } are already spent, so the next request asks whether to go on
session-limit-cleared = the session has no spend limit
session-limit-unknown = { $figure } is not a limit. Use a whole number, with k or m after it for thousands or millions, and credits after that to count premium requests instead of tokens, or off to remove the limit
request-none-yet = No request has been sent to the model in this session yet.
# What /context reports. The section names are fixed words, and none is read from a file or a result.
context-not-measured = The context has not been measured yet, so there is no breakdown.
context-compacted = The conversation was compacted after the last request was measured, so that breakdown no longer describes it. The next request measures it again.
context-no-request = The context has been measured, but this session has sent no request of its own to break down yet.
context-total = Last request
context-approximate = shares of the bytes sent, scaled to the measured total
context-section-system = System prompt
context-section-instructions = Instruction files
context-section-skills = Skills
context-section-tools = Tool definitions
context-section-typed = What you typed
context-section-planner = What the planner wrote
context-section-results = Tool results
context-section-other = Other messages
# The first row of the view /request opens. The words below it are the request as it went.
request-title = The last request sent to { $model }, read from the request itself
request-tools = Tools offered: { $names }
request-no-tools = Tools offered: none
request-instruction-files = Instruction files loaded: { $files }
request-no-instruction-files = Instruction files loaded: none
request-instruction-file = { $path } ({ $bytes } bytes)
request-trusted = { $what } (trusted)
request-bytes-mark = [a picture or file, sent as bytes]
request-label-typed = typed
request-label-typed-with-files = typed, with dropped files
request-label-driver = driver
request-label-planner = planner
request-label-trusted-file = trusted file { $path }
request-label-setting = setting
request-label-released = released: { $from }
request-label-vetted = vetted { $token }
request-label-summary = summary
request-label-unrecorded = unrecorded
# What the total holds that no turn's figure accounts for. A record written before turns were
# charged separately keeps the whole of it here, and a session resumed from one keeps the part it
# spent before the resume. Reported as a figure rather than left out, because the rows are read
# against the total on the first line and a remainder nobody names reads as an arithmetic fault.
cost-unattributed = not recorded against any turn


## The indicator drawn while a turn runs

# How long the turn has been going. No hours: a turn that ran that long has gone wrong, and
# `73m 04s` says so more plainly than `1h 13m`.
elapsed-seconds = { $seconds }s
elapsed-minutes = { $minutes }m { $seconds }s
# Beside a figure already labelled in tokens, so the unit is not repeated.
indicator-tokens-read = ↓ { $tokens } tokens
indicator-tokens-written = ↑ { $tokens }
# Said while the model writes a tool call, before the call runs or has a line of its own. The
# word is the one that line will start with ("Write", "Run"), so the two read as the same call.
indicator-composing = Preparing a call: { $call }
# Said while a confined check reads quarantined content, before any of it may be read. The count
# is what the check was given, which is the one thing that predicts how long it will take. Not a
# word about what it decided: that reaches a person on the prompt and nothing else.
indicator-checking = { $lines ->
    [one] Checking { $lines } line
   *[other] Checking { $lines } lines
    }
# Said while the turn waits for a delegate to finish and has nothing to ask the model until its
# report arrives. The argument is the delegate's number, such as d1.
indicator-waiting-on-delegate = Waiting on { $delegate }
# Said while a hook holds the turn open. The moment is the word the hooks file spells it with and
# the program is the first word of the command the person wrote there: nothing the hook printed.
indicator-hook = Running hook: { $program } ({ $moment })
# The same, over a picture or a PDF, which has no lines to count.
indicator-checking-picture = Checking a picture
indicator-checking-pdf = Checking a PDF
# Said while a turn is being stopped, from the press that asks for it until the turn ends. The turn
# ends once its worker and every delegate it started have returned, which can take seconds, and a
# screen still saying what it said before the press reads as a press nobody heard.
indicator-stopping = Stopping
# The same, while delegates the turn started are still running, since the turn ends only when they
# have returned. The count is the delegates that have not, which is what the wait is for.
indicator-stopping-delegates = { $count ->
    [one] Stopping, waiting on { $count } delegate
   *[other] Stopping, waiting on { $count } delegates
    }
# Abbreviated counts, already rounded to one place.
tokens-thousands = { $thousands }k
tokens-millions = { $millions }M
# Said once a turn is over, because the end of one used to be announced by the indicator
# disappearing, and an announcement made by something vanishing is one nobody reads.
turn-done = turn { $turn } done
turn-failed = turn { $turn } failed
# The text of the terminal notice written when a turn ended or a question is waiting while the
# terminal was left. Fixed: it never carries a command, a path or any part of a reply.
notice-turn-finished = bravebot: turn finished
notice-waiting = bravebot: waiting for an answer
# Said of a turn the person stopped themselves, which is neither of the above.
turn-cancelled = turn { $turn } cancelled


## Picking up a session that ran somewhere, or on something, else

session-reopen-failed = could not reopen { $directory }: { $problem }
session-checkout-not-restored =
    { $count ->
        [one] checkout { $ids } was kept by this session but is not where it was made, so it is not listed
       *[other] checkouts { $ids } were kept by this session but are not where they were made, so they are not listed
    }
session-branch-moved = this session ran on { $was }; this checkout is on { $now }
session-branch-gone = this session ran on { $was }; this checkout is not on a branch
session-branch-new = this session ran on no branch; this checkout is on { $now }
session-build-differs = that session ran on bravebot { $was }; this is { $now }
session-front-differs = that session was written in { $was }; this is { $now }
session-front-terminal = the terminal
session-front-desktop = the desktop app


## Themes

theme-follows-terminal = follows your terminal, light or dark


## Answering a question the agent asked

ask-title = the agent is asking
ask-title-numbered = the agent is asking ({ $at } of { $total })
ask-own-words = Answer in my own words
ask-more-options = … { $count } more, use the arrow keys
ask-key-move = move
ask-key-pick-any = pick any
ask-key-pick-one = pick
ask-key-answer = answer
ask-key-skip = skip
ask-key-skip-question = skip the question
ask-key-back-to-options = back to the options
ask-key-back = back
ask-key-send = send
ask-key-change = change an answer
ask-review-title = review your answers
ask-review-heading = Nothing has been sent yet.
ask-review-skipped = (skipped)


## Handing the line to an editor

editor-none-configured = no editor found: set $VISUAL or $EDITOR to the one you want
editor-scratch-unusable = the file to edit could not be used: { $problem }
editor-named-but-missing =
    '{ $command }' was not found, and $VISUAL or $EDITOR names it, so nothing else was tried
editor-exited-badly = { $editor } exited with status { $code }, so what you had is unchanged
editor-was-stopped = { $editor } was stopped before it finished, so what you had is unchanged
# The editor a file was opened in, which the file is the person's own and may have been saved.
editor-file-exited-badly = { $editor } exited with status { $code }
editor-file-was-stopped = { $editor } was stopped before it finished
editor-would-not-start = { $editor } would not start: { $problem }


## The transcript

input-placeholder = Ask Brave Bot to do anything
quarantined-heading = untrusted · { $origin } · { $label }
transcript-more-lines = { $count ->
    [one] … { $count } more line
   *[other] … { $count } more lines
    }
transcript-earlier-lines = { $count ->
    [one] … { $count } earlier line
   *[other] … { $count } earlier lines
    }
# Said under output a program coloured: how many colour codes were taken out of the text and not
# drawn, so that a person can still tell they were there.
colour-codes-not-drawn = { $count ->
    [one] { $count } colour code not drawn
   *[other] { $count } colour codes not drawn
    }
transcript-unchanged = { $count ->
    [one] … { $count } unchanged line
   *[other] … { $count } unchanged lines
    }
# Said after what a finished call produced, where the call ran a model of its own inside itself: a
# confined check over quarantined content, or a processor's own round. How long it waited there and
# nothing about what came back. Without it a call that was slow because a model was slow reads as a
# slow program, and the figure that tells them apart was already measured.
transcript-waited = { $elapsed } at the model


## Reading back through the transcript

scroller-title = scroller
scroller-key-line = line up/down
scroller-key-half-page = half page   (also u / d)
scroller-key-full-page = full page   (also ctrl-f / ctrl-b)
scroller-key-ends = top / bottom   (also home / end)
scroller-key-prompts = previous / next prompt
scroller-key-search = search, next/previous match
scroller-key-count = a count first goes that many times as far
scroller-key-search-run = run it / delete, then abandon
scroller-key-expand = expand or collapse a call's result
scroller-key-editor = open the transcript in $EDITOR
scroller-key-delegates = open the delegates' views
scroller-key-this-list = this list
scroller-key-close = close the scroller   (also ctrl-c)
scroller-key-close-list = close this list
scroller-searching = enter to search  ·  esc to abandon
scroller-no-matches = no matches
scroller-match-of = { $at } of { $total }
scroller-search-keys = n next  ·  N previous  ·  esc clears  ·  q closes
scroller-rows-below = { $count ->
    [one] { $count } row below
   *[other] { $count } rows below
    }
scroller-footer = scroller
scroller-footer-keys = q closes  ·  ? keys
scroller-footer-search = / search


## Watching what a delegate is doing

# The footer of one delegate's own view. The kind and the number are the driver's words for it,
# never anything the model wrote.
watching-footer = { $kind } delegate { $number }
watching-footer-job = background { $name }
watching-working = working
watching-stopping = stopping
watching-answered = answered
watching-failed = did not finish
watching-position = { $at } of { $total }
watching-keys = q closes  ·  n / p another delegate
watching-keys-one = q closes
watching-keys-back = q goes back  ·  n / p another delegate
watching-keys-stop = x stops it
watching-back = ‹ back
watching-nothing-yet = nothing yet
# The list of every delegate this turn has spawned, with the session above them.
watching-list-title = delegates
# The list of what commands printed, opened by /outputs and its chord and not by the watch key.
watching-outputs-title = command output
watching-list-keys = up / down moves  ·  enter opens  ·  q closes
# The first row of the list: the conversation the delegates were spawned from.
watching-list-session = session
watching-list-session-detail = back to the conversation
watching-calls = { $count ->
    [one] { $count } call
   *[other] { $count } calls
    }
# Said on the bottom line once the agents list has anything to open, which is the one row that
# outlasts the turn that drew it. The count is there because a key with nothing behind it is not
# worth pressing. It counts the asides and the delegates, which the key opens, and nothing a
# command printed: those have their own clause below.
watching-hint = { $chord } { $count } to open
# The same line's clause for what commands printed, said while there is any. The chord and the
# slash command both open them, and the count is of what they open.
outputs-hint = { $chord } or { $command } { $count } to open
# Left in the transcript by /outputs when no command has run.
outputs-none = no command has run in this session, so there is no output to open
# Said on the bottom line for as long as a command the turn is waiting on can be moved, and gone
# the moment it ends or is moved. Short, because it shares the line with everything else there.
background-hint = { $chord } to background
# Said on the bottom line while the info panel is closed and the terminal is wide enough for it.
panel-hint = { $chord } info
# The info panel's last row.
panel-hide = { $chord } hide panel
# Left in the transcript when the info panel's key is pressed on a terminal too narrow for it.
panel-too-narrow = The info panel needs a terminal at least { $columns } columns wide.
# Left in the transcript by the first /caffeinate, which turns nothing on.
caffeinate-explained =
    /caffeinate keeps the computer from going to sleep while a turn runs or a loop waits for its
    next tick, and lets it sleep again once nothing is pending. The display can still turn off and
    the screen can still lock, but the machine keeps running, with your credentials on it, while
    you are away. Turn it on only where your device policy allows that. Type /caffeinate again to
    turn it on.
# Left in the transcript when /caffeinate turns on, and when it turns off.
caffeinate-on = the computer is kept awake while a turn runs or a loop waits
caffeinate-off = the computer may sleep again
# Left in the transcript when /thinking turns on, and when it turns off.
thinking-on = the model's thinking is shown as it arrives, for this session only
thinking-off = the model's thinking is hidden
# The heading over the model's thinking while it arrives.
transcript-thinking = thinking
transcript-copy-block = copy
# Left in the transcript when the program that holds off sleep would not start, which turns
# /caffeinate off.
caffeinate-unavailable = /caffeinate is unavailable: `{ $program }` could not be started ({ $reason })
# Left in the transcript when that program exited by itself, which turns /caffeinate off.
caffeinate-ended = /caffeinate is off: `{ $program }` stopped holding the computer awake
# What /lsp says where no language server has started, and where some have: a line, then one
# line for each.
lsp-none-started = No language server has started in this session. One starts the first time the model asks about a file it has a server for, after you approve it.
lsp-started = { $count ->
    [one] 1 language server has started in this session:
   *[other] { $count } language servers have started in this session:
    }
lsp-server-running = { $program }: runs with your own access until this session ends
# What /open says: a line, then a line for each link of the latest reply that has any, numbered as a
# number given back to /open names it. A link is as it was drawn, which is text the assistant wrote.
open-listed = { $count ->
    [one] 1 link in the latest reply that has one:
   *[other] { $count } links in the latest reply that has any:
    }
open-item = { $number }. { $link }
# Said after the last link /open lists, where the reply has more than it lists.
open-more = { $count ->
    [one] and 1 more, which no number opens: type it after /open
   *[other] and { $count } more, which no number opens: type one after /open
    }
open-none = No reply has a web address or a file path in it
# What /open <number> says where it has no list to read the number against, which is no list made
# since the session began, since /clear, or since another turn began, and where the number is not
# one of the list's.
open-list-first = list the links with /open first, then open one by its number
open-no-such =
    { $links ->
        [0] there is no link { $number }; the last list had none
        [one] there is no link { $number }; the last list had 1 link
       *[other] there is no link { $number }; the last list had { $links } links
    }
# A file is shown at the end of the transcript, which a running turn or command is writing.
open-not-while-working =
    a file is shown when nothing is running, since its lines go at the end of the transcript, where
    the work is writing; type it again then
# What /open edit <number> says where the number names an address, and where an editor would take
# the terminal that work is drawing on.
open-edit-an-address = an address opens in the browser, not in an editor: use /open <number>
open-edit-not-while-working =
    an editor takes the terminal, which the work that is running is drawing on; type it again when
    nothing is running
# What /open says where it was given text with :// in it that is not an address it opens.
open-not-an-address =
    that is not an address to open: only a web address with a host, as http or https and in plain
    ASCII, opens
# What /open says it takes, for `edit` with nothing after it.
open-takes =
    /open lists the links in the latest reply, /open <number>, /open <address> or /open <path>
    opens one, and /open edit <number> or /open edit <path> opens a file in your editor
# The info panel's section headings.
panel-session = Session
panel-model = Model
panel-goal = Goal
panel-goal-paused = paused
panel-context = Context
panel-language-servers = Language servers
panel-mcp-servers = MCP servers
panel-plan = Plan
panel-links = Links
# The rows of the info panel's Links section, each followed by the link.
panel-pull-request = Pull request
panel-issue = Issue
# Left in the transcript when a click on the session's name in the info panel finds the line in
# use, so /rename was not put in it.
panel-rename-line-in-use = The line is in use, so the session's name was not put in it. Clear the line and click the name again, or type /rename.
# The reasoning effort and the session's token total in the info panel, one to a row.
panel-effort = effort { $level }
panel-spent = { $tokens } this session
# The last turn's cache figures in the info panel, one to a row and never added together.
panel-cache-read = cache read { $tokens }
panel-cache-written = cache written { $tokens }
# Where the info panel has no room for the whole plan, a row after the rows it draws says how many
# rows of each kind it left out. A kind with none is not named, and the rows still to do come first.
# The forms of the other two are the same in English and differ in other languages.
panel-hidden-to-do = +{ $count } to do
panel-hidden-done = { $count ->
    [one] +{ $count } done
   *[other] +{ $count } done
    }
panel-hidden-cancelled = { $count ->
    [one] +{ $count } cancelled
   *[other] +{ $count } cancelled
    }
# The info panel's plan names the key that prints it whole, as its last row names the key that hides the panel.
panel-todos = { $chord } whole plan
# What the `?` list says the key that prints the whole plan does, after the key.
shortcut-todos = print the whole plan
# What /todos puts in the transcript above the plan it prints, and what it says where there is none.
todos-heading = plan: { $done } of { $total } done
# The same where some of its tasks were cancelled, which are counted apart from the done ones.
todos-heading-cancelled = plan: { $done } of { $total } done, { $cancelled } cancelled
todos-none = there is no plan in this session yet
# The line a turn ends with when the session's open loops changed (OPEN-3): each kind with its
# count, grouped by whose move it is. Names no file, command or task text.
open-loops-line = open: { $groups }
open-loops-nothing = open: nothing
open-loops-planner = { $items } (planner)
open-loops-person = { $items } (you)
open-loops-unbuilt = changes not built
open-loops-failed = the last command failed
open-loops-todos = { $count ->
    [one] { $count } todo
   *[other] { $count } todos
    }
open-loops-checkouts = { $count ->
    [one] { $count } kept checkout
   *[other] { $count } kept checkouts
    }
open-loops-uncommitted = { $count ->
    [one] { $count } file not committed
   *[other] { $count } files not committed
    }
open-loops-goal = a stopped goal
open-loops-jobs = { $count ->
    [one] { $count } job running
   *[other] { $count } jobs running
    }
# What /unfinished says, to the person only (OPEN-5).
unfinished-nothing = nothing is open
unfinished-unbuilt = changes not built, in the files written since the last command:
unfinished-failed = the last command failed: { $command } ({ $outcome })
unfinished-todos = { $count ->
    [one] { $count } todo is open:
   *[other] { $count } todos are open:
    }
unfinished-checkout = checkout { $id } is kept at { $path }: /checkouts apply { $id } brings it back, /checkouts remove { $id } removes it
unfinished-uncommitted = { $count ->
    [one] { $count } file this session wrote is not committed:
   *[other] { $count } files this session wrote are not committed:
    }
unfinished-goal-paused = the goal is paused: { $condition }. /goal resume judges turns against it again
unfinished-goal-stalled = the goal stopped sending the work back: { $condition }. Your next prompt is judged against it
unfinished-job = { $name } is still running, for { $seconds }s
# The status each task of a printed plan carries beside its mark.
todos-status-pending = pending
todos-status-active = in progress
todos-status-done = completed
todos-status-cancelled = cancelled

# The files state of the info panel, which lists what the session wrote (`/diff`).
# Its heading, above the list: how many files the session's write calls changed.
files-heading = { $count ->
    [one] { $count } file changed
   *[other] { $count } files changed
    }
# The control at the right end of that heading, which ends the state.
files-close = x
# Beside that heading after a resume, since the session record holds no write's changes.
files-earlier-not-kept = earlier edits are not kept
# The first and last rows of a list longer than its room, counting the files left out.
files-more-above = ↑ { $count } more above
files-more-below = ↓ { $count } more below
# The last row of that list when files arrived after the reader last pressed a key.
files-more-below-new = ↓ { $count } more below, { $new } new
# The row above the edits when some of their rows are scrolled out of view above, and the row below
# them for those out of view below.
files-rows-above = { $count ->
    [one] ↑ { $count } row above
   *[other] ↑ { $count } rows above
    }
files-rows-below = { $count ->
    [one] ↓ { $count } row below
   *[other] ↓ { $count } rows below
    }
# Over each edit to the highlighted file: its place among them and the turn it was made in.
files-edit-header = edit { $index } of { $total }, turn { $turn }
# The bar above the edits: the control for the edit before the one at the top, the place of that
# edit among them, and the control for the edit after it.
files-previous-edit = ← previous
files-edit-position = edit { $index } of { $total }
files-next-edit = next →
# After the highlighted file's name.
files-edit-count = { $count ->
    [one] { $count } edit
   *[other] { $count } edits
    }
# Left in the transcript by /diff before the session has written a file.
files-nothing-written = no file has been written in this session yet
# Left in the transcript by /diff on a terminal too narrow for the state.
files-too-narrow = The files view needs a terminal at least { $columns } columns wide.
# The last row of the panel while the state is open, naming its keys, and the hint line's where
# the panel is not drawn.
files-keys = ↑↓ file  ←→ edit  PgUp PgDn scroll  q back
# The info panel's Files section: its heading, and how many files the session wrote.
panel-files = Files
panel-files-total = { $count ->
    [one] { $count } file
   *[other] { $count } files
    }
# Said on the bottom line while the view is scrolled back off the tail, and gone once it reaches it.
held-hint = { $count ->
    [one] held, { $count } row below  ·  { $chord } to return
   *[other] held, { $count } rows below  ·  { $chord } to return
    }
# Said on the bottom line while a background job runs, and gone once the last one ends.
jobs-hint = { $count ->
    [one] 1 in the background
   *[other] { $count } in the background
    }
# Where a background job is, in the view a row opens and in /status. How long is this end's clock,
# counted from when the line started.
job-running = running { $ran_for }
job-ended-with-turn = stopped when the turn ended
# Where a job is between /jobs stop and the turn's next step, which is when the turn stops it.
job-stopping = being stopped
# A background job's name where a delegate started it: each delegate numbers its jobs from one.
job-of-delegate = { $name } of delegate { $number }
# One line of /jobs. The name is the one /jobs stop takes, and the standing is a job-* message above
# or how the job ended.
jobs-listed = { $name }: { $command }, { $standing }
# A line of /jobs for a delegate's job, named as /jobs stop takes it: the job's name, then the
# delegate's number, as in job:1 d2.
jobs-listed-of-delegate = { $name } { $number }: { $command }, { $standing }, started by delegate { $number }
jobs-none =
    there is no background job to list. A turn starts one when it runs a command in the
    background, and /jobs lists a turn's jobs until the next turn starts
jobs-no-such = there is no job { $name } to stop. /jobs lists the jobs there are
jobs-already-ended = { $name } has already ended
job-stop-asked = { $name } will be stopped at the turn's next step
job-stop-already-asked = { $name } is already being stopped
jobs-command-takes =
    /jobs lists the background jobs of this turn, and /jobs stop <name> stops one. A delegate's
    job takes the delegate's number too, as in /jobs stop job:1 d2


## The commands a line beginning with a slash may be

command-status = Report this session, what it may touch, and what it has spent
command-cost = Show what each turn of this session has spent
command-context = Show what fills the context window, by category
command-lsp = Say which language servers this session has started
command-open = List the links in the latest reply, and open one by its number
command-limit = Show the session's spend limit, set it in tokens or credits, or remove it
command-request = Show the last request sent to the model, and where each part of it came from
command-model = Choose which model to think with
command-theme = Choose which theme paints the interface
command-effort = Choose how hard to think before answering
command-variant = Pick a named set of options the model offers, list them, or drop the pick
command-advisor = Name the model the planner may consult, say which it may, or drop the choice
command-style = Choose how the planner answers: list the styles, pick one, or clear the pick
command-mode = Keep a skill applied to every turn: list skills, pick one, or leave
session-mode-set = the { $skill } skill is in force from the next turn, until you leave it with /mode off
session-mode-in-force = mode: { $skill }; available: { $skills }
session-mode-none = no mode; available: { $skills }
session-mode-cleared = mode left
session-mode-unknown = no skill called { $skill }; available: { $skills }
session-mode-resumed = this session was in the { $skill } mode, which is in force again
input-skill-mode = mode: { $skill }
command-config = Choose how the input box edits text
command-language = Choose the interface language for the next start, or drop the choice
command-add-dir = Open another directory and trust it for this session, or close one
command-reach = Remember a directory or credential for a command, or list and remove them
command-sandbox = Show the sandbox mode, or change it
command-cd = Work in another directory from now on, and trust it for this session
command-rename = Call this conversation something else
command-note = Note something on this session's record, without putting it in the conversation
command-branch = Copy this session and carry on in the copy, keeping the original to return to
command-handoff = Start a new session from a brief you can edit, written for the next goal
command-commit-message = Draft a commit message from the staged changes into the box, for you to edit
command-resume = Pick up another session of this directory, by id or from a list
command-background = Hand this session to a background process and leave it running
command-issue = Say which issue this session is for, show it, or clear it
command-pr = Say which pull request this session is for, show it, or clear it
command-compact = Summarise the conversation so far, keeping the recent part
command-btw = Ask something beside the work, without putting it in the conversation
command-recap = Recap where this session stands, without putting it in the conversation
command-clear = Start a new session here, keeping this one resumable
command-forget-trust = Stop remembering that this directory is trusted, so later sessions here ask
command-loop = Send a prompt again and again, say what is repeating, or stop it
command-after = Send a prompt once, after a delay
command-goal = Keep working until a condition you set is judged met
command-watch = List the files this session is watching, and stop one by its number
command-jobs = List this turn's background jobs, and stop one by its name
command-outputs = Open what the commands this session ran printed, background jobs included
command-sessions = Open the Sessions screen: every session and what each is doing
command-panel = Show or hide the info panel beside the transcript
command-diff = List the files the session wrote and show every edit to the one highlighted
command-todos = Print the whole plan, every task with its status
command-unfinished = List what this session left open, and whose move each is
command-caffeinate = Keep the computer awake while a turn or loop is pending
command-thinking = Show or hide the model's thinking as it arrives
command-checkouts = List kept checkouts, bring their files back, or remove one
command-plan = Enter plan mode, and start on a task if you give one
command-manifest = Plan one task in full, show you the plan, then run it with nothing re-planned
command-agent = Run one of your definitions on a task, by its name
command-memory = List each definition's memory, where it is kept and whether it is withheld
command-init = Have the planner draft an AGENTS.md for this project
command-review = Have the planner review local changes or a pull request
command-help = Ask a question about bravebot, answered from its own documentation
command-changelog = Show what changed in this version
command-export = Export the session transcript to a markdown file
command-copy = Put a reply on the clipboard; block copies a code block, text the drawn words
command-undo = Rewind one turn and put back the files it wrote
command-rewind = List the turns a rewind could go back to, or go back that many
command-hunks = List the hunks the last turn wrote, or put one back as it stood before the turn
command-exit = Leave


## When Enter carries out a command listed while work runs

# Carried out as soon as Enter is pressed.
command-when-now = now
# Waits for the work to end, in the order typed.
command-when-queued = queued
# Carried out at once or queued, by what the line says and what is already waiting.
command-when-either = now/queued


## Why a command typed while a turn runs waits

# Said under the turn when a command joins the queue. The turn holds the conversation, the workspace, the terminal and the network.
command-waits-for-the-turn = { $command } waits for the turn to end: it acts on what the turn holds or starts a turn.
# A command that is carried out as typed unless a line waiting ahead of it could change what it acts on. $line is that line: a command as it was typed, or what the line is where it is not a command.
command-waits-behind = { $command } waits for the turn to end, behind { $line }.
# Said under the box when a click on a row of the info panel that stands for a command is made while a command the person typed is running, which reads no line typed after it.
command-waits-for-the-command = { $command } waits for the command that is running to end.

## Where a skill offered after a slash was found

skill-from-project = (project)
skill-from-user = (user)
skill-from-built-in = (built-in)
skill-from-mcp-server = (MCP { $alias })
# Beside where it was found, on a skill row whose file says only the person may have it loaded.
skill-person-only = (only you)


## What the session says back

session-resumed = resumed session: { $title }
session-renamed = renamed to { $title }
session-rename-needs-a-name = /rename needs a name, as in /rename the parser bug
session-rename-needs-something = /rename needs a name with something in it
session-note-added = noted on turn { $turn }, in the record only: the planner is not given it
session-note-needs-text = /note needs the note, as in /note the wrong file was edited here
session-note-not-recorded = not noted: this session writes no record, so a note would be kept nowhere
session-note-listed = turn { $turn }: { $text }
# What /issue and /pr say back. A refused value is not repeated, since it may hold a control
# character.
session-issue-is = this session is for { $url }. /issue clear removes it
session-pull-request-is = this session's pull request is { $url }. /pr clear removes it
session-issue-none = no issue is set. /issue <url> sets one
session-pull-request-none = no pull request is set. /pr <url> sets one
session-issue-set = this session is for { $url }
session-pull-request-set = this session's pull request is { $url }
session-issue-cleared = the issue is cleared
session-pull-request-cleared = the pull request is cleared
session-issue-refused =
    /issue takes one http or https link on one line, in ASCII with no spaces, as in
    /issue https://github.com/brave/bravebot/issues/1. Nothing was set
session-pull-request-refused =
    /pr takes one http or https link on one line, in ASCII with no spaces, as in
    /pr https://github.com/brave/bravebot/pull/1. Nothing was set
session-cleared = cleared: a new session, with the previous one still resumable
# Left in the transcript by /branch, which has moved the session onto a copy of itself.
session-branched =
    branched: this session is now a copy, { $title }. The original is as it was. To return to it,
    run `bravebot --resume { $id }` in { $directory }
# Left in the transcript when /resume names the session that is already open.
session-resume-already-here = that is the session already open
# Left in the transcript when /resume names an id that is no session of this directory.
session-resume-no-such = no session with that id in this directory
# Left in the transcript when /resume names a session a running background session holds.
session-resume-held-by-background = that session is held by a running background session, so it cannot be resumed here
session-resume-held-by-another-process = that session is open in another terminal or window, so it cannot be resumed here
# Left in the transcript when a prompt named a session with @session:<id> and part of it was added.
session-mention-added = added { $chars } characters of session { $id } to the message
session-mention-added-cut = added the newest { $chars } characters of session { $id } to the message, and left out older turns
session-mention-no-such = no session { $id } in this directory, so nothing was added for it
session-mention-manifest = session { $id } is a manifest run with no conversation to quote, so nothing was added for it
session-mention-not-ours = session { $id } holds words this build cannot vouch for, so nothing was added for it
session-mention-private = session { $id } had been shown private content, so nothing was added for it
session-mention-empty = session { $id } has nothing to quote, so nothing was added for it
# A row of the @ list for an earlier session: when it was last written, the most a mention adds, and its title.
session-mention-row = { $when } · up to { $chars } characters · { $title }
session-branch-nothing-written = nothing to branch yet: the session has no record until its first turn ends
# Left in the transcript when /branch is typed where session records are not written.
session-branch-unwritable = /branch needs a session record to copy, and this session does not write one
# Left in the transcript when /branch is typed in a session that cannot be forked.
session-branch-refused = this session cannot be branched
session-branch-keeps-checkouts = { $count ->
    [one] the session keeps checkout { $ids }, which a copy would not carry, so remove it with /checkouts remove before /branch
   *[other] the session keeps checkouts { $ids }, which a copy would not carry, so remove them with /checkouts remove before /branch
    }
session-rewound = rewound the session to before turn { $turn }
session-rewound-partly =
    rewound the session to before turn { $turn }, but these files still hold what was
    written: { $paths }
session-rewind-uncovered = Some changes may remain. Not fully covered: { $causes }.
session-rewind-cause-command = commands
session-rewind-cause-hook = hooks
session-rewind-cause-scratch = scratch writes
session-rewind-cause-server = language servers
session-rewind-cause-desktop = desktop turns
session-rewind-cause-checkout = delegate checkouts
session-rewind-cause-backup = unavailable backups
session-rewind-cause-unknown = unknown coverage
session-nothing-to-undo = nothing left to undo in this session
session-rewind-points = a rewind goes back to one of these, putting back every row down to it:
# One point a rewind could reach: how many turns back it is, which turn it would land before,
# what that turn was asked, and every path it would put back.
session-rewind-point =
    { $turns } back: before turn { $turn }, { $asked }, puts back { $paths }
session-rewind-point-wrote-nothing =
    { $turns } back: before turn { $turn }, { $asked }, no files to put back
session-rewind-needs-a-number = /rewind takes how many turns to go back, as in /rewind 2
session-rewind-goes-no-further =
    { $kept ->
        [one] this session can go back one turn, no further
       *[other] this session can go back { $kept } turns, no further
    }
# Drawn after a line of a hunk that has no terminator, which only a file's final line can lack.
diff-no-final-newline = [no newline at end of file]
# What /hunks says. A hunk is a run of changed lines in a file the last turn wrote.
session-hunks-heading = turn { $turn } wrote these hunks. /hunks <number> puts one back, and leaves the rest of the file
session-hunks-wrote-nothing = turn { $turn } wrote no files, so there are no hunks to put back
# One hunk: the line it starts at in the file now, and how many lines it added and removed.
session-hunk-row = from line { $line }: { $added }, { $removed }
# A hunk that only removes lines has none of its own in the file now, so the line before it places it.
session-hunk-row-after = after line { $line }: { $removed }
session-hunk-row-at-start = at the start of the file: { $removed }
session-hunk-added =
    { $count ->
        [one] added 1 line
       *[other] added { $count } lines
    }
session-hunk-removed =
    { $count ->
        [one] removed 1 line
       *[other] removed { $count } lines
    }
# A file the last turn wrote that has no hunks to list, and why.
session-hunks-left-row = { $path }: { $why }
session-hunks-left-created = the turn created it, so a hunk cannot be taken out of it. /undo removes it, with the rest of the turn
session-hunks-left-not-kept = what it held before the turn was not kept, so it will not go back
session-hunks-left-untrusted =
    its contents are not vouched for, so they are not compared. Only /undo puts it back, with the rest of the turn
session-hunks-left-not-text = it is not text
session-hunks-left-too-different = so much of it changed that its hunks cannot be lined up
session-hunks-left-outside = it does not resolve inside the project now, so it is not read
session-hunks-left-unreadable = it is gone, is not a file, or is larger than a rewind keeps
session-hunks-left-as-it-was = it holds what it held before the turn
session-hunk-put-back = put back hunk { $number } of { $path } as it stood before turn { $turn }
session-hunk-not-put-back = hunk { $number } of { $path } was not put back: { $why }
session-hunk-changed =
    { $path } no longer holds hunk { $number } as it was listed, so nothing was written. /hunks lists
    what it holds now
session-hunk-failed =
    writing { $path } failed, so it may hold part of the change and is not vouched for: { $problem }
session-hunks-unavailable = /hunks could not begin: { $problem }
session-hunks-needs-a-number = /hunks takes the number of a hunk it listed, as in /hunks 2
session-hunks-more =
    { $count ->
        [one] 1 more hunk is not listed. /undo puts it back, with the rest of the turn
       *[other] { $count } more hunks are not listed. /undo puts them back, with the rest of the turn
    }
session-hunks-uncovered = the list may be missing changes. Not fully covered: { $causes }.
session-hunks-list-first = list the hunks with /hunks first, then put one back by its number
session-hunks-turn-moved =
    the session has had another turn since that list. /hunks lists the hunks of the last one
session-hunk-no-such =
    { $hunks ->
        [one] there is no hunk { $number }; the last list had 1 hunk
       *[other] there is no hunk { $number }; the last list had { $hunks } hunks
    }
session-exported = exported transcript to { $path }
session-export-failed = could not export transcript: { $problem }
session-copy-needs-a-number = /copy takes how many replies back to copy, as in /copy 2
session-copy-no-reply = there is no reply in this session to copy
session-copy-goes-no-further =
    { $replies ->
        [one] this session has one reply, so /copy goes no further back than it
       *[other] this session has { $replies } replies, so /copy goes no further back than { $replies }
    }
session-copy-block-needs-a-number = /copy block takes how many blocks back to copy, as in /copy block 2
session-copy-no-block = there is no code block in this session's replies to copy
session-copy-block-goes-no-further =
    { $blocks ->
        [one] this session's replies have one code block, so /copy block goes no further back than it
       *[other] this session's replies have { $blocks } code blocks, so /copy block goes no further back than { $blocks }
    }
session-copy-failed = could not put the reply on the clipboard
# What a copy took, drawn at the right of the footer. The ones that name where it came from are for
# a selection of something other than the transcript, which is where a drag is expected to be.
copied-to-clipboard = { $count ->
    [one] { $count } char to clipboard
   *[other] { $count } chars to clipboard
    }
copied-from-panel = { $count ->
    [one] { $count } char to clipboard, from the panel
   *[other] { $count } chars to clipboard, from the panel
    }
copied-from-box = { $count ->
    [one] { $count } char to clipboard, from the box
   *[other] { $count } chars to clipboard, from the box
    }
copied-from-status = { $count ->
    [one] { $count } char to clipboard, from the status line
   *[other] { $count } chars to clipboard, from the status line
    }
copied-from-offered = { $count ->
    [one] { $count } char to clipboard, from the list under the box
   *[other] { $count } chars to clipboard, from the list under the box
    }
copied-from-footer = { $count ->
    [one] { $count } char to clipboard, from the footer
   *[other] { $count } chars to clipboard, from the footer
    }
# A click on a web address in the transcript, when no browser could be started for it. The address is not repeated: it is text the assistant wrote.
session-link-not-opened = no browser could be started for that address, which can be selected and copied instead
# A click on a file path in the transcript shows the file's lines under a call. None of these repeats the path, which is text the assistant wrote.
session-preview-lines =
    { $count ->
        [one] { $count } line
       *[other] { $count } lines
    }
session-path-not-a-file = that path does not name a file in the directories this session works in
session-preview-denied = a permission rule says this session does not read that file
session-preview-too-large = that file is { $bytes } bytes, and a preview reads files of up to { $limit }
session-preview-unavailable = a preview could not begin: { $problem }
session-add-dir-needs-a-path = /add-dir needs a directory, as in /add-dir ~/notes
session-directory-added = added { $directory }, and trusting it for this session
session-directory-ends-checkouts = { $directory } holds the working directory, so no delegate is given a checkout while it is open; /add-dir close { $directory } ends that
session-close-dir-needs-a-path = /add-dir close needs a directory, as in /add-dir close ~/notes
session-directory-withdrawn = closed { $directory }, and no longer trusting it; open it again with /add-dir { $directory }
session-directory-not-closed = could not close { $directory }: { $problem }
session-cd-needs-a-path = /cd needs a directory, as in /cd ~/projects/other
session-directory-changed = now working in { $directory }, and trusting it for this session
# The mode the person was in, taken away by a settings layer of the directory they moved into.
session-bypass-made-unreachable = permissions.bypassUnreachable is set here, so bypassing is off and the session is asking again
session-bg-auto = /bg is refused in auto, because a session nobody is watching has no one to put a question to when the classifier does not allow it. Switch to another mode first, or use a one-shot run.
session-bg-bypass = /bg is refused in bypass, because a session nobody is watching does not run in a mode that answers every question. Switch to another mode first, or use a one-shot run.
session-bg-option-lost = /bg is refused because this session was started with { $flag }, which a background session does not receive. Start a session without that option to use /bg.
session-bg-sandbox-moved = /bg is refused because /sandbox moved this session to { $mode }, and a background session starts in { $settled }. Run /sandbox { $settled } first to use /bg.
session-bg-keeps-nothing-running = /bg is refused while a loop, a goal or a watch is running, because the background session would not run it. Stop it first.
session-bg-nothing-recorded = /bg hands over the session's record, and this session has none yet. Send a prompt first.
session-bg-incognito = /bg is refused in an incognito session, which writes no record to hand over.
# Said once per directory that was open and is not any more, so nobody discovers it by being
# refused a file they could read a minute ago.
session-directory-closed = closed { $directory }; open it again with /add-dir { $directory }
session-directory-not-changed = could not move to { $directory }: { $problem }
session-permission-rule-ignored = ignoring a permission rule in settings.json: { $problem }
session-model-pick-set-aside = ignoring { $model }, picked with /model, because no configured service serves it
# The other way a pick is set aside: the machine-level layer does not request it. The configured
# model answers and the record is left as it is, for the reason the line above leaves one.
session-model-pick-refused = ignoring { $model }, picked with /model: { $reason }
# An allow rule written in a checkout's settings file. It answers an approval prompt, which is a
# capability rather than a narrowing, so it is read from the person's own file only. Named rather
# than counted: whoever wrote it is looking for their own line.
session-permission-allow-ignored =
    not granting the allow rule { $rule } from { $path }: an allow rule answers a prompt, so a
    project's file proposes one and you grant it
# Said where a rule this project proposed was granted in an earlier session here, so the box does not
# ask about it again. Names the rule rather than counting, for the reason the line above it does, and
# names the file the answer is in: deleting a line from that file is the way back from having granted
# a rule.
session-permission-allow-granted-before =
    granting the allow rule { $rule } from { $path }, which you allowed this project before;
    { $record } is where that answer is kept
# Said once, at the top of a session the flag was given for. A person who did not mean to pass it
# should find out before the first write rather than after it, and the words name the flag so they
# can tell what to take off the command line. The line under the box says so for as long as it holds;
# this is what says it before anything has happened.
session-permissions-skipped =
    --dangerously-skip-permissions: nothing will be asked before a write, a command, or reading a
    file nobody vouched for. shift-tab to change
session-directory-not-added = could not add { $directory }: { $problem }
# The reason given for a name relative to a working directory whose own name is not text. Nothing
# written as text names that directory, so no path is made from it.
session-directory-root-not-text = the working directory's name is not text, so a name relative to it cannot be resolved; give the path in full
# Said when the session has nowhere of its own to write. Not a failure to start, but a person whose
# turn is told there is nowhere to put an intermediate file has nothing else to read it off.
session-scratch-unavailable = no scratch directory this session: { $problem }
session-using-model = using { $model }
# The picker row that said which service answers is gone by the time this is read, and the same
# name reached through two services is two bills and two credentials.
session-using-model-from = using { $model } from { $service }
# Said before the screen is handed to the AWS CLI, so a terminal filling with its output, and a
# browser opening, are accounted for rather than looking like something having gone wrong.
session-signing-in = signing in to AWS; follow the instructions below, and this returns when it is done
session-context-budget = compacting above { $budget } tokens, as this model advertises
session-models-unavailable = could not list models: { $problem }
# Said when the picker opens with a ChatGPT sign-in stored and its listing could not be had. The first
# is for a sign-in file that could not be read where a settings entry names the models, which offers no
# row. The second is for a listing that could not be had, where the picker shows the built-in list. The
# third is for a listing the service refused in a code that names the account, where signing in again
# is no remedy.
session-chatgpt-models-unreadable = could not list the ChatGPT account's models; try again or sign in again with bravebot auth login chatgpt, or name one, as in `bravebot -i "<prompt>" --model chatgpt/<model>` or `bravebot "<task>" --model chatgpt/<model>`
session-chatgpt-models-compiled = could not list the ChatGPT account's models, so /model shows a built-in list that may name a model the account cannot reach; try again or sign in again with bravebot auth login chatgpt, or name one, as in `bravebot -i "<prompt>" --model chatgpt/<model>` or `bravebot "<task>" --model chatgpt/<model>`
session-chatgpt-models-refused = could not list the ChatGPT account's models: { $refusal }
# Said when the picker opens and a stored xAI sign-in's listing could not be had, so none of the account's
# models are offered. A sign-in that was never made is not said here.
session-xai-models-unreadable = could not list the xAI account's models; try again or sign in again with bravebot auth login xai, or name one, as in `bravebot -i "<prompt>" --model xai/<model>` or `bravebot "<task>" --model xai/<model>`
# Said when the picker opens and a gateway whose settings name no models could not be asked for its list,
# so none of its models are offered. A gateway whose key is missing is not asked and is not said here.
session-gateway-models-unreadable = could not read the list of models from { $gateway }, so none of its models are offered; try again, or name one, as in `bravebot -i "<prompt>" --model { $id }/<model>` or `bravebot "<task>" --model { $id }/<model>`
session-theme-set = theme { $theme }
session-no-such-theme = no theme named { $theme }; try /theme for the list
session-editing-vi = editing the way vi does; esc for commands, i to type
session-editing-ordinary = editing with the arrows and the readline chords
session-effort-set = thinking at { $effort }
session-effort-unset = thinking as the service decides
session-no-such-effort = no effort level named { $effort }; try /effort for the list
session-effort-not-read = this model reads no effort level, so requests carry none
# Said where the level chosen is not the one a session on a ChatGPT model began with, because that
# account is sent one level for a whole session. Both are level words or the word for the default.
session-effort-kept-for-account = a ChatGPT session keeps the effort level it began with, { $began }, so { $chosen } applies from the next session, which /clear begins
session-language-in-use = the interface is in { $language }; catalogs: { $available }; /language <tag> saves a choice for the next start and /language auto drops it
session-language-saved = saved choice: { $language }
session-language-set = the interface will be in { $language } from the next start, unless BRAVEBOT_LOCALE is set, which answers first
session-language-auto = the interface will follow the settings file and the environment from the next start
session-language-not-saved = this session cannot keep a choice, so the language is unchanged
session-no-such-language = no catalog for { $language }; the catalogs are { $available }
session-variant-set = requests to this model carry the { $variant } variant from the next turn
session-variant-dropped = no variant; requests carry the model's own options alone
session-variant-in-force = the { $variant } variant is in force; this model offers { $names }
session-variant-listed = no variant is in force; this model offers { $names }
session-variant-none-offered = { $model } offers no variants
session-no-such-variant = this model has no variant named { $variant }; it offers { $names }
session-advisor-set = the planner may consult { $model } from the next turn, which spends that model's tokens
session-advisor-in-force = the planner may consult { $model }
session-advisor-none = no advisor; try /advisor followed by a model name
session-advisor-dropped = advisor dropped
session-advisor-dropped-setting-remains = advisor dropped; the advisorModel setting still names { $model }
session-advisor-nothing-serves = nothing is configured to answer { $model }, so it cannot advise
session-advisor-needs-sign-in = { $model } needs a sign-in first, so it cannot advise
session-style-set = answering in the { $style } style from the next turn
session-style-in-force = style: { $style }; available: { $styles }
session-style-none = no style; available: { $styles }
session-style-set-but-replaced = style { $style } picked, but --system-prompt supplies the opening for this run, so it does not show
session-style-cleared = style cleared
session-style-unknown = no style called { $style }; available: { $styles }
session-trusting = trusting { $directory }
session-trusting-as-left = trusting { $directory } (as this session left it)
# Said where the question was never put, because the mode in force answers it. Naming the flag is
# the point: this is the one grant a person did not make by pressing a key, so the line has to say
# what made it.
session-trusting-unasked =
    trusting { $directory } (--dangerously-skip-permissions, so you were not asked)
# Said where the question was not put because an earlier session here was answered yes and the yes
# was kept (TRUST-23). When it was given and how to take it back, because this is a grant nobody made
# in this session and the line is the only thing on the screen that says where it came from.
session-trusting-kept =
    trusting { $directory } (you trusted it { $when }; /forget-trust to be asked again)
session-trusting-kept-root =
    trusting { $directory }, inside { $root } (you trusted { $root } { $when }; /forget-trust to be asked again)
session-trust-kept =
    trusting { $directory }, and later sessions started here will not ask; /forget-trust takes it back
# The answer was given, but writing it down failed, so the next session will ask after all.
session-trust-not-kept =
    trusting { $directory } for this session only: the answer could not be written to { $path }, so the next session here will ask
session-trust-forgotten =
    the next session started in { $directory } will ask whether to trust it; this one keeps its answer, and /clear starts one that asks
session-trust-forgotten-root =
    the next session started in { $directory } or anywhere else in { $root } will ask whether to trust it; this one keeps its answer, and /clear starts one that asks
session-trust-nothing-to-forget = no answer about { $directory } is kept, so there is nothing to forget
session-trust-not-forgotten = the answer kept in { $path } could not be removed: { $error }
# Incognito writes nothing, and removing a line is a write.
session-trust-forget-incognito =
    an incognito session changes nothing on disk, so any answer kept about this directory stays in { $path }
session-not-trusting = this directory is not trusted; every write will be shown to you
session-vouched-for = trusting { $path } for this session
# Said when a person agrees that a file the scan found a credential in may reach the model.
session-exposed = showing { $path } to the model until the session ends or you change directory, credential and all
# Said when the person pressed the standing key at a vetting prompt. What it changes is that a
# later prompt does not appear, so it is the one decision here they would otherwise see no record
# of, and the file is named because that is where they undo it.
session-vetting-on =
    a check that finds nothing will now read content to the model without asking (~/.bravebot/vetting)
# Said at the top of a session that opened with the mode already on, whichever of the three routes
# turned it on. A question that was never put is the one thing a person cannot read off a
# transcript, so it has to be said before the first slot reaches it.
session-vetting-in-force =
    a check that finds nothing reads content to the model without asking you
# Said once at the top of a session when a newer release has been published. The command is
# passed in rather than written here: it is a line somebody pastes into a shell, and which one it
# is depends on how this copy was installed, so it is not a translator's to reword. The newer
# version is not named: the one on disk is as old as the last launch that asked, and the command
# installs whatever is newest when it runs.
update-available =
    a newer bravebot is out (this is { $running }); update with: { $command }
# Printed by `bravebot update`. The command is passed in for the reason it is above: which line
# updates this copy depends on how it was installed, and it is not a translator's to reword.
# Nothing is asked of a registry, so this says how to update rather than whether to.
update-how = update with: { $command }
# Printed by `bravebot update` for a copy neither the npm package nor the install script put
# here, a build from source above all. There is no command to name, and naming one would replace
# a binary this program did not install.
update-not-installed-by-us =
    this copy was not installed by the npm package or by the install script, so there is no update
    command for it. A build from source is updated by pulling and building again.
# Said once, on the startup screen of the first launch that sends crash reports (CRASH-9). The key
# and the variable are the two ways of turning it off, and are not a translator's to reword.
crash-reports-notice =
    if bravebot crashes, a report is sent to Brave on the next launch. It holds the version, your
    operating system and the line of bravebot's source that failed, and never your prompts, files
    or keys. To turn this off, set crashReports to false in your settings file, or set
    BRAVEBOT_CRASH_REPORTS=0.
session-started-server = running the { $language } language server for this session ({ $program })
# Said when a person agrees to offer a server's tools to the model. The list is recorded, so it
# is offered again in later sessions until the server's list changes.
session-offered-tools =
    { $count ->
        [one] offering { $alias }'s one tool to the model
       *[other] offering { $alias }'s { $count } tools to the model
    }
# Said when a person answers that a server's tool may be called without asking, in this project.
session-stands-for-tool = calling { $tool } without asking in this project
session-answered-already = answered already: { $question }
# Said when a question was closed because no key was pressed for the time askIdleSeconds names. The
# planner was told no answer was given and the person may be away; this is what the person finds when
# they come back to a conversation that went on without them.
session-question-timed-out = { $seconds ->
    [one] a question was closed after { $seconds } second with no key pressed, and the planner was told you may be away
   *[other] a question was closed after { $seconds } seconds with no key pressed, and the planner was told you may be away
    }
session-something-was-refused = a policy gate refused something during that turn
# Said under a finished turn for refusals that no later call to the same tool got past. A refusal
# a later call got past is left to the audit trail.
session-refusals-left =
    { $count ->
        [one] the turn ended with a refused call left undone. Refused: { $first }
       *[other] the turn ended with { $count } refused calls left undone. First refused: { $first }
    }
# The endpoint substitutes a model it will not serve rather than refusing, so without this a
# session can ask for one model and be answered by another with nothing said.
session-model-substituted =
    { $asked } was not served: the endpoint answered with { $served }. Run `bravebot doctor` if a
    subscription was expected.
session-error = error: { $problem }
session-no-output = no output
shell-kept-private = typed with !!, so this output was not sent to the model
shell-stopped-not-sent = what it printed before it stopped was not sent to the model

## Why a turn failed

# Fixed descriptions keep raw backend error text out of the interface.
failure-unauthorized = the service would not accept the credentials
failure-rate-limited = the service asked for fewer requests
failure-rate-limited-for =
    { $minutes ->
        [one] the service asked for fewer requests, and for a wait of about 1 minute
       *[other] the service asked for fewer requests, and for a wait of about { $minutes } minutes
    }
failure-unavailable = the service could not answer
failure-refused = the service rejected the request
failure-transport = the request did not get through
failure-incomplete = the reply stopped before it was finished
failure-undecodable = the reply could not be read
failure-too-long = the model reached its output limit
failure-too-long-at = the model reached its output limit of { $tokens } tokens, which BRAVEBOT_OUTPUT_BUDGET raises
failure-too-long-in-call = the model reached its output limit of { $tokens } tokens, which BRAVEBOT_OUTPUT_BUDGET raises, part way through a call to { $tool }, so the call was not made
failure-too-long-in-a-call = the model reached its output limit of { $tokens } tokens, which BRAVEBOT_OUTPUT_BUDGET raises, part way through a tool call, so the call was not made
failure-too-long-thinking = the model reached its output limit of { $tokens } tokens, which BRAVEBOT_OUTPUT_BUDGET raises, while it was still thinking
failure-unconfigured = nothing here was configured to send the request
failure-blocked = a gate here would not let the request out
failure-workspace = the workspace could not be used
failure-internal = something went wrong here
# Wrapped around the reason rather than written into each of them, so a status or a count of tries
# is said the same way whatever went wrong.
failure-with-status = { $what } (HTTP { $status })
failure-with-attempts = { $what }, after { $attempts } attempts
# A failure of a ChatGPT account is said in the sentence of its own step, the one a one-shot run
# reports for it (CHATGPT-8). No count of tries is added, and a status follows only the sentence of a
# refusal that came with one.
failure-account-not-signed-in = not signed in to a ChatGPT account. Run `bravebot auth login chatgpt` to sign in
failure-account-ended = the ChatGPT sign-in has ended. Run `bravebot auth login chatgpt` to sign in again
failure-account-token-refused = OpenAI refused the ChatGPT sign-in's access token. Run `bravebot auth login chatgpt` to sign in again
failure-account-unusable = the stored ChatGPT sign-in could not be read. Run `bravebot auth status chatgpt` to see why
failure-account-usage-limit = OpenAI says this ChatGPT account has reached its usage limit
failure-account-not-permitted = OpenAI does not allow this ChatGPT account to be used this way
# A refusal that came as a 429 with a wait it asked for says the wait after its sentence. The request is
# not sent again, so the whole of the wait is ahead.
failure-account-wait =
    { $minutes ->
        [one] { $what }, and the service asked for a wait of about 1 minute
       *[other] { $what }, and the service asked for a wait of about { $minutes } minutes
    }
failure-account-renewal-refused = the ChatGPT sign-in could not be renewed: the request was refused
failure-account-renewal-rate-limited = the ChatGPT sign-in could not be renewed: the request was rate-limited
failure-account-renewal-unavailable = the ChatGPT sign-in could not be renewed: the request was met with an unavailable service
failure-account-renewal-incomplete = the ChatGPT sign-in could not be renewed: the request was answered in a shape that could not be read
failure-account-renewal-transport = the ChatGPT sign-in could not be renewed: the request was not completed


## What the desktop window says of the ChatGPT account

# One sentence each, in sentence case, saying what happened and what to do, for the window's error card,
# its run card and the model picker. They are chosen from the step that failed and repeat nothing OpenAI,
# the sign-in file or a settings file said. The window draws { $command } as code, so it is passed in and
# sent beside the sentence and is not written into the words with backticks. The terminal's sentences for
# the same failures are lower case and take the status of a refusal after them, which a card cannot.
#
# The window and an editor read the reference catalog whatever language the machine is set to
# (`bravebot-rpc` chooses en-US, `bravebot-host` and `bravebot-acp` choose none), so no program reads the
# French of these messages. They are kept so that both catalogs hold the same messages and
# contrib/untranslated-messages.txt lists no gap.
desktop-chatgpt-not-signed-in = You are not signed in to ChatGPT, so run { $command } in a terminal to sign in.
desktop-chatgpt-ended = The ChatGPT sign-in has ended, so run { $command } in a terminal to sign in again.
desktop-chatgpt-token-refused = OpenAI refused the ChatGPT sign-in's access token, so run { $command } in a terminal to sign in again.
desktop-chatgpt-unusable = The stored ChatGPT sign-in could not be read, so run { $command } in a terminal to see why.
desktop-chatgpt-renewal-refused = OpenAI refused to renew the ChatGPT sign-in, so try again later.
desktop-chatgpt-renewal-rate-limited = The ChatGPT sign-in could not be renewed because OpenAI is limiting requests, so try again later.
desktop-chatgpt-renewal-unavailable = The ChatGPT sign-in could not be renewed because OpenAI's sign-in service is unavailable, so try again later.
desktop-chatgpt-renewal-incomplete = The ChatGPT sign-in could not be renewed because the reply from OpenAI was incomplete, so try again.
desktop-chatgpt-renewal-transport = The ChatGPT sign-in could not be renewed because OpenAI could not be reached, so check the connection and try again.
desktop-chatgpt-usage-limit = OpenAI says this ChatGPT account has reached its usage limit, so try again later.
# Said where the refusal came as a 429 that asked for a wait, for either refusal, as the terminal says it.
# The request is not sent again, so the whole of the wait is ahead.
desktop-chatgpt-usage-limit-wait =
    { $minutes ->
        [one] OpenAI says this ChatGPT account has reached its usage limit, so try again in about 1 minute.
       *[other] OpenAI says this ChatGPT account has reached its usage limit, so try again in about { $minutes } minutes.
    }
desktop-chatgpt-not-permitted = OpenAI does not allow this ChatGPT account to be used this way, so use another account or another service's model.
desktop-chatgpt-not-permitted-wait =
    { $minutes ->
        [one] OpenAI does not allow this ChatGPT account to be used this way and asked for a wait of about 1 minute, so use another account or another service's model.
       *[other] OpenAI does not allow this ChatGPT account to be used this way and asked for a wait of about { $minutes } minutes, so use another account or another service's model.
    }
# What the window narrates where a turn goes on or moves on after the account failed on the way: a summary
# it could not make, and the model it moved off for the person's fallbackModel. The first is followed by the
# account's sentence above, so each line is one sentence.
desktop-compaction-failed = The conversation could not be summarised, so the turn goes on with the messages it has.
desktop-chatgpt-fell-back-rate-limited = This turn moves from { $from } to { $to } because the ChatGPT sign-in could not be renewed while OpenAI is limiting requests.
desktop-chatgpt-fell-back-unavailable = This turn moves from { $from } to { $to } because the ChatGPT sign-in could not be renewed while OpenAI's sign-in service is unavailable.
desktop-chatgpt-fell-back = This turn moves from { $from } to { $to } because the ChatGPT account could not be used.
# The model picker's warnings, one for each way the account's models could not be listed. The first is for a
# sign-in file that could not be read where a settings entry names the models, which offers no row. The
# second is for a listing that could not be read, where the picker shows the built-in list. The last two are
# for a listing the service refused in a code that names the account, where signing in again is no remedy.
desktop-chatgpt-models-unreadable = Could not load models from ChatGPT, so try again, or run { $command } in a terminal to sign in again.
desktop-chatgpt-models-compiled = Could not load models from ChatGPT, so the list shown is built in and may name a model the account cannot reach; try again, or run { $command } in a terminal to sign in again.
desktop-chatgpt-models-usage-limit = Could not load models from ChatGPT because the account has reached its usage limit, so try again later.
desktop-chatgpt-models-not-permitted = Could not load models from ChatGPT because OpenAI does not allow this account to be used this way, so use another account or another service's model.
# The model picker's warning for the xAI account's models, said where a sign-in is stored and its listing
# could not be had. The window draws { $command } as code.
desktop-xai-models-unreadable = Could not load models from xAI, so try again, or run { $command } in a terminal to sign in again.

## What the desktop window says of the xAI account

# One sentence each, in sentence case, saying what happened and what to do, for the window's error card and
# its run card. They are chosen from the step that failed and repeat nothing xAI, the sign-in file or a
# settings file said. The window draws { $command } as code, so it is passed in and sent beside the sentence
# and is not written into the words with backticks. The window reads the reference catalog whatever
# language the machine is set to, so the French of these messages is read by no program and is kept so that
# both catalogs hold the same messages.
desktop-xai-not-signed-in = You are not signed in to xAI, so run { $command } in a terminal to sign in.
desktop-xai-ended = The xAI sign-in has ended, so run { $command } in a terminal to sign in again.
desktop-xai-token-refused = The xAI sign-in's access token was refused, and so was a new one, so run { $command } in a terminal to sign in again.
desktop-xai-not-permitted = The request was refused by xAI for this account, and signing in again will not change that, so check the plan at xAI or choose another model.
desktop-xai-unusable = The stored xAI sign-in could not be read, so run { $command } in a terminal to see why.
desktop-xai-renewal-refused = The xAI sign-in could not be renewed because xAI refused the request, so try again later.
desktop-xai-renewal-rate-limited = The xAI sign-in could not be renewed because xAI is limiting requests, so try again later.
desktop-xai-renewal-unavailable = The xAI sign-in could not be renewed because xAI is unavailable, so try again later.
desktop-xai-renewal-incomplete = The xAI sign-in could not be renewed because the reply from xAI could not be read, so try again.
desktop-xai-renewal-transport = The xAI sign-in could not be renewed because xAI could not be reached, so check the connection and try again.
desktop-xai-fell-back-rate-limited = This turn moves from { $from } to { $to } because the xAI sign-in could not be renewed while xAI is limiting requests.
desktop-xai-fell-back-unavailable = This turn moves from { $from } to { $to } because the xAI sign-in could not be renewed while xAI is unavailable.
desktop-xai-fell-back = This turn moves from { $from } to { $to } because the xAI account could not be used.


## Repeating a prompt

loop-needs-a-prompt =
    /loop needs something to repeat, as in /loop 5m check the deploy, or /loop watch the build to
    let each turn say when to run again
loop-started-every =
    loop { $number }: repeating every { $every }; /loop stop { $number } ends it, and so does ctrl-c or leaving
loop-started-self-paced =
    loop { $number }: repeating at a pace each turn sets; /loop stop { $number } ends it, and so does ctrl-c or leaving
# The answer to the bare command. The line is in it because the note above scrolls away, and
# somebody asking what is repeating has usually lost sight of what they set going.
loop-active = loop { $number }, repeating: { $prompt } · { $pace } · { $when }
loop-ends-with = /loop stop <number> ends one, /loop stop alone ends the only one, and ctrl-c or leaving ends them all
loop-none =
    nothing is repeating. /loop 5m check the deploy sends a line every five minutes, /loop watch the
    build lets each turn say when to run again, and /loop stop <number> ends one of them
# The part of the row under the box that says a loop is live, which between ticks is the only thing
# on the screen that does. Short on purpose: it shares that row with the mode and the readings, and
# a part a narrow terminal has no room for is a part the row gives up.
loop-hint = looping
loop-hint-next = looping, next in { $next }
loop-hint-several = { $count } loops
loop-hint-several-next = { $count } loops, next in { $next }
loop-interval-raised = the interval was raised to { $every }, which is as fast as a loop goes
loop-interval-capped = the interval was capped at { $every }, which is as long as a loop lives
loop-full = { $count } loops are already repeating, which is as many as a session holds; /loop stop <number> ends one
loop-stop-which = { $count } loops are repeating, so /loop stop needs the number of the one to end; /loop lists them
loop-no-such = no loop is numbered { $number }; /loop lists what is repeating
loop-stopped-numbered = loop { $number } is stopped
loop-stopped-all = all { $count } loops are stopped
loop-cleared-all = all { $count } loops are stopped, because they belonged to the session that was cleared
loop-tick = loop { $number }, tick { $count }
loop-tick-quiet = { $quiet ->
    [one] loop { $number }, tick { $count }, after { $quiet } tick that found nothing
   *[other] loop { $number }, tick { $count }, after { $quiet } ticks that found nothing
    }
loop-stopped = the loop is stopped
loop-cleared = the loop is stopped, because it belonged to the session that was cleared
loop-aged-out = loop { $number } has run for a week and stopped itself
loop-unpaced = that turn did not say when to run again, so loop { $number } has stopped
loop-finished = that turn said the loop is finished, so loop { $number } has stopped
loop-busy = /loop starts with a turn of its own, so it waits until this one is done
loop-replaces-goal =
    the goal that was set has been cleared: a session works towards one thing at a time
loop-armed-by-the-turn =
    loop { $number }: looking again in { $after }, repeating what you asked; /loop stop { $number }
    ends it, and so does ctrl-c or leaving
loop-not-armed-under-a-goal =
    a later look was asked for and not started: this session is working towards a goal, and it
    does one thing at a time
loop-not-armed-under-a-watch =
    a later look was asked for and not started: this session is already watching a file, and it
    does one thing at a time

after-needs-a-delay =
    /after needs a delay and something to send, as in /after 5m check the deploy
after-started =
    sending that once, in { $delay }, as loop { $number }; /loop stop { $number } cancels it, and so does ctrl-c or leaving
after-active = loop { $number }, sending later: { $prompt } · { $when }
after-sent = sending what /after held
after-hint = sending later
after-hint-next = sending in { $next }
after-delay-raised = the delay was raised to { $delay }, which is as short as /after goes
after-delay-capped = the delay was capped at { $delay }, which is as long as /after waits


## Working towards a condition

goal-set =
    working towards: { $condition }. Nothing runs until you send something; from then on each
    turn is judged against it. Ctrl-c takes it off, and so does leaving
goal-replaced = the goal that was set has been replaced
goal-cleared = the goal is cleared
goal-paused =
    the goal is paused: turns are not judged against it until /goal resume, and /goal clear
    still takes it off
goal-already-paused = the goal is already paused; /goal resume arms it again
goal-resumed = the goal is resumed: the next turn is judged against it again
goal-not-paused = the goal is not paused
goal-none =
    no goal is set. /goal <condition> sets one, as in /goal cargo test exits 0, and /goal clear
    takes it off again
goal-active = working towards: { $condition }
goal-last-check = the last check said: { $reason }
goal-usage = it has run for { $elapsed } and the session has spent { $tokens } since it was set
goal-never-checked = nothing has been judged against it yet
goal-not-met = the goal is not met yet: { $reason }
goal-not-met-unsaid = the goal is not met yet, and the check did not say what is missing
goal-met = the goal is met: { $reason }
goal-met-unsaid = the goal is met
goal-impossible = the goal cannot be met, so it is cleared: { $reason }
goal-unreadable =
    the check did not answer with a verdict, so there is nothing to act on and the goal is
    cleared
goal-quarantined =
    this conversation has met untrusted content, so a verdict about it is not something this
    program may act on; the goal is cleared
goal-spent =
    the goal has sent the work back { $rounds } times without being met, and has stopped rather
    than carrying on
goal-stalled =
    the goal has sent the work back without a turn making a tool call, and has stopped sending
    it; it is still set, and your next prompt is judged against it
goal-failed = the goal could not be checked ({ $problem }), so it is cleared
goal-ended-unexpectedly = the goal check ended unexpectedly
goal-replaces-loop =
    the loop that was running has been stopped: a session works towards one thing at a time


## Being told when a file changes

watch-armed =
    watch { $number } is on { $path }: you will be told when it is written to, removed or appears,
    with no turn running. /watch lists them, /watch stop { $number } ends this one, and ctrl-c
    ends them all
watch-not-armed-under-a-loop =
    a watch on a file was asked for and not armed: a loop is running, and a session does one
    thing at a time that happens without anybody typing
watch-not-armed-under-a-goal =
    a watch on a file was asked for and not armed: this session is working towards a goal, and it
    does one thing at a time
watch-not-armed-full =
    a watch on a file was asked for and not armed: { $count } are already live, which is as many
    as a session keeps. /watch stop <n> ends one
watch-not-armed-unreadable =
    a watch on { $path } was asked for and not armed: that path cannot be looked at from here
watch-fired = watch { $number }: { $path } looks written to
watch-fired-removed = watch { $number }: { $path } no longer exists
watch-fired-appeared = watch { $number }: { $path } now exists
watch-listed = watch { $number }: { $path }, armed by turn { $turn }, { $left } left
watch-none =
    nothing is being watched. A turn arms a watch when you ask to be told about a file, and
    /watch stop <n> ends one
watch-no-such = there is no watch { $number }. /watch lists the live ones
watch-command-takes =
    /watch lists what this session is watching, and /watch stop <n> ends the one with that
    number
watch-stopped = watch { $number } is stopped
watch-stopped-with-its-turn =
    watch { $number } is stopped, since stopping the turn it started is how you say you have
    finished with it
watch-aged-out = watch { $number } has been on for a week and has stopped itself
watch-out-of-reach =
    watch { $number } is stopped: this session no longer reaches the path it was on
watches-stopped = { $count ->
    [one] { $count } watch is stopped
   *[other] { $count } watches are stopped
    }
watches-replaced = { $count ->
    [one] { $count } live watch has ended: a session does one such thing at a time
   *[other] { $count } live watches have ended: a session does one such thing at a time
    }
watches-cleared = { $count ->
    [one] { $count } live watch has ended with the conversation it was armed in
   *[other] { $count } live watches have ended with the conversation they were armed in
    }

## The checkouts a delegate kept

checkouts-listed = { $id }: made for delegate { $delegate } of commit { $commit }, at { $path }
# The size is a number of kilobytes, megabytes or gigabytes.
checkouts-size = { $id }: it took { $size } on disk when its delegate ended
checkouts-size-partial =
    { $id }: it took at least { $size } on disk when its delegate ended, since not all of it could be measured
checkouts-size-unmeasured = { $id }: its size is measured when its delegate ends
# The remote is a remote branch as git names it, such as origin/main.
checkouts-pushed =
    { $id }: on branch { $branch }, and { $remote } is at the same commit, so that commit is pushed
checkouts-detached-pushed =
    { $id }: on no branch, and { $remote } is at the same commit, so that commit is pushed
checkouts-unpushed = { $id }: on branch { $branch }, at a commit no remote branch is at
checkouts-detached-unpushed = { $id }: on no branch, at a commit no remote branch is at
checkouts-head-unread =
    { $id }: which commit it is at could not be read, so whether that commit is pushed is not known
checkouts-nothing-done = { $id }: nothing was recorded done in it
# Followed by the names of the files written, separated by commas, which are left as they are.
checkouts-written = { $id }: written in it: { $paths }
checkouts-more = { $count } more
checkouts-referenced = { $count ->
    [one] { $id }: { $count } write through a reference, whose path was not recorded
   *[other] { $id }: { $count } writes through a reference, whose paths were not recorded
    }
checkouts-unread =
    { $id }: its status was not read, so a file changed other than by a write is not named here
checkouts-none =
    this session keeps no checkout. A delegate given one keeps it when something was done in it
# The path is where the directory is, and is left as it is.
checkouts-unlisted =
    { $id } at { $path }: no session record lists it, and another session may be using it. If none is, delete the directory and run git worktree prune to remove it
checkouts-no-such = this session keeps no checkout { $id }. /checkouts lists the ones it keeps
checkouts-command-takes =
    /checkouts lists the checkouts this session keeps, /checkouts apply <n> [path ...] brings back
    the files written in the one with that number, or only the paths named, and /checkouts remove <n>
    removes it
# Followed by a line for each file, as the write gate worded it.
checkouts-applied = files from checkout { $id } were brought back:
checkouts-not-applied = nothing from checkout { $id } was brought back:
checkouts-removed = checkout { $id } at { $path } is removed
checkouts-not-removed = checkout { $id } at { $path } could not be removed, and is still kept
checkouts-worked-from =
    checkout { $id } at { $path } is kept, since the working directory or a directory added with /add-dir is inside it
checkouts-kept = checkout { $id } is kept
## The memory each definition keeps

memory-none = no definition loaded here keeps a memory
# The path is where the memory is kept, and is left as it is.
memory-withheld = { $name }: { $path }, withheld, since the map does not trust that path
memory-withheld-recorded =
    { $name }: { $path }, withheld, since a write left that path untrusted and the record of it still names the path
memory-not-read = { $name }: { $path }, not read, since it is a link, is reached through one or is not a file
memory-empty = { $name }: { $path }, nothing kept there yet
memory-kept = { $name }: { $path }, a file is kept there

## The rules and facts kept (RECALL-11)

memory-no-rules = no rules are kept
memory-rules-file =
    { $lines ->
        [one] rules: { $path }, 1 line
       *[other] rules: { $path }, { $lines } lines
    }
memory-rules-file-more = rules: { $path }, more than { $lines } lines
memory-verified-never = never
memory-no-facts = no facts are kept
memory-fact = fact { $key }: { $scope }, from { $source }, { $label }, verified { $verified }, { $standing }
memory-scope-project = this project
memory-scope-everywhere = every project
memory-source-person = the person
memory-source-agent = the agent
memory-label-public = public
memory-label-private = private
memory-standing-live = live
memory-standing-superseded = superseded by { $by }

remove-checkout-title = remove this checkout?
remove-checkout-which = checkout { $id }, made for delegate { $delegate }, is at
remove-checkout-explained =
    Something was done in it, and nothing brings that work back here. Removing it deletes the
    directory and whatever is in it.
remove-checkout-yes = remove it
remove-checkout-no = keep it

## Where a line sent while something runs is going, beside the mark under it

# At most 26 characters each: the last row also carries the key that sends everything now, and the
# two have to fit one 80-column row together. Longer, and these words are the ones left out.
#
# A prompt the running turn reads once its current round's calls are done.
queued-into-this-turn = into this turn, next round
# A prompt that nothing running will read, which starts a turn when what is running ends.
queued-its-own-turn = a new turn after this
# A prompt behind one of those, which the turn that one starts reads at its first round.
queued-into-the-next-turn = into the next turn
# A command: carried out by this program when what is running ends, rather than sent into it.
queued-carried-out = carried out after this
# A command line: run by the person's own shell, whose output then reaches the model.
queued-run = run in your shell
# A command that would have been carried out as typed, but for a line waiting ahead of it. $line is
# that line, from the two messages below where it is not a command. Same width limit as above.
queued-carried-out-after = after { $line }
# What a waiting prompt is called where a command is said to wait behind it.
queued-a-prompt = the prompt before it
# What a waiting command line is called where a command is said to wait behind it.
queued-a-shell-line = the shell line before it

## Pasting, dropping and attaching

paste-arrived-empty =
    that paste arrived empty: the terminal hands over text only, so a picture needs { $chord }
paste-not-a-command = a picture is not a command: leave shell mode to paste one
paste-not-with-a-command =
    a picture does not go with that command: send it in a prompt for it to be seen
paste-with-the-first-tick =
    that picture goes with the first tick of this loop; the ones after it say it was pasted
paste-too-large = that picture is { $size }, and a paste carries at most { $limit }
attachment-too-large = { $name } is { $size }, and an attachment carries at most { $limit }
paste-nothing-on-clipboard = there is nothing on the clipboard to paste
paste-files-refused =
    a copied file's name is not text or holds a line break, so none of the files were pasted
return-not-pressed =
    that return arrived with other keys, so it was not a press: press Enter to send this line, or Escape to clear it
leave-not-pressed =
    that arrived with other keys, so it was not a press: press it again to leave
suspend-not-pressed =
    that arrived with other keys, so it was not a press: press it again to suspend
suspend-failed = could not suspend the session: { $reason }
# Said under the box while the turn runs that the bare word exit or quit began, which other terminal
# agents leave on and which here is a prompt for the model. A turn is running, so the first way out
# it names is the one that stops it. Drawn at the right of the hint row, where the parts of the line
# are fitted to the room it leaves: kept short enough that any one permission mode name still fits
# beside it on a row eighty columns wide.
leave-word-sent = Sent as a prompt; ctrl-c stops it, /exit leaves
prompt-file-unreadable =
    /{ $name } is a prompt file that could not be read as text, so the line was not sent: { $path }
prompt-file-too-large =
    /{ $name } is a prompt file over 64 KiB, so the line was not sent: { $path }
prompt-file-empty =
    /{ $name } is a prompt file with nothing in it, so the line was not sent: { $path }
prompt-file-agent-not-a-name =
    /{ $name } is a prompt file whose agent is not a single word, so the line was not sent: { $path }
prompt-file-model-not-a-name =
    /{ $name } is a prompt file whose model is not a single word, so the line was not sent: { $path }
prompt-file-agent-and-model =
    /{ $name } is a prompt file that names both an agent and a model, and the box holds one form, so the line was not sent: { $path }
prompt-file-begins-with-a-command =
    /{ $name } is a prompt file that begins with a command, which a file may not start, so the line was not sent: { $path }
paste-folded = { $lines ->
    [one] [Pasted text #{ $number } +{ $lines } line]
   *[other] [Pasted text #{ $number } +{ $lines } lines]
    }
paste-invisible-removed = { $count ->
    [one] removed { $count } invisible character from that paste
   *[other] removed { $count } invisible characters from that paste
    }
cli-prompt-invisible-removed = { $count ->
    [one] removed { $count } invisible character from the prompt -i gave
   *[other] removed { $count } invisible characters from the prompt -i gave
    }
drop-name-not-written =
    a dropped or copied file's name holds a control character or one a terminal draws as nothing, so no path was written into the command
kilobytes = { $size } KB
megabytes = { $size } MB
gigabytes = { $size } GB


## Running a command the person typed

command-thread-stopped = the command's thread stopped unexpectedly
command-reported-a-failure = the command reported a failure


## Shortening a long conversation

compact-stopped = summarising was stopped; the conversation is as it was
compact-ended-unexpectedly = the summary ended unexpectedly
compact-done =
    summarised { $summarised } earlier messages, keeping the last { $kept } as they are
compact-nothing-to-do = there is nothing to summarise yet
compact-failed = the conversation could not be summarised: { $problem }
# Said by a turn that asked for a summary and did not get one, which carries on with the history it has.
compact-failed-and-continuing = the conversation could not be summarised: { $problem }; continuing with the existing context
turn-ended-unexpectedly = the turn ended unexpectedly


## Asking something beside the work

btw-needs-a-question = /btw takes the question to ask, which the conversation will not read
btw-stopped = the question was stopped
btw-nothing-sent-yet = /btw has nothing to read yet: the turn has not sent its first request
btw-no-files-beside-a-turn = /btw does not read dropped files while a turn runs; ask again when it has ended
btw-one-at-a-time = a question is already being answered beside the turn; ask again when it has
btw-ended-unexpectedly = the question ended unexpectedly
btw-failed = the question could not be answered: { $problem }

handoff-needs-a-goal = /handoff takes the next piece of work, which the brief is written for
handoff-nothing-to-hand-off = nothing to hand off yet: the session has no record until its first turn ends
handoff-ready = the brief is in the box. Edit it, then Enter starts a new session from it, or Esc drops it. The file decisions and permissions given here do not carry over
handoff-untrusted = the brief was not offered: this conversation has met something untrusted, so the new session would not be starting from what the planner could have held
handoff-failed = the brief could not be written: { $problem }
handoff-stopped = the brief was stopped
commit-message-usage = /commit-message takes --staged or --range <a>..<b>, then any words about the format you want
commit-message-nothing = there is nothing to draft a message for: the changes named show no file
commit-message-credential = the changes hold what looks like a credential, so no part of them was sent to a model
commit-message-none-written = the model did not mark where the message begins, so there is no draft
commit-message-ready = the draft is in the box. Edit it, then Enter sends it to the planner as your prompt, or Esc drops it. The planner has not seen the diff
commit-message-partial = the changes were longer than one read takes, so the draft describes the part that was read
commit-message-failed = the commit message could not be drafted: { $problem }
commit-message-stopped = the commit message was stopped
handoff-started = handed off: this is a new session, and the earlier one is as it was. To return to it, run `bravebot --resume { $id }`

enhance-ready = the enhanced line is in the box. Edit it, then Enter sends it, or Esc puts back what you wrote
enhance-ready-vi = the enhanced line is in the box. Edit it, then Enter sends it, or Ctrl-C puts back what you wrote
enhance-invisible-removed = { $count ->
    [one] removed { $count } invisible character from the enhanced line
   *[other] removed { $count } invisible characters from the enhanced line
    }
enhance-untrusted = the enhancement was not offered: this conversation has met something untrusted, so the rewrite would not be words the planner could have held
enhance-unusable = the enhancement was not offered: it came back empty or longer than { $limit } characters
enhance-line-changed = the enhancement was not offered: the line changed while it was being written
enhance-failed = the line could not be enhanced: { $problem }
enhance-stopped = the enhancement was stopped
enhance-not-a-command = { $key } rewrites a prompt, and a line that begins with / is a command or a skill, so it was left as written
enhance-not-with-a-picture = { $key } rewrites words, and this line holds a picture or a dropped file, so it was left as written
enhance-too-long = { $key } rewrites a line of up to { $limit } characters, and this one is longer, so it was left as written

# What the session says about a manifest run started from it. The plan, each step and the reply are
# shown as they happen, so what is left to say is that a run is starting, where it was written down,
# and what went wrong where something did. That a run is not a turn of the conversation is what the
# mode is for rather than news about this run, so it is not said here.
manifest-needs-a-task = /manifest takes the task to plan, as in /manifest summarise the docs
manifest-began = planning the whole task first; the session waits here until the run ends
manifest-ended-unexpectedly = the run ended unexpectedly
manifest-failed = the run stopped: { $problem }
manifest-recorded = recorded as { $id }; read it again with bravebot --resume { $id }

# What /init says when the project already has the file it would write.
init-already-there = { $file } already exists here, so /init leaves it alone

# What /review says when the target it was given names nothing it can use.
review-usage = Usage: /review [staged | since <ref> | commit <ref> | pr <number or URL>] [what to look at]

# What /help says when it is typed with no question.
help-usage = Usage: /help <question>, such as /help how do I stop a background job. Type / for the commands

# What /changelog puts above the notes: the version that is running, or the newest release where
# this build's own version has no notes, or nothing where the build carries none at all.
changelog-heading = What changed in { $version }
changelog-newest = No release notes are carried for { $running }. The newest are for { $version }
changelog-none = No release notes are carried in this build

# What the session says about a definition a person addressed with /agent. Every name here is one
# the session resolved from a source somebody vouched for, so it may be printed; it is never offered
# as a completion.
agent-needs-a-task = /agent { $name } takes the task to do, as in /agent { $name } review the diff
agent-resolved = this session resolved { $names }; address one with /agent <name> <task>
agent-no-such-definition = there is no definition called { $name }; this session resolved { $names }
# Drawn above a reply from an addressed turn. The name is the one the driver matched, never
# anything the reply says about itself.
agent-answered = { $name } answered
# Said when a session started with --agent opens, after the directory's trust is settled (CLI-17).
session-working-under =
    every turn is addressed to { $definition }; /agent <name> <task> addresses another for one turn
# Said after the line above where the agent setting, not --agent, chose the definition (ADDRESS-13).
session-working-under-by-setting =
    the agent setting chose this definition; --agent <name> chooses another for the session
# Said after the lines above for each turn of another session in this directory that was addressed
# to the definition for that turn alone, newest first (SESSION-39). The title is that session's, the
# time is how long ago the turn was asked, and the reply is the start of what the definition said.
session-addressed-elsewhere = { $definition } answered in "{ $title }" { $when }: { $reply }
# The line above for a turn whose record holds no reply.
session-addressed-elsewhere-unanswered = { $definition } was asked in "{ $title }" { $when } and did not answer
# Said after the lines above when more turns were addressed to the definition than they list.
session-addressed-elsewhere-more =
    { $count ->
        [one] and 1 more turn; /resume lists the sessions
       *[other] and { $count } more turns; /resume lists the sessions
    }
# Said when the agent setting names a definition this session did not resolve. The reason follows in
# the next line, and the session goes on without one.
session-agent-setting-gone =
    the agent setting names { $definition }, which this session did not resolve: every turn is the
    session's own, with the tools and model it would have without --agent
# Said when a resumed session was started under a definition that cannot be used now. The reason
# follows in the next line. The name is the one the driver recorded from the person's --agent.
session-recorded-definition-gone =
    this session was started under { $definition }, and the narrowing is gone: every turn from here
    is the session's own, with the tools and model it would have without --agent
# Said for /model in a session started with --agent under a definition that names a model.
session-model-is-the-definitions =
    every turn is addressed to { $definition }, which asks for { $model }, so /model has nothing to
    change; start bravebot without --agent to pick a model
# Said where --model and a definition that names a model are both in force. The definition's name
# and its model are the words of a vouched-for file.
agent-model-outranked =
    { $definition } asked for { $model }, and --model outranks it, so this run asked for the model
    the command line named
# Said where a project's output style was picked and the turn could not read it: the trust map no
# longer vouches for the file, or it is no longer a style. The name is what the person typed.
agent-style-not-used =
    the { $style } style was not used: its file is not trusted or is no longer a style, so this turn
    opened with the usual words
# Said where the skill the person put in force with /mode could not be applied to a turn: it is no
# longer found, or its file is no longer trusted. The turn goes on without it. The skill is the name
# the person typed.
agent-skill-mode-not-used =
    the { $skill } mode was not applied: its skill is not found or not trusted, so this turn ran
    without it
# Said where a turn is addressed to a definition whose isolation line asks for a checkout of its
# own. Only a delegate is given one, so the person's own turn works in their working directory. The
# definition's name is the word of a vouched-for file.
agent-checkout-not-applied =
    { $definition } asks for a checkout of its own, which only its delegates are given, so this turn
    works in your working directory


## The opening screen

# What this platform can enforce over a process that runs code we did not write, not something the
# session is running inside: the agent's own work and the programs a person asks for are outside any
# such boundary. /status carries the second half of that, which there is no room for here.
opening-confinement = confinement available: { $level }
opening-network-closed = network closed
opening-filesystem-rules = filesystem rules in force
opening-path-sockets-reachable = programs reach sockets with a path: D-Bus, SSH agent, docker.sock
opening-invitation = Ask a question about this workspace.


## What a turn did, in the words a transcript line begins with

# One per tool. A word rather than the tool's own name, because a person reads the line:
# "Read(src/main.rs)" says what happened and "read_file" says what was typed.
verb-read-file = Read
verb-list-files = List
verb-search = Search
# The history of a repository, read without starting git.
verb-read-git = History
# A map of the declarations in a directory's source files.
verb-repo-map = Map
# A question put to a language server rather than to the files: "Look up" reads as asking
# something that knows the code, where "Search" reads as looking through it.
verb-lsp = Look up
verb-write-file = Write
verb-edit-file = Update
verb-apply-checkout = Apply
verb-todo-write = Plan
# Named for what it is rather than for what it does: every one of these is a model with no
# tools, no memory and one round, and a person watching a line go by should not have to
# remember which of the verbs meant that.
verb-spawn-processor = Isolated processor
verb-load-skill = Skill
verb-search-skills = Find skill
verb-load-tool = Load
verb-ask-user = Ask
verb-request-path = Reach
verb-run = Run
verb-read-output = Read output
verb-vet-content = Vet
verb-fetch-url = Fetch
verb-download-url = Download
verb-job-output = Job
verb-spawn-agent = Delegate
verb-schedule-next = Schedule
verb-watch-file = Watch
verb-advisor = Advise
verb-propose-plan = Plan
verb-message-session = Message
verb-remember = Remember
verb-propose-rule = Propose
verb-recall = Recall
verb-mcp-call = MCP
verb-unknown = Tool

## Where what a call produced ended up, said at the end of the line about it
#
# Names which context, because there is more than one kind of model here and the driver is not
# one of them: the planner is the model holding the conversation, and a processor is an isolated
# model that is handed slots and nothing else. "The model" answers neither question.
landed-in-the-planner = read into the planner's context
landed-quarantined = not in the planner's context; only an isolated processor can be sent to read it
landed-reserved = read by nothing: only its name is known
reach-no-model = in no model's context: nothing can be sent to read this

# How many calls a delegate has made, where its block shows only the last few.
delegate-more-calls = { $count } calls so far
# The definition's name and the model as its file wrote it, both from a vouched-for file.
delegate-model-needs-sign-in =
    { $definition } asked for { $model }, which needs a sign-in first, so it did not run
# The endpoint substitutes a model it will not serve rather than refusing. The name it answered
# with is left out, because a notice is the driver's own words.
delegate-model-substituted = { $definition } asked for { $model } and was answered by a different model
# The model `planModel` named, which a turn begun in plan mode could not use. Said, and the turn
# runs on the session's own model, since the setting asks for a model and not for a refusal.
plan-model-unavailable =
    { $model } is set as the plan model but cannot be used here (no configured service answers for it, it needs a sign-in, or this machine's managed settings do not allow it), so this turn runs on the session's model
# The model `summaryModel` named, from the person's own settings file. A compaction or a goal check
# is refused rather than sent to the session's model, since the setting is a cost boundary.
summary-model-needs-sign-in =
    { $model } is set as the summary model and needs a sign-in first, so nothing was sent
# The same setting naming a model this machine's managed layer excludes (BACKEND-48). Refused rather
# than sent, and rather than fallen back from, for the same reason.
summary-model-refused =
    { $model } is set as the summary model and this machine's managed settings do not allow it, so nothing was sent
# The same setting naming a model no configured service answers for. Refused rather than sent, since
# a judge or a summariser answering without the exchange reads as a verdict rather than a failure.
summary-model-not-served =
    { $model } is set as the summary model and no configured service answers for it, so nothing was sent
# The model `processorModel` or `--processor-model` named, both the person's own. A processor call is
# refused rather than sent to the turn's model, since the name is a cost boundary.
processor-model-needs-sign-in =
    { $model } is set as the processor model and needs a sign-in first, so nothing was sent
# The same name for a model this machine's managed layer excludes (BACKEND-48).
processor-model-refused =
    { $model } is set as the processor model and this machine's managed settings do not allow it, so nothing was sent
# The same name for a model no configured service answers for.
processor-model-not-served =
    { $model } is set as the processor model and no configured service answers for it, so nothing was sent
# The endpoint substitutes a model it will not serve rather than refusing. The name it answered
# with is left out, because a notice is the driver's own words. The model is the one the person named.
processor-model-substituted = A processor asked for { $model } and was answered by a different model
# A definition's skills line named skills this session did not find. The definition is its file's
# path and the skills are that file's own words, joined with a comma, both from a vouched-for file.
delegate-skills-not-found =
    { $count ->
        [one] { $definition } names a skill this session did not find, so its delegate is offered without it: { $skills }
       *[other] { $definition } names skills this session did not find, so its delegate is offered without them: { $skills }
    }
# A definition's skills line named skills whose files keep them from the planner, so no delegate is
# offered them. The definition is its file's path and the skills are that file's own words, joined
# with a comma, both from a vouched-for file.
delegate-skills-kept-from-the-planner =
    { $count ->
        [one] { $definition } names a skill its file keeps from the planner, so its delegate is offered without it: { $skills }
       *[other] { $definition } names skills their files keep from the planner, so its delegate is offered without them: { $skills }
    }
# A definition's mcpServers line named servers this session did not reach. The definition is its
# file's path and the servers are that file's own words, joined with a comma, both from a
# vouched-for file. "MCP" is a protocol's name and stays as it is.
delegate-servers-not-found =
    { $count ->
        [one] { $definition } names an MCP server this session did not reach, so its delegate runs without it: { $servers }
       *[other] { $definition } names MCP servers this session did not reach, so its delegate runs without them: { $servers }
    }
# A definition's spawns line named delegates this session did not resolve. The definition is its
# file's path and the names are that file's own words, joined with a comma, both from a vouched-for
# file.
delegate-spawns-not-found =
    { $count ->
        [one] { $definition } names a delegate this session did not resolve, so its delegate may not start it: { $names }
       *[other] { $definition } names delegates this session did not resolve, so its delegate may not start them: { $names }
    }
# A definition's mcpServers line declared a server inline rather than naming one, so its delegate
# calls no MCP server. The definition is its file's path. Nothing from the line is shown, since an
# inline entry can hold a command line and the value of a secret. "MCP", "mcpServers" and the path
# stay as they are.
delegate-servers-declared = { $definition } declares an MCP server in its mcpServers line, which only ~/.bravebot/mcp.json may do, so its delegate calls no MCP server
# A definition's rounds line is not a whole number above zero, so the file did not load. The
# definition is its file's path.
delegate-rounds-not-a-count = { $definition } was skipped: its rounds must be a whole number above zero
# A definition's budget line is not a count of tokens above zero, so the file did not load. The
# definition is its file's path. "budget" is the key and stays as it is.
delegate-budget-not-a-count = { $definition } was skipped: its budget must be a count of tokens above zero, such as 500k or 2m
# A definition asked for more rounds than its kind may make. The kind is its key's value (reader,
# checker or worker), left as written because it is typed.
delegate-rounds-held = { $definition } asks for { $asked } rounds, more than the { $most } a { $kind } may make, so its delegate is given { $most }
# A definition's memory line named a value other than project or local, so the definition loads
# keeping no memory. The definition is its file's path and the value is that file's own words, both
# from a vouched-for file. "memory", "project" and "local" are the key and its values, and stay as
# they are.
delegate-memory-not-kept = { $definition } keeps no memory: its memory line says { $value }, and only project and local keep one
# A definition asked to keep a memory and its name is not one its memory file can be named after.
# The definition is its file's path.
delegate-memory-not-a-slug = { $definition } keeps no memory: a definition keeping one needs a name of lowercase letters and digits in runs joined by single hyphens, 64 characters at most
# A definition asked to keep a memory in a working directory where the memory would fall inside the
# person's own ~/.bravebot, as it does for a session in the home directory. The definition is its
# file's path, and ~/.bravebot stays as it is.
delegate-memory-in-home = { $definition } keeps no memory here: in this directory its memory would be inside ~/.bravebot, which no write can leave untrusted
# A definition's isolation line named a value other than checkout or worktree, so the definition
# loads and its delegate works in the working directory. The definition is its file's path and the
# value is that file's own words, both from a vouched-for file. "isolation", "checkout" and
# "worktree" are the key and its values, and stay as they are.
delegate-isolation-not-read = { $definition } is loaded without a checkout: its isolation line says { $value }, and only checkout and worktree ask for one
# A definition's effort line names none of the five levels, so the definition loads and its
# delegate asks for the level the spawning turn runs at. The definition is its file's path and the
# effort is that file's own words, both from a vouched-for file. The levels are this program's own
# names for them, joined with a comma, and are not translated: they are what a file has to write to
# be understood.
delegate-effort-not-a-level = { $definition } asks for effort { $effort }, which is none of { $levels }, so its delegate keeps the effort of the turn that spawns it
# A definition's writes line holds patterns that cannot be read: one needing the home or the
# settings directory, which a definition has neither of, or one that is not a path pattern. They
# cover no file, so the delegate may write only what the others cover, and nothing at all where none
# was readable. The definition is its file's path and the patterns are that file's own words, both
# from a vouched-for file. "writes" is the key and stays as it is.
delegate-writes-not-read =
    { $count ->
        [one] { $definition } has a writes pattern that cannot be read here, so it covers no file: { $patterns }. Its delegate may write only what the other patterns cover, and no file where there are none
       *[other] { $definition } has writes patterns that cannot be read here, so they cover no file: { $patterns }. Its delegate may write only what the other patterns cover, and no file where there are none
    }
# A definition asks for a checkout and is loaded as a reader, either as its own kind line says or
# because a definition of the same name narrowed it. A reader is never given a checkout. The
# definition is its file's path.
delegate-checkout-reader = { $definition } is loaded without a checkout: it is a reader, and a reader is never given one
# A definition keeps a memory and asks for a checkout. Each of its delegates works in a checkout,
# which keeps no memory, so only a turn addressed to it keeps one. The definition is its file's
# path, and /agent is the command, which stays as it is.
delegate-memory-in-checkout = { $definition } keeps its memory only in a turn you run with /agent: each of its delegates works in a checkout, which keeps none

# What a skill file named beyond its name and description. The skill is its file's path and the
# model and the effort are that file's own words, all three from a source somebody vouched for.
# An effort word naming none of the five levels. The levels are this program's own names for them,
# joined with a comma, and are not translated: they are what a file has to write to be understood.
skill-effort-not-a-level = { $skill } asks for effort { $effort }, which is none of { $levels }, so its rounds keep this session's
# A skill's disable-model-invocation holds a word that is neither true nor false. It is read as true,
# so the planner is not offered the skill and the person can still name it. The skill is its file's
# path and the value is that file's own word, both from a source somebody vouched for.
skill-person-only-not-a-boolean = { $skill } sets disable-model-invocation to { $value }, which is neither true nor false, so the planner is not offered it
# A skill only the person may have loaded was named as /name in the line they typed, so the driver
# put its instructions in front of the planner. Said because nothing else on the screen shows a skill
# was loaded: no tool call is drawn for it. The skill is its file's name, from a vouched-for source.
skill-named-by-the-person = { $skill } is loaded because you named it
# A skill the person named could not be shown to the planner, which a skill passing the trusted-content
# gate cannot be. The turn stops rather than goes on without a skill the person asked for. The skill
# is its file's name, from a vouched-for source.
turn-named-skill-not-shown = { $skill } was named but could not be shown to the planner
# A skill was loaded and asks the rest of the turn of a model of its own, over whatever the session
# was running. Said because a switch nobody is told about is the person's money spent on a choice
# they did not make.
skill-asks-a-model = { $skill } asks the rest of this turn of { $model }
skill-asks-an-effort = { $skill } asks the rest of this turn at { $effort } effort
# The same skill on a ChatGPT model, whose account is sent the level the session began with for all
# of it, so the turn goes on as it was. The skill and the level are its file's own words.
skill-effort-kept-for-account = { $skill } asks the rest of this turn at { $effort } effort, but a ChatGPT session keeps the level it began with, so this turn does not change
# The skill loads and the turn goes on as it was, rather than stopping: a skill is not the thing the
# person asked for, so a model they cannot reach is a line of its file that does nothing.
skill-model-needs-sign-in = { $skill } asks for { $model }, which needs a sign-in first, so its rounds keep this session's model
# The layer refuses the model, so the skill loads and the turn goes on as it was, for the reason the
# sign-in line above does.
skill-model-refused = { $skill } asks for { $model }, which this machine does not request, so its rounds keep this session's model: { $reason }
# The turn runs on a model an addressed or delegate definition named, which a skill does not
# replace. The definition is its name as the person or the planner wrote it.
skill-model-kept-for-definition = { $skill } asks for { $model }, but this turn stays on the model { $definition } named
# The endpoint answered the rounds after a skill's switch with another model, which it does rather
# than refuse a name it will not serve. The model is the skill file's own word for it.
skill-model-substituted = { $skill } asked for { $model } and was answered by a different model
# A project file this program would have read on its own account (AGENTS.md, CLAUDE.md,
# .claude/CLAUDE.md, the file one of those points at, a skill or a definition) that the person's own
# settings deny reading, so it was left out of the turn. The source is its workspace-relative path
# and stays as it is. "deny" is the name of the settings list the rule sits in.
source-denied-by-rule = { $source } was not loaded: a deny rule in your settings covers it
# Skills in the person's own ~/.bravebot/skills that were not loaded, because the way to the file goes
# through the working directory or a directory opened beside it, where a turn can write, and no
# skill directory of the project offered them in their place. ~/.bravebot/skills stays as it is.
home-skills-reached-through-the-workspace =
    { $count ->
        [one] 1 skill in ~/.bravebot/skills was not loaded: the way to its file goes through a directory this session can write to
       *[other] { $count } skills in ~/.bravebot/skills were not loaded: the way to their files goes through a directory this session can write to
    }
# Skills in a directory of another agent's that the person named in sharedSkillDirectories and that
# were not loaded, for the same reason.
shared-skills-reached-through-the-workspace =
    { $count ->
        [one] 1 skill in { $directory } was not loaded: the way to its file goes through a directory this session can write to
       *[other] { $count } skills in { $directory } were not loaded: the way to their files goes through a directory this session can write to
    }
# Shown by doctor for a directory of another agent's skills that exists and that the person did not
# name in sharedSkillDirectories, so nothing in it is read.
doctor-skills-shared-not-read = { $directory } exists and is not read: name it in sharedSkillDirectories in ~/.bravebot/settings.json to load the skills in it
# The person's own ~/.bravebot/AGENTS.md when it was not read, because the way to the file goes
# through the working directory or a directory opened beside it, where a turn can write.
home-agents-file-reached-through-the-workspace = ~/.bravebot/AGENTS.md was not read: the way to it goes through a directory this session can write to
# The person's rules for this project when they were not read, for the same reason.
project-rules-reached-through-the-workspace = The rules kept in ~/.bravebot for this project were not read: the way to them goes through a directory this session can write to
# The person's rules for this project when the file exists and could not be read.
project-rules-unreadable = The rules kept for this project at { $path } could not be read: { $reason }
# The reason given when the trust gate, not the disk, kept the rules out.
project-rules-policy-refused = the trust policy refused it
project-rules-dangling-link = it is a link that leads to nothing
# Definitions in the person's own ~/.bravebot/agents that were not loaded, for the same reason.
home-definitions-reached-through-the-workspace =
    { $count ->
        [one] 1 delegate definition in ~/.bravebot/agents was not loaded: the way to its file goes through a directory this session can write to
       *[other] { $count } delegate definitions in ~/.bravebot/agents were not loaded: the way to their files goes through a directory this session can write to
    }
# An entry of "references" in the person's settings that was not offered. The alias is the name they
# gave it. One message per reason: the alias cannot be used, the entry names a repository, which
# is not fetched, the entry names no directory, and a directory that could not be opened.
reference-bad-alias = reference { $alias } was not used: an alias cannot be empty or contain /, spaces, backticks or commas
reference-repository-not-fetched = reference { $alias } was not used: an entry that names a repository is not fetched, so give it a path
reference-no-path = reference { $alias } was not used: it names no directory
reference-not-opened = reference { $alias } was not opened: { $problem }
# An `@path` line in AGENTS.md that was left as written. The import is the workspace-relative path
# and stays as it is. One message per reason: nesting or count past the limit, the file already
# being expanded further up, and a file that is outside the project, cannot be read or is untrusted.
import-too-deep = { $import } was not imported: imports are nested too deeply or there are too many
import-cycle = { $import } was not imported: a file above it already imports it
import-not-loaded = { $import } was not imported: it is outside this project, cannot be read or is not trusted
# Said about a standing instructions file long enough to be worth a word, which goes out with every
# request of the session. The source is its workspace-relative path, or ~/.bravebot/AGENTS.md, and
# stays as it is; the sizes are numbers of bytes. The file still loaded.
source-is-large = { $source } is { $bytes } bytes, and every request this session sends carries it
# Said once for the turn, about every instructions file together rather than any one of them.
sources-large-together = standing instructions are { $bytes } bytes in all, past { $limit }, and every request this session sends carries them
# A rule file, kept in .bravebot/rules or ~/.bravebot/rules, that was added to the instructions
# because a file the session used matches one of its paths. The rule is its place and stays as it
# is; the file is the name the planner or the person typed, drawn so that no character in it is
# hidden.
rule-loaded = { $rule } was added to the instructions: { $file } matches its paths
# The project's rule files that were not read, because the directory is not trusted. A count and
# no name: a name in a directory nobody vouched for is not shown.
rules-directory-not-trusted =
    { $count ->
        [one] 1 rule file in .bravebot/rules was not loaded: this directory is not trusted
       *[other] { $count } rule files in .bravebot/rules were not loaded: this directory is not trusted
    }
# Files in a rules directory (.bravebot/rules or ~/.bravebot/rules) that are not rules, because their
# names are not text and so cannot be compared or shown. The directory stays as it is.
rule-names-not-text =
    { $count ->
        [one] 1 file in { $directory } was not read as a rule: its name is not text
       *[other] { $count } files in { $directory } were not read as rules: their names are not text
    }
# A rule file that opens a frontmatter block and never closes it, or a paths list with a [ and no
# closing ], or a paths value that is not a list of globs. Left out rather than read as text for
# every turn, since the file meant to limit itself to some paths and they cannot be read.
rule-frontmatter-unreadable = { $rule } was not loaded: its frontmatter or its paths could not be read, so a --- or ] may be missing, or paths is not a list
# Rule files in the person's own ~/.bravebot/rules that were not loaded, because the way to the file
# goes through the working directory or a directory opened beside it, where a turn can write.
home-rules-reached-through-the-workspace =
    { $count ->
        [one] 1 rule file in ~/.bravebot/rules was not loaded: the way to it goes through a directory this session can write to
       *[other] { $count } rule files in ~/.bravebot/rules were not loaded: the way to them goes through a directory this session can write to
    }

# Advisory checks shown only in a Bravebot source checkout.
doctor-development = development environment { $path }
doctor-agents-ok = OK (link to agents/AGENTS.md)
doctor-agents-copy-ok = OK (Windows copy of agents/AGENTS.md)
doctor-agents-missing = missing; run `python3 agents/setup.py link` from the checkout root
doctor-agents-broken = broken or unreadable link; run `python3 agents/setup.py link` from the checkout root
doctor-agents-wrong = link points to the wrong target; run `python3 agents/setup.py link` from the checkout root
doctor-agents-copy-stale = stale or unreadable Windows copy; run `python3 agents/setup.py link` from the checkout root
doctor-agents-conflict = conflict: resolve the existing file or directory first, then run `python3 agents/setup.py link` from the checkout root
doctor-agents-unreadable = cannot inspect this path; resolve its access permissions first
doctor-direnv-ok = available on PATH
doctor-direnv-missing = not found on PATH; see https://direnv.net/ or run `brew install direnv`

status-undecided = not decided

sessions-usage = sessions takes --json, stop, delete and a session's id, purge, list, import and a tool's name, or search and some text
sessions-none = No background sessions.
sessions-no-home = There is no state directory to find background sessions in.
sessions-missing = No background session { $id }.
sessions-ambiguous = More than one background session begins with { $id }.
sessions-stopped = Stopped { $name }.
sessions-not-running = { $name } was not running.
sessions-stop-failed = Could not record the stop: { $problem }
sessions-import-usage = sessions import takes claude-code or opencode, then --project <directory> if it is not this one, then --all or the first characters of each session to copy
sessions-import-opencode = opencode keeps its sessions in a database this build does not read, so nothing was copied. claude-code sessions can be.
sessions-import-no-project = { $path } is not a directory.
sessions-import-no-source = There is no profile directory to find Claude Code's sessions under.
sessions-import-none = Claude Code kept no sessions to copy for { $directory }.
sessions-import-row = { $id }  { $when }  { $title }
sessions-import-row-there = { $id }  { $when }  { $title }  (already copied)
sessions-import-how = Name the ones to copy by their first characters, or pass --all.
sessions-import-copied = Copied "{ $title }" as { $id }. Resume it with: bravebot --resume { $id }
sessions-import-there = "{ $title }" is already here, and was left as it was.
sessions-import-missing = No Claude Code session here begins with { $id }.
sessions-import-ambiguous = More than one Claude Code session here begins with { $id }.
sessions-import-failed = Could not write "{ $title }": { $problem }
sessions-search-usage = sessions search takes some text to find, and may add since:<n>h, since:<n>d or since:<n>w, and workspace:<directory> if it is not this one
sessions-search-none = No session matches.
sessions-list-usage = sessions list takes --json, and --limit and a count
sessions-list-none = No session is recorded for this directory.
sessions-list-row = { $id }  { $when }  { $title }  { $branch }  { $front }  { $state }
sessions-list-continuable = can be continued
sessions-list-not-continuable = cannot be continued
sessions-list-unknown = unknown
sessions-list-more = { $held } more. Pass --limit to see them.
sessions-delete-usage = sessions delete takes the id of one session, and sessions purge takes --dry-run and -y
sessions-delete-invalid = { $id } is not shaped like a session id.
sessions-delete-missing = No session { $id } is recorded for this directory.
sessions-delete-held = Session { $id } is running in the background. Stop it first with: bravebot sessions stop { $id }
sessions-delete-failed = Could not delete session { $id }.
sessions-delete-done = Deleted session { $id }.
sessions-purge-none = No session is recorded for this directory.
sessions-purge-row = would delete { $id }  { $when }  { $title }
sessions-purge-held = left { $id }, which is running in the background
sessions-purge-question = Delete the recorded sessions for { $directory }, { $count } in all, with their trails and search indexes?
sessions-purge-needs-yes = sessions purge asks before it deletes, and there is no terminal to ask on. Pass -y to delete without asking, or --dry-run to see what it would delete.
sessions-purge-declined = Nothing was deleted.
sessions-purge-done = Deleted: { $count }.
# What `bravebot stats` says. The figures are numbers the program wrote down, so the only words
# here are the headings and the sentence saying what the figures cover.
stats-usage = stats takes --days and a number of days, --models, and --project with a directory if it is not this one
stats-scope-everything = Every recorded session, in every project
stats-scope-days = { $days ->
    [one] Sessions last written in the last day, in every project
   *[other] Sessions last written in the last { $days } days, in every project
    }
stats-scope-project = Every recorded session in { $project }
stats-scope-days-project = { $days ->
    [one] Sessions last written in the last day, in { $project }
   *[other] Sessions last written in the last { $days } days, in { $project }
    }
stats-column-model = Model
stats-column-sessions = Sessions
stats-column-turns = Turns
stats-column-tokens = Tokens
stats-column-read = Read
stats-column-written = Written
# Tokens a record holds that its split into read and written does not cover: every token of a
# session written before the split was kept.
stats-column-unsplit = Unsplit
stats-row-total = Total
# A session whose record names no model: one written before the model was kept, one whose turns
# never reached a server, and one copied in from another program.
stats-model-unknown = unknown
stats-incognito = An incognito session leaves no record, so none is counted.
stats-unreadable = { $count ->
    [one] 1 entry in the sessions directory could not be read as a record and is not counted.
   *[other] { $count } entries in the sessions directory could not be read as records and are not counted.
    }
sessions-state-working = working
sessions-state-idle = idle
sessions-state-stopped = stopped
sessions-state-interrupted = interrupted
sessions-state-needs-input = needs input ({ $kind })
sessions-state-needs-input-unnamed = needs input
sessions-held-write = write
sessions-held-run = run
sessions-held-read = read
sessions-held-fetch = fetch
sessions-held-server = server
sessions-held-vouch = vouch
sessions-held-tools = tools
sessions-held-move = move
sessions-held-manifest = manifest
sessions-held-question = question
sessions-held-message = message
sessions-view-title = Background sessions
sessions-view-keys = ↑↓ choose  ·  Enter attach  ·  Esc leave
sessions-view-failed = Could not draw the list of background sessions: { $problem }
# The Sessions screen: every session on the machine in one list (SCREEN-1).
sessions-screen-title = Sessions
sessions-screen-tab-all = All
sessions-screen-tab-needs-you = Needs you
sessions-screen-tab-working = Working
sessions-screen-tab-ready = Ready
sessions-screen-tab-inactive = Inactive
sessions-screen-filter-keys = Tab/Shift-Tab
sessions-screen-filter-word = filter
sessions-screen-column-tasks = Session
sessions-screen-column-project = Project
sessions-screen-column-branch = Branch
sessions-screen-column-size = Size
sessions-screen-column-status = Status
sessions-screen-column-updated = Updated
sessions-screen-status-needs-input = Needs input
sessions-screen-status-interrupted = Interrupted
sessions-screen-status-working = Working
sessions-screen-status-ready = Ready
sessions-screen-status-open-elsewhere = Open elsewhere
sessions-screen-status-stopped = Stopped
sessions-screen-status-saved = Saved
sessions-screen-group-pinned = Pinned
sessions-screen-here-heading = Here
sessions-screen-group-by-project = { $key }  group by project
sessions-screen-group-by-agent = { $key }  group by agent
sessions-screen-group-by-model = { $key }  group by model
sessions-screen-group-together = { $key }  list together
sessions-screen-agent-none = No agent
sessions-screen-summary-waiting = { $count } waiting on you
sessions-screen-summary-working = { $count } working
sessions-screen-summary-calm = Nothing is waiting on you
sessions-screen-group-count-of = { $shown } of { $all }
sessions-screen-model-unknown = Unknown
sessions-screen-here = here
sessions-screen-for-pull-request = for pull request { $pull_request }
sessions-screen-manifest = manifest
sessions-screen-empty-loading = Loading sessions…
sessions-screen-empty-none = No sessions yet. Start one with bravebot in a project.
sessions-screen-empty-no-match = No matching sessions
sessions-screen-show-more = Show more
sessions-screen-search-label = Search ›
sessions-screen-search-placeholder = sessions, and what was said in them (since:7d for recent)
sessions-screen-rename-label = Rename ›
sessions-screen-details-title = Session details
sessions-screen-details-attention = Needs attention
sessions-screen-details-waiting = Waiting for your answer.
sessions-screen-details-waiting-for = Waiting for your answer to a { $kind }.
sessions-screen-details-open-to-answer = Join the session to answer.
sessions-screen-details-interrupted = The process ended in the middle of a turn.
sessions-screen-details-elsewhere = Open in another terminal.
sessions-screen-details-here = This is the session you are in.
sessions-screen-details-manifest = A manifest run, which can be read but not continued.
sessions-screen-details-model = Model: { $model }
sessions-screen-details-mode = Mode: { $mode }
sessions-screen-details-turns = Turns: { $turns }
sessions-screen-details-tokens-split = Tokens: { $read } in · { $written } out
sessions-screen-details-tokens-total = Tokens: { $total }
sessions-screen-details-project = Project
sessions-screen-details-branch = Branch
sessions-screen-details-prompt = Prompt
sessions-screen-details-agent = Agent
sessions-screen-details-issue = Issue
sessions-screen-details-pull-request = Pull request
sessions-screen-details-handed-off = Handed off from
sessions-screen-details-size = Size
sessions-screen-details-id = Session id
sessions-screen-keys-no-such-action = sessionsKeys: { $name } is not a key of the Sessions screen
sessions-screen-keys-unreadable = sessionsKeys.{ $name }: "{ $spelling }" is not a key the screen can read, so { $name } stays on { $key }
sessions-screen-keys-kept = sessionsKeys.{ $name }: "{ $spelling }" is a key the screen keeps, so { $name } stays on { $key }
sessions-screen-keys-contested = sessionsKeys.{ $name }: "{ $spelling }" is wanted by another action, so { $name } stays on { $key }
sessions-screen-keys-more = and { $count } more keys refused
sessions-screen-hint-help = help
sessions-screen-hint-back = back
sessions-screen-hint-move = move
sessions-screen-hint-open = open
sessions-screen-hint-open-first = open first match
sessions-screen-hint-new = new
sessions-screen-hint-rename = rename
sessions-screen-hint-confirm = confirm
sessions-screen-hint-cancel = cancel
sessions-screen-help-title = Session shortcuts
sessions-screen-help-navigate = Navigate
sessions-screen-help-tasks = Sessions
sessions-screen-help-view = View
sessions-screen-help-up = Up
sessions-screen-help-down = Down
sessions-screen-help-open = Open
sessions-screen-help-page-up = Page up
sessions-screen-help-page-down = Page down
sessions-screen-help-quit = Quit
sessions-screen-help-new = New
sessions-screen-help-fork = Fork
sessions-screen-help-rename = Rename
sessions-screen-help-stop = Stop
sessions-screen-help-archive = Archive
sessions-screen-help-hide = Hide
sessions-screen-help-delete = Delete
sessions-screen-help-pin = Pin/unpin
sessions-screen-help-search = Search
sessions-screen-help-group = Group the list
sessions-screen-help-filter = Filter
sessions-screen-help-tabs = Go to a tab
sessions-screen-help-help = Help
sessions-screen-help-resize = … resize to see all
sessions-screen-archive-title = Archive “{ $name }”?
sessions-screen-archive-body = This ends the process running it, if there is one, and takes it off this list until it is used again. Its record is kept.
sessions-screen-delete-title = Permanently delete “{ $name }”?
sessions-screen-delete-body = This ends the process running it, if there is one, and deletes its record. This cannot be undone.
sessions-screen-note-no-state = There is no state directory to find sessions in.
sessions-screen-note-stopped = Stopped “{ $name }”.
sessions-screen-note-not-running = Nothing is running “{ $name }”.
sessions-screen-note-stop-failed = Could not stop “{ $name }”.
sessions-screen-note-renamed = Renamed to “{ $name }”.
sessions-screen-note-rename-held = “{ $name }” is open in a process, so it is renamed from there.
sessions-screen-note-rename-failed = Could not rename “{ $name }”.
sessions-screen-note-gone = “{ $name }” is gone.
sessions-screen-note-current = This is the session you are in. Leave it first.
sessions-screen-note-open-elsewhere = That session is open in another terminal.
sessions-screen-note-archived = Archived “{ $name }”. It comes back when it is used again.
sessions-screen-note-deleted = Deleted “{ $name }”.
sessions-screen-note-delete-failed = Could not delete “{ $name }”.
sessions-screen-note-fork-failed = Could not copy “{ $name }”.
sessions-screen-interrupt-open-title = Stop this turn and open “{ $name }”?
sessions-screen-interrupt-copy-title = Stop this turn and copy “{ $name }”?
sessions-screen-interrupt-new-title = Stop this turn and begin a session where “{ $name }” ran?
sessions-screen-interrupt-body = The turn that is running is stopped, as Escape stops it, and this session is saved as it stands.
sessions-screen-note-directory-gone = { $directory } is no longer there.
sessions-screen-note-wait-for-turn = There is no session to ask about: begin one when the turn has ended.
sessions-screen-note-busy-request = A request is running: opening, copying or beginning a session ends this one, so do it when the request has ended.
sessions-screen-note-busy-plan = A plan is running: opening, copying or beginning a session ends this one, so do it when the plan has ended.
sessions-screen-note-busy-command = A command is running: opening, copying or beginning a session ends this one, so do it when the command has ended.
sessions-screen-open-not-started = Could not find the program to start the session with.
sessions-screen-open-not-a-session = That is not the name of a session.
sessions-screen-open-failed = Could not start the session: { $problem }
bg-needs-a-prompt = --bg takes the prompt to start with
bg-needs-a-terminal = --bg starts a session from a terminal, and this is not one
bg-takes-nothing-else = --bg takes a prompt and no other option, and { $flag } cannot reach the session it starts
bg-bypass-refused = --dangerously-skip-permissions is refused for a background session, because a session nobody is watching does not run in a mode that answers every question. Use a one-shot run (-p) to ask for that.
# Said to whoever is attached, the mode being fixed for as long as the process runs (BG-8).
bg-mode = Mode: { $mode }. It stays this for as long as the session runs; attaching cannot change it.
bg-not-started = The background session did not start.
bg-started = Started { $id }. Join it with: bravebot attach { $id }
bg-spawn-failed = Could not start the background session: { $problem }
bg-unsupported = Background sessions are not available on this platform.
bg-handed-over = Handed this session to a background process. Join it with: bravebot attach { $id }
attach-usage = attach takes a session's id
attach-needs-a-terminal = attach answers a session's prompts from the lines typed, and this is not a terminal
reply-usage = reply takes a session's id and the prompt to send
attach-not-running = { $name } is not running.
attach-unreachable = Could not reach { $name }: { $problem }
attach-taken = A terminal is attached to { $name } already.
attach-joined = Attached to { $name }. /detach or Ctrl-C leaves it running.
attach-line-not-sent = Not sent: the session is not waiting for a line.
attach-left = The session ended.
attach-detached = Detached from { $name }. It keeps running.
attach-stopping = { $name } is stopping after an hour idle. Attach again to start it.
reply-sent = Sent to { $name }.
reply-working = { $name } is working and takes no prompt now. Reply when it is idle.
reply-needs-input = { $name } is waiting on a question. Answer it with: bravebot attach { $id }
reply-not-sent = { $name } did not take the prompt.
reply-stopping = { $name } is stopping after an hour idle. Reply again to start it.
bg-restart-needs-a-terminal = { $name } is stopped, and only a terminal can start it again.
bg-interrupted-needs-a-terminal = { $name } was interrupted, and only a terminal can start it again.
bg-interrupted-not-repeated = { $name } was interrupted in the middle of a turn. Starting it again does not repeat that turn.
resume-held-by-background = { $name } is held by a running background session. Join it with: bravebot attach { $id }
resume-held-by-another-process = { $id } is open in another terminal or window, so it cannot be resumed here. Copy it with: bravebot --fork { $id }
resume-changed-while-opening = { $id } was written by another process while it was being opened. Resume it again.

# `bravebot doctor --sandbox-check`: git, gh, make, cargo and the other everyday programs, run the way a
# session runs them. A row is a workflow; a row that failed says where, what to do, and where the
# program's output went. The output itself is never printed.
cli-doctor-sandbox-takes-nothing-else = doctor --sandbox-check takes no other arguments.
doctor-sandbox-not-here = doctor --sandbox-check runs programs under the sandbox on Linux and macOS. This platform confines programs another way, and the workflows are not run here.
doctor-sandbox-cannot-confine = This machine cannot confine a program, so there is nothing to check. bravebot doctor reports the confinement it can apply.
doctor-sandbox-no-place = There is no state directory to run the workflows in. bravebot doctor says which variables name one.
doctor-sandbox-temporary = The state directory { $path } is under the temporary directory, which the sandbox lets every program write, so a refused write could not be told from a permitted one. Move the state directory.
doctor-sandbox-io = Could not prepare { $path }: { $detail }
doctor-sandbox-passed = passed
doctor-sandbox-failed = FAILED
doctor-sandbox-skipped = skipped
doctor-sandbox-because = because
doctor-sandbox-not-installed = { $programs } is not installed
doctor-sandbox-at = at
doctor-sandbox-exited = stage { $stage }, { $program }, exited with { $code }
doctor-sandbox-did-not-exit = stage { $stage }, { $program }, did not exit
doctor-sandbox-fix = fix
doctor-sandbox-fix-refused = The sandbox refused something this program needs. Read the log for the path, then add the directory with --add-dir or /add-dir, or run the command yourself outside a session.
doctor-sandbox-fix-allowed = The sandbox let a program reach something it exists to keep from one. Do not rely on it until this is fixed, and report it.
doctor-sandbox-fix-without = The workflow fails without the sandbox too, so the sandbox is not the cause. Check that the program works on this machine.
doctor-sandbox-fix-setup = Setting the workflow up failed outside the sandbox. Check that the program works on this machine.
doctor-sandbox-fix-not-confined = The platform would not confine the program: { $detail }
doctor-sandbox-fix-login = gh has a login outside the sandbox that it cannot read under it. It may be kept in a file the sandbox refuses, such as a keychain other than the login keychain. Add that file to sandbox.filesystem.allowRead in your settings, or store the token in a file the sandbox lets a program read with: gh auth login --insecure-storage
doctor-sandbox-log = log
doctor-sandbox-total = total
doctor-sandbox-counts = { $passed } passed, { $failed } failed, { $skipped } skipped

# /sandbox. The mode is a person's to choose, and the sentences say what it changes and from when.
session-sandbox-report = sandbox mode: { $detail }
session-sandbox-from-command = { $mode } from /sandbox
session-sandbox-from-flag = { $mode } from --sandbox
session-sandbox-needs-a-mode = /sandbox takes one of { $names }, or nothing to report the mode in force
session-sandbox-already = The sandbox mode is already { $mode }.
session-sandbox-set = The sandbox mode is { $mode } for the rest of this session.
session-sandbox-turn-follows-it = The turn running now uses it for each program it starts from now on. A program already started keeps its profile.
session-sandbox-set-off = The sandbox mode is off for the rest of this session: programs `run` starts have no profile.
session-sandbox-jobs-keep-their-mode = Background jobs still running keep the mode they started under and can still write the project: { $jobs }. /jobs stop <name> ends one.
session-sandbox-refused-mode =
    /sandbox { $asked } is refused: { $pinned_in } sets sandbox.mode to { $pinned }, and a session may
    read and write no more than that. { $looser }
session-sandbox-refused-network =
    /sandbox { $asked } is refused: { $pinned_in } pins run.network to closed, and a program started with
    no sandbox is not held to that. Use /sandbox workspace-write, read-only or strict.
cli-usage-doctor-push-identity = Ask GitHub which account each of your ssh keys pushes as
cli-doctor-push-identity-argument = doctor --push-identity takes at most one argument, written <host>/<owner>.
doctor-push-no-home = There is no home directory to look for ssh keys in.
doctor-push-no-keys = No public key file was found at ssh's default names or in an IdentityFile line of ~/.ssh/config, so there is no key to ask about.
doctor-push-remote = Remote
doctor-push-no-remote = no owner named, so no login can be compared with one; pass <host>/<owner> or run it in a repository whose origin is an ssh address on { $host }
doctor-push-no-fingerprint = fingerprint unreadable
doctor-push-login = registered to { $login }
doctor-push-login-owns = registered to { $login }, which owns { $owner }
doctor-push-login-differs = registered to { $login }, which is not { $owner } (it can still write there as a member of an organisation, or as a collaborator)
doctor-push-not-recognised = no account on { $host } has this key (Permission denied (publickey))
doctor-push-unanswered = { $host } gave no answer for this key
doctor-push-chosen = Chosen
doctor-push-chosen-by = git.identities names this key for the owner
doctor-push-chosen-at-prompt = you chose this key for the owner at a run prompt; /reach keys in the terminal client lists and forgets it

## The prompt that offers a finished plan in plan mode
agent-plan-approval-header = Plan
agent-plan-approval-accept-edits = Yes, accept edits
agent-plan-approval-accept-edits-detail = Leave plan mode and carry the plan out. Writes go through unasked; commands are still asked about.
agent-plan-approval-ask = Yes, ask before each edit
agent-plan-approval-ask-detail = Leave plan mode and carry the plan out, asking before every write and command.
agent-plan-approval-keep-planning = No, keep planning

# What the handler for a bravebot:// link is, in the usage table and the option list.
cli-usage-handle-uri = Open a session from a bravebot://open link, its prompt put in the box and not sent
# Said where `--handle-uri` is given anything but the one link, or a flag that would change how the
# session runs. A link carries a directory and words, and nothing else.
deep-link-takes-one-link = --handle-uri takes one bravebot://open link and nothing else
# Said where the `deepLinks` setting is false.
deep-link-switched-off = Links that open a session are switched off by the deepLinks setting
# Said where a link is read in a program with no terminal to open the session on.
deep-link-needs-a-terminal = A link opens a session in a terminal, and this program has none
# The ways a link is refused. None of them quotes the link: its text is somebody else's.
deep-link-not-a-link = That is not a bravebot://open link
deep-link-too-long = That link is longer than the { $limit } bytes read
deep-link-parameter = A link may carry dir and prompt, each once, and nothing else
deep-link-encoding = That link's text is not valid percent-encoded UTF-8
deep-link-directory = The link's dir is not an absolute path
# The first line of a session a link started in the home directory because of where it pointed. The
# path is the link's own and arrives escaped.
deep-link-started-in-home = The link named { $dir }, which is not a directory you have trusted here, so this session started in your home directory
# Under the box while the words an outside link put there are still in it.
deep-link-banner = Prompt from an external link, not sent yet  ·  read it, then Enter sends it
deep-link-banner-long = Prompt from an external link, { $count } characters, not sent  ·  scroll and review it all first

## Choosing one of the signed-in accounts' models at a start that nothing serves

# Said above the numbered rows at a start where a ChatGPT sign-in is stored, the model in force is one of
# Brave's and the account lists models.
chatgpt-start-heading = A ChatGPT sign-in is stored, and the account offers these models:
# The same where an xAI sign-in is stored, and where both are and both list models.
xai-start-heading = An xAI sign-in is stored, and the account offers these models:
accounts-start-heading = A ChatGPT sign-in and an xAI sign-in are stored, and the accounts offer these models:
# One row: the name the service shows and the name to give --model. Both are the service's text.
account-start-row = { $name } ({ $key })
account-start-question = Which one will bravebot use? Type its number, or nothing to stop:
# Said after the pick is recorded. The name is the service's text.
account-start-chosen = Using { $name }. It is remembered for the next start, and /model changes it.
# Said instead where what the start reads back is not the pick: the name is one that is not kept or the write
# failed. The start is then refused as it is where nothing is picked.
account-start-not-remembered = Could not remember { $name }, so no model is chosen.
# The same question where a Bedrock account or a gateway is configured and the model in force is Brave's own.
configured-start-heading = A model service is configured, and offers these models:
# What an xAI account that gave no row says at a start, under the first line of the refusal or above the
# rows another account gave. An ended sign-in says xai-ended.
xai-start-listing-unreadable = The xAI account's models could not be listed, so none is offered to choose from. Name one, as in `bravebot -i "<prompt>" --model xai/<model>` or `bravebot "<task>" --model xai/<model>`
xai-start-reaches-no-model = This xAI account lists no model that bravebot offers, so /model offers none. The sign-in stays stored.
# What the xAI sign-in command ends with, after the sign-in is stored. The first is for a start that would
# ask which of the account's models to use, and runs bravebot to be asked; the second is for a start that
# would not ask about the account, because a service of the person's own is configured, whose models a
# start asks about instead, or a model is already in force. Both name a model together with the prompt or
# the task that starts something.
xai-use-by-asking = Run bravebot to be asked which of the account's models to use, or name one, as in `bravebot -i "<prompt>" --model xai/<model>` or `bravebot "<task>" --model xai/<model>`. Once a session is open, /model lists them.
xai-use-by-naming = Name one of the account's models, as in `bravebot -i "<prompt>" --model xai/<model>` or `bravebot "<task>" --model xai/<model>`. Once a session is open, /model lists them.

## A model the ChatGPT account does not offer

# Said where the serving host refuses the model of a turn because the account does not offer it
# (CHATGPT-8). The first is the terminal's and the one-shot run's, lower case with a second sentence for
# the remedy, and the second is the window's, one sentence in sentence case. Neither names the model or
# quotes the service, and neither sends the person to sign in again: the refusal is of the model.
failure-account-model-not-offered = OpenAI does not offer that model to this ChatGPT account. Choose another model
desktop-chatgpt-model-not-offered = OpenAI does not offer that model to this ChatGPT account, so choose another model.

# What the pointer over a target says a click on it does, drawn on the row of hints in the room the
# hints leave. Each is one line in the interface's own voice, and none holds a word of the text the
# target is drawn over. The address or path of a link follows it on the same line where there is
# room.
hover-back = click to leave this view
hover-reply = click to copy this reply
hover-block = click to copy this block
hover-queued = click to take this prompt back into the box
hover-call-toggle = click to expand or collapse this call
hover-call-open = click to open the scroller on this call
hover-delegate = click to open this delegate's view
hover-address = ctrl-click to open this address in the browser
hover-path = click to show this file, ctrl-click to open it in the editor
hover-path-edit = ctrl-click to open this file in the editor
hover-panel-model = click to choose the model
hover-panel-effort = click to choose the effort
hover-panel-context = click to see how full the context is
hover-panel-servers = click to list the MCP servers
hover-panel-languages = click to list the language servers
hover-panel-hide = click to hide the panel
hover-panel-files = click to see the files this session changed
hover-panel-file = click to show the edits to this file
hover-panel-close-files = click to go back to the panel
hover-panel-edit-previous = click to go to the previous edit
hover-panel-edit-next = click to go to the next edit
hover-panel-name = click to rename this session
hover-panel-task = click to go to the turn that wrote this task
hover-panel-detail = click to open or close the words of this task
hover-panel-link = click to open this address in the browser
hover-footer-mode = click to change the permission mode
hover-footer-trail = click to show or hide the trail
hover-footer-shortcuts = click to show or hide the keys
hover-footer-info = click to show the info panel
hover-footer-watch = click to open the view of what the turn did
hover-footer-outputs = click to open what the commands printed
hover-footer-background = click to move the command to the background
hover-footer-held = click to return to the end
hover-footer-cancel-shell = click to leave shell mode
hover-footer-paste = click to paste the picture
hover-item = click to choose this one, click again to take it
hover-scrollbar = click to move the view here, drag the thumb to scroll

# The small menu a right click opens over a reply, a delegate's block or a held selection. One word
# or short phrase to an item, and each item does what the command or the click it stands for does.
menu-copy = copy
menu-copy-text = copy as text
menu-branch = branch this session
menu-open-delegate = open this delegate

## The prompt that offers a rule for the person's rule files (RECALL-3)
agent-rule-approval-header = Rule
agent-rule-approval-file = File
agent-rule-approval-why = Why
agent-rule-approval-private = These words came from private material, so they cannot be kept as a rule as written. Type the rule in your own words to keep it, or refuse.
agent-rule-approval-keep = Keep this rule
agent-rule-approval-keep-detail = Add it to the file named above. You can edit that file by hand, or type your own wording of the rule instead.
agent-rule-approval-refuse = Don't keep it
# Typed replies to the rule question that mean its first choice, and its second, in a session in lines
# that lists the choices and reads a line (RECALL-3). Comma-separated, matched ignoring case.
agent-rule-approval-yes-words = y, yes, keep, keep it
agent-rule-approval-no-words = n, no, refuse, don't, do not keep it

## What each catalog calls itself, in a list to choose it from

locale-name = English (US)


## The desktop window. A message here is written into the window's own code rather than the terminal's

ui-settings-appearance = Appearance
ui-settings = Settings
ui-settings-general = General
ui-settings-general-explained = How this window looks and which language it uses
ui-settings-theme = Theme
ui-settings-theme-explained = System follows the OS. Light and Dark stay put regardless of it.
ui-appearance-system = System
ui-appearance-light = Light
ui-appearance-dark = Dark
ui-settings-language = Language
ui-settings-display-language = Display language
ui-settings-language-explained = Automatic follows the language in your settings file, then the system's.
ui-settings-language-automatic = Automatic
ui-settings-language-next-start = Applies the next time bravebot starts.
ui-settings-language-overridden = BRAVEBOT_LOCALE is set, so it decides the language whatever is chosen here.
ui-settings-language-unsaved = Couldn't save the language.
ui-settings-language-unread = Couldn't read the language setting.
ui-settings-restart = Restart
ui-settings-restart-running = A turn is still running. Restart once it has finished or been stopped.
gcloud-not-installed = The Google Cloud CLI (`gcloud`) was not found. Install it and run `gcloud auth application-default login`, or set GOOGLE_API_KEY to use Google Vertex AI with a key.
gcloud-refused = The Google Cloud CLI gave no access token ({ $detail }). Run `gcloud auth application-default login`.
gcloud-no-token = the reply was not a token
gcloud-no-reason = the Google Cloud CLI gave no reason
gcloud-login-incomplete = the sign-in did not complete
doctor-gateway-token-google-cloud = none needed (the Google Cloud CLI is asked for an access token)
