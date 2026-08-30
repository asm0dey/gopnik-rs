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


#: A Rust string literal, for the comment-vs-code pairing below.
RUST_LITERAL = re.compile(r'"((?:[^"\\\n]|\\.){2,120})"')


def code_literals(line):
    """Game-text Rust literals on `line`, ignoring comment and doc lines.

    The doc-line exclusion is not cosmetic: `src/game.rs`'s `run_combat`
    prose quotes `"^2\u0422\u044b \u043f\u043e\u0431\u0435\u0434\u0438\u043b."` with straight quotes to say it is
    NOT a per-fight line, which a matcher that read every `"..."` took for a
    literal beside a neighbouring citation.
    """
    if line.lstrip().startswith("//"):
        return []
    return [t for t in RUST_LITERAL.findall(line) if t.startswith("^")]


def comment_code_pairs(text):
    """(pairs, bad) for backticked citations that name an adjacent literal.

    `scan` binds a cited OFFSET to the backticked text beside it. It says
    nothing about the Rust literal the port actually prints, so editing the
    string in the code and leaving the comment alone stays green -- the exact
    mutation the final review demonstrated. This binds the other half: where a
    citation line carries a backticked game-text literal AND the nearest code
    line within two carries one, the two must agree.

    "Agree" is prefix-either-way, not equality, because a comment may cite the
    binary's string while the code holds a `format!` template that extends it
    (`^0\u0417\u0430\u0433\u0440\u0443\u0436\u0435\u043d\u043e \u0438\u0437 save_r` / `...{slot}`) -- and because a comment may quote a
    prefix of a long line.
    """
    lines = text.splitlines()
    pairs, bad = 0, []
    for i, line in enumerate(lines):
        if not CITE.search(line):
            continue
        quoted = [t for t in QUOTED.findall(line) if t.startswith("^")]
        if not quoted:
            continue
        cand = code_literals(line)
        for j in range(i + 1, min(len(lines), i + 3)):
            if cand:
                break
            cand = code_literals(lines[j])
        if not cand:
            continue
        pairs += 1
        if not any(a.startswith(b) or b.startswith(a)
                   for a in quoted for b in cand):
            bad.append("line %d cites %r but the code beside it prints %r"
                       % (i + 1, quoted[0][:40], cand[0][:40]))
    return pairs, bad


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

    def test_a_backticked_citation_agrees_with_the_literal_beside_it(self):
        """The half `scan` cannot see: comment against CODE, not against bytes.

        `scan` proves a cited offset holds the backticked text. Nothing proved
        the backticked text is what the port prints, so changing a
        `term::println` literal and leaving its comment alone stayed green --
        which is what the final whole-branch review demonstrated on
        `^2\u0421\u043f\u0430\u0441\u0430\u0439\u0441\u044f \u043a\u0442\u043e \u043c\u043e\u0436\u0435\u0442!!!`. Together the two halves bind
        offset -> comment -> code.
        """
        pairs, bad = 0, []
        for rel in SOURCES:
            if not rel.endswith(".rs"):
                continue
            p, b = comment_code_pairs((ROOT / rel).read_text(encoding="utf-8"))
            pairs += p
            bad += ["%s %s" % (rel, x) for x in b]
        self.assertEqual(bad, [], "\n".join(bad))
        # Not a floor that cannot fail downwards: the two-sided control below
        # is what proves the pairing can go red at all. This rules out a
        # regex change that silently pairs nothing.
        self.assertGreater(pairs, 0, "no citation was paired with code")

    def test_the_pairing_accepts_a_matching_literal_and_refuses_a_drifted_one(self):
        """Two-sided control for `comment_code_pairs`, on a fixture."""
        ok = ('// CS 0x95c3 `^2\u0421\u043f\u0430\u0441\u0430\u0439\u0441\u044f \u043a\u0442\u043e \u043c\u043e\u0436\u0435\u0442!!!`, pushed at 1000:cd18.\n'
              'term::println("^2\u0421\u043f\u0430\u0441\u0430\u0439\u0441\u044f \u043a\u0442\u043e \u043c\u043e\u0436\u0435\u0442!!!");\n')
        drifted = ('// CS 0x95c3 `^2\u0421\u043f\u0430\u0441\u0430\u0439\u0441\u044f \u043a\u0442\u043e \u043c\u043e\u0436\u0435\u0442!!!`, pushed at 1000:cd18.\n'
                   'term::println("^2\u0421\u043f\u0430\u0441\u0430\u0439\u0441\u044f \u043a\u0442\u043e \u041d\u0415 \u043c\u043e\u0436\u0435\u0442!!!");\n')
        self.assertEqual(comment_code_pairs(ok), (1, []))
        pairs, bad = comment_code_pairs(drifted)
        self.assertEqual(pairs, 1)
        self.assertEqual(len(bad), 1, bad)
        self.assertIn("\u041d\u0415", bad[0])

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


#: `docs/re/gaps.md`'s trimmed-prompt entry publishes this command instead of
#: a pasted listing. `TrimInventoryTest` reruns its filter.
TRIM_GREP = re.compile(r"\.trim\(\)")
TRIM_NOT = re.compile(r"trim_end_matches|trim_start_matches")
#: A line is PROSE when the `.trim()` sits inside a `//` or `///` comment.
TRIM_COMMENT = re.compile(r"^\s*//")
#: `fn foo(` / `pub fn foo(` at any indent -- enough to name the enclosing
#: item for every hit in `src/*.rs`; there are no nested `fn`s among them.
TRIM_FN = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")

#: The entry writes its counts as English words, so read them as written.
WORDS = {"zero": 0, "one": 1, "two": 2, "three": 3, "four": 4, "five": 5,
         "six": 6, "seven": 7, "eight": 8, "nine": 9, "ten": 10,
         "eleven": 11, "twelve": 12, "thirteen": 13, "fourteen": 14,
         "fifteen": 15, "sixteen": 16, "seventeen": 17, "eighteen": 18,
         "nineteen": 19, "twenty": 20}

TRIM_SENTENCE = re.compile(
    r"\*\*([A-Za-z]+) hits: ([A-Za-z]+) call sites and ([A-Za-z]+) lines of "
    r"prose about them\.\*\*")

#: One table row: `| `Game::foo` | ... |`, taking the LAST backticked span of
#: the first cell (`` `main.rs`'s `read_number` `` names two).
TRIM_ROW = re.compile(r"^\|\s*(.+?)\s*\|\s*(.+?)\s*\|$")
BACKTICKED = re.compile(r"`([^`]+)`")


def trim_hits():
    """(call_sites, prose) as [(rel, lineno, enclosing fn name)] each."""
    calls, prose = [], []
    for path in sorted((ROOT / "src").glob("*.rs")):
        fn = None
        for i, line in enumerate(
                path.read_text(encoding="utf-8").splitlines(), 1):
            m = TRIM_FN.match(line)
            if m:
                fn = m.group(1)
            if not TRIM_GREP.search(line) or TRIM_NOT.search(line):
                continue
            rec = ("src/" + path.name, i, fn)
            (prose if TRIM_COMMENT.match(line) else calls).append(rec)
    return calls, prose


class TrimInventoryTest(unittest.TestCase):
    """`docs/re/gaps.md`'s trimmed-prompt inventory, recomputed from `src/`.

    That entry lists every place the port `.trim()`s a line the original
    hands to `0f78:0bd8` unmodified -- the divergence itself is recorded
    there, not here. The listing has gone stale FOUR times: three while it
    was a pasted `grep` output whose line numbers shifted, and once more
    after it became a command, when Task 30 added `Game::sell_offer` as the
    tenth call site and updated nothing. Making the numbers recomputable did
    not make anything recompute them.

    So this does. It is two-sided on purpose: the counts in the entry's
    sentence AND the `where` column of its table are both compared against
    what `src/*.rs` holds, so it fails whether the code moves or the document
    does. It lives in this file rather than in a new sibling because this is
    already the tree's `src/`-versus-a-document scanner -- see the module
    docstring; `re_derive.py` is its opposite number for `docs/re/*.md`.
    """

    @classmethod
    def setUpClass(cls):
        cls.gaps = (ROOT / "docs/re/gaps.md").read_text(encoding="utf-8")
        cls.calls, cls.prose = trim_hits()

    def sentence(self):
        m = TRIM_SENTENCE.search(self.gaps)
        self.assertIsNotNone(
            m, "docs/re/gaps.md no longer carries the trimmed-prompt entry's "
               "`**N hits: M call sites and K lines of prose about them.**` "
               "sentence -- this guard cannot check a listing it cannot find")
        try:
            return tuple(WORDS[w.lower()] for w in m.groups())
        except KeyError as e:
            self.fail("the entry writes a number word this guard does not "
                      "know: %s" % e)

    def table_rows(self):
        """The `where` column, as bare fn names, in the entry's own order."""
        start = self.gaps.index("| where | what it normalises |")
        rows = []
        for line in self.gaps[start:].splitlines()[2:]:
            m = TRIM_ROW.match(line)
            if not m:
                break
            spans = BACKTICKED.findall(m.group(1))
            self.assertTrue(spans, "table row names nothing: %r" % line)
            rows.append(spans[-1].split("::")[-1])
        return rows

    def test_the_counts_the_entry_states_are_what_src_holds(self):
        total, calls, prose = self.sentence()
        self.assertEqual(
            (len(self.calls), len(self.prose)), (calls, prose),
            "docs/re/gaps.md says %d call sites and %d prose lines; "
            "`grep -rn '.trim()' src/*.rs` minus the two `trim_*_matches` "
            "forms holds %d and %d.\ncall sites: %r\nprose: %r"
            % (calls, prose, len(self.calls), len(self.prose),
               self.calls, self.prose))
        self.assertEqual(
            total, calls + prose,
            "the entry's own total does not equal its own two parts")

    def test_the_table_lists_exactly_the_call_sites(self):
        want = sorted(fn for _, _, fn in self.calls)
        self.assertNotIn(
            None, want,
            "a `.trim()` call site sits outside any `fn` -- the row name "
            "cannot be derived: %r" % self.calls)
        self.assertEqual(
            sorted(self.table_rows()), want,
            "docs/re/gaps.md's trimmed-prompt table and `src/*.rs` disagree "
            "about WHICH functions trim.\ntable: %r\nsrc:   %r"
            % (sorted(self.table_rows()), want))

    def test_the_recomputation_is_not_vacuous(self):
        """A guard that found nothing would pass both tests above silently."""
        self.assertGreater(len(self.calls), 5)
        self.assertGreater(len(self.prose), 0)
        self.assertIn(
            "sell_offer", [fn for _, _, fn in self.calls],
            "the Task 30 call site this guard was written for is gone; if "
            "that is deliberate, drop its row from docs/re/gaps.md too")


if __name__ == "__main__":
    unittest.main()
