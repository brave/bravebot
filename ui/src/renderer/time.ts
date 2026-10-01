/**
 * How long ago, as short as a list row can hold it: "now", "4m", "3h", "5d", then a date. The
 * year only when it is not this one. `then` is in seconds, the unit the agent stamps sessions in.
 */
export function shortAgo(then: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.floor(now / 1000) - then)
  if (seconds < 60) return 'now'
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`
  if (seconds < 7 * 86400) return `${Math.floor(seconds / 86400)}d`
  const date = new Date(then * 1000)
  const sameYear = date.getFullYear() === new Date(now).getFullYear()
  return date.toLocaleDateString('en-US', sameYear ? { month: 'short', day: 'numeric' } : { month: 'short', day: 'numeric', year: 'numeric' })
}
