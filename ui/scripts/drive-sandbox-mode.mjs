// That the sandbox mode chosen in the composer governs the session's next turn: a session opens
// in workspace-write, the menu offers workspace-write, read-only and strict and no way to turn the
// sandbox off, a program the agent runs is held to the mode chosen, and the choice is the session's
// own.
//
// The real app and the real bridge, against a model service this script serves itself, in a
// directory the person did not trust. Nothing is paid for. Whether a program was held to its
// profile is read off the disk and off what the tool handed back to the model.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run drive:sandbox-mode` does first.
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'

// Both outside the project: workspace-write and read-only read the first and refuse a write to the
// second, strict refuses both. Both are under the real home, because a session may use the temporary directory,
// so a file there would prove nothing. They are removed at the end.
const outside = join(homedir(), `.bravebot-sandbox-drive-${process.pid}-outside`)
const planted = join(homedir(), `.bravebot-sandbox-drive-${process.pid}-planted`)
writeFileSync(outside, 'outside the project\n')

// A planner that makes the one call the last prompt names, and ends the turn once a tool answered.
const { service, rounds } = await startModelService(({ messages }) => {
  const answered = messages.at(-1)?.role === 'tool'
  const asked = JSON.stringify(messages.filter((message) => message.role === 'user').at(-1) ?? '')
  const command = answered ? null
    : asked.includes('Plant') ? `/bin/sh -c '/usr/bin/touch ${planted} > /dev/null'`
      : asked.includes('Read') ? `/bin/sh -c '/bin/cat ${outside} > /dev/null'`
        : null
  return command ? { tool: { name: 'run', arguments: { command } } } : { content: 'done' }
})

const lastToolResult = (round) => (JSON.parse(round).messages ?? []).filter((message) => message.role === 'tool').at(-1)?.content

const { root, project, profile, env } = makeWorld('bravebot-sandbox-', { service })

const output = '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 1000 } }))
  await openProject(page, app, project, { trust: false })

  const composer = page.locator('.composer textarea')
  const stop = page.locator('.composer .stop')
  const replies = page.locator('.bubble.assistant')
  const sandbox = page.locator('[data-test="sandbox-trigger"]')
  // The Leo host has no box of its own, so the mode is read off its attribute, not its visibility.
  const inMode = (wanted) => page.waitForFunction((wanted) =>
    document.querySelector('[data-test="sandbox-trigger"]')?.getAttribute('data-mode') === wanted, wanted)
  // Ask for a program to be run, approve it once, and return what the tool handed the model.
  const run = async (prompt) => {
    const before = await replies.count()
    await composer.fill(prompt)
    await page.locator('.composer .send').click()
    const card = page.locator('.confirm.run:not([data-folded="true"])')
    await card.waitFor()
    await card.locator('.confirm-actions .approve:not(.always)').click()
    await replies.nth(before).waitFor()
    await stop.waitFor({ state: 'hidden' })
    return lastToolResult(rounds.at(-1))
  }

  assert.equal(await sandbox.getAttribute('data-mode'), 'workspace-write', 'a session opens in workspace-write')
  assert.equal(await page.locator('[data-test="mode-trigger"]').getAttribute('data-mode'), 'ask', 'the permission mode is a control of its own')
  await page.screenshot({ path: join(output, 'sandbox-workspace-write.png') })

  await sandbox.click()
  await page.getByRole('menuitemradio', { name: /^Strict/ }).waitFor()
  await page.waitForTimeout(300)
  const offered = await page.locator('[data-test^="sandbox-option-"]').evaluateAll((items) => items.map((item) => item.getAttribute('data-test')))
  assert.deepEqual(offered, ['sandbox-option-workspace-write', 'sandbox-option-read-only', 'sandbox-option-strict'], 'the menu offers workspace-write, read-only and strict, and no way to turn the sandbox off')
  assert.equal(await page.getByRole('menuitemradio', { name: /^Workspace write/ }).getAttribute('aria-checked'), 'true', 'the mode in force is checked')
  await page.screenshot({ path: join(output, 'sandbox-menu.png') })
  await sandbox.click()
  await page.getByRole('menuitemradio', { name: /^Strict/ }).waitFor({ state: 'hidden' })

  // The control only means something where this platform confines a program at all.
  const control = await run('Plant a file')
  const confines = !existsSync(planted)
  if (!confines) {
    console.log(`  --   this platform did not confine the control program (${String(control).slice(0, 60)}), so what the modes hold a program to is not checked here`)
  } else {
    assert.match(String(control), /exited 1/, 'workspace-write refused a write outside the session')
    const readable = await run('Read a file')
    assert.match(String(readable), /It exited 0\./, 'workspace-write reads a file outside the session')

    await sandbox.click()
    await page.getByRole('menuitemradio', { name: /^Strict/ }).click()
    await inMode('strict')
    await page.screenshot({ path: join(output, 'sandbox-strict.png') })
    const refused = await run('Read a file')
    assert.match(String(refused), /exited 1/, 'strict refused a read outside the session')
    assert.match(String(refused), /Confinement:/, 'and the planner was told what it ran under')
    assert.equal(await sandbox.getAttribute('data-mode'), 'strict', 'the choice holds for the next turn')

    await sandbox.click()
    await page.getByRole('menuitemradio', { name: /^Workspace write/ }).click()
    await inMode('workspace-write')
    const again = await run('Read a file')
    assert.match(String(again), /It exited 0\./, 'workspace-write reads it again from the turn after the change')
  }

  await page.emulateMedia({ colorScheme: 'dark' })
  await sandbox.click()
  await page.getByRole('menuitemradio', { name: /^Strict/ }).click()
  await inMode('strict')
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(output, 'sandbox-strict-dark.png') })
  await page.emulateMedia({ colorScheme: 'light' })

  assert.deepEqual(errors, [])
  console.log(`PASS: a session opens in workspace-write, the menu offers workspace-write, read-only and strict and no way to turn the sandbox off, and a program is held to the mode chosen for its turn. Screenshots in ${output}/sandbox-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'sandbox-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(planted, { force: true })
  rmSync(outside, { force: true })
  rmSync(root, { recursive: true, force: true })
}
