import { useEffect, useRef, useState, type ReactNode } from 'react'
import { AccessibilityInfo, Animated, BackHandler, Pressable, StyleSheet, View, useWindowDimensions, type StyleProp, type ViewStyle } from 'react-native'
import { useTheme } from '../theme'

function useSlide(open: boolean): { slide: Animated.Value; mounted: boolean } {
  const slide = useRef(new Animated.Value(0)).current
  const [mounted, setMounted] = useState(false)
  // A ref, so a change to the setting applies to the next slide and never restarts one in progress.
  const reduceMotion = useRef(false)

  useEffect(() => {
    let live = true
    void AccessibilityInfo.isReduceMotionEnabled().then((enabled) => {
      if (live) reduceMotion.current = enabled
    })
    const subscription = AccessibilityInfo.addEventListener('reduceMotionChanged', (enabled) => {
      reduceMotion.current = enabled
    })
    return () => {
      live = false
      subscription.remove()
    }
  }, [])

  const everMounted = useRef(open)
  useEffect(() => {
    if (open) {
      everMounted.current = true
      setMounted(true)
    } else if (!everMounted.current) {
      return undefined
    }
    const animation = Animated.timing(slide, { toValue: open ? 1 : 0, duration: reduceMotion.current ? 0 : 220, useNativeDriver: true })
    animation.start(({ finished }) => {
      if (finished && !open) setMounted(false)
    })
    return () => animation.stop()
  }, [open, slide])

  return { slide, mounted }
}

/**
 * A panel that slides in from one side over a scrim. It stays mounted until it has slid out, closes on
 * the scrim and on Android's Back button, and is as wide as the window allows up to a fixed width.
 * The caller positions it vertically and draws its surface through `style`.
 */
export function SidePanel({
  open,
  onClose,
  side,
  inset = 0,
  closeLabel,
  testID,
  scrimTestID,
  style,
  children,
}: {
  open: boolean
  onClose: () => void
  side: 'left' | 'right'
  /** How far the panel sits from its edge of the screen. */
  inset?: number
  closeLabel: string
  testID: string
  scrimTestID: string
  style?: StyleProp<ViewStyle>
  children: ReactNode
}) {
  const theme = useTheme()
  const { width } = useWindowDimensions()
  const panelWidth = Math.min(340, Math.round(width * 0.86))
  const { slide, mounted } = useSlide(open)

  // The latest onClose, so a caller's new inline function does not re-register the Back handler.
  const close = useRef(onClose)
  close.current = onClose
  useEffect(() => {
    if (!open) return undefined
    const subscription = BackHandler.addEventListener('hardwareBackPress', () => {
      close.current()
      return true
    })
    return () => subscription.remove()
  }, [open])

  if (!mounted) return null
  const away = panelWidth + inset
  return (
    <View style={StyleSheet.absoluteFill} pointerEvents={open ? 'auto' : 'none'} testID={testID}>
      <Animated.View style={[StyleSheet.absoluteFill, { backgroundColor: theme.surface.scrim, opacity: slide }]}>
        <Pressable style={StyleSheet.absoluteFill} onPress={onClose} accessibilityRole="button" accessibilityLabel={closeLabel} testID={scrimTestID} />
      </Animated.View>
      <Animated.View
        accessibilityViewIsModal
        style={[
          styles.panel,
          side === 'left' ? { left: inset } : { right: inset },
          { width: panelWidth },
          style,
          { transform: [{ translateX: slide.interpolate({ inputRange: [0, 1], outputRange: [side === 'left' ? -away : away, 0] }) }] },
        ]}
      >
        {children}
      </Animated.View>
    </View>
  )
}

const styles = StyleSheet.create({
  panel: { position: 'absolute' },
})
