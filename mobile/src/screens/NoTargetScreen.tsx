import { StyleSheet, Text, View } from 'react-native'
import { DisabledComposer } from '../components/Composer'
import { ConversationCard } from '../components/ConversationCard'
import { Body } from '../components/primitives'
import { TopBar } from '../components/TopBar'
import { space, type, useTheme } from '../theme'

const REASON = 'No target is connected.'

/** What the app shows when a build offers no target: the same frame, with nothing to run a session on. */
export function NoTargetScreen() {
  const theme = useTheme()
  return (
    <View style={[styles.screen, { backgroundColor: theme.surface.app }]} testID="no-target">
      <TopBar title="Bravebot" />
      <ConversationCard>
        <View style={styles.body}>
          <Text accessibilityRole="header" style={[type.hero, styles.heading, { color: theme.ink.primary }]}>
            Nothing is connected
          </Text>
          <Body style={[styles.lede, { color: theme.ink.secondary }]}>
            A session runs on a target, such as a host machine. This build has none, so there is nothing to start.
          </Body>
        </View>
        <DisabledComposer reason={REASON} workspace="No workspace" />
      </ConversationCard>
    </View>
  )
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  body: { flex: 1, paddingTop: space.xxl * 2, paddingHorizontal: space.xl, gap: space.l, alignItems: 'center' },
  heading: { textAlign: 'center' },
  lede: { textAlign: 'center' },
})
