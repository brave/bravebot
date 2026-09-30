/**
 * A path shortened in the middle, so the filename survives.
 *
 * Ellipsising on the right drops the one part of a path somebody is reading for. This keeps the
 * last segment whole and as much of the front as fits, and callers put the whole path in a
 * tooltip. A filename longer than the budget on its own is itself cut in the middle.
 */
export function middleTruncate(path: string, max: number): string {
  if (path.length <= max) return path
  if (max < 5) return `${path.slice(0, Math.max(0, max - 1))}…`
  const slash = path.lastIndexOf('/')
  const name = slash >= 0 ? path.slice(slash) : path
  if (name.length + 4 >= max) {
    const keep = max - 1
    const tail = Math.ceil(keep / 2)
    return `${path.slice(0, keep - tail)}…${path.slice(path.length - tail)}`
  }
  return `${path.slice(0, max - name.length - 1)}…${name}`
}
