// The connector catalog and its helpers: what each form builds, how a command line is split, and
// how a connector's standing is read. The agent resolves and checks every form; these hold the
// window to sending what the setup guides say to declare.
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
const c = load('src/shared/connectors.ts')
const entry = (alias) => c.CATALOG.find((each) => each.alias === alias)

test('the catalog is the four connectors the guides cover, each with a guide', () => {
  assert.deepEqual(c.CATALOG.map((each) => each.alias), ['github', 'gmail', 'calendar', 'brave-search'])
  for (const each of c.CATALOG) assert.match(each.guide, /^https:\/\/brave\.github\.io\/bravebot\//, each.alias)
})

test('GitHub is declared as its guide declares it, read-only, with the token stored', () => {
  const form = entry('github').build({ program: '/home/me/github-mcp-server/github-mcp-server', token: 't0k', toolsets: 'issues' }, new Set())
  assert.deepEqual(form.command, ['/home/me/github-mcp-server/github-mcp-server', 'stdio', '--read-only', '--lockdown-mode', '--toolsets', 'issues'])
  assert.deepEqual(form.variables, [{ name: 'GITHUB_PERSONAL_ACCESS_TOKEN', value: 't0k' }])
  // Left blank on a settings page, the stored token is kept rather than sent empty.
  const kept = entry('github').build({ program: '/p', token: '', toolsets: 'issues' }, new Set(['token']))
  assert.deepEqual(kept.variables, [{ name: 'GITHUB_PERSONAL_ACCESS_TOKEN', keep: true }])
})

test('Gmail and Calendar each run from their own directory with only their own feature group on', () => {
  const gmail = entry('gmail').build({ directory: '/home/me/google-workspace-mcp/' }, new Set())
  assert.deepEqual(gmail.command, ['node', '/home/me/google-workspace-mcp/dist/index.js'])
  assert.equal(gmail.directory, '/home/me/google-workspace-mcp')
  const overrides = (form) => form.variables.find((variable) => variable.name === 'WORKSPACE_FEATURE_OVERRIDES').value
  assert.ok(!overrides(gmail).includes('gmail.read:off'))
  assert.ok(overrides(gmail).includes('calendar.read:off'))
  const calendar = entry('calendar').build({ directory: '/home/me/cal' }, new Set())
  assert.ok(!overrides(calendar).includes('calendar.read:off'))
  assert.ok(overrides(calendar).includes('gmail.read:off'))
})

test('a settings page is filled from the declaration and never from a secret', () => {
  assert.deepEqual(entry('github').values({ alias: 'github', command: ['/p', 'stdio', '--read-only', '--lockdown-mode', '--toolsets', 'issues'], requested: true, connected: true, problem: null }), { program: '/p', toolsets: 'issues' })
  assert.deepEqual(entry('brave-search').values({ alias: 'brave-search', reads: ['/home/me/keys/brave'], requested: true, connected: true, problem: null }), { keyFile: '/home/me/keys/brave' })
})

test('a ~ stands for the home directory, at the start of a path only', () => {
  assert.equal(c.expandHome('~/x/y', '/home/me'), '/home/me/x/y')
  assert.equal(c.expandHome('~', '/home/me/'), '/home/me')
  assert.equal(c.expandHome('/a/~/b', '/home/me'), '/a/~/b')
  assert.equal(c.expandHome('~other/x', '/home/me'), '~other/x')
  assert.equal(c.expandHome('~/x', null), '~/x')
})

test('a command is drawn word by word, with a word holding a space, or an empty one, quoted', () => {
  assert.equal(c.drawCommand(['npx', '-y', 'pkg@1.2.3']), 'npx -y pkg@1.2.3')
  assert.equal(c.drawCommand(['node', '/my dir/index.js', '']), 'node "/my dir/index.js" ""')
  assert.notEqual(c.drawCommand(['a b']), c.drawCommand(['a', 'b']))
  assert.equal(c.drawCommand(null), '')
  // What is split and then drawn reads back as the words it was.
  assert.deepEqual(c.splitCommand(c.drawCommand(['node', '/my dir/x.js', 'a'])), ['node', '/my dir/x.js', 'a'])
})

test('a command line is split on spaces, with quotes keeping a word whole, and nothing else read', () => {
  assert.deepEqual(c.splitCommand('npx -y pkg@1.2.3'), ['npx', '-y', 'pkg@1.2.3'])
  assert.deepEqual(c.splitCommand('  node "/my dir/index.js"  \'a b\' ""'), ['node', '/my dir/index.js', 'a b', ''])
  assert.deepEqual(c.splitCommand('echo $HOME; rm -rf x | y'), ['echo', '$HOME;', 'rm', '-rf', 'x', '|', 'y'])
  assert.deepEqual(c.splitCommand(''), [])
})

test('a connector stands connected only when the agent says so, and needs attention when it cannot start', () => {
  const base = { alias: 'x', requested: true, connected: true, problem: null }
  assert.equal(c.standing(undefined), 'not-set-up')
  assert.equal(c.standing(base), 'connected')
  assert.equal(c.standing({ ...base, connected: false }), 'off')
  assert.equal(c.standing({ ...base, refused: 'denied' }), 'attention')
  assert.equal(c.standing({ ...base, changed: true }), 'attention')
  assert.equal(c.standing({ ...base, problem: 'bad' }), 'attention')
})

test('a declared connector turns back into the form it came from, keeping what it stores', () => {
  assert.deepEqual(c.formOf({ alias: 'calc', transport: 'http', url: 'http://localhost:3333/mcp', requested: false, connected: false, problem: null }),
    { alias: 'calc', transport: 'http', url: 'http://localhost:3333/mcp' })
  assert.deepEqual(c.formOf({ alias: 'gh', transport: 'stdio', command: ['gh-mcp', 'stdio'], variables: [{ name: 'TOKEN', stored: true }, { name: 'PATH', stored: false }], directory: null, requested: false, connected: false, problem: null }),
    { alias: 'gh', transport: 'stdio', command: ['gh-mcp', 'stdio'], variables: [{ name: 'TOKEN', keep: true }, { name: 'PATH' }], directory: undefined })
})
