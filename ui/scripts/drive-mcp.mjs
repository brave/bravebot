// That the MCP servers a project requests are started from the window, and that their tools are
// offered and called only on the answers given there (docs/specs/mcp-servers.md SERVERS-4,
// SERVERS-7, SERVERS-8).
//
// The real app and the real bridge, against a model service and an MCP server this script serves
// itself. Nothing is paid for. The server is a remote one, so nothing needs confining.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run drive:mcp` does first.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, listening, makeWorld, openProject, startModelService } from './drive-world.mjs'

const WIRE = 'mcp__weather__get_forecast'

// The MCP server: one tool, and every method it was sent, in order.
const methods = []
const weather = createServer(async (request, response) => {
  let body = ''
  for await (const chunk of request) body += chunk
  const call = JSON.parse(body || '{}')
  methods.push(call.method)
  const result = call.method === 'initialize'
    ? { protocolVersion: '2025-06-18', capabilities: {}, serverInfo: { name: 'weather', version: '1' } }
    : call.method === 'tools/list'
      ? { tools: [{ name: 'get_forecast', description: 'The forecast for a city.', inputSchema: { type: 'object', properties: { city: { type: 'string' } }, required: ['city'] } }] }
      : call.method === 'tools/call'
        ? { content: [{ type: 'text', text: 'sunny' }] }
        : {}
  response.writeHead(200, { 'Content-Type': 'application/json' })
  response.end(JSON.stringify({ jsonrpc: '2.0', id: call.id ?? null, result }))
})
await listening(weather)
const url = `http://127.0.0.1:${weather.address().port}/mcp`

// The model service. Calls the weather tool wherever it is offered and the last thing said was
// not a tool's result, and otherwise ends the turn.
const { service, rounds } = await startModelService(({ body, messages }) => {
  const offered = (JSON.parse(body).tools ?? []).some((tool) => tool.function?.name === WIRE || tool.name === WIRE)
  return offered && messages.at(-1)?.role !== 'tool' ? { tool: { name: WIRE, arguments: { city: 'Paris' } } } : { content: 'done' }
})

const { root, home, project, profile, env } = makeWorld('bravebot-mcp-', { service, dirs: ['project/.bravebot'] })
writeFileSync(join(home, '.bravebot/mcp.json'), JSON.stringify({ servers: { weather: { transport: 'http', url } } }))
writeFileSync(join(project, '.bravebot/settings.json'), JSON.stringify({ mcp: { request: ['weather'] } }))

const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 1000 } }))
  await openProject(page, app, project)
  assert.equal(methods.length, 0, 'opening a session starts no server')

  const composer = page.locator('.composer textarea')
  const stop = page.locator('.composer .stop')
  const ask = async (prompt) => {
    await composer.fill(prompt)
    await page.locator('.composer .send').click()
    await stop.waitFor({ state: 'visible' })
  }
  const ended = () => stop.waitFor({ state: 'hidden' })

  // ---- whether to use the server -------------------------------------------------------------
  await ask('What is the weather in Paris?')
  const server = page.locator('.confirm.mcp-server').first()
  await server.waitFor()
  assert.equal((await server.locator('.confirm-head .path').innerText()).trim(), 'weather')
  assert.match(await server.innerText(), new RegExp(url.replace(/[.]/g, '\\.')))
  assert.match(await server.innerText(), /\.bravebot\/settings\.json/)
  assert.deepEqual(await server.locator('.confirm-actions button').allInnerTexts(), ['Continue without it', 'Use it and all future servers in this project', 'Use this server'])
  assert.equal(await page.locator('.pending-jump').getAttribute('data-tooltip'), 'Answer the MCP server')
  assert.equal(methods.length, 0, 'nothing reached the server before the answer')
  await page.screenshot({ path: join(output, 'mcp-server.png') })
  await server.getByRole('button', { name: 'Use this server', exact: true }).click()
  await server.locator('.decided.approve').waitFor()

  // ---- what started, and whether to offer its tools -------------------------------------------
  await page.locator('.mcp-started').first().waitFor()
  assert.match(await page.locator('.mcp-started').first().innerText(), /MCP server started: weather/)
  const tools = page.locator('.confirm.mcp-tools').first()
  await tools.waitFor()
  assert.equal((await tools.locator('.mcp-tool-name').innerText()).trim(), 'weather:get_forecast')
  assert.equal((await tools.locator('.mcp-tool-description').innerText()).trim(), 'The forecast for a city.')
  // A check reads the list before it is put to the person (CHECK-10), so the description does reach
  // a model. What it must not reach is the planner, whose requests are the ones offering tools.
  const planner = () => rounds.filter((round) => (JSON.parse(round).tools ?? []).length > 0)
  assert.ok(planner().every((round) => !round.includes('The forecast for a city.')), 'the description reached the planner before the yes')
  await page.screenshot({ path: join(output, 'mcp-tools.png') })
  await tools.getByRole('button', { name: 'Offer these tools', exact: true }).click()
  await tools.locator('.decided.approve').waitFor()

  // ---- the call ------------------------------------------------------------------------------
  const call = page.locator('.confirm.mcp-call').first()
  await call.waitFor()
  assert.equal((await call.locator('.confirm-head .path').innerText()).trim(), 'weather:get_forecast')
  assert.match(await call.innerText(), /"Paris"/)
  assert.ok(!methods.includes('tools/call'), 'the call waited for the answer')
  await page.screenshot({ path: join(output, 'mcp-call.png') })
  await call.getByRole('button', { name: 'Call once', exact: true }).click()
  await call.locator('.decided.approve').waitFor()
  await ended()
  assert.ok(methods.includes('tools/call'), 'a yes made the call')
  assert.match(readFileSync(join(home, '.bravebot/mcp-approved'), 'utf8'), /weather/)

  // ---- the next message: the server is held, the list is not asked about, the call is ---------
  const before = methods.length
  await ask('And tomorrow?')
  const again = page.locator('.confirm.mcp-call').nth(1)
  await again.waitFor()
  assert.equal(await page.locator('.confirm.mcp-server').count(), 1, 'the server was not asked about again')
  assert.equal(await page.locator('.confirm.mcp-tools').count(), 1, 'the list was not asked about again')
  await again.getByRole('button', { name: 'Don’t call', exact: true }).click()
  await again.locator('.decided.reject').waitFor()
  await ended()
  assert.ok(!methods.slice(before).includes('tools/call'), 'a no made no call')
  assert.ok(!methods.slice(before).includes('initialize'), 'the server was not started again')
  await page.screenshot({ path: join(output, 'mcp-refused-call.png') })

  assert.deepEqual(errors, [])
  console.log(`PASS: the first turn asks to use the server and starts it, its tools are offered only on a yes, each call is asked about, and the session keeps the server. Screenshots in ${output}/mcp-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'mcp-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
  await new Promise((resolve) => weather.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
