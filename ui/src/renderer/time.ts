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
  return shortDate(new Date(then * 1000), now)
}

/** A day as the window writes one: "Oct 28", with the year only when it is not this one. */
export function shortDate(date: Date, now = Date.now()): string {
  const sameYear = date.getFullYear() === new Date(now).getFullYear()
  return date.toLocaleDateString('en-US', sameYear ? { month: 'short', day: 'numeric' } : { month: 'short', day: 'numeric', year: 'numeric' })
}

/** The day of a UTC timestamp the bridge sent, in this machine's time zone, as [`shortDate`] writes it. */
export function dayOf(iso: string, now = Date.now()): string {
  return shortDate(new Date(iso), now)
}
