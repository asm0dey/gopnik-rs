"""Screen-driven input for orig/g.exe in the guest.

Blind key scripts (tools/oracle/capture.py's INTRO_KEYS) drift when a page
takes an extra keypress: a probe run typed the documented sequence and the
class answer landed in the NAME prompt instead.  Every step here waits for the
prompt it is answering, so the character that gets created is the one asked
for -- which matters, because the class decides whether draws 10 and 11 of
docs/re/wander.md are gated on at all.
"""
import re
import time

TITLE = "\u041d\u0430\u0436\u043c\u0438 \u043a\u0430\u043a\u0443\u044e-\u043d\u0438\u0431\u0443\u0434\u044c \u043a\u043d\u043e\u043f\u043a\u0443"   # "Нажми какую-нибудь кнопку"
DISTRICT = "\u041d\u0430\u0436\u043c\u0438 \u0446\u0438\u0444\u0440\u0443"                                     # "Нажми цифру ..."
CLASS = "\u0412\u044b\u0431\u0435\u0440\u0438 \u043a\u0435\u043c \u0442\u044b \u0431\u0443\u0434\u0435\u0448\u044c"   # "Выбери кем ты будешь"
NAME = "\u0410 \u0437\u043e\u0432\u0443\u0442 \u0442\u0435\u0431\u044f"                                        # "А зовут тебя"
STREET_PROMPT = "\\"
COMBAT_PROMPT = "\u0411\u0438\u0442\u0432\u0430\\"                                                             # "Битва\"

CLASS_ANSWERS = {0: "\u041f\u0430\u0446\u0430\u043d", 1: "\u041e\u0442\u043c\u043e\u0440\u043e\u0437\u043e\u043a",
                 2: "\u0413\u043e\u043f\u043d\u0438\u043a", 3: "\u0412\u043e\u0440"}
# The stored class value is the menu answer + 3 (1000:71b8).
CLASS_VALUE = {0: 3, 1: 4, 2: 5, 3: 6}
CLASS_NAME_BY_VALUE = {v: CLASS_ANSWERS[k] for k, v in CLASS_VALUE.items()}


class DriveError(RuntimeError):
    pass


def class_record(class_answer, loaded_save, class_389c):
    """What class the run ACTUALLY held, and where that came from.

    `--class-answer` is only what the driver typed at the creation menu.  When a
    save is loaded that menu is never reached, so the answer says nothing about
    the character -- Task 11d's run E loaded SAVE_R3.SAV (a Вор, class 6) and
    still recorded `class_value: 3` from the CLI default, a wrong field about
    the original in a committed artifact.  The guest's own DS:389c is the
    evidence; the answer is at most a corroboration of it, and when the two
    disagree on a run that DID create a character the answer landed in the wrong
    prompt (which has happened -- see this module's docstring), so that is an
    error rather than a note.
    """
    rec = {
        "class_value": class_389c,
        "class_name": CLASS_NAME_BY_VALUE.get(class_389c),
        "class_source": "read from the guest's own DS:389c at the end of the run",
    }
    if loaded_save:
        rec["class_answer"] = None
        rec["class_answer_note"] = (
            "a save was loaded: the class prompt was never reached, so "
            "--class-answer describes nothing about this run")
        return rec
    rec["class_answer"] = class_answer
    want = CLASS_VALUE[class_answer]
    if want != class_389c:
        raise DriveError(
            "the driver answered %d at the class menu (class %d, %s) but the "
            "guest holds class %d (%s) at DS:389c: the answer did not land in "
            "the class prompt, so this character is not the one asked for"
            % (class_answer, want, CLASS_NAME_BY_VALUE.get(want), class_389c,
               CLASS_NAME_BY_VALUE.get(class_389c)))
    rec["class_answer_agrees_with_guest"] = True
    return rec


def last_line(screen: str) -> str:
    for line in reversed(screen.splitlines()):
        if line.strip():
            return line.strip()
    return ""


def at_street(screen: str) -> bool:
    return last_line(screen) == STREET_PROMPT


def at_combat(screen: str) -> bool:
    return last_line(screen).endswith(COMBAT_PROMPT)


def boot_to_dos(vm):
    vm.wait_for("Do you want to proceed", timeout=180)
    vm.type("n\n")
    time.sleep(2.5)
    vm.type("c:\n")
    time.sleep(1.5)


def launch_game(vm):
    vm.type("g\n")
    vm.wait_for(TITLE, timeout=90)


def create_character(vm, class_answer: int, district: str = "1", timeout=180):
    """Answer every creation prompt, each after seeing it.

    Two shapes, both handled: a bare G.EXE goes straight to the story pages and
    the class/name prompts, while a game directory holding the shipped .SAV
    files offers the district prompt first, and answering it with a district
    other than 1 LOADS that save and skips creation entirely.
    """
    vm.wait_for(TITLE, timeout=timeout)
    vm.type("\n")
    saw_district = False
    loaded_save = False
    end = time.time() + timeout
    while time.time() < end:
        s = vm.screen()
        if CLASS in s:
            break
        if at_street(s):                 # a loaded save: no creation prompts
            loaded_save = True
            break
        if DISTRICT in s:
            vm.type(district)
            saw_district = True
        else:
            vm.type("\n")
        time.sleep(0.8)
    else:
        raise DriveError("never reached the class prompt")
    if loaded_save:
        return {"district_prompt_seen": saw_district, "loaded_save": True,
                "district_key": district}
    vm.type("%d\n" % class_answer)
    vm.wait_for(NAME, timeout=timeout)
    vm.type("\n")
    end = time.time() + timeout
    while time.time() < end:
        if at_street(vm.screen()):
            return {"district_prompt_seen": saw_district, "loaded_save": False,
                    "district_key": district}
        time.sleep(1.0)
    raise DriveError("never reached the street prompt after character creation")


def classify(screen: str) -> str:
    """What is the game asking for, judged from the last line it printed."""
    line = last_line(screen)
    if line == STREET_PROMPT:
        return "street"
    if line.endswith(COMBAT_PROMPT):
        return "combat"
    if line.endswith("?"):
        return "question"
    return "other"


def walk(vm, turns: int, timeout=60, max_actions=None):
    """Type `w` at the street prompt `turns` times, answering what comes up.

    A wander turn can leave the street prompt behind: the bucket dispatch at
    1000:b3ba can offer an encounter ("Хочешь наехать?"), and accepting leads
    to the `Битва\\` prompt, which does not accept `w`.  The script this driver
    plays is fixed and stated so the trace can be read against it:

      * street prompt  -> `w`
      * a question     -> `n`   (decline the encounter, decline the mage's save)
      * combat prompt  -> `run` (the flee handler inside 1000:3d11)
      * anything else  -> Enter (an any-key page)

    Draws that the decline path spends are still recorded; they simply sit
    downstream of the bucket dispatch, which docs/re/wander.md puts outside
    the preamble catalogue.
    """
    log = []
    done = 0
    budget = max_actions if max_actions is not None else 30 * turns + 60
    actions = 0
    while done < turns:
        actions += 1
        if actions > budget:
            raise DriveError("driver used %d actions for %d turns; last screen:\n%s"
                             % (actions, turns, vm.screen()))
        what = classify(vm.screen())
        if what == "street":
            vm.type("w\n")
            done += 1
        elif what == "question":
            vm.type("n\n")
        elif what == "combat":
            vm.type("run\n")
        else:
            vm.type("\n")
        log.append({"turn": done, "prompt": what,
                    "typed": {"street": "w", "question": "n",
                              "combat": "run", "other": "<enter>"}[what]})
        time.sleep(1.0)
    # let the last turn finish and come back to a prompt
    end = time.time() + timeout
    while time.time() < end:
        if classify(vm.screen()) in ("street", "combat", "question"):
            break
        time.sleep(0.8)
    return log


class FightLog(dict):
    """The drive's own record, kept as a dict so it serialises straight out."""


# ---------------------------------------------------------------------------
# Task 13: driving a fight.
#
# `walk` above types `run` at the `Битва\\` prompt and `n` at every question,
# which is why not one of the five runs in `data/rng_trace.json` contains a
# draw from inside the blow loop.  `fight` below changes exactly two answers:
#
#   * a question gets `y`.  `1000:b548`, `1000:b696` and `1000:b718` compare
#     the line against the literal `y` (file 0x9BF3) and `jnz` on anything
#     else, so this is the ACCEPT arm of the same compare `walk` declined --
#     and it is what makes fights happen at all.  Declining produced 0 fights
#     in 30 walks on a high-luck save: the aggressive block that can start a
#     fight without consent needs `luck < notice` (`1000:b5fc`), which a lucky
#     character rarely loses.
#   * the `Битва\\` prompt gets `combat_answer` -- `k` (1000:4440) to swing or
#     `run` (1000:48e1) to flee.
#
# A turn counts as DONE when the street prompt comes back, not when `w` is
# typed, because a fight can end the process (`^4Ты сдох.` -> FUN_1000_074b ->
# Halt) and a turn that ended that way was never taken.
#
# Two answers means the port replaying this run cannot feed one constant
# string the way `tests/wander_sequence.rs` does.  So the drive records the
# ordered list of lines it typed at the ENCOUNTER prompts -- every question
# and every `Битва\\` prompt -- and the port is fed exactly that.
#
# `lines_the_game_read` is therefore NOT every line the guest's own `ReadLn`s
# consumed, despite the name, and the name cannot be changed: it is a key in
# the frozen `data/combat_trace.json`, which is never regenerated.  Two
# exclusions, both deliberate:
#
#   * the `w` typed at the street prompt IS read, by the top-level `ReadLn`
#     at `1000:ae63` (`call far 0f78:06c6`, `docs/re/command-dispatch.md`).
#     It is left out because the port's `Game::walk` IS the turn -- it never
#     reads a verb -- so feeding `w` to it would be handing the next fight's
#     answer to nobody.  The turn count, not this list, is what drives the
#     replay's walks.
#   * Enter typed at an any-key page is left out because no `ReadLn` of the
#     game's own consumes it at all.
#
# So the `1000:441d` cross-check below validates the COMBAT subset of the
# input, which is the subset the port is fed.
#
# That list is only usable if the driver's screen classification agreed with
# what the guest actually did, so it is cross-checked rather than trusted: the
# guest's own breakpoint at `1000:441d` counts the `Битва\\` prompts, and
# `fightrun.py` refuses a run where that count differs from the number of
# lines this driver typed at a screen it called `combat`.
DEAD = "Ты сдох"        # "Ты сдох"
ACCEPT = "y"            # file 0x9BF3 -- the literal 1000:b548/b696/b718 compare against

# `C:\>` -- the FreeDOS command prompt.  Matched on its whole shape rather than
# on a trailing `>`, so a game line that happens to end in `>` cannot be read as
# the game having exited.
RE_DOS_PROMPT = re.compile(r"^[A-Za-z]:\\[^>]*>$")


def game_gone(screen: str) -> bool:
    """The game is no longer running: the guest is back at a DOS prompt.

    Judged on the shape DOS leaves behind, not on the death message -- the
    death message scrolls off under `FUN_1000_074b`'s end screen, so looking
    for it would miss the very case this is here to detect.
    """
    return RE_DOS_PROMPT.match(last_line(screen)) is not None


def settled_screen(vm, tries=6, pause=0.35):
    """A screen the game has finished writing.

    Classifying a half-written screen is how a fight capture loses its
    alignment: mid-round the last line is a blow message, which reads as
    `other`, and the Enter that answers it IS consumed by the `Битва\\`
    prompt's `ReadLn` -- a line the game read that the driver would not have
    recorded.  Two identical consecutive reads mean the guest is blocked on
    input, which is the only state it stays still in.
    """
    prev = vm.screen()
    for _ in range(tries):
        time.sleep(pause)
        cur = vm.screen()
        if cur == prev:
            return cur
        prev = cur
    return prev


def fight(vm, turns: int, combat_answer: str = "k", timeout=60,
          max_actions=None):
    """Walk `turns` times, accepting every encounter and answering the
    `Битва\\` prompt with `combat_answer`.

    Returns a record with the action log, the ordered list of lines typed at
    the ENCOUNTER prompts (`lines_the_game_read` -- see the comment above
    this function for the two things it deliberately excludes, including the
    street `w` that `1000:ae63`'s `ReadLn` does consume), how many turns
    actually completed, whether the guest left the game, and the last screen
    -- all of which the capture publishes, because "the drive stopped early"
    must be a stated fact and not something inferred from a short trace.
    """
    if combat_answer not in ("k", "run"):
        raise DriveError("combat_answer must be `k` or `run`, not %r: those are "
                         "the two arms of FUN_1000_3d11's dispatch this driver "
                         "knows how to reach (1000:4440 and 1000:48e1)"
                         % combat_answer)
    log = []
    lines = []
    combat_typings = 0
    done = 0
    started = 0
    budget = max_actions if max_actions is not None else 40 * turns + 200
    actions = 0
    exited = False
    while done < turns:
        actions += 1
        if actions > budget:
            raise DriveError("driver used %d actions for %d turns; last screen:\n%s"
                             % (actions, turns, vm.screen()))
        screen = settled_screen(vm)
        if game_gone(screen):
            exited = True
            break
        what = classify(screen)
        if what == "street":
            if started > done:
                done = started
            if done >= turns:
                break
            vm.type("w\n")
            started = done + 1
            typed = "w"
        elif what == "combat":
            vm.type(combat_answer + "\n")
            typed = combat_answer
            combat_typings += 1
            lines.append(typed)
        elif what == "question":
            vm.type(ACCEPT + "\n")
            typed = ACCEPT
            lines.append(typed)
        else:
            vm.type("\n")
            typed = "<enter>"
        log.append({"turn": done, "prompt": what, "typed": typed})
    end = time.time() + timeout
    while time.time() < end:
        s = settled_screen(vm)
        if game_gone(s):
            exited = True
            break
        if classify(s) == "street":
            done = started
            break
        if classify(s) in ("combat", "question"):
            break
    return FightLog(actions=log, turns_requested=turns, turns_completed=done,
                    combat_answer=combat_answer, guest_left_the_game=exited,
                    lines_the_game_read=lines,
                    combat_prompts_typed_at=combat_typings,
                    last_screen=vm.screen())
