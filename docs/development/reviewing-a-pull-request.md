# Reviewing a pull request

You do not need to read the code. Review from the linked issue, the description, the spec changes
and, if you can, the behaviour. CI runs the checks and the security scan, and `netzenbot-reviewer`
reviews every pull request.

Approve when the answer to each question is yes.

1. **Does the project want this?** Compare the linked issue with the description. If there is no
   issue, or the change does more or less than the issue asks, ask the author. If you are unsure
   the project wants it, ask a maintainer.
2. **Do you understand what it changes?** You should know what changes for a person using bravebot
   and what the change cannot do. If the description does not say, ask the author.
3. **Does it keep the README's promises?** Ask the author if the change does any of these:
   - lets content the agent reads, such as a web page, a file or a command's output, decide what
     the agent does next;
   - does something the person cannot see on screen or in the trace;
   - sends data somewhere new, or writes under `~/.bravebot` in an incognito session;
   - works with only one provider;
   - runs a command or grants a permission the person was not shown;
   - weakens the README, the rule in `agents/AGENTS.md`, or a spec clause.
4. **Is the security side covered?** This applies if the change touches permissions, credentials,
   the sandbox, an installer or how content is labelled, or fixes a security problem. The
   description says where else the same weakness exists, and each of those places has an issue. If a
   count in [labels.md](../specs/labels.md) changed, ask the author why the new place is allowed.
5. **Is the description accurate?** It names an issue for each defect it leaves unfixed
   ([GAP-001](../best-practices/gaps.md#GAP-001)), and uses `Closes` only if the change finishes the
   issue. For a behaviour change it says which regression the new tests reject
   ([commits.md](commits.md)). A new dependency says what it adds
   ([DEP-001](../best-practices/dependencies.md#DEP-001)).

If you can, run the change when it affects what a person sees or does. See
[testing-the-interface.md](testing-the-interface.md).

## Record a rule

If one of your review comments is a rule that applies to future pull requests, run
`/add-best-practice <comment URL>`. If the rule belongs somewhere else, such as a check or a spec
clause, the skill says so.

## Hand it off

If either of these is true, do not approve:

- you still do not understand the change after asking the author;
- the change is in an area you have not worked in, such as security, or is for a platform or
  provider you cannot run.

Comment on the pull request with who should review it and what to look at.
