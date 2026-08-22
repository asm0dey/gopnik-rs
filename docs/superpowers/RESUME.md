# Resume checkpoint — gopnik-rs port

**Branch:** `port/gopnik-rust`. **Last checkpoint commit:** see `git log`; this file was last updated after Task 11f fix round 2.

`.superpowers/sdd/progress.md` is the full ledger but is **git-ignored** — a
`git clean -fdx` destroys it. This file is the committed backup. If they
disagree, trust `git log`.

---

## READ THIS FIRST: `cargo test` is GREEN as of Task 11f

The whole workspace passes with **zero warnings**. `cargo test --no-fail-fast`
reports 11 targets, 132 tests, 0 failed; `cargo clippy --all-targets` and
`cargo fmt --check` are clean.

Task 11c had deliberately left three tests red —
`run_{a,b,e}_replays_exactly` in `tests/wander_sequence.rs` — with one
enumerated cause: `FUN_1000_0d14` (`1000:0d14`..`1000:11bf`), the
random-encounter opponent roll, plus the fight-flow draws around it. **Task 11f
recovered all of it.** All five captured runs now replay their whole draw
stream (1387 draws) *and* their whole 29-variable `final_state`; the
`final_state` assertions for A, B and E were added in the same task, since the
reason they had been withheld was exactly the divergence that is now closed.

The assertions were not weakened to get there: `replay()` still compares site,
`n` and `r` over the whole run including the draw count
(`first_mismatch(.., usize::MAX)`). The constants `A_/B_/E_PREFIX` in
`tests/wander_sequence.rs` are no longer divergence indices — they only bound
the three preamble-prefix assertions, which are kept so a preamble regression
is localised rather than only reported from wherever the whole-run comparison
happens to break.

What `docs/re/gaps.md` still lists open around this area costs **no draw**:
the level>0 arm of combat's `run` (it needs the growth log, which this port
does not carry), the `[0x3c83]` arm of the same, the loot award on victory,
and the flavour text of wander buckets 1 and 4.

---

## State

Tasks 1–11 complete and reviewed (see git log). Since then:

| Task | What | Commits | Status |
|---|---|---|---|
| 11b | Wander `Random` catalogue (18 draws) | `61de765..f1602bd` | complete, reviewed |
| 11d | Live `Random` tracer (qemu+gdb) | `587f9b1..e344c63` | complete, reviewed |
| 11e | Ghidra branch enumeration | `e32aa71..f72c541` | complete, reviewed |
| 11c | Wander sequence wired into `src/` | `f72c541..a31f4a8` | complete, reviewed (two fix rounds) |
| 11f | `FUN_1000_0d14` + fight flow recovered; all five runs replay | `3fac24c` | complete, reviewed and approved; fix rounds 1 and 2 applied |

**Next action: Task 11g** — one address module, and the queries that keep
getting hand-rolled. Brief `.superpowers/sdd/task-11g-brief.md`.

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

134 of 838 game branches (16.0%) have their branch address or guard cited
anywhere in `docs/re/*.md`. Re-run it against the current tree with:

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

It printed `134 / 838`, exit 0, run against the tree at the Task 11f fix-round-2
commit. The `bool(...)` is load-bearing, not tidying: without it `hit` returns
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
| 406 | 74 | `1000:ab59` — main loop + command dispatch |
| 224 | 26 | `1000:3d11` — combat |
| **83** | **0** | **`1000:1a03` — nothing written about it at all** |

The metric undercounts (a function can be understood without every `jz` being
cited) and a citation is not comprehension. Re-run the query above to track
it.

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
