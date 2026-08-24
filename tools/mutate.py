#!/usr/bin/env python3
"""The mutation gate: prove an assertion over a captured oracle CAN fail.

`docs/re/METHODOLOGY.md` is the human-readable authority for the rule; this
module is its executable form, the same way `tools/addr.py` is the executable
form of the address convention.

## Why this exists

Every review this project has run has found the same defect: **a check that
cannot fail, presented as verification.**  A tautological string comparison; a
guard written against one past symptom rather than the class; a scan whose
completeness claim was one value formatted against itself; a frozen-oracle
guard that read the first 116 lines of a 210-line file and missed the only
`write_text(`; a summary asserted against literals rather than against the
structure it summarises.  Reviews keep catching it.  Nothing prevented
authoring it.

A passing test is evidence of nothing until it has been observed FAILING.  So:
break what the assertion claims to check, run the test, and require a non-zero
exit.  A case whose test still passes is a failure OF THE GATE and is reported
by name.

## What the gate does per case

Five checks, because "the test went red" is itself a claim that can be made
vacuously:

  1. **green before** -- the named test must PASS on the unmutated copy.  A
     shadow tree that fails to build would otherwise make every case "red" and
     the whole gate would pass while proving nothing.
  2. **the mutation landed** -- the artifact's bytes must actually differ after
     the perturbation.  A no-op perturbation is the defect in miniature.
  3. **red after** -- the test must exit non-zero.
  4. **red for the RIGHT reason** -- the test's output must contain the case's
     `expect` string, which names the assertion being defended.  Without this a
     mutation that merely made the file unparseable would count as a pass.
  5. **restored** -- the artifact is copied back and re-digested against the
     real file.

A case may also carry `"expect_red": false`.  That is not a weaker case: it is
a recorded FINDING -- a column the captures hold that no assertion in the suite
reads, proved by mutating it and watching nothing happen.  Keeping those in the
manifest rather than dropping them keeps the record executable: the day an
assertion reaches one, the gate fails on it and the case is promoted.

And once per run, around everything: every file under `data/` and `orig/` is
digested before and after, and any change is a hard failure.

## The safety property

**The gate never opens a file under `data/` for writing.**  Not on success, not
on failure, not on crash, not on interrupt -- structurally, not by cleanup:
`_read_real()` is the only function that touches a real repo file and it opens
`"rb"`.  Everything else is confined to a `tempfile.mkdtemp()` shadow tree by
`_under()`, which rejects an absolute path, a `..` component, and any path
whose resolved form leaves the shadow root.  A mutation tool that can damage
ground truth is the very defect it exists to prevent.

Frozen oracles, whose digests are recomputed and printed on every run:

    data/rng_trace.json     148fe3c7...1025
    data/state_trace.json   6f7ae78a...13c7
    data/combat_trace.json  8c4b80e6...180acb

## What the gate covers, and what it does NOT

Covered: the assertions that consume the three frozen oracles plus
`data/combat_vectors.json`, one case per independently falsifiable channel --
see `tools/mutations.json`, where each case names the claim it defends.  Twelve
channels today, plus ten `expect_red: false` findings: columns those captures
hold that the gate found NO assertion reads.

NOT covered, and not claimed to be: every assertion in the project.  The gate
says nothing about `tests/progression.rs`, `tests/save_roundtrip.rs`,
`tests/data_load.rs`, `tests/term_output.rs`, `src/`'s unit tests, or the other
Python tools' suites.  A channel absent from `tools/mutations.json` has NOT
been shown to be falsifiable; absence from the manifest is silence, never a
pass.

Adjacent and worth knowing while reading a tool that defends `data/`:
`tools/test_extract_strings.py` and `tools/test_string_tables.py` DO write into
`data/` when run (`data/strings.json`, `data/string_tables.json` -- not the
frozen oracles, and the same bytes today).  This gate does not run them and
does not make that worse; fixing it is a separate job.

Usage:

    python3 tools/mutate.py                 # every case
    python3 tools/mutate.py --case LABEL    # one
    python3 tools/mutate.py --manifest P    # a different manifest

Exit status is non-zero if any case failed to go red, if any case is
malformed, or if any real file changed.  Standard library only.

Tests: `python3 tools/test_mutate.py`.
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
MANIFEST = REPO / "tools" / "mutations.json"

# Copied into the shadow tree.  `data` and `tools` are the ones a case may
# perturb; the rest is what has to be there for `cargo test` and the Python
# suites to run at all.
SHADOW_TREE = ("Cargo.toml", "Cargo.lock", "build.rs",
               "src", "tests", "data", "orig", "tools")

# Digested before and after every run.  Not just the three frozen oracles:
# `orig/` is the binary and the save corpus, and the rest of `data/` is
# extracted tables the port compiles against.
GUARDED = ("data", "orig")

FROZEN = ("data/rng_trace.json", "data/state_trace.json",
          "data/combat_trace.json")


class GateError(Exception):
    """The manifest, or the gate's own safety, is wrong."""


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def guarded_digests(root=REPO):
    """SHA-256 of every file under `GUARDED`, keyed by repo-relative path."""
    out = {}
    for rel in GUARDED:
        base = Path(root) / rel
        for p in sorted(base.rglob("*")):
            if p.is_file():
                out[str(p.relative_to(root))] = digest(p)
    return out


def _under(root, rel):
    """`root / rel`, or raise if that is not strictly inside `root`.

    The single containment check.  Rejects an absolute path and a `..`
    component lexically, then re-checks the RESOLVED path, so a symlink cannot
    smuggle a write out of the shadow tree either.
    """
    root = Path(root).resolve()
    parts = Path(rel).parts
    if Path(rel).is_absolute() or ".." in parts or not parts:
        raise GateError("%r is not a safe repo-relative path" % (rel,))
    p = root / rel
    resolved = p.resolve()
    if root not in resolved.parents:
        raise GateError("%s resolves to %s, outside %s" % (rel, resolved, root))
    return p


def _read_real(rel):
    """Read one real repo file.  The ONLY access to the repo, and read-only."""
    return _under(REPO, rel).read_bytes()


def build_shadow(dst):
    """Copy the tree a case runs against into `dst`."""
    dst = Path(dst).resolve()
    if dst == REPO or REPO in dst.parents or dst in REPO.parents:
        raise GateError("shadow tree %s overlaps the repo at %s" % (dst, REPO))
    ignore = shutil.ignore_patterns("__pycache__", "*.pyc", "target", "build")
    for rel in SHADOW_TREE:
        src = REPO / rel
        if src.is_dir():
            # `copyfile`, not the default `copy2`: contents only, no mode.  A
            # source file the owner has made read-only -- which is one way to
            # ensure this tool cannot write to it -- must still produce a
            # WRITABLE copy, or the gate cannot perturb its own shadow.
            shutil.copytree(src, dst / rel, ignore=ignore, symlinks=False,
                            copy_function=shutil.copyfile)
        else:
            shutil.copyfile(src, dst / rel)
    return dst


# ---------------------------------------------------------------- mutations

def _walk(doc, path, case):
    """Follow all but the last step of `path`, returning (container, key)."""
    cur = doc
    for step in path[:-1]:
        try:
            cur = cur[step]
        except (KeyError, IndexError, TypeError) as e:
            raise GateError("%s: path %r does not exist (%s)"
                            % (case, path, e))
    return cur, path[-1]


def apply_mutation(mut, target, case):
    """Perturb the file at `target` in place.  Three ops, no more."""
    op = mut.get("op")
    if op == "json-set":
        doc = json.loads(target.read_text())
        holder, key = _walk(doc, mut["path"], case)
        try:
            was = holder[key]
        except (KeyError, IndexError, TypeError) as e:
            raise GateError("%s: path %r does not exist (%s)"
                            % (case, mut["path"], e))
        if was != mut["from"]:
            raise GateError("%s: %r holds %r, the manifest expected %r -- the "
                            "case has drifted from the artifact"
                            % (case, mut["path"], was, mut["from"]))
        if was == mut["to"]:
            raise GateError("%s: the perturbation does not change anything"
                            % case)
        holder[key] = mut["to"]
        target.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")
        return "%s: %r -> %r" % (_pathstr(mut["path"]), was, mut["to"])
    if op == "json-append":
        doc = json.loads(target.read_text())
        holder, key = _walk(doc, mut["path"], case)
        seq = holder[key]
        if not isinstance(seq, list):
            raise GateError("%s: %r is not a list" % (case, mut["path"]))
        seq.append(mut["value"])
        target.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")
        return "%s: append %r (%d -> %d)" % (_pathstr(mut["path"]),
                                             mut["value"], len(seq) - 1,
                                             len(seq))
    if op == "text-replace":
        text = target.read_text()
        want = mut.get("count", 1)
        got = text.count(mut["from"])
        if got != want:
            raise GateError("%s: %d occurrence(s) of the text to replace, the "
                            "manifest expected %d" % (case, got, want))
        target.write_text(text.replace(mut["from"], mut["to"]))
        return "text: %d line(s) replaced" % (mut["from"].count("\n") + 1)
    raise GateError("%s: unknown op %r" % (case, op))


def _pathstr(path):
    out = ""
    for step in path:
        out += "[%d]" % step if isinstance(step, int) else (
            "." + step if out else step)
    return out


# ------------------------------------------------------------------ runner

def _run(cmd, cwd):
    p = subprocess.run(cmd, cwd=str(cwd), capture_output=True, text=True,
                       env=_env())
    return p.returncode, p.stdout + p.stderr


def _env():
    e = dict(os.environ)
    e["CARGO_TERM_COLOR"] = "never"
    e.pop("CARGO_TARGET_DIR", None)   # the shadow tree gets its own
    return e


def run_case(case, shadow, out=sys.stdout):
    """Green, then red, then restored.  Returns (ok, one-line summary)."""
    label = case["label"]
    rel = case["artifact"]
    target = _under(shadow, rel)
    pristine = _read_real(rel)

    # Start from the real bytes whatever an earlier case left behind.
    target.write_bytes(pristine)

    rc, log = _run(case["test"], shadow)
    if rc != 0:
        return False, ("the test is ALREADY red on the unmutated copy, so its "
                       "going red proves nothing:\n" + _tail(log))

    what = apply_mutation(case["mutate"], target, label)
    if target.read_bytes() == pristine:
        raise GateError("%s: the perturbation left the bytes unchanged" % label)

    rc, log = _run(case["test"], shadow)
    if case.get("expect_red", True):
        ok, why = True, "green -> red   %s" % what
        if rc == 0:
            ok, why = False, ("the test still PASSED with %s mutated (%s) -- "
                              "this assertion cannot fail" % (rel, what))
        elif case["expect"] not in log:
            ok, why = False, ("the test went red but its output never says "
                              "%r, so it did not fail on the claim this case "
                              "defends:\n%s" % (case["expect"], _tail(log)))
    else:
        # A recorded finding: this column is asserted by nothing.  Kept in the
        # manifest rather than dropped, and kept EXECUTABLE rather than as
        # prose, so it rots loudly -- the day an assertion reaches it, the run
        # fails here and the case moves to the red list.
        ok, why = True, "green -> green %s   (asserted by nothing)" % what
        if rc != 0:
            ok, why = False, ("recorded as asserted by nothing, but the test "
                              "NOW goes red -- the coverage improved: move "
                              "this case to expect_red with the message it "
                              "prints:\n%s" % _tail(log))

    # Restoration is verified against a FRESH read of the real artifact, not
    # against the copy in hand: the shadow tree has to be back to what the
    # repo holds now, and the repo has to still hold it.
    target.write_bytes(pristine)
    if target.read_bytes() != _read_real(rel):
        raise GateError("%s: %s was not restored after the case" % (label, rel))
    return ok, why


def _tail(log, n=14):
    lines = [ln for ln in log.splitlines() if ln.strip()]
    return "\n".join("          | " + ln for ln in lines[-n:])


def run_manifest(manifest=MANIFEST, only=None, out=sys.stdout):
    """Run every case (or one).  Returns a process exit status."""
    spec = json.loads(Path(manifest).read_text())
    cases = spec["cases"]
    labels = [c["label"] for c in cases]
    if len(set(labels)) != len(labels):
        raise GateError("duplicate case labels in %s" % manifest)
    if only is not None:
        cases = [c for c in cases if c["label"] == only]
        if not cases:
            raise GateError("no case %r in %s (have: %s)"
                            % (only, manifest, ", ".join(labels)))
    if not cases:
        raise GateError("%s registers no cases, so this gate checks nothing"
                        % manifest)

    before = guarded_digests()
    failed = []
    shadow = Path(tempfile.mkdtemp(prefix="gopnik-mutate-"))
    try:
        build_shadow(shadow)
        print("mutation gate: %d case(s), shadow tree %s"
              % (len(cases), shadow), file=out)
        for i, case in enumerate(cases, 1):
            ok, why = run_case(case, shadow, out)
            print("  [%2d/%d] %-6s %-34s %s"
                  % (i, len(cases), "ok" if ok else "FAIL",
                     case["label"], why), file=out)
            if not ok:
                failed.append(case["label"])
    finally:
        shutil.rmtree(shadow, ignore_errors=True)
        after = guarded_digests()

    print("", file=out)
    for name in FROZEN:
        print("  %-24s %s" % (name, after[name]), file=out)
    changed = {k for k in set(before) | set(after)
               if before.get(k) != after.get(k)}
    if changed:
        print("\nSAFETY FAILURE: the gate changed %d real file(s): %s"
              % (len(changed), ", ".join(sorted(changed))), file=out)
        return 2
    print("  %d real file(s) under %s unchanged"
          % (len(after), "/, ".join(GUARDED) + "/"), file=out)

    if failed:
        print("\nGATE FAILURE: %d of %d case(s) did not go red: %s"
              % (len(failed), len(cases), ", ".join(failed)), file=out)
        return 1
    red = [c for c in cases if c.get("expect_red", True)]
    print("\nall %d case(s) went red: every channel above is falsifiable"
          % len(red), file=out)
    if len(red) != len(cases):
        print("  %d further column(s) confirmed still asserted by nothing -- "
              "findings, not coverage" % (len(cases) - len(red)), file=out)
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(
        description=__doc__.splitlines()[0],
        formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--manifest", default=str(MANIFEST))
    ap.add_argument("--case", default=None, help="run one case by label")
    args = ap.parse_args(argv)
    try:
        return run_manifest(args.manifest, args.case)
    except GateError as e:
        print("mutation gate: %s" % e, file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
