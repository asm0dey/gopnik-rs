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

- Walking rolls `Random(25)+1` at `1000:b358`, then buckets the result
  (`cmp byte[0x3971],0x0a`, …). The bucket boundaries ARE the probabilities.
- The bucket-2 branch (`1000:b4e8`–`1000:b575`) ends in `mov byte [0x3697],1`
  after its own further roll — that is a discovery flag being set.
- `mar`'s gate reads `cmp byte [0x3694],1` at `1000:b94f` — that is the flag
  being consumed.

Read those constants and the probability table falls out exactly, per location.
No sampling, no inference. A live trace complements this by enumerating which
`Random` sites actually execute and with what `n`, but the *distribution* comes
from the comparison constants, not from counting outcomes.

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
