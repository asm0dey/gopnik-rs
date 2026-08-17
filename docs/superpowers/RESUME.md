# GOPNIK Rust port — SDD progress ledger

**Plan:** `docs/superpowers/plans/2026-08-17-gopnik-rust-port.md` (committed — the plan is the source of truth, and it has been revised several times)
**Repo:** `/home/finkel/work_self/gopnik-rs`
**Branch:** `port/gopnik-rust`

**Task order (revised twice):** 1, 2, 4, 4b, 4c, **2b**, 3, 5, 6, 7, 8, 9, 9b, 10, 11, 12

## Completed

| Task | Status | Commits |
|---|---|---|
| 1 — corpus verification | complete, approved | `e56e8a6..60c190c` |
| 2 — blind string scan | complete, approved | `60c190c..5e105da` |
| 4 — Ghidra headless export | complete, approved | `18ecf56` |
| 4b — string pointers from immediates | complete, approved (3 review rounds) | `2352278`, `70f0707`, `77a8795`, `a486acb` |
| 4c — indexed string array tables | complete, approved | `534bfe8` |

**NEXT: Task 2b** — re-extract `data/strings.json` anchored on the 695 recovered
pointers and merge in the 54 table entries, replacing Task 2's misframed output.

## Current verified state

All four suites pass:
```
python3 tools/verify_corpus.py         -> OK 7 corpus files verified
python3 tools/test_extract_strings.py  -> OK 696 strings extracted, 39 flagged suspect
python3 tools/test_string_pointers.py  -> OK 695 string pointers recovered, 14 unaccounted
python3 tools/test_string_tables.py    -> OK 54 table entries extracted
```

- `data/strings.json` — 696 entries from the OLD blind scan. **Known misframed**; Task 2b replaces it.
- `data/string_pointers.json` — 695 pointer-anchored offsets. Trustworthy.
- `data/string_tables.json` — 11 ranks + 43 крутизна entries. Trustworthy.

## Decisions made (do not relitigate)

- `^0`–`^7` is markup, not content. Parsed into spans; `plain` strips it, `text` keeps it. Raw sigils allowed ONLY inside byte-exact save round-trips.
- Garbage entries are **flagged (`suspect`), never deleted** — deleting destabilises offsets.
- **No frequency/reuse-based filtering** of string candidates, ever. It discarded real text once already.
- Operand extraction uses `getScalar` (immediates only). Never `getOpObjects` — it decomposes `[BP+4]` into false candidates.
- **RNG fallback approved by the owner:** try to recover the original generator; if impossible, use a self-contained PRNG (NOT the `rand` crate), report DONE_WITH_CONCERNS, and delete the vector-comparison tests rather than seed them from our own implementation. This makes Task 12's differential test deterministic-values-only.
- Task 11 must contain NO placeholder handlers or dummy enemies (owner chose rubric over plan).

## Known open items

- 14 blind-scan strings unrecovered by pointer anchoring; each enumerated with a reason in `docs/re/string-pointers.md`. 3 are scanner garbage, ~11 sit adjacent to 1–2 byte strings excluded by the `N>=3` floor. Runtime mechanism untraced.
- `dosbox-x` is installed; Task 3 must still establish whether headless capture works (`SDL_VIDEODRIVER=dummy` may fail on this SDL1 build; fallback is `xvfb-run`).

## Workflow commands

```bash
SKILL=/home/finkel/.claude/plugins/cache/claude-plugins-official/superpowers/6.1.0/skills/subagent-driven-development
"$SKILL/scripts/task-brief"     docs/superpowers/plans/2026-08-17-gopnik-rust-port.md <N>
"$SKILL/scripts/review-package" <BASE_SHA> HEAD
```
Commit as: `git -c user.name="Claude" -c user.email="noreply@anthropic.com" commit`

(Backup copy of the SDD ledger; the live one is `.superpowers/sdd/progress.md`, which is git-ignored.)
`git clean -fdx` would destroy it. `docs/superpowers/RESUME.md` is the
committed backup.

(Backup of the SDD ledger. The live copy is `.superpowers/sdd/progress.md`, which is git-ignored; this committed file is what survives `git clean`.)
