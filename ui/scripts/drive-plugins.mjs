// That the Plugins page in the settings lists what the terminal's `/plugin add` installed, turns a
// plugin off and on by rewriting only `enabled` in its record, and removes one
// (docs/specs/plugins.md PLUGIN-10, PLUGIN-11).
//
// The real app and the real bridge, against a home of this script's own. The plugins are laid out
// as `/plugin add` leaves them: `<name>/` and `<name>.record` under `~/.bravebot/plugins`, the
// record in the words `commit_install` writes. Nothing is fetched and nothing is paid for.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run drive:plugins` does first.
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld } from './drive-world.mjs'
import { SHOTS_DIR } from './drive-support.mjs'

const { root, home, profile, env } = makeWorld('bravebot-plugins-', { baseURL: 'http://127.0.0.1:9/v1', project: false })
const plugins = join(home, '.bravebot/plugins')
const COMMIT = '4f1c2a9e0b7d3c5a6e8f9d0b1a2c3e4f5a6b7c8d'
const DIGEST = 'sha256:9b74c9897bac770ffc029102a200c5de2f1d1a6b0f5c7e3a9e6d4b2c1a0f9e8d'
const installed = (name, record) => {
  mkdirSync(join(plugins, name, 'skills/review'), { recursive: true })
  writeFileSync(join(plugins, name, 'plugin.toml'), `name = "${name}"\nversion = "2.3.4"\n`)
  writeFileSync(join(plugins, name, 'skills/review/SKILL.md'), '---\nname: review\ndescription: Review a change\n---\nRead the diff.\n')
  writeFileSync(join(plugins, `${name}.record`), record)
}
const gitRecord = `source = https://example.com/acme/reviewer.git\nrequested-ref = v2.3.4\ncommit = ${COMMIT}\nversion = 2.3.4\nenabled = true\ndigest = ${DIGEST}\n`
const localRecord = `source = ${join(root, 'src/notes')}\nversion = 0.1.0\nenabled = true\ndigest = sha256:0a1b\n`
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
  await shoot('empty')

  assert.deepEqual(errors, [])
  console.log(`PASS: the Plugins page lists the installed plugins from their records, turns one off and on by rewriting only enabled, and removes them. Screenshots in ${output}/plugins-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'plugins-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  rmSync(root, { recursive: true, force: true })
}
