# qemu + gdb guest debugger (SUPERSEDED prototype — use `tools/rngtrace`)

> **Superseded by `tools/rngtrace` (Task 11d).** The productionised harness
> lives there: one non-interactive command boots the guest, derives the load
> base, verifies the code at the breakpoint address, pins `RandSeed` in a
> patched COPY of the binary, logs every `Random` draw, and fails loudly rather
> than emitting a short trace. Run it with
> `python3 tools/rngtrace/run.py --boot-img <freedos.img> --walks N`, and see
> `docs/re/rng-trace.md`. `tools/oracle/capture.py` remains the sanctioned
> DOSBox-X screen-capture path; the two are separate.
>
> The files here are kept only as the record of how the approach was proved.
> **Do not extend them** — three things they get wrong are fixed in
> `tools/rngtrace` and documented below under "What the prototype got wrong".

Runs `orig/g.exe` under FreeDOS in qemu with gdb attached to the *guest*, so
breakpoints can be set on the game's own 16-bit code and its `Random` draws
observed at runtime. More capable than the DOSBox-X oracle: it needs no
guest-side TSR, and it can prove a negative ("this verb reaches no branch"),
which static reading can only suggest.

## Proven

- FreeDOS 1.3 boots and runs the game headless.
- The 80x25 screen reads straight out of guest RAM (`xp /4000xb 0xb8000`,
  even bytes are the characters, decode cp866).
- Keys inject via the qemu monitor's `sendkey`.
- gdb 17.2 attaches in real mode (`set architecture i8086`) and **breakpoints
  on guest code fire**: `break *0x2d313` hit the game's main ReadLn, and gdb
  reported IP as `0xae63` — i.e. the Ghidra offset, so results read back
  directly against `docs/re/`.

## Load-base translation (must not be skipped)

Our addresses are Ghidra IMAGE addresses. Derive the base each run by finding
the version banner in guest RAM:

    find /b 0x1000, 0xfffff, 0x5e,0x34,0x47,0x6f,0x70,...    ("^4Gopnik: ^7version")

It appears twice, at FILE offsets `0x7d59` and `0xc3c1` (delta `0x4668`); a
matching delta in RAM confirms it is the image. **Convert to an image offset
before subtracting** — the load image starts at file `0x18d0`, which is Ghidra
`1000:0000`:

    image_off  = file_off - 0x18d0                  (0x7d59 -> 0x6489)
    image_base = found_linear - image_off
    linear(1000:XXXX) = image_base + XXXX

Skipping the `- 0x18d0` puts the base out by exactly that much and every
breakpoint lands on unrelated memory. The header size is not a magic number:
`tools/addr.py` derives it from the MZ header and exposes
`image_off_of_file_off()` / `file_off_of_image_off()`, which `tools/rngtrace`
uses. `docs/re/METHODOLOGY.md` is the authority for the rule; the worked
numbers above are kept because deriving the base is what this file is for.

Testing saw base `0x224B0`, cross-checked against a live `CS=0x224e`. Do NOT
hardcode it — DOS picks the load segment.

## Pitfalls already paid for

- qemu 11 wants `-monitor unix:PATH,server=on,wait=off` (not `server,nowait`).
- Unix socket paths cap at 108 bytes; a session scratchpad path is too long.
- vvfat must be `-hda fat:rw:<dir>`; read-only fails with "Block node is
  read-only". Mount a COPY of the game, never `orig/`.
- `boot.start()` launches a VM — kill it when done or it lingers.

## gdb techniques worth using here

Adapted from the gdb skill at
https://github.com/mohitmishra786/low-level-dev-skills (C/C++ userspace
oriented; the parts below are the ones that survive the jump to a symbol-less
16-bit guest over a remote stub).

**Auto-logging breakpoints — the core of the Random tracer.** The stack layout
below is right and `tools/rngtrace` uses it; the `commands` sketch is **wrong**
and is kept only because the reason it fails is worth knowing (see "What the
prototype got wrong"). The working form is in
`tools/rngtrace/gdbsession.py`.

At a Borland far-call entry the stack holds `[sp]`=return offset,
`[sp+2]`=return segment, `[sp+4]`=the pushed argument. The return offset IS the
Ghidra call-site offset, so the log reads directly against `docs/re/`. The same
frame is still intact at the `retf 2`, where `ax` additionally holds the
result, so `tools/rngtrace` breaks there instead and gets site, `n` and result
from one stop.

**Conditional breakpoints** narrow a hot routine to one caller without stopping
on every draw:

    break *RANDOM_ADDR if *(unsigned short*)($ss*16+$sp) == 0xb725

**`tbreak`** for one-shot stops (e.g. catch the first entry into a routine, then
get out of the way).

**UNVERIFIED, worth one experiment:** gdb reverse debugging
(`target record-btrace` / `record full`, then `reverse-continue`) would answer
"which branch led here" directly. Whether it works over qemu's stub in real
mode is untested — treat as speculative until someone runs it, and do not cite
it in a finding.

**Not applicable:** `rbreak` and anything symbol-driven (no symbols),
`break foo if x > 10` by variable name (no debug info), and the skill's
crash/core-dump material (a DOS guest produces none).

## What the prototype got wrong

All three were found by running it, and all three are fixed in
`tools/rngtrace`. Each fails in the same direction — a plausible SHORT trace,
which reads as evidence that draws did not happen.

1. **Breakpoint `commands` never run.** qemu's i386 gdbstub reports `$pc` as
   the raw 16-bit `eip`, while the breakpoint sits at the LINEAR address
   (`cs_base + eip`). gdb cannot match the stop to its own breakpoint, reports
   a bare `SIGTRAP`, and skips the `commands` block. Observed: a stop inside
   `Random` printed `0x0000114b in ?? ()` against a breakpoint recorded at
   `0x32d7b`. The tracer must dispatch on `$pc` itself, from an explicit gdb
   `while` loop.
2. **Resuming re-traps forever.** For the same reason gdb never does its
   remove/single-step/reinsert dance, so qemu stops again on the very same
   instruction. Measured: 833654 bytes of stops, every one at `$pc = ae63`,
   with the guest making no progress at all — while its screen stayed put, so
   a screen-driven driver went on typing into a frozen game. `hbreak` behaves
   identically. The loop must step over by hand: `disable`, `stepi`, `enable`.
3. **The VM leaks and the log is lost.** `boot.start()` leaves qemu running on
   any exception, and gdb, busy in its script loop, never reads `quit` from
   stdin. `tools/rngtrace` kills the VM on every exit path, and kills it
   *first* — the failing `continue` drops gdb to a prompt where `quit` is read
   and its output is flushed.

`tools/rngtrace` additionally derives the load base by verifying all 1580 MZ
relocations against the candidate segment (not by one banner match), checks the
bytes at the breakpoint address against the file before installing it, and
replays the whole draw stream against the pinned seed — a missed draw
desynchronises the LCG and fails the run.
