import type { ReactNode } from 'react'
import { StyleSheet, View } from 'react-native'
import { useSafeAreaInsets } from 'react-native-safe-area-context'
import { cardGap, radius, readingWidth, space, useTheme } from '../theme'

/**
 * The card the conversation sits on, inset from the ground around it. The bottom safe area sits under the
 * keyboard while it is up, so the card then needs no margin for it.
 */
export function ConversationCard({ children, keyboardOpen = false }: { children: ReactNode; keyboardOpen?: boolean }) {
  const theme = useTheme()
  const insets = useSafeAreaInsets()
  return (
    <View
      style={[
        styles.card,
        { marginBottom: keyboardOpen ? cardGap : Math.max(insets.bottom, cardGap), backgroundColor: theme.surface.panel, borderColor: theme.border.subtle },
      ]}
    >
      {children}
    </View>
  )
}

const styles = StyleSheet.create({
  card: {
    flex: 1,
    width: '100%',
    maxWidth: readingWidth + space.xxl * 2,
    alignSelf: 'center',
    marginHorizontal: cardGap,
    borderWidth: 1,
    borderRadius: radius.card,
    overflow: 'hidden',
  },
})
