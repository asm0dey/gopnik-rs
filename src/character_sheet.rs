//! The player's character sheet.
//!
//! ## The split, and why it is this one
//!
//! The module **builds** the lines; [`crate::game::Game::show_stats`]
//! **prints** them. This split makes the renderer testable without
//! capturing stdout. [`lines`] returns the sheet in the game's display
//! order and every branch is assertable from a string comparison.
//!
//! ## Lines, combining output
//!
//! The display combines multiple pieces. A sequence of writes builds one
//! open line, and the closing write yields the final line. The sequence
//! yields two blank separators around the weapon block.
//!
//! ## What is NOT modelled
//!
//! * **The two health-colour thresholds' decimal values.** See [`HEALTH_BROWN_ABOVE`].
//! * **The `#` substitution when a value is not pushed.** `crate::text::fill`
//!   leaves a `#` with no value as a literal `#` instead of printing `0`.
//!   No literal this module passes has more `#` than it has values, so the
//!   two agree on every string here -- but a player name containing `#`
//!   would diverge, and the name line is the one place a `#` can arrive from data.

use crate::combat;
use crate::combat_dispatch::Pistol;
use crate::data;
use crate::model::Fighter;
use crate::text;

pub const HEALTH_BROWN_ABOVE: f64 = 0.25;

/// The health line's colour digit is `'2'` above this ratio. Port decision
/// on the same footing as [`HEALTH_BROWN_ABOVE`] -- read that first.
pub const HEALTH_GREEN_ABOVE: f64 = 0.50;

/// Everything the sheet reads that is not a field of [`Fighter`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Kit {
    /// XP not yet spent on a level (`crate::progress::Progress::xp`).
    pub xp: u16,
    /// XP needed for the next level.
    pub threshold: u16,
    /// The joint buff's countdown, displayed in Сила, damage, and Обдолбаный condition. Any non-zero count counts.
    pub buff_countdown: u8,
    /// Крестик.
    pub krestik: bool,
    /// кольцо "Гс".
    pub ring_gs: bool,
    /// кольцо "Пг".
    pub ring_pg: bool,
    /// Мега Кольцо.
    pub mega_ring: bool,
    /// кольцо "Гп".
    pub ring_gp: bool,
    /// мобильник.
    pub mobile: bool,
    /// тёмные очки.
    pub dark_glasses: bool,
    /// зоновская наколка.
    pub prison_tattoo: bool,
    /// The pistol block.
    pub pistol: Pistol,
    /// Бутсы.
    pub boots: bool,
    /// Понтовые бутсы.
    pub boots_pontovye: bool,
    /// Кастет.
    pub kastet: bool,
    /// Дубинка.
    pub dubinka: bool,
    /// Нож.
    pub nozh: bool,
    /// Тесак.
    pub tesak: bool,
    /// зубная защита. Its guard is checked for equality rather than the zero check other item flags use.
    pub tooth_guard: bool,
    /// костюм Abibas.
    pub suit_abibas: bool,
    /// костюм Adidas.
    pub suit_adidas: bool,
    /// Кожанка.
    pub jacket: bool,
    /// Крутая кожанка.
    pub jacket_krutaya: bool,
}

/// One open line plus the lines already closed.
///
/// See the module doc: the original's `Write` / `WriteLn` split is what this
/// reproduces, and it is why a bare `WriteLn` can produce an empty line.
///
/// `pub(crate)` because [`crate::enemy_sheet`] emits the same way -- its
/// accuracy block is one `Write` closed by a later `WriteLn`, exactly the
/// shape this type exists for. The two fields stay private; the enemy sheet
/// only ever reaches the four methods below.
#[derive(Default)]
pub(crate) struct Out {
    lines: Vec<String>,
    open: String,
}

impl Out {
    /// `call 0eed:0x0` -- leaves the line open.
    pub(crate) fn write(&mut self, s: &str) {
        self.open.push_str(s);
    }

    /// `call 0eed:0x1c2` -- appends and closes the line.
    pub(crate) fn writeln(&mut self, s: &str) {
        self.open.push_str(s);
        self.newline();
    }

    /// Closes the line with nothing appended.
    pub(crate) fn newline(&mut self) {
        self.lines.push(std::mem::take(&mut self.open));
    }

    /// The function's own exit.
    ///
    /// Nothing is flushed here because nothing can be open: the last two
    /// blocks are both unconditional two-armed `WriteLn` pairs that pick
    /// between `Пиво #.#л.` / `^4Пива нет` and `Бабки #` / `^4Нету бабок` --
    /// so the money line always closes, and the only thing that can follow it
    /// is the `Хлам #` line.
    ///
    /// That logic is sound, but it was only prose before: the method dropped
    /// `self.open` on the floor and no test could have seen a stray unterminated
    /// append. The `debug_assert!` makes it executable, so a future edit that
    /// opens a line and forgets to close it fails the debug-profile test run
    /// instead of silently losing the text.
    pub(crate) fn finish(self) -> Vec<String> {
        debug_assert!(
            self.open.is_empty(),
            "Out::finish dropped an unterminated line: {:?}",
            self.open
        );
        self.lines
    }
}

/// The whole sheet, in the game's display order.
///
/// `name` is a separate parameter rather than [`Fighter::name`].
pub fn lines(p: &Fighter, name: &str, kit: &Kit) -> Vec<String> {
    let mut o = Out::default();
    header(&mut o, p, name, kit);
    stat_line(&mut o, p, kit);
    charms(&mut o, kit);
    worn_singletons(&mut o, kit);
    pistol_block(&mut o, kit);
    damage_line(&mut o, p, kit);
    health_line(&mut o, p, kit);
    accuracy_block(&mut o, p);
    armour_block(&mut o, p, kit);
    purse(&mut o, p);
    o.finish()
}

/// The class/level header, the name, the experience line.
fn header(o: &mut Out, p: &Fighter, name: &str, kit: &Kit) {
    // Four appends, in this order: `^2Ты `, the player's rank,
    // ` # уровня - `, and the krutizna for that level.
    // Then the level is pushed and the WriteLn closes the line.
    o.writeln(&text::fill(
        &format!(
            "^2Ты {} # уровня - {}",
            data::rank_name(p.class),
            data::krutizna(p.level)
        ),
        &[i64::from(p.level)],
    ));
    // The string `^2А зовут тебя: ` and the player's name are appended.
    o.writeln(&format!("^2А зовут тебя: {name}"));
    // At level 40 there is no next threshold and the line is skipped -- which is what a 43-entry ladder with a 40 cap needs.
    if p.level <= 0x27 {
        o.writeln(&text::fill(
            EMITTED[0].1,
            &[i64::from(kit.xp), i64::from(kit.threshold)],
        ));
    }
}

/// The four stats and the colour digits.
///
/// A literal `7777` is assigned into the stats string; each of its four
/// characters is a colour digit that a worn item patches to `'1'`, and
/// the format string interleaves them.
fn stat_line(o: &mut Out, p: &Fighter, kit: &Kit) {
    let stoned = kit.buff_countdown > 0;
    let all = kit.ring_pg || kit.mega_ring;
    // The Сила slot is set to colour digit `'1'`.
    let c0 = digit(stoned || all);
    // Лв and Жв are both set by one guard pair.
    let c1 = digit(all);
    let c2 = c1;
    // The Уд slot is set to colour digit `'1'`.
    let c3 = digit(kit.krestik || kit.ring_gs || all);
    // Five literals interleaved with the four digits, appended in order:
    // `Сл:^`, `#^7 Лв:^`, `#^7 Жв:^`, `#^7 Уд:^`.
    // And the last placeholder on its own.
    // The four stats are pushed and the WriteLn closes the line.
    o.writeln(&text::fill(
        &format!("Сл:^{c0}#^7 Лв:^{c1}#^7 Жв:^{c2}#^7 Уд:^{c3}#"),
        &[
            i64::from(p.strength),
            i64::from(p.agility),
            i64::from(p.vitality),
            i64::from(p.luck),
        ],
    ));
}

/// `'1'` when a worn item boosts the slot, else the `7777` default.
fn digit(boosted: bool) -> char {
    if boosted {
        '1'
    } else {
        '7'
    }
}

/// The two charm sections.
///
/// Each is a header `Write` gated on the disjunction of its own rows,
/// followed by the rows and one bare `WriteLn` that closes the line.
fn charms(o: &mut Out, kit: &Kit) {
    // When neither Крестик nor кольцо "Гс" is owned, the whole block is skipped.
    if kit.krestik || kit.ring_gs {
        o.write(EMITTED[1].1); // CS `0x16d9`
        if kit.krestik {
            o.write(EMITTED[2].1); // CS `0x16e2`
        }
        if kit.ring_gs {
            o.write(EMITTED[3].1); // CS `0x16f7`
        }
        o.newline();
    }
    // When either кольцо "Пг" or Мега Кольцо is missing, the charms section is skipped.
    if kit.ring_pg || kit.mega_ring || kit.ring_gp {
        o.write(EMITTED[4].1); // CS `0x1710`
        if kit.ring_pg {
            o.write(EMITTED[5].1); // CS `0x1720`
        }
        if kit.mega_ring {
            o.write(EMITTED[6].1); // CS `0x1737`
        }
        if kit.ring_gp {
            o.write(EMITTED[7].1); // CS `0x174e`
        }
        o.newline();
    }
}

/// The three items that get a whole line each.
fn worn_singletons(o: &mut Out, kit: &Kit) {
    if kit.mobile {
        o.writeln(EMITTED[8].1); // CS `0x176a`
    }
    if kit.dark_glasses {
        o.writeln(EMITTED[9].1); // CS `0x1782`
    }
    if kit.prison_tattoo {
        o.writeln(EMITTED[10].1); // CS `0x179c`
    }
}

/// The pistol, its silencer and its magazine.
///
/// When there is no pistol, the entire block is skipped.
fn pistol_block(o: &mut Out, kit: &Kit) {
    if !kit.pistol.owned {
        return;
    }
    // The blank separator before the pistol details.
    o.newline();
    o.write(EMITTED[11].1); // CS `0x17b8`

    // When there is no silencer, this section is skipped.
    if kit.pistol.silencer {
        o.write(EMITTED[12].1); // CS `0x17cf`, the game's own typo
    }
    let n = kit.pistol.cartridges;
    // The magazine count guard. A signed word.
    if n > 0 {
        o.writeln(&text::fill(EMITTED[13].1, &[i64::from(n)])); // CS `0x17de`
    }
    // Magazine counts: 1 or 2 rounds left printed distinctly from other states.
    if (1..=2).contains(&n) {
        o.writeln(EMITTED[14].1); // CS `0x17ef`
    }
    if n <= 0 {
        o.writeln(EMITTED[15].1); // CS `0x1805`
    }
    o.newline();
}

/// The damage line and every hand weapon.
///
/// The line's own colour digit starts as `'7'` and a seven-way disjunction
/// raises it to `'1'`. The string starts with a colour marker, appends that
/// digit, then `Урон #-#    ` and the weapon labels.
fn damage_line(o: &mut Out, p: &Fighter, kit: &Kit) {
    let armed = kit.buff_countdown > 0
        || kit.boots
        || kit.boots_pontovye
        || kit.kastet
        || kit.dubinka
        || kit.nozh
        || kit.tesak;
    o.write(&text::fill(
        // `Урон #-#    `, four trailing spaces.
        &format!("^{}Урон #-#    ", digit(armed)),
        &[i64::from(p.dmg_min), i64::from(p.dmg_max)],
    ));
    // Best-item-wins: each pair prints the superseded item dim (`^4`) beside
    // the good one, as two arms sharing the lesser item's flag.
    if kit.boots && !kit.boots_pontovye {
        o.write(EMITTED[16].1); // CS `0x182e`
    }
    if kit.boots && kit.boots_pontovye {
        o.write(EMITTED[17].1); // CS `0x183b`
    }
    if kit.boots_pontovye {
        o.write(EMITTED[18].1); // CS `0x1844`
    }
    // The three blades supersede the кастет; the dim arm is the unarmed blows.
    let over_kastet = kit.nozh || kit.dubinka || kit.tesak;
    if kit.kastet && !over_kastet {
        o.write(EMITTED[19].1); // CS `0x185e`
    }
    if kit.kastet && over_kastet {
        o.write(EMITTED[20].1); // CS `0x186c`
    }
    // Дубинка supersedes in multiple forms.
    let over_dubinka = kit.nozh || kit.tesak;
    if kit.dubinka && !over_dubinka {
        o.write(EMITTED[21].1); // CS `0x1876`
    }
    if kit.dubinka && over_dubinka {
        o.write(EMITTED[22].1); // CS `0x1886`
    }
    // Нож supersedes in multiple forms.
    if kit.nozh && !kit.tesak {
        o.write(EMITTED[23].1); // CS `0x1891`
    }
    if kit.nozh && kit.tesak {
        o.write(EMITTED[24].1); // CS `0x189c`
    }
    // Nothing supersedes the тесак.
    if kit.tesak {
        o.write(EMITTED[25].1); // CS `0x18a3`
    }
    o.newline();
}

/// The health line and the four conditions.
///
/// The conditions are NOT separate lines. A string is emptied and each
/// condition appends its label to it; the whole accumulator is then appended
/// to the health line after `Здоровье #/#  `, and only then is the line closed.
fn health_line(o: &mut Out, p: &Fighter, kit: &Kit) {
    let mut cond = String::new();
    // Челюсть guard is an EQUALITY check, not the `> 0` the item flags use.
    if p.broken_jaw {
        cond.push_str(CONDITIONS[0]); // CS `0x18b4`
    }
    // Челюсть broken guard.
    if kit.tooth_guard {
        cond.push_str(CONDITIONS[1]); // CS `0x18c8`
    }
    // Нога guard is EQUALITY, not the `> 0` the item flags use.
    if p.broken_leg {
        cond.push_str(CONDITIONS[2]); // CS `0x18da`
    }
    // Обдолбаный guard is unsigned.
    if kit.buff_countdown > 0 {
        cond.push_str(CONDITIONS[3]); // CS `0x18eb`
    }
    o.writeln(&text::fill(
        // The colour digit, the condition accumulator, and the health range placeholder `Здоровье #/#  `.
        &format!("^{}Здоровье #/#  {cond}", health_digit(p.hp, p.hpmax)),
        &[i64::from(p.hp), i64::from(p.hpmax)],
    ));
}

/// The three colour transitions: `'4'`, then `'6'`, then `'2'`.
///
/// The two thresholds order is what the original establishes only.
/// `hpmax == 0` is a port decision: when the divisor is zero, the default `'4'`
/// is kept rather than guessing. Nothing in play reaches it -- hpmax is only
/// decreased by `dec` and `sub 5` on values seeded well above zero -- but a
/// hand-built `Fighter` can.
pub(crate) fn health_digit(hp: u16, hpmax: u16) -> char {
    if hpmax == 0 {
        return '4';
    }
    let ratio = f64::from(hp) / f64::from(hpmax);
    if ratio > HEALTH_GREEN_ABOVE {
        '2'
    } else if ratio > HEALTH_BROWN_ABOVE {
        '6'
    } else {
        '4'
    }
}

/// The accuracy block's four literals, shipped by both sheets.
///
/// [`crate::enemy_sheet`] prints the same four through [`accuracy_block`].
// `Попадания`
pub(crate) const ACCURACY_FLAT: &str = EMITTED[26].1;
// `Точность #% ` with a trailing space. The line stays open.
pub(crate) const ACCURACY_CAPPED: &str = EMITTED[27].1;
// Three leading spaces.
pub(crate) const ACCURACY_SECOND: &str = EMITTED[28].1;
// Two spaces after the comma.
pub(crate) const ACCURACY_MANY: &str = EMITTED[29].1;

/// The accuracy block, from Ловкость alone.
///
/// **This is both sheets' copy.** The original writes the block twice and
/// the two are the same program: same gate, same `agility * 5 + 20`, same
/// loop, same three literals, same `Write`-then-`WriteLn` shape.
/// [`crate::enemy_sheet`] therefore calls this function rather than
/// transcribing it a second time.
///
/// The arithmetic is not reimplemented here: `crate::combat` already carries
/// it from the enemy sheet.
pub(crate) fn accuracy_block(o: &mut Out, p: &Fighter) {
    let unopposed = Fighter::default();
    // Gate: agility greater than 14.
    if p.agility <= 0xe {
        o.writeln(&text::fill(
            ACCURACY_FLAT,
            &[i64::from(combat::accuracy_pct(p, &unopposed))],
        ));
        return;
    }
    o.write(ACCURACY_CAPPED);
    let extra = combat::blows_per_round(p, &unopposed) - 1;
    let pct = i64::from(combat::accuracy_pct_nth(p, &unopposed, extra));
    // Only one hit printed.
    if extra == 1 {
        o.writeln(&text::fill(ACCURACY_SECOND, &[pct]));
    }
    // Multiple hits printed.
    if extra > 1 {
        o.writeln(&text::fill(
            ACCURACY_MANY,
            &[i64::from(extra), i64::from(extra) + 1, pct],
        ));
    }
}

/// The armour line, the two suits and the two jackets.
///
/// When there is no armour, no suit or jacket is ever listed. Both clothing
/// pairs are three arms: the lesser item's flag splits into dim label then the
/// better one's bright label, and "the lesser one's own bright label", with a
/// THIRD guard for when the lesser item is not owned.
fn armour_block(o: &mut Out, p: &Fighter, kit: &Kit) {
    if p.armor == 0 {
        return;
    }
    // Four trailing spaces. A zero-extended byte.
    o.write(&text::fill(EMITTED[30].1, &[i64::from(p.armor)]));
    if kit.suit_abibas {
        if kit.suit_adidas {
            o.write(EMITTED[31].1); // CS `0x1964`
            o.write(EMITTED[32].1); // CS `0x196e`
        } else {
            o.write(EMITTED[33].1); // CS `0x1983`
        }
    }
    if kit.suit_adidas && !kit.suit_abibas {
        o.write(EMITTED[34].1); // CS `0x196e`
    }
    if kit.jacket {
        if kit.jacket_krutaya {
            o.write(EMITTED[35].1); // CS `0x1998`
            o.write(EMITTED[36].1); // CS `0x19a3`
        } else {
            o.write(EMITTED[37].1); // CS `0x19b9`
        }
    }
    if kit.jacket_krutaya && !kit.jacket {
        o.write(EMITTED[38].1); // CS `0x19a3`
    }
    o.newline();
}

/// The items: косяки, пиво, бабки, хлам.
fn purse(o: &mut Out, p: &Fighter) {
    // Косяки check.
    if p.joints > 0 {
        o.writeln(&text::fill(EMITTED[39].1, &[i64::from(p.joints)])); // CS `0x19c8`
    }
    // Пиво is stored in HALF-litres: `idiv 2` and `((remainder * 5) mod 10)`
    // so an odd count prints `.5`. The `mod 10` on input 0 or 5 never changes
    // the value; it is kept because this is a port.
    if p.beer_dl > 0 {
        o.writeln(&text::fill(
            EMITTED[40].1, // CS `0x19d1`
            &[
                i64::from(p.beer_dl / 2),
                (i64::from(p.beer_dl % 2) * 5) % 10,
            ],
        ));
    } else {
        o.writeln(EMITTED[41].1); // CS `0x19dc`
    }
    // Бабки check.
    if p.money > 0 {
        o.writeln(&text::fill(EMITTED[42].1, &[i64::from(p.money)])); // CS `0x19e7`
    } else {
        o.writeln(EMITTED[43].1); // CS `0x19ef`
    }
    // Хлам check.
    if p.junk > 0 {
        o.writeln(&text::fill(EMITTED[44].1, &[i64::from(p.junk)])); // CS `0x19fc`
    }
}

/// The four injury conditions, appended to the health line.
///
/// The other thirteen fragments (`Ты `, ` # уровня - `, `Сл:^`, `Урон #-#    `, …)
/// are assembled by this port in one `format!` string.
pub(crate) const CONDITIONS: [&str; 4] = [
    "^4Сломана челюсть  ",
    "^1Зубная защита  ",
    "^4Сломана нога  ",
    "^6Обдолбаный  ",
];

/// The sheet's literal pool, in display order.
///
/// Two texts appear TWICE: `Костюм Adidas(+2) ` and `Крутая кожанка(+4) `.
/// The armour block tests each item on two paths and each path carries its own copy.
///
/// `(closes, text)` -- `closes` is true for a closing line, false for a line
/// that continues with the next literal.
pub(crate) const EMITTED: [(bool, &str); 45] = [
    (true, "^6Сейчас у тебя # опыта, А для прокачки надо #"),
    (false, "Феньки: "),
    (false, "^1Крестик(Удача +2) "),
    (false, "^1Кольцо \"Гс\"(Удача +1) "),
    (false, "Мощные феньки: "),
    (false, "^1Кольцо \"Пг\"(Всё +1) "),
    (false, "^1Мега Кольцо(Всё +4) "),
    (false, "^1Кольцо \"Гп\"(Самолечение) "),
    (true, "^1У тебя есть мобильник"),
    (true, "^1У тебя есть тёмные очки"),
    (true, "^1На тебе зоновская наколка"),
    (false, "^1У тебя есть пистолет"),
    (false, "^1 с гушителем"),
    (true, "^1! патронов - #"),
    (true, "^6 А птронов-то мало "),
    (true, "^1.^4 Правда без патронов"),
    (false, "^1Бутсы(+1) "),
    (false, "^4Бутсы "),
    (false, "^1Понтовые бутсы(Урон+2) "),
    (false, "^1Кастет(+2) "),
    (false, "^4Кастет "),
    (false, "^1Дубинка(+4)  "),
    (false, "^4Дубинка "),
    (false, "^1Нож(+6) "),
    (false, "^4Нож "),
    (false, "^1Тесак(Урон+9) "),
    (true, "Точность #%"),
    (false, "Точность 90% "),
    (true, "   Второй удар #%"),
    (true, "- # ударов,  Точность # удара #%"),
    (false, "^2Броня #    "),
    (false, "^4Abibas "),
    (false, "^1Костюм Adidas(+2) "),
    (false, "^1Костюм Abibas(+1) "),
    (false, "^1Костюм Adidas(+2) "),
    (false, "^4Кожанка "),
    (false, "^1Крутая кожанка(+4) "),
    (false, "^1Кожанка(+2) "),
    (false, "^1Крутая кожанка(+4) "),
    (true, "Косяки #"),
    (true, "Пиво #.#л."),
    (true, "^4Пива нет"),
    (true, "Бабки #"),
    (true, "^4Нету бабок"),
    (true, "Хлам #"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> Fighter {
        Fighter {
            name: "Тест".to_string(),
            class: 3,
            level: 5,
            hp: 20,
            hpmax: 20,
            strength: 6,
            agility: 7,
            vitality: 8,
            luck: 9,
            dmg_min: 3,
            dmg_max: 6,
            ..Fighter::default()
        }
    }

    fn sheet(p: &Fighter, kit: &Kit) -> Vec<String> {
        lines(p, "Вася", kit)
    }

    /// The one property of the two thresholds the ORIGINAL establishes:
    /// the second is strictly above the first, so a rising ratio walks all
    /// three colours and never goes back.
    ///
    /// Deliberately not a simple comparison: that would be constant-folded,
    /// which is the check-that-cannot-fail. This walks `health_digit` instead
    /// and reds when the two constants are swapped or made equal.
    #[test]
    fn the_health_colour_walks_four_six_two_and_never_back() {
        let rank = |c: char| match c {
            '4' => 0,
            '6' => 1,
            '2' => 2,
            other => panic!("unexpected health colour {other:?}"),
        };
        let mut changes = Vec::new();
        let mut last = 0;
        for hp in 0..=1000u16 {
            let r = rank(health_digit(hp, 1000));
            assert!(r >= last, "hp {hp}/1000: the colour went backwards");
            if r != last {
                changes.push(r);
            }
            last = r;
        }
        assert_eq!(changes, [1, 2], "all three colours must occur, in order");
    }

    #[test]
    fn header_names_the_class_and_the_krutizna_rank() {
        let p = player();
        assert_eq!(
            sheet(&p, &Kit::default())[0],
            "^2Ты Подтсан 5 уровня - Чё-то отдалённо похожее на не ЧМО"
        );
        let mut boss = player();
        boss.class = 10;
        boss.level = 0;
        assert_eq!(
            sheet(&boss, &Kit::default())[0],
            "^2Ты Ректор НГУ 0 уровня - Опущеный"
        );
    }

    #[test]
    fn the_name_line_is_the_second() {
        assert_eq!(sheet(&player(), &Kit::default())[1], "^2А зовут тебя: Вася");
    }

    #[test]
    fn the_experience_line_is_gated_at_level_39() {
        let kit = Kit {
            xp: 42,
            threshold: 60,
            ..Kit::default()
        };
        let mut p = player();
        p.level = 39;
        assert_eq!(
            sheet(&p, &kit)[2],
            "^6Сейчас у тебя 42 опыта, А для прокачки надо 60"
        );
        p.level = 40;
        let out = sheet(&p, &kit);
        assert!(
            !out.iter().any(|l| l.contains("опыта")),
            "level 40 must skip it: {out:?}"
        );
        // ...and the stat line takes its place, so nothing else shifted.
        assert!(out[2].starts_with("Сл:^"), "{out:?}");
    }

    /// Each of the four `7777` slots, under each of its own patch rules.
    #[test]
    fn the_stat_line_colours_only_the_boosted_slots() {
        let p = player();
        let line = |kit: &Kit| sheet(&p, kit)[3].clone();
        assert_eq!(line(&Kit::default()), "Сл:^76^7 Лв:^77^7 Жв:^78^7 Уд:^79");
        // Сила alone.
        assert_eq!(
            line(&Kit {
                buff_countdown: 1,
                ..Kit::default()
            }),
            "Сл:^16^7 Лв:^77^7 Жв:^78^7 Уд:^79"
        );
        // Удача alone.
        assert_eq!(
            line(&Kit {
                krestik: true,
                ..Kit::default()
            }),
            "Сл:^76^7 Лв:^77^7 Жв:^78^7 Уд:^19"
        );
        // Удача alone.
        assert_eq!(
            line(&Kit {
                ring_gs: true,
                ..Kit::default()
            }),
            "Сл:^76^7 Лв:^77^7 Жв:^78^7 Уд:^19"
        );
        // All four together.
        for kit in [
            Kit {
                ring_pg: true,
                ..Kit::default()
            },
            Kit {
                mega_ring: true,
                ..Kit::default()
            },
        ] {
            assert_eq!(line(&kit), "Сл:^16^7 Лв:^17^7 Жв:^18^7 Уд:^19");
        }
    }

    fn find<'a>(out: &'a [String], needle: &str) -> Option<&'a String> {
        out.iter().find(|l| l.contains(needle))
    }

    #[test]
    fn the_charm_sections_print_only_when_one_of_their_rows_is_owned() {
        let p = player();
        let bare = sheet(&p, &Kit::default());
        assert!(find(&bare, "Феньки").is_none(), "{bare:?}");

        let kit = Kit {
            ring_gs: true,
            ..Kit::default()
        };
        assert_eq!(
            find(&sheet(&p, &kit), "Феньки").unwrap(),
            "Феньки: ^1Кольцо \"Гс\"(Удача +1) "
        );

        let kit = Kit {
            krestik: true,
            ring_gs: true,
            ..Kit::default()
        };
        assert_eq!(
            find(&sheet(&p, &kit), "Феньки").unwrap(),
            "Феньки: ^1Крестик(Удача +2) ^1Кольцо \"Гс\"(Удача +1) "
        );

        let kit = Kit {
            ring_pg: true,
            mega_ring: true,
            ring_gp: true,
            ..Kit::default()
        };
        assert_eq!(
            find(&sheet(&p, &kit), "Мощные").unwrap(),
            "Мощные феньки: ^1Кольцо \"Пг\"(Всё +1) ^1Мега Кольцо(Всё +4) \
             ^1Кольцо \"Гп\"(Самолечение) "
        );
    }

    #[test]
    fn the_three_singleton_items_each_get_their_own_line() {
        let p = player();
        for (kit, want) in [
            (
                Kit {
                    mobile: true,
                    ..Kit::default()
                },
                "^1У тебя есть мобильник",
            ),
            (
                Kit {
                    dark_glasses: true,
                    ..Kit::default()
                },
                "^1У тебя есть тёмные очки",
            ),
            (
                Kit {
                    prison_tattoo: true,
                    ..Kit::default()
                },
                "^1На тебе зоновская наколка",
            ),
        ] {
            let out = sheet(&p, &kit);
            assert!(out.iter().any(|l| l == want), "{want}: {out:?}");
            assert!(
                !sheet(&p, &Kit::default()).iter().any(|l| l == want),
                "{want} printed with the flag clear"
            );
        }
    }

    fn armed(cartridges: i16, silencer: bool) -> Kit {
        Kit {
            pistol: Pistol {
                owned: true,
                silencer,
                cartridges,
            },
            ..Kit::default()
        }
    }

    #[test]
    fn the_pistol_block_is_bracketed_by_two_blank_lines() {
        let p = player();
        let out = sheet(&p, &armed(3, false));
        let i = out
            .iter()
            .position(|l| l == "^1У тебя есть пистолет^1! патронов - 3")
            .unwrap_or_else(|| panic!("{out:?}"));
        assert_eq!(out[i - 1], "");
        assert_eq!(out[i + 1], "");
        // No pistol -> no block and no blanks at all before the damage line.
        let bare = sheet(&p, &Kit::default());
        assert!(!bare.iter().any(|l| l.is_empty()), "{bare:?}");
    }

    #[test]
    fn the_cartridge_word_line_has_three_arms() {
        let p = player();
        let joined = |kit: &Kit| sheet(&p, kit).join("\n");
        assert!(joined(&armed(3, false)).contains("^1! патронов - 3"));
        assert!(!joined(&armed(3, false)).contains("птронов-то мало"));
        assert!(joined(&armed(2, false)).contains("^6 А птронов-то мало "));
        assert!(joined(&armed(2, false)).contains("^1! патронов - 2"));
        assert!(joined(&armed(0, false)).contains("^1.^4 Правда без патронов"));
        assert!(!joined(&armed(0, false)).contains("патронов - "));
        // The check for the lower guard gate: if no ammunition is left,
        // print `^6 А птронов-то мало ` and limit the range to (0..=2).
        assert!(!joined(&armed(0, false)).contains("птронов-то мало"));
        assert!(joined(&armed(0, true)).contains("^1У тебя есть пистолет^1 с гушителем"));
    }

    fn damage(out: &[String]) -> String {
        out.iter().find(|l| l.contains("Урон")).unwrap().clone()
    }

    #[test]
    fn the_damage_line_is_dim_until_something_boosts_it() {
        let p = player();
        assert_eq!(damage(&sheet(&p, &Kit::default())), "^7Урон 3-6    ");
        // A seven-way `or`, and EACH term has to raise the digit on its own.
        // One `||` written `&&` survives every test that only ever sets two
        // of them together.
        let setters: [fn(&mut Kit); 7] = [
            |k| k.buff_countdown = 3,
            |k| k.boots = true,
            |k| k.boots_pontovye = true,
            |k| k.kastet = true,
            |k| k.dubinka = true,
            |k| k.nozh = true,
            |k| k.tesak = true,
        ];
        for (i, set) in setters.into_iter().enumerate() {
            let mut kit = Kit::default();
            set(&mut kit);
            assert!(
                damage(&sheet(&p, &kit)).starts_with("^1Урон 3-6    "),
                "term {i} did not raise the damage colour"
            );
        }
    }

    #[test]
    fn best_item_wins_dims_the_superseded_weapon() {
        let p = player();
        let d = |kit: Kit| damage(&sheet(&p, &kit));
        let boots = Kit {
            boots: true,
            ..Kit::default()
        };
        assert_eq!(d(boots), "^1Урон 3-6    ^1Бутсы(+1) ");
        assert_eq!(
            d(Kit {
                boots_pontovye: true,
                ..boots
            }),
            "^1Урон 3-6    ^4Бутсы ^1Понтовые бутсы(Урон+2) "
        );
        let kastet = Kit {
            kastet: true,
            ..Kit::default()
        };
        assert_eq!(d(kastet), "^1Урон 3-6    ^1Кастет(+2) ");
        assert_eq!(
            d(Kit {
                dubinka: true,
                ..kastet
            }),
            "^1Урон 3-6    ^4Кастет ^1Дубинка(+4)  "
        );
        assert_eq!(
            d(Kit {
                nozh: true,
                dubinka: true,
                ..kastet
            }),
            "^1Урон 3-6    ^4Кастет ^4Дубинка ^1Нож(+6) "
        );
        assert_eq!(
            d(Kit {
                tesak: true,
                nozh: true,
                dubinka: true,
                ..kastet
            }),
            "^1Урон 3-6    ^4Кастет ^4Дубинка ^4Нож ^1Тесак(Урон+9) "
        );
        // The тесак alone supersedes nothing, so no dim label appears.
        assert_eq!(
            d(Kit {
                tesak: true,
                ..Kit::default()
            }),
            "^1Урон 3-6    ^1Тесак(Урон+9) "
        );
    }

    fn health(out: &[String]) -> String {
        out.iter().find(|l| l.contains("Здоровье")).unwrap().clone()
    }

    #[test]
    fn the_conditions_ride_on_the_health_line() {
        let mut p = player();
        p.broken_jaw = true;
        p.broken_leg = true;
        let kit = Kit {
            tooth_guard: true,
            buff_countdown: 2,
            ..Kit::default()
        };
        assert_eq!(
            health(&sheet(&p, &kit)),
            "^2Здоровье 20/20  ^4Сломана челюсть  ^1Зубная защита  \
             ^4Сломана нога  ^6Обдолбаный  "
        );
    }

    /// One condition case: a setter for the flag one guard reads, and the
    /// label that guard's arm appends.
    type ConditionCase = (fn(&mut Fighter, &mut Kit), &'static str);

    /// The four condition guards, one at a time and in the original's order.
    ///
    /// All four are set together, so a crossed guard pair -- the jaw's guard
    /// reading the leg's flag and the other way round -- leaves the asserted
    /// string identical. Both are `bool`, so the compiler cannot see it either.
    #[test]
    fn each_condition_guard_reads_its_own_flag() {
        let cases: [ConditionCase; 4] = [
            // Челюсть guard.
            (|p, _| p.broken_jaw = true, "^4Сломана челюсть  "),
            // Челюсть broken guard.
            (|_, k| k.tooth_guard = true, "^1Зубная защита  "),
            // Нога guard.
            (|p, _| p.broken_leg = true, "^4Сломана нога  "),
            // Обдолбаный guard.
            (|_, k| k.buff_countdown = 1, "^6Обдолбаный  "),
        ];
        for (set, want) in cases {
            let mut p = player();
            let mut kit = Kit::default();
            assert!(
                !health(&sheet(&p, &kit)).contains(want),
                "{want:?} was on the health line before its flag was set"
            );
            set(&mut p, &mut kit);
            assert_eq!(
                health(&sheet(&p, &kit)),
                format!("^2Здоровье 20/20  {want}"),
                "{want:?} alone"
            );
        }
    }

    #[test]
    fn the_health_colour_climbs_with_the_ratio() {
        let mut p = player();
        p.hpmax = 100;
        for (hp, want) in [(0, '4'), (25, '4'), (26, '6'), (50, '6'), (51, '2')] {
            p.hp = hp;
            assert_eq!(
                health(&sheet(&p, &Kit::default())).chars().nth(1),
                Some(want),
                "hp {hp}"
            );
        }
        // A zero hpmax keeps the default.
        p.hpmax = 0;
        p.hp = 0;
        assert_eq!(
            health(&sheet(&p, &Kit::default())).chars().nth(1),
            Some('4')
        );
    }

    #[test]
    fn accuracy_below_fifteen_agility_is_one_line() {
        let mut p = player();
        p.agility = 14;
        let out = sheet(&p, &Kit::default());
        assert!(out.iter().any(|l| l == "Точность 90%"), "{out:?}");
        p.agility = 3;
        let out = sheet(&p, &Kit::default());
        assert!(out.iter().any(|l| l == "Точность 35%"), "{out:?}");
    }

    #[test]
    fn accuracy_above_fourteen_agility_adds_a_second_blow() {
        let mut p = player();
        p.agility = 15;
        let out = sheet(&p, &Kit::default());
        assert!(
            out.iter().any(|l| l == "Точность 90%    Второй удар 5%"),
            "{out:?}"
        );
        // At exactly one extra hit the `# ударов` line does NOT also print.
        assert!(
            !out.iter().any(|l| l.contains("ударов")),
            "one extra hit must not print the plural line: {out:?}"
        );
        // 18 points per extra hit: 120 - 14 = 106 = 5*18 + 16.
        p.agility = 120;
        let out = sheet(&p, &Kit::default());
        assert!(
            out.iter()
                .any(|l| l == "Точность 90% - 6 ударов,  Точность 7 удара 80%"),
            "{out:?}"
        );
        assert!(
            !out.iter().any(|l| l.contains("Второй удар")),
            "six extra hits must not print the singular line: {out:?}"
        );
    }

    #[test]
    fn no_armour_hides_the_whole_clothing_block() {
        let mut p = player();
        p.armor = 0;
        let kit = Kit {
            suit_adidas: true,
            jacket_krutaya: true,
            ..Kit::default()
        };
        let out = sheet(&p, &kit);
        assert!(!out.iter().any(|l| l.contains("Adidas")), "{out:?}");
        assert!(!out.iter().any(|l| l.contains("Броня")), "{out:?}");
    }

    fn armour(out: &[String]) -> String {
        out.iter().find(|l| l.contains("Броня")).unwrap().clone()
    }

    #[test]
    fn best_item_wins_dims_the_superseded_suit_and_jacket() {
        let mut p = player();
        p.armor = 4;
        let a = |kit: Kit| armour(&sheet(&p, &kit));
        assert_eq!(a(Kit::default()), "^2Броня 4    ");
        assert_eq!(
            a(Kit {
                suit_abibas: true,
                ..Kit::default()
            }),
            "^2Броня 4    ^1Костюм Abibas(+1) "
        );
        assert_eq!(
            a(Kit {
                suit_adidas: true,
                ..Kit::default()
            }),
            "^2Броня 4    ^1Костюм Adidas(+2) "
        );
        assert_eq!(
            a(Kit {
                suit_abibas: true,
                suit_adidas: true,
                ..Kit::default()
            }),
            "^2Броня 4    ^4Abibas ^1Костюм Adidas(+2) "
        );
        assert_eq!(
            a(Kit {
                jacket: true,
                ..Kit::default()
            }),
            "^2Броня 4    ^1Кожанка(+2) "
        );
        assert_eq!(
            a(Kit {
                jacket_krutaya: true,
                ..Kit::default()
            }),
            "^2Броня 4    ^1Крутая кожанка(+4) "
        );
        assert_eq!(
            a(Kit {
                jacket: true,
                jacket_krutaya: true,
                ..Kit::default()
            }),
            "^2Броня 4    ^4Кожанка ^1Крутая кожанка(+4) "
        );
    }

    #[test]
    fn the_purse_lines_carry_the_beer_half_litre() {
        let mut p = player();
        p.joints = 2;
        p.beer_dl = 3;
        p.money = 50;
        p.junk = 7;
        let out = sheet(&p, &Kit::default());
        let tail = &out[out.len() - 4..];
        assert_eq!(tail, ["Косяки 2", "Пиво 1.5л.", "Бабки 50", "Хлам 7"]);
        // An EVEN count separates `beer mod 2` from `beer div 2` inside the
        // fraction: both give `1` for 3 half-litres, and only `mod` gives `0`
        // for 2.
        p.beer_dl = 2;
        assert!(sheet(&p, &Kit::default()).iter().any(|l| l == "Пиво 1.0л."));
        p.beer_dl = 4;
        assert!(sheet(&p, &Kit::default()).iter().any(|l| l == "Пиво 2.0л."));
    }

    #[test]
    fn an_empty_purse_prints_the_two_negative_lines_and_no_junk() {
        let out = sheet(&player(), &Kit::default());
        assert_eq!(out[out.len() - 2..], ["^4Пива нет", "^4Нету бабок"]);
        assert!(!out.iter().any(|l| l.contains("Косяки")), "{out:?}");
        assert!(!out.iter().any(|l| l.contains("Хлам")), "{out:?}");
    }
}
