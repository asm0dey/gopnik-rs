#!/usr/bin/env python3
"""Decode and re-encode GOPNIK .SAV files.

Layout (694 bytes total), established from the five reference saves:

    0x000  string[255]  magic       -- version banner, constant
    0x100  string[255]  name        -- player name, colour-prefixed
    0x200  u16          rank_index  -- indexes the DS:002e name table; the
                                       class-choice -> value mapping is not
                                       established (Task 9b)
    0x202  u16          strength
    0x204  u16          agility
    0x206  u16          vitality
    0x208  u16          luck
    0x20a  u16          level       -- "понтовость", 0..40 (Task 9)
    0x20c  u16          dmg_min
    0x20e  u16          dmg_max
    0x210  u16          hp
    0x212  u16          hpmax
    0x214  ...                      -- flags, counters, and a run of
                                        Pascal string[2] records; not yet
                                        segmented, preserved verbatim.

Field names and offsets 0x200..0x20f are pinned by Task 9: the player's
694-byte record in memory (DS:369c) is byte-identical to the .SAV file, and
that same record is what tools/capture_combat_vectors.py reads via
FIELDS_U16 to build every combat_vectors.json case -- 314 (now 352) blows
matched the original with these fields at these offsets. See
docs/re/combat.md ("The fighter record") and docs/re/save-format.md.

Everything past 0x214 is carried as opaque bytes so that round-trip is
exact. Task 9 replaces the opaque tail with named fields as they are
confirmed against the disassembly.
"""
import json
import pathlib
import sys

SIZE = 694
OFF_MAGIC = 0x000
OFF_NAME = 0x100
OFF_STATE = 0x200
OFF_HP = OFF_STATE + 0x10
OFF_HPMAX = OFF_STATE + 0x12
OFF_TAIL = OFF_STATE + 0x14

PSTRING_CAP = 255


def _get_pstring(blob: bytes, off: int) -> str:
    n = blob[off]
    return blob[off + 1 : off + 1 + n].decode("cp866")


def _put_pstring(buf: bytearray, off: int, s: str, original: bytes) -> None:
    """Write a shortstring, preserving the original padding bytes.

    Borland does not clear the tail of a shortstring buffer, so the bytes
    past the length are whatever was there before. To round-trip exactly we
    copy the original padding rather than zero-filling.
    """
    raw = s.encode("cp866")
    assert len(raw) <= PSTRING_CAP
    buf[off] = len(raw)
    buf[off + 1 : off + 1 + len(raw)] = raw
    buf[off + 1 + len(raw) : off + 1 + PSTRING_CAP] = original[
        off + 1 + len(raw) : off + 1 + PSTRING_CAP
    ]


def _u16(blob: bytes, off: int) -> int:
    return int.from_bytes(blob[off : off + 2], "little")


def decode(blob: bytes) -> dict:
    if len(blob) != SIZE:
        raise ValueError(f"expected {SIZE} bytes, got {len(blob)}")
    return {
        "magic": _get_pstring(blob, OFF_MAGIC),
        "name": _get_pstring(blob, OFF_NAME),
        "stats": [_u16(blob, OFF_STATE + 2 * i) for i in range(8)],
        "hp": _u16(blob, OFF_HP),
        "hpmax": _u16(blob, OFF_HPMAX),
        "tail": blob[OFF_TAIL:],
        "_raw": blob,
    }


def encode(rec: dict) -> bytes:
    original = rec["_raw"]
    buf = bytearray(original)
    _put_pstring(buf, OFF_MAGIC, rec["magic"], original)
    _put_pstring(buf, OFF_NAME, rec["name"], original)
    for i, v in enumerate(rec["stats"]):
        buf[OFF_STATE + 2 * i : OFF_STATE + 2 * i + 2] = int(v).to_bytes(2, "little")
    buf[OFF_HP : OFF_HP + 2] = int(rec["hp"]).to_bytes(2, "little")
    buf[OFF_HPMAX : OFF_HPMAX + 2] = int(rec["hpmax"]).to_bytes(2, "little")
    buf[OFF_TAIL:] = rec["tail"]
    return bytes(buf)


# Names for the eight stat words at 0x200..0x20f, pinned by Task 9 (see the
# module docstring). `rank_index` is the one entry whose own semantics are
# still not fully pinned down -- it is known to select a name-table row, but
# the class-choice -> value mapping is Task 9b's territory -- so it keeps a
# name that says what it is rather than inventing more than is known.
STAT_NAMES = [
    "rank_index",
    "strength",
    "agility",
    "vitality",
    "luck",
    "level",
    "dmg_min",
    "dmg_max",
]

LAYOUT = {
    "size": SIZE,
    "fields": [
        {"name": "magic", "off": OFF_MAGIC, "kind": "pstring", "len": 256},
        {"name": "name", "off": OFF_NAME, "kind": "pstring", "len": 256},
        *[
            {"name": STAT_NAMES[i], "off": OFF_STATE + 2 * i, "kind": "u16", "len": 2}
            for i in range(8)
        ],
        {"name": "hp", "off": OFF_HP, "kind": "u16", "len": 2},
        {"name": "hpmax", "off": OFF_HPMAX, "kind": "u16", "len": 2},
        {"name": "tail", "off": OFF_TAIL, "kind": "bytes", "len": SIZE - OFF_TAIL},
    ],
}


def main() -> None:
    root = pathlib.Path(__file__).resolve().parent.parent
    (root / "data" / "save_layout.json").write_text(
        json.dumps(LAYOUT, indent=1) + "\n", encoding="utf-8"
    )
    for p in sorted((root / "orig").glob("SAVE_R*.SAV")):
        r = decode(p.read_bytes())
        print(f"{p.name}: name={r['name']!r} hp={r['hp']}/{r['hpmax']} stats={r['stats']}")


if __name__ == "__main__":
    sys.exit(main())
