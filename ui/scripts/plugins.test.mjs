// The plugin payloads: the list, the questions a job puts and its end. The bridge decides every
// one; these parsers hold the window to drawing a question only when it can draw all of it, so a
// malformed payload is never shown as a smaller listing with a yes beside it (PLUGIN-4).
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const p = load('src/shared/plugins.ts')
const copy = (value) => structuredClone(value)

const COMMIT = '0123456789abcdef0123456789abcdef01234567'
const local = { name: 'alpha', version: '1.0.0', source: '/src/local', requestedRef: null, commit: null, digest: 'abc', enabled: true }
const git = { name: 'beta', version: '2.3.4', source: 'https://example.com/p.git', requestedRef: 'v2', commit: COMMIT, digest: 'def', enabled: false }
const LIST = { state: '/home/me/.bravebot', writable: true, plugins: [local, git] }

// Shaped as `crates/ui-bridge/src/plugins/listing.rs` builds them.
const skill = { untrusted: { name: 'hello', description: 'says hello' }, shadowedBy: null }
const shadowed = { untrusted: { name: 'review', description: 'reviews' }, shadowedBy: '~/.bravebot/skills/review' }
const definition = { kind: 'worker', untrusted: { name: 'helper' }, shadowedBy: null }
const hook = {
  timeoutSecs: null,
  untrusted: { moment: 'turn-started', tool: null, watches: null, argv: ['/home/me/.bravebot/plugins/demo/hook.sh', '--flag', 'two words'] },
  reaches: [{ kind: 'command', untrusted: '/home/me/.bravebot/plugins/demo/hook.sh' }],
}
const server = {
  usable: true, reachable: true, tooLong: false, neverStarted: false,
  untrusted: { alias: 'srv', transport: 'stdio', argv: ['node', 'server.js'], url: null, variables: ['TOKEN'], stored: ['MODE=fast'] },
  reaches: [{ kind: 'command', untrusted: 'node' }, { kind: 'file', untrusted: '/etc/hosts' }],
}
const unusable = {
  usable: false, reachable: false, tooLong: false, neverStarted: true,
  untrusted: { alias: 'broken', transport: '', argv: [], url: null, variables: [], stored: [] },
  reaches: [],
}
const INSTALL = {
  git: false,
  untrusted: { name: 'demo', version: '2.3.4', source: '/src/demo', requestedRef: null, commit: null, skipped: ['link'] },
  skills: [skill, shadowed],
  definitions: [definition],
  hooks: { rows: [hook], fault: null, passedOver: { entries: 1, keys: 0 } },
  servers: { rows: [server, unusable], fault: null },
  homeSourcesOnly: true,
}
const newer = { ...skill, untrusted: { name: 'hello', description: 'says hello twice' } }
const UPDATE = {
  git: false,
  enabled: true,
  installedDigest: 'abc',
  newDigest: 'def',
  untrusted: {
    name: 'demo', installedVersion: '2.3.4', newVersion: '2.4.0', source: '/src/demo', installedSource: '/src/demo',
    requestedRef: null, installedCommit: null, newCommit: null, skipped: [],
  },
  before: { readable: true, skills: [skill], definitions: [definition], hooks: null, servers: { rows: [server], fault: null } },
  skills: [{ change: 'changed', was: skill, now: newer }, { change: 'added', now: shadowed }],
  definitions: [{ change: 'removed', was: definition }],
  hooks: { deltas: [{ change: 'added', now: hook }], installedFault: null, newFault: null, passedOver: { entries: 0, keys: 0 } },
  servers: { deltas: [], installedFault: null, newFault: null, passedOver: null },
  files: [{ change: 'changed', untrusted: 'skills/hello/SKILL.md' }, { change: 'added', untrusted: 'h.json' }],
  homeSourcesOnly: true,
}
const question = (kind, listing, extra = {}) => ({ job: '1', question: 1, kind, listing, ...extra })

test('the list keeps every record, a git source with its ref and full commit', () => {
  assert.deepEqual(p.parsePluginList(LIST), LIST)
  assert.deepEqual(p.parsePluginList({ ...LIST, plugins: [] }).plugins, [])
})

test('a list with any malformed record is refused whole', () => {
  assert.equal(p.parsePluginList(null), null)
  assert.equal(p.parsePluginList([]), null)
  assert.equal(p.parsePluginList({ ...LIST, plugins: undefined }), null)
  assert.equal(p.parsePluginList({ ...LIST, writable: 'yes' }), null)
  assert.equal(p.parsePluginList({ ...LIST, state: null }), null)
  assert.equal(p.parsePluginList({ ...LIST, plugins: [local, { ...git, enabled: 1 }] }), null)
  assert.equal(p.parsePluginList({ ...LIST, plugins: [{ ...local, commit: 7 }] }), null)
  assert.equal(p.parsePluginList({ ...LIST, plugins: [{ ...local, name: undefined }] }), null)
})

test('a job is the non-empty word add and update answer with', () => {
  assert.equal(p.parseJob({ job: '3' }), '3')
  assert.equal(p.parseJob({ job: 3 }), null)
  assert.equal(p.parseJob({ job: '' }), null)
  assert.equal(p.parseJob(null), null)
})

test('the host question names the host and the URL', () => {
  const asked = question('host', { host: 'example.com', url: 'https://example.com/x.git' })
  assert.deepEqual(p.parsePluginQuestion(asked), asked)
  assert.equal(p.parsePluginQuestion(question('host', { host: 'example.com' })), null)
})

test('the install question keeps every skill, definition, argv word and server row', () => {
  const parsed = p.parsePluginQuestion(question('install', INSTALL))
  assert.deepEqual(parsed, question('install', INSTALL))
  assert.deepEqual(parsed.listing.hooks.rows[0].untrusted.argv, hook.untrusted.argv)
  assert.equal(parsed.listing.skills[1].shadowedBy, '~/.bravebot/skills/review')
  assert.equal(parsed.listing.servers.rows[1].neverStarted, true)
})

test('an install listing with no hooks or servers file, or with a fault, reads as such', () => {
  const none = { ...INSTALL, hooks: null, servers: null }
  assert.deepEqual(p.parseInstallListing(none), none)
  const fault = { ...INSTALL, hooks: { rows: [], fault: 'unreadable', passedOver: { entries: 0, keys: 0 } } }
  assert.equal(p.parseInstallListing(fault).hooks.fault, 'unreadable')
  // The bridge adds `homeSourcesOnly`; a listing without it is not taken to say it.
  const { homeSourcesOnly: _, ...bare } = INSTALL
  assert.equal(p.parseInstallListing(bare).homeSourcesOnly, false)
})

test('an install listing with any malformed part is refused whole', () => {
  const broken = [
    (l) => { l.git = 'no' },
    (l) => { delete l.untrusted.version },
    (l) => { l.untrusted.skipped = [1] },
    (l) => { l.skills[0].untrusted.description = null },
    (l) => { l.skills[0].shadowedBy = 4 },
    (l) => { delete l.definitions[0].kind },
    (l) => { delete l.hooks },
    (l) => { l.hooks.rows[0].untrusted.argv = ['ok', 2] },
    (l) => { l.hooks.rows[0].untrusted.argv = 'one string' },
    (l) => { l.hooks.rows[0].timeoutSecs = -1 },
    (l) => { l.hooks.rows[0].reaches[0].kind = 'network' },
    (l) => { delete l.hooks.passedOver },
    (l) => { l.servers.rows[0].neverStarted = 'no' },
    (l) => { l.servers.rows[0].untrusted.stored = [{ MODE: 'fast' }] },
    (l) => { l.servers = { rows: 'none', fault: null } },
    (l) => { l.homeSourcesOnly = 'yes' },
  ]
  for (const [index, change] of broken.entries()) {
    const listing = copy(INSTALL)
    change(listing)
    assert.equal(p.parseInstallListing(listing), null, `change ${index}`)
    assert.equal(p.parsePluginQuestion(question('install', listing)), null, `change ${index}`)
  }
})

test('the update question keeps the versions, the paired changes and every differing file', () => {
  const parsed = p.parsePluginQuestion(question('update', UPDATE))
  assert.deepEqual(parsed, question('update', UPDATE))
  assert.equal(parsed.listing.untrusted.installedVersion, '2.3.4')
  assert.equal(parsed.listing.untrusted.newVersion, '2.4.0')
  assert.deepEqual(parsed.listing.files.map((file) => file.untrusted), ['skills/hello/SKILL.md', 'h.json'])
  const unreadable = { ...copy(UPDATE), before: { readable: false, skills: [], definitions: [], hooks: null, servers: null }, files: null }
  assert.deepEqual(p.parseUpdateListing(unreadable), unreadable)
})

test('an update listing with any malformed part is refused whole', () => {
  const broken = [
    (l) => { delete l.before },
    (l) => { l.before.readable = 'yes' },
    (l) => { l.before.skills = null },
    (l) => { l.untrusted.newVersion = 2 },
    (l) => { l.installedDigest = null },
    (l) => { l.skills[0].change = 'moved' },
    (l) => { delete l.skills[0].was },
    (l) => { delete l.definitions[0].was },
    (l) => { l.hooks.deltas[0].now.untrusted.argv = [null] },
    (l) => { l.hooks.passedOver = { entries: 'one', keys: 0 } },
    (l) => { delete l.servers.installedFault },
    (l) => { l.files = [{ change: 'changed' }] },
    (l) => { delete l.files },
  ]
  for (const [index, change] of broken.entries()) {
    const listing = copy(UPDATE)
    change(listing)
    assert.equal(p.parseUpdateListing(listing), null, `change ${index}`)
    assert.equal(p.parsePluginQuestion(question('update', listing)), null, `change ${index}`)
  }
})

test('a question of another kind, number or job shape is refused', () => {
  assert.equal(p.parsePluginQuestion(null), null)
  assert.equal(p.parsePluginQuestion(question('remove', INSTALL)), null)
  assert.equal(p.parsePluginQuestion(question('install', INSTALL, { question: 0 })), null)
  assert.equal(p.parsePluginQuestion(question('install', INSTALL, { question: 1.5 })), null)
  assert.equal(p.parsePluginQuestion(question('install', INSTALL, { job: 1 })), null)
  // A listing of one kind is not read as the other.
  assert.equal(p.parsePluginQuestion(question('install', UPDATE)), null)
  assert.equal(p.parsePluginQuestion(question('update', INSTALL)), null)
})

test('a job end carries its outcome, its sentence and the list after it', () => {
  const ended = { job: '1', outcome: 'installed', sentence: 'Installed demo from /src/demo.', plugins: [local] }
  assert.deepEqual(p.parsePluginsEnded(ended), ended)
  for (const outcome of ['updated', 'unchanged', 'declined', 'nobody', 'refused']) {
    assert.equal(p.parsePluginsEnded({ ...ended, outcome }).outcome, outcome)
  }
  assert.equal(p.parsePluginsEnded({ ...ended, outcome: 'done' }), null)
  assert.equal(p.parsePluginsEnded({ ...ended, sentence: null }), null)
  assert.equal(p.parsePluginsEnded({ ...ended, plugins: [{ ...local, digest: 1 }] }), null)
  assert.equal(p.parsePluginsEnded({ ...ended, job: undefined }), null)
})
