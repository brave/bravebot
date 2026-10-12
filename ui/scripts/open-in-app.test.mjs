// Which project files Open in default app hands to the system, and which it only shows in their
// folder.
//
// A program a person approved with `run` may write any file in the project, and the system runs
// some files when they are opened: `.command` and `.terminal` on macOS, `.desktop` on Linux, `.bat`,
// `.cmd`, `.lnk` and `.exe` on Windows. Handed to `shell.openPath`, such a file runs on the host,
// outside the sandbox, with no quarantine mark. `shell` is stubbed here, so each test reads which
// of `openPath` and `showItemInFolder` a file reached and with which path.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { chmodSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)

const source = buildSync({
  stdin: {
    contents: "export { noteRoot, open } from './src/main/files'\n",
    resolveDir: process.cwd(),
    loader: 'ts',
  },
  bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
}).outputFiles[0].text

const reached = []
const electron = {
  shell: {
    openPath: async (path) => { reached.push(['openPath', path]); return '' },
    showItemInFolder: (path) => { reached.push(['showItemInFolder', path]) },
  },
  dialog: {},
  app: { isPackaged: false, getAppPath: () => process.cwd() },
}
const module = { exports: {} }
new Function('require', 'module', 'exports', source)(
  (id) => (id === 'electron' ? electron : require(id)), module, module.exports)
const { noteRoot, open } = module.exports

let sessions = 0
function project(t) {
  const root = mkdtempSync(join(tmpdir(), 'bravebot-open-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const handle = `open-test-${++sessions}`
  noteRoot(handle, root)
  return { root, handle }
}

async function opening(handle, path) {
  reached.length = 0
  const outcome = await open(handle, path)
  return { outcome, reached: [...reached] }
}

test('a file the system may run when opened is shown in its folder and never opened', async (t) => {
  // The first fault is no check at all, which hands every one of these to the system. The second
  // is a block list of executable types: it names the first few here, and misses a `.vbs`, a
  // `.jar`, a `.fileloc` and a file with no extension, which macOS runs in Terminal when it is
  // marked executable and which no list of extensions can name.
  const { root, handle } = project(t)
  const names = ['x.terminal', 'x.command', 'x.desktop', 'x.bat', 'x.cmd', 'x.lnk', 'x.exe',
    'x.vbs', 'x.jar', 'x.fileloc', 'run-me', 'notes.txt.command']
  for (const name of names) writeFileSync(join(root, name), 'touch /tmp/ran\n')
  for (const name of names) {
    assert.deepEqual(await opening(handle, name), {
      outcome: { status: 'revealed' },
      reached: [['showItemInFolder', realpathSync(join(root, name))]],
    }, name)
  }
})

test('a document or media file is opened in its default app', async (t) => {
  // The fault is refusing everything, which no test of the refusals above can tell from the fix.
  // `REPORT.PDF` is the fault of comparing extensions as spelled, which would show a file in its
  // folder because its name is in capitals.
  const { root, handle } = project(t)
  for (const name of ['notes.txt', 'README.md', 'REPORT.PDF', 'photo.png', 'clip.mp4']) {
    writeFileSync(join(root, name), 'x')
    assert.deepEqual(await opening(handle, name), {
      outcome: { status: 'opened' },
      reached: [['openPath', realpathSync(join(root, name))]],
    }, name)
  }
})

test('a document marked executable is shown in its folder', async (t) => {
  // The fault is deciding by the extension alone. A file marked executable is a program whatever
  // it is named, and whether opening it runs it depends on handlers this app cannot see.
  const { root, handle } = project(t)
  writeFileSync(join(root, 'notes.txt'), 'touch /tmp/ran\n')
  chmodSync(join(root, 'notes.txt'), 0o755)
  assert.deepEqual(await opening(handle, 'notes.txt'), {
    outcome: { status: 'revealed' },
    reached: [['showItemInFolder', realpathSync(join(root, 'notes.txt'))]],
  })
})

test('a link is judged by the file it reaches, not by its own name', async (t) => {
  // The fault is reading the extension off the path the window sent. The system opens what the
  // link points at, so a link named `notes.txt` would hand it `x.command`.
  const { root, handle } = project(t)
  writeFileSync(join(root, 'x.command'), 'touch /tmp/ran\n')
  symlinkSync('x.command', join(root, 'notes.txt'))
  assert.deepEqual(await opening(handle, 'notes.txt'), {
    outcome: { status: 'revealed' },
    reached: [['showItemInFolder', realpathSync(join(root, 'x.command'))]],
  })
})

test('a path that is not a file in this project is neither opened nor shown', async (t) => {
  // The fault is showing in its folder whatever is not opened, which would reveal a directory, a
  // file outside the project through a link, or a session this window never opened.
  const { root, handle } = project(t)
  const outside = mkdtempSync(join(tmpdir(), 'bravebot-outside-'))
  t.after(() => rmSync(outside, { recursive: true, force: true }))
  writeFileSync(join(outside, 'x.command'), 'touch /tmp/ran\n')
  symlinkSync(join(outside, 'x.command'), join(root, 'away.command'))
  mkdirSync(join(root, 'folder'))
  for (const [session, path] of [[handle, 'folder'], [handle, 'away.command'], [handle, 'missing.txt'],
    ['open-test-never', 'x.command']]) {
    const { outcome, reached } = await opening(session, path)
    assert.equal(outcome.status, 'failed', path)
    assert.deepEqual(reached, [], path)
  }
})
