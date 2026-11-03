import { FlatList, Pressable, StyleSheet, View } from 'react-native'
import { useSafeAreaInsets } from 'react-native-safe-area-context'
import type { SessionSummary, Status } from '@brave/agent-client'
import { displayString, singleLine } from '../text/displayText'
import { radius, space, useTheme } from '../theme'
import { workspaceName } from '../workspaces'
import { SidePanel } from './SidePanel'
import { Button, Caption, Chip, Divider, Meta, OutlinePill, UntrustedText } from './primitives'

const STATUS_LABEL: Record<Status, string> = {
  awaiting_trust: 'Needs setup',
  idle: 'Ready',
  running: 'Working',
  waiting: 'Needs your answer',
  completed: 'Done',
  failed: 'Failed',
  cancelled: 'Cancelled',
  detached: 'Closed',
}

/**
 * Where the session list stands. `checking` until the target says whether it lists sessions, `unsupported` when it does not,
 * and `failed` when the newest request failed, which keeps any list that arrived before it.
 */
export type Listing = 'checking' | 'unsupported' | 'listed' | 'failed'

const EMPTY: Record<Exclude<Listing, 'unsupported'>, string> = {
  checking: 'Listing sessions…',
  listed: 'No sessions yet.',
  failed: 'Sessions could not be listed.',
}

export interface DrawerTarget {
  label: string
  description: string
}

function SessionRow({
  session,
  workspaceName,
  active,
  onOpen,
}: {
  session: SessionSummary
  workspaceName: string
  active: boolean
  onOpen: (id: string) => void
}) {
  const theme = useTheme()
  const title = session.firstPrompt === null ? 'New session' : singleLine(session.firstPrompt)
  const state = session.ended ? 'Closed' : STATUS_LABEL[session.status]
  return (
    <Pressable
      testID={`session-${session.id}`}
      accessibilityRole="button"
      accessibilityLabel={`${displayString(title)}. ${displayString(workspaceName)}. ${state}.`}
      accessibilityState={{ selected: active }}
      onPress={() => onOpen(session.id)}
      style={({ pressed }) => [
        styles.row,
        { backgroundColor: active ? theme.surface.selected : pressed ? theme.surface.hover : 'transparent' },
      ]}
    >
      <View style={styles.rowText}>
        <UntrustedText text={title} numberOfLines={2} selectable={false} style={styles.rowTitle} />
        <UntrustedText text={`${workspaceName} · ${state}`} size="meta" numberOfLines={1} selectable={false} style={{ color: theme.ink.secondary }} />
      </View>
      {session.waiting && <Chip label="Answer" tone="warning" testID={`session-${session.id}-attention`} />}
    </Pressable>
  )
}

export function Drawer({
  open,
  onClose,
  target,
  sessions,
  listing = 'listed',
  workspaces,
  workspace,
  onWorkspace,
  activeId,
  onOpenSession,
  onNewSession,
  problem = null,
}: {
  open: boolean
  onClose: () => void
  target: DrawerTarget
  sessions: readonly SessionSummary[]
  listing?: Listing
  workspaces: readonly { id: string; name: string }[]
  workspace: string
  onWorkspace: (id: string) => void
  activeId: string | null
  onOpenSession: (id: string) => void
  onNewSession: () => void
  problem?: string | null
}) {
  const theme = useTheme()
  const insets = useSafeAreaInsets()

  return (
    <SidePanel
      open={open}
      onClose={onClose}
      side="left"
      closeLabel="Close sessions"
      testID="drawer"
      scrimTestID="drawer-scrim"
      style={[styles.panel, { backgroundColor: theme.surface.app, borderRightColor: theme.border.subtle, paddingTop: insets.top + space.l, paddingBottom: insets.bottom }]}
    >
      <View style={styles.header}>
        <View style={styles.targetLine}>
          <OutlinePill label={target.label} tone="info" testID="drawer-target" />
          <Caption>Target</Caption>
        </View>
        <Meta>{target.description}</Meta>
        <Button label="New session" onPress={onNewSession} variant="primary" testID="new-session" />
        {problem !== null && (
          <Meta testID="drawer-problem" accessibilityLiveRegion="polite" style={{ color: theme.status.error }}>
            {problem}
          </Meta>
        )}
        {workspaces.length > 1 && (
          <View style={styles.workspaces} accessibilityRole="radiogroup">
            <Caption>Project</Caption>
            <View style={styles.workspaceRow}>
              {workspaces.map((candidate) => {
                const selected = candidate.id === workspace
                return (
                  <Pressable
                    key={candidate.id}
                    testID={`workspace-${candidate.id}`}
                    accessibilityRole="radio"
                    accessibilityState={{ checked: selected }}
                    accessibilityLabel={displayString(candidate.name)}
                    onPress={() => onWorkspace(candidate.id)}
                    style={[
                      styles.workspace,
                      { borderColor: selected ? theme.border.focus : theme.border.subtle, backgroundColor: selected ? theme.surface.selected : theme.surface.raised },
                    ]}
                  >
                    <UntrustedText text={candidate.name} selectable={false} style={selected ? styles.selectedLabel : undefined} />
                  </Pressable>
                )
              })}
            </View>
          </View>
        )}
      </View>
      <Divider />
      {listing === 'unsupported' ? (
        <Meta testID="session-list-unsupported" style={styles.empty}>
          This target does not keep a list of sessions.
        </Meta>
      ) : (
        <FlatList
          testID="session-list"
          data={sessions}
          keyExtractor={(session) => session.id}
          renderItem={({ item }) => (
            <SessionRow session={item} workspaceName={workspaceName(workspaces, item.workspace)} active={item.id === activeId} onOpen={onOpenSession} />
          )}
          ListHeaderComponent={
            <View>
              <Caption style={styles.listHeading}>Sessions</Caption>
              {listing === 'failed' && sessions.length > 0 && (
                <Meta testID="session-list-stale" accessibilityLiveRegion="polite" style={[styles.stale, { color: theme.status.error }]}>
                  The list could not be refreshed. It shows the sessions last listed.
                </Meta>
              )}
            </View>
          }
          ListEmptyComponent={
            <Meta testID="session-list-empty" style={[styles.empty, listing === 'failed' ? { color: theme.status.error } : undefined]}>
              {EMPTY[listing]}
            </Meta>
          }
        />
      )}
    </SidePanel>
  )
}

const styles = StyleSheet.create({
  panel: { top: 0, bottom: 0, borderRightWidth: StyleSheet.hairlineWidth },
  header: { paddingHorizontal: space.xl, paddingBottom: space.l, gap: space.m },
  targetLine: { flexDirection: 'row', alignItems: 'center', gap: space.m },
  workspaces: { gap: space.s },
  workspaceRow: { flexDirection: 'row', flexWrap: 'wrap', gap: space.m },
  workspace: { minHeight: 44, borderWidth: 1, borderRadius: radius.control, paddingHorizontal: space.l, justifyContent: 'center' },
  selectedLabel: { fontWeight: '600' },
  listHeading: { paddingHorizontal: space.xl, paddingTop: space.l, paddingBottom: space.s },
  row: { minHeight: 56, flexDirection: 'row', alignItems: 'center', gap: space.m, marginHorizontal: space.m, paddingHorizontal: space.l, paddingVertical: space.m, borderRadius: radius.control },
  rowText: { flex: 1, gap: 2 },
  rowTitle: { fontWeight: '500' },
  empty: { paddingHorizontal: space.xl, paddingTop: space.l },
  stale: { paddingHorizontal: space.xl, paddingBottom: space.s },
})
