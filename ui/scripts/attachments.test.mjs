// What the main process lets go to the agent as a file, against the real file helper.
//
// The limits are the terminal's: no cap on how many files, no cap on a text file's size, and the
// agent's binary test on the first 8 KiB as the only judgement of the contents. The faults these
// reject are a cap coming back (five files, 256 KB) and the binary test being dropped or widened.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { existsSync, mkdirSync, mkdtempSync, realpathSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)

const helper = join(process.cwd(), '..', 'target', 'debug', process.platform === 'win32' ? 'bravebot-ui-files.exe' : 'bravebot-ui-files')
assert.ok(existsSync(helper), `build the file helper first (npm run bridge): ${helper}`)

/** The real files module, with a picker that answers whatever the test last chose. */
function load() {
  const source = buildSync({
    stdin: { contents: "export * from './src/main/files'\n", resolveDir: process.cwd(), loader: 'ts' },
    bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
  }).outputFiles[0].text
  const picked = { paths: [] }
  const electron = {
    shell: {},
    app: { isPackaged: false, getAppPath: () => process.cwd() },
    dialog: { showOpenDialog: async () => ({ canceled: false, filePaths: picked.paths }) },
  }
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)((id) => (id === 'electron' ? electron : require(id)), module, module.exports)
  return { files: module.exports, picked }
}

/** A project holding text of every size, a binary file, and a directory and a file beside it. */
function project() {
  const base = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-attachments-')))
  const root = join(base, 'project')
  mkdirSync(join(root, 'src/nested'), { recursive: true })
  for (let n = 0; n < 7; n++) writeFileSync(join(root, `note-${n}.txt`), `note ${n}\n`)
  writeFileSync(join(root, 'large.txt'), 'a line of ordinary text\n'.repeat(20000))
  writeFileSync(join(root, 'src/nested/deep.md'), '# deep\n')
  writeFileSync(join(root, 'image.png'), Buffer.from([0x89, 0x50, 0x4e, 0x47, 0, 0, 0, 0]))
  writeFileSync(join(base, 'outside.txt'), 'private\n')
  symlinkSync(join(base, 'outside.txt'), join(root, 'escape.txt'))
  return { base, root }
}

const session = '11111111-1111-4111-8111-111111111111'

test('the picker takes any number of text files of any size, and refuses a binary one by name', async () => {
  const { files, picked } = load()
  const { root } = project()
  files.noteRoot(session, root)

  picked.paths = [...Array.from({ length: 7 }, (_, n) => join(root, `note-${n}.txt`)), join(root, 'large.txt')]
  const chosen = await files.chooseAttachments({}, session)
  assert.deepEqual(chosen.map((file) => file.path), [...Array.from({ length: 7 }, (_, n) => `note-${n}.txt`), 'large.txt'])
  assert.deepEqual(files.attachmentPaths(session, chosen.map((file) => file.id)), chosen.map((file) => file.path),
    'eight grants, one of them over 256 KB, all go at send')

  picked.paths = [join(root, 'note-0.txt'), join(root, 'image.png')]
  await assert.rejects(files.chooseAttachments({}, session), /image\.png/)
  picked.paths = [join(root, 'escape.txt')]
  await assert.rejects(files.chooseAttachments({}, session), /Choose text files inside this project/)
})

test('a grant whose file turned binary since it was chosen is refused at send', async () => {
  const { files, picked } = load()
  const { root } = project()
  files.noteRoot(session, root)
  picked.paths = [join(root, 'note-1.txt')]
  const [granted] = await files.chooseAttachments({}, session)
  writeFileSync(join(root, 'note-1.txt'), Buffer.from('text\0then a NUL'))
  assert.throws(() => files.attachmentPaths(session, [granted.id]), /no longer available/)
  assert.throws(() => files.attachmentPaths(session, ['made-up']), /no longer available/)
})
