// `/goal <condition>` typed into the composer (docs/specs/goal.md, GOAL-1, GOAL-3, GOAL-6, GOAL-7).
//
// The real app and the real bridge, against a model service this script serves itself. A goal is
// set, then a prompt is sent. When its turn ends the window asks for the goal's next step, and the
// bridge puts the goal to the judge: a request with no tools. The judge answers NOT MET, so the
// verdict is drawn as a row and one more turn is sent by itself, with the driver's sentence as the
// prompt. The judge answers MET for that turn, and the goal ends.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`pnpm run build`).
// Pass `--theme dark` or `--theme light` to take the screenshots in that theme.
import assert from 'node:assert/strict'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_NAME, SHOTS_DIR } from './drive-support.mjs'

const theme = process.argv.includes('--theme') ? process.argv[process.argv.indexOf('--theme') + 1] : 'light'
const CONDITION = 'a.txt exists'
let judged = 0

const { service, rounds: requests } = await startModelService(async ({ body }) => {
  // The judge is the only request sent with no tools (GOAL-3).
  if (!JSON.parse(body).tools) {
    judged += 1
    return { content: judged === 1 ? 'NOT MET\na.txt is not in the project yet.' : 'MET\na.txt is in the project.' }
  }
  return { content: 'I made a start on a.txt.' }
})

const { project, profile, env } = makeWorld('bravebot-goal-', { service })
const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? SHOTS_DIR
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1200, height: 800 } }))
  await page.emulateMedia({ colorScheme: theme === 'dark' ? 'dark' : 'light' })
  await openProject(page, app, project)

  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const transcript = () => page.locator('.entries').innerText()
  const until = async (check, what) => {
    for (let tries = 0; tries < 300; tries++) { if (await check()) return; await page.waitForTimeout(50) }
    assert.fail(`${what}\n${await transcript()}`)
  }

  // Setting a goal sends nothing (GOAL-2).
  await composer.fill(`/goal ${CONDITION}`)
  await composer.press('Enter')
  await until(async () => (await transcript()).includes(CONDITION), 'the goal is said to be set')
  assert.equal(requests.length, 0, 'setting a goal sends no request')
  assert.equal(await page.locator('.bubble.user', { hasText: '/goal' }).count(), 0, 'no prompt bubble for /goal')

  await composer.fill('create a.txt')
  await composer.press('Enter')
  await until(async () => (await transcript()).includes('Checking whether the goal is met'), 'the check is drawn')
  await until(async () => (await transcript()).includes('a.txt is not in the project yet.'), 'the NOT MET verdict is drawn')
  await page.screenshot({ path: join(output, `goal-not-met-${theme}.png`) })

  // The work is sent back by itself, and that turn is judged MET.
  await until(async () => (await transcript()).includes('a.txt is in the project.'), 'the MET verdict is drawn')
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(output, `goal-met-${theme}.png`) })

  const turns = requests.map((body) => JSON.parse(body)).filter((body) => body.tools)
  assert.equal(judged, 2, 'two checks')
  assert.equal(turns.length, 2, 'the prompt and one turn the goal sent by itself')
  const carried = JSON.stringify(turns[1].messages.at(-1))
  assert.ok(carried.includes(CONDITION) && carried.includes('a.txt is not in the project yet.'), `the second turn carries the condition and the reason: ${carried}`)
  assert.deepEqual(errors, [], 'no page errors')
  console.log(`drive-goal: ok (${theme})`)
} finally {
  await app?.close()
  service.close()
}
