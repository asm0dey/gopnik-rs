#!/usr/bin/env python3
import json
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import addr as addrmod                                              # noqa: E402
import dis16                                                        # noqa: E402
import re_query                                                     # noqa: E402
import decode_save                                                  # noqa: E402
from decode_save import (                                           # noqa: E402
    RECORD_BASE,
    SIZE,
    decode,
    decode_fields,
    encode,
)

ROOT = pathlib.Path(__file__).resolve().parent.parent
ORIG = ROOT / "orig"

# Offsets as documented in docs/re/save-format.md, hardcoded here as literals
# rather than imported from decode_save's OFF_* constants. Importing them
# would make the offset check below tautological: if a constant in
# decode_save.py were wrong, decode() and this check would both derive the
# same (wrong) slice from it and silently agree. Hardcoding forces the test
# to check the module's actual behaviour against an independently stated
# expectation.
CHK_OFF_MAGIC = 0x000
CHK_OFF_NAME = 0x100
CHK_OFF_STATE = 0x200
CHK_OFF_HP = 0x210
CHK_OFF_HPMAX = 0x212

# Established by inspection of all five saves. Task 9 additionally pinned
# the eight stat words at 0x200-0x20f from the disassembly (see
# docs/re/save-format.md); this dict only carries the values this test
# checks directly (name/hp/hpmax), unrelated to that later confirmation.
EXPECT = {
    "SAVE_R0.SAV": {"name": "^7 adg", "hp": 118, "hpmax": 129},
    "SAVE_R2.SAV": {"name": "^7 vor", "hp": 84, "hpmax": 99},
    "SAVE_R3.SAV": {"name": "^7 vor", "hp": 178, "hpmax": 178},
    "SAVE_R4.SAV": {"name": "^7 vor", "hp": 251, "hpmax": 270},
    "SAVE_R5.SAV": {"name": "^7 Mudila", "hp": 325, "hpmax": 325},
}

MAGIC = "^4Gopnik: ^7version 1.02 june,sept 2003"


def _named_bytes_from_fields(rec: dict) -> dict:
    """Independently rebuild the byte slices for every *named* offset,
    starting only from the decoded field values (never from rec["_raw"]).

    This exists because a round-trip check alone cannot catch a wrong
    offset: decode() stashes the whole input as rec["_raw"], and encode()
    starts from bytearray(rec["_raw"]) and only overwrites the slices it
    knows about, so any byte the decoder mis-locates is still copied
    through untouched from the original blob and the round-trip still
    passes. Comparing bytes built purely from the decoded values against
    the corresponding slice of the original file is the only way to prove
    an offset is right rather than merely self-consistent.
    """
    out = {}
    out["magic"] = rec["magic"].encode("cp866")
    out["name"] = rec["name"].encode("cp866")
    stats_bytes = b"".join(int(v).to_bytes(2, "little") for v in rec["stats"])
    out["stats"] = stats_bytes
    out["hp"] = int(rec["hp"]).to_bytes(2, "little")
    out["hpmax"] = int(rec["hpmax"]).to_bytes(2, "little")
    return out


def _check_offsets(blob: bytes, rec: dict, fname: str) -> None:
    built = _named_bytes_from_fields(rec)

    # pstrings: length byte + payload, compared against the original slice
    # at the documented offset (CHK_OFF_*, hardcoded above -- not imported
    # from decode_save).
    magic_len = blob[CHK_OFF_MAGIC]
    assert blob[CHK_OFF_MAGIC + 1 : CHK_OFF_MAGIC + 1 + magic_len] == built["magic"], (
        f"{fname}: magic bytes at 0x{CHK_OFF_MAGIC:03x} don't match decoded field"
    )
    name_len = blob[CHK_OFF_NAME]
    assert blob[CHK_OFF_NAME + 1 : CHK_OFF_NAME + 1 + name_len] == built["name"], (
        f"{fname}: name bytes at 0x{CHK_OFF_NAME:03x} don't match decoded field"
    )

    assert blob[CHK_OFF_STATE : CHK_OFF_STATE + 16] == built["stats"], (
        f"{fname}: stat words at 0x{CHK_OFF_STATE:03x} don't match decoded field"
    )
    assert blob[CHK_OFF_HP : CHK_OFF_HP + 2] == built["hp"], (
        f"{fname}: hp bytes at 0x{CHK_OFF_HP:03x} don't match decoded field"
    )
    assert blob[CHK_OFF_HPMAX : CHK_OFF_HPMAX + 2] == built["hpmax"], (
        f"{fname}: hpmax bytes at 0x{CHK_OFF_HPMAX:03x} don't match decoded field"
    )


def test_all():
    for fname, want in EXPECT.items():
        blob = (ORIG / fname).read_bytes()
        rec = decode(blob)

        assert rec["magic"] == MAGIC, f"{fname}: magic {rec['magic']!r}"
        assert rec["name"] == want["name"], f"{fname}: name {rec['name']!r}"
        assert rec["hp"] == want["hp"], f"{fname}: hp {rec['hp']}"
        assert rec["hpmax"] == want["hpmax"], f"{fname}: hpmax {rec['hpmax']}"
        assert rec["hp"] <= rec["hpmax"], f"{fname}: hp exceeds hpmax"

        # Independent check that the named offsets are actually right:
        # rebuild the claimed byte regions from the decoded values alone
        # (not from rec["_raw"]) and compare against the original file.
        _check_offsets(blob, rec, fname)

        # Round trip proves we do not corrupt any byte we re-emit (named
        # fields and opaque tail/padding alike); it does NOT by itself
        # prove the named offsets are correct -- see _check_offsets above.
        assert encode(rec) == blob, f"{fname}: round-trip mismatch"

    print(f"OK {len(EXPECT)} saves decoded and round-tripped byte-identically")


# ---------------------------------------------------------------------------
# Task 19: the layout artifact re-derived from `orig/g.exe`.
#
# `data/save_layout.json` is a claim about which DGROUP byte each `.SAV`
# offset is. Every check below recomputes that claim out of the binary
# rather than restating it, because a table checked against its own literals
# is the failure this project keeps finding.
# ---------------------------------------------------------------------------

#: The SHIPPED artifact, not decode_save.LAYOUT. Reading the generator's own
#: table here would make every check below a comparison of the generator with
#: itself; `data/save_layout.json` is what `src/save.rs` and
#: `tests/save_roundtrip.rs` consume, so it is what has to be right.
#: `test_the_generator_and_the_artifact_agree` is the one check that reads
#: both.
LAYOUT = json.loads((ROOT / "data" / "save_layout.json").read_text(encoding="utf-8"))

#: Every `.SAV` byte the record's Boolean flags occupy.
#: A Pascal `Boolean` is one byte holding 0 or 1 and nothing else; the test
#: below proves that from every direct store image-wide.
FLAG_FIELDS = [f for f in LAYOUT["fields"] if f["kind"] == "bool"]

#: The four signed words of the old `unk_0214` span, the street-cred word,
#: and the cartridge count. Each is asserted to be reached by a WORD compare
#: followed by a SIGNED conditional, which is what makes it an `Integer`
#: rather than two independent bytes.
SIGNED_WORD_FIELDS = [f for f in LAYOUT["fields"] if f["kind"] == "i16"]

#: The two censuses, written down rather than derived, so that RETYPING a
#: field cannot quietly shrink the population a check below walks. Every
#: kind-driven test asserts its own list against these first: without that,
#: turning `cartridges` from `i16` into `u8` makes the signed-word check
#: pass by having nothing left to look at.
EXPECT_FLAG_NAMES = [
    "broken_jaw", "broken_leg", "dark_glasses", "suit_abibas", "boots",
    "jacket", "suit_adidas", "boots_pontovye", "jacket_krutaya", "kastet",
    "mobile", "prison_tattoo", "krestik", "ring_gs", "ring_pg", "mega_ring",
    "ring_gp", "nozh", "tooth_guard", "dubinka", "tesak", "pistol",
    "silencer",
]
EXPECT_SIGNED_WORD_NAMES = [
    "beer_half_litres", "joints", "money", "junk", "street_cred",
    "cartridges",
]

#: Every field Task 19 established, i.e. everything at or past `.SAV 0x214`
#: that carries a `guest` address. 31 in the two former `unk_` spans plus
#: the four Task 9b already had.
EXPECT_ESTABLISHED_TAIL_NAMES = sorted(
    EXPECT_FLAG_NAMES + EXPECT_SIGNED_WORD_NAMES
    + ["armour", "buff_countdown", "xp", "threshold", "growth_log",
       "church_stage"]
)

#: Fields the five reference saves cannot corroborate, because every save
#: holds the same value: their reading rests on FLOW alone. Named here so
#: nobody later reads the state table as if it had confirmed them.
#: `broken_jaw` is clear in all five (nobody saved mid-fracture);
#: `dark_glasses`, `mobile`, `prison_tattoo`, `ring_pg` and `mega_ring` are
#: set in all five.
FLOW_ONLY_FIELDS = [
    "broken_jaw",
    "dark_glasses",
    "mega_ring",
    "mobile",
    "prison_tattoo",
    "ring_pg",
]

SIGNED_CONDITIONALS = {
    "jl", "jnl", "jle", "jnle", "jg", "jge", "jng", "jnge",
}


def _image():
    return addrmod.load_image(addrmod.read_exe(ROOT / "orig" / "g.exe"))


def _insn_at(image, citation):
    off = addrmod.image_off_of_citation(citation)
    return off, dis16.decode(image, off)


class TestRecordBaseIsEstablishedFromFlow(unittest.TestCase):
    """`.SAV` offset + RECORD_BASE == the DGROUP address of the same byte.

    Not a correlation over the five saves: the record is moved between the
    file and `DS:369c` by one untyped block operation in each direction, so
    the delta is whatever those two call sites name.
    """

    def setUp(self):
        self.image = _image()

    def test_the_block_read_targets_the_record_base(self):
        _, insn = _insn_at(self.image, "1000:6c01")
        self.assertEqual(insn.text, "mov di,0x%x" % RECORD_BASE)
        # ...and the call two instructions later is BlockRead, not something
        # else that happens to take a pointer.
        _, call = _insn_at(self.image, "1000:6c06")
        self.assertEqual(call.text, "call 0xf78:0x81e")

    def test_the_block_writes_source_the_record_base(self):
        for setup, call in (("1000:acc3", "1000:acc8"), ("1000:7658", "1000:765d")):
            _, insn = _insn_at(self.image, setup)
            self.assertEqual(insn.text, "mov di,0x%x" % RECORD_BASE, setup)
            _, c = _insn_at(self.image, call)
            self.assertEqual(c.text, "call 0xf78:0x825", call)

    def test_the_record_size_pushed_at_every_open_is_the_file_size(self):
        # Reset (load), Rewrite (district-advance autosave), Rewrite (mage).
        for cit in ("1000:6bcb", "1000:acb5", "1000:764a"):
            _, insn = _insn_at(self.image, cit)
            self.assertEqual(insn.text, "mov ax,0x%x" % SIZE, cit)

    def test_the_delta_reproduces_the_two_independent_landmarks(self):
        # The class word, pinned by docs/re/progression.md long before this
        # task, and the pistol byte Task 18 ported. Neither was derived from
        # RECORD_BASE, so both are real checks on it.
        self.assertEqual(0x200 + RECORD_BASE, 0x389C)
        self.assertEqual(0x2B1 + RECORD_BASE, 0x394D)


class TestEveryNamedFieldMatchesTheBinary(unittest.TestCase):
    def setUp(self):
        self.image = _image()

    def test_every_field_declares_its_tier(self):
        """`docs/re/METHODOLOGY.md` requires a tier per claim, and this table
        is 47 of them. Asserted per field, not once in prose, so a field
        added at a weaker tier cannot inherit `flow` by silence."""
        missing = [f["name"] for f in LAYOUT["fields"] if "tier" not in f]
        self.assertEqual(missing, [], "fields with no tier: %s" % missing)
        tiers = {f["tier"] for f in LAYOUT["fields"]}
        self.assertEqual(
            tiers, {"flow"},
            "a non-flow tier appeared; say which field and why in "
            "docs/re/save-format.md before relaxing this",
        )

    def test_the_generator_and_the_artifact_agree(self):
        """`python3 tools/decode_save.py` must have been re-run after an edit
        to TAIL_FIELDS, or the shipped artifact is stale."""
        self.assertEqual(decode_save.LAYOUT, LAYOUT)

    def test_every_guest_address_is_the_record_base_plus_the_offset(self):
        seen = []
        for f in LAYOUT["fields"]:
            guest = f.get("guest")
            if guest is None:
                continue
            seen.append(f["name"])
            self.assertEqual(
                addrmod.citation(guest).off,
                RECORD_BASE + f["off"],
                "%s claims %s for .SAV 0x%03x" % (f["name"], guest, f["off"]),
            )
        # Exact, not a floor: dropping `guest` from a field would otherwise
        # excuse it from the check instead of failing it.
        self.assertEqual(sorted(seen), EXPECT_ESTABLISHED_TAIL_NAMES)

    def test_every_evidence_address_really_references_that_byte(self):
        """The cited instruction must carry the field's DGROUP address as an
        operand -- an address attributed to the wrong field fails here."""
        checked = []
        for f in LAYOUT["fields"]:
            cit, guest = f.get("evidence"), f.get("guest")
            if cit is None or guest is None:
                continue
            # `evidence_operand` exists for exactly one field: the growth
            # log, whose every reference carries Borland's biased base
            # (see decode_save.TAIL_FIELDS).
            want = addrmod.citation(f.get("evidence_operand", guest)).off
            _, insn = _insn_at(self.image, cit)
            values = {o.value for o in insn.operands if o.value is not None}
            self.assertIn(
                want,
                values,
                "%s: %s decodes as %r, which does not reference 0x%04x"
                % (f["name"], cit, insn.text, want),
            )
            checked.append(f["name"])
        # `xp` and `threshold` are the two Task 9b fields whose evidence
        # lives in docs/re/progression.md rather than in a single guard, so
        # they carry no `evidence` key and are not walked here.
        self.assertEqual(
            sorted(checked),
            sorted(n for n in EXPECT_ESTABLISHED_TAIL_NAMES
                   if n not in ("xp", "threshold")),
        )

    def test_the_flag_bytes_are_pascal_booleans(self):
        """Every direct store to a flag byte, image-wide, writes 0 or 1."""
        self.assertEqual(
            sorted(f["name"] for f in FLAG_FIELDS),
            sorted(EXPECT_FLAG_NAMES),
            "the boolean census has moved: retyping a field out of it would "
            "shrink what this test walks instead of failing it",
        )
        prog = re_query.Program(ROOT / "orig" / "g.exe")
        for f in FLAG_FIELDS:
            scan = re_query.xrefs_to(prog, f["guest"])["scan"]
            stores = [
                x for x in scan["accepted"]
                if x["text"].startswith("mov byte [") or "add byte" in x["text"]
            ]
            self.assertTrue(stores, "%s has no store at all" % f["name"])
            for x in stores:
                imm = x["text"].rsplit(",", 1)[1]
                self.assertIn(
                    imm, ("0x0", "0x1"),
                    "%s (%s): %s at %s writes %s, so it is not a Boolean"
                    % (f["name"], f["guest"], x["text"], x["at"], imm),
                )

    def test_the_word_fields_are_signed_integers(self):
        self.assertEqual(
            sorted(f["name"] for f in SIGNED_WORD_FIELDS),
            sorted(EXPECT_SIGNED_WORD_NAMES),
            "the signed-word census has moved: retyping a field out of it "
            "would shrink what this test walks instead of failing it",
        )
        for f in SIGNED_WORD_FIELDS:
            off, insn = _insn_at(self.image, f["evidence"])
            self.assertTrue(
                insn.text.startswith("cmp word ["),
                "%s: %s is %r, not a word compare" % (f["name"], f["evidence"], insn.text),
            )
            nxt = dis16.decode(self.image, off + insn.length)
            self.assertIn(
                nxt.text.split()[0],
                SIGNED_CONDITIONALS,
                "%s: %s is followed by %r, an unsigned test"
                % (f["name"], f["evidence"], nxt.text),
            )

    def test_the_word_fields_high_halves_have_no_reference_of_their_own(self):
        """The claim that makes them words rather than pairs of flags.

        If `20ae:38c4` were a flag in its own right the game would have to
        read it somewhere; nothing does.
        """
        self.assertEqual(
            sorted(f["name"] for f in SIGNED_WORD_FIELDS),
            sorted(EXPECT_SIGNED_WORD_NAMES),
            "the signed-word census has moved: retyping a field out of it "
            "would shrink what this test walks instead of failing it",
        )
        prog = re_query.Program(ROOT / "orig" / "g.exe")
        for f in SIGNED_WORD_FIELDS:
            high = addrmod.citation(f["guest"]).off + 1
            scan = re_query.xrefs_to(prog, "20ae:%04x" % high)["scan"]
            self.assertEqual(
                scan["accepted"], [],
                "%s's high half 20ae:%04x is referenced: %s"
                % (f["name"], high, scan["accepted"]),
            )


class TestFieldValuesInTheReferenceSaves(unittest.TestCase):
    """State-tier corroboration. These cannot establish a field's meaning --
    only the flow above does that -- but a name that contradicts all five
    real saves is refuted, and every one of these did have to hold."""

    def setUp(self):
        self.saves = {
            p.name: decode_fields(p.read_bytes())
            for p in sorted(ORIG.glob("SAVE_R*.SAV"))
        }
        self.assertEqual(len(self.saves), 5)

    def test_only_the_save_whose_owner_bought_a_pistol_carries_one(self):
        for name, r in self.saves.items():
            want = name == "SAVE_R5.SAV"
            self.assertEqual(bool(r["pistol"]), want, name)
            self.assertEqual(bool(r["silencer"]), want, name)
            self.assertEqual(r["cartridges"], 8 if want else 0, name)

    def test_every_boolean_field_really_holds_0_or_1(self):
        for name, r in self.saves.items():
            for f in FLAG_FIELDS:
                self.assertIn(r[f["name"]], (0, 1), "%s: %s" % (name, f["name"]))

    def test_the_growth_log_holds_exactly_level_entries_of_two_codes(self):
        for name, r in self.saves.items():
            log, level = r["growth_log"], r["level"]
            for i in range(40):
                ln, a, b = log[3 * i], log[3 * i + 1], log[3 * i + 2]
                if i < level:
                    self.assertEqual(ln, 2, "%s: slot %d" % (name, i))
                    self.assertIn(chr(a), "1234", "%s: slot %d" % (name, i))
                    self.assertIn(chr(b), "1234", "%s: slot %d" % (name, i))
                else:
                    self.assertEqual((ln, a, b), (0, 0, 0), "%s: slot %d" % (name, i))

    def test_the_buff_countdown_explains_the_two_hpmax_outliers(self):
        # hpmax == 10 + 5*vitality + strength - 2*(buff live), the identity
        # docs/re/progression.md records. It only closes if 0x231 is the
        # countdown AND 0x202/0x206 are strength/vitality.
        for name, r in self.saves.items():
            live = 1 if r["buff_countdown"] else 0
            self.assertEqual(
                r["hpmax"],
                10 + 5 * r["vitality"] + r["strength"] - 2 * live,
                name,
            )

    def test_the_five_saves_disagree_on_every_field_this_task_named(self):
        """A field constant across all five would be corroborated by nothing.

        This is the negative half: it names the fields whose reading rests on
        flow ALONE, so nobody later reads the state table as if it confirmed
        them.
        """
        named = [f["name"] for f in LAYOUT["fields"]
                 if f.get("evidence") and f["off"] >= 0x214]
        constant = sorted(
            n for n in named
            if len({bytes(r[n]) if isinstance(r[n], (bytes, bytearray)) else r[n]
                    for r in self.saves.values()}) == 1
        )
        self.assertEqual(
            constant,
            FLOW_ONLY_FIELDS,
            "the set of fields the five saves cannot distinguish has moved; "
            "update FLOW_ONLY_FIELDS and say which readings now rest on flow "
            "alone",
        )
        # ...and the complement is not empty either, or the check above would
        # be passing for the wrong reason.
        self.assertGreater(len(named) - len(constant), 20, named)


if __name__ == "__main__":
    test_all()
    unittest.main(argv=[sys.argv[0]], exit=False)
