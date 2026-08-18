#!/usr/bin/env python3
r"""Capture combat vectors from orig/g.exe running under the Task 3 oracle.

Regenerates data/combat_vectors.json. Nothing here reads, links or shells out
to the Rust crate: every number in the artifact comes out of the original
program -- the blow lines out of the 80x25 text screen, the fighters' stats
and RandSeed out of the guest's own data segment. Vectors produced by the
implementation under test would prove nothing, so this tool must stay
independent of src/combat.rs.

How a case is pinned down
-------------------------
1. The scratch copy of g.exe gets System.Randomize's body replaced by a
   constant store (capture.pin_seed), so the run is reproducible. orig/g.exe
   is never touched and the patched copy is never committed -- see
   docs/re/combat.md, "Seed pinning", for how to apply and how to undo it.
2. scrhook.com appends a window of the interrupted program's data segment to
   STATE.BIN at every blocking key read, so RandSeed (DS:367Eh) and both
   fighter records (player DS:369Ch + 0x200, enemy DS:3952h) are readable at
   each prompt. The window is checked against the save-file banner at
   DS:369Ch before anything is read out of it.
3. A battle round starts right after the Enter that completes a `k` command:
   the game reads the command line, compares it, and enters the blow loop
   without drawing from the generator in between (1000:4429..1000:445c). So
   RandSeed captured at that Enter is exactly the state the round's first
   Random(100) steps.
4. The round's blow lines are read off the next captured screen, using the
   scroll alignment between the two frames.

Usage:  python3 tools/capture_combat_vectors.py [--out data/combat_vectors.json]
"""
import argparse
import json
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent / "oracle"))
import capture  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent

# --- guest data segment layout (docs/re/combat.md) -------------------------
PLAYER_BASE = 0x389C
ENEMY_BASE = 0x3952
RANDSEED = 0x367E
SAVE_BANNER_AT = 0x369C  # the .SAV magic, in memory; used to check the window
SAVE_BANNER = "^4Gopnik: ^7version 1.02 june,sept 2003"

# Offsets inside a fighter record, i.e. .SAV offsets minus 0x200.
FIELDS_U16 = {
    "strength": 0x02,
    "agility": 0x04,
    "vitality": 0x06,
    "luck": 0x08,
    "level": 0x0A,
    "dmg_min": 0x0C,
    "dmg_max": 0x0E,
    "hp": 0x10,
    "hpmax": 0x12,
}
FIELDS_U8 = {"broken_jaw": 0x14, "broken_leg": 0x15, "armor": 0x16}

# The player's зубная защита. When it is set and the enemy breaks the
# player's jaw, the game draws one extra Random(4) (1000:47fa) that
# src/combat.rs does not model, so enemy-side cases are not taken from a run
# where the player owns it.
TOOTH_GUARD = 0x394A

# System.@Rand's recurrence, from docs/re/rng.md (1f78:11a8). Used only to
# step a captured seed forward by one draw; it is not combat logic and does
# not come from src/.
LCG_MULT = 0x08088405
LCG_INC = 1


def lcg_step(seed: int) -> int:
    return (seed * LCG_MULT + LCG_INC) & 0xFFFFFFFF


BATTLE_PROMPT = "Битва\\"
BATTLE_PROMPT_K = BATTLE_PROMPT + "k"

# One line per blow, printed by the blow loop in FUN_1000_3d11. The player's
# lines come first in a round, then the enemy's.
PLAYER_HIT = re.compile(r"^Ты пнул врага на (-?\d+)з\. У него осталось (-?\d+)$")
PLAYER_MISS = "Ты промазал"
ENEMY_HIT = re.compile(r"^Он пнул тебя на (-?\d+)з\. У тебя осталось (-?\d+)$")
ENEMY_MISS = "Враг промазал"

# Scripts to run. `expect_frames` is pinned per run so a truncated capture
# fails instead of silently producing fewer cases; rediscover it with
# --discover after changing a script.
INTRO = "\\n{district}\\n\\n\\n\\n\\n\\n\\n0\\n\\n"
# `w` wanders; a found enemy asks "Хочешь наехать?" and takes `y`; `k` then
# fights one round per command. Anything not understood at the current prompt
# is ignored by the game, so the same tail drives every district.
TAIL = ("w\\ny\\n" + "k\\n" * 6) * 10
RUNS = [
    # name, district, seed, expected frame count
    ("d0_seed3", 0, 3, 173),
    ("d0_seed77", 0, 77, 173),
    ("d1_seed7", 1, 7, 54),
    ("d1_seed2024", 1, 2024, 173),
    ("d2_seed0", 2, 0, 173),
    ("d2_seed5", 2, 5, 173),
    ("d3_seed11", 3, 11, 173),
    ("d3_seed12345", 3, 12345, 118),
    ("d4_seed42", 4, 42, 173),
    ("d4_seed900", 4, 900, 173),
    ("d5_seed0", 5, 0, 41),
    ("d5_seed99", 5, 99, 41),
    # Extra seeds, added to chase two states the first ten runs never
    # produced: a hit whose damage the defender's armour fully absorbs, and a
    # crit roll landing exactly on luck*3. See docs/re/combat.md.
    ("d1_seed13", 1, 13, 173),
    ("d1_seed555", 1, 555, 84),
    ("d2_seed13", 2, 13, 173),
    ("d2_seed777", 2, 777, 173),
    ("d3_seed4", 3, 4, 86),
    ("d4_seed4", 4, 4, 173),
]


def window_byte(win, off):
    return win[off - capture.STATE_BASE]


def window_u16(win, off):
    i = off - capture.STATE_BASE
    return int.from_bytes(win[i : i + 2], "little")


def check_window(ds, win):
    """The window is only usable if DS really is the game's data segment."""
    n = window_byte(win, SAVE_BANNER_AT)
    text = win[
        SAVE_BANNER_AT - capture.STATE_BASE + 1 : SAVE_BANNER_AT
        - capture.STATE_BASE
        + 1
        + n
    ].decode("cp866")
    if text != SAVE_BANNER:
        raise capture.OracleError(
            f"DS={ds:#06x} does not look like g.exe's data segment: "
            f"expected the save banner at {SAVE_BANNER_AT:#x}, found {text!r}"
        )


def read_fighter(win, base):
    f = {k: window_u16(win, base + off) for k, off in FIELDS_U16.items()}
    f["armor"] = window_byte(win, base + FIELDS_U8["armor"])
    f["broken_jaw"] = window_byte(win, base + FIELDS_U8["broken_jaw"]) != 0
    f["broken_leg"] = window_byte(win, base + FIELDS_U8["broken_leg"]) != 0
    return {
        "level": f["level"],
        "strength": f["strength"],
        "agility": f["agility"],
        "vitality": f["vitality"],
        "luck": f["luck"],
        "armor": f["armor"],
        "dmg_min": f["dmg_min"],
        "dmg_max": f["dmg_max"],
        "hp": f["hp"],
        "hpmax": f["hpmax"],
        "broken_jaw": f["broken_jaw"],
        "broken_leg": f["broken_leg"],
    }


def scroll_delta(before: str, after: str, min_overlap: int = 5):
    """Lines `after` gained over `before`, given a console that scrolls up.

    Returns None when the two screens do not overlap by at least
    `min_overlap` lines -- a cleared or redrawn screen, where "what was added"
    is not answerable and a guess would invent blows that were never printed.
    """
    a = before.split("\n")
    b = after.split("\n")
    n = len(a)
    for shift in range(n - min_overlap + 1):
        if a[shift:] == b[: n - shift]:
            return b[n - shift :]
    return None


def blow_budget(agility: int, opponent_agility: int) -> int:
    """Agility budget for one round -- FUN_1000_3d11 at 1000:3daa/1000:3e26.

    Cross-check only: used here to report how many blows the round should
    have had, never to decide what was printed.
    """
    mine = agility + 4
    theirs = opponent_agility + 4
    if mine > 10:
        while theirs > 18:
            if mine < 28:
                mine = 10
                break
            mine -= 18
            theirs -= 18
    return mine


def accuracy(budget: int) -> int:
    return min(budget * 5, 90)


def parse_round(lines):
    """Split a round's output into the player's blows and the enemy's.

    A round is always the player's blows first, then the enemy's
    (FUN_1000_3d11: the two loops are sequential, 1000:445c and 1000:467f),
    so the first enemy line ends the player's half.
    """
    player, enemy, in_enemy_half = [], [], False
    for line in lines:
        line = line.strip()
        if line == PLAYER_MISS:
            (enemy if in_enemy_half else player).append({"hit": False, "damage": 0})
            continue
        m = PLAYER_HIT.match(line)
        if m:
            (enemy if in_enemy_half else player).append(
                {"hit": True, "damage": int(m.group(1))}
            )
            continue
        if line == ENEMY_MISS:
            in_enemy_half = True
            enemy.append({"hit": False, "damage": 0})
            continue
        m = ENEMY_HIT.match(line)
        if m:
            in_enemy_half = True
            enemy.append({"hit": True, "damage": int(m.group(1))})
    return player, enemy


def truncate_to_opening_accuracy(blows, budget):
    """The leading blows drawn at the round's opening accuracy.

    resolve_blow() answers for one blow at that accuracy; later blows in the
    same round come from a budget 18 lower, so the recorded list stops where
    the effective accuracy changes.
    """
    keep = 0
    while keep < len(blows) and accuracy(budget - 18 * keep) == accuracy(budget):
        keep += 1
    return keep


def harvest(name, frames, states, notes):
    """Turn one captured run into cases."""
    cases = []
    for i in range(len(frames) - 1):
        tail = [l for l in frames[i].split("\n") if l.strip()]
        if not tail or tail[-1].rstrip() != BATTLE_PROMPT_K:
            continue  # not the Enter that completes a `k` command
        added = scroll_delta(frames[i], frames[i + 1])
        if added is None:
            notes.append(f"{name}: frame {i} -> {i + 1} redrawn, round dropped")
            continue
        blows, enemy_blows = parse_round(added)
        if not blows:
            continue

        ds, win = states[i]
        check_window(ds, win)
        attacker = read_fighter(win, PLAYER_BASE)
        defender = read_fighter(win, ENEMY_BASE)
        seed = int.from_bytes(
            win[RANDSEED - capture.STATE_BASE : RANDSEED - capture.STATE_BASE + 4],
            "little",
        )

        # Independent cross-check: the damage the game printed must equal the
        # HP the enemy actually lost, read from the other capture channel.
        _, win_after = states[i + 1]
        hp_after = window_u16(win_after, ENEMY_BASE + FIELDS_U16["hp"])
        hp_before = defender["hp"]
        same_enemy = window_u16(
            win_after, ENEMY_BASE + FIELDS_U16["hpmax"]
        ) == defender["hpmax"]
        enemy_lines = len(enemy_blows)
        if same_enemy and hp_after >= 0x8000:
            hp_after -= 0x10000  # the game lets HP go negative on a kill
        if same_enemy and enemy_lines == 0 and hp_before - hp_after != sum(
            b["damage"] for b in blows
        ):
            raise capture.OracleError(
                f"{name} frame {i}: printed damage "
                f"{sum(b['damage'] for b in blows)} != HP lost "
                f"{hp_before - hp_after}"
            )

        # resolve_blow() answers for one blow at the round's opening accuracy.
        # Later blows in the same round are drawn at a lower budget; keep only
        # the leading run whose effective accuracy is unchanged, so the
        # recorded list is exactly what repeated resolve_blow() calls model.
        budget = blow_budget(attacker["agility"], defender["agility"])
        keep = truncate_to_opening_accuracy(blows, budget)
        if keep < len(blows):
            notes.append(
                f"{name}: frame {i} round had {len(blows)} blows, kept {keep} "
                f"(blow {keep + 1} is drawn at accuracy "
                f"{accuracy(budget - 18 * keep)}%, not {accuracy(budget)}%)"
            )
        cases.append(
            {
                "run": name,
                "frame": i,
                "seed": seed,
                "attacker": attacker,
                "defender": defender,
                "expected_blows": blows[:keep],
                "blows_in_round": len(blows),
                "opening_accuracy_pct": accuracy(budget),
                "attacker_is": "player",
            }
        )

        # The enemy's half of the same round, when -- and only when -- the
        # player's half provably consumed exactly one draw. A round whose
        # player half is a single miss took the straight line
        # 1000:445c -> 1000:447a -> 1000:460b: one Random(100) and no other
        # call, so the enemy's first Random(100) steps lcg_step(seed). Any
        # other shape of player half would need the draw count inferred from
        # the transcript, which is not something to guess at.
        if not enemy_blows:
            continue
        if len(blows) != 1 or blows[0]["hit"]:
            continue
        if window_byte(win, TOOTH_GUARD):
            notes.append(
                f"{name}: frame {i} enemy half skipped, player owns the "
                "зубная защита (extra Random(4) at 1000:47fa is not modelled)"
            )
            continue
        e_budget = blow_budget(defender["agility"], attacker["agility"])
        e_keep = truncate_to_opening_accuracy(enemy_blows, e_budget)
        cases.append(
            {
                "run": name,
                "frame": i,
                "seed": lcg_step(seed),
                "attacker": defender,
                "defender": attacker,
                "expected_blows": enemy_blows[:e_keep],
                "blows_in_round": len(enemy_blows),
                "opening_accuracy_pct": accuracy(e_budget),
                "attacker_is": "enemy",
            }
        )
    return cases


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", type=pathlib.Path, default=ROOT / "data" / "combat_vectors.json")
    ap.add_argument("--work", type=pathlib.Path, default=ROOT / "build" / "combat_capture")
    ap.add_argument(
        "--discover",
        action="store_true",
        help="ignore the pinned expect_frames and print what each run produced",
    )
    args = ap.parse_args()

    cases, notes = [], []
    for name, district, seed, expect in RUNS:
        keys = capture.unescape(INTRO.format(district=district) + TAIL)
        out = args.work / name
        frames = capture.run(
            keys,
            out,
            timeout=180,
            seed=seed,
            expect_frames=None if args.discover else expect,
        )
        states = capture.decode_states((out / "work" / "STATE.BIN").read_bytes())
        if len(states) != len(frames):
            raise capture.OracleError(
                f"{name}: {len(frames)} screens but {len(states)} state records"
            )
        got = harvest(name, frames, states, notes)
        print(f"{name}: {len(frames)} frames, {len(got)} cases", file=sys.stderr)
        cases.extend(got)

    if args.discover:
        return 0

    payload = {
        "note": (
            "Captured from orig/g.exe under DOSBox-X with RandSeed pinned in a "
            "scratch copy of the binary. Blow lines come from the 80x25 text "
            "screen; each fighter's stats and the seed come from the guest's "
            "own data segment (scrhook.com's STATE.BIN window). Nothing here "
            "was produced by the Rust port. Method and addresses: "
            "docs/re/combat.md. Regenerate: python3 "
            "tools/capture_combat_vectors.py"
        ),
        "seed_is": (
            "RandSeed as read from DS:367Eh at the Enter that completed the "
            "`k` command, i.e. the state the round's first Random(100) steps."
        ),
        "expected_blows_is": (
            "The attacker's blows at the head of the round, truncated to the "
            "leading run whose effective accuracy equals the round's opening "
            "accuracy -- that is exactly what repeated resolve_blow() calls "
            "model. blows_in_round records the full count."
        ),
        "capture_notes": notes,
        "cases": cases,
    }
    args.out.write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(f"{len(cases)} cases -> {args.out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
