#!/usr/bin/env python3
r"""Capture XP-award and level-threshold cases from orig/g.exe (Task 9b).

Regenerates the `award_cases`, `level_up_cases` and `threshold_observations`
sections of data/xp.json. Nothing here reads, links or shells out to the Rust
crate: every number comes out of the original program -- the award and the
status line off the 80x25 text screen, the fighters' records and the XP/
threshold words out of the guest's own data segment. Numbers produced by the
implementation under test would prove nothing, so this tool must stay
independent of src/progress.rs.

How a case is pinned down
-------------------------
1. The scratch copy of g.exe gets System.Randomize's body replaced by a
   constant store (capture.pin_seed), so the run is reproducible. orig/g.exe
   is never touched and the patched copy is never committed -- see
   docs/re/combat.md, "Seed pinning".
2. scrhook.com appends a window of the interrupted program's data segment to
   STATE.BIN at every blocking key read, so the player record (DS:389Ch), the
   enemy record (DS:3952h), the XP total (DS:38CEh) and the next-level
   threshold (DS:38D0h) are readable at each prompt.
3. A kill prints `За отпин врага ты получаешь # качков опыта` (1000:51b4).
   The state captured at the key read that *preceded* that screen is the
   "before" state: the enemy still holds the record the award was summed
   from, and the player's XP has not been credited yet.
4. The sequence ends with `^6Сейчас у тебя # качков опыта. До слеующей
   прокачки надо #` -- printed either by the combat function when no level
   was gained (1000:521b) or by the level-up routine when one was
   (1000:28a6). The state captured at the next key read after that line is
   the "after" state.

Usage:  python3 tools/capture_xp_cases.py [--out data/xp_cases.json]
        python3 tools/capture_xp_cases.py --discover   # re-pin frame counts
"""
import argparse
import json
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent / "oracle"))
import capture  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent

# --- guest data segment layout (docs/re/combat.md, docs/re/progression.md) --
PLAYER_BASE = 0x389C
ENEMY_BASE = 0x3952
XP = 0x38CE
THRESHOLD = 0x38D0
SAVE_BANNER_AT = 0x369C
SAVE_BANNER = "^4Gopnik: ^7version 1.02 june,sept 2003"

STAT_OFFSETS = {
    "class": 0x00,
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

AWARD = re.compile(r"За отпин врага ты получаешь (\d+) качков опыта")
# Two different status lines close a kill, both printing (xp, threshold):
# 1000:28a6 (CS:24ea) after a level-up, 1000:521b (CS:39f8) when the kill did
# not reach the threshold. Typos are the original's; kept verbatim.
STATUS = re.compile(
    r"Сейчас у тебя (\d+) качков опыта"
    r"(?:\. До слеующей прокачки надо |, А для прокачки надо )(\d+)"
)
# The four per-level gain messages (1000:2621 .. 1000:281d). Kept verbatim,
# markup stripped by the screen capture already.
GAIN_WORDS = {
    "Сила +1": "strength",
    "Ловкость +1": "agility",
    "Живучесть +1": "vitality",
    "Удача +1": "luck",
}
LEVEL_UP_LINE = "Понтовость увеличивается:"

# District 1 starts a fresh level-0 character; 0 and 2..5 load the shipped
# SAVE_R0/R2..R5 files, which is how a run reaches thresholds a fresh
# character cannot grind to inside one 1024-key script.
INTRO = "\\n{district}\\n\\n\\n\\n\\n\\n\\n0\\n\\n"
TAIL = ("w\\n" * 3 + "y\\n" + "k\\n" * 6) * 50

# A run ends when the player dies (the game drops back to the title screen),
# so `expect_frames` is well below the 1024-key budget for most of them. It is
# still pinned: under a pinned seed the death point is deterministic, and a
# short capture would silently drop cases.
RUNS = [
    # name, district, seed, expected frame count
    ("fresh_seed5", 1, 5, 206),
    ("fresh_seed11", 1, 11, 508),
    ("fresh_seed123", 1, 123, 272),
    ("save_r0_seed3", 0, 3, 1010),
    ("save_r2_seed3", 2, 3, 412),
    ("save_r3_seed3", 3, 3, 1013),
    ("save_r4_seed4", 4, 4, 926),
]

# --- constants read straight out of orig/g.exe ------------------------------
# Physical file offset of DS (Ghidra segment 20ae): the load image starts at
# 0x18d0 and code segment 1000 is its origin, so DS:0000 is
# 0x18d0 + (0x20ae - 0x1000) * 16. Cross-checked by decoding the rank-name
# table at DS:002e, whose first entry reads "Дохляк".
DS_FILE_BASE = 0x18D0 + (0x20AE - 0x1000) * 16
CS_FILE_BASE = 0x18D0
CLASS_WEIGHTS_AT = 0x0002  # DS:0002, four bytes per class (1000:25aa..25b6)
CLASS_NAMES_AT = 0x002E  # DS:002e, 256-byte-stride shortstrings (1000:13dc)
N_CLASSES = 11
# The four `mov word [stat],imm` immediates the class prompt stores, per
# answer (1000:7148, 1000:7167, 1000:7186, 1000:71a0). Each instruction is
# `c7 06 <addr16> <imm16>`, so the immediate sits four bytes in.
START_STAT_SITES = {0: 0x71A0, 1: 0x7148, 2: 0x7167, 3: 0x7186}
CLASS_OF_ANSWER_ADD = 0x71B8  # `add word [0x389c],0x3`

# The four scalar constants of FUN_1000_2526, each read out of its own
# instruction's immediate rather than hand-transcribed -- the same
# opcode-checked-before-taking-an-immediate method used above for the class
# weights and the starting stats.
MAX_LEVEL_AT = 0x2580  # `cmp word [0x38a6],imm8` -- the понтовость cap
GAINS_PER_LEVEL_AT = 0x287D  # `cmp word [bp-0x8],imm8` -- draws per level
THRESHOLD_BASE_AT = 0x6DE0  # `mov word [0x38d0],imm16` -- a new character's first threshold
THRESHOLD_STEP_AT = 0x2550  # `add word [0x38d0],imm8` -- per-level threshold step


def read_scalar_constants(blob):
    """MAX_LEVEL, GAINS_PER_LEVEL, THRESHOLD_BASE and THRESHOLD_STEP, out of
    the immediates of the instructions that set them, each checked against
    its expected opcode bytes before the immediate is taken.
    """
    at = CS_FILE_BASE + MAX_LEVEL_AT
    if blob[at] != 0x83 or blob[at + 1] != 0x3E or blob[at + 2 : at + 4] != bytes.fromhex("a638"):
        raise capture.OracleError(
            f"1000:{MAX_LEVEL_AT:04x} is not `cmp word [0x38a6],imm8`"
        )
    max_level = blob[at + 4]

    at = CS_FILE_BASE + GAINS_PER_LEVEL_AT
    if blob[at] != 0x83 or blob[at + 1] != 0x7E or blob[at + 2] != 0xF8:
        raise capture.OracleError(
            f"1000:{GAINS_PER_LEVEL_AT:04x} is not `cmp word [bp-0x8],imm8`"
        )
    gains_per_level = blob[at + 3]

    at = CS_FILE_BASE + THRESHOLD_BASE_AT
    if blob[at] != 0xC7 or blob[at + 1] != 0x06 or blob[at + 2 : at + 4] != bytes.fromhex("d038"):
        raise capture.OracleError(
            f"1000:{THRESHOLD_BASE_AT:04x} is not `mov word [0x38d0],imm16`"
        )
    threshold_base = int.from_bytes(blob[at + 4 : at + 6], "little")

    at = CS_FILE_BASE + THRESHOLD_STEP_AT
    if blob[at] != 0x83 or blob[at + 1] != 0x06 or blob[at + 2 : at + 4] != bytes.fromhex("d038"):
        raise capture.OracleError(
            f"1000:{THRESHOLD_STEP_AT:04x} is not `add word [0x38d0],imm8`"
        )
    threshold_step = blob[at + 4]

    return {
        "max_level": max_level,
        "gains_per_level": gains_per_level,
        "threshold_base": threshold_base,
        "threshold_step": threshold_step,
    }


# Player-record words, by DS address (docs/re/combat.md, "The fighter record").
PLAYER_WORDS = {
    0x389C: "class",
    0x389E: "strength",
    0x38A0: "agility",
    0x38A2: "vitality",
    0x38A4: "luck",
    0x38A6: "level",
    0x38A8: "dmg_min",
    0x38AA: "dmg_max",
    0x38AC: "hp",
    0x38AE: "hpmax",
}

# The one-shot stat grants in the post-kill block of FUN_1000_3d11, each
# behind its own "already had this" flag byte. Reached only when the
# Random(0x1e) at 1000:52d5 comes up 0. Deltas are not written here: they are
# decoded out of the instructions in the address range, so a misreading of the
# listing cannot survive into the artifact.
STAT_EVENTS = [
    {
        "name": "event_1",
        "flag_ds": 0x38BF,
        "range": [0x532F, 0x5362],
        "note": (
            "dmg_min also gains `1 - strength mod 2` (1000:534d..1000:5361), "
            "which is not a constant and so is not in `deltas`."
        ),
    },
    {
        "name": "event_2",
        "flag_ds": 0x38C0,
        "range": [0x538A, 0x53B2],
        "note": "",
    },
    {
        "name": "luck_plus_2",
        "flag_ds": 0x38BD,
        "range": [0x5493, 0x5498],
        "note": "",
    },
    {
        "name": "luck_plus_1",
        "flag_ds": 0x38BE,
        "range": [0x54C4, 0x54C8],
        "note": "",
    },
]


def decode_stat_deltas(blob, start, end):
    """Constant increments to player-record words inside a code range.

    Recognises `inc word [addr]` (ff 06) and `add word [addr],imm8`
    (83 06 ... ), the only two forms these blocks use. Anything else touching
    a player word is reported rather than dropped, so a block whose gain is
    computed at run time cannot quietly turn into a constant here.
    """
    deltas, other = {}, []
    i = CS_FILE_BASE + start
    stop = CS_FILE_BASE + end
    while i < stop:
        addr = int.from_bytes(blob[i + 2 : i + 4], "little")
        if blob[i] == 0xFF and blob[i + 1] == 0x06 and addr in PLAYER_WORDS:
            deltas[PLAYER_WORDS[addr]] = deltas.get(PLAYER_WORDS[addr], 0) + 1
            i += 4
            continue
        if blob[i] == 0x83 and blob[i + 1] == 0x06 and addr in PLAYER_WORDS:
            deltas[PLAYER_WORDS[addr]] = deltas.get(PLAYER_WORDS[addr], 0) + blob[i + 4]
            i += 5
            continue
        if blob[i] == 0x01 and blob[i + 1] == 0x06 and addr in PLAYER_WORDS:
            other.append(f"1000:{i - CS_FILE_BASE:04x} add [{PLAYER_WORDS[addr]}],reg")
        i += 1
    return deltas, other


def read_exe_constants(exe: pathlib.Path):
    blob = exe.read_bytes()
    banner_at = DS_FILE_BASE + CLASS_NAMES_AT
    if blob[banner_at + 1 : banner_at + 1 + blob[banner_at]].decode("cp866") != "Дохляк":
        raise capture.OracleError(
            f"{exe}: DS:{CLASS_NAMES_AT:#06x} is not the rank-name table; "
            "refusing to read constants out of it"
        )
    weights, names = [], []
    for i in range(N_CLASSES):
        off = DS_FILE_BASE + CLASS_WEIGHTS_AT + 4 * i
        weights.append(list(blob[off : off + 4]))
        n_at = DS_FILE_BASE + CLASS_NAMES_AT + 256 * i
        names.append(blob[n_at + 1 : n_at + 1 + blob[n_at]].decode("cp866"))
    start = {}
    for answer, cs in START_STAT_SITES.items():
        vals = []
        for k in range(4):
            at = CS_FILE_BASE + cs + 6 * k
            if blob[at] != 0xC7 or blob[at + 1] != 0x06:
                raise capture.OracleError(
                    f"{exe}: 1000:{cs + 6 * k:04x} is not `mov word [addr],imm`"
                )
            vals.append(int.from_bytes(blob[at + 4 : at + 6], "little"))
        start[answer] = vals
    events = []
    for ev in STAT_EVENTS:
        deltas, other = decode_stat_deltas(blob, ev["range"][0], ev["range"][1])
        events.append(
            {
                "name": ev["name"],
                "flag_save_offset": ev["flag_ds"] - 0x389C + 0x200,
                "at": "1000:%04x" % ev["range"][0],
                "deltas": deltas,
                "non_constant_writes": other,
                "note": ev["note"],
            }
        )
    at = CS_FILE_BASE + CLASS_OF_ANSWER_ADD
    if blob[at : at + 4] != bytes.fromhex("83069c38"):
        raise capture.OracleError(
            f"{exe}: 1000:{CLASS_OF_ANSWER_ADD:04x} is not `add word [0x389c],imm8`"
        )
    scalars = read_scalar_constants(blob)
    return {
        "class_weights": weights,
        "class_names": names,
        "start_stats": {str(k): v for k, v in sorted(start.items())},
        "class_of_answer_offset": blob[at + 4],
        **scalars,
        "post_kill_stat_events_is": (
            "One-shot stat grants in the post-kill block of FUN_1000_3d11, "
            "each behind the flag byte at `flag_save_offset` in the .SAV "
            "file. They fire only when the Random(0x1e) at 1000:52d5 comes up "
            "0 and the flag is still clear. `deltas` are decoded out of the "
            "instructions at `at`, not transcribed by hand. These are not "
            "level-ups; they are the reason a character's stats exceed what "
            "its growth log alone accounts for."
        ),
        "post_kill_stat_events": events,
    }


def window_u16(win, off):
    i = off - capture.STATE_BASE
    return int.from_bytes(win[i : i + 2], "little")


def check_window(ds, win):
    n = win[SAVE_BANNER_AT - capture.STATE_BASE]
    i = SAVE_BANNER_AT - capture.STATE_BASE + 1
    text = win[i : i + n].decode("cp866")
    if text != SAVE_BANNER:
        raise capture.OracleError(
            f"DS={ds:#06x} does not look like g.exe's data segment: "
            f"expected the save banner at {SAVE_BANNER_AT:#x}, found {text!r}"
        )


def read_record(win, base):
    return {k: window_u16(win, base + off) for k, off in STAT_OFFSETS.items()}


def read_progress(win):
    return {
        "level": window_u16(win, PLAYER_BASE + STAT_OFFSETS["level"]),
        "xp": window_u16(win, XP),
        "threshold": window_u16(win, THRESHOLD),
    }


def scroll_delta(before: str, after: str, min_overlap: int = 5):
    """Lines `after` gained over `before`, given a console that scrolls up.

    Same helper as tools/capture_combat_vectors.py. Returns None when the two
    screens do not overlap, i.e. the screen was redrawn and "what was added"
    is not answerable.
    """
    a = before.split("\n")
    b = after.split("\n")
    n = len(a)
    for shift in range(n - min_overlap + 1):
        if a[shift:] == b[: n - shift]:
            return b[n - shift :]
    return None


def harvest(name, seed, frames, states, notes):
    deltas = [scroll_delta(frames[i], frames[i + 1]) for i in range(len(frames) - 1)]
    cases = []
    for i, added in enumerate(deltas):
        if added is None:
            continue
        m = AWARD.search("\n".join(added))
        if not m:
            continue
        award = int(m.group(1))

        # Walk forward to the status line that closes the sequence, gathering
        # the level-up messages printed on the way.
        gains, levels_announced, end = [], 0, None
        status = None
        for j in range(i, min(i + 12, len(deltas))):
            if deltas[j] is None:
                break
            text = "\n".join(deltas[j])
            levels_announced += text.count(LEVEL_UP_LINE)
            for line in deltas[j]:
                for word, stat in GAIN_WORDS.items():
                    gains.extend([stat] * line.count(word))
            s = STATUS.search(text)
            if s:
                status = (int(s.group(1)), int(s.group(2)))
                end = j + 1
                break
        if status is None:
            notes.append(f"{name}: frame {i} award {award} has no closing status line")
            continue

        ds_b, win_b = states[i]
        ds_a, win_a = states[end]
        check_window(ds_b, win_b)
        check_window(ds_a, win_a)
        enemy = read_record(win_b, ENEMY_BASE)
        before = read_progress(win_b)
        player_before = read_record(win_b, PLAYER_BASE)
        after = read_progress(win_a)
        player_after = read_record(win_a, PLAYER_BASE)
        cases.append(
            {
                "run": name,
                "seed": seed,
                "frame": i,
                "enemy": enemy,
                "player_before": player_before,
                "player_after": player_after,
                "xp_before": before["xp"],
                "threshold_before": before["threshold"],
                "level_before": before["level"],
                "award_printed": award,
                "xp_after": after["xp"],
                "threshold_after": after["threshold"],
                "level_after": after["level"],
                "status_line": {"xp": status[0], "threshold": status[1]},
                "levels_announced": levels_announced,
                "gains_announced": gains,
            }
        )
    return cases


def reference_save_observations():
    """(level, threshold) read straight out of the five shipped .SAV files.

    Save offsets: level at 0x20a, next-level threshold at 0x234 (see
    docs/re/save-format.md). These are original data files, independent of
    both the oracle runs and the Rust port.
    """
    obs = {}
    for name in ("SAVE_R0", "SAVE_R2", "SAVE_R3", "SAVE_R4", "SAVE_R5"):
        blob = (ROOT / "orig" / f"{name}.SAV").read_bytes()
        level = int.from_bytes(blob[0x20A:0x20C], "little")
        threshold = int.from_bytes(blob[0x234:0x236], "little")
        obs.setdefault(level, []).append((f"orig/{name}.SAV", threshold))
    return obs


def build_thresholds(cases, max_level, threshold_base, threshold_step):
    """thresholds[i] = XP needed to go from level i+1 to level i+2.

    Every entry carries where it came from. An entry no run and no shipped
    save ever showed is marked UNVERIFIED: the value is what the two
    instructions that maintain the word predict, not something observed.

    `max_level`, `threshold_base` and `threshold_step` come from
    `read_scalar_constants` -- opcode-checked reads out of the binary, not
    hand-transcribed literals.
    """
    observed = {}
    for c in cases:
        for lvl, thr, when in (
            (c["level_before"], c["threshold_before"], "before"),
            (c["level_after"], c["threshold_after"], "after"),
        ):
            observed.setdefault(lvl, []).append(
                (f"oracle {c['run']} frame {c['frame']} ({when})", thr)
            )
    for lvl, entries in reference_save_observations().items():
        observed.setdefault(lvl, []).extend(entries)

    values, provenance = [], []
    for level in range(1, max_level + 1):
        predicted = threshold_base + threshold_step * level
        seen = observed.get(level, [])
        for where, thr in seen:
            if thr != predicted:
                raise capture.OracleError(
                    f"threshold for level {level}: {where} says {thr}, "
                    f"1000:2550/1000:6de0 predict {predicted}"
                )
        values.append(predicted)
        if seen:
            provenance.append(
                "observed: " + "; ".join(sorted({w for w, _ in seen}))
            )
        else:
            provenance.append(
                "UNVERIFIED by observation -- no scripted run and no shipped "
                "save reaches this level. Value is what 1000:6de0 "
                "(threshold := 10 on a new character) and 1000:2550 "
                "(threshold += 10 per level) give; the same two instructions "
                "are confirmed at the levels that are observed."
            )
    return values, provenance


def build_award_cases(cases):
    return [
        {
            "run": c["run"],
            "frame": c["frame"],
            "player_level": c["level_before"],
            "enemy": c["enemy"],
            "expected": c["award_printed"],
        }
        for c in cases
    ]


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", type=pathlib.Path, default=ROOT / "data" / "xp.json")
    ap.add_argument(
        "--work", type=pathlib.Path, default=ROOT / "build" / "xp_capture"
    )
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
            timeout=600,
            seed=seed,
            expect_frames=None if args.discover else expect,
        )
        states = capture.decode_states((out / "work" / "STATE.BIN").read_bytes())
        if len(states) != len(frames):
            raise capture.OracleError(
                f"{name}: {len(frames)} screens but {len(states)} state records"
            )
        got = harvest(name, seed, frames, states, notes)
        print(f"{name}: {len(frames)} frames, {len(got)} cases", file=sys.stderr)
        cases.extend(got)

    if args.discover:
        return 0
    if not cases:
        raise capture.OracleError("no cases captured")

    exe_consts = read_exe_constants(ROOT / "orig" / "g.exe")
    thresholds, provenance = build_thresholds(
        cases,
        exe_consts["max_level"],
        exe_consts["threshold_base"],
        exe_consts["threshold_step"],
    )
    payload = {
        "note": (
            "Task 9b. Two independent sources, neither of them the Rust port. "
            "(1) orig/g.exe itself: the class weight table, the rank names and "
            "the character-creation stat immediates are read out of the load "
            "image at fixed file offsets. (2) The Task 3 oracle: orig/g.exe "
            "run under DOSBox-X with RandSeed pinned in a scratch copy, with "
            "the award and the status line read off the 80x25 text screen and "
            "the fighter records, XP total and threshold read out of the "
            "guest's own data segment. Addresses and method: "
            "docs/re/progression.md. Regenerate: python3 "
            "tools/capture_xp_cases.py"
        ),
        "max_level": exe_consts["max_level"],
        "gains_per_level": exe_consts["gains_per_level"],
        "thresholds_is": (
            "thresholds[i] is the XP needed to go from level i+1 to level "
            "i+2, i.e. xp_to_next(i+1). threshold_provenance[i] says where "
            "that entry was seen; entries marked UNVERIFIED were never "
            "reached by a scripted run or a shipped save and carry only what "
            "1000:2550 and 1000:6de0 predict."
        ),
        "thresholds": thresholds,
        "threshold_provenance": provenance,
        "award_cases_is": (
            "One per kill captured. `expected` is the number the game printed "
            "in `За отпин врага ты получаешь # качков опыта` (1000:51b4); "
            "`enemy` is that enemy's record, read out of the guest's data "
            "segment at DS:3952 at the key read immediately before. "
            "`player_level` is the player's level at the same moment -- the "
            "award does not depend on it, which these cases are what shows."
        ),
        "award_cases": build_award_cases(cases),
        "level_up_cases_is": (
            "The same kills, with the XP bookkeeping either side: the XP "
            "total (DS:38ce), the next-level threshold (DS:38d0) and the "
            "level (DS:38a6) before and after, the two numbers the status "
            "line printed, how many `Понтовость увеличивается:` lines the "
            "screen showed and which stat each `+1` line named."
        ),
        "level_up_cases": cases,
        "capture_notes": notes,
    }
    payload.update(exe_consts)
    args.out.write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(f"{len(cases)} cases -> {args.out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
