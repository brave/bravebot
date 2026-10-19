import type { ReactNode } from 'react'
import { StyleSheet, View } from 'react-native'

// Glyphs drawn from views on a 16 point grid, so the app needs no icon package. They take the
// colour and size they are given, and are hidden from assistive technology: the control or row
// that holds one carries the label.

interface IconProps {
  color: string
  size?: number
}

function Frame({ size = 16, children }: { size?: number; children: ReactNode }) {
  return (
    <View
      pointerEvents="none"
      style={{ width: size, height: size, alignItems: 'center', justifyContent: 'center' }}
      importantForAccessibility="no-hide-descendants"
      accessibilityElementsHidden
    >
      <View style={[styles.grid, { transform: [{ scale: size / 16 }] }]}>{children}</View>
    </View>
  )
}

export function ArrowUpIcon({ color, size }: IconProps) {
  return (
    <Frame size={size}>
      <View style={{ position: 'absolute', left: 7, top: 3, width: 2, height: 11, borderRadius: 1, backgroundColor: color }} />
      <View
        style={{
          position: 'absolute',
          left: 4,
          top: 2,
          width: 8,
          height: 8,
          borderTopWidth: 2,
          borderLeftWidth: 2,
          borderColor: color,
          transform: [{ rotate: '45deg' }],
        }}
      />
    </Frame>
  )
}

export function FolderIcon({ color, size }: IconProps) {
  return (
    <Frame size={size}>
      <View
        style={{
          position: 'absolute',
          left: 1,
          top: 2,
          width: 6,
          height: 4,
          borderTopWidth: 1.5,
          borderLeftWidth: 1.5,
          borderRightWidth: 1.5,
          borderTopLeftRadius: 2,
          borderTopRightRadius: 2,
          borderColor: color,
        }}
      />
      <View style={{ position: 'absolute', left: 1, top: 4, width: 14, height: 10, borderWidth: 1.5, borderRadius: 2, borderColor: color }} />
    </Frame>
  )
}

const styles = StyleSheet.create({
  grid: { width: 16, height: 16 },
})
