import { runArguments } from './args.js'
import { ProcessError } from './errors.js'
import { Mailbox } from './mailbox.js'
import { start } from './process.js'
import type { BravebotEvent, Message, RunOptions, RunResult, Turn } from './types.js'

/**
 * Runs `bravebot --json-stream -p <prompt>` once and resolves with the events it wrote and the
 * result object that ended it (CLI-24).
 *
 * A run that ends badly still resolves: `result.ok`, `result.status` and `result.reason` say how it
 * ended, and `result.refusals[].principle` names what refused. The promise rejects only when there
 * is no result to give: the process could not be started, was stopped, or wrote something that is
 * not the stream.
 *
 * A prompt that starts with `-` is refused with an `ArgumentError`, since the command line would
 * read it as a flag.
 */
export function run(prompt: string, options: RunOptions = {}): Promise<Turn> {
  let args: string[]
  try {
    args = runArguments(prompt, options)
  } catch (error) {
    return Promise.reject(error as Error)
  }
  const events: BravebotEvent[] = []
  let result: RunResult | undefined
  return new Promise<Turn>((resolve, reject) => {
    const running = start(args, options, {
      message(message) {
        if (message.type === 'event') {
          events.push(message.event)
          options.onEvent?.(message.event)
        } else {
          result = message.result
        }
      },
      failed: reject,
      ended(info) {
        if (result) resolve({ events, result })
        else reject(new ProcessError('bravebot ended without writing a result object', info))
      },
    })
    // A one-shot run has nothing to send, so its stdin is closed at once and the command line never waits on it.
    running.endInput()
  })
}

/**
 * Like `run`, but gives each line as it is written: every event, then the result object.
 * Breaking out of the loop stops the process.
 */
export function stream(prompt: string, options: RunOptions = {}): AsyncIterable<Message> {
  const args = runArguments(prompt, options)
  const mailbox = new Mailbox<Message>()
  let sawResult = false
  const running = start(args, options, {
    message(message) {
      if (message.type === 'result') sawResult = true
      mailbox.push(message)
    },
    failed: (error) => mailbox.fail(error),
    ended(info) {
      if (sawResult) mailbox.end()
      else mailbox.fail(new ProcessError('bravebot ended without writing a result object', info))
    },
  })
  running.endInput()
  return {
    [Symbol.asyncIterator]() {
      return {
        next: () => mailbox.next(),
        return: () => {
          running.kill()
          mailbox.end()
          return Promise.resolve({ value: undefined, done: true as const })
        },
      }
    },
  }
}
