# `FUN_1000_29c4` — the beer routine, and the citations that miss by four bytes

**Spec authority:** `docs/re/METHODOLOGY.md` (binding evidence standard) and the
Global Constraints of `docs/superpowers/plans/2026-08-17-gopnik-rust-port.md`,
which this plan inherits verbatim and does not restate.

**Branch:** `main`, by the owner's standing instruction ("develop on main"),
the same ruling the previous plan records.

## Why this slice, with the number

The per-entry table printed by the block in `docs/re/branches.md` under
*Recomputation, from the shipped artifacts → Coverage* reads, at `fe2e8bf`:

```
1000:29c4    bytes   666 branches  19 touched   2 citations  26
```

**Do not trust that cell — re-run the block.** With combat at its ceiling
(222/224) and `entry` the only larger hole, `1000:29c4` is the largest
*whole-function* hole left outside the two giants: 17 of 19 uncited (89%), in
666 bytes, and it is player-visible behaviour — `h` and `mh`, reachable from
the street, the gym exit (`1000:e966`) and from inside a fight (`1000:4b00`).

## The trap this plan is written around

**26 citations already sit inside this function's range and they move the
metric by two.** `Game::beer`'s doc comment (`src/game.rs:3951`) cites
`1000:29f0`, `2a02`, `2a18`, `2a3b`, `2a47`, `2a51`, `2a55`, `2a6a`, `2aa0`,
`2ab9`, `2af2`, `2b40`, `2b83`, `2b89`, `2bb0`, `2bbf`, `2c53` and more. Only
two of them — `2a18` and `2a47` — are a branch's own address or its **guard's**,
which is the rule `port_touched` applies. The rest cite the `mov di,imm16`
that loads a string, or a `cmp` that is not the guard record, and land four to
twenty bytes off the address the metric reads.

So the cheap move here is to paste seventeen guard addresses into that doc
comment and report "+17". That is the defect `CLAUDE.md` names — a number that
rose while the game stood still — dressed as coverage. This plan therefore
requires the **split**, and requires every citation to land on the `src/`
construct that *evaluates* the condition, never in a prose block that merely
mentions it.

The port is not assumed correct either. The 19 branches include three separate
compares of hp against the saved entry hp (`1000:2bc2`, `2c09`, `2c32`, each
`CMP AX,word ptr [BP + 0xfefe]` with a different target) where `Game::beer`'s
`mh` tail has a two-way `if healed != 0 { .. } else if ..`. A three-way
original against a two-way port is either a proof the shapes agree or a bug;
Task 41 decides which, from flow.

## Task-local constraints

Additional to the inherited Global Constraints:

- **The beer routine spends no RNG draw** if scanning `[0x29c4, 0x2c62)` for
  `9a 4b 11 78 0f` returns zero hits — **establish that, do not inherit it.**
  If it holds, `tests/combat_sequence.rs` and `tests/wander_sequence.rs` must
  stay green draw-for-draw and any movement is a bug in Task 42, not a
  rebaseline. The five oracles under `data/` are never regenerated.
- **State the runner with every test number.** `python3 -m unittest discover -s
  tools -p 'test_*.py'` is the number of record; `.venv/bin/pytest tools/ -q`
  disagrees by design and both are right. Report `skipped` too — `3` means
  `build/decomp/` was present, `16` means it was not.
- **Gates green before DONE:** `cargo test`, `cargo clippy --all-targets` (zero
  warnings), `cargo fmt --check`, both Python runners, `python3
  tools/mutate.py`, `python3 tools/difftest.py`.
- **Every claim states its evidence tier and cites an address**, verified from
  an aligned instruction start with `python3 tools/re_query.py resolve
  <citation>` — never from a byte scan. `build/decomp/FUN_1000_29c4_1000_29c4.c`
  (regenerate with `bash tools/ghidra/run_ghidra.sh --decomp-only`) is a
  **lead**, never evidence.
- **A citation names the construct that evaluates the condition.** Writing an
  address beside code that does not evaluate that condition is a false citation
  and is worse than leaving the branch uncited.

---

### Task 41: Decode all 666 bytes, and classify all 19

**RE only. This task changes no line of `src/`.**

Recover, from flow, over `1000:29c4`..`1000:2c61`:

- The full body: prologue, the two argument compares (`1000:29f0` `"h"`,
  `1000:2a02` `"mh"`) and the neither-token return, the drink loop, and the
  `mh` tail `1000:2bbf`..`1000:2c53`. Every gate in the original's order.
- **Every string the routine writes**: file offset, the `mov di,imm16` that
  loads it, the verbatim CP866→UTF-8 text, and which arm writes it. The known
  offsets `0x419C`, `0x41CD`, `0x41E4`, `0x4208`, `0x4240`, `0x424C`, `0x4283`
  are a lead from the existing doc comment, **not a census** — the existing
  comment is exactly the artifact this task is auditing.
- **Every store**: a scan of the range for writes to `DS:` globals, so the
  claim "the routine touches hp and beer and nothing else" is established
  rather than assumed. `DS:38ac` hp, `DS:38ae` hpmax, `DS:38b0` broken jaw,
  `DS:38c3` beer in half-litres — confirm each identity by `xrefs-to`.
- **The three `[BP + 0xfefe]` compares** (`1000:2bc2`, `2c09`, `2c32`): what
  that stack slot holds, what each of the three decides, and each target.
- **The zero-draw claim** (see Task-local constraints).
- **Both call sites**: `1000:e966` (the tail `entry`/gym exit shares) and
  `1000:4b00` (inside `FUN_1000_3d11`) — what each pushes, and whether the two
  differ in anything the port must model.

**The classification.** For **each** of the 19 game branches of
`FUN_1000_29c4` — the list is reproducible with the *Recomputation → Coverage*
block filtered to `func_entry == "1000:29c4"` — decide exactly one of:

- **`implemented`** — `src/` already evaluates this condition. Name the precise
  construct (module + function + the expression) so Task 42 can put the
  citation there and a reviewer can falsify the pairing. Note that the two
  token compares (`1000:29fa`, `2a0c`) are evaluated in the port's **command
  dispatch**, not in `Game::beer`; if that is where they belong, say so and
  name that construct.
- **`unimplemented`** — `src/` does not evaluate it. Say what it does, from
  flow, in one line.
- **`unreachable-in-port`** — the port deliberately diverges. Cite the
  `docs/re/gaps.md` entry that records the divergence, or open one.

An `implemented` row whose named construct does not evaluate that condition is
a defect; the reviewer will check a sample against `src/`.

**Deliverables:**
- `docs/re/beer.md` — the routine map, evidence tier and address per claim,
  including the string census and the store scan with the command that produced
  each.
- `data/beer_arms.json` — machine-readable twin, same shape as
  `data/gym_arms.json`.
- `data/beer_uncited.json` — the 19 rows: `addr`, `guard`, `class`, and for
  `implemented` the `src` construct.
- `tools/test_beer_arms.py` — re-derives every address and every string from
  `orig/g.exe`, and asserts `data/beer_uncited.json` covers exactly the branches
  the recomputation reports for this entry — a count **derived from
  `data/branches.json`, never hard-coded**. A literal `19` in an assertion is a
  check that cannot fail.
- Mutation cases in `tools/mutations.json` defending the new artifacts.

**Report contract:** the three classification counts, separately, and every
divergence found between the shipped `Game::beer` and the recovered flow —
including "none", if that is the honest answer, with the comparison that
supports it.

---

### Task 42: Fix what diverges, then cite what is already there

**Consumes Task 41.** Two halves, reported separately.

**Half 1 — behaviour.** Every branch Task 41 classified `unimplemented`,
implemented where it is behaviour the port can reach, or recorded in
`docs/re/gaps.md` with what blocks it — never silently dropped. Every
divergence Task 41 found between the shipped `Game::beer` and the recovered
flow, fixed or recorded as a deliberate divergence in `docs/re/gaps.md`.

Tests that fail if a gate's order, a message's text or the drink loop's
termination is wrong — the existing `Game::beer` tests at `src/game.rs:8080`
onward are the starting point, not the ceiling. Not tests that assert the code
calls itself.

**If Task 41 finds no divergence and nothing unimplemented, Half 1 is
legitimately empty** — say so in the report with Task 41's evidence, and do not
manufacture a change to make the number non-zero. That is the one case where a
`src/` diff of only citations closes this task, and it must be stated, not
implied.

**Half 2 — citations.** For every branch Task 41 classified `implemented`, put
its address (its own or its guard's — the metric accepts either) on the `src/`
construct Task 41 named. Correct, rather than extend, the `Game::beer` doc
comment's near-miss addresses: an address four bytes off the guard is worse
than no address, because it reads as verified.

**Report contract.** Two numbers with the command that produced each: *N
branches newly touched because this task implemented them*, *M newly touched
because this task cited code that already implemented them*, `N + M` equal to
the recomputed delta, and `lost 0`. Use the per-task delta block in
`docs/re/branches.md` under *Recomputation → Coverage → The per-task delta*,
run against a `git worktree` of the base commit, never the live tree. **A
single combined number does not satisfy this contract.**

Also update: `docs/re/branches.md`'s coverage sentence, its per-entry output
block and span ranking with the figures this task's tree actually prints (the
5-branch span `1000:2bc0..1000:2c52` is in the ranking's top rows and must be
re-derived, not edited by hand); `docs/superpowers/RESUME.md`'s
measured-history table with a new row.
