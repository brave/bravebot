## Lane: what the person's inherited environment can select to run

A program bravebot starts is loaded and launched by code that reads environment variables to choose
more code: the dynamic loader, an interpreter, the wrapper script a package manager put in front of
the program, or its runtime. The variables are the ones bravebot inherited from the person's shell.
Whoever can set one before bravebot starts, or can make the person's shell set one, chooses code that
then runs with the access the start gives the program, which for an unconfined start is the person's
`~/.ssh`, `~/.aws` and `~/.bravebot`.

`checkout-controls` covers what a cloned repository's files can set. The preamble's sentence that the
rule says nothing about "this program's own configuration and startup" covers the person's own
environment as an input, and it is no reason to drop a candidate here. This lane asks one question:
**for each program bravebot starts, which variables does its launcher read to choose code to run, and
does the start withhold the ones that can be set to a file or a program the person did not choose?**

The mechanism is `programs::start(program, inherits, path, selectors)` in
`crates/sandbox/src/programs.rs`. `selectors` is `CodeSelectors::Kept`, which passes the inherited
environment on as it is, or `CodeSelectors::Checked(&[(&str, Names)])`, which takes each variable in
`CODE_SELECTORS`, in `CODE_SELECTOR_PREFIXES` and in the list the start passes, and keeps it only when
`may_be_run` holds for its value. `Names::Options` is never kept. The clauses and the declarations:

{selector_clauses}

{selector_sites}

The documents that state the rule, which hold the programs the start list is written against:

{selector_docs}

### Where to start

{start_count} starts of a program outside the tests, with the name of the selectors argument each
passes (`(not found)` means the call has no `CodeSelectors::` within twelve lines, so read the call):

{start_sites}

A start the list misses is in scope. `programs::start_against` and any other function that builds a
`Command` for a program found by name are starts as well, and so is a start that reaches `start`
through a helper in another crate. Search `crates/` for the helper's callers and for `Command::new`
beside `programs::find`.

### What to ask

For each start, in this order:

**Which program, and who chose it.** Resolve the name the start passes. A program found by name
(`aws`, `gh`, `git`, `ssh`, `osascript`, a clipboard tool, `crontab`, `open`) is bravebot's own
choice, and so this lane's. A program the person named is out of scope below.

**Which variables its launcher reads to choose code.** Read the program's launcher as well as the
program. Check the shebang and the interpreter it names (`python3`, `node`, `ruby`, `perl`, `bash`,
`java`), what that interpreter reads on start (`PYTHONPATH`, `PYTHONHOME`, `PYTHONUSERBASE`,
`PYTHONSTARTUP`, `NODE_OPTIONS`, `NODE_PATH`, `RUBYOPT`, `RUBYLIB`, `PERL5OPT`, `PERL5LIB`,
`JAVA_TOOL_OPTIONS`, `BASH_ENV`, `ENV`), what the dynamic loader reads (`LD_PRELOAD`,
`LD_LIBRARY_PATH`, `LD_AUDIT`, `DYLD_INSERT_LIBRARIES`, `DYLD_LIBRARY_PATH`, `DYLD_FRAMEWORK_PATH`),
and what a wrapper script or a runtime reads (`CLOUDSDK_PYTHON`, `CLOUDSDK_PYTHON_ARGS`,
`GIT_EXEC_PATH`, `GIT_SSH`, `GIT_SSH_COMMAND`, `GIT_ASKPASS`, `SSH_ASKPASS`, `GIT_CONFIG_*`,
`GIT_PAGER`, `PAGER`, `EDITOR`, `DEVELOPER_DIR`, `XDG_DATA_DIRS`, `AWS_CA_BUNDLE`, `AWS_CONFIG_FILE`
where it names a credential process, `ELECTRON_RENDERER_URL`). The names above are examples and the
list is longer. Where the program is a script on this machine's `PATH`, read it. Where it is not
installed,
say which launcher you assumed and from what.

**Whether the start passes `Checked` or `Kept`.** `Kept` is a claim that the program's launcher reads
no selector, or that the environment it is given has already been reduced to a list of names by
`Inherits`. Check the claim against the launcher. A `Kept` start of a program whose launcher reads a
selector is a candidate. Also check what `Inherits` lets through, since a start that inherits only
named variables selects nothing else.

**Whether the program's own names are in the list.** For a `Checked` start, the list it passes is for
the names the program reads besides the generic table. Compare it with the variables found above. A
name the program reads that is in neither the list nor `CODE_SELECTORS` is a candidate. The test in
`programs.rs` that walks `CODE_SELECTORS` does not know about names a program reads that are not in it.

**Whether `CODE_SELECTORS` is missing a name.** For each variable found above that selects code for
any program in the list, check that it is in `CODE_SELECTORS` or covered by a prefix. A name missing
from the generic table is a candidate against the table, and names each start it affects.

**Whether the `Names` kind is right.** `Names::Program` allows a bare name that resolves on `PATH`,
`Names::File` a file, `Names::Directory` a directory, `Names::Directories` a colon-separated list
each entry of which is a directory, `Names::BusAddress` a semicolon-separated D-Bus address each
entry of which is a socket outside every stage-writable directory, and
`Names::Options` is never allowed. A variable whose value is run as a command line (`GIT_SSH_COMMAND`,
`CLOUDSDK_PYTHON_ARGS`, `NODE_OPTIONS`) given a kind other than `Options` is a candidate. So is a file
check that passes a file the account can write when a confined stage could have written it (the
`outside_stage_writable` test in `programs.rs`).

### Already known, not to be filed again

These are tracked. Name the issue in `already_tracked` instead of filing a candidate:

- `#3793`: the AWS CLI start.
- `#3794`: `doctor --push-identity`'s `ssh-keygen`, `ssh` and `git`.
- `#3795`: the `gcloud` start.
- `#3796`: `ELECTRON_RENDERER_URL` in the desktop app's start.
- `#3797`: `doctor --sandbox-check`'s starts.
- `#3798`: the browser opener (`open`, `xdg-open`, `rundll32`).
- `#3799`: the request for this lane. A start that the tree now passes through `CodeSelectors` is
  covered by that change, and a start it still gives `Kept` is a candidate only when the launcher
  reads a selector.

### What is not this lane

A start of a program the person chose: the stages of `run`, a hook's program, a language server, the
editor, an MCP server, and the destination program of a scheduled entry. These run the person's
program with the person's environment on purpose, and the code they choose is the person's. The
start of bravebot's own executable. A variable the person exported to point bravebot at a program they
meant to run, where the start is of a program the person chose. A selector that only changes where a
program reads data and runs no code of the person's choosing. A finding whose whole content is that
the environment can be set: name the program, the variable its launcher reads, the code that variable
makes it run and the line of the start that passes it on, or do not file it.
