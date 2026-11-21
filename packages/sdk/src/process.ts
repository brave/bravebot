import { spawn } from 'node:child_process'
import { StringDecoder } from 'node:string_decoder'
import { ProcessError } from './errors.js'
import { LineSplitter, parseLine } from './lines.js'
import type { CommonOptions, ExitInfo, Message } from './types.js'

export interface Handlers {
  /** One line of stdout, read. */
  message(message: Message): void
  /** The process could not be used: it could not be started, or wrote a line that is not the stream. */
  failed(error: Error): void
  /** The process ended and everything it wrote has been given to `message`. */
  ended(info: ExitInfo): void
}

export interface Running {
  /** Writes one line to stdin. A line is one JSON object, so it holds no raw newline. */
  write(line: string): void
  endInput(): void
  /** Marks the run as stopped, so the close is reported to `failed` as a `ProcessError`, not to `ended`. */
  kill(): void
  readonly exited: Promise<ExitInfo>
}

/**
 * Starts `command` with `args`, reads its stdout a line at a time and hands each line on.
 *
 * Stderr is handed to `onStderr` as text or inherited, and never read here. The library reads from
 * stdout only the fixed fields of the envelope (see `parseLine`).
 */
export function start(args: readonly string[], options: CommonOptions, handlers: Handlers): Running {
  const command = options.command ?? 'bravebot'
  const child = spawn(command, args, {
    cwd: options.cwd,
    env: options.env,
    stdio: ['pipe', 'pipe', options.onStderr ? 'pipe' : 'inherit'],
    windowsHide: true,
  })
  const { stdin, stdout } = child
  if (!stdin || !stdout) throw new ProcessError(`${command} was started without its pipes`)
  let spawnError: Error | undefined
  let stopped = false
  let broken = false
  let number = 0

  const splitter = new LineSplitter()
  const deliver = (lines: string[]): void => {
    for (const line of lines) {
      if (broken) return
      number += 1
      try {
        const message = parseLine(line, number)
        if (message) handlers.message(message)
      } catch (error) {
        broken = true
        child.kill()
        handlers.failed(error as Error)
      }
    }
  }
  stdout.on('data', (chunk: Buffer) => deliver(splitter.push(chunk)))

  if (options.onStderr && child.stderr) {
    const decoder = new StringDecoder('utf8')
    const sink = options.onStderr
    child.stderr.on('data', (chunk: Buffer) => {
      const text = decoder.write(chunk)
      if (text !== '') sink(text)
    })
    child.stderr.on('end', () => {
      const text = decoder.end()
      if (text !== '') sink(text)
    })
  }

  // A write to a process that has ended is reported by the stream's error event, which would
  // otherwise be thrown. The process ending is what the caller learns of.
  stdin.on('error', () => undefined)

  const exited = new Promise<ExitInfo>((resolve) => {
    child.on('error', (error) => {
      spawnError = error
      handlers.failed(new ProcessError(`${command} could not be started`, { cause: error }))
      resolve({ code: null, signal: null, error })
    })
    child.on('close', (code, signal) => {
      deliver(splitter.end())
      const info: ExitInfo = { code, signal }
      if (spawnError) info.error = spawnError
      if (!broken && !spawnError) {
        if (stopped) handlers.failed(new ProcessError(`${command} was stopped`, { code, signal }))
        else handlers.ended(info)
      }
      resolve(info)
    })
  })

  let abort: (() => void) | undefined
  if (options.signal) {
    const signal = options.signal
    abort = () => {
      stopped = true
      child.kill()
    }
    if (signal.aborted) abort()
    else signal.addEventListener('abort', abort, { once: true })
    void exited.then(() => signal.removeEventListener('abort', abort as () => void))
  }

  return {
    write(line) {
      if (stdin.writable) stdin.write(`${line}\n`)
    },
    endInput() {
      if (stdin.writable) stdin.end()
    },
    kill() {
      stopped = true
      child.kill()
    },
    exited,
  }
}
