/**
 * What a sidebar row shows of the pull request a chat is linked to.
 *
 * The URL is a string the record holds, so it is read here and nowhere nearer the agent: a row
 * draws a link only for an `http` or `https` address whose path names a pull request, and the text
 * is the number alone, never the URL or a title.
 */
export interface PullRequestLink {
  /** `PR #4309`. */
  text: string
  /** The address the link opens, exactly as recorded. */
  href: string
}

export function pullRequestLink(url: string | null | undefined): PullRequestLink | null {
  if (!url) return null
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return null
  }
  if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:') return null
  const number = /\/pull\/(\d+)(?:\/|$)/.exec(parsed.pathname)?.[1]
  return number ? { text: `PR #${number}`, href: url } : null
}
