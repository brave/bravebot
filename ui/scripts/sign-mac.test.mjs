// What signing and notarising the macOS application asks of `codesign`, `notarytool` and
// `stapler`, and in what order ([RELEASE-23](../../docs/specs/releases.md#RELEASE-23)).
//
// `@electron/osx-sign` and `@electron/notarize` run for real, against stand-ins for the tools they
// start: each records its arguments, and `notarytool` answers with the verdict a test sets and
// exits 0 whatever it is, as the real one does for a rejected submission. What the real tools
// accept, and whether Apple notarises the result, is not checked here: that needs a Developer ID
// identity and the notary service.
//
// `@electron/osx-sign` leaves the hardened runtime out below macOS 10.13.6, by reading the OS
// version when it is first imported, so the version is set before the script is imported and the
// tests run the same on a Linux host.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import { delimiter, dirname, join, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

const release = os.release
os.release = () => '25.0.0'
const { run } = await import('./sign-mac.mjs')
os.release = release

const skip = process.platform === 'win32' ? 'the stand-in tools are shell scripts' : false
const BUILD = join(dirname(fileURLToPath(import.meta.url)), '..', 'build')
const APP_PLIST = join(BUILD, 'entitlements.mac.plist')
const HELPER_PLIST = join(BUILD, 'entitlements.mac.inherit.plist')
const NONE_PLIST = join(BUILD, 'entitlements.mac.none.plist')
const HASH = '0123456789ABCDEF0123456789ABCDEF01234567'
const IDENTITY = 'Developer ID Application: Test'
const PROFILE = 'test-profile'
const SETTINGS = { SIGN_IDENTITY: IDENTITY, NOTARY_PROFILE: PROFILE }

// Records the tool, the directory it ran in and its arguments, tab separated, one call to a line.
const RECORD = `printf '%s\\t' "$TOOL" "$PWD" "$@" >> "$FAKE_LOG"\nprintf '\\n' >> "$FAKE_LOG"\n`
const TOOLS = {
  codesign: '',
  ditto: 'for last; do :; done\n: > "$last"\n',
  security: `printf '  1) ${HASH} "${IDENTITY} (ABCDE12345)"\\n     1 valid identities found\\n'\n`,
  xcrun: `case "$1 $2" in
  "--find notarytool") echo /usr/bin/notarytool ;;
  "notarytool submit") printf '{"id":"1","status":"%s"}\\n' "\${FAKE_VERDICT:-Accepted}" ;;
  "notarytool log") echo '{}' ;;
esac
`,
}

// A Mach-O header, which is what makes a file a binary to sign.
const BINARY = Buffer.from([0xcf, 0xfa, 0xed, 0xfe, 0, 0, 0, 0, 0, 0, 0, 0])

// What `make app-bundles` leaves, with the items that matter to the signing order: the two Rust
// executables, a framework holding a helper, a dylib and an executable in its `Resources`
// (Squirrel's `ShipIt`) beside a property list, two helper apps, and the main executable.
const BINARIES = [
  'Contents/MacOS/Brave Bot',
  'Contents/Resources/bravebot-rpc',
  'Contents/Resources/bravebot-ui-files',
  'Contents/Frameworks/Electron Framework.framework/Versions/A/Electron Framework',
  'Contents/Frameworks/Electron Framework.framework/Versions/A/Helpers/chrome_crashpad_handler',
  'Contents/Frameworks/Electron Framework.framework/Versions/A/Libraries/libffmpeg.dylib',
  'Contents/Frameworks/Squirrel.framework/Versions/A/Resources/ShipIt',
  'Contents/Frameworks/Brave Bot Helper.app/Contents/MacOS/Brave Bot Helper',
  'Contents/Frameworks/Brave Bot Helper (GPU).app/Contents/MacOS/Brave Bot Helper (GPU)',
]
const DIRECTORIES = [
  'Contents/Frameworks/Electron Framework.framework',
  'Contents/Frameworks/Squirrel.framework',
  'Contents/Frameworks/Brave Bot Helper.app',
  'Contents/Frameworks/Brave Bot Helper (GPU).app',
]

// A scratch directory with the stand-in tools first on `PATH`, a bundle for each architecture and a
// disk image for each, and the environment the tools read. `calls` is what the tools recorded.
function fixture(t, { verdict = 'Accepted' } = {}) {
  const root = realpathSync(mkdtempSync(join(os.tmpdir(), 'sign-mac-')))
  const bin = join(root, 'bin')
  mkdirSync(bin)
  for (const [name, body] of Object.entries(TOOLS)) {
    writeFileSync(join(bin, name), `#!/bin/sh\nTOOL=${name}\n${RECORD}${body}`)
    chmodSync(join(bin, name), 0o755)
  }
  const saved = { ...process.env }
  const log = join(root, 'tools.log')
  Object.assign(process.env, { PATH: `${bin}${delimiter}${saved.PATH}`, FAKE_LOG: log, FAKE_VERDICT: verdict })
  t.after(() => {
    process.env = saved
    rmSync(root, { recursive: true, force: true })
  })
  const apps = ['arm64', 'x64'].map((electron) => {
    const app = join(root, `Brave Bot-darwin-${electron}`, 'Brave Bot.app')
    for (const file of BINARIES) {
      mkdirSync(dirname(join(app, file)), { recursive: true })
      writeFileSync(join(app, file), BINARY)
    }
    for (const directory of DIRECTORIES) mkdirSync(join(app, directory), { recursive: true })
    writeFileSync(join(app, 'Contents/Info.plist'), '<plist/>\n')
    writeFileSync(join(app, 'Contents/Frameworks/Squirrel.framework/Versions/A/Resources/Info.plist'), '<plist/>\n')
    return app
  })
  const images = ['arm64', 'amd64'].map((arch) => {
    const image = join(root, `bravebot-app-darwin-${arch}.dmg`)
    writeFileSync(image, 'image\n')
    return image
  })
  const calls = () => {
    let text = ''
    try {
      text = readFileSync(log, 'utf8')
    } catch {}
    return text.split('\n').filter(Boolean).map((line) => {
      const [tool, cwd, ...args] = line.split('\t').slice(0, -1)
      return { tool, cwd, args }
    })
  }
  return { apps, images, calls }
}

// The signatures made: a call with `--sign` signs the path it ends with.
const signatures = (calls) => calls
  .filter((call) => call.tool === 'codesign' && call.args.includes('--sign'))
  .map((call) => ({ path: call.args.at(-1), args: call.args }))
const after = (args, flag) => args[args.indexOf(flag) + 1]
const xcrun = (calls, ...words) => calls.filter((call) => call.tool === 'xcrun' && words.every((word, index) => call.args[index] === word))

// Every item is signed before the item that contains it, the main executable follows everything
// else in the app, and the app is last. The signed items are exactly the binaries, the frameworks
// and the helper apps, and the app: a property list beside `ShipIt` is not one.
//
// The regression it rejects: signing the app first, or `--deep`, which seals a nested item before
// it is signed, so the outer signature is invalidated when the nested one is made and
// notarisation rejects the app; and an executable inside a framework left unsigned.
test('the app is signed from the inside out', { skip }, async (t) => {
  const { apps, calls } = fixture(t)
  await run('sign', SETTINGS, apps)
  const signed = signatures(calls()).map(({ path }) => path)
  for (const app of apps) {
    const mine = signed.filter((path) => path.startsWith(app))
    const expected = [...BINARIES, ...DIRECTORIES].map((item) => join(app, item)).concat(app)
    assert.deepEqual([...mine].sort(), expected.sort())
    for (const inner of mine) {
      for (const outer of mine.filter((path) => inner.startsWith(path + sep))) {
        assert.ok(mine.indexOf(inner) < mine.indexOf(outer), `${inner} is signed after ${outer}`)
      }
    }
    assert.equal(mine.indexOf(join(app, 'Contents/MacOS/Brave Bot')), mine.length - 2, 'the main executable follows every other item')
    assert.equal(mine.at(-1), app, 'the app is signed last')
  }
})

// The regression it rejects: dropping `--options runtime` or `--timestamp`, which notarisation
// refuses, signing with another identity than the one given, and `--deep`, which applies one set
// of entitlements to everything beneath the app.
test('every signature has the hardened runtime and a secure timestamp, uses the identity, and is not deep', { skip }, async (t) => {
  const { apps, calls } = fixture(t)
  await run('sign', SETTINGS, apps)
  const made = signatures(calls())
  assert.equal(made.length, apps.length * (BINARIES.length + DIRECTORIES.length + 1))
  for (const { path, args } of made) {
    assert.equal(after(args, '--options'), 'runtime', path)
    assert.ok(args.includes('--timestamp'), `${path} has no secure timestamp`)
    assert.ok(!args.some((arg) => arg.startsWith('--timestamp=')), `${path} has a timestamp that is not the secure one`)
    assert.equal(after(args, '--sign'), HASH, path)
    assert.ok(!args.includes('--deep'), path)
  }
})

// The app and its main executable carry the app's entitlements, a helper app and what is in it the
// helpers', and everything else none.
//
// The regression it rejects: swapping the two files, giving every item the app's, or giving the
// agent and the file helper, which are Rust executables that run no JIT-compiled code, the
// `allow-jit` entitlement meant for the Electron processes, which widens what a compromised agent
// process may do.
test('each item is signed with the entitlements it is meant to carry', { skip }, async (t) => {
  const { apps, calls } = fixture(t)
  await run('sign', SETTINGS, apps)
  for (const { path, args } of signatures(calls())) {
    const app = apps.find((candidate) => path.startsWith(candidate))
    const inside = path.slice(app.length + 1)
    let wanted = NONE_PLIST
    if (path === app || inside === 'Contents/MacOS/Brave Bot') wanted = APP_PLIST
    else if (inside.startsWith('Contents/Frameworks/Brave Bot Helper')) wanted = HELPER_PLIST
    assert.equal(after(args, '--entitlements'), wanted, path)
  }
})

// The regression it rejects: signing with an empty identity, which `codesign` reads as ad hoc and
// so produces a signature notarisation refuses, and signing one architecture before finding the
// other missing.
test('signing stops before a tool runs when it cannot complete', { skip }, async (t) => {
  const { apps, images, calls } = fixture(t)
  await assert.rejects(run('sign', { NOTARY_PROFILE: PROFILE }, apps), /SIGN_IDENTITY is not set/)
  await assert.rejects(run('sign', SETTINGS, []), /takes the paths/)
  await assert.rejects(run('sign', SETTINGS, [...apps, `${apps[0]}-missing.app`]), /make app-bundles/)
  await assert.rejects(run('sign', SETTINGS, images), /not a \.app/)
  assert.deepEqual(calls(), [])
})

// Each app has its signature checked, is submitted with the profile it was given and waited for,
// and is stapled where it lies, before the next one is submitted.
//
// The regression it rejects: submitting without `--wait`, stapling before the submission has been
// accepted, stapling a path other than the app, and submitting an app whose signature was never
// checked.
test('an accepted app is checked, submitted with the profile, then stapled', { skip }, async (t) => {
  const { apps, calls } = fixture(t)
  await run('notarize', SETTINGS, apps)
  const made = calls()
  const submissions = xcrun(made, 'notarytool', 'submit')
  const staples = xcrun(made, 'stapler')
  assert.equal(submissions.length, apps.length)
  assert.deepEqual(staples.map((call) => join(call.cwd, call.args.at(-1))), apps)
  apps.forEach((app, index) => {
    const checked = made.findIndex((call) => call.tool === 'codesign' && call.args.includes('--strict') && call.cwd === dirname(app))
    assert.ok(checked >= 0 && checked < made.indexOf(submissions[index]), `${app} was submitted unchecked`)
    assert.ok(submissions[index].args.includes('--wait'))
    assert.equal(after(submissions[index].args, '--keychain-profile'), PROFILE)
    assert.ok(made.indexOf(submissions[index]) < made.indexOf(staples[index]), `${app} was stapled before it was submitted`)
    if (index > 0) assert.ok(made.indexOf(staples[index - 1]) < made.indexOf(submissions[index]), 'the next app was submitted before this one was stapled')
  })
})

// A submission the service did not accept stops the run before anything is stapled, though
// `notarytool` exited 0.
//
// The regression it rejects: trusting the exit status of `notarytool submit --wait`, which is 0
// for an `Invalid` verdict, so a rejected app is stapled, which fails late and reads as a problem
// with stapling.
test('a rejected submission is not stapled', { skip }, async (t) => {
  for (const [command, kind] of [['notarize', 'apps'], ['image', 'images']]) {
    const scratch = fixture(t, { verdict: 'Invalid' })
    await assert.rejects(run(command, SETTINGS, scratch[kind]), /Failed to notarize/, command)
    assert.equal(xcrun(scratch.calls(), 'notarytool', 'submit').length, 1, `${command}: the rejected submission was not the first`)
    assert.deepEqual(xcrun(scratch.calls(), 'stapler'), [], `${command}: a rejected submission was stapled`)
  }
})

// The regression it rejects: submitting the first architecture and finding the second missing
// afterwards, which leaves one app or image notarised and the release half done, and running
// `notarytool` with an empty profile, which fails late with an error about the keychain.
test('notarising stops before a tool runs when it cannot complete', { skip }, async (t) => {
  const { apps, images, calls } = fixture(t)
  await assert.rejects(run('notarize', { SIGN_IDENTITY: IDENTITY }, apps), /NOTARY_PROFILE is not set/)
  await assert.rejects(run('image', { SIGN_IDENTITY: IDENTITY }, images), /NOTARY_PROFILE is not set/)
  await assert.rejects(run('image', { NOTARY_PROFILE: PROFILE }, images), /SIGN_IDENTITY is not set/)
  await assert.rejects(run('notarize', SETTINGS, [...apps, `${apps[0]}-missing.app`]), /make app-sign/)
  await assert.rejects(run('image', SETTINGS, [...images, `${images[0]}-missing.dmg`]), /make app-dmg/)
  await assert.rejects(run('image', SETTINGS, apps), /not a \.dmg/)
  await assert.rejects(run('stamp', SETTINGS, apps), /sign, notarize or image/)
  assert.deepEqual(calls(), [])
})

// An image is signed with the identity, then submitted as the image itself and not archived, then
// stapled, for each architecture.
//
// The regression it rejects: submitting an unsigned image, which the service accepts and
// Gatekeeper then refuses to open, and archiving the image as if it were an app.
test('an accepted image is signed, submitted as it is, and stapled', { skip }, async (t) => {
  const { images, calls } = fixture(t)
  await run('image', SETTINGS, images)
  const made = calls()
  assert.ok(!made.some((call) => call.tool === 'ditto'), 'an image was archived')
  for (const image of images) {
    const signed = made.findIndex((call) => call.tool === 'codesign' && call.args.at(-1) === image)
    const submitted = made.findIndex((call) => call.tool === 'xcrun' && call.args[1] === 'submit' && call.args[2] === image)
    const stapled = made.findIndex((call) => call.tool === 'xcrun' && call.args[0] === 'stapler' && join(call.cwd, call.args.at(-1)) === image)
    assert.ok(signed >= 0 && signed < submitted && submitted < stapled, `${image}: signed ${signed}, submitted ${submitted}, stapled ${stapled}`)
    assert.deepEqual(made[signed].args, ['--force', '--timestamp', '--sign', IDENTITY, image])
  }
})
