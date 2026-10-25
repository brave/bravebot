// VIEW-36: the window counts the agents a conversation has started, offers them from the sidebar,
// and lists each with its kind, its task and whether it is working, finished or did not finish.
//
// The events are folded by the window's own reducer, and the components are rendered through
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

const started = (delegate, kind, task) => ({ delegate, kind, task })
const states = (live) => live.agents.map((agent) => `${agent.id}:${agent.state}`)

test('an agent that starts is counted as working, and one that ends is finished or failed by its number', () => {
  const s = session()
  s.send('turn.started', { turn: 1 })
  s.send('delegate.started', started('d1', 'checker', 'run the tests'))
  s.send('delegate.started', started('d2', 'reader', 'read the spec'))
  assert.deepEqual(states(s.live), ['d1:working', 'd2:working'])
  // The second ends first. A finish paired with the first still working, or with the last one
  // started, marks the wrong row.
  s.send('delegate.finished', { delegate: 'd2', failed: false })
  assert.deepEqual(states(s.live), ['d1:working', 'd2:finished'])
  s.send('delegate.finished', { delegate: 'd1', failed: true })
  assert.deepEqual(states(s.live), ['d1:failed', 'd2:finished'])
})

test('an agent is kept as its number, kind, task and state, and nothing a delegate printed', () => {
  const s = session()
  s.send('delegate.started', started('d1', 'reader', 'read the spec'))
  s.send('delegate.finished', { delegate: 'd1', failed: false, note: 'NOTE-FROM-THE-DELEGATE', report: 'REPORT-FROM-THE-DELEGATE', lines: ['LINE-FROM-THE-DELEGATE'] })
  assert.deepEqual(s.live.agents, [{ id: 'd1', kind: 'reader', task: 'read the spec', state: 'finished' }])
  assert.ok(!JSON.stringify(s.live).includes('FROM-THE-DELEGATE'), 'something a delegate wrote was kept')
})

test('a number used again in a later turn is a new agent, and the earlier one is left as it ended', () => {
  const s = session()
  s.send('turn.started', { turn: 1 })
  s.send('delegate.started', started('d1', 'reader', 'first task'))
  s.send('delegate.finished', { delegate: 'd1', failed: false })
  s.send('turn.started', { turn: 2 })
  s.send('delegate.started', started('d1', 'checker', 'second task'))
  assert.equal(s.live.agents.length, 2, 'the second d1 replaced or merged into the first')
  assert.deepEqual(states(s.live), ['d1:finished', 'd1:working'])
  s.send('delegate.finished', { delegate: 'd1', failed: true })
  assert.deepEqual(states(s.live), ['d1:finished', 'd1:failed'], 'the finish went to the earlier agent')
})

test('an agent still working when its turn ends is shown as not finished, whichever way the turn ends', () => {
  for (const [event, data] of [
    ['turn.done', { turn: 1, prompt: 0, reply: 'ok', contextTokens: 0, archived: 0, id: null }],
    ['turn.error', { kind: 'cancelled', message: 'x', category: 'other', attempts: 1, status: null, turn: 1, prompt: 0, contextTokens: 0, id: null }],
  ]) {
    const s = session()
    s.send('turn.started', { turn: 1 })
    s.send('delegate.started', started('d1', 'reader', 'look'))
    s.send(event, data)
    assert.deepEqual(states(s.live), ['d1:failed'], `${event} left an agent working in a turn that is over`)
  }
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

const agent = (id, kind, task, state) => ({ id, kind, task, state })

test('the agents view lists each agent with its kind, its task and where it stands', async () => {
  const { Agents } = await components()
  const markup = renderToStaticMarkup(React.createElement(Agents, {
    agents: [agent('d1', 'checker', 'run the tests', 'working'), agent('d2', 'reader', 'read the spec', 'finished'), agent('d3', 'worker', 'fix the typo', 'failed')],
    onClose() {},
  }))
  for (const words of ['checker', 'run the tests', 'Working', 'reader', 'read the spec', 'Finished', 'worker', 'fix the typo', 'Did not finish']) {
    assert.ok(markup.includes(words), `${words} is missing: ${markup}`)
  }
  assert.equal(markup.match(/<li\b/g).length, 3, 'one row for each agent')
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
