import type { BridgeEvent } from '../shared/protocol'

export interface EventQueueParts {
  prepare: (message: BridgeEvent) => Promise<(() => void) | void>
  tell: (message: BridgeEvent) => void
  report: (error: unknown) => void
}

export function eventQueue({ prepare, tell, report }: EventQueueParts): (message: BridgeEvent) => void {
  const tails = new Map<string, Promise<void>>()
  return (message) => {
    const key = message.session ?? ''
    const run = (tails.get(key) ?? Promise.resolve()).then(async () => {
      let after: (() => void) | void = undefined
      try {
        after = await prepare(message)
      } catch (error) {
        report(error)
      }
      try {
        tell(message)
      } catch (error) {
        report(error)
      }
      // After the event, because both reach the same window in the order they are sent and an
      // announcement drawn above the turn that provoked it reads as the app interrupting.
      try {
        after?.()
      } catch (error) {
        report(error)
      }
    }).catch(() => undefined)
    tails.set(key, run)
    void run.finally(() => {
      if (tails.get(key) === run) tails.delete(key)
    })
  }
}
