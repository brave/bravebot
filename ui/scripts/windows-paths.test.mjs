// What a path is when the host spells one with a backslash.
//
// Every judgement here was written against a POSIX host and reads a Windows path as something
// else: a relative path is one segment however many climbs are in it, an absolute path is not
// absolute at all, and a folder's label is the whole path. Three of those are a refusal that
// should be an acceptance, which is only an app that does not work; the first is an acceptance
// that should be a refusal, which is a request naming a file other than the one it appears to.
//
// The rules split across two modules, and the split is the point: what is safe to apply on every
// platform lives in `shared/`, which the renderer is given no `process` to interrogate, and what
// is a refusal only on Windows lives in `main/`, which knows. One bundle for all of them, so a
// rule that moved from one side to the other is still covered by one file.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)

const source = buildSync({
  stdin: {
    contents:
      "export * from './src/shared/files'\n" +
      "export * from './src/shared/recents'\n" +
      "export * from './src/main/helpers'\n" +
      "export { namesWhatItSpells } from './src/main/files'\n",
    resolveDir: process.cwd(),
    loader: 'ts',
  },
  bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
}).outputFiles[0].text

const module = { exports: {} }
const electron = { shell: {}, dialog: {}, app: { isPackaged: false, getAppPath: () => process.cwd() } }
new Function('require', 'module', 'exports', source)(
  (id) => (id === 'electron' ? electron : require(id)), module, module.exports)
const { isSubpath, isProjectPath, projectLabel, helperPaths, namesWhatItSpells } = module.exports

test('a climb spelled with backslashes is not one path segment', () => {
  // The fault is splitting on the slash alone. `a\..\..\x` then has no `..` segment in it, so it
  // is accepted, handed to `join`, and read by Windows as two climbs above the project root. The
  // `realpath` containment check in `main/files.ts` still catches where it lands, but a lexical
  // check that passes a traversal to be caught downstream is not the check it claims to be.
  for (const path of ['a\\..\\..\\x', '..\\x', 'a\\.\\b', 'a\\\\b', '\\x', '\\\\server\\share']) {
    assert.equal(isSubpath(path), false, `must refuse ${path}`)
  }
  // A backslash still separates rather than being refused outright, because that is what
  // `path.relative` hands back on Windows for a file inside the project.
  for (const path of ['', 'a', 'a/b', 'a\\b', 'a\\b/c']) {
    assert.equal(isSubpath(path), true, `must accept ${path}`)
  }
})

test('a name that means something else on Windows is refused there and nowhere else', () => {
  // The fault is applying these on every platform. Each one resolves on Windows to something the
  // segment does not spell: an alternate data stream, a device, or the same name with its trailing
  // dot dropped. None of them is a way out of the project, so refusing them everywhere buys
  // nothing and costs a POSIX project every file it has with a colon in its name, which is one
  // timestamped log away: the attachment picker throws for the whole selection over a single one.
  for (const path of ['x:stream', 'a/b:s', 'a\\b:s', 'C:x', 'NUL', 'a/con.txt', 'a/COM1', 'notes. ', 'a/b.']) {
    assert.equal(namesWhatItSpells(path, 'win32'), false, `Windows must refuse ${path}`)
    assert.equal(namesWhatItSpells(path, 'linux'), true, `POSIX must accept ${path}`)
    // Nothing about these is lexically wrong as a relative path, which is why the rule is here
    // rather than in the check the renderer shares.
    assert.equal(isSubpath(path), true, `${path} is a well-formed relative path`)
  }
  for (const path of ['', 'a', 'a/b', 'a\\b', 'connect', 'null/x', 'a.b', 'com0']) {
    assert.equal(namesWhatItSpells(path, 'win32'), true, `Windows must accept ${path}`)
  }
})

test('a Windows absolute path is a project path and a drive-relative one is not', () => {
  // The fault is requiring a leading slash, which refuses every path a Windows picker hands back.
  // Nothing downstream recovers from that: the file tree's root, saving a bot, the recents list
  // and loading a saved bot are all gated on this, so the app opens and does nothing.
  for (const path of ['C:\\work\\project', 'c:/work/project', 'C:\\', '\\\\server\\share\\project', '/work/project']) {
    assert.equal(isProjectPath(path), true, `must accept ${path}`)
  }
  // Accepting the shape grants nothing, so the list of refusals is short by design: what it has
  // to rule out is a path that resolves against wherever this process was started, and a spelling
  // that skips normalisation on the way to a device.
  for (const path of ['C:work', 'work', '.\\work', 'C:', '\\\\?\\C:\\work', '\\\\.\\NUL', '\\\\server', 'C:\\work:stream', '\\\\server\\share\\work:stream', '/work\0']) {
    assert.equal(isProjectPath(path), false, `must refuse ${path}`)
  }
})

test('a project is labelled with its own folder on either spelling', () => {
  // The fault is splitting on the slash alone, which hands back the whole of a Windows path. The
  // label sits beside the full path in every row that has one, so the row becomes the same string
  // twice and the thing it is there to tell apart, two checkouts of one project, is unreadable.
  assert.equal(projectLabel('C:\\work\\project'), 'project')
  assert.equal(projectLabel('C:/work/project'), 'project')
  assert.equal(projectLabel('/work/project'), 'project')
  assert.equal(projectLabel('/work/project/'), 'project')
  assert.equal(projectLabel('\\\\server\\share\\project'), 'project')
  // A drive root has no folder of its own, and the drive is more use than an empty label.
  assert.equal(projectLabel('C:\\'), 'C:')
  assert.equal(projectLabel('/'), '/')
})

test('a helper binary is named with an extension on Windows wherever it is looked for', () => {
  // The fault is the suffix reaching one of the two places. Neither failure is visible from a
  // POSIX host: a checkout that finds no helper says to run `npm run bridge`, and a packaged app
  // that finds none falls back to a workspace path that is not in the bundle, so file previews,
  // attachment checks and every bot memory read fail with the helper sitting beside the app.
  const windows = helperPaths('bravebot-rpc', '/app', '/resources', 'win32')
  assert.equal(windows.packaged.endsWith('bravebot-rpc.exe'), true, windows.packaged)
  assert.equal(windows.development.endsWith('bravebot-rpc.exe'), true, windows.development)

  for (const platform of ['darwin', 'linux']) {
    const posix = helperPaths('bravebot-ui-files', '/app', '/resources', platform)
    assert.equal(posix.packaged.endsWith('/bravebot-ui-files'), true, posix.packaged)
    assert.equal(posix.development.endsWith('/bravebot-ui-files'), true, posix.development)
  }
  // The checkout path is the workspace above the app, which is where `cargo build` writes.
  assert.equal(helperPaths('bravebot-rpc', '/w/ui', '/r', 'linux').development, '/w/target/debug/bravebot-rpc')
})
