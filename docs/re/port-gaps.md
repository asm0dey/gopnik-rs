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

**Next:** every row in this file is struck, and one follow-up flow survey has
run outside the list -- `FUN_1000_6a0d`, the save-load / new-game setup path,
picked as the weakest-verified body left. It found and fixed one divergence:
the stale start-up banner print recorded below.

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
