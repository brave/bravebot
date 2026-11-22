// That a person can ask the Conversation permissions panel which rule decides a call (CLI-30):
// `Bash git push` is refused by the person's own deny rule, and the panel names the rule, the file
// that wrote it and the reason. A checkout's allow rule that is not granted is named as not in force.
//
// The real app and the real bridge, against a model service this script serves itself. Nothing is
// paid for. Screenshots of each answer are taken in light and dark.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run drive:rule-check` does first.
import assert from 'node:assert/strict'
import { mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { SHOTS_DIR } from './drive-support.mjs'

const { service } = await startModelService(() => ({ content: 'done' }))

const { root, home, project, profile, env } = makeWorld('bravebot-rule-check-', {
  service,
  dirs: ['project/.bravebot'],
  settings: {
    permissions: {
      deny: [{ rule: 'Bash(git push *)', reason: 'pushes go through review' }],
      ask: ['Bash(git commit *)'],
      allow: ['Bash(git *)'],
    },
  },
})
const homeFile = join(home, '.bravebot/settings.json')
const checkout = join(project, '.bravebot/settings.json')
writeFileSync(checkout, JSON.stringify({ permissions: { allow: ['Bash(cargo test *)'] } }))

const output = SHOTS_DIR
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1440, height: 900 } }))
  await openProject(page, app, project)
  // The checkout's allow rule is asked about first (PERM-15). Declining leaves it not in force.
  const rulesQuestion = page.getByRole('dialog', { name: 'Project rules', exact: true })
  await rulesQuestion.waitFor()
  await rulesQuestion.locator('[data-test="rules-decline"]').click()
  await rulesQuestion.waitFor({ state: 'hidden' })

  await page.getByRole('button', { name: 'More', exact: true }).click()
  await page.getByRole('menuitem', { name: 'Permissions…', exact: true }).click()
  const dialog = page.getByRole('dialog', { name: 'Conversation permissions', exact: true })
  const section = dialog.locator('[data-test="rule-check"]')
  await section.waitFor()

  const shots = async (name) => {
    // The dialog scrolls inside Leo's shadow root, so each scrolling box above the answer is taken to its end.
    await section.locator('[data-test="rule-check-answer"]').evaluate((answer) => {
      for (let node = answer; node; node = node.assignedSlot ?? node.parentElement ?? node.getRootNode().host) {
        if (node.scrollHeight > node.clientHeight) node.scrollTo({ top: node.scrollHeight, behavior: 'instant' })
      }
    })
    await page.waitForTimeout(300)
    await page.screenshot({ path: join(output, `${name}-light.png`) })
    await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'dark'))
    await page.screenshot({ path: join(output, `${name}-dark.png`) })
    await page.evaluate(() => document.documentElement.removeAttribute('data-theme'))
  }
  /** Types `words`, presses Check, and waits for an answer with `decision`; a different one times out. */
  const check = async (words, decision) => {
    await section.locator('[data-test="rule-check-words"] input').fill(words)
    await section.locator('[data-test="rule-check-submit"]').click()
    await section.locator(`[data-test="rule-check-decision"][data-decision="${decision}"]`).waitFor({ timeout: 10000 })
    return section.locator('[data-test="rule-check-answer"]').innerText()
  }

  // ---- the person's own deny rule decides, with its file and reason ---------------------------
  const refused = await check('git push origin main', 'deny')
  assert.ok(refused.includes('Bash(git push *)'), refused)
  // A long path is cut in the middle on screen and spelled whole as the name and tooltip.
  const file = section.locator('[data-test="rule-check-file"] code')
  assert.equal(await file.getAttribute('aria-label') ?? await file.innerText(), homeFile, 'the file that wrote the rule is named')
  assert.ok(refused.includes('pushes go through review'), refused)
  await shots('rule-check-deny')

  // ---- a checkout's allow rule that was not granted is not in force ---------------------------
  await check('cargo test', 'none')
  const note = await section.locator('[data-test="rule-check-not-in-force"]').innerText()
  assert.ok(note.includes('Bash(cargo test *)') && note.includes(checkout), note)
  await shots('rule-check-not-in-force')

  assert.deepEqual(errors, [])
  console.log(`PASS: Bash git push is refused by the person's own rule, named with its file and reason, and a checkout's ungranted allow rule is named as not in force. Screenshots in ${output}/rule-check-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'rule-check-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
