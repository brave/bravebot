/**
 * The marks for the panels in the context column.
 *
 * Drawn rather than typed, for the reason `ForkIcon` gives about its own: the characters that
 * would do this job are hairlines at the size they would be used at. Five of them sit in one row
 * of connected buttons, so they have to be told apart at a glance and at twelve pixels — which is
 * why each is one idea (a list, a page, a pencil, a lock, a folder) and none is a scene.
 *
 * `currentColor` throughout, so a pressed button colours its mark by colouring itself, and always
 * `aria-hidden`: every button carries the panel's name in its `title` and its accessible label,
 * and a mark that announced itself as well would say it twice.
 */

import type { PanelName } from '../../shared/state'

const PATHS: Record<PanelName, React.ReactNode> = {
  // A list with the first thing on it done, which is what a plan in this window looks like.
  plan: (
    <>
      <path d="M2.5 5.2l1.4 1.4L6.5 3.5" />
      <path d="M8.5 5.5h5" />
      <path d="M2.5 10h11" />
      <path d="M2.5 13.2h7" />
    </>
  ),
  // A page with its corner turned: something read rather than something written.
  read: (
    <>
      <path d="M4 2h5l3.2 3.2V14H4z" />
      <path d="M9 2v3.4h3.2" />
    </>
  ),
  // A pencil. The one mark here that says an action rather than a thing, because the panel is
  // about what the turn *did* to a file.
  writes: (
    <>
      <path d="M11.4 2.6l2 2-7.6 7.6-2.7.7.7-2.7z" />
      <path d="M10 4l2 2" />
    </>
  ),
  // A padlock, shut. Confined content is content this window will show and not release.
  confined: (
    <>
      <path d="M3.8 7.4h8.4V14H3.8z" />
      <path d="M6 7.4V5.8a2 2 0 0 1 4 0v1.6" />
    </>
  ),
  // A folder, for the one panel that is about the disk rather than the transcript.
  files: (
    <>
      <path d="M2.2 4.4h4l1.6 2h6V13H2.2z" />
    </>
  ),
}

export function PanelIcon({
  panel,
  size = 13,
}: {
  panel: PanelName
  size?: number
}): React.JSX.Element {
  return (
    <svg
      className="panel-icon"
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {PATHS[panel]}
    </svg>
  )
}
