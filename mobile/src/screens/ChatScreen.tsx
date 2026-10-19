import { useEffect, useMemo, useState } from 'react'
import { AccessibilityInfo, Keyboard, KeyboardAvoidingView, Platform, Pressable, StyleSheet, Text, View } from 'react-native'
import type { AskAnswer, Row, ViewEnd } from '@brave/agent-client'
import { ApprovalPanel } from '../components/approvals/ApprovalPanel'
import { ConversationCard } from '../components/ConversationCard'
import { CallsDrawer } from '../components/CallsDrawer'
import { Composer } from '../components/Composer'
import { Body, Caption } from '../components/primitives'
import { TopBar } from '../components/TopBar'
import { Transcript } from '../components/transcript/Transcript'
import { callsFrom, turnIsRunning } from '../calls'
import type { SessionController } from '../session/controller'
import { readPromptText } from '../payloads'
import { useController, useSessionView } from '../session/hooks'
import { radius, readingWidth, space, type, useTheme } from '../theme'

const ENDED: Record<ViewEnd['reason'], string> = {
  detached: 'This session is closed.',
  connection_lost: 'The connection was lost. Start a new session to continue.',
  sequence_gap: 'This session fell out of step. Start a new session to continue.',
  protocol_error: 'This session fell out of step. Start a new session to continue.',
}

const WAITING_ANNOUNCEMENT = new Map([
  ['confirm', 'A file write is waiting for your answer.'],
  ['run', 'A command is waiting for your answer.'],
  ['fetch', 'A page fetch is waiting for your answer.'],
  ['ask', 'A question is waiting for your answer.'],
])

/** The bottom safe area sits under the keyboard while it is up, so the card needs no margin for it. */
function useKeyboardOpen(): boolean {
  const [open, setOpen] = useState(false)
  useEffect(() => {
    const shown = Keyboard.addListener('keyboardDidShow', () => setOpen(true))
    const hidden = Keyboard.addListener('keyboardDidHide', () => setOpen(false))
    return () => {
      shown.remove()
      hidden.remove()
    }
  }, [])
  return open
}

function EmptyState({
  workspaceName,
  suggestions,
  canSuggest,
  sending,
  onSuggest,
}: {
  workspaceName: string
  suggestions: readonly string[]
  canSuggest: boolean
  sending: boolean
  onSuggest: (text: string) => void
}) {
  const theme = useTheme()
  return (
    <View style={styles.empty} testID="empty-session">
      <Text accessibilityRole="header" style={[type.hero, styles.heading, { color: theme.ink.primary }]}>
        Ready in <Text style={styles.underline}>{workspaceName}</Text>
      </Text>
      <Body style={[styles.lede, { color: theme.ink.secondary }]}>
        Ask for a change, a review or an explanation. Nothing is written without your approval.
      </Body>
      {canSuggest && (
        <View style={styles.suggestions}>
          <Caption style={styles.suggestionsHeading}>Try one of these</Caption>
          {suggestions.map((text, at) => (
            <Pressable
              key={text}
              testID={`suggestion-${at}`}
              accessibilityRole="button"
              accessibilityLabel={text}
              disabled={sending}
              onPress={() => onSuggest(text)}
              style={({ pressed }) => [
                styles.suggestion,
                { borderColor: theme.border.subtle, backgroundColor: pressed ? theme.surface.hover : theme.surface.sunken },
              ]}
            >
              <Body>{text}</Body>
            </Pressable>
          ))}
        </View>
      )}
    </View>
  )
}

/** The text the person first sent, from the rows the screen already holds, for a title before the list says it. */
function firstPromptText(rows: readonly Row[]): string | null {
  const first = rows.find((row) => row.kind === 'prompt')
  return first === undefined ? null : readPromptText(first.data)
}

export function ChatScreen({
  controller,
  target,
  title,
  suggestions,
  workspaceName,
  onMenu,
}: {
  controller: SessionController
  target: string
  title: string | null
  suggestions: readonly string[]
  workspaceName: string
  onMenu: () => void
}) {
  const theme = useTheme()
  const keyboardOpen = useKeyboardOpen()
  const state = useController(controller)
  const view = useSessionView(controller.session)
  // The status is Rust's word on whether the question is still open; the question itself can outlive it for a moment.
  const trust = view.status === 'awaiting_trust' ? controller.session.startupTrust : null
  const [callsOpen, setCallsOpen] = useState(false)
  // One object per controller, so typing in the composer does not make the question card read its payload again.
  const handlers = useMemo(
    () => ({
      onDecide: (request: number, decision: 'approve' | 'reject') => void controller.decide(request, decision),
      onAnswer: (request: number, answers: AskAnswer[]) => void controller.answer(request, answers),
      onTrust: (trusted: boolean) => void controller.trust(trusted),
    }),
    [controller],
  )
  const running = turnIsRunning(view.status, view.ended !== null)
  const calls = useMemo(() => callsFrom(view.rows, running), [view.rows, running])

  useEffect(() => {
    if (view.pending !== null) AccessibilityInfo.announceForAccessibility(WAITING_ANNOUNCEMENT.get(view.pending.kind) ?? 'A question is waiting for your answer.')
  }, [view.pending?.request, view.pending?.kind])

  useEffect(() => {
    if (trust !== null) AccessibilityInfo.announceForAccessibility('This project needs your trust answer before it can start.')
  }, [trust !== null])

  const ended = view.ended === null ? null : ENDED[view.ended.reason]
  const showEmpty = view.rows.length === 0
  const canSuggest = showEmpty && view.status === 'idle' && view.ended === null
  const sending = state.sending
  const empty = useMemo(
    () =>
      showEmpty ? (
        <EmptyState workspaceName={workspaceName} suggestions={suggestions} canSuggest={canSuggest} sending={sending} onSuggest={(text) => void controller.send(text)} />
      ) : null,
    [showEmpty, workspaceName, suggestions, canSuggest, sending, controller],
  )

  const hasQuestion = trust !== null || view.pending !== null
  return (
    <View style={[styles.screen, { backgroundColor: theme.surface.app }]} testID="chat-screen">
      {/* While the calls panel is open, what is behind it is hidden from a screen reader, which Android does not do for a modal view. */}
      <View style={styles.body} testID="chat-content" importantForAccessibility={callsOpen ? 'no-hide-descendants' : 'auto'} accessibilityElementsHidden={callsOpen}>
        <TopBar title={title ?? firstPromptText(view.rows)} target={target} onMenu={onMenu} onCalls={() => setCallsOpen(true)} />
        <KeyboardAvoidingView style={styles.body} behavior={Platform.OS === 'ios' ? 'padding' : 'height'}>
          <ConversationCard keyboardOpen={keyboardOpen}>
            <Transcript rows={view.rows} status={view.status} ended={view.ended !== null} decisions={state.decisions} empty={empty} />
            {hasQuestion && view.ended === null && (
              <View style={styles.panelWrap}>
                <View style={styles.panel}>
                  <ApprovalPanel
                    pending={view.pending}
                    startupTrust={trust}
                    target={target}
                    busy={state.replying}
                    handlers={handlers}
                  />
                </View>
              </View>
            )}
            <Composer
              draft={state.draft}
              onDraft={(text) => controller.setDraft(text)}
              status={view.status}
              ended={ended}
              sending={state.sending}
              workspace={workspaceName}
              onSend={() => void controller.send()}
              onCancel={() => void controller.cancel()}
              error={state.error}
            />
          </ConversationCard>
        </KeyboardAvoidingView>
      </View>
      <CallsDrawer open={callsOpen} onClose={() => setCallsOpen(false)} calls={calls} />
    </View>
  )
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  body: { flex: 1 },
  panelWrap: { paddingHorizontal: space.l, paddingBottom: space.s },
  panel: { width: '100%', maxWidth: readingWidth, alignSelf: 'center' },
  empty: { paddingTop: space.xxl * 2, paddingBottom: space.xxl, gap: space.l, alignItems: 'center' },
  heading: { textAlign: 'center' },
  underline: { textDecorationLine: 'underline' },
  lede: { textAlign: 'center', paddingHorizontal: space.m },
  suggestions: { alignSelf: 'stretch', gap: space.m, paddingTop: space.xl },
  suggestionsHeading: { paddingHorizontal: space.s },
  suggestion: {
    minHeight: 44,
    borderWidth: 1,
    borderRadius: radius.card,
    paddingHorizontal: space.xl,
    paddingVertical: space.l,
    justifyContent: 'center',
  },
})
