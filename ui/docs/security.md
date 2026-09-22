# Security boundaries

The UI uses the agent's existing library interfaces without modifying the pinned
upstream source. The bridge is a separate `bravebot-rpc` child process, speaking
newline-delimited JSON over pipes. Electron's main process manages its lifetime;
window close and app quit explicitly stop the bridge. A renderer-only crash does
not itself close the pipe: there is currently no `render-process-gone` handler, so
a pending decision may remain waiting until the window closes or the app quits.
A closed stdout alone also does not trigger shutdown in the current transport;
its write errors are ignored. Neither condition grants an approval.

## Renderer and IPC

The main window enables `contextIsolation` and `sandbox`, disables `nodeIntegration`
and `webviewTag`, and rejects in-window navigation. External links open through the
system browser. The preload exposes specific IPC operations and subscriptions,
not general Node or filesystem access. The main process allow-lists agent methods.

Project-file operations use a session handle and a relative path. Main-process
code resolves the session root and validates the path. Text previews and memory
operations use the descriptor-based `bravebot-ui-files` helper; directory listing
and opening files in an external app use separate validation in `src/main/files.ts`.
See [file access and retention](file-access-security.md) for the helper's guarantees
and limits. Do not assume every filesystem operation uses the helper.

File contents **do** cross IPC for previews and bot-memory editing. These operations
do not themselves send the contents to a model. File attachments require a native
picker, a session-bound grant, validation at send time, and an explicit Send action.
The main process strips raw `files` and `dropped` lists from renderer turn requests
and composes authorized paths itself. Bot briefings are also composed by the main process.
Both lists are admitted as trusted context, so the only path a bot contributes is the
briefing, whose every byte the main process wrote from what somebody typed. A bot's memory
is neither named nor quoted in it: the briefing says where the memory is and the model reads
it under whatever the trust map says about that path. See
[how a purpose reaches the model](interface.md#how-a-purpose-reaches-the-model).

## Decisions and refusal

The transcript presents write, command, command-output, path-vouch and user-question
requests. Replies must match both the pending request ID and its kind. Within a turn, an unknown
or consumed ID cannot approve another request. Clients discard pending questions
when a turn ends because IDs may recur in later turns. Malformed decisions decline;
question answers are checked against the offered choices.

Cancellation wakes pending questions and refuses them. Session close and bridge
shutdown also refuse outstanding questions. There is no timeout that grants approval.
These properties depend on both bridge refusal handling and Electron lifetime handling;
they are covered by the Rust refusal suites and Electron tests in [testing](testing.md).

The v0.9.0 agent also has fetch-host, language-server and manifest-plan approvals.
The UI does not present those requests yet; the bridge refuses them without taking
an answer intended for another pending question.

Vetted-content approval releases only the displayed bytes once; it does not create a trust
rule for future reads. Checker verdicts are advisory. Checking already sends the content
to the backend before the approval; approving admits it to the planner. Processor remarks
are labelled untrusted and displayed beside the write diff. Neither kind of advice grants
authority. Cancellation invalidates pending vetted approvals as it does other requests.

Watch prompts contain only the watch number and the path chosen when it was armed, never
file contents. Watches reuse upstream's eight-file limit, seven-day expiry and cooldown,
and automatic turns retain the normal trust and approval gates. Closing a session drops
its watches; cancelling an automatic turn stops the watch that started it.

Hooks are the user's own commands in the agent home, shared with the CLI. They are edited
through a dedicated IPC API and saved with the descriptor-relative secure file helper,
which accepts only `hooks.json` for this operation. Saves check the previously read text,
refuse symlinks, and atomically replace the file. Hook commands use argument arrays;
no shell is added. Configuration diagnostics return credential presence, never tokens.
Settings overrides can only be selected through the native picker, and administrator
pins retain upstream precedence. Overrides configure model services; they do not enable
settings-file permission grants in this UI.

Command-output content is released for display so the person deciding can read it.
Only the agent's approved path admits it to the planner. Confined material is labelled
in the UI; showing it is not evidence that the planner read it.

How it is labelled is this window's answer to
[LAYER-5](../../docs/specs/layering.md#LAYER-5), the rule that any surface showing released
content marks it. The mark here is structural rather than a margin: a quarantined preview sits in
a container the renderer draws, with a head saying `confined` and a foot saying what the content
cannot reach, and an untrusted write is marked on the card. Content reaches the tree as a text
child, so its own spelling of that chrome is drawn as characters. Nothing renders released content
as HTML (`dangerouslySetInnerHTML` appears nowhere and `rehype-raw` is not a dependency), and
nothing content renders makes the app fetch: an image is never an `<img>`, and no element drawn
around released content carries a `src`, a `srcset` or a `url(` in a style, so nothing leaves the
machine unless the person picks it. Following a link leaves the app only where a URL parser reads
its scheme as `http:`, `https:` or `mailto:`, since the main process answers a window-open by
handing the URL to the operating system; a relative path becomes a preview in this window instead,
and only where it stays inside the project. Markdown is applied to the assistant bubble and nowhere
else, so formatting is itself a statement about where the words came from.
`scripts/marking.test.mjs` is what holds all of that; see [testing](testing.md).

Not every write needs a prompt: the agent's integrity policy determines that, including for bot
memory.
See [memory write policy](interface.md#what-a-memory-write-is-actually-gated-on).

The bridge protocol and its implementation references are in
[phase-0-rpc-protocol.md](phase-0-rpc-protocol.md). File retention is local storage,
not encryption or a guarantee of secure erasure.
