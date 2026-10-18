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
import { mkdirSync, writeFileSync, readFileSync, existsSync, chmodSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_INPUT } from './drive-support.mjs'

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
const { service, rounds } = await startModelService(({ messages }) =>
  messages.at(-1)?.role === 'tool'
    ? { content: 'done' }
    : { tool: { name: 'lsp', arguments: { operation: 'goToDefinition', path: 'src/a.rs', line: 1, character: 12 } } })

// makeWorld resolves the root, because the agent names the project by its real path and the card is compared with it.
const { root, project, profile, env } = makeWorld('bravebot-lsp-', { service, dirs: ['project/src', 'bin'] })
const bin = join(root, 'bin')
writeFileSync(join(project, 'src/a.rs'), 'pub struct Held;\n')
const program = join(bin, 'rust-analyzer')
writeFileSync(program, SERVER)
chmodSync(program, 0o755)
const starts = join(root, 'starts')
const started = () => (existsSync(starts) ? readFileSync(starts, 'utf8').split('\n').filter(Boolean).map(Number) : [])
const alive = (pid) => { try { process.kill(pid, 0); return true } catch { return false } }

Object.assign(env, {
  // The script's directory first, so the name the agent looks up is this server and no other.
  PATH: `${bin}:${process.env.PATH}`,
  STARTS: starts, DEFINED_AT: join(project, 'src/a.rs'),
})

const output = '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
let app, page, errors
let closed = false
try {
  ;({ app, page, errors } = await launchWorld({ profile, env }))
  await openProject(page, app, project)

  const composer = page.locator(COMPOSER_INPUT)
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
  if (!closed) await app?.close().catch(() => undefined)
  for (const pid of started()) if (alive(pid)) process.kill(pid)
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
