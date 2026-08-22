#!/usr/bin/env python3
"""A minimal 16-bit x86 decoder: instruction boundaries and operand spans.

`capstone` is not available in this environment (standard library only), and
the questions this project keeps asking do not need a full disassembler.  They
need two things the existing byte-scan tools cannot give:

  * **where an instruction starts and ends** -- so "is this address an
    instruction boundary?" stops being answered by eye;
  * **where an instruction's operands are** -- so a byte-scan hit for a data
    address can be told apart from a hit that straddles two instructions.

So this decodes *length and field layout*, and renders text only for the
subset it can render honestly; `Insn.text` falls back to `db`-style bytes
rather than guessing a mnemonic.  Every `Insn` carries its raw bytes, so a
consumer can always check the decode by eye.

Decoding is 16-bit real mode.  `0x66`/`0x67` are handled (they toggle operand
and address size), and anything genuinely unknown raises `DecodeError` -- an
honest failure, never a length guess, because a wrong length silently
desynchronises everything after it.

Addresses here are offsets into whatever buffer is passed in; `tools/addr.py`
owns the conversion between file, image and `seg:off`.  Rendered branch targets
are offsets in that same buffer -- so a near call inside the image reads as an
IMAGE offset, not a segment offset.  Memory operands are printed as the raw
16-bit displacement the encoding carries, because which segment register
applies is not decidable from the bytes alone.
"""

__all__ = ["DecodeError", "Operand", "Insn", "decode", "decode_run",
           "boundary_votes", "instruction_covering"]


class DecodeError(ValueError):
    """The bytes at this offset are not a decodable 16-bit instruction."""


class Operand:
    """One immediate/displacement field, located in the buffer.

    `kind` is one of:
      `disp16`   -- mod=00 rm=110, a DIRECT memory address (`[0x3b74]`)
      `disp16x`  -- mod=10, a base/index displacement (`[bx+0x3b74]`)
      `disp8`    -- mod=01
      `moffs16`  -- the direct address of `mov al,[addr]` (opcodes A0..A3)
      `imm8` / `imm16` / `imm32`
      `rel8` / `rel16`
      `ptr_off` / `ptr_seg` -- the two halves of a far pointer (9A/EA)
    """

    __slots__ = ("kind", "start", "size", "value")

    def __init__(self, kind, start, size, value):
        self.kind, self.start, self.size, self.value = kind, start, size, value

    @property
    def end(self):
        return self.start + self.size

    def covers(self, off):
        return self.start <= off < self.end

    def __repr__(self):
        return "Operand(%s@%#x+%d=%#x)" % (self.kind, self.start, self.size,
                                           self.value)


class Insn:
    __slots__ = ("off", "length", "raw", "opcode", "prefixes", "modrm",
                 "operands", "text")

    def __init__(self, off, length, raw, opcode, prefixes, modrm, operands, text):
        self.off, self.length, self.raw = off, length, raw
        self.opcode, self.prefixes, self.modrm = opcode, prefixes, modrm
        self.operands, self.text = operands, text

    @property
    def end(self):
        return self.off + self.length

    def covers(self, off):
        return self.off <= off < self.end

    def operand_at(self, off, size=None):
        """The operand field that STARTS at `off` (optionally of `size` bytes)."""
        for o in self.operands:
            if o.start == off and (size is None or o.size == size):
                return o
        return None

    def hex(self):
        return self.raw.hex(" ")

    def __repr__(self):
        return "Insn(%#x %s  %s)" % (self.off, self.hex(), self.text)


# --- opcode map --------------------------------------------------------------
# value: (has_modrm, imm_spec).  imm_spec is None, or one of
#   'ib' 'iw' 'iv' (operand-size) 'iw_ib' 'ptr' 'rel8' 'relv' 'moffs'
_MAP = {}
_PREFIX_SEG = {0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65}
_PREFIX_OTHER = {0xF0, 0xF2, 0xF3}
_PREFIX_OPSIZE = 0x66
_PREFIX_ADDRSIZE = 0x67


def _fill():
    for base in (0x00, 0x08, 0x10, 0x18, 0x20, 0x28, 0x30, 0x38):
        for k in range(4):
            _MAP[base + k] = (True, None)
        _MAP[base + 4] = (False, "ib")
        _MAP[base + 5] = (False, "iv")
    for op in (0x06, 0x07, 0x0E, 0x16, 0x17, 0x1E, 0x1F,
               0x27, 0x2F, 0x37, 0x3F, 0x60, 0x61,
               0x6C, 0x6D, 0x6E, 0x6F,
               0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
               0x98, 0x99, 0x9B, 0x9C, 0x9D, 0x9E, 0x9F,
               0xA4, 0xA5, 0xA6, 0xA7, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF,
               0xC3, 0xC9, 0xCB, 0xCC, 0xCE, 0xCF, 0xD6, 0xD7,
               0xEC, 0xED, 0xEE, 0xEF, 0xF1, 0xF4, 0xF5,
               0xF8, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD):
        _MAP[op] = (False, None)
    for op in range(0x40, 0x60):
        _MAP[op] = (False, None)
    _MAP[0x62] = (True, None)
    _MAP[0x63] = (True, None)
    _MAP[0x68] = (False, "iv")
    _MAP[0x69] = (True, "iv")
    _MAP[0x6A] = (False, "ib")
    _MAP[0x6B] = (True, "ib")
    for op in range(0x70, 0x80):
        _MAP[op] = (False, "rel8")
    _MAP[0x80] = (True, "ib")
    _MAP[0x81] = (True, "iv")
    _MAP[0x82] = (True, "ib")
    _MAP[0x83] = (True, "ib")
    for op in range(0x84, 0x90):
        _MAP[op] = (True, None)
    _MAP[0x9A] = (False, "ptr")
    for op in (0xA0, 0xA1, 0xA2, 0xA3):
        _MAP[op] = (False, "moffs")
    _MAP[0xA8] = (False, "ib")
    _MAP[0xA9] = (False, "iv")
    for op in range(0xB0, 0xB8):
        _MAP[op] = (False, "ib")
    for op in range(0xB8, 0xC0):
        _MAP[op] = (False, "iv")
    _MAP[0xC0] = (True, "ib")
    _MAP[0xC1] = (True, "ib")
    _MAP[0xC2] = (False, "iw")
    _MAP[0xC4] = (True, None)
    _MAP[0xC5] = (True, None)
    _MAP[0xC6] = (True, "ib")
    _MAP[0xC7] = (True, "iv")
    _MAP[0xC8] = (False, "iw_ib")
    _MAP[0xCA] = (False, "iw")
    _MAP[0xCD] = (False, "ib")
    for op in range(0xD0, 0xD4):
        _MAP[op] = (True, None)
    _MAP[0xD4] = (False, "ib")
    _MAP[0xD5] = (False, "ib")
    for op in range(0xD8, 0xE0):        # x87 escapes
        _MAP[op] = (True, None)
    for op in range(0xE0, 0xE4):
        _MAP[op] = (False, "rel8")
    for op in range(0xE4, 0xE8):
        _MAP[op] = (False, "ib")
    _MAP[0xE8] = (False, "relv")
    _MAP[0xE9] = (False, "relv")
    _MAP[0xEA] = (False, "ptr")
    _MAP[0xEB] = (False, "rel8")
    _MAP[0xF6] = (True, None)           # +ib when reg is 0 or 1 (test)
    _MAP[0xF7] = (True, None)           # +iv when reg is 0 or 1 (test)
    _MAP[0xFE] = (True, None)
    _MAP[0xFF] = (True, None)


_fill()

# 0x0f two-byte opcodes, restricted to what a real-mode Borland image can
# plausibly contain.  Anything else raises rather than guessing a length.
_MAP0F = {}
for _op in range(0x80, 0x90):
    _MAP0F[_op] = (False, "relv")
for _op in range(0x90, 0xA0):
    _MAP0F[_op] = (True, None)
for _op in (0xAF, 0xB6, 0xB7, 0xBE, 0xBF, 0xA5, 0xAD):
    _MAP0F[_op] = (True, None)
for _op in (0xA4, 0xAC):            # shld/shrd r/m,r,imm8 -- Borland's 32-bit math
    _MAP0F[_op] = (True, "ib")

_REG8 = ("al", "cl", "dl", "bl", "ah", "ch", "dh", "bh")
_REG16 = ("ax", "cx", "dx", "bx", "sp", "bp", "si", "di")
_SREG = ("es", "cs", "ss", "ds", "fs", "gs", "?6", "?7")
_RM16 = ("bx+si", "bx+di", "bp+si", "bp+di", "si", "di", "bp", "bx")
_ARITH = ("add", "or", "adc", "sbb", "and", "sub", "xor", "cmp")
_SHIFT = ("rol", "ror", "rcl", "rcr", "shl", "shr", "sal", "sar")
_JCC = ("jo", "jno", "jb", "jnb", "jz", "jnz", "jbe", "ja",
        "js", "jns", "jp", "jnp", "jl", "jnl", "jle", "jnle")
_GRP3 = ("test", "test", "not", "neg", "mul", "imul", "div", "idiv")
_GRP5 = ("inc", "dec", "call", "call far", "jmp", "jmp far", "push", "?7")


def _u8(buf, i):
    return buf[i]


def _s8(buf, i):
    v = buf[i]
    return v - 0x100 if v >= 0x80 else v


def _u16(buf, i):
    return buf[i] | (buf[i + 1] << 8)


def decode(buf, off):
    """Decode one instruction at `off`.  Raises `DecodeError` on anything
    it cannot decode exactly."""
    if not 0 <= off < len(buf):
        raise DecodeError("offset %#x outside the buffer" % off)
    start = off
    prefixes = []
    seg_prefix = None
    opsize = 2
    addrsize = 2
    i = off
    while i < len(buf):
        b = buf[i]
        if b in _PREFIX_SEG:
            seg_prefix = b
            prefixes.append(b)
            i += 1
        elif b in _PREFIX_OTHER:
            prefixes.append(b)
            i += 1
        elif b == _PREFIX_OPSIZE:
            opsize = 4
            prefixes.append(b)
            i += 1
        elif b == _PREFIX_ADDRSIZE:
            addrsize = 4
            prefixes.append(b)
            i += 1
        else:
            break
        if len(prefixes) > 4:
            raise DecodeError("more than four prefixes at %#x" % start)
    if i >= len(buf):
        raise DecodeError("prefixes run off the end of the buffer at %#x" % start)
    if addrsize == 4:
        raise DecodeError(
            "address-size prefix at %#x: 32-bit addressing is not decoded here"
            % start)

    op = buf[i]
    i += 1
    two_byte = False
    if op == 0x0F:
        if i >= len(buf):
            raise DecodeError("0f runs off the end of the buffer at %#x" % start)
        op2 = buf[i]
        i += 1
        if op2 not in _MAP0F:
            raise DecodeError("unhandled two-byte opcode 0f %02x at %#x"
                              % (op2, start))
        has_modrm, imm = _MAP0F[op2]
        two_byte = True
        op = op2
    else:
        if op not in _MAP:
            raise DecodeError("unhandled opcode %02x at %#x" % (op, start))
        has_modrm, imm = _MAP[op]

    operands = []
    modrm = None
    mod = reg = rm = None
    if has_modrm:
        if i >= len(buf):
            raise DecodeError("modrm runs off the end of the buffer at %#x" % start)
        modrm = buf[i]
        i += 1
        mod, reg, rm = modrm >> 6, (modrm >> 3) & 7, modrm & 7
        if mod == 3 and not two_byte and op in (0x8D, 0xC4, 0xC5):
            raise DecodeError(
                "opcode %02x at %#x has mod=3, which is not a legal encoding"
                % (op, start))
        if mod == 0 and rm == 6:
            _need(buf, i, 2, start)
            operands.append(Operand("disp16", i, 2, _u16(buf, i)))
            i += 2
        elif mod == 1:
            _need(buf, i, 1, start)
            operands.append(Operand("disp8", i, 1, buf[i]))
            i += 1
        elif mod == 2:
            _need(buf, i, 2, start)
            operands.append(Operand("disp16x", i, 2, _u16(buf, i)))
            i += 2
        if not two_byte and op in (0xF6, 0xF7) and reg in (0, 1):
            imm = "ib" if op == 0xF6 else "iv"

    if imm == "iv":
        imm = "iw" if opsize == 2 else "id"
    if imm == "relv":
        imm = "rel16" if opsize == 2 else "rel32"

    if imm == "ib":
        _need(buf, i, 1, start)
        operands.append(Operand("imm8", i, 1, buf[i]))
        i += 1
    elif imm == "iw":
        _need(buf, i, 2, start)
        operands.append(Operand("imm16", i, 2, _u16(buf, i)))
        i += 2
    elif imm == "id":
        _need(buf, i, 4, start)
        operands.append(Operand("imm32", i, 4,
                                _u16(buf, i) | (_u16(buf, i + 2) << 16)))
        i += 4
    elif imm == "iw_ib":
        _need(buf, i, 3, start)
        operands.append(Operand("imm16", i, 2, _u16(buf, i)))
        operands.append(Operand("imm8", i + 2, 1, buf[i + 2]))
        i += 3
    elif imm == "moffs":
        _need(buf, i, 2, start)
        operands.append(Operand("moffs16", i, 2, _u16(buf, i)))
        i += 2
    elif imm == "rel8":
        _need(buf, i, 1, start)
        operands.append(Operand("rel8", i, 1, _s8(buf, i)))
        i += 1
    elif imm == "rel16":
        _need(buf, i, 2, start)
        v = _u16(buf, i)
        operands.append(Operand("rel16", i, 2, v - 0x10000 if v >= 0x8000 else v))
        i += 2
    elif imm == "rel32":
        _need(buf, i, 4, start)
        operands.append(Operand("rel32", i, 4,
                                _u16(buf, i) | (_u16(buf, i + 2) << 16)))
        i += 4
    elif imm == "ptr":
        _need(buf, i, 4, start)
        operands.append(Operand("ptr_off", i, 2, _u16(buf, i)))
        operands.append(Operand("ptr_seg", i + 2, 2, _u16(buf, i + 2)))
        i += 4
    elif imm is not None:
        raise DecodeError("internal: unknown imm spec %r" % imm)

    raw = bytes(buf[start:i])
    text = _render(op, two_byte, prefixes, seg_prefix, opsize, mod, reg, rm,
                   operands, start, i)
    return Insn(start, i - start, raw, op, tuple(prefixes), modrm, operands, text)


def _need(buf, i, n, start):
    if i + n > len(buf):
        raise DecodeError("instruction at %#x runs off the end of the buffer"
                          % start)


# --- text rendering ----------------------------------------------------------

def _rm_text(mod, rm, operands, seg_prefix, wide):
    if mod == 3:
        return (_REG16 if wide else _REG8)[rm]
    seg = ""
    if seg_prefix is not None:
        seg = {0x26: "es:", 0x2E: "cs:", 0x36: "ss:", 0x3E: "ds:",
               0x64: "fs:", 0x65: "gs:"}[seg_prefix]
    if mod == 0 and rm == 6:
        return "[%s0x%x]" % (seg, operands[0].value)
    base = _RM16[rm]
    if mod == 0:
        return "[%s%s]" % (seg, base)
    disp = operands[0].value
    if mod == 1:
        disp = disp - 0x100 if disp >= 0x80 else disp
        sign = "+" if disp >= 0 else "-"
        return "[%s%s%s0x%x]" % (seg, base, sign, abs(disp))
    if disp >= 0x8000:                 # a 16-bit displacement is signed
        return "[%s%s-0x%x]" % (seg, base, 0x10000 - disp)
    return "[%s%s+0x%x]" % (seg, base, disp)


def _imm_of(operands, kind):
    for o in operands:
        if o.kind == kind:
            return o
    return None


def _render(op, two_byte, prefixes, seg_prefix, opsize, mod, reg, rm,
            operands, start, end):
    rep = ""
    if 0xF3 in prefixes:
        rep = "rep "
    elif 0xF2 in prefixes:
        rep = "repne "
    if two_byte:
        if 0x80 <= op <= 0x8F:
            o = _imm_of(operands, "rel16") or _imm_of(operands, "rel32")
            return "%s near 0x%x" % (_JCC[op - 0x80], end + o.value)
        if op in (0xB6, 0xB7, 0xBE, 0xBF):
            name = {0xB6: "movzx", 0xB7: "movzx", 0xBE: "movsx", 0xBF: "movsx"}[op]
            return "%s %s,%s" % (name, _REG16[reg],
                                 _rm_text(mod, rm, operands, seg_prefix,
                                          op in (0xB7, 0xBF)))
        if op == 0xAF:
            return "imul %s,%s" % (_REG16[reg],
                                   _rm_text(mod, rm, operands, seg_prefix, True))
        if 0x90 <= op <= 0x9F:
            return "set%s %s" % (_JCC[op - 0x90][1:],
                                 _rm_text(mod, rm, operands, seg_prefix, False))
        return "db 0f %02x" % op

    simple = {0x27: "daa", 0x2F: "das", 0x37: "aaa", 0x3F: "aas",
              0x60: "pusha", 0x61: "popa", 0x9B: "wait", 0x9C: "pushf",
              0x9D: "popf", 0x9E: "sahf", 0x9F: "lahf", 0xCC: "int3",
              0xCE: "into", 0xD6: "salc", 0xD7: "xlatb", 0xF4: "hlt",
              0xF5: "cmc", 0xF8: "clc", 0xF9: "stc", 0xFA: "cli",
              0xFB: "sti", 0xFC: "cld", 0xFD: "std"}
    if op in simple:
        return simple[op]
    if op in (0x06, 0x0E, 0x16, 0x1E):
        return "push %s" % _SREG[op >> 3]
    if op in (0x07, 0x17, 0x1F):
        return "pop %s" % _SREG[op >> 3]
    if op < 0x40 and (op & 7) < 6 and op not in (0x0F,):
        name = _ARITH[op >> 3]
        low = op & 7
        wide = bool(low & 1) if low < 4 else True
        if low < 4:
            r = (_REG16 if wide else _REG8)[reg]
            m = _rm_text(mod, rm, operands, seg_prefix, wide)
            return "%s %s,%s" % (name, m, r) if low in (0, 1) else \
                   "%s %s,%s" % (name, r, m)
        o = operands[-1]
        return "%s %s,0x%x" % (name, "al" if low == 4 else "ax", o.value)
    if 0x40 <= op < 0x48:
        return "inc %s" % _REG16[op - 0x40]
    if 0x48 <= op < 0x50:
        return "dec %s" % _REG16[op - 0x48]
    if 0x50 <= op < 0x58:
        return "push %s" % _REG16[op - 0x50]
    if 0x58 <= op < 0x60:
        return "pop %s" % _REG16[op - 0x58]
    if op == 0x68:
        return "push 0x%x" % operands[-1].value
    if op == 0x6A:
        return "push byte 0x%x" % operands[-1].value
    if 0x70 <= op < 0x80:
        return "%s 0x%x" % (_JCC[op - 0x70], end + operands[-1].value)
    if op in (0x80, 0x81, 0x82, 0x83):
        wide = op in (0x81, 0x83)
        size = "" if mod == 3 else ("word " if wide else "byte ")
        return "%s %s%s,0x%x" % (_ARITH[reg], size,
                                 _rm_text(mod, rm, operands, seg_prefix, wide),
                                 operands[-1].value)
    if op in (0x84, 0x85):
        wide = op == 0x85
        return "test %s,%s" % (_rm_text(mod, rm, operands, seg_prefix, wide),
                               (_REG16 if wide else _REG8)[reg])
    if op in (0x86, 0x87):
        wide = op == 0x87
        return "xchg %s,%s" % (_rm_text(mod, rm, operands, seg_prefix, wide),
                               (_REG16 if wide else _REG8)[reg])
    if 0x88 <= op <= 0x8B:
        wide = bool(op & 1)
        r = (_REG16 if wide else _REG8)[reg]
        m = _rm_text(mod, rm, operands, seg_prefix, wide)
        return "mov %s,%s" % ((m, r) if op < 0x8A else (r, m))
    if op == 0x8C:
        return "mov %s,%s" % (_rm_text(mod, rm, operands, seg_prefix, True),
                              _SREG[reg])
    if op == 0x8D:
        return "lea %s,%s" % (_REG16[reg],
                              _rm_text(mod, rm, operands, seg_prefix, True))
    if op == 0x8E:
        return "mov %s,%s" % (_SREG[reg],
                              _rm_text(mod, rm, operands, seg_prefix, True))
    if op == 0x8F:
        return "pop %s" % _rm_text(mod, rm, operands, seg_prefix, True)
    if op == 0x90:
        return "nop"
    if 0x91 <= op < 0x98:
        return "xchg ax,%s" % _REG16[op - 0x90]
    if op == 0x98:
        return "cbw"
    if op == 0x99:
        return "cwd"
    if op == 0x9A:
        return "call 0x%x:0x%x" % (operands[1].value, operands[0].value)
    if 0xA0 <= op <= 0xA3:
        m = "[0x%x]" % operands[-1].value
        r = "al" if op in (0xA0, 0xA2) else "ax"
        return "mov %s,%s" % ((r, m) if op < 0xA2 else (m, r))
    if op in (0xA4, 0xA5):
        return rep + ("movsb" if op == 0xA4 else "movsw")
    if op in (0xA6, 0xA7):
        return rep + ("cmpsb" if op == 0xA6 else "cmpsw")
    if op in (0xAA, 0xAB):
        return rep + ("stosb" if op == 0xAA else "stosw")
    if op in (0xAC, 0xAD):
        return rep + ("lodsb" if op == 0xAC else "lodsw")
    if op in (0xAE, 0xAF):
        return rep + ("scasb" if op == 0xAE else "scasw")
    if op in (0xA8, 0xA9):
        return "test %s,0x%x" % ("al" if op == 0xA8 else "ax", operands[-1].value)
    if 0xB0 <= op < 0xB8:
        return "mov %s,0x%x" % (_REG8[op - 0xB0], operands[-1].value)
    if 0xB8 <= op < 0xC0:
        return "mov %s,0x%x" % (_REG16[op - 0xB8], operands[-1].value)
    if op in (0xC0, 0xC1):
        wide = op == 0xC1
        return "%s %s,0x%x" % (_SHIFT[reg],
                               _rm_text(mod, rm, operands, seg_prefix, wide),
                               operands[-1].value)
    if op == 0xC2:
        return "ret 0x%x" % operands[-1].value
    if op == 0xC3:
        return "ret"
    if op in (0xC4, 0xC5):
        return "%s %s,%s" % ("les" if op == 0xC4 else "lds", _REG16[reg],
                             _rm_text(mod, rm, operands, seg_prefix, True))
    if op in (0xC6, 0xC7):
        wide = op == 0xC7
        size = "" if mod == 3 else ("word " if wide else "byte ")
        return "mov %s%s,0x%x" % (size,
                                  _rm_text(mod, rm, operands, seg_prefix, wide),
                                  operands[-1].value)
    if op == 0xC9:
        return "leave"
    if op == 0xCA:
        return "retf 0x%x" % operands[-1].value
    if op == 0xCB:
        return "retf"
    if op == 0xCD:
        return "int 0x%x" % operands[-1].value
    if op == 0xCF:
        return "iret"
    if 0xD0 <= op <= 0xD3:
        wide = bool(op & 1)
        amount = "1" if op < 0xD2 else "cl"
        return "%s %s,%s" % (_SHIFT[reg],
                             _rm_text(mod, rm, operands, seg_prefix, wide),
                             amount)
    if 0xE0 <= op <= 0xE3:
        name = ("loopne", "loope", "loop", "jcxz")[op - 0xE0]
        return "%s 0x%x" % (name, end + operands[-1].value)
    if op == 0xE8:
        return "call 0x%x" % (end + operands[-1].value)
    if op == 0xE9:
        return "jmp 0x%x" % (end + operands[-1].value)
    if op == 0xEA:
        return "jmp 0x%x:0x%x" % (operands[1].value, operands[0].value)
    if op == 0xEB:
        return "jmp short 0x%x" % (end + operands[-1].value)
    if op in (0xF6, 0xF7):
        wide = op == 0xF7
        m = _rm_text(mod, rm, operands, seg_prefix, wide)
        if reg in (0, 1):
            size = "" if mod == 3 else ("word " if wide else "byte ")
            return "test %s%s,0x%x" % (size, m, operands[-1].value)
        return "%s %s" % (_GRP3[reg], m)
    if op == 0xFE:
        return "%s %s" % (("inc", "dec")[reg & 1],
                          _rm_text(mod, rm, operands, seg_prefix, False))
    if op == 0xFF:
        wide = reg not in (0, 1) or True
        return "%s %s" % (_GRP5[reg],
                          _rm_text(mod, rm, operands, seg_prefix, wide))
    return "db %02x" % op


# --- sweeps ------------------------------------------------------------------

def decode_run(buf, start, stop):
    """Decode instructions from `start` until an instruction reaches `stop`.

    Returns the list of instructions.  Raises `DecodeError` if any instruction
    fails to decode -- the sweep is not allowed to guess its way past one.
    """
    out = []
    pos = start
    while pos < stop:
        insn = decode(buf, pos)
        out.append(insn)
        pos = insn.end
    return out


def instruction_covering(insns, off):
    for insn in insns:
        if insn.covers(off):
            return insn
    return None


def boundary_votes(buf, target, back=64, lo=None):
    """The linear-sweep alignment test: does a sweep started `k` bytes before
    `target` land exactly on it?

    Returns `(hits, tried, misses)`.  A high score says only that `target` is a
    plausible instruction boundary -- `1000:d83b` scores 64/64 and is still the
    wrong address, four bytes before the call it was supposed to name.  So this
    is never sufficient on its own; identity (the bytes at `target`) is the
    separate, stronger signal.
    """
    lo = 0 if lo is None else lo
    hits = tried = 0
    misses = []
    for k in range(1, back + 1):
        start = target - k
        if start < lo:
            break
        tried += 1
        pos = start
        try:
            while pos < target:
                pos = decode(buf, pos).end
        except DecodeError:
            misses.append((k, "decode failed before reaching the target"))
            continue
        if pos == target:
            hits += 1
        else:
            misses.append((k, "sweep stepped over it, landing at +%d" % (pos - target)))
    return hits, tried, misses
