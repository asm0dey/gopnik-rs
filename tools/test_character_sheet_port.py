#!/usr/bin/env python3
"""Every game string `src/character_sheet.rs` SHIPS is decoded out of `orig/g.exe`.

`tools/test_string_citations.py` compares a `CS 0x....` citation against a
BACKTICK-quoted literal in the comment beside it. That is the right check for
a prose citation, and it leaves the thing that actually reaches the player
unchecked: the sheet's strings are Rust **double-quoted** literals, and a
comment can agree with the binary while the code one line below it does not.
A missing trailing space, a `^4` where the original writes `^1`, a Cyrillic
`с` typed as a Latin `c` -- none of those is visible to a reader and none is
caught by grep.

So this decodes the code. Two checks, both over `orig/g.exe`:

  * **every shipped literal exists** -- each game-text string literal above
    the module's `#[cfg(test)]` is encoded as a Borland shortstring (a length
    byte then cp866) and required to occur in the image;
  * **every `CS 0x....` citation holds the literal beside it** -- the
    shortstring at the cited offset must equal the nearest Rust literal, or,
    when that literal is a `format!` template, one of the fragments the
    `{...}` placeholders split it into. `Сл:^{c0}#^7 Лв:^{c1}...` splits into
    exactly the five literals `1000:1b24`..`1000:1ba0` append, which is what
    makes the citation on a composed line checkable at all.

Both checks are two-sided: the controls below feed the same scanners a
literal that is one character wrong and require them to reject it.

Standard library only.
"""
import pathlib
import re
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import addr as addrmod                                              # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
PORT = ROOT / "src" / "character_sheet.rs"

#: A `CS 0xNNNN` citation in a comment.
CS_CITE = re.compile(r"\bCS\s+`0x([0-9A-Fa-f]{4})`")
#: One Rust double-quoted string literal, `\"` and `\\` escapes honoured.
RUST_STR = re.compile(r'"((?:[^"\\\n]|\\.)*)"')
#: A `{...}` placeholder in a `format!` template.
PLACEHOLDER = re.compile(r"\{[^{}]*\}")

#: How far forward a citation may look for the literal it names. The longest
#: real gap is the stat line's, where five citations share one composed
#: literal eight lines below the first of them.
LOOKAHEAD = 10


def image():
    return addrmod.load_image(addrmod.read_exe(ROOT / "orig" / "g.exe"))


def shortstring(img, image_off):
    """The Borland shortstring at an IMAGE offset, or `None`."""
    if not 0 <= image_off < len(img):
        return None
    n = img[image_off]
    try:
        return img[image_off + 1:image_off + 1 + n].decode("cp866")
    except UnicodeDecodeError:
        return None


def encode(s):
    """`s` as the bytes a Borland shortstring would hold, or `None`."""
    try:
        body = s.encode("cp866")
    except UnicodeEncodeError:
        return None
    if not 0 < len(body) < 256:
        return None
    return bytes([len(body)]) + body


def unescape(lit):
    """The Rust source form of one string literal -> its value."""
    return lit.replace('\\"', '"').replace("\\\\", "\\")


def is_game_text(s):
    """Text the game would print: a `^N` colour code or any Cyrillic."""
    return bool(re.search(r"\^[0-7]", s)) or any(
        "Ѐ" <= c <= "ӿ" for c in s)


def code_of(line):
    """The line with any trailing `//` comment removed."""
    i = line.find("//")
    return line if i < 0 else line[:i]


def port_lines():
    """The module's source above `#[cfg(test)]`, as a list of lines."""
    text = PORT.read_text(encoding="utf-8")
    return text[:text.index("#[cfg(test)]")].splitlines()


def literals_on(line):
    """Every game-text literal in the CODE part of one line, unescaped."""
    return [unescape(m.group(1)) for m in RUST_STR.finditer(code_of(line))
            if is_game_text(unescape(m.group(1)))]


def fragments(lit):
    """The runs of a literal that the ORIGINAL holds as one shortstring.

    A plain literal is one run. A `format!` template is the runs its `{...}`
    placeholders separate -- `Сл:^{c0}#^7 Лв:^{c1}...` is five appends in the
    original, not one string, so five runs are what must be looked up.
    """
    if PLACEHOLDER.search(lit):
        return {p for p in PLACEHOLDER.split(lit) if p}
    return {lit}


def shipped_literals(lines):
    """Every game-text run the module ships, with its line number."""
    out = []
    for i, line in enumerate(lines):
        for lit in literals_on(line):
            for frag in sorted(fragments(lit)):
                out.append((i + 1, frag))
    return out


def cited_pairs(lines):
    """`(line_no, offset, literal_or_None)` for every `CS 0x....` citation."""
    out = []
    for i, line in enumerate(lines):
        for hexoff in CS_CITE.findall(line):
            found = None
            for j in range(i, min(i + 1 + LOOKAHEAD, len(lines))):
                lits = literals_on(lines[j])
                if lits:
                    found = lits[0]
                    break
            out.append((i + 1, int(hexoff, 16), found))
    return out


def scan_existence(img, lines):
    """Literals whose bytes are nowhere in the image."""
    return [(n, lit) for n, lit in shipped_literals(lines)
            if (b := encode(lit)) is None or img.find(b) < 0]


def scan_citations(img, lines):
    """Citations whose offset does not hold the literal beside them."""
    bad = []
    for n, off, lit in cited_pairs(lines):
        here = shortstring(img, off)
        if lit is None:
            bad.append("line %d: CS 0x%04X has no literal within %d lines"
                       % (n, off, LOOKAHEAD))
        elif here is None or here not in fragments(lit):
            bad.append("line %d: CS 0x%04X holds %r, but the literal there "
                       "is %r" % (n, off, here, lit))
    return bad


class CharacterSheetPortTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.img = image()
        cls.lines = port_lines()

    def test_every_shipped_literal_is_a_shortstring_in_the_image(self):
        bad = scan_existence(self.img, self.lines)
        self.assertEqual(
            bad, [],
            "\n".join("line %d: %r is not in orig/g.exe" % b for b in bad))
        # The scan must have done something. The floor is deliberately far
        # below the real count -- a tight number here would fail on every
        # honest edit and teach the next author to bump it, which is how a
        # count stops being evidence.
        self.assertGreater(len(shipped_literals(self.lines)), 30)

    def test_every_cs_citation_holds_the_literal_beside_it(self):
        self.assertEqual(scan_citations(self.img, self.lines), [])
        self.assertGreater(len(cited_pairs(self.lines)), 40)

    def test_the_existence_scan_rejects_a_one_character_typo(self):
        """`^1Бутсы(+1) ` is real; the same string with a Latin `c` is not.

        The two differ in one code point and render identically in a diff.
        """
        good = ['        o.write("^1Бутсы(+1) ");']
        bad = ['        o.write("^1Бутcы(+1) ");']
        self.assertEqual(scan_existence(self.img, good), [])
        self.assertEqual(len(scan_existence(self.img, bad)), 1)

    def test_the_existence_scan_rejects_a_dropped_trailing_space(self):
        good = ['        o.write("^4Бутсы ");']
        bad = ['        o.write("^4Бутсы");']
        self.assertEqual(scan_existence(self.img, good), [])
        self.assertEqual(len(scan_existence(self.img, bad)), 1)

    def test_the_citation_scan_rejects_a_neighbouring_offset(self):
        """`0x196e` and `0x1983` are the two suit labels, 21 bytes apart.

        Both are real offsets, both are cited correctly elsewhere in the
        module, and only decoding them tells the pair apart -- the Task 19
        defect class exactly.
        """
        good = ['            o.write("^1Костюм '
                'Abibas(+1) "); // CS `0x1983`']
        bad = ['            o.write("^1Костюм '
               'Abibas(+1) "); // CS `0x196e`']
        self.assertEqual(scan_citations(self.img, good), [])
        self.assertEqual(len(scan_citations(self.img, bad)), 1)

    def test_the_citation_scan_reports_a_citation_with_no_literal(self):
        orphan = ["    // CS `0x1983`"] + ["    // nothing here"] * LOOKAHEAD
        bad = scan_citations(self.img, orphan)
        self.assertEqual(len(bad), 1, bad)
        self.assertIn("no literal", bad[0])

    def test_a_format_template_is_checked_fragment_by_fragment(self):
        """The composed lines are the ones a naive scanner has to skip."""
        self.assertEqual(
            fragments("^{}Урон #-#    "),
            {"^", "Урон #-#    "})
        good = ['        // CS `0x1821`',
                '        &format!("^{}Урон #-#    ", d),']
        self.assertEqual(scan_citations(self.img, good), [])
        # 0x1664 is `^2Ты `, which is NOT a fragment of that template.
        bad = ['        // CS `0x1664`',
               '        &format!("^{}Урон #-#    ", d),']
        self.assertEqual(len(scan_citations(self.img, bad)), 1)


if __name__ == "__main__":
    unittest.main()
