# Which build wrote a session

Every session record carries the build that produced it, and `bravebot --version` prints the same
string:

```
bravebot 0.1.0 (f2a6e1a, modified)
```

The commit is what the binary was compiled from, and `modified` means the tree had uncommitted
changes at that point. Both matter when reading a transcript back: a session that behaved oddly
is usually being read against code that has moved since, and the alternative to a stamp is
inferring the build from the transcript's own symptoms. Resuming a session recorded by a
different build says so, beside the note about a changed branch.

## Which surface wrote it

The record names the front end too, `terminal` or `desktop`, because the `bravebot` command and
the desktop application share `~/.bravebot` and write the same kind of record into it. The build
answers which code ran; this answers which of the two surfaces it drew, and a reader needs both
before treating a transcript as evidence: a detail that looks like the agent behaving oddly may be
the other surface's rendering of the same record. Resuming a session the other surface wrote says
so, alongside the build and branch notes.

It is written by the program doing the writing rather than carried forward out of the record, so
the word describes the turns being added rather than the turns already there. A record from before
this was kept names no surface, and one naming a surface this build has never heard of is reported
as it was written. `bravebot_session::sessions::Front` is the enumeration, and every caller of
`Handle::begin` and `Handle::resuming` states it: a front end that could leave it out would be
recorded as the other one. [specs/sessions.md](../specs/sessions.md#SESSION-29) is the clause.

The stamp is taken by `crates/stamp/build.rs`, which watches every crate's sources rather than only
its own, so `modified` cannot go stale while another crate changes underneath it. A build with no
git available says `(no git)` rather than naming a commit it cannot see.

It sits in a crate of its own, depending on nothing and linking no terminal library, because every
front end writes the same string into the record and a front end that draws nothing should not link
a terminal library to ask which build it is.
