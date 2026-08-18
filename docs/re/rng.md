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
(the Borland `System` unit). File offsets below assume the standard MZ
mapping `file = 0x18D0 + (seg - 0x1000) * 16 + off` (header is 397
paragraphs; Ghidra loaded the image at segment `0x1000`).

| Ghidra address | Ghidra name | File offset | Pascal identity |
|---|---|---|---|
| `1f78:11a8` | `FUN_1f78_11a8` | `0x122b8` | `System.@Rand` — steps `RandSeed` |
| `1f78:114b` | `FUN_1f78_114b` | `0x1225b` | `System.Random(Range: Word): Word` |
| `1f78:1168` | (in `FUN_1f78_114b`'s block) | `0x12278` | `System.Random: Real` (unused by the game) |
| `1f78:11e0` | `FUN_1f78_11e0` | `0x122f0` | `System.Randomize` |
| `1f78:11de` | — | `0x122ee` | the literal word `$8405` (the multiplier's low half) |
| `20ae:367e` | — | `0x15a2e` | `RandSeed: LongInt`, four bytes, `$00000000` in the load image |

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
routine's main input loop. Everywhere else the game runs on whatever
`RandSeed` holds; the load image ships it as `$00000000`, which is why the
vectors below use seed `0`.

Reproducing a `Randomize` seed is a port-level policy decision, not an RE
one, and is deliberately left out of `src/rng.rs`: `Rng::new` takes the seed
explicitly.

## How the game uses it

`Random` (`1f78:114b`) has 5 direct callers — `entry`, `FUN_1000_0d14`,
`FUN_1000_2526`, `FUN_1000_3d11`, `FUN_1000_7c67` — and roughly 130 call
sites across them, so the generator is pervasive rather than incidental.
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

The original was never executed under DOSBox-X for this task. The oracle
harness from Task 3 could in principle confirm the sequence end-to-end
through observable damage rolls, but that requires pinning the `Randomize`
seed and decoding printed numbers back to RNG draws, and the static recovery
is unambiguous enough not to need it. If a future task wants a second,
fully independent confirmation, that is the route: patch a copy of the
binary in the oracle workdir to skip the `call` at `1000:6a0d` that reaches
`Randomize` (leaving `RandSeed` at `$00000000`), then compare printed rolls
against `data/rng_vectors.json`.
