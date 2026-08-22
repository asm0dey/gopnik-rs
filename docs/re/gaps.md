# Known gaps in the port

The list of things the port does **not** reproduce, and why. Source comments
cite this file by section.

Each entry states its evidence tier per `docs/re/METHODOLOGY.md`:
**established from flow** (with an address), **corroborated** (by state or
output, and by what), or **unverified** (and what would settle it). Every
address below was re-derived from `orig/g.exe` — `file_off = 0x18d0 + off` for
a `1000:off` code address, and a `mov di,<n>` / `push cs` string operand names
the string at file offset `0x18d0 + n`.

---

## Discovery flags: the complete store inventory

*Cited from `src/game.rs`'s `enter_shop` and `Game::new`.*

The seven discovery flags are seven contiguous bytes at `20ae:3694..369a`
(`docs/re/command-dispatch.md`, "Discovery gates"). Scanning `orig/g.exe` for
`c6 06 [94-9a] 36 imm8` (`mov byte [0x36??],imm8`) yields **31** stores:
**14 clears** and **17 set-to-1**. The clears are the two block resets —
`1000:6d3b`..`1000:6d6e` (the `places.sav` load-failure arm) and
`1000:ab96`..`1000:abc9` (`reset_for_new_district`). All seventeen setters are
below; **established from flow** (the scan is byte-exact and the encoding is
fixed-length, so it cannot miss a store of this form).

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
| `1000:73e0` | `0x3695` | BigMarket | `[0x389c] == 6` at `1000:73bb` | flow | **yes** (Task 11c) — `Game::apply_class_bonus` |
| `1000:dcf6` | `0x3695` | BigMarket | the `a` token at `1000:dcef` | flow | no |
| `1000:dcfb` | `0x369a` | Gym | the `a` token at `1000:dcef` | flow | no |
| `1000:ae1f` | `0x3696` | Den | the chapter-5 endgame arm at `1000:adbf` | flow | no |
| `1000:4aa5` | `0x3696` | Den | the de-level (flee) penalty, `1000:4a87`/`1000:4aa0` | flow | no |
| `1000:52b3` | `0x3696` | Den | the post-kill block, `1000:5295`/`1000:52b1` | flow | no |

All three Den triggers were **closed by Task 11b** — see `docs/re/wander.md`,
"The three Den setters". `1000:4aa5`'s store and the line it prints contradict
each other in the original; that is recorded there, not resolved.

**All seven flags are now reachable in this port** (Task 11c). Market and Vet
from character creation and from the wander preamble's draws 6 and 5; Club from
`girl`, from the class-3 bonus and from draw 7; Gym from draw 8; Girl from the
wander bucket and the class-3 bonus; Den from the class-5 bonus; BigMarket from
the class-6 bonus. Six setters remain unimplemented: the `a` token
(`1000:dcf6`/`1000:dcfb`), the chapter-5 endgame (`1000:ae1f`), the de-level
penalty (`1000:4aa5`) and the post-kill block (`1000:52b3`).

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
73db  cmp ax,6   / 73de jnz 0x73e5 / 73e0  [0x3695] := 1   (BigMarket)
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
dcba  cmp byte [0x3695],0 / 74 07  ; already-have check: BigMarket and
dcc1  cmp byte [0x369a],0 / 75 6a  ;   Gym both set -> skip to 0xdd32
dcc8..dcdc                         ; ax := ([0x38a6] - ([0x3692]-1)*10)*2 + [0x38cb]
dce0  cmp ax,0x28 / 7c 4d          ; < 40 -> skip
dce5  push ds:0x3a72               ; the line just typed
dcea  mov di,0x9fc9 / push cs      ; file 0xB899 = the single character 'a'
dcef  call 0f78:0bd8 / 75 3c       ; string compare; not equal -> skip
dcf6  [0x3695] := 1                ; BigMarket
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
table; and the mage's two file writes on the paid path (`save_r0.sav` at
`1000:764e`/`1000:765d`, `places.sav` at `1000:766f`..), because
`Game::write_save` is `Unsupported` for every `Game` this port can build. All
are text or I/O; none costs a draw.

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
  `Game::flag_3693`.

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

* `1000:493b`..`1000:4adc`, the **level > 0** arm of `run`: it reads the
  growth log entry at `[0x38a6] * 3 + 0x38cf`, undoes the two stat grants it
  records, clears it, may set the den flag (`1000:4aa5`), then
  `dec word [0x38a6]` / `sub word [0x38d0],0xa` and clamps the XP. This port
  carries no growth log, so it prints the arm's line (file `0x4CEF`) and
  leaves the fight without applying the penalty. Costs no draw, and no
  captured run reaches it: run A's cop fight is fled at level 0.
* `1000:48eb`'s `[0x3c83] == 1` arm (file `0x4C8F`, the rector refusing to
  let you run). Nothing in this port sets `[0x3c83]`, so the arm is
  unreachable rather than wrong.
* The three loot words are rolled and stored on the returned `Fighter`
  (`beer_dl`, `money`, `junk`) but **not awarded** on victory: this port's
  `Game::run_combat` tail does not yet reproduce `1000:523e`..`1000:5251`.
  Costs no draw.

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
  the readers are `1000:0d86` and `1000:0e54`, both inside `FUN_1000_0d14`,
  and between them they change the draw count and the draw values of every
  later encounter. It is `Game::flag_3693`.
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
6cb4  call 0f78:081e -> DS:0x3695    ; Read #2  BigMarket
6cc6  call 0f78:081e -> DS:0x3696    ; Read #3  Den
6cd8  call 0f78:081e -> DS:0x3697    ; Read #4  Girl
6cea  call 0f78:081e -> DS:0x3698    ; Read #5  Vet
6cfc  call 0f78:081e -> DS:0x3699    ; Read #6  Club
6d0e  call 0f78:081e -> DS:0x369a    ; Read #7  Gym
6d1b  call 0f78:07ea                 ; Close
6d20  writes '^0Загружено из places' (file 0x7CCD)
```

File order therefore equals flag-address order: **Market, BigMarket, Den, Girl,
Vet, Club, Gym**. `TRACKED` carried Vet and Den swapped at slots 2 and 4 and
has been corrected; the file's own bytes still cannot arbitrate (`orig/*.SAV`
and `orig/PLACES.SAV` are `01` in every slot), but they no longer need to.

Earlier revisions of this section and of `src/locations.rs` said the read "has
not been located" and that "locating the `BlockRead` would settle it". Both
claims were wrong: the routine exists and there is no `BlockRead`.

The failure arm at `1000:6d3b` is a **conditional** reset. It clears Vet
(`6d3b`), Market (`6d40`), Club (`6d4c`), Gym (`6d51`), Girl (`6d5d`),
BigMarket (`6d62`) and Den (`6d6e`), except that `1000:6d45`
(`cmp word [0x389c],3` / `jz 0x6d51`) skips the Club clear, `1000:6d56`
(same compare, `jz 0x6d62`) skips the Girl clear, and `1000:6d67`
(`cmp word [0x389c],5` / `jz 0x6d73`) skips the Den clear — one flag each,
not pairs. It then writes `^6Чё-то глюкануло - немогу прoгрузить Places:Ресет ту Default` (file `0x7CE3`) and
leaves via `1000:6d8c`/`1000:6da0`, never reaching `1000:6dbe`.
`[0x389c]` is the character class (Task 11b) — the skips keep the class
bonuses and clear only what was discovered. The port has no `.SAV` load path at
all, so none of this is reproduced.

## No `.SAV` load path

*Cited from `src/main.rs`.*

`orig/g.exe` runs from itself alone, so "no save file" is the ordinary
new-game case (**corroborated** by running it). Loading an existing character
is out of scope; `Save::parse` is the only constructor, and `.SAV` offsets
`0x214` (29 bytes) and `0x2ae` (8 bytes) are still unknown, so
`Game::write_save` returns `Unsupported` for every `Game` this code can build.

## No typed save verb, and no "saved" message

*Cited from `src/game.rs`'s `write_save` note.*

**Established from flow** that `sv` is not save (it sizes up the enemy — see
`src/commands.rs`). Saving in the original is checkpoint-only:
`docs/re/tables.md`'s "Other price sources" names `1000:761d` (a paid service,
`district * 50` rubles) and a second path at `0x9bcd`. Neither is a typed verb.
There is no "saved OK" / "save failed" string anywhere in `data/strings.json`,
so a wrapper could only print composed text — which is why there is none.

## `help`'s printed content

*Cited from `src/game.rs`'s `show_help`.*

**Established from flow** that `help` is dispatched at `1000:edd5`. Its handler
body was not traced, so nothing is printed rather than inventing a line: the
game has no "not implemented" string to quote. Disassembling the handler
settles it.

## `rename`'s prompts

*Cited from `src/game.rs`'s `rename`.*

`^2Звали тебя:^7 ` and `^2А теперь будут:^7 ` are **this port's own wording**
and are the one place the code knowingly departs from the byte-verbatim rule.
`1000:ecf1`'s handler body was not traced, so the real prompts are unknown.

## The vet's charged amounts

*Cited from `src/game.rs`'s `heal_jaw` / `heal_leg`.*

**Established from flow** that the menu prints `3` and `7` (files `0xB2B2`,
`0xB2D9`) and that the affordability colour compares money against the same
literals (`cmp word [0x38c7],0x3` at `1000:d410`, `cmp word [0x38c7],0x7` at
`1000:d465`). That the *debit* is also 3 and 7 is an **inference** — the vet's
own submenu handler was not traced.

## The in-combat verb set

*Cited from `src/game.rs`'s `run_combat`.*

**Corroborated** modal by the live capture (`mar` and `i` typed at `^0Битва\`
were ignored, reprinting the prompt). `sv` (inspect) is corroborated by
`docs/re/tables.md`'s oracle capture; `h`/`mh` (beer) are **established from
flow** via `FUN_1000_3d11`'s call into `FUN_1000_29c4` at `1000:4b00`. `k`
(attack) is **this port's own choice** — consistent with `k` being the fight
verb everywhere else, but not independently confirmed. `FUN_1000_3d11`'s own
input loop was not disassembled.

## Other unreproduced behaviour

* **`kl` / `trn` priced rows** — prices are not in `data/shops.json`.
* **The class-keyed combat-opener table** (`1000:3d32`..`1000:3e8a`, files
  `0x452E`, `0x453B`, `0x4548`, `0x4565`, `0x457A`, …).
* **The rector death branch and the hospital rescue** (`1000:4f8c`,
  `1000:4fce`) — need fields `crate::model::Fighter` does not have.
* **`sv`, `v`, `x`, `wes` token compare sites** — not located; those four
  verbs are corroboration-only, not dispatch-confirmed.
* **The quit message** (files `0xC3F3`, `0xC41A`, written at `1000:ee04`) and
  the university backstory (`0x7D81`..`0x7F1F`) — real strings, not wired up.
* **Shop purchase effects** — `data/shops.json` rows deduct `price` and print
  their text, but never change `strength` / `armor` / etc.: most rows have no
  representable target on `Fighter`.
* **The joint (`kos`) heal formula** reuses beer's `FUN_1000_29c4` by analogy;
  the joint's own handler was not traced.
* **The decline branch after a fight encounter.** The evade-vs-detected split
  on the `Random(2)` at `1000:b725` (`1000:b721` is its `mov ax,2`,
  `1000:b724` the `push`) is **established from flow**, but a second,
  similarly-shaped path at `1000:b691` has no roll on decline at all. Which one
  a real encounter reaches depends on `1000:b5fc`, untraced. The port always
  takes the `Random(2)` branch.
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
  `Fighter::armor` there. `data/wander.json` is a reviewed artifact this task
  did not modify, so its `globals` entry still reads `unk_38b2`; that is a
  stale name, not a disagreement.
* **The item at `DS:394d`.** Bought from the dealers for 150 roubles at
  `1000:cd05` (price byte `DS:0b3e`), and it arms the 25-walk delivery counter
  `DS:3e32` that `1000:af1d` drives. `docs/re/tables.md` calls that counter
  "the silencer"; the purchase's own name string was not traced, so
  `data/wander.json` keeps the neutral `dealer_order_placed`.
* **`1000:4aa5` sets the Den flag while printing a refusal.** The byte is
  `c6 06 96 36 01` (verified) and the line is
  `^4Такого конявого непустят в местный притон!` (file `0x4D42`); the den gate
  at `1000:d80c` reads nothing but that flag. Whether a clear was intended is
  **unverified** and cannot be settled from the binary.
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
* **`.SAV` offsets `0x2b1` and `0x2b5` are inside `unk_02ae`.** The record
  spans `DS:369c`..`DS:3952`, so `unk_02ae` (`.SAV 0x2ae`..`0x2b5`) is
  `20ae:394a`..`20ae:3951` — which makes `.SAV 0x2b1` the
  `dealer_order_placed` byte `20ae:394d` and `.SAV 0x2b5` the
  `church_visits` byte `20ae:3951`, both named in `data/wander.json`'s
  `globals`. `tests/wander_sequence.rs` reads run E's state from those two
  offsets. `data/save_layout.json` and `docs/re/save-format.md` still carry
  the whole span as one opaque `unk_02ae`; this task did not modify either
  (both are outside its file list), so the two documents disagree about a
  name, not about a byte.
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
  the `0f16:031a` delays the original spaces its phone-call gags with.
* **`Game::mage` charges but cannot save.** On the paid path the money leaves
  and no file is written, because `Game::write_save` is `Unsupported` for
  every `Game` this port can build (see "No `.SAV` load path"). No captured
  run took that path — draw 14 returned `0` once, in run A's turn 24, and the
  driver answered `n` — so this is untested against the original.
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
