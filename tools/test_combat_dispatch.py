#!/usr/bin/env python3
"""`data/combat_dispatch.json` re-derived from `orig/g.exe`, claim by claim.

The artifact and `docs/re/combat-dispatch.md` are the two places the same
claims live; this is what stops them drifting from the binary they describe.
Nothing here reads `src/`, a screen, or Ghidra's C.

Every address in the artifact is checked two ways, because they are different
signals and only the second one settles anything (`docs/re/METHODOLOGY.md`,
"Is this address a call site?"):

  * **alignment** -- the address is reached by decoding forward from its
    enclosing function's entry, so it is a real instruction boundary and not a
    byte-scan hit in the middle of one;
  * **identity** -- the instruction decoded there says what the artifact says
    it says.

Beyond that, the claims this document makes that a per-address check cannot
reach are re-derived as SCANS over the image, each with a companion test that
runs the same scan over a doctored copy so an empty answer is a measurement:

  * the verb set is every `rtl_str_compare` site in the function -- so
    "the dispatcher accepts exactly these" is closure, not a list;
  * the buffer's reference set inside the function is exactly twelve -- so
    nothing else can be reading it;
  * `FUN_1000_1348` touches no address in the player's record -- so "it is the
    ENEMY's sheet" does not rest on reading the messages;
  * `1000:4e2a` has no second entry -- so "unreachable" is not "I did not find
    one".

    python3 tools/test_combat_dispatch.py
"""
import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import addr as addrmod            # noqa: E402
import dis16                      # noqa: E402
from re_derive import (CITE, aligned_boundaries, far_calls_to,  # noqa: E402
                       inline_spans, load_image, near_calls_to, strip_fences)

REPO = Path(__file__).resolve().parents[1]
ART = REPO / "data" / "combat_dispatch.json"
BRANCHES = REPO / "data" / "branches.json"
DOC = REPO / "docs" / "re" / "combat-dispatch.md"

RANDOM = bytes.fromhex("9a4b11780f")          # call far 0f78:114b -- System.Random
STR_COMPARE = bytes.fromhex("9ad80b780f")     # call far 0f78:0bd8 -- rtl_str_compare
BUFFER = 0x3A72                               # 20ae:3a72, the combat prompt's buffer
#: `mov di,0x3a72` (3) / `push ds` (1) / `push di` (1) sits this far ahead of
#: the token push in every one of the nine compare setups.
BUFFER_SETUP_BACK = 5

# `1000:5080` is the EXCLUSIVE end of the dispatcher range and `1000:5078` the
# exclusive end of the death block; both are one past the block they close.
# `1000:5080` happens to be a boundary as well and is left to the general check;
# only an address that is NOT one belongs here, and there is none, so this map
# is empty on purpose.  It stays as the place a future exemption must be
# argued rather than absorbed into a tolerance.
NOT_A_BOUNDARY = {}


def find_bytes(img, sig, lo, hi):
    """Every offset in `[lo, hi)` holding `sig`.  Byte identity, not alignment."""
    out, i = [], lo
    while True:
        i = img.find(sig, i, hi)
        if i < 0:
            return out
        out.append("1000:%04x" % i)
        i += 1


def branch_targets(insns):
    """`{target_off: [(site, mnemonic), ...]}` for every direct jump."""
    out = {}
    for x in insns:
        m = re.match(r"^(j[a-z]+|jmp|jmp short)\s+0x([0-9a-f]+)$", x.text)
        if m:
            out.setdefault(int(m.group(2), 16), []).append(
                ("1000:%04x" % x.off, m.group(1)))
    return out


class DispatchTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.art = json.loads(ART.read_text(encoding="utf-8"))
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        fn = {f["entry"]: f for f in cls.branches["functions"]}
        cls.fn = fn["1000:3d11"]
        cls.fnrec = fn
        cls.entry = addrmod.image_off_of_citation("1000:3d11")
        cls.body = list(dis16.decode_run(cls.img, cls.entry,
                                         cls.entry + cls.fn["size"]))
        # The scan window gaps.md uses: entry to the NEXT segment-1000 function
        # entry, deliberately wider than the `size` span so the count does not
        # depend on reading `size` as one.
        ents = sorted(addrmod.image_off_of_citation(f["entry"])
                      for f in cls.branches["functions"] if f["seg"] == "1000")
        cls.next_entry = ents[ents.index(cls.entry) + 1]

    # ---------------------------------------------------------------- helpers
    # Minimum-count guards below are floors with deliberate headroom, not the
    # current value: the artifact carries 296 `{addr, text}` records and 56
    # literals, and the prose 260 citations, 116 instruction spans, 37 CS
    # offsets and 35 text/CS pairs.  A guard set AT the current number cannot
    # fail in the direction that matters (something stopped being checked) --
    # it only fails when a record is added.
    def at(self, cit):
        self.assertIn(cit, self.aligned,
                      "%s is not an instruction boundary reached by decoding "
                      "forward from any enclosing function's entry -- the "
                      "citation is a byte-scan hit, not an address" % cit)
        return self.aligned[cit]

    def check_insn(self, rec, where):
        ins = self.at(rec["addr"])
        self.assertEqual(
            ins.text, rec["text"],
            "%s: data/combat_dispatch.json says %s at %s, orig/g.exe decodes "
            "%s there" % (where, rec["text"], rec["addr"], ins.text))
        return ins

    def cs_literal(self, cs_offset):
        off = int(cs_offset, 16)
        n = self.img[off]
        return self.img[off + 1:off + 1 + n].decode("cp866")

    def _walk(self, key_a, key_b):
        def rec(node, path):
            if isinstance(node, dict):
                if isinstance(node.get(key_a), str) \
                        and isinstance(node.get(key_b), str):
                    yield node, path
                for k, v in node.items():
                    yield from rec(v, "%s.%s" % (path, k))
            elif isinstance(node, list):
                for i, v in enumerate(node):
                    yield from rec(v, "%s[%d]" % (path, i))
        return list(rec(self.art, "$"))

    def off(self, cit):
        return addrmod.image_off_of_citation(cit)

    # ------------------------------------------------------------------ tests
    def test_the_recorded_function_extents_match_the_branch_catalogue(self):
        """The two `size_bytes` the decodes run on are not written down twice."""
        self.assertEqual(self.art["function"]["size_bytes"], self.fn["size"])
        es = self.art["enemy_sheet"]
        self.assertEqual(es["size_bytes"], self.fnrec[es["entry"]]["size"])
        for rec in es["epilogue"]:
            self.check_insn(rec, "enemy sheet epilogue")
        last = es["epilogue"][-1]
        self.assertEqual(self.off(last["addr"]) + 1,
                         self.off(es["entry"]) + es["size_bytes"],
                         "the recorded `ret` is not the last byte of the "
                         "recorded extent")

    def test_every_cited_instruction_decodes_to_what_the_artifact_says(self):
        seen = self._walk("addr", "text")
        self.assertGreater(len(seen), 150,
                           "the artifact carries only %d instruction records; "
                           "a walk that finds almost nothing must not pass"
                           % len(seen))
        for node, path in seen:
            self.check_insn(node, path)

    def test_every_cs_literal_decodes_to_the_recorded_text(self):
        seen = self._walk("cs_offset", "text")
        self.assertGreater(len(seen), 45,
                           "the literal walk found only %d records" % len(seen))
        for node, path in seen:
            self.assertEqual(
                self.cs_literal(node["cs_offset"]), node["text"],
                "%s: the Pascal shortstring at CS %s is not what the artifact "
                "records" % (path, node["cs_offset"]))

    # -- the verb set ------------------------------------------------------
    def test_the_verb_table_is_every_string_compare_in_the_function(self):
        """Closure, not a list: the nine ARE all of them."""
        found = find_bytes(self.img, STR_COMPARE, self.entry, self.next_entry)
        self.assertGreater(self.next_entry, self.entry + self.fn["size"],
                           "the scan window is not wider than the `size` span, "
                           "so the count depends on reading `size` as one")
        recorded = [v["compare"]["addr"] for v in self.art["verbs"]]
        self.assertEqual(sorted(found), sorted(recorded),
                         "the scan finds string compares the artifact does not "
                         "list (or the reverse): %r vs %r" % (found, recorded))
        self.assertEqual(len(recorded), 9)

    def test_each_compare_pushes_the_buffer_and_the_token_the_artifact_names(self):
        by_off = {x.off: x for x in self.body}
        for v in self.art["verbs"]:
            where = "verb %r at %s" % (v["token"], v["compare"]["addr"])
            self.check_insn(v["compare"], where)
            push = self.check_insn(v["literal_push"], where)
            self.assertEqual(
                [o.value for o in push.operands if o.kind == "imm16"],
                [int(v["literal"]["cs_offset"], 16)],
                "%s: the literal push does not push %s"
                % (where, v["literal"]["cs_offset"]))
            # ... and the buffer is pushed five bytes earlier, in the same
            # idiom: `mov di,0x3a72` (3) / `push ds` (1) / `push di` (1).
            setup = self.off(v["literal_push"]["addr"]) - BUFFER_SETUP_BACK
            self.assertIn(setup, by_off, "%s: no aligned instruction six bytes "
                                         "before the literal push" % where)
            self.assertEqual(
                [o.value for o in by_off[setup].operands if o.kind == "imm16"],
                [BUFFER],
                "%s: the compare's first argument is %r, not the command "
                "buffer" % (where, by_off[setup].text))

    def test_every_verb_arm_is_where_its_branch_sends_it(self):
        spans = []
        for v in self.art["verbs"]:
            where = "verb %r" % v["token"]
            branch = self.check_insn(v["branch"], where)
            start, end = (self.off(a) for a in v["arm"])
            self.assertLess(start, end, "%s: empty arm" % where)
            target = int(branch.text.split()[-1], 16)
            if v["branch"]["arm"] == "target":
                self.assertEqual(target, start,
                                 "%s: the branch goes to 0x%04x, not to the "
                                 "arm it names" % (where, target))
            else:
                self.assertEqual(branch.end, start,
                                 "%s: the fallthrough arm does not start at "
                                 "the branch's end" % where)
                self.assertEqual(target, end,
                                 "%s: the branch skips to 0x%04x, not past the "
                                 "arm it names" % (where, target))
            first = self.check_insn(v["first_instruction"], where)
            self.assertTrue(start <= first.off < end,
                            "%s: the arm's first instruction is outside the "
                            "arm" % where)
            self.assertEqual(first.off, start)
            spans.append((start, end, v["token"]))
        spans.sort()
        for (a0, a1, ta), (b0, b1, tb) in zip(spans, spans[1:]):
            self.assertLessEqual(a1, b0, "arms %r and %r overlap" % (ta, tb))

    def test_the_buffer_reference_set_inside_the_function_is_closed(self):
        found = []
        for x in self.body:
            if any(o.value == BUFFER and o.kind in ("imm16", "disp16", "moffs16")
                   for o in x.operands):
                found.append("1000:%04x" % x.off)
        recorded = [s["addr"] for s in self.art["buffer_references"]["sites"]]
        self.assertEqual(sorted(found), sorted(recorded),
                         "the function reads 20ae:3a72 at addresses the "
                         "artifact does not account for (or the reverse): %r "
                         "vs %r" % (found, recorded))
        self.assertEqual(len(recorded), 12)
        roles = {}
        for site in self.art["buffer_references"]["sites"]:
            self.check_insn(site, "buffer reference")
            roles.setdefault(site["role"], []).append(site["addr"])
        self.assertEqual(
            sorted(roles["compare_setup"]),
            sorted("1000:%04x" % (self.off(v["literal_push"]["addr"])
                                  - BUFFER_SETUP_BACK)
                   for v in self.art["verbs"]),
            "the sites labelled `compare_setup` are not the nine that precede "
            "a token push")
        self.assertEqual(sorted(roles),
                         ["case_fold", "compare_setup", "readln_destination",
                          "subroutine_call"])

    def test_only_one_near_call_in_the_function_receives_the_buffer(self):
        got = []
        for i, x in enumerate(self.body):
            if x.raw[:1] != b"\xe8":
                continue
            window = [y.text for y in self.body[max(0, i - 6):i]]
            if "mov di,0x%x" % BUFFER in window:
                disp = int.from_bytes(x.raw[1:3], "little", signed=True)
                got.append(("1000:%04x" % x.off, "1000:%04x" % ((x.end + disp) & 0xFFFF)))
        sub = self.art["subroutine_verbs"]
        self.assertEqual(got, [(sub["call"]["addr"], "1000:29c4")],
                         "the buffer reaches a near call the artifact does not "
                         "account for (or the reverse): %r" % (got,))
        # ... and the eight compares inside it really are compares.
        for cit in sub["compare_sites"]:
            self.assertEqual(self.at(cit).raw, STR_COMPARE,
                             "%s is not a string compare" % cit)
        for lit in sub["literals"]:
            self.assertEqual(self.cs_literal(lit["cs_offset"]), lit["text"])

    def test_the_verbs_the_dispatcher_does_not_accept_are_never_compared(self):
        pushed = {o.value for x in self.body for o in x.operands
                  if o.kind == "imm16"}
        for row in self.art["not_in_combat"]["verbs"]:
            off = int(row["literal"]["cs_offset"], 16)
            self.assertEqual(self.cs_literal(row["literal"]["cs_offset"]),
                             row["literal"]["text"])
            self.assertNotIn(
                off, pushed,
                "%r's literal at CS 0x%04x IS materialised somewhere in "
                "FUN_1000_3d11, so `the fight prompt never compares it` is "
                "wrong" % (row["token"], off))
            # ... and where it IS compared is not this function.
            site = self.at(row["compared_at"])
            self.assertEqual(site.raw, STR_COMPARE)
            self.assertFalse(self.entry <= site.off < self.entry + self.fn["size"])

    # -- FUN_1000_1348, the `sv` handler -----------------------------------
    def test_sv_calls_the_enemy_sheet_and_it_is_the_only_caller(self):
        es = self.art["enemy_sheet"]
        target = self.off(es["entry"])
        sv = [v for v in self.art["verbs"] if v["token"] == "sv"][0]
        call = self.at(sv["first_instruction"]["addr"])
        self.assertEqual(call.raw[0], 0xE8)
        disp = int.from_bytes(call.raw[1:3], "little", signed=True)
        self.assertEqual("1000:%04x" % ((call.end + disp) & 0xFFFF), es["entry"])
        near = near_calls_to(self.img, target)
        self.assertEqual(sorted(near), sorted(es["called_from"]),
                         "the byte scan finds near calls to the enemy sheet "
                         "the artifact does not list (or the reverse): %r"
                         % near)
        self.assertEqual(far_calls_to(self.img, target), [])

    def test_the_enemy_sheet_takes_no_parameters(self):
        es = self.art["enemy_sheet"]
        entry = self.off(es["entry"])
        body = dis16.decode_run(self.img, entry, entry + es["size_bytes"])
        self.assertEqual(body[-1].raw, b"\xc3",
                         "the last instruction is %r, not a bare `ret`"
                         % body[-1].text)
        self.assertEqual(es["parameter_bytes"], 0)
        positive = [("1000:%04x" % i.off, i.text) for i in body
                    if re.search(r"bp\+0x", i.text)]
        self.assertEqual(positive, [],
                         "the enemy sheet reads a positive bp displacement, so "
                         "it DOES take a parameter: %r" % positive[:5])

    def test_the_enemy_sheet_touches_no_address_in_the_players_record(self):
        """`it is the ENEMY's sheet` from operands, not from the messages."""
        es = self.art["enemy_sheet"]
        entry = self.off(es["entry"])
        body = list(dis16.decode_run(self.img, entry, entry + es["size_bytes"]))
        lo, hi = 0x3690, 0x3951          # 20ae:3690..20ae:3951, up to the enemy record
        bad = [("1000:%04x" % x.off, x.text) for x in body for o in x.operands
               if o.kind in ("disp16", "moffs16", "imm16") and lo <= o.value <= hi]
        self.assertEqual(bad, [],
                         "the enemy sheet reads the player's side of DGROUP: %r"
                         % bad[:5])
        # ... and it really does read the enemy record, at the recorded sites.
        enemy = set()
        for r in es["reads"]:
            self.check_insn(r, "enemy sheet read")
            enemy.add(r["ds"])
        record = {d for d in enemy if 0x3952 <= int(d.split(":")[1], 16) <= 0x3969}
        self.assertGreaterEqual(len(record), 12,
                                "only %d distinct enemy-record fields recorded"
                                % len(record))

    def test_the_second_blow_block_reads_the_enemy_agility(self):
        ab = self.art["enemy_sheet"]["accuracy_block"]
        load = self.check_insn(ab["load"], "accuracy load")
        self.assertIn("0x3956", load.text)
        for key in ("guard", "second_blow", "multi_blow"):
            ins = self.check_insn(ab[key], "accuracy %s" % key)
            lo, hi = (self.off(a) for a in ab["range"])
            self.assertTrue(lo <= ins.off < hi,
                            "%s is outside the accuracy block" % key)

    # -- the four draws ----------------------------------------------------
    def test_the_four_random_sites_are_the_only_ones_in_the_range(self):
        lo, hi = 0x4900, 0x5080
        found = find_bytes(self.img, RANDOM, lo, hi)
        recorded = [r["addr"] for r in self.art["random_sites"]]
        self.assertEqual(sorted(found), sorted(recorded),
                         "the scan of [0x%04x, 0x%04x) finds %r, the artifact "
                         "lists %r" % (lo, hi, found, recorded))
        for r in self.art["random_sites"]:
            ins = self.check_insn(r, "random site")
            self.assertEqual(ins.raw, RANDOM)
            push = self.check_insn(r["n_push"], "random argument")
            self.assertEqual(push.end, ins.off,
                             "%s: the recorded push is not the instruction "
                             "immediately before the call" % r["addr"])
            self.assertEqual(push.raw, b"\x50", "the argument is not `push ax`")

    def test_the_flee_arm_and_the_death_block_draw_nothing(self):
        for key, node in (("flee", self.art["flee"]),
                          ("death_and_hospital", self.art["death_and_hospital"])):
            self.assertEqual(node["draws"], [])
        flee = [v for v in self.art["verbs"] if v["token"] == "run"][0]["arm"]
        for lo, hi in (tuple(self.off(a) for a in flee),
                       tuple(self.off(a) for a in
                             self.art["death_and_hospital"]["range"])):
            self.assertEqual(
                find_bytes(self.img, RANDOM, lo, hi), [],
                "a Random call site exists in [0x%04x, 0x%04x)" % (lo, hi))

    # -- the unreachable line ----------------------------------------------
    def test_the_unreachable_line_has_no_second_entry(self):
        u = self.art["backup"]["unreachable"]
        self.check_insn(u["literal_push"], "unreachable line")
        self.assertEqual(self.cs_literal(u["literal"]["cs_offset"]),
                         u["literal"]["text"])
        lo, hi = (self.off(a) for a in u["no_entry_range"])
        self.assertTrue(lo <= self.off(u["literal_push"]["addr"]) < hi)
        targets = branch_targets(self.body)
        inside = {t: v for t, v in targets.items() if lo <= t < hi}
        self.assertEqual(inside, {},
                         "something jumps into [0x%04x, 0x%04x), so the block "
                         "has an entry other than the fall-through the "
                         "unreachability argument assumes: %r"
                         % (lo, hi, inside))
        # The guard that dominates the fall-through, and the test that can then
        # never be equal.
        guard = self.check_insn(u["dominating_guard"], "dominating guard")
        self.check_insn(u["dominating_branch"], "dominating branch")
        test = self.check_insn(u["test"], "the equality test")
        self.check_insn(u["branch"], "the equality branch")
        inc = self.check_insn(self.art["backup"]["assist"]["attrition"]["inc"],
                              "the increment between them")
        floor = int(guard.text.split(",")[-1], 16)
        want = int(test.text.split(",")[-1], 16)
        self.assertEqual(floor, want,
                         "the guard admits >= %d and the test wants == %d; the "
                         "unreachability argument needs them equal" % (floor, want))
        self.assertTrue(guard.off < inc.off < test.off,
                        "the increment is not between the guard and the test")

    # -- the flags ---------------------------------------------------------
    def refs_to(self, ds_addr):
        """Every aligned instruction with `ds_addr` in a memory-operand field."""
        found = []
        for fn in self.branches["functions"]:
            if fn["seg"] != "1000":
                continue
            start = addrmod.image_off_of_citation(fn["entry"])
            for x in dis16.decode_run(self.img, start, start + fn["size"]):
                if any(o.kind in ("disp16", "moffs16") and o.value == ds_addr
                       for o in x.operands):
                    found.append("1000:%04x" % x.off)
        return found

    def test_the_wander_toggle_has_a_third_reader_in_combat(self):
        """`docs/re/gaps.md` named two readers; the pistol arm is a third."""
        f = self.art["ds_flags"]["20ae:3693"]
        found = self.refs_to(0x3693)
        recorded = [r["addr"] for r in f["references"]]
        self.assertEqual(sorted(found), sorted(recorded),
                         "20ae:3693 is referenced at %r, the artifact records "
                         "%r" % (found, recorded))
        self.assertEqual(f["reference_count"], len(recorded))
        by_fn = {}
        for r in f["references"]:
            self.check_insn(r, "3693 reference")
            by_fn.setdefault(r["in_function"], []).append(r["addr"])
        self.assertEqual(by_fn.get("FUN_1000_3d11"), ["1000:4ebc"],
                         "the combat reader that makes this a finding is not "
                         "where the artifact puts it")
        self.assertEqual(sorted(by_fn.get("FUN_1000_0d14", [])),
                         ["1000:0d86", "1000:0e54"],
                         "the two readers gaps.md named have moved")
        # ... and it really is the pistol arm's gate.
        gate = self.art["pistol"]["allowed_here"]["guard"]
        self.assertEqual(gate["addr"], "1000:4ebc")

    def test_the_rector_flag_reference_set_is_exactly_what_is_recorded(self):
        f = self.art["ds_flags"]["20ae:3c83"]
        found = self.refs_to(0x3C83)
        recorded = ([w["addr"] for w in f["writes"]]
                    + [r["addr"] for r in f["reads"]])
        self.assertEqual(sorted(found), sorted(recorded),
                         "20ae:3c83 is referenced at %r, the artifact records "
                         "%r" % (found, recorded))
        self.assertEqual(f["reference_count"], len(recorded))
        for w in f["writes"]:
            ins = self.check_insn(w, "3c83 write")
            self.assertIn(",0x1", ins.text,
                          "%s does not store 1, so `never cleared` is wrong"
                          % w["addr"])
            self.assertEqual(self.cs_literal(w["literal"]["cs_offset"]),
                             w["literal"]["text"])
        for r in f["reads"]:
            self.check_insn(r, "3c83 read")
        self.assertTrue(f["never_cleared"])
        # `1000:ae18` tests what `1000:ae13` stored, with nothing between them
        # that could change it -- which is why the two rector fights are
        # unconditional.
        store, test = f["writes"][1], f["reads"][0]
        self.assertEqual(self.at(store["addr"]).end, self.off(test["addr"]))
        for c in f["rector_fight"]:
            ins = self.check_insn(c, "rector fight call")
            disp = int.from_bytes(ins.raw[1:3], "little", signed=True)
            self.assertEqual("1000:%04x" % ((ins.end + disp) & 0xFFFF), "1000:3d11")
            kind = self.at("1000:%04x" % (ins.off - 3))
            self.assertEqual(kind.text, "mov al,0x%x" % c["opponent_kind"],
                             "%s is preceded by %r, not the kind push the "
                             "artifact records" % (c["addr"], kind.text))
        self.check_insn(f["kind_4_is_the_rector"], "the kind-4 test")

    def test_the_den_flag_store_in_the_flee_arm_opens_the_den(self):
        d = self.art["flee"]["den_grant"]
        store = self.check_insn(d["store"], "flee den store")
        self.assertIn(",0x1", store.text)
        arm = [v for v in self.art["verbs"] if v["token"] == "run"][0]["arm"]
        lo, hi = (self.off(a) for a in arm)
        self.assertTrue(lo <= store.off < hi,
                        "the store is not inside the flee arm")
        # Every immediate store to 20ae:3696 in the image is 0 or 1, so the
        # flag is a boolean and `1` is the value 1000:d80c admits.
        stores = []
        for fn in self.branches["functions"]:
            if fn["seg"] != "1000":
                continue
            start = addrmod.image_off_of_citation(fn["entry"])
            for x in dis16.decode_run(self.img, start, start + fn["size"]):
                if x.text.startswith("mov byte [0x3696],"):
                    stores.append(int(x.text.split(",")[-1], 16))
        self.assertTrue(stores, "no store to 20ae:3696 found at all")
        self.assertEqual(set(stores), {0, 1},
                         "20ae:3696 takes values %r, so reading it as a boolean "
                         "is wrong" % sorted(set(stores)))
        gate = self.at("1000:d80c")
        self.assertEqual(gate.text, "cmp byte [0x3696],0x1",
                         "the den gate is %r, so `1 opens the den` needs "
                         "re-deriving" % gate.text)
        # ... and the post-kill twin uses the same expression with `jl`.
        twin = self.check_insn(d["post_kill_twin"], "post-kill twin")
        self.check_insn(d["post_kill_twin"]["branch"], "post-kill twin branch")
        self.assertEqual(twin.text, self.at(d["test"]["addr"]).text,
                         "the flee test and the post-kill test no longer "
                         "compare the same constant")
        self.assertEqual(self.at(d["branch"]["addr"]).text.split()[0], "jnz")
        self.assertEqual(self.at(d["post_kill_twin"]["branch"]["addr"])
                         .text.split()[0], "jl")

    # -- the hospital bill --------------------------------------------------
    def test_the_hospital_bill_ratio_needs_no_exponent_bias(self):
        b = self.art["death_and_hospital"]["hospital"]["bill"]
        for key in ("to_real", "divide", "multiply", "round", "debit",
                    "divisor_exponent", "divisor_mantissa_low",
                    "divisor_mantissa", "multiplier_exponent",
                    "multiplier_mantissa_low", "multiplier_mantissa"):
            self.check_insn(b[key], "bill %s" % key)
        e1 = int(b["divisor_exponent"]["text"].split(",")[-1], 16)
        e2 = int(b["multiplier_exponent"]["text"].split(",")[-1], 16)
        m1 = int(b["divisor_mantissa"]["text"].split(",")[-1], 16)
        m2 = int(b["multiplier_mantissa"]["text"].split(",")[-1], 16)
        # Borland's 6-byte real: `cl` is the exponent, `ch`+`si` the low
        # mantissa and `di` the high half, whose top bit is the sign with the
        # leading 1 implicit.  Reading each significand off `di` ALONE is only
        # valid because the low half is zeroed, so that is asserted rather than
        # assumed -- a non-zero `si` would change both significands and with
        # them the 0.6, and nothing else in this tree would notice.
        for key in ("divisor_mantissa_low", "multiplier_mantissa_low"):
            self.assertEqual(
                b[key]["text"], "xor si,si",
                "bill %s is %r, not the `xor si,si` that makes the low "
                "mantissa half zero; the significands cannot be read off `di` "
                "alone and the 0.6 does not follow" % (key, b[key]["text"]))
        # ... and each `xor si,si` really is between its exponent load and the
        # runtime call that consumes the pair.
        for lo, mid, hi in (("divisor_exponent", "divisor_mantissa_low", "divide"),
                            ("multiplier_exponent", "multiplier_mantissa_low",
                             "multiply")):
            self.assertTrue(
                self.off(b[lo]["addr"]) < self.off(b[mid]["addr"])
                < self.off(b[hi]["addr"]),
                "bill %s is not between %s and %s, so it may be zeroing `si` "
                "for something else" % (mid, lo, hi))
        # The mantissa's top bit is the sign; both constants are positive.
        self.assertEqual((m1 | m2) & 0x8000, 0,
                         "a mantissa's top bit is set, so one of the constants "
                         "is negative and the significand reading is wrong")
        sig = lambda w: 1.0 + ((w << 1) & 0xFFFF) / 0x10000
        ratio = (sig(m2) / sig(m1)) * 2.0 ** (e2 - e1)
        self.assertAlmostEqual(ratio, float(b["ratio"]), places=9,
                               msg="the recorded ratio is not what the two "
                                   "register loads compute")
        self.assertEqual(e1 - e2, 1,
                         "the exponents differ by %d steps, so the bias no "
                         "longer cancels the way the claim needs" % (e1 - e2))
        self.assertTrue(b["ratio_is_bias_free"])
        self.assertIsNone(b["decimal_value_of_either_constant"])

    def test_the_negative_purse_is_paid_out_of_cred_and_then_zeroed(self):
        n = self.art["death_and_hospital"]["hospital"]["negative_purse"]
        for k in ("test", "branch"):
            self.check_insn(n[k], "negative purse %s" % k)
        steps = [self.check_insn(x, "negative purse step") for x in n["steps"]]
        self.assertEqual(
            [x.text for x in steps],
            ["mov ax,[0x38cb]", "add ax,[0x38c7]", "mov [0x38cb],ax",
             "xor ax,ax", "mov [0x38c7],ax"],
            "the block is %r, which is not cred += purse then purse := 0"
            % [x.text for x in steps])
        # What sits between the two stores is the whole finding: exactly one
        # `xor ax,ax`, so the second store writes ZERO.  The first draft of
        # docs/re/combat-dispatch.md read a listing with that two-byte
        # instruction filtered out and reported a windfall that is not there;
        # this test is why it did not survive.
        between = [x.text for x in self.body
                   if steps[2].end <= x.off < steps[4].off]
        self.assertEqual(between, ["xor ax,ax"],
                         "the instructions between the two stores are %r; the "
                         "reading that `[0x38c7]` ends at zero rests on the "
                         "`xor` being there and being the only one" % between)

    # -- the two lanes -----------------------------------------------------
    def test_the_live_probe_agrees_with_the_static_verb_table(self):
        """Which verbs the breakpoint saw must be which verbs the code calls.

        The disassembly says exactly one typed verb reaches `1000:1348` -- `sv`
        at the `Битва\\` prompt -- so the predicted answer for any other line,
        at either prompt, is "does not reach".  The load-bearing negative is
        combat `s`: it is a verb that DOES call a function from this same chain
        (`1000:4c35` -> `1000:1a03`) and still must not enter the target, so
        the probe is separating two callees rather than separating "typed
        something" from "typed nothing".
        """
        probe = self.art["live_probe"]
        target = probe["target"]
        reach = {v["token"] for v in self.art["verbs"]
                 if v.get("calls") == target}
        self.assertEqual(reach, {"sv"})
        self.assertGreaterEqual(len(probe["per_verb"]), 5,
                                "the probe covers too few verbs to separate "
                                "`sv reaches it` from `everything does`")
        self.assertTrue(any(not v["reaches"] for v in probe["per_verb"]),
                        "the probe recorded no negative at all")
        self.assertTrue(any(v["reaches"] for v in probe["per_verb"]))
        self.assertIn(("combat", "s"),
                      [(v["prompt"], v["line"]) for v in probe["per_verb"]],
                      "the probe never typed `s` at the combat prompt, which "
                      "is the negative that makes it an experiment")
        for v in probe["per_verb"]:
            predicted = v["prompt"] == "combat" and v["line"] in reach
            self.assertEqual(
                predicted, v["reaches"],
                "%r typed at the %s prompt: the disassembly predicts "
                "reaches=%s (the only verb whose arm calls %s is `sv`) and the "
                "breakpoint observed reaches=%s over %d prompt(s)"
                % (v["line"], v["prompt"], predicted, target, v["reaches"],
                   v["prompts"]))
            self.assertEqual(v["reaches"], v["entries"] > 0)
        self.assertEqual(probe["marker_stream"].count("T"),
                         sum(v["entries"] for v in probe["per_verb"]))
        self.assertTrue(probe["runs_agree"])

    def test_the_branch_partition_is_the_whole_range(self):
        part = self.art["branch_partition"]
        lo, hi = (self.off(a) for a in part["range"])
        B = [b for b in self.branches["branches"]
             if b["func"] == "FUN_1000_3d11"
             and lo <= int(b["addr"].split(":")[1], 16) < hi]
        self.assertEqual(part["total"], len(B))
        # The partition node itself is EXCLUDED from the scan: leaving it in
        # would make the check agree with a recomputation its own contents
        # produced.
        without = {k: v for k, v in self.art.items() if k != "branch_partition"}
        cited = {m.group(0).lower()
                 for m in CITE.finditer(json.dumps(without, ensure_ascii=False))}
        want_cited, want_uncited = [], []
        for b in B:
            hit = (b["addr"].lower() in cited
                   or bool(b["guard"] and b["guard"]["addr"].lower() in cited))
            (want_cited if hit else want_uncited).append(b["addr"])
        self.assertEqual(part["cited"], want_cited,
                         "the recorded cited set is not what the artifact "
                         "actually cites")
        self.assertEqual(part["uncited"], want_uncited,
                         "the recorded uncited set is not what is actually "
                         "left uncited")
        self.assertEqual(set(part["cited"]) & set(part["uncited"]), set())


class ScanTest(unittest.TestCase):
    """The scans, shown able to find something.

    Three of the assertions above pass by returning the EMPTY list over the
    shipped image -- no far call to the enemy sheet, no `Random` in the flee
    arm, no jump into the unreachable block. Over that image alone each of them
    passes whether the scan works or not, so each is run again over a doctored
    copy that really does contain what it looks for.
    """

    def setUp(self):
        self.img = load_image()

    def test_the_random_scan_finds_the_four_it_should(self):
        self.assertEqual(find_bytes(self.img, RANDOM, 0x4900, 0x5080),
                         ["1000:4db7", "1000:4e16", "1000:4ef5", "1000:4f18"])

    def test_the_random_scan_finds_a_planted_call_in_the_flee_arm(self):
        doctored = bytearray(self.img)
        doctored[0x4990:0x4995] = RANDOM
        self.assertEqual(find_bytes(bytes(doctored), RANDOM, 0x48EB, 0x4AFB),
                         ["1000:4990"])

    def test_the_compare_scan_finds_a_planted_compare(self):
        doctored = bytearray(self.img)
        doctored[0x4990:0x4995] = STR_COMPARE
        self.assertIn("1000:4990",
                      find_bytes(bytes(doctored), STR_COMPARE, 0x3D11, 0x5F55))

    def test_the_far_scan_finds_a_planted_call_to_the_enemy_sheet(self):
        for seg in (0x0000, 0x0134, 0x0F78):
            doctored = bytearray(self.img)
            doctored[0x0100:0x0105] = (b"\x9a" + (0x1348).to_bytes(2, "little")
                                       + seg.to_bytes(2, "little"))
            self.assertEqual(far_calls_to(bytes(doctored), 0x1348),
                             ["1000:0100"], "segment 0x%04x" % seg)

    def test_the_near_scan_finds_a_planted_call_to_the_enemy_sheet(self):
        doctored = bytearray(self.img)
        at = 0x0100
        disp = (0x1348 - (at + 3)) & 0xFFFF
        doctored[at:at + 3] = bytes([0xE8]) + disp.to_bytes(2, "little")
        found = near_calls_to(bytes(doctored), 0x1348)
        self.assertIn("1000:0100", found)
        self.assertIn("1000:4c49", found)

    def test_the_branch_target_scan_finds_a_planted_jump(self):
        insns = list(dis16.decode_run(self.img, 0x3D11, 0x3D11 + 6971))
        self.assertEqual(branch_targets(insns).get(0x4E2A), None)
        planted = dis16.decode(b"\x90\xe9\x00\x00", 1)   # jmp to the next byte
        self.assertEqual(branch_targets([planted]), {0x4: [("1000:0001", "jmp")]})


class ProseTest(unittest.TestCase):
    """`docs/re/combat-dispatch.md` re-derived from `orig/g.exe`.

    The same three checks Task 16's fix round added for the character sheet,
    where they found three live defects on their first run.  The document is
    half the deliverable; without these it is the half with no net under it.
    """

    @classmethod
    def setUpClass(cls):
        cls.img = load_image()
        cls.branches = json.loads(BRANCHES.read_text(encoding="utf-8"))
        cls.aligned = aligned_boundaries(cls.img, cls.branches)
        cls.art = json.loads(ART.read_text(encoding="utf-8"))
        cls.md = strip_fences(DOC.read_text(encoding="utf-8"))
        cls.spans = inline_spans(cls.md)

    def cs_literal(self, off):
        n = self.img[off]
        return self.img[off + 1:off + 1 + n].decode("cp866")

    def known_literals(self):
        offs = {int(m.group(1), 16)
                for m in re.finditer(r"CS `0x([0-9a-f]{4})`", self.md)}

        def walk(node, key):
            if isinstance(node, dict):
                if isinstance(node.get(key), str):
                    yield node[key]
                for v in node.values():
                    yield from walk(v, key)
            elif isinstance(node, list):
                for v in node:
                    yield from walk(v, key)
        offs |= {int(o, 16) for o in walk(self.art, "cs_offset")}
        return {self.cs_literal(o) for o in offs} | set(walk(self.art, "text"))

    def test_every_prose_address_is_an_instruction_boundary(self):
        cites = sorted({m.group(0) for m in CITE.finditer(self.md)})
        self.assertGreater(len(cites), 150,
                           "the prose scan found only %d citations; a scan "
                           "that finds nothing must not pass" % len(cites))
        bad = [c for c in cites
               if c not in self.aligned and c not in NOT_A_BOUNDARY]
        self.assertEqual(
            bad, [],
            "docs/re/combat-dispatch.md cites %r, which an aligned decode from "
            "every segment-1000 function entry never reaches -- so it is a "
            "byte offset, not an address" % bad)
        for c, why in NOT_A_BOUNDARY.items():
            if c in cites:
                self.assertNotIn(c, self.aligned,
                                 "%s is exempted as %s but IS a boundary; the "
                                 "exemption has gone stale" % (c, why))

    def test_every_prose_instruction_says_what_the_binary_says(self):
        checked = 0
        for span in self.spans:
            m = re.match(r"^(1000:[0-9a-f]{4})\s+([a-z].*)$", span)
            if not m:
                continue
            cit, text = m.groups()
            self.assertIn(cit, self.aligned, "%r: not a boundary" % span)
            checked += 1
            self.assertEqual(
                self.aligned[cit].text, text,
                "docs/re/combat-dispatch.md writes `%s %s`, but tools/dis16.py "
                "decodes %r there" % (cit, text, self.aligned[cit].text))
        self.assertGreaterEqual(
            checked, 60,
            "only %d `1000:xxxx <instruction>` spans found in the prose; the "
            "pattern has drifted and this test is checking almost nothing"
            % checked)

    def test_every_prose_literal_comes_out_of_the_binary(self):
        offs = [int(m.group(1), 16)
                for m in re.finditer(r"CS `0x([0-9a-f]{4})`", self.md)]
        self.assertGreaterEqual(len(offs), 30, "only %d CS offsets" % len(offs))
        for o in offs:
            self.assertTrue(self.img[o], "CS 0x%04x has a zero length byte" % o)
            self.cs_literal(o)          # raises on undecodable bytes
        pairs = re.findall(r"`((?!1000:)[^`]+)`\s*\(CS `0x([0-9a-f]{4})`\)",
                           self.md, re.S)
        self.assertGreaterEqual(len(pairs), 25, "only %d pairs" % len(pairs))
        for text, off in pairs:
            self.assertEqual(
                self.cs_literal(int(off, 16)), text,
                "the prose quotes %r beside CS 0x%s, which holds %r"
                % (text, off, self.cs_literal(int(off, 16))))
        known = self.known_literals()
        unmatched = sorted({run for span in self.spans
                            for run in re.findall(r"[\u0400-\u04ff]+", span)
                            if not any(run in k for k in known)})
        self.assertEqual(
            unmatched, [],
            "Russian in docs/re/combat-dispatch.md that matches no literal in "
            "orig/g.exe at any address the doc or the artifact names: %r"
            % unmatched)


if __name__ == "__main__":
    unittest.main(verbosity=2)
