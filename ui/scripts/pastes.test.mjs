// Pictures pasted into the desktop composer (docs/specs/pasting.md, PASTE-2, PASTE-3 and PASTE-6).
//
// Two halves. The composer numbers a paste from the counter drops use and sends what the draft
// still names. The main process is the only place pasted bytes become `images`: from grants it
// minted for what it read off the clipboard and wrote out again as PNG, never from the window. The
// cap, the note for a picture over it, the type and the marker's noun are the bridge's answer to
// `pastes.check`.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

const require = createRequire(import.meta.url)

// Electron's image codec stands in as a marker on the bytes: `toPNG` prefixes what it was given,
// so a send carrying the clipboard's bytes rather than the re-encoded ones is told apart.
const decoded = (bytes) => ({
  isEmpty: () => bytes.toString() === 'not a picture',
  toPNG: () => Buffer.concat([Buffer.from('PNG:'), bytes]),
  getSize: () => ({ width: 300, height: 200 }),
  resize: () => ({ isEmpty: () => false, toDataURL: () => 'data:image/png;base64,c21hbGw=' }),
})
const electron = {
  app: { getPath: () => '/nonexistent', getAppPath: () => process.cwd(), isPackaged: false },
  nativeImage: { createFromBuffer: decoded },
  clipboard: { read: async () => [] },
}
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)((id) => (id === 'electron' ? electron : require(id)), module, module.exports)
  return module.exports
}

const staging = load("export * from './src/renderer/staging'")
const main = load("export * from './src/main/sanitise'; export * from './src/main/pastes'; export * from './src/main/drops'; export { noteRoot } from './src/main/files'")

// ---- the composer's markers ---------------------------------------------------------------------

const dropped = (id) => ({ kind: 'staged', file: { id, name: `${id}.png`, kind: 'image', noun: 'Image' } })
const picture = (id, noun = 'Image') => ({ id, noun, thumbnail: 'data:image/png;base64,c21hbGw=' })

test('a paste is numbered from the counter drops use, so a drop then a paste are #1 and #2', () => {
  const drop = staging.stageDrop(staging.EMPTY_STAGING, [dropped('d')], 'compare ', 8)
  const paste = staging.stagePaste(drop.staging, picture('p'), drop.draft, drop.caret)
  assert.equal(paste.draft, 'compare [Image #1] [Image #2] ')
  assert.equal(paste.caret, paste.draft.length)
  const again = staging.stageDrop(paste.staging, [dropped('e')], paste.draft, 0)
  assert.equal(again.draft, '[Image #3] compare [Image #1] [Image #2] ')
  assert.deepEqual(staging.grantsOf(staging.named(again.staging, again.draft)), { drops: ['d', 'e'], pastes: ['p'] })
})

test('a paste lands at the caret, with a space before it when the word there would run into it', () => {
  const pasted = staging.stagePaste(staging.EMPTY_STAGING, picture('p'), 'what isthis', 7)
  assert.equal(pasted.draft, 'what is [Image #1] this')
  assert.equal(pasted.caret, 'what is [Image #1] '.length)
})

test('deleting a pasted marker unstages it, and only pictures the draft still names are sent', () => {
  let { staging: staged, draft } = staging.stagePaste(staging.EMPTY_STAGING, picture('a'), '', 0)
  ;({ staging: staged, draft } = staging.stagePaste(staged, picture('b'), draft, draft.length))
  assert.deepEqual(staging.grantsOf(staging.named(staged, draft)).pastes, ['a', 'b'])
  assert.deepEqual(staging.grantsOf(staging.named(staged, draft.replace('[Image #1]', ''))).pastes, ['b'])
  assert.equal(staging.withoutMarker(draft, '[Image #2]'), '[Image #1] ', 'removing the chip deletes its marker')
  const after = staging.sent(staged)
  assert.equal(staging.stagePaste(after, picture('c'), '', 0).draft, '[Image #3] ', 'a number never comes round again')
})

test('a pasted marker uses the noun the main process was told', () => {
  assert.equal(staging.stagePaste(staging.EMPTY_STAGING, picture('p', 'Picture'), '', 0).draft, '[Picture #1] ')
})

// ---- the main process ---------------------------------------------------------------------------

main.noteRoot('s1', '/nonexistent/project')
main.noteRoot('s2', '/nonexistent/project')

// ---- folded text (PASTE-1) ----------------------------------------------------------------------

const exits = load("export { btwQuestion } from './src/shared/aside'; export { scheduleArgument } from './src/shared/schedule'")

test('a paste of fewer newlines than the threshold is left to the browser, and one at it folds', () => {
  assert.equal(staging.stageText(staging.EMPTY_STAGING, 'one\ntwo\nthree', '', 0), null)
  assert.equal(staging.stageText(staging.EMPTY_STAGING, 'one\ntwo\nthree\nfour', '', 0).draft, '[Pasted text #1 +4 lines] ')
  assert.equal(staging.stageText(staging.EMPTY_STAGING, 'one\ntwo\nthree\n', '', 0).draft, '[Pasted text #1 +3 lines] ',
    'a trailing newline ends the last line rather than starting another')
  const windows = staging.stageText(staging.EMPTY_STAGING, 'a\r\nb\r\nc\r\nd', '', 0)
  assert.equal(windows.draft, '[Pasted text #1 +4 lines] ')
  assert.equal(windows.staging.staged[0].pasted.text, 'a\nb\nc\nd', 'line endings are normalised')
  assert.equal(staging.stageText(staging.EMPTY_STAGING, 'a\rb\rc\rd', '', 0).staging.staged[0].pasted.text, 'a\nb\nc\nd')
  assert.equal(staging.stageText(staging.EMPTY_STAGING, 'a\r\nb\r\nc', '', 0), null, 'a CRLF counts once')
})

test('lines are counted as a person would count them', () => {
  assert.equal(staging.linesIn('one'), 1)
  assert.equal(staging.linesIn('one\n'), 1)
  assert.equal(staging.linesIn('one\ntwo'), 2)
  assert.equal(staging.linesIn('one\n\n'), 2)
  assert.equal(staging.linesIn(''), 1)
})

test('a folded paste lands at the caret and leaves the words around it as they were', () => {
  const folded = staging.stageText(staging.EMPTY_STAGING, '1\n2\n3\n4', 'look atthis', 7)
  assert.equal(folded.draft, 'look at [Pasted text #1 +4 lines] this')
  assert.equal(folded.caret, 'look at [Pasted text #1 +4 lines] '.length)
  assert.deepEqual(folded.staging.staged[0].pasted, { marker: '[Pasted text #1 +4 lines]', text: '1\n2\n3\n4', number: 1, lines: 4 })
})

test('a drop, a picture and a folded paste share one counter, and a number never comes round again', () => {
  const drop = staging.stageDrop(staging.EMPTY_STAGING, [dropped('d')], '', 0)
  const shot = staging.stagePaste(drop.staging, picture('p'), drop.draft, drop.caret)
  const text = staging.stageText(shot.staging, 'a\nb\nc\nd\ne', shot.draft, shot.caret)
  assert.equal(text.draft, '[Image #1] [Image #2] [Pasted text #3 +5 lines] ')
  assert.deepEqual(staging.grantsOf(staging.named(text.staging, text.draft)), { drops: ['d'], pastes: ['p'] }, 'text is never a grant')
  assert.equal(staging.stageText(staging.sent(text.staging), 'a\nb\nc\nd', '', 0).draft, '[Pasted text #4 +4 lines] ')
})

test('unfolding puts back every copy of a marker, drops a deleted one, and leaves pictures alone', () => {
  const shot = staging.stagePaste(staging.EMPTY_STAGING, picture('p'), 'see ', 4)
  const first = staging.stageText(shot.staging, 'A\nB\nC\nD', shot.draft, shot.caret)
  const second = staging.stageText(first.staging, 'W\nX\nY\nZ', first.draft, first.caret)
  const one = '[Pasted text #2 +4 lines]', two = '[Pasted text #3 +4 lines]'
  assert.equal(staging.unfolded(second.staging, second.draft), 'see [Image #1] A\nB\nC\nD W\nX\nY\nZ ')
  assert.equal(staging.unfolded(second.staging, `${one} and ${one}`), 'A\nB\nC\nD and A\nB\nC\nD', 'a copied marker means the words twice')
  assert.equal(staging.unfolded(second.staging, staging.withoutMarker(second.draft, one)), 'see [Image #1] W\nX\nY\nZ ')
  assert.equal(staging.unfolded(second.staging, `typed [Image #1] ${two.slice(1)}`), `typed [Image #1] ${two.slice(1)}`, 'a broken marker is left as typed')
  assert.equal(staging.unfolded(undefined, one), one)
})

test('the folded paste under the caret is found at its start, inside it and at its end', () => {
  const { staging: staged, draft } = staging.stageText(staging.stagePaste(staging.EMPTY_STAGING, picture('p'), 'x ', 2).staging, '1\n2\n3\n4', 'x [Image #1] ', 13)
  const marker = '[Pasted text #2 +4 lines]'
  const at = draft.indexOf(marker)
  for (const caret of [at, at + 5, at + marker.length]) assert.equal(staging.markerAt(staged, draft, caret)?.pasted.number, 2, `caret ${caret}`)
  assert.equal(staging.markerAt(staged, draft, 0), null, 'plain text')
  assert.equal(staging.markerAt(staged, draft, draft.indexOf('[Image #1]') + 3), null, 'a picture marker')
  assert.equal(staging.markerAt(staged, `${draft}then ${marker}`, draft.length + 6)?.marker, marker, 'a second copy of the marker')
  assert.equal(staging.markerAt(undefined, draft, at), null)
})

test('a draft leaving the box carries the words, and what it names is found on the folded draft', () => {
  const folded = staging.stageText(staging.stagePaste(staging.EMPTY_STAGING, picture('p'), '', 0).staging, 'l1\nl2\nl3\nl4', 'explain [Image #1] ', 19)
  const queued = staging.leaving(folded.staging, `  ${folded.draft}  `)
  assert.equal(queued.words, 'explain [Image #1] l1\nl2\nl3\nl4')
  assert.deepEqual(queued.staged.map((item) => item.via), ['paste', 'text'])
  const aside = staging.leaving(folded.staging, `/btw ${folded.draft}`)
  assert.equal(exits.btwQuestion(aside.words), 'explain [Image #1] l1\nl2\nl3\nl4')
  const scheduled = staging.leaving(folded.staging, `/schedule ${folded.draft}`)
  assert.equal(exits.scheduleArgument(scheduled.words), 'explain [Image #1] l1\nl2\nl3\nl4')
})

// What `submit`, `queueDraft` and `interjectDraft` in use-turn.ts hand on: the prompt to `send`,
// the queued `prompt`, the line sent into the turn, the `/btw` question and the `/schedule`
// argument all come from `exitOf`, and a refused send or a failed aside puts its `line` back.
test('every exit of a draft holding a folded paste carries the pasted words', () => {
  const shot = staging.stagePaste(staging.EMPTY_STAGING, picture('p'), '', 0)
  const folded = staging.stageText(shot.staging, 'l1\nl2\nl3\nl4', `${shot.draft}and `, shot.caret + 4)
  const words = '[Image #1] and l1\nl2\nl3\nl4'
  const grants = { drops: [], pastes: ['p'] }

  const prompt = staging.exitOf(folded.staging, folded.draft)
  assert.deepEqual({ to: prompt.to, line: prompt.line }, { to: 'prompt', line: words }, 'the prompt sent or queued')
  assert.deepEqual(staging.grantsOf(prompt.staged), grants, 'the picture is still granted')
  assert.ok(!prompt.line.includes('[Pasted text'))

  const btw = staging.exitOf(folded.staging, `/btw ${folded.draft}`)
  assert.deepEqual({ to: btw.to, kind: btw.kind, question: btw.question, line: btw.line }, { to: 'aside', kind: 'btw', question: words, line: `/btw ${words}` })
  assert.deepEqual(staging.grantsOf(btw.staged), grants)

  const scheduled = staging.exitOf(folded.staging, `/schedule ${folded.draft}`)
  assert.deepEqual({ to: scheduled.to, argument: scheduled.argument, line: scheduled.line }, { to: 'schedule', argument: words, line: `/schedule ${words}` })
  assert.deepEqual(staging.grantsOf(scheduled.staged), grants)

  assert.equal(staging.exitOf(folded.staging, '/recap').to, 'aside')
  assert.equal(staging.exitOf(folded.staging, '   '), null)
  // A command is read from the words, as the terminal reads it from `self.unfolded(self.input.trim())`.
  const command = staging.stageText(staging.EMPTY_STAGING, '/schedule x\n2\n3\n4', '', 0)
  assert.equal(staging.exitOf(command.staging, command.draft).to, 'schedule')
})

// The marker and the threshold are the terminal's, read from where the terminal keeps them, so the
// two front ends cannot drift apart.
test('the folded marker and threshold are the terminal’s', () => {
  const catalog = readFileSync(join('..', 'crates', 'i18n', 'locales', 'en-US.ftl'), 'utf8')
  const entry = catalog.match(/^paste-folded = \{ \$lines ->\n((?:\s+\*?\[\w+\] .*\n)+)\s+\}/m)
  assert.ok(entry, 'paste-folded is in en-US.ftl')
  const forms = Object.fromEntries([...entry[1].matchAll(/\*?\[(\w+)\] (.*)/g)].map(([, key, form]) => [key, form.trim()]))
  const spelled = (key, number, lines) => forms[key].replace('{ $number }', number).replace('{ $lines }', lines)
  assert.equal(staging.foldedMarker(7, 1), spelled('one', 7, 1))
  assert.equal(staging.foldedMarker(7, 12), spelled('other', 7, 12))
  const source = readFileSync(join('..', 'crates', 'tui', 'src', 'state', 'attachments.rs'), 'utf8')
  assert.equal(staging.FOLD_AT_NEWLINES, Number(source.match(/const FOLD_AT_NEWLINES: usize = (\d+);/)[1]))
})

// A stub of the bridge's `pastes.check`, with a cap, a type, a noun and a note the agent does not
// use, so a number, literal or wording of this process's own would fail the tests below. The
// bridge's own answer is pinned in crates/ui-bridge/tests/attaching.rs.
const LIMIT = 1024 * 1024
const asked = []
const check = async (bytes) => {
  asked.push(bytes)
  return bytes > LIMIT ? { ok: false, note: `stub: ${bytes} is too many` } : { ok: true, media: 'image/x-test', noun: 'Picture' }
}

const paste = async (session, bytes = Buffer.from('pixels')) => {
  const outcome = await main.stagePaste(session, bytes, check)
  assert.equal(outcome?.kind, 'staged')
  return outcome.picture.id
}

test('a paste is granted as an id, the bridge’s noun and a drawing, and the page is told nothing of the bytes', async () => {
  const outcome = await main.stagePaste('s1', Buffer.from('a secret screenshot'), check)
  assert.equal(outcome.kind, 'staged')
  assert.deepEqual(Object.keys(outcome.picture).sort(), ['id', 'noun', 'thumbnail'])
  assert.equal(outcome.picture.noun, 'Picture')
  assert.ok(!JSON.stringify(outcome).includes(Buffer.from('a secret screenshot').toString('base64')))
  assert.equal(await main.stagePaste('nobody', Buffer.from('pixels'), check), null, 'a session this process does not hold is granted nothing')
  assert.equal(await main.stagePaste('s1', null, check), null, 'a clipboard with no picture stages nothing')
  assert.equal(await main.stagePaste('s1', Buffer.alloc(0), check), null)
  assert.equal(await main.stagePaste('s1', Buffer.from('not a picture'), check), null, 'bytes that do not decode stage nothing')
})

test('a bridge that does not answer, or answers something else, stages nothing', async () => {
  assert.equal(await main.stagePaste('s1', Buffer.from('pixels'), async () => { throw new Error('the agent is not running') }), null)
  assert.equal(await main.stagePaste('s1', Buffer.from('pixels'), async () => ({ ok: true, media: 'image/png' })), null)
  assert.equal(await main.stagePaste('s1', Buffer.from('pixels'), async () => ({ ok: false })), null)
})

test('a picture the bridge says is too large is refused with the bridge’s note, asked before and after it is re-encoded', async () => {
  asked.length = 0
  assert.deepEqual(await main.stagePaste('s1', Buffer.alloc(LIMIT + 1), check), { kind: 'too-large', note: `stub: ${LIMIT + 1} is too many` })
  assert.deepEqual(asked, [LIMIT + 1], 'refused before it is decoded')
  const grows = Buffer.alloc(LIMIT - 2)
  assert.deepEqual(await main.stagePaste('s1', grows, check), { kind: 'too-large', note: `stub: ${LIMIT + 2} is too many` })
  assert.deepEqual(asked.slice(1), [LIMIT - 2, LIMIT + 2], 'asked again about the PNG that would be sent')
  assert.equal((await main.stagePaste('s1', Buffer.alloc(LIMIT - 4), check)).kind, 'staged', 'one the bridge takes is staged')
})

test('a window’s turn carries what it pasted as PNG bytes named by the bridge’s type, in marker order, and nothing it sent itself', async () => {
  const first = await paste('s1', Buffer.from('first')), second = await paste('s1', Buffer.from('second'))
  const forwarded = main.sanitised('turn.send', {
    session: 's1', prompt: '[Image #1] [Image #2]', pastes: [first, second],
    images: [{ media: 'image/png', data: Buffer.from('forged').toString('base64') }],
  })
  assert.deepEqual(forwarded, {
    session: 's1', prompt: '[Image #1] [Image #2]', files: [],
    images: [
      { media: 'image/x-test', data: Buffer.from('PNG:first').toString('base64') },
      { media: 'image/x-test', data: Buffer.from('PNG:second').toString('base64') },
    ],
  })
  assert.deepEqual(main.sanitised('turn.send', { session: 's1', prompt: 'p', images: [{ media: 'image/gif', data: 'R0lGOA==' }] }),
    { session: 's1', prompt: 'p', files: [] }, 'a window’s own images never reach the agent')
})

test('bytes, an unknown id, or another session’s id where a paste goes refuses the send', async () => {
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', pastes: [Buffer.from('PNG:x').toString('base64')] }), main.SendRefused)
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', pastes: [{ media: 'image/png', data: 'eA==' }] }), main.SendRefused)
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', pastes: 'nope' }), main.SendRefused)
  const theirs = await paste('s2')
  assert.throws(() => main.sanitised('turn.send', { session: 's1', prompt: 'p', pastes: [theirs] }), /no longer available/)
  assert.equal(main.sanitised('turn.send', { session: 's2', prompt: 'p', pastes: [theirs] }).images.length, 1)
})

test('a closed session’s pictures are gone', async () => {
  const id = await paste('s2')
  main.forgetPastes('s2')
  assert.throws(() => main.sanitised('turn.send', { session: 's2', prompt: 'p', pastes: [id] }), main.SendRefused)
})

test('a bot’s turn keeps its briefing and carries what was pasted', async () => {
  const id = await paste('s1', Buffer.from('shot'))
  const carried = main.withPastes('s1', [id], main.withDrops('s1', [], { session: 's1', prompt: 'p', dropped: ['/app/briefing.md'] }))
  assert.deepEqual(carried.dropped, ['/app/briefing.md'], 'a bot’s briefing is left where it was')
  assert.deepEqual(carried.images, [{ media: 'image/x-test', data: Buffer.from('PNG:shot').toString('base64') }])
})

test('a manifest run takes no paste', async () => {
  const forwarded = main.sanitised('manifest.run', { session: 's1', task: 't', model: 'm', pastes: [await paste('s1')] })
  assert.deepEqual(forwarded, { session: 's1', task: 't', model: 'm' })
})

/**
 * The notes that say a size are the bridge's, built from the catalogs the terminal reads, so the
 * window cannot say one differently. A unit written into the window is the start of a second copy.
 */
test('nothing in the window says a size in megabytes', () => {
  const sources = readdirSync('src', { recursive: true }).filter((path) => /\.tsx?$/.test(path))
  const saying = sources.filter((path) => /\b(MB|MiB|Mo)\b/.test(readFileSync(join('src', path), 'utf8')))
  assert.deepEqual(saying, [])
})
