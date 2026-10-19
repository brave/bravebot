import { RpcAgentClient, decodeUpdate } from '@brave/agent-client';

describe('the linked client package', () => {
  it('decodes a view update with its rows and pending question', () => {
    const update = decodeUpdate({
      sequence: 4,
      turn: 1,
      target: 1,
      status: 'waiting',
      pending: { row: 3, request: 7, kind: 'fetch', supported: true, data: { request: 7, url: 'https://example.test/', host: 'example.test' } },
      rows: [{ id: 3, turn: 1, kind: 'approval', event: 'fetch.request', data: { request: 7 }, resolved: false }],
    });
    expect(update.status).toBe('waiting');
    expect(update.pending?.request).toBe(7);
    expect(update.rows[0]?.kind).toBe('approval');
  });

  it('refuses a status the contract does not name', () => {
    expect(() => decodeUpdate({ sequence: 1, turn: 0, target: 0, status: 'sleeping', pending: null, rows: [] })).toThrow();
  });

  it('constructs a client, whose private state the bundler keeps working', () => {
    const client = new RpcAgentClient({ write: () => undefined });
    expect(client.workspaces()).toEqual([]);
  });
});
