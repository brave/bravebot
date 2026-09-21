import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'

// Bundle the browser module for Node so its TypeScript and Three imports use the same code.
const require = createRequire(import.meta.url)
const source = buildSync({ entryPoints: ['src/renderer/avatar/figure.ts'], bundle: true, write: false, platform: 'node', format: 'cjs', packages: 'external' }).outputFiles[0].text
const module = { exports: {} }
new Function('require', 'module', 'exports', source)(require, module, module.exports)
const { traitsOf, signature, buildFigureFromTraits } = module.exports

test('legacy seeds retain their identity and opt out of new geometry', () => {
  assert.equal(signature('review-3'), 'red-round-bobble-ears-big-pill-collar')
  assert.equal(signature('review-61'), 'blue-squat-antenna-ears-big-pill-collar')
  for (const seed of ['review-3', 'review-61', 'existing-bot', 'a492-fc02']) {
    const t = traitsOf(seed)
    assert.equal(t.version, 1)
    assert.equal(t.eyeShape, 'round')
    assert.equal(t.faceShape, 'bare')
    assert.equal(t.proportion, 'balanced')
    assert.equal(t.earShape, 'disc')
    assert.equal(t.torso, 'classic')
  }
})

test('versioned seeds reproduce every trait and cover the full new vocabulary', () => {
  const sets = Object.fromEntries(['eyeShape', 'faceShape', 'earShape', 'proportion', 'torso'].map(key => [key, new Set()]))
  for (let i = 0; i < 512; i++) {
    const seed = `v2:bot-${i}`
    const t = traitsOf(seed)
    assert.deepEqual(traitsOf(seed), t)
    assert.equal(t.version, 2)
    assert.ok(signature(seed).endsWith('-v2'))
    for (const key in sets) sets[key].add(t[key])
  }
  assert.deepEqual(Object.values(sets).map(set => set.size), [4, 3, 3, 4, 3])
})

test('head, proportion, panel, and eye combinations build finite geometry with attached eyes', () => {
  let index = 0
  for (const head of ['round', 'squat', 'tall', 'boxy'])
    for (const proportion of ['balanced', 'forehead', 'tapered', 'compact'])
      for (const faceShape of ['bare', 'oval', 'panel'])
        for (const eyeShape of ['round', 'oval', 'square', 'pill']) {
          const t = { ...traitsOf('v2:geometry'), head, proportion, faceShape, eyeShape, ears: true,
            earShape: ['disc', 'fin', 'pod'][index % 3], torso: ['compact', 'broad', 'neck'][index++ % 3] }
          const figure = buildFigureFromTraits(t, 22)
          try {
            const eyes = figure.eyes.children
            assert.equal(eyes.length, 2)
            assert.ok(eyes.every(eye => eye.position.z > 0.5 && eye.position.z < 1.2))
            assert.equal(Boolean(figure.head.getObjectByName('face-panel')), faceShape !== 'bare')
            figure.root.traverse(part => {
              const positions = part.geometry?.getAttribute('position')
              if (positions) assert.ok(positions.array.every(Number.isFinite))
            })
          } finally { figure.dispose() }
        }
  assert.equal(index, 192)
})
