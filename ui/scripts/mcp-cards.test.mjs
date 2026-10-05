// What the window shows when an MCP server is started, offers its tools, is called, or moves.
//
// docs/specs/mcp-servers.md puts four questions to the person: whether to use a server
// (SERVERS-4), whether to offer its tools (SERVERS-8), whether to make a call (SERVERS-7), and
// whether a remote server moved (SERVERS-11). These tests hold each card to what it must show and
// to the answers it must offer.
//
// The cards are rendered through `react-dom/server` and the markup is what is asserted on.

import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

function load(path) {
  const source = buildSync({
    entryPoints: [path],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { Row } = load('src/renderer/components/Transcript.tsx')
const t = load('src/renderer/transcript.ts')

const draw = (entry, onDecide = () => {}) =>
  renderToStaticMarkup(
    React.createElement(Row, { entry, onDecide, onAnswer() {}, onFork() {}, forkable: false }),
  )

const buttons = (markup) => [...markup.matchAll(/<button[^>]*>(.*?)<\/button>/g)].map((found) => found[1])

const server = (overrides = {}) =>
  t.askedMcpServer({
    request: 4,
    alias: 'weather',
    transport: 'stdio',
    command: ['npx', '-y', 'weather-mcp@latest'],
    url: null,
    program: '/usr/local/bin/npx',
    variables: [
      { name: 'WEATHER_KEY', stored: true },
      { name: 'PATH', stored: false },
    ],
    reads: ['/home/me/keys/weather'],
    directory: null,
    digest: 'abc123def456',
    requestedBy: '.bravebot/settings.json',
    changed: false,
    fetching: ['npx fetches what it runs when it starts', 'weather-mcp@latest names no exact version, so it runs whatever is published under it'],
    ...overrides,
  })

const tools = (overrides = {}) =>
  t.askedMcpTools({
    request: 5,
    alias: 'weather',
    tools: [{ name: 'weather:get_forecast', arguments: ['city (string, required)'], description: 'the forecast for a city' }],
    refused: 0,
    changed: false,
    vetting: { verdict: 'safe' },
    ...overrides,
  })

const call = (overrides = {}) =>
  t.askedMcpCall({
    request: 6,
    alias: 'weather',
    tool: 'get_forecast',
    name: 'weather:get_forecast',
    arguments: [{ name: 'city', value: '"Paris"' }],
    description: 'the forecast for a city',
    mayStand: true,
    ...overrides,
  })

const move = (overrides = {}) =>
  t.askedMcpMove({
    request: 7,
    alias: 'weather',
    declared: 'https://weather.example/mcp',
    destination: 'https://elsewhere.example/mcp',
    authority: 'elsewhere.example:443',
    mayRecord: true,
    ...overrides,
  })

test('the server card shows the whole declaration, and a stored value by its name only', () => {
  const markup = draw(server())

  assert.ok(markup.includes('<code class="path">weather</code>'), markup)
  assert.ok(markup.includes('.bravebot/settings.json'), markup)
  assert.ok(markup.includes('npx -y weather-mcp@latest'), markup)
  assert.ok(markup.includes('/usr/local/bin/npx'), markup)
  assert.ok(markup.includes('<code>WEATHER_KEY</code> (stored)'), markup)
  assert.ok(markup.includes('<code>PATH</code>'), markup)
  assert.ok(markup.includes('/home/me/keys/weather'), markup)
  assert.ok(markup.includes('abc123def456'), markup)
})

test('a server that fetches what it runs is warned about and bordered', () => {
  const markup = draw(server())
  assert.ok(markup.includes('confirm mcp-server fetches'), markup)
  assert.ok(markup.includes('names no exact version'), markup)

  const pinned = draw(server({ fetching: [] }))
  assert.ok(!pinned.includes('fetches'), pinned)
})

test('a changed declaration says so', () => {
  assert.ok(draw(server({ changed: true })).includes('declaration changed since you approved it'))
  assert.ok(!draw(server()).includes('declaration changed'))
})

test('a remote server shows the address it reaches and no command', () => {
  const markup = draw(server({ transport: 'http', command: null, url: 'https://docs.example/mcp', program: null, variables: [], reads: [], fetching: [] }))
  assert.ok(markup.includes('<strong>Reaches:</strong> <code class="mcp-url">https://docs.example/mcp</code>'), markup)
  assert.ok(!markup.includes('Runs:'), markup)
})

test('the server card offers no, yes and the project-wide yes, each its own press', () => {
  const markup = draw(server())
  assert.deepEqual(buttons(markup), ['Continue without it', 'Use it and all future servers in this project', 'Use this server'])
  // The standing answer says what it covers where it can be pressed, not only after.
  assert.ok(markup.includes('until you run bravebot mcp forget here'), markup)
})

test('an answered server card says which of the three answers was given', () => {
  assert.ok(draw({ ...server(), decision: 'approve', remember: false }).includes('You approved this server<'))
  assert.ok(draw({ ...server(), decision: 'approve', remember: true }).includes('every server this project requests'))
  assert.ok(draw({ ...server(), decision: 'reject', remember: false }).includes('You left this server out'))
  assert.deepEqual(buttons(draw({ ...server(), decision: 'approve', remember: true })), [])
})

test('the tools card draws each tool and its description as the server wrote it, set apart', () => {
  const markup = draw(tools())
  assert.ok(markup.includes('<code class="mcp-tool-name">weather:get_forecast</code>'), markup)
  assert.ok(markup.includes('city (string, required)'), markup)
  assert.ok(markup.includes('class="mcp-tool-description"'), markup)
  assert.ok(markup.includes('the forecast for a city'), markup)
  assert.deepEqual(buttons(markup), ['Continue without them', 'Offer these tools'])
})

test('a list that changed, or holds tools that cannot be offered, says so', () => {
  assert.ok(draw(tools({ changed: true })).includes('not the list you approved before'))
  assert.ok(draw(tools({ refused: 2 })).includes('2 more tools are not listed'))
  assert.ok(draw(tools({ refused: 1 })).includes('1 more tool is not listed'))
})

test('a description is drawn as text and never as markup', () => {
  const forged = '<img src="https://example.com/x.png"><a href="https://example.com">x</a>'
  for (const markup of [
    draw(tools({ tools: [{ name: 'weather:get_forecast', arguments: [], description: forged }] })),
    draw(call({ description: forged, arguments: [{ name: 'city', value: forged }] })),
    draw(move({ destination: forged })),
  ]) {
    for (const forbidden of ['<img', '<a ', '<a>']) assert.ok(!markup.includes(forbidden), `drew ${forbidden}: ${markup}`)
    assert.ok(markup.includes('&lt;img src=&quot;https://example.com/x.png&quot;&gt;'), markup)
  }
})

test('the call card shows each argument as the model wrote it', () => {
  const markup = draw(call())
  assert.ok(markup.includes('<code class="path">weather:get_forecast</code>'), markup)
  assert.ok(markup.includes('<code>city</code>'), markup)
  assert.ok(markup.includes('&quot;Paris&quot;'), markup)
  assert.ok(draw(call({ arguments: [] })).includes('No arguments.'))
})

test('the call card offers to stop asking only where that can be recorded', () => {
  assert.deepEqual(buttons(draw(call())), ['Don’t call', 'Call, and stop asking for get_forecast here', 'Call once'])
  const unrecorded = draw(call({ mayStand: false }))
  assert.deepEqual(buttons(unrecorded), ['Don’t call', 'Call once'])
  assert.ok(unrecorded.includes('cannot stop asking'), unrecorded)
})

test('the move card shows the declaration, the destination and the host a yes reaches', () => {
  const markup = draw(move())
  assert.ok(markup.includes('https://weather.example/mcp'), markup)
  assert.ok(markup.includes('<code class="mcp-destination">https://elsewhere.example/mcp</code>'), markup)
  assert.ok(markup.includes('<strong>Reaching:</strong> <code>elsewhere.example:443</code>'), markup)
  assert.ok(markup.includes('Nothing was sent there'), markup)
  assert.ok(markup.includes('in this conversation and the next'), markup)
  assert.ok(draw(move({ mayRecord: false })).includes('until this conversation ends'))
  assert.deepEqual(buttons(markup), ['Don’t move it', 'It moved there'])
})

test('each card takes an answer to itself only, and the third answer only from an approval', () => {
  const kinds = { 'mcp-server': server(), 'mcp-tools': tools(), 'mcp-call': call(), 'mcp-move': move() }
  for (const [kind, entry] of Object.entries(kinds)) {
    const entries = [entry]
    assert.equal(t.outstanding(entries).kind, kind)
    assert.equal(t.REPLY[kind], `${kind}.reply`)
    for (const other of Object.keys(t.REPLY).filter((each) => each !== kind)) {
      assert.equal(t.decide(entries, other, entry.request.request, 'approve')[0].decision, null, `${other} answered ${kind}`)
    }
    assert.equal(t.decide(entries, kind, entry.request.request + 100, 'approve')[0].decision, null)
    assert.equal(t.decide(entries, kind, entry.request.request, 'approve')[0].decision, 'approve')
  }
  assert.equal(t.decide([server()], 'mcp-server', 4, 'approve', true)[0].remember, true)
  assert.equal(t.decide([server()], 'mcp-server', 4, 'reject', true)[0].remember, false)
  assert.equal(t.decide([call()], 'mcp-call', 6, 'reject', true)[0].remember, false)
})

test('a question whose turn ended cannot be answered from its card', () => {
  for (const entry of [server(), tools(), call(), move()]) {
    const markup = draw({ ...entry, interrupted: true })
    assert.deepEqual(buttons(markup), [], entry.kind)
    assert.ok(markup.includes('Nobody answered this'), entry.kind)
  }
})

test('what starting the servers came to is said, with a line for each request that did not start', () => {
  const started = draw(t.mcpStarted({ servers: ['weather'], confined: true, notes: ['docs is not used in this session'] }))
  assert.ok(started.includes('MCP server started: weather'), started)
  assert.ok(started.includes('docs is not used in this session'), started)
  assert.ok(draw(t.mcpStarted({ servers: [], confined: false, notes: [] })).includes('No MCP server started'))
})

test('the MCP cards are left out of an export and cannot be copied as a message', () => {
  for (const entry of [server(), tools(), call(), move(), t.mcpStarted({ servers: [], confined: false, notes: [] })]) {
    assert.deepEqual(t.conversation([entry], true), [], entry.kind)
    assert.equal(t.plainText(entry), null, entry.kind)
  }
})
