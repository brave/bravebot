// That the window can be repainted, and that `brave` still costs nothing.
//
// The assertions worth making here are the ones stills cannot make. A screenshot of a themed
// window proves a colour arrived; it does not prove that moving the cursor previewed one *before*
// anything was written down, that Escape put back exactly what was there, that every one of the
// nineteen tokens still resolves after being run through a chain of `color-mix`, or that a palette
// saved to disk reaches a window that is already open. Those are what this checks.
//
// Like the other drivers it perturbs `bravebot-ui.json`, and it puts its own key back: the theme
// is remembered there beside somebody's columns. It also writes palettes into the `themes`
// directory next to that file, and removes them — and the directory, if it made it.
import { _electron as electron } from 'playwright-core'
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const shots = '/tmp/bravebot-ui'
mkdirSync(shots, { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
}

// Where the app keeps what it remembers, asked of the app rather than composed here: `userData`
// is Electron's to decide and differs by platform. A launch of its own because the choice has to
// be cleared *before* the first launch that is measured — the assertions below are about a window
// nobody has themed, and a name left in the file by an earlier run would be the theme it opened in.
const naming = await electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })
const userData = await naming.evaluate(({ app }) => app.getPath('userData'))
await naming.close()

const stateFile = join(userData, 'bravebot-ui.json')
const themesDirectory = join(userData, 'themes')
const driverTheme = join(themesDirectory, 'driver-test.json')
const brokenTheme = join(themesDirectory, 'driver-broken.json')
const paleTheme = join(themesDirectory, 'driver-pale.json')

const readState = () => {
  try {
    return JSON.parse(readFileSync(stateFile, 'utf8'))
  } catch {
    return {}
  }
}

// What was there before this ran, so it can be there after. Only this driver's own key, and only
// the palettes it wrote: the rest of that file is somebody's arrangement of this window.
const hadTheme = readState().theme
const hadThemes = existsSync(themesDirectory)

const putTheme = (name) => {
  const state = readState()
  if (name === undefined) delete state.theme
  else state.theme = name
  try {
    writeFileSync(stateFile, `${JSON.stringify(state, null, 2)}\n`, 'utf8')
  } catch {
    // The app has not written the file yet, which is a state it handles on every first launch.
  }
}

const restore = () => {
  try {
    putTheme(hadTheme)
    rmSync(driverTheme, { force: true })
    rmSync(brokenTheme, { force: true })
    rmSync(paleTheme, { force: true })
    if (!hadThemes) rmSync(themesDirectory, { recursive: true, force: true })
  } catch {
    // Best-effort, like everything else that touches this file.
  }
}
process.on('exit', restore)

putTheme(undefined)

const launch = () => electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })

const open = async () => {
  const app = await launch()
  const page = await app.firstWindow()
  await page.waitForLoadState('domcontentloaded')
  page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
  page.on('console', (m) => m.type() === 'error' && console.log('CONSOLE ERROR:', m.text()))
  await page.waitForTimeout(2000)
  return { app, page }
}

/** One resolved token off the root element, as the browser actually computes it. */
const token = (page, name) =>
  page.evaluate((n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim(), name)

const attribute = (page, name) =>
  page.evaluate((n) => document.documentElement.getAttribute(n), name)

// The nineteen colour tokens. A `color-mix` chain that breaks — a role never set, a typo in a
// variable name — leaves one of these resolving to nothing, and nothing else goes wrong first.
const TOKENS = [
  '--bg', '--bg-side', '--ink', '--ink-dim', '--ink-faint', '--line',
  '--accent', '--accent-ink', '--accent-quiet', '--bubble-user', '--bubble-user-ink',
  '--bubble-agent', '--added', '--added-bg', '--removed', '--removed-bg',
  '--warn', '--warn-bg', '--confine', '--confine-bg', '--code-bg', '--tree-bg',
]

// ---------------------------------------------------------------- brave costs nothing

let { app, page } = await open()

check((await attribute(page, 'data-theme')) === null, 'with no theme chosen the root carries no data-theme')
const braveBg = await token(page, '--bg')
check(/^(#|rgb)/.test(braveBg), `brave leaves --bg as the stylesheet's own (${braveBg})`)

// The two literals in `shared/theme.ts` exist because a partial palette has to inherit against
// something, and they duplicate the `:root` block. This is the guard against them drifting.
const braveRoles = {
  '--bg': 'background', '--ink': 'text', '--ink-dim': 'muted', '--added': 'ok',
  '--removed': 'fail', '--warn': 'running', '--confine': 'accent',
  '--accent': 'note', '--bubble-user': 'primary',
}
const dark = await page.evaluate(() => matchMedia('(prefers-color-scheme: dark)').matches)
// Read out of the source rather than imported, because this is a `.mjs` and that is a `.ts`.
// A regex is enough for a literal that exists to be compared against the stylesheet.
const source = readFileSync('src/shared/theme.ts', 'utf8')
const literal = source.slice(source.indexOf(`export const BRAVE_${dark ? 'DARK' : 'LIGHT'}`))
const declared = Object.fromEntries(
  [...literal.slice(0, literal.indexOf('}')).matchAll(/(\w+): '(#[0-9a-f]{6})'/g)].map((m) => [m[1], m[2]]),
)
for (const [name, role] of Object.entries(braveRoles)) {
  // Both sides through the browser's own parser, so `#ffffff` and `rgb(255, 255, 255)` compare
  // equal and the check is about the colour rather than about how it was spelled.
  const asRgb = (value) =>
    page.evaluate((v) => {
      const probe = document.createElement('span')
      probe.style.color = ''
      probe.style.color = v
      document.body.appendChild(probe)
      const rgb = getComputedStyle(probe).color
      probe.remove()
      return rgb
    }, value)
  const computed = await asRgb(await token(page, name))
  const want = await asRgb(declared[role])
  check(computed === want, `BRAVE_${dark ? 'DARK' : 'LIGHT'}.${role} still matches ${name} in styles.css`)
}

// ---------------------------------------------------------------- the picker previews

// The picker is opened from the native menu, which Playwright cannot click. The command is sent
// down the same channel a chosen menu item arrives on, so this exercises the real path.
await app.evaluate(({ BrowserWindow }) => {
  BrowserWindow.getAllWindows()[0].webContents.send('bravebot:command', 'view.theme', null)
})
await page.waitForTimeout(400)

check((await page.locator('.theme-picker').count()) === 1, 'View ▸ Theme… opens the picker')
const rows = await page.locator('.theme-row').count()
check(rows >= 22, `the picker offers brave and the twenty-one named schemes (${rows} rows)`)
check(
  (await page.locator('.theme-row').first().locator('.theme-name').textContent()) === 'brave',
  'brave is the first row',
)

await page.locator('.theme-list').focus()
await page.keyboard.press('ArrowDown')
await page.waitForTimeout(250)
const previewed = await token(page, '--bg')
check((await page.locator('.theme-picker').count()) === 1, 'the picker is still open after previewing')
check(previewed !== braveBg, `moving the cursor repaints the window behind the panel (${previewed})`)
// `parseState` writes every key on any update, so an untouched choice reads as `brave` rather
// than as absent. Either is "nobody has chosen anything"; a name is not.
const remembered = readState().theme
check(remembered === undefined || remembered === 'brave', `previewing has written nothing down (${remembered})`)
await page.screenshot({ path: join(shots, '01-theme-preview.png') })

// A palette saved while the picker is open must update the list without repainting over the
// preview somebody is looking at. The window belongs to the picker until it closes.
mkdirSync(themesDirectory, { recursive: true })
writeFileSync(brokenTheme, '{ not json', 'utf8')
await page.waitForTimeout(1200)
check(
  (await token(page, '--bg')) === previewed,
  'a palette saved mid-preview does not repaint over what is being previewed',
)

await page.keyboard.press('Escape')
await page.waitForTimeout(250)
check((await page.locator('.theme-picker').count()) === 0, 'Escape closes the picker')
check((await token(page, '--bg')) === braveBg, 'Escape puts the previous theme back exactly')

// ---------------------------------------------------------------- keeping one

await app.evaluate(({ BrowserWindow }) => {
  BrowserWindow.getAllWindows()[0].webContents.send('bravebot:command', 'view.theme', null)
})
await page.waitForTimeout(400)
await page.locator('.theme-list').focus()
for (let i = 0; i < rows; i++) {
  const name = await page.locator('.theme-row.active .theme-name').textContent()
  if (name === 'nord') break
  await page.keyboard.press('ArrowDown')
}
check((await page.locator('.theme-row.active .theme-name').textContent()) === 'nord', 'arrowed to nord')
await page.keyboard.press('Enter')
await page.waitForTimeout(300)

check((await page.locator('.theme-picker').count()) === 0, 'Enter closes the picker')
check((await attribute(page, 'data-theme')) === 'nord', 'the root now says nord')
check((await attribute(page, 'data-ground')) === 'own', 'a named theme paints its own ground')
check(readState().theme === 'nord', 'the choice was remembered in bravebot-ui.json')

const empty = []
for (const name of TOKENS) if ((await token(page, name)) === '') empty.push(name)
check(empty.length === 0, `every token still resolves under a theme${empty.length ? `: ${empty}` : ''}`)
await page.screenshot({ path: join(shots, '02-theme-nord.png') })

// ---------------------------------------------------------------- it survives a relaunch

await app.close()
;({ app, page } = await open())
check((await attribute(page, 'data-theme')) === 'nord', 'the theme is still nord after a relaunch')

// ---------------------------------------------------------------- palettes somebody wrote

mkdirSync(themesDirectory, { recursive: true })
writeFileSync(
  driverTheme,
  JSON.stringify({ defs: { ink: '#123456' }, background: 'none', primary: 'ink' }),
  'utf8',
)
writeFileSync(brokenTheme, '{ not json', 'utf8')
// A pale ground and a pale primary, so the luma test has to answer black where it answered white
// for everything above. This is the check that a light palette is legible rather than white on
// white, and it is the one thing in this file a screenshot genuinely could not settle.
writeFileSync(
  paleTheme,
  JSON.stringify({ background: '#fbf7ef', text: '#3c3836', muted: '#7c6f64', primary: '#ffee00', note: '#ffee00' }),
  'utf8',
)
await page.waitForTimeout(1200)

await app.evaluate(({ BrowserWindow }) => {
  BrowserWindow.getAllWindows()[0].webContents.send('bravebot:command', 'view.theme', null)
})
await page.waitForTimeout(400)
const names = await page.locator('.theme-name').allTextContents()
check(names.includes('driver-test'), 'a palette written by hand appears in the picker')
check(!names.includes('driver-broken'), 'a broken palette file is not a theme')

const at = names.indexOf('driver-test')
await page.locator('.theme-list').focus()
await page.keyboard.press('Home')
for (let i = 0; i < at; i++) await page.keyboard.press('ArrowDown')
await page.waitForTimeout(250)
check((await attribute(page, 'data-ground')) === null, 'a palette that inherits its ground keeps the window blur')
check((await token(page, '--bubble-user')) !== '', 'the one role it names took effect')
await page.screenshot({ path: join(shots, '03-theme-custom.png') })

const pale = names.indexOf('driver-pale')
await page.keyboard.press('Home')
for (let i = 0; i < pale; i++) await page.keyboard.press('ArrowDown')
await page.waitForTimeout(250)
check((await token(page, '--bubble-user-ink')) === '#000000', 'a pale primary takes black text')
check((await token(page, '--accent-ink')) === '#000000', 'and so does a pale accent')
check((await token(page, '--role-scheme')) === 'light', 'a pale ground asks for light native controls')
await page.screenshot({ path: join(shots, '04-theme-light.png') })

// ---------------------------------------------------------------- the editing loop

// Writing a palette means saving the file and looking at the window. Keeping this one and then
// rewriting it is that loop: the window must follow without a relaunch, or every adjustment of a
// colour costs a restart. Nothing on screen says whether it did, which is why it is asserted.
await page.keyboard.press('Home')
for (let i = 0; i < at; i++) await page.keyboard.press('ArrowDown')
await page.keyboard.press('Enter')
await page.waitForTimeout(300)
check((await attribute(page, 'data-theme')) === 'driver-test', 'a palette written by hand can be kept')
const before = await token(page, '--bubble-user')

writeFileSync(driverTheme, JSON.stringify({ background: 'none', primary: '#ff00aa' }), 'utf8')
await page.waitForTimeout(1200)
check(
  (await token(page, '--bubble-user')) !== before,
  'editing the palette in use repaints the window without a relaunch',
)

// ---------------------------------------------------------------- the PDF stays white

// The print window pins the palette light by source order, and `:root[data-theme]` would outrank
// the plain `:root` block that does it. It never matches there because `export.tsx` does not
// import the applier — which is a promise about an import graph, so it is checked as one. Nothing
// visible would go wrong first: a session exported at night in a dark palette would simply come
// out black on paper, and only on paper.
const printed = readdirSync('out/renderer/assets').find((f) => f.startsWith('export-') && f.endsWith('.js'))
const bundle = printed ? readFileSync(join('out/renderer/assets', printed), 'utf8') : ''
check(printed !== undefined, 'the print bundle was built')
check(!bundle.includes('data-theme'), 'the print bundle carries no theming, so a PDF is always brave')

// ---------------------------------------------------------------- back to brave

await app.evaluate(({ BrowserWindow }) => {
  BrowserWindow.getAllWindows()[0].webContents.send('bravebot:command', 'view.theme', null)
})
await page.waitForTimeout(400)
await page.locator('.theme-list').focus()
await page.keyboard.press('Home')
await page.keyboard.press('Enter')
await page.waitForTimeout(300)
check((await attribute(page, 'data-theme')) === null, 'choosing brave leaves no theme on the root')
check((await token(page, '--bg')) === braveBg, 'and the window is the one it was at launch')

await app.close()

console.log(problems.length === 0 ? 'RESULT: ok' : `RESULT: failed — ${problems.join('; ')}`)
process.exit(problems.length === 0 ? 0 : 1)
