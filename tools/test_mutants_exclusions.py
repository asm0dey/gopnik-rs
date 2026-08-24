#!/usr/bin/env python3
"""Guard: every `exclude_re` in `.cargo/mutants.toml` still matches a live mutant.

`cargo mutants` exits 0 when an `exclude_re` matches nothing at all -- an
exclusion whose line number has drifted (the code moved) silently stops
excluding anything, and a clean "0 missed" report then means "nothing looked
here", not "verified equivalent".  `docs/re/METHODOLOGY.md`'s "an assertion is
not evidence until it has been seen failing" rule applies to this exclusion
list exactly as it does to any other captured-oracle assertion: before
trusting the fail-open property, this independently confirms every pattern
still lines up with a mutant `cargo mutants` would actually generate today.

Skips (does not fail) when `cargo mutants` is not installed -- it is a
platform tool this project does not vendor (unlike the frozen oracles under
`data/`), so its absence is not evidence about the exclusion list one way or
the other.

Run:  python3 -m unittest tools.test_mutants_exclusions -v
"""
import re
import shutil
import subprocess
import sys
import tomllib
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
MUTANTS_TOML = REPO / ".cargo" / "mutants.toml"


def _exclude_patterns():
    """The `exclude_re` list, read with a real TOML parser."""
    doc = tomllib.loads(MUTANTS_TOML.read_text())
    return doc.get("exclude_re", [])


def _file_of(pattern):
    """The literal repo-relative path a `^src/foo\\.rs:line:col: ...` pattern
    is anchored on.

    Every pattern here starts with `^`, then the source path with regex
    metacharacters escaped (just `.` -> `\\.` in practice), then
    `:line:col: `.  Splitting on the first unescaped `:` and undoing the `.`
    escape recovers the literal path `cargo mutants -f` expects.
    """
    m = re.match(r"\^((?:[^:\\]|\\.)+):", pattern)
    if not m:
        raise AssertionError(
            "exclude_re %r has no recognisable ^<file>: prefix" % (pattern,))
    return m.group(1).replace("\\.", ".")


def _cargo_mutants_available():
    if shutil.which("cargo") is None:
        return False
    try:
        proc = subprocess.run(["cargo", "mutants", "--version"],
                               cwd=REPO, capture_output=True, text=True,
                               timeout=30)
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False
    return proc.returncode == 0


class ExclusionsMatchALiveMutantTest(unittest.TestCase):
    """Each `exclude_re` must still name a mutant `cargo mutants` would emit.

    One `--list` call covers every excluded file at once; matching each
    pattern against that combined listing is enough to catch a line number
    that moved out from under its exclusion.
    """

    @classmethod
    def setUpClass(cls):
        if not _cargo_mutants_available():
            raise unittest.SkipTest(
                "cargo-mutants is not installed in this environment -- "
                "see docs/re/METHODOLOGY.md's fail-open note on "
                ".cargo/mutants.toml")
        cls.patterns = _exclude_patterns()
        if not cls.patterns:
            raise unittest.SkipTest("no exclude_re entries in "
                                     "%s to check" % MUTANTS_TOML)
        files = sorted({_file_of(p) for p in cls.patterns})
        # `--no-config`: `cargo mutants` reads `.cargo/mutants.toml`
        # automatically, including its `exclude_re`, and `--list` shows the
        # mutants left AFTER that filtering -- so a still-valid exclusion
        # hides its own target from a plain `--list` and this test would
        # report every good exclusion as stale.  `--no-config` lists the raw
        # mutant set the exclusions are meant to be checked against.
        args = ["cargo", "mutants", "--no-config", "--list"]
        for f in files:
            args += ["-f", f]
        proc = subprocess.run(args, cwd=REPO, capture_output=True,
                               text=True, timeout=120)
        if proc.returncode != 0:
            raise AssertionError(
                "%s failed (exit %d):\n%s"
                % (" ".join(args), proc.returncode, proc.stderr))
        cls.mutant_names = [ln for ln in proc.stdout.splitlines() if ln.strip()]
        if not cls.mutant_names:
            raise AssertionError(
                "%s listed zero mutants across %s -- cannot check any "
                "exclusion against an empty listing" % (" ".join(args), files))

    def test_every_exclusion_matches_a_listed_mutant(self):
        for pattern in self.patterns:
            rx = re.compile(pattern)
            hit = [n for n in self.mutant_names if rx.match(n)]
            self.assertTrue(
                hit,
                "exclude_re %r matches none of the %d mutants cargo mutants "
                "lists for its file -- the exclusion is stale (the code it "
                "names has moved or is gone)" % (pattern, len(self.mutant_names)))


if __name__ == "__main__":
    unittest.main()
