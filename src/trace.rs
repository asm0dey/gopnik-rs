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
//! Two later groups append to the same stream without moving anything above
//! them -- the endings (`docs/re/port-gaps.md` rows 2, 4, 7, 13, 17, 21) and
//! the opening (rows 1, 6, 8, 10, 15, 16):
//!
//! ```text
//! ending_line       <tag> <i> <text>
//! errand_award      <district multiplier> <text>
//! end_banner        indent|death_colour|victory_colour <value>
//! banner_row        <i> <row>
//! marquee           indent|phases|word <value>
//! boss_stats        <id> <11 fields>
//! opening_line      <tag> <i> <text>
//! opening_gap       <tag> <i> <B|K events between line i-1 and line i>
//! help_district_line <index of the one line `1000:61c0` gates>
//! help_fragment     <i> <CS literal of a composed `help` line>
//! help_weight_line  <i> <weight index> <text>
//! ```
//!
//! A third group appends after those -- the church and the level-up
//! announcements (rows 3, 14, 18, 20 and row 19's share):
//!
//! ```text
//! church_line     <tag> <i> <text>
//! church_gap      <tag> <i> <B|K|C events between line i-1 and line i>
//! church_fragment <tag> <i> <CS literal of a composed church line>
//! levelup_write   <i> <text of a `Write`, not a `WriteLn`>
//! levelup_gap     <i> <B events in that span>
//! levelup_tail    <first fill global> <second> <text>
//! ```
//!
//! A fourth group appends after those -- `run`'s own extra line, bucket 1's
//! district lines and bucket 4's flavour turn (rows 22, 12 and 11, plus row
//! 19's remaining share):
//!
//! ```text
//! wander_line     <tag> <i> <text>
//! wander_gap      <tag> <i> <B|K|C events between line i-1 and line i>
//! wander_fragment <tag> <i> <CS literal of a composed wander line>
//! ```
//!
//! `levelup_gain` rows are sorted by field name inside each stat rather than
//! left in the original's instruction order: this side derives them by
//! applying [`crate::progress::grant`] and diffing the record, which cannot
//! see an order. Instruction order is therefore **not** compared -- see
//! `docs/re/difftest.md`.

use std::io::{self, Write};

use crate::church;
use crate::data;
use crate::ending;
use crate::game::IMM_ROWS;
use crate::model::Fighter;
use crate::opening;
use crate::progress::{
    self, Stat, CLASS_WEIGHTS, GAINS_PER_LEVEL, MAX_LEVEL, THRESHOLD_BASE, THRESHOLD_STEP,
};
use crate::text;
use crate::wander;

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

    endings(out)?;
    opening_records(out)?;
    church_records(out)?;
    wander_records(out)?;

    Ok(())
}

/// The church and the level-up announcements -- `docs/re/port-gaps.md` rows
/// 3, 18, 20, 14 and row 19's share, landed in Phase 2 batch C.
///
/// Same reasoning as [`endings`] and [`opening_records`], and appended after
/// both so no record above moves.
///
/// Two things here that the opening's records could not express:
///
/// * a `'C'` gap event -- the two lines the church assembles on the stack
///   rather than quoting. Emitting it places the composed line among the
///   plain ones, and the CS halves go out as `church_fragment` records;
/// * `levelup_write` -- `FUN_1000_2526` prints its per-level line with
///   `Write` (`0eed:0000`), not `WriteLn`, so those five literals are a
///   different instruction shape from every `*_line` record above.
fn church_records(out: &mut impl Write) -> io::Result<()> {
    let groups: [(&str, &[&str], opening::Gaps, &[&str]); 5] = [
        ("sermon2", &church::SERMON_2, church::SERMON_2_GAPS, &[]),
        ("sermon1", &church::SERMON_1, church::SERMON_1_GAPS, &[]),
        (
            "sermon0",
            &church::SERMON_0,
            church::SERMON_0_GAPS,
            &church::SERMON_0_FRAGMENTS,
        ),
        (
            "forced_level",
            &church::FORCED_LEVEL,
            church::FORCED_LEVEL_GAPS,
            &church::FORCED_LEVEL_FRAGMENTS,
        ),
        ("parting", &church::PARTING, church::PARTING_GAPS, &[]),
    ];
    for (tag, lines, _, _) in groups {
        for (i, line) in lines.iter().enumerate() {
            writeln!(out, "church_line {tag} {i} {}", text::strip(line))?;
        }
    }
    for (tag, _, gaps, _) in groups {
        for (at, events) in gaps {
            writeln!(out, "church_gap {tag} {at} {events}")?;
        }
    }
    for (tag, _, _, fragments) in groups {
        for (i, fragment) in fragments.iter().enumerate() {
            writeln!(out, "church_fragment {tag} {i} {}", text::strip(fragment))?;
        }
    }

    // `FUN_1000_2526`'s five `Write`s, in address order: the line's opener
    // and then the four stat arms, which is also `Stat`'s own order.
    writeln!(
        out,
        "levelup_write 0 {}",
        text::strip(progress::LEVELUP_PREFIX)
    )?;
    for (i, stat) in [Stat::Strength, Stat::Agility, Stat::Vitality, Stat::Luck]
        .into_iter()
        .enumerate()
    {
        writeln!(
            out,
            "levelup_write {} {}",
            i + 1,
            text::strip(stat.message())
        )?;
    }
    // `1000:288b`'s bare `WriteLn`, in the same `(index, events)` shape the
    // gap tables use: the tail gap of the five-`Write` span.
    writeln!(out, "levelup_gap 5 B")?;
    writeln!(
        out,
        "levelup_tail {:04x} {:04x} {}",
        progress::LEVELUP_TAIL_FILLS[0],
        progress::LEVELUP_TAIL_FILLS[1],
        text::strip(progress::LEVELUP_TAIL)
    )?;
    Ok(())
}

/// `run`'s own extra line, bucket 1's district lines and bucket 4's flavour
/// turn -- `docs/re/port-gaps.md` rows 22, 12 and 11, landed in Phase 2
/// batch C part 2. Same reasoning as [`church_records`], appended after it
/// so no record above moves.
///
/// `bucket4 1 C`'s `wander_gap` is written directly, the same way
/// `levelup_gap 5 B` is above: [`wander::BUCKET4`] is kept in the image's
/// ADDRESS order rather than run through [`opening::play`] (see that
/// module's doc for why), so there is no `Gaps` table to read the gap from
/// -- the composed line falls strictly between its index 0 and index 1.
fn wander_records(out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "wander_line ran 0 {}", text::strip(wander::RAN))?;

    for (i, line) in wander::BUCKET1.iter().enumerate() {
        writeln!(out, "wander_line bucket1 {i} {}", text::strip(line))?;
    }

    for (i, line) in wander::BUCKET4.iter().enumerate() {
        writeln!(out, "wander_line bucket4 {i} {}", text::strip(line))?;
    }
    writeln!(out, "wander_gap bucket4 1 C")?;
    for (i, frag) in wander::BUCKET4_FRAGMENTS.iter().enumerate() {
        writeln!(out, "wander_fragment bucket4 {i} {}", text::strip(frag))?;
    }
    Ok(())
}

/// The opening's half of the stream -- `docs/re/port-gaps.md` rows 1, 6, 8,
/// 10, 15 and 16, landed in Phase 2.
///
/// Same reasoning as [`endings`]: these are hand-transcribed shortstrings
/// and `difftest.py` re-decodes each of them out of `orig/g.exe` by
/// instruction shape. Appended after the ending records so nothing above
/// moves.
///
/// The two district groups are emitted **separately** on purpose: the image
/// holds two copies of districts 2/3/4's wording (CS `0x6849`.. and CS
/// `0x8346`..) and one shared record would compare one copy twice.
fn opening_records(out: &mut impl Write) -> io::Result<()> {
    let groups: [(&str, &[&str], opening::Gaps); 7] = [
        ("splash", &opening::SPLASH, opening::SPLASH_GAPS),
        ("backstory", &opening::BACKSTORY, opening::BACKSTORY_GAPS),
        ("start_arrival", &opening::START_ARRIVAL, opening::NO_GAPS),
        ("tutorial", &opening::TUTORIAL, opening::NO_GAPS),
        (
            "advance_arrival",
            &opening::ADVANCE_ARRIVAL,
            opening::NO_GAPS,
        ),
        ("quit", &opening::QUIT_TAIL, opening::QUIT_GAPS),
        ("help", &opening::HELP_PLAIN, opening::NO_GAPS),
    ];
    for (tag, lines, _) in groups {
        for (i, line) in lines.iter().enumerate() {
            writeln!(out, "opening_line {tag} {i} {}", text::strip(line))?;
        }
    }
    // The blank lines and `ReadKey`s between those literals. Only non-empty
    // gaps are emitted; a gap the image has and the port does not (or the
    // other way round) shows up as a record-count difference, which
    // `difftest.py` compares like any other.
    for (tag, _, gaps) in groups {
        for (at, events) in gaps {
            writeln!(out, "opening_gap {tag} {at} {events}")?;
        }
    }
    writeln!(out, "help_district_line {}", opening::HELP_DISTRICT_LINE)?;
    for (i, frag) in opening::HELP_FRAGMENTS.iter().enumerate() {
        writeln!(out, "help_fragment {i} {}", text::strip(frag))?;
    }
    for (i, line) in opening::HELP_WEIGHT_LINES.iter().enumerate() {
        writeln!(
            out,
            "help_weight_line {i} {} {}",
            opening::HELP_WEIGHT_INDEX,
            text::strip(line)
        )?;
    }
    Ok(())
}

/// The endings' half of the stream -- `docs/re/port-gaps.md` rows 2, 4, 7,
/// 13, 17, 21, landed in Phase 2.
///
/// These records exist because Phase 2 does not gate a ported line on a
/// citation: every string below was typed into `src/ending.rs` by hand from
/// a decoded shortstring, and a transcription slip in one of them is exactly
/// the error class that gate would otherwise have caught. `difftest.py`
/// re-decodes each one out of `orig/g.exe` -- finding the sites by the
/// `WriteLn`/`rtl_str_append` instruction shapes rather than by a quoted
/// address -- so the comparison has two independent readings of the same
/// bytes on its two sides.
///
/// Appended after every record kind that existed before, so the positions of
/// those are unchanged (`difftest.py` compares ordered lists).
fn endings(out: &mut impl Write) -> io::Result<()> {
    let groups: [(&str, &[&str]); 6] = [
        ("opener1", &ending::OPENER_1),
        ("opener3", &ending::OPENER_3),
        ("opener4", &ending::OPENER_4),
        ("ending4", &ending::ENDING_4),
        ("ending3", &ending::ENDING_3),
        // The end screen's three full-width lines, in the order the image
        // holds them: 1000:0793, 1000:07ae, 1000:0a4f.
        (
            "endscreen",
            &[ending::DEATH_LINE, ending::VICTORY_LINE, ending::ANY_KEY],
        ),
    ];
    for (tag, lines) in groups {
        for (i, line) in lines.iter().enumerate() {
            writeln!(out, "ending_line {tag} {i} {}", text::strip(line))?;
        }
    }
    for (mult, line) in ending::ERRAND_AWARDS {
        writeln!(out, "errand_award {mult} {}", text::strip(line))?;
    }
    // The prefix is spaces then a bare caret; what is compared is the number
    // of spaces, so the caret does not reach the stream.
    writeln!(
        out,
        "end_banner indent {}",
        ending::BANNER_INDENT.chars().count() - 1
    )?;
    writeln!(out, "end_banner death_colour 4")?;
    writeln!(out, "end_banner victory_colour 2")?;
    for (i, row) in ending::BANNER.iter().enumerate() {
        writeln!(out, "banner_row {i} {row}")?;
    }
    writeln!(out, "marquee indent {}", ending::MARQUEE_INDENT)?;
    writeln!(out, "marquee phases {}", ending::MARQUEE_PHASES)?;
    writeln!(
        out,
        "marquee word {}",
        text::strip(&ending::marquee_frame(0))
    )?;
    // The two scripted stat blocks `FUN_1000_11c2` writes, in `Game::boss`'s
    // own order -- the ids are what pick the block, so they are compared too.
    for e in data::enemies().iter().filter(|e| e.stats.is_some()) {
        let f = e.to_fighter().expect("filtered on stats");
        writeln!(
            out,
            "boss_stats {} {} {} {} {} {} {} {} {} {} {} {}",
            e.id,
            f.class,
            f.level,
            f.strength,
            f.agility,
            f.vitality,
            f.luck,
            f.armor,
            f.dmg_min,
            f.dmg_max,
            f.hp,
            f.hpmax
        )?;
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
