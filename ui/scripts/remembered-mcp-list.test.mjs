// How the Permissions dialog draws a project's remembered MCP answers, and its Forget.
//
// `permissions.list` sends `mcpRemembered`: whether every server the project requests is started
// without asking, and the tools called without asking, or a record that cannot be read; and
// `forgettable`, which the bridge sets false in an incognito session. `permissions.revoke` with
// `kind: "mcp"` adds `forgotten`. These pin that the window draws each of those as sent and
// decides nothing the bridge did not: Forget opens on `forgettable` and on something to forget.
//
// The component is rendered through `react-dom/server` and the markup is what is asserted on.

import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { loadComponents } from './render-support.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

// `react-dom/server` draws a Leo button as a bare `<leo-button>`, with none of the props that shut
// it, so here it is drawn as the plain button it stands for, with its tooltip.
const plainButton = {
  name: 'plain-button',
  setup(built) {
    built.onResolve({ filter: /^@brave\/leo\/react\/button$/ }, () => ({ path: 'button', namespace: 'plain-leo-button' }))
    built.onLoad({ filter: /.*/, namespace: 'plain-leo-button' }, () => ({
      loader: 'js',
      resolveDir: process.cwd(),
      contents: `import { createElement } from 'react'
        export default function Button({ isDisabled, onClick, children, ...rest }) {
          return createElement('button', { disabled: !!isDisabled, onClick, 'data-test': rest['data-test'], 'data-tooltip': rest['data-tooltip'] }, children)
        }`,
    }))
  },
}

const { RememberedMcpAnswers } = await loadComponents('src/renderer/components/Permissions.tsx', [plainButton])

const project = { project: '/work/app', shownProject: '/work/app' }
const kept = (overrides = {}) => ({
  ...project,
  everyServer: true,
  tools: [{ tool: 'weather:get_forecast', shownTool: 'weather:get_forecast' }, { tool: 'weather:alerts', shownTool: 'weather:alerts' }],
  forgettable: true,
  ...overrides,
})
const render = (remembered, { forgotten = null, problem = '', busy = false } = {}) =>
  renderToStaticMarkup(React.createElement(RememberedMcpAnswers, { remembered, forgotten, problem, busy, onForget() {} }))

/** The Forget button's host element, as drawn. */
const forget = (markup) => markup.match(/<button[^>]*data-test="mcp-forget"[^>]*>/)?.[0] ?? ''
const shut = (markup) => /\sdisabled=""/.test(forget(markup))

test('the remembered server answer and each tool are listed, by their shown spelling, under the project', () => {
  const markup = render(kept())
  assert.ok(markup.includes('Remembered MCP answers'), markup)
  assert.ok(markup.includes('data-test="mcp-every-server"') && markup.includes('Every server for this project'), markup)
  assert.ok(markup.includes('<code>weather:get_forecast</code>') && markup.includes('<code>weather:alerts</code>'), markup)
  assert.equal(markup.match(/data-test="mcp-tool"/g)?.length, 2)
  assert.ok(markup.includes('for /work/app'), 'the note names the project the answers are kept for')
  assert.ok(!markup.includes('data-test="mcp-empty"'), markup)
})

test('a tool is drawn as the bridge spelled it, so a character that draws as nothing shows', () => {
  const markup = render(kept({ everyServer: false, tools: [{ tool: 'w:a​', shownTool: '"w:a\\u{200b}"' }] }))
  assert.ok(markup.includes('&quot;w:a\\u{200b}&quot;'), markup)
  assert.ok(!markup.includes('Every server for this project'), 'no server answer is listed where none was given')
})

test('Forget is open where the bridge says it can forget and something is remembered', () => {
  const markup = render(kept())
  assert.ok(forget(markup), markup)
  assert.ok(!shut(markup), forget(markup))
  assert.ok(!forget(markup).includes('data-tooltip'), 'an open Forget needs no excuse')
})

test('nothing remembered is said, and Forget is shut', () => {
  const markup = render(kept({ everyServer: false, tools: [] }))
  assert.ok(markup.includes('No MCP answers are remembered for this project.'), markup)
  assert.ok(shut(markup), forget(markup))
})

test('an incognito session lists the answers and shuts Forget, saying why on its tooltip', () => {
  const markup = render(kept({ forgettable: false }))
  assert.ok(markup.includes('<code>weather:alerts</code>'), 'the answers are still listed')
  assert.ok(shut(markup), forget(markup))
  assert.ok(forget(markup).includes('data-tooltip="An incognito session writes nothing, so it can&#x27;t forget remembered MCP answers."'), forget(markup))
})

test('a record that cannot be read is named, is not listed as empty, and offers no Forget', () => {
  const markup = render({ ...project, unreadable: { path: '/h/.bravebot/mcp-tools', shownPath: '/h/.bravebot/mcp-tools', reason: 'it is a directory' }, forgettable: false })
  assert.ok(markup.includes('data-test="mcp-unreadable"'), markup)
  assert.ok(markup.includes("can&#x27;t be read") && markup.includes('/h/.bravebot/mcp-tools (it is a directory)'), markup)
  assert.ok(!markup.includes('No MCP answers are remembered'), 'an unreadable record is not drawn as nothing remembered')
  assert.equal(forget(markup), '', 'there is nothing Forget could do')
})

test('no state directory is said as such', () => {
  const markup = render(null)
  assert.ok(markup.includes('there is no state directory'), markup)
  assert.equal(forget(markup), '')
})

test('what was forgotten is listed, with what happens to running servers and the next call', () => {
  const markup = render(kept({ everyServer: false, tools: [] }), {
    forgotten: { everyServer: true, tools: [{ tool: 'weather:alerts', shownTool: 'weather:alerts' }, { tool: 'weather:get_forecast', shownTool: 'weather:get_forecast' }] },
  })
  const said = markup.match(/<leo-alert[^>]*data-test="mcp-forgotten"[^>]*>(.*?)<\/leo-alert>/s)?.[1] ?? ''
  assert.ok(said.includes('Forgotten:'), markup)
  assert.ok(said.includes('<li>Every server for this project</li>'), said)
  assert.ok(said.includes('<code>weather:alerts</code>') && said.includes('<code>weather:get_forecast</code>'), said)
  assert.ok(said.includes('already started keep running until it ends'), said)
  assert.ok(said.includes('The next call to a forgotten tool asks again.'), said)
  assert.ok(markup.includes('No MCP answers are remembered for this project.'), 'the list under it is the one read after forgetting')
})

test('forgetting where nothing was remembered says that nothing was forgotten', () => {
  const markup = render(kept({ everyServer: false, tools: [] }), { forgotten: { everyServer: false, tools: [] } })
  assert.ok(markup.includes('Nothing was remembered for this project, so nothing was forgotten.'), markup)
  assert.ok(!markup.includes('Forgotten:'), markup)
})

test('a refusal is said in the bridge’s words, beside the button', () => {
  const words = 'Nothing was forgotten: /h/.bravebot/mcp-tools cannot be read (it is a directory).'
  const markup = render(kept(), { problem: words })
  assert.ok(markup.includes('data-test="mcp-forget-error"') && markup.includes(words), markup)
})

test('Forget is shut while a request is out', () => {
  assert.ok(shut(render(kept(), { busy: true })))
})
