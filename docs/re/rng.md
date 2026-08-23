# RNG (Task 8)

**Outcome: fallback tier 1 — the generator was recovered statically.** No
emulator run, no oracle capture, no substitute PRNG. The Rust port in
`src/rng.rs` is bit-faithful to the original.

The starting assumption for this task was that the generator is custom,
because the Borland multiplier `$08088405` does not appear as a contiguous
little-endian dword anywhere in `orig/g.exe` and there is no
`mov ax,8405` / `mov dx,0808` pair. That is true, and it is a red herring:
the routine **is** the stock Borland Pascal `System` unit RNG, but the
compiler-supplied assembly never materialises the 32-bit constant. It
multiplies by the low word `$8405` (held as a literal *word* in the code
segment) and synthesises every partial product involving the high word
`$0808` out of shifts and byte-wise adds. Byte-searching for the dword can
never find it.

## Addresses

All three routines live in Ghidra memory block `CODE_5`, segment `1f78`
(the Borland `System` unit), so every citation in this section is a Ghidra
label. `docs/re/METHODOLOGY.md`, "Address convention, and its range of validity", is the authority for the rule; `tools/addr.py` is its executable form and `python3 tools/re_query.py resolve <citation>` checks any single address against the bytes.

| Ghidra address | Ghidra name | File offset | Pascal identity |
|---|---|---|---|
| `1f78:11a8` | `FUN_1f78_11a8` | `0x121f8` | `System.@Rand` — steps `RandSeed` |
| `1f78:114b` | `FUN_1f78_114b` | `0x1219b` | `System.Random(Range: Word): Word` |
| `1f78:1168` | (in `FUN_1f78_114b`'s block) | `0x121b8` | `System.Random: Real` (unused by the game) |
| `1f78:11e0` | `FUN_1f78_11e0` | `0x12230` | `System.Randomize` |
| `1f78:11de` | — | `0x1222e` | the literal word `$8405` (the multiplier's low half) |
| `20ae:367e` | — | `0x15a2e` | `RandSeed: LongInt`, four bytes, `$00000000` in the load image |

**Corrected in Task 9.** The five `1f78:*` file offsets in the table above
were each 0xC0 too high in the first revision of this document
(`0x122b8` for `@Rand`, and so on) -- `(0x1f78 - 0x1000) * 16` had been
worked out as `0xf840` instead of `0xf780`. The `20ae:367e` row was right,
because it was computed separately. The *formula* stated above the table
was always correct, and `tools/gen_rng_vectors.py` derives its offsets from
that formula at run time rather than from this table, so
`data/rng_vectors.json` was never affected. Task 9 needed
`System.Randomize`'s real offset in order to patch it for seed pinning
(`docs/re/combat.md`), which is how the slip surfaced; the corrected values
are what `orig/g.exe` actually contains, checked byte for byte against the
disassembly quoted below.

`RandSeed`'s two halves are addressed separately by the code: low word at
`DS:$367e`, high word at `DS:$3680`. `DS` is segment `20ae`.

## `@Rand` — the recurrence

Disassembly of `1f78:11a8` (54 bytes), verbatim:

```
000011A8  A17E36            mov ax,[0x367e]        ; ax = RandSeed.lo
000011AB  8B1E8036          mov bx,[0x3680]        ; bx = RandSeed.hi
000011AF  8BC8              mov cx,ax
000011B1  2EF726DE11        mul word [cs:0x11de]   ; dx:ax = lo * $8405
000011B6  D1E1              shl cx,1
000011B8  D1E1              shl cx,1
000011BA  D1E1              shl cx,1               ; cx = lo * 8
000011BC  02E9              add ch,cl              ; cx = lo * $0808  (mod 2^16)
000011BE  03D1              add dx,cx
000011C0  03D3              add dx,bx              ; += hi * $0001
000011C2  D1E3              shl bx,1
000011C4  D1E3              shl bx,1               ; bx = hi * 4
000011C6  03D3              add dx,bx              ; += hi * $0004
000011C8  02F3              add dh,bl              ; += hi * $0400
000011CA  B105              mov cl,0x5
000011CC  D3E3              shl bx,cl              ; bx = hi * 128
000011CE  02F3              add dh,bl              ; += hi * $8000
000011D0  050100            add ax,0x1
000011D3  83D200            adc dx,0x0             ; +1, 32-bit
000011D6  A37E36            mov [0x367e],ax
000011D9  89168036          mov [0x3680],dx        ; RandSeed := dx:ax
000011DD  C3                ret
000011DE  0584                                     ; <-- data: the word $8405
```

Reading the partial products:

* `mul word [cs:11de]` gives `lo * $8405` as a full 32-bit product in `dx:ax`.
* `cx` becomes `lo * 8`, then `add ch,cl` folds `(lo*8 and $ff) shl 8` back
  in, which is exactly `lo * $0808` modulo 2^16 — the contribution of the
  multiplier's high word to bits 16..31.
* The `bx` sequence accumulates `hi * ($0001 + $0004 + $0400 + $8000)` into
  `dx`, and `$0001 + $0004 + $0400 + $8000 = $8405`. That is `hi` times the
  multiplier's low word, which is all of `hi`'s contribution that survives
  mod 2^32.
* `add ax,1 / adc dx,0` adds the increment.

Total:

```
RandSeed := (RandSeed * $08088405 + 1) mod 2^32
```

with `$08088405 = 134775813` and increment `1`. It is a plain 32-bit LCG,
and it is the standard Borland Pascal 7.0 one after all — only the encoding
of the constant is unusual. `@Rand` returns the **new** state in `DX:AX`, so
the first value the program ever sees from a zero seed is `1`, not `0`.

## `Random(Range: Word): Word` — the range mapping

Disassembly of `1f78:114b` (29 bytes):

```
0000114B  E85A00            call 0x11a8            ; dx:ax = new RandSeed
0000114E  8BDC              mov bx,sp
00001150  8BCA              mov cx,dx              ; cx = seed.hi
00001152  36F76704          mul word [ss:bx+0x4]   ; dx:ax = seed.lo * n
00001156  8BC1              mov ax,cx              ; ax = seed.hi
00001158  8BCA              mov cx,dx              ; cx = (seed.lo * n) shr 16
0000115A  36F76704          mul word [ss:bx+0x4]   ; dx:ax = seed.hi * n
0000115E  03C1              add ax,cx
00001160  83D200            adc dx,0x0
00001163  8BC2              mov ax,dx              ; result = high word of the sum
00001165  CA0200            retf word 0x2
```

This is the classic widening-multiply-and-take-the-top mapping, **not** a
modulo:

```
Random(n) = (RandSeed * n) shr 32          { RandSeed is the post-step state }
```

which is uniform on `0..n-1` up to the usual 2^32-vs-`n` rounding bias, and
is exactly the shape the task brief predicted. `Random(0)` yields `0`.

Note the ordering: `Random` steps the seed **first** and uses the new state.
`Rng::next_u32` and `Rng::below` in `src/rng.rs` follow that ordering, so
`below(n)` consumes exactly one `next_u32` worth of state, as the original
does.

## `Randomize`

Disassembly of `1f78:11e0` (13 bytes):

```
000011E0  B42C              mov ah,0x2c            ; DOS "get system time"
000011E2  CD21              int 0x21               ; CX = hour:minute, DX = sec:hundredths
000011E4  890E7E36          mov [0x367e],cx
000011E8  89168036          mov [0x3680],dx
000011EC  CB                retf
```

The seed source is **DOS INT 21h/AH=2Ch**, not the BIOS tick via INT 1Ah —
there is no `INT 1Ah` anywhere in the binary. `RandSeed.lo := CX`
(`CH` = hour, `CL` = minute), `RandSeed.hi := DX` (`DH` = second,
`DL` = hundredths). So the effective seed is
`(sec shl 24) or (hundredths shl 16) or (hour shl 8) or minute`.

`Randomize` is called exactly once, from `FUN_1000_6a0d` (`1000:6a0d`,
decompiled line 36), immediately after `FUN_1000_02c2` and before the
routine's main input loop. **This runs on every real playthrough**, not just
once ever: the load image ships `RandSeed` (`20ae:367e`) as `$00000000`,
but that value never survives past the first few instructions of a real
run — `Randomize` overwrites it from the system clock before the player
sees the command prompt. No later task should assume a fresh game starts
with `RandSeed = 0`; it does not. Seed `0` is used for the vectors below
for a different, narrower reason: choice of seed is arbitrary for an LCG
(every seed produces an equally valid sequence, none is more "correct" than
another), so `0` was picked as a convenient, memorable fixed point for a
reproducibility fixture — it is not, and was never claimed to be, the
game's operating seed.

Reproducing a `Randomize` seed is a port-level policy decision, not an RE
one, and is deliberately left out of `src/rng.rs`: `Rng::new` takes the seed
explicitly.

### Determinism under the emulator (fix wave 1, empirical)

`docs/re/oracle.md` used to claim that repeated oracle runs — including ones
issuing a `w` (wander) command whose outcome is RNG-driven — produced
byte-identical `SCREEN.BIN` captures, and read that as evidence against
timer-seeding ("a `Randomize` seeded from the timer would have shown up
here immediately"). That claim contradicted this file's static finding
(`Randomize` reseeds from `INT 21h/AH=2Ch` on every run) and needed to be
settled empirically rather than by picking whichever note was more
convenient. It has now been checked directly against `orig/g.exe` running
under `tools/oracle/`'s DOSBox-X config, on 2026-08-18. **The static finding
in this file was correct; the old claim in `docs/re/oracle.md` was an
over-read and has been corrected there.**

Two separate checks, both reproducible with the tooling in `tools/oracle/`:

**1. The guest's clock is live, not fixed.** A standalone 40-byte COM probe
(not committed — a throwaway NASM program built for this check) ran
`INT 21h/AH=2Ch` (get system time) and `AH=2Ah` (get system date) directly
under the oracle's DOSBox-X config and wrote the raw `CX:DX` result to a
host file. Three separate emulator invocations, launched roughly 6 seconds
apart on the wall clock, returned:

| run | host UTC time at launch | guest `CX:DX` (hour:min / sec:hundredths, hex) |
|---|---|---|
| A | 10:19:55.10 | `0C12` / `1E5F` → 12:18:30.95 |
| B | 10:20:10.89 | `0C12` / `2463` → 12:18:36.99 |
| C | 10:20:26.80 | `0C12` / `2B61` → 12:18:43.97 |

The guest second/hundredths field tracks the host wall clock across
invocations (each ~1s of guest boot time behind the host launch instant,
consistent, not coincidental). DOSBox-X's emulated `INT 21h/AH=2Ch` is
backed by the real host clock in this configuration and version
(2026.08.02) — it is **not** a fixed simulated instant. That is the
opposite of the "constant seed under the emulator" hypothesis: nothing
here pins the seed.

**2. That live clock actually reaches observable RNG output — but only once
play starts.** Two scripts were run through `tools/oracle/capture.py`
(`--expect-frames` pinned in both cases so a truncated capture could not
pass as complete):

- *Intro-only* (`capture.INTRO_KEYS + "e\n\n"`, 15 frames, the same script
  `tools/oracle/test_oracle_smoke.py::test_capture` uses): three runs with
  real 20-second gaps between them all produced the exact same
  `SCREEN.BIN`, md5 `4447e10ac1c3f02a0519f5d833d85054` — matching the hash
  already on record in `docs/re/oracle.md`. This is real, and it is
  **deterministic by construction, not evidence about the RNG**: the
  scripted key sequence marches through a fixed sequence of prompts (title
  screen, district, seven any-key story pages, class `0`, default name),
  and none of the text on any of those screens is drawn from a value
  `Random`/`@Rand` produced after `Randomize` ran. A test that cannot reach
  RNG-dependent output cannot detect clock-seeding, no matter how many
  times it is repeated.
- *Walking* (`capture.INTRO_KEYS` + `w\n` × 50 + `"e\n\n"`, 177 bytes —
  comfortably under the 1024-byte key-script limit, 114 frames): three runs
  with real ~15-second gaps between them produced **three different**
  `SCREEN.BIN` captures — md5 `2ef8be0a56e9ca368b9fa2a22cbdd5d8`,
  `209de01a7a48937032fdccfd2e5331af`,
  `ccabc6bea770861727272742502d2037`. The first two of the three diverge as
  early as frame 18 of 114 (already inside the wander loop, where
  accumulated "Ничё не происходит" ["nothing happens"] text differs), and
  each run generated a *different* first enemy while wandering — a
  `Random`-drawn table index, not a scripted value: run 1 hit "ментяра 0
  уровня" at frame 18, run 2 hit "Нарк 0 уровня" at frame 20, run 3 hit
  "Беспредельщик 0 уровня" at frame 34.

Together these rule out both "the seed is constant under the emulator" and
"`w`'s outcome isn't actually RNG-driven": the emulator's clock genuinely
advances with wall time, and that live clock genuinely reaches the RNG
output the player sees once the game leaves its scripted intro. The
original `docs/re/oracle.md` note tested only a single `w` per run (not
50), which mostly draws the low-probability "nothing happens" branch
regardless of seed (roughly 85% of draws in the sample above), and does not
say the runs it tested had a real wall-clock gap between them — a plausible
account of how that note ended up "byte-identical" is that it re-ran the
same fast (<1s) script back-to-back with no gap and got unlucky on a single
low-information draw, not that the seed was actually fixed. That account is
not independently verified (the script and hash `1c9a769c…` it cites were
never committed), so treat it as the likely explanation rather than a
proven one; what *is* proven is the walking-script divergence above, which
directly falsifies the "byte-identical...would have shown up immediately"
conclusion regardless of why the earlier note read the way it did.

**What this means for Task 12.** The differential harness cannot rely on
run-to-run determinism for any screen whose content depends on a value
drawn from `Random`/`@Rand` after the game's own `Randomize` call — under
this DOSBox-X config, every fresh emulator run gets a different seed, so
raw oracle output for such a screen will not even reproduce against
*itself* from one run to the next, let alone against the Rust port. This
would also be true, independently, on real DOS hardware (the same
`INT 21h/AH=2Ch` timer source), and would only stop being true if the
DOSBox-X config were changed to pin the emulated clock (not attempted
here — no such option was tried or is known to exist in the current
config) or the guest binary were patched to skip the `Randomize` call
(sketched below, in "What was *not* done"). Task 12 needs to pick one of:
comparing only RNG-independent screens/quantities, pinning `RandSeed` on
both sides via the binary patch described below, or finding and pinning an
emulator-level clock override — this file does not decide which; that is
Task 12's call.

## How the game uses it

`Random` (`1f78:114b`) has 5 direct callers — `entry`, `FUN_1000_0d14`,
`FUN_1000_2526`, `FUN_1000_3d11`, `FUN_1000_7c67` — and **86** call sites
across them, so the generator is pervasive rather than incidental. Verified
two ways: `orig/g.exe` contains exactly 86 far-call (`9Ah`) encodings whose
offset is `114b`, and `build/decomp/` has exactly 86 textual call sites to
`FUN_1f78_114b`, split 42 in `entry`, 27 in `FUN_1000_3d11`, 14 in
`FUN_1000_0d14`, 2 in `FUN_1000_7c67`, 1 in `FUN_1000_2526` — the two counts
agree, and the per-function split sums to the total.
Literal moduli seen at those call sites include `2`, `3`, `4`, `5`, `6`,
`10`, `18`, `30`, `50`, `51`, `100`, plus computed ones such as
`Random(hi - lo)` for ranges and `Random(level * 25)` /
`Random(level * 40)`-shaped rolls.

## `data/rng_vectors.json` — provenance

Regenerate with:

```bash
python3 tools/gen_rng_vectors.py
```

`tools/gen_rng_vectors.py` opens `orig/g.exe`, computes the file offsets of
`1f78:11a8` and `1f78:114b` from the MZ header, and **decodes and executes
the original's own instruction bytes** in a ~100-line 8086 interpreter that
implements only the two dozen opcodes those routines use. Every number that
matters — the `$8405` literal, the shift counts, the `+1`, the `$367e` /
`$3680` seed addresses — is read out of the binary at run time; none of it is
typed into the script. The script aborts on any opcode it does not
recognise, so it cannot silently skip an instruction.

The script additionally cross-checks its per-step output against the closed
form `s := s * $08088405 + 1` derived by hand above, and exits non-zero on
any mismatch. That is a consistency check between two readings of the same
bytes, not a second independent source — the interpreter is the ground truth.

**The generator is independent of `src/rng.rs`.** It does not build, link,
import, or shell out to the Rust crate; it never reads `src/rng.rs`. The
vectors would be identical if `src/rng.rs` did not exist. This matters
because vectors produced by the implementation under test would prove
nothing.

Contents: seed `0`, 96 consecutive `next_u32` outputs, and 64 consecutive
`Random(n)` outputs for each of `n` in `{100, 51, 10, 6, 3, 2}` — all six
moduli taken from literal `Random(n)` call sites in the game code.

## What was *not* done

The original was never executed under DOSBox-X to confirm the RNG
*algorithm* itself (the multiplier, increment and range-mapping formula):
the static recovery above is unambiguous enough not to need it, and the
"fix wave 1" oracle runs described above were only to settle the
determinism question, not to decode printed numbers back into RNG draws or
compare them against `data/rng_vectors.json`. That remains the route for a
future, fully independent algorithmic confirmation: patch a copy of the
binary in the oracle workdir to skip the `call` at `1000:6a0d` that reaches
`Randomize` (leaving `RandSeed` at `$00000000`), then compare printed rolls
against `data/rng_vectors.json`. Task 12 will likely need exactly this patch
anyway, for a different reason — see "What this means for Task 12" above:
without it, no RNG-dependent oracle screen reproduces even against itself
from one run to the next.
