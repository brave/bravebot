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
// the renderer was handed. The exception is the property stated as an absence: there is nothing
// to render to show that something is nowhere, so that one reads the sources and the manifest.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
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

/** A write of bytes a processor produced, which is the card the clause marks explicitly. */
const UNTRUSTED_WRITE = {
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

/**
 * Every kind of entry the transcript has, one fixture each, and what the card owes it.
 *
 * `marks` is the container the card draws around released content, asserted to appear exactly
 * once: the content is inside a mark the renderer drew and did not produce a second. `marks:
 * null` is the judgement that this card shows no released content, with the reason written
 * down, because that half of the quantifier is the half no grep can decide. Every fixture is
 * drawn either way, and every one of them has to escape what it carries and to make no request.
 *
 * `carries: false` is for the two kinds with no field to put content in at all.
 *
 * The keys are checked against the `Entry` union below rather than trusted, which is the whole
 * point of the table: a kind added to the transcript arrives red here until somebody either
 * draws it with its marking or records why it shows nothing to mark.
 */
const CARDS = {
  'turn-start': { entry: () => t.turnStarted(1), marks: null, why: 'an ordinal and nothing else', carries: false },
  consolidation: { entry: () => t.consolidating(), marks: null, why: 'a sentence this window wrote', carries: false },
  user: { entry: () => t.userSaid(FORGED_CHROME), marks: null, why: 'what the person typed themselves' },
  assistant: { entry: () => t.replied(FORGED_CHROME, 1), marks: null, why: 'the planner’s own words, which is what the one formatted surface means' },
  narration: { entry: () => t.narrated(FORGED_CHROME), marks: null, why: 'the agent’s account of what it did' },
  attached: { entry: () => ({ kind: 'attached', id: 'a1', path: FORGED_CHROME }), marks: null, why: 'the path somebody named, never the file’s bytes' },
  watch: { entry: () => t.watchFired(1, FORGED_CHROME), marks: null, why: 'the watch’s own line: a number and a path' },
  error: { entry: () => t.errored(FORGED_CHROME), marks: null, why: 'a diagnostic from the service the agent spoke to' },
  'replayed-tool': { entry: () => ({ kind: 'replayed-tool', id: 'r1', text: FORGED_CHROME }), marks: null, why: 'the line a record kept of a call, with no result in it' },
  tool: {
    // `Activity.changes` and `Activity.untrusted` are on the wire already and this card draws
    // neither. The day it does it needs a `marks`, which is the case this table exists for.
    entry: () => t.started({ verb: 'write', target: 'notes.md', note: FORGED_CHROME, failed: false, untrusted: false, changes: [] }),
    marks: null,
    why: 'the driver’s verb and the model’s own naming of its target',
  },
  run: {
    entry: () => t.askedRun({
      request: 1,
      stages: [{ display: FORGED_CHROME, resolved: '/bin/cat' }],
      directory: '/tmp/project',
      releasesPrivate: false,
      vouches: [{ program: 'cat', args: [], display: 'cat' }],
      summary: 'one command',
      stdin: 'output-1',
    }),
    marks: null,
    // crates/agent/src/confirm.rs documents `stdin` as the reference name and never the bytes.
    why: 'the planner’s own argv, what $PATH resolved it to, and a reference name for any input',
  },
  ask: {
    entry: () => t.askedQuestions({
      request: 1,
      prompts: [{ header: FORGED_CHROME, question: FORGED_CHROME, rows: [{ index: 0, label: FORGED_CHROME, detail: null }], multiple: false, key: 'k' }],
    }),
    marks: null,
    // labels.md:355 gives a reply out of a transport the context's label, not the network's.
    why: 'the planner’s questions, which are its words rather than content it read',
  },
  quarantined: { entry: () => confined([FORGED_CHROME, 'api_key = hunter2']), marks: '<pre class="preview">' },
  confirm: { entry: () => t.asked(UNTRUSTED_WRITE), marks: 'confirm untrusted' },
  output: {
    entry: () => t.askedOutput({ request: 1, command: 'cat notes.md', reference: 'output-1', lines: 1, output: FORGED_CHROME, summary: 'one line' }),
    marks: '<pre class="preview">',
  },
  vet: {
    entry: () => t.askedVet({ request: 1, origin: 'notes.md', expects: 'a note', content: FORGED_CHROME, lines: 1, vetting: { verdict: 'safe' } }),
    marks: '<pre class="preview">',
  },
  vouch: {
    entry: () => t.askedVouch({ request: 1, path: 'notes.md', preview: FORGED_CHROME, truncated: false }),
    marks: '<pre class="preview">',
  },
}

/**
 * Every entry kind the transcript declares, read out of the union that declares them.
 *
 * `Entry` is a TypeScript union, so none of it survives to runtime and no import can ask what
 * its members are. Its source is the next best thing, and it is the same source the compiler
 * holds the renderer's `switch` to, so the two lists cannot drift apart without this failing.
 */
function declaredKinds() {
  const source = readFileSync('src/renderer/transcript.ts', 'utf8')
  const start = source.indexOf('export type Entry = (')
  const end = source.indexOf('\n) & {', start)
  assert.ok(start >= 0 && end > start, 'the Entry union is not where this test looks for it')
  return new Set([...source.slice(start, end).matchAll(/\bkind: '([a-z-]+)'/g)].map((found) => found[1]))
}

test('every entry kind is drawn with its marking, whether or not its turn ended first', () => {
  // The quantifier the clause states, made mechanical. A new kind, or one dropped, fails here.
  assert.deepEqual(new Set(Object.keys(CARDS)), declaredKinds())

  for (const [kind, card] of Object.entries(CARDS)) {
    // A card said to mark nothing has to say why. That is the judgement a reader checks, and
    // leaving it out is how a card that does release something comes to sit in this column.
    if (!card.marks) assert.ok(card.why && card.why.length > 10, `${kind} marks nothing, for no stated reason`)

    // Both ways round. A turn ending sets `interrupted` on every unanswered question, and the
    // cheap thing to draw for one is a summary of its text, which is the bytes with no
    // container, no origin, no label, no reach line and, for an untrusted write, no border and
    // no warning. A turn ending declassifies nothing, so the marking is owed either way.
    for (const interrupted of [false, true]) {
      const drawn = draw(interrupted ? { ...card.entry(), interrupted: true } : card.entry())
      const where = `${kind}${interrupted ? ', interrupted' : ''}:\n${drawn}`

      if (card.marks) assert.equal(occurrences(drawn, card.marks), 1, where)
      // The head belongs to the one card that draws it. Anywhere else it would be content
      // having painted the chrome the reader is meant to trust.
      assert.equal(occurrences(drawn, 'class="quarantine-head"'), kind === 'quarantined' ? 1 : 0, where)
      if (card.carries !== false) {
        assert.match(drawn, /&lt;div class=&quot;quarantine-head&quot;&gt;/, where)
        assert.ok(!drawn.includes('<span class="mark">confined</span><span class="origin">README.md'), where)
      }
      for (const attribute of FETCHING) assert.ok(!drawn.includes(attribute), `drew ${attribute}, ${where}`)
    }
  }
})

/**
 * The second property is stated as an absence, so it is checked as one.
 *
 * `dangerouslySetInnerHTML` and a markdown plugin that turns HTML in content into elements are
 * the two routes by which the property above would stop being a property of the renderer and
 * become a property of whatever the content happens to hold. Neither is in the tree; an absence
 * written in a clause and asserted nowhere is one somebody has to remember.
 */
test('no route from content to raw markup exists in the front end', () => {
  const sources = []
  const walk = (directory) => {
    for (const item of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, item.name)
      if (item.isDirectory()) walk(path)
      else if (/\.(ts|tsx|js|jsx|mjs|cjs)$/.test(item.name)) sources.push(path)
    }
  }
  walk('src')
  // A walk that found nothing would pass the loop below without reading a line.
  assert.ok(sources.length > 20, `only ${sources.length} source files were scanned`)

  // Matched where it would do something, as a JSX prop or a key in a props object, rather than
  // wherever the word occurs: `src/main/export.ts` names it in the paragraph explaining why the
  // PDF is drawn by a real renderer instead of an injected string, and a check that forbade
  // saying so would be a check against writing the reason down.
  const RAW = [
    /\bdangerouslySetInnerHTML\s*[:=]/,
    /\.(inner|outer)HTML\s*=/,
    /\b(document\.write|insertAdjacentHTML)\s*\(/,
  ]
  for (const path of sources) {
    const source = readFileSync(path, 'utf8')
    for (const route of RAW) assert.ok(!route.test(source), `${path} reaches raw markup: ${route}`)
  }

  const manifest = JSON.parse(readFileSync('package.json', 'utf8'))
  const declared = Object.keys({ ...manifest.dependencies, ...manifest.devDependencies })
  for (const plugin of ['rehype-raw', 'rehype-stringify', 'remark-html', 'remark-rehype']) {
    assert.ok(!declared.includes(plugin), `${plugin} would turn HTML in content into elements`)
  }
  // And the surface all of this is about is still the one being drawn.
  assert.ok(declared.includes('react-markdown'), declared.join(' '))
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
  assert.match(inside, /class="[^"]*local-file-link[^"]*"/)
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
  const request = UNTRUSTED_WRITE
  const drawn = draw(t.asked(request))

  assert.match(drawn, /confirm untrusted/)
  assert.equal(occurrences(drawn, 'class="quarantine-head"'), 0, drawn)
  assert.equal(occurrences(drawn, 'confirm untrusted'), 1, drawn)
  assert.match(drawn, /&lt;div class=&quot;quarantine-head&quot;&gt;/)
  // The same bytes arrive twice, in the remark and in the diff, and neither is an element.
  assert.equal(occurrences(drawn, 'processor-remark'), 1, drawn)
  assert.match(drawn, /<strong>[^<]*untrusted<\/strong>/)
  for (const attribute of FETCHING) assert.ok(!drawn.includes(attribute), drawn)

  // The mark distinguishes the two cases, which is what stops it from being decoration: a
  // container that always said `untrusted` would pass every assertion above and mark nothing.
  const trusted = draw(t.asked({ ...request, untrusted: false, remark: null }))
  assert.ok(!trusted.includes('untrusted'), `a vouched-for write is marked as one anyway:\n${trusted}`)
})

/**
 * A failure is titled from the tag the agent sent, never from the words in the detail.
 *
 * The detail is prose: a service's own diagnostic, sometimes translated, sometimes quoting
 * something a model wrote. The card used to match it against `/401|403|unauthoriz|credential/`
 * and friends whenever no category arrived, which is the one thing the wire protocol forbids in
 * as many words, and is how a message that merely mentions a refusal becomes one.
 */
test('a failure card reads the category and never the wording of the detail', () => {
  const { ErrorCard } = load('src/renderer/components/ErrorCard.tsx')
  const draw = (props) => renderToStaticMarkup(React.createElement(ErrorCard, props))

  // The tag decides, and a detail whose prose points the other way does not move it.
  assert.match(draw({ category: 'rate-limited', detail: 'unauthorized: bad credential' }), /<strong>The provider is busy<\/strong>/)
  assert.match(draw({ category: 'cancelled', detail: 'HTTP 429 from the gateway' }), /<strong>Task stopped<\/strong>/)

  // With no tag there is nothing to read, so the card says the unknown-category thing rather
  // than guessing from the sentence. These four details are the four the sniffing matched.
  for (const detail of [
    'unauthorized: the credential was rejected (401)',
    'HTTP 429: rate limit reached',
    'the write was cancelled by the checker',
    'payment required: 402, no credit balance',
  ]) {
    const drawn = draw({ detail })
    assert.match(drawn, /<strong>The turn could not finish<\/strong>/, drawn)
  }

  // The detail is still shown, under the fold, which is where prose belongs.
  assert.match(draw({ detail: 'unauthorized: the credential was rejected (401)' }), /unauthorized: the credential was rejected \(401\)/)
})
