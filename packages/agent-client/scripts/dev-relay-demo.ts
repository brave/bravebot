/**
 * Run `bravebot-host` with the development relay against synthetic data, for trying the phone
 * experiment by hand.
 *
 *   node dist/scripts/dev-relay-demo.js --host /path/to/bravebot-host
 *
 * It makes a scratch directory with an empty home, a workspace and a sessions root, starts a stub
 * model service on 127.0.0.1, and runs the host with the relay as its client in an environment
 * that holds only what the stub needs. Nothing reads or writes a real home, credential or session.
 * The host's standard input is this terminal, so `launch` and `quit` work as they do for the host.
 * The scratch directory is removed when the host stops.
 *
 * The stub answers by the plan named in a prompt: `plan:hello` replies, and `plan:write` asks to
 * write `hello.txt` in the workspace. Any other prompt gets the reply `done`.
 */

import { spawn } from 'node:child_process'
import { mkdirSync, mkdtempSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, isAbsolute, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import { ModelStub } from '../tests/support/model-stub.js'
import { isolatedEnvironment } from '../tests/support/rig.js'

const { values } = parseArgs({ options: { host: { type: 'string' } } })
if (values.host === undefined || !isAbsolute(values.host)) {
  process.stderr.write('usage: dev-relay-demo --host /absolute/path/to/bravebot-host\n')
  process.exit(2)
}

const scratch = realpathSync(mkdtempSync(join(tmpdir(), 'dev-relay-demo-')))
const workspace = join(scratch, 'demo')
const home = join(scratch, 'home')
mkdirSync(workspace)
mkdirSync(join(home, '.bravebot'), { recursive: true })

const stub = await ModelStub.start({
  'plan:hello': [{ say: 'Hello from the stub model. Nothing here is real.' }],
  'plan:write': [{ tool: { name: 'write_file', arguments: { path: 'hello.txt', contents: 'written from the phone\n' } } }, { say: 'Wrote hello.txt.' }],
})

const relay = join(dirname(fileURLToPath(import.meta.url)), 'dev-relay.js')
process.stderr.write(`dev-relay-demo: workspace ${workspace}; try the prompts plan:hello and plan:write\n`)
const host = spawn(
  values.host,
  ['--sessions-root', join(scratch, 'sessions'), '--workspace', workspace, '--', process.execPath, relay, '--workspace', workspace],
  { env: isolatedEnvironment(home, stub.endpoint), stdio: 'inherit' },
)
host.on('exit', async (code) => {
  await stub.stop()
  rmSync(scratch, { recursive: true, force: true })
  process.exit(code ?? 1)
})
