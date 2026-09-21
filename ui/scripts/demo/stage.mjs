// The stage every scene is written against: launch the real window, put a pointer and a
// caption on it, and offer a handful of primitives that all move at a speed a person can
// follow.
//
// The pacing lives here and nowhere else. Every wait in a scene is expressed in *beats*
// rather than milliseconds, and a beat is scaled once, by `--speed`, so the whole video can
// be re-timed for a slower audience or a shorter cut without a scene being touched. A scene
// that reached for `waitForTimeout` directly would be the one shot that ignores the dial.
//
// The other thing this file owns is putting everything back. The drivers share one state
// file under `userData`, and a demo that dragged the columns somewhere cinematic would leave
// `drive-columns.mjs` measuring the video's idea of a layout. So the file is snapshotted at
// launch and restored in a `finally`, the way `drive-columns.mjs` already restores what it
// changes.
import { _electron as electron } from 'playwright-core'
import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { spawn, spawnSync } from 'node:child_process'
import { dirname, join } from 'node:path'
import { INSTALL } from './overlay.mjs'
import { projectsIn } from './world.mjs'

// The two numbers the whole video is timed by, and the only two worth arguing about. They
// started at 700/620, which watched back as dead air — a pause between two things happening
// reads as the app being slow, which is the opposite of what a demo is for. 430/380 was then
// too quick to read a caption before it went. These are the settled middle. `--speed` moves
// both together, for an audience that wants it slower or a cut that wants it tighter.
/** One beat: the pause that lets a change land before the next one starts. */
const BEAT = 620
/** How long the pointer takes to cross the window. Long enough to be followed by eye. */
const GLIDE = 430

export const SHOTS = '/tmp/bravebot-ui/demo'

/** `mm:ss` since the first caption, for reading the narration log against the footage. */
const stamp = (from) => {
  const s = Math.round((Date.now() - from) / 1000)
  return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`
}

export async function launch(opts) {
  mkdirSync(SHOTS, { recursive: true })

  // The world, and it takes *two* redirections rather than one.
  //
  // `$HOME` is what the agent reads to find `~/.bravebot` — sessions, history, standing
  // instructions — and pointing it at the world sanitises all of that. It does not sanitise this
  // app's own remembered state, which was the assumption here and is wrong on macOS: Electron
  // derives `userData` from `NSHomeDirectory()`, which comes from the password database and not
  // from the environment, so a run with `HOME` redirected still read and wrote the *real*
  // `bravebot-ui.json`. Measured, not guessed — `app.getPath('userData')` under a redirected HOME
  // answers with the actual user's Application Support directory.
  //
  // That put real things on camera: the recents list is real project paths, and File ▸ Open Recent
  // is filmed; the bots list is real names, purposes and checkouts, and the bots tab is filmed.
  // `--user-data-dir` is the switch that actually moves it, so the world now gets one of its own
  // and the two halves of "nothing real is on screen" are finally both true.
  //
  // `--real` opts out of both, for driving the demo against actual data while working on it.
  const env = opts.world ? { ...process.env, HOME: opts.world } : process.env
  const args = opts.world ? ['.', `--user-data-dir=${join(opts.world, 'userData')}`] : ['.']
  const app = await electron.launch({ args, cwd: process.cwd(), timeout: 40000, env })
  const page = await app.firstWindow()
  await page.waitForLoadState('domcontentloaded')
  page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
  page.on('console', (m) => m.type() === 'error' && console.log('CONSOLE ERROR:', m.text()))

  // Framing before anything is recorded, so the window is not still settling in frame one.
  const [width, height] = opts.size
  await app.evaluate(
    ({ BrowserWindow, nativeTheme }, [w, h, theme]) => {
      nativeTheme.themeSource = theme
      const win = BrowserWindow.getAllWindows()[0]
      win?.setSize(w, h)
      win?.center()
      win?.show()
      win?.focus()
    },
    [width, height, opts.theme],
  )
  // `nativeTheme` alone is not enough. It settles the native chrome — the traffic lights and
  // the sidebar vibrancy — but the renderer's palette is a `prefers-color-scheme` media query,
  // and a window that has already loaded does not re-evaluate one from a main-process
  // assignment. So the query is emulated as well, which is the half CDP can actually reach.
  if (opts.theme !== 'system') await page.emulateMedia({ colorScheme: opts.theme })

  // The saved state, held so it can be given back. Read through the main process rather than
  // guessed at, because `userData` moves with the app's name.
  // Only worth holding when the run is against the real profile: inside a world the state
  // file is the world's own and nothing else reads it.
  const userData = await app.evaluate(({ app }) => app.getPath('userData'))
  const stateFile = join(userData, 'bravebot-ui.json')
  const savedState =
    !opts.world && existsSync(stateFile) ? readFileSync(stateFile, 'utf8') : null

  // A native save panel is modal and would hang the run behind a sheet nobody can dismiss,
  // so exports are pointed at a temporary directory instead. `drive-export.mjs` does the
  // same, for the same reason.
  const exports = join(SHOTS, 'exports')
  mkdirSync(exports, { recursive: true })
  await app.evaluate(({ dialog }, dir) => {
    globalThis.__exported = []
    dialog.showSaveDialog = async (_window, options) => {
      const path = `${dir}/${options?.defaultPath?.split('/').pop() ?? 'export'}`
      globalThis.__exported.push(path)
      return { canceled: false, filePath: path }
    }
  }, exports)

  // The folder picker is a native panel and would hang the run behind a sheet nobody can dismiss,
  // exactly as the save panel above would — so it answers itself too. Pointed at one of the
  // world's own fixture checkouts, which is a path this run laid out rather than one the window
  // composed, and which reads as a real project name on camera. `world.mjs` stubs the same call
  // for the same reason while it is earning the sessions.
  //
  // A `--real` run has no fixtures, so it gets a scratch directory instead: filming this machine
  // is for working on the demo, and a bot made during one should not appear in somebody's repo.
  const chosenDirectory = opts.world
    ? projectsIn(opts.world)[0]
    : join(SHOTS, 'demo-checkout')
  mkdirSync(chosenDirectory, { recursive: true })
  await app.evaluate(({ dialog }, where) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [where] })
  }, chosenDirectory)

  // The session list is filled by a round trip to the agent; nothing can be pointed at until
  // it lands. Deliberately not a `waitForSelector`, because a window with no sessions in it
  // is a legitimate state that the scenes report on rather than hang on.
  await page.waitForTimeout(2500)
  await page.evaluate(INSTALL)

  let started = Date.now()
  const narration = []
  const skipped = []
  const failed = []

  const wait = (ms) => page.waitForTimeout(Math.round(ms * opts.speed))

  /** `n` beats. The unit of "let that land". */
  const beat = (n = 1) => wait(BEAT * n)

  /** Caption the screen, and write the same line to the narration log with its offset. */
  const say = async (title, line, hold = 1) => {
    const at = stamp(started)
    narration.push(`${at}  ${title}${line ? ` — ${line}` : ''}`)
    console.log(`  ${at}  ${title}${line ? ` — ${line}` : ''}`)
    await page.evaluate(([t, l]) => window.__demo.say(t, l), [title, line ?? ''])
    await beat(hold)
  }

  const hush = async () => {
    await page.evaluate(() => window.__demo.say(null))
    await beat(0.4)
  }

  /** Where the pointer should land: the centre of a box, clamped into the window. */
  const centre = async (target) => {
    const locator = typeof target === 'string' ? page.locator(target).first() : target
    const box = await locator.boundingBox()
    if (!box) throw new Error(`nothing on screen to point at: ${locator}`)
    return { locator, x: box.x + box.width / 2, y: box.y + box.height / 2 }
  }

  /**
   * Glide the drawn pointer to something, and wait out the glide.
   *
   * Chromium's own mouse is moved to the same place at the end of the glide, which is what
   * keeps the window's hover states and its tooltips underneath the pointer somebody can
   * actually see. Without it they appear wherever the last click happened to land and hang
   * there, which on camera looks like a tooltip belonging to nothing.
   */
  const pointAt = async (target) => {
    const { locator, x, y } = await centre(target)
    await page.evaluate(([px, py, ms]) => window.__demo.cursor(px, py, ms), [x, y, GLIDE * opts.speed])
    await wait(GLIDE + 90)
    await page.mouse.move(x, y)
    return locator
  }

  /** Point, ripple, click, breathe. The verb almost every scene is written in. */
  const click = async (target, options) => {
    const locator = await pointAt(target)
    await page.evaluate(() => window.__demo.tap())
    await locator.click(options)
    await beat()
    return locator
  }

  /** Point and hover — the fork control only exists once a prompt is under the pointer. */
  const hover = async (target) => {
    const locator = await pointAt(target)
    await locator.hover()
    await beat()
    return locator
  }

  /** Typing that reads as typing rather than as text appearing. */
  const slowType = async (target, text) => {
    const locator = await pointAt(target)
    await page.evaluate(() => window.__demo.tap())
    await locator.click()
    await locator.pressSequentially(text, { delay: Math.round(55 * opts.speed) })
    await beat()
    return locator
  }

  /** A ring around something the pointer is not going to, to send the eye there anyway. */
  const spotlight = async (target, beats = 1.4) => {
    const locator = typeof target === 'string' ? page.locator(target).first() : target
    const box = await locator.boundingBox()
    if (!box) return
    await page.evaluate((b) => window.__demo.ring(b), box)
    await beat(beats)
  }

  const unspot = async () => {
    await page.evaluate(() => window.__demo.ring(null))
    await beat(0.3)
  }

  /** Scroll something into view slowly, rather than jumping the transcript to it. */
  const glideTo = async (target) => {
    const locator = typeof target === 'string' ? page.locator(target).first() : target
    await locator.evaluate((el) => el.scrollIntoView({ behavior: 'smooth', block: 'center' }))
    await beat(1.2)
    return locator
  }

  const shot = (name) => page.screenshot({ path: `${SHOTS}/${name}.png` })

  /** Ask the agent something directly, the way the window does. */
  const request = (method, params) =>
    page.evaluate(([m, p]) => window.bravebot.request(m, p), [method, params])

  /**
   * Record the window to a file, so a run produces the video rather than producing something
   * somebody then has to remember to record.
   *
   * `screencapture -v` is macOS's own recorder and is already a dependency of nothing: it
   * ships with the OS, it takes a rectangle, and this app's window is at a rectangle we chose
   * ourselves a moment ago. It finalises the file on SIGINT, which is how the run ends it.
   *
   * It needs Screen Recording permission for whatever is running this. There is no way to ask
   * for it from here and no way to detect it up front — the process starts happily either way
   * — so the check is after the fact: a file that never appeared, or appeared empty, means the
   * permission is missing, and that is worth saying plainly rather than leaving somebody to
   * find an empty `.mov` later.
   */
  let recorder = null
  let movie = null

  const record = async (where) => {
    const bounds = await app.evaluate(({ BrowserWindow }) => {
      const win = BrowserWindow.getAllWindows()[0]
      return win ? win.getBounds() : null
    })
    if (!bounds) return console.log('  not recording: there is no window to record')

    movie = where
    mkdirSync(dirname(movie), { recursive: true })
    rmSync(movie, { force: true })
    const rect = [bounds.x, bounds.y, bounds.width, bounds.height].map(Math.round).join(',')
    // `-x` because a shutter sound in the middle of a take is a strange thing to explain, and
    // the cursor is deliberately not captured: the drawn one is the one that moves.
    recorder = spawn('screencapture', ['-v', '-x', '-R', rect, movie], { stdio: 'ignore' })
    recorder.on('error', (error) => {
      console.log(`  not recording: ${error.message}`)
      recorder = null
    })
    // The recorder takes a moment to actually start, and the clock is zeroed once it has, so
    // the narration offsets are the video's own timestamps rather than approximately them.
    await page.waitForTimeout(900)
    started = Date.now()
    console.log(`  recording ${rect} → ${movie}`)
  }

  const megabytes = (path) => (statSync(path).size / 1e6).toFixed(1)

  /**
   * Re-encode the capture to something shareable.
   *
   * `screencapture` records at the display's own resolution and refresh rate, which on this
   * hardware means a retina 2560×1640 at **120 fps** — around 45 MB for three minutes of a
   * mostly stationary window. Nothing in the flag list changes that, so it is fixed afterwards
   * rather than asked for up front: halved to logical resolution, dropped to 30 fps, and
   * encoded at a CRF where the app's own text is still sharp. That is roughly a tenth of the
   * size for footage that looks the same at 1:1.
   *
   * `ffmpeg` is not a dependency of this repository and the run does not require one — a
   * machine without it keeps the raw capture and is told why it is large. macOS's own
   * `avconvert` was tried and is not an alternative: it re-encodes at the source resolution
   * and frame rate and saved almost nothing.
   */
  const encode = async (from) => {
    if (!opts.compress) return from
    if (spawnSync('ffmpeg', ['-version'], { stdio: 'ignore' }).status !== 0) {
      console.log(`  ${megabytes(from)} MB, uncompressed — install ffmpeg to get about a tenth of that`)
      return from
    }

    const to = from.replace(/\.[^.]+$/, '') + '.mp4'
    const scale = opts.width > 0 ? ['-vf', `scale=${opts.width}:-2:flags=lanczos`] : []
    console.log(`  encoding ${opts.width || 'full'}w ${opts.fps}fps crf${opts.crf}…`)
    const done = spawnSync(
      'ffmpeg',
      ['-v', 'error', '-y', '-i', from, ...scale, '-r', String(opts.fps),
       '-c:v', 'libx264', '-crf', String(opts.crf), '-preset', 'slow',
       // `yuv420p` and `+faststart` are what make it play everywhere and start before it has
       // finished downloading, which is the whole point of a file meant to be shared.
       '-pix_fmt', 'yuv420p', '-movflags', '+faststart', to],
      { stdio: 'inherit' },
    )
    if (done.status !== 0 || !existsSync(to)) {
      console.log('  ffmpeg would not encode it; keeping the raw capture')
      return from
    }
    console.log(`  ${megabytes(from)} MB → ${megabytes(to)} MB`)
    if (!opts.keepRaw) rmSync(from, { force: true })
    return to
  }

  const stopRecording = async () => {
    if (!recorder) return null
    const ended = new Promise((resolve) => recorder.once('exit', resolve))
    recorder.kill('SIGINT') // how screencapture is told to finalise the file
    await Promise.race([ended, new Promise((r) => setTimeout(r, 8000))])
    recorder = null
    const size = existsSync(movie) ? statSync(movie).size : 0
    if (size === 0) {
      console.log(
        `\n  the recording is empty — grant Screen Recording to whatever is running this\n` +
          `  (System Settings › Privacy & Security › Screen Recording), then run it again`,
      )
      return null
    }
    return await encode(movie)
  }

  const stage = {
    app,
    page,
    opts,
    speed: opts.speed,
    beat,
    wait,
    say,
    record,
    hush,
    pointAt,
    click,
    hover,
    slowType,
    spotlight,
    unspot,
    glideTo,
    shot,
    request,
    exports,
    /** Where the stubbed folder picker answers with, for a scene that needs to name it. */
    chosenDirectory,
    /**
     * Open an in-window menu and wait until it is actually there.
     *
     * `PopMenu` closes on any scroll — deliberately, because AppKit does, and because the
     * alternative is a reflow loop chasing the anchor. A transcript that is still settling
     * after a session opened emits exactly such a scroll, a beat after the click, and the
     * menu is gone before the item can be picked. That is a race a demo loses on camera, so
     * the click is retried rather than trusted.
     */
    async openMenu(target, tries = 3) {
      for (let n = 0; n < tries; n++) {
        await stage.click(target)
        try {
          await page.locator('[role="menu"]').first().waitFor({ state: 'visible', timeout: 1200 })
          await beat(0.4)
          if (await page.locator('[role="menu"]').first().isVisible()) return true
        } catch {
          /* it closed under us; settle and go again */
        }
        await page.waitForTimeout(500)
      }
      return false
    },
    /**
     * Read whatever modal is standing and take it away again. A saved export reports itself
     * through `Notice`, which is worth a beat on camera and fatal to leave up.
     */
    async dismissNotice({ hold = 2 } = {}) {
      const notice = page.locator('.notice')
      if (!(await notice.isVisible().catch(() => false))) return null
      const said = ((await notice.locator('.notice-body').textContent()) ?? '').trim()
      await beat(hold)
      const button = page.locator('.notice-actions button, .notice button').first()
      if (await button.count()) await button.click()
      await beat(0.5)
      return said
    },
    /** A scene bows out rather than failing: a missing precondition is not a broken video. */
    skip(why) {
      throw Object.assign(new Error(why), { skipped: true })
    },
  }

  /** Put the columns back the way a first-time viewer would find them. */
  stage.reset = async () => {
    // A modal left standing by the scene before this one would take this one out too: the
    // scrim swallows every click, and the failure it produces names the button that was
    // blocked rather than the dialog doing the blocking, which is a bad half-hour. So a scene
    // never inherits one.
    for (let tries = 0; tries < 3; tries++) {
      if (!(await page.locator('.scrim').isVisible().catch(() => false))) break
      const dismiss = page.locator('.notice-actions button, .notice button').first()
      if (await dismiss.count()) await dismiss.click().catch(() => {})
      else await page.keyboard.press('Escape').catch(() => {})
      await page.waitForTimeout(300)
    }
    for (const side of ['left', 'right']) {
      const toggle = page.locator(`.fold-toggle.${side}`)
      if ((await toggle.getAttribute('aria-expanded')) === 'false') {
        await toggle.click()
        await page.waitForTimeout(300)
      }
    }
    // The left column has two lists now, and a scene that left it on the bots would hand the next
    // one a session list that is present and invisible. Put back with the columns, for the same
    // reason they are: a scene begins where a first-time viewer would find the window.
    const sessionsTab = page.locator('.sidebar-tab').first()
    if ((await sessionsTab.count()) && (await sessionsTab.getAttribute('aria-pressed')) !== 'true') {
      await sessionsTab.click()
      await page.waitForTimeout(250)
    }

    const group = page.locator('.session-group')
    if ((await group.getAttribute('aria-pressed')) === 'true') {
      await group.click()
      await page.waitForTimeout(250)
    }
    const find = page.locator('.session-find')
    if (await find.isVisible().catch(() => false)) await find.fill('')
    await page.waitForTimeout(200)
  }

  stage.finish = async () => {
    await hush()
    await beat(1) // a second of the finished window, rather than a cut on the last caption
    const film = await stopRecording()
    await app.close()
    // The state file is shared with the assertion drivers, so a `--real` run does not get to
    // decide what they measure next.
    if (savedState !== null) writeFileSync(stateFile, savedState, 'utf8')

    // Subtitles, for free. Every caption already knows when it appeared and the next one says
    // when it went away, which is exactly a cue — so a run that recorded itself also produced
    // the track that explains itself, and an editor has something to cut against.
    if (film) {
      const seconds = (mmss) => Number(mmss.slice(0, 2)) * 60 + Number(mmss.slice(3, 5))
      const clock = (s) =>
        `${String(Math.floor(s / 3600)).padStart(2, '0')}:` +
        `${String(Math.floor(s / 60) % 60).padStart(2, '0')}:` +
        `${String(s % 60).padStart(2, '0')}.000`
      const cues = narration.map((line) => ({
        at: seconds(line.slice(0, 5)),
        text: line.slice(5).trim(),
      }))
      const vtt = ['WEBVTT', '']
      cues.forEach((cue, i) => {
        // A cue lasts until the next one, and the last one until the film stops — otherwise
        // the closing card would flash a zero-length subtitle and vanish.
        const to = cues[i + 1]?.at ?? cue.at + 6
        vtt.push(`${clock(cue.at)} --> ${clock(Math.max(to, cue.at + 1))}`, cue.text, '')
      })
      writeFileSync(film.replace(/\.[^.]+$/, '.vtt'), vtt.join('\n'), 'utf8')
    }
    return { narration, skipped, failed, film }
  }

  /**
   * Run one scene. A scene that fails takes itself out of the video and nothing else with it:
   * a take is expensive, and losing eight good minutes to a ninth scene that could not find a
   * quarantined blob is the wrong trade.
   */
  stage.play = async (scene) => {
    console.log(`\n── ${scene.id}  ${scene.title}`)
    try {
      await stage.reset()
      await scene.run(stage)
      await hush()
    } catch (error) {
      await hush().catch(() => {})
      if (error.skipped) {
        skipped.push(`${scene.id}: ${error.message}`)
        console.log(`   skipped — ${error.message}`)
      } else {
        failed.push(`${scene.id}: ${error.message}`)
        console.log(`   FAILED — ${error.message}`)
        await shot(`failed-${scene.id}`).catch(() => {})
      }
    }
  }

  return stage
}
