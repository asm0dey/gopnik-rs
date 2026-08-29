# Known gaps in the port

The list of things the port does **not** reproduce, and why. Source comments
cite this file by section.

Each entry states its evidence tier per `docs/re/METHODOLOGY.md`:
**established from flow** (with an address), **corroborated** (by state or
output, and by what), or **unverified** (and what would settle it). Every
address below was re-derived from `orig/g.exe`; a `mov di,<n>` / `push cs`
string operand names the string whose file offset is what `1000:<n>` resolves
to. `docs/re/METHODOLOGY.md`, "Address convention, and its range of validity", is the authority for the rule; `tools/addr.py` is its executable form and `python3 tools/re_query.py resolve <citation>` checks any single address against the bytes.

---

## Discovery flags: the complete store inventory

*Cited from `src/game.rs`'s `enter_shop` and `Game::new`.*

The seven discovery flags are seven contiguous bytes at `20ae:3694..369a`
(`docs/re/command-dispatch.md`, "Discovery gates"). Scanning `orig/g.exe` for
`c6 06 [94-9a] 36 imm8` (`mov byte [0x36??],imm8`) yields **31** stores:
**14 clears** and **17 set-to-1**. The clears are the two block resets —
`1000:6d3b`..`1000:6d6e` (the `places.sav` load-failure arm) and
`1000:ab96`..`1000:abc9` (`reset_for_new_district`). All seventeen
**immediate** setters (`mov byte [0x36??],1`, this exact encoding) are
below; **established from flow** (the scan is byte-exact and the encoding is
fixed-length, so it cannot miss a store of this form). A separate, non-scanned
form also sets all seven flags: the `places.sav` reader at
`1000:6ca2`..`1000:6d0e`, seven `call 0f78:081e` (`rtl_file_read`, INT 21h
AH=3Fh per `docs/re/rtl.md:545`) with `DS:0x369X` pushed, one per flag,
restoring them from the save file. It is inventoried below, in
"`PLACES.SAV`'s byte order — settled" (`gaps.md:465`), not in this scan —
`rtl_file_read` writes through a pointer, not an immediate.

An earlier revision of this section claimed the same scan "finds every store to
them", then listed twelve of the seventeen and said "**Two** further stores"
while naming five addresses. That is the "evidence that proves less than it
claims" failure `docs/re/METHODOLOGY.md` exists to stop; the count and the
inventory are now stated together.

| setter | flag | location | trigger | tier | in the port? |
|---|---|---|---|---|---|
| `1000:6dc3` | `0x3698` | Vet | character creation, `1000:6dbe` | flow | **yes** — `Game::new` |
| `1000:6dc8` | `0x3694` | Market | character creation, `1000:6dbe` | flow | **yes** — `Game::new` |
| `1000:b196` | `0x3698` | Vet | wander preamble, `Random(10)` at `1000:b186` | flow | **yes** (Task 11c) — `Game::wander_preamble` |
| `1000:b1c8` | `0x3694` | Market | wander preamble, `Random(10)` at `1000:b1b8` | flow | **yes** (Task 11c) — `Game::wander_preamble` |
| `1000:b1fa` | `0x3699` | Club | wander preamble, `Random(100)` at `1000:b1ea` | flow | **yes** (Task 11c) — `Game::wander_preamble` |
| `1000:b22c` | `0x369a` | Gym | wander preamble, `Random(100)` at `1000:b21c` | flow | **yes** (Task 11c) — `Game::wander_preamble` |
| `1000:b570` | `0x3697` | Girl | wander bucket 2 | flow | **yes** — `Game::wander_girl` |
| `1000:d751` | `0x3699` | Club | `girl`'s own reveal | flow | **yes** — `Game::visit_girl` |
| `1000:73c3` | `0x3696` | Den | `[0x389c] == 5` at `1000:73bb` | flow | **yes** (Task 11c) — `Game::apply_class_bonus` |
| `1000:73cf` | `0x3697` | Girl | `[0x389c] == 3` at `1000:73bb` | flow | **yes** (Task 11c) — `Game::apply_class_bonus` |
| `1000:73d4` | `0x3699` | Club | `[0x389c] == 3` at `1000:73bb` | flow | **yes** (Task 11c) — `Game::apply_class_bonus` |
| `1000:73e0` | `0x3695` | Dealers | `[0x389c] == 6` at `1000:73bb` | flow | **yes** (Task 11c) — `Game::apply_class_bonus` |
| `1000:dcf6` | `0x3695` | Dealers | the `a` token at `1000:dcef` | flow | **yes** (Task 20) — `Game::den_reveal` |
| `1000:dcfb` | `0x369a` | Gym | the `a` token at `1000:dcef` | flow | **yes** (Task 20) — `Game::den_reveal` |
| `1000:ae1f` | `0x3696` | Den | the chapter-5 endgame arm at `1000:adbf` | flow | **yes** (Task 20) — `Game::enter_district_5` |
| `1000:4aa5` | `0x3696` | Den | the de-level (flee) penalty, `1000:4a87`/`1000:4aa0` | flow | **yes** — `Game::flee_penalty` |
| `1000:52b3` | `0x3696` | Den | the post-kill block, `1000:5295`/`1000:52b1` | flow | **yes** — `Game::claim_spoils` |

All three Den triggers were **closed by Task 11b** — see `docs/re/wander.md`,
"The three Den setters". `1000:4aa5`'s store and the line it prints contradict
each other in the original; that is recorded there, not resolved.

**A revision of this section carried between Task 11b and Task 20 was
itself wrong about the last two rows.** `1000:4aa5` and `1000:52b3` were
marked `no` here even though both had already been ported — `1000:4aa5` in
`Game::flee_penalty` (the `class != 5 && level - (district-1)*10 == 3` arm,
storing the den flag right before `^4Такого конявого непустят в местный
притон!`) and `1000:52b3` in `Game::claim_spoils` (the
`!is_found(Den) && level - (district-1)*10 >= 3` arm). Both were confirmed
against the shipped `src/game.rs` at the start of Task 20, before either was
touched again. The table now says which won: the code, not the inventory
that disagreed with it.

Task 20 closed the two rows that were genuinely open: the `a` token
(`1000:dcf6`, `1000:dcfb`, both stores unconditional once reached, in
`Game::den_reveal`) and the chapter-5 endgame's own flag store and Den grant
(`1000:ae1f`, in `Game::enter_district_5`, reached the turn `district` first
becomes 5). The four calls that same endgame arm makes afterward
(`FUN_1000_11c2` twice, `FUN_1000_3d11` twice, for the rector and final-boss
fights) are deliberately NOT ported — see `Game::enter_district_5`'s doc
comment in `src/game.rs` for why, and "The district-advance autosave —
wired (Task 21)" below. Task 21 narrowed this twice: the arm's own store
and Den grant are not the divergence (the flag store runs once in the
original too, and the Den grant is idempotent), and the four calls are —
they run on **every** turn there, via `1000:ae18`, while the port runs none.
The one reason that survives is `FUN_1000_3d11`'s untraced `param_1`.

**All seventeen immediate setters are now in the port.** Market and Vet from
character creation and from the wander preamble's draws 6 and 5; Club from
`girl`, from the class-3 bonus and from draw 7; Gym from draw 8 and from the
den's `a` reveal; Girl from the wander bucket and the class-3 bonus; Den from
the class-5 bonus, the flee penalty, the post-kill block and the chapter-5
endgame arm; Dealers from the class-6 bonus and from the den's `a` reveal.
**Zero** of the seventeen immediate setters remain unimplemented: the table
has no `no` rows left. 17 `yes` + 0 `no` = 17. (An earlier revision said
"Five" while naming the same five addresses this correction closes; before
that, an earlier one still said "Six" naming the same five; before that, the
earliest recorded revision — the intro paragraph above — said "Two" while
itself naming five. The count and the inventory are stated together each
time specifically so the next reader does not have to trust either alone.)
This does not cover the `places.sav` reader's seven `rtl_file_read` restores
(above) — a separate, already-ported path, inventoried in "`PLACES.SAV`'s
byte order — settled" (`gaps.md:465`), not a store this table's scan could
see.

### Character creation grants Vet and Market — `1000:6dbe`

**Established from flow.** `1000:6dbe` writes `[0x3692] := 1` (district),
`1000:6dc3` writes Vet and `1000:6dc8` writes Market, three consecutive
five-byte stores. Three paths reach the block and all three write all three
bytes: `1000:6b3a` (the `save_r?.sav` scan at `1000:6a62`..`1000:6ab9` found
nothing — **the path a fresh run with no `.SAV` files takes**, and it prints
nothing), `1000:6b81` (the slot prompt at `1000:6b51` read a key that is none
of `'0'`,`'2'`..`'5'`, i.e. "начать сначала"), and `1000:6bdd` (`IOResult`
non-zero at `1000:6bd4`, via `1000:6da5`, which prints file `0x7D21`).

The `places.sav` reader's own failure arm (`1000:6d3b`) does **not** reach
`1000:6dbe`; it clears flags and leaves at `1000:6da0`.

### The `[0x389c]` progression reveals — `1000:73bb`..`1000:73e0`

**Established from flow**, contrary to an earlier "not yet traced to a trigger
/ unverified" tiering. `1000:73bb` `a1 9c 38` `mov ax,[0x389c]`, then:

```text
73be  cmp ax,5   / 73c1 jnz 0x73ca / 73c3  [0x3696] := 1   (Den)
73ca  cmp ax,3   / 73cd jnz 0x73db / 73cf  [0x3697] := 1   (Girl)
                                    73d4  [0x3699] := 1   (Club)
73db  cmp ax,6   / 73de jnz 0x73e5 / 73e0  [0x3695] := 1   (Dealers)
73e5  mov byte [0x3e35],5
```

**Closed by Task 11b, implemented by Task 11c** (`Game::apply_class_bonus`,
called from `Game::new`). `[0x389c]` is the character class, written only at
`1000:6fed`, `1000:6ffc`, `1000:712a`, `1000:713d` and `1000:71b8` (plus the
694-byte record `BlockRead` at `1000:6c01`), and these four stores are the
class bonuses the creation menu advertises. `1000:73bb` is reached on **every**
entry into the game — new character and loaded save alike, both converging on
`1000:7262` — so the bonuses are re-applied each time. Full derivation and the
complete write inventory: `docs/re/wander.md`, "`[20ae:389c]` is the character
class".

### The `a` token — `1000:dce5`..`1000:dcfb`

**Established from flow.** Not an untraceable path: it is a typed word.

```text
dcba  cmp byte [0x3695],0 / 74 07  ; already-have check: Dealers and
dcc1  cmp byte [0x369a],0 / 75 6a  ;   Gym both set -> skip to 0xdd32
dcc8..dcdc                         ; ax := ([0x38a6] - ([0x3692]-1)*10)*2 + [0x38cb]
dce0  cmp ax,0x28 / 7c 4d          ; < 40 -> skip
dce5  push ds:0x3a72               ; the line just typed
dcea  mov di,0x9fc9 / push cs      ; file 0xB899 = the single character 'a'
dcef  call 0f78:0bd8 / 75 3c       ; string compare; not equal -> skip
dcf6  [0x3695] := 1                ; Dealers
dcfb  [0x369a] := 1                ; Gym
```

`DS:3a72` is the same submenu input buffer `mar` reads into (`1000:bd21`).
**Closed by Task 11b**: the read that leaves the token there is the den's own
`ReadLn` at `1000:db00`..`1000:db09` — the only `0f78:06c6` call between
`1000:d802` and `1000:dd48` — so `a` is typed at the `^0Притон\` prompt, not at
the top level. `[0x38cb]` is a street-cred counter distinct from the level
(`1000:5291` grows it per kill, `1000:db9b` spends it, `1000:dc79` prints it).
See `docs/re/wander.md`. Still not implemented here.

### Wander preamble (`1000:aea1`..`1000:b3b9`) — CLOSED by Task 11c

*Cited from `src/game.rs`'s `Game::wander_preamble`.*

This section used to read "not reproduced". **It is now implemented**:
`Game::wander_preamble` walks `data/wander.json`'s `steps` array in execution
order — all fourteen catalogued `Random` sites, the state steps between them
(the joint-buff decay, the den's loan credit, the dealers' delivery counter,
the two cooldown decrements, the ring's regen, the class-perk dispatch), the
church at `1000:7c67` with its own draws 15–18, and the mage at `1000:7538`.

**The headline divergence this closes.** `Game::walk` used to spend **one**
draw (the bucket roll) where the original spends **nine** in steady state, and
the bucket roll is draw 12 of 14, so the port's RNG stream desynchronised from
the original's on the very first walk and never recovered. It no longer does.

**How that is checked.** `tests/wander_sequence.rs` replays all five captured
runs in `data/rng_trace.json` — the original itself, under qemu, with
`RandSeed` pinned — asserting the port's `(call site, n, result)` sequence
equals the capture's for the whole run. Runs **C** and **D** (3 walks each, 30
and 29 draws, the church firing on turn 1 on two different arms) replay
**exactly**, and their 29-variable `final_state` agrees field for field.
Runs **A**, **B** and **E** replay exactly up to their first bucket-3
encounter and then diverge — see the next section, which is the reason.

The four discovery rolls, unchanged and still established from flow:

| roll | gate | setter | string |
|---|---|---|---|
| `1000:b186` `Random(10)` | `1000:b18f` `cmp byte [0x3698],0` | `1000:b196` `[0x3698] := 1` (Vet) | file `0x9F8B` |
| `1000:b1b8` `Random(10)` | `1000:b1c1` `cmp byte [0x3694],0` | `1000:b1c8` `[0x3694] := 1` (Market) | file `0x9FB2` |
| `1000:b1ea` `Random(100)` | `1000:b1f3` `cmp byte [0x3699],0` | `1000:b1fa` `[0x3699] := 1` (Club) | file `0x9FC4` |
| `1000:b21c` `Random(100)` | `1000:b225` `cmp byte [0x369a],0` | `1000:b22c` `[0x369a] := 1` (Gym) | file `0x9FFE` |

Each *effect* fires when its roll returns `0` and its flag is still clear; the
**roll itself is unconditional**, which is the part a "one-shot event" reading
gets wrong.

Two more shapes worth restating because they are easy to lose:

* **Draws 1 and 2 are not one-shots.** `1000:af71` / `1000:afd0` write the
  never-repeat flag *after* the `or ax,ax / jnz` at `1000:af6d` / `1000:afcc`,
  so the flag is set only by the 1-in-20 roll that returns `0`; until then the
  draw fires every turn. Nine draws per turn decays to eight, then seven.
* **The church cancels the turn it fires on.** `1000:8282` zeroes the
  already-rolled bucket on every path out of `1000:7c67`. Run C's turn 1 is
  the live proof: its bucket roll is `9` (bucket 3, a fight) and the capture
  shows no enemy-generation draws on that turn at all.

### The two draws after the bucket roll — `1000:b39e`, `1000:b3ae` — CLOSED

*Cited from `src/game.rs`'s `Game::church` and `Game::mage`.*

Both are now spent, and both callees are implemented:

```text
b39a  b8 c8 00        mov ax,0xc8      ; 200
b39e  9a 4b 11 78 0f  call Random
b3a5  75 03           jnz 0xb3aa
b3a7  e8 bd c8        call 0x7c67      ; the church
b3aa  b8 64 00        mov ax,0x64      ; 100
b3ae  9a 4b 11 78 0f  call Random
b3b5  75 03           jnz 0xb3ba
b3b7  e8 7e c1        call 0x7538      ; the mage
```

The church (`Game::church`) spends `Random(5)` at `1000:7f63` unconditionally,
`Random(4)` at `1000:7fff` on the `== 1` arm, and two `Random(weight_sum)`
draws at `1000:25fe` on the `== 0` arm (which sets `xp := threshold` at
`1000:7fe4`/`1000:7fe7` and calls the level-up routine at `1000:7fed`, whose
inner loop bound at `1000:287d` is exactly 2). The mage (`Game::mage`) spends
no draw but consumes a line, and charges `district * 50` while printing
`district * 25` — the original's own divergence, reproduced.

**Still not reproduced inside those two:** the church's two long sermons (the
`== 0` and `== 1` stage arms, `1000:7cf5`.. and `1000:7dd5`..) and the
old/new rank names its level-up arm prints from the `DS:0b42` 256-byte-stride
table. Both are text; neither costs a draw. ~~and the mage's two file writes
on the paid path~~ — **Task 19 implemented those**: `save_r0.sav`
(`1000:764e`/`1000:765d`), `places.sav` (`1000:766f`..`1000:7724`) and
`^0Сохранено! ^1Можешь беспредельничать дальше.` (`1000:7729`) are all in
`Game::mage` now, via `crate::persist::Game::mage_save`.

### The random-encounter opponent — `FUN_1000_0d14` — CLOSED (Task 11f)

*Cited from `src/game.rs`'s `Game::walk`, `Game::roll_enemy` and
`Game::cop_encounter`.*

This was the largest known divergence in the port and the reason three of the
five captured runs did not replay. **It is closed.** All five runs of
`data/rng_trace.json` now replay their whole draw stream — 1387 draws — and
all five also match their whole 29-variable `final_state`.

**Established from flow.** `1000:0d14`..`1000:11bf` (file
`0x25e4`..`0x2a8f`) was disassembled with
`ndisasm -b16 -o 0xd14 -e 0x25e4 orig/g.exe`, i.e. from the routine's own
`55` / `89 e5` entry, so every address below sits on a confirmed instruction
boundary; each of the fourteen `Random` sites carries the `9a 4b 11 78 0f`
signature at the address named. `1000:b5b8` calls it with `param_1 = 0`
(`b0 00` / `50` at `1000:b5b5`); `1000:c3d0`, `1000:dc0e` and `1000:e181`
pass 1 and `1000:ddf6` passes 2, which selects only the two extra clamps at
`1000:0da7`/`1000:0dba`.

| site | what it draws | `n` | stops |
|---|---|---|---|
| `1000:0d26` | class seed, folded by the triangular walk at `1000:0d2f`..`1000:0d68` | 51 | 13 |
| `1000:0d70` | class += `Random(district)` | district | 13 |
| `1000:0d91` | class += `Random(4)`, **only** when `[0x3693]` is set (`1000:0d86`) | 4 | 5 |
| `1000:0dcc` | крутизна += `4 * Random(district)` (`shl ax,1` twice at `1000:0dd1`) | district | 13 |
| `1000:0ddd` | the `+ s` term of the крутизна expression | 5 | 13 |
| `1000:0df0` | its **divisor**, `+ 1` | 2 | 13 |
| `1000:0e04` | its **multiplier**, `+ 1` | 2 | 13 |
| `1000:0efd` | one stat point, bucketed against the weight row | Σ weights | 348 |
| `1000:102e` | Хлам's flat term | 6 | 13 |
| `1000:109c` | Хлам's spread | `k` | 13 |
| `1000:10c4` | money's flat term | 6 | 13 |
| `1000:113c` | money's spread | `k` | 13 |
| `1000:1162` | beer | 2 | 13 |
| `1000:1197` | armour | `2 * (district − 1)²` | 13 |

The recovered routine, in order:

1. **Class.** `Random(0x33) + 1` (`1000:0d26`), then a triangular walk: for
   `i` in `1..=10`, if the running value goes negative on subtracting `i` the
   class becomes `10 − i` and the walk stops, otherwise `i` is subtracted
   (`1000:0d64` `cmp byte [bp-1],0x0a` / `jnz 0xd35` bounds it). 51 cannot
   survive all ten subtractions — they total 55 — so the walk always leaves
   through the break. The fold **inverts** the roll: `Random(0x33)` of 0–1
   gives class 8, 44–50 gives class 0. Then `+ Random(district)`, then
   `+ Random(4)` when `[0x3693]` is set, then clamp to 9.
2. **Крутизна** (`1000:0dc6`..`1000:0e76`).
   `Round(player_level * f / d + s − 2) + 4 * Random(district)`, floored at 0
   (`1000:0e48`), and then **multiplied by 1.5** (`1000:0e6c`) when
   `[0x3693]` is set. `s` is `Random(5)`, `f` is `Random(2) + 1` at
   `1000:0e04` and `d` is `Random(2) + 1` at `1000:0df0` — that assignment is
   not cosmetic: `1000:0dfd` pushes the `1000:0df0` value as a real which
   `1000:0e1e` pops back into `cx:si:di`, the divisor operand of `0f78:1117`,
   while the `1000:0e04` value stays in `cx:bx` for the `0f78:09d2` multiply
   against `[0x38a6]`. `0f78:1117` is the divide, not the multiply: it is the
   entry that tests `cl` for a zero divisor and raises runtime error 200
   (`0f78:1117` `0a c9` / `74 2a`); `0f78:1111` is the multiply.
3. **Stats.** The four are zeroed (`1000:0e79`..`1000:0e8a`), then
   `Σ weights + крутизна * 2` points are distributed, each
   `Random(Σ weights) + 1` bucketed against the running prefix sums of the
   class's weight row at `20ae:0002 + class*4` — `progress::CLASS_WEIGHTS`.
   Both the sum and the point count are stored as **bytes** (`1000:0ed1`,
   `1000:0ee2`).
4. **Derived** (`1000:0ff3`..`1000:101d`): `dmg_min = strength div 2`,
   `dmg_max = strength`, `hpmax = vitality * 5 + strength + 10`, `hp = hpmax`.
5. **Loot** (`1000:102a`..`1000:1181`). With
   `k = крутизна div 2 + Round(class * крутизна / 5)`, recomputed from
   scratch each time it is needed (`1000:1037`, `1000:106a`, `1000:10cd`,
   `1000:110a` are all `mov ax,[0x3952]` / `mul word [0x395c]`):
   Хлам `[0x396e] = max(0, Random(6) + 2 * Random(k) − k)`, money
   `[0x396c] = max(0, Random(6) + Random(k) − k div 2)`, beer
   `[0x396a] = Random(2) + крутизна div 10 + 1`. `1000:523e`..`1000:5251`,
   the victory block of `FUN_1000_3d11`, is what names them: they are added
   into `[0x38c3]` (beer), `[0x38c7]` (money) and `[0x38c9]` (Хлам).
6. **Armour** (`1000:1184`..`1000:11b9`): `Random(b) + b` stored as a byte,
   `b = 2 * (district − 1)²`. District 1 therefore always draws `Random(0)`,
   which returns 0 — the draw still happens, which is why `1000:1197` has 13
   stops and not 3.

`Round` is Borland's, and it is **half away from zero**: `0f78:1131` sets
`ch = 1` and calls `0f78:1091`, whose `0f78:10d4` `add bh,bh` sets CF when the
byte shifted out of the mantissa is ≥ `0x80` and whose `adc ax,0` / `adc dx,0`
add that carry into the *magnitude*, before the sign is applied at
`0f78:10e4`. It matters exactly once, at step 2's `× 1.5`: run A turn 11 of
`data/rng_trace.json` rolls крутизна 1 there and then spends **12** draws at
`1000:0efd` rather than 10, which only happens if `Round(1.5) = 2`.

#### Two corrections this recovery forces

* **`docs/re/tables.md`'s sketch had the `1000:0efd` loop drawing
  `Random(remaining points)`.** It does not: the `n` is the constant
  weight-row sum. The capture's own `n` set at that site is exactly
  `{6, 8, 9, 12, 20, 22}`, which is the six distinct weight-row sums of
  classes 0..9 (class 7's 14 never came up), not a decreasing remainder.
* **`20ae:3693` is not flavour.** `docs/re/wander.md` and this file both
  described bucket 1's toggle as having no reader. `FUN_1000_0d14` reads it
  twice — `1000:0d86` and `1000:0e54` — so it changes both the draw *count*
  and the draw *values* of every later encounter. The port carries it as
  `Game::flag_3693`. **There is a third reader, found in Task 17:**
  `1000:4ebc`, the in-combat pistol arm, where the flag (or the silencer
  `20ae:394e`) is what allows a shot at all. `python3 tools/re_query.py
  xrefs-to 20ae:3693` accepts **seven** references in total — `1000:b3c4` /
  `1000:b3ce` (the toggle), `1000:b3d1` and `1000:b45b` in `entry`,
  `1000:0d86` and `1000:0e54`, and `1000:4ebc`. Saying "the readers are
  `1000:0d86` and `1000:0e54`" was a completeness claim that stopped the next
  search, in the very entry written to correct one.

#### The fight flow around it

**Established from flow**, disassembled forward from `1000:b353` (the
`9a 4b 11 78 0f` at file `0xcc23`):

* `1000:b5c0` `cmp word [0x3952],8` / `jnz 0xb5ca` — a rolled `Мент` takes
  `1000:b76a` instead, which **asks no question**: it writes
  `^6Идет ментяра # уровня гроза гопов.` (file `0xA2DB`), draws
  `Random(district * 7 + 15)` at `1000:b792`, and compares luck against it.
  Luck wins → `^2Ты затаился…` (file `0xA300`) and no fight. Luck loses with
  `[0x38b3]` (тёмные очки) set → files `0xA33C`/`0xA38A` and no fight. Luck
  loses without them → `^4Запалил!` (file `0xA3B2`) and `1000:b81a` sets the
  accept flag, so `1000:b829` calls `FUN_1000_3d11(0)` outright.
* `1000:b5ed`/`1000:b5f1` — the ordinary encounter's notice roll, the same
  `district * 7 + 15`, but **halved** when `[0x38bc]` (зоновская наколка) is
  set (`1000:b5da` `cmp byte [0x38bc],1`). The cop's roll has no such branch,
  which is why the capture shows `n` 18 and 22 at `1000:b5f1` but 22 and 36
  at `1000:b792`.
* `1000:b5fc`..`1000:b61b` — the branch that was "untraced". It compares luck
  against the notice roll as a longint (`cwd`, `cmp dx,bx`, `cmp ax,cx`,
  `jnc 0xb614`) and then applies a class threshold that **differs between the
  two arms**: 3 when luck lost (`1000:b60a`), 7 when luck won (`1000:b614`).
  Meeting it selects the aggressive block at `1000:b6a0` (file `0xA28A`,
  ` # уровня, ищущий кого отпинать. Хочешь наехать?`, decline roll
  `Random(2)` at `1000:b725`); otherwise the quiet block at `1000:b61e`
  (file `0xA26F`, ` # уровня. Хочешь наехать?`, **no** decline roll).
* `1000:48dc` — combat's own `run` token compare (file `0x4C8B`), reached by
  the cop fight. Its level-0 arm at `1000:4ade` writes file `0x4D6F` and
  leaves the fight. **No arm of the flee path draws**: there is no
  `9a 4b 11 78 0f` anywhere in `1000:48eb`..`1000:4afb`, which is why run A's
  turn 7 — a cop fight entered and fled — shows zero draws between
  `1000:b792` and the next turn's `1000:af68`.

#### What is still not modelled inside the fight flow

* ~~`1000:493b`..`1000:4adc`, the **level > 0** arm of `run`: this port
  carries no growth log, so it prints the arm's line (file `0x4CEF`) and
  leaves the fight without applying the penalty.~~ **CLOSED by Task 18 — the
  penalty is applied.** `crate::progress::Progress::growth_log` is the
  `array[1..40] of string[2]` at `.SAV 0x236`, reached through Borland's
  biased base `20ae:38cf` (real base `20ae:38d2`);
  `crate::progress::apply_levels` appends to it,
  `crate::progress::undo_growth` walks it *forward* over positions 1 and 2
  without consulting the copied length byte (`1000:4982` / `1000:4989` /
  `1000:4a6f`), `crate::progress::demote` is `1000:4ac3`..`1000:4ad9`, and
  `Game::flee_penalty` holds the middle block — the class-5 skip
  (`1000:4a87`), the equality test `level - (district-1)*10 == 3`
  (`1000:4aa0` `cmp ax,3` / `jnz`, where the post-kill twin at `1000:52ae`
  uses `jl`), and `1000:4aa5`'s backwards den-flag store, reproduced as
  written. `docs/re/combat-dispatch.md` has the per-code table this entry
  used to point at, including that code `'1'` also decrements `dmg_min` at
  `1000:49c6`, but only when the NEW strength is odd — the exact inverse of
  `1000:2683`'s grant, which fires when it is even.

  Still true, and still worth knowing: the arm **costs no draw**, and **no
  captured run reaches it** — run A's cop fight and all five of run D's are
  fled at level 0, and run E of `data/rng_trace.json` loads `SAVE_R3` at
  level 20 but never flees. So the two replays confirm the penalty causes no
  regression; they do not exercise it. What does is
  `tests/progression.rs`'s round trip against `data/xp.json`'s captured
  `gains_announced`: replay each level-up through `grant`, log it, flee it
  back, and land on the record the original held before the kill.
* `1000:48eb`'s `[0x3c83] == 1` arm (file `0x4C8F`, the rector refusing to
  let you run). Nothing in this port sets `[0x3c83]`, so the arm is
  unreachable rather than wrong.
* ~~The three loot words are rolled and stored on the returned `Fighter`
  (`beer_dl`, `money`, `junk`) but **not awarded** on victory.~~ **Closed by
  Task 13.** `Game::claim_spoils` reproduces `1000:523e`..`1000:5251` (and the
  whole victory block after it), and `data/combat_trace.json`'s run B — six
  fights, all won, `SAVE_R2` loaded — asserts the resulting `20ae:38c3`,
  `20ae:38c7` and `20ae:38c9` against the guest's own memory. That is also
  what makes `Fighter::junk` non-zero, so the dealers' sell-junk branch is no
  longer always the one taken.

### Wander buckets 1 and 4 — their text is still not modelled

**Established from flow** that neither writes a discovery flag (no
`c6 06 [94-9a] 36 imm8` store falls between `1000:b3ba` and `1000:b940` except
`1000:b570`) and that neither spends a draw, so leaving their *text* out
cannot move the RNG sequence.

* Bucket 1 (`1000:b3c4`) toggles `[0x3693]`
  (`80 3e 93 36 00` / `b0 00` / `75 01` / `40` / `a2 93 36` — read, then
  written back inverted), then writes one district-keyed line from one of two
  sets (`1000:b3db`.. when the toggle is set, `1000:b465`.. when it is clear).
  Neither line set is extracted, so the port writes nothing here.

  **The toggle itself IS modelled, since Task 11f.** An earlier revision of
  this section said `20ae:3693` was not modelled because "a field carrying
  the toggle would have no reader". That was wrong, and it is the "scan whose
  completeness claim stopped the next search" failure `METHODOLOGY.md` names:
  `1000:0d86` and `1000:0e54`, both inside `FUN_1000_0d14`, change the draw
  count and the draw values of every later encounter, and Task 17 found a
  third reader at `1000:4ebc` in `FUN_1000_3d11` — see the `20ae:3693` entry
  above for the full seven-reference list. It is `Game::flag_3693`.
* Bucket 4 (`1000:b836`) branches on the joint-buff countdown `[0x38cd]` and
  writes name-keyed flavour built with `0f78:0ae7` / `0f78:0b66` string calls.
  Nothing outside bucket 4 reads what it writes, and it spends no draw.

### `run`'s extra line — `1000:aeda`

**Established from flow.** `1000:ae86` (`w`) and `1000:ae97` (`run`) both jump
to `1000:aea1`, and `1000:aeda` re-compares the typed line against `run`
(token file `0x9D60`) to decide whether to print `^6Забегал мудак.` (file
`0x9D7D`). `crate::commands::parse` folds both verbs into one `Command::Walk`,
so this port cannot tell them apart and prints nothing. It costs no draw.

---

## `PLACES.SAV`'s byte order — settled

*Cited from `src/locations.rs`'s `TRACKED`.*

**Established from flow.** The reader is at `1000:6c5a` and uses `Read`, not
`BlockRead` — seven one-byte reads, each naming its destination flag:

```text
6c5a  push ds:0x3e36                 ; the file variable
6c6a  call 0f78:0ae7                 ; copy DS:3d32 (the directory) into a temp
6c74  call 0f78:0b66                 ; append cs:0x63f2 = file 0x7CC2, 'places.sav'
6c79  call 0f78:072e                 ; Assign
6c87  call 0f78:0769                 ; Reset(f, 1)  -- record size 1
6c8c  call 0f78:028a                 ; IOResult; non-zero -> 1000:6d3b
6ca2  call 0f78:081e -> DS:0x3694    ; Read #1  Market
6cb4  call 0f78:081e -> DS:0x3695    ; Read #2  Dealers
6cc6  call 0f78:081e -> DS:0x3696    ; Read #3  Den
6cd8  call 0f78:081e -> DS:0x3697    ; Read #4  Girl
6cea  call 0f78:081e -> DS:0x3698    ; Read #5  Vet
6cfc  call 0f78:081e -> DS:0x3699    ; Read #6  Club
6d0e  call 0f78:081e -> DS:0x369a    ; Read #7  Gym
6d1b  call 0f78:07ea                 ; Close
6d20  writes '^0Загружено из places' (file 0x7CCD)
```

File order therefore equals flag-address order: **Market, Dealers, Den, Girl,
Vet, Club, Gym**. `TRACKED` carried Vet and Den swapped at slots 2 and 4 and
has been corrected; the file's own bytes still cannot arbitrate (`orig/*.SAV`
and `orig/PLACES.SAV` are `01` in every slot), but they no longer need to.

Earlier revisions of this section and of `src/locations.rs` said the read "has
not been located" and that "locating the `BlockRead` would settle it". Both
claims were wrong: the routine exists and there is no `BlockRead`.

The failure arm at `1000:6d3b` is a **conditional** reset. It clears Vet
(`6d3b`), Market (`6d40`), Club (`6d4c`), Gym (`6d51`), Girl (`6d5d`),
Dealers (`6d62`) and Den (`6d6e`), except that `1000:6d45`
(`cmp word [0x389c],3` / `jz 0x6d51`) skips the Club clear, `1000:6d56`
(same compare, `jz 0x6d62`) skips the Girl clear, and `1000:6d67`
(`cmp word [0x389c],5` / `jz 0x6d73`) skips the Den clear — one flag each,
not pairs. It then writes `^6Чё-то глюкануло - немогу прoгрузить Places:Ресет ту Default` (file `0x7CE3`) and
leaves via `1000:6d8c`/`1000:6da0`, never reaching `1000:6dbe`.
`[0x389c]` is the character class (Task 11b) — the skips keep the class
bonuses and clear only what was discovered. ~~The port has no `.SAV` load path
at all, so none of this is reproduced.~~ **Task 19 built the load path**, and
`crate::persist::load_slot` reproduces the failure arm as an all-clear
`Places` plus `1000:73bb`'s class bonus, which restores exactly the three
flags the arm's compares spare. Note the arm is reached only from **slot 0**:
`1000:6c50` `cmp byte [0x3692],0` sends slots 2..5 past `places.sav`
entirely.

## ~~No `.SAV` load path~~ — CLOSED by Task 19

*Cited from `src/main.rs` and `src/persist.rs`.*

`orig/g.exe` runs from itself alone, so "no save file" is still the ordinary
new-game case (**corroborated** by running it) and the slot menu prints
nothing at all when the directory holds no `save_r?.sav` (`1000:6b33`).

Everything else in this entry was true and is not any more. `.SAV` offsets
`0x214` and `0x2ae` are established (`docs/re/save-format.md`), `Save::parse`
is no longer the only constructor (`Save::blank`), and `Game::write_save`'s
`Unsupported` is gone with the method. `crate::persist` holds the whole path
with its addresses: the `FindFirst` scan and menu (`1000:6a62`..`1000:6b81`),
the five accepted keys and the `1` that is not one (`1000:6b5e`..`1000:6b7f`),
the `Reset`/`BlockRead` (`1000:6bcb`..`1000:6c13`), the district taken from
the slot digit (`1000:6bf9`), and slot 0's two extras — `places.sav`
(`1000:6c50`) and `district := level div 10 + 1` (`1000:6d93`).
`tests/save_load.rs` covers it; `src/main.rs` runs the menu before character
creation, the order `FUN_1000_6a0d` uses.

**Still not reproduced, and it is one thing.** The original does the
save-slot menu, the district-advance check and the whole main loop inside one
procedure, so its `ReadKey` at `1000:6b56` and its `ReadLn` at `1000:ac31`
are both available where they are needed. This port's menu reads a **line**
and takes its first character, because nothing in it does raw-key input at
all — a port decision, recorded in `crate::persist::choose_slot`. A second
site substitutes the same way: `Game::enter_district_5`'s `1000:addc` call
to `0f16:031a` (`ReadKey`) is ported as a discarded line read, matching this
one's "one keystroke, value unused" shape.

## ~~No typed save verb, and no "saved" message~~ — half closed, half REFUTED

*Cited from `src/persist.rs`.*

**The "no typed save verb" half stands, from flow**: `sv` sizes up the enemy
(`src/commands.rs`), and no compare in `entry` or `FUN_1000_3d11` reaches a
file write. Saving is checkpoint-only, at `1000:761d` (the mage, a paid
service at `district * 50`) and at the `0x9bcd` prompt (the district-advance
autosave).

**The "no 'saved' message" half was FALSE, and this entry is what made the
port print nothing on a save.** There are two, both in `data/strings.json`:

| decimal off | file off | string | printed at |
|---:|---|---|---|
| 36242 | `0x8D92` | `^0Сохранено! ^1Можешь беспредельничать дальше.` | `1000:7729`, the mage's paid arm |
| 39937 | `0x9C01` | `^1Сохранено в save_r` (+ the digit + `.sav`) | `1000:ace0`..`1000:ad0d`, the district autosave |

`0x9BCD` — which this entry cited as "a second path" without reading it — is
`^0Хочешь сохранить свои достижения?`, the autosave's own prompt. The
supporting claim ("a wrapper could only print composed text — which is why
there is none") therefore justified an absence with a false premise. The
mage's arm is implemented and prints its line; see below for the other.

## What the port REFUSES that the original accepts

*Cited from `src/save.rs`'s `SaveError`, `src/persist.rs`'s `load_slot` and
`Game::from_save`.*

**The original validates nothing on load.** `1000:6c01`/`1000:6c06` is an
untyped `BlockRead` of 694 bytes into `DS:369c` — the record *is* guest
memory, there is no unmarshalling step, and the only failure it can report is
`Reset`'s `IOResult` at `1000:6bd4` (the file is missing or unreadable). Any
694-byte file loads. Not an inference: `data/probes/saveprobe-record-base.json`
is a run of `orig/g.exe` loading a record carrying sentinels `0x40`..`0x64`
across `0x214`..`0x231` and `0x2ae`..`0x2b5` — values no game path writes —
and the guest reaches the street prompt with them in memory.

This port refuses three classes of record the original would load. Each is a
**port decision**, none is a finding about the original, and all three are
reachable with `tools/savegen.py` — which matters because that is the
instrument the next several tasks use to force states.

| what | where | what the port does | what the original does |
|---|---|---|---|
| a flag byte outside `{0, 1}` | `Save::parse`, `SaveError::NotBoolean` | refuses the file | loads it; every consumer tests `<> 0`, so a 2 reads as true |
| `joints`, `beer_half_litres` or `junk` negative | `Game::from_save`'s three `.max(0)` clamps | loads it, silently clamping to 0 | keeps the negative `Integer` |
| a name of exactly 255 CP866 bytes | `Game::to_save` re-adds the `^7 ` prefix (`1000:723a`) | `mage_save` fails with `TooLong(258)` | writes it; the record round-trips |

The first is the one with a visible symptom, and it is worse than the refusal
itself: `load_slot` maps `Save::parse`'s error onto the original's
`Reset`-`IOResult` arm, so it prints
`^6Чё-то глюкануло - нaверно нет такого сейва, Default:1` (CS `0x6451`, file
`0x7D21`, written at `1000:6da5`) and starts a new character. **The original
never prints that line for a record it could open** — that arm is reached only
when `Reset` itself failed. The port therefore reports a refusal of its own
using a string the original reserves for a different cause.

Why each is kept rather than fixed:

* **The Boolean check is what keeps the round trip total.** The 23 flag bytes
  are `bool` in `Save`, so a 2 could not survive re-serialisation; silently
  rewriting it as 1 would make the round trip byte-exact for every file the
  game writes and quietly lossy for one it does not. Every direct store to
  those bytes image-wide is `mov byte [X],0` or `mov byte [X],1`, so the
  original cannot produce such a file — only a synthesiser can.
* **The clamps** are the cost of `Fighter` holding `u16` where the record
  holds `Integer`. Widening those three fields is a `crate::model` change,
  and `model.rs` is shared with the combat replays.
* **The 255-byte name** is a boundary case of the prefix divergence recorded
  below: the original keeps `^7 ` in the live variable `DS:379c` and this port
  adds it at the format boundary, so the port's ceiling for a typed name is
  252 bytes rather than 255.

**`Game` -> `Save` -> `Game` is not the identity above the original's widths.**
`Game::to_save` narrows five fields the port had widened — `money` and
`street_cred` from `i32`, `armour` from `u16`, and `xp`/`threshold` from `u32`
— by truncation, which is what the original's own 16-bit and 8-bit arithmetic
does, but it means a `Game` holding an out-of-range value does not survive a
save. `Save` -> bytes -> `Save` **is** exact, and that is the round trip
`tests/save_roundtrip.rs` asserts.

**One more refusal, not in the table above because it is not a save-format
one:** the port never runs the chapter-5 endgame arm's two forced fights
(`1000:ae2d` `FUN_1000_3d11(3)` and `1000:ae39` `FUN_1000_3d11(4)`), which the
original re-enters on **every** turn once district 5 is reached. The arm's
three lines and its `1000:addc` keystroke are **not** part of that refusal —
those run exactly once in the original too, and the port matches them. Detail,
and the decode that separates the two halves, is in "The district-advance
autosave — wired (Task 21)", below.

## The four armour flags are carried but the gym's `abs` ignores them

*Cited from `src/game.rs`'s `imm_row_visible` and `src/persist.rs`.*

**Established from flow.** The gym recomputes a scratch byte `20ae:3e34` on
every entry (`1000:e3a4`..`1000:e3e2`): it starts as the armour byte
`20ae:38b2` and then has the armour that came from *equipment* subtracted
back out, so what is left is the armour the player TRAINED.

| at | subtracts | when |
|---|---|---|
| `1000:e3aa`..`1000:e3b8` | 1 | `[0x38b4]` set and `[0x38b7]` not |
| `1000:e3bc`..`1000:e3c3` | 2 | `[0x38b7]` set |
| `1000:e3c8`..`1000:e3d6` | 2 | `[0x38b6]` set and `[0x38b9]` not |
| `1000:e3db`..`1000:e3e2` | 4 | `[0x38b9]` set |

Those are `mar` rows 4, 7, 6 and 9 — the abibas suit, the adidas suit, the
leather jacket and the crutaya kozhanka — and the four subtrahends are the
rows' own advertised bonuses. Exactly one thing reads the result: `trn`
row 5. It has **two** gates and only the second reads `abs` —
`1000:e576` (`cmp byte [0x3692],0x2` / `jbe 0xe5e4`) is `district > 2`, and
`1000:e57d`..`1000:e58d` (`shl ax,1` on the district, `mov al,[0x3e34]`,
`cmp ax,dx` / `jnl 0xe5e4`) is `abs < district * 2`. `imm_row_visible`
implements both; an earlier revision of this entry folded them into one.

**The port carries the four flags (`Game::wear_suit_abibas_38b4`,
`wear_jacket_38b6`, `wear_suit_adidas_38b7`, `wear_jacket_krutaya_38b9`,
`.SAV 0x218`/`0x21a`/`0x21b`/`0x21d`) and `imm_row_visible` ignores all four,
so its `abs` is exactly `armor`.** The consequence is one-directional: the
port's `abs` is never smaller than the original's, so it can only **hide** a
gym row the original would show.

**Task 19 made this live, and deliberately did not fix it.** Before it, no
path in the port could set those bytes — `mar` purchases deduct and print but
apply no effect — so the divergence could not be reached. A loaded `.SAV`
sets them, and the shipped corpus contains a witness.

| save | `38b4` | `38b6` | `38b7` | `38b9` | `armour` | original `abs` | port `abs` |
|---|---:|---:|---:|---:|---:|---:|---:|
| `SAVE_R0` | 0 | 1 | 1 | 0 | 4 | 0 | 4 |
| `SAVE_R2` | 1 | 0 | 0 | 0 | 1 | 0 | 1 |
| `SAVE_R3` | 1 | 1 | 1 | 0 | 4 | 0 | 4 |
| `SAVE_R4` | 1 | 1 | 1 | 0 | 10 | **6** | **10** |
| `SAVE_R5` | 0 | 0 | 1 | 1 | 26 | 20 | 26 |

**No shipped save carries all four flags**; `SAVE_R3` and `SAVE_R4` carry
three. (An earlier revision of this entry said those two carried all four.
The frozen corpus refutes it — `38b9` is clear in both, and set only in
`SAVE_R5`.)

**The witness is `SAVE_R4` loaded at slot 4.** The original computes
`abs = 10 − 2 (38b7) − 2 (38b6 without 38b9) = 6` against a threshold of
`district * 2 = 8`, and 6 < 8, so it **shows** `trn` row 5; the port computes
`abs = 10`, which is not < 8, so it **hides** it. Money is 952, well over the
row's own 20-rouble test at `1000:e58f`, so nothing else suppresses it.
`SAVE_R3` at slot 3 and `SAVE_R5` at slot 5 agree either way, and `SAVE_R2`
is district 2, where the first gate hides the row for both.

Fixing it properly needs `mar`'s purchase effects, which are the larger
unimplemented gap below; applying the subtraction on its own would gate a gym
row on a flag the player has no way to earn.

## The district-advance autosave — wired (Task 21)

*Cited from `src/game.rs`'s `Game::run` and `Game::district_advance`, and
`src/persist.rs`'s module doc.*

**Established from flow**, `1000:ab75`..`1000:ad12`, re-disassembled for
Task 21 with `python3 tools/re_query.py resolve 1000:ab75 -n 420 -i 200`. At
the top of every main-loop iteration: `1000:ab7f` (`cmp ax,[0x38a6]` with
`ax = district * 10`, i.e. `district * 10 <= level`) and `1000:ab88`
(`cmp byte [0x3692],5` / `jb`, i.e. `district < 5`) gate
`1000:ab92 inc [0x3692]`, then the discovery-flag resets
(`1000:ab96`..`1000:abc9`) and both ban countdowns are cleared
(`1000:abce`/`1000:abd3`), then
`^1Ты доказал, что ты самый крутой в этом районе - отправляйся в следующий`
(file `0x9B83`, decimal 39811), `^0Хочешь сохранить свои достижения?`
(file `0x9BCD`, 39885), a bare `\` written with `0eed:0000` — `Write`, no
newline — (file `0x9BF1`), `ReadLn` into `DS:3a72` (`1000:ac31`), the
case-fold at `1000:ac45` (`0eed:0216`), and `1000:ac54`'s compare against `y`
(file `0x9BF3`). On a match it builds `DS:3d32` (the directory) + `save_r` +
`Str([0x3692])` + `.sav` — **the district AFTER the increment**, which is
exactly why the shipped corpus is `SAVE_R2`..`SAVE_R5` with no `SAVE_R1` —
`Assign` at `1000:acab`, `Rewrite(f, 0x2b6)` = 694 at `1000:acb9`,
`BlockWrite` from `DS:369c` at `1000:acc8`, `Close` at `1000:acd5`, and the
`^1Сохранено в save_r` + digit + `.sav` line at `1000:ad0d` (files `0x9C01`
= 39937 and `0x9BFC` = 39932).

**All of that is now in the port**, as `Game::district_advance`, called from
the top of `Game::run`'s `while self.running` loop. Three things had to be
established before the placement was safe, and all three are flow:

* **The block is upstream of the turn's own prompt.** `1000:ae3c` writes the
  same bare `\` (`cs:0x8321`, file `0x9BF1`) through `0eed:0000` and
  `1000:ae55`..`1000:ae63` is the top-level `ReadLn` into `DS:3972` — both
  *after* the whole `ab75`..`ae18` region. So the advance prints, prompts and
  possibly saves before the player is asked what to do that turn.
* **The block cannot loop, so at most ONE district is gained per turn.**
  Every branch inside `ab75`..`ad12` is forward (`ab83`, `ab85`, `ab8d`,
  `ab8f`, `aba5`, `abb6`, `abc7`, `ac59`, `ac5b`), and the only branch
  instruction in the image targeting `0xab75` is `1000:ee01 e9 71 bd`
  `jmp 0xab75`, at the END of the turn; `1000:ab72 e8 98 be` `call 0x6a0d`
  is a three-byte near call whose next instruction is `ab75`, which is the
  fall-through entry the first time. **The port used to get this wrong**:
  `Game::run_combat` ran the gate as `while self.district < 5 && …`, so a
  level-40 district-1 character gained four districts inside one fight where
  the original needs four turns.
* **Only street turns pass through it.** Each shop handler writes its own
  prompt and `ReadLn`s into `DS:3a72` inside its own loop (`1000:bd08` /
  `1000:bd21` for `mar` — "Shop modality" in
  `docs/re/command-dispatch.md`), never reaching `1000:ee01`. `Game::run`
  therefore gates the call on `Mode::Street`; `Mode::Shop` is this port's
  line-at-a-time stand-in for that inner loop and must not promote.

**A byte scan alone gets the back-edge claim wrong, and this is worth
recording.** Scanning every `jmp`/`Jcc`/`call`/`loop` encoding whose target
is `0xab75` returns **two** raw hits. The second, `1000:ab00` `72 73`
(`jb 0xab75`), passes the 64-way alignment sweep **63/64** — and is the `rs`
of `^4Gopnik: ^7version 1.02 june,`, sitting in the CS literal pool, the
`0x82b3`..`0xab59` gap `data/functions.json` leaves between `FUN_1000_7c67`
and `entry`. (`0x82b3` is not a coincidence: it is the `mov di,0x82b3` at
`1000:abd8`, this block's own first string.) That is
`docs/re/METHODOLOGY.md`'s `1000:d83b` lesson reproduced on a second
address: alignment never answers yes. Earlier revisions of this section
asserted "the only branch instruction … whose target is `0xab75`" without
saying that a naive re-derivation finds two and that the sweep endorses the
wrong one.

**`1000:ab92` is the only in-play district write.**
`python3 tools/re_query.py xrefs-to 20ae:3692` accepts 97 references and
discards 0. **Four** of them are direct stores — `1000:6bf9`, `1000:6d9d` and
`1000:6dbe`, all three inside `FUN_1000_6a0d`, the one-time setup, plus
`1000:ab92` itself.

**`FUN_1000_3d11` — the fight — contains no WRITE**, which is the whole of
what the promotion's placement needs: a level won in a fight cannot move the
district from inside the fight, so the district it earns is collected by
`1000:ab92` at the top of the next turn. It is **not** true that the fight
has "no reference of any kind", which an earlier revision of this sentence
claimed. The same 97-reference scan puts **twelve** references inside it,
every one a read and every one the identical three bytes `a0 92 36`
`mov al,[0x3692]`, each anchored from the function's own entry:

```text
1000:4a8e  1000:4cbb  1000:4dad  1000:4dbe  1000:4e68  1000:529c
1000:53f7  1000:5449  1000:57d4  1000:57e7  1000:5808  1000:5824
```

By function, the 97 accepted references fall out as `entry` 67,
`FUN_1000_3d11` 12, `FUN_1000_6a0d` 7, `FUN_1000_0d14` 4, `FUN_1000_7538` 3,
`FUN_1000_7c67` 2, `FUN_1000_5f55` 1, and one outside every catalogued
function — which is the fifth writer, below.

A **fifth** reference also writes, by pointer rather than by displacement,
and an inventory that stopped at four would be the completeness shape
`docs/re/METHODOLOGY.md` names:

```text
0f78:134c  bf 92 36     mov di,0x3692     ; the DGROUP BSS start
0f78:134f  1e / 07      push ds / pop es
0f78:1351  b9 18 41     mov cx,0x4118     ; the BSS end
0f78:1354  2b cf        sub cx,di
0f78:1356  d1 e9        shr cx,1          ; words, not bytes
0f78:1358  33 c0        xor ax,ax
0f78:135a  fc / f3 ab   cld / rep stosw
0f78:135d  c3           ret
```

That is the runtime's startup zero-fill of `20ae:3692`..`20ae:4118`, which
covers the whole 694-byte record as well. **"At startup" is a chain of
addresses, not an adjective** — an earlier revision asserted "it runs once,
before anything game-shaped" with no caller cited at all:

* `orig/g.exe`'s MZ header holds `e_cs:e_ip = 0000:ab59`. A stored segment is
  relative to the load base (`docs/re/METHODOLOGY.md`), so relseg `0` is
  Ghidra `0x1000` and the program entry point is `1000:ab59` — which is
  exactly where `data/functions.json` puts the function it names `entry`.
* `1000:ab59 9a 00 00 78 0f call 0f78:0000` is that entry's **first**
  instruction.
* `0f78:0000 ba ae 10 / 8e da` sets `DS` to DGROUP `0x10ae`, and
  `0f78:000b e8 3e 13 call 0x10acc` reaches `0f78:134c`. That is the only
  **direct** transfer to it in the image: one near call within segment
  `0f78`, and zero occurrences of the far-call signature `9a 4c 13 78 0f`
  anywhere. The scan covers `e8`/`e9`/`eb`/`7x`/`ea`/`9a`; it does **not**
  cover indirect transfers (`ff /2`, `ff /3`), which no scan by target
  address can, so "only" is scoped to direct ones rather than absolute.
* Control returns and reaches `1000:ab72 e8 98 be call 0x6a0d` — the
  character setup — and only then falls through to `1000:ab75`.

So the fill precedes every game write to `[0x3692]`, by address, and runs
once.

**What is still NOT reproduced, and why:**

* **The discovery-flag resets are unconditional in the port.** `1000:aba0`,
  `1000:abb1` and `1000:abc2` each compare `[0x389c]` (the class) and skip
  exactly one clear — Club (`1000:aba7`) and Girl (`1000:abb8`) are spared
  for class 3, the Den (`1000:abc9`) for class 5; Gym (`1000:abac`) and
  Dealers (`1000:abbd`) are always cleared, being the second store in each
  pair, past the skip. `Places::reset_for_new_district` clears all seven.
  This divergence predates Task 21 and Task 21 did not spend the opening it
  created: the class is now in scope at the one call site, so passing it in
  is a local change, but it alters which locations a player keeps across a
  promotion and wants its own test. `src/locations.rs`'s module doc records
  the same.
* **The district-keyed announcement arms at `1000:ad12`..`1000:adbf`**
  (`cmp al,2` at `1000:ad15` and the chain after it) are unported text.
* **The chapter-5 arm's two forced fights** — and *only* those.

  **A correction, established from flow.** A first cut of this section (and
  of the two `src/game.rs` doc comments that quote it) said `1000:adbf`'s
  `cmp al,5` was "unconditional, so the original re-prints the three lines,
  re-takes the `1000:addc` keystroke, re-sets both flags and re-enters both
  forced fights every single turn". **The first four of those five are
  false**, and the decode says so plainly. Once `[0x3692]` reaches 5:

  ```text
  ab88  80 3e 92 36 05  cmp byte [0x3692],5
  ab8d  72 03           jb 0xab92      -- NOT taken at 5
  ab8f  e9 86 02        jmp 0xae18     -- skips ad12..adbf entirely
  ```

  So `1000:ad12` and `1000:adbf` are unreachable on a non-promotion turn. An
  encoding scan of the whole image (`e9`/`e8`/`eb`/short-`Jcc`/near-`Jcc`/
  `loop`, each hit then checked against an anchored decode) finds **exactly
  one** branch into `0xad12` — `1000:ac5b e9 b4 00 jmp 0xad12`, the `y`
  mismatch arm, post-increment — and **exactly one** into `0xadbf` —
  `1000:ad89 75 34 jnz 0xadbf`, the `cmp al,4` arm inside the `ad12` chain,
  itself only entered post-increment (the chain's other entry is
  fall-through from `1000:ad0d`'s `WriteLn`, also post-increment).
  `1000:adc3`, `1000:addc` and `1000:ae13` have **zero** branches targeting
  them at all; `1000:adbd eb 59 jmp short 0xae18` is what sits immediately
  before `adbf`, so it is not reachable by fall-through either.

  **What actually repeats every turn is the `1000:ae18` arm**, which every
  gate failure jumps to (`ab85`, `ab8f`, `ad4b`, `ad84`, `adbd`, `adc1` — six
  branches, plus fall-through from `ae13`):

  ```text
  ae18  80 3e 83 3c 01  cmp byte [0x3c83],1   -- nothing ever clears it
  ae1d  75 1d           jnz 0xae3c
  ae1f  c6 06 96 36 01  mov byte [0x3696],1   -- the Den, idempotent
  ae27  e8 98 63        call 0x111c2          -- FUN_1000_11c2(0)
  ae2d  e8 e1 8e        call 0x3d11           -- FUN_1000_3d11(3), the rector
  ae33  e8 8c 63        call 0x111c2          -- FUN_1000_11c2(1)
  ae39  e8 d5 8e        call 0x3d11           -- FUN_1000_3d11(4), the ending
  ```

  `docs/re/wander.md`, "The three Den setters" (§`423`), already scoped this
  correctly — "`1000:ae18` sits at the top of every turn … once chapter 5 is
  reached, this block runs every turn" — so the wrong version above put the
  repo in contradiction with itself for one review round.

  **Consequence for the port.** `Game::enter_district_5` prints the three
  lines, consumes one line for `1000:addc`'s `ReadKey`, sets
  `rector_showdown` (`1000:ae13`) and grants the Den (`1000:ae1f`) once,
  from `district_advance`'s just-incremented branch. That **matches** the
  original for the prints, the keystroke and the `ae13` store, which run
  exactly once there too; the Den grant repeats in the original but is a
  boolean store, so it is idempotent and indistinguishable. The **only**
  remaining divergence is the four calls at `ae27`..`ae39` — in practice the
  two fights, since `FUN_1000_11c2` only fills the enemy record.

  **The reason that stays open is now the whole reason**, with nothing
  propping it up: `FUN_1000_3d11`'s `param_1` is not modelled by
  `Game::run_combat` — the XP-award skip at `1000:51b9`..`1000:51e9` and the
  `param_1 == 4` victory ending at `1000:5085`, which has never been traced
  (`docs/re/wander.md`: "Whether `FUN_1000_3d11(4)` returns is not traced
  here"). Closing it is a combat-dispatch task: trace `param_1`, then call
  the two fights from a per-turn `rector_showdown` check. The earlier
  argument that repeating the arm "would announce two fights every turn"
  was built on the false half and is withdrawn. Full detail on
  `FUN_1000_11c2`, which Task 20 DID fully trace, is in
  "`FUN_1000_11c2` -- traced (Task 20), not ported", below.

A *loaded* save already at district 5 is handled separately and was already
right: `1000:7364`, inside `FUN_1000_6a0d`, reads `[0x3692]` (the district,
not the class byte `[0x389c]`) and arms `rector_showdown` at entry. It is
ported in `Game::apply_class_bonus`, called from `src/persist.rs`'s
`from_save`.

Also cross-referenced from "What the port REFUSES that the original
accepts", above (§`557`); the detail lives here because that section is
scoped to save-load refusals specifically and this divergence is not one —
it belongs to the main loop's shape.

## The trimmed `y` prompts — the port accepts input the original refuses

*Cited from `src/commands.rs`'s `parse` and `src/game.rs`'s
`district_advance`, `walk`, `mage`, `wander_girl`, `shop_turn` and
`run_combat`.*

**Established from flow.** Each of the four `y` prompts below reads a line,
case-folds it with `0eed:0216`, and compares it with `0f78:0bd8`
`rtl_str_compare` against a CS shortstring. `rtl_str_compare` compares Pascal
shortstrings, whose **length byte is part of the value** — so `" y"` (length
2) can never equal `y` (length 1), and no step between the `ReadLn` and the
compare strips a space. That is the whole argument, and it does not depend on
where the buffer lives.

**It is not "always `DS:3972` or `DS:3a72`", and an earlier revision of this
paragraph said it was.** Three of the four rows read into `DS:3a72`; the
mage's reads into a **stack-local** shortstring, re-pushed for each step:

```text
75c7  8d be 00 ff / 16 / 57   lea di,[bp-0x100] / push ss / push di
75cd  b8 ff 00 / 50           mov ax,0xff / push ax        ; max length
75d1  9a c6 06 78 0f          call 0f78:06c6               ; the ReadLn worker
75d6  9a 9d 05 78 0f          call 0f78:059d               ; the line skip
75db  9a 91 02 78 0f          call 0f78:0291               ; the {$I+} check
75e0  8d be 00 ff / 16 / 57   lea di,[bp-0x100] / push ss / push di
75e6  9a 16 02 ed 0e          call 0eed:0216               ; the case-fold
75eb  8d be 00 ff / 16 / 57   lea di,[bp-0x100] / push ss / push di
75f1  bf a9 74 / 0e / 57      mov di,0x74a9 / push cs      ; file 0x8D79 = `y`
75f6  9a d8 0b 78 0f          call 0f78:0bd8
75fb  74 03 / e9 5f 01        jz 0x7600, else jmp 0x775f
```

A stack shortstring carries its length byte exactly as a DGROUP one does, so
the conclusion is untouched — but the universal as written was false, and
`Game::mage`'s own doc already recorded the stack buffer as "a third input
buffer". The four `y` prompts all compare against the same one-byte literal
at file `0x9BF3` (`01 79`), except the mage's, whose copy is file `0x8D79`:

| prompt | buffer | case-fold | compare | port |
|---|---|---|---|---|
| the district autosave | `DS:3a72` | `1000:ac45` | `1000:ac54` | `Game::district_advance` |
| the encounter accept | `DS:3a72` | `1000:b704` | `1000:b713` | `Game::walk` |
| wander bucket 2, the girl | `DS:3a72` | `1000:b534` | `1000:b543` | `Game::wander_girl` |
| the mage's paid save | `SS:[bp-0x100]` | `1000:75e6` | `1000:75f6` | `Game::mage` |

**The port trims first, so it accepts `" y"` where the original refuses it.**
This is **not** introduced by the autosave: it is the port's house idiom for
every typed compare, `crate::commands::parse` (`src/commands.rs:214`,
`input.trim().to_lowercase()`) included, and it therefore also widens the
street verb table, `Game::shop_turn`'s key match, and the two in-combat verb
compares `run_combat` handles itself (`run` at `1000:48e1` and `e` at
`1000:4c56` — two rows of the nine-row compare-site table in "The in-combat
verb set", below; that nine counts the original's `0f78:0bd8` sites inside
`FUN_1000_3d11` and is unrelated to the nine trim sites counted here). The
port-side inventory is a grep, with its command, rather than an assertion of
completeness:

```
$ grep -rn '\.trim()' src/*.rs | grep -v 'trim_end_matches\|trim_start_matches'
src/commands.rs:214:    let v = input.trim().to_lowercase();
src/game.rs:886:        // `.trim()` is a PORT ADDITION and a real (if tiny) divergence:
src/game.rs:898:        if answer.trim().eq_ignore_ascii_case("y") {
src/game.rs:1406:        let key = line.trim().to_lowercase();
src/game.rs:1811:        if answer.trim().eq_ignore_ascii_case("y") {
src/game.rs:2341:        if !answer.trim().eq_ignore_ascii_case("y") {
src/game.rs:2410:        if !answer.trim().eq_ignore_ascii_case("y") {
src/game.rs:2661:    ///   kept, not substituted. `Game::rename` must not `.trim()` the line
src/game.rs:2691:        // not `.trim()` `n` before this check -- that would substitute on
src/game.rs:3265:            if line.trim().eq_ignore_ascii_case("run") {
src/game.rs:3304:            if line.trim().eq_ignore_ascii_case("e") {
src/game.rs:5545:    /// `Game::rename` stopped `.trim()`-ing the line, this case wrongly
src/main.rs:92:    buf.trim().parse().unwrap_or(0)
```

**Thirteen hits: nine call sites and four lines of prose about them.** The
output above is pasted verbatim from the shipped tree rather than summarised,
because an earlier revision of this block pasted a hand-annotated listing
whose `src/game.rs` line numbers were each one lower — it had been produced
before the commit that inserted the comment this block's own table now lists
at `src/game.rs:886`.

Two caveats on "verbatim", both learned the hard way:

* **Any edit above `src/game.rs:886` desynchronises this block**, which is
  how it went stale the first time and again while this very correction was
  being written (a four-line doc comment added at `src/game.rs:809` shifted
  every `game.rs` number by three). It has to be re-run and re-pasted in the
  same commit as any such edit, not before it.
* `grep` on the machine this was captured on is `ugrep 7.8.4`, which searches
  in parallel, so the **file order** varies between runs — `src/main.rs:92`
  moved from second to last between two consecutive captures. The content of
  every line is stable; only the interleaving is not. Compare this block
  line-set-wise, not byte-wise.

The nine call sites:

| line | what it normalises |
|---|---|
| `src/commands.rs:214` | the street verb table |
| `src/main.rs:92` | `Val()` on the class answer — a number, not a token |
| `src/game.rs:898` | the district autosave's `y` |
| `src/game.rs:1406` | `shop_turn`'s key |
| `src/game.rs:1811` | the encounter accept's `y` |
| `src/game.rs:2341` | the mage's `y` |
| `src/game.rs:2410` | `wander_girl`'s `y` |
| `src/game.rs:3265` | combat's `run` |
| `src/game.rs:3304` | combat's `e` |

The four prose hits are `886` (the autosave comment that points here) and
`2661`, `2691`, `5545`, which all say `Game::rename` must **not** trim — the
next paragraph is what they are about.

**One place deliberately does not trim, and it is the interesting one.**
`1000:7220` and `1000:ed5f` test the just-read name shortstring's **length
byte**, so a line of only spaces is a nonempty name and is kept rather than
replaced by `Раз^6дол^4бай`. `Game::rename` and `main.rs`'s
`create_character` strip only the line terminator for exactly that reason —
which is the evidence that the trim elsewhere is a port choice, not a
property of `ReadLn`.

**Not fixed here.** Removing the trim is a one-line change per site, but it
changes what those nine input paths accept and belongs with a task that can
test all of them; doing it only at the autosave would make this port
self-inconsistent for no gain. Recorded so the divergence is in one place
instead of nine scattered comments.

## `FUN_1000_11c2` -- traced (Task 20), not ported

*Cited from `src/game.rs`'s `Game::enter_district_5`.*

**Entry point `1000:11c2`.** Never named in `docs/re/` before Task 20, which
disassembled it in full: `python3 tools/re_query.py resolve 1000:11c2 -n 250
-i 80`.

**Established from flow.** 50 instructions, `0x11c2`..`0x1273` (178 bytes,
prologue through the 3-byte `ret 0x2`; `file_off 0x2a92`..`0x2b43`;
`python3 tools/re_query.py resolve 1000:11c2 -n 175 -i 60 --json` and count
instructions with `image_off <= 0x1271`, the START of that `ret`), no branch
besides its own two argument arms, no draw, and no call besides the
`0f78:02cd` stack-check prologue every Pascal procedure carries. It takes one
byte argument (`bp+4`) and stores a fixed block into the enemy record
`20ae:3952..396e`:

* `20ae:3952` (class) := `0xa`, unconditionally, before either arm.
* arg `0`: `395c`(level)=`0x7d`, `3954`(str)=`0x29`, `3956`(agi)=`0x32`,
  `3958`(vit)=`0x7b`, `395a`(luck)=`0x24`, `3968`(armour)=`0x3c`.
* arg `1`: `395c`=`0xa0`, `3954`=`0x32`, `3956`=`0x3c`, `3958`=`0xbc`,
  `395a`=`0x20`, `3968`=`0x50`.
* both arms: `395e`(dmg_min) := strength/2 (`idiv 2`), `3960`(dmg_max) :=
  strength, `3964`(hpmax) := `5*vitality + strength + 10`, `3962`(hp) :=
  hpmax, `3966`/`3967` (jaw/leg) := 0, `396a`/`396c`/`396e` (loot) := 0.

Both arms match `data/enemies.json`'s `rektor_ngu_v0` (arg 0: level 125, str
41, agi 50, vit 123, luck 36, dmg 20-41, hp/hpmax 666, armour 60) and
`rektor_ngu_v1` (arg 1: level 160, str 50, agi 60, vit 188, luck 32, dmg
25-50, hp/hpmax 1000, armour 80) exactly, and the derived fields match this
port's own `Game::roll_enemy` formulas (`hpmax = 10 + 5*vitality + strength`,
`dmg_min/dmg_max = strength/2, strength`). So `1000:11c2` itself holds no
open question: it is a stat-block initialiser for two already-catalogued
`data/enemies.json` rows, and `Enemy::to_fighter` already builds a `Fighter`
from either.

**What is still open is `FUN_1000_3d11`'s `param_1`, not `FUN_1000_11c2`.**
The two calls this arm makes to the fight function (`1000:ae2d call 0x3d11`
with `param_1 = 3`, `1000:ae39` with `4`) are what stay unported, because
`Game::run_combat` does not model `param_1` at all:

* `1000:51b9`..`1000:51e9`, the XP award, is skipped for `param_1` in `{3,
  4}` (`docs/re/combat.md`, "The victory block"); `run_combat` awards XP
  unconditionally.
* `1000:5085 cmp byte [bp+0x4],0x4` selects a separate victory ending for
  `param_1 == 4` — `FUN_1000_074b(1)`, file `0x1DBF` (a 49-byte shortstring,
  "^2Ты победил." padded with 36 leading spaces to centre it) — that this
  project has never traced. `docs/re/wander.md`, "The three Den setters":
  "Whether `FUN_1000_3d11(4)` returns is not traced here."

Porting the rector and final-boss fights needs that ending traced and
`run_combat`'s signature widened for `param_1` first — a combat-dispatch
task, not a flag-setter one. Until then `1000:ae2d`/`1000:ae39` (and the two
`FUN_1000_11c2` calls that feed them, `1000:ae27`/`1000:ae33`) stay
unreproduced, registered here rather than left implicit.

## `help`'s printed content

*Cited from `src/game.rs`'s `show_help`.*

**Established from flow** that `help` is dispatched at `1000:edd5`. Its handler
body was not traced, so nothing is printed rather than inventing a line: the
game has no "not implemented" string to quote. Disassembling the handler
settles it.

## `rename`'s prompts — the retraction was wrong; there is no deviation

*Cited from `src/game.rs`'s `rename`.*

An earlier revision of this entry said `^2Звали тебя:^7 ` and
`^2А теперь будут:^7 ` were "this port's own wording … the one place the code
knowingly departs from the byte-verbatim rule", because "`1000:ecf1`'s handler
body was not traced". **Both prompts are the game's own strings**, and the
handler is now traced. There is **no** knowing departure from byte-verbatim
text anywhere in this port.

**Established from flow**, `1000:ecfb`..`1000:ed9c`, the arm `name`'s compare
at `1000:ecf1` takes:

* `1000:ecfb`..`1000:ed24` — build a temp shortstring from the literal at
  image `0xaab1` = file **`0xC381`** = `^2Звали тебя:^7 ` (`0f78:0ae7`,
  `rtl_str_assign`), append the name variable `DS:379c` (`0f78:0b66`,
  `rtl_str_append`), and `WriteLn` it (`0eed:01c2`).
* `1000:ed29`..`1000:ed3d` — `Write` (`0eed:0000`, no newline) the literal at
  image `0xaac2` = file **`0xC392`** = `^2А теперь будут:^7 `.
* `1000:ed42`..`1000:ed5a` — `ReadLn(Input, DS:379c)` (`0f78:06c6` /
  `0f78:059d` / `0f78:0291`, the three-call `ReadLn` idiom `docs/re/rtl.md`
  names).

The two file offsets are `data/strings.json` entries `0xC381` and `0xC392`,
sitting immediately after the `name` token at `0xC37C` that
`docs/re/command-dispatch.md` cites for this handler.

### What `rename` really was missing

Tracing the handler turned up two behaviours the port did not have. The first
is now fixed; the second is registered here and still open.

**1. An empty new name becomes `Раз^6дол^4бай` — fixed.** `1000:ed5f`
`cmp byte [0x379c],0` / `jnz 0xed79` tests the length byte of what was just
read; on zero, `1000:ed74` calls `0f78:0b01` (`rtl_str_assign_max`) with the
literal at image `0xaad7` = file **`0xC3A7`** = `Раз^6дол^4бай` as source and
`DS:379c` as destination. `0f78:0b01`'s operand layout settles which is which:
`lds si,[ss:bx+0xa]` is the source, `les di,[ss:bx+6]` the destination,
`mov cx,[ss:bx+4]` the length cap, and it ends `retf 0xa`.

This is the same code the port already models at character creation
(`1000:7220` / `1000:7227`, literal at file `0x80B4` — `src/main.rs`'s
`create_character`), so the port kept the old name on an empty rename while
substituting the default on an empty creation. `Game::rename` now substitutes.

**2. The stored name is prefixed with `^7 ` — NOT modelled.** Both name-entry
sites end with the same three-call idiom that rebuilds the variable as
`<literal> + <name>`:

| path | sequence | the three calls: assign / append / store | literal | file off |
|---|---|---|---|---|
| creation | `1000:723a`..`1000:725d` | `1000:7245` / `1000:724f` / `1000:725d` | `^7 ` | `0x80C2` |
| `rename` | `1000:ed79`..`1000:ed9c` | `1000:ed84` / `1000:ed8e` / `1000:ed9c` | `^7 ` | `0xC3B5` |

`0f78:0ae7` assigns the literal into a stack temp, `0f78:0b66` appends
`DS:379c`, `0f78:0b01` assigns the temp back to `DS:379c`; the first two
`retf 4`, popping only their source and leaving the destination pointer on the
stack for the next call, which is why one `lea di,[bp-0x100]` serves all
three. So the original's stored name is literally `^7 ` + whatever was typed.

**Corroborated by state**, and this is what fixes the direction of the
concatenation beyond the operand layout: all five saves in `orig/` carry the
prefix in their `pstring` at `.SAV 0x100` — `^7 adg`, `^7 vor`, `^7 vor`,
`^7 vor`, `^7 Mudila`. `docs/re/save-format.md` already records that field as
"player name, colour-prefixed".

This port stores the bare name in both paths. **Not implemented**: the prefix
is part of the name the save writes, so adding it changes the `.SAV` bytes
this port emits and every line that interpolates the name, and no capture in
`data/rng_trace.json` or `data/state_trace.json` exercises a rename or a
character creation whose name is compared. (Nothing is lost on the read side:
`src/main.rs` never loads a save, and `tools/decode_save.py` round-trips the
original files byte-for-byte, prefix included.) Registered here with both
addresses rather than left as a silent difference.

## The vet's charged amounts

*Cited from `src/game.rs`'s `heal_jaw` / `heal_leg`.*

**Established from flow** that the menu prints `3` and `7` (files `0xB2B2`,
`0xB2D9`) and that the affordability colour compares money against the same
literals (`cmp word [0x38c7],0x3` at `1000:d410`, `cmp word [0x38c7],0x7` at
`1000:d465`).

**The debit is no longer an inference.** An earlier revision of this entry
called it one because "the vet's own submenu handler was not traced"; the
`difftest` task traced it, and this entry did not follow. Both purchase arms
are in the image and are **established from flow**:

| site | bytes | instruction | row |
|---|---|---|---|
| `1000:d553` (file `0xee23`) | `83 2e c7 38 07` | `sub word [0x38c7],0x7` | `r`, the broken bones |
| `1000:d5d9` (file `0xeea9`) | `83 2e c7 38 03` | `sub word [0x38c7],0x3` | `h`, the jaw |

Each arm's row is fixed by the nearest preceding token compare: `1000:d537`
tests `r` (file `0xB320`), `1000:d5b9` tests `h` (file `0xB392`). Note the two
arms are laid out in the **opposite** order to the two menu rows, which is why
`docs/re/difftest.md`, "Eight of the nine unnamed `sub [money],imm8` sites are
now named", pairs them by key rather than by position; that document carries
the enumeration, and `data/other_price_sites.json` now records both.

## The in-combat verb set

*Cited from `src/game.rs`'s `run_combat`.*

**The verb set is established from flow.** An earlier revision of this entry
said `FUN_1000_3d11`'s own input loop "was not disassembled" and called `k`
"this port's own choice … not independently confirmed". Both statements were
false by the time they were read, and neither survives the scan below.

`FUN_1000_3d11` (entry `1000:3d11`) does its own token comparison with
`0f78:0bd8`, the same Pascal shortstring compare `entry` uses
(`docs/re/command-dispatch.md`), but against its **own** input buffer
`DS:3a72` rather than `entry`'s `DS:3972` — which is exactly why
`crate::commands::parse`'s table does not describe this prompt.

The image holds 93 `9a d8 0b 78 0f` call sites in total. Scanning from
`1000:3d11` to the **next function entry**, `1000:5f55`, returns exactly
**nine** of them. That window is deliberately wider than the record's own
`size` span (`1000:3d11`..`1000:584b`), so the count does not depend on
reading `size` as a span — the trap `docs/re/branches.md` documents. Every one
of the nine is preceded, byte for byte, by
`bf 72 3a` / `1e` / `57` (`mov di,0x3a72` / `push ds` / `push di`) and
`bf <lo> <hi>` / `0e` / `57` (`mov di,<token>` / `push cs` / `push di`), so
each site's token is read out of the instruction, not guessed from proximity:

| compare site | token | token file off |
|---|---|---|
| `1000:4440` | `k` | `0x4A52` |
| `1000:48e1` | `run` | `0x4C8B` |
| `1000:4b0d` | `kos` | `0x4D81` |
| `1000:4c2e` | `s` | `0x4E6F` |
| `1000:4c42` | `sv` | `0x4E71` |
| `1000:4c56` | `e` | `0x4E74` |
| `1000:4c75` | `k` (a second compare, gated on `[0x3c80] >= 1` at `1000:4c64`) | `0x4A52` |
| `1000:4caa` | `v` | `0x4E96` |
| `1000:4ea8` | `f` | `0x4FE4` |

Check any single row with `python3 tools/re_query.py resolve 1000:4440`; the
count is reproduced by finding `9a d8 0b 78 0f` over `[0x3d11, 0x5f55)` in the
load image.

So `k` **is** the in-combat attack verb — established from flow, at
`1000:4440` — and `sv` (inspect, `1000:4c42`), whose only prior evidence was
`docs/re/tables.md`'s oracle capture, is a dispatched verb here too. `h`/`mh`
(beer) remain established from flow via `FUN_1000_3d11`'s call into
`FUN_1000_29c4` at `1000:4b00`, which is a *subroutine* call and therefore not
one of the nine.

**What is NOT established.** *(Superseded by Task 17: all nine arms are now
followed into their bodies in `docs/re/combat-dispatch.md`, which also
establishes that these nine plus `FUN_1000_29c4`'s `h`/`mh` are the whole
accepted set — the buffer has exactly twelve references inside
`FUN_1000_3d11` and only one near call receives it. The paragraph and table
below are kept as the record of what was open before that.)* The verb *set*
was established; the *effects* of most arms were not. Three of the nine arms
had been followed into their bodies:

* `1000:4440` `k` — the fight itself. `docs/re/combat.md` traces the blow
  budget, accuracy and damage inside it (`1000:445c`..`1000:4660` and the
  enemy's mirror); `Game::combat_round` is that reconstruction.
* `1000:48e1` `run` — traced in full, `src/game.rs`'s `run_combat` doc.
* `1000:4b0d` `kos` — traced in full by the final-review fix wave, below.

Six arms are **not traced**, and the only thing quoted for each below is the
instruction that was actually read at its jump target:

| token | compare | first instruction of the arm | in the port? |
|---|---|---|---|
| `s` | `1000:4c2e` | `1000:4c35` `call 0x1a03` — **traced, Task 16**: `docs/re/character-sheet.md` | no |
| `sv` | `1000:4c42` | `1000:4c49` `call 0x1348` — **not** `FUN_1000_1a03`; Task 16's breakpoint confirms `sv` never enters it | yes, but from the oracle capture, not from this arm |
| `e` | `1000:4c56` | `1000:4c5d` `xor ax,ax` / `call 0f78:0116` | no |
| `k` (2nd) | `1000:4c75` | `1000:4c7c` `inc [0x3c80]`, then `cmp word [0x3c80],3` | no |
| `v` | `1000:4caa` | `1000:4cb4` `cmp byte [0x3696],1` (the den flag) | no |
| `f` | `1000:4ea8` | its own arm | no |

Nothing beyond those instructions was claimed for any of the six when this was
written. **Task 17 traced all six**: `sv` calls the enemy's sheet
(`FUN_1000_1348`, which reads no address in the player's record at all), `e` is
`Halt(0)` through `rtl_halt`, the second `k` is the backup countdown, `v` calls
the local gopota, and `f` fires the pistol. `sv`'s row was the one place where
the port did something at a dispatch site whose arm it had not read: what it
prints comes from `docs/re/tables.md`'s capture, which is output-tier evidence
and is labelled as such in `Command::Inspect`'s doc — and
`docs/re/combat-dispatch.md` now has the flow-tier reading beside it.

### `kos` inside a fight: the same handler with a shorter buff

**Established from flow.** The arm `1000:4b0d`'s `jz 0x4b17` takes is the
**269 bytes at `1000:4b17`**, ending with the `call 0eed:01c2` at
`1000:4c1f`; the top-level `kos` handler is the **269 bytes at `1000:e97d`**,
ending with its own `call 0eed:01c2` at `1000:ea85`. Compared byte for byte
they differ in exactly **15** places: seven `mov di,imm16` string operands
pointing into the combat string pool instead of the top-level one, and one
immediate.

The immediate is the whole behavioural difference: `1000:4b52`
`c6 06 cd 38 03` sets the stoned countdown `20ae:38cd` to **3**, where
`1000:e9b8` `c6 06 cd 38 0a` sets it to **10**. Everything else — the broken-jaw
guard `cmp byte [0x38b0],1`, the already-stoned guard `cmp byte [0x38cd],0`,
the no-joints guard `cmp word [0x38c5],0`, `dec [0x38c5]`,
`add word [0x389e],2`, `inc [0x38a8]`, `add word [0x38aa],2` and the
under-10/over-10 heal split — is the identical instruction sequence.

Six of the seven strings are byte-identical to their top-level twins; the
seventh differs by one letter. The combat copy at file `0x4DF0` is
`^2Колёса прибавляют #з. Здоровья:#/#. Осталось # косяков` (Pascal length
byte 56) where the top-level copy at file `0xBF5E` ends `косякова` (57). Both
are quoted verbatim where they are used; neither is a typo this port may fix,
and `Joint::long_heal_line`'s test asserts the two differ by exactly the one
trailing letter so a later edit cannot quietly unify them.

| purpose | combat pool | top-level pool |
|---|---|---|
| broken jaw | `0x4D85` | `0xBEF3` |
| heal prefix, shortfall < 10 | `0x4DB4` | `0xBF22` |
| heal suffix | `0x4DCD` | `0xBF3B` |
| heal line, shortfall >= 10 | `0x4DF0` | `0xBF5E` (one letter longer) |
| `^2Сила +2.` | `0x4E29` | `0xBF98` |
| no joints | `0x4E34` | `0xBFA3` |
| already stoned | `0x4E49` | `0xBFB8` |

This arm **is** implemented — `Game::smoke` takes the countdown and the
long-heal line from its call site, so the fight prompt's `kos` sets 3 and the
street prompt's sets 10.

### The four arms this port drops, and what it prints instead

`s`, `e`, `v` and `f` reach `run_combat`'s `match` and are **registered as
unimplemented**, not silently discarded: `src/game.rs`'s `run_combat` names
each with its compare address in the arm that ignores it. An earlier revision
justified dropping them with the comment *"matches the live capture's mar/i
rejection"*. That is evidence about `mar` and `i` — neither of which is one of
the nine — and about nothing else; a verb the dispatcher compares may be
dispatched and print nothing (`docs/re/METHODOLOGY.md`, "Absence of visible
response is not absence of dispatch").

Implementing them needs each arm's body traced first. `e` looks like a
`Halt(0)` from its one quoted instruction and `v` like a den-gated call for
backup, but *looks like* is not a tier and neither is written up as one.

## `Delete`'s index clamp — the linked runtime is not this library's

**Established from flow** (`0f78:0c8f`, Task 11h; see `docs/re/rtl.md`, "which
build of TP 7"). The Borland `Delete(S, Index, Count)` linked into `orig/g.exe`
is **not** the one in the TP 7.0 `TURBO.TPL` it was matched against, and the
difference is behavioural rather than cosmetic:

* the library's copy tests `cmp word [bp+8],0` / `jle` and **returns without
  touching the string** when `Index <= 0` — 6 bytes, at `+13`;
* this build has no such test. At `+32` it has 11 bytes instead —
  `83 7e 08 01` / `7d 05` / `c7 46 08 01 00`, i.e. `if Index < 1 then Index :=
  1` — and then deletes.

So `Delete(S, 0, 3)` is a no-op in the library and removes three characters
from the front here. (The net size difference is +5 bytes, which is what shifts
everything after; the edit itself is 11 added against 6 removed.)

**It cannot affect this program, and the port does not need it today.**
`0f78:0c8f` has no caller anywhere in the image: 0 far-call sites
(`9a 8f 0c 78 0f`), 0 near `e8` calls to `0x0c8f` inside segment `0f78`, and
the far pointer `8f 0c 78 0f` appears 0 times as data. It is dead code the
smart-linker kept. Recorded here because it is the sharpest single piece of
evidence that the build is not 7.0-as-shipped, and because a port that ever
emulates `Delete` must pick the clamping semantics, not the library's.

`tools/test_rtlmatch.py::test_deletes_divergence_is_visible_in_the_image_itself`
pins the 11 bytes against `orig/g.exe`, so this needs no library to re-check.

## The two ban countdowns are modelled and decremented but never set

*Cited from `src/game.rs`'s `market_ban_countdown` / `club_ban_countdown`,
`Game::walk_preamble` and `Game::visit_girl`.*

`20ae:3b76` (market) and `20ae:3b77` (club) are two byte cooldowns. The port
declares both at the right addresses, ticks both down once per walk, and reads
both for a phone message — but **nothing in `src/` ever assigns either a
non-zero value**, so the two `== 1` message branches and both `> 0` decrements
are currently dead code. That is a real omission, registered here rather than
left implicit; all six sites below are **established from flow** and were
re-derived from `orig/g.exe` for this entry.

| what | site | bytes | in the port? |
|---|---|---|---|
| set the market ban to 5 | `1000:c465` | `c6 06 76 3b 05` | **no** |
| set the club ban to 5 | `1000:e23e` | `c6 06 77 3b 05` | **no** |
| `mar`'s gate on it | `1000:b95e` | `80 3e 76 3b 00` + `jz 0xb968` | **no** |
| `kl`'s gate on it | `1000:df1a` | `80 3e 77 3b 00` + `jbe 0xdf3d` | **no** |
| `girl` clears the market ban | `1000:d793` | `c6 06 76 3b 00` | **no** |
| both tick down, once per walk | `1000:b173` / `1000:b17e` | `fe 0e 76 3b` / `fe 0e 77 3b` | yes |
| the district advance clears both | `1000:abce` / `1000:abd3` | `c6 06 76 3b 00` / `c6 06 77 3b 00` | yes (Task 21) |

The last row is new and does **not** change the verdict: clearing a byte that
nothing ever sets is still inert. It is listed because the two clears are
now genuinely executed by `Game::district_advance`, so when the setters do
land they will already be reset on every promotion.

The gates are what the countdowns are *for*, and each has its own refusal
line. `1000:b95e` runs immediately after `mar`'s discovery-flag check at
`1000:b954`: ban zero takes `jz 0xb968` into the market intro (file `0xA430`),
ban non-zero takes `jmp 0xc480`, which prints file `0xA9C4`
(`^6На базар пока нельзя там менты бродят, тебя ищут.`) and returns to the
prompt. `1000:df1a` is the same gate with the branch polarity reversed: ban
zero takes `jbe 0xdf3d` into the club, ban non-zero falls through to
`1000:df21`, which prints file `0xB9BD`
(`^6Тебе не стоит пока туда соваться`).

Both refusal strings are in `data/strings.json` and neither is printed
anywhere in `src/` — the port's `enter_shop` gates on the discovery flag only.
Implementing the two setters without the two gates would be worse than the
present state, so this entry lists them as one omission, not five.

**Consequence, stated plainly:** `src/game.rs`'s two "it blew over" phone
messages (`1000:b11e`, `1000:b145`) can never print in this port, and the
decrements they share a preamble with can never run. They are left in place —
at the right addresses, in the right order in the walk preamble — so that
implementing the two setters and the `girl` clear is the only work needed to
make them live. Nothing about them is *wrong*; they are unreachable.

`Game::visit_girl`'s doc previously said the clear at `1000:d793` was "not
modelled here" without saying that the field it would clear exists; it now
points at this entry.

---

## Other unreproduced behaviour

* ~~**`kl` / `trn` priced rows** — prices are not in `data/shops.json`.~~
  **Closed by Task 12.** They are not in `data/shops.json` because their
  prices are instruction immediates, not bytes of the `20ae:0b2e` array
  `tools/extract_tables.py` scans; the same is true of the vet's two rows.
  All nine are now **established from flow** — `1000:d410`, `1000:d465`
  (`rep` 3, 7), `1000:df6f`, `1000:dfcb` (`kl` 15, 22), `1000:e400`,
  `1000:e455`, `1000:e4c4`, `1000:e521`, `1000:e58f` (`trn` 20, 20, 10, 30,
  20) — and the port prints them from `src/game.rs`'s `IMM_ROWS`.
  `docs/re/difftest.md` has the enumeration, the gate per row, and the
  `20ae:3e34` scratch byte the gym's fifth row is capped by. **Still open:**
  none of the nine applies its effect when bought (same reason as "Shop
  purchase effects" below), and the club's card game and the gym's purchase
  handlers are not implemented.

  An earlier revision of this entry ended "and the gym is unreachable in the
  port because nothing sets `20ae:369a`". **That was false**, and it
  contradicted this file's own setter inventory above. The gym's flag is set
  by the wander preamble's draw 8 — `1000:b21c` `Random(100)`, store at
  `1000:b22c`, ported at `src/game.rs:1421-1422` — so the gym is **rare, not
  unreachable**: 1 in 100 per walk. The port's setter is
  `mark_found(Location::Gym)` and `369a` appears near it only in a comment,
  which is how a grep for the address literal produced the wrong answer.
* **The class-keyed combat-opener table** (`1000:3d32`..`1000:3e8a`, files
  `0x452E`, `0x453B`, `0x4548`, `0x4565`, `0x457A`, …). Its **text** is still
  not extracted; what Task 13 settled is that it cannot matter to the
  generator: scanning `[0x3d11, 0x3f00)` for `9a 4b 11 78 0f` returns **zero**
  hits, so the whole `cmp [0x3952],N` chain is print-only.
* ~~**The rector death branch** (`1000:4f8c`) — nothing in this port sets
  `[0x3c83]` ... Still not modelled here.~~ **CLOSED by Task 18 — modelled.**
  `[0x3c83]` is the rector-showdown flag, armed at `1000:7364` and
  `1000:ae13` and never cleared (Task 17, `docs/re/combat-dispatch.md`); the
  port carries it as `Game::rector_showdown` and implements **all three** of
  its effects: no spectators (`1000:411d`, which sits *after* the counter
  block so `^7Начинают собираться зрители` still prints and only the two
  draws are suppressed), no fleeing (`1000:48eb`), and the death message that
  names the killer with **no rescue behind it** (`1000:4f8c`, ahead of the
  hospital's own gates).

  **Nothing sets the flag**, because neither writer is modelled —
  `1000:ae2d` / `1000:ae39` are the endgame's own two calls to
  `FUN_1000_3d11` with opponent kinds 3 and 4, and this port has no endgame.
  So the three arms are reachable only from a test, the same shape as
  `Game::market_ban_countdown` below. That is a **reachability** gap, not an
  implementation one: the difference matters, because the previous wording
  sent a reader to write code that already exists. ~~**and the hospital
  rescue** (`1000:4fce`) — need fields `crate::model::Fighter` does not
  have.~~ **The hospital rescue is implemented** (Task 13,
  `Game::hospital_rescue`): it needs the den flag, `20ae:38cb` and
  `20ae:38c7`, all of which `Game` already carried. It is **UNVERIFIED by
  observation** — see `docs/re/combat.md`, "What Task 13's capture did and did
  not reach": both captured deaths are of characters whose den flag is clear,
  so the branch was never taken in the original either.
* ~~**`sv`, `v`, `x`, `wes` token compare sites** — not located.~~ **Closed by
  Task 17.** `sv` (`1000:4c42`) and `v` (`1000:4caa`) are combat verbs,
  compared against `DS:3a72`, which is why a search of `entry`'s `DS:3972`
  chain could never find them; both arms are now mapped in
  `docs/re/combat-dispatch.md`. `x` and `wes` are **not** combat verbs: they
  are compared at `1000:ce80` (CS `0x96ce`) and `1000:ced8` (CS `0x970a`),
  both in `entry`, and no instruction anywhere in `FUN_1000_3d11`
  materialises either literal. What their arms do is still open.
* **The quit message** (files `0xC3F3`, `0xC41A`, written at `1000:ee04`) and
  the university backstory (`0x7D81`..`0x7F1F`) — real strings, not wired up.
* **Shop purchase effects — open for every row except three.**
  `data/shops.json` rows deduct `price` and print their menu text, but never
  change `strength` / `armor` / etc.: most rows have no representable target
  on `Fighter`. Two further divergences on that generic path, both
  pre-existing and both still open: it echoes the **menu line** where the
  original prints each arm's own confirmation, and it refuses a
  district-gated row, where the original's *buy* compares carry no district
  test at all (`1000:cc04`..`1000:ccd8`: row 6's arm is gated only on item
  flags, so a row the menu did not print is still buyable).

  **`bmar` rows 7, 8 and 9 are done** (Task 18, `Game::buy_pistol_row`),
  because they are what makes `20ae:394d` reachable and therefore what makes
  `f` at either prompt do anything: `1000:ccd8` the pistol
  (`mov byte [0x394d],1` and `add word [0x394f],3`), `1000:cd76` the
  cartridges (`add word [0x394f],5`), `1000:cdf9` the silencer
  (`mov byte [0x394e],1`, gated on the pistol AND on
  `1000:ce00 cmp byte [0x3e32],0x19`). Each arm's own gates and refusal lines
  are reproduced.

  **Two original-behaviour findings from those three arms**, both reproduced
  rather than fixed. Row 8's menu line reads `#^7 руб. Патроны - 6.` and
  `1000:cda3` adds **five** — the confirmation line
  `^2Получи пять пуль.. на руки` says five as well, so only the menu
  disagrees. And `docs/re/tables.md`'s `bmar` gate column carries row 9's
  `byte[20ae:394d]!=0` / `byte[20ae:3e32]==25` but not row 7's own
  `[0x394d] == 0` (`1000:ccd8`, else `^6Ну.. ты.. ВАЩЕ ОФИГЕЛ!`) or row 8's
  `[0x394d] != 0` (`1000:cd7b`, else `^6Нету пушки. Сначала купи пистолет`).

  `20ae:394d`'s name is now right in `src/` too: it was
  `Game::dealer_order_placed`, "a 150-rouble order placed with the dealers",
  which the closed entry above had already called a stale name. It is
  `Game::pistol`. `data/wander.json`'s `globals` still carries the old name —
  a reviewed artifact, unchanged.
* ~~**The joint (`kos`) heal formula** reuses beer's `FUN_1000_29c4` by
  analogy; the joint's own handler was not traced.~~ **Closed.** Both copies of
  the handler are traced: `1000:e97d` (top level) and `1000:4b17` (combat),
  269 bytes each, differing in 15 bytes — see "`kos` inside a fight" above.
  The heal is the handler's own `cmp ax,0xa` split at `1000:e9d2` /
  `1000:4b6c`, not an analogy with beer's.
* ~~**The decline branch after a fight encounter.**~~ **Closed by Task 11f**
  (stale entry corrected in Task 11g). The evade-vs-detected split on the
  `Random(2)` at `1000:b725` (`1000:b721` is its `mov ax,2`, `1000:b724` the
  `push`) is **established from flow**, and so now is the choice between it and
  the similarly-shaped path at `1000:b691`, which has no roll on decline at
  all: `1000:b5fc`..`1000:b61b` compares luck against the `1000:b5f1` notice
  roll as a longint and applies a class threshold of **3** on the luck-lost arm
  (`1000:b60a`) and **7** on the luck-won arm (`1000:b614`) — see "The
  random-encounter opponent" above. `Game::wander_fight` in `src/game.rs`
  models both arms and rolls at `1000:b725` only on the aggressive one, so the
  port no longer "always takes the `Random(2)` branch".
* **Shop modality** — `Mode::Shop`'s "accept a few keys, `w` to leave, ignore
  the rest" shape is **established from flow** only as far as each location
  writing its own prompt and `ReadLn`-ing into `DS:3a72` (`1000:bd08` /
  `1000:bd21` for `mar`); the submenu dispatch chain itself was not traced.

---

## Opened by Task 11b (the wander catalogue)

*Cited from `docs/re/wander.md` and `data/wander.json`.*

The wander preamble is now fully catalogued as one ordered sequence, so the
port's divergence there is a known quantity rather than an unknown one. These
are the questions that pass left open, and the ones it created.

* ~~**The whole sequence is static-only.**~~ **Closed by Task 11d.** All
  eighteen draws were observed in the running original — `tools/rngtrace`,
  `docs/re/rng-trace.md`, `data/rng_trace.json`. Five runs, 1387 draws, each
  fired at the catalogued site with the catalogued `n` (the two computed ones
  checked at two districts), in the catalogued order — asserted by the tool, not
  read off the turn signatures: `data/rng_trace.json.order_check` records 86
  turns checked and 0 violations — and **nothing was contradicted**. The catalogue's tier is now **flow, corroborated by live
  trace**. What that pass did *not* raise: probabilities still come from the
  comparison constants and never from counting outcomes, and the fight-flow
  questions below are untouched.
* ~~**`unk_38b2`.**~~ **Resolved by Task 11c — it is the ARMOUR byte.**
  `20ae:38b2` is fighter-record offset `+0x16` (`0x38b2 - 0x389c = 0x16`), and
  `crate::model`'s record table and `docs/re/combat.md` already establish that
  field as armour: subtracted from damage at `1000:4769`, printed as
  `^2Броня #` (file `0x2C0A`) at `1000:163f`. Its two neighbours in
  `data/wander.json`'s own `globals` corroborate the alignment — `20ae:38b0`
  (`+0x14`) is the jaw and `20ae:38b1` (`+0x15`) the leg, exactly the record's
  `broken_jaw`/`broken_leg`. `1000:81e9`'s `inc byte [0x38b2]` under
  `^1Накладываю на тебя защиту!` ("I lay protection on you") is therefore the
  church granting +1 armour. **Corroborated by state:** `SAVE_R3.SAV` holds
  `4` at `.SAV 0x216` and run E's guest, which never entered the church,
  reports `unk_38b2 == 4` at the end of the run. `src/game.rs` increments
  `Fighter::armor` there. `data/wander.json`'s `globals` entry carried the stale
  `unk_38b2` until it was renamed to `armor` (tier `flow+corroborated`); the
  frozen probe captures under `data/probes/` and the `tools/rngtrace` column
  key still spell it `unk_38b2`, which is a capture field name, not a claim.
* ~~**The item at `DS:394d`.**~~ **CLOSED by Task 16** — it is the **pistol**.
  Bought from the dealers for 150 roubles at `1000:cd05` (price byte
  `DS:0b3e`), and it arms the 25-walk delivery counter `DS:3e32` that
  `1000:af1d` drives. The character sheet names it, from flow and from a
  second site: `1000:1d38 cmp byte [0x394d],0x0` / `1000:1d3d jnz 0x1d42`,
  and the taken arm runs `1000:1d42`..`1000:1d51 mov di,0x17b8` —
  `^1У тебя есть пистолет` — with **no branch between `1000:1d42` and
  `1000:1d51`**, so the label is unconditional on that flag. `20ae:394e` is
  the silencer (`1000:1d6a`, `^1 с гушителем`) and `20ae:394f` the patron
  count (`1000:1d8a`), which is what `docs/re/tables.md:290` already read from
  the other direction. `data/wander.json`'s `dealer_order_placed` is a **stale
  name** for `20ae:394d` — the artifact was not modified for that finding.
* **`1000:4aa5` sets the Den flag while printing a refusal.** The byte is
  `c6 06 96 36 01` (verified) and the line is
  `^4Такого конявого непустят в местный притон!` (file `0x4D42`); the den gate
  at `1000:d80c` reads nothing but that flag. Whether a clear was intended is
  **unverified** and cannot be settled from the binary. Task 17 adds two
  mechanical facts either way: every immediate store to `20ae:3696` in the
  image is `0` or `1`, so it is a boolean and `1` is the value the gate
  admits; and the post-kill twin at `1000:52ae` tests the same
  `level - (district-1)*10` expression with `jl` where `1000:4aa3` uses `jnz`,
  so the flee arm fires only on the exact value 3.
* **Does the chapter-5 block re-run every turn?** `1000:ae18` is at the top of
  every iteration (back-edge `1000:ee01` `jmp 0xab75`) and nothing clears
  `[0x3c83]` — its only writes are `1000:7364` and `1000:ae13`. So on the face
  of the flow, once chapter 5 is reached the rector fight and the endgame fight
  run every turn. Whether `FUN_1000_3d11(4)` returns at all was not traced.
* **Bucket 2's `Random(2)` is behind a typed `y`, and the port may not know
  that.** New in Task 11d, **established from flow** and corroborated by a
  breakpoint that did not fire. `1000:b4e8`'s arm is gated twice before it
  reaches the draw at `1000:b54e`: `1000:b4ef` `cmp byte [0x3697],0x0` (skip
  when the girl is already known, printing file `0xA24C`), and then a `ReadLn`
  at `1000:b520` whose input is compared against the token `y` (file `0x9BF3`)
  at `1000:b543`, with `1000:b548` `jnz 0xb590` skipping the draw when it does
  not match. The live trace saw fourteen bucket-2 turns — seven of them with
  the girl flag still clear — and `1000:b54e` never fired, because the harness
  declines every question. `docs/re/METHODOLOGY.md`'s worked example describes
  the `y` path correctly but does not mention either gate. ~~**Open question
  for the wander implementation.**~~ **Checked and closed by Task 11c:
  `Game::wander_girl` already agrees with both gates**, and the whole block
  `1000:b4e8`..`1000:b592` was re-derived from `orig/g.exe` to confirm it —
  `1000:b4ef` `cmp byte [0x3697],0x0` with `1000:b4f6` `jmp 0xb592` (the
  already-known arm, file `0xA24C`, which reads no input), the `ReadLn` into
  `DS:3a72` at `1000:b520`, the case-fold `0eed:0216` at `1000:b534`, the
  compare against `y` (file `0x9BF3`) at `1000:b543` and `1000:b548`
  `jnz 0xb590` skipping the draw. The port takes the same three decisions in
  the same order and spends the draw only on `y`. The draw is now recorded
  under the site label `1000:b54e`, so a future regression shows up as a named
  site rather than a value drift.
* **The `y` path of bucket 2 was never driven**, so the `Random(2)` at
  `1000:b54e` and the flag store at `1000:b570` are still static-only. One
  `tools/rngtrace` run whose driver answers `y` would close it.
* ~~**The mage's printed price disagrees with the charged price.**~~
  **Folded back in fix wave 1.** `docs/re/tables.md`'s "Other price sources"
  now records both halves — printed `chapter*25` at `1000:758d`, checked and
  charged `chapter*50` at `1000:7605`/`1000:7618`.
* ~~**`data/command_dispatch.json` still records the three Den setters as
  trigger-UNVERIFIED.**~~ **Folded back in fix wave 1.** All three
  `setters_found` entries now carry the trigger established from flow;
  `1000:4aa5` keeps its unresolved set-while-refusing note (above).
* ~~**`docs/re/command-dispatch.md` step 5 is wrong.**~~ **Folded back in fix
  wave 1.** Step 5 now names `1000:b353` as the regular-turn bucket roll, says
  there is one wander path, and points at `docs/re/wander.md`. Step 4's "not
  catalogued" was corrected at the same time.
* ~~**`docs/re/progression.md` lists `DS:38c1` as "text only".**~~ **Folded
  back in fix wave 1.** The one-shot table now names it the ring "Господи
  помилуй" with its per-walk regen, and records the church's second grant site
  for all three gift flags.

---

## Opened by Task 11c (wiring the wander sequence in)

*Cited from `src/game.rs` and `tests/wander_sequence.rs`.*

* ~~**`FUN_1000_0d14` and the fight flow.**~~ **Closed by Task 11f.** It has
  its own section above ("The random-encounter opponent"), now marked CLOSED:
  all five captured runs replay their whole draw stream and their whole
  `final_state`, and `cargo test` is green. What remains open inside the
  fight flow is enumerated there, and none of it costs a draw.
* **The market's second pickpocket block spends three draws this port never
  makes** — `1000:c344` `Random(district * 5 + 5)`, `1000:c361` `Random(10)`
  and `1000:c371` `Random(luck * 2)`.
  **Established from flow**, re-derived from an aligned start at `1000:c2a0`:
  the token compare at `1000:c329` (`call 0f78:0bd8`, string operand
  `cs:0x9089`) jumps to `1000:c333` on a match, which reads the district
  `[0x3692]`, builds `district * 5 + 5` (`1000:c33a`..`1000:c340`) and spends
  the **first** of the three at `1000:c344`, comparing it against luck `[0x38a4]`
  (`1000:c353` `cmp dx,bx` / `1000:c355` `jg` / `1000:c357` `jl 0xc3cd` /
  `1000:c359` `cmp ax,cx` / `1000:c35b` `jc 0xc3cd`). Only then does
  `1000:c361` `Random(10)` run, with `1000:c366` `cmp ax,0x9` /
  `1000:c369` `jnc 0xc3cd` — so the block is skipped one time in ten — and
  `1000:c371` `Random([0x38a4] * 2)` produce the amount, which is `inc`'d at
  `1000:c376`, stored at `1000:c377`, added to money at `1000:c37d` and
  printed at `1000:c396`. `data/rng_trace.json` never observed any of the
  three (the capture driver never entered the market submenu), and
  `docs/re/wander.md`'s
  catalogue is the wander preamble only, so these are outside it. This was
  found while correcting a false completeness claim in `src/game.rs` about
  `20ae:3b74` — the theft amount — whose earlier comment asserted that
  `1000:b321`..`1000:b346` was its only reader. It is not; this block is the
  second, and the two are byte-for-byte the same shape from the `inc ax`
  onward.
* **Run E's starting discovery flags are inferred, not observed.**
  `data/rng_trace.json` records a 29-variable `final_state` per run but no
  starting state, and the seven `places.sav` flags are outside the 694-byte
  record. Run E ends with Den, Girl, Club and Gym clear, and nothing in a
  wander turn can clear a flag, so those four must have started clear;
  Vet and Market are *set during the run* by draws 5 and 6, so their starting
  value is not determined by the capture at all. `tests/wander_sequence.rs`
  starts all seven clear and says so. No discovery flag gates any draw in the
  preamble, so the choice cannot move the sequence — but the guest's own
  `places.sav` load path is not modelled and it is not known why the flags
  were clear when `orig/PLACES.SAV` (copied into the run's game directory) is
  all `01`.
* ~~**`.SAV` offsets `0x2b1` and `0x2b5` are inside `unk_02ae`.**~~
  **CLOSED by Task 19.** The reasoning here was right and its `0x2b1` label
  was not: the span is `20ae:394a`..`20ae:3951`, but `20ae:394d` is the
  **pistol**, not `dealer_order_placed` — Task 18 had already corrected that
  name and this entry kept it. `data/save_layout.json` and
  `docs/re/save-format.md` now name all eight bytes with their addresses and
  their evidence, so nothing disagrees any more.
* **`Fighter::stoned` and `Game::buff_countdown` are two models of one
  variable.** The original keeps only the countdown at `20ae:38cd`
  (`1000:e9b4` sets it to 10, `1000:aea8` decays it, `1000:aeb3` takes the
  buff back at zero). `crate::model::Fighter` already had a `stoned: bool`,
  and this task added the counter rather than changing the frozen model
  module, keeping the two in step in `Game::smoke` and in the decay step. One
  of them should go.
* **Text the wander turn writes that this port still does not.** None costs a
  draw: `run`'s extra line (`1000:aeda`, above), the church's two long
  sermons and its rank-name pair, bucket 1's and bucket 4's flavour lines, and
  the `0f16:031a` `ReadKey`s the original spaces its phone-call gags with --
  waiting for a keystroke between each, not a timed pause. This line
  previously called `0f16:031a` a delay; it is `ReadKey`
  (`docs/re/rtl.md:494`; `Delay` is the unrelated `0f16:02a8`), fixed as part
  of Task 20's review round (C1), which found the same mislabel newly
  introduced in `src/game.rs`.
* ~~**`Game::mage` charges but cannot save.**~~ **CLOSED by Task 19**: the
  paid path writes both files and prints `1000:7729`'s line.
  `tests/save_load.rs::the_mage_writes_both_files_when_paid` and its
  `..._when_declined_or_broke` twin cover it. Still true, and unchanged: no
  captured run took that path — draw 14 returned `0` once, in run A's turn
  24, and the driver answered `n` — so the *whole arm* remains untested
  against the original, and only the file contents are (against the five
  shipped saves).
* **Two claims the differential test cannot reach**, found by mutating the
  port and checking that runs C and D still passed:
  * **The mage spends no draw.** Since no *passing* run enters `1000:7538`
    (run A does, at turn 24, but it has diverged since turn 2), this rests on
    the byte scan alone — no `9a 4b 11 78 0f` occurs in
    `1000:7538`..`1000:7778` — and not on the live trace. A capture whose seed
    puts a mage turn in the first few walks would close it.
  * **`church_visits`'s three transitions** (`1000:7dc7`, `1000:7f5b`, and the
    `1000:8247` read). The stage byte selects sermon text only; no draw
    depends on it, so the replay is blind to it.

---

## Opened and closed by Task 13 (the fight capture)

*Cited from `data/combat_trace.json`, `tests/combat_sequence.rs`,
`src/game.rs`'s `run_combat`/`crowd`/`claim_spoils`/`hospital_rescue` and
`src/combat.rs`'s `Swing`.*

Combat now has a control-flow oracle: four live runs of `orig/g.exe`, **1900
draws, 15 fights**, all four replayed draw-for-draw. `data/rng_trace.json` and
`data/state_trace.json` were not read, written or regenerated to build it, and
the new file records their SHA-256 so that is checkable. Method:
`docs/re/rng-trace.md`, "The fight channel".

### Closed

* ~~**The crowd's draws were not modelled at all.**~~ `1000:4135`
  `Random(10)` and `1000:4145` `Random(18)` are recovered and ported as
  `Game::crowd`, including the shape that is easy to get wrong: the counter at
  `[bp-0x113]` is initialised **outside** the prompt loop (`1000:40ed`, whose
  only back edge is `1000:583e`) and STOPS at 5, so `Random(10)` fires at every
  `Битва\` prompt from the fifth onward. Run A's 30-prompt fight shows 26 of
  them.
* ~~**The victory block after the XP award.**~~ `1000:523e`..`1000:57cc` is
  ported as `Game::claim_spoils`: the loot award, `hp += 5`, the street-cred
  term, the den flag, the `Random(30)` gift chain and both luck-gated item
  rolls. Run B asserts the whole 35-variable end state that results.
* ~~**The зубная защита's extra `Random(4)`** — "not modelled at all, because
  `Fighter` as the brief specifies it has no field for the item".~~ It is
  modelled as `Game::tooth_guard` + `combat::Swing::enemy`. It had to be:
  `SAVE_R3`, `SAVE_R4` and `SAVE_R5` all ship it, so a save-loaded replay
  desynchronises at the first player jaw break without it.
* ~~**The break rolls were asserted by nothing.**~~ Before this task the only
  `broken_jaw`/`broken_leg` assertion in the suite was `tests/data_load.rs`'s
  check that a *fresh* fighter has neither. `data/combat_trace.json`'s
  per-round channel samples `20ae:38b0`/`38b1`/`3966`/`3967` at every
  `1000:441d` stop, and run A (player's jaw) and run B (enemy's jaw, in two of
  six fights) now pin the **effect**, not just the roll.

### Three defects the capture found in code that was already there

None of these moves a draw; all three are output or state the replay caught.

* **The player's half never set the enemy's break flags.** `1000:45be`
  (`mov byte [0x3966],1`) and `1000:45e5` have no counterpart in the old
  `combat_round`, which printed the break line and left the enemy's record
  untouched. The per-round channel's `20ae:3966` is what caught it.
* **Both halves printed a break line the original suppresses.** `1000:459e`,
  `1000:45c5`, `1000:47c7` and `1000:4842` all test the flag first and print
  only on the transition.
* **The crit's `Random(3)` was drawn and discarded.** The port printed
  `^2Точный удар!!!` whatever it returned, and printed nothing at all for an
  ENEMY crit. All six lines (files `0x4A54`/`0x4A65`/`0x4A7B` and
  `0x4B52`/`0x4B67`/`0x4B7F`) are now selected by the roll.

### Opened

* **`Fighter::hp` is a `u16` and the original's is a signed word.** The
  killing blow drives `20ae:38ac` below zero — run A's post-death dump reads
  `-2` — while this port saturates the STORED value at 0.

  The half of this that costs draws is fixed: `Game::combat_round` keeps each
  round's running hp as an `i32` and drives both loop exits from it, because
  the two exits are different signed tests and the difference is a draw count.
  `1000:4629` `jg` leaves the PLAYER's loop at `enemy hp <= 0`, while
  `1000:48cd` `jl` leaves the ENEMY's only at `player hp < 0` — so a player
  sitting at exactly 0 gets swung at again, and that swing spends draws. A
  `u16` saturated at 0 cannot tell "exactly 0" from "would have gone
  negative", which is why the local is signed. The printed `У тебя осталось #`
  is the signed value too, as the original prints it.

  What is still open is the stored field: `player.hp` is 0 where the guest
  holds `-2`, so `final_state.hp_38ac` after a death is not comparable, and
  `tests/combat_sequence.rs` asserts the whole end state only for a run that
  ended at the turn marker. Fixing it means widening the field, which touches
  `src/model.rs`, `src/save.rs` and every test that reads it.
* **The two blow loops are not mirrors at the tail.** `src/combat.rs` calls
  them "the same instruction sequence twice with the two records' addresses
  swapped", which is true of the blow BODY and not of the loop tail: the
  player's half tests the defender before printing
  `Из-за большой ловкости ты можешь пнуть ещё раз` (`1000:4629` ahead of
  `1000:4639`) and the enemy's half has no such test before its own line
  (`1000:48a6` guards `1000:48ad` on the budget alone). Both are now written
  out separately in `combat_round`. It is text, not draws — but the shape
  matters, because "one function covers both" was the argument for not reading
  the second copy.
* **Four item arms are implemented but never observed.**
  `1000:5482`/`1000:5530`/`1000:5617`/`1000:5681` are gated on
  `luck >= Random(district * 40)`; the gate was reached eight times across the
  captures and passed once, against a class-2 enemy, which has no arm. Their
  damage-gating arithmetic rests on the disassembly alone.
* **A leg break has never been observed.** All five limb picks captured
  returned 0 (the jaw).
* **The class-keyed opener's text** (`1000:3d32`..`1000:3e8a`) is still not
  extracted. Task 13 established only that it spends no draw.
* **The port advances the district inside `run_combat`**, at the end; the
  original does it at the top of the next turn (`1000:ab75`..`1000:ab92`,
  which also clears `[0x3698]`/`[0x3694]` and conditionally `[0x3699]`). The
  order is equivalent for everything the captures reach — the new victory
  draws at `1000:5402`/`1000:5454` read the district and are made before the
  advance — but it is a placement difference, not an identity.

## Opened and closed by Task 16 (`FUN_1000_1a03`, the character sheet)

The map is `docs/re/character-sheet.md`; the machine-readable twin is
`data/character_sheet.json`, checked by `tools/test_character_sheet.py` and
defended by five `tools/mutations.json` cases (four over the artifact, one over the prose).

### Closed

* **What `FUN_1000_1a03` is.** The player's character sheet. Four call sites,
  `1000:ec89` and `1000:ee36` in `entry`, `1000:4c35` and `1000:512b` in
  `FUN_1000_3d11`; the verb is `s` at both prompts.
* **Its argument convention — there isn't one.** Bare `ret` at `1000:248e`, no
  positive `bp` displacement anywhere in the body, `ax` overwritten at
  `1000:1a06`. So the `s`-vs-`sv` difference cannot live in an argument, and
  does not: `sv` calls a different function (`1000:4c49` → `1000:1348`).
* **`stats` is not a verb.** The byte sequence does not occur in `orig/g.exe`.
* **`1000:1a36` is the CLASS-name lookup** into `ranks` (`20ae:002e`), not the
  rank lookup; the `krutizna` (`20ae:0b42`) lookup is `1000:1a53`.
* **The rector-VICTORY arm** — `1000:507b`/`1000:5085`, opponent kind 4 with
  the enemy at 0 hp, ending in the sheet at `1000:512b` and
  `FUN_1000_0aec`. Distinct from the rector *death* branch (`1000:4f8c`),
  which stays open above.
* **Seven DGROUP bytes now have names off the sheet's own labels**:
  `20ae:38b5` Бутсы, `20ae:38b8` Понтовые бутсы, `20ae:394a` Зубная защита,
  `20ae:394d` **пистолет**, `20ae:394e` глушитель, `20ae:394f` the patron
  count, and `20ae:38b2` confirmed a third time as Броня.
  `20ae:394d` is the one two other files explicitly asked for — see the
  closed entry above and `docs/re/wander.md:475`; the first version of this
  list omitted it, which a review caught.

### Opened

* **The decimal value of the health-colour thresholds.** `1000:211d mov cx,0x7f`
  and `1000:214d mov cx,0x80` are the two 6-byte-real comparands `hp/hpmax` is
  tested against (`rtl_real_op_cmp` at `1000:2124` and `1000:2154`), and the
  colour digit goes `'4'` → `'6'` → `'2'`. The ORDER is established — only the
  one register differs, and by one. The *values* are not: reading a 6-byte real
  as a decimal needs the layout confirmed against a known value, which
  `docs/re/rtl.md` records as **not established** for the `1000:4ff5` /
  `1000:5002` constants either. **What would settle it:** pin `20ae:38ac` and
  `20ae:38ae` under gdb to a pair straddling a candidate ratio, single-step
  `1000:2124`, and read which way the branch at `1000:2129` goes — two pokes
  bracket each threshold to arbitrary precision, and the same run settles
  `rtl.md`'s two constants because it establishes the layout.
* **24 of the 83 branches are still uncited**, listed exactly in
  `data/character_sheet.json`'s `branch_partition.uncited` (the test recomputes
  the split, so the list cannot drift). They are the section-header
  disjunctions — `Феньки: ` (`1000:1bc2`/`1000:1bc9`), `Мощные феньки: `
  (`1000:1c38`..`1000:1c46`), the weapon-line header
  (`1000:1e06`..`1000:1e30`) — the ammo-quantity flavour
  (`1000:1dab`..`1000:1dd7`), and the dimmed `^4` arms of the
  best-item-wins pairs. **What would settle them:** each is `or` over flags the
  cited arms already read, so a save synthesised with a chosen subset of
  `20ae:38b3`..`20ae:38c2` and `20ae:394a`..`20ae:394f` set, plus a breakpoint
  on the header's `Write`, decides each one without any new instrument. They
  were left because the brief said to map the structure and stop rather than
  pad the count.
* **`FUN_1000_1348` (`sv`, 791 bytes, 11 branches) is not mapped.** Only its
  entry and its one caller (`1000:4c49`) are established. Its output is still
  described from `docs/re/tables.md`'s oracle capture, which is output-tier.
* **`FUN_1000_0aec` and `FUN_1000_2526`** are cited from the rector-victory arm
  (`1000:5133`, `1000:5097`) and nothing more is claimed about either.

