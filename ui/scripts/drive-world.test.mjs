// The world the stub-service drivers share: the model service, the temporary root, the launch and
// the project opening.
//
// The drivers themselves need a display and the built app. These run the helpers against a real
// local server where the helper is one, and against recording doubles for the page and the app
// where it drives them, so what each driver relied on is pinned without launching Electron.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, symlinkSync } from 'node:fs'
import net from 'node:net'
import { setTimeout as delay } from 'node:timers/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { inheritedEnv, launchArgs, launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'

const post = (service, body) =>
  fetch(`http://127.0.0.1:${service.address().port}/v1/chat/completions`, { method: 'POST', body: JSON.stringify(body) })

/** The one chunk the service writes, read back out of its event stream. */
async function chunkOf(response) {
  const text = await response.text()
  const [data, done] = text.split('\n\n').filter(Boolean)
  assert.equal(done, 'data: [DONE]')
  assert.ok(data.startsWith('data: '))
  return JSON.parse(data.slice('data: '.length))
}

async function withService(reply, run) {
  const { service, rounds } = await startModelService(reply)
  try {
    return await run(service, rounds)
  } finally {
    service.closeAllConnections()
    await new Promise((resolve) => service.close(resolve))
  }
}

test('the model service lists one model on a GET and counts no round for it', async () => {
  await withService(() => ({ content: 'unused' }), async (service, rounds) => {
    const response = await fetch(`http://127.0.0.1:${service.address().port}/v1/models`)
    assert.equal(response.headers.get('content-type'), 'application/json')
    assert.deepEqual(await response.json(), { data: [{ id: 'test' }] })
    assert.deepEqual(rounds, [])
  })
})

test('the model service answers a content reply with one assistant chunk that stops', async () => {
  await withService(() => ({ content: 'done' }), async (service) => {
    const response = await post(service, { messages: [] })
    assert.equal(response.headers.get('content-type'), 'text/event-stream')
    assert.deepEqual(await chunkOf(response), {
      id: 'c1',
      object: 'chat.completion.chunk',
      model: 'test',
      choices: [{ index: 0, delta: { role: 'assistant', content: 'done' }, finish_reason: 'stop' }],
      usage: { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 },
    })
  })
})

test('the model service answers a tool reply with one call named for its round that finishes with tool_calls', async () => {
  await withService(() => ({ tool: { name: 'read_file', arguments: { path: '.env' } } }), async (service) => {
    for (const round of [1, 2]) {
      const chunk = await chunkOf(await post(service, { messages: [] }))
      assert.deepEqual(chunk.choices, [{
        index: 0,
        delta: {
          role: 'assistant',
          tool_calls: [{ index: 0, id: `call-${round}`, type: 'function', function: { name: 'read_file', arguments: '{"path":".env"}' } }],
        },
        finish_reason: 'tool_calls',
      }])
    }
  })
})

test('a tool call keeps the round that asked for it when a later request arrives while its reply is awaited', async () => {
  let release, arrived
  const held = new Promise((resolve) => { release = resolve })
  const firstArrived = new Promise((resolve) => { arrived = resolve })
  const reply = async ({ round }) => {
    if (round === 1) {
      arrived()
      await held
    }
    return { tool: { name: 'run', arguments: { round } } }
  }
  await withService(reply, async (service) => {
    const first = post(service, { messages: [] })
    await firstArrived
    // The second request is answered while the first waits, and a read of the round count after the wait would see it.
    const second = await chunkOf(await post(service, { messages: [] }))
    assert.equal(second.choices[0].delta.tool_calls[0].id, 'call-2')
    release()
    const early = await chunkOf(await first)
    assert.equal(early.choices[0].delta.tool_calls[0].id, 'call-1')
    assert.equal(early.choices[0].delta.tool_calls[0].function.arguments, '{"round":1}')
  })
})

test('the model service records each POST body in order and hands the reply the body, the messages and the round', async () => {
  const seen = []
  const reply = (request) => {
    seen.push(request)
    return { content: 'ok' }
  }
  await withService(reply, async (service, rounds) => {
    const first = { messages: [{ role: 'user', content: 'one' }] }
    const second = { model: 'x' }
    await (await post(service, first)).text()
    await (await post(service, second)).text()
    assert.deepEqual(rounds, [JSON.stringify(first), JSON.stringify(second)])
    assert.deepEqual(seen, [
      { body: JSON.stringify(first), messages: first.messages, round: 1 },
      { body: JSON.stringify(second), messages: [], round: 2 },
    ])
  })
})

test('the model service reads a body whole when a character is split across two chunks', async () => {
  await withService(() => ({ content: 'ok' }), async (service, rounds) => {
    const body = JSON.stringify({ messages: [{ role: 'user', content: 'a\u20acb' }] })
    const bytes = Buffer.from(body)
    const cut = bytes.indexOf(0xe2) + 1
    await new Promise((resolve, reject) => {
      const socket = net.connect(service.address().port, '127.0.0.1', async () => {
        socket.write(`POST /v1/chat/completions HTTP/1.1\r\nHost: x\r\nContent-Length: ${bytes.length}\r\nConnection: close\r\n\r\n`)
        socket.write(bytes.subarray(0, cut))
        // The first part is read as a chunk of its own before the rest is sent.
        await delay(50)
        socket.end(bytes.subarray(cut))
      })
      socket.on('data', () => undefined)
      socket.on('close', resolve)
      socket.on('error', reject)
    })
    assert.deepEqual(rounds, [body])
  })
})

test('the model service writes nothing until an awaited reply settles', async () => {
  let release
  const held = new Promise((resolve) => { release = resolve })
  await withService(async () => { await held; return { content: 'late' } }, async (service) => {
    const pending = post(service, { messages: [] })
    const outcome = await Promise.race([pending.then(() => 'answered'), new Promise((resolve) => setTimeout(resolve, 100, 'waiting'))])
    assert.equal(outcome, 'waiting')
    release()
    assert.equal((await chunkOf(await pending)).choices[0].delta.content, 'late')
  })
})

const settingsOf = (world) => JSON.parse(readFileSync(join(world.home, '.bravebot/settings.json'), 'utf8'))

function inEnvironment(changes, run) {
  const saved = Object.fromEntries(Object.keys(changes).map((key) => [key, process.env[key]]))
  for (const [key, value] of Object.entries(changes)) {
    if (value === undefined) delete process.env[key]
    else process.env[key] = value
  }
  try {
    return run()
  } finally {
    for (const [key, value] of Object.entries(saved)) {
      if (value === undefined) delete process.env[key]
      else process.env[key] = value
    }
  }
}

test('makeWorld writes the local/test provider at the service port and keeps it beside the settings it is given', () => {
  const world = makeWorld('drive-world-test-', { service: { address: () => ({ port: 4321 }) }, settings: { permissions: { allow: ['WebFetch(domain:127.0.0.1)'] } } })
  try {
    assert.deepEqual(settingsOf(world), {
      provider: { local: { options: { baseURL: 'http://127.0.0.1:4321/v1' }, models: { test: {} } } },
      model: 'local/test',
      permissions: { allow: ['WebFetch(domain:127.0.0.1)'] },
    })
  } finally {
    rmSync(world.root, { recursive: true, force: true })
  }
})

test('makeWorld points the provider at a base URL when it is given one and no service', () => {
  const world = makeWorld('drive-world-test-', { baseURL: 'http://127.0.0.1:9/v1' })
  try {
    assert.equal(settingsOf(world).provider.local.options.baseURL, 'http://127.0.0.1:9/v1')
  } finally {
    rmSync(world.root, { recursive: true, force: true })
  }
})

test('makeWorld creates home, profile, the project and the extra directories under its root', () => {
  const world = makeWorld('drive-world-test-', { baseURL: 'http://x/v1', dirs: ['project/src/nested', 'bin'] })
  try {
    assert.equal(world.home, join(world.root, 'home'))
    assert.equal(world.profile, join(world.root, 'profile'))
    assert.equal(world.project, join(world.root, 'project'))
    for (const path of [join(world.home, '.bravebot'), world.profile, world.project, join(world.project, 'src/nested'), join(world.root, 'bin')]) {
      assert.ok(statSync(path).isDirectory(), `${path} is a directory`)
    }
  } finally {
    rmSync(world.root, { recursive: true, force: true })
  }
})

test('makeWorld makes no project directory when told it has none', () => {
  const world = makeWorld('drive-world-test-', { baseURL: 'http://x/v1', project: false })
  try {
    assert.equal(world.project, undefined)
    assert.equal(existsSync(join(world.root, 'project')), false)
  } finally {
    rmSync(world.root, { recursive: true, force: true })
  }
})

test('makeWorld resolves its root through realpath unless told not to', { skip: process.platform === 'win32' }, () => {
  const real = mkdtempSync(join(tmpdir(), 'drive-world-real-'))
  const link = `${real}-link`
  symlinkSync(real, link)
  try {
    inEnvironment({ TMPDIR: link }, () => {
      const resolved = makeWorld('drive-world-test-', { baseURL: 'http://x/v1' })
      const verbatim = makeWorld('drive-world-test-', { baseURL: 'http://x/v1', realpath: false })
      try {
        assert.equal(dirname(resolved.root), realpathSync(real), 'the resolved root is under the real directory')
        assert.equal(dirname(verbatim.root), link, 'the verbatim root keeps the link it was made through')
      } finally {
        rmSync(resolved.root, { recursive: true, force: true })
        rmSync(verbatim.root, { recursive: true, force: true })
      }
    })
  } finally {
    rmSync(link, { force: true })
    rmSync(real, { recursive: true, force: true })
  }
})

test('makeWorld passes on only the inherited variables that are set, and adds the home, config, proxy and locale', () => {
  const changes = { PATH: '/usr/bin', DISPLAY: ':7', XAUTHORITY: undefined, WAYLAND_DISPLAY: '', LANG: 'C' }
  const env = inEnvironment(changes, () => {
    const world = makeWorld('drive-world-test-', { baseURL: 'http://x/v1' })
    rmSync(world.root, { recursive: true, force: true })
    return world
  })
  assert.equal(env.env.PATH, '/usr/bin')
  assert.equal(env.env.DISPLAY, ':7')
  assert.equal(env.env.LANG, 'C')
  assert.equal('XAUTHORITY' in env.env, false, 'an unset variable is not passed on as the word undefined')
  assert.equal('WAYLAND_DISPLAY' in env.env, false, 'an empty variable is not passed on')
  assert.equal(env.env.HOME, env.home)
  assert.equal(env.env.XDG_CONFIG_HOME, join(env.home, '.config'))
  assert.equal(env.env.NO_PROXY, '127.0.0.1,localhost')
  assert.equal(env.env.BRAVEBOT_LOCALE, 'en-US')
})

test('inheritedEnv holds only the variables that are set, in an object of its own each time', () => {
  inEnvironment({ PATH: '/usr/bin', DISPLAY: undefined, LANG: '', XDG_RUNTIME_DIR: '/run/user/1' }, () => {
    const first = inheritedEnv()
    assert.equal(first.PATH, '/usr/bin')
    assert.equal(first.XDG_RUNTIME_DIR, '/run/user/1')
    assert.equal('DISPLAY' in first, false, 'an unset variable is left out')
    assert.equal('LANG' in first, false, 'an empty variable is left out')
    assert.equal('HOME' in first, false, 'only the variables an app needs to start are passed on')
    first.PATH = 'changed'
    assert.equal(inheritedEnv().PATH, '/usr/bin')
  })
})

test('makeWorld says what it is missing when it has neither a service nor a base URL', () => {
  assert.throws(() => makeWorld('drive-world-test-'), /service or a baseURL/)
})

test('each world has an env of its own that a driver may change without touching another', () => {
  const first = makeWorld('drive-world-test-', { baseURL: 'http://x/v1' })
  const second = makeWorld('drive-world-test-', { baseURL: 'http://x/v1' })
  try {
    delete first.env.NO_PROXY
    assert.equal(second.env.NO_PROXY, '127.0.0.1,localhost')
    assert.notEqual(first.env.HOME, second.env.HOME)
  } finally {
    rmSync(first.root, { recursive: true, force: true })
    rmSync(second.root, { recursive: true, force: true })
  }
})

test('launchArgs adds --no-sandbox only on CI and the x11 flag and the extras only on Linux', () => {
  const profile = '/tmp/profile'
  const user = `--user-data-dir=${profile}`
  assert.deepEqual(launchArgs({ profile, ci: '', platform: 'darwin', linuxArgs: ['--disable-gpu'] }), ['.', user])
  assert.deepEqual(launchArgs({ profile, ci: '1', platform: 'darwin' }), ['.', '--no-sandbox', user])
  assert.deepEqual(launchArgs({ profile, ci: '', platform: 'linux' }), ['.', '--ozone-platform=x11', user])
  assert.deepEqual(
    launchArgs({ profile, ci: 'true', platform: 'linux', linuxArgs: ['--disable-gpu'] }),
    ['.', '--no-sandbox', '--ozone-platform=x11', '--disable-gpu', user],
  )
})

test('launchArgs reads CI and the platform from the process when it is not told', () => {
  const profile = '/tmp/profile'
  const onCi = inEnvironment({ CI: 'true' }, () => launchArgs({ profile }))
  const offCi = inEnvironment({ CI: undefined }, () => launchArgs({ profile }))
  assert.equal(onCi.includes('--no-sandbox'), true)
  assert.equal(offCi.includes('--no-sandbox'), false)
  assert.equal(offCi.includes('--ozone-platform=x11'), process.platform === 'linux')
})

// A page and an app that record what they were asked, in order.
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

test('launchWorld launches with the profile, the env as given and the 40 second launch timeout', async () => {
  const { app } = windowDouble()
  const env = { HOME: '/tmp/home' }
  const started = []
  const launched = await launchWorld({ profile: '/tmp/profile', env }, async (options) => { started.push(options); return app })
  assert.equal(launched.app, app)
  assert.equal(started.length, 1)
  assert.equal(started[0].env, env, 'the env is forwarded unchanged')
  assert.equal(started[0].timeout, 40000)
  assert.deepEqual(started[0].args, launchArgs({ profile: '/tmp/profile' }))
})

test('launchWorld sets the window timeout, then the viewport when there is one, and collects page errors', async () => {
  const { app, page, calls, listeners } = windowDouble()
  const launched = await launchWorld({ profile: 'p', env: {}, timeout: 15000, viewport: { width: 1400, height: 900 } }, async () => app)
  assert.equal(launched.page, page)
  assert.deepEqual(calls, [['setDefaultTimeout', 15000], ['setViewportSize', { width: 1400, height: 900 }]])
  listeners.pageerror({ message: 'first' })
  listeners.pageerror({ message: 'second' })
  assert.deepEqual(launched.errors, ['first', 'second'])
})

test('launchWorld refuses to launch without a profile and an env, so the app never runs on the real ones', async () => {
  const started = []
  const start = async (options) => { started.push(options); return windowDouble().app }
  await assert.rejects(launchWorld({ env: {} }, start), /profile and an env/)
  await assert.rejects(launchWorld({ profile: 'p' }, start), /profile and an env/)
  assert.deepEqual(started, [])
})

test('launchWorld waits 30 seconds by default and leaves the viewport alone when none is given', async () => {
  const { app, calls } = windowDouble()
  await launchWorld({ profile: 'p', env: {} }, async () => app)
  assert.deepEqual(calls, [['setDefaultTimeout', 30000]])
})

test('launchWorld closes the app again when its window never comes up', async () => {
  const { app, calls } = windowDouble({ firstWindowFails: true })
  await assert.rejects(launchWorld({ profile: 'p', env: {} }, async () => app), /no window/)
  assert.deepEqual(calls, [['close']])
})

// An app that runs the function it is asked to evaluate against a dialog of its own, and a page
// whose roles and locators record the clicks made through them.
function projectDouble() {
  const calls = []
  const dialog = {}
  const target = (path) => ({
    click: async () => calls.push(['click', ...path]),
    waitFor: async (options) => calls.push(['waitFor', ...path, options]),
    locator: (selector) => target([...path, ['locator', selector]]),
    getByRole: (role, options) => target([...path, ['role', role, options]]),
  })
  const page = { getByRole: (role, options) => target([['role', role, options]]) }
  const app = { evaluate: async (run, argument) => { calls.push(['evaluate']); return run({ dialog }, argument) } }
  return { app, page, calls, dialog }
}

test('openProject answers the picker with the project, presses Open project, approves the trust question and waits for it to go', async () => {
  const { app, page, calls, dialog } = projectDouble()
  await openProject(page, app, '/tmp/project')
  assert.deepEqual(await dialog.showOpenDialog(), { canceled: false, filePaths: ['/tmp/project'] })
  const question = ['role', 'dialog', { name: 'Project trust', exact: true }]
  assert.deepEqual(calls, [
    ['evaluate'],
    ['click', ['role', 'button', { name: 'Open project', exact: true }]],
    ['click', question, ['locator', '[data-test="trust-approve"]']],
    ['waitFor', question, { state: 'hidden' }],
  ])
})

test('openProject declines the trust question when told not to trust, and approves nothing', async () => {
  const { app, page, calls } = projectDouble()
  await openProject(page, app, '/tmp/project', { trust: false })
  const question = ['role', 'dialog', { name: 'Project trust', exact: true }]
  assert.deepEqual(calls.slice(2), [
    ['click', question, ['role', 'button', { name: "Don't trust", exact: true }]],
    ['waitFor', question, { state: 'hidden' }],
  ])
})
