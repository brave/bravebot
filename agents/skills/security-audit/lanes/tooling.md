## Lane: what a maintainer's own tooling runs

Other lanes ask about the product. This one asks about the tools the people who work on this
repository run on their own machines with their own credentials: the scripts in `agents/` and
`contrib/`, the skills and agent definitions, the hooks git runs on a commit, a rebase or a push, and
the `Makefile` targets. A pull request is a branch of files. A tool that checks one out, rebases it or
builds it runs whatever the branch contains in each step that reads it. The question is: **what does a
maintainer's tool run, from where, on whose files and with which credentials?**

Most of this sits outside the product's own guarantee, and a finding here is about the people who
maintain it. Look for the cases where a tool gives a branch's files something the maintainer's own
checkout never gave them.

### Where to start

The scripts and hooks, by path:

{tool_files}

The definitions that say what a skill or an agent may run, and the settings they run under:

{tool_definitions}

The places a script starts a process (`subprocess`, `os.system`, `shell=True`), by file with the
functions to open. `surface.json` lists each line:

{process_starts}

The `Makefile` lines that set git configuration:

{make_recipes}

### What to ask

**A step that runs a branch's files where it builds nothing.** Git runs the hooks in `core.hooksPath`,
and a relative path resolves against the worktree the command runs in. A tool that checks out a pull
request and runs `git rebase`, `git commit` or `git push` there runs the hooks that branch tracks. For
each script that runs git in a worktree it did not create from a ref it trusts, ask what
configuration the command carries (`-c core.hooksPath=...`, `--no-verify`) and what is in its
environment.

**Which credentials the step holds.** A skill or script that runs with a token, an ssh agent, a
signing key or an unconfined shell gives every command inside it all of them. Compare what each tool
needs with what its definition grants, and list the commands inside the grant that read a branch's
files.

**Whose files it reads.** Python that reads JSON, YAML, TOML or a pickle from a work directory, a
manifest or a pull request, and a path from one of them joined onto a directory without a check that
it stays inside. A deserialiser that can construct arbitrary objects is the shape to look for.

**Where a script is found.** A script or program run by a bare name, from `PATH` or from the current
directory, instead of from a location the maintainer chose. Python run from a directory holding files
a branch wrote loads whatever those files say an `import` is.

**What a make target sets.** A target that changes git configuration, installs a hook or points a tool
at a file in the tree makes everything after it run the tree's files. Ask what the setting resolves
against and who can write there.

### What is not this lane

The ordinary exposure of building a pull request: its `build.rs`, its `.cargo/config.toml` and its
tests run when somebody runs them. A script that runs only the maintainer's own files. A dependency
question, which is the supply-chain lane's. And a finding whose whole content is that a tool could in
principle be misused: name the file, the step that runs it, the credential it holds and who writes
the file, or do not file it.
