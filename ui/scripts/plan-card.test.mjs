// What a manifest run puts in front of a person: the plan to approve, and how the run ended.
//
// [MANIFEST-10](../../docs/specs/manifest.md#MANIFEST-10) says the frozen plan is put to a person
// with the task and every step, that an approval covers that plan only, and that it does not
// approve the plan's writes. [MANIFEST-11](../../docs/specs/manifest.md#MANIFEST-11) says a run is
// not part of the conversation. These tests hold the window to both.
//
// The rows are rendered through `react-dom/server` and the markup is what is asserted on.

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

const { Row } = load('src/renderer/components/Transcript.tsx')
const t = load('src/renderer/transcript.ts')

const draw = (entry) =>
  renderToStaticMarkup(
    React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: true }),
  )

const STEPS = ['1. [fetch] read CHANGELOG.md into log', '2. [act] write NOTES.md from log']

/** The question as the bridge sends it. */
const asked = (overrides = {}) =>
  t.askedManifest({ request: 4, task: 'write the release notes', steps: STEPS, ...overrides })

/** A run that stopped, as the bridge reports it. */
const ended = (overrides = {}) =>
  t.planEnded({
    run: 1, kind: 'precommit', message: 'internal', category: 'internal',
    stopped: false, declined: false, problem: null,
    attempt: { goal: 'Write the notes.', proposed: '{"steps": []}', plan: 'Plan, fixed before anything runs:\n  1. [act] write NOTES.md\n', steps: [] },
    record: 'run-123',
    ...overrides,
  })

const buttons = (markup) => [...markup.matchAll(/<button[^>]*>(.*?)<\/button>/g)].map((found) => found[1])

test('the plan card shows the task and every step, in order', () => {
  const many = Array.from({ length: 60 }, (_, index) => `${index + 1}. [fetch] read file-${index + 1}.md`)
  const markup = draw(asked({ steps: many }))

  assert.ok(markup.includes('write the release notes'), markup)
  assert.ok(markup.includes('60 steps'), markup)
  const drawn = [...markup.matchAll(/<li><code>(.*?)<\/code><\/li>/g)].map((found) => found[1])
  assert.deepEqual(drawn, many, 'a step was dropped, shortened or moved')
})

test('the plan card says what approving it does and does not do', () => {
  const markup = draw(asked())

  assert.ok(markup.includes('nothing re-plans once the run starts'), markup)
  assert.ok(markup.includes('Approving the plan does not approve its writes'), markup)
  assert.ok(markup.includes('This answer covers this plan only'), markup)
})

test('a waiting plan offers a refusal and one run, and no standing answer', () => {
  const markup = draw(asked())

  assert.deepEqual(buttons(markup), ['Don’t run', 'Run this plan'])
  assert.ok(!markup.includes('always'), markup)
})

test('a plan that was answered says which way, and offers nothing further', () => {
  const approved = draw({ ...asked(), decision: 'approve' })
  assert.ok(approved.includes('You approved this plan'), approved)
  assert.deepEqual(buttons(approved), [])

  const declined = draw({ ...asked(), decision: 'reject' })
  assert.ok(declined.includes('You declined this plan'), declined)
  assert.deepEqual(buttons(declined), [])
})

test('a plan whose run ended first cannot be approved from its card', () => {
  const markup = draw({ ...asked(), interrupted: true })

  assert.deepEqual(buttons(markup), [])
  assert.ok(markup.includes('Nobody answered this'), markup)
  assert.ok(markup.includes(STEPS[1]), 'the steps stay on the card')
})

test('only an answer to a plan is drawn on a plan', () => {
  const entries = [asked()]
  assert.equal(t.outstanding(entries).kind, 'manifest')
  assert.equal(t.REPLY.manifest, 'manifest.reply')

  for (const other of Object.keys(t.REPLY).filter((kind) => kind !== 'manifest')) {
    assert.equal(t.decide(entries, other, 4, 'approve')[0].decision, null, other)
  }
  assert.equal(t.decide(entries, 'manifest', 4, 'approve')[0].decision, 'approve')
})

test('the task of a run is drawn as a run’s task and cannot be forked from', () => {
  const markup = draw(t.planAsked('write the release notes'))

  assert.ok(markup.includes('class="plan-task-mark">Plan<'), markup)
  assert.ok(markup.includes('write the release notes'), markup)
  // A prompt offers a fork. A run's task is in no conversation, so there is nothing to cut.
  assert.deepEqual(buttons(markup), [])
  assert.ok(!markup.includes('fork-here'), markup)
})

test('what a run released is drawn as plain text in a marked container', () => {
  const released = '# A heading\n<b>bold</b> [a link](https://example.com) ![x](https://example.com/x.png)'
  const markup = draw(t.planReplied(released, 'run-123'))

  assert.ok(markup.includes('run result'), markup)
  assert.ok(markup.includes('<pre class="preview">'), markup)
  // Not formatted: no heading, no element from the text, no link and no image.
  for (const forbidden of ['<h1', '<b>', '<a ', 'href=', '<img', 'src=']) {
    assert.ok(!markup.includes(forbidden), `drew ${forbidden}: ${markup}`)
  }
  assert.ok(markup.includes('&lt;b&gt;bold&lt;/b&gt;'), markup)
  assert.ok(markup.includes('It is not part of this conversation'), markup)
  assert.ok(markup.includes('run-123'), markup)
})

test('a run is left out of an export and is not a prompt', () => {
  const entries = [
    t.planAsked('write the release notes'),
    asked(),
    t.planReplied('the notes', 'run-123'),
    ended(),
  ]

  assert.deepEqual(t.conversation(entries, true), [])
  assert.ok(entries.every((entry) => entry.kind !== 'user' && entry.kind !== 'assistant'))
})

test('a declined plan is said to be declined, read off the flag and not the sentence', () => {
  // The sentence is one the agent would not write, so the title can only have come from the flag.
  const declined = draw(ended({ declined: true, problem: 'some other wording entirely' }))
  assert.ok(declined.includes('The plan was declined, so nothing ran'), declined)
  assert.ok(declined.includes('class="plan-ended quiet"'), declined)

  // And wording that sounds like a decline, without the flag, is not drawn as one.
  const failed = draw(ended({ declined: false, problem: 'the plan was not approved' }))
  assert.ok(!failed.includes('The plan was declined'), failed)
  assert.ok(failed.includes('The run stopped'), failed)
})

test('a run the person stopped says so, and says it was not saved', () => {
  const markup = draw(ended({ stopped: true, record: null, problem: 'cancelled', attempt: { goal: null, proposed: null, plan: 'the plan', steps: ['1. [act] write NOTES.md: new file, 1 line'] } }))

  assert.ok(markup.includes('You stopped this run'), markup)
  assert.ok(markup.includes('The run was not saved'), markup)
  assert.ok(markup.includes('Steps that ran'), markup)
  assert.ok(markup.includes('write NOTES.md: new file, 1 line'), markup)
  assert.ok(!markup.includes('Saved as run'), markup)
})

test('a run that stopped shows its plan and names its record', () => {
  const markup = draw(ended({ problem: 'step 2 writes outside the workspace' }))

  assert.ok(markup.includes('step 2 writes outside the workspace'), markup)
  assert.ok(markup.includes('The plan'), markup)
  assert.ok(markup.includes('write NOTES.md'), markup)
  assert.ok(markup.includes('run-123'), markup)
})

test('a plan that could not be used shows what the planner proposed', () => {
  const markup = draw(ended({ problem: 'the manifest is not JSON', attempt: { goal: 'Write the notes.', proposed: 'Sure! Here is the plan', plan: null, steps: [] } }))

  assert.ok(markup.includes('What the planner proposed, which could not be used'), markup)
  assert.ok(markup.includes('Sure! Here is the plan'), markup)
})

test('a service failure in a run reads the category and never the service’s words', () => {
  const markup = draw(ended({ kind: 'chat', message: 'transport', category: 'transport', problem: null }))

  assert.ok(markup.includes('The model service could not be reached'), markup)
})
