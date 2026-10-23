import { useState } from 'react'
import { Pressable, StyleSheet, Text, TextInput, View } from 'react-native'
import type { AskAnswer } from '@brave/agent-client'
import { displayString } from '../../text/displayText'
import { radius, space, type, useTheme } from '../../theme'
import type { AskView, Change, ConfirmView, FetchView, RunView, TrustView } from '../../payloads'
import { Button, Caption, Meta, UI, UntrustedText } from '../primitives'
import { CheckIcon } from '../icons'
import { Actions, CardFrame, Evidence, Field, ReadOnlyField, UntrustedBand } from './parts'

interface Common {
  target: string
  busy: boolean
}

const INTENT = { create: 'Create a file', overwrite: 'Replace a file', edit: 'Edit a file' } as const

function ChangeLine({ change }: { change: Change }) {
  const theme = useTheme()
  if (change.kind === 'elided') return <Caption style={styles.elided}>{`${change.lines} unchanged lines not shown`}</Caption>
  const sign = change.kind === 'added' ? '+' : change.kind === 'removed' ? '-' : ' '
  const ink = change.kind === 'added' ? theme.status.success : change.kind === 'removed' ? theme.status.error : theme.ink.secondary
  const wash = change.kind === 'added' ? theme.status.successBg : change.kind === 'removed' ? theme.status.errorBg : 'transparent'
  const label = change.kind === 'added' ? 'Added' : change.kind === 'removed' ? 'Removed' : 'Kept'
  return (
    <View style={[styles.change, { backgroundColor: wash }]} accessible accessibilityLabel={`${label}: ${displayString(change.text)}`}>
      <Meta style={[type.code, styles.sign, { color: ink }]}>{sign}</Meta>
      <View style={styles.changeText}>
        <UntrustedText text={change.text} mono style={{ color: ink }} />
      </View>
    </View>
  )
}

export function ConfirmCard({ view, target, busy, onDecide }: Common & { view: ConfirmView; onDecide: (decision: 'approve' | 'reject') => void }) {
  const theme = useTheme()
  return (
    <CardFrame
      testID="confirm-card"
      title={INTENT[view.intent]}
      target={target}
      marking={
        view.untrusted ? (
          <UntrustedBand
            testID="untrusted-write"
            heading="Untrusted content"
            detail="This text came from outside and nobody has vouched for it. Reviewing it is not the same as reviewing the model's own work."
          />
        ) : undefined
      }
      actions={<Actions onReject={() => onDecide('reject')} onApprove={() => onDecide('approve')} approveLabel="Approve write" busy={busy} />}
    >
      <ReadOnlyField label="Path" value={view.shownPath} mono testID="confirm-path" />
      <Field label="Effect">
        <UI>{view.existing ? 'Changes a file that exists' : 'Creates a new file'}</UI>
        <Meta>
          <Text style={{ color: theme.status.success }}>{`${view.added} added`}</Text>
          {', '}
          <Text style={{ color: theme.status.error }}>{`${view.removed} removed`}</Text>
          {view.exact ? '' : ' (approximate)'}
        </Meta>
      </Field>
      <Evidence flush>
        {view.changes.map((change, at) => (
          <ChangeLine key={at} change={change} />
        ))}
      </Evidence>
      {view.lineEndings !== null && <ReadOnlyField label="Line endings" value={view.lineEndings} />}
      {view.writtenSinceCheckout && <UI>This path was written after the checkout this change was made from.</UI>}
      {view.credentials.length > 0 && (
        <Field label="Credentials found in the text">
          {view.credentials.map((entry, at) => (
            <UntrustedText key={at} text={entry} mono />
          ))}
        </Field>
      )}
      {view.remark !== null && (
        <Field label={`Note, ${view.remark.lines} lines`}>
          <UntrustedText text={view.remark.label} size="meta" style={{ color: theme.ink.secondary }} />
          <Evidence>
            {view.remark.preview.map((line, at) => (
              <UntrustedText key={at} text={line} mono />
            ))}
          </Evidence>
        </Field>
      )}
    </CardFrame>
  )
}

export function RunCard({ view, target, busy, onDecide }: Common & { view: RunView; onDecide: (decision: 'approve' | 'reject') => void }) {
  const theme = useTheme()
  return (
    <CardFrame
      testID="run-card"
      title="Run a command"
      target={target}
      actions={<Actions onReject={() => onDecide('reject')} onApprove={() => onDecide('approve')} approveLabel="Approve once" busy={busy} />}
    >
      {view.summary !== '' && <UntrustedText text={view.summary} />}
      {view.line !== '' && <ReadOnlyField label="As written" value={view.line} mono testID="run-line" />}
      {view.plan !== '' && view.plan !== view.line && <ReadOnlyField label="Plan" value={view.plan} mono testID="run-plan" />}
      <Field label={view.stages.length === 1 ? 'Program' : 'Programs'}>
        <Evidence>
          {view.stages.map((stage, at) => (
            <View key={at} style={styles.stage}>
              <UntrustedText text={stage.display} mono />
              <ReadOnlyField label="Name" value={stage.program} mono testID={`run-stage-${at}-name`} />
              {stage.resolved === null ? (
                <Field label="Resolves to">
                  <Meta>Did not resolve to a file</Meta>
                </Field>
              ) : (
                // The file the step runs, and the path that starts it where that is another, spelled
                // by the bridge: `resolved`, `startedAs` and `binary` are drawn as held and are not
                // drawn here, since a path ending in a space reads as the one without it.
                <ReadOnlyField label="Runs" value={stage.shownBinary} mono testID={`run-stage-${at}-binary`} />
              )}
            </View>
          ))}
        </Evidence>
      </Field>
      <ReadOnlyField label="Directory" value={view.shownDirectory} mono />
      {view.shownWrites.length > 0 && (
        <Field label="Writes to">
          {view.shownWrites.map((path, at) => (
            <UntrustedText key={at} text={path} mono />
          ))}
        </Field>
      )}
      {view.stdin !== null && <ReadOnlyField label="Input" value={view.stdin} mono />}
      {view.releasesPrivate && <UI>This hands your private data to a program.</UI>}
      {view.requestedScopes.length > 0 && (
        <Field label="Asks to be lent">
          {view.requestedScopes.map((scope, at) => (
            <UntrustedText key={at} text={scope} />
          ))}
        </Field>
      )}
      {view.ambient.length > 0 && (
        <Field label="Reaches access nothing here holds">
          {view.ambient.map((entry, at) => (
            <UntrustedText key={at} text={`${entry.authority}: ${entry.named}`} />
          ))}
        </Field>
      )}
      {view.confinement !== null && (
        <Field label={<UntrustedText text={view.confinement.heading} size="meta" style={{ color: theme.ink.tertiary }} />}>
          {view.confinement.shownDirectories.map((path, at) => (
            <UntrustedText key={`d${at}`} text={path} mono />
          ))}
          {view.confinement.sentences.map((sentence, at) => (
            <UntrustedText key={`s${at}`} text={sentence} />
          ))}
        </Field>
      )}
      {view.confinement === null && view.notSandboxed !== null && (
        <Field label="Not confined">
          <UntrustedText text={view.notSandboxed} />
        </Field>
      )}
      <Caption>Approving covers this run only. It is never remembered from this device.</Caption>
    </CardFrame>
  )
}

export function FetchCard({ view, target, busy, onDecide }: Common & { view: FetchView; onDecide: (decision: 'approve' | 'reject') => void }) {
  return (
    <CardFrame
      testID="fetch-card"
      title="Fetch a page"
      target={target}
      actions={<Actions onReject={() => onDecide('reject')} onApprove={() => onDecide('approve')} approveLabel="Approve fetch" busy={busy} />}
    >
      {view.summary !== '' && <UntrustedText text={view.summary} />}
      <ReadOnlyField label="Connects to" value={view.host} mono testID="fetch-host" />
      <ReadOnlyField label="Address as sent" value={view.url} mono testID="fetch-url" />
      {view.ambient.length > 0 && (
        <Field label="Reaches access nothing here holds">
          {view.ambient.map((entry, at) => (
            <UntrustedText key={at} text={`${entry.authority}: ${entry.named}`} />
          ))}
        </Field>
      )}
      <Caption>What comes back is kept apart from the model whatever you answer. Approving covers this address only.</Caption>
    </CardFrame>
  )
}

export function AskCard({ view, target, busy, onAnswer }: Common & { view: AskView; onAnswer: (answers: AskAnswer[]) => void }) {
  const theme = useTheme()
  const [chosen, setChosen] = useState<number[][]>(() => view.prompts.map(() => []))
  const [typed, setTyped] = useState<string[]>(() => view.prompts.map(() => ''))
  const toggle = (prompt: number, index: number, multiple: boolean): void => {
    setTyped((current) => current.map((text, at) => (at === prompt ? '' : text)))
    setChosen((current) =>
      current.map((picked, at) => {
        if (at !== prompt) return picked
        if (picked.includes(index)) return picked.filter((value) => value !== index)
        return multiple ? [...picked, index] : [index]
      }),
    )
  }
  const typeAnswer = (prompt: number, value: string): void => {
    setTyped((current) => current.map((text, at) => (at === prompt ? value : text)))
    if (value.trim() !== '') setChosen((current) => current.map((picked, at) => (at === prompt ? [] : picked)))
  }
  const answers = (): AskAnswer[] =>
    view.prompts.map((_, at) => {
      const text = typed[at] ?? ''
      if (text.trim() !== '') return { typed: text }
      const picked = chosen[at] ?? []
      return picked.length > 0 ? { chosen: picked } : null
    })
  const answered = answers().some((answer) => answer !== null)
  return (
    <CardFrame
      testID="ask-card"
      title={view.prompts.length === 1 ? 'A question' : `${view.prompts.length} questions`}
      target={target}
      actions={
        <>
          <Button label="Decline" onPress={() => onAnswer(view.prompts.map(() => null))} disabled={busy} testID="decline" />
          <Button label="Send answers" onPress={() => onAnswer(answers())} variant="primary" disabled={busy || !answered} testID="send-answers" />
        </>
      }
    >
      {!answered && view.prompts.length > 0 && <Meta testID="ask-hint">Choose or type an answer to send it, or decline.</Meta>}
      {view.prompts.map((prompt, at) => (
        <View key={at} style={styles.prompt}>
          <UntrustedText text={prompt.header} size="meta" style={{ color: theme.ink.tertiary }} />
          <UntrustedText text={prompt.question} style={styles.strong} />
          {prompt.rows.map((row) => {
            const selected = (chosen[at] ?? []).includes(row.index)
            return (
              <Pressable
                key={row.index}
                testID={`ask-row-${at}-${row.index}`}
                accessibilityRole={prompt.multiple ? 'checkbox' : 'radio'}
                accessibilityState={{ checked: selected }}
                accessibilityLabel={displayString(row.label)}
                onPress={() => toggle(at, row.index, prompt.multiple)}
                style={[
                  styles.row,
                  { borderColor: selected ? theme.border.focus : theme.border.subtle, backgroundColor: selected ? theme.surface.selected : theme.surface.raised },
                ]}
              >
                <View style={styles.rowText}>
                  <UntrustedText text={row.label} selectable={false} style={styles.strong} />
                  {row.detail !== null && <UntrustedText text={row.detail} selectable={false} style={{ color: theme.ink.secondary }} />}
                </View>
                {selected && <CheckIcon color={theme.ink.accent} size={18} />}
              </Pressable>
            )
          })}
          <TextInput
            testID={`ask-typed-${at}`}
            accessibilityLabel={`Your own answer to: ${displayString(prompt.header)}`}
            placeholder={prompt.rows.length === 0 ? 'Type your answer' : 'Or type your own answer'}
            placeholderTextColor={theme.ink.tertiary}
            value={typed[at] ?? ''}
            onChangeText={(value) => typeAnswer(at, value)}
            multiline
            style={[type.body, styles.input, { color: theme.ink.primary, borderColor: theme.border.strong, backgroundColor: theme.surface.raised }]}
          />
        </View>
      ))}
    </CardFrame>
  )
}

export function TrustCard({ view, target, busy, onAnswer }: Common & { view: TrustView; onAnswer: (trusted: boolean) => void }) {
  return (
    <CardFrame
      testID="trust-card"
      title="Project trust"
      target={target}
      actions={
        <>
          <Button label="Don't trust" onPress={() => onAnswer(false)} disabled={busy} testID="trust-decline" />
          <Button label="Trust this directory" onPress={() => onAnswer(true)} variant="primary" disabled={busy} testID="trust-approve" />
        </>
      }
    >
      <UI style={styles.strong}>Do you trust this directory?</UI>
      <ReadOnlyField label="Directory" value={view.shownDirectory} mono />
      {view.shownKeeping === null ? (
        <UI>This answer is not kept. It applies to this session only.</UI>
      ) : (
        <>
          <ReadOnlyField label="A yes is kept in" value={view.shownKeeping} mono />
          <UI>A yes is written to that file and applies to later sessions in this directory.</UI>
        </>
      )}
      <UI>Trust it and files here are read normally, so ordinary work proceeds without a prompt for every edit.</UI>
      <UI>Decline and nothing here is trusted. The agent still works on these files, but never reads them: they go to an isolated processor, and you see every change before it is applied.</UI>
    </CardFrame>
  )
}

/** A question this device cannot answer completely. Nothing is offered but cancelling the turn, which refuses it. */
export function UnanswerableCard({ kind, reason, target, way = 'Cancel the turn to refuse this question.' }: { kind: string; reason: string; target: string; way?: string }) {
  return (
    <CardFrame testID="unanswerable-card" title="Cannot be answered here" target={target} actions={<Meta>{way}</Meta>}>
      <UI>{reason}</UI>
      <ReadOnlyField label="Kind of question" value={kind} />
    </CardFrame>
  )
}

const styles = StyleSheet.create({
  change: { flexDirection: 'row', gap: space.s, paddingHorizontal: space.m, paddingVertical: 1 },
  elided: { paddingHorizontal: space.m, paddingVertical: space.s },
  sign: { width: 12 },
  changeText: { flex: 1 },
  stage: { gap: 2 },
  prompt: { gap: space.m },
  strong: { fontWeight: '600' },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.m,
    borderWidth: 1,
    borderRadius: radius.control,
    paddingHorizontal: space.l,
    paddingVertical: space.m,
    minHeight: 44,
  },
  rowText: { flex: 1, gap: 2 },
  input: { borderWidth: 1, borderRadius: radius.control, paddingHorizontal: space.l, paddingVertical: space.m, minHeight: 44 },
})
