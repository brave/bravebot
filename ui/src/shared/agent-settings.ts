export interface Hook { on: 'turn-started' | 'tool-finished' | 'turn-finished'; tool?: string; run: string[] }
export interface HooksDocument { path: string; text: string | null; hooks: Hook[] }
export interface AgentSettings {
  build: string; configured: boolean; problem: string | null; model: string | null
  brave: boolean; bedrock: boolean; providers: { name: string; credential: string }[]
  selected: string | null; layers: string[]; overrides: { name: string; path: string }[]
  managed: { path: string | null; keys: string[] }
  network: { roots: string[]; problem: string | null; trustsNothing: boolean; proxy: string | null; authenticated: boolean; unusableProxy: string | null; noProxy: string | null }
}

/** Reject unknown shapes instead of silently dropping commands during an edit. */
export function parseHooks(text: string): Hook[] {
  if (new TextEncoder().encode(text).length > 65536) throw new Error('Hooks must fit within 64 KB.')
  const document = JSON.parse(text)
  if (!document || typeof document !== 'object' || Array.isArray(document) || !Array.isArray(document.hooks)) throw new Error('Expected a hooks array.')
  if (Object.keys(document).some(key => key !== 'hooks')) throw new Error('This hooks file has extra settings. Edit it directly to preserve them.')
  return document.hooks.map((hook: unknown) => {
    if (!hook || typeof hook !== 'object' || Array.isArray(hook)) throw new Error('Each hook needs a lifecycle event and program.')
    const h = hook as Record<string, unknown>
    if (Object.keys(h).some(key => !['on', 'tool', 'run'].includes(key))) throw new Error('This hook has unsupported fields. Edit the file directly to preserve them.')
    if (!['turn-started', 'tool-finished', 'turn-finished'].includes(String(h.on))) throw new Error('Choose a supported lifecycle event.')
    if (!Array.isArray(h.run) || !h.run.length || h.run.some(word => typeof word !== 'string' || word.includes('\0')) || !h.run[0].trim()) throw new Error('Enter a program and separate text arguments.')
    if (h.tool !== undefined && (typeof h.tool !== 'string' || !h.tool.trim() || h.on !== 'tool-finished')) throw new Error('Tool filters only apply to tool completion hooks.')
    return { on: h.on, ...(h.tool ? { tool: h.tool } : {}), run: h.run } as Hook
  })
}
