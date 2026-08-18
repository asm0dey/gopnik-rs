#!/usr/bin/env python3
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from decode_save import decode, encode  # noqa: E402

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

# Established by inspection of all five saves. hp/hpmax are the only
# semantically confirmed words at this stage; the rest stay unk_* until
# Task 9 pins them from the disassembly.
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


if __name__ == "__main__":
    test_all()
