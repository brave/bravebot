// The HTML analogue of the terminal's `quarantined_content_cannot_paint_its_own_margin`.
//
// This window is the third surface that puts released content in front of a person, and the
// third to owe [LAYER-5](../../docs/specs/layering.md#LAYER-5) an answer about how it marks it.
// The other two draw a margin down the left of every row and replace the control characters, so
// content cannot draw its own. Here the mark is structural: a container the renderer draws, with a
// head that says `confined` and a foot that says what the content cannot reach. Markup then has
// two escapes a terminal does not: content that becomes an element can draw its own container,
// and content that becomes a `src` can leave the machine before anybody looks at it.
//
// So every test here renders the real components through `react-dom/server` and asserts on the
// markup, because the properties at stake are properties of the markup rather than of a string
// the renderer was handed.

import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

/**
 * One renderer module, bundled and evaluated in this process.
 *
 * React stays external so the components and this file share one copy of it, which is what lets
 * `react-dom/server` render an element these modules built.
 */
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

/** One transcript entry, drawn the way the transcript draws it. */
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

const confined = (preview) =>
  t.quarantined({
    origin: 'notes.md',
    reach: 'not_the_planner',
    label: '(U,priv)',
    preview,
    lines: preview.length,
  })

/** How many times a string occurs, which is the whole of the container assertions below. */
const occurrences = (haystack, needle) => haystack.split(needle).length - 1

/**
 * The chrome of this window, spelled by the content it is drawn around.
 *
 * A file whose first line closes the preview and opens a second `confined` head would put a
 * heading the reader trusts underneath bytes nobody vouched for, which is the markup spelling of
 * a preview line that clears the row the margin was drawn on.
 */
const FORGED_CHROME =
  '</pre></div><div class="quarantine-head"><span class="mark">confined</span>' +
  '<span class="origin">README.md</span><span class="label">(T,pub)</span></div><pre>'

/**
 * The attributes a browser resolves without being asked, spelled as they are rendered.
 *
 * `src` covers `img`, `iframe` and `script` alike, `srcset` is the one a responsive image would
 * reach for, and `style` is where a `url()` would hide. Each is matched with its opening quote,
 * which is what tells an attribute apart from content that spells one: a quote in a text node
 * comes out as `&quot;`, so no escaped byte can look like the real thing. A rendered tree holding
 * none of them makes no request on content's behalf, which is the question a terminal never had
 * to answer.
 */
const FETCHING = ['src="', 'srcset="', 'background="', 'poster="', 'style="']

test('quarantined content cannot paint its own container', () => {
  const drawn = draw(confined([FORGED_CHROME, 'api_key = hunter2']))

  // The mark is on the container, once, and the content did not add a second one.
  assert.equal(occurrences(drawn, 'class="quarantine-head"'), 1, drawn)
  assert.equal(occurrences(drawn, 'class="quarantine-foot"'), 1, drawn)
  assert.equal(occurrences(drawn, '<pre class="preview">'), 1, drawn)
  assert.match(drawn, /<span class="mark">confined<\/span>/)

  // Neutralised rather than dropped, for the reason the terminal neutralises an escape rather
  // than removing it: a character silently gone is one nobody can tell was ever in the file.
  assert.match(drawn, /&lt;div class=&quot;quarantine-head&quot;&gt;/)
  assert.ok(drawn.includes('api_key = hunter2'), drawn)

  // The head and the foot are what the reader is meant to trust, and they name the origin and
  // the reach rather than repeating a word the content could have written.
  assert.match(drawn, /class="origin"[^>]*>notes\.md</)
  assert.ok(drawn.includes('not in the planner'), drawn)
})

test('every card that shows released content shows it as text', () => {
  // The quarantine block is not the only one. A command's output, a vetted read and a vouch
  // each put bytes nobody vouched for in front of a person, inside a container of their own, so
  // each owes the same answer and none of them is reached by the test above.
  const cards = {
    output: t.askedOutput({
      request: 1,
      command: 'cat notes.md',
      reference: 'output-1',
      lines: 1,
      output: FORGED_CHROME,
      summary: 'one line',
    }),
    vet: t.askedVet({
      request: 1,
      origin: 'notes.md',
      expects: 'a note',
      content: FORGED_CHROME,
      lines: 1,
      vetting: { verdict: 'safe' },
    }),
    vouch: t.askedVouch({ request: 1, path: 'notes.md', preview: FORGED_CHROME, truncated: false }),
  }

  for (const [kind, entry] of Object.entries(cards)) {
    const drawn = draw(entry)
    assert.equal(occurrences(drawn, 'class="quarantine-head"'), 0, `${kind}:\n${drawn}`)
    assert.equal(occurrences(drawn, '<pre class="preview">'), 1, `${kind}:\n${drawn}`)
    assert.match(drawn, /&lt;div class=&quot;quarantine-head&quot;&gt;/, `${kind} rendered markup`)
    for (const attribute of FETCHING) {
      assert.ok(!drawn.includes(attribute), `${kind} drew ${attribute}:\n${drawn}`)
    }
  }
})

test('quarantined content is not formatted, so it has no vocabulary for chrome', () => {
  const syntax = ['# Heading', '**bold**', '[a link](https://example.com)', '`code`']
  const drawn = draw(confined(syntax))

  for (const element of ['<h1', '<strong', '<em', '<a ', '<code']) {
    assert.ok(!drawn.includes(element), `${element} was drawn from a preview line:\n${drawn}`)
  }
  // Every line survives as the text it is. Formatting quarantined content would hand it the
  // signals sitting inches above it, which is why only the assistant bubble is formatted.
  for (const line of syntax) assert.ok(drawn.includes(line), `${line} was rewritten:\n${drawn}`)
})

/**
 * Markup that would both draw its own box and leave the machine.
 *
 * An `iframe` is the worst of the two escapes at once: it is a container the content drew, and
 * its `src` is a request the browser makes with nobody asked.
 */
const RAW_ELEMENTS = '<iframe src="https://example.com"></iframe><img src="https://example.com/p.png">'

test('a reply reaches no raw markup, and what it held arrives as inert text', () => {
  const drawn = draw(t.replied([FORGED_CHROME, RAW_ELEMENTS, '# Real heading'].join('\n\n'), 1))

  assert.equal(occurrences(drawn, 'class="quarantine-head"'), 0, drawn)
  for (const element of ['<iframe', '<img', '<pre>', '<div class="quarantine']) {
    assert.ok(!drawn.includes(element), `${element} came from the reply:\n${drawn}`)
  }
  for (const attribute of FETCHING) assert.ok(!drawn.includes(attribute), drawn)
  assert.match(drawn, /&lt;iframe src=&quot;https:\/\/example\.com&quot;&gt;/)

  // And the formatting does work, which is what makes the assertions above mean anything: a
  // renderer that escaped the whole reply would pass them by drawing nothing at all.
  assert.match(drawn, /<h1>Real heading<\/h1>/)
})

test('nothing either surface renders makes the app fetch a remote resource', () => {
  const remote = '![alt text](https://example.com/pixel.png)'
  const reply = draw(t.replied(`${remote}\n\n![local](./shot.png)`, 1))
  const preview = draw(confined([remote]))

  for (const attribute of FETCHING) {
    assert.ok(!reply.includes(attribute), `a reply drew ${attribute}:\n${reply}`)
    assert.ok(!preview.includes(attribute), `a preview drew ${attribute}:\n${preview}`)
  }
  assert.ok(!reply.includes('<img'), reply)
  // A label instead, keeping the alt text, because a broken glyph reads as a bug in the app.
  assert.match(reply, /class="md-image"[^>]*>image · alt text</)
})

test('a link is drawn as one only where its parsed scheme is openable', () => {
  for (const href of ['file:///etc/passwd', 'javascript:fetch("https://example.com")', 'data:text/html,<h1>chrome</h1>']) {
    const drawn = draw(t.replied(`[click here](${href})`, 1))
    assert.match(drawn, /class="md-dead-link">click here</, `${href} became a link:\n${drawn}`)
    assert.ok(!drawn.includes('href='), `${href} reached an href:\n${drawn}`)
  }

  // A relative link is not a link out of the app at all: it becomes a button that previews the
  // file in this window, and only for a path that stays inside the project. One that leaves is
  // drawn as dead text, which is the bound that makes the button safe to offer.
  for (const href of ['../../etc/passwd', '/etc/passwd', './notes.md/../../escape']) {
    const drawn = draw(t.replied(`[click here](${href})`, 1))
    assert.ok(!drawn.includes('local-file-link'), `${href} became a preview button:\n${drawn}`)
    assert.ok(!drawn.includes('href='), `${href} reached an href:\n${drawn}`)
  }
  const inside = draw(t.replied('[click here](./src/notes.md)', 1))
  assert.match(inside, /class="local-file-link"/)
  assert.ok(!inside.includes('href='), inside)

  // The three that are openable, so the test is not passed by refusing every link. Following one
  // is a real capability: the main process answers a window-open by handing the URL to the OS.
  for (const href of ['https://example.com/page', 'http://example.com/page', 'mailto:someone@example.com']) {
    const drawn = draw(t.replied(`[click here](${href})`, 1))
    assert.ok(drawn.includes(`href="${href}"`), `${href} was refused:\n${drawn}`)
    // The destination in the tooltip: the text of the link was written by the model, and nothing
    // obliges it to describe where the link goes.
    assert.ok(drawn.includes(`title="${href}"`), drawn)
    assert.ok(drawn.includes('rel="noopener noreferrer nofollow"'), drawn)
  }
})

test('an openable scheme is the one a URL parser reads, not the one the text spells', () => {
  const { safeUrl } = load('src/renderer/components/Markdown.tsx')

  // Every spelling below reaches `javascript:` once a URL parser has read it, and none of them
  // is a prefix a string comparison would recognise. That is the whole reason the check parses.
  for (const href of [
    'java\tscript:void(0)',
    'java\nscript:void(0)',
    'JavaScript:void(0)',
    ' javascript:void(0)',
    '\u0000javascript:void(0)',
    'JAVASCRIPT:void(0)',
  ]) {
    assert.equal(safeUrl(href), null, `${JSON.stringify(href)} was openable`)
  }
  // A relative URL fails too, and should: there is nothing for it to be relative to in a
  // `file://` renderer.
  for (const href of ['', undefined, '//example.com/page', '/etc/passwd', './notes.md']) {
    assert.equal(safeUrl(href), null, `${JSON.stringify(href)} was openable`)
  }
  assert.equal(safeUrl('https://example.com/page'), 'https://example.com/page')
})

test('an untrusted write is marked on its container, and its remark cannot forge one', () => {
  const request = {
    request: 1,
    path: 'notes.md',
    intent: 'update',
    untrusted: true,
    existing: true,
    added: 1,
    removed: 0,
    exact: true,
    changes: [{ kind: 'added', text: FORGED_CHROME }],
    remark: { preview: [FORGED_CHROME], lines: 1, label: '(U,priv)' },
  }
  const drawn = draw(t.asked(request))

  assert.match(drawn, /class="confirm untrusted"/)
  assert.equal(occurrences(drawn, 'class="quarantine-head"'), 0, drawn)
  assert.equal(occurrences(drawn, 'class="confirm untrusted"'), 1, drawn)
  assert.match(drawn, /&lt;div class=&quot;quarantine-head&quot;&gt;/)
  // The same bytes arrive twice, in the remark and in the diff, and neither is an element.
  assert.equal(occurrences(drawn, 'class="processor-remark"'), 1, drawn)
  assert.match(drawn, /<strong>[^<]*untrusted<\/strong>/)
  for (const attribute of FETCHING) assert.ok(!drawn.includes(attribute), drawn)

  // The mark distinguishes the two cases, which is what stops it from being decoration: a
  // container that always said `untrusted` would pass every assertion above and mark nothing.
  const trusted = draw(t.asked({ ...request, untrusted: false, remark: null }))
  assert.ok(!trusted.includes('untrusted'), `a vouched-for write is marked as one anyway:\n${trusted}`)
})
