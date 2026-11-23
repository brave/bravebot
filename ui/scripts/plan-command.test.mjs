// `/plan` typed into the composer (CMD-1, MODE-12), as the desktop window reads the line.
//
// The faults these reject are a longer word or a mid-line mention taken for the command, and the
// typed line, rather than its task, being what a turn would be sent.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const shared = load("export * from './src/shared/plan'")

test('the command word alone gives an empty task', () => {
  assert.equal(shared.planTask('/plan'), '')
  assert.equal(shared.planTask('  /plan  '), '')
})

test('the command word with a task gives the task alone, trimmed', () => {
  assert.equal(shared.planTask('  /plan   fix the build '), 'fix the build')
  assert.equal(shared.planTask('/plan list the TODOs'), 'list the TODOs')
})

test('a longer word, a sentence that mentions the command, or another command is a prompt', () => {
  for (const prompt of ['/planned', '/planx', 'say /plan x', 'plan x', '/schedule x', '/Plan x', '']) {
    assert.equal(shared.planTask(prompt), null, prompt)
  }
})
