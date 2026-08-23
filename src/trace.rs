//! `--trace-deterministic`: everything the port believes about the numbers
//! the game was *built with*, in a form `tools/difftest.py` can diff against
//! the same numbers read straight out of `orig/g.exe`.
//!
//! ## Why this is a separate output and not a screen scrape
//!
//! The port already replays the original's `Random` stream draw for draw
//! (`data/rng_trace.json`, 1387 captured draws, `tests/wander_sequence.rs`)
//! and its per-turn guest state (`data/state_trace.json`, 91 samples of 35
//! variables). Those two oracles cover every number the game *computes*.
//! What neither of them observes is the numbers the game was *authored*
//! with -- a shop price, a level threshold, a class's opening stat line --
//! because nothing in a captured run has to exercise them. This stream is
//! exactly that residue, and nothing else: no RNG, no turn state, no screen
//! layout.
//!
//! ## The format
//!
//! One record per line, `<kind> <field> ...`, ASCII field separators, game
//! text last on the line so it can contain spaces. Every text field has its
//! `^N` colour markup removed by [`crate::text::strip`] before it is
//! written: the markup is not content, so it is not printed here and it is
//! not compared.
//!
//! Records are emitted in a fixed order and `tools/difftest.py` compares the
//! two streams as ordered lists, so a record appearing, vanishing or moving
//! is a difference like any other.
//!
//! ```text
//! scalar        <name> <value>
//! xp_threshold  <level> <xp needed to leave that level>
//! class_weights <class> <str> <agi> <vit> <luck>
//! start_stats   <answer> <stored class> <str> <agi> <vit> <luck>
//! levelup_gain  <stat> <field> <delta> <always|conditional>
//! item          <bonus> <name>
//! priced_row    <shop> <key> <price> <displayed> <text>
//! menu_order    <shop> <comma-separated keys>
//! imm_row_site  <shop> <key> <address of the cmp carrying the price>
//! ```
//!
//! `levelup_gain` rows are sorted by field name inside each stat rather than
//! left in the original's instruction order: this side derives them by
//! applying [`crate::progress::grant`] and diffing the record, which cannot
//! see an order. Instruction order is therefore **not** compared -- see
//! `docs/re/difftest.md`.

use std::io::{self, Write};

use crate::data;
use crate::game::IMM_ROWS;
use crate::model::Fighter;
use crate::progress::{
    self, Stat, CLASS_WEIGHTS, GAINS_PER_LEVEL, MAX_LEVEL, THRESHOLD_BASE, THRESHOLD_STEP,
};
use crate::text;

/// Reads one field of a fighter record as a signed number, so a grant's
/// effect on it is a subtraction rather than eight hand-written cases.
type Observe = fn(&Fighter) -> i64;

/// The record fields [`crate::progress::grant`] is allowed to move, and the
/// name each is reported under. The names match `docs/re/progression.md`'s
/// record table, which is what `tools/difftest.py` maps the original's
/// `20ae:389c`-relative addresses onto.
const OBSERVED: [(&str, Observe); 8] = [
    ("strength", |f| i64::from(f.strength)),
    ("agility", |f| i64::from(f.agility)),
    ("vitality", |f| i64::from(f.vitality)),
    ("luck", |f| i64::from(f.luck)),
    ("dmg_min", |f| i64::from(f.dmg_min)),
    ("dmg_max", |f| i64::from(f.dmg_max)),
    ("hp", |f| i64::from(f.hp)),
    ("hpmax", |f| i64::from(f.hpmax)),
];

/// The two probes one stat grant is measured from.
///
/// `grant` has exactly one conditional effect -- strength's `dmg_min + 1`,
/// which fires when the *new* strength is even (`1000:2683`..`1000:2691`) --
/// so measuring each stat from a base whose strength is even and again from
/// one whose strength is odd separates "always" from "conditional" without
/// this module having to restate the predicate. The predicate itself is
/// pinned by `tests/progression.rs`, not here.
fn probe(strength: u16) -> Fighter {
    Fighter {
        name: String::new(),
        class: 0,
        level: 0,
        strength,
        agility: 10,
        vitality: 10,
        luck: 10,
        hp: 60,
        hpmax: 60,
        dmg_min: strength / 2,
        dmg_max: strength,
        ..Fighter::default()
    }
}

/// Every field `grant(stat)` moves, as `(field, delta, conditional)`.
fn gains(stat: Stat) -> Vec<(&'static str, i64, bool)> {
    let mut out = Vec::new();
    for (name, get) in OBSERVED {
        // Strength 10 -> 11 is odd, strength 11 -> 12 is even, so the two
        // runs straddle `1000:2691`'s guard whichever way it points.
        let mut deltas = [0i64; 2];
        for (i, base_str) in [10u16, 11u16].into_iter().enumerate() {
            let before = probe(base_str);
            let mut after = before.clone();
            progress::grant(&mut after, stat);
            deltas[i] = get(&after) - get(&before);
        }
        let (a, b) = (deltas[0], deltas[1]);
        if a == 0 && b == 0 {
            continue;
        }
        // A conditional gain shows up as a delta in one probe and not the
        // other; report the non-zero one as the amount it adds when it does
        // fire.
        out.push((name, a.max(b), a != b));
    }
    out.sort_by(|x, y| x.0.cmp(y.0));
    out
}

/// Write the whole record stream to `out`.
pub fn emit(out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "scalar max_level {MAX_LEVEL}")?;
    writeln!(out, "scalar gains_per_level {GAINS_PER_LEVEL}")?;
    writeln!(out, "scalar threshold_base {THRESHOLD_BASE}")?;
    writeln!(out, "scalar threshold_step {THRESHOLD_STEP}")?;

    for level in 0..=MAX_LEVEL {
        writeln!(out, "xp_threshold {level} {}", progress::xp_to_next(level))?;
    }

    for (class, w) in CLASS_WEIGHTS.iter().enumerate() {
        writeln!(
            out,
            "class_weights {class} {} {} {} {}",
            w[0], w[1], w[2], w[3]
        )?;
    }

    for answer in 0..=3u16 {
        let (f, _) = progress::new_character("", answer);
        writeln!(
            out,
            "start_stats {answer} {} {} {} {} {}",
            f.class, f.strength, f.agility, f.vitality, f.luck
        )?;
    }

    for (stat, name) in [
        (Stat::Strength, "strength"),
        (Stat::Agility, "agility"),
        (Stat::Vitality, "vitality"),
        (Stat::Luck, "luck"),
    ] {
        for (field, delta, conditional) in gains(stat) {
            let how = if conditional { "conditional" } else { "always" };
            writeln!(out, "levelup_gain {name} {field} {delta} {how}")?;
        }
    }

    for item in data::items() {
        writeln!(out, "item {} {}", item.bonus, text::strip(item.name))?;
    }

    for tag in ["mar", "bmar"] {
        for row in data::shops().iter().filter(|r| r.shop == tag) {
            writeln!(
                out,
                "priced_row {tag} {} {} {} {}",
                row.key,
                row.price,
                row.displayed_price,
                text::strip(row.text)
            )?;
        }
    }
    for tag in ["rep", "kl", "trn"] {
        for row in IMM_ROWS.iter().filter(|r| r.shop == tag) {
            writeln!(
                out,
                "priced_row {tag} {} {} {} {}",
                row.key,
                row.price,
                row.price,
                text::strip(row.text)
            )?;
        }
    }

    for tag in ["mar", "bmar"] {
        let keys: Vec<&str> = data::shops()
            .iter()
            .filter(|r| r.shop == tag)
            .map(|r| r.key)
            .collect();
        writeln!(out, "menu_order {tag} {}", keys.join(","))?;
    }
    for tag in ["rep", "kl", "trn"] {
        let keys: Vec<&str> = IMM_ROWS
            .iter()
            .filter(|r| r.shop == tag)
            .map(|r| r.key)
            .collect();
        writeln!(out, "menu_order {tag} {}", keys.join(","))?;
    }

    // The address each immediate-priced row's price is written down at.
    // Emitted so it is *compared* rather than merely quoted: `difftest.py`
    // finds these nine addresses by scanning, and a citation nothing checks
    // is exactly the kind that drifts.
    for tag in ["rep", "kl", "trn"] {
        for row in IMM_ROWS.iter().filter(|r| r.shop == tag) {
            writeln!(out, "imm_row_site {tag} {} {}", row.key, row.site)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream() -> Vec<String> {
        let mut buf = Vec::new();
        emit(&mut buf).expect("emit into a Vec cannot fail");
        String::from_utf8(buf)
            .expect("the stream is UTF-8")
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn no_colour_markup_survives_into_the_stream() {
        for line in stream() {
            assert!(
                !line.contains('^'),
                "colour markup reached the trace stream: {line}"
            );
        }
    }

    /// The literal numbering each menu prints, per `docs/re/difftest.md`'s
    /// "The 27 priced rows".
    ///
    /// Deliberately NOT re-derived from the `priced_row` lines this same
    /// emitter just wrote: [`emit`] builds both from one walk of
    /// `data::shops()` / [`IMM_ROWS`], so comparing the two would be a list
    /// against itself and could not fail except on a field-splitting bug.
    /// Written out, it moves when a row is dropped, added or reordered.
    #[test]
    fn every_menu_order_is_the_numbering_that_shop_prints() {
        let lines = stream();
        let want = [
            ("mar", "1,2,3,4,5,6,7,8,9"),
            ("bmar", "1,2,3,4,5,6,7,8,9"),
            ("rep", "h,r"),
            ("kl", "1,2"),
            ("trn", "1,2,3,4,5"),
        ];
        for (tag, order) in want {
            let got = lines
                .iter()
                .find_map(|l| l.strip_prefix(&format!("menu_order {tag} ")))
                .unwrap_or_else(|| panic!("no menu_order for {tag}"));
            assert_eq!(got, order, "{tag}");
        }
        let emitted = lines
            .iter()
            .filter(|l| l.starts_with("menu_order "))
            .count();
        assert_eq!(emitted, want.len(), "an unexpected menu_order was emitted");
    }

    #[test]
    fn the_only_conditional_gain_is_strengths_dmg_min() {
        let conditional: Vec<String> = stream()
            .into_iter()
            .filter(|l| l.starts_with("levelup_gain") && l.ends_with(" conditional"))
            .collect();
        assert_eq!(
            conditional,
            vec!["levelup_gain strength dmg_min 1 conditional".to_string()]
        );
    }

    #[test]
    fn the_stream_is_stable_across_calls() {
        assert_eq!(stream(), stream());
    }
}
