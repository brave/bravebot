// How the window puts the planner's questions to a person: one at a time with the position shown,
// a skip that moves on, and every answer sent together once the last is settled.
//
// The walkthrough (`drive-manual-walkthrough.mjs`) drives the whole path through a real window and
// bridge. These hold the two halves that need no window: how the stepping follows the bridge, and
// what the card draws for the question it is on. Where a question stands and whether settling it
// ends the series are the bridge's (`wire::ask_request`), so the fixtures carry them as the bridge
// sends them, and nothing here counts the questions. The card is rendered through `react-dom/server` and the
// markup is what is asserted on.

import test from 'node:test'
import assert from 'node:assert/strict'
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

const asking = load('src/renderer/asking.ts')
const { Row } = load('src/renderer/components/Transcript.tsx')
const t = load('src/renderer/transcript.ts')

const prompt = (header, question, labels = [], multiple = false, place = null, last = true) => ({
  header,
  question,
  rows: labels.map((label, index) => ({ index, label, detail: null })),
  multiple,
  key: question,
  place,
  last,
})

// What the bridge sends for a series of three. The wording is the bridge's, so a window that
// composed its own would not match it.
const SERIES = [
  prompt('Approach', 'Which approach should the flag take?', ['Global flag', 'Per command'], false, 'question 1 sur 3', false),
  prompt('Name', 'What should the flag be called?', ['--json', '--format json'], false, 'question 2 sur 3', false),
  prompt('Tests', 'Which suites should run?', ['Unit', 'Integration'], true, 'question 3 sur 3', true),
]

const draw = (entry) =>
  renderToStaticMarkup(
    React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: true }),
  )

const text = (markup) => markup.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim()

test('a series is over when the bridge says the question settled was the last, whatever the answer was', () => {
  const [one, two, three] = SERIES
  const first = asking.settle(asking.first, { chosen: [1] }, one.last)
  assert.ok('step' in first, 'the series ended on its first answer')
  assert.equal(first.step.at, 1)

  const second = asking.settle(first.step, {}, two.last)
  assert.ok('step' in second, 'a skipped question ended the series')
  assert.equal(second.step.at, 2)

  // A skip on the last question ends it exactly as an answer does.
  const skipped = asking.settle(second.step, {}, three.last)
  const answered = asking.settle(second.step, { chosen: [0] }, three.last)
  assert.ok('answers' in skipped && 'answers' in answered, 'the last question did not end the series')
})

test('the answers go back in the order the questions were put, with a skip as an empty answer', () => {
  let step = asking.first
  const given = [{ typed: 'beta' }, {}, { chosen: [0, 1] }]
  let sent
  for (const [at, answer] of given.entries()) {
    const next = asking.settle(step, answer, SERIES[at].last)
    if ('answers' in next) sent = next.answers
    else step = next.step
  }
  assert.deepEqual(sent, given, 'an answer moved or a skip was dropped')
})

test('settling does not change the step it was given', () => {
  const before = { at: 0, settled: [] }
  asking.settle(before, { typed: 'a' }, false)
  assert.deepEqual(before, { at: 0, settled: [] })
  assert.deepEqual(asking.first, { at: 0, settled: [] })
})

test('typed words win over a selection, and neither is a skip', () => {
  assert.deepEqual(asking.answerOf([1], '  my own words  '), { typed: 'my own words' })
  assert.deepEqual(asking.answerOf([2, 0], ''), { chosen: [2, 0] })
  assert.deepEqual(asking.answerOf([], '   '), {}, 'blank words are not an answer')
})

test('progress kept for a series comes back for that series and no other', () => {
  const progress = { step: { at: 1, settled: [{ chosen: [0] }] }, chosen: [1], typed: 'half a sentence' }
  asking.keep('series-a', progress)

  assert.deepEqual(asking.recall('series-a'), progress, 'what was settled and entered was not kept')
  assert.deepEqual(asking.recall('series-b'), { step: asking.first, chosen: [], typed: '' }, 'another series started part way through')

  asking.release('series-a')
  assert.deepEqual(asking.recall('series-a'), { step: asking.first, chosen: [], typed: '' }, 'an answered series kept its progress')
})

test('the card puts the first question of a series and none of the others', () => {
  const markup = draw(t.askedQuestions({ request: 7, prompts: SERIES }))

  assert.ok(markup.includes('question 1 sur 3'), `the card did not say the position the bridge sent: ${markup}`)
  assert.ok(!markup.includes('Question 1 of 3'), `the card worded a position of its own: ${markup}`)
  assert.ok(markup.includes(SERIES[0].question), markup)
  for (const later of SERIES.slice(1)) assert.ok(!markup.includes(later.question), `${later.question} is drawn before its turn`)
  assert.equal(markup.match(/<fieldset/g).length, 1, 'more than one question is on screen')
})

test('the card offers a skip beside the answer for the question it is on', () => {
  const markup = draw(t.askedQuestions({ request: 7, prompts: SERIES }))

  // Leo buttons are custom elements that carry no attributes when rendered on the server, so
  // whether Next is open is read in the walkthrough, which drives the real window.
  const buttons = [...markup.matchAll(/<leo-button>(.*?)<\/leo-button>/g)].map((found) => found[1])
  assert.deepEqual(buttons, ['Skip', 'Next'])
})

test('a lone question is not counted and its button says it ends the series', () => {
  const markup = draw(t.askedQuestions({ request: 8, prompts: [prompt('Approach', 'Which approach?', ['Global flag'])] }))

  assert.ok(markup.includes('1 question'), markup)
  assert.ok(!markup.includes(' of 1'), markup)
  const buttons = [...markup.matchAll(/<leo-button>(.*?)<\/leo-button>/g)].map((found) => found[1])
  assert.deepEqual(buttons, ['Skip', 'Answer'])
})

test('the card ends the series where the bridge says it ends, and not where its own count would', () => {
  // The first of two, marked as the last. A card that counted its questions would offer Next.
  const marked = [{ ...SERIES[0], last: true }, SERIES[1]]
  const markup = draw(t.askedQuestions({ request: 10, prompts: marked }))

  const buttons = [...markup.matchAll(/<leo-button>(.*?)<\/leo-button>/g)].map((found) => found[1])
  assert.deepEqual(buttons, ['Skip', 'Answer'], 'the button followed a count the card kept')
})

test('an answered series is a record of every question, with what each was answered with', () => {
  const entry = { ...t.askedQuestions({ request: 9, prompts: SERIES }), answers: [{ chosen: [1] }, {}, { chosen: [0, 1] }] }
  const markup = draw(entry)

  assert.ok(markup.includes('3 questions'), markup)
  const given = [...markup.matchAll(/<div class="given">(.*?)<\/div>/g)].map((found) => text(found[1]))
  assert.deepEqual(given, ['Per command', 'Declined', 'Unit, Integration'])
  assert.ok(!markup.includes('<fieldset'), 'an answered series still offers its choices')
})
