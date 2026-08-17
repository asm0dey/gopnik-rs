# GOPNIK v1.02 — Bit-Faithful Rust Port — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reimplement the 2003 Borland Pascal 7.0 DOS text-RPG "ГОПНИК v1.02" (`g.exe`, 88656 bytes) in Rust so that game logic — RNG, combat math, stat growth, prices, save format — reproduces the original's numbers exactly.

**Architecture:** Three layers, built in order. (1) A Python RE toolchain that extracts facts from `g.exe` into checked-in JSON/Markdown artifacts. (2) A DOSBox-X oracle harness that drives the real binary with scripted keystrokes and captures screen text, producing ground-truth vectors. (3) A Rust crate whose logic modules are validated against those vectors. The Rust game never parses `g.exe` at runtime — it consumes the extracted JSON.

**Tech Stack:** Rust 1.97.1 (stdlib + `serde`/`serde_json` only), Python 3.14 (stdlib only), Ghidra 12.1.2 driven by **Java** scripts via `analyzeHeadless`, DOSBox-X 2026.08.02.

## Global Constraints

- **Fidelity target is game logic, not terminal bytes.** Damage rolls, hit chance, XP thresholds, prices, RNG sequence, and save file bytes must match the original exactly. Screen layout/ANSI output need only be faithful in content and colour index, not in exact cursor positioning.
- **Source text is CP866.** All extracted strings are converted to UTF-8 exactly once, at extraction time, and stored as UTF-8 in JSON. No CP866 handling anywhere in the Rust crate.
- **`^0`–`^7` are markup, not content.** They are colour-change directives in
  the original's own display language. Never treat them as literal characters:
  never print them, never measure string width with them included, never let
  them reach a comparison of user-visible text, and never concatenate them into
  a name or label as if they were part of it. Parse them into structured spans
  at the boundary and re-emit styling on output. The only place the raw
  sigils are permitted is inside byte-exact save round-trips, where they are
  part of the original file's bytes and must be preserved verbatim.
- **All Russian game text is preserved verbatim.** No translation, no censoring, no rewording. The game contains deliberate crude slang; that is the content.
- **Ghidra is driven by Java scripts only.** PyGhidra requires `jpype1`, which does not build on Python 3.14. Do not attempt `pip install pyghidra`.
- **Python tooling uses the standard library only.** No pip installs, no venv.
- **Rust dependencies limited to `serde` + `serde_json`.** Anything else requires explicit sign-off.
- **Never modify files under `orig/`.** They are the reference corpus and are checked in read-only.
- **Every RE finding lands in two places:** a human-readable note under `docs/re/` citing the Ghidra address, and a machine-readable artifact under `data/`. A finding that exists only in a commit message does not count.
- **Unknown means unknown.** If a field's meaning is not established, name it `unk_<hex_offset>` and preserve its bytes. Never guess a semantic name to make a table look finished.

## Reference facts (already verified — do not re-derive)

| Fact | Value |
|---|---|
| `g.exe` MD5 | `10eb0af07a2d2f5e9da790df7058891c` |
| Size / format | 88656 B, MZ real-mode, Borland Pascal 7.0 (1992 RTL), no overlay |
| MZ header | 1580 relocations, header 6352 B, image 88656 B |
| Ghidra recovery | 123 functions, 19480 instructions, 1610 defined data items |
| Memory blocks | `CODE_0` 1000:0000–1000:ee4f (61008 B), `CODE_1` 1ee5, `CODE_2` 1eed, `CODE_3` 1f16, `CODE_4` 1f78, `CODE_5` 20ae:0000–20ae:369f (13984 B), `DATA` 20ae:36a0–20ae:811f (19072 B) |
| Pascal shortstrings | 696 CP866 strings recoverable by length-prefix scan |
| Version strings | `Gopnik: version 1.02 june,sept 2003`, `by V.P.U` |
| Save file | 694 B = `string[255]` magic + `string[255]` name + 182 B state |
| `PLACES.SAV` | 7 B, one flag per rediscoverable location |
| Colour codes | `^0`–`^7` inline; `#` is a numeric placeholder in format strings |
| Command verbs | `bmar mar rep girl pr kl trn s w f i hp sv name kos wes help` + keys `a d e h k t x` |

**Explicitly NOT established** (these are RE tasks, not assumptions):
- The RNG algorithm. Borland's `0x08088405` LCG multiplier is **absent** from this binary; the constant does not appear contiguously and there is no `mov ax,8405`/`mov dx,0808` pair. Task 8 must identify the actual generator from disassembly.
- The meaning of state words at save offset `0x200+0x00`–`0x0E` and everything past `0x14`.
- The `02 3x 3x` repeating records near the end of the save (Pascal `string[2]` of ASCII digits `1`–`4`, count varies 10–39 across saves).

---

### Task 1: Project skeleton and reference corpus

**Files:**
- Create: `/home/finkel/work_self/gopnik-rs/.gitignore`
- Create: `/home/finkel/work_self/gopnik-rs/README.md`
- Create: `/home/finkel/work_self/gopnik-rs/tools/verify_corpus.py`

**Already present — do NOT create, copy, or modify:** the repo itself, the
directory skeleton, and `orig/` with all eight reference files. They were
placed before this task began, because they originate outside the repo.

**Interfaces:**
- Produces: `orig/g.exe` and five `orig/SAVE_R*.SAV` at fixed paths; every later task reads these.

- [ ] **Step 1: Confirm the corpus is in place**

```bash
cd /home/finkel/work_self/gopnik-rs
ls -la orig/
```

Expected exactly these eight files: `g.exe`, `PLACES.SAV`, `README!`, `SAVE_R0.SAV`, `SAVE_R2.SAV`, `SAVE_R3.SAV`, `SAVE_R4.SAV`, `SAVE_R5.SAV`.

If any are missing, STOP and report NEEDS_CONTEXT. Do not attempt to source
them yourself — they are irreplaceable reference data.

- [ ] **Step 2: Write the corpus verification test**

Create `tools/verify_corpus.py`:

```python
#!/usr/bin/env python3
"""Fail loudly if the reference corpus is missing or altered."""
import hashlib
import pathlib
import sys

ORIG = pathlib.Path(__file__).resolve().parent.parent / "orig"

EXPECTED = {
    "g.exe": ("10eb0af07a2d2f5e9da790df7058891c", 88656),
    "PLACES.SAV": (None, 7),
    "SAVE_R0.SAV": (None, 694),
    "SAVE_R2.SAV": (None, 694),
    "SAVE_R3.SAV": (None, 694),
    "SAVE_R4.SAV": (None, 694),
    "SAVE_R5.SAV": (None, 694),
}


def main() -> int:
    failures = []
    for name, (want_md5, want_size) in EXPECTED.items():
        path = ORIG / name
        if not path.exists():
            failures.append(f"{name}: MISSING")
            continue
        blob = path.read_bytes()
        if len(blob) != want_size:
            failures.append(f"{name}: size {len(blob)} != {want_size}")
        if want_md5 is not None:
            got = hashlib.md5(blob).hexdigest()
            if got != want_md5:
                failures.append(f"{name}: md5 {got} != {want_md5}")
    for line in failures:
        print("FAIL", line)
    if failures:
        return 1
    print(f"OK {len(EXPECTED)} corpus files verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 3: Run it and confirm it passes**

Run: `python3 tools/verify_corpus.py`
Expected: `OK 7 corpus files verified`, exit 0.

- [ ] **Step 4: Confirm it actually detects damage**

A verifier that cannot fail is worthless, so prove it fails on a corrupted copy:

```bash
mkdir -p /tmp/corpus_check && cp -r orig /tmp/corpus_check/
printf 'x' >> /tmp/corpus_check/orig/PLACES.SAV
python3 - <<'PY'
import pathlib, subprocess, sys, shutil, tempfile
root = pathlib.Path('.').resolve()
tmp = pathlib.Path(tempfile.mkdtemp())
shutil.copytree(root / 'tools', tmp / 'tools')
shutil.copytree(pathlib.Path('/tmp/corpus_check/orig'), tmp / 'orig')
r = subprocess.run([sys.executable, str(tmp / 'tools' / 'verify_corpus.py')],
                   capture_output=True, text=True)
print(r.stdout.strip())
assert r.returncode == 1, f"verifier passed on corrupted corpus (rc={r.returncode})"
assert 'PLACES.SAV' in r.stdout, "verifier did not name the corrupted file"
print('OK verifier correctly rejects a corrupted corpus')
PY
rm -rf /tmp/corpus_check
```

Expected: `FAIL PLACES.SAV: size 8 != 7` followed by `OK verifier correctly rejects a corrupted corpus`.

- [ ] **Step 5: Write `.gitignore`**

```
/target
/ghidra_proj
/build
*.log
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "chore: init gopnik-rs with verified reference corpus"
```

---

### Task 2: Pascal shortstring extractor

**Files:**
- Create: `tools/extract_strings.py`
- Create: `data/strings.json`
- Test: `tools/test_extract_strings.py`

**Interfaces:**
- Consumes: `orig/g.exe`.
- Produces: `data/strings.json` — a JSON array of objects
  `{"off": <int file offset>, "text": <UTF-8 string>, "plain": <UTF-8 string>,
  "suspect": <bool>}`, sorted ascending by `off`. `text` is the raw string
  including `^N` markup; `plain` is the same string with markup removed.
  Consumers that match or display content use `plain`; only the renderer uses
  `text`. Tasks 10 and 11 read this file.

**On `suspect`.** The length-prefix scan is a heuristic, so a small number of
machine-code byte sequences satisfy it and appear as entries — e.g.
`'к8бЮ8Щ'`, `'D6гN6г'`, `'X9ыUЛ>'`. These are flagged, never deleted: removing
them would change the entry count and destabilise offsets that other tasks
reference. Consumers that iterate the table for display MUST filter on
`suspect == false`; consumers that select by known offset may ignore it.

The rule is: `suspect = (longest run of consecutive Cyrillic letters < 3) and
(no space character in plain)`. Measured against this binary it flags 39 of
696 entries, of which 37 are genuine noise. It has exactly two known false
positives, both real game text: `0x2F87` `'Сл:^'` and `0x92D1` `'Ну..'`. The
flag is a hint for filtering, not a correctness claim — do not "fix" those two
by special-casing them.

- [ ] **Step 1: Write the failing test**

Create `tools/test_extract_strings.py`:

```python
#!/usr/bin/env python3
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_extraction():
    subprocess.run([sys.executable, str(ROOT / "tools" / "extract_strings.py")], check=True)
    items = json.loads((ROOT / "data" / "strings.json").read_text(encoding="utf-8"))

    assert len(items) == 696, f"expected 696 strings, got {len(items)}"

    offs = [i["off"] for i in items]
    assert offs == sorted(offs), "strings must be sorted by offset"
    assert len(set(offs)) == len(offs), "offsets must be unique"

    by_off = {i["off"]: i["text"] for i in items}
    assert by_off[0x2B44] == "Не в этой жизни."
    assert by_off[0x2FB2] == '^1Крестик(Удача +2) '
    assert by_off[0x3173] == "^1Тесак(Урон+9) "
    assert by_off[0x4548] == "^4Пацан ты из какого района?"

    joined = "\n".join(i["text"] for i in items)
    assert "Кольцо \"Гп\"(Самолечение)" in joined
    assert "Костюм Adidas(+2)" in joined

    for i in items:
        assert "\x00" not in i["text"]

    # ^N is markup, not content: it must survive in `text` and be absent
    # from `plain`, and stripping must not disturb anything else.
    plain = {i["off"]: i["plain"] for i in items}
    assert plain[0x2B44] == "Не в этой жизни.", "plain equals text when no markup"
    assert plain[0x3173] == "Тесак(Урон+9) "
    assert plain[0x4548] == "Пацан ты из какого района?"

    markup = re.compile(r"\^[0-7]")
    for i in items:
        assert not markup.search(i["plain"]), (
            f"markup survived stripping at {i['off']:#x}: {i['plain']!r}"
        )
    assert any(markup.search(i["text"]) for i in items), (
        "no markup found in any raw text -- the extractor or the test is wrong"
    )

    # `suspect` flags probable machine-code noise. Entries are flagged, never
    # dropped, so the total stays 696 and offsets stay stable.
    suspects = [i for i in items if i["suspect"]]
    assert len(suspects) == 39, f"expected 39 suspect entries, got {len(suspects)}"

    suspect_offs = {i["off"] for i in suspects}
    for off in (0x285E, 0x3F50, 0x654D, 0x11075, 0x11C34):
        assert off in suspect_offs, f"known-noise entry {off:#x} not flagged"
    for off in (0x2B44, 0x3173, 0x4548, 0x2FB2):
        assert off not in suspect_offs, f"real game text {off:#x} wrongly flagged"

    # Two known false positives -- documented, deliberately not special-cased.
    assert 0x2F87 in suspect_offs and 0x92D1 in suspect_offs, (
        "the two known false positives changed; re-check the heuristic"
    )

    print(f"OK {len(items)} strings extracted, {len(suspects)} flagged suspect")


if __name__ == "__main__":
    test_extraction()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python3 tools/test_extract_strings.py`
Expected: FAIL — `FileNotFoundError` / `CalledProcessError`, because `tools/extract_strings.py` does not exist yet.

- [ ] **Step 3: Write the extractor**

Create `tools/extract_strings.py`:

```python
#!/usr/bin/env python3
"""Extract Borland Pascal shortstrings (length-prefixed, CP866) from g.exe.

A shortstring is one length byte N followed by exactly N payload bytes.
We accept a candidate only when every payload byte is printable in CP866
and at least two of them are Cyrillic, which is what separates real game
text from machine code that happens to look string-shaped.
"""
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE = ROOT / "orig" / "g.exe"
OUT = ROOT / "data" / "strings.json"

MIN_LEN = 3
MAX_LEN = 200
MIN_CYRILLIC = 2


def is_cyrillic(b: int) -> bool:
    # CP866: 0x80-0xAF is А-п, 0xE0-0xF1 is р-я plus Ё/ё.
    return 0x80 <= b <= 0xAF or 0xE0 <= b <= 0xF1


def is_printable(b: int) -> bool:
    return 32 <= b < 127 or is_cyrillic(b) or b == 0xB0


MARKUP_RE = re.compile(r"\^[0-7]")
CYRILLIC_RE = re.compile(r"[А-Яа-яЁё]")


def strip_markup(s: str) -> str:
    """Remove the original's ^N colour directives, leaving displayable text."""
    return MARKUP_RE.sub("", s)


def longest_cyrillic_run(s: str) -> int:
    best = cur = 0
    for ch in s:
        if CYRILLIC_RE.match(ch):
            cur += 1
            best = max(best, cur)
        else:
            cur = 0
    return best


def is_suspect(plain: str) -> bool:
    """Heuristic flag for entries that are probably machine code, not text.

    Real game text either contains a space or has a run of three or more
    consecutive Cyrillic letters. Byte sequences that merely satisfy the
    length-prefix scan tend to alternate letters with digits and symbols.
    Flagged entries are kept, never deleted -- see the plan for why.
    """
    return longest_cyrillic_run(plain) < 3 and " " not in plain


def extract(blob: bytes) -> list[dict]:
    out = []
    i = 0
    end = len(blob)
    while i < end:
        n = blob[i]
        if MIN_LEN <= n <= MAX_LEN and i + 1 + n <= end:
            payload = blob[i + 1 : i + 1 + n]
            if all(is_printable(c) for c in payload) and sum(
                is_cyrillic(c) for c in payload
            ) >= MIN_CYRILLIC:
                text = payload.decode("cp866")
                plain = strip_markup(text)
                out.append(
                    {
                        "off": i,
                        "text": text,
                        "plain": plain,
                        "suspect": is_suspect(plain),
                    }
                )
                i += 1 + n
                continue
        i += 1
    return out


def main() -> None:
    items = extract(EXE.read_bytes())
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(
        json.dumps(items, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )
    print(f"wrote {len(items)} strings to {OUT}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `python3 tools/test_extract_strings.py`
Expected: `OK 696 strings extracted and validated`

- [ ] **Step 5: Commit**

```bash
git add tools/extract_strings.py tools/test_extract_strings.py data/strings.json
git commit -m "feat: extract 696 CP866 Pascal shortstrings from g.exe"
```

---

### Task 3: DOSBox-X oracle harness

This is the single most valuable asset in the project: it turns the original binary into a queryable ground-truth source. Build it before any formula RE, because every later task validates against it.

**Files:**
- Create: `tools/oracle/run_oracle.sh`
- Create: `tools/oracle/dosbox-oracle.conf`
- Create: `tools/oracle/capture.py`
- Test: `tools/oracle/test_oracle_smoke.py`

**Interfaces:**
- Produces: `run_oracle(keys: str, out_dir: Path) -> str` behaviour via CLI — given a keystroke script, returns captured screen text as UTF-8. Tasks 8, 9 and 12 consume this.

- [ ] **Step 1: Write the DOSBox-X config**

Create `tools/oracle/dosbox-oracle.conf`:

```ini
[sdl]
autolock=false
windowresolution=640x400
output=surface

[dosbox]
machine=vgaonly
captures=capture
memsize=16

[render]
aspect=false

[cpu]
core=normal
cputype=386
cycles=fixed 3000

[dos]
keyboardlayout=ru446
xms=true
ems=true

[autoexec]
mount c .
c:
```

Note: the original DOSBox fragment recovered from `gopnik.data` used `keyboardlayout=RU`. `ru446` is the DOSBox-X spelling that also loads CP866 as the display codepage, which is required for the Cyrillic text to render.

- [ ] **Step 2: Write the capture driver**

Create `tools/oracle/capture.py`:

```python
#!/usr/bin/env python3
"""Drive g.exe under DOSBox-X and capture VGA text-mode screen contents.

DOSBox-X's -c "..." lets us script startup, and its BOOT/DEBUG facilities
are heavyweight for our needs. Instead we let the game run, feed keystrokes
via the built-in autotype command, and dump text-mode video memory with a
screenshot of the text layer.

Usage:
    capture.py --keys "1\\n2\\ns\\n" --out run1/
"""
import argparse
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
ORIG = ROOT / "orig"
CONF = pathlib.Path(__file__).resolve().parent / "dosbox-oracle.conf"


def build_autotype(keys: str) -> list[str]:
    """Translate a keystroke script into DOSBox-X autotype commands.

    Each line of `keys` becomes one autotype invocation followed by Enter.
    autotype syntax: autotype -w <initial wait> -p <pace> <keys...>
    """
    cmds = []
    for line in keys.split("\n"):
        if not line:
            continue
        spaced = " ".join(list(line))
        cmds.append(f"autotype -w 0.6 -p 0.08 {spaced} enter")
    return cmds


def run(keys: str, out_dir: pathlib.Path, timeout: int = 120) -> pathlib.Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    work = out_dir / "work"
    if work.exists():
        shutil.rmtree(work)
    work.mkdir()
    for f in ORIG.iterdir():
        shutil.copy2(f, work / f.name)
        (work / f.name).chmod(0o644)

    cmd = [
        "dosbox-x",
        "-conf", str(CONF),
        "-fastlaunch",
        "-c", "g.exe",
    ]
    for c in build_autotype(keys):
        cmd += ["-c", c]
    cmd += ["-c", "screenshot", "-c", "exit"]

    env = {"SDL_VIDEODRIVER": "dummy", "HOME": str(work)}
    subprocess.run(
        cmd, cwd=work, timeout=timeout, env=env,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    return work


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--keys", required=True)
    ap.add_argument("--out", required=True, type=pathlib.Path)
    args = ap.parse_args()
    work = run(args.keys.replace("\\n", "\n"), args.out)
    print(f"oracle run complete: {work}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 3: Write the smoke test**

Create `tools/oracle/test_oracle_smoke.py`:

```python
#!/usr/bin/env python3
"""The oracle must at minimum boot the game and leave its save files intact."""
import pathlib
import shutil
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import capture  # noqa: E402

OUT = pathlib.Path("/tmp/gopnik_oracle_smoke")


def test_boot():
    if OUT.exists():
        shutil.rmtree(OUT)
    work = capture.run(keys="", out_dir=OUT, timeout=90)
    assert (work / "g.exe").exists(), "g.exe missing from oracle workdir"
    assert (work / "SAVE_R0.SAV").stat().st_size == 694
    print("OK oracle boots and workdir is intact")


if __name__ == "__main__":
    test_boot()
```

- [ ] **Step 4: Run the smoke test**

Run: `python3 tools/oracle/test_oracle_smoke.py`
Expected: `OK oracle boots and workdir is intact`

If DOSBox-X exits nonzero or hangs, the most likely cause is `SDL_VIDEODRIVER=dummy` being unsupported in this SDL1 build. Fallback: drop the env override and add `-nogui`, or run under `xvfb-run -a`. Record whichever works in `docs/re/oracle.md` — later tasks depend on it.

- [ ] **Step 5: Document the oracle**

Create `docs/re/oracle.md` recording: the exact working invocation, the fallback used (if any), measured boot time, and how to read captured output. One short page.

- [ ] **Step 6: Commit**

```bash
git add tools/oracle docs/re/oracle.md
git commit -m "feat: DOSBox-X oracle harness for ground-truth capture"
```

---

### Task 4: Ghidra headless export pipeline

**Files:**
- Create: `tools/ghidra/ExportAll.java`
- Create: `tools/ghidra/run_ghidra.sh`
- Create: `build/decomp/` (generated, gitignored)
- Create: `data/functions.json`

**Interfaces:**
- Produces: `data/functions.json` — array of `{"name", "entry", "size", "called_by": [...], "calls": [...]}`; and `build/decomp/<name>.c` per function. Tasks 8, 9, 10 read these.

- [ ] **Step 1: Write the Ghidra export script**

Create `tools/ghidra/ExportAll.java`:

```java
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceManager;

import java.io.File;
import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.List;

public class ExportAll extends GhidraScript {

    private String safe(String s) {
        return s.replaceAll("[^A-Za-z0-9_.-]", "_");
    }

    @Override
    public void run() throws Exception {
        String outDir = getScriptArgs().length > 0 ? getScriptArgs()[0] : "build";
        File decompDir = new File(outDir, "decomp");
        decompDir.mkdirs();

        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);

        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();

        List<String> json = new ArrayList<>();

        for (Function f : fm.getFunctions(true)) {
            String name = f.getName();
            Address entry = f.getEntryPoint();

            DecompileResults res = di.decompileFunction(f, 60, monitor);
            String c = res.decompileCompleted()
                    ? res.getDecompiledFunction().getC()
                    : "// DECOMPILATION FAILED: " + res.getErrorMessage() + "\n";

            File out = new File(decompDir, safe(name) + "_" + entry.toString().replace(":", "_") + ".c");
            try (PrintWriter pw = new PrintWriter(out, "UTF-8")) {
                pw.print(c);
            }

            List<String> callers = new ArrayList<>();
            for (Reference r : rm.getReferencesTo(entry)) {
                Function cf = fm.getFunctionContaining(r.getFromAddress());
                if (cf != null && !callers.contains(cf.getName())) callers.add(cf.getName());
            }

            List<String> callees = new ArrayList<>();
            for (Function cf : f.getCalledFunctions(monitor)) {
                if (!callees.contains(cf.getName())) callees.add(cf.getName());
            }

            StringBuilder sb = new StringBuilder();
            sb.append("  {\"name\": \"").append(name).append("\"");
            sb.append(", \"entry\": \"").append(entry).append("\"");
            sb.append(", \"size\": ").append(f.getBody().getNumAddresses());
            sb.append(", \"called_by\": [").append(quoteJoin(callers)).append("]");
            sb.append(", \"calls\": [").append(quoteJoin(callees)).append("]}");
            json.add(sb.toString());
        }

        File jf = new File(outDir, "functions.json");
        try (PrintWriter pw = new PrintWriter(jf, "UTF-8")) {
            pw.println("[");
            pw.println(String.join(",\n", json));
            pw.println("]");
        }

        println("EXPORTED functions=" + json.size() + " to " + outDir);
        di.dispose();
    }

    private String quoteJoin(List<String> xs) {
        List<String> q = new ArrayList<>();
        for (String x : xs) q.add("\"" + x + "\"");
        return String.join(", ", q);
    }
}
```

- [ ] **Step 2: Write the runner**

Create `tools/ghidra/run_ghidra.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GHIDRA=/opt/ghidra/support/analyzeHeadless
PROJ="$ROOT/ghidra_proj"
OUT="$ROOT/build"

mkdir -p "$PROJ" "$OUT"

if [ ! -d "$PROJ/gopnik.rep" ]; then
  "$GHIDRA" "$PROJ" gopnik -import "$ROOT/orig/g.exe" -analysisTimeoutPerFile 600
fi

"$GHIDRA" "$PROJ" gopnik -process g.exe -noanalysis \
  -scriptPath "$ROOT/tools/ghidra" \
  -postScript ExportAll.java "$OUT"

cp "$OUT/functions.json" "$ROOT/data/functions.json"
echo "decomp files: $(ls "$OUT/decomp" | wc -l)"
```

- [ ] **Step 3: Run it**

```bash
chmod +x tools/ghidra/run_ghidra.sh
./tools/ghidra/run_ghidra.sh
```

Expected: `EXPORTED functions=123 to .../build`, and `decomp files: 123`.

- [ ] **Step 4: Verify the export is usable**

Run:
```bash
python3 -c "
import json,pathlib
fs=json.loads(pathlib.Path('data/functions.json').read_text())
print('functions', len(fs))
big=sorted(fs,key=lambda f:-f['size'])[:10]
for f in big: print(f['size'], f['entry'], f['name'])
"
```
Expected: 123 functions listed; the ten largest printed. The largest non-RTL functions are the game's main loop and combat routine — note their entry addresses in `docs/re/functions.md`, they are the starting point for Tasks 8 and 9.

- [ ] **Step 5: Commit**

```bash
git add tools/ghidra data/functions.json docs/re/functions.md
git commit -m "feat: Ghidra headless decompilation export pipeline"
```

---

### Task 4b: Recover string pointers from code immediates

**Why this task exists.** Task 2's length-prefix scan is structurally ambiguous:
an ASCII space (`0x20`) is indistinguishable from a length byte of 32, so the
scanner resynchronises mid-string and emits plausible-looking fragments.
Measured against this binary, roughly 60–90 of its 657 non-suspect entries are
misframed — e.g. offset `0xBCF8` yields `'боксёров(-75% что сломают челюст'`,
truncated before the closing `ь)`.

The fix is to stop guessing where strings start. Borland Pascal passes a string
constant's address to the RTL as a 16-bit immediate, so the true starts are
exactly the immediate operands the code actually uses as pointers. A naive byte
scan for opcode `BA`/`B8`/`BF`/`BE`/`68` is NOT sufficient — it produced 218
false starts. You must work from Ghidra's real disassembly so that only genuine
instruction operands are considered.

**On the run of offsets at `0x18D0`–`0x18DA`:** this plan has now been wrong
about these twice; here is the established truth. They are **artefacts of
operand mis-extraction**, but not of byte scanning. They arise when a memory
operand's *displacement* is treated as a string offset — e.g. the audit trail
records `LDS SI,[BP + 0x4]` yielding candidate `0x18D0 + 4 = 0x18D4`. A stack
frame displacement is not a string address. The small displacements 0, 2, 4, 6…
collide with the image base and produce that consecutive run.

The correct handling is neither to assert they are absent (revision 1's error)
nor to declare them legitimate scrolling references (revision 2's error), but to
extract only genuine immediate operands so they never become candidates.

**Operand extraction rule.** Use `instruction.getScalar(opIndex)`, which yields
the scalar of an *immediate* operand. Do NOT walk `instruction.getOpObjects()`
and treat every scalar found inside a memory expression as a candidate — that
decomposes `[BP + 0x4]`, `word ptr [0x38C5]`, and branch targets into false
candidates. Apply `getScalar` across **all** mnemonics: the breadth requirement
is about not restricting to `MOV`/`PUSH`, not about accepting address arithmetic.

**Reject candidates that fall inside an already-accepted string's payload.**
An offset interior to another string is a framing collision, not a distinct
string — this is the same space-as-length-byte pathology the task exists to
eliminate. For example `0x5195` is byte 8 of `'^1После этого сразу началась
анархия и полный беспредел.'` at `0x518D`, and must not be emitted as its own
pointer.

**Do not add a reuse-count or reference-frequency filter.** How many times the
code references a string is not evidence about whether it is a string. Common
UI messages like `'Не хватает'` are printed from many call sites precisely
because they are generic. An earlier attempt used a reuse threshold and
discarded real game text (`0xA71D` `'Не хватает'`, `0xB00C` `'Продать вещи'`).

**Extraction breadth:** do not restrict to `MOV`/`PUSH`. String addresses also
reach the RTL via `LES`/`LDS` far-pointer loads and other forms. Consider every
instruction's scalar operands, and let the content filter and the coverage
assertion decide what qualifies.

**Verified viability (do not re-derive):** the immediate-operand approach
recovers `0xBCDD` → `'30^7  купить зубную защиту боксёров(-75% что сломают
челюсть)'`, complete and correctly terminated, and 558 of its candidate starts
agree with Task 2's scan.

**Files:**
- Create: `tools/ghidra/DumpImmediates.java`
- Create: `data/string_pointers.json`
- Create: `docs/re/string-pointers.md`
- Test: `tools/test_string_pointers.py`

**Interfaces:**
- Consumes: the Ghidra project created in Task 4.
- Produces: `data/string_pointers.json` — `{"note": str, "pointers": [int]}`,
  a sorted, deduplicated list of **file offsets** that code uses as string
  constant addresses. Task 2b consumes this.

- [ ] **Step 1: Write the Ghidra script**

`tools/ghidra/DumpImmediates.java` iterates `currentProgram.getListing().getInstructions(true)`.
For each instruction, for each operand, take scalar values via
`instr.getScalar(opIndex)`. Keep 16-bit scalars in the range `0x0000..0xFFFF`.
For each, compute the candidate file offset and write it out with the
instruction's address and mnemonic so the artifact is auditable.

The address mapping, already established: the program image begins at file
offset `0x18D0` and loads at segment `0x1000`, so for a scalar `imm` used as a
DS/CS-relative offset within the first code block, `file_offset = 0x18D0 + imm`.
Record the mapping you use in `docs/re/string-pointers.md`; if a scalar's
segment differs, derive its base from the containing memory block rather than
assuming `0x1000`.

- [ ] **Step 2: Filter to genuine string starts**

A candidate offset qualifies as a string pointer when `blob[off]` is a length
`N` in `3..=250`, `off + 1 + N <= len(blob)`, and every payload byte is either
printable ASCII (`0x20..0x7E`), CP866 high-range (`0x80..0xF1`), or one of the
separators `0x07`, `0x0A`, `0x0D`. `0x07` is the original's line separator
inside multi-line menu strings — it is legitimate content, not noise.

- [ ] **Step 3: Write the failing test**

Create `tools/test_string_pointers.py`:

```python
#!/usr/bin/env python3
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_pointers():
    data = json.loads((ROOT / "data" / "string_pointers.json").read_text(encoding="utf-8"))
    ptrs = data["pointers"]

    assert ptrs == sorted(ptrs), "pointers must be sorted"
    assert len(set(ptrs)) == len(ptrs), "pointers must be unique"
    assert len(ptrs) >= 400, f"expected >=400 string pointers, got {len(ptrs)}"

    blob = (ROOT / "orig" / "g.exe").read_bytes()

    # Every pointer must land on a well-formed length-prefixed string.
    for off in ptrs:
        n = blob[off]
        assert 3 <= n <= 250, f"{off:#x}: implausible length {n}"
        assert off + 1 + n <= len(blob), f"{off:#x}: payload runs past EOF"

    # The known-truncated case from Task 2 must now resolve completely.
    assert 0xBCDD in ptrs, "0xBCDD (the боксёров line) not recovered"
    n = blob[0xBCDD]
    text = blob[0xBCDE : 0xBCDE + n].decode("cp866")
    assert text.endswith("челюсть)"), f"still truncated: {text!r}"

    # Coverage must not regress against the blind scan. Every non-suspect
    # entry the old scanner found must either appear as a pointer or fall
    # inside some pointer's payload span (i.e. be superseded by a correctly
    # framed, longer string). Anything else is real game text we lost.
    # Elements of the indexed string-array tables are reached by index
    # arithmetic (base + i*256), so no literal pointer to them exists and
    # this task structurally cannot recover them. Task 4c handles those;
    # exclude their ranges here rather than counting them as losses.
    TABLE_RANGES = ((0x123DE, 0x12DDE), (0x12EF2, 0x158F2))

    def in_table(off):
        return any(lo <= off <= hi and (off - lo) % 256 == 0 for lo, hi in TABLE_RANGES)

    old = json.loads((ROOT / "data" / "strings.json").read_text(encoding="utf-8"))
    ptr_set = set(ptrs)
    missing = []
    for entry in old:
        if entry["suspect"] or in_table(entry["off"]):
            continue
        off = entry["off"]
        if off in ptr_set:
            continue
        if any(q <= off < q + 1 + blob[q] for q in ptrs):
            continue
        missing.append(entry)
    # 14 is the measured residual, not an aspiration. Every one of them must
    # be listed individually in docs/re/string-pointers.md with a reason.
    # Lowering this number is good; raising it requires re-measuring and
    # documenting the new survivors, never silently widening the bound.
    assert len(missing) <= 14, (
        f"{len(missing)} real strings lost vs the blind scan, e.g. "
        f"{[(hex(m['off']), m['plain'][:40]) for m in missing[:5]]}"
    )

    print(f"OK {len(ptrs)} string pointers recovered, {len(missing)} blind-scan entries unaccounted for")


if __name__ == "__main__":
    test_pointers()
```

- [ ] **Step 4: Run it to verify it fails**

Run: `python3 tools/test_string_pointers.py`
Expected: FAIL — `data/string_pointers.json` does not exist yet.

- [ ] **Step 5: Implement, run the Ghidra script, and regenerate**

Run the script through `tools/ghidra/run_ghidra.sh` (extend it to invoke
`DumpImmediates.java` as a second `-postScript`), then re-run the test.
Expected: `OK <n> string pointers recovered and validated`

- [ ] **Step 6: Document**

`docs/re/string-pointers.md`: the address mapping used, the opcode/operand
extraction method, the count recovered, and an explicit list of any candidate
offsets rejected by the filter and why.

- [ ] **Step 7: Commit**

```bash
git add tools/ghidra/DumpImmediates.java tools/ghidra/run_ghidra.sh data/string_pointers.json tools/test_string_pointers.py docs/re/string-pointers.md
git commit -m "feat: recover string constant pointers from code immediates"
```

---

### Task 4c: Recover indexed string array tables

**Why this task exists.** Task 4b recovers strings the code addresses by literal
pointer. It cannot recover strings the code reaches by *index arithmetic* —
Pascal `array[..] of string[255]` elements, addressed as `base + i * 256`. No
literal offset for element `i` exists anywhere in the binary, so pointer
recovery structurally misses them. This accounts for 55 of the 67 entries Task
4b leaves unaccounted for.

Two such tables exist. Both are verified; the entry text below is ground truth,
not a sample to be re-derived:

| Table | Base | Stride | Entries | First | Last |
|---|---|---|---|---|---|
| ranks/classes | `0x123DE` | 256 | 11 | `Дохляк` | `Ректор НГУ` |
| крутизна ladder | `0x12EF2` | 256 | 43 | `Опущеный` | `Пацан, который всех опрокинул` |

Table A is the class/enemy ladder: `Дохляк, Нефор, Нарк, Подтсан, Отморозок,
Гопник, Вор, Беспредельщик, Мент, Маньячок, Ректор НГУ`. The last is the final
boss named in the README. Table B is the 43-step крутизна ladder the README
describes as the "иерархическая лестница уровней крутизны". Tasks 9b and 10
both depend on these.

**Files:**
- Create: `tools/extract_tables_indexed.py`
- Create: `data/string_tables.json`
- Create: `docs/re/string-tables.md`
- Test: `tools/test_string_tables.py`

**Interfaces:**
- Produces: `data/string_tables.json` —
  `{"tables": [{"name": str, "base": int, "stride": int, "entries": [{"index": int, "off": int, "text": str, "plain": str}]}]}`.
  Task 2b merges these into `data/strings.json`; Tasks 9b and 10 read them by name.
  Use names `"ranks"` and `"krutizna"`.

- [ ] **Step 1: Write the failing test**

Create `tools/test_string_tables.py`:

```python
#!/usr/bin/env python3
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_tables():
    subprocess.run(
        [sys.executable, str(ROOT / "tools" / "extract_tables_indexed.py")], check=True
    )
    data = json.loads((ROOT / "data" / "string_tables.json").read_text(encoding="utf-8"))
    tables = {t["name"]: t for t in data["tables"]}

    assert set(tables) == {"ranks", "krutizna"}, f"unexpected tables: {sorted(tables)}"

    ranks = tables["ranks"]
    assert ranks["base"] == 0x123DE
    assert ranks["stride"] == 256
    assert len(ranks["entries"]) == 11
    assert [e["plain"] for e in ranks["entries"]] == [
        "Дохляк", "Нефор", "Нарк", "Подтсан", "Отморозок", "Гопник",
        "Вор", "Беспредельщик", "Мент", "Маньячок", "Ректор НГУ",
    ]

    kr = tables["krutizna"]
    assert kr["base"] == 0x12EF2
    assert kr["stride"] == 256
    assert len(kr["entries"]) == 43
    assert kr["entries"][0]["plain"] == "Опущеный"
    assert kr["entries"][21]["plain"] == "Пацан"
    assert kr["entries"][42]["plain"] == "Пацан, который всех опрокинул"

    # Offsets must follow the stride exactly.
    for t in data["tables"]:
        for i, e in enumerate(t["entries"]):
            assert e["off"] == t["base"] + i * t["stride"], (
                f"{t['name']}[{i}] off {e['off']:#x} breaks stride"
            )
            assert e["index"] == i

    print(f"OK {sum(len(t['entries']) for t in data['tables'])} table entries extracted")


if __name__ == "__main__":
    test_tables()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python3 tools/test_string_tables.py`
Expected: FAIL — `extract_tables_indexed.py` does not exist.

- [ ] **Step 3: Implement the extractor**

Read each table by walking `base + i * stride`, reading the length byte and
payload, decoding CP866, and stopping when the entry is no longer a well-formed
string (length outside `1..200`, or a payload byte outside printable
ASCII/CP866). Compute `plain` with the same markup-stripping used elsewhere.
Do NOT hardcode the entry counts — they must fall out of the walk, and the test
asserts the resulting counts are 11 and 43.

- [ ] **Step 4: Run the test to verify it passes**

Run: `python3 tools/test_string_tables.py`
Expected: `OK 54 table entries extracted`

- [ ] **Step 5: Document**

`docs/re/string-tables.md`: both tables with base, stride, count, and full entry
lists; how they were located (256-byte stride clustering among the offsets Task
4b could not account for); and a note that they are reached by index arithmetic,
which is why pointer recovery cannot find them.

- [ ] **Step 6: Commit**

```bash
git add tools/extract_tables_indexed.py tools/test_string_tables.py data/string_tables.json docs/re/string-tables.md
git commit -m "feat: recover indexed string array tables (ranks, krutizna ladder)"
```

---

### Task 2b: Re-extract strings anchored on recovered pointers

**Files:**
- Modify: `tools/extract_strings.py`
- Modify: `tools/test_extract_strings.py`
- Regenerate: `data/strings.json`
- Create: `docs/re/strings.md`

**Interfaces:**
- Consumes: `data/string_pointers.json` from Task 4b.
- Produces: `data/strings.json`, same record shape as Task 2
  (`{"off", "text", "plain", "suspect"}`), but with `off` values taken from the
  recovered pointer list rather than from a blind scan.

- [ ] **Step 1: Change the extraction source**

Replace the blind forward scan with: for each offset in
`data/string_pointers.json`, read the length byte and payload, decode CP866,
compute `plain`, and compute `suspect` with the existing rule. Keep the
`suspect` field — it still guards against a pointer that happens to address
non-text.

Retain the old scanner in the file as `scan_blind()` **only if** Task 4b's
pointer list turns out to miss strings the scan found; if it is unused, delete
it rather than keeping dead code.

- [ ] **Step 2: Update the test**

The count and suspect assertions from Task 2 are now wrong — they described the
blind scan. Replace them with:

```python
    # Anchored extraction must fix the framing bugs the blind scan produced.
    by_off = {i["off"]: i for i in items}

    assert 0xBCDD in by_off, "the боксёров line was not extracted"
    assert by_off[0xBCDD]["plain"].endswith("челюсть)"), (
        f"still truncated: {by_off[0xBCDD]['plain']!r}"
    )

    # Framing is checked structurally, by tiling. The string region is packed
    # with no delimiter, so a truncated string strands its tail in the gap
    # before the next string's start, and an over-long one runs into it.
    blob = (ROOT / "orig" / "g.exe").read_bytes()

    def alnum(c):
        return (0x80 <= c <= 0xAF or 0xE0 <= c <= 0xF1
                or 48 <= c <= 57 or 65 <= c <= 90 or 97 <= c <= 122)

    offs = sorted(by_off)
    for a, b in zip(offs, offs[1:]):
        end = a + 1 + blob[a]
        assert end <= b, f"0x{a:X} (len {blob[a]}) overlaps next string 0x{b:X}"
        if b - end < 40:
            tail = blob[end:b]
            assert not any(alnum(c) for c in tail), (
                f"letter bytes stranded after 0x{a:X}: {tail!r}"
            )
```

**Do NOT use a next-byte heuristic here.** An earlier revision of this plan
asserted a string was cut when its last payload byte and the byte after it
were both alphanumeric. That check is structurally broken, and was measured
producing 39 false positives on correct data: strings are packed back-to-back,
so the byte after any string is the *next string's length byte*, and ordinary
lengths (48–57, 65–90, 97–122) all land inside the alphanumeric ranges. A
same-alphabet-class variant still produced 3 false positives from the same
cause. No next-byte rule can distinguish a cut from a length byte. The tiling
check above replaces it; it measured 0 overlaps and 633 exact abutments across
749 entries.

Record the actual measured totals in `docs/re/strings.md` rather than
hardcoding a guessed count into the test.

- [ ] **Step 3: Run, compare, and document**

Run `python3 tools/test_extract_strings.py`. In `docs/re/strings.md` record:
the new entry count, how many entries the blind scan had that the anchored
extraction dropped (and spot-check a sample of them), how many are new, and the
tiling result — overlap count, exact-abutment count, gap count — identifying
the large gaps as inter-region rather than stranded text.

- [ ] **Step 4: Commit**

```bash
git add tools/extract_strings.py tools/test_extract_strings.py data/strings.json docs/re/strings.md
git commit -m "fix: anchor string extraction on recovered pointers, fixing truncation"
```

---

### Task 2c: Recover the short strings the pointer scan missed

**Why this task exists:** Task 2b's tiling check did its job and found a real
coverage gap. 39 gaps between anchored strings contain letter bytes, and 37 of
them tile exactly as complete Pascal shortstrings. They are the game's
single-character command tokens — `s`, `sv`, `e`, `v`, `f`, `k`, `y`, `\`,
`1`–`4` — plus a `С^ У^ П^ Е^` banner split across four strings. Task 4b's
pointer scan skipped them because of its `N>=3` Cyrillic-run floor, and Task 4c
only covered indexed arrays. Task 11 compares user input against these tokens,
so leaving them out would break the game loop.

**Files:**
- Modify: `tools/extract_strings.py`
- Modify: `tools/test_extract_strings.py`
- Regenerate: `data/strings.json`
- Modify: `docs/re/strings.md`

**Interfaces:**
- Consumes: `data/strings.json` from Task 2b (same record shape).
- Produces: `data/strings.json` with the gap-tiled strings merged in, sorted by
  `off`. No shape change.

- [ ] **Step 1: Add gap-tiling recovery**

After the pointer-anchored and table entries are collected, walk consecutive
pairs of recovered offsets. For a gap between `a` and `b`:

- Skip if either `a` or `b` is `suspect`. A suspect entry is not a known-good
  anchor, so a gap beside one proves nothing. (Measured: the only 2 gaps that
  fail to tile sit between suspect entries at `0x1105D` and `0x11070`, and are
  code bytes, not text.)
- Skip if the gap is >= 40 bytes — those are inter-region spans, not stranded
  strings.
- Do **not** filter on byte content. Every gap meeting the conditions above is
  walked, whatever it holds — see below.
- Walk the gap as a chain of Pascal shortstrings: read a length byte, skip that
  many payload bytes, repeat. If the chain lands exactly on `b`, every element
  is a real string; emit each with the same `{"off","text","plain","suspect"}`
  shape. If it overruns `b`, emit nothing for that gap.

**Do not add a letter-byte condition.** An earlier revision of this plan
required the gap to contain a byte in `alnum()`'s ranges, on the grounds that
tiling alone was weak evidence — "~13% of random windows tile, flat across gap
lengths 2–40." **That measurement was a sampling artifact and the requirement
was wrong.** The sample spanned `0x18D0`–`0x158F2`, which includes the
`0x11000`+ tail; that tail is 69.0% NUL bytes, and a run of `0x00` is a chain
of zero-length strings that tiles at *any* length. The flatness across gap
lengths was the artifact announcing itself.

Measured per region, 20000 random windows each:

| region | NUL | 2 B | 3 B | 7 B | 20 B | 40 B |
|---|---|---|---|---|---|---|
| `0x18D0`–`0x11000` (holds all 40 recovered) | 2.1% | 1.7% | 0.6% | 1.1% | 0.1% | 0.2% |
| `0x11000`–`0x158F2` (tail) | 69.0% | 67.2% | 67.5% | 66.4% | 64.4% | 64.7% |
| union (the misleading sample) | 17.4% | 17.1% | 15.8% | 15.7% | 14.8% | 14.1% |

In the region where the recovered strings actually live, an arbitrary window
tiles exactly ~0.1–1.7% of the time. For a 2-byte gap that rate is just
`P(byte == 0x01)` = 1.64%. **Tiling between two verified anchors is strong
evidence, not a coin flip**, and it is equally strong for a one-character
string as for a longer one. A Pascal program emitting `write(' ')` produces
exactly such a length-1 literal, so single punctuation strings are expected
content, not noise.

The seven entries the letter-byte condition excluded — `'^'` (`0x2BAC`,
`0x30EF`), `'#'` (`0x2FA7`), `' '` (`0x712A`, `0xB1CA`), `':'` (`0x7179`),
`'.'` (`0x9E63`) — therefore stay in. Five are flagged `suspect` by the
existing heuristic, which is the correct place to express low confidence;
excluding them outright was not. Note also that dropping `0xB1CA` is what left
`0xB1CB` uncovered among Task 4b's residual offsets.

This rule guesses nothing: it accepts only bytes that tile exactly between two
independently-verified anchors. Do **not** relax it into a scan — an unanchored
forward scan is the original defect this whole sequence exists to fix.

- [ ] **Step 2: Skip suspect neighbours in the tiling check**

In `tools/test_extract_strings.py`, the gap half of the tiling check must skip
pairs where either neighbour is `suspect`. Keep the overlap assertion applying
to **all** pairs — an overlap is a framing error regardless of suspect status.

```python
    offs = sorted(by_off)
    for a, b in zip(offs, offs[1:]):
        end = a + 1 + blob[a]
        assert end <= b, f"0x{a:X} (len {blob[a]}) overlaps next string 0x{b:X}"
        if by_off[a]["suspect"] or by_off[b]["suspect"]:
            continue
        if b - end < 40:
            tail = blob[end:b]
            assert not any(alnum(c) for c in tail), (
                f"letter bytes stranded after 0x{a:X}: {tail!r}"
            )
```

- [ ] **Step 3: Assert the command tokens are present**

These are the entries this task exists to recover. Add:

```python
    assert by_off[0x4E71]["plain"] == "sv", "the sv command token is missing"
    assert by_off[0x4E6F]["plain"] == "s"
    assert by_off[0x3D87]["plain"] == "1"
    assert by_off[0x23A4]["plain"] == "С^"
```

- [ ] **Step 4: Run and document**

Run `python3 tools/test_extract_strings.py`. Expected: 796 entries total, 47
recovered by gap-tiling, 0 tiling violations. These numbers were measured before
this task was written — if yours differ, that is a finding to report, not a
number to adjust the code toward.

In `docs/re/strings.md`, record the recovery rule, the count, and the full list
of recovered offsets with their text. Note explicitly that these are input
tokens rather than display text, since Task 11 will need them.

Also record, as a known gap to be cited in Task 11's brief: the
suspect-neighbour skip correctly excludes five small gaps, three of which tile
as real tokens — `0x8D79 'y'` (after `'Ты хочешь сохраниться?'`), `0x9BF1 '\'`
and `'y'` (after `'^0Хочешь сохранить...'`), and `0x9D5E 'w'` (before `'run'`).
Their anchors are `suspect` only because `is_suspect()` flags pure-ASCII
keywords like `save_r0.sav` and `run`. The rule is right — a suspect neighbour
is not a known-good anchor — but the consequence is that the yes/no
confirmation token for the save and quit prompts is **not** in
`data/strings.json`. Task 11 must recover it from the disassembly rather than
assume it is present.

- [ ] **Step 5: Commit**

```bash
git add tools/extract_strings.py tools/test_extract_strings.py data/strings.json docs/re/strings.md
git commit -m "feat: recover short command-token strings by gap tiling"
```

---

### Task 5: Save format decoder validated against all five real saves

**Files:**
- Create: `tools/decode_save.py`
- Create: `docs/re/save-format.md`
- Create: `data/save_layout.json`
- Test: `tools/test_decode_save.py`

**Interfaces:**
- Produces: `data/save_layout.json` — `{"size": 694, "fields": [{"name", "off", "kind", "len"}]}` where `kind` is one of `"pstring"`, `"u16"`, `"u8"`, `"bytes"`. Task 7's Rust `save.rs` is generated against this exact schema.

- [ ] **Step 1: Write the failing test**

Create `tools/test_decode_save.py`:

```python
#!/usr/bin/env python3
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from decode_save import decode, encode  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
ORIG = ROOT / "orig"

# Established by inspection of all five saves. hp/hpmax are the only
# semantically confirmed words at this stage; the rest stay unk_* until
# Task 9 pins them from the disassembly.
EXPECT = {
    "SAVE_R0.SAV": {"name": "^7 adg", "hp": 118, "hpmax": 129},
    "SAVE_R2.SAV": {"name": "^7 vor", "hp": 84, "hpmax": 99},
    "SAVE_R3.SAV": {"name": "^7 vor", "hp": 178, "hpmax": 178},
    "SAVE_R4.SAV": {"name": "^7 vor", "hp": 251, "hpmax": 270},
    "SAVE_R5.SAV": {"name": "^7 Mudila", "hp": 325, "hpmax": 325},
}

MAGIC = "^4Gopnik: ^7version 1.02 june,sept 2003"


def test_all():
    for fname, want in EXPECT.items():
        blob = (ORIG / fname).read_bytes()
        rec = decode(blob)

        assert rec["magic"] == MAGIC, f"{fname}: magic {rec['magic']!r}"
        assert rec["name"] == want["name"], f"{fname}: name {rec['name']!r}"
        assert rec["hp"] == want["hp"], f"{fname}: hp {rec['hp']}"
        assert rec["hpmax"] == want["hpmax"], f"{fname}: hpmax {rec['hpmax']}"
        assert rec["hp"] <= rec["hpmax"], f"{fname}: hp exceeds hpmax"

        # Round-trip must be byte-identical. This is the real assertion:
        # it proves we account for every one of the 694 bytes.
        assert encode(rec) == blob, f"{fname}: round-trip mismatch"

    print(f"OK {len(EXPECT)} saves decoded and round-tripped byte-identically")


if __name__ == "__main__":
    test_all()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python3 tools/test_decode_save.py`
Expected: FAIL — `ModuleNotFoundError: No module named 'decode_save'`.

- [ ] **Step 3: Write the decoder**

Create `tools/decode_save.py`:

```python
#!/usr/bin/env python3
"""Decode and re-encode GOPNIK .SAV files.

Layout (694 bytes total), established from the five reference saves:

    0x000  string[255]  magic  -- version banner, constant
    0x100  string[255]  name   -- player name, colour-prefixed
    0x200  u16 x 8             -- stat block, semantics TBD (Task 9)
    0x210  u16          hp
    0x212  u16          hpmax
    0x214  ...                 -- flags, counters, and a run of
                                  Pascal string[2] records; not yet
                                  segmented, preserved verbatim.

Everything past 0x214 is carried as opaque bytes so that round-trip is
exact. Task 9 replaces the opaque tail with named fields as they are
confirmed against the disassembly.
"""
import json
import pathlib
import sys

SIZE = 694
OFF_MAGIC = 0x000
OFF_NAME = 0x100
OFF_STATE = 0x200
OFF_HP = OFF_STATE + 0x10
OFF_HPMAX = OFF_STATE + 0x12
OFF_TAIL = OFF_STATE + 0x14

PSTRING_CAP = 255


def _get_pstring(blob: bytes, off: int) -> str:
    n = blob[off]
    return blob[off + 1 : off + 1 + n].decode("cp866")


def _put_pstring(buf: bytearray, off: int, s: str, original: bytes) -> None:
    """Write a shortstring, preserving the original padding bytes.

    Borland does not clear the tail of a shortstring buffer, so the bytes
    past the length are whatever was there before. To round-trip exactly we
    copy the original padding rather than zero-filling.
    """
    raw = s.encode("cp866")
    assert len(raw) <= PSTRING_CAP
    buf[off] = len(raw)
    buf[off + 1 : off + 1 + len(raw)] = raw
    buf[off + 1 + len(raw) : off + 1 + PSTRING_CAP] = original[
        off + 1 + len(raw) : off + 1 + PSTRING_CAP
    ]


def _u16(blob: bytes, off: int) -> int:
    return int.from_bytes(blob[off : off + 2], "little")


def decode(blob: bytes) -> dict:
    if len(blob) != SIZE:
        raise ValueError(f"expected {SIZE} bytes, got {len(blob)}")
    return {
        "magic": _get_pstring(blob, OFF_MAGIC),
        "name": _get_pstring(blob, OFF_NAME),
        "stats": [_u16(blob, OFF_STATE + 2 * i) for i in range(8)],
        "hp": _u16(blob, OFF_HP),
        "hpmax": _u16(blob, OFF_HPMAX),
        "tail": blob[OFF_TAIL:],
        "_raw": blob,
    }


def encode(rec: dict) -> bytes:
    original = rec["_raw"]
    buf = bytearray(original)
    _put_pstring(buf, OFF_MAGIC, rec["magic"], original)
    _put_pstring(buf, OFF_NAME, rec["name"], original)
    for i, v in enumerate(rec["stats"]):
        buf[OFF_STATE + 2 * i : OFF_STATE + 2 * i + 2] = int(v).to_bytes(2, "little")
    buf[OFF_HP : OFF_HP + 2] = int(rec["hp"]).to_bytes(2, "little")
    buf[OFF_HPMAX : OFF_HPMAX + 2] = int(rec["hpmax"]).to_bytes(2, "little")
    buf[OFF_TAIL:] = rec["tail"]
    return bytes(buf)


LAYOUT = {
    "size": SIZE,
    "fields": [
        {"name": "magic", "off": OFF_MAGIC, "kind": "pstring", "len": 256},
        {"name": "name", "off": OFF_NAME, "kind": "pstring", "len": 256},
        *[
            {"name": f"unk_stat{i}", "off": OFF_STATE + 2 * i, "kind": "u16", "len": 2}
            for i in range(8)
        ],
        {"name": "hp", "off": OFF_HP, "kind": "u16", "len": 2},
        {"name": "hpmax", "off": OFF_HPMAX, "kind": "u16", "len": 2},
        {"name": "tail", "off": OFF_TAIL, "kind": "bytes", "len": SIZE - OFF_TAIL},
    ],
}


def main() -> None:
    root = pathlib.Path(__file__).resolve().parent.parent
    (root / "data" / "save_layout.json").write_text(
        json.dumps(LAYOUT, indent=1) + "\n", encoding="utf-8"
    )
    for p in sorted((root / "orig").glob("SAVE_R*.SAV")):
        r = decode(p.read_bytes())
        print(f"{p.name}: name={r['name']!r} hp={r['hp']}/{r['hpmax']} stats={r['stats']}")


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `python3 tools/test_decode_save.py`
Expected: `OK 5 saves decoded and round-tripped byte-identically`

- [ ] **Step 5: Generate the layout artifact and notes**

Run: `python3 tools/decode_save.py`
Expected output includes `SAVE_R5.SAV: name='^7 Mudila' hp=325/325 stats=[5, 90, 120, 45, 49, 40, 57, 102]`

Write `docs/re/save-format.md` with the layout table above, the observed values for all five saves, and an explicit "unknown" list covering the eight stat words and the `string[2]` run in the tail.

- [ ] **Step 6: Commit**

```bash
git add tools/decode_save.py tools/test_decode_save.py data/save_layout.json docs/re/save-format.md
git commit -m "feat: byte-exact .SAV decoder validated on 5 reference saves"
```

---

### Task 6: Rust crate skeleton and text layer

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/text.rs`
- Test: `src/text.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `pub enum Color { Black, Blue, Green, Cyan, Red, Magenta, Brown, White }` with `pub fn from_code(c: char) -> Option<Color>`
  - `pub struct Span { pub color: Option<Color>, pub text: String }`
  - `pub fn parse(src: &str) -> Vec<Span>` — splits source text into styled
    spans. This is the primitive; `render` and `strip` are both defined in
    terms of it, so the markup is understood in exactly one place.
  - `pub fn render(src: &str) -> String` — spans to ANSI SGR, reset at end.
  - `pub fn strip(src: &str) -> String` — spans to plain text, markup removed.
    Use this for anything compared, measured, or stored as a name.
  - `pub fn fill(template: &str, values: &[i64]) -> String` — replaces each `#` in order with the next value; extra `#` beyond `values.len()` are left literal.

- [ ] **Step 1: Write `Cargo.toml`**

```toml
[package]
name = "gopnik"
version = "0.1.0"
edition = "2021"
rust-version = "1.97"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
panic = "abort"
```

- [ ] **Step 2: Write the failing test**

Create `src/text.rs` containing only the tests plus stub signatures:

```rust
//! Rendering of the original's inline `^N` colour codes and `#` placeholders.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Black,
    Blue,
    Green,
    Cyan,
    Red,
    Magenta,
    Brown,
    White,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub color: Option<Color>,
    pub text: String,
}

pub fn from_code(_c: char) -> Option<Color> {
    todo!()
}

pub fn parse(_src: &str) -> Vec<Span> {
    todo!()
}

pub fn render(_src: &str) -> String {
    todo!()
}

pub fn strip(_src: &str) -> String {
    todo!()
}

pub fn fill(_template: &str, _values: &[i64]) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_map_to_colors() {
        assert_eq!(from_code('0'), Some(Color::Black));
        assert_eq!(from_code('4'), Some(Color::Red));
        assert_eq!(from_code('7'), Some(Color::White));
        assert_eq!(from_code('9'), None);
    }

    #[test]
    fn parse_splits_into_styled_spans() {
        let spans = parse("^4Ты сдох.");
        assert_eq!(
            spans,
            vec![Span { color: Some(Color::Red), text: "Ты сдох.".to_string() }]
        );
    }

    #[test]
    fn parse_handles_leading_plain_text_and_multiple_colors() {
        let spans = parse("Зрители:^6Мочи его!");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0], Span { color: None, text: "Зрители:".to_string() });
        assert_eq!(spans[1].color, Some(Color::Brown));
        assert_eq!(spans[1].text, "Мочи его!");
    }

    #[test]
    fn strip_removes_markup_entirely() {
        assert_eq!(strip("^4Gopnik: ^7version 1.02"), "Gopnik: version 1.02");
        assert_eq!(strip("^7 Mudila"), " Mudila");
        assert_eq!(strip("Не в этой жизни."), "Не в этой жизни.");
    }

    #[test]
    fn strip_output_contains_no_sigils() {
        for s in ["^0a^1b^2c^3d^4e^5f^6g^7h", "^1Крестик(Удача +2) "] {
            assert!(!strip(s).contains('^'), "sigil survived in {s:?}");
        }
    }

    #[test]
    fn caret_not_followed_by_digit_is_literal() {
        assert_eq!(strip("2^3"), "2");
        assert_eq!(strip("a^zb"), "a^zb");
        assert_eq!(strip("trailing^"), "trailing^");
    }

    #[test]
    fn render_emits_ansi_and_resets() {
        let out = render("^4Ты сдох.");
        assert!(out.starts_with("\x1b[31m"), "got {out:?}");
        assert!(out.contains("Ты сдох."));
        assert!(out.ends_with("\x1b[0m"));
    }

    #[test]
    fn render_passes_through_plain_text() {
        assert_eq!(render("Не в этой жизни."), "Не в этой жизни.");
    }

    #[test]
    fn fill_substitutes_in_order() {
        assert_eq!(fill("Урон #-#", &[3, 7]), "Урон 3-7");
        assert_eq!(fill("Здоровье #/#  ", &[118, 129]), "Здоровье 118/129  ");
    }

    #[test]
    fn fill_leaves_surplus_placeholders_literal() {
        assert_eq!(fill("Сл:# Лв:# Жв:#", &[1, 2]), "Сл:1 Лв:2 Жв:#");
    }

    #[test]
    fn fill_handles_no_placeholders() {
        assert_eq!(fill("Пива нет", &[]), "Пива нет");
    }
}
```

Add to `src/main.rs`:

```rust
mod text;

fn main() {
    println!("{}", text::render("^4Gopnik: ^7version 1.02 june,sept 2003"));
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test`
Expected: FAIL — all eleven tests panic with `not yet implemented`.

- [ ] **Step 4: Implement the text layer**

Replace the three stubs in `src/text.rs`:

```rust
pub fn from_code(c: char) -> Option<Color> {
    Some(match c {
        '0' => Color::Black,
        '1' => Color::Blue,
        '2' => Color::Green,
        '3' => Color::Cyan,
        '4' => Color::Red,
        '5' => Color::Magenta,
        '6' => Color::Brown,
        '7' => Color::White,
        _ => return None,
    })
}

impl Color {
    fn sgr(self) -> &'static str {
        match self {
            Color::Black => "\x1b[30m",
            Color::Blue => "\x1b[34m",
            Color::Green => "\x1b[32m",
            Color::Cyan => "\x1b[36m",
            Color::Red => "\x1b[31m",
            Color::Magenta => "\x1b[35m",
            Color::Brown => "\x1b[33m",
            Color::White => "\x1b[37m",
        }
    }
}

/// Split source text into styled spans. This is the only place that
/// understands the `^N` markup; `render` and `strip` are both built on it.
pub fn parse(src: &str) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut color: Option<Color> = None;
    let mut buf = String::new();
    let mut chars = src.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '^' {
            if let Some(&next) = chars.peek() {
                if let Some(new_color) = from_code(next) {
                    chars.next();
                    if !buf.is_empty() {
                        spans.push(Span { color, text: std::mem::take(&mut buf) });
                    }
                    color = Some(new_color);
                    continue;
                }
            }
        }
        buf.push(c);
    }
    if !buf.is_empty() {
        spans.push(Span { color, text: buf });
    }
    spans
}

pub fn render(src: &str) -> String {
    let spans = parse(src);
    let mut out = String::with_capacity(src.len());
    let mut styled = false;
    for span in &spans {
        if let Some(c) = span.color {
            out.push_str(c.sgr());
            styled = true;
        }
        out.push_str(&span.text);
    }
    if styled {
        out.push_str("\x1b[0m");
    }
    out
}

pub fn strip(src: &str) -> String {
    parse(src).into_iter().map(|s| s.text).collect()
}

pub fn fill(template: &str, values: &[i64]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut it = values.iter();
    for c in template.chars() {
        if c == '#' {
            match it.next() {
                Some(v) => out.push_str(&v.to_string()),
                None => out.push('#'),
            }
        } else {
            out.push(c);
        }
    }
    out
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test`
Expected: `test result: ok. 11 passed; 0 failed`

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/text.rs
git commit -m "feat: rust crate skeleton with ^N colour and # placeholder rendering"
```

---

### Task 7: Rust save load/store with byte-exact round-trip

**Files:**
- Create: `src/save.rs`
- Modify: `src/main.rs` (add `mod save;`)
- Test: `tests/save_roundtrip.rs`

**Interfaces:**
- Consumes: `orig/SAVE_R*.SAV` (read directly by the integration test), `data/save_layout.json` (documentation only — the Rust struct mirrors it by hand).
- Produces:
  ```rust
  pub struct Save {
      pub magic: String,
      pub name: String,
      pub stats: [u16; 8],
      pub hp: u16,
      pub hpmax: u16,
      pub tail: Vec<u8>,
      raw: Vec<u8>,
  }
  impl Save {
      pub fn parse(bytes: &[u8]) -> Result<Save, SaveError>;
      pub fn to_bytes(&self) -> Vec<u8>;
      /// The player's name with `^N` markup removed -- e.g. the raw
      /// `"^7 Mudila"` displays and compares as `" Mudila"`. Use this
      /// anywhere a name is shown or matched; `self.name` keeps the raw
      /// bytes solely so round-trip stays byte-exact.
      pub fn display_name(&self) -> String;
  }
  pub enum SaveError { BadSize(usize), Encoding(u8) }
  ```
  Note: there is deliberately no `BadMagic` variant. The magic string is
  returned as data and asserted by the caller, so that a save written by a
  different build is still parseable and inspectable rather than rejected.
  Task 11's game loop calls `Save::parse` and `Save::to_bytes`.

- [ ] **Step 1: Write the failing integration test**

Create `tests/save_roundtrip.rs`:

```rust
use gopnik::save::Save;
use std::path::Path;

const MAGIC: &str = "^4Gopnik: ^7version 1.02 june,sept 2003";

fn load(name: &str) -> Vec<u8> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("orig").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

#[test]
fn all_reference_saves_round_trip_byte_exactly() {
    for name in [
        "SAVE_R0.SAV",
        "SAVE_R2.SAV",
        "SAVE_R3.SAV",
        "SAVE_R4.SAV",
        "SAVE_R5.SAV",
    ] {
        let bytes = load(name);
        assert_eq!(bytes.len(), 694, "{name}: unexpected size");

        let save = Save::parse(&bytes).unwrap_or_else(|e| panic!("{name}: parse: {e:?}"));
        assert_eq!(save.magic, MAGIC, "{name}: magic");
        assert!(save.hp <= save.hpmax, "{name}: hp {} > hpmax {}", save.hp, save.hpmax);

        let out = save.to_bytes();
        assert_eq!(out, bytes, "{name}: round-trip is not byte-exact");
    }
}

#[test]
fn known_values_match_reference_saves() {
    let cases = [
        ("SAVE_R0.SAV", "^7 adg", 118u16, 129u16),
        ("SAVE_R2.SAV", "^7 vor", 84, 99),
        ("SAVE_R3.SAV", "^7 vor", 178, 178),
        ("SAVE_R4.SAV", "^7 vor", 251, 270),
        ("SAVE_R5.SAV", "^7 Mudila", 325, 325),
    ];
    for (file, name, hp, hpmax) in cases {
        let save = Save::parse(&load(file)).unwrap();
        assert_eq!(save.name, name, "{file}: name");
        assert_eq!(save.hp, hp, "{file}: hp");
        assert_eq!(save.hpmax, hpmax, "{file}: hpmax");
    }
}

#[test]
fn rejects_wrong_size() {
    assert!(Save::parse(&[0u8; 10]).is_err());
}

#[test]
fn display_name_strips_markup_but_raw_name_keeps_it() {
    let save = Save::parse(&load("SAVE_R5.SAV")).unwrap();
    assert_eq!(save.name, "^7 Mudila", "raw name must keep markup for round-trip");
    assert_eq!(save.display_name(), " Mudila");
    assert!(!save.display_name().contains('^'));
}
```

Create `src/lib.rs` so integration tests can import the crate:

```rust
pub mod save;
pub mod text;
```

Change `src/main.rs` to use the library:

```rust
use gopnik::text;

fn main() {
    println!("{}", text::render("^4Gopnik: ^7version 1.02 june,sept 2003"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test save_roundtrip`
Expected: FAIL — `unresolved module 'save'` / compilation error.

- [ ] **Step 3: Implement `src/save.rs`**

```rust
//! GOPNIK .SAV parsing. 694 bytes, Borland Pascal record layout.
//!
//! Round-trip must be byte-exact, which constrains two things:
//!   * shortstring padding past the length byte is NOT cleared by Borland,
//!     so we retain the original bytes rather than zero-filling;
//!   * every byte past the known fields is preserved verbatim in `tail`.

use std::fmt;

pub const SIZE: usize = 694;
const OFF_MAGIC: usize = 0x000;
const OFF_NAME: usize = 0x100;
const OFF_STATE: usize = 0x200;
const OFF_HP: usize = OFF_STATE + 0x10;
const OFF_HPMAX: usize = OFF_STATE + 0x12;
const OFF_TAIL: usize = OFF_STATE + 0x14;
const PSTRING_CAP: usize = 255;

#[derive(Debug)]
pub enum SaveError {
    BadSize(usize),
    Encoding(u8),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::BadSize(n) => write!(f, "expected {SIZE} bytes, got {n}"),
            SaveError::Encoding(b) => write!(f, "byte {b:#04x} is not valid CP866"),
        }
    }
}

/// CP866 -> Unicode for the high half. Index 0 is 0x80.
const CP866_HIGH: [char; 128] = [
    'А', 'Б', 'В', 'Г', 'Д', 'Е', 'Ж', 'З', 'И', 'Й', 'К', 'Л', 'М', 'Н', 'О', 'П',
    'Р', 'С', 'Т', 'У', 'Ф', 'Х', 'Ц', 'Ч', 'Ш', 'Щ', 'Ъ', 'Ы', 'Ь', 'Э', 'Ю', 'Я',
    'а', 'б', 'в', 'г', 'д', 'е', 'ж', 'з', 'и', 'й', 'к', 'л', 'м', 'н', 'о', 'п',
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐',
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧',
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀',
    'р', 'с', 'т', 'у', 'ф', 'х', 'ц', 'ч', 'ш', 'щ', 'ъ', 'ы', 'ь', 'э', 'ю', 'я',
    'Ё', 'ё', 'Є', 'є', 'Ї', 'ї', 'Ў', 'ў', '°', '∙', '·', '√', '№', '¤', '■', '\u{a0}',
];

fn cp866_decode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b < 0x80 {
                b as char
            } else {
                CP866_HIGH[(b - 0x80) as usize]
            }
        })
        .collect()
}

fn cp866_encode(s: &str) -> Result<Vec<u8>, SaveError> {
    let mut out = Vec::with_capacity(s.len());
    for ch in s.chars() {
        if (ch as u32) < 0x80 {
            out.push(ch as u8);
        } else if let Some(i) = CP866_HIGH.iter().position(|&c| c == ch) {
            out.push(0x80 + i as u8);
        } else {
            return Err(SaveError::Encoding(0));
        }
    }
    Ok(out)
}

pub struct Save {
    pub magic: String,
    pub name: String,
    pub stats: [u16; 8],
    pub hp: u16,
    pub hpmax: u16,
    pub tail: Vec<u8>,
    raw: Vec<u8>,
}

fn u16le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn get_pstring(b: &[u8], off: usize) -> String {
    let n = b[off] as usize;
    cp866_decode(&b[off + 1..off + 1 + n])
}

fn put_pstring(buf: &mut [u8], off: usize, s: &str) -> Result<(), SaveError> {
    let raw = cp866_encode(s)?;
    assert!(raw.len() <= PSTRING_CAP);
    buf[off] = raw.len() as u8;
    buf[off + 1..off + 1 + raw.len()].copy_from_slice(&raw);
    // Bytes past the length are left exactly as they were.
    Ok(())
}

impl Save {
    /// `name` holds the original bytes, markup included, because round-trip
    /// must be byte-exact. Everything user-facing goes through here.
    pub fn display_name(&self) -> String {
        crate::text::strip(&self.name)
    }

    pub fn parse(bytes: &[u8]) -> Result<Save, SaveError> {
        if bytes.len() != SIZE {
            return Err(SaveError::BadSize(bytes.len()));
        }
        let mut stats = [0u16; 8];
        for (i, s) in stats.iter_mut().enumerate() {
            *s = u16le(bytes, OFF_STATE + 2 * i);
        }
        Ok(Save {
            magic: get_pstring(bytes, OFF_MAGIC),
            name: get_pstring(bytes, OFF_NAME),
            stats,
            hp: u16le(bytes, OFF_HP),
            hpmax: u16le(bytes, OFF_HPMAX),
            tail: bytes[OFF_TAIL..].to_vec(),
            raw: bytes.to_vec(),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = self.raw.clone();
        put_pstring(&mut buf, OFF_MAGIC, &self.magic).expect("magic is CP866-safe");
        put_pstring(&mut buf, OFF_NAME, &self.name).expect("name is CP866-safe");
        for (i, s) in self.stats.iter().enumerate() {
            buf[OFF_STATE + 2 * i..OFF_STATE + 2 * i + 2].copy_from_slice(&s.to_le_bytes());
        }
        buf[OFF_HP..OFF_HP + 2].copy_from_slice(&self.hp.to_le_bytes());
        buf[OFF_HPMAX..OFF_HPMAX + 2].copy_from_slice(&self.hpmax.to_le_bytes());
        buf[OFF_TAIL..].copy_from_slice(&self.tail);
        buf
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --test save_roundtrip`
Expected: `test result: ok. 4 passed; 0 failed`

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/main.rs src/save.rs tests/save_roundtrip.rs
git commit -m "feat: byte-exact .SAV parse/serialise in Rust"
```

---

### Task 8: Recover the RNG and port it

The Borland `0x08088405` multiplier is **not present** in this binary. Do not assume the stock `Random`. Find the actual generator.

**Fallback policy (decided by the project owner):** recovering the original
generator is the goal, but it is not worth blocking on. Work in this order:

1. **Recover it statically.** Read the routine in the decompilation and
   transcribe the recurrence. This is the preferred outcome and needs no
   emulator, no oracle, and no fixed seed.
2. **If the routine is recovered but capturing a reference sequence is
   impractical**, generate the vectors by executing the original routine's
   own bytes — not our Rust port — under an emulator, and say so in
   `docs/re/rng.md`.
3. **If the generator cannot be recovered at all**, substitute a
   self-contained PRNG of our own and move on. Do NOT add the `rand` crate;
   a documented 5-line xorshift or LCG in `src/rng.rs` keeps the dependency
   constraint intact and is sufficient.

**If you take option 3, you must do all of the following**, because it
downgrades the project's fidelity guarantee:
- Write `docs/re/rng.md` stating plainly that the RNG is NOT bit-faithful,
  what was tried, and why recovery failed.
- Delete `tests/rng_vectors.rs`'s `raw_sequence_matches_original` and
  `below_matches_original` tests rather than leaving them asserting against
  self-generated numbers. A test that compares our implementation to vectors
  produced by our implementation proves nothing and is worse than no test.
- Keep `below_stays_in_range` and add a determinism test (same seed produces
  the same sequence).
- Report DONE_WITH_CONCERNS, not DONE.

Never generate `data/rng_vectors.json` from the Rust implementation and
present it as ground truth. That is circular and silently fakes the
project's central guarantee.

**Files:**
- Create: `docs/re/rng.md`
- Create: `src/rng.rs`
- Modify: `src/lib.rs` (add `pub mod rng;`)
- Create: `data/rng_vectors.json`
- Test: `tests/rng_vectors.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct Rng { state: u32 }
  impl Rng {
      pub fn new(seed: u32) -> Rng;
      pub fn next_u32(&mut self) -> u32;
      /// Original's Random(n): uniform in 0..n.
      pub fn below(&mut self, n: u16) -> u16;
  }
  ```
  Task 9's combat code consumes `Rng`.

- [ ] **Step 1: Locate the generator in the decompilation**

Run:
```bash
grep -rln 'RandSeed\|randseed' build/decomp/ || true
grep -rl '0x8405\|33797\|134775813' build/decomp/ || true
python3 -c "
import json,pathlib
fs=json.loads(pathlib.Path('data/functions.json').read_text())
# The RNG is small and called from many places.
cands=[f for f in fs if f['size']<120 and len(f['called_by'])>=3]
for f in sorted(cands,key=lambda f:-len(f['called_by']))[:15]:
    print(len(f['called_by']), f['size'], f['entry'], f['name'])
"
```

The RNG is the highest-fan-in small function. Open its `build/decomp/*.c` file and read it.

- [ ] **Step 2: Record the finding**

Write `docs/re/rng.md` containing: the function's Ghidra address, its full decompiled listing, the recurrence in algebraic form (e.g. `state = state * A + C mod 2^32`) with the **actual constants read from the disassembly**, how the seed is initialised (look for `INT 1Ah` / BIOS tick reads if the game calls `Randomize`), and how `Random(n)` maps the 32-bit state onto `0..n-1`.

If the routine turns out not to be an LCG, document what it actually is. Do not force it into an LCG shape.

- [ ] **Step 3: Capture ground-truth vectors from the oracle**

The game must be made to reveal RNG output. The cheapest observable is combat damage rolls, which print as numbers. Use the oracle with a **fixed seed**: if `Randomize` seeds from the BIOS tick, pin it by launching DOSBox-X with a fixed start time, or patch a copy of the binary in the oracle workdir to skip `Randomize`.

Produce `data/rng_vectors.json`:

```json
{
  "note": "Captured from g.exe under DOSBox-X. See docs/re/rng.md for method.",
  "seed": 0,
  "next_u32": [],
  "below": [{"n": 0, "expected": []}]
}
```

Fill `next_u32` with at least 64 consecutive outputs and `below` with at least 3 moduli actually used by the game.

- [ ] **Step 4: Write the failing test**

Create `tests/rng_vectors.rs`:

```rust
use gopnik::rng::Rng;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct BelowCase {
    n: u16,
    expected: Vec<u16>,
}

#[derive(Deserialize)]
struct Vectors {
    seed: u32,
    next_u32: Vec<u32>,
    below: Vec<BelowCase>,
}

fn vectors() -> Vectors {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("rng_vectors.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn raw_sequence_matches_original() {
    let v = vectors();
    assert!(v.next_u32.len() >= 64, "need >=64 captured outputs");
    let mut r = Rng::new(v.seed);
    for (i, want) in v.next_u32.iter().enumerate() {
        assert_eq!(r.next_u32(), *want, "next_u32 diverges at index {i}");
    }
}

#[test]
fn below_matches_original() {
    let v = vectors();
    assert!(!v.below.is_empty(), "need at least one modulus case");
    for case in &v.below {
        let mut r = Rng::new(v.seed);
        for (i, want) in case.expected.iter().enumerate() {
            assert_eq!(r.below(case.n), *want, "below({}) diverges at {i}", case.n);
        }
    }
}

#[test]
fn below_stays_in_range() {
    let mut r = Rng::new(12345);
    for _ in 0..10_000 {
        assert!(r.below(37) < 37);
    }
}
```

- [ ] **Step 5: Run it to verify it fails**

Run: `cargo test --test rng_vectors`
Expected: FAIL — `unresolved module 'rng'`.

- [ ] **Step 6: Implement `src/rng.rs` from the documented recurrence**

Write the implementation using the constants recorded in `docs/re/rng.md`. The structure below is the shape to fill in — **substitute the real constants, do not ship these placeholders**:

```rust
//! Reimplementation of the original's pseudo-random generator.
//! Constants and structure are transcribed from docs/re/rng.md, which
//! cites the Ghidra address they were read from.

pub struct Rng {
    state: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng { state: seed }
    }

    pub fn next_u32(&mut self) -> u32 {
        // TRANSCRIBE FROM docs/re/rng.md — replace MULT and INC with the
        // constants actually read from the disassembly.
        const MULT: u32 = 0;
        const INC: u32 = 0;
        self.state = self.state.wrapping_mul(MULT).wrapping_add(INC);
        self.state
    }

    /// Original's `Random(n)`. The mapping from the 32-bit state to the
    /// 0..n range must match the RTL's, which is a widening multiply and
    /// high-word take, NOT a modulo — confirm against docs/re/rng.md.
    pub fn below(&mut self, n: u16) -> u16 {
        let r = self.next_u32() as u64;
        ((r * n as u64) >> 32) as u16
    }
}
```

Add `pub mod rng;` to `src/lib.rs`.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test --test rng_vectors`
Expected: `test result: ok. 3 passed; 0 failed`

If `raw_sequence_matches_original` fails at index 0, the seeding is wrong. If it fails at index 1+, the recurrence constants are wrong. If only `below_matches_original` fails, the range-mapping is wrong.

- [ ] **Step 8: Commit**

```bash
git add src/rng.rs src/lib.rs data/rng_vectors.json tests/rng_vectors.rs docs/re/rng.md
git commit -m "feat: recover and port original RNG with captured vectors"
```

---

### Task 9: Recover combat math and port it

**Files:**
- Create: `docs/re/combat.md`
- Create: `src/model.rs`
- Create: `src/combat.rs`
- Modify: `src/lib.rs`
- Create: `data/combat_vectors.json`
- Test: `tests/combat_vectors.rs`

**Interfaces:**
- Consumes: `Rng` from Task 8.
- Produces:
  ```rust
  // src/model.rs
  pub struct Fighter {
      pub name: String,
      pub level: u16,
      pub hp: u16,
      pub hpmax: u16,
      pub strength: u16,
      pub agility: u16,
      pub vitality: u16,
      pub luck: u16,
      pub armor: u16,
      pub dmg_min: u16,
      pub dmg_max: u16,
      pub broken_jaw: bool,
      pub broken_leg: bool,
      // Inventory/status fields. Not used by combat, but declared here so
      // the struct is defined exactly once; Task 11's handlers rely on them.
      pub joints: u16,
      pub stoned: bool,
      pub beer_dl: u16,
      pub money: i32,
  }

  impl Default for Fighter { /* zeroed, name empty */ }

  // src/combat.rs
  pub struct Blow { pub hit: bool, pub damage: u16 }
  pub fn accuracy_pct(attacker: &Fighter, defender: &Fighter) -> u16;
  pub fn second_blow_pct(attacker: &Fighter) -> u16;
  pub fn blows_per_round(attacker: &Fighter, defender: &Fighter) -> u16;
  pub fn resolve_blow(rng: &mut Rng, attacker: &Fighter, defender: &Fighter) -> Blow;
  ```
  Task 11's game loop calls `resolve_blow`.

- [ ] **Step 1: Find the combat function**

Anchor on strings. `^4Ты промазал` sits at file offset `0x4B13` and `^2Враг промазал` at `0x4C49` (from `data/strings.json`). Find which function references them:

```bash
python3 -c "
import json,pathlib
ss={s['off']:s['text'] for s in json.loads(pathlib.Path('data/strings.json').read_text())}
for off in (0x4b13,0x4c49,0x4b67,0x46bc,0x4701):
    print(hex(off), ss.get(off))
"
grep -rl 'Ты промазал\|4b13\|0x4b13' build/decomp/ || true
```

If the decompiler does not surface the string references directly, extend `ExportAll.java` to emit, per function, the list of data addresses it references, then map those to string offsets. Record the combat function's Ghidra address in `docs/re/combat.md`.

- [ ] **Step 2: Transcribe the formulas**

Read the decompiled combat function. Into `docs/re/combat.md`, record with the Ghidra address for each:
- hit-chance computation (the strings show a base `Точность #%` and a special-cased `Точность 90%`, so there is a cap — find it)
- damage roll: how `dmg_min`/`dmg_max` combine with weapon bonus and `Броня #`
- second-blow chance (`Второй удар #%`)
- multi-blow count (`- # ударов,  Точность # удара #%`)
- the agility comparison that produces `Из-за твоей хорошей ловкости враг сможет пнуть тебя раз # вместо #`
- jaw/leg break trigger conditions
- XP award per kill and the level-up threshold (`Сейчас у тебя # опыта, А для прокачки надо #`)

Every formula gets its address cited. Anything not fully understood is written down as an open question, not smoothed over.

- [ ] **Step 3: Capture combat vectors from the oracle**

Using the fixed-seed technique from Task 8, run scripted fights and record, per blow: attacker/defender stats before, RNG call count, hit/miss, damage, resulting HP. Write `data/combat_vectors.json`:

```json
{
  "note": "Captured from g.exe under DOSBox-X, fixed seed. Method in docs/re/combat.md.",
  "cases": [
    {
      "seed": 0,
      "attacker": {"level": 4, "strength": 24, "agility": 13, "vitality": 19, "luck": 7,
                   "armor": 0, "dmg_min": 3, "dmg_max": 7, "hp": 118, "hpmax": 129,
                   "broken_jaw": false, "broken_leg": false},
      "defender": {"level": 4, "strength": 20, "agility": 15, "vitality": 18, "luck": 5,
                   "armor": 2, "dmg_min": 2, "dmg_max": 6, "hp": 100, "hpmax": 100,
                   "broken_jaw": false, "broken_leg": false},
      "expected_blows": [{"hit": true, "damage": 5}]
    }
  ]
}
```

At minimum 20 cases spanning: zero armour and high armour, broken jaw, broken leg, large agility gap in both directions, and a level-1 vs a level-6 fighter.

- [ ] **Step 4: Write the failing test**

Create `tests/combat_vectors.rs`:

```rust
use gopnik::combat::{resolve_blow, Blow};
use gopnik::model::Fighter;
use gopnik::rng::Rng;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct FighterSpec {
    level: u16,
    strength: u16,
    agility: u16,
    vitality: u16,
    luck: u16,
    armor: u16,
    dmg_min: u16,
    dmg_max: u16,
    hp: u16,
    hpmax: u16,
    broken_jaw: bool,
    broken_leg: bool,
}

impl FighterSpec {
    fn build(&self, name: &str) -> Fighter {
        Fighter {
            name: name.to_string(),
            level: self.level,
            hp: self.hp,
            hpmax: self.hpmax,
            strength: self.strength,
            agility: self.agility,
            vitality: self.vitality,
            luck: self.luck,
            armor: self.armor,
            dmg_min: self.dmg_min,
            dmg_max: self.dmg_max,
            broken_jaw: self.broken_jaw,
            broken_leg: self.broken_leg,
            ..Default::default()
        }
    }
}

#[derive(Deserialize)]
struct ExpectedBlow {
    hit: bool,
    damage: u16,
}

#[derive(Deserialize)]
struct Case {
    seed: u32,
    attacker: FighterSpec,
    defender: FighterSpec,
    expected_blows: Vec<ExpectedBlow>,
}

#[derive(Deserialize)]
struct Vectors {
    cases: Vec<Case>,
}

#[test]
fn combat_matches_original() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("combat_vectors.json");
    let v: Vectors = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    assert!(v.cases.len() >= 20, "need >=20 captured cases, got {}", v.cases.len());

    for (ci, case) in v.cases.iter().enumerate() {
        let mut rng = Rng::new(case.seed);
        let a = case.attacker.build("A");
        let d = case.defender.build("D");
        for (bi, want) in case.expected_blows.iter().enumerate() {
            let Blow { hit, damage } = resolve_blow(&mut rng, &a, &d);
            assert_eq!(hit, want.hit, "case {ci} blow {bi}: hit");
            assert_eq!(damage, want.damage, "case {ci} blow {bi}: damage");
        }
    }
}

#[test]
fn damage_never_exceeds_defender_hp_underflow() {
    let mut rng = Rng::new(1);
    let a = FighterSpec {
        level: 6, strength: 90, agility: 120, vitality: 45, luck: 49, armor: 0,
        dmg_min: 20, dmg_max: 40, hp: 325, hpmax: 325,
        broken_jaw: false, broken_leg: false,
    }.build("A");
    let d = FighterSpec {
        level: 1, strength: 5, agility: 5, vitality: 5, luck: 1, armor: 0,
        dmg_min: 1, dmg_max: 2, hp: 3, hpmax: 3,
        broken_jaw: false, broken_leg: false,
    }.build("D");
    for _ in 0..1000 {
        let b = resolve_blow(&mut rng, &a, &d);
        assert!(b.damage < 10_000, "implausible damage {}", b.damage);
    }
}
```

- [ ] **Step 5: Run it to verify it fails**

Run: `cargo test --test combat_vectors`
Expected: FAIL — `unresolved module 'combat'`.

- [ ] **Step 6: Implement `src/model.rs` and `src/combat.rs`**

Write `Fighter` exactly as declared in the Interfaces block. Write `combat.rs` transcribing the formulas from `docs/re/combat.md`, with a comment on each function citing the Ghidra address it came from. Add `pub mod model; pub mod combat;` to `src/lib.rs`.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test --test combat_vectors`
Expected: `test result: ok. 2 passed; 0 failed`

- [ ] **Step 8: Commit**

```bash
git add src/model.rs src/combat.rs src/lib.rs data/combat_vectors.json tests/combat_vectors.rs docs/re/combat.md
git commit -m "feat: port combat math validated against captured oracle vectors"
```

---

### Task 9b: XP thresholds and stat growth

Combat produces XP; this task turns XP into levels. Split from Task 9 because a
reviewer can accept the damage math while rejecting the level curve.

**Files:**
- Create: `src/progress.rs`
- Modify: `src/lib.rs`
- Create: `data/xp.json`
- Create: `docs/re/progression.md`
- Test: `tests/progression.rs`

**Interfaces:**
- Consumes: `Fighter` from Task 9.
- Produces:
  ```rust
  /// XP required to advance FROM `level` to `level + 1`.
  pub fn xp_to_next(level: u16) -> u32;
  /// XP awarded for defeating `enemy` while at `player_level`.
  pub fn xp_award(player_level: u16, enemy: &Fighter) -> u32;
  pub struct LevelUp { pub new_level: u16, pub hpmax_gain: u16 }
  /// Applies as many level-ups as `xp` allows. Returns each one in order.
  pub fn apply_levels(f: &mut Fighter, xp: u32) -> Vec<LevelUp>;
  ```

- [ ] **Step 1: Recover the curve**

The status line `^6Сейчас у тебя # опыта, А для прокачки надо #` (offset `0x2F58`)
and `^6Сейчас у тебя # качков опыта. До слеующей прокачк` (offset `0x3DB9`) are
printed by the routine that owns the threshold. Find their referencing function
by the string-xref method from Task 9 Step 1, then read the threshold
computation.

Record in `docs/re/progression.md`, with the Ghidra address for each: the
threshold formula or lookup table, the XP award formula, and what a level-up
grants. The strings `^1Сила +1 `, `^1Ловкость +1 `, `^1Живучесть +1 `,
`^1Удача +1 ` (offsets `0x3D7C`–`0x3DAC`) show the per-level stat gains, and
`^1Понтовость увеличивается: ` (`0x3D5F`) precedes them.

- [ ] **Step 2: Capture the curve as data**

Write `data/xp.json`:

```json
{
  "note": "Read from g.exe. See docs/re/progression.md for addresses.",
  "thresholds": [],
  "award_cases": []
}
```

`thresholds[i]` is the XP needed to go from level `i+1` to `i+2`, covering at
least levels 1 through 10. `award_cases` holds `{"player_level", "enemy_level",
"expected"}` triples captured from the oracle.

Cross-check against the reference saves: `SAVE_R0` is level 4, `SAVE_R2`–`R4`
are level 6, `SAVE_R5` is level 5 (word at save offset `0x200`, pending Task 9's
confirmation of that field's meaning). Whatever curve you recover must be
consistent with those levels given each save's XP value.

- [ ] **Step 3: Write the failing test**

Create `tests/progression.rs`:

```rust
use gopnik::model::Fighter;
use gopnik::progress::{apply_levels, xp_award, xp_to_next};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct AwardCase {
    player_level: u16,
    enemy_level: u16,
    expected: u32,
}

#[derive(Deserialize)]
struct Xp {
    thresholds: Vec<u32>,
    award_cases: Vec<AwardCase>,
}

fn xp() -> Xp {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("data").join("xp.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn dummy(level: u16) -> Fighter {
    Fighter {
        name: "e".into(),
        level,
        hp: 10,
        hpmax: 10,
        strength: 5,
        agility: 5,
        vitality: 5,
        luck: 1,
        armor: 0,
        dmg_min: 1,
        dmg_max: 2,
        broken_jaw: false,
        broken_leg: false,
    }
}

#[test]
fn thresholds_match_original() {
    let x = xp();
    assert!(x.thresholds.len() >= 10, "need thresholds for levels 1..=10");
    for (i, want) in x.thresholds.iter().enumerate() {
        assert_eq!(xp_to_next(i as u16 + 1), *want, "threshold for level {}", i + 1);
    }
}

#[test]
fn thresholds_are_monotonic() {
    let x = xp();
    for w in x.thresholds.windows(2) {
        assert!(w[1] >= w[0], "curve must not decrease: {:?}", w);
    }
}

#[test]
fn awards_match_original() {
    let x = xp();
    assert!(!x.award_cases.is_empty(), "need captured award cases");
    for c in &x.award_cases {
        assert_eq!(
            xp_award(c.player_level, &dummy(c.enemy_level)),
            c.expected,
            "award for player {} vs enemy {}",
            c.player_level,
            c.enemy_level
        );
    }
}

#[test]
fn multiple_levels_apply_in_one_go() {
    let mut f = dummy(1);
    let huge = xp_to_next(1) + xp_to_next(2) + xp_to_next(3);
    let ups = apply_levels(&mut f, huge);
    assert!(ups.len() >= 3, "expected >=3 level-ups, got {}", ups.len());
    assert_eq!(f.level, 1 + ups.len() as u16);
    for w in ups.windows(2) {
        assert_eq!(w[1].new_level, w[0].new_level + 1, "levels must be sequential");
    }
}

#[test]
fn insufficient_xp_grants_nothing() {
    let mut f = dummy(1);
    let ups = apply_levels(&mut f, 0);
    assert!(ups.is_empty());
    assert_eq!(f.level, 1);
}
```

- [ ] **Step 4: Run it to verify it fails**

Run: `cargo test --test progression`
Expected: FAIL — `unresolved module 'progress'`.

- [ ] **Step 5: Implement `src/progress.rs`**

Transcribe the curve from `docs/re/progression.md`. If it is a lookup table
rather than a formula, embed `data/xp.json` with `include_str!` exactly as
Task 10's `data.rs` does, rather than hand-copying the numbers into Rust.
Add `pub mod progress;` to `src/lib.rs`.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --test progression`
Expected: `test result: ok. 5 passed; 0 failed`

- [ ] **Step 7: Commit**

```bash
git add src/progress.rs src/lib.rs data/xp.json tests/progression.rs docs/re/progression.md
git commit -m "feat: XP thresholds and stat growth ported from g.exe"
```

---

### Task 10: Extract item, shop and enemy tables

**Files:**
- Create: `tools/extract_tables.py`
- Create: `data/items.json`, `data/shops.json`, `data/enemies.json`
- Create: `docs/re/tables.md`
- Modify: `src/model.rs` (add `Item`, `ShopEntry`)
- Create: `src/data.rs`
- Test: `tools/test_extract_tables.py`, `tests/data_load.rs`

**Interfaces:**
- Produces:
  - `data/items.json`: `[{"id", "name", "kind", "bonus", "price"}]` where `kind` ∈ `weapon|armor|suit|charm|consumable|misc`
  - `data/enemies.json`: `[{"id", "name", "level", "stats": {...}}]`
  - `src/data.rs`: `pub struct Item { id, name, kind, bonus, price }` and
    `pub fn items() -> Vec<Item>`, deserialised from JSON embedded at compile
    time via `include_str!`. Returns an owned `Vec` rather than a `&'static
    [Item]` because `serde_json` cannot produce a `'static` slice without a
    `OnceLock`; add one only if profiling shows the parse matters.

- [ ] **Step 1: Enumerate the item strings**

The item names are already in `data/strings.json` with known offsets. Ground truth for the weapon/armour set, transcribed from the extraction:

| Offset | Text | Kind | Bonus |
|---|---|---|---|
| `0x30FE` | `^1Бутсы(+1) ` | weapon | +1 |
| `0x3114` | `^1Понтовые бутсы(Урон+2) ` | weapon | +2 |
| `0x312E` | `^1Кастет(+2) ` | weapon | +2 |
| `0x3146` | `^1Дубинка(+4)  ` | weapon | +4 |
| `0x3161` | `^1Нож(+6) ` | weapon | +6 |
| `0x3173` | `^1Тесак(Урон+9) ` | weapon | +9 |
| `0x323E` | `^1Костюм Adidas(+2) ` | suit | +2 |
| `0x3253` | `^1Костюм Abibas(+1) ` | suit | +1 |
| `0x3273` | `^1Крутая кожанка(+4) ` | armor | +4 |
| `0x3289` | `^1Кожанка(+2) ` | armor | +2 |
| `0x2FB2` | `^1Крестик(Удача +2) ` | charm | luck +2 |
| `0x2FC7` | `^1Кольцо "Гс"(Удача +1) ` | charm | luck +1 |
| `0x2FF0` | `^1Кольцо "Пг"(Всё +1) ` | charm | all +1 |
| `0x3007` | `^1Мега Кольцо(Всё +4) ` | charm | all +4 |
| `0x301E` | `^1Кольцо "Гп"(Самолечение) ` | charm | regen |

Prices are **not** in these strings — they are in the shop routine (see `#^7 руб. Самопальный пистолет ...` at `0xABCA`). Find the shop function by the same string-xref method as Task 9 and read the price table.

- [ ] **Step 2: Write the failing test**

Create `tools/test_extract_tables.py`:

```python
#!/usr/bin/env python3
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_tables():
    subprocess.run([sys.executable, str(ROOT / "tools" / "extract_tables.py")], check=True)
    items = json.loads((ROOT / "data" / "items.json").read_text(encoding="utf-8"))

    names = {i["name"] for i in items}
    for expected in ["Тесак", "Кастет", "Дубинка", "Нож", "Бутсы",
                     "Костюм Adidas", "Кожанка", "Мега Кольцо"]:
        assert expected in names, f"missing item {expected}"

    by_name = {i["name"]: i for i in items}
    assert by_name["Тесак"]["kind"] == "weapon"
    assert by_name["Тесак"]["bonus"] == 9
    assert by_name["Нож"]["bonus"] == 6
    assert by_name["Костюм Adidas"]["kind"] == "suit"
    assert by_name["Костюм Adidas"]["bonus"] == 2

    ids = [i["id"] for i in items]
    assert len(set(ids)) == len(ids), "item ids must be unique"

    for i in items:
        assert i["kind"] in {"weapon", "armor", "suit", "charm", "consumable", "misc"}
        assert isinstance(i["price"], int) or i["price"] is None

    print(f"OK {len(items)} items extracted")


if __name__ == "__main__":
    test_tables()
```

- [ ] **Step 3: Run it to verify it fails**

Run: `python3 tools/test_extract_tables.py`
Expected: FAIL — `extract_tables.py` missing.

- [ ] **Step 4: Write `tools/extract_tables.py`**

```python
#!/usr/bin/env python3
"""Derive item/shop/enemy tables from the extracted string table.

Item names carry their own bonus in the display text, e.g.
"^1Тесак(Урон+9) ", so the item table is recoverable from strings alone.
Prices are NOT in the strings -- they live in the shop routine's code and
are filled in from docs/re/tables.md once that routine is read. Until then
price is null, which is honest; an invented number is not.
"""
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
STRINGS = ROOT / "data" / "strings.json"
OUT_ITEMS = ROOT / "data" / "items.json"
OUT_SHOPS = ROOT / "data" / "shops.json"
OUT_ENEMIES = ROOT / "data" / "enemies.json"

# name-suffix -> (kind, bonus-group). Order matters: the more specific
# "Урон+N" / "Удача +N" patterns must be tried before the bare "+N".
PATTERNS = [
    (re.compile(r"^(?P<name>.+?)\(Урон\s*\+(?P<bonus>\d+)\)"), "weapon"),
    (re.compile(r"^(?P<name>.+?)\(Удача\s*\+(?P<bonus>\d+)\)"), "charm"),
    (re.compile(r"^(?P<name>.+?)\(Всё\s*\+(?P<bonus>\d+)\)"), "charm"),
    (re.compile(r"^(?P<name>.+?)\(Самолечение\)"), "charm"),
    (re.compile(r"^(?P<name>.+?)\(\+(?P<bonus>\d+)\)"), None),
]

# Bare "(+N)" is ambiguous, so classify by name.
BARE_KIND = {
    "Бутсы": "weapon",
    "Кастет": "weapon",
    "Дубинка": "weapon",
    "Нож": "weapon",
    "Костюм Adidas": "suit",
    "Костюм Abibas": "suit",
    "Кожанка": "armor",
    "Крутая кожанка": "armor",
}

COLOR_RE = re.compile(r"\^[0-7]")


def clean(text: str) -> str:
    return COLOR_RE.sub("", text).strip()


def slug(name: str) -> str:
    ascii_map = {
        "а": "a", "б": "b", "в": "v", "г": "g", "д": "d", "е": "e", "ё": "e",
        "ж": "zh", "з": "z", "и": "i", "й": "y", "к": "k", "л": "l", "м": "m",
        "н": "n", "о": "o", "п": "p", "р": "r", "с": "s", "т": "t", "у": "u",
        "ф": "f", "х": "h", "ц": "c", "ч": "ch", "ш": "sh", "щ": "sch",
        "ъ": "", "ы": "y", "ь": "", "э": "e", "ю": "yu", "я": "ya",
    }
    out = []
    for ch in name.lower():
        if ch in ascii_map:
            out.append(ascii_map[ch])
        elif ch.isalnum():
            out.append(ch)
        elif out and out[-1] != "_":
            out.append("_")
    return "".join(out).strip("_")


def parse_items(strings: list[dict]) -> list[dict]:
    items: dict[str, dict] = {}
    for s in strings:
        text = clean(s["text"])
        for rx, kind in PATTERNS:
            m = rx.match(text)
            if not m:
                continue
            name = m.group("name").strip()
            bonus = int(m.groupdict().get("bonus") or 0)
            k = kind or BARE_KIND.get(name)
            if k is None:
                k = "misc"
            # Prefer the first (lowest-offset) definition of a given name.
            if name not in items:
                items[name] = {
                    "id": slug(name),
                    "name": name,
                    "kind": k,
                    "bonus": bonus,
                    "price": None,
                    "src_off": s["off"],
                }
            break
    return sorted(items.values(), key=lambda i: i["src_off"])


def main() -> None:
    strings = json.loads(STRINGS.read_text(encoding="utf-8"))
    items = parse_items(strings)

    seen = set()
    for it in items:
        base = it["id"]
        n = 2
        while it["id"] in seen:
            it["id"] = f"{base}_{n}"
            n += 1
        seen.add(it["id"])

    OUT_ITEMS.write_text(
        json.dumps(items, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )
    # Shops and enemies require the code tables; emit empty scaffolds so the
    # files exist and later tasks can fill them without restructuring.
    for path in (OUT_SHOPS, OUT_ENEMIES):
        if not path.exists():
            path.write_text("[]\n", encoding="utf-8")
    print(f"wrote {len(items)} items to {OUT_ITEMS}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `python3 tools/test_extract_tables.py`
Expected: `OK <n> items extracted`

- [ ] **Step 6: Write `src/data.rs` and its test**

```rust
//! Compile-time embedded game tables extracted from g.exe.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub bonus: i32,
    pub price: Option<i32>,
}

static ITEMS_JSON: &str = include_str!("../data/items.json");

pub fn items() -> Vec<Item> {
    serde_json::from_str(ITEMS_JSON).expect("data/items.json is malformed")
}
```

Create `tests/data_load.rs`:

```rust
use gopnik::data;

#[test]
fn items_load_and_contain_known_entries() {
    let items = data::items();
    assert!(items.len() >= 15, "expected >=15 items, got {}", items.len());
    let tesak = items.iter().find(|i| i.name == "Тесак").expect("Тесак missing");
    assert_eq!(tesak.kind, "weapon");
    assert_eq!(tesak.bonus, 9);
}
```

- [ ] **Step 7: Run the Rust test**

Run: `cargo test --test data_load`
Expected: `test result: ok. 1 passed; 0 failed`

- [ ] **Step 8: Commit**

```bash
git add tools/extract_tables.py tools/test_extract_tables.py data/items.json data/shops.json data/enemies.json src/data.rs tests/data_load.rs docs/re/tables.md
git commit -m "feat: extract item/shop/enemy tables from g.exe"
```

---

### Task 11: Locations, command parser and game loop

**Files:**
- Create: `src/commands.rs`
- Create: `src/locations.rs`
- Create: `src/game.rs`
- Modify: `src/main.rs`, `src/lib.rs`
- Test: `src/commands.rs` (inline tests), `tests/game_flow.rs`

**Interfaces:**
- Produces:
  ```rust
  // src/commands.rs
  pub enum Command {
      BigMarket, Market, Vet, Girl, Den, Club, Gym, Stats, Leave, Fight,
      Inventory, Health, Save, Name, Joint, Weapon, Help, Quit,
      Key(char), Unknown(String),
  }
  pub fn parse(input: &str) -> Command;

  // src/locations.rs
  pub enum Location { Street, BigMarket, Market, Vet, Girl, Den, Club, Gym, Temple, Dorm }
  pub struct Places { found: [bool; 7] }   // mirrors PLACES.SAV
  impl Places {
      pub fn from_bytes(b: &[u8]) -> Places;
      pub fn to_bytes(&self) -> [u8; 7];
      pub fn reset_for_new_district(&mut self);
  }
  ```

- [ ] **Step 1: Write the failing command-parser tests**

In `src/commands.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_letter_verbs() {
        assert!(matches!(parse("bmar"), Command::BigMarket));
        assert!(matches!(parse("mar"), Command::Market));
        assert!(matches!(parse("rep"), Command::Vet));
        assert!(matches!(parse("girl"), Command::Girl));
        assert!(matches!(parse("pr"), Command::Den));
        assert!(matches!(parse("kl"), Command::Club));
        assert!(matches!(parse("trn"), Command::Gym));
        assert!(matches!(parse("sv"), Command::Save));
        assert!(matches!(parse("help"), Command::Help));
    }

    #[test]
    fn parses_single_letter_verbs() {
        assert!(matches!(parse("s"), Command::Stats));
        assert!(matches!(parse("w"), Command::Leave));
        assert!(matches!(parse("f"), Command::Fight));
        assert!(matches!(parse("i"), Command::Inventory));
    }

    #[test]
    fn is_case_insensitive_and_trims() {
        assert!(matches!(parse("  BMAR "), Command::BigMarket));
        assert!(matches!(parse("Trn"), Command::Gym));
    }

    #[test]
    fn longest_match_wins() {
        // "s" is Stats but "sv" is Save -- the parser must not prefix-match.
        assert!(matches!(parse("sv"), Command::Save));
        assert!(matches!(parse("s"), Command::Stats));
    }

    #[test]
    fn unknown_input_is_preserved() {
        match parse("zzz") {
            Command::Unknown(s) => assert_eq!(s, "zzz"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib commands`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement `commands.rs`**

Prepend this above the `#[cfg(test)]` block written in Step 1:

```rust
//! Verb parsing. The original dispatches on exact whole-word matches, so
//! `s` and `sv` are distinct commands and prefix matching would be wrong.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    BigMarket,
    Market,
    Vet,
    Girl,
    Den,
    Club,
    Gym,
    Stats,
    Leave,
    Fight,
    Inventory,
    Health,
    Save,
    Name,
    Joint,
    Weapon,
    Help,
    Quit,
    Key(char),
    Unknown(String),
}

pub fn parse(input: &str) -> Command {
    let v = input.trim().to_lowercase();
    match v.as_str() {
        "bmar" => Command::BigMarket,
        "mar" => Command::Market,
        "rep" => Command::Vet,
        "girl" => Command::Girl,
        "pr" => Command::Den,
        "kl" => Command::Club,
        "trn" => Command::Gym,
        "s" => Command::Stats,
        "w" => Command::Leave,
        "f" => Command::Fight,
        "i" => Command::Inventory,
        "hp" => Command::Health,
        "sv" => Command::Save,
        "name" => Command::Name,
        "kos" => Command::Joint,
        "wes" => Command::Weapon,
        "help" => Command::Help,
        "x" => Command::Quit,
        "a" | "d" | "e" | "h" | "k" | "t" => {
            Command::Key(v.chars().next().expect("non-empty by match arm"))
        }
        _ => Command::Unknown(v),
    }
}
```

Note `#[derive(Debug)]` is required — the Step 1 test formats a `Command` with `{other:?}`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test --lib commands`
Expected: `test result: ok. 5 passed`

- [ ] **Step 5: Write the `Places` test and implementation**

Create `tests/game_flow.rs`:

```rust
use gopnik::locations::Places;
use std::path::Path;

#[test]
fn places_round_trips_the_real_file() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("orig").join("PLACES.SAV");
    let bytes = std::fs::read(p).unwrap();
    assert_eq!(bytes.len(), 7);

    let places = Places::from_bytes(&bytes);
    assert_eq!(places.to_bytes().to_vec(), bytes);
}

#[test]
fn new_district_hides_all_places() {
    let mut places = Places::from_bytes(&[1u8; 7]);
    places.reset_for_new_district();
    assert_eq!(places.to_bytes(), [0u8; 7]);
}
```

Implement `src/locations.rs`:

```rust
//! Locations and the per-district rediscovery flags.
//!
//! PLACES.SAV is 7 bytes, one per rediscoverable location, in the order
//! below. The README states that entering a new district hides them all
//! again, which is what `reset_for_new_district` models.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Street,
    BigMarket,
    Market,
    Vet,
    Girl,
    Den,
    Club,
    Gym,
    Temple,
    Dorm,
}

/// The seven locations tracked by PLACES.SAV, in file order.
pub const TRACKED: [Location; 7] = [
    Location::Market,
    Location::BigMarket,
    Location::Vet,
    Location::Girl,
    Location::Den,
    Location::Club,
    Location::Gym,
];

#[derive(Debug, Clone)]
pub struct Places {
    found: [bool; 7],
}

impl Places {
    pub fn from_bytes(b: &[u8]) -> Places {
        assert_eq!(b.len(), 7, "PLACES.SAV must be 7 bytes, got {}", b.len());
        let mut found = [false; 7];
        for (i, slot) in found.iter_mut().enumerate() {
            *slot = b[i] != 0;
        }
        Places { found }
    }

    pub fn to_bytes(&self) -> [u8; 7] {
        let mut out = [0u8; 7];
        for (i, &f) in self.found.iter().enumerate() {
            out[i] = u8::from(f);
        }
        out
    }

    pub fn reset_for_new_district(&mut self) {
        self.found = [false; 7];
    }

    pub fn is_found(&self, loc: Location) -> bool {
        TRACKED
            .iter()
            .position(|&l| l == loc)
            .map(|i| self.found[i])
            .unwrap_or(true)
    }

    pub fn mark_found(&mut self, loc: Location) {
        if let Some(i) = TRACKED.iter().position(|&l| l == loc) {
            self.found[i] = true;
        }
    }
}
```

The ordering of `TRACKED` is a **hypothesis** — all five reference `PLACES.SAV`
bytes are `01`, so the file cannot disambiguate it. Confirm the order against
the save/load routine in the disassembly during Task 10 and correct it here if
it differs. The round-trip test passes either way, so it will not catch a
wrong order; only the disassembly will.

- [ ] **Step 6: Run**

Run: `cargo test --test game_flow`
Expected: `test result: ok. 2 passed`

- [ ] **Step 7: Implement `src/game.rs` and wire up `main.rs`**

```rust
//! The main loop. Dispatch only — the per-location behaviour lives in the
//! handlers, which are filled in as Task 10's tables land.

use crate::combat::resolve_blow;
use crate::commands::{parse, Command};
use crate::locations::{Location, Places};
use crate::model::Fighter;
use crate::rng::Rng;
use crate::text;
use std::io::{self, BufRead, Write};

pub struct Game {
    pub player: Fighter,
    pub places: Places,
    pub district: u8,
    pub rng: Rng,
    pub location: Location,
    running: bool,
}

impl Game {
    pub fn new(player: Fighter, seed: u32) -> Game {
        Game {
            player,
            places: Places::from_bytes(&[0u8; 7]),
            district: 1,
            rng: Rng::new(seed),
            location: Location::Street,
            running: true,
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();
        self.banner();
        while self.running {
            print!("> ");
            io::stdout().flush()?;
            let Some(line) = lines.next() else { break };
            self.dispatch(parse(&line?));
        }
        Ok(())
    }

    fn banner(&self) {
        println!(
            "{}",
            text::render("^4Gopnik: ^7version 1.02 june,sept 2003")
        );
    }

    fn dispatch(&mut self, cmd: Command) {
        match cmd {
            Command::Quit => self.running = false,
            Command::Stats => self.show_stats(),
            Command::Health => self.show_health(),
            Command::Fight => self.fight(),
            Command::Leave => self.location = Location::Street,
            Command::Market => self.goto(Location::Market),
            Command::BigMarket => self.goto(Location::BigMarket),
            Command::Vet => self.goto(Location::Vet),
            Command::Girl => self.goto(Location::Girl),
            Command::Den => self.goto(Location::Den),
            Command::Club => self.goto(Location::Club),
            Command::Gym => self.goto(Location::Gym),
            Command::Help => self.show_help(),
            Command::Inventory => self.show_inventory(),
            Command::Save => self.save_game(),
            Command::Name => self.rename(),
            Command::Joint => self.smoke(),
            Command::Weapon => self.show_weapon(),
            Command::Key(k) => self.handle_key(k),
            Command::Unknown(s) => println!("{}", text::render(&format!("^4? {s}"))),
        }
    }

    fn goto(&mut self, loc: Location) {
        if self.places.is_found(loc) {
            self.location = loc;
        } else {
            println!(
                "{}",
                text::render("^6Ты пока что неузнал где в этом районе это место")
            );
        }
    }

    fn show_health(&self) {
        println!(
            "{}",
            text::render(&text::fill(
                "Здоровье #/#  ",
                &[self.player.hp as i64, self.player.hpmax as i64],
            ))
        );
    }

    fn show_stats(&self) {
        let p = &self.player;
        println!(
            "{}",
            text::render(&text::fill(
                "Сл:# Лв:# Жв:# Уд:#",
                &[
                    p.strength as i64,
                    p.agility as i64,
                    p.vitality as i64,
                    p.luck as i64,
                ],
            ))
        );
        self.show_health();
    }

    /// Picks an opponent for the current district from data/enemies.json.
    /// Returns None only if no enemy is defined for this district, which is
    /// a data error rather than a game state.
    fn pick_enemy(&mut self) -> Option<Fighter> {
        let pool = crate::data::enemies();
        let eligible: Vec<_> = pool
            .iter()
            .filter(|e| e.district == self.district)
            .collect();
        if eligible.is_empty() {
            return None;
        }
        let i = self.rng.below(eligible.len() as u16) as usize;
        Some(eligible[i].to_fighter())
    }

    fn show_inventory(&self) {
        for line in self.player.inventory_lines() {
            println!("{}", text::render(&line));
        }
    }

    fn save_game(&self) {
        match self.write_save() {
            Ok(path) => println!("{}", text::render(&format!("^2Сохранено: {path}"))),
            Err(e) => println!("{}", text::render(&format!("^4Ошибка записи: {e}"))),
        }
    }

    fn show_weapon(&self) {
        println!(
            "{}",
            text::render(&text::fill(
                "Урон #-#    ",
                &[self.player.dmg_min as i64, self.player.dmg_max as i64],
            ))
        );
    }

    fn rename(&mut self) {
        print!("^7 ");
        let _ = io::stdout().flush();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_ok() {
            let n = line.trim();
            if !n.is_empty() {
                self.player.name = n.to_string();
            }
        }
    }

    /// Косяк: the joint. Effects are transcribed from the "Обдолбаный"
    /// status strings; see docs/re/tables.md.
    fn smoke(&mut self) {
        if self.player.joints == 0 {
            println!("{}", text::render("^4Косяков нет"));
            return;
        }
        self.player.joints -= 1;
        self.player.stoned = true;
        println!("{}", text::render("^6Обдолбаный  "));
    }

    /// Single-key commands a/d/e/h/k/t. Each maps to a shop or menu action
    /// whose meaning is established in Task 10; dispatch is exhaustive so a
    /// key with no action is an explicit no-op, not a silent fallthrough.
    fn handle_key(&mut self, k: char) {
        match k {
            'h' => self.show_health(),
            'e' => self.drink_beer(),
            'a' | 'd' | 'k' | 't' => self.shop_action(k),
            _ => println!("{}", text::render(&format!("^4? {k}"))),
        }
    }
        for line in [
            "Напиши: ^6bmar^7 чтобы идти на рынок",
            "Напиши: ^6rep^7  чтобы идти к ветеринару",
            "Напиши: ^6girl^7 чтобы завалиться к своей девчонке",
            "Напиши: ^6pr^7   чтобы идти в местный притон гопоты",
            "Напиши: ^6kl^7   чтобы идти в клуб",
            "Напиши: ^6trn^7  чтобы идти в качалку",
            "Напиши: ^6s^7    чтобы посмотреть статистику",
        ] {
            println!("{}", text::render(line));
        }
    }

    fn fight(&mut self) {
        // Opponents come from data/enemies.json, populated in Task 10.
        let Some(mut enemy) = self.pick_enemy() else {
            println!("{}", text::render("^6Тут никого нет"));
            return;
        };

        while self.player.hp > 0 && enemy.hp > 0 {
            let blow = resolve_blow(&mut self.rng, &self.player, &enemy);
            enemy.hp = enemy.hp.saturating_sub(blow.damage);
            if enemy.hp == 0 {
                break;
            }
            let back = resolve_blow(&mut self.rng, &enemy, &self.player);
            self.player.hp = self.player.hp.saturating_sub(back.damage);
        }

        let msg = if self.player.hp == 0 {
            "^4Ты сдох."
        } else {
            "^2Ты победил."
        };
        println!("{}", text::render(msg));
    }
}
```

Wire `src/main.rs`:

```rust
use gopnik::game::Game;
use gopnik::model::Fighter;

fn main() -> std::io::Result<()> {
    let player = Fighter {
        name: "пацан".to_string(),
        level: 1,
        hp: 30,
        hpmax: 30,
        strength: 5,
        agility: 5,
        vitality: 5,
        luck: 3,
        armor: 0,
        dmg_min: 1,
        dmg_max: 3,
        broken_jaw: false,
        broken_leg: false,
    };
    Game::new(player, 0).run()
}
```

Add `pub mod game;` to `src/lib.rs`. The `saturating_sub` calls matter: HP is
`u16`, and a plain subtraction would wrap to 65535 on a killing blow.

**Remaining handler contracts.** The dispatch above references these; implement
each from the strings in `data/strings.json` and the tables from Task 10. Every
one is small — none should exceed ~15 lines.

| Signature | Behaviour | Source strings |
|---|---|---|
| `fn drink_beer(&mut self)` | Refuse if `broken_jaw`; else if `beer_dl == 0` print "Пива нету"; else consume 1 unit, add the healing amount, clamp at `hpmax` | `0x419C`, `0x41CD`, `0x41E4`, `0x4240`, `0x424C`, `0x4283` |
| `fn shop_action(&mut self, k: char)` | Look up the entry for key `k` in `data/shops.json` for the current location; if affordable, deduct price and grant the item, else print the no-money line | `0x32B7`, `0x32BF` |
| `fn write_save(&self) -> std::io::Result<String>` | Build a `Save` from current state, write `SAVE_R<district>.SAV` via `Save::to_bytes`, return the filename | — |
| `fn inventory_lines(&self) -> Vec<String>` on `Fighter` | One line per owned item and status flag, in the original's display order | `0x2FA9`–`0x32CC` |

**Additional `Fighter` fields** beyond Task 9's declaration, needed here — add
them in Task 9 when you write `model.rs` so the struct is defined once:
`pub joints: u16`, `pub stoned: bool`, `pub beer_dl: u16`, `pub money: i32`.

- [ ] **Step 8: Manual smoke run**

Run: `cargo run`
Expected: the banner renders in colour, `s` prints a stat block, `help` lists verbs, `x` exits cleanly.

- [ ] **Step 9: Commit**

```bash
git add src/commands.rs src/locations.rs src/game.rs src/main.rs src/lib.rs tests/game_flow.rs
git commit -m "feat: command parser, locations and playable game loop"
```

---

### Task 12: Differential test against the original

The final gate. Same seed plus same input script must produce the same numbers in both the original and the port.

**Scope depends on Task 8's outcome.** Check `docs/re/rng.md` first:

- **RNG was recovered bit-faithfully** — compare the full integer sequence, as
  described below. Every number the game prints is in scope.
- **RNG fell back to a substitute generator (Task 8 option 3)** — the two
  implementations diverge on the first random draw, so a full-sequence
  comparison is meaningless and must NOT be attempted. Restrict the
  comparison to values that do not depend on the RNG: shop prices, XP
  thresholds, level-up stat gains, starting stats per class, item bonuses,
  and menu numbering. Implement this by having the port emit a
  `--trace-deterministic` mode that prints only those quantities, and compare
  that against the same values read from the original's screens. Say
  explicitly in `docs/re/difftest.md` which quantities are covered and which
  are out of scope, so the reduced guarantee is visible rather than implied.

**Files:**
- Create: `tools/difftest.py`
- Create: `data/difftest_scripts/*.txt`
- Create: `docs/re/difftest.md`
- Test: `tools/test_difftest.py`

**Interfaces:**
- Consumes: the oracle from Task 3, the built Rust binary.

- [ ] **Step 1: Write input scripts**

Create at least five scripts under `data/difftest_scripts/`, each a newline-separated keystroke sequence covering: character creation for each of the four classes; a full fight to the death; a shopping trip; a gym session; save-then-reload.

- [ ] **Step 2: Write the comparator**

Create `tools/difftest.py`:

```python
#!/usr/bin/env python3
"""Compare the original binary and the Rust port on identical input.

We compare the ordered sequence of integers each side prints, not the raw
text. That is deliberate: the fidelity target is game logic, not cursor
positioning, so layout differences must not fail the test while a wrong
damage roll must.
"""
import pathlib
import re
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent / "oracle"))
import capture  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "data" / "difftest_scripts"
PORT_BIN = ROOT / "target" / "release" / "gopnik"

INT_RE = re.compile(r"-?\d+")
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")


def numbers(text: str) -> list[int]:
    return [int(m.group()) for m in INT_RE.finditer(ANSI_RE.sub("", text))]


def run_port(keys: str) -> str:
    r = subprocess.run(
        [str(PORT_BIN)],
        input=keys,
        capture_output=True,
        text=True,
        timeout=60,
    )
    return r.stdout


def run_original(keys: str, out_dir: pathlib.Path) -> str:
    work = capture.run(keys, out_dir)
    # capture.run leaves DOSBox-X's captured screen text in the workdir;
    # docs/re/oracle.md records the exact filename produced on this machine.
    caps = sorted(work.glob("capture/*.txt")) or sorted(work.glob("*.txt"))
    if not caps:
        raise RuntimeError(f"no captured output in {work}")
    return "\n".join(c.read_text(encoding="cp866", errors="replace") for c in caps)


def compare(script: pathlib.Path) -> tuple[bool, str]:
    keys = script.read_text(encoding="utf-8")
    orig = numbers(run_original(keys, pathlib.Path("/tmp/difftest") / script.stem))
    port = numbers(run_port(keys))

    for i, (a, b) in enumerate(zip(orig, port)):
        if a != b:
            lo = max(0, i - 5)
            return False, (
                f"{script.name}: diverges at index {i}: "
                f"original={a} port={b}\n"
                f"  original[{lo}:{i + 5}] = {orig[lo:i + 5]}\n"
                f"  port    [{lo}:{i + 5}] = {port[lo:i + 5]}"
            )
    if len(orig) != len(port):
        return False, (
            f"{script.name}: length mismatch: original emitted {len(orig)} "
            f"integers, port emitted {len(port)}"
        )
    return True, f"{script.name}: {len(orig)} integers match"


def main() -> int:
    scripts = sorted(SCRIPTS.glob("*.txt"))
    if not scripts:
        print("FAIL no difftest scripts found")
        return 1
    failures = 0
    for s in scripts:
        ok, msg = compare(s)
        print(("OK   " if ok else "FAIL ") + msg)
        failures += not ok
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 3: Write the failing test**

Create `tools/test_difftest.py`:

```python
#!/usr/bin/env python3
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_all_scripts_match():
    scripts = sorted((ROOT / "data" / "difftest_scripts").glob("*.txt"))
    assert len(scripts) >= 5, f"expected >=5 difftest scripts, got {len(scripts)}"

    r = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "difftest.py")],
        capture_output=True,
        text=True,
    )
    print(r.stdout)
    print(r.stderr, file=sys.stderr)
    assert r.returncode == 0, "difftest reported divergences"
    print("OK original and port agree on all scripts")


if __name__ == "__main__":
    test_all_scripts_match()
```

- [ ] **Step 4: Run it**

Run: `cargo build --release && python3 tools/test_difftest.py`
Expected: all scripts match. Divergences are real bugs — trace each back to the responsible formula, fix it in the Rust source, and add a targeted vector to Task 9's `combat_vectors.json` so the regression is pinned.

- [ ] **Step 5: Document results**

Write `docs/re/difftest.md`: which scripts pass, any known divergences with their cause, and the reproduction command.

- [ ] **Step 6: Commit**

```bash
git add tools/difftest.py tools/test_difftest.py data/difftest_scripts docs/re/difftest.md
git commit -m "test: differential validation of Rust port against original binary"
```

---

## Known risks

1. **Fixed-seed capture may be impossible without patching.** If `Randomize` seeds from the BIOS tick, the oracle cannot reproduce sequences. Mitigation: patch a *copy* of `g.exe` in the oracle workdir to NOP the `Randomize` call, and document the patch offset in `docs/re/rng.md`. Never modify `orig/g.exe`.
2. **`SDL_VIDEODRIVER=dummy` may not work with this SDL1 build.** Fallback is `xvfb-run`. Task 3 Step 4 resolves this before anything depends on it.
3. **Screen capture granularity.** If `autotype` timing proves flaky, switch to DOSBox-X's debugger to dump the text-mode video buffer at breakpoints. Slower but deterministic.
4. **The eight state words at save `0x200` may not all be stats.** They are named `unk_stat*` until Task 9 confirms them from the disassembly. Byte-exact round-trip holds regardless, so Task 7 is not blocked.
5. **Some of the 123 functions are Borland RTL, not game code.** Expect roughly 60–80 RTL functions. Identify them by their stereotyped shapes (string concat, `Val`, `Str`, file I/O) and exclude them early to avoid wasted RE effort.
