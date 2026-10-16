import { StyleSheet, TextInput, View } from 'react-native'
import { radius, readingWidth, space, type, useTheme } from '../theme'
import { ArrowUpIcon, FolderIcon } from './icons'
import { Meta } from './primitives'

const SEND = 36

/** The message box and, under it, the folder the session works in, as the desktop draws them: one object in two tiers. It cannot be used, and says why. */
export function DisabledComposer({ reason, workspace }: { reason: string; workspace: string }) {
  const theme = useTheme()
  return (
    <View style={styles.dock}>
      <View style={styles.inner}>
        <Meta testID="composer-note" accessibilityLiveRegion="polite" style={styles.note}>
          {reason}
        </Meta>
        <View style={[styles.shell, { backgroundColor: theme.surface.app, borderColor: theme.border.subtle }]}>
          <View style={[styles.box, { backgroundColor: theme.surface.raised, borderColor: theme.border.subtle }]}>
            <TextInput testID="composer-input" accessibilityLabel="Message" editable={false} multiline value="" style={[type.body, styles.input, { color: theme.ink.disabled }]} />
            <View style={styles.tools}>
              <View
                testID="send"
                accessibilityRole="button"
                accessibilityLabel="Send"
                accessibilityState={{ disabled: true }}
                style={[styles.send, { backgroundColor: theme.border.strong }]}
              >
                <ArrowUpIcon color={theme.surface.raised} size={18} />
              </View>
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
