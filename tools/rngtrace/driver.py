"""Screen-driven input for orig/g.exe in the guest.

Blind key scripts (tools/oracle/capture.py's INTRO_KEYS) drift when a page
takes an extra keypress: a probe run typed the documented sequence and the
class answer landed in the NAME prompt instead.  Every step here waits for the
prompt it is answering, so the character that gets created is the one asked
for -- which matters, because the class decides whether draws 10 and 11 of
docs/re/wander.md are gated on at all.
"""
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


class DriveError(RuntimeError):
    pass


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
