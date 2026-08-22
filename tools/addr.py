#!/usr/bin/env python3
"""The address convention for `orig/g.exe`, defined once, in code.

`docs/re/METHODOLOGY.md`, section "Address convention, and its range of
validity", is the human-readable authority for this rule.  This module is its
executable form and the only place the arithmetic is written in Python: every
other tool imports from here rather than restating it.

Two citation forms are in use across `docs/re/`, `src/` and the task briefs,
and they are **not the same arithmetic**:

  Form A -- a Ghidra label `SEG:OFF`.  Ghidra loaded the image at segment
      0x1000, so the relative segment is `SEG - GHIDRA_BASE_SEG`::

          image_off = (SEG - 0x1000) * 16 + OFF        # only for SEG >= 0x1000

  Form B -- a real runtime `seg:off`, i.e. what `ndisasm` prints for a far-call
      operand (`0eed:`, `0f16:`, `0f78:`).  The operand already IS the relative
      segment, so there is no `- 0x1000`::

          image_off = SEG * 16 + OFF                   # only for SEG < 0x1000

  and in both cases::

          file_off  = header_bytes(exe) + image_off

Applying Form A to a Form B address undershoots by 64 KiB; applying Form B to a
Form A address overshoots by the same.  That defect has already produced one
wrong adjudication on this project, so each form here is a separate function
that **rejects the other form's segment range** rather than returning a
plausible wrong number, and `citation()` -- which picks the form from the
segment value -- is the only place the choice is ever made.

`HEADER_BYTES` is derived from the MZ header's `e_cparhdr` field, never
written down as a literal; `check_image()` asserts that the derivation lands on
`EXPECTED_HEADER_BYTES` for this image.

Landmarks, checked by `tools/test_addr.py` against the bytes in `orig/g.exe`:

    1000:b353  -> file 0xcc23, holding `9a 4b 11 78 0f` (a `Random` call)
    0f78:114b  -> file 0x1219b, the Borland `Random` entry

Standard library only.
"""
import re
import struct
from pathlib import Path

__all__ = [
    "AddressError", "Citation",
    "GHIDRA_BASE_SEG", "DATA_SEG_GHIDRA", "DATA_SEG_IMAGE_OFF",
    "EXPECTED_HEADER_BYTES", "HEADER_BYTES", "RANDOM_CALL_BYTES",
    "REPO", "EXE_PATH",
    "header_bytes", "check_image", "read_exe",
    "file_off_of_image_off", "image_off_of_file_off",
    "image_off_of_ghidra", "image_off_of_seg_off",
    "parse_citation", "citation", "image_off_of_citation",
    "file_off_of_citation", "ghidra_label",
    "data_off_of_image_off", "image_off_of_data_off", "is_data_image_off",
    "relocation_entries", "relocation_image_offs", "relocation_segments",
    "parse_relocations", "load_image",
]


class AddressError(ValueError):
    """A citation outside the range of validity of the form it was written in."""


# --- the two fixed points of the convention ---------------------------------

GHIDRA_BASE_SEG = 0x1000        # Ghidra loaded g.exe at segment 0x1000
DATA_SEG_GHIDRA = 0x20AE        # DGROUP, as Ghidra labels it (`20ae:xxxx`)

EXPECTED_HEADER_BYTES = 0x18D0  # the landmark this image must derive to
RANDOM_CALL_BYTES = bytes.fromhex("9a4b11780f")  # `call far 0f78:114b`

REPO = Path(__file__).resolve().parent.parent
EXE_PATH = REPO / "orig" / "g.exe"


def header_bytes(exe: bytes) -> int:
    """Size of the MZ header in bytes, from `e_cparhdr` -- not a literal."""
    if exe[:2] not in (b"MZ", b"ZM"):
        raise AddressError("not an MZ image")
    hdrpara, = struct.unpack_from("<H", exe, 0x08)
    if hdrpara == 0:
        raise AddressError("MZ header claims 0 paragraphs")
    return hdrpara * 16


def read_exe(path=None) -> bytes:
    return Path(path or EXE_PATH).read_bytes()


def _derive_header_bytes() -> int:
    with open(EXE_PATH, "rb") as fh:
        head = fh.read(0x40)
    return header_bytes(head)


#: Derived from `orig/g.exe`'s own MZ header at import time.
HEADER_BYTES = _derive_header_bytes()


def check_image(exe=None) -> dict:
    """Re-derive the header size and confirm the landmarks, from the bytes.

    Returns the evidence rather than a bare bool, so a caller can print it.
    Raises `AddressError` on any mismatch.
    """
    exe = read_exe() if exe is None else exe
    hb = header_bytes(exe)
    if hb != EXPECTED_HEADER_BYTES:
        raise AddressError(
            "header is %d paragraphs (%#x bytes), expected %#x"
            % (hb // 16, hb, EXPECTED_HEADER_BYTES))
    ev = {"header_paragraphs": hb // 16, "header_bytes": hb}
    for text, want in (("1000:b353", RANDOM_CALL_BYTES),
                       ("0f78:114b", bytes.fromhex("e85a008bdc"))):
        fo = file_off_of_citation(text)
        got = exe[fo:fo + len(want)]
        if got != want:
            raise AddressError("landmark %s -> file %#x holds %s, expected %s"
                               % (text, fo, got.hex(" "), want.hex(" ")))
        ev[text] = {"file_off": fo, "bytes": got.hex(" ")}
    return ev


# --- image / file offsets ----------------------------------------------------

def file_off_of_image_off(image_off: int) -> int:
    """Image offset -> offset in the `orig/g.exe` FILE."""
    if image_off < 0:
        raise AddressError("negative image offset %d" % image_off)
    return HEADER_BYTES + image_off


def image_off_of_file_off(file_off: int) -> int:
    """Offset in the FILE -> image offset.  Raises inside the MZ header."""
    if file_off < HEADER_BYTES:
        raise AddressError(
            "file offset %#x is inside the %#x-byte MZ header, so it has no "
            "image offset" % (file_off, HEADER_BYTES))
    return file_off - HEADER_BYTES


# --- the two forms, each with its range of validity enforced -----------------

def image_off_of_ghidra(seg: int, off: int) -> int:
    """Form A: a Ghidra label `SEG:OFF` -> image offset.  Requires SEG >= 0x1000.

    Rejecting SEG < 0x1000 is the point: `image_off_of_ghidra(0x0f78, 0x114b)`
    is the 64 KiB undershoot, and it raises instead of answering `0x8cb`.
    """
    _check_off(off, "1000:XXXX")
    if seg < GHIDRA_BASE_SEG:
        raise AddressError(
            "%04x:%04x: segment %#x is below the Ghidra base segment %#x, so "
            "it is a real runtime segment (Form B) -- use image_off_of_seg_off. "
            "Applying the Ghidra form here undershoots by %d bytes."
            % (seg, off, seg, GHIDRA_BASE_SEG, GHIDRA_BASE_SEG * 16))
    if seg > 0xFFFF:
        raise AddressError("segment %#x does not fit in 16 bits" % seg)
    return (seg - GHIDRA_BASE_SEG) * 16 + off


def image_off_of_seg_off(seg: int, off: int) -> int:
    """Form B: a real runtime `seg:off` -> image offset.  Requires SEG < 0x1000.

    Rejecting SEG >= 0x1000 is the mirror image of the check above: the same
    address written as a Ghidra label (`1f78:114b`) must not be fed through the
    runtime form, which would overshoot by 64 KiB.
    """
    _check_off(off, "seg:off")
    if seg < 0:
        raise AddressError("negative segment %d" % seg)
    if seg >= GHIDRA_BASE_SEG:
        raise AddressError(
            "%04x:%04x: segment %#x is at or above the Ghidra base segment "
            "%#x, so it is a Ghidra label (Form A) -- use image_off_of_ghidra. "
            "Applying the runtime form here overshoots by %d bytes."
            % (seg, off, seg, GHIDRA_BASE_SEG, GHIDRA_BASE_SEG * 16))
    return seg * 16 + off


def _check_off(off: int, what: str) -> None:
    if not 0 <= off <= 0xFFFF:
        raise AddressError("%s: offset %#x does not fit in 16 bits" % (what, off))


# --- citations ---------------------------------------------------------------

_CITATION = re.compile(r"^\s*([0-9A-Fa-f]{1,4})\s*:\s*([0-9A-Fa-f]{1,4})\s*$")


class Citation:
    """A parsed `seg:off`, carrying which form it is and what it resolves to."""

    __slots__ = ("text", "seg", "off", "form", "image_off")

    def __init__(self, text: str, seg: int, off: int, form: str, image_off: int):
        self.text, self.seg, self.off = text, seg, off
        self.form, self.image_off = form, image_off

    @property
    def file_off(self) -> int:
        return file_off_of_image_off(self.image_off)

    @property
    def ghidra_label(self) -> str:
        return ghidra_label(self.seg, self.off)

    def __repr__(self):
        return ("Citation(%s, form=%s, image=%#x, file=%#x)"
                % (self.text, self.form, self.image_off, self.file_off))


def parse_citation(text: str):
    """`"1000:b353"` -> `(seg, off)`.  Hex, no `0x`, exactly one colon."""
    m = _CITATION.match(text)
    if not m:
        raise AddressError(
            "%r is not a seg:off citation (expected e.g. 1000:b353 or 0f78:114b)"
            % (text,))
    return int(m.group(1), 16), int(m.group(2), 16)


def citation(text: str) -> Citation:
    """Resolve a citation in EITHER form.  The segment picks the form.

    This is the only place the choice between Form A and Form B is made, which
    is what makes the 64 KiB mistake unrepresentable for callers: they never
    get to apply the wrong one.
    """
    seg, off = parse_citation(text)
    if seg >= GHIDRA_BASE_SEG:
        return Citation(text, seg, off, "ghidra", image_off_of_ghidra(seg, off))
    return Citation(text, seg, off, "runtime", image_off_of_seg_off(seg, off))


def image_off_of_citation(text: str) -> int:
    return citation(text).image_off


def file_off_of_citation(text: str) -> int:
    return citation(text).file_off


def ghidra_label(seg: int, off: int) -> str:
    """The Ghidra label for a citation in either form (`0f78:114b` -> `1f78:114b`)."""
    if seg < GHIDRA_BASE_SEG:
        seg += GHIDRA_BASE_SEG
    return "%04x:%04x" % (seg, off)


# --- the data segment --------------------------------------------------------

#: Image offset of DGROUP (`20ae:0000`).  Derived from the Ghidra form, not a
#: literal: above this the runtime mutates memory, so the file stops matching.
DATA_SEG_IMAGE_OFF = image_off_of_ghidra(DATA_SEG_GHIDRA, 0)


def is_data_image_off(image_off: int) -> bool:
    return image_off >= DATA_SEG_IMAGE_OFF


def data_off_of_image_off(image_off: int) -> int:
    """Image offset -> `20ae:` offset (the value a DS operand carries)."""
    if not is_data_image_off(image_off):
        raise AddressError("image offset %#x is below DGROUP (%#x)"
                           % (image_off, DATA_SEG_IMAGE_OFF))
    return image_off - DATA_SEG_IMAGE_OFF


def image_off_of_data_off(data_off: int) -> int:
    """A `20ae:` offset -> image offset."""
    return image_off_of_ghidra(DATA_SEG_GHIDRA, data_off)


# --- relocations -------------------------------------------------------------

def relocation_entries(exe: bytes):
    """`[(seg, off, image_off), ...]` for every MZ relocation entry."""
    if exe[:2] not in (b"MZ", b"ZM"):
        raise AddressError("not an MZ image")
    crlc, = struct.unpack_from("<H", exe, 0x06)
    lfarlc, = struct.unpack_from("<H", exe, 0x18)
    if header_bytes(exe) != HEADER_BYTES:
        raise AddressError("unexpected header size %d paragraphs"
                           % (header_bytes(exe) // 16))
    out = []
    for i in range(crlc):
        off, seg = struct.unpack_from("<HH", exe, lfarlc + i * 4)
        out.append((seg, off, seg * 16 + off))
    return out


def relocation_image_offs(exe: bytes):
    """`[image_off, ...]` for every MZ relocation entry."""
    return [io for _, _, io in relocation_entries(exe)]


def relocation_segments(exe: bytes):
    """The distinct relative segments the relocation table names, sorted.

    For `orig/g.exe` this is `(0x0, 0xeed, 0xf16, 0xf78)` -- every one of them
    below `GHIDRA_BASE_SEG`, i.e. exactly the domain of the runtime form.
    """
    return tuple(sorted({seg for seg, _, _ in relocation_entries(exe)}))


#: Historical name kept so `tools/rngtrace` and its tests keep working.
parse_relocations = relocation_image_offs


def load_image(exe: bytes) -> bytes:
    """The bytes DOS copies into memory (everything after the MZ header)."""
    return exe[HEADER_BYTES:]


if __name__ == "__main__":  # pragma: no cover - a convenience, not the CLI
    import json
    print(json.dumps(check_image(), indent=2))
