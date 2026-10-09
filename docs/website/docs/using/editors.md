---
sidebar_position: 8
title: Using Brave Bot in an editor
description: Run a session from an editor that speaks the Agent Client Protocol, and what it is asked to approve there.
---

# Using Brave Bot in an editor

`bravebot-acp` runs a session for an editor that speaks the
[Agent Client Protocol](https://agentclientprotocol.com), such as Zed. The editor starts the
program, draws the conversation, and shows each question as an approval.

In Zed, declare it as a custom agent in your settings:

```json
{
  "agent_servers": {
    "Brave Bot": {
      "type": "custom",
      "command": "bravebot-acp",
      "args": []
    }
  }
}
```

The program is built with the rest of the workspace and sits beside `bravebot-rpc`.

## What the editor can send

Text you type is a prompt, exactly as in the terminal, and a `/` word in it is text for the model,
not a command. A picture you attach arrives as a pasted picture and a file you link arrives as a
dropped file. An embedded selection or other resource is refused, because it could only be carried
by putting its text in your words. MCP servers an editor lists are not started.

## Approvals

The first prompt in a directory asks whether to trust it, as the terminal does, and nothing runs
before you answer. After that, each write, command or fetch that the terminal would ask about is a
permission request in the editor with the same details. Only choosing an allow option approves.
Closing the request, an error, or no answer refuses it, and stopping the prompt refuses what is
waiting. A planner's own question is not shown as an approval and is declined.

The editor offers a standing allow only for a question that has something to record. Nothing
you answer about trusting a directory is remembered.
