#!/usr/bin/env python3
"""Decode and re-encode GOPNIK .SAV files.

Layout (694 bytes total), established from the five reference saves:

    0x000  string[255]  magic       -- version banner, constant
    0x100  string[255]  name        -- player name, colour-prefixed
    0x200  u16          rank_index  -- indexes the DS:002e name table and the
                                       DS:0002 growth-weight table; the
                                       class-choice -> value mapping is
                                       answer + 3 (Task 9b, 1000:71b8)
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
                                        Pascal string[2] records. Task 9b
                                        fix wave 1 named four regions of it
                                        (buff_countdown, xp, threshold,
                                        growth_log below); fix wave 2
                                        partitioned the remaining bytes into
                                        their own unk_<hex_offset> entries so
                                        that LAYOUT["fields"] tiles the whole
                                        694-byte record with no overlap and
                                        no gap (see TAIL_FIELDS below).

Field names and offsets 0x200..0x20f are pinned by Task 9: the player's
694-byte record in memory (DS:369c) is byte-identical to the .SAV file, and
that same record is what tools/capture_combat_vectors.py reads via
FIELDS_U16 to build every combat_vectors.json case -- 314 (now 352) blows
matched the original with these fields at these offsets. See
docs/re/combat.md ("The fighter record") and docs/re/save-format.md.

Everything past 0x214 is carried as opaque `tail` bytes in decode()/encode()
below (and in src/save.rs's `Save::tail`) so that round-trip is exact --
that in-memory representation is intentionally coarser than the layout
artifact, which documents each byte of it individually as it becomes
established, per-region unk_<hex_offset> or a real name.
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
# module docstring). `rank_index`'s own semantics -- which class-prompt
# answer maps to which stored value -- were closed by Task 9b
# (docs/re/progression.md): the stored word is the prompt's answer plus 3,
# and it indexes both the DS:002e rank-name table and the DS:0002
# growth-weight table.
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

# The 162-byte tail (0x214..0x2b6), fully partitioned: the four regions
# named by Task 9b's fix wave 1 (docs/re/progression.md,
# docs/re/save-format.md) at their real offsets, plus every remaining
# unestablished span named unk_<hex_offset> per the project convention
# ("Unknown means unknown" -- a field whose meaning is not confirmed stays
# named unk_<hex_offset> and its bytes are preserved, never guessed into a
# real name just to make the table look finished). Fix wave 1 had added the
# four named regions *alongside* a still-present, now-overlapping opaque
# `tail` entry; fix wave 2 replaced that with this partition so that
# LAYOUT["fields"] tiles the full record with no gaps and no overlaps --
# tests/save_roundtrip.rs::save_layout_json_fields_tile_the_record enforces
# this structurally.
#
# `unk_0214` (0x214..0x231, 29 bytes) also contains the four one-shot
# post-kill event flags at 0x221-0x225 (Task 9b), which are intentionally
# *not* named here -- they already live in data/xp.json's
# `post_kill_stat_events[*].flag_save_offset`, and this file only names a
# span once, so from save_layout.json's point of view those bytes are still
# part of the unnamed run.
TAIL_FIELDS = [
    # 0x214..0x231: unestablished (includes the post-kill flags -- see above).
    {"name": "unk_0214", "off": 0x214, "kind": "bytes", "len": 0x231 - 0x214},
    # `.SAV 0x231`, `DS:38cd`: countdown on the temporary +2 strength / +1
    # dmg_min / +2 dmg_max buff from a smoked joint (1000:4b52 sets it to 3,
    # 1000:e9b8 to 10 from a second grant site; 1000:aeb3 clears it and
    # reverses the buff when it reaches 0). Nonzero means the buff is live
    # and hpmax does not reflect the +2 strength.
    {"name": "buff_countdown", "off": 0x231, "kind": "u8", "len": 1},
    # `.SAV 0x232`, `DS:38ce`: XP not yet spent on a level (1000:2536,
    # 1000:254d).
    {"name": "xp", "off": 0x232, "kind": "u16", "len": 2},
    # `.SAV 0x234`, `DS:38d0`: XP needed for the next level (1000:2550,
    # 1000:6de0).
    {"name": "threshold", "off": 0x234, "kind": "u16", "len": 2},
    # `.SAV 0x236`, `DS:38d2`: `array[1..40] of string[2]`, the two stat
    # codes ('1'..'4') each level granted (1000:2641..1000:267a). 3 bytes
    # per level (a Pascal string[2] length byte plus its two payload bytes),
    # 40 levels.
    {"name": "growth_log", "off": 0x236, "kind": "bytes", "len": 40 * 3},
    # 0x2ae..0x2b6: unestablished, runs to end of record.
    {"name": "unk_02ae", "off": 0x236 + 40 * 3, "kind": "bytes", "len": SIZE - (0x236 + 40 * 3)},
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
        *TAIL_FIELDS,
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
