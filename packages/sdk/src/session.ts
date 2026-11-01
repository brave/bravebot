import { sessionArguments } from './args.js'
import { ArgumentError, ProcessError } from './errors.js'
import { Mailbox } from './mailbox.js'
import { start } from './process.js'
import type { BravebotEvent, ExitInfo, Positions, QuestionEvent, SessionOptions, Turn } from './types.js'

/**
 * One `bravebot --json-stream --input-format lines` process and the conversation it carries
 * (CLI-36). Turns run one after another over one session record.
 *
 * The session sends lines and reads lines. It decides nothing from what the process writes except
 * the envelope: whether a line is an event or a result, and, for an event named `question`, its
 * `id`, which an answer must name.
 */
export class Session {
  readonly #running: ReturnType<typeof start>
  readonly #turns = new Mailbox<Turn>()
  readonly #options: SessionOptions
  #events: BravebotEvent[] = []
  #inputClosed = false

  constructor(options: SessionOptions) {
    this.#options = options
    this.#running = start(sessionArguments(options), options, {
      message: (message) => {
        if (message.type === 'result') {
          this.#turns.push({ events: this.#events, result: message.result })
          this.#events = []
          return
        }
        this.#events.push(message.event)
        options.onEvent?.(message.event)
        if (message.event.event === 'question') this.#questioned(message.event as QuestionEvent)
      },
      failed: (error) => this.#turns.fail(error),
      ended: (info) => {
        if (this.#inputClosed) this.#turns.end()
        else this.#turns.fail(new ProcessError('bravebot ended while the session was open', info))
      },
    })
  }

  #send(line: Record<string, unknown>): void {
    if (this.#inputClosed) throw new ProcessError('the session is closed')
    this.#running.write(JSON.stringify(line))
  }

  /**
   * Starts a turn with `text`. A prompt sent while a turn runs waits and runs after it. Unless the
   * session was started with `trustInputLines`, the planner is given a reference to `text` and
   * never the words.
   */
  send(text: string): void {
    if (typeof text !== 'string' || text === '') throw new ArgumentError('the prompt must be a non-empty string')
    this.#send({ type: 'prompt', text })
  }

  /** Delivers `text` to the turn that is running at its next round boundary. If no turn runs, or the turn ended without taking it, it starts a turn of its own. */
  steer(text: string): void {
    if (typeof text !== 'string' || text === '') throw new ArgumentError('the text must be a non-empty string')
    this.#send({ type: 'steer', text })
  }

  /** Ends the turn that is running. Prompts waiting behind it still run. */
  stop(): void {
    this.#send({ type: 'stop' })
  }

  /** Answers a `question` event by position: one array for each question, and an empty array declines it. */
  answer(id: number, choices: Positions): void {
    this.#send({ type: 'answer', id, choices })
  }

  /**
   * Resolves with the next turn to end, in the order the turns end, and with `undefined` once the
   * session is closed and every turn has ended. Rejects if the process failed.
   */
  nextTurn(): Promise<Turn | undefined> {
    return this.#turns.next().then((step) => (step.done ? undefined : step.value))
  }

  /**
   * Sends `text` and resolves with the next turn to end. This is the turn of `text` only while no
   * other turn is running or waiting, since turns end in order and a `steer` may start a turn of
   * its own.
   */
  async prompt(text: string): Promise<Turn> {
    this.send(text)
    const turn = await this.nextTurn()
    if (!turn) throw new ProcessError('the session ended before the turn did')
    return turn
  }

  /** Closes stdin, which ends the process once every turn has ended. Resolves with how it ended. */
  close(): Promise<ExitInfo> {
    if (!this.#inputClosed) {
      this.#inputClosed = true
      this.#running.endInput()
    }
    return this.#running.exited
  }

  /** Stops the process without waiting for its turns. */
  kill(): Promise<ExitInfo> {
    this.#inputClosed = true
    this.#running.kill()
    return this.#running.exited
  }

  #questioned(event: QuestionEvent): void {
    const handler = this.#options.onQuestion
    if (!handler) return
    const report = (error: unknown): void => this.#options.onError?.(error instanceof Error ? error : new Error(String(error)))
    // A handler that fails, or returns something that is not positions, declines each question.
    const decline = (): Positions => event.questions.map(() => [])
    void (async () => {
      let choices: Positions
      try {
        const given = await handler(event)
        choices = isPositions(given) ? given : decline()
        if (choices !== given) report(new ArgumentError('onQuestion must return an array of arrays of integers'))
      } catch (error) {
        report(error)
        choices = decline()
      }
      try {
        if (!this.#inputClosed) this.answer(event.id, choices)
      } catch (error) {
        report(error)
      }
    })()
  }
}

function isPositions(value: unknown): value is Positions {
  return Array.isArray(value) && value.every((one) => Array.isArray(one) && one.every((at) => Number.isInteger(at)))
}

/** Starts a session. Throws an `ArgumentError`, and starts nothing, for an option the library does not offer. */
export function startSession(options: SessionOptions = {}): Session {
  return new Session(options)
}
