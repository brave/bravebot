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
  env: process.env,
  workspaces: [{ id: 'project', name: 'Project', directory: '/path/to/project' }],
})
const session = await connection.client.createSession({ workspace: 'project' })
session.subscribe((view) => console.log(view.sequence, view.status))
await session.answerTrust(false) // always an explicit call
await session.send('hello')
```

A question the turn asks appears in `view.pending`. This version cannot answer it: `session.cancel()`
refuses it and ends the turn.

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
