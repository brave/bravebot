/**
 * What kind of thing a filename is, in the width of two characters.
 *
 * Letters rather than pictograms, and for a measured reason: this app has no icon set, the one
 * drawing in it is `ForkIcon`, and a tree row is eleven pixels tall. A hand-drawn camera at that
 * size is a grey smudge, where `PNG` is the thing itself — and the extension is what the person
 * already reads the name for. Colour carries the family so a folder of one kind of file reads as
 * a block at a glance; the letters carry the detail when they look closer.
 *
 * Every colour is a token this window already uses. Nothing new was invented for a badge.
 */

/** Which family each extension belongs to. Extensions, not names: a name is not a type. */
const FAMILIES: Record<string, string> = {
  ts: 'code', tsx: 'code', js: 'code', jsx: 'code', mjs: 'code', cjs: 'code', rs: 'code',
  py: 'code', go: 'code', rb: 'code', php: 'code', java: 'code', kt: 'code', swift: 'code',
  c: 'code', h: 'code', cc: 'code', cpp: 'code', hpp: 'code', m: 'code', mm: 'code',
  sh: 'code', bash: 'code', zsh: 'code', fish: 'code', sql: 'code', lua: 'code', vim: 'code',

  json: 'data', yaml: 'data', yml: 'data', toml: 'data', ini: 'data', conf: 'data', cfg: 'data',
  csv: 'data', tsv: 'data', lock: 'data', xml: 'data', plist: 'data', env: 'data', db: 'data',

  html: 'markup', htm: 'markup', css: 'markup', scss: 'markup', sass: 'markup', less: 'markup',

  md: 'doc', markdown: 'doc', txt: 'doc', rst: 'doc', pdf: 'doc', rtf: 'doc', doc: 'doc',
  docx: 'doc', pages: 'doc', log: 'doc',

  png: 'image', jpg: 'image', jpeg: 'image', gif: 'image', svg: 'image', webp: 'image',
  ico: 'image', icns: 'image', heic: 'image', tiff: 'image', bmp: 'image', psd: 'image',

  mp3: 'media', wav: 'media', m4a: 'media', aiff: 'media', flac: 'media', mp4: 'media',
  mov: 'media', webm: 'media', avi: 'media', mkv: 'media',

  zip: 'archive', tar: 'archive', gz: 'archive', tgz: 'archive', bz2: 'archive', xz: 'archive',
  zst: 'archive', rar: 'archive', dmg: 'archive', pkg: 'archive', whl: 'archive', jar: 'archive',
}

/**
 * The extension, or `''` for a name that has none.
 *
 * The last dot, and never the first character: `.gitignore` is a dotfile rather than a file of
 * type `gitignore`, and labelling it `GIT` would be this badge inventing a family. `foo.tar.gz`
 * is an archive by its last segment, which is the one that says how to open it.
 */
function extensionOf(name: string): string {
  const dot = name.lastIndexOf('.')
  if (dot <= 0 || dot === name.length - 1) return ''
  return name.slice(dot + 1).toLowerCase()
}

/** The badge for one filename: three letters at most, and the family that colours them. */
export function glyphOf(name: string): { label: string; family: string } {
  const extension = extensionOf(name)
  if (extension === '') return { label: '·', family: 'plain' }
  return {
    // An extension this table has never heard of still shows its own letters. The colour says
    // "not a family I know"; blanking the label would throw away the one true thing about it.
    label: extension.slice(0, 3).toUpperCase(),
    family: FAMILIES[extension] ?? 'plain',
  }
}

/**
 * The badge beside a row.
 *
 * `aria-hidden`, like every other glyph in this window: it is a second rendering of the
 * extension, which is already in the name being read out, and a screen reader announcing
 * "TS index dot ts" would be the tree saying it twice.
 */
export function FileGlyph({ name }: { name: string }): React.JSX.Element {
  const { label, family } = glyphOf(name)
  return (
    <span className={`tree-glyph ${family}`} aria-hidden="true">
      {label}
    </span>
  )
}
