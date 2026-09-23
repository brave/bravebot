// What the run card says about access the agent reaches and does not hold.
//
// This window is the third surface that grants a command the access the person already has
// elsewhere, and the third that owes
// [CRED-5](../../docs/specs/credential-protection.md#CRED-5) an answer: a container daemon, a
// tool already logged in, the ssh agent and a machine's metadata service hand nothing over, can
// refuse nobody, and cannot be taken back, so the one thing available is that whoever approves
// the command is told what they are approving.
//
// The agent sends the kind and the word that named it and no sentence at all, because the words
// a person reads belong to the surface drawing them. So what has to be checked here is the
// drawing: that the card says what the access is, and that a command reaching none of it draws
// nothing.

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

/** The run question as the bridge sends it, with whatever the line was found to reach. */
const asked = (ambient) =>
  t.askedRun({
    request: 1,
    plan: 'docker ps',
    stages: [{ program: 'docker', resolved: '/usr/bin/docker', args: ['ps'], display: 'docker ps' }],
    directory: '/home/someone/project',
    releasesPrivate: false,
    ambient,
    vouches: [{ program: '/usr/bin/docker', args: ['ps'], display: 'docker ps' }],
    summary: 'run 1 step in /home/someone/project',
  })

test('a command that reaches ambient authority says which, and what it costs', () => {
  const drawn = draw(asked([{ authority: 'container-daemon', named: 'docker' }]))

  assert.ok(drawn.includes('spends access that is yours elsewhere'), drawn)
  assert.ok(drawn.includes('runs anything as root on this machine'), drawn)
  // The word that named it, so the sentence is attached to something in the command rather than
  // floating above it.
  assert.ok(drawn.includes('<code>docker</code>'), drawn)
})

test('a command that reaches none says nothing of the sort', () => {
  for (const ambient of [[], undefined]) {
    const drawn = draw(asked(ambient))
    assert.ok(!drawn.includes('spends access that is yours elsewhere'), drawn)
  }
})

test('an authority this window has no sentence for is still drawn as one', () => {
  // The kinds are the agent's and this window may be older than it. A row with nothing beside it
  // would be a grant with nothing said about it, which is the outcome the question exists to
  // avoid, so an unknown kind says what every one of them has in common.
  const drawn = draw(asked([{ authority: 'something-later', named: 'whatever' }]))

  assert.ok(drawn.includes('spends access that is yours elsewhere'), drawn)
  assert.ok(drawn.includes('<code>whatever</code>'), drawn)
  assert.ok(drawn.includes('nobody is asked for and nothing here takes back'), drawn)
})
