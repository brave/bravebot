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

test('the request handler makes a bot home again for session.new and session.open, before forwarding them', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const remake = /if \(method === 'session\.new' \|\| method === 'session\.open'\) \{\s*const directory[^\n]*\n\s*const owner = bots\(\)\.find\(\(each\) => each\.home === directory\)\s*if \(owner && !ensureHome\(owner\)\)/.exec(index)
  assert.ok(remake, 'a saved conversation reopened in a deleted home has the folder back before it opens')
  assert.ok(index.indexOf('bridge.request(method', remake.index) > remake.index, 'and the folder is made before the request is forwarded')
})

const GATE_ARGUMENT = '(params as { directory?: unknown } | null)?.directory'

test('the request handler refuses session.new, session.open, session.delete, session.purge and session.import before forwarding them', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const gate = /method === 'session\.new' \|\| method === 'session\.open' \|\| method === 'session\.delete' \|\| method === 'session\.purge' \|\| method === 'session\.import'\) \{\s*if \(!mayOpenSessionIn\(([^\n]*)\)\) \{\s*return \{ error/.exec(index)
  assert.ok(gate, 'an unoffered folder is answered with an error, unconditionally')
  assert.equal(gate[1], GATE_ARGUMENT, 'and what it checks is the directory the window named, not the whole of its params')
  assert.ok(index.indexOf('bridge.request(method', gate.index) > gate.index, 'and the check comes before the request is forwarded')
})

// Every method the window may send, placed by whether the bridge reads a directory from it. A method
// added to ALLOWED has to be put in one of these, so a new one that opens a session in a folder the
// window composed cannot go unnoticed because the three-method gate above never heard of it.
const GATED = ['session.new', 'session.open', 'session.delete', 'session.purge', 'session.import']
const NO_GATE = {
  'models.list': 'answered in the main process, which reads the configuration of the folder it is given and opens no session',
  'session.list': 'the sanitiser drops a directory the window states, so the window cannot choose the store',
  'manifest.read': 'reads a store record and opens nothing',
  'connectors.preview': 'a stdio connector\'s own working folder, typed into the form',
  'connectors.connect': 'a stdio connector\'s own working folder, typed into the form',
}
const TAKES_NO_DIRECTORY = [
  'agent.info', 'session.fork', 'session.close', 'session.mode', 'session.sandbox', 'session.rewind',
  'turn.send', 'turn.cancel', 'turn.interject', 'session.enhance', 'session.enhance.cancel', 'mentions.offer', 'mentions.named', 'confirm.reply', 'run.reply',
  'output.reply', 'vouch.reply', 'vet.reply', 'fetch.reply', 'server.reply', 'manifest.run',
  'manifest.reply', 'exposure.reply', 'mcp-server.reply', 'mcp-tools.reply', 'mcp-call.reply',
  'mcp-move.reply', 'host.reply', 'ask.reply', 'trust.reply', 'permissions.list', 'permissions.revoke',
  'doctor', 'changelog.read', 'hooks.inspect', 'language.get', 'language.set', 'connectors.list', 'connectors.disconnect', 'connectors.remove',
  'settings.inspect', 'watches.list', 'watches.add', 'watches.stop', 'update.status',
]

function allowedMethods(index) {
  const body = /const ALLOWED = new Set\(\[([^\]]*)\]\)/.exec(index)
  assert.ok(body, 'the allow-list is a literal set the test can read')
  return [...body[1].matchAll(/'([^']+)'/g)].map(each => each[1])
}

test('every method the window may send is classified by whether it carries a directory', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const allowed = allowedMethods(index)
  assert.ok(allowed.length > 40, 'the list was read whole')
  const placed = [...GATED, ...Object.keys(NO_GATE), ...TAKES_NO_DIRECTORY]
  assert.deepEqual(allowed.filter(name => !placed.includes(name)), [], 'a method added to ALLOWED is placed here, and gated if it opens or deletes a session by directory')
  assert.deepEqual(placed.filter(name => !allowed.includes(name)), [], 'and a name that left ALLOWED leaves this list')
  assert.equal(new Set(placed).size, placed.length, 'and no method is in two of the sets')
})
test('the request handler forwards a session list through the sanitiser, which drops a directory', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /bridge\.request\(method, sanitised\(method, params\)\)/, 'every method the window sends, session.list included, is forwarded sanitised')
})
