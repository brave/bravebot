import { connectDevRelay, RelayRefusedError, type RelaySocket } from '@brave/agent-client/dev-relay'
import type { Connect } from './targets'

const deadlines = {
  ms: 30_000,
  schedule(ms: number, fire: () => void) {
    const timer = setTimeout(fire, ms)
    return () => clearTimeout(timer)
  },
}

/**
 * Reach `bravebot-host` through the development relay (`packages/agent-client/scripts/dev-relay.ts`)
 * on this device's 127.0.0.1, which a cable or emulator tunnel carries to the computer. Only a
 * development build offers it, and it is for synthetic data only.
 */
export const connectDevRelayTarget: Connect = async (port, code) => {
  let relay
  try {
    relay = await connectDevRelay({ port, code, deadlines, open: (url) => new WebSocket(url) as unknown as RelaySocket })
  } catch (error) {
    if (error instanceof RelayRefusedError && error.wrongCode) {
      throw new Error('That is not the code the relay showed, so the relay has ended. Type launch on the host for a new port and code.')
    }
    if (error instanceof RelayRefusedError) {
      throw new Error(`Nothing took the connection on port ${port}. Check that the relay is running, that the port is forwarded to this device, and that no other app is connected.`)
    }
    throw new Error('The relay did not answer as expected.')
  }
  const client = relay.client
  return {
    target: {
      id: `dev-relay-${port}`,
      label: 'Development relay',
      description: `bravebot-host through the relay on port ${port}. Use it with synthetic data only.`,
      suggestions: [],
      create: () => client,
    },
    ended: relay.ended,
  }
}
