# Resume checkpoint — gopnik-rs port

**Branch:** `port/gopnik-rust`. **HEAD at checkpoint:** `2b2ad33`.
**Last session ended:** 2026-08-19, at the owner's request, after Task 11c.

`.superpowers/sdd/progress.md` is the full ledger but is **git-ignored** — a
`git clean -fdx` destroys it. This file is the committed backup. If they
disagree, trust `git log`.

---

## READ THIS FIRST: `cargo test` is RED, deliberately

`cargo test --test wander_sequence` fails 3 of 11:
`run_a_replays_exactly`, `run_b_replays_exactly`, `run_e_replays_exactly`.
Everything else in the workspace is green (113 tests).

**This is not a broken build and must not be "fixed" by weakening the tests.**
The three runs replay the original's `Random` stream *exactly* up to their first
bucket-3 encounter and then diverge at `1000:0d26`. The first **mismatching**
draw is at index **18, 63 and 79** of 393, 325 and 610 — 0-based, and exactly
what the harness prints — so the matching prefixes are 18, 63 and 79 draws long.
(An earlier revision of this paragraph said 17, 62 and 78, which is the index of
the last *matching* draw, one short of what the failure message names. The
constants `A_/B_/E_DIVERGES_AT` in `tests/wander_sequence.rs` carry the corrected
values and bound the prefix assertions.)

One enumerated gap causes all three: `FUN_1000_0d14`
(`1000:0d14..11c2`), the random-encounter opponent roll, is not recovered —
`Game::pick_enemy` is an approximation, and the fight-flow draws at `1000:b5f1`
and `1000:b792` are unmodelled. `docs/re/wander.md` puts all of them outside its
catalogue. Because the RNG is one shared stream, a single bucket-3 turn
desynchronises the rest of that run.

The implementer deliberately did not narrow the assertions to the preamble.
Closing `FUN_1000_0d14` is the next task and turns all five green.

---

## State

Tasks 1–11 complete and reviewed (see git log). Since then:

| Task | What | Commits | Status |
|---|---|---|---|
| 11b | Wander `Random` catalogue (18 draws) | `61de765..f1602bd` | complete, reviewed |
| 11d | Live `Random` tracer (qemu+gdb) | `587f9b1..e344c63` | complete, reviewed |
| 11e | Ghidra branch enumeration | `e32aa71..f72c541` | complete, reviewed |
| 11c | Wander sequence wired into `src/` | `2b2ad33` | **NOT yet reviewed** |

**Next action: review Task 11c.** Base for the review package is `f72c541`.
Brief `.superpowers/sdd/task-11c-brief.md`, report `.superpowers/sdd/task-11c-report.md`.

## Owner constraints in force

- **One agent at a time.** Serialise every dispatch — implementers, reviewers
  and fix waves alike. The reason is token spend rate, not wall-clock. Also
  saved to durable memory as `one-agent-at-a-time`.
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

121 of 838 game branches (14.4%) have their branch address or guard cited
anywhere in `docs/re/`. Two functions hold 75% of all game branches:

| branches | cited | function |
|---:|---:|---|
| 406 | 68 | `1000:ab59` — main loop + command dispatch |
| 224 | 26 | `1000:3d11` — combat |
| **83** | **0** | **`1000:1a03` — nothing written about it at all** |

The metric undercounts (a function can be understood without every `jz` being
cited) and a citation is not comprehension. Re-run the query in the ledger to
track it.

### The best lead for next session

`FUN_1000_1a03` — 2700 bytes, 83 branches, third-largest in the game, zero
coverage. Called by exactly two things (`entry` and combat `3d11`) and calls
**nothing but Borland RTL**. `1000:1a36`, the player rank-name lookup, sits 51
bytes inside it.

**Hypothesis, tier `unverified`:** it is the character-sheet / stats renderer —
the body behind `stats` from the main loop and `sv` (size up the enemy) from
combat. That would explain why `sv` and `v` were never dispatcher-confirmed.

Settle it cheaply: break on `1000:1a03` under `tools/rngtrace/`, type `stats`,
then `sv` mid-fight. That proves which verbs reach it, including a negative.

## Remaining work

1. Review Task 11c; then **`FUN_1000_0d14`** (turns the three red tests green).
2. Task 12 — now much smaller: the draw-replay covers the RNG half, so 12 is
   prices, XP thresholds, level-up gains, starting stats, menu numbering.
3. The bulk, from `docs/re/gaps.md`: no `.SAV` load path and `write_save`
   returns `Unsupported`; shop purchase effects; the class-keyed combat-opener
   table (`1000:3d32..3e8a`); the rector death branch and hospital rescue
   (`1000:4f8c`); the encounter decline branch; `sv`/`v`/`x`/`wes` dispatcher
   sites; shop modality; `kl`/`trn` prices; `help` and `rename` content; the
   quit messages; the joint heal formula (rests on analogy with beer — a
   hypothesis, not a finding).
4. Small follow-ups: `ExportAll.java` serialises an unsorted set so
   `run_ghidra.sh` rewrites `data/functions.json` nondeterministically (one-line
   sort); `data/strings.json` false positives (10 entries inside function
   bodies, one unflagged); the tracer's progress guard has an inert `RandSeed`
   half; its strongest guard-replay test skips for anyone who clones.
5. Windows VT: compiles and runs under wine, but VT changes how a console
   *renders* bytes, not which bytes are written — no byte-capture test can
   verify it. Needs a human at a `cmd.exe` window.
6. Final whole-branch review, then `superpowers:finishing-a-development-branch`.

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

**Address convention:** two forms, and they are not the same arithmetic --
`docs/re/METHODOLOGY.md`, "Address convention, and its range of validity", is
the authority. Ghidra labels (`SEG >= 0x1000`, e.g. `1000:`, `1f78:`, `20ae:`)
map as `file_off = 0x18d0 + (SEG - 0x1000)*16 + OFF`; the `- 0x1000` is
load-bearing there, and dropping it overshoots by 64 KiB. Real runtime
`seg:off` as `ndisasm` prints a far-call operand (`0eed:`, `0f16:`, `0f78:` --
the image's relative segments) map as `file_off = 0x18d0 + SEG*16 + OFF`, with
**no** `- 0x1000`; applying the Ghidra form to one of these *under*shoots by
the same 64 KiB. Check any derived address against a landmark: `1000:b353`
holds `9a 4b 11 78 0f` at file `0xcc23`; `0f78:114b` (== Ghidra `1f78:114b`)
is file `0x1219b`, `0x18d0 + 0xf780 + 0x114b`.

Every RE miss caught here has been a two-to-five-byte drift — near enough to
read as authoritative — with one exception: the address-convention error
above, which is 64 KiB. Verify from an aligned instruction start, never from a
byte-scan hit.

**The recurring defect across every review this project has run: a check that
cannot fail, presented as verification.** A tautological string comparison, a
guard written against one past symptom rather than the class, a scan whose
completeness claim stopped the next search. When a number or a guarantee
matters, recompute it from the shipped artifact and show the command.
