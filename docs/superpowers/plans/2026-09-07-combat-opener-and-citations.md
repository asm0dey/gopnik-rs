# The combat function's 117 uncited branches — the opener, and the honest split

**Spec authority:** `docs/re/METHODOLOGY.md` (binding evidence standard) and
the Global Constraints of `docs/superpowers/plans/2026-08-17-gopnik-rust-port.md`,
which this plan inherits verbatim and does not restate.

**Branch:** `main`, by explicit owner instruction ("develop on main").

## Why this slice, with the number

`data/branches.json`'s `port_touched` — a game branch counts when its own
address **or its guard's** appears as a `SEG:OFF` citation in `src/**/*.rs` or
`data/command_dispatch.json` — stands at **519 / 838 (61.9%)** at `b139158`,
recomputed by the block in `docs/re/branches.md` under *Recomputation, from the
shipped artifacts → Coverage*. **Do not trust a figure this plan quotes without
re-running it.**

Per function, the untouched population is now dominated by combat:

| func entry | what | total | touched | untouched |
|---|---|---:|---:|---:|
| `1000:3d11` | combat | 224 | 107 | **117** |
| `1000:ab59` | `entry`, main loop | 406 | 308 | 98 |
| `1000:1a03` | character sheet | 83 | 54 | 29 |
| `1000:6a0d` | — | 33 | 15 | 18 |
| `1000:29c4` | beer / `h`,`mh` | 19 | 2 | 17 |

`docs/re/branches.md`'s own span ranking agrees and says so in prose: *"no
location handler contributes a span of five or more any more, so what is left
at the top is inside `FUN_1000_3d11`"* — the crowd-taunt table
(`1000:4169`..`1000:43f5`, 17) and the combat opening (`1000:3d33`..`1000:3dc6`,
8). **117 is the largest single-function hole in the port, by a factor of 1.2
over `entry` and 4 over anything else.**

## The trap this plan is written around

Task 34's fix round moved **eleven of its forty-three** branches from uncited
to cited *without one line of behaviour changing* — purely by writing an
address beside a `matches!` that already implemented it. `docs/re/branches.md`
records that caveat: the behaviour-versus-commentary split is **form-dependent,
not principled**.

That is exactly the risk here. The crowd taunt table's eighteen lines are
**already fully implemented** in `Game::crowd` (`src/game.rs:6425`), verbatim
strings and all; its seventeen `cmp ax,N` guards are uncited only because
nobody wrote them down. Citing them moves the metric by 17 and the game by
zero.

So this plan does **not** treat "117 → 0" as the goal. It treats the *split*
as the deliverable: every one of the 117 is classified, the unimplemented ones
are implemented, the implemented ones are cited, and the report states both
counts separately. A task that reports a single number here has committed the
defect `docs/re/METHODOLOGY.md` names.

## What moves the metric, and what does not

`port_touched` reads `src/**/*.rs` and `data/command_dispatch.json` — it does
**not** read `docs/`. Measured five times in this repo's history, an RE-only
task moves this metric by **exactly zero**. Task 39 below is therefore paired
with Task 40, which consumes it, per `CLAUDE.md`'s *Priority* section, and **a
`src/` diff that is only comments does not close Task 40** — Task 40 must ship
the opener as behaviour, and its report must show the behaviour half of the
split as non-zero.

## Task-local constraints

Additional to the inherited Global Constraints:

- **`src/game.rs` is at ~10.6k lines and must not simply grow.** The opener
  goes in a new module (`src/combat_opener.rs`); `src/game.rs` keeps the call
  site. Moving *existing* unrelated code out of `src/game.rs` is out of scope.
- **The five oracles under `data/` are never regenerated.** The opener spends
  no draw (`docs/re/combat.md`: scanning `[0x3d11, 0x3f00)` for
  `9a 4b 11 78 0f` returns zero hits), so nothing in this plan may change a
  single RNG draw. `tests/combat_sequence.rs` staying green is the check.
- **State the runner with every test number.** `python3 -m unittest discover -s
  tools -p 'test_*.py'` is the project's number of record; `.venv/bin/pytest
  tools/ -q` disagrees by design and both are right. Report `skipped` too — `3`
  means `build/decomp/` was present, `16` means it was not.
- **Gates that must be green before a task reports DONE:** `cargo test`,
  `cargo clippy --all-targets` (zero warnings), `cargo fmt --check`, both
  Python runners, `python3 tools/mutate.py`, `python3 tools/difftest.py`.
- **Every new RE claim states its evidence tier and cites an address**, verified
  from an aligned instruction start with `python3 tools/re_query.py resolve
  <citation>` — never from a byte scan. Read the annotated decompilation
  (`build/decomp/FUN_1000_3d11_1000_3d11.c`, regenerate with
  `bash tools/ghidra/run_ghidra.sh --decomp-only`) as a **lead**, never as
  evidence.
- **A citation names the construct that evaluates the condition**, not a line
  number, and it goes on the `src/` site that actually implements it. Writing
  an address beside code that does not evaluate that condition is a false
  citation and is worse than leaving the branch uncited.

---

### Task 39: Map the class-keyed combat opener, and classify all 117

**RE only. This task changes no line of `src/`.**

**Range A — the opener:** `1000:3d32`..`1000:3e8a`. A `cmp [0x3952],N` chain
over the enemy class (`1000:3d35` `cmp ax,0x0` through `1000:3e35`
`cmp ax,0x9`), ten branches of which **10 are uncited**. `docs/re/gaps.md`
carries it as an open hole in two places (lines ~1766 and ~2391): *"Its text is
still not extracted"*. Task 13 established only that it spends no draw.

Recover, from flow:
- The exact extent of each of the ten class arms, its gates in the original's
  order, and its exit. `docs/re/gaps.md` already records that classes 0 and 6
  share the target `1000:3d32` and that the arm's only exit is
  `1000:3e8a jmp 0x3fa7` — **verify both, do not inherit them.**
- Every string each arm writes: file offset, the `mov di,imm16` that loads it,
  and the verbatim CP866→UTF-8 text. The known starts are files `0x452E`,
  `0x453B`, `0x4548`, `0x4565`, `0x457A`; that list is a lead, not a census.
- Whether any arm reads or writes state beyond printing. The zero-draw claim is
  established; a zero-*store* claim is not, and must be established or refuted
  the same way (a scan of the range for stores to `DS:` globals).
- The identity of `20ae:3952` with an `xrefs-to` census.

**Range B — the classification.** For **each** of the 117 uncited game branches
in `FUN_1000_3d11` (the list is reproducible with the block under
*Recomputation → Coverage* in `docs/re/branches.md`, filtered to
`func_entry == "1000:3d11"`), decide exactly one of:

- **`implemented`** — `src/` already evaluates this condition. Name the precise
  construct that does (module + function + the expression), so Task 40 can put
  the citation there and a reviewer can falsify the pairing.
- **`unimplemented`** — `src/` does not evaluate it. Say what it does, from
  flow, in one line.
- **`unreachable-in-port`** — the port deliberately diverges. Cite the
  `docs/re/gaps.md` entry that records the divergence, or open one.

The classification is the deliverable that Task 40 is scored against, so it
must be **falsifiable**: an `implemented` row whose named construct does not
evaluate that condition is a defect, and the reviewer will check a sample.

**Deliverables:**
- `docs/re/combat-opener.md` — the opener map, ten arms, evidence tier and
  address per claim.
- `data/combat_opener.json` — machine-readable twin, same shape as
  `data/gym_arms.json`.
- `data/combat_uncited.json` — the 117 rows, one per branch: `addr`, `guard`,
  `class` (`implemented` / `unimplemented` / `unreachable-in-port`), and for
  `implemented` the `src` construct.
- `tools/test_combat_opener.py` — re-derives every address and every string in
  `data/combat_opener.json` from `orig/g.exe`, and asserts
  `data/combat_uncited.json` covers exactly the 117 the recomputation block
  reports (a count that is derived from the artifact, never hard-coded — a
  hard-coded 117 is a check that cannot fail).
- Mutation cases in `tools/mutations.json` defending the new artifacts.

---

### Task 40: Port the opener, then cite what is already there

**Consumes Task 39.** Two halves, reported separately.

**Half 1 — behaviour (this is what closes the task).** New module
`src/combat_opener.rs` carrying the ten class arms with their verbatim strings
and their gates in the original's order; `src/game.rs`'s combat entry calls it
where `1000:3d32` sits in the original's flow. Plus every branch Task 39
classified `unimplemented`, implemented where it is behaviour the port can
reach, or recorded in `docs/re/gaps.md` with what blocks it — never silently
dropped.

Tests that fail if an arm's class key, gate order or string is wrong. Not tests
that assert the code calls itself. `tests/combat_sequence.rs` must stay green
draw-for-draw: the opener spends no draw, so any movement there is a bug in
this task, not a rebaseline.

**Half 2 — citations.** For every branch Task 39 classified `implemented`, put
its address on the `src/` construct Task 39 named. A citation that does not
land on code evaluating that condition does not ship.

**Report contract.** The task report states the split as two numbers with the
command that produced each: *N branches newly touched because this task
implemented them*, *M branches newly touched because this task cited code that
already implemented them*, `N + M` equal to the recomputed delta, and `lost 0`.
Use the per-task delta block in `docs/re/branches.md` under *Recomputation →
Coverage → The per-task delta*, run against a `git worktree` of the base
commit, never the live tree. **A single combined number does not satisfy this
contract.**

Also update: `docs/re/branches.md`'s coverage sentence, span ranking prose and
recomputation output block with the figures this task's tree actually prints;
`docs/re/gaps.md`'s two class-keyed-opener entries (close them, or say what is
left); `docs/superpowers/RESUME.md`'s measured-history table with a new row.
