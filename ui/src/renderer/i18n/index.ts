/**
 * The window's words, in the catalog chosen once before the first paint (LOCALE-7).
 *
 * `t.uiSettingsTheme()` names a message the compiler checks, and there is no lookup by a computed
 * key (LOCALE-2): `scripts/check-nala.mjs` refuses `t[` and any reach into the catalogs outside
 * this directory. The text is generated from `crates/i18n/locales/*.ftl` (LOCALE-6).
 */
import { CATALOGS, REFERENCE, type Messages, type Tag } from './messages.gen'

export type { Tag } from './messages.gen'

let chosen = false

/** The messages of the catalog in use. */
export let t: Messages = CATALOGS[REFERENCE]

/** The tag of the catalog in use. */
export let locale: Tag = REFERENCE

const shipped = (tag: string): tag is Tag => Object.prototype.hasOwnProperty.call(CATALOGS, tag)

/** Fix the catalog for the life of the window. The first call wins; a tag nothing shipped is the reference. */
export function setLocale(tag: string): void {
  if (chosen) return
  chosen = true
  locale = shipped(tag) ? tag : REFERENCE
  t = CATALOGS[locale]
  document.documentElement.lang = locale
}
