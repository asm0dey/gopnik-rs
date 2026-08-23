# The Borland runtime (Task 11h)

`data/functions.json` records 123 functions.  16 of them are the game's own
code in segment `1000`; the other 107 are in segments `0ee5`, `0eed`, `0f16`
and `0f78`, and 105 of those were still `FUN_*`.  This document says what they
are.

The method is identification, not reverse engineering: the Turbo Pascal 7
runtime ships compiled, with its symbol tables, in a distribution's
`BIN/TURBO.TPL`.  `tools/tpl.py` reads that file; `tools/rtlmatch.py` aligns
this program's runtime segments against it and writes `data/rtl_names.json`.
`python3 tools/re_query.py resolve <citation>` prints the name for any address
inside an identified routine.

Addresses are runtime `seg:off` (Form B in `docs/re/METHODOLOGY.md`); the
Ghidra label for each is the same offset with `0x1000` added to the segment.

## The version claim

**Established from flow.** The linked runtime is Borland's Turbo Pascal 7
library.  Four independent markers:

1. **The copyright string.** `orig/g.exe` contains, at `0f78:0264` (file
   `0x112b4`), `Portions Copyright (c) 1983,92 Borland`.  `1983,92` is the
   TP 7 stamp; 6.0 stamps `1983,90`.
2. **The same string is inside Borland's own `SYSTEM` unit, at the same block
   offset.**  In `TURBO.TPL` the `SYSTEM` unit's code offset `0x24c` holds
   `Runtime error \0 at \0.\r\n\0Portions Copyright (c) 1983,92 Borland`,
   byte for byte what `0f78:024c` holds here.  That is not a string a linker
   copies from anywhere else: it is the tail of `SYSTEM`'s block 0, which is
   also where the runtime-error printer that uses it lives (`0f78:0116`).
3. **The distribution identifies itself as 7.0.** `/home/finkel/Downloads/TP`
   is the library this was matched against; its `README` opens "Welcome to
   Turbo Pascal 7.0", and the five units in its `TURBO.TPL` name their sources
   `SYSTEM.PAS`, `OVERLAY.PAS`, `CRT.PAS`, `DOS.PAS`, `PRINTER.PAS`.
4. **The code matches.**  4958 of the 4960 bytes of segment `0f78` are 17
   `SYSTEM` blocks in table order; all 1567 bytes of `CRT`'s code are segment
   `0f16`; 124 of the 128 bytes of segment `0ee5` are one `DOS` block.  The
   remainder in each case is the linker's padding to a paragraph boundary.
   Reproduce with `python3 tools/rtlmatch.py align`.

**Not established: which build of TP 7.** Four routines match their block's
prefix and then diverge structurally, so this program links a *slightly
different build* of the same library than this distribution's:

| routine | what differs |
|---|---|
| `0f78:0c8f` (`Delete`) | **11 bytes added, 6 removed** (net +5, which is why everything after is shifted by five). See below — the difference is behavioural, not just a size |
| `0f16:003b` | uses `bl` where the library uses `al` in the CGA read-back loop |
| `0f16:02a8` (`Delay`) | a different 32-byte body |
| `0f16:02c8` | the snow-avoidance wait loop, `al` where the library uses `bl` |

**`Delete`'s index clamp, exactly.**  `Delete(S, Index, Count)` passes `Count`
at `[bp+6]` and `Index` at `[bp+8]`.  Both copies open the same way, and both
clamp `Count` to 255.  The difference, decoded from `orig/g.exe` on one side
and `TURBO.TPL`'s `SYSTEM` code on the other:

```
library                                  this program
  +7  83 7e 06 00  cmp word [bp+6],0       +7  83 7e 06 00  cmp word [bp+6],0
 +11  7e 5c        jle epilogue           +11  7e 61        jle epilogue
 +13  83 7e 08 00  cmp word [bp+8],0       -- REMOVED, 6 bytes --
 +17  7e 56        jle epilogue            --
 +19  81 7e 08 ff 00  cmp [bp+8],255      +13  81 7e 08 ff 00  cmp [bp+8],255
 +24  7f 4f        jg epilogue            +18  7f 5a        jg epilogue
 +26  81 7e 06 ff 00  cmp [bp+6],255      +20  81 7e 06 ff 00  cmp [bp+6],255
 +31  7e 05        jle +5                 +25  7e 05        jle +5
 +33  c7 46 06 ff 00  mov [bp+6],255      +27  c7 46 06 ff 00  mov [bp+6],255
                                          -- ADDED, 11 bytes --
                                          +32  83 7e 08 01  cmp word [bp+8],1
                                          +36  7d 05        jge +5
                                          +38  c7 46 08 01 00  mov [bp+8],1
 +38  8d be 00 ff  lea di,[bp-0x100]      +43  8d be 00 ff  lea di,[bp-0x100]
```

So it is **not** "a five-byte clamp added"; five is the net.  And the two
copies do not merely differ in size: the library **returns without touching
the string** when `Index <= 0`, while this build **clamps `Index` to 1** and
deletes from the front.  `Delete(S, 0, 3)` is a no-op in the library and
removes three characters here.  That is a behavioural difference and is
recorded in `docs/re/gaps.md` as well.

It changes nothing in this program: `0f78:0c8f` has **no caller anywhere in
the image** — 0 far-call sites (`9a 8f 0c 78 0f`), 0 near `e8` calls to
`0x0c8f` within segment `0f78`, and the far pointer `8f 0c 78 0f` occurs 0
times as data.  `Delete` is linked in and never reached.  It is evidence about
which *build* was linked, not a live semantic difference — and the port would
only need it if it ever emulated the routine.

That pattern — string-index clamping in `SYSTEM` and the `CRT` timing and snow
loops — is what a 7.01 or a patched `CRT` would look like, but this
distribution holds only one `TURBO.TPL` and there is nothing here to compare a
second build against.  **Unverified**, and the way to settle it is a TP 7.01
`TURBO.TPL`.  It does not weaken the identifications: `python3
tools/rtlmatch.py align` reports the divergence per block, and
`data/rtl_names.json` records `mode: divergent` on exactly those four
routines.

## How the alignment works

Turbo Pascal 7 smart-links at **block** granularity.  Each unit's code section
in the library is a tiling of blocks (`tools/tpl.py` asserts the block sizes
sum exactly to the code size), and the linker keeps a subset of them, in
table order, dropping the rest.  So the alignment is a walk: from offset 0 of
the program's segment, find the next block of the unit whose bytes fit there.

"Fit" is not "equal".  The library stores each block with its address fields
unresolved, so a correctly placed block still differs from the linked copy at
every fixup.  The test is the SHAPE of the difference: every maximal run of
differing bytes must be at most four bytes, the width of a far pointer, and
the bytes in longer runs must be under a third of the block.  Both halves
matter — the run-length test alone lets a 46-byte block that is 45% relocation
through, and the fraction test alone accepts nothing but near-identity.

**The matcher can say no, and does.**  `python3 tools/rtlmatch.py reject`
runs the game's own 61008-byte code segment against all five units and each
runtime segment against every unit it is not:

```
$ python3 tools/rtlmatch.py reject
game code segment 1000 vs SYSTEM  : 0 of 61008 bytes aligned
game code segment 1000 vs OVERLAY : 0 of 61008 bytes aligned
game code segment 1000 vs CRT     : 0 of 61008 bytes aligned
game code segment 1000 vs DOS     : 0 of 61008 bytes aligned
game code segment 1000 vs PRINTER : 0 of 61008 bytes aligned
segment 0ee5 vs SYSTEM  : 0 of 128 bytes aligned
segment 0ee5 vs OVERLAY : 0 of 128 bytes aligned
segment 0ee5 vs CRT     : 0 of 128 bytes aligned
segment 0ee5 vs DOS     : 124 of 128 bytes aligned
segment 0ee5 vs PRINTER : 0 of 128 bytes aligned
segment 0eed vs SYSTEM  : 0 of 656 bytes aligned
segment 0eed vs OVERLAY : 0 of 656 bytes aligned
segment 0eed vs CRT     : 0 of 656 bytes aligned
segment 0eed vs DOS     : 0 of 656 bytes aligned
segment 0eed vs PRINTER : 0 of 656 bytes aligned
segment 0f16 vs SYSTEM  : 0 of 1568 bytes aligned
segment 0f16 vs OVERLAY : 13 of 1568 bytes aligned
segment 0f16 vs CRT     : 1567 of 1568 bytes aligned
segment 0f16 vs DOS     : 0 of 1568 bytes aligned
segment 0f16 vs PRINTER : 0 of 1568 bytes aligned
segment 0f78 vs SYSTEM  : 4958 of 4960 bytes aligned
segment 0f78 vs OVERLAY : 0 of 4960 bytes aligned
segment 0f78 vs CRT     : 0 of 4960 bytes aligned
segment 0f78 vs DOS     : 0 of 4960 bytes aligned
segment 0f78 vs PRINTER : 0 of 4960 bytes aligned
VERDICT: the game's own code matches no unit
```

The one non-zero cross-match is `0f16` against `OVERLAY`, 13 bytes: `CRT`
block 0 and `OVERLAY` block 0 are both the same 13-byte `Halt` stub.  13
against `CRT`'s 1567 is not a competitor, and the unit is chosen by coverage.

Before the long-run fraction gate was added, `CRT`'s segment "aligned" 1152
bytes against `SYSTEM` — `SYSTEM` block 35 opens with the same twelve bytes as
`CRT` block 1 — of which 1127 differed.  That false positive is why the gate
exists and why `reject` prints both sides rather than a verdict alone.

### Why `fits()` alone proves nothing

`rtlmatch.fits()` is the prefix gate, and on its own it is a **weak** test —
weak enough that reusing it as a standalone "is this the same routine?"
criterion would produce confident wrong matches.  It accepts any pair with a
matching byte at least every five, so most of a byte string may differ and it
still returns true.  Measured against segment `0eed`, which is not runtime at
all, at the `PREFIX` length of 12:

| library block | offset in `0eed` | `fits()` | bytes differing |
|---|---|---|---:|
| `SYSTEM` block 3 | `0eed:00b4` | yes | 9 of 12 |
| `SYSTEM` block 15 | `0eed:00f5` | yes | 9 of 12 |
| `CRT` block 0 (whole, 13 bytes) | `0eed:00f7` | yes | 11 of 13 |

The third row survives `MAX_LONG_RUN_FRACTION` as well, because 11 differing
bytes spread over three runs of at most four contain no long run to measure.
Discrimination comes from three things in ascending order of strength: the
prefix gate (nearly free), the long-run fraction **over the whole block**
(which is what kills the 1139-byte `SYSTEM` block 35 false positive above),
and above both the **coverage** the walk accumulates — 4958 of 4960 for a
segment that is the unit, 37 of 656 for one that is not (and of that 37 only
13 bytes were compared at all; see "Which unit each segment is" below).

## Which unit each segment is

Chosen by how much of the segment the alignment accounts for, not by the
segment number:

| segment | unit | code aligned | blocks kept | functions |
|---|---|---|---|---:|
| `0ee5` | `DOS` | 124 of 128 | 1 of 19 | 3 |
| `0eed` | **not a Turbo Pascal unit** | 0 of 656 | — | 3 |
| `0f16` | `CRT` | 1567 of 1568 | 3 of 3 | 20 |
| `0f78` | `SYSTEM` | 4958 of 4960 | 17 of 47 | 81 |

`0eed` matches no unit of the library, and the program's other three runtime
segments each match exactly one.  It is the game's own second code segment — a
Pascal unit of its own — and it is why the brief's "107 Borland runtime
functions" is three too many.

**What "matches no unit" is measured over.** `python3 tools/rtlmatch.py
reject` prints `0 of 656` for all five units, but `align()` only looks for the
first block at offsets `0..SLACK` (24) from the segment start, so that run on
its own cannot support the words "at any offset".  Restarting `align()` at
**each of the 656 offsets** and taking the best coverage any start reaches:

```
$ python3 - <<'PY'
import sys; sys.path.insert(0, 'tools')
import addr, tpl, rtlmatch
units = list(tpl.units(tpl.read_tpl()))
img = addr.load_image(addr.read_exe())
base, length = rtlmatch.SEGMENTS[0x0EED]
seg = img[base:base + length]
for u in units:
    best = max((rtlmatch.align(seg[q:], u)[1], q) for q in range(length))
    print("0eed vs %-8s: best coverage %4d of %d bytes over all %d start offsets"
          % (u.name, best[0], length, length))
PY
0eed vs SYSTEM  : best coverage   36 of 656 bytes over all 656 start offsets
0eed vs OVERLAY : best coverage    0 of 656 bytes over all 656 start offsets
0eed vs CRT     : best coverage   37 of 656 bytes over all 656 start offsets
0eed vs DOS     : best coverage   36 of 656 bytes over all 656 start offsets
0eed vs PRINTER : best coverage    0 of 656 bytes over all 656 start offsets
```

(Needs the library: `GOPNIK_TPL=…/BIN/TURBO.TPL`.)

37 of 656 is 5.6%, against 1567 of 1568 for `0f16` and 4958 of 4960 for
`0f78` — and even 5.6% flatters it, because `align()`'s `covered` is the
offset the walk **reached**, not the number of bytes it compared, and the walk
is allowed to skip up to `SLACK` (24) bytes before a block.  The `CRT` best of
37 is a single placement:

```
CRT best cov 37 at start offset 223 (0eed:00df)
    block 0, size 13, seg_off 24, gap_before 24, matched_bytes 13,
    diff_bytes 11 in 3 runs, long_run_bytes 0
```

(`seg_off` is relative to that start offset, so the block sits at
223 + 24 = 247 = `0eed:00f7` — the same placement the triple scan below finds.)

24 skipped + 13 compared = 37, so **13 bytes were ever compared and 2 of them
agreed**.  `SYSTEM`'s and `DOS`'s bests of 36 have the same shape: 24 skipped
plus a 12-byte fragment of a 145-byte and a 153-byte block, cut off by the end
of the segment.

Those bytes are not a partial match either.  Exhaustively testing every
(offset, unit, block) triple — 46,728 of them — against both of `align()`'s
gates:

```
triples tried:                                             46728
prefix-gate survivors (fits() alone):                         86
survivors of both gates, whole block inside the segment:       3
   off 247 (0eed:00f7) CRT block 0 size 13  diff_bytes=11 in 3 runs
   off 268 (0eed:010c) CRT block 0 size 13  diff_bytes=11 in 3 runs
   off 288 (0eed:0120) CRT block 0 size 13  diff_bytes=11 in 3 runs
survivors of both gates, block truncated by the segment end:  37
   at offsets 644, 646, 647, 648 only; blocks of 18..1139 bytes,
   of which 8..12 bytes were compared, 2..9 of them differing
```

The **three** whole-block survivors are all the same 13-byte `CRT` block 0
(the `Halt` stub), each with **11 of its 13 bytes differing** — `fits()` being
weak on a short block, not a match; see "Why `fits()` alone proves nothing"
above.  The **37** truncated survivors are recorded here rather than left out,
but they carry less still: they sit at four start offsets inside the segment's
last twelve bytes, and each compares only the 8-to-12-byte head of a block
that is otherwise entirely outside the segment.  A block that is 99% off the
end is not evidence that the segment is that block's unit.

**Positive evidence for what `0eed` is**, which is stronger than the negative:
the byte pair `31 c0` (`xor ax,ax`, the `0x31` direction) occurs **8 times in
`0eed`'s 656 bytes and 3200 times in the game's own segment `1000` and its
embedded data, and 0 times in all 27,826 code bytes of the library** — see
"The instruction-encoding observation" below, which says exactly what those
byte counts are and are not, and why `55 89 e5` is *not* a discriminator.

Segment `0eed`'s three routines call the
runtime (`lcall 0f78:02cd`, the stack check, opens each of them) and one of
them calls `CRT.TextColor`; `docs/re/command-dispatch.md` already cites
`0eed:0000` as the game's colour-markup write and `0eed:0216` as its
case-fold.  They are game code and are left unnamed here.

`DOS` contributes exactly one block: unit code `0x171`, 124 bytes, which the
`DOS` symbol table names as `FindFirst` (entry `+0x090`, unit offset `0x171`)
and `FindNext` (entry `+0x098`, unit offset `0x1af`).  Nothing else of `DOS`
is linked in.

## Where the names come from, and where they cannot

Three kinds of name, and the difference is load-bearing:

- **`tpl_symbol`** — read verbatim from the library's own symbol table.  A
  type-0x52 record names an ordinary routine and carries its entry-table
  offset, and the entry table resolves to a block and an offset in it.  Eight
  of this program's routines land exactly on such an address.
- **`borland`** — a Borland *standard procedure* name established from the
  routine's own disassembly.  `SYSTEM` exports nothing but standard
  procedures, and their symbol records carry a compiler intrinsic id
  (`Assign` = 0x28, `Random` = 0xa0) that `TPC.EXE` maps to an entry, not a
  code offset.  **The library cannot tell you which `SYSTEM` entry is which
  standard procedure.**  So each of these rests on flow, and several are
  additionally pinned by a cross-unit call from a unit whose behaviour is
  known:
    - `PRINTER`'s initialisation is `Assign(Lst, 'LPT1'); Rewrite(Lst)` — the
      `'LPT1'` literal is in its code — and it calls `SYSTEM` entries
      `+0x228` and `+0x238`, which resolve to `0f78:02e6` and `0f78:0369`.
    - `CRT`'s initialisation calls `+0x230` and `+0x238`, which resolve to
      `0f78:0364` and `0f78:0369` — the same `+0x238`, from an independent
      unit.
    - That fixes `+0x228`/`+0x230`/`+0x238` as `Assign`/`Reset`/`Rewrite`, and
      `+0x240` continues the run as `Close`, which its own body confirms (it
      rejects a `TextRec` whose mode is not `0xd7b1`/`0xd7b2` with error 103).
- **`behavioural`** — a name coined here for a routine with no user-visible
  Borland name: a helper, an operator, a device driver entry.  These are
  descriptions, not Borland symbols; do not cite one as if it were.
  **All 72 carry an `rtl_` prefix**, so a coined name is unmistakable on
  sight even where the `kind` field is not to hand.  One did not — the CRT
  unit initialiser was `Crt_initialization`, which reads exactly like a
  compiler-generated Pascal symbol, the misreading this convention exists to
  prevent; it is `rtl_crt_initialization`.  A new coined name must keep the
  prefix.

Anything that could be more than one standard procedure keeps a behavioural
name.  `0f78:081e` and `0f78:0825` are INT 21h AH=3Fh/AH=40h of `RecSize`
bytes, which is the shape of both `Read`/`Write` on a typed file and
`BlockRead`/`BlockWrite`; they are `rtl_file_read` and `rtl_file_write`, not
either pair.

Every record in `data/rtl_names.json` carries its `name_kind`, its tier
(`flow`, per `docs/re/METHODOLOGY.md`) and the evidence, and
`tools/test_rtlmatch.py` fails if any name lacks one.

## What this settles elsewhere in `docs/re/`

- **`docs/re/command-dispatch.md`, the input read.**  It cited `1000:ae63`
  `call far 0f78:06c6` as "confirmed a Pascal `ReadLn` by its position".  That
  is right, and it is now established from flow rather than from position:
  `0f78:06c6` is the `Read(Text, String)` worker (it takes the destination at
  `[bp+8]` and its maximum length at `[bp+6]`), and `1000:ae63` is followed
  immediately by `lcall 0f78:059d` — the `ReadLn` line-skip — and
  `lcall 0f78:0291`, the `{$I+}` check.  Those three calls in that order ARE
  a `ReadLn(s)` statement.
- **`docs/re/command-dispatch.md`, the token compare.**  `FUN_1f78_0bd8` is
  `rtl_str_compare`: `0f78:0bf8` is `repe cmpsb` over the shorter of the two
  lengths and `0f78:0bfc` then compares the lengths, leaving flags and no
  return value.  The document's reading is confirmed from the disassembly, not
  from Ghidra's C.
- **`docs/re/tables.md`, `1000:5014` "unidentified".**  The debited amount is
  now readable as flow: `0f78:1125` converts an integer to a 6-byte real,
  `0f78:1117` divides it by the literal materialised at `1000:4ff5`
   (`0f78:1117`'s `or cl,cl` / `je` rejects a zero *second* operand, so the
   second operand is the divisor)
  (`cx=0x0083`, `si=0x0000`, `di=0x2000`), `0f78:1111` multiplies by the
  literal at `1000:5002` (`cx=0x0082`, `si=0x0000`, `di=0x4000`), and
  `0f78:1131` converts back to an integer, raising error 207 on overflow.  So
  the amount is `round(x / K1 * K2)`.  Reading `K1` and `K2` as decimals needs
  the 6-byte real layout confirmed against a known value and is **not
  established here**.
- **`docs/re/gaps.md`** corroborates the real-arithmetic block from the other
  direction.  Recovering character generation, it established that `0f78:1117`
  is the divide "not the multiply: it is the entry that tests `cl` for a zero
  divisor and raises runtime error 200", that `0f78:1111` is the multiply, and
  that `0f78:1131` sets `ch = 1` and calls `0f78:1091` for a half-away-from-zero
  `Round` — checked there against `data/rng_trace.json`.  Every one of those
  matches what the alignment and the disassembly say here, arrived at without
  the library.
- **`docs/re/rng.md`** is unchanged and corroborated: `0f78:114b` `Random`,
  `0f78:11a8` the LCG step, `0f78:11e0` `Randomize` all fall on `SYSTEM` entry
  table offsets `+0x2e8`, block-internal, and `+0x220` respectively, and
  `0f78:114b` is byte-identical to the library's copy.

## The instruction-encoding observation, and what it is NOT

The game's own code encodes `push bp; mov bp,sp` as `55 89 e5` and `xor ax,ax`
as `31 c0`; `SYSTEM` uses `55 8b ec` and `33 c0` throughout.  That looked at
first like evidence of a different compiler.  It is not: `PRINTER`, whose 54
bytes of code are a plain Pascal `Assign(Lst,'LPT1'); Rewrite(Lst)`, also
encodes `55 89 e5`, and so do `CRT` and `OVERLAY` in their compiled parts.
The split is between Borland's hand-written assembler (`SYSTEM`, `DOS`, most
of `CRT`) and compiler output, not between compilers.

**What was counted, exactly.**  These are **raw byte-substring occurrences**
over whole regions, counted with `bytes.find` at every offset — *not* decoded
instructions, and no offset here was checked for being an instruction
boundary.  The regions are the whole segment ranges from
`tools/rtlmatch.py`'s `SEGMENTS` (`0eed` 656 bytes, `0f16` 1568, `0f78` 4960)
and, for the game, image offsets `0x0`..`0xee50` — **61,008 bytes** that hold
the game's code *and* the string and table data embedded in it.  3200 hits in
61,008 mixed bytes is 5.2% of all offsets, which no instruction census would
give; most of them are data.  The column is a byte-frequency comparison, and
it is only used as one:

| encoding | seg `1000` + its data (61,008 B) | seg `0eed` (656 B) | seg `0f16` (1568 B) | seg `0f78` (4960 B) | library code (27,826 B) |
|---|---:|---:|---:|---:|---:|
| `55 89 e5` | 15 | 3 | 2 | 0 | 4 |
| `31 c0` | 3200 | 8 | 0 | 0 | **0** |
| `30 c0` | 0 | 0 | 0 | 0 | 0 |

So `55 89 e5` is **not** a discriminator — Borland emits it too, in `PRINTER`,
`CRT` and `OVERLAY` — and any claim resting on it is retracted.  `31 c0` is:
the `30`/`31` `xor` forms do not occur once in the library's 27,826 code
bytes, and occur 8 times in `0eed` and 3200 times in the game's own segment.
That asymmetry is what `tools/rtlmatch.py`'s `NON_RUNTIME_SEGMENTS` comment
now cites.  It remains **unexplained**, and on its own it is a fact about
encodings, not a conclusion about which compiler built the game — the
alignment coverage above is what settles `0eed`.

## Decoder cross-check

`tools/dis16.py` and `capstone` (`CS_ARCH_X86` + `CS_MODE_16`) draw the same
instruction boundaries everywhere they were both run: **0 disagreements**.
There are **two** comparisons, over two different populations, and an earlier
version of this section reported the smaller one's count as if it were the
whole thing.

**1. A linear sweep of each runtime segment — 1282 instructions.**  A linear
sweep starts at offset 0 and halts at the first byte it will not decode,
whether or not that byte is code, so it is not a disassembly of the segment.
Where each sweep stops:

| segment | length | sweep stops at | why | `dis16` insns | capstone insns |
|---|---:|---:|---|---:|---:|
| `0ee5` | `0x0080` | `0x0080` (end) | ran to the end | 70 | 70 |
| `0eed` | `0x0290` | `0x028f` | modrm runs off the end of the buffer | 260 | 260 |
| `0f16` | `0x0620` | `0x061f` | modrm runs off the end of the buffer | 663 | 663 |
| `0f78` | `0x1360` | **`0x0273`** | **`0x67`, an address-size prefix** | **289** | **2235** |

`0f78:0273` is the letter `g` of `Copyright`, inside
`Portions Copyright (c) 1983,92 Borland` at `0f78:0264` — the same string the
version claim rests on.  `0x67` is the 386 address-size prefix and `dis16`
refuses it by design, so the sweep stops there and **87% of the segment
holding 81 of the 107 routines never entered this comparison**.  1282 is
70 + 260 + 663 + 289, not a count over four whole segments.

**2. Every named routine, anchored at its own entry — 2258 instructions.**
This one is not sweep-limited, because each routine starts a fresh decode at
an address the export says is an entry.  It covers **4973 of the 4975 bytes**
of the 104 named routines; the two missing are the last two of the 22-byte
window taken for `0f78:1117`.  That window is not a body — `0f78:1117` is a
split function, so decoding 22 bytes forward from its entry runs
into the `call` at `0f78:112b`, which belongs to a different routine and whose
third byte falls outside the window ("One thing the assertion found" below).
This is the comparison that actually covers `0f78`.

`tools/test_rtlmatch.py`'s `TestCapstoneAgreesWithDis16` asserts both, and now
asserts the exact populations (1282 and 2258) and the sweep stop points rather
than a `> 1000` floor — a sweep that silently shortens fails instead of still
clearing the floor.  It also counts the offsets one decoder reached that the
other did not start an instruction at, which the previous version skipped with
a bare `continue`; that count is **0**, and is asserted to stay 0.

`dis16` remains the shipped decoder; capstone is a third opinion, alongside
the `ndisasm` validation `dis16` already carried.  The capstone here is
`importlib.metadata.version("capstone")` = `5.0.9`, `capstone.__version__` =
`5.0.7`, `capstone.cs_version()` = `(5, 0, 1280)`.  An earlier version of this
section wrote that last tuple as "capstone 5.0.1280", which is not a release
number.

## The routines

### `0ee5` — DOS

| address | size | name | kind | match | game callers |
|---|---:|---|---|---|---:|
| `0ee5:0000` | 62 | `FindFirst` | tpl_symbol | exact | 1 |
| `0ee5:003e` | 26 | `FindNext` | tpl_symbol | exact | 1 |
| `0ee5:0058` | 36 | `rtl_dos_findnext_interior` | behavioural | fixups_only | 0 |

### `0eed` — not a Turbo Pascal unit

| address | size | name | kind | match | game callers |
|---|---:|---|---|---|---:|
| `0eed:0000` | 450 | — *(unnamed)* | — | not_runtime | 7 |
| `0eed:01c2` | 84 | — *(unnamed)* | — | not_runtime | 13 |
| `0eed:0216` | 117 | — *(unnamed)* | — | not_runtime | 3 |

### `0f16` — CRT

| address | size | name | kind | match | game callers |
|---|---:|---|---|---|---:|
| `0f16:0000` | 13 | `rtl_crt_halt255` | behavioural | fixups_only | 0 |
| `0f16:000d` | 46 | `rtl_crt_initialization` | behavioural | fixups_only | 1 |
| `0f16:003b` | 104 | `rtl_crt_detect_display` | behavioural | divergent | 0 |
| `0f16:00a3` | 77 | `rtl_crt_set_mode` | behavioural | fixups_only | 0 |
| `0f16:00f0` | 72 | `rtl_crt_read_window_size` | behavioural | fixups_only | 0 |
| `0f16:014e` | 41 | `rtl_crt_flush_keyboard` | behavioural | fixups_only | 0 |
| `0f16:01cc` | 26 | `ClrScr` | tpl_symbol | fixups_only | 4 |
| `0f16:0263` | 26 | `TextColor` | tpl_symbol | fixups_only | 2 |
| `0f16:02a8` | 32 | `Delay` | tpl_symbol | divergent | 1 |
| `0f16:02c8` | 12 | `rtl_crt_wait_retrace` | behavioural | divergent | 0 |
| `0f16:0308` | 18 | `KeyPressed` | tpl_symbol | fixups_only | 1 |
| `0f16:031a` | 34 | `ReadKey` | tpl_symbol | fixups_only | 8 |
| `0f16:033c` | 43 | `AssignCrt` | tpl_symbol | fixups_only | 0 |
| `0f16:039f` | 173 | `rtl_crt_read_line` | behavioural | fixups_only | 0 |
| `0f16:0482` | 7 | `rtl_crt_newline` | behavioural | exact | 0 |
| `0f16:0489` | 89 | `rtl_crt_put_char` | behavioural | fixups_only | 0 |
| `0f16:04e2` | 33 | `rtl_crt_scroll_if_last_row` | behavioural | fixups_only | 0 |
| `0f16:0503` | 7 | `rtl_crt_get_cursor` | behavioural | exact | 0 |
| `0f16:050a` | 7 | `rtl_crt_set_cursor` | behavioural | exact | 0 |
| `0f16:0614` | 11 | `rtl_crt_bios_video` | behavioural | exact | 0 |

### `0f78` — SYSTEM

| address | size | name | kind | match | game callers |
|---|---:|---|---|---|---:|
| `0f78:0000` | 177 | `rtl_init` | behavioural | fixups_only | 1 |
| `0f78:00b1` | 37 | `rtl_cpu_probe` | behavioural | fixups_only | 0 |
| `0f78:010f` | 4 | `rtl_runerror_here` | behavioural | exact | 0 |
| `0f78:0116` | 218 | `rtl_halt` | behavioural | fixups_only | 3 |
| `0f78:01f0` | 14 | `rtl_write_cs_asciiz` | behavioural | exact | 0 |
| `0f78:01fe` | 12 | `rtl_write_dec_word` | behavioural | exact | 0 |
| `0f78:020a` | 14 | `rtl_write_dec_digit` | behavioural | exact | 0 |
| `0f78:0218` | 7 | `rtl_write_hex_word` | behavioural | exact | 0 |
| `0f78:021f` | 11 | `rtl_write_hex_byte` | behavioural | exact | 0 |
| `0f78:022a` | 8 | `rtl_write_hex_digit` | behavioural | exact | 0 |
| `0f78:0232` | 7 | `rtl_write_char_dos` | behavioural | exact | 0 |
| `0f78:028a` | 7 | `IOResult` | borland | fixups_only | 1 |
| `0f78:0291` | 14 | `rtl_io_check` | behavioural | fixups_only | 10 |
| `0f78:02cd` | 24 | `rtl_stack_check` | behavioural | fixups_only | 15 |
| `0f78:02e6` | 83 | `Assign` | borland | fixups_only | 0 |
| `0f78:0364` | 5 | `Reset` | borland | exact | 0 |
| `0f78:0369` | 5 | `Rewrite` | borland | exact | 0 |
| `0f78:0371` | 73 | `rtl_text_open` | behavioural | fixups_only | 0 |
| `0f78:03be` | 60 | `Close` | borland | fixups_only | 0 |
| `0f78:03fa` | 17 | `rtl_text_call_vector` | behavioural | fixups_only | 0 |
| `0f78:0499` | 94 | `rtl_text_read_bytes` | behavioural | fixups_only | 0 |
| `0f78:04f7` | 79 | `rtl_text_write_bytes` | behavioural | fixups_only | 0 |
| `0f78:0546` | 87 | `rtl_text_write_block` | behavioural | fixups_only | 0 |
| `0f78:059d` | 30 | `ReadLn` | borland | fixups_only | 4 |
| `0f78:05dd` | 33 | `WriteLn` | borland | fixups_only | 8 |
| `0f78:05fe` | 27 | `rtl_text_flush_if_set` | behavioural | fixups_only | 1 |
| `0f78:0619` | 14 | `rtl_text_inout_vector` | behavioural | fixups_only | 0 |
| `0f78:0627` | 14 | `rtl_text_flush_vector` | behavioural | fixups_only | 0 |
| `0f78:0635` | 70 | `rtl_text_read_char` | behavioural | fixups_only | 0 |
| `0f78:067b` | 75 | `rtl_text_write_char` | behavioural | fixups_only | 1 |
| `0f78:06c6` | 35 | `rtl_text_read_string` | behavioural | fixups_only | 4 |
| `0f78:0701` | 44 | `rtl_text_write_string` | behavioural | fixups_only | 0 |
| `0f78:072e` | 59 | `Assign` | borland | exact | 3 |
| `0f78:0769` | 9 | `Reset` | borland | fixups_only | 1 |
| `0f78:0772` | 92 | `Rewrite` | borland | fixups_only | 2 |
| `0f78:07ea` | 37 | `Close` | borland | fixups_only | 3 |
| `0f78:080f` | 15 | `rtl_file_check_open` | behavioural | fixups_only | 0 |
| `0f78:081e` | 7 | `rtl_file_read` | behavioural | exact | 1 |
| `0f78:0825` | 47 | `rtl_file_write` | behavioural | fixups_only | 2 |
| `0f78:08bc` | 48 | `Seek` | borland | fixups_only | 0 |
| `0f78:08ec` | 81 | `GetDir` | borland | exact | 1 |
| `0f78:093d` | 65 | `ChDir` | borland | fixups_only | 0 |
| `0f78:097e` | 21 | `MkDir` | borland | exact | 0 |
| `0f78:0993` | 21 | `RmDir` | borland | exact | 0 |
| `0f78:09a8` | 27 | `rtl_str_to_asciiz` | behavioural | exact | 0 |
| `0f78:09c3` | 15 | `rtl_dos_path_call` | behavioural | fixups_only | 0 |
| `0f78:09d2` | 61 | `rtl_longint_mul` | behavioural | fixups_only | 1 |
| `0f78:0a0f` | 166 | `rtl_longint_divmod` | behavioural | fixups_only | 1 |
| `0f78:0ae7` | 26 | `rtl_str_assign` | behavioural | exact | 11 |
| `0f78:0b01` | 36 | `rtl_str_assign_max` | behavioural | exact | 6 |
| `0f78:0b25` | 65 | `Copy` | borland | exact | 0 |
| `0f78:0b66` | 44 | `rtl_str_append` | behavioural | exact | 11 |
| `0f78:0b92` | 70 | `Pos` | borland | exact | 0 |
| `0f78:0bd8` | 43 | `rtl_str_compare` | behavioural | exact | 4 |
| `0f78:0c03` | 18 | `rtl_char_to_str` | behavioural | exact | 6 |
| `0f78:0c30` | 95 | `Insert` | borland | exact | 0 |
| `0f78:0c8f` | 116 | `Delete` | borland | divergent | 0 |
| `0f78:0dea` | 4 | `rtl_real_neg_add` | behavioural | exact | 0 |
| `0f78:0dee` | 195 | `rtl_real_add` | behavioural | exact | 0 |
| `0f78:0eb1` | 251 | `rtl_real_mul` | behavioural | exact | 0 |
| `0f78:0fad` | 7 | `rtl_real_zero` | behavioural | exact | 0 |
| `0f78:0fb4` | 119 | `rtl_real_div` | behavioural | exact | 0 |
| `0f78:102b` | 23 | `rtl_real_sign_cmp` | behavioural | exact | 0 |
| `0f78:1042` | 19 | `rtl_real_equal` | behavioural | exact | 0 |
| `0f78:1055` | 60 | `rtl_real_from_longint` | behavioural | exact | 0 |
| `0f78:1091` | 110 | `rtl_real_to_longint` | behavioural | exact | 0 |
| `0f78:10ff` | 6 | `rtl_real_op_add` | behavioural | fixups_only | 1 |
| `0f78:1105` | 6 | `rtl_real_op_sub` | behavioural | fixups_only | 1 |
| `0f78:1111` | 6 | `rtl_real_op_mul` | behavioural | fixups_only | 2 |
| `0f78:1117` | 22 | `rtl_real_op_div` | behavioural | fixups_only | 4 |
| `0f78:1121` | 4 | `rtl_real_op_cmp` | behavioural | fixups_only | 2 |
| `0f78:1125` | 4 | `rtl_real_op_from_longint` | behavioural | fixups_only | 4 |
| `0f78:1131` | 14 | `rtl_real_op_to_longint` | behavioural | fixups_only | 2 |
| `0f78:114b` | 29 | `Random` | borland | exact | 5 |
| `0f78:11a8` | 54 | `rtl_rand_step` | behavioural | fixups_only | 0 |
| `0f78:11e0` | 13 | `Randomize` | borland | fixups_only | 1 |
| `0f78:11ed` | 28 | `rtl_longint_to_digits` | behavioural | exact | 0 |
| `0f78:1209` | 32 | `rtl_digit_out` | behavioural | exact | 0 |
| `0f78:1229` | 167 | `rtl_parse_number` | behavioural | exact | 0 |
| `0f78:12d0` | 75 | `Str` | borland | fixups_only | 1 |
| `0f78:131b` | 49 | `Val` | borland | fixups_only | 1 |

`size` is `data/functions.json`'s `size` verbatim, and that field is Ghidra's
count of **addresses in the body**, not the length of a contiguous span.  For
every row above but one the body is contiguous and the two numbers coincide.
The exception is `0f78:1117`, whose 22 is 10 bytes at `1117`..`1120` plus two
6-byte out-of-line error tails at `113f` and `1145` — see "One thing the
assertion found" below.  So **the 22 in that row is a byte count, not a
window**, and neither is the `compared_bytes: 22` that `rtlmatch.classify`
records for it in `data/rtl_names.json`: `rtlmatch.build` passes `size` to
`classify` as a length (`tools/rtlmatch.py`), so that comparison ran over the
contiguous window `1117`..`112c`, which contains the neighbouring thunks
`0f78:1121` and `0f78:1125` and not the two tails.  The verdict survives it —
comparing the true address set instead gives **3 runs / 6 differing bytes**
against the span's 4 runs / 7, and `fixups_only` either way, because the
library's bytes at the phantom offsets are those same thunks with their call
targets still unrelocated:

```
head    0f78:1117..1120  here 0a c9 74 2a e8 96 fe 72 1f cb  lib ... e8 00 00 ...
phantom 0f78:1121..112c  here e8 07 ff cb e8 2d ff cb b5 00 e8 63   (not this body)
tail    0f78:113f..1144  here b8 cd 00 e9 ca ef  lib b8 cd 00 e9 00 00
tail    0f78:1145..114a  here b8 c8 00 e9 c4 ef  lib b8 c8 00 e9 00 00
```

`kind` is the name's provenance as described above; `match` is `exact` (no
differing byte at all), `fixups_only` (differs only in relocation-shaped
runs), `divergent` (a longer run — a different build) or `not_runtime`.
`game callers` counts distinct functions in segment `1000` that call the
routine, from `data/functions.json`.

## Counts

```
$ python3 -c "
import json,collections
d=json.load(open('data/rtl_names.json'))
print(d['counts'])
print(collections.Counter(r['name_kind'] for r in d['routines']))
print(collections.Counter(r['match']['mode'] for r in d['routines']))"
```
{'routines': 107, 'named': 104, 'unnamed': 3}
Counter({'behavioural': 72, 'borland': 24, 'tpl_symbol': 8, None: 3})
Counter({'fixups_only': 57, 'exact': 43, 'divergent': 4, 'not_runtime': 3})

## Reproducing

```
python3 tools/tpl.py [TURBO.TPL]        # the library's structure
python3 tools/rtlmatch.py align         # the block alignment, per segment
python3 tools/rtlmatch.py reject        # the negative controls
python3 tools/rtlmatch.py emit          # rewrite data/rtl_names.json
python3 tools/test_rtlmatch.py          # 18 tests, no library needed
```

`emit` needs the library; point `GOPNIK_TPL` at a `TURBO.TPL` or pass the path.
The committed `data/rtl_names.json` records the library's SHA-256 so a
regeneration against a different one is visible in the diff.

## Not wired: `tools/ghidra/ExportAll.java`

The brief asked whether `ExportAll.java` should read the side table and emit
the names.  It is **not** wired, deliberately.

**A correction.** An earlier version of this section gave the reason as
"Ghidra is not installed in this environment (`/opt/ghidra` … does not
exist)".  That is false, and it was committed here.  Ghidra *is* installed:

```
$ ls -l /opt/ghidra/support/analyzeHeadless
-rwxr-xr-x 1 root root 2388 Jun 29 17:18 /opt/ghidra/support/analyzeHeadless
$ command -v analyzeHeadless || echo "not on PATH"
not on PATH
```

It is simply not on `PATH`, which is what a `command -v` check reports and
what the wrong claim was inferred from; `tools/ghidra/run_ghidra.sh` invokes
it by absolute path and does not need it on `PATH`.  Task 11g ran it — that
is where `data/functions.json`'s `data_xrefs` field came from.  A second
sentence in the task report, that the Python consumption path "is tested",
was false too when it was written: `tools/test_re_query.py` gained no test in
Task 11h.  It has them now (`TestResolveReportsTheRuntimeName`, 8 tests), and
one of them found a real defect — see "One thing the assertion found" below.

**The decision stands; the reasons are these.**

1. The names must not become **renames**.  Every citation in `docs/re/` and in
   `src/` refers to a routine by its `FUN_*` name, and `docs/re/functions.md`
   documents the export by those names.  A rename invalidates all of them at
   once.
2. Wiring it means regenerating `data/functions.json`, and Task 11g's
   guarantee — two consecutive `ExportAll` runs are byte-identical (`cmp`
   exits 0) — would have to be re-established for the changed generator, on
   an artifact this task was told to leave untouched.
3. The side table is a **different kind of claim** from the export.  The
   export is what Ghidra asserts; `data/rtl_names.json` carries a
   `name_kind`, a tier and per-routine evidence, and 72 of its 104 names are
   coined here rather than read from the library.  Flattening that into a
   name field in a build artifact loses exactly the provenance that keeps a
   coined name from being cited as a Borland symbol.

Wiring it later is still a small job, and (1) is the constraint on how: read
`data/rtl_names.json`, key it by the `entry` string, and emit an extra
`"rtl_name"` field **beside** `name`, never in place of it — then re-run the
byte-identical check.

### One thing the assertion found: `size` is an address count, and one body is split

**The export is correct here.**  An earlier version of this section called
`1f78:1117`'s 22 bytes a Ghidra defect.  It is not one, and this repository
already said so before Task 11h began.  **`docs/re/branches.md`** states the
rule under "`size` on a function record is `getNumAddresses()`, a set
cardinality, not a span" (lines 248-258), and the note in its audit script
(lines 612-628) names `1f78:1117` as the image's sole OVERLAPPING record,
names `1f78:1121`..`112c` as its phantom range, names `1000:0d14` as the
second non-contiguity and `1000:11c0`/`11c1` as the two addresses it
under-reads past, and ends *"Do not copy this line
into a check where the over-read would matter — use the address set, not the
span."*  That comment is
the authority for what follows; this section only records what the span model
does in `tools/re_query.py` and what was confirmed by disassembly.  (Until the
final-review fix wave the note said "the sole split body in the image", which
this section had already corrected below without the note following; both now
say the same thing.)

`data/functions.json` gives each function an `entry` and a `size`, and `size`
is Ghidra's **address-set byte count**.  `re_query.Program.__init__` has no
address set to read, so it builds a span `[entry, entry + size)` — a **span
approximation**, exact whenever the body is contiguous and wrong in two
directions when it is not.

Disassembled from `orig/g.exe` (`0f78:1117` → image `0x10897`, file `0x12167`;
`capstone`, `CS_MODE_16`):

```
0f78:1117  0a c9        or cl, cl        \
0f78:1119  74 2a        je 0x1145        |
0f78:111b  e8 96 fe     call 0xfb4       |  head, 10 bytes
0f78:111e  72 1f        jb 0x113f        |
0f78:1120  cb           retf             /
0f78:1121  e8 07 ff     call 0x102b         <- FUN_1f78_1121, not this body
0f78:1124  cb           retf
0f78:1125  e8 2d ff     call 0x1055         <- FUN_1f78_1125, not this body
0f78:1128  cb           retf
0f78:1129  b5 00        mov ch, 0           <- not exported at all
0f78:112b  e8 63 ff     call 0x1091
0f78:112e  72 09        jb 0x1139
0f78:1130  cb           retf
0f78:1131  b5 01        mov ch, 1           <- FUN_1f78_1131
0f78:1133  e8 5b ff     call 0x1091
0f78:1136  72 01        jb 0x1139
0f78:1138  cb           retf
0f78:1139  b8 cf 00     mov ax, 0xcf        <- error tail, reached by both `jb`s
0f78:113c  e9 d0 ef     jmp 0x10f
0f78:113f  b8 cd 00     mov ax, 0xcd     \  out-of-line error tail, 6 bytes
0f78:1142  e9 ca ef     jmp 0x10f        /   (`jb` target of 1102/1108/1114/111e)
0f78:1145  b8 c8 00     mov ax, 0xc8     \  out-of-line error tail, 6 bytes
0f78:1148  e9 c4 ef     jmp 0x10f        /   (`je` target of 0f78:1119 alone)
```

`1145` is `0f78:1117`'s alone.  `113f` is a **shared** tail: the `jb` of all
four real-op thunks — `0f78:10ff` (`72 3b`), `1105` (`72 35`), `1111`
(`72 29`) and `1117` (`72 1f`) — jumps to it.  A shared tail can be counted
into only one address set, and Ghidra charged it to `1117`; the other three
are recorded as 6 bytes each, head only.  `1139`..`113e` is charged to `1131`
for the same reason, though there the two pieces happen to be adjacent.

10 + 6 + 6 = **22**, exactly what the export records, and `113f`..`114a` is
exactly the 12-byte hole the export's spans leave between `1f78:1131`'s end
and the next entry `1f78:114b`.  (Two smaller holes in this run, `110b`..`1110`
and `112d`..`1130`, are the interiors of routines the export does not carry an
entry for at all, so they are a different thing.)
`1f78:1131`'s 14 is 8 + 6 the same way — but there the two pieces are
**adjacent** (`1131`..`1138` then `1139`..`113e`), so its span and its address
set are the same bytes.

**How many records this affects, measured over all 123.**  Decode forward from
each entry for exactly `size` bytes and see whether the instructions tile onto
the recorded end.  121 of 123 do.  The two that do not are:

| record | `size` | what decoding says | direction |
|---|---:|---|---|
| `1f78:1117` | 22 | 22 bytes forward overshoots the end by 1 — the window is not a body | over-reads |
| `1000:0d14` | 1196 | its `ret 0x2` ends at `1000:11c1`, and the next entry is `1000:11c2`, so the contiguous body is **1198** bytes | under-reads by 2 |

`1f78:1117` is the only pair of overlapping spans in the file.  `1000:0d14` is
a **second** non-contiguity, in the game's own code rather than the runtime,
and it was not known before this round: two of its addresses are missing from
Ghidra's set somewhere inside `1000:0d14`..`11c1`, and the artifact emits no
body ranges to say which.  The live consequence is the under-read direction
again — `resolve 1000:11c0` and `1000:11c1` report `function: None` for the
two operand bytes of that `ret 0x2`.  Naming it is all this task does with it;
explaining it needs Ghidra, not this document.

`branches.md` used to call `1f78:1117` "the sole split body in the image",
which was one record too narrow read literally; both that paragraph and the
audit-script note now say **overlapping** instead, and both name `1000:0d14`.
The sentence's own use — the strings-overlap count — is unaffected either way,
because `1000:0d14`'s two lost addresses are `1000:11c0`/`11c1`, the operand
bytes of the `c2 02 00` (`ret 0x2`) at `1000:11bf`, and no
`data/strings.json` entry touches them (checked: 0).

**The span model's two failure directions**, both live and both now asserted in
`tools/test_re_query.py`:

1. **It over-reads.**  `[0x10897, 0x108ad)` is `0f78:1117`..`112c`, so
   `resolve 0f78:1129` and `resolve 0f78:112b` report `FUN_1f78_1117` /
   `rtl_real_op_div` for bytes that belong to no exported function at all
   (`0f78:1129` is not in the export).  For `0f78:1121` and `0f78:1125`, which
   *are* exported — and the second of which `docs/re/tables.md:346` cites by
   name — the innermost-range rule fixes it: `function_containing` now returns the
   tightest containing range instead of the first in file order, so those two
   resolve to themselves.  The rule is right whenever extents nest; it is not
   justified by any defect in the export.
2. **It under-reads.**  `0f78:113f`..`114a` **is** `FUN_1f78_1117`'s body and
   the span does not contain it, so `resolve` reports `function: None` there
   and falls back to the back-sweep anchor.  Nothing in the span model can
   recover those bytes.  `1000:11c0`..`11c1` is the same direction on
   `1000:0d14` (see the census above).

`tools/test_re_query.py` asserts the census (exactly those two records, by
citation) and both directions on both records, so a third non-contiguous
record — or a change that silently repairs one direction — fails a test.

`data/functions.json` was not edited, and not because it is a build artifact
one hesitates to touch: it is **right**.  What is approximate is `_ranges`.
Modelling the address set properly would mean reconstructing each function's
partition from flow — new inference, presented in the shape of exported data —
and the export carries no address set to check it against.  So the
approximation stays, named as one, with both of its failure directions pinned
by tests rather than left to be rediscovered.
