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

export function MenuIcon({ color, size }: IconProps) {
  return (
    <Frame size={size}>
      {[3, 7, 11].map((top) => (
        <View key={top} style={{ position: 'absolute', left: 1, top, width: 14, height: 2, borderRadius: 1, backgroundColor: color }} />
      ))}
    </Frame>
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

export function StopIcon({ color, size }: IconProps) {
  return (
    <Frame size={size}>
      <View style={{ position: 'absolute', left: 3, top: 3, width: 10, height: 10, borderRadius: 2, backgroundColor: color }} />
    </Frame>
  )
}

export function CheckIcon({ color, size }: IconProps) {
  return (
    <Frame size={size}>
      <View
        style={{
          position: 'absolute',
          left: 3,
          top: 3.5,
          width: 10,
          height: 6,
          borderLeftWidth: 2,
          borderBottomWidth: 2,
          borderColor: color,
          transform: [{ rotate: '-45deg' }],
        }}
      />
    </Frame>
  )
}

const CHEVRON = { down: '45deg', up: '-135deg', right: '-45deg', left: '135deg' } as const

export function ChevronIcon({ color, size, direction = 'down' }: IconProps & { direction?: keyof typeof CHEVRON }) {
  return (
    <Frame size={size}>
      <View
        style={{
          position: 'absolute',
          left: 4.5,
          top: 4,
          width: 7,
          height: 7,
          borderRightWidth: 2,
          borderBottomWidth: 2,
          borderColor: color,
          transform: [{ rotate: CHEVRON[direction] }],
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

export function TerminalIcon({ color, size }: IconProps) {
  return (
    <Frame size={size}>
      <View style={{ position: 'absolute', left: 1, top: 2, width: 14, height: 12, borderWidth: 1.5, borderRadius: 2, borderColor: color }} />
      <View
        style={{
          position: 'absolute',
          left: 4,
          top: 5.5,
          width: 4,
          height: 4,
          borderRightWidth: 1.5,
          borderBottomWidth: 1.5,
          borderColor: color,
          transform: [{ rotate: '-45deg' }],
        }}
      />
      <View style={{ position: 'absolute', left: 8, top: 10, width: 4, height: 1.5, backgroundColor: color }} />
    </Frame>
  )
}

export function PanelRightIcon({ color, size }: IconProps) {
  return (
    <Frame size={size}>
      <View style={{ position: 'absolute', left: 1, top: 2, width: 14, height: 12, borderWidth: 1.5, borderRadius: 3, borderColor: color }} />
      <View style={{ position: 'absolute', left: 10, top: 2, width: 1.5, height: 12, backgroundColor: color }} />
    </Frame>
  )
}

/** An open ring with an arrowhead where it opens. */
export function ReplayIcon({ color, size }: IconProps) {
  return (
    <Frame size={size}>
      <View
        style={{
          position: 'absolute',
          left: 1,
          top: 1,
          width: 14,
          height: 14,
          borderRadius: 7,
          borderWidth: 1.75,
          borderColor: color,
          borderTopColor: 'transparent',
        }}
      />
      <View
        style={{
          position: 'absolute',
          left: 0.6,
          top: 1,
          width: 0,
          height: 0,
          borderLeftWidth: 3,
          borderRightWidth: 3,
          borderBottomWidth: 5,
          borderLeftColor: 'transparent',
          borderRightColor: 'transparent',
          borderBottomColor: color,
          transform: [{ rotate: '45deg' }],
        }}
      />
    </Frame>
  )
}

const styles = StyleSheet.create({
  grid: { width: 16, height: 16 },
})
