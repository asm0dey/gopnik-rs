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
