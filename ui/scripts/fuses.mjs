// Electron's fuses: switches compiled into the Electron binary, read before any of our code runs.
//
// A release has to turn some of them off, because the app is signed as Brave and every one of
// these makes the signed binary run code that is not ours. With `RunAsNode` on,
// `ELECTRON_RUN_AS_NODE=1` turns the app into a Node interpreter for whatever script it is handed;
// `NODE_OPTIONS` and `--inspect` get code into the main process by other routes. Integrity
// validation and loading only from the asar close the last one, a changed `app.asar` or an `app/`
// directory beside it.
//
// Setting them is a byte write, so it is done here rather than by adding `@electron/fuses`: the
// format is a sentinel, a version byte, a count byte, then one byte per fuse, `0` or `1`, or `r`
// for one Electron has removed. The write changes the binary, so it happens before signing.

export const SENTINEL = 'dL7pKGdnNz796PbbjQWNKmHXBZaB9tsX'
const WIRE_VERSION = 1

// Positions in the wire, in the order Electron declares them. Only the ones a release sets.
const FUSES = {
  RunAsNode: 0,
  EnableNodeOptionsEnvironmentVariable: 2,
  EnableNodeCliInspectArguments: 3,
  EnableEmbeddedAsarIntegrityValidation: 4,
  OnlyLoadAppFromAsar: 5,
}

export const RELEASE_FUSES = {
  RunAsNode: false,
  EnableNodeOptionsEnvironmentVariable: false,
  EnableNodeCliInspectArguments: false,
  EnableEmbeddedAsarIntegrityValidation: true,
  OnlyLoadAppFromAsar: true,
}

// Every wire in the binary, since a universal binary carries one per slice and setting only the
// first leaves the other architecture as it came.
function wires(bytes) {
  const sentinel = Buffer.from(SENTINEL)
  const found = []
  for (let at = bytes.indexOf(sentinel); at !== -1; at = bytes.indexOf(sentinel, at + 1)) {
    const version = bytes[at + sentinel.length]
    const count = bytes[at + sentinel.length + 1]
    const start = at + sentinel.length + 2
    // A write past the end of a Buffer is dropped without an error, so a wire the file cuts off
    // would be set as far as the file goes and reported as set.
    if (count === undefined || start + count > bytes.length) throw new Error('fuse wire cut off: the binary ends inside it')
    if (version !== WIRE_VERSION) throw new Error(`fuse wire version ${version}, and this writes version ${WIRE_VERSION}`)
    found.push({ start, count })
  }
  if (found.length === 0) throw new Error('no fuse wire: this is not an Electron binary')
  return found
}

// What each wire says, one string of `0`, `1` and `r` per wire.
export function readFuses(bytes) {
  return wires(bytes).map(({ start, count }) => bytes.subarray(start, start + count).toString('latin1'))
}

// Whether a debugger can attach, which is how Playwright drives an app. A universal binary runs
// whichever slice the machine picks, so every wire has to allow it.
export function acceptsInspect(bytes) {
  return readFuses(bytes).every((wire) => wire[FUSES.EnableNodeCliInspectArguments] !== '0')
}

// Refuses rather than skipping a fuse it cannot set: an Electron without one of these is an app
// that would ship with that door open and a packaging step that said nothing.
export function setFuses(bytes, settings) {
  for (const { start, count } of wires(bytes)) {
    for (const [name, on] of Object.entries(settings)) {
      const index = FUSES[name]
      if (index === undefined) throw new Error(`unknown fuse ${name}`)
      if (index >= count) throw new Error(`this Electron has no ${name} fuse`)
      if (bytes[start + index] === 0x72) throw new Error(`this Electron has removed the ${name} fuse`)
      bytes[start + index] = on ? 0x31 : 0x30
    }
  }
}
