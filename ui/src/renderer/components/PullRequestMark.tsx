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
