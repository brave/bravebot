// Picking a model's named option variant from the desktop window (BACKEND-66).
//
// The bridge holds the pick behind `session.variant`. The composer lists the names the model
// offers, plus Off, and picks through the same method. The window reaches it only if the main
// process forwards the method, so the allow-list has to name it.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { loadComponents } from './render-support.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

const COMPONENT = 'src/renderer/components/VariantPicker.tsx'

function allowedMethods(index) {
  const body = /const ALLOWED = new Set\(\[([^\]]*)\]\)/.exec(index)
  assert.ok(body, 'the allow-list is a literal set the test can read')
  return [...body[1].matchAll(/'([^']+)'/g)].map(each => each[1])
}

function menu(VariantMenu, read, refused = null) {
  return renderToStaticMarkup(React.createElement(VariantMenu, { read, refused, onPick() {} }))
}

test('the main process forwards session.variant from the window', () => {
  const allowed = allowedMethods(readFileSync('src/main/index.ts', 'utf8'))
  assert.ok(allowed.length > 40, 'the list was read whole')
  assert.ok(allowed.includes('session.variant'), 'session.variant is in ALLOWED')
})

test('a model that offers no variants shows nothing', async () => {
  const { VariantMenu } = await loadComponents(COMPONENT)
  assert.equal(menu(VariantMenu, { model: 'm', offered: [], variant: null }), '')
  // A pick kept from another model is not shown under one that offers none.
  assert.equal(menu(VariantMenu, { model: 'm', offered: [], variant: 'terse' }), '')
})

test('the menu has one item per offered name plus Off, and marks the pick', async () => {
  const { VariantMenu } = await loadComponents(COMPONENT)
  const items = (markup) => [...markup.matchAll(/<leo-menu-item[^>]*>/g)].map(([tag]) => ({
    test: /data-test="variant-option-([^"]+)"/.exec(tag)[1],
    checked: /aria-checked="true"/.test(tag),
  }))

  const picked = menu(VariantMenu, { model: 'm', offered: ['terse', 'wide'], variant: 'terse' })
  assert.deepEqual(items(picked), [
    { test: 'terse', checked: true },
    { test: 'wide', checked: false },
    { test: 'off', checked: false },
  ], picked)
  assert.ok(picked.includes('data-variant="terse"'), picked)
  assert.ok(!picked.includes('variant-refused'), 'no refusal, no alert')

  const off = menu(VariantMenu, { model: 'm', offered: ['terse', 'wide'], variant: null })
  assert.deepEqual(items(off).map(({ checked }) => checked), [false, false, true], off)
  assert.ok(off.includes('Default options'), off)
})

test('a refusal from the bridge is shown as an alert', async () => {
  const { VariantMenu } = await loadComponents(COMPONENT)
  const markup = menu(VariantMenu, { model: 'm', offered: ['terse'], variant: null }, 'm offers no variant named typo')
  assert.match(markup, /role="alert"[^>]*>m offers no variant named typo</)
})

test('the labels name the pick, or the default options', async () => {
  const { variantLabel, variantSentence } = await loadComponents(COMPONENT)
  assert.equal(variantLabel({ model: 'm', offered: ['terse'], variant: null }), 'Default options')
  assert.equal(variantLabel({ model: 'm', offered: ['terse'], variant: 'terse' }), 'terse')
  assert.equal(variantSentence({ model: 'm', offered: ['terse'], variant: null }), 'Options variant: off. m is sent its default options')
  assert.equal(variantSentence({ model: 'm', offered: ['terse'], variant: 'terse' }),
    "Options variant: terse. Its options are added to m's from the next turn")
})

test('the control reads and picks through session.variant for the composer model', () => {
  const source = readFileSync(COMPONENT, 'utf8')
  assert.match(source, /call<SessionVariant>\('session\.variant', \{ session, model \}\)/, 'read with no variant named')
  assert.match(source, /call<SessionVariant>\('session\.variant', \{ session, model, variant \}\)/, 'pick a name, or null for Off')
  assert.match(source, /\}, \[session, model, running\]\)/, 'read again when the session, the model or the running flag changes')
  const composer = readFileSync('src/renderer/components/Composer.tsx', 'utf8')
  assert.match(composer, /<VariantPicker [^>]*session=\{session\} model=\{model\} running=\{running\}/)
  const toolbar = composer.indexOf('<VariantPicker ')
  assert.ok(toolbar > 0 && composer.indexOf('<ModelPicker ', toolbar) - toolbar < 200, 'mounted beside the model picker')
})
