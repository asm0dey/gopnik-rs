# GOPNIK Rust port — SDD progress ledger

**Plan:** `docs/superpowers/plans/2026-08-17-gopnik-rust-port.md` (committed — the plan is the source of truth, and it has been revised several times)
**Repo:** `/home/finkel/work_self/gopnik-rs`
**Branch:** `port/gopnik-rust`

**Task order (revised four times):** 1, 2, 4, 4b, 4c, 2b, 2c, 3, 5, 6, 7, 8, 9,
9b, 10, **10b**, 11, 12

Task 10b (cross-platform colour output) was added at the owner's request after
Task 7. It sits before Task 11 so the game loop is written against `term::` from
the start instead of having ~20 `println!` sites rewritten afterwards.

## Completed

| Task | Status | Commits |
|---|---|---|
| 1 — corpus verification | complete, approved | `e56e8a6..60c190c` |
| 2 — blind string scan | complete, approved | `60c190c..5e105da` |
| 4 — Ghidra headless export | complete, approved | `18ecf56` |
| 4b — string pointers from immediates | complete, approved (3 review rounds) | `2352278`, `70f0707`, `77a8795`, `a486acb` |
| 4c — indexed string array tables | complete, approved | `534bfe8` |

| 2b + 2c — pointer-anchored re-extraction + gap tiling | complete, approved (2 review rounds) | `01de56b..4fd2fda` |
| 3 — DOSBox-X oracle harness | complete, approved (3 fix waves) | `3f372a0..f44b6f7` |
| 5 — .SAV decoder + layout artifact | complete, approved (first pass) | `8973100` |
| 6 — Rust crate skeleton + text layer | complete, approved (1 fix wave) | `85c38b3`, `2533d35` |
| 7 — Rust save load/store, byte-exact | complete, approved (1 fix wave) | `5b066ad`, `a7b1152` |
| 8 — RNG recovered and ported | complete, approved (1 fix wave) | `74adfdc`, `8fe47cc` |
| 9 — combat math recovered + ported | complete, approved (2 fix waves) | `ab6b8d3`, `11eeea8`, `002c674` |
| 9b — XP thresholds + stat growth | complete, approved (2 fix waves) | `e699366`, `faf55fe`, `c90e73b` |
| 10 — item/shop/enemy tables | implemented + approved; 2 fix waves AWAITING RE-REVIEW | `17a23a0`, `0f83d5d`, `1b2b797` |

**NEXT: re-review Task 10's two fix waves, then Task 10b, then Task 11.**

### RESUME HERE — exact state

Task 10 is IMPLEMENTED and was APPROVED on first review, then had two fix waves
applied which have NOT yet been re-reviewed:
- `17a23a0` implementation (approved: no Critical/Important; every price, gate,
  boss immediate and class weight re-derived from the binary by the reviewer,
  and the artifacts regenerate byte-identically in a tree containing no Rust)
- `0f83d5d` fix wave 1 — 2 Important + 6 Minor doc/consistency findings
- `1b2b797` fix wave 2 — owner-directed runtime/provenance split

**To resume:** `scripts/review-package 16b8171 HEAD` and dispatch a task
reviewer over the whole range, or `0f83d5d^..HEAD` for just the two waves.

### Task 10 outcome

Items 15 · shops 18 (2 shops x 9) · enemies 13 (11 rank classes + 2 scripted
bosses). Prices are NOT immediates — a 19-byte const array at `20ae:0b2e`,
bounded by `ranks` ending exactly there and `krutizna` starting at `0b42`,
read via `mov al,[20ae:0b2e+n]`. The Ghidra decompiler mangles the 16-bit
far-call argument order, so rows were read from a real `ndisasm` disassembly.

**There is NO enemy stat table and none was invented.** `FUN_1000_0d14` rolls
classes 0..9 from the class-weight array at `20ae:0002`, so those 11 rows carry
`level: null, stats: null`. The only fixed stat blocks are the two scripted
Ректор НГУ fights at `FUN_1000_11c2`. Proven as a negative by an
immediate-store scan: `C7 06 <5439|5639|5839|5A39>` occurs at exactly two
sites, both inside `1000:11c2`.

**An original bug is reproduced, not fixed:** `bmar` row 9 prints `[20ae:0b3f]`
(70) but charges `[20ae:0b40]` (60). Both values are runtime fields; a test
pins the asymmetry; an oracle screen shows `Бабки 970 -> 910`.

**Runtime/provenance split (owner-directed, done in `1b2b797`).** The game
binary embeds ONLY runtime fields. Addresses live in
`data/{items,shops,enemies}.provenance.json`, keyed by `id` (shops by
`"<shop>:<key>"`, e.g. `"bmar:9"`). `charged` was judged provenance — it is a
cross-check, always true for all 18 rows, and no gameplay branch reads it.
`code_off`/`prefix_off`/`text_off` were never on the struct at all; serde was
silently dropping them.

**13 of 15 items have `price: null`, split into two distinct cases** — `sold:
false` for loot-only items (`Тесак`, `Нож`, `Крестик`, the rings, verified
against the wandering-encounter find table) and pending for the suits/jackets
sold under paraphrased names. Only `Кастет` (25) and `Дубинка` (50) are priced.
The boots ambiguity is the one genuinely undecidable case.

**Two Ghidra citations were file offsets wearing a segment prefix** — the
classic `0x2af8 - 0x18d0 = 0x1228` confusion. Both fixed, and an audit of every
other paired citation found one more (`1000:aeb1`/`aebd` -> `1000:ae27`/`ae33`).
`file_off = 0x18d0 + (seg - 0x1000)*16 + off`. **Watch for this.**

**`data/other_price_sites.json` records all 32 places `orig/g.exe` debits
`[20ae:38c7]`** — 21 `sub [money],ax` (`29 06 C7 38`) and 11 `sub
[money],imm8` (`83 2E C7 38 ib`). 18 of the `ax` sites are the `mar`/`bmar`
rows already in `data/shops.json`, so **14 are further price sites**: the two
Клуб `imm8` rows at 15 and 22, nine still-unidentified `imm8` sites, the
computed `district*50` save charge at `1000:761d`, the `var`-read BSS site
`20ae:3c82` debited at `1000:e0a8`, and a call-result debit at `1000:5014`
whose amount is the return value of `call 0f78:1131` — purpose unknown. Task
11 should start there rather than rediscovering them; eleven of the fourteen
carry `what: null` — three are named (the two Клуб rows and the save charge).

Both `sub` encodings are now scanned bare over the whole file and every `ax`
match is classified by the idiom that produced `ax`, so the artifact is
complete by construction: `ax_debit_sites.count` equals the number of `29 06
C7 38` occurrences in the binary *and* the sum of its category counts, and
`tools/test_extract_tables.py` re-scans `orig/g.exe` to assert it. The earlier
claim that the `district*50` row "has no fixed byte idiom to scan for" was
false — that claim is what licensed leaving the whole `sub [money],ax` form
unscanned, which is how `1000:5014` went unrecorded. It is scanned now; only
`what` text and its string cross-references are hand-annotated.

### Task 9b outcome

`xp_to_next(level) = 10 + 10*level` — a stored requirement, not a table
(`1000:6de0` init 10, `1000:2550` +10, `1000:4ac7` -10 on de-level).
XP award = sum of the enemy's four stats (`1000:51b9`), level-independent.
A level draws exactly TWO increases against a class weight table at `DS:0002`
(`FUN_1000_2526`).

**The `hpmax` anomaly is SOLVED and it was not a coincidence.** A consumable at
`1000:4b57` grants `+2 str / +1 dmg_min / +2 dmg_max` and sets a countdown byte
`[0x38cd] := 3`; `1000:aeb3` subtracts it back on expiry. Neither touches
`hpmax`. That byte is `.SAV 0x231` and reads `0,1,0,2,0` across R0/R2/R3/R4/R5
— nonzero in exactly the two saves that were 2 low. So
`hpmax == 10 + 5*vit + str - 2*(buff active)` on all five. Corroborated
independently: `dmg_max - str` and `dmg_min - str div 2` are buff-invariant and
agree pairwise on every save. A second grant site exists at `1000:e9b8` with
countdown 10 instead of 3.

**`rank_index` is CLOSED** (was a Task 9 open question): the stored value is the
class prompt's answer + 3 (`1000:71b8`). It selects both the rank name and the
growth weights, and a class's starting stats ARE its weight row.
`new_character`: `hpmax := vit*5 + 10 + str`, `hp := hpmax`,
`dmg_min := str idiv 2`, `dmg_max := str` (`1000:71bd..71e4`).

**Owner-approved interface amendments (plan updated, do not revert):**
- `apply_levels(&mut Progress, &mut Fighter, &mut Rng, award, uncapped)`. The
  brief's `apply_levels(f, xp)` CANNOT express the original: the draw needs the
  class (`1000:25aa`) and the shared generator (`1000:25fe`), and the stored
  threshold diverges from `10+10*level` at the cap because the drain loop
  (`1000:2546`) is uncapped while the grant loop (`1000:2580`) is not.
- **`class` lives in `Fighter`**, field `+0x00` of the record it mirrors.
  `Progress` is `{xp, threshold}`.

**The original's `==`-not-`>=` cap bug is reproduced deliberately**
(`1000:2580`), so a level already past 40 is not stopped.

**Draw order at level-up IS pinned** — better than the implementer's own concern
claimed. `FUN_1000_2526` (`0x2526..0x28c8`) contains EXACTLY ONE `Random` call
site, `1000:25fe`, in a loop bounded by `cmp word [bp-0x8],0x2`. Two draws per
level, nothing else, called at `1000:523b` BEFORE the post-kill `Random(0x1e)`
at `1000:52d5`. Only the roll->stat mapping under a live seed is unreplayed.

**8 of Task 9's 12 unmapped `Random` sites are now mapped** (the whole post-kill
group). Four flee/command-handler sites remain open: `1000:4db7, 4e16, 4ef5,
4f18`.

`data/save_layout.json` now TILES the record exactly (694 = 694, no gaps, no
overlaps), enforced by `save_layout_json_fields_tile_the_record`. Tail region:
`unk_0214`(29) `buff_countdown`(1) `xp`(2) `threshold`(2) `growth_log`(120)
`unk_02ae`(8).

**Coverage gap, honestly marked:** thresholds observed at 13 levels
(1,2,10,11,15,16,17,20,21,30,31,32,40). The rest carry
`UNVERIFIED by observation` in `data/xp.json`, enforced by a test — a fresh
character dies before grinding that far (12 seeds tried, best reached level 2).

### Task 9 outcome

Combat function is **`FUN_1000_3d11` (`1000:3d11`)**, confirmed by five combat
strings resolving to instructions inside its body and nowhere else.
**Task 4's three candidates (`1000:1a03`, `1000:6a0d`, `1000:7c67`) were all
wrong** and are retracted in `docs/re/functions.md`.

`src/combat.rs` + `src/model.rs` ported and validated against **295 cases /
352 blows** captured from the original under a seed-pinned oracle. A reviewer
independently re-derived the blow math from the disassembly and reproduced
every blow with zero mismatches. Swapping the high-take for `%` breaks 206 of
314 blows — the data pins the semantics hard.

**Seed pinning exists now and Task 12 should reuse it.** `pin_seed` in
`tools/oracle/capture.py` patches `Randomize`'s 13-byte body at file offset
`0x12230` — on the SCRATCH COPY only. It refuses any binary lacking that exact
body. `docs/re/combat.md:314-345` documents apply AND remove. `orig/g.exe` is
never touched; no pinned binary is ever committed. Owner's constraint: unpin
when possible; pinning is capture-time only, the port keeps normal seeding.

**Vector capture is genuinely non-circular.** `tools/capture_combat_vectors.py`
reads damage/hit-miss from CP866 screen text and stats/seed from guest memory
via a new `STATE.BIN` window, gated on a save-banner signature so `DS` is
proven not assumed. It never imports or reads the Rust crate.

**The TSR changed.** `scrhook.asm`/`.com` grew 81 bytes to dump the stats the
vectors need (agility, luck, armor, dmg_min/max, RandSeed are never printed).
Reassembles byte-identically; `test_scrhook_matches_source` still guards it.

**Harness defect fixed:** DOSBox-X was popping a modal "Quit DOSBox-X warning"
on teardown, which would hang any unattended run. `quit warning=false` in the
conf, and the SIGTERM step was removed entirely (every frame is already on
disk, so SIGTERM bought nothing but dosbox-x's shutdown path). Always run
through `capture.py` — it sets `SDL_VIDEODRIVER=dummy`; a bare `dosbox-x`
invocation opens a real window and brings the modal back.

**Findings that change other tasks:**
- The eight save stat words are now NAMED, not `unk_*`:
  `rank_index, strength, agility, vitality, luck, level, dmg_min, dmg_max`.
  Evidenced from `1000:1419`, which prints `+0x02..+0x08` against
  `Сл:# Лв:# Жв:# Уд:#`, and `1000:143b` printing `Урон #-#`.
  **Level is at `0x20a`. `0x200` is the rank-name index.** Task 9b's brief was
  corrected. `SAVE_R0` is level 15, not 4 — 4 was the rank index.
- **Level and broken jaw/leg do NOT affect combat math at all.** Verified by
  linear sweep of `[3d11,584c)`. The brief's "level 1 vs level 6" coverage row
  is vacuous and cannot discriminate a correct implementation.
- **12 of the 27 `Random` sites in the combat function are unmapped**
  (`4db7, 4e16, 4ef5, 4f18, 52d5, 5402, 5427, 5454, 5482, 5530, 5617, 5681` —
  flee handlers and the post-kill loot/stat-gain block). Recorded as open
  questions in `docs/re/combat.md`. **Task 12's whole-battle differential
  replay will desync on every one of them.**

**Open, deliberately not guessed:** `hpmax == 10 + 5*vitality + strength` holds
exactly for R0/R3/R5 but is off by 2 for R2 and R4. `rank_index`'s class→value
mapping is unmapped. Five UNVERIFIED combat gaps remain (armour fully absorbing
a hit — unreachable; break comparison at exact equality; the зубная защита
`Random(4)` branch — `Fighter` has no field for the item; collapse constants
10/28 in isolation).

### Task 8 outcome — the RNG is the real one, tier 1, no substitute

`System.@Rand` at **`1f78:11a8`** IS the stock Borland LCG:
`RandSeed := RandSeed * $08088405 + 1 (mod 2^32)`.

**The plan's old "multiplier is absent" fact was a false conclusion from true
observations.** `05 84 08 08` occurs 0 times in the file, `b8 05 84` 0 times,
`ba 08 08` 0 times — all correct. But the compiler never materialises the
dword: it multiplies by the low word `$8405` (the single literal `05 84` at
`1f78:11de`) and synthesises `$0808` from a shift/add chain. No byte search
could have found it. Corrected in the plan.

**`Random(n)` at `1f78:114b` is `(RandSeed * n) >> 32` — a high-take, NOT a
modulo.** Different distribution. Confirmed: committed vectors match
`(s*n)>>32` and fail `s%n` for all six moduli.

**Task 9 must mirror 16-bit wrapping at `Random(Integer)` call sites, not
clamp.** Real examples: `Random(hi - lo)`, `Random(level*0x19)`,
`Random(level*0x28)`. `below(n: u16)` forces the caller to be explicit.
86 call sites total (42 `entry`, 27 `3d11`, 14 `0d14`, 2 `7c67`, 1 `2526`).

`Randomize` at `1f78:11e0` seeds from DOS `INT 21h/AH=2Ch`; deliberately NOT
ported, seeding is a caller decision. `src/rng.rs` has `Debug`/`Clone` and
`state()`/`set_state()` for Task 9 snapshots.

**Vector provenance is genuinely non-circular.** `tools/gen_rng_vectors.py`
decodes and executes the original's own instruction bytes out of `orig/g.exe`
in a small 8086 interpreter; constants are read from the binary at runtime.
The reviewer proved it by flipping the multiplier byte in a COPY of the exe and
watching the emitted vectors change. Never regenerate these from `src/rng.rs`.

### THE SEEDING ANSWER — Task 12 depends on this

**The game is genuinely clock-seeded and diverges run to run.**
`docs/re/rng.md` was right; `docs/re/oracle.md` was WRONG and is now corrected.

Proven empirically on the owner's protocol: enter a name, then ~50 `w`
commands. Three runs with real wall-clock gaps gave three different
`SCREEN.BIN` captures, diverging at frame 18 of 114, with a different first
enemy each run. A direct probe of `INT 21h/AH=2Ch` confirmed DOSBox-X's guest
clock tracks real host wall time, not a fixed instant.

The old five-run "byte-identical" evidence is still true but was **over-read**:
those scripts stayed inside the deterministic opening and never reached
RNG-dependent output, so the test could not have failed. Owner's domain
knowledge matched exactly — the opening is always the same, variation starts
when the protagonist walks.

**Consequence for Task 12:** the differential harness CANNOT compare raw oracle
output on any RNG-dependent screen without pinning the seed on both sides.
Options: patch the guest to skip `Randomize`; pin the emulated clock; or set
`RandSeed` directly in the guest (now possible — Task 8 recovered both the seed
location and `Randomize`'s formula). Otherwise restrict comparison to
RNG-independent screens, which would exclude combat. This is a real property of
the game, not an emulator artifact to route around.

### Task 7 outcome

`src/save.rs` parses and re-serialises the 694-byte `.SAV` byte-exactly.
`to_bytes` returns `Result` and cannot panic. `display_name()` strips `^N`
markup via `text::strip`; `self.name` keeps the raw sigils for round-trip.

**CP866 in Rust — the constraint was amended.** The original global constraint
said "no CP866 handling anywhere in the Rust crate", which `Save::parse` /
`to_bytes` cannot satisfy: a `.SAV` holds a player-typed name as live CP866
bytes produced at runtime, so no extraction-time conversion can cover it. Owner
narrowed the constraint to game *text*, and signed off `encoding_rs` (owner
prefers a crate to a hand-written table). **Use only the strict APIs:**
`decode_without_bom_handling_and_without_replacement` in, and `new_encoder()` +
`encode_from_utf8_without_replacement` out. `IBM866.encode()` is lossy by
WHATWG mandate — an unmappable char becomes an HTML numeric reference
(verified: `漢` -> the bytes `&#28450;`), which would write a corrupt save that
still round-trips.

**Two claims the reviewer checked rather than accepted, both of which held:**
- `BadCp866Bytes` really is dead code. IBM866's high-byte table has 128 entries,
  none of them 0, and `Malformed` is only returned when `mapped == 0`, so the
  strict decoder cannot return `None` for this encoding. Kept for API honesty.
- The drift guard genuinely compares Rust constants against
  `data/save_layout.json` at runtime, not the JSON against itself.

**Still weak, by design:** the byte round-trip does NOT validate `unk_stat*` or
`tail` offsets — `to_bytes` writes identical values back to the same
self-computed offsets, so it passes whatever those offsets are. Only the drift
guard covers them, and only for Python/Rust *agreement*, not correctness.
Task 9 pins the real semantics.

The fix wave closed a panic: `put_pstring` asserted the 255-byte shortstring
cap, so an over-long name aborted the process (release sets `panic = "abort"`).
Now `SaveError::TooLong(n)`. The cap is on **CP866 bytes**, not `str::len()`
(UTF-8) or char count — a Cyrillic char is 2 UTF-8 bytes but 1 CP866 byte, so
200 Cyrillic chars fit and 256 do not. Both covered.

### Task 6 outcome

Crate is live: `cargo 1.97.1`, deps `serde` + `serde_json` only, `Cargo.lock`
committed. `src/text.rs` is the markup boundary — `parse()` is the single
primitive, `render()` and `strip()` are both built on it. 16 tests, no warnings.

Owner approved amending the plan's own test code: one test's name contradicted
its assertion (`caret_not_followed_by_digit_is_literal` asserted
`strip("2^3") == "2"`, where `^3` IS a valid code), and four edge cases were
untested. Split and covered. Reviewer independently re-derived all four
expected values from `parse`'s control flow — none was fitted to run output.

### Task 5 outcome

`data/save_layout.json` (694 B, schema `{"size", "fields":[{"name","off","kind","len"}]}`)
and `tools/decode_save.py` are in. Task 7 generates the Rust `save.rs` against
that exact schema. `hp` @ `0x210` and `hpmax` @ `0x212` are the only
semantically confirmed words; the eight stat words at `0x200` and the tail
from `0x214` stay `unk_*` until Task 9 pins them from disassembly.

The owner-approved amendment landed correctly: `_check_offsets()` rebuilds the
named regions from decoded values only, never touching `_raw`, and hardcodes
its own `CHK_OFF_*` literals rather than importing them from `decode_save` —
so a wrong offset cannot be self-consistent. The implementer caught that exact
tautology in its own first draft. Reviewer traced per-field that the check
fails for a wrong `magic`, `name`, `stats`, `hp` or `hpmax` offset, not just
the perturbed one. The stats-block slice check is the only thing in the suite
that would catch a wrong `OFF_STATE` at all, since `EXPECT` has no ground
truth for `stats`.

### Task 3 outcome — the oracle works, and how

Headless capture via `-c screenshot`/autotype was a dead end and was replaced.
`g.exe > OUT.TXT` yields 0 bytes (Borland Crt writes straight to VGA text
memory), and dosbox-x `-c` commands only fire when the shell is idle, so
autotype after `-c g.exe` never runs until the game exits.

The harness instead loads a TSR (`tools/oracle/scrhook.asm`/`.com`) that hooks
INT 16h: on every blocking key read it appends the 80x25 text buffer to
SCREEN.BIN and answers the read from a scripted key file. Serving keys from
the handler is what makes it deterministic — the Nth key request gets the Nth
scripted key, so nothing depends on autotype pacing, emulator speed, or the
15-key BIOS buffer, and scripts are not bound by the 127-char DOS command line.

**Consequences later tasks must know:**
- Only input-request screens are captured. A screen the game overwrites
  between two key requests is never seen. Task 9 should script fights so each
  interesting screen is followed by a key request (it naturally is).
- Pass `--expect-frames`. The host stops the run after SCREEN.BIN is quiet for
  3s, which is a wall-clock judgement; a stall truncates the capture and a
  truncated capture is otherwise indistinguishable from a complete one.
  `run_oracle.sh` forwards `--expect-frames` and `--timeout` through to
  `capture.py`.
- Key script limit is 1024 bytes (the TSR's buffer); longer is refused.
- A key request made while DOS is busy (InDOS) is neither captured nor
  answered.
- `data/oracle_prompts.json` is authoritative for which prompt consumes which
  key; the table in `docs/re/oracle.md` is a hand copy.
- Determinism is empirical, not proved: 5 runs across 2 scripts agree byte for
  byte, including an RNG-driven outcome. If a later task sees drift, first
  suspect is the game seeding from the emulated clock.

Three fix waves. Round 1: `--timeout` parsed but never forwarded, no
truncation guard. Round 2: both fixes unreachable from `run_oracle.sh` (the
interface Tasks 8/9/12 actually call), untested, and the prompt/key RE finding
had no `data/` artifact. Round 3: the new tests were anchored at the helper,
not the call site — reverting the guard's wiring line or the shell's `"$@"`
forwarding both left the suite green. **The recurring shape: a fix that is
correct in the code but unreachable or untested at the site that broke.**

Controller ruling: the "citing the Ghidra address" half of the two-places
constraint binds static-disassembly findings. This task's prompt/key finding
is behavioral, recovered by driving the emulator, so it has no address to
cite. Not a gap.

### Task 2b + 2c outcome

`data/strings.json` is 796 entries: 695 pointer-anchored + 54 table + 47
gap-tiled. The truncation the owner caught is fixed — `0xBCDD` reads
`...сломают челюсть)`. The blind scanner is deleted.

### Task 2b outcome

`data/strings.json` is now 749 entries: 695 pointer-anchored + 54 table.
The truncation the owner caught is fixed — `0xBCDD` reads
`...сломают челюсть)`. The blind scanner is gone.

The plan's mid-word-cut check was **structurally broken** and was replaced
(commit `b4d8f14`). Strings are packed with no delimiter, so the byte after
any string is the next string's length byte, and ordinary lengths (48–57,
65–90, 97–122) all land in the "alphanumeric" ranges it tested. Measured 39
false positives on correct data; a same-alphabet-class variant still gave 3.
**No next-byte rule can work here** — do not reintroduce one.

### Three controller errors in this task pair — read before trusting a measurement

1. **A letter-byte condition was added to gap tiling, then reversed**
   (`309f3a4` → `8136bbc`). It rested on "~13% of random windows tile, flat
   across gap lengths 2–40, so tiling is a coin flip." That sample spanned
   `0x18D0`–`0x158F2` and silently included a tail that is **69% NUL**; a run
   of `0x00` is a chain of zero-length strings that tiles at any length. The
   flatness was the artifact announcing itself. Per region, `0x18D0`–`0x11000`
   (where every recovered string lives) tiles at **0.1–1.7%** — for a 2-byte
   gap that is just `P(byte == 0x01)` = 1.64%. **Tiling between two verified
   anchors is strong evidence.** Do not re-add a content filter.
2. **"unaccounted dropped 14 → 1" was claimed as strong evidence. It is not.**
   `test_string_pointers.py` skips `suspect` entries, and 44 of the 47
   gap-tiled are suspect, so that metric is near-self-referential. The real
   figure is **11 of 14 residual offsets covered**, 3 uncovered (`0x42B0`,
   `0x11204`, `0x122EB` — the ones Task 4b independently called blind-scan
   artifacts with no code reference).
3. **"0 tiling violations" only proves the overlap half.** The gap half
   evaluates **0 pairs**, because `gap_tile()` fills exactly the gaps it
   inspects. Tiling also *masks* anchor loss by re-emitting the string from
   the widened gap — dropping 20 real pointers still left 781–791 entries.
   The test now pins three exact counts (796 / 695 / 47) instead of a floor;
   that is what actually detects a lost anchor.

Each of the three was caught by a reviewer, not by the controller. Keep
reviewers explicitly prompted to attack the controller's reasoning, not just
the implementer's.

## Current verified state

All seven suites pass:
```
python3 tools/verify_corpus.py         -> OK 7 corpus files verified
python3 tools/test_extract_strings.py  -> OK 796 strings extracted, 77 flagged suspect
python3 tools/test_string_pointers.py  -> OK 695 string pointers recovered, 3 unaccounted
python3 tools/test_string_tables.py    -> OK 54 table entries extracted
python3 tools/oracle/test_oracle_smoke.py -> OK 8 checks, ~1.75s, 2 dosbox-x launches
python3 tools/test_decode_save.py       -> OK 5 saves decoded and round-tripped
cargo test                             -> ok. 16 unit + 8 integration; 0 failed; 0 warnings
cargo clippy --all-targets             -> clean
cargo test (54 total, all suites green); cargo clippy --all-targets clean
python3 tools/gen_rng_vectors.py       -> reproduces data/rng_vectors.json byte-identically
```

- `data/strings.json` — 796 entries (695 pointer-anchored + 54 table + 47 gap-tiled). Trustworthy; rebuilds byte-identically from the two input artifacts.
- `data/string_pointers.json` — 695 pointer-anchored offsets. Trustworthy.
- `data/string_tables.json` — 11 ranks + 43 крутизна entries. Trustworthy.

**Task 11 must know:** the yes/no confirmation token for the save and quit
prompts is **not** in `data/strings.json`. The suspect-neighbour rule
correctly refuses `0x8D79 'y'`, `0x9BF1 '\'`/`'y'` and `0x9D5E 'w'` because
their anchors (`save_r0.sav`, `save_r`, `run`) are pure ASCII and so get
flagged `suspect`. Recover it from the disassembly; do not assume it is
present. Also: Task 11 must not filter on `suspect` — 44 of the 47 verified
gap-tiled tokens carry `suspect: true`, and the field cannot distinguish
them from byte noise.

## Decisions made (do not relitigate)

- `^0`–`^7` is markup, not content. Parsed into spans; `plain` strips it, `text` keeps it. Raw sigils allowed ONLY inside byte-exact save round-trips.
- Garbage entries are **flagged (`suspect`), never deleted** — deleting destabilises offsets.
- **No frequency/reuse-based filtering** of string candidates, ever. It discarded real text once already.
- Operand extraction uses `getScalar` (immediates only). Never `getOpObjects` — it decomposes `[BP+4]` into false candidates.
- **RNG fallback approved by the owner:** try to recover the original generator; if impossible, use a self-contained PRNG (NOT the `rand` crate), report DONE_WITH_CONCERNS, and delete the vector-comparison tests rather than seed them from our own implementation. This makes Task 12's differential test deterministic-values-only.
- **RNG effort is timeboxed (owner, during Task 8).** The owner does not believe
  the RNG strongly influences this game and does not want significant effort
  spent recreating it. Do not grind the decompilation: if the generator is not
  identifiable with reasonable effort, take fallback option 3 and move on.
  **Exception:** concrete evidence that the RNG materially drives outcomes —
  called from the damage-roll, hit-chance, or loot/price paths in a way that
  dominates results — is worth escalating to the owner before continuing, not
  worth pressing on unilaterally. Task 8's report must record how far recovery
  got and what a future attempt should try first, so this stays resumable if
  the owner later decides fidelity here matters.
- **`rand` was offered by the owner and declined, with the owner deferring to
  this judgement — do not revisit without new information.** `rand`'s `StdRng`
  is explicitly non-portable: its own docs (`rand-0.9.4/src/rngs/std.rs:20-24`)
  say "any future library version may replace the algorithm and results may be
  platform-dependent" and "even with a fixed seed, output is not portable".
  Fallback option 3's entire requirement is same-seed-same-sequence stable
  across builds and platforms, because save files and Task 12's differential
  harness depend on reproducible rolls; a `rand` upgrade could silently change
  every sequence while the tests still passed (they would be comparing our
  output to our output). It also pulls 6 transitive crates for one seeded
  integer sequence. If a crate is ever genuinely wanted here, the correct one
  is `rand_chacha` pinned — the portable variant `rand`'s own docs point to —
  but the 5-line xorshift does the same job with zero dependencies.
- Task 11 must contain NO placeholder handlers or dummy enemies (owner chose rubric over plan).
- **Task 5 amendment (owner-approved, option 2):** the plan's round-trip test
  is near-tautological — `encode()` starts from `rec["_raw"]` and copies the
  tail verbatim, so every unnamed byte round-trips regardless of correctness,
  and a wrong-but-consistent `OFF_HP` would still pass. The plan's comment
  claiming it "proves we account for every one of the 694 bytes" is FALSE and
  must be deleted. Task 5 additionally builds the named regions from the
  decoded fields WITHOUT `_raw` and asserts those bytes match the original.
  The tail stays declared opaque. Reason: Task 7's Rust `save.rs` is generated
  from these offsets; a silently wrong offset propagates into the port.

## Minor findings deferred to the final whole-branch review

- **Task 8** frame-level evidence is asserted, not inspectable. `docs/re/rng.md`
  and `docs/re/oracle.md` cite "frame 18 of 114" and per-run enemy names from
  oracle runs whose raw output was never committed. The load-bearing conclusion
  (three differing `SCREEN.BIN` md5s) is reproducible by re-running; the
  frame-level detail is not. Next empirical check of this kind should commit the
  divergence excerpt to `docs/re/` or `data/`.
- **Task 8** `src/rng.rs:53` doc comment on `below` mentions only
  `below(0) == 0`, not `below(1) == 0`, though the test pins both.

## Minor findings deferred to the final whole-branch review

- **Task 9** the 12 unmapped `Random` call sites live only in `docs/re/combat.md`
  with no `data/` counterpart — the one place the "two places" rule is unmet.
  Defensible (they are open questions, not findings) but worth a decision.
- **Task 9** `src/save.rs` keeps `stats: [u16; 8]` with meanings in a doc
  comment; Task 11 will index it with bare literals. Named constants or
  accessors would age better now that the fields have real names.
- **Task 9** blow-index coverage is thin above index 1: 40 cases reach index >= 1,
  8 reach >= 2, 2 reach index 4.

## QUEUED — dispatch as soon as the Task 10 fix wave lands

**Split RE provenance out of the runtime data artifacts (owner-directed).**

The provenance addresses are currently fields on the RUNTIME structs in
`src/data.rs`, so Task 11's gameplay code would see them:
`Item.src_off`, `Item.price_src`, `ShopEntry.price_addr`,
`ShopEntry.displayed_price_addr`, `ShopEntry.code_addr`, `Enemy.source`.
The `String`s also allocate on every parse.

Fix: `data/{items,shops,enemies}.json` keep only what the game needs; a sibling
`data/*.provenance.json` keyed by `id` carries the addresses. Only the runtime
files get `include_str!`'d in `src/data.rs`. The "two places" rule is still met
— provenance stays a machine-readable `data/` artifact, just not one compiled
into the game binary. `tools/extract_tables.py` emits both; artifacts must
still regenerate byte-identically.

Do it BEFORE Task 11 consumes the API — same reasoning as the `class` move.
Apply the same principle when Task 11 embeds `data/strings.json` (133K): the
game needs the text, not the `suspect` flag or the RE offsets.

## Carried forward — Task 11 must handle

- **The game runs from `g.exe` alone — VERIFIED empirically, not assumed.**
  Staged a corpus containing only `g.exe` (no `.SAV`, no `places.sav`) and ran
  it under the oracle: full intro, character creation, and `w` responds
  normally. `places.sav` is NOT a precondition — it is CREATED BY the save
  routine, alongside the character save. An earlier note here claimed Task 11
  must create it when absent; that was wrong.

  Save paths, from `data/strings.json`:
  - `0x8d62 Ты хочешь сохраниться?` -> writes `0x8d7b save_r0.sav` AND
    `0x8d87 places.sav` (the Рушель Блаво service, the `district*50` charge at
    `1000:761d`).
  - `0x9bcd Хочешь сохранить свои достижения?` -> writes `save_r<N>.sav`.
  - `0x7c3b Можно начать с того места где ты сохранился` — load is offered only
    when a save exists.

- **Still open: `Save` cannot CREATE a save.** `src/save.rs` exposes only
  `Save::parse(bytes)`, and `to_bytes()` starts from `self.raw.clone()` — a
  pre-existing 694-byte image. Right for round-trip fidelity (Borland never
  clears shortstring padding) but it leaves no path to write a save for a
  character that never loaded one, which is the normal new-game case.

  **Do not invent the missing bytes.** `unk_0214` (29 B) and `unk_02ae` (8 B)
  are genuinely unknown, and "save file bytes must match the original exactly"
  is a top-level constraint. Capture what the original writes: new character,
  earn the save fee, save, extract both files, commit as a `data/` artifact,
  and have `Save::new_game(...)` start from those real bytes.

- **Saving is CHECKPOINT-ONLY — there is no `sv` command (owner).** Saving
  happens at specific locations, not by typing a verb. Consistent with what was
  observed: `sv` drew no response in a live oracle run, and both save strings
  are location-bound (`0x8d62` the Рушель Блаво service / `district*50` charge
  at `1000:761d`; `0x9bcd` a second checkpoint path).

  **The plan's "Reference facts" command-verb list includes `sv` and is
  therefore suspect.** That same table asserted the RNG multiplier was absent,
  which Task 8 proved was a false conclusion drawn from true observations.
  **Task 11 must derive the command table from the disassembly, not from that
  list**, and treat every verb in it as unverified until checked.
  Reaching a save checkpoint is what the new-character template capture above
  requires — plan for a location, not a command.

## Carried forward — Task 11 (rendering / print orchestration) must decide this

**Trailing colour codes are irrecoverably dropped by `parse()`.** In the
original, a Borland Crt colour directive sets terminal state that persists into
whatever is printed *next*. Our `Span` model cannot represent "colour is now
active, no text yet": the post-loop flush in `src/text.rs` is gated on
`!buf.is_empty()`, so `parse("abc^4")` returns exactly `[Span{None,"abc"}]` —
the `^4` leaves no trace at all — and `parse("^4^7abc")` returns a single
White span, silently discarding the Red. `render()` also emits `\x1b[0m` at the
end of every string.

Not a Task 6 bug: the brief specified a per-string primitive and that is what
was built. But the information is gone by the time `parse` returns, so this
CANNOT be patched downstream from a `Vec<Span>` — fixing it means changing
`parse`'s output shape (e.g. an explicit trailing-colour field).

Task 11 must check the disassembly for whether a game string ever ends in a
colour code intended to tint the following output. If yes, `Span`/`parse` grow
a slot for it. The plan's fidelity constraint covers colour index, so this is
in scope for fidelity, not cosmetics.

## Minor findings deferred to the final whole-branch review

Triage these before merge; none blocked their task.

- **Task 5** `tools/test_decode_save.py` — no test exercises `decode()`'s
  wrong-length `ValueError` guard. The guard is correct; nothing calls it with
  a non-694-byte blob.
- **Task 5** `tools/decode_save.py` `encode()` — `buf[OFF_TAIL:] = rec["tail"]`
  and the `stats` loop have no length assertions. `bytearray` slice assignment
  silently resizes, so a caller building a `rec` with a wrong-length `tail` or
  `stats` of length != 8 gets a silently corrupted file instead of an error.
  Latent (tail/stats currently always come from a same-length `decode()`), but
  Task 7 generates Rust from this path.

## Known open items

- ~~14 blind-scan strings unrecovered by pointer anchoring~~ — **resolved by Task 2c.** The tiling check found them: 37 of 39 letter-bearing gaps tile exactly as complete Pascal shortstrings, and they are the game's command tokens (`s`, `sv`, `e`, `v`, `f`, `k`, `y`, `\`, `1`–`4`) plus a `С^ У^ П^ Е^` split banner. Task 4b's `N>=3` Cyrillic floor had excluded them. Task 11 compares input against these. The 2 non-tiling gaps sit between `suspect` entries and are code bytes — hence the check skips gaps beside suspect anchors.
- ~~whether headless capture works~~ — **resolved by Task 3.** It does; see the Task 3 outcome above for the mechanism and its limits.
- Accepted residual risk (Task 3, reviewer-flagged Minor, no action): `test_run_wires_frame_count_guard` patches three `capture.py` internals (`subprocess.Popen`, `_wait`, `decode_frames`) to reach `run()` without an emulator, so a harmless refactor of `run()` can break it. Proportionate given the no-second-emulator-launch constraint. If a fourth wave of stub-anchored tests is ever needed, reconsider injection seams instead.

## Workflow commands

```bash
SKILL=/home/finkel/.claude/plugins/cache/claude-plugins-official/superpowers/6.1.0/skills/subagent-driven-development
"$SKILL/scripts/task-brief"     docs/superpowers/plans/2026-08-17-gopnik-rust-port.md <N>
"$SKILL/scripts/review-package" <BASE_SHA> HEAD
```
Commit as: `git -c user.name="Claude" -c user.email="noreply@anthropic.com" commit`

NOTE: `.superpowers/` is git-ignored, so this ledger is NOT committed and
`git clean -fdx` would destroy it. `docs/superpowers/RESUME.md` is the
committed backup.
