// A stand-in for a bravebot-rpc that predates the session view: it answers agent.info without
// capabilities and records every request line it receives on stderr. It supports nothing else.
import { createInterface } from 'node:readline'

const lines = createInterface({ input: process.stdin })
lines.on('line', (line) => {
  process.stderr.write(`request ${line}\n`)
  const request = JSON.parse(line)
  if (request.method === 'agent.info') {
    process.stdout.write(`${JSON.stringify({ id: request.id, ok: { build: 'legacy', version: '0.1.0', home: '/legacy' } })}\n`)
  } else {
    process.stdout.write(`${JSON.stringify({ id: request.id, error: { code: 'bad_request', message: 'unknown method' } })}\n`)
  }
})
