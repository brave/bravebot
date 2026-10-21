// How the window puts the planner's questions to a person: one at a time with the position shown,
// a skip that moves on, and every answer sent together once the last is settled.
//
// The walkthrough (`drive-manual-walkthrough.mjs`) drives the whole path through a real window and
// bridge. These hold the two halves that need no window: what the stepping decides, and what the
// card draws for the question it is on. The card is rendered through `react-dom/server` and the
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

const prompt = (header, question, labels = [], multiple = false) => ({
  header,
  question,
  rows: labels.map((label, index) => ({ index, label, detail: null })),
  multiple,
  key: question,
})

const SERIES = [
  prompt('Approach', 'Which approach should the flag take?', ['Global flag', 'Per command']),
  prompt('Name', 'What should the flag be called?', ['--json', '--format json']),
  prompt('Tests', 'Which suites should run?', ['Unit', 'Integration'], true),
]

const draw = (entry) =>
  renderToStaticMarkup(
    React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: true }),
  )

const text = (markup) => markup.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim()

test('a series is over when the last question is settled, whatever that answer was', () => {
  const total = SERIES.length
  const first = asking.settle(asking.first, { chosen: [1] }, total)
  assert.ok('step' in first, 'the series ended on its first answer')
  assert.equal(first.step.at, 1)

  const second = asking.settle(first.step, {}, total)
  assert.ok('step' in second, 'a skipped question ended the series')
  assert.equal(second.step.at, 2)

  // A skip on the last question ends it exactly as an answer does.
  const skipped = asking.settle(second.step, {}, total)
  const answered = asking.settle(second.step, { chosen: [0] }, total)
  assert.ok('answers' in skipped && 'answers' in answered, 'the last question did not end the series')
})

test('the answers go back in the order the questions were put, with a skip as an empty answer', () => {
  let step = asking.first
  const given = [{ typed: 'beta' }, {}, { chosen: [0, 1] }]
  let sent
  for (const answer of given) {
    const next = asking.settle(step, answer, given.length)
    if ('answers' in next) sent = next.answers
    else step = next.step
  }
  assert.deepEqual(sent, given, 'an answer moved or a skip was dropped')
})

test('settling does not change the step it was given', () => {
  const before = { at: 0, settled: [] }
  asking.settle(before, { typed: 'a' }, 3)
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

test('a question says where it stands in a series, and a lone question says nothing', () => {
  assert.equal(asking.position(0, 3), 'Question 1 of 3')
  assert.equal(asking.position(2, 3), 'Question 3 of 3')
  assert.equal(asking.position(0, 1), null)
})

test('the card puts the first question of a series and none of the others', () => {
  const markup = draw(t.askedQuestions({ request: 7, prompts: SERIES }))

  assert.ok(markup.includes('Question 1 of 3'), markup)
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
  const markup = draw(t.askedQuestions({ request: 8, prompts: [SERIES[0]] }))

  assert.ok(markup.includes('1 question'), markup)
  assert.ok(!markup.includes('Question 1 of 1'), markup)
  const buttons = [...markup.matchAll(/<leo-button>(.*?)<\/leo-button>/g)].map((found) => found[1])
  assert.deepEqual(buttons, ['Skip', 'Answer'])
})

test('an answered series is a record of every question, with what each was answered with', () => {
  const entry = { ...t.askedQuestions({ request: 9, prompts: SERIES }), answers: [{ chosen: [1] }, {}, { chosen: [0, 1] }] }
  const markup = draw(entry)

  assert.ok(markup.includes('3 questions'), markup)
  const given = [...markup.matchAll(/<div class="given">(.*?)<\/div>/g)].map((found) => text(found[1]))
  assert.deepEqual(given, ['Per command', 'Declined', 'Unit, Integration'])
  assert.ok(!markup.includes('<fieldset'), 'an answered series still offers its choices')
})
