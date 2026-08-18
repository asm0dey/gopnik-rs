# qemu + gdb guest debugger (PROTOTYPE — not yet reviewed)

Runs `orig/g.exe` under FreeDOS in qemu with gdb attached to the *guest*, so
breakpoints can be set on the game's own 16-bit code and its `Random` draws
observed at runtime. More capable than the DOSBox-X oracle: it needs no
guest-side TSR, and it can prove a negative ("this verb reaches no branch"),
which static reading can only suggest.

**Status: prototype.** These scripts proved the approach end to end and are
committed so the working invocation is not lost with the session scratchpad.
They are not a reviewed harness — `tools/oracle/capture.py` remains the
sanctioned capture path until a proper task replaces this.

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

It appears twice, at file offsets `0x7d59` and `0xc3c1` (delta `0x4668`); a
matching delta in RAM confirms it is the image. Then:

    linear(1000:XXXX) = image_base + XXXX

Testing saw base `0x224B0`, cross-checked against a live `CS=0x224e`. Do NOT
hardcode it — DOS picks the load segment.

## Pitfalls already paid for

- qemu 11 wants `-monitor unix:PATH,server=on,wait=off` (not `server,nowait`).
- Unix socket paths cap at 108 bytes; a session scratchpad path is too long.
- vvfat must be `-hda fat:rw:<dir>`; read-only fails with "Block node is
  read-only". Mount a COPY of the game, never `orig/`.
- `boot.start()` launches a VM — kill it when done or it lingers.
