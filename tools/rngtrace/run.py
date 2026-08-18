#!/usr/bin/env python3
"""Trace every `Random` draw the original spends, live, on a pinned seed.

    python3 tools/rngtrace/run.py --boot-img build/rngtrace/boot.img \
        --walks 30 --class-answer 0 --out build/rngtrace/trace-A.json

One command, no manual gdb steps.  Every exit path kills the VM.  A run that
logs no draws, that cannot install its breakpoints, or whose draw stream does
not replay against the pinned seed exits non-zero -- it never writes a short
trace and calls it evidence.
"""
import argparse
import json
import shutil
import sys
import time
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from rngtrace import (driver, gdbsession, loadbase, rng, seedpatch,
                          tracelog, vm)
else:
    from . import driver, gdbsession, loadbase, rng, seedpatch, tracelog, vm

REPO = Path(__file__).resolve().parents[2]
DEFAULT_SEED = 0x12345678

# Image offsets used for the runtime verifications.
IMAGE_OFF_RANDOM = 0xF78 * 16 + 0x114B        # 0x108cb -- file 0x1219b
IMAGE_OFF_RANDOMIZE = 0xF78 * 16 + 0x11E0     # 0x10960 -- file 0x12230
IMAGE_OFF_RANDSEED = 0x10AE0 + 0x367E         # 0x1415e -- DS:367e, file 0x15a2e
FILE_OFF_RANDOM = 0x1219B


# DS is Ghidra 20ae; image offset of DS:0000 is (0x20ae - 0x1000) * 16.
DS_IMAGE_OFF = 0x10AE0
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


def read_state(mem, base):
    """The state the gates were evaluated against, read out of guest memory."""
    out = {}
    for name, off in sorted(STATE_WORDS.items()):
        at = base + DS_IMAGE_OFF + off
        out[name] = int.from_bytes(mem[at:at + 2], "little")
    for name, off in sorted(STATE_BYTES.items()):
        out[name] = mem[base + DS_IMAGE_OFF + off]
    out["randseed_367e"] = int.from_bytes(
        mem[base + IMAGE_OFF_RANDSEED: base + IMAGE_OFF_RANDSEED + 4], "little")
    return out


def verify_guest_code(mem, exe, base, seed):
    """Check the guest really holds OUR image at OUR base before breaking on it.

    A breakpoint on the wrong address produces a plausible EMPTY trace, which is
    the worst failure mode this harness has.
    """
    out = {}
    want = exe[FILE_OFF_RANDOM:FILE_OFF_RANDOM + 29]
    got = bytes(mem[base + IMAGE_OFF_RANDOM: base + IMAGE_OFF_RANDOM + 29])
    if got != want:
        raise RuntimeError("guest code at linear 0x%x is %s, expected Random %s"
                           % (base + IMAGE_OFF_RANDOM, got.hex(" "), want.hex(" ")))
    # The instruction encoding itself, re-checked here rather than trusted:
    # `mul word [ss:bx+0x4]` twice and a `retf 2` tail is Random(Word), not a
    # neighbouring routine.
    if got.count(bytes.fromhex("36f76704")) != 2 or got[26:29] != bytes.fromhex("ca0200"):
        raise RuntimeError("bytes at Random do not look like the 32x16 high take: %s"
                           % got.hex(" "))
    out["random_linear"] = "0x%X" % (base + IMAGE_OFF_RANDOM)
    out["random_bytes"] = got.hex(" ")

    patched = seedpatch.build_patch(seed)
    gotp = bytes(mem[base + IMAGE_OFF_RANDOMIZE: base + IMAGE_OFF_RANDOMIZE + len(patched)])
    if gotp != patched:
        raise RuntimeError("seed patch is not in guest memory at linear 0x%x: %s"
                           % (base + IMAGE_OFF_RANDOMIZE, gotp.hex(" ")))
    out["randomize_linear"] = "0x%X" % (base + IMAGE_OFF_RANDOMIZE)
    out["randomize_bytes"] = gotp.hex(" ")

    randseed = int.from_bytes(mem[base + IMAGE_OFF_RANDSEED: base + IMAGE_OFF_RANDSEED + 4],
                              "little")
    out["randseed_at_attach"] = "0x%08X" % randseed
    if randseed == 0:
        out["randseed_state"] = "image value -- patched Randomize has not run yet"
    elif randseed == seed:
        out["randseed_state"] = "pinned seed in place, no draw has been spent yet"
    else:
        out["randseed_state"] = "already stepped -- draws happened before the attach"
    return out, randseed


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
        checks, randseed = verify_guest_code(mem, exe, base, seed)
        if randseed not in (0, seed):
            raise RuntimeError(
                "RandSeed is already 0x%08X before the breakpoint could be "
                "installed: draws were spent before the attach, so any trace "
                "from this run would be missing its head." % randseed)

        script = work / "trace.gdb"
        log = work / "trace.gdb.log"
        script.write_text(gdbsession.build_script(base, log, args.gdb_port))
        gdb = gdbsession.GdbSession(script, log).start()
        gdb.wait_ready()

        creation = driver.create_character(machine, args.class_answer,
                                          district=args.district)
        log_before = log.stat().st_size
        drive_log = driver.walk(machine, args.walks)
        time.sleep(1.5)
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
        # trace: it must equal the LCG stepped once per logged draw.
        final_state = read_state(machine.dump_memory(), base)
    finally:
        # Order matters: the VM must die first so gdb's `continue` fails and
        # gdb drops to a prompt where `quit` is read.  See GdbSession.stop.
        machine.kill()
        if gdb is not None:
            gdb.stop()

    text = log.read_text(errors="replace")
    parsed = tracelog.parse(text)
    verification = tracelog.verify(parsed, seed, min_draws=args.min_draws)
    if parsed["ready"]["image_base"] != base:
        raise RuntimeError("gdb attached at a different base than derived")

    # Second completeness check, at the state tier: replaying the LCG one step
    # per logged draw must land exactly on the RandSeed the guest ended with.
    # A draw the tracer failed to log would leave the guest ahead of the replay.
    expected_seed = seed
    for _ in parsed["draws"]:
        expected_seed = rng.step(expected_seed)
    if final_state["randseed_367e"] != expected_seed:
        raise RuntimeError(
            "final RandSeed 0x%08X does not match the replay of %d logged draws "
            "(0x%08X): the trace is incomplete."
            % (final_state["randseed_367e"], len(parsed["draws"]), expected_seed))
    verification["final_randseed"] = "0x%08X" % final_state["randseed_367e"]
    verification["final_randseed_matches_replay"] = True

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
        "run": {
            "walks_requested": args.walks,
            "class_answer": args.class_answer,
            "class_value": driver.CLASS_VALUE[args.class_answer],
            "class_name": driver.CLASS_ANSWERS[args.class_answer],
            "district_key": args.district,
            "saves_copied": saves_copied,
            "creation": creation,
            "prompt_stops": parsed["prompt_stops"],
            "drive_log": drive_log,
        },
        "final_state": final_state,
        "verification": verification,
        "draws": parsed["draws"],
    }
    out = Path(args.out) if args.out else work / "trace.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print("draws=%d prompt_stops=%d base=%s -> %s"
          % (len(parsed["draws"]), parsed["prompt_stops"], base_info["image_base_hex"], out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
