import type { ReactNode } from 'react'
import { Text, type StyleProp, type TextProps, type TextStyle } from 'react-native'
import { type, useTheme } from '../theme'

type TextExtras = Pick<TextProps, 'numberOfLines' | 'accessibilityLiveRegion'>
interface TextOwn {
  children: ReactNode
  style?: StyleProp<TextStyle>
  testID?: string
}

/** The conversation's own type: the large role, 16 on 24. */
export function Body({ children, style, testID, numberOfLines, accessibilityLiveRegion }: TextOwn & TextExtras) {
  const theme = useTheme()
  return (
    <Text testID={testID} numberOfLines={numberOfLines} accessibilityLiveRegion={accessibilityLiveRegion} style={[type.body, { color: theme.ink.primary }, style]}>
      {children}
    </Text>
  )
}

export function Meta({ children, style, testID, numberOfLines, accessibilityLiveRegion }: TextOwn & TextExtras) {
  const theme = useTheme()
  return (
    <Text testID={testID} numberOfLines={numberOfLines} accessibilityLiveRegion={accessibilityLiveRegion} style={[type.meta, { color: theme.ink.secondary }, style]}>
      {children}
    </Text>
  )
}
