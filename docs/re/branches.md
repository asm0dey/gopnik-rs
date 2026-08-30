# Every branch Ghidra found (Task 11e)

Machine-readable form: `data/branches.json`, emitted by
`tools/ghidra/EnumerateBranches.java` through `tools/ghidra/run_ghidra.sh`
(Ghidra 12.1.2, headless, no re-analysis). This document changes no Rust.

The deliverable is a checklist: every conditional branch in code Ghidra
identified as a function, the flag-setting instruction that guards it, whether
it sits in game code or in Borland's runtime library, and whether the Rust port
has ever cited it. It exists so that "have we covered the branch set?" stops
being a search problem — the last "complete scan" claim in this project was one
person's reading, and it stopped early.

Per `docs/re/METHODOLOGY.md` every claim below states its tier. The enumeration
itself is **flow** evidence: instructions, their conditions, their targets. The
port cross-reference in the last section is a **proxy** and is labelled as one
wherever it appears.

## Read this before quoting any number: what this is a lower bound on

**This is every branch Ghidra found, not every branch.** Code Ghidra failed to
disassemble contributes no branches and cannot report that it is missing. The
honest form of the headline is:

> 1119 conditional branches, in the 43,890 bytes of the 68,320-byte code
> segments that Ghidra placed inside a function — 64.2% of those segments by
> byte.

That 64.2% sounds alarming, so here is what the other 35.8% actually is,
measured rather than asserted:

| block | real seg | size | in functions | undefined | of which inside a known string | unaccounted |
|-------|---------:|-----:|-------------:|----------:|-------------------------------:|------------:|
| `CODE_0` (game) | `0000` | 61008 | 38264 | 22742 | 22714 | **28** |
| `CODE_1` | `0ee5` | 128 | 124 | 4 | 0 | 4 |
| `CODE_2` | `0eed` | 656 | 651 | 5 | 0 | 5 |
| `CODE_3` | `0f16` | 1568 | 871 | 697 | 0 | 697 |
| `CODE_4` | `0f78` | 4960 | 3980 | 949 | 0 | 949 |

"Inside a known string" means the byte falls in `[off, off + 1 + len(text))`
for an entry in `data/strings.json`, whose `off` is the Pascal length byte.

### The check that the attribution rests on — and the one it must not

`img[off] == len(text)` holds for 796 of 796 entries, and it is **worthless as
evidence**: `strings.json`'s `text` is a strict 1:1 byte→character rendering of
the bytes the extractor read, so that comparison is true by construction and
cannot fail. An earlier draft of this document cited it. It is the same defect
`docs/re/METHODOLOGY.md` records under "evidence that proves less than it
claims", and it is withdrawn.

The check that *can* fail is structural. Walk each undefined `CODE_0` run from
its first byte as a **chain** of Pascal length-prefixed records — `p += 1 +
img[p]`, starting at the run start, nothing else supplied — and see where the
chain lands. Arbitrary bytes overshoot the run end or leave a residue; a run
that really is a literal pool tiles exactly, end to end, with zero bytes left
over. **11 of the 17 `CODE_0` undefined runs tile exactly, including every
large one:**

| run | file off | length | records | residual |
|-----|---------:|-------:|--------:|---------:|
| `1000:82b3` | 39811 | 10406 | 352 | **0** |
| `1000:2c5e` | 17710 | 4275 | 133 | **0** |
| `1000:584c` | 28956 | 1801 | 36 | **0** |
| `1000:633d` | 31757 | 1744 | 52 | **0** |
| `1000:777c` | 36940 | 1259 | 38 | **0** |
| `1000:165f` | 12079 | 932 | 59 | **0** |
| `1000:04be` | 7566 | 653 | 12 | **0** |
| `1000:0000` | 6352 | 569 | 8 | 57 |
| `1000:73ee` | 36030 | 330 | 10 | **0** |
| `1000:28c7` | 16791 | 253 | 9 | **0** |
| `1000:1274` | 11076 | 212 | 15 | **0** |
| `1000:248f` | 15711 | 151 | 10 | **0** |
| `1000:023b` | 6923 | 135 | 1 | 88 |
| `1000:0adb` | 9131 | 17 | 0 | 17 |
| `1000:0d59` | 9769 | 2 | 0 | 2 |
| `1000:6da3` | 34419 | 2 | 0 | 2 |
| `1000:0acb` | 9115 | 1 | 0 | 1 |

Independently: **98.26% of the 22,742 undefined `CODE_0` bytes are printable**
(byte ≥ `0x20` and ≠ `0x7f`, or TAB/CR/LF — CP866, so the high half is Cyrillic
text, not noise). Both commands are in the *Recomputation* block at the end of
this document.

### The game's own code segment is 99.88% accounted for — with a disclosure

Of the 22,742 bytes of `CODE_0` outside any function, 22,714 fall inside a
`data/strings.json` entry — Borland put the main unit's literals in the code
segment, interleaved with code, which is exactly why Ghidra stops there. **28
bytes** of `CODE_0` are neither code Ghidra recognised nor a string the
extractor found. They fall in **nine** runs (an earlier draft said eight):

| Ghidra | file off | bytes |
|--------|---------:|------:|
| `1000:0acb` | `0x239b` | 1 |
| `1000:0ae4` | `0x23b4` | **8** |
| `1000:0d59` | `0x2629` | 2 |
| `1000:28c7` | `0x4197` | 5 |
| `1000:633d` | `0x7c0d` | 2 |
| `1000:6da3` | `0x8673` | 2 |
| `1000:74a9` | `0x8d79` | 2 |
| `1000:8321` | `0x9bf1` | 4 |
| `1000:848e` | `0x9d5e` | 2 |

The largest single unaccounted stretch is 8 bytes, inside the 17-byte run at
`1000:0adb` that sits between the end of `FUN_1000_0acc` and the start of
`FUN_1000_0aec`.

**What that 28 rests on, stated rather than assumed.** The attribution inherits
whatever `data/strings.json` got wrong, and it got some things wrong:

* **252** of the 22,714 attributed bytes are attributed *only* by entries that
  `strings.json` itself flags `suspect: true`.
* **356** rest only on matches shorter than 8 bytes; **74** of those bytes come
  from one-character "strings" (37 entries of 2 bytes each).
* **10** `strings.json` entries overlap a Ghidra function body outright, which
  is direct proof that the extractor emits false positives: entries at file
  9120/9124/9127/9130 sit inside `FUN_1000_0acc` (15 bytes), and six more sit
  inside `FUN_1eed_0000`, `FUN_1f16_000d` and `FUN_1f78_0000`. Nine of the ten
  are already flagged `suspect`; the one at file 9120 (`Ы ^`) is not.

Discount every byte that rests solely on a suspect entry and the residual is
**~280 bytes (28 + 252), not 28**. State both: 28 is the figure if you take
`strings.json` at face value, ~280 is the figure if you discard everything it
already doubts, and the difference between them is exactly "how much do you
trust the extractor's own suspect flag". **The conclusion does not move.** A
`jcc` is two bytes, so 28 — or 280 — bytes could in principle hide a handful of
branches; neither could hide a routine. The residual risk of missed *game*
branches is bytes, not the 22 KB the raw coverage figure suggests.

The 1,655 unaccounted bytes in `CODE_3`/`CODE_4` are Borland RTL data tables and
unreferenced RTL routines. RTL branches are marked, not counted, in the coverage
work below, so this does not affect the game numbers.

Other stated limits:

* **0 unresolved indirect jumps.** Every one of the 1119 branches is a two-byte
  `jcc rel8` with a resolved target. There is no computed `jmp` anywhere in the
  disassembled code, so there is no jump table Ghidra silently gave up on. There
  are 4 indirect *calls* (`CALLF [BX+DI]`, `CALL AX`, `CALLF [BX+0x14]`,
  `CALLF [BX+0x18]`), all in RTL exit/overlay plumbing, and 28 software
  interrupts; all 32 are listed in `unresolved_indirect_control_flow_detail`.
* **131 of 1119 guards are unresolved** (97 of them in game code) and are
  emitted as `"guard": null` with a reason, never guessed. 123 of the 131 are
  `blocked_by_call`: the flags come out of a callee. 93 of those calls are to
  `0f78:0bd8`, the Pascal shortstring compare already identified in
  `docs/re/command-dispatch.md` — for those the condition is "typed line equals
  the literal pushed before the call", and `data/command_dispatch.json` already
  resolves the literals. This script does not.
* **The port cross-reference is citation-based**, not semantic. See the last
  section for both directions in which it lies.
* Ghidra's function boundaries are Ghidra's. `entry` is 17,143 bytes and holds
  most of the game, so per-function ranking alone is useless for it; the
  `uncited_spans` array exists for that reason.

## Address convention, and how it was checked

Every address in this file is a Ghidra label (`SEG >= 0x1000`), so Form A of
the convention applies throughout. `docs/re/METHODOLOGY.md`, "Address convention, and its range of validity", is the authority for the rule; `tools/addr.py` is its executable form and `python3 tools/re_query.py resolve <citation>` checks any single address against the bytes. `data/branches.json`
records the same formula in its `file_off_formula` field, which is a
description of that artifact, not a second authority.

Every branch record carries `file_off` (that formula) **and**
`file_off_ghidra` (Ghidra's own `AddressSourceInfo.getFileOffset()`, computed
by a different path) and the script counts disagreements: `0` of 1119. This
caught a real bug during development — `Address.getOffset()` on a segmented
address returns the *flat* offset, not the in-segment offset, which put every
file offset 0xF780 bytes out until the two-way check flagged it.

Independent byte check, re-runnable, no Ghidra required:

```bash
python3 - <<'EOF'
import json
img = open('orig/g.exe','rb').read()
d = json.load(open('data/branches.json'))
bad = [b for b in d['branches']
       if img[b['file_off']:b['file_off']+len(bytes.fromhex(b['bytes']))]
          != bytes.fromhex(b['bytes'])]
print(len(d['branches']), 'branches,', len(bad), 'byte mismatches')
EOF
```

Result: **1119 branches, 0 byte mismatches**; the same check over the 988
resolved guard instructions is also 0. Independently, decoding each branch's
own `rel8` displacement (`target = addr + 2 + rel`) reproduces Ghidra's `taken`
field for all 1119.

## Shape of `data/branches.json`

Top level:

| key | what |
|-----|------|
| `limits` | every number in the section above, plus the caveat string |
| `memory_blocks` | per-block byte accounting (size / in functions / instructions / data / undefined) |
| `undefined_runs` | every contiguous run Ghidra left undefined, with block, `SEG:OFF`, `file_off`, length |
| `classification_rule` | the game/RTL rule and its basis, in the artifact itself |
| `functions` | all 123, with `class`, `seg`, `real_seg`, `size`, `branch_count`, `caller_count`, `calls_segment_1000`, port citations |
| `branches` | the 1119 records |
| `uncited_spans` | maximal intervals of game functions containing no port citation, with the branches inside |
| `unresolved_indirect_control_flow_detail` | the 32 indirect calls / interrupts |
| `port_cross_reference` | the metric, `is_proxy: true`, and both observed failure directions — so a consumer that never opens this document still gets the caveat |

A branch record:

```json
{"addr": "1000:b279", "file_off": 52041, "file_off_ghidra": 52041,
 "real_seg_off": "0000:b279", "bytes": "7751", "func": "entry",
 "func_entry": "1000:ab59", "class": "game", "mnemonic": "JA",
 "text": "JA 0x1000:b2cc", "taken": "1000:b2cc", "fallthrough": "1000:b27b",
 "reads_flags": ["CF", "ZF"],
 "guard": {"addr": "1000:b277", "file_off": 52039, "bytes": "09c0",
           "mnemonic": "OR", "text": "OR AX,AX", "distance": 1,
           "kind": "flags", "join_crossed": false,
           "shared_with_preceding_branch": false},
 "guard_status": "resolved", "cited_in_port": false,
 "guard_cited_in_port": false, "port_touched": false,
 "bytes_to_nearest_port_citation": 39,
 "port_citations": [], "guard_port_citations": []}
```

Intended queries are one filter each:

* every unreached branch in function F — `class == "game" && func == F && !port_touched`
* every branch whose condition is unknown — `guard == null`
* everything to ignore — `class == "rtl"`
* what to work on next — sort `uncited_spans` by `branch_count`, then read
  `port_cross_reference` before treating the result as a to-do list

**Address fields all share the branch's segment.** `addr`, `taken` and
`fallthrough` are rendered in the segment of the branch instruction, so
`fallthrough` and `taken` join against another record's `addr` as plain strings.
This needed a fix: Ghidra's `Instruction.getFallThrough()` renormalises the
segment of the address it returns while `getFlows()` does not, so the first
version of this artifact emitted `{"addr": "1ee5:0013", "taken": "1ee5:0017",
"fallthrough": "1000:ee65"}` — the same linear byte as `1ee5:0015`, but a
string join fails on it. All 281 RTL records were affected; game records never
were, because game code lives in segment `1000` already. `EnumerateBranches.java`
now rebuilds both destinations in the branch's own segment (`addrIn`), and the
regenerated file has 0 segment mismatches with `flat(fallthrough) ==
flat(addr) + len(bytes)` still holding 1119/1119.

`text` is Ghidra's own rendering of the instruction and is **not** normalised —
on an RTL record it can still show the renormalised target (`"JC 0x1000:ee67"`
on the branch above). Read `taken`, not `text`, when you want the destination.

**`size` on a function record is `getNumAddresses()`, a set cardinality, not a
span.** Function bodies in the RTL are not always contiguous, so reading `size`
as `[entry, entry + size)` invents overlaps that do not exist. The one case in
this binary: `1f78:1117` reports `size: 22`, but `ndisasm` from the entry shows
its straight-line body running `or cl,cl` / `jz` / `call` / `jc` / `retf` and
ending at `1f78:1120` — 10 bytes, with the next function entry at `1f78:1121`.
The other 12 addresses are therefore in a second, non-adjacent chunk (the shared
error tail its own `jc 0x113f` targets is the obvious candidate, but that part
is an inference — the artifact does not emit body ranges). Taking `size` as a
span makes `1f78:1117` appear to swallow the two 4-byte functions `1f78:1121`
and `1f78:1125`, an 8-byte phantom overlap.

Task 11h pinned down the second chunk, and the candidate above was half right.
The 12 addresses are `1f78:113f`..`114a`: **two** 6-byte out-of-line error
tails, `mov ax,0xcd` / `jmp 0x10f` at `113f` (the shared overflow exit that the
`jc` of all four real-op thunks targets) and `mov ax,0xc8` / `jmp 0x10f` at
`1145` (divide-by-zero, reached only from `1117`'s own `jz`).  10 + 6 + 6 = 22,
and `113f`..`114a` is exactly the 12-byte hole the export's spans leave between
`1f78:1131`'s end and `1f78:114b`.  The artifact still emits no body ranges, so
this remains an inference — but it is now a checked one with no competing fit.
`docs/re/rtl.md`, "One thing the assertion found", carries the disassembly and
what the span approximation in `tools/re_query.py` gets wrong because of it.

Same task, one correction to the paragraph above and to the note in the audit
script below: `1f78:1117` is the sole **overlapping** record, not the sole
non-contiguous one.  A tiling census over all 123 records found a second,
`1000:0d14` in the game's own code — `size: 1196` where the contiguous body up
to the next entry `1000:11c2` is 1198 bytes.  It creates no overlap (its span
is short, not long), and the strings-overlap count below is 10 either way.

**Which two addresses `1000:0d14` loses, settled from the image.**  They are
`1000:11c0` and `1000:11c1`: the last instruction inside the body is `c2 02 00`
(`ret 0x2`) at `1000:11bf`, and the next function entry is `55` (`push bp`) at
`1000:11c2` — check either with `python3 tools/re_query.py resolve 1000:11bf`.
So the span `[0d14, 0d14+1196)` stops one instruction short, on the two operand
bytes of that `ret`, and **no `data/strings.json` entry touches them**
(checked: 0).  This needs no Ghidra re-run to say; `docs/re/rtl.md`, "How many
records this affects, measured over all 123", is where the census lives, and
what remains unexplained there is *why* Ghidra's address set omits those two
bytes, not which two they are.

The bodies really are disjoint, and the argument does not need the chunk
boundaries: the sum of `getNumAddresses()` over all 123 functions is 43,890, and
the independent per-address block walk that produces the 64.2% headline — which
visits each byte once and can therefore never double-count — also gives 43,890.
Two ways of counting agree, so no byte lies in two bodies and the headline is
not inflated.

## How a branch's guard is found

Walk back from the branch through the function, at most 12 instructions, for the
nearest instruction that writes a register the branch reads (Ghidra's
`getInputObjects` / `getResultObjects`, flags preferred over other registers so
`LOOP`/`JCXZ` resolve to whatever last wrote `CX`). Stop — and emit `null` — at
a call, a return, an unconditional jump, or the function start. A *conditional*
jump is stepped over rather than stopped at, because `jcc` does not write flags:
that is the 32-bit compare idiom (`cmp dx,bx` / `jg` / `jl` / `cmp ax,cx`), and
the 21 branches that share a guard this way carry
`guard.shared_with_preceding_branch: true`.

Results: 988 resolved, of which 963 are the immediately preceding instruction
(`distance: 1`). Guard mnemonics, recounted from the shipped
`data/branches.json` (an earlier draft of this document said `CMP` 816 / `OR`
97, which matches no subset of the artifact and was wrong):

| mnemonic | all | game | rtl |
|----------|----:|-----:|----:|
| `CMP` | **830** | 709 | 121 |
| `OR` | **98** | 32 | 66 |
| `DEC` | 13 | 0 | 13 |
| `SUB` | 9 | 0 | 9 |
| `ADD` | 6 | 0 | 6 |
| `MOV` | 5 | 0 | 5 |
| `ADC` | 5 | 0 | 5 |
| `TEST` | 4 | 0 | 4 |
| `RCL` / `XOR` | 3 each | 0 | 3 each |
| `SBB` / `CMPSB.REPE` / `INC` | 2 each | 0 | 2 each |
| `AND` / `SHRD` / `NEG` / `SHR` / `SCASB.REPNE` / `POPF` | 1 each | 0 | 1 each |

Total 988; game code resolves 741 of its 838 branches and uses only `CMP` and
`OR`.

**On the `or ax,ax` idiom.** An earlier draft glossed all the `OR` guards as
"the `or ax,ax` after a `Random` call". That is a wrong inference: 66 of the 98
are in RTL and cannot be. Checked against `orig/g.exe` — is the `9a 4b 11 78 0f`
far call to `Random` the five bytes immediately before the guard? — **22 of the
98 `OR` guards qualify, all of them in game code** (22 of the 32 game `OR`
guards). For those 22 the condition really is "the draw came back non-zero".
The other 76 are something else and are not claimed.

`join_crossed: true` (4 cases, all RTL) means the walk stepped over an address
something else jumps to, so the guard holds on the fall-through path only.

The 12-instruction limit and the stop-at-call rule are why 131 are `null`. They
are reported as `null` rather than filled in with the nearest plausible `cmp`:
an unestablished guard is unknown, and `docs/re/METHODOLOGY.md` says so.

## Game code versus Borland RTL

**Rule:** a function is `game` iff its Ghidra segment is `0x1000` (real DOS
segment `0000`, the program's own code segment); everything else is `rtl`.

**Basis:** Borland Pascal emits the main program body into its own code segment
and each linked unit (`System`, `Crt`, `Dos`, overlay manager) into further
segments; Ghidra's MZ loader gives each a separate block. This is a heuristic
and is labelled as one in the artifact.

It splits 123 functions into **16 game (838 branches)** and **107 RTL (281
branches)**. Corroboration, not proof: the RTL segments are the ones
every top-level routine calls into, never the reverse, and the four RTL entry
points the docs have already identified by hand (`0f78:114b` `Random`,
`0f78:06c6` `ReadLn`, `0eed:01c2` `WriteLn`, `0f78:0bd8` shortstring compare)
all land on the RTL side. One `1000:` function is *not* reached from `entry`:
`FUN_1000_0acc` (15 bytes, 0 branches) is called from `1f78:0000`, the shape of
a Pascal `ExitProc` the program registers and the RTL calls back — game code by
segment, invoked by RTL, which is why the rule keys on the segment and not on
the call graph.

**No branch is deleted by classification** — RTL branches are in the file,
tagged. Every function record also carries `seg`, `real_seg`, `caller_count` and
`calls_segment_1000`, so a different rule can be applied to `data/branches.json`
without re-running Ghidra.

## Verification against an independently hand-recovered branch set (Step 5)

Two checks, one against `docs/re/wander.md` (the wander turn) and one against
`docs/re/combat.md`.

**1. The fracture block, `1000:b277`..`1000:b2ae`.** `docs/re/wander.md` quotes
this block's disassembly literally, with byte encodings. Comparison is exact:

| doc | script |
|-----|--------|
| `b279 7751 ja 0xb2cc`, guard `b277 09C0 or ax,ax` | `1000:b279` `7751` JA → `1000:b2cc`, guard `1000:b277` `09c0` OR AX,AX |
| `b280 7525 jnz 0xb2a7`, guard `b27b cmp byte [0x38b0],0` | `1000:b280` `7525` JNZ → `1000:b2a7`, guard `1000:b27b` CMP byte [0x38b0],0x0 |
| `b287 741E jz 0xb2a7`, guard `b282 cmp byte [0x38b1],0` | `1000:b287` `741e` JZ → `1000:b2a7`, guard `1000:b282` CMP byte [0x38b1],0x0 |
| `b2ac 741E jz 0xb2cc`, guard `b2a7 cmp byte [0x38b0],0` | `1000:b2ac` `741e` JZ → `1000:b2cc`, guard `1000:b2a7` CMP byte [0x38b0],0x0 |

Four branches documented, four found, same addresses, same bytes, same targets,
same guards, no fifth branch in the range. **Agreement, no disagreement.**

**2. The whole wander turn, `1000:ae5a`..`1000:b3ba`.** Taking every address
cited by `docs/re/wander.md` plus `data/wander.json` in that range — **121**
distinct addresses. The regex must be the broad one, `(?:1000:)?\b[0-9a-f]{4}\b`:
`wander.md` quotes disassembly listings whose addresses are **bare**
(`b368  cmp [0x3971],0x09`), and a `1000:`-only regex misses them. That is not a
detail — it is the bug that produced the wrong figure below in the first draft.

* **45** are the **guard** of a branch the script found,
* **1** is a **branch** address (`1000:b279`, from the quoted disassembly above),
* **75** are neither, and every one of them is a non-branch site the documents
  cite for another reason — `Random` call sites (`af68`, `b030`, `b353`, …),
  flag stores (`af71`, `b196`, `b570`), `call` sites (`b3a7`, `b3ae`), `mov`s.

So the hand-recovered document cites **conditions** (the `cmp`), where this
artifact keys on the **branch** — different addresses for the same decision, and
every documented decision is present. **No documented branch is missing from the
enumeration.**

The reverse direction is the interesting one: the script finds **61** branches
in that range, and **16 of them** — `ae8b ae9c aeb1 aee9 af17 af29 af30 b12a
b151 b17c b258 b266 b2db b30b b30d b311` — are mentioned by neither `wander.md`
nor `wander.json`, at the branch address or at the guard. That is **26%** (16 of
61) of the branches in the most carefully documented region of the game. Not an
error in `wander.md`, which set out to catalogue the `Random` sequence and does
that correctly; it is the measure of what mechanical enumeration adds.

An earlier draft of this document said **18 of 61 (30%)** and listed `b36d` and
`b380` among them. That was wrong, and wrong for a reason worth recording: the
narrow `1000:`-only regex could not see `wander.md`'s own disassembly listing,
which documents both branches at their guard —

```
wander.md:139  b368  cmp [0x3971],0x09 / ja  b37b     <- guard of 1000:b36d
wander.md:141  b37b  cmp [0x3971],0x04 / ja  b38e     <- guard of 1000:b380
```

— and `data/branches.json` gives exactly those two guards for `1000:b36d` and
`1000:b380`. The document's own stated criterion therefore excludes them. 16, not
18.

Spot-check of three of those 16 against `ndisasm` (independent disassembler,
started from the aligned instruction `1000:af04` that `data/wander.json` cites
with bytes):

```
0000AF15  3BC2              cmp ax,dx           ; script: guard of af17
0000AF17  7D04              jnl 0xaf1d          ; script: 1000:af17 7d04 JGE -> 1000:af1d
0000AF24  803E4D3900        cmp byte [0x394d],0 ; script: guard of af29
0000AF29  7432              jz 0xaf5d           ; script: 1000:af29 7432 JZ  -> 1000:af5d
0000AF2B  803E323E19        cmp byte [0x3e32],0x19
0000AF30  732B              jnc 0xaf5d          ; script: 1000:af30 732b JNC -> 1000:af5d
```

Address, encoding, mnemonic (`jnl` = `JGE`), target and guard all match.

**3. Combat, `1000:3fa7`..`1000:408f` and `1000:446a`..`1000:4484`.**
`docs/re/combat.md` cites the guard addresses and names the branch mnemonic. The
enemy blow budget: doc `1000:3fbb cmp mine,0x0a / jng` → script `1000:3fc0`
`7e2a` JLE → `1000:3fec` with guard `1000:3fbb` CMP word [BP+0xfef2],0xa; doc
`1000:3fc2 cmp theirs,0x12 / jng` → script `1000:3fc7` JLE with guard
`1000:3fc2`; doc `1000:3fc9 cmp mine,0x1c / jl` → script `1000:3fce` JL →
`1000:3fe2`, and `1000:3fe2` is where the doc says `mine := 10` is written.
Accuracy: doc `1000:447f cmp roll,0x5a → miss` → script `1000:4484` `7e03` JLE
with guard `1000:447f` CMP word [BP+0xfeee],0x5a. The player's mirror at
`1000:404a`..`1000:408f` is the same three branches with the record addresses
swapped, as the doc states. **Agreement; `JNG` and `JLE` are the same opcode
(`0x7e`) under two names, which is a spelling difference, not a disagreement.**

## Coverage against the port (Step 4)

**The metric, and how it lies.** A branch is `port_touched` when its own address
or its guard's address appears as a `SEG:OFF` citation in `src/**/*.rs` or
`data/command_dispatch.json`. That is a proxy for "the port has reckoned with
this branch", and it errs in both directions — both observed here:

* **Over-reports coverage.** `src/game.rs` cites `1000:b3a7` and `1000:b3b7`
  precisely in order to say the port does *not* spend those two draws. A
  citation can be a record of a gap.
* **Under-reports coverage.** Shop prices were ported from `data/shops.json`,
  extracted as a table rather than read branch by branch, so the shop menu
  bodies show as untouched even though part of their behaviour is modelled.

Read the ranking as "where the port's own notes stop", not as "unimplemented".
Deciding which is which is the next task's job, not this artifact's.

**That caveat also lives inside `data/branches.json`**, as the top-level
`port_cross_reference` object: the metric, `is_proxy: true`, what
`port_touched: false` does and does not mean, and both failure directions with
their examples. The brief asked for a file usable without reading this prose,
and `uncited_spans` is the most tempting thing in it to mistake for a to-do
list, so the warning travels with the data rather than only here.

**Totals — and the one warning that matters about them.** Every number in this
section counts citations in `src/`, so **every number here is revision-bound and
goes stale on the next commit that adds one.** `data/branches.json` froze its
own columns when Ghidra last ran (`82a08d8`); the table below is recomputed from
the shipped tree, and the block under *Recomputation* prints it. Quote the
command, not the cell.

After Task 28: 838 game branches; **424 touched (50.6%)**; 414 with no citation
at the branch or its guard. The recent history, each figure from the block under
*Recomputation* run against a `git worktree` of that commit: `3981f74` (before
Task 22) 280 (33.4%); `e657bbe` (Task 22 ported) 305 (36.4%); `19a1a34`
(Task 24, `bmar` rows 1-6 ported) 348 (41.5%); `7b04cdf` and `768a85e`
(Task 25, `mar` rows 1-9 MAPPED) **still 348** — an RE task moves this metric
by nothing at all; Task 26 (`mar` rows 1-9 ported) 388 (46.3%), all of the +40
in `1000:ab59` (148 → 188); `629e56a` (Task 27, the den's submenu MAPPED)
**still 388** — the same nothing an RE-only task moved at Task 25, measured
again; Task 28 (the den's submenu ported) 424 (50.6%).

**Task 28's +36 splits across two functions.** `1000:ab59` goes 188 → 214
(+26, the den's own gates and arms) and `1000:3d11` goes 96 → 106 (+10, from
the `param_1` dispatch chain the den's two blocked call sites cite in
`Game::den_beat_up`'s and `Game::den_job`'s doc comments). **The `touched`
column moved in exactly those two functions and nowhere else**; the
`citations` column moved in six, which is the gap between *citing an address*
and *citing a branch* the paragraph above is about:

| entry | touched | citations |
|---|---|---|
| `1000:ab59` | 188 → **214** | 1136 → 1356 |
| `1000:3d11` | 96 → **106** | 630 → 647 |
| `1000:1a03` | 54 (unchanged) | 174 → 176 |
| `1000:6a0d` | 15 (unchanged) | 158 → 159 |
| `1000:0d14` | 11 (unchanged) | 53 → 58 |
| `1000:2526` | 9 (unchanged) | 51 → 54 |

**The per-entry table below is Task 26's and is NOT recomputed here.** Every
cell Task 28 changed is in the table just above; the rest of the columns are
byte-for-byte what the *Recomputation* block prints at this tree. Quote the
command, not the cell.

**Over the whole shop-arms branch the figure is +83, and it does NOT all land
in one function.** `bfad0b4` → `e4e929b` is 305 → 388, splitting **+81 in
`1000:ab59`** (107 → 188) and **+2 in `1000:3d11`** (94 → 96, from Task 24's
combat-adjacent citations). The Task-26-only "+40, all `1000:ab59`" above is
exact because `1000:3d11` was already at 96 by 348; a reader who scales that
sentence up to the branch gets the split wrong, which is why both are written
out. Recomputed per revision with the block under *Recomputation*, over
`git show <rev>:<file>` rather than a checkout.

The frozen columns inside
`data/branches.json` still say 84 (10.0%), which was true at `82a08d8` and is
what this paragraph printed, unrecomputed, for four tasks. `docs/re/*.md` is
deliberately excluded from the scan: documented is not ported — which is
exactly what the two Task 25 rows above demonstrate.

| entry | bytes | branches | touched | citations | note |
|-------|------:|---------:|--------:|----------:|------|
| `1000:ab59` `entry` | 17143 | 406 | 188 | 1136 | the top-level body; most of the game |
| `1000:3d11` | 6971 | 224 | 96 | 630 | combat (identified in `docs/re/combat.md`) |
| `1000:1a03` | 2700 | 83 | 54 | 174 | the character sheet — ported in Task 22, `src/character_sheet.rs` |
| `1000:6a0d` | 2527 | 33 | 15 | 158 | |
| `1000:29c4` | 666 | 19 | 2 | 26 | |
| `1000:0d14` | 1196 | 17 | 11 | 53 | rolls the enemy (`src/game.rs`) |
| `1000:2526` | 929 | 17 | 9 | 51 | |
| `1000:7c67` | 1612 | 16 | 12 | 39 | the church (`docs/re/wander.md:162`, `docs/re/gaps.md:201`) |
| `1000:1348` | 791 | 11 | 1 | 18 | |
| `1000:0aec` | 552 | 5 | **0** | **0** | |
| `1000:074b` | 896 | 2 | 0 | 4 | |
| `1000:11c2` | 178 | 2 | 0 | 1 | |
| `1000:7538` | 580 | 2 | 0 | 29 | the mage's paid save (`docs/re/wander.md:287`) |
| `1000:5f55` | 1000 | 1 | 0 | 4 | |
| `1000:02c2` | 508 | 0 | 0 | 0 | the title banner — **inference from string pointers**, see below |
| `1000:0acc` | 15 | 0 | 0 | 0 | |

Every note in that table is sourced. `1000:3d11` is combat per
`docs/re/combat.md`; `1000:0d14` rolls the enemy per `src/game.rs`; the church
and the mage are named by `docs/re/wander.md` and `docs/re/gaps.md` at the lines
cited. **`1000:02c2` is the one inference**, and it is marked as one, on the same
footing as the span table below: the function loads `di` with `0x0`, `0x40`,
`0x80`, `0xc0`, `0x100`, `0x140`, `0x180`, `0x1c0` at `1000:0317`, `1000:0330`,
`1000:0349`, `1000:0362`, `1000:037b`, `1000:0394`, `1000:03ad` and `1000:03c6`,
pushing each to `WriteLn` (`0eed:01c2`) — established from flow, re-derived with
`ndisasm` from the aligned function entry. Those eight immediates resolve
(`file = imm + 0x18d0`) to `data/strings.json` entries 0–7 at file
6352/6416/…/6800, eight consecutive 63-character rows that draw a box-drawing
banner. "Title screen" is what those eight rows *look like*; nothing here traces
the caller, so it stays an inference.

### The ranking: largest uncited spans by branch count

**Revision-bound, exactly like the totals above.** A span is cut by every port
citation that lands in it, so porting a function does not remove one span — it
shatters it into many small ones. After Task 26, 421 spans hold 698 of the 838
game branches (at `e657bbe`, 416 spans held 767; at `82a08d8`, when
`data/branches.json` was generated, 191 spans held 822 and the `uncited_spans`
array in that file still says so). **More spans, fewer branches in them** is
what porting looks like on this metric. The top 12 below is recomputed from the
shipped tree by the block under *Recomputation*.

The three largest spans in the `e657bbe` ranking — `1000:c53b..1000:cb04` (30
branches), `1000:bd22..1000:bf7f` (17) and `1000:cb06..1000:ccd7` (13) — are
the two shops' menu and purchase blocks, and all three are gone from the top 12
after Tasks 24 and 26.

The `strings loaded inside` column is recomputed the same way, and by the method
the previous revision claimed and did not ship: decode the enclosing function
linearly from its entry with `tools/dis16.py`, keep each `mov di,imm16` whose
`imm + 0x18d0` is the `off` of a `data/strings.json` entry. That reimplementation
reproduces all seven of the counts the previous table printed (62 / 83 / 49 / 35
/ 26 / 28 / 42, for the old spans at `1a03`, `c53b`, `bd22`, `e39b`, `d8c9`,
`df11`, `7c67`), which is why the new counts are quotable. The span boundaries
are mechanical; the labels remain an **inference from those string pointers**,
not a traced dispatch.

| rank | span | function | bytes | branches | strings loaded inside suggest |
|-----:|------|----------|------:|---------:|-------------------------------|
| 1 | `1000:ced9`..`1000:d382` | `entry` | 1194 | 24 | selling back to the dealers — `^2У тебя есть ненужный костюм хочешь продать?`, `^0Продать вещи\` (20 string loads) |
| 2 | `1000:d8c9`..`1000:dc0d` | `entry` | 837 | 22 | the den — `^6На одного пацана наехал какой-то урод`, `^6Ты пацан нормальный. Есть дело.` (21) |
| 3 | `1000:e590`..`1000:e947` | `entry` | 952 | 21 | the gym — `20^7  прокачать пресс(Броня +1)` (26) |
| 4 | `1000:4169`..`1000:43f5` | `FUN_1000_3d11` | 653 | 17 | combat, the crowd lines — `Зрители:^6Врежь ему!` (20) |
| 5 | `1000:3d12`..`1000:3dc6` | `FUN_1000_3d11` | 181 | 10 | the combat opening — `Слышь Вась..`, `^4А чё ваще?` (4) |
| 6 | `1000:dd33`..`1000:ddf5` | `entry` | 195 | 9 | stealing — `^0Давай быстрее..` (4) |
| 7 | `1000:dfcc`..`1000:e180` | `entry` | 437 | 9 | the club — `22^7  разузнать приемы мухлёжников(Удача +1)` (9) |
| 8 | `1000:ea95`..`1000:ec81` | `entry` | 493 | 8 | the help text — `Напиши: ^6mar^7  чтобы идти на рынок` (18) |
| 9 | `1000:1e07`..`1000:1e34` | `FUN_1000_1a03` | 46 | 6 | inside the weapon line's seven-way disjunction; loads no strings |
| 10 | `1000:e23f`..`1000:e36c` | `entry` | 302 | 6 | the club's price check — `^6Не хватает денег - надо #.` (9) |
| 11 | `1000:0aec`..`1000:0d13` | `1000:0aec` | 552 | 5 | loads only the six two- and three-character fragments `Ы ^`, `С^`, `У^`, `П^`, `Е^`, `Р ^` (6) |
| 12 | `1000:2bc0`..`1000:2c52` | `FUN_1000_29c4` | 147 | 5 | beer — `^2Пиво прибавляет #з. Здоровья:#/#. Осталось #.#л. пива`, `^4Кончилось пиво` (3) |

**Three spans left this table between `e657bbe` and Task 26**, and they are
the whole of the two shops: `1000:c53b`..`1000:cb04` (rank 1, 30 branches,
the dealers' menu and rows 1-6), `1000:bd22`..`1000:bf7f` (rank 6, 17, the
market's menu and rows 1-4) and `1000:cb06`..`1000:ccd7` (rank 7, 13, the rest
of the dealers' body). Tasks 24 and 26 shattered all three with per-arm
citations. Next after the twelve above: `1000:3dc8`..`1000:3fa6` (5, combat —
`^4Я МАНЬЯК!!!`).

Defensible order of work, from this table alone: the **dealers' sell path**
(`1000:ced9`..`1000:d382`, 24 branches — the `x`/`wes` arms `docs/re/gaps.md`
still lists as open), then the den and the gym as a block of location menus.
The status screen, which headed this list for four tasks, is off it:
`FUN_1000_1a03` is ported (`src/character_sheet.rs`, Task 22), 54 of its 83
branches touched across 174 citation sites, and its largest remaining uncited
span is rank 9 at six branches.

## Recomputation, from the shipped artifacts

Standard library only, run from the repo root. Nothing here needs Ghidra.

**What these blocks do and do not cover.** They recompute every *derived* number
in this document — the coverage totals, the per-entry columns, the span ranking,
the string-load counts, guard mnemonics, the undefined-run tiling, string
attribution, and the byte / fallthrough / rel8 encoding checks. They do **not**
re-derive the byte tallies in the `limits` table at the top of this document
(`bytes_in_identified_functions`, `bytes_in_code_blocks`, and the 64.2% that
divides them); those are read out of `data/branches.json` and can only be
re-derived by re-running Ghidra. The one cross-check available without it —
43,890 two ways, `sum(size)` against the per-block walk — is the last block
here. An earlier revision of this header said "every number above" while
recomputing no coverage figure at all; that is the defect
`docs/re/METHODOLOGY.md` calls a check that cannot fail, and it is what let the
totals rot by 3.6x.

### Coverage: the totals, the per-entry columns and the span ranking

This block existed for four revisions without recomputing a single coverage
figure — the two blocks that came closest read `port_touched` and
`uncited_spans` straight out of `data/branches.json` and compared them with
themselves, which is a check that cannot fail. That is why the numbers above
rotted from 84 to 305 unnoticed. This is the check that can fail.

It reimplements `tools/ghidra/EnumerateBranches.java`'s rule rather than reading
its answer: the `CITATION` pattern (`\b hex{4} : hex{1,4} \b`, ASCII word
boundaries, as Java's default `\b` is), the `seg < 0x1000 ? seg + 0x1000 : seg`
normalisation, the flat-address comparison, the citation source globs read from
the artifact's own `port_citation_sources`, and `port_touched` = the branch's
own address **or its guard's**.

**Validated against the artifact it must reproduce.** Run at `82a08d8` — the
revision whose `src/` generated `data/branches.json` — it prints
`838 | touched 84 (10.0%) | uncited 754`, every per-entry `touched`/`citations`
column equal to that function's recorded `branches_touched_by_port` /
`port_citation_count`, and `191 spans hold 822 of the 838`. A companion check
over the raw records reported **0 mismatches on `cited_in_port`,
`guard_cited_in_port` and `port_touched` across all 1119 branch records**, 0
column mismatches across all 123 functions, and a symmetric difference of 0
between its 191 spans and the artifact's `uncited_spans`. Reproduce that before
trusting any figure this block prints at a later revision:

```bash
git worktree add /tmp/wt82 82a08d8 && cd /tmp/wt82   # then run the block below
```

```bash
python3 - <<'EOF'
import json, glob, re, collections
d = json.load(open('data/branches.json'))
CITE = re.compile(r'\b([0-9a-fA-F]{4}):([0-9a-fA-F]{1,4})\b', re.ASCII)
flat = lambda a: int(a[:4], 16) * 16 + int(a[5:], 16)
cite = collections.defaultdict(set)
for pat in d['port_citation_sources']:                 # ["src/**/*.rs", "data/command_dispatch.json"]
    for p in sorted(glob.glob(pat, recursive=True)):
        for i, ln in enumerate(open(p, encoding='utf-8').read().splitlines(), 1):
            for s, o in CITE.findall(ln):
                s = int(s, 16)                         # normalise a runtime segment to Ghidra's
                cite[(s + 0x1000 if s < 0x1000 else s) * 16 + int(o, 16)].add('%s:%d' % (p, i))
B = [b for b in d['branches'] if b['class'] == 'game']
hit = lambda b: flat(b['addr']) in cite or bool(b['guard']) and flat(b['guard']['addr']) in cite
t = sum(map(hit, B))
print('game branches %d | touched %d (%.1f%%) | uncited %d' % (len(B), t, 100*t/len(B), len(B)-t))
spans = []
for f in sorted((f for f in d['functions'] if f['class'] == 'game'), key=lambda f: -f['branch_count']):
    lo, hi = flat(f['entry']), flat(f['entry']) + f['size']
    br = sorted(flat(b['addr']) for b in B if b['func_entry'] == f['entry'])
    cits = set().union(*[cite[a] for a in cite if lo <= a < hi] or [set()])
    print('%-12s bytes %5d branches %3d touched %3d citations %3d' % (
        f['entry'], f['size'], len(br),
        sum(1 for b in B if b['func_entry'] == f['entry'] and hit(b)), len(cits)))
    lo2 = lo
    for cut in sorted(a for a in cite if lo <= a < hi) + [hi]:
        n = [b for b in br if lo2 <= b < cut]
        if n:
            spans.append((len(n), '1000:%04x..1000:%04x' % (lo2 - 0x10000, cut - 1 - 0x10000),
                          f['entry'], cut - lo2))
        lo2 = cut + 1
spans.sort(key=lambda s: (-s[0], s[1]))
print('%d spans hold %d of the %d' % (len(spans), sum(s[0] for s in spans), len(B)))
for s in spans[:12]:
    print('  %3d  %-24s %-14s %5d bytes' % (s[0], s[1], s[2], s[3]))
EOF
```

At Task 26's tree that prints, verbatim, the numbers this document's tables
carry:

```
game branches 838 | touched 388 (46.3%) | uncited 450
1000:ab59    bytes 17143 branches 406 touched 188 citations 1136
1000:3d11    bytes  6971 branches 224 touched  96 citations 630
1000:1a03    bytes  2700 branches  83 touched  54 citations 174
1000:6a0d    bytes  2527 branches  33 touched  15 citations 158
1000:29c4    bytes   666 branches  19 touched   2 citations  26
1000:0d14    bytes  1196 branches  17 touched  11 citations  53
1000:2526    bytes   929 branches  17 touched   9 citations  51
1000:7c67    bytes  1612 branches  16 touched  12 citations  39
1000:1348    bytes   791 branches  11 touched   1 citations  18
1000:0aec    bytes   552 branches   5 touched   0 citations   0
1000:074b    bytes   896 branches   2 touched   0 citations   4
1000:11c2    bytes   178 branches   2 touched   0 citations   1
1000:7538    bytes   580 branches   2 touched   0 citations  29
1000:5f55    bytes  1000 branches   1 touched   0 citations   4
1000:02c2    bytes   508 branches   0 touched   0 citations   0
1000:0acc    bytes    15 branches   0 touched   0 citations   0
421 spans hold 698 of the 838
   24  1000:ced9..1000:d382     1000:ab59       1194 bytes
   22  1000:d8c9..1000:dc0d     1000:ab59        837 bytes
   21  1000:e590..1000:e947     1000:ab59        952 bytes
   17  1000:4169..1000:43f5     1000:3d11        653 bytes
   10  1000:3d12..1000:3dc6     1000:3d11        181 bytes
    9  1000:dd33..1000:ddf5     1000:ab59        195 bytes
    9  1000:dfcc..1000:e180     1000:ab59        437 bytes
    8  1000:ea95..1000:ec81     1000:ab59        493 bytes
    6  1000:1e07..1000:1e34     1000:1a03         46 bytes
    6  1000:e23f..1000:e36c     1000:ab59        302 bytes
    5  1000:0aec..1000:0d13     1000:0aec        552 bytes
    5  1000:2bc0..1000:2c52     1000:29c4        147 bytes
```

At `e657bbe` — and unchanged by the fix wave that followed it, which added no
`SEG:OFF` citation to `src/` — it printed, verbatim:

```
game branches 838 | touched 305 (36.4%) | uncited 533
1000:ab59    bytes 17143 branches 406 touched 107 citations 626
1000:3d11    bytes  6971 branches 224 touched  94 citations 611
1000:1a03    bytes  2700 branches  83 touched  54 citations 170
1000:6a0d    bytes  2527 branches  33 touched  15 citations 158
1000:29c4    bytes   666 branches  19 touched   2 citations  26
1000:0d14    bytes  1196 branches  17 touched  11 citations  53
1000:2526    bytes   929 branches  17 touched   9 citations  51
1000:7c67    bytes  1612 branches  16 touched  12 citations  39
1000:1348    bytes   791 branches  11 touched   1 citations  18
1000:0aec    bytes   552 branches   5 touched   0 citations   0
1000:074b    bytes   896 branches   2 touched   0 citations   4
1000:11c2    bytes   178 branches   2 touched   0 citations   1
1000:7538    bytes   580 branches   2 touched   0 citations  29
1000:5f55    bytes  1000 branches   1 touched   0 citations   4
1000:02c2    bytes   508 branches   0 touched   0 citations   0
1000:0acc    bytes    15 branches   0 touched   0 citations   0
416 spans hold 767 of the 838
   30  1000:c53b..1000:cb04     1000:ab59       1482 bytes
   24  1000:ced9..1000:d382     1000:ab59       1194 bytes
   22  1000:d8c9..1000:dc0d     1000:ab59        837 bytes
   21  1000:e590..1000:e947     1000:ab59        952 bytes
   17  1000:4169..1000:43f5     1000:3d11        653 bytes
   17  1000:bd22..1000:bf7f     1000:ab59        606 bytes
   13  1000:cb06..1000:ccd7     1000:ab59        466 bytes
   10  1000:3d12..1000:3dc6     1000:3d11        181 bytes
    9  1000:dd33..1000:ddf5     1000:ab59        195 bytes
    9  1000:dfcc..1000:e180     1000:ab59        437 bytes
    8  1000:ea95..1000:ec81     1000:ab59        493 bytes
    6  1000:1e07..1000:1e34     1000:1a03         46 bytes
```

At `3981f74` the first line is `touched 280 (33.4%) | uncited 558` and
`1000:1a03` reads `touched 29 citations 39`, which is where the +25 comes from.

### The `strings loaded inside` column of the span ranking

```bash
python3 - <<'EOF'
import json, sys, re
sys.path.insert(0, 'tools')
import dis16
img = open('orig/g.exe', 'rb').read()
d = json.load(open('data/branches.json'))
byoff = {e['off']: e for e in json.load(open('data/strings.json'))}
funcs = {f['entry']: f for f in d['functions']}
MOVDI = re.compile(r'^mov di,0x([0-9a-f]+)$')
def loads(start, end, entry):
    f = funcs[entry]
    out = []
    for i in dis16.decode_run(img, f['entry_file_off'], f['entry_file_off'] + f['size']):
        m = MOVDI.match(i.text) if 0x18d0 + start <= i.off <= 0x18d0 + end else None
        if m:
            e = byoff.get(int(m.group(1), 16) + 0x18d0)
            if e:
                out.append(e['text'])
    return out
# the seven spans the PREVIOUS revision of the table labelled -- the control
for s, e, fe, want in [(0x1a03,0x248e,'1000:1a03',62), (0xc53b,0xd382,'1000:ab59',83),
                       (0xbd22,0xc49a,'1000:ab59',49), (0xe39b,0xe947,'1000:ab59',35),
                       (0xd8c9,0xdce4,'1000:ab59',26), (0xdf11,0xe36c,'1000:ab59',28),
                       (0x7c67,0x82b2,'1000:7c67',42)]:
    n = len(loads(s, e, fe))
    print('%04x..%04x got %2d want %2d %s' % (s, e, n, want, 'OK' if n == want else 'MISMATCH'))
for s, e, fe in [(0xc53b,0xcb04,'1000:ab59'), (0xced9,0xd382,'1000:ab59'), (0xd8c9,0xdc0d,'1000:ab59'),
                 (0xe590,0xe947,'1000:ab59'), (0x4169,0x43f5,'1000:3d11'), (0xbd22,0xbf7f,'1000:ab59'),
                 (0xcb06,0xccd7,'1000:ab59'), (0x3d12,0x3dc6,'1000:3d11'), (0xdd33,0xddf5,'1000:ab59'),
                 (0xdfcc,0xe180,'1000:ab59'), (0xea95,0xec81,'1000:ab59'), (0x1e07,0x1e34,'1000:1a03')]:
    L = loads(s, e, fe)
    print('%04x..%04x %2d  %s' % (s, e, len(L), ' | '.join(L[:2])))
EOF
```

All seven controls print `OK`.

### The rest: guards, tiling, attribution, encoding

```bash
python3 - <<'EOF'
import json, collections
img = open('orig/g.exe','rb').read()
d   = json.load(open('data/branches.json'))
S   = json.load(open('data/strings.json'))
B   = d['branches']

# --- guard mnemonics (830 CMP / 98 OR; game 709 / 32) -----------------------
for cls in (None, 'game', 'rtl'):
    c = collections.Counter(b['guard']['mnemonic'] for b in B
                            if b['guard'] and (cls is None or b['class'] == cls))
    print(cls or 'all', sum(c.values()), c.most_common(4))

# --- OR guards that really do follow a Random call (22, all game) -----------
call = bytes.fromhex('9a4b11780f')                 # call far 0f78:114b
ors  = [b for b in B if b['guard'] and b['guard']['mnemonic'] == 'OR']
pre  = [b for b in ors
        if img[b['guard']['file_off']-5:b['guard']['file_off']] == call]
print('OR guards', len(ors), 'after a Random call', len(pre),
      collections.Counter(b['class'] for b in pre))

# --- CODE_0 undefined runs: Pascal tiling (11 of 17) and printability -------
runs = [r for r in d['undefined_runs'] if r['block'] == 'CODE_0']
exact = 0
for r in runs:
    p, end = r['file_off'], r['file_off'] + r['length']
    while p < end and p + 1 + img[p] <= end:
        p += 1 + img[p]
    exact += (p == end)
print('runs tiling exactly', exact, 'of', len(runs))
data = b''.join(img[r['file_off']:r['file_off']+r['length']] for r in runs)
pr = sum(1 for c in data if (c >= 0x20 and c != 0x7f) or c in (9, 10, 13))
print('printable %.2f%% of %d' % (100*pr/len(data), len(data)))

# --- attribution: 22714 attributed / 28 unaccounted / 252 suspect-only ------
undef = set()
for r in runs: undef.update(range(r['file_off'], r['file_off']+r['length']))
cover = collections.defaultdict(list)
for i, e in enumerate(S):
    for b in range(e['off'], e['off'] + 1 + len(e['text'])):
        if b in undef: cover[b].append(i)
un = sorted(undef - set(cover))
print('attributed', len(cover), 'unaccounted', len(un))
print('suspect-only bytes',
      sum(1 for b, ix in cover.items() if all(S[i]['suspect'] for i in ix)))
print('short(<8)-only bytes',
      sum(1 for b, ix in cover.items() if all(len(S[i]['text']) < 8 for i in ix)))
print('1-char-only bytes',
      sum(1 for b, ix in cover.items() if all(len(S[i]['text']) == 1 for i in ix)))
grp = []
for b in un:
    if grp and b == grp[-1][-1] + 1: grp[-1].append(b)
    else: grp.append([b])
print('unaccounted runs', len(grp),
      [(hex(g[0]), len(g)) for g in grp])

# --- strings.json entries that overlap a function body (10) ----------------
# NOTE: this line reads `size` as the span [entry, entry+size), which is the
# very trap described above -- `size` is a count of addresses in a body that
# may be non-contiguous, so a span misses the body's real extent in EITHER
# direction.  Two records in the image are non-contiguous, and they fail in
# opposite directions:
#   1f78:1117 OVER-reads -- it is the image's sole overlapping record, and
#     its phantom range 1f78:1121..112c contains no strings.json entry.
#   1000:0d14 UNDER-reads by 2 -- its span stops before 1000:11c0/11c1, the
#     operand bytes of the `c2 02 00` (`ret 0x2`) at 1000:11bf, and no
#     strings.json entry touches those two either.
# So the count is 10 either way and this line is safe HERE and only here.  An
# earlier revision of this note called 1f78:1117 "the sole split body in the
# image", which is one record too narrow: it is the sole OVERLAPPING one.
# Do not copy this line into a check
# where the over-read would matter -- use the address set, not the span.
iv = [(f['entry_file_off'], f['entry_file_off']+f['size']) for f in d['functions']]
print('entries overlapping a function body',
      sum(1 for e in S if any(e['off'] < t and e['off']+1+len(e['text']) > s0
                              for s0, t in iv)))

# --- address / encoding checks (0 / 0 / 1119 / 1119) -----------------------
flat = lambda a: int(a.split(':')[0],16)*16 + int(a.split(':')[1],16)
print('byte mismatches', sum(
    1 for b in B if img[b['file_off']:b['file_off']+len(bytes.fromhex(b['bytes']))]
                    != bytes.fromhex(b['bytes'])))
print('fallthrough segment mismatches', sum(
    1 for b in B if b['fallthrough'].split(':')[0] != b['addr'].split(':')[0]))
print('fallthrough linear-correct', sum(
    1 for b in B if flat(b['fallthrough']) == flat(b['addr']) + len(bytes.fromhex(b['bytes']))))
def rel8(b):
    y = bytes.fromhex(b['bytes'])[1]
    return y - 256 if y > 127 else y
print('taken rel8-correct', sum(
    1 for b in B if flat(b['taken']) == flat(b['addr']) + 2 + rel8(b)))
EOF
```

The wander cross-check (121 / 45 / 1 / 75 cited, 61 branches, 16 unmentioned):

```bash
python3 - <<'EOF'
import json, re
lo, hi = 0xae5a, 0xb3ba
blob = (open('docs/re/wander.md',encoding='utf-8').read()
        + open('data/wander.json',encoding='utf-8').read())
cited = {int(m,16) for m in re.findall(r'(?:1000:)?\b([0-9a-fA-F]{4})\b', blob)}
cited = {a for a in cited if lo <= a <= hi}
B = [b for b in json.load(open('data/branches.json'))['branches']
     if b['class'] == 'game']
off = lambda a: int(a.split(':')[1], 16)
rng = [b for b in B if lo <= off(b['addr']) <= hi]
br = {off(b['addr']) for b in rng}
gd = {off(b['guard']['addr']) for b in rng if b['guard']}
miss = [b for b in rng if off(b['addr']) not in cited
        and (b['guard'] is None or off(b['guard']['addr']) not in cited)]
print('branches', len(rng), '| cited', len(cited),
      '| guard', len(cited & gd), '| branch', len(cited & br),
      '| neither', len(cited - br - gd))
print('unmentioned', len(miss), '%.1f%%' % (100*len(miss)/len(rng)),
      ' '.join('%04x' % off(b['addr']) for b in miss))
EOF
```

The function-body disjointness argument (43,890 two ways):

```bash
python3 -c "
import json; d=json.load(open('data/branches.json'))
print('sum getNumAddresses', sum(f['size'] for f in d['functions']))
print('per-block walk     ', sum(b['bytes_in_functions'] for b in d['memory_blocks']))"
ndisasm -b16 -o 0x1117 -e 74087 orig/g.exe | head -8   # 1f78:1117 is non-contiguous
```

## For `docs/re/gaps.md` — item 1 is discharged

`docs/re/gaps.md` belonged to another task in flight when this document was
written, so nothing here was written into it. Two items were listed:

1. ~~**`FUN_1000_1a03` (2700 bytes, 83 branches) has no citation anywhere in the
   port.** … the single largest uncited branch cluster in the binary.~~
   **Discharged, and both clauses are now false.** Task 22 ported it into
   `src/character_sheet.rs`; the coverage block above prints
   `1000:1a03 … touched 54 citations 170`, and its largest remaining uncited
   span is 6 branches, rank 12. `docs/re/gaps.md` carries the Task 22 section
   (`grep -n 'FUN_1000_1a03' docs/re/gaps.md`). The largest uncited cluster in
   the binary is now the dealers' shop body, `1000:c53b`..`1000:cb04`, at 30.
2. **The uncited-span ranking above is the gap list's missing spine.** Still
   open. Whatever `gaps.md` lists, its coverage can be checked against the
   recomputed ranking: any span in the top 12 with no corresponding gaps.md
   entry is a gap in the gap list. Check it against the recomputation block, not
   against `data/branches.json`'s frozen `uncited_spans` — that array is a
   snapshot of `82a08d8` and its top entry has been ported since.
