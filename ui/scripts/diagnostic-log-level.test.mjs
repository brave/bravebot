// DIAG-11 in the main process: the level chosen in the window is passed to `bravebot-rpc` with
// `--log-level` whenever the agent is started, a word that is not a level never reaches the agent,
// and the window cannot ask `diagnostic_log.set` itself. `Bridge` is bundled with Electron, the
// process spawner and the file check replaced by fakes, so no agent is started.

import test from 'node:test'
import assert from 'node:assert/strict'
import { EventEmitter } from 'node:events'
import { readFileSync } from 'node:fs'
import { build } from 'esbuild'
import { createRequire } from 'node:module'
import { allowedMethods } from './allow-list.mjs'

const require = createRequire(import.meta.url)

const FAKES = {
  electron: 'export const app = { isPackaged: false, getAppPath: () => "/app" }',
  'node:child_process': 'export const spawn = (...args) => globalThis.__fakeSpawn(...args)',
  'node:fs': 'export const existsSync = () => true',
}

const fakes = {
  name: 'fakes',
  setup(built) {
    built.onResolve({ filter: /^(electron|node:child_process|node:fs)$/ }, (args) => ({ path: args.path, namespace: 'fakes' }))
    built.onLoad({ filter: /.*/, namespace: 'fakes' }, (args) => ({ loader: 'js', contents: FAKES[args.path] }))
  },
}

async function loadBridge() {
  const built = await build({ entryPoints: ['src/main/bridge.ts'], bundle: true, write: false, platform: 'node', format: 'cjs', plugins: [fakes] })
  const source = built.outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { Bridge } = await loadBridge()

/**
 * A stand-in for `bravebot-rpc`. Each spawn is recorded with its arguments, and each request is
 * answered by `answer`, which returns the `ok` value or throws to answer with an error.
 */
function agent(answer = () => ({})) {
  const spawned = []
  const asked = []
  globalThis.__fakeSpawn = (path, args) => {
    const child = new EventEmitter()
    child.killed = false
    child.stdout = new EventEmitter()
    child.stderr = new EventEmitter()
    child.stdout.setEncoding = () => {}
    child.stderr.setEncoding = () => {}
    child.stdin = {
      write(line, done) {
        const { id, method, params } = JSON.parse(line)
        asked.push({ method, params })
        let reply
        try {
          reply = { id, ok: answer(method, params) }
        } catch (error) {
          reply = { id, error: { code: 'bad_request', message: error.message } }
        }
        done?.()
        queueMicrotask(() => child.stdout.emit('data', JSON.stringify(reply) + '\n'))
        return true
      },
      end() {},
    }
    spawned.push({ path, args, child })
    return child
  }
  return { spawned, asked, die: () => spawned.at(-1).child.emit('exit', 1, null) }
}

test('the agent is started with no --log-level until a level is chosen', async () => {
  const fake = agent()
  const bridge = new Bridge(() => {}, '/tmp/settings.json')
  await bridge.request('agent.info')
  assert.deepEqual(fake.spawned[0].args, ['--settings', '/tmp/settings.json'])
})

test('a chosen level is asked of the agent and passed again when the agent is started again', async () => {
  const fake = agent((method, params) => (method === 'diagnostic_log.set' ? { level: params.level, writing: true } : {}))
  const bridge = new Bridge(() => {}, '/tmp/settings.json')
  const answer = await bridge.setLogLevel('debug')
  assert.deepEqual(answer, { level: 'debug', writing: true })
  assert.deepEqual(fake.asked, [{ method: 'diagnostic_log.set', params: { level: 'debug' } }])
  assert.deepEqual(fake.spawned[0].args, ['--settings', '/tmp/settings.json'], 'the first agent was started before the change')

  fake.die()
  await bridge.request('agent.info')
  assert.equal(fake.spawned.length, 2)
  assert.deepEqual(fake.spawned[1].args, ['--settings', '/tmp/settings.json', '--log-level', 'debug'])
})

test('going back to error passes no argument on the next start', async () => {
  const fake = agent((method, params) => (method === 'diagnostic_log.set' ? { level: params.level, writing: true } : {}))
  const bridge = new Bridge(() => {})
  await bridge.setLogLevel('info')
  fake.die()
  await bridge.request('agent.info')
  assert.deepEqual(fake.spawned[1].args, ['--log-level', 'info'])
  await bridge.setLogLevel('error')
  fake.die()
  await bridge.request('agent.info')
  assert.deepEqual(fake.spawned[2].args, [])
})

test('a bridge made with a level starts its agent at that level and keeps it after a restart', async () => {
  const fake = agent((method, params) => (method === 'diagnostic_log.set' ? { level: params.level, writing: true } : {}))
  const bridge = new Bridge(() => {}, null, 'info')
  await bridge.request('agent.info')
  assert.deepEqual(fake.spawned[0].args, ['--log-level', 'info'])
  fake.die()
  await bridge.request('agent.info')
  assert.deepEqual(fake.spawned[1].args, ['--log-level', 'info'])
  await bridge.setLogLevel('error')
  fake.die()
  await bridge.request('agent.info')
  assert.deepEqual(fake.spawned[2].args, [])
})

test('the main process keeps the level the agent accepted for the bridge of a window made later', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /^let diagnosticLogLevel: LogLevel = 'error'$/m)
  assert.match(index, /bridge = new Bridge\([\s\S]*?\}, selectedSettings, diagnosticLogLevel\)/)
  // Assigned after the agent answered, so a refused change leaves the stored level as it was.
  assert.match(index, /const answer = await bridge\.setLogLevel\(level\)[\s\S]{0,200}?diagnosticLogLevel = level as LogLevel/)
})

test('a word that is not a level is refused before it reaches the agent', async () => {
  const fake = agent()
  const bridge = new Bridge(() => {})
  for (const word of ['verbose', 'DEBUG', '', null, 3, { level: 'debug' }]) {
    await assert.rejects(bridge.setLogLevel(word), (error) => error.code === 'bad_request')
  }
  assert.equal(fake.spawned.length, 0, 'no agent was started for a refused word')
  assert.deepEqual(fake.asked, [])
})

test('a change the agent refuses is not kept for the next start', async () => {
  const fake = agent((method) => {
    if (method === 'diagnostic_log.set') throw new Error('refused')
    return {}
  })
  const bridge = new Bridge(() => {})
  await assert.rejects(bridge.setLogLevel('debug'), (error) => error.code === 'bad_request')
  fake.die()
  await bridge.request('agent.info')
  assert.deepEqual(fake.spawned[1].args, [])
})

test('the window reaches diagnostic_log.set only through its own channel', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const allowed = allowedMethods(index)
  assert.ok(allowed.includes('settings.inspect'), 'the list was read')
  assert.ok(!allowed.includes('diagnostic_log.set'), 'the window cannot set the level past the bridge')
  assert.match(index, /ipcMain\.handle\('bravebot:diagnostic-log:set-level', async \(_event, level: unknown\) => \{[^}]*bridge\.setLogLevel\(level\)/)
  const preload = readFileSync('src/preload/index.ts', 'utf8')
  assert.match(preload, /setLogLevel\([^)]*\)[^{]*\{\s*return ipcRenderer\.invoke\('bravebot:diagnostic-log:set-level', level\)/)
})
