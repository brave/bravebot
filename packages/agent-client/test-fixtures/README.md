# Test fixtures

`wire-contract.json` is written by the Rust test `view::tests::the_client_wire_contract_matches_the_rust_types`
and holds the capability, every status and row kind, and one update using every field.

`scenarios/*.json` are language-independent transcripts of a client against a scripted server. A
runner plays the `steps` in order:

| Step | Meaning |
|---|---|
| `start` + `call` (+ `session`, `args`) | begin a client operation without waiting for it: `describe`, `createSession`, `raw`, `unsupported`, `answerTrust`, `send`, `cancel`, `close` |
| `expectRequest` + `method`, `params` | the next request the client wrote must match exactly; its id is bound to the name |
| `expectNoRequest` | the client has written nothing since |
| `send` | server lines to deliver; `"$name"` stands for a bound request id; a string is delivered as a raw line |
| `await` (+ `result`, `error`, `bindSession`) | the operation's result matches, or it failed with the named error class and code |
| `expectView` + fields | the session's current view matches the listed fields (arrays by length and element) |
| `expectWorkspaces` | the configured workspaces, by id and name |
| `expectStartupTrust` + `value` | the trust question payload the client holds |
| `closeTransport` | the connection ends |

Server lines are delivered in chunks the scenario does not control; a runner plays each scenario
under several chunkings and expects the same result.
