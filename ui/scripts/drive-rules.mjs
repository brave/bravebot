// That the permission rules in the settings files govern a conversation in the window: a host a
// rule refuses is not fetched and not asked about, a host the person's own file allows is
// fetched without a question, and what a file wrote that is not in force is said.
//
// The real app and the real bridge, against a model service and a website this script serves
// itself. Nothing is paid for. Whether a fetch went out is read off the website.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run drive:rules` does first.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, listening, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_INPUT } from './drive-support.mjs'

// The website, under two names. The person's file allows one and refuses the other.
const reached = []
const site = createServer((request, response) => {
  reached.push(`${request.headers.host?.split(':')[0]} ${request.method} ${request.url}`)
  response.writeHead(200, { 'Content-Type': 'text/plain' })
  response.end('a page\n')
})
await listening(site)
const port = site.address().port
const allowed = `http://127.0.0.1:${port}/allowed`
const refused = `http://localhost:${port}/refused`

// The model service. A planner that fetches the address named in the last prompt, and ends the
// turn once a tool has answered.
const { service, rounds } = await startModelService(({ messages }) => {
  const answered = messages.at(-1)?.role === 'tool'
  const asked = JSON.stringify(messages.filter((message) => message.role === 'user').at(-1) ?? '')
  const url = asked.match(/http:\/\/[a-z0-9.]+:\d+\/[a-z]+/)?.[0]
  return answered || !url ? { content: 'done' } : { tool: { name: 'fetch_url', arguments: { url } } }
})

const toolResults = (round) => (JSON.parse(round).messages ?? []).filter((message) => message.role === 'tool').map((message) => message.content).join('\n')

const { root, project, profile, env } = makeWorld('bravebot-rules-', {
  service,
  dirs: ['project/.bravebot'],
  settings: {
    permissions: {
      allow: ['WebFetch(domain:127.0.0.1)'],
      deny: ['WebFetch(domain:localhost)', 'Fetchh(domain:localhost)'],
    },
  },
})
const checkout = join(project, '.bravebot/settings.json')
writeFileSync(checkout, JSON.stringify({
  permissions: { allow: ['WebFetch(domain:localhost)'], additionalDirectories: ['../other'] },
}))

const output = '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 1000 } }))
  await openProject(page, app, project)

  const composer = page.locator(COMPOSER_INPUT)
  const stop = page.locator('.composer .stop')
  const replies = page.locator('.bubble.assistant')
  // A turn the fixture answers at once can show and hide Stop between two polls, so a turn is
  // waited out by its reply.
  const ask = async (prompt) => {
    const before = await replies.count()
    await composer.fill(prompt)
    await page.locator('.composer .send').click()
    await replies.nth(before).waitFor()
    await stop.waitFor({ state: 'hidden' })
  }

  // ---- what is not in force is said as the conversation opens -------------------------------
  const banner = page.locator('.rules-banner')
  await banner.waitFor()
  assert.match(await banner.locator('summary').innerText(), /3 permission settings are not in force/)
  await banner.locator('summary').click()
  const said = await banner.innerText()
  assert.ok(said.includes('Fetchh(domain:localhost)'), 'the entry that is not a rule is named')
  assert.ok(said.includes('WebFetch(domain:localhost)') && said.includes(checkout), 'the project’s allow rule is named with its file')
  assert.ok(said.includes('You will still be asked'))
  assert.ok(said.includes('../other'), 'the directory a file named is said not to be open')
  await page.screenshot({ path: join(output, 'rules-banner.png') })
  await banner.locator('summary').click()

  // ---- a host the person's own file allows ----------------------------------------------------
  await ask(`Fetch ${allowed}`)
  assert.equal(await page.locator('.confirm.fetch').count(), 0, 'a host the person allowed was not asked about')
  assert.deepEqual(reached, ['127.0.0.1 GET /allowed'], 'and was fetched')

  // ---- a host a rule refuses, which the project's file tried to allow ---------------------------
  await ask(`Fetch ${refused}`)
  assert.equal(await page.locator('.confirm.fetch').count(), 0, 'a host a rule refuses was not asked about')
  assert.deepEqual(reached, ['127.0.0.1 GET /allowed'], 'and was not fetched')
  assert.match(toolResults(rounds.at(-1)), /refused/, 'and the planner was told a rule refused it')
  await page.screenshot({ path: join(output, 'rules-refused.png') })

  // ---- the rules in force, in the permissions dialog ----------------------------------------
  await page.getByRole('button', { name: 'More', exact: true }).click()
  await page.getByRole('menuitem', { name: 'Permissions…', exact: true }).click()
  const dialog = page.getByRole('dialog', { name: 'Conversation permissions', exact: true })
  await dialog.getByText('Rules from settings files', { exact: true }).waitFor()
  const listed = await dialog.innerText()
  assert.ok(listed.includes('WebFetch(domain:localhost)'), 'the rule that refuses is listed')
  assert.ok(listed.includes('WebFetch(domain:127.0.0.1)'), 'the rule that allows is listed')
  assert.ok(!listed.includes('Fetchh'), 'an entry that is not a rule is not listed as in force')
  assert.equal(await dialog.locator('.settings-rules button').count(), 0, 'a rule has nothing to press')
  await page.screenshot({ path: join(output, 'rules-dialog.png') })

  assert.deepEqual(errors, [])
  console.log(`PASS: what is not in force is said as the conversation opens, a host the person allowed is fetched unasked, a host a rule refuses is neither asked about nor fetched, and the rules in force are listed. Screenshots in ${output}/rules-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'rules-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  await new Promise((resolve) => site.close(resolve))
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
