// SESSION-36 in the window: the Notes panel lists the notes on a chat's record, and the note dialog
// adds one. Both are rendered through `react-dom/server`. `Modal` draws through a portal, which that
// cannot render, so here it is drawn in place, and the file tree, which reads the disk, is left out.

import test from 'node:test'
import assert from 'node:assert/strict'
import { inPlaceModal, loadComponents } from './render-support.mjs'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

// `react-dom/server` draws a Leo button as a bare `<leo-button>`, with none of the props that shut
// it, so here it is drawn as the plain button it stands for.
const inPlace = {
  name: 'in-place',
  setup(built) {
    built.onResolve({ filter: /^@brave\/leo\/react\/button$/ }, () => ({ path: 'button', namespace: 'plain-leo-button' }))
    built.onLoad({ filter: /.*/, namespace: 'plain-leo-button' }, () => ({
      loader: 'js',
      resolveDir: process.cwd(),
      contents: `import { createElement } from 'react'
        export default function Button({ className, isDisabled, onClick, children, ...rest }) {
          return createElement('button', { className, disabled: !!isDisabled, onClick, 'data-test': rest['data-test'] }, children)
        }`,
    }))
    built.onResolve({ filter: /^\.\/FileTree$/ }, () => ({ path: 'tree', namespace: 'no-tree' }))
    built.onLoad({ filter: /.*/, namespace: 'no-tree' }, () => ({ loader: 'js', contents: 'export const FileTree = () => null' }))
  },
}

let bundle
const components = () => (bundle ??= loadComponents({
  contents: "export { Context } from './src/renderer/components/Context.tsx'\nexport { NoteDialog } from './src/renderer/components/NoteDialog.tsx'",
}, [inPlace, inPlaceModal]))

const live = (overrides = {}) => ({
  handle: 's-1', summary: { directory: '/project' }, entries: [], todos: [], quarantine: [], phase: null, tokens: 0, running: false,
  ...overrides,
})

/** The Notes panel's own markup, cut out of the column. */
function notesPanel(markup) {
  const start = markup.indexOf('id="panel-notes"')
  assert.ok(start > 0, 'the column has a Notes panel')
  return markup.slice(start, markup.indexOf('</section>', start))
}

/** The opening tag of the element carrying `data-test="<name>"`. */
function tagged(markup, name) {
  const found = new RegExp(`<[^>]*data-test="${name}"[^>]*>`).exec(markup)
  assert.ok(found, `nothing is marked ${name}`)
  return found[0]
}

const disabled = (tag) => /\sdisabled=""/.test(tag)

test('the panel draws each note as the text it is, with its turn, and counts them', async () => {
  const { Context } = await components()
  const notes = [
    { turn: 1, text: '<b>bold</b> & **not markdown** <img src=x onerror=alert(1)>' },
    { turn: 3, text: 'second' },
  ]
  const panel = notesPanel(renderToStaticMarkup(React.createElement(Context, { live: live({ notes }), onAddNote() {} })))
  assert.ok(panel.includes('&lt;b&gt;bold&lt;/b&gt; &amp; **not markdown** &lt;img src=x onerror=alert(1)&gt;'), panel)
  assert.ok(!panel.includes('<b>') && !panel.includes('<img') && !panel.includes('<strong>'), 'no note markup is drawn as markup')
  assert.match(panel, />2<\/leo-label>|class="count num"[^>]*>2</, 'the count is the number of notes')
  assert.ok(panel.includes(', 2 items'))
  assert.ok(panel.includes('Turn 1') && panel.includes('Turn 3'))
  assert.equal(panel.match(/<li\b/g).length, 2)
})

test('a chat with no notes says so and counts none', async () => {
  const { Context } = await components()
  const panel = notesPanel(renderToStaticMarkup(React.createElement(Context, { live: live(), onAddNote() {} })))
  assert.ok(panel.includes('No notes.'))
  assert.ok(panel.includes(', 0 items'))
})

test('Add note is offered between turns and disabled while one runs', async () => {
  const { Context } = await components()
  const idle = renderToStaticMarkup(React.createElement(Context, { live: live(), onAddNote() {} }))
  const busy = renderToStaticMarkup(React.createElement(Context, { live: live({ running: true }), onAddNote() {} }))
  assert.equal(disabled(tagged(idle, 'notes-add')), false, tagged(idle, 'notes-add'))
  assert.equal(disabled(tagged(busy, 'notes-add')), true, tagged(busy, 'notes-add'))
  const none = renderToStaticMarkup(React.createElement(Context, { live: live() }))
  assert.ok(!none.includes('data-test="notes-add"'), 'and absent where nothing can add one')
})

test('the note dialog says where the note goes, and cannot add while a turn runs or with nothing typed', async () => {
  const { NoteDialog } = await components()
  const draw = (running) => renderToStaticMarkup(React.createElement(NoteDialog, { running, onCancel() {}, onAdd: async () => true }))
  const idle = draw(false)
  assert.ok(idle.includes('data-subtitle="Kept on this chat record. Never sent to the model."'))
  assert.equal(disabled(tagged(idle, 'note-add')), true, 'nothing typed yet')
  assert.equal(disabled(tagged(idle, 'note-cancel')), false)
  assert.ok(idle.indexOf('data-test="note-cancel"') < idle.indexOf('data-test="note-add"'), 'Cancel just before Add')
  assert.ok(!tagged(idle, 'note-cancel').includes('modal-leading'), 'both at the right-hand end, as Nala’s dialog lays them out')
  assert.equal(disabled(tagged(draw(true), 'note-add')), true)
})
