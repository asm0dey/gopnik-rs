# Goal

## The fastest correct port of `orig/g.exe` to Rust.

Not the most thoroughly documented disassembly. The game is **16 functions,
38,264 bytes**. The port is already ~21,000 lines across 14 modules. This is a
completion job.

The method is **port first, verify after**, in four phases:

| phase | what | gate |
|---|---|---|
| 1 | **Gap survey** — read `build/decomp/<fn>.c` against existing Rust, list what has no counterpart | a list, nothing else |
| 2 | **Port** — transliterate from the decompilation | `cargo test`, `difftest`, the sequence oracles |
| 3 | **Audit** — run the evidence chain over the finished port | `docs/re/METHODOLOGY.md` in full |
| 4 | **Fix** what the audit found | as Phase 2 |

**Why this order.** Verification parallelises; gating does not. An evidence gate
in front of every ported line blocks that line's task. The same chain run over a
finished port fans out across many agents at once, and `tools/re_query.py`,
`tools/mutate.py`, `tools/difftest.py` and `data/branches.json` are all
read-only instruments built for exactly that.

**The cost, stated plainly.** Ghidra's C is sometimes wrong, and without a
per-line citation gate some of its errors will land in `src/` and be caught late
— by an oracle, by the Phase 3 audit, or not at all. That trade is deliberate.
The errors it admits are mechanical classes (signedness, 16-bit wraparound, a
mis-typed variable) that Phase 3 finds once and fixes as a sweep. It is cheaper
than gating every line to prevent them.

### How to apply

- **Know which phase you are in, and say so.** A Phase 2 task does not stop to
  write prose or place citations. A Phase 3 task does not change behaviour.
- **`build/decomp/<name>_<entry>.c` is the working source in Phase 2**, not a
  lead to be re-derived. Ghidra's C with every line annotated by the machine
  addresses that produced it. Gitignored; regenerate with
  `./tools/ghidra/run_ghidra.sh --decomp-only`, which writes nothing under
  `data/`.
- **When an oracle disagrees with the decompilation, stop and read the
  disassembly.** That is the one place Phase 2 owes evidence work.
- **Extend the oracles when a new subsystem lands**, in the same task — not
  afterwards. 43 difftest records and two sequence tests are the whole net.
- **Do not build instruments.** `tools/rngtrace/verbprobe.py`, `re_query.py`,
  `addr.py`, `dis16.py`, `mutate.py`, `decomp_addresses.py` and the five frozen
  oracles already cover the questions. Re-point them; do not build a sibling.

# Evidence

## Two regimes. Know which one binds you.

`docs/re/METHODOLOGY.md` is the standard, and in **Phase 3** it binds in full:
every claim states its evidence tier and cites an address, no address no claim,
verified from an aligned instruction start.

In **Phase 2** the gate is behaviour, not citation. Write the Rust, run the
oracles. You are not required to cite an address to port a line.

### What binds in every phase, without exception

- **The five oracles under `data/` are never regenerated.** If one disagrees
  with your port, the port is wrong until proven otherwise.
- **Flow > state > output.** Output can *falsify* a flow claim and can never
  *establish* one. A screen, a help text, or a string you found is not evidence
  of behaviour; the branch that produced it is.
- **`tools/addr.py` is the only place address arithmetic lives in code.**
  `python3 tools/re_query.py resolve <citation>` runs it. Two forms, two
  different arithmetics. Do not re-derive by hand and do not trust a `file`
  label without checking which form produced it — this project has shipped a
  `0x18d0`-byte miss (the header size) from exactly that. Verify from an aligned
  instruction start, never from a byte-scan hit. The normal size of a miss here
  is two to five bytes, which reads as authoritative.
- **When output and flow disagree, flow wins**, and the disagreement gets
  written down in `docs/re/gaps.md` if it is a divergence this port keeps.
- **State the runner with every number.** `unittest discover` and `pytest`
  disagree by design in this repo and both are right; a count without its method
  is a defect.

## The recurring defect: a check that cannot fail, presented as verification

Found by every review this project has run, and six times in the beer plan
alone. A tautological comparison. A guard written against one past symptom
rather than the class. An inventory whose completeness claim stopped the next
search — the cited-branch metric counts 87 branches cited by nothing but the
document that enumerates them.

When a number or a guarantee matters, recompute it from the shipped artifact and
show the command that produced it. A prose quantifier asserted by nothing is
wrong more often than not: five in the beer plan were.

# Priority

## Behaviour in `src/` is the only progress.

**Cited-branch coverage is not progress.** It globs `docs/re/*.md` and rises
whenever a document is written, including the inventory that lists every branch.
`port_touched` (651/838 at `979611e`) is a citation proxy and errs in both
directions; `data/branches.json`'s `port_cross_reference` says so.

**What yak shaving looked like here.** Tasks 16 and 17 documented +103 branches
between them and changed **zero** lines of `src/`. The beer plan shipped 9
commits, of which one touched Rust: 298 lines, against ~3,600 lines of documents
and tooling.

### How to apply

- **A `src/` diff that is only comments does not close a Phase 2 task.**
- **Before building an instrument, say what port work it unblocks.** "It would
  improve confidence in claims we already believe" is a deferred-minor, not a
  task.
- **Fixing a document that contradicts itself is not yak shaving** — a wrong
  address propagates into every task that reads it. Growing the map for its own
  sake is.
- **Dispatch subagents one at a time.** Spend rate is the constraint being
  managed, not wall-clock. Batch several functions into one dispatch rather than
  fanning out one agent per function; ask before parallelising.

<!-- jbcontext-instructions-start -->
# Tools

## Code discovery: context-explorer first

When a task requires finding or understanding code whose location you don't
already know, your FIRST code-discovery step MUST be:

Task(subagent_type='context-explorer',
     description=<short label>,
     prompt=<1-2 sentence intent describing what to find>)

Start there instead of opening with your own `grep`/`glob`/`bash` searches or
git history: the subagent runs the semantic exploration in its own context and
hands back concrete `file:line` references, so you don't burn your context
re-reading the same files.

This governs *how* you begin code discovery — not whether every task needs it.
Do NOT call context-explorer when the task doesn't involve locating code:

- the task names the exact file, class, or symbol — open it or grep directly;
- the relevant file is already open or identified;
- the work is a git operation (rebase, merge, commit), a test/build run,
  shell/statusline/config setup, or a review of a diff you already have.

Invoking context-explorer as a formality "to get started" on such tasks wastes
a subagent round and returns irrelevant findings. It is a research step, not a
gate to clear — skip it and proceed directly.

When you do use it, the subagent runs up to 3 semantic searches in its own
context (restricted to `jbcontext search` via `Bash` and `Read` only) and
returns a short report:

Searched: <one-line summary>
Findings:
- <relative/path>:<line> — <description>
- ...
Notes: <confidence; whether keyword grep would be more direct here>

Use its findings if they look useful, or ignore them entirely if `Notes:` flags
the task as keyword-based. You retain full freedom for the rest of the run.

## Semantic Code Search (jbcontext)

You have access to `jbcontext search` for searching the codebase semantically.
It finds code by meaning, not just keywords.

### Usage

```bash
jbcontext search "<detailed and descriptive query>"
jbcontext search -p <path> "<query>"  # <path> must be relative to the project root
```

### Query Tips

- Be descriptive: "function that validates user email addresses" > "email"
- Include context: "error handling middleware for HTTP requests with logging"
- Specify what you're looking for: "React component that renders a modal dialog"

### Single-Shot Policy

Use `jbcontext search` as a semantic bootstrap when the relevant file or subsystem is still unknown.

- If no relevant file is open yet, start with one `jbcontext search`.
- Make the first query specific to the issue's named feature, class, method, config flag, or behavior when available.
- After the first search, open at least one returned file and inspect it locally.
- If the first hit is relevant but incomplete, inspect neighboring files locally in that same directory or subsystem before any semantic retry.
- After the first relevant file or path is known, prefer direct file reads and exact search to inspect nearby code.
- If a semantic retry is still needed, use `jbcontext search -p <path> ...` with the directory of the best first hit.

### Examples

```bash
# Find authentication-related code
jbcontext search "user authentication login flow"

# Narrow to specific directory
jbcontext search -p src/auth "JWT token validation"
```

Use `jbcontext search` once to get the initial pointer, then inspect nearby code locally. If that still fails, do a narrowed retry with `-p`.
<!-- jbcontext-instructions-end -->

@AGENTS.md
