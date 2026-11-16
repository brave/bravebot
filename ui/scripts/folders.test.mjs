// The folders a session has open beside its project, against the real files module (TRUST-9).
//
// The tree may read under a folder only while the agent's last list has it, and only by the name
// the agent gave it. The faults these reject: a folder the window names itself being browsable, a
// closed folder staying browsable, and the picker's add noting folders the agent did not answer.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { mkdirSync, mkdtempSync, realpathSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)

/** The real files module, with a picker that answers whatever the test last chose. */
function load() {
  const source = buildSync({
    stdin: { contents: "export * from './src/main/files'\n", resolveDir: process.cwd(), loader: 'ts' },
    bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
  }).outputFiles[0].text
  const picked = { paths: [], canceled: false, asked: [] }
  const electron = {
    shell: {},
    app: { isPackaged: false, getAppPath: () => process.cwd() },
    dialog: {
      showOpenDialog: async (_window, options) => {
        picked.asked.push(options)
        return { canceled: picked.canceled, filePaths: picked.paths }
      },
    },
  }
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)((id) => (id === 'electron' ? electron : require(id)), module, module.exports)
  return { files: module.exports, picked }
}

/** A project, and a folder beside it holding a file. */
function layout() {
  const base = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-folders-')))
  const root = join(base, 'project')
  const beside = join(base, 'beside')
  mkdirSync(root, { recursive: true })
  mkdirSync(join(beside, 'docs'), { recursive: true })
  writeFileSync(join(root, 'main.txt'), 'main\n')
  writeFileSync(join(beside, 'docs', 'notes.md'), '# notes\n')
  return { base, root, beside }
}

const session = '11111111-1111-4111-8111-111111111111'

const names = (listing) => listing?.rows?.map((entry) => entry.name)

test('a folder the agent listed can be browsed by its name, and not once a list leaves it out', () => {
  const { files } = load()
  const { root, beside } = layout()
  files.noteRoot(session, root)

  assert.equal(files.list(session, '', beside), null, 'a folder no list named was browsable')
  files.noteFolders(session, [beside])
  assert.deepEqual(names(files.list(session, '', beside)), ['docs'])
  assert.deepEqual(names(files.list(session, 'docs', beside)), ['notes.md'])
  assert.deepEqual(names(files.list(session, '')), ['main.txt'], 'no folder still means the project')
  assert.equal(files.list(session, '', join(beside, 'docs')), null, 'a folder inside one listed is not one listed')
  assert.equal(files.list(session, '..', beside), null)

  files.noteFolders(session, [])
  assert.equal(files.list(session, '', beside), null, 'a closed folder stayed browsable')
})

test('closing the session forgets its folders, and a list that is not one leaves none', () => {
  const { files } = load()
  const { root, beside } = layout()
  files.noteRoot(session, root)
  files.noteFolders(session, [beside])
  files.forgetRoot(session)
  files.noteRoot(session, root)
  assert.equal(files.list(session, '', beside), null)

  files.noteFolders(session, [beside])
  files.noteFolders(session, 'not a list')
  assert.equal(files.list(session, '', beside), null)
  files.noteFolders(session, ['relative/folder', 7])
  assert.deepEqual(files.foldersForSession(session), [])
})

test('the picker asks the agent about the folder it returned, and notes what the agent answered', async () => {
  const { files, picked } = load()
  const { root, beside } = layout()
  files.noteRoot(session, root)
  const asked = []

  picked.canceled = true
  assert.equal(await files.chooseFolder({}, session, async (path) => asked.push(path)), null)
  assert.deepEqual(asked, [], 'a cancelled picker still asked the agent')

  picked.canceled = false
  picked.paths = [beside]
  const added = await files.chooseFolder({}, session, async (path) => {
    asked.push(path)
    return { folder: path, folders: [path] }
  })
  assert.deepEqual(asked, [beside])
  assert.deepEqual(added.ok.folders, [beside])
  assert.deepEqual(picked.asked.at(-1).properties, ['openDirectory'])
  assert.deepEqual(names(files.list(session, '', beside)), ['docs'])

  // A refusal from the agent notes nothing.
  files.noteFolders(session, [])
  await assert.rejects(files.chooseFolder({}, session, async () => {
    throw new Error('already open')
  }))
  assert.deepEqual(files.foldersForSession(session), [])

  files.forgetRoot(session)
  const unopened = await files.chooseFolder({}, session, async (path) => asked.push(path))
  assert.equal(unopened.error.code, 'bad_request')
  assert.equal(asked.length, 1, 'a session with no project still asked the agent')
})
