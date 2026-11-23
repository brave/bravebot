// The desktop as a client of `bravebot-host`, left running for a person to try by hand.
//
//   node scripts/prototype-demo.mjs [--host /path/to/bravebot-host]
//
// It makes a scratch home and folder, serves a model service of its own, and starts the real host
// with two clients: the desktop, which opens its window, and the host test client standing in for a
// phone, which takes its commands from a named pipe. Nothing here reads or writes a real home,
// credential or session, and nothing is paid for. The host asks about the folder first and is
// answered no, so a session asks the desktop whether to trust it.
//
// The model service answers by the plan a message names: `plan:hello` replies, `plan:write` asks to
// write `out.txt` in the folder, and `plan:slow` takes 25 seconds, long enough to try a busy send and
// a cancel. Any other message gets the reply `done`.
//
// Type the phone's commands into the pipe from another terminal, for example
//   echo 'new' > <pipe>            open a session as the phone (it is asked nothing about trust)
//   echo 'attach s1' > <pipe>      join the session the desktop opened
//   echo 'send plan:write' > <pipe>
//   echo 'cancel' > <pipe>
//   echo 'list' > <pipe>
// and read what the phone says in the log the script names. Ctrl-C stops the host and the window.
import { execFileSync, spawn } from 'node:child_process'
import { appendFileSync, existsSync, rmSync, realpathSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { parseArgs } from 'node:util'
import { fileURLToPath } from 'node:url'
import { makeWorld, startModelService } from './drive-world.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const ui = join(here, '..')
const { values } = parseArgs({ options: { host: { type: 'string' } } })
const hostBinary = values.host ?? join(ui, '../target/debug/bravebot-host')
const phoneScript = join(ui, '../packages/agent-client/dist/scripts/host-client.js')
for (const [what, path] of [['bravebot-host', hostBinary], ['the host test client', phoneScript], ['the app', join(ui, 'out/main/index.js')]]) {
  if (!existsSync(path)) {
    process.stderr.write(`${what} is not built at ${path}. Run \`pnpm run bridge\`, \`pnpm --dir ../packages/agent-client run build\` and \`pnpm exec electron-vite build\` in ui/.\n`)
    process.exit(2)
  }
}
const electronPath = createRequire(import.meta.url)('electron')

const { service } = await startModelService(async ({ messages }) => {
  const last = messages.at(-1)
  const asked = JSON.stringify(messages.filter((message) => message.role === 'user').at(-1) ?? '')
  if (asked.includes('plan:write')) {
    return last?.role === 'tool' ? { content: 'Wrote out.txt.' } : { tool: { name: 'write_file', arguments: { path: 'out.txt', contents: 'written after approval\n' } } }
  }
  if (asked.includes('plan:hello')) return { content: 'Hello from the stub model. Nothing here is real.' }
  if (asked.includes('plan:slow')) {
    await new Promise((resolve) => setTimeout(resolve, 25_000))
    return { content: 'That took a while.' }
  }
  return { content: 'done' }
})

const world = makeWorld('bravebot-prototype-demo-', { service })
const project = realpathSync(world.project)
const pipe = join(world.root, 'phone.input')
execFileSync('mkfifo', [pipe])
const log = join(world.root, 'host.log')

const host = spawn(hostBinary, [
  '--sessions-root', join(world.root, 'sessions'), '--workspace', project,
  '--client', 'desktop', '--answers', 'confirm,run,fetch,ask,trust', '--program', electronPath,
  '--arg', ui, '--arg', '--bravebot-host-client', '--arg', '--host-client-name', '--arg', 'desktop', '--arg', '--host-client-workspace', '--arg', project,
  '--arg', `--user-data-dir=${world.profile}`,
  '--client', 'phone', '--answers', 'confirm,run,fetch,ask', '--program', process.execPath,
  '--arg', phoneScript, '--arg', '--name', '--arg', 'phone', '--arg', '--workspace', '--arg', project, '--arg', '--input', '--arg', pipe,
], { env: world.env, stdio: ['pipe', 'pipe', 'pipe'] })

let answeredFolder = false
host.stderr.setEncoding('utf8')
host.stderr.on('data', (chunk) => {
  appendFileSync(log, chunk)
  process.stdout.write(chunk)
  if (!answeredFolder && chunk.includes('trust this directory? [y/N] ')) {
    answeredFolder = true
    host.stdin.write('n\n')
  }
})
process.stdout.write(`
The desktop window opens in a moment, in the folder ${project}.
The phone's commands go to this pipe:  ${pipe}
What the host and both clients say is in:  ${log}
Try: New session in the window, then  echo 'attach s1' > ${pipe}  and  echo 'send plan:write' > ${pipe}

`)

const stop = () => {
  host.stdin.end('quit\n')
  setTimeout(() => host.kill('SIGKILL'), 10_000).unref()
}
process.on('SIGINT', stop)
process.on('SIGTERM', stop)
await new Promise((resolve) => host.once('exit', resolve))
service.closeAllConnections?.()
service.close()
rmSync(world.root, { recursive: true, force: true })
