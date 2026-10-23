// What the window draws for a name that would draw as nothing, or as another name.
//
// [PROMPT-1](../../docs/specs/prompting.md#PROMPT-1) says a person cannot endorse a name they were
// not shown. The agent spells such a name once, in `shownPath`, and every card and list that names
// a path draws that field. A card that drew `path` would show `note.md ` as `note.md`.
//
// The cards are rendered through `react-dom/server` and the markup is what is asserted on.

import test from 'node:test'
import assert from 'node:assert/strict'
import { build, buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

const options = (path) => ({
  entryPoints: [path],
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  external: ['react', 'react-dom', 'react/jsx-runtime'],
})

function evaluate(source) {
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const load = (path) => evaluate(buildSync(options(path)).outputFiles[0].text)

const { Row, SessionNotices, RulesBanner } = load('src/renderer/components/Transcript.tsx')
// The modal portals into `document.body`, which a server render has none of, so the question is
// drawn inside a plain element: what is asserted is the question's own text.
const withoutModal = {
  name: 'without-modal',
  setup(build) {
    build.onResolve({ filter: /\/Modal$/ }, () => ({ path: 'modal', namespace: 'stub' }))
    build.onLoad({ filter: /.*/, namespace: 'stub' }, () => ({
      contents: 'import React from "react"; export const Modal = ({ children }) => React.createElement("div", null, children)',
      loader: 'js',
      resolveDir: process.cwd(),
    }))
  },
}
const { TrustPrompt } = evaluate((await build({ ...options('src/renderer/components/TrustPrompt.tsx'), plugins: [withoutModal] })).outputFiles[0].text)
const { PathPermissions, RememberedHere } = load('src/renderer/components/Permissions.tsx')
const { Context } = load('src/renderer/components/Context.tsx')
const t = load('src/renderer/transcript.ts')

const draw = (entry) =>
  renderToStaticMarkup(
    React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false }),
  )

// The name as the planner wrote it, and the form the agent sends for a person to read.
const WRITTEN = 'note.md '
const SPELLED = '"note.md␣"'

// The same name as markup holds it, which writes a quotation mark in text or an attribute as an entity.
const SPELLED_MARKUP = '&quot;note.md␣&quot;'
const BLANK_MARKUP = '&quot;␣&quot;'
const ROOT_MARKUP = '&quot;&quot;'

const write = (overrides = {}) =>
  t.asked({
    request: 1, path: 'note.md', shownPath: 'note.md', intent: 'create', untrusted: false, existing: false,
    added: 1, removed: 0, exact: true, changes: [{ kind: 'added', text: 'a line' }], ...overrides,
  })

const exposure = (overrides = {}) =>
  t.askedExposure({
    request: 2, path: 'note.md', shownPath: 'note.md', credentials: ['an API key at line 1'],
    summary: 'let the model read note.md, which holds 1 credential', ...overrides,
  })

const vouch = (overrides = {}) =>
  t.askedVouch({
    request: 3, path: 'note.md', shownPath: 'note.md', rule: '/work/note.md', shownRule: '/work/note.md',
    preview: 'a line', truncated: false, ...overrides,
  })

const run = (overrides = {}) =>
  t.askedRun({
    request: 4, stages: [{ program: 'tee', resolved: '/usr/bin/tee', shownBinary: '/usr/bin/tee', args: [], display: 'tee' }],
    directory: '/work', shownDirectory: '/work', releasesPrivate: false, canBeRemembered: false, line: '', plan: 'tee',
    writes: ['note.md'], shownWrites: ['note.md'], ambient: [], vouches: [], summary: 'write it',
    confinement: { heading: 'confined to:', directories: ['/work'], shownDirectories: ['/work'], sentences: [] },
    ...overrides,
  })

const spelled = { path: WRITTEN, shownPath: SPELLED }

test('a write card draws the name as spelled, and does not draw the name as written', () => {
  const markup = draw(write(spelled))
  assert.ok(markup.includes(`<code class="path" data-tooltip="${SPELLED_MARKUP}">${SPELLED_MARKUP}</code>`), markup)
  assert.ok(!markup.includes(`>${WRITTEN}<`), markup)
})

test('a write card that has been answered still folds to the name as spelled', () => {
  const markup = draw({ ...write(spelled), decision: 'approve' })
  assert.ok(markup.includes(SPELLED_MARKUP), markup)
  assert.ok(!markup.includes(`${WRITTEN}</code>`), markup)
})

test('a credential-read card draws the name as spelled', () => {
  const markup = draw(exposure(spelled))
  assert.ok(markup.includes(`<code class="path">${SPELLED_MARKUP}</code>`), markup)
  assert.ok(!markup.includes(`>${WRITTEN}<`), markup)
})

test('a vouch card that has been answered still folds to the name as spelled', () => {
  const markup = draw({ ...vouch(spelled), decision: 'approve' })
  assert.ok(markup.includes(SPELLED_MARKUP), markup)
  assert.ok(!markup.includes(`${WRITTEN}</code>`), markup)
})

test('a run card draws each file the line writes and each directory it is confined to as spelled', () => {
  const markup = draw(run({
    writes: [WRITTEN],
    shownWrites: [SPELLED],
    confinement: { heading: 'confined to:', directories: ['/work '], shownDirectories: ['"/work␣"'], sentences: [] },
  }))
  assert.ok(markup.includes(`<code>${SPELLED_MARKUP}</code>`), markup)
  assert.ok(markup.includes('<code>&quot;/work␣&quot;</code>'), markup)
  assert.ok(!markup.includes(`<code>${WRITTEN}</code>`), markup)
  assert.ok(!markup.includes('<code>/work </code>'), markup)
})

test('a run card draws the directory it runs in and the file each step runs as spelled', () => {
  const markup = draw(run({
    directory: '/work ',
    shownDirectory: '"/work␣"',
    stages: [
      { program: 'tee', resolved: '/usr/bin/tee ', startedAs: '/usr/bin/tee ', shownBinary: '"/usr/bin/tee␣"', args: [], display: 'tee' },
      { program: 'cat', resolved: '/usr/bin/cat', startedAs: '/usr/local/bin/cat', shownBinary: '/usr/local/bin/cat -> /usr/bin/cat', args: [], display: 'cat' },
    ],
  }))
  assert.ok(markup.includes('<code class="path" data-tooltip="&quot;/work␣&quot;">&quot;/work␣&quot;</code>'), markup)
  assert.ok(markup.includes('<span class="resolved">&quot;/usr/bin/tee␣&quot;</span>'), markup)
  assert.ok(markup.includes('<span class="resolved">/usr/local/bin/cat -&gt; /usr/bin/cat</span>'), markup)
  assert.ok(!markup.includes('/work </code>'), markup)
  assert.ok(!markup.includes('/usr/bin/tee </span>'), markup)
})

test('the permissions list draws each rule as spelled, and the root as the project root', () => {
  const markup = renderToStaticMarkup(React.createElement(PathPermissions, {
    paths: [
      { path: '', shownPath: '""', integrity: 'trusted' },
      { path: WRITTEN, shownPath: SPELLED, integrity: 'trusted' },
      { path: ' ', shownPath: '"␣"', integrity: 'untrusted' },
    ],
    busy: false,
    onRevoke() {},
  }))
  assert.ok(markup.includes(`<code>${SPELLED_MARKUP}</code>`), markup)
  assert.ok(markup.includes(`<code>${BLANK_MARKUP}</code>`), markup)
  assert.ok(markup.includes('<code>Project root</code>'), markup)
  assert.ok(!markup.includes(`<code>${ROOT_MARKUP}</code>`), markup)
  assert.ok(!markup.includes(`<code>${WRITTEN}</code>`), markup)
  assert.ok(!markup.includes('<code> </code>'), markup)
})

test('the startup question draws the directory and the file a yes is kept in as spelled', () => {
  const markup = renderToStaticMarkup(React.createElement(TrustPrompt, {
    directory: '"/work␣"',
    keeping: '"/home/me/state␣/trusted"',
    confinedLines: null,
    onAnswer() {},
  }))
  assert.ok(markup.includes('<code class="path">&quot;/work␣&quot;</code>'), markup)
  assert.ok(markup.includes('<code>&quot;/home/me/state␣/trusted&quot;</code>'), markup)
})

test('the notice that a yes was kept names the file as spelled', () => {
  const markup = renderToStaticMarkup(React.createElement(SessionNotices, {
    forkedFrom: null,
    kept: { at: Math.floor(Date.now() / 1000), path: '/home/me/state /trusted', shownPath: '"/home/me/state␣/trusted"' },
    vetting: false,
    rules: null,
    onOpenParent() {},
    onManage() {},
  }))
  assert.ok(markup.includes('kept in &quot;/home/me/state␣/trusted&quot;'), markup)
  assert.ok(!markup.includes('kept in /home/me/state /trusted'), markup)
})

test('the list of what is kept draws the file and the root as spelled', () => {
  const markup = renderToStaticMarkup(React.createElement(RememberedHere, {
    kept: {
      at: Math.floor(Date.now() / 1000),
      path: '/home/me/state /trusted',
      shownPath: '"/home/me/state␣/trusted"',
      root: '/work/repo ',
      shownRoot: '"/work/repo␣"',
    },
    busy: false,
    onForget() {},
  }))
  assert.ok(markup.includes('<code>&quot;/home/me/state␣/trusted&quot;</code>'), markup)
  assert.ok(markup.includes('for &quot;/work/repo␣&quot;'), markup)
  assert.ok(!markup.includes('/work/repo </span>'), markup)
  assert.ok(!markup.includes('<code>/home/me/state /trusted</code>'), markup)
})

test('the list of rules a checkout proposed draws each rule and its file as spelled', () => {
  const markup = renderToStaticMarkup(React.createElement(RulesBanner, {
    rules: {
      deny: [], ask: [], allow: [], unreadable: [], directories: [], filesystem: [], filesystemIgnored: [],
      proposed: [{
        rule: 'WebFetch(domain:exa\u200bmple.com)',
        shownRule: '"WebFetch(domain:exa\\u{200b}mple.com)"',
        file: '/work/proj /.bravebot/settings.json',
        shownFile: '"/work/proj␣/.bravebot/settings.json"',
      }],
    },
  }))
  assert.ok(markup.includes('<code>&quot;WebFetch(domain:exa\\u{200b}mple.com)&quot;</code>'), markup)
  assert.ok(markup.includes('<code>&quot;/work/proj␣/.bravebot/settings.json&quot;</code>'), markup)
  assert.ok(!markup.includes('exa\u200bmple'), markup)
  assert.ok(!markup.includes('/work/proj /.bravebot'), markup)
})

// A call's own `target` is what a window matches the call to its confirmation on, so it stays as the
// model wrote it. What the transcript line and the Context lists draw is `shownTarget`.
const call = (verb, target, shownTarget, note = 'done', shownFile = shownTarget) =>
  ({ verb, target, shownTarget, shownFile, why: '', note, failed: false, untrusted: false, changes: [], waitedSeconds: null })

test('a tool line draws the name of its file as spelled, and does not draw it as written', () => {
  const markup = draw(t.started(call('Write', WRITTEN, SPELLED, null)))
  assert.ok(markup.includes(`>${SPELLED_MARKUP}</span>`), markup)
  assert.ok(!markup.includes(`>${WRITTEN}<`), markup)
})

test('the Context lists draw the files read and written as spelled', () => {
  const live = {
    handle: 'h', summary: { directory: '/work' }, todos: [], quarantine: [], phase: null, tokens: 0, running: false,
    entries: [
      { kind: 'tool', id: 'read', activity: call('Read', 'old.md ', '"old.md␣"'), landing: null },
      { kind: 'tool', id: 'wrote', activity: call('Write', WRITTEN, SPELLED), landing: null },
      { kind: 'tool', id: 'blank', activity: call('Write', ' ', '"␣"'), landing: null },
    ],
  }
  const markup = renderToStaticMarkup(React.createElement(Context, { live }))
  for (const name of [SPELLED_MARKUP, '&quot;old.md␣&quot;', BLANK_MARKUP]) {
    assert.ok(markup.includes(`<span class="file-path">${name}</span>`), `${name}: ${markup}`)
  }
  for (const bare of ['>note.md <', '>old.md <', '> <']) {
    assert.ok(!markup.includes(bare), `${bare}: ${markup}`)
  }
})

test('a write that was asked about is listed once, under the name as spelled', () => {
  const { written } = load('src/renderer/components/Context.tsx')
  const asked = { kind: 'confirm', id: 'asked', request: { path: WRITTEN, shownPath: SPELLED }, decision: 'approve' }
  const rows = written([{ kind: 'tool', id: 'wrote', activity: call('Write', WRITTEN, SPELLED), landing: null }, asked])
  assert.deepEqual(rows, [{ target: WRITTEN, shown: SPELLED, state: 'applied' }])
  assert.deepEqual(written([asked]), [{ target: WRITTEN, shown: SPELLED, state: 'approved' }])
})

test('a write named through a reference or a leading ./ is listed under the name the bridge sent for the file', () => {
  const { written } = load('src/renderer/components/Context.tsx')
  const rows = written([
    { kind: 'tool', id: 'a', activity: call('Write', 'ref:1(U,priv):note.md ', '"ref:1(U,priv):note.md␣"', 'done', '"note.md␣"'), landing: null },
    { kind: 'tool', id: 'b', activity: call('Write', './other.md ', '"./other.md␣"', 'done', '"other.md␣"'), landing: null },
    { kind: 'tool', id: 'c', activity: call('Write', 'ref:2(U,priv):plain.md', 'ref:2(U,priv):plain.md', 'done', 'plain.md'), landing: null },
  ])
  assert.deepEqual(rows.map((row) => row.shown), ['"note.md␣"', '"other.md␣"', 'plain.md'])
})

test('an exported or searched tool line holds the name as spelled', () => {
  const entry = t.started(call('Write', WRITTEN, SPELLED, null))
  assert.deepEqual(t.conversation([entry], true), [{ role: 'tool', verb: 'Write', target: SPELLED, note: null, failed: false }])
  assert.ok(t.searchableText(entry).includes('note.md␣'), t.searchableText(entry))
  assert.ok(!t.searchableText(entry).includes('note.md '), t.searchableText(entry))
})
