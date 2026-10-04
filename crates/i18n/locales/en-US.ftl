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
cli-usage-plain = Start a session in lines, taking nothing from the terminal
cli-usage-task = Run a single task
cli-usage-piped = ...with piped input, never trusted
cli-usage-resume = Pick up a session in this directory
cli-usage-continue = Pick up the most recent session in this directory
cli-usage-fork = Fork a session and start exploring a different path
cli-usage-doctor = Check configuration and confinement
cli-usage-import = Import a Leo Premium subscription
cli-usage-import-providers = Import a model service Claude Code or opencode configured
cli-usage-auth-login = Sign in to a model service, listing every way when none is named
cli-usage-auth-logout = Forget an imported Leo Premium subscription or a stored gateway key
cli-usage-mcp = Declare, list and approve MCP servers

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
# Said where the startup question was not put because an earlier session here was told to remember
# the answer (TRUST-23). A session in lines has no slash commands, so the ways to be asked again are
# the ones it can name: the command in the interface that draws, or the lines in the file. The
# lines rather than the file, since a directory whose path is spelled alike shares the file.
cli-plain-trusting-kept =
    trusting { $directory } (you said to remember it { $when }; to be asked again, run
    /forget-trust in bravebot without --plain, or delete the lines naming it from { $path })

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

cli-options-heading = Options:
cli-option-file = Include a workspace file as context (repeatable)
cli-option-add-dir = Reach into a directory outside the working one (repeatable)
cli-option-settings = Read this settings file for this run, above the ones found on disk
cli-option-agent = Address every turn to this definition, as /agent does for one
cli-option-system-prompt =
    Replace the opening sentence of the planner's system prompt for every turn. The rest of it stays
cli-option-append-system-prompt =
    Add this text to the planner's standing instructions for every turn, after AGENTS.md
cli-option-mode = turn (default) decides step by step; manifest plans the whole run first
cli-option-model = The model this run asks for, in place of the remembered or configured one
cli-option-effort = How hard this run asks the model to think, in place of the remembered or configured level
cli-option-print = Non-interactive. Reads piped stdin as quarantined context
cli-option-trace = Print the audit trail
cli-option-json = Print one result object on stdout instead of the reply
cli-option-incognito = Write nothing to ~/.bravebot: no history, no session record, no preference
cli-option-vet =
    For this run, let a check answer: content it finds nothing in is promoted without asking you,
    and where nobody can be asked, anything else is kept back
cli-option-dangerously-skip-permissions =
    Bypass all permission checks. Recommended only for sandboxes with no internet access
cli-option-help = Show this message
cli-option-version = Show the version


## What a command-line run says when it cannot start

cli-unknown-option = unknown option: { $flag }
cli-file-needs-a-path = --file requires a path
cli-add-dir-needs-a-path = --add-dir requires an absolute path to a directory
cli-settings-needs-a-path = --settings requires a path to a settings file
cli-settings-not-a-file = --settings names no file: { $path }
cli-agent-needs-a-name = --agent requires the name of a definition
# The flag is one of --resume, --continue and --fork, as typed.
cli-agent-not-with-a-recorded-session =
    --agent starts a new session, and { $flag } picks up a recorded one, which does not keep the
    definition it worked under
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
# Said when a session in lines started with --agent opens (CLI-17). It has no slash commands, so it
# names no way to address another definition. The model is the one the definition names.
cli-plain-working-under = every prompt is addressed to { $definition }
cli-plain-working-under-model = every prompt is addressed to { $definition }, which asks for { $model }
# The flag that asks to be asked about nothing, where a settings layer made that mode unreachable.
# The file is named because the flag is documented and works everywhere else, so a refusal without
# it sends somebody looking for a fault in the program.
cli-bypass-unreachable =
    --dangerously-skip-permissions is refused: permissions.bypassUnreachable in { $path } makes
    that mode unreachable here. Remove it there, or run without the flag.
cli-mode-needs-a-name = --mode requires one of { $names }
cli-model-needs-a-name = --model requires the name of a model
cli-effort-needs-a-level = --effort requires one of { $levels }
cli-unexpected-argument = unexpected argument: { $argument }
cli-task-required = a task is required
cli-configuration-problem = configuration error: { $problem }
cli-workspace-problem = workspace error: { $problem }
cli-interface-problem = interface error: { $problem }
cli-directory-unknown = cannot tell which directory this is
cli-no-such-session = no session { $id } in this directory
cli-manifest-run = { $id } is a manifest run, so there is nothing to continue; this is what it did
cli-nothing-to-continue = no session to continue in this directory
cli-fork-needs-a-name = --fork requires a session id
cli-piped-input-unreadable = warning: could not read piped input: { $problem }
cli-piped-input-too-large =
    piped input is larger than { $limit } MiB. Write it to a file and name that instead


## What a run says when no model service is configured

# Said instead of starting work at all. A session opened with nothing configured reads as the agent
# being poor rather than as nothing having been set up yet, so the first run says what to configure
# instead of starting work that has no service to do it.
onboarding-no-model = no model service is configured yet
# Said beside it where a subscription is stored and could not be read, because somebody in that
# case is one import away rather than a whole configuration away.
onboarding-subscription-unusable = the subscription that is stored could not be used: { $problem }
# Said before the routes where Claude Code or opencode configures a service bravebot can use, or a
# running Ollama serves one, and nobody was there to be asked about it: a one-shot run, --json, a
# pipe, or doctor.
onboarding-import-one =
    { $source } configures a model service bravebot can use: run `bravebot auth login import` in a terminal to import it.
# The same, where the one source is Ollama running on this machine.
onboarding-import-running =
    { $source } is running here with models bravebot can use: run `bravebot auth login import` in a terminal to import it.
onboarding-import-both =
    { $first } and { $second } each have a model service bravebot can use: run `bravebot auth login import` in a terminal to import them.
onboarding-import-three =
    { $first }, { $second } and { $third } each have a model service bravebot can use: run `bravebot auth login import` in a terminal to import them.
# Said instead, where a service is configured and only the model in force is Brave's own. A
# settings block copied out of another tool names its models and names no default, so this is
# where somebody following that route lands, and what they have to do is name one of their own.
onboarding-name-a-configured-model =
    A service is configured, but the model in force is one of Brave's: name one of your own with the `model` key in ~/.bravebot/settings.json, or with --model on a one-shot run. `bravebot doctor` lists what each configured service offers.
onboarding-pick-one = Configure one of these, then run bravebot again:
onboarding-bedrock =
    AWS Bedrock, through your own account: put a `provider` block named `amazon-bedrock` in ~/.bravebot/settings.json, with its region and the models to offer.
onboarding-openrouter =
    OpenRouter, or any other OpenAI-compatible gateway: put a `provider` block named for it in ~/.bravebot/settings.json, with the variable that holds its API key and the models to offer.
# Last of the three, and said to be last, because these models are reached through Brave's AI
# gateway, which has open problems of its own. It is still the shortest route for somebody who
# already subscribes, so it is offered rather than left out.
onboarding-leo =
    Brave Leo Premium, if you already subscribe: run `bravebot auth login leo` on a machine where Brave is signed in to that subscription. It reaches models through Brave's AI gateway, which has open issues being worked on, so prefer one of the two above for now.
# Said after either, so it reads after the three routes and after the one line alike.
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
doctor-ends-gateway-token =
    a gateway bearer token: issued by { $gateway }, which is also the only surface that revokes it; deleting it from the settings file, unsetting the variable or running `bravebot auth logout gateway` ends this machine's custody and leaves the token live there
doctor-ends-subscription-batch =
    an imported subscription's credential batch: minted by Brave's subscription service against the order this install registered as a device on; each credential is spent by one premium request and the batch stops working when its last window closes, and nothing revokes an unspent one, so `bravebot auth logout leo` ends this machine's custody and leaves the batch spendable by whatever copied the file
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
doctor-dropped-gateway-token-not-minted-for-one-step =
    gate { $gate }, { $answer }: it is not minted for one step, since the token is whatever the settings file carries, the variable holds or `bravebot auth login gateway` stored, and it is held for the whole run
doctor-dropped-subscription-batch-nothing-decides-each-use =
    gate { $gate }, { $answer }: nothing the agent cannot impersonate decides each use, since this process presents a credential from the batch itself and nothing is asked to authorise the request
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
doctor-settings-model-ignored =
    model in { $path } is not obeyed: it is read from ~/.bravebot/settings.json and from the file --settings names only
# A key that only ever refuses, spelled as something other than a boolean. It is read as absence, so
# the session is as permissive as one that named nothing, and nothing else would say so.
doctor-settings-narrowing-ignored =
    { $key } in { $path } is not a boolean, so it is read as absent and refuses nothing
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
doctor-leo = leo
doctor-subscription =
    { $environment } subscription imported, { $unspent } of { $total } credentials unspent
# Which variable answered as well as where the directory is: more than one can name a profile
# directory, and the one that won is what somebody has to change to put the directory elsewhere.
doctor-state-directory = state directory { $path }, from { $variable }
# What this program asks for as each file is created is a mode no other account can read. Where the
# platform is not told that, the files carry whatever the profile directory grants them instead. A
# prompt history holds every path, branch name and pasted fragment somebody has typed, so which of
# the two they have is theirs to know rather than a detail of the build.
doctor-state-directory-unprotected = not restricted
doctor-state-directory-permissions =
    prompt history, session records and saved choices carry your profile directory's permissions
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
doctor-mechanisms = mechanisms
doctor-network-denial = network denial
doctor-kernel-enforced = kernel-enforced
doctor-not-enforced = NOT enforced
doctor-confinement-unavailable = confinement unavailable
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
doctor-no-proxy = not proxied


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
permission-rule-unanchored = needs a home directory or a settings directory to say where it points
permission-rule-not-a-domain-rule = needs a domain, written WebFetch(domain:example.com)
permission-rule-no-domain-named = names no domain after 'domain:'
permission-rule-not-a-tool-rule = needs a server, or a server and one of its tools, written Mcp(weather) or Mcp(weather:get_forecast)
# A permissions block, or its deny, ask or allow list, written as something other than a list.
# The rules another settings file wrote still apply, which is what "removes none" says.
permission-rule-not-a-list = sets no rules and removes none; rules go in a list, such as "deny": ["Read(./.env)"]


## Importing a Leo Premium subscription

leo-no-premium-endpoint =
    warning: this build has no premium endpoint, so imported credentials will not be used
leo-set-and-rebuild = set { $variable } and rebuild
leo-unknown-channel = unknown channel: { $channel }
leo-expected-channel = expected one of: stable, beta, nightly, development
leo-forgotten = forgot the imported subscription
leo-forget-takes-no-channel = --forget takes no channel: one subscription is stored for every channel
leo-not-while-incognito = an import stores credentials on disk, which an incognito session will not do
leo-looking = looking for a Leo subscription in Brave { $channel }
leo-found = found a { $environment } subscription: { $order }
leo-registering = registering this install as a new device
leo-stored = stored { $count } credentials in { $path }, valid through { $expiry }
leo-browser-untouched =
    premium requests will now use them; the browser's own credentials were untouched

# Said when a subscription is stored but could not be read. Worth a line because the request
# then goes out with no subscription, where a premium model name is answered by a weaker model
# rather than by an error, so the only symptom is a worse answer.
subscription-unusable =
    the imported subscription could not be used ({ $problem }), so this turn spends none

# Said when a background job exits. The line drawn when it started said only that something had
# been started, and nothing else in the transcript ever says it is over, so a build that failed
# while the turn was doing something else would leave nothing on the screen about it. What it
# printed goes to the view a person can open; this is the sentence saying the thing has ended.
background-job-finished = `{ $command }` finished in the background: { $outcome }

# Said when a reply reached the output limit and the model is asked again rather than the turn
# ending. Which one follows from what the reply was doing when it stopped: a call it was writing
# was never made, so the work is asked for in parts, and a reply that wrote nothing is asked again.
ceiling-stop-in-call = the model reached its output limit of { $tokens } tokens while writing a call to { $tool }, so the call was not made; asking it to do the work in smaller parts
ceiling-stop-in-a-call = the model reached its output limit of { $tokens } tokens while writing a tool call, so the call was not made; asking it to do the work in smaller parts
ceiling-stop-thinking = the model reached its output limit of { $tokens } tokens while thinking, before it wrote anything; asking it again
ceiling-stop-silent = the model reached its output limit of { $tokens } tokens before it wrote anything; asking it again
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


## Importing a model service Claude Code or opencode configured, or a running Ollama serves

# Said above everything an import would write, naming the files it was read from. Every name and
# value follows before the question, because what is written is what the person approves.
import-found = { $source } configures a model service bravebot can use, in { $files }.
# Where no file was read: the setup is exported rather than written down.
import-found-exported =
    { $source } configures a model service bravebot can use, in this process's environment.
# Where the source is a server that answered rather than a file: Ollama, at the address asked.
import-found-running = { $source } is running at { $url }, serving models bravebot can use.
import-adds = Importing it adds these to { $file }:
# One gateway, with the host its requests go to: that host is where a credential is sent, so it is
# the part of the entry the question is really about.
import-adds-gateway = provider.{ $id }, reached at { $endpoint }: { $entry }
import-key-held = provider.{ $id }: a key is held for it, which is asked about on its own
import-key-file =
    provider.{ $id }: its key is read from { $path }, which is not followed, so no key is written
import-kept = Left as they are, since { $file } already sets them:
# A model a Bedrock tier variable names, which the tiers answer for before any entry does.
import-named = Not added, since a tier already names them:
import-named-model = { $model } in provider.{ $id }, named by { $variable }
import-pinned = Not offered, since { $file } sets them for every user of this machine:
# Found and not imported, one line each, by name and reason and never by value.
import-left-heading = Found in { $source } and not imported:
import-left-anthropic-api = Anthropic's own API, whose wire format no service here speaks
import-left-vertex = Google Vertex AI without a Google Cloud project, or through Google Cloud credentials, neither of which a service here reaches
import-left-bearer-token =
    a Bedrock API key; bravebot signs Bedrock requests through the AWS credential chain instead
import-left-no-region = Bedrock with no region to sign for
import-left-sign-in = a sign-in that belongs to opencode
import-left-another-sdk = an entry reached through an SDK other than an OpenAI-compatible one
import-left-no-endpoint = no reachable endpoint is stated or known for it
import-left-substitution =
    its key is built from an opencode substitution inside a longer value, which bravebot does not make
import-left-elsewhere = names a server on another machine, which is not asked
import-left-no-tool-model = running there, with no model that can call tools
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
    neither Claude Code nor opencode configures a model service bravebot can use, and no Ollama serving one is running here
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
# The heading over the list. Each line under it starts with a number and a word, leo, bedrock,
# import or gateway, which a script types and which are not translated.
auth-ways-heading = Ways to sign in to a model service:
auth-way-leo = Brave Leo Premium, from a Brave install that subscribes
auth-way-bedrock = An AWS account, for Amazon Bedrock
auth-way-import = A model service Claude Code, opencode or Ollama has, imported into settings
auth-way-gateway = A key for a gateway a provider block in settings names, typed here and kept by bravebot
# The status of the gateway way where keys are stored. The ids are provider ids.
auth-gateway-held = a key stored for { $ids }
auth-gateway-held-unreadable = the file of gateway keys cannot be read
# A way that is already signed in, with what it holds.
auth-way-held = { $description } ({ $status })
auth-signed-in = signed in
auth-which-way = Which one? Type its number or its name, or nothing to stop:
# Said where the answer to auth-which-way names none of the ways listed.
auth-not-a-listed-way = { $answer } is not one of the ways listed
auth-which-channel =
    Which Brave channel subscribes? stable, beta, nightly or development, or nothing for stable:
# Said before auth-sign-in-again. The status is doctor-subscription.
auth-leo-held =
    Brave Leo Premium is signed in: { $status }. bravebot auth logout leo signs out.
# A second sign-in registers this machine with Brave as one more device.
auth-sign-in-again = Sign in again, as a new device?
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
auth-gateway-not-while-incognito =
    an incognito session writes nothing to disk, and storing a key is a write: run bravebot auth login gateway without --incognito
auth-gateway-none-configured =
    no gateway is configured: add a provider block to settings.json, or run bravebot auth login import, then store its key here
auth-gateway-not-configured = that is not the id of a provider block; the gateways configured are { $ids }
auth-gateway-needs-a-terminal =
    a gateway key is typed at a terminal with nothing drawn, so bravebot auth login gateway needs a terminal
auth-gateway-no-home = there is no home directory to store the key in
auth-gateway-keys-unreadable =
    { $path } is not a file of keys bravebot wrote, so it was left as it is and nothing was changed
auth-gateways-heading = Gateways configured:
# Beside a gateway in the list under auth-gateways-heading.
auth-gateway-key-stored = a key stored
auth-which-gateway = Which one? Type its number or its id, or nothing to stop:
auth-not-a-listed-gateway = { $answer } is not one of the gateways listed
auth-gateway-key-held = A key is already stored for { $id }.
auth-gateway-replace = Replace it?
# Asked with echo off, so nothing appears as the key is typed or pasted.
auth-gateway-key-question = Key for { $id }, sent to { $host } (not shown as you type):
auth-gateway-nothing-stored = nothing was stored
auth-gateway-not-read = the key could not be read from the terminal: { $error }
auth-gateway-not-stored = the key was not stored: { $path }: { $error }
auth-gateway-stored =
    the key for { $id } is stored in { $path }, and sessions started from now on send it to { $host }
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
mcp-env-needs-a-name = -e needs NAME=value, or the name of a variable read from your environment
mcp-env-after-the-alias =
    -e and --env come after the alias, as in add weather -e KEY=value -- weather-mcp
mcp-env-word-refused =
    word { $position } after add is not NAME=value, and is not repeated since it may be a value: a
    name alone is read from your environment only as the one word its -e takes
mcp-dir-needs-a-path = --dir needs a directory
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
mcp-problem-url = the url is not http or https with a host
mcp-problem-credentials =
    the url carries a user or a password, which would keep a credential in plain text
mcp-problem-remote = a remote server takes no { $key }
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
# A variable given its value at add, which is kept in mcp.json and is never shown.
mcp-variable-stored = { $name } (stored)
# A file a stored value or an argument names, which the server is let read.
mcp-may-read = may read: { $path }
mcp-directory = directory, which it may write: { $path }
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
servers-for-this-session-only =
    { $alias } is used in this session only: an incognito session records no answer
servers-not-kept = { $alias } is used, and its approval was not recorded: { $reason }
servers-project-not-kept = { $path } was not recorded as a project whose servers are used
servers-not-confined = { $alias } was not started, since nothing here can confine it: { $reason }
servers-no-confinement-here =
    { $alias } was not started: this platform has no confinement for a local MCP server yet
servers-no-home =
    { $alias } was not started: a directory of its own could not be made in { $path }: { $reason }
servers-no-handshake = { $alias } was started and did not complete its handshake: { $reason }
servers-too-slow = { $alias } did not complete its handshake within { $seconds } seconds

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

## The tools an MCP server offers, read by the person before any of them is offered to the model

mcp-tools-title = offer these tools to the model?
mcp-tools-offered =
    { $count ->
        [one] { $alias } offers one tool
       *[other] { $alias } offers { $count } tools
    }
mcp-tools-none = { $alias } lists no tool it can offer
mcp-tools-changed = this is not the list you said yes to before: the tools it offers have changed
mcp-tools-explained =
    The model will read each tool's name, its arguments and what the server says about it, as
    shown here. Every call is still put to you. Say no if a description gives instructions.
mcp-tools-not-listed =
    { $count ->
        [one] one more tool is not listed: its name or its arguments cannot be offered
       *[other] { $count } more tools are not listed: their names or their arguments cannot be offered
    }
# How one argument's kind reads in a list, as in `city_name (string, required)`. The kinds are
# the server's JSON Schema words and stay as it wrote them.
mcp-tools-argument-list-of = { $kind } of { $items }
mcp-tools-argument-required = required
mcp-tools-yes = Yes, offer them
mcp-tools-no = No, continue without them
mcp-tools-declined = { $alias } offers no tool in this session: its list was not approved
mcp-tools-refused = { $alias } offers no tool in this session: { $reason }
mcp-tools-not-recorded =
    the tools { $alias } offers are approved for this session only, since the answer could not be
    recorded: { $error }

## One call to a tool of an MCP server

mcp-call-title = call this tool?
mcp-call-kind = (MCP)
mcp-call-no-arguments = no arguments
mcp-call-question = Proceed?
mcp-call-yes = Yes
mcp-call-stand = Yes, and stop asking for { $tool } in this project
mcp-call-cannot-stand = not offered: nothing answered in this session can be recorded
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

## Vouching for a directory, asked once when a session starts somewhere new

trust-directory-title = trust this directory?
trust-directory-question = Trust
trust-directory-explained =
    Files here will be read as trusted, and edits to them will not be shown to you one by
    one. Say no if you did not write this code.
trust-directory-regardless =
    Either way, anything derived from the web or from an untrusted file is still shown
    before it is written.
trust-directory-yes = trust it
trust-directory-no = ask me about every write
# The key that keeps the answer for later sessions (TRUST-23). Offered only where it can be written
# down, and its lines say what it covers and where it goes, since nobody can endorse a record they
# were not shown.
trust-directory-remember = trust and remember
trust-directory-remember-explained =
    r: trust it, and skip this question in later sessions started in exactly this directory
trust-directory-remember-exact =
    A session started inside or above this directory is still asked, and so is one started in a directory deleted and made again here.
trust-directory-remember-where = /forget-trust takes it back, and it is written down here:
# Above the keys while the lines saying what r does and where it writes have not been on the screen
# together, which is when r is not taken. What r does comes first, so a narrow terminal that cuts the
# line off keeps it.
trust-directory-remember-unseen = ↑↓ r remembers nothing: what it writes is not shown yet
# The same where those lines are taller than the box, so no scroll shows them together.
trust-directory-remember-too-small = r remembers nothing: what it writes is taller than this box
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
# Both rosters are offered at once, and a tier name alone does not say which of the two it is: the
# same model is reachable through either, billed and reached differently. Not "Bedrock" alone, which
# the Brave roster already says of the models it serves through its own account.
picker-service-bedrock-profile = Bedrock, your { $profile } AWS profile
picker-service-bedrock = Bedrock, your AWS account
# Ctrl-R over the prompts already sent.
history-search-title = Search prompts
history-scope-everywhere = everywhere
history-scope-here = this project
history-search-placeholder = Filter history…
history-search-keys = ↑↓ to move  ·  Enter to use  ·  { $scope } to scope  ·  Esc to cancel
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
input-history-position = History { $index }/{ $total }
# At the other end of that same border, the ways in: the search over every prompt, and the search
# narrowed to the ones sent from this project. The narrower one is dropped where the row will not
# hold both beside the position, and then the other one is, so a longer wording is one that fewer
# terminals show at all.
input-history-search = { $chord } to search
input-history-scope = { $chord } this project
resume-heading = Resume session
resume-search-placeholder = Search…
resume-keys = ↑↓ to choose  ·  Enter to resume  ·  type to search  ·  Esc for a new session
resume-nothing-matches = nothing matches that
resume-manifest-run = that was a manifest run, which cannot be continued; start a new session


## Shared by every question the interface stops to ask

stop-the-turn = stop the turn
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
write-untrusted = untrusted: nobody has read this, and the model never saw it
write-remark =
    what the isolated processor said about this change, which nothing has checked against it
write-credentials =
    this looks like it would put a secret in the tree, going by the name beside the value and how
    the value reads. Nothing recognised it as a particular provider's key, so it is a guess and
    yours to settle
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
run-line-sent = the model wrote:
run-writes = it writes these files:
run-is-fed = it is fed the contents of:
run-not-sandboxed = this is not sandboxed: it runs with the access your own shell has
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
run-private-not-remembered =
    private input is asked about every time, so this one cannot be remembered
run-assignment-not-remembered =
    an assignment in front of a program is asked about every time, so this one cannot be remembered
run-write-not-remembered =
    a line naming a file to write is asked about every time, so this one cannot be remembered
run-remember-explained = r: stop asking about this exact line, in this directory, from now on
run-remember-where = it is written down here, and deleting the line is the way back:
run-remember-only-asking = it stops the asking only: what it prints stays quarantined
run-remember-every-session = every session started in this directory reads it, not just this one
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
status-agent = Agent
status-agent-every-turn = every turn is addressed to it, named with --agent
status-effort = Effort
status-effort-chosen = chosen with /effort
status-effort-default = whatever the service does on its own
status-effort-not-read = chosen with /effort, but this model reads none
status-theme = Theme
status-theme-chosen = chosen with /theme
status-served = Answered by
status-served-instead = served instead of { $asked }, which that turn asked for
status-endpoint = Endpoint
# Which tier the last turn actually ran on, not what this build was compiled knowing about.
status-premium-available = premium available, nothing sent yet
status-premium-in-use = premium, a credential was spent
status-premium-not-spent = no subscription was spent
status-no-subscription = no subscription configured
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
status-loop = Loop
status-loop-every = every { $every }
status-loop-self-paced = paced by each turn
status-loop-next = next in { $next }
status-loop-running = running now
status-loop-unpaced = waiting for the turn to say when
status-goal = Goal
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
# Said only where auto-vetting is on. The file is named because that is where the answer is kept
# and where it is undone; nothing in the interface turns it back off.
status-vetting = Vetting
status-vetting-auto = a check that finds nothing reads content to the model without asking
status-vetting-where = kept in ~/.bravebot/vetting
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
status-remembered = Remembered lines
status-remembered-note = run unasked in this directory, and their output stays quarantined
status-remembered-this-session = remembered in this session
status-remembered-earlier = remembered in an earlier session
status-remembered-where = delete a line from { $path } to be asked again
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
# The same, over a picture or a PDF, which has no lines to count.
indicator-checking-picture = Checking a picture
indicator-checking-pdf = Checking a PDF
# Said while a turn is being stopped, from the press that asks for it until the turn ends. The turn
# ends once its worker and every delegate it started have returned, which can take seconds, and a
# screen still saying what it said before the press reads as a press nobody heard.
indicator-stopping = Stopping
# Abbreviated counts, already rounded to one place.
tokens-thousands = { $thousands }k
tokens-millions = { $millions }M
# Said once a turn is over, because the end of one used to be announced by the indicator
# disappearing, and an announcement made by something vanishing is one nobody reads.
turn-done = turn { $turn } done
turn-failed = turn { $turn } failed
# Said of a turn the person stopped themselves, which is neither of the above.
turn-cancelled = turn { $turn } cancelled


## Picking up a session that ran somewhere, or on something, else

session-reopen-failed = could not reopen { $directory }: { $problem }
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


## Handing the line to an editor

editor-none-configured = no editor found: set $VISUAL or $EDITOR to the one you want
editor-scratch-unusable = the file to edit could not be used: { $problem }
editor-named-but-missing =
    '{ $command }' was not found, and $VISUAL or $EDITOR names it, so nothing else was tried
editor-exited-badly = { $editor } exited with status { $code }, so the line is unchanged
editor-was-stopped = { $editor } was stopped before it finished, so the line is unchanged
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
scroller-key-editor = open the transcript in $EDITOR
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
watching-answered = answered
watching-failed = did not finish
watching-position = { $at } of { $total }
watching-keys = q closes  ·  n / p another delegate
watching-keys-one = q closes
watching-keys-back = q goes back  ·  n / p another delegate
watching-nothing-yet = nothing yet
# The list of every delegate this turn has spawned, with the session above them.
watching-list-title = delegates
watching-list-keys = up / down moves  ·  enter opens  ·  q closes
# The first row of the list: the conversation the delegates were spawned from.
watching-list-session = session
watching-list-session-detail = back to the conversation
watching-calls = { $count ->
    [one] { $count } call
   *[other] { $count } calls
    }
# Said on the bottom line once the view has anything to open, which is the one row that outlasts
# the turn that drew it. The count is there because a key with nothing behind it is not worth
# pressing. Every kind of row is counted together, since one key opens the list holding all of
# them and naming one kind here would undercount the rest.
watching-hint = { $chord } { $count } to open
# Said on the bottom line for as long as a command the turn is waiting on can be moved, and gone
# the moment it ends or is moved. Short, because it shares the line with everything else there.
background-hint = { $chord } to background
# Said on the bottom line while the info panel is closed and the terminal is wide enough for it.
panel-hint = { $chord } info
# The info panel's last row.
panel-hide = { $chord } hide panel
# Left in the transcript when the info panel's key is pressed on a terminal too narrow for it.
panel-too-narrow = The info panel needs a terminal at least { $columns } columns wide.
# The info panel's section headings.
panel-session = Session
panel-goal = Goal
panel-context = Context
panel-language-servers = Language servers
panel-mcp-servers = MCP servers
panel-plan = Plan
panel-links = Links
# The rows of the info panel's Links section, each followed by the link.
panel-pull-request = Pull request
panel-issue = Issue
# The last turn's cache figures in the info panel, one to a row and never added together.
panel-cache-read = cache read { $tokens }
panel-cache-written = cache written { $tokens }
# Where the info panel has no room for the whole plan.
panel-more = +{ $count } more
# Where the info panel's plan starts past its first rows, counting those it left out above.
panel-earlier = +{ $count } earlier
# The same, with rows left out below as well.
panel-earlier-and-more = +{ $earlier } earlier, +{ $later } more
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
command-model = Choose which model to think with
command-theme = Choose which theme paints the interface
command-effort = Choose how hard to think before answering
command-config = Choose how the input box edits text
command-add-dir = Open another directory, and trust it for this session
command-cd = Work in another directory from now on, and trust it for this session
command-rename = Call this conversation something else
command-issue = Say which issue this session is for, show it, or clear it
command-pr = Say which pull request this session is for, show it, or clear it
command-compact = Summarise the conversation so far, keeping the recent part
command-btw = Ask something beside the work, without putting it in the conversation
command-clear = Start a new session here, keeping this one resumable
command-forget-trust = Stop remembering that this directory is trusted, so later sessions here ask
command-loop = Send a prompt again and again, say what is repeating, or stop it
command-goal = Keep working until a condition you set is judged met
command-watch = List the files this session is watching, and stop one by its number
command-jobs = List this turn's background jobs, and stop one by its name
command-panel = Show or hide the info panel beside the transcript
command-checkouts = List the checkouts delegates kept, and remove one by its number
command-manifest = Plan one task in full, show you the plan, then run it with nothing re-planned
command-agent = Run one of your definitions on a task, by its name
command-export = Export the session transcript to a markdown file
command-undo = Rewind one turn and put back the files it wrote
command-rewind = List the turns a rewind could go back to, or go back that many
command-exit = Leave


## Where a skill offered after a slash was found

skill-from-project = (project)
skill-from-user = (user)
skill-from-built-in = (built-in)


## What the session says back

session-resumed = resumed session: { $title }
session-renamed = renamed to { $title }
session-rename-needs-a-name = /rename needs a name, as in /rename the parser bug
session-rename-needs-something = /rename needs a name with something in it
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
session-exported = exported transcript to { $path }
session-export-failed = could not export transcript: { $problem }
session-add-dir-needs-a-path = /add-dir needs a directory, as in /add-dir ~/notes
session-directory-added = added { $directory }, and trusting it for this session
session-cd-needs-a-path = /cd needs a directory, as in /cd ~/projects/other
session-directory-changed = now working in { $directory }, and trusting it for this session
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
session-theme-set = theme { $theme }
session-no-such-theme = no theme named { $theme }; try /theme for the list
session-editing-vi = editing the way vi does; esc for commands, i to type
session-editing-ordinary = editing with the arrows and the readline chords
session-effort-set = thinking at { $effort }
session-effort-unset = thinking as the service decides
session-no-such-effort = no effort level named { $effort }; try /effort for the list
session-effort-not-read = this model reads no effort level, so requests carry none
session-trusting = trusting { $directory }
session-trusting-as-left = trusting { $directory } (as this session left it)
# Said where the question was never put, because the mode in force answers it. Naming the flag is
# the point: this is the one grant a person did not make by pressing a key, so the line has to say
# what made it.
session-trusting-unasked =
    trusting { $directory } (--dangerously-skip-permissions, so you were not asked)
# Said where the question was not put because an earlier session here was told to remember the
# answer (TRUST-23). When it was given and how to take it back, because this is a grant nobody made
# in this session and the line is the only thing on the screen that says where it came from.
session-trusting-kept =
    trusting { $directory } (you said to remember it { $when }; /forget-trust to be asked again)
session-trust-kept =
    trusting { $directory }, and later sessions started here will not ask; /forget-trust takes it back
# The answer was given, but writing it down failed, so the next session will ask after all.
session-trust-not-kept =
    trusting { $directory } for this session only: the answer could not be written to { $path }, so the next session here will ask
session-trust-forgotten =
    the next session started in { $directory } will ask whether to trust it; this one keeps its answer, and /clear starts one that asks
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
session-something-was-refused = a policy gate refused something during that turn
# The endpoint substitutes a model it will not serve rather than refusing, so without this a
# session can ask for one model and be answered by another with nothing said.
session-model-substituted =
    { $asked } was not served: the endpoint answered with { $served }. Run `bravebot doctor` if a
    subscription was expected.
session-error = error: { $problem }
session-no-output = no output

## Why a turn failed

# Fixed descriptions keep raw backend error text out of the interface.
failure-unauthorized = the service would not accept the credentials
failure-rate-limited = the service asked for fewer requests
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


## Repeating a prompt

loop-needs-a-prompt =
    /loop needs something to repeat, as in /loop 5m check the deploy, or /loop watch the build to
    let each turn say when to run again
loop-started-every =
    repeating every { $every }; /loop stop ends it, and so does ctrl-c or leaving
loop-started-self-paced =
    repeating at a pace each turn sets; /loop stop ends it, and so does ctrl-c or leaving
# The answer to the bare command. The line is in it because the note above scrolls away, and
# somebody asking what is repeating has usually lost sight of what they set going.
loop-active = repeating: { $prompt } · { $pace } · { $when }
loop-ends-with = /loop stop ends it, and so does ctrl-c or leaving
loop-none =
    nothing is repeating. /loop 5m check the deploy sends a line every five minutes, /loop watch the
    build lets each turn say when to run again, and /loop stop ends either of them
# The part of the row under the box that says a loop is live, which between ticks is the only thing
# on the screen that does. Short on purpose: it shares that row with the mode and the readings, and
# a part a narrow terminal has no room for is a part the row gives up.
loop-hint = looping
loop-hint-next = looping, next in { $next }
loop-interval-raised = the interval was raised to { $every }, which is as fast as a loop goes
loop-interval-capped = the interval was capped at { $every }, which is as long as a loop lives
loop-replaced = the loop that was running has been replaced
loop-tick = loop { $count }
loop-tick-quiet = { $quiet ->
    [one] loop { $count }, after { $quiet } tick that found nothing
   *[other] loop { $count }, after { $quiet } ticks that found nothing
    }
loop-stopped = the loop is stopped
loop-cleared = the loop is stopped, because it belonged to the session that was cleared
loop-aged-out = the loop has run for a week and stopped itself
loop-unpaced = that turn did not say when to run again, so the loop has stopped
loop-busy = /loop starts with a turn of its own, so it waits until this one is done
loop-replaces-goal =
    the goal that was set has been cleared: a session works towards one thing at a time
loop-armed-by-the-turn =
    looking again in { $after }, repeating what you asked; /loop stop ends it, and so does ctrl-c
    or leaving
loop-not-armed-under-a-goal =
    a later look was asked for and not started: this session is working towards a goal, and it
    does one thing at a time
loop-not-armed-under-a-watch =
    a later look was asked for and not started: this session is already watching a file, and it
    does one thing at a time


## Working towards a condition

goal-set =
    working towards: { $condition }. Nothing runs until you send something; from then on each
    turn is judged against it. Ctrl-c takes it off, and so does leaving
goal-replaced = the goal that was set has been replaced
goal-cleared = the goal is cleared
goal-none =
    no goal is set. /goal <condition> sets one, as in /goal cargo test exits 0, and /goal clear
    takes it off again
goal-active = working towards: { $condition }
goal-last-check = the last check said: { $reason }
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
goal-failed = the goal could not be checked ({ $problem }), so it is cleared
goal-uninterruptible =
    the check already in flight is one request and cannot be stopped part way, but nothing
    more will be sent
goal-ended-unexpectedly = the goal check ended unexpectedly
goal-replaces-loop =
    the loop that was running has been stopped: a session works towards one thing at a time


## Being told when a file changes

watch-armed =
    watch { $number } is on { $path }: you will be told when it looks written to, with no turn
    running. /watch lists them, /watch stop { $number } ends this one, and ctrl-c ends them all
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
    a watch on { $path } was asked for and not armed: that path cannot be looked at, so there is
    nothing for a later look to be compared against
watch-fired = watch { $number }: { $path } looks written to
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
checkouts-no-such = this session keeps no checkout { $id }. /checkouts lists the ones it keeps
checkouts-command-takes =
    /checkouts lists the checkouts this session keeps, and /checkouts remove <n> removes the one
    with that number
checkouts-removed = checkout { $id } at { $path } is removed
checkouts-not-removed = checkout { $id } at { $path } could not be removed, and is still kept
checkouts-worked-from =
    checkout { $id } at { $path } is kept, since the working directory or a directory added with /add-dir is inside it
checkouts-kept = checkout { $id } is kept
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

## Pasting, dropping and attaching

paste-arrived-empty =
    that paste arrived empty: the terminal hands over text only, so a picture needs { $chord }
paste-not-a-command = a picture is not a command: leave shell mode to paste one
paste-not-with-a-command =
    a picture does not go with that command: send it in a prompt for it to be seen
paste-with-the-first-tick =
    that picture goes with the first tick of this loop; the ones after it say it was pasted
paste-too-large = that picture is { $size }, and a paste carries at most { $limit }
paste-nothing-on-clipboard = there is nothing on the clipboard to paste
return-not-pressed =
    that return arrived with other keys, so it was not a press: press Enter to send this line, or Escape to clear it
leave-not-pressed =
    that arrived with other keys, so it was not a press: press it again to leave
paste-folded = { $lines ->
    [one] [Pasted text #{ $number } +{ $lines } line]
   *[other] [Pasted text #{ $number } +{ $lines } lines]
    }
kilobytes = { $size } KB
megabytes = { $size } MB
gigabytes = { $size } GB


## Running a command the person typed

command-thread-stopped = the command's thread stopped unexpectedly
command-reported-a-failure = the command reported a failure


## Shortening a long conversation

compact-uninterruptible = summarising cannot be interrupted; it takes one request
compact-ended-unexpectedly = the summary ended unexpectedly
compact-done =
    summarised { $summarised } earlier messages, keeping the last { $kept } as they are
compact-nothing-to-do = there is nothing to summarise yet
compact-failed = the conversation could not be summarised: { $problem }
turn-ended-unexpectedly = the turn ended unexpectedly


## Asking something beside the work

btw-needs-a-question = /btw takes the question to ask, which the conversation will not read
btw-uninterruptible = the question cannot be interrupted; it takes one request
btw-ended-unexpectedly = the question ended unexpectedly
btw-failed = the question could not be answered: { $problem }

# What the session says about a manifest run started from it. The plan, each step and the reply are
# shown as they happen, so what is left to say is that a run is starting, where it was written down,
# and what went wrong where something did. That a run is not a turn of the conversation is what the
# mode is for rather than news about this run, so it is not said here.
manifest-needs-a-task = /manifest takes the task to plan, as in /manifest summarise the docs
manifest-began = planning the whole task first; the session waits here until the run ends
manifest-ended-unexpectedly = the run ended unexpectedly
manifest-failed = the run stopped: { $problem }
manifest-recorded = recorded as { $id }; read it again with bravebot --resume { $id }

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
# Said for /model in a session started with --agent under a definition that names a model.
session-model-is-the-definitions =
    every turn is addressed to { $definition }, which asks for { $model }, so /model has nothing to
    change; start bravebot without --agent to pick a model
# Said where --model and a definition that names a model are both in force. The definition's name
# and its model are the words of a vouched-for file.
agent-model-outranked =
    { $definition } asked for { $model }, and --model outranks it, so this run asked for the model
    the command line named
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
opening-invitation = Ask a question about this workspace.


## What a turn did, in the words a transcript line begins with

# One per tool. A word rather than the tool's own name, because a person reads the line:
# "Read(src/main.rs)" says what happened and "read_file" says what was typed.
verb-read-file = Read
verb-list-files = List
verb-search = Search
# The history of a repository, read without starting git.
verb-read-git = History
# A question put to a language server rather than to the files: "Look up" reads as asking
# something that knows the code, where "Search" reads as looking through it.
verb-lsp = Look up
verb-write-file = Write
verb-edit-file = Update
verb-todo-write = Plan
# Named for what it is rather than for what it does: every one of these is a model with no
# tools, no memory and one round, and a person watching a line go by should not have to
# remember which of the verbs meant that.
verb-spawn-processor = Isolated processor
verb-load-skill = Skill
verb-ask-user = Ask
verb-run = Run
verb-read-output = Read output
verb-vet-content = Vet
verb-fetch-url = Fetch
verb-job-output = Job
verb-spawn-agent = Delegate
verb-schedule-next = Schedule
verb-watch-file = Watch
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
reach-not-the-planner = not in the planner's context; a processor can be sent to read it
reach-no-model = in no model's context: nothing can be sent to read this

# How many calls a delegate has made, where its block shows only the last few.
delegate-more-calls = { $count } calls so far
# The definition's name and the model as its file wrote it, both from a vouched-for file.
delegate-model-needs-sign-in =
    { $definition } asked for { $model }, which needs a sign-in first, so it did not run
# The endpoint substitutes a model it will not serve rather than refusing. The name it answered
# with is left out, because a notice is the driver's own words.
delegate-model-substituted = { $definition } asked for { $model } and was answered by a different model
# A definition's skills line named skills this session did not find. The definition is its file's
# path and the skills are that file's own words, joined with a comma, both from a vouched-for file.
delegate-skills-not-found =
    { $count ->
        [one] { $definition } names a skill this session did not find, so its delegate is offered without it: { $skills }
       *[other] { $definition } names skills this session did not find, so its delegate is offered without them: { $skills }
    }
# A definition's mcpServers line named servers this session did not reach. The definition is its
# file's path and the servers are that file's own words, joined with a comma, both from a
# vouched-for file. "MCP" is a protocol's name and stays as it is.
delegate-servers-not-found =
    { $count ->
        [one] { $definition } names an MCP server this session did not reach, so its delegate runs without it: { $servers }
       *[other] { $definition } names MCP servers this session did not reach, so its delegate runs without them: { $servers }
    }
# A definition's mcpServers line declared a server inline rather than naming one, so its delegate
# calls no MCP server. The definition is its file's path. Nothing from the line is shown, since an
# inline entry can hold a command line and the value of a secret. "MCP", "mcpServers" and the path
# stay as they are.
delegate-servers-declared = { $definition } declares an MCP server in its mcpServers line, which only ~/.bravebot/mcp.json may do, so its delegate calls no MCP server
# A definition's rounds line is not a whole number above zero, so the file did not load. The
# definition is its file's path.
delegate-rounds-not-a-count = { $definition } was skipped: its rounds must be a whole number above zero
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
# A skill was loaded and asks the rest of the turn of a model of its own, over whatever the session
# was running. Said because a switch nobody is told about is the person's money spent on a choice
# they did not make.
skill-asks-a-model = { $skill } asks the rest of this turn of { $model }
skill-asks-an-effort = { $skill } asks the rest of this turn at { $effort } effort
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
