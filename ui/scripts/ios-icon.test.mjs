// The iOS app's icon, which Xcode compiles and nothing here builds.
//
// The project keeps its own copy of the desktop app's Icon Composer document, because Xcode reads
// an icon from inside the project. A project that names one icon and ships another builds with no
// icon and no error.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { files } from './icon-test-support.mjs'

const UI_BUILD = new URL('../build/', import.meta.url).pathname
const MOBILE = new URL('../../mobile/', import.meta.url).pathname

// The iOS project keeps its own copy, because Xcode reads an icon from inside the project.
test('the iOS app icon is the same document as the desktop one', () => {
  const desktop = join(UI_BUILD, 'icon.icon')
  const ios = join(MOBILE, 'ios/Bravebot/AppIcon.icon')
  assert.deepEqual(files(ios), files(desktop))
  for (const file of files(desktop)) assert.ok(readFileSync(join(ios, file)).equals(readFileSync(join(desktop, file))), `${file} differs`)
})

// The icon name is the document's file name without its extension, so a project that names one
// and ships the other builds with no icon and no error.
test('the iOS project names the icon it ships and builds it as a resource', () => {
  const project = readFileSync(join(MOBILE, 'ios/Bravebot.xcodeproj/project.pbxproj'), 'utf8')
  assert.ok(existsSync(join(MOBILE, 'ios/Bravebot/AppIcon.icon/icon.json')))
  const names = [...project.matchAll(/ASSETCATALOG_COMPILER_APPICON_NAME = (\w+);/g)].map((match) => match[1])
  assert.ok(names.length > 0)
  assert.ok(names.every((name) => name === 'AppIcon'), `the project names ${names}`)
  assert.match(project, /AppIcon\.icon in Resources/)
  assert.match(project, /lastKnownFileType = folder\.iconcomposer\.icon;[^}]*path = Bravebot\/AppIcon\.icon;/)
  assert.ok(!existsSync(join(MOBILE, 'ios/Bravebot/Images.xcassets/AppIcon.appiconset')), 'a second icon of the same name')
})
