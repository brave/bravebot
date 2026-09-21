// Give the development app its own name in the menu bar.
//
// The bold word beside the Apple menu is the one piece of the menu a template cannot set.
// AppKit reads it from the running bundle's `CFBundleName`, before any JavaScript has run,
// and `app.setName` does not touch it — that call renames `app.name`, which is what the
// role labels and `app.getPath('userData')` are built from, and pointing it somewhere new
// would move the layout file and orphan every remembered column.
//
// Unpackaged, the running bundle is Electron's own, in `node_modules`, and it is called
// Electron. So this renames it. That is a real edit to a dependency: it is undone by the
// next `npm install`, which is why it is also wired to `postinstall` rather than being a
// thing anybody has to remember.
//
// The proper fix is a packaging step with a `productName`, which the README notes does not
// exist yet. When it does, this script stops being needed for anything but `npm run dev`.
import { execFileSync } from 'node:child_process'
import { existsSync } from 'node:fs'

const NAME = 'Brave Bot'
const plist = 'node_modules/electron/dist/Electron.app/Contents/Info.plist'

if (process.platform !== 'darwin') process.exit(0)
if (!existsSync(plist)) {
  // No Electron yet — a `postinstall` can run before it is unpacked, and a missing
  // dependency is not this script's problem to report.
  process.exit(0)
}

const read = (key) => {
  try {
    return execFileSync('plutil', ['-extract', key, 'raw', '-o', '-', plist], {
      encoding: 'utf8',
    }).trim()
  } catch {
    return null
  }
}

if (read('CFBundleName') === NAME) process.exit(0)

for (const key of ['CFBundleName', 'CFBundleDisplayName']) {
  execFileSync('plutil', ['-replace', key, '-string', NAME, plist])
}
console.log(`named the development app "${NAME}"`)
