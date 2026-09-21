// Walk the whole product at a speed a person can follow, so a screen recording of the run is
// the video.
//
// Every other driver in this directory asserts. This one performs: same Playwright, same real
// window, same class names — but paced in beats rather than in milliseconds, with a pointer
// drawn into the page (CDP moves no cursor) and a caption naming each feature as it plays.
//
//   npm run demo                       the whole thing, no model calls
//   npm run demo -- --live             plus a real turn, an approval card and a question
//   npm run demo -- --only 08-fork     one scene, for a retake
//   npm run demo -- --list             what it would film, in order
//   npm run demo -- --rebuild          throw the demo world away and build it again
//
// Nothing real is filmed. The run launches with `$HOME` pointed at a demo world — two invented
// checkouts and the sessions the demo earned in them — so the session list, the paths, the
// prompts and the file tree are all fixtures. `scripts/demo/world.mjs` says how, and why they
// are earned rather than written. `--real` films this machine instead, for working on the demo
// rather than for recording one.
//
// A take usually wants `--countdown 5`, which holds the window still long enough to start the
// recorder, and `--session <words>` once a run has come out well — a second take of a good
// video means filming the *same* session again, and naming it is the only way to be sure.
import { readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { SCENES } from './demo/manifest.mjs'
import { launch, SHOTS } from './demo/stage.mjs'
import { DEFAULT_WORLD, ensureWorld } from './demo/world.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const sceneDir = join(here, 'demo', 'scenes')

const FLAGS = {
  speed: 1, // multiplies every pause; 1.4 is a slower read, 0.7 a tighter cut
  size: [1280, 820],
  theme: 'dark',
  countdown: 0,
  record: null, // a path, or `true` for the default one under /tmp
  live: false,
  compress: true, // the raw capture is retina at 120fps; nobody wants to be sent that
  width: 1280, // 0 keeps the capture's own resolution
  fps: 30,
  crf: 28, // where this app's text is still sharp at 1:1
  keepRaw: false,
  world: DEFAULT_WORLD, // `null` means the real HOME, which is what `--real` sets
  rebuild: false,
  session: null,
  only: null,
  from: null,
  list: false,
}

const argv = process.argv.slice(2)
for (let i = 0; i < argv.length; i++) {
  const flag = argv[i]
  const next = () => argv[++i]
  if (flag === '--live') FLAGS.live = true
  else if (flag === '--record') {
    // The path is optional: `--record` on its own is the common case, and a bare flag must not
    // swallow the flag that happens to follow it.
    FLAGS.record = argv[i + 1] && !argv[i + 1].startsWith('--') ? next() : true
  }
  else if (flag === '--no-compress') FLAGS.compress = false
  else if (flag === '--keep-raw') FLAGS.keepRaw = true
  else if (flag === '--width') FLAGS.width = Number(next())
  else if (flag === '--fps') FLAGS.fps = Number(next())
  else if (flag === '--crf') FLAGS.crf = Number(next())
  else if (flag === '--real') FLAGS.world = null
  else if (flag === '--world') FLAGS.world = next()
  else if (flag === '--rebuild') FLAGS.rebuild = true
  else if (flag === '--list') FLAGS.list = true
  else if (flag === '--speed') FLAGS.speed = Number(next())
  else if (flag === '--theme') FLAGS.theme = next()
  else if (flag === '--countdown') FLAGS.countdown = Number(next())
  else if (flag === '--session') FLAGS.session = next()
  else if (flag === '--only') FLAGS.only = next()
  else if (flag === '--from') FLAGS.from = next()
  else if (flag === '--size') FLAGS.size = next().split('x').map(Number)
  else {
    console.error(`unknown flag: ${flag}`)
    process.exit(2)
  }
}

if (!(FLAGS.speed > 0)) {
  console.error('--speed wants a positive number')
  process.exit(2)
}
if (!['dark', 'light', 'system'].includes(FLAGS.theme)) {
  console.error('--theme wants dark, light or system')
  process.exit(2)
}

// The manifest and the directory have to agree. A scene added and not listed would never play
// and nobody would find out from watching, which is exactly the failure a demo that is meant
// to stay current cannot afford.
const onDisk = readdirSync(sceneDir)
  .filter((name) => name.endsWith('.mjs'))
  .map((name) => name.replace(/\.mjs$/, ''))
  .sort()
const listed = [...SCENES].sort()
const unlisted = onDisk.filter((id) => !listed.includes(id))
const missing = listed.filter((id) => !onDisk.includes(id))
if (unlisted.length || missing.length) {
  if (unlisted.length) console.error(`scenes on disk but not in the manifest: ${unlisted.join(', ')}`)
  if (missing.length) console.error(`scenes in the manifest but not on disk: ${missing.join(', ')}`)
  console.error('add them to scripts/demo/manifest.mjs, in the order they should be filmed')
  process.exit(2)
}

const all = []
for (const id of SCENES) {
  const scene = (await import(join(sceneDir, `${id}.mjs`))).default
  if (scene.id !== id) {
    console.error(`${id}.mjs calls itself "${scene.id}" — the id and the filename have to match`)
    process.exit(2)
  }
  all.push(scene)
}

let chosen = all
if (FLAGS.only) chosen = all.filter((scene) => scene.id.includes(FLAGS.only))
else if (FLAGS.from) {
  const at = all.findIndex((scene) => scene.id.includes(FLAGS.from))
  if (at < 0) {
    console.error(`no scene matches --from ${FLAGS.from}`)
    process.exit(2)
  }
  chosen = all.slice(at)
}
// A live scene only when it was asked for — unless it was named outright, which is a clear
// enough request on its own.
if (!FLAGS.live && !FLAGS.only) chosen = chosen.filter((scene) => !scene.live)

if (!chosen.length) {
  console.error('nothing to film with those flags')
  process.exit(2)
}

if (FLAGS.list) {
  for (const scene of chosen) {
    console.log(`${scene.id.padEnd(14)} ${scene.title}${scene.live ? '   (--live)' : ''}`)
  }
  process.exit(0)
}

console.log(
  `filming ${chosen.length} scene${chosen.length === 1 ? '' : 's'} at ${FLAGS.speed}× ` +
    `into ${FLAGS.size.join('×')}, ${FLAGS.theme}${FLAGS.live ? ', with a live turn' : ''}`,
)

// Before the window opens, because the world is what the window will be showing. A `--real`
// run skips it and films whatever is actually on this machine, which is a thing to do while
// working on the demo and not a thing to do while recording one.
if (FLAGS.world) await ensureWorld(FLAGS.world, FLAGS)
else console.log('  --real: filming this machine\'s own sessions, which are not sanitised')

const stage = await launch(FLAGS)

for (let n = FLAGS.countdown; n > 0; n--) {
  console.log(`  starting in ${n}…`)
  await stage.page.waitForTimeout(1000)
}

// After the countdown, so the count is not in the film, and before the first scene, so the
// narration offsets below are the film's own timestamps.
if (FLAGS.record) {
  const where =
    typeof FLAGS.record === 'string'
      ? FLAGS.record
      : join(SHOTS, `bravebot-ui-${new Date().toISOString().slice(0, 16).replace(/[:T]/g, '')}.mov`)
  await stage.record(where)
}

for (const scene of chosen) await stage.play(scene)

const { narration, skipped, failed, film } = await stage.finish()

console.log('\n── narration ' + '─'.repeat(52))
for (const line of narration) console.log(line)
console.log('─'.repeat(65))
if (skipped.length) console.log(`\nskipped:\n  ${skipped.join('\n  ')}`)
if (failed.length) console.log(`\nfailed:\n  ${failed.join('\n  ')}`)
console.log(`\nstills in ${SHOTS}`)
if (film) console.log(`film     ${film}\nsubtitles ${film.replace(/\.[^.]+$/, '.vtt')}`)
process.exit(failed.length ? 1 : 0)
