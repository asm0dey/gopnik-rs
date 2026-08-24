"""Marker only: makes `tools` a package so `tools.rngtrace...` is a valid
import path.

mutmut keys every mutant by the source file's path from the repo root, so the
tests must import the mutated modules under that same path or every trampoline
hit misses.  See the note at the top of `tools/test_rngtrace.py`.

The flat modules here (`addr`, `difftest`, `dis16`, ...) are still imported as
top-level names off a `sys.path` entry pointing at this directory; this file
does not change that and adds nothing to the namespace.
"""
