import { pullRequestLink } from '../../shared/pull-request'

/**
 * The pull request a chat produced, as `PR #<number>` laid over the bottom right of its row.
 *
 * A link beside the row's button and not inside it, because a link inside a button is two controls
 * answering one click. `target="_blank"` is what the main process answers by opening the user's
 * browser, the same road a link in a reply takes, and the destination is in the tooltip for the
 * reason Markdown gives there. Nothing is drawn for an address that is not a pull request.
 */
export function PullRequestMark({ url }: { url: string | null | undefined }): React.JSX.Element | null {
  const link = pullRequestLink(url)
  if (!link) return null
  return <a className="session-pr num" href={link.href} target="_blank" rel="noopener noreferrer nofollow" data-tooltip={link.href}>{link.text}</a>
}

/**
 * Room for the link inside the branch line, so the branch ends in its ellipsis before the link.
 *
 * The link is laid over the row, outside the line it must not cover, so the line holds an unseen
 * copy of the link's own text in the link's own type: its width is the link's, however many digits
 * the number has.
 */
export function PullRequestRoom({ url }: { url: string | null | undefined }): React.JSX.Element | null {
  const link = pullRequestLink(url)
  if (!link) return null
  return <span className="session-pr-room num" aria-hidden="true">{link.text}</span>
}
