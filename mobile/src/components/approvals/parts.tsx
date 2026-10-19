import { createContext, useContext, useState, type ReactNode } from 'react'
import { ScrollView, StyleSheet, View, useWindowDimensions, type NativeScrollEvent, type NativeSyntheticEvent } from 'react-native'
import { radius, space, type, useTheme } from '../../theme'
import { Button, Caption, Divider, Meta, OutlinePill, UI, UntrustedText } from '../primitives'

/** True while a card has more to read than fits, so the action that approves waits until the end has been reached. */
const ReadingContext = createContext(false)

/** The frame every question shares: what is asked, where it will happen, the evidence, and the answers. */
export function CardFrame({
  title,
  target,
  marking,
  children,
  actions,
  testID,
}: {
  title: string
  target: string
  marking?: ReactNode
  children: ReactNode
  actions: ReactNode
  testID: string
}) {
  const theme = useTheme()
  const { height, fontScale } = useWindowDimensions()
  const [viewport, setViewport] = useState(0)
  const [content, setContent] = useState(0)
  const [offset, setOffset] = useState(0)
  const more = viewport > 0 && content > viewport + 4 && offset + viewport < content - 8
  return (
    <View
      testID={testID}
      accessibilityViewIsModal={false}
      style={[styles.frame, { backgroundColor: theme.surface.raised, borderColor: theme.border.subtle }]}
    >
      <View style={styles.head}>
        <UI style={[type.title, styles.title]} testID={`${testID}-title`}>
          {title}
        </UI>
        <OutlinePill label={target} tone="info" testID={`${testID}-target`} />
      </View>
      {marking}
      <ScrollView
        testID={`${testID}-scroll`}
        style={{ maxHeight: Math.round(height * 0.42) }}
        contentContainerStyle={styles.body}
        keyboardShouldPersistTaps="handled"
        persistentScrollbar
        onLayout={(event) => setViewport(event.nativeEvent.layout.height)}
        onContentSizeChange={(_, contentHeight) => setContent(contentHeight)}
        onScroll={(event: NativeSyntheticEvent<NativeScrollEvent>) => setOffset(event.nativeEvent.contentOffset.y)}
        scrollEventThrottle={64}
      >
        {children}
      </ScrollView>
      <View style={[styles.more, { height: Math.round(20 * Math.max(1, fontScale)) }]}>{more && <Caption testID={`${testID}-more`}>Scroll to read the rest before you approve</Caption>}</View>
      <Divider />
      <ReadingContext.Provider value={more}>
        <View style={styles.actions}>{actions}</View>
      </ReadingContext.Provider>
    </View>
  )
}

/** A marking drawn by the app around content that came from outside. Text, a dashed edge and a wash: never colour alone. */
export function UntrustedBand({ heading, detail, testID }: { heading: string; detail: string; testID?: string }) {
  const theme = useTheme()
  return (
    <View
      testID={testID}
      accessible
      accessibilityLabel={`${heading}. ${detail}`}
      style={[styles.band, { backgroundColor: theme.status.warningBg, borderColor: theme.status.warning }]}
    >
      <Meta style={[styles.bandHeading, { color: theme.status.warning }]}>{heading}</Meta>
      <Meta style={{ color: theme.status.warning }}>{detail}</Meta>
    </View>
  )
}

export function Field({ label, children, testID }: { label: ReactNode; children: ReactNode; testID?: string }) {
  return (
    <View style={styles.field} testID={testID}>
      {typeof label === 'string' ? <Caption>{label}</Caption> : label}
      {children}
    </View>
  )
}

export function ReadOnlyField({ label, value, mono = false, testID }: { label: string; value: string; mono?: boolean; testID?: string }) {
  return (
    <Field label={label} testID={testID}>
      <UntrustedText text={value} mono={mono} testID={testID === undefined ? undefined : `${testID}-value`} />
    </Field>
  )
}

/** A sunken box for evidence. `flush` lets rows inside it run to its edges, as the diff's washes do. */
export function Evidence({ children, flush = false }: { children: ReactNode; flush?: boolean }) {
  const theme = useTheme()
  return (
    <View
      style={[
        styles.evidence,
        flush ? styles.flush : styles.padded,
        { backgroundColor: theme.surface.sunken, borderColor: theme.border.hairline },
      ]}
    >
      {children}
    </View>
  )
}

export function Actions({
  onReject,
  onApprove,
  approveLabel,
  rejectLabel = 'Reject',
  busy,
}: {
  onReject: () => void
  onApprove: () => void
  approveLabel: string
  rejectLabel?: string
  busy: boolean
}) {
  // Rejecting never waits. Approving does, so a person cannot approve what is still below the fold.
  const reading = useContext(ReadingContext)
  return (
    <>
      <View style={styles.action}>
        <Button label={rejectLabel} onPress={onReject} disabled={busy} testID="reject" />
      </View>
      <View style={styles.action}>
        <Button label={approveLabel} onPress={onApprove} variant="primary" disabled={busy || reading} testID="approve" />
      </View>
    </>
  )
}

const styles = StyleSheet.create({
  frame: { borderWidth: 1, borderRadius: radius.card, overflow: 'hidden' },
  head: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: space.m, padding: space.xl, paddingBottom: space.m },
  title: { flexShrink: 1 },
  body: { paddingHorizontal: space.xl, paddingBottom: space.l, gap: space.l },
  band: { marginHorizontal: space.xl, marginBottom: space.m, padding: space.m, borderRadius: radius.control, borderWidth: 1, borderStyle: 'dashed', gap: 2 },
  bandHeading: { fontWeight: '700' },
  more: { paddingHorizontal: space.xl, justifyContent: 'center' },
  field: { gap: 2 },
  evidence: { borderWidth: StyleSheet.hairlineWidth, borderRadius: radius.control, overflow: 'hidden', gap: 2 },
  padded: { padding: space.m },
  flush: { paddingVertical: space.s },
  actions: { flexDirection: 'row', justifyContent: 'flex-end', gap: space.m, padding: space.l },
  action: { flexShrink: 1 },
})
