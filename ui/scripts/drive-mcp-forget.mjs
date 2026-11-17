// A project's remembered MCP answers in the Permissions dialog: answer 2 at a server's question and
// at two tool calls' are listed, Forget drops them for this project and for no other, as
// `bravebot mcp forget` does in a terminal, and says what it dropped. A record that cannot be read
// is named and is neither listed as empty nor forgotten from.
//
// The real app and the real bridge, with a home of the script's own: the records are written into
// its state directory before the app starts, and read back off the disk after Forget. Nothing is
// asked of a model.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run drive:mcp-forget` does first. MCP_FORGET_SHOTS moves the screenshots.
import assert from 'node:assert/strict'
import { chmodSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject } from './drive-world.mjs'
import { SHOTS_DIR } from './drive-support.mjs'

const { root, home, project, profile, env } = makeWorld('bravebot-mcp-forget-', { baseURL: 'http://127.0.0.1:9/v1', dirs: ['elsewhere'] })
const elsewhere = join(root, 'elsewhere')
const state = join(home, '.bravebot')
const projects = join(state, 'mcp-projects'), tools = join(state, 'mcp-tools')
// Answer 2 at a server's question here and in another project, and at two calls here and one there.
writeFileSync(projects, `${project}\n${elsewhere}\n`)
writeFileSync(tools, `weather get_forecast ${project}\nweather alerts ${project}\nweather get_forecast ${elsewhere}\n`)

const output = process.env.MCP_FORGET_SHOTS ?? SHOTS_DIR
mkdirSync(output, { recursive: true })
const shot = (page, name) => page.screenshot({ path: join(output, `mcp-forget-${name}.png`) })

let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1200, height: 900 } }))
  await openProject(page, app, project)

  await page.getByRole('button', { name: 'More', exact: true }).click()
  await page.getByRole('menuitem', { name: 'Permissions…', exact: true }).click()
  const dialog = page.getByRole('dialog', { name: 'Conversation permissions', exact: true })
  const section = dialog.locator('[data-test="mcp-remembered"]')
  const forget = section.locator('[data-test="mcp-forget"]')
  await section.waitFor()
  const showSection = () => section.evaluate((element) => element.scrollIntoView({ block: 'start', behavior: 'instant' }))

  // ---- (a) what is remembered for this project, and only this project -----------------------
  await section.locator('[data-test="mcp-every-server"]').waitFor()
  const listed = (await section.locator('[data-test="mcp-tool"] code').allInnerTexts()).sort()
  assert.deepEqual(listed, ['weather:alerts', 'weather:get_forecast'], 'the two tools answered here are listed')
  assert.ok((await section.innerText()).includes(project), 'the note names the project')
  assert.notEqual(await forget.getAttribute('aria-disabled'), 'true', 'Forget is open with something to forget')
  await showSection()
  await shot(page, 'listed')

  // ---- (c) Forget, and what it dropped -------------------------------------------------------
  await forget.click()
  const forgotten = section.locator('[data-test="mcp-forgotten"]')
  await forgotten.waitFor()
  const said = await forgotten.innerText()
  assert.ok(said.includes('Every server for this project'), said)
  assert.ok(said.includes('weather:alerts') && said.includes('weather:get_forecast'), said)
  assert.match(said, /already started keep running until it ends/)
  assert.match(said, /next call to a forgotten tool asks again/)
  await section.locator('[data-test="mcp-empty"]').waitFor()
  assert.equal(await forget.getAttribute('aria-disabled'), 'true', 'nothing is left to forget')
  assert.equal(readFileSync(projects, 'utf8'), `${elsewhere}\n`, 'the other project keeps its server answer')
  assert.equal(readFileSync(tools, 'utf8'), `weather get_forecast ${elsewhere}\n`, 'the other project keeps its tool answer')
  await showSection()
  await shot(page, 'forgotten')

  // ---- (b) nothing remembered, read afresh ---------------------------------------------------
  await dialog.locator('[data-test="permissions-refresh"]').click()
  await forgotten.waitFor({ state: 'detached' })
  await section.locator('[data-test="mcp-empty"]').waitFor()
  assert.equal(await forget.getAttribute('aria-disabled'), 'true')
  await showSection()
  await shot(page, 'empty')

  // ---- (d) a record that cannot be read ------------------------------------------------------
  writeFileSync(tools, Buffer.from([0xff, 0xfe, 0x0a]))
  await dialog.locator('[data-test="permissions-refresh"]').click()
  const unreadable = section.locator('[data-test="mcp-unreadable"]')
  await unreadable.waitFor()
  assert.ok((await unreadable.innerText()).includes(tools), 'the record that cannot be read is named')
  assert.equal(await section.locator('[data-test="mcp-empty"]').count(), 0, 'it is not drawn as nothing remembered')
  assert.equal(await forget.count(), 0, 'and offers no Forget')
  await showSection()
  await shot(page, 'unreadable')

  // ---- a record that cannot be written: the bridge refuses, in its words, beside the button ---
  writeFileSync(tools, `weather alerts ${project}\n`)
  await dialog.locator('[data-test="permissions-refresh"]').click()
  await section.locator('[data-test="mcp-tool"]').waitFor()
  chmodSync(state, 0o555)
  try {
    await forget.click()
    const refused = section.locator('[data-test="mcp-forget-error"]')
    await refused.waitFor()
    assert.match(await refused.innerText(), /could not be written/)
    assert.equal(readFileSync(tools, 'utf8'), `weather alerts ${project}\n`, 'a refused Forget leaves the record as it was')
    await showSection()
    await shot(page, 'refused')
  } finally {
    chmodSync(state, 0o755)
  }

  assert.deepEqual(errors, [])
  console.log(`PASS: remembered MCP answers are listed, forgotten for this project only, an unreadable record is named, and a refused Forget is said. Screenshots in ${output}/mcp-forget-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'mcp-forget-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  rmSync(root, { recursive: true, force: true })
}
