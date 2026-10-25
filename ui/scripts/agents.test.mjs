// VIEW-36: the window counts the agents a conversation has started, offers them from the sidebar,
// and lists each with its kind, its task and whether it is working, finished or did not finish.
//
// The `agents` event is folded by the window's own reducer, and the components are rendered through
// `react-dom/server`, whose one pass is the first paint. `Modal` draws through a portal, which that
// cannot render, so here it is drawn in place.

import test from 'node:test'
import assert from 'node:assert/strict'
import { build, buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)((id) => require(id), module, module.exports)
  return module.exports
}

function session() {
  const { apply } = load('src/renderer/apply.ts')
  const state = {
    live: { handle: 's-1', summary: { title: '', project: '', branch: null, directory: '/', id: 'a' }, entries: [], turns: {}, todos: [], quarantine: [], phase: null, checking: null, hook: null, composing: null, thinking: '', tokens: 0, running: false, archived: 0, awaitingOrdinal: null, bot: null, agents: [] },
  }
  state.send = (event, data) => apply({ event, data }, (update) => { state.live = update(state.live) }, () => {}, () => {})
  return state
}

const agent = (id, kind, task, state) => ({ id, kind, task, state })

/** The event the bridge sends: the whole list, with its two counts. */
const listing = (...agents) => ({ agents, started: agents.length, working: agents.filter((one) => one.state === 'working').length })

const states = (live) => live.agents.map((one) => `${one.id}:${one.state}`)

test('the agents the window holds are the list the last agents event carried', () => {
  const s = session()
  s.send('turn.started', { turn: 1 })
  const first = [agent('d1', 'checker', 'run the tests', 'working'), agent('d2', 'reader', 'read the spec', 'working')]
  s.send('agents', listing(...first))
  assert.deepEqual(s.live.agents, first)
  const second = [agent('d1', 'checker', 'run the tests', 'failed'), agent('d2', 'reader', 'read the spec', 'finished')]
  s.send('agents', listing(...second))
  assert.deepEqual(s.live.agents, second, 'the second list was merged into the first rather than replacing it')
})

test('an event with fewer entries shrinks the list, and an empty one empties it', () => {
  const s = session()
  s.send('agents', listing(agent('d1', 'reader', 'a', 'finished'), agent('d2', 'reader', 'b', 'finished'), agent('d3', 'reader', 'c', 'working')))
  assert.equal(s.live.agents.length, 3)
  s.send('agents', listing(agent('d1', 'reader', 'a', 'finished')))
  assert.deepEqual(states(s.live), ['d1:finished'], 'an agent the event left out was kept')
  s.send('agents', listing())
  assert.deepEqual(s.live.agents, [])
})

test('a number sent twice is two agents, in the order sent', () => {
  const s = session()
  s.send('agents', listing(agent('d1', 'reader', 'first task', 'finished'), agent('d1', 'checker', 'second task', 'working')))
  assert.deepEqual(states(s.live), ['d1:finished', 'd1:working'], 'the second d1 replaced or merged into the first')
  assert.deepEqual(s.live.agents.map((one) => one.task), ['first task', 'second task'])
})

test('the window computes no state: what the event says is what is held, whatever else arrives', () => {
  const sent = listing(agent('d1', 'reader', 'look', 'working'), agent('d2', 'checker', 'test', 'finished'), agent('d3', 'worker', 'fix', 'failed'))
  const ends = [
    ['turn.done', { turn: 1, prompt: 0, reply: 'ok', contextTokens: 0, archived: 0, id: null }],
    ['turn.error', { kind: 'cancelled', message: 'x', category: 'other', attempts: 1, status: null, turn: 1, prompt: 0, contextTokens: 0, id: null }],
    ['manifest.done', { run: 1, reply: 'done', model: 'm', steps: 1, clean: true, tokens: 0, outputTokens: 0, attempt: null, record: null, trust: { rules: [] } }],
    ['manifest.error', { run: 1, kind: 'failed', message: 'x', category: 'other', stopped: false, declined: false, problem: null, attempt: null, record: null }],
  ]
  for (const [event, data] of ends) {
    const s = session()
    s.send(event === 'turn.done' || event === 'turn.error' ? 'turn.started' : 'manifest.started', event.startsWith('turn') ? { turn: 1 } : { run: 1 })
    s.send('agents', sent)
    s.send(event, data)
    assert.deepEqual(s.live.agents, sent.agents, `${event} changed an agent the bridge had not changed`)
    assert.deepEqual(states(s.live), ['d1:working', 'd2:finished', 'd3:failed'], `${event} ended an agent in the window`)
  }
})

test('the agents survive the other events of a turn unchanged', () => {
  const s = session()
  s.send('agents', listing(agent('d1', 'reader', 'look', 'working')))
  s.send('turn.started', { turn: 2 })
  s.send('narration', { text: 'hello' })
  assert.deepEqual(states(s.live), ['d1:working'])
})

test('a new conversation starts with no agents', () => {
  const { newLive } = load('src/renderer/live.ts')
  const live = newLive({
    model: null, handle: 'h', summary: { id: null, title: '', project: '', branch: null, directory: '/' },
    entries: [], todos: [], askingTrust: null, keepingTrust: null, shownKeepingTrust: null, confinedLinesNotice: null,
    trustRemembered: null, forkedFrom: null, focus: null, bot: null, archived: 0, autoVetting: false,
    permissionMode: 'ask', sandboxMode: 'on',
  })
  assert.deepEqual(live.agents, [])
  assert.notEqual(newLive({ ...live }).agents, live.agents, 'two conversations share one list')
})

test('the counts the sidebar draws follow the list the window holds', () => {
  const { tally } = load('src/renderer/agents.ts')
  assert.deepEqual(tally([]), { started: 0, working: 0 })
  const sent = listing(agent('d1', 'reader', 'a', 'working'), agent('d1', 'reader', 'b', 'working'), agent('d2', 'checker', 'c', 'failed'), agent('d3', 'worker', 'd', 'finished'))
  const s = session()
  s.send('agents', sent)
  assert.deepEqual(tally(s.live.agents), { started: sent.started, working: sent.working })
  assert.deepEqual(tally(s.live.agents), { started: 4, working: 2 })
})

const inPlaceModal = {
  name: 'in-place-modal',
  setup(built) {
    // The two lists read stores and the disk, which one server pass cannot; the entry under test is not in them.
    built.onResolve({ filter: /^\.\/(Sessions|Bots)$/ }, (args) => ({ path: args.path, namespace: 'empty-list' }))
    built.onLoad({ filter: /.*/, namespace: 'empty-list' }, () => ({
      loader: 'js',
      contents: 'export const Sessions = () => null; export const Bots = () => null',
    }))
    built.onResolve({ filter: /^\.\/Modal$/ }, () => ({ path: 'modal', namespace: 'in-place-modal' }))
    built.onLoad({ filter: /.*/, namespace: 'in-place-modal' }, () => ({
      loader: 'js',
      resolveDir: process.cwd(),
      contents: `import { createElement } from 'react'
        export function Modal({ title, children, actions }) { return createElement('div', { className: 'modal', 'data-title': title }, children, actions) }`,
    }))
  },
}

let bundle
async function components() {
  bundle ??= build({
    stdin: {
      contents: "export { Agents } from './src/renderer/components/Agents.tsx'\nexport { Sidebar } from './src/renderer/components/Sidebar.tsx'",
      resolveDir: process.cwd(),
      loader: 'ts',
    },
    bundle: true, write: false, platform: 'node', format: 'cjs', jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
    plugins: [inPlaceModal],
  }).then((built) => {
    const module = { exports: {} }
    new Function('require', 'module', 'exports', built.outputFiles[0].text)(require, module, module.exports)
    return module.exports
  })
  return bundle
}

test('the agents view draws the list the event sent, one row each with its kind, its task and where it stands', async () => {
  const { Agents } = await components()
  const s = session()
  s.send('agents', listing(agent('d1', 'checker', 'run the tests', 'working'), agent('d2', 'reader', 'read the spec', 'finished'), agent('d3', 'worker', 'fix the typo', 'failed')))
  const markup = renderToStaticMarkup(React.createElement(Agents, { agents: s.live.agents, onClose() {} }))
  for (const words of ['checker', 'run the tests', 'Working', 'reader', 'read the spec', 'Finished', 'worker', 'fix the typo', 'Did not finish']) {
    assert.ok(markup.includes(words), `${words} is missing: ${markup}`)
  }
  assert.equal(markup.match(/<li\b/g).length, 3, 'one row for each agent')
})

test('a later event that leaves an agent working draws it as working, and one that drops it draws no row', async () => {
  const { Agents } = await components()
  const s = session()
  s.send('agents', listing(agent('d1', 'reader', 'look', 'working'), agent('d2', 'reader', 'other', 'working')))
  s.send('turn.done', { turn: 1, prompt: 0, reply: 'ok', contextTokens: 0, archived: 0, id: null })
  let markup = renderToStaticMarkup(React.createElement(Agents, { agents: s.live.agents, onClose() {} }))
  assert.equal(markup.match(/Working/g).length, 2, markup)
  assert.ok(!markup.includes('Did not finish'), 'the window marked an agent failed on its own')
  s.send('agents', listing(agent('d1', 'reader', 'look', 'failed')))
  markup = renderToStaticMarkup(React.createElement(Agents, { agents: s.live.agents, onClose() {} }))
  assert.equal(markup.match(/<li\b/g).length, 1)
  assert.ok(markup.includes('Did not finish'))
  assert.ok(!markup.includes('Working'))
})

test('the agents view says so when none has started, and has a way back', async () => {
  const { Agents } = await components()
  const markup = renderToStaticMarkup(React.createElement(Agents, { agents: [], onClose() {} }))
  assert.ok(markup.includes('No agents have started in this conversation'), markup)
  assert.ok(!markup.includes('<li'), markup)
  assert.ok(markup.includes('data-test="agents-done"'), 'the view has no control that closes it')
})

const sidebarProps = (agents) => ({
  sessions: [], openId: undefined, forked: new Set(), onOpen() {}, onDelete() {}, onNew() {}, onNewChat() {}, onTab() {},
  bots: [], openSlug: null, openDoing: 'idle', onOpenBot() {}, onSaveBot() {}, onRetireBot() {}, onRemoveBot() {},
  onSettings() {}, onAgents() {}, build: null, agentsStarted: agents.total, agentsWorking: agents.working,
})

/** The text inside the sidebar's agents entry, or null where there is none. */
function entry(markup) {
  const match = markup.match(/<leo-navigationitem\b[^>]*data-test="agents-open"[^>]*>(.*?)<\/leo-navigationitem>/s)
  return match && match[1].replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim()
}

test('the sidebar offers the agents beside settings, with the number started and the number working', async () => {
  const { Sidebar } = await components()
  const none = entry(renderToStaticMarkup(React.createElement(Sidebar, sidebarProps({ total: 0, working: 0 }))))
  assert.equal(none, 'Agents', 'a conversation with no agents shows a count')
  const some = entry(renderToStaticMarkup(React.createElement(Sidebar, sidebarProps({ total: 3, working: 0 }))))
  assert.equal(some, 'Agents 3')
  const busy = entry(renderToStaticMarkup(React.createElement(Sidebar, sidebarProps({ total: 3, working: 2 }))))
  assert.equal(busy, 'Agents 3 · 2 working')
})

test('the entry that opens the agents is drawn in the sidebar foot, and the composer shortcut is untouched', () => {
  const source = readFileSync('src/renderer/components/Sidebar.tsx', 'utf8')
  assert.match(source, /<footer className="sidebar-foot">[\s\S]*data-test="agents-open"[\s\S]*<\/footer>/)
  const commands = readFileSync('src/shared/commands.ts', 'utf8')
  assert.match(commands, /view\.focus-composer[\s\S]{0,200}CmdOrCtrl\+L/)
})
