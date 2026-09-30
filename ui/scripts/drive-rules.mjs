// That the permission rules in the settings files govern a conversation in the window: a host a
// rule refuses is not fetched and not asked about, a host the person's own file allows is
// fetched without a question, and what a file wrote that is not in force is said.
//
// The real app and the real bridge, against a model service and a website this script serves
// itself. Nothing is paid for. Whether a fetch went out is read off the website.
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`), which
// `npm run drive:rules` does first.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

const listening = (server) => new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))

// The website, under two names. The person's file allows one and refuses the other.
const reached = []
const site = createServer((request, response) => {
  reached.push(`${request.headers.host?.split(':')[0]} ${request.method} ${request.url}`)
  response.writeHead(200, { 'Content-Type': 'text/plain' })
  response.end('a page\n')
})
await listening(site)
const port = site.address().port
const allowed = `http://127.0.0.1:${port}/allowed`
const refused = `http://localhost:${port}/refused`

// The model service. A planner that fetches the address named in the last prompt, and ends the
// turn once a tool has answered.
const rounds = []
const service = createServer(async (request, response) => {
  let body = ''
  for await (const chunk of request) body += chunk
  if (request.method !== 'POST') {
    response.writeHead(200, { 'Content-Type': 'application/json' })
    return response.end(JSON.stringify({ data: [{ id: 'test' }] }))
  }
  rounds.push(body)
  const messages = JSON.parse(body).messages ?? []
  const answered = messages.at(-1)?.role === 'tool'
  const asked = JSON.stringify(messages.filter((message) => message.role === 'user').at(-1) ?? '')
  const url = asked.match(/http:\/\/[a-z0-9.]+:\d+\/[a-z]+/)?.[0]
  const delta = answered || !url
    ? { role: 'assistant', content: 'done' }
    : { role: 'assistant', tool_calls: [{ index: 0, id: `call-${rounds.length}`, type: 'function', function: { name: 'fetch_url', arguments: JSON.stringify({ url }) } }] }
  const chunk = { id: 'c1', object: 'chat.completion.chunk', model: 'test', choices: [{ index: 0, delta, finish_reason: delta.tool_calls ? 'tool_calls' : 'stop' }], usage: { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 } }
  response.writeHead(200, { 'Content-Type': 'text/event-stream' })
  response.end(`data: ${JSON.stringify(chunk)}\n\ndata: [DONE]\n\n`)
})
await listening(service)

const toolResults = (round) => (JSON.parse(round).messages ?? []).filter((message) => message.role === 'tool').map((message) => message.content).join('\n')

const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-rules-')))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), join(project, '.bravebot'), profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({
  provider: { local: { options: { baseURL: `http://127.0.0.1:${service.address().port}/v1` }, models: { test: {} } } },
  model: 'local/test',
  permissions: {
    allow: ['WebFetch(domain:127.0.0.1)'],
    deny: ['WebFetch(domain:localhost)', 'Fetchh(domain:localhost)'],
  },
}))
const checkout = join(project, '.bravebot/settings.json')
writeFileSync(checkout, JSON.stringify({
  permissions: { allow: ['WebFetch(domain:localhost)'], additionalDirectories: ['../other'] },
}))
const env = Object.fromEntries(['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter((key) => process.env[key]).map((key) => [key, process.env[key]]))
Object.assign(env, { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost', BRAVEBOT_LOCALE: 'en-US' })

const output = '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
const app = await electron.launch({
  args: ['.', ...(process.env.CI ? ['--no-sandbox'] : []), ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`],
  env,
  timeout: 40000,
})
let page
try {
  page = await app.firstWindow()
  page.setDefaultTimeout(30000)
  await page.setViewportSize({ width: 1400, height: 1000 })
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
  const ask = async (prompt) => {
    await composer.fill(prompt)
    await page.locator('.composer .send').click()
    await stop.waitFor({ state: 'visible' })
    await stop.waitFor({ state: 'hidden' })
  }

  // ---- what is not in force is said as the conversation opens -------------------------------
  const banner = page.locator('.rules-banner')
  await banner.waitFor()
  assert.match(await banner.locator('summary').innerText(), /3 permission settings are not in force/)
  await banner.locator('summary').click()
  const said = await banner.innerText()
  assert.ok(said.includes('Fetchh(domain:localhost)'), 'the entry that is not a rule is named')
  assert.ok(said.includes('WebFetch(domain:localhost)') && said.includes(checkout), 'the project’s allow rule is named with its file')
  assert.ok(said.includes('You will still be asked'))
  assert.ok(said.includes('../other'), 'the directory a file named is said not to be open')
  await page.screenshot({ path: join(output, 'rules-banner.png') })
  await banner.locator('summary').click()

  // ---- a host the person's own file allows ----------------------------------------------------
  await ask(`Fetch ${allowed}`)
  assert.equal(await page.locator('.confirm.fetch').count(), 0, 'a host the person allowed was not asked about')
  assert.deepEqual(reached, ['127.0.0.1 GET /allowed'], 'and was fetched')

  // ---- a host a rule refuses, which the project's file tried to allow ---------------------------
  await ask(`Fetch ${refused}`)
  assert.equal(await page.locator('.confirm.fetch').count(), 0, 'a host a rule refuses was not asked about')
  assert.deepEqual(reached, ['127.0.0.1 GET /allowed'], 'and was not fetched')
  assert.match(toolResults(rounds.at(-1)), /refused/, 'and the planner was told a rule refused it')
  await page.screenshot({ path: join(output, 'rules-refused.png') })

  // ---- the rules in force, in the permissions dialog ----------------------------------------
  await page.getByRole('button', { name: 'Permissions', exact: true }).click()
  const dialog = page.getByRole('dialog', { name: 'Conversation permissions', exact: true })
  await dialog.getByText('Rules from settings files', { exact: true }).waitFor()
  const listed = await dialog.innerText()
  assert.ok(listed.includes('WebFetch(domain:localhost)'), 'the rule that refuses is listed')
  assert.ok(listed.includes('WebFetch(domain:127.0.0.1)'), 'the rule that allows is listed')
  assert.ok(!listed.includes('Fetchh'), 'an entry that is not a rule is not listed as in force')
  assert.equal(await dialog.locator('.settings-rules button').count(), 0, 'a rule has nothing to press')
  await page.screenshot({ path: join(output, 'rules-dialog.png') })

  assert.deepEqual(errors, [])
  console.log(`PASS: what is not in force is said as the conversation opens, a host the person allowed is fetched unasked, a host a rule refuses is neither asked about nor fetched, and the rules in force are listed. Screenshots in ${output}/rules-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'rules-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app.close()
  await new Promise((resolve) => site.close(resolve))
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
