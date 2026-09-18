#!/usr/bin/env python3
"""Turn a raw terminal capture into the screen a person would have seen.

A full-screen program draws by moving the cursor and overwriting cells, so the
bytes it wrote are not the picture it painted. Stripping the escape sequences out
of a capture leaves the characters in the right order and the wrong shape: a box
border lands in the middle of a sentence and every word runs together. That is
what `drive_tui.py --squash` produces, and it is enough for a substring check and
not enough for anybody to look at. Replaying the same bytes against a grid gives
back the screen itself, which is the thing a reviewer or a bug report needs.

The output is text, and deliberately: it costs no hosting, it goes in a fenced
block in any issue or pull request body, and unlike an image it can be searched,
diffed and quoted. Colour is dropped for the same reason. A screenshot that
survives copy-and-paste is worth more than one that carries its palette.

    contrib/drive_tui.py session.txt --raw capture.txt -- target/debug/bravebot
    contrib/terminal-screenshot.py capture.txt
    contrib/terminal-screenshot.py - < capture.txt

`--cols` and `--rows` must match the terminal the capture was made at, because
the program laid every frame out against that size. Getting them wrong gives a
screen that is wrong in a way that looks plausible, so they default to the 120x40
`drive_tui.py` uses and should otherwise be passed explicitly.

Anything this does not model is named on stderr rather than dropped in silence,
since a screenshot nobody can trust is worse than no screenshot: see `--strict`.
"""

import argparse
import re
import sys

# One escape sequence, or one control character, or a run of printable text.
# CSI is matched with its parameter and intermediate bytes so an unknown final
# byte is reported as the whole sequence rather than leaking its parameters into
# the screen as text.
TOKEN = re.compile(
    r"""
      \x1b\[ (?P<csi_params> [0-9;:<=>?]* ) (?P<csi_inter> [ -/]* ) (?P<csi_final> [@-~] )
    | \x1b\] (?P<osc> .*? ) (?: \x07 | \x1b\\ )
    | \x1b (?P<esc_inter> [ -/] ) (?P<esc_final> [0-~] )
    | \x1b (?P<esc_alone> [0-~] )
    | (?P<control> [\x00-\x08\x0a-\x1f\x7f] )
    | (?P<text> [^\x00-\x1f\x7f]+ )
    """,
    re.VERBOSE | re.DOTALL,
)

# A CSI carrying a private parameter prefix sets a mode, or asks the terminal a
# question, or negotiates a protocol. Mouse reporting, bracketed paste, focus
# events, cursor visibility and the keyboard protocol are all here, and not one
# of them changes a cell, so a screenshot is the same with or without them.
#
# The alternate screen switch is deliberately among them. Returning to the
# primary screen is the last thing the interface does as it exits, and honouring
# it would blank the very frame this tool exists to capture.
#
# The exceptions are the two selective erases, which do clear cells but only
# those left unprotected by an attribute this does not track. Those are reported
# rather than guessed at.
PRIVATE_PREFIXES = "?><="
SELECTIVE_ERASES = {"J", "K"}

# Requests for the terminal to describe itself. The reply travels the other way,
# so nothing lands on the screen.
DEVICE_REPORTS = {"c", "n"}


class Screen:
    """A grid of characters, and a cursor that writes into it."""

    def __init__(self, cols, rows):
        self.cols, self.rows = cols, rows
        self.grid = [[" "] * cols for _ in range(rows)]
        self.col = self.row = 0
        self.unmodelled = {}

    def put(self, text):
        for char in text:
            if self.col >= self.cols:
                self.col = 0
                self.linefeed()
            self.grid[self.row][self.col] = char
            self.col += 1

    def linefeed(self):
        if self.row + 1 < self.rows:
            self.row += 1
        else:
            # Scrolling loses the top line, exactly as the terminal would.
            self.grid.pop(0)
            self.grid.append([" "] * self.cols)

    def goto(self, row, col):
        self.row = max(0, min(self.rows - 1, row))
        self.col = max(0, min(self.cols - 1, col))

    def erase_in_line(self, mode):
        if mode == 1:
            span = range(0, self.col + 1)
        elif mode == 2:
            span = range(0, self.cols)
        else:
            span = range(self.col, self.cols)
        for col in span:
            self.grid[self.row][col] = " "

    def erase_in_display(self, mode):
        if mode == 1:
            self.erase_in_line(1)
            rows = range(0, self.row)
        elif mode in (2, 3):
            rows = range(0, self.rows)
        else:
            self.erase_in_line(0)
            rows = range(self.row + 1, self.rows)
        for row in rows:
            self.grid[row] = [" "] * self.cols

    def note_unmodelled(self, sequence):
        self.unmodelled[sequence] = self.unmodelled.get(sequence, 0) + 1

    def display(self):
        lines = ["".join(row).rstrip() for row in self.grid]
        while lines and not lines[-1]:
            lines.pop()
        return lines


def numbers(params, count, default=1):
    """`params` as `count` integers, with the terminal's default for a blank."""
    parts = params.split(";")
    out = []
    for index in range(count):
        raw = parts[index] if index < len(parts) else ""
        out.append(int(raw) if raw.isdigit() else default)
    return out


def replay(capture, cols, rows):
    """Feed `capture` to a fresh screen and return it."""
    screen = Screen(cols, rows)
    for match in TOKEN.finditer(capture):
        text = match.group("text")
        if text is not None:
            screen.put(text)
            continue

        control = match.group("control")
        if control is not None:
            if control == "\r":
                screen.col = 0
            elif control in ("\n", "\x0b", "\x0c"):
                screen.linefeed()
            elif control == "\b":
                screen.col = max(0, screen.col - 1)
            elif control == "\t":
                screen.col = min(screen.cols - 1, (screen.col // 8 + 1) * 8)
            # Everything else here is a bell, a shift or a NUL: no cell changes.
            continue

        final = match.group("csi_final")
        if final is None:
            # An OSC sets the window title, and the two-byte escapes select a
            # character set or a keypad mode. None of them writes a cell.
            continue

        params = match.group("csi_params")
        if params and params[0] in PRIVATE_PREFIXES:
            if final in SELECTIVE_ERASES:
                screen.note_unmodelled(match.group(0))
            continue
        if final in DEVICE_REPORTS:
            continue

        if final in ("H", "f"):
            row, col = numbers(params, 2)
            screen.goto(row - 1, col - 1)
        elif final == "A":
            screen.goto(screen.row - numbers(params, 1)[0], screen.col)
        elif final == "B":
            screen.goto(screen.row + numbers(params, 1)[0], screen.col)
        elif final == "C":
            screen.goto(screen.row, screen.col + numbers(params, 1)[0])
        elif final == "D":
            screen.goto(screen.row, screen.col - numbers(params, 1)[0])
        elif final == "G":
            screen.goto(screen.row, numbers(params, 1)[0] - 1)
        elif final == "d":
            screen.goto(numbers(params, 1)[0] - 1, screen.col)
        elif final == "J":
            screen.erase_in_display(numbers(params, 1, default=0)[0])
        elif final == "K":
            screen.erase_in_line(numbers(params, 1, default=0)[0])
        elif final == "m":
            pass  # Colour and weight, which a text screenshot does not carry.
        else:
            screen.note_unmodelled(match.group(0))

    return screen


# One capture whose screen is known, per thing the replay has to get right.
CASES = [
    ("text is the line it wrote", "hello", 20, 2, ["hello"], []),
    # The reason the tool exists. Stripping the escapes here leaves "loadingdone".
    ("a redraw overwrites what it replaces", "loading\r\x1b[Kdone", 20, 2, ["done"], []),
    ("a wrap starts the next row", "abcdef", 4, 3, ["abcd", "ef"], []),
    ("a cell lands where the cursor was sent", "\x1b[2;3Hx", 5, 3, ["", "  x"], []),
    ("scrolling loses the top line", "one\r\ntwo\r\nthree", 8, 2, ["two", "three"], []),
    # Honouring this would blank the frame, since the interface leaves the
    # alternate screen on the way out and the capture ends just after.
    (
        "leaving the alternate screen keeps the frame",
        "\x1b[?1049hframe\x1b[?1049l",
        10,
        2,
        ["frame"],
        [],
    ),
    ("colour is dropped", "\x1b[31mred\x1b[0m", 10, 2, ["red"], []),
    # A sequence that moves cells this does not model would otherwise give a
    # plausible screen that is wrong, which is worse than no screen at all.
    ("an unmodelled sequence is named", "a\x1b[5Xb", 10, 2, ["ab"], ["\x1b[5X"]),
]


def selftest():
    """Prove the replay draws the screen, on captures whose answer is known.

    A renderer that is quietly wrong produces a plausible screen forever, and
    what it is for is pasting into a bug report somebody else has to act on. So
    each case here is one thing the replay has to get right, checked against the
    screen a terminal would have shown.
    """
    checks = []
    for name, capture, cols, rows, lines, unmodelled in CASES:
        screen = replay(capture, cols, rows)
        checks.append((name, screen.display() == lines, screen.display()))
        checks.append(
            (
                f"{name}: reports {len(unmodelled)} unmodelled",
                sorted(screen.unmodelled) == sorted(unmodelled),
                sorted(screen.unmodelled),
            )
        )

    broke = [(claim, got) for claim, held, got in checks if not held]
    for claim, got in broke:
        print(f"selftest: {claim}, got {got!r}", file=sys.stderr)
    if broke:
        print(f"{len(broke)} of {len(checks)} checks failed", file=sys.stderr)
        return 1
    print(f"selftest: {len(checks)} checks passed")
    return 0


def main():
    ap = argparse.ArgumentParser(
        description="Replay a raw terminal capture and print the screen it drew.",
    )
    ap.add_argument(
        "capture", nargs="?", help="file holding the raw capture, or - for stdin"
    )
    ap.add_argument(
        "--cols", type=int, default=120, help="the capture's width (default: 120)"
    )
    ap.add_argument(
        "--rows", type=int, default=40, help="the capture's height (default: 40)"
    )
    ap.add_argument(
        "--strict",
        action="store_true",
        help="exit non-zero if anything in the capture was not modelled, so a "
        "screen that may be wrong does not reach an issue body",
    )
    ap.add_argument(
        "--selftest", action="store_true", help="check the replay and render nothing"
    )
    args = ap.parse_args()

    if args.selftest:
        return selftest()
    if args.capture is None:
        ap.error("a capture file is required, or - for stdin")

    if args.capture == "-":
        capture = sys.stdin.buffer.read().decode("utf-8", "replace")
    else:
        with open(args.capture, "rb") as handle:
            capture = handle.read().decode("utf-8", "replace")

    screen = replay(capture, args.cols, args.rows)
    print("\n".join(screen.display()))

    if screen.unmodelled:
        for sequence, count in sorted(screen.unmodelled.items()):
            shown = sequence.replace("\x1b", "ESC")
            sys.stderr.write(f"warning: {shown!r} x{count} was not modelled\n")
        sys.stderr.write(
            "The screen above may differ from what the terminal showed. "
            "Do not paste it into a body without checking it.\n"
        )
        if args.strict:
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
