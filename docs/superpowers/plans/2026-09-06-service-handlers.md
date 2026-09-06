# The gym, the club, the vet, the joint and the command list — the five unported service handlers

**Spec authority:** `docs/re/METHODOLOGY.md` (binding evidence standard) and
the Global Constraints of `docs/superpowers/plans/2026-08-17-gopnik-rust-port.md`,
which this plan inherits verbatim and does not restate.

**Branch:** `main`, by explicit owner instruction ("develop on main").

## Why this slice, with the number

`data/branches.json`'s `port_touched` — a game branch counts when its own
address **or its guard's** appears as a `SEG:OFF` citation in `src/**/*.rs` or
`data/command_dispatch.json` — stands at **449 / 838 (53.6%)** at `72f1bae`,
recomputed by the block in `docs/re/branches.md` under *Recomputation, from the
shipped artifacts → Coverage*. That block reimplements
`tools/ghidra/EnumerateBranches.java`'s rule rather than reading its answer, and
its `git worktree add /tmp/wt82 82a08d8` validation still holds. **Do not trust
a figure this plan quotes without re-running it.**

Split `entry`'s 406 branches by dispatch handler, using
`data/command_dispatch.json`'s `confirmed_dispatch_chain` compare addresses as
the range boundaries (each handler runs from its own compare to the next verb's
compare). Measured at `72f1bae`:

| handler | range | untouched / total | largest contiguous cluster |
|---|---|---:|---|
| `run`/`w` wander | `ae97`..`b949` | 51 / 97 | 33, but in six disjoint groups |
| `trn` gym | `e390`..`e972` | **23 / 38** | **20** (`e633`..`e941`) |
| `rep` vet | `d3a6`..`d6ec` | **20 / 23** | 13 (`d4c1`..`d61e`) |
| `kl` club | `df06`..`e38f` | **15 / 20** | 6 (`e283`..`e366`) |
| `bmar` dealers | `c4be`..`d3a5` | 12 / 82 | — |
| `mar` market | `b94a`..`c4bd` | 12 / 61 | — |
| `pr` den | `d802`..`df05` | 9 / 44 | — |
| `i` command list | `ea94`..`ec81` | **8 / 8** | **8** (whole handler) |
| `kos` joint | `e973`..`ea93` | **3 / 5** | — |

`run`'s 51 is again the largest single count and again the worst yield: the
handler is already 46/97 ported and the residue is scattered over six groups of
five branches and under — edge cases, not a feature. Reproduced from the same
scan that produced the table above; the plan this one follows rejected `run`
for the same measured reason and nothing has changed it.

The five handlers this plan takes total **69 untouched branches**, every one of
them in a self-contained, single-command handler of the same shape as the den
(`pr`, +26) and the dealers' sell path (`wes`/`x`, +25) that the previous plan
ported. That is the largest coherent block available and the highest yield per
task on the board.

The bulk of `1000:3d11` (combat, 118 untouched, with a 44-branch cluster at
`5278`..`57c2`) is deliberately **out of scope**. It is larger but it is the
combat engine, it is guarded by five frozen oracles under `data/`, and it is not
a "port one handler" task; it deserves its own plan.

## What moves the metric, and what does not

`port_touched` reads `src/**/*.rs` and `data/command_dispatch.json` — it does
**not** read `docs/`. Measured four times in this repo's history (Tasks 25, 27,
29 and the Task 26 baseline), an RE-only task moves this metric by **exactly
zero**. Every RE task below is therefore paired with the porting task that
consumes it, per `CLAUDE.md`'s *Priority* section, and **a `src/` diff that is
only comments does not close a porting task.**

## Task-local constraints

These bind every task in this plan and are additional to the inherited Global
Constraints:

- **`src/game.rs` is at 10,542 lines and must not simply grow.** Each porting
  task in this plan puts its handler in a new module (`src/gym.rs`,
  `src/vet.rs`, …) or extends one an earlier task created, rather than
  appending to `src/game.rs`. Moving *existing* unrelated code out of
  `src/game.rs` is out of scope — that is a separate refactor, and mixing it
  into a porting diff makes the port unreviewable.
- **`data/shop_arms.json` is 168 KB.** A handler mapped by this plan gets its
  own `data/<handler>_arms.json`, not a fourth block appended to that file.
- **The five oracles under `data/` are never regenerated.**
- **State the runner with every test number.** `python3 -m unittest discover -s
  tools -p 'test_*.py'` is the project's number of record;
  `.venv/bin/pytest tools/ -q` disagrees by design and both are right.
- **Gates that must be green before a task reports DONE:** `cargo test`,
  `cargo clippy --all-targets` (zero warnings), `cargo fmt --check`, both Python
  runners, `python3 tools/mutate.py`, `python3 tools/difftest.py`.
- **Every new RE claim states its evidence tier and cites an address**, and
  every address is verified from an aligned instruction start with
  `python3 tools/re_query.py resolve <citation>` — never from a byte scan.
  Three of four wrong citations caught in Task 19 carried no literal at all.

---

### Task 31: Map the gym (`trn`) and the joint's residue (`kos`)

**RE only. This task changes no line of `src/`.**

**Range:** `1000:e390`..`1000:e972` (`trn`, 38 branches, 23 untouched) and
`1000:e973`..`1000:ea93` (`kos`, 5 branches, 3 untouched).

Ported today: the gym's *first* block, `1000:e400`..`1000:e594` (see
`Game::enter_shop(Location::Gym)` and the two gym tests at `src/game.rs:9541`
and `:9553`), and `Game::smoke` at `src/game.rs:3575`. Everything from
`1000:e633` to `1000:e941` is unported — a second block of the same shape as the
first (money `cmp word [0x38c7]` against `0x14`/`0xa`/`0x1e`, district
`cmp byte [0x3692]` against 1/2/4, and stat ceilings against `[0x38a6]` and
`[0x38d0]`) — as are three gates in the first block (`1000:e395`, `1000:e3b6`,
`1000:e3d4`, reading `[0x38b7]` and `[0x38b9]`) and the `kos` gates at
`1000:e978`, `1000:e9a5` (`[0x38cd]`) and `1000:e9d5` (`cmp ax,0xa`).

**Recover:**
- What the second block is and how it is reached from the first — establish it
  from flow, not from the printed menu. Whether it is a second menu tier, a
  repeat-purchase loop, or a distinct sub-verb is exactly the question.
- Every arm: its span, its gates in order, its price, its effect on state, and
  the string it writes (file offset + verbatim CP866→UTF-8 text).
- The identity of `20ae:38a6`, `20ae:38b4`, `20ae:38b6`, `20ae:38b7`,
  `20ae:38b9`, `20ae:38d0`, `20ae:394a` and `20ae:38c5` — each with an
  `xrefs-to` census, each named `unk_<hex>` if not established.
- The three unported first-block gates: what they skip and why the ported half
  does not need them (or that it does, which is a port bug to record).

**Deliverables:** `docs/re/gym.md`, `data/gym_arms.json` (machine-readable twin,
same shape as `data/den_arms.json`), and `tools/test_gym_arms.py` re-deriving
every address and every string in the artifact from `orig/g.exe`. A
`what_the_port_must_change` block naming, per arm, what `src/` does today and
what it must do — written so a later port can falsify it.

---

### Task 32: Port the gym's second block and the joint's residue

**Consumes Task 31.** Target: the 26 branches Task 31 mapped become
`port_touched`.

- New module `src/gym.rs` carrying the gym handler; `src/game.rs` keeps the
  dispatch arm and delegates.
- Every arm Task 31 established, with its gates in the original's order and its
  verbatim strings.
- `kos`'s three untouched gates, including the `cmp ax,0xa` at `1000:e9d5`.
- Tests that fail if an arm's gate order, price or ceiling is wrong — not tests
  that assert the code calls itself.
- Update `docs/re/branches.md`'s recomputation output block and the coverage
  sentence with the figure this task's tree actually prints, and record in
  `docs/re/gaps.md` any divergence this port deliberately keeps.

---

### Task 33: Map the club (`kl`) and the command list (`i`)

**RE only. This task changes no line of `src/`.**

**Range:** `1000:df06`..`1000:e38f` (`kl`, 20 branches, 15 untouched) and
`1000:ea94`..`1000:ec81` (`i`, 8 branches, all 8 untouched).

**The `i` handler is a known port divergence, not just uncited branches.**
`Game::show_command_list` (`src/game.rs:2380`) prints thirteen fixed lines with
no citation and no gating. The original prints one ungated line at
`1000:ea9e` and then gates seven further lines on
`cmp byte [0x3694],1` … `cmp byte [0x369a],1` — the seven discovery flags — at
`1000:eab7`, `ead7`, `eaf7`, `eb17`, `eb37`, `eb57` and `eb77`. Establish the
full line list, each line's file offset, and which flag gates it. Note the
count: thirteen ungated lines in the port against a gated list in the original
means the port's line inventory itself needs re-deriving from flow, not
assuming the thirteen are the same thirteen.

For `kl`, ported today are only the entry gate and the first two price arms
(`1000:df74` `cmp [0x38c7],0xf`, `1000:dfd0` `cmp [0x38c7],0x16`, and the
`[0x3692]` district gate at `1000:dfc4`). Unported: `1000:e074`..`1000:e0ce`
(a money gate at `1000:e07e` `cmp ax,[0x38c7]`, then the `cmp dx,bx` /
`cmp ax,cx` pair at `1000:e0c6`/`1000:e0cc` — the signed-high/unsigned-low
32-bit compare idiom `Game::luck_below_random_32` already models, whose two
operands this task must establish), `1000:e14f`..`1000:e179` (`[0x3c82]`
against `0x11` and `0x5`), and `1000:e283`..`1000:e366` (a block whose guards
read the same `[0x38c7]` constants `0xf` and `0x16` and the same `[0x3692]`
district gate as `1000:df74`..`1000:dfd0`).

**Recover:** every arm's span, gates, price, effect and string; the identity of
`20ae:3c82` and `20ae:3b77` with an `xrefs-to` census; and whether
`1000:e283`..`1000:e366` is a duplicate of `1000:df74`..`1000:dfd0` or a
distinct arm — the identical guard constants are a hypothesis, not a finding.
Measure the byte diff of the two spans and say what differs, the way Task 27
measured the den's three reveal predicates (two byte-identical, one 43 bytes
against 52) instead of assuming symmetry.

**Deliverables:** `docs/re/club.md`, `data/club_arms.json`,
`tools/test_club_arms.py`, and a `what_the_port_must_change` block for each of
the two handlers.

---

### Task 34: Port the club and the command list

**Consumes Task 33.** Target: the 23 branches Task 33 mapped become
`port_touched`.

- New module `src/club.rs`; `src/game.rs` delegates.
- `Game::show_command_list` rewritten against the flow Task 33 established:
  the ungated line, then one gated line per discovery flag, each citing its
  compare address. A test that turns each flag on and off independently and
  asserts the printed list changes — `term::capture` already exists for this
  (added in Task 28, `src/term.rs`).
- Update `docs/re/branches.md`'s recomputation block and coverage sentence, and
  `docs/re/gaps.md` for any kept divergence.

---

### Task 35: Map the vet (`rep`)

**RE only. This task changes no line of `src/`.**

**Range:** `1000:d3a6`..`1000:d6ec` — 23 branches, 20 untouched. The most
nearly-unported handler on the board: only the entry gate (`1000:d3b0`
`cmp byte [0x3698],1`) and the two price checks at `1000:d410`
(`cmp word [0x38c7],3`) and `1000:d465` (`cmp word [0x38c7],7`) are cited.

Unported: `1000:d3da`..`1000:d3f2` (HP against `[0x38ae]`, then `[0x38b0]` and
`[0x38b1]`), the 13-branch block `1000:d4c1`..`1000:d61e` (the same three
globals again, then `cmp word [0x38c7],7`, then an `ax` dispatch against 1 and
2 at `1000:d5fb`/`1000:d61b`), and `1000:d6b2`/`1000:d6c3`.

**Recover:** every arm, its gates in order, its price, its effect on HP and on
`[0x38b0]`/`[0x38b1]`, and its string; the identity of `20ae:38ae`,
`20ae:38b0` and `20ae:38b1` with an `xrefs-to` census — `[0x38b0]` and
`[0x38b1]` are read by the vet, by `kos` (`1000:e97d`) and by the wander
handler (`1000:b27b`, `1000:b282`), so the census spans handlers; the `ax`
dispatch at `1000:d5fb`/`1000:d61b` — what `ax` holds and where it was set.

**Deliverables:** `docs/re/vet.md`, `data/vet_arms.json`,
`tools/test_vet_arms.py`, and a `what_the_port_must_change` block.

---

### Task 36: Port the vet

**Consumes Task 35.** Target: the 20 branches Task 35 mapped become
`port_touched`.

- New module `src/vet.rs`; `src/game.rs` delegates.
- Every arm with its gates in the original's order, the `ax` dispatch, and the
  verbatim strings.
- Tests that fail on a wrong gate order or a wrong price.
- Final coverage recomputation: re-run the `docs/re/branches.md` block and
  replace both the headline sentence and the verbatim output block with what
  this task's tree prints, plus one line in the measured-history sentence.

---

### Task 37: Address-annotated decompilation

**Moved to the end of the plan (ruling R10).** It was drafted as Task 32b,
between the gym's port and the club's map. It is tooling: it moves
`port_touched` by zero, and it accelerates the RE half of a task, not the
writing of Rust. Three porting tasks therefore run ahead of it. Its
validation targets -- the den, the dealers' sell path and the market -- are
already mapped and do not expire.

**Tooling task. It moves `port_touched` by zero and that is expected** — the
port work it unblocks is named below, per `CLAUDE.md`'s requirement that an
instrument declare one before it is built.

**Why.** Every wrong citation this project has shipped came from a hand-derived
or byte-scanned address: the `0x18d0` header-size miss; the misaligned
`1000:ce99`, which is the last byte of `1000:ce97 mov [0x38c9],ax`; three of the
four bad citations caught in Task 19, which carried no literal at all and so no
textual scan could have found them. **An address Ghidra emits from its own
instruction listing is aligned by construction**, so this removes that defect
class at the source instead of catching it in review. The secondary gain is
speed: it cuts the arm-finding half of every later RE task.

**Port work unblocked:** combat, `1000:3d11` — 224 branches, **118 untouched**,
the one slice where hand-walking disassembly is the actual bottleneck. Tasks 33
and 35 take a smaller share of the same gain.

**Files:** modify `tools/ghidra/ExportAll.java` and `tools/ghidra/run_ghidra.sh`;
create `tools/test_decomp_addresses.py` and its golden file.

`ExportAll.java` already calls `DecompInterface` and writes 123 files to
`build/decomp/` — gitignored, documented in `docs/re/functions.md`, and cited by
`docs/re/rng.md` and `docs/re/command-dispatch.md`. That C carries no addresses
today, which is why `docs/superpowers/RESUME.md` ranks it a lead rather than a
citation.

**Build:**

1. Annotate each emitted C line with the **set** of machine addresses that
   contributed to it, from `DecompileResults.getCCodeMarkup()` and the
   `ClangLine` tokens' addresses.
   **`getMinAddress()` alone is forbidden.** A decompiled line merges several
   instructions; emitting one confident-looking address for it is exactly the
   "check that cannot fail, presented as verification" that
   `docs/re/METHODOLOGY.md` names. Emit every contributing address.
2. Give `run_ghidra.sh` a decomp-only mode that does **not** rewrite
   `data/branches.json`, `data/functions.json` or `data/string_pointers.json`.
   Those are committed, and this plan's whole coverage baseline is measured
   against them. If a full run is performed instead, show that
   `git diff --stat` over those three paths is empty.
3. The five oracles under `data/` are never regenerated.

**Falsification — this is the task, not a postscript.**
`tools/test_decomp_addresses.py`, over the shipped `build/decomp/`, must:

- assert every annotated address is a real **aligned instruction start**,
  decoded from `orig/g.exe` — not merely well-formed;
- for the three handlers already mapped and reviewed by hand — the den
  (`1000:d802`..`1000:df05`), the dealers' sell path
  (`1000:ce76`..`1000:d383`) and the market (`1000:b94a`..`1000:c4bd`) — take
  every branch address and guard address `data/branches.json` records in range
  and check it appears in the annotation of the enclosing function's file;
- record the resulting coverage as a **golden set** checked into the test, so
  later drift fails. Do **not** assert a hand-picked percentage threshold: a
  threshold chosen so it passes is the same defect one level up. Report the
  exact figure with the command that produced it, and list every address that
  did not appear.

A disagreement between the annotation and a known-good citation is the finding
this task exists to surface. If one turns up, report it — do not tune it away.

**Second checker, same theme — `src/` line-number citations.** `docs/` cites
`src/<file>:<line>` in many places and those line numbers drift every time
`src/` changes. Four were already stale when this task was written, found by
decoding rather than by any test: `docs/re/gaps.md` cites `src/save.rs:257`
for `beer_half_litres` (actually 255) and `src/persist.rs:345` for
`joints: it.joints.max(0) as u16` (actually 357), with `:350` and `:352`
shifted by the same amount. Every claim still held; only the line numbers had
moved. This plan then adds three modules and moves handler code, which will
invalidate more of them.

Extend `tools/test_decomp_addresses.py` (or add a sibling) to parse the
`` `src/f.rs:NNN` `symbol` `` pairs the docs already write and assert the symbol
appears at that line. Report the count checked and every pair that failed.
The same rule applies as above: no hand-picked pass threshold, and a pair the
parser cannot understand is reported, never silently skipped — an inventory
whose completeness claim stopped the next search is the defect this project
keeps finding.

**Docs:** one line in `docs/re/functions.md` stating that the annotation makes a
citation cheap to *verify*, not pre-verified, and that
`python3 tools/re_query.py resolve <citation>` is still required. The
lead-not-evidence ranking in `docs/superpowers/RESUME.md` does not move.

---
