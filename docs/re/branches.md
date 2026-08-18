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

"Inside a known string" means the byte falls in `[off, off + 1 + len)` for some
entry in `data/strings.json`, whose `off` is the Pascal length byte (checked:
796 of 796 entries have `img[off] == len(text)`).

**The game's own code segment is 99.88% accounted for.** Of the 22,742 bytes of
`CODE_0` outside any function, 22,714 are string-literal pool — Borland put the
main unit's literals in the code segment, interleaved with code, which is
exactly why Ghidra stops there. Only **28 bytes** of `CODE_0`, spread over eight
runs, are neither code Ghidra recognised nor a string it extracted; the largest
single unaccounted stretch is 8 bytes, inside the 17-byte run at `1000:0adb`
that sits between the end of `FUN_1000_0acc` and the start of `FUN_1000_0aec`.
A `jcc` is two bytes, so those 28 bytes could in principle hide a handful of
branches; they could not hide a routine. So the residual risk of missed *game*
branches is a few bytes, not 22 KB.

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

Ghidra loads the image at segment `0x1000`, so for a Ghidra address `SEG:OFF`:

```
file_off  = 0x18d0 + (SEG - 0x1000) * 16 + OFF
real DOS seg = SEG - 0x1000        # Ghidra 1f78:114b == the docs' 0f78:114b
```

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
* what to work on next — sort `uncited_spans` by `branch_count`

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
(`distance: 1`). Guard mnemonics are `CMP` 816, `OR` 97 (the `or ax,ax` after a
`Random` call), then `DEC`/`SUB`/`ADD`/`TEST`/… . `join_crossed: true` (4 cases,
all RTL) means the walk stepped over an address something else jumps to, so the
guard holds on the fall-through path only.

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

**2. The whole wander turn, `1000:ae5a`..`1000:b3ba`.** Taking every `1000:xxxx`
address cited by `docs/re/wander.md` plus `data/wander.json` in that range — 118
distinct addresses:

* 43 are the **guard** of a branch the script found,
* 1 is a **branch** address (`1000:b279`, from the quoted disassembly above),
* 74 are neither, and every one of them is a non-branch site the documents cite
  for another reason — `Random` call sites (`af68`, `b030`, `b353`, …), flag
  stores (`af71`, `b196`, `b570`), `call` sites (`b3a7`, `b3ae`), `mov`s.

So the hand-recovered document cites **conditions** (the `cmp`), where this
artifact keys on the **branch** — different addresses for the same decision, and
every documented decision is present. **No documented branch is missing from the
enumeration.**

The reverse direction is the interesting one: the script finds **61** branches
in that range, and **18 of them** — `ae8b ae9c aeb1 aee9 af17 af29 af30 b12a
b151 b17c b258 b266 b2db b30b b30d b311 b36d b380` — are mentioned by neither
`wander.md` nor `wander.json`, at the branch address or at the guard. That is
30% of the branches in the most carefully documented region of the game. Not an
error in `wander.md`, which set out to catalogue the `Random` sequence and does
that correctly; it is the measure of what mechanical enumeration adds.

Spot-check of three of those 18 against `ndisasm` (independent disassembler,
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

**Totals.** 838 game branches; **84 touched (10.0%)**; 754 with no citation at
the branch or its guard. `docs/re/*.md` was deliberately excluded from the scan:
documented is not ported.

| entry | bytes | branches | touched | citations | note |
|-------|------:|---------:|--------:|----------:|------|
| `1000:ab59` `entry` | 17143 | 406 | 45 | 229 | the top-level body; most of the game |
| `1000:3d11` | 6971 | 224 | 21 | 67 | combat (identified in `docs/re/combat.md`) |
| `1000:1a03` | 2700 | **83** | **0** | **0** | — |
| `1000:6a0d` | 2527 | 33 | 6 | 71 | |
| `1000:29c4` | 666 | 19 | 2 | 23 | |
| `1000:0d14` | 1196 | 17 | 0 | 1 | rolls the enemy (`src/game.rs`) |
| `1000:2526` | 929 | 17 | 9 | 31 | |
| `1000:7c67` | 1612 | **16** | **0** | **0** | the church |
| `1000:1348` | 791 | 11 | 1 | 15 | |
| `1000:0aec` | 552 | 5 | 0 | 0 | |
| `1000:074b` | 896 | 2 | 0 | 4 | |
| `1000:11c2` | 178 | 2 | 0 | 0 | |
| `1000:7538` | 580 | 2 | 0 | 1 | the mage's paid save |
| `1000:5f55` | 1000 | 1 | 0 | 3 | |
| `1000:02c2` | 508 | 0 | 0 | 0 | title screen |
| `1000:0acc` | 15 | 0 | 0 | 0 | |

### The ranking: largest uncited spans by branch count

191 spans hold 822 of the 838 game branches. The top 12, with what each span
*loads string pointers to* — established from flow (`mov di,imm`; file offset =
`imm + 0x18d0`, cross-referenced against `data/strings.json`). The span
boundaries are mechanical; the labels are an **inference from those string
pointers**, not a traced dispatch.

| rank | span | function | bytes | branches | strings loaded inside suggest |
|-----:|------|----------|------:|---------:|-------------------------------|
| 1 | `1000:1a03`..`1000:248e` | `FUN_1000_1a03` | 2700 | 83 | the status screen — `Ты # уровня`, `Сл:`, `Феньки:`, `Крестик(Удача +2)` (62 string loads) |
| 2 | `1000:c53b`..`1000:d382` | `entry` | 3656 | 79 | the dealers' shop — `# руб. Косяк`, `# руб. Краденый мобильник(Подмога быстрее приходит)` (83 string loads) |
| 3 | `1000:52b4`..`1000:584b` | `FUN_1000_3d11` | 1432 | 65 | tail of the combat routine |
| 4 | `1000:bd22`..`1000:c49a` | `entry` | 1913 | 46 | eating — `Ты сожрал хот-дог`, `Ты не можешь хавать из-за сломаной челюсти.` (49 string loads) |
| 5 | `1000:e39b`..`1000:e947` | `entry` | 1453 | 37 | the gym — `Ты пришел в качалку`, `качаться гателями и шгангой(Сила +1)` (35) |
| 6 | `1000:4b01`..`1000:4f81` | `FUN_1000_3d11` | 1153 | 36 | middle of the combat routine |
| 7 | `1000:af05`..`1000:b195` | `entry` | 657 | 28 | the wander preamble — already a known gap |
| 8 | `1000:d8c9`..`1000:dce4` | `entry` | 1052 | 27 | the den — `Пацаны хотят тебе кое-чё сказать`, `p чтобы угостить пацанов пивом` (26) |
| 9 | `1000:40b7`..`1000:445b` | `FUN_1000_3d11` | 933 | 24 | combat |
| 10 | `1000:df11`..`1000:e36c` | `entry` | 1116 | 19 | the club — `Ты пришел в клуб`, `Здесь можно сыграть в карты`, `потусоваться на дискотеке(Ловкость +1)` (28) |
| 11 | `1000:0d15`..`1000:11bf` | `FUN_1000_0d14` | 1195 | 17 | enemy generation; loads no strings |
| 12 | `1000:7c67`..`1000:82b2` | `FUN_1000_7c67` | 1612 | 16 | the church — `Ты наткнулся на храм Божий.`, `Бог: "А ты опять."` (42) |

Next after those: `1000:d479`..`1000:d6c9` (15, the vet — `7 рублей починят
переломы`), `1000:3d12`..`1000:3dc6` (10), `1000:dcfc`..`1000:dee2` (10,
stealing — `Ты пришел воровать деньги`, `Шухер менты!`), `1000:ea95`..`1000:ec81`
(8, the help text).

Defensible order of work, from this table alone: the status screen
(`FUN_1000_1a03`, 83 branches, not one citation anywhere in the port), then the
dealers' shop body, then the two large uncited stretches of the combat routine,
then eating / gym / den / club as a block of location menus.

## For `docs/re/gaps.md` — fold in later

`docs/re/gaps.md` belongs to another task in flight, so nothing here was written
into it. Two items belong there:

1. **`FUN_1000_1a03` (2700 bytes, 83 branches) has no citation anywhere in the
   port.** The string pointers it loads are the status-screen texts. It is the
   single largest uncited branch cluster in the binary.
2. **The uncited-span ranking above is the gap list's missing spine.** Whatever
   `gaps.md` lists, its coverage can now be checked against
   `data/branches.json`: any span in the top 12 with no corresponding gaps.md
   entry is a gap in the gap list.
