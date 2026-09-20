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
Rows 3, 18, 20 and 14 (~916 B) are done as well -- the church's three sermons,
its composed `Был ты X а стал Y` line, its parting line, and the level-up
announcements, which were completely silent before. Rows 11, 12 and 22
(~592 B) are done too -- wander bucket 4's flavour turn (and bucket 0's
line, which `Game::walk`'s own doc had wrongly called silent), bucket 1's
eight district-keyed lines, and `run`'s own extra line, none of which
printed anything before. Row 5 (~546 B) is done as well, and it was the
largest single row left: the enemy sheet's крутизна suffix, its two
injury flags, its health-colour digit, its whole accuracy block, and the
armour gate that used to show every unarmoured opponent a `^2Броня 0`
line the original never prints. Rows 9 and 25 (~333 B) are done as well —
the market's pickpocket verb `t`, its three `Random` draws, its bust (a real
fight and the flag it sets), the police ban the bust leaves behind, the gate
that keeps a wanted player out of the market, and the `girl` clear that lifts
it. Landing row 9 also gave row 21's opener (`1000:3e8d`) its first caller:
`1000:c436` is the only `FUN_1000_3d11(1)` site in the image, so that arm had
been correct and unreachable since `fc0d0c7`. Row 23 (~15 B) is done as
well — the wandering mage's three `ReadKey`s, spacing the four lines it
already prints.

**Row 19 is done too, now that row 23 has landed.** Its ~34 sites were spread
across rows 23 and 24 as well. Those are, by batch:

* batch B, eight sites, in `crate::opening`'s gap tables (splash, backstory,
  quit tail);
* batch C part 1, twenty-two sites, in `crate::church`'s: `1000:7caf`/`7ce6`
  (the third-visit arm), `1000:7d0e`..`7dc2` (seven, the second),
  `1000:7dee`..`7f56` (eleven, the first, one of them after the composed
  line), `1000:7f89` (the forced level-up) and `1000:8242` (the convergence,
  which every draw-15 arm reaches);
* batch C part 2, three sites, `1000:b055`/`b092`/`b0b0` -- the phone gag
  (row 24), paced directly in `Game::wander_preamble`'s existing draw-3 arm;
* batch F, three sites, `1000:7560`/`757e`/`75a4` -- the wandering mage (row
  23), spacing the four lines `Game::mage` already printed. Unlike row 24's
  phone gag, these three are also a COMPARED fact, not just a ported one:
  `tools/difftest.py`'s `mage(img)` re-derives the same four-literal, three-
  `ReadKey` shape out of `orig/g.exe` by a `literal_walk` over
  `1000:7538`..`75c7`, and the port's own `--trace-deterministic` stream is
  diffed against it record for record.

Batches D and E add none. Row 5's whole span (`1000:135c`..`165e`), row 9's
(`1000:c329`..`c46a`) and row 25's refusal arm (`1000:c480`..`c499`) hold
**no** `ReadKey` at all, and that is compared rather than assumed:
`difftest.py`'s `enemy_gap` and `market_gap` sweeps run the same
`call 0f16:031a` scan over those spans and would put a `'K'` into a record if
one appeared. The image holds 59 such sites and none of them is in the three
spans.

Every one of row 19's sites has now landed: 8 + 22 + 3 + 3 = 36 against the
survey's `~34`; the tilde is the survey's own, and the row keeps it rather
than being quietly re-counted here. A row is struck when its whole span
ships, and both row 19 and row 23 do here -- the last two open rows in this
file.

Status values: `open` · `done (<commit>)`. Add no other column.


| # | block | addresses | bytes | class | status |
|---:|---|---|---:|---|---|
| 1 | `1000:5f55` — the whole `help` text | `1000:5f64`..`633c` | 985 | MISSING | done (61f0f1c) |
| 2 | `FUN_1000_074b` — the end screen, both endings reach it | `1000:074b`..`0aca` | 896 | MISSING | done (fc0d0c7) |
| 3 | `1000:7c67` — the church's two long sermons | `1000:7ceb`..`7f63` | 622 | MISSING | done (ecbeb9b) |
| 4 | `FUN_1000_0aec` — the victory marquee | `1000:0aec`..`0d13` | 552 | MISSING | done (fc0d0c7) |
| 5 | `1000:1348` — the enemy sheet's five halves | `1000:135c`..`165e` | ~546 | MISSING + PARTIAL | done (89fdb72) |
| 6 | `FUN_1000_02c2` — the splash screen | `1000:02c2`..`04be` | 508 | MISSING | done (61f0f1c) |
| 7 | `1000:3d11` — the two rector openers + the two endings + the XP gates | `1000:3ead`..`3fa7`, `5085`..`5189`, `51a6`, `51f6` | ~534 | MISSING | done (fc0d0c7) |
| 8 | `1000:6de6` — the university backstory | `1000:6de6`..`6f2b` | 325 | MISSING | done (61f0f1c) |
| 9 | `1000:c329` — the market pickpocket, verb `t` | `1000:c329`..`c46a` | 321 | MISSING | done (1ae22f6) |
| 10 | `1000:7262` — the start-up district announcements + the district-1 tutorial | `1000:7262`..`7347`, `7369`..`73bb` | 311 | MISSING | done (61f0f1c) |
| 11 | `1000:b82f` — wander bucket 4 (and bucket 0's line) | `1000:b82f`..`b94a` | 283 | MISSING | done (19055ab) |
| 12 | `1000:b3c4` — bucket 1's eight flavour lines | `1000:b3db`..`b4ca` | ~279 | PARTIAL | done (19055ab) |
| 13 | `1000:11c2` — the two boss stat blocks have no caller | `1000:11c2`..`1274` | 178 | MISSING | done (fc0d0c7) |
| 14 | `1000:2526` — the level-up announcements | `1000:2591`..`28c0` | ~173 | PARTIAL | done (ecbeb9b) |
| 15 | `1000:ad12` — the district 2/3/4 arrival announcements | `1000:ad12`..`adbf` | 173 | MISSING | done (61f0f1c) |
| 16 | `1000:ee04` — the quit tail (message + character sheet + `ReadKey`) | `1000:ee04`..`ee8b` | 135 | MISSING | done (61f0f1c) |
| 17 | `1000:57ce` — `param_1 == 6`, the den fight's reward block | `1000:57ce`..`5838` | 106 | MISSING | done (fc0d0c7) |
| 18 | `1000:7f8e` — the church's forced-level rank names | `1000:7f8e`..`7fe4` | 86 | PARTIAL | done (ecbeb9b) |
| 19 | `1000:3eca` … — `ReadKey` pacing across the new text | ~34 sites | ~190 | PARTIAL | done (5552482) |
| 20 | `1000:828c` — the church's parting line | `1000:828c`..`82af` | 35 | MISSING | done (ecbeb9b) |
| 21 | `1000:3e8d` — `param_1 == 1`'s opener (reachable now that #9 has landed) | `1000:3e8d`..`3ead` | 32 | MISSING | done (fc0d0c7) |
| 22 | `1000:aee4` — `run`'s own extra line | `1000:aee4`, `aeff` | ~30 | PARTIAL | done (19055ab) |
| 23 | `1000:7560` … — the mage's three `ReadKey`s | `1000:7560`, `757e`, `75a4` | ~15 | PARTIAL | done (5552482) |
| 24 | `1000:b055` … — the phone gag's three `ReadKey`s | `1000:b055`, `b092`, `b0b0` | ~15 | PARTIAL | done (19055ab) |
| 25 | `1000:b95e` / `1000:d793` — the market ban gate and the `girl` clear | 2 sites | ~12 | MISSING | done (1ae22f6) |

## Method and confidence

Surveys: `entry` (17,143 B), then `3d11`/`6a0d`/`7c67`/`1a03`/`11c2` (13,988 B),
then `0d14`/`5f55`/`2526`/`1348`/`7538`/`0acc` (4,511 B). `074b`/`0aec`/`02c2`
(1,956 B) were pulled in as gaps but never block-surveyed -- `074b` (896 B)
since has been, leaving `0aec` (552 B) and `02c2` (508 B). `29c4` (666 B) was
ported and audited before Phase 1. Every game function is accounted for.

**Weakest parts, stated by the surveys themselves:**

- The total carries about ±200 B. ~~`FUN_1000_074b` (896 B) is counted wholly
  unported on a grep, never block-surveyed by anyone.~~ **Block-surveyed and
  flow-diffed** -- see `## FUN_1000_074b, flow-diffed` below. The grep was
  right that it was unported when row 2 was written, and row 2's port
  (`fc0d0c7`) is a call-site-for-call-site match; the survey found nothing to
  change.
- The three dispatches used three different PARTIAL-counting conventions.
- ~~"PORTED" for the large shop/den/club/gym bodies rests on string presence
  and citation density, not a line-by-line flow diff.~~ **All four are now
  flow-diffed, and the bullet was right to doubt them: two of the four held
  an unported block.** Den 44/44 branches counterparted; club 20/20; gym
  38/38 only after porting the trained-armour recompute (`1000:e3a4`..`e3e2`,
  6 branches); shop 80/82, the missing 2 being row 9's extra menu gates
  (`1000:c824`, `1000:c82b`), ported with it. **The den is struck: the
  claim was already false when this file was written.** `docs/re/den.md`
  (`8b46249`) is a line-for-line flow map of `1000:d802`..`1000:df06` and
  `c352f3f` is its port, both landed 2026-08-30 -- three weeks before this
  file (`eb5b323`, 2026-09-19) asserted the opposite. A follow-up survey
  re-verified the range independently against `orig/g.exe` rather than
  against those docs: 44 of 44 branches have a counterpart (22 a literal
  `if`/`match` arm, 18 dissolved into a shared helper or the `match (loc,key)`
  table, 4 absorbed into a sibling conjunct, **0 with none**), all 5 `Random`
  draws match their `below_at` citation by `pushed-n`, and all 17 absolute
  `20ae:` writes have the same write in `src/`. Shop, club and gym are NOT
  struck -- no equivalent diff has been run on them.

  **Club and gym went the same way**, and for the same reason: `docs/re/club.md`
  (`08dfb09`) and `docs/re/gym.md` (`0f6749a`) are line-for-line flow maps
  dated 2026-09-06, two weeks before this file asserted none existed. A
  survey re-derived both ranges from `orig/g.exe`: club is 20 of 20 branches
  counterparted with **0** having none, and gym is 38 of 38 with **6** that
  had none -- all six the trained-armour recompute at `1000:e3a4`..`e3e2`,
  which was a registered open divergence rather than an unknown, and which
  that survey's dispatch closed. Their oracle position matches the den's:
  `difftest.py` carries `priced_row` / `imm_row_site` / `menu_order` records
  for the MENU rows only, nothing for the arm bodies, and the real
  port-behaviour coverage is the module-local unit tests in `src/club.rs`
  (14) and `src/gym.rs` (26).

  What the den does lack is an **oracle**: no `tools/difftest.py` record is
  scoped to its range (the only `--dump` lines matching "притон" are
  `opening_line help 21` and `23`, the help text). `data/den_arms.json`
  verifies the ARTIFACT against `orig/g.exe` and its own header says
  "Nothing here reads `src/`", so it is not a port-behaviour oracle.
  `tests/den_reveal_subprocess.rs` covers exactly two branches
  (`1000:dcbf`/`1000:dcc6`). Everything else rests on the in-process unit
  tests in `src/game.rs`.
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
  (43 level rows); `ranks` is `DS:002e` (11 class rows). **Confirmed by landing
  row 18**, whose composed line reads `DS:0b42` indexed by `[0x38a6]` (the
  level) at `1000:7f9e` while `1000:7ee9`, twelve instructions of the same
  routine earlier, reads `DS:002e` indexed by `[0x389c]` (the class). The two
  tables are used side by side in `FUN_1000_7c67` with different indices, so
  the mislabel would have put the wrong strings on screen.
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
- **`Game::walk`'s doc said wander bucket 0 "ends the turn with nothing"**,
  found by landing row 11. False: `1000:b92a`, the outer verb-dispatch
  chain's own mismatch arm, sits inside the SAME `1000:b82f`..`b94a` address
  range as bucket 4 and prints `Ничё не происходит.` unconditionally,
  through a CS reference separate from the one bucket 4 itself uses at
  `1000:b90f`. `wander::BUCKET4` now carries the string twice for that
  reason, and `Game::walk_verb`'s `_` (bucket 0) arm prints it.
- **This batch's own brief asserted bucket 4's four `Random` draws
  (`1000:b841`, `b871`, `b891`, `b8bd`) were "already ported"**; they were
  not -- `Game::walk` fell straight to its wildcard arm for bucket 4 and
  spent nothing. Caught only because the instruction said to verify against
  `src/game.rs` before writing anything, not because any test failed: none
  of the five `tests/wander_sequence.rs` captures stones the player on a
  bucket-4 turn, so the missing draws never desynchronised the RNG oracle --
  only the missing TEXT on the (overwhelmingly common) not-stoned path was
  visible before this batch.


## Resuming

Phase 1 is complete: every game function surveyed, this list is its output.
Phase 2 is in progress — work the open rows, biggest first, one batch per
dispatch. Batch reports are in `.superpowers/sdd/phase2/` and the surveys in
`.superpowers/sdd/phase1-gap-survey/` (git-ignored, retained on disk).

**Landed so far:** batch A the endings (`fc0d0c7`, the game can be finished —
`difftest` 126→163), batch B the opening (`61f0f1c..ab98fea`, the game prints
its banner — `difftest` 163→255), batch C part 1 the church and the level-up
(`ecbeb9b`, rows 3, 18, 20, 14 — `difftest` 255→314), batch C part 2 the
wander text (`19055ab`, rows 11, 12, 22, 24 — `difftest` 314→330), batch D the
enemy sheet (`89fdb72`, row 5 — `difftest` 330→347), batch E the market
pickpocket (`1ae22f6`, rows 9 and 25 — `difftest` 347→356, and row 21's
opener finally has a caller), batch F the wandering mage (`5552482`, row
23 and, with it, row 19 — `difftest` 356→363).

**Next:** every row in this file is struck, and follow-up flow surveys now run
outside the list, picked by weakest verification rather than by byte count.
Three have run:

1. `FUN_1000_6a0d`, the save-load / new-game setup path. One divergence found
   and fixed: the stale start-up banner print recorded below. Per-branch
   mapping in the next section.
2. `FUN_1000_074b`, the end screen -- the last function `## Method and
   confidence` still flagged as never block-surveyed. Clean; no `src/`
   change. Write-up in `## FUN_1000_074b, flow-diffed` below (`8cff3a0`).
3. `entry` slice 1, `1000:ab59`..`1000:b3b7` (2,142 B, 72 branches) -- the
   main-loop head and the district-promotion block. **One real gap found and
   ported** (`27def70`): the discovery-flag reset at `1000:ab96`..`1000:abc9`
   is not unconditional, and `Places::reset_for_new_district` was clearing
   all seven. `1000:aba5`, `1000:abb6` and `1000:abc7` were the only three
   branches in the slice with no Rust test of any kind; the other 69 have a
   counterpart (28 literal, 39 dissolved into a combined `&&` or an index, 2
   absorbed into a sibling's test). The same survey caught two stale
   "not reproduced" claims in `docs/re/command-dispatch.md` and
   `docs/re/gaps.md`, both fixed in `a124bc0` -- one of which also had a
   colour code wrong (`^4` for `^6`), which no oracle could have caught
   because `difftest.py` strips markup before comparing.

4. The **den handler** (`pr`, `1000:d802`..`1000:df06`, 1,796 B, 44
   branches), picked because `## Method and confidence` named the
   shop/den/club/gym bodies as the weakest "PORTED" claim in the file.
   Clean: 44 of 44 branches counterparted, 0 with none. The survey's real
   product is that the weakest-parts bullet itself was **wrong about the
   den** and has been narrowed -- see `## Method and confidence`.

5. **club + gym** in one dispatch (`1000:df06`..`e973`, 2,669 B, 58
   branches). Club clean 20/20; gym 38/38 only after **porting the
   trained-armour recompute** (`1000:e3a4`..`e3e2`, `4540de5`), whose six
   branches were the only ones in the range with no Rust test of any kind.
6. **shop / dealers** (`bmar`, `1000:c4be`..`d3a6`, 3,816 B, 82 branches).
   80/82; the missing two were **row 9's extra menu gates**
   (`1000:c824`, `1000:c82b`), ported in `74c4ab7`.
7. **market** (`mar`, `1000:b94a`..`c4be`, 2,932 B, 61 branches). Clean
   61/61. Its finding was a citation: three different instructions were all
   being called "the `ReadLn`" and one of them is the case-fold
   (`a6113f0`, anatomy in `docs/re/command-dispatch.md`).

8. **vet + girl + joint + small verbs** (2,319 B, 51 branches). Clean
   51/51. Product was four documents: `f`, `k` and `help` were all marked
   "not traced past `jz`" and all three are traced, and `docs/re/gaps.md`
   contradicted itself on the `^7 ` name prefix -- one entry calling it "not
   implemented" while another said the port adds it at the format boundary,
   which `persist::NAME_PREFIX` and `tests/save_roundtrip.rs` confirm it
   does (`54fb36f`).
9. **wander buckets** (`1000:b3b7`..`b94a`, 1,427 B, 37 branches), the last
   block of `entry`. Clean 37/37. Taken despite heavy capture coverage
   because this exact range had already hidden a real bug from the oracle
   once -- bucket 4's four draws were unspent and no capture stoned the
   player on a bucket-4 turn. Re-derived: all five `Random` sites now match,
   and **none of the five has ever fired in a live trace**.
   `difftest.py`'s wander spans are text-only and its gap scanner never
   checks for a `Random` at all, so those draws rest on the disassembly
   match plus module-local tests of the port's RNG against itself.

**`entry` is 100% flow-diffed** -- all 17,143 B across nine surveys.

**Where the GAME stands -- and what "no gaps" does and does not mean.** `entry` done; 13 functions and 17,698 B never
flow-diffed, 46% of the 38,264 B of game code:

```
python3 -c "import json;fns={f['name']:f['size'] for f in json.load(open('data/functions.json')) if f['entry'].startswith('1000:')};print(sum(fns.values()))"
# 38264
```

**All thirteen struck. Every one of the 38,264 bytes of game code has now
had at least a list-building pass, and the pass found zero gap rows.**

The last nine (`7c67` 1,612 · `5f55` 1,000 · `2526` 929 · `1348` 791 ·
`7538` 580 · `0aec` 552 · `02c2` 508 · `11c2` 178 · `0acc` 15 = 6,165 B)
came back clean, with four things checked from aligned disassembly rather
than from Ghidra's C:

* **The mage's unexamined 444 bytes.** `FUN_1000_7538` is 580 B and
  `difftest.py` only walks `7538`..`75c7`, so the rest had never been looked
  at by anything. It is the paid-save path -- debit `[0x38c7]`, write
  `save_r0.sav`, write the seven flags to `places.sav`, print the
  confirmation -- and it is ported, as `Game::mage` plus
  `persist::mage_save`. The narrow difftest span is a test-coverage note,
  not a missing port.
* **The marquee's loop bound.** `1000:0cf4 call 0f16:0308` (`KeyPressed`) /
  `or al,al` / `jnz 0xd00` / **`jmp 0xb2a`** -- the original really is an
  unbounded poll, and `src/ending.rs`'s nine frames really are the recorded
  port decision rather than a miscount.
* **The boss immediates** at `1000:11c2` re-derive `data/enemies.json`'s two
  rows exactly, derived fields included (666/666 and 1000/1000 hp).
* **The splash tail.** `1000:04b7` is `ClrScr` / `pop bp` / `ret` at
  `1000:04bd`; `04be` onward decodes as the next literal's bytes, not
  reachable code. Nothing lives past the span.

**`1a03`, `0d14` and `29c4` struck too -- the three functions with no
`difftest.py` span of any kind, 4,562 B, zero gap rows.** `1a03`'s ~83
branches (header, four stat digits, both charm sections, the three worn
singletons, the pistol block, the damage line's seven weapon arms, the health
line's four conditions, accuracy, armour, purse) all match
`src/character_sheet.rs`. `0d14`'s **14** `Random` draws are all spent by
`Game::roll_enemy` in the same order with the same `n` -- `1000:0d26`,
`0d70`, `0d91`, `0dcc`, `0ddd`, `0df0`, `0e04`, `0efd`, `102e`, `109c`,
`10c4`, `113c`, `1162`, `1197` -- which was the highest-severity thing
available in the batch, since a missing draw in the wander's opponent
generator corrupts every combat after it with no visible symptom. `29c4`'s
`h`/`mh` dispatch, broken-jaw refusal, spend-loop and healing split all
match, with `db9fd24`'s divergence still the only one.

**`FUN_1000_3d11` is struck: list-building pass, zero gap rows.** 6,971 B,
the largest function after `entry`. All 27 `Random` sites are spent by
`src/` (`combat.rs` `4497`/`46ba`/`4571`/`4794`, `combat_dispatch.rs`
`4db7`/`4e16`/`4ef5`/`4f18`, `game.rs`'s `crowd` and the friend-recruitment
rolls) and all 16 `ReadKey` sites sit inside already-ported opener and
ending blocks. It had been ported incrementally across ten modules without
anyone recognising it as one function-sized unit -- which is exactly why it
read as "never flow-diffed" while having nothing outstanding.

**These results are weaker evidence than the nine `entry` surveys and must
not be quoted as if they were the same.** A LIST-BUILDING pass reads the
decompilation and greps `src/` for each block's counterpart; it is not a
per-branch flow diff from aligned disassembly. The `1a03`/`0d14`/`29c4` pass
went further and says so plainly in its own report: it dropped to
`tools/re_query.py resolve` **zero times**, "since the decomp matched `src/`
on every branch checked". Ghidra's C being wrong is the standing assumption
in `CLAUDE.md`, so a pass that never leaves it inherits that risk whole.

What these passes settle is "is there bulk porting work here" -- no -- which
is what they were run to settle. What they do not settle is whether the
functions are correct. Those are different questions and this file keeps its
own habit of conflating them out of the record.

**And a note on how to work them, because the rate dropped.** 2026-09-19
landed 10 port commits and 4,591 `src/` insertions; 2026-09-20 landed 3 and
292. The difference is not effort: the first day WORKED THIS LIST, which
Phase 1 had already built, so each commit was read-a-row / write-the-Rust.
Once the last row was struck, every port needed a deep flow survey to find
its gap first -- and six of nine came back clean. That is paying discovery
cost per port instead of amortising it, which is the interleaving
`CLAUDE.md` warns about ("Verification parallelises; gating does not").
The remaining 13 functions get a LIST-BUILDING pass first -- gap rows only,
no per-branch tables, no doc archaeology -- and then the rows get worked in
bulk.

**The scoreboard, because three clean results in a row invite the wrong
conclusion.** Seven surveys: 3 found a real unported block, 4 came back
clean, and 6 turned up a stale or wrong document -- a claim of absence the
code refuted, a colour code no oracle compares, a blocker a later task had
removed, a table generated and read by nothing, and three instructions
sharing one name. The port's remaining risk is concentrated in the map, not
in `src/`. That is a statement about seven samples, not a law.

**Three of the four surveys came back clean.** That is worth stating plainly
rather than reading as a coverage figure: the one that did not
(`entry` slice 1) found a real gap, and two of the four found stale
documents instead -- a claim of absence that the code had already refuted.
On this evidence the port's remaining risk sits more in the map than in
`src/`.

**What that survey established, with a re-derivable per-branch mapping.**
`0e44787`'s "32 decision points" and "33 branches" were both eyeballed off
`build/decomp/FUN_1000_6a0d_1000_6a0d.c` and neither was checked against a
machine list, which is exactly the unbacked-count shape this project keeps
tripping on. The actual list is `data/branches.json`'s own `branches` array
(a plain disassembly enumeration of every conditional jump, not a citation
or coverage metric) filtered to this function:

```
python3 -c "import json;bs=[b for b in json.load(open('data/branches.json'))['branches'] if b['func']=='FUN_1000_6a0d'];print(len(bs))"
# 33
```

That gives 33 branch addresses, and every one of them was re-checked here
against the guard text `data/branches.json` records for it and the `src/`
construct that runs the same test:

| branch (jcc) | guard | test | `src/` counterpart |
|---|---|---|---|
| `6a94` | `6a8f cmp [3eb6],0` | more `FindNext` results | `persist::present_slots`'s `read_dir` iterator (structural replacement; Rust's iterator exhaustion stands in for the loop test, no explicit branch needed) |
| `6a9e` | `6a99 cmp [3d04],0` | slots-so-far != 0 | `persist::slot_menu_lines`: `if i > 0` |
| `6ac2` | `6abd cmp [3d2b],'0'` | digit != `'0'` | `slot_menu_lines`: the `else` arm |
| `6b0b` | `6b06 cmp [3d2b],'0'` | digit == `'0'` | `slot_menu_lines`: `if slot == '0'` |
| `6b38` | `6b33 cmp [3d04],0` | any slot found | `persist::choose_slot`: `if slots.is_empty()` |
| `6b63`/`6b6a`/`6b71`/`6b78`/`6b7f` | `6b5e`/`65`/`6c`/`73`/`7a cmp [3d31],'2'/'3'/'4'/'5'/'0'` | key match | `persist::SLOT_KEYS.contains(&k)` (one call for all five compares) |
| `6bdb` | `6bd9 or ax,ax` | slot file `Reset` `IOResult`==0 | `persist::load_slot`'s `read_slot(..)` `Ok`/`Err` match |
| `6c55` | `6c50 cmp [3692],0` | district==0 (slot `'0'`) | `load_slot`: `if slot == '0'` (the `places.sav` branch) |
| `6c93` | `6c91 or ax,ax` | `places.sav` `IOResult`==0 | `load_slot`: `Ok(b) if b.len() >= PLACES_BYTES` vs `_` |
| `6d4a`/`6d5b`/`6d6c` | `6d45`/`56`/`67 cmp [389c],3/3/5` | class-keyed spare of Club/Girl/Den in the `places.sav` failure arm | **not** three guarded clears in the port -- see note below |
| `6d91` | `6d8c cmp [3692],0` | district still 0 | `load_slot`: the same `if slot == '0'` covers this and `6c55` in one Rust test |
| `6ff5` | `6ff0 cmp [389c],4` | class answer == 4 | `main.rs::create_character`: `if answer == 4` |
| `7132`/`7139` | `712d`/`34 cmp [389c],0/3` | clamp answer to `0..=3` | `create_character`: `(0..=3).contains(&answer)` |
| `7146`/`7165`/`7184` | `7143`/`62`/`81 cmp ax,1/2/3` | class stat-quad dispatch | `progress::new_character`: `START_STATS[usize::from(answer)]` |
| `7225` | `7220 cmp [379c],0` | typed name's length byte == 0 | `create_character`: `if name.is_empty()` |
| `7267`/`72a0`/`72d9`/`7311`/`7349` | `7265`/`9e`/`d7`/`0f`/`47 cmp al,1/2/3/4/5` | district-keyed entry announcement | `Game::announce_district`: `START_ARRIVAL.chunks_exact(2).nth(district-1)` (1..4) + `if self.district == 5` |
| `736e` | `7369 cmp [3692],1` | district==1 | `announce_district`: `if self.district == 1` (the tutorial) |
| `73c1`/`73cd`/`73de` | `73be`/`ca`/`db cmp ax,5/3/6` | class-keyed bonus flags | `Game::apply_class_bonus`: `match self.player.class { 5 => .., 3 => .., 6 => .. }` |

33 rows, 33 branches, every one read against `src/` and given a stated
counterpart -- but "given a counterpart" is not "one Rust `if` per row", and
saying so without the breakdown would be the same overclaim in a new shape.
Of the 33:

* **14 have their own literal, one-to-one conditional in `src/`** -- a
  distinct `if`, `else`, or `match` arm that tests the same fact and nothing
  else: `6a9e`, `6ac2`, `6b0b`, `6b38`, `6bdb`, `6c55`, `6c93`, `6ff5`,
  `7225`, `7349`, `736e`, `73c1`, `73cd`, `73de`.
* **15 dissolve into a non-branching Rust construct** that covers several
  original branches at once -- a loop, a `.contains()`, a range test, or an
  array index, verified by reading what it does rather than by finding a
  matching `if`: the `FindNext` loop `6a94` (a `read_dir` iterator, no branch
  at all); the five-way key test `6b63`/`6b6a`/`6b71`/`6b78`/`6b7f`
  (`SLOT_KEYS.contains(&k)`, one call); the two-part clamp `7132`/`7139`
  (`(0..=3).contains(&answer)`); the three-way class-stat dispatch
  `7146`/`7165`/`7184` (`START_STATS[usize::from(answer)]`, an array index,
  not a compare); and four of the five district arms `7267`/`72a0`/`72d9`/
  `7311` (`chunks_exact(2).nth(district - 1)`; the fifth, `7349`, IS a
  literal `if self.district == 5` and is counted in the 14 above).
* **1 is absorbed into a different branch's own test**: `6d91` re-tests
  "is the district still 0", which `6c55` already tested earlier in the same
  function; the port reads that fact once, in `load_slot`'s single
  `if slot == '0'`, and `6d91` has no test of its own.
* **3 have no branch of their own anywhere** -- `6d4a`/`6d5b`/`6d6c`, the
  `places.sav` failure arm's three class-keyed spares (Club/Girl/Den). The
  port does not guard three clears; `load_slot`'s failure arm clears all
  seven `places.sav` flags unconditionally (`Places::from_bytes(&[0u8; 7])`),
  and `Game::apply_class_bonus` -- called unconditionally right after, from
  `persist::from_save`, and already counted above as the counterpart of
  `73c1`/`73cd`/`73de` -- re-marks exactly the same three locations for the
  same three classes. Both paths reach the same final state
  (`src/persist.rs`'s own doc on `load_slot` already says so); the port
  reuses branches `73c1`/`73cd`/`73de`'s effect instead of adding three more.

14 + 15 + 1 + 3 = 33. Every branch is accounted for by name; none of the 19
non-literal ones is silently counted as "covered" by the mere existence of
the table row -- the mechanism each one actually relies on is named above,
and a reader can re-open `src/persist.rs`, `src/main.rs` or `src/progress.rs`
at the cited construct and check it directly.

**Reconciling `branches.md:803`'s `touched: 15`, `data/branches.json`'s
`branches_touched_by_port: 6`, and this table's 33 -- three different
questions, not a disagreement.** `data/branches.json`'s per-function field
and `docs/re/branches.md`'s per-function column use the **identical**
metric (`docs/re/branches.md`, "Coverage against the port": a branch counts
as touched when its own address or its guard's address appears as a literal
`1000:XXXX` citation in `src/**/*.rs`); they differ only in **when** that
scan ran. `data/branches.json` froze its columns at `82a08d8`
(`docs/re/branches.md`'s own "Totals" paragraph says so); `branches.md`'s
`15` is a later recomputation against a tree with more citations accumulated
since. Recomputing the identical scan against the CURRENT tree reproduces
`branches.md`'s figure exactly, unmoved by this survey's own new citations
(neither `1000:6dcd` nor `1000:edb2`, the two addresses `0e44787`/this commit
added, is one of the 33 branch-or-guard addresses):

```
python3 -c "
import json, re, glob
B = [b for b in json.load(open('data/branches.json'))['branches'] if b['func'] == 'FUN_1000_6a0d']
blob = ''.join(open(p, encoding='utf-8').read() for p in glob.glob('src/*.rs'))
cited = {int(m, 16) for m in re.findall(r'1000:([0-9a-fA-F]{4})', blob)}
off = lambda a: int(a.split(':')[1], 16)
print(sum(1 for b in B if off(b['addr']) in cited or (b['guard'] and off(b['guard']['addr']) in cited)))"
# 15
```

Neither `6` nor `15` is "33" for the reason the 14/15/1/3 breakdown above
gives directly: **19 of the 33 branches were verified without ever writing
their own address next to a matching `src/` line**, because their
counterpart is a loop, a `.contains()`, an array index, or another branch's
effect -- none of which a literal-citation grep can find, by construction.
`docs/re/branches.md`'s own "Coverage against the port" section already
names this failure mode ("under-reports coverage... shop menu bodies show as
untouched even though part of their behaviour is modelled") for exactly this
reason; this function's 19 uncited-but-verified branches are the same
pattern, just larger. The 33-row table above was built by reading `src/`
directly, not by grepping for address strings, so `6` and `15` measuring
something else is expected, not a contradiction to resolve.

## `FUN_1000_074b`, flow-diffed -- row 2's grep caveat, discharged

Row 2 shipped at `fc0d0c7` but `## Method and confidence` flagged it as
"counted wholly unported on a grep, never block-surveyed by anyone". This
discharges that: an unbroken instruction sweep from the aligned entry
`1000:074b` to the `ret 0x2` at `1000:0ac8`, diffed against
`src/ending.rs::end_screen`. **No divergence; no `src/` change.**

**The two branches.** `data/branches.json` records exactly two conditional
jumps in the function, and both test the same byte:

```
python3 -c "import json;bs=[b for b in json.load(open('data/branches.json'))['branches'] if b['func']=='FUN_1000_074b'];print(len(bs));[print(b['addr'],b['guard']['addr'],b['guard']['text']) for b in bs]"
# 2
# 1000:0763 1000:075f CMP byte ptr [BP + 0x4],0x0
# 1000:0791 1000:078d CMP byte ptr [BP + 0x4],0x0
```

| branch (jcc) | guard | test | `src/` counterpart |
|---|---|---|---|
| `0763` | `075f cmp byte [bp+4],0` | `param_1 == 0` -> banner colour digit | `end_screen`: `let digit = if victory { '2' } else { '4' }` |
| `0791` | `078d cmp byte [bp+4],0` | `param_1 == 0` -> verdict line | `end_screen`: `if victory { VICTORY_LINE } else { DEATH_LINE }` |

2 rows, 2 branches, both literal one-to-one conditionals -- the 14/15/1/3
breakdown the `FUN_1000_6a0d` table needed has no analogue here because there
is nothing to dissolve. `param_1` is read at `1000:075f` and `1000:078d` and
nowhere else in the function, so those two `if`s are the whole of its effect.

**A two-row table proves almost nothing about a 896-byte function**, which is
the point the `6a0d` write-up makes about counting coverage off branches: this
function is straight-line output. The real inventory is its call sites, and
they are enumerated rather than eyeballed:

```
python3 -c "
import sys, collections; sys.path.insert(0,'.')
from tools import dis16, difftest as D
img=D.load(); off=0x074b; c=collections.Counter()
while off<0x0acb:
    i=dis16.decode(img,off)
    if i.text.startswith('call'): c[i.text]+=1
    off+=i.length
print(sum(c.values())); [print('%3d  %s'%(n,t)) for t,n in c.most_common()]"
# 78
#  16  call 0xf78:0xb66     rtl_str_append
#  14  call 0xf78:0x5dd     WriteLn (bare -- writes the CR/LF pair, no argument)
#  14  call 0xf78:0x291     rtl_io_check, the {$I+} half of each WriteLn
#  11  call 0xeed:0x1c2     the game's colour-markup WriteLn
#   8  call 0xf78:0xae7     rtl_str_assign
#   8  call 0xf78:0xc03     rtl_char_to_str
#   2  call 0xf16:0x1cc     ClrScr
#   2  call 0xf16:0x263     TextColor
#   1  call 0xf78:0x2cd     rtl_stack_check
#   1  call 0xf16:0x31a     ReadKey
#   1  call 0xf78:0x116     rtl_halt
```

Names are `docs/re/rtl.md`'s, keyed by that file's `1fXX` = `0fXX` convention.
`0f78:05dd` being a BARE `WriteLn` is the one identity the structure depends
on, and it is `re_query.py`'s own evidence string, not an inference:
`0f78:05dd writes exactly 2 bytes from 20ae:3690 through 0f78:0546: the CR/LF
pair`. Every one of the 14 is preceded only by `push ds` / `push di` with
`di = 0x3fcc`, the output `Text` variable -- no string argument exists to push.

**The output sequence, in address order**, which is what `end_screen` must
reproduce:

| addresses | effect | `src/ending.rs` |
|---|---|---|
| `0751` | `rtl_stack_check` | none needed |
| `075a` | `ClrScr` | dropped, documented |
| `0765` / `076b` | `mov byte [bp-1],0x34` / `0x32` | `digit` |
| `0774`, `0783` | 2 blank `WriteLn` | `term::println("")` x2 |
| `07a7` / `07c2` | verdict line, CS `0x4be` / `0x4ef` | `DEATH_LINE` / `VICTORY_LINE` |
| `07cc`, `07db`, `07ea` | 3 blank | `for _ in 0..3` |
| `0831`..`09ff` | 8 banner rows | `for row in BANNER` |
| `0a09`, `0a18`, `0a27`, `0a36`, `0a45` | 5 blank | `for _ in 0..5` |
| `0a63` | CS `0x715` | `ANY_KEY` |
| `0a6d`, `0a7c`, `0a8b`, `0a9a` | 4 blank | `for _ in 0..4` |
| `0aa4` | `TextColor(0)` | dropped, documented |
| `0aac` | `ReadKey` | `term::read_key` |
| `0ab1` | `TextColor(15)` | dropped, documented |
| `0ab9` | `ClrScr` | dropped, documented |
| `0abe`/`0ac0` | `xor ax,ax` / `call 0f78:0116` = `Halt(0)` | `self.running = false` in the caller |
| `0ac5`..`0ac8` | epilogue | unreachable behind `Halt` |

The blank runs are 2 / 3 / 5 / 4 = 14, matching the call census above and
`end_screen`'s four loops exactly.

**Ghidra was wrong about the two `TextColor` arguments and the disassembly
settles it.** `build/decomp/FUN_1000_074b_1000_074b.c` renders them
`(uint)extraout_AH_07 << 8` and `CONCAT11(extraout_AH_08,0xf)`, which reads as
if the high byte carried information. It does not -- `TextColor` takes a
`Byte` and reads only `AL`, so Ghidra never pins `AH`:

```
python3 tools/re_query.py resolve 1000:0aa4 -n 16 -i 8
# 1000:0aa4  mov al,0x0
# 1000:0aa6  push ax
# 1000:0aa7  call 0xf16:0x263
# 1000:0aac  call 0xf16:0x31a
# 1000:0ab1  mov al,0xf
# 1000:0ab3  push ax
# 1000:0ab4  call 0xf16:0x263
```

`src/ending.rs`'s doc already said `TextColor(0)` and `TextColor(15)`; this is
the first time those two values were read off an aligned instruction rather
than off the decompilation.

**The twelve string literals are byte-identical** to `DEATH_LINE`,
`VICTORY_LINE`, `BANNER_INDENT`, `BANNER[0..8]` and `ANY_KEY`, compared as
CP866 shortstrings at CS `0x04be`, `0x04ef`, `0x0521`, `0x052d`, `0x056a`,
`0x05a7`, `0x05e4`, `0x0621`, `0x065e`, `0x069b`, `0x06d8`, `0x0715`. Eleven of
the twelve were already COMPARED facts before this survey -- `difftest.py`
re-derives them as `ending_line endscreen 0..2`, `end_banner indent`, both
colour digits and `banner_row 0..7`. The survey adds `0x0521`'s own bytes and
the row-assembly ORDER, which no record covered.

**What this survey did NOT establish.** The 14 blank `WriteLn`s, the `ReadKey`
at `0aac`, both `ClrScr`s, both `TextColor`s and the `Halt` are read off the
disassembly here but are compared by no oracle: of the 78 call sites, the 12
that feed a `difftest` record are the 11 `0eed:01c2` prints plus the first
`rtl_str_assign`. A regression that deleted one blank `WriteLn` from
`end_screen` would pass `difftest` and every test in `tests/`, which holds no
end-screen case at all. Left as a deferred-minor: the structure is now read,
and the row it belonged to is closed.

The next Phase 2 work is whatever a fresh gap survey finds outside this
list, per `docs/re/gaps.md`.

**Do not trust a string-coverage metric built on `data/strings.json`.** That
file covers one pool of 796 entries and does not contain the opening text — a
sweep over it reported the same 100 missing strings before and after batch B
ported the splash, the backstory and the whole of `help`. A blind
Pascal-shortstring walk of the image is noisier but real (659 Cyrillic strings,
161 absent from `src/` at `ab98fea`); the sound form anchors on `0x18d0 +
cs_off` from actual code references, the way the Phase 1 dispatch-2 survey did.

## Known defects not yet filed as rows

- ~~`src/main.rs` prints `^4Gopnik: ^7version 1.02 june,sept 2003` at
  start-up.~~ **Fixed by the `FUN_1000_6a0d` survey.** The original never
  prints it there: `1000:6dcd` copies that string silently into `DS:369c`,
  the save record's `magic` slot (`docs/re/save-format.md`), with no console
  call at all -- not "for the `v` verb" as this bullet used to say. The
  `version` verb's on-screen text is a THIRD, separate copy of the same
  string at `1000:edb2` (file `0xC3C1`), read only on demand
  (`src/commands.rs`'s `version` row, corrected in the same pass). `main.rs`
  no longer prints it at start; `Game::banner` (the `version` verb) is
  unaffected.
- The victory marquee runs one 9-phase cycle; the original loops
  `until KeyPressed`, with `Delay`/`ClrScr`. Recorded in `docs/re/gaps.md`.
