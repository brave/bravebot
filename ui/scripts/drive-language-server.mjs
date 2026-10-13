// That a language server the planner asks for is put to the window, that a yes given there starts
// one process for the conversation, and that the process does not outlive the app.
//
// The real app and the real bridge, against a model service and a language server this script
// supplies. Nothing is paid for. The server is the witness: it writes its process id as it comes
// up, so how many started, and whether one is still running, is read off the processes themselves.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run drive:language-server` does first. Not for Windows: the server is a shell script.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, chmodSync, rmSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { _electron as electron } from 'playwright-core'

/** A language server that answers what a session asks of one, and says when it came up. */
const SERVER = `#!/bin/sh
echo $$ >> "$STARTS"
reply() {
  printf 'Content-Length: %s\\r\\n\\r\\n%s' "\${#1}" "$1"
}
while IFS= read -r header; do
  case "$header" in
    Content-Length:*) length=$(printf '%s' "$header" | tr -cd '0-9') ;;
    *) continue ;;
  esac
  IFS= read -r blank
  body=$(dd bs=1 count="$length" 2>/dev/null)
  id=$(printf '%s' "$body" | sed -n 's/.*"id":\\([0-9]*\\).*/\\1/p')
  case "$body" in
    *'"initialize"'*)
      reply "{\\"jsonrpc\\":\\"2.0\\",\\"id\\":$id,\\"result\\":{\\"capabilities\\":{}}}"
      reply '{"jsonrpc":"2.0","method":"$/progress","params":{"token":"rustAnalyzer/cachePriming","value":{"kind":"end"}}}'
      ;;
    *'"textDocument/definition"'*)
      reply "{\\"jsonrpc\\":\\"2.0\\",\\"id\\":$id,\\"result\\":[{\\"uri\\":\\"file://$DEFINED_AT\\",\\"range\\":{\\"start\\":{\\"line\\":0,\\"character\\":10},\\"end\\":{\\"line\\":0,\\"character\\":14}}}]}"
      ;;
    *'"shutdown"'*)
      reply "{\\"jsonrpc\\":\\"2.0\\",\\"id\\":$id,\\"result\\":null}"
      ;;
  esac
done
`

// The model service. A planner that asks where a symbol is defined whenever the last thing said
// was not a tool's result, and ends the turn when it was, so every prompt is one question.
const rounds = []
const service = createServer(async (request, response) => {
  let body = ''
  for await (const chunk of request) body += chunk
  if (request.method !== 'POST') {
    response.writeHead(200, { 'Content-Type': 'application/json' })
    return response.end(JSON.stringify({ data: [{ id: 'test' }] }))
  }
  rounds.push(body)
  const answered = (JSON.parse(body).messages ?? []).at(-1)?.role === 'tool'
  const question = { operation: 'goToDefinition', path: 'src/a.rs', line: 1, character: 12 }
  const delta = answered
    ? { role: 'assistant', content: 'done' }
    : { role: 'assistant', tool_calls: [{ index: 0, id: `call-${rounds.length}`, type: 'function', function: { name: 'lsp', arguments: JSON.stringify(question) } }] }
  const chunk = { id: 'c1', object: 'chat.completion.chunk', model: 'test', choices: [{ index: 0, delta, finish_reason: answered ? 'stop' : 'tool_calls' }], usage: { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 } }
  response.writeHead(200, { 'Content-Type': 'text/event-stream' })
  response.end(`data: ${JSON.stringify(chunk)}\n\ndata: [DONE]\n\n`)
})
await new Promise((resolve) => service.listen(0, '127.0.0.1', resolve))

// Resolved, because the agent names the project by its real path and the card is compared with it.
const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-lsp-')))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile'), bin = join(root, 'bin')
for (const path of [join(home, '.bravebot'), join(project, 'src'), profile, bin]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, 'src/a.rs'), 'pub struct Held;\n')
const program = join(bin, 'rust-analyzer')
writeFileSync(program, SERVER)
chmodSync(program, 0o755)
writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({
  provider: { local: { options: { baseURL: `http://127.0.0.1:${service.address().port}/v1` }, models: { test: {} } } },
  model: 'local/test',
}))
const starts = join(root, 'starts')
const started = () => (existsSync(starts) ? readFileSync(starts, 'utf8').split('\n').filter(Boolean).map(Number) : [])
const alive = (pid) => { try { process.kill(pid, 0); return true } catch { return false } }

const env = Object.fromEntries(['DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter((key) => process.env[key]).map((key) => [key, process.env[key]]))
Object.assign(env, {
  // The script's directory first, so the name the agent looks up is this server and no other.
  PATH: `${bin}:${process.env.PATH}`,
  HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost', BRAVEBOT_LOCALE: 'en-US',
  STARTS: starts, DEFINED_AT: join(project, 'src/a.rs'),
})

const output = '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
const app = await electron.launch({
  args: ['.', ...(process.env.CI ? ['--no-sandbox'] : []), ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`],
  env,
  timeout: 40000,
})
let page
let closed = false
try {
  page = await app.firstWindow()
  page.setDefaultTimeout(30000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await app.evaluate(({ dialog }, path) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] })
  }, project)
  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  await trust.getByRole('button', { name: 'Trust this directory' }).click()
  await trust.waitFor({ state: 'hidden' })

  const composer = page.locator('.composer textarea')
  const stop = page.locator('.composer .stop')
  const cards = page.locator('.confirm.server')
  const ask = async (prompt) => {
    await composer.fill(prompt)
    await page.locator('.composer .send').click()
    await stop.waitFor({ state: 'visible' })
  }
  const ended = () => stop.waitFor({ state: 'hidden' })

  // ---- the question, and a yes ---------------------------------------------------------------
  await ask('Where is Held declared?')
  const card = cards.first()
  await card.waitFor()
  assert.match(await card.locator('.confirm-head').innerText(), /Rust language server/)
  const scope = await card.locator('.permission-scope code').allInnerTexts()
  assert.deepEqual(scope.map((text) => text.trim()), [program, project], 'what would run, and the tree it would read')
  assert.match(await card.locator('.warn').innerText(), /code from your dependencies runs with your own access/, 'that it builds is said out loud')
  assert.deepEqual(await card.locator('.confirm-actions button').allInnerTexts(), ['Don’t start', 'Start for this conversation'])
  assert.match(await page.locator('.pending-jump').innerText(), /Approval needed/)
  assert.equal(await page.locator('.pending-jump').getAttribute('data-tooltip'), 'Answer the language server')
  assert.equal(await page.getByRole('button', { name: 'Send', exact: true }).count(), 0, 'nothing can be sent while the question stands: the round button is Stop')
  assert.deepEqual(started(), [], 'nothing started before anybody answered')
  await page.screenshot({ path: join(output, 'server-asked.png') })

  await card.getByRole('button', { name: 'Start for this conversation', exact: true }).click()
  await card.locator('.decided.approve').waitFor()
  assert.match(await card.locator('.decided').innerText(), /You started this server for the conversation/)
  await ended()
  assert.equal(started().length, 1, 'a yes started one process')
  const [pid] = started()
  assert.ok(alive(pid), 'and it is running')
  assert.match(rounds.at(-1), /src\/a\.rs:1:11/, 'and what it answered reached the planner as a place in a file')
  await page.screenshot({ path: join(output, 'server-started.png') })

  // ---- the next message, which asks nobody ---------------------------------------------------
  await ask('And once more?')
  await ended()
  assert.equal(await cards.count(), 1, 'the second message put no second question')
  assert.deepEqual(started(), [pid], 'and started no second process')
  assert.ok(alive(pid), 'the server is still the conversation’s')
  assert.match(rounds.at(-1), /src\/a\.rs:1:11/, 'and it answered again')
  await page.screenshot({ path: join(output, 'server-kept.png') })

  assert.deepEqual(errors, [])

  // ---- the app closing ------------------------------------------------------------------------
  await app.close()
  closed = true
  const deadline = Date.now() + 10000
  while (alive(pid) && Date.now() < deadline) await delay(100)
  assert.ok(!alive(pid), 'the server did not outlive the app that started it')

  console.log(`PASS: a language server is put to the window with what would run, a yes starts one process for the conversation, the next message asks nobody, and the process ends with the app. Screenshots in ${output}/server-*.png`)
} catch (error) {
  if (page && !closed) await page.screenshot({ path: join(output, 'server-failure.png') }).catch(() => undefined)
  throw error
} finally {
  if (!closed) await app.close().catch(() => undefined)
  for (const pid of started()) if (alive(pid)) process.kill(pid)
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
