// The questions a chat asks once its trust question is answered, about what the checkout's
// settings file asks for: whether to grant the allow rules it proposes (PERM-15), then whether to
// open each directory it names (TRUST-9).
//
// Two runs, each in a world of its own. In the first the project proposes a short list of rules,
// which fits and so counts as read at once, and names two directories: the rules are granted, the
// first directory opened and the second not, and the note and the Folders section say so. In the
// second the project proposes a long list: Grant stays shut until the list has been scrolled
// through, the rules are declined, the one directory is not opened, and the note says what is not
// in force.
//
// The real app and the real bridge, against a model service this script serves itself, under a
// temporary HOME: nothing here reads or writes the real ~/.bravebot. Needs `bravebot-rpc` built
// (`npm run bridge`) and the app built (`electron-vite build`), which `npm run
// drive:project-questions` does first. Screenshots go to QUESTIONS_OUTPUT, or /tmp/bravebot-ui,
// named QUESTIONS_PREFIX + `project-questions-` + state.
import assert from 'node:assert/strict'
import { mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_NAME, SHOTS_DIR, readTrust } from './drive-support.mjs'

const { service } = await startModelService(async () => ({ content: 'done' }))
const output = process.env.QUESTIONS_OUTPUT || SHOTS_DIR
const prefix = process.env.QUESTIONS_PREFIX ?? ''
mkdirSync(output, { recursive: true })

/** A Leo button holds its shut state as a property, not an attribute. */
const shut = (page, test, want) => page.waitForFunction(([name, value]) => {
  const host = document.querySelector(`[data-test="${name}"]`)
  return host && Boolean(host.isDisabled) === value
}, [test, want])

/**
 * Open a project whose settings file proposes `allow` and names `directories`, answer its trust
 * question, and hand the window to `body`.
 */
async function world(name, { allow, directories }, body) {
  const { root, project, profile, env } = makeWorld(`bravebot-questions-${name}-`, {
    service,
    dirs: ['project/.bravebot', ...directories.map((directory) => directory.replace(/^\.\.\//, ''))],
  })
  writeFileSync(join(project, 'README.md'), '# Project\n')
  writeFileSync(join(project, '.bravebot/settings.json'), JSON.stringify({ permissions: { allow, additionalDirectories: directories } }))
  for (const directory of directories) writeFileSync(join(project, directory, 'notes.md'), '# Notes\n')
  let app, page, errors
  try {
    ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 900 } }))
    const shoot = async (state) => {
      await page.waitForTimeout(300)
      await page.screenshot({ path: join(output, `${prefix}project-questions-${state}.png`) })
    }
    const theme = async (chosen) => {
      // The app's own theme setting, in this world's profile: the machine's appearance is left alone.
      await page.evaluate((value) => window.bravebot.writeTheme(value), chosen)
      await page.waitForFunction((value) => document.documentElement.dataset.theme === value, chosen)
    }
    await openProject(page, app, project)
    await body({ page, project, root, shoot, theme })
    assert.deepEqual(errors, [], 'the window raised no errors')
  } finally {
    await app?.close().catch(() => undefined)
    // The bridge may still be writing its record as the app goes, so the removal is retried.
    rmSync(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 })
  }
}

const rulesQuestion = (page) => page.getByRole('dialog', { name: 'Project rules', exact: true })
const folderQuestion = (page) => page.getByRole('dialog', { name: 'Open a folder', exact: true })
const waiting = (page) => page.locator('[data-test="question-waiting"]')

/**
 * Open the folded note above the chat and return it. The usage ping's notice and the crash
 * reports' notice are put away first, in this world's profile, so the transcript has the room to
 * show the note. The crash reports' notice is drawn only by a bridge built with a submission
 * address, so it is usually absent here.
 */
async function unfoldNote(page) {
  for (const name of ['usage-ping-notice', 'crash-reports-notice']) {
    const dismiss = page.locator(`[data-test="${name}-dismiss"]`)
    if (await dismiss.isVisible().catch(() => false)) {
      await dismiss.click()
      await page.locator(`[data-test="${name}"]`).waitFor({ state: 'detached' })
    }
  }
  const note = page.locator('[data-test="rules-note"]')
  await note.waitFor()
  if (!(await note.evaluate((element) => element.open))) await note.locator('summary').click()
  return note
}

// ---- a short list, granted; one folder opened and one not ---------------------------------------
await world('short', { allow: ['Bash(cargo test)', 'Edit(src/**)'], directories: ['../shared', '../other'] }, async ({ page, root, shoot }) => {
  const rules = rulesQuestion(page)
  await rules.waitFor()
  assert.equal(await rules.locator('[data-test="rules-row"]').count(), 2)
  assert.match(await rules.locator('[data-test="rules-keeping"]').innerText(), /kept in/)
  // A list that fits is read at once: the grant opens with no scrolling.
  await shut(page, 'rules-grant', false)
  assert.match(await waiting(page).innerText(), /rules/, 'the composer says what it waits on')
  await shoot('short-rules')

  await rules.locator('[data-test="rules-grant"]').click()
  const folder = folderQuestion(page)
  await folder.waitFor()
  assert.equal((await folder.locator('[data-test="folder-path"]').innerText()).trim(), join(root, 'shared'))
  assert.match(await folder.locator('[data-test="folder-remaining"]').innerText(), /1 more folder/)
  await shut(page, 'folder-open', false)
  assert.match(await waiting(page).innerText(), /folder/)
  await shoot('folder')

  await folder.locator('[data-test="folder-open"]').click()
  await page.waitForFunction((path) => document.querySelector('[data-test="folder-path"]')?.textContent?.trim() === path, join(root, 'other'))
  assert.equal(await folder.locator('[data-test="folder-remaining"]').count(), 0, 'the last folder has none after it')
  await shut(page, 'folder-decline', false)
  await folder.locator('[data-test="folder-decline"]').click()
  await folder.waitFor({ state: 'hidden' })
  await waiting(page).waitFor({ state: 'detached' })

  const note = await unfoldNote(page)
  const granted = note.locator('[data-test="rules-note-granted"]')
  await granted.waitFor()
  assert.match(await granted.innerText(), /Bash\(cargo test\)/)
  assert.match(await granted.innerText(), /kept in/)
  assert.equal(await note.locator('[data-test="rules-note-proposed"]').count(), 0, 'a granted rule is not said not to be in force')
  assert.match(await note.locator('[data-test="rules-note-directories"]').innerText(), /other/, 'the folder said no to is still not open')

  await page.getByRole('tab', { name: 'Files', exact: true }).click()
  const rows = page.locator('#panel-folders .folder-row')
  await rows.first().waitFor()
  assert.equal(await rows.count(), 1)
  assert.equal((await rows.first().locator('.folder-name').innerText()).trim(), 'shared')
  await page.locator('[data-test="tree-root-item"]').nth(1).getByText('notes.md').waitFor()
  await note.locator('[data-test="rules-note-directories"]').scrollIntoViewIfNeeded()
  await shoot('granted')

  // The chat sends now.
  await page.getByRole('textbox', { name: COMPOSER_NAME }).fill('Hello.')
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await page.getByText('done', { exact: true }).waitFor()
})

// ---- a long list: Grant waits for the scroll; declined; the folder not opened -----------------
const many = Array.from({ length: 36 }, (_, index) => `Bash(make step-${String(index + 1).padStart(2, '0')})`)
await world('long', { allow: many, directories: ['../shared'] }, async ({ page, shoot, theme }) => {
  const rules = rulesQuestion(page)
  await rules.waitFor()
  assert.equal(await rules.locator('[data-test="rules-row"]').count(), many.length)
  const list = rules.locator('[data-test="rules-list"]')
  assert.ok(await list.evaluate((element) => element.scrollHeight > element.clientHeight + 1), 'the long list scrolls in its own box')
  await page.waitForTimeout(400)
  await shut(page, 'rules-grant', true)
  await shoot('long-unread')

  await readTrust(rules)
  // Scrolled through and back to the top: every row has been on screen, and the grant opens.
  await shut(page, 'rules-grant', false)
  await list.evaluate((element) => { element.scrollTop = element.scrollHeight })
  await page.waitForTimeout(200)
  await shoot('long-read')
  await theme('dark')
  await shoot('long-read-dark')
  await theme('light')

  await rules.locator('[data-test="rules-decline"]').click()
  const folder = folderQuestion(page)
  await folder.waitFor()
  await shut(page, 'folder-decline', false)
  await folder.locator('[data-test="folder-decline"]').click()
  await folder.waitFor({ state: 'hidden' })

  const note = await unfoldNote(page)
  assert.match(await note.locator('summary').innerText(), /37 permission settings are not in force/)
  assert.equal(await note.locator('[data-test="rules-note-proposed"] li').count(), many.length)
  assert.match(await note.locator('[data-test="rules-note-directories"]').innerText(), /shared/)
  assert.equal(await note.locator('[data-test="rules-note-granted"]').count(), 0)
  await note.locator('summary').scrollIntoViewIfNeeded()
  await shoot('declined')
})

service.close()
console.log('drive-project-questions: ok')
