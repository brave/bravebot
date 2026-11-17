// Adding and closing folders beside a chat's project from the Files tab and the More menu: the
// Folders section empty, a folder added through the picker and listed with the tree showing it as
// a second root, the folder closed again, a choice the agent refuses said in the section, both
// controls shut while a turn runs, and the More menu's Add folder item.
//
// The real app and the real bridge, against a model service this script serves itself. Only the
// OS folder picker is substituted. Needs `bravebot-rpc` and `bravebot-ui-files` built
// (`npm run bridge`) and the app built (`electron-vite build`), which `npm run drive:folders`
// does first. Screenshots go to FOLDERS_OUTPUT, or /tmp/bravebot-ui, named FOLDERS_PREFIX + state.
import assert from 'node:assert/strict'
import { mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_NAME, SHOTS_DIR } from './drive-support.mjs'

// While set, the next reply waits until the test resolves it, so a turn stays running.
let held = null
const { service } = await startModelService(async () => {
  if (held) await held.promise
  return { content: 'done' }
})

const { root, project, profile, env } = makeWorld('bravebot-folders-', {
  service,
  dirs: ['project/src', 'beside/docs'],
})
writeFileSync(join(project, 'README.md'), '# Project\n')
writeFileSync(join(project, 'src/main.rs'), 'fn main() {}\n')
writeFileSync(join(root, 'beside/guide.md'), '# Guide\n')
writeFileSync(join(root, 'beside/docs/api.md'), '# API\n')
const beside = join(root, 'beside')

const output = process.env.FOLDERS_OUTPUT || SHOTS_DIR
const prefix = process.env.FOLDERS_PREFIX ?? ''
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 900 } }))
  await openProject(page, app, project)
  const pick = (path) => app.evaluate(({ dialog }, chosen) => {
    globalThis.__picked = 0
    dialog.showOpenDialog = async () => { globalThis.__picked++; return chosen ? { canceled: false, filePaths: [chosen] } : { canceled: true, filePaths: [] } }
  }, path)
  const picked = () => app.evaluate(() => globalThis.__picked)
  const section = page.locator('#panel-folders')
  const rows = section.locator('.folder-row')
  const add = section.locator('[data-test="folders-add"]')
  const problem = section.locator('[data-test="folders-problem"]')
  const shoot = async (state) => {
    await page.waitForTimeout(250)
    await page.screenshot({ path: join(output, `${prefix}folders-${state}.png`) })
  }
  const theme = async (name) => {
    await page.evaluate((chosen) => window.bravebot.writeTheme(chosen), name)
    await page.waitForFunction((chosen) => document.documentElement.dataset.theme === chosen, name)
  }

  // ---- the section, on the Files tab, says only the project is open -------------------------
  await page.getByRole('tab', { name: 'Files', exact: true }).click()
  await section.waitFor()
  await section.getByText('Only the project is open.').waitFor()
  assert.equal(await page.locator('[data-test="tree-roots"]').count(), 0, 'one root draws as it always has')
  await shoot('empty')

  // ---- a cancelled picker does nothing -------------------------------------------------------
  await pick(null)
  await add.click()
  for (let tries = 0; tries < 40 && (await picked()) === 0; tries++) await page.waitForTimeout(50)
  assert.equal(await picked(), 1, 'the picker was asked')
  await page.waitForTimeout(300)
  assert.equal(await rows.count(), 0)
  assert.equal(await problem.count(), 0, 'a cancel says nothing')

  // ---- an add lists the folder and the tree shows it as a second root -------------------------
  await pick(beside)
  await add.click()
  await rows.first().waitFor()
  assert.equal(await rows.count(), 1)
  assert.equal((await rows.first().locator('.folder-name').innerText()).trim(), 'beside')
  assert.equal(await rows.first().locator('.folder-text').getAttribute('data-tooltip'), beside)
  const roots = page.locator('[data-test="tree-root-item"]')
  await roots.nth(1).waitFor()
  assert.deepEqual((await roots.locator(':scope > .tree-root-row .tree-name').allInnerTexts()).map((s) => s.trim()), ['project', 'beside'])
  // The folder's own entries were read under it, not under the project.
  await roots.nth(1).getByText('guide.md').waitFor()
  await roots.nth(0).getByText('README.md').waitFor()
  await shoot('added')
  await theme('dark')
  await shoot('added-dark')
  await theme('light')

  // ---- the same folder again is refused, in the section --------------------------------------
  await add.click()
  await problem.waitFor()
  assert.match(await problem.innerText(), /already/i)

  // ---- closing takes it out of the list and the tree ----------------------------------------
  await rows.first().locator('[data-test="folder-close"]').click()
  await section.getByText('Only the project is open.').waitFor()
  await page.locator('[data-test="tree-roots"]').waitFor({ state: 'detached' })
  assert.equal(await problem.count(), 0, 'a close that went through clears the last refusal')
  await shoot('closed')

  // ---- a folder inside the project is refused, and nothing is listed --------------------------
  await pick(join(project, 'src'))
  await add.click()
  await problem.waitFor()
  const refusal = await problem.innerText()
  assert.ok(refusal.trim().length > 0, 'the refusal is said')
  assert.equal(await rows.count(), 0)
  await shoot('refused')
  await pick(project)
  await add.click()
  await page.waitForFunction((before) => document.querySelector('[data-test="folders-problem"]')?.textContent !== before, refusal)
  await shoot('refused-project')
  await theme('dark')
  await shoot('refused-dark')
  await theme('light')

  // ---- the More menu offers Add folder ------------------------------------------------------
  await page.locator('[data-test="conversation-more"]').click()
  const item = page.locator('[data-test="add-folder"]')
  await item.waitFor()
  assert.equal(await item.getAttribute('aria-disabled'), null)
  await shoot('menu')
  await pick(beside)
  await item.click()
  await rows.first().waitFor()
  assert.equal(await rows.count(), 1, 'the menu adds as the section does')

  // ---- while a turn runs, Add and Close are shut and say why --------------------------------
  let release
  held = { promise: new Promise((resolve) => { release = resolve }) }
  await page.getByRole('textbox', { name: COMPOSER_NAME }).fill('Hold on.')
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  // A Leo button holds its shut state as a property, not an attribute.
  const shut = (want) => page.waitForFunction((value) => ['folders-add', 'folder-close'].every((name) => {
    const host = document.querySelector(`#panel-folders [data-test="${name}"]`)
    return host && Boolean(host.isDisabled) === value
  }), want)
  await shut(true)
  assert.equal(await add.getAttribute('data-tooltip'), 'Wait for the turn to finish')
  assert.equal(await rows.first().locator('[data-test="folder-close"]').getAttribute('data-tooltip'), 'Wait for the turn to finish')
  await add.hover({ force: true })
  await page.locator('.tooltip-text', { hasText: 'Wait for the turn to finish' }).waitFor()
  await shoot('running')
  await page.mouse.move(0, 0)
  await page.locator('[data-test="conversation-more"]').click()
  await item.waitFor()
  assert.equal(await item.getAttribute('aria-disabled'), 'true')
  await shoot('menu-running')
  await page.keyboard.press('Escape')
  held = null
  release()
  await shut(false)

  assert.deepEqual(errors, [], 'the window raised no errors')
  console.log('drive-folders: ok')
} finally {
  await app?.close().catch(() => undefined)
  service.close()
  // The bridge may still be writing its record as the app goes, so the removal is retried.
  rmSync(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 })
}
