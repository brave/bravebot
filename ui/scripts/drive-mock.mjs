// What the drivers that replace the bridge with IPC fixtures share: the launch, and the answer to
// `session.open`.
//
// These drivers run the built app on a temporary `--user-data-dir` and answer `bravebot:request`
// from the main process, so no agent and no model service is involved. `drive-world.mjs` is the
// module for drivers that run the real bridge, and `ui/docs/testing.md` lists which drivers use
// which.
//
// `playwright-core` is loaded when a launch happens, so what only takes a page or an app can be
// tested without a browser driver installed.
import { launchElectron } from './drive-world.mjs'

/**
 * The arguments of a mocked-bridge launch: the app, then the flags that keep the renderer drawing
 * and its timers running while the window is behind another or hidden, then `args`, then the
 * profile. `x11` adds `--ozone-platform=x11` on Linux, which keeps animation frames flowing
 * during Playwright's stability checks.
 */
export function mockedLaunchArguments({ profile, args = [], x11 = false, platform = process.platform }) {
  return [
    '.',
    ...(x11 && platform === 'linux' ? ['--ozone-platform=x11'] : []),
    '--disable-renderer-backgrounding',
    '--disable-background-timer-throttling',
    ...args,
    `--user-data-dir=${profile}`,
  ]
}

/**
 * Launch the app on `profile` and return it with its first window and the messages of the page
 * errors the window raises, each of which is also printed as `RENDERER <message>`. `env` is added
 * to the variables the driver runs with. `timeout` is the window's default wait, in milliseconds,
 * and `viewport` its size. The app is closed again if the window never comes up.
 */
export async function launchMocked({ profile, env, args, x11, timeout = 10000, viewport }, start = launchElectron) {
  if (!profile) throw new Error('launchMocked needs a profile, or the app would run on the real one')
  const app = await start({
    args: mockedLaunchArguments({ profile, args, x11 }),
    cwd: process.cwd(),
    timeout: 40000,
    ...(env ? { env: { ...process.env, ...env } } : {}),
  })
  try {
    const page = await app.firstWindow()
    if (viewport) await page.setViewportSize(viewport)
    page.setDefaultTimeout(timeout)
    const errors = []
    page.on('pageerror', (error) => {
      errors.push(error.message)
      console.error('RENDERER', error.message)
    })
    return { app, page, errors }
  } catch (error) {
    await app.close().catch(() => undefined)
    throw error
  }
}

/**
 * The fields a mocked `session.open` answer holds besides the ones that name the session, as data
 * a driver passes to its `app.evaluate` call: Playwright sends that function to the main process
 * as source text, so its body cannot call anything in this module. The handler there completes it
 * for the session it is asked for:
 *
 *     if (method === 'session.open') {
 *       const row = rows.find((each) => each.id === p.id)
 *       return { ok: { ...opened, session: `s-${p.id}`, record: { ...row, ...opened.record } } }
 *     }
 *
 * `record` holds what a stored session adds to its row. A field the renderer reads is added here
 * once, and not to each driver.
 */
export function mockedSessionOpen({ model = 'sample/fast', said = [], todos = {}, turns = 0, tokens = 0 } = {}) {
  return {
    model,
    record: { started: 1, turns, tokens, build: 'fixture' },
    said,
    todos,
    context: '',
    trust: { known: true, rules: [] },
    archived: 0,
    branchNote: null,
    buildNote: null,
    frontNote: null,
  }
}
