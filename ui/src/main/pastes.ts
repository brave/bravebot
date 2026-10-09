/**
 * Grants for pictures a person pasted into the composer (PASTE-2).
 *
 * The preload hears a paste the browser marked trusted and asks for one; nothing else does. This
 * process then reads the operating system's clipboard itself, so the bytes are the clipboard's and
 * never anything the page said. They are written out again as PNG and named by the type the bridge
 * states for a re-encoded paste, never by something the content claims (PASTE-3). Whether a picture
 * is too large, the note that says so and the marker's noun are the bridge's too (`pastes.check`),
 * the ones the terminal stages a paste by, so this process holds no cap, type or wording of its
 * own. The page is told an opaque id bound to the session and a small drawing, never the bytes.
 *
 * At send the page names ids, and this process turns them into the bridge's `images` after the
 * window's own `images` has been thrown away (`sanitise.ts`). An id this session does not hold
 * refuses the send rather than starting a turn without the picture.
 */

import { randomUUID } from 'node:crypto'
import { clipboard, nativeImage } from 'electron'
import type { PasteOutcome } from '../shared/pastes'
import { rootForSession } from './files'
import { SendRefused } from './drops'

/** Thumbnail height in device pixels: a 20px chip drawn for a Retina screen, as a drop's is. */
const THUMBNAIL_HEIGHT = 40

/** More pictures than one message names; a longer list at send is not a draft's. */
const MOST_IN_ONE_SEND = 100

/** A pasted picture as PNG, and the type the bridge said to name it by. */
interface Grant {
  png: Buffer
  media: string
}

const grants = new Map<string, Map<string, Grant>>()

/**
 * What the bridge's `pastes.check` says about a picture of some size: the type to name it by and
 * the word its marker uses, or the note to show for one too large.
 */
type Checked = { ok: true; media: string; noun: string } | { ok: false; note: string }

/** Asks the bridge's `pastes.check` about a picture of `bytes`. */
export type Check = (bytes: number) => Promise<unknown>

/** The bridge's answer about a picture of `bytes`, or `null` when it gave none or not one. */
async function checked(check: Check, bytes: number): Promise<Checked | null> {
  let answer: unknown
  try {
    answer = await check(bytes)
  } catch {
    return null
  }
  const { ok, media, noun, note } = (answer ?? {}) as Record<string, unknown>
  if (ok === true && typeof media === 'string' && typeof noun === 'string') return { ok, media, noun }
  if (ok === false && typeof note === 'string') return { ok, note }
  return null
}

/**
 * The picture on the operating system's clipboard as PNG, or `null` when it holds none.
 *
 * The clipboard offers `image/png` for any picture it holds, converting a TIFF (a macOS screenshot
 * or a copy from Preview) on the way out.
 */
export async function clipboardPng(): Promise<Buffer | null> {
  for (const item of await clipboard.read()) {
    if (!item.types.includes('image/png')) continue
    const blob = await item.getType('image/png') as Blob
    return Buffer.from(await blob.arrayBuffer())
  }
  return null
}

/**
 * Grant the picture `copied` holds to `session`, or `null` when there is none to take.
 *
 * The picture is decoded and written out again as PNG, so what is sent is a PNG whatever the
 * clipboard said it was, and is named by the type the bridge states (PASTE-3). The bridge is asked
 * about its size as it came off the clipboard, so one too large is refused before it is decoded,
 * and again once re-encoded, since that is what is sent. A refusal carries the bridge's note. A
 * bridge that does not answer stages nothing. The decode is the one an operating system's clipboard read does
 * anyway, so the thumbnail is drawn here from pixels in hand, rather than handed to the renderer to
 * decode as a drop's is.
 */
export async function stagePaste(session: unknown, copied: Buffer | null, check: Check): Promise<PasteOutcome | null> {
  if (typeof session !== 'string' || rootForSession(session) === undefined) return null
  if (!copied || copied.length === 0) return null
  const copiedFits = await checked(check, copied.length)
  if (!copiedFits) return null
  if (!copiedFits.ok) return { kind: 'too-large', note: copiedFits.note }
  const image = nativeImage.createFromBuffer(copied)
  if (image.isEmpty()) return null
  const png = image.toPNG()
  if (png.length === 0) return null
  const fits = await checked(check, png.length)
  if (!fits) return null
  if (!fits.ok) return { kind: 'too-large', note: fits.note }
  // The session may have closed while the bridge answered, and its grants with it.
  if (rootForSession(session) === undefined) return null
  const held = grants.get(session) ?? new Map<string, Grant>()
  grants.set(session, held)
  const id = randomUUID()
  held.set(id, { png, media: fits.media })
  const small = image.resize({ height: Math.min(THUMBNAIL_HEIGHT, image.getSize().height), quality: 'good' })
  return { kind: 'staged', picture: { id, noun: fits.noun, thumbnail: small.isEmpty() ? undefined : small.toDataURL() } }
}

/**
 * The pictures `ids` grant in `session`, in the order given, as the bridge's `images` entries.
 *
 * Throws `SendRefused` for anything that is not a live grant of this session.
 */
export function pastesFor(session: string, ids: unknown): { media: string; data: string }[] {
  if (ids === undefined || ids === null) return []
  if (!Array.isArray(ids) || ids.length > MOST_IN_ONE_SEND) throw new SendRefused('The pasted pictures could not be read. Paste them again.')
  const held = grants.get(session)
  return ids.map((id) => {
    const grant = typeof id === 'string' ? held?.get(id) : undefined
    if (!grant) throw new SendRefused('A pasted picture is no longer available. Paste it again.')
    return { media: grant.media, data: grant.png.toString('base64') }
  })
}

/** `params` carrying the pictures `ids` grant as `images`, or left without the key when none. */
export function withPastes(session: string, ids: unknown, params: Record<string, unknown>): Record<string, unknown> {
  const images = pastesFor(session, ids)
  return images.length > 0 ? { ...params, images } : params
}

/** Forget a closed session's pictures. */
export function forgetPastes(session: string): void {
  grants.delete(session)
}
