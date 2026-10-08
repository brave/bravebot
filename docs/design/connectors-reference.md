# Connectors: the reference

The keys of a connector definition, the forms of its values, the engine's fixed behaviour, GitHub's
definition and each provider's resources, what changes in bravebot, the limits, the implementation's
gaps and the tests. The design is in [connectors.md](connectors.md), whose terms this uses.

## Reference

The keys of a definition, by where they sit.

**Surface, per resource** (`surface.resources.<r>`)

| key | meaning |
| --- | --- |
| `step` | `fetch` or `act` (required) |
| `uses` | the plan fields it takes, each required unless defaulted; `a\|b` means exactly one (required) |
| `summary` | one line for the planner and the popup's action line (required) |
| `select` | the selector terms it accepts; a trailing `*` takes any rest of the value, possibly empty |
| `narrow` | terms one of which must be used: by a fetch's own `select`, by an act's own `select`, or by the fetch an act names in `on` unless that fetch names its item |
| `narrow_when` | stricter `narrow` terms for one option value (`APPROVE`) |
| `receipt` | a fetch records what it chose, so an act can name it in `on` |
| `bound` | values an act takes from that chosen item, with the words the plan line uses |
| `text` | `false` for an act that takes no text and says what it does by its option |
| `high_impact` | `true` for an act that is named "high-impact write" on its plan line and whose popup opens with a warning (GitHub's `review`, `put_file`, `open_pull`) |
| `forms`, `values` | override the shared ones for this resource |

**Surface, shared** (`surface.*`): `values` (allowed values per field; a trailing `*` stands for any
non-empty rest), `defaults`, `forms` ([Forms](#forms)), and `examples` (one worked example per
resource, for the planner).

**bravebot's copy** holds `name`, `definition_id`, `authority`, `item` and `surface`, and nothing
else. `definition_id` is the SHA-256 the install script computed over the canonical definition
without the client id and secret, 64 lower-case hex characters
([Loading a definition](connectors.md#loading-a-definition)).

**Wire, shared** (`wire.*`)

| key | meaning |
| --- | --- |
| `base` | the https API origin; every call stays on it |
| `headers` | sent with every call |
| `auth` | sign-in: `flow` (only `pkce-loopback`), `client_id`, `client_secret`, `authorize`, `token`, `scope`, `params` (not ones the flow sets), `callback`, `on`, `whoami {call, account, fold_case}`, `refresh` (default true), `sign_in_again` (refresh errors that drop the credential, beside `invalid_grant`), `install` |
| `paging` | `kind` (`link-header` or `token`), `per_page`, `size` (the page-size parameter); `next` and `cursor` for `token` |
| `projection` | per part, the fields kept: a path, or `{as, from, decode, html}` where `html` paths are read as the text they show when `from` finds nothing |
| `integrity` | `standing {field, approved, unapproved, fallback}`, `default_floor {public, private}`, optional `author_id` and `vetted {when, parts}` ([The standing floor](connectors.md#the-standing-floor)) |
| `visibility` | `"private"`, `"public"`, or `{call, private}` to ask the service |
| `writes` | `max_bytes`, `refuse` (write rules), `code` (resources whose text is file content) |
| `popup` | the paths of the target's `title` and `author` |
| `ok` | `{path, equals}`: a success predicate every 2xx answer is held to, unless a resource names its own |

A definition that fetches needs `paging` and `integrity`.

**Wire, per resource** (`wire.resources.<r>`), by kind:

| kind | keys |
| --- | --- |
| read | `calls`, each with `part`, `call`, `pages`, `items`, `expand {call, item}`, `code`, `budget_words`, `budget_drops`, `max_items`, `excerpt_words`; `receipt`; `select` |
| resolve (a fetch that chooses) | `candidates`, `pages`, `items`, `expand`, `stop` (`first` or `all`), `max_items`, `exclude {present \| equals [path, value], unless}`, `then` (the read run on the choice), `receipt`, `select` |
| selector rule | `query` (a parameter, or `[sort, direction]`) with `join` and `template`; or a local predicate (`empty`, `contains`, `equals`, `present`, `absent`); or `max_items` for a limit term. `me` lets the value `me` stand for the signed-in account; `exact_mailbox` keeps, for an address-shaped value, only messages whose parsed `From` mailbox equals it |
| act | `call`, `text`, `encoding`, `title`, `option`, `options`, `collect`, `message`, `fields`, `bind`, `base`, `branch`, `ensure_branch`, `made_in_run`, `replaces`, `max_bytes`, `refuse_paths`, `link`, `target`, `ok`, and `select` for an act on every match |

The act keys with structure of their own:

| key | sub-keys |
| --- | --- |
| `bind.<v>` | `from` (a field the receipt recorded), `take` (`mailbox`, `msg-id`, `msg-ids`), `template`, `unless_prefix`, `form`, `optional` |
| `options.<value>` | `says` (the popup's words), `body` (the fragment it adds), `call`; for a pattern value, `named {list, items, name, id, fold_case, create, create_body}` |
| `collect` | an act on every match of `select`: `list`, `items`, `id`, `pages`, `max`, the batch `call`, the body `field` for the ids, `expand` (read each match whole, as an exact-sender term needs) and `show` (a template of paths for each target's display line in the popup); each element is held to the target floor; above `none`, an element without the standing field refuses the act (`below-target-floor`) |
| `ensure_branch` | `sha`, `exists`, `create`, `body`, `new_in_run` |
| `replaces` | the file a write replaces: `call`, `sha`, `content`, `encoding` |
| `made_in_run` | the act's `item` must be a branch an earlier act in the run made |
| `link` | `{template, answer {<name> {from, form}}}`: the https URL of an accepted write, on a fixed host, filled from formed plan fields or answer fields |
| `target` | the read resource that reads a plan-named `item` again before the popup, to hold it to the target floor and draw the popup's target; required of an act that takes `item` unless its item is a branch this run made |
| `ok` | `{path, equals}`: a success predicate on a 2xx answer's body; a body that fails it is refused `service-error` |

An unknown key anywhere refuses the file. Every plan field a call's path fills needs a declared
form; `message`, `fields` and body templates are not held to one. The loader refuses a definition
whose parts disagree, and its error names the rule.

A service's several verbs on one object are one resource with the verb as an option: GitHub's
`modify` closes, reopens and labels an issue, as GitHub's MCP server has `issue_write` with a
`method`. An option adds a body fragment, names its own call where the service needs another method
or path, or both.

## Forms

A routing value filled into a call's path has a named form from a closed set. The engine requires
the definition to declare it and checks the value before each call. bravebot checks the same form
before approval for `place`, `item` and `path`, the only fields the shipped definitions give a form.
Defaults are held to their form too. Every form refuses an empty value, whitespace and control
characters. A segment is only letters, digits, `.`, `_` and `-`, and not `.` or `..`, so
`x/../../user` cannot turn `/repos/{place}/issues` into another endpoint.

| form | accepts |
| --- | --- |
| `owner/name` | two segments |
| `positive-integer` | digits, no leading zero, at most 10 |
| `branch` | segments, at most 100; not starting with `-`, `refs/`, `heads/`, `tags/` or `remotes/`; not ending in `.lock` or `.`; not `HEAD`; no `..`, `@{` or segment starting with `.` |
| `relative-path` | segments, at most 255 |
| `opaque-id` | at most 64 letters, digits, `-` and `_` |
| `address` | one mailbox, at most 254: a local part of 1 to 64 letters, digits and `. _ % + ' -`, `@`, then two or more labels of 1 to 63 letters, digits and inner hyphens |
| `addresses` | 1 to 10 different `address` values, compared ignoring case, joined by `,`; at most 900 |

bravebot (`crates/config/src/connectors/forms.rs`) and the engine (brave-vault's
`crates/connector/src/forms.rs`) each hold these rules, kept identical by hand; no test compares the
two. A selector value is a bare word or a double-quoted phrase (`label:"good first issue"`).

## The engine

The engine's behaviour is fixed in code, and a definition selects it by name.

- **Calls.** Each plan value is held to its form before it fills a method and path template; query
  values are percent-encoded, and `{id}` is one path segment. A read is a `GET`. A call or next page
  whose URL leaves the `base` origin (scheme, host and port) is refused.
- **Paging.** `link-header`, or `token` (read from a reply field and sent back as a query
  parameter). A read call that stops at its page cap before filling its item cap counts `more-pages`
  once, and the payload names the part in `truncated`.
- **Lists and expansion.** `expand` reads each listed element whole by one more call; one gone
  before its read is withheld, or skipped in a resolve. Ids the service names are held to the item
  form. In a path, `a.b[key=value].c` picks the element whose field matches, ignoring ASCII case.
- **Selection.** The candidates call with query terms, local predicates and exclusions, then the
  `then` read on each chosen item. A `template` writes a term in the service's query language; a
  quote in a value is refused, and so is a space unless the template quotes it. A resolve
  chooses only candidates at the target floor, never one item twice, and each choice must pass the
  read floor when read (`below-floor`). Choosing nothing
  fails with `nothing-chosen`; a `limit:N` above what it may choose is refused `too-many`.
- **Projection and bounds.** Only named fields are kept, each with the element's level, and text is
  sanitised ([Sanitising](connectors.md#sanitising)). Caps and budgets drop whole items and count
  them (a budget first drops the fields `budget_drops` names); an excerpt marks its cut with ` …`. A
  word cap also caps characters, so text without spaces is bounded.
- **Encodings.** `json` (default), `base64`, `base64url`. `message` sends a plain-text message under
  named header templates over checked values, refusing a line break or control character, leaving
  out an empty header, folding long ASCII values and encoding non-ASCII as RFC 2047 words. Only the
  engine writes `MIME-Version`, `Content-Type` and `Content-Transfer-Encoding`.

## GitHub's definition

From brave-vault's `crates/connector/definitions/github.json`, trimmed to two resources:
`find_issue` chooses an issue, and `comment` writes on it. A list or text cut short ends in `…`.

```jsonc
{
  "name": "github",
  "surface": {                                     // for bravebot
    "resources": {
      "find_issue": {
        "step": "fetch",
        "uses": ["place", "select"],               // the plan fields it takes
        "select": ["state:*", "label:*", "no:label", "limit:*"],   // selector terms offered
        "receipt": true,                           // an act may land on what it chose
        "summary": "the first issue matching a selector, with its comments; …"   // trimmed
      },
      "comment": {
        "step": "act",
        "uses": ["place", "item|on"],              // `a|b`: exactly one of them
        "summary": "comment on an issue or pull request"
      }
    },
    "forms": { "place": "owner/name", "item": "positive-integer" }
  },
  "wire": {                                        // for the vault only
    "base": "https://api.github.com",
    "auth": { "flow": "pkce-loopback", "scope": "repo offline_access" },
    "paging": { "kind": "link-header", "per_page": 100, "size": "per_page" },
    "integrity": {
      "standing": { "field": "author_association",
                    "approved": ["OWNER", "MEMBER", "COLLABORATOR"],
                    "unapproved": ["CONTRIBUTOR"], "fallback": "none" },
      "default_floor": { "public": "approved", "private": "none" }
    },
    "resources": {
      "find_issue": {
        "candidates": "GET /repos/{place}/issues", // list, then choose one
        "exclude": { "present": "pull_request" },  // the list also holds pull requests
        "stop": "first",
        "then": "issue",                           // read the choice with `issue`'s calls
        "receipt": { "item": "number" },           // what the choice records
        "select": {
          "state:*":  { "query": "state" },        // sent as ?state=…
          "label:*":  { "query": "labels", "join": "," },
          "no:label": { "empty": "labels" },       // checked locally
          "limit:*":  { "max_items": "at most 20" }
        }
      },
      "comment": {
        "call": "POST /repos/{place}/issues/{item}/comments",
        "text": "body",                            // the text goes in the JSON field `body`
        "target": "issue",                         // re-reads a numbered item for the target floor
        "link": { "template": "https://github.com/{place}/issues/{item}#issuecomment-{id}",
                  "answer": { "id": { "from": "id", "form": "positive-integer" } } }
      }
    }
  }
}
```

## GitHub resources

| resource | step | does |
| --- | --- | --- |
| `issue` | fetch | one issue by number, with its comments; records a receipt |
| `pull` | fetch | one pull request by number, with its comments and changed files; records a receipt with its head commit |
| `list` | fetch | issues and pull requests matching a selector, without comments; one page |
| `search` | fetch | issues or pull requests across repositories; no place |
| `find_issue` | fetch | the first issue matching a selector at the target floor, or with `limit:N` up to N; records a receipt |
| `find_pull` | fetch | the first pull request matching a selector at the target floor; records a receipt with its head commit |
| `comment` | act | comment on an issue or pull request, by number or `on` a fetch |
| `modify` | act | `close`, `close_not_planned`, `close_duplicate`, `reopen`, `add_label:<name>`, `remove_label:<name>`; no text |
| `review` | act | `COMMENT`, `REQUEST_CHANGES` or `APPROVE` on the pull request a fetch chose, bound to the commit it read |
| `put_file` | act | write one file to a branch this run creates; `title` is the commit message |
| `open_pull` | act | open a pull request from a branch this run made into the default branch; the text is its description |

- **`modify`** closes or reopens by `PATCH`, adds a label by `POST` and removes one by `DELETE`. The
  label is found by name ignoring case and spelled as the repository spells it; a missing one is
  made after approval when added, and refused when removed.
- **A set.** With `limit:N`, `find_issue` chooses up to N issues under one receipt, each read with
  its comments. The plan writes the limit, so whether one item or a set comes back is known before
  any read.
- **`find_pull`.** `head:` is a local exact match on `head.ref`, because GitHub's `head` filter
  takes `owner:branch`[^pulls], and `author:` a local, case-sensitive one on `user.login`. `base:`
  is sent to GitHub. Drafts are left out unless the selector says `draft:include`.
- **`review`.** Its resolve must narrow by `head:`, `author:`, `title:` or a recency sort
  (`sort:created-*`, `sort:updated-*`), so "the newest open pull request" can be reviewed and "the
  pull request labelled X" cannot. An `APPROVE` needs a pull request named by number (a `pull`
  fetch) or chosen by `head:`, so "approve the newest pull request" is refused before approval. A
  review lands on whichever pull request matches when the run reaches it, and the popup shows its
  number. `commit_id` is the bound `commit`.
- **`put_file` and `open_pull`** read the default branch from `GET /repos/{place}`, held to the
  branch form.
- **`me`** in `author:`, `assignee:`, `mentioned:`, `user:` and `involves:` is the signed-in
  account.
- **Search.** `search` takes only `select`, sent as one query with `advanced_search=true`. Its
  `narrow` requires a scope term, `user:`, `org:` or `repo:`[^advanced], so bravebot and the engine
  refuse a query that would search all of GitHub; terms combine with AND, so the planner is told to
  give one. Each value is quoted or checked so it cannot add terms. With no place to read visibility
  from, hits are held to the public floor, a stranger's issue in a private repository included,
  unless the `*` floor is set lower.

Bodies are read as `body_text`, GitHub's text rendering of the markdown[^media], not the raw `body`.
Where GitHub cannot filter (`no:label`, `title:`, `head:`, labels and author on the pulls listing,
`kind:`, drafts), the engine filters locally.

## Gmail resources

| resource | step | does |
| --- | --- | --- |
| `inbox` | fetch | messages matching a selector, newest first, each with sender, recipient, subject, date and a snippet; one page |
| `find_message` | fetch | the newest match, with its decoded body; records a receipt |
| `message` | fetch | one message by id, which only a task supplies; records a receipt |
| `send` | act | one plain-text message: `place` the recipients, joined by commas, `title` the subject, the text the body |
| `reply` | act | answer the message a fetch chose; recipient, subject and thread are bound |
| `modify` | act | `archive`, `mark_read`, `mark_unread`, `star`, `unstar`, `add_label:<name>`, `remove_label:<name>`; no text |

- **Selectors.** `in:inbox`, `in:sent`, `is:unread`, `from:…`, `to:…`, `subject:…`, `after:…` and
  the rest compose one Gmail `q` parameter; `inbox` also takes `limit:`. Gmail's `from:` also
  matches display names, so a `from:` value shaped like an address keeps only messages whose parsed
  `From` mailbox equals it, ignoring ASCII case.
- **`modify`** lands on the message the fetch in `on` chose, or on every message a selector given in
  place of `on` matches, listed before the popup and changed by Gmail's batch call. The selector, or
  the `find_message` named in `on`, must narrow by `from:`, `to:` or `subject:`; a `message` fetch
  by id needs none. `add_label` makes a missing label after Allow.
- **`reply`** joins the conversation as Gmail requires: the message's `threadId` in the send body,
  `In-Reply-To` its `Message-ID`, and `References` its references then its `Message-ID`, with the
  subject matching[^threads]. A message without a `Message-ID` is replied to in its thread with no
  `In-Reply-To`, and `References` carries only its own references, if any. It replies to the `From`
  mailbox, not `Reply-To`, so the sender cannot redirect the reply; for mailing lists, ticket
  systems and aliases that can differ from a mail client's Reply.
- **Places.** A read's `place` is the mailbox (`opaque-id`, default `me`). `send` takes `addresses`,
  so a send naming no recipient, the defaulted `me` included, is refused.

## What changes in bravebot

bravebot takes no dependency on brave-vault: it speaks a JSON protocol to whatever authority a
definition names. With no connector installed, every request, prompt, catalogue and plan line is
what it was. For the broker, bravebot is to start no `brave-vault-agent`, check the broker's
identity, take each surface from the signed definition instead of its own copy, and send the plan's
releases with the run; the steps, the surface check and the specs stay as they are.

| file | change | modelled on |
| --- | --- | --- |
| `crates/core/src/capability.rs` | `Capability::Connector(ConnectorName)`: output untrusted and private, no network of this process | `McpCall` |
| `crates/core/src/manifest.rs` | two contract rows, empty defaults for optional connector routing, an act that takes no text, the release lines and step notes | the other contract rows |
| `crates/core/src/policy.rs` | `ReleasePlan::allow_into` and the gate `declassify_into_place` | safehouse's `declassify_slot` |
| `crates/agent/src/connectors.rs` (new) | the surface check, defaults, notes, catalogue, receipts per run | |
| `crates/agent/src/manifest.rs` | the two step arms, one re-ask on a connector check error, the goal call told each connector in plain words, plan mode refusing a connector act | MCP's `call_tool` arm |
| `crates/agent/src/turn.rs` | `preview_for` shared, to draw the authority's detail in the margin | |
| `crates/config/src/connectors.rs` and `connectors/forms.rs` (new) | reads bravebot's copy of each definition, its `definition_id` included; the closed set of forms | |
| `crates/connector/` (new crate) | the authority client: absolute path, cleared environment, one request on stdin carrying the `definition_id`, one reply on stdout | the Bedrock credential process |
| `crates/agent/tests/manifest.rs` | end-to-end tests against a scripted authority | |
| `Cargo.toml`, `Cargo.lock`, crate manifests, `lib.rs` lines | the new crate and modules | |

Permission rules do not name the connector capability. The change amends
[MANIFEST-1](../specs/manifest.md#MANIFEST-1), -5, -6 and -10,
[MODE-3](../specs/permission-modes.md#MODE-3), [LABEL-6](../specs/labels.md#LABEL-6),
[LABEL-8](../specs/labels.md#LABEL-8) and [ROUTE-5](../specs/routing.md#ROUTE-5), and adds two
clauses: MANIFEST-12, a connector act's content is released only to the place its plan fixed, and
MANIFEST-13, an authority's detail about a call reaches only the margin. Each ships with the code it
describes.

## Limits

The numbers the design sets, and where each is enforced; form lengths are in [Forms](#forms).

| limit | value | where |
| --- | --- | --- |
| bravebot's wait for one authority call | 10 minutes | `crates/connector` |
| a reply bravebot reads; a service or token answer the vault reads | 16 MiB | `crates/connector`; `connector_auth.rs` |
| a request `brave-vault-agent` reads; a request line the vault's socket reads | 4 MiB; no cap | `agent-cli`; `broker.rs` |
| a reason code or floor name bravebot prints | 48 characters, lower-case kebab starting with a letter | `crates/connector` |
| a link bravebot prints | 512 characters, `https://` and more, printable ASCII, no space | `crates/connector` |
| text and title on a plan line | 200 characters | `agent/connectors.rs` |
| bravebot's run id; the run id the vault accepts | 32 hex characters; 16 to 64 letters and digits | `crates/connector`; `broker.rs` |
| a selector value; a named option's name | 100 bytes | both sides; `act.rs` |
| a popup's wait, from when it is queued; a read grant | 2 minutes; 15 minutes at most and by default | `broker.rs` |
| Allow armed after | 800 ms | `App.jsx` |
| sign-in | 5 minutes; 8,192-byte request line; 5 seconds per connection | `connector.rs`, `connector_auth.rs` |
| a service request | 15 s to connect, 60 s in all | `connector_auth.rs` |
| a receipt | 24 hours, in memory; at least 128 bits from a CSPRNG | `connector.rs` |
| a `definition_id` | 64 lower-case hex characters (SHA-256) | `config/connectors.rs`; install script |
| pages per call, resolve, named lookup or collect; elements read whole per call or resolve | 10; 100 | `engine.rs`, `act.rs` |
| items one resolve may choose (a set); `max_items` | 20; 1 to 20, needing a receipt | `definition.rs` |
| a word cap's character cap | 8 × the words | `engine.rs` |
| a write's text; a file `put_file` writes | 64 KiB; 1 MiB | definitions |
| `References` ids kept; a header `mailbox` reads; a message id | the first and the last 19 of more than 20; 998 bytes; 250 characters inside the brackets | `forms.rs` |
| GitHub comments; changed files | 6,000 words over 3 pages; 4,000 words over 3 pages | `github.json` |
| GitHub `list` and `search`; Gmail `inbox` | 1 page of 50; 1 page of 20; 60-word excerpts | `github.json`, `gmail.json` |
| GitHub resolves; label lookup | 10 pages of 100 | `github.json` |
| Gmail `find_message`; a message body | 3 pages of 20 candidates; 2,000 words | `gmail.json` |
| Gmail `modify` on every match | 200 messages over 10 pages of 20, each listed in the popup | `gmail.json` |

## Implementation gaps

Gaps in how the prototype implements the design, beside those in [Prototype status and
gaps](connectors.md#prototype-status-and-gaps).

- The desktop app's Connectors page uses "connector" for an MCP server
  ([SERVERS-3](../specs/mcp-servers.md#SERVERS-3)); one of the two needs another name before this
  ships.
- Settings are edited by hand, and lists take account ids.
- Only bravebot checks that the fetch an `on` names narrows. The vault checks `narrow` only for a
  fetch's own selector and an act on every match, and re-checks forms, the option and selector
  terms against its own copy.
- bravebot takes any `*.json` file name as a connector name; the vault refuses a name outside its
  rule at call time.
- Both sides follow symlinks; bravebot checks the target file's mode but the mode of the folder the
  link sits in, not the target's folder, and does not check the owner.

## Tests

Every test uses fakes. The vault's are in brave-vault's `crates/connector` (`engine/tests.rs`,
`engine/act/tests.rs` and each module's own), `crates/core` and `crates/app/src-tauri/src/`.

| area | where | what is tested |
| --- | --- | --- |
| catalogue and plan | `agent/connectors.rs`, `agent/manifest.rs`, `tests/manifest.rs` | no step offered without a definition; a refused connector's reason; the one re-ask; each surface rule, `narrow` and `narrow_when`; locked defaults |
| forms | `agent/connectors.rs`, `config/connectors/forms.rs`; vault `forms.rs` | each form on both sides (`x/../../user` in bravebot, `a/../../user` in the vault, and `a/..`); in bravebot the forms and the check's rules are one table |
| definitions in bravebot | `config/connectors.rs` | each refusal in [Loading a definition](connectors.md#loading-a-definition), and a sixth field, a missing or malformed `definition_id`, a relative authority or a mismatched name |
| the authority client | `connector/src/lib.rs`, `tests/manifest.rs` | the cleared environment; a stop and the wait limit; reason-code and link forms; a reply that does not parse |
| releases and labels | `core/manifest.rs`, `core/policy.rs`, `tests/manifest.rs` | releases fixed at validation and named on the line; the gate; the payload reaching a processor and no planning call; the detail in the margin; no grant after an act |
| refusals and modes | `tests/manifest.rs` | refusals before the authority is asked or the plan is shown; plan mode; bypass still sending the act to the authority |
| integrity and settings | vault `integrity.rs`, `engine/tests.rs` | fallback standing, the blocked list, vetting, visibility, per-place floors and lists, a malformed settings file |
| reads | vault `engine/tests.rs`, `sanitise.rs` | withholding and counting, `below-floor`, paging on the base origin, `more-pages`, character bounds, sanitising in linear time, `code` escaping |
| sign-in and credentials | vault `connector_auth.rs`, `app/connector.rs` | `state`, no redirects, answer caps, host pinning, scopes, same client, `sign_in_again`, definition mode |
| writes | vault `engine/act/tests.rs` | prepared once and sent once, write rules, outcomes and the `ok` predicate, branch creation, `open_pull`, workflow paths, header injection |
| popups and grants | vault `app/connector.rs`, `broker.rs` | one-line popup text, a set's report, read-grant scope, handle policy per account; vault `protocol.rs`: `definition-mismatch` |
| resolves and receipts | vault `engine/tests.rs`, `engine/act/tests.rs`, `app/connector.rs` | the target floor on a resolve, a plan-named `item`, a set and a `collect`; no item twice, bounds and `too-many`, receipt scope, `moved` |
| mail and bound values | vault `engine/tests.rs`, `engine/act/tests.rs`, `forms.rs`; `tests/manifest.rs` | a reply's recipient, subject and thread; bound values refused in the plan and the request; references; duplicate singular headers; an exact `from:` mailbox; HTML-only mail |
| sets, labels, search | vault `engine/tests.rs`, `engine/act/tests.rs` | splitting text over a set and `not-chosen`, label lookup and creation, the `collect` cap, search scope and floor |

Not covered by automated tests: the popup's window (Allow's delay, focus on Deny, withdrawal on
timeout); the `timed-out` path; the read-grant prompt and Disconnect; refresh-token rotation and
the drop itself after a refused refresh; that a handle policy covers no write; the vault file's
atomic write beyond interleaved updates; reading the settings file from disk; the
one-sign-in-at-a-time lock; the standing staying out of the slot's label; and the two copies
of the forms agreeing.

[^pulls]: [GitHub REST: list pull requests, the `head` parameter](https://docs.github.com/en/rest/pulls/pulls#list-pull-requests)
[^advanced]: [GitHub changelog: issues advanced search in the API](https://github.blog/changelog/2025-03-06-github-issues-projects-api-support-for-issues-advanced-search-and-more/)
[^media]: [GitHub REST: media types](https://docs.github.com/en/rest/using-the-rest-api/getting-started-with-the-rest-api#media-types)
[^threads]: [Gmail API: managing threads](https://developers.google.com/workspace/gmail/api/guides/threads)
