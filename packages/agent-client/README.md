# @brave/agent-client

A thin typed client for the bravebot session view. It drives a local `bravebot-rpc` process over
stdio, requires the `sessionView` capability (version 1), and applies the view updates Rust sends.
Rust owns session transitions and approvals; this package frames, correlates, and applies. See
[the client contract](../../docs/design/mobile/client-contract.md#implemented-typescript-client)
and [the spec](../../docs/specs/session-view.md).

```
src/common   wire types, framing, correlation, view application, AgentClient interface
             (no Node, DOM, Electron or JNI)
src/node     the child-process adapter
test-fixtures  wire-contract.json (written by a Rust test) and scenarios/ (language-independent)
tests        node:test suites, including ones that run a real bravebot-rpc
```

## Use

```ts
import { connectStdio } from '@brave/agent-client/node'

const connection = connectStdio({
  command: '/path/to/bravebot-rpc',
  env: { HOME: process.env.HOME ?? '', PATH: process.env.PATH ?? '' }, // the child's whole environment
  workspaces: [{ id: 'project', name: 'Project', directory: '/path/to/project' }],
})
const session = await connection.client.createSession({ workspace: 'project' })
session.subscribe((view) => console.log(view.sequence, view.status))
await session.answerTrust(false) // always an explicit call
await session.send('hello')
```

A question the turn asks appears in `view.pending`. Answer it with `session.decide(request, 'approve' | 'reject')`
for a write, command or fetch, or `session.answer(request, answers)` for a user question. A reply for a
request that is no longer on screen is refused before anything is sent. `session.cancel()` refuses the
question and ends the turn.

A view that has `ended` is the last state received. It is not recovered: version 1 has no
reconnect, so start a new session.

## Check

`make check-agent-client` from the repository root builds `bravebot-rpc`, installs, type-checks
(the common code with no ambient types), builds and runs the tests. Inside the package,
`npm run check` does the same once `bravebot-rpc` is built; set `BRAVEBOT_RPC` to use a binary
other than `target/debug/bravebot-rpc`. The tests use an empty home, a scratch project and a model
service of their own, and read no credentials or settings.

If the Rust view types change, regenerate the contract file with
`WRITE_WIRE_CONTRACT=1 cargo test -p bravebot-ui-bridge --lib the_client_wire_contract`, then update
`src/common/wire.ts`.

## Notes for embedders

- `env` is the child's whole environment. Pass only what `bravebot-rpc` needs, not `process.env`.
- Row and question `data` is released content that can come from untrusted sources. Show it as plain
  text with its label, and never as markup. The client does not read it or enforce this.
- `stderr()` keeps the last 64 KB the bridge wrote, which can include paths. Treat it as sensitive in logs.
- The bridge is the authority on what a session may do; the client only keeps callers to configured workspaces.
