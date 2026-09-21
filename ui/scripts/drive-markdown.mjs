// That a reply is formatted, and that formatting it did not open anything.
//
// The interesting assertions here are the negative ones. Markdown rendering is the one
// place model output stops being text and starts becoming DOM, so this checks the things
// that must NOT appear — an <img>, a javascript: link, a same-window anchor — as well as
// the ones that must.
//
// Asserts on DOM shape rather than exact text: the model is asked to produce a document,
// and it will not echo one byte for byte.
import { _electron as electron } from 'playwright-core'
import { mkdirSync } from 'node:fs'

mkdirSync('/tmp/bravebot-ui', { recursive: true })

const ASK = `Reply with exactly this markdown document and nothing else:

## Heading

Some **bold** and a bit of \`inline code\`.

- first bullet
- second bullet

\`\`\`rust
fn main() { println!("a deliberately long line that should scroll rather than wrap in the bubble"); }
\`\`\`

| col a | col b |
| --- | --- |
| 1 | 2 |

A link: [example](https://example.com), a bad one: [nope](javascript:alert(1)),
an image: ![a picture](https://example.com/a.png)

And this literal text: <script>alert(1)</script>`

// The left column has two tabs and remembers which was last open, so a run after somebody left it
// on the bots — or after a run of `drive-bots.mjs` that was interrupted before its teardown — would
// find the session list present in the DOM and invisible, and every click below would time out
// against an element that is right there. Everything here is about that list, so it is put back on
// screen first: a driver leaves the window as the next one expects to find it, and does not assume
// it was left that way.
async function showSessions(page) {
  await page
    .locator('.sidebar-tab')
    .first()
    .waitFor({ state: 'visible', timeout: 15000 })
    .catch(() => undefined)
  await page
    .locator('.sidebar-tab')
    .first()
    .click({ timeout: 3000 })
    .catch(() => undefined)
  await page.waitForTimeout(250)
}

const app = await electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })
const page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')
await showSessions(page)
page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
page.on('console', (m) => m.type() === 'error' && console.log('CONSOLE ERROR:', m.text()))

await page.waitForTimeout(2000)
await page.locator('.session').first().click()
await page.waitForTimeout(1200)

await page.locator('.composer textarea').fill(ASK)
await page.locator('.send').click()
console.log('sent; waiting for the turn…')

const blocked = page.locator('.unconfigured')
for (let i = 0; i < 90; i++) {
  if (await blocked.isVisible().catch(() => false)) {
    console.log('RESULT: unconfigured screen shown')
    await app.close()
    process.exit(1)
  }
  const running = await page.locator('.working').isVisible().catch(() => false)
  if (!running && i > 3) break
  await page.waitForTimeout(1000)
}

const failed = await page.locator('.bubble.failed').count()
if (failed > 0) {
  console.log('RESULT: turn failed ->', await page.locator('.bubble.failed').last().textContent())
  await page.screenshot({ path: '/tmp/bravebot-ui/04-markdown-failed.png' })
  await app.close()
  process.exit(1)
}

const bubble = page.locator('.bubble.assistant').last()

// Rendered structure. The model may drop an element or two, so these are reported rather
// than asserted individually — the screenshot is the real review.
const counts = {
  headings: await bubble.locator('h1, h2, h3').count(),
  listItems: await bubble.locator('li').count(),
  codeBlocks: await bubble.locator('pre code').count(),
  inlineCode: await bubble.locator('code:not(pre code)').count(),
  tables: await bubble.locator('.md-table-wrap table').count(),
  links: await bubble.locator('a').count(),
}
console.log('rendered:', JSON.stringify(counts))

// The negative assertions. These are the ones that must hold whatever the model wrote.
const problems = []

const images = await bubble.locator('img').count()
if (images > 0) problems.push(`${images} <img> rendered; images must be inert labels`)

const jsLinks = await bubble.locator('a[href^="javascript:"], a[href^="file:"], a[href^="data:"]').count()
if (jsLinks > 0) problems.push(`${jsLinks} link(s) with a scheme that must never be openable`)

const anchors = await bubble.locator('a').all()
for (const a of anchors) {
  const [href, target, rel] = await Promise.all([
    a.getAttribute('href'),
    a.getAttribute('target'),
    a.getAttribute('rel'),
  ])
  if (target !== '_blank') problems.push(`link ${href} lacks target=_blank, so it is dead`)
  if (!(rel ?? '').includes('noopener')) problems.push(`link ${href} lacks rel=noopener`)
}

// Raw HTML must survive as visible text rather than being parsed or silently dropped.
const shown = (await bubble.innerText()) ?? ''
if (!shown.includes('<script>')) {
  problems.push('the literal <script> tag is not visible as text')
}

// A code block must scroll inside the bubble rather than widen it.
const overflows = await bubble.evaluate((el) => {
  const pre = el.querySelector('pre')
  return pre ? pre.scrollWidth > pre.clientWidth + 1 : null
})
console.log(`code block scrolls: ${overflows === null ? '(no pre found)' : overflows}`)

await page.screenshot({ path: '/tmp/bravebot-ui/04-markdown.png', fullPage: false })

// The dark palette is a separate set of tokens and is otherwise never looked at.
await page.emulateMedia({ colorScheme: 'dark' })
await page.waitForTimeout(400)
await page.screenshot({ path: '/tmp/bravebot-ui/05-markdown-dark.png', fullPage: false })

await app.close()

if (problems.length > 0) {
  console.log('\nRESULT: failed')
  problems.forEach((p) => console.log('  - ' + p))
  process.exit(1)
}
console.log('\nRESULT: ok — shots in /tmp/bravebot-ui/04-markdown.png and 05-markdown-dark.png')
