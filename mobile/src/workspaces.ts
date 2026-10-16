import { displayString } from './text/displayText'

/** The name a workspace is shown by, or its id when the target does not list it. */
export function workspaceName(workspaces: readonly { id: string; name: string }[], id: string): string {
  return workspaces.find((candidate) => candidate.id === id)?.name ?? displayString(id)
}
