//! Понтовость: XP thresholds, level-ups and the stat growth they hand out.
//!
//! Everything here is transcribed from `FUN_1000_2526` (`1000:2526`), the
//! routine the combat function calls once a kill has been credited, plus the
//! XP award at `1000:51b9`..`1000:51c8` and the character-creation block at
//! `1000:7140`..`1000:71e8`. `docs/re/progression.md` cites every address and
//! records how each number was checked against the original.
//!
//! The two words the original keeps *outside* the fighter record are modelled
//! by [`Progress`]:
//!
//! | global | `.SAV` | meaning | evidence |
//! |---|---|---|---|
//! | `DS:38ce` | `0x232` | XP not yet spent on a level | `1000:2536`, `1000:254d` |
//! | `DS:38d0` | `0x234` | XP needed for the next level | `1000:2550`, `1000:6de0` |
//!
//! `DS:389c` / `.SAV 0x200`, the class/rank index, is **not** here: it is
//! field `+0x00` of the same 16-byte record the rest of [`crate::model::Fighter`]
//! mirrors (`1000:25aa` indexes the weight table with it, `1000:712a`/
//! `1000:71b8` store it at character creation), so it lives on
//! [`crate::model::Fighter::class`] instead. This is a fix-wave-1 move: Task
//! 9b originally carried it here because `Fighter` had no class field yet.
//!
//! ## Deviation from the Task 9b brief
//!
//! The brief specified `apply_levels(f: &mut Fighter, xp: u32) -> Vec<LevelUp>`.
//! That signature cannot express what the original does: each level draws two
//! random stat increases from the character's *class* weight table
//! (`1000:25aa`, `1000:25fe`), so both the class and the generator have to
//! reach the function, and the running threshold is stored state that goes out
//! of step with the level once the level cap bites (`1000:2580`). The
//! signature here takes those inputs explicitly. `xp_to_next` and `xp_award`
//! keep the brief's signatures unchanged.

use crate::model::Fighter;
use crate::rng::Rng;

/// `1000:2580`, `cmp word [0x38a6],0x28` — the понтовость cap.
pub const MAX_LEVEL: u16 = 40;

/// `1000:287d`, `cmp word [bp-0x8],0x2` — stat increases handed out per level.
pub const GAINS_PER_LEVEL: usize = 2;

/// `1000:6de0`, `mov word [0x38d0],0xa` — a new character's first threshold.
pub const THRESHOLD_BASE: u32 = 10;

/// `1000:2550`, `add word [0x38d0],0xa` — how much each level adds to it.
pub const THRESHOLD_STEP: u32 = 10;

/// Per-class stat-growth weights, read out of the table at `DS:0002`
/// (`1000:25aa`..`1000:25b6` reads `[[0x389c] * 4 + 2]` and its three
/// siblings). Index is the class/rank word at `.SAV` `0x200`; the four bytes
/// are the weights of strength, agility, vitality and luck in that order.
///
/// The eleven rows are the eleven rank names at `DS:002e`: Дохляк, Нефор,
/// Нарк, Подтсан, Отморозок, Гопник, Вор, Беспредельщик, Мент, Маньячок,
/// Ректор НГУ. `tests/progression.rs` checks this table against
/// `data/xp.json`, which the capture tool reads straight out of `orig/g.exe`.
pub const CLASS_WEIGHTS: [[u16; 4]; 11] = [
    [1, 2, 1, 2],
    [2, 2, 2, 3],
    [2, 2, 2, 2],
    [3, 3, 3, 3],
    [5, 2, 4, 1],
    [4, 3, 3, 2],
    [3, 3, 2, 4],
    [5, 3, 4, 2],
    [5, 5, 5, 5],
    [5, 6, 8, 3],
    [0, 0, 0, 0],
];

/// The four stat answers the class prompt accepts, and the starting stats
/// each stores — `1000:7148`, `1000:7167`, `1000:7186` and the `else` at
/// `1000:71a0`, in strength/agility/vitality/luck order. Index is the answer
/// the player typed; anything outside `0..=3` is folded to `0`
/// (`1000:712d`..`1000:713b`).
pub const START_STATS: [[u16; 4]; 4] = [[3, 3, 3, 3], [5, 2, 4, 1], [4, 3, 3, 2], [3, 3, 2, 4]];

/// `1000:71b8`, `add word [0x389c],0x3` — the class prompt's answer plus this
/// is the stored class/rank index.
pub const CLASS_OF_ANSWER_OFFSET: u16 = 3;

/// One of the four stats a level-up can raise.
///
/// The discriminants are the branch order of `FUN_1000_2526`
/// (`1000:2615`, `1000:26c0`, `1000:275c`, `1000:2814`), which is also the
/// order of the weight bytes and of the codes the original writes into its
/// growth log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stat {
    Strength,
    Agility,
    Vitality,
    Luck,
}

impl Stat {
    /// The character the original records for this stat in the
    /// `array[1..40] of string[2]` growth log at `.SAV 0x236` — `'1'`..`'4'`,
    /// written next to each `+1` message (`1000:24b7`, `1000:24c8`,
    /// `1000:24da`, `1000:24e8`) and replayed in reverse by the de-level
    /// penalty at `1000:498f`..`1000:4a4e`.
    pub fn code(self) -> u8 {
        match self {
            Stat::Strength => b'1',
            Stat::Agility => b'2',
            Stat::Vitality => b'3',
            Stat::Luck => b'4',
        }
    }

    /// Inverse of [`Stat::code`]. `None` for any other byte, including the
    /// `0` an entry cleared by the de-level penalty holds (`1000:497d`).
    pub fn from_code(code: u8) -> Option<Stat> {
        match code {
            b'1' => Some(Stat::Strength),
            b'2' => Some(Stat::Agility),
            b'3' => Some(Stat::Vitality),
            b'4' => Some(Stat::Luck),
            _ => None,
        }
    }
}

/// One level gained, with what it handed out.
///
/// `gains` is `None` in a slot only when the class weights sum to zero, which
/// makes every one of the four range tests fail (`1000:2814`'s `jl` falls
/// through to the end of the loop body) and the draw grant nothing. Only
/// class 10, Ректор НГУ, has all-zero weights, and no player character can
/// hold it — see [`CLASS_WEIGHTS`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelUp {
    pub new_level: u16,
    pub hpmax_gain: u16,
    pub gains: [Option<Stat>; GAINS_PER_LEVEL],
}

/// One level's worth of growth-log codes — `string[2]`, `'1'`..`'4'`, with
/// `0` for an empty position. See [`Progress::growth_log`].
pub type GrowthEntry = [u8; GAINS_PER_LEVEL];

/// The XP bookkeeping the original keeps outside the fighter record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// `DS:38ce` / `.SAV 0x232`.
    pub xp: u32,
    /// `DS:38d0` / `.SAV 0x234`.
    pub threshold: u32,
    /// `array[1..40] of string[2]` at `.SAV 0x236`, addressed through
    /// Borland's biased base `20ae:38cf` — the real base is `20ae:38d2` and
    /// `1000:2647`..`1000:2651` reaches element `n` as `20ae:38cf + n * 3`,
    /// which is `base - 1 * 3 + n * 3`. **Established from flow.**
    ///
    /// Ten references image-wide (`python3 tools/re_query.py xrefs-to
    /// 20ae:38cf`): eight in `FUN_1000_2526` — two per stat, the read at
    /// `1000:2651`/`26f9`/`2795`/`284d` and the write-back at
    /// `1000:2670`/`2718`/`27b4`/`286c` — and two in `FUN_1000_3d11`, the
    /// flee penalty's copy at `1000:495e` and its clear at `1000:497d`. So
    /// the log is written **only** by the level-up and read **only** by the
    /// de-level, which is why it lives here and not on
    /// [`crate::model::Fighter`].
    ///
    /// Index 0 exists to keep the original's 1-based indexing: `1000:258e`
    /// raises the level *before* `1000:2647` reads it, so nothing is ever
    /// appended at 0. There are `MAX_LEVEL + 1` slots and
    /// `append_growth_code` drops anything past the last, where the
    /// original would run off the end of its array — that overrun is
    /// reachable only through the two uncapped level-ups at `1000:5094` and
    /// `1000:5145` (opponent kind 3 and 4), which this port never triggers,
    /// and reproducing a memory overwrite is not something a port should do.
    pub growth_log: [GrowthEntry; MAX_LEVEL as usize + 1],
}

impl Progress {
    /// A new character's state: `1000:6de0` sets the threshold to 10 and
    /// nothing writes the level or the XP total, both of which start at 0.
    /// The growth log is BSS and starts as 41 empty shortstrings.
    pub fn new() -> Progress {
        Progress {
            xp: 0,
            threshold: THRESHOLD_BASE,
            growth_log: [[0; GAINS_PER_LEVEL]; MAX_LEVEL as usize + 1],
        }
    }
}

/// `1000:2657`..`1000:267a` and its three siblings — append one stat code to
/// `growth_log[level]`.
///
/// The original does it as a Pascal string concatenation
/// (`rtl_str_concat` at `0f78:0b66` with the CS literal, then
/// `rtl_str_assign_max` at `0f78:0b01` with a max of **2**), so the entry
/// holds at most two codes and a third would be dropped by the truncation.
/// That is why the array element is `string[2]` and why
/// [`GAINS_PER_LEVEL`] is the same 2.
fn append_growth_code(p: &mut Progress, level: u16, stat: Stat) {
    let Some(entry) = p.growth_log.get_mut(usize::from(level)) else {
        return;
    };
    if let Some(slot) = entry.iter_mut().find(|c| **c == 0) {
        *slot = stat.code();
    }
}

impl Default for Progress {
    fn default() -> Progress {
        Progress::new()
    }
}

/// XP required to advance **from** `level` to `level + 1`.
///
/// The original does not hold a curve: it stores the current requirement in
/// `DS:38d0`, sets it to 10 for a new character (`1000:6de0`) and adds 10 per
/// level gained (`1000:2550`), so the requirement at level *n* is
/// `10 + 10 * n`. The de-level penalty subtracts the same 10
/// (`1000:4ac7`), which keeps the two in step downwards as well.
pub fn xp_to_next(level: u16) -> u32 {
    THRESHOLD_BASE + THRESHOLD_STEP * u32::from(level)
}

/// XP awarded for defeating `enemy`.
///
/// `1000:51b9`..`1000:51c8`: the sum of the enemy's four stats, printed as
/// `^6За отпин врага ты получаешь # качков опыта` (`1000:51b4`) and added to
/// `DS:38ce` at `1000:51e9`.
///
/// `player_level` is accepted because the task brief's interface names it,
/// and is deliberately unused: nothing between `1000:51b4` and `1000:51e9`
/// reads the player's level. Thirty captured kills spanning player levels 0,
/// 1, 2, 10, 11, 15, 16, 20, 21, 30, 31 and 32 all print exactly this sum —
/// see `data/xp.json`, `award_cases`.
///
/// Two callers skip the award entirely rather than scale it: `1000:51a6`
/// jumps past it when the fight was the rector or the endgame (`param_1` 3 or
/// 4), and those two paths instead force a level with `xp := threshold`
/// (`1000:508e`, `1000:513f`).
pub fn xp_award(player_level: u16, enemy: &Fighter) -> u32 {
    let _ = player_level;
    u32::from(enemy.strength)
        + u32::from(enemy.agility)
        + u32::from(enemy.vitality)
        + u32::from(enemy.luck)
}

/// The weight row for `class`, or all zeros for a class outside the table.
///
/// The original indexes `DS:(class * 4 + 2)` with no bounds check
/// (`1000:25aa`); a class past the eleventh row would read whatever follows
/// the table, which is the rank-name strings. Returning zeros instead is a
/// deliberate divergence: no reachable class exceeds 10 (`1000:712d` clamps
/// the prompt's answer to `0..=3` before adding 3), so the original's
/// behaviour there is unreachable and inventing it would be a guess.
pub fn class_weights(class: u16) -> [u16; 4] {
    CLASS_WEIGHTS
        .get(usize::from(class))
        .copied()
        .unwrap_or([0; 4])
}

/// Which stat a draw of `roll` (already `Random(sum) + 1`, so `1..=sum`) hits.
///
/// The four range tests of `FUN_1000_2526`, in order: `roll <= w0`
/// (`1000:2615`), `roll <= w0+w1` (`1000:26c0`), `roll <= w0+w1+w2`
/// (`1000:275c`), `roll <= w0+w1+w2+w3` (`1000:2814`).
fn pick(weights: [u16; 4], roll: u16) -> Option<Stat> {
    let mut edge = 0u16;
    for (i, w) in weights.iter().enumerate() {
        edge += w;
        if roll <= edge {
            return Some([Stat::Strength, Stat::Agility, Stat::Vitality, Stat::Luck][i]);
        }
    }
    None
}

/// Apply one stat increase and everything it drags along.
///
/// * strength (`1000:261d`..`1000:269d`): `dmg_max + 1`, `dmg_min + 1` when
///   the *new* strength is even, `hpmax + 1`, `hp + 1`.
/// * agility (`1000:26c5`): nothing else.
/// * vitality (`1000:2761`, `1000:27c3`): `hpmax + 5`, `hp + 5`.
/// * luck (`1000:2819`): nothing else.
///
/// Together these keep the identity the in-game help screen states
/// (`1000:610c`) and character creation establishes (`1000:71bd`):
/// `hpmax = 10 + 5 * vitality + strength`. Neither branch clamps `hp` to
/// `hpmax`; both go up by the same amount.
pub fn grant(f: &mut Fighter, stat: Stat) {
    match stat {
        Stat::Strength => {
            f.strength = f.strength.wrapping_add(1);
            f.dmg_max = f.dmg_max.wrapping_add(1);
            // `1000:2683`..`1000:2691`: idiv by 2, act on a zero remainder.
            if f.strength.is_multiple_of(2) {
                f.dmg_min = f.dmg_min.wrapping_add(1);
            }
            f.hpmax = f.hpmax.wrapping_add(1);
            f.hp = f.hp.wrapping_add(1);
        }
        Stat::Agility => f.agility = f.agility.wrapping_add(1),
        Stat::Vitality => {
            f.vitality = f.vitality.wrapping_add(1);
            f.hpmax = f.hpmax.wrapping_add(5);
            f.hp = f.hp.wrapping_add(5);
        }
        Stat::Luck => f.luck = f.luck.wrapping_add(1),
    }
}

/// Credit `award` XP and apply every level it buys, in order.
///
/// `FUN_1000_2526` (`1000:2526`) in full. Two loops, exactly as the original
/// has them:
///
/// 1. `1000:2546`..`1000:255f` drains the XP pool — `xp -= threshold;
///    threshold += 10` — counting the levels bought. This loop has no cap.
/// 2. `1000:257a`..`1000:289d` hands out one level per count: raise the level,
///    then draw [`GAINS_PER_LEVEL`] stat increases against the class weights.
///
/// `uncapped` is the routine's `param_1`. When it is `false` the second loop
/// stops the moment the level is already [`MAX_LEVEL`] (`1000:2580`), so XP
/// drained by the first loop is *lost* and the threshold keeps climbing past
/// `xp_to_next(MAX_LEVEL)` — that is why [`Progress::threshold`] is carried
/// rather than derived from the level. The combat path passes `false`
/// (`1000:5238`, `mov al,0`); the rector and endgame kills pass `true`
/// (`1000:5094`, `1000:5145`, `mov al,1`) and can push the level past 40.
///
/// Draws come from `rng` in the order the original makes them: one
/// `Random(sum of the four class weights)` per stat increase (`1000:25fe`),
/// the result incremented by one (`1000:2603`).
pub fn apply_levels(
    p: &mut Progress,
    f: &mut Fighter,
    rng: &mut Rng,
    award: u32,
    uncapped: bool,
) -> Vec<LevelUp> {
    p.xp += award;
    let mut ups = Vec::new();
    if p.xp < p.threshold {
        return ups; // 1000:2536..1000:253c
    }
    let mut levels = 0u32;
    while p.xp >= p.threshold {
        p.xp -= p.threshold;
        p.threshold += THRESHOLD_STEP;
        levels += 1;
    }

    let weights = class_weights(f.class);
    let sum: u16 = weights.iter().sum();
    for _ in 0..levels {
        if !uncapped && f.level == MAX_LEVEL {
            break;
        }
        f.level += 1;
        let hpmax_before = f.hpmax;
        let mut gains = [None; GAINS_PER_LEVEL];
        for slot in gains.iter_mut() {
            let roll = rng.below_at("1000:25fe", sum).wrapping_add(1);
            let stat = pick(weights, roll);
            if let Some(stat) = stat {
                grant(f, stat);
                // 1000:2657/26ff/279b/2853 -- the code is appended inside the
                // arm that granted the stat, so a roll that matches no range
                // (only class 10, whose weights are all zero) records
                // nothing.
                append_growth_code(p, f.level, stat);
            }
            *slot = stat;
        }
        ups.push(LevelUp {
            new_level: f.level,
            hpmax_gain: f.hpmax.wrapping_sub(hpmax_before),
            gains,
        });
    }
    ups
}

/// Take back the stat grants `growth_log[level]` records, and spend the entry
/// — `[1000:4954, 1000:4a78)`, the first half of the flee penalty.
///
/// **Established from flow**, re-derived from `orig/g.exe` for this
/// implementation. The block:
///
/// ```text
/// 4954  growth_log[level] -> the local at [bp-0x10a]   (rtl_str_assign_max, max 0xff)
/// 497d  mov byte [di+0x38cf],0        ; the SOURCE entry's length byte
/// 4982  for i := 1 to 2 do            ; 1000:4989 inc / 1000:4a6f cmp,2 / 1000:4a73 jz
/// 498f    '1' -> dec [0x389e] ... 49b3 dec [0x38aa] ... 49ca dec [0x38ae], clamp hp
/// 49e3    '2' -> dec [0x38a0]
/// 4a0c    '3' -> dec [0x38a2] ... 4a30 sub word [0x38ae],5, clamp hp
/// 4a49    '4' -> dec [0x38a4]
/// ```
///
/// The codes are the inverse of [`grant`], including the parity branch:
/// `1000:49b7`..`1000:49c4` divides the *new* strength by two and takes the
/// `dmg_min` decrement when the remainder is 1, where `1000:2683` takes the
/// increment when it is 0. So a strength that gained `dmg_min` on the way up
/// loses it on the way down, at the same crossing.
///
/// **The copy is what makes the clear safe:** `1000:497d` zeroes the source
/// *before* the loop runs, and the loop reads the copy at `[bp-0x10a]`. Two
/// divergences, both deliberate and neither reachable in play:
///
/// * the loop does not consult the copied string's length byte, so an entry
///   that was already spent would have the loop read whatever the shortstring
///   assignment left in the local's positions 1 and 2 — uninitialised stack.
///   This clears **both codes** instead of only the length, so a spent entry
///   reliably costs nothing. Reaching that state means fleeing twice at the
///   same level without levelling in between, and the flee itself decrements
///   the level, so the entry is always re-earned first.
/// * the stat decrements use `wrapping_sub`, matching [`grant`]'s
///   `wrapping_add`, rather than the original's signed word arithmetic.
///
/// Returns the codes it acted on, in order, so the caller can write the four
/// `^4… -1 ` lines the original writes between the decrements. Nothing else is
/// written inside the loop, so collecting them and printing afterwards is
/// byte-identical.
///
/// **No draw**: there is no `9a 4b 11 78 0f` anywhere in
/// `[1000:48eb, 1000:4afb)`.
pub fn undo_growth(p: &mut Progress, f: &mut Fighter) -> Vec<Stat> {
    let level = usize::from(f.level);
    let entry = p.growth_log.get(level).copied().unwrap_or_default();
    if let Some(slot) = p.growth_log.get_mut(level) {
        *slot = GrowthEntry::default();
    }
    let mut undone = Vec::new();
    for code in entry {
        // 1000:4a6f is reached by every non-matching code, so an empty
        // position costs nothing.
        let Some(stat) = Stat::from_code(code) else {
            continue;
        };
        match stat {
            Stat::Strength => {
                f.strength = f.strength.wrapping_sub(1);
                f.dmg_max = f.dmg_max.wrapping_sub(1);
                if !f.strength.is_multiple_of(2) {
                    f.dmg_min = f.dmg_min.wrapping_sub(1);
                }
                f.hpmax = f.hpmax.wrapping_sub(1);
            }
            Stat::Agility => f.agility = f.agility.wrapping_sub(1),
            Stat::Vitality => {
                f.vitality = f.vitality.wrapping_sub(1);
                f.hpmax = f.hpmax.wrapping_sub(5);
            }
            Stat::Luck => f.luck = f.luck.wrapping_sub(1),
        }
        // 1000:49ce and 1000:4a35 -- the same three-instruction clamp, only
        // after the two codes that move `hpmax`.
        if matches!(stat, Stat::Strength | Stat::Vitality) && f.hp > f.hpmax {
            f.hp = f.hpmax;
        }
        undone.push(stat);
    }
    undone
}

/// Give the level back — `1000:4ac3`..`1000:4ad9`, the last three steps of the
/// flee penalty.
///
/// ```text
/// 4ac3  ff 0e a6 38        dec [0x38a6]              ; the level
/// 4ac7  83 2e d0 38 0a     sub word [0x38d0],10      ; the threshold
/// 4acc  xp >= threshold -> xp := threshold - 1       ; 1000:4acf jl 0x4adc
/// ```
///
/// The threshold step is exactly the one `apply_levels` adds at
/// `1000:2550`, which is what keeps [`xp_to_next`] true of the pair after a
/// de-level as well as after a level-up. Both subtractions saturate here;
/// the original's are signed word arithmetic, and neither can go negative
/// from a state this port can reach — `1000:4931`'s `[0x38a6] > 0` guard is
/// what stops the level, and the threshold is `10 + 10 * level` whenever the
/// level cap has not bitten.
pub fn demote(p: &mut Progress, f: &mut Fighter) {
    f.level = f.level.saturating_sub(1);
    p.threshold = p.threshold.saturating_sub(THRESHOLD_STEP);
    if p.xp >= p.threshold {
        p.xp = p.threshold.saturating_sub(1);
    }
}

/// A freshly created character: `1000:7140`..`1000:71e8`.
///
/// `name` is stored verbatim into [`Fighter::name`]; the original prompts for
/// it separately (`^0А зовут тебя:`) and this port has no way to invent a
/// default, so a caller must supply one rather than risk shipping a
/// silently-empty name (Task 11's job; this signature just makes forgetting
/// it a compile error instead of a blank save).
///
/// `answer` is what the player typed at
/// `0-Пацан, 1-Отморозок, 2-Гопник, 3-Вор`; anything outside `0..=3` is
/// folded to `0` (`1000:712d`..`1000:713b`). The stored class is
/// `answer + 3` (`1000:71b8`), which is why a fresh Пацан is class 3
/// (Подтсан) and not class 0.
///
/// The derived fields are `hpmax = 10 + 5 * vitality + strength`
/// (`1000:71bd`..`1000:71cf`), `hp = hpmax` (`1000:71d2`),
/// `dmg_min = strength div 2` (`1000:71d8`) and `dmg_max = strength`
/// (`1000:71e4`). Level and XP start at zero and the threshold at 10
/// (`1000:6de0`).
pub fn new_character(name: &str, answer: u16) -> (Fighter, Progress) {
    let answer = if answer <= 3 { answer } else { 0 };
    let [strength, agility, vitality, luck] = START_STATS[usize::from(answer)];
    let hpmax = 10 + 5 * vitality + strength;
    let f = Fighter {
        name: name.to_string(),
        class: answer + CLASS_OF_ANSWER_OFFSET,
        level: 0,
        hp: hpmax,
        hpmax,
        strength,
        agility,
        vitality,
        luck,
        dmg_min: strength / 2,
        dmg_max: strength,
        ..Fighter::default()
    };
    (f, Progress::new())
}
