//! Task 9b: XP thresholds, level-ups and the stat growth they hand out.
//!
//! Every expectation comes out of `data/xp.json`, which
//! `tools/capture_xp_cases.py` builds from two sources that have nothing to
//! do with this crate: constants read straight out of `orig/g.exe`'s load
//! image, and thirty kills captured from the original running under the Task
//! 3 oracle. `docs/re/progression.md` records the addresses.
//!
//! Shape note: the task brief sketched `award_cases` as
//! `{player_level, enemy_level, expected}` triples. That shape cannot check
//! anything — the award is the sum of the enemy's four *stats*
//! (`1000:51b9`), and neither level enters it — so the captured cases carry
//! the enemy's whole record instead, and `player_level` is kept as what it
//! now shows: that the award does not move with it.

use gopnik::model::Fighter;
use gopnik::progress::{
    apply_levels, class_weights, grant, new_character, xp_award, xp_to_next, Progress, Stat,
    CLASS_WEIGHTS, GAINS_PER_LEVEL, MAX_LEVEL, START_STATS, THRESHOLD_STEP,
};
use gopnik::rng::Rng;
use gopnik::save::Save;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Clone)]
struct Record {
    strength: u16,
    agility: u16,
    vitality: u16,
    luck: u16,
    level: u16,
    dmg_min: u16,
    dmg_max: u16,
    hp: u16,
    hpmax: u16,
}

impl Record {
    fn build(&self) -> Fighter {
        Fighter {
            name: "captured".into(),
            level: self.level,
            hp: self.hp,
            hpmax: self.hpmax,
            strength: self.strength,
            agility: self.agility,
            vitality: self.vitality,
            luck: self.luck,
            dmg_min: self.dmg_min,
            dmg_max: self.dmg_max,
            ..Default::default()
        }
    }
}

#[derive(Deserialize)]
struct AwardCase {
    run: String,
    frame: usize,
    player_level: u16,
    enemy: Record,
    expected: u32,
}

#[derive(Deserialize)]
struct LevelUpCase {
    run: String,
    frame: usize,
    enemy: Record,
    player_before: Record,
    player_after: Record,
    xp_before: u32,
    threshold_before: u32,
    level_before: u16,
    award_printed: u32,
    xp_after: u32,
    threshold_after: u32,
    level_after: u16,
    levels_announced: usize,
    gains_announced: Vec<String>,
}

#[derive(Deserialize)]
struct StatEvent {
    name: String,
    flag_save_offset: usize,
    deltas: BTreeMap<String, i64>,
}

#[derive(Deserialize)]
struct Xp {
    max_level: u16,
    gains_per_level: usize,
    thresholds: Vec<u32>,
    threshold_provenance: Vec<String>,
    award_cases: Vec<AwardCase>,
    level_up_cases: Vec<LevelUpCase>,
    class_weights: Vec<Vec<u16>>,
    start_stats: BTreeMap<String, Vec<u16>>,
    class_of_answer_offset: u16,
    post_kill_stat_events: Vec<StatEvent>,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn xp() -> Xp {
    let p = root().join("data").join("xp.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn stat_of(name: &str) -> Stat {
    match name {
        "strength" => Stat::Strength,
        "agility" => Stat::Agility,
        "vitality" => Stat::Vitality,
        "luck" => Stat::Luck,
        other => panic!("unknown stat {other:?} in data/xp.json"),
    }
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
        dmg_min: 1,
        dmg_max: 2,
        ..Default::default()
    }
}

// --- the curve -------------------------------------------------------------

#[test]
fn thresholds_match_original() {
    let x = xp();
    assert!(
        x.thresholds.len() >= 10,
        "need thresholds for levels 1..=10"
    );
    assert_eq!(x.thresholds.len(), x.threshold_provenance.len());
    for (i, want) in x.thresholds.iter().enumerate() {
        assert_eq!(
            xp_to_next(i as u16 + 1),
            *want,
            "threshold for level {}",
            i + 1
        );
    }
}

#[test]
fn thresholds_are_monotonic() {
    let x = xp();
    for w in x.thresholds.windows(2) {
        assert!(w[1] >= w[0], "curve must not decrease: {w:?}");
    }
}

/// A fresh character owes 10, which the brief's `thresholds` array (level 1
/// onwards) has no slot for. `1000:6de0`, and the oracle: the game shows a
/// level-0 character `До слеующей прокачки надо 10`.
#[test]
fn fresh_character_owes_ten() {
    assert_eq!(xp_to_next(0), 10);
    let (_, p) = new_character(0);
    assert_eq!(p.threshold, 10);
    assert_eq!(p.xp, 0);
}

/// Levels the capture never reached are marked in the artifact rather than
/// quietly presented as observed. This asserts the marking is real, not that
/// the values are, and pins how much of the curve is actually witnessed.
#[test]
fn threshold_provenance_is_honest() {
    let x = xp();
    let observed = x
        .threshold_provenance
        .iter()
        .filter(|p| p.starts_with("observed:"))
        .count();
    let unverified = x
        .threshold_provenance
        .iter()
        .filter(|p| p.contains("UNVERIFIED"))
        .count();
    assert_eq!(observed + unverified, x.thresholds.len());
    assert!(
        observed >= 11,
        "expected at least 11 levels witnessed by a run or a shipped save, got {observed}"
    );
}

// --- the award -------------------------------------------------------------

#[test]
fn awards_match_original() {
    let x = xp();
    assert!(!x.award_cases.is_empty(), "need captured award cases");
    for c in &x.award_cases {
        assert_eq!(
            xp_award(c.player_level, &c.enemy.build()),
            c.expected,
            "award in {} frame {} (player level {})",
            c.run,
            c.frame,
            c.player_level
        );
    }
}

/// The captured cases span player levels 0..32 against enemies of every
/// level; this pins the same claim directly.
#[test]
fn award_ignores_player_level() {
    let enemy = dummy(7);
    let want = xp_award(0, &enemy);
    for level in 0..=MAX_LEVEL {
        assert_eq!(xp_award(level, &enemy), want);
    }
    assert_eq!(want, 16);
}

// --- levelling -------------------------------------------------------------

#[test]
fn insufficient_xp_grants_nothing() {
    let (mut f, mut p) = new_character(0);
    let mut rng = Rng::new(1);
    let ups = apply_levels(&mut p, &mut f, &mut rng, 0, false);
    assert!(ups.is_empty());
    assert_eq!(f.level, 0);
    assert_eq!(p.xp, 0);
    assert_eq!(p.threshold, 10);
}

#[test]
fn multiple_levels_apply_in_one_go() {
    let (mut f, mut p) = new_character(0);
    let mut rng = Rng::new(1);
    let huge = xp_to_next(0) + xp_to_next(1) + xp_to_next(2);
    let ups = apply_levels(&mut p, &mut f, &mut rng, huge, false);
    assert_eq!(ups.len(), 3, "expected 3 level-ups, got {}", ups.len());
    assert_eq!(f.level, 3);
    assert_eq!(p.xp, 0);
    assert_eq!(p.threshold, xp_to_next(3));
    for (i, up) in ups.iter().enumerate() {
        assert_eq!(up.new_level, i as u16 + 1, "levels must be sequential");
        assert!(up.gains.iter().all(|g| g.is_some()));
    }
}

/// `1000:2580`: the second loop refuses to raise a level that is already 40,
/// but the first loop has already run and raised the threshold. So at the cap
/// the level and the threshold come apart — which is why `Progress` carries
/// the threshold instead of deriving it from the level.
#[test]
fn level_cap_stops_the_level_but_not_the_threshold() {
    let (mut f, mut p) = new_character(0);
    f.level = MAX_LEVEL;
    p.threshold = xp_to_next(MAX_LEVEL);
    let mut rng = Rng::new(1);
    let award = p.threshold;
    let ups = apply_levels(&mut p, &mut f, &mut rng, award, false);
    assert!(ups.is_empty());
    assert_eq!(f.level, MAX_LEVEL);
    assert_eq!(p.xp, 0);
    assert_eq!(p.threshold, xp_to_next(MAX_LEVEL) + THRESHOLD_STEP);
    assert_ne!(p.threshold, xp_to_next(f.level));
}

/// `1000:5094` / `1000:5145`: the rector and endgame kills pass `param_1 = 1`
/// and are not stopped by the cap.
#[test]
fn uncapped_level_up_passes_forty() {
    let (mut f, mut p) = new_character(0);
    f.level = MAX_LEVEL;
    p.threshold = xp_to_next(MAX_LEVEL);
    let mut rng = Rng::new(1);
    let award = p.threshold;
    let ups = apply_levels(&mut p, &mut f, &mut rng, award, true);
    assert_eq!(ups.len(), 1);
    assert_eq!(f.level, MAX_LEVEL + 1);
}

/// Replays each captured kill's XP bookkeeping. The generator state at the
/// moment of each level-up was not captured, so the draws cannot be replayed;
/// what is checked here is the arithmetic — XP total, threshold and level
/// before and after — plus how many level-ups the screen announced.
#[test]
fn captured_level_ups_replay() {
    let x = xp();
    assert!(!x.level_up_cases.is_empty());
    for c in &x.level_up_cases {
        let mut f = c.player_before.build();
        let mut p = Progress {
            xp: c.xp_before,
            threshold: c.threshold_before,
            class: 3,
        };
        assert_eq!(f.level, c.level_before, "{} frame {}", c.run, c.frame);
        assert_eq!(
            xp_award(c.level_before, &c.enemy.build()),
            c.award_printed,
            "{} frame {}",
            c.run,
            c.frame
        );
        let mut rng = Rng::new(0);
        let ups = apply_levels(&mut p, &mut f, &mut rng, c.award_printed, false);
        assert_eq!(ups.len(), c.levels_announced, "{} frame {}", c.run, c.frame);
        assert_eq!(p.xp, c.xp_after, "xp after, {} frame {}", c.run, c.frame);
        assert_eq!(
            p.threshold, c.threshold_after,
            "threshold after, {} frame {}",
            c.run, c.frame
        );
        assert_eq!(
            f.level, c.level_after,
            "level after, {} frame {}",
            c.run, c.frame
        );
        assert_eq!(
            c.gains_announced.len(),
            c.levels_announced * GAINS_PER_LEVEL,
            "{} frame {}",
            c.run,
            c.frame
        );
    }
}

/// The stats the screen announced, applied through `grant`, must land on the
/// stats the guest's own memory held after the kill. This is what pins the
/// side effects of each stat — the damage and HP the original drags along
/// with strength and vitality — against the original rather than against the
/// listing.
#[test]
fn captured_gains_reproduce_the_stats() {
    let x = xp();
    let mut with_levels = 0;
    for c in &x.level_up_cases {
        let mut f = c.player_before.build();
        for name in &c.gains_announced {
            grant(&mut f, stat_of(name));
        }
        if !c.gains_announced.is_empty() {
            with_levels += 1;
        }
        let after = &c.player_after;
        let where_ = format!("{} frame {}", c.run, c.frame);
        assert_eq!(f.strength, after.strength, "strength, {where_}");
        assert_eq!(f.agility, after.agility, "agility, {where_}");
        assert_eq!(f.vitality, after.vitality, "vitality, {where_}");
        assert_eq!(f.luck, after.luck, "luck, {where_}");
        assert_eq!(f.hpmax, after.hpmax, "hpmax, {where_}");
        assert_eq!(f.dmg_min, after.dmg_min, "dmg_min, {where_}");
        assert_eq!(f.dmg_max, after.dmg_max, "dmg_max, {where_}");
    }
    assert!(with_levels >= 6, "only {with_levels} captured level-ups");
}

/// `hpmax = 10 + 5 * vitality + strength` is not an extra rule: it is what
/// character creation sets up and what `grant` maintains.
#[test]
fn growth_preserves_the_hpmax_identity() {
    for answer in 0..=3u16 {
        let (mut f, mut p) = new_character(answer);
        let mut rng = Rng::new(u32::from(answer) * 7 + 1);
        for _ in 0..200 {
            assert_eq!(f.hpmax, 10 + 5 * f.vitality + f.strength);
            let award = p.threshold;
            apply_levels(&mut p, &mut f, &mut rng, award, true);
        }
        assert_eq!(f.hpmax, 10 + 5 * f.vitality + f.strength);
    }
}

// --- the tables, against the artifact --------------------------------------

#[test]
fn tables_match_the_binary() {
    let x = xp();
    assert_eq!(x.max_level, MAX_LEVEL);
    assert_eq!(x.gains_per_level, GAINS_PER_LEVEL);
    assert_eq!(x.class_of_answer_offset, 3);
    assert_eq!(x.class_weights.len(), CLASS_WEIGHTS.len());
    for (i, row) in x.class_weights.iter().enumerate() {
        assert_eq!(row.as_slice(), CLASS_WEIGHTS[i].as_slice(), "class {i}");
        assert_eq!(class_weights(i as u16).as_slice(), row.as_slice());
    }
    assert_eq!(x.start_stats.len(), START_STATS.len());
    for (answer, stats) in &x.start_stats {
        let i: usize = answer.parse().unwrap();
        assert_eq!(
            stats.as_slice(),
            START_STATS[i].as_slice(),
            "answer {answer}"
        );
    }
}

/// A class outside the table gets zeros, and a zero-weight class gains
/// nothing at all — see `class_weights`.
#[test]
fn zero_weight_class_gains_nothing() {
    assert_eq!(class_weights(10), [0, 0, 0, 0]);
    assert_eq!(class_weights(11), [0, 0, 0, 0]);
    let (mut f, mut p) = new_character(0);
    p.class = 10;
    let before = f.clone();
    let mut rng = Rng::new(5);
    let ups = apply_levels(&mut p, &mut f, &mut rng, 10, false);
    assert_eq!(ups.len(), 1);
    assert_eq!(ups[0].gains, [None, None]);
    assert_eq!(ups[0].hpmax_gain, 0);
    assert_eq!(f.level, 1);
    assert_eq!(
        (f.strength, f.agility, f.vitality, f.luck),
        (
            before.strength,
            before.agility,
            before.vitality,
            before.luck
        )
    );
}

#[test]
fn stat_codes_round_trip() {
    for s in [Stat::Strength, Stat::Agility, Stat::Vitality, Stat::Luck] {
        assert_eq!(Stat::from_code(s.code()), Some(s));
    }
    assert_eq!(Stat::from_code(b'0'), None);
    assert_eq!(Stat::from_code(0), None);
    assert_eq!(
        [b'1', b'2', b'3', b'4'],
        [
            Stat::Strength.code(),
            Stat::Agility.code(),
            Stat::Vitality.code(),
            Stat::Luck.code()
        ]
    );
}

// --- the whole thing, against a shipped save -------------------------------

/// Rebuild `SAVE_R0`'s character from nothing but the original's own rules
/// and the original's own data, and land on the stats the file holds.
///
/// The save records, per level, which two stats that level granted, as an
/// `array[1..40] of string[2]` of the codes `'1'`..`'4'` at offset `0x236`
/// (`1000:2641`..`1000:267a` writes it). So: start the class the save's rank
/// index names, replay those 30 grants, then apply the one-shot post-kill
/// events whose flag bytes the save has set — and every stat has to match.
///
/// Nothing here is fitted. The growth log, the flags and the target stats are
/// bytes in a file that shipped with the game in 2003; the rules are read out
/// of the executable.
#[test]
fn save_r0_rebuilds_from_its_growth_log() {
    let x = xp();
    let blob = std::fs::read(root().join("orig").join("SAVE_R0.SAV")).unwrap();
    let save = Save::parse(&blob).unwrap();
    let class = save.stats[0];
    let level = save.stats[5];
    assert_eq!((class, level), (4, 15));

    // The class prompt's answer that stores this class (1000:71b8).
    let (mut f, _) = new_character(class - x.class_of_answer_offset);

    // The growth log: .SAV 0x236, three bytes per level (a length byte and
    // two stat codes), indexed from 1.
    let log_at = 0x236 - gopnik::save::OFF_TAIL;
    let mut grants = 0;
    for lvl in 1..=level as usize {
        let at = log_at + (lvl - 1) * 3;
        assert_eq!(save.tail[at], 2, "growth log entry {lvl} is not two codes");
        for code in &save.tail[at + 1..at + 3] {
            let stat = Stat::from_code(*code)
                .unwrap_or_else(|| panic!("growth log entry {lvl} holds byte {code:#04x}"));
            grant(&mut f, stat);
            grants += 1;
        }
        f.level += 1;
    }
    assert_eq!(grants, level as usize * GAINS_PER_LEVEL);
    assert_eq!(f.level, level);

    // The one-shot post-kill grants this character has already collected.
    let mut fired = 0;
    for ev in &x.post_kill_stat_events {
        if blob[ev.flag_save_offset] == 0 {
            continue;
        }
        fired += 1;
        for (field, delta) in &ev.deltas {
            let d = u16::try_from(*delta).unwrap();
            match field.as_str() {
                "strength" => f.strength += d,
                "agility" => f.agility += d,
                "vitality" => f.vitality += d,
                "luck" => f.luck += d,
                "hpmax" => f.hpmax += d,
                // hp and the damage range are not asserted below: HP moves
                // with every blow and the damage range carries a weapon
                // bonus this reconstruction does not model.
                "hp" | "dmg_min" | "dmg_max" => {}
                other => panic!("unhandled field {other:?} in {}", ev.name),
            }
        }
    }
    assert_eq!(
        fired, 3,
        "SAVE_R0 should have three of the one-shot flags set"
    );

    assert_eq!(
        [f.strength, f.agility, f.vitality, f.luck],
        [save.stats[1], save.stats[2], save.stats[3], save.stats[4]],
        "rebuilt stats"
    );
    assert_eq!(f.hpmax, save.hpmax, "rebuilt hpmax");
    assert_eq!(f.hpmax, 10 + 5 * f.vitality + f.strength);
}

/// The five shipped saves, against the curve and against the HP identity.
///
/// `SAVE_R2` and `SAVE_R4` sit 2 below `10 + 5*vitality + strength`. That is
/// not a hole in the identity: both have the temporary `+2 strength` buff
/// running (`1000:4b57` grants it and sets the countdown at `.SAV 0x231`;
/// `1000:aeb3` takes it back when the countdown runs out), and neither site
/// touches `hpmax`. The two saves with a live countdown are exactly the two
/// that are 2 low.
#[test]
fn reference_saves_agree_with_the_curve() {
    let mut checked = 0;
    let mut buffed = 0;
    for name in ["SAVE_R0", "SAVE_R2", "SAVE_R3", "SAVE_R4", "SAVE_R5"] {
        let blob = std::fs::read(root().join("orig").join(format!("{name}.SAV"))).unwrap();
        let save = Save::parse(&blob).unwrap();
        let level = save.stats[5];
        let threshold = u16::from_le_bytes([blob[0x234], blob[0x235]]);
        assert_eq!(
            u32::from(threshold),
            xp_to_next(level),
            "{name}: level {level} threshold"
        );

        let buff = u16::from(blob[0x231] != 0) * 2;
        buffed += usize::from(buff > 0);
        assert_eq!(
            save.hpmax,
            10 + 5 * save.stats[3] + save.stats[1] - buff,
            "{name}: hpmax against 10 + 5*vitality + strength (buff {buff})"
        );
        checked += 1;
    }
    assert_eq!(checked, 5);
    assert_eq!(buffed, 2, "SAVE_R2 and SAVE_R4 carry the +2 strength buff");
}
