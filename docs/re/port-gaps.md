# Port gaps — the Phase 1 survey's work list

**This is a checklist, not a map.** Every row is behaviour in `orig/g.exe` with
no counterpart in `src/`, found by the three Phase 1 gap surveys. A Phase 2 task
that lands a row **strikes it in the same commit**. A row left standing after
its behaviour ships is the `what_the_port_must_change` defect
(`docs/re/gaps.md`) repeated, so do not let this file outlive its rows.

Totals below are the surveys' own, recomputed by them; see
`## Method and confidence`.

**~7,244 unported bytes of 38,249 game-code bytes in 15 functions = 18.9%.**
Rows 1-10 are 77% of it.

Status values: `open` · `done (<commit>)`. Add no other column.


| # | block | addresses | bytes | class | status |
|---:|---|---|---:|---|---|
| 1 | `1000:5f55` — the whole `help` text | `1000:5f64`..`633c` | 985 | MISSING | open |
| 2 | `FUN_1000_074b` — the end screen, both endings reach it | `1000:074b`..`0aca` | 896 | MISSING | open |
| 3 | `1000:7c67` — the church's two long sermons | `1000:7ceb`..`7f63` | 622 | MISSING | open |
| 4 | `FUN_1000_0aec` — the victory marquee | `1000:0aec`..`0d13` | 552 | MISSING | open |
| 5 | `1000:1348` — the enemy sheet's five halves | `1000:135c`..`165e` | ~546 | MISSING + PARTIAL | open |
| 6 | `FUN_1000_02c2` — the splash screen | `1000:02c2`..`04be` | 508 | MISSING | open |
| 7 | `1000:3d11` — the two rector openers + the two endings + the XP gates | `1000:3ead`..`3fa7`, `5085`..`5189`, `51a6`, `51f6` | ~534 | MISSING | open |
| 8 | `1000:6de6` — the university backstory | `1000:6de6`..`6f2b` | 325 | MISSING | open |
| 9 | `1000:c329` — the market pickpocket, verb `t` | `1000:c329`..`c46a` | 321 | MISSING | open |
| 10 | `1000:7262` — the start-up district announcements + the district-1 tutorial | `1000:7262`..`7347`, `7369`..`73bb` | 311 | MISSING | open |
| 11 | `1000:b82f` — wander bucket 4 (and bucket 0's line) | `1000:b82f`..`b94a` | 283 | MISSING | open |
| 12 | `1000:b3c4` — bucket 1's eight flavour lines | `1000:b3db`..`b4ca` | ~279 | PARTIAL | open |
| 13 | `1000:11c2` — the two boss stat blocks have no caller | `1000:11c2`..`1274` | 178 | MISSING | open |
| 14 | `1000:2526` — the level-up announcements | `1000:2591`..`28c0` | ~173 | PARTIAL | open |
| 15 | `1000:ad12` — the district 2/3/4 arrival announcements | `1000:ad12`..`adbf` | 173 | MISSING | open |
| 16 | `1000:ee04` — the quit tail (message + character sheet + `ReadKey`) | `1000:ee04`..`ee8b` | 135 | MISSING | open |
| 17 | `1000:57ce` — `param_1 == 6`, the den fight's reward block | `1000:57ce`..`5838` | 106 | MISSING | open |
| 18 | `1000:7f8e` — the church's forced-level rank names | `1000:7f8e`..`7fe4` | 86 | PARTIAL | open |
| 19 | `1000:3eca` … — `ReadKey` pacing across the new text | ~34 sites | ~190 | PARTIAL | open |
| 20 | `1000:828c` — the church's parting line | `1000:828c`..`82af` | 35 | MISSING | open |
| 21 | `1000:3e8d` — `param_1 == 1`'s opener (reachable only once #9 lands) | `1000:3e8d`..`3ead` | 32 | MISSING | open |
| 22 | `1000:aee4` — `run`'s own extra line | `1000:aee4`, `aeff` | ~30 | PARTIAL | open |
| 23 | `1000:7560` … — the mage's three `ReadKey`s | `1000:7560`, `757e`, `75a4` | ~15 | PARTIAL | open |
| 24 | `1000:b055` … — the phone gag's three `ReadKey`s | `1000:b055`, `b092`, `b0b0` | ~15 | PARTIAL | open |
| 25 | `1000:b95e` / `1000:d793` — the market ban gate and the `girl` clear | 2 sites | ~12 | MISSING | open |

## Method and confidence

Surveys: `entry` (17,143 B), then `3d11`/`6a0d`/`7c67`/`1a03`/`11c2` (13,988 B),
then `0d14`/`5f55`/`2526`/`1348`/`7538`/`0acc` (4,511 B). `074b`/`0aec`/`02c2`
(1,956 B) were pulled in as gaps but never block-surveyed. `29c4` (666 B) was
ported and audited before Phase 1. Every game function is accounted for.

**Weakest parts, stated by the surveys themselves:**

- The total carries about ±200 B. `FUN_1000_074b` (896 B) is counted wholly
  unported on a grep, never block-surveyed by anyone.
- The three dispatches used three different PARTIAL-counting conventions.
- "PORTED" for the large shop/den/club/gym bodies rests on string presence and
  citation density, not a line-by-line flow diff.
- `FUN_1000_0aec`'s marquee semantics come from Ghidra's C only; this project
  has never disassembled it.

**Why the branch metric is not used here.** `1a03` is 100% ported with 29
uncited branches; `3d11` has 2 uncited branches and ~670 unported bytes. It is
wrong in both directions on adjacent functions.

## Defects in shipped code the survey found in passing

- `src/game.rs:5507` treats `1000:5133` as `FUN_1000_074b(1)`. It is
  `call 0xaec` — the victory ending runs the marquee first.
- `docs/re/branches.md:361` calls `FUN_1000_0acc` an ExitProc called from
  `1f78:0000`. It is data: 11 Pascal shortstrings spelling `ТЫ СУПЕР ГОП`,
  consumed by the `0aec` marquee. `docs/re/gaps.md` already has it right; the
  two contradict.
- `dispatch2`'s gap 6 calls `DS:0b42` the "rank" table. It is `krutizna`
  (43 level rows); `ranks` is `DS:002e` (11 class rows).

