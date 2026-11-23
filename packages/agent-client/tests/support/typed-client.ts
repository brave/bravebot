import { closeSync, constants, openSync, writeSync } from 'node:fs'
import { within } from './wait.js'

/** Where a typed client's lines are read from: the host's standard error, and who to wake on more. */
export interface Output {
  lines(name: string): string[]
  readonly waiters: (() => void)[]
}

/**
 * What a typed client of `scripts/host-client.ts` says, one line each, as the host passes its
 * standard error on, and the way to type a line to it.
 */
export class Typed {
  private cursor = 0
  private readonly fd: number
  constructor(
    readonly name: string,
    private readonly host: Output,
    readonly fifo: string,
  ) {
    // Read and write, so opening it waits for no reader.
    this.fd = openSync(fifo, constants.O_RDWR)
  }

  type(line: string): void {
    writeSync(this.fd, `${line}\n`)
  }

  /** The lines this client has said that match `wanted`, in order, from the start. */
  said(wanted: RegExp): string[] {
    return this.host.lines(this.name).filter((line) => wanted.test(line))
  }

  /** The next line this client says, after the last one asked for, that matches `wanted`. */
  async next(wanted: RegExp): Promise<string> {
    return within(
      new Promise<string>((resolve) => {
        const look = () => {
          const lines = this.host.lines(this.name)
          for (let at = this.cursor; at < lines.length; at++) {
            if (wanted.test(lines[at]!)) {
              this.cursor = at + 1
              return resolve(lines[at]!)
            }
          }
          this.host.waiters.push(look)
        }
        look()
      }),
      `${this.name} to say ${wanted}`,
    )
  }

  /** End the client by typing `quit`, and release this end of its input. */
  leave(): void {
    try {
      this.type('quit')
    } finally {
      closeSync(this.fd)
    }
  }
}
