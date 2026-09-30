import { useEffect, useState } from 'react'
import type { ModelCatalogue } from '../shared/protocol'

/**
 * How many tokens the session's model can hold, for the composer's context meter.
 *
 * The catalogue is the model picker's; whichever of the two asks first keeps it here, per session,
 * so opening the picker and drawing the meter cost one `models.list` between them. A model the
 * catalogue does not know, or one it gives no window for, is `null`: the meter then says the size
 * of the last request and draws no fraction of anything.
 */
const catalogues = new Map<string, ModelCatalogue>()
const asking = new Map<string, Promise<ModelCatalogue | null>>()
const listeners = new Set<() => void>()

export function rememberCatalogue(session: string | undefined, catalogue: ModelCatalogue | null): void {
  if (!session || !catalogue) return
  catalogues.set(session, catalogue)
  for (const listener of listeners) listener()
}

function catalogueFor(session: string): Promise<ModelCatalogue | null> {
  const known = catalogues.get(session)
  if (known) return Promise.resolve(known)
  let pending = asking.get(session)
  if (!pending) {
    pending = window.bravebot.request<ModelCatalogue>('models.list', { session })
      .then((answer) => { const catalogue = answer.ok ?? null; rememberCatalogue(session, catalogue); return catalogue })
      .catch(() => null)
    asking.set(session, pending)
  }
  return pending
}

/** Asked only once there is a measurement to compare: an unmeasured session needs no window. */
export function useContextWindow(session: string, model: string | null, measured: boolean): number | null {
  const read = (): number | null => {
    const catalogue = catalogues.get(session)
    const id = model ?? catalogue?.defaultModel
    return catalogue?.models.find((row) => row.id === id)?.contextWindow ?? null
  }
  const [size, setSize] = useState(read)
  useEffect(() => {
    const update = () => setSize(read())
    listeners.add(update)
    update()
    if (measured && !catalogues.has(session)) void catalogueFor(session)
    return () => { listeners.delete(update) }
  }, [session, model, measured])
  return size
}
