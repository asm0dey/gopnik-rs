# The DOSBox-X oracle (Task 3)

`tools/oracle/` turns `orig/g.exe` into a queryable ground-truth source: give
it a keystroke script, get back the screens the original printed, decoded
from CP866 to UTF-8. Tasks 8 (RNG), 9 (combat) and 12 (differential test)
validate against it.

```sh
tools/oracle/run_oracle.sh '\n1\n\n\n\n\n\n\n0\n\ne\n\n' /tmp/run1
python3 tools/oracle/test_oracle_smoke.py
```

`run_oracle.sh` prints the last captured screen on stdout. In `<out_dir>` it
leaves `screens.txt` (every frame), `screen.txt` (the last one),
`dosbox.log`, and `work/` — the scratch copy of the game that was actually
run, including the raw capture `work/SCREEN.BIN`. From Python:

```python
import capture
frames = capture.run(capture.INTRO_KEYS + "e\n\n", pathlib.Path("/tmp/run1"))
```

## Why the screen has to be read by guest code

The original writes through Borland Pascal's Crt unit straight into VGA text
memory. Redirecting inside the guest proves it: `g.exe > OUT.TXT` leaves
OUT.TXT at **0 bytes** while the game happily draws its title screen. So
there is no stdout to capture, and `-c` commands are no help either — they
run only when the shell is idle, never while the game holds the console.
DOSBox-X's own screenshot facility saves a PNG of rendered pixels, not text.

What is left is to have guest code read the text buffer at `B800:0000` and
write it to a file on the mounted host directory. `tools/oracle/scrhook.asm`
(assembled to the committed `scrhook.com`, 1298 bytes) is that code:

- It hooks INT 16h and, on every **blocking** key read (AH=00h/10h), appends
  the whole 80x25 buffer — 4000 bytes, characters *and* attributes — to
  `SCREEN.BIN`, then commits the file (INT 21h AH=68h) so the host sees the
  frame immediately.
- It then answers that read with the next byte of `KEYS.TXT` instead of a
  real keystroke. When the script runs out it chains to the BIOS, so the game
  simply waits and the harness tears the emulator down.
- It checks the DOS InDOS flag before doing anything (COMMAND.COM's line
  input reaches INT 16h from *inside* INT 21h, and re-entering DOS there
  would corrupt it), and it makes its own PSP current around the write,
  because DOS file handles are indexed per PSP — without that the write goes
  to whatever handle number the *game* happens to have open, and the capture
  file stays empty.

Reassemble with `nasm -f bin tools/oracle/scrhook.asm -o
tools/oracle/scrhook.com`; the smoke test checks the committed binary against
the source whenever nasm is installed.

Key injection is what makes the oracle deterministic. DOSBox-X's `AUTOTYPE`
and `ADDKEY` deliver keys on wall-clock timers into a 15-key BIOS buffer, so
scripts are length-limited (a DOS command line is 127 characters) and their
delivery races the emulator. Serving keys from the interrupt handler instead
means the Nth key request always gets the Nth scripted key, at any emulator
speed, for scripts up to 1024 keystrokes.

## Frame semantics

One frame per key the game asks for, captured **before** that key is served —
so frame *n* is the screen the game was showing when it asked for key *n*.
The screen after the last key is captured when the game next asks for input,
which is why a script normally ends with one extra keystroke.

Only what is on screen is captured: text that has scrolled off is gone, and
anything the game draws and overwrites between two input requests is never
seen. `SCREEN.BIN` keeps the attribute byte of every cell, so colour indices
stay recoverable; `capture.py` decodes the character plane only.

Two limits follow from how the hook works. A key request made while DOS is
busy is neither captured nor answered — the InDOS guard steps aside and the
real BIOS handles it — so a code path that reads input through DOS instead of
Crt would stall; nothing in the original has been seen to do that. And a
script is at most 1024 keystrokes, the resident buffer's size; `capture.py`
refuses a longer one rather than let the tail be dropped silently.

## Headless operation

`SDL_VIDEODRIVER=dummy` with `-nogui`. `xvfb-run`/`Xvfb` are **not installed**
on this host and are not needed. DOSBox-X 2026.08.02 (banner says SDL1, links
libSDL3) boots, creates a 720x400 surface and runs `[autoexec]` normally.

The config deliberately carries no `keyboardlayout=` line: this host ships no
keyboard layout files, so `keyboardlayout=ru446` only logs "Keyboard layout
file ru446 not found", and the capture path sits below both the layout and
the display codepage (raw CP866 bytes out of the text buffer, CP866 bytes in
as keystrokes). Two unrelated notes remain in `dosbox.log` — a missing MT32
ROM and "No translation support (to host) for code page 0"; neither touches
the capture.

## Timing

Because keys are served the instant the game asks, a run is not paced by
anything: the full intro-to-quit script above takes **0.9 s** wall clock.
A script that leaves the game sitting at a prompt costs the 3-second
quiescence window plus up to 2 seconds of emulator shutdown (dosbox-x can sit
on a SIGTERM). `capture.py` fails with `OracleError` rather than hanging:
`-time-limit` bounds the emulator and a host-side deadline bounds the wait.

## Determinism

Three consecutive runs of the intro script, and two more including a `w`
(wander) command whose outcome is drawn by the game's RNG, produced
byte-identical `SCREEN.BIN` files (`md5 4447e10a…` and `1c9a769c…`
respectively). The smoke test re-checks this on every run by capturing the
same script twice and comparing the raw bytes. Nothing has yet been observed
to vary between runs — noteworthy because a `Randomize` seeded from the
timer would have shown up here immediately.

## Key scripts

`--keys` takes text with backslash escapes: `\n` (and `\r`) is Enter, `\t`
Tab, `\xNN` a literal byte, `\\` a backslash. Characters are encoded to
**CP866**, so a scripted Cyrillic name arrives exactly as the original's
keyboard would deliver it. A `00h` byte is the BIOS extended-key escape and
the byte after it is a scan code, e.g. `\x00\x50` for Down.

`capture.INTRO_KEYS` walks a fresh game from the title screen to the command
prompt. The prompts it answers, recovered by reading captured frames back one
key request at a time:

| key(s) | consumed by |
| --- | --- |
| `\n` | title screen, "Нажми какую-нибудь кнопку" |
| `1` | "Нажми цифру с какого района начать" — district, `1` starts from scratch |
| `\n` × 7 | seven any-key story pages, ending at "Выбери кем ты будешь" |
| `0` `\n` | class prompt (line input): 0-Пацан, 1-Отморозок, 2-Гопник, 3-Вор |
| `\n` | "А зовут тебя:" (line input) — empty accepts the default Раздолбай |

After that the game is at its `\` command prompt, where `i` lists commands
(`w` wander, `k` fight, `mar` market, `rep` vet, `girl`, `kl` club, `s` look
at yourself, `sv` inspect the enemy, `v` reinforcements, `kos`, `h`, `mh`,
`name`, `e` quit). `e` quits, prints the result sheet and waits for one more
key before returning to DOS.

The machine-readable form of this table and command list is
`data/oracle_prompts.json`. `tools/oracle/capture.py`'s `INTRO_KEY_PROMPTS`
and `COMMANDS` constants are the source of truth; the JSON is generated from
them by `capture.write_re_findings()`, not hand-maintained. Commands whose
semantics were not established (`girl`, `kos`, `h`, `mh`, `name`) are `null`
in the JSON rather than guessed.

The table above is prose copied by hand from those constants and is not
checked by anything; `test_oracle_prompts_json_matches_source` in
`tools/oracle/test_oracle_smoke.py` only checks the JSON against
`capture.py`. If this table and `data/oracle_prompts.json` ever disagree,
`data/oracle_prompts.json` is authoritative.

## The corpus stays read-only

Every run copies `orig/` into `<out_dir>/work/` and mounts *that*; `orig/` is
never mounted, so no `.SAV` the game writes can land in it. `HOME` is
redirected into the work directory as well, so DOSBox-X leaves nothing in the
real home. The smoke test hashes every file in `orig/` before and after a run
and pins `g.exe` to md5 `10eb0af07a2d2f5e9da790df7058891c`.
