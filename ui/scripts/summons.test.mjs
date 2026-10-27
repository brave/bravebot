/**
 * Summoning a bot for one turn (MEMORY-13): the record the main process keeps of which bot
 * answered which prompt, where its "Replied to" card goes, the marker a pick writes into the draft,
 * and that only the main process's summoned send names a definition for one turn.
 */

import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

const require = createRequire(import.meta.url)
const load = (contents, profile = '/nonexistent') => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  const electron = { app: { getPath: () => profile, getAppPath: () => process.cwd(), isPackaged: false } }
  new Function('require', 'module', 'exports', source)((id) => (id === 'electron' ? electron : require(id)), module, module.exports)
  return module.exports
}

const summons = load(`export * from './src/shared/summons'`)
const staging = load(`export * from './src/renderer/staging'; export { route } from './src/renderer/interjecting'`)
const transcript = load(`export { summonedReplies, summonedAt } from './src/renderer/transcript'`)
const bots = load(`export { parseBots } from './src/shared/bots'`)

const A = '11111111-1111-4111-8111-111111111111'
const B = '22222222-2222-4222-8222-222222222222'
const HOME = '33333333-3333-4333-8333-333333333333'
const project = { directory: '/work/site', id: A }
const record = (over = {}) => ({
  session: project, prompt: 2, slug: 'reviewer', host: { kind: 'project' }, after: null, reply: 'Looks fine.', at: 1000, ...over,
})

test('a stored summon is kept only where every field is one the main process could have written', () => {
  const kept = summons.parseSummons({ summons: [
    record(),
    record({ prompt: 3, host: { kind: 'bot', slug: 'ada' }, after: { id: HOME, prompt: 4 } }),
    record({ prompt: 4, slug: '../escape' }),
    record({ prompt: 5, host: { kind: 'bot', slug: 'Not A Slug' } }),
    record({ prompt: 6, session: { directory: 'relative', id: A } }),
    record({ prompt: -1 }),
    record({ prompt: 7, reply: 42 }),
    'not a record',
  ] }).summons
  assert.deepEqual(kept.map((each) => each.prompt), [2, 3])
  assert.deepEqual(kept[1].host, { kind: 'bot', slug: 'ada' })
  assert.deepEqual(kept[1].after, { id: HOME, prompt: 4 })
  assert.deepEqual(summons.parseSummons(null), { summons: [] })
})

test('a stored reply is cut to what a card keeps', () => {
  const long = summons.parseSummons({ summons: [record({ reply: 'x'.repeat(summons.REPLY_MAX + 50) })] }).summons[0]
  assert.equal(long.reply.length, summons.REPLY_MAX)
})

test('a prompt answered again replaces what was recorded at it, and other prompts stay', () => {
  const first = summons.withSummon([], record({ slug: 'reviewer' }))
  const other = summons.withSummon(first, record({ prompt: 5, slug: 'ada' }))
  const again = summons.withSummon(other, record({ slug: 'ada', at: 2000 }))
  assert.deepEqual(again.map((each) => [each.prompt, each.slug]), [[2, 'ada'], [5, 'ada']])
  assert.deepEqual(summons.withoutSummonAt(again, project, 2).map((each) => each.prompt), [5])
  assert.deepEqual(summons.withoutSession(again, project), [])
  assert.equal(summons.withoutSession(again, { directory: '/work/site', id: B }).length, 2)
})

test('the label map names the bot for each summoned prompt of the open conversation only', () => {
  const all = [record(), record({ session: { directory: '/work/site', id: B }, prompt: 9, slug: 'ada' })]
  assert.deepEqual([...summons.summonedIn(all, project)], [[2, 'reviewer']])
  assert.equal(summons.summonedIn(all, null).size, 0)
})

test('a turn that ends without a summon removes what was recorded at its prompt, and a summoned one records it', () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-summons-'))
  try {
    const store = load(`export * from './src/main/summons'`, profile)
    const pending = { slug: 'reviewer', host: { kind: 'bot', slug: 'ada' }, after: { id: HOME, prompt: 1 } }
    store.noteTurnEnded(project, 2, pending, 'Reviewed.')
    store.noteTurnEnded(project, 3, null, 'Not summoned.')
    const written = JSON.parse(readFileSync(join(profile, 'bravebot-ui.json'), 'utf8')).summons
    assert.deepEqual(written.map((each) => [each.prompt, each.slug, each.reply]), [[2, 'reviewer', 'Reviewed.']])
    assert.deepEqual(written[0].host, { kind: 'bot', slug: 'ada' })
    // The same prompt sent again after a rewind, this time to nobody in particular.
    store.noteTurnEnded(project, 2, null, 'Plain reply.')
    assert.deepEqual(store.summons(), [])
    store.noteTurnEnded(project, 4, pending, 'Again.')
    store.forgetSummons(project)
    assert.deepEqual(store.summons(), [])
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

const rows = (...prompts) => prompts.map((prompt, at) => prompt === 'reply'
  ? { id: `r${at}`, kind: 'assistant' }
  : { id: `u${at}`, kind: 'user', ...(prompt === null ? {} : { prompt }) })

test('a card follows the turn its record says the bot’s conversation had reached', () => {
  // Prompts 0 and 1 have been answered; the bot was summoned elsewhere after prompt 0.
  const transcriptRows = rows(0, 'reply', 1, 'reply')
  const after0 = record({ slug: 'ada', after: { id: HOME, prompt: 0 } })
  const after1 = record({ slug: 'ada', prompt: 3, after: { id: HOME, prompt: 1 }, at: 2000 })
  const placed = summons.placeCards([after1, after0], 'ada', HOME, transcriptRows)
  assert.deepEqual([...placed.before], [['u2', [after0]]])
  assert.deepEqual(placed.end, [after1])
})

test('a card goes after a prompt this window has not been told the number of yet, once the prompt it follows is passed', () => {
  const placed = summons.placeCards([record({ slug: 'ada', after: { id: HOME, prompt: 0 } })], 'ada', HOME, rows(0, 'reply', null))
  assert.deepEqual([...placed.before.keys()], ['u2'])
})

test('a card from before the conversation began, or from another of the bot’s conversations, goes first', () => {
  const none = record({ slug: 'ada', after: null })
  const elsewhere = record({ slug: 'ada', prompt: 3, after: { id: B, prompt: 0 }, at: 2000 })
  const placed = summons.placeCards([none, elsewhere], 'ada', HOME, rows(0, 'reply'))
  assert.deepEqual([...placed.before], [['u0', [none, elsewhere]]])
  assert.deepEqual(summons.placeCards([none], 'ada', HOME, []).end, [none])
})

test('a bot’s conversation shows only the cards of its own replies elsewhere', () => {
  const others = [record({ slug: 'reviewer' }), record({ slug: 'ada', host: { kind: 'bot', slug: 'ada' } })]
  const placed = summons.placeCards(others, 'ada', HOME, rows(0))
  assert.equal(placed.before.size, 0)
  assert.deepEqual(placed.end, [])
})

test('the marker for a bot is never an @ name, so the bridge does not read it as a file', () => {
  assert.equal(summons.summonMarker('Code Reviewer', 'code-reviewer'), '[@Code Reviewer]')
  assert.equal(summons.summonMarker('[@x]', 'x'), '[@x]')
  assert.equal(summons.summonMarker('  ', 'fallback'), '[@fallback]')
  assert.doesNotMatch(summons.summonMarker('Ada', 'ada'), /^@/)
})

const ada = { slug: 'ada', name: 'Ada', avatar: 'v2:ada' }
const reviewer = { slug: 'reviewer', name: 'Reviewer', avatar: 'v2:reviewer' }

test('picking a bot writes its marker in place of the half-typed name, and a second pick replaces the first', () => {
  const empty = { made: 0, staged: [] }
  const first = staging.stageSummon(empty, ada, 'look at this @ad', '@ad')
  assert.equal(first.draft, 'look at this [@Ada] ')
  assert.equal(first.caret, first.draft.length)
  assert.deepEqual(staging.summonOf(staging.named(first.staging, first.draft)), ada)
  const second = staging.stageSummon(first.staging, reviewer, `${first.draft}and @rev`, '@rev')
  assert.equal(second.draft, 'look at this and [@Reviewer] ')
  assert.deepEqual(second.staging.staged.map((item) => item.bot.slug), ['reviewer'])
  assert.equal(staging.stageSummon(empty, ada, 'no name here', '@ad'), null)
})

test('a message whose marker was deleted summons nobody, and a summoned line waits for the running turn to end', () => {
  const staged = staging.stageSummon({ made: 0, staged: [] }, ada, '@a', '@a')
  assert.equal(staging.summonOf(staging.named(staged.staging, 'plain words')), null)
  assert.equal(staging.route({}, staging.named(staged.staging, staged.draft)), 'queue')
  assert.equal(staging.route({}, []), 'interject')
  assert.deepEqual(staging.grantsOf(staging.named(staged.staging, staged.draft)), { drops: [], pastes: [] })
})

test('the label is drawn over the first reply to a summoned prompt and over no reply after it', () => {
  const entries = [
    { kind: 'user', id: 'p1', text: '[@Ada] hi', prompt: 0, summoned: 'ada' },
    { kind: 'assistant', id: 'a1', text: 'first' },
    { kind: 'assistant', id: 'a2', text: 'second' },
    { kind: 'user', id: 'p2', text: 'next', prompt: 1 },
    { kind: 'assistant', id: 'a3', text: 'own' },
  ]
  assert.deepEqual([...transcript.summonedReplies(entries)], [['a1', 'ada']])
})

test('a reopened conversation marks its prompts from the record, by ordinal', () => {
  const entries = [{ kind: 'user', id: 'p1', text: 'hi', prompt: 0 }, { kind: 'user', id: 'p2', text: 'again', prompt: 1 }]
  const marked = transcript.summonedAt(entries, new Map([[1, 'reviewer']]))
  assert.equal(marked[0].summoned, undefined)
  assert.equal(marked[1].summoned, 'reviewer')
  assert.equal(transcript.summonedAt(entries, new Map()), entries)
})

test('a bot’s place in its own conversation is kept only where it names one', () => {
  const base = { slug: 'ada', name: 'Ada', purpose: 'Help', home: '/homes/ada', avatar: 'v2:ada' }
  assert.deepEqual(bots.parseBots({ bots: [{ ...base, lastHome: { id: HOME, prompt: 3 } }] }).bots[0].lastHome, { id: HOME, prompt: 3 })
  for (const lastHome of [undefined, null, { id: '../x', prompt: 1 }, { id: HOME, prompt: -1 }, { id: HOME, prompt: 1.5 }, { id: HOME }]) {
    assert.equal(bots.parseBots({ bots: [{ ...base, lastHome }] }).bots[0].lastHome, null)
  }
})

test('a turn in a bot’s home moves its place there, and a turn in a project does not', () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-last-home-'))
  try {
    const store = load(`export * from './src/main/bots'`, profile)
    const base = bots.parseBots({ bots: [{ slug: 'ada', name: 'Ada', purpose: 'Help', home: join(profile, 'home'), avatar: 'v2:ada' }] }).bots[0]
    store.saveBot(base)
    store.noteBotSession('ada', HOME, base.home, 2)
    assert.deepEqual(store.bot('ada').lastHome, { id: HOME, prompt: 2 })
    store.noteBotSession('ada', A, '/work/site', 7)
    assert.deepEqual(store.bot('ada').lastHome, { id: HOME, prompt: 2 })
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

test('only the main process’s summoned send names a definition for one turn, and from the bot row', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const assignments = (index.match(/\baddress\b\s*[:=][^,}\n]+/g) ?? []).map((each) => each.trim())
  assert.deepEqual(assignments, ['address: summoned.definition'])
  const sending = index.slice(index.indexOf('async function sendSummonedTurn'), index.indexOf('async function sendSummonedTurn') + 3000)
  assert.match(sending, /address: summoned\.definition/)
  assert.match(sending, /migrateBot\(summoned/)
})
