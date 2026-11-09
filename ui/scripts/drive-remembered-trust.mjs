// TRUST-23 and TRUST-24 in the window: a yes at the trust question is kept and settles the next
// session started in that directory, which says so above its transcript, and Forget in
// Permissions makes the next one ask again. The question has two answers and no separate one to
// remember. Real Electron and bridge, across two launches with one home; only the file picker is
// substituted.
//
// The project's name ends in a space, so the question and the list of what is kept draw the
// directory as the agent spells it (PROMPT-1): quoted, with the space as `␣`, and not as a name
// that reads like the one without it.
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { inheritedEnv, launchWorld } from './drive-world.mjs'
import { TRUST_APPROVE, approveTrust, readTrust } from './drive-support.mjs'

const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-remembered-')))
const home = join(root, 'home'), project = join(root, 'project '), profile = join(root, 'profile')
const spelledProject = `"${project.slice(0, -1)}\u2423"`
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
const shots = '/tmp/bravebot-ui'
mkdirSync(shots, { recursive: true })

async function launch(drive, directory = project, viewport = undefined) {
  const env = Object.assign(inheritedEnv(), { HOME: home, XDG_CONFIG_HOME: join(home, '.config') })
  const { app, page, errors } = await launchWorld({ profile, env, timeout: 15000, viewport })
  try {
    await app.evaluate(({ dialog }, path) => {
      dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] })
    }, directory)
    await drive(page)
    assert.deepEqual(errors, [])
  } finally {
    await app.close()
  }
}

const trust = (page) => page.getByRole('dialog', { name: 'Project trust', exact: true })
const banner = (page) => page.locator('.entries .session-banner', { hasText: 'You trusted this directory' })
const openProject = (page) => page.getByRole('button', { name: 'Open project', exact: true }).click()

// A long directory in the smallest window: three 255-character components, the longest each can be.
// The path fills its box and the rows under it start out of view. The record is named by a bounded
// key, so the yes can be kept here as for any other directory. The yes is shut, so a press sends
// nothing and writes no record, until the question has been read through. Then the yes keeps the
// answer with `confined_lines`, which the sentence on screen is the ground for (TRUST-7, TRUST-23).
const far = join(root, 'a'.repeat(255), 'b'.repeat(255), 'c'.repeat(255))
mkdirSync(far, { recursive: true })

try {
  await launch(async (page) => {
    await openProject(page)
    const asked = trust(page)
    const yes = asked.locator(TRUST_APPROVE)
    await yes.waitFor()
    const box = await asked.locator('.path').evaluate((path) => ({ scrolls: path.scrollHeight > path.clientHeight + 1, height: path.clientHeight }))
    assert.ok(box.scrolls, `the long path scrolls inside its own box: ${JSON.stringify(box)}`)
    const record = (await asked.locator('.aside code').innerText()).trim()
    assert.equal(await yes.getAttribute('aria-disabled'), 'true', 'the yes is shut in a question nobody has read')
    assert.match(await asked.locator('.shown-left').innerText(), /^\d+ more lines? to read before approving$/, 'the question says how many lines are left')
    assert.equal(await asked.getByRole('button', { name: "Don't trust", exact: true }).getAttribute('aria-disabled'), null, 'the no is never held')
    await yes.click({ force: true })
    await yes.dispatchEvent('click')
    await page.waitForTimeout(500)
    assert.equal(await asked.count(), 1, 'a press on the shut yes answered the question')
    assert.equal(existsSync(record), false, 'a press on the shut yes wrote the record')
    await page.screenshot({ path: `${shots}/remembered-0-long-path-waiting.png` })
    await readTrust(asked)
    await page.waitForTimeout(500)
    assert.notEqual(await yes.getAttribute('aria-disabled'), 'true', 'the yes opens once the question has been read')
    await yes.click()
    await asked.waitFor({ state: 'hidden' })
    assert.match(readFileSync(record, 'utf8'), /"confined_lines":true/, 'the entry says the question said so')
    rmSync(record)
  }, far, { width: 900, height: 560 })

  let record = null
  await launch(async (page) => {
    await openProject(page)
    const asked = trust(page)
    const yes = asked.getByRole('button', { name: 'Trust and remember', exact: true })
    await yes.waitFor()
    assert.equal(await asked.locator(TRUST_APPROVE).innerText(), 'Trust and remember', 'the yes is the one that keeps')
    assert.equal(await asked.locator('[data-test="trust-remember"]').count(), 0, 'there is no separate answer to remember')
    assert.equal(await asked.locator('code.path').innerText(), spelledProject, 'the question draws the directory as spelled')
    record = (await asked.locator('.aside code').innerText()).trim()
    assert.ok(record.startsWith(join(home, '.bravebot', 'trusted')), `the question names the record: ${record}`)
    // RUN-5: the question says what a yes also grants, in the words the terminal draws.
    assert.match(
      await asked.locator('[data-test="trust-confined-lines"]').innerText(),
      /without asking when the sandbox confines it/,
      'the question says that confined lines run without asking',
    )
    await page.screenshot({ path: `${shots}/remembered-1-offered.png` })
    await approveTrust(asked)
    await asked.waitFor({ state: 'hidden' })
    await banner(page).waitFor()
    assert.ok(existsSync(record), 'a yes wrote the record it named')
    assert.match(readFileSync(record, 'utf8'), /"confined_lines":true/, 'the entry says the question said so')
    await page.screenshot({ path: `${shots}/remembered-2-kept.png` })
  })

  await launch(async (page) => {
    await openProject(page)
    const shown = banner(page)
    await shown.waitFor()
    assert.match(await shown.innerText(), /chats started here are not asked/)
    assert.equal(await trust(page).count(), 0, 'a kept yes asks nothing')
    await page.screenshot({ path: `${shots}/remembered-3-not-asked.png` })

    await shown.getByRole('button', { name: 'Manage', exact: true }).click()
    const forget = page.getByRole('button', { name: 'Forget', exact: true })
    await forget.waitFor()
    const rootSpan = page.locator('[data-test="remembered-root"]')
    const visible = (await rootSpan.innerText()).trim()
    assert.ok(visible.startsWith('· for '), 'the list names the directory the answer is about')
    // A root longer than the row is cut in the middle. The full spelling is the accessible name.
    const labeled = await rootSpan.getAttribute('aria-label')
    assert.equal(labeled ?? visible.replace(/^·\s*/, ''), `for ${spelledProject}`, 'the list names the directory the answer is about as spelled')
    await page.screenshot({ path: `${shots}/remembered-4-permissions.png` })
    await forget.click()
    await page.getByText('Nothing is kept for this directory.', { exact: true }).waitFor()
    assert.equal(existsSync(record), false, 'forgetting removed the record')
    assert.equal(await shown.count(), 0, 'the banner went with the answer')
    await page.getByRole('button', { name: 'Done', exact: true }).click()

    await page.getByRole('button', { name: 'New chat', exact: true }).click()
    await trust(page).waitFor()
    await page.screenshot({ path: `${shots}/remembered-5-asked-again.png` })
  })
  console.log(`Kept trust verified across two launches. Screenshots in ${shots}/remembered-*.png`)
} finally {
  rmSync(root, { recursive: true, force: true })
}
