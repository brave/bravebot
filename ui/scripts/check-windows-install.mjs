// That a Windows installer does what #769 settled, on a machine of the architecture it is for: it
// installs silently and per user, the Start menu entry starts the app and the app starts its agent,
// the three ways into a fused Electron are shut, it upgrades over an older install, and uninstalling
// leaves the app's data and the agent's where they are.
//
// It installs into the account running it and uninstalls again, so it is for a CI runner, and it
// refuses an account where the app is installed already. It reads what the Makefile leaves behind:
// the bundle from `make app-bundles-windows`, the installer from `make app-installers-windows`, and
// an older installer of the same bundle in `dist/previous/`, which CI builds under a lower version.
//
// A fused app cannot be driven, because Playwright attaches through `--inspect`, so everything here
// is watched from outside. The sign that the app got as far as running its own code is its agent:
// the app spawns `bravebot-rpc` from the install's resources for the first thing its window asks.
//
// Node's own modules only, so it runs without `npm ci`.
import { execFileSync, spawn, spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { connect, createServer } from 'node:net'
import { homedir, tmpdir } from 'node:os'
import { join, relative } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

if (process.platform !== 'win32') {
  console.error(`RESULT: failed, because this installs the app, which needs Windows, and this is ${process.platform}`)
  process.exit(1)
}

// Written out rather than imported from `windows-installer.mjs`, since the first release fixes them
// and a change there has to fail here as well.
const NAME = 'bravebot-desktop'
const DISPLAY_NAME = 'Brave Bot Desktop'
// Electron's name for each, and what Win32_Processor calls the machine.
const ARCHES = { amd64: { electron: 'x64', processor: 9 }, arm64: { electron: 'arm64', processor: 12 } }

const ROOT = fileURLToPath(new URL('../../', import.meta.url))
const { version } = JSON.parse(readFileSync(join(ROOT, 'ui', 'package.json'), 'utf8'))

const INSTALL = join(process.env.LOCALAPPDATA, 'Programs', NAME)
const APP = join(INSTALL, 'Brave Bot.exe')
const AGENT = join(INSTALL, 'resources', 'bravebot-rpc.exe')
// electron-builder names it for the executable, not for the entry it is the uninstaller of.
const UNINSTALLER = 'Uninstall Brave Bot.exe'
const SHORTCUT = join(process.env.APPDATA, 'Microsoft', 'Windows', 'Start Menu', 'Programs', `${DISPLAY_NAME}.lnk`)
const ENTRY = `HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${NAME}`
// Electron's user data is named for `app.name`, which is the manifest's `name`.
const USER_DATA = join(process.env.APPDATA, 'bravebot-ui')
const AGENT_STATE = join(homedir(), '.bravebot')

const problems = []
function check(ok, what) {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
  return ok
}

class Fatal extends Error {}

// What Node cannot ask Windows itself. Paths reach the script through the environment, so none is
// ever quoted into it.
function powershell(script, env = {}) {
  return execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], {
    encoding: 'utf8',
    env: { ...process.env, ...env },
  })
}

function json(script, env) {
  return JSON.parse(powershell(`ConvertTo-Json -Compress -InputObject @(${script})`, env))
}

// Every process running from the install, however it was started.
function processes() {
  return json(
    String.raw`Get-CimInstance Win32_Process |
      Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($env:INSTALL + '\', 'OrdinalIgnoreCase') } |
      Select-Object ProcessId, ParentProcessId, ExecutablePath`,
    { INSTALL },
  )
}

const same = (a, b) => a.toLowerCase() === b.toLowerCase()

// The agent, spawned from the install by the app from the install, and by `parent` if it is given.
function agent(parent) {
  const running = processes()
  const apps = new Set(running.filter((p) => same(p.ExecutablePath, APP)).map((p) => p.ProcessId))
  return running.find((p) => same(p.ExecutablePath, AGENT) && apps.has(p.ParentProcessId) && (parent === undefined || p.ParentProcessId === parent))
}

async function until(done, seconds) {
  for (const end = Date.now() + seconds * 1000; ; await sleep(500)) {
    if (done()) return true
    if (Date.now() > end) return false
  }
}

// The agent of the app `child` is, or of any app from the install. Also ends when `child` exits,
// which is what an Electron that honours ELECTRON_RUN_AS_NODE does after running its script. The
// bound is generous because the first start of a new executable waits on Defender's scan of it.
async function waitForAgent(child) {
  let found
  const exited = () => child !== undefined && (child.exitCode !== null || child.signalCode !== null)
  await until(() => (found = agent(child?.pid)) !== undefined || exited(), 120)
  return { agent: found, exited: found === undefined && exited() ? (child.exitCode ?? child.signalCode) : undefined }
}

// Forced, because nothing outside a fused app can ask it to quit, and a running app would hold
// files the next installer or uninstaller has to replace. On Windows every signal terminates.
async function stop() {
  for (const { ProcessId } of processes()) {
    try {
      process.kill(ProcessId)
    } catch (error) {
      if (error.code !== 'ESRCH') throw error
    }
  }
  if (!(await until(() => processes().length === 0, 60))) throw new Fatal('the app did not stop')
}

// Every Apps list entry with the app's name, per user and per machine, in both registry views, so
// that a second copy or a per-machine install shows up as an entry that should not be there.
function entries() {
  return json(
    String.raw`foreach ($root in @(
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall')) {
      Get-ChildItem -Path $root -ErrorAction SilentlyContinue | ForEach-Object {
        $entry = Get-ItemProperty -Path $_.PSPath -ErrorAction SilentlyContinue
        if ($entry.DisplayName -eq $env:DISPLAY_NAME) {
          [pscustomobject]@{ Key = $root + '\' + $_.PSChildName; DisplayVersion = $entry.DisplayVersion; QuietUninstallString = $entry.QuietUninstallString }
        }
      }
    }`,
    { DISPLAY_NAME },
  )
}

function digests(dir) {
  const found = new Map()
  for (const entry of readdirSync(dir, { withFileTypes: true, recursive: true })) {
    if (!entry.isFile()) continue
    const path = join(entry.parentPath, entry.name)
    found.set(relative(dir, path), createHash('sha256').update(readFileSync(path)).digest('hex'))
  }
  return found
}

async function install(installer) {
  const run = spawnSync(installer, ['/S'], { timeout: 300_000 })
  if (run.status !== 0) throw new Fatal(`${installer} /S exited ${run.status ?? run.signal ?? run.error?.message}`)
  if (!(await until(() => existsSync(APP) && entries().length > 0, 60))) {
    const left = existsSync(INSTALL) ? readdirSync(INSTALL).join(', ') : 'nothing'
    throw new Fatal(
      `${installer} /S exited 0, and ${existsSync(APP) ? 'the app is there' : `there is no ${APP}`} with ${entries().length} Apps list entries (in ${INSTALL}: ${left})`,
    )
  }
}

// What an install has to look like, the first time and after an upgrade.
function checkInstalled(bundle) {
  const found = entries()
  check(
    found.length === 1 && found[0].Key === ENTRY,
    `one Apps list entry named "${DISPLAY_NAME}", per user at ${ENTRY} (${found.map((e) => e.Key).join(', ') || 'none'})`,
  )
  check(found[0]?.DisplayVersion === version, `and it is version ${version} (${found[0]?.DisplayVersion})`)
  check(/\/S\b/.test(found[0]?.QuietUninstallString ?? ''), `with a silent uninstall command (${found[0]?.QuietUninstallString})`)

  // The files the release job signed are the files installed, and the uninstaller is all there is
  // beside them.
  const want = digests(bundle)
  const have = digests(INSTALL)
  check(have.delete(UNINSTALLER), `the uninstaller is in ${INSTALL}`)
  const differ = [...new Set([...want.keys(), ...have.keys()])].filter((file) => want.get(file) !== have.get(file))
  check(differ.length === 0, `the install is the bundle, file for file and byte for byte (${differ.length} differ${differ.length ? `: ${differ.slice(0, 5).join(', ')}` : ''})`)

  const target = existsSync(SHORTCUT)
    ? JSON.parse(powershell('ConvertTo-Json -InputObject ((New-Object -ComObject WScript.Shell).CreateShortcut($env:SHORTCUT).TargetPath)', { SHORTCUT }))
    : undefined
  check(target !== undefined && same(target, APP), `the Start menu entry runs ${APP} (${target ?? `no ${SHORTCUT}`})`)
  check(processes().length === 0, 'installing silently started nothing')
}

async function startFromStartMenu() {
  powershell('Start-Process -FilePath $env:SHORTCUT', { SHORTCUT })
  const { agent } = await waitForAgent()
  if (!check(agent !== undefined, `the Start menu entry starts the app, and the app starts its agent from ${AGENT}`)) {
    throw new Fatal('the agent never started, and the fuse checks wait on it')
  }
  await stop()
}

// The environment every launch starts from, without the two variables two of the checks set.
const BASE = Object.fromEntries(Object.entries(process.env).filter(([name]) => !['ELECTRON_RUN_AS_NODE', 'NODE_OPTIONS'].includes(name.toUpperCase())))
const launch = (args, env = {}) => spawn(APP, args, { env: { ...BASE, ...env }, stdio: 'ignore' })

function freePort() {
  return new Promise((resolve, reject) => {
    const server = createServer()
    server.once('error', reject).listen(0, '127.0.0.1', () => {
      const { port } = server.address()
      server.close(() => resolve(port))
    })
  })
}

function listening(port) {
  return new Promise((resolve) => {
    const socket = connect({ host: '127.0.0.1', port })
    socket.once('connect', () => {
      socket.destroy()
      resolve(true)
    })
    socket.once('error', () => resolve(false))
  })
}

// Each launch waits for the agent before looking, since an app that has spawned it is past the
// point where each of these would have run code or opened a port.
async function checkFuses() {
  const scratch = mkdtempSync(join(tmpdir(), 'check-windows-install-'))
  const script = join(scratch, 'marker.js')
  writeFileSync(script, "require('node:fs').writeFileSync(process.env.CHECK_MARKER, 'ran')\n")

  // An Electron that honours it is a Node interpreter: it runs the script it is handed and exits.
  {
    const marker = join(scratch, 'run-as-node')
    const seen = await waitForAgent(launch([script], { ELECTRON_RUN_AS_NODE: '1', CHECK_MARKER: marker }))
    const ran = existsSync(marker)
    check(
      seen.agent !== undefined && !ran,
      `ELECTRON_RUN_AS_NODE=1 is ignored: ${ran ? 'the app ran the script as Node' : seen.agent ? 'the app started its agent and ran no script' : `the app ${seen.exited === undefined ? 'never started its agent' : `exited ${seen.exited} without starting its agent`}`}`,
    )
    await stop()
  }

  // A packaged Electron drops `--require` from NODE_OPTIONS whatever the fuse says, so a script that
  // does not run shows nothing about the fuse. What does is Electron's log, which says which of the
  // two dropped it.
  {
    const marker = join(scratch, 'node-options')
    const log = join(scratch, 'node-options.log')
    const seen = await waitForAgent(
      launch(['--enable-logging=file', `--log-file=${log}`], { NODE_OPTIONS: `--require=${script}`, CHECK_MARKER: marker }),
    )
    const ran = existsSync(marker)
    const logged = existsSync(log) ? readFileSync(log, 'utf8') : ''
    const fused = logged.includes('NODE_OPTIONS ignored due to disabled nodeOptions fuse')
    const read = logged.includes('NODE_OPTIONs are not supported in packaged apps')
    check(
      seen.agent !== undefined && !ran && fused,
      `NODE_OPTIONS is ignored: ${ran ? 'the app ran the --require script' : fused ? 'Electron logged that the fuse dropped it' : read ? 'Electron read it, and dropped --require only because the app is packaged' : seen.agent ? 'Electron logged nothing about it' : 'the app never started its agent'}`,
    )
    await stop()
  }

  {
    const port = await freePort()
    const seen = await waitForAgent(launch([`--inspect=127.0.0.1:${port}`]))
    const open = await listening(port)
    check(
      seen.agent !== undefined && !open,
      `--inspect is ignored: ${open ? `a debugger can attach on port ${port}` : seen.agent ? `the app started its agent and nothing listens on port ${port}` : 'the app never started its agent'}`,
    )
    await stop()
  }
}

// Electron makes its user data directory as the app starts, so finding it after the app has run
// is what shows USER_DATA is the app's: electron-builder's uninstaller would delete directories
// named for the installer, and a check of one the app never used would pass whatever it did.
// The agent's is made here if it is not there yet, since it is the agent's own path either way.
function plantData() {
  check(existsSync(USER_DATA), `the app made its data directory, ${USER_DATA}`)
  mkdirSync(AGENT_STATE, { recursive: true })
  for (const dir of [USER_DATA, AGENT_STATE]) {
    if (existsSync(dir)) writeFileSync(join(dir, 'check-windows-install'), 'kept\n')
  }
}

function checkDataKept(after) {
  for (const dir of [USER_DATA, AGENT_STATE]) {
    check(existsSync(join(dir, 'check-windows-install')), `${after} leaves ${dir} where it was`)
  }
}

async function uninstall() {
  const [entry] = entries()
  const [, program, args] = /^"([^"]+)"\s*(.*)$/.exec(entry?.QuietUninstallString ?? '') ?? []
  if (program === undefined) throw new Fatal(`no silent uninstall command to run (${entry?.QuietUninstallString})`)
  const run = spawnSync(program, args.split(/\s+/).filter(Boolean), { timeout: 300_000 })
  if (run.status !== 0) throw new Fatal(`${entry.QuietUninstallString} exited ${run.status ?? run.signal ?? run.error?.message}`)
  // An NSIS uninstaller runs a copy of itself from the temp directory, so that it can delete its
  // own file, and the process started here exits before that copy has finished.
  await until(() => entries().length === 0 && !existsSync(INSTALL) && !existsSync(SHORTCUT), 120)
  check(entries().length === 0, 'uninstalling removes the Apps list entry')
  check(!existsSync(INSTALL), `and ${INSTALL}`)
  check(!existsSync(SHORTCUT), 'and the Start menu entry')
}

async function main() {
  const arch = process.argv.slice(2).find((arg) => arg.startsWith('--arch='))?.slice('--arch='.length)
  if (!Object.hasOwn(ARCHES, arch)) throw new Fatal(`usage: node ui/scripts/check-windows-install.mjs --arch=<${Object.keys(ARCHES).join('|')}>`)

  // An x64 app runs on an arm64 machine under emulation, so the arm64 check on the wrong runner
  // would pass having run nothing native.
  const processor = JSON.parse(powershell('ConvertTo-Json -InputObject ((Get-CimInstance Win32_Processor | Select-Object -First 1).Architecture)'))
  if (processor !== ARCHES[arch].processor) throw new Fatal(`this is the ${arch} check, and this machine's processor architecture is ${processor}`)

  const asset = `${NAME}-windows-${arch}-setup.exe`
  const installer = join(ROOT, 'dist', asset)
  const previous = join(ROOT, 'dist', 'previous', asset)
  const bundle = join(ROOT, 'ui', 'dist', `Brave Bot-win32-${ARCHES[arch].electron}`)
  for (const path of [installer, previous, bundle]) {
    if (!existsSync(path)) throw new Fatal(`no ${path}: see the Windows installers job in .github/workflows/ci.yml`)
  }
  if (existsSync(INSTALL) || entries().length > 0) throw new Fatal(`the app is installed here already, and this would uninstall it`)

  console.log(`\n${asset} ${version}, installed fresh:`)
  await install(installer)
  checkInstalled(bundle)
  await startFromStartMenu()
  await checkFuses()
  plantData()
  await uninstall()
  checkDataKept('uninstalling')

  console.log(`\n${asset} ${version}, over an older build of it:`)
  await install(previous)
  const [older] = entries()
  check(older?.DisplayVersion !== version, `the older build is installed first (${older?.DisplayVersion})`)
  // A file only the older install has, which the upgrade has to take away with the rest of it. An
  // upgrade that installed over the old files would leave it, and the check below would name it.
  writeFileSync(join(INSTALL, 'check-windows-install-stale'), 'stale\n')
  await install(installer)
  checkInstalled(bundle)
  checkDataKept('upgrading')
  await startFromStartMenu()
  await uninstall()
}

try {
  await main()
} catch (error) {
  if (!(error instanceof Fatal)) throw error
  console.log(`\nRESULT: failed, because ${error.message}`)
  process.exit(1)
}
if (problems.length > 0) {
  console.log(`\nRESULT: ${problems.length} failed`)
  process.exit(1)
}
console.log('\nRESULT: ok')
