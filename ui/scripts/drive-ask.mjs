// That the questions the planner asks can be answered from the window — including several
// at once, and a second series after the first is answered.
//
// A live turn: it drives a real model and costs a few tokens. The prompt asks the planner
// to put a choice to the person before doing anything, which is what makes it reach for
// the question it would otherwise only simulate in its reply.
//
// Note that a planner shown untrusted content may not ask at all — that gate is upstream
// and deliberate — so this drives a session with nothing quarantined in it.
import { _electron as electron } from 'playwright-core'
import { mkdirSync } from 'node:fs'

mkdirSync('/tmp/bravebot-ui', { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
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
if (await page.locator('.trust').isVisible().catch(() => false)) {
  await page.locator('.trust-actions .approve').click()
  await page.waitForTimeout(600)
}

await page.locator('.composer textarea').fill(
  'I want to add a --json flag to this project. Do not write anything. First, in ONE ask_user ' +
    'call, ask me two questions together: which approach, and what to name the flag. After I ' +
    'answer, make a SECOND ask_user call with one further question. Then just summarise my ' +
    'answers.',
)
await page.locator('.send').click()
console.log('sent; waiting to be asked…')

const asked = await page
  .locator('.confirm.ask')
  .first()
  .waitFor({ state: 'visible', timeout: 150000 })
  .then(() => true)
  .catch(() => false)

if (!asked) {
  console.log('  --   the planner answered without asking; nothing to drive this run')
  await page.screenshot({ path: '/tmp/bravebot-ui/16-not-asked.png' })
  await app.close()
  process.exit(0)
}

check(true, 'the questions reach the window')
const card = page.locator('.confirm.ask').first()

const questions = await card.locator('.ask-question').count()
check(questions >= 1, `one block per question (${questions})`)
check(
  (await card.locator('.ask-question .question').first().textContent())?.trim().length > 0,
  'the question itself is drawn',
)

const choices = await card.locator('.choices .choice').count()
check(choices >= 1, `the choices are drawn as the agent shaped them (${choices})`)
check(
  (await card.locator('.typed').count()) === questions,
  'and every question keeps a way to answer in your own words',
)

// The composer is closed to sending while the series stands.
check(
  await page.locator('.composer .send').isDisabled(),
  'nothing can be sent while the questions stand',
)
const placeholder = await page.locator('.composer textarea').getAttribute('placeholder')
check(
  (placeholder ?? '').includes('questions'),
  `and the composer says what is waiting (${placeholder})`,
)
await page.screenshot({ path: '/tmp/bravebot-ui/16-asked.png' })

check(questions >= 2, `several questions arrive in one series (${questions})`)

// Answer every question, so nothing is silently declined and the multi-question path is
// the one under test.
const first = card.locator('.choices .choice').first()
const label = (await first.locator('.label').textContent())?.trim() ?? ''
for (let at = 0; at < questions; at++) {
  const block = card.locator('.ask-question').nth(at)
  if ((await block.locator('.choices .choice').count()) > 0) {
    await block.locator('.choices .choice').first().click()
  } else {
    await block.locator('.typed').fill('whatever you think best')
  }
}
await page.waitForTimeout(200)
check(
  (await first.getAttribute('aria-pressed')) === 'true',
  'a picked choice says so, to itself and to a screen reader',
)
check(
  (await card.locator('.choice.picked').count()) >= 1,
  'each question keeps its own selection, independently of the others',
)

await card.locator('.confirm-actions .approve').click()
await page.waitForTimeout(1500)

const records = await card.locator('.asked-answer').count()
check(records === questions, `every question is recorded, not just the first (${records})`)
const given = (await card.locator('.asked-answer .given').first().textContent())?.trim() ?? ''
check(
  given === label,
  `the transcript records what was answered (${given} vs ${label})`,
)
check(
  (await card.locator('.choices').count()) === 0,
  'and stops offering the choices once they are answered',
)

// --- a second series, in the same turn -----------------------------------------------
//
// Nothing upstream limits a turn to one ask: it is an ordinary tool, so the model may reach
// for it again on a later round. Each series is its own question with its own id, and only
// the newest is outstanding.
const again = await page
  .locator('.confirm.ask')
  .nth(1)
  .waitFor({ state: 'visible', timeout: 150000 })
  .then(() => true)
  .catch(() => false)

if (again) {
  check(true, 'a second series in the same turn is put to the person too')
  const second = page.locator('.confirm.ask').nth(1)
  // The answered series must stay answered: its record stands and it offers no choices, so
  // a second question cannot reopen the first.
  const earlier = page.locator('.confirm.ask').first()
  check(
    (await earlier.locator('.choices').count()) === 0 &&
      (await earlier.locator('.asked-answer').count()) === questions,
    'the first series stays answered rather than reopening',
  )
  const secondChoices = await second.locator('.choices .choice').count()
  if (secondChoices > 0) await second.locator('.choices .choice').first().click()
  else await second.locator('.typed').first().fill('either is fine')
  await second.locator('.confirm-actions .approve').click()
  await page.waitForTimeout(1200)
  check(
    (await second.locator('.asked-answer').count()) > 0,
    'and it records what was answered',
  )
  await page.screenshot({ path: '/tmp/bravebot-ui/18-asked-again.png' })
} else {
  console.log('  --   the planner did not ask a second time this run')
}

const done = await page
  .locator('.composer textarea:not([disabled])')
  .waitFor({ state: 'visible', timeout: 150000 })
  .then(() => true)
  .catch(() => false)
check(done, 'the turn carries on with the answers')
await page.screenshot({ path: '/tmp/bravebot-ui/17-answered.png' })

await app.close()
console.log(problems.length ? `\nRESULT: ${problems.length} failed` : '\nRESULT: ok')
process.exit(problems.length ? 1 : 0)
