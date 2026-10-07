import { ProtocolError } from './errors.js'

/**
 * Splits a text stream into newline-delimited messages.
 *
 * A chunk is not a message: it can hold several, or half of one. Blank lines are skipped, as the
 * bridge does on input. The caller decodes bytes to text, so a multi-byte character split across
 * two reads is already whole here. The splitting follows `receive` in ui/src/main/bridge.ts.
 */
export class LineFramer {
  private pending = ''

  constructor(private readonly maxLine = 16 * 1024 * 1024) {}

  /** Feed a chunk and return the complete lines it finished, in order. */
  push(chunk: string): string[] {
    this.pending += chunk
    const lines: string[] = []
    let newline = this.pending.indexOf('\n')
    while (newline !== -1) {
      const line = this.pending.slice(0, newline)
      this.pending = this.pending.slice(newline + 1)
      if (line.trim() !== '') lines.push(line)
      newline = this.pending.indexOf('\n')
    }
    if (this.pending.length > this.maxLine) {
      this.pending = ''
      throw new ProtocolError(`a message exceeded ${this.maxLine} characters without ending`)
    }
    return lines
  }

  reset(): void {
    this.pending = ''
  }
}
