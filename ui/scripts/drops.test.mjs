// Files dropped on the desktop window (docs/specs/dropping.md).
//
// Two halves. The composer's markers decide what a send carries. And the main process is the only
// place a dropped path becomes `dropped` or `attachments`, from grants it minted, checked again at
// send. What kind of file each is, its marker's noun, and the note for one too large to carry are
// the bridge's answer to `drops.classify` (crates/ui-bridge/tests/attaching.rs), so here the bridge
// is a stub that answers with a cap and words the agent does not use, to show the window takes them
// from the answer and holds no cap or wording of its own.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { mkdtempSync, mkdirSync, realpathSync, rmSync, symlinkSync, truncateSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, join } from 'node:path'

const require = createRequire(import.meta.url)
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  const electron = { app: { getPath: () => '/nonexistent', getAppPath: () => process.cwd(), isPackaged: false } }
  new Function('require', 'module', 'exports', source)((id) => (id === 'electron' ? electron : require(id)), module, module.exports)
  return module.exports
}

const staging = load("export * from './src/renderer/staging'")
const main = load("export * from './src/main/sanitise'; export * from './src/main/drops'; export { noteRoot } from './src/main/files'")

// ---- the composer's markers ---------------------------------------------------------------------

const NOUN = { image: 'Image', pdf: 'PDF', text: 'File' }
const file = (id, name, kind) => ({ kind: 'staged', file: { id, name, kind, noun: NOUN[kind] } })

test('a marker uses the noun the main process was told, whatever the kind', () => {
  const { draft } = staging.stageDrop(staging.EMPTY_STAGING, [{ kind: 'staged', file: { id: 'a', name: 'x', kind: 'image', noun: 'Picture' } }], '', 0)
  assert.equal(draft, '[Picture #1] ')
})

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

/** The stub bridge's cap, one the agent does not use, so a check against 8 MiB here would show. */
const MOST = 1024 * 1024
const picture = { kind: 'image', noun: 'Image' }
const ANSWERS = {
  'shot.png': picture, 'scan.pdf': { kind: 'pdf', noun: 'PDF' }, 'notes.md': { kind: 'text', noun: 'File' },
  'app.dmg': null, 'gone.md': { kind: 'text', noun: 'File' }, 'swapped.png': picture, 'grown.png': picture, 'big.png': picture,
  'blob': null,
}
const asked = []
const classify = async (files) => {
  asked.push(files.map((file) => file.path))
  return {
    files: files.map(({ path, bytes }) => {
      assert.ok(basename(path) in ANSWERS, path)
      const answer = ANSWERS[basename(path)]
      return answer?.kind === 'image' && bytes > MOST ? { note: `stub: ${basename(path)} weighs ${bytes}` } : answer
    }),
  }
}

const grant = async (session, path) => {
  const [outcome] = await main.stageDrops(session, [path], classify)
  assert.equal(outcome.kind, 'staged', path)
  return outcome.file.id
}

test('a drop is granted per file, and the page is told an id, a name and the bridge’s noun and never the path', async () => {
  const outcomes = await main.stageDrops('s1', [png, pdf, md, join(outside, 'folder'), dmg, 'relative.md'], classify)
  assert.deepEqual(outcomes.map((outcome) => outcome.kind), ['staged', 'staged', 'staged', 'skipped', 'named', 'skipped'])
  assert.deepEqual(outcomes.slice(0, 3).map((outcome) => [outcome.file.name, outcome.file.kind, outcome.file.noun]), [['shot.png', 'image', 'Image'], ['scan.pdf', 'pdf', 'PDF'], ['notes.md', 'text', 'File']])
  for (const outcome of outcomes.slice(0, 3)) assert.ok(!JSON.stringify(outcome).includes(outside), 'no path crosses to the page')
  assert.equal(outcomes[3].why, 'folder')
  assert.deepEqual(outcomes[4], { kind: 'named', text: dmg })
  assert.deepEqual(asked.at(-1), [png, pdf, md, dmg], 'only the regular files are asked about, resolved')
  assert.deepEqual(await main.stageDrops('nobody', [png], classify), [], 'a session this process does not hold is granted nothing')
})

test('a file is classified by the name it resolves to, not the name of a link to it', async () => {
  const blob = at('blob'), link = join(outside, 'link.png')
  symlinkSync(blob, link)
  const [outcome] = await main.stageDrops('s1', [link], classify)
  assert.deepEqual(outcome, { kind: 'named', text: link })
  assert.deepEqual(asked.at(-1), [blob])
})

test('a bridge that cannot say what the files are stages none of them', async () => {
  const failing = async () => { throw new Error('the agent is not running') }
  assert.deepEqual((await main.stageDrops('s1', [png, md], failing)).map((outcome) => outcome.why), ['unreadable', 'unreadable'])
  const short = async () => ({ files: [picture] })
  assert.deepEqual((await main.stageDrops('s1', [png, md], short)).map((outcome) => outcome.why), ['unreadable', 'unreadable'])
})

test('a window’s turn carries what it dropped as dropped text and attachments, and nothing it named', async () => {
  const ids = [await grant('s1', md), await grant('s1', png), await grant('s1', pdf)]
  const forwarded = main.sanitised('turn.send', {
    session: 's1', prompt: '[File #1] [Image #2] [PDF #3]', drops: ids,
    dropped: ['/etc/hosts'], attachments: undefined, images: [{ media: 'image/png', data: 'iVBORw0KGgo=' }],
  })
  assert.deepEqual(forwarded, { session: 's1', prompt: '[File #1] [Image #2] [PDF #3]', files: [], dropped: [md], attachments: [png, pdf] })
})

test('a path where a grant id goes, or another session’s grant, refuses the send', async () => {
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', drops: [png] }), main.SendRefused)
  const theirs = await grant('s2', png)
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', drops: [theirs] }), main.SendRefused)
  main.forgetDrops('s2')
  assert.throws(() => main.sanitised('turn.send', { session: 's2', prompt: 'p', drops: [theirs] }), main.SendRefused, 'a closed session’s grants are gone')
})

test('a grant is checked again at send: gone, or swapped for a link', async () => {
  const gone = at('gone.md'), swapped = at('swapped.png'), grown = at('grown.png')
  const ids = { gone: await grant('s1', gone), swapped: await grant('s1', swapped), grown: await grant('s1', grown) }
  rmSync(gone)
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', drops: [ids.gone] }), /gone\.md is no longer there/)
  rmSync(swapped)
  symlinkSync(png, swapped)
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', drops: [ids.swapped] }), /swapped\.png is no longer there/)
  // A file that grew past the cap is the bridge's to refuse at `turn.send`, in its own note.
  truncateSync(grown, 64 * MOST)
  assert.deepEqual(main.sanitised('turn.send', { session: 's1', prompt: 'p', drops: [ids.grown] }).attachments, [grown], 'this process holds no cap')
})

test('a file the bridge says is too large is left out with the bridge’s note, as it stands', async () => {
  const big = at('big.png')
  truncateSync(big, MOST + 1)
  assert.deepEqual(await main.stageDrops('s1', [big], classify), [{ kind: 'skipped', name: 'big.png', why: 'too-large', note: `stub: big.png weighs ${MOST + 1}` }], 'never granted, and told the size this process found')
  truncateSync(big, MOST)
  assert.equal((await main.stageDrops('s1', [big], classify))[0].kind, 'staged', 'one the bridge takes is')
})

test('a bot’s briefing stays first in dropped, with what the person dropped after it', async () => {
  const ids = [await grant('s1', md), await grant('s1', png)]
  const carried = main.withDrops('s1', ids, { session: 's1', prompt: 'p', dropped: ['/app/briefing.md'], files: [] })
  assert.deepEqual(carried.dropped, ['/app/briefing.md', md])
  assert.deepEqual(carried.attachments, [png])
})

test('a manifest run takes no drop', async () => {
  const forwarded = main.sanitised('manifest.run', { session: 's1', task: 't', model: 'm', drops: [await grant('s1', png)] })
  assert.deepEqual(forwarded, { session: 's1', task: 't', model: 'm' })
})
