# Resume checkpoint — gopnik-rs port

**Branch: `main`.** Last updated after **Task 16**. The old feature branches
(`port/gopnik-rust` @ `f2d4fce`, `fix/task-13-review`, `feat/mutation-gate`) are
history now; `main` carries everything and PR #1 is merged.

`.superpowers/sdd/progress.md` is the full ledger but is **git-ignored** — a
`git clean -fdx` destroys it. This file is the committed backup. If they
disagree, trust `git log`.

---

## READ THIS FIRST

Everything is green: **167 Rust**, and for Python **333 by `unittest`** —

```
python3 -m unittest discover -s tools -p 'test_*.py'    -> Ran 333 tests, OK
.venv/bin/pytest tools/ -q                              -> 347 passed, 3 skipped, 668 subtests
```

**State the runner with the number, always.** The two disagree by design, both
are right, and the whole gap is one thing: pytest also collects the **17
module-level `def test_*` functions across 6 files** that `unittest` cannot see
(it only gathers `TestCase` subclasses — those functions still run, via each
file's `__main__` block). 9 are in `tools/oracle/test_oracle_smoke.py`, 4 in
`tools/test_extract_tables.py`, and one each in `test_decode_save.py`,
`test_extract_strings.py`, `test_string_pointers.py`, `test_string_tables.py`.
`347 + 3 = 350` collected, `350 − 333 = 17`, exactly. Subtests are NOT part of
the difference: pytest reports the 668 separately, on its own line, and does not
fold them into the 347. Re-measure with
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
exit 0 with **19 red** + 10 findings, `cargo clippy --all-targets` clean.
`cargo fmt --check` shows exactly three PRE-EXISTING diffs in
`tests/wander_sequence.rs` (lines 241, 973, 1100) — six reviewers have now
confirmed they predate current work. Leave them; do not let them mask a new one.

**The honest state of the project: the last session moved branch coverage by
three.** See "How much is actually traced" below. Tasks 13-review, 14 and the
mutation tooling are sound and found real defects in the *evidence*, but none of
it was decompilation and none of it found a port bug. Task 16 is the return to
the port.

### Four oracles now, all ground truth, none ever regenerated

| file | what | digest |
|---|---|---|
| `data/rng_trace.json` | 1387 wander draws, 5 runs, 29-var `final_state` each | `148fe3c7…1025` |
| `data/state_trace.json` | 91 per-turn samples of 35 guest variables | `6f7ae78a…13c7` |
| `data/combat_trace.json` | 15 whole fights across 4 runs, 1900 draws | `8c4b80e6…180acb` |
| `data/combat_vectors.json` | RNG vectors | `705415b2…f044` |

`combat_trace.json` records the first two files' digests inside itself.
`tools/mutate.py` now guards 91 files across `data/`, `orig/` and `tools/`, and
`combattrace.main()` refuses an `--out` naming a frozen oracle.

---

## State

Tasks 1–16 complete and reviewed. Since the last checkpoint:

| Task | What | Commits | Status |
|---|---|---|---|
| 13 | Whole fights captured; port replays 15 of 15 | `521db0e` | complete, reviewed (one fix round) |
| — | Task 13's fix round: 3 Important + 6 promoted Minors | `5e9776f..035c367` | complete |
| 14 | `tools/mutate.py` — the mutation gate for captured oracles | `035c367..57c29b8` | complete, reviewed (two fix rounds) |
| — | Mutation tooling: `cargo-mutants`, mutmut + pytest, `.venv/` | `5af1673..1a9338a` | complete |
| 15 | The eight missed `cargo-mutants` findings in `src/` | `c47abb9..f27b73c` | complete, **reviewed and approved, no fix round** |
| — | Task 15's four deferred Minors, batched | `1a9338a..0a29594` | complete |
| 16 | `FUN_1000_1a03` mapped — **it is the character sheet** | `a293f51..e3e3963` | complete, reviewed (one fix round) |

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

### What Task 14 built, and what it is for

`tools/mutate.py` mutates a **captured ground-truth artifact** and requires a
named test to go red. It is the executable form of a rule `METHODOLOGY.md` could
not previously enforce: *an assertion over a captured oracle is not evidence
until it has been observed failing.* 23 cases; 13 red channels; 10 registered as
`expect_red: false` — columns the capture holds that **no assertion reads**,
including `r_randseed_367e`/`e_randseed_367e`, which wander asserts per sample
and combat does not.

`cargo-mutants` covers the half that gate structurally cannot — mutating `src/`
itself. `-f src/combat.rs -f src/rng.rs` is now **76 mutants, 0 missed**.
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

**233 of 838 game branches (27.8%)** have their branch address or guard cited
anywhere in `docs/re/*.md`, measured after Task 16.

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
| 224 | 54 | `1000:3d11` — combat |
| 83 | 50 | `1000:1a03` — the character sheet, mapped in Task 16 |

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
3. The bulk, from `docs/re/gaps.md`: no `.SAV` load path and `write_save`
   returns `Unsupported`; shop purchase effects; the class-keyed combat-opener
   table (`1000:3d32..3e8a`); the rector death branch and hospital rescue
   (`1000:4f8c`); `sv`/`v`/`x`/`wes` dispatcher sites; shop modality;
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
