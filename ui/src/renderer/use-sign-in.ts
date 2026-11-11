import { useEffect, useRef, useState } from 'react'
import type { Attempt, SignInOutcome, SignInWay } from '../shared/accounts'

/** A sign-in the main process is running. */
export interface Waiting {
  attempt: Attempt
  way: SignInWay
  /** The row's id: an AWS account's profile, `null` for its default chain and the other ways. */
  id: string | null
  started: boolean
  port: number | null
  /** When the sign-in stops waiting, in this window's clock. */
  until: number | null
}

export interface SignInResult {
  way: SignInWay | null
  id: string | null
  outcome: SignInOutcome
  sentence: string | null
}

const deadline = (expiresIn: number | null) => expiresIn === null ? null : Date.now() + expiresIn * 1000

/**
 * The window's side of a sign-in (CHATGPT-15, XAI-20, BACKEND-70): an ask, then what the main process says of
 * it. The page never holds the address, the code or the URL; the notices do not carry them.
 *
 * An `ended` can come before the ask is answered, or with no `started` at all, so an attempt that
 * has ended is remembered and its answer then shows nothing.
 */
export function useSignIn(onSignedIn?: () => void, session?: string) {
  const [waiting, setWaiting] = useState<Waiting | null>(null)
  const [asking, setAsking] = useState<SignInWay | null>(null)
  const [result, setResult] = useState<SignInResult | null>(null)
  const ended = useRef(new Set<string>())
  const signedIn = useRef(onSignedIn)
  signedIn.current = onSignedIn
  useEffect(() => {
    let live = true
    const off = window.bravebot.onSignIn((notice) => {
      if (notice.kind === 'started') {
        setWaiting({ attempt: notice.attempt, way: notice.way, id: notice.id, started: true, port: notice.port, until: deadline(notice.expiresIn) })
        return
      }
      ended.current.add(String(notice.attempt))
      setWaiting((now) => now?.attempt === notice.attempt ? null : now)
      setResult((before) => notice.way
        ? { way: notice.way, id: notice.id, outcome: notice.outcome, sentence: notice.sentence }
        : { way: before?.way ?? null, id: before?.id ?? null, outcome: notice.outcome, sentence: notice.sentence })
      if (notice.outcome === 'signedIn') signedIn.current?.()
    })
    void window.bravebot.signInCurrent().then((current) => {
      if (!live || !current || ended.current.has(String(current.attempt))) return
      setWaiting((now) => now ?? { attempt: current.attempt, way: current.way, id: current.id, started: current.started, port: current.port, until: deadline(current.expiresIn) })
    })
    return () => { live = false; off() }
  }, [])

  const start = async (way: SignInWay, id: string | null = null) => {
    setResult(null)
    setAsking(way)
    try {
      const answer = await window.bravebot.signIn(way, id, session)
      if (!answer) return
      if ('error' in answer) { setResult({ way, id, outcome: 'failed', sentence: answer.error }); return }
      if (ended.current.has(String(answer.attempt))) return
      setWaiting((now) => now?.attempt === answer.attempt ? now : { attempt: answer.attempt, way, id, started: false, port: null, until: null })
    } catch (error) {
      setResult({ way, id, outcome: 'failed', sentence: String(error) })
    } finally {
      setAsking(null)
    }
  }
  const cancel = () => { if (waiting) void window.bravebot.cancelSignIn(waiting.attempt) }
  const show = () => { void window.bravebot.showSignInWindow() }
  return { waiting, asking, result, busy: asking !== null || waiting !== null, start, cancel, show, dismiss: () => setResult(null) }
}
