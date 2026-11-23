// One instance of the app per profile, in the real Electron on Linux (#4343).
//
// `single-instance.test.mjs` drives `src/main/single-instance.ts` with a fake `app`, so it cannot
// show that Electron keeps its lock per `--user-data-dir`, that a launch that loses it ends with no
// window, or that a restart is not turned away by the lock of the instance that is quitting. Here
// the helper is bundled with `src/main/relaunch.ts` into a small main script and Electron is
// started more than once, each start recording what it did in a log file.
//
// The main script is the order `src/main/index.ts` follows: take the lock first, and build a
// window only in a start that got it. The wiring of `index.ts` itself is read as text by
// `single-instance.test.mjs`, because the real main process also starts the agent.
//
// Needs Linux, `xvfb-run` and the Electron binary `pnpm install` fetches. Anywhere else it prints
// `skipped` and passes, and under `CI` on Linux a missing `xvfb-run` or Electron fails it. To run it
// by hand on another platform, set `SINGLE_INSTANCE_ELECTRON=1`: it then opens, shows and closes
// windows on that desktop.
import test from 'node:test'
import assert from 'node:assert/strict'
import { spawn, spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)

function unavailable() {
  if (process.platform !== 'linux' && !process.env.SINGLE_INSTANCE_ELECTRON) return 'not Linux'
  const missing = []
  if (process.platform === 'linux' && spawnSync('xvfb-run', ['--help'], { stdio: 'ignore' }).error) missing.push('xvfb-run')
  let electron = ''
  try { electron = require('electron') } catch { /* not installed */ }
  if (typeof electron !== 'string' || !existsSync(electron)) missing.push('the electron binary')
  if (missing.length === 0) return ''
  if (process.env.CI) throw new Error(`missing ${missing.join(' and ')}`)
  return `no ${missing.join(' and ')}`
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

/** Wait, up to `ms`, for `check` to answer something other than a falsy value, and fail with `what` if it does not. */
async function until(what, ms, check) {
  const end = Date.now() + ms
  for (;;) {
    const answer = check()
    if (answer) return answer
    if (Date.now() > end) assert.fail(`timed out waiting for ${what}`)
    await sleep(100)
  }
}

const alive = (pid) => {
  try { process.kill(pid, 0); return true } catch { return false }
}

/**
 * The pids of the processes whose command line has a profile under `root`. Every Electron start
 * has one, and so do its GPU, utility and renderer helpers, whose pids no log holds.
 */
function usingRoot(root) {
  const listing = spawnSync('ps', ['-axo', 'pid=,command='], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 })
  if (listing.error || typeof listing.stdout !== 'string') return []
  return listing.stdout.split('\n').flatMap((line) => {
    const match = /^\s*(\d+)\s(.*)$/.exec(line)
    return match && Number(match[1]) !== process.pid && match[2].includes(`--user-data-dir=${root}`) ? [Number(match[1])] : []
  })
}

/** Whether `pid` is a live process whose command line names `root`, so a pid that was reused is left alone. */
function ownedBy(root, pid) {
  if (!alive(pid)) return false
  const listing = spawnSync('ps', ['-o', 'command=', '-p', String(pid)], { encoding: 'utf8' })
  return typeof listing.stdout === 'string' && listing.stdout.includes(root)
}

/**
 * The main script of every start. It writes a line to `LAUNCH_LOG` for each step, with the pid of
 * the process that took it, and builds its window with `show: false` so a window that is visible
 * afterwards was shown by `raiseMainWindow`. `CLOSE_WINDOW` closes it once ready, which is the
 * macOS state after the last window closed: the app stays alive and has no window. `RELAUNCH_ONCE`
 * names a file whose absence makes the first start ask to be restarted.
 */
const MAIN = `
const { app, BrowserWindow } = require('electron')
const { appendFileSync, existsSync, writeFileSync } = require('fs')
const { takeSingleInstance, raiseMainWindow } = require('./single-instance.cjs')
const { relaunchUnlessRunning } = require('./relaunch.cjs')
const log = (text) => appendFileSync(process.env.LAUNCH_LOG, process.pid + ' ' + text + '\\n')
log('start')
let window = null
function createWindow() {
  window = new BrowserWindow({ show: false, width: 320, height: 200 })
  window.on('closed', () => { window = null })
  log('window')
}
const primary = takeSingleInstance(app, () => {
  raiseMainWindow(app, window, createWindow)
  log('raised visible=' + (window ? window.isVisible() : 'none'))
})
log('lock=' + primary)
if (primary) {
  app.on('window-all-closed', () => {})
  app.whenReady().then(() => {
    createWindow()
    log('ready')
    if (process.env.CLOSE_WINDOW) {
      window.close()
      log('closed')
    }
    const once = process.env.RELAUNCH_ONCE
    if (once && !existsSync(once)) {
      writeFileSync(once, '')
      log('relaunch ' + relaunchUnlessRunning(new Set(), app))
    }
  })
}
`

/** The lines a start wrote, as { pid, text }. */
function lines(log) {
  if (!existsSync(log)) return []
  return readFileSync(log, 'utf8').split('\n').filter(Boolean).map((line) => {
    const space = line.indexOf(' ')
    return { pid: Number(line.slice(0, space)), text: line.slice(space + 1) }
  })
}

/** Run `body` with a scratch root, a display where one is needed, and every Electron it started ended at the end. */
async function inScratch(context, body) {
  const why = unavailable()
  if (why) {
    console.log(`skipped: ${why}`)
    context.skip(why)
    return
  }
  const root = mkdtempSync(join(tmpdir(), 'single-instance-electron-'))
  const children = []
  let holder = null
  try {
    mkdirSync(join(root, 'home'))
    const bundle = (entry) => buildSync({
      entryPoints: [entry], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
    }).outputFiles[0].text
    writeFileSync(join(root, 'single-instance.cjs'), bundle('src/main/single-instance.ts'))
    writeFileSync(join(root, 'relaunch.cjs'), bundle('src/main/relaunch.ts'))
    writeFileSync(join(root, 'main.cjs'), MAIN)

    // One display for every start of a test: a restart's new process outlives the process that
    // started it, so a display owned by one start would end under it.
    let display = {}
    if (process.platform === 'linux') {
      const file = join(root, 'display')
      holder = spawn('xvfb-run', ['-a', 'sh', '-c', `printf '%s\\n%s\\n' "$DISPLAY" "$XAUTHORITY" > '${file}.tmp' && mv '${file}.tmp' '${file}'; exec sleep 600`], {
        detached: true, stdio: 'ignore',
      })
      const [DISPLAY, XAUTHORITY] = (await until('a display', 30000, () => existsSync(file) && readFileSync(file, 'utf8').split('\n').slice(0, 2).join('\n'))).split('\n')
      display = { DISPLAY, ...(XAUTHORITY ? { XAUTHORITY } : {}) }
    }

    /** Start Electron on `profile`, writing to the log `name`. */
    const launch = (name, profile, extraEnv = {}) => {
      const log = join(root, `${name}.log`)
      const env = {
        ...process.env, ...display, ...extraEnv,
        HOME: join(root, 'home'), BRAVEBOT_HOME: join(root, 'home', '.bravebot'), LAUNCH_LOG: log,
      }
      delete env.ELECTRON_RUN_AS_NODE
      const child = spawn(require('electron'), [
        '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage', `--user-data-dir=${join(root, profile)}`, join(root, 'main.cjs'),
      ], { cwd: root, env, stdio: ['ignore', 'ignore', 'pipe'], detached: true })
      let stderr = ''
      child.stderr.on('data', (chunk) => { stderr += chunk })
      const exited = new Promise((resolve) => child.on('exit', (code, signal) => resolve({ code, signal })))
      children.push(child)
      return { log, exited, stderr: () => stderr, lines: () => lines(log), texts: () => lines(log).map((line) => line.text) }
    }
    await body({ launch })
  } finally {
    for (const file of readdirSync(root)) {
      if (!file.endsWith('.log')) continue
      for (const { pid } of lines(join(root, file))) {
        if (!ownedBy(root, pid)) continue
        try { process.kill(pid, 'SIGKILL') } catch { /* already gone */ }
      }
    }
    for (const child of children) { try { process.kill(-child.pid, 'SIGKILL') } catch { /* already gone */ } }
    if (holder) { try { process.kill(-holder.pid, 'SIGTERM') } catch { /* already gone */ } }
    // A killed process can still be writing into its profile, and a restart's new process may not
    // have logged yet, so end whatever names the root until nothing does, then remove it.
    const end = Date.now() + 15000
    for (let pids = usingRoot(root); pids.length > 0 && Date.now() < end; pids = usingRoot(root)) {
      for (const pid of pids) { try { process.kill(pid, 'SIGKILL') } catch { /* already gone */ } }
      await sleep(100)
    }
    rmSync(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 250 })
  }
}

/** The start's own pid, which is the one that wrote its first line. */
const pidOf = (launched) => launched.lines()[0].pid

/** Wait for the start to end, and answer how. */
async function ends(launched, name) {
  const result = await Promise.race([launched.exited, sleep(60000).then(() => 'running')])
  assert.notEqual(result, 'running', `${name} is still running after 60 seconds\n${launched.stderr()}`)
  return result
}

/**
 * A lock that is not per profile, or never requested, or requested and then ignored, leaves a
 * second start on the same profile running with its own window and agent, and a lock shared by
 * every profile ends the start on another one. Electron keeps the lock per `--user-data-dir`.
 */
test('electron ends a second start on one profile with no window, keeps the first and its window, and runs a start on another profile beside them', { timeout: 240000 }, (context) =>
  inScratch(context, async ({ launch }) => {
    const first = launch('first', 'profile-one')
    await until('the first start to be ready', 90000, () => first.texts().includes('ready'))
    const firstPid = pidOf(first)

    const second = launch('second', 'profile-one')
    const result = await ends(second, 'the second start')
    assert.deepEqual(result, { code: 0, signal: null }, second.stderr())
    // Only the lock was asked: no window was built and the start never reached `ready`.
    assert.deepEqual(second.texts(), ['start', 'lock=false'])

    // The first start was told, raised the window it had hidden, and built no other.
    await until('the first start to raise its window', 30000, () => first.texts().includes('raised visible=true'))
    assert.equal(first.texts().filter((text) => text === 'window').length, 1)
    assert.equal(first.texts().filter((text) => text === 'start').length, 1)
    assert.ok(alive(firstPid), 'the first start is still running')

    // A different profile is a different app: it starts, stays, and does not disturb the first.
    const other = launch('other', 'profile-two')
    await until('a start on another profile to be ready', 90000, () => other.texts().includes('ready'))
    assert.deepEqual(other.texts().slice(0, 2), ['start', 'lock=true'])
    await sleep(2000)
    assert.ok(alive(pidOf(other)), 'the start on another profile is still running')
    assert.ok(alive(firstPid), 'the first start is still running beside it')
  }))

/**
 * The state macOS reaches when the last window is closed: the app is alive with no window. A second
 * start that only focuses a window that is not there leaves the person with nothing on screen.
 */
test('electron builds a window for a second start when the first has none, and the first start stays', { timeout: 240000 }, (context) =>
  inScratch(context, async ({ launch }) => {
    const first = launch('first', 'profile-one', { CLOSE_WINDOW: '1' })
    await until('the first start to close its window', 90000, () => first.texts().includes('closed'))
    const firstPid = pidOf(first)

    const second = launch('second', 'profile-one')
    assert.deepEqual(await ends(second, 'the second start'), { code: 0, signal: null }, second.stderr())
    assert.deepEqual(second.texts(), ['start', 'lock=false'])

    await until('the first start to build a window', 30000, () => first.texts().filter((text) => text === 'window').length === 2)
    assert.ok(alive(firstPid), 'the first start is still running')
  }))

/**
 * `app.relaunch()` starts the new process when the old one exits. A restart that starts it before
 * the old one has gone, or an old one that keeps its lock past its exit, would have the new
 * process turned away like a second launch and leave the person with no app.
 */
test('electron starts the restarted process after the quitting one exits, and the lock does not turn it away', { timeout: 240000 }, (context) =>
  inScratch(context, async ({ launch }) => {
    const once = join(tmpdir(), `single-instance-relaunch-${process.pid}-${Date.now()}`)
    try {
      const first = launch('first', 'profile-one', { RELAUNCH_ONCE: once })
      const both = () => {
        const starts = first.lines().filter((line) => line.text === 'start').map((line) => line.pid)
        return new Set(starts).size === 2 && first.texts().includes('relaunch true') && starts
      }
      await until('the restarted process to start', 90000, both)
      const [oldPid, newPid] = both()
      assert.notEqual(oldPid, newPid)

      const of = (pid) => first.lines().filter((line) => line.pid === pid).map((line) => line.text)
      await until('the restarted process to hold the lock and be ready', 60000, () => of(newPid).includes('ready'))
      assert.deepEqual(of(newPid).slice(0, 2), ['start', 'lock=true'], 'the restarted process was turned away by the lock')
      assert.ok(!of(newPid).some((text) => text.startsWith('relaunch')), 'the restarted process restarts again')
      await until('the quitting process to exit', 30000, () => !alive(oldPid))
      await sleep(2000)
      assert.ok(alive(newPid), 'the restarted process is still running')
    } finally {
      rmSync(once, { force: true })
    }
  }))
