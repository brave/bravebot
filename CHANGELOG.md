## [0.8.0](https://github.com/brave/bravebot/releases/tag/v0.8.0)

 - Added `/rewind`, which lists the turns a session can go back to, what each was asked and every path it would put back, and `/rewind 2` goes back that far. `/undo` now reaches five turns back rather than one, and both survive closing bravebot and resuming. ([#116](https://github.com/brave/bravebot/issues/116))
 - Added a `keybindings` block in `settings.json`, which moves seven of the chords onto keys of your own choosing. A chord has to carry ctrl or alt, and where two actions ask for the same key both give it up rather than one of them going dead. ([#104](https://github.com/brave/bravebot/issues/104))
 - Added `/manifest <task>`, which plans a whole task before anything is read, puts the frozen plan to you, and walks it once you agree. Nothing a step reads enters the conversation, so the session carries on afterwards with the reply and none of the content. ([#71](https://github.com/brave/bravebot/issues/71))
 - Added being told when a file changes: a read carries a token to compare one look with the next, and a turn arranges its own next look instead of asking you to start a loop. The wait can be as short as a second, measured from the end of a turn. ([#264](https://github.com/brave/bravebot/issues/264))
 - Added telling a turn when a background job has finished, with the status it exited on and what it printed since anybody last looked, so a build started in the background is no longer left unmentioned. ([#168](https://github.com/brave/bravebot/issues/168))
 - Added a wait to `job_output`, so watching a background job spends part of one turn rather than a whole turn per look. It comes back as soon as there is new output, the job ends, or the seconds it was given run out. ([#264](https://github.com/brave/bravebot/issues/264))
 - Added a `directory` argument to `run`, so a command can run in a subdirectory and later calls in the same turn stay there. A line outside your working directory is asked about every time and its output is quarantined, whatever you answered before. ([#166](https://github.com/brave/bravebot/issues/166))
 - Added `deadline_seconds` to `run`, which raises or lowers the 300 second default up to a ceiling of 600. It does nothing to a line started in the background. ([#167](https://github.com/brave/bravebot/issues/167))
 - Added reaching the matches past a search's cap: a capped result gives the offset to carry on from, and `search.maxFiles` and `search.maxSeconds` in a settings file raise the caps a search runs under. ([#99](https://github.com/brave/bravebot/issues/99))
 - Added `r` at a run prompt, which remembers your answer for that one exact line in every later session in the same directory. It stops only the asking, so the output stays quarantined, and `/status` lists what is covered and names the file to delete a line from. ([#70](https://github.com/brave/bravebot/issues/70))
 - Added a directory of its own to every session, outside your project, so a turn with an intermediate file to write no longer has to put it in your tree or be handed `/tmp`. Programs a `run` starts find it in `BRAVEBOT_SCRATCH_DIR`, and it goes when the session does. ([#304](https://github.com/brave/bravebot/issues/304))
 - Added `NO_COLOR`, which draws the session in the terminal's own inks and outranks whatever theme is in force, and `NO_MOTION`, which stills the glyph beside a running turn. ([#105](https://github.com/brave/bravebot/issues/105))
 - Added Ctrl-S on a prompt you walked back to, which opens the prompt search narrowed to this workspace rather than storing a second copy of a line the history already holds. ([#281](https://github.com/brave/bravebot/issues/281))
 - Added the cache split to `/status`, which says how much of the last turn's prompt the backend answered out of its cache and how much it wrote into one. ([#90](https://github.com/brave/bravebot/issues/90))
 - Changed the backend most sessions use to ask the service to cache the part of a prompt that is the same every round, and a request nothing ever sends again no longer pays to store its prefix at all. ([#289](https://github.com/brave/bravebot/issues/289))
 - Changed a directory you vouched for above your project to cover the project's own files where the project has no rule about them, rather than leaving them quarantined. ([#24](https://github.com/brave/bravebot/issues/24))
 - Changed a `run` result to name the second stream where there is anything on it, so an explanation of a failure is no longer read as the result. ([#165](https://github.com/brave/bravebot/issues/165))
 - Changed `/status` to name the line a loop is repeating rather than only its pacing, and `/loop` now says when an interval you typed was moved inside its bounds. ([#186](https://github.com/brave/bravebot/issues/186))
 - Changed a run prompt to say where a line whose arguments differ every time is answered, naming the settings file a pattern for the family goes in. ([#70](https://github.com/brave/bravebot/issues/70))
 - Changed `doctor` to name the state directory it resolved, or to say there is none, why, and what is not kept without one. ([#88](https://github.com/brave/bravebot/issues/88))
 - Changed the installer, the update notice and the documentation links to the repository's home at `brave/bravebot`.
 - Fixed Windows resolving no state directory at all, which left a stock install with no settings of your own, no session to resume, no prompt history and no skills. `USERPROFILE` answers where `HOME` is unset. ([#317](https://github.com/brave/bravebot/issues/317))
 - Fixed a language server being started, approved and asked to index the tree again on every message, so a session asks once and waits once. ([#162](https://github.com/brave/bravebot/issues/162))
 - Fixed an environment assignment in front of a command you had approved being run unasked, so `LD_PRELOAD=./evil.so git log` no longer matches the answer you gave about `git log` and no longer comes back as trusted text. ([#326](https://github.com/brave/bravebot/issues/326))
 - Fixed a `run` line naming a project file by its absolute path taking its label from a rule about the directory above the project, which handed the planner a page it had just fetched into that file as trusted content. ([#288](https://github.com/brave/bravebot/issues/288))
 - Fixed `/add-dir` on Windows admitting a directory like `C:\other`, whose rules were then decided by the answer you gave about your project. ([#318](https://github.com/brave/bravebot/issues/318))
 - Fixed being asked again about a file you had already vouched for, where the path reached it by another spelling: through a symlinked directory, by absolute path inside your project, or as `/tmp` on macOS where the directory is really `/private/tmp`. ([#302](https://github.com/brave/bravebot/issues/302))
 - Fixed a `deny` rule for a host being put to you as a question and then refusing the fetch after you had said yes. It refuses before there is a prompt. ([#144](https://github.com/brave/bravebot/issues/144))
 - Fixed `/undo` rewinding into the middle of the turn it was undoing, which left the prompt in the scrollback, the turn count one too high, and a session rewound past its first turn offering a resume into an empty conversation. ([#190](https://github.com/brave/bravebot/issues/190))
 - Fixed an imported subscription being read as absent where an interrupted import had left the file empty, which dropped the session to the free tier with nothing said and no symptom but the agent getting worse. ([#192](https://github.com/brave/bravebot/issues/192))
 - Fixed two wrong notices about a subscription: one on a turn whose model runs on Bedrock, which could not have spent it, and one saying your model had been substituted when you picked Automatic yourself. ([#193](https://github.com/brave/bravebot/issues/193))
 - Fixed a command typed while a turn runs being sent to the agent as a question about itself, so `/clear` mid-turn clears the session when the queue reaches it. ([#184](https://github.com/brave/bravebot/issues/184))
 - Fixed a prompt typed while a delegate was running being thrown away, which left the instruction reaching no agent at all and then becoming a turn of its own. ([#176](https://github.com/brave/bravebot/issues/176))
 - Fixed Escape and Ctrl-C not reaching a summary, an aside or a goal check while a delegate view or the prompt search stood over it: Ctrl-C there ended the session, and Escape wrote a notice behind a panel nobody could read it from. ([#171](https://github.com/brave/bravebot/issues/171))
 - Fixed the scroller: a paste or a dropped file no longer rewrites the prompt behind it, Ctrl-C and Ctrl-O close it while a search is half typed, and the key list names Ctrl-O. ([#178](https://github.com/brave/bravebot/issues/178))
 - Fixed the caret coming to rest inside a folded paste or a picture, where the next character typed split the marker and took the file off the turn with nothing on screen saying so. Up and Down land on a marker now, and in vi mode `e` no longer stops past the end of a line. ([#173](https://github.com/brave/bravebot/issues/173))
 - Fixed a stopped turn leaving a reply's last words on screen under no prompt, and a prompt being lost where the stop landed with a second line already typed. ([#172](https://github.com/brave/bravebot/issues/172))
 - Fixed a goal that gives up saying only how many rounds it spent, with nothing about what the last check said. ([#187](https://github.com/brave/bravebot/issues/187))
 - Fixed a `/cd` before a session's first turn leaving the session listed and resumable only from the directory it had left. ([#189](https://github.com/brave/bravebot/issues/189))
 - Fixed a forked session's files being readable by every other account on the machine until its first turn. ([#191](https://github.com/brave/bravebot/issues/191))
 - Fixed a line started in the background running without a redirection you were shown, where the redirection renamed a descriptor rather than naming a file. ([#164](https://github.com/brave/bravebot/issues/164))
 - Fixed `run` accepting a line that names your terminal, so `cat < /dev/tty` no longer takes your keystrokes, and a glob in program position is refused rather than standing for whichever file it matched today. ([#165](https://github.com/brave/bravebot/issues/165))
 - Fixed a one-shot run measuring every round against a default context window rather than the window of the model in force, which left compaction firing far too late or not at all. ([#157](https://github.com/brave/bravebot/issues/157))
 - Fixed `/loop everything you can` reading the words after `every` as an interval.
 - Fixed four features speaking English on a French screen: `/goal` and every verdict it reports, what `doctor` says about where sessions and prompt history are kept, and the two prompts before opening a directory a settings file named and before starting a language server.

## [0.7.0](https://github.com/brave/bravebot/releases/tag/v0.7.0)

 - Added `/goal`, which keeps a session working towards a condition you set. Each turn is judged against it, and where it does not hold the work goes back with the reason, up to ten rounds. `/goal clear` takes it off, and so does Ctrl-C with nothing running.
 - Added every model your AWS account can reach on Bedrock, not just Claude. A `provider` block naming `amazon-bedrock` states a region and lists as many models as you like, each shown under the name you give it, alongside the three tier variables. ([#202](https://github.com/brave/bravebot/issues/202))
 - Added `--model` and `--add-dir` to a one-shot run, so a script can name the model it wants and reach a directory beside the one it runs in. With no flag, a run takes the model `/model` recorded, the same one a session opening there would. ([#95](https://github.com/brave/bravebot/issues/95))
 - Improved `run` for a line that only reads: where every step is a known reading command over paths you have vouched for, it runs without asking and its output comes back as text rather than quarantined. ([#70](https://github.com/brave/bravebot/issues/70))
 - Changed a run nobody is watching to ignore the `allow` rules in your settings file, so a script that writes a file, runs a program or fetches a URL now needs `--dangerously-skip-permissions` to do it. The `deny` and `ask` rules still decide as they did. ([#145](https://github.com/brave/bravebot/issues/145))
 - Changed `permissions.additionalDirectories` to ask about each directory it names as a session opens, and to open and vouch for only the ones you accept. A settings file a checkout carries no longer opens a path elsewhere on your machine with nobody asked. ([#140](https://github.com/brave/bravebot/issues/140))
 - Changed hover text from a language server to come back quarantined. Nothing in an answer says which file the prose was written in, so it cannot be labelled by the file you asked about. ([#154](https://github.com/brave/bravebot/issues/154))
 - Changed the repository name to `bravebot`, so the install script and the line the update notice prints now name `github.com/brave-experiments/bravebot`. A copy already installed still finds its update, through a redirect. ([#133](https://github.com/brave/bravebot/issues/133))
 - Changed a processor's reply so that no word in it means leave this file alone: `UNCHANGED` is gone, and an answer that marks no document now writes nothing and reports which file stands as it was. A document whose whole content was that word could not be written before, because the reply was read as a verdict instead of as content. ([#28](https://github.com/brave/bravebot/issues/28))
 - Fixed a crash while editing the prompt in vi mode: marking a stretch in VISUAL mode and then shortening the line, with Backspace or Ctrl-W, panicked and left the terminal unusable. Every key that shortens the line now ends the selection. ([#175](https://github.com/brave/bravebot/issues/175))
 - Fixed `~/.bravebot` and the files under it being readable by every account on the machine, which included your prompt history and a stored credential. A directory an older build left open is narrowed, and a language server index an earlier build wrote one level too deep is removed. ([#114](https://github.com/brave/bravebot/issues/114))
 - Fixed the middle of a long command's output being lost. What `run` and `job_output` printed is kept whole now, so a capped result comes back with a reference to the rest instead of the command having to be run again. ([#200](https://github.com/brave/bravebot/issues/200))
 - Fixed a `deny` rule being ignored by a search or a listing that reached the file from a directory above it. A file a rule covers is left out before it is opened or named. ([#143](https://github.com/brave/bravebot/issues/143))
 - Fixed `run` not asking before a redirection feeds a file to a program, so `cat < ~/.ssh/id_rsa` no longer goes through on an earlier answer about `cat`. Such a run asks every time, and the answer cannot be remembered. ([#146](https://github.com/brave/bravebot/issues/146))
 - Fixed a delegate being offered `fetch_url`, and being able to put a question on your screen or replace your task list. ([#147](https://github.com/brave/bravebot/issues/147))
 - Fixed a write through a symlinked directory inside your working directory landing outside it, so the bytes go where the path you approved said. ([#214](https://github.com/brave/bravebot/issues/214))
 - Fixed a slow endpoint being given up on after 60 seconds and asked again, when the wait for a reply to begin is allowed to be ten times that. ([#158](https://github.com/brave/bravebot/issues/158))
 - Fixed a release being published without its Windows binary, which left every Windows install failing on a missing download. ([#195](https://github.com/brave/bravebot/issues/195))
 - Fixed a background job being reported as ended with the last of its output still unread.
 - Fixed reading an image or a PDF reaching a file outside your working directory and the directories you added. ([#141](https://github.com/brave/bravebot/issues/141))
 - Fixed a file a command's output was redirected into staying recorded as trusted, so reading it back no longer returns an unvouched program's output as trusted text. ([#142](https://github.com/brave/bravebot/issues/142))

## [0.6.0](https://github.com/brave/bravebot/releases/tag/v0.6.0)

 - Added `/btw`, which asks a question beside the work. It sends a copy of the conversation with your question on the end and puts neither half back, so the digression is not in front of the agent for the rest of the session. The answer opens under Ctrl-L as a row of its own, and a resume brings it back there.

## [0.5.1](https://github.com/brave/bravebot/releases/tag/v0.5.1)

 - Added an install script for macOS and Linux, so a machine without npm can install bravebot without building it: `curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh | sh`. It lands in `/usr/local/bin` unless `INSTALL_DIR` says otherwise, and the download is checked against its published checksum.
 - Added a notice at startup when a newer version has been published, giving the line that updates the copy you are running: the npm command where npm installed it, the install script where that did. The check runs in the background at most once a day, so nothing waits on it, and a build from source is left quiet.

## [0.5.0](https://github.com/brave/bravebot/releases/tag/v0.5.0)

 - Added code navigation through a language server: jump to a definition, find references, read a hover, list the symbols in a file or the workspace, and follow a call in either direction. The server runs with your access once you allow it, and its index is cached so later sessions start fast.
 - Added fetching a URL, so an agent can read a doc page or the issue behind an error. It asks before each one, and the page is quarantined: a page telling it to ignore its instructions is talking to nobody.
 - Added vi editing in the prompt box, with the modes, motions, operators, text objects like `ci(`, and visual mode drawn as you select. Turn it on in `/config`; the box is otherwise unchanged.
 - Added background commands, so an agent can start a server and then use it. `run` hands back a job name and `job_output` reads what it has printed since the last look.
 - Added regular expressions to search. A pattern is now matched as one instead of being treated as literal text and reported as no matches.
 - Added reading images. A screenshot or scanned page is now readable, though only a component with no tools looks at it.
 - Added npm as a way to install: `npm install -g @brave/bravebot`. The binary is checked against its published checksum.
 - Added `/config`, a panel for interface preferences, starting with vi editing.
 - Added project settings: `.bravebot/settings.json` and `settings.local.json` beside your work now layer over the global file, and `bravebot doctor` names which file a value came from.
 - Added the changed lines to what an edit reports, so an agent can see what it wrote instead of a count of replacements.
 - Added a warning when a turn edited files and ran nothing, so an unbuilt diff is not mistaken for a checked one. The agent is also asked once whether any of it runs.
 - Added a nudge to an agent that has read for eight rounds without writing anything, so settled work lands while the turn is still going.
 - Changed the model list to the ones this agent is offered, and the default to `automatic-brave-bot`. `automatic` still resolves to it. ([#129](https://github.com/brave/bravebot/issues/129))
 - Changed the prompt to state the working directory, platform, shell, whether the tree is a checkout, and the date, so an agent stops running `pwd` to find out.
 - Improved speed on Bedrock by telling the service which part of a request it has already read, instead of paying full price for the whole conversation every round.
 - Improved how much gets done per round: independent calls go out together, and a read with no line range returns the file up to a page instead of thirty lines at a time.
 - Fixed quarantined command output looking like a dead end, which left agents reading files one at a time. It now says how to see the output, vouch for the command, or read the file directly.

## [0.4.0](https://github.com/brave/bravebot/releases/tag/v0.4.0)

 - Added a command line to `run`, with pipes, `&&`, `||`, `;`, redirections and brace, glob and tilde expansion, compiled here rather than handed to a shell and put in front of you as a plan naming every file it would write.
 - Added shift-tab, which cycles a session between asking about every write, accepting edits, planning, and bypassing, and draws the mode in force under the prompt.
 - Added `--dangerously-skip-permissions`, which answers a write, a run, a command's output and vouching for a quarantined file without asking, while deny rules from the settings file still refuse.
 - Added `/undo`, which puts the session back where it stood before the most recent turn: the files that turn wrote, the conversation, and the turn count, spend and trust map that went with it. ([#91](https://github.com/brave/bravebot/issues/91))
 - Added `bravebot --fork <id>`, which copies a session into one with its own id and opens it, so a second approach starts from the part of the conversation worth keeping. ([#98](https://github.com/brave/bravebot/issues/98))
 - Added `/export`, which writes the conversation out as a markdown file under the working directory, named on the line or after the session id. ([#98](https://github.com/brave/bravebot/issues/98))
 - Added every command a turn ran to the ctrl-l list, after the delegates, so what a program printed is readable even where the planner was kept from it.
 - Added `CLAUDE.md` and `.claude/CLAUDE.md` as places a project's instructions are read from where `AGENTS.md` is absent, with a file short enough to be nothing but a pointer followed to the document it names.
 - Changed search to reach a hundred thousand files rather than two thousand, skip vendored dependencies, and accept several patterns at once along with a case-insensitive flag.
 - Fixed session records, temporary files and audit trails under `~/.bravebot` being created with the process umask, which left whole conversations readable by anyone with an account on the machine. ([#86](https://github.com/brave/bravebot/issues/86))
 - Fixed switching to a model the endpoint does not describe raising the context budget back to the default, which left it above the window actually in force so compaction never ran.
 - Fixed the Windows builds, which failed to compile the credential store; saving a credential there is refused rather than done without the file protection Unix gets. ([#115](https://github.com/brave/bravebot/issues/115))
 - Fixed Escape and ctrl-c cancelling the turn behind the delegate view or the prompt search instead of closing the view they were pressed in.
 - Fixed the context reading on the hint line going blank after a resume, a compaction or a failed turn, and marked a reading as approximate where the budget is one no model advertised. ([#69](https://github.com/brave/bravebot/issues/69))
 - Fixed brace groups in a search pattern being matched literally rather than expanded, which returned no matches in the same words as a search that read the whole tree and found nothing.

## [0.3.0](https://github.com/brave/bravebot/releases/tag/v0.3.0)

 - Added ctrl-l, which opens the list of delegates a session has run, so you can watch one working or read what it did afterwards.
 - Added a block under each delegate holding the last few things it did and the report it ends with, so work a delegate was sent off to do is readable where it was started.
 - Added `/effort`, which picks how hard a model thinks from low, medium, high, xhigh and max, keeps the choice beside the model and the theme, and sends no level to a model whose listing says it does not read one. ([#109](https://github.com/brave/bravebot/issues/109))
 - Added `bravebot --incognito`, a session that writes nothing to `~/.bravebot`: no prompt history, no session record, no title, and no audit trail.
 - Added a pair of colours to a theme file, `{"dark": ..., "light": ...}`, resolving to the arm matching the terminal background sensed at startup.
 - Changed delegates to run alongside the turn and each other, so a turn waits for the slowest piece of work rather than the sum of it, and one call can start up to eight of them.
 - Changed `catppuccin`, `gruvbox` and `solarized` to one row each in the theme picker, painted from the half matching the terminal background, with the six fixed halves still reachable through `/theme`.
 - Changed where the confinement is reported, from a row on every frame to the mark printed at startup and `/status` on request.
 - Fixed an answer given to a prompt inside a delegate overwriting the session's whole record of what you had vouched for, which put back rules that later answers had replaced.
 - Fixed a delegate that could not finish being reported as having answered.
 - Fixed a build with no Brave credentials refusing to load its configuration when only a gateway was configured, though the gateway uses its own key. ([#106](https://github.com/brave/bravebot/issues/106))
 - Fixed a model that writes its reasoning in `<think>` tags having that working drawn above every reply, kept in the session record and drawn again on every resume.
 - Fixed a Bedrock session that stopped working before its stated expiry going on being treated as good, which left every turn falling through to a sign-in opened where nobody could see it.
 - Fixed Bedrock attempting a sign-in for an AWS profile that is not configured, which is now reported as missing along with the profiles that exist.
 - Fixed ctrl-t being offered from the first frame, before any turn had left a trail for it to show.
 - Fixed an aside being drawn in bright black, which most terminal colour schemes leave too dim to read.

## [0.2.0](https://github.com/brave/bravebot/releases/tag/v0.2.0)

 - Added support for an OpenAI-compatible gateway, named by a `provider` block in `~/.bravebot/settings.json` in opencode's shape, whose models are offered beside the Brave and Bedrock ones.
 - Added `/loop`, which repeats a prompt on an interval you give it or at a pace each turn sets, until ctrl-c stops it.
 - Added `/cd`, which moves a session to another directory and carries its trusted paths with it.
 - Added ctrl-r, which searches the prompts already sent and puts the one you pick in the box rather than sending it.
 - Added the `permissions` block from Claude Code's settings file, so allow, ask and deny rules and `additionalDirectories` copied out of `~/.claude/settings.json` govern this agent unedited.
 - Added the `model` key in `~/.bravebot/settings.json`, where `opus`, `sonnet` and `haiku` name a tier and resolve to a model a reachable service serves.
 - Added search to the model list, which now filters as you type and is grouped by the service that answers rather than drawn as one flat list.
 - Added a breakdown to `/status` of where a session's time went: the model, tools, waiting for you, and the rest.
 - Changed the Bedrock environment variable to `BRAVEBOT_USE_BEDROCK`. The old name is no longer read.
 - Changed where an imported Brave subscription is kept, from the system keychain to one file only you can read, so a machine with no desktop session can use it. Import it again to move an existing one, and `--forget` no longer takes a channel.
 - Changed a prompt typed while a turn is running to reach that turn between rounds, instead of waiting until the whole turn has finished. ([#68](https://github.com/brave/bravebot/issues/68))
 - Fixed `run` handing this agent's own signing credentials to every program it starts, which no approval ever showed. ([#84](https://github.com/brave/bravebot/issues/84))
 - Fixed the delay before every turn on Bedrock, caused by re-checking the AWS session each time.
 - Fixed the confirm prompts and the trust prompt drawing their contents in the terminal's own colours instead of the theme's, which made them unreadable under a light theme in a dark terminal.
 - Fixed the prompt being drawn after the checks that run before a turn, which left it blank for a moment.
 - Fixed `?`, ctrl-t and ctrl-g being ignored while a turn was running, and the key list being left standing over a line written under it. ([#66](https://github.com/brave/bravebot/issues/66))
