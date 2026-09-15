# Credential discovery: the design

What Brave builds to find the credentials already sitting in a working tree, in what order, and what
the user has to do.

The directory a person starts bravebot in may already hold their credentials. Nobody put them
through a gate and nobody recorded what they reach, and the moment the agent is pointed at that
directory they are readable by whatever it does next.

**We build the detector in [Brave guardrails](https://github.com/bbondy/guardrails) and vendor the
rule set from [betterleaks](https://github.com/betterleaks/betterleaks).** Guardrails has the half
that is hard and already done. Betterleaks has the half nobody should write twice.

**Patterns change weekly. Engines do not.** A rotated prefix, a new service, a format that gains a
checksum: all rule changes, and the rules are maintained by people whose job that is. The machinery
that runs them changes rarely and has to live in our process.

**So the dependency we take is a file, not a project.** A pinned TOML, updated as a reviewed diff.
No Go binary shelled out to per platform, and no other scanner's unpublished crates dragging a
vendored C++ regex engine into our build. What the engine needs from outside is a regex crate, a
multi-pattern matcher, a directory walker, a hasher and a TOML parser.

## What this design covers

**In scope.** The directory bravebot was started in, scanned for credentials before the trust
question is put; any directory added later, scanned as it is added; what a turn writes to the tree,
scanned before the change is recorded as complete; and what a command prints, scanned before it
reaches the transcript. Each is a moment where a credential can still be caught before it lands
somewhere it cannot be recalled from.

**What it is for.** A credential the scan finds is at Held and was never walked through a gate.
Showing it to the person is how it enters the flow. Most findings will not move a tier, and that is
a success: knowing a credential is there, and keeping it out of the context, is worth more than a
tier nobody could reach.

## The tiers this refers to

The spec ranks how a credential may be held. The four tiers and three gates are restated here
because the dispositions below are a walk through them.

| tier | what the account holds |
| --- | --- |
| Delegated | nothing. A performer outside the account decides each use and can refuse |
| Granted | a bounded derivative the issuer minted, enforced where the account cannot reach |
| Held briefly | a bearer secret the issuer mints on demand and can kill, unrefreshable from inside |
| Held | a bearer secret, held indefinitely |

Start at Delegated and walk down. **Gate 1** asks whether a performer outside the account can decide
each use and refuse; **gate 2**, whether the issuer can mint a bounded derivative enforced where the
account cannot reach; **gate 3**, whether the issuer can mint on demand and enforce death. Failing
one drops exactly one tier, and nothing climbs back without the arrangement changing.

**Everything this scan finds starts at Held**, because a file on disk is a bearer secret nobody
bounded. That is the floor the dispositions climb from.

## What betterleaks already gives us

| what we take | what it is |
| --- | --- |
| the rules | a TOML set of provider patterns with id, description, keywords and regex |
| keywords per rule | the strings that make a single-pass prefilter possible, already written by whoever wrote the rule |
| a confidence level, where a rule declares one | an optional `low`, `medium` or `high` on the rule, rather than a level inferred from which layer fired |
| regexes that will mostly compile | it matches with re2, the same class of engine as Rust's `regex` crate: no lookahead, no backreferences. Not the same feature set, which is why the loader reports what it could not compile |
| the gitleaks lineage | the same core fields the ecosystem already writes rules in, extended rather than replaced, so a rule we add is a rule anyone can read |

## What we do not want from betterleaks

| what we refuse | why |
| --- | --- |
| the binary | a Go executable shipped per platform is a worse problem than the one it solves, and guardrails is in-process Rust |
| validation | calling a provider to check whether a credential is live is a use of somebody's credential, in their audit log, by a tool run to find out whether it was safe |
| rule-set tracking | pinned, not followed. A rule change is a diff somebody reviewed, not a silent change in what the scan refuses |

## What is missing in betterleaks

| gap | what we build |
| --- | --- |
| a filter runtime | the rules carry filter expressions and the evaluator is Expr, a Go expression language. We inherit the expressions and write our own interpreter. It is not small: upstream, filters replaced allowlists, entropy and token efficiency alike, so the builtins it needs are path and content matching, entropy, and token efficiency |
| a confidence on most rules | `confidence` is optional, and the gitleaks rules the set inherits declare none. A rule that declares nothing is reported at no level rather than a guessed one |
| the rows of our own tree | a session fixture, an agent instruction file, an in-repo MCP config. Same format, same file, reviewed the same way |
| rarity scoring | token efficiency over BPE rather than Shannon entropy. Upstream a filter builtin, described rather than shipped as a library, so we implement it |
| anything about disposition | betterleaks reports and stops. A person's answer, its persistence and its expiry have no upstream |

## What Brave guardrails already gives us

| our requirement | what exists |
| --- | --- |
| a place for this to live, in our language | a Rust CLI we maintain, already built, shipped and installable |
| interception of a command's output | it wraps a command, buffers both streams, and decides before a byte is forwarded |
| a blocking verdict, not a warning | an unsafe verdict exits 42 and forwards nothing |
| a filtering verdict as well | `filter` emits sanitised output where blocking would be too blunt, though it still exits 42 on a detection |
| a pluggable detector | the checker is selected and configured, rather than compiled in |
| terminal-shaped output handled | `--pty` preserves formatting that a naive buffer destroys |

**The interception half is the genuinely hard part, and it is done.** Buffering two streams without
deadlocking, preserving exit status, handling a pseudo-terminal, deciding before anything is
forwarded: that mechanism exists and it is ours.

## What is missing in Brave guardrails

| gap | why it matters here |
| --- | --- |
| a deterministic local detector | every checker it ships is a model, and asking a model whether bytes are a credential sends the credential to the model |
| the rules themselves | there is no pattern set, because prompt injection is judged rather than matched |
| directory traversal | it inspects what a command emitted. Nothing walks a tree, resolves a symlink, or reads git objects |
| any sight of what a command wrote | it sees two streams. A file a command created is invisible to it, and nothing tracks which paths a turn touched |
| a finding as a record | a verdict is safe or unsafe. A credential needs a location, a fingerprint, an author and a disposition that outlives the run |
| somewhere to put findings | a verdict is consumed and discarded. Findings accumulate, dedupe and carry a person's answer forward |

**The checker is the load-bearing gap.** Everything else is addition; this is a substitution. A
model checker is right for prompt injection, where the thing judged is not confidential and no
pattern describes it. Credentials are the opposite on both counts.

## Architecture

```
┌─ the directory, at startup, before the trust question ───────┐
│  tracked, untracked, ignored, submodules, vendored           │
│  read only. Nothing the scan produces is written back here   │
└──────────┬───────────────────────────────────────────────────┘
           │ read, and never transmitted anywhere
┌──────────▼─ the agent's account ─────────────────────────────┐
│  guardrails, with the rule-matching checker                  │
│  no network call, no hosted model, no provider check         │
└──────────┬──────────────────────┬────────────────────────────┘
           │ findings             │ a question
           ▼                      ▼
   store, outside the tree    the person
   and outside the prompt     answers a disposition
```

**Three properties hold that picture together.** Nothing read leaves the account. Nothing written
lands in the tree. The planner is on none of these paths: it sees neither the findings nor the
values.

Six pieces, in build order. None is a new algorithm, and only the last does not exist in some form
already.

| piece | what it does | what it rests on |
| --- | --- | --- |
| rule loader | parses the pinned TOML into rules: id, description, keywords, regex, confidence. Ignores `validate`, defers `filter` and `components` | a TOML parser |
| keyword prefilter | every rule's keywords go into one automaton. One pass over a file narrows hundreds of rules to the few that could match | a multi-pattern matcher |
| regex stage | compiles each pattern once at load, runs only the candidates the prefilter named | a finite-automata regex crate, which makes hanging impossible rather than unlikely |
| traversal | walks the tree, inverts the ignore default because ignored files are in scope, applies the skip list and size caps, resolves symlinks without following them | a directory walker |
| findings | builds the record below, hashes under a per-machine salt, writes outside the tree | a hasher |
| touched-path tracking | records which paths a wrapped command changed, so a command's writes are scanned like a tool's | nothing yet. See Decisions |

**It is a checker beside the model ones, not a replacement.** Guardrails already selects a checker,
so this fills an existing slot, and nothing above it knows how matching happens.

**Matching runs over whole file content, not line by line.** The upstream rules span newlines: the
private-key rule runs from the BEGIN marker to the END marker, and several others match across a
handful of lines. A line-at-a-time scanner silently finds none of them. The finding records the line
the match starts on.

**Compilation failures are reported, never silent.** re2 and Rust's regex crate are the same class
of engine but not the same feature set, so some rules will not compile. Each is named at load with
the reason, and the scan reports how many rules loaded against how many the file contains. A scan
that gets quieter over time because rules stopped compiling is worse than one nobody built.

**Filters are deferred, and the cost is noise.** The filter expressions are what keep an example
file and a documented sample key from being reported, so layers 1 and 2 ship noisier than this
document otherwise implies.

## How the scan works

Where it reads, in one pass before the trust question.

| surface | included | why |
| --- | --- | --- |
| tracked files | yes | the ordinary case |
| untracked and ignored files | yes | `.gitignore` hides a file from git, not from the planner |
| submodules and vendored trees | yes | vouching covers them |
| git objects, packfiles, reflog, stashes | deep mode only | expensive, and a deleted secret is still a live secret |
| symlink targets outside the tree | resolved, not followed | a link into `~/.aws` is a finding about the link |
| files above a size cap | header and tail only | dumps and bundles, where a full read is the larger risk |
| dependency directories | skipped by default | `node_modules`, `venv`: the bulk of a large tree, and nobody here wrote it |
| build output | scanned, not skipped | `dist` and `target` hold what this repository produced, and a key baked into a bundle at build time is in the tree and about to ship |

**The scan has a time budget, because it blocks the trust question.** Two seconds by default. It
runs against that deadline and reports what it covered. Exceeding the budget gives a partial result
that says how partial, never a silent stop and never an unbounded wait. Stopping at three per cent
and stopping at ninety-seven are different answers, and a tree too large to finish is a finding in
itself.

What it matches with, in three layers.

| layer | what it matches | catches | phase |
| --- | --- | --- | --- |
| 1. path | filename and location rules from the tree inventory | `.env`, `*.pem`, `storageState.json`, `terraform.tfstate` | 1 |
| 2. structure | provider patterns, with checksums where they exist | `sk_live_`, `xox*`, `ghp_`, `AKIA`, PEM headers, JWT shape | 1 |
| 3. rarity | strings improbable as language, rather than merely high-entropy | unprefixed keys, generated app secrets | 5 |

## The shape of a finding

| field | contents |
| --- | --- |
| type | the tree-inventory row it matched |
| path and line | location only |
| fingerprint | a hash of the value under a per-machine salt, so findings dedupe without the value and the hash is useless off this machine. The salt is not stored beside the findings |
| preview | masked, fixed width, never a prefix or suffix of the real value |
| author | `person`, `turn`, or `unknown`, from blame and the turn record |
| confidence | the level the matching rule declares, where it declares one. A rule that declares none reports none, and layer 3 matches have no rule behind them and are advisory |
| disposition | what the person chose, carried to the next scan |

**Findings are stored, not only shown.** A finding that vanishes with the session is re-decided
every run. The disposition is part of the finding, so the next scan matches the same fingerprint and
does not ask again.

**The finding must not become the leak.** It is written outside the tree, kept out of the prompt,
and shown through a path the planner does not read. A scan that reports values into the transcript
has moved a Held credential into a second Held location and called it a feature.

**Attribution decides the response, and it is a fact for a tool and an inference for a command.** A
file the turn's write or edit tools produced is attributed to that turn, because the scan sees what
the tool was asked to write, and that is the case the spec forbids outright. A file that merely
appeared while a wrapped command ran is attributed on whatever evidence the tracking mechanism
provides; where that cannot separate the command's work from an edit in another window, the finding
is `unknown`. A `person` author is a fact about the repository, shown and not enforced. `unknown` is
a real answer, not a gap: an untracked `.env` has no blame and no turn record, and guessing there
turns a routine finding into a false accusation.

## Disposition is a gate walk, run upward

| disposition | gate it attempts | resulting tier |
| --- | --- | --- |
| Remove | none | no custody in this run, and nothing revoked |
| Deny the path for the session | none | unchanged |
| Enrol with the authority | gate 1 | Delegated, if a performer exists for the operation |
| Ask the issuer for a scoped grant | gate 2 | Granted, and usually refused by the counterparty |
| Rotate, then mint per run | gate 3 | Held briefly |
| Accept | none | Held, and gate 3's drop cost is owed |

**No disposition moves a tier because the scan ran.** Finding a credential and showing it is not a
gate.

**Deny is the default, and it is a tool-boundary block rather than confinement.** Most findings are
a file the task never needed to read, so denying the path keeps the value out of the context while
the work proceeds. It is refused by read, edit, write and search, which is every route the planner
can name a file by. A command the turn runs is not one of those routes: it runs as the person and
can read anything the person can, and what holds there is the output scan, not a read block.

**Only Enrol reaches Delegated**, and only where a performer exists for the operation. A credential
enrolled with nothing to use it is a Held credential in a better cupboard.

**Remove is not revocation.** Deleting the file ends this run's custody and nothing else: the
credential is still live at the issuer, still in the history, still in whatever copied it. It is
offered as rotate-then-delete, and a Remove without a rotation is recorded as the partial answer it
is.

**Accept must expire.** An acceptance with no expiry is a finding that was deleted slowly.

## The first run

A real repository answers the first scan with hundreds of findings, and a person asked hundreds of
questions before their first turn answers none of them. This is how credential scanning gets
switched off.

**Findings are ranked, and the tail is not a question.** High confidence and a path rule first,
layer 3 last. What clears the bar is put to the person one at a time; the rest is counted, stored
and readable on request.

**One answer can cover many findings.** A disposition applies to a fingerprint, a path, or a
directory, so an unreviewed vendored tree is one answer rather than four hundred. The breadth is
part of the record, and a directory-wide deny does not silently cover a file added later.

**The default is deny the tail.** Anything not asked about is denied for the session rather than
accepted, because an unread answer must fail towards the value staying out of the context.

## Output

Two artifacts, both outside the tree: a snapshot of what was scanned, which layers ran, how many
rules loaded and what coverage was reached; and the findings, appended and deduped by fingerprint.
**Neither goes in the tree**, for the reason the finding record gives above.

**The findings store is also the allowlist**, so there is no second file for one. An acceptance is a
finding with an acceptance on it.

**It is per machine, and that is the trade.** A colleague cloning the repository is asked about the
same findings. Sharing would need a fingerprint that matches across machines, which means a salt in
the repository and a hash of a secret anybody can attack offline, and a file in the tree listing
where the accepted credentials are, which is the map this section refuses. Revisit only if somebody
answers both.

## Build order

The ordering rule: **what can only be shown lands before what can be enforced**, because showing
needs nothing that does not exist and enforcing needs a mechanism we have to build.

| phase | lands | buys |
| --- | --- | --- |
| 1 | the rule loader with compile reporting; prefilter and regex stage; traversal; layers 1 and 2; findings stored outside the tree and carried between runs; ranking and one-answer-covers-many; deny and accept | the common case. A path rule and a provider pattern find most of what is there, and an answer given once is not asked again |
| 2 | the Expr interpreter and the filter subset the noisy rules turn out to need, without the HTTP builtins; expiry on an acceptance; drift detection on a fingerprint | the noise comes down, an acceptance stops being permanent, and an edited secret stops being an accepted one |
| 3 | the scan on what the turn's write and edit tools produced | the enforceable half, on the routes where attribution is a fact |
| 4 | touched-path tracking, and the same scan on what a wrapped command wrote | the most ordinary way an agent creates a file stops being the one route with no check on it |
| 5 | layer 3, rarity, threshold tuned against a measured false-positive rate; one level of base64 and JSON-string decoding before matching | generated app secrets, unprefixed keys, and the secrets that are only a decode away |
| 6 | the same checker on command output | a credential a turn printed never reaches the transcript |
| 7 | deep history mode, off by default | deleted secrets that are still live |
| 8 | the enrol disposition | a finding that can reach Delegated, once a performer exists |

Phase 1 carries persistence because accept without it is a button that does nothing. Phases 3 and 4
split what reads like one step, because tool writes need no new mechanism and command writes need
the one thing here that has to be invented. Phase 5 is cheap once phase 2 lands, because upstream
rarity is a filter builtin and the interpreter is already there. Phase 8 is the only one that waits
on an authority existing, which is credential brokering's phase 3.

## Decisions

**Vendor the rules, write the engine.** The rules are data that changes weekly and the engine is
code that changes rarely, so only one of them is worth taking from outside. Betterleaks is written
by the gitleaks maintainers, the original author among them, and it carries that rule lineage
forward: this is the set the ecosystem has been correcting for years, not a new one. It is also
young, created February, with four maintainers and Aikido Security's sponsorship behind an
independent project. Young argues for carrying a pinned copy ourselves. It does not argue against
starting from their rules.

**No third-party engine, in either form it is offered.** Not a Go binary per platform, and not
another Rust scanner's unpublished crates behind a git reference with a vendored C++ regex engine.
Both trade a week of assembly for a permanent build and supply-chain cost.

**No validation, and no flag for it.** Checking whether a credential is live is a use of it, in
somebody else's audit log. Upstream this runs on the same Expr runtime as the filters, so once we
build that interpreter validation costs almost nothing to add, which is exactly when somebody adds
it. **The interpreter omits the HTTP builtins.** A rule carrying `validate` loads and that clause is
ignored, so the refusal is a thing the code cannot do rather than a flag nobody turned on.

**How a command's writes are seen is the one open mechanism.** Guardrails sees two streams, not the
filesystem. An mtime-and-size delta after the command exits is cheap, needs no new privilege, and
cannot separate the command's work from a concurrent edit. OS-level write tracking is exact and is
three platform implementations. **Take the mtime delta**, attributing anything it reports as
`unknown` unless a tool write already claimed it. That is why phase 4 is separate from phase 3, and
why attribution above refuses to call a command's writes certain.

## What this does not cover

| not covered | why |
| --- | --- |
| the rest of the account: a home directory, a tool's cache, a browser profile | the tree is what gets vouched for and disclosed. The rest is reachable by anything running as the person, and bounding it is confinement rather than detection |
| anything found after the session has started reading | the first read is the disclosure. A scan that runs later reports something that already happened |
| credentials with no pattern and no shape | nothing matches them, which is why a clean result is not a clearance |

## What this does not fix

**A clean result is not a clearance, even over what was scanned.** Layer 3 is advisory, deep history
is off by default, and a partial result covers less than the tree. Silence means nothing matched.

**The scanner can be lied to.** It runs in the account the attacker is assumed to hold, so what it
reports is what that account chose to show it. It defends against the person not knowing what is in
their own repository, which is real and common, and not against the adversary this system is written
against.

**A denied path is not custody.** The value is still Held, still in the file, still readable by
anything else running as the person.

**Removing a finding does not end the credential.** Until the issuer is asked to kill it, Remove is
housekeeping.

**We now own the rule format.** Every pin bump is loader work when the upstream format moves, paid
in small amounts forever rather than once.

## Tests, and the phase each one gates

**Phase 1**

1. A finding never contains the credential value, in the store, in the prompt, or in anything the
   planner reads.
2. The scan completes and its findings are shown before the trust question is put, and no file
   content reaches the planner first.
3. No layer makes a network call, and no tree content leaves the account.
4. A rule that fails to compile is named with a reason, and the loaded count against the file count
   appears in what the scan reports.
5. A denied path is refused by every tool that names a file: read, edit, write and search.
6. A credential a person put in the tree is reported and not refused.
7. A tree that cannot be scanned inside the budget produces a partial result that says what coverage
   it reached, and the trust question is still put.
8. A directory added mid-session is scanned as it is added, before anything in it is read.
9. A turn cannot add, edit or remove a disposition.
10. No file the scan writes lands in the tree.
11. A finding that has been shown, and a disposition that attempts no gate, leave the tier
    unchanged.
12. A private key spanning many lines is found, and a rule matching across lines is not defeated by
    the file being read in pieces.
13. A rule declaring no confidence produces a finding with no confidence, not a guessed one.
14. Findings past the ranking cut are denied for the session, counted, and stored, and none is
    silently accepted.
15. A disposition given for a directory does not cover a file added to it afterwards.

**Phase 2**

16. A fingerprint that no longer matches produces a new finding rather than a renewed acceptance.
17. An acceptance past its expiry produces a finding again.
18. A rule with a filter the evaluator does not support is reported as unfiltered rather than
    silently dropped.
19. A rule carrying a `validate` clause loads, and nothing in the interpreter can make a network
    call.

**Phase 3**

20. A credential the turn's write or edit tools produced is attributed to that turn and does not
    remain in the tree.
21. A file changed by something other than the turn's tools is not attributed to the turn.

**Phase 4**

22. A credential written by a command the turn ran is found before the turn is recorded as complete.
23. A file changed in another window while a wrapped command ran is reported as `unknown`, never as
    `turn`.

**Phase 5**

24. Layer 3 matches are marked advisory and cannot by themselves refuse anything.
25. A credential that is only a base64 or JSON-string decode away is found, and decoding stops at
    one level.

**Phase 6**

26. A credential in a command's output never reaches the transcript or the planner.
27. Where nothing is found, the wrapped command's exit status and output are preserved unchanged.
28. A blocked command is distinguishable from a command that failed on its own.

**Phase 7**

29. Deep history mode finds a credential that was committed and later deleted, and is off unless
    asked for.

**Phase 8**

30. Enrolling moves a finding to Delegated only where a performer exists, and otherwise reports that
    it did not.
