#!/usr/bin/env python3
"""Generate data/rng_vectors.json by interpreting g.exe's own RNG instructions.

This is the ground-truth source for tests/rng_vectors.rs. It must stay
INDEPENDENT of src/rng.rs: nothing here reads, imports, or mirrors the Rust
implementation. The constants, the shift amounts, the increment and the
seed-variable addresses are all decoded out of `orig/g.exe` at runtime by a
tiny 16-bit x86 interpreter, so the numbers come from the original binary's
bytes rather than from anybody's transcription of them.

Routines executed (Ghidra addresses, see docs/re/rng.md):
  1f78:11a8  @Rand         RandSeed := RandSeed * $08088405 + 1
  1f78:114b  Random(Word)  result := (RandSeed * n) shr 32

Standard library only. Run:  python3 tools/gen_rng_vectors.py
"""

import json
import pathlib
import struct
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
EXE = REPO / "orig" / "g.exe"
OUT = REPO / "data" / "rng_vectors.json"

# Ghidra loaded g.exe with the image starting at segment 0x1000, so
# file_offset = header_size + (seg - 0x1000) * 16 + off.
IMAGE_SEG = 0x1000
RAND_SEG = 0x1F78
RAND = 0x11A8  # @Rand
RANDOM = 0x114B  # Random(Word)

# Documented in docs/re/rng.md; asserted against the image below.
RANDSEED_OFF = 0x367E


def load_image():
    data = EXE.read_bytes()
    hdr_paragraphs = struct.unpack_from("<H", data, 8)[0]
    return data, hdr_paragraphs * 16


class Cpu:
    """Just enough 8086 to run @Rand and Random(Word) out of the real image."""

    def __init__(self, code, base):
        self.code = code  # bytes of the whole file
        self.base = base  # file offset of RAND_SEG:0000
        self.r = {"ax": 0, "bx": 0, "cx": 0, "dx": 0}
        self.mem = {}  # DS words, keyed by offset
        self.arg = 0  # the single word pushed by the caller (ss:bx+4)
        self.cf = 0

    # --- helpers -------------------------------------------------------
    def word(self, off):
        return struct.unpack_from("<H", self.code, self.base + off)[0]

    def byte(self, off):
        return self.code[self.base + off]

    def get_hi(self, reg):
        return (self.r[reg] >> 8) & 0xFF

    def get_lo(self, reg):
        return self.r[reg] & 0xFF

    def set_hi(self, reg, v):
        self.r[reg] = ((v & 0xFF) << 8) | (self.r[reg] & 0xFF)

    def set_lo(self, reg, v):
        self.r[reg] = (self.r[reg] & 0xFF00) | (v & 0xFF)

    def add16(self, reg, v):
        t = self.r[reg] + v
        self.cf = 1 if t > 0xFFFF else 0
        self.r[reg] = t & 0xFFFF

    def mul16(self, src):
        p = self.r["ax"] * src
        self.r["ax"] = p & 0xFFFF
        self.r["dx"] = (p >> 16) & 0xFFFF

    # --- fetch/execute -------------------------------------------------
    def run(self, ip, depth=0):
        if depth > 4:
            raise RuntimeError("call depth exceeded")
        while True:
            op = self.byte(ip)
            if op == 0xA1:  # mov ax,[iw]
                self.r["ax"] = self.mem.get(self.word(ip + 1), 0)
                ip += 3
            elif op == 0xA3:  # mov [iw],ax
                self.mem[self.word(ip + 1)] = self.r["ax"]
                ip += 3
            elif op == 0x8B and self.byte(ip + 1) == 0x1E:  # mov bx,[iw]
                self.r["bx"] = self.mem.get(self.word(ip + 2), 0)
                ip += 4
            elif op == 0x89 and self.byte(ip + 1) == 0x16:  # mov [iw],dx
                self.mem[self.word(ip + 2)] = self.r["dx"]
                ip += 4
            elif op == 0x8B and self.byte(ip + 1) == 0xC8:  # mov cx,ax
                self.r["cx"] = self.r["ax"]
                ip += 2
            elif op == 0x8B and self.byte(ip + 1) == 0xCA:  # mov cx,dx
                self.r["cx"] = self.r["dx"]
                ip += 2
            elif op == 0x8B and self.byte(ip + 1) == 0xC1:  # mov ax,cx
                self.r["ax"] = self.r["cx"]
                ip += 2
            elif op == 0x8B and self.byte(ip + 1) == 0xC2:  # mov ax,dx
                self.r["ax"] = self.r["dx"]
                ip += 2
            elif op == 0x8B and self.byte(ip + 1) == 0xDC:  # mov bx,sp
                ip += 2  # only used to address the pushed argument
            elif op == 0x2E and self.code[self.base + ip + 1 : self.base + ip + 3] == b"\xf7\x26":
                # mul word [cs:iw] -- the multiplier constant lives in CODE_5
                self.mul16(self.word(self.word(ip + 3)))
                ip += 5
            elif op == 0x36 and self.code[self.base + ip + 1 : self.base + ip + 4] == b"\xf7\x67\x04":
                self.mul16(self.arg)  # mul word [ss:bx+4]
                ip += 4
            elif op == 0xD1 and self.byte(ip + 1) in (0xE1, 0xE3):  # shl cx/bx,1
                reg = "cx" if self.byte(ip + 1) == 0xE1 else "bx"
                self.r[reg] = (self.r[reg] << 1) & 0xFFFF
                ip += 2
            elif op == 0xD3 and self.byte(ip + 1) == 0xE3:  # shl bx,cl
                self.r["bx"] = (self.r["bx"] << (self.get_lo("cx") & 0x1F)) & 0xFFFF
                ip += 2
            elif op == 0x02 and self.byte(ip + 1) == 0xE9:  # add ch,cl
                self.set_hi("cx", self.get_hi("cx") + self.get_lo("cx"))
                ip += 2
            elif op == 0x02 and self.byte(ip + 1) == 0xF3:  # add dh,bl
                self.set_hi("dx", self.get_hi("dx") + self.get_lo("bx"))
                ip += 2
            elif op == 0x03 and self.byte(ip + 1) == 0xD1:  # add dx,cx
                self.add16("dx", self.r["cx"])
                ip += 2
            elif op == 0x03 and self.byte(ip + 1) == 0xD3:  # add dx,bx
                self.add16("dx", self.r["bx"])
                ip += 2
            elif op == 0x03 and self.byte(ip + 1) == 0xC1:  # add ax,cx
                self.add16("ax", self.r["cx"])
                ip += 2
            elif op == 0x05:  # add ax,iw
                self.add16("ax", self.word(ip + 1))
                ip += 3
            elif op == 0x83 and self.byte(ip + 1) == 0xD2:  # adc dx,imm8
                imm = self.byte(ip + 2)
                imm = imm - 0x100 if imm > 0x7F else imm
                self.add16("dx", (imm + self.cf) & 0xFFFF)
                ip += 3
            elif op == 0xB1:  # mov cl,imm8
                self.set_lo("cx", self.byte(ip + 1))
                ip += 2
            elif op == 0xE8:  # call rel16
                rel = struct.unpack_from("<h", self.code, self.base + ip + 1)[0]
                ip += 3
                self.run(ip + rel, depth + 1)
            elif op == 0xC3:  # ret
                return
            elif op == 0xCA:  # retf imm16
                return
            else:
                raise RuntimeError(f"unhandled opcode {op:#04x} at {RAND_SEG:04x}:{ip:04x}")

    # --- the two entry points -----------------------------------------
    def seed(self, value):
        self.mem[RANDSEED_OFF] = value & 0xFFFF
        self.mem[RANDSEED_OFF + 2] = (value >> 16) & 0xFFFF

    def state(self):
        return (self.mem.get(RANDSEED_OFF + 2, 0) << 16) | self.mem.get(RANDSEED_OFF, 0)

    def next_u32(self):
        self.run(RAND)
        return self.state()

    def below(self, n):
        self.arg = n & 0xFFFF
        self.run(RANDOM)
        return self.r["ax"]


def main():
    data, hdr = load_image()
    base = hdr + (RAND_SEG - IMAGE_SEG) * 16

    # Sanity: the routines start where docs/re/rng.md says they do.
    assert data[base + RAND : base + RAND + 4] == b"\xa1\x7e\x36\x8b", "unexpected @Rand prologue"
    assert data[base + RANDOM : base + RANDOM + 3] == b"\xe8\x5a\x00", "unexpected Random prologue"

    ds_base = hdr + (0x20AE - IMAGE_SEG) * 16
    assert data[ds_base + RANDSEED_OFF : ds_base + RANDSEED_OFF + 4] == b"\x00\x00\x00\x00", (
        "RandSeed is not zero in the load image"
    )

    seed = 0
    cpu = Cpu(data, base)
    cpu.seed(seed)
    raw = [cpu.next_u32() for _ in range(96)]

    # Independent algebraic cross-check of the same bytes: the interpreter
    # result must equal the closed form read off the disassembly. A mismatch
    # means one of the two readings is wrong -- fail loudly rather than emit.
    s = seed
    for i, got in enumerate(raw):
        s = (s * 0x08088405 + 1) & 0xFFFFFFFF
        if s != got:
            sys.exit(f"interpreter/closed-form mismatch at index {i}: {got:#010x} != {s:#010x}")

    # Moduli that appear as literal Random(n) arguments in the game code.
    # 100 -> FUN_1000_3d11:245, 51 -> FUN_1000_0d14:22, 10 -> FUN_1000_3d11:143,
    # 6   -> FUN_1000_0d14:118, 3  -> FUN_1000_3d11:259, 2 -> FUN_1000_0d14:54.
    moduli = [100, 51, 10, 6, 3, 2]
    below = []
    for n in moduli:
        c = Cpu(data, base)
        c.seed(seed)
        below.append({"n": n, "expected": [c.below(n) for _ in range(64)]})

    OUT.write_text(
        json.dumps(
            {
                "note": (
                    "Ground truth for the original RNG. Produced by "
                    "tools/gen_rng_vectors.py, which decodes and interprets the "
                    "instruction bytes of @Rand (1f78:11a8) and Random(Word) "
                    "(1f78:114b) directly out of orig/g.exe. NOT generated from "
                    "src/rng.rs. See docs/re/rng.md."
                ),
                "source": "orig/g.exe md5 10eb0af07a2d2f5e9da790df7058891c",
                "seed": seed,
                "next_u32": raw,
                "below": below,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"wrote {OUT} ({len(raw)} next_u32, {len(below)} below cases)")


if __name__ == "__main__":
    main()
