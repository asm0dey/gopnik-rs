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


class CtrlDDoesNotQuit(unittest.TestCase):
    """Ctrl+D is a Unix key the original cannot see.

    DOS has no EOF keystroke, and the `Crt` unit this game links runs with
    `CheckEof = False` -- set at `1f16:003b` (`xor ax,ax` / `mov
    [0x3eb9],al`) and never overridden in the game's own code -- so not
    even DOS's own Ctrl+Z ends input there. Nothing the player types can
    stop the original this way.

    Ctrl+D only MEANS end-of-input at a `ReadLn`; at a `ReadKey` the tty is
    in raw mode and `0x04` is simply the key that was pressed, which is
    faithful and not what this tests. So this drives the game to its first
    real `ReadLn` -- the class prompt -- and sends Ctrl+D there.

    Only a pty can check it: on a pipe an end-of-input IS the genuine end
    of the input, and every other test in this repo depends on that.
    """

    CLASS_PROMPT = "Выбери кем ты будешь"

    @classmethod
    def setUpClass(cls):
        subprocess.run(
            ["cargo", "build", "-q"], cwd=str(REPO), check=True)
        if not BIN.exists():
            raise unittest.SkipTest(f"no debug binary at {BIN}")

    def test_ctrl_d_at_a_readln_does_not_end_the_process(self):
        pid, fd = pty.fork()
        if pid == 0:
            os.execv(str(BIN), ["gopnik"])
        try:
            # Pump keystrokes through the opening's ReadKeys until the
            # first ReadLn prompt shows up.
            seen = ""
            for _ in range(40):
                seen += run_until(fd, 0.4).decode("utf-8", "replace")
                if self.CLASS_PROMPT in seen:
                    break
                os.write(fd, b"n")
            self.assertIn(self.CLASS_PROMPT, seen,
                          "never reached the class prompt")

            os.write(fd, b"\x04")   # Ctrl+D at a ReadLn
            run_until(fd, 1.0)
            os.write(fd, b"\x04")
            run_until(fd, 1.0)

            done, status = os.waitpid(pid, os.WNOHANG)
            self.assertEqual(
                (done, status), (0, 0),
                "Ctrl+D ended the process; the original cannot be quit "
                "this way")

            # Still listening: a real answer must still be accepted.
            os.write(fd, b"0\n")
            after = seen + run_until(fd, 2.0).decode("utf-8", "replace")
            self.assertIn(
                "зовут тебя", after,
                "the game stopped accepting input after Ctrl+D")
        finally:
            try:
                os.kill(pid, 9)
                os.waitpid(pid, 0)
            except (ProcessLookupError, ChildProcessError):
                pass
            os.close(fd)


if __name__ == "__main__":
    sys.exit(0 if unittest.main(exit=False).result.wasSuccessful() else 1)
