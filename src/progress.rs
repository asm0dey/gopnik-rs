//! Понтовость: XP thresholds, level-ups and the stat growth they hand out.
//!
//! The two words the original keeps *outside* the fighter record are
//! modelled by [`Progress`]:
//!
//! | field | meaning |
//! |---|---|
//! | [`Progress::xp`] | XP not yet spent on a level |
//! | [`Progress::threshold`] | XP needed for the next level |
//!
//! The class/rank index is **not** modelled here: it lives on
//! [`crate::model::Fighter::class`] instead, since it is part of the same
//! record the rest of `Fighter` mirrors.
//!
//! Each level draws two random stat increases from the character's
//! *class* weight table, so both the class and the random generator have
//! to reach the level-up function. The running threshold is stored state,
//! not derived from the level, because it goes out of step with the level
//! once the level cap bites.

use crate::model::Fighter;
use crate::rng::Rng;
use crate::term;
use crate::text;

/// The понтовость cap.
pub const MAX_LEVEL: u16 = 40;

/// Stat increases handed out per level.
pub const GAINS_PER_LEVEL: usize = 2;

/// A new character's first XP threshold.
pub const THRESHOLD_BASE: u16 = 10;

/// How much each level adds to the threshold.
pub const THRESHOLD_STEP: u16 = 10;

/// Per-class stat-growth weights. Index is the class/rank; the four bytes
/// are the weights of strength, agility, vitality and luck, in that order.
///
/// The eleven rows are the eleven rank names: Дохляк, Нефор, Нарк,
/// Подтсан, Отморозок, Гопник, Вор, Беспредельщик, Мент, Маньячок,
/// Ректор НГУ.
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
/// each stores, in strength/agility/vitality/luck order. Index is the
/// answer the player typed; anything outside `0..=3` is folded to `0`.
pub const START_STATS: [[u16; 4]; 4] = [[3, 3, 3, 3], [5, 2, 4, 1], [4, 3, 3, 2], [3, 3, 2, 4]];

/// The class prompt's answer plus 3 is the stored class/rank index.
pub const CLASS_OF_ANSWER_OFFSET: u16 = 3;

/// What opens every level-up line. It is a `Write`, not a `WriteLn`, so
/// the two stat gains land on the same line, ended afterwards by a bare
/// `WriteLn`.
pub const LEVELUP_PREFIX: &str = "^1Понтовость увеличивается: ";

/// Printed once after the whole loop, and only in the capped form. Its two
/// `#`s are filled from [`LEVELUP_TAIL_FILLS`]'s two globals, in that
/// order.
pub const LEVELUP_TAIL: &str = "^6Сейчас у тебя # качков опыта. До слеующей прокачки надо #";

/// Which globals fill [`LEVELUP_TAIL`]'s two `#`s, in order: the XP left
/// over, then the new threshold -- [`Progress::xp`] then
/// [`Progress::threshold`].
pub const LEVELUP_TAIL_FILLS: [u16; 2] = [0x38CE, 0x38D0];

/// One of the four stats a level-up can raise.
///
/// The discriminants are in the same order as the weight bytes and the
/// codes the original writes into its growth log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stat {
    Strength,
    Agility,
    Vitality,
    Luck,
}

impl Stat {
    /// The character the original records for this stat in the growth log
    /// — `'1'`..`'4'`, written next to each `+1` message, and replayed in
    /// reverse by the de-level penalty.
    pub fn code(self) -> u8 {
        match self {
            Stat::Strength => b'1',
            Stat::Agility => b'2',
            Stat::Vitality => b'3',
            Stat::Luck => b'4',
        }
    }

    /// The `Write` this stat's arm makes before it appends its code. All
    /// four keep their trailing space: the gains run together on one line,
    /// ended by a final `WriteLn`.
    pub fn message(self) -> &'static str {
        match self {
            Stat::Strength => "^1Сила +1 ",
            Stat::Agility => "^1Ловкость +1 ",
            Stat::Vitality => "^1Живучесть +1 ",
            Stat::Luck => "^1Удача +1 ",
        }
    }

    /// Inverse of [`Stat::code`]. `None` for any other byte, including the
    /// `0` an entry cleared by the de-level penalty holds.
    pub fn from_code(code: u8) -> Option<Stat> {
        match code {
            b'1' => Some(Stat::Strength),
            b'2' => Some(Stat::Agility),
            b'3' => Some(Stat::Vitality),
            b'4' => Some(Stat::Luck),
            // The chain's own miss, and it carries NO address of its own: a
            // byte none of the four compares names -- including the 0 a
            // cleared slot holds -- takes all four `jnz`s above and the loop
            // body does nothing for it. The address the four `jnz`s share as
            // their target belongs to the growth-log loop's index test, not
            // to this arm, and citing it here would name a condition this
            // construct does not evaluate.
            _ => None,
        }
    }
}

/// One level gained, with what it handed out.
///
/// `gains` is `None` in a slot only when the class weights sum to zero, so
/// the draw grants nothing. Only class 10, Ректор НГУ, has all-zero
/// weights, and no player character can hold it -- see [`CLASS_WEIGHTS`].
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
    pub xp: u16,
    pub threshold: u16,
    /// `array[1..40] of string[2]`.
    ///
    /// The log is written **only** by the level-up and read **only** by
    /// the de-level, which is why it lives here and not on
    /// [`crate::model::Fighter`].
    ///
    /// Index 0 exists to keep the original's 1-based indexing: the level
    /// is raised before the log is read, so nothing is ever appended at 0.
    /// There are `MAX_LEVEL + 1` slots, and `append_growth_code` drops
    /// anything past the last -- an overrun only the two uncapped
    /// level-ups (the rector and endgame kills) could reach, and this port
    /// never triggers them.
    pub growth_log: [GrowthEntry; MAX_LEVEL as usize + 1],
}

impl Progress {
    /// A new character's state: the threshold starts at 10 and nothing
    /// writes the level or the XP total, both of which start at 0. The
    /// growth log starts as 41 empty entries.
    pub fn new() -> Progress {
        Progress {
            xp: 0,
            threshold: THRESHOLD_BASE,
            growth_log: [[0; GAINS_PER_LEVEL]; MAX_LEVEL as usize + 1],
        }
    }
}

/// Append one stat code to `growth_log[level]`.
///
/// The entry holds at most two codes; a third would be dropped by
/// truncation. That is why [`GAINS_PER_LEVEL`] is 2.
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
/// The original does not hold a curve: it stores the current requirement,
/// sets it to 10 for a new character and adds 10 per level gained, so the
/// requirement at level *n* is `10 + 10 * n`. The de-level penalty
/// subtracts the same 10, which keeps the two in step downwards as well.
pub fn xp_to_next(level: u16) -> u16 {
    THRESHOLD_BASE + THRESHOLD_STEP * level
}

/// XP awarded for defeating `enemy`.
///
/// The sum of the enemy's four stats, printed as
/// `^6За отпин врага ты получаешь # качков опыта` and added to the XP
/// total.
///
/// `player_level` is deliberately unused: nothing in the award calculation
/// reads the player's level.
///
/// Two callers skip the award entirely rather than scale it: when the
/// fight was the rector or the endgame, those paths instead force a level
/// with `xp := threshold`.
pub fn xp_award(player_level: u16, enemy: &Fighter) -> u16 {
    let _ = player_level;
    // The four stats are summed and added as a word, so the sum wraps
    // rather than widening.
    enemy
        .strength
        .wrapping_add(enemy.agility)
        .wrapping_add(enemy.vitality)
        .wrapping_add(enemy.luck)
}

/// The weight row for `class`, or all zeros for a class outside the table.
///
/// The original indexes the table with no bounds check; a class past the
/// eleventh row would read whatever follows the table, which is the
/// rank-name strings. Returning zeros instead is a deliberate divergence:
/// no reachable class exceeds 10, since the class prompt's answer is
/// clamped to `0..=3` before deriving the class, so the original's
/// behaviour there is unreachable and inventing it would be a guess.
pub fn class_weights(class: u16) -> [u16; 4] {
    CLASS_WEIGHTS
        .get(usize::from(class))
        .copied()
        .unwrap_or([0; 4])
}

/// Which stat a draw of `roll` (already `Random(sum) + 1`, so `1..=sum`)
/// hits.
///
/// The four range tests, in order: `roll <= w0`, `roll <= w0+w1`,
/// `roll <= w0+w1+w2`, `roll <= w0+w1+w2+w3`.
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
/// * strength: `dmg_max + 1`, `dmg_min + 1` when the *new* strength is
///   even, `hpmax + 1`, `hp + 1`.
/// * agility: nothing else.
/// * vitality: `hpmax + 5`, `hp + 5`.
/// * luck: nothing else.
///
/// Together these keep the identity the in-game help screen states and
/// character creation establishes: `hpmax = 10 + 5 * vitality + strength`.
/// Neither branch clamps `hp` to `hpmax`; both go up by the same amount.
pub fn grant(f: &mut Fighter, stat: Stat) {
    match stat {
        Stat::Strength => {
            f.strength = f.strength.wrapping_add(1);
            f.dmg_max = f.dmg_max.wrapping_add(1);
            // Divide by 2 and act on a zero remainder (i.e., even).
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
/// Two loops:
///
/// 1. Drains the XP pool — `xp -= threshold; threshold += 10` — counting
///    the levels bought. This loop has no cap.
/// 2. Hands out one level per count: raise the level, then draw
///    [`GAINS_PER_LEVEL`] stat increases against the class weights.
///
/// When `uncapped` is `false` the second loop stops the moment the level is
/// already [`MAX_LEVEL`], so XP drained by the first loop is *lost* and the
/// threshold keeps climbing past `xp_to_next(MAX_LEVEL)` — that is why
/// [`Progress::threshold`] is carried rather than derived from the level.
/// The combat path passes `false`; the rector and endgame kills pass
/// `true` and can push the level past 40.
///
/// Draws come from `rng` in the order the original makes them: one
/// `Random(sum of the four class weights)` per stat increase, the result
/// incremented by one.
pub fn apply_levels(
    p: &mut Progress,
    f: &mut Fighter,
    rng: &mut Rng,
    award: u16,
    uncapped: bool,
) -> Vec<LevelUp> {
    // A word add, so it wraps.
    p.xp = p.xp.wrapping_add(award);
    let mut ups = Vec::new();
    if p.xp < p.threshold {
        return ups;
    }
    let mut levels = 0u32;
    while p.xp >= p.threshold {
        p.xp = p.xp.wrapping_sub(p.threshold);
        p.threshold = p.threshold.wrapping_add(THRESHOLD_STEP);
        levels += 1;
    }

    let weights = class_weights(f.class);
    let sum: u16 = weights.iter().sum();
    for _ in 0..levels {
        if !uncapped && f.level == MAX_LEVEL {
            break;
        }
        f.level += 1;
        // Before the weight sum is built and before either draw.
        term::print(LEVELUP_PREFIX);
        let hpmax_before = f.hpmax;
        let mut gains = [None; GAINS_PER_LEVEL];
        for slot in gains.iter_mut() {
            let roll = rng.below(sum).wrapping_add(1);
            let stat = pick(weights, roll);
            if let Some(stat) = stat {
                // Inside the arm, so a draw that matches no range prints
                // nothing, the same way it records nothing.
                term::print(stat.message());
                grant(f, stat);
                // The code is appended inside the arm that granted the
                // stat, so a roll that matches no range (only class 10,
                // whose weights are all zero) records nothing.
                append_growth_code(p, f.level, stat);
            }
            *slot = stat;
        }
        // The bare `WriteLn` that ends this level's line.
        term::println("");
        ups.push(LevelUp {
            new_level: f.level,
            hpmax_gain: f.hpmax.wrapping_sub(hpmax_before),
            gains,
        });
    }
    // The tail is the capped caller's only. It is inside the
    // `xp >= threshold` test, so the early return above is what keeps it
    // off a no-op call.
    if !uncapped {
        term::println(&text::fill(
            LEVELUP_TAIL,
            &[i64::from(p.xp), i64::from(p.threshold)],
        ));
    }
    ups
}

/// Take back the stat grants `growth_log[level]` records, and spend the
/// entry -- the first half of the flee penalty.
///
/// The codes are the inverse of [`grant`], including the parity branch: it
/// divides the *new* strength by two and takes the `dmg_min` decrement
/// when the remainder is 1, where [`grant`] takes the increment when it is
/// 0. So a strength that gained `dmg_min` on the way up loses it on the
/// way down, at the same crossing.
///
/// **The copy is what makes the clear safe:** the source entry is zeroed
/// *before* the loop runs, and the loop reads a copy. Divergences here,
/// all deliberate and none reachable in play:
///
/// * the loop does not consult the copied string's length byte, so an
///   entry that was already spent would read whatever was left over from
///   an earlier assignment -- uninitialised stack. This clears **both
///   codes** instead of only the length, so a spent entry reliably costs
///   nothing. Reaching that state means fleeing twice at the same level
///   without levelling in between, and the flee itself decrements the
///   level, so the entry is always re-earned first.
/// * the stat decrements use `wrapping_sub`, matching [`grant`]'s
///   `wrapping_add`, rather than the original's signed word arithmetic.
/// * the parity test is `!strength.is_multiple_of(2)` on a `u16`, where the
///   original does a signed division whose remainder for a negative
///   strength is never equal to 1. The two agree unless the strength's top
///   bit is set, which is unreachable here for the same reason the
///   `wrapping_sub` divergence is. [`grant`]'s own parity test carries the
///   identical caveat.
///
/// Returns the codes it acted on, in order, so the caller can write the
/// four `-1` lines the original writes between the decrements.
///
/// **No draw:** this function never touches the RNG.
pub fn undo_growth(p: &mut Progress, f: &mut Fighter) -> Vec<Stat> {
    let level = usize::from(f.level);
    let entry = p.growth_log.get(level).copied().unwrap_or_default();
    if let Some(slot) = p.growth_log.get_mut(level) {
        *slot = GrowthEntry::default();
    }
    let mut undone = Vec::new();
    for code in entry {
        // Reached by every non-matching code, so an empty position costs
        // nothing.
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
        // The same clamp, only after the two codes that move `hpmax`: both
        // store hpmax into hp only when hp is STRICTLY above it.
        if matches!(stat, Stat::Strength | Stat::Vitality) && f.hp > f.hpmax {
            f.hp = f.hpmax;
        }
        undone.push(stat);
    }
    undone
}

/// Give the level back — the last three steps of the flee penalty:
/// decrement the level by 1, subtract 10 from the threshold, and if xp is
/// now at or above the threshold, clamp it to threshold - 1.
///
/// The threshold step is exactly the one `apply_levels` adds, which is
/// what keeps [`xp_to_next`] true of the pair after a de-level as well as
/// after a level-up. Both subtractions saturate here; neither can go
/// negative from a state this port can reach — the level is guarded to
/// stay above 0, and the threshold is `10 + 10 * level` whenever the level
/// cap has not bitten.
pub fn demote(p: &mut Progress, f: &mut Fighter) {
    f.level = f.level.saturating_sub(1);
    p.threshold = p.threshold.saturating_sub(THRESHOLD_STEP);
    if p.xp >= p.threshold {
        p.xp = p.threshold.saturating_sub(1);
    }
}

/// A freshly created character.
///
/// `name` is stored verbatim into [`Fighter::name`]; the original prompts
/// for it separately (`^0А зовут тебя:`) and this port has no way to
/// invent a default, so a caller must supply one rather than risk shipping
/// a silently-empty name.
///
/// `answer` is what the player typed at
/// `0-Пацан, 1-Отморозок, 2-Гопник, 3-Вор`; anything outside `0..=3` is
/// folded to `0`. The stored class is `answer + 3`, which is why a fresh
/// Пацан is class 3 (Подтсан) and not class 0.
///
/// The derived fields are `hpmax = 10 + 5 * vitality + strength`,
/// `hp = hpmax`, `dmg_min = strength div 2` and `dmg_max = strength`.
/// Level and XP start at zero and the threshold at 10.
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
