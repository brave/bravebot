interface Waiter<T> {
  resolve(result: IteratorResult<T, undefined>): void
  reject(error: Error): void
}

/** A queue read by one consumer: items arrive in order, then the queue ends or fails. */
export class Mailbox<T> {
  private readonly items: T[] = []
  private readonly waiting: Waiter<T>[] = []
  private ended = false
  private failure: Error | undefined

  push(item: T): void {
    if (this.ended) return
    const waiter = this.waiting.shift()
    if (waiter) waiter.resolve({ value: item, done: false })
    else this.items.push(item)
  }

  end(): void {
    if (this.ended) return
    this.ended = true
    for (const waiter of this.waiting.splice(0)) waiter.resolve({ value: undefined, done: true })
  }

  /** Ends the queue with an error, which is reported once the items already queued have been read. */
  fail(error: Error): void {
    if (this.ended) return
    this.ended = true
    this.failure = error
    for (const waiter of this.waiting.splice(0)) waiter.reject(error)
  }

  next(): Promise<IteratorResult<T, undefined>> {
    if (this.items.length > 0) return Promise.resolve({ value: this.items.shift() as T, done: false })
    if (this.failure) return Promise.reject(this.failure)
    if (this.ended) return Promise.resolve({ value: undefined, done: true })
    return new Promise((resolve, reject) => this.waiting.push({ resolve, reject }))
  }
}
