import { useEffect, useRef, useState } from 'react'
import type { KeyWay } from '../shared/accounts'

/** A row's key window, open or asked for. */
export interface KeyTarget { way: KeyWay; id: string | null }

export interface KeyResult extends KeyTarget {
  outcome: 'stored' | 'failed'
  sentence: string | null
}

export const sameKeyTarget = (a: KeyTarget | null, way: string, id: string | null) =>
  a !== null && a.way === way && (a.way === 'brave' || a.id === id)

/**
 * The window's side of entering a key (PREM-16, CRED-24): an ask, then what the main process says
 * of its key window. The page never holds the key; the notices do not carry it.
 */
export function useKeyEntry(onStored?: () => void, session?: string) {
  const [open, setOpen] = useState<KeyTarget | null>(null)
  const [asking, setAsking] = useState<KeyTarget | null>(null)
  const [result, setResult] = useState<KeyResult | null>(null)
  const stored = useRef(onStored)
  stored.current = onStored
  useEffect(() => {
    let live = true
    const off = window.bravebot.onKeyEntry((notice) => {
      if (notice.kind === 'opened') { setOpen({ way: notice.way, id: notice.id }); return }
      setOpen(null)
      if (notice.outcome === 'stored') {
        setResult({ way: notice.way, id: notice.id, outcome: 'stored', sentence: notice.sentence })
        stored.current?.()
      }
    })
    void window.bravebot.enterKeyCurrent().then((current) => { if (live && current) setOpen((now) => now ?? current) })
    return () => { live = false; off() }
  }, [])

  const start = async (way: KeyWay, id: string | null) => {
    setResult(null)
    setAsking({ way, id })
    try {
      const answer = await window.bravebot.enterKey(way, id, session)
      if (answer && 'error' in answer) setResult({ way, id, outcome: 'failed', sentence: answer.error })
    } catch (error) {
      setResult({ way, id, outcome: 'failed', sentence: String(error) })
    } finally {
      setAsking(null)
    }
  }
  const show = () => { void window.bravebot.showKeyWindow() }
  return { open, asking, result, busy: asking !== null || open !== null, start, show, dismiss: () => setResult(null) }
}
