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
src/dev-relay  a client over the development relay (no Node, DOM, Electron or JNI)
scripts      local-client: drives one session from a terminal
             dev-relay, websocket: the development relay that bravebot-host runs as its client
             dev-relay-demo: the host and the relay against a stub model, for trying the app
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

A view that has `ended` is the last state received. It is not recovered. Against `bravebot-host`, a
new client can take the session over: `instance()` lists the sessions the host holds, and
`attach(id)` takes over one whose controller's connection ended and starts its view from the host's
snapshot. `bravebot-rpc` alone refuses both, so there start a new session.

## Listing and opening sessions

`listSessions()` and `openSession(id)` are part of `AgentClient` for a target that keeps sessions a person can come back to. `describe()` reports whether a target has them as `sessionList`. `RpcAgentClient` does not: it reports `sessionList: false` and rejects both with `UnsupportedError`, sending nothing.

## Try it from a terminal

```sh
npm run build
node dist/scripts/local-client.js --rpc /path/to/bravebot-rpc --directory /path/to/project \
  --trust no --decide prompt "your prompt"
```

`--trust yes|no` is required: the program never assumes a trust answer. `--decide` is `approve`,
`reject` or `prompt` (ask on the terminal; without a terminal, or after Ctrl-D, the answer is reject). A user question is
declined. The child inherits this process's environment, so it reaches the model the way `bravebot-rpc`
normally does. Released payloads are printed as one line of JSON with control and bidirectional
characters escaped; this is a diagnostic dump, not a display surface.

Before you use it:

- `--decide approve` approves every question the turn asks, including commands and fetches and writes of
  content that came from outside. Use it only on a project and a task you would approve anyway.
- The dump prints full prompts, approval details (including the text of files about to be written) and
  fetched pages. Keep the output out of CI logs and anywhere else shared.
- The bridge child inherits this process's whole environment, secrets included. Run the program from a
  shell that holds only what `bravebot-rpc` needs.
- If an answer cannot be sent, the program cancels the turn, and if the bridge does not end it within
  `--grace` seconds (default 10) it gives up and exits non-zero.

## The development relay

The relay lets an app on a phone or emulator drive `bravebot-host` for the stage-3d experiment. It is a development transport for synthetic data. It does not resolve U1 or authorize control of a real session.

```sh
bravebot-host --sessions-root <root> --workspace <dir> -- "$(command -v node)" dist/scripts/dev-relay.js --workspace <dir>
```

The host runs `dist/scripts/dev-relay.js` as its client. The relay listens on 127.0.0.1 on a port the system picks and prints the port and a six-digit code for this launch on standard error. Nothing else records the code, and it is not part of the address. It takes one WebSocket connection, and the first message must be the code. A wrong code, or none within a minute, ends the relay. After the code it sends the host's workspaces and then passes the channel through unchanged. When either end closes the relay exits, and typing `launch` to the host starts a new one with a new port and code. Any program on the computer that can reach 127.0.0.1 can reach the port, and only the code keeps it out.

`@brave/agent-client/dev-relay` is the app's end. `connectDevRelay({ port, code, open })` takes the WebSocket constructor from the caller and resolves with a client and an `ended` promise. It is a separate export so a release build need not import it.

To try it with synthetic data:

```sh
make check-agent-client    # builds bravebot-host and dist/
node packages/agent-client/dist/scripts/dev-relay-demo.js --host "$PWD/target/debug/bravebot-host"
```

The demo makes a scratch home, workspace and sessions root, starts a stub model, and removes the scratch directory when the host stops. The prompts `plan:hello` and `plan:write` give a reply and a write to approve. [mobile/README.md](../../mobile/README.md#connect-to-a-host) covers the tunnel and the app.

## Check

`make check-agent-client` from the repository root builds `bravebot-rpc` and `bravebot-host`,
installs, type-checks (the common code with no ambient types), builds and runs the tests. Inside the
package, `pnpm run check` does the same once both are built; set `BRAVEBOT_RPC` or `BRAVEBOT_HOST` to
use a binary other than the one in `target/debug`. The tests use an empty home, a scratch project and a model
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
