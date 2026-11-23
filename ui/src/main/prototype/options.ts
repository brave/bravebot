/**
 * Whether this process was started by `bravebot-host` as one of its clients, and how.
 *
 * The host starts a client program with its channel as the program's standard input and output and
 * the arguments the person put on the host's command line (`--arg`). The desktop is such a program
 * when it is given `--bravebot-host-client`, with the name the person gave it on that command line
 * and the folder it opens sessions in. Nothing else makes this process a client, and no environment
 * variable does: a packaged app started from a shell reads the same arguments and no more.
 */

import { isAbsolute } from 'node:path'

export interface HostClientOptions {
  /**
   * The name the person gave this client on the host's command line, written again here: the host
   * does not tell a client its name, so the window trusts the two to be the same. It is what the
   * window shows as this client, and compares with the name in the view to say "this window".
   */
  name: string
  /** The folder sessions open in. */
  workspace: string
}

/** The flag that makes the desktop a host client. */
export const HOST_CLIENT_FLAG = '--bravebot-host-client'

const NAME = /^[a-z0-9-]{1,32}$/

/**
 * The options of a desktop started as a host client, null where it was not, and an Error where it
 * was and the arguments are wrong. A wrong set is refused rather than guessed at: a client that
 * named itself otherwise than the person did would be shown under a name the host never used.
 */
export function hostClientOptions(argv: readonly string[]): HostClientOptions | null | Error {
  if (!argv.includes(HOST_CLIENT_FLAG)) return null
  let name: string | undefined
  let workspace: string | undefined
  for (let at = 0; at < argv.length; at++) {
    const word = argv[at]
    if (word === '--host-client-name') name = argv[++at]
    else if (word === '--host-client-workspace') workspace = argv[++at]
  }
  if (name === undefined || !NAME.test(name)) return new Error('--host-client-name is the name the host gave this client: 1 to 32 of a-z, 0-9 and -')
  if (workspace === undefined || !isAbsolute(workspace)) return new Error('--host-client-workspace is the absolute path of the folder this client opens sessions in')
  return { name, workspace }
}
