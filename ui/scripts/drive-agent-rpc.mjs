// Exercise a real watch-triggered turn and hooks against a local deterministic gateway.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { spawn } from 'node:child_process'
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { createInterface } from 'node:readline'
import { setTimeout as delay } from 'node:timers/promises'
const root = mkdtempSync(join(tmpdir(), 'bravebot-rpc-09-'))
const home = join(root, 'home'), project = join(root, 'project')
mkdirSync(join(home, '.bravebot'), { recursive: true }); mkdirSync(project)
const requests = [], messages = [], pending = new Map()
let child, id = 0
const server = createServer(async (request, response) => {
  let body = ''
  for await (const chunk of request) body += chunk
  requests.push({ url: request.url, headers: request.headers, body: JSON.parse(body) })
  response.writeHead(200, { 'Content-Type': 'text/event-stream' })
  response.end(`data: ${JSON.stringify({ model: 'test', choices: [{ index: 0, delta: { content: 'The watch fired. No file content was read.' }, finish_reason: 'stop' }], usage: { prompt_tokens: 12, completion_tokens: 9, total_tokens: 21 } })}\n\ndata: [DONE]\n\n`)
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const call = (method, params = {}) => new Promise((resolve, reject) => {
  const key = ++id
  const timer = setTimeout(() => { pending.delete(key); reject(new Error(`${method} timed out`)) }, 15000)
  pending.set(key, { resolve: value => { clearTimeout(timer); resolve(value) }, reject: error => { clearTimeout(timer); reject(error) } })
  child.stdin.write(JSON.stringify({ id: key, method, params }) + '\n')
})
async function until(predicate, label) {
  const deadline = Date.now() + 20000
  while (!predicate()) { if (Date.now() > deadline) throw new Error(`${label} timed out: ${JSON.stringify(messages.slice(-4))}`); await delay(50) }
}
try {
  const override = join(root, 'settings.json')
  writeFileSync(override, JSON.stringify({ provider: { local: { options: { baseURL: `http://127.0.0.1:${server.address().port}/v1` }, models: { test: {} } } }, model: 'local/test' }))
  const hookLog = join(root, 'hooks.log')
  const hookScript = join(root, 'hook.cjs')
  writeFileSync(hookScript, 'require("node:fs").appendFileSync(process.argv[2], process.argv[3] + "\\n")')
  writeFileSync(join(home, '.bravebot/hooks.json'), JSON.stringify({ hooks: [
    { on: 'turn-started', run: [process.execPath, hookScript, hookLog, 'started'] },
    { on: 'turn-finished', run: [process.execPath, hookScript, hookLog, 'finished'] },
  ] }))
  writeFileSync(join(project, 'watched.txt'), 'Original private bytes')
  child = spawn(resolve('target/debug/bravebot-rpc'), ['--settings', override], {
    cwd: project, env: { PATH: process.env.PATH, HOME: home, NO_PROXY: '127.0.0.1,localhost', BRAVEBOT_DEFAULT_MODEL: 'local/test' }, stdio: ['pipe', 'pipe', 'pipe'],
  })
  let stderr = ''
  child.stderr.on('data', chunk => { stderr += chunk })
  createInterface({ input: child.stdout }).on('line', line => {
    const message = JSON.parse(line)
    if (message.id !== undefined) {
      const waiter = pending.get(message.id); pending.delete(message.id)
      if (message.error) waiter?.reject(new Error(message.error.message)); else waiter?.resolve(message.ok)
    } else messages.push(message)
  })
  await until(() => messages.some(m => m.event === 'agent.ready'), 'ready')
  const { session } = await call('session.new', { directory: project })
  await call('trust.reply', { session, trusted: false })
  await call('watches.add', { session, path: 'watched.txt' })
  writeFileSync(join(project, 'watched.txt'), 'NEVER PLACE THESE PRIVATE BYTES IN THE GENERATED PROMPT')
  await delay(5100)
  await call('watches.poll')
  await until(() => messages.some(m => m.event === 'turn.done' || m.event === 'turn.error'), 'automatic turn')
  const done = messages.find(m => m.event === 'turn.done')
  assert(done, JSON.stringify(messages.filter(m => m.event === 'turn.error')) + stderr)
  assert.equal(done.session, session)
  assert.equal(done.data.contextTokens, 12)
  assert.equal(requests.length, 1)
  assert.equal(requests[0].headers.authorization, undefined)
  assert(JSON.stringify(requests[0].body.messages).includes('Watch 1 fired: watched.txt'))
  assert(!JSON.stringify(requests).includes('NEVER PLACE THESE PRIVATE BYTES'))
  assert.equal(readFileSync(hookLog, 'utf8'), 'started\nfinished\n')
  await call('watches.stop', { session, all: true })
  assert.deepEqual((await call('watches.list', { session })).watches, [])
  await call('session.close', { session })
  assert(messages.findIndex(m => m.event === 'watch.fired') < messages.findIndex(m => m.event === 'turn.started'))
  console.log('PASS: real watch-triggered turn, local gateway without credentials, context measurement, lifecycle hooks, private-content isolation and stop/close')
} finally {
  if (child) { child.stdin.end(); child.kill(); await new Promise(resolve => child.once('close', resolve)) }
  await new Promise(resolve => server.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
