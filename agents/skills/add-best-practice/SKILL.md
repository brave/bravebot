---
name: add-best-practice
description: 'Add a rule to docs/best-practices/. Checks that the rule belongs there and is not already covered, chooses the document, assigns the next id, and keeps the index current. Triggers on: add best practice, new best practice, add bp, new bp, /add-best-practice.'
argument-hint: '<rule, or a pull request review comment URL>'
disable-model-invocation: true
allowed-tools: Bash(git *), Bash(gh *), Bash(grep *), Bash(rg *), Bash(ls *), Bash(make check-narration*), Read, Edit, Write, Grep, Glob, Agent
---

# Add a best practice

A best practice is a rule a reviewer applies by reading a diff.
[docs/best_practices.md](../../../docs/best_practices.md) says what belongs in
`docs/best-practices/` and what does not. This skill adds one entry in the format the existing
documents use, under a stable id, without duplicating a rule, a check or a spec.

---

## Step 0: Work in a worktree

Make a worktree on a branch of its own from the latest `upstream/main`, and do every step below
there. The checkout the user started from is left alone.

```sh
git fetch upstream main
git worktree add -b add-best-practice-<topic> ../bravebot-add-best-practice-<topic> upstream/main
```

---

## Step 1: Understand the rule

From the argument, settle three things:

1. **What** the rule is.
2. **Why** it matters: the mistake a diff makes without it and what that costs.
3. **Where it comes from**, so the entry can be checked against it.

If the argument is a pull request review comment URL, read the comment and the code it is about
before drafting:

```sh
gh api repos/<owner>/<repo>/pulls/comments/<comment_id> --jq '{body, path, line, diff_hunk}'
```

Read the file at that path, around the commented line, and the rest of the comment thread. A
review comment is short because the reviewer could see the code, and a rule drafted from the text
alone is often a different rule. If the rule is still vague after that, ask the user before going
on.

---

## Step 2: Check that it belongs in this directory

A rule is refused at this step when any of these holds. Say which one and stop.

- **A tool enforces it or could.** Formatting, lints, clause numbering, lockfiles, dashes,
  attribution markers, `declassify` outside the gates, `deny.toml` exceptions without a reason and
  first-person narration are checks (`make check`, `make check-spec`, `make check-npm`,
  `make check-deps`, `make check-narration`, the security scan). A rule that can be written as a
  check is a bug against this directory. Offer to write the check instead.
- **A spec owns it.** How bravebot behaves is in [docs/specs/](../../../docs/specs/README.md), and a
  spec wins where it disagrees with a rule here. Find the spec with the table in that README and
  grep the clauses. If a clause already says it, point to the clause. If the behaviour is missing
  from the spec, the fix is a clause.
- **It is about the rule the repository exists for** (untrusted content never reaches the driver or
  the planner). That review pass is
  [docs/development/reviewing-for-the-rule.md](../../../docs/development/reviewing-for-the-rule.md).
  Propose the change there, and do not file it as a best practice.
- **It is not decided by reading a diff.** A preference about one person's style, or a rule nobody
  could hold a diff against, is not an entry.

---

## Step 3: Check for duplicates

Read `docs/best_practices.md`, then search every document for the topic:

```sh
grep -rn -i '<keyword>' docs/best-practices/
```

Read any match in full. If an entry already covers the rule, reply: "Not added: already covered by
**<title>** (`<ID>`) in `<document>.md`." If the entry is close and the new case sharpens it,
offer to amend that entry instead, and stop unless the user agrees.

Look at the pending pull requests too, since another one may be adding the same rule:

```sh
gh pr list --repo brave/bravebot --search 'best practice in:title' --state open
```

---

## Step 4: Choose the document

List the documents and read the top of the likely ones:

```sh
ls docs/best-practices/
```

Each document has a title, an `<!-- applicability: always -->` line and a short paragraph saying
what it holds. Put the entry in the document whose paragraph it fits. Where none fits:

- **A one-off:** add it to the closest document.
- **A topic that will attract more rules:** create a document.
  1. Name it for the topic, lowercase, hyphenated (`docs/best-practices/<topic>.md`).
  2. Start it with a `# Title` line, the applicability line and a short paragraph saying what a
     reviewer checks here that no tool does.
  3. Choose a prefix of two to four capital letters that no other document uses. Find the ones in
     use with:
     ```sh
     grep -ho '<a id="[A-Z]*-' docs/best-practices/*.md | sort -u
     ```
  4. Add a row to the table in `docs/best_practices.md`, in the same voice as the others.

---

## Step 5: Draft the entry

Read the entries of the target document first and match them. The format is:

````markdown
---

<a id="PREFIX-NNN"></a>

## Rule title, stated as the rule

**One bold sentence that is the rule.** The sentences after it say what a diff that follows the
rule looks like, and what a diff that breaks it looks like.

```rust
// Wrong: what is wrong with it.
<bad example>

// Right: what makes it right.
<good example>
```

**Why:** the cost of not following it.
````

- The title and the first sentence state the rule as a claim, in the present tense. A reader
  should not need the body to learn what is asked.
- A code example goes in only where it shortens the explanation. Use the language of the code the
  rule is about, and label the two halves `Wrong` and `Right` in comments, as the existing
  documents do. Prose-only entries are normal.
- Name what a reviewer looks for in a diff. A rule phrased as an attitude cannot be applied.
- Follow the repository's writing rules in [AGENTS.md](../../AGENTS.md): plain statements, no
  aphorisms or metaphors, no "not X but Y" contrast, no em dash anywhere, and no history of why the
  rule was added or who asked for it. Link to specs and documents with relative paths.
- Wrap at 100 columns.

Give the entry the next id now: the highest number in use for the prefix, plus one. Never reuse an
id and never fill a gap, because an id is linked from skills, specs and pull requests.

```sh
grep -ho '<a id="PREFIX-[0-9]*"' docs/best-practices/*.md | sort | tail -1
```

A first entry in a new document is `PREFIX-001`.

---

## Step 6: Validate the draft with a subagent

Before showing the draft to the user, launch an Agent to check it against the source. This is what
catches an entry that misreads the review comment or the code it was about.

Give the agent the source material and the draft, and ask for the checklist below. Do not describe
your own reading of the source, so that its reading is independent.

```
Verify a drafted best-practice entry against its source. Report PASS or FAIL.

SOURCE:
- Review comment and thread (if any): <text>
- File and lines: <path:line>, with the surrounding code pasted in
- Or, for a rule stated by the user: <the user's words>

DRAFT:
<the full entry>

CHECK:
1. Does the draft describe the problem the source shows?
2. Do the Wrong and Right examples match what the source flagged and fixed?
3. Is the Why technically correct?
4. Could a reviewer misapply the rule from the draft alone?
5. Does the draft claim anything the source does not support?
6. Does every file, command, make target and spec clause it names exist in this repository?
7. Does it restate a check, a spec clause or another entry in docs/best-practices/?

Reply "PASS: <one line>" or "FAIL: <what is wrong and what the source actually shows>".
```

On FAIL, revise and run it again. After a second FAIL, ask the user.

Then show the user the final draft, the document it goes in and its id. Write it only once they
approve.

---

## Step 7: Add the entry

1. Append the entry at the end of the target document, after a `---` line, so entries stay in id
   order. A new document gets the entry after its introduction and a `---`.
2. For a new document, add the row to `docs/best_practices.md`. For an existing document, update
   the "For" cell of its row if the entry widens what the document covers.
3. Check that the id is unique and the anchor is well formed:
   ```sh
   grep -ho '<a id="[A-Z]*-[0-9]*"' docs/best-practices/*.md | sort | uniq -d
   ```
   Nothing printed means no id is used twice.
4. Run `make check-narration`. It reads the commit messages and added lines for first-person
   process narration.

Nothing else formats or checks these documents, so read the diff once for em dashes, lines over
100 columns and a `---` between every pair of entries.

---

## Step 8: Commit and open the pull request

Follow [docs/development/commits.md](../../../docs/development/commits.md): one commit holds the
entry, the index row and nothing else, with no co-attribution trailer and no em dash.

1. Commit with a subject naming the rule and a body saying what a reviewer can now hold a diff to:
   ```
   best practices: <the rule, as a short claim>

   Adds <ID>. <One or two sentences: what the entry asks of a diff, and why no check or spec
   covers it.>
   ```
2. Before pushing, confirm who will author the pull request. When working as the bot this
   repository's commits and pull requests come from, `gh api user --jq .login` must print that
   account. Stop and tell the user if it does not, because a pull request's author cannot be
   changed afterwards.
3. Push the branch and open the pull request against `brave/bravebot`, from the fork remote:
   ```sh
   git push -u origin HEAD
   gh pr create --repo brave/bravebot --base main --title "best practices: <the rule>" --body "$(cat <<'EOF'
   ## Summary
   - Adds `<ID>` to `docs/best-practices/<document>.md`: <the rule>.
   - <Why it is not a check and not a spec clause.>
   <If from a review comment:> - Comes from <URL>.

   ## Checks
   - `make check-narration`
   EOF
   )"
   ```
4. Report the pull request URL.

Do not push, open the pull request or merge without the user having asked for the pull request. If
the user only asked for the entry, stop after the commit and ask.
