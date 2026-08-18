//! The main loop: dispatch, locations, and the handlers small enough to
//! belong here rather than in their own module.
//!
//! ## Every user-visible string here is a verbatim byte range of `orig/g.exe`
//!
//! Nothing in this module composes, paraphrases or translates game text.
//! Each literal below is quoted from `data/strings.json` with its file
//! offset, keeping its `^N` colour markup, its typos, its double spaces and
//! its trailing padding. `crate::term` is the only writer; it applies the
//! colour policy itself, so the markup must survive into what it receives.
//!
//! Ghidra convention used by every citation: a `1000:XXXX` code address is
//! at file offset `0x18d0 + XXXX`, and a `mov di,<n>` / `push cs` string
//! operand at `1000:XXXX` names the string at file offset `0x18d0 + n`.
//!
//! ## This module was substantially redesigned mid-task
//!
//! The brief's `game.rs` sketch was a flat, stateless dispatcher: any verb
//! reachable from any location, and `fight()` resolving a whole battle
//! synchronously inside one command. Disassembling `entry` (see
//! `crate::commands`' module doc for the method) disproved both:
//!
//! * **The prompt is a bare `\`, not `"> "`.** Confirmed two ways: the live
//!   capture (`docs/re/oracle-captures/command-table-and-combat.md`) and the
//!   binary itself -- file offset `0x9BF1` is the one-byte Pascal shortstring
//!   `"\"`, printed repeatedly through `entry`.
//! * **Combat is modal.** `FUN_1000_3d11` (`docs/re/combat.md`) runs its own
//!   `^0Битва\` prompt loop (file `0x4A49`); the live capture shows it
//!   rejecting `mar` and `i` outright rather than routing them anywhere.
//! * **Walking (`w`/`run`) rolls for a random encounter**, which itself
//!   reads a *second* line (into a different variable, `DS:3a72`) answering
//!   `"Хочешь наехать?"` -- confirmed by disassembling `1000:ae5a`..`1000:b82c`
//!   (see [`Game::walk`]'s doc for the full trace, with addresses).
//! * **Locations are their own modal loop.** This was flagged as an
//!   *inference* by the previous revision of this task; it is now
//!   **confirmed**. Each location handler ends by writing its own prompt
//!   string and then `ReadLn`-ing into `DS:3a72` -- the same second input
//!   variable combat uses, not the top-level `DS:3972`. The prompt strings
//!   are real and distinct per location: `^0Базар\` (file `0xA691`, written
//!   at `1000:bd08`, `ReadLn` at `1000:bd21`), `^0Барыги\` (`0xAC4B`),
//!   `^0Ветеренар\` (`0xB313`), `^0Притон\` (`0xB787`), `^0Клуб\` (`0xBAB2`),
//!   `^0Качалка\` (`0xBD43`). `girl` has no prompt string and no `ReadLn`:
//!   it is **not** modal, and [`Game::visit_girl`] runs it to completion in
//!   one turn, matching `1000:d701`..`1000:d798`.
//!
//! ## No typed save command
//!
//! `crate::commands` documents why `sv` is not save. Saving in the original
//! is checkpoint-only: `docs/re/tables.md`'s "Other price sources" section
//! names two save-triggering sites, `1000:761d` (a paid service,
//! `district * 50` rubles) and a second path at `0x9bcd` -- both
//! location-bound, neither a typed verb, and neither modelled by
//! [`crate::locations::Location`] (extending it would need disassembling the
//! paid-save screen itself, out of this task's scope). No command here
//! triggers a save.
//!
//! [`Game::write_save`] still exists as infrastructure a future
//! save-capable location could call. It always returns `Unsupported` today:
//! nothing in this task populates `save_template_bytes` (loading an existing
//! `.SAV` into a `Game` is out of scope here), and fabricating the unknown
//! bytes at `.SAV` offsets `0x214`/`0x2ae` for a freshly created character is
//! explicitly out of bounds per the task's own instruction. That is the
//! reported blocker, scoped to this one method.

use crate::combat::{blows_per_round, resolve_blow_nth, Break};
use crate::commands::{parse, Command};
use crate::data;
use crate::locations::{Location, Places};
use crate::model::Fighter;
use crate::progress::{self, Progress};
use crate::rng::Rng;
use crate::save::Save;
use crate::term;
use crate::text;
use std::io::{self, BufRead};

/// What the main loop is currently doing. Only [`Mode::Street`] dispatches
/// the full verb table (`crate::commands::parse`'s whole vocabulary);
/// [`Mode::Shop`] reads its own restricted key set at the location's own
/// prompt and ignores everything else, matching the per-location
/// `ReadLn DS:3a72` loops cited in the module doc.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    Street,
    Shop(Location),
}

pub struct Game {
    pub player: Fighter,
    pub progress: Progress,
    pub places: Places,
    pub district: u8,
    pub rng: Rng,
    pub location: Location,
    mode: Mode,
    /// The most recently fought opponent, shown by `Command::Inspect` (`sv`).
    last_enemy: Option<Fighter>,
    /// The original save this game was loaded from, kept only for its
    /// unknown byte regions. Always `None` for a freshly created character;
    /// [`Game::write_save`] is `Unsupported` until something populates this.
    save_template_bytes: Option<Vec<u8>>,
    running: bool,
}

impl Game {
    /// Start a brand-new character.
    ///
    /// `district` starts at 1: the original's own load-failure fallback says
    /// so in words -- `^6Чё-то глюкануло - нaверно нет такого сейва,
    /// Default:1` (file `0x7D21`) -- and its district-select screen calls 1
    /// "start from the beginning" (`^0Нажми цифру с какого района начать.
    /// 1-начать сначала`, file `0x7C69`). `orig/SAVE_R0.SAV` exists but is a
    /// *save slot* file, not evidence of a district 0 start; this task did
    /// not disassemble the district-select screen, so nothing here reads it.
    ///
    /// `places` starts with nothing discovered.
    pub fn new(player: Fighter, progress: Progress, seed: u32) -> Game {
        Game {
            player,
            progress,
            places: Places::from_bytes(&[0u8; 7]),
            district: 1,
            rng: Rng::new(seed),
            location: Location::Street,
            mode: Mode::Street,
            last_enemy: None,
            save_template_bytes: None,
            running: true,
        }
    }

    /// The banner is printed once by `main.rs` before character creation,
    /// matching a DOS splash-then-prompt startup; `run()` itself does not
    /// print it again (only `Command::Version` calls [`Game::banner`]).
    pub fn run(&mut self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();
        while self.running {
            self.prompt();
            let Some(line) = lines.next() else { break };
            let line = line?;
            match self.mode.clone() {
                Mode::Street => {
                    let cmd = parse(&line);
                    self.dispatch(cmd, &mut lines)?;
                }
                Mode::Shop(loc) => self.shop_turn(loc, &line),
            }
        }
        Ok(())
    }

    /// The street prompt is confirmed at file `0x9BF1`: a one-byte Pascal
    /// shortstring `"\"`. Each location writes its own prompt instead --
    /// see the module doc for the six offsets and the `1000:bd08`/`1000:bd21`
    /// write-then-`ReadLn` pair that proves the pattern.
    fn prompt(&self) {
        let p = match &self.mode {
            Mode::Street => "\\",
            Mode::Shop(Location::Market) => "^0Базар\\",
            Mode::Shop(Location::BigMarket) => "^0Барыги\\",
            Mode::Shop(Location::Vet) => "^0Ветеренар\\",
            Mode::Shop(Location::Den) => "^0Притон\\",
            Mode::Shop(Location::Club) => "^0Клуб\\",
            Mode::Shop(Location::Gym) => "^0Качалка\\",
            Mode::Shop(_) => "\\",
        };
        term::print(p);
    }

    fn banner(&self) {
        term::println("^4Gopnik: ^7version 1.02 june,sept 2003");
    }

    fn dispatch(&mut self, cmd: Command, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        match cmd {
            Command::Quit => self.running = false,
            Command::Stats => self.show_stats(),
            // file 0xC343, printed immediately after `k`'s own compare at
            // 1000:ecc7. `^6`, not `^4`.
            Command::Fight => {
                term::println("^6Чё машешь копытами? Ищи мудака которого будешь пинать!")
            }
            Command::Shoot => self.shoot(),
            Command::Inspect => self.inspect_enemy(),
            Command::Backup => self.call_backup(),
            Command::Walk => self.walk(lines)?,
            // file 0xB58A -- the inner `^6w^7` markup is part of the string.
            Command::LegacyFight => {
                term::println("^6Пережитки прошлого жми ^6w^7 чтобы искать врагов");
            }
            Command::Market => self.enter_shop(Location::Market),
            Command::BigMarket => self.enter_shop(Location::BigMarket),
            Command::Vet => self.enter_shop(Location::Vet),
            Command::Girl => self.enter_shop(Location::Girl),
            Command::Den => self.enter_shop(Location::Den),
            Command::Club => self.enter_shop(Location::Club),
            Command::Gym => self.enter_shop(Location::Gym),
            Command::CommandList => self.show_command_list(),
            Command::Help => self.show_help(),
            Command::Version => self.banner(),
            Command::Name => self.rename(lines)?,
            Command::Joint => self.smoke(),
            Command::Drink => self.beer(Beer::One),
            Command::BingeDrink => self.beer(Beer::Binge),
            Command::SellJunk => self.sell_junk(),
            Command::SellItems => self.sell_items(),
            // An unmatched line writes nothing at all: the last compare in
            // the chain (`exit`/`e`, 1000:edfa) falls through to
            // `jmp 0xab75` at 1000:ee01, straight back to the top of the
            // loop, with no output in between. The `^4? <input>` line an
            // earlier revision printed here was composed, not a real string.
            Command::Key(_) | Command::Unknown(_) => {}
        }
        Ok(())
    }

    /// The "you have not found this place yet" refusal, one verbatim string
    /// per location. Every one was read off its own gate branch: the token
    /// compare jumps to `cmp byte [<flag>],1`, and the not-equal arm jumps
    /// to a block that writes exactly one string.
    ///
    /// | verb | gate | flag | refusal jumps to | string |
    /// |---|---|---|---|---|
    /// | `mar` | `1000:b954` | `20ae:3694` | `1000:c49b` | file `0xA9F8` |
    /// | `bmar` | `1000:c4c8` | `20ae:3695` | `1000:d383` | file `0xB1CC` |
    /// | `pr` | `1000:d80c` | `20ae:3696` | `1000:dee3` | file `0xB980` |
    /// | `girl` | `1000:d6f7` | `20ae:3697` | `1000:d7b5` | file `0xB568` |
    /// | `rep` | `1000:d3b0` | `20ae:3698` | `1000:d6ca` | file `0xB440` |
    /// | `kl` | `1000:df10` | `20ae:3699` | `1000:e36d` | file `0xBBF6` |
    /// | `trn` | `1000:e39a` | `20ae:369a` | `1000:e948` | file `0xBEC2` |
    fn undiscovered_line(loc: Location) -> &'static str {
        match loc {
            Location::Market => "^6Ты незнаешь, пока ешё, где находтся базар",
            Location::BigMarket => {
                "^6Туда любого дебила с улицы непропустят - сначала докажи, что ты не засранец - отпинай побольше ублюдков"
            }
            Location::Den => "^4Тебя мудака такого туда не пустят - поднимай понтовость",
            Location::Girl => "^4У тебя пока нет девчонки.",
            Location::Vet => "^6Сначала найди где находтся эта больница",
            Location::Club => "^6Ты пока что неузнал где в этом районе клуб",
            Location::Gym => "^6Ты пока незнаешь где в этом районе качалка",
            Location::Street | Location::Temple | Location::Dorm => "",
        }
    }

    /// `mar`/`bmar`/`rep`/`girl`/`pr`/`kl`/`trn`, gated by [`Places::is_found`]
    /// exactly as the original gates on its seven contiguous discovery flags
    /// `20ae:3694`..`20ae:369a` (see [`Game::undiscovered_line`]).
    ///
    /// A refused entry only prints. It does **not** discover the place: the
    /// original's flags are set elsewhere, never by a failed entry.
    ///
    /// Two of those setters are implemented here, both established from
    /// flow and both re-derived from `orig/g.exe`:
    ///
    /// * `1000:d751` `c6 06 99 36 01` -- `mov byte [0x3699],1`, the
    ///   **club's** flag, set by `girl` ([`Game::visit_girl`]).
    /// * `1000:b570` `c6 06 97 36 01` -- `mov byte [0x3697],1`, the
    ///   **girl's** flag, set by the wander path's bucket 2
    ///   ([`Game::wander_girl`]). `1000:b575` is the `eb 19` `jmp` that
    ///   follows the store, not the store; and `0x3697` is the girl's flag,
    ///   not the den's -- the den is `0x3696` (gate `1000:d80c`), the girl
    ///   `0x3697` (gate `1000:d6f7`). An earlier revision of this comment
    ///   got both wrong.
    ///
    /// So `w` -> girl -> club is a real, reachable chain. The remaining five
    /// flags are set by wander events this port does not model; they are
    /// listed with their addresses in `docs/re/gaps.md`.
    fn enter_shop(&mut self, loc: Location) {
        if !self.places.is_found(loc) {
            term::println(Self::undiscovered_line(loc));
            return;
        }
        self.location = loc;
        if loc == Location::Girl {
            // Not modal: no prompt string, no ReadLn (1000:d701..1000:d798).
            self.visit_girl();
            self.location = Location::Street;
            return;
        }
        self.mode = Mode::Shop(loc);
        self.print_shop_intro(loc);
    }

    /// `girl`, `1000:d701`..`1000:d798`, in order:
    ///
    /// * `1000:d701` -- needs 12 rubles (`cmp word [0x38c7],0xc`); otherwise
    ///   file `0xB53D` and nothing else happens.
    /// * `1000:d70b` -- file `0xB46F`.
    /// * `1000:d724` -- `Random(2)`; on `0`, and only if the club is still
    ///   undiscovered (`cmp byte [0x3699],0`), prints file `0xB48C` and sets
    ///   the club's discovery flag at `1000:d751`. This is one of the two
    ///   discovery paths in the game.
    /// * `1000:d756`/`1000:d76f` -- files `0xB4CD`, `0xB4F6`.
    /// * `1000:d788`..`1000:d793` -- `hp := hpmax`, `money -= 12`, and clears
    ///   the pursuit flag `20ae:3b76` (not modelled here).
    fn visit_girl(&mut self) {
        if self.player.money < 12 {
            term::println("^6Ну непойдёшь же как придурок без ничего.");
            return;
        }
        term::println("^2Ты пришел к своей подруге.");
        if self.rng.below(2) == 0 && !self.places.is_found(Location::Club) {
            term::println("^2Она вытащила тебя в клуб и теперь ты знаешь где он находиться.");
            self.places.mark_found(Location::Club);
        }
        term::println("^6Ты купил ей чё-то, потратив 12 рублей.");
        term::println("^2Ты расслабился, отдохнул и снова можешь творить свои гоповские дела.");
        self.player.hp = self.player.hpmax;
        self.player.money -= 12;
    }

    /// The colour digit the original appends to a price row's prefix.
    ///
    /// Every priced menu row is built as `<prefix ending in "^">` +
    /// `'0'`/`'4'` + `<row text>`, so the two halves only form a valid `^N`
    /// code once joined: `'0'` when the row is affordable, `'4'` when it is
    /// not (`1000:b9b3`..`1000:b9c5` for `mar` row 1, and the same shape at
    /// `1000:d410` and `1000:d465` for the vet's two services). The price
    /// digit itself is *not* eaten by the markup -- the colour digit sits
    /// between the `^` and the price.
    fn afford(&self, price: i32) -> &'static str {
        if self.player.money >= price {
            "0"
        } else {
            "4"
        }
    }

    /// Everything a location writes before its own prompt.
    ///
    /// `mar` (`1000:b968`..`1000:bd08`) and `bmar` (`1000:c4d2`..) print
    /// three flavour lines then their priced rows; each row is
    /// `^6N^7 - ^` (files `0xA4A2`, `0xA4C4`, `0xA4E1`, `0xA51B`, `0xA565`,
    /// `0xA599`, `0xA5E9`, `0xA633`, `0xA660` -- `bmar` reuses the same nine
    /// prefixes, confirmed at `1000:c53a` pushing `mar` row 1's prefix
    /// `cs:0x8bd2`) + the affordability digit + the row's own text, which is
    /// `crate::data::shops`' `text` with `#` filled from `displayed_price`.
    /// District gating of a *printed* row is the same `district > N` test the
    /// row's `gate` records (`1000:bb80`, `1000:bc42`, `1000:bca5`).
    ///
    /// **Not reproduced:** `kl`'s and `trn`'s numbered rows. Their prefixes
    /// (` 1 -  ^` file `0xBA48`, ` 2 -  ^` file `0xBA7D`, ` 3 -  ^` `0xBCAE`,
    /// ` 4 -  ^` `0xBCD5`, ` 5 -  ^` `0xBD1B`) and row texts exist, but
    /// `data/shops.json` covers only `mar`/`bmar`, so their prices and
    /// affordability thresholds were not traced by this task. Printing them
    /// would mean guessing which prefix pairs with which text. Their intro
    /// lines are printed; the rows are a reported gap.
    fn print_shop_intro(&mut self, loc: Location) {
        match loc {
            Location::Market => {
                term::println("Ты пришел на базар напиши  ^6w^7  чтобы уйти.");
                term::println("Можно потискать здесь у лохов кошельки(^6t^7).");
                term::println("А можно чё-то купить");
                self.print_priced_rows("mar");
            }
            Location::BigMarket => {
                term::println("Ты пришел к барыгам напиши  ^6w^7  чтобы уйти.");
                term::println("Здесь можно толкнуть хлам(^6x^7) и купить кое-что");
                term::println("Ещё ты можешь продать ненужные вещи - ^6wes^7");
                self.print_priced_rows("bmar");
            }
            Location::Vet => {
                term::println("Ты пришел на ремот, к ветеринару напиши  ^6w^7  чтобы уйти");
                // 1000:d3d3: healthy (hp >= hpmax, no broken jaw, no broken
                // leg) skips the whole menu.
                if self.player.hp >= self.player.hpmax
                    && !self.player.broken_jaw
                    && !self.player.broken_leg
                {
                    return;
                }
                term::println("^0Док: не волнуйся всё зарастёт как на собаке");
                // 1000:d423 / 1000:d478: prefix + affordability digit + text.
                term::println(&format!(
                    "  ^2h^7 - за ^{}3^7 рубля тебя залатают",
                    self.afford(3)
                ));
                term::println(&format!(
                    "  ^2r^7 - за ^{}7^7 рублей починят переломы",
                    self.afford(7)
                ));
            }
            Location::Den => self.print_den_intro(),
            Location::Club => {
                term::println("Ты пришел в клуб напиши  ^6w^7  чтобы уйти");
                term::println(" Здесь можно сыграть в карты (^6p^7 Минимальная ставка- 5р.)");
            }
            Location::Gym => {
                term::println("Ты пришел в качалку напиши  ^6w^7  чтобы уйти");
            }
            Location::Girl | Location::Street | Location::Temple | Location::Dorm => {}
        }
    }

    /// The nine `^6N^7 - ^` prefixes, file `0xA4A2` upward.
    const ROW_PREFIXES: [&'static str; 9] = [
        "^61^7 - ^",
        "^62^7 - ^",
        "^63^7 - ^",
        "^64^7 - ^",
        "^65^7 - ^",
        "^66^7 - ^",
        "^67^7 - ^",
        "^68^7 - ^",
        "^69^7 - ^",
    ];

    fn print_priced_rows(&self, tag: &str) {
        for row in data::shops().iter().filter(|r| r.shop == tag) {
            if !self.gate_open(row.gate) {
                continue;
            }
            let Some(idx) = row.key.parse::<usize>().ok().filter(|n| (1..=9).contains(n)) else {
                continue;
            };
            term::println(&format!(
                "{}{}{}",
                Self::ROW_PREFIXES[idx - 1],
                self.afford(row.price),
                text::fill(row.text, &[row.displayed_price as i64])
            ));
        }
    }

    /// `pr`, `1000:d816`..`1000:d8b9`. `Ты пришел в притон - ` (file
    /// `0xB5C0`) is *written without a newline* (`call 0eed:0000` at
    /// `1000:d82a`), then exactly one district-keyed suffix completes the
    /// line. District 1 spends a real `Random(6)` draw for the dorm number
    /// (`1000:d83b`, `+3`), so this branch is part of the RNG sequence.
    ///
    /// **Not reproduced:** the conditional lines that follow
    /// (`1000:d8c8` onward, gated on flags `20ae:3b78`/`20ae:3b79` this port
    /// does not model) and the den's own `p`/`r`/`hp`/`s`/`a`/`d` menu.
    fn print_den_intro(&mut self) {
        term::print("Ты пришел в притон - ");
        match self.district {
            1 => {
                let n = self.rng.below(6) + 3;
                term::println(&text::fill("^0общагу №#", &[n as i64]));
            }
            2 => term::println("^0общагу ВКИ"),
            3 => term::println("^0гоповский притон"),
            4 => term::println("^0притон отморозков"),
            _ => term::println(""),
        }
    }

    /// One turn at a location's own prompt. The location's keys are checked
    /// *before* the street verb table, because the original reads them with
    /// its own `ReadLn DS:3a72` that never reaches `entry`'s `DS:3972`
    /// dispatch chain at all -- this is why the vet's `h` (heal a jaw) and
    /// the street's `h` (drink a beer, `1000:e966`) can share a letter.
    /// `w`/`run` leaves, which every location's intro text names as the way
    /// out. Anything else is ignored and the prompt repeats.
    fn shop_turn(&mut self, loc: Location, line: &str) {
        let key = line.trim().to_lowercase();
        match (loc, key.as_str()) {
            (Location::Vet, "h") => return self.heal_jaw(),
            (Location::Vet, "r") => return self.heal_leg(),
            (Location::BigMarket, "x") => return self.sell_junk(),
            (Location::BigMarket, "wes") => return self.sell_items(),
            (Location::Market | Location::BigMarket, k)
                if k.len() == 1 && k.chars().all(|c| c.is_ascii_digit()) =>
            {
                return self.shop_action(k.chars().next().unwrap());
            }
            _ => {}
        }
        if matches!(parse(line), Command::Walk) {
            self.location = Location::Street;
            self.mode = Mode::Street;
        }
        // Everything else: ignored, prompt repeats.
    }

    /// `Здоровье #/#  ` -- file `0x2BAE`, trailing double space included.
    fn show_health(&self) {
        term::println(&text::fill(
            "Здоровье #/#  ",
            &[self.player.hp as i64, self.player.hpmax as i64],
        ));
    }

    /// `s`. `Сл:# Лв:# Жв:# Уд:#` file `0x2B6A`, `Урон #-#` file `0x2B7E`,
    /// `Здоровье #/#  ` file `0x2BAE`, `^2Броня #    ` file `0x2C0A` -- the
    /// last one's four trailing spaces and its `^2` are part of the string.
    fn show_stats(&self) {
        let p = &self.player;
        term::println(&text::fill(
            "Сл:# Лв:# Жв:# Уд:#",
            &[
                p.strength as i64,
                p.agility as i64,
                p.vitality as i64,
                p.luck as i64,
            ],
        ));
        term::println(&text::fill("Урон #-#", &[p.dmg_min as i64, p.dmg_max as i64]));
        if p.armor > 0 {
            term::println(&text::fill("^2Броня #    ", &[p.armor as i64]));
        }
        self.show_health();
        for line in self.player.inventory_lines() {
            term::println(&line);
        }
    }

    /// `i`. Confirmed dispatched at `1000:ea94`. Text is the 13-line list
    /// the live capture printed verbatim -- see `crate::commands`' module
    /// doc for the confirmed-vs-corroborated status of each line's own verb.
    fn show_command_list(&self) {
        for line in [
            "Напиши: ^6w^7    чтобы шататься по окрестностям - искать на свою жопу приключения",
            "Напиши: ^6mar^7  чтобы идти на рынок",
            "Напиши: ^6rep^7  чтобы идти к ветеринару",
            "Напиши: ^6pr^7   чтобы идти в местный притон гопоты",
            "Напиши: ^6s^7    чтобы посмотреть в лужу на свою уродскую рожу",
            "Напиши: ^6sv^7   чтобы приглядеться к пинаемому мудаку",
            "Напиши: ^6k^7    чтобы гасить мудака который тебе попался на дороге",
            "Напиши: ^6v^7    чтобы позвать подкрепление",
            "Напиши: ^6kos^7  чтобы схавать косяк",
            "Напиши: ^6h^7    чтобы выпить пиво (если не охото к ветеринару)",
            "Напиши: ^6mh^7   чтобы набухаться до чёртиков",
            "Напиши: ^6name^7 чтобы сменить погоняло",
            "Напиши: ^6e^7    если захочешь выйти",
        ] {
            term::println(line);
        }
    }

    /// `help`. Dispatched confirmed at `1000:edd5`; its printed content was
    /// not traced. Nothing is printed rather than inventing a line: the game
    /// has no "not implemented" string, so there is nothing verbatim to say.
    /// Reported as a gap in `docs/re/gaps.md`.
    fn show_help(&self) {}

    /// `sv`. Shows the last-fought opponent's stat block. The header is the
    /// two real fragments `^2Это ` (file `0x2B59`) and ` # уровня` (file
    /// `0x2B60`), concatenated around the enemy's rank name exactly as
    /// `1000:13d2`..`1000:1404` does.
    ///
    /// **Not reproduced:** the original appends the крутизна descriptor
    /// built by `FUN_1000_1348` from a 256-byte-stride table at `DS:0b42`
    /// indexed by level, and separated by ` - ` (file `0x2B55`).
    ///
    /// Before any fight the original still has a zeroed enemy record and
    /// prints the block anyway; there is no "nothing to inspect" string in
    /// the binary, so this prints nothing rather than composing one.
    fn inspect_enemy(&self) {
        let Some(enemy) = &self.last_enemy else {
            return;
        };
        self.print_enemy_block(enemy);
    }

    fn print_enemy_block(&self, enemy: &Fighter) {
        term::print("^2Это ");
        term::print(&enemy.name);
        term::println(&text::fill(" # уровня", &[enemy.level as i64]));
        term::println(&text::fill(
            "Сл:# Лв:# Жв:# Уд:#",
            &[
                enemy.strength as i64,
                enemy.agility as i64,
                enemy.vitality as i64,
                enemy.luck as i64,
            ],
        ));
        term::println(&text::fill("Урон #-#", &[enemy.dmg_min as i64, enemy.dmg_max as i64]));
        term::println(&text::fill(
            "Здоровье #/#  ",
            &[enemy.hp as i64, enemy.hpmax as i64],
        ));
        term::println(&text::fill("^2Броня #    ", &[enemy.armor as i64]));
    }

    /// `v`. Corroboration-only verb (see `crate::commands`); the original's
    /// gating condition (befriended the den's gopota) is not tracked here,
    /// so this always prints the real refusal line
    /// (`^4Ни кто не хочет за тебя впрягаться.`, file `0x4EB9`).
    fn call_backup(&self) {
        term::println("^4Ни кто не хочет за тебя впрягаться.");
    }

    /// `f`. Corroborated as "shoot"; gating (owns a pistol, bandit district)
    /// is not tracked by `crate::model::Fighter`, so this always prints the
    /// refusal line found immediately after `f`'s own compare in the code
    /// layout (`^6Ты чё псих? мигом менты накроют!`, file `0xC31E`).
    fn shoot(&self) {
        term::println("^6Ты чё псих? мигом менты накроют!");
    }

    /// `w`/`run`. Reconstructed from `1000:ae5a`..`1000:b82c`:
    ///
    /// * `1000:ae63` -- `ReadLn` into `DS:3972` (this is the whole loop's
    ///   own input read, not specific to walking; every verb goes through
    ///   it, matching this port's `run()`).
    /// * `1000:ae86`/`1000:ae97` -- compare against `"w"`/`"run"`; both jump
    ///   to `1000:aea1` (**confirmed synonyms**).
    /// * `1000:aea1`..`1000:af04` -- decays a "stoned" counter (`DS:38cd`)
    ///   and, when it just hit zero, applies a strength penalty with the
    ///   message `^4Глюки прошли. Сила -2.` (`0x9D64`) -- not modelled here,
    ///   `crate::model::Fighter` has a `stoned: bool` flag, not a countdown.
    /// * `1000:af04` onward -- a long run of one-shot flavour/discovery
    ///   events (phone calls, finding the market's own sign, the silencer's
    ///   25-wander counter `docs/re/tables.md` already documents at
    ///   `20ae:3e32`), each gated by its own `Random()` roll and a
    ///   never-repeat flag. **Not reproduced** -- there are too many to
    ///   catalogue in this task's remaining budget; see
    ///   `docs/re/gaps.md`, "Wander preamble".
    /// * **The four-way bucket roll -- established from flow.** Earlier
    ///   revisions of this doc called the roll an assumption ("the specific
    ///   `Random` call feeding the regular-turn branch was not found"). It
    ///   has now been found, and it is the same one: `1000:b34d`
    ///   `mov ax,5` / `1000:b350` `f7 e8` `imul ax` (`DX:AX := AX*AX`, i.e.
    ///   **25**) / `1000:b352` `push ax` / `1000:b353` `call Random` /
    ///   `1000:b358` `inc ax` / `1000:b359` `mov [0x3971],al`. The result is
    ///   then bucketed into `DS:3970` by four independent compares:
    ///   `1000:b35c` (`>= 0x0a` gives bucket 4), `1000:b368` with
    ///   `1000:b36f` (`5..=9` gives 3), `1000:b37b` with `1000:b382`
    ///   (`2..=4` gives 2), `1000:b38e` (`== 1` gives 1). It is dispatched
    ///   at `1000:b3ba` (`mov al,[0x3970]`) through `cmp al,1`
    ///   (`1000:b3bd`), `cmp al,2` (`1000:b4e8`), `cmp al,3` (`1000:b5ae`)
    ///   and `cmp al,4` (`1000:b82f`). `1000:b34d` is reached
    ///   unconditionally on the walk path: every arm of the preceding
    ///   `[0x389c]` switch converges on it (`1000:b2e1` and `1000:b2e8`
    ///   `jmp 0xb34d`, `1000:b2ed` `jnz 0xb34d`, and the `== 6` arm falls
    ///   through it at `1000:b34b`).
    /// * **Bucket 1** (`1000:b3c4`) toggles `[0x3693]` and writes one
    ///   district-keyed flavour line from either of two sets
    ///   (`1000:b3db`.. and `1000:b465`..). Sets no discovery flag; not
    ///   modelled -- see `docs/re/gaps.md`.
    /// * **Bucket 2** (`1000:b4ef`) is the girl encounter, implemented by
    ///   [`Game::wander_girl`].
    /// * **Bucket 3** (`1000:b5b5`) is the fight encounter, below.
    /// * **Bucket 4** (`1000:b836`) is the stoned-counter (`[0x38cd]`)
    ///   flavour path; sets no discovery flag, not modelled.
    /// * `1000:b5b8` -- `call FUN_1000_0d14` (rolls the enemy).
    /// * `1000:b6a6`..`1000:b6dd` -- writes `^6Идет ` (file `0xA267`), the
    ///   rank name, and ` # уровня, ищущий кого отпинать. Хочешь наехать?`
    ///   (file `0xA28A`) as one `WriteLn`. **No prompt is written after it**
    ///   -- the very next instruction (`1000:b6e0`) sets up the `ReadLn`.
    /// * `1000:b6e0`..`1000:b704` -- a **second** `ReadLn`, this time into
    ///   `DS:3a72` (confirmed a different variable from the line-level
    ///   `DS:3972`), then `call 0eed:0216` -- the same case-folding routine
    ///   `entry` applies to every typed line, so the answer **is**
    ///   case-insensitive -- then compared against the literal `"y"`
    ///   (file `0x9BF3`: length-prefixed `01 79`).
    /// * `1000:b718` -- `jnz 0xb721`, i.e. **the answer was not `y`**:
    ///   `Random(2)` at `1000:b725`, then `or ax,ax` / `jnz 0xb74e`.
    ///   * `ax == 0` falls through to `1000:b72e`: writes `^4Он тебя
    ///     заметил.` (file `0xA2BB`) and then `mov byte [0x3b72],1` at
    ///     `1000:b747` -- the accept flag. **Roll 0 means the fight
    ///     happens.**
    ///   * `ax != 0` jumps to `1000:b74e`: writes `^2Ты смылся.` (file
    ///     `0xA2CE`) and leaves the flag clear. **Non-zero means escaped.**
    ///
    ///   Nothing else is written on either arm; `^4Эй мудак?!` (file
    ///   `0x457A`) belongs to `FUN_1000_3d11`'s class-7 combat opener
    ///   (`1000:3dc7`), not here.
    /// * `1000:b81f`/`1000:b826` -- if the accept flag is set, `call
    ///   FUN_1000_3d11` (combat) with `param_1 = 0`.
    ///
    /// There is a second, structurally similar answer block at `1000:b691`
    /// that has **no** random roll on decline (a non-`y` answer simply ends
    /// the encounter); which of the two a real encounter reaches depends on
    /// the rolled enemy's class via `1000:b5fc`, which this task did not
    /// trace. This port always takes the `Random(2)` branch.
    fn walk(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        // 1000:b34d..1000:b359: Random(5*5) + 1, bucketed at 1000:b35c..
        // 1000:b393 and dispatched at 1000:b3ba.
        let roll = self.rng.below(25) + 1;
        let bucket = if roll == 1 {
            1
        } else if roll <= 4 {
            2
        } else if roll <= 9 {
            3
        } else {
            4
        };
        match bucket {
            2 => return self.wander_girl(lines),
            3 => {}
            // Buckets 1 (1000:b3c4) and 4 (1000:b836) are flavour-only and
            // are not modelled; see this function's doc and docs/re/gaps.md.
            _ => return Ok(()),
        }

        let enemy = self.pick_enemy();
        term::print("^6Идет ");
        term::print(&enemy.name);
        term::println(&text::fill(
            " # уровня, ищущий кого отпинать. Хочешь наехать?",
            &[enemy.level as i64],
        ));
        let Some(line) = lines.next() else {
            self.running = false;
            return Ok(());
        };
        let answer = line?;
        if answer.trim().eq_ignore_ascii_case("y") {
            self.run_combat(enemy, lines)?;
        } else if self.rng.below(2) == 0 {
            term::println("^4Он тебя заметил.");
            self.run_combat(enemy, lines)?;
        } else {
            term::println("^2Ты смылся.");
        }
        Ok(())
    }

    /// Wander bucket 2 -- **the girl discovery event**, and the only
    /// discovery path this port implements. Established from flow, every
    /// instruction of `1000:b4e8`..`1000:b5ab` re-derived from `orig/g.exe`:
    ///
    /// * `1000:b4e8` `3c 02` -- `cmp al,2`, the bucket test; `1000:b4ea`
    ///   `jz 0xb4ef` selects this branch.
    /// * `1000:b4ef` `80 3e 97 36 00` -- `cmp byte [0x3697],0`, the
    ///   **girl's** discovery flag (the gate `girl` itself reads at
    ///   `1000:d6f7`; the den is `0x3696`, gate `1000:d80c`). Non-zero --
    ///   already found -- jumps to `1000:b592`, which writes
    ///   `Совсем ничё не происходит.` (file `0xA24C`) and ends the turn.
    /// * `1000:b4f9` -- writes `^5Идет типа клёвая цыпа. Хочешь её
    ///   зацепить?` (file `0xA19E`).
    /// * `1000:b512`..`1000:b52a` -- `ReadLn` into `DS:3a72`, the same
    ///   second input variable the fight encounter and the locations use,
    ///   **not** the line-level `DS:3972`.
    /// * `1000:b534` -- `call 0eed:0216`, the case-fold `entry` applies to
    ///   every typed line, so the answer is case-insensitive.
    /// * `1000:b543`/`1000:b548` -- compared against the literal `"y"`
    ///   (file `0x9BF3`, `01 79`); `75 46` (`jnz 0xb590`) means **any other
    ///   answer writes nothing at all** and ends the turn -- there is no
    ///   decline message on this branch, unlike the fight encounter's.
    /// * `1000:b54a`..`1000:b553` -- `Random(2)`, then `09 c0` (`or ax,ax`)
    ///   and `75 20` (`jnz 0xb577`).
    ///   * `ax == 0` falls through to `1000:b557`: writes `^5Ты такой
    ///     подкатываешь, а она:"Глянулся ты мне парниша"` (file `0xA1CB`)
    ///     and then `1000:b570` `c6 06 97 36 01` -- `mov byte [0x3697],1`,
    ///     the flag being **set**. (`1000:b575` is the `eb 19` `jmp` after
    ///     it, not the setter.)
    ///   * `ax != 0` jumps to `1000:b577`: writes `^4Ты ещё подкатить
    ///     неуспел - а она:"Отдыхай урод". - Тебя обломали кент` (file
    ///     `0xA204`) and leaves the flag clear.
    ///
    /// Exactly one `Random` draw on the `"y"` path and none on any other,
    /// matching the single `call` at `1000:b54e`.
    fn wander_girl(
        &mut self,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        if self.places.is_found(Location::Girl) {
            term::println("Совсем ничё не происходит.");
            return Ok(());
        }
        term::println("^5Идет типа клёвая цыпа. Хочешь её зацепить?");
        let Some(line) = lines.next() else {
            self.running = false;
            return Ok(());
        };
        let answer = line?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            return Ok(());
        }
        if self.rng.below(2) == 0 {
            term::println("^5Ты такой подкатываешь, а она:\"Глянулся ты мне парниша\"");
            self.places.mark_found(Location::Girl);
        } else {
            term::println(
                "^4Ты ещё подкатить неуспел - а она:\"Отдыхай урод\". - Тебя обломали кент",
            );
        }
        Ok(())
    }

    /// Picks and rolls a random-encounter opponent, reproducing
    /// `FUN_1000_0d14` (`1000:0d14`, `docs/re/tables.md` section 3) as far
    /// as this task traced it: distribute `sum(weights)` points across the
    /// four stats by repeated `Random(sum)` in proportion to the class's
    /// weight row (the same range-pick `crate::progress`'s level-up `pick`
    /// uses, reimplemented here since that helper is private); derive
    /// `dmg_min = str/2`, `dmg_max = str`, `hpmax = vit*5 + str + 10`.
    ///
    /// **Simplifications, not guesses:** the class pick is uniform 0..=9
    /// rather than the original's `Random(0x33)`-plus-district formula
    /// (not fully recovered); крутизна's point bonus and the class-8
    /// (`Мент`/cop) special-case branch at `1000:b5c0` are not modelled.
    fn pick_enemy(&mut self) -> Fighter {
        let class = self.rng.below(10);
        let weights = progress::class_weights(class);
        let sum: u16 = weights.iter().sum();
        let mut stats = [0u16; 4]; // strength, agility, vitality, luck
        for _ in 0..sum {
            let roll = self.rng.below(sum) + 1;
            let mut edge = 0u16;
            for (i, w) in weights.iter().enumerate() {
                edge += w;
                if roll <= edge {
                    stats[i] += 1;
                    break;
                }
            }
        }
        let [strength, agility, vitality, luck] = stats;
        let hpmax = 10 + 5 * vitality + strength;
        // `data/enemies.json` has one row per rolled class 0..=9 (classes
        // 0..9 are unique there; only the scripted class 10 has variants),
        // so this lookup is total for every value `below(10)` can return.
        // A nameless fighter must never reach the player, so a missing row
        // is a build-data bug, not something to paper over with "".
        let name = data::enemies()
            .iter()
            .find(|e| e.class == class)
            .map(|e| e.name.to_string())
            .unwrap_or_else(|| {
                panic!("data/enemies.json has no row for rolled class {class}")
            });
        Fighter {
            name,
            class,
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
        }
    }

    // There is deliberately no `save_game()` wrapper here. The original has
    // no "saved OK" / "save failed" string anywhere, so a wrapper could only
    // print composed text -- see [`Game::write_save`] and `docs/re/gaps.md`.

    /// Writes a checkpoint save. See the module doc: `Unsupported` for
    /// every `Game` this task can construct.
    #[allow(dead_code)]
    fn write_save(&self) -> io::Result<String> {
        let Some(template) = &self.save_template_bytes else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "cannot save a freshly created character: .SAV offsets 0x214 \
                 (29 bytes) and 0x2ae (8 bytes) are unknown, and Save::parse \
                 is the only constructor Task 9 provides -- reaching a \
                 checkpoint in the original and capturing its output is the \
                 sanctioned way to learn them",
            ));
        };
        let mut save = Save::parse(template)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        save.name = self.player.name.clone();
        save.stats = [
            self.player.class,
            self.player.strength,
            self.player.agility,
            self.player.vitality,
            self.player.luck,
            self.player.level,
            self.player.dmg_min,
            self.player.dmg_max,
        ];
        save.hp = self.player.hp;
        save.hpmax = self.player.hpmax;
        let bytes = save
            .to_bytes()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let filename = format!("SAVE_R{}.SAV", self.district);
        std::fs::write(&filename, bytes)?;
        Ok(filename)
    }

    /// `name`. `^2Звали тебя:^7 ` / `^2А теперь будут:^7 ` are this port's
    /// own prompts and are the one place the module knowingly departs from
    /// the byte-verbatim rule; `1000:ecf1`'s handler body was not traced.
    /// Flagged in `docs/re/gaps.md`.
    fn rename(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        term::print("^2Звали тебя:^7 ");
        term::println(&self.player.name);
        term::print("^2А теперь будут:^7 ");
        let Some(line) = lines.next() else {
            self.running = false;
            return Ok(());
        };
        let n = line?;
        let n = n.trim();
        if !n.is_empty() {
            self.player.name = n.to_string();
        }
        Ok(())
    }

    /// `kos`, the joint. Traced at `1000:e97d`..`1000:ea6f` (the copy
    /// `entry` dispatches; `FUN_1000_3d11` has its own near-identical copy
    /// with slightly different strings at file `0x4D85`..`0x4E49`):
    ///
    /// * broken jaw (`DS:38b0`) -> `^4Ты не схавать колёса из-за сломаной
    ///   челюсти.` (file `0xBEF3`).
    /// * already stoned (`DS:38cd != 0`) -> `^6Ты неможешь схавать ещё один
    ///   косяк.` (file `0xBFB8`).
    /// * no joints (`DS:38c5 <= 0`) -> `^4У тебя нет косяков` (file
    ///   `0xBFA3`).
    /// * otherwise **exactly one** joint (`1000:e9b4`): stoned counter := 10,
    ///   strength += 2, `dmg_min` += 1, `dmg_max` += 2, and heal a flat
    ///   **+10** capped at `hpmax`, then `^2Сила +2.` (file `0xBF98`).
    ///   The heal message splits like the beer routine's: when the shortfall
    ///   is under 10 it writes `^2Колёса прибавляют #з. ` (file `0xBF22`,
    ///   no newline) then `^2Здоровья:#/#. Осталось # косяков` (file
    ///   `0xBF3B`); otherwise the single combined line at file `0xBF5E`,
    ///   whose `косякова` typo is the original's.
    ///
    /// `crate::model::Fighter` has a `stoned: bool`, not the original's
    /// 10-turn countdown, so the counter is modelled as "stoned or not".
    fn smoke(&mut self) {
        if self.player.broken_jaw {
            term::println("^4Ты не схавать колёса из-за сломаной челюсти.");
            return;
        }
        if self.player.stoned {
            term::println("^6Ты неможешь схавать ещё один косяк.");
            return;
        }
        if self.player.joints == 0 {
            term::println("^4У тебя нет косяков");
            return;
        }
        self.player.joints -= 1;
        self.player.stoned = true;
        self.player.strength += 2;
        self.player.dmg_min += 1;
        self.player.dmg_max += 2;
        let shortfall = self.player.hpmax.saturating_sub(self.player.hp);
        if shortfall < 10 {
            term::print(&text::fill(
                "^2Колёса прибавляют #з. ",
                &[shortfall as i64],
            ));
            self.player.hp = self.player.hpmax;
            term::println(&text::fill(
                "^2Здоровья:#/#. Осталось # косяков",
                &[
                    self.player.hp as i64,
                    self.player.hpmax as i64,
                    self.player.joints as i64,
                ],
            ));
        } else {
            self.player.hp += 10;
            term::println(&text::fill(
                "^2Колёса прибавляют #з. Здоровья:#/#. Осталось # косякова",
                &[
                    10,
                    self.player.hp as i64,
                    self.player.hpmax as i64,
                    self.player.joints as i64,
                ],
            ));
        }
        term::println("^2Сила +2.");
    }

    /// `h` (one 0.5-litre unit) or `mh` (drink until full or dry).
    ///
    /// Both verbs are dispatched by `FUN_1000_29c4` itself, not by an inline
    /// compare in `entry`: `entry` pushes the just-read line `DS:3972` and
    /// calls it at `1000:e966` (`E8 5B 40`, wrapping to `1000:29c4`), and
    /// the routine compares its own argument against `"h"` (token file
    /// `0x4197`) at `1000:29f0` and `"mh"` (token file `0x4199`) at
    /// `1000:2a02`, returning immediately when it is neither. Six later
    /// `"h"` compares (`1000:2a6a`, `2aa0`, `2af2`, `2b40`, `2b89`) and one
    /// more `"mh"` compare (`1000:2bb0`) select which messages are written
    /// and whether the drink loop repeats. `FUN_1000_3d11` calls the same
    /// routine at `1000:4b00` with its own `DS:3a72`, which is why beer works
    /// inside a fight too.
    ///
    /// Traced body, with `DS:38ac` = hp, `DS:38ae` = hpmax, `DS:38b0` =
    /// broken jaw, `DS:38c3` = beer in half-litres:
    ///
    /// * `1000:2a18` broken jaw -> file `0x419C`, nothing else.
    /// * `1000:2a3b` already at full hp -> file `0x424C`, immediate return.
    /// * `1000:2a47` no beer -> `h` writes file `0x4240`.
    /// * `1000:2a51` otherwise spend one half-litre. `1000:2a55`: when the
    ///   shortfall is under 5, `h` writes file `0x41CD` (no newline) then
    ///   file `0x41E4` and hp goes to `hpmax`; otherwise hp += 5 and `h`
    ///   writes the combined file `0x4208`.
    /// * `1000:2b83` `h` stops after that one unit; `mh` loops back to
    ///   `1000:2a3b` while hp < hpmax and beer remains, writing nothing.
    /// * `1000:2bbf`..`1000:2c53` `mh`'s tail: the file `0x4208` summary with
    ///   the total healed, then file `0x4283` if that drank the last of it,
    ///   or file `0x4240` if nothing was drunk at all.
    ///
    /// The `#.#л.` pair is `beer/2` and `(beer mod 2) * 5` (`1000:2ab9`).
    fn beer(&mut self, how: Beer) {
        let single = how == Beer::One;
        let hp0 = self.player.hp;
        if self.player.broken_jaw {
            term::println("^4Ты не можешь пить пиво из-за сломаной челюсти.");
            return;
        }
        loop {
            if self.player.hp >= self.player.hpmax {
                term::println("^6Блин только тупить не надо - и так здоровья до фига.");
                return;
            }
            if self.player.beer_dl == 0 {
                if single {
                    term::println("^4Пива нету");
                }
                break;
            }
            self.player.beer_dl -= 1;
            let shortfall = self.player.hpmax - self.player.hp;
            if shortfall < 5 {
                if single {
                    term::print(&text::fill("^2Пиво прибавляет #з. ", &[shortfall as i64]));
                }
                self.player.hp = self.player.hpmax;
                if single {
                    term::println(&text::fill(
                        "^2Здоровья:#/#. Осталось #.#л. пива",
                        &self.beer_numbers(),
                    ));
                }
            } else {
                self.player.hp += 5;
                if single {
                    let n = self.beer_numbers();
                    term::println(&text::fill(
                        "^2Пиво прибавляет #з. Здоровья:#/#. Осталось #.#л. пива",
                        &[5, n[0], n[1], n[2], n[3]],
                    ));
                }
            }
            if single || self.player.hp >= self.player.hpmax || self.player.beer_dl == 0 {
                break;
            }
        }
        if single {
            return;
        }
        let healed = i64::from(self.player.hp) - i64::from(hp0);
        if healed != 0 {
            let n = self.beer_numbers();
            term::println(&text::fill(
                "^2Пиво прибавляет #з. Здоровья:#/#. Осталось #.#л. пива",
                &[healed, n[0], n[1], n[2], n[3]],
            ));
            if self.player.beer_dl == 0 {
                term::println("^4Кончилось пиво");
            }
        } else if self.player.beer_dl == 0 {
            term::println("^4Пива нету");
        }
    }

    /// `hp`, `hpmax`, litres, tenths -- the four trailing `#`s of the beer
    /// messages (`1000:2ab1`..`1000:2ae0`).
    fn beer_numbers(&self) -> [i64; 4] {
        [
            self.player.hp as i64,
            self.player.hpmax as i64,
            (self.player.beer_dl / 2) as i64,
            i64::from(self.player.beer_dl % 2) * 5,
        ]
    }

    /// `x` at the dealers: sell junk. `crate::model::Fighter` has no field
    /// for carried junk items, so the "nothing to sell" branch is always
    /// true (`^4Тебе нечего спихнуть.`, file `0xAFC2`).
    fn sell_junk(&self) {
        term::println("^4Тебе нечего спихнуть.");
    }

    /// `wes` at the dealers: sell unneeded items. Same gap as
    /// [`Game::sell_junk`] (`^6У тебя нет неужных вещей.`, file `0xB1AE`).
    fn sell_items(&self) {
        term::println("^6У тебя нет неужных вещей.");
    }

    /// `h` at the vet: 3 rubles to fix a broken jaw.
    ///
    /// The price is the literal `3` of the menu line the vet prints (file
    /// `0xB2B2`), and the same literal is what the display's affordability
    /// test compares money against (`cmp word [0x38c7],0x3` at `1000:d410`).
    /// The debiting code inside the vet's own submenu was not traced, so
    /// "3 rubles is also what is charged" remains an inference; see
    /// `docs/re/gaps.md`.
    fn heal_jaw(&mut self) {
        self.pay_and_heal(3, self.player.broken_jaw, |f| f.broken_jaw = false);
    }

    /// `r` at the vet: 7 rubles to fix a broken leg (file `0xB2D9`,
    /// `cmp word [0x38c7],0x7` at `1000:d465`). Same caveat as
    /// [`Game::heal_jaw`].
    fn heal_leg(&mut self) {
        self.pay_and_heal(7, self.player.broken_leg, |f| f.broken_leg = false);
    }

    fn pay_and_heal(&mut self, price: i32, already_broken: bool, clear: impl FnOnce(&mut Fighter)) {
        if !already_broken {
            term::println("^0Док: вали отсюда ты здоров.");
            return;
        }
        if self.player.money < price {
            term::println("^4Блин халявщик, медицина не бесплатная");
            return;
        }
        self.player.money -= price;
        clear(&mut self.player);
        term::println("^2Твои переломы залечены.");
    }

    /// Whether a row's `district>N` gate is satisfied.
    fn gate_open(&self, gate: Option<&'static str>) -> bool {
        match gate.and_then(|g| g.strip_prefix("district>")) {
            Some(n) => match n.parse::<u8>() {
                Ok(need) => self.district > need,
                Err(_) => true,
            },
            None => true,
        }
    }

    /// Buy row `key` (a shop-row digit `'1'..'9'`) at the current market.
    /// Only `Market`/`BigMarket` have a row table (`data/shops.json` covers
    /// just `mar`/`bmar`).
    ///
    /// **Known gap:** buying only deducts `price` and prints the row text;
    /// it never applies the row's effect to `self.player` (no per-item
    /// ownership fields on `crate::model::Fighter`). Two rows also have a
    /// second `#` placeholder this does not fill (`mar` row 2's literal `5`,
    /// `bmar` row 7's pistol damage range).
    fn shop_action(&mut self, k: char) {
        let tag = match self.location {
            Location::Market => "mar",
            Location::BigMarket => "bmar",
            _ => return,
        };
        let key = k.to_string();
        let Some(row) = data::shops().iter().find(|r| r.shop == tag && r.key == key) else {
            return;
        };
        if !self.gate_open(row.gate) {
            return;
        }
        if self.player.money < row.price {
            // file 0xAC55 at the dealers (1000:c4d2's block), file 0xA6CA at
            // the market. Both are the same situation, different wording.
            term::println(match tag {
                "bmar" => "^4Чёрт, бабок не хватает.",
                _ => "^4Чёрт, бабок даже на жратву не хватает.",
            });
            return;
        }
        self.player.money -= row.price;
        term::println(&text::fill(row.text, &[row.displayed_price as i64]));
    }

    /// `^0Битва\` (file `0x4A49`). Confirmed modal by the live capture
    /// (`mar`/`i` typed here were ignored, reprinting the prompt). The exact
    /// in-combat verb set beyond `sv` (inspect) and `h`/`mh` (beer, confirmed
    /// by `FUN_1000_3d11`'s own call into `FUN_1000_29c4` at `1000:4b00`) was
    /// not traced; `k` (attack) is this port's own choice, consistent with
    /// `k` being the fight verb everywhere else, but **not independently
    /// confirmed as the in-combat attack key**. See `docs/re/gaps.md`.
    ///
    /// Death and victory both come from `FUN_1000_3d11`'s own tail:
    ///
    /// * `1000:4f82` `hp <= 0`. With the rector flag set, file `0x509C`;
    ///   otherwise, if the den is known and money >= 10, the hospital rescue
    ///   at `1000:4fce` (file `0x50DF`, `money -= 10`, hp restored) --
    ///   neither modelled here. The plain case is `1000:5053`: file `0x5127`
    ///   (`^4Ты сдох.`) and then `FUN_1000_074b(0)`, the end screen, which
    ///   ends in `Halt` (`1000:0ac0`). So death **ends the game**.
    /// * `1000:5189` the enemy died: file `0x5250` (`^2Враг сдох.`), then
    ///   `1000:51b4` file `0x525D` with `str+agi+vit+luck` as the award.
    ///   `^2Ты победил.` is *not* a per-fight line: it is file `0x1DBF`, the
    ///   centred end-of-game banner `FUN_1000_074b` writes when you beat the
    ///   rector, and printing it here was a fabrication.
    fn run_combat(&mut self, mut enemy: Fighter, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        loop {
            if self.player.hp == 0 || enemy.hp == 0 {
                break;
            }
            term::print("^0Битва\\");
            let Some(line) = lines.next() else {
                self.running = false;
                return Ok(());
            };
            let line = line?;
            match parse(&line) {
                Command::Inspect => self.print_enemy_block(&enemy),
                Command::Fight => self.combat_round(&mut enemy),
                Command::Drink => self.beer(Beer::One),
                Command::BingeDrink => self.beer(Beer::Binge),
                _ => {} // ignored: matches the live capture's mar/i rejection
            }
        }

        self.last_enemy = Some(enemy.clone());
        if self.player.hp == 0 {
            term::println("^4Ты сдох.");
            self.running = false;
            return Ok(());
        }

        term::println("^2Враг сдох.");
        let award = progress::xp_award(self.player.level, &enemy);
        term::println(&text::fill(
            "^6За отпин врага ты получаешь # качков опыта",
            &[award as i64],
        ));
        progress::apply_levels(&mut self.progress, &mut self.player, &mut self.rng, award, false);

        while self.district < 5 && self.player.level >= u16::from(self.district) * 10 {
            self.district += 1;
            self.places.reset_for_new_district();
        }
        Ok(())
    }

    /// One round of blows, both sides, using the already-verified
    /// blows-per-round budget and per-blow resolution from `crate::combat`.
    /// Per-blow messages are `docs/re/combat.md`'s own cited strings, quoted
    /// here with the markup they actually carry: miss (`^4Ты промазал` file
    /// `0x4B13`, `^2Враг промазал` file `0x4C49`), hit (`^2Ты пнул врага на
    /// #з. У него осталось #` file `0x4AEA`, and its mirror `^4Он пнул тебя
    /// на #з. У тебя осталось #` file `0x4C21`), crit (`^2Точный удар!!!`
    /// file `0x4A54`), and break (`^2Ты сломал врагу челюсть. ^4Враг: А!
    /// козёл!` / `^2Ты сломал врагу ногу. ^4Враг: Ну что за урод!` files
    /// `0x4A8D`/`0x4ABA`, whose inner `^4` is part of the string, and the
    /// mirrors `^4Враг сломал тебе челюсть.` / `^4Враг сломал тебе ногу.`
    /// files `0x4B95`/`0x4C08`).
    fn combat_round(&mut self, enemy: &mut Fighter) {
        let player_blows = blows_per_round(&self.player, enemy);
        for i in 0..player_blows {
            if enemy.hp == 0 {
                break;
            }
            let blow = resolve_blow_nth(&mut self.rng, &self.player, enemy, i);
            if !blow.hit {
                term::println("^4Ты промазал");
                continue;
            }
            if blow.critical {
                term::println("^2Точный удар!!!");
            }
            enemy.hp = enemy.hp.saturating_sub(blow.damage);
            term::println(&text::fill(
                "^2Ты пнул врага на #з. У него осталось #",
                &[blow.damage as i64, enemy.hp as i64],
            ));
            match blow.broke {
                Some(Break::Jaw) => {
                    term::println("^2Ты сломал врагу челюсть. ^4Враг: А! козёл!")
                }
                Some(Break::Leg) => {
                    term::println("^2Ты сломал врагу ногу. ^4Враг: Ну что за урод!")
                }
                None => {}
            }
        }
        if enemy.hp == 0 {
            return;
        }
        let enemy_blows = blows_per_round(enemy, &self.player);
        for i in 0..enemy_blows {
            if self.player.hp == 0 {
                break;
            }
            let blow = resolve_blow_nth(&mut self.rng, enemy, &self.player, i);
            if !blow.hit {
                term::println("^2Враг промазал");
                continue;
            }
            self.player.hp = self.player.hp.saturating_sub(blow.damage);
            term::println(&text::fill(
                "^4Он пнул тебя на #з. У тебя осталось #",
                &[blow.damage as i64, self.player.hp as i64],
            ));
            match blow.broke {
                Some(Break::Jaw) => {
                    self.player.broken_jaw = true;
                    term::println("^4Враг сломал тебе челюсть.");
                }
                Some(Break::Leg) => {
                    self.player.broken_leg = true;
                    term::println("^4Враг сломал тебе ногу.");
                }
                None => {}
            }
        }
    }
}

/// Which beer verb was typed. `h` drinks one half-litre and narrates it;
/// `mh` drinks silently until full or dry and prints one summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Beer {
    One,
    Binge,
}

/// Extension impl, deliberately **not** in `crate::model` -- that module is
/// reviewed and frozen for this task. Rust does not require an `impl` block
/// to share a file with its type's definition as long as both are in the
/// same crate.
impl Fighter {
    /// One line per owned item/status the status screen would show
    /// (`data/strings.json`, file `0x2FA9`..`0x32CC`). Only fields `Fighter`
    /// actually carries are reproduced: per-item ownership (dental guard,
    /// mobile phone, sunglasses, tattoo, pistol, silencer, bullet count) has
    /// no backing field, so those status lines are not guessed at.
    pub fn inventory_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(text::fill("Косяки #", &[self.joints as i64]));
        if self.beer_dl > 0 {
            lines.push(text::fill(
                "Пиво #.#л.",
                &[(self.beer_dl / 2) as i64, i64::from(self.beer_dl % 2) * 5],
            ));
        } else {
            lines.push("^4Пива нет".to_string());
        }
        if self.money > 0 {
            lines.push(text::fill("Бабки #", &[self.money as i64]));
        } else {
            lines.push("^4Нету бабок".to_string());
        }
        if self.broken_jaw {
            lines.push("^4Сломана челюсть  ".to_string());
        }
        if self.broken_leg {
            lines.push("^4Сломана нога  ".to_string());
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> Fighter {
        Fighter {
            name: "Тест".to_string(),
            hp: 20,
            hpmax: 20,
            strength: 5,
            agility: 5,
            vitality: 5,
            luck: 5,
            dmg_min: 1,
            dmg_max: 3,
            ..Fighter::default()
        }
    }

    fn game() -> Game {
        Game::new(player(), Progress::new(), 12345)
    }

    /// An input script for the handlers that read more lines.
    fn input(lines: &[&str]) -> std::vec::IntoIter<io::Result<String>> {
        lines
            .iter()
            .map(|s| Ok(s.to_string()))
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn no_input() -> std::vec::IntoIter<io::Result<String>> {
        input(&[])
    }

    #[test]
    fn new_game_starts_on_the_street_with_nothing_discovered() {
        let g = game();
        assert_eq!(g.location, Location::Street);
        assert_eq!(g.mode, Mode::Street);
        assert_eq!(g.district, 1);
        assert!(!g.places.is_found(Location::Market));
        assert!(g.places.is_found(Location::Street));
    }

    #[test]
    fn entering_a_known_place_switches_to_shop_mode() {
        let mut g = game();
        g.places.mark_found(Location::Market);
        g.dispatch(Command::Market, &mut no_input()).unwrap();
        assert_eq!(g.location, Location::Market);
        assert_eq!(g.mode, Mode::Shop(Location::Market));
    }

    /// I6: a refused entry must NOT discover the place. The original sets
    /// the seven flags at `20ae:3694`..`369a` from the wander path and from
    /// `girl` (`1000:d751`), never from a failed entry.
    #[test]
    fn entering_an_undiscovered_place_does_not_discover_it() {
        let mut g = game();
        for _ in 0..3 {
            g.dispatch(Command::Market, &mut no_input()).unwrap();
            assert_eq!(g.location, Location::Street);
            assert_eq!(g.mode, Mode::Street);
            assert!(
                !g.places.is_found(Location::Market),
                "a refused entry must not mark the place found"
            );
        }
    }

    #[test]
    fn every_gated_location_has_its_own_refusal_string() {
        let mut seen = std::collections::HashSet::new();
        for loc in crate::locations::TRACKED {
            let s = Game::undiscovered_line(loc);
            assert!(!s.is_empty(), "{loc:?} has no refusal string");
            assert!(s.starts_with('^'), "{loc:?}'s refusal lost its markup");
            assert!(seen.insert(s), "{loc:?} shares a refusal string");
        }
    }

    #[test]
    fn shop_mode_leaves_on_w_and_ignores_other_verbs() {
        let mut g = game();
        g.places.mark_found(Location::Vet);
        g.location = Location::Vet;
        g.mode = Mode::Shop(Location::Vet);
        g.shop_turn(Location::Vet, "mar"); // must not teleport
        assert_eq!(g.location, Location::Vet);
        g.shop_turn(Location::Vet, "w");
        assert_eq!(g.location, Location::Street);
        assert_eq!(g.mode, Mode::Street);
    }

    /// I5: `h` is the beer verb at the top level (`entry` -> `FUN_1000_29c4`,
    /// `1000:e966`) *and* the vet's jaw key, because the vet reads its own
    /// input at its own prompt. Both must work, through the same public path
    /// the player uses -- not by calling a handler directly.
    #[test]
    fn h_heals_the_jaw_at_the_vet_and_drinks_beer_on_the_street() {
        let mut g = game();
        g.places.mark_found(Location::Vet);
        g.location = Location::Vet;
        g.mode = Mode::Shop(Location::Vet);
        g.player.broken_jaw = true;
        g.player.money = 10;
        g.player.beer_dl = 4;
        g.shop_turn(Location::Vet, "h");
        assert!(!g.player.broken_jaw, "vet's h must heal the jaw");
        assert_eq!(g.player.money, 7);
        assert_eq!(g.player.beer_dl, 4, "vet's h must not drink beer");

        let mut g = game();
        g.player.hp = 10;
        g.player.beer_dl = 4;
        g.dispatch(parse("h"), &mut no_input()).unwrap();
        assert_eq!(g.player.beer_dl, 3, "street h must drink exactly one unit");
        assert_eq!(g.player.hp, 15);
    }

    #[test]
    fn vet_r_still_works_through_the_parser() {
        let mut g = game();
        g.location = Location::Vet;
        g.mode = Mode::Shop(Location::Vet);
        g.player.broken_leg = true;
        g.player.money = 10;
        g.shop_turn(Location::Vet, "r");
        assert!(!g.player.broken_leg);
        assert_eq!(g.player.money, 3);
    }

    #[test]
    fn quit_stops_the_loop() {
        let mut g = game();
        g.dispatch(Command::Quit, &mut no_input()).unwrap();
        assert!(!g.running);
    }

    #[test]
    fn write_save_is_blocked_for_a_fresh_character() {
        let g = game();
        let err = g.write_save().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn h_drinks_one_unit_and_mh_drinks_until_full() {
        let mut g = game();
        g.player.hp = 5;
        g.player.beer_dl = 10;
        g.beer(Beer::One);
        assert_eq!(g.player.hp, 10);
        assert_eq!(g.player.beer_dl, 9);

        let mut g = game();
        g.player.hp = 5;
        g.player.beer_dl = 10;
        g.beer(Beer::Binge);
        assert_eq!(g.player.hp, 20);
        assert_eq!(g.player.beer_dl, 7);
    }

    #[test]
    fn beer_refuses_with_broken_jaw() {
        let mut g = game();
        g.player.hp = 5;
        g.player.beer_dl = 4;
        g.player.broken_jaw = true;
        g.beer(Beer::One);
        assert_eq!(g.player.hp, 5);
        assert_eq!(g.player.beer_dl, 4);
    }

    #[test]
    fn beer_does_nothing_at_full_health_or_with_no_beer() {
        let mut g = game();
        g.player.beer_dl = 4;
        g.beer(Beer::Binge);
        assert_eq!(g.player.beer_dl, 4);

        let mut g = game();
        g.player.hp = 5;
        g.player.beer_dl = 0;
        g.beer(Beer::One);
        assert_eq!(g.player.hp, 5);
    }

    /// `1000:e9b4`: one joint, +10 hp capped at hpmax, Сила +2, урон +1/+2,
    /// and the stoned flag blocks a second one.
    #[test]
    fn kos_smokes_exactly_one_joint_and_buffs_strength() {
        let mut g = game();
        g.player.hp = 1;
        g.player.joints = 2;
        g.smoke();
        assert_eq!(g.player.joints, 1);
        assert_eq!(g.player.hp, 11);
        assert_eq!(g.player.strength, 7);
        assert_eq!(g.player.dmg_min, 2);
        assert_eq!(g.player.dmg_max, 5);
        assert!(g.player.stoned);

        g.smoke();
        assert_eq!(g.player.joints, 1, "already stoned: no second joint");
    }

    #[test]
    fn shop_action_buys_an_affordable_row_and_debits_price() {
        let mut g = game();
        g.location = Location::Market;
        g.player.money = 10;
        g.shop_turn(Location::Market, "1"); // mar row 1, price 2
        assert_eq!(g.player.money, 8);
    }

    #[test]
    fn shop_action_refuses_when_too_poor() {
        let mut g = game();
        g.location = Location::Market;
        g.player.money = 1;
        g.shop_turn(Location::Market, "1");
        assert_eq!(g.player.money, 1);
    }

    #[test]
    fn shop_action_respects_the_district_gate() {
        let mut g = game();
        g.location = Location::Market;
        g.player.money = 1000;
        g.district = 1; // mar row 6 needs district>1
        g.shop_turn(Location::Market, "6");
        assert_eq!(g.player.money, 1000, "gated row must not be sellable yet");
        g.district = 2;
        g.shop_turn(Location::Market, "6");
        assert_eq!(g.player.money, 1000 - 25);
    }

    #[test]
    fn inventory_lines_reflect_fighter_state() {
        let mut f = player();
        f.joints = 2;
        f.money = 50;
        f.beer_dl = 3;
        let lines = f.inventory_lines();
        assert!(lines.iter().any(|l| l.contains("Косяки 2")));
        assert!(lines.iter().any(|l| l.contains("Бабки 50")));
        assert!(lines.iter().any(|l| l.contains("1.5")));
    }

    #[test]
    fn inspect_before_any_fight_prints_nothing_and_does_not_panic() {
        let g = game();
        assert!(g.last_enemy.is_none());
        g.inspect_enemy();
    }

    #[test]
    fn sell_junk_and_sell_items_are_the_dealers_own_keys() {
        let mut g = game();
        g.location = Location::BigMarket;
        g.mode = Mode::Shop(Location::BigMarket);
        // Neither has a backing field yet, so the only observable effect is
        // that the keys are routed at all: they must not leave the shop.
        g.shop_turn(Location::BigMarket, "x");
        assert_eq!(g.mode, Mode::Shop(Location::BigMarket));
        g.shop_turn(Location::BigMarket, "wes");
        assert_eq!(g.mode, Mode::Shop(Location::BigMarket));
    }

    /// Every class `pick_enemy` can roll must resolve to a named row, or the
    /// `panic!` in `pick_enemy` would show a nameless fighter to the player.
    #[test]
    fn every_rolled_enemy_class_has_a_name() {
        for class in 0u16..10 {
            let row = data::enemies().iter().find(|e| e.class == class);
            let row = row.unwrap_or_else(|| panic!("no enemies.json row for class {class}"));
            assert!(!row.name.is_empty(), "class {class} has an empty name");
        }
    }

    #[test]
    fn pick_enemy_produces_a_named_fighter() {
        let mut g = game();
        let e = g.pick_enemy();
        assert!(!e.name.is_empty());
        assert!(e.hp > 0);
    }

    #[test]
    fn combat_round_actually_lands_hits_over_a_bounded_number_of_rounds() {
        let mut g = game();
        let mut enemy = player();
        enemy.agility = 0; // give the roll every chance to land a hit
        enemy.hpmax = 10_000;
        enemy.hp = 10_000;
        g.player.agility = 30;
        g.player.dmg_min = 5;
        g.player.dmg_max = 10;
        let before = enemy.hp;
        for _ in 0..20 {
            g.combat_round(&mut enemy);
        }
        assert!(
            enemy.hp < before,
            "20 rounds at a 90% cap must land at least one blow (hp {} -> {})",
            before,
            enemy.hp
        );
    }

    /// Replays `walk`'s draws for `seed` and reports the `Random(2)` the
    /// decline branch would see, or `None` when the wander roll does not
    /// produce an encounter at all.
    ///
    /// **What this can and cannot catch.** It re-runs the same draw sequence
    /// `walk` runs, so it pins which *arm* a given roll takes and would fail
    /// if the branch were inverted -- but it is not an independent oracle
    /// for the draws themselves. If `walk` gained, lost or reordered a
    /// `Random` call, this helper would drift with it and stay green. Only
    /// the disassembly (and a live trace of the `Random` call sites) settles
    /// the draw count and order.
    fn decline_roll_for(seed: u32) -> Option<u16> {
        let mut g = Game::new(player(), Progress::new(), seed);
        let roll = g.rng.below(25) + 1;
        if !(5..=9).contains(&roll) {
            return None;
        }
        let _ = g.pick_enemy();
        Some(g.rng.below(2))
    }

    /// **C1.** `1000:b718`..`1000:b74e`: after a non-`y` answer, `Random(2)`
    /// returning **0** falls through to `1000:b72e`, which writes
    /// `^4Он тебя заметил.` and sets the accept flag at `1000:b747` -- the
    /// fight happens. A **non-zero** roll jumps to `1000:b74e`, writes
    /// `^2Ты смылся.` and leaves the flag clear -- no fight.
    ///
    /// Observed through `running`: entering combat consumes the (empty)
    /// rest of the input script and stops the loop; escaping does not.
    #[test]
    fn declining_an_encounter_fights_on_roll_zero_and_escapes_otherwise() {
        let mut zero = None;
        let mut nonzero = None;
        for seed in 1u32..40_000 {
            match decline_roll_for(seed) {
                Some(0) if zero.is_none() => zero = Some(seed),
                Some(n) if n != 0 && nonzero.is_none() => nonzero = Some(seed),
                _ => {}
            }
            if zero.is_some() && nonzero.is_some() {
                break;
            }
        }
        let zero = zero.expect("no seed produced a decline roll of 0");
        let nonzero = nonzero.expect("no seed produced a non-zero decline roll");

        let mut g = Game::new(player(), Progress::new(), zero);
        g.walk(&mut input(&["n"])).unwrap();
        assert!(
            !g.running,
            "Random(2) == 0 means noticed: combat must start (seed {zero})"
        );

        let mut g = Game::new(player(), Progress::new(), nonzero);
        g.walk(&mut input(&["n"])).unwrap();
        assert!(
            g.running,
            "Random(2) != 0 means escaped: no combat (seed {nonzero})"
        );
        assert!(g.last_enemy.is_none(), "escaping must not record a fight");
    }

    /// Accepting with `y` always fights, whatever the RNG says next.
    #[test]
    fn accepting_an_encounter_always_fights() {
        let seed = (1u32..40_000)
            .find(|s| decline_roll_for(*s).is_some())
            .expect("no seed produced an encounter");
        let mut g = Game::new(player(), Progress::new(), seed);
        g.walk(&mut input(&["Y"])).unwrap(); // 0eed:0216 case-folds first
        assert!(!g.running, "an accepted encounter must enter combat");
    }

    /// The first seed whose wander roll lands in bucket 2 (`2..=4`, the
    /// girl event) and whose following `Random(2)` equals `want`. Same
    /// caveat as [`decline_roll_for`]: it replays `walk`'s own draws, so it
    /// pins the branch, not the draw sequence.
    fn girl_seed_with_roll(want: u16) -> u32 {
        (1u32..40_000)
            .find(|&seed| {
                let mut g = Game::new(player(), Progress::new(), seed);
                let roll = g.rng.below(25) + 1;
                (2..=4).contains(&roll) && g.rng.below(2) == want
            })
            .unwrap_or_else(|| panic!("no seed produced a girl event with Random(2) == {want}"))
    }

    /// **CRITICAL.** `1000:b54e`'s `Random(2)` returning **0** reaches
    /// `1000:b570` (`c6 06 97 36 01`), which sets the *girl's* flag
    /// `20ae:3697`. This is the only discovery path the port implements and
    /// the only reason any location is reachable in a real session.
    #[test]
    fn wander_bucket_two_discovers_the_girl_on_roll_zero() {
        let seed = girl_seed_with_roll(0);
        let mut g = Game::new(player(), Progress::new(), seed);
        assert!(!g.places.is_found(Location::Girl));
        g.walk(&mut input(&["y"])).unwrap();
        assert!(
            g.places.is_found(Location::Girl),
            "Random(2) == 0 must set 20ae:3697 (seed {seed})"
        );
        // The flag is the girl's, not the den's: 0x3696 stays clear.
        assert!(!g.places.is_found(Location::Den));
        assert!(g.running, "the girl event never enters combat");
    }

    /// A non-zero `Random(2)` jumps to `1000:b577`, writes the brush-off and
    /// leaves `20ae:3697` clear.
    #[test]
    fn wander_bucket_two_leaves_the_flag_clear_on_a_non_zero_roll() {
        let seed = girl_seed_with_roll(1);
        let mut g = Game::new(player(), Progress::new(), seed);
        g.walk(&mut input(&["y"])).unwrap();
        assert!(
            !g.places.is_found(Location::Girl),
            "Random(2) != 0 must not set 20ae:3697 (seed {seed})"
        );
    }

    /// `1000:b548` `75 46`: any answer but `y` skips the `Random(2)`
    /// entirely and ends the turn, so declining must neither discover the
    /// girl nor consume a draw.
    #[test]
    fn declining_the_girl_sets_nothing_and_spends_no_draw() {
        let seed = girl_seed_with_roll(0);
        let mut g = Game::new(player(), Progress::new(), seed);
        g.walk(&mut input(&["n"])).unwrap();
        assert!(!g.places.is_found(Location::Girl));
        // The declined turn spent only the bucket roll, so the next draw is
        // still the Random(2) the accepted turn would have seen.
        assert_eq!(g.rng.below(2), 0, "the decline branch must not draw");
    }

    /// `1000:b4ef`'s non-zero arm (`1000:b592`) writes
    /// `Совсем ничё не происходит.` and reads no input at all -- so an
    /// already-discovered girl must not consume a line from the script.
    #[test]
    fn wander_bucket_two_reads_no_input_once_the_girl_is_known() {
        let seed = girl_seed_with_roll(0);
        let mut g = Game::new(player(), Progress::new(), seed);
        g.places.mark_found(Location::Girl);
        let mut lines = input(&["y"]);
        g.walk(&mut lines).unwrap();
        assert!(g.running);
        assert!(
            lines.next().is_some(),
            "the already-found arm must not ReadLn"
        );
    }

    /// The chain the CRITICAL finding was about: wander discovers the girl,
    /// `girl` then discovers the club. Both flags come from real setters
    /// (`1000:b570`, `1000:d751`).
    #[test]
    fn wander_then_girl_makes_the_club_reachable() {
        let seed = girl_seed_with_roll(0);
        let mut g = Game::new(player(), Progress::new(), seed);
        g.player.money = 100;
        g.walk(&mut input(&["y"])).unwrap();
        assert!(g.places.is_found(Location::Girl));
        // visit_girl's own Random(2) must come up 0 for the club reveal;
        // drive it until it does, which the player can do by revisiting.
        for _ in 0..40 {
            if g.places.is_found(Location::Club) {
                break;
            }
            g.player.money = 100;
            g.dispatch(Command::Girl, &mut no_input()).unwrap();
        }
        assert!(
            g.places.is_found(Location::Club),
            "girl must be able to reveal the club (seed {seed})"
        );
        g.dispatch(Command::Club, &mut no_input()).unwrap();
        assert_eq!(g.mode, Mode::Shop(Location::Club));
    }

    /// I7: a dead player ends the game (`1000:5053` -> `FUN_1000_074b(0)`,
    /// which ends in `Halt`), rather than walking on as a 0-HP corpse.
    #[test]
    fn player_death_stops_the_loop() {
        let mut g = game();
        g.player.hp = 0;
        let enemy = player();
        g.run_combat(enemy, &mut no_input()).unwrap();
        assert!(!g.running, "death must end the game");
    }

    #[test]
    fn winning_a_fight_awards_experience_and_keeps_the_game_running() {
        let mut g = game();
        let mut enemy = player();
        enemy.hp = 0;
        enemy.strength = 3;
        enemy.agility = 3;
        enemy.vitality = 3;
        enemy.luck = 3;
        let before = g.progress.xp;
        g.run_combat(enemy, &mut no_input()).unwrap();
        assert!(g.running);
        assert!(g.last_enemy.is_some());
        assert!(g.progress.xp > before, "an award must be credited");
    }
}
