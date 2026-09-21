/** Model choices belong to this UI's conversations, not the CLI's global preference. */
export function modelKey(directory: string, id: string): string {
  return `bravebot.conversation-model:${JSON.stringify([directory, id])}`
}

export function conversationModel(directory: string, id: string, fallback: string | null): string | null {
  try {
    return localStorage.getItem(modelKey(directory, id)) || fallback
  } catch {
    return fallback
  }
}

export function rememberModel(directory: string, id: string, model: string | null): void {
  try {
    if (model) localStorage.setItem(modelKey(directory, id), model)
    else localStorage.removeItem(modelKey(directory, id))
  } catch {
    // A full or unavailable preference store must not prevent sending a prompt.
  }
}
