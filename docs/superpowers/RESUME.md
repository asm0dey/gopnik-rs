# Resume checkpoint — gopnik-rs port

**Branch: `re/task-19-save-unknowns`, cut from `main` @ `3eb7c74`.** Last
updated after **Task 19**. Tasks 16–18 landed on
`re/task-17-combat-dispatch` and are merged into `main`. The older
feature branches (`port/gopnik-rust` @ `f2d4fce`, `fix/task-13-review`,
`feat/mutation-gate`) are history; `main` carries everything up to Task 15 and
PR #1 is merged, and Tasks 16, 17 and 18 live on this branch pending its
merge. So this file is not a checkpoint any more — it is the **handover**, and
whoever picks the project up reads it instead of asking.

`.superpowers/sdd/progress.md` is the full ledger but is **git-ignored** — a
`git clean -fdx` destroys it. This file is the committed backup. If they
disagree, trust `git log`.

---

## READ THIS FIRST

Everything is green: **231 Rust** (204 after Task 18), and for Python
**404 by `unittest`** (366 after Task 18; Task 19 added 16 in
`tools/test_decode_save.py`, 9 in the new `tools/test_savegen.py`, 9 in the
new `tools/test_branch_reach.py` and 4 in the new
`tools/test_string_citations.py`) —

```
python3 -m unittest discover -s tools -p 'test_*.py'    -> Ran 404 tests, OK
.venv/bin/pytest tools/ -q                              -> 418 passed, 3 skipped, 668 subtests
                                                          (380 + 3 after Task 18)
```

**State the runner with the number, always.** The two disagree by design, both
are right, and the whole gap is one thing: pytest also collects the **17
module-level `def test_*` functions across 6 files** that `unittest` cannot see
(it only gathers `TestCase` subclasses — those functions still run, via each
file's `__main__` block). 9 are in `tools/oracle/test_oracle_smoke.py`, 4 in
`tools/test_extract_tables.py`, and one each in `test_decode_save.py`,
`test_extract_strings.py`, `test_string_pointers.py`, `test_string_tables.py`.
`418 + 3 = 421` collected, `421 − 404 = 17`, exactly — the same 17, because
Task 19's new tests are all `TestCase` methods that `unittest` does collect. Subtests are NOT part of
the difference: pytest reports the 668 separately, on its own line, and does not
fold them into the 380. Re-measure with
`git grep -c '^def test_' -- 'tools/test_*.py' 'tools/*/test_*.py'`
(6 files, 17 hits — the single-`*` pathspecs are load-bearing: `tools/**/…`
misses the flat `tools/test_*.py` and returns 9). `unittest discover` is this
project's documented method, from the Task 12 report, so **it is the number of
record**.

This block previously read "309 Python (3 skipped)" with no runner named. That
was a pytest figure published into a handover document as if it were the
project's own count. It was **reproducible** — `309 + 3 skipped = 312`
collected, under the exact command it omitted — and the Task 16 report's claim
that it "could not be reproduced by any invocation" was itself wrong; what was
missing was the runner, not the number. A number without its method is the defect class this
project keeps finding, committed into the file whose job is to prevent it.

Also green:
`python3 tools/difftest.py` exit 0 / 126 records, `python3 tools/mutate.py`
exit 0 with **32 red** + 10 findings over **134 guarded files** (29 red / 126
files after Task 18; the file count is derived from the tree, and Task 19 added
`tools/savegen.py`, `tools/test_savegen.py`, `tools/branch_reach.py`,
`tools/test_branch_reach.py`, `tools/rngtrace/saveprobe.py` and two files under
`data/probes/`), `cargo clippy --all-targets` clean,
`cargo doc --no-deps` **13 warnings** (11 pre-existing private-item links,
plus 2 added by Task 20's `rector_showdown` doc linking to the private
`Game::apply_class_bonus` and `Game::enter_district_5` — 11 at the merge
base `9837b74` and at `90e2d28`, 13 from `a776a97` on; the count was
previously reported as a stale 12, never any commit's actual value, caught
in the Task 20 final review round).
`cargo fmt --check` is now clean. It had regressed 3 → 7 diffs (four new,
from the `BigMarket -> Dealers` rename commit `90e2d28`, which shortened
identifiers and changed rustfmt's comment-alignment decisions in
`src/locations.rs:175`, `tests/combat_sequence.rs:588` and
`tests/wander_sequence.rs:484`/`:938`) and was reported as "seven
PRE-EXISTING", baselined against `90e2d28` itself rather than the merge
base — a check that could not fail on a regression `90e2d28` introduced.
Fixed in the Task 20 final review round by running `cargo fmt`; no
behaviour changed.

**Agent-config decisions, made deliberate in the Task 20 final review round.**
`20a36e9` added `CLAUDE.md`'s trailing `@AGENTS.md` include silently (not
mentioned in that commit's own message), and since `AGENTS.md` itself was
not tracked until the next commit (`8e936bc`), a checkout of `20a36e9` alone
has a `CLAUDE.md` pointing at a file that does not exist there — bisect-
hostile, self-healing one commit later. Left as history (rewriting an
already-made commit was out of scope for this fix round), but the chain it
completes is now a deliberate keep, not a side effect: `CLAUDE.md` →
`AGENTS.md` → `.tessl/RULES.md` → `good-oss-citizen.md` loads ~140 lines of
open-source-contribution procedure into every session of this solo RE port
with no upstream. Reviewed and kept — the procedural rules (templates,
DCO, AI-disclosure) are inert with no target repo to apply them to, and
`good-oss-citizen.md`'s "exclude `.claude/` from contributions" line is
about a PR diff to a *different* project, not this repo's own tracked
`.claude/` files (see I1's fix, above and in `.gitignore`); the two do not
conflict. If the chain is ever judged not worth the session-start cost, drop
it by editing `AGENTS.md`'s include, not by re-touching `.gitignore`.

**The honest state of the project.** Tasks 13-review and 14 moved branch
coverage by zero — sound work on the measuring apparatus that found real
defects in the *evidence* and none in the port. Tasks 16–19 are the return to
the thing measured: +118 branches and, in 18 and 19, real behaviour. Task 19
in particular closed the largest single hole — the game can now save and
load.

### Four oracles now, all ground truth, none ever regenerated

| file | what | digest |
|---|---|---|
| `data/rng_trace.json` | 1387 wander draws, 5 runs, 29-var `final_state` each | `148fe3c7…1025` |
| `data/state_trace.json` | 91 per-turn samples of 35 guest variables | `6f7ae78a…13c7` |
| `data/combat_trace.json` | 15 whole fights across 4 runs, 1900 draws | `8c4b80e6…180acb` |
| `data/combat_vectors.json` | RNG vectors | `705415b2…f044` |

`combat_trace.json` records the first two files' digests inside itself.
`tools/mutate.py` now guards **134** files across `data/`, `orig/`, `tools/`
and `docs/` (122 before Task 18 committed `data/probes/`, 126 after it, 134
after Task 19 added six tools and two files under `data/probes/`) — the count
is derived from the tree, so it moves when files are added —
and
`combattrace.main()` refuses an `--out` naming a frozen oracle.

---

## State

Tasks 1–17 complete and reviewed. Since the last checkpoint:

| Task | What | Commits | Status |
|---|---|---|---|
| 13 | Whole fights captured; port replays 15 of 15 | `521db0e` | complete, reviewed (one fix round) |
| — | Task 13's fix round: 3 Important + 6 promoted Minors | `5e9776f..035c367` | complete |
| 14 | `tools/mutate.py` — the mutation gate for captured oracles | `035c367..57c29b8` | complete, reviewed (two fix rounds) |
| — | Mutation tooling: `cargo-mutants`, mutmut + pytest, `.venv/` | `5af1673..1a9338a` | complete |
| 15 | The eight missed `cargo-mutants` findings in `src/` | `c47abb9..f27b73c` | complete, **reviewed and approved, no fix round** |
| — | Task 15's four deferred Minors, batched | `1a9338a..0a29594` | complete |
| 16 | `FUN_1000_1a03` mapped — **it is the character sheet** | `a293f51..e3e3963` | complete, reviewed (one fix round) |
| 17 | The in-combat dispatcher mapped — `docs/re/combat-dispatch.md` | `54dd7b7..8632db5` | complete, reviewed (one fix round) |
| 18 | The dispatcher arms **ported into `src/`** | `fc1d23d..c0e7dd7` | complete, reviewed (one fix round), **merge with fixes** |
| 19 | The 33 unknown save bytes, and the save/load path they blocked | `a5a6e67..` | complete |

### Task 16, and why it is the shape to copy

**178/838 → 233/838 in one task.** The previous best was Task 11f at +13.

The hypothesis was **half refuted, on the half that mattered** — it *is* the
character sheet, but `stats` is not a verb at all (those bytes are absent from
`orig/g.exe`, whole-file case-insensitive scan) and `sv` calls a different
function (`1000:4c49` → `1000:1348`). The verb is **`s`**, at the street `\`
prompt and at `Битва\`.

There are **four call sites, not the two this project claimed since Task 11c** —
the two in `entry` are near calls whose rel16 displacement wraps 16-bit
(`0xec8c + 0x2d77 = 0x11a03`). The two in `3d11` use negative rel16 and do not
wrap, which is exactly why the wrong count survived so long.

There is **no argument convention**: bare `ret` at `1000:248e`, no positive `bp`
displacement in 2700 bytes, `ax` clobbered at `1000:1a06`. Every call site
renders the same sheet from globals.

`tools/rngtrace/verbprobe.py` is the new instrument: it answers *which typed
verbs reach a chosen function, and which provably do not*. **The negative is the
point** — a probe that only confirms positives cannot distinguish "these verbs
reach it" from "everything reaches it". Reuse it on the next function.

**Both `character-sheet.md` halves are now checked.** `ProseTest` resolves every
`1000:xxxx` in the prose from an aligned decode. It found three live defects on
its first run against the committed document, two of which two reviewers had
walked past.

### Task 17 — the in-combat dispatcher

**233/838 → 281/838**, and `FUN_1000_3d11` 54/224 → 95/224. Map:
`docs/re/combat-dispatch.md`; artifact `data/combat_dispatch.json`; test
`tools/test_combat_dispatch.py`. What it settled, all from flow:

- **`sv` is the ENEMY's sheet.** `FUN_1000_1348` references no address in
  `[20ae:3690, 20ae:3951]` at all, and ends in a bare `ret` at `1000:165e`.
  `combat.md`'s `1000:15bd`..`1000:1611` "second blow display" is the
  **enemy's** accuracy inside it; the player's copy is `1000:21b0` in
  `FUN_1000_1a03`.
- **The verb set is closed at ten** — `k`, `run`, `kos`, `s`, `sv`, `e`, `v`,
  `f` from the nine compares in `FUN_1000_3d11`, plus `h`/`mh` from
  `FUN_1000_29c4` at `1000:4b00`. `20ae:3a72` has exactly twelve references
  inside the function and only one near call receives it, so anything else
  typed at the fight prompt is silently ignored. `x` and `wes` are `entry`'s.
- **The four unmapped `Random` sites** are the backup's damage
  (`Random(district*4)`, the untraced argument being `20ae:3692` shifted
  twice), the backup's attrition tick, and the pistol's hit test and damage.
- **`20ae:3c83` confirmed** as the rector-showdown flag; **`20ae:3696`'s
  boolean closure**; **`1000:4e2a` is dead code**; and **`1000:4ebc` is a
  third reader of `20ae:3693`** where `gaps.md` claimed two — the same
  completeness-claim failure that entry was written to correct.

`tools/rngtrace/verbprobe.py` now takes `--target`, and its output names the
target rather than Task 16's. The re-pointed run: `sv` reaches `1000:1348`
twice, six negatives do not, including combat `s`, which calls `1000:1a03`
from the same chain.

**Nine `tools/mutations.json` cases** now defend `data/combat_dispatch.json`
and `docs/re/combat-dispatch.md`, so `mutate.py` read **28 red + 10
findings** (was 19 + 10; Task 18 took it to **29 + 10**). Five are one-to-one twins of Task 16's
`character-sheet-*` cases — guard address, literal text, caller scan, prose
address, branch partition. Four defend claims Task 16's deliverable never
made: the buffer's twelve-reference closure (which is what "any other verb is
silently ignored" rests on), the four-`Random`-sites scan, the `1000:4e2a`
dead-code argument, and the two `xor si,si` the hospital bill's `0.6` needs.
They were added in the review fix round; the fourteen probes run by hand
during the task are now redundant with them and are kept in the task report
only as the record of what was checked when.

### Task 18 — the arms turned into behaviour, and why it is the shape to copy

**This is the task that ended the drought.** Tasks 16 and 17 documented +103
branches between them and produced **zero Rust**; the owner's words, mid-
session, were *"I see no changes in rs files for 3 hours."* Task 18 is the
other half: `git diff --stat` for `src/` is **5 files, +2059/−115, of which
1212 added lines are not comments**, and the branch metric moved by four — all
four being gates the task implemented and tested, not citations added to move
a number.

It is the first task in this project to pass its own review with **zero
Critical and zero Important**. The whole-branch review then re-derived ~40
instruction sites and every hard-coded Cyrillic literal from `orig/g.exe` and
found no wrong address, no wrong instruction text, no wrong constant, and no
path that could move a frozen oracle.

What it built:

- **`src/combat_dispatch.rs`** — the arithmetic and state of the `v` and `f`
  arms: `20ae:3c80` as `Backup` (a fight-local, because `1000:5841`/`5843`
  zero it on return and all 17 of its image-wide references are inside
  `FUN_1000_3d11`), and `20ae:394d`/`394e`/`394f` as `Pistol`. `game.rs`
  prints; this module holds what a test can pin to a number — the same split
  `src/combat.rs` already used for the blow loop.
- **`run_combat`'s command loop is a straight line, not a `match`.** That is
  the load-bearing correction and the whole-branch reviewer called it out as
  such. The chain is **nine independent `if`s**: `1000:583e jmp 0x40f2` is the
  function's only back edge, so every arm rejoins the line with the buffer
  still holding what was typed. Two consequences a `match` cannot express —
  the second `k` compare at `1000:4c75` is reachable because the blow loop's
  three exits land on `1000:48d7` (the complete re-entry set, verified
  image-wide: `1000:4447`/`467c`/`48cb`/`48d2` → `1000:48d7`), and the backup
  block at `[1000:4d93, 1000:4e9e)` runs on **every** prompt, including one
  whose line matched no compare at all.
- **The flee penalty is applied.** `Progress::growth_log` is the
  `array[1..40] of string[2]` at `.SAV 0x236`, reached through Borland's
  biased base `20ae:38cf` (real base `20ae:38d2`).
- **`20ae:3c83` as `Game::rector_showdown`**, with all three of its effects.
  Nothing sets it — a reachability gap, not an implementation one.
- **`20ae:394d` is the PISTOL**, not `dealer_order_placed`, "a 150-rouble
  order placed with the dealers". `docs/re/gaps.md` had that closed since
  Task 16 and had explicitly called the port's name stale; nobody had
  propagated it. `bmar` rows 7/8/9 now apply their effects, which is what
  makes `f` reachable in play at all.

**The trap it fell into, and the one it caught.** `exit` typed at the fight
prompt quit the game: `crate::commands::parse` folds `e` and `exit` into one
`Command::Quit` because `entry` dispatches both, and `FUN_1000_3d11` compares
only `e`. The task had **already solved that exact fold problem correctly**
three blocks earlier — `run` is matched on the literal precisely because
`parse` folds `w`/`run` — and did not apply the same treatment to `e`. It was
harmless until the arm stopped being `=> {}`. **Whenever a fight-prompt arm
keys off a `Command`, check whether `parse` folds two spellings into it.**

What it caught, by mutating its own code 30 times: `if fled && enemy.hp > 0`
survived a mutation to `if fled`, because the test meant to cover it killed
the enemy a prompt too early and never reached the branch. **Mutate your own
new code before reporting; it is the only thing that found that.**

### Task 19 — persistence, and the instrument that came out of it

**`.SAV` offset + `0x369c` IS the DGROUP address of that byte**, established
from flow: the whole 694-byte record moves between the file and `DS:369c`
with one *untyped* block operation in each direction (`1000:6c01`
`BlockRead`, `1000:acc8` and `1000:765d` `BlockWrite`), so there is no
marshalling for a per-field mapping to hide in. That resolves both `unk_`
spans at once against `FUN_1000_1a03`'s flag lines, which Task 16 had already
mapped and which nobody had read against the save format. `.SAV` now has
**no unestablished byte**.

Three corrections, not just additions:

- **`cartridges` is a WORD at `0x2b3`** (`1000:1d8a cmp word [0x394f],0`), so
  `0x2b4` is its high byte, not a 32nd flag. The obvious byte reading — which
  the task brief carried — was wrong.
- **`0x221`–`0x225` are FIVE post-kill item flags**, four of which grant a
  stat delta. `save-format.md` said "four … at `0x221`–`0x225`";
  `data/xp.json`'s 545–548 was the right list and the prose count was wrong.
- **`gaps.md`'s "there is no 'saved OK' string anywhere in
  `data/strings.json`" was FALSE.** There are two, at decimal 36242 and
  39937, and that false premise is why the port printed nothing on a save.

**The instrument, and why it outlives the task.** **331 of the game's 838
branches (39.5%) have a guard that reads a byte inside the save record** —
`entry` 160 of 406, `FUN_1000_1a03` 77 of 83, `FUN_1000_3d11` 65 of 224. The
derivation is **`python3 tools/branch_reach.py`**, committed for the reason
this file keeps rediscovering: it published `134/838` and `157/838` as bare
numbers and had to correct both. Whatever the script prints is the number.

This entry first said **355 / 42%**. That came from a window based at
`0x389c` — which is the record base **plus `0x200`**, the offset of the stat
words *inside* the record, not the record base. The shift counts 26 branches
whose guards read the ENEMY record at `DS:3952` and the wander bucket at
`20ae:3971`, neither of which a `.SAV` can set, and misses the two
empty-name tests that read `.SAV 0x100`. `branch_reach.py --window
stat-block-base` reproduces the wrong number so the discrepancy stays
checkable. Read either figure as an upper bound on reach-by-save, never as
coverage.

So a save file is a direct write into guest memory for 39.5% of the game's
branch guards:

- **`tools/savegen.py`** — synthesise a valid record by name
  (`--set money=5000 --set level=6`) or by raw offset, starting from a real
  save so everything unnamed is known-good. Refuses an `--out` inside
  `orig/`. Tests in `tools/test_savegen.py`.
- **`tools/rngtrace/saveprobe.py`** — load a synthesised record in the real
  `orig/g.exe` under qemu and read guest memory back. Two committed runs:
  `data/probes/saveprobe-record-base.json` (all 694 bytes verbatim at
  `20ae:369c`, 37 sentinels at their predicted addresses) and
  `saveprobe-fresh-record.json` (`--fresh`, the record a brand-new character
  starts with — which is how "a fresh save fills the unknown bytes with
  zero" became an observation rather than a port decision).

* **`tools/test_string_citations.py`** — decodes every `file`/`CS`/`image
  0xNNNN` citation in `src/persist.rs`, `src/save.rs`, `src/locations.rs` and
  `docs/re/save-format.md` and requires the quoted literal beside it to be
  what that offset holds. The final review found **four** wrong `file` offsets
  on this branch, each the offset of a neighbouring string and two of them
  values used CORRECTLY elsewhere in the same file. Grep cannot tell those
  apart; decoding can. Point it at a new source by adding to `SOURCES`.

Both probes are **state-tier** and say so in their own output. Use them to
*localise* (which guest byte, and what visibly changes), then disassemble the
read site to *establish*. And a synthesised record can build states no
playthrough reaches: say "forced" when you report one.

**The port saves and loads.** `src/persist.rs` holds `Game::to_save` /
`from_save`, the slot menu (`1000:6a62`..`1000:6b81`), the loader, and both
writers. `src/save.rs`'s `to_bytes` now starts from a **zeroed** buffer and
copies through only the shortstring padding, so the five reference saves'
round trip is an offset check rather than a copy — verified by breaking one
offset and watching it fail. The port's fresh 694-byte record is
byte-identical to the original's own.

**Closed by Task 21:** the district-advance autosave
(`1000:ab75`..`1000:ad12`) is wired. `Game::district_advance` runs at the top
of `Game::run`'s loop — the original's own position, ahead of the street
prompt at `1000:ae3c`/`1000:ae55` and reached by the `1000:ee01` back edge —
with both gates, the increment, the flag resets, both ban-countdown clears,
the two lines, the `\` prompt, the `y` compare and the 694-byte write into
`save_r<district>.sav`. It also fixed a divergence the old placement hid: the
post-fight `while` loop promoted several districts inside one fight, where
`1000:ab75`..`1000:ad12` has no back edge and gains at most one per turn. See
`docs/re/gaps.md`, "The district-advance autosave — wired (Task 21)", for
what is still open there (the class-conditional flag resets, `1000:ad12`'s
announcement text, and the chapter-5 arm's per-turn repeat).

### What Task 14 built, and what it is for

`tools/mutate.py` mutates a **captured ground-truth artifact** and requires a
named test to go red. It is the executable form of a rule `METHODOLOGY.md` could
not previously enforce: *an assertion over a captured oracle is not evidence
until it has been observed failing.* Task 14 shipped it with 23 cases and 13
red channels; the manifest is now **39 cases; 29 red channels**, Task 16 having
added five, Task 17 nine and Task 18 one (the growth log's `.SAV` offset). 10 are registered as
`expect_red: false` — columns the capture holds that **no assertion reads**,
including `r_randseed_367e`/`e_randseed_367e`, which wander asserts per sample
and combat does not.

`cargo-mutants` covers the half that gate structurally cannot — mutating `src/`
itself. `-f src/combat.rs -f src/rng.rs` is **76 mutants, 0 missed**, and
`-f src/save.rs -f src/persist.rs` is **152 mutants, 0 missed** after Task 19.
Its first run there found 2 survivors in `save.rs` and **10 in `persist.rs`**,
all ten in the load path's DECISIONS — which file is opened, which menu line
is printed, which arm a malformed `places.sav` takes, what district a level
maps to — while the data transformations were already covered. Expect that
asymmetry on the next port task.
Whole-crate, as a measurement only: **1175 mutants, 446 missed**, `game.rs` 416
of 833. Nothing was fixed there.

mutmut + pytest are installed in `.venv/` for the Python half, configured in
`pyproject.toml`, and currently report **1439 mutants, 345 survived**. Read
those survivors narrowly: the replay is already a strong independent check on
anything *recorded*, so the ones that matter are in the guards that detect
**absence** — `tracelog.check_fight_markers`,
`tracelog.check_state_sample_shape`, `fightrun.verify_image_after_drive` — which
is the one failure mode a replay cannot see.

## Owner constraints in force

- **Dependency constraints RELAXED 2026-08-24, replaced by a consultation rule.**
  The owner's words: *"I don't care about dependency constraint as long as you
  consult with me."* This supersedes the plan's "Python tooling uses the standard
  library only" and the enumerated Rust dependency allowlist as *hard* limits. It
  does not make dependencies free — **ask before adding one, every time**, and
  record the answer here. The old constraints stay the default worth defending
  (the shipped binary carries no JSON parser, and that is a property worth
  keeping); what changed is that they are now a preference to argue with, not a
  wall to route around. First use: `cargo-mutants` (below).
- **`cargo-mutants` approved 2026-08-24** for the code-side mutation gap that
  `tools/mutate.py` structurally cannot cover. It is `cargo install`ed dev
  tooling, not a `Cargo.toml` entry, so it never enters the shipped binary's
  dependency graph — same shape as the owner-installed `capstone`. Scope its runs
  (`-f src/combat.rs`); every mutant re-runs a suite that replays 1387 wander and
  1900 combat draws.
- **One agent at a time, AMENDED 2026-08-23: genuinely small work may run in
  parallel.** The reason is unchanged — token spend rate, not wall-clock — so the
  test is **size, not independence**. A task that is merely independent of what is
  running does not qualify. In practice the only things that have qualified are
  batched deferred-minor cleanups, scoped to files the running task cannot touch.
- **`capstone` is permitted** (owner-installed), amending the plan's stdlib-only
  Python constraint. `tools/dis16.py` stays the shipped decoder — it is validated
  against `ndisasm` over 19,000+ instructions and has no dependency. Where capstone
  and `dis16` decode the same bytes **they must agree**; a disagreement is a finding
  worth more than whatever it was found while doing.
- **`/home/finkel/Downloads/TP/`** is a Turbo Pascal 7.0 distribution the owner put
  on disk. `BIN/TURBO.TPL` is what Task 11h matched the runtime against. It is
  outside the repo, so tests needing it must degrade gracefully for anyone cloning.
- **`jbcontext` semantic search** is available over `docs/re/`'s ~6300 lines.
  **A semantic hit is a lead, not a citation** — same status as `build/decomp/`'s
  Ghidra C. `METHODOLOGY.md` still requires an address and a tier per claim.
- **Full branch coverage is the target**, not a faithful core loop.
- Declined, do not re-propose: a play-recorder built on `scrhook` (manual play
  cannot reach all paths in reasonable time). Ghidra branch enumeration was
  declined and then reversed — it is done, Task 11e.

## Instruments now available (this is what changed)

- **`tools/rngtrace/`** — qemu+gdb harness. Breakpoints on the game's own
  16-bit code fire, and gdb reports IP as the Ghidra offset, so results read
  straight against `docs/re/`. Nine guards, each with a test that fails without
  it. `data/rng_trace.json` holds five captured runs of the ORIGINAL: 1387
  ordered `{i,turn,site,n,r}` draws plus a 29-variable `final_state` per run.
  **That file is ground truth — never regenerate it to match the port.**
  Task 12 must assert on `order_check.in_catalogue_order`: `check_order`
  records violations rather than raising, so a drifted run still exits 0.
- **`data/branches.json`** — 1119 conditional branches (838 game, 281 RTL) with
  the guard condition on each. The guard is the recipe for forcing a state.
  Its `port_touched` field is a weak proxy that errs both ways; do not read
  `uncited_spans` as a to-do list.
- **Reaching states without grinding:** synthesise save files (byte-exact
  round-trip, plus `PLACES.SAV` flags) to start inside a chosen
  class/level/district/flag state, or poke guest memory under gdb. Label what
  the code DOES in a forced state separately from whether a player can REACH
  it — different claims.

## How much is actually traced

**296 of 838 game branches (35.3%)** have their branch address or guard cited
anywhere in `docs/re/*.md`, measured after Task 19 with the snippet below.

This file previously said `134 / 838` here and `157/838` further down, both
stale, and both left standing through a whole session because the instruction
to re-measure was not followed. Measured history, so a future session can see
the rate rather than trust a number:

| commit | | cited |
|---|---|---|
| `f2d4fce` | resume point, Task 13 landed | 175/838 |
| `5e9776f` | merge to `main` | 175/838 |
| `57c29b8` | Task 14 complete (mutation gate) | 175/838 |
| `f27b73c` | Task 15 complete | **178/838** |
| Task 16 complete (`FUN_1000_1a03` mapped) | | **233/838** |
| Task 17 complete (the in-combat dispatcher mapped) | | **281/838** |
| Task 18 complete (the dispatcher arms ported into `src/`) | | **285/838** |
| Task 19 complete (the save bytes, and the save/load path) | | **296/838** |

The whole of Tasks 13-review and 14 moved it by **zero**; Task 15's +3 are the
addresses its equivalence proofs had to cite. That is the cost of a session
spent on the measuring apparatus rather than the thing measured — worth
knowing before the next one starts. Task 11f, for contrast, added 13 in one
task.

Re-run it against the current tree with:

```
python3 - <<'EOF'
import json, re, glob
d = json.load(open('data/branches.json'))
B = [b for b in d['branches'] if b['class'] == 'game']
text = "".join(open(f, encoding='utf-8').read() for f in sorted(glob.glob('docs/re/*.md')))
cited = {m.group(0).lower() for m in re.finditer(r'\b[0-9a-fA-F]{4}:[0-9a-fA-F]{2,4}\b', text)}
hit = lambda b: b['addr'].lower() in cited or bool(b['guard'] and b['guard']['addr'].lower() in cited)
print(sum(map(hit, B)), '/', len(B))
EOF
```

It printed `178 / 838`, exit 0, run against the tree at `f27b73c` (Task 15). The `bool(...)` is load-bearing, not tidying: without it `hit` returns
`None` — not `False` — for a branch that is uncited and has no guard, because
`x and y` yields `x` when `x` is falsy, and `sum()` then raises
`TypeError: unsupported operand type(s) for +: 'int' and 'NoneType'`. An
earlier revision of this file shipped the snippet without the `bool` **under a
`$` prompt and a hand-written `134 / 838` output line** — a command that cannot
run, presented as a transcript of its own output. The number was right; the
evidence for it was fabricated-looking. Do not re-simplify it.

**What this metric does not see.** It globs `docs/re/*.md` and nothing else. RE
citations that live in `src/` doc comments — for instance the `run_combat`
block in `src/game.rs` — never count toward it, in either direction. Read the
percentage as coverage of `docs/re/`, not as the project's real coverage.

**Where the +13 since Task 11c came from.** The baseline is 121/838 at commit
`9bfd4bd` (the Task 11c checkpoint) — that is what the snippet above returns
when pointed at that commit's `docs/re/*.md`. All thirteen new hits are
citations added to `docs/re/gaps.md`, in two commits:

| commit | new hits | function | addresses newly cited |
|---|---:|---|---|
| `9794362` (11c fix round) | 4 | `entry` — market pickpocket block | `1000:c353`..`1000:c369` |
| `3fac24c` (Task 11f) | 6 | `FUN_1000_0d14` — opponent roll | `1000:0d64`, `1000:0d86`, `1000:0da7`, `1000:0dba`, `1000:0e48`, `1000:0e54` |
| `3fac24c` (Task 11f) | 3 | `entry` — encounter notice/decline | `1000:b5da`, `1000:b60a`, `1000:b614` |

Fix round 1 (`3ef0959`) added none. So `entry` goes 67 → 74 and
`FUN_1000_0d14` 0 → 6, and **combat `FUN_1000_3d11` is unchanged at 26/224**.
Task 11f's fight-flow addresses `1000:48eb`, `1000:490e` and `1000:4931` are
inside `FUN_1000_3d11`, combat — *not* inside `entry`, which is what this file
claimed until now — and they moved the count by zero: `1000:48eb` and
`1000:4931` were already cited in `docs/re/wander.md` and
`docs/re/progression.md` before Task 11f, and `1000:490e` appears nowhere
under `docs/re/` at all — among the project's RE prose it is only in
`src/game.rs` and in this file, neither of which the metric reads. (It does
appear twice in `data/branches.json`, as catalogue data, and in
`data/string_pointers_audit.tsv`; those are generated artifacts, not prose,
and the metric does not read them either.
`git grep -l '1000:490e'` lists exactly those four files.)

Two functions hold 75% of all game branches:

| branches | cited | function |
|---:|---:|---|
| 406 | 93 | `1000:ab59` — main loop + command dispatch |
| 224 | 95 | `1000:3d11` — combat; its dispatcher half mapped in Task 17 |
| 83 | 50 | `1000:1a03` — the character sheet, mapped in Task 16 |
| 11 | 7 | `1000:1348` — the ENEMY's sheet, the `sv` handler, mapped in Task 17 |

The metric undercounts (a function can be understood without every `jz` being
cited) and a citation is not comprehension. Re-run the query above to track
it.

### `FUN_1000_1a03` — settled in Task 16, and how the hypothesis fared

`docs/re/character-sheet.md` is the map; `data/character_sheet.json` the
machine-readable twin; `tools/test_character_sheet.py` re-derives every claim
from `orig/g.exe`.

The hypothesis this file used to carry — "the body behind `stats` from the main
loop and `sv` from combat" — was **half refuted**, and the half that mattered
was the wrong half:

- It IS the character sheet. That part held.
- **`stats` is not a verb**: those five bytes do not occur in `orig/g.exe`. The
  verb is `s`, compared at `1000:ec82` against the CS literal at `0x9f85`.
- **`sv` never enters it.** `1000:4c42` matches `sv` and `1000:4c49` calls
  `FUN_1000_1348` instead. `docs/re/gaps.md`'s in-combat verb table had both
  rows already; nobody had read them against this hypothesis.
- **"Called by exactly two things (`entry` and combat)" was right about the two
  FUNCTIONS and hid four call sites.** The two in `entry` (`1000:ec89`,
  `1000:ee36`) are near calls whose displacement wraps 16-bit — both decoders
  render them `call 0x11a03` — so a byte scan that compares the un-wrapped sum
  finds neither. Match modulo 64 KiB.
- **There is no argument convention to find.** Bare `ret` at `1000:248e`, no
  positive `bp` displacement anywhere in 2700 bytes, `ax` clobbered at
  `1000:1a06`. All four sites render the same sheet from globals.

Instrument added: `tools/rngtrace/verbprobe.py` — three breakpoints
(`1000:ae63`, `1000:441d`, a target), a scripted verb list, and a per-verb count
of entries, with the driver's screen classification cross-checked against the
guest's own prompt markers position by position. It produced five negatives
alongside the two positives. Point it at any other function to ask the same
question.

## Remaining work

1. ~~**Task 11g**~~ — **done.** The address convention lives in
   `tools/addr.py` with tests in `tools/test_addr.py`; the four recurring
   disassembly questions are `python3 tools/re_query.py {resolve,is-call-site,
   pushed-n,xrefs-to}` with tests in `tools/test_re_query.py`. See
   `docs/re/METHODOLOGY.md`, "How to check this mechanically".
2. Task 12 — now much smaller: the draw-replay covers the RNG half, so 12 is
   prices, XP thresholds, level-up gains, starting stats, menu numbering.
3. The bulk, from `docs/re/gaps.md`: ~~no `.SAV` load path and `write_save`
   returns `Unsupported`~~ — **done in Task 19**, along with the mage's paid
   save; what is left of persistence is the district-advance autosave, which
   needs the district advance moved to the top of the main loop.
   **Shop purchase effects for every row except `bmar`
   7/8/9** (Task 18 did those three because they are what makes the pistol
   reachable; the generic path also echoes the MENU line where the original
   prints each arm's own confirmation, and refuses a district-gated row where
   the original's buy compares carry no district test at all); the class-keyed
   combat-opener table (`1000:3d32..3e8a`); ~~the rector death branch and
   hospital rescue (`1000:4f8c`)~~ and ~~`sv`/`v`/`x`/`wes` dispatcher
   sites~~ — mapped in Task 17 and **both modelled in `src/` by Task 18**
   (`Game::rector_showdown`, `Game::hospital_rescue`), though nothing SETS the
   rector flag, so its three arms are reachable only from a test, and
   `x`/`wes`'s own arms in `entry` are still unread; shop modality;
   `kl`/`trn` prices; `help` and `rename` content; the quit messages; the
   joint heal formula (rests on analogy with beer — a hypothesis, not a
   finding). The encounter decline branch itself is now resolved (Task 11f
   traced `1000:b5fc` and the port models both the aggressive and quiet
   arms), dropped from this list; Task 11g's stale-entry sweep corrected the
   two places that still described it as open, `docs/re/gaps.md`'s "Other
   unreproduced behaviour" entry and `docs/re/rng-trace.md`'s "Limits" list.
4. Small follow-ups: ~~`ExportAll.java` serialises an unsorted set~~ **fixed
   in Task 11g** — every collection it writes is sorted and two runs are
   byte-identical, and it now also emits `data_xrefs`. Still open:
   `data/strings.json` false positives (10 entries inside function bodies, one
   unflagged); the tracer's progress guard has an inert `RandSeed` half; its
   strongest guard-replay test skips for anyone who clones. **New:**
   `data/branches.json` is stale against `src/` — regenerating it changes only
   the port-citation fields (`cited_in_port`, `port_citations`,
   `bytes_to_nearest_port_citation`, …) on 342 of 1119 records, because Task
   11f edited `src/`. Task 11g verified this and deliberately left it alone as
   out of scope.
5. Windows VT: compiles and runs under wine, but VT changes how a console
   *renders* bytes, not which bytes are written — no byte-capture test can
   verify it. Needs a human at a `cmd.exe` window.
6. ~~Final whole-branch review~~ — **done after Task 18**; verdict *merge with
   fixes*, and this file is one of the fixes. Next step is
   `superpowers:finishing-a-development-branch`.

## The highest-value cleanup left, and why it is one job not two

**Add a `term` sink.** `crate::term` writes straight to this process's stdout,
and nothing in the crate can capture it. That single missing seam is what
blocks two separate things, which is why it is worth doing as one refactor
rather than being rediscovered twice:

* **The combat `s` and `sv` arms have no executable assertion.** Both are
  print-only calls (`1000:4c35 call 0x1a03` → `Game::show_stats`,
  `1000:4c49 call 0x1348` → `Game::print_enemy_block`), so a mutation
  swapping one for the other survives every test in the suite. Task 18
  reported them as known survivors rather than faking coverage;
  `tests/game_flow.rs` had to drive the real binary as a **subprocess** to
  assert anything at all about the street verbs, which works but cannot reach
  a fight prompt without a deterministic seed the binary does not take.
* **Test output is full of game text.** `cargo test` prints `Битва\`,
  spectator taunts and encounter lines interleaved with the harness's own
  output, because every fight a test runs writes to the real stdout. It makes
  a failure genuinely hard to read.

The same sink fixes both: give `term` an installable destination (a thread-
local `Vec<u8>` is enough), have the tests install one, and the assertions and
the quiet become available together. Nothing else in the deferred list has
that leverage.

### Deferred from the Task 19 review, recorded so they are not lost

None is a defect the review asked to be fixed; each is a judgement call a
future reader may want to revisit.

* ~~**`present_slots` returns `SLOT_KEYS` order.**~~ **Fixed, and it was a
  bigger defect than the ordering.** The mask is `save_r?.sav` with a DOS
  wildcard, so the SCAN and the KEY TEST are two mechanisms; filtering the
  scan on `SLOT_KEYS` made `save_r1.sav` and `save_rx.sav` invisible to the
  port and visible to the original. Order is now by name, recorded as a port
  decision (FAT directory order is not portable and nothing depends on it).
* ~~**`data/probes/README.md` names the fresh-record assertion at the wrong
  path.**~~ **Fixed.** It said `tests/save_roundtrip.rs`; the tests are in
  `tests/save_load.rs`, and there are two —
  `a_fresh_record_matches_what_the_original_starts_a_new_character_with`
  (line 183) and `a_fresh_record_is_byte_identical_to_the_probe_dump`
  (line 219). An earlier revision of THIS list paired line 183 with the
  second name, which was also wrong.
* **`saveprobe-fresh-record.json` carries no `tier` field**, unlike
  `saveprobe-record-base.json`. The caution is in the README instead, which
  is adequate; the two artifacts are just inconsistent.
* ~~**`data/save_layout.json` carries no `tier` key.**~~ **Fixed:** every
  field now carries `"tier": "flow"`, so the next field added at a weaker
  tier has to say so.
* **`tools/test_decode_save.py`'s store filter would miss a
  `mov [0x38b0],al`.** No such store exists (the review re-derived the
  boolean claim over all 23 flag bytes including that form and confirmed it),
  so the conclusion holds; the *filter* is narrower than the claim it backs.
* **`tests/save_load.rs`'s `every_in_record_address_named_in_game_rs_is_persisted`
  matches on comment text**, so it is defeated by a citation written in a
  different form. It is a completeness prompt, not a proof.
* ~~**`src/persist.rs`'s slot-0 district uses unsigned division.**~~
  **Fixed.** "Unreachable because level is 0..40" was true of *play* and not
  of `tools/savegen.py --set level=0x8000`, which is the workflow this branch
  hands the next tasks.
* **Three `.max(0)` clamps and the name-prefix gain** in `Game::from_save`
  are **kept**, and now documented rather than silent: `docs/re/gaps.md`,
  "What the port REFUSES that the original accepts", carries all three with
  the reason each stays.
* ~~**A condition in `tests/save_load.rs`'s short-`places.sav` test is true
  for all four cases.**~~ **Fixed** — deleted.

### Also deferred from the Task 18 review, recorded so they are not lost

None is a defect; each is a judgement call a future reader may want to
revisit. The `chain_reentry` closure test; the den-flag scan's `mov byte`
scope (judged over-stated — the test's assertion is about immediate stores and
its scope matches its claim); the `live_probe` key-name divergence;
`i32::from(agility)`'s zero-extension; `tests/game_flow.rs`'s backslash
anchor; `Game::buy_pistol_row`'s home in `game.rs` rather than a shop module;
the shop display-gate-as-action-gate divergence (`src/game.rs`'s
`shop_action`); the duplicated gates inside `buy_pistol_row`; and
`Game::backup_in_fight` passing one `has_mobile` to both `1000:4cdb`'s
equality test and `1000:4d73`'s inequality test.

## Findings from 11c worth not losing

- **`20ae:38b2` is the armour byte** (record `+0x16`), not `unk_38b2` —
  established from the record layout, corroborated by `SAVE_R3.SAV` and run E.
- `Game::wander_girl` already agreed with both gates at `1000:b4ef` and
  `1000:b548`, verified from the disassembly rather than assumed.
- Two claims the oracle cannot reach, found by mutation-testing the passing
  runs: "the mage spends no draw" (no passing run enters `1000:7538`) and
  `church_visits`'s three transitions (text-only). Both rest on flow alone.
- Run E's starting discovery flags are inferred from its `final_state`, not
  observed. No discovery flag gates a preamble draw so it cannot move the
  sequence, but why the guest's flags were clear when the `PLACES.SAV` copied
  into its game directory is all `01` was never established.

## Methodology in force

`docs/re/METHODOLOGY.md` is binding. Flow > state > output; output can falsify
a flow claim but never establish one. Probabilities come from the comparison
constants that bucket a `Random` result, never from counting outcomes. Every
claim states its tier and cites an address.

**Address convention:** two forms, and they are not the same arithmetic.
`docs/re/METHODOLOGY.md`, "Address convention, and its range of validity", is
the authority for the rule; **`tools/addr.py` is its executable form** and the
only place the arithmetic lives in code. Do not re-derive it by hand and do not
restate it here -- import `addr.citation()`, which picks the form from the
segment, or run `python3 tools/re_query.py resolve <citation>`. Each form's
function rejects the other form's segment range, so the 64 KiB mix-up raises
instead of returning a plausible number. Landmarks, if you want a sanity check
without running anything: `1000:b353` holds `9a 4b 11 78 0f` at file `0xcc23`;
`0f78:114b` (== Ghidra `1f78:114b`) is file `0x1219b`.

Every RE miss caught here has been a two-to-five-byte drift — near enough to
read as authoritative — with two exceptions: the address-convention error
above, which is 64 KiB, and Task 11f's fix round, which found three citations
in `src/game.rs` (and one in `docs/re/gaps.md`) labelled "file" while holding
an **image** offset — a `0x18d0`-byte miss, the header size itself. Verify
from an aligned instruction start, never from a byte-scan hit, and never trust
a "file" label without checking which arithmetic actually produced it.

**The recurring defect across every review this project has run: a check that
cannot fail, presented as verification.** A tautological string comparison, a
guard written against one past symptom rather than the class, a scan whose
completeness claim stopped the next search. When a number or a guarantee
matters, recompute it from the shipped artifact and show the command.
