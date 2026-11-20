/**
 * What starts first. A desktop started by `bravebot-host` as one of its clients is the prototype
 * window and nothing else (HOSTCLIENT-1), so none of the ordinary app's top-level work runs: it
 * would start an agent process of its own beside the host's. Everything else is the app as it
 * always was.
 */

import { hostClientOptions } from './prototype/options'

const options = hostClientOptions(process.argv)

if (options instanceof Error) {
  // Standard output is the host's channel in a host client, so a word about the arguments goes to
  // standard error.
  process.stderr.write(`bravebot: ${options.message}\n`)
  process.exit(2)
} else if (options !== null) {
  void import('./prototype/main')
    .then(({ startHostClient }) => startHostClient(options))
    .catch((error: unknown) => {
      // A client that failed to start must end, not stay connected to the host and say nothing.
      process.stderr.write(`bravebot: could not start as a host client: ${error instanceof Error ? (error.stack ?? error.message) : String(error)}\n`)
      process.exit(1)
    })
} else {
  // A module that fails as the app loads is reported as it was when this file was the app itself:
  // thrown from here, where Electron shows its own error.
  void import('./index').catch((error: unknown) => {
    setImmediate(() => {
      throw error
    })
  })
}
