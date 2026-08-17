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

**NEXT: Task 3** — DOSBox-X oracle. Must first establish whether headless
capture works: `SDL_VIDEODRIVER=dummy` may fail on this SDL1 build, fallback
`xvfb-run`. Every differential test in Task 12 depends on this.

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

All four suites pass:
```
python3 tools/verify_corpus.py         -> OK 7 corpus files verified
python3 tools/test_extract_strings.py  -> OK 796 strings extracted, 77 flagged suspect
python3 tools/test_string_pointers.py  -> OK 695 string pointers recovered, 3 unaccounted
python3 tools/test_string_tables.py    -> OK 54 table entries extracted
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

## Known open items

- ~~14 blind-scan strings unrecovered by pointer anchoring~~ — **resolved by Task 2c.** The tiling check found them: 37 of 39 letter-bearing gaps tile exactly as complete Pascal shortstrings, and they are the game's command tokens (`s`, `sv`, `e`, `v`, `f`, `k`, `y`, `\`, `1`–`4`) plus a `С^ У^ П^ Е^` split banner. Task 4b's `N>=3` Cyrillic floor had excluded them. Task 11 compares input against these. The 2 non-tiling gaps sit between `suspect` entries and are code bytes — hence the check skips gaps beside suspect anchors.
- `dosbox-x` is installed; Task 3 must still establish whether headless capture works (`SDL_VIDEODRIVER=dummy` may fail on this SDL1 build; fallback is `xvfb-run`).

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
