"""Read back what `tools/ghidra/ExportAll.java` wrote, and what `docs/` cites.

Two independent checkers live here, joined only by the theme
`docs/re/METHODOLOGY.md` names: *a check that cannot fail, presented as
verification*.

1. `read_annotations()` / `alignment_report()` -- the per-line address annotation in
   `build/decomp/*.c`.  Every address it emits is re-decoded from `orig/g.exe`
   and required to be an ALIGNED INSTRUCTION START, anchored at a function
   entry from the Ghidra export.  "Well-formed `SEG:OFF`" is not the check;
   `1000:d83b` is well-formed and is the standing counter-example in
   `tools/re_query.py`.

2. `src_citations()` -- the `` `src/f.rs:NNN` `symbol` `` pairs the docs write.
   `src/` is not frozen, so those numbers are stale-by-construction; this asks
   the tree whether each one still points at what the doc says it does.

Neither checker owns a pass threshold.  A threshold picked so it passes today
is the same defect one level up, so both return the whole classification and
`tools/test_decomp_addresses.py` pins it against a golden file.
"""

from __future__ import annotations

import bisect
import json
import re
from pathlib import Path

import addr
import dis16

REPO = Path(__file__).resolve().parent.parent
DECOMP_DIR = REPO / "build" / "decomp"

# A COMMITTED slice of the same export.  `build/decomp/` is gitignored, so on a
# fresh clone every assertion over it skips and the suite still reports OK --
# a green run that validated nothing.  These few files are tracked so at least
# the alignment half always executes, and a separate test (which does skip)
# requires each of them to be byte-identical to its `build/decomp/`
# counterpart wherever that tree exists, so the fixture cannot silently rot.
FIXTURE_DIR = REPO / "tools" / "fixtures" / "decomp"

DOCS_DIR = REPO / "docs"
BRANCHES = REPO / "data" / "branches.json"
FUNCTIONS = REPO / "data" / "functions.json"

# The marker ExportAll.java writes.  Kept in sync with its ANNOT constant --
# and the exporter's own file header deliberately does NOT spell this string
# out, because the first draft's header did and this parser then read nine
# English words per file as malformed addresses.
ANNOTATION = re.compile(r"//\s@\s+(\S.*?)\s*$")
_ADDRESS = re.compile(r"\A[0-9a-fA-F]{1,4}:[0-9a-fA-F]{1,4}\Z")


# --- half one: the decompilation annotation ----------------------------------


class Annotation:
    """One `SEG:OFF` token as it was found, with where it was found."""

    __slots__ = ("file", "line", "text")

    def __init__(self, file, line, text):
        self.file = file
        self.line = line
        self.text = text

    def __repr__(self):  # pragma: no cover - debugging aid
        return "Annotation(%s:%d %s)" % (self.file, self.line, self.text)


def parse_file(path: Path):
    """`(tokens, malformed)` for one annotated `.c` file.

    A token that does not look like `SEG:OFF` is RETURNED, never dropped: the
    parser reporting what it could not understand is the whole point.  Ghidra
    prints nothing but segmented addresses here, so a malformed token means
    either the exporter changed or something else is writing the marker.
    """
    tokens = []
    malformed = []
    for lineno, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
        m = ANNOTATION.search(line)
        if not m:
            continue
        for tok in m.group(1).split():
            if _ADDRESS.match(tok):
                tokens.append(Annotation(path.name, lineno, tok.lower()))
            else:
                malformed.append(Annotation(path.name, lineno, tok))
    return tokens, malformed


_ANNOTATION_CACHE = {}


def read_annotations(decomp_dir: Path = DECOMP_DIR):
    """`{filename: [Annotation, ...]}` plus every malformed token, over a tree.

    NOT named `annotations`: that shadows the `from __future__ import
    annotations` binding at the top of this module, which is harmless at
    runtime and is flagged by pyright.

    Memoised on the directory AND the modification time of its newest `.c`
    file, so a re-export (or a deliberate corruption, which is how this suite
    is shown to fail) invalidates the cache instead of being read from it.
    """
    key = Path(decomp_dir).resolve()
    paths = sorted(key.glob("*.c"))
    stamp = tuple((p.name, p.stat().st_mtime_ns, p.stat().st_size) for p in paths)
    hit = _ANNOTATION_CACHE.get(key)
    if hit is not None and hit[0] == stamp:
        return hit[1]
    per_file = {}
    malformed = []
    for path in paths:
        toks, bad = parse_file(path)
        per_file[path.name] = toks
        malformed.extend(bad)
    _ANNOTATION_CACHE[key] = (stamp, (per_file, malformed))
    return per_file, malformed


class Image:
    """`orig/g.exe`'s load image, plus the function entries to anchor from.

    An instruction start is only meaningful relative to where decoding began.
    Anchoring at a function entry that Ghidra EXPORTED is an anchor that was
    not guessed -- the distinction `tools/re_query.py.anchored_stream` draws
    between a `function-entry` anchor and a `back-sweep` consensus.  This class
    only ever uses the former; an address with no entry at or before it fails
    rather than falling back to a guess.
    """

    def __init__(self, exe_path=None, functions_path=FUNCTIONS):
        self.image = addr.load_image(addr.read_exe(exe_path))
        records = json.loads(Path(functions_path).read_text())
        entries = sorted(
            (addr.image_off_of_citation(f["entry"]), f["name"]) for f in records
        )
        self._offs = [e[0] for e in entries]
        self._names = [e[1] for e in entries]
        self._runs = {}

    def anchor_for(self, image_off):
        i = bisect.bisect_right(self._offs, image_off) - 1
        if i < 0:
            return None, None
        return self._offs[i], self._names[i]

    def instruction_starts_through(self, anchor, image_off):
        """Offsets of every instruction in the linear decode `anchor`..`off`.

        Cached per anchor at the furthest offset asked for so far, so the
        13,000-line sweep does not re-decode `entry`'s 17 KiB body per address.
        """
        cached = self._runs.get(anchor)
        if cached is not None and cached[0] >= image_off:
            return cached[1]
        try:
            insns = dis16.decode_run(self.image, anchor, image_off + 1)
        except dis16.DecodeError:
            return None
        starts = {i.off for i in insns}
        if cached is not None:
            starts |= cached[1]
        self._runs[anchor] = (image_off, starts)
        return starts

    def classify(self, citation: str):
        """`(ok, reason)` for one `SEG:OFF` citation.

        `reason` is None when it is an aligned instruction start, and otherwise
        says which of the three ways it failed -- outside the image, no anchor,
        or decoded-but-not-a-boundary.
        """
        try:
            image_off = addr.image_off_of_citation(citation)
        except addr.AddressError as e:
            return False, "not a resolvable citation: %s" % e
        if image_off >= len(self.image):
            return False, (
                "image offset 0x%x is past the end of the load image (0x%x)"
                % (image_off, len(self.image))
            )
        anchor, name = self.anchor_for(image_off)
        if anchor is None:
            return False, "no exported function entry at or before it to anchor on"
        starts = self.instruction_starts_through(anchor, image_off)
        if starts is None:
            return False, "linear decode anchored at %s failed before reaching it" % name
        if image_off not in starts:
            return False, (
                "not an instruction start in the decode anchored at %s" % name
            )
        return True, None


def alignment_report(decomp_dir: Path = DECOMP_DIR, image: "Image | None" = None):
    """Every distinct annotated address, and which ones are not real starts."""
    per_file, malformed = read_annotations(decomp_dir)
    image = image or Image()
    distinct = set()
    for toks in per_file.values():
        for t in toks:
            distinct.add(t.text)
    failures = {}
    for cit in sorted(distinct):
        ok, why = image.classify(cit)
        if not ok:
            failures[cit] = why
    sites = {}
    for name, toks in per_file.items():
        for t in toks:
            if t.text in failures:
                sites.setdefault(t.text, []).append("%s:%d" % (name, t.line))
    return {
        "files": len(per_file),
        "annotated_tokens": sum(len(v) for v in per_file.values()),
        "distinct_addresses": len(distinct),
        "malformed": ["%s:%d %r" % (m.file, m.line, m.text) for m in malformed],
        "not_instruction_starts": failures,
        "not_instruction_start_sites": {k: sorted(v) for k, v in sites.items()},
    }


# --- half one, continued: the six hand-mapped handlers -----------------------

# Ranges whose branches were walked and cited BY HAND, in docs/re/.  They are
# the ground truth this annotation is checked against: every branch address and
# every guard address `data/branches.json` records inside one of them is a fact
# established independently of Ghidra's decompiler, so the annotation either
# reproduces it or has a hole worth naming.
HANDLER_RANGES = {
    "market": ("1000:b94a", "1000:c4bd"),
    "sell": ("1000:ce76", "1000:d383"),
    "vet": ("1000:d3a6", "1000:d6ec"),
    "den": ("1000:d802", "1000:df05"),
    "club": ("1000:df06", "1000:e38f"),
    "gym": ("1000:e390", "1000:e972"),
}

# All six live inside the one exported function that covers 1000:ab59..1000:ee4f.
HANDLER_FILE = "entry_1000_ab59.c"


def handler_coverage(decomp_dir: Path = DECOMP_DIR, branches_path=BRANCHES):
    """Per-handler branch/guard address coverage of the annotation.

    A `guard` is the flag-setting instruction `EnumerateBranches.java` paired
    with the conditional jump; both are addresses a doc cites, so both count.
    """
    per_file, _ = read_annotations(decomp_dir)
    have = {t.text for t in per_file.get(HANDLER_FILE, [])}
    records = json.loads(Path(branches_path).read_text())["branches"]
    out = {}
    for name, (lo, hi) in sorted(HANDLER_RANGES.items()):
        lo_off = addr.image_off_of_citation(lo)
        hi_off = addr.image_off_of_citation(hi)
        wanted = set()
        pairs = []
        for b in records:
            a = addr.image_off_of_citation(b["addr"])
            if not (lo_off <= a <= hi_off):
                continue
            branch = b["addr"].lower()
            wanted.add(branch)
            guard = (b.get("guard") or {}).get("addr")
            guard = guard.lower() if guard else None
            if guard:
                wanted.add(guard)
            pairs.append((branch, guard))
        missing = sorted(wanted - have)
        out[name] = {
            "wanted": len(wanted),
            "present": len(wanted) - len(missing),
            "missing": missing,
            "pairs": pairs,
        }
    return out


def orphaned_pairs(coverage, decomp_dir: Path = DECOMP_DIR):
    """Branch/guard pairs where NEITHER address is annotated.

    The WEAK form of the fold check, kept because the golden records it and
    because it names the failure in the pair's own vocabulary.  It is subsumed
    by `fold_partners` below, which fires on a pair that loses ONE half for a
    reason other than the fold; this one fires only when a pair loses both.

    `decomp_dir` is a parameter rather than the module default because
    `coverage` may have been computed over a different tree, and comparing one
    tree's coverage against another tree's annotation is a silent cross-tree
    read waiting for the first caller that parameterises the directory.
    """
    per_file, _ = read_annotations(decomp_dir)
    have = {t.text for t in per_file.get(HANDLER_FILE, [])}
    orphans = []
    for name, rec in sorted(coverage.items()):
        for branch, guard in rec["pairs"]:
            if branch in have:
                continue
            if guard is not None and guard in have:
                continue
            orphans.append("%s %s (guard %s)" % (name, branch, guard))
    return sorted(orphans)


def fold_partners(coverage, decomp_dir: Path = DECOMP_DIR, branches_path=BRANCHES):
    """For every UNANNOTATED handler address, its `data/branches.json` partner.

    The partner of a branch address is its guard's address, and the partner of
    a guard address is every branch that guard resolves.  This is the property
    that makes "the decompiler folded the pair onto one address" a claim that
    can be wrong: a miss caused by the fold necessarily leaves its partner in
    the annotation, and a miss caused by anything else does not have to.

    It replaces a proximity test that could not discriminate.  That test asked
    whether a missing address had ANY annotated address within 10 bytes; over
    `1000:b94a`..`1000:e972` the largest gap between consecutive annotated
    offsets is 11, so every one of the 12328 byte offsets in the span passed it,
    not just the 29 misses.  It would have fired only on a contiguous
    unannotated run of 21 bytes or more -- a different failure from the one it
    was presented as testing, and a threshold picked so it passes.

    Returns `{missing_address: {"partners": [...], "unannotated_partners": [...],
    "max_delta": int}}`.  `unannotated_partners` non-empty is the finding.
    """
    per_file, _ = read_annotations(decomp_dir)
    have = {t.text for t in per_file.get(HANDLER_FILE, [])}
    records = json.loads(Path(branches_path).read_text())["branches"]

    partners = {}
    for b in records:
        branch = b["addr"].lower()
        guard = (b.get("guard") or {}).get("addr")
        if not guard:
            continue
        guard = guard.lower()
        partners.setdefault(branch, set()).add(guard)
        partners.setdefault(guard, set()).add(branch)

    out = {}
    for rec in coverage.values():
        for miss in rec["missing"]:
            mine = sorted(partners.get(miss, ()))
            off = addr.image_off_of_citation(miss)
            out[miss] = {
                "partners": mine,
                "unannotated_partners": [p for p in mine if p not in have],
                "max_delta": max(
                    (abs(addr.image_off_of_citation(p) - off) for p in mine),
                    default=None),
            }
    return out


# --- half two: `src/f.rs:NNN` citations in docs/ -----------------------------

# An inline-code span.  Markdown's ``` `` `` escape does not occur in these
# docs, and a fenced block is handled by the fence tracker in `_spans`.
_CODE_SPAN = re.compile(r"`([^`\n]+)`")

# A `path:lines` citation of ANY file, not just `src/`.  The wider pattern is
# load-bearing: a bare `` `:941` `` continuation binds to the LAST file cited,
# and `docs/re/difftest.md:500` writes `` `tests/wander_sequence.rs:484` ``
# then `` `:941` ``.  A src-only tracker mis-bound that to a `src/` path three
# paragraphs earlier and reported a line number the file does not have.
_FILE_CITE = re.compile(
    r"\A([A-Za-z0-9_][A-Za-z0-9_/.-]*\.(?:rs|py|java|sh|json|md|toml)):"
    r"([0-9]+(?:[-,][0-9]+)*)\Z")
_CONT_CITE = re.compile(r"\A:([0-9]+(?:[-,][0-9]+)*)\Z")

# Binding rule, stated so it can be argued with rather than tuned.
#
# FOLLOWING form -- `` `src/f.rs:NNN` `symbol` `` -- is the shape the docs use
# when the citation introduces the symbol, and it binds only with NOTHING
# between the two spans.  Any prose between them means the next span is the
# subject of a new clause, not this citation's symbol.
#
# PRECEDING form -- `` `symbol` in `src/f.rs:NNN` `` -- binds only across one
# of these connectives.  The set is closed and short on purpose: widening it to
# "any short gap" bound `src/save.rs:318` to `Fighter::level` across ", read
# straight into", which is a different clause.
_PRECEDING_CONNECTIVES = frozenset(
    ["", "(", "in", "in (", "at", "at (", "is", "is (", "=", "--"])


def _spans(text):
    """`(offset, line, inner)` for every inline-code span outside fenced code."""
    out = []
    off = 0
    in_fence = False
    for lineno, line in enumerate(text.split("\n"), 1):
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            in_fence = not in_fence
        elif not in_fence:
            for m in _CODE_SPAN.finditer(line):
                out.append((off + m.start(), lineno, m.group(1)))
        off += len(line) + 1
    return out, text


def _is_cite(inner):
    return bool(_FILE_CITE.match(inner) or _CONT_CITE.match(inner))


def _linespec_rows(linespec):
    """`173-175` -> [173, 174, 175]; `202,210` -> [202, 210]; `61` -> [61].

    The two separators mean different things in these docs and conflating them
    is a silent widening: reading `src/trace.rs:202,210` as the range 202..210
    makes the check pass on any of nine lines instead of the two cited.
    """
    rows = []
    for part in linespec.split(","):
        if "-" in part:
            lo, hi = part.split("-", 1)
            rows.extend(range(int(lo), int(hi) + 1))
        else:
            rows.append(int(part))
    return rows


def src_citations(docs_dir: Path = DOCS_DIR, repo: Path = REPO):
    """Classify every `src/<file>.rs:<line>` citation the docs make.

    Returns a dict with four disjoint lists, which together are every citation
    found -- `verified`, `mismatch`, `unbound` and `unreadable`.  Nothing is
    filtered out on the way: an inventory whose completeness claim stopped the
    next search is the defect this project keeps finding, so the parser reports
    what it could not bind (`unbound`) and what it could not resolve at all
    (`unreadable`) rather than shrinking the denominator.
    """
    verified, mismatch, unbound, unreadable = [], [], [], []
    src_cache = {}

    for doc in sorted(Path(docs_dir).rglob("*.md")):
        spans, text = _spans(doc.read_text(errors="replace"))
        last_path = None
        for i, (start, lineno, inner) in enumerate(spans):
            m = _FILE_CITE.match(inner)
            cont = None if m else _CONT_CITE.match(inner)
            if m:
                last_path = m.group(1)
                path, linespec = last_path, m.group(2)
            elif cont and last_path is not None:
                path, linespec = last_path, cont.group(1)
            else:
                continue
            if not path.startswith("src/"):
                continue
            wanted = _linespec_rows(linespec)
            # The DOC's own line number is deliberately NOT part of the record.
            # It is stale-by-construction in exactly the way this checker exists
            # to catch, so putting it in the golden would redden the suite every
            # time a paragraph above a citation gained a line -- churn that
            # trains the reader to regenerate without looking. The backticked
            # citation text is in the record and `grep -n` finds it.
            where = str(doc.relative_to(repo))
            cite = "`%s` -> %s:%s" % (inner, path, linespec)

            symbol = None
            end = start + len(inner) + 2
            if i + 1 < len(spans):
                nxt = spans[i + 1]
                if (not text[end:nxt[0]].strip() and not _is_cite(nxt[2])):
                    symbol = nxt[2]
            if symbol is None and i > 0:
                prv = spans[i - 1]
                gap = text[prv[0] + len(prv[2]) + 2:start].strip()
                if (gap in _PRECEDING_CONNECTIVES and not _is_cite(prv[2])):
                    symbol = prv[2]

            src_file = repo / path
            if src_file not in src_cache:
                src_cache[src_file] = (
                    src_file.read_text(errors="replace").splitlines()
                    if src_file.exists() else None
                )
            lines = src_cache[src_file]
            if lines is None:
                unreadable.append("%s %s -- no such file" % (where, cite))
                continue
            over = [n for n in wanted if n < 1 or n > len(lines)]
            if over:
                unreadable.append(
                    "%s %s -- file has %d lines" % (where, cite, len(lines)))
                continue
            if symbol is None:
                unbound.append("%s %s -- no symbol span adjacent to it" % (where, cite))
                continue

            body = "\n".join(lines[n - 1] for n in wanted)
            needles = [symbol]
            if "::" in symbol:
                needles.append(symbol.rsplit("::", 1)[1])
            if "(" in symbol:
                needles.append(symbol.split("(", 1)[0])
            hit = any(n and n in body for n in needles)
            record = "%s %s `%s`" % (where, cite, symbol)
            (verified if hit else mismatch).append(record)

    return {
        "verified": sorted(verified),
        "mismatch": sorted(mismatch),
        "unbound": sorted(unbound),
        "unreadable": sorted(unreadable),
    }


GOLDEN = REPO / "tools" / "decomp_addresses_golden.json"


def multi_address_lines(decomp_dir: Path = DECOMP_DIR):
    """How many annotated lines carry more than one address.

    This is the number that distinguishes a SET from a minimum.  If a later
    edit of ExportAll.java quietly reverted to one address per line -- the
    thing docs/re/METHODOLOGY.md calls a check that cannot fail -- this drops
    to zero while every other assertion here stays green.
    """
    per_line = {}
    for path in sorted(Path(decomp_dir).glob("*.c")):
        toks, _ = parse_file(path)
        for t in toks:
            per_line.setdefault((path.name, t.line), set()).add(t.text)
    return sum(1 for v in per_line.values() if len(v) > 1)


def build_golden():
    """The whole classification, in the shape tools/test_decomp_addresses.py pins."""
    align = alignment_report()
    cov = handler_coverage()
    return {
        "note": (
            "Golden set for tools/test_decomp_addresses.py. Regenerate with "
            "`python3 tools/decomp_addresses.py --write-golden` and READ THE DIFF: "
            "every number here is a fact about the shipped build/decomp/ tree, "
            "orig/g.exe and docs/, and a change in any of them is a finding, not "
            "noise. There is deliberately no percentage threshold -- a threshold "
            "chosen so it passes is the defect this file exists to prevent."
        ),
        "annotation": {
            "files": align["files"],
            "annotated_tokens": align["annotated_tokens"],
            "distinct_addresses": align["distinct_addresses"],
            "lines_carrying_more_than_one_address": multi_address_lines(),
            "malformed": align["malformed"],
            "not_instruction_starts": align["not_instruction_starts"],
            "not_instruction_start_sites": align["not_instruction_start_sites"],
        },
        "handlers": {
            name: {"wanted": c["wanted"], "present": c["present"],
                   "missing": c["missing"]}
            for name, c in sorted(cov.items())
        },
        "handler_total": {
            "wanted": sum(c["wanted"] for c in cov.values()),
            "present": sum(c["present"] for c in cov.values()),
        },
        "branch_guard_pairs_with_neither_half_annotated": orphaned_pairs(cov),
        "fold_partners": fold_partners(cov),
        "fixture": {
            "files": sorted(p.name for p in FIXTURE_DIR.glob("*.c")),
            "annotated_tokens": sum(
                len(v) for v in read_annotations(FIXTURE_DIR)[0].values()),
            "not_instruction_starts":
                alignment_report(FIXTURE_DIR)["not_instruction_starts"],
        },
        "src_citations": src_citations(),
    }


def _main(argv=None):  # pragma: no cover - a reporting entry point
    import sys

    what = (argv or sys.argv[1:] or ["all"])[0]
    if what == "--write-golden":
        GOLDEN.write_text(json.dumps(build_golden(), indent=2, sort_keys=True) + "\n")
        print("wrote %s" % GOLDEN)
        return 0
    if what in ("all", "align"):
        rep = alignment_report()
        print("annotation: files=%(files)d tokens=%(annotated_tokens)d "
              "distinct=%(distinct_addresses)d" % rep)
        print("  malformed tokens: %d" % len(rep["malformed"]))
        for m in rep["malformed"]:
            print("    %s" % m)
        print("  not aligned instruction starts: %d" % len(rep["not_instruction_starts"]))
        for k, v in sorted(rep["not_instruction_starts"].items()):
            print("    %s -- %s" % (k, v))
    if what in ("all", "handlers"):
        cov = handler_coverage()
        tot = sum(c["wanted"] for c in cov.values())
        got = sum(c["present"] for c in cov.values())
        for name, c in sorted(cov.items()):
            print("  %-7s %3d/%3d" % (name, c["present"], c["wanted"]))
        print("  TOTAL %d/%d = %.2f%%" % (got, tot, 100.0 * got / tot))
        fp = fold_partners(cov)
        for name, c in sorted(cov.items()):
            for a in c["missing"]:
                r = fp[a]
                print("    missing %-7s %s  partner(s) %s%s"
                      % (name, a, " ".join(r["partners"]) or "(NONE)",
                         "  UNANNOTATED: " + " ".join(r["unannotated_partners"])
                         if r["unannotated_partners"] else ""))
        print("  pairs with NEITHER half annotated: %d" % len(orphaned_pairs(cov)))
        print("  misses whose branches.json partner is NOT annotated: %d"
              % sum(1 for r in fp.values() if r["unannotated_partners"] or not r["partners"]))
    if what in ("all", "src"):
        rep = src_citations()
        for k in ("verified", "mismatch", "unbound", "unreadable"):
            print("src citations %s: %d" % (k, len(rep[k])))
            if k != "verified":
                for r in rep[k]:
                    print("    %s" % r)
    return 0


if __name__ == "__main__":  # pragma: no cover
    import sys

    sys.path.insert(0, str(Path(__file__).resolve().parent))
    raise SystemExit(_main())
