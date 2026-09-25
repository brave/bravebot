// Guards that the shadcn migration cannot slide backwards: no raw native controls in
// application components, and no leftover stylesheets besides the two that remain.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

function walk(directory, testFile) {
  const files = []
  for (const item of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, item.name)
    if (item.isDirectory()) files.push(...walk(path, testFile))
    else if (testFile(item.name, path)) files.push(path)
  }
  return files
}

test('application components use no raw native controls', () => {
  const files = walk('src/renderer/components', (name, path) =>
    name.endsWith('.tsx') && !path.includes('/ui/'))
  assert.ok(files.length > 10, `only ${files.length} component files scanned`)
  const raw = /<(button|input|select|textarea|details|summary)[\s>]/
  for (const path of files) {
    const source = readFileSync(path, 'utf8')
    assert.ok(!raw.test(source), `${path} still has a raw native control`)
  }
})

test('only shadcn.css and export.css are imported as stylesheets', () => {
  const files = walk('src', (name) => /\.(ts|tsx|css)$/.test(name))
  const allowed = new Set(['shadcn.css', 'export.css'])
  for (const path of files) {
    const source = readFileSync(path, 'utf8')
    for (const match of source.matchAll(/import\s+['"]([^'"]+\.css)['"]/g)) {
      const name = match[1].split('/').pop()
      if (name.startsWith('tailwindcss') || name === 'tw-animate-css') continue
      if (name.endsWith('theme.css') || name.endsWith('utilities.css')) continue
      assert.ok(allowed.has(name), `${path} imports leftover stylesheet ${match[1]}`)
    }
  }
})

test('overlays declare an accessible title', () => {
  const files = {
    'src/renderer/components/TrustPrompt.tsx': 'AlertDialogTitle',
    'src/renderer/components/Modal.tsx': 'DialogTitle',
    'src/renderer/App.tsx': 'SheetTitle',
  }
  for (const [path, needle] of Object.entries(files)) {
    assert.ok(readFileSync(path, 'utf8').includes(needle), `${path} is missing ${needle}`)
  }
})
