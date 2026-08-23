#!/usr/bin/env python3
"""Trace every `Random` draw a FIGHT spends, live, on a pinned seed.

    python3 tools/rngtrace/fightrun.py --boot-img build/rngtrace/boot.img \
        --walks 40 --class-answer 0 --combat-answer k \
        --out build/rngtrace/fightA.json

`run.py` is the frozen-oracle producer: it types `run` at the `Битва\\` prompt,
which is why `data/rng_trace.json`'s 1387 draws contain **zero** sites inside
`[0x3d11, 0x584c)`.  This tool is the same harness with `k` typed there
instead, so the blow loop, the taunt roll and the victory block actually
execute and get logged.

It is a SEPARATE entry point on purpose.  `run.py`, `gdbsession.build_script`
and `driver.walk` are untouched by Task 13, because they are what produced
`data/rng_trace.json` and `data/state_trace.json` and neither file is ever
regenerated.  What is shared is shared by import, never by copy: the seed
patch, the load-base derivation, the state field table, the log parser and
every guard in `tracelog`.

Four channels come out of one run:

  * **draws** -- `{site, n, r}` per `Random`, the same channel `run.py` logs.
  * **per-turn state** -- `run.state_fields()` at every `1000:ae63` stop.
  * **per-fight enemy record** -- the whole opponent at `1000:3d11`, which is
    an independent check on `FUN_1000_0d14`: the port must roll the same
    fighter, not merely the same draws.
  * **per-round hp and break flags** -- both fighters at every `1000:441d`
    (`Битва\\`) stop, which is what pins the jaw/leg break EFFECT rather than
    just the roll that decides it.
"""
import argparse
import json
import shutil
import sys
import time
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from rngtrace import (driver, gdbsession, loadbase, run as runmod,
                          seedpatch, tracelog, vm)
else:
    from . import driver, gdbsession, loadbase, seedpatch, tracelog, vm
    from . import run as runmod

REPO = Path(__file__).resolve().parents[2]

DS_IMAGE_OFF = loadbase.DATA_SEG_IMAGE_OFF

# The enemy fighter record, `docs/re/combat.md`'s own table ("The fighter
# record"): the opponent's copy of the same eight stat words, its hp pair, its
# three status bytes, and the three loot words `1000:523e`..`1000:5251` adds
# into the player's purse on a win.  Every width here is the width of the
# instruction that touches the field, not a guess: the stat words are `mov
# ax,[0x3954]`-shaped (`1000:51b9`..`1000:51c4`), the status bytes are
# `cmp byte [0x3966],0` / `mov byte [0x3966],1` (`1000:459e`, `1000:45be`), and
# the loot words are the `a1 6a 39` / `01 06 c3 38` pairs at `1000:523e`.
ENEMY_WORDS = {
    "e_class_3952": 0x3952, "e_strength_3954": 0x3954,
    "e_agility_3956": 0x3956, "e_vitality_3958": 0x3958,
    "e_luck_395a": 0x395A, "e_level_395c": 0x395C,
    "e_dmg_min_395e": 0x395E, "e_dmg_max_3960": 0x3960,
    "e_hp_3962": 0x3962, "e_hpmax_3964": 0x3964,
    "e_beer_396a": 0x396A, "e_money_396c": 0x396C, "e_hlam_396e": 0x396E,
}
ENEMY_BYTES = {
    "e_broken_jaw_3966": 0x3966, "e_broken_leg_3967": 0x3967,
    "e_armor_3968": 0x3968,
}

# What the `Битва\` prompt is read against: both fighters' hp and all four
# break flags.  The player's pair is 20ae:38b0/38b1 (`1000:4571`..`1000:45ea`
# sets the ENEMY's, `1000:4787`..`1000:4867` the PLAYER's), the enemy's pair is
# 20ae:3966/3967, and the two hp words are what the blow loop's exit tests read
# (`1000:4652` / `1000:48c6`).
ROUND_WORDS = {
    "p_hp_38ac": 0x38AC, "p_hpmax_38ae": 0x38AE,
    "r_e_hp_3962": 0x3962, "r_e_hpmax_3964": 0x3964,
}
ROUND_BYTES = {
    "p_broken_jaw_38b0": 0x38B0, "p_broken_leg_38b1": 0x38B1,
    "r_e_broken_jaw_3966": 0x3966, "r_e_broken_leg_3967": 0x3967,
    "p_tooth_guard_394a": 0x394A,
}


# Every gdb-read sample carries RandSeed, so `tracelog.check_sample_seeds` can
# pin it to the draw stream: a sample read at the wrong address or the wrong
# width, or one sitting at the wrong point in the stream, then fails against
# `docs/re/rng.md`'s recurrence instead of being published.  The name differs
# per channel only so a fold that merges channels cannot silently overwrite one
# with another; the ADDRESS is the same 20ae:367e in all three.
SEED_FIELD = {"fight": "e_randseed_367e", "round": "r_randseed_367e"}


def _fields(words, byts, seed_name=None):
    out = [(n, DS_IMAGE_OFF + o, 2) for n, o in sorted(words.items())]
    out += [(n, DS_IMAGE_OFF + o, 1) for n, o in sorted(byts.items())]
    if seed_name is not None:
        out.append((seed_name, loadbase.IMAGE_OFF_RANDSEED, 4))
    return out


def enemy_fields():
    """`[(name, image offset, width), ...]` for the per-fight enemy sample."""
    return _fields(ENEMY_WORDS, ENEMY_BYTES, SEED_FIELD["fight"])


def round_fields():
    """`[(name, image offset, width), ...]` for the per-combat-prompt sample."""
    return _fields(ROUND_WORDS, ROUND_BYTES, SEED_FIELD["round"])


def field_names(fields):
    return [n for n, _, _ in fields]


def read_fields(mem, base, fields):
    return {n: int.from_bytes(mem[base + o:base + o + w], "little")
            for n, o, w in fields}


def verify_image_after_drive(mem, exe, base, expected_patch):
    """The image is STILL the image at the end of the drive.

    A losing run leaves the game the way the original leaves it -- `^4Ты сдох.`
    at `1000:5053`, `FUN_1000_074b`, and `1f78:0116`'s `mov ah,0x4c` /
    `int 0x21` at file `0x1123C` -- so the final whole-memory dump is taken
    after the process has exited and DOS has freed its block.  Every guard that
    reads that dump (`reconcile_final_randseed`, `check_state_samples`'s
    last-sample comparison, `final_state`) then rests on those bytes still
    being the game's.  DOS does not scrub a freed block, but "does not scrub"
    is an assumption, and this makes it a checked one instead: every code-region
    relocation is re-verified at the same base, `Random`'s 29 bytes must still
    be `Random`, and the seed patch must still be in place.  If COMMAND.COM's
    transient (or anything else) had landed on the image, this raises rather
    than letting a stale read be published as guest state.

    Returns `(checks, randseed)`.  RandSeed is READ, never judged, here:
    `verify_guest_code`'s pre-attach rule (0 or exactly the seed) is the
    opposite of what is true after a drive, and `reconcile_final_randseed` is
    what judges it.
    """
    image = loadbase.load_image(exe)
    relocs = loadbase.parse_relocations(exe)
    out = dict(loadbase.verify_base(mem, image, relocs, base))
    got = bytes(mem[base + loadbase.IMAGE_OFF_RANDOM:
                    base + loadbase.IMAGE_OFF_RANDOM + loadbase.RANDOM_BYTES])
    want = exe[loadbase.FILE_OFF_RANDOM:
               loadbase.FILE_OFF_RANDOM + loadbase.RANDOM_BYTES]
    if got != want:
        raise loadbase.GuestCodeError(
            "after the drive, linear 0x%x no longer holds Random: %s"
            % (base + loadbase.IMAGE_OFF_RANDOM, got.hex(" ")))
    gotp = bytes(mem[base + loadbase.IMAGE_OFF_RANDOMIZE:
                     base + loadbase.IMAGE_OFF_RANDOMIZE + len(expected_patch)])
    if gotp != bytes(expected_patch):
        raise loadbase.GuestCodeError(
            "after the drive, the seed patch is gone from linear 0x%x: %s"
            % (base + loadbase.IMAGE_OFF_RANDOMIZE, gotp.hex(" ")))
    out["random_bytes_intact"] = True
    out["seed_patch_intact"] = True
    randseed = int.from_bytes(mem[base + loadbase.IMAGE_OFF_RANDSEED:
                                  base + loadbase.IMAGE_OFF_RANDSEED + 4],
                              "little")
    return out, randseed


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--boot-img", required=True, help="FreeDOS boot floppy image")
    ap.add_argument("--exe", default=str(REPO / "orig" / "g.exe"))
    ap.add_argument("--workdir", default=str(REPO / "build" / "rngtrace" / "fight"))
    ap.add_argument("--seed", default=hex(runmod.DEFAULT_SEED))
    ap.add_argument("--walks", type=int, default=40)
    ap.add_argument("--class-answer", type=int, default=0, choices=[0, 1, 2, 3])
    ap.add_argument("--district", default="1")
    ap.add_argument("--combat-answer", default="k", choices=["k", "run"],
                    help="what to type at the `Битва\\` prompt: `k` fights "
                         "(1000:4440), `run` flees (1000:48e1)")
    ap.add_argument("--with-saves", action="store_true",
                    help="copy the shipped orig/*.SAV corpus into the game "
                         "directory, so the district prompt can load a save")
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
    sfields = runmod.state_fields()
    efields = enemy_fields()
    rfields = round_fields()
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
        script.write_text(gdbsession.build_fight_script(
            base, args.gdb_port, sfields, efields, rfields))
        gdb = gdbsession.GdbSession(script, log).start()
        gdb.wait_ready()

        creation = driver.create_character(machine, args.class_answer,
                                           district=args.district)
        log_before = log.stat().st_size
        screen_before = machine.screen()
        drive_log = driver.fight(machine, args.walks,
                                 combat_answer=args.combat_answer)
        time.sleep(1.5)
        screen_after = machine.screen()
        if not gdb.alive():
            raise RuntimeError("gdb exited during the run; the trace is truncated:\n%s"
                               % log.read_text(errors="replace")[-2000:])
        if log.stat().st_size <= log_before:
            raise RuntimeError("no trace output while walking -- the tracer stopped "
                               "(log stayed at %d bytes)" % log_before)
        t0 = time.time()
        dump = machine.dump_memory()
        dump_seconds = time.time() - t0
        final_state = runmod.read_state(dump, base)
        final_enemy = read_fields(dump, base, efields)
        final_round = read_fields(dump, base, rfields)
        # The image must still BE the image when the final read is taken.  A
        # losing run leaves the game through `FUN_1000_074b` and back to DOS,
        # so the dump happens after the process exited -- and the whole
        # final-RandSeed reconciliation (tracelog guard 9) rests on DS:367e
        # still holding the guest's own value.  This re-runs the same
        # relocation-by-relocation check that derived the base, against the
        # post-drive dump, so "nothing overwrote the image" is verified rather
        # than assumed.
        post_checks, post_randseed = verify_image_after_drive(
            dump, exe, base, seedpatch.build_patch(seed))
    finally:
        machine.kill()
        if gdb is not None:
            gdb.stop()

    # Whether the two-transport reconciliation applies at all: it needs the
    # guest to have been sitting in the top-level `ReadLn` when the final dump
    # was taken.  A drive that left the game (the player died) or stopped
    # mid-fight was somewhere else, and the check is replaced -- explicitly --
    # by the per-sample LCG verification.  Judged from the guest's own last
    # screen, not from a flag the driver set about itself.
    ended_at_turn_marker = (not drive_log["guest_left_the_game"]
                            and driver.at_street(screen_after))

    text = log.read_text(errors="replace")
    parsed = tracelog.parse(text, state_names=runmod.state_field_names(),
                            enemy_names=field_names(efields),
                            round_names=field_names(rfields))
    if parsed["ready"] is not None and parsed["ready"]["image_base"] != base:
        raise RuntimeError("gdb attached at a different base than derived")
    # The driver's screen classification, cross-checked against the guest's
    # own breakpoint.  `drive_log["lines_the_game_read"]` is what the port
    # replaying this run is fed, and it is only the game's input if every line
    # the driver typed at a screen it called `combat` really was answered by
    # the `Битва\` prompt's ReadLn.  1000:441d counts those from inside the
    # guest, so a disagreement is a mis-classified screen -- exactly the way
    # this list could be silently wrong -- and it stops the run.
    typed_at_combat = drive_log["combat_prompts_typed_at"]
    seen_at_combat = len(parsed["combat_prompts"])
    if typed_at_combat != seen_at_combat:
        raise RuntimeError(
            "the driver typed %d line(s) at a screen it classified as the "
            "`Битва\\` prompt, but the guest stopped at 1000:441d %d time(s): "
            "the recorded input list does not describe what the game read, so "
            "it must not be published as the port's input"
            % (typed_at_combat, seen_at_combat))
    if not parsed["fights"]:
        raise RuntimeError(
            "this run contains no fight at all (0 stops at 1000:3d11) -- the "
            "whole point of a fight capture.  Drive more walks, or start from "
            "a character whose encounters reach combat.")

    verification = tracelog.verify_combat_run(
        parsed, seed,
        walks_completed=drive_log["turns_completed"],
        load_seg=base_info["load_seg"],
        screen_before=screen_before,
        screen_after=screen_after,
        randseed_at_attach=randseed,
        randseed_final=final_state["randseed_367e"],
        state_names=runmod.state_field_names(),
        final_state=final_state,
        enemy_names=field_names(efields),
        round_names=field_names(rfields),
        ended_at_turn_marker=ended_at_turn_marker,
        seed_field=SEED_FIELD,
        min_draws=args.min_draws)
    verification["image_intact_after_drive"] = post_checks
    if post_randseed != final_state["randseed_367e"]:
        raise RuntimeError(
            "the post-drive image check read RandSeed 0x%08X but read_state "
            "read 0x%08X from the same dump" % (post_randseed,
                                                final_state["randseed_367e"]))

    verification["driver_combat_typings_match_1000_441d_stops"] = typed_at_combat

    result = {
        "note": ("Live Random trace of a FIGHT in orig/g.exe under qemu+gdb "
                 "with RandSeed pinned by patching a COPY of the binary.  "
                 "Produced by tools/rngtrace/fightrun.py; see "
                 "docs/re/rng-trace.md.  Ground truth is the original only -- "
                 "nothing here comes from src/."),
        "harness": "tools/rngtrace (python3 tools/rngtrace/fightrun.py)",
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
        "fight_marker": {
            "ghidra": "1000:3d11",
            "what": ("FUN_1000_3d11's own prologue -- one stop per fight, with "
                     "the enemy record 20ae:3952.. already rolled by "
                     "FUN_1000_0d14"),
            "fields": [{"name": n, "ds_offset": "20ae:%04x" % (o - DS_IMAGE_OFF),
                        "width": w} for n, o, w in efields],
        },
        "round_marker": {
            "ghidra": "1000:441d",
            "what": ("the `Битва\\` prompt's own ReadLn call (9a c6 06 78 0f, "
                     "buffer DS:3a72 pushed at 1000:4414) -- one stop per "
                     "combat prompt, i.e. the state the previous round left"),
            "fields": [{"name": n, "ds_offset": "20ae:%04x" % (o - DS_IMAGE_OFF),
                        "width": w} for n, o, w in rfields],
        },
        "state_channel": {
            "what": ("every variable in run.state_fields(), read out of guest "
                     "memory by gdb at each 1000:ae63 stop"),
            "granularity_limit": ("one sample per TURN.  A sample pair shows a "
                                  "turn's net effect on these variables, never "
                                  "the order in which they changed inside it."),
            "fields": [{"name": n, "ds_offset": "20ae:%04x" % (o - DS_IMAGE_OFF)
                        if o >= DS_IMAGE_OFF else None,
                        "image_off": "0x%x" % o, "width": w,
                        "citation": runmod.STATE_CITATIONS.get(n)}
                       for n, o, w in sfields],
            "full_dump_seconds": round(dump_seconds, 3),
            "full_dump_bytes": len(dump),
        },
        "run": {
            "ended_at_turn_marker": ended_at_turn_marker,
            "walks_requested": args.walks,
            **driver.class_record(args.class_answer,
                                  bool(creation.get("loaded_save")),
                                  final_state["class_389c"]),
            "district_key": args.district,
            "combat_answer": args.combat_answer,
            "saves_copied": saves_copied,
            "creation": creation,
            "prompt_stops": parsed["prompt_stops"],
            "drive_log": dict(drive_log),
        },
        "final_state": final_state,
        "final_enemy": final_enemy,
        "final_round": final_round,
        "state_samples": parsed["state_samples"],
        "fights": parsed["fights"],
        "combat_prompts": parsed["combat_prompts"],
        "verification": verification,
        "draws": parsed["draws"],
    }
    out = Path(args.out) if args.out else work / "fight.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print("draws=%d fights=%d combat_prompts=%d turns=%d/%d exited=%s base=%s -> %s"
          % (len(parsed["draws"]), len(parsed["fights"]),
             len(parsed["combat_prompts"]), drive_log["turns_completed"],
             args.walks, drive_log["guest_left_the_game"],
             base_info["image_base_hex"], out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
