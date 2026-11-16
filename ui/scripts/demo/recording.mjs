// Recording a demo run: the screen capture, the re-encode, and the subtitle track.
//
// `stage.mjs` owns the window, the pointer and the pacing. Everything that turns a take into
// files lives here, so changing the encoder settings or the subtitle format does not mean
// reading the stage.
import { existsSync, mkdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { spawn, spawnSync } from 'node:child_process'
import { dirname } from 'node:path'

const megabytes = (path) => (statSync(path).size / 1e6).toFixed(1)

/**
 * The recorder for one run. `record(where)` starts the capture and returns the time it
 * started, which the stage uses to zero the narration clock; it returns nothing when there
 * is no window to record. `stop()` ends the capture and returns the path of the finished
 * film, or null when nothing usable was recorded.
 */
export function createRecorder({ app, page, opts }) {
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
    const started = Date.now()
    console.log(`  recording ${rect} → ${movie}`)
    return started
  }

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

  const stop = async () => {
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

  return { record, stop }
}

/**
 * Write the narration beside the film as a WebVTT file with the film's name and a `.vtt`
 * extension.
 *
 * Every caption already knows when it appeared and the next one says when it went away, which
 * is exactly a cue — so a run that recorded itself also produced the track that explains
 * itself, and an editor has something to cut against. Each narration line is `mm:ss  text`.
 */
export function writeSubtitles(narration, film) {
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
