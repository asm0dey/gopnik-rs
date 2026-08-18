# The live `Random` tracer, and what it says about the wander catalogue (Task 11d)

Harness: `tools/rngtrace/`. Tests: `tools/test_rngtrace.py`. Machine-readable
result: `data/rng_trace.json`. This document changes no Rust.

`docs/re/wander.md` catalogues eighteen `Random` draws a wander turn can spend,
every one of them **established from flow** out of the disassembly and none of
them ever observed. Two things were about to be built on that catalogue — the
port's wander implementation and the differential test meant to validate it —
so an error in it would land in both at once and look exactly like a port bug.
This task built the instrument that checks the catalogue against the running
original, and used it.

Per `docs/re/METHODOLOGY.md`, a live breakpoint is **flow** evidence, the same
tier as the disassembly, and it is the only kind that can prove a negative: a
breakpoint that does not fire where the reading says it should is a real
contradiction, not an absence of output. Every claim below states its tier.

## What the tracer does

One non-interactive command:

```bash
python3 tools/rngtrace/run.py --boot-img <freedos.img> --walks 30 \
    --class-answer 0 --seed 0x12345678 --out build/rngtrace/traceA.json
```

It boots FreeDOS under qemu with `orig/g.exe` (a patched COPY — see below) on a
vvfat drive, attaches gdb to the guest in real mode, derives the load base,
verifies the bytes at the breakpoint address, installs the breakpoints, drives
the game through character creation and N walks from the qemu monitor, and
writes the trace as JSON. The VM is killed on every exit path, including
exceptions. Python standard library only.

The FreeDOS boot floppy is not committed (it is a 1.4 MB third-party image);
`--boot-img` points at one. FreeDOS 1.3 "FloppyEdition" was used here.

## Step 1 — pinning the seed

`Randomize` seeds `RandSeed` from `INT 21h/AH=2Ch` on every run
(`docs/re/rng.md`), so two runs of the original draw different numbers and
nothing is comparable. `tools/rngtrace/seedpatch.py` rewrites those 13 bytes in
a **copy** of the binary in the harness workdir. `orig/g.exe` is never written;
the patched copy is never committed. The harness refuses to run if the source
md5 or the bytes at the site are not what it expects.

| | |
|---|---|
| site | `1f78:11e0` (Ghidra) = `0f78:11e0` at runtime = **file offset `0x12230`** |
| before | `b4 2c cd 21 89 0e 7e 36 89 16 80 36 cb` |
| after (seed `0x12345678`) | `c7 06 7e 36 78 56 c7 06 80 36 34 12 cb` |
| length | 13 bytes → 13 bytes |

```
    before                                  after
    b4 2c        mov ah,0x2c                c7 06 7e 36 LL LL  mov word [0x367e],lo
    cd 21        int 0x21                   c7 06 80 36 HH HH  mov word [0x3680],hi
    89 0e 7e 36  mov [0x367e],cx            cb                 retf
    89 16 80 36  mov [0x3680],dx
    cb           retf
```

Same entry, same length, same far return, same two destination words, so no
other address moves. The patch is verified a second time **in guest memory**
before any breakpoint is set.

## Step 2 — finding `Random` in the guest, and proving it is `Random`

Two address conventions, which must not be mixed:

* a Ghidra citation `1000:XXXX` → image offset `XXXX`, file offset `0x18d0 + XXXX`;
* a real runtime `seg:off` such as `0f78:114b` → image offset `seg*16 + off`,
  file offset `0x18d0 + seg*16 + off`.

`0f78:114b` → image `0x108cb` → **file `0x1219b`**, re-derived here rather than
taken from the Task 11b review. Disassembling `orig/g.exe` at that offset
(`ndisasm -b16`) gives, verbatim:

```
0000114B  E85A00            call 0x11a8            ; @Rand: step RandSeed
0000114E  8BDC              mov bx,sp
00001150  8BCA              mov cx,dx
00001152  36F76704          mul word [ss:bx+0x4]
00001156  8BC1              mov ax,cx
00001158  8BCA              mov cx,dx
0000115A  36F76704          mul word [ss:bx+0x4]
0000115E  03C1              add ax,cx
00001160  83D200            adc dx,0x0
00001163  8BC2              mov ax,dx
00001165  CA0200            retf word 0x2
```

That is the 32×16 high-take with a `retf 2` tail — `Random(Word)`, exactly as
`docs/re/rng.md` records it. **Established from flow.**

**The load base is derived every run and never hardcoded** (`loadbase.py`). The
prototype's `0x224B0` is one run's value; DOS picks the load segment. The
derivation finds a relocation-free 64-byte window of the loaded image in a
physical-memory dump, then *verifies* the candidate by checking **every** MZ
relocation below the data segment against it: memory word must equal file word
plus load segment. All 1580 relocations must agree, and exactly one candidate
base must survive. That is a much stronger check than matching one banner
string, and it fails loudly rather than picking a near-miss.

Before installing a breakpoint the harness then re-reads the guest's memory at
the derived address and requires:

1. the 29 bytes at `image_base + 0x108cb` to equal the file's bytes at
   `0x1219b`, and to contain the two `36 f7 67 04` (`mul word [ss:bx+4]`)
   encodings and the `ca 02 00` tail;
2. the seed patch to be present at `image_base + 0x10960`;
3. `RandSeed` at `image_base + 0x1415e` (`20ae:367e`) to read either
   `0x00000000` (the image value — the patched `Randomize` has not run yet) or
   exactly the pinned seed (it has run, no draw spent).

Check 3 is what proves the tracer is attached **before the first draw**. In
every run recorded here it read `0x00000000`.

## Step 3 — the observation point, and two gdb landmines

The breakpoint is on the **`retf 2` at `0f78:1165`**, not on the entry. There
the callee has restored `SP` to its entry value, so the caller's frame is still
intact *and* `ax` already holds the result: one stop yields call site, `n` and
result together, with no entry/exit pairing to get wrong.

```
[sp]   = return offset   -> the call site is that minus 5 (a far call is 5 bytes)
[sp+2] = return segment
[sp+4] = the pushed n
ax     = the result
```

Every catalogued caller is in segment `1000`, whose runtime segment IS the load
segment, so the return offset is the Ghidra offset and the log reads directly
against `docs/re/`. A second breakpoint on `1000:ae63` — the top-level prompt's
`ReadLn` call, bytes `9a c6 06 78 0f` — marks turn boundaries, so the stream is
segmented by **flow**, not by wall-clock timing.

Two things the prototype in `tools/qemu/` got wrong, both found by running it,
both fixed here, and both failing in the same direction — a plausible SHORT
trace:

1. **Breakpoint `commands` blocks never run.** qemu's i386 gdbstub reports
   `$pc` as the raw 16-bit `eip` while the breakpoint is at the LINEAR address
   (`cs_base + eip`), so gdb cannot attribute the stop, reports a bare
   `SIGTRAP`, and skips the block. The tracer dispatches on `$pc` itself from
   an explicit gdb `while` loop.
2. **Resuming re-traps forever.** For the same reason gdb never performs its
   remove/single-step/reinsert dance, so qemu stops again on the same
   instruction. Measured: 833654 bytes of stops, every one at `$pc = ae63`,
   with the guest making no progress — and its screen unchanged, so the
   screen-driven driver kept typing into a frozen game. `hbreak` behaves
   identically. The loop steps over by hand: `disable`, `stepi`, `enable`.

## The guards — why a short trace cannot be published as evidence

A tracer that logs three draws when nine happened is worse than no tracer, so
the run fails loudly instead:

* **installed** — the log must show both breakpoints accepted and the harness's
  own `READY` line, or the run errors.
* **non-empty** — zero draws is an error exit, never an empty file. Any stop at
  an unexpected `$pc` is an error too.
* **alive** — gdb must still be running at the end of the drive, and the log
  must have grown while walking.
* **LCG replay (flow-tier completeness)** — with the seed pinned, the whole
  stream is predictable: the k-th logged draw must equal
  `(step^k(seed) * n) >> 32`. A MISSED draw desynchronises the LCG and every
  later prediction fails. This works because `Random` is the sole runtime path
  into `@Rand`: `orig/g.exe` contains **86** far calls to `0f78:114b`, **0** to
  `0f78:11a8` and **0** to `0f78:1168`. The only other `@Rand` caller,
  `0f78:1168` (the Real-valued `Random`), is itself a *near* caller of `@Rand`
  — but it is never far called, so nothing reaches it at runtime, and no other
  near call reaches either from outside `Random` (checked by decoding every
  `e8` in the segment).
* **final `RandSeed` (state-tier completeness)** — at the end of the run the
  guest's own `RandSeed` is read back and must equal the seed stepped once per
  logged draw. An unlogged draw would leave the guest ahead of the replay.

Both completeness checks passed on every run below, with zero leading states
skipped.

### The guard the first five missed (fix wave 1)

A gdb command error inside the `while 1` loop aborts the sourced script and
drops gdb to its prompt **with the guest stopped at a breakpoint**. Every guard
above passes there, and this is the point: gdb is alive; the log grew, because
gdb's own error text grew it; the frozen screen still classifies as the street
prompt, so `driver.walk` types `w` the requested number of times and returns
normally, inside budget; the truncated prefix replays against the LCG perfectly,
because a prefix is self-consistent; and the stopped guest spends no further
draws, so its final `RandSeed` still equals the replay of the logged prefix. The
result is a trace with the first 40 of 393 draws that exits 0 and reads as
evidence the other 353 did not happen — the third disguise of the one failure
this harness exists to prevent.

Three more guards, each with a unit test that fails without it:

* **walk** (flow tier, and the decisive one) — every `w` typed at the street
  prompt must produce a stop at the top-level `ReadLn` (`1000:ae63`), so
  `prompt_stops >= walks` or the run errors. A guest that stopped progressing
  cannot produce them, however healthy its screen looked. All five runs below
  recorded `prompt_stops == walks + 1` (the prompt before the first `w`, then
  one per completed turn).
* **abort message** (log tier) — the harness's own shutdown aborts the script
  too (it kills the VM first, on purpose), so the abort is not the signal, its
  message is: the deliberate one always reads `Remote connection closed`. Any
  other message, more than one abort, or any event logged after one, is an
  error. All five committed logs carry exactly one abort, with that message, as
  their last event.
* **progress** (state tier) — the screen or `RandSeed` must differ between the
  start and the end of the drive. A guest frozen from the first keystroke moves
  neither.

Two more from the same pass: no log line may be dropped silently
(`printf "? %04x", $pc` pads to four digits but does not truncate, so a `$pc`
above `0xffff` prints five and a four-digit-only pattern would have made an
unexpected stop VANISH instead of tripping the non-empty guard), and every
draw's return segment must equal the load segment.

The five runs below predate these guards, so their `verification` blocks in
`data/rng_trace.json` record what each run asserted **at the time** — the
original five. The new guards were replayed afterwards against each run's own
committed gdb log and all five pass: `prompt_stops` is `walks + 1` in every one,
each log carries exactly one script abort with the deliberate message as its
last event, every draw returned into segment `224b`, and no log line failed to
parse. Nothing about the runs was re-executed to establish that; the logs are
the same bytes the runs wrote.

`tracelog.verify_run` holds all of them behind keyword-only parameters, so
omitting one at the call site is a `TypeError` rather than a quietly weaker run,
and the two strongest checks — the pre-breakpoint guest verification and the
final-`RandSeed` reconciliation — are pure functions
(`loadbase.verify_guest_code`, `tracelog.reconcile_final_randseed`) with tests
that drive them with synthetic memory: wrong bytes at the breakpoint, a wrong
base, the patch absent, `RandSeed` already stepped, and a final seed off by one
step in either direction.

## The runs

Five runs, 1387 draws, all of them replay-verified. The seed is arbitrary —
choice of seed is meaningless for an LCG — but two of the five were **chosen by
searching seeds** so that a rare gate would be satisfied: the church fires on
`Random(200) == 0`, roughly once in 200 turns, and waiting for it by walking
would have been luck. The search predicts only *whether the gate opens*; what
the code then does is observed, so a wrong catalogue could not manufacture a
corroboration, it could only fail to open the gate.

| run | seed | character | walks | draws | what it is for |
|---|---|---|---|---|---|
| A | `0x12345678` | fresh Подтсан (class 3), district 1 | 30 | 393 | the plain case |
| B | `0x12345678` | fresh Вор (class 6), district 1 | 25 | 325 | draws 10/11, gated on class 6 |
| C | `0x4e1` | fresh Подтсан, district 1 | 3 | 30 | church with `Random(5) == 0` → draws 15, 17, 18 |
| D | `0x27c` | fresh Подтсан, district 1 | 3 | 29 | church with `Random(5) == 1` → draws 15, 16 |
| E | `0x12345678` | `SAVE_R3.SAV` loaded: Вор, level 20, **district 3**, phone and ring | 25 | 610 | draws 3, 4, 9, and the computed `n` at a second district |

Run E loads a save from the shipped corpus rather than creating a character:
`orig/SAVE_R3.SAV` carries `has_mobile` and the ring, which are what gate draws
3, 4 and 9, and it sits in district 3, which is what the computed `n` of draws
10 and 11 multiplies. Nothing is patched to arrange that — the `.SAV` files are
copied into the game directory and the district prompt loads one.

Every run's class, district, luck and item flags are read out of the guest's own
data segment at the end of the run (`final_state` in `data/rng_trace.json`), so
the gate operands are recorded rather than assumed. **The class in each run
record is that guest read (`DS:389c`), never the harness's `--class-answer`:**
the answer is what the driver typed at the creation menu, and run E never
reached that menu. The first version of `data/rng_trace.json` echoed the CLI
default there and stated `class_value: 3` / `Пацан` for run E, which loaded a
class-6 Вор — a wrong field about the original, corrected in fix wave 1. On a
run that *did* create a character the two must agree, and the harness now errors
if they do not (that is the drift that once put the class answer into the NAME
prompt).

## The comparison, draw by draw

**All eighteen catalogued draws were observed, at the catalogued call site,
with the catalogued `n`. There are no contradictions.**

| # | site | catalogued `n` | observed | verdict |
|---|---|---|---|---|
| 1 | `1000:af68` | 20 | 69× `n=20` | corroborated |
| 2 | `1000:afc7` | 20 | 46× `n=20` | corroborated |
| 3 | `1000:b030` | 200 | 25× `n=200` (run E only) | corroborated |
| 4 | `1000:b0dc` | 100 | 25× `n=100` (run E only) | corroborated |
| 5 | `1000:b186` | 10 | 86× `n=10` | corroborated |
| 6 | `1000:b1b8` | 10 | 86× `n=10` | corroborated |
| 7 | `1000:b1ea` | 100 | 86× `n=100` | corroborated |
| 8 | `1000:b21c` | 100 | 86× `n=100` | corroborated |
| 9 | `1000:b272` | 20 | 25× `n=20` (run E only) | corroborated |
| 10 | `1000:b2fa` | `chapter * 20` | 25× `n=20` at district 1, 25× **`n=60` at district 3** | corroborated |
| 11 | `1000:b321` | `chapter * 5` | 5× `n=5` at district 1, 11× **`n=15` at district 3** | corroborated |
| 12 | `1000:b353` | 25 | 86× `n=25` | corroborated |
| 13 | `1000:b39e` | 200 | 86× `n=200` | corroborated |
| 14 | `1000:b3ae` | 100 | 86× `n=100` | corroborated |
| 15 | `1000:7f63` | 5 | 2× `n=5` (runs C, D) | corroborated |
| 16 | `1000:7fff` | 4 | 1× `n=4` (run D) | corroborated |
| 17 | `1000:25fe` | Σ class growth weights | 2× `n=12` (run C) | corroborated |
| 18 | `1000:25fe` | same Σ | (the same two stops — see below) | corroborated |

Notes on the rows that need one:

* **Draws 10 and 11 are the strongest single result here.** The catalogue reads
  their `n` as *computed*, `chapter*20` and `chapter*5`. Runs B and E observed
  both at two different districts — `20`/`5` at district 1 and `60`/`15` at
  district 3 — with the district read out of the guest's `DS:3692`. A constant
  would not have moved.
* **Draws 17 and 18 share one call site** (`1000:25fe`, inside the level-up
  loop), so they cannot be told apart by address. What the trace shows is that
  the church's `Random(5) == 0` arm spends **exactly two** draws there, which is
  the loop bound at `1000:287d` the catalogue cites. Both entries in
  `data/rng_trace.json` and both `live_trace` blocks in `data/wander.json` now
  carry a `shared_call_site` field saying so: each reports `observed_count: 2`
  for the SAME two stops, and reading the artifacts alone they would otherwise
  total four independent observations. Both carried `n = 12`,
  and 12 is the sum of the four class growth weights for class 3
  (`3+3+3+3`, read out of `orig/g.exe` at `DS:(class*4+2)`) — the class the run
  actually held, read from `DS:389c`.
* **Draw 16's gate is exact.** Run D's church rolled `Random(5) = 1` and the
  `Random(4)` at `1000:7fff` followed; run C's rolled `0` and it did not — the
  two draws at `1000:25fe` did. That is the corrected gate from Task 11b
  (`1000:7ff3` `cmp ax,1`, not the `== 0` arm at `1000:7f68`) firing both ways.

## What the trace shows beyond "the sites are right"

**Order.** Turn signatures — the ordered list of call sites between two
top-level `ReadLn` stops — are the catalogue's order, not just its set. Run E's
most common turn is **thirteen** of the fourteen preamble draws in sequence
(`b321` absent, 14 turns); the row below it is the same turn with the Вор's
theft succeeding, all **fourteen**, 8 turns:

```
af68 afc7 b030 b0dc b186 b1b8 b1ea b21c b272 b2fa b353 b39e b3ae      (14x)  13 sites
af68 afc7 b030 b0dc b186 b1b8 b1ea b21c b272 b2fa b321 b353 b39e b3ae  (8x)  14 sites
```

Order is **asserted, not eyeballed**: `compare.check_order` requires each
turn's preamble draws to appear in catalogued ordinal order (1..14), each at
most once, and `data/rng_trace.json.order_check` records the result — 86 turns
checked across the five runs, 0 violations. Without it the tool matched on call
site and `n` alone, so a re-run whose order had drifted would still have read as
corroborated, which is exactly what Task 12 will rely on it to catch. The
church's draws 15..18 fire nested inside another routine and are outside the
check.

**Call sites are attributed by offset, and that needs one segment.** Every
return segment logged across every run was `224b` — the load segment — so every
one of these is a segment-`1000` offset and reads directly against `docs/re/`.
That is now asserted per run (`return_segment_equals_load_seg`) rather than only
summarised: a draw from another code segment whose offset collided with a
catalogued one would otherwise be reported as a corroboration. The risk is small
for this binary and here is why — all **86** far-call sites to `0f78:114b` lie at
image offsets `0xd26`..`0xe0b7`, so every one of them is inside the first 64 KiB
addressed by the load segment (re-derived here by scanning `orig/g.exe` for
`9a 4b 11 78 0f`) — but "small" is not "checked".

**"Nine draws per turn, falling to eight and then seven."** `docs/re/wander.md`
predicts exactly this for a fresh Подтсан with no phone and no ring, and run A
is exactly that shape:

```
af68 afc7 b186 b1b8 b1ea b21c b353 b39e b3ae   (3x)   nine  -- both one-shots pending
af68      b186 b1b8 b1ea b21c b353 b39e b3ae   (9x)   eight -- errand 2 has fired
          b186 b1b8 b1ea b21c b353 b39e b3ae  (12x)   seven -- both have fired
```

**The one-shots burn on the `0`, and only on the `0`.** Draw 1 fired on 16
consecutive turns of run A and its sixteenth result was `0`; it never fired
again. Draw 2 fired 5 times, its fifth result was `0`, and it never fired
again. That is `1000:af6d`/`1000:afcc`'s `or ax,ax / jnz` followed by the flag
store at `1000:af71`/`1000:afd0`, watched happening.

**The church cancels the turn.** Run C's church turn rolled `25 → 8` at the
bucket (`[0x3971] := 9`, which buckets to 3, the fight), then `Random(200) → 0`
fired the church, and the turn produced **no encounter draws at all** — the
next draws are `1000:25fe` twice and then `1000:b3ae`. That is
`1000:8282`'s `mov byte [0x3970],0` erasing a bucket that had already been
rolled, which was the catalogue's widest claim, and the trace shows it
happening.

**The mage spends no draws.** Run A's turn 24 drew `Random(100) → 0` at
`1000:b3ae`, and the turn ends there: no draw follows in that turn. A negative
that only a live breakpoint can establish.

**The catalogue's completeness claim survives.** 1387 draws were observed at 34
distinct call sites — the catalogue's seventeen (draws 17 and 18 share
`1000:25fe`) and seventeen more. None of the seventeen extra sites is in the
catalogue, and
**none of them is inside `1000:ae5a`..`1000:b3ba`**, the range the catalogue's
byte scan claims to enumerate completely. They are the encounter machinery
downstream of the bucket dispatch — `1000:0d26`..`1000:1197` (enemy generation
inside `FUN_1000_0d14`, 348 of them at `1000:0efd` alone), plus `1000:b5f1`,
`1000:b725` and `1000:b792`, which `docs/re/wander.md` already lists as out of
scope.

## One finding the trace produced that is not in the catalogue

**Bucket 2's `Random(2)` is behind a typed token, not automatic.**
`docs/re/METHODOLOGY.md`'s worked example says bucket 2 "runs its own further
`Random(2)` and, on zero, reaches `1000:b570`", the girl's discovery flag. The
trace saw fourteen bucket-2 turns across the five runs — including seven in
runs B and E, where the girl flag was still clear — and `1000:b54e` **never
fired**. Reading the arm settles why, and it is two gates, not none
(**established from flow**, then corroborated by the breakpoint's silence):

```
0000B4E8  3C02              cmp al,0x2
0000B4EA  7403              jz 0xb4ef
0000B4EC  E9BF00            jmp 0xb5ae
0000B4EF  803E973600        cmp byte [0x3697],0x0   ; girl already known?
0000B4F4  7403              jz 0xb4f9
0000B4F6  E99900            jmp 0xb592              ; -> "Совсем ничё не происходит."
...
0000B520  9AC606780F        call word 0xf78:word 0x6c6   ; ReadLn into DS:3a72
0000B53E  BF2383            mov di,0x8323                ; the token `y` (file 0x9BF3)
0000B543  9AD80B780F        call word 0xf78:word 0xbd8   ; string compare
0000B548  7546              jnz 0xb590                   ; not `y` -> no draw at all
0000B54A  B80200            mov ax,0x2
0000B54D  50                push ax
0000B54E  9A4B11780F        call word 0xf78:word 0x114b  ; Random(2)
0000B553  09C0              or ax,ax
0000B555  7520              jnz 0xb577
0000B570  C606973601        mov byte [0x3697],0x1        ; the girl flag
```

The arm prints `^5Идет типа клёвая цыпа. Хочешь её зацепить?` (file `0xA19E`),
reads a line, and spends the draw **only** when the player types `y`. The
harness's driver declines every question, so it never typed `y` — the silence is
explained, and the gate is now on record. This is a refinement of the worked
example, not a contradiction of it: on the `y` path the example's reading is
exactly right. It matters for the port, because a draw spent unconditionally
where the original spends it only after a `y` puts the whole sequence out of
step; `docs/re/gaps.md` carries that question.

## Reproducing

```bash
# one run (writes build/rngtrace/traceA.json)
python3 tools/rngtrace/run.py --boot-img <freedos.img> --walks 30 \
    --class-answer 0 --seed 0x12345678 --workdir build/rngtrace/runA \
    --out build/rngtrace/traceA.json

# run E: load the shipped save corpus instead of creating a character
python3 tools/rngtrace/run.py --boot-img <freedos.img> --walks 25 \
    --with-saves --district 3 --seed 0x12345678 \
    --workdir build/rngtrace/runE --out build/rngtrace/traceE.json

# the comparison against the catalogue -> data/rng_trace.json
python3 tools/rngtrace/compare.py build/rngtrace/trace{A,B,C,D,E}.json \
    --labels A,B,C,D,E --out data/rng_trace.json

# the parts that need no emulator
python3 tools/test_rngtrace.py
```

Determinism: the same script and seed produce the same trace. Runs A and B were
each executed twice in separate VMs and produced the same draw count and the
same prompt-stop count both times, and run C was re-run into a separate workdir
and compared draw for draw — identical. Both completeness checks (LCG replay,
final `RandSeed`) are what turn that from an impression into a check the harness
performs on itself every run.

## Limits — what this does NOT establish

* **Probabilities are still read from the comparison constants, never counted.**
  The result columns in `data/rng_trace.json` are one seed's outcomes. Nothing
  here measures a distribution, and `docs/re/METHODOLOGY.md` forbids inferring
  one from counts.
* **The three fight-flow questions `docs/re/gaps.md` records are untouched.**
  The draws at `1000:b5f1`, `1000:b725`, `1000:b792` and inside
  `FUN_1000_0d14` were logged but not analysed; which of `1000:b691` /
  `1000:b721` a real encounter reaches is still open.
* **Bucket 4 and bucket 1 arms were not driven** beyond whatever the runs
  happened to hit, and the `y` path of bucket 2 was never taken.
* **Only two districts were visited** (1 and 3), so `chapter*20` is corroborated
  at two points, not across its whole range.
* **The trace says nothing about the port.** No Rust was run, read, or compared
  here; ground truth is `orig/g.exe` alone.
