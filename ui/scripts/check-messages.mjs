/**
 * A message is named in the source, never computed (LOCALE-2), and only `i18n/` holds the catalogs.
 * `name` is the file's path under `src/renderer`. Answers each offence as `{ line, message }`.
 */
export function messageProblems(name, text) {
  if (name.startsWith('i18n/')) return []
  const usesCatalog = /from '(?:\.\.?\/)+i18n'/.test(text)
  const problems = []
  text.split('\n').forEach((line, i) => {
    const code = line.replace(/\/\/.*$/, '')
    const trimmed = code.trim()
    if (trimmed.startsWith('*') || trimmed.startsWith('/*') || trimmed.startsWith('{/*')) return
    if (usesCatalog && /\bt\s*\[/.test(code)) problems.push({ line: i + 1, message: `message looked up by a computed key; write t.<name>(): ${trimmed.slice(0, 80)}` })
    if (/messages\.gen/.test(code)) problems.push({ line: i + 1, message: 'the catalogs are reached through i18n, by t.<name>()' })
  })
  return problems
}
