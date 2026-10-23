import { act } from 'react-test-renderer'
import { Text } from 'react-native'
import { SidePanel } from '../components/SidePanel'
import { has, press, render, update } from './helpers'

const mockBack: { handlers: (() => boolean)[] } = { handlers: [] }
jest.mock('react-native/Libraries/Utilities/BackHandler', () => ({
  __esModule: true,
  default: {
    addEventListener: (_name: string, handler: () => boolean) => {
      mockBack.handlers.push(handler)
      return { remove: () => void (mockBack.handlers = mockBack.handlers.filter((candidate) => candidate !== handler)) }
    },
  },
}))

function panel(open: boolean, onClose: () => void) {
  return (
    <SidePanel open={open} onClose={onClose} side="right" inset={8} closeLabel="Close it" testID="panel" scrimTestID="scrim">
      <Text>inside</Text>
    </SidePanel>
  )
}

beforeEach(() => {
  mockBack.handlers = []
})

describe('a side panel', () => {
  it('closes on the scrim', async () => {
    const closes: string[] = []
    const tree = await render(panel(true, () => closes.push('scrim')))
    await press(tree, 'scrim')
    expect(closes).toEqual(['scrim'])
  })

  it('closes on the Back button, and takes the press so it goes no further', async () => {
    const closes: string[] = []
    await render(panel(true, () => closes.push('back')))
    expect(mockBack.handlers).toHaveLength(1)
    expect(mockBack.handlers[0]!()).toBe(true)
    expect(closes).toEqual(['back'])
  })

  it('leaves the Back button alone while it is closed', async () => {
    await render(panel(false, () => undefined))
    expect(mockBack.handlers).toHaveLength(0)
  })

  it('lets go of the Back button when it closes', async () => {
    const tree = await render(panel(true, () => undefined))
    expect(mockBack.handlers).toHaveLength(1)
    await update(tree, panel(false, () => undefined))
    expect(mockBack.handlers).toHaveLength(0)
  })

  it('unmounts once it has slid out, so it stops blocking touches', async () => {
    jest.useFakeTimers()
    try {
      const tree = await render(panel(true, () => undefined))
      expect(has(tree, 'panel')).toBe(true)
      await update(tree, panel(false, () => undefined))
      await act(async () => {
        await jest.advanceTimersByTimeAsync(600)
      })
      expect(has(tree, 'panel')).toBe(false)
    } finally {
      jest.useRealTimers()
    }
  })
})
