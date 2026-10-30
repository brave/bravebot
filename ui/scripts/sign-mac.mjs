// Sign and notarise the macOS application and its disk images, for `make app-sign`,
// `app-notarize` and `app-dmg-notarize`.
//
// `@electron/osx-sign` and `@electron/notarize` do the work. They are the libraries behind the
// packager's `osxSign` and `osxNotarize` options, called here instead because `package.mjs` sets
// the fuses after the packager returns. That rewrites the Electron binary, so a signature made
// inside the packager would no longer match it. `make app-bundles` leaves the bundle unsigned and
// this signs it where it lies.
//
// `@electron/osx-sign` signs every binary in the app from the inside out, each with the hardened
// runtime and a secure timestamp and none with `--deep`, and the app last. `@electron/notarize`
// checks the signature, submits, requires the status `Accepted` whatever `notarytool`'s exit
// status is, and staples. What is decided here is the entitlements each item carries, signing a
// disk image, and that nothing runs unless the identity or profile and every path it works on are
// present.
//
//   SIGN_IDENTITY=<identity> node scripts/sign-mac.mjs sign <app>...
//   NOTARY_PROFILE=<profile> node scripts/sign-mac.mjs notarize <app>...
//   SIGN_IDENTITY=<identity> NOTARY_PROFILE=<profile> node scripts/sign-mac.mjs image <dmg>...
import { notarize } from '@electron/notarize'
import { sign } from '@electron/osx-sign'
import { execFile } from 'node:child_process'
import { existsSync, realpathSync, statSync } from 'node:fs'
import { dirname, join, relative, resolve, sep } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { promisify } from 'node:util'

const BUILD = join(dirname(fileURLToPath(import.meta.url)), '..', 'build')

// Only the Electron processes run JIT-compiled code. The Rust executables, the frameworks and the
// dylibs carry an empty set, since `allow-jit` would only widen what a compromised agent process
// may do. `@electron/osx-sign` always passes an entitlements file, so none is a file with no keys.
export const ENTITLEMENTS = {
  app: join(BUILD, 'entitlements.mac.plist'),
  helper: join(BUILD, 'entitlements.mac.inherit.plist'),
  none: join(BUILD, 'entitlements.mac.none.plist'),
}

// The entitlements for `file`, a path in or equal to `app`. The app and its main executable carry
// the app's. Anything inside a helper app, which is an `.app` under `Contents/Frameworks`, carries
// the helpers'. Everything else carries none.
export function entitlementsFor(app, file) {
  const inside = relative(app, file)
  if (inside === '' || inside.startsWith(join('Contents', 'MacOS') + sep)) return ENTITLEMENTS.app
  if (inside.split(sep).some((part) => part.endsWith('.app'))) return ENTITLEMENTS.helper
  return ENTITLEMENTS.none
}

export function signOptions(app, identity) {
  return {
    app,
    identity,
    platform: 'darwin',
    optionsForFile: (file) => ({ entitlements: entitlementsFor(app, file), hardenedRuntime: true }),
  }
}

const SETTINGS = {
  SIGN_IDENTITY: 'the Developer ID Application identity to sign with',
  NOTARY_PROFILE: 'the notarytool keychain profile to submit with',
}

// What each command works on, what it needs from the environment, and what makes the thing it works
// on: an `.app` is a directory and a `.dmg` is a file.
const COMMANDS = {
  sign: { extension: '.app', needs: ['SIGN_IDENTITY'], after: 'make app-bundles' },
  notarize: { extension: '.app', needs: ['NOTARY_PROFILE'], after: 'make app-sign' },
  image: { extension: '.dmg', needs: ['SIGN_IDENTITY', 'NOTARY_PROFILE'], after: 'make app-dmg' },
}

// Why the command cannot run, or null. Checked for every path before any tool runs, so a missing
// second bundle does not leave the first signed or submitted.
export function refusal(command, env, paths) {
  const spec = COMMANDS[command]
  if (!spec) return `the command is sign, notarize or image, not ${command}`
  for (const name of spec.needs) {
    if (!env[name]) return `${name} is not set: ${SETTINGS[name]}`
  }
  if (paths.length === 0) return `${command} takes the paths to work on`
  for (const path of paths) {
    if (!path.endsWith(spec.extension)) return `${path} is not a ${spec.extension} for ${command} to work on`
    if (!existsSync(path) || statSync(path).isDirectory() !== (spec.extension === '.app')) {
      return `no ${path}: run \`${spec.after}\` first`
    }
  }
  return null
}

export async function run(command, env, paths) {
  const why = refusal(command, env, paths)
  if (why) throw new Error(why)
  for (const given of paths) {
    const path = resolve(given)
    if (command === 'sign') {
      await sign(signOptions(path, env.SIGN_IDENTITY))
    } else {
      // `codesign` reads a name or a hash, and an image has no nested items to order.
      if (command === 'image') {
        await promisify(execFile)('codesign', ['--force', '--timestamp', '--sign', env.SIGN_IDENTITY, path])
      }
      await notarize({ appPath: path, keychainProfile: env.NOTARY_PROFILE })
    }
    console.log(`${command === 'sign' ? 'signed' : 'notarised and stapled'} ${path}`)
  }
}

// Run when this file is the program, and not when a test imports it.
if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  const [command, ...paths] = process.argv.slice(2)
  try {
    await run(command, process.env, paths)
  } catch (error) {
    console.error(error.message)
    process.exit(1)
  }
}
