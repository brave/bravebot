// The window's half of the fault log (DIAG-10): the page names the error and nothing else, the main
// process forwards five fields and no more, and the person is shown the details that are not sent.
//
// A fault caught by React in a real commit is driven in a browser by `drive-window-fault.mjs`.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

/** One module, bundled and evaluated here, with React shared so elements can be rendered. */
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

const { faultReporter, REPORT_LIMIT } = load('src/main/window-fault.ts')
const fault = load('src/renderer/fault.ts')
const { WindowFault, FaultBoundary } = load('src/renderer/components/WindowFault.tsx')

/** A bridge that records what it is asked and answers as told. */
function bridge(answer = async () => ({})) {
  const asked = []
  return { asked, ask: async (method, params) => { asked.push({ method, params }); return answer() } }
}

/** Lets the forwarder's microtasks run. */
const settle = () => new Promise((resolve) => setImmediate(resolve))

test('the main process forwards the five fields the agent reads and nothing else the page sent', async () => {
  const { ask, asked } = bridge()
  faultReporter(ask)({
    kind: 'render', name: 'NotFoundError', reason: 'crashed', process: 'GPU', exit: 11,
    message: 'a sentence from the conversation', stack: 'at secret', session: 's1',
  })
  await settle()
  assert.deepEqual(asked, [{
    method: 'window.fault',
    params: { kind: 'render', name: 'NotFoundError', reason: 'crashed', process: 'GPU', exit: 11 },
  }])
})

test('a report that is not an object with a text kind is dropped', async () => {
  const { ask, asked } = bridge()
  const report = faultReporter(ask)
  for (const bad of [null, undefined, 'render', 7, [], {}, { kind: 3 }, { kind: null, name: 'Error' }]) report(bad)
  await settle()
  assert.deepEqual(asked, [])
})

test('a string longer than the agent could recognise is cut', async () => {
  const { ask, asked } = bridge()
  faultReporter(ask)({ kind: 'k'.repeat(500), name: 'n'.repeat(500) })
  await settle()
  assert.equal(asked[0].params.kind.length, 64)
  assert.equal(asked[0].params.name.length, 64)
})

test('an exit code is forwarded only where it is a count', async () => {
  const { ask, asked } = bridge()
  const report = faultReporter(ask)
  for (const exit of [-1, 1.5, '11', NaN, Infinity, null]) report({ kind: 'renderer_gone', exit })
  report({ kind: 'renderer_gone', exit: 0 })
  await settle()
  assert.deepEqual(asked.map((a) => a.params.exit), [undefined, undefined, undefined, undefined, undefined, undefined, 0])
})

test('a run forwards no more than the limit however often the page fails', async () => {
  const { ask, asked } = bridge()
  const report = faultReporter(ask)
  for (let n = 0; n < REPORT_LIMIT + 30; n++) report({ kind: 'render', name: 'Error' })
  await settle()
  assert.equal(asked.length, REPORT_LIMIT)
})

test('a bridge that is not running or that refuses is not an error for the caller', async () => {
  for (const ask of [async () => { throw new Error('the agent is not running') }, () => { throw new Error('sync') }, () => Promise.reject(new Error('no'))]) {
    const unhandled = []
    const catcher = (reason) => unhandled.push(reason)
    process.on('unhandledRejection', catcher)
    faultReporter(ask)({ kind: 'render', name: 'Error' })
    await settle()
    process.off('unhandledRejection', catcher)
    assert.deepEqual(unhandled, [])
  }
})

test('the page names an error by its name, and Error where what was thrown has none', () => {
  const dom = Object.assign(new Error('x'), { name: 'NotFoundError' })
  assert.equal(fault.nameOf(dom), 'NotFoundError')
  for (const odd of ['text', 7, null, undefined, {}, { name: 5 }, { name: '' }]) assert.equal(fault.nameOf(odd), 'Error')
  assert.equal(fault.nameOf({ name: 'x'.repeat(500) }).length, 64)
})

test('the page tells the main process the name of the error and not its message or stack', () => {
  const sent = []
  globalThis.window = { bravebot: { reportFault: (value) => sent.push(value) } }
  try {
    const error = Object.assign(new Error('removeChild: text from the conversation'), { name: 'NotFoundError' })
    fault.reportFault('render', error)
    assert.deepEqual(sent, [{ kind: 'render', name: 'NotFoundError' }])
  } finally {
    delete globalThis.window
  }
})

test('a page with no way to tell the main process, or one that throws, carries on', () => {
  globalThis.window = {}
  try {
    fault.reportFault('render', new Error('x'))
    globalThis.window = { bravebot: { reportFault: () => { throw new Error('closed') } } }
    fault.reportFault('render', new Error('x'))
  } finally {
    delete globalThis.window
  }
})

test('an error nothing caught and a promise nothing handled are reported, and the browser notice with no error is not', () => {
  const sent = []
  const handlers = {}
  globalThis.window = {
    addEventListener: (type, handler) => { handlers[type] = handler },
    bravebot: { reportFault: (value) => sent.push(value) },
  }
  try {
    fault.watchForFaults()
    handlers.error({ error: null, message: 'ResizeObserver loop completed with undelivered notifications.' })
    handlers.error({ error: new TypeError('x') })
    handlers.unhandledrejection({ reason: 'a string, not an error' })
    handlers.unhandledrejection({ reason: Object.assign(new Error('y'), { name: 'AbortError' }) })
    assert.deepEqual(sent, [
      { kind: 'uncaught', name: 'TypeError' },
      { kind: 'rejection', name: 'Error' },
      { kind: 'rejection', name: 'AbortError' },
    ])
  } finally {
    delete globalThis.window
  }
})

test('the details name the error, the component it was drawing in, and the stack once', () => {
  const error = Object.assign(new Error('The node to be removed is not a child of this node.'), { name: 'NotFoundError' })
  const head = 'NotFoundError: The node to be removed is not a child of this node.'
  error.stack = `${head}\n    at removeChild (bundle.js:1:1)`
  const text = fault.detailsOf(error, '\n    at ToolRun\n    at EntryList\n')
  assert.ok(text.startsWith(head), text)
  assert.equal(text.split(head).length - 1, 1, 'the first line of the stack is not repeated')
  assert.match(text, /removeChild \(bundle\.js/)
  assert.match(text, /Component stack:\n\s*at ToolRun\n\s*at EntryList/)
})

test('the details are readable for what was thrown that is not an error, and are cut', () => {
  assert.equal(fault.detailsOf('plain text'), 'Error: plain text')
  assert.match(fault.detailsOf({ toString() { throw new Error('no') } }), /unreadable/)
  assert.equal(fault.detailsOf(new Error('m'.repeat(20000))).length, 8000)
})

test('the fallback draws the details as text, so markup in a message stays inert', () => {
  const hostile = '<img src=x onerror="alert(1)"> and </pre><script>x()</script>'
  const html = renderToStaticMarkup(React.createElement(WindowFault, { details: hostile, onReload() {} }))
  assert.ok(!html.includes('<img'), html)
  assert.ok(!html.includes('<script'), html)
  assert.ok(html.includes('&lt;img'), html)
})

test('the fallback says what happened, offers a reload, and keeps its hooks', () => {
  const html = renderToStaticMarkup(React.createElement(WindowFault, { details: 'NotFoundError: x', onReload() {} }))
  for (const hook of ['data-test="window-fault"', 'data-test="window-fault-reload"', 'data-test="window-fault-copy"', 'data-test="window-fault-details"']) {
    assert.ok(html.includes(hook), `${hook} is missing: ${html}`)
  }
  assert.match(html, /This window hit an error/)
  assert.match(html, /Reload/)
})

test('the boundary draws its children until something fails, then takes over with the error it was given', () => {
  const fine = renderToStaticMarkup(React.createElement(FaultBoundary, null, React.createElement('p', null, 'the app')))
  assert.equal(fine, '<p>the app</p>')
  const error = new Error('boom')
  assert.deepEqual(FaultBoundary.getDerivedStateFromError(error), { failed: true, error })
})

test('the boundary reports only the name of what it caught, and keeps the component stack for the screen', () => {
  const sent = []
  globalThis.window = { bravebot: { reportFault: (value) => sent.push(value) } }
  try {
    const boundary = new FaultBoundary({ children: null })
    let state = null
    boundary.setState = (update) => { state = update }
    const error = Object.assign(new Error('a prompt the person typed'), { name: 'NotFoundError' })
    boundary.componentDidCatch(error, { componentStack: '\n    at ToolRun' })
    assert.deepEqual(sent, [{ kind: 'render', name: 'NotFoundError' }])
    assert.deepEqual(state, { componentStack: '\n    at ToolRun' })
  } finally {
    delete globalThis.window
  }
})
