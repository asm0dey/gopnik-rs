#!/usr/bin/env python3
"""Trace every `Random` draw the original spends, live, on a pinned seed.

    python3 tools/rngtrace/run.py --boot-img build/rngtrace/boot.img \
        --walks 30 --class-answer 0 --out build/rngtrace/trace-A.json

One command, no manual gdb steps.  Every exit path kills the VM.  A run that
logs no draws, that cannot install its breakpoints, whose gdb script aborted
mid-walk, whose walks did not all reach the top-level prompt, or whose draw
stream does not replay against the pinned seed exits non-zero -- it never
writes a short trace and calls it evidence.  tracelog.verify_run holds the
guards and states what each one defends against.

Two channels come out of one run.  The draws are the first; the second is a
per-turn STATE sample -- every variable state_fields() names, read out of guest
memory by gdb at each 1000:ae63 stop.  Both live in this file's output; the
folds that publish them are separate tools writing separate files, because
data/rng_trace.json (draws) is frozen and data/state_trace.json (state) is not.
"""
import argparse
import json
import shutil
import sys
import time
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from rngtrace import (driver, gdbsession, loadbase, seedpatch,
                          tracelog, vm)
else:
    from . import driver, gdbsession, loadbase, seedpatch, tracelog, vm

REPO = Path(__file__).resolve().parents[2]
DEFAULT_SEED = 0x12345678

# The guest-memory verification that used to live here is loadbase.verify_guest_code:
# it is a pure computation over a memory image, and it is the defence against
# attaching at a wrong base, so it belongs where it can be unit tested.

# DS is Ghidra 20ae; image offset of DS:0000 is (0x20ae - 0x1000) * 16.
DS_IMAGE_OFF = loadbase.DATA_SEG_IMAGE_OFF
# Field offsets within DS, from docs/re/combat.md's fighter record and
# docs/re/wander.md's globals.  Read only, purely to record what state the
# gates were evaluated against.
STATE_WORDS = {
    "class_389c": 0x389C, "strength_389e": 0x389E, "agility_38a0": 0x38A0,
    "vitality_38a2": 0x38A2, "luck_38a4": 0x38A4, "level_38a6": 0x38A6,
    "dmg_min_38a8": 0x38A8, "dmg_max_38aa": 0x38AA, "hp_38ac": 0x38AC,
    "hpmax_38ae": 0x38AE, "xp_38ce": 0x38CE, "xp_threshold_38d0": 0x38D0,
    "street_cred_38cb": 0x38CB,
}
STATE_BYTES = {
    "district_3692": 0x3692, "flag_market_3694": 0x3694, "flag_3695": 0x3695,
    "flag_den_3696": 0x3696, "flag_girl_3697": 0x3697, "flag_vet_3698": 0x3698,
    "flag_club_3699": 0x3699, "flag_gym_369a": 0x369A,
    "broken_jaw_38b0": 0x38B0, "broken_leg_38b1": 0x38B1, "unk_38b2": 0x38B2,
    "has_mobile_38bb": 0x38BB, "ring_38c1": 0x38C1,
    "den_errand_1_3b78": 0x3B78, "den_errand_2_3b79": 0x3B79,
}

# Task 11i widened STATE_WORDS with the six purse/loot words.  Every one of
# them is a WORD because the instructions that touch them are word-sized, not
# because a byte would have looked wrong: `1000:523e`..`1000:5251` is
# `a1 6a 39` / `01 06 c3 38` / `a1 6c 39` / `01 06 c7 38` / `a1 6e 39` /
# `01 06 c9 38`, i.e. three `mov ax,[enemy word]` / `add [player word],ax`
# pairs (re-derived with `python3 tools/re_query.py resolve 1000:523e`).
STATE_CITATIONS = {
    "beer_38c3": ("20ae:38c3, beer in half-litres -- docs/re/gaps.md:283; "
                  "`add [0x38c3],ax` at 1000:5241"),
    "money_38c7": ("20ae:38c7, the player's money -- docs/re/tables.md:191 "
                   "(`3B 06 C7 38  cmp ax,[0x38c7]`, the shop affordability "
                   "compare); `add [0x38c7],ax` at 1000:5248"),
    "hlam_38c9": ("20ae:38c9, Хлам -- docs/re/gaps.md:283; "
                  "`add [0x38c9],ax` at 1000:524f"),
    "enemy_beer_396a": ("20ae:396a, the rolled enemy's beer drop -- "
                        "docs/re/progression.md:233, docs/re/gaps.md:281; "
                        "`mov ax,[0x396a]` at 1000:523e"),
    "enemy_money_396c": ("20ae:396c, the rolled enemy's money drop -- "
                         "docs/re/progression.md:233, docs/re/gaps.md:280; "
                         "`mov ax,[0x396c]` at 1000:5245"),
    "enemy_hlam_396e": ("20ae:396e, the rolled enemy's Хлам "
                        "drop -- docs/re/progression.md:233, "
                        "docs/re/gaps.md:279; `mov ax,[0x396e]` at 1000:524c"),
}
STATE_WORDS.update({
    "beer_38c3": 0x38C3, "money_38c7": 0x38C7, "hlam_38c9": 0x38C9,
    "enemy_beer_396a": 0x396A, "enemy_money_396c": 0x396C,
    "enemy_hlam_396e": 0x396E,
})


def state_fields():
    """`[(name, image offset, width), ...]` -- the sampled variables, in ONE
    order, defined once.

    Both transports are built from this list: `read_state` below reads them out
    of a full `pmemsave` dump, and `gdbsession.build_script` reads the same
    addresses over gdb's remote protocol at every turn marker.  That is what
    makes `tracelog.check_state_samples`'s last-sample-vs-`final_state`
    reconciliation a real check -- two independent paths into guest memory,
    compared -- rather than a table compared with itself.

    The offsets are `20ae:` offsets (`addr.DATA_SEG_IMAGE_OFF` is the image
    offset of DGROUP, derived in `tools/addr.py`, not written down here).
    """
    out = [(name, DS_IMAGE_OFF + off, 2) for name, off in sorted(STATE_WORDS.items())]
    out += [(name, DS_IMAGE_OFF + off, 1) for name, off in sorted(STATE_BYTES.items())]
    out.append(("randseed_367e", loadbase.IMAGE_OFF_RANDSEED, 4))
    return out


def state_field_names():
    return [name for name, _, _ in state_fields()]


def read_state(mem, base):
    """The state the gates were evaluated against, read out of guest memory."""
    out = {}
    for name, image_off, width in state_fields():
        at = base + image_off
        out[name] = int.from_bytes(mem[at:at + width], "little")
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--boot-img", required=True, help="FreeDOS boot floppy image")
    ap.add_argument("--exe", default=str(REPO / "orig" / "g.exe"))
    ap.add_argument("--workdir", default=str(REPO / "build" / "rngtrace" / "run"))
    ap.add_argument("--seed", default=hex(DEFAULT_SEED))
    ap.add_argument("--walks", type=int, default=20)
    ap.add_argument("--class-answer", type=int, default=0, choices=[0, 1, 2, 3])
    ap.add_argument("--district", default="1")
    ap.add_argument("--with-saves", action="store_true",
                    help="copy the shipped orig/*.SAV corpus into the game directory, "
                         "so the district prompt can load a save instead of creating "
                         "a character (SAVE_R3.SAV ships with the phone and the ring, "
                         "which gate draws 3, 4 and 9)")
    ap.add_argument("--gdb-port", type=int, default=1234)
    ap.add_argument("--sock-dir", default="/tmp")
    ap.add_argument("--min-draws", type=int, default=1)
    ap.add_argument("--out", default=None, help="write the trace JSON here")
    args = ap.parse_args(argv)

    seed = int(args.seed, 0)
    work = Path(args.workdir)
    if work.exists():
        shutil.rmtree(work)
    gamedir = work / "gamedir"
    gamedir.mkdir(parents=True)

    saves_copied = []
    if args.with_saves:
        for src in sorted(Path(args.exe).parent.glob("*.SAV")):
            shutil.copy(src, gamedir / src.name)
            saves_copied.append(src.name)

    patch = seedpatch.write_patched_copy(args.exe, gamedir / "G.EXE", seed)
    exe = (gamedir / "G.EXE").read_bytes()

    machine = vm.Vm(args.boot_img, gamedir, work, sock_dir=args.sock_dir,
                    gdb_port=args.gdb_port)
    gdb = None
    try:
        machine.start()
        driver.boot_to_dos(machine)
        driver.launch_game(machine)

        mem = machine.dump_memory()
        base_info = loadbase.derive(mem, exe)
        base = base_info["image_base"]
        checks, randseed = loadbase.verify_guest_code(
            mem, exe, base, seed, seedpatch.build_patch(seed))

        script = work / "trace.gdb"
        log = work / "trace.gdb.log"
        fields = state_fields()
        script.write_text(gdbsession.build_script(base, args.gdb_port, fields))
        gdb = gdbsession.GdbSession(script, log).start()
        gdb.wait_ready()

        creation = driver.create_character(machine, args.class_answer,
                                          district=args.district)
        log_before = log.stat().st_size
        # Screens either side of the drive: a guest that never ran -- because
        # the gdb script aborted and left it stopped at a breakpoint -- shows a
        # byte-identical screen while the driver types happily into it.
        screen_before = machine.screen()
        drive_log = driver.walk(machine, args.walks)
        time.sleep(1.5)
        screen_after = machine.screen()
        # Liveness, checked before the VM goes away: a dead gdb or a log that
        # stopped growing means the trace is truncated, and a truncated trace
        # must never be published as if it were the whole stream.
        if not gdb.alive():
            raise RuntimeError("gdb exited during the run; the trace is truncated:\n%s"
                               % log.read_text(errors="replace")[-2000:])
        if log.stat().st_size <= log_before:
            raise RuntimeError("no trace output while walking -- the tracer stopped "
                               "(log stayed at %d bytes)" % log_before)
        # The guest is idle in the top-level ReadLn here (running, not stopped),
        # so a second dump is safe.  Its RandSeed is an independent check on the
        # trace: it must equal the LCG stepped once per logged draw, and the
        # rest of it is the reconciliation of the per-turn samples (which come
        # over gdb) against a wholly separate path into guest memory.
        t0 = time.time()
        dump = machine.dump_memory()
        dump_seconds = time.time() - t0
        final_state = read_state(dump, base)
    finally:
        # Order matters: the VM must die first so gdb's `continue` fails and
        # gdb drops to a prompt where `quit` is read.  See GdbSession.stop.
        machine.kill()
        if gdb is not None:
            gdb.stop()

    text = log.read_text(errors="replace")
    parsed = tracelog.parse(text, state_names=state_field_names())
    if parsed["ready"] is not None and parsed["ready"]["image_base"] != base:
        raise RuntimeError("gdb attached at a different base than derived")
    # EVERY guard, in one keyword-only call: leaving one out is a TypeError
    # here rather than a quietly weaker run.  See tracelog's module docstring
    # for what each guard defends against.
    verification = tracelog.verify_run(
        parsed, seed,
        walks=args.walks,
        load_seg=base_info["load_seg"],
        screen_before=screen_before,
        screen_after=screen_after,
        randseed_at_attach=randseed,
        randseed_final=final_state["randseed_367e"],
        state_names=state_field_names(),
        final_state=final_state,
        min_draws=args.min_draws)

    result = {
        "note": ("Live Random trace of orig/g.exe under qemu+gdb with RandSeed "
                 "pinned by patching a COPY of the binary.  Produced by "
                 "tools/rngtrace/run.py; see docs/re/rng-trace.md.  Ground truth "
                 "is the original only -- nothing here comes from src/."),
        "harness": "tools/rngtrace",
        "seed": seed,
        "seed_hex": "0x%08X" % seed,
        "seed_patch": patch,
        "load_base": base_info,
        "runtime_checks": checks,
        "observation_point": {
            "ghidra": "1f78:1165",
            "runtime_seg_off": "0f78:1165",
            "file_offset": "0x121b5",
            "what": ("the `retf word 0x2` at the tail of Random(Word); SP is back "
                     "at its entry value so [sp]=return offset, [sp+2]=return "
                     "segment, [sp+4]=n, and ax already holds the result"),
        },
        "turn_marker": {
            "ghidra": "1000:ae63",
            "what": "the top-level prompt's ReadLn call (9a c6 06 78 0f)",
        },
        "state_channel": {
            "what": ("every variable in state_fields(), read out of guest "
                     "memory by gdb at each 1000:ae63 stop -- the state the "
                     "top-level prompt is about to be read against"),
            "granularity_limit": ("one sample per TURN.  A sample pair shows a "
                                  "turn's net effect on these variables, never "
                                  "the order in which they changed inside it."),
            "fields": [{"name": n, "ds_offset": "20ae:%04x" % (io - DS_IMAGE_OFF)
                        if io >= DS_IMAGE_OFF else None,
                        "image_off": "0x%x" % io, "width": w,
                        "citation": STATE_CITATIONS.get(n)}
                       for n, io, w in state_fields()],
            "full_dump_seconds": round(dump_seconds, 3),
            "full_dump_bytes": len(dump),
            "sampled_bytes_per_turn": sum(w for _, _, w in state_fields()),
        },
        "run": {
            "walks_requested": args.walks,
            # The class comes from the guest, never from the CLI answer: with a
            # save loaded the class prompt is never reached.  See class_record.
            **driver.class_record(args.class_answer,
                                  bool(creation.get("loaded_save")),
                                  final_state["class_389c"]),
            "district_key": args.district,
            "saves_copied": saves_copied,
            "creation": creation,
            "prompt_stops": parsed["prompt_stops"],
            "drive_log": drive_log,
        },
        "final_state": final_state,
        "state_samples": parsed["state_samples"],
        "verification": verification,
        "draws": parsed["draws"],
    }
    out = Path(args.out) if args.out else work / "trace.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print("draws=%d prompt_stops=%d state_samples=%d base=%s -> %s"
          % (len(parsed["draws"]), parsed["prompt_stops"],
             len(parsed["state_samples"]), base_info["image_base_hex"], out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
