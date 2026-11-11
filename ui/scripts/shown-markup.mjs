// Reading server-rendered markup the way the shown tests need it: the text of it, run by run, each
// with the `data-deciding` of the nearest element marked, which is what says whether a row counts
// toward an approval (PROMPT-4).

export const VOID = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'source', 'track', 'wbr'])

// The modal portals into `document.body`, which a server render has none of, so the startup
// question is drawn inside a plain element: what is asserted is the question's own text.
export const withoutModal = {
  name: 'without-modal',
  setup(build) {
    build.onResolve({ filter: /\/Modal$/ }, () => ({ path: 'modal', namespace: 'stub' }))
    build.onLoad({ filter: /.*/, namespace: 'stub' }, () => ({
      contents: 'import React from "react"; export const Modal = ({ children, actions }) => React.createElement("div", null, children, actions)',
      loader: 'js',
      resolveDir: process.cwd(),
    }))
  },
}

// The Leo wrappers set `isDisabled` as a property on a custom element, which a server render does not
// write into markup, so the question is built with plain elements that draw it as `disabled`.
export const plainLeo = {
  name: 'plain-leo',
  setup(build) {
    build.onResolve({ filter: /\/nala$/ }, () => ({ path: 'nala', namespace: 'plain' }))
    build.onLoad({ filter: /.*/, namespace: 'plain' }, () => ({
      contents: `import React from "react"
        export const Button = ({ isDisabled, children, kind, size, ...rest }) => React.createElement("button", { ...rest, disabled: isDisabled }, children)
        export const Alert = ({ children }) => React.createElement("div", null, children)
        export const Icon = () => null`,
      loader: 'js',
      resolveDir: process.cwd(),
    }))
  },
}

/** The text of some markup, run by run, each with the `data-deciding` of the nearest element marked. */
export function texts(markup) {
  const open = []
  const runs = []
  let at = 0
  for (const found of markup.matchAll(/<(\/?)([a-zA-Z][\w-]*)([^>]*)>/g)) {
    if (found.index > at) runs.push({ text: markup.slice(at, found.index), deciding: open.findLast((element) => element.deciding)?.deciding ?? null })
    at = found.index + found[0].length
    const [, closing, name, attributes] = found
    if (closing) {
      const index = open.findLastIndex((element) => element.name === name)
      if (index >= 0) open.length = index
    } else if (!VOID.has(name)) {
      open.push({ name, deciding: attributes.match(/data-deciding="([a-z]+)"/)?.[1] ?? null })
    }
  }
  return runs
}
