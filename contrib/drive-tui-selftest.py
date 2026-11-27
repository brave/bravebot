#!/usr/bin/env python3
# Copyright (c) 2026 The Brave Authors. All rights reserved.
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this file,
# You can obtain one at https://mozilla.org/MPL/2.0/.
"""When drive_tui.py ends a step, against stand-in interfaces that are busy, slow or silent.

Each stand-in is a small program run under the driver's pty. One that is busy after Enter says
EARLY instead of DONE when keys were waiting for it as it finished, which is the next step being
typed while it was busy. The timing tests measure how long a run took, with bounds wide enough
for a loaded machine.

The stand-in writes READY only once its terminal is raw, and the driver types the first step only
after the program has drawn, so no key reaches a terminal that would throw it away.
"""

import importlib.util
import re
import sys
import tempfile
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("drive_tui", HERE / "drive_tui.py")
drive_tui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(drive_tui)

# Echoes each key after `ECHO_DELAY`, or not at all when it is negative. On Enter it writes nothing
# for `BUSY` seconds, then says EARLY if a key arrived meanwhile and DONE if not. `q` ends it. Its
# READY line carries the time it was written, so a run is timed from then and not from its start.
INTERFACE = """
import os, select, sys, time, tty
ECHO_DELAY, BUSY = float(sys.argv[1]), float(sys.argv[2])
tty.setraw(0)
os.write(1, b"READY %.3f\\r\\n" % time.time())
while True:
    key = os.read(0, 16)
    if not key or key == b"q":
        os.write(1, b"BYE\\r\\n")
        break
    if key == b"\\r":
        time.sleep(BUSY)
        early = select.select([0], [], [], 0)[0]
        os.write(1, b"EARLY\\r\\n" if early else b"DONE\\r\\n")
    elif ECHO_DELAY >= 0:
        time.sleep(ECHO_DELAY)
        os.write(1, key)
"""


class Steps(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.interface = Path(self.scratch.name) / "interface.py"
        self.interface.write_text(INTERFACE)

    def tearDown(self):
        self.scratch.cleanup()

    def drive(self, steps, echo_delay, busy, answer=5.0):
        """What the stand-in wrote, and how long the run took from the moment it was ready.

        The stand-in's start is left out, since how long an interpreter takes to start on a loaded
        machine says nothing about when the driver ends a step.
        """
        argv = [sys.executable, "-I", str(self.interface), str(echo_delay), str(busy)]
        out = drive_tui.drive(argv, steps, quiet=0.5, settle=30, answer=answer)
        ended = time.time()
        ready = re.search(r"READY (\d+\.\d+)", out)
        self.assertIsNotNone(ready, f"the stand-in never said it was ready: {out!r}")
        return out, ended - float(ready.group(1))

    def test_the_next_step_waits_for_an_interface_busy_after_the_last_key(self):
        out, _ = self.drive([(10, "abc\r"), (5, "q")], echo_delay=0, busy=2.0)
        self.assertIn("DONE", out)
        self.assertNotIn("EARLY", out)

    def test_a_late_echo_of_an_earlier_key_is_not_taken_as_the_answer_to_the_last(self):
        # One earlier key, echoed 0.15 s late. The driver starts reading 0.12 s after the key and
        # reads until 0.3 s of quiet before it types Enter, so the echo is read off if it arrives
        # within about 0.42 s of the key, 0.27 s past its delay. A driver that took only what had
        # arrived would miss it at 0.12 s.
        out, _ = self.drive([(10, "a\r"), (5, "q")], echo_delay=0.15, busy=2.0)
        self.assertIn("DONE", out)
        self.assertNotIn("EARLY", out)

    def test_a_step_whose_keys_draw_nothing_ends_after_answer_seconds(self):
        out, took = self.drive([(30, "x"), (5, "q")], echo_delay=-1, busy=0, answer=1.5)
        self.assertIn("BYE", out)
        self.assertLess(took, 15, "the step waited out its timeout and not --answer")

    def test_a_step_whose_keys_draw_nothing_ends_at_its_timeout_when_that_is_sooner(self):
        out, took = self.drive([(2, "x"), (5, "q")], echo_delay=-1, busy=0, answer=600)
        self.assertIn("BYE", out)
        self.assertLess(took, 15, "the step waited past its own timeout")

    def test_a_step_whose_keys_draw_ends_on_quiet_and_not_after_answer_seconds(self):
        out, took = self.drive([(30, "ab"), (5, "q")], echo_delay=0, busy=0, answer=600)
        self.assertIn("BYE", out)
        self.assertLess(took, 15, "a step whose keys drew did not end once it was quiet")


if __name__ == "__main__":
    unittest.main()
