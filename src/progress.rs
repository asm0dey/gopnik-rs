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
//! | `DS:389c` | `0x200` | class / rank index | `1000:25aa`, `1000:712a` |
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

/// The XP bookkeeping the original keeps outside the fighter record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// `DS:38ce` / `.SAV 0x232`.
    pub xp: u32,
    /// `DS:38d0` / `.SAV 0x234`.
    pub threshold: u32,
    /// `DS:389c` / `.SAV 0x200`, indexing [`CLASS_WEIGHTS`].
    pub class: u16,
}

impl Progress {
    /// A new character's state: `1000:6de0` sets the threshold to 10 and
    /// nothing writes the level or the XP total, both of which start at 0.
    pub fn new(class: u16) -> Progress {
        Progress {
            xp: 0,
            threshold: THRESHOLD_BASE,
            class,
        }
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

    let weights = class_weights(p.class);
    let sum: u16 = weights.iter().sum();
    for _ in 0..levels {
        if !uncapped && f.level == MAX_LEVEL {
            break;
        }
        f.level += 1;
        let hpmax_before = f.hpmax;
        let mut gains = [None; GAINS_PER_LEVEL];
        for slot in gains.iter_mut() {
            let roll = rng.below(sum).wrapping_add(1);
            let stat = pick(weights, roll);
            if let Some(stat) = stat {
                grant(f, stat);
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

/// A freshly created character: `1000:7140`..`1000:71e8`.
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
pub fn new_character(answer: u16) -> (Fighter, Progress) {
    let answer = if answer <= 3 { answer } else { 0 };
    let [strength, agility, vitality, luck] = START_STATS[usize::from(answer)];
    let hpmax = 10 + 5 * vitality + strength;
    let f = Fighter {
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
    (f, Progress::new(answer + CLASS_OF_ANSWER_OFFSET))
}
