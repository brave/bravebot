// Files dropped on the desktop window (docs/specs/dropping.md).
//
// Three halves. The extension tables are the terminal's and the agent's, read here out of the Rust
// source so a change to either fails until this side follows. The composer's markers decide what a
// send carries. And the main process is the only place a dropped path becomes `dropped` or
// `attachments`, from grants it minted, checked again at send.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { mkdtempSync, mkdirSync, readFileSync, realpathSync, rmSync, symlinkSync, truncateSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const require = createRequire(import.meta.url)
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  const electron = { app: { getPath: () => '/nonexistent', getAppPath: () => process.cwd(), isPackaged: false } }
  new Function('require', 'module', 'exports', source)((id) => (id === 'electron' ? electron : require(id)), module, module.exports)
  return module.exports
}

const shared = load("export * from './src/shared/drops'")
const staging = load("export * from './src/renderer/staging'")
const main = load("export * from './src/main/sanitise'; export * from './src/main/drops'; export { noteRoot } from './src/main/files'")

// ---- the tables ---------------------------------------------------------------------------------

const dropped = readFileSync('../crates/tui/src/dropped.rs', 'utf8')
const workspace = readFileSync('../crates/agent/src/workspace.rs', 'utf8')
const strings = (text) => [...text.matchAll(/"([^"]*)"/g)].map((match) => match[1])
const rustList = (source, name) => {
  const start = source.indexOf(`const ${name}: &[`)
  assert.ok(start >= 0, `${name} is still a list in the Rust source`)
  const open = source.indexOf('= &[', start) + '= &['.length
  return source.slice(open, source.indexOf('];', open))
}

test('the extensions carried as bytes are the agent’s, with the agent’s media types', () => {
  const pairs = [...rustList(workspace, 'ATTACHABLE').matchAll(/\("([^"]+)",\s*"([^"]+)"\)/g)].map((match) => [match[1], match[2]])
  assert.ok(pairs.length > 0)
  assert.deepEqual(shared.ATTACHABLE.map(([extension, media]) => [extension, media]), pairs)
})

test('the extensions and names read as text are the terminal’s', () => {
  assert.deepEqual([...shared.TEXTUAL], strings(rustList(dropped, 'TEXTUAL')))
  assert.deepEqual([...shared.TEXTUAL_NAMES], strings(rustList(dropped, 'TEXTUAL_NAMES')))
  const body = dropped.slice(dropped.indexOf('pub fn kind_of'), dropped.indexOf('pub fn dropped_with'))
  assert.deepEqual([...shared.TEXTUAL_PREFIXES], [...body.matchAll(/bare\.starts_with\("([^"]+)"\)/g)].map((match) => match[1]))
})

test('a marker uses the terminal’s noun for each kind', () => {
  const noun = dropped.slice(dropped.indexOf('pub fn noun'))
  assert.match(noun, /Kind::Attachment\("application\/pdf"\) => "PDF"/)
  assert.match(noun, /Kind::Attachment\(_\) => "Image"/)
  assert.match(noun, /Kind::Text => "File"/)
  assert.deepEqual(shared.NOUN, { image: 'Image', pdf: 'PDF', text: 'File' })
})

test('a name is classified the way kind_of classifies it', () => {
  const cases = {
    '/a/SHOT.PNG': 'image', '/a/scan.Pdf': 'pdf', '/a/photo.jpeg': 'image', '/a/notes.md': 'text',
    '/a/Makefile': 'text', '/a/.gitignore': 'text', '/a/.gitattributes': 'text', '/a/README': 'text',
    '/a/x.dmg': null, '/a/notes.': null, '/a/archive.tar.gz': null, '/a/.png': null, '/a/plain': null,
  }
  for (const [path, kind] of Object.entries(cases)) assert.equal(shared.kindOf(path), kind, path)
})

// ---- the composer's markers ---------------------------------------------------------------------

const file = (id, name, kind) => ({ kind: 'staged', file: { id, name, kind } })

test('each dropped file gets its own number, and a second drop carries on from the first', () => {
  const first = staging.stageDrop(staging.EMPTY_STAGING, [file('a', 'shot.png', 'image'), file('b', 'scan.pdf', 'pdf'), file('c', 'notes.md', 'text')], 'look at ', 8)
  assert.equal(first.draft, 'look at [Image #1] [PDF #2] [File #3] ')
  assert.equal(first.caret, first.draft.length)
  const second = staging.stageDrop(first.staging, [file('d', 'other.png', 'image')], first.draft, 0)
  assert.equal(second.draft, '[Image #4] look at [Image #1] [PDF #2] [File #3] ')
  assert.deepEqual(staging.named(second.staging, second.draft).map((item) => item.file.id), ['a', 'b', 'c', 'd'])
})

test('deleting a marker takes its file off, and putting it back brings it back', () => {
  const { staging: staged, draft } = staging.stageDrop(staging.EMPTY_STAGING, [file('a', 'shot.png', 'image'), file('b', 'scan.pdf', 'pdf')], '', 0)
  const edited = draft.replace('[Image #1]', '')
  assert.deepEqual(staging.named(staged, edited).map((item) => item.file.id), ['b'])
  assert.deepEqual(staging.named(staged, draft).map((item) => item.file.id), ['a', 'b'])
  assert.equal(staging.withoutMarker(draft, '[PDF #2]'), '[Image #1] ')
})

test('a type nothing takes is written as its path, and a folder is reported rather than written', () => {
  const outcome = staging.stageDrop(staging.EMPTY_STAGING, [
    { kind: 'named', text: '/Users/me/app.dmg' },
    { kind: 'skipped', name: 'src', why: 'folder' },
    file('a', 'shot.png', 'image'),
  ], 'see', 3)
  assert.equal(outcome.draft, 'see /Users/me/app.dmg [Image #1] ')
  assert.deepEqual(outcome.skipped, [{ kind: 'skipped', name: 'src', why: 'folder' }])
  assert.equal(staging.stageDrop(staging.EMPTY_STAGING, [{ kind: 'skipped', name: 'src', why: 'folder' }], 'x', 1).draft, 'x')
})

test('sending clears what was staged and never reuses a number', () => {
  const { staging: staged } = staging.stageDrop(staging.EMPTY_STAGING, [file('a', 'shot.png', 'image')], '', 0)
  const after = staging.sent(staged)
  assert.deepEqual(after.staged, [])
  assert.equal(staging.named(after, '[Image #1]').length, 0)
  assert.equal(staging.stageDrop(after, [file('b', 'next.png', 'image')], '', 0).draft, '[Image #2] ')
})

// ---- the main process ---------------------------------------------------------------------------

const outside = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-drops-')))
const project = join(outside, 'project')
mkdirSync(project)
mkdirSync(join(outside, 'folder'))
const at = (name, bytes = 'x') => { const path = join(outside, name); writeFileSync(path, bytes); return path }
const png = at('shot.png'), pdf = at('scan.pdf'), md = at('notes.md', '# notes\n'), dmg = at('app.dmg')
main.noteRoot('s1', project)
main.noteRoot('s2', project)
test.after(() => rmSync(outside, { recursive: true, force: true }))

const grant = (session, path) => {
  const [outcome] = main.stageDrops(session, [path])
  assert.equal(outcome.kind, 'staged', path)
  return outcome.file.id
}

test('a drop is granted per file, and the page is told an id and a name and never the path', () => {
  const outcomes = main.stageDrops('s1', [png, pdf, md, join(outside, 'folder'), dmg, 'relative.md'])
  assert.deepEqual(outcomes.map((outcome) => outcome.kind), ['staged', 'staged', 'staged', 'skipped', 'named', 'skipped'])
  assert.deepEqual(outcomes.slice(0, 3).map((outcome) => [outcome.file.name, outcome.file.kind]), [['shot.png', 'image'], ['scan.pdf', 'pdf'], ['notes.md', 'text']])
  for (const outcome of outcomes.slice(0, 3)) assert.ok(!JSON.stringify(outcome).includes(outside), 'no path crosses to the page')
  assert.equal(outcomes[3].why, 'folder')
  assert.deepEqual(outcomes[4], { kind: 'named', text: dmg })
  assert.deepEqual(main.stageDrops('nobody', [png]), [], 'a session this process does not hold is granted nothing')
})

test('a window’s turn carries what it dropped as dropped text and attachments, and nothing it named', () => {
  const ids = [grant('s1', md), grant('s1', png), grant('s1', pdf)]
  const forwarded = main.sanitised('turn.send', {
    session: 's1', prompt: '[File #1] [Image #2] [PDF #3]', drops: ids,
    dropped: ['/etc/hosts'], attachments: undefined, images: [{ media: 'image/png', data: 'iVBORw0KGgo=' }],
  })
  assert.deepEqual(forwarded, { session: 's1', prompt: '[File #1] [Image #2] [PDF #3]', files: [], dropped: [md], attachments: [png, pdf] })
})

test('a path where a grant id goes, or another session’s grant, refuses the send', () => {
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', drops: [png] }), main.SendRefused)
  const theirs = grant('s2', png)
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', drops: [theirs] }), main.SendRefused)
  main.forgetDrops('s2')
  assert.throws(() => main.sanitised('turn.send', { session: 's2', prompt: 'p', drops: [theirs] }), main.SendRefused, 'a closed session’s grants are gone')
})

test('a grant is checked again at send: gone, swapped for a link, or grown too large', () => {
  const gone = at('gone.md'), swapped = at('swapped.png'), grown = at('grown.png')
  const ids = { gone: grant('s1', gone), swapped: grant('s1', swapped), grown: grant('s1', grown) }
  rmSync(gone)
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', drops: [ids.gone] }), /gone\.md is no longer there/)
  rmSync(swapped)
  symlinkSync(png, swapped)
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', drops: [ids.swapped] }), /swapped\.png is no longer there/)
  truncateSync(grown, 8 * 1024 * 1024 + 1)
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', drops: [ids.grown] }), /grown\.png is larger than 8 MB/)
  const big = at('big.png')
  truncateSync(big, 8 * 1024 * 1024 + 1)
  assert.equal(main.stageDrops('s1', [big])[0].why, 'too-large', 'and a picture too large to carry is never granted')
})

test('a bot’s briefing stays first in dropped, with what the person dropped after it', () => {
  const ids = [grant('s1', md), grant('s1', png)]
  const carried = main.withDrops('s1', ids, { session: 's1', prompt: 'p', dropped: ['/app/briefing.md'], files: [] })
  assert.deepEqual(carried.dropped, ['/app/briefing.md', md])
  assert.deepEqual(carried.attachments, [png])
})

test('a manifest run takes no drop', () => {
  const forwarded = main.sanitised('manifest.run', { session: 's1', task: 't', model: 'm', drops: [grant('s1', png)] })
  assert.deepEqual(forwarded, { session: 's1', task: 't', model: 'm' })
})
