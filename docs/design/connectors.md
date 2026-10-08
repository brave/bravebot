# Connectors: the design

**Status:** design. None of it is on `main`; until the #852 broker exists, brave-vault prototypes
the authority.

## Goals, adversary model and non-goals

**Goal.** Let bravebot read and write a person's accounts, starting with GitHub and Gmail, without
ever holding their credentials. Untrusted service content may shape data and generated text, but
cannot choose capabilities or arbitrary routing. Runtime-derived targets are permitted only through
typed authority-held receipts under rules fixed before the read. A separate credential holder makes
every call. A service the engine can already express is added by writing a definition, with no
service-specific code.

**Scope.** This design covers manifest mode, where a run's control flow, routing constraints and
releases are fixed before any service content is read, and normally the person approves the plan
first. Bypass skips that approval and nothing else: routing stays locked, reads still need the
authority's grant, and a write that releases runtime-derived content is authorised in the
authority's popup ([Releases](#releases)). The fixed plan is what keeps a fetched issue or email
from choosing steps, or destinations outside the rules the plan fixed. Connectors will be extended
to turn mode later. There the next call is chosen after each read, so that extension needs its own
case for the same guarantee; the authority and the definitions carry over unchanged.

**Adversary model.** The attackers are the authors of what a run reads at a service: strangers who
open issues, comment or send email, and authors with standing whose account is compromised or who
quote a stranger. Their text reaches a processor and the authority's popup, never the planner. The
planner may also write a wrong plan; the surface check holds a plan to the definition's surface, and
outside bypass the person approves it before anything is read. Out of scope until the #852 broker
replaces the prototype: programs running as the person, and a hostile definition file. Also out of
scope: a web search the model service runs during a planning call, which can shape the plan. Each of
these is listed in [Prototype status and gaps](#prototype-status-and-gaps).

**Non-goals.**

- Turn mode, for now (Scope).
- The calls [GitHub tasks](#github-tasks) and [Gmail tasks](#gmail-tasks) list as not offered, such
  as merge, deleting a branch, and deleting or trashing mail.
- Content choosing an act's option or the next step; the plan fixes both. A task whose act depends
  on what is read waits for the proposed `choose` step
  ([Data-dependent branches](#data-dependent-branches)).

Words used in a narrow sense are defined in [Terms](#terms).

## Security invariants

Each invariant is enforced in code, in the section linked. They hold against what a run reads and
what bravebot does; the prototype's gaps are in
[Prototype status and gaps](#prototype-status-and-gaps).

1. **Credential custody.** bravebot never receives a connector credential; the authority makes every
   authenticated service call ([Credentials](#credentials)).
2. **Planning isolation.** Service content fetched in a manifest run never reaches a planning call
   of that run ([Labels](#labels)).
3. **Typed routing.** Capabilities, places, releases and routing rules are fixed before service
   content is read; a target taken from a fetch enters routing only through the authority's receipt,
   as the definition declares ([Receipts and `on`](#receipts-and-on),
   [Values taken from the item](#values-taken-from-the-item)).
4. **Quarantine.** What a service sends back is labelled untrusted and private and stored unread in
   one quarantined slot ([Labels](#labels)).
5. **Target policy.** Every existing item an act will change is held to the target floor just before
   its popup, or the act is refused ([The standing floor](#the-standing-floor)).
6. **Release control.** Content from a slot leaves only to a destination a release names; in normal
   manifest mode plan approval authorises the release, and under bypass the authority's popup does
   ([Releases](#releases)).
7. **Write authorisation.** Every write is authorised in the authority's popup against the prepared
   request, and only that request is sent ([Writing](#writing)).
8. **Read authorisation.** A read whose service data is released from the authority needs a grant, a
   policy or a popup; authority-internal reads used only to prepare a pending write release nothing
   and are governed by the write protocol ([When the person is asked](#when-the-person-is-asked)).
9. **Definition identity.** bravebot's surface and the authority's wire are named by one
   `definition_id`, and a request naming another is refused
   ([Loading a definition](#loading-a-definition)).
10. **Input validation.** Every routing or bound value put into a service request is checked before
    it is inserted: a path value against its declared form on both sides, a query-language value for
    quotes and spaces, a header value for line breaks and controls
    ([The engine](connectors-reference.md#the-engine), [Forms](connectors-reference.md#forms)).

Four mechanisms do separate jobs:

- **IFC labels** keep a connector payload from choosing a step, a destination or what the planner
  sees.
- **Standing floors** limit whose text shapes what the run writes, and which items an act may land
  on.
- **Releases** limit where quarantined content may go.
- **Receipts** let an act land on an item a fetch chose, without a processor choosing routing.

Standing is read from a service's metadata and is not an IFC integrity label. A floor never changes
a label, a label never filters items, and sufficient standing does not make text trustworthy.

Together they give the design's claim: service content may shape data inside the frozen plan but
cannot widen what the plan may do. A runtime target enters routing only through a receipt the
authority holds, and the authority checks every write
against its own copy of the definition before sending it. This design combines
plan-then-execute, a quarantined reader and user confirmation, which *Design Patterns for Securing
LLM Agents against Prompt Injections* advises combining for an email assistant[^patterns].

## Architecture and trust boundaries

A connector gives bravebot access to one service. Two services so far:

- GitHub (issues, pull requests, files)
- Gmail (read, send, reply, label, archive)

Each is one JSON file, its definition, with a surface bravebot reads and a wire only the authority
reads ([Connector definition language](#connector-definition-language)). Each side checks a request
against its own copy and does not rely on the other side's reading of it.

bravebot plans the run, checks each step against the surface, locks where each step reads and
writes, and keeps what comes back in a quarantined slot that no planning call sees. The vault holds
the credential, makes every call from the wire, and asks the person before every write.

```
 bravebot                               authority: #852 broker (prototype: brave-vault)  service
 plan, lock, approve, slots  request ─▶ credential store, engine, popups  ── HTTPS ─▶ REST API
 holds no credential         ◀─ reply   reads its own copy of the definition
```

Everything specific to a service lives in its definition. bravebot and the vault only follow
whatever definition is installed. Provider names appear in their code only in a leak guard: before
any write, the engine refuses text that contains something shaped like a credential, such as a
GitHub token (`ghp_…`) or an AWS key (`AKIA…`) ([The engine](connectors-reference.md#the-engine)).

The authority is to be the credential broker of
[#852](https://github.com/brave/bravebot/issues/852), which is not built yet. Until it is,
brave-vault, an experimental desktop app, plays that role, and bravebot's side stays unmerged
([Prototype status and gaps](#prototype-status-and-gaps)).

### Compared with MCP servers

bravebot can already read GitHub and Gmail in a turn through each service's MCP server
([GitHub](../website/docs/customize/mcp/github.md),
[Gmail](../website/docs/customize/mcp/gmail.md)). Connectors differ mainly in who holds the
credential and who may write.

| | existing: MCP servers | ours: connectors |
| --- | --- | --- |
| mode | a turn; the planner picks each call as it goes | manifest mode today, where the plan fixes every place before anything is read; turn mode later |
| OAuth client | GitHub: none, a pasted fine-grained token. Gmail: Google's Gemini CLI client, through a Google-run service that holds its secret and sees each refresh | the person's own OAuth App (GitHub) or Desktop client (Gmail), with PKCE on a loopback redirect |
| token scope | read-only: chosen repositories (GitHub), `gmail.readonly` | read and write: `repo`, every repository the person can reach (GitHub); `gmail.modify` (Gmail) |
| token stored | GitHub: `~/.bravebot/mcp.json`. Gmail: the server's directory, encrypted with a key kept beside it | in the authority; bravebot never receives it |
| makes the call | the confined server bravebot starts | the authority |
| writes | none: write tools are off and denied | each one approved in the authority's popup, against the exact text and target |
| what is read | quarantined; GitHub's lockdown mode shows public-repository content only from authors with push access, best effort | quarantined, and held to the standing floor the person sets per place |

### Relation to credential-brokering.md

This design needs [credential-brokering.md](credential-brokering.md)'s broker, account boundary,
peer verification and phase 1 hardening. It replaces that design's phase 3 mail plan (`send_mail`,
envelope equality, a per-task byte cap) with Gmail's `send` and `reply` acts and a per-write byte
cap, and reads mail now, with `gmail.modify` under the floor and a read grant, where that design
waits for phase 6 and provider-side scoping. `git push` by substitution stays phase 4.

## Manifest-mode protocol

### The two steps

In manifest mode a connector is used through two steps, `connector_fetch` and `connector_act`, rows
in the validator's contract table. Every argument is text.

| field | step | routing | meaning |
| --- | --- | --- | --- |
| `connector`, `resource` | both | yes | which definition, which resource |
| `place` | both | yes; optional on a fetch | where content comes from or lands |
| `item` | both | yes, optional | one thing in the place |
| `path` | act | yes, optional | where in the item the text lands |
| `select` | both | yes, optional | selector terms; on an act, the items it lands on |
| `option` | act | yes, optional | a choice from the definition's `values` |
| `on` | act | no, a slot reference | the slot of a fetch whose chosen item the act targets |
| `out_slot` | fetch | no | the slot the payload fills |
| `contents` or `from_slot` | act | no | the text; neither for a resource with `text: false` |
| `title` | act | no, optional | a subject or commit message, written in the plan |

- Optional routing is locked as empty and left off the plan line. An omitted field takes its
  `defaults` entry before validation, unless the other half of its `a|b` pair is given.
- An act's line carries the text and title the plan wrote (`sends: “…”`, `title: “…”`), cut short
  and escaped onto one line, or names the release for text a slot carries. Bound values come last.
- The surface check refuses, before approval: a field the resource does not use, or one it needs and
  lacks; a value outside its form, `values` or `select`; a bound value; text given to an act that
  takes none, or none to one that does; an `on` naming a fetch of another connector or place, or one
  with no receipt; a fetch in `on` that neither names its item nor uses one of the `narrow` terms,
  or of the option's `narrow_when` terms, an empty selector included; and an act's own `select`, or
  a fetch's, that uses none of its `narrow` terms.
- A plan the surface check refuses goes back to the fit call once, before anything is read or shown,
  with the check's own words ("'test' is not a key:value term"). A second refusal of any kind fails
  the run. Only this check asks again, which amends
  [MANIFEST-1](../specs/manifest.md#MANIFEST-1): today a plan that fails validation fails the run.
- The planner never corrects a routing value: an address or repository is copied from the task as
  written (`bob@gmial.com` stays `bob@gmial.com`), since a near-name may be another real recipient,
  owner or tenant. The plan line and the popup show every recipient.

### Calling the authority

In the prototype, bravebot starts the authority unconfined, as it starts the `aws` CLI for a Bedrock
credential, because it has to reach the vault and the person named it in their own definition. Its
environment is cleared to `HOME`, `XDG_RUNTIME_DIR`, `PATH`, `TMPDIR`, `LANG` and
`DBUS_SESSION_BUS_ADDRESS`, where set, and it is handed one request. Confinement
([sandboxing.md](../specs/sandboxing.md)) is for MCP servers and programs `run` starts.

Only an agent handle can make connector calls, and only while the vault is unlocked; a locked vault
answers `authority-refused`. When the vault app is not running, `brave-vault-agent` answers
`refused` with `authority-unavailable`. Before asking the person anything, the vault checks the run
id's form, the connector name, the credential item's form and the request's `definition_id`
([Loading a definition](#loading-a-definition)). A stop request, or bravebot's wait
running out, kills `brave-vault-agent`, and an act is reported as not known to have happened
([Prototype status and gaps](#prototype-status-and-gaps)).

### A run, end to end

The task: *"reply to the oldest open issue with no label in brave/bravebot, summarising what its
comments ask for."*

1. **Plan.** The planner writes three steps: a `connector_fetch` of `find_issue` in place
   `brave/bravebot` with select `state:open no:label sort:created-asc` into slot `issue`; a
   transform of `issue` into `summary`; and a `connector_act` of `comment` in the same place, `on`
   `issue`, from slot `summary`.
2. **Check and approval.** Defaults are filled in, every routing field is locked, and the surface
   check passes. `summary` comes from `issue`, which comes from `github brave/bravebot`, so the
   act's line ends "content from github brave/bravebot to github brave/bravebot".
3. **Fetch.** The vault checks the request and asks for the read grant. The engine lists the
   repository's issues with `state=open`, `sort=created`, `direction=asc` and `per_page=100`,
   applies `no:label` locally, drops pull requests and candidates below the target floor, and takes
   the first that remains. It reads that issue with its comments, projects, sanitises and bounds the
   fields, and replies with the payload, a receipt and the counts withheld.
4. **Store and process.** The payload goes into slot `issue`, and the step's line reads "fetched
   find_issue from brave/bravebot; 3 withheld, below-floor approved; standing sufficient". A
   processor writes `summary` from it.
5. **Act.** bravebot releases `summary` to `github brave/bravebot` and sends it with the receipt.
   The vault finds the receipt, applies the write rules, reads the issue again, holds it to the
   target floor and shows the popup. On Allow it sends the prepared request once and answers
   accepted with a link to the comment.

## Authorisation and receipts

### When the person is asked

| moment | asked | where | who decides |
| --- | --- | --- | --- |
| a run's plan | once, before anything runs | bravebot | bravebot: every manifest plan |
| first use of an account | once: the service's own sign-in page | the browser | the authority, when it holds no credential |
| a read (GitHub or Gmail) | once per connector, account and run; never if the person ticked "Read <name> without asking" for that agent and account | the authority's popup | the authority, from its grants and that setting |
| a write (GitHub or Gmail) | every write, against its exact text and target | the authority's popup | always asked today |

Bypass mode cannot answer the authority's popup, so reads and writes still ask under it, and no
release is authorised before the popup ([Releases](#releases)).

A connector act goes to the authority in every mode that runs the plan, bypass included; plan mode
refuses one, as an amended [MODE-3](../specs/permission-modes.md#MODE-3) would say.

- **Reads.** The vault uses a live grant for this run, else the handle's policy, else a popup that
  offers a time window and no use count. The grant's scope is
  `connector:<name>/<account>@run:<tag>`, the tag being the first 32 hex characters of an
  HMAC-SHA256 keyed by the run id over the connector name. The grant is saved in the vault file and
  listed in the Agents panel; the run id is not stored, so a later run asks again, and after a
  failed save the next read asks again. The handle policy that "Read <name> without asking" sets
  names `connector:<name>/<account>` and covers reads in every run of that connector and account,
  and no write. An act's preparation reads without the grant, since nothing it reads leaves the
  vault and the act's popup asks: the item again, the default branch, whether the branch exists,
  the file it replaces, and named lookups, each a `GET`.
- **Grants and releases.** A read grant lets service data leave the authority into bravebot's
  quarantine; a release lets slot content leave quarantine for a later act's destination. They are
  separate boundaries, and neither implies the other.
- **A write's popup** shows, in order: a warning for a high-impact act ([Writing](#writing)); the
  action (the resource's `summary`), account and connector; each bound value, marked as taken from
  the item; the place and other routing values; the target; the option; the title, unless a bound
  value fills it; the release line; and the exact text or diff, invisible characters and controls
  escaped (U+2028, U+2029 and U+0085 too before the text, so no value draws a line). The target of
  an act on one existing item is `<item> by <author> (<level>): “<title>”`; an act on every match
  shows the count and lists the frozen targets, each id with a display line where the definition's
  `show` template can be filled. The method and path are
  not shown; the wording comes from the definition. An act with no stored credential opens the
  sign-in before its popup.
- **The window.** Popups are shown one at a time, each keyed to its request, with focus on Deny and
  Allow armed after a delay. An unanswered popup is withdrawn and the act refused `timed-out`.
  bravebot issues no grant after an act.
- **A set.** Each item of an act on a set gets its own popup, numbered ("2 of 5"). A Deny moves on;
  an `unknown` or timed-out popup stops the rest, reported as not tried.

Approval fatigue is the known weakness of per-action prompts[^dual-llm][^camel]. Reads do not prompt
per call, writes are few per run, the popup leads with the destination, and a write has no "always
allow". MCP leaves confirmation to the client[^mcp-elicitation] and, since 2026-07-28, has the
server integrity-protect a request for input that influences authorisation[^mrtr]; here the popup
belongs to the credential holder and authorises the prepared request.

### Receipts and `on`

A fetch whose resource has `receipt` replies with a CSPRNG-generated opaque receipt (≥128 bits) for
what it chose. An act whose `on` names that fetch's slot lands on the chosen item: bravebot sends
the receipt back, and the vault takes the item from its own record. The vault keeps, in memory, the
run, connector, place and `definition_id`, and per chosen item the reading resource, recorded
fields, title, author and level, taken for a resolve from the listed or expanded element. An element
without the item's id fails the fetch (`bad-definition`); a recorded field it lacks fails only an
act that binds it (`no-bound-field`). The receipt is refused in any other run, connector, place or
definition, and acts in the run may reuse it. Before the popup, the vault reads the item again and
holds it to the target floor. A receipt lets the frozen plan refer to what the authority chose
without letting fetched text supply the route.

### Values taken from the item

A reply goes to the sender of the message it answers. The plan cannot name that sender and must not
take it from text a processor saw, so the vault takes such values from its receipt
([Receipts and `on`](#receipts-and-on)).

- **Recording and binding.** A fetch's `receipt` lists fields read by path from the item it chose.
  An act's `bind` names each value it takes from them and how to shape it, and the surface's `bound`
  gives the same names words for the plan line. A missing `optional` value fills in as nothing, and
  a header left blank is left out.
- **Readings.** `mailbox` is the one address of a `From`-style header. `msg-id` is one bracketed
  `<left@right>` id; a malformed one refuses the act (`bad-bound-value`) even where optional. A
  header a receipt records as singular (`From`, `Subject`, `Message-ID`, `References`) and
  appears more than once refuses the act (`bad-bound-value`); none is chosen from duplicates.
  `msg-ids` is a `References`-style list: malformed ids are dropped, and past the cap the root and
  nearest parents are kept (no RFC sets it; Netnews keeps the first and last two[^rfc5537]).
- **At the plan.** The planner is told not to give a bound value, and the surface check refuses one.
  The plan line names what will be filled in ("the authority fills in, from the item 'mail' chose:
  recipient = the sender of the email it replies to; ...").
- **At the act.** The vault refuses a request that supplies a bound value in its routing or title.
  It shapes each value from its record, holds it to the form its `bind` names, if any, and shows it
  first in the popup. It refuses a missing required field, and a header that is a list or group or
  has a comment, a stray bracket or a line break.
- **Moved.** After an `on` act reads its item again, a bound value that differs from the receipt
  refuses the act (`moved`). This narrows the window for a review's commit without closing it: a
  push made while the popup is open still lands before the review.

An attacker who controls the item can make themselves the destination, which is what replying to
them means, and shape the words; the person still sees the recipient and the text. They cannot
choose another destination, add a recipient, open another header, redirect through `Reply-To` (a
reply is bound to `From`), or change a value after the plan is approved.

### Credentials

The vault holds one credential item per connected account: provider, account, client id and secret,
token endpoint, the refresh token (or, with `refresh: false`, the one access token), the requested
scopes and the hosts recorded at sign-in. Access tokens from a refresh are held in memory only.

- **Sign-in, on first use.** With no credential, the vault opens the service's authorize page,
  receives the code and its `state` on its `127.0.0.1` listener, checks `state`, exchanges the code
  with the PKCE verifier, asks the service which account signed in (`whoami`), and stores the
  refresh token. If that account is not the credential item's, the sign-in is refused and the
  tokens are dropped, not revoked. A token with a lifetime is refused under `refresh: false`, and
  one with no refresh token otherwise. bravebot takes no part.
- **Scopes.** A sign-in whose token answer's `scope` lacks a requested scope is refused, naming the
  missing scope, so unticking one on Google's consent page[^granular] fails the sign-in.
  `offline_access` is exempt, and an absent or empty `scope` is accepted. A refresh is not checked.
- **Host pinning.** The access token goes only to the `base` host over HTTPS on port 443, and to the
  `whoami` host for that one call at sign-in. A refresh goes only to a recorded token endpoint. The
  HTTP client follows no redirects. GitHub's tokens carry no audience, so host pinning stands in for
  the audience binding the MCP authorization spec[^mcp-auth] and the OAuth security BCP[^bcp] ask
  for.
- **Same client only.** A stored credential is used only if it came from the definition's client id.
- **Refresh.** A refresh uses the stored secret. A rotated refresh token is held in memory until
  saved. `invalid_grant`, or a code the definition's `sign_in_again` lists (GitHub's
  `bad_refresh_token`), drops the credential item, so the next call signs in again. The sign-in's
  access token is used only for `whoami`, so the first call refreshes. With `refresh: false` the one
  token never expires.
- **The vault file** is written atomically (a temporary file, fsync, rename, mode 0600). An existing
  file that fails to open is not overwritten, and every writer in the app holds one in-process lock.
- **Custody.** Claude Code's cloud sessions attach the token at a proxy outside the VM[^claude-web],
  Copilot cloud agent cannot run `git push` itself[^copilot], and Amazon Bedrock AgentCore keeps
  refresh tokens where the agent cannot read them[^agentcore]. An agent beside its token has had it
  stolen through a command injection in a branch name[^codex]. A token `gh` holds is outside the
  vault, and keeping it from the agent is the sandbox's job.
- **Revocation and tier.** Disconnect in the Agents panel removes only the vault's copy; the token
  ends at the service. The tier is Held while the vault runs as the person, Delegated once the
  broker runs as its own account.

| | GitHub | Gmail |
| --- | --- | --- |
| client | an OAuth App ([GitHub's OAuth App](#githubs-oauth-app)) | a Desktop app client; Google assumes such apps "cannot keep secrets"[^google-native] |
| scopes | `repo offline_access` | `gmail.modify` (read, send, label), the one mail scope, and `userinfo.email` for the account check[^google-scopes] |
| tokens | the access token lasts eight hours; each refresh issues a new refresh token that lapses after six months unused[^authorizing]; `offline_access` gets expiring tokens whatever the App's setting[^gh-expiring] | while the client is in Testing, refresh tokens expire after 7 days[^google-expiry] |
| revoked under | Settings, Applications, Authorized OAuth Apps | the Google account's third-party connections |

## Data flow and release policy

### Labels

- A fetch's payload is labelled by its capability, `connector:<name>`, whose output is untrusted and
  private (the first row of [LABEL-8](../specs/labels.md#LABEL-8), "what a capability observed"),
  and goes through the usual quarantine into one slot in memory. The audit trail records its slot id
  and label, not its text. The planner does not see it; a processor that reads it sends it to the
  model service the person configured ([processors.md](../specs/processors.md)).
- bravebot parses the reply's envelope, then labels the payload and the detail with
  `policy.observe(Connector(name))`. The detail may quote the service. It is drawn in the quarantine
  margin, for a fetch that was not accepted and for every act, and reaches no model (a new clause,
  MANIFEST-13).
- The fixed fields are the authority's: protocol version, outcome (`accepted`, `refused`,
  `not_connected`, `unknown`), reason code, counts withheld, receipt, standing and link. A new
  LABEL-8 row gives the outcome, standing, reason codes, counts and link, each with a fixed form;
  none reaches a model. The receipt is not checked and is only sent back to the authority. A reply
  that does not parse fails the step, citing serde's category, line and column and none of the text;
  another protocol version fails it naming the version. A reason code or floor name outside its form
  prints as `unreadable-code`.
- **Standing** (`sufficient` only if every item is at `vetted` or `approved`) goes in the step's
  note ("standing sufficient"), not the slot's label, and no gate reads it. Applying it would need a
  new LABEL-8 row that deals with a compromised account with standing and a maintainer quoting a
  stranger.

### Releases

A release says where content may go. This is safehouse's precommitted release, built on machinery
manifest mode already has and stated as a new clause, MANIFEST-12. There are two paths:

- **Normal manifest mode.** The person approves the plan, its release origins, destinations and
  routing. Approving the plan is the declassification: it authorises each release. It does not make
  the content read later trusted; it authorises a bounded flow that the run then enforces.
- **Bypass mode.** Nobody approves the plan, so no release is authorised in advance. Every write
  still goes through the authority's popup, which bypass cannot answer, so a write that releases
  runtime-derived content is authorised there, where the person first sees its release.

How a release is formed and enforced is the same on both paths:

- At validation, a pure function over the frozen plan gives each slot the places its content came
  from: `workspace` for a workspace read, the connector and place for a `connector_fetch`, and the
  union of its inputs for a processor's slot. Each act carrying a slot gets one release: these
  origins, to this connector and place.
- At plan approval, each act's line names its release ("content from workspace to github
  brave/bravebot"), and a `write_file` that carries a slot names "; content from <origins>" unless
  the only origin is the workspace.
- At run time, one gate, `declassify_into_place`, releases a slot only to a destination a release
  names for it.
- The popup repeats the release as "bravebot says the plan releases: content from … to …", with the
  origins bravebot sent; the vault writes the destination from the act's own connector and place,
  not the one bravebot sent.

Text in `contents` or `title` was written before any read and needs no release. Invariant Labs
showed an injected public issue leading an agent to copy private repository data into a public pull
request through GitHub's MCP server, and advised one repository per session[^toxic]. Here that flow
needs the person to approve a plan whose act line names the private origin and the public
destination.

### The standing floor

The engine scores every item before it reaches a payload. The levels and their order are the
engine's; which service fields put an item at each level is the definition's, under its `integrity`
key. A level is a service-metadata filter, not an IFC integrity label, and the settings file's
`trusted` list raises an account's standing without making its text IFC-trusted.

| level | put there by |
| --- | --- |
| `vetted` | the vetted rule, for the parts it names |
| `approved` | a standing value in the `approved` list; an account on the `trusted` list |
| `unapproved` | a standing value in the `unapproved` list |
| `none` | the fallback, for any value no list names, including one added later |
| `blocked` | an account on the blocked list |

Blocked is decided first, refused at every floor, and nothing promotes it. Otherwise an item takes
the highest level any rule gives it. An item at `vetted` or `approved` has sufficient standing; the
rest have insufficient standing. Standing says who wrote an item, not whether its text is safe
([Prototype status and gaps](#prototype-status-and-gaps)).

- Levels come from declared metadata fields, so content cannot change one.
- A part with no author of its own inherits its item's level only where the vetted rule names it,
  and is raised to `vetted` where the rule holds; any other takes the fallback.
- The `trusted` and `blocked` lists hold account ids at `author_id`, so a renamed account keeps its
  place. A settings file that names accounts for a definition with no `author_id` is refused.
- Visibility picks the default floor and changes no item's level. With no `visibility`, or one that
  cannot be read, a place counts as public, the stricter default.
- The read floor decides what reaches a payload, and so what shapes a write. With both floors at
  their default, "the oldest open issue" means the oldest open issue whose author meets the read
  floor, not the oldest one. Whether reading could be looser than acting is an open question
  ([Read floor and target floor](#read-floor-and-target-floor)).
- The target floor decides which item an act may land on. A resolve applies it while it selects, and
  every existing item an act changes is held to it again before the popup: an `item` the plan names
  is read through the definition's `target` resource (GitHub's `comment` and `modify` read through
  `issue`; an act on a branch this run made needs none), an `on` item through its receipt, each item
  of a set in turn, and each element a `collect` gathers ([Writing](#writing)). Both floors default
  to `default_floor`.
- A single item below the read floor fails the step (`below-floor`). Anything else withheld, or
  dropped for bounds, is counted with its reason in the reply and in the payload.

| | GitHub | Gmail |
| --- | --- | --- |
| standing field | `author_association`[^issues] | `labelIds` |
| `approved` | `OWNER`, `MEMBER`, `COLLABORATOR` | `SENT`: the person sent it |
| `unapproved` | `CONTRIBUTOR` | not used |
| `none` | `FIRST_TIME_CONTRIBUTOR`, `FIRST_TIMER`, `MANNEQUIN`, `NONE`, any later value | everything else |
| `vetted` | a merged pull request's changed files; its title, body and comments keep their standing | not used |
| `author_id` | `user.id` | none, so settings cannot name accounts |
| visibility | read from `GET /repos/{place}` | always private |
| default floor, public / private | `approved` / `none` | `none` / `none` |

Gmail counts only `SENT` because a `From` header is whatever the sender wrote; at its floor of
`none` the person's mail is readable as untrusted content.

The person sets floors and lists per place in `~/.brave_vault/connectors/<name>.settings.json`, held
to the same owner and mode checks as the definition:

```json
{ "places": { "*":        { "read_floor": "approved" },
              "acme/web": { "read_floor": "none", "target_floor": "vetted",
                            "trusted": ["583231"], "blocked": ["9919"] } } }
```

Places match ignoring ASCII case, and one named twice is refused. A place's own floor wins over `*`,
and both lists apply. No file means the definition's `default_floor`. A file that does not parse,
names an unknown key or level, uses `blocked` as a floor, or names an empty id or place refuses
every call of that connector (`bad-settings`), so a floor the person set is never dropped silently.
The settings sit with the vault because the floor keeps injected items out of bravebot.

The level names are gh-aw's[^gh-aw], which counts every private-repository item `approved` and a
`FIRST_TIME_CONTRIBUTOR` `unapproved`; this design keeps standing and puts the latter at `none`.
GitHub's MCP server[^lockdown] and Copilot cloud agent[^copilot] filter by push or write access.
Counting `MEMBER` as `approved` is a deliberate approximation of write access to the repository,
since `author_association` is what each item carries and a member may lack push access.

### Sanitising

Before a quarantined processor reads fetched text, the vault removes what a reader of the rendered
page would not see: Unicode default-ignorable characters (zero-width, bidirectional and invisible
formatting marks, variation selectors, tag characters), interlinear annotation marks, control
characters other than tab and line breaks, HTML comments, and elements hidden by their own markup
(`hidden`, `aria-hidden="true"`, an inline `display:none` or `visibility:hidden`) with everything
inside them, whatever the spacing, case or entities. HTML-only mail loses scripts, styles, the title
and templates, and the rest is reduced to its text. Code fields keep every character, shown as
`\u{XXXX}` escapes and `<!--` as `&lt;!--` in the payload and the popup. No stylesheet is applied,
so text hidden by a CSS class, zero font size or opacity, matching colours or off-screen positioning
reaches the processor. Sanitising is not a security boundary: mail text reaches the processor at
Gmail's floor whether hidden or not, and a message's plain-text part, read first, may differ from
what Gmail shows.

### Writing

An act is prepared without changing anything at the service, shown in the authority's popup
([When the person is asked](#when-the-person-is-asked)), and then that same request is sent once.

- **Write rules,** checked before the popup: a byte cap, refused paths (`refused-path`) and named
  rules: `empty`, `credential` (GitHub, GitLab, Slack, Anthropic, OpenAI, Google and AWS key
  prefixes and PEM private keys, in title or text; defence in depth, not an exfiltration control),
  `image`, `html-comment`, and
  `link-outside:<host>` (literal links; the host's subdomains allowed). The markup rules skip `code`
  resources and their title. Both definitions refuse empty or oversized text and credentials. GitHub
  also refuses HTML comments, and images and links outside `github.com`. External embeds are
  refused because rendering can cause network retrieval. External links may also be refused because
  generated URLs can encode sensitive data and disclose it when followed.
- **High-impact acts.** A surface resource with `high_impact: true` (GitHub's `review`, `put_file`
  and `open_pull`) is named "high-impact write" on its plan line and has its popup open with a
  warning; a push adds that it may run the repository's CI
  with its secrets ([Prototype status and gaps](#prototype-status-and-gaps)).
- **Outcomes.** A 2xx is `accepted`: the service accepted the operation by the connector's success
  condition, and its downstream effects are not guaranteed. Where the definition gives an `ok`
  predicate, a 2xx whose body fails it is refused (`service-error`). No answer, an oversized answer,
  or a 5xx is `unknown` and is not retried, since the write may have happened: GitHub ends a request
  that takes over 10 seconds with a "Server Error"[^gh-timeout], and Gmail warns that "you can't
  assume that a 200 response means the email was successfully sent"[^gmail-errors]. A reply lost
  after an act reached the vault is `unknown` too. Any other status is refused: 401
  `not-authorised`, 403 `forbidden`, 404 `not-found`, 409 and 422 `rejected`, and the rest, 408 and
  429 included, `service-error`.
- **Links.** An accepted write's reply may carry `link`, an https URL on a fixed host that the vault
  fills from the definition's template with formed plan fields or answer fields; a set act puts a
  link per item in the detail. bravebot shows a well-formed link on the step's line ("comment on
  github brave/bravebot: accepted · https://…") and drops any other.
- **Pushing lands on a branch this run created.** The first `put_file` to a branch creates it from
  the default branch after approval, and its popup says so; later ones in the run add to it. A
  branch that existed before the run is refused (`branch-exists`), and so is any path under
  `.github/workflows/` (`refused-path`). The popup shows the diff against the file replaced.
  `open_pull` opens a pull request only from a branch an earlier act in the run made, and refuses
  any other before the popup (`not-made-in-run`); nothing merges.
- **Options.** The catalogue, the surface check and the engine each refuse an option no `values`
  list offers. GitHub's `APPROVE` can satisfy branch protection; it is offered because the plan
  fixes the option before any read, so no pull request can turn a comment into an approval.
- **An option that names a thing** (`add_label:Receipts`) is looked up by name before the popup; if
  missing, and the definition allows, the popup says it will be made after Allow. A name not found
  within the page cap while more remain, or with surrounding spaces, a control character or too many
  bytes, is refused.
- **A create, then the write.** A branch or label create is classified like a write; if it is
  `unknown` or refused, or a label create answers 2xx without an id (`service-error`), the write is
  not sent. If the create succeeded and the write fails, the reply says what was made.
- **An act on every match** (`collect`) gathers the ids before the popup, each held to the item
  form. More than `max`, or pages left over, refuses the act whole (`too-many`); no match refuses it
  `nothing-matched`. Each gathered element is held to the target floor where it carries the
  standing field: those below it are left out and the popup counts them, and the act is refused
  (`below-target-floor`) if none is left. Where elements carry no standing field, the act is refused
  above floor `none`. One popup names the count and the selector and lists the frozen targets, each
  id with a display line where the definition's `show` template can be filled (Gmail's, when the
  matches were read whole for an exact sender), and Allow is bound to those ids.
- **An act on a set** names in `on` a fetch that chose a set. Text that does not start like JSON
  (`{`, `[` or a code fence) goes to every item. Text that does must parse as an object from item to
  text (keys may lead with `#`; one code fence may wrap it). A malformed object (`bad-value`) or a
  key the fetch did not choose (`not-chosen`) refuses the act before anybody is asked. An item left
  out or given blank text gets nothing; an act with no text lands on every item. Each item is read
  again and held to the target floor.

## Connector definition language

The full key list, the forms of values, the limits and the tests are in
[connectors-reference.md](connectors-reference.md).

A definition says:

- **what can be done:** the service's resources, each a fetch (a read) or an act (a write), the plan
  fields each takes, and the selector terms a fetch accepts;
- **what a value must look like:** the form every repository, issue number, path or address must
  have before it goes into a call ([Forms](connectors-reference.md#forms));
- **how to sign in:** the OAuth endpoints, scopes and client;
- **how to call:** for each resource, the HTTP method and URL, the body fields, and how results are
  paged;
- **what to keep:** which fields of each answer are kept, and how text is cleaned;
- **whose writing has standing:** which authors count as having standing at the service
  ([The standing floor](#the-standing-floor));
- **what a write may contain:** size limits and refused content such as credentials.

The file has two halves:

| half | read by | holds | installed at |
| --- | --- | --- | --- |
| **surface** | bravebot | what can be done and the forms: what the planner may ask for and the plan check needs. No URLs, no secrets | `~/.bravebot/connectors/<name>.json`, with the connector's name, its `definition_id`, the authority's path and the credential item |
| **wire** | the vault only | everything else: base URL, sign-in with the client secret, calls, paging, kept fields, standing and write rules | `~/.brave_vault/connectors/<name>.json`, the whole file |

### Gmail's definition

From brave-vault's `crates/connector/definitions/gmail.json`, trimmed to two resources:
`find_message` chooses an email, and `reply` answers it in its thread. A list or text cut short ends
in `…`, and `// …` stands for keys left out. GitHub's definition, trimmed the same way, is in
[the reference](connectors-reference.md#githubs-definition).

```jsonc
{
  "name": "gmail",
  "surface": {                                     // for bravebot
    "resources": {
      "find_message": {
        "step": "fetch",
        "uses": ["place", "select"],
        "forms": { "place": "opaque-id" },         // a mailbox; `me` by default
        "select": ["in:inbox", "is:unread", "from:*", "to:*", "subject:*", "after:*", …],
        "receipt": true,
        "summary": "the newest email matching a selector, with its body; …"
      },
      "reply": {
        "step": "act",
        "uses": ["place", "on"],                   // lands only on what a fetch chose
        "forms": { "place": "opaque-id" },
        "bound": {                                 // taken from that email, never from the plan
          "recipient": "the sender of the email it replies to",
          "title": "Re: and the subject of that email",
          "thread": "that email's conversation",
          "message_id": "that email's message id",
          "references": "the message ids that email refers to"
        },
        "summary": "reply with one plain-text email to the email a find_message or message fetch chose; …"
      }
    },
    "forms": { "place": "address", "item": "opaque-id" },   // `send`'s place is an address
    "defaults": { "place": "me" }
  },
  "wire": {                                        // for the vault only
    "base": "https://gmail.googleapis.com",
    "auth": { "flow": "pkce-loopback",
              "scope": "https://www.googleapis.com/auth/gmail.modify https://www.googleapis.com/auth/userinfo.email" },
    "paging": { "kind": "token", "per_page": 20, "size": "maxResults",
                "next": "nextPageToken", "cursor": "pageToken" },
    "visibility": "private",                       // mail is always private
    "integrity": {                                 // the standing rules
      "standing": { "field": "labelIds", "approved": ["SENT"], "fallback": "none" },
      "default_floor": { "public": "none", "private": "none" }   // anyone can email you
    },
    "resources": {
      "find_message": {
        "candidates": "GET /gmail/v1/users/{place}/messages",
        "stop": "first",
        "then": "message",                         // read the choice with `message`'s calls
        "receipt": { "item": "id", "thread": "threadId",
                     "sender": "payload.headers[name=From].value",
                     "subject": "payload.headers[name=Subject].value",
                     "message_id": "payload.headers[name=Message-ID].value",
                     "references": "payload.headers[name=References].value" },
        "select": {
          "from:*": { "query": "q", "join": " ", "template": "from:\"{value}\"",
                      "exact_mailbox": "payload.headers[name=From].value" }   // an address matches only that mailbox
          // …
        }
        // …
      },
      "reply": {
        "call": "POST /gmail/v1/users/me/messages/send",
        "text": "raw", "encoding": "base64url",    // the engine builds the MIME message
        "message": { "To": "{recipient}", "Subject": "{title}",
                     "In-Reply-To": "{message_id}", "References": "{references} {message_id}" },
        "fields": { "threadId": "{thread}" },
        "bind": {                                  // fill the bound values from the receipt
          "recipient": { "from": "sender", "take": "mailbox", "form": "address" },
          "title": { "from": "subject", "template": "Re: {value}", "unless_prefix": "re:" },
          "thread": { "from": "thread", "form": "opaque-id" },
          "message_id": { "from": "message_id", "take": "msg-id", "optional": true },
          "references": { "from": "references", "take": "msg-ids", "optional": true }
        }
        // …
      }
    }
  }
}
```

### Loading a definition

In the prototype, brave-vault's `scripts/install-connector.sh` writes the two halves to the places
in the table under [Connector definition language](#connector-definition-language), and the person
writes the client secret into the vault's copy. brave-vault's README, under Connectors, covers
registering the OAuth clients and the rest of setting up. The broker is to ship definitions signed
by Brave instead ([Broker requirements](#broker-requirements)).

The install script also computes the `definition_id`, the SHA-256 of the canonical definition
without the client id and secret, and writes it into bravebot's copy. bravebot refuses a copy whose
`definition_id` is missing or not 64 lower-case hex characters, and sends it with every request; the
vault refuses a request that names none (`definition-missing`) or an id that differs from its own
copy's (`definition-mismatch`), and binds receipts to it. The id ties the two halves to one
definition but is not signed, so whoever can write both copies can change both.

bravebot reads definitions only from the person's state directory, never a settings layer a checkout
could carry, because the file names the program that receives every write's text. Both sides refuse
a definition whose file or folder group or others can write, an unknown key, a form name nothing
defines, a fetch's `narrow` term its own `select` does not offer, and every definition off Unix.
bravebot also refuses one whose mode it cannot read. The vault also requires the file and folder to
belong to the owner of the home folder. A refused definition stays listed, and the planner and the
goal call are told it is refused and why. Where another connector is usable, a plan naming the
refused one is refused with that reason; where none is, a connector step is an unknown capability.

### Adding a service

1. Write its definition: the surface (resources, fields, forms) and the wire (calls, sign-in,
   paging, kept fields, standing, write rules), following the
   [reference](connectors-reference.md#reference).
2. Register an OAuth client with the service and install the definition, as brave-vault's README
   describes.
3. If the service needs something no definition can express yet, the engine first gains a general
   feature for it, which any definition can then use. Gmail's `reply`, for example, uses the
   engine's `message` block, which builds a raw MIME email for any mail service. The gaps known
   today are listed under [Prototype status and gaps](#prototype-status-and-gaps).

A definition cannot contain scripts, because the vault runs it while holding the person's credential
([Decisions](#decisions)).

## What the connectors do

Each supported task is one plan, approved once, with a popup per write. Each refused task names
the rule that stops it. The resources behind the tasks, their selectors and each provider's
details are in the reference ([GitHub resources](connectors-reference.md#github-resources),
[Gmail resources](connectors-reference.md#gmail-resources)).

### GitHub tasks

**Supported.**

| task | how it is planned |
| --- | --- |
| read an issue or a pull request, with comments and changed files | `issue` or `pull` with `item` |
| list issues, search an organisation, find the newest open pull request | `list`, `search` with `org:`, `find_pull` |
| comment on an issue named by number, or one a fetch chose | `comment` with `item`, or `on` an `issue` or `find_issue` |
| comment on each issue of a set, with one text for all or one per issue | `find_issue` with `limit:N`, then `comment` with `on` |
| close (done, not planned, duplicate) or reopen, one issue or a set | `modify` with the option fixed in the plan |
| add or remove a label | `modify` with `add_label:<name>` or `remove_label:<name>` |
| push a file to a new branch and open a pull request | `put_file`, `open_pull` |
| review a pull request with a fixed verdict | `find_pull` narrowed, or `pull`, then `review` |

**Refused.**

| task | refused by |
| --- | --- |
| write a file under `.github/workflows/` | `refuse_paths` |
| "approve the pull request labelled X", even the newest one | `narrow_when` |
| "approve it if it is good, otherwise request changes" | content would choose the option, which is routing ([LABEL-5](../specs/labels.md#LABEL-5); [Data-dependent branches](#data-dependent-branches)) |
| "close the duplicates", "label each issue bug, feature or question" | content would choose the option, per issue |
| merge, delete a branch, edit a title or body, assign, request reviewers | not offered |
| more items in one act than a set allows ([Limits](connectors-reference.md#limits)) | `too-many` |
| act on a search hit | no place or receipt |
| search all of GitHub | `search`'s `narrow` |

### GitHub's OAuth App

The OAuth App is registered once by whoever owns the connector; its tokens are in
[Credentials](#credentials). The prototype uses an OAuth App; a GitHub App is preferred for
organisations and production ([Decisions](#decisions)).

- **Callback** `http://127.0.0.1/github/callback`, on any port, as RFC 8252 asks of native
  apps[^authorizing][^rfc8252]. The device flow is off, since GitHub warns it can be used to
  impersonate an app in phishing[^oauth-best].
- **`repo`** lets a token comment, review and change files in private and public repositories.
  Without `workflow`, GitHub refuses most changes under `.github/workflows/` but allows a workflow
  file identical to one on another branch[^scopes], so the definition's `refuse_paths` is the
  guarantee.
- **The client secret.** GitHub's code exchange always requires `client_secret`[^authorizing]. A
  desktop app ships it as configuration, as `gh` and Git Credential Manager do; RFC 8252 says such a
  secret "serves little value beyond client identification"[^rfc8252]. PKCE, which GitHub accepts
  with S256 only[^pkce], and the loopback redirect protect the exchange. With the secret, somebody
  else can start a sign-in as the App, and refresh a refresh token they have stolen.
- **Reach** is everything the person can reach, subject to organisation policy; writing to a
  repository they can write to needs nothing installed there. The token acts as the person, bounded
  by custody, the listed resources (no delete, merge or administration call), the locked `place` and
  the popup per write. A leaked refresh token keeps that reach until revoked at GitHub.
- **Organisations.** OAuth App access restrictions are on by default for new organisations. Until
  the organisation approves the OAuth App, it has no API access to private organisation resources
  and no privileged create, update or delete actions on public ones[^restrict]. GitHub does not list
  which writes count as privileged; commenting, closing and reopening may work while creating a
  label or a branch answers 403 (`forbidden`). An organisation that enforces SAML single sign-on
  needs the person to authorise the App during an active session[^saml].

### Gmail tasks

**Supported.**

| task | how it is planned |
| --- | --- |
| read the newest N inbox messages, the newest unread, or the newest by subject or sender | `inbox`, `find_message` |
| read one email by id | `message` with `item` |
| send to addresses the task names | `send` |
| reply to an email in its thread | `find_message`, a processor, `reply` with `on` |
| archive, star, mark read or unread, or label one email | `find_message`, then `modify` with `on` |
| the same for every email from a sender, to a recipient or with a subject | `modify` with `select`; one popup states the count and lists the messages |

**Refused.**

| task | refused by |
| --- | --- |
| "archive everything" | the act's `narrow` |
| reply to every email from a sender | `find_message` chooses one |
| "file it or reply, depending on what it says" | content would choose the step ([Data-dependent branches](#data-dependent-branches)) |
| delete or trash mail | not offered |

### Gmail's OAuth client

`gmail.modify` is restricted, so publishing the client beyond Testing needs Google's
restricted-scope verification, and a security assessment if restricted data is stored on or
transmitted through servers[^gmail-scopes]. Mail a processor reads is sent to the model service the
person configured, which may count. In Testing, a test-user list is needed.

## Prototype status and gaps

**In the prototype.** brave-vault falls short of the #852 broker in these properties:

| property | #852 broker | brave-vault today |
| --- | --- | --- |
| the agent cannot read the credential | yes | partial: the vault file is sealed with a fixed password, so anything running as the person can decrypt stored tokens |
| definition semantics authenticated | yes | no: a hostile definition could describe one act in the popup and send another to the same host |
| popup approval bound to an authenticated caller | yes | no: anything running as the person can stand in as the authority, or reach the vault with the agent's handle |
| release provenance verified by the authority | yes | no: the origins on the popup's release line are bravebot's report |
| a stop cancels a pending authorisation | yes | no (below) |
| definitions signed | yes | no |
| surface and wire bound to one version | yes, signed | yes, by `definition_id`, unsigned ([Loading a definition](#loading-a-definition)) |
| receipts survive a restart | a design choice | no: a restart between a fetch and its act fails the act (`unknown-receipt`), since receipts and a run's branches live in memory |

- bravebot starts whatever program the definition names.
- The vault does not watch the connection, so an act, read popup or sign-in carries on after a stop
  or bravebot's wait runs out, and a late Allow still sends the act. A set whose popups are each
  answered near their timeout outlasts bravebot's wait and is reported `unknown` with no tally.

**Runs and popups.**

- A popup queued behind another can time out before it is shown. Allow arms whether or not the
  person has scrolled to the end of the text. A run's read grant outlives the run.

**Standing of what is read.**

- A compromised account with standing writes `approved` content, and somebody with standing who
  quotes a stranger puts the quote at their level. A Gmail message with `SENT` is `approved` whole,
  a forward of an attacker's mail included.
- A member-opened pull request whose head is a stranger's branch is read at the member's level. A
  member whose membership is private may show as `CONTRIBUTOR` or `NONE`, which under-counts their
  standing.
- A Gmail `from:` value that is not an address, such as a name, matches display names, so "the
  newest mail from Alice" can choose any sender who calls themselves Alice. The popup shows the
  address first.
- A spoofed `From` receives the reply; checking `Authentication-Results` before binding would narrow
  this. A message with no `Subject` cannot be replied to.
- A web search the model service runs during a planning call can shape the plan.

**Writes.**

- An `APPROVE` can let code merge on the person's say-so. Its popup warns, and shows the review's
  text and the commit, not the diff. GitHub refuses an approval of the person's own pull request.
- Pushing runs the repository's CI with its secrets, as for any branch the person pushes; GitHub
  withholds secrets only from workflows "triggered from a forked repository"[^secrets].
  `.github/workflows/` is refused; other build files a workflow runs are not.
- A bulk change is not undone; the opposite option over the same selector may match differently.

**Services and the format.**

- A third service may need a client authentication method (none or basic), the token's place in
  the token answer, a fixed callback port, a per-credential base URL (Jira), `POST` or GraphQL reads
  (Linear), nested or typed values in an act's `fields` (Calendar), offset paging or
  per-placeholder encoding.

Gaps in the implementation itself are in
[Implementation gaps](connectors-reference.md#implementation-gaps).


## Future proposals

None of these is built, and none is part of the [Security invariants](#security-invariants).

### Exact pre-authorised writes

A write would run without its popup only when the complete request was fixed before any service
content was read: the connector, account, capability, place, item, option, recipient, title and
exact text or diff are all written in the plan; no fetched slot contributes text; there is no `on`;
no bound value comes from a receipt; and no service answer changes the request. The authority would
verify the request against an authorisation capability created at plan approval, which it stores or
a broker signs, so the agent is never authoritative about its own approval; a mismatch asks or is
refused. Anything else asks. A high-impact write always asks, and under bypass no plan is approved,
so no capability exists.

### Broker requirements

The #852 broker is to provide these; none is built.

- **A canonical authorisation object** per prepared write: definition id, credential id and
  account, run id, connector, resource and action, place, item or items, path, selector snapshot,
  option, bound values, title hash, text or diff hash, release origins, and preparation and expiry
  times. Its SHA-256 is bound to the popup's Allow, the request sent is generated from that object
  only, and the audit record holds the hash, time, account, action, outcome and the service's
  request id or link.
- **Signed definition bundles.** One canonical definition, signed, whose surface and wire are
  projections of it, recording the definition's name and version, the schema version, the digest,
  the signer and the signature; an unknown schema version fails closed. Against rollback, the broker
  keeps a minimum accepted version per definition and supports revocation and signing-key rotation.
- **Popups that separate authority facts from service text.** The connector, account, action,
  destination, bound routing values, release origin and risk are drawn apart from untrusted text (an
  issue title, an author's name, generated text, a diff), which is never styled like a label.

### Read floor and target floor

An open question, not adopted. Reading could be more permissive than acting, since a processor is
quarantined, but a processor's output feeds writes, so today the read floor guards what shapes a
write. A possible rule applies the read floor only to content flowing into a write.

### Data-dependent branches

Some refused tasks know every possible act in advance, but the right one can be decided only after
reading service content: approve or request changes, file an email or reply to it.
[manifest-choose.md](manifest-choose.md) proposes a `choose` step for them. The plan fixes every
branch and its acts before anything is read, and at run time only a trusted selector, such as the
person, picks one. Untrusted observations may inform that choice but cannot select a branch. An act
in the chosen branch still passes its usual checks and its popup.

### Turn mode

Connectors in turn mode need their own case for the guarantee manifest mode gives
([Scope](#goals-adversary-model-and-non-goals)).

## Prior art, decisions and terms

### Prior art

- **Threat models.** A connector run holds all three legs of Willison's lethal trifecta[^trifecta]
  and faces goal hijack, tool misuse, and identity and privilege abuse, the OWASP Top 10 for Agentic
  Applications' first three[^owasp]. Meta's Agents Rule of Two says such a session needs
  supervision[^rule-of-two], which the popup per write gives.
- **Structural defences.** *The Attacker Moves Second* breaks twelve published defences with
  adaptive attacks and sets aside designs whose control flow does not depend on data, where the
  attack cannot succeed[^attacker]. The dual LLM pattern is the quarantined reader's
  origin[^dual-llm].
- **Nearest relatives.** SPA is the closest general architecture: plan-first information-flow
  control that tracks control dependencies too[^spa]. This design applies the same separation of
  trusted control from untrusted observations to external services, and adds credential custody
  outside the agent and receipts the authority holds. ACE plans from trusted information and checks
  the plan statically[^ace], f-secure keeps untrusted input out of planning by information
  flow[^fsecure], and Progent bounds tool calls by policy[^progent]. Unlike them, this design fixes
  destinations in the plan.

### Decisions

| alternative | not used because |
| --- | --- |
| generating tools from an OpenAPI file | it exposes every endpoint, delete and merge included; a surface is a short allowlist |
| a code escape hatch in a definition | a definition is input to the program holding the credential; most REST services differ only in auth, paths, paging and fields, as Airbyte's low-code connectors assume[^airbyte] |
| per-value provenance in an interpreter, as CaMeL[^camel] | it needs an interpreter in the control path; per-slot releases and one plan approval check every run the same way |
| a GitHub App's user tokens | a trade-off, and the preferred choice for organisations and production: an installation boundary, selected repositories, fine-grained permissions and short-lived tokens, but it writes only where installed[^user-token]: by an organisation owner, or by a repository admin where the App asks for no organisation or repository-administration permission and owners have not restricted admin installs[^app-install]. An OAuth App (`repo`, every repository the person can reach, little setup) is acceptable for personal and experimental use, which is what the prototype uses |
| a personal access token | Brave's policy is moving away from them, and a person would paste a long-lived secret |
| the `gh` CLI's or Git Credential Manager's token | the agent can take it (`gh auth token`), it lasts until revoked or a year unused[^token-expiry], and `gh api` can make any call with no popup |
| installation tokens | minting needs the App's private key[^keys], so a Brave-run server, as Octo STS runs[^octo] |
| GitHub's remote MCP server | no dynamic client registration, so it needs a registered app anyway[^mcp-host], and its calls carry no routing |
| GitHub's local MCP server ([Compared with MCP servers](#compared-with-mcp-servers)) | a step through it would be an opaque tool call with no routing the plan can lock |

This design carries [safehouse](https://github.com/brave-experiments/safehouse)'s precommitted
release, payload counts, review narrowing, default option, draft exclusion and no-retry rule. It
differs in that GitHub is a data file; the credential is never in the agent; a resolved item is
approved in a popup instead of promoted; lists hold ids and there are two floors; writes are checked
and a push lands on a new branch; recipients come from the vault's record, and a private place's
items keep their standing.

### Terms

| term | meaning |
| --- | --- |
| broker | the production authority #852 is to build, running as its own account |
| vault | brave-vault, the experimental desktop app that prototypes the authority; its definitions name `brave-vault-agent`, which bravebot starts and which relays to the app. "The vault" describes the prototype; the broker is to take the same role |
| engine | the crate that interprets a definition, today brave-vault's `crates/connector` |
| surface check | bravebot's check of a plan's connector steps against the surface, before approval |
| routing field | a plan argument locked before approval and shown on the plan line ([MANIFEST-6](../specs/manifest.md#MANIFEST-6)) |
| place | where a step reads from or lands (a repository, a mailbox, a recipient list); a routing field |
| item | one thing in a place, named by the plan (an issue number) or chosen by a fetch |
| credential item | the vault entry a connector uses, `svc:<name>:<account>` |
| resolve | a fetch that chooses one item, or a set of several, from candidates the service lists |
| standing | an author's place at the service as its metadata records it (a repository association, Gmail's `SENT`); sufficient at `vetted` or `approved` |
| high-impact act | an act whose resource has `high_impact: true`; its popup warns first |

[^patterns]: [Design Patterns for Securing LLM Agents against Prompt Injections](https://arxiv.org/abs/2506.08837)
[^dual-llm]: [The Dual LLM pattern for building AI assistants that can resist prompt injection](https://simonwillison.net/2023/Apr/25/dual-llm-pattern/)
[^camel]: [Defeating Prompt Injections by Design (CaMeL)](https://arxiv.org/abs/2503.18813)
[^airbyte]: [Airbyte low-code connector development kit](https://docs.airbyte.com/platform/connector-development/config-based/low-code-cdk-overview)
[^gh-timeout]: [Troubleshooting the REST API: timeouts](https://docs.github.com/en/rest/using-the-rest-api/troubleshooting-the-rest-api)
[^gmail-errors]: [Gmail API: resolve errors](https://developers.google.com/workspace/gmail/api/guides/handle-errors)
[^gh-aw]: [GitHub Agentic Workflows: integrity filtering](https://github.github.com/gh-aw/reference/integrity/)
[^lockdown]: [GitHub MCP server: server configuration, lockdown mode](https://github.com/github/github-mcp-server/blob/main/docs/server-configuration.md)
[^copilot]: [Copilot cloud agent: risks and mitigations](https://docs.github.com/en/copilot/concepts/security-governance-and-network-settings/risks-and-mitigations)
[^toxic]: [Invariant Labs: GitHub MCP exploited](https://invariantlabs.ai/blog/mcp-github-vulnerability)
[^rfc5537]: [RFC 5537, section 3.4.4: trimming References](https://datatracker.ietf.org/doc/html/rfc5537#section-3.4.4)
[^mcp-elicitation]: [MCP specification 2025-11-25: elicitation](https://modelcontextprotocol.io/specification/2025-11-25/client/elicitation)
[^mrtr]: [MCP specification 2026-07-28: multi round-trip requests](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr)
[^mcp-auth]: [MCP specification 2026-07-28: authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization)
[^bcp]: [RFC 9700, OAuth 2.0 Security Best Current Practice](https://datatracker.ietf.org/doc/html/rfc9700)
[^issues]: [GitHub REST: issues](https://docs.github.com/en/rest/issues/issues)
[^authorizing]: [Authorizing OAuth apps](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
[^rfc8252]: [RFC 8252, OAuth 2.0 for Native Apps, sections 7.3 and 8.5](https://datatracker.ietf.org/doc/html/rfc8252#section-8.5)
[^oauth-best]: [Best practices for creating an OAuth app](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/best-practices-for-creating-an-oauth-app)
[^scopes]: [Scopes for OAuth apps](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/scopes-for-oauth-apps)
[^gh-expiring]: [GitHub changelog: multiple redirect URIs and token refresh for OAuth apps](https://github.blog/changelog/2026-08-14-multiple-redirect-uris-and-token-refresh-for-oauth-apps/)
[^pkce]: [PKCE support for OAuth and GitHub App authentication](https://github.blog/changelog/2025-07-14-pkce-support-for-oauth-and-github-app-authentication/)
[^claude-web]: [Claude Code on the web](https://code.claude.com/docs/en/claude-code-on-the-web)
[^agentcore]: [Amazon Bedrock AgentCore Identity: credential providers](https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/identity-outbound-credential-provider.html)
[^codex]: [Codex command injection and GitHub token theft](https://blog.barrack.ai/openai-codex-command-injection-github-token/)
[^app-install]: [Installing a GitHub App from a third party](https://docs.github.com/en/apps/using-github-apps/installing-a-github-app-from-a-third-party)
[^restrict]: [About OAuth app access restrictions](https://docs.github.com/en/organizations/managing-oauth-access-to-your-organizations-data/about-oauth-app-access-restrictions)
[^saml]: [About authentication with single sign-on](https://docs.github.com/en/enterprise-cloud@latest/authentication/authenticating-with-single-sign-on/about-authentication-with-single-sign-on)
[^google-native]: [OAuth 2.0 for iOS and desktop apps](https://developers.google.com/identity/protocols/oauth2/native-app)
[^google-scopes]: [OAuth 2.0 scopes for Google APIs](https://developers.google.com/identity/protocols/oauth2/scopes)
[^gmail-scopes]: [Gmail API scopes](https://developers.google.com/workspace/gmail/api/auth/scopes)
[^google-expiry]: [Using OAuth 2.0 to access Google APIs: refresh token expiration](https://developers.google.com/identity/protocols/oauth2#expiration)
[^granular]: [How to handle granular permissions](https://developers.google.com/identity/protocols/oauth2/resources/granular-permissions)
[^secrets]: [Using secrets in GitHub Actions](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-secrets)
[^user-token]: [Generating a user access token for a GitHub App](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app)
[^token-expiry]: [Token expiration and revocation](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/token-expiration-and-revocation)
[^keys]: [Managing private keys for GitHub Apps](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/managing-private-keys-for-github-apps)
[^octo]: [Octo STS](https://github.com/octo-sts/app)
[^mcp-host]: [GitHub MCP server: host integration](https://github.com/github/github-mcp-server/blob/main/docs/host-integration.md)
[^owasp]: [OWASP Top 10 for Agentic Applications for 2026](https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/)
[^trifecta]: [The lethal trifecta for AI agents](https://simonwillison.net/2025/Jun/16/the-lethal-trifecta/)
[^rule-of-two]: [Agents Rule of Two: a practical approach to AI agent security](https://ai.meta.com/blog/practical-ai-agent-security/)
[^attacker]: [The Attacker Moves Second](https://arxiv.org/abs/2510.09023)
[^spa]: Girrens and Wang, [*SPA: Securing Persistent LLM Agents Across Queries with Plan-First Information-Flow Control*](https://arxiv.org/abs/2608.27234), arXiv:2608.27234
[^ace]: [ACE: A Security Architecture for LLM-Integrated App Systems](https://arxiv.org/abs/2504.20984)
[^fsecure]: [System-Level Defense against Indirect Prompt Injection Attacks: An Information Flow Control Perspective](https://arxiv.org/abs/2409.19091)
[^progent]: [Progent: Securing AI Agents with Privilege Control](https://arxiv.org/abs/2504.11703)
