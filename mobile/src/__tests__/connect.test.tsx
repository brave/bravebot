import { act } from 'react-test-renderer'
import App from '../App'
import { connectDevRelayTarget } from '../devRelay'
import type { Connect, Connection } from '../targets'
import { allText, byTestId, has, isDisabled, press, render, textOf } from './helpers'
import { fakeTarget } from './support/appScenarios'
import { FakeClient } from './support/fake'

async function type(tree: Awaited<ReturnType<typeof render>>, testID: string, text: string) {
  await act(async () => {
    byTestId(tree, testID).props.onChangeText(text)
  })
}

/** A connect that records what it was asked and settles as the test says. */
function fakeConnect() {
  const asked: [number, string][] = []
  let settle!: { resolve: (made: Connection) => void; reject: (error: Error) => void }
  const connect: Connect = (port, code) => {
    asked.push([port, code])
    return new Promise((resolve, reject) => (settle = { resolve, reject }))
  }
  return { connect, asked, settle: () => settle }
}

function connection() {
  let end!: (reason: string) => void
  const ended = new Promise<string>((done) => (end = done))
  const client = new FakeClient({ workspaces: [{ id: 'w', name: 'work' }] })
  return { made: { target: fakeTarget(client, { id: 'relay', label: 'Development relay' }), ended }, end }
}

const flush = () => act(async () => await Promise.resolve())

describe('connecting to a target from the blank slate', () => {
  // A release build passes no connect, so it must not show a way to reach a relay.
  it('offers no form when the build cannot connect', async () => {
    const tree = await render(<App />)
    expect(has(tree, 'connect-form')).toBe(false)
  })

  // Nothing is opened for a port or code that cannot be right.
  it('keeps Connect disabled until the port is 1 to 65535 and the code is six digits', async () => {
    const { connect, asked } = fakeConnect()
    const tree = await render(<App connect={connect} />)
    for (const [port, code] of [['', ''], ['0', '123456'], ['65536', '123456'], ['8080', '12345'], ['8080', '12a456'], ['80 0', '123456']]) {
      await type(tree, 'connect-port', port!)
      await type(tree, 'connect-code', code!)
      expect(isDisabled(tree, 'connect')).toBe(true)
      await press(tree, 'connect')
    }
    await type(tree, 'connect-port', '65535')
    await type(tree, 'connect-code', '012345')
    expect(isDisabled(tree, 'connect')).toBe(false)
    expect(asked).toEqual([])
  })

  it('connects with the port and code typed and then shows the connected target', async () => {
    const { connect, asked, settle } = fakeConnect()
    const tree = await render(<App connect={connect} />)
    await type(tree, 'connect-port', '8080')
    await type(tree, 'connect-code', '012345')
    await press(tree, 'connect')
    expect(asked).toEqual([[8080, '012345']])
    expect(isDisabled(tree, 'connect')).toBe(true)
    await act(async () => settle().resolve(connection().made))
    await flush()
    expect(has(tree, 'no-target')).toBe(false)
    expect(textOf(byTestId(tree, 'topbar-target'))).toBe('Development relay')
  })

  // A wrong code ends that relay, so the code field is cleared for the code of the next launch.
  it('says why a connection failed, clears the code and stays on the form', async () => {
    const { connect, settle } = fakeConnect()
    const tree = await render(<App connect={connect} />)
    await type(tree, 'connect-port', '8080')
    await type(tree, 'connect-code', '012345')
    await press(tree, 'connect')
    await act(async () => settle().reject(new Error('That is not the code the relay showed.')))
    expect(textOf(byTestId(tree, 'connect-problem'))).toBe('That is not the code the relay showed.')
    expect(byTestId(tree, 'connect-code').props.value).toBe('')
    expect(byTestId(tree, 'connect-port').props.value).toBe('8080')
    expect(has(tree, 'chat-screen')).toBe(false)
  })

  it('returns to the form with the reason once the connection ends', async () => {
    const { connect, settle } = fakeConnect()
    const tree = await render(<App connect={connect} />)
    await type(tree, 'connect-port', '8080')
    await type(tree, 'connect-code', '012345')
    await press(tree, 'connect')
    const { made, end } = connection()
    await act(async () => settle().resolve(made))
    await flush()
    expect(has(tree, 'no-target')).toBe(false)
    await act(async () => end('the relay ended the connection: the host ended the connection'))
    await flush()
    expect(has(tree, 'connect-form')).toBe(true)
    expect(textOf(byTestId(tree, 'connect-problem'))).toBe('The connection ended: the relay ended the connection: the host ended the connection.')
    expect(allText(tree)).not.toContain('Development relay')
  })
})

/** A WebSocket that closes before opening with the code given, as a relay that refused would. */
function refusingSocket(code: number) {
  return class {
    readyState = 0
    onopen: (() => void) | null = null
    onmessage = null
    onerror = null
    onclose: ((event: { code: number; reason: string }) => void) | null = null
    constructor() {
      setTimeout(() => this.onclose?.({ code, reason: '' }), 0)
    }
    send() {}
    close() {}
  }
}

describe('the development relay adapter', () => {
  const original = globalThis.WebSocket
  afterEach(() => {
    globalThis.WebSocket = original
  })

  it('says that a wrong code ended the relay and a new launch is needed', async () => {
    globalThis.WebSocket = refusingSocket(4001) as unknown as typeof WebSocket
    await expect(connectDevRelayTarget(8080, '012345')).rejects.toThrow('That is not the code the relay showed, so the relay has ended. Type launch on the host for a new port and code.')
  })

  it('says that nothing took the connection when the relay could not be reached', async () => {
    globalThis.WebSocket = refusingSocket(1006) as unknown as typeof WebSocket
    await expect(connectDevRelayTarget(8080, '012345')).rejects.toThrow(/^Nothing took the connection on port 8080\./)
  })
})
