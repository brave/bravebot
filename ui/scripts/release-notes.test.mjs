// CMD-25 in the window: Help › Release Notes shows the notes `changelog.read` returns, as plain text.
// The notice is rendered through `react-dom/server`, with `Modal` drawn in place of its portal.

import test from 'node:test'
import assert from 'node:assert/strict'
import { loadComponents } from './render-support.mjs'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

let bundle
const notice = () => (bundle ??= loadComponents('src/renderer/components/Notice.tsx'))

const draw = (Notice, shown) => renderToStaticMarkup(React.createElement(Notice, { ...shown, onClose() {} }))

test('the running version’s notes are shown under its version, with no subtitle', async () => {
  const { releaseNotesNotice } = await notice()
  assert.deepEqual(releaseNotesNotice({ version: '0.19.0', running: true, notes: '- fixed' }), {
    title: 'Release notes for 0.19.0', subtitle: undefined, body: '- fixed',
  })
})

test('a build with no notes of its own shows the newest release’s and says so', async () => {
  const { Notice, releaseNotesNotice } = await notice()
  const shown = releaseNotesNotice({ version: '0.18.2', running: false, notes: '- older' })
  assert.equal(shown.title, 'Release notes for 0.18.2')
  assert.equal(shown.subtitle, 'This build has no notes of its own. These are the newest release’s.')
  assert.ok(draw(Notice, shown).includes('data-subtitle="This build has no notes of its own. These are the newest release’s."'))
})

test('a build that carries no release says so', async () => {
  const { Notice, releaseNotesNotice } = await notice()
  const shown = releaseNotesNotice({ version: '0.19.0', running: false, notes: null })
  assert.deepEqual(shown, { title: 'Release notes', body: 'No release notes in this build.' })
  assert.ok(draw(Notice, shown).includes('<pre class="notice-body">No release notes in this build.</pre>'))
})

test('markup in the notes is drawn as the characters it is', async () => {
  const { Notice, releaseNotesNotice } = await notice()
  const notes = '- **bold** [link](https://example.com) <script>alert(1)</script> <a href="x">a</a> & more'
  const markup = draw(Notice, releaseNotesNotice({ version: '1.0.0', running: true, notes }))
  assert.ok(markup.includes('&lt;script&gt;alert(1)&lt;/script&gt; &lt;a href=&quot;x&quot;&gt;a&lt;/a&gt; &amp; more'), markup)
  assert.ok(markup.includes('**bold** [link](https://example.com)'), 'Markdown is not applied')
  assert.ok(!markup.includes('<script') && !markup.includes('<a ') && !markup.includes('<strong'))
})
