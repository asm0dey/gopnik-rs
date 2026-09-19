"""`ReadKey` must return on ONE keystroke, and only a pty can prove it.

The rest of this repo's net drives the game through a PIPE, where a
keypress `ReadKey` and a line-reading one are indistinguishable: both
consume one `\\n`-terminated line.  That is why the port shipped a
`ReadKey` that needed Enter while the splash's own banner said
`Нажми какую-нибудь кнопку`, and why 437 unit tests, 347 difftest
records and five frozen oracles all stayed green over it.

So this test allocates a real pty, sends ONE byte with no newline, and
requires the game to move on.  Run it against a debug build.
"""

import os
import pathlib
import pty
import select
import subprocess
import sys
import time
import unittest

REPO = pathlib.Path(__file__).resolve().parent.parent
BIN = REPO / "target" / "debug" / "gopnik"

BANNER = "Нажми какую-нибудь кнопку"
AFTER = "Год 2xxx"


def run_until(fd, seconds):
    """Drain `fd` for `seconds`, returning everything read so far."""
    buf = b""
    end = time.time() + seconds
    while time.time() < end:
        ready, _, _ = select.select([fd], [], [], 0.2)
        if ready:
            try:
                buf += os.read(fd, 65536)
            except OSError:  # slave closed
                break
    return buf


class ReadKeyTakesOneKeystroke(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(
            ["cargo", "build", "-q"], cwd=str(REPO), check=True)
        if not BIN.exists():
            raise unittest.SkipTest(f"no debug binary at {BIN}")

    def test_a_bare_keypress_advances_past_the_splash(self):
        pid, fd = pty.fork()
        if pid == 0:  # child: becomes the game, never returns
            os.execv(str(BIN), ["gopnik"])
        try:
            before = run_until(fd, 3.0).decode("utf-8", "replace")
            self.assertIn(BANNER, before,
                          "the splash never reached its ReadKey")
            self.assertNotIn(AFTER, before,
                             "the game ran past the ReadKey without input")

            os.write(fd, b"n")  # ONE byte, deliberately no newline
            after = (before
                     + run_until(fd, 3.0).decode("utf-8", "replace"))
            self.assertIn(
                AFTER, after,
                "ReadKey did not return on a single keystroke -- it is "
                "waiting for Enter, which the banner does not ask for")
        finally:
            os.kill(pid, 9)
            os.waitpid(pid, 0)
            os.close(fd)


if __name__ == "__main__":
    sys.exit(0 if unittest.main(exit=False).result.wasSuccessful() else 1)
