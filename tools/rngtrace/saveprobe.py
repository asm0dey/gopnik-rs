#!/usr/bin/env python3
"""Load a synthesised `.SAV` in the real `orig/g.exe` and read the guest back.

    python3 tools/rngtrace/saveprobe.py --boot-img build/rngtrace/boot.img \\
        --out build/rngtrace/saveprobe.json

**The question this answers:** *which guest byte does each `.SAV` offset
become?*  `tools/savegen.py` writes a record with a distinct sentinel at
every offset under test; this boots FreeDOS under qemu, launches the real
executable, answers the district prompt with the slot the probe wrote, and
dumps guest physical memory.  Wherever the sentinel run appears is where the
record landed, and each offset's guest address falls out of the same dump.

It is a **controlled experiment**, not a correlation: the five shipped saves
differ from each other in dozens of bytes at once, so no pair of them can
isolate one offset.  A synthesised pair that differs in exactly one byte can.

**What a run of this proves, and what it does not.**  It is STATE-tier
evidence (`docs/re/METHODOLOGY.md`): it says which guest variable a save byte
reaches, never *why* the code does anything with it.  It can falsify a
mapping outright, and it narrows a byte to one candidate address so that
`python3 tools/re_query.py xrefs-to 20ae:xxxx` is a short search rather than
a blind one -- but the claim about what the byte MEANS is carried by the
instruction that reads it, and that is flow.  It also forces states no real
playthrough can reach, so anything observed downstream of a probe load must
say it was forced.

What this never touches is the **frozen set**: `orig/*.SAV`, `orig/g.exe`,
`data/rng_trace.json`, `data/state_trace.json`, `data/combat_trace.json` and
`data/combat_vectors.json`.  The probe save is written into the run's own temp
game directory, and `savegen` refuses an `--out` naming anything frozen.
`--out` does write under `data/` -- that is how both committed
`data/probes/*.json` artifacts were produced -- so this is scoped to the
frozen set and not to a directory.
"""
import argparse
import json
import shutil
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from rngtrace import driver, loadbase, vm
else:
    from . import driver, loadbase, vm

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import savegen                                                       # noqa: E402
from decode_save import LAYOUT, RECORD_BASE, SIZE                    # noqa: E402

REPO = Path(__file__).resolve().parents[2]
DS_IMAGE_OFF = loadbase.DATA_SEG_IMAGE_OFF

#: The two spans Task 19 established. Sentinels go here by default because
#: these are the bytes whose guest address was the open question; every other
#: offset keeps the base save's real value so the record stays loadable.
DEFAULT_SPANS = [(0x214, 0x231), (0x2AE, SIZE)]


def probe_record(base_bytes, spans):
    """The synthesised record and the sentinel map that went into it."""
    sentinels = savegen.sentinel_bytes(spans)
    return savegen.synthesise(base_bytes, raw_bytes=sentinels), sentinels


def locate(mem: bytes, needle: bytes):
    """Every physical address holding `needle`. More than one is a finding."""
    out, i = [], mem.find(needle)
    while i != -1:
        out.append(i)
        i = mem.find(needle, i + 1)
    return out


def field_at(off: int):
    for f in LAYOUT["fields"]:
        if f["off"] <= off < f["off"] + f["len"]:
            return f
    return None


def main(argv=None):
    ap = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--boot-img", required=True)
    ap.add_argument("--exe", default=str(REPO / "orig" / "g.exe"))
    ap.add_argument("--base-save", default=str(REPO / "orig" / "SAVE_R3.SAV"),
                    help="the real save the probe record is built from; every "
                         "byte outside --span keeps its value")
    ap.add_argument("--slot", default="3", choices=list("2345"),
                    help="the district digit the probe save is written as, and "
                         "the key the driver presses at the slot prompt. Slot "
                         "0 is excluded on purpose: it is the only slot that "
                         "also reads places.sav (1000:6c50) and derives the "
                         "district from the level (1000:6d93), so it changes "
                         "two things at once")
    ap.add_argument("--workdir",
                    default=str(REPO / "build" / "rngtrace" / "saveprobe"))
    ap.add_argument("--sock-dir", default="/tmp")
    ap.add_argument("--gdb-port", type=int, default=1240)
    ap.add_argument("--fresh", action="store_true",
                    help="stage NO save and create a character instead, then "
                         "dump the record a brand-new character starts with. "
                         "That is what a save written by a port has to match "
                         "for the bytes nothing has touched yet -- otherwise "
                         "'a fresh save fills them with zeroes' is a guess.")
    ap.add_argument("--class-answer", type=int, default=0, choices=[0, 1, 2, 3],
                    help="--fresh only: the class menu answer")
    ap.add_argument("--out", default=None)
    args = ap.parse_args(argv)

    work = Path(args.workdir)
    if work.exists():
        shutil.rmtree(work)
    gamedir = work / "gamedir"
    gamedir.mkdir(parents=True)

    exe_bytes = Path(args.exe).read_bytes()
    (gamedir / "G.EXE").write_bytes(exe_bytes)
    if args.fresh:
        record, sentinels = None, {}
    else:
        record, sentinels = probe_record(Path(args.base_save).read_bytes(),
                                         DEFAULT_SPANS)
        (gamedir / ("SAVE_R%s.SAV" % args.slot)).write_bytes(record)

    machine = vm.Vm(args.boot_img, gamedir, work, sock_dir=args.sock_dir,
                    gdb_port=args.gdb_port)
    try:
        machine.start()
        driver.boot_to_dos(machine)
        driver.launch_game(machine)
        drive = driver.create_character(machine, args.class_answer,
                                        district=args.slot)
        if drive["loaded_save"] == args.fresh:
            raise driver.DriveError(
                "wanted %s and got the other: %s\n%s"
                % ("a fresh character" if args.fresh else "a loaded save",
                   drive, machine.screen()))
        screen = machine.screen()
        mem = machine.dump_memory()
    finally:
        machine.kill()

    base = loadbase.derive(mem, exe_bytes)["image_base"]
    record_at = base + DS_IMAGE_OFF + RECORD_BASE

    # The whole-record check first: it is the one that settles the delta, and
    # it settles it for all 694 bytes at once rather than byte by byte.
    guest_record = mem[record_at:record_at + SIZE]

    if args.fresh:
        # No sentinels and no file to compare against: the whole point is
        # what the record HOLDS, so it is reported rather than checked.
        fresh = {
            "what": "the 694-byte record a brand-new character starts with, "
                    "read out of guest memory at 20ae:369c",
            "class_answer": args.class_answer,
            "name_typed": "(empty -- the game substitutes its own default)",
            "record_hex": guest_record.hex(),
            "magic": guest_record[1:1 + guest_record[0]].decode("cp866"),
            "magic_padding_all_zero": set(
                guest_record[1 + guest_record[0]:0x100]) <= {0},
            "name": guest_record[0x101:0x101 + guest_record[0x100]]
                    .decode("cp866"),
            "name_padding_all_zero": set(
                guest_record[0x101 + guest_record[0x100]:0x200]) <= {0},
            "tail_all_zero": set(guest_record[0x214:]) <= {0},
            "screen_tail": "\n".join(
                [l.rstrip() for l in screen.splitlines() if l.strip()][-6:]),
        }
        text = json.dumps(fresh, indent=1, ensure_ascii=False) + "\n"
        if args.out:
            Path(args.out).write_text(text, encoding="utf-8")
        print(text)
        return 0

    # ...and the independent half: find the sentinel run in RAM without
    # assuming where it should be. If the delta is wrong, this still reports
    # the address the bytes actually reached.
    #
    # It is NOT unique, and that is expected rather than a defect: DOS reads
    # the file through its own sector buffers, which sit below the program's
    # load base and still hold the bytes when the dump is taken. So the
    # assertion is "the record base is among the hits", and every hit below
    # `base` is reported as a buffer copy rather than silently dropped -- a
    # hit ABOVE the load base that is not the record base would be a real
    # finding and would show up here.
    span_lo, span_hi = DEFAULT_SPANS[0]
    needle = bytes(sentinels[o] for o in range(span_lo, span_hi))
    hits = locate(mem, needle)
    want = record_at + span_lo
    other_in_program = [h for h in hits if h >= base and h != want]

    rows = []
    for off in sorted(sentinels):
        f = field_at(off)
        rows.append({
            "sav_off": "0x%03x" % off,
            "guest": "20ae:%04x" % (RECORD_BASE + off),
            "field": f["name"] if f else None,
            "wrote": sentinels[off],
            "guest_holds": guest_record[off],
            "match": guest_record[off] == sentinels[off],
        })

    result = {
        "what": "a synthesised .SAV loaded in orig/g.exe under qemu; guest "
                "physical memory read back at the record base",
        "tier": "state -- it maps a save offset to a guest address; the "
                "MEANING of each byte is carried by the instruction that "
                "reads it (docs/re/save-format.md), not by this run",
        "slot": args.slot,
        "base_save": Path(args.base_save).name,
        "image_base_hex": "0x%X" % base,
        "record_base": "20ae:%04x" % RECORD_BASE,
        "drive": drive,
        "whole_record_matches_the_file": guest_record == record,
        "sentinel_run_physical_addresses": ["0x%X" % h for h in hits],
        "sentinel_run_lands_at_the_record_base": want in hits,
        "sentinel_run_record_base_physical": "0x%X" % want,
        "sentinel_run_copies_below_the_load_base": [
            "0x%X" % h for h in hits if h < base],
        "sentinel_run_other_copies_inside_the_program": [
            "0x%X" % h for h in other_in_program],
        "copies_below_the_load_base_are_dos_sector_buffers": (
            "expected: DOS reads the record through its own buffers and they "
            "still hold it at dump time. Only a hit at or above 0x%X that is "
            "not the record base would be a finding." % base),
        "bytes": rows,
        # Non-blank lines only: the guest clears the screen for the street
        # prompt, so a raw tail is six empty rows and says nothing about
        # whether the save loaded. `drive.loaded_save` is the real evidence
        # (the class prompt was never reached).
        "screen_tail": "\n".join(
            [l.rstrip() for l in screen.splitlines() if l.strip()][-6:]),
    }
    text = json.dumps(result, indent=1, ensure_ascii=False) + "\n"
    if args.out:
        Path(args.out).write_text(text, encoding="utf-8")
    print(text)

    ok = (result["whole_record_matches_the_file"]
          and result["sentinel_run_lands_at_the_record_base"]
          and not other_in_program
          and all(r["match"] for r in rows))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
