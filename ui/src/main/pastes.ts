/**
 * Grants for pictures a person pasted into the composer (PASTE-2).
 *
 * The preload hears a paste the browser marked trusted and asks for one; nothing else does. This
 * process then reads the operating system's clipboard itself, so the bytes are the clipboard's and
 * never anything the page said. They are written out again as PNG, so the media type is this
 * process's literal and never something the content claims (PASTE-3). The page is told an opaque id
 * bound to the session and a small drawing, never the bytes.
 *
 * At send the page names ids, and this process turns them into the bridge's `images` after the
 * window's own `images` has been thrown away (`sanitise.ts`). An id this session does not hold
 * refuses the send rather than starting a turn without the picture.
 */

import { randomUUID } from 'node:crypto'
import { clipboard, nativeImage } from 'electron'
import { MAX_PASTED_BYTES, type PasteOutcome } from '../shared/pastes'
import { rootForSession } from './files'
import { SendRefused } from './drops'

/** Thumbnail height in device pixels: a 20px chip drawn for a Retina screen, as a drop's is. */
const THUMBNAIL_HEIGHT = 40

/** More pictures than one message names; a longer list at send is not a draft's. */
const MOST_IN_ONE_SEND = 100

const grants = new Map<string, Map<string, Buffer>>()

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
 * clipboard said it was, and is named by this process's literal (PASTE-3). One over the cap as it
 * came off the clipboard is refused before it is decoded. The decode is the one an operating
 * system's clipboard read does anyway, so the thumbnail is drawn here from pixels in hand, rather
 * than handed to the renderer to decode as a drop's is.
 */
export function stagePaste(session: unknown, copied: Buffer | null): PasteOutcome | null {
  if (typeof session !== 'string' || rootForSession(session) === undefined) return null
  if (!copied || copied.length === 0) return null
  if (copied.length > MAX_PASTED_BYTES) return { kind: 'too-large', size: copied.length, limit: MAX_PASTED_BYTES }
  const image = nativeImage.createFromBuffer(copied)
  if (image.isEmpty()) return null
  const png = image.toPNG()
  if (png.length === 0) return null
  if (png.length > MAX_PASTED_BYTES) return { kind: 'too-large', size: png.length, limit: MAX_PASTED_BYTES }
  const held = grants.get(session) ?? new Map<string, Buffer>()
  grants.set(session, held)
  const id = randomUUID()
  held.set(id, png)
  const small = image.resize({ height: Math.min(THUMBNAIL_HEIGHT, image.getSize().height), quality: 'good' })
  return { kind: 'staged', picture: { id, thumbnail: small.isEmpty() ? undefined : small.toDataURL() } }
}

/**
 * The pictures `ids` grant in `session`, in the order given, as the bridge's `images` entries.
 *
 * Throws `SendRefused` for anything that is not a live grant of this session.
 */
export function pastesFor(session: string, ids: unknown): { media: 'image/png'; data: string }[] {
  if (ids === undefined || ids === null) return []
  if (!Array.isArray(ids) || ids.length > MOST_IN_ONE_SEND) throw new SendRefused('The pasted pictures could not be read. Paste them again.')
  const held = grants.get(session)
  return ids.map((id) => {
    const png = typeof id === 'string' ? held?.get(id) : undefined
    if (!png) throw new SendRefused('A pasted picture is no longer available. Paste it again.')
    return { media: 'image/png', data: png.toString('base64') }
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
