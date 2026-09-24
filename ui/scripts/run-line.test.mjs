// What the run card says the planner wrote, above the plan that line compiled to.
//
// [CMDLINE-3](../../docs/specs/tools/command-line.md#CMDLINE-3) binds an endorsement to the plan
// and not to the text, and shows the text anyway: a reader given only the plan has nothing to
// compare it against, and that comparison is what would catch a compiler that read the line
// wrong. A glob resolving to a file nobody meant, or a redirection landing elsewhere, draws a
// card indistinguishable from a correct one on a surface that shows one of the two.
//
// So what has to be checked here is the drawing: that the card carries the line as well as the
// plan, that neither is drawn in place of the other, and that a call which was never a line draws
// no empty row where the comparison would be.

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
    React.createElement(Row, {
      entry,
      onDecide() {},
      onAnswer() {},
      onFork() {},
      forkable: false,
    }),
  )

/** The run question as the bridge sends it, for a line whose glob the harness expanded. */
const asked = (line) =>
  t.askedRun({
    request: 1,
    line,
    plan: '/usr/bin/wc -l notes.txt report.txt',
    stages: [
      {
        program: 'wc',
        resolved: '/usr/bin/wc',
        args: ['-l', 'notes.txt', 'report.txt'],
        display: 'wc -l notes.txt report.txt',
      },
    ],
    directory: '/home/someone/project',
    releasesPrivate: false,
    ambient: [],
    vouches: [{ program: '/usr/bin/wc', args: ['-l'], display: 'wc -l' }],
    summary: 'run 1 step in /home/someone/project',
  })

test('the run card shows the line the planner wrote beside the plan it compiled to', () => {
  const drawn = draw(asked('wc -l *.txt'))

  assert.ok(drawn.includes('The model wrote:'), drawn)
  assert.ok(drawn.includes('<code>wc -l *.txt</code>'), drawn)
  // Beside, never instead of: the plan is what the answer binds to, and the expansion the person
  // is being asked about is only in the plan.
  assert.ok(drawn.includes('<code>/usr/bin/wc -l notes.txt report.txt</code>'), drawn)
  assert.ok(drawn.indexOf('The model wrote:') < drawn.indexOf('Execution plan:'), drawn)
})

test('the line the planner wrote is searchable, as the card that shows it is', () => {
  // A cancelled request is drawn from this same text, so a line that is not in it is a command
  // the transcript shows while the turn was live and cannot account for afterwards.
  const found = t.searchableText(asked('wc -l *.txt'))

  assert.ok(found.includes('wc -l *.txt'), found)
})

test('a call that was never spelled as a line draws no row for one', () => {
  for (const line of ['', undefined]) {
    const drawn = draw(asked(line))
    assert.ok(!drawn.includes('The model wrote:'), drawn)
    assert.ok(drawn.includes('Execution plan:'), drawn)
  }
})
