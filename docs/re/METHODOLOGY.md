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
There the code is Borland's `Random(n)` — eleven instructions, 29 bytes, ending
in a far return that pops the one argument the call pushed:

```
0f78:114b  e8 5a 00        call 0f78:11a8      ; the LCG on RandSeed, DGROUP 0x367e
0f78:114e  8b dc           mov bx,sp
0f78:1150  8b ca           mov cx,dx
0f78:1152  36 f7 67 04     mul word [ss:bx+4]  ; low half of the 32x16 multiply
0f78:1156  8b c1           mov ax,cx
0f78:1158  8b ca           mov cx,dx
0f78:115a  36 f7 67 04     mul word [ss:bx+4]  ; high half -- the elided one
0f78:115e  03 c1           add ax,cx
0f78:1160  83 d2 00        adc dx,0
0f78:1163  8b c2           mov ax,dx           ; the high word IS the result
0f78:1165  ca 02 00        retf 2
```

Earlier versions of this passage quoted five of those byte groups. Nothing in
that list was wrong and every byte string was at the right address in order,
but the elision hid the **second** `36 f7 67 04`: `Random` does a 32x16
widening multiply of the whole `RandSeed` longint, not a 16x16 one. The full
listing is above so the abbreviation cannot be read as the whole routine.

## How to check this mechanically

The convention above is the human-readable authority; **`tools/addr.py` is its
executable form**, and it is the only place the arithmetic is written in
Python. Each form is a separate function that rejects the other form's segment
range, so the 64 KiB mistake raises instead of returning a plausible number,
and `addr.citation()` picks the form from the segment so a caller never gets to
choose wrongly. Import it rather than recomputing `0x18d0` — which is itself
derived there from the MZ header's `e_cparhdr`, not written down.

`tools/re_query.py` answers the four questions this project keeps hand-rolling.
Run the query instead of doing the sweep by eye; each subcommand prints
evidence a `docs/re/` claim can quote directly. The outputs below were produced
by running exactly these commands.

**Resolve a citation, in either form.**

```
$ python3 tools/re_query.py resolve 0f78:114b -n 5 -i 1
citation: 0f78:114b
form: runtime
seg: 0f78
off: 114b
ghidra_label: 1f78:114b
image_off: 0x108cb
file_off: 0x1219b
bytes: e8 5a 00 8b dc
function: FUN_1f78_114b
instructions:
  at: 0f78:114b
  image_off: 0x108cb
  file_off: 0x1219b
  bytes: e8 5a 00
  text: call 0x10928
```

(`call 0x10928` is an IMAGE offset — `0f78:11a8` — because the decoder renders
branch targets as offsets in the buffer it was handed. Drop `-n`/`-i` for the
default 16 bytes and 4 instructions.)

**Is this address a call site?** Alignment and identity are separate signals
and only identity decides. `1000:d83b` is the standing counter-example: it
passes the alignment sweep and is still the wrong address.

```
$ python3 tools/re_query.py is-call-site 1000:d83b
citation: 1000:d83b
image_off: 0xd83b
file_off: 0xf10b
function: entry
signature: 9a 4b 11 78 0f
identity:
  match: False
  bytes_here: b8 06 00 50 9a
  nearest_signature_deltas:
    - 4
alignment:
  sweep_votes: 63
  sweep_tried: 64
  anchored_from_function_entry: True
  first_misses:
    - (53, 'decode failed before reaching the target')
verdict: NOT a call site
note: Alignment alone never answers yes: 1000:d83b scores all but one of the same sweeps and is still the wrong address -- a real instruction boundary four bytes before the call it was mistaken for.  Only `identity.match` settles it.
```

`b8 06 00 50` is `mov ax,6` / `push ax` — the argument idiom. The call is four
bytes later, at `1000:d83f`.

**What `n` does a draw site push?** The walk-back reproduces all 17 draw sites
`data/wander.json` records by hand, byte for byte.

```
$ python3 tools/re_query.py pushed-n 1000:b2fa --json | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['n_at'], '|', d['n_bytes'], '->', d['n_expr'] or d['n'])"
1000:b2ef | a0 92 36 30 e4 ba 14 00 f7 e2 50 -> byte[0x3692] * 20
```

**Who references a data address?** A raw byte scan cannot tell an operand from
two adjacent instructions that happen to spell the same word, so hits are kept
only when they land on an operand FIELD of an aligned instruction, and the
discards are printed with their reason.

```
$ python3 tools/re_query.py xrefs-to 20ae:3b74 --json | python3 -c "import json,sys; s=json.load(sys.stdin)['scan']; print(s['raw_hits'],'raw,',len(s['accepted']),'accepted,',len(s['discarded']),'discarded'); [print(' discarded',x['image_off'],x['why']) for x in s['discarded']]"
7 raw, 6 accepted, 1 discarded
 discarded 0xc358 the word straddles `jl 0xc3cd` (7c 74) and the instruction after it -- it is not one field
```

Tests: `python3 tools/test_addr.py` and `python3 tools/test_re_query.py`.

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
