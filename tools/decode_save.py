#!/usr/bin/env python3
"""Decode and re-encode GOPNIK .SAV files.

Layout (694 bytes total), established from the five reference saves:

    0x000  string[255]  magic  -- version banner, constant
    0x100  string[255]  name   -- player name, colour-prefixed
    0x200  u16 x 8             -- stat block, semantics TBD (Task 9)
    0x210  u16          hp
    0x212  u16          hpmax
    0x214  ...                 -- flags, counters, and a run of
                                  Pascal string[2] records; not yet
                                  segmented, preserved verbatim.

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


LAYOUT = {
    "size": SIZE,
    "fields": [
        {"name": "magic", "off": OFF_MAGIC, "kind": "pstring", "len": 256},
        {"name": "name", "off": OFF_NAME, "kind": "pstring", "len": 256},
        *[
            {"name": f"unk_stat{i}", "off": OFF_STATE + 2 * i, "kind": "u16", "len": 2}
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
