"""Pin RandSeed by patching a COPY of g.exe.

`System.Randomize` (Ghidra 1f78:11e0, file offset 0x12230) seeds RandSeed from
DOS INT 21h/AH=2Ch, so two runs of the original draw different numbers and
nothing is comparable run to run (docs/re/rng.md).  This module rewrites those
13 bytes, in a copy, with an unconditional store of a constant.

    original (13 bytes)                  replacement (13 bytes)
    b4 2c        mov ah,0x2c             c7 06 7e 36 LL LL  mov word [0x367e],lo
    cd 21        int 0x21                c7 06 80 36 HH HH  mov word [0x3680],hi
    89 0e 7e 36  mov [0x367e],cx         cb                 retf
    89 16 80 36  mov [0x3680],dx
    cb           retf

Same length, same entry, same far return, same two destination words, so the
patch cannot shift any other address.  orig/g.exe is never touched.
"""
import hashlib
from pathlib import Path

# file_off = 0x18d0 + seg*16 + off  for a real seg:off.  0f78:11e0 -> 0x12230.
RANDOMIZE_FILE_OFF = 0x12230
RANDOMIZE_ORIG = bytes.fromhex("b42ccd21890e7e3689168036cb")
SEED_LO_ADDR = 0x367E  # DS:367e -- RandSeed low word
SEED_HI_ADDR = 0x3680  # DS:3680 -- RandSeed high word
ORIG_MD5 = "10eb0af07a2d2f5e9da790df7058891c"


def build_patch(seed: int) -> bytes:
    lo = seed & 0xFFFF
    hi = (seed >> 16) & 0xFFFF
    return (
        b"\xc7\x06" + SEED_LO_ADDR.to_bytes(2, "little") + lo.to_bytes(2, "little")
        + b"\xc7\x06" + SEED_HI_ADDR.to_bytes(2, "little") + hi.to_bytes(2, "little")
        + b"\xcb"
    )


def patch_bytes(image: bytes, seed: int) -> tuple:
    """Return (patched image, record).  Refuses if the site does not match."""
    at = RANDOMIZE_FILE_OFF
    found = image[at:at + len(RANDOMIZE_ORIG)]
    if found != RANDOMIZE_ORIG:
        raise ValueError(
            "Randomize is not where it is documented: file 0x%x holds %s, "
            "expected %s" % (at, found.hex(" "), RANDOMIZE_ORIG.hex(" "))
        )
    new = build_patch(seed)
    assert len(new) == len(RANDOMIZE_ORIG), "patch must not change length"
    out = bytearray(image)
    out[at:at + len(new)] = new
    return bytes(out), {
        "file_offset": hex(at),
        "ghidra_address": "1f78:11e0",
        "runtime_seg_off": "0f78:11e0",
        "bytes_before": RANDOMIZE_ORIG.hex(" "),
        "bytes_after": new.hex(" "),
        "length": len(new),
        "seed": seed,
        "seed_hex": "0x%08x" % seed,
    }


def write_patched_copy(src: Path, dst: Path, seed: int) -> dict:
    """Patch src -> dst.  src is never modified."""
    src, dst = Path(src), Path(dst)
    image = src.read_bytes()
    src_md5 = hashlib.md5(image).hexdigest()
    if src_md5 != ORIG_MD5:
        raise ValueError("unexpected source binary md5 %s (want %s)" % (src_md5, ORIG_MD5))
    patched, rec = patch_bytes(image, seed)
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(patched)
    rec["source"] = str(src)
    rec["source_md5"] = src_md5
    rec["patched_copy"] = str(dst)
    rec["patched_md5"] = hashlib.md5(patched).hexdigest()
    return rec
