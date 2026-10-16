import { useState } from 'react'
import { Pressable, StyleSheet, TextInput, View } from 'react-native'
import type { Status } from '@brave/agent-client'
import { radius, readingWidth, space, type, useTheme } from '../theme'
import { ArrowUpIcon, FolderIcon, StopIcon } from './icons'
import { Meta } from './primitives'

const SEND = 36

export interface ComposerState {
  /** Whether the person can type. */
  editable: boolean
  /** Why sending is not possible, in a sentence, or null when it is. */
  reason: string | null
}

/** What the composer offers for a session's state. Rust reports the status; this only maps it to controls. */
export function composerState(status: Status, ended: string | null): ComposerState {
  if (ended !== null) return { editable: false, reason: ended }
  switch (status) {
    case 'awaiting_trust':
      return { editable: false, reason: 'Answer the trust question to start.' }
    case 'detached':
      return { editable: false, reason: 'This session is closed.' }
    default:
      return { editable: true, reason: null }
  }
}

/** The message box and, under it, the folder the session works in, as the desktop draws them: one object in two tiers. */
export function Composer({
  draft,
  onDraft,
  status,
  ended,
  sending,
  workspace,
  onSend,
  onCancel,
  error,
}: {
  draft: string
  onDraft: (text: string) => void
  status: Status
  ended: string | null
  sending: boolean
  workspace: string
  onSend: () => void
  onCancel: () => void
  error: string | null
}) {
  const theme = useTheme()
  const [focused, setFocused] = useState(false)
  const state = composerState(status, ended)
  const turnActive = status === 'running' || status === 'waiting'
  const canSend = state.editable && !turnActive && draft.trim() !== '' && !sending
  const note = error ?? state.reason
  const button = turnActive
    ? { label: 'Cancel', testID: 'cancel-turn', fill: theme.ink.primary, glyph: theme.surface.panel, onPress: onCancel, disabled: false }
    : {
        label: 'Send',
        testID: 'send',
        fill: canSend ? theme.ink.accent : theme.border.strong,
        glyph: canSend ? theme.ink.onAccent : theme.surface.raised,
        onPress: onSend,
        disabled: !canSend,
      }
  return (
    <View style={styles.dock}>
      <View style={styles.inner}>
        {note !== null && (
          <Meta testID="composer-note" accessibilityLiveRegion="polite" style={[styles.note, error !== null ? { color: theme.status.error } : undefined]}>
            {note}
          </Meta>
        )}
        <View style={[styles.shell, { backgroundColor: theme.surface.app, borderColor: theme.border.subtle }]}>
          <View style={[styles.box, { backgroundColor: theme.surface.raised, borderColor: focused ? theme.border.focus : theme.border.subtle }]}>
            <TextInput
              testID="composer-input"
              accessibilityLabel="Message"
              editable={state.editable}
              multiline
              value={draft}
              onChangeText={onDraft}
              onFocus={() => setFocused(true)}
              onBlur={() => setFocused(false)}
              placeholder={state.editable ? "Let's do something great" : ''}
              placeholderTextColor={theme.ink.tertiary}
              style={[type.body, styles.input, { color: state.editable ? theme.ink.primary : theme.ink.disabled }]}
            />
            <View style={styles.tools}>
              <Pressable
                testID={button.testID}
                accessibilityRole="button"
                accessibilityLabel={button.label}
                accessibilityHint={turnActive ? 'Stops the turn and refuses any question it is waiting on' : undefined}
                accessibilityState={{ disabled: button.disabled }}
                disabled={button.disabled}
                onPress={button.onPress}
                hitSlop={4}
                style={[styles.send, { backgroundColor: button.fill }]}
              >
                {turnActive ? <StopIcon color={button.glyph} size={18} /> : <ArrowUpIcon color={button.glyph} size={18} />}
              </Pressable>
            </View>
          </View>
          <View style={styles.footer} testID="composer-workspace" accessible accessibilityLabel={`Project ${workspace}`}>
            <FolderIcon color={theme.ink.secondary} size={14} />
            <Meta numberOfLines={1} style={styles.workspace}>
              {workspace}
            </Meta>
          </View>
        </View>
      </View>
    </View>
  )
}

const styles = StyleSheet.create({
  dock: { paddingHorizontal: space.l, paddingTop: space.m, paddingBottom: space.l },
  inner: { width: '100%', maxWidth: readingWidth, alignSelf: 'center', gap: space.s },
  note: { paddingHorizontal: space.s },
  shell: { borderWidth: 1, borderRadius: radius.field },
  box: { borderWidth: 1, borderRadius: radius.field, marginTop: -1, marginHorizontal: -1 },
  input: { minHeight: 52, maxHeight: 160, paddingHorizontal: space.xl, paddingTop: space.l, paddingBottom: space.s },
  tools: { flexDirection: 'row', justifyContent: 'flex-end', alignItems: 'center', paddingHorizontal: space.m, paddingBottom: space.m },
  send: { width: SEND, height: SEND, borderRadius: SEND / 2, alignItems: 'center', justifyContent: 'center' },
  footer: { flexDirection: 'row', alignItems: 'center', gap: space.m, minHeight: 36, paddingHorizontal: space.l },
  workspace: { flexShrink: 1 },
})
