#!/usr/bin/env node
// A bridge stand-in whose own text carries characters a terminal would act on: its version, its
// build and the message of the error it answers session.new with. It supports nothing else.
import { createInterface } from 'node:readline'

const capability = { version: 1, start: 'session.view.start', scope: 'fresh_session', approvals: ['confirm', 'run', 'fetch', 'ask'], reconnect: false }
createInterface({ input: process.stdin }).on('line', (line) => {
  const request = JSON.parse(line)
  const answer =
    request.method === 'agent.info'
      ? { ok: { build: 'b\u001b[31m', version: 'v\u009b31m', configured: true, capabilities: { sessionView: capability } } }
      : { error: { code: 'bad_request', message: 'bad \u001b[2J ‮ \u{e0041} ⁢' } }
  process.stdout.write(`${JSON.stringify({ id: request.id, ...answer })}\n`)
})
