import { useEffect, useMemo, useState, type SetStateAction } from 'react'
import { AuditInspector } from './components/AuditInspector'
import { useEvent } from './hooks'
import type { Live } from './live'
import type { TurnDisclosure } from './turn-details'

interface Audit {
  handle: string
  turn: number | null
  trigger: HTMLButtonElement
  wasFolded: boolean
}

/**
 * The audit panel of the open session: which turn it shows, opening and closing it, and the
 * disclosure state of a turn's details.
 *
 * `collapsed` and `toggle` are the right column's, since the panel is drawn there and a column
 * the person had folded is unfolded for it and folded again on close.
 */
export function useAudit(
  live: Live | null,
  collapsed: { right: boolean },
  toggle: (side: 'right') => void,
  updateSession: (handle: string, action: SetStateAction<Live | null>) => void,
) {
  const [audit, setAudit] = useState<Audit | null>(null)
  useEffect(() => { setAudit(null) }, [live?.handle])
  const selectedAudit = audit?.handle === live?.handle ? audit : null
  const openAudit = (turn: number | null, trigger: HTMLButtonElement) => {
    if (!live) return
    setAudit({ handle: live.handle, turn, trigger, wasFolded: selectedAudit?.wasFolded ?? collapsed.right })
    if (collapsed.right) toggle('right')
  }
  const closeAudit = () => {
    if (selectedAudit?.wasFolded && !collapsed.right) toggle('right')
    setAudit(null)
    // The live trigger disappears at completion; return to that turn's new footer instead.
    const trigger = selectedAudit?.trigger.isConnected ? selectedAudit.trigger :
      selectedAudit && selectedAudit.turn !== null ? document.querySelector<HTMLButtonElement>(`.turn-footer [data-audit-turn="${selectedAudit.turn}"]`) : null
    trigger?.focus({ preventScroll: true })
  }
  const discloseTurn = (turn: number, field: TurnDisclosure, open: boolean) => {
    if (!live) return
    updateSession(live.handle, (old) => old?.turns[turn] ? { ...old, turns: { ...old.turns, [turn]: { ...old.turns[turn]!, [field]: open } } } : old)
  }

  const stableCloseAudit = useEvent(closeAudit)
  const auditTurn = selectedAudit && selectedAudit.turn !== null ? live?.turns[selectedAudit.turn] : undefined
  const auditPanel = useMemo(() => selectedAudit
    ? <AuditInspector key={`${selectedAudit.handle}:${selectedAudit.turn}`} details={auditTurn} onClose={stableCloseAudit} />
    : null, [selectedAudit, auditTurn, stableCloseAudit])

  return { selectedAudit, openAudit, discloseTurn, auditPanel }
}
