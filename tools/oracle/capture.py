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
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

import addr  # noqa: E402  -- the address convention, defined once

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
ORIG = ROOT / "orig"
HERE = pathlib.Path(__file__).resolve().parent
CONF = HERE / "dosbox-oracle.conf"
SCRHOOK = HERE / "scrhook.com"

COLUMNS = 80
ROWS = 25
FRAME_BYTES = COLUMNS * ROWS * 2  # character byte + attribute byte per cell
MAX_KEYS = 1024  # KEYBUF_SIZE in scrhook.asm; the TSR reads no more than this

# --- guest memory window ---------------------------------------------------
# Mirrors STATE_BASE/STATE_SIZE in scrhook.asm. Each STATE.BIN record is the
# interrupted program's DS as a little-endian word, then STATE_SIZE bytes read
# from DS:STATE_BASE.
STATE_BASE = 0x3600
STATE_SIZE = 2048
STATE_RECORD = 2 + STATE_SIZE

# --- seed pinning ----------------------------------------------------------
# g.exe seeds RandSeed from the DOS clock (System.Randomize at 1f78:11e0, the
# only INT 21h/AH=2Ch in the binary and reached from exactly one call site,
# 1000:6a5d), so two runs of the same script diverge. Overwriting Randomize's
# body in the *scratch copy* with a constant store makes a run reproducible.
# orig/g.exe is never touched, and a pinned binary is never committed: pass
# seed=None -- the default -- and the game seeds itself from the clock exactly
# as it ships. See docs/re/combat.md.
# tools/addr.py owns the citation arithmetic; 0f78:11e0 is a real runtime
# seg:off (Form B), so it goes through image_off_of_seg_off, not the Ghidra
# form.  See docs/re/METHODOLOGY.md.
RANDOMIZE_FILE_OFF = addr.file_off_of_citation("0f78:11e0")  # 0x12230
RANDOMIZE_ORIGINAL = bytes.fromhex("b42ccd21890e7e3689168036cb")


def pin_seed_patch(seed: int) -> bytes:
    r"""The 13 bytes that replace System.Randomize's body to pin `seed`.

    Same length as the original body, so nothing else in the image moves:

        C7 06 7E 36 lo lo   mov word [0x367e], seed_lo   ; RandSeed.lo
        C7 06 80 36 hi hi   mov word [0x3680], seed_hi   ; RandSeed.hi
        CB                  retf

    The addressing is DS-relative, exactly as the instructions it replaces
    (`mov [0x367e],cx` / `mov [0x3680],dx`), so it stores through the same
    segment the original did.
    """
    if not 0 <= seed <= 0xFFFFFFFF:
        raise ValueError(f"seed {seed} does not fit in 32 bits")
    lo = seed & 0xFFFF
    hi = (seed >> 16) & 0xFFFF
    return (
        b"\xc7\x06\x7e\x36" + lo.to_bytes(2, "little")
        + b"\xc7\x06\x80\x36" + hi.to_bytes(2, "little")
        + b"\xcb"
    )


def pin_seed(exe: pathlib.Path, seed: int) -> None:
    """Apply pin_seed_patch to `exe` in place. Refuses anything but g.exe.

    To undo: delete the patched copy. There is nothing else to reverse --
    the patch is only ever applied to the scratch copy _prepare() makes, and
    _prepare() rebuilds that copy from orig/ on every run.
    """
    blob = bytearray(exe.read_bytes())
    found = bytes(blob[RANDOMIZE_FILE_OFF : RANDOMIZE_FILE_OFF + len(RANDOMIZE_ORIGINAL)])
    if found != RANDOMIZE_ORIGINAL:
        raise OracleError(
            f"{exe} does not have System.Randomize's body at "
            f"{RANDOMIZE_FILE_OFF:#x} (found {found.hex()}); refusing to patch"
        )
    patch = pin_seed_patch(seed)
    blob[RANDOMIZE_FILE_OFF : RANDOMIZE_FILE_OFF + len(patch)] = patch
    exe.write_bytes(bytes(blob))

# Keys that walk a fresh game from the title screen to the command prompt:
# any key past the logo, district "1", seven any-key story pages, class "0"
# and Enter, then Enter to accept the default name. Recovered by reading the
# captured frames back, one key request at a time; see docs/re/oracle.md.
INTRO_KEYS = "\n1\n\n\n\n\n\n\n0\n\n"

# Structured form of the same finding: which prompt each segment of
# INTRO_KEYS answers, recovered the same way (reading captured frames back
# one key request at a time). This is the source of truth for
# data/oracle_prompts.json -- see write_re_findings() below -- and for the
# table in docs/re/oracle.md; keep all three in sync if the intro script
# ever changes.
INTRO_KEY_PROMPTS = [
    {"keys": r"\n", "prompt": 'title screen, "Нажми какую-нибудь кнопку"'},
    {
        "keys": "1",
        "prompt": (
            '"Нажми цифру с какого района начать" -- district, 1 starts '
            "from scratch"
        ),
    },
    {
        "keys": r"\n" + " x 7",
        "prompt": 'seven any-key story pages, ending at "Выбери кем ты будешь"',
    },
    {
        "keys": r"0\n",
        "prompt": "class prompt (line input): 0-Пацан, 1-Отморозок, 2-Гопник, 3-Вор",
    },
    {
        "keys": r"\n",
        "prompt": '"А зовут тебя:" (line input) -- empty accepts the default Раздолбай',
    },
]

# The game's command vocabulary at its `\` prompt (reached after
# INTRO_KEYS), as listed by the in-game `i` command and cross-checked by
# running each one through the oracle. A value of None means the command
# was observed to exist but its semantics were not established -- left
# explicitly unknown rather than guessed.
COMMANDS = {
    "i": "lists commands",
    "w": "wander",
    "k": "fight",
    "mar": "market",
    "rep": "vet",
    "girl": None,
    "kl": "club",
    "s": "look at yourself",
    "sv": "inspect the enemy",
    "v": "reinforcements",
    "kos": None,
    "h": None,
    "mh": None,
    "name": None,
    "e": "quit",
}


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


def decode_states(blob: bytes) -> list[tuple[int, bytes]]:
    """Split a STATE.BIN into one (ds, window) pair per captured frame.

    `window` is STATE_SIZE bytes of the interrupted program's data segment
    starting at STATE_BASE, so window[off - STATE_BASE] is the byte the guest
    had at DS:off.
    """
    if len(blob) % STATE_RECORD:
        raise OracleError(
            f"STATE.BIN is {len(blob)} bytes, not a whole number of "
            f"{STATE_RECORD}-byte records"
        )
    out = []
    for start in range(0, len(blob), STATE_RECORD):
        rec = blob[start : start + STATE_RECORD]
        out.append((int.from_bytes(rec[:2], "little"), rec[2:]))
    return out


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


def write_re_findings(path: pathlib.Path = ROOT / "data" / "oracle_prompts.json") -> None:
    """Emit the machine-readable form of INTRO_KEY_PROMPTS and COMMANDS.

    capture.py is the single source for this RE finding -- data/*.json is a
    generated artifact, not hand-maintained. Re-run this (e.g. `python3 -c
    "import capture; capture.write_re_findings()"` from tools/oracle/) after
    editing INTRO_KEY_PROMPTS or COMMANDS above, and commit the result.
    """
    payload = {
        "note": (
            "Which prompt each segment of capture.INTRO_KEYS answers, and the "
            "game's \\-prompt command vocabulary. Recovered by driving "
            "orig/g.exe through tools/oracle/capture.py and reading the "
            "captured frames back one key request at a time. Commands whose "
            "semantics were not established are null, not guessed. Generated "
            "by capture.write_re_findings() from INTRO_KEY_PROMPTS/COMMANDS "
            "in tools/oracle/capture.py -- see docs/re/oracle.md."
        ),
        "intro_keys": INTRO_KEYS,
        "intro_key_prompts": INTRO_KEY_PROMPTS,
        "commands": COMMANDS,
    }
    path.write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )


def _prepare(out_dir: pathlib.Path, keys: str, seed: int | None = None) -> pathlib.Path:
    """Build a scratch copy of the game to run. orig/ is never mounted.

    With `seed` set, the scratch g.exe -- never orig/g.exe -- gets its
    System.Randomize body replaced by a constant store, so the run is
    reproducible. Leave it None for the shipped clock-seeded behaviour.
    """
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
    if seed is not None:
        pin_seed(work / "g.exe", seed)
    return work


def _wait(proc: subprocess.Popen, screen: pathlib.Path, deadline: float, settle: float):
    """Wait for the emulator to finish, or for the capture to go quiet.

    A script that does not end in the game's own `e` quit command leaves the
    game sitting at a prompt for a key that never comes, so the run is over
    once SCREEN.BIN has stopped growing -- the last frame is already
    committed to disk by then. Hitting the deadline is a failure, never a
    silent partial capture.

    Teardown is SIGKILL, not SIGTERM. scrhook.com commits every frame to disk
    as it writes it (INT 21h/AH=68h), so there is nothing buffered for a
    graceful shutdown to flush and nothing to lose. A SIGTERM'd dosbox-x, on
    the other hand, runs its own shutdown path -- which can put up a modal
    "are you sure you want to quit" dialog while a program is still running,
    and then sit on it. A killed process cannot show a dialog and cannot
    hang, which is what an unattended harness needs.
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
            proc.kill()
            proc.wait()
            return
        time.sleep(0.1)


def _check_frame_count(
    frames: list[str], expect_frames: int | None, log: pathlib.Path
) -> None:
    """Raise OracleError if `frames` does not have exactly `expect_frames` entries.

    Split out of run() so this guard can be exercised directly against an
    already-captured frame list in tests, without launching the emulator
    again. See run()'s docstring for why the check exists.
    """
    if expect_frames is not None and len(frames) != expect_frames:
        raise OracleError(
            f"captured {len(frames)} frames, expected {expect_frames}: the run "
            "was cut short (or the script changed). A short capture is not a "
            f"usable oracle result; see {log} for the emulator log"
        )


def run(
    keys: str,
    out_dir: pathlib.Path,
    timeout: int = 120,
    settle: float = 3.0,
    expect_frames: int | None = None,
    seed: int | None = None,
) -> list[str]:
    """Run the original with `keys` and return one screen of text per frame.

    Writes into out_dir: work/ (the scratch game directory, including the raw
    work/SCREEN.BIN), screens.txt (every captured frame) and screen.txt (the
    last one). Raises OracleError if no usable capture came back.

    `expect_frames` pins how many frames the script must produce. Pass it for
    anything an oracle result depends on. The guest side of this harness is
    inherently deterministic -- frames are pinned to key requests and the Nth
    request always gets the Nth scripted key -- but the host side stops the
    run once SCREEN.BIN has been quiet for `settle` seconds, and that is a
    wall-clock judgement. If the emulator ever stalls longer than that
    mid-script, the capture ends early and every later frame is missing.
    Without this check a short capture looks exactly like a complete one: the
    caller just gets a shorter list. A frame count is cheap and turns that
    silent truncation into a failure.
    """
    out_dir = pathlib.Path(out_dir)
    work = _prepare(out_dir, keys, seed=seed)
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
    _check_frame_count(frames, expect_frames, log)
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
    ap.add_argument(
        "--expect-frames",
        type=int,
        help="fail unless the run captures exactly this many frames",
    )
    ap.add_argument(
        "--seed",
        type=lambda v: int(v, 0),
        help=(
            "pin RandSeed in the scratch copy of g.exe instead of letting it "
            "seed from the DOS clock; omit for the shipped behaviour"
        ),
    )
    args = ap.parse_args()

    try:
        frames = run(
            unescape(args.keys),
            args.out,
            timeout=args.timeout,
            expect_frames=args.expect_frames,
            seed=args.seed,
        )
    except (OracleError, ValueError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    print(f"{len(frames)} frames -> {args.out / 'screens.txt'}", file=sys.stderr)
    print(frames[-1])
    return 0


if __name__ == "__main__":
    sys.exit(main())
