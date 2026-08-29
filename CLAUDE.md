# Evidence

## `docs/re/METHODOLOGY.md` is binding. Read it before you claim anything.

Not a style guide — it is the standard every claim in this repo is held to, and
every review runs against it. It was reachable only from a task brief until now,
which meant an agent starting from this file could get all the way to a commit
without ever learning the rules its work would be judged by.

**The hierarchy: flow > state > output.** Flow is the disassembly — what the
instructions do. State is memory observed under a breakpoint. Output is what the
screen shows. Output can *falsify* a flow claim and can never *establish* one. A
screen, a help text, or a string you found is not evidence of behaviour; the
branch that produced it is.

**Every claim states its evidence tier and cites an address.** No address, no
claim. Probabilities come from the comparison constants that bucket a `Random`
result, never from counting outcomes.

**Address convention: two forms, two different arithmetics.** `tools/addr.py` is
the only place that arithmetic lives in code, and
`python3 tools/re_query.py resolve <citation>` runs it. Do not re-derive it by
hand and do not trust a `file` label without checking which form produced it —
this project has already shipped a `0x18d0`-byte miss (the header size) from
exactly that. Verify from an aligned instruction start, never from a byte-scan
hit. The normal size of an RE miss here is two to five bytes, which reads as
authoritative.

**The recurring defect, found by every review this project has run: a check that
cannot fail, presented as verification.** A tautological comparison. A guard
written against one past symptom rather than the class. An inventory whose
completeness claim stopped the next search. When a number or a guarantee matters,
recompute it from the shipped artifact and show the command that produced it.

### How to apply

- Decode the bytes. A citation you found by grepping for a literal is unverified
  — three of four wrong citations caught in Task 19 carried no literal at all, so
  no textual scan could have found them.
- State the runner with every number. `unittest discover` and `pytest` disagree
  by design in this repo and both are right; a count without its method is the
  defect above, committed.
- The five oracles under `data/` are ground truth and are **never** regenerated.
- When output and flow disagree, flow wins and the disagreement gets written
  down — in `docs/re/gaps.md` if it is a divergence this port keeps.

# Priority

## Completing the port is the goal. Tooling is not.

**The objective is a complete, working port of `orig/g.exe`.** Everything else —
the disassembly documents, the oracle captures, the mutation gates, the probes —
exists to serve that, and is worth building only up to the point where it does.

**Quality is not the thing being traded away.** This is not a licence to guess, to
skip the review loop, to assert from output what only flow can establish, or to
land a claim without an address. Every constraint in `docs/re/METHODOLOGY.md`
still binds. The trade being refused is *scope*, not *rigour*: build the smaller
correct thing, not the larger apparatus around it.

**What "yak shaving" looks like here, concretely.** Tasks 16 and 17 documented
**+103 branches** of the original between them and changed **zero** lines of
behaviour in `src/`. Before them, a whole session went into the mutation tooling
and moved branch coverage by **zero**. That tooling was not worthless — it found
three defects in Task 13's evidence, ten unasserted oracle columns, a dead
`Rng::set_state` — but it found **no port bugs**, and the game gained nothing a
player could see.

**Why the metric misleads.** Cited-branch coverage globs `docs/re/*.md` and
nothing else — `docs/superpowers/RESUME.md` records that it never counts `src/`
doc comments. So it rises indefinitely while the port stands still. Do not treat
it as progress on its own. **Progress is behaviour in `src/`.**

### How to apply

- **Pair every decompilation task with a porting task that consumes it.** Do not
  run two RE tasks back to back. When an RE task finishes, the next dispatch is
  Rust unless there is a stated reason otherwise.
- **A `src/` diff that is only comments does not close a porting task.**
- **Before building an instrument, say what port work it unblocks.** If the
  answer is "it would improve confidence in claims we already believe", that is a
  deferred-minor, not a task.
- **Reach for the existing instrument.** `tools/rngtrace/verbprobe.py`,
  `tools/re_query.py`, `tools/addr.py`, `tools/dis16.py`, `tools/mutate.py` and
  the five frozen oracles already cover most questions. Re-point them; do not
  build a sibling.
- **Fixing a document that contradicts itself is not yak shaving** — a wrong
  address propagates into every task that reads it. Correctness of the map is
  part of the port. Growing the map for its own sake is not.

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
