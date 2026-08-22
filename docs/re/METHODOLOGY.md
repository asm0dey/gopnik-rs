# Methodology: recover program FLOW, not program OUTPUT

The question is never "what did the game print?" It is "what does the code do,
and under what conditions?" Output is a consequence of flow. Reasoning from
output back to flow is guessing, and this project has shipped that guess more
than once.

## Evidence hierarchy

Strongest to weakest. A claim is only as strong as the best evidence actually
gathered for it — not the best evidence available in principle.

1. **Flow** — the instructions, their branches, and their conditions.
   Static: the disassembly. Live: breakpoints in `tools/qemu` (gdb attached to
   the guest, which reports IP as the Ghidra offset). Only this establishes
   *why* something happens, and only this can prove a negative.
2. **State** — values in guest memory, save bytes, extracted tables. Says what
   a variable held, never why it changed.
3. **Output** — screens, printed strings, the in-game help text. The weakest
   evidence there is: a claim the program makes *about itself*. It can be
   stale, incomplete, or simply wrong relative to the code that runs.

Output can FALSIFY a flow claim. Output can never ESTABLISH one.

## The rule that follows

Absence of visible response is not absence of dispatch. When `mar` typed at the
`Битва\` prompt produces nothing on screen, the established fact is "nothing was
printed" — not "the verb is not dispatched". The branch may be taken and do
nothing observable. Only the dispatcher, or a breakpoint that does not fire,
settles it.

Symmetry is not evidence either. "Shops are probably modal because combat is
modal" is a hypothesis to test with a breakpoint on the shop input read, not a
finding to record.

## Address convention, and its range of validity

Two conventions are in use across `docs/re/`, `src/` and the task briefs, and
they are **not the same arithmetic**. Mixing them is a 64 KiB error, not the
usual two-to-five-byte drift, and it has already produced one wrong
adjudication on this project.

`orig/g.exe` is an MZ image whose `e_cparhdr` is `0x18d` paragraphs, so the
header is `0x18d0` bytes and the load image begins at file offset `0x18d0`.
Every segment value *stored in the file* — in a far-call operand and in the
relocation table — is **relative to the load base**, because the loader adds
the base to it at load time. The image has exactly four relative segments,
which is also exactly the set of segments named in its 1580 relocation
entries: `0x0000` (game code), `0x0eed`, `0x0f16`, `0x0f78` (runtime).

**Form A — a Ghidra label `SEG:OFF`.** Ghidra loads at segment `0x1000`, so
`relseg = SEG - 0x1000`:

```
file_off = 0x18d0 + (SEG - 0x1000) * 16 + OFF        # valid only for SEG >= 0x1000
```

`1000:b353` → `0x18d0 + 0 + 0xb353` = `0xcc23`, which holds
`9a 4b 11 78 0f`. `20ae:xxxx` is DGROUP (`relseg 0x10ae`), which the runtime
sets itself: `0f78:0000` is `ba ae 10` / `8e da` = `mov dx,0x10ae` /
`mov ds,dx`.

**Form B — a real runtime `seg:off`, i.e. what `ndisasm` prints for a far-call
operand.** The operand already *is* the relative segment, so `relseg = SEG`
and there is no `- 0x1000`:

```
file_off = 0x18d0 + SEG * 16 + OFF                   # the SEG < 0x1000 runtime segments
```

Anything written `0eed:`, `0f16:` or `0f78:` in this repo is Form B. The
Ghidra label for the same address is `SEG + 0x1000`, and Form A then agrees:
`0f78:114b` and `1f78:114b` are the same address.

**Do not apply Form A to a Form B address.** Worked, on the address that was
adjudicated wrongly:

```
0f78:114b, Form B (correct):
    0x18d0 + 0x0f78*16 + 0x114b = 0x18d0 + 0xf780 + 0x114b = 0x1219b
0f78:114b, Form A misapplied:
    0x18d0 + (0x0f78 - 0x1000)*16 + 0x114b = 0x18d0 + (-0x880) + 0x114b = 0x219b
difference = 0x1000 * 16 = 0x10000 = 64 KiB
```

`0x1219b` is `Random`; `0x219b` is not even an instruction boundary — it is
the interior of `1000:08ca` `8d be fe fe` (`lea di,[bp-0x102]`), in the middle
of a screen-drawing loop in the game's own code segment. The 64 KiB warning
attached to the `- 0x1000` term is real, but it points the *other* way: it is
Form A addresses that overshoot when the term is dropped.

**Established from flow.** All 86 `9a 4b 11 78 0f` far calls carry their
segment word in the relocation table, so `0f78` is a relative segment awaiting
a fixup, and following the call lands on `relseg 0x0f78`, offset `0x114b`.
There the code is `e8 5a 00` (`call 0f78:11a8`, the LCG on `RandSeed` at
DGROUP `0x367e`), `8b dc` (`mov bx,sp`), `36 f7 67 04` (`mul word [ss:bx+4]`,
the pushed argument), `8b c2` (`mov ax,dx`, the high word) and `ca 02 00`
(`retf 2`) — Borland's `Random(n)`, with a far return that pops the one
argument the call pushed.

## Worked example: where do discovery probabilities live?

"What is the chance of finding the club, or the gym?" cannot be answered by
playing and counting — that measures one seed's luck, and the sample needed for
a stable estimate is enormous. It is answered by reading flow:

- Walking calls `Random` at `1000:b353` (`9a 4b 11 78 0f`); `1000:b358` is the
  `inc ax` / `mov [0x3971],al` that stores the result. It is then bucketed by
  `cmp byte [0x3971],0x0a`, then `09`, then `05`. **The bucket boundaries ARE
  the probabilities.**
- Bucket 2 (`1000:b4e8` = `cmp al,2`) runs its own further `Random(2)` and, on
  zero, reaches `1000:b570` = `c6 06 97 36 01` — `mov byte [0x3697],1`, the
  GIRL's discovery flag. (`1000:b575` is the `jmp` after it, not the setter.)
- `mar`'s gate reads `cmp byte [0x3694],1` at `1000:b954`; `1000:b94f` is the
  `jz` that precedes it. That is a flag being consumed.
- Each location has its own flag, contiguous at `20ae:3694..369a` — the den is
  `0x3696` (gate `1000:d80c`), the girl `0x3697` (gate `1000:d6f7`). Naming the
  wrong flag is as wrong as naming the wrong address.

Read those constants and the probability table falls out exactly, per location.
No sampling, no inference. A live trace complements this by enumerating which
`Random` sites actually execute and with what `n`, but the *distribution* comes
from the comparison constants, not from counting outcomes.

## This document has already broken its own rule

The first version of the worked example above cited `1000:b358` for a `Random`
call that is five bytes earlier, and `1000:b94f` for a `cmp` that is five bytes
later; a reviewer re-derived both from `orig/g.exe` and caught them. Citing an
address is not the same as citing the RIGHT address, and a near-miss reads as
authoritative to everyone downstream. Re-derive before you write it down.

## What this rules out

Each of these has actually happened here:

- **Trusting a table of "reference facts" with no code behind it.** The plan's
  table asserted the RNG multiplier was absent (false — the compiler synthesises
  it from shifts) and that `sv` meant save (false — it sizes up the enemy).
- **Treating the in-game help text as the command table.** It is a string in the
  binary, not the dispatcher.
- **Recording an inference as a finding** (shop modality "by symmetry").
- **Inventing a mechanic to fill a gap** — Task 11 made failed entry to a
  location mark it discovered, so the gate became a one-shot message. Nothing in
  the original does that; the real flag is set by a wander event.
- **Evidence that proves less than it claims** — a completeness check printing
  `14/14` by formatting one value against itself.

## Standard of report

Every behavioural claim states which tier it rests on and cites an address:
**established from flow** (address), **corroborated by state/output** (what was
observed), or **unverified** (and what would settle it). There is no fourth
category, and "it looked right when I played it" is not one of the three.
