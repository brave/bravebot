// `/manifest <task>` typed into the desktop composer (docs/specs/manifest.md MANIFEST-9, MANIFEST-11;
// docs/specs/commands.md CMD-1).
//
// The real app and the real bridge, against a model service this script serves itself, which plans
// one write for each task it is given. Three runs:
//
// - approved: the plan card is drawn, approved, and the run's result is drawn as a run result and
//   no prompt bubble, and the session list then holds a "Plan run" row;
// - queued: a `/manifest` line typed while a turn is held waits in the queue, and starts once the
//   turn ends;
// - declined: the plan is declined, nothing is written, and the run ends on one caption line under
//   the decided card, which still shows the plan.
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`).
// Pass `--theme dark` or `--theme light` to take the screenshots in that theme.
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { APPROVE_BUTTON, COMPOSER_NAME, heldTurns, REJECT_BUTTON, SHOTS_DIR, showSessions, untilIn } from './drive-support.mjs'

const theme = process.argv.includes('--theme') ? process.argv[process.argv.indexOf('--theme') + 1] : 'light'

const { hold, release, waiting } = heldTurns()

// A run asks the planner twice: once for the plan in words, then for its steps. Each task names an
// id, and the plan for it writes `plan-<id>.md`.
const body = (id) => `# Written by plan ${id}\n`
const file = (id) => `plan-${id}.md`
let planning = null
const { service, rounds } = await startModelService(async ({ body: sent, messages }) => {
  if (planning) {
    const id = planning
    planning = null
    return { content: JSON.stringify({ steps: [{ capability: 'FILE_WRITE', args: { path: file(id), contents: body(id) } }] }) }
  }
  const task = /TASK-(\w+)/.exec(sent)
  if (task) {
    planning = task[1]
    return { content: `1. Write ${file(task[1])} with a fixed body.` }
  }
  const last = JSON.stringify(messages.at(-1) ?? '')
  if (last.includes('Hold on')) { await hold(); return { content: 'Done holding.' } }
  return { content: 'I looked around the project.' }
})

const { project, profile, env } = makeWorld('bravebot-manifest-', { service })
const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? SHOTS_DIR
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1200, height: 800 } }))
  await page.emulateMedia({ colorScheme: theme === 'dark' ? 'dark' : 'light' })
  await openProject(page, app, project)

  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const stop = page.locator('.composer .stop')
  const card = page.locator('.confirm.manifest').last()
  const transcript = () => page.locator('.entries').innerText()
  const until = untilIn(page, 400)
  // A yes stays shut until the rows the card rests on have been on screen (PROMPT-4), so the card is
  // read top to bottom first, as drive-manual-walkthrough.mjs does.
  const OPEN_APPROVE = `${APPROVE_BUTTON}:not([aria-disabled="true"])`
  const readThrough = (question) => question.evaluate(async (card) => {
    const entries = card.closest('.entries')
    const frames = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    const top = card.getBoundingClientRect().top - entries.getBoundingClientRect().top + entries.scrollTop
    const furthest = () => entries.scrollHeight - entries.clientHeight
    for (let at = Math.max(0, top - 24); ; at += 120) {
      const target = Math.min(at, furthest()), until = performance.now() + 5000
      do { entries.scrollTo({ top: target, behavior: 'instant' }); await frames() } while (Math.abs(entries.scrollTop - target) > 1 && performance.now() < until)
      if (at >= furthest() - 1) break
    }
  })
  const approve = async (question) => {
    await readThrough(question)
    await question.locator(OPEN_APPROVE).waitFor({ timeout: 30000 })
    await question.locator(OPEN_APPROVE).click()
  }
  // A write the plan reaches is still put to the person (MANIFEST-11); answer each with a yes.
  const approveWrites = async (done) => {
    for (let n = 0; n < 400; n++) {
      if (await done()) return
      const asked = page.locator(`.confirm:not(.manifest) ${OPEN_APPROVE}`)
      if (await asked.count()) await asked.first().click().catch(() => undefined)
      await page.waitForTimeout(50)
    }
    assert.fail('the approved run ended')
  }
  const promptBubbles = (text) => page.locator('.bubble.user:not(.plan-task)', { hasText: text }).count()
  const rows = (kind) => page.locator(`.entries .${kind}`).count()

  await composer.fill('Look around')
  await composer.press('Enter')
  await until(async () => (await transcript()).includes('I looked around the project.'), 'the first turn answered')
  await stop.waitFor({ state: 'hidden' })

  // ---- approved -----------------------------------------------------------------------------------
  await composer.fill('/manifest list the files TASK-alpha')
  await composer.press('Enter')
  await card.waitFor({ state: 'visible', timeout: 60000 })
  assert.match(await card.innerText(), /list the files TASK-alpha/, 'the card names the task')
  assert.match(await card.locator('.manifest-steps').innerText(), /plan-alpha\.md/, 'the card lists the step')
  assert.equal(await page.locator('.plan-task', { hasText: 'TASK-alpha' }).count(), 1, 'the task is drawn as a plan row')
  assert.equal(await promptBubbles('TASK-alpha'), 0, 'the task is no prompt bubble')
  // The card lists the steps, so the agent's narration of them is not drawn as well; the goal is.
  assert.ok(!(await transcript()).includes('Plan, fixed before anything runs'), 'the narrated plan gave way to the card')
  assert.ok((await transcript()).includes('Goal, as understood'), 'the goal stays')
  // At 1200×800 the card's yes sits above the composer once the transcript is at its end.
  await page.locator('.entries').evaluate((entries) => entries.scrollTo({ top: entries.scrollHeight, behavior: 'instant' }))
  await until(async () => {
    const yes = await card.locator(APPROVE_BUTTON).boundingBox()
    const box = await page.locator('footer.composer').boundingBox()
    return Boolean(yes && box && yes.y + yes.height <= box.y)
  }, 'Run this plan is above the composer')
  await page.screenshot({ path: join(output, `manifest-plan-${theme}.png`) })
  await approve(card)
  await approveWrites(async () => (await rows('plan-reply')) === 1)
  await stop.waitFor({ state: 'hidden' })
  assert.equal(readFileSync(join(project, file('alpha')), 'utf8'), body('alpha'), 'the approved plan wrote its file')
  assert.equal(await promptBubbles('TASK-alpha'), 0, 'still no prompt bubble once the run is done')
  // The reply is marked as the run's result, apart from the agent's own words (MANIFEST-11).
  assert.match(await page.locator('.plan-reply .plan-reply-head').last().textContent(), /run result/)
  assert.match(await page.locator('.plan-reply').last().innerText(), /Ran the 1 step/)
  // The result wraps inside its box, and starts where the head and the foot start.
  const fit = await page.locator('.plan-reply').last().evaluate((reply) => {
    const left = (selector) => {
      const element = reply.querySelector(selector)
      return element.getBoundingClientRect().left + parseFloat(getComputedStyle(element).paddingLeft)
    }
    const preview = reply.querySelector('.preview')
    return { overflow: preview.scrollWidth - preview.clientWidth, text: left('.preview'), head: left('.plan-reply-head'), foot: left('.plan-reply-foot') }
  })
  assert.ok(fit.overflow <= 0, `the result runs off its box: ${JSON.stringify(fit)}`)
  assert.ok(Math.abs(fit.text - fit.head) < 1 && Math.abs(fit.text - fit.foot) < 1, `the result's left edge: ${JSON.stringify(fit)}`)
  await showSessions(page)
  await until(async () => (await page.locator('.session .plan-run').count()) > 0, 'the session list holds a Plan run row')
  await page.screenshot({ path: join(output, `manifest-approved-${theme}.png`) })

  // ---- queued while a turn runs -------------------------------------------------------------------
  await composer.fill('Hold on')
  await composer.press('Enter')
  await stop.waitFor({ state: 'visible' })
  await until(async () => waiting.length > 0, 'the held turn reached the model')
  const asked = rounds.length
  await composer.fill('/manifest again TASK-beta')
  await composer.press('Enter')
  const queue = page.locator('.queued-messages')
  await until(async () => (await queue.count()) > 0 && (await queue.innerText()).includes('/manifest again TASK-beta'), 'the /manifest line waits in the queue')
  assert.equal(await page.locator('.confirm.manifest', { hasText: 'TASK-beta' }).count(), 0, 'no plan while the turn runs')
  assert.equal(await page.locator('.plan-task', { hasText: 'TASK-beta' }).count(), 0, 'no plan row while it waits')
  assert.equal(rounds.length, asked, 'the planner was not asked while the turn runs')
  assert.equal(await queue.locator('.tray-row .plan-task-mark').innerText(), 'PLAN', 'the waiting line carries the plan mark')
  assert.equal(await queue.locator('.tray-note').innerText(), 'Each starts after this one', 'the caption calls no plan run a turn')
  await page.screenshot({ path: join(output, `manifest-queued-${theme}.png`) })
  release()
  const second = page.locator('.confirm.manifest', { hasText: 'TASK-beta' })
  await second.waitFor({ state: 'visible', timeout: 60000 })
  assert.ok((await transcript()).includes('Done holding.'), 'the held turn ended first')
  assert.equal(await queue.count(), 0, 'the queue is empty once the run starts')
  await approve(second)
  await approveWrites(async () => (await rows('plan-reply')) === 2)
  await stop.waitFor({ state: 'hidden' })
  assert.equal(readFileSync(join(project, file('beta')), 'utf8'), body('beta'), 'the queued plan ran')

  // ---- declined -----------------------------------------------------------------------------------
  await composer.fill('/manifest write nothing TASK-gamma')
  await composer.press('Enter')
  const third = page.locator('.confirm.manifest', { hasText: 'TASK-gamma' })
  await third.waitFor({ state: 'visible', timeout: 60000 })
  await third.locator(REJECT_BUTTON).click()
  // The decided card above shows the plan, so the ending is one caption line with no fold.
  const ended = page.locator('.plan-ended-note').last()
  await ended.waitFor({ state: 'visible', timeout: 60000 })
  assert.match(await ended.innerText(), /^Nothing was read or written\. Saved as run \S+\.$/)
  assert.equal(await page.locator('.plan-ended').count(), 0, 'no plan-ended box for a declined plan')
  await page.screenshot({ path: join(output, `manifest-declined-${theme}.png`) })
  await third.getByText('Show', { exact: true }).click()
  assert.match(await third.locator('.manifest-steps').innerText(), /plan-gamma\.md/, 'the decided card still shows the declined plan')
  assert.ok(!existsSync(join(project, file('gamma'))), 'a declined plan wrote nothing')
  assert.equal(await promptBubbles('TASK-gamma'), 0, 'the declined task is no prompt bubble')

  assert.deepEqual(errors, [])
  console.log(`PASS: /manifest approved, queued while a turn ran, and declined. Screenshots in ${output}/manifest-*-${theme}.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'manifest-failure.png') }).catch(() => undefined)
  throw error
} finally {
  release()
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
}
