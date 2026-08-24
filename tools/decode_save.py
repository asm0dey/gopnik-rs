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
                                        Pascal string[2] records; TAIL_FIELDS
                                        below names every one of them. Task 9b
                                        named four regions (buff_countdown,
                                        xp, threshold, growth_log) and left
                                        two `unk_` spans; Task 19 closed
                                        those, so LAYOUT["fields"] now tiles
                                        the whole 694-byte record with no
                                        overlap, no gap and no `unk_` entry.

Field names and offsets 0x200..0x20f are pinned by Task 9: the player's
694-byte record in memory (DS:369c) is byte-identical to the .SAV file, and
that same record is what tools/capture_combat_vectors.py reads via
FIELDS_U16 to build every combat_vectors.json case -- 314 (now 352) blows
matched the original with these fields at these offsets. See
docs/re/combat.md ("The fighter record") and docs/re/save-format.md.

decode()/encode() keep everything past 0x214 as one opaque `tail` blob so
that round-trip is exact with no per-field work; decode_fields() is the
typed view over the same bytes, driven by LAYOUT so it cannot drift from the
artifact. src/save.rs no longer has an opaque tail at all -- it rebuilds
every byte of 0x214..0x2b6 from a named field, which is what makes its
round-trip against the five reference saves an offset check rather than a
copy-through.
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

# `.SAV` offset + RECORD_BASE == the DGROUP address of the same byte.
#
# **Established from flow.** The whole 694-byte record is moved between the
# file and `DS:369c` in one untyped block operation, in both directions:
#
#   1000:6bc6  mov di,0x3c84 / push ds / push di   ; the file variable
#   1000:6bcb  mov ax,0x2b6 / push ax              ; RecSize = 694
#   1000:6bcf  call 0f78:0769                      ; Reset(f, 694)
#   1000:6c01  mov di,0x369c / push ds / push di   ; the buffer
#   1000:6c06  call 0f78:081e                      ; BlockRead  -> DS:369c
#
#   1000:acb5  mov ax,0x2b6 / push ax              ; Rewrite(f, 694)
#   1000:acc3  mov di,0x369c / push ds / push di
#   1000:acc8  call 0f78:0825                      ; BlockWrite <- DS:369c
#
# and the mage's paid save writes the same buffer at 1000:765d. So byte `n`
# of the file IS `20ae:(0x369c + n)`, with no marshalling in between, and
# every DGROUP address this project has mapped inside
# `20ae:369c`..`20ae:3951` names a `.SAV` offset for free. `0x200 + 0x369c`
# = `0x389c`, the class word docs/re/progression.md already pins, and
# `0x2b1 + 0x369c` = `0x394d`, the pistol Task 18 ported: the two
# independently-established landmarks the delta has to reproduce.
RECORD_BASE = 0x369C

# The 162-byte tail (0x214..0x2b6), fully partitioned. Task 19 established
# every byte of the two spans that used to be `unk_0214` and `unk_02ae`:
# they are `20ae:38b0`..`20ae:38cc` and `20ae:394a`..`20ae:3951` under
# RECORD_BASE above, and `FUN_1000_1a03` -- the character sheet Task 16
# mapped -- prints all but one of them with the guard's operand naming the
# address and the label sitting inside the arm that guard selects
# (docs/re/character-sheet.md, "Flag lines"). `evidence` below is that
# guard; `tools/test_decode_save.py` re-derives each one out of `orig/g.exe`
# and checks the instruction there really references `RECORD_BASE + off`.
#
# The 23 flag bytes are Pascal `Boolean`: every direct store to any of them
# image-wide is `mov byte [X],0` or `mov byte [X],1` and there is no third
# value (`python3 tools/re_query.py xrefs-to 20ae:38b0` and friends). The
# five word fields are Pascal `Integer` -- signed 16-bit -- because every
# compare against them is followed by a SIGNED conditional
# (`1000:23da jle`, `1000:23b9 jle`, `1000:2433 jle`, `1000:246f jle`,
# `1000:4e7e jnle`, `1000:1d8f jle`), and the odd offsets they occupy are
# what makes the five `0x214`-span bytes with no reference of their own
# (`20ae:38c4`, `38c6`, `38c8`, `38ca`, `38cc`) the high halves of words
# rather than five more unread flags.
TAIL_FIELDS = [
    # --- 0x214..0x230 == 20ae:38b0..20ae:38cc -------------------------------
    {"name": "broken_jaw", "off": 0x214, "kind": "bool", "len": 1,
     "guest": "20ae:38b0", "evidence": "1000:2037",
     "note": "^4Сломана челюсть; set by a jaw break at 1000:47ee, cleared by the hospital rescue at 1000:5031. Fighter record +0x14."},
    {"name": "broken_leg", "off": 0x215, "kind": "bool", "len": 1,
     "guest": "20ae:38b1", "evidence": "1000:2099",
     "note": "^4Сломана нога; set at 1000:4862, cleared at 1000:5036. Fighter record +0x15."},
    {"name": "armour", "off": 0x216, "kind": "u8", "len": 1,
     "guest": "20ae:38b2", "evidence": "1000:227b",
     "note": "^2Броня #; subtracted from damage at 1000:4769, raised by 1000:81e9/bfa7/c107/c1b1/c1b7/c2f8/c2ff/e8d6. Fighter record +0x16 (Task 11c)."},
    {"name": "dark_glasses", "off": 0x217, "kind": "bool", "len": 1,
     "guest": "20ae:38b3", "evidence": "1000:1cf8",
     "note": "^1У тебя есть тёмные очки; stops the cop encounter at 1000:b7c6."},
    {"name": "suit_abibas", "off": 0x218, "kind": "bool", "len": 1,
     "guest": "20ae:38b4", "evidence": "1000:22a1",
     "note": "^1Костюм Abibas(+1); mar row 4, bought at 1000:bf80."},
    {"name": "boots", "off": 0x219, "kind": "bool", "len": 1,
     "guest": "20ae:38b5", "evidence": "1000:1e81",
     "note": "^1Бутсы(+1); bought at 1000:c029."},
    {"name": "jacket", "off": 0x21A, "kind": "bool", "len": 1,
     "guest": "20ae:38b6", "evidence": "1000:2323",
     "note": "^1Кожанка(+2); mar row 6, bought at 1000:c0e0."},
    {"name": "suit_adidas", "off": 0x21B, "kind": "bool", "len": 1,
     "guest": "20ae:38b7", "evidence": "1000:22fc",
     "note": "^1Костюм Adidas(+2); mar row 7, bought at 1000:c183."},
    {"name": "boots_pontovye", "off": 0x21C, "kind": "bool", "len": 1,
     "guest": "20ae:38b8", "evidence": "1000:1ecf",
     "note": "^1Понтовые бутсы(Урон+2); bought at 1000:c222."},
    {"name": "jacket_krutaya", "off": 0x21D, "kind": "bool", "len": 1,
     "guest": "20ae:38b9", "evidence": "1000:237e",
     "note": "^1Крутая кожанка(+4); mar row 9, bought at 1000:c2ca."},
    {"name": "kastet", "off": 0x21E, "kind": "bool", "len": 1,
     "guest": "20ae:38ba", "evidence": "1000:1eef",
     "note": "^1Кастет(+2); post-kill grant 1000:5541, shop 1000:cb9d."},
    {"name": "mobile", "off": 0x21F, "kind": "bool", "len": 1,
     "guest": "20ae:38bb", "evidence": "1000:1cd8",
     "note": "^1У тебя есть мобильник; gates wander draws 3 and 4 and the in-fight backup call at 1000:4cdb."},
    {"name": "prison_tattoo", "off": 0x220, "kind": "bool", "len": 1,
     "guest": "20ae:38bc", "evidence": "1000:1d18",
     "note": "^1На тебе зоновская наколка; halves the encounter notice roll at 1000:b5da."},
    {"name": "krestik", "off": 0x221, "kind": "bool", "len": 1,
     "guest": "20ae:38bd", "evidence": "1000:1be9",
     "note": "^1Крестик(Удача +2); post-kill one-shot, gate 1000:548c, flag 1000:54b1, delta at 1000:5493 (data/xp.json luck_plus_2)."},
    {"name": "ring_gs", "off": 0x222, "kind": "bool", "len": 1,
     "guest": "20ae:38be", "evidence": "1000:1c09",
     "note": "^1Кольцо \"Гс\"(Удача +1); gate 1000:54bd, flag 1000:54e1, delta 1000:54c4 (luck_plus_1)."},
    {"name": "ring_pg", "off": 0x223, "kind": "bool", "len": 1,
     "guest": "20ae:38bf", "evidence": "1000:1c69",
     "note": "^1Кольцо \"Пг\"(Всё +1); flags at 1000:5362 and 1000:8134, delta 1000:532f (event_1)."},
    {"name": "mega_ring", "off": 0x224, "kind": "bool", "len": 1,
     "guest": "20ae:38c0", "evidence": "1000:1c89",
     "note": "^1Мега Кольцо(Всё +4); flags at 1000:53b2 and 1000:8184, delta 1000:538a (event_2)."},
    {"name": "ring_gp", "off": 0x225, "kind": "bool", "len": 1,
     "guest": "20ae:38c1", "evidence": "1000:1ca9",
     "note": "^1Кольцо \"Гп\"(Самолечение); flags at 1000:53f2 and 1000:81c4. The FIFTH post-kill one-shot: it grants no stat delta, which is why data/xp.json's post_kill_stat_events stops at 548 (0x224)."},
    {"name": "nozh", "off": 0x226, "kind": "bool", "len": 1,
     "guest": "20ae:38c2", "evidence": "1000:1fb5",
     "note": "^1Нож(+6); post-kill grant 1000:5698."},
    {"name": "beer_half_litres", "off": 0x227, "kind": "i16", "len": 2,
     "guest": "20ae:38c3", "evidence": "1000:23d5",
     "note": "Пиво #.#л. -- stored in HALF-litres, printed as [0x38c3] div 2 with a `.5` for the odd half (1000:23e5..1000:2403). Loot add at 1000:5241."},
    {"name": "joints", "off": 0x229, "kind": "i16", "len": 2,
     "guest": "20ae:38c5", "evidence": "1000:23b4",
     "note": "Косяки #; spent at 1000:4b4e and 1000:e9b4, bought at 1000:c90e."},
    {"name": "money", "off": 0x22B, "kind": "i16", "len": 2,
     "guest": "20ae:38c7", "evidence": "1000:242e",
     "note": "Бабки #; every shop row's affordability test is `cmp ax,[0x38c7]` and every purchase is `sub [0x38c7],ax`. 107 references image-wide."},
    {"name": "junk", "off": 0x22D, "kind": "i16", "len": 2,
     "guest": "20ae:38c9", "evidence": "1000:246a",
     "note": "Хлам #; sold to the dealers at 1000:ce87..1000:ce97, loot add at 1000:524f."},
    {"name": "street_cred", "off": 0x22F, "kind": "i16", "len": 2,
     "guest": "20ae:38cb", "evidence": "1000:4e79",
     "note": "понтовость на улице -- NOT the level at 20ae:38a6. Gates the hospital rescue (1000:4fc4, >= 10) and wander draw 2's message (1000:afdc, >= 100)."},
    # `.SAV 0x231`, `DS:38cd`: countdown on the temporary +2 strength / +1
    # dmg_min / +2 dmg_max buff from a smoked joint (1000:4b52 sets it to 3,
    # 1000:e9b8 to 10 from a second grant site; 1000:aeb3 clears it and
    # reverses the buff when it reaches 0). Nonzero means the buff is live
    # and hpmax does not reflect the +2 strength. The character sheet prints
    # it as `^6Обдолбаный  ` (guard 1000:20ca).
    {"name": "buff_countdown", "off": 0x231, "kind": "u8", "len": 1,
     "guest": "20ae:38cd", "evidence": "1000:20ca"},
    # `.SAV 0x232`, `DS:38ce`: XP not yet spent on a level (1000:2536,
    # 1000:254d).
    {"name": "xp", "off": 0x232, "kind": "u16", "len": 2, "guest": "20ae:38ce"},
    # `.SAV 0x234`, `DS:38d0`: XP needed for the next level (1000:2550,
    # 1000:6de0).
    {"name": "threshold", "off": 0x234, "kind": "u16", "len": 2, "guest": "20ae:38d0"},
    # `.SAV 0x236`, `DS:38d2`: `array[1..40] of string[2]`, the two stat
    # codes ('1'..'4') each level granted (1000:2641..1000:267a). 3 bytes
    # per level (a Pascal string[2] length byte plus its two payload bytes),
    # 40 levels. The length byte is NOT always 2: the writer appends one
    # code at a time (1000:267a `rtl_str_assign_max` with max 2) and the
    # flee penalty clears ONLY the length byte at 1000:497d, leaving the two
    # payload bytes behind -- so all three bytes of a slot have to be
    # carried, not just the two codes.
    {"name": "growth_log", "off": 0x236, "kind": "bytes", "len": 40 * 3,
     "guest": "20ae:38d2", "evidence": "1000:2651",
     # Borland's BIASED base: `array[1..40]` is reached as
     # `base - 1*elem + n*elem`, so every instruction that touches the log
     # carries 0x38d2 - 3 = 0x38cf, never 0x38d2 itself. `evidence_operand`
     # is what the instruction actually spells; without it the check below
     # would look for an address no instruction in the image contains.
     "evidence_operand": "20ae:38cf"},
    # --- 0x2ae..0x2b5 == 20ae:394a..20ae:3951 -------------------------------
    {"name": "tooth_guard", "off": 0x2AE, "kind": "bool", "len": 1,
     "guest": "20ae:394a", "evidence": "1000:2068",
     "note": "^1Зубная защита; bought at 1000:e828. Splits a jaw break into the plain arm (1000:47ee) and a Random(4) at 1000:47fe -- a DRAW-COUNT difference, not flavour."},
    {"name": "dubinka", "off": 0x2AF, "kind": "bool", "len": 1,
     "guest": "20ae:394b", "evidence": "1000:1f59",
     "note": "^1Дубинка(+4); post-kill grant 1000:55a7, shop 1000:cc56."},
    {"name": "tesak", "off": 0x2B0, "kind": "bool", "len": 1,
     "guest": "20ae:394c", "evidence": "1000:2003",
     "note": "^1Тесак(Урон+9); post-kill grant 1000:573e."},
    {"name": "pistol", "off": 0x2B1, "kind": "bool", "len": 1,
     "guest": "20ae:394d", "evidence": "1000:1d38",
     "note": "^1У тебя есть пистолет; bmar row 7 hands it over at 1000:cd05. NOT `dealer_order_placed` -- see docs/re/combat-dispatch.md."},
    {"name": "silencer", "off": 0x2B2, "kind": "bool", "len": 1,
     "guest": "20ae:394e", "evidence": "1000:1d6a",
     "note": "^1 с гушителем; bmar row 9, gated on the delivery counter 20ae:3e32 at 1000:ce00."},
    {"name": "cartridges", "off": 0x2B3, "kind": "i16", "len": 2,
     "guest": "20ae:394f", "evidence": "1000:1d8a",
     "note": "^1! патронов - #. A WORD, not a byte: the sheet's guard is `cmp word [0x394f],0` and 20ae:3950 has no reference of its own image-wide. bmar row 7 adds 3 at 1000:cd0a."},
    {"name": "church_stage", "off": 0x2B5, "kind": "u8", "len": 1,
     "guest": "20ae:3951", "evidence": "1000:7c76",
     "note": "The church's sermon stage, 0..2; raised at 1000:7dc7 and 1000:7f5b, read at 1000:7c76/7ceb/7dcb and at 1000:8247 for the parting line."},
]

def decode_fields(blob: bytes) -> dict:
    """Typed view of every LAYOUT field, keyed by name.

    Driven by LAYOUT rather than by a second hand-written offset list, so a
    field added there is decoded here without a matching edit -- the drift
    this project keeps finding between a document and its artifact.
    """
    if len(blob) != SIZE:
        raise ValueError(f"expected {SIZE} bytes, got {len(blob)}")
    out = {}
    for f in LAYOUT["fields"]:
        off, ln, kind = f["off"], f["len"], f["kind"]
        raw = blob[off : off + ln]
        if kind == "pstring":
            out[f["name"]] = raw[1 : 1 + raw[0]].decode("cp866")
        elif kind == "u16":
            out[f["name"]] = int.from_bytes(raw, "little")
        elif kind == "i16":
            out[f["name"]] = int.from_bytes(raw, "little", signed=True)
        elif kind == "u8":
            out[f["name"]] = raw[0]
        elif kind == "bool":
            out[f["name"]] = raw[0]
        else:
            out[f["name"]] = raw
    return out


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
