// That System / Light / Dark can be chosen and remembered.
import { _electron as electron } from 'playwright-core'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const shots = '/tmp/bravebot-ui'
mkdirSync(shots, { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
}

const naming = await electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })
const userData = await naming.evaluate(({ app }) => app.getPath('userData'))
await naming.close()

const stateFile = join(userData, 'bravebot-ui.json')

const readState = () => {
  try {
    return JSON.parse(readFileSync(stateFile, 'utf8'))
  } catch {
    return {}
  }
}

const hadTheme = readState().theme

const putTheme = (name) => {
  const state = readState()
  if (name === undefined) delete state.theme
  else state.theme = name
  try {
    writeFileSync(stateFile, `${JSON.stringify(state, null, 2)}\n`, 'utf8')
  } catch {
    // First launch may not have written the file yet.
  }
}

const restore = () => {
  try {
    putTheme(hadTheme)
  } catch {
    // Best-effort.
  }
}
process.on('exit', restore)

putTheme(undefined)

const open = async () => {
  const app = await electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })
  const page = await app.firstWindow({ timeout: 40000 })
  await page.waitForSelector('.app', { timeout: 40000 })
  return { app, page }
}

const appearanceAttr = async (page) =>
  page.evaluate(() => document.documentElement.getAttribute('data-theme'))

{
  const { app, page } = await open()
  check((await appearanceAttr(page)) === null, 'system leaves data-theme unset')
  await page.screenshot({ path: join(shots, 'appearance-system.png') })

  await page.evaluate(() => window.bravebot.writeTheme('dark'))
  await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') === 'dark', null, {
    timeout: 5000,
  })
  check((await appearanceAttr(page)) === 'dark', 'dark sets data-theme=dark')
  check(readState().theme === 'dark', 'dark is remembered in bravebot-ui.json')
  await page.screenshot({ path: join(shots, 'appearance-dark.png') })

  await page.evaluate(() => window.bravebot.writeTheme('light'))
  await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') === 'light', null, {
    timeout: 5000,
  })
  check((await appearanceAttr(page)) === 'light', 'light sets data-theme=light')
  check(readState().theme === 'light', 'light is remembered')

  await page.evaluate(() => window.bravebot.writeTheme('system'))
  await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') === null, null, {
    timeout: 5000,
  })
  check((await appearanceAttr(page)) === null, 'system clears data-theme again')
  check(readState().theme === 'system', 'system is remembered')

  putTheme('nord')
  await app.close()
}

{
  const { app, page } = await open()
  check((await appearanceAttr(page)) === null, 'legacy nord falls back to system painting')
  const chosen = await page.evaluate(async () => (await window.bravebot.readTheme()).chosen)
  check(chosen === 'system', 'readTheme normalises nord to system')
  await app.close()
}

if (problems.length) {
  console.error(`\n${problems.length} check(s) failed`)
  process.exit(1)
}
console.log('\nall appearance checks passed')
