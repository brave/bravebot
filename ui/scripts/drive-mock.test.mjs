// What the mocked-bridge drivers share: the launch and the `session.open` answer.
//
// The drivers need a display and the built app. These run the helpers against recording doubles
// for the app and its window, and the answer through the handler shape the drivers use, so what
// each driver relied on is pinned without launching Electron.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { launchMocked, mockedLaunchArguments, mockedSessionOpen } from './drive-mock.mjs'

test('a mocked launch always passes both throttling flags, then its own arguments, then the profile last', () => {
  const user = '--user-data-dir=/tmp/profile'
  assert.deepEqual(mockedLaunchArguments({ profile: '/tmp/profile', platform: 'darwin' }), [
    '.',
    '--disable-renderer-backgrounding',
    '--disable-background-timer-throttling',
    user,
  ])
  assert.deepEqual(mockedLaunchArguments({ profile: '/tmp/profile', platform: 'darwin', args: ['--disable-backgrounding-occluded-windows'] }), [
    '.',
    '--disable-renderer-backgrounding',
    '--disable-background-timer-throttling',
    '--disable-backgrounding-occluded-windows',
    user,
  ])
})

test('the x11 flag is added only on Linux and only when asked for, ahead of the throttling flags', () => {
  const flags = ['--disable-renderer-backgrounding', '--disable-background-timer-throttling', '--user-data-dir=p']
  assert.deepEqual(mockedLaunchArguments({ profile: 'p', platform: 'linux' }), ['.', ...flags])
  assert.deepEqual(mockedLaunchArguments({ profile: 'p', platform: 'darwin', x11: true }), ['.', ...flags])
  assert.deepEqual(mockedLaunchArguments({ profile: 'p', platform: 'linux', x11: true }), ['.', '--ozone-platform=x11', ...flags])
})

test('a mocked launch reads the platform from the process when it is not told', () => {
  const asked = mockedLaunchArguments({ profile: 'p', x11: true })
  assert.equal(asked.includes('--ozone-platform=x11'), process.platform === 'linux')
})

function windowDouble({ firstWindowFails = false } = {}) {
  const calls = []
  const listeners = {}
  const page = {
    setDefaultTimeout: (ms) => calls.push(['setDefaultTimeout', ms]),
    setViewportSize: async (size) => calls.push(['setViewportSize', size]),
    on: (event, handler) => { listeners[event] = handler },
  }
  const app = {
    firstWindow: async () => {
      if (firstWindowFails) throw new Error('no window')
      return page
    },
    close: async () => calls.push(['close']),
  }
  return { app, page, calls, listeners }
}

async function silenced(run) {
  const original = console.error
  const lines = []
  console.error = (...args) => lines.push(args.join(' '))
  try {
    return { value: await run(), lines }
  } finally {
    console.error = original
  }
}

test('a mocked launch uses the 40 second launch timeout, this process as the working directory and no env of its own unless given', async () => {
  const { app } = windowDouble()
  const started = []
  const launched = await launchMocked({ profile: '/tmp/profile' }, async (options) => { started.push(options); return app })
  assert.equal(launched.app, app)
  assert.equal(started.length, 1)
  assert.equal(started[0].timeout, 40000)
  assert.equal(started[0].cwd, process.cwd())
  assert.equal('env' in started[0], false, 'with no env given the app inherits the one the driver has')
  assert.deepEqual(started[0].args, mockedLaunchArguments({ profile: '/tmp/profile' }))
})

test('the env a mocked launch is given is added to the variables the driver has, which stay', async () => {
  const { app } = windowDouble()
  const started = []
  const saved = process.env.G5_KEPT
  process.env.G5_KEPT = 'kept'
  try {
    await launchMocked({ profile: 'p', env: { BRAVEBOT_HOME: '/tmp/state', G5_OTHER: 'added' } }, async (options) => { started.push(options); return app })
  } finally {
    if (saved === undefined) delete process.env.G5_KEPT
    else process.env.G5_KEPT = saved
  }
  assert.equal(started[0].env.BRAVEBOT_HOME, '/tmp/state')
  assert.equal(started[0].env.G5_OTHER, 'added')
  assert.equal(started[0].env.G5_KEPT, 'kept')
  assert.equal(started[0].env.PATH, process.env.PATH)
})

test('a mocked launch waits 10 seconds by default, sets the timeout it is given, and sizes the window only when asked', async () => {
  const plain = windowDouble()
  await launchMocked({ profile: 'p' }, async () => plain.app)
  assert.deepEqual(plain.calls, [['setDefaultTimeout', 10000]])
  const sized = windowDouble()
  await launchMocked({ profile: 'p', timeout: 7000, viewport: { width: 1350, height: 900 } }, async () => sized.app)
  assert.deepEqual(sized.calls, [['setViewportSize', { width: 1350, height: 900 }], ['setDefaultTimeout', 7000]])
})

test('a mocked launch collects each page error and prints it as RENDERER', async () => {
  const { app, listeners } = windowDouble()
  const { value, lines } = await silenced(async () => {
    const launched = await launchMocked({ profile: 'p' }, async () => app)
    listeners.pageerror({ message: 'first' })
    listeners.pageerror({ message: 'second' })
    return launched
  })
  assert.deepEqual(value.errors, ['first', 'second'])
  assert.deepEqual(lines, ['RENDERER first', 'RENDERER second'])
})

test('a mocked launch refuses to run without a profile, so the app never runs on the real one', async () => {
  const started = []
  await assert.rejects(launchMocked({}, async (options) => { started.push(options) }), /needs a profile/)
  assert.deepEqual(started, [])
})

test('a mocked launch closes the app again when its window never comes up', async () => {
  const { app, calls } = windowDouble({ firstWindowFails: true })
  await assert.rejects(launchMocked({ profile: 'p' }, async () => app), /no window/)
  assert.deepEqual(calls, [['close']])
})

// The `session.open` handler the drivers write completes the answer with the session and the
// stored row it is asked for: `{ ...opened, session, record: { ...row, ...opened.record } }`.

test('the session.open answer carries every field the renderer reads', () => {
  assert.deepEqual(mockedSessionOpen(), {
    model: 'sample/fast',
    record: { started: 1, turns: 0, tokens: 0, build: 'fixture' },
    said: [],
    todos: {},
    context: '',
    trust: { known: true, rules: [] },
    archived: 0,
    branchNote: null,
    buildNote: null,
    frontNote: null,
  })
})

test('the session.open answer takes the fields the drivers differ in, and nothing else changes', () => {
  const said = [{ kind: 'user', text: 'Hello', prompt: 0 }]
  const todos = { rows: [] }
  const answer = mockedSessionOpen({ said, todos, turns: 1, tokens: 100, model: 'other/small' })
  assert.deepEqual(answer.said, said)
  assert.deepEqual(answer.todos, todos)
  assert.equal(answer.model, 'other/small')
  assert.deepEqual(answer.record, { started: 1, turns: 1, tokens: 100, build: 'fixture' })
  const others = ({ model, record, said, todos, ...rest }) => rest
  assert.deepEqual(others(answer), others(mockedSessionOpen()), 'the other fields do not move with the arguments')
})

test('each answer is its own object, so a driver that changes one changes no other', () => {
  const first = mockedSessionOpen()
  first.said.push({ kind: 'user', text: 'x' })
  first.trust.rules.push({ path: 'a', integrity: 'trusted' })
  const second = mockedSessionOpen()
  assert.deepEqual(second.said, [])
  assert.deepEqual(second.trust.rules, [])
})

test('the answer survives being sent to Electron unchanged', () => {
  const answer = mockedSessionOpen({ said: [{ kind: 'user', text: 'x' }], todos: { rows: [] } })
  assert.deepEqual(JSON.parse(JSON.stringify(answer)), answer)
})
