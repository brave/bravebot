import { displayString } from './text/displayText'

/** The name a workspace is shown by, or its id when the target does not list it, with characters that hide or reorder text written out. */
export function workspaceName(workspaces: readonly { id: string; name: string }[], id: string): string {
  return displayString(workspaces.find((candidate) => candidate.id === id)?.name ?? id)
}
