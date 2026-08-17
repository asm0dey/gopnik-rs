#!/usr/bin/env python3
"""Drive orig/g.exe under DOSBox-X and capture what it printed.

The original is a Borland Pascal program that writes through the Crt unit
straight into VGA text memory: `g.exe > OUT.TXT` inside the guest produces a
zero-byte file, so there is no stdout to capture and DOSBox-X's -c commands
only run when the shell is idle, never while the game holds the console.
The screen therefore has to be read out of the guest by guest code.

`scrhook.com` (source: scrhook.asm) does that. It hooks INT 16h and on every
blocking key read it appends the 80x25 text buffer at B800:0000 to
SCREEN.BIN and then answers the read with the next byte of KEYS.TXT instead
of a real keystroke. Two properties follow, and both matter for an oracle:

- One frame per key the game consumes, taken at the moment the game asks --
  so frames are pinned to game state, not to a wall-clock sampling interval.
- The Nth key request always gets the Nth scripted key. Nothing depends on
  autotype pacing, on the emulator's speed, or on the 15-key BIOS keyboard
  buffer, and a script can be any length. Repeated runs of the same script
  reproduce SCREEN.BIN byte for byte (see docs/re/oracle.md).

Text is CP866 and is decoded to UTF-8 exactly once, here, matching
tools/extract_strings.py. Attributes are kept in the raw SCREEN.BIN so
colour indices remain recoverable.

Usage:
    capture.py --keys "\\n1\\n\\n\\n\\n\\n\\n\\n0\\n\\ne\\n\\n" --out run1/
"""
import argparse
import os
import pathlib
import shutil
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
ORIG = ROOT / "orig"
HERE = pathlib.Path(__file__).resolve().parent
CONF = HERE / "dosbox-oracle.conf"
SCRHOOK = HERE / "scrhook.com"

COLUMNS = 80
ROWS = 25
FRAME_BYTES = COLUMNS * ROWS * 2  # character byte + attribute byte per cell
MAX_KEYS = 1024  # KEYBUF_SIZE in scrhook.asm; the TSR reads no more than this

# Keys that walk a fresh game from the title screen to the command prompt:
# any key past the logo, district "1", seven any-key story pages, class "0"
# and Enter, then Enter to accept the default name. Recovered by reading the
# captured frames back, one key request at a time; see docs/re/oracle.md.
INTRO_KEYS = "\n1\n\n\n\n\n\n\n0\n\n"


class OracleError(RuntimeError):
    """The emulator did not produce a usable capture."""


def unescape(spec: str) -> str:
    r"""Decode the backslash escapes accepted on the command line.

    \n and \r both mean Enter, \t is Tab, \xNN is a literal byte value and
    \\ is a backslash. Anything else after a backslash is an error rather
    than a silently passed-through character.
    """
    out = []
    i = 0
    while i < len(spec):
        ch = spec[i]
        if ch != "\\":
            out.append(ch)
            i += 1
            continue
        if i + 1 >= len(spec):
            raise ValueError("key script ends with a lone backslash")
        code = spec[i + 1]
        i += 2
        if code == "x":
            if i + 2 > len(spec):
                raise ValueError(r"\x needs two hex digits")
            out.append(chr(int(spec[i : i + 2], 16)))
            i += 2
        elif code in "nr":
            out.append("\r")
        elif code == "t":
            out.append("\t")
        elif code == "\\":
            out.append("\\")
        else:
            raise ValueError(rf"unknown escape \{code} in key script")
    return "".join(out)


def encode_keys(keys: str) -> bytes:
    """Turn a key script into the bytes scrhook.com hands back to the game.

    One byte per keystroke, in the game's own encoding: CP866, so a scripted
    Cyrillic name arrives exactly as the original's keyboard would deliver
    it. Newline means Enter, which the BIOS reports as CR. A 00h byte is the
    BIOS extended-key escape and the byte after it is the scan code, e.g.
    "\\x00\\x50" for Down.
    """
    return keys.replace("\n", "\r").encode("cp866")


def decode_frames(blob: bytes) -> list[str]:
    """Split a SCREEN.BIN into one UTF-8 screen of text per captured frame.

    Only the character plane is decoded; the attribute byte of each cell is
    dropped here and stays available in the raw capture.
    """
    if not blob:
        raise OracleError("SCREEN.BIN is empty: the game never asked for a key")
    if len(blob) % FRAME_BYTES:
        raise OracleError(
            f"SCREEN.BIN is {len(blob)} bytes, not a whole number of "
            f"{FRAME_BYTES}-byte frames"
        )
    frames = []
    for start in range(0, len(blob), FRAME_BYTES):
        frame = blob[start : start + FRAME_BYTES]
        lines = [
            frame[r * COLUMNS * 2 : (r + 1) * COLUMNS * 2 : 2].decode("cp866").rstrip()
            for r in range(ROWS)
        ]
        frames.append("\n".join(lines))
    return frames


def _prepare(out_dir: pathlib.Path, keys: str) -> pathlib.Path:
    """Build a scratch copy of the game to run. orig/ is never mounted."""
    work = out_dir / "work"
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)
    for path in sorted(ORIG.iterdir()):
        target = work / path.name
        shutil.copy2(path, target)
        target.chmod(0o644)
    shutil.copy2(SCRHOOK, work / "SCRHOOK.COM")
    script = encode_keys(keys)
    if len(script) > MAX_KEYS:
        # scrhook.com would read the first MAX_KEYS bytes and quietly drop
        # the rest, which is exactly the kind of silent truncation an oracle
        # must never do.
        raise OracleError(
            f"key script is {len(script)} bytes, more than the {MAX_KEYS} "
            "scrhook.com can hold"
        )
    (work / "KEYS.TXT").write_bytes(script)
    return work


def _wait(proc: subprocess.Popen, screen: pathlib.Path, deadline: float, settle: float):
    """Wait for the emulator to finish, or for the capture to go quiet.

    A script that does not end in the game's own `e` quit command leaves the
    game sitting at a prompt for a key that never comes, so the run is over
    once SCREEN.BIN has stopped growing -- the last frame is already
    committed to disk by then. Hitting the deadline is a failure, never a
    silent partial capture.
    """
    size = 0
    changed = time.monotonic()
    while proc.poll() is None:
        now = time.monotonic()
        if now > deadline:
            proc.kill()
            proc.wait()
            raise OracleError(
                f"dosbox-x did not finish within the time budget; "
                f"{size // FRAME_BYTES} frames captured before the deadline"
            )
        current = screen.stat().st_size if screen.exists() else 0
        if current != size:
            size, changed = current, now
        elif size and now - changed >= settle:
            # Every frame is already committed to disk, so there is nothing
            # to lose by being brisk here; dosbox-x can sit on a SIGTERM for
            # ten seconds, which would otherwise dominate the run time.
            proc.terminate()
            try:
                proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
            return
        time.sleep(0.1)


def run(
    keys: str,
    out_dir: pathlib.Path,
    timeout: int = 120,
    settle: float = 3.0,
) -> list[str]:
    """Run the original with `keys` and return one screen of text per frame.

    Writes into out_dir: work/ (the scratch game directory, including the raw
    work/SCREEN.BIN), screens.txt (every captured frame) and screen.txt (the
    last one). Raises OracleError if no usable capture came back.
    """
    out_dir = pathlib.Path(out_dir)
    work = _prepare(out_dir, keys)
    screen = work / "SCREEN.BIN"
    log = out_dir / "dosbox.log"

    cmd = [
        "dosbox-x",
        "-conf", str(CONF),
        "-nogui",
        "-fastlaunch",
        "-exit",
        "-time-limit", str(timeout),
        "-c", "scrhook.com",
        "-c", "g.exe",
    ]
    env = os.environ.copy()
    env["SDL_VIDEODRIVER"] = "dummy"  # headless: no X, no Xvfb on this host
    env["HOME"] = str(work)  # keep any dosbox-x state out of the real home

    with log.open("wb") as sink:
        proc = subprocess.Popen(cmd, cwd=work, env=env, stdout=sink, stderr=sink)
        try:
            _wait(proc, screen, time.monotonic() + timeout + 5, settle)
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()

    if not screen.exists():
        raise OracleError(
            f"scrhook.com produced no SCREEN.BIN; see {log} for the emulator log"
        )
    frames = decode_frames(screen.read_bytes())
    (out_dir / "screens.txt").write_text(
        "".join(f"=== frame {i} ===\n{f}\n" for i, f in enumerate(frames)),
        encoding="utf-8",
    )
    (out_dir / "screen.txt").write_text(frames[-1] + "\n", encoding="utf-8")
    return frames


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "--keys",
        required=True,
        help=r"keystroke script; \n is Enter, \xNN a literal byte value",
    )
    ap.add_argument("--out", required=True, type=pathlib.Path)
    ap.add_argument("--timeout", type=int, default=120)
    args = ap.parse_args()

    frames = run(unescape(args.keys), args.out)
    print(f"{len(frames)} frames -> {args.out / 'screens.txt'}", file=sys.stderr)
    print(frames[-1])
    return 0


if __name__ == "__main__":
    sys.exit(main())
