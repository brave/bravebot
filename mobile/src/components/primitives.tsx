import type { ReactNode } from 'react'
import { Pressable, StyleSheet, Text, View, type StyleProp, type TextProps, type TextStyle, type ViewStyle } from 'react-native'
import { displaySegments, displayString } from '../text/displayText'
import { hit, radius, space, type, useTheme, type Theme } from '../theme'

// First-strong isolate and pop-directional-isolate. The app adds them around content after
// escaping the content's own, so right-to-left letters inside it cannot reorder the labels around it.
const ISOLATE_START = '\u2068'
const ISOLATE_END = '\u2069'

type TextExtras = Pick<TextProps, 'numberOfLines' | 'accessibilityLiveRegion'>
interface TextOwn {
  children: ReactNode
  style?: StyleProp<TextStyle>
  testID?: string
}

/** The conversation's own type: the large role, 16 on 24. */
export function Body({ children, style, testID, ...extras }: TextOwn & TextExtras) {
  const theme = useTheme()
  return (
    <Text testID={testID} {...extras} style={[type.body, { color: theme.ink.primary }, style]}>
      {children}
    </Text>
  )
}

/** Interface text inside cards and controls: the default role, 14 on 22. */
export function UI({ children, style, testID, ...extras }: TextOwn & TextExtras) {
  const theme = useTheme()
  return (
    <Text testID={testID} {...extras} style={[type.ui, { color: theme.ink.primary }, style]}>
      {children}
    </Text>
  )
}

export function Meta({ children, style, testID, ...extras }: TextOwn & TextExtras) {
  const theme = useTheme()
  return (
    <Text testID={testID} {...extras} style={[type.meta, { color: theme.ink.secondary }, style]}>
      {children}
    </Text>
  )
}

export function Caption({ children, style, testID }: TextOwn) {
  const theme = useTheme()
  return (
    <Text testID={testID} style={[type.caption, { color: theme.ink.tertiary }, style]}>
      {children}
    </Text>
  )
}

interface UntrustedTextProps {
  text: string
  multiline?: boolean
  mono?: boolean
  /** `body` is the conversation's size, `ui` the size inside cards, `meta` the small secondary size. */
  size?: 'body' | 'ui' | 'meta'
  style?: StyleProp<TextStyle>
  numberOfLines?: number
  testID?: string
  selectable?: boolean
}

/**
 * The only way released content is drawn. Characters that could hide or reorder text are written
 * out as styled runs, the content is bidi-isolated, and nothing in it can switch on markup or style.
 */
export function UntrustedText({ text, multiline = false, mono = false, size = 'ui', style, numberOfLines, testID, selectable = true }: UntrustedTextProps) {
  const theme = useTheme()
  const segments = displaySegments(text, { multiline })
  return (
    <Text
      testID={testID}
      selectable={selectable}
      numberOfLines={numberOfLines}
      accessibilityLabel={displayString(text, { multiline })}
      style={[mono ? type.code : size === 'body' ? type.body : size === 'meta' ? type.meta : type.ui, { color: theme.ink.primary }, style]}
    >
      {ISOLATE_START}
      {segments.map((segment, at) =>
        segment.escaped ? (
          <Text key={at} style={[type.code, styles.escape, { color: theme.status.info, backgroundColor: theme.status.infoBg }]}>
            {segment.text}
          </Text>
        ) : (
          segment.text
        ),
      )}
      {ISOLATE_END}
    </Text>
  )
}

type Variant = 'primary' | 'secondary' | 'plain'

interface ButtonProps {
  label: string
  onPress: () => void
  variant?: Variant
  disabled?: boolean
  width?: number
  testID?: string
  accessibilityHint?: string
}

export function Button({ label, onPress, variant = 'secondary', disabled = false, width, testID, accessibilityHint }: ButtonProps) {
  const theme = useTheme()
  const colors = buttonColors(theme, variant, disabled)
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityHint={accessibilityHint}
      accessibilityState={{ disabled }}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [
        styles.button,
        { backgroundColor: pressed && !disabled ? colors.pressed : colors.background, borderColor: colors.border, width },
      ]}
    >
      <Text style={[type.uiStrong, { color: colors.ink }]} numberOfLines={1}>
        {label}
      </Text>
    </Pressable>
  )
}

function buttonColors(theme: Theme, variant: Variant, disabled: boolean) {
  if (disabled) return { background: theme.surface.sunken, pressed: theme.surface.sunken, border: 'transparent', ink: theme.ink.disabled }
  if (variant === 'primary') return { background: theme.ink.accent, pressed: theme.ink.accent, border: theme.ink.accent, ink: theme.ink.onAccent }
  if (variant === 'plain') return { background: 'transparent', pressed: theme.surface.hover, border: 'transparent', ink: theme.ink.accent }
  return { background: theme.surface.raised, pressed: theme.surface.hover, border: theme.border.strong, ink: theme.ink.primary }
}

type Tone = 'neutral' | 'warning' | 'error' | 'success' | 'info'

function toneColors(theme: Theme, tone: Tone) {
  return {
    neutral: { ink: theme.ink.secondary, background: theme.surface.sunken },
    warning: { ink: theme.status.warning, background: theme.status.warningBg },
    error: { ink: theme.status.error, background: theme.status.errorBg },
    success: { ink: theme.status.success, background: theme.status.successBg },
    info: { ink: theme.ink.accent, background: theme.surface.selected },
  }[tone]
}

/** A small filled pill, like the desktop's Manage link or a status label. */
export function Chip({ label, tone = 'neutral', testID, style }: { label: string; tone?: Tone; testID?: string; style?: StyleProp<ViewStyle> }) {
  const theme = useTheme()
  const colors = toneColors(theme, tone)
  return (
    <View testID={testID} style={[styles.chip, { backgroundColor: colors.background }, style]}>
      <Text style={[type.metaStrong, styles.tabular, { color: colors.ink }]} numberOfLines={1}>
        {label}
      </Text>
    </View>
  )
}

/** An outlined pill, like the desktop's Vetting on. */
export function OutlinePill({ label, tone = 'info', testID, style }: { label: string; tone?: Tone; testID?: string; style?: StyleProp<ViewStyle> }) {
  const theme = useTheme()
  const colors = toneColors(theme, tone)
  return (
    <View testID={testID} style={[styles.outline, { borderColor: tone === 'info' ? theme.border.focus : colors.ink }, style]}>
      <Text style={[type.metaStrong, { color: colors.ink }]} numberOfLines={1}>
        {label}
      </Text>
    </View>
  )
}

/** A count beside a heading. */
export function Count({ value, testID }: { value: number; testID?: string }) {
  const theme = useTheme()
  return (
    <View testID={testID} style={[styles.count, { backgroundColor: theme.surface.sunken }]}>
      <Text style={[type.metaStrong, styles.tabular, { color: theme.ink.secondary }]}>{value}</Text>
    </View>
  )
}

export function Divider({ style }: { style?: StyleProp<ViewStyle> }) {
  const theme = useTheme()
  return <View style={[{ height: StyleSheet.hairlineWidth, backgroundColor: theme.border.subtle }, style]} />
}

const styles = StyleSheet.create({
  escape: { fontWeight: '600' },
  button: {
    minHeight: hit,
    paddingHorizontal: space.xl,
    borderRadius: radius.control,
    borderWidth: StyleSheet.hairlineWidth,
    alignItems: 'center',
    justifyContent: 'center',
  },
  chip: { borderRadius: radius.pill, paddingHorizontal: space.l, paddingVertical: space.s, alignSelf: 'flex-start' },
  outline: { borderRadius: radius.pill, borderWidth: 1, paddingHorizontal: space.l, paddingVertical: space.s - 1, alignSelf: 'flex-start' },
  count: { borderRadius: radius.pill, minWidth: 28, paddingHorizontal: space.m, paddingVertical: 2, alignItems: 'center' },
  tabular: { fontVariant: ['tabular-nums'] },
})
