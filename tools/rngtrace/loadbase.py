"""Derive the guest load base of g.exe from a physical-memory dump.

DOS picks the load segment, so the base is NOT fixed and must never be
hardcoded (tools/qemu/README.md saw 0x224B0 once; that is one run's value).

Address conventions -- the two do not mix:
  * a Ghidra citation `1000:XXXX`      -> image offset XXXX          (Ghidra loaded at seg 0x1000)
  * a real runtime `seg:off`, e.g. `0f78:114b` -> image offset seg*16 + off
  * file offset = 0x18d0 + image offset (the MZ header is 397 paragraphs)

Derivation, and then three independent verifications:
  1. locate a relocation-free byte window of the loaded image in the dump;
  2. require the resulting base to be paragraph aligned and unique;
  3. check EVERY relocation in the code region: memory word must equal
     file word + load_seg -- this is what actually proves the candidate is
     our image loaded at that segment and not a stale copy or a coincidence.
"""
import struct

HEADER_BYTES = 0x18D0          # 397 paragraphs
GHIDRA_BASE_SEG = 0x1000       # Ghidra loaded the image at segment 0x1000
DATA_SEG_IMAGE_OFF = 0x10AE0   # image offset of DS (Ghidra 20ae:0000); above this,
                               # memory diverges from the file at runtime

# Image offsets used by the pre-breakpoint guest verification below.
IMAGE_OFF_RANDOM = 0xF78 * 16 + 0x114B          # 0x108cb -- file 0x1219b
IMAGE_OFF_RANDOMIZE = 0xF78 * 16 + 0x11E0       # 0x10960 -- file 0x12230
IMAGE_OFF_RANDSEED = DATA_SEG_IMAGE_OFF + 0x367E  # 0x1415e -- DS:367e, file 0x15a2e
FILE_OFF_RANDOM = HEADER_BYTES + IMAGE_OFF_RANDOM  # 0x1219b
RANDOM_BYTES = 29                                # entry .. `ca 02 00` inclusive


def file_off_of_image_off(image_off: int) -> int:
    return HEADER_BYTES + image_off


def image_off_of_ghidra(off: int) -> int:
    """`1000:off` -> image offset."""
    return off


def image_off_of_seg_off(seg: int, off: int) -> int:
    """A real runtime `seg:off` (pre-relocation segment) -> image offset."""
    return seg * 16 + off


def parse_relocations(exe: bytes):
    """Return [image_off, ...] for every MZ relocation entry."""
    if exe[:2] not in (b"MZ", b"ZM"):
        raise ValueError("not an MZ image")
    crlc, = struct.unpack_from("<H", exe, 0x06)
    lfarlc, = struct.unpack_from("<H", exe, 0x18)
    hdrpara, = struct.unpack_from("<H", exe, 0x08)
    if hdrpara * 16 != HEADER_BYTES:
        raise ValueError("unexpected header size %d paragraphs" % hdrpara)
    out = []
    for i in range(crlc):
        off, seg = struct.unpack_from("<HH", exe, lfarlc + i * 4)
        out.append(seg * 16 + off)
    return out


def load_image(exe: bytes) -> bytes:
    """The bytes DOS copies into memory (everything after the header)."""
    return exe[HEADER_BYTES:]


def find_anchor(image: bytes, relocs, start_hint: int, length: int = 64) -> int:
    """First image offset >= start_hint whose window touches no relocation."""
    rset = set()
    for r in relocs:
        rset.add(r)
        rset.add(r + 1)
    off = start_hint
    while off + length <= len(image):
        if not any((off + i) in rset for i in range(length)):
            return off
        off += 1
    raise ValueError("no relocation-free window at/after 0x%x" % start_hint)


def _candidates(mem: bytes, needle: bytes):
    out, i = [], mem.find(needle)
    while i != -1:
        out.append(i)
        i = mem.find(needle, i + 1)
    return out


def verify_base(mem: bytes, image: bytes, relocs, base: int) -> dict:
    """Check every code-region relocation against `base`.  Raises on mismatch."""
    if base % 16:
        raise ValueError("base 0x%x is not paragraph aligned" % base)
    load_seg = base // 16
    checked = 0
    for r in relocs:
        if r >= DATA_SEG_IMAGE_OFF:
            continue                      # data segment: mutated at runtime
        want = (int.from_bytes(image[r:r + 2], "little") + load_seg) & 0xFFFF
        got = int.from_bytes(mem[base + r:base + r + 2], "little")
        if got != want:
            raise ValueError(
                "relocation at image 0x%x: memory %04x != file %04x + load_seg %04x"
                % (r, got, want, load_seg))
        checked += 1
    return {"load_seg": load_seg, "relocations_checked": checked,
            "relocations_total": len(relocs)}


def derive(mem: bytes, exe: bytes, hints=(0x2000, 0xC000), window: int = 64) -> dict:
    """Find the unique load base of `exe` inside physical memory `mem`."""
    image = load_image(exe)
    relocs = parse_relocations(exe)
    a1 = find_anchor(image, relocs, hints[0], window)
    a2 = find_anchor(image, relocs, hints[1], window)
    cands = [c - a1 for c in _candidates(mem, image[a1:a1 + window])]
    cands = [b for b in cands if b >= 0 and b % 16 == 0
             and b + len(image) <= len(mem)
             and mem[b + a2:b + a2 + window] == image[a2:a2 + window]]
    good = []
    errors = []
    for b in cands:
        try:
            good.append((b, verify_base(mem, image, relocs, b)))
        except ValueError as e:
            errors.append((b, str(e)))
    if len(good) != 1:
        raise ValueError(
            "expected exactly one load base, got %d (candidates %s, rejected %s)"
            % (len(good), [hex(b) for b, _ in good], [(hex(b), e) for b, e in errors]))
    base, info = good[0]
    info.update({
        "image_base": base,
        "image_base_hex": "0x%X" % base,
        "anchor1_image_off": hex(a1),
        "anchor2_image_off": hex(a2),
        "anchor_window": window,
    })
    return info


def linear(base: int, image_off: int) -> int:
    return base + image_off


class GuestCodeError(RuntimeError):
    pass


def verify_guest_code(mem, exe, base, seed, expected_patch):
    """Check the guest really holds OUR image at OUR base before breaking on it.

    A breakpoint on the wrong address produces a plausible EMPTY trace, which is
    the worst failure mode this harness has -- so this runs before a breakpoint
    is installed, reads the guest's own memory, and refuses on any surprise:

      * the 29 bytes at `base + 0x108cb` must equal the file's at `0x1219b`, and
        must carry the two `36 f7 67 04` (`mul word [ss:bx+4]`) encodings and the
        `ca 02 00` tail -- i.e. be the 32x16 high take, not a neighbour;
      * the seed patch must be present at `base + 0x10960`;
      * `RandSeed` at `base + 0x1415e` must read either 0x00000000 (the image
        value: the patched Randomize has not run yet) or exactly the pinned seed
        (it has run, no draw spent).  Anything else means draws were spent
        BEFORE the attach, so the trace would be missing its head.

    Pure: takes a memory image and returns (checks, randseed).  `expected_patch`
    is passed in rather than computed here so this module stays independent of
    seedpatch; run.py hands it `seedpatch.build_patch(seed)`.
    """
    out = {}
    want = exe[FILE_OFF_RANDOM:FILE_OFF_RANDOM + RANDOM_BYTES]
    got = bytes(mem[base + IMAGE_OFF_RANDOM: base + IMAGE_OFF_RANDOM + RANDOM_BYTES])
    if got != want:
        raise GuestCodeError("guest code at linear 0x%x is %s, expected Random %s"
                             % (base + IMAGE_OFF_RANDOM, got.hex(" "), want.hex(" ")))
    # The instruction encoding itself, re-checked here rather than trusted.
    if got.count(bytes.fromhex("36f76704")) != 2 or got[26:29] != bytes.fromhex("ca0200"):
        raise GuestCodeError("bytes at Random do not look like the 32x16 high take: %s"
                             % got.hex(" "))
    out["random_linear"] = "0x%X" % (base + IMAGE_OFF_RANDOM)
    out["random_bytes"] = got.hex(" ")

    gotp = bytes(mem[base + IMAGE_OFF_RANDOMIZE:
                     base + IMAGE_OFF_RANDOMIZE + len(expected_patch)])
    if gotp != bytes(expected_patch):
        raise GuestCodeError("seed patch is not in guest memory at linear 0x%x: %s"
                             % (base + IMAGE_OFF_RANDOMIZE, gotp.hex(" ")))
    out["randomize_linear"] = "0x%X" % (base + IMAGE_OFF_RANDOMIZE)
    out["randomize_bytes"] = gotp.hex(" ")

    randseed = int.from_bytes(mem[base + IMAGE_OFF_RANDSEED:
                                  base + IMAGE_OFF_RANDSEED + 4], "little")
    out["randseed_at_attach"] = "0x%08X" % randseed
    if randseed == 0:
        out["randseed_state"] = "image value -- patched Randomize has not run yet"
    elif randseed == seed:
        out["randseed_state"] = "pinned seed in place, no draw has been spent yet"
    else:
        raise GuestCodeError(
            "RandSeed is already 0x%08X before the breakpoint could be "
            "installed: draws were spent before the attach, so any trace from "
            "this run would be missing its head." % randseed)
    return out, randseed
