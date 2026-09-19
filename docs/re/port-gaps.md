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

That total is the surveys' own and is **not** recomputed when a row lands; the
`status` column is what says what is left. Rows 2, 4, 7, 13, 17 and 21 (~1,746 B
by the surveys' counts) are done -- that is the batch that made the game
finishable. Rows 1, 6, 8, 10, 15 and 16 (~2,437 B) are done too -- that is the
batch that gave it an opening: before it the port printed nothing at all of the
splash, the backstory, the district announcements, `help`, or the quit tail.

**Row 19 stays open even though those six rows' `ReadKey`s are now paced.** Its
~34 sites are spread across rows 3, 14, 23 and 24 as well, and only the sites
inside the six rows above have landed -- eight of them, all in
`crate::opening`'s gap tables. A row is struck when its whole span ships, not
when part of it does.

Status values: `open` · `done (<commit>)`. Add no other column.


| # | block | addresses | bytes | class | status |
|---:|---|---|---:|---|---|
| 1 | `1000:5f55` — the whole `help` text | `1000:5f64`..`633c` | 985 | MISSING | done (61f0f1c) |
| 2 | `FUN_1000_074b` — the end screen, both endings reach it | `1000:074b`..`0aca` | 896 | MISSING | done (fc0d0c7) |
| 3 | `1000:7c67` — the church's two long sermons | `1000:7ceb`..`7f63` | 622 | MISSING | open |
| 4 | `FUN_1000_0aec` — the victory marquee | `1000:0aec`..`0d13` | 552 | MISSING | done (fc0d0c7) |
| 5 | `1000:1348` — the enemy sheet's five halves | `1000:135c`..`165e` | ~546 | MISSING + PARTIAL | open |
| 6 | `FUN_1000_02c2` — the splash screen | `1000:02c2`..`04be` | 508 | MISSING | done (61f0f1c) |
| 7 | `1000:3d11` — the two rector openers + the two endings + the XP gates | `1000:3ead`..`3fa7`, `5085`..`5189`, `51a6`, `51f6` | ~534 | MISSING | done (fc0d0c7) |
| 8 | `1000:6de6` — the university backstory | `1000:6de6`..`6f2b` | 325 | MISSING | done (61f0f1c) |
| 9 | `1000:c329` — the market pickpocket, verb `t` | `1000:c329`..`c46a` | 321 | MISSING | open |
| 10 | `1000:7262` — the start-up district announcements + the district-1 tutorial | `1000:7262`..`7347`, `7369`..`73bb` | 311 | MISSING | done (61f0f1c) |
| 11 | `1000:b82f` — wander bucket 4 (and bucket 0's line) | `1000:b82f`..`b94a` | 283 | MISSING | open |
| 12 | `1000:b3c4` — bucket 1's eight flavour lines | `1000:b3db`..`b4ca` | ~279 | PARTIAL | open |
| 13 | `1000:11c2` — the two boss stat blocks have no caller | `1000:11c2`..`1274` | 178 | MISSING | done (fc0d0c7) |
| 14 | `1000:2526` — the level-up announcements | `1000:2591`..`28c0` | ~173 | PARTIAL | open |
| 15 | `1000:ad12` — the district 2/3/4 arrival announcements | `1000:ad12`..`adbf` | 173 | MISSING | done (61f0f1c) |
| 16 | `1000:ee04` — the quit tail (message + character sheet + `ReadKey`) | `1000:ee04`..`ee8b` | 135 | MISSING | done (61f0f1c) |
| 17 | `1000:57ce` — `param_1 == 6`, the den fight's reward block | `1000:57ce`..`5838` | 106 | MISSING | done (fc0d0c7) |
| 18 | `1000:7f8e` — the church's forced-level rank names | `1000:7f8e`..`7fe4` | 86 | PARTIAL | open |
| 19 | `1000:3eca` … — `ReadKey` pacing across the new text | ~34 sites | ~190 | PARTIAL | open |
| 20 | `1000:828c` — the church's parting line | `1000:828c`..`82af` | 35 | MISSING | open |
| 21 | `1000:3e8d` — `param_1 == 1`'s opener (reachable only once #9 lands) | `1000:3e8d`..`3ead` | 32 | MISSING | done (fc0d0c7) |
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
- ~~`FUN_1000_0aec`'s marquee semantics come from Ghidra's C only; this project
  has never disassembled it.~~ Disassembled with row 4: the phase wrap
  (`1000:0ce5`) and the 32-space indent bound (`1000:0b8f`) were decoded with
  `tools/dis16.py`, and `docs/re/gaps.md` records the one divergence the port
  keeps (the `until KeyPressed` loop bound).

**Why the branch metric is not used here.** `1a03` is 100% ported with 29
uncited branches; `3d11` has 2 uncited branches and ~670 unported bytes. It is
wrong in both directions on adjacent functions.

## Defects in shipped code the survey found in passing

- ~~`src/game.rs:5507` treats `1000:5133` as `FUN_1000_074b(1)`~~ — **FIXED**
  with rows 2/4/7. It is `call 0xaec`: the victory ending runs the marquee
  first, and `src/ending.rs` is the port of both.
- ~~`docs/re/branches.md:361` calls `FUN_1000_0acc` an ExitProc~~ — **FIXED**
  with row 4. It is data: 11 Pascal shortstrings spelling `ТЫ СУПЕР ГОП`,
  consumed by the `0aec` marquee, and `difftest.py`'s `marquee word` record
  now re-derives the word from the image.
- `dispatch2`'s gap 6 calls `DS:0b42` the "rank" table. It is `krutizna`
  (43 level rows); `ranks` is `DS:002e` (11 class rows).
- **`Game::apply_class_bonus` runs TWICE on a load**, found by landing row 10.
  `Game::new` calls it with the struct literal's `district: 1`, and
  `persist::from_save` calls it again once the loaded district is installed.
  That is harmless for idempotent stores and wrong for a `WriteLn`, so row 10's
  text went into a new `Game::announce_district`, called once from `main.rs`.
  `1000:734b`'s district-5 line moved out with it: it was printed from inside
  `apply_class_bonus` and came out right only because district 5 cannot be true
  on the `Game::new` pass -- an accident, not a property. Caught by
  `tests/district_advance_subprocess.rs`, which saw the district-1 announcement
  and then the district-2 one in a single load.


## Resuming

Phase 1 is complete: every game function surveyed, this list is its output.
Phase 2 is in progress — work the open rows, biggest first, one batch per
dispatch. Batch reports are in `.superpowers/sdd/phase2/` and the surveys in
`.superpowers/sdd/phase1-gap-survey/` (git-ignored, retained on disk).

**Landed so far:** batch A the endings (`fc0d0c7`, the game can be finished —
`difftest` 126→163), batch B the opening (`61f0f1c..ab98fea`, the game prints
its banner — `difftest` 163→255).

**Next:** batch C — the church (rows 3, 18, 20), the wander text (rows 11, 12,
22, 24), level-up (row 14). Then batch D the enemy sheet (row 5), batch E the
market pickpocket verb `t` (rows 9, 25, which also make row 21's opener
reachable). Row 19's `ReadKey` pacing spans several rows; land each row's share
with that row.

**Do not trust a string-coverage metric built on `data/strings.json`.** That
file covers one pool of 796 entries and does not contain the opening text — a
sweep over it reported the same 100 missing strings before and after batch B
ported the splash, the backstory and the whole of `help`. A blind
Pascal-shortstring walk of the image is noisier but real (659 Cyrillic strings,
161 absent from `src/` at `ab98fea`); the sound form anchors on `0x18d0 +
cs_off` from actual code references, the way the Phase 1 dispatch-2 survey did.

## Known defects not yet filed as rows

- `src/main.rs` prints `^4Gopnik: ^7version 1.02 june,sept 2003` at start-up.
  The original never prints it there — `1000:6dcd` only copies that string into
  `DS:369c` for the `v` verb.
- The victory marquee runs one 9-phase cycle; the original loops
  `until KeyPressed`, with `Delay`/`ClrScr`. Recorded in `docs/re/gaps.md`.
