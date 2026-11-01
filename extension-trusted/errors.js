// The errors a call ends in. This file is copied byte for byte into the second
// extension (extension-trusted/), so it imports nothing; shared-files.json lists
// every file that is.

// JSON-RPC's code for a call that was refused or could not be done.
export const SERVER_ERROR = -32000;

// An error whose message is the reply's message.
export class ToolError extends Error {
  constructor(message, code = SERVER_ERROR) {
    super(message);
    this.code = code;
  }
}

// An error for a call one of the extension's own rules stopped: a switch, the
// group, its lifetime, a list of sites, or the browser's permissions. The
// activity log tells it from a tool that was tried and did not complete.
export class Refusal extends ToolError {}
