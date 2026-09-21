#!/usr/bin/env bash
# A live turn through bravebot-rpc, end to end.
#
# Uses the configured backend credentials (settings, environment, or built-in).
# It drives a real model: expect it to cost a few tokens and
# take a few seconds.
#
# Usage, from anywhere:
#     ~/repos/bravebot-ui/scripts/smoke-turn.sh [working-directory]
#
# The working directory defaults to bravebot itself, so the agent has something
# real to read. Nothing is written: the prompt only asks a question, and this session
# declines to trust the directory anyway, so any write would be shown rather than applied.

set -uo pipefail

UI="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
WORKDIR="${1:-$UI/vendor/bravebot}"
RPC="$UI/target/debug/bravebot-rpc"

if [ ! -x "$RPC" ]; then
  echo "building bravebot-rpc..." >&2
  ( cd "$UI" && cargo build -p bravebot-bridge ) || exit 1
fi

# Settings can supply provider credentials even when no Brave service key is exported.
# Keep stdin open until completion and answer the fixture's read confirmation. The old
# pipe slept for two minutes but could never answer a question coming back from the agent.
python3 - "$RPC" "$WORKDIR" <<'PYTHON'
import json, queue, subprocess, sys, threading, time
from pathlib import Path

rpc, directory = sys.argv[1:]
process = subprocess.Popen([rpc], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
messages = queue.Queue()
def read():
    for line in process.stdout:
        messages.put(line)
    messages.put(None)
threading.Thread(target=read, daemon=True).start()
sequence = 0
handle = None
success = False
fixture = (Path(directory) / 'crates/core/src/label.rs').resolve()
def send(method, **params):
    global sequence
    sequence += 1
    process.stdin.write(json.dumps({'id': sequence, 'method': method, 'params': params}) + '\n')
    process.stdin.flush()

try:
    send('session.new', directory=directory)
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        try:
            line = messages.get(timeout=min(1, deadline-time.monotonic()))
        except queue.Empty:
            continue
        if line is None:
            break
        message = json.loads(line)
        if 'error' in message:
            print('[ERROR]', message['error'], flush=True)
            break
        if message.get('id') == 1:
            handle = message['ok']['session']
            send('trust.reply', session=handle, trusted=False)
        elif message.get('id') == 2:
            send('turn.send', session=handle, prompt='In one sentence, what is the purpose of crates/core/src/label.rs?')
        event = message.get('event')
        data = message.get('data', {})
        if event == 'vouch.request':
            requested = Path(data['path'])
            if not requested.is_absolute(): requested = Path(directory) / requested
            if requested.resolve() != fixture:
                print('[ERROR] Unexpected read confirmation:', data['path'], flush=True)
                break
            send('vouch.reply', session=handle, request=data['request'], decision='approve')
            print('[read ] Approved the source fixture', flush=True)
        elif event in ('confirm.request', 'run.request', 'output.request', 'ask.request'):
            print('[ERROR] Unexpected interaction:', event, flush=True)
            break
        elif event == 'turn.done':
            success = bool(data.get('reply', '').strip())
            print('[reply]', data.get('reply', '')[:600], flush=True)
            break
        elif event == 'turn.error':
            print('[ERROR]', data.get('message'), flush=True)
            break
        elif event in ('phase', 'tool.started', 'tool.finished'):
            print('[' + event + ']', data.get('phase') or data.get('verb'), flush=True)
finally:
    process.stdin.close()
    try: process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.terminate()
        process.wait(timeout=5)
print('RESULT: ok' if success else 'RESULT: failed — no successful reply', flush=True)
sys.exit(0 if success else 1)
PYTHON
