import type { ReactElement } from 'react'
import renderer, { act, type ReactTestInstance, type ReactTestRenderer } from 'react-test-renderer'
import { SafeAreaProvider } from 'react-native-safe-area-context'

const metrics = { frame: { x: 0, y: 0, width: 390, height: 844 }, insets: { top: 47, left: 0, right: 0, bottom: 34 } }

const mounted: ReactTestRenderer[] = []

afterEach(async () => {
  await act(async () => {
    for (const tree of mounted.splice(0)) tree.unmount()
  })
})

/** Show new props for an element already rendered, as a parent re-rendering would. */
export async function update(tree: ReactTestRenderer, element: ReactElement): Promise<void> {
  await act(async () => {
    tree.update(<SafeAreaProvider initialMetrics={metrics}>{element}</SafeAreaProvider>)
  })
}

export async function render(element: ReactElement): Promise<ReactTestRenderer> {
  let tree!: ReactTestRenderer
  await act(async () => {
    tree = renderer.create(<SafeAreaProvider initialMetrics={metrics}>{element}</SafeAreaProvider>)
  })
  mounted.push(tree)
  return tree
}

export function textOf(node: ReactTestInstance | ReactTestRenderer): string {
  const root = 'root' in node ? node.root : node
  const parts: string[] = []
  const visit = (current: ReactTestInstance | string): void => {
    if (typeof current === 'string') {
      parts.push(current)
      return
    }
    current.children.forEach(visit)
  }
  visit(root)
  return parts.join('')
}

/** The host element with this testID. */
export function byTestId(tree: ReactTestRenderer, testID: string): ReactTestInstance {
  const found = tree.root.findAll((node) => node.props.testID === testID && typeof node.type === 'string')
  if (found.length === 0) throw new Error(`no element with testID ${testID}`)
  return found[0]!
}

export function has(tree: ReactTestRenderer, testID: string): boolean {
  return tree.root.findAll((node) => node.props.testID === testID && typeof node.type === 'string').length > 0
}

/** Press a control as a person would: a disabled control does nothing. */
export async function press(tree: ReactTestRenderer, testID: string): Promise<void> {
  const target = tree.root.findAll((node) => node.props.testID === testID && typeof node.props.onPress === 'function')[0]
  if (!target) throw new Error(`nothing pressable with testID ${testID}`)
  if (target.props.disabled === true) return
  await act(async () => {
    target.props.onPress()
  })
}

export function allText(tree: ReactTestRenderer): string {
  return textOf(tree)
}

/** Whether the control that is drawn, as assistive technology sees it, is disabled. */
export function isDisabled(tree: ReactTestRenderer, testID: string): boolean {
  return byTestId(tree, testID).props.accessibilityState?.disabled === true
}
