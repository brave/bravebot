// What the window shows when the planner proposes consulting a bot.
//
// [CONSULT-12](../../docs/specs/consulting-a-bot.md#CONSULT-12) says the person is shown the bot,
// the model it answers on and its bound, and the brief, and that nothing is sent without a yes.
//
// The card is rendered through `react-dom/server` and the markup is what is asserted on.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

function load(path) {
  const source = buildSync({
    entryPoints: [path],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { Row, BotFaces } = load('src/renderer/components/Transcript.tsx')
const t = load('src/renderer/transcript.ts')
const { TurnNotices } = load('src/renderer/components/TurnDetails.tsx')

const draw = (entry) =>
  renderToStaticMarkup(
    React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false }),
  )

const asked = (overrides = {}) =>
  t.askedConsult({
    request: 5,
    bot: 'reviewer',
    definition: '~/.bravebot/agents/reviewer.md',
    model: 'claude-haiku',
    rounds: 8,
    budget: 20000,
    brief: 'Review the retry loop in fetch.rs.',
    label: '(U,pub)',
    ...overrides,
  })

const buttons = (markup) => [...markup.matchAll(/<button[^>]*>(.*?)<\/button>/g)].map((found) => found[1])

test('the card names the bot, where it is defined, its model and its bound', () => {
  const markup = draw(asked())

  assert.ok(markup.includes('asking the bot <code>reviewer</code>'), markup)
  assert.ok(markup.includes('~/.bravebot/agents/reviewer.md'), markup)
  assert.ok(markup.includes('claude-haiku'), markup)
  assert.ok(markup.includes('up to 8 rounds and 20000 tokens'), markup)
})

test('a bound with no token budget names only its rounds', () => {
  const markup = draw(asked({ budget: null }))

  assert.ok(markup.includes('up to 8 rounds.'), markup)
  assert.ok(!markup.includes('tokens'), markup)
})

test('the card shows the brief with its label, and says nothing is sent without a yes', () => {
  const markup = draw(asked())

  assert.ok(markup.includes('Review the retry loop in fetch.rs.'), markup)
  assert.ok(markup.includes('labelled (U,pub)'), markup)
  assert.ok(markup.includes('Nothing is sent unless you say yes'), markup)
})

test('a waiting proposal offers to send it or not', () => {
  assert.deepEqual(buttons(draw(asked())), ['Don’t send it', 'Yes, send it'])
})

test('a proposal that was answered says which way, and offers nothing further', () => {
  const sent = draw({ ...asked(), decision: 'approve' })
  assert.ok(sent.includes('You sent this consult'), sent)
  assert.deepEqual(buttons(sent), [])

  const declined = draw({ ...asked(), decision: 'reject' })
  assert.ok(declined.includes('You did not send this consult'), declined)
  assert.deepEqual(buttons(declined), [])
})

// CONSULT-10, CONSULT-12: a proposed bot this machine cannot run comes with no card, only the
// turn's notice naming the model, drawn under the turn with nothing to dismiss.
test('a bot this machine cannot run is a notice under the turn, and no card', () => {
  const notice = 'The planner proposed consulting reviewer, which cannot be consulted here: reviewer asked for claude-opus, which needs a sign-in first, so it did not run. Nothing was sent, and no other model was asked in its place.'
  const session = drafting()
  session.send('turn.started', { turn: 1 })
  session.send('turn.done', { turn: 1, notices: [notice] })

  assert.ok(!session.live.entries.some((entry) => entry.kind === 'consult'), 'a card was drawn')
  const details = session.live.turns[1]
  assert.deepEqual(details.notices, [notice])
  const markup = renderToStaticMarkup(React.createElement(TurnNotices, { details, onDisclosure() {} }))
  assert.ok(markup.includes('claude-opus, which needs a sign-in first'), markup)
  assert.deepEqual(buttons(markup).filter((label) => /close|dismiss/i.test(label)), [])
})

const reply = (faces, bot) =>
  renderToStaticMarkup(
    React.createElement(BotFaces.Provider, { value: faces },
      React.createElement(Row, {
        entry: { id: 'r1', kind: 'assistant', text: 'The loop never backs off.', bot },
        onDecide() {}, onAnswer() {}, onFork() {}, forkable: false,
      })),
  )

const seedOf = (markup) => markup.match(/data-avatar="([^"]*)"/)?.[1]

test('a consulted reply carries the bot’s face and name, so it never reads as the host’s', () => {
  const faces = new Map([['reviewer', { name: 'Rule Reviewer', avatar: 'v2:seed-a' }]])
  const markup = reply(faces, 'reviewer')

  assert.ok(markup.includes('bubble assistant consulted'), markup)
  assert.ok(markup.includes('Rule Reviewer'), markup)
  assert.ok(markup.includes('@reviewer · consulted'), markup)
  assert.ok(seedOf(markup), markup)
  assert.notEqual(seedOf(markup), seedOf(reply(faces, 'other')), 'two bots draw one face')
})

test('a bot defined outside the window is drawn with its slug and a face from it', () => {
  const markup = reply(new Map(), 'hand-written')

  assert.ok(markup.includes('class="bot-avatar"'), markup)
  assert.ok(markup.includes('@hand-written · consulted'), markup)
})

test('the host’s own reply carries no bot header', () => {
  const markup = reply(new Map(), undefined)

  assert.ok(!markup.includes('consulted'), markup)
  assert.ok(!markup.includes('bot-avatar'), markup)
})

const line = (faces, text, lineTags) =>
  renderToStaticMarkup(
    React.createElement(BotFaces.Provider, { value: faces },
      React.createElement(Row, { entry: { id: 'u1', kind: 'user', text, ...(lineTags ? { tags: lineTags } : {}) }, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false })),
  )

const tags = (markup) => [...markup.matchAll(/class="bot-tag" data-bot="([^"]*)"/g)].map((found) => found[1])

test('a line tagging a bot draws its face beside the tag, and keeps every word', () => {
  const faces = new Map([['dev-bot', { name: 'Dev Bot', avatar: 'v2:seed-a' }]])
  const markup = line(faces, 'Ask @dev-bot, what he thinks of this', [{ start: 4, end: 12, bot: 'dev-bot' }])

  assert.deepEqual(tags(markup), ['dev-bot'])
  assert.ok(markup.includes('class="bot-face"'), markup)
  assert.equal(markup.replace(/<[^>]*>/g, ''), 'Ask @dev-bot, what he thinks of this')
})

// CONSULT-2, CONSULT-14: which words are tags was decided when the line was sent, so a bot this
// window knows now, one that opted out or one made since, is not drawn as a tag in an earlier line.
test('a line is tagged only where the bridge said, whatever bots the window knows now', () => {
  const faces = new Map([['notes', { name: 'Notes', avatar: 'a' }], ['dev-bot', { name: 'Dev Bot', avatar: 'b' }], ['later', { name: 'Later', avatar: 'c' }]])

  assert.deepEqual(tags(line(faces, 'ask @dev-bot and @later')), [])
  assert.deepEqual(tags(line(faces, 'ask @dev-bot and @later', [{ start: 4, end: 12, bot: 'dev-bot' }])), ['dev-bot'])
})

test('a tag that does not sit on its bot’s name in the line is not drawn', () => {
  const faces = new Map([['dev-bot', { name: 'Dev Bot', avatar: 'b' }]])
  const text = 'ask @dev-bot'

  assert.deepEqual(tags(line(faces, text, [{ start: 0, end: 8, bot: 'dev-bot' }])), [])
  assert.equal(line(faces, text, [{ start: 0, end: 8, bot: 'dev-bot' }]).replace(/<[^>]*>/g, ''), text)
})

test('a bot the window has no face for is tagged with one drawn from its slug', () => {
  const markup = line(new Map(), 'ask @outside', [{ start: 4, end: 12, bot: 'outside' }])

  assert.deepEqual(tags(markup), ['outside'])
})

test('the turn a brief starts tags the line it was sent as, from the bridge', () => {
  const session = drafting()
  session.live = { ...session.live, entries: [t.userSaid('ask @dev-bot')] }
  session.live = { ...session.live, awaitingOrdinal: session.live.entries[0].id }
  session.send('turn.started', { turn: 1, consulted: 'dev-bot', tags: [{ start: 4, end: 12, bot: 'dev-bot' }] })

  assert.deepEqual(session.live.entries[0].tags, [{ start: 4, end: 12, bot: 'dev-bot' }])
})

test('a resumed line keeps the tags the bridge sent with it', () => {
  const [said] = t.fromSaid([{ kind: 'user', text: 'ask @dev-bot', prompt: 0, tags: [{ start: 4, end: 12, bot: 'dev-bot' }] }])

  assert.deepEqual(said.tags, [{ start: 4, end: 12, bot: 'dev-bot' }])
})

test('the brief is drawn as text and never as markup', () => {
  const forged = '<img src="https://example.com/x.png"><a href="https://example.com">x</a>'
  const markup = draw(asked({ brief: forged, bot: forged }))

  for (const forbidden of ['<img', '<a ', '<a>']) {
    assert.ok(!markup.includes(forbidden), `drew ${forbidden}: ${markup}`)
  }
})

test('only an answer to this proposal is drawn on it', () => {
  const entries = [asked()]
  assert.equal(t.outstanding(entries).kind, 'consult')

  for (const other of Object.keys(t.REPLY).filter((kind) => kind !== 'consult')) {
    assert.equal(t.decide(entries, other, 5, 'approve')[0].decision, null, other)
  }
  assert.equal(t.decide(entries, 'consult', 6, 'approve')[0].decision, null, 'another request')
  assert.equal(t.decide(entries, 'consult', 5, 'approve')[0].decision, 'approve')
})

test('the window sends the answer on a channel the main process lets through', () => {
  assert.equal(t.REPLY.consult, 'consult.reply')

  const main = readFileSync('src/main/index.ts', 'utf8')
  const start = main.indexOf('const ALLOWED = new Set([')
  const end = main.indexOf('])', start)
  assert.ok(start >= 0 && end > start, 'the allow-list is not where this test looks for it')
  const allowed = new Set([...main.slice(start, end).matchAll(/'([a-z.-]+)'/g)].map((found) => found[1]))
  assert.ok(allowed.has('consult.reply'), 'consult.reply is not in the allow-list')
  assert.ok(allowed.has('consult.brief'), 'consult.brief is not in the allow-list')
  assert.ok(main.includes('if (brief) params.brief = true'), 'a bot session does not send its brief with the flag')
})

// CONSULT-5: a line naming a bot is drafted into a brief by the host, which comes back to the
// composer to be read and edited before anything is sent.
const { apply } = load('src/renderer/apply.ts')
const { route, mayGo } = load('src/renderer/interjecting.ts')

const drafting = () => {
  const session = {
    live: { handle: 's-1', summary: { title: '', project: '', branch: null, directory: '/', id: 'a' }, entries: [], turns: {}, todos: [], quarantine: [], phase: null, checking: null, hook: null, composing: null, tokens: 0, running: false, archived: 0, awaitingOrdinal: null, bot: null },
  }
  session.send = (event, data) => apply({ event, data }, (update) => { session.live = update(session.live) }, () => {}, () => {})
  return session
}

const later = (live) => ({ ...live, queued: [{ prompt: 'later' }], queuePaused: false })

test('drafting holds the session, and the draft that comes back waits for the person', () => {
  const session = drafting()
  session.send('consult.drafting', { bot: 'reviewer', row: 'up to 8 rounds', line: 'ask @reviewer about this', skipped: [] })
  assert.equal(session.live.running, true)
  assert.deepEqual(session.live.brief, { bot: 'reviewer', row: 'up to 8 rounds', line: 'ask @reviewer about this', drafting: true, message: null })
  assert.equal(route(session.live), 'queue', 'a line typed while the host drafts is not read by it')

  session.send('consult.brief', { bot: 'reviewer', line: 'ask @reviewer about this', brief: 'Review fetch.rs', message: 'Brief ready.' })
  assert.equal(session.live.running, false, 'a draft ends with consult.brief, never turn.done')
  assert.deepEqual(session.live.brief, { bot: 'reviewer', line: 'ask @reviewer about this', drafting: false, message: 'Brief ready.' })
  assert.deepEqual(session.live.entries, [], 'nothing is said in the conversation until the brief is sent')
  assert.equal(mayGo(later(session.live)), false, 'a waiting brief goes, or is put back, before the queue')
})

// CONSULT-10: a bot the line named that may not be consulted is said, and the next one is briefed.
test('each bot passed over before the one being briefed is said', () => {
  const session = drafting()
  session.send('consult.drafting', { bot: 'reviewer', row: '', line: 'ask @first and @reviewer', skipped: ['first needs a sign-in'] })

  assert.deepEqual(session.live.entries.map((entry) => entry.text), ['first needs a sign-in'])
  assert.equal(session.live.brief.bot, 'reviewer')
})

test('a draft that did not come says why, and leaves nothing waiting', () => {
  const session = drafting()
  session.send('consult.drafting', { bot: 'reviewer', row: '', line: 'ask @reviewer', skipped: [] })
  session.send('consult.brief', { bot: 'reviewer', line: 'ask @reviewer', brief: null, message: 'You declined the brief for reviewer.' })

  assert.equal(session.live.running, false)
  assert.equal(session.live.brief, null)
  assert.equal(session.live.entries.at(-1).text, 'You declined the brief for reviewer.')
  assert.equal(mayGo(later(session.live)), true)
})
