import { StyleSheet, View } from 'react-native'
import { useSafeAreaInsets } from 'react-native-safe-area-context'
import { readingWidth, space, titlebar, useTheme } from '../theme'
import { Body } from './primitives'

/** The head of the screen, drawn on the ground above the card, as the desktop draws it. */
export function TopBar({ title }: { title: string }) {
  const theme = useTheme()
  const insets = useSafeAreaInsets()
  return (
    <View style={{ paddingTop: insets.top, backgroundColor: theme.surface.app }}>
      <View style={styles.inner}>
        <Body style={styles.title}>{title}</Body>
      </View>
    </View>
  )
}

const styles = StyleSheet.create({
  inner: {
    height: titlebar + 4,
    width: '100%',
    maxWidth: readingWidth + space.xl * 2,
    alignSelf: 'center',
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: space.l,
  },
  title: { fontWeight: '600' },
})
