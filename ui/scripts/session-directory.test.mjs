// Which folder a window may open a session in.
//
// A session's directory becomes the root the confined file helper is pinned to, so the property
// is about where the folder came from and not about the shape of the string: `isProjectPath`
// accepts every absolute path on the account. The faults rejected here are the shape check
// standing in for it, and a folder the window itself put on the recents list counting as offered.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { readFileSync } from 'node:fs'

const require = createRequire(import.meta.url)

function load(showOpenDialog) {
  const source = buildSync({
    stdin: { contents: "export * from './src/main/opened'\n", resolveDir: process.cwd(), loader: 'ts' },
    bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
  }).outputFiles[0].text
  const electron = { dialog: { showOpenDialog } }
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(id => id === 'electron' ? electron : require(id), module, module.exports)
  return module.exports
}

test('a window may open a session only in a folder it was handed', async () => {
  const picked = '/work/picked'
  const listed = '/work/listed'
  const composed = '/home/someone/.ssh'
  const main = load(async () => ({ canceled: false, filePaths: [picked] }))
  // Absolute and NUL-free, so the shape cannot tell any of them from the others.
  for (const directory of [picked, listed, composed]) assert.equal(main.mayOpenSessionIn(directory), false)

  await main.chooseDirectory({})
  main.offerDirectories([listed, 'relative/path', 42, `/with\0nul`])
  assert.equal(main.mayOpenSessionIn(picked), true)
  assert.equal(main.mayOpenSessionIn(listed), true)
  assert.equal(main.mayOpenSessionIn(composed), false, 'handing over two folders says nothing about a third')
  assert.equal(main.mayOpenSessionIn('relative/path'), false)
  assert.equal(main.mayOpenSessionIn('/with\0nul'), false)
  assert.equal(main.mayOpenSessionIn(undefined), false)
})

test('offering a folder does not make it one a bot may be pinned to', () => {
  const main = load(async () => ({ canceled: true, filePaths: [] }))
  main.offerDirectories(['/work/listed'])
  assert.equal(main.mayOpenSessionIn('/work/listed'), true)
  assert.equal(main.isOpenedDirectory('/work/listed'), false)
})

test('the request handler refuses session.new and session.open before forwarding them', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const gate = /method === 'session\.new' \|\| method === 'session\.open'\) \{\s*if \(!mayOpenSessionIn\([^\n]*\)\) \{\s*return \{ error/.exec(index)
  assert.ok(gate, 'an unoffered folder is answered with an error, unconditionally')
  assert.ok(index.indexOf('bridge.request(method', gate.index) > gate.index, 'and the check comes before the request is forwarded')
})
