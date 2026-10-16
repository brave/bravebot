import { memo } from 'react'
import { StyleSheet, View } from 'react-native'
import type { Row } from '@brave/agent-client'
import {
  readActivity,
  readAsk,
  readConfirm,
  readFailure,
  readFetch,
  readNarration,
  readPromptText,
  readReply,
  readRun,
  readShown,
} from '../../payloads'
import { displayString } from '../../text/displayText'
import { radius, space, type, useTheme } from '../../theme'
import { CALL_STATE_WORD, callState } from '../../calls'
import type { Decision } from '../../session/controller'
import { CheckIcon, CrossIcon } from '../icons'
import { Caption, Chip, Meta, UI, UntrustedText } from '../primitives'

const WAITING_TITLE = new Map([
  ['confirm.request', 'Write a file'],
  ['run.request', 'Run a command'],
  ['fetch.request', 'Fetch a page'],
  ['ask.request', 'A question'],
])

function approvalDetail(row: Row): string | null {
  switch (row.event) {
    case 'confirm.request':
      return readConfirm(row.data)?.path ?? null
    case 'run.request': {
      const run = readRun(row.data)
      return run?.line || run?.stages[0]?.display || null
    }
    case 'fetch.request':
      return readFetch(row.data)?.host ?? null
    case 'ask.request':
      return readAsk(row.data)?.prompts[0]?.header ?? null
    default:
      return null
  }
}

function ApprovalSummary({ row, decision }: { row: Row; decision: Decision | undefined }) {
  const theme = useTheme()
  const title = (row.event !== null && WAITING_TITLE.get(row.event)) || 'A question'
  const detail = approvalDetail(row)
  const state = row.resolved ? (decision === 'approved' ? 'Approved' : decision === 'rejected' ? 'Rejected' : row.event === 'ask.request' ? 'Answered' : 'Resolved') : 'Waiting for your answer'
  return (
    <View
      testID={`row-approval-${row.id}`}
      accessible
      accessibilityLabel={`${title}. ${state}.`}
      style={[styles.summary, { borderColor: theme.border.subtle, backgroundColor: theme.surface.raised }]}
    >
      <View style={styles.summaryHead}>
        <UI style={styles.strong}>{title}</UI>
        <Chip label={state} tone={!row.resolved ? 'warning' : state === 'Approved' || state === 'Answered' ? 'success' : 'neutral'} />
      </View>
      {detail !== null && <UntrustedText text={detail} mono numberOfLines={2} selectable={false} />}
    </View>
  )
}

function Quarantined({ row }: { row: Row }) {
  const theme = useTheme()
  const shown = readShown(row.data)
  if (shown === null) return <Meta testID={`row-${row.id}`}>Content from outside could not be shown.</Meta>
  const reach = shown.reach === 'no_model' ? 'No model has read this.' : 'The planner has not read this.'
  return (
    <View
      testID={`row-quarantined-${row.id}`}
      style={[styles.quarantine, { borderColor: theme.status.warning, backgroundColor: theme.surface.raised }]}
    >
      <View style={[styles.quarantineHead, { backgroundColor: theme.status.warningBg }]}>
        <Meta style={[styles.strong, { color: theme.status.warning }]}>Untrusted content from outside</Meta>
        <View style={styles.quarantineMeta}>
          <Meta style={{ color: theme.status.warning }}>Label</Meta>
          <UntrustedText text={shown.label} mono selectable={false} testID={`row-${row.id}-label`} />
        </View>
        <Meta style={{ color: theme.status.warning }}>{reach}</Meta>
      </View>
      <View style={styles.quarantineBody}>
        <Caption>Origin</Caption>
        <UntrustedText text={shown.origin} mono />
        <Caption>{`Preview, ${shown.preview.length} of ${shown.lines} lines`}</Caption>
        <View style={[styles.preview, { backgroundColor: theme.surface.sunken, borderColor: theme.border.hairline }]}>
          {shown.preview.map((line, at) => (
            <UntrustedText key={at} text={line} mono multiline testID={`row-${row.id}-line-${at}`} />
          ))}
        </View>
      </View>
    </View>
  )
}

function Activity({ row, live }: { row: Row; live: boolean }) {
  const theme = useTheme()
  const activity = readActivity(row.data)
  if (activity === null) return <Caption>An action that could not be shown.</Caption>
  const state = callState(row, activity, live)
  return (
    <View testID={`row-activity-${row.id}`} style={styles.activity} accessible accessibilityLabel={`${displayString(activity.verb)} ${displayString(activity.target)}, ${CALL_STATE_WORD[state]}`}>
      <View style={styles.activityLine}>
        <View testID={`activity-${state}-${row.id}`} style={styles.marker}>
          {state === 'running' || state === 'unfinished' ? (
            <View style={[styles.dot, { backgroundColor: state === 'running' ? theme.ink.accent : theme.ink.tertiary }]} />
          ) : state === 'failed' ? (
            <CrossIcon color={theme.status.error} size={14} />
          ) : (
            <CheckIcon color={theme.ink.tertiary} size={14} />
          )}
        </View>
        <UntrustedText text={state === 'running' ? `${activity.verb} started` : state === 'unfinished' ? `${activity.verb} did not finish` : state === 'failed' ? `${activity.verb} failed` : activity.verb} size="meta" selectable={false} style={{ color: theme.ink.secondary }} />
        <View style={styles.target}>
          <UntrustedText text={activity.target} mono numberOfLines={1} selectable={false} style={{ color: theme.ink.tertiary }} />
        </View>
        {activity.note !== null && <UntrustedText text={activity.note} size="meta" selectable={false} style={[type.caption, { color: theme.ink.tertiary }]} />}
      </View>
      {activity.why !== '' && (
        <View style={styles.why}>
          <UntrustedText text={activity.why} selectable={false} style={[type.meta, { color: theme.ink.tertiary }]} />
        </View>
      )}
      {activity.untrusted && <Chip label="From outside" tone="warning" style={styles.why} />}
    </View>
  )
}

function Failure({ row }: { row: Row }) {
  const theme = useTheme()
  const failure = readFailure(row.data)
  const cancelled = failure?.kind === 'cancelled'
  return (
    <View
      testID={`row-error-${row.id}`}
      style={[styles.failure, { borderColor: cancelled ? theme.border.subtle : theme.status.error, backgroundColor: cancelled ? theme.surface.sunken : theme.status.errorBg }]}
    >
      <UI style={[styles.strong, { color: cancelled ? theme.ink.secondary : theme.status.error }]}>{cancelled ? 'Cancelled' : 'The turn failed'}</UI>
      {failure !== null && <UntrustedText text={failure.message} />}
    </View>
  )
}

function RowViewBase({ row, decision, live }: { row: Row; decision: Decision | undefined; live: boolean }) {
  const theme = useTheme()
  switch (row.kind) {
    case 'prompt': {
      const text = readPromptText(row.data)
      return (
        <View style={styles.promptWrap}>
          <View testID={`row-prompt-${row.id}`} style={[styles.prompt, { backgroundColor: theme.surface.sunken }]}>
            {text === null ? <Meta>A message that could not be shown.</Meta> : <UntrustedText text={text} multiline size="body" />}
          </View>
        </View>
      )
    }
    case 'narration': {
      const text = readNarration(row.data)
      return text === null ? null : <UntrustedText testID={`row-narration-${row.id}`} text={text} multiline size="body" style={{ color: theme.ink.secondary }} />
    }
    case 'reply': {
      const text = readReply(row.data)
      return <UntrustedText testID={`row-reply-${row.id}`} text={text ?? 'The reply could not be shown.'} multiline size="body" />
    }
    case 'error':
      return <Failure row={row} />
    case 'quarantined':
      return <Quarantined row={row} />
    case 'approval':
      return <ApprovalSummary row={row} decision={decision} />
    case 'activity':
      return <Activity row={row} live={live} />
  }
}

export const RowView = memo(RowViewBase)

const styles = StyleSheet.create({
  strong: { fontWeight: '600' },
  promptWrap: { alignItems: 'flex-end' },
  prompt: { maxWidth: '80%', borderRadius: radius.bubble, paddingHorizontal: space.xxl, paddingVertical: space.l },
  summary: { borderWidth: 1, borderRadius: radius.card, padding: space.l, gap: space.s },
  summaryHead: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: space.m },
  quarantine: { borderWidth: 1, borderStyle: 'dashed', borderRadius: radius.card, overflow: 'hidden' },
  quarantineHead: { padding: space.l, gap: 2 },
  quarantineMeta: { flexDirection: 'row', gap: space.m, alignItems: 'center' },
  quarantineBody: { padding: space.l, gap: space.s },
  preview: { borderWidth: StyleSheet.hairlineWidth, borderRadius: radius.control, padding: space.m, gap: 2 },
  activity: { gap: 2, minHeight: 28, justifyContent: 'center' },
  activityLine: { flexDirection: 'row', alignItems: 'center', gap: space.m, minHeight: 28 },
  marker: { width: 14, height: 14, alignItems: 'center', justifyContent: 'center' },
  dot: { width: 6, height: 6, borderRadius: 3 },
  target: { flexShrink: 1 },
  why: { marginLeft: 14 + space.m },
  failure: { borderWidth: 1, borderRadius: radius.card, padding: space.l, gap: 2 },
})
