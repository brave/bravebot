// That moving the pointer over a conversation does not end the renderer process (#3581).
//
// Real Electron window, real bridge, a model service this script serves, and a scratch state
// directory, home and `--user-data-dir`: nothing of the person's is read or written. The session is
// made through the window and its record is marked as written by the terminal, as in the report.
// Each run then launches the app again, opens that session and sweeps the pointer (Playwright's
// mouse, which reaches the renderer as input and not as dispatched events) over the window. The
// window is short, so the inspector column holds more than it can show and scrolls. A renderer that
// ends is read from the page's `crash` event and the window's log line (DIAG-10).
//
// The failure is intermittent in the report, so one pass proves nothing: POINTER_RUNS sets how many
// runs are made (default 8) and a single run that ends the renderer fails the script.
//
//   pnpm run drive:pointer-crash
//   POINTER_RUNS=16 POINTER_HEIGHT=600 node scripts/drive-pointer-crash.mjs
//   POINTER_RECORD=<copy of a session record> node scripts/drive-pointer-crash.mjs
import assert from 'node:assert/strict'
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { approveTrustIfAsked, ASSISTANT_BUBBLE, COMPOSER_INPUT, COMPOSER_SEND, SHOTS_DIR } from './drive-support.mjs'

const RUNS = Number(process.env.POINTER_RUNS ?? 8)
const TURNS = Number(process.env.POINTER_TURNS ?? 3)
// Short enough that the inspector column (its panels and their headings) is taller than the window.
const viewport = { width: 1400, height: Number(process.env.POINTER_HEIGHT ?? 700) }
const output = process.env.POINTER_CRASH_OUTPUT || SHOTS_DIR
mkdirSync(output, { recursive: true })

// A planner that reads the project three times before it answers, so the session holds tool rows
// as well as replies, like the sessions the terminal writes.
const { service } = await startModelService(({ messages, round }) => {
  const lastUser = messages.map((message) => message.role).lastIndexOf('user')
  const trailing = messages.slice(lastUser + 1).filter((message) => message.role === 'tool').length
  return trailing < 3
    ? { tool: { name: 'list_files', arguments: { directory: '.' } } }
    : { content: `Reply ${round}. A paragraph of **markdown** with \`code\` and a list:\n\n- one\n- two\n\nAnd a closing line.` }
})
const world = makeWorld('bravebot-pointer-crash-', { service })
const { root, home, project, profile } = world
const state = join(home, '.bravebot')
const env = { ...world.env, BRAVEBOT_HOME: state }

/** Every file under `directory` whose name ends in `suffix`. */
const filesEnding = (directory, suffix) => readdirSync(directory, { recursive: true, withFileTypes: true })
  .filter((entry) => entry.isFile() && entry.name.endsWith(suffix))
  .map((entry) => join(entry.parentPath, entry.name))

/** The `window.fault` lines the agent has logged in the scratch state directory. */
const faultLines = () => {
  try {
    return filesEnding(join(state, 'logs'), '').flatMap((file) =>
      readFileSync(file, 'utf8').split('\n').filter((line) => line.includes(' window.fault ')))
  } catch {
    return []
  }
}

let failures = 0
// Playwright can throw from a protocol message of a page that is going away, outside any await of
// the run. It is recorded against the run in progress, as a run in which the page did not stay up.
let current = null
process.on('uncaughtException', (error) => {
  const message = error instanceof Error ? error.message.split('\n')[0] : String(error)
  if (current) current.fault = message
  else console.log(`       the driver reported, between runs: ${message}`)
})
try {
  // Write the session through the window, as the desktop app does.
  {
    const { app, page } = await launchWorld({ profile, env, viewport })
    try {
      await openProject(page, app, project, { trust: true })
      const replies = page.locator(ASSISTANT_BUBBLE)
      for (let turn = 0; turn < TURNS; turn++) {
        await page.locator(COMPOSER_INPUT).fill(`Question ${turn + 1}`)
        await page.locator(COMPOSER_SEND).click()
        await replies.nth(turn).waitFor()
        await page.locator('.composer .stop').waitFor({ state: 'hidden' })
      }
    } finally {
      await app.close()
    }
  }
  // Mark the record as the terminal's.
  const records = filesEnding(join(state, 'sessions'), '.json')
  assert.equal(records.length, 1, `one session record was written: ${records.join(', ')}`)
  let record = JSON.parse(readFileSync(records[0], 'utf8'))
  if (process.env.POINTER_RECORD) {
    // A copy of another record, moved into the scratch project. Only its location is rewritten.
    const copy = JSON.parse(readFileSync(process.env.POINTER_RECORD, 'utf8'))
    record = { ...copy, id: record.id, directory: record.directory, confined_lines_in: record.confined_lines_in, branch: null }
  }
  record.front = 'terminal'
  writeFileSync(records[0], JSON.stringify(record, null, 2))
  console.log(`session written (${process.env.POINTER_RECORD ? `copy of ${process.env.POINTER_RECORD}` : `${TURNS} turns`}), window ${viewport.width}x${viewport.height}`)

  for (let run = 1; run <= RUNS; run++) {
    const { app, page } = await launchWorld({ profile, env, viewport })
    let crashed = false
    page.on('crash', () => { crashed = true })
    let alive = true
    let setup = null
    const progress = { fault: null }
    current = progress
    try {
      await page.locator('.session').first().waitFor()
      await page.locator('.session').first().click()
      await approveTrustIfAsked(page)
      await page.locator('.turn-footer').last().waitFor({ state: 'attached' })
      await page.getByText(/written in the terminal/i).first().waitFor()
      const column = await page.evaluate(() => {
        const element = document.querySelector('.context-content')
        return element ? { scroll: element.scrollHeight, client: element.clientHeight } : null
      })
      assert.ok(column, 'the inspector column is drawn')
      if (run === 1) {
        console.log(`       inspector column: ${column.scroll}px of content in ${column.client}px`)
        await page.screenshot({ path: join(output, 'pointer-crash-open.png') })
      }
      assert.ok(column.scroll > column.client, 'the inspector column scrolls, which is what the sweep needs')
      const box = await page.locator('.entries').boundingBox()
      assert.ok(box, 'the transcript has a box')
      // Real pointer input in steps across every row, down and back up, twice.
      for (let pass = 0; pass < 2; pass++) {
        for (const y of [0.1, 0.3, 0.5, 0.7, 0.9, 0.7, 0.5, 0.3]) {
          for (const x of [0.2, 0.5, 0.8, 0.5]) {
            if (crashed) break
            await page.mouse.move(box.x + box.width * x, box.y + box.height * y, { steps: 8 })
          }
        }
      }
      await page.waitForTimeout(500)
      await page.evaluate(() => document.querySelectorAll('.turn-footer').length)
    } catch (error) {
      // A page that went away is the failure under test, wherever in the run it went. Anything
      // else is a failure of the run's own setup, reported as such and not counted as a pass.
      if (crashed || /crash|closed|disposed|Target/i.test(String(error))) {
        alive = false
        if (!crashed) console.log(`       ended by: ${String(error).split('\n')[0]}`)
      } else setup = error instanceof Error ? error.message.split('\n')[0] : String(error)
    } finally {
      await app.close().catch(() => undefined)
    }
    alive = alive && !crashed
    current = null
    if (progress.fault) console.log(`       the driver reported: ${progress.fault}`)
    if (setup) {
      failures++
      console.log(` FAIL  run ${run}/${RUNS}: the run did not get as far as the pointer: ${setup}`)
    } else {
      if (!alive) failures++
      console.log(`${alive ? '  ok  ' : ' FAIL '} run ${run}/${RUNS}: renderer ${alive ? 'survived' : 'ended'}`)
    }
  }
  const lines = faultLines()
  console.log(`window.fault lines: ${lines.length}${lines.length ? `\n${lines.join('\n')}` : ''}`)
} finally {
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}

console.log(failures ? `\nRESULT: ${failures} of ${RUNS} runs ended the renderer` : `\nRESULT: ok (${RUNS} runs)`)
process.exit(failures ? 1 : 0)
