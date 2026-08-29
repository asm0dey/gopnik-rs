#!/usr/bin/env python3
"""Every `file 0xNNNN` citation next to a quoted literal must RESOLVE to it.

Task 19's review found four wrong `file` offsets in `src/persist.rs`, each
one the offset of a *neighbouring* string. None was catchable by grep: two of
the four values were real offsets used CORRECTLY elsewhere in the same file,
and the other two were valid-looking numbers a few bytes off. The only thing
that separates a right citation from a plausible one is decoding the bytes,
so this decodes them.

What it checks, per source file: for every `file 0xNNNN` (or `image 0xNNNN`
/ `CS 0xNNNN`) that has a backtick-quoted literal within one line of it,
read the Borland shortstring at that offset out of `orig/g.exe` and require
the literal to be a prefix of it. `unchecked` counts the citations with no
adjacent literal to compare against -- those are addresses, not strings, and
the count is asserted so the population cannot quietly shrink to zero.
`unchecked` is NOT a clean "unverified string citations" tally: a `file`
citation for an *instruction* address (e.g. `src/locations.rs`'s
`file 0xC462` / `1000:ab92`) matches `CITE` and has no nearby literal to
compare, so it lands in `unchecked` too, indistinguishable from a citation
this scanner simply cannot reach. Do not read `unchecked` as a defect count.

`tools/re_derive.py` does the same job for `docs/re/*.md`'s `1000:xxxx`
instruction citations; this is its counterpart for string offsets in `src/`.
Standard library only.
"""
import pathlib
import re
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import addr as addrmod                                              # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent

#: Files whose string citations this checks. Task 19's own prose plus the two
#: `src/` files the review found it contradicting, plus `src/game.rs`
#: (Task 20's review round, M6): Task 20 added five new `file 0x...`
#: citations there and none was machine-checked, because nothing pointed
#: this scanner at the file that carries the game's own text -- the single
#: largest source of string citations in the tree.
SOURCES = [
    "src/persist.rs",
    "src/save.rs",
    "src/locations.rs",
    "src/game.rs",
    "src/character_sheet.rs",
    "docs/re/save-format.md",
]

# `{4,5}` -- most citations are the game code segment's 4-hex-digit file
# offsets, but a handful (Task 20's review round, M6: added while widening
# `SOURCES` to `src/game.rs`) reach into the RTL segments past file offset
# 0x10000 (`orig/g.exe` is 0x15a50 bytes), which needs a 5th digit -- e.g.
# `file 0x11166`, `1f78:0116`'s real offset. Greedy, so a genuine 4-digit
# citation is unaffected as long as the character right after it is not
# itself a hex digit (true of every citation format in this tree: a
# backtick, space, or punctuation always follows).
CITE = re.compile(r"\b(file|image|CS)\s+`?0x([0-9A-Fa-f]{4,5})`?")
#: A backtick-quoted run. Non-greedy so `a` / `b` on one line yields two.
QUOTED = re.compile(r"`([^`\n]{1,80})`")

#: A quoted span this test is willing to call GAME TEXT, i.e. something that
#: must exist verbatim at SOME cited offset on the line: a colour-marked
#: string, a whole DOS filename, or any span holding a Cyrillic character.
#:
#: Two false positives it was tuned against, both hit while writing it:
#: `.sav` alone and `save_r<slot>.sav` are fragments of a name BUILT at
#: runtime (`save_r` + a digit + `.sav`), so no single offset holds either
#: and citing one is not a claim about its bytes -- hence the `<`/`{`
#: exclusion and the 4-character minimum. And a plain-Russian span with no
#: `^` prefix, such as ` района`, IS game text: a line may cite two literals
#: at two offsets, and a matcher that saw only the colour-marked one flagged
#: the second citation for holding the first one's string.
GAME_TEXT = re.compile(r"^(?![^`]*[<{])(\^.|[a-z0-9_?]{4,}\.sav$|[^\u0400-\u04FF]*[\u0400-\u04FF])")

#: Quoted spans that are never string literals: addresses, identifiers, the
#: markup of a citation itself.
NOT_A_LITERAL = re.compile(
    r"^(0x[0-9A-Fa-f]+|[0-9a-f]{4}:[0-9a-f]{4}|\.SAV.*|20ae:.*|DS:.*|"
    r"[A-Za-z_][A-Za-z0-9_:<>\[\]&' ]*|\d+|.{0,1})$")


def image():
    return addrmod.load_image(addrmod.read_exe(ROOT / "orig" / "g.exe"))


def shortstring(img, image_off):
    n = img[image_off]
    try:
        return img[image_off + 1:image_off + 1 + n].decode("cp866")
    except UnicodeDecodeError:                       # pragma: no cover
        return None


def literals_near(lines, i):
    """Backtick-quoted spans on line `i` and its two neighbours."""
    out = []
    for j in (i - 1, i, i + 1):
        if 0 <= j < len(lines):
            for m in QUOTED.finditer(lines[j]):
                text = m.group(1)
                if not NOT_A_LITERAL.match(text):
                    out.append(text)
    return out


def scan(img, text, rel="<memory>"):
    """(checked, unchecked, bad) for one source file's citations."""
    lines = text.splitlines()
    checked, unchecked, bad = 0, 0, []
    for i, line in enumerate(lines):
        for kind, hexoff in CITE.findall(line):
            off = int(hexoff, 16)
            io = (addrmod.image_off_of_file_off(off)
                  if kind == "file" else off)
            if not 0 <= io < len(img):
                bad.append("%s:%d %s 0x%04X is outside the image"
                           % (rel, i + 1, kind, off))
                continue
            here = shortstring(img, io)
            near = [t for t in literals_near(lines, i) if GAME_TEXT.match(t)]
            if not near:
                unchecked += 1
                continue
            if here is not None and any(here.startswith(t) for t in near):
                checked += 1
                continue
            bad.append(
                "%s:%d  %s 0x%04X holds %r, but the line cites it for %r"
                % (rel, i + 1, kind, off, (here or "")[:40], near[0][:40]))
    return checked, unchecked, bad


class StringCitationTest(unittest.TestCase):
    def setUp(self):
        self.img = image()

    def test_no_cited_offset_in_the_tree_holds_the_wrong_string(self):
        checked, unchecked, bad = 0, 0, []
        for rel in SOURCES:
            c, u, b = scan(self.img, (ROOT / rel).read_text(encoding="utf-8"),
                           rel)
            checked, unchecked = checked + c, unchecked + u
            bad += b
        self.assertEqual(bad, [], "\n".join(bad))
        # The scan must have DONE something. A count floor here would be a
        # number that cannot fail downwards, so the two-sided control is the
        # synthetic fixture below instead; this only rules out zero.
        self.assertGreater(checked, 0, "no citation was resolved at all")
        self.assertGreater(unchecked, 0, "sanity: some citations are addresses")

    def test_the_scanner_accepts_a_right_citation_and_refuses_a_wrong_one(self):
        """The two-sided control, on a fixture rather than on the tree.

        Both offsets are real and both are used correctly somewhere in `src/`;
        only decoding the bytes tells them apart, which is exactly why the
        review's four wrong offsets were invisible to grep.
        """
        good = "/// `places.sav` lives at file `0x7CC2`.\n"
        bad_ = "/// `places.sav` lives at file `0x7C33`.\n"
        c, _, b = scan(self.img, good)
        self.assertEqual((c, b), (1, []))
        c, _, b = scan(self.img, bad_)
        self.assertEqual(c, 0)
        self.assertEqual(len(b), 1, b)
        self.assertIn("0x7C33", b[0])
        self.assertIn(" \u0440\u0430\u0439\u043e\u043d\u0430", b[0])

    def test_a_composed_filename_is_not_treated_as_a_literal(self):
        """`save_r<slot>.sav` is built at runtime from the `save_r` prefix and
        a digit, so no single offset holds it. Citing one is not a claim about
        its bytes, and treating it as one was a false positive this test hit
        while it was being written."""
        text = "/// `save_r<slot>.sav`, built from `save_r` at CS `0x63d0`.\n"
        c, u, b = scan(self.img, text)
        self.assertEqual(b, [])
        self.assertEqual((c, u), (0, 1))

    def test_the_two_citations_src_already_carried_are_the_ones_kept(self):
        """`src/locations.rs` and `src/game.rs` had `0x7CC2` and `0x7D21`
        before this branch; the branch briefly contradicted both."""
        for off, want in ((0x7CC2, "places.sav"),
                          (0x7D21, "^6\u0427\u0451-\u0442\u043e ")):
            got = shortstring(self.img, addrmod.image_off_of_file_off(off))
            if got is None:
                # `shortstring` returns `None` on a cp866 decode failure
                # (`scan()` treats that as a BAD citation, not a skip -- see
                # its `here is not None` guard). Match that convention here:
                # report it as a real failure, not an unguarded crash.
                self.fail("%s: shortstring failed to decode (cp866)" % hex(off))
            self.assertTrue(got.startswith(want), (hex(off), got[:40]))


if __name__ == "__main__":
    unittest.main()
