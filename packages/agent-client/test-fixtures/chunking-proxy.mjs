// Runs a real bravebot-rpc and hands its output on in tiny pieces: one to three bytes at a time,
// with a turn of the event loop between them, so a reader sees lines and multi-byte characters
// split wherever the pieces fall. Input and the exit code pass through unchanged.
// usage: node chunking-proxy.mjs <bravebot-rpc>
import { spawn } from 'node:child_process'

const child = spawn(process.argv[2], [], { stdio: ['pipe', 'pipe', 'inherit'] })
process.stdin.pipe(child.stdin)

const queue = []
let pumping = false
let size = 0
async function pump() {
  if (pumping) return
  pumping = true
  while (queue.length > 0) {
    size = (size % 3) + 1
    const head = queue[0]
    const piece = head.subarray(0, size)
    if (head.length > size) queue[0] = head.subarray(size)
    else queue.shift()
    if (!process.stdout.write(piece)) await new Promise((resolve) => process.stdout.once('drain', resolve))
    await new Promise((resolve) => setImmediate(resolve))
  }
  pumping = false
}
child.stdout.on('data', (chunk) => {
  queue.push(chunk)
  void pump()
})
child.stdin.on('error', () => undefined)
child.on('close', async (code, signal) => {
  while (pumping || queue.length > 0) await new Promise((resolve) => setImmediate(resolve))
  process.exitCode = signal ? 1 : (code ?? 1)
  process.stdout.end()
})
process.on('SIGTERM', () => child.kill('SIGTERM'))
