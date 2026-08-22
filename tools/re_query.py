#!/usr/bin/env python3
"""The four disassembly questions this project keeps hand-rolling, as a CLI.

Every defect the recent review loops found was an address, a count, or a
convention error -- never a failure to read disassembly.  So these four
queries exist to be *run* instead of done by eye, and each prints
command-shaped evidence a `docs/re/` claim can quote directly:

  resolve CITATION
      Either citation form (`1000:b353` or `0f78:114b`) -> image offset, file
      offset, the bytes there, and the instructions that start there.  The
      form is chosen by `tools/addr.py` from the segment, so the 64 KiB
      mix-up cannot happen here.

  is-call-site CITATION
      Alignment AND identity, reported separately.  `1000:d83b` scores 64/64
      on the linear-sweep alignment test and is still the wrong address -- a
      real instruction boundary four bytes before the call.  Alignment alone
      never answers yes.

  pushed-n CITATION
      Given a `Random` call site, the `n` the preceding aligned idiom pushes.
      Task 11c did this by eye 21 times.  Walks back from the call over the
      minimal contiguous run of instructions that determines the pushed
      value, and evaluates it symbolically.

  xrefs-to ADDRESS
      Who references a data address.  A raw byte scan for the operand bytes
      of `20ae:3b74` returns 7 hits, one of which (`1000:c358`) is the
      straddle of a `jl` and a `cmp` and not an operand at all.  Hits are
      kept only when they fall on an operand field of a decoded instruction,
      and the output says how many were discarded and why.

Standard library only; `tools/dis16.py` does the decoding because `capstone`
is not available here.  `docs/re/METHODOLOGY.md` is the authority for the
address convention; `tools/addr.py` is its executable form.
"""
import argparse
import json
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import addr  # noqa: E402
import dis16  # noqa: E402

DEFAULT_FUNCTIONS = addr.REPO / "data" / "functions.json"

#: The identity of a `Random` draw site: `call far 0f78:114b`.
RANDOM_CALL_BYTES = addr.RANDOM_CALL_BYTES

#: Operand kinds that can carry a data address.
ADDRESS_OPERANDS = ("disp16", "disp16x", "moffs16", "imm16")


class Program:
    """`orig/g.exe` plus whatever the Ghidra export knows about it."""

    def __init__(self, exe_path=None, functions_path=DEFAULT_FUNCTIONS):
        self.exe = addr.read_exe(exe_path)
        self.image = addr.load_image(self.exe)
        self.functions = []
        self.functions_path = Path(functions_path) if functions_path else None
        if self.functions_path and self.functions_path.exists():
            self.functions = json.loads(self.functions_path.read_text())
        self._ranges = []
        for f in self.functions:
            lo = addr.image_off_of_citation(f["entry"])
            self._ranges.append((lo, lo + f["size"], f))

    # --- anchoring -----------------------------------------------------------

    def function_containing(self, image_off):
        for lo, hi, f in self._ranges:
            if lo <= image_off < hi:
                return f
        return None

    def anchored_stream(self, image_off):
        """Instructions covering `image_off`, and how the alignment was fixed.

        Returns `(insns, anchor)` where `anchor` is a dict describing the
        provenance: a function entry from the Ghidra export is an anchor that
        was not guessed; a back-sweep consensus is, and says so.
        """
        f = self.function_containing(image_off)
        if f is not None:
            entry = addr.image_off_of_citation(f["entry"])
            try:
                insns = dis16.decode_run(self.image, entry, image_off + 1)
                return insns, {"kind": "function-entry",
                               "function": f["name"], "entry": f["entry"]}
            except dis16.DecodeError as e:
                anchor_err = str(e)
        else:
            anchor_err = "no function in data/functions.json contains it"
        insns, votes, tried = self.sweep_stream(image_off)
        return insns, {"kind": "back-sweep", "votes": votes, "tried": tried,
                       "why_not_anchored": anchor_err}

    def sweep_stream(self, image_off, back=64):
        """Self-synchronising fallback: sweep from many earlier starts and take
        the instruction layout the majority of them converge on."""
        tally = Counter()
        streams = {}
        for k in range(1, back + 1):
            start = image_off - k
            if start < 0:
                break
            try:
                insns = dis16.decode_run(self.image, start, image_off + 1)
            except dis16.DecodeError:
                continue
            cover = dis16.instruction_covering(insns, image_off)
            if cover is None:
                continue
            key = (cover.off, cover.length)
            tally[key] += 1
            streams.setdefault(key, insns)
        if not tally:
            return [], 0, back
        key, votes = tally.most_common(1)[0]
        return streams[key], votes, sum(tally.values())


# --- resolve -----------------------------------------------------------------

def resolve(prog, text, nbytes=16, ninsns=4):
    cit = addr.citation(text)
    image_off, file_off = cit.image_off, cit.file_off
    out = {
        "citation": cit.text,
        "form": cit.form,
        "seg": "%04x" % cit.seg,
        "off": "%04x" % cit.off,
        "ghidra_label": cit.ghidra_label,
        "image_off": "0x%x" % image_off,
        "file_off": "0x%x" % file_off,
        "bytes": self_slice(prog.exe, file_off, nbytes),
    }
    f = prog.function_containing(image_off)
    out["function"] = f["name"] if f else None
    out["instructions"] = []
    if image_off < len(prog.image):
        pos = image_off
        for _ in range(ninsns):
            try:
                insn = dis16.decode(prog.image, pos)
            except dis16.DecodeError as e:
                out["instructions"].append({"at": None, "error": str(e)})
                break
            out["instructions"].append(_insn_json(cit.seg, cit.off, insn, image_off))
            pos = insn.end
    return out


def self_slice(exe, file_off, n):
    return exe[file_off:file_off + n].hex(" ")


def _insn_json(seg, off, insn, base_image_off):
    """Render an instruction, labelled in the SAME citation form it was asked in."""
    delta = insn.off - base_image_off
    return {"at": "%04x:%04x" % (seg, off + delta),
            "image_off": "0x%x" % insn.off,
            "file_off": "0x%x" % addr.file_off_of_image_off(insn.off),
            "bytes": insn.hex(),
            "text": insn.text}


# --- is-call-site ------------------------------------------------------------

def is_call_site(prog, text, signature=RANDOM_CALL_BYTES, back=64, near=32):
    """Report ALIGNMENT and IDENTITY separately, and never conflate them."""
    cit = addr.citation(text)
    io = cit.image_off
    got = bytes(prog.image[io:io + len(signature)])
    identity = got == signature

    hits, tried, misses = dis16.boundary_votes(prog.image, io, back=back)
    f = prog.function_containing(io)
    anchored = None
    if f is not None:
        entry = addr.image_off_of_citation(f["entry"])
        try:
            insns = dis16.decode_run(prog.image, entry, io + 1)
            anchored = any(i.off == io for i in insns)
        except dis16.DecodeError:
            anchored = None

    nearest = []
    lo, hi = max(0, io - near), io + near
    window = bytes(prog.image[lo:hi + len(signature)])
    i = window.find(signature)
    while i != -1:
        nearest.append(lo + i - io)
        i = window.find(signature, i + 1)

    return {
        "citation": cit.text,
        "image_off": "0x%x" % io,
        "file_off": "0x%x" % cit.file_off,
        "function": f["name"] if f else None,
        "signature": signature.hex(" "),
        "identity": {
            "match": identity,
            "bytes_here": got.hex(" "),
            "nearest_signature_deltas": nearest,
        },
        "alignment": {
            "sweep_votes": hits,
            "sweep_tried": tried,
            "anchored_from_function_entry": anchored,
            "first_misses": misses[:3],
        },
        "verdict": "call site" if identity else "NOT a call site",
        "note": ("Alignment alone never answers yes: 1000:d83b scores all but "
                 "one of the same sweeps and is still the wrong address -- a "
                 "real instruction boundary four bytes before the call it was "
                 "mistaken for.  Only `identity.match` settles it."),
    }


# --- pushed-n ----------------------------------------------------------------

class Val:
    """A 16-bit value as `const + sum(coeff * symbol)`, or unknown.

    Deliberately tiny: it only has to cover the idioms that actually push a
    `Random` argument in this image -- an immediate, `imul ax`, `mul dx`, and
    the shift-and-add multiply Borland emits for a small constant.
    """

    __slots__ = ("const", "terms", "known", "hi_known")

    def __init__(self, const=0, terms=None, known=True, hi_known=True):
        self.const = const
        self.terms = dict(terms or {})
        self.known = known
        self.hi_known = hi_known

    @classmethod
    def unknown(cls):
        return cls(known=False, hi_known=False)

    @classmethod
    def sym(cls, name):
        return cls(0, {name: 1})

    def copy(self):
        return Val(self.const, self.terms, self.known, self.hi_known)

    @property
    def usable(self):
        return self.known and self.hi_known

    def is_const(self):
        return self.usable and not self.terms

    def scale(self, k):
        if not self.usable:
            return Val.unknown()
        return Val(self.const * k, {s: c * k for s, c in self.terms.items()})

    def add(self, other):
        if not (self.usable and other.usable):
            return Val.unknown()
        terms = dict(self.terms)
        for s, c in other.terms.items():
            terms[s] = terms.get(s, 0) + c
            if terms[s] == 0:
                del terms[s]
        return Val(self.const + other.const, terms)

    def mul(self, other):
        if not (self.usable and other.usable):
            return Val.unknown()
        if self.is_const():
            return other.scale(self.const)
        if other.is_const():
            return self.scale(other.const)
        return Val.unknown()

    def render(self):
        if not self.usable:
            return None
        if not self.terms:
            return self.const
        parts = []
        for s, c in sorted(self.terms.items()):
            parts.append(s if c == 1 else "%s * %d" % (s, c))
        if self.const:
            parts.append(str(self.const))
        return " + ".join(parts)


_REG16_INDEX = {0: "ax", 1: "cx", 2: "dx", 3: "bx", 4: "sp", 5: "bp", 6: "si", 7: "di"}
_REG8_PARENT = {0: "ax", 1: "cx", 2: "dx", 3: "bx", 4: "ax", 5: "cx", 6: "dx", 7: "bx"}
_REG8_IS_HIGH = {0: False, 1: False, 2: False, 3: False, 4: True, 5: True, 6: True, 7: True}


def _writes_reads(insn):
    """`(writes, reads)` as sets of 16-bit register names, for the small set of
    instructions a `Random` argument idiom is built from.

    Returns `(None, None)` for anything outside that set, which stops the
    backwards walk rather than guessing at its dataflow.
    """
    if insn.prefixes:
        return None, None
    op, modrm = insn.opcode, insn.modrm
    mod = reg = rm = None
    if modrm is not None:
        mod, reg, rm = modrm >> 6, (modrm >> 3) & 7, modrm & 7
    if 0xB8 <= op < 0xC0:                                   # mov r16,imm16
        return {_REG16_INDEX[op - 0xB8]}, set()
    if 0xB0 <= op < 0xB8:                                   # mov r8,imm8
        return {_REG8_PARENT[op - 0xB0]}, set()
    if op == 0xA0:                                          # mov al,[addr]
        return {"ax"}, set()
    if op == 0xA1:                                          # mov ax,[addr]
        return {"ax"}, set()
    if op in (0x88, 0x89):                                  # mov r/m,r
        if mod != 3:
            return set(), {_REG16_INDEX[reg] if op == 0x89 else _REG8_PARENT[reg]}
        dst = _REG16_INDEX[rm] if op == 0x89 else _REG8_PARENT[rm]
        src = _REG16_INDEX[reg] if op == 0x89 else _REG8_PARENT[reg]
        return {dst}, {src}
    if op in (0x8A, 0x8B):                                  # mov r,r/m
        dst = _REG16_INDEX[reg] if op == 0x8B else _REG8_PARENT[reg]
        src = set() if mod != 3 else {_REG16_INDEX[rm] if op == 0x8B
                                      else _REG8_PARENT[rm]}
        return {dst}, src
    if op in (0x30, 0x31, 0x32, 0x33) and mod == 3 and reg == rm:
        # `xor r,r` is a pure define -- it reads the register but the result
        # does not depend on it.  Treating it as a read would make the
        # backwards walk think the value is still undetermined.
        wide = bool(op & 1)
        return {_REG16_INDEX[rm] if wide else _REG8_PARENT[rm]}, set()
    if op in (0x00, 0x01, 0x28, 0x29, 0x30, 0x31, 0x20, 0x21, 0x08, 0x09,
              0x02, 0x03, 0x2A, 0x2B, 0x32, 0x33, 0x22, 0x23, 0x0A, 0x0B):
        wide = bool(op & 1)
        to_rm = (op & 2) == 0
        r = _REG16_INDEX[reg] if wide else _REG8_PARENT[reg]
        m = None if mod != 3 else (_REG16_INDEX[rm] if wide else _REG8_PARENT[rm])
        if to_rm:
            if m is None:
                return set(), {r}
            return {m}, {m, r}
        return {r}, ({r} if m is None else {r, m})
    if op in (0xD0, 0xD1):                                  # shift r/m,1
        if mod != 3:
            return set(), set()
        t = _REG16_INDEX[rm] if op == 0xD1 else _REG8_PARENT[rm]
        return {t}, {t}
    if op in (0xF6, 0xF7) and reg in (4, 5):                # mul / imul r/m
        src = set() if mod != 3 else {_REG16_INDEX[rm] if op == 0xF7
                                      else _REG8_PARENT[rm]}
        return {"ax", "dx"}, {"ax"} | src
    if 0x40 <= op < 0x48:                                   # inc r16
        return {_REG16_INDEX[op - 0x40]}, {_REG16_INDEX[op - 0x40]}
    if 0x48 <= op < 0x50:                                   # dec r16
        return {_REG16_INDEX[op - 0x48]}, {_REG16_INDEX[op - 0x48]}
    if op == 0x98:                                          # cbw
        return {"ax"}, {"ax"}
    return None, None


def _push_source(insn):
    """The register a `push` pushes, or None when it pushes memory."""
    if 0x50 <= insn.opcode < 0x58 and not insn.prefixes:
        return _REG16_INDEX[insn.opcode - 0x50]
    return None


def _is_push(insn):
    if 0x50 <= insn.opcode < 0x58:
        return True
    return insn.opcode == 0xFF and insn.modrm is not None and \
        ((insn.modrm >> 3) & 7) == 6


def _eval_run(insns, prog):
    """Forward-evaluate the idiom.  Returns the pushed `Val`."""
    regs = {r: Val.unknown() for r in _REG16_INDEX.values()}
    pushed = Val.unknown()
    for insn in insns:
        op, modrm = insn.opcode, insn.modrm
        mod = reg = rm = None
        if modrm is not None:
            mod, reg, rm = modrm >> 6, (modrm >> 3) & 7, modrm & 7
        if _is_push(insn):
            src = _push_source(insn)
            pushed = regs[src].copy() if src else Val.unknown()
            continue
        if 0xB8 <= op < 0xC0:
            regs[_REG16_INDEX[op - 0xB8]] = Val(insn.operands[-1].value)
        elif 0xB0 <= op < 0xB8:
            parent = _REG8_PARENT[op - 0xB0]
            if _REG8_IS_HIGH[op - 0xB0]:
                v = regs[parent].copy()
                v.hi_known = insn.operands[-1].value == 0
                regs[parent] = v
            else:
                regs[parent] = Val(insn.operands[-1].value, hi_known=False)
        elif op == 0xA0:                       # mov al,[addr]
            regs["ax"] = Val(0, {"byte[0x%x]" % insn.operands[-1].value: 1},
                             hi_known=False)
        elif op == 0xA1:                       # mov ax,[addr]
            regs["ax"] = Val.sym("word[0x%x]" % insn.operands[-1].value)
        elif op in (0x30, 0x32) and mod == 3 and reg == rm:
            # xor r8,r8 -- the zero-extend half of `mov al,[m] / xor ah,ah`
            parent = _REG8_PARENT[rm]
            v = regs[parent].copy()
            if _REG8_IS_HIGH[rm]:
                v.hi_known = True
            else:
                v = Val(0, hi_known=v.hi_known)
            regs[parent] = v
        elif op == 0x8B and mod == 3:          # mov r16,r16
            regs[_REG16_INDEX[reg]] = regs[_REG16_INDEX[rm]].copy()
        elif op == 0x89 and mod == 3:
            regs[_REG16_INDEX[rm]] = regs[_REG16_INDEX[reg]].copy()
        elif op == 0xD1 and mod == 3 and reg == 4:   # shl r16,1
            regs[_REG16_INDEX[rm]] = regs[_REG16_INDEX[rm]].scale(2)
        elif op in (0x01, 0x03) and mod == 3:        # add r16,r16
            dst = _REG16_INDEX[rm] if op == 0x01 else _REG16_INDEX[reg]
            src = _REG16_INDEX[reg] if op == 0x01 else _REG16_INDEX[rm]
            regs[dst] = regs[dst].add(regs[src])
        elif op == 0xF7 and reg in (4, 5):           # mul / imul r/m16
            if mod == 3:
                regs["ax"] = regs["ax"].mul(regs[_REG16_INDEX[rm]])
            else:
                regs["ax"] = Val.unknown()
            regs["dx"] = Val.unknown()
        else:
            w, _ = _writes_reads(insn)
            for r in (w or _REG16_INDEX.values()):
                regs[r] = Val.unknown()
    return pushed


def pushed_n(prog, text, signature=RANDOM_CALL_BYTES, max_back=24):
    cit = addr.citation(text)
    io = cit.image_off
    got = bytes(prog.image[io:io + len(signature)])
    if got != signature:
        raise ValueError(
            "%s (image 0x%x, file 0x%x) holds %s, not the %s call signature -- "
            "run `is-call-site` first: pushed-n only means anything at a real "
            "call site." % (cit.text, io, cit.file_off, got.hex(" "),
                            signature.hex(" ")))

    insns, anchor = prog.anchored_stream(io)
    before = [i for i in insns if i.end <= io]
    if not before or before[-1].end != io:
        raise ValueError(
            "no aligned instruction ends at %s: the stream anchored by %s does "
            "not reach it cleanly" % (cit.text, anchor))

    # Walk back over the MINIMAL contiguous run that determines the pushed
    # value: start from the push, and keep absorbing while some register the
    # push depends on is still undefined.
    idx = len(before) - 1
    push = before[idx]
    if not _is_push(push):
        raise ValueError("the instruction before %s is `%s`, not a push"
                         % (cit.text, push.text))
    src = _push_source(push)
    needed = set()
    if src:
        needed = {(src, 0), (src, 1)}
    run = [push]
    steps = 0
    while needed and idx > 0 and steps < max_back:
        idx -= 1
        steps += 1
        insn = before[idx]
        w, r = _writes_reads(insn)
        if w is None:
            break                     # not part of an argument idiom: stop
        defs = _written_lanes(insn, w)
        if not (defs & needed):
            break                     # contributes nothing to the pushed value
        run.insert(0, insn)
        # Backward liveness: needed := (needed - defs) | uses.  A register that
        # is BOTH read and written (`mul dx`, `shl ax,1`) stays needed, which is
        # the case the first version of this walk got wrong.
        uses = {(reg, lane) for reg in r for lane in (0, 1)}
        needed = (needed - defs) | uses

    value = _eval_run(run, prog)
    start = run[0].off
    return {
        "citation": cit.text,
        "image_off": "0x%x" % io,
        "file_off": "0x%x" % cit.file_off,
        "function": (prog.function_containing(io) or {}).get("name"),
        "anchor": anchor,
        "n": value.render() if value.is_const() else None,
        "n_expr": None if value.is_const() else value.render(),
        "n_at": "%04x:%04x" % (cit.seg, cit.off - (io - start)),
        "n_bytes": " ".join(i.hex() for i in run),
        "idiom": [_insn_json(cit.seg, cit.off, i, io) for i in run],
        "undetermined": None if value.usable else
                        "the pushed value is not statically determined here",
    }


def _written_lanes(insn, writes):
    """Which byte lanes an instruction writes -- an 8-bit `mov al,..` writes
    only lane 0, which is why `xor ah,ah` has to follow it."""
    op = insn.opcode
    lanes = set()
    for reg in writes:
        if 0xB0 <= op < 0xB8:
            lanes.add((reg, 1 if _REG8_IS_HIGH[op - 0xB0] else 0))
        elif op == 0xA0:
            lanes.add((reg, 0))
        elif op in (0x88, 0x8A) or (op in (0x00, 0x02, 0x28, 0x2A, 0x30, 0x32,
                                           0x20, 0x22, 0x08, 0x0A)
                                    and insn.modrm is not None):
            idx = (insn.modrm >> 3) & 7 if op in (0x8A, 0x02, 0x2A, 0x32,
                                                  0x22, 0x0A) else insn.modrm & 7
            lanes.add((reg, 1 if _REG8_IS_HIGH[idx] else 0))
        elif op == 0xF6:
            lanes.add((reg, 0))
            lanes.add((reg, 1))
        else:
            lanes.add((reg, 0))
            lanes.add((reg, 1))
    return lanes


# --- xrefs-to ----------------------------------------------------------------

def _parse_data_target(text):
    """`20ae:3b74`, or a bare `0x3b74` / `3b74` DGROUP offset."""
    t = text.strip()
    if ":" in t:
        cit = addr.citation(t)
        if cit.seg != addr.DATA_SEG_GHIDRA:
            raise ValueError(
                "%s is not a DGROUP address; xrefs-to takes a `%04x:xxxx` "
                "citation or a bare DGROUP offset" % (t, addr.DATA_SEG_GHIDRA))
        return cit.off
    return int(t, 16) if not t.lower().startswith("0x") else int(t, 16)


def xrefs_to(prog, text, scan_lo=0, scan_hi=None):
    data_off = _parse_data_target(text)
    target_cit = "%04x:%04x" % (addr.DATA_SEG_GHIDRA, data_off)
    needle = data_off.to_bytes(2, "little")
    scan_hi = addr.DATA_SEG_IMAGE_OFF if scan_hi is None else scan_hi

    # --- what the Ghidra export claims, re-checked against the bytes --------
    export_claims, export_ok, export_rejected = [], [], []
    for f in prog.functions:
        for x in f.get("data_xrefs", []):
            if x["to"] != target_cit:
                continue
            export_claims.append(dict(x, function=f["name"]))
    for claim in export_claims:
        io = addr.image_off_of_citation(claim["at"])
        try:
            insn = dis16.decode(prog.image, io)
        except dis16.DecodeError as e:
            export_rejected.append(dict(claim, why="does not decode: %s" % e))
            continue
        hit = next((o for o in insn.operands
                    if o.kind in ADDRESS_OPERANDS and o.value == data_off), None)
        if hit is None:
            export_rejected.append(dict(
                claim, why="the instruction there is `%s` (%s), which carries "
                           "no operand equal to 0x%x"
                           % (insn.text, insn.hex(), data_off)))
        else:
            export_ok.append(dict(claim, kind=hit.kind, text=insn.text,
                                  bytes=insn.hex()))

    # --- the byte scan, with the straddle filter ----------------------------
    raw_hits = []
    pos = prog.image.find(needle, scan_lo, scan_hi)
    while pos != -1:
        raw_hits.append(pos)
        pos = prog.image.find(needle, pos + 1, scan_hi)

    accepted, discarded = [], []
    for h in raw_hits:
        insns, anchor = prog.anchored_stream(h)
        cover = dis16.instruction_covering(insns, h)
        if cover is None:
            discarded.append({"image_off": "0x%x" % h,
                              "file_off": "0x%x" % addr.file_off_of_image_off(h),
                              "why": "no aligned decode covers it (anchor: %s)"
                                     % anchor.get("kind")})
            continue
        operand = cover.operand_at(h, size=2)
        if operand is None or operand.kind not in ADDRESS_OPERANDS:
            if cover.end <= h + 2:
                why = ("the word straddles `%s` (%s) and the instruction after "
                       "it -- it is not one field" % (cover.text, cover.hex()))
            elif operand is None:
                why = ("it falls inside `%s` (%s) but not at the start of an "
                       "operand field" % (cover.text, cover.hex()))
            else:
                why = ("it is the %s field of `%s` (%s), which is not an "
                       "address operand" % (operand.kind, cover.text, cover.hex()))
            discarded.append({"image_off": "0x%x" % h,
                              "file_off": "0x%x" % addr.file_off_of_image_off(h),
                              "anchor": anchor.get("kind"),
                              "function": (prog.function_containing(h) or {}).get("name"),
                              "why": why})
            continue
        accepted.append({
            "at": _citation_for(prog, cover.off),
            "image_off": "0x%x" % cover.off,
            "hit_image_off": "0x%x" % h,
            "operand_at": "0x%x" % operand.start,
            "file_off": "0x%x" % addr.file_off_of_image_off(cover.off),
            "function": (prog.function_containing(cover.off) or {}).get("name"),
            "operand": operand.kind,
            "bytes": cover.hex(),
            "text": cover.text,
            "anchor": anchor.get("kind"),
        })

    source = "ghidra-export" if export_ok else "byte-scan"
    return {
        "target": target_cit,
        "dgroup_off": "0x%x" % data_off,
        "operand_bytes": needle.hex(" "),
        "source": source,
        "export": {
            "available": bool(prog.functions) and
                         any("data_xrefs" in f for f in prog.functions),
            "claims": len(export_claims),
            "verified": export_ok,
            "rejected": export_rejected,
        },
        "scan": {
            "range": "image 0x%x..0x%x" % (scan_lo, scan_hi),
            "raw_hits": len(raw_hits),
            "accepted": accepted,
            "discarded": discarded,
        },
        "note": ("An accepted hit is a 2-byte operand FIELD of an aligned "
                 "instruction whose value equals the target -- that rules out "
                 "straddles, not coincidence: an `imm16` hit may be a constant "
                 "that happens to equal the address, so read the `operand` "
                 "kind. `disp16`/`disp16x`/`moffs16` are memory operands and "
                 "really do address it. Which segment register applies is not "
                 "decidable from the bytes, so a DGROUP answer still rests on "
                 "DS holding %04x." % addr.DATA_SEG_GHIDRA),
    }


def _citation_for(prog, image_off):
    """Label an image offset the way the rest of the repo would write it.

    The segment is taken from the containing function when the export knows
    one -- guessing a segment from the offset alone is exactly the kind of
    bookkeeping this module exists to stop.
    """
    f = prog.function_containing(image_off)
    if f is not None:
        seg, off = addr.parse_citation(f["entry"])
        base = addr.image_off_of_citation(f["entry"]) - off
        return "%04x:%04x" % (seg, image_off - base)
    if image_off < 0x10000:
        return "%04x:%04x" % (addr.GHIDRA_BASE_SEG, image_off)
    return "image+0x%x" % image_off


# --- CLI ---------------------------------------------------------------------

def _print(obj, as_json):
    if as_json:
        print(json.dumps(obj, indent=2))
    else:
        print(_human(obj))


def _human(obj, indent=0):
    pad = "  " * indent
    if isinstance(obj, dict):
        lines = []
        for k, v in obj.items():
            if isinstance(v, (dict, list)) and v:
                lines.append("%s%s:" % (pad, k))
                lines.append(_human(v, indent + 1))
            else:
                lines.append("%s%s: %s" % (pad, k, v))
        return "\n".join(lines)
    if isinstance(obj, list):
        return "\n".join(_human(v, indent) if isinstance(v, (dict, list))
                         else "%s- %s" % (pad, v) for v in obj)
    return "%s%s" % (pad, obj)


def main(argv=None):
    p = argparse.ArgumentParser(
        prog="re_query.py", description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--exe", default=None, help="default: orig/g.exe")
    p.add_argument("--functions", default=str(DEFAULT_FUNCTIONS))
    p.add_argument("--json", action="store_true")
    # `--json` is accepted on either side of the subcommand; SUPPRESS stops the
    # subparser's default from clobbering a flag given before it.
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--json", action="store_true", default=argparse.SUPPRESS)
    sub = p.add_subparsers(dest="cmd", required=True)

    r = sub.add_parser("resolve", parents=[common],
                       help="citation -> file/image offset + bytes")
    r.add_argument("citation")
    r.add_argument("-n", "--bytes", type=int, default=16)

    c = sub.add_parser("is-call-site", parents=[common],
                         help="alignment AND identity, separately")
    c.add_argument("citation")
    c.add_argument("--signature", default=RANDOM_CALL_BYTES.hex())

    n = sub.add_parser("pushed-n", parents=[common],
                         help="what `n` the idiom before a call pushes")
    n.add_argument("citation")
    n.add_argument("--signature", default=RANDOM_CALL_BYTES.hex())

    x = sub.add_parser("xrefs-to", parents=[common],
                         help="who references a DGROUP address")
    x.add_argument("address", help="e.g. 20ae:3b74, or a bare 3b74")

    a = p.parse_args(argv)
    prog = Program(a.exe, a.functions)
    if a.cmd == "resolve":
        _print(resolve(prog, a.citation, a.bytes), a.json)
    elif a.cmd == "is-call-site":
        _print(is_call_site(prog, a.citation,
                            bytes.fromhex(a.signature.replace(" ", ""))), a.json)
    elif a.cmd == "pushed-n":
        _print(pushed_n(prog, a.citation,
                        bytes.fromhex(a.signature.replace(" ", ""))), a.json)
    elif a.cmd == "xrefs-to":
        _print(xrefs_to(prog, a.address), a.json)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (addr.AddressError, ValueError) as exc:
        print("error: %s" % exc, file=sys.stderr)
        sys.exit(2)
