"""The original's LCG, as a host-side predictor.

Recovered statically in docs/re/rng.md from orig/g.exe's own instruction
bytes (`@Rand` at 1f78:11a8, `Random(Word)` at 1f78:114b):

    RandSeed := (RandSeed * 0x08088405 + 1) mod 2^32       -- @Rand
    Random(n) := (RandSeed * n) >> 32                      -- high take, NOT a modulo

`Random` steps the seed FIRST and maps the NEW state, so one call consumes
exactly one state transition.

This module is deliberately independent of src/rng.rs (the port); it is
checked against data/rng_vectors.json in tools/test_rngtrace.py, which is
itself produced by an 8086 interpreter over orig/g.exe.
"""

MUL = 0x08088405
INC = 1
MASK32 = 0xFFFFFFFF


def step(seed: int) -> int:
    """One @Rand transition: returns the new state."""
    return (seed * MUL + INC) & MASK32


def random_of(seed_after_step: int, n: int) -> int:
    """Random(n) given the ALREADY-stepped state."""
    return ((seed_after_step * n) >> 32) & 0xFFFF


def draw(seed: int, n: int):
    """One Random(n) call: returns (new_seed, result)."""
    s = step(seed)
    return s, random_of(s, n)


def predict(seed: int, ns):
    """Replay a sequence of Random(n) calls. Returns list of results."""
    out = []
    s = seed
    for n in ns:
        s, r = draw(s, n)
        out.append(r)
    return out
