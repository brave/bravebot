import { useState } from 'react'
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native'
import { useSafeAreaInsets } from 'react-native-safe-area-context'
import { CALL_STATE_WORD, callLabel, type Call } from '../calls'
import { displayString } from '../text/displayText'
import { cardGap, hit, radius, space, titlebar, type, useTheme } from '../theme'
import { ChevronIcon, TerminalIcon } from './icons'
import { SidePanel } from './SidePanel'
import { Caption, Chip, Count, Divider, Meta, UI, UntrustedText } from './primitives'

function CallRow({ call }: { call: Call }) {
  const theme = useTheme()
  const label = callLabel(call)
  const ink = call.state === 'failed' ? theme.status.error : theme.ink.secondary
  return (
    <View
      testID={`calls-row-${call.id}`}
      accessible
      accessibilityLabel={`${displayString(label)}, ${CALL_STATE_WORD[call.state]}`}
      style={styles.row}
    >
      <TerminalIcon color={theme.ink.tertiary} size={14} />
      <View style={styles.rowLabel}>
        <UntrustedText text={label} mono numberOfLines={1} selectable={false} style={{ color: ink }} />
      </View>
      {call.state === 'running' && <Chip label="Running" tone="info" />}
      {call.state === 'unfinished' && <Chip label="Did not finish" />}
      {call.state === 'failed' && <Chip label="Failed" tone="error" />}
      {call.untrusted && <Chip label="From outside" tone="warning" />}
    </View>
  )
}

/** The right-hand inspector: every call the session has made, as the desktop's Calls made list shows them. */
export function CallsDrawer({ open, onClose, calls }: { open: boolean; onClose: () => void; calls: readonly Call[] }) {
  const theme = useTheme()
  const insets = useSafeAreaInsets()
  const [folded, setFolded] = useState(false)

  return (
    <SidePanel
      open={open}
      onClose={onClose}
      side="right"
      inset={cardGap}
      closeLabel="Close calls"
      testID="calls-drawer"
      scrimTestID="calls-scrim"
      style={[styles.panel, { top: insets.top + cardGap, bottom: insets.bottom + cardGap, backgroundColor: theme.surface.panel, borderColor: theme.border.subtle }]}
    >
      <View style={styles.head}>
        <View style={[styles.tab, { borderBottomColor: theme.ink.accent }]}>
          <Text style={[type.uiStrong, { color: theme.ink.accent }]}>Overview</Text>
        </View>
      </View>
      <Divider />
      <View style={styles.sectionHead}>
        <UI style={styles.sectionTitle}>Calls made</UI>
        <Count value={calls.length} testID="calls-count" />
        <Pressable
          testID="calls-toggle"
          accessibilityRole="button"
          accessibilityLabel={folded ? 'Show calls made' : 'Hide calls made'}
          accessibilityState={{ expanded: !folded }}
          onPress={() => setFolded((current) => !current)}
          hitSlop={4}
          style={({ pressed }) => [styles.toggle, { backgroundColor: pressed ? theme.surface.hover : 'transparent' }]}
        >
          <ChevronIcon color={theme.ink.secondary} size={16} direction={folded ? 'down' : 'up'} />
        </Pressable>
      </View>
      {!folded && (
        <ScrollView style={styles.body} contentContainerStyle={styles.bodyContent}>
          <Caption style={styles.about}>Every call this session has made, in order.</Caption>
          {calls.length === 0 ? (
            <View testID="calls-empty" style={styles.empty}>
              <TerminalIcon color={theme.ink.tertiary} size={16} />
              <Meta style={{ color: theme.ink.tertiary }}>No calls yet.</Meta>
            </View>
          ) : (
            calls.map((call) => <CallRow key={call.id} call={call} />)
          )}
        </ScrollView>
      )}
    </SidePanel>
  )
}

const styles = StyleSheet.create({
  panel: { borderWidth: 1, borderRadius: radius.card, overflow: 'hidden' },
  head: { height: titlebar, paddingHorizontal: space.xl, justifyContent: 'flex-end' },
  tab: { alignSelf: 'flex-start', height: titlebar - 2, justifyContent: 'center', borderBottomWidth: 2 },
  sectionHead: { flexDirection: 'row', alignItems: 'center', gap: space.m, paddingLeft: space.xl, paddingRight: space.s, minHeight: hit },
  sectionTitle: { flex: 1, fontWeight: '600' },
  toggle: { width: hit, height: hit, borderRadius: radius.control, alignItems: 'center', justifyContent: 'center' },
  body: { flex: 1 },
  bodyContent: { paddingHorizontal: space.l, paddingBottom: space.l },
  about: { paddingHorizontal: space.s, paddingBottom: space.s },
  row: { flexDirection: 'row', alignItems: 'center', gap: space.m, minHeight: 32, paddingHorizontal: space.m },
  rowLabel: { flex: 1, minWidth: 0 },
  empty: { flexDirection: 'row', alignItems: 'center', gap: space.m, paddingHorizontal: space.s, paddingTop: space.s },
})
