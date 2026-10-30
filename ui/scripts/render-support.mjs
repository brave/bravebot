// Bundling window code for a test that renders it through `react-dom/server`.

import { build, buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)

function evaluate(source) {
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

/** `Modal` draws through a portal, which `react-dom/server` cannot render, so it is drawn in place. */
export const inPlaceModal = {
  name: 'in-place-modal',
  setup(built) {
    built.onResolve({ filter: /^\.\/Modal$/ }, () => ({ path: 'modal', namespace: 'in-place-modal' }))
    built.onLoad({ filter: /.*/, namespace: 'in-place-modal' }, () => ({
      loader: 'js',
      resolveDir: process.cwd(),
      contents: `import { createElement } from 'react'
        export function Modal({ title, subtitle, children, actions }) { return createElement('div', { className: 'modal', 'data-title': title, 'data-subtitle': subtitle }, children, actions) }`,
    }))
  },
}

/** A module with no JSX and nothing to stand in for, such as the main process's own rules. */
export function loadSync(path) {
  return evaluate(buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text)
}

/** Window components, from one entry file or from `stdin` source, with React left to the test. */
export async function loadComponents(entry, plugins = [inPlaceModal]) {
  const built = await build({
    ...(typeof entry === 'string' ? { entryPoints: [entry] } : { stdin: { ...entry, resolveDir: process.cwd(), loader: 'ts' } }),
    bundle: true, write: false, platform: 'node', format: 'cjs', jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
    plugins,
  })
  return evaluate(built.outputFiles[0].text)
}
