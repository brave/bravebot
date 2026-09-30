---
sidebar_position: 3
title: Connecting GitHub
description: Read GitHub issues and pull requests from a session through GitHub's own MCP server, with every tool that writes switched off.
---

# Connecting GitHub

This page assumes you have read [MCP servers](../mcp-servers.md), which covers
[adding a server](../mcp-servers.md#adding-one), [approving it](../mcp-servers.md#approving-one)
and [what a server can reach](../mcp-servers.md#what-a-server-can-reach).

GitHub publishes an [MCP server](https://github.com/github/github-mcp-server) (MIT, described in
its repository as "GitHub's official MCP Server"). This recipe runs the released binary of that
server, checks it against a digest so no package manager runs, and lets the session read issues
and pull requests and nothing else.

It does not use the other two ways GitHub documents:

- **The hosted server at `api.githubcopilot.com`** needs an `Authorization` header, and
  `bravebot mcp add --http` sends none.
- **The Docker image** cannot be used, because a confined server does not reach the Docker daemon.

The server is a Go binary, so nothing else has to be installed. Windows is not covered, because
bravebot does not start a server there.

1. Download release v1.12.2 into a directory of its own, check the digest, and unpack it there.
   Pick the asset for your machine:

   | Asset | sha256 |
   |---|---|
   | `github-mcp-server_Darwin_arm64.tar.gz` | `7e6c5aec43f26b82d3580e77a4ee26872bcd34b48c9a08d0eaef48b5d0563904` |
   | `github-mcp-server_Darwin_x86_64.tar.gz` | `6e73f5c9738050e44318d37aa919cb8ed2e29453d6341e942dc9c40c9c7ced5b` |
   | `github-mcp-server_Linux_arm64.tar.gz` | `2b30f9fcc061b57456cbe38ddc0f13c88863bad49557508a9196f2d1c4cb17a5` |
   | `github-mcp-server_Linux_x86_64.tar.gz` | `95843162759da2c31dde082dd145be35db82164594796c294414b69790c2290e` |

   ```sh
   ASSET=github-mcp-server_Darwin_arm64.tar.gz
   DIGEST=7e6c5aec43f26b82d3580e77a4ee26872bcd34b48c9a08d0eaef48b5d0563904
   mkdir -p ~/github-mcp-server && cd ~/github-mcp-server
   curl -fLO "https://github.com/github/github-mcp-server/releases/download/v1.12.2/$ASSET"
   echo "$DIGEST  $ASSET" | shasum -a 256 -c    # sha256sum -c on Linux
   tar -xzf "$ASSET" && rm "$ASSET"
   ```

2. Create a [fine-grained token](https://github.com/settings/personal-access-tokens/new) that can
   read what you want the session to read and nothing more. Choose **Only select repositories**
   and name the repositories, set an expiry, and give it these repository permissions, all
   **Read-only**: Issues, Pull requests, and Metadata (GitHub adds that one). Leave every other
   permission at no access.

3. Declare the server with the token. `read -rs` keeps the token out of your shell history and off
   the screen:

   ```sh
   read -rs GITHUB_TOKEN_VALUE     # paste the token, press Enter
   bravebot mcp add github -s user \
     -e GITHUB_PERSONAL_ACCESS_TOKEN="$GITHUB_TOKEN_VALUE" \
     -- ~/github-mcp-server/github-mcp-server stdio \
        --read-only --lockdown-mode --toolsets issues,pull_requests
   unset GITHUB_TOKEN_VALUE
   ```

   ```
     github   stdio   /Users/you/github-mcp-server/github-mcp-server stdio --read-only --lockdown-mode --toolsets issues,pull_requests
              variables: GITHUB_PERSONAL_ACCESS_TOKEN (stored)
   ```

   It then asks whether to use the server, and shows a digest line under these. Answer yes once
   the lines above are what you expect. The token is stored in `~/.bravebot/mcp.json` and is shown
   by its name and never as itself. `-s user` asks for the server in every session once you answer
   yes.

   The three flags are part of what you approve, so they show at the question:

   - `--read-only` leaves out every tool that writes.
   - `--toolsets issues,pull_requests` offers those two groups of tools and not the server's
     defaults, which also read repository contents and users.
   - `--lockdown-mode` is described [below](#what-you-have-handed-over).

4. Refuse the tools that write, in `~/.bravebot/settings.json`:

   ```json
   {
     "permissions": {
       "deny": [
         "Mcp(github:add_comment_to_pending_review)",
         "Mcp(github:add_issue_comment)",
         "Mcp(github:add_reply_to_pull_request_comment)",
         "Mcp(github:create_pull_request)",
         "Mcp(github:issue_write)",
         "Mcp(github:merge_pull_request)",
         "Mcp(github:pull_request_review_write)",
         "Mcp(github:sub_issue_write)",
         "Mcp(github:update_issue_comment)",
         "Mcp(github:update_pull_request)",
         "Mcp(github:update_pull_request_branch)"
       ]
     }
   }
   ```

   With `--read-only` the server does not offer any of them, so these rules matter on the day
   someone edits the declaration and drops the flag. `pull_request_review_write` and
   `merge_pull_request` are the two that can approve or merge a pull request. A `deny` rule holds
   where no question is asked, answer 2 and `--dangerously-skip-permissions` included.

The session then offers `github:issue_read`, `github:list_issues`, `github:search_issues`,
`github:list_pull_requests`, `github:pull_request_read`, `github:search_pull_requests`,
`github:get_label`, `github:list_issue_fields` and `github:list_issue_types`. What they return is
private and quarantined, so the model reads an issue or a pull request only through a processor or
after you let it out of quarantine (see [vetting](../../security/vetting.md)).

## What you have handed over

- **The token reads the issues and pull requests of the repositories you named.** It cannot
  comment, review, merge, or change anything. Taking it back is deleting it in your
  [GitHub token settings](https://github.com/settings/personal-access-tokens), and removing the
  server with `bravebot mcp remove github`.
- **The token sits in `~/.bravebot/mcp.json`.** Anything that can read that file as you can use
  the token until it expires.
- **Other people write what it reads.** On a public repository anyone with an account can file an
  issue or comment on one, and the session reads it. `--lockdown-mode` makes the server return
  content on a public repository only from authors who have push access to it. GitHub describes it
  as a best-effort filter and not a security boundary, and it does not limit what the token can
  read.
- **Lockdown needs push access.** The server asks GitHub whether each author has push access, and
  GitHub answers only a person who has it themselves. Without push access to a repository, a read
  of another author's issue fails with `403 Must have push access to view collaborator permission`.
  For a repository you cannot push to, drop `--lockdown-mode` and accept that the session reads
  text from strangers, or leave the repository out.
- **`git clone` and `git push` are not covered.** The server has no tool for either. This recipe
  offers no tool that reads a repository's files, though a pull request's changed files and diff
  can be read.
- **You update it yourself.** The directory holds the release you checked. A newer release is
  downloaded and checked the same way, against the digest in its own `checksums.txt`.
