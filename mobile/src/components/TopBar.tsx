import type { ReactNode } from 'react'
import { Pressable, StyleSheet, View } from 'react-native'
import { useSafeAreaInsets } from 'react-native-safe-area-context'
import { hit, readingWidth, space, titlebar, useTheme } from '../theme'
import { MenuIcon, PanelRightIcon, ReplayIcon } from './icons'
import { Body, OutlinePill, UntrustedText } from './primitives'

function HeaderButton({
  testID,
  label,
  hint,
  onPress,
  children,
}: {
  testID: string
  label: string
  hint?: string
  onPress: () => void
  children: ReactNode
}) {
  const theme = useTheme()
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityHint={hint}
      onPress={onPress}
      hitSlop={4}
      style={({ pressed }) => [styles.button, { backgroundColor: pressed ? theme.surface.hover : 'transparent' }]}
    >
      {children}
    </Pressable>
  )
}

/** The head of the conversation, drawn on the ground above the card, as the desktop draws it. */
export function TopBar({
  title,
  target,
  onMenu,
  onReplay,
  onCalls,
}: {
  title: string | null
  /** The target's name, drawn as a pill. Absent when there is no target. */
  target?: string
  onMenu?: () => void
  onReplay?: () => void
  onCalls?: () => void
}) {
  const theme = useTheme()
  const insets = useSafeAreaInsets()
  return (
    <View style={[styles.wrap, { paddingTop: insets.top, backgroundColor: theme.surface.app }]}>
      <View style={styles.inner}>
        {onMenu !== undefined && (
          <HeaderButton testID="menu" label="Open sessions" onPress={onMenu}>
            <MenuIcon color={theme.ink.primary} size={20} />
          </HeaderButton>
        )}
        <View style={[styles.title, onMenu === undefined && styles.titleAlone]}>
          {title === null ? <Body style={styles.titleText}>New session</Body> : <UntrustedText text={title} size="body" numberOfLines={1} selectable={false} style={styles.titleText} />}
        </View>
        {target !== undefined && <OutlinePill label={target} tone="info" testID="topbar-target" style={styles.target} />}
        {onReplay !== undefined && (
          <HeaderButton testID="replay" label="Replay from the start" hint="Starts this session again from its recorded beginning" onPress={onReplay}>
            <ReplayIcon color={theme.ink.primary} size={20} />
          </HeaderButton>
        )}
        {onCalls !== undefined && (
          <HeaderButton testID="calls" label="Show calls made" hint="Opens the list of calls this session has made" onPress={onCalls}>
            <PanelRightIcon color={theme.ink.primary} size={20} />
          </HeaderButton>
        )}
      </View>
    </View>
  )
}

const styles = StyleSheet.create({
  wrap: {},
  inner: {
    height: titlebar + 4,
    width: '100%',
    maxWidth: readingWidth + space.xl * 2,
    alignSelf: 'center',
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: space.s,
    gap: space.s,
  },
  button: { width: hit, height: hit, borderRadius: 8, alignItems: 'center', justifyContent: 'center' },
  title: { flex: 1, paddingLeft: space.s },
  titleAlone: { paddingLeft: space.l },
  titleText: { fontWeight: '600' },
  target: { alignSelf: 'center' },
})
