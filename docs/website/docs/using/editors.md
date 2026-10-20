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

The editor offers a standing allow only for a question that has something to record. A yes to the
trust question is kept: the option reads **Trust and remember** and the question names the file it is
written to, so later sessions started in that directory are not asked. Where a yes cannot be kept, the
option reads **Trust for this session** and the question says the answer cannot be kept here. A
refusal is never kept. See [Remembering the answer](../security/trust.md#remembering-the-answer).

A host that a sandboxed `run` line reaches and that [`sandbox.network`](../customize/configuration.md)
does not list is also asked about in the editor. An allow there is labelled as lasting the session and
writes no rule.

Text the model or a command produced and nobody vouched for is shown behind a bar down the left
side, as it is in the terminal. A question the editor integration has no lines for is refused
without being shown. A picture, an HTML tag with an attribute, and a `<script>` or `<style>` tag even with none, in a message is cut by a space, so the editor shows its
address and does not load it.

## Settings that approve for you

Whether you are asked is partly the editor's setting. If you configure the editor to allow every
permission request, or requests of one kind, it answers for you, including the trust question and
each later one, and Brave Bot cannot tell that from you choosing. The mode `acceptEdits` also
answers writes inside the directory before they reach the editor. Leave both off in a directory you
have not read.
