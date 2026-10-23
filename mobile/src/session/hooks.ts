import { useCallback, useSyncExternalStore } from 'react'
import type { AgentSession, ViewState } from '@brave/agent-client'
import type { SessionController } from './controller'

/** The session's latest view. The client replaces the view on every change, so identity is the change signal. */
export function useSessionView(session: AgentSession): ViewState {
  const subscribe = useCallback((onChange: () => void) => session.subscribe(() => onChange()), [session])
  return useSyncExternalStore(subscribe, () => session.view)
}

/** Re-render when the controller's client-local state changes, and return it. */
export function useController(controller: SessionController): SessionController {
  useSyncExternalStore(controller.subscribe, controller.getVersion)
  return controller
}
