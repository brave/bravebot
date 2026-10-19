// Which project files Open in default app hands to the system at once (text, PDF, images, audio
// and video), and which wait for an answer from the person.
//
// A program a person approved with `run` may write any file in the project, and the system runs
// some files when they are opened: `.command` and `.terminal` on macOS, `.desktop` on Linux, `.bat`,
// `.cmd`, `.lnk` and `.exe` on Windows. Handed to `shell.openPath`, such a file runs on the host,
// outside the sandbox. `shell` and `dialog` are stubbed here, so each test reads which of
// `openPath` and `showItemInFolder` a file reached, with which path, and what the question said.
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
const asked = []
let answer = 'Cancel'
let failure = null
const electron = {
  shell: {
    openPath: async (path) => { reached.push(['openPath', path]); return '' },
    showItemInFolder: (path) => { reached.push(['showItemInFolder', path]) },
  },
  dialog: {
    showMessageBox: async (...args) => {
      const options = args[args.length - 1]
      asked.push({ parent: args.length === 2 ? args[0] : undefined, options })
      if (failure) throw failure
      return { response: options.buttons.indexOf(answer), checkboxChecked: false }
    },
  },
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

async function opening(handle, path, answering = 'Cancel', parent) {
  reached.length = 0
  asked.length = 0
  answer = answering
  failure = null
  const outcome = await open(handle, path, parent)
  return { outcome, reached: [...reached], questions: asked.length }
}

const PROGRAMS = ['x.terminal', 'x.command', 'x.desktop', 'x.bat', 'x.cmd', 'x.lnk', 'x.exe',
  'x.vbs', 'x.jar', 'x.fileloc', 'x.html', 'x.svg', 'x.rs', 'x.csv', 'x.xlsx', 'x.docx', 'x.rtf',
  'run-me', 'notes.txt.command']

// A symbolic link needs a privilege on Windows, and a file there has no execute bit to set.
const posix = { skip: process.platform === 'win32' && 'needs symbolic links and mode bits' }

test('a file that is not text, PDF, an image, audio or video is not opened until the person says to', async (t) => {
  // The first fault is no check at all, which hands every one of these to the system. The second
  // is a block list of executable types: it names the first few here, and misses a `.vbs`, a
  // `.jar`, a `.fileloc` and a file with no extension, which macOS runs in Terminal when it is
  // marked executable and which no list of extensions can name. The third is an allow list that
  // includes spreadsheets, office files and RTF, which the app opening them updates links in and
  // evaluates formulas of. The fourth is a question whose Cancel still opens, which this answer
  // tells apart from one that does not.
  const { root, handle } = project(t)
  for (const name of PROGRAMS) writeFileSync(join(root, name), 'touch /tmp/ran\n')
  for (const name of PROGRAMS) {
    assert.deepEqual(await opening(handle, name, 'Cancel'), {
      outcome: { status: 'cancelled' },
      reached: [],
      questions: 1,
    }, name)
  }
})

test('the answer Open hands the file to the system and the answer Show in folder reveals it', async (t) => {
  // The faults are a question whose answers are read by position and swapped, and one that opens
  // on every answer but Cancel. Each answer reaches exactly one call, with the resolved path.
  const { root, handle } = project(t)
  writeFileSync(join(root, 'x.command'), 'touch /tmp/ran\n')
  const target = realpathSync(join(root, 'x.command'))
  assert.deepEqual(await opening(handle, 'x.command', 'Open'), {
    outcome: { status: 'opened' }, reached: [['openPath', target]], questions: 1,
  })
  assert.deepEqual(await opening(handle, 'x.command', 'Show in folder'), {
    outcome: { status: 'revealed' }, reached: [['showItemInFolder', target]], questions: 1,
  })
})

test('the question is modal on the window, warns that opening may run the file and defaults to the safe button', async (t) => {
  // The fault is a default button that opens, which a stray Enter would answer, and a Cancel that
  // is not what closing the dialog selects. Without a window the question is still asked.
  const { root, handle } = project(t)
  writeFileSync(join(root, 'x.command'), 'touch /tmp/ran\n')
  const window = { id: 'main window' }
  await opening(handle, 'x.command', 'Cancel', window)
  const { parent, options } = asked[0]
  assert.equal(parent, window)
  assert.equal(options.type, 'warning')
  assert.match(options.detail, /may run it on this computer, outside the sandbox/)
  assert.deepEqual(options.buttons, ['Open', 'Show in folder', 'Cancel'])
  assert.notEqual(options.buttons[options.defaultId], 'Open')
  assert.equal(options.buttons[options.cancelId], 'Cancel')
  assert.deepEqual(await opening(handle, 'x.command', 'Cancel'), {
    outcome: { status: 'cancelled' }, reached: [], questions: 1,
  })
  assert.equal(asked[0].parent, undefined)
})

test('a question that cannot be shown opens nothing', async (t) => {
  // The fault is a catch around the question that carries on to open the file.
  const { root, handle } = project(t)
  writeFileSync(join(root, 'x.command'), 'touch /tmp/ran\n')
  reached.length = 0
  failure = new Error('no window')
  await assert.rejects(open(handle, 'x.command'), /no window/)
  failure = null
  assert.deepEqual(reached, [])
})

test('a text, PDF, image, audio or video file is opened in its default app without a question', async (t) => {
  // The fault is asking about everything, or refusing everything, which no test of the files above
  // can tell from the fix. `REPORT.PDF` is the fault of comparing extensions as spelled, which
  // would ask about a file because its name is in capitals.
  const { root, handle } = project(t)
  for (const name of ['notes.txt', 'README.md', 'REPORT.PDF', 'photo.png', 'clip.mp4', 'data.json']) {
    writeFileSync(join(root, name), 'x')
    assert.deepEqual(await opening(handle, name), {
      outcome: { status: 'opened' },
      reached: [['openPath', realpathSync(join(root, name))]],
      questions: 0,
    }, name)
  }
})

test('a document marked executable is asked about like a program', posix, async (t) => {
  // The fault is deciding by the extension alone. A file marked executable is a program whatever
  // it is named, and whether opening it runs it depends on handlers this app cannot see.
  const { root, handle } = project(t)
  writeFileSync(join(root, 'notes.txt'), 'touch /tmp/ran\n')
  chmodSync(join(root, 'notes.txt'), 0o755)
  assert.deepEqual(await opening(handle, 'notes.txt'), {
    outcome: { status: 'cancelled' }, reached: [], questions: 1,
  })
})

test('a link is judged by the file it reaches, not by its own name', posix, async (t) => {
  // The fault is reading the extension off the path the window sent. The system opens what the
  // link points at, so a link named `notes.txt` would hand it `x.command`, and the other way round
  // a link named `x.command` to a text file would be asked about for nothing. The question names
  // the file that would open, because a person who clicked `notes.txt` has to be told it is
  // `x.command`.
  const { root, handle } = project(t)
  writeFileSync(join(root, 'x.command'), 'touch /tmp/ran\n')
  symlinkSync('x.command', join(root, 'notes.txt'))
  assert.deepEqual(await opening(handle, 'notes.txt', 'Open'), {
    outcome: { status: 'opened' },
    reached: [['openPath', realpathSync(join(root, 'x.command'))]],
    questions: 1,
  })
  assert.match(asked[0].options.detail, /^"x\.command" /)
  assert.doesNotMatch(asked[0].options.detail, /notes\.txt/)
  writeFileSync(join(root, 'plain.txt'), 'x')
  symlinkSync('plain.txt', join(root, 'plain.command'))
  assert.deepEqual(await opening(handle, 'plain.command'), {
    outcome: { status: 'opened' },
    reached: [['openPath', realpathSync(join(root, 'plain.txt'))]],
    questions: 0,
  })
})

test('a path that is not a file in this project is neither asked about, opened nor shown', posix, async (t) => {
  // The fault is asking, or showing in the folder, whatever is not opened, which would reveal a
  // directory, a file outside the project through a link, or a session this window never opened.
  const { root, handle } = project(t)
  const outside = mkdtempSync(join(tmpdir(), 'bravebot-outside-'))
  t.after(() => rmSync(outside, { recursive: true, force: true }))
  writeFileSync(join(outside, 'x.command'), 'touch /tmp/ran\n')
  symlinkSync(join(outside, 'x.command'), join(root, 'away.command'))
  mkdirSync(join(root, 'folder'))
  for (const [session, path] of [[handle, 'folder'], [handle, 'away.command'], [handle, 'missing.txt'],
    ['open-test-never', 'x.command']]) {
    const { outcome, reached, questions } = await opening(session, path, 'Open')
    assert.equal(outcome.status, 'failed', path)
    assert.deepEqual(reached, [], path)
    assert.equal(questions, 0, path)
  }
})
