// That the packaged app is the app.
//
// Everything else here drives a checkout, and two of the things most worth being sure of are
// only true of a bundle. The developer items are gated on `app.isPackaged`, so a release is
// the only build where their absence means anything — and a DevTools console in this renderer
// is a JavaScript prompt inside the process that draws every approval card. And the agent is
// found by a different path when packaged: `process.resourcesPath` rather than whatever
// `cargo` last built, which no development run ever exercises.
//
// Costs nothing: it never sends a prompt. Run `npm run package` first.
import { _electron as electron } from 'playwright-core'
import { existsSync, readFileSync } from 'node:fs'
import { mkdirSync } from 'node:fs'

mkdirSync('/tmp/bravebot-ui', { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
}

const BUNDLE = `dist/Brave Bot-darwin-${process.arch === 'arm64' ? 'arm64' : 'x64'}/Brave Bot.app`
if (!existsSync(BUNDLE)) {
  console.log(`RESULT: skipped — no bundle at ${BUNDLE}; run \`npm run package\``)
  process.exit(0)
}

// The menu bar's title is the one thing no template can set, so it is checked where AppKit
// actually reads it rather than through the menu API, which would only report what we asked
// for. In a release this comes from the bundle we built, not from renaming Electron's.
const plist = readFileSync(`${BUNDLE}/Contents/Info.plist`, 'utf8')
check(/<key>CFBundleName<\/key>\s*<string>Brave Bot<\/string>/.test(plist), 'the bundle is named "Brave Bot"')
check(!/<string>Electron<\/string>/.test(plist.split('CFBundleName')[1] ?? ''), 'and not Electron')
check(
  existsSync(`${BUNDLE}/Contents/Resources/bravebot-rpc`),
  'the agent ships beside the app as a resource',
)

check(existsSync(`${BUNDLE}/Contents/Resources/bravebot-ui-files`), 'the secure file helper ships beside the app')

const app = await electron.launch({
  executablePath: `${BUNDLE}/Contents/MacOS/Brave Bot`,
  timeout: 60000,
})
const page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')
page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
page.on('console', (m) => m.type() === 'error' && console.log('CONSOLE ERROR:', m.text()))
await page.waitForTimeout(4000)

const seen = await app.evaluate(({ app, Menu }) => {
  const menu = Menu.getApplicationMenu()
  const flat = menu.items.flatMap((top) =>
    (top.submenu?.items ?? []).map((i) => ({
      top: top.label,
      id: i.id,
      role: (i.role ?? '').toLowerCase(),
      label: i.label,
    })),
  )
  return {
    packaged: app.isPackaged,
    fromResources: process.resourcesPath,
    titles: menu.items.map((i) => i.label),
    view: flat.filter((i) => i.top === 'View').map((i) => i.role || i.id),
    developer: flat.filter((i) => /devtools|reload/.test(i.role + (i.label ?? '').toLowerCase())),
    edit: flat.filter((i) => i.top === 'Edit').map((i) => i.role),
  }
})

check(seen.packaged === true, 'this is a packaged build, so the gate under test is the real one')

// The assertion this file exists for.
check(
  seen.developer.length === 0,
  `a release offers no Reload and no Developer Tools (${seen.developer.map((i) => i.label).join(', ') || 'none'})`,
)
check(
  seen.view.filter(Boolean).join() === 'view.fold-left,view.fold-right,view.reset-columns,view.theme',
  `View offers columns and themes (${seen.view.filter(Boolean).join(', ')})`,
)

// The roles still have to survive packaging, same as anywhere else.
check(
  ['undo', 'cut', 'copy', 'paste', 'selectall'].every((r) => seen.edit.includes(r)),
  'Edit kept the clipboard roles',
)
check(seen.titles[4] === 'Session', `the app's own menu is there (${seen.titles.join(', ')})`)

// That the agent was found at the packaged path and actually answered. Sessions on screen
// mean `bravebot-rpc` was spawned from Resources and the store was read through it — the one
// code path a development run never takes.
const sessions = await page.locator('.session').count()
check(sessions > 0, `the agent ran from the bundle and returned sessions (${sessions})`)
check(
  seen.fromResources.includes('.app/Contents/Resources'),
  'and it was looked for inside the bundle',
)
const build = await page.locator('.build').textContent().catch(() => null)
check(Boolean(build && build.trim()), `the build stamp came back (${build?.trim() ?? 'nothing'})`)

await page.screenshot({ path: '/tmp/bravebot-ui/13-packaged.png' })
await app.close()

if (problems.length > 0) {
  console.log(`\nRESULT: ${problems.length} failed`)
  process.exit(1)
}
console.log('\nRESULT: ok — shot in /tmp/bravebot-ui/13-packaged.png')
