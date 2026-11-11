import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { readdirSync, readFileSync } from 'node:fs'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { rendererUrl } = load('src/main/renderer-url.ts')

test('a packaged app ignores the page the environment names', () => {
  assert.equal(rendererUrl({ isPackaged: true }, { ELECTRON_RENDERER_URL: 'file:///scratch/page.html' }), null)
  assert.equal(rendererUrl({ isPackaged: true }, { ELECTRON_RENDERER_URL: 'http://localhost:5173' }), null)
})

test('an app run from source loads the development server the build tool names', () => {
  assert.equal(rendererUrl({ isPackaged: false }, { ELECTRON_RENDERER_URL: 'http://localhost:5173' }), 'http://localhost:5173')
})

test('an app run from source with nothing named loads its own files', () => {
  assert.equal(rendererUrl({ isPackaged: false }, {}), null)
  assert.equal(rendererUrl({ isPackaged: false }, { ELECTRON_RENDERER_URL: '' }), null)
})

test('no window in the main process reads the variable except through rendererUrl', () => {
  const reading = readdirSync('src/main')
    .filter((name) => name.endsWith('.ts') && name !== 'renderer-url.ts')
    .filter((name) =>
      readFileSync(`src/main/${name}`, 'utf8')
        .split('\n')
        .some((line) => line.includes('ELECTRON_RENDERER_URL') && !/^\s*(\/\/|\*|\/\*)/.test(line)),
    )
  assert.deepEqual(reading, [])
})
