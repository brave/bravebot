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
    --dangerously-skip-permissions and --settings go with it; everything else is another way of
    starting.

## How much a session asks before it acts, drawn under the input box
#
# The markers are Claude Code's, and deliberately: somebody who has used one of these knows what
# ⏵⏵ means at a glance, and inventing our own would make a familiar thing need reading. Asking has
# no line of its own, being what a session has always done.
mode-accept-edits = ⏵ accept edits on
mode-plan = ⏸ plan mode on
mode-bypass = ⏵⏵ bypass permissions on

cli-options-heading = Options:
cli-option-file = Include a workspace file as context (repeatable)
cli-option-add-dir = Reach into a directory outside the working one (repeatable)
cli-option-settings = Read this settings file for this run, above the ones found on disk
cli-option-mode = turn (default) decides step by step; manifest plans the whole run first
cli-option-model = The model this run asks for, in place of the remembered or configured one
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
cli-mode-needs-a-name = --mode requires one of { $names }
cli-model-needs-a-name = --model requires the name of a model
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
    Brave Leo Premium, if you already subscribe: run `bravebot import-leo-creds` on a machine where Brave is signed in to that subscription. It reaches models through Brave's AI gateway, which has open issues being worked on, so prefer one of the two above for now.
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
doctor-key-name = key
doctor-key = { $key } (never transmitted)
# What would end each credential this build holds for itself: who issued it, the surface that
# revokes it, and anything minted from it that revoking it would not reach. Written down here
# because the moment somebody needs it is the moment it is too late to work out, and because the
# disposition people reach for, deleting the local copy, ends this machine's custody and nothing
# else. One line per credential, and both AWS arrangements where an account is configured, since
# which one a profile resolves to is the AWS CLI's answer and this report does not run it. One
# per gateway a settings file configured too, so each names the host that would end its token.
doctor-ends = ends
doctor-ends-signing-key =
    the signing key: issued by the Brave backend, which derives its copy from a master seed and this key id; ended only by retiring that id there and shipping another build, since one build's key is every install's
doctor-ends-aws-access-key =
    a long-lived access key: issued by AWS IAM to the user the profile names; ended with `aws iam delete-access-key`
doctor-ends-aws-session =
    a session credential: issued by AWS STS for the profile, and this program asks the AWS CLI for another as it builds each request, so the expiry ends that copy rather than this program's access; ended at its issuer, since `aws sso logout` clears this machine's copy rather than the session behind it, and the next one is minted from whatever the profile chains to, for as long as that lasts
doctor-ends-gateway-token =
    a gateway bearer token: issued by { $gateway }, which is also the only surface that revokes it; deleting it from the settings file or unsetting the variable ends this machine's custody and leaves the token live there
doctor-ends-subscription-batch =
    an imported subscription's credential batch: minted by Brave's subscription service against the order this install registered as a device on; each credential is spent by one premium request and the batch stops working when its last window closes, and nothing revokes an unspent one, so `bravebot import-leo-creds --forget` ends this machine's custody and leaves the batch spendable by whatever copied the file
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
    gate { $gate }, { $answer }: it is not minted for one step, since the token is whatever the settings file carries or the variable holds, and it is held for the whole run
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
doctor-gateway-token-absent = none found (set a variable its `env` names)
doctor-gateway-token-not-needed = none needed (the block names none)
doctor-gateway-models-absent = none configured (the gateway is asked what it serves)
doctor-region = region
doctor-profile = profile
doctor-profile-absent = default credentials
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
# The machine-level layer, above everything a person can set. The names rather than the values, for
# the reason the settings lines give, and the path because a pin somebody wants lifted is lifted by
# whoever can write that file.
doctor-managed = managed
doctor-managed-pinned = { $names } from { $path }
# A file somebody wrote that holds nothing this layer may pin. Reported, because the alternative
# leaves them unable to tell it from a file that was never found.
doctor-managed-nothing = { $path }, pinning nothing
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
permission-rule-not-a-line = is not a rule; a rule is written as a line of text
permission-rule-empty = is empty
permission-rule-unclosed-bracket = is missing its closing bracket
permission-rule-unknown-family = names no family of tools this agent has; use Read, Edit or Bash
permission-rule-empty-brackets = has empty brackets; drop them to mean every use
permission-rule-unanchored = needs a home directory or a settings directory to say where it points
permission-rule-not-a-domain-rule = needs a domain, written WebFetch(domain:example.com)
permission-rule-no-domain-named = names no domain after 'domain:'


## Importing a Leo Premium subscription

leo-no-premium-endpoint =
    warning: this build has no premium endpoint, so imported credentials will not be used
leo-set-and-rebuild = set { $variable } and rebuild
leo-unknown-channel = unknown channel: { $channel }
leo-expected-channel = expected one of: stable, beta, nightly, development
leo-forgotten = forgot the imported subscription
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

# Said when a hook a person attached to a moment did not end well. Three sentences rather than one
# because what to do about each is different: a program that is not there is a path to fix, a
# non-zero status is the hook's own business, and one that was stopped was too slow to be run from
# a turn at all. Nothing a hook prints is read, so this is the whole of what can be said about it.
hook-not-started = the { $moment } hook `{ $program }` could not be started ({ $detail })
hook-failed = the { $moment } hook `{ $program }` did not end well ({ $status })
hook-stopped =
    the { $moment } hook `{ $program }` was still running after { $seconds } seconds and was
    stopped


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
write-yes = write it
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
    this runs the build tooling of its ecosystem, so code from your dependencies runs with
    your own access, the way cargo test does. it stays running for this session.
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
status-also-open = Also open
status-added-directory = added with /add-dir
status-scratch = Scratch
status-scratch-note = this session's own to write in, removed when it ends
status-model = Model
status-model-chosen = chosen with /model
status-model-default = the configured default
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
status-loop = Loop
status-loop-every = every { $every }
status-loop-self-paced = paced by each turn
status-loop-next = next in { $next }
status-loop-running = running now
status-loop-unpaced = waiting for the turn to say when
status-goal = Goal
status-watch = Watch { $number }
status-watch-armed-by = armed by turn { $turn } · { $left } left
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
status-trust = Trust
status-nothing-vouched-for = nothing vouched for
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
# Said while a confined check reads quarantined content, before any of it may be read. The count
# is what the check was given, which is the one thing that predicts how long it will take. Not a
# word about what it decided: that reaches a person on the prompt and nothing else.
indicator-checking = { $lines ->
    [one] Checking { $lines } line
   *[other] Checking { $lines } lines
    }
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
scroller-key-half-page = half page
scroller-key-full-page = full page   (also ctrl-f / ctrl-b)
scroller-key-ends = top / bottom   (also home / end)
scroller-key-prompts = previous / next prompt
scroller-key-search = search, next/previous match
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
command-compact = Summarise the conversation so far, keeping the recent part
command-btw = Ask something beside the work, without putting it in the conversation
command-clear = Start a new session here, keeping this one resumable
command-loop = Send a prompt again and again, say what is repeating, or stop it
command-goal = Keep working until a condition you set is judged met
command-watch = List the files this session is watching, and stop one by its number
command-manifest = Plan one task in full, show you the plan, then run it with nothing re-planned
command-export = Export the session transcript to a markdown file
command-undo = Rewind one turn and put back the files it wrote
command-rewind = List the turns a rewind could go back to, or go back that many
command-exit = Leave


## What the session says back

session-resumed = resumed session: { $title }
session-renamed = renamed to { $title }
session-rename-needs-a-name = /rename needs a name, as in /rename the parser bug
session-rename-needs-something = /rename needs a name with something in it
session-cleared = cleared: a new session, with the previous one still resumable
session-rewound = rewound the session to before turn { $turn }
session-rewound-partly =
    rewound the session to before turn { $turn }, but these files still hold what was
    written: { $paths }
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
session-not-trusting = this directory is not trusted; every write will be shown to you
session-vouched-for = trusting { $path } for this session
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
# is depends on how this copy was installed, so it is not a translator's to reword.
update-available =
    bravebot { $version } is out (this is { $running }); update with: { $command }
session-started-server = running the { $language } language server for this session ({ $program })
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
megabytes = { $size } MB


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
