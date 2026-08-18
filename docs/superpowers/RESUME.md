# GOPNIK Rust port — SDD progress ledger

**Plan:** `docs/superpowers/plans/2026-08-17-gopnik-rust-port.md` (committed — the plan is the source of truth, and it has been revised several times)
**Repo:** `/home/finkel/work_self/gopnik-rs`
**Branch:** `port/gopnik-rust`

**Task order (revised three times):** 1, 2, 4, 4b, 4c, 2b, **2c**, 3, 5, 6, 7, 8, 9, 9b, 10, 11, 12

## Completed

| Task | Status | Commits |
|---|---|---|
| 1 — corpus verification | complete, approved | `e56e8a6..60c190c` |
| 2 — blind string scan | complete, approved | `60c190c..5e105da` |
| 4 — Ghidra headless export | complete, approved | `18ecf56` |
| 4b — string pointers from immediates | complete, approved (3 review rounds) | `2352278`, `70f0707`, `77a8795`, `a486acb` |
| 4c — indexed string array tables | complete, approved | `534bfe8` |

| 2b + 2c — pointer-anchored re-extraction + gap tiling | complete, approved (2 review rounds) | `01de56b..4fd2fda` |
| 3 — DOSBox-X oracle harness | complete, approved (3 fix waves) | `3f372a0..f44b6f7` |
| 5 — .SAV decoder + layout artifact | complete, approved (first pass) | `8973100` |
| 6 — Rust crate skeleton + text layer | complete, approved (1 fix wave) | `85c38b3`, `2533d35` |

**NEXT: Task 7.**

### Task 6 outcome

Crate is live: `cargo 1.97.1`, deps `serde` + `serde_json` only, `Cargo.lock`
committed. `src/text.rs` is the markup boundary — `parse()` is the single
primitive, `render()` and `strip()` are both built on it. 16 tests, no warnings.

Owner approved amending the plan's own test code: one test's name contradicted
its assertion (`caret_not_followed_by_digit_is_literal` asserted
`strip("2^3") == "2"`, where `^3` IS a valid code), and four edge cases were
untested. Split and covered. Reviewer independently re-derived all four
expected values from `parse`'s control flow — none was fitted to run output.

### Task 5 outcome

`data/save_layout.json` (694 B, schema `{"size", "fields":[{"name","off","kind","len"}]}`)
and `tools/decode_save.py` are in. Task 7 generates the Rust `save.rs` against
that exact schema. `hp` @ `0x210` and `hpmax` @ `0x212` are the only
semantically confirmed words; the eight stat words at `0x200` and the tail
from `0x214` stay `unk_*` until Task 9 pins them from disassembly.

The owner-approved amendment landed correctly: `_check_offsets()` rebuilds the
named regions from decoded values only, never touching `_raw`, and hardcodes
its own `CHK_OFF_*` literals rather than importing them from `decode_save` —
so a wrong offset cannot be self-consistent. The implementer caught that exact
tautology in its own first draft. Reviewer traced per-field that the check
fails for a wrong `magic`, `name`, `stats`, `hp` or `hpmax` offset, not just
the perturbed one. The stats-block slice check is the only thing in the suite
that would catch a wrong `OFF_STATE` at all, since `EXPECT` has no ground
truth for `stats`.

### Task 3 outcome — the oracle works, and how

Headless capture via `-c screenshot`/autotype was a dead end and was replaced.
`g.exe > OUT.TXT` yields 0 bytes (Borland Crt writes straight to VGA text
memory), and dosbox-x `-c` commands only fire when the shell is idle, so
autotype after `-c g.exe` never runs until the game exits.

The harness instead loads a TSR (`tools/oracle/scrhook.asm`/`.com`) that hooks
INT 16h: on every blocking key read it appends the 80x25 text buffer to
SCREEN.BIN and answers the read from a scripted key file. Serving keys from
the handler is what makes it deterministic — the Nth key request gets the Nth
scripted key, so nothing depends on autotype pacing, emulator speed, or the
15-key BIOS buffer, and scripts are not bound by the 127-char DOS command line.

**Consequences later tasks must know:**
- Only input-request screens are captured. A screen the game overwrites
  between two key requests is never seen. Task 9 should script fights so each
  interesting screen is followed by a key request (it naturally is).
- Pass `--expect-frames`. The host stops the run after SCREEN.BIN is quiet for
  3s, which is a wall-clock judgement; a stall truncates the capture and a
  truncated capture is otherwise indistinguishable from a complete one.
  `run_oracle.sh` forwards `--expect-frames` and `--timeout` through to
  `capture.py`.
- Key script limit is 1024 bytes (the TSR's buffer); longer is refused.
- A key request made while DOS is busy (InDOS) is neither captured nor
  answered.
- `data/oracle_prompts.json` is authoritative for which prompt consumes which
  key; the table in `docs/re/oracle.md` is a hand copy.
- Determinism is empirical, not proved: 5 runs across 2 scripts agree byte for
  byte, including an RNG-driven outcome. If a later task sees drift, first
  suspect is the game seeding from the emulated clock.

Three fix waves. Round 1: `--timeout` parsed but never forwarded, no
truncation guard. Round 2: both fixes unreachable from `run_oracle.sh` (the
interface Tasks 8/9/12 actually call), untested, and the prompt/key RE finding
had no `data/` artifact. Round 3: the new tests were anchored at the helper,
not the call site — reverting the guard's wiring line or the shell's `"$@"`
forwarding both left the suite green. **The recurring shape: a fix that is
correct in the code but unreachable or untested at the site that broke.**

Controller ruling: the "citing the Ghidra address" half of the two-places
constraint binds static-disassembly findings. This task's prompt/key finding
is behavioral, recovered by driving the emulator, so it has no address to
cite. Not a gap.

### Task 2b + 2c outcome

`data/strings.json` is 796 entries: 695 pointer-anchored + 54 table + 47
gap-tiled. The truncation the owner caught is fixed — `0xBCDD` reads
`...сломают челюсть)`. The blind scanner is deleted.

### Task 2b outcome

`data/strings.json` is now 749 entries: 695 pointer-anchored + 54 table.
The truncation the owner caught is fixed — `0xBCDD` reads
`...сломают челюсть)`. The blind scanner is gone.

The plan's mid-word-cut check was **structurally broken** and was replaced
(commit `b4d8f14`). Strings are packed with no delimiter, so the byte after
any string is the next string's length byte, and ordinary lengths (48–57,
65–90, 97–122) all land in the "alphanumeric" ranges it tested. Measured 39
false positives on correct data; a same-alphabet-class variant still gave 3.
**No next-byte rule can work here** — do not reintroduce one.

### Three controller errors in this task pair — read before trusting a measurement

1. **A letter-byte condition was added to gap tiling, then reversed**
   (`309f3a4` → `8136bbc`). It rested on "~13% of random windows tile, flat
   across gap lengths 2–40, so tiling is a coin flip." That sample spanned
   `0x18D0`–`0x158F2` and silently included a tail that is **69% NUL**; a run
   of `0x00` is a chain of zero-length strings that tiles at any length. The
   flatness was the artifact announcing itself. Per region, `0x18D0`–`0x11000`
   (where every recovered string lives) tiles at **0.1–1.7%** — for a 2-byte
   gap that is just `P(byte == 0x01)` = 1.64%. **Tiling between two verified
   anchors is strong evidence.** Do not re-add a content filter.
2. **"unaccounted dropped 14 → 1" was claimed as strong evidence. It is not.**
   `test_string_pointers.py` skips `suspect` entries, and 44 of the 47
   gap-tiled are suspect, so that metric is near-self-referential. The real
   figure is **11 of 14 residual offsets covered**, 3 uncovered (`0x42B0`,
   `0x11204`, `0x122EB` — the ones Task 4b independently called blind-scan
   artifacts with no code reference).
3. **"0 tiling violations" only proves the overlap half.** The gap half
   evaluates **0 pairs**, because `gap_tile()` fills exactly the gaps it
   inspects. Tiling also *masks* anchor loss by re-emitting the string from
   the widened gap — dropping 20 real pointers still left 781–791 entries.
   The test now pins three exact counts (796 / 695 / 47) instead of a floor;
   that is what actually detects a lost anchor.

Each of the three was caught by a reviewer, not by the controller. Keep
reviewers explicitly prompted to attack the controller's reasoning, not just
the implementer's.

## Current verified state

All seven suites pass:
```
python3 tools/verify_corpus.py         -> OK 7 corpus files verified
python3 tools/test_extract_strings.py  -> OK 796 strings extracted, 77 flagged suspect
python3 tools/test_string_pointers.py  -> OK 695 string pointers recovered, 3 unaccounted
python3 tools/test_string_tables.py    -> OK 54 table entries extracted
python3 tools/oracle/test_oracle_smoke.py -> OK 8 checks, ~1.75s, 2 dosbox-x launches
python3 tools/test_decode_save.py       -> OK 5 saves decoded and round-tripped
cargo test                             -> ok. 16 passed; 0 failed; 0 warnings
```

- `data/strings.json` — 796 entries (695 pointer-anchored + 54 table + 47 gap-tiled). Trustworthy; rebuilds byte-identically from the two input artifacts.
- `data/string_pointers.json` — 695 pointer-anchored offsets. Trustworthy.
- `data/string_tables.json` — 11 ranks + 43 крутизна entries. Trustworthy.

**Task 11 must know:** the yes/no confirmation token for the save and quit
prompts is **not** in `data/strings.json`. The suspect-neighbour rule
correctly refuses `0x8D79 'y'`, `0x9BF1 '\'`/`'y'` and `0x9D5E 'w'` because
their anchors (`save_r0.sav`, `save_r`, `run`) are pure ASCII and so get
flagged `suspect`. Recover it from the disassembly; do not assume it is
present. Also: Task 11 must not filter on `suspect` — 44 of the 47 verified
gap-tiled tokens carry `suspect: true`, and the field cannot distinguish
them from byte noise.

## Decisions made (do not relitigate)

- `^0`–`^7` is markup, not content. Parsed into spans; `plain` strips it, `text` keeps it. Raw sigils allowed ONLY inside byte-exact save round-trips.
- Garbage entries are **flagged (`suspect`), never deleted** — deleting destabilises offsets.
- **No frequency/reuse-based filtering** of string candidates, ever. It discarded real text once already.
- Operand extraction uses `getScalar` (immediates only). Never `getOpObjects` — it decomposes `[BP+4]` into false candidates.
- **RNG fallback approved by the owner:** try to recover the original generator; if impossible, use a self-contained PRNG (NOT the `rand` crate), report DONE_WITH_CONCERNS, and delete the vector-comparison tests rather than seed them from our own implementation. This makes Task 12's differential test deterministic-values-only.
- Task 11 must contain NO placeholder handlers or dummy enemies (owner chose rubric over plan).
- **Task 5 amendment (owner-approved, option 2):** the plan's round-trip test
  is near-tautological — `encode()` starts from `rec["_raw"]` and copies the
  tail verbatim, so every unnamed byte round-trips regardless of correctness,
  and a wrong-but-consistent `OFF_HP` would still pass. The plan's comment
  claiming it "proves we account for every one of the 694 bytes" is FALSE and
  must be deleted. Task 5 additionally builds the named regions from the
  decoded fields WITHOUT `_raw` and asserts those bytes match the original.
  The tail stays declared opaque. Reason: Task 7's Rust `save.rs` is generated
  from these offsets; a silently wrong offset propagates into the port.

## Carried forward — Task 11 (rendering / print orchestration) must decide this

**Trailing colour codes are irrecoverably dropped by `parse()`.** In the
original, a Borland Crt colour directive sets terminal state that persists into
whatever is printed *next*. Our `Span` model cannot represent "colour is now
active, no text yet": the post-loop flush in `src/text.rs` is gated on
`!buf.is_empty()`, so `parse("abc^4")` returns exactly `[Span{None,"abc"}]` —
the `^4` leaves no trace at all — and `parse("^4^7abc")` returns a single
White span, silently discarding the Red. `render()` also emits `\x1b[0m` at the
end of every string.

Not a Task 6 bug: the brief specified a per-string primitive and that is what
was built. But the information is gone by the time `parse` returns, so this
CANNOT be patched downstream from a `Vec<Span>` — fixing it means changing
`parse`'s output shape (e.g. an explicit trailing-colour field).

Task 11 must check the disassembly for whether a game string ever ends in a
colour code intended to tint the following output. If yes, `Span`/`parse` grow
a slot for it. The plan's fidelity constraint covers colour index, so this is
in scope for fidelity, not cosmetics.

## Minor findings deferred to the final whole-branch review

Triage these before merge; none blocked their task.

- **Task 5** `tools/test_decode_save.py` — no test exercises `decode()`'s
  wrong-length `ValueError` guard. The guard is correct; nothing calls it with
  a non-694-byte blob.
- **Task 5** `tools/decode_save.py` `encode()` — `buf[OFF_TAIL:] = rec["tail"]`
  and the `stats` loop have no length assertions. `bytearray` slice assignment
  silently resizes, so a caller building a `rec` with a wrong-length `tail` or
  `stats` of length != 8 gets a silently corrupted file instead of an error.
  Latent (tail/stats currently always come from a same-length `decode()`), but
  Task 7 generates Rust from this path.

## Known open items

- ~~14 blind-scan strings unrecovered by pointer anchoring~~ — **resolved by Task 2c.** The tiling check found them: 37 of 39 letter-bearing gaps tile exactly as complete Pascal shortstrings, and they are the game's command tokens (`s`, `sv`, `e`, `v`, `f`, `k`, `y`, `\`, `1`–`4`) plus a `С^ У^ П^ Е^` split banner. Task 4b's `N>=3` Cyrillic floor had excluded them. Task 11 compares input against these. The 2 non-tiling gaps sit between `suspect` entries and are code bytes — hence the check skips gaps beside suspect anchors.
- ~~whether headless capture works~~ — **resolved by Task 3.** It does; see the Task 3 outcome above for the mechanism and its limits.
- Accepted residual risk (Task 3, reviewer-flagged Minor, no action): `test_run_wires_frame_count_guard` patches three `capture.py` internals (`subprocess.Popen`, `_wait`, `decode_frames`) to reach `run()` without an emulator, so a harmless refactor of `run()` can break it. Proportionate given the no-second-emulator-launch constraint. If a fourth wave of stub-anchored tests is ever needed, reconsider injection seams instead.

## Workflow commands

```bash
SKILL=/home/finkel/.claude/plugins/cache/claude-plugins-official/superpowers/6.1.0/skills/subagent-driven-development
"$SKILL/scripts/task-brief"     docs/superpowers/plans/2026-08-17-gopnik-rust-port.md <N>
"$SKILL/scripts/review-package" <BASE_SHA> HEAD
```
Commit as: `git -c user.name="Claude" -c user.email="noreply@anthropic.com" commit`

NOTE: `.superpowers/` is git-ignored, so this ledger is NOT committed and
`git clean -fdx` would destroy it. `docs/superpowers/RESUME.md` is the
committed backup.
