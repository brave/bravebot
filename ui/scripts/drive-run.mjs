// That a command the planner wants to run can actually be approved from the window.
//
// A live turn: it drives a real model, costs a few tokens, and takes a while. There is no
// way to fake it that would prove anything — the whole question is whether an approval
// made in the interface reaches the turn that is blocked waiting for it.
//
// Needs credentials baked into bravebot-rpc (`npm run bridge` in a configured shell).
import { _electron as electron } from 'playwright-core'
import { mkdirSync } from 'node:fs'

mkdirSync('/tmp/bravebot-ui', { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
}

/** Wait for a locator to appear, reporting rather than throwing. */
async function appears(locator, seconds, what) {
  try {
    await locator.first().waitFor({ state: 'visible', timeout: seconds * 1000 })
    check(true, what)
    return true
  } catch {
    check(false, `${what} (nothing after ${seconds}s)`)
    return false
  }
}

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
await page.waitForTimeout(2500)

if ((await page.locator('.session').count()) === 0) {
  console.log('RESULT: skipped — no sessions to open')
  await app.close()
  process.exit(0)
}

await page.locator('.session').first().click()
await page.waitForTimeout(1500)

// A session whose record kept no trust map asks again on open. Trusting is what lets the
// turn get as far as wanting to run something.
if (await page.locator('.trust').isVisible().catch(() => false)) {
  await page.locator('.trust-actions .approve').click()
  await page.waitForTimeout(600)
}

await page
  .locator('.composer textarea')
  .fill('Run `git status --short` in this directory, then tell me how many lines it printed.')
await page.locator('.send').click()
console.log('sent; waiting for the command to be put to us…')

// --- the run card ---------------------------------------------------------------------
if (await appears(page.locator('.confirm.run'), 120, 'the pipeline is put to the person')) {
  const card = page.locator('.confirm.run').first()
  const stages = await card.locator('.stages li').count()
  check(stages >= 1, `the argv is shown, one line per stage (${stages})`)

  const argv = (await card.locator('.stages .argv').first().textContent())?.trim() ?? ''
  check(argv.includes('git'), `the command is shown as it will run (${argv})`)
  const resolved = (await card.locator('.stages .resolved').first().textContent())?.trim() ?? ''
  check(
    resolved.startsWith('/') || resolved === 'not found on PATH',
    `and what the name resolved to (${resolved})`,
  )

  // Drafting while a question stands is allowed; *sending* is not, so a prompt cannot get
  // past a decision nobody has made.
  check(
    await page.locator('.composer .send').isDisabled(),
    'nothing can be sent while the question stands',
  )
  const placeholder = await page.locator('.composer textarea').getAttribute('placeholder')
  check(
    (placeholder ?? '').includes('command'),
    `and says which question is waiting (${placeholder})`,
  )

  check(
    (await card.locator('.confirm-actions button').count()) === 3,
    'three answers: refuse, run once, run and stop asking',
  )
  await page.screenshot({ path: '/tmp/bravebot-ui/13-run-asked.png' })

  await card.locator('.confirm-actions .approve:not(.always)').click()
  await page.waitForTimeout(1200)
  check(
    (await card.locator('.decided.approve').textContent())?.includes('once') ?? false,
    'the card records that it was run once, not vouched for',
  )
} 

// --- the output card, if the planner asks to read what it printed ---------------------
const sawOutput = await page
  .locator('.confirm.output')
  .first()
  .waitFor({ state: 'visible', timeout: 60000 })
  .then(() => true)
  .catch(() => false)

if (sawOutput) {
  check(true, 'reading the output is put to the person as its own question')
  const card = page.locator('.confirm.output').first()
  // Whatever the command printed, all of it is here — including nothing, which is a real
  // answer and the case a `length > 0` assertion would fail on for the wrong reason. What
  // matters is that the card's own line count and the bytes it shows agree, so a
  // truncation could not hide behind a smaller number.
  const shown = (await card.locator('.preview').textContent()) ?? ''
  const stated = Number(((await card.locator('.counts').textContent()) ?? '0').split(' ')[0])
  const actual = shown === '' ? 0 : shown.split('\n').length
  check(
    actual === stated,
    `the bytes shown are all of them (${actual} lines, card says ${stated})`,
  )
  await page.screenshot({ path: '/tmp/bravebot-ui/14-output-asked.png' })
  await card.locator('.confirm-actions .approve').click()
  await page.waitForTimeout(1000)
} else {
  console.log('  --   the planner did not ask to read the output this time')
}

// --- the turn finishes ----------------------------------------------------------------
const done = await page
  .locator('.composer textarea:not([disabled])')
  .waitFor({ state: 'visible', timeout: 120000 })
  .then(() => true)
  .catch(() => false)
check(done, 'the turn runs to completion after the answers')

// The call is reported by verb, with the command itself on the card rather than the line —
// so this looks for the verb, not for the argv.
const verbs = await page.locator('.tool .verb').allTextContents()
check(verbs.includes('Run'), `the transcript shows the command actually ran (${verbs.join(', ')})`)
check(
  (await page.locator('.quarantine').count()) > 0,
  'and that what it printed was confined rather than read',
)
await page.screenshot({ path: '/tmp/bravebot-ui/15-run-done.png' })

await app.close()
console.log(problems.length ? `\nRESULT: ${problems.length} failed` : '\nRESULT: ok')
process.exit(problems.length ? 1 : 0)
