/**
 * One hook, as the agent read it. `firesForNothing` is its answer about an entry naming a tool on a
 * moment that has no tool call, and is not worked out again here: what a hook is has one reader.
 */
export interface Hook { on: 'turn-started' | 'tool-finished' | 'turn-finished'; tool: string | null; run: string[]; firesForNothing: boolean }
/** `entire` is false where the agent passed over part of the file, so composing it back from
 * `hooks` alone would drop what it did not read. */
export interface HooksDocument { path: string; text: string | null; entire: boolean; hooks: Hook[] }
export interface AgentSettings {
  build: string; configured: boolean; problem: string | null; model: string | null
  brave: boolean; bedrock: boolean; providers: { name: string; credential: string }[]
  selected: string | null; layers: string[]; overrides: { name: string; path: string }[]
  managed: { path: string | null; keys: string[] }
  network: { roots: string[]; problem: string | null; trustsNothing: boolean; proxy: string | null; authenticated: boolean; unusableProxy: string | null; noProxy: string | null }
}

/** The file the agent will read back, composed from the entries an editor holds. */
export function composeHooks(hooks: Hook[]): string {
  const entries = hooks.map(({ on, tool, run }) => ({ on, ...(tool ? { tool } : {}), run }))
  return JSON.stringify({ hooks: entries }, null, 2) + '\n'
}
