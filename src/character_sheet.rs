//! The player's character sheet -- `FUN_1000_1a03`, `[1000:1a03, 1000:248f)`.
//!
//! 2700 bytes, 83 conditional branches, the third-largest function in
//! `orig/g.exe` and the largest one this port had not touched.
//! `docs/re/character-sheet.md` is the map and `data/character_sheet.json`
//! its machine-readable twin; this module is the port of what they describe.
//!
//! **Established from flow** throughout. Every address below was re-derived
//! from `orig/g.exe` for this implementation with `tools/dis16.py`, decoding
//! the whole range in one aligned walk from the function's entry, so each
//! citation is an instruction boundary the walk actually reached and each
//! quoted instruction is what decodes there. To reproduce:
//!
//! ```text
//! python3 tools/re_query.py resolve 1000:1a03
//! python3 -m unittest tools.test_character_sheet -v
//! ```
//!
//! And the strings this module SHIPS -- not the ones its comments quote --
//! are decoded out of `orig/g.exe` by
//! `python3 tools/test_character_sheet_port.py`, which also pins every
//! `CS 0x....` below to the literal beside it. Two cases in
//! `tools/mutations.json` (`character-sheet-port-literal`,
//! `character-sheet-port-citation`) show both checks going red.
//!
//! ## The split, and why it is this one
//!
//! The module **builds** the lines; [`crate::game::Game::show_stats`]
//! **prints** them -- the same split `crate::combat_dispatch` uses, and for
//! the same reason: `crate::term::println` writes to this process's stdout
//! and a unit test cannot capture it, so a renderer that printed as it went
//! would be untestable. [`lines`] returns the sheet in the original's order
//! and every branch below is assertable from a string comparison.
//!
//! ## Lines, not `Write` calls
//!
//! The original does not emit lines; it emits 30 `Write` calls
//! (`0eed:0000`), 20 `WriteLn` calls (`0eed:01c2`, the same colour-markup
//! formatter with a newline) and 6 bare Pascal `WriteLn` on the `Text` at
//! `20ae:3fcc` (`0f78:05dd` + the `{$I+}` check at `0f78:0291`) --
//! `docs/re/character-sheet.md`, "It reaches no game code", which counts
//! them. A `Write` leaves the line open and the next call continues it. So
//! [`Out`] keeps one open line, `Write` appends to it and either flavour of
//! `WriteLn` closes it; a bare `WriteLn` with nothing open yields an empty
//! line, which is how the two blank separators around the pistol block
//! arise. `crate::game::Game::wander_preamble` already models `1000:4a78`
//! the same way.
//!
//! ## What is NOT modelled
//!
//! * **The two health-colour thresholds' decimal values.** See
//!   [`HEALTH_BROWN_ABOVE`].
//! * **`0eed:0000`'s own `#` substitution when a value is not pushed.** The
//!   original pushes five words at every call site and pads the unused ones
//!   with `xor ax,ax` / `push ax`; `crate::text::fill` leaves a `#` with no
//!   value as a literal `#` instead of printing `0`. No literal this module
//!   passes has more `#` than it has values, so the two agree on every
//!   string here -- but a player name containing `#` would diverge, and the
//!   name line is the one place a `#` can arrive from data. Registered in
//!   `docs/re/gaps.md`.

use crate::combat;
use crate::combat_dispatch::Pistol;
use crate::data;
use crate::model::Fighter;
use crate::text;

/// The health line's colour digit is `'6'` above this ratio of `hp/hpmax`.
///
/// **This value is a PORT DECISION, not a finding.** What is established
/// from flow is only the ORDER of the two thresholds. `1000:2118` and
/// `1000:2148` each divide `hp` by `hpmax` in Turbo Pascal 6-byte reals
/// (`rtl_real_op_div`, `0f78:1117`) and `1000:2124` / `1000:2154` compare
/// the quotient against a comparand held in `CX:SI:DI`
/// (`rtl_real_op_cmp`, `0f78:1121`). The two comparands differ in exactly
/// one register and by exactly one: `1000:211d mov cx,0x7f` against
/// `1000:214d mov cx,0x80`, with `1000:2120`/`1000:2122` and
/// `1000:2150`/`1000:2152` zeroing `si` and `di` both times. `docs/re/rtl.md`
/// establishes that `CL` is the exponent byte (`0f78:1117`'s `or cl,cl` /
/// `je` is a zero-divisor test, and a Turbo `Real` is zero exactly when its
/// exponent byte is zero), so the two comparands have the same zero mantissa
/// and exponents one step apart: the second is strictly above the first, and
/// nothing else about them is established. The exponent BIAS is not --
/// `docs/re/rtl.md` records it as open for the `1000:4ff5` / `1000:5002`
/// constants too -- so "25% and 50%" would be a guess, and this port makes
/// it a labelled choice instead of a claim.
///
/// `docs/re/gaps.md`, "The decimal value of the health-colour thresholds",
/// carries the entry and what would settle it (two gdb pokes bracketing each
/// threshold). The only property this port guarantees against the original
/// is `HEALTH_BROWN_ABOVE < HEALTH_GREEN_ABOVE`, which
/// `the_health_colour_walks_four_six_two_and_never_back` asserts through
/// [`health_digit`] rather than by comparing the two constants -- that
/// comparison is constant-folded and could not fail.
pub const HEALTH_BROWN_ABOVE: f64 = 0.25;

/// The health line's colour digit is `'2'` above this ratio. Port decision
/// on the same footing as [`HEALTH_BROWN_ABOVE`] -- read that first.
pub const HEALTH_GREEN_ABOVE: f64 = 0.50;

/// Everything the sheet reads that is not a field of [`Fighter`].
///
/// Field names carry their DGROUP address because that is what identifies
/// them: the sheet is the only place several of these bytes are ever
/// printed, and `src/save.rs`'s `Items` documents each one's `.SAV` offset.
/// [`crate::game::Game`] owns the live values and copies them in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Kit {
    /// `20ae:38ce` -- XP not yet spent on a level
    /// (`crate::progress::Progress::xp`), pushed at `1000:1ab5`.
    pub xp_38ce: u32,
    /// `20ae:38d0` -- XP needed for the next level, pushed at `1000:1ab9`.
    pub threshold_38d0: u32,
    /// `20ae:38cd` -- the joint buff's countdown. Read three times: the
    /// Сила colour slot (`1000:1acb`), the damage line's colour
    /// (`1000:1e06`) and the `Обдолбаный` condition (`1000:20ca`). All three
    /// guards are unsigned (`ja` / `jbe`), so any non-zero count counts.
    pub buff_countdown_38cd: u8,
    /// `20ae:38bd` -- Крестик (`1000:1be9`).
    pub krestik_38bd: bool,
    /// `20ae:38be` -- кольцо "Гс" (`1000:1c09`).
    pub ring_gs_38be: bool,
    /// `20ae:38bf` -- кольцо "Пг" (`1000:1c69`).
    pub ring_pg_38bf: bool,
    /// `20ae:38c0` -- Мега Кольцо (`1000:1c89`).
    pub mega_ring_38c0: bool,
    /// `20ae:38c1` -- кольцо "Гп" (`1000:1ca9`).
    pub ring_gp_38c1: bool,
    /// `20ae:38bb` -- мобильник (`1000:1cd8`).
    pub mobile_38bb: bool,
    /// `20ae:38b3` -- тёмные очки (`1000:1cf8`).
    pub dark_glasses_38b3: bool,
    /// `20ae:38bc` -- зоновская наколка (`1000:1d18`).
    pub prison_tattoo_38bc: bool,
    /// `20ae:394d` / `394e` / `394f` -- the pistol block (`1000:1d38`).
    pub pistol: Pistol,
    /// `20ae:38b5` -- Бутсы (`1000:1e81`).
    pub boots_38b5: bool,
    /// `20ae:38b8` -- Понтовые бутсы (`1000:1ecf`).
    pub boots_pontovye_38b8: bool,
    /// `20ae:38ba` -- Кастет (`1000:1eef`).
    pub kastet_38ba: bool,
    /// `20ae:394b` -- Дубинка (`1000:1f59`).
    pub dubinka_394b: bool,
    /// `20ae:38c2` -- Нож (`1000:1fb5`).
    pub nozh_38c2: bool,
    /// `20ae:394c` -- Тесак (`1000:2003`).
    pub tesak_394c: bool,
    /// `20ae:394a` -- зубная защита (`1000:2068`). Its guard is
    /// `cmp byte [0x394a],0x1`, an EQUALITY, not the `cmp ..,0x0` the
    /// item flags use.
    pub tooth_guard_394a: bool,
    /// `20ae:38b4` -- костюм Abibas (`1000:22a1`).
    pub suit_abibas_38b4: bool,
    /// `20ae:38b7` -- костюм Adidas (`1000:22fc`).
    pub suit_adidas_38b7: bool,
    /// `20ae:38b6` -- Кожанка (`1000:2323`).
    pub jacket_38b6: bool,
    /// `20ae:38b9` -- Крутая кожанка (`1000:237e`).
    pub jacket_krutaya_38b9: bool,
}

/// One open line plus the lines already closed.
///
/// See the module doc: the original's `Write` / `WriteLn` split is what this
/// reproduces, and it is why a bare `WriteLn` can produce an empty line.
#[derive(Default)]
struct Out {
    lines: Vec<String>,
    open: String,
}

impl Out {
    /// `call 0eed:0x0` -- leaves the line open.
    fn write(&mut self, s: &str) {
        self.open.push_str(s);
    }

    /// `call 0eed:0x1c2` -- appends and closes the line.
    fn writeln(&mut self, s: &str) {
        self.open.push_str(s);
        self.newline();
    }

    /// `call 0f78:0x5dd` + `call 0f78:0x291` on the `Text` at `20ae:3fcc` --
    /// closes the line with nothing appended.
    fn newline(&mut self) {
        self.lines.push(std::mem::take(&mut self.open));
    }

    /// The function's own exit, `1000:248b` `mov sp,bp` / `1000:248d pop bp`
    /// / `1000:248e ret`.
    ///
    /// Nothing is flushed here because nothing can be open: the last two
    /// blocks are both unconditional two-armed `WriteLn` pairs --
    /// `1000:23d5` `jle 0x2415` picks `Пиво #.#л.` or `^4Пива нет`, and
    /// `1000:242e` `jle 0x2451` picks `Бабки #` or `^4Нету бабок` -- so the
    /// money line always closes, and the only thing that can follow it is
    /// the `Хлам #` `WriteLn` at `1000:2471`.
    fn finish(self) -> Vec<String> {
        self.lines
    }
}

/// The whole sheet, in the original's order.
///
/// `name` is the string at `DS:379c`, appended at `1000:1a90`; it is a
/// separate parameter rather than [`Fighter::name`] only because the
/// enemy record's name is never what this line prints -- the function takes
/// no arguments at all (`docs/re/character-sheet.md`, "The entry, and the
/// argument convention": a bare `ret` at `1000:248e` and not one positive
/// `bp` displacement in 2700 bytes) and reads the player's globals directly,
/// so all four of its call sites render the same sheet.
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

/// `1000:1a26`..`1000:1ac6` -- the class/level header, the name, the
/// experience line.
fn header(o: &mut Out, p: &Fighter, name: &str, kit: &Kit) {
    // Four appends, in this order: CS `0x1664` (`^2Ты `),
    // `ranks[class]` (`1000:1a44`),
    // CS `0x166a` (` # уровня - `),
    // and `krutizna[level]` (`1000:1a61`).
    // `1000:1a66` then pushes the level and `1000:1a76` is the WriteLn.
    o.writeln(&text::fill(
        &format!(
            "^2Ты {} # уровня - {}",
            data::rank_name(p.class),
            data::krutizna(p.level)
        ),
        &[i64::from(p.level)],
    ));
    // CS `0x1677` is `^2А зовут тебя: `;
    // `DS:379c` is appended onto it at `1000:1a90`.
    o.writeln(&format!("^2А зовут тебя: {name}"));
    // `1000:1aa9 cmp word [0x38a6],0x27` / `1000:1aae jnle 0x1acb`: at level
    // 40 there is no next threshold and the line is skipped -- which is what
    // a 43-entry ladder with a 40 cap (`1000:2580`) needs.
    if p.level <= 0x27 {
        o.writeln(&text::fill(
            // CS `0x1688`.
            "^6Сейчас у тебя # опыта, А для прокачки надо #",
            &[i64::from(kit.xp_38ce), i64::from(kit.threshold_38d0)],
        ));
    }
}

/// `1000:1acb`..`1000:1bbd` -- the four stats and the colour digits.
///
/// `1000:1a12` assigns the CS literal `7777` into the shortstring at
/// `[bp-0x100]`; each of its four characters is a Turbo colour digit that a
/// worn item patches to `'1'`, and the format string interleaves them.
fn stat_line(o: &mut Out, p: &Fighter, kit: &Kit) {
    let stoned = kit.buff_countdown_38cd > 0;
    let all = kit.ring_pg_38bf || kit.mega_ring_38c0;
    // `1000:1acb`/`1ad2`/`1ad9` -> `1000:1ae0 mov byte [bp-0xff],0x31`.
    let c0 = digit(stoned || all);
    // `1000:1ae5`/`1aec` -> `1000:1af3` and `1000:1af8`, one guard pair
    // setting BOTH slots.
    let c1 = digit(all);
    let c2 = c1;
    // `1000:1afd`/`1b04`/`1b0b`/`1b12` -> `1000:1b19`.
    let c3 = digit(kit.krestik_38bd || kit.ring_gs_38be || all);
    // Five literals interleaved with the four digits, appended in order:
    // CS `0x16b7` is `Сл:^`,
    // CS `0x16bc` is `#^7 Лв:^`,
    // CS `0x16c5` is `#^7 Жв:^`,
    // CS `0x16ce` is `#^7 Уд:^`.
    //
    // CS `0x16d7` is the last placeholder on its own.
    //
    // The four stats are pushed at `1000:1baa`..`1000:1bb6` and
    // `1000:1bbd` is the WriteLn.
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

/// `1000:1bc2`..`1000:1cd3` -- the two charm sections.
///
/// Each is a header `Write` gated on the disjunction of its own rows,
/// followed by the rows and one bare `WriteLn` that closes the line. Both
/// header disjunctions are among the 24 branches
/// `data/character_sheet.json`'s `branch_partition` leaves uncited; they are
/// ported here from the same aligned decode as everything else, and the
/// addresses below are the guards themselves.
fn charms(o: &mut Out, kit: &Kit) {
    // `1000:1bc2 cmp byte [0x38bd],0x0` / `jnz 0x1bd0`, `1000:1bc9
    // cmp byte [0x38be],0x0` / `jz 0x1c38` -- the whole block, including its
    // closing newline at `1000:1c29`, is skipped when neither is set.
    if kit.krestik_38bd || kit.ring_gs_38be {
        o.write("Феньки: "); // CS `0x16d9`
        if kit.krestik_38bd {
            o.write("^1Крестик(Удача +2) "); // CS `0x16e2`
        }
        if kit.ring_gs_38be {
            o.write("^1Кольцо \"Гс\"(Удача +1) "); // CS `0x16f7`
        }
        o.newline();
    }
    // `1000:1c38`/`1c3f`/`1c46`, whose all-clear arm is the
    // `1000:1c4d jmp 0x1cd8` over the section and its `1000:1cc9` newline.
    if kit.ring_pg_38bf || kit.mega_ring_38c0 || kit.ring_gp_38c1 {
        o.write("Мощные феньки: "); // CS `0x1710`
        if kit.ring_pg_38bf {
            o.write("^1Кольцо \"Пг\"(Всё +1) "); // CS `0x1720`
        }
        if kit.mega_ring_38c0 {
            o.write("^1Мега Кольцо(Всё +4) "); // CS `0x1737`
        }
        if kit.ring_gp_38c1 {
            o.write("^1Кольцо \"Гп\"(Самолечение) "); // CS `0x174e`
        }
        o.newline();
    }
}

/// `1000:1cd8`..`1000:1d33` -- the three items that get a whole line each.
fn worn_singletons(o: &mut Out, kit: &Kit) {
    if kit.mobile_38bb {
        o.writeln("^1У тебя есть мобильник"); // CS `0x176a`
    }
    if kit.dark_glasses_38b3 {
        o.writeln("^1У тебя есть тёмные очки"); // CS `0x1782`
    }
    if kit.prison_tattoo_38bc {
        o.writeln("^1На тебе зоновская наколка"); // CS `0x179c`
    }
}

/// `1000:1d38`..`1000:1dfc` -- the pistol, its silencer and its magazine.
///
/// `1000:1d38 cmp byte [0x394d],0x0` / `1000:1d3d jnz 0x1d42` skips the
/// entire block, blank separators included, when there is no pistol.
fn pistol_block(o: &mut Out, kit: &Kit) {
    if !kit.pistol.owned {
        return;
    }
    o.newline(); // `1000:1d42`..`1000:1d4c`
    o.write("^1У тебя есть пистолет"); // CS `0x17b8`
    if kit.pistol.silencer {
        o.write("^1 с гушителем"); // CS `0x17cf`, the game's own typo
    }
    let n = kit.pistol.cartridges;
    // `1000:1d8a cmp word [0x394f],0x0` / `jle 0x1dab`. A signed word.
    if n > 0 {
        o.writeln(&text::fill("^1! патронов - #", &[i64::from(n)])); // CS `0x17de`
    }
    // `1000:1dab cmp word [0x394f],0x2` / `jnle 0x1dd2`, then
    // `1000:1db2 cmp word [0x394f],0x0` / `jle 0x1dd2`: 1 or 2 rounds left.
    if (1..=2).contains(&n) {
        o.writeln("^6 А птронов-то мало "); // CS `0x17ef`
    }
    // `1000:1dd2 cmp word [0x394f],0x0` / `jnle 0x1df2`.
    if n <= 0 {
        o.writeln("^1.^4 Правда без патронов"); // CS `0x1805`
    }
    o.newline(); // `1000:1df2`..`1000:1dfc`
}

/// `1000:1e01`..`1000:202d` -- the damage line and every hand weapon.
///
/// The line's own colour digit lives in `[bp-0x101]`: `1000:1e01` sets it to
/// `'7'` and the seven-way disjunction at `1000:1e06`..`1000:1e35` raises it
/// to `'1'`. Then `1000:1e42` starts the string with the CS literal `^`
/// (`0x181f`), appends that digit, appends `Урон #-#    ` and `Write`s the
/// pair at `1000:1e7c`, leaving the line open for the weapon labels.
fn damage_line(o: &mut Out, p: &Fighter, kit: &Kit) {
    let armed = kit.buff_countdown_38cd > 0
        || kit.boots_38b5
        || kit.boots_pontovye_38b8
        || kit.kastet_38ba
        || kit.dubinka_394b
        || kit.nozh_38c2
        || kit.tesak_394c;
    o.write(&text::fill(
        // CS `0x1821` `Урон #-#    `, four trailing spaces.
        &format!("^{}Урон #-#    ", digit(armed)),
        &[i64::from(p.dmg_min), i64::from(p.dmg_max)],
    ));
    // Best-item-wins: each pair prints the superseded item dim (`^4`) beside
    // the good one, as two arms sharing the lesser item's flag.
    // `1000:1e81`/`1e88` and `1000:1ea8`/`1eaf`.
    if kit.boots_38b5 && !kit.boots_pontovye_38b8 {
        o.write("^1Бутсы(+1) "); // CS `0x182e`
    }
    if kit.boots_38b5 && kit.boots_pontovye_38b8 {
        o.write("^4Бутсы "); // CS `0x183b`
    }
    if kit.boots_pontovye_38b8 {
        o.write("^1Понтовые бутсы(Урон+2) "); // CS `0x1844`
    }
    // The three blades supersede the кастет, `1000:1eef`..`1000:1f09`; the
    // dim arm is `1000:1f24`..`1000:1f3e`.
    let over_kastet = kit.nozh_38c2 || kit.dubinka_394b || kit.tesak_394c;
    if kit.kastet_38ba && !over_kastet {
        o.write("^1Кастет(+2) "); // CS `0x185e`
    }
    if kit.kastet_38ba && over_kastet {
        o.write("^4Кастет "); // CS `0x186c`
    }
    // `1000:1f59`/`1f60`/`1f67` and `1000:1f87`/`1f8e`/`1f95`.
    let over_dubinka = kit.nozh_38c2 || kit.tesak_394c;
    if kit.dubinka_394b && !over_dubinka {
        o.write("^1Дубинка(+4)  "); // CS `0x1876`
    }
    if kit.dubinka_394b && over_dubinka {
        o.write("^4Дубинка "); // CS `0x1886`
    }
    // `1000:1fb5`/`1fbc` and `1000:1fdc`/`1fe3`.
    if kit.nozh_38c2 && !kit.tesak_394c {
        o.write("^1Нож(+6) "); // CS `0x1891`
    }
    if kit.nozh_38c2 && kit.tesak_394c {
        o.write("^4Нож "); // CS `0x189c`
    }
    // `1000:2003` -- nothing supersedes the тесак.
    if kit.tesak_394c {
        o.write("^1Тесак(Урон+9) "); // CS `0x18a3`
    }
    o.newline(); // `1000:2023`..`1000:202d`
}

/// `1000:2032`..`1000:21ab` -- the health line and the four conditions.
///
/// The conditions are NOT separate lines. `1000:2032` empties the
/// shortstring at `[bp-0x100]` (the same local the `7777` colour digits used
/// earlier) and each condition appends its label to it; `1000:2195` then
/// appends the whole accumulator to the health line after
/// `Здоровье #/#  `, and only `1000:21ab` closes it.
fn health_line(o: &mut Out, p: &Fighter, kit: &Kit) {
    let mut cond = String::new();
    // `1000:2037 cmp byte [0x38b0],0x1` / `jnz 0x2068` -- equality, not `> 0`.
    if p.broken_jaw {
        cond.push_str("^4Сломана челюсть  "); // CS `0x18b4`
    }
    if kit.tooth_guard_394a {
        cond.push_str("^1Зубная защита  "); // CS `0x18c8`
    }
    if p.broken_leg {
        cond.push_str("^4Сломана нога  "); // CS `0x18da`
    }
    // `1000:20ca cmp byte [0x38cd],0x0` / `jbe 0x20fb` -- unsigned.
    if kit.buff_countdown_38cd > 0 {
        cond.push_str("^6Обдолбаный  "); // CS `0x18eb`
    }
    o.writeln(&text::fill(
        // `1000:2166` opens the string with the one-character CS literal at
        // 0x181f and `1000:217b` appends the colour digit, then CS `0x18fa`
        // `Здоровье #/#  ` and the conditions accumulator.
        &format!("^{}Здоровье #/#  {cond}", health_digit(p.hp, p.hpmax)),
        &[i64::from(p.hp), i64::from(p.hpmax)],
    ));
}

/// `1000:20fb`..`1000:215b` -- `'4'`, then `'6'`, then `'2'`.
///
/// The two thresholds are [`HEALTH_BROWN_ABOVE`] and [`HEALTH_GREEN_ABOVE`];
/// read that doc before trusting either number. Only their ORDER is the
/// original's.
///
/// `hpmax == 0` is a **port decision**: `0f78:1117`'s `or cl,cl` / `je`
/// rejects a zero divisor, and `docs/re/rtl.md` does not establish what it
/// returns, so this port keeps the `1000:20fb` default `'4'` rather than
/// guessing. Nothing in play reaches it -- `1000:49ca` and `1000:4a30`, the
/// only writers that lower `hpmax`, are `dec` and `sub 5` on a value the
/// creation block seeds well above zero -- but a hand-built `Fighter` can.
fn health_digit(hp: u16, hpmax: u16) -> char {
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

/// `1000:21b0`..`1000:2276` -- the accuracy block, from Ловкость alone.
///
/// The arithmetic is **not** reimplemented here: `crate::combat` already
/// carries it from the enemy sheet (`FUN_1000_1348`, whose copy of this
/// block is `1000:1574`..`1000:15e7`), and the two agree because the sheet
/// computes an *unopposed* budget. `crate::combat::blow_budget` is
/// `attacker.agility + 4` unless the defender's own budget exceeds 18, and
/// against a defaulted `Fighter` the defender's is 4 -- so the loop at that
/// function's `while theirs > PER_BLOW` never runs and the budget is exactly
/// the `agility + 4` this block works from.
///
/// The identities, each checked by a test below:
///
/// * `1000:21c9`..`1000:21cf` (`shl`, `shl`, `add si`, `add 0x14`) is
///   `agility * 5 + 20`, and `blow_budget * 5` is the same number.
/// * `1000:2204 sub ax,0xe` then the `1000:2211`..`1000:2221` loop spends 18
///   per extra hit. The hit counter at `[bp-0x106]` ends one BELOW
///   `crate::combat::blows_per_round`, and what is left in `[bp-0x104]` is
///   exactly `accuracy_pct_nth`'s budget at that blow index divided by 5.
fn accuracy_block(o: &mut Out, p: &Fighter) {
    let unopposed = Fighter::default();
    // `1000:21b7 cmp word [bp-0x104],0xe` / `1000:21bc jnle 0x21e7`.
    if p.agility <= 0xe {
        o.writeln(&text::fill(
            // CS `0x1909`.
            "Точность #%",
            &[i64::from(combat::accuracy_pct(p, &unopposed))],
        ));
        return;
    }
    o.write("Точность 90% "); // CS `0x1915`, trailing space, no newline
    let extra = combat::blows_per_round(p, &unopposed) - 1;
    let pct = i64::from(combat::accuracy_pct_nth(p, &unopposed, extra));
    // `1000:2223 cmp word [bp-0x106],0x1` / `1000:2228 jnz 0x224d`.
    if extra == 1 {
        // CS `0x1923`, three leading spaces.
        o.writeln(&text::fill("   Второй удар #%", &[pct]));
    }
    // `1000:224d cmp word [bp-0x106],0x1` / `1000:2252 jle 0x227b`.
    if extra > 1 {
        o.writeln(&text::fill(
            // CS `0x1935`, two spaces after the comma.
            "- # ударов,  Точность # удара #%",
            &[i64::from(extra), i64::from(extra) + 1, pct],
        ));
    }
}

/// `1000:227b`..`1000:23af` -- the armour line, the two suits and the two
/// jackets.
///
/// `1000:227b cmp byte [0x38b2],0x0` / `1000:2280 ja 0x2285` -- an UNSIGNED
/// test, and its else arm (`1000:2282 jmp 0x23b4`) skips the clothing rows
/// and the closing newline too. So a player with no armour never sees a suit
/// or a jacket listed, however many are worn.
///
/// Both clothing pairs are three arms, not two: `1000:22a1`/`1000:22a8`
/// splits the lesser item's flag into "dim label then the better one's
/// bright label" (`1000:22af`, `1000:22c8`) and "the lesser one's own bright
/// label" (`1000:22e3`), and a THIRD guard at `1000:22fc`/`1000:2303` prints
/// the better item alone when the lesser one is not owned.
fn armour_block(o: &mut Out, p: &Fighter, kit: &Kit) {
    if p.armor == 0 {
        return;
    }
    // CS `0x1956`, four trailing spaces. `1000:228a` loads the byte and
    // zero-extends it.
    o.write(&text::fill("^2Броня #    ", &[i64::from(p.armor)]));
    if kit.suit_abibas_38b4 {
        if kit.suit_adidas_38b7 {
            o.write("^4Abibas "); // CS `0x1964`
            o.write("^1Костюм Adidas(+2) "); // CS `0x196e`
        } else {
            o.write("^1Костюм Abibas(+1) "); // CS `0x1983`
        }
    }
    if kit.suit_adidas_38b7 && !kit.suit_abibas_38b4 {
        o.write("^1Костюм Adidas(+2) "); // CS `0x196e`
    }
    if kit.jacket_38b6 {
        if kit.jacket_krutaya_38b9 {
            o.write("^4Кожанка "); // CS `0x1998`
            o.write("^1Крутая кожанка(+4) "); // CS `0x19a3`
        } else {
            o.write("^1Кожанка(+2) "); // CS `0x19b9`
        }
    }
    if kit.jacket_krutaya_38b9 && !kit.jacket_38b6 {
        o.write("^1Крутая кожанка(+4) "); // CS `0x19a3`
    }
    o.newline(); // `1000:23a5`..`1000:23af`
}

/// `1000:23b4`..`1000:2486` -- косяки, пиво, бабки, хлам.
fn purse(o: &mut Out, p: &Fighter) {
    // `1000:23b4 cmp word [0x38c5],0x0` / `jle 0x23d5`.
    if p.joints > 0 {
        o.writeln(&text::fill("Косяки #", &[i64::from(p.joints)])); // CS `0x19c8`
    }
    // `1000:23d5` / `jle 0x2415`. Пиво is stored in HALF-litres:
    // `1000:23e5`/`23e8` is `idiv 2` and `1000:23f4`..`1000:2403` is
    // `((remainder * 5) mod 10)`, so an odd count prints `.5`.
    //
    // The `mod 10` the original spends four instructions on
    // (`1000:23fe mov cx,0xa` / `2401 idiv cx` / `2403 xchg ax,dx`) can
    // never change the value: its input is 0 or 5. It is kept because this
    // is a port, and the cost is one EQUIVALENT mutant that
    // `cargo mutants -f src/character_sheet.rs` reports and no test can
    // kill -- `% 2` -> `+ 2`, since `((b + 2) * 5) mod 10` equals
    // `((b mod 2) * 5) mod 10` for every `b` (checked over the whole `u16`
    // range). It is the only survivor of the 107.
    if p.beer_dl > 0 {
        o.writeln(&text::fill(
            "Пиво #.#л.", // CS `0x19d1`
            &[
                i64::from(p.beer_dl / 2),
                (i64::from(p.beer_dl % 2) * 5) % 10,
            ],
        ));
    } else {
        o.writeln("^4Пива нет"); // CS `0x19dc`
    }
    // `1000:242e` / `jle 0x2451`.
    if p.money > 0 {
        o.writeln(&text::fill("Бабки #", &[i64::from(p.money)])); // CS `0x19e7`
    } else {
        o.writeln("^4Нету бабок"); // CS `0x19ef`
    }
    // `1000:246a` / `jle 0x248b`.
    if p.junk > 0 {
        o.writeln(&text::fill("Хлам #", &[i64::from(p.junk)])); // CS `0x19fc`
    }
}

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
    /// `1000:214d`'s comparand is strictly above `1000:211d`'s, so a rising
    /// ratio walks `'4'` -> `'6'` -> `'2'`, hits all three, and never goes
    /// back.
    ///
    /// Deliberately not `assert!(HEALTH_BROWN_ABOVE < HEALTH_GREEN_ABOVE)`:
    /// that comparison is constant-folded (clippy says so), so it is the
    /// check-that-cannot-fail `docs/re/METHODOLOGY.md` names. This walks
    /// `health_digit` instead and reds when the two constants are swapped
    /// (`'6'` then never occurs) or made equal.
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
            xp_38ce: 42,
            threshold_38d0: 60,
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
        // 20ae:38cd -- Сила alone.
        assert_eq!(
            line(&Kit {
                buff_countdown_38cd: 1,
                ..Kit::default()
            }),
            "Сл:^16^7 Лв:^77^7 Жв:^78^7 Уд:^79"
        );
        // 20ae:38bd -- Удача alone.
        assert_eq!(
            line(&Kit {
                krestik_38bd: true,
                ..Kit::default()
            }),
            "Сл:^76^7 Лв:^77^7 Жв:^78^7 Уд:^19"
        );
        // 20ae:38be -- Удача alone.
        assert_eq!(
            line(&Kit {
                ring_gs_38be: true,
                ..Kit::default()
            }),
            "Сл:^76^7 Лв:^77^7 Жв:^78^7 Уд:^19"
        );
        // 20ae:38bf and 20ae:38c0 -- all four.
        for kit in [
            Kit {
                ring_pg_38bf: true,
                ..Kit::default()
            },
            Kit {
                mega_ring_38c0: true,
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
            ring_gs_38be: true,
            ..Kit::default()
        };
        assert_eq!(
            find(&sheet(&p, &kit), "Феньки").unwrap(),
            "Феньки: ^1Кольцо \"Гс\"(Удача +1) "
        );

        let kit = Kit {
            krestik_38bd: true,
            ring_gs_38be: true,
            ..Kit::default()
        };
        assert_eq!(
            find(&sheet(&p, &kit), "Феньки").unwrap(),
            "Феньки: ^1Крестик(Удача +2) ^1Кольцо \"Гс\"(Удача +1) "
        );

        let kit = Kit {
            ring_pg_38bf: true,
            mega_ring_38c0: true,
            ring_gp_38c1: true,
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
                    mobile_38bb: true,
                    ..Kit::default()
                },
                "^1У тебя есть мобильник",
            ),
            (
                Kit {
                    dark_glasses_38b3: true,
                    ..Kit::default()
                },
                "^1У тебя есть тёмные очки",
            ),
            (
                Kit {
                    prison_tattoo_38bc: true,
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
        assert!(joined(&armed(0, true)).contains("^1У тебя есть пистолет^1 с гушителем"));
    }

    fn damage(out: &[String]) -> String {
        out.iter().find(|l| l.contains("Урон")).unwrap().clone()
    }

    #[test]
    fn the_damage_line_is_dim_until_something_boosts_it() {
        let p = player();
        assert_eq!(damage(&sheet(&p, &Kit::default())), "^7Урон 3-6    ");
        // `1000:1e06`..`1000:1e35` is a seven-way `or`, and EACH term has to
        // raise the digit on its own -- one `||` written `&&` survives every
        // test that only ever sets two of them together.
        let setters: [fn(&mut Kit); 7] = [
            |k| k.buff_countdown_38cd = 3,
            |k| k.boots_38b5 = true,
            |k| k.boots_pontovye_38b8 = true,
            |k| k.kastet_38ba = true,
            |k| k.dubinka_394b = true,
            |k| k.nozh_38c2 = true,
            |k| k.tesak_394c = true,
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
            boots_38b5: true,
            ..Kit::default()
        };
        assert_eq!(d(boots), "^1Урон 3-6    ^1Бутсы(+1) ");
        assert_eq!(
            d(Kit {
                boots_pontovye_38b8: true,
                ..boots
            }),
            "^1Урон 3-6    ^4Бутсы ^1Понтовые бутсы(Урон+2) "
        );
        let kastet = Kit {
            kastet_38ba: true,
            ..Kit::default()
        };
        assert_eq!(d(kastet), "^1Урон 3-6    ^1Кастет(+2) ");
        assert_eq!(
            d(Kit {
                dubinka_394b: true,
                ..kastet
            }),
            "^1Урон 3-6    ^4Кастет ^1Дубинка(+4)  "
        );
        assert_eq!(
            d(Kit {
                nozh_38c2: true,
                dubinka_394b: true,
                ..kastet
            }),
            "^1Урон 3-6    ^4Кастет ^4Дубинка ^1Нож(+6) "
        );
        assert_eq!(
            d(Kit {
                tesak_394c: true,
                nozh_38c2: true,
                dubinka_394b: true,
                ..kastet
            }),
            "^1Урон 3-6    ^4Кастет ^4Дубинка ^4Нож ^1Тесак(Урон+9) "
        );
        // The тесак alone supersedes nothing, so no dim label appears.
        assert_eq!(
            d(Kit {
                tesak_394c: true,
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
            tooth_guard_394a: true,
            buff_countdown_38cd: 2,
            ..Kit::default()
        };
        assert_eq!(
            health(&sheet(&p, &kit)),
            "^2Здоровье 20/20  ^4Сломана челюсть  ^1Зубная защита  \
             ^4Сломана нога  ^6Обдолбаный  "
        );
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
        // Port decision: a zero hpmax keeps the 1000:20fb default.
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
        // `1000:2252 jle 0x227b` -- at exactly one extra hit the `# ударов`
        // line does NOT also print. An `any` assertion above cannot see a
        // spurious extra line, so the absence is asserted separately.
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
            suit_adidas_38b7: true,
            jacket_krutaya_38b9: true,
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
                suit_abibas_38b4: true,
                ..Kit::default()
            }),
            "^2Броня 4    ^1Костюм Abibas(+1) "
        );
        assert_eq!(
            a(Kit {
                suit_adidas_38b7: true,
                ..Kit::default()
            }),
            "^2Броня 4    ^1Костюм Adidas(+2) "
        );
        assert_eq!(
            a(Kit {
                suit_abibas_38b4: true,
                suit_adidas_38b7: true,
                ..Kit::default()
            }),
            "^2Броня 4    ^4Abibas ^1Костюм Adidas(+2) "
        );
        assert_eq!(
            a(Kit {
                jacket_38b6: true,
                ..Kit::default()
            }),
            "^2Броня 4    ^1Кожанка(+2) "
        );
        assert_eq!(
            a(Kit {
                jacket_krutaya_38b9: true,
                ..Kit::default()
            }),
            "^2Броня 4    ^1Крутая кожанка(+4) "
        );
        assert_eq!(
            a(Kit {
                jacket_38b6: true,
                jacket_krutaya_38b9: true,
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
        // An EVEN count is the case that separates `beer mod 2` from
        // `beer div 2` inside the fraction: both give `1` for 3 half-litres,
        // and only `mod` gives `0` for 2 (`1000:23f4 xchg ax,dx` takes the
        // REMAINDER of the `1000:23f2 idiv cx`, not the quotient).
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
