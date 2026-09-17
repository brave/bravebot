# Testing the interface

`cargo test` covers the interface a piece at a time: a key press becomes an action, an action is
handled, a screen is drawn. What it cannot reach is the wiring between those pieces, and that is
where the interface bugs have been. `contrib/drive_tui.py` runs a scripted session against a real
terminal so those paths can be exercised, and `contrib/README.md` says how. It needs a backend and
writes real sessions, so it is a tool to reach for deliberately rather than part of `make check`.

What a scripted session captures is bytes rather than a picture, and the interface draws by
overwriting cells, so the capture read as text is the right characters in the wrong shape.
`contrib/terminal-screenshot.py` replays one against a grid and prints the frame, which is what to
put in a bug report about something a person can see.
