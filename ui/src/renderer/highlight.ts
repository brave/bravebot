import { createElement, type ReactNode } from 'react'
import { createLowlight } from 'lowlight'
import type { Root, RootContent } from 'hast'
import bash from 'highlight.js/lib/languages/bash'
import c from 'highlight.js/lib/languages/c'
import cpp from 'highlight.js/lib/languages/cpp'
import css from 'highlight.js/lib/languages/css'
import diff from 'highlight.js/lib/languages/diff'
import go from 'highlight.js/lib/languages/go'
import ini from 'highlight.js/lib/languages/ini'
import java from 'highlight.js/lib/languages/java'
import javascript from 'highlight.js/lib/languages/javascript'
import json from 'highlight.js/lib/languages/json'
import markdown from 'highlight.js/lib/languages/markdown'
import python from 'highlight.js/lib/languages/python'
import rust from 'highlight.js/lib/languages/rust'
import shell from 'highlight.js/lib/languages/shell'
import sql from 'highlight.js/lib/languages/sql'
import swift from 'highlight.js/lib/languages/swift'
import typescript from 'highlight.js/lib/languages/typescript'
import xml from 'highlight.js/lib/languages/xml'
import yaml from 'highlight.js/lib/languages/yaml'

/**
 * The grammars this window colours, shared by fenced code in a reply and by a proposed diff so
 * the two never disagree about what a keyword looks like. A subset, because every grammar is
 * bundled whether or not anybody writes in it.
 */
export const LANGUAGES = { bash, c, cpp, css, diff, go, ini, java, javascript, json, markdown, python, rust, shell, sql, swift, typescript, xml, yaml }
export const ALIASES = { bash: ['shell-script'], ini: ['toml'], json: ['jsonc', 'json5'], typescript: ['ts', 'tsx'], javascript: ['js', 'jsx'] }
export const PLAIN_TEXT = ['text', 'txt', 'plain', 'plaintext', 'output', 'log']

const lowlight = createLowlight(LANGUAGES)
lowlight.registerAlias(ALIASES)

const EXTENSIONS: Record<string, string> = {
  ts: 'typescript', tsx: 'typescript', mts: 'typescript', cts: 'typescript',
  js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript',
  rs: 'rust', py: 'python', go: 'go', java: 'java', swift: 'swift', sql: 'sql',
  c: 'c', h: 'c', cc: 'cpp', cpp: 'cpp', cxx: 'cpp', hpp: 'cpp',
  css: 'css', json: 'json', md: 'markdown', markdown: 'markdown',
  sh: 'bash', bash: 'bash', zsh: 'bash', yml: 'yaml', yaml: 'yaml', toml: 'ini', ini: 'ini',
  html: 'xml', xml: 'xml', svg: 'xml', diff: 'diff', patch: 'diff',
}

/** The grammar for a path, by its extension; null for anything this window does not colour. */
export function languageOf(path: string): string | null {
  const name = path.split(/[\\/]/).pop() ?? ''
  const dot = name.lastIndexOf('.')
  return dot > 0 ? EXTENSIONS[name.slice(dot + 1).toLowerCase()] ?? null : null
}

function draw(nodes: RootContent[], key: string): ReactNode[] {
  return nodes.map((node, index) => {
    if (node.type === 'text') return node.value
    if (node.type !== 'element') return null
    const names = node.properties?.className
    const className = Array.isArray(names) ? names.join(' ') : undefined
    return createElement('span', { key: `${key}.${index}`, className }, ...draw(node.children, `${key}.${index}`))
  })
}

/**
 * One line of source, coloured, as React elements.
 *
 * The grammar's tree is walked into spans with class names and text children, so nothing here
 * turns a string into markup: the bytes stay text whatever they spell.
 */
export function highlightLine(text: string, language: string | null): ReactNode {
  if (!language || !text) return text
  let tree: Root
  try { tree = lowlight.highlight(language, text) } catch { return text }
  return draw(tree.children, 'h')
}
