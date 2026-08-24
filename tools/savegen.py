#!/usr/bin/env python3
"""Synthesise a valid `.SAV` record with chosen field values.

    python3 tools/savegen.py --base orig/SAVE_R3.SAV --out /tmp/probe.SAV \\
        --set level=6 --set money=5000 --set mobile=1

**The question this answers:** *how do I put the original into a chosen state
without grinding for it?*  `orig/g.exe` reads its whole 694-byte character
record straight off disk into `DS:369c` (`docs/re/save-format.md`, "The
record IS guest memory"), so a `.SAV` file is a direct write into guest
memory for every variable the record covers -- the class, the level, the
stats, every item flag, the money, the street cred, the buff countdown and
the growth log.

**331 of the game's 838 conditional branches (39.5%) have a guard that reads
one of those bytes** -- `entry` 160 of 406, `FUN_1000_1a03` 77 of 83,
`FUN_1000_3d11` 65 of 224.  That is what `python3 tools/branch_reach.py`
prints; the method is one sentence in its docstring and the number is
whatever the script says, never a figure written down beside it.  An earlier
revision of this docstring claimed **355 / 42%**, from a window based at
`0x389c` -- the record base plus the `0x200` the stat words sit at *inside*
the record.  That shifted window counts 26 branches reading the ENEMY record
at `DS:3952` and the wander bucket at `20ae:3971`, neither of which is in a
`.SAV` file.  `--window stat-block-base` reproduces the wrong figure on
demand, and `tools/test_branch_reach.py` pins both.

Read the number as an upper bound on reach-by-save, not as coverage: it says
the guard reads a byte a save writes, not that the branch is satisfiable or
that a player could reach the state.

`tools/rngtrace/saveprobe.py` is the harness that loads what this writes into
the real executable under qemu and reports what the guest actually holds.

**What it can and cannot claim.**  A synthesised record can construct states
no real playthrough produces -- a level-1 character with a тесак, say.  So:
what the code DOES in a forced state and whether a player can REACH that
state are different claims, and anything observed from a forced state must
say it was forced (`docs/superpowers/RESUME.md`, "Reaching states without
grinding").

**Everything not named is known-good, not invented.**  The output starts from
a real save's bytes and only the named slices are overwritten -- the same
discipline `decode_save.encode` uses, and the reason the result is a valid
record rather than a plausible-looking one.  `--base` therefore has to be a
real 694-byte save; there is no from-nothing mode here, because a record
built from zeroes would carry a zero `magic` banner and a zero class, and
neither is a state the original can produce.

Field names are read from `decode_save.LAYOUT`, which is the same table
`data/save_layout.json` ships, so a caller names `street_cred` rather than
`0x22f` and a field renamed there is renamed here with no second edit.
`--set-byte` is the escape hatch for byte-level probing, where the point is
precisely to write a value no named field would allow.

Standard library only.
"""
import argparse
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import decode_save                                                   # noqa: E402
from decode_save import LAYOUT, PSTRING_CAP, SIZE                    # noqa: E402

#: Field kinds `--set` understands, and the value range each accepts.
_INT_KINDS = {
    "u8": (0, 0xFF, False),
    "bool": (0, 1, False),
    "u16": (0, 0xFFFF, False),
    "i16": (-0x8000, 0x7FFF, True),
}

FIELDS = {f["name"]: f for f in LAYOUT["fields"]}


class SaveGenError(ValueError):
    pass


def field(name):
    try:
        return FIELDS[name]
    except KeyError:
        raise SaveGenError(
            "no field named %r; known fields: %s"
            % (name, ", ".join(sorted(FIELDS)))
        ) from None


def encode_field(f, value) -> bytes:
    """The bytes `value` occupies in the record, per the field's kind."""
    kind = f["kind"]
    if kind == "pstring":
        raw = value.encode("cp866") if isinstance(value, str) else bytes(value)
        if len(raw) > PSTRING_CAP:
            raise SaveGenError(
                "%s: %d bytes exceeds the %d-byte shortstring cap"
                % (f["name"], len(raw), PSTRING_CAP))
        # Only the length byte and the payload; the padding past it belongs
        # to the base save and is left alone by write_field below.
        return bytes([len(raw)]) + raw
    if kind == "bytes":
        raw = bytes(value)
        if len(raw) != f["len"]:
            raise SaveGenError(
                "%s: expected exactly %d bytes, got %d"
                % (f["name"], f["len"], len(raw)))
        return raw
    lo, hi, signed = _INT_KINDS[kind]
    v = int(value)
    if not lo <= v <= hi:
        raise SaveGenError(
            "%s is %s, so %d is out of range [%d, %d]"
            % (f["name"], kind, v, lo, hi))
    return v.to_bytes(f["len"], "little", signed=signed)


def write_field(buf: bytearray, name: str, value) -> None:
    f = field(name)
    raw = encode_field(f, value)
    buf[f["off"] : f["off"] + len(raw)] = raw


def synthesise(base: bytes, fields=None, raw_bytes=None) -> bytes:
    """A 694-byte record: `base` with `fields` and `raw_bytes` applied.

    `fields` maps a `decode_save.LAYOUT` field name to a value; `raw_bytes`
    maps a record offset to a single byte, applied AFTER the named fields so
    a probe can override one byte of a field it otherwise wants intact.
    """
    if len(base) != SIZE:
        raise SaveGenError("base is %d bytes, expected %d" % (len(base), SIZE))
    buf = bytearray(base)
    for name, value in (fields or {}).items():
        write_field(buf, name, value)
    for off, value in (raw_bytes or {}).items():
        off = int(off)
        if not 0 <= off < SIZE:
            raise SaveGenError("offset 0x%x is outside the record" % off)
        if not 0 <= int(value) <= 0xFF:
            raise SaveGenError("0x%x: %r is not a byte" % (off, value))
        buf[off] = int(value)
    return bytes(buf)


def sentinel_bytes(spans) -> dict:
    """One distinct, non-Boolean value per offset across `spans`.

    A probe that writes 0/1 into flag bytes cannot tell "the record landed at
    `DS:369c`" apart from "the guest happened to hold 0/1 there anyway"; a run
    of distinct values that occurs nowhere else can. Values start at 0x40 and
    step by one, which keeps every byte printable-ish, away from 0/1, and
    unique across the whole probe.
    """
    out, v = {}, 0x40
    for lo, hi in spans:
        for off in range(lo, hi):
            out[off] = v
            v += 1
            if v > 0xFE:
                raise SaveGenError("more offsets than distinct sentinels")
    return out


def _parse_set(text):
    if "=" not in text:
        raise SaveGenError("--set wants NAME=VALUE, got %r" % text)
    name, value = text.split("=", 1)
    f = field(name.strip())
    if f["kind"] == "pstring":
        return name.strip(), value
    if f["kind"] == "bytes":
        return name.strip(), bytes.fromhex(value)
    return name.strip(), int(value, 0)


def _parse_set_byte(text):
    if "=" not in text:
        raise SaveGenError("--set-byte wants OFFSET=VALUE, got %r" % text)
    off, value = text.split("=", 1)
    return int(off, 0), int(value, 0)


def main(argv=None):
    ap = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--base", required=True,
                    help="a real 694-byte .SAV to start from; every byte not "
                         "named below is carried over from it unchanged")
    ap.add_argument("--out", required=True, help="where to write the record")
    ap.add_argument("--set", action="append", default=[], metavar="NAME=VALUE",
                    help="set a named field (see data/save_layout.json); "
                         "integers accept 0x prefixes, `bytes` fields take hex")
    ap.add_argument("--set-byte", action="append", default=[],
                    metavar="OFFSET=VALUE",
                    help="set one raw record byte, applied after --set")
    ap.add_argument("--list-fields", action="store_true",
                    help="print every settable field with its offset and kind")
    args = ap.parse_args(argv)

    if args.list_fields:
        for f in LAYOUT["fields"]:
            print("0x%03x  %-6s %-18s %s"
                  % (f["off"], f["kind"], f["name"], f.get("guest", "")))
        return 0

    base = pathlib.Path(args.base).read_bytes()
    fields = dict(_parse_set(s) for s in args.set)
    raw = dict(_parse_set_byte(s) for s in args.set_byte)
    out = synthesise(base, fields, raw)

    # A synthesised record must survive the reference decoder unchanged, or
    # it is not a record -- it is a blob that happens to be 694 bytes.
    rec = decode_save.decode(out)
    if decode_save.encode(rec) != out:
        raise SaveGenError("the synthesised record does not round-trip")

    dest = pathlib.Path(args.out)
    if dest.resolve().parent == (pathlib.Path(__file__).resolve().parents[1]
                                 / "orig"):
        raise SaveGenError(
            "refusing to write into orig/: the five shipped saves and "
            "PLACES.SAV are frozen ground truth (tools/mutate.py guards them)")
    dest.write_bytes(out)
    print("%s: %d bytes, %d named field(s), %d raw byte(s)"
          % (dest, len(out), len(fields), len(raw)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
