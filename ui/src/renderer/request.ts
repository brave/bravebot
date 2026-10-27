/** One bridge method, answered or thrown with the agent's own message. */
export async function call<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  const answer = await window.bravebot.request<T>(method, params)
  if (answer.error) throw new Error(answer.error.message)
  return answer.ok as T
}

/** What an `ipcRenderer.invoke` rejection says, without Electron's "Error invoking remote method" prefix. */
export function invokeMessage(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error)
  return text.replace(/^Error invoking remote method '[^']*': (?:Error: )?/, '')
}
