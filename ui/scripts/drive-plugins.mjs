// That the Plugins page in the settings lists what the terminal's `/plugin add` installed, turns a
// plugin off and on by rewriting only `enabled` in its record, and removes one
// (docs/specs/plugins.md PLUGIN-10, PLUGIN-11). Then that it installs and updates one: a git
// source's host is asked about before any git starts (PLUGIN-3), the install and update sheets list
// the plugin's parts inside the untrusted margin with every argv word whole, the yes is offered only
// once every row is read, a no or Esc writes nothing, and a yes installs or updates (PLUGIN-4,
// PLUGIN-9).
//
// The real app and the real bridge, against a home of this script's own. The plugins are laid out
// as `/plugin add` leaves them: `<name>/` and `<name>.record` under `~/.bravebot/plugins`, the
// record in the words `commit_install` writes. The installed one is a folder this script writes.
// Nothing is fetched and nothing is paid for.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run drive:plugins` does first.
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld } from './drive-world.mjs'
import { SHOTS_DIR } from './drive-support.mjs'

const { root, home, profile, env } = makeWorld('bravebot-plugins-', { baseURL: 'http://127.0.0.1:9/v1', project: false })
const plugins = join(home, '.bravebot/plugins')
const COMMIT = '4f1c2a9e0b7d3c5a6e8f9d0b1a2c3e4f5a6b7c8d'
// 64 hex digits, as `commit_install` records a digest.
const DIGEST = '9b74c9897bac770ffc029102a200c5de2f1d1a6b0f5c7e3a9e6d4b2c1a0f9e8d'
const LOCAL_DIGEST = '0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f9'
const installed = (name, record) => {
  mkdirSync(join(plugins, name, 'skills/review'), { recursive: true })
  writeFileSync(join(plugins, name, 'plugin.toml'), `name = "${name}"\nversion = "2.3.4"\n`)
  writeFileSync(join(plugins, name, 'skills/review/SKILL.md'), '---\nname: review\ndescription: Review a change\n---\nRead the diff.\n')
  writeFileSync(join(plugins, `${name}.record`), record)
}
const gitRecord = `source = https://example.com/acme/reviewer.git\nrequested-ref = v2.3.4\ncommit = ${COMMIT}\nversion = 2.3.4\nenabled = true\ndigest = ${DIGEST}\n`
const localRecord = `source = ${join(root, 'src/notes')}\nversion = 0.1.0\nenabled = true\ndigest = ${LOCAL_DIGEST}\n`
installed('reviewer', gitRecord)
installed('notes', localRecord)
const record = (name) => readFileSync(join(plugins, `${name}.record`), 'utf8')

const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? SHOTS_DIR
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 1000 } }))
  page.on('dialog', (dialog) => void dialog.accept())
  const shoot = async (name) => {
    for (const theme of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme: theme, reducedMotion: 'reduce' })
      await page.waitForTimeout(300)
      await page.screenshot({ path: join(output, `plugins-${name}-${theme}.png`) })
    }
    await page.emulateMedia({ colorScheme: null, reducedMotion: 'reduce' })
  }

  // ---- the list, as the records say -------------------------------------------------------------
  await page.locator('[data-test="agent-settings"]').click()
  await page.locator('[data-test="settings-page-plugins"]').click()
  const view = page.locator('[data-test="plugins"]')
  const reviewer = view.locator('[data-test="plugin-reviewer"]')
  const notes = view.locator('[data-test="plugin-notes"]')
  await reviewer.waitFor()
  assert.equal(await view.locator('.plugin-row').count(), 2)
  assert.equal((await reviewer.locator('[data-test="plugin-name"]').innerText()).trim(), 'reviewer')
  assert.equal((await reviewer.locator('[data-test="plugin-version"]').innerText()).trim(), '2.3.4')
  assert.equal(await reviewer.locator('[data-test="plugin-source"]').innerText(), 'https://example.com/acme/reviewer.git')
  assert.equal(await reviewer.locator('[data-test="plugin-ref"]').innerText(), 'v2.3.4')
  assert.equal(await reviewer.locator('[data-test="plugin-commit"]').innerText(), COMMIT)
  assert.equal(await reviewer.locator('[data-test="plugin-digest"]').innerText(), DIGEST)
  assert.equal(await notes.locator('[data-test="plugin-commit"]').count(), 0, 'a local source has no commit')
  assert.equal(await notes.locator('[data-test="plugin-source"]').innerText(), join(root, 'src/notes'))
  assert.equal(await reviewer.locator('[data-test="plugin-enabled"]').getAttribute('checked'), '', 'an installed plugin is on')
  await shoot('list')

  // ---- off and on again: only `enabled` changes ---------------------------------------------------
  await reviewer.locator('[data-test="plugin-enabled"] button').click()
  await view.locator('[data-test="plugins-status"]').filter({ hasText: 'reviewer is off' }).waitFor()
  assert.equal(record('reviewer'), gitRecord.replace('enabled = true', 'enabled = false'))
  assert.equal(record('notes'), localRecord, 'the other record is untouched')
  assert.equal(await reviewer.locator('[data-test="plugin-enabled"]').getAttribute('checked'), null, 'the switch shows it off')
  await shoot('off')
  await reviewer.locator('[data-test="plugin-enabled"] button').click()
  await view.locator('[data-test="plugins-status"]').filter({ hasText: 'reviewer is on' }).waitFor()
  assert.equal(record('reviewer'), gitRecord)

  // ---- removing one ---------------------------------------------------------------------------------
  await notes.locator('[data-test="plugin-remove"]').click()
  await view.locator('[data-test="plugins-status"]').filter({ hasText: 'notes was removed' }).waitFor()
  assert.equal(await notes.count(), 0)
  assert.ok(!existsSync(join(plugins, 'notes')), 'its directory is gone')
  assert.ok(!existsSync(join(plugins, 'notes.record')), 'its record is gone')
  assert.equal(record('reviewer'), gitRecord, 'the other plugin stays')
  await reviewer.locator('[data-test="plugin-remove"]').click()
  await view.locator('[data-test="plugins-empty"]').waitFor()
  assert.equal(await view.locator('[data-test="plugins-empty"]').innerText(), 'No plugins are installed. Add one below.')
  assert.equal(await view.locator('[data-test="plugin-add"]').isVisible(), true, 'the install form stays with no plugin installed')
  await shoot('empty')

  // ---- a git source: the host is asked about before any git starts, and a no writes nothing ---------
  const source = view.locator('[data-test="plugin-add-source"] input')
  const status = view.locator('[data-test="plugins-status"]')
  const yes = (name) => page.getByRole('button', { name, exact: true })
  await source.fill('https://example.com/acme/greeter.git')
  await view.locator('[data-test="plugin-add-submit"]').click()
  const hostQuestion = page.locator('[data-test="plugin-host-question"]')
  await hostQuestion.waitFor()
  assert.equal(await hostQuestion.locator('[data-test="plugin-host"]').innerText(), 'example.com')
  await shoot('host')
  await page.getByRole('button', { name: 'Don’t fetch', exact: true }).click()
  await hostQuestion.waitFor({ state: 'detached' })
  await status.filter({ hasNotText: 'Reading' }).waitFor()
  assert.deepEqual(readdirSync(plugins), [], 'a no to the host writes nothing')

  // ---- a local fixture: the listing is drawn inside the untrusted margin ---------------------------
  const fixture = join(root, 'src-greeter')
  const description = `Greets the person at the start of a turn. ${'It says hello in the words the person chose, and nothing else. '.repeat(5)}The end of the description.`
  const writeFixture = (version, skill) => {
    mkdirSync(join(fixture, 'skills/greet'), { recursive: true })
    mkdirSync(join(fixture, 'hooks'), { recursive: true })
    writeFileSync(join(fixture, 'plugin.toml'), `name = "greeter"\nversion = "${version}"\ndescription = "d"\nskills = "skills"\nhooks = "hooks.json"\n`)
    writeFileSync(join(fixture, 'skills/greet/SKILL.md'), `---\nname: greet\ndescription: ${skill}\n---\n\nSay hello.\n`)
    writeFileSync(join(fixture, 'hooks.json'), JSON.stringify({ hooks: [{ on: 'turn-started', run: ['${PLUGIN_ROOT}/hooks/greet.sh', '--quiet', 'two words'] }] }))
    writeFileSync(join(fixture, 'hooks/greet.sh'), '#!/bin/sh\necho hello\n')
  }
  writeFixture('2.3.4', description)
  const sheet = page.locator('[data-test="plugin-sheet"]')
  /** Bring every row on screen and open every cut description, as a person reading it would. */
  const readAll = async (button) => {
    for (const more of await sheet.locator('[data-test="plugin-show-more"]').all()) await more.click()
    for (const row of await sheet.locator('[data-test="plugin-sheet-row"]').all()) await row.scrollIntoViewIfNeeded()
    for (let tries = 0; tries < 50 && await yes(button).isDisabled(); tries++) await page.waitForTimeout(100)
    assert.equal(await yes(button).isDisabled(), false, 'every row read, the yes is offered')
  }

  await source.fill(fixture)
  await view.locator('[data-test="plugin-add-submit"]').click()
  await sheet.waitFor()
  assert.equal(await sheet.locator('[data-test="plugin-sheet-version"]').innerText(), '2.3.4')
  assert.equal(await sheet.locator('[data-test="plugin-skill"]').innerText(), 'greet')
  const argv = (await sheet.locator('[data-test="plugin-argv"] li').allInnerTexts()).map((word) => word.trim())
  assert.deepEqual(argv, [join(plugins, 'greeter/hooks/greet.sh'), '--quiet', 'two words'], 'each argv word whole, ${PLUGIN_ROOT} expanded')
  assert.ok((await sheet.locator('[data-test="plugin-sheet-description"]').innerText()).length < description.length, 'a long description is cut')
  assert.equal(await yes('Yes, install').isDisabled(), true, 'no yes before the cut description is shown')
  await page.waitForTimeout(300)
  const hint = await page.locator('[data-test="plugin-reading"]').innerText()
  assert.match(hint, /^\d+ of \d+ rows read\. 1 description is cut short\. Choose Show more/, `the hint names the cut description: ${hint}`)
  const cutText = (await sheet.locator('[data-test="plugin-sheet-description"]').innerText()).replace(/…$/, '')
  assert.ok(description.startsWith(cutText) && /\s/.test(description[cutText.length]), `a cut description ends at a word: ${cutText}`)
  await shoot('install')
  await page.keyboard.press('Escape')
  await sheet.waitFor({ state: 'detached' })
  await status.filter({ hasNotText: 'Reading' }).waitFor()
  assert.deepEqual(readdirSync(plugins), [], 'Esc is a no, and a no writes nothing')

  // ---- accepted, from the folder picker ----------------------------------------------------------
  await app.evaluate(({ dialog }, path) => { dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] }) }, fixture)
  await view.locator('[data-test="plugin-choose-folder"]').click()
  await sheet.waitFor()
  await readAll('Yes, install')
  assert.equal(await sheet.locator('[data-test="plugin-sheet-description"]').innerText(), description, 'Show more draws the whole description')
  await yes('Yes, install').click()
  const greeter = view.locator('[data-test="plugin-greeter"]')
  await greeter.waitFor()
  assert.equal((await greeter.locator('[data-test="plugin-version"]').innerText()).trim(), '2.3.4')
  assert.ok(existsSync(join(plugins, 'greeter.record')) && existsSync(join(plugins, 'greeter/plugin.toml')), 'a yes writes the plugin and its record')
  assert.match((await status.innerText()).trim(), /^Installed greeter from .+\.$/, 'the job’s sentence is shown, ended with a stop')
  await shoot('installed')

  // ---- updated after the fixture changes -----------------------------------------------------------
  const before = record('greeter')
  writeFixture('2.4.0', 'Greets the person, now in two languages.')
  writeFileSync(join(fixture, 'skills/greet/fr.md'), 'Bonjour.\n')
  await greeter.locator('[data-test="plugin-update"]').click()
  await sheet.waitFor()
  assert.equal((await sheet.locator('[data-test="plugin-sheet-versions"]').innerText()).replace(/\s+/g, ' ').trim(), '2.3.4 to 2.4.0')
  const files = await sheet.locator('[data-test="plugin-file"]').allInnerTexts()
  for (const file of ['plugin.toml', 'skills/greet/SKILL.md', 'skills/greet/fr.md']) assert.ok(files.includes(file), `${file} is listed as differing: ${files}`)
  await page.waitForTimeout(300)
  // The sheet keeps a margin from the window's edges, and only its body scrolls.
  const frame = await page.evaluate(() => {
    const box = document.querySelector('.plugin-question')?.shadowRoot?.querySelector('dialog')?.getBoundingClientRect()
    const body = document.querySelector('[data-test="plugin-sheet-scroll"]')
    return box && body && { top: box.top, bottom: box.bottom, height: innerHeight, bodyMax: getComputedStyle(body).maxHeight, overflow: getComputedStyle(body).overflowY }
  })
  assert.ok(frame && frame.top >= 24 && frame.bottom <= frame.height - 24, `the sheet stays inside the window: ${JSON.stringify(frame)}`)
  assert.equal(frame.overflow, 'auto')
  await shoot('update')
  await page.getByRole('button', { name: 'Keep it as it is', exact: true }).click()
  await sheet.waitFor({ state: 'detached' })
  await status.filter({ hasNotText: 'Reading' }).waitFor()
  assert.equal(record('greeter'), before, 'a no keeps the record as it was')
  await greeter.locator('[data-test="plugin-update"]').click()
  await sheet.waitFor()
  await readAll('Yes, update')
  await yes('Yes, update').click()
  await greeter.locator('[data-test="plugin-version"]').filter({ hasText: '2.4.0' }).waitFor()
  assert.match(record('greeter'), /version = 2\.4\.0/)
  await shoot('updated')

  // ---- removed -------------------------------------------------------------------------------------
  await greeter.locator('[data-test="plugin-remove"]').click()
  await view.locator('[data-test="plugins-empty"]').waitFor()
  assert.deepEqual(readdirSync(plugins), [], 'Remove deletes the directory and the record')

  assert.deepEqual(errors, [])
  console.log(`PASS: the Plugins page lists, switches and removes plugins, asks about a git host first, lists a local plugin inside the untrusted margin, writes nothing on a no, installs and updates on a yes once every row is read, and removes it. Screenshots in ${output}/plugins-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'plugins-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  rmSync(root, { recursive: true, force: true })
}
