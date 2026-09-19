# Reverse-engineering approach evaluation

Date: 2026-09-18

## Verdict

The approach is reasonable and, for a hobby/source-port reverse-engineering
project, unusually rigorous. It combines static control-flow recovery, dynamic
guest tracing, extracted state, differential execution, explicit provenance,
and falsification tests. The main weaknesses are reproducibility friction and
some remaining circularity/incompleteness in the oracle layer, not a flawed
core method.

Overall assessment: **strong methodology, currently imperfect execution**.

## What is particularly sound

1. **The evidence hierarchy is correct.** `docs/re/METHODOLOGY.md` distinguishes
   flow, state, and output, and refuses to infer dispatch or causality from
   visible output alone. That is the right posture for behavioral recovery.
2. **Address arithmetic is centralized and fail-closed.** The two segment
   conventions are documented, mechanically implemented in `tools/addr.py`,
   and tested rather than repeatedly recomputed by hand.
3. **Claims are tied to executable queries.** `tools/re_query.py` turns common
   citation, call-site, pushed-argument, and xref questions into repeatable
   commands. Port references use recomputation commands rather than unstable
   source line numbers.
4. **There is genuine triangulation.** The project uses disassembly/Ghidra
   exports, an independently checked 16-bit decoder, QEMU/GDB traces, save and
   memory state, and Rust differential tests. No single decompiler output is
   treated as unquestionable ground truth.
5. **Negative and uncertain findings are explicit.** Known gaps, uncovered
   tracer paths, divergences, and fields asserted by no test are recorded
   instead of being silently promoted to facts.
6. **Falsifiability is treated seriously.** `tools/mutate.py` perturbs copied
   artifacts and requires the named assertion to fail with a claim-specific
   diagnostic. The manifest contains 188 cases: 178 expected to go red and 10
   deliberately recorded as currently asserted by nothing.
7. **The original corpus is protected.** Mutation work occurs in a shadow tree,
   and the gate hashes `orig/`, `data/`, `tools/`, and `docs/` before and after.

## Important limitations

1. **Captured traces prove sampled executions, not total behavior.** Static
   branch recovery mitigates this, but dynamic evidence remains limited by the
   selected seeds, input scripts, and environment. The shop gap documented in
   `docs/re/rng-trace.md` is a good example of this limitation being handled
   honestly.
2. **Some oracle construction remains self-referential.** Custom extraction and
   tracing code can generate a wrong oracle that downstream Rust tests then
   reproduce. Differential checking against `ndisasm`, mutation cases, and raw
   byte assertions reduce this risk but do not eliminate it.
3. **Source mutation testing is not operational.** `pyproject.toml` explicitly
   says the `mutmut` setup is not yet runnable because of import-path identity.
   The custom artifact-mutation gate is valuable, but it is not a substitute
   for mutating the Python code that creates and folds the evidence.
4. **Reproduction depends on uncommitted external inputs.** Emulator-backed
   tracing requires a FreeDOS image and local QEMU/GDB tooling. The binary is
   present and hashable, but a machine-readable corpus/toolchain manifest with
   expected hashes and versions would make independent reproduction easier.
5. **Provenance coverage is uneven.** For example,
   `data/items.provenance.json` leaves many `price_src` fields null. Null can be
   legitimate, but each should carry an explicit reason/status so “not found,”
   “not applicable,” and “not yet investigated” cannot be confused.
6. **The full local validation entry point is fragile.** Running bare `pytest`
   concurrently with Cargo caused collection errors under transient `target/`
   directories because `target` is not excluded in pytest configuration.
   `pytest -q tools` avoids that collection problem.

## Current verification result

- `cargo test --all-targets`: passed on 2026-09-18.
- `pytest -q tools`: 686 passed and 3 failed. All three failures arise from the
  full mutation gate and share one current work-tree issue:
  `beer-gaps-divergence-entry` does turn its test red, but
  `tools/mutations.json` still expects the older diagnostic text after
  `tools/test_beer_arms.py` strengthened the message. The two safety-property
  tests fail because they run that same full gate.
- The working tree already contains uncommitted beer-analysis changes, so this
  is best interpreted as an incomplete review cycle rather than evidence that
  the overall method is unsound.

## Recommended priorities

1. Restore the mutation gate by updating the stale expected diagnostic for
   `beer-gaps-divergence-entry`, then rerun `pytest -q tools`.
2. Add `target` to pytest's excluded directories so Python and Cargo checks can
   run safely in parallel.
3. Make Python source mutation testing runnable; prioritize oracle-generation
   and trace-folding modules over broad mutation counts.
4. Add a machine-readable evidence manifest containing the original binary
   hash, external tool versions, trace environment, derivation command, and
   generated-artifact hash for each frozen oracle.
5. Replace provenance nulls with explicit statuses and require every important
   behavioral claim to state its confidence/evidence tier.

## Bottom line

Yes: the approach is reasonable. More strongly, its core epistemic discipline
is better than typical reverse-engineering ports. The project repeatedly asks
“what observation would make this claim fail?” and encodes the answer. The
remaining work is to make that rigor easier to reproduce and to extend it from
captured artifacts into the Python oracle-generating code itself.
