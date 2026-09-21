//! The main loop: dispatch, locations, and the handlers small enough to
//! belong here.
//!
//! Every user-visible string here is kept verbatim from the original,
//! with its `^N` colour markup and spacing.
//!
//! ## Design
//!
//! The game is modal:
//! * **Combat is modal.** The combat dispatcher runs its own prompt loop.
//! * **Locations are modal.** Each location writes its own prompt.
//! * **Walking rolls for an encounter**, reading a second line.
//!
//! No typed save command exists. Saving is checkpoint-only at two sites:
//! the mage's paid save and the district-advance autosave.

use crate::character_sheet;
use crate::church;
use crate::club;
use crate::combat::{self, blows_per_round, resolve_blow_nth, Break, Swing};
use crate::combat_dispatch::{self, Backup, Called, Shot, Status};
use crate::combat_opener;
use crate::commands::{parse, Command};
use crate::data;
use crate::den;
use crate::ending;
use crate::enemy_sheet;
use crate::gym;
use crate::locations::{Location, Places};
use crate::market;
use crate::model::Fighter;
use crate::opening;
use crate::progress::{self, Progress};
use crate::rng::Rng;
use crate::spoils;
use crate::term;
use crate::text;
use crate::vet;
use crate::wander;
use std::io::{self, BufRead};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    Street,
    Shop(Location),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImmRow {
    /// The verb whose handler contains the row: `rep`, `kl` or `trn`.
    pub shop: &'static str,
    /// The key the player types, read off the row's own prefix string.
    pub key: &'static str,
    /// The immediate at `site`, in rubles.
    pub price: i32,
    pub site: &'static str,
    pub prefix: &'static str,
    pub text: &'static str,
}

pub const IMM_ROWS: [ImmRow; 9] = [
    ImmRow {
        shop: "rep",
        key: "h",
        price: 3,
        site: "1000:d410",
        prefix: "  ^2h^7 - за ^",
        text: "3^7 рубля тебя залатают",
    },
    ImmRow {
        shop: "rep",
        key: "r",
        price: 7,
        site: "1000:d465",
        prefix: "  ^2r^7 - за ^",
        text: "7^7 рублей починят переломы",
    },
    ImmRow {
        shop: "kl",
        key: "1",
        price: 15,
        site: "1000:df6f",
        prefix: " 1 -  ^",
        text: "15^7  потусоваться на дискотеке(Ловкость +1)",
    },
    ImmRow {
        shop: "kl",
        key: "2",
        price: 22,
        site: "1000:dfcb",
        prefix: " 2 -  ^",
        text: "22^7  разузнать приемы мухлёжников(Удача +1)",
    },
    ImmRow {
        shop: "trn",
        key: "1",
        price: 20,
        site: "1000:e400",
        prefix: " 1 -  ^",
        text: "20^7  качаться гателями и шгангой(Сила +1)",
    },
    ImmRow {
        shop: "trn",
        key: "2",
        price: 20,
        site: "1000:e455",
        prefix: " 2 -  ^",
        text: "20^7  качаться на тренажерах(Выносливость +1)",
    },
    ImmRow {
        shop: "trn",
        key: "3",
        price: 10,
        site: "1000:e4c4",
        prefix: " 3 -  ^",
        text: "10^7  прокачать # качков опыта",
    },
    ImmRow {
        shop: "trn",
        key: "4",
        price: 30,
        site: "1000:e521",
        prefix: " 4 -  ^",
        text: "30^7  купить зубную защиту боксёров(-75% что сломают челюсть)",
    },
    ImmRow {
        shop: "trn",
        key: "5",
        price: 20,
        site: "1000:e58f",
        prefix: " 5 -  ^",
        text: "20^7  прокачать пресс(Броня +1)",
    },
];

pub const MAGE_LINES: [&str; 4] = [
    "Бродя по окрестностям с самыми грязными намериниями...",
    "Ты встретил великого мага и экстрасенса - Рушеля Блаво.",
    "За # рублей он может сделать сохранение прямо здесь.",
    "Ты хочешь сохраниться?",
];

pub struct Game {
    pub player: Fighter,
    pub progress: Progress,
    pub places: Places,
    pub district: u8,
    pub rng: Rng,
    pub location: Location,
    pub has_mobile: bool,
    pub harder_encounters: bool,
    pub dark_glasses: bool,
    pub prison_tattoo: bool,
    pub oneshot_gift_1: bool,
    pub oneshot_gift_2: bool,
    pub ring_gospodi_pomilui: bool,
    /// The street-cred counter, понтовость на улице, is **not** the level.
    /// Gates a message (`>= 100`) and is topped up by the church's arm 4.
    pub pontovost_street: i16,
    pub buff_countdown: u8,
    pub market_ban_countdown: u8,
    pub club_ban_countdown: u8,
    pub club_stake: u8,
    pub den_errand_1_pending: bool,
    pub den_errand_2_pending: bool,
    pub fight_accepted: bool,
    /// The pistol, its silencer, and its magazine.
    ///
    /// Dealers row 7 sells the pistol; row 8 requires it; row 9 requires
    /// exactly 25 walks after owning it and sells the silencer.
    pub pistol: crate::combat_dispatch::Pistol,
    /// Counts walks 0..25 once the pistol is owned. The phone call fires
    /// at exactly 25. Tracks the dealers' delivery time on the silencer.
    pub dealer_delivery_counter: u8,
    /// The rector showdown flag. When set, there is no crowd, no fleeing,
    /// and the death message names the killer.
    ///
    /// **Nothing ever clears it.** Once set, all three effects apply for
    /// the rest of the game.
    pub rector_showdown: bool,
    /// The den's loan credit. Set to 5 and topped up once per walk while
    /// below `district * 10`.
    pub den_loan_credit: u8,
    /// зубная защита (tooth guard). Changes only jaw breaks landing on the
    /// player: `Random(4)` decides if the guard saves the teeth. This is a
    /// **draw count** difference -- a save with it desynchronises replays
    /// without it.
    pub tooth_guard: bool,
    /// the крестик (`luck += 2`).
    pub charm_krestik: bool,
    /// кольцо "Господи спаси" (`luck += 1`).
    pub charm_ring: bool,
    /// кастет. The four weapon flags gate each other's damage bonuses,
    /// so all four have to be carried.
    pub weapon_kastet: bool,
    /// дубинка (club).
    pub weapon_dubinka: bool,
    /// ножик (knife).
    pub weapon_nozhik: bool,
    /// тесак (axe).
    pub weapon_tesak: bool,
    /// костюм Abibas (`mar` row 4).
    pub wear_suit_abibas: bool,
    /// Бутсы.
    pub wear_boots: bool,
    /// Кожанка, `mar` row 6.
    pub wear_jacket: bool,
    /// костюм Adidas, `mar` row 7.
    pub wear_suit_adidas: bool,
    /// Понтовые бутсы.
    pub wear_boots_pontovye: bool,
    /// Крутая кожанка, `mar` row 9.
    pub wear_jacket_krutaya: bool,
    /// Where [`Game::mage_save`](crate::persist) and any other writer put
    /// their files.
    ///
    /// Save files are named `save_r0.sav`..`save_rN.sav` and
    /// `places.sav`, all written to the current directory by default.
    /// It is a field rather than a `current_dir()` call so a test can
    /// point a save at a scratch directory instead of dropping
    /// `save_r0.sav` into the working tree, which is what `cargo test`
    /// would otherwise do the first time a test answers `y` to the mage.
    pub save_dir: std::path::PathBuf,
    /// The church's sermon stage, 0..2. Picks which sermon runs, and
    /// picks the parting line.
    pub church_visits: u8,
    mode: Mode,
    /// The most recently fought opponent, shown by `Command::Inspect` (`sv`).
    last_enemy: Option<Fighter>,
    running: bool,
}

impl Game {
    /// Start a brand-new character.
    ///
    /// A brand-new character already starts in district 1 with the vet
    /// and the market discovered.
    ///
    /// This runs one of three ways: silently, when no save files exist
    /// yet; after the player answers `1` ("начать сначала") at the
    /// save-slot prompt `^0Нажми цифру с какого района начать. 1-начать
    /// сначала`, when save files are present; or when loading an
    /// existing save fails, which prints `^6Чё-то глюкануло - нaверно
    /// нет такого сейва, Default:1` and falls through into a fresh
    /// character.
    ///
    /// Loading `places.sav` is a separate path: if that load fails, it
    /// clears the discovery flags instead of setting them, and does not
    /// go through this setup.
    pub fn new(player: Fighter, progress: Progress, seed: u32) -> Game {
        let mut places = Places::from_bytes(&[0u8; 7]);
        places.mark_found(Location::Vet);
        places.mark_found(Location::Market);
        let mut g = Game {
            player,
            progress,
            places,
            district: 1,
            rng: Rng::new(seed),
            location: Location::Street,
            has_mobile: false,
            harder_encounters: false,
            dark_glasses: false,
            prison_tattoo: false,
            oneshot_gift_1: false,
            oneshot_gift_2: false,
            ring_gospodi_pomilui: false,
            pontovost_street: 0,
            buff_countdown: 0,
            market_ban_countdown: 0,
            club_ban_countdown: 0,
            club_stake: 5,
            den_errand_1_pending: false,
            den_errand_2_pending: false,
            fight_accepted: false,
            pistol: crate::combat_dispatch::Pistol::default(),
            rector_showdown: false,
            dealer_delivery_counter: 0,
            den_loan_credit: 0,
            church_visits: 0,
            tooth_guard: false,
            charm_krestik: false,
            charm_ring: false,
            weapon_kastet: false,
            weapon_dubinka: false,
            weapon_nozhik: false,
            weapon_tesak: false,
            wear_suit_abibas: false,
            wear_boots: false,
            wear_jacket: false,
            wear_suit_adidas: false,
            wear_boots_pontovye: false,
            wear_jacket_krutaya: false,
            save_dir: std::path::PathBuf::from("."),
            mode: Mode::Street,
            last_enemy: None,
            running: true,
        };
        g.apply_class_bonus();
        g
    }

    /// The district-5 rector-showdown arm, then the class bonus, then the
    /// den's opening loan credit. Runs on every entry into the game, new
    /// character or loaded save.
    ///
    /// If the district is already 5 at entry -- which a loaded save can
    /// be, even before a single turn is played -- this arms the rector
    /// showdown and prints `^1Пора наконец отомстить ректору...`,
    /// exactly once. That line is a separate copy of the one
    /// [`Game::enter_district_5`] prints when the district first becomes
    /// 5 during play: the game repeats itself with two copies of a
    /// similar message rather than one, and only one of the two ever
    /// fires per game -- this one only when district is already 5 at
    /// entry, the other only when it becomes 5 mid-game.
    ///
    /// The class bonus is mutually exclusive by class:
    ///
    /// * Гопник -- Den discovered
    /// * Подтсан -- Girl and Club discovered
    /// * Вор -- Dealers discovered
    /// * Отморозок gets no flag here; its bonus is +1 HP per walk.
    ///
    /// Regardless of class, the den's opening loan credit is set to 5.
    ///
    /// This function's flag-setting runs twice -- once for a new game,
    /// once when loading a save -- which is fine for these idempotent
    /// writes but would double-print a message, so the district-5
    /// announcement text itself lives in [`Game::announce_district`],
    /// printed once by the caller after the `Game` is produced, not
    /// here.
    pub(crate) fn apply_class_bonus(&mut self) {
        if self.district == 5 {
            self.rector_showdown = true;
        }
        match self.player.class {
            5 => self.places.mark_found(Location::Den),
            3 => {
                self.places.mark_found(Location::Girl);
                self.places.mark_found(Location::Club);
            }
            6 => self.places.mark_found(Location::Dealers),
            _ => {}
        }
        self.den_loan_credit = 5;
    }

    /// Everything the entry pass prints, as distinct from what
    /// [`Game::apply_class_bonus`] stores.
    ///
    /// Called once per process, from `main.rs`, whichever way the
    /// `Game` was produced -- reached from both a new character and a
    /// loaded save.
    ///
    /// Districts 2, 3 and 4 print the same wording
    /// [`Game::district_advance`] prints on promotion, from a different
    /// copy of each string; see `crate::opening`'s module doc.
    pub fn announce_district(&self) {
        if let Some(pair) = opening::START_ARRIVAL
            .chunks_exact(2)
            .nth(usize::from(self.district).wrapping_sub(1))
        {
            for line in pair {
                term::println(line);
            }
        }
        if self.district == 5 {
            term::println("^1Пора наконец отомстить ректору...");
        }
        if self.district == 1 {
            for line in opening::TUTORIAL {
                term::println(line);
            }
        }
    }

    /// [`Game::banner`] is never printed at start-up -- neither by
    /// `main.rs` nor by this method. The only on-screen copy of the
    /// version text is the `version` command's own literal, read on
    /// demand; `main.rs`'s start-up screen is an ASCII splash
    /// ([`opening::splash`]) instead.
    ///
    /// **The loop starts with [`Game::district_advance`], not with the
    /// prompt.** It is the first thing every turn does, upstream of the
    /// street prompt.
    ///
    /// **Only `Mode::Street` turns pass through it.** Each shop handler
    /// has its own prompt and its own loop and never reaches this point.
    /// `Mode::Shop` is this port's line-at-a-time stand-in for that
    /// inner loop, so running the advance on those iterations would
    /// promote the player on turns the original does not.
    ///
    /// The level only rises on Street turns, through exactly two paths:
    /// [`Game::run_combat`]'s post-fight award (entered from
    /// [`Game::walk`]), and [`Game::church`]'s zero arm, which forces a
    /// level by setting xp to the threshold. The church itself is
    /// reached from the wander preamble by a `Random(200)` roll landing
    /// on zero, and the level-forcing outcome is one of five the church
    /// then rolls for.
    ///
    /// A promotion clears all seven discovery flags on the way
    /// ([`Places::reset_for_new_district`]), so no shop is enterable
    /// again until rediscovered.
    pub fn run(&mut self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();
        while self.running {
            if matches!(self.mode, Mode::Street) {
                self.district_advance(&mut lines)?;
                if !self.running {
                    break;
                }
                // The endgame arm runs on every turn once the district is 5,
                // not only on the turn it's reached.
                self.rector_endgame(&mut lines)?;
                if !self.running {
                    break;
                }
            }
            self.prompt();
            let Some(line) = term::read_line(&mut lines) else {
                break;
            };
            let line = line?;
            match self.mode.clone() {
                Mode::Street => {
                    let cmd = parse(&line);
                    if cmd == Command::Walk {
                        // Typing `run` (vs `w`) is checked again here, since
                        // `parse` already folded both into the same
                        // `Command::Walk` variant; the raw comparison is what
                        // `walk_verb` uses to tell them apart. See
                        // `crate::wander`'s module doc.
                        self.walk_verb(line.eq_ignore_ascii_case("run"), &mut lines)?;
                    } else {
                        self.dispatch(cmd, &mut lines)?;
                    }
                }
                Mode::Shop(loc) => self.shop_turn(loc, &line, &mut lines)?,
            }
        }
        Ok(())
    }

    /// The district-advance preamble, and the autosave prompt hanging
    /// off it. Runs at the top of every street turn.
    ///
    /// Promotion happens once level reaches `district * 10` and the
    /// district is still under 5: the district advances by one, every
    /// discovery flag resets, the market and club ban countdowns reset
    /// to zero, and two lines print:
    ///
    /// `^1Ты доказал, что ты самый крутой в этом районе - отправляйся в
    /// следующий`
    /// `^0Хочешь сохранить свои достижения?`
    ///
    /// Then a bare `\` prompt asks to save. On `y` (case-insensitive),
    /// the character is written to `save_r<digit>.sav` and
    /// `^1Сохранено в save_r<digit>.sav` prints.
    ///
    /// At most one district is gained per turn, even if the level
    /// clears more than one threshold at once -- promotion is checked
    /// once per turn, not looped.
    pub fn district_advance(
        &mut self,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        if u16::from(self.district) * 10 > self.player.level {
            return Ok(());
        }
        if self.district >= 5 {
            return Ok(());
        }
        self.district += 1; // 1000:ab92
        self.places.reset_for_new_district(self.player.class);
        self.market_ban_countdown = 0; // 1000:abce
        self.club_ban_countdown = 0; // 1000:abd3
        term::println("^1Ты доказал, что ты самый крутой в этом районе - отправляйся в следующий");
        term::println("^0Хочешь сохранить свои достижения?");
        term::print("\\");
        let Some(line) = term::read_line(lines) else {
            self.running = false;
            return Ok(());
        };
        let answer = line?;
        // The confirmation compare is case-insensitive but not trimmed: a
        // leading or trailing space (e.g. " y") is rejected. Kept for
        // consistency with the identical prompt in `Game::mage` and in
        // `crate::commands::parse`.
        if answer.eq_ignore_ascii_case("y") {
            // The save filename uses the district after the increment, so
            // shipped saves run `SAVE_R2`..`SAVE_R5` with no `SAVE_R1` --
            // district is always bound to 2..=5 here.
            let digit = char::from(b'0' + self.district);
            let name = crate::persist::slot_filename(digit);
            let dir = self.save_dir.clone();
            match self.write_save_as(&dir, &name) {
                Ok(_) => term::println(&format!("^1Сохранено в save_r{digit}.sav")),
                Err(e) => term::println(&format!("^6{e}")),
            }
        }
        // The arrival announcement for the district just promoted into.
        // Only districts 2, 3 and 4 have one here; it's a separate copy of
        // the districts 2/3/4 text also printed at game entry -- see
        // `crate::opening`'s module doc.
        if let Some(pair) = opening::ADVANCE_ARRIVAL
            .chunks_exact(2)
            .nth(usize::from(self.district).wrapping_sub(2))
        {
            for line in pair {
                term::println(line);
            }
        }
        if self.district == 5 {
            self.enter_district_5(lines);
        }
        Ok(())
    }

    /// The street prompt is a single backslash (`\`). Each location
    /// writes its own prompt instead of reusing this one.
    fn prompt(&self) {
        let p = match &self.mode {
            Mode::Street => "\\",
            Mode::Shop(Location::Market) => "^0Базар\\",
            Mode::Shop(Location::Dealers) => "^0Барыги\\",
            Mode::Shop(Location::Vet) => crate::vet::EMITTED[3].1,
            Mode::Shop(Location::Den) => den::EMITTED[13].1,
            Mode::Shop(Location::Club) => crate::club::EMITTED[3].1,
            Mode::Shop(Location::Gym) => crate::gym::EMITTED[1].1,
            Mode::Shop(_) => "\\",
        };
        term::print(p);
    }

    #[cfg(test)]
    pub(crate) fn enter_vet_for_test(&mut self) {
        self.enter_shop(Location::Vet);
    }

    /// Whether the player is at the STREET prompt rather than a location's
    /// own. `mode` is private to this module; `crate::vet`'s exit test needs
    /// to tell "left the shop" from "quit the game" and this is the half it
    /// can see.
    #[cfg(test)]
    pub(crate) fn mode_is_street(&self) -> bool {
        self.mode == Mode::Street
    }

    fn banner(&self) {
        term::println("^4Gopnik: ^7version 1.02 june,sept 2003");
    }

    fn dispatch(
        &mut self,
        cmd: Command,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        match cmd {
            // Both `exit` and `e` print two lines and the full character sheet before
            // quitting. The `e` typed at the FIGHT prompt is different -- it quits without
            // printing any of this. See `Game::run_combat`.
            Command::Quit => {
                for line in opening::QUIT_TAIL {
                    term::println(line);
                }
                self.show_stats();
                term::read_key(lines);
                self.running = false;
            }
            Command::Stats => self.show_stats(),
            Command::Fight => {
                term::println("^6Чё машешь копытами? Ищи мудака которого будешь пинать!")
            }
            Command::Shoot => self.shoot(),
            Command::Inspect => self.inspect_enemy(),
            Command::Backup => self.call_backup(),
            Command::Walk => self.walk(lines)?,
            Command::LegacyFight => {
                term::println("^6Пережитки прошлого жми ^6w^7 чтобы искать врагов");
            }
            Command::Market => self.enter_shop(Location::Market),
            Command::Dealers => self.enter_shop(Location::Dealers),
            Command::Vet => self.enter_shop(Location::Vet),
            Command::Girl => self.enter_shop(Location::Girl),
            Command::Den => self.enter_shop(Location::Den),
            Command::Club => self.enter_shop(Location::Club),
            Command::Gym => self.enter_shop(Location::Gym),
            Command::CommandList => self.show_command_list(),
            Command::Help => self.show_help(),
            Command::Version => self.banner(),
            Command::Name => self.rename(lines)?,
            // `kos` is a one-shot street verb, not a submenu -- it runs once
            // and returns straight to the street prompt.
            Command::Joint => self.smoke(Joint::Street),
            Command::Drink => self.beer(Beer::One),
            Command::BingeDrink => self.beer(Beer::Binge),
            // `x` and `wes` are DEALERS sub-verbs, not street verbs, so
            // typing them at the street prompt is silently ignored, the
            // same as any unmatched line -- the original has no such
            // street-level verb. `Game::shop_turn` is the only route to
            // those two.
            Command::SellJunk | Command::SellItems => {}
            // An unmatched line at the street prompt produces no output at
            // all.
            Command::Unknown(_) => {}
        }
        Ok(())
    }

    /// The "you have not found this place yet" refusal, one verbatim
    /// string per location: `mar`, `bmar`, `pr`, `girl`, `rep`, `kl`,
    /// `trn`.
    fn undiscovered_line(loc: Location) -> &'static str {
        match loc {
            Location::Market => "^6Ты незнаешь, пока ешё, где находтся базар",
            Location::Dealers => {
                "^6Туда любого дебила с улицы непропустят - сначала докажи, что ты не засранец - отпинай побольше ублюдков"
            }
            Location::Den => den::EMITTED[30].1,
            Location::Girl => "^4У тебя пока нет девчонки.",
            Location::Vet => crate::vet::EMITTED[13].1,
            Location::Club => crate::club::EMITTED[19].1,
            Location::Gym => crate::gym::EMITTED[20].1,
            Location::Street | Location::Temple | Location::Dorm => "",
        }
    }

    /// `mar`/`bmar`/`rep`/`girl`/`pr`/`kl`/`trn`, gated by
    /// [`Places::is_found`], mirroring the seven discovery flags (see
    /// [`Game::undiscovered_line`]).
    ///
    /// A refused entry only prints. It does **not** discover the place
    /// -- the flags are set elsewhere, never by a failed entry.
    ///
    /// The market and the vet are open from turn one. Visiting the girl
    /// sets the club's discovery flag; the girl's own flag is set by one
    /// of the wander outcomes -- so `w` -> girl -> club is a real,
    /// reachable chain.
    ///
    /// Den comes from a class-5 bonus, Dealers from a class-6 bonus, and
    /// Gym from a 1-in-100 roll per walk -- rare, not unreachable.
    fn enter_shop(&mut self, loc: Location) {
        if !self.places.is_found(loc) {
            term::println(Self::undiscovered_line(loc));
            return;
        }
        // The club is open only while its ban countdown is zero; a
        // non-zero countdown prints the refusal and ends the visit.
        if loc == Location::Club && self.club_ban_countdown > 0 {
            // The refusal: `^6Тебе не стоит пока туда соваться`.
            term::println(crate::club::EMITTED[0].1);
            return;
        }
        // Same rule for the market: open only while its ban countdown is
        // zero, checked after the discovery gate -- so an undiscovered
        // market prints its own refusal, never this one.
        if loc == Location::Market && self.market_ban_countdown > 0 {
            term::println(market::BANNED);
            return;
        }
        self.location = loc;
        if loc == Location::Girl {
            // Not modal -- no prompt, no follow-up input.
            self.visit_girl();
            self.location = Location::Street;
            return;
        }
        self.mode = Mode::Shop(loc);
        self.print_shop_intro(loc);
        if loc == Location::Vet {
            // Entering the vet runs the loop's top once before the first
            // prompt is shown.
            vet::loop_top(self);
        }
        if loc == Location::Club {
            self.club_stake = 5;
        }
    }

    /// End the current visit without the player typing the exit key.
    ///
    /// Used when the club catches the player cheating, ejecting them
    /// from the location.
    pub(crate) fn leave_shop(&mut self) {
        self.location = Location::Street;
        self.mode = Mode::Street;
    }

    /// Visiting the girl, in order:
    ///
    /// * Costs 12 rubles; too poor and nothing else happens beyond the
    ///   refusal.
    /// * A coin flip (`Random(2)`), only while the club is still
    ///   undiscovered, sets the club's discovery flag -- one of the two
    ///   ways the club can be found.
    /// * On success: heals to full HP, deducts the 12 rubles, and clears
    ///   the market ban countdown.
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
        self.player.hp = self.player.hpmax; // 1000:d788
        self.player.money = self.player.money.wrapping_sub(12_i16); // 1000:d78e
        self.market_ban_countdown = 0; // 1000:d793
    }

    /// The colour digit the original appends to a price row's prefix.
    ///
    /// Every priced menu row is built as `<prefix ending in "^">` +
    /// `'0'`/`'4'` + `<row text>`, so the two halves only form a valid
    /// `^N` code once joined: `'0'` when the row is affordable, `'4'`
    /// when it is not. The price digit itself is *not* eaten by the
    /// markup -- the colour digit sits between the `^` and the price.
    fn afford(&self, price: i32) -> &'static str {
        if i32::from(self.player.money) >= price {
            "0"
        } else {
            "4"
        }
    }

    /// Everything a location writes before its own prompt.
    ///
    /// `mar` and `bmar` each print three flavour lines then their priced
    /// rows; each row is `^6N^7 - ^` + the affordability digit + the
    /// row's own text, with `#` filled from the displayed price. `bmar`
    /// reuses the same nine row prefixes as `mar`. A row's district gate
    /// is the same `district > N` test used to list it.
    ///
    /// `rep`, `kl` and `trn` price their rows from a value baked
    /// directly into the code rather than from the shared price table.
    fn print_shop_intro(&mut self, loc: Location) {
        match loc {
            Location::Market => {
                term::println("Ты пришел на базар напиши  ^6w^7  чтобы уйти.");
                term::println("Можно потискать здесь у лохов кошельки(^6t^7).");
                term::println("А можно чё-то купить");
                self.print_priced_rows("mar");
            }
            Location::Dealers => {
                term::println("Ты пришел к барыгам напиши  ^6w^7  чтобы уйти.");
                term::println("Здесь можно толкнуть хлам(^6x^7) и купить кое-что");
                term::println("Ещё ты можешь продать ненужные вещи - ^6wes^7");
                self.print_priced_rows("bmar");
            }
            Location::Vet => {
                term::println(crate::vet::EMITTED[0].1);
                // A healthy player skips the vet's menu entirely on entry;
                // this is the menu skip, not the eject (the eject is a
                // different path).
                if self.player.hp >= self.player.hpmax
                    && !self.player.broken_jaw
                    && !self.player.broken_leg
                {
                    return;
                }
                term::println(crate::vet::EMITTED[1].1);
                self.print_imm_rows("rep");
            }
            Location::Den => {
                self.print_den_intro();
                self.print_den_menu();
            }
            Location::Club => {
                term::println(crate::club::EMITTED[1].1);
                term::println(crate::club::EMITTED[2].1);
                self.print_imm_rows("kl");
            }
            Location::Gym => {
                term::println(crate::gym::EMITTED[0].1);
                self.print_imm_rows("trn");
            }
            Location::Girl | Location::Street | Location::Temple | Location::Dorm => {}
        }
    }

    /// The nine "^6N^7 - ^" prefixes (N = the row's digit).
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

    /// Which of `tag`'s rows the menu lists, in order -- the district
    /// filter and nothing else.
    ///
    /// This is only the listing filter; buying a row applies its own,
    /// separate district check.
    fn listed_rows(&self, tag: &str) -> Vec<&'static data::ShopEntry> {
        data::shops()
            .iter()
            .filter(|r| r.shop == tag && self.gate_open(r.gate) && self.extra_gates_open(r))
            .collect()
    }

    /// The menu gates that are not a district test -- the dealers'
    /// silencer row is the only one with extra requirements: owning a
    /// pistol, and the delivery counter at exactly 25 (on top of
    /// district > 3). Missing any of these prints nothing at all, rather
    /// than printing the row in a refusing colour.
    fn extra_gates_open(&self, row: &data::ShopEntry) -> bool {
        row.extra_gates.iter().all(|gate| match *gate {
            "byte[20ae:394d]!=0" => self.pistol.owned,
            "byte[20ae:3e32]==25" => self.dealer_delivery_counter == 25,
            other => panic!(
                "unmodelled extra_gate {other:?} on {} row {}",
                row.shop, row.key
            ),
        })
    }

    fn print_priced_rows(&self, tag: &str) {
        for row in self.listed_rows(tag) {
            let Some(idx) = row
                .key
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=9).contains(n))
            else {
                continue;
            };
            term::println(&format!(
                "{}{}{}",
                Self::ROW_PREFIXES[idx - 1],
                self.afford(row.price),
                text::fill(row.text, &Self::row_fill_values(row))
            ));
        }
    }

    /// What a priced row's `#` placeholders are filled with.
    ///
    /// Sixteen of the eighteen priced rows hold exactly one `#`, the
    /// price. Two hold more, and the original discards the ones a row
    /// does not use -- writing more fill values than a template has
    /// `#`s is intentional and matches the original, not a port
    /// convenience.
    ///
    /// `mar` row 2, `#^7 руб.  Пиво(#з)`, always reads `Пиво(5з)`: the
    /// second `#` is filled from a literal `5` baked into the row, not
    /// from the price -- it only looks price-driven because the price
    /// also happens to be 5.
    ///
    /// `bmar`'s Кастет and Дубинка rows (`#^7 руб. Кастет(урон+2)` and
    /// `#^7 руб. Дубинка(урон+4), заменяет кастет`) each push an extra
    /// literal `5` that the original discards outright; it changes
    /// nothing on screen.
    ///
    /// `bmar` row 7 (`... ^6f^7 урон(#-#).`) advertises 20-30 damage,
    /// but the shot it sells actually rolls `20 + Random(10)`, i.e.
    /// 20..=29 -- the original's own off-by-one, reproduced
    /// deliberately.
    fn row_fill_values(row: &data::ShopEntry) -> Vec<i64> {
        let mut values = vec![row.displayed_price as i64];
        match (row.shop, row.key) {
            ("mar", "2") => values.push(5),           // 1000:ba5a, file 0xD32A
            ("bmar", "7") => values.extend([20, 30]), // 1000:c7a7, 1000:c7ab
            _ => {}
        }
        values
    }

    /// The [`IMM_ROWS`] belonging to `tag`, in image order, each gated
    /// by [`Game::imm_row_visible`].
    ///
    /// Assembly is the same three parts as [`Game::print_priced_rows`]:
    /// prefix, affordability colour digit, row text. The one `#` in the
    /// whole table -- `trn` row 3's `10^7  прокачать # качков опыта` --
    /// is filled from a value that happens to equal that row's own
    /// price.
    fn print_imm_rows(&self, tag: &str) {
        for row in IMM_ROWS.iter().filter(|r| r.shop == tag) {
            if self.imm_row_visible(row) {
                term::println(&self.render_imm_row(row));
            }
        }
    }

    /// One [`IMM_ROWS`] row as the original assembles it, markup and all.
    fn render_imm_row(&self, row: &ImmRow) -> String {
        format!(
            "{}{}{}",
            row.prefix,
            self.afford(row.price),
            text::fill(row.text, &[i64::from(row.price)])
        )
    }

    /// Whether an [`IMM_ROWS`] row is printed at all.
    ///
    /// The vet's two rows have no gate of their own -- the whole menu is
    /// skipped when the player is unhurt. The other seven are gated:
    ///
    /// | row | gate |
    /// |---|---|
    /// | `kl` 1 | none |
    /// | `kl` 2 | `district > 1` |
    /// | `trn` 1 | none |
    /// | `trn` 2 | none |
    /// | `trn` 3 | `district > 1` **and** `district * 10 - 3 > level` |
    /// | `trn` 4 | `district > 1` |
    /// | `trn` 5 | `district > 2` **and** `abs < district * 2` |
    ///
    /// `abs` is the armour the player trained rather than bought: it
    /// starts as the total armour byte, then has the armour that came
    /// from equipment subtracted back out --
    ///
    /// * `-1` for the Abibas suit ("Смягчает пинок на 1") when the
    ///   Adidas suit isn't also owned,
    /// * `-2` for the Adidas suit ("на 2"),
    /// * `-2` for the Кожанка jacket ("защиты ... на 2") when the
    ///   Крутая кожанка isn't also owned,
    /// * `-4` for the Крутая кожанка ("Броня +4").
    ///
    /// The lesser item's bonus is skipped when the better one is owned,
    /// the same pairing [`crate::character_sheet`]'s `armour_block`
    /// prints.
    ///
    /// A related check in the gym's train command uses the same
    /// trained-armour value against a different threshold --
    /// `(district - 2) * 10` there, `district * 2` here -- the two are
    /// deliberately not shared, since they are different numbers.
    ///
    /// **Byte arithmetic, and it can underflow, matching the original.**
    /// Armour 1 while wearing the Крутая кожанка computes `1 - 4 = 253`,
    /// not `-3`, which reads as *above* either threshold and blocks the
    /// row.
    pub(crate) fn trained_armour(&self) -> u8 {
        let mut abs = self.player.armor;
        if self.wear_suit_abibas && !self.wear_suit_adidas {
            abs = abs.wrapping_sub(1); // 1000:e3b8
        }
        if self.wear_suit_adidas {
            abs = abs.wrapping_sub(2); // 1000:e3c3
        }
        if self.wear_jacket && !self.wear_jacket_krutaya {
            abs = abs.wrapping_sub(2); // 1000:e3d6
        }
        if self.wear_jacket_krutaya {
            abs = abs.wrapping_sub(4); // 1000:e3e2
        }
        abs
    }

    pub(crate) fn imm_row_visible(&self, row: &ImmRow) -> bool {
        let district = i32::from(self.district);
        let level = i32::from(self.player.level);
        let abs = i32::from(self.trained_armour());
        match (row.shop, row.key) {
            ("kl", "2") => district > 1,
            ("trn", "3") => district > 1 && district * 10 - 3 > level,
            ("trn", "4") => district > 1,
            ("trn", "5") => district > 2 && abs < district * 2,
            _ => true,
        }
    }

    /// `pr`, the den intro. `Ты пришел в притон - ` is written without
    /// a newline, then exactly one district-keyed suffix completes the
    /// line.
    ///
    /// District 1 spends a `Random(6)` draw (`+3`) for the dorm number,
    /// so this branch is part of the RNG sequence.
    ///
    /// The remaining menu lines are [`Game::print_den_menu`].
    fn print_den_intro(&mut self) {
        term::print(den::EMITTED[0].1);
        match self.district {
            1 => {
                let n = self.rng.below(6) + 3;
                term::println(&text::fill(den::EMITTED[1].1, &[n as i64]));
            }
            2 => term::println(den::EMITTED[2].1),
            3 => term::println(den::EMITTED[3].1),
            4 => term::println(den::EMITTED[4].1),
            // District 5 (reachable once promotion caps out) prints
            // only the prefix here, with no suffix and no trailing
            // newline -- the four district blocks are independent, and
            // none of them matches 5.
            _ => {}
        }
    }

    /// The den's menu -- the rest of the lines
    /// [`Game::print_den_intro`] does not print.
    ///
    /// **They print ONCE, on entry**, never again on the loop's back
    /// edge.
    ///
    /// | # | command | gate |
    /// |---|---|---|
    /// | 5 | -- | blank line |
    /// | 6 | -- | errand one pending |
    /// | 7 | -- | errand two pending **and** cred >= 100 |
    /// | 8 | -- | threshold block #1 ([`Game::den_menu_reveal_hint`]) |
    /// | 9 | -- | blank line |
    /// | 10 | -- | unconditional |
    /// | 11 | `p` | always shown; dimmed unless the beer count is non-zero |
    /// | 12 | `r` | shown while the den loan is unpaid; dimmed unless cred >= 2 |
    /// | 13 | `hp` | errand one pending |
    /// | 14 | -- | unconditional |
    /// | 15 | `a` | threshold block #2 |
    /// | 16 | `d` | cred >= 100 **and** errand two not pending |
    ///
    /// Rows 11 and 12 share the prefix `Напиши ^`, then the colour digit,
    /// then the row text.
    fn print_den_menu(&self) {
        term::println("");
        if self.den_errand_1_pending {
            term::println(den::EMITTED[5].1);
        }
        if self.den_errand_2_pending && self.pontovost_street >= 0x64 {
            term::println(den::EMITTED[6].1);
        }
        if self.den_menu_reveal_hint() {
            term::println(den::EMITTED[7].1);
        }
        term::println("");
        term::println(den::EMITTED[8].1);
        // Row 11 always prints; only its colour depends on the beer
        // count.
        term::println(&format!(
            "Напиши ^{}p^7  чтобы угостить пацанов пивом",
            if self.player.beer_dl != 0 { "0" } else { "4" }
        ));
        if self.den_loan_credit != 0 {
            term::println(&format!(
                "Напиши ^{}r^7  чтобы занять 2 рубля",
                if self.pontovost_street >= 2 { "0" } else { "4" }
            ));
        }
        if self.den_errand_1_pending {
            term::println(den::EMITTED[9].1);
        }
        term::println(den::EMITTED[10].1);
        if self.den_menu_reveal_hint() {
            term::println(den::EMITTED[11].1);
        }
        if self.pontovost_street >= 0x64 && self.den_errand_2_pending {
            term::println(den::EMITTED[12].1);
        }
    }

    /// Threshold blocks **#1** (menu line 8) and **#2** (menu line 15,
    /// the `a` row).
    ///
    /// **These two are one predicate, and the `a` command's own check is
    /// a different one -- and neither implies the other.** With
    /// `k = level - (district-1)*10`, the menu's predicate is
    /// `5k - 25 + cred >= 40` and the `a` command's is `2k + cred >= 40`.
    /// So `k = 1, cred = 38` satisfies the command and not the menu: the
    /// reveal can succeed with no menu line ever having offered it.
    /// `k = 13, cred = 0` satisfies the menu and not the command: the
    /// menu offers `a` and using it fails in silence.
    ///
    /// All three thresholds share the same prefix check: they're
    /// skipped once both Dealers and Gym are already discovered.
    fn den_menu_reveal_hint(&self) -> bool {
        if self.places.is_found(Location::Dealers) && self.places.is_found(Location::Gym) {
            return false;
        }
        let level_in_district = i32::from(self.player.level) - (i32::from(self.district) - 1) * 10;
        (level_in_district - 5) * 5 + i32::from(self.pontovost_street) >= 0x28
    }

    /// One turn at a location's own prompt. Location keys are checked
    /// before the street's own commands, using the location's own input
    /// buffer -- which is why the vet's `h` (heal a jaw) and the
    /// street's `h` (drink a beer) can share a letter and mean different
    /// things.
    ///
    /// Only `w` leaves a location; `run` is not an exit here the way it
    /// is on the street. Anything else is ignored and the prompt
    /// repeats.
    ///
    /// ## The den's seven keys
    ///
    /// `p`, `r`, `hp`, `s`, `a`, `d`, `w`.
    ///
    /// `hp` only works while an errand is pending -- with none pending,
    /// the input is never even compared against it and falls through to
    /// the `s` check instead. `a` is gated the same way, behind its own
    /// threshold (see [`Game::den_reveal`]).
    ///
    /// An unrecognised key prints nothing.
    ///
    /// **Keys are not trimmed.** A leading space (e.g. ` p`) is a miss,
    /// matching the original.
    ///
    /// ## The gym's five keys
    ///
    /// `1`, `2`, `3`, `4`, `5`, and the shared `w`. Three of the five sit
    /// behind their own district gate. Keys are not trimmed here
    /// either.
    pub(crate) fn shop_turn(
        &mut self,
        loc: Location,
        line: &str,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        let key = line.to_lowercase();
        match (loc, key.as_str()) {
            // The vet's two keys. Neither sits behind a gate that skips its
            // own compare -- the arms carry their own preconditions
            // instead.
            (Location::Vet, k) if vet::key_dispatches(k) => vet::run_key(self, k),
            // The vet is the only location with a second exit key: both
            // `w` and `e` leave it. `e` here means leaving the vet, not
            // quitting the game -- that's a separate compare on the
            // street's own buffer.
            (Location::Vet, k) if vet::exits(k) => self.leave_shop(),
            (Location::Dealers, "x") => self.sell_junk(),
            (Location::Dealers, "wes") => return self.sell_items(lines),
            (Location::Den, "p") => self.den_beer(),
            (Location::Den, "r") => self.den_borrow(),
            (Location::Den, "hp") if self.den_errand_1_pending => {
                return self.den_beat_up(lines);
            }
            (Location::Den, "s") => self.den_regard(),
            (Location::Den, "a") => self.den_reveal(),
            (Location::Den, "d") => return self.den_job(lines),
            (Location::Gym, k) if gym::key_dispatches(self, k) => gym::run_key(self, k),
            // The club's three keys, gated the same way as the gym's -- a
            // district check before the key compare, with an
            // unrecognised key falling through to `w`.
            (Location::Club, k) if club::key_dispatches(self, k) => {
                return club::run_key(self, k, lines);
            }
            (Location::Market | Location::Dealers, k)
                if k.len() == 1 && k.chars().all(|c| c.is_ascii_digit()) =>
            {
                self.shop_action(k.chars().next().unwrap());
            }
            // The market's pickpocket (`t`), handled in [`crate::market`].
            // `bmar` has no equivalent.
            (Location::Market, "t") => return market::pickpocket(self, lines),
            _ => {
                // This is a shared exit key across locations, not a
                // den-specific one. `run` is a synonym for `w` on the
                // street, and folding both into `Command::Walk` lets `run`
                // also exit every shop, which the original does not
                // respond to there.
                if key == "w" {
                    self.location = Location::Street;
                    self.mode = Mode::Street;
                }
                // Everything else: ignored, prompt repeats.
            }
        }
        // The vet's back edge returns to a health check that can eject
        // the player (`crate::vet::loop_top`), not to the prompt -- the
        // only location that works this way.
        if loc == Location::Vet && self.location == Location::Vet {
            vet::loop_top(self);
        }
        Ok(())
    }

    /// `a` at the den prompt -- the hidden Dealers+Gym reveal.
    ///
    /// Fires only when Dealers and Gym are not **both** already known, and
    /// when `(level - (district-1)*10) * 2 + street cred >= 40`. Prints two
    /// lines and marks Dealers and Gym as discovered -- unconditionally,
    /// even when one of them was already set.
    fn den_reveal(&mut self) {
        if self.places.is_found(Location::Dealers) && self.places.is_found(Location::Gym) {
            return;
        }
        let level_in_district = i32::from(self.player.level) - (i32::from(self.district) - 1) * 10;
        if level_in_district * 2 + i32::from(self.pontovost_street) < 0x28 {
            return;
        }
        // dcf6/dcfb store before dd00/dd19 print; matched here even though
        // nothing reads either flag in between, so there is no observable
        // difference -- this is a port, not just a functional match.
        self.places.mark_found(Location::Dealers);
        self.places.mark_found(Location::Gym);
        term::println(den::EMITTED[21].1);
        term::println(den::EMITTED[22].1);
    }

    /// `p` at the den prompt -- treat the lads to beer (пиво). Costs one
    /// beer and raises street cred (понтовость) by 5, with a confirmation
    /// message printed after both changes. Refuses when the beer count is
    /// zero **or negative** (a signed check, not just `== 0`).
    ///
    /// Repeatable: nothing one-shot is consumed, and the arm spends no
    /// random roll.
    fn den_beer(&mut self) {
        if self.player.beer_dl <= 0 {
            term::println(den::EMITTED[15].1);
            return;
        }
        self.player.beer_dl -= 1;
        self.pontovost_street = self.pontovost_street.wrapping_add(5);
        term::println(den::EMITTED[14].1);
    }

    /// `r` at the den prompt -- borrow two roubles: money +2, street cred
    /// (понтовость) -2, and the loan credit -1.
    ///
    /// Two refusals, checked in this order: first the loan credit
    /// (`^6Ты уже всю мелочь выгреб!` when exhausted), then street cred
    /// (`^6Ты не можешь занять денег.` when too low). Swapping the order
    /// would show the wrong refusal to a player who is out of both.
    ///
    /// The loan credit starts at 5 and tops up once per walk while below
    /// `district * 10` ([`Game::den_loan_credit`]).
    fn den_borrow(&mut self) {
        if self.den_loan_credit == 0 {
            term::println(den::EMITTED[18].1);
            return;
        }
        if self.pontovost_street <= 0 {
            term::println(den::EMITTED[17].1);
            return;
        }
        self.player.money = self.player.money.wrapping_add(2_i16);
        self.pontovost_street = self.pontovost_street.wrapping_sub(2);
        self.den_loan_credit -= 1;
        term::println(den::EMITTED[16].1);
    }

    /// `hp` at the den prompt -- beat up the lout who leaned on one of the
    /// lads. Requires the errand to be active. Rolls an opponent whose
    /// class is capped at 7, so it can never be a cop (`Мент`), and prints
    /// `^6Это ` + the rank name + ` # уровня.`, filling in the rolled
    /// level. The errand is consumed once the fight returns, win or lose.
    ///
    /// Winning raises street cred by `district * 20` and awards
    /// `district * 10` xp, then runs a capped level-up drain that can
    /// spend random rolls.
    fn den_beat_up(
        &mut self,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        let enemy = self.roll_enemy(1);
        self.fight_accepted = true;
        term::print("^6Это ");
        term::print(&Self::rank_name(enemy.class));
        term::println(&text::fill(" # уровня.", &[enemy.level as i64]));
        self.run_combat(6, enemy, lines)?;
        self.den_errand_1_pending = false;
        Ok(())
    }

    /// `s` at the den prompt -- ask how the lads regard you.
    ///
    /// Always prints `^4Твоя понтовость сейчас = #.` with the current
    /// street cred. Prints a second line only when street cred is at or
    /// below `district * 10 + 10`.
    ///
    /// This option changes nothing -- it only reads and reports street
    /// cred.
    fn den_regard(&self) {
        term::println(&text::fill(
            den::EMITTED[19].1,
            &[i64::from(self.pontovost_street)],
        ));
        if i32::from(self.district) * 10 + 10 <= i32::from(self.pontovost_street) {
            term::println(den::EMITTED[20].1);
        }
    }

    /// The `d` arm's luck roll -- the same predicate evaluated twice:
    /// whether street luck (Удача) loses against `Random(district * 15)`.
    ///
    /// Удача is the fourth of four stats shown together (`Сл:^` /
    /// `#^7 Лв:^` / `#^7 Жв:^` / `#^7 Уд:^`), and losing a point of it
    /// prints `^4Удача -1 `.
    pub(crate) fn luck_below_random_32(luck: u16, random: u16) -> bool {
        // Only the luck stat can be negative; the random roll cannot.
        let luck_high: i16 = if (luck as i16) < 0 { -1 } else { 0 };
        let random_high: i16 = 0;
        if luck_high != random_high {
            return luck_high < random_high; // 1000:dda8 / 1000:ddac, signed
        }
        luck < random // 1000:ddb1 / 1000:ddf1, unsigned
    }

    /// `d` at the den prompt -- go on the job. The largest arm, and the
    /// only one with wide compares or draws.
    ///
    /// Two silent gates, in order: street cred must be at least 100, and
    /// the second errand must still be active. Failing either ends the
    /// command with nothing printed.
    ///
    /// Otherwise prints `^0Давай быстрее..` then
    /// `^2Ты пришел воровать деньги`, and rolls luck against
    /// `Random(district * 15)`:
    ///
    /// * Luck wins -> `^2Ты наваровал денег`, then money and хлам each
    ///   gain `district * 10 + Random(district * 10)`, and xp gains
    ///   `district * 12`, followed by a capped level-up drain.
    /// * Luck loses -> `^4Шухер менты!`, then a second
    ///   `Random(district * 15)` roll compared against luck again:
    ///   * loses again -> forced into a fight (`^6Пора валить!`);
    ///   * wins -> escapes with `^2Ты смылся от ментов.`.
    ///
    /// The second errand is consumed on every path past the two gates,
    /// including both cop outcomes.
    ///
    /// The cop fight does not receive the extra rewards the haul's fight
    /// does.
    fn den_job(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        if self.pontovost_street < 0x64 {
            return Ok(());
        }
        if !self.den_errand_2_pending {
            return Ok(());
        }
        term::println(den::EMITTED[23].1);
        term::println(den::EMITTED[24].1);
        let n15 = u16::from(self.district) * 15;
        let roll = self.rng.below(n15);
        if Self::luck_below_random_32(self.player.luck, roll) {
            term::println(den::EMITTED[25].1);
            let roll2 = self.rng.below(u16::from(self.district) * 15);
            if Self::luck_below_random_32(self.player.luck, roll2) {
                // This roll forces the enemy to be a `Мент` (class 8).
                let cop = self.roll_enemy(2);
                self.run_combat(5, cop, lines)?;
                term::println(den::EMITTED[26].1); // 1000:ddff, CS 0xa075
            } else {
                term::println(den::EMITTED[27].1); // 1000:de1a, CS 0xa084
            }
        } else {
            term::println(den::EMITTED[28].1);
            let base = u16::from(self.district) * 10;
            let cash = i32::from(base) + i32::from(self.rng.below(base));
            self.player.money = self.player.money.wrapping_add(cash as i16);
            let junk = base.wrapping_add(self.rng.below(base));
            self.player.junk = self.player.junk.wrapping_add(junk as i16);
            let xp = u16::from(self.district) * 12;
            term::println(&text::fill(den::EMITTED[29].1, &[i64::from(xp)]));
            progress::apply_levels(
                &mut self.progress,
                &mut self.player,
                &mut self.rng,
                xp,
                false,
            );
        }
        self.den_errand_2_pending = false;
        Ok(())
    }

    /// Everything [`crate::character_sheet::lines`] needs that is not a
    /// field of [`Fighter`], gathered off `self`.
    fn sheet_kit(&self) -> character_sheet::Kit {
        character_sheet::Kit {
            xp: self.progress.xp,
            threshold: self.progress.threshold,
            buff_countdown: self.buff_countdown,
            krestik: self.charm_krestik,
            ring_gs: self.charm_ring,
            ring_pg: self.oneshot_gift_1,
            mega_ring: self.oneshot_gift_2,
            ring_gp: self.ring_gospodi_pomilui,
            mobile: self.has_mobile,
            dark_glasses: self.dark_glasses,
            prison_tattoo: self.prison_tattoo,
            pistol: self.pistol,
            boots: self.wear_boots,
            boots_pontovye: self.wear_boots_pontovye,
            kastet: self.weapon_kastet,
            dubinka: self.weapon_dubinka,
            nozh: self.weapon_nozhik,
            tesak: self.weapon_tesak,
            tooth_guard: self.tooth_guard,
            suit_abibas: self.wear_suit_abibas,
            suit_adidas: self.wear_suit_adidas,
            jacket: self.wear_jacket,
            jacket_krutaya: self.wear_jacket_krutaya,
        }
    }

    /// `s` -- the character sheet.
    ///
    /// Reached from the street prompt, the fight prompt (`Битва\`), the
    /// quit sequence and the rector-victory ending. All four render the
    /// same sheet -- see [`Game::sheet_kit`].
    ///
    /// The lines are built by [`crate::character_sheet`] so a test can
    /// assert them; this method is only the print loop.
    fn show_stats(&self) {
        for line in character_sheet::lines(&self.player, &self.player.name, &self.sheet_kit()) {
            term::println(&line);
        }
    }

    /// `i` -- prints the list of available commands: one line always
    /// shown, then seven lines gated on whether the corresponding location
    /// has been discovered (checked in this order: Market, Dealers, Vet,
    /// Girl, Den, Club, Gym), then nine more lines always shown.
    ///
    /// The gate order is deliberate and does not match the order those
    /// discovery flags are stored in -- reordering it to match would
    /// reintroduce a Den/Vet swap this port once shipped.
    ///
    /// This command has no other effect: nothing is written, no random
    /// draw is spent, and no further input is read.
    fn show_command_list(&self) {
        // `Напиши: ^6w^7    чтобы шататься по окрестностям - искать на свою жопу приключения`
        term::println(COMMAND_LIST[0].1);
        // The seven gated lines, in the original's gate order. Each tuple is
        // (the flag the gate reads, the line the fall-through prints).
        for (loc, line) in [
            // `Напиши: ^6mar^7  чтобы идти на рынок`
            (Location::Market, COMMAND_LIST[1].1),
            // `Напиши: ^6bmar^7 чтобы идти к барыгам`
            (Location::Dealers, COMMAND_LIST[2].1),
            // `Напиши: ^6rep^7  чтобы идти к ветеринару`
            (Location::Vet, COMMAND_LIST[3].1),
            // `Напиши: ^6girl^7 чтобы завалиться к своей девчонке`
            (Location::Girl, COMMAND_LIST[4].1),
            // `Напиши: ^6pr^7   чтобы идти в местный притон гопоты`
            (Location::Den, COMMAND_LIST[5].1),
            // `Напиши: ^6kl^7   чтобы идти в клуб`
            (Location::Club, COMMAND_LIST[6].1),
            // `Напиши: ^6trn^7  чтобы идти в качалку`
            (Location::Gym, COMMAND_LIST[7].1),
        ] {
            if self.places.is_found(loc) {
                term::println(line);
            }
        }
        for line in [
            // `Напиши: ^6s^7    чтобы посмотреть в лужу на свою уродскую рожу`
            COMMAND_LIST[8].1,
            // `Напиши: ^6sv^7   чтобы приглядеться к пинаемому мудаку`
            COMMAND_LIST[9].1,
            // `Напиши: ^6k^7    чтобы гасить мудака который тебе попался на дороге`
            COMMAND_LIST[10].1,
            // `Напиши: ^6v^7    чтобы позвать подкрепление`
            COMMAND_LIST[11].1,
            // `Напиши: ^6kos^7  чтобы схавать косяк`
            COMMAND_LIST[12].1,
            // `Напиши: ^6h^7    чтобы выпить пиво (если не охото к ветеринару)`
            COMMAND_LIST[13].1,
            // `Напиши: ^6mh^7   чтобы набухаться до чёртиков`
            COMMAND_LIST[14].1,
            // `Напиши: ^6name^7 чтобы сменить погоняло`
            COMMAND_LIST[15].1,
            // `Напиши: ^6e^7    если захочешь выйти`
            COMMAND_LIST[16].1,
        ] {
            term::println(line);
        }
    }

    /// `help` -- a personalised tutorial: thirty-four lines, one branch,
    /// no randomness and no state changes.
    ///
    /// The groups interleave: lines built from fragments with the rank and
    /// player name filled in, then plain lines, then two lines filled with
    /// the class's strength growth weight, then more plain lines.
    ///
    /// [`crate::opening::HELP_DISTRICT_LINE`] is skipped while the district
    /// is 1.
    fn show_help(&self) {
        let rank = data::rank_name(self.player.class);
        let name = &self.player.name;
        let f = opening::HELP_FRAGMENTS;
        term::println(&format!("{}{rank}{}{name}{}", f[0], f[1], f[2]));
        term::println(opening::HELP_PLAIN[0]);
        term::println(&format!("{}{rank}{}", f[3], f[4]));
        let weight = progress::class_weights(self.player.class)[opening::HELP_WEIGHT_INDEX];
        for line in opening::HELP_WEIGHT_LINES {
            term::println(&text::fill(line, &[i64::from(weight)]));
        }
        for (i, line) in opening::HELP_PLAIN.iter().enumerate().skip(1) {
            if i == opening::HELP_DISTRICT_LINE && self.district <= 1 {
                continue;
            }
            term::println(line);
        }
    }

    /// `sv`. Shows the last-fought opponent's stat block --
    /// [`crate::enemy_sheet`] builds the lines, this prints them, the same
    /// split [`Game::show_stats`] uses for the player's sheet.
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
        for line in enemy_sheet::lines(enemy) {
            term::println(&line);
        }
    }

    /// `v` at the STREET prompt: the original does nothing at all, so
    /// neither does this.
    ///
    /// `v` matters only at the fight prompt -- see
    /// [`Game::backup_in_fight`]. This method used to print
    /// `^4Ни кто не хочет за тебя впрягаться.`, but that line belongs to
    /// the fight prompt's *cred too low* refusal, not the street.
    fn call_backup(&self) {}

    /// `f` at the STREET prompt.
    ///
    /// Prints `^6Ты чё псих? мигом менты накроют!` when carrying a pistol;
    /// otherwise the verb is accepted and answered with silence. Either way
    /// nothing else happens: no random draw, no state change, and the
    /// pistol is not fired -- gated on [`Game::pistol`].
    fn shoot(&self) {
        if self.pistol.owned {
            term::println("^6Ты чё псих? мигом менты накроют!");
        }
    }

    /// `w`/`run` -- one whole wander turn.
    ///
    /// A turn runs [`Game::wander_preamble`], then a bucket dispatch:
    ///
    /// * **0** -- nothing happens: [`wander::BUCKET4`]`[3]`
    ///   ("Ничё не происходит."). Reached only when the church has
    ///   zeroed the already-rolled bucket.
    /// * **1** -- toggles harder encounters ([`Game::harder_encounters`],
    ///   which changes the draw count and values of later encounters) and
    ///   writes one district-keyed line from [`wander::BUCKET1`] (four
    ///   "entered" lines, then four "left" lines; districts 1..4 only --
    ///   district 5 prints nothing in either half).
    /// * **2** -- the girl encounter, [`Game::wander_girl`].
    /// * **3** -- the fight encounter, below.
    /// * **4** -- flavour only, gated on the joint buff's countdown
    ///   (`self.player.stoned`); see [`Game::wander_flavor`].
    ///
    /// The fight encounter:
    ///
    /// * Rolls the opponent ([`Game::roll_enemy`]). A rolled `Мент` skips
    ///   everything below and goes straight to [`Game::cop_encounter`],
    ///   which asks no question at all.
    /// * Otherwise rolls a notice check (`Random(district * 7 + 15)`,
    ///   halved when [`Game::prison_tattoo`] is set) and compares it
    ///   against luck: luck lost -> the aggressive block (enemy class
    ///   threshold 3); luck won -> the quiet block (threshold 7).
    /// * Aggressive block: writes `^6Идет ` + the rank name +
    ///   ` # уровня, ищущий кого отпинать. Хочешь наехать?`. A non-`y`
    ///   answer still rolls a coin flip: it can still turn into a fight
    ///   (`^4Он тебя заметил.`) instead of an escape (`^2Ты смылся.`).
    /// * Quiet block: same shape, but ` # уровня. Хочешь наехать?`, and a
    ///   non-`y` answer always ends the turn -- no roll, no risk of still
    ///   being noticed.
    /// * `^4Эй мудак?!` belongs to a different, class-7 combat opener, not
    ///   this encounter.
    ///
    /// `pub` so tests can drive one turn at a time; `run()` is still the
    /// only path a player takes. Always the `w` spelling -- see
    /// [`Game::walk_verb`] for `run`'s own extra line.
    pub fn walk(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        self.walk_verb(false, lines)
    }

    /// [`Game::walk`]'s body, plus `ran`: whether the just-read line was
    /// literally `run` rather than `w`. `crate::commands::parse` folds both
    /// into one `Command::Walk`, so only `Game::run`'s Street-mode dispatch
    /// -- the one place the raw line is still in scope -- can tell them
    /// apart; see `crate::wander`'s module doc for why that check has to
    /// live inside the shared preamble rather than in `parse` or
    /// `dispatch`.
    fn walk_verb(
        &mut self,
        ran: bool,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        let bucket = self.wander_preamble(ran, lines)?;
        match bucket {
            1 => {
                self.harder_encounters = !self.harder_encounters;
                if (1..=4).contains(&self.district) {
                    let base = if self.harder_encounters { 0 } else { 4 };
                    term::println(wander::BUCKET1[base + usize::from(self.district - 1)]);
                }
                return Ok(());
            }
            2 => return self.wander_girl(lines),
            3 => {}
            4 => return self.wander_flavor(lines),
            _ => {
                term::println(wander::BUCKET4[3]);
                return Ok(());
            }
        }

        let enemy = self.roll_enemy(0);
        if enemy.class == 8 {
            return self.cop_encounter(enemy, lines);
        }

        let mut n = u16::from(self.district) * 7 + 15;
        if self.prison_tattoo {
            n /= 2;
        }
        let notice = self.rng.below(n);
        // The notice-vs-luck compare widens both sides via zero-extension,
        // unlike the original's mixed signed/unsigned compare. That only
        // differs at luck values near the top of a 16-bit range, which the
        // game never reaches -- a deliberate, harmless simplification.
        let aggressive = if i32::from(self.player.luck) < i32::from(notice) {
            enemy.class >= 3
        } else {
            enemy.class >= 7
        };
        term::print("^6Идет ");
        term::print(&enemy.name);
        term::println(&text::fill(
            if aggressive {
                " # уровня, ищущий кого отпинать. Хочешь наехать?"
            } else {
                " # уровня. Хочешь наехать?"
            },
            &[enemy.level as i64],
        ));
        let Some(line) = term::read_line(lines) else {
            self.running = false;
            return Ok(());
        };
        let answer = line?;
        if answer.eq_ignore_ascii_case("y") {
            self.run_combat(0, enemy, lines)?;
        } else if !aggressive {
        } else if self.rng.below(2) == 0 {
            term::println("^4Он тебя заметил.");
            self.run_combat(0, enemy, lines)?;
        } else {
            term::println("^2Ты смылся.");
        }
        Ok(())
    }

    /// What a rolled `Мент` (cop, class 8) does instead of the ordinary
    /// wander encounter.
    ///
    /// Writes `^6Идет ментяра # уровня гроза гопов.` with the rolled level.
    /// No line is read -- there is no "Хочешь наехать?" on this path.
    ///
    /// Rolls a notice check, `district * 7 + 15`, which unlike the ordinary
    /// encounter's roll is **never** halved by the tattoo. Luck against it
    /// decides the outcome:
    ///
    /// * Luck wins --
    ///   `^2Ты затаился, прикинулся не гопом... Мент вроде не заметил`,
    ///   no fight.
    /// * Luck loses but the player has тёмные очки -- still no fight.
    /// * Luck loses without them -- `^4Запалил!`, and the fight starts.
    fn cop_encounter(
        &mut self,
        enemy: Fighter,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        term::println(&text::fill(
            "^6Идет ментяра # уровня гроза гопов.",
            &[enemy.level as i64],
        ));
        let n = u16::from(self.district) * 7 + 15;
        let notice = self.rng.below(n);
        if i32::from(self.player.luck) >= i32::from(notice) {
            term::println("^2Ты затаился, прикинулся не гопом... Мент вроде не заметил");
            return Ok(());
        }
        if self.dark_glasses {
            term::println(
                "^2Ты напялил тёмные очки и мент не узнал твою рожу, которая весит на почётном",
            );
            term::println("^2стенде \"Разыскиваются за гопничество\"");
            return Ok(());
        }
        term::println("^4Запалил!");
        self.run_combat(0, enemy, lines)
    }

    /// Wander bucket 4 -- flavour only.
    ///
    /// Not stoned: prints `wander::BUCKET4[2]` ("Ничё не происходит.") and
    /// spends nothing.
    ///
    /// Stoned: two `Random(7)` draws, unconditionally.
    /// * A zero on the first additionally prints `wander::BUCKET4[0]`
    ///   first.
    /// * A non-zero on the second joins the not-stoned line
    ///   (`wander::BUCKET4[2]`) and ends the turn; a zero composes
    ///   `wander::BUCKET4_FRAGMENTS` around a rolled rank (`Random(7)`,
    ///   [`data::rank_name`]) and a rolled fill amount
    ///   (`Random(district * 10 + 1)`), reads one (unused) line, and prints
    ///   `wander::BUCKET4[1]`.
    fn wander_flavor(
        &mut self,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        if !self.player.stoned {
            term::println(wander::BUCKET4[2]);
            return Ok(());
        }
        if self.rng.below(7) == 0 {
            term::println(wander::BUCKET4[0]);
        }
        if self.rng.below(7) != 0 {
            term::println(wander::BUCKET4[2]);
            return Ok(());
        }
        let rank_roll = self.rng.below(7);
        let fill = self.rng.below(u16::from(self.district) * 10 + 1);
        term::print(wander::BUCKET4_FRAGMENTS[0]);
        term::print(data::rank_name(rank_roll));
        term::println(&text::fill(
            wander::BUCKET4_FRAGMENTS[1],
            &[i64::from(fill)],
        ));
        // The line read here is unused, but the read still happens (and
        // still ignores Ctrl+D), so the game pauses for input all the
        // same.
        let _ = term::read_line(lines);
        term::println(wander::BUCKET4[1]);
        Ok(())
    }

    /// Everything a walk does before the bucket dispatch, in execution
    /// order.
    ///
    /// Two shapes are easy to get wrong:
    ///
    /// * **Draws 1 and 2 are not one-shots.** Each keeps firing every turn
    ///   until its own 1-in-20 roll actually returns `0`; only then does it
    ///   stop for good. Steady state is nine draws per turn, decaying to
    ///   eight and then seven.
    /// * **Draws 5..8 always fire.** Only their *effect* is gated on the
    ///   corresponding discovery flag still being clear -- the roll itself
    ///   always happens.
    fn wander_preamble(
        &mut self,
        ran: bool,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<u8> {
        // The joint buff decays each turn; hitting zero takes back
        // exactly what it granted.
        if self.buff_countdown > 0 {
            self.buff_countdown -= 1;
            if self.buff_countdown == 0 {
                self.player.strength = self.player.strength.wrapping_sub(2);
                self.player.dmg_min = self.player.dmg_min.wrapping_sub(1);
                self.player.dmg_max = self.player.dmg_max.wrapping_sub(2);
                self.player.stoned = false;
                term::println("^6Глюки прошли. Сила -2.");
            }
        }

        // Re-checks whether the command was `run` (versus `w`), which is
        // used later in the turn.
        if ran {
            term::println(wander::RAN);
        }

        // The den's loan credit tops up once per walk while it is below
        // `district * 10`.
        if u16::from(self.den_loan_credit) < u16::from(self.district) * 10 {
            self.den_loan_credit += 1;
        }

        // The dealers' delivery counter increments once per walk; the
        // delivery itself triggers only on the turn it reaches exactly 25,
        // and only with a phone.
        if self.places.is_found(Location::Dealers)
            && self.pistol.owned
            && self.dealer_delivery_counter < 25
        {
            self.dealer_delivery_counter += 1;
            if self.dealer_delivery_counter == 25 && self.has_mobile {
                term::println(
                    "Телефон:^6Алё, ты где? Приходи, мы вещицу для тебя раздобыли.(Иди к барыгам)",
                );
            }
        }

        // Draw 1 -- a 1-in-20 chance to offer the `hp` errand. The
        // never-repeat flag is set before checking whether the player has
        // a phone, so a player without one loses the errand permanently
        // and sees nothing.
        if !self.den_errand_1_pending && self.rng.below(20) == 0 {
            self.den_errand_1_pending = true;
            if self.places.is_found(Location::Den) && self.has_mobile {
                term::print("Телефон:^6Алё,");
                term::print(&self.player.name);
                term::println("^6? ты где щас? Тут помощь нужна.(Иди в притон)");
            }
        }

        // Draw 2 -- one errand along, same shape; prints its message
        // only when понтовость is at least 100.
        if !self.den_errand_2_pending && self.rng.below(20) == 0 {
            self.den_errand_2_pending = true;
            if self.places.is_found(Location::Den)
                && self.pontovost_street >= 100
                && self.has_mobile
            {
                term::print("Телефон:^6Алё,");
                term::print(&self.player.name);
                term::println("^6? ты щас где? Базар есть.(Иди в притон)");
            }
        }

        // Draw 3, draw 4, and the two cooldown messages below all require
        // a phone; without one, all four are skipped.
        if self.has_mobile {
            // Draw 3 -- a 1-in-200 chance for the wrong-number gag, its
            // messages paced one keystroke at a time.
            if self.rng.below(200) == 0 {
                term::println("Телефон:^6Алё Вася?");
                term::read_key(lines); // 1000:b055
                term::print("^2Нет это ");
                term::print(&self.player.name);
                term::println(".");
                term::read_key(lines); // 1000:b092
                term::println("Телефон:^6А Васю можно?");
                term::read_key(lines); // 1000:b0b0
                term::println("^2Нет, он будет в больнице в ближайшие 2 месяца.");
            }
            // Draw 4 -- 1-in-100; prints only with a girl.
            if self.rng.below(100) == 0 && self.places.is_found(Location::Girl) {
                term::println("Телефон(Твоя пассия):^5Привет, это я. Зайдешь ко мне сегодня?");
                term::println("^2А ты: Безбазаров, жди.");
            }
            // The "it blew over" messages fire on the last turn of a
            // ban, and only once the den is known.
            if self.market_ban_countdown == 1 && self.places.is_found(Location::Den) {
                term::println(
                    "Телефон:^2Это ты там на базаре шухер наводил? Ну короче там менты свалили.",
                );
            }
            if self.club_ban_countdown == 1 && self.places.is_found(Location::Den) {
                term::println("Телефон:^2Ты че там, в клуб-та пойдёшь. Уже утряслось всё.");
            }
        }

        // Both cooldowns count down each turn, but each is guarded so a
        // cooldown already at zero does not wrap around.
        if self.market_ban_countdown > 0 {
            self.market_ban_countdown -= 1;
        }
        if self.club_ban_countdown > 0 {
            self.club_ban_countdown -= 1;
        }

        // The four discovery rolls: 1-in-10 for the vet, 1-in-10 for the
        // market, 1-in-100 for the club, 1-in-100 for the gym.
        if self.rng.below(10) == 0 && !self.places.is_found(Location::Vet) {
            self.places.mark_found(Location::Vet); // 1000:b196
            term::println("^1Ты спросил у прохожего где больница.");
        }
        if self.rng.below(10) == 0 && !self.places.is_found(Location::Market) {
            self.places.mark_found(Location::Market); // 1000:b1c8
            term::println("^1Ты нашел базар.");
        }
        if self.rng.below(100) == 0 && !self.places.is_found(Location::Club) {
            self.places.mark_found(Location::Club); // 1000:b1fa
            term::println("^1Ты увидел объявление \"Типа заходи в наш понтовый клуб\".");
        }
        if self.rng.below(100) == 0 && !self.places.is_found(Location::Gym) {
            self.places.mark_found(Location::Gym); // 1000:b22c
            term::println("^1На стене реклама \"Жизнь тяжела. Если не хочешь сдохнуть качайся!\".");
        }

        // Gated on wearing the ring `Господи помилуй`, which grants +3 HP
        // and a 5% chance to heal a fracture, exactly as its own
        // description advertises.
        if self.ring_gospodi_pomilui {
            // The heal is clamped to max HP.
            if self.player.hp < self.player.hpmax {
                self.player.hp += 3;
                if self.player.hp > self.player.hpmax {
                    self.player.hp = self.player.hpmax;
                }
            }
            // Draw 9 -- a 1-in-20 chance to heal a fracture. At most one
            // fracture clears per turn, the jaw first: the leg only heals
            // when the jaw is already intact.
            if self.rng.below(20) == 0 {
                if !self.player.broken_jaw && self.player.broken_leg {
                    self.player.broken_leg = false;
                    term::println("^2Твоя нога залечилась с Божей помощью.");
                }
                if self.player.broken_jaw {
                    self.player.broken_jaw = false;
                    term::println("^2Твоя челюсть залечилась с Божей помощью.");
                }
            }
        }

        // Each class's wander perk is applied here.
        match self.player.class {
            // Отморозок heals one scratch a walk -- the "Бонус -
            // Самолечение царапин" the creation menu advertises.
            4 => {
                if self.player.hp < self.player.hpmax {
                    self.player.hp += 1;
                }
            }
            // Гопник has no wander perk.
            5 => {}
            // Вор steals -- the menu's "Бонус - Воровство".
            6 => {
                // Draw 10 -- the roll's `n` is `district * 20`.
                let r = self.rng.below(u16::from(self.district) * 20);
                // The theft succeeds when luck >= result.
                if i32::from(self.player.luck) >= i32::from(r) {
                    let amount = self.rng.below(u16::from(self.district) * 5) + 1;
                    self.player.money = self.player.money.wrapping_add(amount as i16);
                    term::println(&text::fill(
                        "^2Опа бабки! # рублей на пиво!",
                        &[amount as i64],
                    ));
                }
            }
            _ => {}
        }

        // The bucket roll, 1..25, sets the wander encounter bucket.
        let roll = self.rng.below(25) + 1;
        let mut bucket = if roll >= 10 {
            4
        } else if roll >= 5 {
            3
        } else if roll >= 2 {
            2
        } else {
            1
        };

        // A 1-in-200 chance calls the church.
        if self.rng.below(200) == 0 {
            self.church(lines);
            // Every path through the church zeroes the bucket: a church
            // turn produces no further encounter even though the roll
            // already happened.
            bucket = 0;
        }

        // A 1-in-100 chance calls the mage.
        if self.rng.below(100) == 0 {
            self.mage(lines)?;
        }

        Ok(bucket)
    }

    /// Three sermon arms are selected by the church-visit stage (2, 1, or
    /// 0) and all converge afterward, so the gift roll always happens once
    /// the church fires. The two lower arms raise the stage on their way
    /// out, which is why it saturates at 2.
    ///
    /// The three sermons, their key-presses, the forced level-up's
    /// composed line, and the parting lines live in [`crate::church`].
    fn church(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) {
        let stage = self.church_visits;
        // The sermon reads the stage before it is raised.
        church::sermon(lines, stage, &self.player);
        if stage <= 1 {
            self.church_visits += 1;
        }

        // Five equally likely arms decide the church's gift.
        match self.rng.below(5) {
            // The zero arm forces a level-up.
            0 => {
                // The composed `Был ты X а стал Y` line reads the level
                // BEFORE the call below changes it, so the old level is
                // captured here.
                let level = self.player.level;
                opening::play(
                    lines,
                    &church::FORCED_LEVEL,
                    church::FORCED_LEVEL_GAPS,
                    Some(&church::forced_level_composed(level)),
                );
                // xp is set to the threshold, so the level-up runs exactly
                // once. At level 40 it grants nothing, but the xp rewrite
                // still happens.
                self.progress.xp = self.progress.threshold;
                progress::apply_levels(
                    &mut self.progress,
                    &mut self.player,
                    &mut self.rng,
                    0,
                    false,
                );
            }
            // A stat blessing.
            1 => match self.rng.below(4) {
                // dmg_min reads the ALREADY-incremented strength, so it
                // gains +1 when the new strength is even -- the same rule
                // as a level-up's.
                0 => {
                    term::println("^1Да увеличиться твоя сила!");
                    self.player.strength += 1;
                    self.player.hpmax += 1;
                    self.player.hp += 1;
                    self.player.dmg_max += 1;
                    if self.player.strength.is_multiple_of(2) {
                        self.player.dmg_min += 1;
                    }
                }
                1 => {
                    term::println("^1Да уменьшиться твоя корявость!");
                    self.player.agility += 1; // 1000:8067
                }
                2 => {
                    term::println("^1Да возрастут твой силы жизненные!");
                    self.player.vitality += 1;
                    self.player.hpmax += 5;
                    self.player.hp += 5;
                }
                _ => {
                    term::println("^1Да снизойдет на тебя удача!");
                    self.player.luck += 1; // 1000:80b9
                }
            },
            // The first unfired one-shot gift. These are the same three
            // flags the post-kill block grants -- this is a second grant
            // site.
            2 => {
                term::println("^1Дарю тебе феньку!");
                if !self.oneshot_gift_1 {
                    term::println("^1Кольцо \"Помоги Господи\"");
                    self.player.strength += 1;
                    self.player.agility += 1;
                    self.player.vitality += 1;
                    self.player.luck += 1;
                    self.player.hpmax += 6;
                    self.player.hp += 6;
                    self.player.dmg_max += 1;
                    if self.player.strength.is_multiple_of(2) {
                        self.player.dmg_min += 1; // 1000:811f..1000:8130
                    }
                    self.oneshot_gift_1 = true;
                } else if !self.oneshot_gift_2 {
                    term::println("^1\"Мега Кольцо\"! со своего, можно сказать, пальца");
                    self.player.strength += 4;
                    self.player.agility += 4;
                    self.player.vitality += 4;
                    self.player.luck += 4;
                    self.player.hpmax += 24;
                    self.player.hp += 24;
                    self.player.dmg_max += 4;
                    self.player.dmg_min += 2;
                    self.oneshot_gift_2 = true;
                } else if !self.ring_gospodi_pomilui {
                    // Text only here -- the ring's effect (the wander hp
                    // regen and fracture-heal chance) is applied above.
                    term::println("^1Ваще полезное кольцо \"Господи помилуй\"");
                    term::println("^1Восст. жизни - 3, 5% - самозарост переломов");
                    self.ring_gospodi_pomilui = true;
                }
            }
            // Increments ARMOUR, printed as `^2Броня #` -- "накладываю защиту" is exactly
            // that.
            3 => {
                term::println("^1Накладываю на тебя защиту!");
                self.player.armor = self.player.armor.wrapping_add(1);
            }
            _ => {
                term::println("^1Да увеличится, офигенно, твоя понтовость среди гопоты!");
                let gain = i16::from(self.district) * 50 + 50;
                self.pontovost_street = self.pontovost_street.wrapping_add(gain);
                term::println(&text::fill("^1Получи #!", &[i64::from(gain)]));
            }
        }

        // Every arm converges on the same key-press prompt.
        term::read_key(lines);
        // Reads the stage AFTER it was raised. Only one of the two parting
        // lines ever prints, which is why `PARTING` is indexed here rather
        // than played through `opening::play`.
        term::println(church::PARTING[usize::from(self.church_visits >= 2)]);
        // A blank line separates the two parting lines; the bucket is zeroed between
        // them, as noted at the call site.
        term::println("");
        term::println(church::PARTING[2]);
    }

    /// The wandering mage Рушель Блаво. Spends no random draw of its own,
    /// but blocks on a line of input.
    ///
    /// **A divergence in the original, reproduced here.** The price it
    /// PRINTS is `district * 25`; the price it CHECKS and CHARGES is
    /// `district * 50`.
    ///
    /// On the paid path the game is saved and discovery progress written,
    /// printing `^0Сохранено! ^1Можешь беспредельничать дальше.` Money is
    /// spent either way, even if the save write fails; a failed save is
    /// reported to the player and the turn continues.
    pub fn mage(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        term::println(MAGE_LINES[0]);
        term::read_key(lines); // 1000:7560
        term::println(MAGE_LINES[1]);
        term::read_key(lines); // 1000:757e
        term::println(&text::fill(MAGE_LINES[2], &[i64::from(self.district) * 25]));
        term::read_key(lines); // 1000:75a4
        term::println(MAGE_LINES[3]);
        let Some(line) = term::read_line(lines) else {
            self.running = false;
            return Ok(());
        };
        let answer = line?;
        if !answer.eq_ignore_ascii_case("y") {
            term::println("^6Нехотите как хотите - мое дело предложить");
            return Ok(());
        }
        let price = i32::from(self.district) * 50;
        if i32::from(self.player.money) < price {
            term::println("^6Парень, все стоит бабок!");
            return Ok(());
        }
        self.player.money = self.player.money.wrapping_sub(price as i16);
        // The debit happens before the save is attempted, so money is
        // spent even if the write fails.
        if let Err(e) = self.mage_save() {
            term::println(&format!("^6{e}"));
        }
        Ok(())
    }

    /// Wander bucket 2 -- the girl discovery event.
    ///
    /// If already found, prints "Совсем ничё не происходит." and ends the
    /// turn. Otherwise asks "^5Идет типа клёвая цыпа. Хочешь её зацепить?"
    ///
    /// The answer is case-insensitive; anything but "y" ends the turn with
    /// no message, unlike the fight encounter's decline. On "y", a 50/50
    /// roll decides it: success prints "^5Ты такой подкатываешь, а она:
    /// "Глянулся ты мне парниша"" and marks the girl found; failure prints
    /// "^4Ты ещё подкатить неуспел - а она:"Отдыхай урод". - Тебя обломали
    /// кент" and leaves her unfound. The roll only happens when the player
    /// answers "y".
    fn wander_girl(
        &mut self,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        if self.places.is_found(Location::Girl) {
            term::println("Совсем ничё не происходит.");
            return Ok(());
        }
        term::println("^5Идет типа клёвая цыпа. Хочешь её зацепить?");
        let Some(line) = term::read_line(lines) else {
            self.running = false;
            return Ok(());
        };
        let answer = line?;
        if !answer.eq_ignore_ascii_case("y") {
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

    /// Rounds a value that is an exact multiple of one half, taking that
    /// value **doubled** so the caller needs no float. It rounds
    /// half-away-from-zero rather than half-to-even, so `Round(1.5)` is 2 --
    /// a distinction that is load-bearing for one `level * 1.5` computation
    /// elsewhere.
    fn round_half(twice: i32) -> i32 {
        if twice >= 0 {
            (twice + 1) / 2
        } else {
            -((-twice + 1) / 2)
        }
    }

    /// The random-encounter opponent's stats, used by the wander encounter
    /// (`param_1 == 0`) and by other callers that pass `param_1 == 1`
    /// (clamps the class to 7) or `param_1 == 2` (forces class 8).
    ///
    /// 1. Base class: `Random(0x33) + 1`, folded by a triangular walk that
    ///    maps low rolls to high classes (class 8 for a roll of 0..1, class
    ///    0 for 44..50).
    /// 2. `class += Random(district)`.
    /// 3. `class += Random(4)`, but only when harder encounters are on.
    /// 4. Clamp `class` to 9, then apply the `param_1` clamp above.
    /// 5. крутизна: `Round(level * f / d + s - 2) + 4 * Random(district)`,
    ///    where `s = Random(5)`, `f = Random(2) + 1` and `d = Random(2) + 1`.
    ///    Floored at 0, then multiplied by 1.5 when harder encounters are on.
    /// 6. The four stats start at 0.
    /// 7. `sum(weights) + крутизна * 2` points are distributed one at a
    ///    time by a weighted random draw against the rolled class's weight
    ///    row (see `crate::progress::CLASS_WEIGHTS`). Both the weight sum
    ///    and the point count wrap at 256, as they did in the original.
    /// 8. `dmg_min = strength / 2`, `dmg_max = strength`,
    ///    `hpmax = vitality * 5 + strength + 10`, `hp = hpmax`.
    /// 9. Loot, from `k = крутизна / 2 + Round(class * крутизна / 5)`:
    ///    Хлам is `Random(6) + 2 * Random(k) - k`, money is
    ///    `Random(6) + Random(k) - k / 2`, both floored at 0.
    /// 10. Beer: `Random(2) + крутизна / 10 + 1` half-litres.
    /// 11. Armour: `Random(b) + b` stored as a byte, where
    ///     `b = 2 * (district - 1)^2`. District 1 always draws `Random(0)`,
    ///     consuming the roll even though the result is always 0.
    pub(crate) fn roll_enemy(&mut self, param_1: u8) -> Fighter {
        let mut cls = i32::from(self.rng.below(0x33)) + 1;
        for i in 1..=10 {
            if cls - i < 0 {
                cls = 10 - i;
                break;
            }
            cls -= i;
        }
        cls += i32::from(self.rng.below(u16::from(self.district)));
        if self.harder_encounters {
            cls += i32::from(self.rng.below(4));
        }
        if cls > 9 {
            cls = 9;
        }
        if param_1 == 1 && cls > 7 {
            cls = 7;
        }
        if param_1 == 2 {
            cls = 8;
        }
        let class = cls as u16;

        let district_bonus = 4 * i32::from(self.rng.below(u16::from(self.district)));
        let spread = i32::from(self.rng.below(5));
        let divisor = i32::from(self.rng.below(2)) + 1;
        let factor = i32::from(self.rng.below(2)) + 1;
        // `divisor` is 1 or 2 and the numerator is doubled first, so this is
        // the real quotient exactly, with no rounding of its own.
        let twice = i32::from(self.player.level) * factor * 2 / divisor + 2 * (spread - 2);
        let mut ponty = Self::round_half(twice) + district_bonus;
        if ponty < 0 {
            ponty = 0;
        }
        if self.harder_encounters {
            ponty = Self::round_half(ponty * 3);
        }

        let weights = progress::class_weights(class);
        let sum = weights.iter().sum::<u16>() & 0xff;
        let points = ((i32::from(sum) + 2 * ponty) & 0xff) as u16;
        let mut stats = [0u16; 4]; // strength, agility, vitality, luck
        for _ in 0..points {
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

        let k = ponty / 2 + (2 * i32::from(class) * ponty + 5) / 10;
        let junk_bonus = i32::from(self.rng.below(6));
        let junk_roll = i32::from(self.rng.below(k as u16));
        let junk = (junk_bonus + 2 * junk_roll - k).max(0);
        let money_bonus = i32::from(self.rng.below(6));
        let money_roll = i32::from(self.rng.below(k as u16));
        let money = (money_bonus + money_roll - k / 2).max(0);
        let beer_dl = i32::from(self.rng.below(2)) + ponty / 10 + 1;
        let armour_base = 2 * (i32::from(self.district) - 1).pow(2);
        let armor = (i32::from(self.rng.below(armour_base as u16)) + armour_base) & 0xff;

        // This lookup is total for every class the clamps above can leave.
        // A nameless fighter must never reach the player, so a missing row
        // is a data bug, not something to paper over with "".
        let name = data::enemies()
            .iter()
            .find(|e| e.class == class)
            .map(|e| e.name.to_string())
            .unwrap_or_else(|| panic!("data/enemies.json has no row for rolled class {class}"));
        Fighter {
            name,
            class,
            level: ponty as u16,
            hp: hpmax,
            hpmax,
            strength,
            agility,
            vitality,
            luck,
            armor: armor as u8,
            dmg_min: strength / 2,
            dmg_max: strength,
            beer_dl: beer_dl as i16,
            money: money as i16,
            junk: junk as i16,
            ..Fighter::default()
        }
    }

    /// Prints "^2Звали тебя:^7 " then the current name, then asks
    /// "^2А теперь будут:^7 " for the new one.
    ///
    /// An empty line substitutes "Раз^6дол^4бай" -- the same substitution
    /// character creation uses. The test is on the line's raw length, not
    /// its trimmed content: a line of only spaces counts as non-empty and
    /// is kept verbatim, not substituted. Do not `.trim()` the line before
    /// this check.
    fn rename(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) -> io::Result<()> {
        term::print("^2Звали тебя:^7 ");
        term::println(&self.player.name);
        term::print("^2А теперь будут:^7 ");
        let Some(line) = term::read_line(lines) else {
            self.running = false;
            return Ok(());
        };
        let n = line?;
        // Tests the line's raw length, not its trimmed content: a line of
        // only spaces is kept verbatim; only a genuinely empty line
        // triggers the substitution below. Do not `.trim()` `n` before this
        // check.
        let n = if n.is_empty() {
            // The same substitution literal character creation uses.
            "Раз^6дол^4бай".to_string()
        } else {
            n
        };
        // After the substitution, the name is rebuilt with a leading
        // `^7 `, exactly as character creation does. The prefix lives in
        // the name, not at the save boundary; `crate::persist::NAME_PREFIX`
        // is the literal.
        self.player.name = format!("{}{n}", crate::persist::NAME_PREFIX);
        Ok(())
    }

    /// `kos`, the joint. The game has **two** copies of this handler; see
    /// [`Joint`] for the difference between them. This doc traces the
    /// street-prompt copy:
    ///
    /// * broken jaw -> `^4Ты не схавать` ... .
    /// * already stoned -> `^6Ты неможешь` ... .
    /// * no joints -> `^4У тебя нет косяков`.
    /// * otherwise **exactly one** joint is spent: the stoned countdown is
    ///   set to 10, strength += 2, `dmg_min` += 1, `dmg_max` += 2, and a
    ///   flat **+10** heal capped at `hpmax`, then `^2Сила +2.`
    ///   The heal message splits like the beer routine's: when the
    ///   shortfall is under 10 it writes `^2Колёса прибавляют #з. ` (no
    ///   newline) then `^2Здоровья:#/#. Осталось # косяков`; otherwise the
    ///   single combined line (`^2Колёса прибавляют` ...), whose
    ///   "косякова" typo is the original's.
    ///
    /// `crate::model::Fighter` has a `stoned: bool`, not the original's
    /// countdown, so the flag is modelled as "stoned or not" and the
    /// countdown itself lives in [`Game::buff_countdown`].
    fn smoke(&mut self, site: Joint) {
        if self.player.broken_jaw {
            term::println(crate::gym::EMITTED[21].1);
            return;
        }
        if self.player.stoned {
            term::println(crate::gym::EMITTED[27].1);
            return;
        }
        if self.player.joints <= 0 {
            term::println(crate::gym::EMITTED[26].1);
            return;
        }
        self.player.joints -= 1;
        // The countdown is set to 10 at the street prompt, but only 3
        // inside a fight. The wander preamble decays it and clears the
        // buff at zero; `Fighter::stoned` mirrors that as a bool.
        self.player.stoned = true;
        self.buff_countdown = site.buff_turns();
        self.player.strength += 2;
        self.player.dmg_min += 1;
        self.player.dmg_max += 2;
        // If hp ever exceeds hpmax, the printed shortfall differs from the
        // original: the original would print a negative number here, this
        // port prints 0.
        let shortfall = self.player.hpmax.saturating_sub(self.player.hp);
        if shortfall < 10 {
            term::print(&text::fill(crate::gym::EMITTED[22].1, &[shortfall as i64]));
            self.player.hp = self.player.hpmax;
            term::println(&text::fill(
                crate::gym::EMITTED[23].1,
                &[
                    self.player.hp as i64,
                    self.player.hpmax as i64,
                    self.player.joints as i64,
                ],
            ));
        } else {
            self.player.hp += 10;
            term::println(&text::fill(
                site.long_heal_line(),
                &[
                    10,
                    self.player.hp as i64,
                    self.player.hpmax as i64,
                    self.player.joints as i64,
                ],
            ));
        }
        term::println(crate::gym::EMITTED[25].1);
    }

    /// `h` (one 0.5-litre unit) or `mh` (drink until full or dry). Both
    /// verbs share the same routine, which is why beer works inside a
    /// fight too.
    ///
    /// * Broken jaw -> refusal message, and the refusal falls through into
    ///   the `mh` tail rather than returning.
    /// * Already at full hp -> a message and a return, so this arm never
    ///   reaches the tail.
    /// * No beer -> `h` writes a "no beer" message.
    /// * Otherwise, one half-litre is spent before any message. When the
    ///   shortfall is under 5, hp tops up to `hpmax`; otherwise hp += 5.
    /// * `h` stops after that one unit; `mh` loops until full or dry.
    /// * `mh`'s tail summarizes the total healed, adding a "beer's gone"
    ///   line when that drank the last of it, or a "no beer" line when
    ///   nothing was drunk **and** the beer is already gone.
    ///
    /// The `#.#л.` figure in these messages is litres and tenths of beer
    /// remaining.
    fn beer(&mut self, how: Beer) {
        let single = how == Beer::One;
        // hp is snapshotted before the jaw gate, which is what lets the
        // refusal path still mean "nothing was drunk".
        let hp0 = self.player.hp;
        // Missed: the refusal prints, and this arm falls through into
        // the `mh` tail below rather than returning.
        if self.player.broken_jaw {
            term::println("^4Ты не можешь пить пиво из-за сломаной челюсти.");
        } else {
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
                // Spends the half-litre before any message.
                self.player.beer_dl -= 1;
                let shortfall = self.player.hpmax - self.player.hp;
                // Sizes the drink against a shortfall of 5.
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
        }
        // Everything below is `mh`-only.
        if single {
            return;
        }
        // The subtraction makes the summary's first field the TOTAL healed,
        // not just the last unit's gain.
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
            // Writes the second of the two `^4Пива нету` messages.
        } else if self.player.beer_dl == 0 {
            term::println("^4Пива нету");
        }
    }

    /// `hp`, `hpmax`, litres, tenths -- the four trailing `#`s of the beer
    /// messages.
    fn beer_numbers(&self) -> [i64; 4] {
        [
            self.player.hp as i64,
            self.player.hpmax as i64,
            (self.player.beer_dl / 2) as i64,
            i64::from(self.player.beer_dl % 2) * 5,
        ]
    }

    /// `x` at the dealers -- sell the Хлам.
    ///
    /// **There is no rate.** The whole Хлам amount becomes money, unscaled.
    ///
    /// Junk isn't always zero: [`Game::claim_spoils`] can credit it to the
    /// winner after a fight.
    fn sell_junk(&mut self) {
        if self.player.junk <= 0 {
            term::println("^4Тебе нечего спихнуть.");
            return;
        }
        self.player.money = self.player.money.wrapping_add(self.player.junk);
        self.player.junk = 0;
        term::println("^6Барыги дали тебе денег за хлам.");
    }

    /// The middle every one of the six `wes` arms shares: the prompt, the
    /// `ReadLn`, the refund roll and the `y` compare.
    ///
    /// **The refund roll happens even when the player declines** -- it is
    /// drawn before the yes/no answer is even checked.
    ///
    /// `None` is EOF on the read, which ends the session, as every other
    /// line read in this file does.
    fn sell_offer(
        rng: &mut Rng,
        base: u16,
        span: u16,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<Option<(bool, i32)>> {
        term::print("^0Продать вещи\\");
        let Some(line) = term::read_line(lines) else {
            return Ok(None);
        };
        let answer = line?;
        // The roll, AFTER the read and BEFORE the compare.
        let refund = i32::from(base + rng.below(span));
        // The fold is ASCII-only, so `Y` sells; a line like `" y"` (with a
        // leading space) misses.
        Ok(Some((answer.eq_ignore_ascii_case("y"), refund)))
    }

    /// `wes` at the dealers -- six sequential offers (the junk-sell miss
    /// branch is not a seventh).
    ///
    /// The gate is `own && (any strictly better rung owned)` -- an
    /// own-plus-REPLACEMENT pairing, not own-plus-equipped; there is no
    /// separate equipped flag, only ownership. тесак is never sellable:
    /// only the loot arm ever grants it.
    ///
    /// **The arms are not exclusive.** One `wes` can sell up to six items
    /// and read up to six lines.
    ///
    /// **Nothing is unwound.** Selling an item does not remove its stat
    /// bonus, and does not clear the better item's flag. This is
    /// intentional, reproduced as the game does it.
    ///
    /// **The refund is the arm's own pair of immediates, never the buy
    /// price** -- 8+R(5), 8+R(5), 13+R(8), 13+R(8), 25+R(15), 38+R(23).
    fn sell_items(
        &mut self,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        let mut offered = false;

        // Arm 1, костюм Abibas. Both gate misses are silent.
        if self.wear_suit_abibas && self.wear_suit_adidas {
            offered = true;
            term::println("^2У тебя есть ненужный костюм хочешь продать?");
            let Some((yes, refund)) = Self::sell_offer(&mut self.rng, 8, 5, lines)? else {
                self.running = false;
                return Ok(());
            };
            // Declining just moves on to the next arm.
            if yes {
                self.wear_suit_abibas = false; // 1000:cf74
                self.player.money = self.player.money.wrapping_add(refund as i16); // 1000:cf79 / 1000:cf7c / 1000:cf7d
                term::println(&text::fill(
                    "^2Ты продал костюм за #.",
                    &[i64::from(refund)],
                ));
            }
        }

        // Arm 2, Бутсы.
        if self.wear_boots && self.wear_boots_pontovye {
            offered = true;
            term::println("^2У тебя есть ненужные кроссовки хочешь продать?");
            let Some((yes, refund)) = Self::sell_offer(&mut self.rng, 8, 5, lines)? else {
                self.running = false;
                return Ok(());
            };
            if yes {
                self.wear_boots = false; // 1000:d029
                self.player.money = self.player.money.wrapping_add(refund as i16); // 1000:d032
                term::println(&text::fill(
                    "^2Ты продал кроссовки за #.",
                    &[i64::from(refund)],
                ));
            }
        }

        // Arm 3, Кожанка.
        if self.wear_jacket && self.wear_jacket_krutaya {
            offered = true;
            term::println("^2У тебя есть ненужная кожанка хочешь продать?");
            let Some((yes, refund)) = Self::sell_offer(&mut self.rng, 13, 8, lines)? else {
                self.running = false;
                return Ok(());
            };
            if yes {
                self.wear_jacket = false; // 1000:d0de
                self.player.money = self.player.money.wrapping_add(refund as i16); // 1000:d0e7
                term::println(&text::fill(
                    "^2Ты продал кожанку за #.",
                    &[i64::from(refund)],
                ));
            }
        }

        // Arm 4, Кастет.
        if self.weapon_kastet && (self.weapon_dubinka || self.weapon_nozhik || self.weapon_tesak) {
            offered = true;
            term::println("^2У тебя есть кастет, а это отстой хочешь продать?");
            let Some((yes, refund)) = Self::sell_offer(&mut self.rng, 13, 8, lines)? else {
                self.running = false;
                return Ok(());
            };
            if yes {
                self.weapon_kastet = false; // 1000:d1a1
                self.player.money = self.player.money.wrapping_add(refund as i16); // 1000:d1aa
                term::println(&text::fill(
                    "^2Ты продал кастет за #.",
                    &[i64::from(refund)],
                ));
            }
        }

        // Arm 5, Дубинка.
        if self.weapon_dubinka && (self.weapon_nozhik || self.weapon_tesak) {
            offered = true;
            term::println("^2У тебя есть дубинка - барахло - хочешь продать?");
            let Some((yes, refund)) = Self::sell_offer(&mut self.rng, 25, 15, lines)? else {
                self.running = false;
                return Ok(());
            };
            if yes {
                self.weapon_dubinka = false; // 1000:d25d
                self.player.money = self.player.money.wrapping_add(refund as i16); // 1000:d266
                term::println(&text::fill(
                    "^2Ты продал дубинку за #.",
                    &[i64::from(refund)],
                ));
            }
        }

        // Arm 6, ножик.
        if self.weapon_nozhik && self.weapon_tesak {
            offered = true;
            // The e in тeсак is a Latin e in the game's own text; kept
            // verbatim.
            term::println("^2У тебя есть ножик и тeсак, хочешь продать ножик?");
            let Some((yes, refund)) = Self::sell_offer(&mut self.rng, 38, 23, lines)? else {
                self.running = false;
                return Ok(());
            };
            if yes {
                self.weapon_nozhik = false; // 1000:d312
                self.player.money = self.player.money.wrapping_add(refund as i16); // 1000:d31b
                term::println(&text::fill("^2Ты продал ножик за #.", &[i64::from(refund)]));
            }
        }

        // The line means "nothing was OFFERED", not "nothing was sold": declining
        // every offer leaves the scratch byte holding that arm's roll and suppresses
        // it.
        if !offered {
            term::println("^6У тебя нет неужных вещей.");
        }

        // Answering `w` to a sell offer does NOT leave the dealers -- the answer is
        // consumed by [`Game::sell_offer`] and never reaches `Game::shop_turn`'s exit
        // arm, so `self.mode` is untouched on every path out of here.
        Ok(())
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

    /// Buy row `key` (a shop-row digit `'1'..'9'`) at the current market. Only
    /// `Market`/`Dealers` have a row table.
    ///
    /// ## At the dealers the district gate is a MENU gate, not a buy gate
    ///
    /// The `bmar` handler's five district-gate checks sit in the menu-print block,
    /// deciding which lines are LISTED. The arms reached when the player types a
    /// key carry no district test at all. Typing `5` at district 1 buys the Кастет
    /// off a menu that never listed it.
    ///
    /// So [`Game::print_priced_rows`] keeps its gate and the buy path below drops
    /// it -- **for `bmar` only**.
    ///
    /// ## At the market it is BOTH, for three rows out of four
    ///
    /// Symmetry with `bmar` would have been the wrong answer here: `mar`'s buy path
    /// DOES gate three rows -- 6, 8 and 9 -- on district, so those three really are
    /// unbuyable below their district. Each prints **nothing**: the gate sits ahead
    /// of the row's key compare, so the line falls through to the handler's own
    /// re-prompt exactly as an unrecognised key does.
    ///
    /// **Row 7 is menu-gated and NOT buy-gated -- a divergence reproduced, not
    /// fixed.** The menu gate covers rows 6 and 7 together, while the buy path
    /// gates only row 6. So at district 1 the market lists rows 1-5, typing `6`
    /// prints not a word, and typing `7` buys the adidas suit for 30 руб. and
    /// applies its armour. [`Game::buy_market_row`] deliberately does not consult
    /// `row.gate` at all -- it carries the three buy-path checks itself.
    fn shop_action(&mut self, k: char) {
        let tag = match self.location {
            Location::Market => "mar",
            Location::Dealers => "bmar",
            _ => return,
        };
        let key = k.to_string();
        let Some(row) = data::shops().iter().find(|r| r.shop == tag && r.key == key) else {
            return;
        };
        // Every row of both shops has an arm of its own -- its own gates, its own
        // refusal lines, its own confirmation and its own effect. See
        // [`Game::buy_dealer_row`] and [`Game::buy_market_row`].
        let handled = match tag {
            "mar" => self.buy_market_row(row.key, row.price),
            _ => self.buy_dealer_row(row.key, row.price),
        };
        debug_assert!(handled, "{tag} row {} has no purchase arm", row.key);
    }

    /// The shape all **eighteen** purchase arms share -- `bmar` rows 1..9
    /// ([`Game::buy_dealer_row`]) and `mar` rows 1..9 ([`Game::buy_market_row`]):
    /// the prerequisite / better-item gate where the row has one, then the
    /// already-own gate, then affordability, then the debit, then the effect.
    ///
    /// `gates` is `(refuse, line)` in order. `line` is `None` for a gate that
    /// prints nothing at all -- true for five of the eighteen: `bmar` row 9's first
    /// two gates, and `mar`'s three district gates.
    ///
    /// The money test is last in every one of the eighteen arms: the sale goes
    /// through when `price <= money`, and refusal is the fall-through -- only the
    /// wording differs between rows.
    ///
    /// **The write-before-debit order is not observable, in either shop.** At `mar`
    /// rows 3-9 all set their ownership flag ahead of the debit, and rows 1 and 2
    /// write no flag at all; none of the seven reads the money or the byte it just
    /// wrote between the write and the debit, and the three upgrade guards each
    /// read a different flag from the one their own arm sets. So the effect closure
    /// runs after the debit for all eighteen.
    ///
    /// At `bmar`, only rows 1 and 3 debit before they write anything else; the
    /// other seven write first. Six of the seven set an ownership flag ahead of the
    /// debit; the seventh, row 8, increments a COUNT instead of a flag, ahead of
    /// its debit, and row 7 adds to that same count before its own debit too. In
    /// every one of the seven nothing between the write and the debit reads the
    /// money or the thing just written, so the effect closure runs after the debit
    /// here as well.
    fn buy_after_gates(
        &mut self,
        price: i32,
        gates: &[(bool, Option<&str>)],
        too_poor: &str,
        effect: impl FnOnce(&mut Self),
    ) {
        for (refuse, line) in gates {
            if *refuse {
                if let Some(line) = line {
                    term::println(line);
                }
                return;
            }
        }
        if i32::from(self.player.money) < price {
            term::println(too_poor);
            return;
        }
        self.player.money = self.player.money.wrapping_sub(price as i16);
        effect(self);
    }

    /// The dealers' nine purchase arms -- `bmar` rows 1..9. Returns `true` when the
    /// key was one of the nine and the arm has run. A `false` here means a row with
    /// no arm, which [`Game::shop_action`]'s `debug_assert!` catches in debug.
    ///
    /// Each row compares the same one-character buffer -- read after the
    /// `^0Барыги\` prompt -- against its own key; a miss falls to the next row's
    /// compare, so the nine are a chain of independent `if`s over one buffer, not
    /// an `if`/`else`.
    ///
    /// | row | item | price |
    /// |---|---|---|
    /// | 1 | Косяк | 15 |
    /// | 2 | Краденый мобильник | 30 |
    /// | 3 | Офигенный косяк | 20 |
    /// | 4 | зоновская наколка | 10 |
    /// | 5 | Кастет | 25 |
    /// | 6 | Дубинка | 50 |
    /// | 7 | пистолет | 150 |
    /// | 8 | патроны | 70 |
    /// | 9 | глушитель | 60 |
    ///
    /// **No arm of the nine tests the district** -- see [`Game::shop_action`]. Rows
    /// 1 and 3 have no already-own test and are **repeatable**; rows 2, 4, 5, 6, 7
    /// and 9 are one-shot through their own already-own test, and row 8 through
    /// none at all.
    ///
    /// Row 3 is the only one that draws, so a purchase there advances the RNG
    /// stream.
    fn buy_dealer_row(&mut self, key: &str, price: i32) -> bool {
        match key {
            // Row 1, Косяк. One gate only -- no already-own test and no prerequisite, so
            // the row is repeatable.
            "1" => {
                self.buy_after_gates(
                    price, // 20ae:0b38 = 15
                    &[],
                    // `^4Чёрт, бабок не хватает.` -- row 1's own refusal line.
                    "^4Чёрт, бабок не хватает.",
                    |g| {
                        // Increments a word COUNT of joints, not a flag. Read by the character sheet,
                        // by `kos` in a fight, and at the street prompt -- the effect is fully
                        // consumed.
                        g.player.joints += 1;
                        term::println("^2Ты купил косяк"); // CS 0x939f `^2Ты купил косяк`, 1000:c912
                    },
                );
                true
            }
            // Row 2, Краденый мобильник.
            "2" => {
                let owned = self.has_mobile;
                self.buy_after_gates(
                    price, // 20ae:0b39 = 30
                    // `^6У тебя уже есть мобила.`
                    &[(owned, Some("^6У тебя уже есть мобила."))],
                    "^4Нету денег", // CS 0x93b0 `^4Нету денег`, 1000:c94e
                    |g| {
                        // Sets the ownership flag. Read by the character sheet, by the in-combat
                        // backup countdown -- which is what the menu line's "подмога быстрее приходит"
                        // actually is -- and by wander encounters.
                        g.has_mobile = true;
                        term::println("^2Чё ты модный типа да?."); // CS 0x93bd `^2Чё ты модный типа да?.`, 1000:c977
                    },
                );
                true
            }
            // Row 3, Офигенный косяк. One gate, so the row is repeatable and each purchase
            // rolls again.
            "3" => {
                self.buy_after_gates(
                    price, // 20ae:0b3a = 20
                    &[],
                    "^4Не хватает", // CS 0x8e4d `^4Не хватает`, 1000:c9ca
                    |g| {
                        // Debit, then the line, then the draw.
                        term::println("^2Пошли стероиды!"); // CS 0x93f0 `^2Пошли стероиды!`, 1000:c9ef

                        match g.rng.below(4) {
                            0 => {
                                g.player.strength += 1; // 1000:ca16 inc [0x389e]
                                term::println("^1Сила +1 "); // CS 0x9402 `^1Сила +1 `, 1000:ca1a
                                g.player.dmg_max += 1; // 1000:ca33 inc [0x38aa]

                                // The dmg-min half runs only when the NEW Сила is even -- the mirror of the
                                // in-combat stat-loss arm, which takes its dmg-min half when Сила is odd.
                                if g.player.strength % 2 == 0 {
                                    g.player.dmg_min += 1; // 1000:ca45 inc [0x38a8]
                                }
                                g.player.hpmax += 1; // 1000:ca49 inc [0x38ae]
                                g.player.hp += 1; // 1000:ca4d inc [0x38ac]
                            }
                            1 => {
                                g.player.agility += 1; // 1000:ca58 inc [0x38a0]
                                term::println("^1Ловкость +1 "); // CS 0x940d `^1Ловкость +1 `, 1000:ca5c
                            }
                            2 => {
                                g.player.vitality += 1; // 1000:ca7c inc [0x38a2]
                                term::println("^1Живучесть +1 "); // CS 0x941c `^1Живучесть +1 `, 1000:ca80
                                g.player.hpmax += 5; // 1000:ca99 add word [0x38ae],0x5
                                g.player.hp += 5; // 1000:ca9e add word [0x38ac],0x5
                            }
                            _ => {
                                g.player.luck += 1; // 1000:caaa inc [0x38a4]
                                term::println("^1Удача +1 "); // CS 0x942c `^1Удача +1 `, 1000:caae
                            }
                        }
                    },
                );
                true
            }
            // Row 4, зоновская наколка.
            "4" => {
                let owned = self.prison_tattoo;
                self.buy_after_gates(
                    price, // 20ae:0b3b = 10
                    // `^6Сделать, конечно, можно но толку не будет.`
                    &[(owned, Some("^6Сделать, конечно, можно но толку не будет."))],
                    "^4Нету денег", // CS 0x93b0 `^4Нету денег`, 1000:caea -- row 2's literal
                    |g| {
                        // Sets the ownership flag. Read outside this arm by the character sheet and by
                        // the wander mugging roll, which halves the chance when it is set -- that
                        // single branch is the row's entire gameplay effect.
                        g.prison_tattoo = true;
                        term::println("^2Чистый зек."); // CS 0x9438 `^2Чистый зек.`, 1000:cb13
                    },
                );
                true
            }
            // Row 5, Кастет.
            "5" => {
                // The better-weapon gate is a short-circuit conjunction:
                // 1000:cb5b `cmp byte [0x394b],0x0` / 1000:cb60 `jz 0xcb70`,
                // 1000:cb62 `cmp byte [0x38c2],0x0` / 1000:cb67 `jz 0xcb70`,
                // 1000:cb69 `cmp byte [0x394c],0x0` / 1000:cb6e
                // `jnz 0xcbeb`. It
                // refuses only when the club AND the knife AND the cleaver
                // are ALL owned -- any one missing falls through to
                // 1000:cb70 and the sale proceeds.
                //
                // ORIGINAL BEHAVIOUR, reproduced rather than reconciled: the
                // combat loot arm granting the same knuckles refuses when ANY
                // one is set (1000:555f, 1000:5566, 1000:556d are each a
                // `jnz <refusal>`), so a player holding a knife can buy the
                // knuckles here but cannot loot them.
                let better = self.weapon_dubinka && self.weapon_nozhik && self.weapon_tesak;
                let owned = self.weapon_kastet;
                self.buy_after_gates(
                    price, // 20ae:0b3c = 25
                    &[
                        // `^6Нафиг тебе он нужен, когда есть более мощное оружие.`
                        (
                            better,
                            Some("^6Нафиг тебе он нужен, когда есть более мощное оружие."),
                        ),
                        // `^6У тебя есть эта железка.`
                        (owned, Some("^6У тебя есть эта железка.")),
                    ],
                    "^4Не хватает деньжат", // CS 0x9473 `^4Не хватает деньжат`, 1000:cb82
                    |g| {
                        g.weapon_kastet = true; // 1000:cb9d mov byte [0x38ba],0x1

                        // The +2/+2 is unconditional here.
                        g.player.dmg_min += 2; // 1000:cbab add word [0x38a8],0x2
                        g.player.dmg_max += 2; // 1000:cbb0 add word [0x38aa],0x2

                        // `^2Ты купил кастет смотри чтоб менты с ним не запалили.`
                        term::println("^2Ты купил кастет смотри чтоб менты с ним не запалили.");
                    },
                );
                true
            }
            // Row 6, Дубинка.
            "6" => {
                // Two conjuncts this time; refuses only when both are set. Same AND/OR
                // mismatch with the loot arm as row 5.
                let better = self.weapon_nozhik && self.weapon_tesak;
                let owned = self.weapon_dubinka;
                let kastet = self.weapon_kastet;
                self.buy_after_gates(
                    price, // 20ae:0b3d = 50
                    &[
                        // `^6Да нафиг она нужна, когда есть более мощное оружие.`
                        (
                            better,
                            Some("^6Да нафиг она нужна, когда есть более мощное оружие."),
                        ),
                        // `^6У тебя есть дубина.`
                        (owned, Some("^6У тебя есть дубина.")),
                    ],
                    "^4Не хватает на дубинку деньжат", // CS 0x9511 `^4Не хватает на дубинку деньжат`, 1000:cc3b
                    |g| {
                        g.weapon_dubinka = true; // 1000:cc56 mov byte [0x394b],0x1

                        // ORIGINAL BUG, reproduced: the menu line advertises `урон+4`, but buying the
                        // club skips BOTH damage adds when the knuckles are not already owned -- there
                        // is no other add on that path. So buying the club first costs 50 руб., sets
                        // the flag, prints the confirmation, and changes the damage range by nothing.
                        // The loot arm granting the same club has both halves (+2/+2 and +4/+4); the
                        // shop arm is missing the +4 branch.
                        if kastet {
                            g.player.dmg_min += 2; // 1000:cc6b add word [0x38a8],0x2
                            g.player.dmg_max += 2; // 1000:cc70 add word [0x38aa],0x2
                        }
                        // `^2Ты купил дубинку - похоже задумал чё-то нехорошее.`
                        term::println("^2Ты купил дубинку - похоже задумал чё-то нехорошее.");
                    },
                );
                true
            }
            // Row 7, самопальный пистолет.
            "7" => {
                let owned = self.pistol.owned;
                self.buy_after_gates(
                    price, // 20ae:0b3e = 150
                    // `^6Ну.. ты.. ВАЩЕ ОФИГЕЛ!`
                    &[(owned, Some("^6Ну.. ты.. ВАЩЕ ОФИГЕЛ!"))],
                    "^4Дорогая штука!", // CS 0x95b2 `^4Дорогая штука!`, 1000:ccea
                    |g| {
                        g.pistol.owned = true; // 1000:cd05 mov byte [0x394d],0x1
                        g.pistol.cartridges += 3; // 1000:cd0a add word [0x394f],0x3

                        // `^2Спасайся кто может!!!`
                        term::println("^2Спасайся кто может!!!");
                        term::println(
                            // `^0Только помни стреляй в бандитских районах - там менты не накроют`
                            "^0Только помни стреляй в бандитских районах - там менты не накроют",
                        );
                    },
                );
                true
            }
            // Row 8, патроны.
            "8" => {
                let no_gun = !self.pistol.owned;
                self.buy_after_gates(
                    price, // 20ae:0b3f = 70
                    // `^6Нету пушки. Сначала купи пистолет`
                    &[(no_gun, Some("^6Нету пушки. Сначала купи пистолет"))],
                    "^4Нехватка денег.", // CS 0x9637 `^4Нехватка денег.`, 1000:cd88
                    |g| {
                        // Adds FIVE, though the menu line says six.
                        g.pistol.cartridges += 5;
                        // `^2Получи пять пуль.. на руки`
                        term::println("^2Получи пять пуль.. на руки");
                    },
                );
                true
            }
            // Row 9, глушитель.
            "9" => {
                // Both gates print nothing at all -- the only silent gates among the nine.
                let no_gun = !self.pistol.owned;
                let not_delivered = self.dealer_delivery_counter != 25;
                let owned = self.pistol.silencer;
                // Nothing else reads the walk counter that gates this row, so the dealers'
                // 25-walk delivery is the silencer's and nothing else's.
                self.buy_after_gates(
                    // Price is 60, though the menu line prints 70 -- reproduced as in the
                    // original.
                    price,
                    &[
                        (no_gun, None),
                        (not_delivered, None),
                        // `^6Да купил уже, купил`
                        (owned, Some("^6Да купил уже, купил")),
                    ],
                    "^4Подкопи бабла.", // CS 0x968a `^4Подкопи бабла.`, 1000:ce19
                    |g| {
                        g.pistol.silencer = true; // 1000:ce34 mov byte [0x394e],0x1

                        // `^2Теперь стреляй где хочешь!`
                        term::println("^2Теперь стреляй где хочешь!");
                    },
                );
                true
            }
            _ => false,
        }
    }

    /// The market's nine purchase arms -- `mar` rows 1..9. Returns `true` when the
    /// key was one of the nine and the arm has run.
    ///
    /// Each row compares the same one-character buffer -- read after the `^0Базар\`
    /// prompt -- against its own key; a miss falls to the next row's compare, so
    /// the nine are a chain of independent `if`s over one buffer. Row 9's miss
    /// reaches the market pickpocket verb `t`, which is what bounds the nine on the
    /// right.
    ///
    /// | row | item | price |
    /// |---|---|---|
    /// | 1 | Хотдог | 2 |
    /// | 2 | Пиво | 5 |
    /// | 3 | Затемнённые очки | 10 |
    /// | 4 | abibas | 15 |
    /// | 5 | Понтовые бутсы | 15 |
    /// | 6 | Реальную кожанку | 25 |
    /// | 7 | adidas | 30 |
    /// | 8 | Понтовёйшие бутсы | 30 |
    /// | 9 | Ваще крутую кожанку | 50 |
    ///
    /// **Three arms test the district and one that should does not** -- see
    /// [`Game::shop_action`] for why row 7 is sold at district 1 off a menu that
    /// never listed it.
    ///
    /// Rows 1 and 2 have no already-own test and are **repeatable**; rows 3-9 are
    /// one-shot through their own.
    ///
    /// **Two arms draw.** Row 1's draw is consumed arithmetically; row 2's draw
    /// picks one of three confirmation lines and **changes no state at all** -- it
    /// is drawn anyway, since skipping it would desynchronise every draw after the
    /// first beer.
    ///
    /// **Rows 7, 8 and 9 grant the upgrade DELTA, not the advertised bonus.**
    /// Applying the full bonus unconditionally would double-count whenever the
    /// lesser item is already owned; the totals come out the same in either
    /// purchase order -- which the gym's recompute subtracts back out.
    fn buy_market_row(&mut self, key: &str, price: i32) -> bool {
        match key {
            // Row 1, Хотдог. Three gates, no already-own test, repeatable.
            "1" => {
                // This gate refuses on the FALL-THROUGH -- the opposite sense to every other
                // already-own/prerequisite gate in either shop.
                let jaw = self.player.broken_jaw;
                // Refuse when hp is already at max.
                let healthy = self.player.hp >= self.player.hpmax;
                self.buy_after_gates(
                    price, // 20ae:0b2e = 2
                    &[
                        // `^4Ты не можешь хавать из-за сломаной челюсти.`
                        (jaw, Some("^4Ты не можешь хавать из-за сломаной челюсти.")),
                        // `^6Да неохота хавать`
                        (healthy, Some("^6Да неохота хавать")),
                    ],
                    // `^4Чёрт, бабок даже на жратву не хватает.` -- belongs to row 1 alone.
                    "^4Чёрт, бабок даже на жратву не хватает.",
                    |g| {
                        // The hot dog heals 3 or 4, and nothing stands between the draw and the add.
                        g.player.hp += 3 + g.rng.below(2);
                        // Clamps hp to its maximum afterward.
                        if g.player.hp > g.player.hpmax {
                            g.player.hp = g.player.hpmax;
                        }
                        term::println("^2Ты сожрал хот-дог"); // CS 0x8e23 `^2Ты сожрал хот-дог`, 1000:bdd6
                    },
                );
                true
            }
            // Row 2, Пиво. One gate, repeatable.
            "2" => {
                self.buy_after_gates(
                    price, // 20ae:0b2f = 5
                    &[],
                    "^4Не хватает", // CS 0x8e4d `^4Не хватает`, 1000:be29
                    |g| {
                        // Dispatches over three compares; all three converge and the roll changes NO
                        // state -- it is purely cosmetic, and it is still a draw.
                        match g.rng.below(3) {
                            0 => term::println("^2Глинское? Чё за нафиг? А ладно."), // CS 0x8e5a `^2Глинское? Чё за нафиг? А ладно.`, 1000:be5b
                            1 => term::println("^2Пивко. Холодненькое."), // CS 0x8e7c `^2Пивко. Холодненькое.`, 1000:be7b
                            2 => term::println("^2Ну чё по пиву?."), // CS 0x8e93 `^2Ну чё по пиву?.`, 1000:be9b
                            // Unreachable in practice: no line prints here, though the increment still
                            // runs on this arm too.
                            _ => {}
                        }
                        // Increments a WORD count of half-litres, not a flag. A failed purchase adds
                        // no beer.
                        g.player.beer_dl += 1;
                    },
                );
                true
            }
            // Row 3, Затемнённые очки. Two gates.
            "3" => {
                let owned = self.dark_glasses;
                self.buy_after_gates(
                    price, // 20ae:0b30 = 10
                    // `^6У тебя есть очки от солнца.`
                    &[(owned, Some("^6У тебя есть очки от солнца."))],
                    "^4Не хватает бабок", // CS 0x8ea7 `^4Не хватает бабок`, 1000:bedb
                    |g| {
                        // Sets the ownership flag. Read outside this arm by the character sheet and by
                        // the wander cop encounter, which is where the glasses actually stop a fight.
                        g.dark_glasses = true;
                        term::println("^2Модные такие очки от солнца."); // CS 0x8eba `^2Модные такие очки от солнца.`, 1000:bf04
                    },
                );
                true
            }
            // Row 4, костюм abibas.
            "4" => {
                // ONE conjunct -- unlike `bmar` rows 5 and 6, whose better-weapon gates AND
                // three and two flags together.
                let better = self.wear_suit_adidas;
                let owned = self.wear_suit_abibas;
                self.buy_after_gates(
                    price, // 20ae:0b31 = 15
                    &[
                        // `^6У тебя есть более крутой костюм.`
                        (better, Some("^6У тебя есть более крутой костюм.")),
                        // `^6У тебя уже есть костюм.`
                        (owned, Some("^6У тебя уже есть костюм.")),
                    ],
                    "^4Не хватает денег", // CS 0x8ef9 `^4Не хватает денег`, 1000:bf65
                    |g| {
                        g.wear_suit_abibas = true; // 1000:bf80
                        term::println("^2Теперь ты больше похож на гопа."); // CS 0x8f0c `^2Теперь ты больше похож на гопа.`, 1000:bf8e
                                                                            // ARMOUR +1, unconditionally. The menu line's `Смягчает пинок на 1` agrees.
                                                                            // Read outside this arm by the kick's damage reduction and the gym's
                                                                            // recompute.
                        g.player.armor = g.player.armor.wrapping_add(1);
                    },
                );
                true
            }
            // Row 5, Понтовые бутсы.
            "5" => {
                let better = self.wear_boots_pontovye;
                let owned = self.wear_boots;
                self.buy_after_gates(
                    price, // 20ae:0b32 = 15
                    &[
                        // `^6У тебя бутсы по круче.`
                        (better, Some("^6У тебя бутсы по круче.")),
                        // `^6У тебя такие уже есть.`
                        (owned, Some("^6У тебя такие уже есть.")),
                    ],
                    "^4Нету на них денег", // CS 0x8f6d `^4Нету на них денег`, 1000:c00e
                    |g| {
                        g.wear_boots = true; // 1000:c029
                        term::println("^2Зацени красовки."); // CS 0x8f81 `^2Зацени красовки.`, 1000:c037
                                                             // The damage range, +1/+1, unconditionally. The menu says only `Увеличивают
                                                             // урон`.
                        g.player.dmg_min += 1;
                        g.player.dmg_max += 1;
                    },
                );
                true
            }
            // Row 6, Реальную кожанку.
            "6" => {
                // A buy-path district test, which no `bmar` row has, and it prints nothing --
                // the line falls through to the re-prompt.
                let below_district = self.district <= 1;
                let better = self.wear_jacket_krutaya;
                let owned = self.wear_jacket;
                self.buy_after_gates(
                    price, // 20ae:0b33 = 25
                    &[
                        (below_district, None),
                        // `^6Утебя есть кожанка круче.` -- the missing space after `У` is the
                        // original's, kept as-is.
                        (better, Some("^6Утебя есть кожанка круче.")),
                        // `^6Ты уже купил это.`
                        (owned, Some("^6Ты уже купил это.")),
                    ],
                    "^4Не достаточно бабла", // CS 0x8fc8 `^4Не достаточно бабла`, 1000:c0c5
                    |g| {
                        g.wear_jacket = true; // 1000:c0e0
                        term::println("^2Ну весь на понтах."); // CS 0x8fde `^2Ну весь на понтах.`, 1000:c0ee
                                                               // A byte add, so it wraps at 255 rather than widening.
                        g.player.armor = g.player.armor.wrapping_add(2);
                    },
                );
                true
            }
            // Row 7, костюм adidas. Setup 1000:c142, key compare 1000:c14c,
            // miss 1000:c151 `jz 0xc156` over 1000:c153 `jmp 0xc1d7` -- and
            // 1000:c1d7 is row 8's district gate.
            "7" => {
                // ORIGINAL BEHAVIOUR, reproduced rather than fixed: **there
                // is no district gate here**, though the menu hides this row
                // below district 2. `1000:bb80 cmp byte [0x3692],0x1` covers
                // rows 6 AND 7 in the menu block (their price bytes
                // `20ae:0b33` and `20ae:0b34` are loaded at 1000:bb8a and
                // 1000:bbe6, both inside its listed range 1000:bb8a..
                // 1000:bc42), while on the buy path row 6's gate skip
                // 1000:c095 jumps to 1000:c142, this row's setup, with
                // nothing in between. So the original sells the adidas suit
                // at district 1 off a menu that never listed it. There is no
                // better-item gate either -- the sweep of this span finds
                // four conditional branches, and the miss, the two gates
                // below and the upgrade guard account for all of them.
                //
                // 1000:c156 `cmp byte [0x38b7],0x0` / 1000:c15b `jnz 0xc1be`.
                let owned = self.wear_suit_adidas;
                // 1000:c1aa `cmp byte [0x38b4],0x0` / 1000:c1af `jz 0xc1b7`.
                // Read BEFORE the arm runs because the flag this arm writes
                // is a different one (`20ae:38b7`), so nothing here observes
                // its own write.
                let has_abibas = self.wear_suit_abibas;
                self.buy_after_gates(
                    price, // 20ae:0b34 = 30
                    // CS 0x9036 `^6У тебя уже есть этот костюм.`, pushed at 1000:c1be.
                    &[(owned, Some("^6У тебя уже есть этот костюм."))],
                    "^4Не хватает денег", // CS 0x8ef9 `^4Не хватает денег`, 1000:c168 -- row 4's literal
                    |g| {
                        g.wear_suit_adidas = true; // 1000:c183
                                                   // Debit 1000:c18d.
                        term::println("^2Чистый гопник."); // CS 0x9025 `^2Чистый гопник.`, 1000:c191
                                                           // The UPGRADE SPLIT: 1000:c1b1 `inc [0x38b2]` when
                                                           // the abibas suit is already owned, 1000:c1b7
                                                           // `add byte [0x38b2],0x2` when it is not, rejoining
                                                           // at 1000:c1b5 `jmp short 0xc1bc`. Either way the
                                                           // player ends on +2 of suit armour, whichever order
                                                           // the two rows were bought in.
                        g.player.armor =
                            g.player.armor.wrapping_add(if has_abibas { 1 } else { 2 });
                    },
                );
                true
            }
            // Row 8, Понтовёйшие бутсы. Span starts at the district gate
            // 1000:c1d7; setup 1000:c1e1, key compare 1000:c1eb, miss
            // 1000:c1f0 `jz 0xc1f5` over 1000:c1f2 `jmp 0xc27f`.
            "8" => {
                // 1000:c1d7 `cmp byte [0x3692],0x2` / 1000:c1dc `ja 0xc1e1` /
                // 1000:c1de `jmp 0xc27f`. Silent, like row 6's.
                let below_district = self.district <= 2;
                // 1000:c1f5 `cmp byte [0x38b8],0x0` / 1000:c1fa `jnz 0xc266`.
                // No better-item gate.
                let owned = self.wear_boots_pontovye;
                // 1000:c249 `cmp byte [0x38b5],0x0` / 1000:c24e `jz 0xc25a`.
                let has_boots = self.wear_boots;
                self.buy_after_gates(
                    price, // 20ae:0b35 = 30
                    &[
                        (below_district, None),
                        // CS 0x8f94 `^6У тебя такие уже есть.`, pushed at 1000:c266 -- row 5's literal.
                        (owned, Some("^6У тебя такие уже есть.")),
                    ],
                    "^4Нету на них денег", // CS 0x8f6d `^4Нету на них денег`, 1000:c207 -- row 5's too
                    |g| {
                        g.wear_boots_pontovye = true; // 1000:c222
                                                      // Debit 1000:c22c.
                        term::println("^2Офигенные бутцы."); // CS 0x9057 `^2Офигенные бутцы.`, 1000:c230
                                                             // The UPGRADE SPLIT on the damage range: 1000:c250
                                                             // `inc [0x38a8]` / 1000:c254 `inc [0x38aa]` with the
                                                             // lesser boots owned, 1000:c25a / 1000:c25f
                                                             // `add word [...],0x2` without, rejoining at
                                                             // 1000:c258 `jmp short 0xc264`. The menu's `Урон+2`
                                                             // is the TOTAL, not this arm's own add.
                        let delta = if has_boots { 1 } else { 2 };
                        g.player.dmg_min += delta;
                        g.player.dmg_max += delta;
                    },
                );
                true
            }
            // Row 9, Ваще крутую кожанку. Span starts at the district gate
            // 1000:c27f; setup 1000:c289, key compare 1000:c293, miss
            // 1000:c298 `jz 0xc29d` over 1000:c29a `jmp 0xc31f`.
            "9" => {
                // 1000:c27f `cmp byte [0x3692],0x3` / 1000:c284 `ja 0xc289` /
                // 1000:c286 `jmp 0xc31f`. Silent, like rows 6 and 8.
                let below_district = self.district <= 3;
                // 1000:c29d `cmp byte [0x38b9],0x0` / 1000:c2a2 `jnz 0xc306`.
                let owned = self.wear_jacket_krutaya;
                // 1000:c2f1 `cmp byte [0x38b6],0x0` / 1000:c2f6 `jz 0xc2ff`.
                let has_jacket = self.wear_jacket;
                self.buy_after_gates(
                    price, // 20ae:0b36 = 50
                    &[
                        (below_district, None),
                        // CS 0x8ff3 `^6Ты уже купил это.`, pushed at 1000:c306 -- row 6's literal.
                        (owned, Some("^6Ты уже купил это.")),
                    ],
                    "^4Не достаточно бабла", // CS 0x8fc8 `^4Не достаточно бабла`, 1000:c2af -- row 6's too
                    |g| {
                        g.wear_jacket_krutaya = true; // 1000:c2ca
                                                      // Debit 1000:c2d4.
                        term::println("^2Ну крутой, сдохнуть можно!"); // CS 0x906c `^2Ну крутой, сдохнуть можно!`, 1000:c2d8
                                                                       // The UPGRADE SPLIT: 1000:c2f8
                                                                       // `add byte [0x38b2],0x2` with the lesser jacket
                                                                       // owned, 1000:c2ff `add byte [0x38b2],0x4` without,
                                                                       // rejoining at 1000:c2fd `jmp short 0xc304`.
                        g.player.armor =
                            g.player.armor.wrapping_add(if has_jacket { 2 } else { 4 });
                    },
                );
                true
            }
            _ => false,
        }
    }

    /// `^0Битва\` (file `0x4A49`). Confirmed modal by the live capture
    /// (`mar`/`i` typed here were ignored, reprinting the prompt).
    ///
    /// ## The verb set -- established from flow
    ///
    /// `FUN_1000_3d11` compares the typed line itself, with `0f78:0bd8` (the
    /// same Pascal shortstring compare `entry` uses) against its **own**
    /// buffer `DS:3a72`. The image holds 93 `9a d8 0b 78 0f` call sites;
    /// scanning from `1000:3d11` to the next function entry `1000:5f55` --
    /// a window wider than the record's own `size` span, so the count does
    /// not rest on reading `size` as a span -- returns exactly **nine** of
    /// them, each preceded byte-for-byte by `bf 72 3a` / `1e` / `57` and
    /// `bf <lo> <hi>` / `0e` / `57`, so each site's token is read out of the
    /// instruction rather than inferred:
    ///
    /// | compare | token | token file |
    /// |---|---|---|
    /// | `1000:4440` | `k` | `0x4A52` |
    /// | `1000:48e1` | `run` | `0x4C8B` |
    /// | `1000:4b0d` | `kos` | `0x4D81` |
    /// | `1000:4c2e` | `s` | `0x4E6F` |
    /// | `1000:4c42` | `sv` | `0x4E71` |
    /// | `1000:4c56` | `e` | `0x4E74` |
    /// | `1000:4c75` | `k` again, gated on `[0x3c80] >= 1` at `1000:4c64` | `0x4A52` |
    /// | `1000:4caa` | `v` | `0x4E96` |
    /// | `1000:4ea8` | `f` | `0x4FE4` |
    ///
    /// So `k` **is** the in-combat attack verb, and `sv` is a dispatched verb
    /// here rather than an oracle-capture inference. An earlier revision of
    /// this comment said the input loop "was not traced" and called `k` "this
    /// port's own choice"; both statements were false. `h`/`mh` are not among
    /// the nine because they go through the subroutine call at `1000:4b00`,
    /// which makes the in-combat verb set **ten**.
    ///
    /// ## Nine independent `if`s, not an `if`/`else` chain
    ///
    /// **Established from flow**, and this is why the loop below is a
    /// straight line rather than a `match`. One `Битва\` prompt runs the
    /// whole chain top to bottom: `1000:583e jmp 0x40f2` is the function's
    /// only back edge, so no arm returns to the prompt and every arm rejoins
    /// the line with the buffer still holding what was typed. Two
    /// consequences the port has to reproduce:
    ///
    /// * that is **why there are two `k` compares**. `1000:4445 jz 0x444a`
    ///   enters the blow loop and its three exits (`1000:467c`, `1000:48cb`,
    ///   `1000:48d2`) all land on `1000:48d7`, the `run` compare's setup --
    ///   so `1000:4c75` gets a second go at the same line and gives the
    ///   attack verb its second effect, the backup countdown.
    /// * the backup block at `[1000:4d93, 1000:4e9e)` sits between the `v`
    ///   arm and the `f` compare and belongs to neither, so it runs on
    ///   **every** prompt -- including one whose line matched no compare at
    ///   all.
    ///
    /// Every arm is now implemented. `docs/re/combat-dispatch.md` is the map
    /// (Task 17) and [`crate::combat_dispatch`] the arithmetic; what each one
    /// does, in chain order:
    ///
    /// | at | verb | here |
    /// |---|---|---|
    /// | `1000:444a` | `k` | [`Game::combat_round`], `docs/re/combat.md` |
    /// | `1000:48eb` | `run` | [`Game::flee`] |
    /// | `1000:4b00` | `h`/`mh` | [`Game::beer`] |
    /// | `1000:4b17` | `kos` | [`Game::smoke`] |
    /// | `1000:4c35` | `s` | [`Game::show_stats`] -- `call 0x1a03`, Task 16 |
    /// | `1000:4c49` | `sv` | [`Game::print_enemy_block`] -- `call 0x1348`, the **enemy's** sheet |
    /// | `1000:4c5d` | `e` | `xor ax,ax` / `call 0f78:0116` = `Halt(0)` |
    /// | `1000:4c7c` | `k` (2nd) | [`crate::combat_dispatch::Backup::tick_on_attack`] |
    /// | `1000:4cb4` | `v` | [`Game::backup_in_fight`] |
    /// | `1000:4d93` | -- | [`Game::backup_attacks`], on every prompt |
    /// | `1000:4eb2` | `f` | [`Game::shoot_in_fight`] |
    ///
    /// `sv` calling a *different* function from `s` is the correction Task 17
    /// made to Task 16's hypothesis, and it is what makes
    /// `print_enemy_block` -- not `show_stats` -- right here:
    /// `FUN_1000_1348` references no address in `[20ae:3690, 20ae:3951]`,
    /// the player's record, at all.
    ///
    /// Death and victory both come from `FUN_1000_3d11`'s own tail:
    ///
    /// * `1000:4f82` `hp <= 0`. With the rector flag set, file `0x509C` and
    ///   no rescue behind it ([`Game::rector_showdown`]); otherwise, if the
    ///   den is known and the street cred is at least 10, the hospital rescue
    ///   at `1000:4fce` ([`Game::hospital_rescue`]). The plain case is
    ///   `1000:5053`: file `0x5127`
    ///   (`^4Ты сдох.`) and then `FUN_1000_074b(0)`, the end screen. So death
    ///   **ends the game** -- established from flow, not from the RTL's
    ///   symbol layout: `FUN_1000_074b`'s last act is `1000:0abe`
    ///   `xor ax,ax` / `1000:0ac0` `call 1f78:0116`, and that routine
    ///   (Ghidra `1f78`, file `0x11166`) restores the saved interrupt
    ///   vectors and terminates the process at file `0x1123C`..`0x1123E`
    ///   with `b4 4c` `cd 21` -- `mov ah,0x4c` / `int 0x21`. The `mov sp,bp`
    ///   / `pop bp` / `ret 2` epilogue at `1000:0ac5` is unreachable
    ///   compiler boilerplate.
    /// * `1000:5189` the enemy died: file `0x5250` (`^2Враг сдох.`), then
    ///   `1000:51b4` file `0x525D` (`^6За отпин врага ты получаешь` ...) with
    ///   `str+agi+vit+luck` as the award.
    ///   "^2Ты победил." is *not* a per-fight line: it is file `0x1DBF` (a
    ///   49-byte shortstring padded with 36 leading spaces to centre it),
    ///   the end-of-game banner `FUN_1000_074b` writes when you beat the
    ///   rector, and printing it here was a fabrication.
    ///
    /// ## `run` -- fleeing
    ///
    /// **Established from flow**, and needed because Task 11f's cop
    /// encounter reaches this loop without ever asking a question:
    /// `1000:48d7`..`1000:48e1` compares the typed line against the literal
    /// `run` (file `0x4C8B`, `03 72 75 6e`) with `0f78:0bd8`, combat's own
    /// token compare -- **not** the street dispatcher's, which is why
    /// `crate::commands::parse` (where `w` and `run` fold into one verb) is
    /// bypassed for it here.
    ///
    /// [`Game::flee`] is the arm, [`Game::flee_penalty`] the level it costs.
    /// The `1000:48eb` refusal reads [`Game::rector_showdown`], which
    /// [`Game::enter_district_5`] sets once `self.district` reaches 5 (Task
    /// 20), so this arm is now reachable in real play, not only from a test.
    ///
    /// Fleeing does **not** end the prompt: `1000:4af7 mov byte [bp-0x1],1`
    /// only raises the exit flag, and `1000:5838` does not read it until the
    /// rest of the chain, the death test and the victory test have all run.
    /// So a `run` typed in the prompt where the gopota land the killing blow
    /// is a victory, and the loop below reproduces that.
    ///
    /// No arm of the flee path draws: there is no `9a 4b 11 78 0f` anywhere
    /// in `1000:48eb`..`1000:4afb`. That is what makes run A turn 7 of
    /// `data/rng_trace.json` -- a cop fight entered and fled -- show zero
    /// draws between `1000:b792` and the next turn's `1000:af68`.
    ///
    /// ## `opponent_kind` IS `param_1`, and all five of its effects are here
    ///
    /// The argument is `FUN_1000_3d11`'s own `bp+4`, and every caller passes
    /// the literal its original call site pushes: 0 at the wander's
    /// `1000:b826`/`1000:b829`, 6 at the den's `1000:dc5b`, 5 at the den
    /// job's `1000:ddfc`, 2 at the club's `1000:e222`, and 3 and 4 at
    /// `1000:ae2d`/`1000:ae39` ([`Game::rector_endgame`]). Task 40 widened
    /// the signature for the opener gate below
    /// (`1000:3d27`..`1000:3d2f`); the other four landed with
    /// `docs/re/port-gaps.md` rows 7, 17 and 21:
    ///
    /// * the `param_1` 1 / 3 / 4 openers at `1000:3e8d`, `1000:3ead` and
    ///   `1000:3f2b` -- [`crate::ending::OPENER_1`] and its two siblings.
    /// * `1000:5085 cmp byte [bp+0x4],0x4` -- the victory ending, which
    ///   reaches [`crate::ending::end_screen`] through
    ///   [`crate::ending::marquee`] and never returns to the tail.
    ///   `1000:5133` is `call 0xaec`, NOT `FUN_1000_074b(1)`; an earlier
    ///   revision of this comment said otherwise and the marquee is the
    ///   difference.
    /// * `1000:5139` -- the `param_1 == 3` fake-out, which DOES fall through
    ///   into the ordinary tail.
    /// * `1000:51a6` / `1000:51f6` -- the XP award and the "too weak an
    ///   opponent" pair, both skipped for `param_1` in `{3, 4}`.
    /// * `1000:57ce cmp byte [bp+0x4],0x6` gates `1000:57d4`..`1000:5838`,
    ///   47 instructions holding a понтовость award, an xp award, two lines
    ///   and `FUN_1000_2526(0)`, **which spends draws**.
    ///
    /// So a non-`{0, 6}` value reaching here skips the class-keyed greeting
    /// and takes whichever of the four arms above names it.
    pub(crate) fn run_combat(
        &mut self,
        opponent_kind: u8,
        mut enemy: Fighter,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        // 1000:3d24 `mov al,[bp+0x4]` / 1000:3d27 `cmp al,0x0` /
        // 1000:3d29 `jz 0x3d32` / 1000:3d2b `cmp al,0x6` /
        // 1000:3d2d `jz 0x3d32` -- the OUTER chain, on `param_1`. Everything
        // else takes 1000:3d2f `jmp 0x3e8d` and skips the opener entirely,
        // which is why the den's cop fight (`param_1 = 5`) and the club's
        // (`param_1 = 2`) are silent here. `crate::combat_opener` is the arm
        // and `docs/re/combat-opener.md` the map.
        if matches!(opponent_kind, 0 | 6) {
            // The rank is passed as a THUNK, not a String: only arms 8 and 9
            // read `20ae:389c` (1000:3e0c, 1000:3e63), and `rank_name` panics
            // on a class `data/enemies.json` has no row for, so evaluating it
            // here would widen the port's panic surface past the original's
            // read set. See `combat_opener::greet`.
            let player_class = self.player.class;
            combat_opener::greet(enemy.class, &self.player.name, || {
                Self::rank_name(player_class)
            });
        }
        // The three OTHER arms of the same chain. `1000:3d2f jmp 0x3e8d`
        // enters them, and each falls out to 1000:3fa7 below.
        match opponent_kind {
            // 1000:3e8d `cmp al,0x1` / 1000:3e8f `jnz 0x3ead` -- the market
            // pickpocket's opener (CS 0x2cfa, printed 1000:3ea5). One line
            // and no ReadKey. Its only caller image-wide is
            // 1000:c433/1000:c436, the verb `t`, and that landed with
            // `docs/re/port-gaps.md` row 9: the arm had no caller until
            // then. `crate::market`'s
            // `the_bust_is_the_only_caller_of_the_param_one_opener` is what
            // drives it.
            1 => term::println(ending::OPENER_1[0]),
            // 1000:3ead `cmp al,0x3` / 1000:3eaf `jnz 0x3f2b` -- the first
            // rector fight. Four lines, each followed by a ReadKey
            // (1000:3eca, 3ee8, 3f06, 3f24).
            3 => {
                for line in ending::OPENER_3 {
                    term::println(line);
                    term::read_key(lines);
                }
            }
            // 1000:3f2b `cmp al,0x4` / 1000:3f2d -- the second. Same shape,
            // ReadKeys at 1000:3f48, 3f66, 3f84, 3fa2.
            4 => {
                for line in ending::OPENER_4 {
                    term::println(line);
                    term::read_key(lines);
                }
            }
            _ => {}
        }
        // 1000:3fa7..1000:40e5 -- the two blow budgets and the two lines that
        // report the reduction. Both run whatever `param_1` was: the opener's
        // exit 1000:3e8a `jmp 0x3fa7` and the chain's own misses all land
        // here. `crate::combat::budget_report` carries the gates.
        //
        // The enemy's budget first (1000:3fa7, `[0x3956] + 4` cut down by the
        // player's `[0x38a0] + 4`), then the player's (1000:404a, the mirror).
        if let Some((reduced, unreduced)) = combat::budget_report(&enemy, &self.player) {
            // 1000:4013, CS 0x2dec / file 0x46BC, printed at 1000:403d.
            term::println(&text::fill(
                "^2Из-за твоей хорошей ловкости враг сможет пнуть тебя раз # вместо #",
                &[i64::from(reduced), i64::from(unreduced)],
            ));
        }
        if let Some((reduced, unreduced)) = combat::budget_report(&self.player, &enemy) {
            // 1000:40b6, CS 0x2e31 / file 0x4701, printed at 1000:40e0.
            term::println(&text::fill(
                "^4Из-за хорошей ловкости врага ты сможешь пнуть его раз # вместо #",
                &[i64::from(reduced), i64::from(unreduced)],
            ));
        }
        // 1000:40ed `c6 86 ed fe 00` -- `mov byte [bp-0x113],0`, OUTSIDE the
        // prompt loop whose top is 1000:40f2 (its back edge is 1000:583e
        // `jmp 0x40f2`, the only branch in the whole function that targets
        // it). So the counter is per FIGHT, not per session.
        let mut prompts_seen: u8 = 0;
        // `20ae:3c80`. A fight-local even though it lives in DGROUP:
        // `1000:5841` / `1000:5843` zero it as the function returns, and all
        // 17 of its image-wide references are inside `FUN_1000_3d11`.
        let mut backup = Backup::default();
        loop {
            if self.player.hp == 0 || enemy.hp == 0 {
                break;
            }
            self.crowd(&mut prompts_seen);
            term::print("^0Битва\\");
            // 1000:441d, the prompt's own ReadLn: the sample point.
            let Some(line) = term::read_line(lines) else {
                self.running = false;
                return Ok(());
            };
            let line = line?;
            let cmd = parse(&line);
            // 1000:4af7 / 1000:5077 / 1000:51a2 all write `[bp-0x1]`, and
            // 1000:5838 at the bottom of the loop is what reads it. Only the
            // first of the three is set inside the chain; the other two are
            // the death and victory blocks, which this port runs after the
            // loop.
            let mut fled = false;

            // 1000:4440, token file 0x4A52 -- the blow exchange. The arm
            // rejoins the chain at 1000:48d7, so everything below still runs.
            if cmd == Command::Fight {
                self.combat_round(&mut enemy);
            }

            // 1000:48dc -- combat's own `run` compare, ahead of everything
            // `parse` knows about (`parse` folds `w` and `run` into
            // `Command::Walk`). 1000:48e1 is the `call 0f78:0bd8` and
            // 1000:48e6 `jz 0x48eb` the branch it sets ZF for; the miss is
            // 1000:48e8 `jmp 0x4afb`.
            if line.eq_ignore_ascii_case("run") {
                fled = self.flee();
            }

            // 1000:4afb / 1000:4b00 -- FUN_1000_3d11 calls FUN_1000_29c4, the
            // same routine `entry` calls at 1000:e966, with its own DS:3a72.
            match cmd {
                Command::Drink => self.beer(Beer::One),
                Command::BingeDrink => self.beer(Beer::Binge),
                // 1000:4b0d, token file 0x4D81 -> the arm at 1000:4b17,
                // reached by 1000:4b12 `jz 0x4b17`.
                Command::Joint => self.smoke(Joint::Fight),
                // 1000:4c2e, token CS 0x359f -> 1000:4c35 `call 0x1a03`, the
                // PLAYER's sheet (Task 16, `docs/re/character-sheet.md`).
                // 1000:4c33 `jnz 0x4c38` is the miss that skips the call.
                Command::Stats => self.show_stats(),
                // 1000:4c42, token CS 0x35a1 -> 1000:4c49 `call 0x1348`, the
                // ENEMY's sheet -- a different function, settled in Task 17.
                // `FUN_1000_1348` references no address in the player's
                // record at all, so `print_enemy_block` is the right callee
                // here and `show_stats` would be the wrong one.
                // 1000:4c47 `jnz 0x4c4c` is this compare's own miss.
                Command::Inspect => self.print_enemy_block(&enemy),
                _ => {}
            }

            // 1000:4c56, token CS 0x35a4 -> 1000:4c5d `xor ax,ax` /
            // `call 0f78:0116`, which is `System.Halt(0)`: the RTL restores
            // the saved interrupt vectors and ends the process with
            // `mov ah,0x4c` / `int 0x21`. So `e` at the fight prompt does not
            // leave the fight -- it leaves the GAME, without writing a save
            // and without the end screen `FUN_1000_074b` draws on death.
            //
            // Matched on the LITERAL, not on `Command::Quit`, for the same
            // reason the `run` arm above is: `parse` folds `e` and `exit`
            // into one verb because `entry` dispatches both (`1000:edfa` and
            // `1000:ede9`), and the fight prompt compares only `e`. The
            // shortstring `exit` exists at exactly one image offset,
            // CS `0xab1e`, and `1000:ede9` is its only reference -- it is
            // never materialised inside `FUN_1000_3d11`, so `exit` typed
            // here falls through the whole chain and prints nothing, like
            // any other unmatched line.
            // 1000:4c5b `jnz 0x4c64` is the miss; the hit falls through to
            // 1000:4c5d.
            if line.eq_ignore_ascii_case("e") {
                self.last_enemy = Some(enemy);
                self.running = false;
                return Ok(());
            }

            // 1000:4c64 `cmp word [0x3c80],1` / `jl 0x4ca0` guards the SECOND
            // `k` compare at 1000:4c75, so the countdown only ticks once the
            // backup has been called. The three conjuncts below are the
            // original's three branches, in its order: 1000:4c64's guard,
            // then 1000:4c7a `jnz 0x4ca0` (the `k` compare missed), then
            // 1000:4c80 `cmp word [0x3c80],0x3` / 1000:4c85 -- which is what
            // `tick_on_attack` returns.
            if backup.count() >= 1 && cmd == Command::Fight && backup.tick_on_attack() {
                // 1000:4c87, CS 0x35a6 -- the copy WITHOUT the trailing dot.
                term::println("^2Подошли пацаны - Ща начнется!");
            }

            // 1000:4caa, token CS 0x35c6 -> the arm at 1000:4cb4, entered
            // by 1000:4caf `jz 0x4cb4`.
            if cmd == Command::Backup {
                self.backup_in_fight(&mut backup);
            }

            // [1000:4d93, 1000:4e9e) -- the gopota's own attack, and NOT part
            // of the `v` arm: it is on the straight line between `v` and `f`,
            // so it runs whatever was typed, including a line no compare
            // matched. Both fighters' hp are carried as `i32` across it for
            // the same reason `combat_round` does; only the stored value
            // saturates.
            let mut ehp = i32::from(enemy.hp);
            self.backup_attacks(&mut backup, &mut ehp, &enemy);

            // 1000:4ea8, token CS 0x3714 -> the arm at 1000:4eb2, entered
            // by 1000:4ead `jz 0x4eb2`. There is no
            // enemy-alive gate on it, unlike the backup block's 1000:4d93 --
            // so a shot fired in the same prompt the backup landed a killing
            // blow still lands, and its `У него осталось #` is negative.
            if cmd == Command::Shoot {
                self.shoot_in_fight(&mut ehp);
            }
            enemy.hp = ehp.max(0) as u16;

            // Everything else really is not compared here. The ten verbs are
            // the whole in-combat table: `20ae:3a72` has 102 references
            // image-wide, and `docs/re/combat-dispatch.md` closes the twelve
            // that are INSIDE `FUN_1000_3d11` -- the ReadLn destination, the
            // case fold, the nine compares' setups and the `h`/`mh`
            // subroutine call. The scope is load-bearing: the buffer is
            // shared with every sub-prompt in `entry`, which is why `x` and
            // `wes` are compared against it too, at `1000:ce80` and
            // `1000:ced8`, in the dealers' menu (see `crate::commands`).
            // So a street verb typed at `^0Битва\` reaches no handler at all
            // -- which is what the live capture saw for `mar` and `i`.

            // 1000:5838 `cmp byte [bp-0x1],0` is the loop's exit test, and it
            // is read AFTER the death test at 1000:4f82 and the victory test
            // at 1000:507b. Those two are the loop-top `break` below, so the
            // one case the two orderings disagree about is a `run` in the
            // same prompt where the backup landed the killing blow: there
            // `1000:507b`'s `jle 0x5085` takes the victory arm and the flee
            // flag never gets read.
            //
            // The PLAYER cannot be newly dead here, so `1000:4f82` needs no
            // counterpart in this condition: `run` parses to `Command::Walk`,
            // so `combat_round` did not fire in this prompt, and the only
            // other thing that touches the player's hp on a flee is
            // `flee_penalty`'s `hp := hpmax` clamp, which would need `hpmax`
            // to have reached 0 -- impossible at the level >= 1 that
            // `1000:4931` requires before the penalty runs at all. A
            // `player.hp > 0` clause would therefore be a condition that
            // cannot be false, which is this project's signature defect.
            if fled && enemy.hp > 0 {
                self.last_enemy = Some(enemy);
                return Ok(());
            }
        }

        self.last_enemy = Some(enemy.clone());
        if self.player.hp == 0 {
            // 1000:4f82 `cmp word [0x38ac],0` / `jle 0x4f8c` -- the death
            // test, and it runs BEFORE the victory test at 1000:507b.
            //
            // 1000:4f8c `cmp byte [0x3c83],1` / `jnz 0x4fba` -- the rector's
            // own death line (CS 0x37cc), ahead of the hospital and with no
            // rescue behind it: 1000:4fac is `ReadKey` and 1000:4fb4 calls
            // FUN_1000_074b with `al = 0`, the end screen, which halts. So
            // dying to the rector is final however much cred and whatever
            // flags the player is carrying. [`Game::enter_district_5`]
            // (Task 20) sets [`Game::rector_showdown`] once `self.district`
            // reaches 5.
            if self.rector_showdown {
                term::println("^4Ты сдох. Ректор тебя замочил. Ты так и не доказал свою крутизну.");
                // 1000:4fac ReadKey, then 1000:4fb4 FUN_1000_074b(0).
                term::read_key(lines);
                ending::end_screen(false, lines);
                self.running = false;
                return Ok(());
            }
            if self.hospital_rescue() {
                return Ok(());
            }
            // 1000:5053, file 0x5127, then 1000:506c ReadKey and 1000:5074
            // FUN_1000_074b(0), whose own tail is the RTL's `mov ah,0x4c` /
            // `int 0x21`: death ends the process.
            term::println("^4Ты сдох.");
            term::read_key(lines);
            ending::end_screen(false, lines);
            self.running = false;
            return Ok(());
        }

        // 1000:5085 `cmp byte [bp+0x4],0x4` -- the victory ENDING, and it
        // does not rejoin anything: 1000:5133 `call 0xaec` runs the marquee,
        // which calls `FUN_1000_074b(1)`, which halts. No spoils, no XP, no
        // item roll. (`1000:5136 jmp 0x5838` is the unreachable tail.)
        if opponent_kind == 4 {
            // 1000:508e/5091 `[0x38ce] := [0x38d0]` then 1000:5094
            // FUN_1000_2526(1) -- a forced level with the cap lifted.
            self.progress.xp = self.progress.threshold;
            progress::apply_levels(&mut self.progress, &mut self.player, &mut self.rng, 0, true);
            // The first four each carry a ReadKey -- 1000:50b3, 50d1, 50ef,
            // 510d.
            let (last, first_four) = ending::ENDING_4
                .split_last()
                .expect("ENDING_4 is not empty");
            for line in first_four {
                term::println(line);
                term::read_key(lines);
            }
            // CS 0x3915, printed 1000:5126 -- no ReadKey behind it.
            term::println(last);
            // 1000:512b `call 0x1a03`, the character sheet, then 1000:512e
            // ReadKey and 1000:5133 the marquee.
            self.show_stats();
            term::read_key(lines);
            ending::marquee(lines);
            self.running = false;
            return Ok(());
        }

        // 1000:5139 `cmp byte [bp+0x4],0x3` -- the fake-out. Same forced
        // level as the ending above, two lines, and then it FALLS THROUGH
        // into the ordinary tail (spoils, the item roll, the den discovery),
        // with only the XP award and the "too weak" pair gated out below.
        let boss = matches!(opponent_kind, 3 | 4);
        if opponent_kind == 3 {
            // 1000:513f/5142 then 1000:5145 FUN_1000_2526(1).
            self.progress.xp = self.progress.threshold;
            progress::apply_levels(&mut self.progress, &mut self.player, &mut self.rng, 0, true);
            // CS 0x3924 / 0x3965, printed 1000:515f and 1000:517d, ReadKeys
            // at 1000:5164 and 1000:5182.
            for line in ending::ENDING_3 {
                term::println(line);
                term::read_key(lines);
            }
        } else {
            // 1000:519d -- the ordinary `^2Враг сдох.`, in the ELSE of the
            // `param_1 == 3` test, so neither boss fight prints it.
            term::println("^2Враг сдох.");
        }
        // 1000:51a6 `cmp byte [bp+0x4],0x3` / 1000:51ac `cmp byte [bp+0x4],0x4`
        // -- both boss values skip the award line AND the 1000:51e9 add.
        let award = if boss {
            0
        } else {
            progress::xp_award(self.player.level, &enemy)
        };
        if !boss {
            term::println(&text::fill(
                "^6За отпин врага ты получаешь # качков опыта",
                &[award as i64],
            ));
        }
        // 1000:51ed..1000:5238: the award is added first, and only then is
        // `xp >= threshold` tested -- `progress::apply_levels` does both, so
        // the branch that has to be reproduced here is the OTHER one, the
        // two lines printed at 1000:5202 (file 0x528A) and 1000:521b (file
        // 0x52C8) when the award was not enough.
        // The test is `1000:51ed`..`1000:51f4` -- `mov ax,[0x38ce]` /
        // `cmp ax,[0x38d0]` / `jge 0x5238`, evaluated on the xp AFTER the add
        // at 1000:51e9. Taken from the numbers rather than from whether
        // `apply_levels` reported a level: at MAX_LEVEL it reports none while
        // the original still takes the `jge` arm and prints nothing here.
        let short_of_the_threshold = self.progress.xp.wrapping_add(award) < self.progress.threshold;
        progress::apply_levels(
            &mut self.progress,
            &mut self.player,
            &mut self.rng,
            award,
            false,
        );
        // 1000:51f6 / 1000:51fc -- the same pair of boss compares guards the
        // "too weak an opponent" lines as guards the award above.
        if short_of_the_threshold && !boss {
            term::println("^6Ты запинал слишком слабого мудака для увеличения понтовости");
            term::println(&text::fill(
                "^6Сейчас у тебя # качков опыта, А для прокачки надо #",
                &[self.progress.xp as i64, self.progress.threshold as i64],
            ));
        }
        self.claim_spoils(&enemy);
        // 1000:57ce `cmp byte [bp+0x4],0x6` / 1000:57d2 `jnz 0x5838` -- the
        // den errand's reward, the LAST thing the function does before the
        // loop-exit test, so it sits after the item table `claim_spoils`
        // walks. `district * 20` понтовость and `district * 10` xp, and the
        // xp add is followed by 1000:5835 `FUN_1000_2526(0)` -- the capped
        // level drain, which SPENDS DRAWS when the award crosses the
        // threshold. That is why this block is not text-only.
        if opponent_kind == 6 {
            let district = i32::from(self.district);
            // 1000:57d4..1000:57de `add [0x38cb],ax`, `ax = district*20`.
            self.pontovost_street = self.pontovost_street.wrapping_add((district * 20) as i16);
            // CS 0x3c99 (`district * 20`, pushed 1000:57e7, printed
            // 1000:57fe) and CS 0x3ce9 (`district * 10`, pushed 1000:5808,
            // printed 1000:581f).
            for (mult, line) in ending::ERRAND_AWARDS {
                term::println(&text::fill(line, &[i64::from(district * mult)]));
            }
            // 1000:5824..1000:582e `add [0x38ce],ax` then 1000:5832/5835
            // `FUN_1000_2526(district*10 & 0xff00)` -- the high byte of a
            // value at most 50, i.e. 0. `apply_levels` adds the award and
            // then runs the same capped drain, which is the pair.
            progress::apply_levels(
                &mut self.progress,
                &mut self.player,
                &mut self.rng,
                (district * 10) as u16,
                false,
            );
        }

        // No promotion here. `1000:3d11` ends at its own `ret`; the district
        // gate is `1000:ab75`, at the TOP of the next turn, and Task 21 moved
        // this port's copy of it there ([`Game::district_advance`]). A level
        // won in this fight therefore promotes on the following turn, not
        // inside the post-fight block -- and one district per turn, because
        // `ab75`..`ad12` has no back edge.
        Ok(())
    }

    /// `1000:adbf`..`1000:ae1f` -- the chapter-5 endgame arm's flag stores
    /// and its three announcement lines. Called from
    /// [`Game::district_advance`], which since Task 21 IS the port of the
    /// original's per-turn preamble `1000:ab75`..`1000:ad12` and sits at the
    /// top of [`Game::run`]'s loop -- so the call site is now the original's
    /// own position in the turn (this arm is the direct continuation of that
    /// preamble; `docs/re/wander.md` calls `1000:ab75`..`1000:ae18` "the
    /// genuine district-transition block").
    ///
    /// **The frequency matches the original, and an earlier revision of this
    /// comment claimed otherwise.** It said `1000:adbf`'s `cmp al,5` was
    /// unconditional, so the whole arm repeated every turn. Flow refutes
    /// that: at district 5 `1000:ab8d`'s `jb 0xab92` is not taken and
    /// `1000:ab8f e9 86 02 jmp 0xae18` skips `1000:ad12`..`1000:adbf`
    /// outright, so `adbf` is only reachable on a promotion turn -- the one
    /// branch into `0xad12` is `1000:ac5b` and the one branch into `0xadbf`
    /// is `1000:ad89`, both post-increment, and `1000:adc3`, `1000:addc` and
    /// `1000:ae13` have no branch targeting them at all. So the three
    /// prints, the `ReadKey` and the `[0x3c83]` store run exactly once in
    /// the original too, which is what this method does. See the section
    /// below for what genuinely does repeat.
    ///
    /// This is the **per-turn** trigger, reached only while the game is
    /// already running: it fires the turn `self.district` first becomes 5.
    /// It is not the only original site that arms `rector_showdown` --
    /// `1000:7364`, inside `FUN_1000_6a0d`, does so once at game **entry**
    /// (new character or loaded save) when district is already 5 at that
    /// point, and is ported in [`Game::apply_class_bonus`], not here. See
    /// that method's doc for why the two are different original addresses
    /// doing the same store.
    ///
    /// **Established from flow**, re-disassembled for this task:
    ///
    /// ```text
    /// adbf  cmp al,5 / jnz 0xae18   ; chapter == district, [0x3692]
    /// adc3  WriteLn file 0x9CF2     ; ^1Пора наконец отомстить ректору...
    /// addc  call 0f16:031a          ; ReadKey -- a blocking keypress, ported
    /// ade1  WriteLn file 0x9D16     ; ^1Ты пробрался в универ...
    /// adfa  WriteLn file 0x9D4E     ; ^1А вот и он...
    /// ae13  mov byte [0x3c83],1     ; rector_showdown
    /// ae18  cmp byte [0x3c83],1 / jnz 0xae3c  ; always taken -- ae13 wrote
    ///       the exact byte this reads, five bytes later
    /// ae1f  mov byte [0x3696],1     ; Den
    /// ae24  mov al,0 / push ax / call 0x11c2 (ae27) ; FUN_1000_11c2(0)
    /// ae2a  mov al,3 / push ax / call 0x3d11 (ae2d) ; the rector fight
    /// ae30  mov al,1 / push ax / call 0x11c2 (ae33) ; FUN_1000_11c2(1)
    /// ae36  mov al,4 / push ax / call 0x3d11 (ae39) ; the endgame fight
    /// ```
    ///
    /// **`0f16:031a` is `ReadKey`, not `Delay`** (`docs/re/rtl.md:494`;
    /// `Delay` is the unrelated `0f16:02a8`). An earlier revision of this
    /// comment mislabelled it and dropped it as "no state" -- wrong on both
    /// counts: `ReadKey` blocks for one keystroke (`int 0x16`, confirmed by
    /// decoding `0f16:031a` directly) and its return value is discarded by
    /// the caller (nothing after `addc` reads `al`), so it is a pure
    /// input-stream synchronisation point, not a no-op. Ported the same way
    /// `src/persist.rs`'s `choose_slot` already substitutes for a
    /// `ReadKey`: this port has no raw-key input, so it consumes one line
    /// from `lines` and discards it, matching the original's "one keystroke,
    /// value unused" shape as closely as a line-based port can.
    ///
    /// **The four calls at `ae27`..`ae39` are ported, in
    /// [`Game::rector_endgame`] -- `docs/re/port-gaps.md` rows 7 and 13.**
    /// `FUN_1000_11c2` was traced by Task 40: 50 instructions, 178 bytes
    /// (`0x11c2`..`0x1273`, prologue through the 3-byte `ret 0x2`), no
    /// branch besides its own two argument arms, no draw, and no call
    /// besides the `0f78:02cd` stack-check prologue every Pascal procedure
    /// carries -- storing a fixed stat block into the enemy record
    /// `20ae:3952..396e` -- the same fields [`Game::roll_enemy`] fills for a
    /// rolled encounter -- selecting one of two blocks on its argument.
    /// Both blocks match `data/enemies.json`'s `rektor_ngu_v0` (arg 0) and
    /// `rektor_ngu_v1` (arg 1) exactly, including the derived
    /// `hpmax := 5*vitality + strength + 10` and
    /// `dmg_min, dmg_max := strength/2, strength` this port's own
    /// `roll_enemy` already computes the same way. So `FUN_1000_11c2` itself
    /// is not the obstacle to porting these two fights.
    ///
    /// What blocked them until this batch was `FUN_1000_3d11`'s own
    /// `param_1`, which [`Game::run_combat`] now models in full: the 3 and 4
    /// openers, the `param_1 == 4` ending (`1000:5085`, which reaches the
    /// end screen through the marquee at `1000:5133` -- `call 0xaec`, not
    /// `FUN_1000_074b(1)` as an earlier revision of this comment said), the
    /// `param_1 == 3` fake-out, and the two XP gates at `1000:51a6` /
    /// `1000:51f6`.
    ///
    /// **`1000:ae18`'s arm runs every turn, this one does not**, and that is
    /// why the two are separate methods.
    /// `ab75` really is the loop top -- `1000:ee01 e9 71 bd jmp 0xab75` is
    /// the only branch INSTRUCTION in the image targeting it, and
    /// `1000:ab72 e8 98 be call 0x6a0d` is a three-byte near call whose next
    /// instruction is `ab75`, the one-time fall-through entry -- but at
    /// district 5 the block leaves it immediately:
    ///
    /// ```text
    /// ab8d  72 03           jb 0xab92     ; not taken once [0x3692] == 5
    /// ab8f  e9 86 02        jmp 0xae18    ; ad12..adbf skipped entirely
    /// ...
    /// ae18  80 3e 83 3c 01  cmp byte [0x3c83],1   ; nothing ever clears it
    /// ae1d  75 1d           jnz 0xae3c
    /// ae1f  c6 06 96 36 01  mov byte [0x3696],1   ; the Den -- idempotent
    /// ae27/ae2d/ae33/ae39   the four calls        ; THESE repeat
    /// ```
    ///
    /// The three prints, the `1000:addc` `ReadKey` and the `1000:ae13` store
    /// are on the other side of that jump and run exactly once, which is
    /// what this method does. Branch-target scans over the whole image
    /// (`docs/re/gaps.md`) find one branch into `0xad12` (`1000:ac5b`), one
    /// into `0xadbf` (`1000:ad89`), both post-increment, and none at all
    /// into `0xadc3`, `0xaddc` or `0xae13`; `1000:adbd eb 59 jmp short
    /// 0xae18` precedes `adbf`, so it is not a fall-through either.
    ///
    /// **A raw byte scan alone gets the `ab75` half wrong, and Task 21
    /// caught how.** Scanning every `jmp`/`Jcc`/`call`/`loop` encoding of
    /// that target returns TWO hits, and the second, `1000:ab00` `72 73`,
    /// scores 63 of 64 votes in the alignment sweep -- yet it is the `rs` of
    /// `^4Gopnik: ^7version 1.02 june,` inside the CS literal pool, the
    /// `0x82b3`..`0xab59` gap `data/functions.json` leaves between
    /// `FUN_1000_7c67` and `entry`. This is `docs/re/METHODOLOGY.md`'s
    /// `1000:d83b` lesson on a second address: alignment never answers yes.
    ///
    /// **So the only thing the port refuses here is the four calls** -- in
    /// practice the two fights, since `FUN_1000_11c2` merely fills the enemy
    /// record. The reason is `FUN_1000_3d11`'s `param_1`, above, and nothing
    /// else: an earlier revision of this comment argued that repeating the
    /// arm "would nag the player every turn with an announcement of two
    /// fights the port then does not run", which was built on the false
    /// claim that the announcement repeats. That argument is withdrawn.
    /// Recorded in `docs/re/gaps.md`, "The district-advance autosave --
    /// wired (Task 21)".
    fn enter_district_5(&mut self, lines: &mut dyn Iterator<Item = io::Result<String>>) {
        term::println("^1Пора наконец отомстить ректору...");
        // 1000:addc -- ReadKey, blocking for one keystroke whose value is
        // never read afterward. `lines.next()` is this port's line-based
        // stand-in (see the doc comment above); `None` at EOF is treated the
        // same as any other discarded keystroke.
        term::read_key(lines);
        term::println("^1Ты пробрался в универ, в тёмный ректорский кабинет...");
        term::println("^1А вот и он...");
        self.rector_showdown = true;
        self.places.mark_found(Location::Den);
    }

    /// `1000:ae18`..`1000:ae3c` -- the two rector fights, the pair of calls
    /// that makes the game finishable.
    ///
    /// ```text
    /// ae18  cmp byte [0x3c83],1 / jnz 0xae3c   ; rector_showdown
    /// ae1f  mov byte [0x3696],1                ; the Den, idempotent
    /// ae27  call 0x11c2                        ; FUN_1000_11c2(0)
    /// ae2d  call 0x3d11                        ; FUN_1000_3d11(3)
    /// ae33  call 0x11c2                        ; FUN_1000_11c2(1)
    /// ae39  call 0x3d11                        ; FUN_1000_3d11(4)
    /// ```
    ///
    /// `FUN_1000_11c2` is the enemy record's constructor and nothing else:
    /// class 10 for both, then the argument's own block, then the four
    /// derived fields (`dmg_min = str/2`, `dmg_max = str`,
    /// `hpmax = 5*vit + str + 10`, `hp = hpmax`) and a clear of the two
    /// break flags and the three spoils. `data/enemies.json`'s
    /// `rektor_ngu_v0` / `rektor_ngu_v1` carry every one of those constants
    /// including the derived ones, so `Enemy::to_fighter` IS the port of the
    /// function and the gap was only ever the missing caller
    /// (`docs/re/port-gaps.md` row 13). `Fighter::default()` supplies the
    /// cleared flags and spoils.
    ///
    /// **It repeats.** Nothing clears `[0x3c83]`, so a player who flees both
    /// fights meets them again on the next turn -- `1000:ae18` is read at
    /// the top of every street turn. The only exits are death (the end
    /// screen halts) and beating the second rector (the marquee halts).
    fn rector_endgame(
        &mut self,
        lines: &mut dyn Iterator<Item = io::Result<String>>,
    ) -> io::Result<()> {
        if !self.rector_showdown {
            return Ok(());
        }
        // 1000:ae1f.
        self.places.mark_found(Location::Den);
        // 1000:ae27 FUN_1000_11c2(0) then 1000:ae2d FUN_1000_3d11(3).
        self.run_combat(3, Self::boss("rektor_ngu_v0"), lines)?;
        if !self.running {
            return Ok(());
        }
        // 1000:ae33 FUN_1000_11c2(1) then 1000:ae39 FUN_1000_3d11(4).
        self.run_combat(4, Self::boss("rektor_ngu_v1"), lines)?;
        Ok(())
    }

    /// The scripted stat block `FUN_1000_11c2` writes, by its
    /// `data/enemies.json` id.
    ///
    /// Panics on an id the table has no *scripted* row for. That is not a
    /// reachable branch: the two ids below are the only callers and
    /// `tests/data_load.rs` already asserts both rows carry `stats`. A
    /// silent fallback here would turn a missing table row into a fight
    /// against a zeroed enemy, which is worse than a loud stop.
    fn boss(id: &str) -> Fighter {
        data::enemies()
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| e.to_fighter())
            .unwrap_or_else(|| panic!("data/enemies.json has no scripted row `{id}`"))
    }

    /// `1000:4fba`..`1000:5051` -- the hospital rescue that turns a death
    /// into a survivable turn. Returns `true` when it fired, i.e. the player
    /// lives and the fight is left.
    ///
    /// **Established from flow.** `1000:4fba` `cmp byte [0x3696],1` (the den
    /// flag) and `1000:4fc4` `cmp word [0x38cb],0xa` / `jge` (street cred at
    /// least 10) are the two gates; anything else falls to `1000:5053` and
    /// the end screen. The body, in order:
    ///
    /// ```text
    /// 4fce  file 0x50DF, `^1Тебе повезло знакомые пацаны отвезли тебя в больницу` ...
    /// 4fe7  83 2e cb 38 0a   sub word [0x38cb],10
    /// 4fec  a1 ae 38 / 99    mov ax,[0x38ae] / cdq        ; hpmax as a real
    /// 4ff0  call 0f78:1125                                ; int -> real
    /// 4ff5  cx=0x83 si=0 di=0x2000 / call 0f78:1117       ; divide by 5.0
    /// 5002  cx=0x82 si=0 di=0x4000 / call 0f78:1111       ; multiply by 3.0
    /// 500f  call 0f78:1131                                ; Round
    /// 5014  29 06 c7 38      sub [0x38c7],ax              ; the bill
    /// 5018  a1 ae 38 / a3 ac 38   hp := hpmax
    /// 501e  a jaw or a leg broken -> `sub [0x38c7],7` and clear BOTH
    /// 503b  money < 0 -> `[0x38cb] += [0x38c7]`, `[0x38c7] := 0`
    /// ```
    ///
    /// **The bill is `Round(hpmax * 3 / 5)`, and that needs no exponent
    /// bias.** `0f78:1117` is the divide and `0f78:1111` the multiply, and
    /// each constant's significand is fixed by its `di` word alone because
    /// `1000:4ff8` and `1000:5005` zero the low mantissa half -- so the two
    /// exponent bytes differ by exactly one step whatever the bias is, the
    /// ratio is `(1.5 / 1.25) * 2^-1 = 0.6`, and the bias cancels.
    /// `docs/re/combat-dispatch.md`, "The bill does not need the exponent
    /// bias", is the argument in full.
    ///
    /// An earlier revision of this comment read the two constants as `5.0`
    /// and `3.0` and called them "decoded, not guessed". Those decimals
    /// assume a bias of 129, which `docs/re/rtl.md` records as **not
    /// established** and which `docs/re/combat.md` was corrected in this
    /// branch to say so; they are one consistent pair, not the only one. The
    /// computed bill is identical either way -- what was wrong was the tier,
    /// and a `src/` doc comment is read as a citation
    /// (`docs/re/METHODOLOGY.md`).
    ///
    /// `Round` is Borland's, half away from zero -- [`Self::round_half`].
    /// Rounding is unambiguous here: `3h/5` is never exactly a half-integer,
    /// since `6h = 10k + 5` has no solution.
    ///
    /// **No draw**: there is no `9a 4b 11 78 0f` anywhere in
    /// `1000:4f82`..`1000:5077`.
    fn hospital_rescue(&mut self) -> bool {
        if !self.places.is_found(Location::Den) || self.pontovost_street < 10 {
            return false;
        }
        term::println("^1Тебе повезло знакомые пацаны отвезли тебя в больницу а то бы ты сдох.");
        self.pontovost_street = self.pontovost_street.wrapping_sub(10);
        // Round(hpmax / 5 * 3), computed exactly rather than through two
        // truncating divisions: `round_half(x)` rounds `x / 2` half away from
        // zero, so feeding it `6 * hpmax / 5` as a doubled numerator gives
        // `Round(3 * hpmax / 5)`. (The half case never arises -- `3h/5` has
        // fractional part 0, .2, .4, .6 or .8 -- but the rounding is written
        // the original's way rather than assumed away.)
        let bill = Self::round_half(6 * i32::from(self.player.hpmax) / 5);
        self.player.money = self.player.money.wrapping_sub(bill as i16);
        self.player.hp = self.player.hpmax;
        // The two limb tests are one DISJUNCTION, and the original writes it
        // as two branches into the same block at 1000:502c: 1000:501e
        // `cmp byte [0x38b0],0x1` / 1000:5023 `jz 0x502c` enters on a broken
        // jaw, and 1000:5025 `cmp byte [0x38b1],0x1` / 1000:502a
        // `jnz 0x503b` leaves only when the leg is unbroken too.
        if self.player.broken_jaw || self.player.broken_leg {
            self.player.money = self.player.money.wrapping_sub(7_i16);
            self.player.broken_jaw = false;
            self.player.broken_leg = false;
        }
        // 1000:503b `cmp word [0x38c7],0x0` / 1000:5040 `jnl 0x5051` -- a
        // purse driven NEGATIVE by the bill is settled out of street cred
        // (1000:5042..1000:504e), and the test is signed and strict, so an
        // exact zero skips the block.
        if self.player.money < 0 {
            self.pontovost_street = self.pontovost_street.wrapping_add(self.player.money);
            self.player.money = 0;
        }
        true
    }

    /// `run` at the fight prompt -- `[1000:48eb, 1000:4af7]`. Returns `true`
    /// when the arm reached `1000:4af7 mov byte [bp-0x1],1`, i.e. the fight
    /// is over.
    ///
    /// Two refusals leave the fight running; both `jmp 0x4afb`, the next step
    /// of the chain, so a refused flee is still followed by every compare
    /// after `run`.
    ///
    /// **No draw**: there is no `9a 4b 11 78 0f` anywhere in
    /// `[1000:48eb, 1000:4afb)`. That is what makes run A turn 7 of
    /// `data/rng_trace.json` -- a cop fight entered and fled -- show zero
    /// draws between `1000:b792` and the next turn's `1000:af68`.
    fn flee(&mut self) -> bool {
        // 1000:48eb `cmp byte [0x3c83],1` / `jnz 0x490e`, CS 0x33bf.
        if self.rector_showdown {
            term::println("^4Ректор: Кудa? Стоять! Бейся до конца трусливый урод!");
            return false;
        }
        // 1000:490e `cmp byte [0x38b1],1` / `jnz 0x4931`, CS 0x33f6.
        if self.player.broken_leg {
            term::println("^4Ты не можешь убежать на сломаной ноге.");
            return false;
        }
        // 1000:4931 `cmp word [0x38a6],0` / `jnle 0x493b`.
        if self.player.level > 0 {
            self.flee_penalty();
        } else {
            // 1000:4ade, CS 0x349f -- at level 0 there is nothing to take.
            term::println("^4Враг: Засранец!");
        }
        true
    }

    /// The flee penalty -- `[1000:493b, 1000:4adc]`, one level given back.
    ///
    /// `docs/re/combat.md` recorded this as "replayed in reverse when the
    /// player flees (`1000:499a`)" and `run_combat`'s own doc as "this port
    /// carries no growth log, so the penalty is not applied". Task 17
    /// corrected the first (`1000:499a` is the `^4Сила -1 ` literal push, the
    /// codes are **inverted** rather than walked backwards, and the loop runs
    /// forward); this method is what closes the second.
    /// [`crate::progress::undo_growth`] carries the per-code table and
    /// [`crate::progress::demote`] the three steps after it.
    ///
    /// The middle block is here rather than in `crate::progress` because it
    /// reads the district and the discovery flags:
    ///
    /// * `1000:4a87 cmp word [0x389c],5` / `jz 0x4ac3` -- class 5 skips it.
    /// * `1000:4a8e`..`1000:4aa3` computes `level - (district - 1) * 10` and
    ///   tests it against 3 with `cmp ax,3` / `jnz 0x4ac3` -- **equality**,
    ///   where the post-kill twin in [`Game::claim_spoils`] (`1000:52ae`)
    ///   uses `jl` on the same expression.
    /// * on equality, `1000:4aa5 mov byte [0x3696],0x1` **sets** the den flag
    ///   while `1000:4aaa` writes `^4Такого конявого непустят в местный
    ///   притон!` (CS `0x3472`) -- an announcement that the player is now too
    ///   shabby for the den, granting den access. That is the original's own
    ///   behaviour, not a decode error: `20ae:3696` is a boolean whose every
    ///   immediate store image-wide is a 0 or a 1, `1000:d80c` is the gate
    ///   that reads it, and this is one of the stores of 1
    ///   (`docs/re/combat-dispatch.md`). The store is reproduced as written.
    ///
    /// Unlike `claim_spoils`, there is no "already discovered" gate on the
    /// store or on the line, so both happen again on a second flee at the
    /// same measured level.
    fn flee_penalty(&mut self) {
        // 1000:493b, CS 0x341f -- `call 0eed:0x0`, no newline, so the stat
        // lines run on from it.
        term::print("^4Враг: Трусливый засранец! ");
        for stat in progress::undo_growth(&mut self.progress, &mut self.player) {
            term::print(match stat {
                // 1000:499a / 49ee / 4a17 / 4a54, CS 0x343c / 3447 / 3456 /
                // 3466. All four are `call 0eed:0x0` too.
                progress::Stat::Strength => "^4Сила -1 ",
                progress::Stat::Agility => "^4Ловкость -1 ",
                progress::Stat::Vitality => "^4Живучесть -1 ",
                progress::Stat::Luck => "^4Удача -1 ",
            });
        }
        // 1000:4a78..1000:4a82 -- a bare `WriteLn` on the Text at 20ae:3fcc,
        // which closes the line the four writes above left open.
        term::println("");
        if self.player.class != 5
            && i32::from(self.player.level) - (i32::from(self.district) - 1) * 10 == 3
        {
            self.places.mark_found(Location::Den);
            term::println("^4Такого конявого непустят в местный притон!");
        }
        progress::demote(&mut self.progress, &mut self.player);
    }

    /// `v` at the fight prompt -- `[1000:4cb4, 1000:4d93)`.
    ///
    /// The arm and the status line are two blocks, not one: every arm of the
    /// first falls through to the second, so a refused call still gets a
    /// countdown line if a countdown is already running.
    /// [`crate::combat_dispatch::Backup`] carries both.
    ///
    /// **No draw**: there is no `9a 4b 11 78 0f` in `[1000:4cb4, 1000:4d93)`.
    fn backup_in_fight(&mut self, backup: &mut Backup) {
        match backup.call(
            self.places.is_found(Location::Den),
            self.pontovost_street,
            self.district,
            self.has_mobile,
        ) {
            // 1000:4ce8, CS 0x35c8 -- WITH the trailing dot, unlike
            // 1000:4c87's copy.
            Called::ByPhone => term::println("^2Подошли пацаны - Ща начнется!."),
            // 1000:4d0a, CS 0x35e9.
            Called::NobodyWillBackYou => term::println("^4Ни кто не хочет за тебя впрягаться."),
            // 1000:4d25, CS 0x360f.
            Called::NoDen => term::println("^6Сначала надо скорешиться с местной гопотой."),
            // 1000:4cd5 sets the counter and writes nothing; the status line
            // below is what the player sees.
            Called::OnTheWay => {}
        }
        match backup.status(self.has_mobile) {
            // 1000:4d4c, CS 0x363d, with 1000:4d51/1000:4d54's `3 - counter`.
            Status::KicksToHold(n) => term::println(&text::fill(
                "^6Тебе надо продержатся до подхода братвы # пинка.",
                &[i64::from(n)],
            )),
            // 1000:4d7a, CS 0x3670.
            Status::TheyAreHere => term::println("^2Они уже здесь."),
            Status::Nothing => {}
        }
    }

    /// `[1000:4d93, 1000:4e9e)` -- the gopota swing, on every prompt once
    /// they have arrived. [`crate::combat_dispatch::backup_round`] carries
    /// the arithmetic, the two draws and the argument for not porting
    /// `1000:4e2a`.
    fn backup_attacks(&mut self, backup: &mut Backup, ehp: &mut i32, enemy: &Fighter) {
        let Some(fought) = combat_dispatch::backup_round(
            &mut self.rng,
            backup,
            self.district,
            enemy.armor,
            *ehp,
            &mut self.pontovost_street,
        ) else {
            return;
        };
        *ehp = fought.enemy_hp_after;
        // 1000:4df3, CS 0x3681. 1000:4df8/1000:4dfb push `hp_before - hp_now`
        // and 1000:4e00 the remainder.
        term::println(&text::fill(
            "^2Врага отпинали на #з. У него осталось #",
            &[i64::from(fought.damage), i64::from(*ehp)],
        ));
        // 1000:4e4f, CS 0x36bd.
        if fought.beaten {
            term::println("^2Твою подмогу отпинали.");
        }
        // 1000:4e85, CS 0x36d6.
        if fought.gave_up {
            term::println("^4Подмоге надоело столько парится из-за мало понтового мудака");
        }
    }

    /// `f` at the fight prompt -- `[1000:4eb2, 1000:4f82)`.
    /// [`crate::combat_dispatch::fire`] carries the gates, the hit test and
    /// the damage.
    fn shoot_in_fight(&mut self, ehp: &mut i32) {
        match combat_dispatch::fire(
            &mut self.rng,
            &mut self.pistol,
            self.harder_encounters,
            self.player.agility,
        ) {
            // 1000:4eb9 jumps straight to the death test: an accepted verb
            // that prints nothing at all.
            Shot::NoPistol => {}
            // 1000:4eca, CS 0x3716 -- the game's own typo for "Нельзя".
            Shot::NotHere => term::println("^6Тельзя тут стрелять! Менты накроют!"),
            // 1000:4f69, CS 0x37a5.
            Shot::NoCartridges => term::println("^6Чё за батва? Блин патроны кончились!"),
            // 1000:4f4e, CS 0x3789.
            Shot::Miss => term::println("^2Это был хреновый выстрел."),
            Shot::Hit { damage } => {
                *ehp -= i32::from(damage);
                // 1000:4f2c, CS 0x373c. 1000:4f31/1000:4f34 push the
                // difference, 1000:4f39 the remainder and 1000:4f3d the
                // cartridges LEFT -- 1000:4eed has already spent one.
                term::println(&text::fill(
                    "^2Ты выстрелил и ранил врага на #з. У него осталось #з., осталось патронов #",
                    &[
                        i64::from(damage),
                        i64::from(*ehp),
                        i64::from(self.pistol.cartridges),
                    ],
                ));
            }
        }
    }

    /// `1000:523e`..`1000:57cc` -- everything the victory block does after
    /// the XP award, in the order the instructions do it.
    ///
    /// **Established from flow**, disassembled forward from `1000:5189`.
    /// `docs/re/progression.md` already carried the shape of this block and
    /// `data/xp.json`'s `post_kill_stat_events` the one-shot deltas; the
    /// addresses below were re-derived from `orig/g.exe` for this
    /// implementation and every `Random` site named carries the
    /// `9a 4b 11 78 0f` signature.
    ///
    /// | address | what |
    /// |---|---|
    /// | `1000:523e` | `[0x38c3] += [0x396a]`, `[0x38c7] += [0x396c]`, `[0x38c9] += [0x396e]` -- the loot |
    /// | `1000:526c` | `hp += 5`, clamped to `hpmax` at `1000:5271` |
    /// | `1000:5280` | `[0x38cb] += enemy.class + 1 + enemy.level div 3` |
    /// | `1000:5295` | den flag, when `level - (district-1)*10 >= 3` (`1000:52ae` `cmp ax,3` / `jl`) |
    /// | `1000:52d5` | `Random(30)`; only `0` (`or ax,ax` / `jbe`) reaches the one-shot gift chain |
    /// | `1000:5402` | `Random(district*25)`; `luck >= r` AND enemy class 2 -> `1000:5427` `Random(3)` joints |
    /// | `1000:5454` | `Random(district*40)`; `luck >= r` -> a class-keyed item, each arm with its own draw |
    ///
    /// Both luck comparisons are Borland's 32-bit pair with the roll
    /// **zero**-extended (`xor dx,dx` at `1000:5407` / `1000:5459`) and luck
    /// **sign**-extended (`cwd` at `1000:5410` / `1000:5462`), taken as
    /// `luck >= roll` -- `jg` on the high words, then `jb`/`jae` on the low.
    /// This port widens both sides with `i32::from(u16)`, exactly as
    /// [`Game::walk`] does at `1000:b5fc`, so it never reproduces the
    /// negative reading a `luck` with bit 15 set would get.
    fn claim_spoils(&mut self, enemy: &Fighter) {
        // 1000:523e..1000:5251 -- three `mov ax,[enemy] / add [player],ax`
        // pairs. `docs/re/gaps.md` recorded this as NOT reproduced; it is now.
        self.player.beer_dl += enemy.beer_dl;
        self.player.money = self.player.money.wrapping_add(enemy.money);
        self.player.junk += enemy.junk;
        // file 0x52FE
        term::println(spoils::EMITTED[0].1);
        // 1000:5274 `cmp ax,[0x38ae]` / 1000:5278 `jle 0x5280` -- the +5 is
        // stored only while it stays at or below hpmax; above it, 1000:527a
        // stores hpmax instead.
        self.player.hp = (self.player.hp + 5).min(self.player.hpmax);
        // 1000:5291 `add [0x38cb],ax` -- a word add, so it wraps.
        self.pontovost_street = self
            .pontovost_street
            .wrapping_add((enemy.class + 1 + enemy.level / 3) as i16);
        // 1000:5295..1000:52cc. `[0x3692]` is the district; the level is
        // measured within it, so three levels into a district opens the den.
        if !self.places.is_found(Location::Den)
            && i32::from(self.player.level) - (i32::from(self.district) - 1) * 10 >= 3
        {
            self.places.mark_found(Location::Den);
            term::println(spoils::EMITTED[1].1);
        }
        // 1000:52da `or ax,ax` / 1000:52dc `jbe 0x52e1` -- only a 0 out of
        // Random(30) reaches the gift chain; anything else takes 1000:52de
        // `jmp 0x53f7`.
        if self.rng.below(30) == 0 {
            self.grant_oneshot_gift();
        }
        // 1000:53f7..1000:5444 -- the Нарк's joints.
        let roll = self.rng.below(u16::from(self.district) * 25);
        // Borland's 32-bit compare again, three branches for one predicate:
        // 1000:5411 `cmp dx,bx` / 1000:5413 `jnle 0x541b` (luck's high half
        // above the roll's -> pass) / 1000:5415 `jl 0x5449` (below -> fail),
        // then 1000:5417 `cmp ax,cx` / 1000:5419 `jb 0x5449` on the low
        // halves, UNSIGNED. Then 1000:541e `cmp ax,0x2` / 1000:5421
        // `jnz 0x5449` -- only a Нарк carries one.
        if i32::from(self.player.luck) >= i32::from(roll) && enemy.class == 2 {
            // 1000:5427 `add [0x38c5],ax` -- a word add of the draw.
            self.player.joints += self.rng.below(3) as i16;
            term::println(spoils::EMITTED[7].1); // file 0x540B
        }
        // 1000:5449..1000:57cc -- the class-keyed item table.
        let roll = self.rng.below(u16::from(self.district) * 40);
        // The same 32-bit shape with the senses swapped: 1000:5463
        // `cmp dx,bx` / 1000:5465 `jnle 0x5473` (high half above -> on to
        // the table) / 1000:5467 `jnl 0x546c`, then 1000:546c `cmp ax,cx` /
        // 1000:546e `jnb 0x5473`. Every failing path is 1000:5469 /
        // 1000:5470 `jmp 0x57ce`, the function's tail.
        if i32::from(self.player.luck) < i32::from(roll) {
            return;
        }
        // The class-keyed item table, in the original's own chain order.
        match enemy.class {
            // 1000:5476 `cmp ax,0x1` / 1000:5479 `jz 0x547e`
            1 => self.spoil_charm(),
            // 1000:5515 `cmp ax,0x3` / 1000:5518 `jz 0x552c`
            // 1000:551a `cmp ax,0x4` / 1000:551d `jz 0x552c`
            // 1000:551f `cmp ax,0x5` / 1000:5522 `jz 0x552c`
            // 1000:5524 `cmp ax,0x6` / 1000:5527 `jz 0x552c`
            3..=6 => self.spoil_club(),
            // 1000:560e `cmp ax,0x7` / 1000:5611 `jnz 0x5675`
            7 => self.spoil_glasses(),
            // 1000:5675 `cmp ax,0x9` / 1000:5678 `jz 0x567d`
            9 => self.spoil_blade(),
            // Class 0, 2 and 8 (and anything above 9) reach 1000:57ce with
            // no table at all -- the chain names no arm for them.
            _ => {}
        }
    }

    /// `1000:52e1`..`1000:53f2` -- the first one-shot gift that has not
    /// fired yet, on `Random(30) == 0`.
    ///
    /// **Established from flow, and byte-identical to the church's copy**:
    /// `docs/re/progression.md` records that the 56 bytes at `1000:8101` and
    /// at `1000:532f` compare equal through each block's `c6 06 bf 38 01`
    /// flag store. `Game::church`'s arm 2 is the same three grants; the
    /// deltas are `data/xp.json`'s `post_kill_stat_events`.
    ///
    /// The preamble line (file `0x535E`) is printed when ANY of the three is
    /// still unfired. The original writes that disjunction as three tests
    /// with the first two jumping onto one common target and the third
    /// leaving: `1000:52e1` `cmp byte [0x38bf],0x0` / `1000:52e6`
    /// `jz 0x52f6`, `1000:52e8` `cmp byte [0x38c0],0x0` / `1000:52ed`
    /// `jz 0x52f6`, and `1000:52ef` `cmp byte [0x38c1],0x0` / `1000:52f4`
    /// `jnz 0x530f`. (An earlier revision of this sentence called
    /// `1000:52ed` one of three compares; it is the second `je`.)
    fn grant_oneshot_gift(&mut self) {
        if !self.oneshot_gift_1 || !self.oneshot_gift_2 || !self.ring_gospodi_pomilui {
            term::println(spoils::EMITTED[2].1);
        }
        // The three-way chain below is a separate set of tests on the same
        // three flags, and it is an if/else: 1000:530f `cmp byte [0x38bf],0x0`
        // / 1000:5314 `jnz 0x536a`, 1000:536a `cmp byte [0x38c0],0x0` /
        // 1000:536f `jnz 0x53b9`, 1000:53b9 `cmp byte [0x38c1],0x0` /
        // 1000:53be `jnz 0x53f7`.
        if !self.oneshot_gift_1 {
            term::println(spoils::EMITTED[3].1);
            self.player.strength += 1;
            self.player.agility += 1;
            self.player.vitality += 1;
            self.player.luck += 1;
            self.player.hpmax += 6;
            self.player.hp += 6;
            self.player.dmg_max += 1;
            if self.player.strength.is_multiple_of(2) {
                self.player.dmg_min += 1; // 1000:534d..1000:5361
            }
            self.oneshot_gift_1 = true;
        } else if !self.oneshot_gift_2 {
            term::println(spoils::EMITTED[4].1);
            self.player.strength += 4;
            self.player.agility += 4;
            self.player.vitality += 4;
            self.player.luck += 4;
            self.player.hpmax += 24;
            self.player.hp += 24;
            self.player.dmg_max += 4;
            self.player.dmg_min += 2;
            self.oneshot_gift_2 = true;
        } else if !self.ring_gospodi_pomilui {
            term::println(spoils::EMITTED[5].1);
            term::println(spoils::EMITTED[6].1);
            self.ring_gospodi_pomilui = true;
        }
    }

    /// Enemy class 1 (Нефор): `1000:547e`..`1000:5512`, `Random(3)`.
    fn spoil_charm(&mut self) {
        match self.rng.below(3) {
            // 1000:5487 `cmp ax,0x0` / 1000:548a `jnz 0x54b8`
            0 => {
                // 1000:548c gate, 1000:5493 `add [0x38a4],2`, 1000:54b1 flag.
                if !self.charm_krestik {
                    self.player.luck += 2;
                    term::println(spoils::EMITTED[8].1);
                    self.charm_krestik = true;
                }
            }
            // 1000:54b8 `cmp ax,0x1` / 1000:54bb `jnz 0x54e8`
            1 => {
                // 1000:54bd gate, 1000:54c4 `inc [0x38a4]`, 1000:54e1 flag.
                if !self.charm_ring {
                    self.player.luck += 1;
                    term::println(spoils::EMITTED[9].1);
                    self.charm_ring = true;
                }
            }
            // 1000:54e8 `cmp ax,0x2` / 1000:54eb `jnz 0x5512` -- the chain's
            // last link drops anything else and this `_` arm does not, but
            // `Random(3)` at 1000:5482 returns 0..2 (`port_equivalences`).
            _ => {
                // 1000:54ed gate, 1000:550d flag. No stat change.
                if !self.has_mobile {
                    term::println(spoils::EMITTED[10].1);
                    self.has_mobile = true;
                }
            }
        }
    }

    /// Enemy classes 3..6: `1000:552c`..`1000:560b`, `Random(2)`.
    ///
    /// Both arms grant a weapon and both add to `dmg_min`/`dmg_max` only when
    /// no BETTER weapon is already owned -- the "better" set differs between
    /// them, which is why the two are written out rather than folded.
    fn spoil_club(&mut self) {
        match self.rng.below(2) {
            // 1000:5535 `cmp ax,0x0` / 1000:5538 `jnz 0x559b`
            0 => {
                // 1000:553a `cmp byte [0x38ba],0x0` / 1000:553f `jnz 0x5599`
                // -- the кастет is granted only once.
                if self.weapon_kastet {
                    return;
                }
                self.weapon_kastet = true; // 1000:5541
                term::println(spoils::EMITTED[11].1);
                // 1000:555f/1000:5566/1000:556d -- ножик, дубинка, тесак.
                if !self.weapon_nozhik && !self.weapon_dubinka && !self.weapon_tesak {
                    self.player.dmg_min += 2; // 1000:5574
                    self.player.dmg_max += 2;
                } else {
                    term::println(spoils::EMITTED[12].1);
                }
            }
            // 1000:559b `cmp ax,0x1` / 1000:559e `jnz 0x560b` -- the last
            // link drops anything else where this `_` does not; `Random(2)`
            // at 1000:5530 returns 0..1 (`port_equivalences`).
            _ => {
                if self.weapon_dubinka {
                    return;
                }
                self.weapon_dubinka = true; // 1000:55a7
                term::println(spoils::EMITTED[13].1);
                // 1000:55c5/1000:55cc -- ножик, тесак.
                if self.weapon_nozhik || self.weapon_tesak {
                    term::println(spoils::EMITTED[14].1);
                } else if self.weapon_kastet {
                    self.player.dmg_min += 2; // 1000:55da
                    self.player.dmg_max += 2;
                } else {
                    self.player.dmg_min += 4; // 1000:55e6
                    self.player.dmg_max += 4;
                }
            }
        }
    }

    /// Enemy class 7 (Беспредельщик): `1000:5613`..`1000:5672`, `Random(2)`.
    fn spoil_glasses(&mut self) {
        match self.rng.below(2) {
            // 1000:561c `cmp ax,0x0` / 1000:561f `jnz 0x5648`
            0 => {
                // 1000:5621 gate, 1000:5628 flag. No stat change.
                if !self.dark_glasses {
                    self.dark_glasses = true;
                    term::println(spoils::EMITTED[15].1);
                }
            }
            // 1000:5648 `cmp ax,0x1` / 1000:564b `jnz 0x5672` -- the same
            // last-link widening as the other draw-keyed tables above.
            _ => {
                // 1000:564d gate, 1000:566d flag.
                if !self.has_mobile {
                    term::println(spoils::EMITTED[16].1);
                    self.has_mobile = true;
                }
            }
        }
    }

    /// Enemy class 9 (Маньячок): `1000:567d`..`1000:57cc`, `Random(2)`.
    ///
    /// The damage terms are a chain of independent `if`s, not a `match`:
    /// each arm can add more than one of them. `1000:56b6` `mov al,1` /
    /// `or al,al` / `jz` is a never-taken branch the compiler left in, so
    /// the first term's condition is only what follows it.
    fn spoil_blade(&mut self) {
        match self.rng.below(2) {
            // 1000:5686 `cmp ax,0x0` / 1000:5689 `jz 0x568e`
            0 => {
                if self.weapon_nozhik {
                    return;
                }
                self.weapon_nozhik = true; // 1000:5698
                term::println(spoils::EMITTED[17].1);
                // 1000:56bc..1000:56cd: al := (394b == 0); `cmp al,[0x38ba]`.
                if !self.weapon_dubinka == self.weapon_kastet {
                    self.player.dmg_min += 4; // 1000:56cf
                    self.player.dmg_max += 4;
                }
                // 1000:56d9 `cmp byte [0x394b],0x0` / 1000:56de `jz 0x56ea`
                if self.weapon_dubinka {
                    self.player.dmg_min += 2; // 1000:56e0
                    self.player.dmg_max += 2;
                }
                // Three conjuncts, three branches onto one target 0x5709:
                // 1000:56ea / 1000:56ef `jnz 0x5709`, 1000:56f1 / 1000:56f6
                // `jnz 0x5709`, 1000:56f8 / 1000:56fd `jnz 0x5709`.
                if !self.weapon_kastet && !self.weapon_dubinka && !self.weapon_tesak {
                    self.player.dmg_min += 6; // 1000:56ff
                    self.player.dmg_max += 6;
                }
                // 1000:5709 `cmp byte [0x394c],0x0` / 1000:570e `jz 0x5729`
                if self.weapon_tesak {
                    term::println(spoils::EMITTED[18].1); // file 0x5516
                }
            }
            // 1000:572c `cmp ax,0x1` / 1000:572f `jz 0x5734`. The `_` arm is
            // wider than that last link -- the original drops anything else;
            // `Random(2)` at 1000:5681 returns 0..1.
            _ => {
                if self.weapon_tesak {
                    return;
                }
                self.weapon_tesak = true; // 1000:573e
                term::println(spoils::EMITTED[19].1);
                // 1000:5762..1000:577a: al := (394b == 0 && 38c2 == 0). Its
                // two halves are 1000:5762 `cmp byte [0x394b],0x0` /
                // 1000:5767 `jnz 0x5770` and 1000:5769
                // `cmp byte [0x38c2],0x0` / 1000:576e `jz 0x5774`;
                // 1000:5776 `cmp al,[0x38ba]` / 1000:577a `jnz 0x5786` is
                // the comparison itself. The `mov al,1` / `or al,al` / `jz`
                // at `1000:575c` is the тесак's copy of the never-taken
                // artefact the doc above names, and is deliberately NOT
                // cited: a port cannot evaluate a condition that is
                // constant-true, and writing its address beside code that
                // does something else would be a false citation.
                // `docs/re/gaps.md`, "Two never-taken branches ... are
                // permanently excluded from citation".
                if (!self.weapon_dubinka && !self.weapon_nozhik) == self.weapon_kastet {
                    self.player.dmg_min += 7; // 1000:577c
                    self.player.dmg_max += 7;
                }
                // 1000:5786 `cmp byte [0x394b],0x0` / 1000:578b `jz 0x579e`
                // and 1000:578d `cmp byte [0x38c2],0x0` / 1000:5792
                // `jnz 0x579e` -- one conjunction, two branches.
                if self.weapon_dubinka && !self.weapon_nozhik {
                    self.player.dmg_min += 5; // 1000:5794
                    self.player.dmg_max += 5;
                }
                // 1000:579e `cmp byte [0x38c2],0x0` / 1000:57a3 `jz 0x57af`
                if self.weapon_nozhik {
                    self.player.dmg_min += 3; // 1000:57a5
                    self.player.dmg_max += 3;
                }
                // Three conjuncts onto 0x57ce: 1000:57af / 1000:57b4
                // `jnz 0x57ce`, 1000:57b6 / 1000:57bb `jnz 0x57ce`,
                // 1000:57bd / 1000:57c2 `jnz 0x57ce`.
                if !self.weapon_kastet && !self.weapon_dubinka && !self.weapon_nozhik {
                    self.player.dmg_min += 9; // 1000:57c4
                    self.player.dmg_max += 9;
                }
            }
        }
    }

    /// `1000:40f2`..`1000:4168` -- the crowd that gathers around a long
    /// fight, and the two draws it spends.
    ///
    /// **Established from flow**, disassembled from `1000:40ed` (the
    /// `c6 86 ed fe 00` that zeroes the counter) forward, so every address
    /// below sits on a confirmed instruction boundary; both call sites carry
    /// the `9a 4b 11 78 0f` signature.
    ///
    /// ```text
    /// 40ed  c6 86 ed fe 00   mov byte [bp-0x113],0     ; once per fight
    /// 40f2  80 be ed fe 05   cmp byte [bp-0x113],5     ; loop top
    /// 40f7  73 24            jae 0x411d                ; already 5: no inc
    /// 40f9  fe 86 ed fe      inc byte [bp-0x113]
    /// 40fd  80 be ed fe 05   cmp byte [bp-0x113],5
    /// 4102  75 19            jne 0x411d
    /// 4104  bf 74 2e         mov di,0x2e74             ; file 0x4744
    /// 411d  80 3e 83 3c 00   cmp byte [0x3c83],0       ; the rector flag
    /// 4122  74 03            je 0x4127 / jmp 0x43f6
    /// 4127  80 be ed fe 05   cmp byte [bp-0x113],5
    /// 412c  74 03            je 0x4131 / jmp 0x43f6
    /// 4131  b8 0a 00 / 50    mov ax,10 / push ax
    /// 4135  9a 4b 11 78 0f   call Random               ; nonzero -> 0x43f6
    /// 4141  b8 12 00 / 50    mov ax,18 / push ax
    /// 4145  9a 4b 11 78 0f   call Random               ; picks the line
    /// ```
    ///
    /// The counter stops at 5 (`jae` skips the `inc`), so `== 5` stays true
    /// for every later prompt: **`Random(10)` fires at every `Битва\` prompt
    /// from the fifth onward**, not once.
    ///
    /// **Corroborated by state.** Every run in `data/combat_trace.json`
    /// spends exactly `sum(max(0, prompts - 4))` draws at `1000:4135`,
    /// counted from the capture itself: run A's single 30-prompt fight
    /// shows **26**, run B's six fights of 8/5/4/4/3/3 prompts show **5**
    /// (4+1+0+0+0+0), run C's three of 4/5/3 show **1**, and run D's five
    /// one-prompt fleeing fights show **0**. Per-fight, not per-session, and
    /// per-prompt, not per-fight. `tests/combat_sequence.rs` is what holds
    /// that to the port, draw for draw.
    ///
    /// Called BEFORE the prompt is written, because `1000:43f6` (the
    /// `^0Битва\` write) is what this block falls through to.
    fn crowd(&mut self, prompts_seen: &mut u8) {
        if *prompts_seen < 5 {
            *prompts_seen += 1;
            // 1000:40fd `cmp byte [bp-0x113],0x5` / 1000:4102 `jnz 0x411d`
            // -- the line prints on the turn the counter REACHES 5, once.
            if *prompts_seen == 5 {
                // file 0x4744
                term::println("^7Начинают собираться зрители");
            }
        }
        // 1000:411d `cmp byte [0x3c83],0` / `jz 0x4127` -- the rector
        // showdown has no spectators. The gate sits AFTER the counter block
        // at 1000:40f2, so `^7Начинают собираться зрители` still prints and
        // only the taunts (and their two draws) are suppressed.
        // [`Game::enter_district_5`] (Task 20) sets [`Game::rector_showdown`]
        // once `self.district` reaches 5.
        if self.rector_showdown {
            return;
        }
        // 1000:4127 `cmp byte [bp-0x113],0x5` / 1000:412c `jz 0x4131` --
        // the same counter tested a SECOND time, after the rector gate, and
        // this one leaves for the prompt at 1000:43f6 on any other value.
        if *prompts_seen != 5 {
            return;
        }
        // 1000:413a `or ax,ax` / 1000:413c `jz 0x4141` -- only a 0 out of
        // Random(10) reaches the taunt draw.
        if self.rng.below(10) != 0 {
            return;
        }
        let which = self.rng.below(18);
        // The eighteen lines at code offsets 0x2e92..0x314d (files
        // 0x4762..0x4A1D), in the order the `cmp ax,N` chain at 1000:414a
        // onwards tests them. Two are built from a name: 4 splices the
        // PLAYER'S RANK name (`[0x389c] * 0x100 + 0x2e`, the DS:002e table
        // `data/enemies.json` carries) and 17 the player's own name
        // (`DS:379c`).
        // The eighteen links of the `cmp ax,N` chain, in the original's own
        // order: each arm below carries the compare that selects it and the
        // `jnz` that moves on to the next.
        match which {
            // 1000:414a `cmp ax,0x0` / 1000:414d `jnz 0x416b`
            0 => term::println("Зрители:^6Мочи его, мочи!"),
            // 1000:416b `cmp ax,0x1` / 1000:416e `jnz 0x418c`
            1 => term::println("Зрители:^6Врежь ему!"),
            // 1000:418c `cmp ax,0x2` / 1000:418f `jnz 0x41ad`
            2 => term::println("Зрители:^6Блин долго ты ещё будешь мудиться?"),
            // 1000:41ad `cmp ax,0x3` / 1000:41b0 `jnz 0x41ce`
            3 => term::println("Зрители:^6Да вы только посмотрите на эти пинки!"),
            // 1000:41ce `cmp ax,0x4` / 1000:41d1 `jnz 0x4217`
            4 => {
                term::print("Зрители:^6Не подкачай ");
                term::print(&Self::rank_name(self.player.class));
                term::println(", я на тебя трёшку поставил!");
            }
            // 1000:4217 `cmp ax,0x5` / 1000:421a `jnz 0x4238`
            5 => term::println("Зрители:^6Чё-тут за батва?"),
            // 1000:4238 `cmp ax,0x6` / 1000:423b `jnz 0x4259`
            6 => term::println("Зрители:^6Я знаю вон того мудака, он уже нескольких запинал!"),
            // 1000:4259 `cmp ax,0x7` / 1000:425c `jnz 0x427a`
            7 => term::println("Зрители:^6Чё так слабо бьёшь?! Пинай сильнее!"),
            // 1000:427a `cmp ax,0x8` / 1000:427d `jnz 0x42b4`
            8 => {
                term::println("Зрители:^6Дерьмово дерётесь придурки");
                term::println("^2А ты: Заткнись мудак, а то щас тебя запинаю!");
            }
            // 1000:42b4 `cmp ax,0x9` / 1000:42b7 `jnz 0x42d5`
            9 => term::println(
                "Зрители:^6Да, а помнишь мы вчера также одного пинали, пинали.. \
                 А потом подошла его братва..",
            ),
            // 1000:42d5 `cmp ax,0xa` / 1000:42d8 `jnz 0x42f6`
            10 => term::println("Зрители:^6Это чё реслинг?"),
            // 1000:42f6 `cmp ax,0xb` / 1000:42f9 `jnz 0x4317`
            11 => term::println("Зрители:^6Двинь ему в рыло!"),
            // 1000:4317 `cmp ax,0xc` / 1000:431a `jnz 0x4338`
            12 => term::println("Зрители:^6И куда менты смотрят?"),
            // 1000:4338 `cmp ax,0xd` / 1000:433b `jnz 0x4359`
            13 => term::println("Зрители:^6Пинай!"),
            // 1000:4359 `cmp ax,0xe` / 1000:435c `jnz 0x4379`
            14 => term::println("Зрители:^6Врежь гаду!"),
            // 1000:4379 `cmp ax,0xf` / 1000:437c `jnz 0x4399`
            15 => term::println("Зрители:^6Господа делайте ваши ставки!"),
            // 1000:4399 `cmp ax,0x10` / 1000:439c `jnz 0x43b9`
            16 => term::println("Зрители:^6Ну чё там? Какой счет?"),
            // 1000:43b9 `cmp ax,0x11` / 1000:43bc `jnz 0x43f6`. The `_` arm
            // is wider than the original's last link, which drops anything
            // above 17; `Random(18)` at 1000:4145 cannot produce one.
            _ => {
                term::print("Зрители:^6Ну и кого там ");
                term::print(&self.player.name);
                term::println(" ^6сегодня пинает?");
            }
        }
    }

    /// The rank name at `DS:002e + class * 0x100` -- the same eleven-row
    /// table `1000:13dc`..`1000:13e4` indexes for the enemy's display name,
    /// which `data/enemies.json` carries one row per class of.
    pub(crate) fn rank_name(class: u16) -> String {
        data::enemies()
            .iter()
            .find(|e| e.class == class)
            .map(|e| e.name.to_string())
            .unwrap_or_else(|| panic!("data/enemies.json has no row for class {class}"))
    }

    /// One round of blows, both sides, using the already-verified
    /// blows-per-round budget and per-blow resolution from `crate::combat`.
    ///
    /// Per-blow messages are `docs/re/combat.md`'s own cited strings, quoted
    /// here with the markup they actually carry, and every file offset below
    /// was decoded from `orig/g.exe` as a length-prefixed CP866 string: miss
    /// (`^4Ты промазал` file `0x4B13`, `^2Враг промазал` file `0x4C49`), hit
    /// (`^2Ты пнул врага на #з. У него осталось #` file `0x4AEA`, and its
    /// mirror `^4Он пнул тебя на #з. У тебя осталось #` file `0x4C21`), and
    /// break (`^2Ты сломал врагу челюсть. ^4Враг: А! козёл!` /
    /// `^2Ты сломал врагу ногу. ^4Враг: Ну что за урод!` files
    /// `0x4A8D`/`0x4ABA`, whose inner `^4` is part of the string, and the
    /// mirrors `^4Враг сломал тебе челюсть.` / `^4Враг сломал тебе ногу.`
    /// files `0x4B95`/`0x4C08`).
    ///
    /// Task 13 added the lines the `Random(3)` crit pick and the зубная
    /// защита choose between, which this port previously did not print at
    /// all or printed only the first arm of:
    ///
    /// * the player's crit trio, picked by `1000:44e3` and printed at
    ///   `1000:44ed`/`1000:450d`/`1000:452d` -- `^2Точный удар!!!`,
    ///   `^2Не хило приложил!!!`, `^2Двойной урон!!!` (files `0x4A54`,
    ///   `0x4A65`, `0x4A7B`);
    /// * the enemy's, picked by `1000:4706` and printed at
    ///   `1000:4710`/`1000:4730`/`1000:4750` -- `^4Враг:Сдохни урод!!`,
    ///   `^4Тебе не хило врезали!`, `^4Враг:Получи гнида!!` (files `0x4B52`,
    ///   `0x4B67`, `0x4B7F`);
    /// * the two зубная защита arms the `Random(4)` at `1000:47fe` picks
    ///   between -- `^4Враг сломал тебе челюсть, даже защита не помогла.`
    ///   (`1000:4807`, file `0x4BB1`) on a 0, and
    ///   `^2Защита спасла твои кривые клыки.` (`1000:4827`, file `0x4BE5`)
    ///   otherwise;
    /// * the two "ещё раз" lines, `^2Из-за большой ловкости ты можешь пнуть` ...
    ///   (`1000:4639`, file `0x4B21`) and its enemy mirror,
    ///   `^4Из-за большой ловкости враг может пнуть ещё раз` (`1000:48ad`, file `0x4C59`),
    ///   whose guards are NOT mirror images -- see the
    ///   comment on the enemy loop's tail below.
    fn combat_round(&mut self, enemy: &mut Fighter) {
        // Both loops' exits are SIGNED tests on a defender's hp word, and the
        // two are not the same test:
        //
        //   1000:4629  cmp word [0x3962],0 / jg 0x4632   leave at enemy hp <= 0
        //   1000:4659  cmp word [0x3962],0 / jl 0x4663   ... and again at < 0
        //   1000:48cd  cmp word [0x38ac],0 / jl 0x48d7   leave at player hp < 0
        //
        // so a defender sitting at EXACTLY 0 stops the player's loop and does
        // NOT stop the enemy's -- the enemy swings again, and that swing costs
        // draws. `Fighter::hp` is a `u16` this port saturates at 0, which
        // cannot tell "exactly 0" from "would have gone negative", so the
        // running hp is kept here as an `i32` and the loop exits are driven
        // from it. Only the STORED value saturates; see `docs/re/gaps.md`,
        // "Opened by Task 13", for what that still costs.
        let mut ehp = i32::from(enemy.hp);
        let player_blows = blows_per_round(&self.player, enemy);
        for i in 0..player_blows {
            if ehp <= 0 {
                break;
            }
            let blow = resolve_blow_nth(&mut self.rng, &self.player, enemy, i, Swing::player());
            if !blow.hit {
                term::println("^4Ты промазал");
                continue;
            }
            // 1000:44ed/1000:450d/1000:452d -- the crit's `Random(3)` picks
            // ONE of three lines (files 0x4A54, 0x4A65, 0x4A7B). This port
            // used to draw it and print the first line whatever it returned.
            match blow.taunt {
                // 1000:44e8 `cmp ax,0x0` / 1000:44eb `jnz 0x4508`
                Some(0) => term::println("^2Точный удар!!!"),
                // 1000:4508 `cmp ax,0x1` / 1000:450b `jnz 0x4528`
                Some(1) => term::println("^2Не хило приложил!!!"),
                // 1000:4528 `cmp ax,0x2` / 1000:452b `jnz 0x4546` -- the
                // chain's last link DROPS anything else, where this arm
                // accepts it. `Random(3)` at 1000:44e3 cannot return one;
                // `data/combat_uncited.json`'s `port_equivalences` states
                // the assumption.
                Some(_) => term::println("^2Двойной урон!!!"),
                None => {}
            }
            ehp -= i32::from(blow.damage);
            enemy.hp = ehp.max(0) as u16;
            term::println(&text::fill(
                "^2Ты пнул врага на #з. У него осталось #",
                &[blow.damage as i64, ehp as i64],
            ));
            // 1000:459e/1000:45be and 1000:45c5/1000:45e5: the message is
            // suppressed when that limb is ALREADY broken, and the flag is
            // set on the enemy's record. This port printed unconditionally
            // and never set either flag -- the enemy's `20ae:3966`/`3967` in
            // `data/combat_trace.json`'s per-round channel is what caught it.
            match blow.broke {
                Some(Break::Jaw) if !enemy.broken_jaw => {
                    enemy.broken_jaw = true;
                    term::println("^2Ты сломал врагу челюсть. ^4Враг: А! козёл!");
                }
                Some(Break::Leg) if !enemy.broken_leg => {
                    enemy.broken_leg = true;
                    term::println("^2Ты сломал врагу ногу. ^4Враг: Ну что за урод!");
                }
                // Already broken, or no break at all: 1000:45a3 and 1000:45ca
                // jump past the message, leaving the flag as it was.
                _ => {}
            }
            // 1000:4624 subtracts 18; 1000:4629 `cmp word [0x3962],0` /
            // `jg 0x4632` leaves the loop with NO message when the enemy is
            // down; 1000:4639 prints file 0x4B21 when the budget still has
            // room (`cmp [bp-0x10e],0` / `jle 0x4652` at 1000:4632).
            if ehp > 0 && i + 1 < player_blows {
                term::println("^2Из-за большой ловкости ты можешь пнуть ещё раз");
            }
        }
        // 1000:4675 `cmp word [0x3962],0` / `jg 0x467f` -- the enemy swings
        // only if it is still up; anything else jumps straight to the verb
        // dispatch at 1000:48d7.
        if ehp <= 0 {
            return;
        }
        let mut php = i32::from(self.player.hp);
        let enemy_blows = blows_per_round(enemy, &self.player);
        for i in 0..enemy_blows {
            if php < 0 {
                break;
            }
            let blow = resolve_blow_nth(
                &mut self.rng,
                enemy,
                &self.player,
                i,
                Swing::enemy(self.tooth_guard),
            );
            if !blow.hit {
                term::println("^2Враг промазал");
                continue;
            }
            // 1000:4710/1000:4730/1000:4750 -- the enemy's copy of the crit
            // lines (files 0x4B52, 0x4B67, 0x4B7F). This port printed nothing
            // at all for an enemy crit.
            match blow.taunt {
                // 1000:470b `cmp ax,0x0` / 1000:470e `jnz 0x472b`
                Some(0) => term::println("^4Враг:Сдохни урод!!"),
                // 1000:472b `cmp ax,0x1` / 1000:472e `jnz 0x474b`
                Some(1) => term::println("^4Тебе не хило врезали!"),
                // 1000:474b `cmp ax,0x2` / 1000:474e `jnz 0x4769` -- the
                // same last-link widening as the player's copy above.
                Some(_) => term::println("^4Враг:Получи гнида!!"),
                None => {}
            }
            php -= i32::from(blow.damage);
            self.player.hp = php.max(0) as u16;
            term::println(&text::fill(
                "^4Он пнул тебя на #з. У тебя осталось #",
                &[blow.damage as i64, php as i64],
            ));
            match blow.broke {
                // 1000:47c7..1000:4840. Without the зубная защита this is
                // the plain `cmp byte [0x38b0],0` gate at 1000:47c7 plus the
                // set at 1000:47ee; with it, the `Random(4)` at 1000:47fe has
                // already been drawn inside `resolve_blow_nth` and its result
                // is what picks between the two lines here.
                Some(Break::Jaw) => match blow.jaw_guard {
                    Some(true) => {
                        // 1000:4807, file 0x4BB1, then 1000:4820 sets it.
                        self.player.broken_jaw = true;
                        term::println("^4Враг сломал тебе челюсть, даже защита не помогла.");
                    }
                    // 1000:4827, file 0x4BE5 -- the jaw is NOT broken.
                    Some(false) => term::println("^2Защита спасла твои кривые клыки."),
                    None => {
                        if !self.player.broken_jaw {
                            self.player.broken_jaw = true;
                            term::println("^4Враг сломал тебе челюсть.");
                        }
                    }
                },
                // 1000:4842 `cmp byte [0x38b1],0` / `jnz 0x4867`: same
                // suppression as the jaw's.
                Some(Break::Leg) if !self.player.broken_leg => {
                    self.player.broken_leg = true;
                    term::println("^4Враг сломал тебе ногу.");
                }
                _ => {}
            }
            // 1000:48a1 subtracts 18 and 1000:48a6 `cmp [bp-0x10e],0` /
            // `jle 0x48c6` guards file 0x4C59 -- and that is ALL it guards.
            // The player half has a defender-is-down test ahead of its
            // message (1000:4629) and this one does not, so the two "mirror"
            // halves really do differ here: the enemy announces another blow
            // even on the swing that finished the player.
            if i + 1 < enemy_blows {
                term::println("^4Из-за большой ловкости враг может пнуть ещё раз");
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

/// Which of the game's **two** `kos` handlers is running.
///
/// The image carries the joint handler twice: once at `1000:e97d`, dispatched
/// by `entry`'s compare at `1000:e973` against its input buffer `DS:3972`,
/// and once at `1000:4b17`, dispatched by `FUN_1000_3d11`'s own compare at
/// `1000:4b0d` against the combat buffer `DS:3a72`. That second copy is why
/// `kos` works mid-fight without going through `crate::commands::parse`'s
/// street table.
///
/// **Established from flow.** Both bodies are 269 bytes -- `1000:4b17` and
/// `1000:e97d`, each ending in its own `call 0eed:01c2` (`1000:4c1f` and
/// `1000:ea85`) -- and compared byte for byte they differ in exactly **15**
/// places: seven `mov di,imm16` string operands pointing at the
/// combat string pool instead of the top-level one, and one immediate --
/// `1000:4b52` `c6 06 cd 38 03` against `1000:e9b8` `c6 06 cd 38 0a`. Every
/// guard, every stat grant and the whole heal split are the identical
/// instruction sequence, so the only two things that vary are modelled here.
///
/// Six of the seven strings are byte-identical between the pools. The seventh
/// is not, and the difference is one letter: the combat copy at file `0x4DF0`
/// ends "косяков", the top-level copy at file `0xBF5E` ends "косякова". Both
/// are quoted verbatim -- a typo in the original is not this port's to fix,
/// and neither is a typo the original made only once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Joint {
    /// `1000:e97d`, reached from the street prompt.
    Street,
    /// `1000:4b17`, reached from `^0Битва\`.
    Fight,
}

impl Joint {
    /// The value written to the stoned countdown `20ae:38cd`: `1000:e9b8`
    /// stores 10, `1000:4b52` stores 3.
    fn buff_turns(self) -> u8 {
        match self {
            Joint::Street => 10,
            Joint::Fight => 3,
        }
    }

    /// The single combined heal line, used when the hp shortfall is >= 10.
    /// The two pools disagree by one trailing letter; the other six strings
    /// this handler prints are identical and are written inline.
    fn long_heal_line(self) -> &'static str {
        match self {
            // file 0xBF5E, loaded by `bf 8e a6` at 1000:ea1e.
            Joint::Street => crate::gym::EMITTED[24].1,
            // file 0x4DF0, loaded by `bf 20 35` at 1000:4bb8.
            Joint::Fight => "^2Колёса прибавляют #з. Здоровья:#/#. Осталось # косяков",
        }
    }
}

/// The literal pool for the street command list -- `1000:ea94`..`1000:ec7d`, in the image's
/// ADDRESS order, the order `tools/difftest.py`'s `literal_walk` reads them
/// in. The span's last five bytes are trimmed: they push the NEXT verb's
/// key literal, which a call past the end consumes, so a walk including
/// them reports a literal nothing in the span takes.
///
/// `docs/re/port-gaps.md` recorded that the club and gym rest on their
/// module-local unit tests, with `difftest` carrying their MENU rows and
/// nothing else. This pool is the arm bodies' half of that comparison.
///
/// `(closes, text)` -- `closes` is true for a `WriteLn`, false for a
/// `Write` the next literal continues.
pub const COMMAND_LIST: [(bool, &str); 17] = [
    (
        true,
        "Напиши: ^6w^7    чтобы шататься по окрестностям - искать на свою жопу приключения",
    ), // 1000:ea9e
    (true, "Напиши: ^6mar^7  чтобы идти на рынок"), // 1000:eabe
    (true, "Напиши: ^6bmar^7 чтобы идти к барыгам"), // 1000:eade
    (true, "Напиши: ^6rep^7  чтобы идти к ветеринару"), // 1000:eafe
    (true, "Напиши: ^6girl^7 чтобы завалиться к своей девчонке"), // 1000:eb1e
    (true, "Напиши: ^6pr^7   чтобы идти в местный притон гопоты"), // 1000:eb3e
    (true, "Напиши: ^6kl^7   чтобы идти в клуб"),   // 1000:eb5e
    (true, "Напиши: ^6trn^7  чтобы идти в качалку"), // 1000:eb7e
    (
        true,
        "Напиши: ^6s^7    чтобы посмотреть в лужу на свою уродскую рожу",
    ), // 1000:eb97
    (
        true,
        "Напиши: ^6sv^7   чтобы приглядеться к пинаемому мудаку",
    ), // 1000:ebb0
    (
        true,
        "Напиши: ^6k^7    чтобы гасить мудака который тебе попался на дороге",
    ), // 1000:ebc9
    (true, "Напиши: ^6v^7    чтобы позвать подкрепление"), // 1000:ebe2
    (true, "Напиши: ^6kos^7  чтобы схавать косяк"), // 1000:ebfb
    (
        true,
        "Напиши: ^6h^7    чтобы выпить пиво (если не охото к ветеринару)",
    ), // 1000:ec14
    (true, "Напиши: ^6mh^7   чтобы набухаться до чёртиков"), // 1000:ec2d
    (true, "Напиши: ^6name^7 чтобы сменить погоняло"), // 1000:ec46
    (true, "Напиши: ^6e^7    если захочешь выйти"), // 1000:ec5f
];

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
    fn new_game_starts_on_the_street_with_only_the_vet_and_market() {
        let g = game();
        assert_eq!(g.location, Location::Street);
        assert_eq!(g.mode, Mode::Street);
        assert_eq!(g.district, 1);
        assert!(g.places.is_found(Location::Street));
        // 1000:6dc3 and 1000:6dc8.
        assert!(g.places.is_found(Location::Vet));
        assert!(g.places.is_found(Location::Market));
        // Nothing else: 1000:6dbe writes exactly those two flags.
        for loc in crate::locations::TRACKED {
            if matches!(loc, Location::Vet | Location::Market) {
                continue;
            }
            assert!(
                !g.places.is_found(loc),
                "{loc:?} must not be discovered at character creation"
            );
        }
    }

    /// The two flags a fresh character gets are exactly the ones
    /// `1000:6dbe`'s block writes, and reaching them needs no `Random` draw:
    /// a brand-new game can walk straight into `mar` and `rep`.
    #[test]
    fn new_game_can_enter_the_market_and_the_vet_without_discovering_them() {
        let mut g = game();
        g.dispatch(Command::Market, &mut no_input()).unwrap();
        assert_eq!(g.location, Location::Market);
        assert_eq!(g.mode, Mode::Shop(Location::Market));

        // A WOUNDED character, because the vet's loop top at `1000:d4ba`
        // ejects a whole one on entry (`crate::vet::loop_top`). What is
        // under test here is the discovery gate at `1000:d3b0`, not the
        // health test behind it.
        let mut g = game();
        g.player.hp = 1;
        g.dispatch(Command::Vet, &mut no_input()).unwrap();
        assert_eq!(g.location, Location::Vet);
        assert_eq!(g.mode, Mode::Shop(Location::Vet));
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
    /// the seven flags at `20ae:3694`..`369a` from character creation
    /// (`1000:6dc3`/`1000:6dc8`), the wander path, `girl` (`1000:d751`) and
    /// the progression reveals (`1000:73c3`..`1000:73e0`) -- never from a
    /// failed entry. The gym is used here because it is one of the five
    /// flags `1000:6dbe` leaves clear.
    #[test]
    fn entering_an_undiscovered_place_does_not_discover_it() {
        let mut g = game();
        for _ in 0..3 {
            g.dispatch(Command::Gym, &mut no_input()).unwrap();
            assert_eq!(g.location, Location::Street);
            assert_eq!(g.mode, Mode::Street);
            assert!(
                !g.places.is_found(Location::Gym),
                "a refused entry must not mark the place found"
            );
        }
    }

    /// The opener runs on `param_1` 0 and 6 and on nothing else --
    /// `1000:3d27`/`1000:3d29` and `1000:3d2b`/`1000:3d2d` against
    /// `1000:3d2f jmp 0x3e8d`. The arm's own contents are
    /// `crate::combat_opener`'s tests; what this one pins is the GATE, and
    /// the position of the greeting in the fight's output.
    #[test]
    fn the_class_keyed_opener_runs_only_for_param_1_zero_and_six() {
        let greeted = |kind: u8| {
            let mut g = game();
            g.player.level = 0; // 1000:4ade: `run` leaves with no penalty.
            term::capture::lines(|| {
                g.run_combat(kind, punchbag(), &mut input(&["run"]))
                    .unwrap();
            })
        };
        // punchbag() is class 0, so the arm at 1000:3d44 is the one that runs.
        for kind in [0u8, 6] {
            assert_eq!(
                &greeted(kind)[..2],
                ["Слышь Вась..", "^4А чё ваще?"],
                "param_1 {kind} takes 1000:3d32 and greets first"
            );
        }
        // The five values the outer chain sends to 1000:3e8d and beyond.
        // 2 is the club's (`1000:e222`) and 5 the den job's (`1000:ddfc`).
        for kind in [1u8, 2, 3, 4, 5] {
            let out = greeted(kind);
            assert!(
                !out.iter().any(|l| l.contains("Слышь Вась")),
                "param_1 {kind} takes 1000:3d2f and must not greet: {out:?}"
            );
        }
    }

    /// The two agility-reduction lines, at the position `1000:3ff5` and
    /// `1000:4098` put them: after the opener and before the first
    /// `^0Битва\` prompt.
    ///
    /// The pair of numbers is `crate::combat::budget_report`'s test; what
    /// this one pins is that `run_combat` prints them at all, with the
    /// records the right way round -- the ENEMY's line is the one that reads
    /// `[0x3956] + 4` (`1000:3fec`), so a swap would put the player's
    /// numbers on the enemy's string.
    #[test]
    fn the_agility_reduction_lines_print_before_the_first_prompt() {
        let mut g = game();
        g.player.level = 0;
        g.player.agility = 120;
        let enemy = Fighter {
            agility: 50,
            ..punchbag()
        };
        let out = term::capture::lines(|| {
            g.run_combat(0, enemy, &mut input(&["run"])).unwrap();
        });
        // Class 0, so the opener's two lines come first (1000:3d44).
        assert_eq!(&out[..2], ["Слышь Вась..", "^4А чё ваще?"]);
        // 1000:4013, CS 0x2dec -- the enemy's budget, cut by the player's
        // agility: 50 -> 1 blow instead of 3.
        assert_eq!(
            out[2],
            "^2Из-за твоей хорошей ловкости враг сможет пнуть тебя раз 1 вместо 3"
        );
        // 1000:40b6, CS 0x2e31 -- the mirror: 120 -> 5 instead of 7.
        assert_eq!(
            out[3],
            "^4Из-за хорошей ловкости врага ты сможешь пнуть его раз 5 вместо 7"
        );
        // ... and the prompt is next, so nothing was printed between.
        assert!(out[4].starts_with("^0Битва\\"), "{:?}", out[4]);

        // Neither line prints when the reduction costs no blow -- the same
        // fight with an agility-0 enemy leaves the prompt directly after the
        // opener.
        let mut g = game();
        g.player.level = 0;
        let out = term::capture::lines(|| {
            g.run_combat(0, punchbag(), &mut input(&["run"])).unwrap();
        });
        assert!(
            !out.iter().any(|l| l.contains("вместо")),
            "1000:4011 / 1000:40b4 refuse both lines: {out:?}"
        );
    }

    // --- Task 18: the rest of the in-combat dispatcher ---------------------

    /// A fight the player can stand in indefinitely: the enemy has a lot of
    /// hp and no agility, so nothing but the verb under test moves the
    /// numbers these tests read.
    fn punchbag() -> Fighter {
        Fighter {
            name: "Мудак".to_string(),
            hp: 500,
            hpmax: 500,
            ..Fighter::default()
        }
    }

    /// A game whose player can actually call for backup: `1000:4cb4` wants
    /// the den flag and `1000:4cc8` wants `cred >= district * 10 + 10`.
    fn game_with_gopota() -> Game {
        let mut g = game();
        g.places.mark_found(Location::Den);
        g.pontovost_street = 500;
        g.player.hp = 10_000;
        g.player.hpmax = 10_000;
        g
    }

    /// The shot lands on the enemy record, and its 20..=29 is subtracted with
    /// no armour term (`1000:4f28`) -- so an enemy in full armour loses
    /// exactly as much as a naked one from the same seed.
    #[test]
    fn the_pistol_ignores_the_enemy_armour() {
        let hit = |armor: u8| {
            let mut g = game();
            g.rng = Rng::new(4);
            g.player.agility = 50; // beats every Random(0x32)
            g.pistol = combat_dispatch::Pistol {
                owned: true,
                silencer: true,
                cartridges: 6,
            };
            let enemy = Fighter {
                armor,
                ..punchbag()
            };
            // The closing `run` (level 0, so no penalty) is what makes the
            // fight record `last_enemy`; running out of input does not.
            g.run_combat(0, enemy, &mut input(&["f", "run"])).unwrap();
            g.last_enemy.as_ref().unwrap().hp
        };
        let bare = hit(0);
        assert!((500 - 29..=500 - 20).contains(&bare), "hp {bare}");
        assert_eq!(hit(60), bare, "1000:4f28 has no `armour div 3` term");
    }

    /// The flee penalty end to end -- `1000:493b`..`1000:4adc`. The growth
    /// log is spent, the level and the threshold come back down, and the
    /// stats the level granted go with them.
    ///
    /// [`crate::progress::undo_growth`]'s own round trip against
    /// `data/xp.json` is in `tests/progression.rs`; what this adds is that
    /// `run` at the fight prompt is wired to it at all, and that a level-0
    /// player still gets `1000:4931`'s free exit.
    #[test]
    fn fleeing_above_level_zero_gives_a_level_back() {
        let mut g = game();
        let mut rng = Rng::new(5);
        let award = g.progress.threshold;
        progress::apply_levels(&mut g.progress, &mut g.player, &mut rng, award, false);
        assert_eq!(g.player.level, 1);
        let grown = g.player.clone();
        let threshold = g.progress.threshold;

        g.run_combat(0, punchbag(), &mut input(&["run"])).unwrap();
        assert_eq!(g.player.level, 0, "1000:4ac3 dec [0x38a6]");
        assert_eq!(
            g.progress.threshold,
            threshold - progress::THRESHOLD_STEP,
            "1000:4ac7 sub word [0x38d0],0xa"
        );
        assert_eq!(
            g.progress.growth_log[1],
            [0; progress::GAINS_PER_LEVEL],
            "1000:497d clears the entry"
        );
        assert_ne!(
            (
                g.player.strength,
                g.player.agility,
                g.player.vitality,
                g.player.luck
            ),
            (grown.strength, grown.agility, grown.vitality, grown.luck),
            "two stats were taken back"
        );

        // Level 0 is `1000:4931`'s other arm: nothing to take, nothing taken.
        let mut g = game();
        let before = g.player.clone();
        let before_p = g.progress.clone();
        g.run_combat(0, punchbag(), &mut input(&["run"])).unwrap();
        assert_eq!(g.player, before);
        assert_eq!(g.progress, before_p);
    }

    /// `1000:4a87`..`1000:4abe` -- the den block inside the flee penalty.
    /// `1000:4aa0 cmp ax,3` / `jnz` is **equality** on
    /// `level - (district - 1) * 10`, unlike the post-kill twin at
    /// `1000:52ae` which uses `jl`; and `1000:4a87` lets class 5 out of the
    /// whole thing.
    ///
    /// The store at `1000:4aa5` SETS the den flag while announcing that the
    /// player is too shabby for the den. That is the original's, and it is
    /// asserted here as written rather than corrected.
    #[test]
    fn the_flee_penalty_opens_the_den_on_the_exact_measured_level() {
        let cases = [
            (1u8, 2u16, false), // 2, below
            (1, 3, true),       // 3, exactly
            (1, 4, false),      // 4, above -- `jnz`, not `jl`
            (2, 13, true),      // 13 - 10 = 3
            (2, 3, false),      // 3 - 10 = -7
            (3, 23, true),      // 23 - 20 = 3
        ];
        for (district, level, want_den) in cases {
            let mut g = game();
            g.district = district;
            g.player.level = level;
            g.run_combat(0, punchbag(), &mut input(&["run"])).unwrap();
            assert_eq!(
                g.places.is_found(Location::Den),
                want_den,
                "district {district}, level {level}"
            );
        }

        // 1000:4a87 `cmp word [0x389c],5` / `jz 0x4ac3` -- class 5 skips the
        // block even on the level that would otherwise open the den.
        let mut g = game();
        g.district = 1;
        g.player.level = 3;
        g.player.class = 5;
        g.run_combat(0, punchbag(), &mut input(&["run"])).unwrap();
        assert!(!g.places.is_found(Location::Den), "class 5 skips 1000:4a8e");
        assert_eq!(g.player.level, 2, "but still pays the level");
    }

    /// `1000:48eb` and `1000:4f8c` -- the rector refuses the flee and, when
    /// he wins, there is no hospital behind the death message however much
    /// cred and whatever den flag the player is carrying.
    #[test]
    fn the_rector_refuses_the_flee_and_leaves_no_hospital() {
        let mut g = game();
        g.rector_showdown = true;
        g.player.level = 5;
        let mut lines = input(&["run", "run", "run"]);
        g.run_combat(0, punchbag(), &mut lines).unwrap();
        assert_eq!(lines.count(), 0, "1000:490b re-prompts instead of leaving");
        assert_eq!(g.player.level, 5, "and the penalty never runs");

        // Death: the hospital's own gates are wide open and it still must
        // not fire.
        let mut g = game_with_gopota();
        g.rector_showdown = true;
        g.player.hp = 0;
        g.player.hpmax = 40;
        g.player.money = 100;
        g.run_combat(0, punchbag(), &mut no_input()).unwrap();
        assert!(!g.running, "1000:4fb4 calls the end screen, which halts");
        assert_eq!(g.player.hp, 0, "1000:5018's `hp := hpmax` is not reached");
        assert_eq!(g.player.money, 100, "and no bill was paid");
        assert_eq!(g.pontovost_street, 500, "1000:4fe7's -10 is not reached");
    }

    /// `1000:4b0d`'s arm, reached through the combat prompt rather than
    /// through `Game::dispatch`: `kos` is one of the nine tokens
    /// `FUN_1000_3d11` compares, and its arm sets the 3-turn buff.
    #[test]
    fn kos_typed_at_the_combat_prompt_smokes_a_joint() {
        let enemy = || Fighter {
            name: "Дохляк".to_string(),
            hp: 50,
            hpmax: 50,
            ..Fighter::default()
        };

        let mut g = game();
        g.player.hp = 1;
        g.player.joints = 2;
        let mut lines = input(&["kos"]);
        g.run_combat(0, enemy(), &mut lines).unwrap();
        assert_eq!(g.player.joints, 1, "1000:4b4e dec [0x38c5]");
        assert_eq!(g.player.hp, 11, "the flat +10 heal");
        assert_eq!(g.buff_countdown, 3, "1000:4b52 stores 3, not 10");
        assert!(g.player.stoned);
    }

    /// The seventeen lines of `i`, in the order `1000:ea9e`..`1000:ec73`
    /// prints them, with every discovery flag set.
    ///
    /// **This is the test that fails if the array literal is re-sorted.**
    /// `data/club_arms.json`'s `command_list.what_the_port_must_change[2]`
    /// carries a `do_not_fix`: the seven gates run `3694`, `3695`, **`3698`**,
    /// `3697`, **`3696`**, `3699`, `369a` -- Vet before Girl before Den, where
    /// the flag ADDRESSES go Den, Girl, Vet. Read as flow that confirms the
    /// PLACES.SAV read order; read as an ordering it would reintroduce the
    /// swap `src/locations.rs` records earlier revisions of this port
    /// carrying. Asserting the whole sequence is what makes "do not tidy it"
    /// executable: any re-sort of the seven tuples moves `rep`, `girl` or
    /// `pr` and reds this.
    #[test]
    fn the_command_list_prints_seventeen_lines_in_the_originals_gate_order() {
        let mut g = game();
        for loc in crate::locations::TRACKED {
            g.places.mark_found(loc);
        }
        let out = crate::term::capture::lines(|| {
            g.dispatch(Command::CommandList, &mut no_input()).unwrap();
        });
        assert_eq!(
            out,
            vec![
                "Напиши: ^6w^7    чтобы шататься по окрестностям - искать на свою жопу приключения",
                "Напиши: ^6mar^7  чтобы идти на рынок",
                "Напиши: ^6bmar^7 чтобы идти к барыгам",
                "Напиши: ^6rep^7  чтобы идти к ветеринару",
                "Напиши: ^6girl^7 чтобы завалиться к своей девчонке",
                "Напиши: ^6pr^7   чтобы идти в местный притон гопоты",
                "Напиши: ^6kl^7   чтобы идти в клуб",
                "Напиши: ^6trn^7  чтобы идти в качалку",
                "Напиши: ^6s^7    чтобы посмотреть в лужу на свою уродскую рожу",
                "Напиши: ^6sv^7   чтобы приглядеться к пинаемому мудаку",
                "Напиши: ^6k^7    чтобы гасить мудака который тебе попался на дороге",
                "Напиши: ^6v^7    чтобы позвать подкрепление",
                "Напиши: ^6kos^7  чтобы схавать косяк",
                "Напиши: ^6h^7    чтобы выпить пиво (если не охото к ветеринару)",
                "Напиши: ^6mh^7   чтобы набухаться до чёртиков",
                "Напиши: ^6name^7 чтобы сменить погоняло",
                "Напиши: ^6e^7    если захочешь выйти",
            ],
            "1000:ea9e ungated, then 1000:eab7/ead7/eaf7/eb17/eb37/eb57/eb77 \
             in THAT order, then 1000:eb97 onward ungated"
        );
        assert_eq!(out.len(), 17);
    }

    /// The gate order really is not the flag-address order, spelled out so a
    /// reader of the assertion above does not have to hold both sequences in
    /// their head. Printing the two gated lines whose flags are `20ae:3698`
    /// (Vet) and `20ae:3696` (Den) and nothing else, the VET line must come
    /// first -- `1000:eaf7` precedes `1000:eb37` -- while
    /// `crate::locations::TRACKED`, which is the flag-ADDRESS order, has Den
    /// at slot 2 and Vet at slot 4.
    #[test]
    fn the_i_lists_gate_order_is_not_the_flag_address_order() {
        let mut g = game();
        g.places = Places::from_bytes(&[0u8; 7]);
        g.places.mark_found(Location::Den);
        g.places.mark_found(Location::Vet);
        let out = crate::term::capture::lines(|| {
            g.dispatch(Command::CommandList, &mut no_input()).unwrap();
        });
        let gated: Vec<&String> = out
            .iter()
            .filter(|l| l.contains("^6rep^7") || l.contains("^6pr^7"))
            .collect();
        assert_eq!(gated.len(), 2);
        assert!(
            gated[0].contains("^6rep^7"),
            "1000:eaf7 (Vet) gates before 1000:eb37 (Den); got {gated:?}"
        );
        // And the flag-address order really is the other way round, so the
        // assertion above is a contrast and not a restatement.
        let den = crate::locations::TRACKED
            .iter()
            .position(|&l| l == Location::Den)
            .unwrap();
        let vet = crate::locations::TRACKED
            .iter()
            .position(|&l| l == Location::Vet)
            .unwrap();
        assert!(
            den < vet,
            "locations::TRACKED is the PLACES.SAV / flag-address order, Den \
             at slot 2 and Vet at slot 4 -- do not reorder it to match the \
             `i` list"
        );
    }

    /// `data/club_arms.json`'s `command_list.what_the_port_must_change[0]`
    /// `falsifiable_as`, made executable: **with only Market and Vet found
    /// the list must be TWELVE lines** -- the ungated head, the two gated
    /// lines whose flags are set, and the nine ungated tail lines. That is
    /// the state `Game::new` leaves a fresh character in (`1000:6dc3` sets
    /// `20ae:3698`, `1000:6dc8` sets `20ae:3694`), so it is also what a
    /// player sees on turn one.
    #[test]
    fn the_command_list_is_twelve_lines_with_only_market_and_vet_found() {
        let mut g = game();
        assert!(g.places.is_found(Location::Market) && g.places.is_found(Location::Vet));
        for loc in crate::locations::TRACKED {
            if loc != Location::Market && loc != Location::Vet {
                assert!(!g.places.is_found(loc), "{loc:?} must start clear");
            }
        }
        let out = crate::term::capture::lines(|| {
            g.dispatch(Command::CommandList, &mut no_input()).unwrap();
        });
        assert_eq!(out.len(), 12, "1 + 2 + 9, not 13 and not 17: {out:#?}");
        assert_eq!(out[1], "Напиши: ^6mar^7  чтобы идти на рынок");
        assert_eq!(out[2], "Напиши: ^6rep^7  чтобы идти к ветеринару");
        for absent in ["^6bmar^7", "^6girl^7", "^6pr^7", "^6kl^7", "^6trn^7"] {
            assert!(
                !out.iter().any(|l| l.contains(absent)),
                "{absent} is gated on a flag that is clear"
            );
        }
    }

    /// Each of the seven gates reads its OWN verb's flag: setting exactly one
    /// must add exactly its own line to the twelve above. A gate wired to the
    /// wrong flag passes every other test here.
    #[test]
    fn each_i_gate_reads_its_own_verbs_discovery_flag() {
        for (loc, token) in [
            (Location::Dealers, "^6bmar^7"),
            (Location::Girl, "^6girl^7"),
            (Location::Den, "^6pr^7"),
            (Location::Club, "^6kl^7"),
            (Location::Gym, "^6trn^7"),
        ] {
            let mut g = game();
            g.places = Places::from_bytes(&[0u8; 7]);
            g.places.mark_found(loc);
            let out = crate::term::capture::lines(|| {
                g.dispatch(Command::CommandList, &mut no_input()).unwrap();
            });
            assert_eq!(out.len(), 11, "1 + 1 + 9 for {loc:?}");
            assert!(
                out.iter().any(|l| l.contains(token)),
                "{loc:?} found, but {token} is not listed"
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
        g.player.hp = 1; // 1000:d4ba ejects a whole player after every turn
        g.shop_turn(Location::Vet, "mar", &mut no_input()).unwrap(); // must not teleport
        assert_eq!(g.location, Location::Vet);
        g.shop_turn(Location::Vet, "w", &mut no_input()).unwrap();
        assert_eq!(g.location, Location::Street);
        assert_eq!(g.mode, Mode::Street);
    }

    /// I5: `h` is the beer verb at the top level (`entry` -> `FUN_1000_29c4`,
    /// `1000:e966`) *and* a vet key, because the vet reads its own input at
    /// its own prompt. Both must work, through the same public path the
    /// player uses -- not by calling a handler directly.
    ///
    /// The vet's `h` is the five-point HEAL (`1000:d5de`), not a jaw: see
    /// [`crate::vet`], whose own tests carry the arm. This one is only
    /// about the two `h`s not being the same `h`.
    #[test]
    fn h_heals_at_the_vet_and_drinks_beer_on_the_street() {
        let mut g = game();
        g.places.mark_found(Location::Vet);
        g.location = Location::Vet;
        g.mode = Mode::Shop(Location::Vet);
        g.player.hp = g.player.hpmax - 6; // 1000:d5c3 needs hp < hpmax
        g.player.money = 10;
        g.player.beer_dl = 4;
        let hp0 = g.player.hp;
        g.shop_turn(Location::Vet, "h", &mut no_input()).unwrap();
        assert_eq!(g.player.hp, hp0 + 5, "vet's h is 1000:d5de, +5 health");
        assert_eq!(g.player.money, 7, "1000:d5d9 sub 0x3");
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
        g.player.broken_jaw = true;
        g.player.money = 10;
        g.shop_turn(Location::Vet, "r", &mut no_input()).unwrap();
        // 1000:d558 AND 1000:d55d, behind the one price test at 1000:d54c.
        assert!(!g.player.broken_leg);
        assert!(!g.player.broken_jaw);
        assert_eq!(g.player.money, 3);
    }

    #[test]
    fn quit_stops_the_loop() {
        let mut g = game();
        g.dispatch(Command::Quit, &mut no_input()).unwrap();
        assert!(!g.running);
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

    /// The Task 41 divergence, closed by Task 42.
    ///
    /// `1000:2a1d` missed prints the refusal and then `1000:2a38 jmp 0x2baa`
    /// lands in the `mh` TAIL, not at the return `1000:2c58`. The snapshot
    /// `1000:2a11` / `1000:2a14` was already taken, so hp == hp0 there:
    /// `1000:2bc6` and `1000:2c0d` both take, `1000:2c36` does not, and
    /// `1000:2c3d` falls through whenever `20ae:38c3` is at or below zero.
    /// `h` is unaffected -- `1000:2bba` misses for it and `1000:2bbc` returns.
    #[test]
    fn a_broken_jaw_still_runs_the_tail_for_mh_and_returns_for_h() {
        let refusal = "^4Ты не можешь пить пиво из-за сломаной челюсти.";

        let mut g = game();
        g.player.hp = 5;
        g.player.beer_dl = 0;
        g.player.broken_jaw = true;
        let out = term::capture::lines(|| g.beer(Beer::Binge));
        assert_eq!(
            out,
            vec![refusal.to_string(), "^4Пива нету".to_string()],
            "1000:2a38 enters the tail: `mh` + broken jaw + no beer writes \
             the refusal AND file 0x4240"
        );
        assert_eq!(g.player.hp, 5, "the tail writes no hp");
        assert_eq!(g.player.beer_dl, 0);

        // Beer in hand: the tail runs, but `1000:2c3d` takes and it is silent.
        let mut g = game();
        g.player.hp = 5;
        g.player.beer_dl = 4;
        g.player.broken_jaw = true;
        let out = term::capture::lines(|| g.beer(Beer::Binge));
        assert_eq!(
            out,
            vec![refusal.to_string()],
            "1000:2c3d takes while beer > 0, so the tail adds nothing"
        );
        assert_eq!(g.player.beer_dl, 4, "the jaw arm spends no half-litre");

        // `h` never reaches the tail at all.
        let mut g = game();
        g.player.hp = 5;
        g.player.beer_dl = 0;
        g.player.broken_jaw = true;
        let out = term::capture::lines(|| g.beer(Beer::One));
        assert_eq!(
            out,
            vec![refusal.to_string()],
            "1000:2bba misses for `h` and 1000:2bbc returns"
        );
    }

    /// Gate ORDER, which no state assertion can see.
    ///
    /// `1000:2a18` is reached before `1000:2a3e`, and `1000:2a3e` before
    /// `1000:2a47`. Swap either pair and the state is identical while a
    /// different line is written.
    #[test]
    fn the_beer_gates_run_in_the_originals_order() {
        // Jaw before full-health: 1000:2a18 precedes 1000:2a3e.
        let mut g = game();
        g.player.hp = g.player.hpmax;
        g.player.beer_dl = 4;
        g.player.broken_jaw = true;
        let out = term::capture::lines(|| g.beer(Beer::One));
        assert_eq!(
            out,
            vec!["^4Ты не можешь пить пиво из-за сломаной челюсти.".to_string()],
            "1000:2a18 runs before 1000:2a3e, so a full-health drunk with a \
             broken jaw hears about the jaw"
        );

        // Full-health before no-beer: 1000:2a3e precedes 1000:2a47. And
        // 1000:2b80 returns, so `mh` does not reach the tail's 0x4240 either.
        let mut g = game();
        g.player.hp = g.player.hpmax;
        g.player.beer_dl = 0;
        let out = term::capture::lines(|| g.beer(Beer::Binge));
        assert_eq!(
            out,
            vec!["^6Блин только тупить не надо - и так здоровья до фига.".to_string()],
            "1000:2a3e runs before 1000:2a47 and 1000:2b80 returns, so this \
             arm never reaches the tail"
        );
    }

    /// Message TEXT, literal for literal, against the nine-string pool
    /// `docs/re/beer.md` tiles at file `0x4197`..`0x4294`.
    ///
    /// The partial arm's two strings are one physical line: `1000:2a8f` is
    /// `call 0eed:0000` (`Write`, no newline) and `1000:2ae0` is
    /// `call 0eed:01c2` (`WriteLn`), so file `0x41CD` and file `0x41E4`
    /// arrive joined.
    #[test]
    fn the_h_arms_write_the_literals_the_pool_holds() {
        // Partial arm, 1000:2a64: shortfall 3 -> file 0x41CD + file 0x41E4.
        let mut g = game();
        g.player.hp = 17;
        g.player.beer_dl = 3;
        let out = term::capture::lines(|| g.beer(Beer::One));
        assert_eq!(
            out,
            vec!["^2Пиво прибавляет 3з. ^2Здоровья:20/20. Осталось 1.0л. пива".to_string()],
            "1000:2a8f writes file 0x41CD without a newline"
        );
        assert_eq!(g.player.hp, 20, "1000:2a97 tops hp up to hpmax");

        // Flat arm, 1000:2ae7: shortfall 5 is NOT under 5 -> file 0x4208.
        let mut g = game();
        g.player.hp = 15;
        g.player.beer_dl = 10;
        let out = term::capture::lines(|| g.beer(Beer::One));
        assert_eq!(
            out,
            vec!["^2Пиво прибавляет 5з. Здоровья:20/20. Осталось 4.5л. пива".to_string()],
            "1000:2a5f is `jl 5`, so a shortfall of exactly 5 takes the flat arm"
        );

        // No-beer arm, 1000:2b3a -> file 0x4240.
        let mut g = game();
        g.player.hp = 5;
        g.player.beer_dl = 0;
        let out = term::capture::lines(|| g.beer(Beer::One));
        assert_eq!(out, vec!["^4Пива нету".to_string()]);
    }

    /// Loop TERMINATION: the two ways out of `1000:2a3b`'s loop, and what
    /// the tail writes for each.
    #[test]
    fn mh_stops_at_hpmax_and_at_the_last_half_litre() {
        // Out through 1000:2b9e -- hp reached hpmax with beer left over.
        let mut g = game();
        g.player.hp = 5;
        g.player.beer_dl = 10;
        let out = term::capture::lines(|| g.beer(Beer::Binge));
        assert_eq!(g.player.hp, 20);
        assert_eq!(g.player.beer_dl, 7, "three half-litres, not four");
        assert_eq!(
            out,
            vec!["^2Пиво прибавляет 15з. Здоровья:20/20. Осталось 3.5л. пива".to_string()],
            "1000:2bd0 makes the first field the TOTAL healed, and 1000:2c14 \
             takes while beer remains"
        );

        // Out through 1000:2ba5 -- the beer ran out first.
        let mut g = game();
        g.player.hp = 5;
        g.player.beer_dl = 2;
        let out = term::capture::lines(|| g.beer(Beer::Binge));
        assert_eq!(g.player.hp, 15);
        assert_eq!(g.player.beer_dl, 0);
        assert_eq!(
            out,
            vec![
                "^2Пиво прибавляет 10з. Здоровья:15/20. Осталось 0.0л. пива".to_string(),
                "^4Кончилось пиво".to_string(),
            ],
            "1000:2c14 misses when the drink emptied it, so file 0x4283 follows"
        );

        // Never entered: 1000:2a4c misses on the first pass, and the tail's
        // 1000:2c36 / 1000:2c3d pair writes file 0x4240.
        let mut g = game();
        g.player.hp = 5;
        g.player.beer_dl = 0;
        let out = term::capture::lines(|| g.beer(Beer::Binge));
        assert_eq!(out, vec!["^4Пива нету".to_string()]);
        assert_eq!(g.player.hp, 5);
    }

    /// `1000:e9b4`: one joint, +10 hp capped at hpmax, Сила +2, урон +1/+2,
    /// and the stoned flag blocks a second one.
    #[test]
    fn kos_smokes_exactly_one_joint_and_buffs_strength() {
        let mut g = game();
        g.player.hp = 1;
        g.player.joints = 2;
        g.smoke(Joint::Street);
        assert_eq!(g.player.joints, 1);
        assert_eq!(g.player.hp, 11);
        assert_eq!(g.player.strength, 7);
        assert_eq!(g.player.dmg_min, 2);
        assert_eq!(g.player.dmg_max, 5);
        assert!(g.player.stoned);

        g.smoke(Joint::Street);
        assert_eq!(g.player.joints, 1, "already stoned: no second joint");
    }

    /// The combat copy of the handler (`1000:4b17`) grants the identical
    /// stats and spends one joint the same way -- the only thing it does
    /// differently is `1000:4b52` `c6 06 cd 38 03` where `1000:e9b8` stores
    /// `0a`. Both are asserted here, because "same except one immediate" is
    /// only worth writing down if the "same" half is checked too.
    #[test]
    fn kos_in_a_fight_is_the_same_handler_with_a_three_turn_buff() {
        let mut street = game();
        street.player.hp = 1;
        street.player.joints = 2;
        street.smoke(Joint::Street);

        let mut fight = game();
        fight.player.hp = 1;
        fight.player.joints = 2;
        fight.smoke(Joint::Fight);

        assert_eq!(fight.player.joints, street.player.joints);
        assert_eq!(fight.player.hp, street.player.hp);
        assert_eq!(fight.player.strength, street.player.strength);
        assert_eq!(fight.player.dmg_min, street.player.dmg_min);
        assert_eq!(fight.player.dmg_max, street.player.dmg_max);
        assert!(fight.player.stoned);

        assert_eq!(street.buff_countdown, 10, "1000:e9b8 stores 10");
        assert_eq!(fight.buff_countdown, 3, "1000:4b52 stores 3");
    }

    /// The two pools' long heal lines differ by one trailing letter and both
    /// are quoted verbatim. Asserting them against each other is what stops
    /// a later edit "fixing" the original's typo in one place.
    #[test]
    fn the_two_joint_pools_long_heal_lines_differ_by_one_letter() {
        let street = Joint::Street.long_heal_line();
        let fight = Joint::Fight.long_heal_line();
        assert_ne!(street, fight);
        assert_eq!(
            street,
            "^2Колёса прибавляют #з. Здоровья:#/#. Осталось # косякова"
        );
        assert_eq!(
            fight,
            "^2Колёса прибавляют #з. Здоровья:#/#. Осталось # косяков"
        );
        assert_eq!(street.chars().count(), fight.chars().count() + 1);
        assert!(street.starts_with(fight));
    }

    /// `1000:ed5f` / `1000:ed74`: an empty rename does not keep the old
    /// name, it installs the default -- the same substitution character
    /// creation already makes at `1000:7220` / `1000:7227`. Both then get
    /// the `^7 ` prefix at `1000:ed79`, which is AFTER the substitution, so
    /// the default carries it too.
    #[test]
    fn an_empty_rename_installs_the_default_name() {
        let mut g = game();
        g.player.name = "Вася".to_string();
        let mut lines = input(&[""]);
        g.rename(&mut lines).unwrap();
        assert_eq!(g.player.name, "^7 Раз^6дол^4бай");

        let mut g = game();
        g.player.name = "Вася".to_string();
        let mut lines = input(&["Петя"]);
        g.rename(&mut lines).unwrap();
        assert_eq!(g.player.name, "^7 Петя");
    }

    /// The point of carrying `^7 ` in the name rather than adding it at the
    /// save boundary: `20ae:379c` holds it, so everything that renders the
    /// name renders it. The character sheet is the visible one --
    /// `1000:1a03` reads the same variable the save does.
    ///
    /// This is the half that was NOT modelled while `persist` added the
    /// prefix on the way out and stripped it on the way in; the record
    /// matched the original's bytes and every line on screen was missing a
    /// colour reset and a leading space.
    #[test]
    fn the_name_prefix_reaches_the_character_sheet() {
        let mut g = game();
        g.rename(&mut input(&["Петя"])).unwrap();
        assert_eq!(g.player.name, "^7 Петя");
        let sheet = character_sheet::lines(&g.player, &g.player.name, &g.sheet_kit());
        assert!(
            sheet.iter().any(|l| l.contains("^7 Петя")),
            "the sheet renders the stored name verbatim: {sheet:?}"
        );
    }

    /// And it survives the record in both directions, because the prefix is
    /// now part of the value rather than something the boundary adds:
    /// `to_save` copies `20ae:379c` (`1000:761d`) and the load reads it
    /// straight back (`1000:6dd7`).
    #[test]
    fn the_name_prefix_round_trips_through_a_save() {
        let mut g = game();
        g.rename(&mut input(&["Петя"])).unwrap();
        let save = g.to_save();
        assert_eq!(save.name, "^7 Петя", "the record carries it");
        let back = Game::from_save(&save, g.places.clone(), g.district, 1);
        assert_eq!(
            back.player.name, "^7 Петя",
            "and the load does not strip it"
        );
    }

    /// `1000:ed5f` `cmp byte [0x379c],0` tests the shortstring's LENGTH
    /// BYTE, not whether its content is all whitespace. A line of three
    /// spaces has length 3, so `jnz 0xed79` is taken and `1000:ed74`'s
    /// substitution never runs -- the typed spaces are kept verbatim. Before
    /// `Game::rename` stopped `.trim()`-ing the line, this case wrongly
    /// installed the default name instead.
    #[test]
    fn a_whitespace_only_rename_is_kept_not_substituted() {
        let mut g = game();
        g.player.name = "Вася".to_string();
        let mut lines = input(&["   "]);
        g.rename(&mut lines).unwrap();
        // `1000:ed79`'s prefix still applies -- it is unconditional.
        assert_eq!(g.player.name, "^7    ");
    }

    /// `mar` row 1, price 2 (`20ae:0b2e`), debited at `1000:bdb3`. The hp
    /// has to be below max or `1000:bd86 jnl 0xbdf1` refuses the sale before
    /// the money is ever tested -- which is why this reads 19/20 and not the
    /// 20/20 `player()` ships.
    #[test]
    fn shop_action_buys_an_affordable_row_and_debits_price() {
        let mut g = game();
        g.location = Location::Market;
        g.player.hp = 19;
        g.player.money = 10;
        g.shop_turn(Location::Market, "1", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 8);
    }

    /// `1000:bd91` is a `jle`, so 2 exactly is enough and 1 is not.
    #[test]
    fn shop_action_refuses_when_too_poor() {
        for (money, want) in [(1i16, 1i16), (2, 0)] {
            let mut g = game();
            g.location = Location::Market;
            g.player.hp = 19;
            g.player.money = money;
            g.shop_turn(Location::Market, "1", &mut no_input()).unwrap();
            assert_eq!(g.player.money, want, "money {money}");
        }
    }

    /// The market's district gates DO sit on the buy path -- `1000:c08e`
    /// (row 6), `1000:c1d7` (row 8) and `1000:c27f` (row 9) -- unlike the
    /// dealers', which are all menu gates. Below the district the row is not
    /// refused, it is unreachable, so nothing is spent and no flag moves.
    #[test]
    fn shop_action_respects_the_district_gate() {
        let mut g = game();
        g.location = Location::Market;
        g.player.money = 1000;
        g.district = 1; // 1000:c08e is `cmp byte [0x3692],0x1`
        g.shop_turn(Location::Market, "6", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 1000, "gated row must not be sellable yet");
        assert!(!g.wear_jacket, "1000:c0e0 must not have run");
        g.district = 2;
        g.shop_turn(Location::Market, "6", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 1000 - 25);
        assert!(g.wear_jacket, "1000:c0e0");
    }

    /// The dealers' three pistol rows, each on both sides of every gate its
    /// arm has. This is the only place [`Game::pistol`] can be filled in, so
    /// it is what makes `f` reachable in play.
    #[test]
    fn the_dealers_sell_the_pistol_its_cartridges_and_its_silencer() {
        let shop = || {
            let mut g = game();
            g.location = Location::Dealers;
            g.mode = Mode::Shop(Location::Dealers);
            g.district = 4; // all three rows are `district>3`
            g.player.money = 1_000;
            g
        };

        // Row 7: the pistol, 150 roubles, and three cartridges with it
        // (1000:cd0a `add word [0x394f],3`).
        let mut g = shop();
        g.shop_turn(Location::Dealers, "7", &mut no_input())
            .unwrap();
        assert!(g.pistol.owned, "1000:cd05");
        assert_eq!(g.pistol.cartridges, 3);
        assert_eq!(g.player.money, 850);
        // 1000:ccdd -- buying it twice is refused and costs nothing.
        g.shop_turn(Location::Dealers, "7", &mut no_input())
            .unwrap();
        assert_eq!(g.player.money, 850, "1000:cd4c is a refusal, not a sale");
        assert_eq!(g.pistol.cartridges, 3);

        // ... and 1000:cce8's `jle` means 150 exactly is enough while 149 is
        // not.
        for (money, want) in [(149i16, false), (150, true)] {
            let mut g = shop();
            g.player.money = money;
            g.shop_turn(Location::Dealers, "7", &mut no_input())
                .unwrap();
            assert_eq!(g.pistol.owned, want, "money {money}");
        }

        // Row 8: five cartridges (1000:cda3), though the menu line says six,
        // and refused outright without a pistol (1000:cd7b).
        let mut g = shop();
        g.shop_turn(Location::Dealers, "8", &mut no_input())
            .unwrap();
        assert_eq!(g.pistol.cartridges, 0, "1000:cdcc -- no gun, no rounds");
        assert_eq!(g.player.money, 1_000);
        g.pistol.owned = true;
        g.shop_turn(Location::Dealers, "8", &mut no_input())
            .unwrap();
        assert_eq!(
            g.pistol.cartridges, 5,
            "the arm adds five, not the six the line promises"
        );
        assert_eq!(g.player.money, 930);

        // Row 9: the silencer, gated on the pistol AND on `20ae:3e32`
        // reaching exactly 25 (1000:ce00).
        for (owned, walks, want) in [(false, 25u8, false), (true, 24, false), (true, 25, true)] {
            let mut g = shop();
            g.pistol.owned = owned;
            g.dealer_delivery_counter = walks;
            g.shop_turn(Location::Dealers, "9", &mut no_input())
                .unwrap();
            assert_eq!(
                g.pistol.silencer, want,
                "owned {owned}, delivery counter {walks}"
            );
            assert_eq!(
                g.player.money,
                if want { 1_000 - 60 } else { 1_000 },
                "owned {owned}, delivery counter {walks}"
            );
        }
    }

    /// A player standing at the dealers' prompt with `money` roubles.
    /// `district` is left at `Game::new`'s 1 on purpose: none of the nine
    /// arms tests it (`Game::shop_action`).
    fn dealers(money: i16) -> Game {
        let mut g = game();
        g.location = Location::Dealers;
        g.mode = Mode::Shop(Location::Dealers);
        g.player.money = money;
        g
    }

    /// `bmar` row 1, Косяк -- `20ae:38c5` is a word COUNT (`1000:c90e`
    /// `inc [0x38c5]`), so the row is repeatable and each purchase adds one.
    #[test]
    fn the_dealers_sell_a_joint_every_time_it_is_asked_for() {
        let mut g = dealers(40);
        g.shop_turn(Location::Dealers, "1", &mut no_input())
            .unwrap();
        assert_eq!(g.player.joints, 1, "1000:c90e");
        assert_eq!(g.player.money, 25, "20ae:0b38 = 15, debit 1000:c90a");
        // No already-own test in the arm at all -- buying again works.
        g.shop_turn(Location::Dealers, "1", &mut no_input())
            .unwrap();
        assert_eq!(g.player.joints, 2, "the row is repeatable");
        assert_eq!(g.player.money, 10);
        // 1000:c8e8 is `jle`, so 15 exactly buys and 14 does not.
        for (money, want) in [(14i16, 0i16), (15, 1)] {
            let mut g = dealers(money);
            g.shop_turn(Location::Dealers, "1", &mut no_input())
                .unwrap();
            assert_eq!(g.player.joints, want, "money {money}");
            assert_eq!(g.player.money, if want == 1 { money - 15 } else { money });
        }
    }

    /// `bmar` row 2, Краденый мобильник -- and the number the effect moves is
    /// the in-combat backup countdown at `1000:4cdb`, not the flag alone.
    #[test]
    fn the_dealers_sell_the_stolen_mobile_once() {
        let mut g = dealers(40);
        g.shop_turn(Location::Dealers, "2", &mut no_input())
            .unwrap();
        assert!(g.has_mobile, "1000:c969");
        assert_eq!(g.player.money, 10, "20ae:0b39 = 30, debit 1000:c973");
        // 1000:c93c / 1000:c941 -- the already-own refusal costs nothing.
        g.player.money = 40;
        g.shop_turn(Location::Dealers, "2", &mut no_input())
            .unwrap();
        assert_eq!(g.player.money, 40, "1000:c992 is a refusal, not a sale");
        // Too poor: 1000:c94c is `jle`, so 29 is short and 30 is enough.
        for (money, want) in [(29i16, false), (30, true)] {
            let mut g = dealers(money);
            g.shop_turn(Location::Dealers, "2", &mut no_input())
                .unwrap();
            assert_eq!(g.has_mobile, want, "money {money}");
        }
        // The effect's NUMBER, not just the flag: `1000:4ce2` puts the
        // gopota countdown straight at 3 -- the arrival -- where `1000:4cd5`
        // only starts it at 1 without a phone.
        let counter = |has_mobile| {
            let mut b = crate::combat_dispatch::Backup::default();
            b.call(true, 100, 1, has_mobile);
            b.count()
        };
        assert_eq!(counter(false), 1, "1000:4cd5");
        assert_eq!(counter(true), 3, "1000:4ce2 -- the menu line's promise");
    }

    /// `bmar` row 4, зоновская наколка -- and the number is the wander
    /// mugging roll's ceiling at `1000:b5da`, this row's entire gameplay
    /// effect.
    #[test]
    fn the_dealers_ink_a_prison_tattoo_once() {
        let mut g = dealers(20);
        g.shop_turn(Location::Dealers, "4", &mut no_input())
            .unwrap();
        assert!(g.prison_tattoo, "1000:cb05");
        assert_eq!(g.player.money, 10, "20ae:0b3b = 10, debit 1000:cb0f");
        // 1000:cad8 / 1000:cadd -- the already-own refusal costs nothing.
        g.shop_turn(Location::Dealers, "4", &mut no_input())
            .unwrap();
        assert_eq!(g.player.money, 10, "1000:cb2e is a refusal, not a sale");
        // Too poor: 1000:cae8 is `jle`.
        for (money, want) in [(9i16, false), (10, true)] {
            let mut g = dealers(money);
            g.shop_turn(Location::Dealers, "4", &mut no_input())
                .unwrap();
            assert_eq!(g.prison_tattoo, want, "money {money}");
        }
    }

    /// `bmar` row 5, Кастет -- +2/+2 unconditionally (`1000:cbab`,
    /// `1000:cbb0`), and a better-weapon gate that is an AND.
    #[test]
    fn the_dealers_sell_the_knuckles_and_the_damage_moves_by_two() {
        let mut g = dealers(40);
        let (min, max) = (g.player.dmg_min, g.player.dmg_max);
        g.shop_turn(Location::Dealers, "5", &mut no_input())
            .unwrap();
        assert!(g.weapon_kastet, "1000:cb9d");
        assert_eq!(g.player.money, 15, "20ae:0b3c = 25, debit 1000:cba7");
        assert_eq!(g.player.dmg_min, min + 2, "1000:cbab");
        assert_eq!(g.player.dmg_max, max + 2, "1000:cbb0");
        // 1000:cb70 / 1000:cb75 -- the already-own refusal costs nothing and
        // does not add the damage a second time.
        g.player.money = 40;
        g.shop_turn(Location::Dealers, "5", &mut no_input())
            .unwrap();
        assert_eq!(g.player.money, 40, "1000:cbd0 is a refusal, not a sale");
        assert_eq!(g.player.dmg_max, max + 2);
        // The better-weapon gate is a short-circuit AND over 1000:cb5b,
        // 1000:cb62 and 1000:cb69, so only ALL THREE refuse. The loot arm
        // granting the same item refuses on ANY of them (1000:555f,
        // 1000:5566, 1000:556d) -- reproduced, not reconciled.
        for (club, knife, cleaver, want) in [
            (true, false, false, true),
            (true, true, false, true),
            (false, true, true, true),
            (true, true, true, false),
        ] {
            let mut g = dealers(40);
            g.weapon_dubinka = club;
            g.weapon_nozhik = knife;
            g.weapon_tesak = cleaver;
            g.shop_turn(Location::Dealers, "5", &mut no_input())
                .unwrap();
            assert_eq!(
                g.weapon_kastet, want,
                "club {club} knife {knife} cleaver {cleaver}"
            );
        }
        // Too poor: 1000:cb80 is `jle`.
        for (money, want) in [(24i16, false), (25, true)] {
            let mut g = dealers(money);
            g.shop_turn(Location::Dealers, "5", &mut no_input())
                .unwrap();
            assert_eq!(g.weapon_kastet, want, "money {money}");
        }
    }

    /// `bmar` row 6, Дубинка -- including the original bug: the menu line
    /// promises `урон+4` and the arm grants **nothing** without the knuckles,
    /// because `1000:cc69 jz 0xcc75` skips both adds and lands on the
    /// confirmation push.
    #[test]
    fn the_dealers_club_adds_no_damage_at_all_without_the_knuckles() {
        let mut g = dealers(60);
        let (min, max) = (g.player.dmg_min, g.player.dmg_max);
        g.shop_turn(Location::Dealers, "6", &mut no_input())
            .unwrap();
        assert!(g.weapon_dubinka, "1000:cc56");
        assert_eq!(g.player.money, 10, "20ae:0b3d = 50, debit 1000:cc60");
        assert_eq!(g.player.dmg_min, min, "1000:cc69 skips 1000:cc6b");
        assert_eq!(g.player.dmg_max, max, "1000:cc69 skips 1000:cc70");

        // With the knuckles it is +2/+2 -- never the +4/+4 the loot arm has
        // at 1000:55e6.
        let mut g = dealers(60);
        g.weapon_kastet = true;
        g.shop_turn(Location::Dealers, "6", &mut no_input())
            .unwrap();
        assert_eq!(g.player.dmg_min, min + 2, "1000:cc6b");
        assert_eq!(g.player.dmg_max, max + 2, "1000:cc70");

        // 1000:cc29 / 1000:cc2e -- the already-own refusal costs nothing.
        g.player.money = 60;
        g.shop_turn(Location::Dealers, "6", &mut no_input())
            .unwrap();
        assert_eq!(g.player.money, 60, "1000:cc90 is a refusal, not a sale");
        assert_eq!(g.player.dmg_max, max + 2);

        // The better-weapon gate has TWO conjuncts here (1000:cc18,
        // 1000:cc1f) and the club's own flag is not one of them.
        for (knife, cleaver, want) in [
            (true, false, true),
            (false, true, true),
            (true, true, false),
        ] {
            let mut g = dealers(60);
            g.weapon_nozhik = knife;
            g.weapon_tesak = cleaver;
            g.shop_turn(Location::Dealers, "6", &mut no_input())
                .unwrap();
            assert_eq!(g.weapon_dubinka, want, "knife {knife} cleaver {cleaver}");
        }
        // Too poor: 1000:cc39 is `jle`.
        for (money, want) in [(49i16, false), (50, true)] {
            let mut g = dealers(money);
            g.shop_turn(Location::Dealers, "6", &mut no_input())
                .unwrap();
            assert_eq!(g.weapon_dubinka, want, "money {money}");
        }
    }

    /// Every `bmar` row in `data::shops()` is consumed by
    /// [`Game::buy_dealer_row`], so [`Game::shop_action`] has no
    /// fall-through to reach from the dealers -- and since Task 26 it has
    /// none to reach at all: the generic "debit and echo the menu line" path
    /// is deleted, and a keyless row now trips a `debug_assert!` that is a
    /// no-op in release.
    ///
    /// So this is the guard that works in both profiles. Task 24's reason for
    /// it still holds and is why it is per-row rather than a count: file
    /// `0xAC55` / CS `0x9385` is row 1's OWN refusal (`1000:c8ea`), not a
    /// shop-wide literal, so a tenth `bmar` row added to the table without an
    /// arm would not fall back to anything -- it would silently do nothing.
    #[test]
    fn every_dealers_row_has_an_arm_of_its_own() {
        for row in data::shops().iter().filter(|r| r.shop == "bmar") {
            let mut g = dealers(0);
            assert!(
                g.buy_dealer_row(row.key, row.price),
                "bmar row {} has no purchase arm",
                row.key
            );
        }
    }

    /// The district gates the dealers' MENU, never the sale. Five rows are
    /// gated -- 5 (`1000:c68d`), 6 (`1000:c6f1`), 7 (`1000:c755`), 8
    /// (`1000:c7ba`) and 9 (`1000:c81d`) -- and every one of them is in the
    /// menu-print block. At district 1 none of the five is listed and all
    /// five are still buyable.
    ///
    /// `1000:c824`/`1000:c82b` -- the silencer needs a pistol AND the
    /// delivery counter at exactly 25, on top of `district > 3`. All three
    /// jump to `0xc88e`, past the row's print block, so a miss prints
    /// nothing.
    #[test]
    fn the_silencer_row_is_listed_only_with_a_pistol_and_a_full_counter() {
        let listed = |pistol: bool, counter: u8| {
            let mut g = game();
            g.district = 4; // 1000:c81d, satisfied throughout
            g.pistol.owned = pistol;
            g.dealer_delivery_counter = counter;
            g.listed_rows("bmar").iter().any(|r| r.key == "9")
        };
        assert!(listed(true, 25));
        assert!(!listed(false, 25), "1000:c824 wants the pistol");
        assert!(!listed(true, 24), "1000:c82b is == 25, not >=");
        // `jnz` is an equality test, so overshooting shuts the row again.
        assert!(!listed(true, 26), "1000:c830 is jnz, so 26 misses too");
        assert!(!listed(false, 0));
    }

    /// The district gate alone never opens row 9, at any district -- the
    /// falsifier for a fix that moved the two extra gates into
    /// `Game::afford` (where a miss would recolour the row rather than
    /// remove it) instead of into the `listed_rows` filter.
    #[test]
    fn no_district_alone_lists_the_silencer() {
        for d in 1..=5u8 {
            let mut g = game();
            g.district = d;
            assert!(
                !g.listed_rows("bmar").iter().any(|r| r.key == "9"),
                "district {d}"
            );
        }
    }

    /// `Game::extra_gates_open` panics on a gate string it does not model,
    /// so this is what keeps that panic unreachable: every `extra_gates`
    /// entry `build.rs` generates out of `data/shops.json` must be one of
    /// the two modelled predicates.
    #[test]
    fn every_extra_gate_in_the_table_is_modelled() {
        let modelled = ["byte[20ae:394d]!=0", "byte[20ae:3e32]==25"];
        let mut seen = 0;
        for row in data::shops() {
            for gate in row.extra_gates {
                assert!(modelled.contains(gate), "{} {} {gate}", row.shop, row.key);
                seen += 1;
            }
        }
        // Both are on `("bmar","9")` and nothing else carries any.
        assert_eq!(seen, 2);
    }

    /// The district gates the dealers' MENU, never the sale. Five rows are
    /// gated -- 5 (`1000:c68d`), 6 (`1000:c6f1`), 7 (`1000:c755`), 8
    /// (`1000:c7ba`) and 9 (`1000:c81d`) -- and every one of them is in the
    /// menu-print block. At district 1 none of the five is listed and all
    /// five are still buyable.
    #[test]
    fn a_gated_dealers_row_is_bought_below_its_district() {
        // Through `Game::listed_rows`, the predicate `print_priced_rows`
        // itself walks -- not a copy of it. Deleting the menu's gate makes
        // this assertion fail, which the re-implemented filter round 1
        // shipped here did not.
        let listed = |d: u8| {
            let mut g = game();
            g.district = d;
            g.listed_rows("bmar")
                .iter()
                .map(|r| r.key)
                .collect::<Vec<_>>()
        };
        assert_eq!(listed(1), ["1", "2", "3", "4"], "the menu keeps its gate");
        // Row 9 is NOT here: `1000:c824`/`1000:c82b` also want a pistol and
        // a delivery counter of 25, and a fresh game has neither. This
        // assertion used to read `..,"8","9"` and encoded the missing gates
        // as expected behaviour.
        assert_eq!(
            listed(4),
            ["1", "2", "3", "4", "5", "6", "7", "8"],
            "district alone does not open the silencer"
        );

        let mut g = dealers(1_000);
        assert_eq!(g.district, 1);
        g.shop_turn(Location::Dealers, "5", &mut no_input())
            .unwrap(); // gate district>1
        assert!(g.weapon_kastet, "1000:cb9d fires at district 1");
        g.shop_turn(Location::Dealers, "6", &mut no_input())
            .unwrap(); // gate district>2
        assert!(g.weapon_dubinka, "1000:cc56 fires at district 1");
        g.shop_turn(Location::Dealers, "7", &mut no_input())
            .unwrap(); // gate district>3
        assert!(g.pistol.owned, "1000:cd05 fires at district 1");
        g.shop_turn(Location::Dealers, "8", &mut no_input())
            .unwrap();
        assert_eq!(g.pistol.cartridges, 8, "1000:cda3 fires at district 1");
        g.dealer_delivery_counter = 25;
        g.shop_turn(Location::Dealers, "9", &mut no_input())
            .unwrap();
        assert!(g.pistol.silencer, "1000:ce34 fires at district 1");
        assert_eq!(g.player.money, 1_000 - 25 - 50 - 150 - 70 - 60);
    }

    fn market(money: i16) -> Game {
        let mut g = game();
        g.location = Location::Market;
        g.mode = Mode::Shop(Location::Market);
        g.player.money = money;
        g
    }

    /// `mar` row 3, Затемнённые очки -- `1000:bef6 mov byte [0x38b3],0x1`,
    /// one-shot through `1000:bece jnz 0xbf1f`.
    #[test]
    fn the_market_sells_the_dark_glasses_exactly_once() {
        let mut g = market(25);
        g.shop_turn(Location::Market, "3", &mut no_input()).unwrap();
        assert!(g.dark_glasses, "1000:bef6");
        assert_eq!(g.player.money, 15, "1000:bf00, price 10 at 20ae:0b30");
        g.shop_turn(Location::Market, "3", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 15, "1000:bf1f is a refusal, not a sale");

        // 1000:bed9 is a `jle`: 10 exactly buys, 9 does not.
        for (money, want) in [(9i16, false), (10, true)] {
            let mut g = market(money);
            g.shop_turn(Location::Market, "3", &mut no_input()).unwrap();
            assert_eq!(g.dark_glasses, want, "money {money}");
        }
    }

    /// `mar` rows 4 and 7, the two suits. The armour NUMBER is the point:
    /// row 7 adds the UPGRADE DELTA at `1000:c1b1` when the abibas suit is
    /// already owned and the full bonus at `1000:c1b7` when it is not
    /// (`1000:c1aa` / `1000:c1af`), so the total is 2 either way. An arm
    /// that always applied `+2` would read 3 in the first block below.
    #[test]
    fn the_market_suits_end_on_two_armour_in_either_purchase_order() {
        let mut g = market(100);
        g.shop_turn(Location::Market, "4", &mut no_input()).unwrap();
        assert!(g.wear_suit_abibas, "1000:bf80");
        assert_eq!(g.player.armor, 1, "1000:bfa7");
        g.shop_turn(Location::Market, "7", &mut no_input()).unwrap();
        assert!(g.wear_suit_adidas, "1000:c183");
        assert_eq!(g.player.armor, 2, "1000:c1b1, the delta, not 1000:c1b7");
        assert_eq!(g.player.money, 100 - 15 - 30);

        // The adidas suit alone takes the full bonus...
        let mut g = market(100);
        g.shop_turn(Location::Market, "7", &mut no_input()).unwrap();
        assert_eq!(g.player.armor, 2, "1000:c1b7");
        // ...and row 4's better-item gate 1000:bf51 then refuses, free.
        g.shop_turn(Location::Market, "4", &mut no_input()).unwrap();
        assert!(!g.wear_suit_abibas, "1000:bfc8 is a refusal");
        assert_eq!(g.player.money, 70);
        assert_eq!(g.player.armor, 2);

        // Both already-own gates: 1000:bf58 and 1000:c15b.
        let mut g = market(100);
        g.shop_turn(Location::Market, "4", &mut no_input()).unwrap();
        g.shop_turn(Location::Market, "4", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 85, "1000:bfad is a refusal, not a sale");
        g.shop_turn(Location::Market, "7", &mut no_input()).unwrap();
        g.shop_turn(Location::Market, "7", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 55, "1000:c1be is a refusal, not a sale");
        assert_eq!(g.player.armor, 2);
    }

    /// `mar` rows 5 and 8, the two pairs of boots. Same upgrade split, on
    /// the damage RANGE this time (`1000:c249` / `1000:c24e`): +1/+1 at
    /// `1000:c250`/`1000:c254` with the lesser boots owned, +2/+2 at
    /// `1000:c25a`/`1000:c25f` without.
    #[test]
    fn the_market_boots_end_on_two_damage_in_either_purchase_order() {
        let (base_min, base_max) = (game().player.dmg_min, game().player.dmg_max);

        let mut g = market(100);
        g.district = 3; // 1000:c1d7 is `cmp byte [0x3692],0x2`
        g.shop_turn(Location::Market, "5", &mut no_input()).unwrap();
        assert!(g.wear_boots, "1000:c029");
        assert_eq!(
            (g.player.dmg_min, g.player.dmg_max),
            (base_min + 1, base_max + 1),
            "1000:c050 / 1000:c054"
        );
        g.shop_turn(Location::Market, "8", &mut no_input()).unwrap();
        assert!(g.wear_boots_pontovye, "1000:c222");
        assert_eq!(
            (g.player.dmg_min, g.player.dmg_max),
            (base_min + 2, base_max + 2),
            "1000:c250 / 1000:c254, the delta"
        );
        assert_eq!(g.player.money, 100 - 15 - 30);

        // The better boots alone take the full +2/+2.
        let mut g = market(100);
        g.district = 3;
        g.shop_turn(Location::Market, "8", &mut no_input()).unwrap();
        assert_eq!(
            (g.player.dmg_min, g.player.dmg_max),
            (base_min + 2, base_max + 2),
            "1000:c25a / 1000:c25f"
        );
        // Row 5's better-item gate 1000:bffa then refuses, free.
        g.shop_turn(Location::Market, "5", &mut no_input()).unwrap();
        assert!(!g.wear_boots, "1000:c075 is a refusal");
        assert_eq!(g.player.money, 70);
        // Row 8's already-own gate 1000:c1fa.
        g.shop_turn(Location::Market, "8", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 70, "1000:c266 is a refusal, not a sale");

        // Row 5's own already-own gate 1000:c001, and its `jle` at
        // 1000:c00c: 15 exactly buys, 14 does not.
        for (money, want) in [(14i16, false), (15, true)] {
            let mut g = market(money);
            g.shop_turn(Location::Market, "5", &mut no_input()).unwrap();
            assert_eq!(g.wear_boots, want, "money {money}");
        }
        let mut g = market(100);
        g.shop_turn(Location::Market, "5", &mut no_input()).unwrap();
        g.shop_turn(Location::Market, "5", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 85, "1000:c05a is a refusal, not a sale");
    }

    /// `mar` rows 6 and 9, the two jackets: +2 at `1000:c107`, then either
    /// the delta +2 at `1000:c2f8` or the full +4 at `1000:c2ff`
    /// (`1000:c2f1` / `1000:c2f6`). Total 4 in either order.
    #[test]
    fn the_market_jackets_end_on_four_armour_in_either_purchase_order() {
        let mut g = market(100);
        g.district = 4; // 1000:c27f is `cmp byte [0x3692],0x3`
        g.shop_turn(Location::Market, "6", &mut no_input()).unwrap();
        assert!(g.wear_jacket, "1000:c0e0");
        assert_eq!(g.player.armor, 2, "1000:c107");
        g.shop_turn(Location::Market, "9", &mut no_input()).unwrap();
        assert!(g.wear_jacket_krutaya, "1000:c2ca");
        assert_eq!(g.player.armor, 4, "1000:c2f8, the delta, not 1000:c2ff");
        assert_eq!(g.player.money, 100 - 25 - 50);

        // The better jacket alone takes the full +4.
        let mut g = market(100);
        g.district = 4;
        g.shop_turn(Location::Market, "9", &mut no_input()).unwrap();
        assert_eq!(g.player.armor, 4, "1000:c2ff");
        // Row 6's better-item gate 1000:c0b1 then refuses, free.
        g.shop_turn(Location::Market, "6", &mut no_input()).unwrap();
        assert!(!g.wear_jacket, "1000:c129 is a refusal");
        assert_eq!(g.player.money, 50);
        // Row 9's already-own gate 1000:c2a2.
        g.shop_turn(Location::Market, "9", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 50, "1000:c306 is a refusal, not a sale");

        // Row 6's already-own gate 1000:c0b8, and its `jle` at 1000:c0c3.
        for (money, want) in [(24i16, false), (25, true)] {
            let mut g = market(money);
            g.district = 2;
            g.shop_turn(Location::Market, "6", &mut no_input()).unwrap();
            assert_eq!(g.wear_jacket, want, "money {money}");
        }
        let mut g = market(100);
        g.district = 2;
        g.shop_turn(Location::Market, "6", &mut no_input()).unwrap();
        g.shop_turn(Location::Market, "6", &mut no_input()).unwrap();
        assert_eq!(g.player.money, 75, "1000:c10e is a refusal, not a sale");
    }

    /// **The divergence Task 26 exists to reproduce.** The market's MENU
    /// gate `1000:bb80` hides rows 6 AND 7 below district 2; the BUY path
    /// gates only row 6, at `1000:c08e`. `1000:c095 jmp 0xc142` lands on
    /// row 7's setup with no district test in between, so the original
    /// sells the adidas suit off a menu that never listed it.
    ///
    /// The listing half goes through [`Game::listed_rows`] -- the predicate
    /// [`Game::print_priced_rows`] itself walks, not a copy of it, so
    /// deleting the menu's gate reds this rather than passing quietly.
    #[test]
    fn the_market_sells_row_7_off_a_menu_that_never_listed_it() {
        let listed = |d: u8| {
            let mut g = game();
            g.district = d;
            g.listed_rows("mar")
                .iter()
                .map(|r| r.key)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            listed(1),
            ["1", "2", "3", "4", "5"],
            "1000:bb80 hides rows 6 AND 7"
        );
        assert_eq!(listed(2), ["1", "2", "3", "4", "5", "6", "7"]);
        assert_eq!(listed(3), ["1", "2", "3", "4", "5", "6", "7", "8"]);
        assert_eq!(listed(4), ["1", "2", "3", "4", "5", "6", "7", "8", "9"]);

        let mut g = market(100);
        assert_eq!(g.district, 1);
        // Row 6 IS buy-gated: 1000:c095 skips it, silently.
        g.shop_turn(Location::Market, "6", &mut no_input()).unwrap();
        assert!(!g.wear_jacket, "1000:c08e is on the buy path");
        assert_eq!(g.player.money, 100);
        // Row 7 is NOT: 1000:c142 is reached with no district test.
        g.shop_turn(Location::Market, "7", &mut no_input()).unwrap();
        assert!(g.wear_suit_adidas, "1000:c183 fires at district 1");
        assert_eq!(g.player.money, 70);
        assert_eq!(g.player.armor, 2, "1000:c1b7");
        // Rows 8 and 9 have matching menu and buy gates, so both stay shut.
        g.shop_turn(Location::Market, "8", &mut no_input()).unwrap();
        g.shop_turn(Location::Market, "9", &mut no_input()).unwrap();
        assert_eq!(
            g.player.money, 70,
            "1000:c1de and 1000:c286 skip, in silence"
        );
        assert!(!g.wear_boots_pontovye);
        assert!(!g.wear_jacket_krutaya);
    }

    /// Every `mar` row in `data::shops()` is consumed by
    /// [`Game::buy_market_row`]. With [`Game::shop_action`]'s generic
    /// "debit and echo the menu line" path deleted, a tenth row added to the
    /// table without an arm would silently do nothing at all, and the
    /// `debug_assert!` there is what this makes true.
    #[test]
    fn every_market_row_has_an_arm_of_its_own() {
        for row in data::shops().iter().filter(|r| r.shop == "mar") {
            let mut g = market(0);
            assert!(
                g.buy_market_row(row.key, row.price),
                "mar row {} has no arm",
                row.key
            );
        }
    }

    /// Two rows of the eighteen carry more than one `#`, and every extra one
    /// is an instruction immediate rather than a price: `mar` row 2's `5` at
    /// `1000:ba5a` (file `0xD32A`) and `bmar` row 7's `20` and `30` at
    /// `1000:c7a7` / `1000:c7ab`. Both printed a bare `#` until Task 26 --
    /// the second was found by the sweep at the bottom of this test, which
    /// is why the sweep is here rather than an assertion about the one row
    /// the brief named.
    #[test]
    fn the_market_beer_row_fills_both_of_its_placeholders() {
        let row = |shop, key| {
            data::shops()
                .iter()
                .find(|r| r.shop == shop && r.key == key)
                .expect("row")
        };
        let rendered = |r: &data::ShopEntry| text::fill(r.text, &Game::row_fill_values(r));

        let beer = row("mar", "2");
        assert_eq!(beer.text, "#^7 руб.  Пиво(#з)");
        assert_eq!(
            rendered(beer),
            "5^7 руб.  Пиво(5з)",
            "1000:ba54 pushes the price byte, 1000:ba5a the literal 5"
        );

        // 20 and 30 are the MENU's own immediates. The shot itself rolls
        // 20..=29 (1000:4f14 / 1000:4f1d), so the 30 is the original's
        // off-by-one and it is reproduced, not corrected.
        let pistol = row("bmar", "7");
        assert!(
            rendered(pistol).ends_with("урон(20-30))."),
            "1000:c7a7 / 1000:c7ab: {}",
            rendered(pistol)
        );

        // And no row anywhere is left holding an unfilled placeholder.
        for r in data::shops() {
            assert!(
                !rendered(r).contains('#'),
                "{} row {} leaves a placeholder unfilled",
                r.shop,
                r.key
            );
        }
    }

    /// One `sheet_kit` wiring case: a setter for one `Game` field and the
    /// sheet line that field's DGROUP byte gates.
    type FlagCase = (fn(&mut Game), &'static str);

    /// The replacement for the old `inventory_lines` test, which covered
    /// five of the sheet's thirty rows. `crate::character_sheet` owns the
    /// rendering and tests it line by line; what is only testable HERE is
    /// [`Game::sheet_kit`]'s wiring, so this flips one `Game` field at a
    /// time and requires the line that field's DGROUP byte gates.
    ///
    /// A crossed pair of assignments in `sheet_kit` is the defect this
    /// catches and nothing else can: both sides are `bool`, so the compiler
    /// is happy and `character_sheet`'s own tests still pass.
    #[test]
    fn sheet_kit_wires_each_game_flag_to_its_own_sheet_line() {
        let cases: [FlagCase; 18] = [
            (|g| g.charm_krestik = true, "^1Крестик(Удача +2) "),
            (|g| g.charm_ring = true, "^1Кольцо \"Гс\"(Удача +1) "),
            (|g| g.oneshot_gift_1 = true, "^1Кольцо \"Пг\"(Всё +1) "),
            (|g| g.oneshot_gift_2 = true, "^1Мега Кольцо(Всё +4) "),
            (
                |g| g.ring_gospodi_pomilui = true,
                "^1Кольцо \"Гп\"(Самолечение) ",
            ),
            (|g| g.has_mobile = true, "^1У тебя есть мобильник"),
            (|g| g.dark_glasses = true, "^1У тебя есть тёмные очки"),
            (|g| g.prison_tattoo = true, "^1На тебе зоновская наколка"),
            (|g| g.pistol.owned = true, "^1У тебя есть пистолет"),
            (|g| g.wear_boots = true, "^1Бутсы(+1) "),
            (
                |g| g.wear_boots_pontovye = true,
                "^1Понтовые бутсы(Урон+2) ",
            ),
            (|g| g.weapon_kastet = true, "^1Кастет(+2) "),
            (|g| g.weapon_dubinka = true, "^1Дубинка(+4)  "),
            (|g| g.weapon_nozhik = true, "^1Нож(+6) "),
            (|g| g.weapon_tesak = true, "^1Тесак(Урон+9) "),
            (|g| g.tooth_guard = true, "^1Зубная защита  "),
            (|g| g.buff_countdown = 3, "^6Обдолбаный  "),
            (
                |g| {
                    g.player.armor = 2;
                    g.wear_suit_abibas = true;
                },
                "^1Костюм Abibas(+1) ",
            ),
        ];
        for (set, want) in cases {
            let mut g = game();
            let before = character_sheet::lines(&g.player, &g.player.name, &g.sheet_kit());
            assert!(
                !before.iter().any(|l| l.contains(want)),
                "{want:?} was already there before the flag was set"
            );
            set(&mut g);
            let after = character_sheet::lines(&g.player, &g.player.name, &g.sheet_kit());
            assert!(
                after.iter().any(|l| l.contains(want)),
                "{want:?} missing after setting its flag: {after:?}"
            );
        }
    }

    /// The four clothing flags whose line the armour gate hides
    /// (`1000:2280 ja 0x2285`) need armour before they can be seen at all,
    /// so they are wired separately from the loop above.
    #[test]
    fn sheet_kit_wires_the_three_remaining_clothing_flags() {
        let cases: [FlagCase; 3] = [
            (|g| g.wear_suit_adidas = true, "^1Костюм Adidas(+2) "),
            (|g| g.wear_jacket = true, "^1Кожанка(+2) "),
            (|g| g.wear_jacket_krutaya = true, "^1Крутая кожанка(+4) "),
        ];
        for (set, want) in cases {
            let mut g = game();
            g.player.armor = 2;
            // The "absent before" half its 18-case sibling has. Without it a
            // line that printed unconditionally would satisfy the assertion
            // below without the flag having done anything.
            let before = character_sheet::lines(&g.player, &g.player.name, &g.sheet_kit());
            assert!(
                !before.iter().any(|l| l.contains(want)),
                "{want:?} was already there before the flag was set"
            );
            set(&mut g);
            let after = character_sheet::lines(&g.player, &g.player.name, &g.sheet_kit());
            assert!(
                after.iter().any(|l| l.contains(want)),
                "{want:?} missing: {after:?}"
            );
        }
    }

    /// The three non-boolean fields `sheet_kit` copies.
    #[test]
    fn sheet_kit_carries_the_xp_pair_and_the_magazine() {
        let mut g = game();
        g.progress.xp = 7;
        g.progress.threshold = 30;
        g.pistol = crate::combat_dispatch::Pistol {
            owned: true,
            silencer: true,
            cartridges: 4,
        };
        let out = character_sheet::lines(&g.player, &g.player.name, &g.sheet_kit()).join("\n");
        assert!(
            out.contains("^6Сейчас у тебя 7 опыта, А для прокачки надо 30"),
            "{out}"
        );
        assert!(out.contains("^1 с гушителем"), "{out}");
        assert!(out.contains("^1! патронов - 4"), "{out}");
    }

    /// Only the "does not panic" half is asserted. `term` writes straight to
    /// this process's stdout, so a unit test cannot capture it; the
    /// "prints nothing" half rests on `inspect_enemy`'s early `return` when
    /// `last_enemy` is `None`, which is structural, not observed. Asserting
    /// it would need an output-capture harness the crate does not have.
    #[test]
    fn inspect_before_any_fight_does_not_panic() {
        let g = game();
        assert!(g.last_enemy.is_none());
        g.inspect_enemy();
    }

    /// Every flag the six `wes` arms gate on, set -- so all six are offered.
    fn all_sellable(g: &mut Game) {
        g.wear_suit_abibas = true; // 20ae:38b4
        g.wear_suit_adidas = true; // 20ae:38b7
        g.wear_boots = true; // 20ae:38b5
        g.wear_boots_pontovye = true; // 20ae:38b8
        g.wear_jacket = true; // 20ae:38b6
        g.wear_jacket_krutaya = true; // 20ae:38b9
        g.weapon_kastet = true; // 20ae:38ba
        g.weapon_dubinka = true; // 20ae:394b
        g.weapon_nozhik = true; // 20ae:38c2
        g.weapon_tesak = true; // 20ae:394c
    }

    /// The `1000:ce8c jle` refusal: CS 0x96f2 and nothing else changes.
    #[test]
    fn dealers_x_refuses_with_no_junk_and_changes_nothing() {
        let mut g = dealers(7);
        g.player.junk = 0;
        let out = term::capture::lines(|| {
            g.shop_turn(Location::Dealers, "x", &mut no_input())
                .unwrap();
        });
        assert_eq!(out, vec!["^4Тебе нечего спихнуть."]);
        assert_eq!(g.player.money, 7);
        assert_eq!(g.player.junk, 0);
    }

    /// `1000:ce87` is `83 3e c9 38 00` and `1000:ce8c` is a `jle`, so the
    /// word is read SIGNED: a Хлам word with the top bit set refuses too.
    /// `Fighter::junk` is now `i16`, the record's own width, so that word IS
    /// `i16::MIN` rather than a `u16` needing an `as i16` at the gate --
    /// which is what `Game::sell_junk` used to carry. A `u16` field with the
    /// cast dropped would have sold 32768 roubles' worth.
    #[test]
    fn dealers_x_refuses_a_junk_word_whose_top_bit_is_set() {
        let mut g = dealers(7);
        g.player.junk = i16::MIN;
        let out = term::capture::lines(|| {
            g.shop_turn(Location::Dealers, "x", &mut no_input())
                .unwrap();
        });
        assert_eq!(out, vec!["^4Тебе нечего спихнуть."]);
        assert_eq!(g.player.money, 7, "1000:ce8c takes the refusal");
        assert_eq!(g.player.junk, i16::MIN);
    }

    /// `1000:d35a`..`1000:d368` overwrite the shared buffer `20ae:3a72` with
    /// the one-character literal CS 0x98fa BEFORE the handler's exit compare
    /// at `1000:d377` reads it, so answering `w` to a sell offer cannot
    /// leave the dealers -- `1000:d37c` falls to `1000:d37e jmp 0xc88e`, the
    /// prompt push. Without that assign the answer would still be in the
    /// buffer and `w` would walk out.
    #[test]
    fn answering_w_to_a_sell_offer_does_not_leave_the_dealers() {
        let mut g = dealers(0);
        all_sellable(&mut g);
        term::capture::lines(|| {
            g.shop_turn(
                Location::Dealers,
                "wes",
                &mut input(&["w", "w", "w", "w", "w", "w"]),
            )
            .unwrap();
        });
        assert_eq!(g.mode, Mode::Shop(Location::Dealers));
        assert_eq!(g.location, Location::Dealers);
        assert_eq!(g.player.money, 0, "`w` is not `y`");
    }

    /// Nothing is unwound on a sale: no arm subtracts the sold item's stat
    /// bonus. The armour byte `20ae:38b2` and the damage words `20ae:38a8`
    /// and `20ae:38aa` are not among the thirteen DGROUP addresses
    /// `1000:ce76`..`1000:d383` references at all. Reproduced, not fixed.
    #[test]
    fn selling_everything_subtracts_no_armour_and_no_damage() {
        let mut g = dealers(0);
        all_sellable(&mut g);
        g.player.armor = 11;
        g.player.dmg_min = 4;
        g.player.dmg_max = 9;
        term::capture::lines(|| {
            g.shop_turn(
                Location::Dealers,
                "wes",
                &mut input(&["y", "y", "y", "y", "y", "y"]),
            )
            .unwrap();
        });
        assert_eq!(g.player.armor, 11);
        assert_eq!(g.player.dmg_min, 4);
        assert_eq!(g.player.dmg_max, 9);
    }

    /// EOF on a sell prompt ends the run rather than reading as "not `y`" --
    /// the same port decision `Game::mage` and `Game::district_advance`
    /// already take, and not a property of the original, which blocks in
    /// `ReadLn`.
    #[test]
    fn eof_on_a_sell_prompt_ends_the_run() {
        let mut g = dealers(0);
        all_sellable(&mut g);
        term::capture::lines(|| {
            g.shop_turn(Location::Dealers, "wes", &mut no_input())
                .unwrap();
        });
        assert!(!g.running);
        assert_eq!(g.player.money, 0);
        assert!(g.wear_suit_abibas, "1000:cf74 is not reached");
    }

    /// `x` and `wes` are dealers' sub-verbs (`1000:ce7b`, `1000:ced3`), not
    /// street verbs: neither is in `data/command_dispatch.json`'s confirmed
    /// chain, so the street prompt takes the silent `1000:ee01 jmp 0xab75`.
    /// Before this task `Game::dispatch` called the two arms, which would
    /// now sell from the street.
    #[test]
    fn the_street_prompt_has_no_x_and_no_wes() {
        let mut g = game();
        g.player.junk = 40;
        g.player.money = 3;
        all_sellable(&mut g);
        let out = term::capture::lines(|| {
            g.dispatch(Command::SellJunk, &mut no_input()).unwrap();
            g.dispatch(Command::SellItems, &mut input(&["y"])).unwrap();
        });
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(g.player.junk, 40);
        assert_eq!(g.player.money, 3);
        assert!(g.wear_suit_abibas);
    }

    #[test]
    fn sell_junk_and_sell_items_are_the_dealers_own_keys() {
        let mut g = dealers(0);
        // 1000:ce80 and 1000:ced8 are the two token compares; neither arm
        // can leave the shop (1000:d377 can never match on either path).
        term::capture::lines(|| {
            g.shop_turn(Location::Dealers, "x", &mut no_input())
                .unwrap();
        });
        assert_eq!(g.mode, Mode::Shop(Location::Dealers));
        term::capture::lines(|| {
            g.shop_turn(Location::Dealers, "wes", &mut no_input())
                .unwrap();
        });
        assert_eq!(g.mode, Mode::Shop(Location::Dealers));
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
    fn roll_enemy_produces_a_named_fighter() {
        let mut g = game();
        let e = g.roll_enemy(0);
        assert!(!e.name.is_empty());
        assert!(e.hp > 0);
    }

    /// `param_1` is the only thing `FUN_1000_0d14`'s two extra clamps
    /// (`1000:0da7`, `1000:0dba`) react to, and this port's own caller
    /// passes 0 -- so without this the two arms would ship untested.
    /// Driven over 200 seeds rather than one so the assertion is not
    /// satisfied by a single lucky roll.
    #[test]
    fn roll_enemy_param_clamps_the_class() {
        let mut saw_eight_without_the_clamp = false;
        for seed in 0..200u32 {
            let mut g = game();
            g.rng = Rng::new(seed);
            let free = g.roll_enemy(0).class;
            saw_eight_without_the_clamp |= free > 7;

            let mut g = game();
            g.rng = Rng::new(seed);
            assert!(
                g.roll_enemy(1).class <= 7,
                "seed {seed}: param_1 = 1 must clamp to 7 (1000:0dad)"
            );

            let mut g = game();
            g.rng = Rng::new(seed);
            assert_eq!(
                g.roll_enemy(2).class,
                8,
                "seed {seed}: param_1 = 2 must force 8 (1000:0dc0)"
            );
        }
        assert!(
            saw_eight_without_the_clamp,
            "no seed in 0..200 rolled a class above 7 with param_1 = 0, so the \
             param_1 = 1 assertion above never had anything to clamp"
        );
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

    /// I7: a dead player ends the game (`1000:5053` -> `FUN_1000_074b(0)`,
    /// whose tail-call at `1000:0ac0` reaches the RTL's `mov ah,0x4c` /
    /// `int 0x21` at file `0x1123C` -- see [`Game::run_combat`]), rather
    /// than walking on as a 0-HP corpse.
    #[test]
    fn player_death_stops_the_loop() {
        let mut g = game();
        g.player.hp = 0;
        let enemy = player();
        g.run_combat(0, enemy, &mut no_input()).unwrap();
        assert!(!g.running, "death must end the game");
    }

    /// Which [`IMM_ROWS`] rows [`Game::imm_row_visible`] lets through, for a
    /// given district / level / armour.
    fn visible(district: u8, level: u16, armor: u8) -> Vec<(&'static str, &'static str)> {
        let mut g = game();
        g.district = district;
        g.player.level = level;
        g.player.armor = armor;
        IMM_ROWS
            .iter()
            .filter(|r| g.imm_row_visible(r))
            .map(|r| (r.shop, r.key))
            .collect()
    }

    /// The vet's two rows carry no gate of their own, and the club's second
    /// and the gym's third, fourth and fifth are all shut at district 1:
    /// `1000:dfc4`, `1000:e4aa`, `1000:e51a` and `1000:e576` are `jbe` on
    /// `cmp byte [0x3692],1` (or `,2`), and district 1 fails every one.
    #[test]
    fn district_one_opens_only_the_ungated_imm_rows() {
        assert_eq!(
            visible(1, 0, 0),
            [
                ("rep", "h"),
                ("rep", "r"),
                ("kl", "1"),
                ("trn", "1"),
                ("trn", "2")
            ]
        );
    }

    /// `trn` row 3's second test is `district * 10 - 3 > level`
    /// (`1000:e4b1`..`1000:e4c2`): at district 2 it opens up to level 16 and
    /// shuts at 17.
    #[test]
    fn the_gyms_experience_row_closes_at_its_level_ceiling() {
        assert!(visible(2, 16, 0).contains(&("trn", "3")));
        assert!(!visible(2, 17, 0).contains(&("trn", "3")));
        // The ceiling moves with the district: 3 * 10 - 3 = 27.
        assert!(visible(3, 26, 0).contains(&("trn", "3")));
        assert!(!visible(3, 27, 0).contains(&("trn", "3")));
    }

    /// `trn` row 5 needs `district > 2` and `abs < district * 2`
    /// (`1000:e576`, `1000:e57d`..`1000:e58d`). `abs` is `20ae:3e34`,
    /// [`Game::trained_armour`] -- with no equipment it equals the armour.
    #[test]
    fn the_gyms_abs_row_needs_a_third_district_and_room_to_train() {
        assert!(!visible(2, 0, 0).contains(&("trn", "5")));
        assert!(visible(3, 0, 5).contains(&("trn", "5")));
        assert!(!visible(3, 0, 6).contains(&("trn", "5")));
    }

    /// `1000:e3a4`..`1000:e3e2` -- each item subtracts the armour it granted,
    /// and the lesser of a pair is skipped when the better one is owned
    /// (`1000:e3b1` `jnz` and `1000:e3cf` `jnz`), so the two suits never
    /// subtract 3 and the two jackets never subtract 6.
    #[test]
    fn trained_armour_subtracts_what_the_equipment_granted() {
        let abs = |armor, abibas, adidas, jacket, krutaya| {
            let mut g = game();
            g.player.armor = armor;
            g.wear_suit_abibas = abibas;
            g.wear_suit_adidas = adidas;
            g.wear_jacket = jacket;
            g.wear_jacket_krutaya = krutaya;
            g.trained_armour()
        };
        // Nothing owned: the scratch is the armour byte.
        assert_eq!(abs(10, false, false, false, false), 10);
        // One of each, alone.
        assert_eq!(abs(10, true, false, false, false), 9); // 1000:e3b8, -1
        assert_eq!(abs(10, false, true, false, false), 8); // 1000:e3c3, -2
        assert_eq!(abs(10, false, false, true, false), 8); // 1000:e3d6, -2
        assert_eq!(abs(10, false, false, false, true), 6); // 1000:e3e2, -4
                                                           // Both of a pair: the lesser is skipped, not added.
        assert_eq!(abs(10, true, true, false, false), 8);
        assert_eq!(abs(10, false, false, true, true), 6);
        // Everything: 10 - 2 - 4.
        assert_eq!(abs(10, true, true, true, true), 4);
    }

    /// `20ae:3e34` is a BYTE and both readers zero-extend (`xor ah,ah` at
    /// `1000:e589` and `1000:e890`), so the subtraction wraps instead of
    /// going negative -- and a wrapped scratch reads as far ABOVE either
    /// threshold, shutting the row and the arm rather than opening them.
    #[test]
    fn a_trained_armour_underflow_wraps_and_shuts_the_row() {
        let mut g = game();
        g.district = 3;
        g.player.armor = 1;
        g.wear_jacket_krutaya = true;
        assert_eq!(g.trained_armour(), 253); // 1 - 4, as a byte
        let row = IMM_ROWS
            .iter()
            .find(|r| r.shop == "trn" && r.key == "5")
            .unwrap();
        // 253 < 6 is false, so the row is hidden -- not shown as -3 would be.
        assert!(!g.imm_row_visible(row));
    }

    /// The witness the divergence was registered against: `SAVE_R4` holds
    /// `38b4`/`38b6`/`38b7` set and `38b9` clear with armour 10, so
    /// `abs = 10 - 2 - 2 = 6` against district 4's threshold of 8 and the
    /// row SHOWS. Substituting plain `armor` computed 10 and hid it, which
    /// is the whole of what this fix changes.
    #[test]
    fn the_save_r4_witness_now_shows_the_row_it_used_to_hide() {
        let mut g = game();
        g.district = 4;
        g.player.armor = 10;
        g.wear_suit_abibas = true;
        g.wear_suit_adidas = true;
        g.wear_jacket = true;
        g.wear_jacket_krutaya = false;
        assert_eq!(g.trained_armour(), 6);
        let row = IMM_ROWS
            .iter()
            .find(|r| r.shop == "trn" && r.key == "5")
            .unwrap();
        assert!(g.imm_row_visible(row));
        // The old substitution: armour 10 is not below 8.
        assert!(i32::from(g.player.armor) >= i32::from(g.district) * 2);
    }

    /// The composed line, for the two rows that exercise everything the
    /// assembly can do: the affordability digit flipping between `^0` and
    /// `^4`, and the one `#` in the whole table.
    #[test]
    fn an_imm_row_is_prefix_then_colour_digit_then_text() {
        let mut g = game();
        let gym3 = IMM_ROWS
            .iter()
            .find(|r| r.shop == "trn" && r.key == "3")
            .unwrap();
        let vet_h = IMM_ROWS
            .iter()
            .find(|r| r.shop == "rep" && r.key == "h")
            .unwrap();

        g.player.money = 0;
        assert_eq!(
            g.render_imm_row(gym3),
            " 3 -  ^410^7  прокачать 10 качков опыта"
        );
        assert_eq!(
            g.render_imm_row(vet_h),
            "  ^2h^7 - за ^43^7 рубля тебя залатают"
        );

        g.player.money = 1000;
        assert_eq!(
            g.render_imm_row(gym3),
            " 3 -  ^010^7  прокачать 10 качков опыта"
        );
        assert_eq!(
            g.render_imm_row(vet_h),
            "  ^2h^7 - за ^03^7 рубля тебя залатают"
        );
    }

    /// Every gate open: all nine rows, in image order.
    #[test]
    fn a_high_district_opens_every_imm_row() {
        assert_eq!(
            visible(4, 0, 0),
            [
                ("rep", "h"),
                ("rep", "r"),
                ("kl", "1"),
                ("kl", "2"),
                ("trn", "1"),
                ("trn", "2"),
                ("trn", "3"),
                ("trn", "4"),
                ("trn", "5"),
            ]
        );
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
        g.run_combat(0, enemy, &mut no_input()).unwrap();
        assert!(g.running);
        assert!(g.last_enemy.is_some());
        assert!(g.progress.xp > before, "an award must be credited");
    }

    /// `1000:dce0 cmp ax,0x28 / jl 0xdd32` -- the threshold is on
    /// `(level - (district-1)*10)*2 + pontovost_street`, computed here from
    /// the formula rather than hard-coded, per the task brief. District 1
    /// and `pontovost_street == 0` reduce it to `2 * level >= 0x28`, so
    /// `level == 20` is the exact boundary.
    #[test]
    fn den_reveal_gates_on_the_computed_threshold() {
        let district: i32 = 1;
        let pontovost_street: i16 = 0;
        // ax = (level - (district-1)*10) * 2 + pontovost_street; solved for
        // ax == 0x28 exactly (the boundary), then one level below it.
        let boundary_level =
            ((0x28 - i32::from(pontovost_street)) / 2 + (district - 1) * 10) as u16;

        let mut g = game();
        g.district = district as u8;
        g.pontovost_street = pontovost_street;
        g.player.level = boundary_level - 1;
        g.shop_turn(Location::Den, "a", &mut no_input()).unwrap();
        assert!(
            !g.places.is_found(Location::Dealers),
            "below the threshold, Dealers must not be revealed"
        );
        assert!(
            !g.places.is_found(Location::Gym),
            "below the threshold, Gym must not be revealed"
        );

        let mut g = game();
        g.district = district as u8;
        g.pontovost_street = pontovost_street;
        g.player.level = boundary_level;
        g.shop_turn(Location::Den, "a", &mut no_input()).unwrap();
        assert!(
            g.places.is_found(Location::Dealers),
            "at the threshold, Dealers must be revealed"
        );
        assert!(
            g.places.is_found(Location::Gym),
            "at the threshold, Gym must be revealed"
        );
    }

    /// `1000:dcbf jz 0xdcc8` / `1000:dcc6 jnz 0xdd32` -- Dealers set and Gym
    /// CLEAR must still fall through and reveal Gym. This is the exact
    /// assertion that catches `74`/`75` read backwards: misreading either
    /// jump would make this case skip instead of reveal.
    #[test]
    fn den_reveal_still_fires_when_dealers_is_set_and_gym_is_not() {
        let mut g = game();
        g.player.level = 40; // comfortably above the threshold at district 1
        g.places.mark_found(Location::Dealers);
        assert!(!g.places.is_found(Location::Gym));
        g.shop_turn(Location::Den, "a", &mut no_input()).unwrap();
        assert!(
            g.places.is_found(Location::Gym),
            "Dealers set + Gym clear must still reveal Gym (the fall-through)"
        );
    }

    /// Three gates the second `cargo mutants` run found once the first
    /// round of tests had shifted the line numbers: `1000:e020`'s club
    /// stake, `print_imm_rows`' shop filter, and the mage's PRINTED price.
    #[test]
    fn three_gates_the_second_mutants_run_uncovered() {
        // 1000:e020 -- the stake is reset on entering the CLUB and nowhere
        // else. `Game::new` already starts it at 5, so the test moves it
        // first or the assertion proves nothing.
        for (loc, reset) in [
            (Location::Club, true),
            (Location::Den, false),
            (Location::Gym, false),
        ] {
            let mut g = game();
            g.places.mark_found(loc);
            g.club_stake = 99;
            term::capture::lines(|| g.enter_shop(loc));
            assert_eq!(g.club_stake, if reset { 5 } else { 99 }, "{loc:?} stake");
        }

        // `print_imm_rows` filters IMM_ROWS by shop tag. With `!=` it would
        // print every OTHER shop's rows, so each tag is compared against
        // the rows the table itself says belong to it.
        for tag in ["rep", "kl", "trn"] {
            let mut g = game();
            g.player.broken_jaw = true;
            g.player.broken_leg = true;
            g.player.hp = 1;
            let out = term::capture::lines(|| g.print_imm_rows(tag));
            let want: Vec<String> = IMM_ROWS
                .iter()
                .filter(|r| r.shop == tag && g.imm_row_visible(r))
                .map(|r| g.render_imm_row(r))
                .collect();
            assert!(!want.is_empty(), "{tag} has no visible rows to compare");
            assert_eq!(out, want, "{tag}");
            // And nothing from a different shop leaked in.
            for other in ["rep", "kl", "trn"].iter().filter(|t| **t != tag) {
                for row in IMM_ROWS.iter().filter(|r| r.shop == *other) {
                    assert!(
                        !out.contains(&g.render_imm_row(row)),
                        "{tag} printed a {other} row"
                    );
                }
            }
        }

        // 1000:758d `ba 19 00` PRINTS `district * 25` while 1000:7605 and
        // 1000:7618 (`ba 32 00`) CHECK and CHARGE `district * 50` -- a
        // divergence inside the original that this port reproduces. The
        // charge was already pinned; the printed number was not, so both
        // are asserted here against the SAME district.
        for district in [1u8, 2, 4] {
            let mut g = game();
            g.district = district;
            g.player.money = 500;
            g.save_dir = std::env::temp_dir().join(format!("gopnik-mage2-{}", std::process::id()));
            std::fs::create_dir_all(&g.save_dir).unwrap();
            let out = term::capture::lines(|| {
                g.mage(&mut input(&["", "", "", "y"])).unwrap();
            });
            let shown = i64::from(district) * 25;
            assert!(
                out.iter().any(|l| l.contains(&shown.to_string())),
                "district {district} must advertise {shown}: {out:?}"
            );
            // Charged double what it advertised.
            assert_eq!(
                500 - g.player.money,
                i16::from(district) * 50,
                "district {district} charge"
            );
            std::fs::remove_dir_all(&g.save_dir).ok();
        }
    }

    /// `Game::shop_turn`'s vet tail -- `1000:d6c5 jmp 0xd4ba` returns to the
    /// LOOP TOP, not to the prompt, so a whole player is ejected by
    /// `crate::vet::loop_top` after the turn. **Unless the turn was the
    /// exit**: an exit taken at `1000:d6a8` or `1000:d6b9` reaches
    /// `1000:d6c8` without passing `1000:d4ba`.
    ///
    /// That second half is what the `self.location == Vet` conjunct models,
    /// and `cargo mutants` could rewrite the `&&` to `||` because no test
    /// exercised a whole player LEAVING -- only whole players staying.
    #[test]
    fn the_vet_loop_top_runs_after_a_turn_but_not_after_the_exit() {
        let eject = "^0Док: вали отсюда ты здоров.";
        for (key, exits) in [("zzz", false), ("w", true), ("e", true)] {
            let mut g = game();
            // Whole: 1000:d4ba's three tests all pass, so loop_top ejects.
            g.player.hp = g.player.hpmax;
            g.player.broken_jaw = false;
            g.player.broken_leg = false;
            g.places.mark_found(Location::Vet);
            g.location = Location::Vet;
            g.mode = Mode::Shop(Location::Vet);
            let out = term::capture::lines(|| {
                g.shop_turn(Location::Vet, key, &mut no_input()).unwrap();
            });
            assert_eq!(out.iter().any(|l| l == eject), !exits, "`{key}`: {out:?}");
            assert_eq!(
                g.location,
                Location::Street,
                "`{key}` must end on the street"
            );
        }

        // A HURT player stays: 1000:d4ba's first test fails, loop_top
        // returns without printing, and the vet keeps the prompt.
        let mut g = game();
        g.player.hp = 1;
        g.places.mark_found(Location::Vet);
        g.location = Location::Vet;
        g.mode = Mode::Shop(Location::Vet);
        let out = term::capture::lines(|| {
            g.shop_turn(Location::Vet, "zzz", &mut no_input()).unwrap();
        });
        assert!(!out.iter().any(|l| l == eject), "hurt must not be ejected");
        assert_eq!(g.location, Location::Vet, "hurt stays at the vet");
    }

    /// `1000:7dc7`/`7f5b`'s stage counter and `1000:8247`'s parting line.
    ///
    /// The counter saturates at 2 and the parting is indexed by
    /// `church_visits >= 2` read AFTER the raise, so the FIRST visit is the
    /// only one that prints `PARTING[0]`. `cargo mutants` left three alive
    /// here -- the `<= 1` guard, the `+= 1`, and the `>= 2` -- because no
    /// test ever called the church twice and looked at which parting came
    /// out.
    #[test]
    fn the_church_stage_saturates_and_picks_the_parting_line() {
        // Arm 3 of 1000:7f63 is the armour blessing: one draw, no sub-draw,
        // no level-up, so the visit is as short as the church gets.
        let seed = (0..1_000_000u32)
            .find(|&s| Rng::new(s).below(5) == 3)
            .expect("a seed drawing arm 3");

        let mut g = game();
        // (visit, church_visits after, parting index)
        for (visit, after, parting) in [(1u8, 1u8, 0usize), (2, 2, 1), (3, 2, 1), (4, 2, 1)] {
            g.rng = Rng::new(seed);
            let out = term::capture::lines(|| g.church(&mut no_input()));
            assert_eq!(g.church_visits, after, "visit {visit} counter");
            assert!(
                out.iter().any(|l| l == church::PARTING[parting]),
                "visit {visit} wanted PARTING[{parting}]: {out:?}"
            );
            // Exactly one of the two ever prints.
            assert!(
                !out.iter().any(|l| l == church::PARTING[1 - parting]),
                "visit {visit} printed both partings: {out:?}"
            );
            // 1000:828c's bare WriteLn then PARTING[2], on every visit.
            assert!(
                out.iter().any(|l| l == church::PARTING[2]),
                "visit {visit} lost the tail: {out:?}"
            );
        }
    }

    /// Seed whose draws satisfy `spec` in order: `Some(v)` is an exact
    /// value, `None` is "any non-zero", which is how a draw whose zero
    /// would fire an unrelated event is neutralised.
    fn walk_seed(spec: &[(u16, Option<u16>)]) -> u32 {
        (0..8_000_000u32)
            .find(|&seed| {
                let mut r = Rng::new(seed);
                spec.iter().all(|&(bound, want)| {
                    let got = r.below(bound);
                    match want {
                        Some(v) => got == v,
                        None => got != 0,
                    }
                })
            })
            .unwrap_or_else(|| panic!("no seed for {spec:?}"))
    }

    /// A walker with every place found and the four discovery draws
    /// therefore harmless, class 5, no ring. `phone` decides whether
    /// `1000:b022`/`b0ce` let draws 3 and 4 happen at all.
    fn walker(phone: bool) -> Game {
        let mut g = game();
        g.has_mobile = phone;
        g.player.class = 5;
        g.ring_gospodi_pomilui = false;
        for loc in [
            Location::Vet,
            Location::Market,
            Location::Club,
            Location::Gym,
            Location::Den,
        ] {
            g.places.mark_found(loc);
        }
        g
    }

    /// The draws a [`walker`] spends AFTER the two errand draws: the phone
    /// pair when it has one, then the four discovery rolls, the bucket, the
    /// church and the mage. All neutralised.
    fn walk_tail(phone: bool) -> Vec<(u16, Option<u16>)> {
        let mut v = Vec::new();
        if phone {
            v.push((200, None)); // 1000:b030, the wrong-number gag
            v.push((100, None)); // 1000:b0dc, the girl's call
        }
        v.extend([(10, None), (10, None), (100, None), (100, None)]);
        v.extend([(25, None), (200, None), (100, None)]);
        v
    }

    /// `1000:af68`'s errand and `1000:afc7`'s -- the flag is set BEFORE the
    /// den/phone tests, so a player without a phone loses the errand
    /// permanently and is never told.
    ///
    /// `cargo mutants` left both inner conjunctions alive: every case had
    /// the den found AND a phone, where `&&` and `||` agree.
    #[test]
    fn the_den_errands_fire_once_and_announce_only_with_a_phone() {
        let line1 = "^6? ты где щас? Тут помощь нужна.(Иди в притон)";
        for (den, phone, announces) in [
            (true, true, true),
            (false, true, false), // the den half of the `&&`
            (true, false, false), // the phone half
        ] {
            let mut g = walker(phone);
            if !den {
                g.places.reset_for_new_district(0);
            }
            let mut spec = vec![(20u16, Some(0u16)), (20, None)];
            spec.extend(walk_tail(phone));
            g.rng = Rng::new(walk_seed(&spec));
            let out = term::capture::lines(|| {
                g.wander_preamble(false, &mut no_input()).unwrap();
            });
            let label = format!("den {den} phone {phone}");
            assert_eq!(
                out.iter().any(|l| l.ends_with(line1)),
                announces,
                "{label}: {out:?}"
            );
            // 1000:af71 -- the flag is set whatever the gates say.
            assert!(g.den_errand_1_pending, "{label}: flag not set");
        }

        // 1000:afdc -- the second errand adds `понтовость >= 100`.
        let line2 = "^6? ты щас где? Базар есть.(Иди в притон)";
        for (den, ponty, announces) in [
            (true, 100i16, true),
            (true, 99, false),   // the boundary: `>= 100`
            (false, 100, false), // the den conjunct
        ] {
            let mut g = walker(true);
            g.den_errand_1_pending = true; // skip draw 1 entirely
            g.pontovost_street = ponty;
            if !den {
                g.places.reset_for_new_district(0);
            }
            let mut spec = vec![(20u16, Some(0u16))];
            spec.extend(walk_tail(true));
            g.rng = Rng::new(walk_seed(&spec));
            let out = term::capture::lines(|| {
                g.wander_preamble(false, &mut no_input()).unwrap();
            });
            let label = format!("den {den} ponty {ponty}");
            assert_eq!(
                out.iter().any(|l| l.ends_with(line2)),
                announces,
                "{label}: {out:?}"
            );
            assert!(g.den_errand_2_pending, "{label}: flag not set");
        }
    }

    /// `1000:b0dc`'s draw 4 -- the girl's call, `Random(100) == 0` AND the
    /// girl already found.
    #[test]
    fn the_girls_call_needs_both_a_zero_draw_and_a_girl() {
        let line = "Телефон(Твоя пассия):^5Привет, это я. Зайдешь ко мне сегодня?";
        for (draw, girl, prints) in [(0u16, true, true), (1, true, false), (0, false, false)] {
            let mut g = walker(true);
            g.den_errand_1_pending = true;
            g.den_errand_2_pending = true;
            if girl {
                g.places.mark_found(Location::Girl);
            }
            let mut spec = vec![(200u16, None), (100u16, Some(draw))];
            spec.extend([(10, None), (10, None), (100, None), (100, None)]);
            spec.extend([(25, None), (200, None), (100, None)]);
            g.rng = Rng::new(walk_seed(&spec));
            let out = term::capture::lines(|| {
                g.wander_preamble(false, &mut no_input()).unwrap();
            });
            assert_eq!(
                out.iter().any(|l| l == line),
                prints,
                "draw {draw} girl {girl}: {out:?}"
            );
        }
    }

    /// `1000:b186`/`b1b8`/`b1ea`/`b21c` -- the four discovery rolls. Each
    /// announces only when the place is NOT already found; `cargo mutants`
    /// could delete one of those `!`s because no case drove a zero draw
    /// against an already-found place.
    #[test]
    fn a_discovery_roll_announces_only_an_unknown_place() {
        // (index among the four draws, bound, line)
        let places = [
            (
                0usize,
                10u16,
                Location::Vet,
                "^1Ты спросил у прохожего где больница.",
            ),
            (1, 10, Location::Market, "^1Ты нашел базар."),
            (
                2,
                100,
                Location::Club,
                "^1Ты увидел объявление \"Типа заходи в наш понтовый клуб\".",
            ),
            (
                3,
                100,
                Location::Gym,
                "^1На стене реклама \"Жизнь тяжела. Если не хочешь сдохнуть качайся!\".",
            ),
        ];
        for (idx, _bound, loc, line) in places {
            for already in [false, true] {
                let mut g = walker(false);
                g.den_errand_1_pending = true;
                g.den_errand_2_pending = true;
                g.places.reset_for_new_district(0);
                g.places.mark_found(Location::Den); // keep the den out of it
                if already {
                    g.places.mark_found(loc);
                }
                // Only the roll under test draws a zero.
                let mut spec: Vec<(u16, Option<u16>)> = [10u16, 10, 100, 100]
                    .iter()
                    .enumerate()
                    .map(|(i, &b)| (b, if i == idx { Some(0) } else { None }))
                    .collect();
                spec.extend([(25, None), (200, None), (100, None)]);
                g.rng = Rng::new(walk_seed(&spec));
                let out = term::capture::lines(|| {
                    g.wander_preamble(false, &mut no_input()).unwrap();
                });
                let label = format!("{loc:?} already {already}");
                assert_eq!(out.iter().any(|l| l == line), !already, "{label}: {out:?}");
                assert!(g.places.is_found(loc), "{label}: not marked");
            }
        }
    }

    /// `Game::luck_below_random_32` -- the den's 32-bit luck compare.
    ///
    /// `cargo mutants` left `luck < random` → `<=` alive because nothing
    /// called it with the two EQUAL, the only input the two operators
    /// disagree on.
    #[test]
    fn luck_below_random_is_strict_at_equality() {
        for (luck, random, want) in [
            (5u16, 5u16, false), // equal: `<=` would say true
            (4, 5, true),
            (5, 4, false),
            (0, 0, false),
            (0, 1, true),
            // Only the luck side can be negative (1000:dda5's `cwd` against
            // 1000:dd9c's `xor dx,dx`), so a high-bit luck is BELOW any
            // random, however large the unsigned value looks.
            (0x8000, 1, true),
            (0x8000, 0, true),
        ] {
            assert_eq!(
                Game::luck_below_random_32(luck, random),
                want,
                "luck {luck} random {random}"
            );
        }
    }

    /// `Game::mage`'s price -- `district * 50` (`1000:7601`) and the
    /// refusal at `1000:7744`, which is `money < price`, so exactly the
    /// price BUYS.
    ///
    /// District 1 cannot pin the multiply (`1 * 50` and `1 + 50` are 50 and
    /// 51, but both refuse at money 40 and both pass at money 60 unless the
    /// row sits between them), so the rows below use district 2, where the
    /// product is 100 and the sum is 52.
    #[test]
    fn the_mage_charges_fifty_a_district_and_exactly_the_price_buys() {
        let broke = "^6Парень, все стоит бабок!";
        let dir = std::env::temp_dir().join(format!("gopnik-mage-{}", std::process::id()));
        for (district, money, refuses) in [
            (2u8, 99i16, true),
            (2, 100, false), // exactly the price buys -- pins `<` vs `<=`
            (2, 52, true),   // `+ 50` would have let this one through
            (2, 3, true),    // `/ 50` would have too
            (1, 49, true),
            (1, 50, false),
        ] {
            let mut g = game();
            g.district = district;
            g.player.money = money;
            g.save_dir = dir.clone();
            std::fs::create_dir_all(&dir).unwrap();
            let out = term::capture::lines(|| {
                g.mage(&mut input(&["", "", "", "y"])).unwrap();
            });
            let label = format!("district {district} money {money}");
            assert_eq!(out.iter().any(|l| l == broke), refuses, "{label}: {out:?}");
            let price = i16::from(district) * 50;
            assert_eq!(
                g.player.money,
                if refuses { money } else { money - price },
                "{label}"
            );
        }
        std::fs::remove_dir_all(&dir).ok();

        // 1000:775f -- anything but `y` declines before the price is even
        // computed, so a pauper sees the decline and not the refusal.
        let mut g = game();
        g.player.money = 0;
        let out = term::capture::lines(|| {
            g.mage(&mut input(&["", "", "", "n"])).unwrap();
        });
        assert!(
            out.iter()
                .any(|l| l == "^6Нехотите как хотите - мое дело предложить"),
            "{out:?}"
        );
        assert!(!out.iter().any(|l| l == broke), "declined, not refused");
    }

    /// `Game::round_half` -- Pascal's `Round` on a halved integer, the
    /// half-away-from-zero rule the original's `Round(x/2)` produces.
    ///
    /// A pure function with no state and no draw, and `cargo mutants` still
    /// had four live mutants in it, because nothing called it with an ODD
    /// argument: at even `twice` the `+ 1` is absorbed by the truncating
    /// divide and every rewrite of it agrees.
    #[test]
    fn round_half_rounds_halves_away_from_zero() {
        // (twice, want) -- `twice` is 2x, so 1 is 0.5 and 3 is 1.5.
        for (twice, want) in [
            (0i32, 0i32),
            (1, 1), // Round(0.5) = 1, the case that pins `+ 1`
            (2, 1),
            (3, 2), // Round(1.5) = 2
            (4, 2),
            (5, 3),
            (-1, -1),
            (-2, -1),
            (-3, -2),
            (-4, -2),
        ] {
            assert_eq!(Game::round_half(twice), want, "round_half({twice})");
        }
    }

    /// `Game::enter_shop`'s two location gates and `Game::visit_girl`'s
    /// two, each driven on both sides so the operator is what decides.
    ///
    /// `cargo mutants` left six alive here because every existing case sat
    /// where the condition's operands already agreed: a club test with the
    /// countdown at zero cannot tell `== Club` from `!= Club`, and a girl
    /// test with plenty of money cannot tell `< 12` from `<= 12`.
    #[test]
    fn the_shop_entry_gates_decide_on_their_own_operands() {
        let ban = "^6Тебе не стоит пока туда соваться";

        // 1000:df1a -- the club is open only while its countdown is zero.
        // The `loc == Club` half needs a BANNED non-club entry to pin: with
        // `!=` the countdown would lock every other shop instead.
        for (loc, countdown, refused) in [
            (Location::Club, 0u8, false),
            (Location::Club, 1, true),
            (Location::Den, 1, false),
        ] {
            let mut g = game();
            g.places.mark_found(loc);
            g.club_ban_countdown = countdown;
            let out = term::capture::lines(|| g.enter_shop(loc));
            let label = format!("{loc:?} countdown {countdown}");
            assert_eq!(out.iter().any(|l| l == ban), refused, "{label}: {out:?}");
            assert_eq!(g.mode == Mode::Street, refused, "{label} mode");
        }

        // 1000:d701 -- the Girl is not modal: she runs and hands control
        // back to the street. With `!= Girl` the visit would fire on every
        // OTHER location instead, so both halves are driven.
        let mut g = game();
        g.places.mark_found(Location::Girl);
        g.player.money = 50;
        g.player.hp = 1;
        g.rng = Rng::new(seed_drawing(&[(2, 1)]));
        term::capture::lines(|| g.enter_shop(Location::Girl));
        assert_eq!(g.mode, Mode::Street, "the girl is not modal");
        assert_eq!(g.player.money, 38, "the visit costs 12");
        assert_eq!(g.player.hp, g.player.hpmax, "the visit heals");

        let mut g = game();
        g.places.mark_found(Location::Den);
        g.player.money = 50;
        term::capture::lines(|| g.enter_shop(Location::Den));
        assert_eq!(g.mode, Mode::Shop(Location::Den), "the den IS modal");
        assert_eq!(g.player.money, 50, "the den charges nothing on entry");

        // 1000:d706 -- the refusal is `money < 12`, so 12 itself pays.
        for (money, paid) in [(11i16, false), (12, true)] {
            let mut g = game();
            g.player.money = money;
            g.player.hp = 1;
            g.rng = Rng::new(seed_drawing(&[(2, 1)]));
            let out = term::capture::lines(|| g.visit_girl());
            let label = format!("money {money}");
            assert_eq!(
                out.iter()
                    .any(|l| l == "^6Ну непойдёшь же как придурок без ничего."),
                !paid,
                "{label}: {out:?}"
            );
            assert_eq!(
                g.player.money,
                if paid { money - 12 } else { money },
                "{label}"
            );
        }

        // 1000:d728 -- the club reveal is `Random(2) == 0 AND the club is
        // not yet known`. A non-zero draw must not reveal (the `==`), and a
        // club already known must not print the line (the `&&`).
        let reveal = "^2Она вытащила тебя в клуб и теперь ты знаешь где он находиться.";
        for (draw, known, prints) in [(0u16, false, true), (1, false, false), (0, true, false)] {
            let mut g = game();
            g.player.money = 50;
            if known {
                g.places.mark_found(Location::Club);
            }
            g.rng = Rng::new(seed_drawing(&[(2, draw)]));
            let out = term::capture::lines(|| g.visit_girl());
            let label = format!("draw {draw} known {known}");
            assert_eq!(out.iter().any(|l| l == reveal), prints, "{label}: {out:?}");
            assert!(
                g.places.is_found(Location::Club) == (known || prints),
                "{label}"
            );
        }

        // 1000:d3dc..d3f2 -- the vet skips its menu only when the player is
        // WHOLE: full health AND no jaw AND no leg. Each row below is false
        // for a different conjunct, which is what separates the two `&&`s.
        let doc = "^0Док: не волнуйся всё зарастёт как на собаке";
        for (hp, jaw, leg, menu) in [
            (20u16, false, false, false), // whole: 1000:d3f4 skips
            (10, false, false, true),     // hurt only
            (20, true, false, true),      // jaw only
            (20, false, true, true),      // leg only
        ] {
            let mut g = game();
            g.player.hp = hp;
            g.player.broken_jaw = jaw;
            g.player.broken_leg = leg;
            let out = term::capture::lines(|| g.print_shop_intro(Location::Vet));
            assert_eq!(
                out.iter().any(|l| l == doc),
                menu,
                "hp {hp} jaw {jaw} leg {leg}: {out:?}"
            );
        }
    }

    /// The six output-only methods, each asserted to actually produce
    /// output -- `banner`, `print_priced_rows`, `print_imm_rows`,
    /// `inspect_enemy`, `print_enemy_block` and `shoot`.
    ///
    /// **`cargo mutants` replaced each of their whole bodies with `()` and
    /// every test stayed green.** The shop's entire price list could vanish
    /// and nothing noticed. The reason is a seam: `difftest.py`'s
    /// `priced_row` / `imm_row_site` / `menu_order` records compare the row
    /// DATA against `orig/g.exe`, and the module tests exercise the arms
    /// those rows dispatch to, but nothing asserted the port ever calls the
    /// printer. This test closes that seam only -- it deliberately does not
    /// restate row text, which the oracle already owns.
    #[test]
    fn the_output_only_methods_print_something() {
        // 1000:edb2's third copy of the version string, the `version` verb.
        let g = game();
        assert_eq!(
            term::capture::lines(|| g.banner()),
            vec!["^4Gopnik: ^7version 1.02 june,sept 2003".to_string()]
        );

        // The two priced menus. Every line the printer emits must carry the
        // numbered prefix its row index selects, and the count must equal
        // the rows `listed_rows` admits -- the tie to the data rather than a
        // copy of it.
        for tag in ["mar", "bmar"] {
            let g = game();
            let want: Vec<usize> = g
                .listed_rows(tag)
                .iter()
                .filter_map(|r| r.key.parse::<usize>().ok())
                .filter(|n| (1..=9).contains(n))
                .collect();
            assert!(!want.is_empty(), "{tag} lists no numbered rows");
            let out = term::capture::lines(|| g.print_priced_rows(tag));
            assert_eq!(out.len(), want.len(), "{tag}: {out:?}");
            for (line, idx) in out.iter().zip(&want) {
                assert!(
                    line.starts_with(Game::ROW_PREFIXES[idx - 1]),
                    "{tag} row {idx}: {line}"
                );
            }
        }

        // The three immediate menus. `trn` needs no gate; `rep` and `kl`
        // are driven through a Game that satisfies theirs.
        for tag in ["rep", "kl", "trn"] {
            let mut g = game();
            g.player.broken_jaw = true;
            g.player.broken_leg = true;
            g.player.hp = 1;
            let out = term::capture::lines(|| g.print_imm_rows(tag));
            assert!(!out.is_empty(), "{tag} printed nothing");
            for line in &out {
                assert!(!line.is_empty(), "{tag} printed a blank row");
            }
        }

        // `sv` -- the sheet with an enemy, and silence without one, which
        // is the port's recorded choice (no "nothing to inspect" string
        // exists in the image).
        let mut g = game();
        assert!(
            term::capture::lines(|| g.inspect_enemy()).is_empty(),
            "no last enemy must print nothing"
        );
        let enemy = player();
        let block = term::capture::lines(|| g.print_enemy_block(&enemy));
        assert_eq!(block, enemy_sheet::lines(&enemy), "the block IS the sheet");
        assert!(!block.is_empty());
        g.last_enemy = Some(enemy);
        assert_eq!(
            term::capture::lines(|| g.inspect_enemy()),
            block,
            "with an enemy, `sv` prints the same block"
        );

        // `sh` -- gated on the pistol, and silent without it.
        let mut g = game();
        assert!(
            term::capture::lines(|| g.shoot()).is_empty(),
            "no pistol must print nothing"
        );
        g.pistol.owned = true;
        assert_eq!(
            term::capture::lines(|| g.shoot()),
            vec!["^6Ты чё псих? мигом менты накроют!".to_string()]
        );
    }

    /// `1000:dcd3`..`dcf4`'s threshold -- `(level - (district - 1) * 10) * 2
    /// + понтовость >= 0x28`, the den's `a` reveal.
    ///
    /// **The district term needs two districts to pin.** `cargo mutants`
    /// left `- 1`→`+ 1` and `* 10`→`/ 10` alive against the existing tests,
    /// which all sat at district 1 where `(1 - 1) * 10` and `(1 + 1) * 10`
    /// and `(1 - 1) / 10` are not all distinguishable, and far enough above
    /// the threshold that the arithmetic never decided the answer. Row 3
    /// separates the `-`; row 4 separates the `*`, and needs district 3
    /// because at district 2 the quotient and the product agree on the
    /// verdict.
    #[test]
    fn the_den_reveal_threshold_counts_levels_within_the_district() {
        // (district, level, понтовость, reveals)
        for (district, level, ponty, want) in [
            (1u8, 20u16, 0i16, true), // lid 20 -> 40, exactly 0x28
            (1, 19, 0, false),        // lid 19 -> 38
            (1, 19, 2, true),         // the понтовость term reaches it
            (2, 20, 20, true),        // lid 10 -> 20 + 20; `+ 1` gives -10
            (3, 30, 0, false),        // lid 10 -> 20; `/ 10` gives lid 30
        ] {
            let mut g = game();
            g.district = district;
            g.player.level = level;
            g.pontovost_street = ponty;
            let out = term::capture::lines(|| g.den_reveal());
            let label = format!("district {district} level {level} ponty {ponty}");
            assert_eq!(g.places.is_found(Location::Gym), want, "{label}");
            assert_eq!(g.places.is_found(Location::Dealers), want, "{label}");
            // 1000:dd00 and 1000:dd19 -- two lines, or none.
            assert_eq!(out.len(), if want { 2 } else { 0 }, "{label}: {out:?}");
        }

        // 1000:dcbf / 1000:dcc6 -- both already found is the early return,
        // whatever the arithmetic says.
        let mut g = game();
        g.player.level = 40;
        g.places.mark_found(Location::Dealers);
        g.places.mark_found(Location::Gym);
        let out = term::capture::lines(|| g.den_reveal());
        assert!(out.is_empty(), "both set must be silent: {out:?}");
    }

    // The both-already-set skip (`1000:dcbf` clear + `1000:dcc6` taken) has
    // no assertable game-STATE effect once both flags are already found --
    // `mark_found` on an already-found slot is a no-op either way. Its only
    // other effect is the ABSENCE of two `WriteLn`s.
    // `tests/den_reveal_subprocess.rs` covers it by driving the real binary
    // and asserting on its piped stdout, the same technique
    // `tests/term_output.rs` uses; see that file's module doc for why a
    // synthesized save is what makes the precondition (Dealers and Gym
    // already found, threshold cleared) reachable deterministically, without
    // depending on the wall-clock RNG seed real play would need. That test
    // stays: it is the only check in the tree that the SHIPPED BINARY
    // reaches this arm. What is no longer true is the reason once given
    // here for it being the ONLY option -- `term::capture` (added by Task 28
    // for the den's arms, whose `d` branches need an RNG outcome a
    // subprocess cannot pin) now makes an in-process line assertion
    // possible too.

    // ---------------------------------------------------------------
    // Task 28 -- the den's submenu, `1000:d802`..`1000:df06`.
    // `docs/re/den.md` and `data/den_arms.json` are the map; every
    // expected string below is `data/strings.json`'s own `text`, quoted at
    // the file offset the artifact records, never retyped from a screen.
    // ---------------------------------------------------------------

    /// A den with every menu gate satisfied, at district 1.
    fn den_game_all_gates_open() -> Game {
        let mut g = game();
        g.places.mark_found(Location::Den);
        g.district = 1;
        g.player.level = 20;
        g.pontovost_street = 100; // >= 0x64: lines 7 and 16
        g.den_errand_1_pending = true; // lines 6 and 13
        g.den_errand_2_pending = true; // lines 7 and 16
        g.player.beer_dl = 1; // line 11's colour digit
        g.den_loan_credit = 1; // line 12's visibility
        g
    }

    /// `1000:d8b9`..`1000:dae2` with every gate open: all twelve lines and
    /// both blank `WriteLn`s (`1000:d8be`, `1000:d961`), in the original's
    /// order.
    #[test]
    fn the_den_menu_prints_all_twelve_lines_when_every_gate_is_open() {
        let g = den_game_all_gates_open();
        assert!(g.den_menu_reveal_hint(), "lines 8 and 15 must be open too");
        let out = term::capture::lines(|| g.print_den_menu());
        assert_eq!(
            out,
            vec![
                "",                                                              // 1000:d8be
                "^6На одного пацана наехал какой-то урод",                       // CS 0x9d46
                "^6Ты пацан нормальный. Есть дело.",                             // CS 0x9d6e
                "^6Пацаны хотят тебе кое-чё сказать",                            // CS 0x9d90
                "",                                                              // 1000:d961
                "Напиши ^6w^7 чтобы уйти",                                       // CS 0x9db3
                "Напиши ^0p^7  чтобы угостить пацанов пивом", // CS 0x9dcb + 0x9dd4
                "Напиши ^0r^7  чтобы занять 2 рубля",         // CS 0x9dcb + 0x9df6
                "Напиши ^6hp^7 чтобы отпинать мудака который наезжал на пацана", // CS 0x9e10
                "Напиши ^6s^7  чтобы узнать отношение",       // CS 0x9e4e
                "Напиши ^6a^7  чтобы спросить чё-то",         // CS 0x9e73
                "Напиши ^6d^7 чтобы пойти на дело",           // CS 0x9e96
            ]
        );
    }

    /// The same block with every gate CLOSED. Five lines survive: the two
    /// blank `WriteLn`s and the three ungated ones (10, 11, 14). Row 11 is
    /// present but DIMMED, because `1000:d984`'s test only chooses a colour
    /// -- it is not a visibility gate, unlike row 12's `1000:d9ec`.
    #[test]
    fn the_den_menu_hides_its_gated_lines_and_dims_the_beer_row() {
        let mut g = game();
        g.district = 1;
        g.player.level = 0;
        g.pontovost_street = 0;
        g.den_errand_1_pending = false;
        g.den_errand_2_pending = false;
        g.player.beer_dl = 0;
        g.den_loan_credit = 0;
        // (0 - 5) * 5 + 0 = -25, below 0x28.
        assert!(!g.den_menu_reveal_hint());
        let out = term::capture::lines(|| g.print_den_menu());
        assert_eq!(
            out,
            vec![
                "",
                "",
                "Напиши ^6w^7 чтобы уйти",
                "Напиши ^4p^7  чтобы угостить пацанов пивом",
                "Напиши ^6s^7  чтобы узнать отношение",
            ]
        );
    }

    /// Row 12's colour is `1000:d9d9 cmp word [0x38cb],0x2` /
    /// `1000:d9de jnl 0xd9e7` -- `>= 2` is the normal digit -- and its
    /// visibility is the separate `1000:d9ec cmp byte [0x3e35],0x0` /
    /// `jbe 0xda35`. Two different bytes, so they are checked apart.
    #[test]
    fn the_den_loan_row_dims_below_two_cred_and_vanishes_without_credit() {
        let row = |cred: i16, credit: u8| {
            let mut g = game();
            g.district = 1;
            g.pontovost_street = cred;
            g.den_loan_credit = credit;
            term::capture::lines(|| g.print_den_menu())
                .into_iter()
                .find(|l| l.contains("чтобы занять 2 рубля"))
        };
        assert_eq!(
            row(2, 1).as_deref(),
            Some("Напиши ^0r^7  чтобы занять 2 рубля"),
            "cred == 2 is the boundary of `jnl`"
        );
        assert_eq!(
            row(1, 1).as_deref(),
            Some("Напиши ^4r^7  чтобы занять 2 рубля"),
            "one below it dims"
        );
        assert_eq!(row(2, 0), None, "1000:d9ec hides the row entirely");
    }

    /// **The menu is wired to entry, and prints exactly once.** Every other
    /// menu test calls `print_den_menu` directly, which cannot see whether
    /// anything calls it -- deleting the call in [`Game::print_shop_intro`]
    /// left the whole suite green, and so would moving it into
    /// [`Game::shop_turn`], which would reprint the menu on every turn.
    /// This drives the real path: `dispatch(Command::Den)` ->
    /// `enter_shop(Location::Den)` -> `print_shop_intro`, then a `shop_turn`
    /// at the den prompt, and asserts the placement claim
    /// `print_den_menu`'s own doc rests on -- `1000:dede`, the loop's only
    /// back edge from below, targets the prompt push `1000:dae2` and not the
    /// menu.
    #[test]
    fn entering_the_den_prints_the_menu_once_and_a_turn_does_not_reprint_it() {
        // CS 0x9db3, the unconditional `w` line: present in every state, so
        // counting it counts menu prints and nothing else.
        const W_LINE: &str = "Напиши ^6w^7 чтобы уйти";
        // CS 0x9cf0, the intro's own prefix -- `print_den_intro`, not the
        // menu, so it separates "the menu ran" from "entry ran at all".
        const INTRO: &str = "Ты пришел в притон - ";
        let count = |v: &[String], t: &str| v.iter().filter(|l| l.contains(t)).count();

        let mut g = den_game_all_gates_open();
        assert_eq!(g.mode, Mode::Street);
        let entry = term::capture::lines(|| {
            g.dispatch(Command::Den, &mut no_input()).unwrap();
        });
        assert_eq!(g.mode, Mode::Shop(Location::Den), "the den is modal");
        assert_eq!(
            count(&entry, INTRO),
            1,
            "1000:d816's prefix, once: {entry:?}"
        );
        assert_eq!(
            count(&entry, W_LINE),
            1,
            "the menu must print on entry, exactly once: {entry:?}"
        );
        // Every other gated line is there too, so this is the whole menu and
        // not one surviving line.
        assert!(entry
            .iter()
            .any(|l| l == "Напиши ^6d^7 чтобы пойти на дело"));
        assert!(entry
            .iter()
            .any(|l| l == "Напиши ^6a^7  чтобы спросить чё-то"));

        // A turn at the prompt. `s` is chosen because it prints and writes
        // nothing, so anything else in the capture came from a reprint.
        let turn = term::capture::lines(|| {
            g.shop_turn(Location::Den, "s", &mut no_input()).unwrap();
        });
        assert_eq!(
            count(&turn, W_LINE),
            0,
            "1000:dede targets the prompt, not the menu -- no reprint: {turn:?}"
        );
        assert_eq!(count(&turn, INTRO), 0, "nor the intro: {turn:?}");
        assert_eq!(
            turn,
            vec![
                "^4Твоя понтовость сейчас = 100.",
                "^0Да если чё мы за тебя впрягаемся.",
            ],
            "the turn prints the `s` arm and nothing else"
        );

        // And an unrecognised key prints nothing at all -- the same shape,
        // measured through the real dispatch path rather than a direct call.
        let silent = term::capture::lines(|| {
            g.shop_turn(Location::Den, "zzz", &mut no_input()).unwrap();
        });
        assert!(silent.is_empty(), "{silent:?}");
    }

    /// Menu lines 7 and 16 are each a CONJUNCTION of two different bytes --
    /// `1000:d8e8`/`1000:d8ef` and `1000:dabb`/`1000:dac2` -- so each
    /// conjunct is varied on its own here. The all-gates-closed test above
    /// cannot separate them: it fails both at once, so dropping either
    /// compare would still pass there.
    #[test]
    fn the_den_menu_conjunction_lines_need_both_of_their_bytes() {
        let lines_for = |errand2: bool, cred: i16| {
            let mut g = game();
            g.district = 1;
            g.den_errand_2_pending = errand2;
            g.pontovost_street = cred;
            term::capture::lines(|| g.print_den_menu())
        };
        let deal = "^6Ты пацан нормальный. Есть дело."; // line 7, CS 0x9d6e
        let job = "Напиши ^6d^7 чтобы пойти на дело"; // line 16, CS 0x9e96
        let has = |v: &[String], t: &str| v.iter().any(|l| l == t);

        // Both bytes set: both lines.
        let both = lines_for(true, 100);
        assert!(has(&both, deal) && has(&both, job), "{both:?}");
        // The errand alone is not enough -- 1000:d8f4 and 1000:dac0 are
        // signed `jl`s against 0x64, so 99 is one below the boundary.
        let no_cred = lines_for(true, 99);
        assert!(!has(&no_cred, deal) && !has(&no_cred, job), "{no_cred:?}");
        // The cred alone is not enough either -- 1000:d8ed and 1000:dac7.
        let no_errand = lines_for(false, 100);
        assert!(
            !has(&no_errand, deal) && !has(&no_errand, job),
            "{no_errand:?}"
        );
    }

    /// **Controller ruling R1, measured.** Threshold blocks #1/#2
    /// ([`Game::den_menu_reveal_hint`], `1000:d90f` / `1000:da6e`) and
    /// block #3 ([`Game::den_reveal`], `1000:dcba`) are two predicates, and
    /// **neither implies the other**. Both directions are driven here, with
    /// the printed menu line checked alongside the flag, so folding the
    /// three into one helper fails this test whichever way it is folded.
    #[test]
    fn the_den_menu_hint_and_the_a_arm_disagree_in_both_directions() {
        // k = 1, cred = 38. Arm: 1*2 + 38 = 40 >= 0x28 -> fires.
        // Menu: (1 - 5)*5 + 38 = 18 < 0x28 -> the `a` row is not offered.
        let mut g = game();
        g.district = 1;
        g.player.level = 1;
        g.pontovost_street = 38;
        assert!(!g.den_menu_reveal_hint());
        let menu = term::capture::lines(|| g.print_den_menu());
        assert!(
            !menu.iter().any(|l| l.contains("чтобы спросить")),
            "menu line 15 must be absent: {menu:?}"
        );
        g.shop_turn(Location::Den, "a", &mut no_input()).unwrap();
        assert!(
            g.places.is_found(Location::Dealers) && g.places.is_found(Location::Gym),
            "the `a` ARM must still fire with no menu line offering it"
        );

        // k = 13, cred = 0. Arm: 13*2 + 0 = 26 < 0x28 -> refuses.
        // Menu: (13 - 5)*5 + 0 = 40 >= 0x28 -> the `a` row IS offered.
        let mut g = game();
        g.district = 1;
        g.player.level = 13;
        g.pontovost_street = 0;
        assert!(g.den_menu_reveal_hint());
        let menu = term::capture::lines(|| g.print_den_menu());
        assert!(
            menu.contains(&"Напиши ^6a^7  чтобы спросить чё-то".to_string()),
            "menu line 15 must be present: {menu:?}"
        );
        g.shop_turn(Location::Den, "a", &mut no_input()).unwrap();
        assert!(
            !g.places.is_found(Location::Dealers) && !g.places.is_found(Location::Gym),
            "the `a` ARM must refuse in silence while the menu offers it"
        );
    }

    /// The `1000:db38 jle` refusal: no beer, no effect, and the other
    /// literal (CS `0x9efb`).
    #[test]
    fn den_p_refuses_without_beer_and_changes_nothing() {
        let mut g = game();
        g.player.beer_dl = 0;
        g.pontovost_street = 7;
        let out =
            term::capture::lines(|| g.shop_turn(Location::Den, "p", &mut no_input()).unwrap());
        assert_eq!(out, vec!["^6А нет у тебя пива."]);
        assert_eq!(g.player.beer_dl, 0);
        assert_eq!(g.pontovost_street, 7);
    }

    /// `r` -- `1000:db77`..`1000:dbf3`. All three effects
    /// (`1000:db96`, `1000:db9b`, `1000:dba0`) and the confirmation.
    #[test]
    fn den_r_borrows_two_roubles_for_two_cred_and_one_credit() {
        let mut g = game();
        g.den_loan_credit = 3;
        g.pontovost_street = 5;
        g.player.money = 40;
        let out =
            term::capture::lines(|| g.shop_turn(Location::Den, "r", &mut no_input()).unwrap());
        assert_eq!(
            out,
            vec!["^2Ты занял 2 рубля на пиво. Понтовость уменьшилась на 2."]
        );
        assert_eq!(g.player.money, 42, "1000:db96 `add word [0x38c7],0x2`");
        assert_eq!(g.pontovost_street, 3, "1000:db9b `sub word [0x38cb],0x2`");
        assert_eq!(g.den_loan_credit, 2, "1000:dba0 `dec [0x3e35]`");
    }

    /// The two refusals are **different strings and not interchangeable**,
    /// and `1000:db8d` (the credit) is checked BEFORE `1000:db94` (the
    /// cred). The third case is what pins the order: with both exhausted
    /// the original prints the credit line, so a port that tested the cred
    /// first would print the other one here.
    #[test]
    fn den_r_has_two_distinct_refusals_and_checks_the_credit_first() {
        let refusal = |credit: u8, cred: i16| {
            let mut g = game();
            g.den_loan_credit = credit;
            g.pontovost_street = cred;
            g.player.money = 40;
            let out =
                term::capture::lines(|| g.shop_turn(Location::Den, "r", &mut no_input()).unwrap());
            assert_eq!(g.player.money, 40, "a refusal must not pay out");
            assert_eq!(g.pontovost_street, cred);
            assert_eq!(g.den_loan_credit, credit);
            out
        };
        // 1000:db8d only: credit gone, cred plentiful.
        assert_eq!(refusal(0, 50), vec!["^6Ты уже всю мелочь выгреб!"]);
        // 1000:db94 only: credit left, no cred. `jle` is signed, so 0 refuses.
        assert_eq!(refusal(3, 0), vec!["^6Ты не можешь занять денег."]);
        // Both: the credit refusal wins, because 1000:db8d comes first.
        assert_eq!(refusal(0, 0), vec!["^6Ты уже всю мелочь выгреб!"]);
    }

    /// With the errand pending: `1000:dc0e` rolls with `param_1 = 1`,
    /// `1000:dc11` sets the accept flag, `1000:dc53` announces the opponent
    /// and `1000:dc5e` consumes the errand after the fight returns.
    ///
    /// The expected announcement is composed from a SECOND game on the same
    /// seed whose only act is `roll_enemy(1)`, so the assertion pins the
    /// rank name (`1000:dc26`..`1000:dc2e`) and the level `1000:dc43`
    /// pushes without this test re-deriving either.
    #[test]
    fn den_hp_rolls_a_clamped_opponent_announces_it_and_consumes_the_errand() {
        let mut probe = game();
        let enemy = probe.roll_enemy(1);
        assert!(
            enemy.class <= 7,
            "1000:0dad clamps param_1 == 1 below the Мент"
        );
        let expected = format!(
            "^6Это {} {} уровня.",
            Game::rank_name(enemy.class),
            enemy.level
        );

        let mut g = game();
        g.den_errand_1_pending = true;
        // `run` at the fight prompt flees, which ends the fight and returns,
        // so 1000:dc5e is reached the way the original reaches it.
        let out = term::capture::lines(|| {
            g.shop_turn(Location::Den, "hp", &mut input(&["run"]))
                .unwrap()
        });
        assert_eq!(out.first().map(String::as_str), Some(expected.as_str()));
        assert!(g.fight_accepted, "1000:dc11 `mov byte [0x3b72],1`");
        assert!(
            !g.den_errand_1_pending,
            "1000:dc5e `mov byte [0x3b78],0`, after the fight"
        );
        assert_eq!(
            g.last_enemy.as_ref().map(|e| e.class),
            Some(enemy.class),
            "the fight must be against the opponent 1000:dc0e rolled"
        );
    }

    /// The 32-bit compare at `1000:dda6`..`1000:ddb3`: high halves SIGNED
    /// (`1000:dda8 jl`), low halves UNSIGNED (`1000:ddb1 jb`). The last two
    /// cases are what a single signed 16-bit compare would get wrong -- a
    /// luck word with bit 15 set is NEGATIVE after `cwd`, so it loses to
    /// every random, while an unsigned 16-bit compare would have it win.
    #[test]
    fn the_den_luck_compare_is_signed_high_and_unsigned_low() {
        assert!(!Game::luck_below_random_32(5, 5), "equal is not below");
        assert!(Game::luck_below_random_32(4, 5));
        assert!(!Game::luck_below_random_32(6, 5));
        // 0x8000 as a Longint is -32768, below any zero-extended Word.
        assert!(Game::luck_below_random_32(0x8000, 0));
        assert!(Game::luck_below_random_32(0xffff, 1));
        // Those last two are what pins the `cwd` at 1000:dda5: read as
        // plain unsigned 16-bit words, 0x8000 and 0xffff are ABOVE 0 and 1,
        // so a port that dropped the sign-extension would answer `false`
        // to both and this test would go red.
    }

    /// `w` at the den leaves, via `1000:ded7`'s compare and `1000:dee1`'s
    /// jump out. Everything else is silent and stays in the submenu --
    /// there is no "unknown command" literal in `1000:d802`..`1000:df06`
    /// for a bad key to print.
    #[test]
    fn the_den_leaves_on_w_and_is_silent_on_anything_else() {
        let mut g = game();
        g.places.mark_found(Location::Den);
        g.location = Location::Den;
        g.mode = Mode::Shop(Location::Den);
        let out =
            term::capture::lines(|| g.shop_turn(Location::Den, "zzz", &mut no_input()).unwrap());
        assert!(
            out.is_empty(),
            "an unrecognised key prints nothing: {out:?}"
        );
        assert_eq!(g.mode, Mode::Shop(Location::Den));
        g.shop_turn(Location::Den, "w", &mut no_input()).unwrap();
        assert_eq!(g.mode, Mode::Street);
        assert_eq!(g.location, Location::Street);
    }

    /// `1000:d914`/`d91b` -- the reveal hint's early return, and its
    /// byte-identical twin at `1000:da73`/`da7a`. Both flags found means no
    /// hint, whatever the arithmetic says. Every other den-menu test builds
    /// from `game()`, which has only the vet and market found
    /// (`new_game_starts_on_the_street_with_only_the_vet_and_market`), so
    /// until this test the early return was never taken TRUE: deleting it, or
    /// flipping the `&&` to `||`, left the whole suite green.
    #[test]
    fn the_den_reveal_hint_is_suppressed_once_both_places_are_found() {
        // Arithmetic that comfortably clears the 0x28 gate on its own:
        // (20 - 0)*... at district 1 is (20-5)*5 + 100 = 175.
        // 1000:d91d..1000:d93a is `(level - (district-1)*10 - 5) * 5 +
        // pontovost >= 0x28`. The rows below straddle that 40 exactly, so
        // the `-`, the `*` and the threshold each change the answer -- a
        // row that clears the gate by a wide margin leaves all three alive.
        for (level, district, ponty, want) in [
            (13u16, 1u8, 0i16, true), // (13-5)*5 + 0 = 40, the boundary
            (12, 1, 0, false),        // 35
            (12, 1, 5, true),         // 40 again, from the other term
            (23, 2, 0, true),         // the district subtraction: (13-5)*5
            (22, 2, 0, false),
        ] {
            let mut g = game();
            g.district = district;
            g.player.level = level;
            g.pontovost_street = ponty;
            assert_eq!(
                g.den_menu_reveal_hint(),
                want,
                "level {level} district {district} ponty {ponty}"
            );
        }

        let mut g = game();
        g.district = 1;
        g.player.level = 20;
        g.pontovost_street = 100;
        assert!(g.den_menu_reveal_hint(), "the gate itself must pass");

        g.places.mark_found(Location::Dealers);
        assert!(g.den_menu_reveal_hint(), "one flag is not enough");
        g.places.mark_found(Location::Gym);
        assert!(!g.den_menu_reveal_hint(), "both flags suppress the hint");
    }

    /// A `Game` configured so `wander_preamble` spends a KNOWN draw
    /// sequence: both errand flags already pending (their `&&` short-
    /// circuits before the draw), no phone (so `1000:b022`/`b0ce` jump past
    /// draws 3 and 4), class 5 (Гопник, `1000:b2e3`, no perk draws) and no
    /// ring (no draw 9). What remains is the four discovery draws, the
    /// bucket, the church and the mage.
    fn quiet_walker() -> Game {
        let mut g = game();
        g.den_errand_1_pending = true;
        g.den_errand_2_pending = true;
        g.has_mobile = false;
        g.ring_gospodi_pomilui = false;
        g.player.class = 5;
        for loc in [
            Location::Vet,
            Location::Market,
            Location::Club,
            Location::Gym,
        ] {
            g.places.mark_found(loc);
        }
        g
    }

    /// Seed for [`quiet_walker`]: no discovery draw hits its zero, the
    /// bucket draw is `bucket` when asked, and neither the church
    /// (`1000:b39e`) nor the mage (`1000:b3ae`) fires.
    fn quiet_walk_seed(bucket: Option<u16>) -> u32 {
        (0..5_000_000u32)
            .find(|&seed| {
                let mut r = Rng::new(seed);
                // 1000:b186, b1b8, b1ea, b21c.
                if [10u16, 10, 100, 100].iter().any(|&b| r.below(b) == 0) {
                    return false;
                }
                let roll = r.below(25); // 1000:b353
                if bucket.is_some_and(|w| roll != w) {
                    return false;
                }
                r.below(200) != 0 && r.below(100) != 0
            })
            .unwrap_or_else(|| panic!("no quiet seed for bucket {bucket:?}"))
    }

    /// The preamble's three per-walk counters -- `1000:af04` (the den's
    /// loan credit), `1000:af1d` (the dealers' delivery counter) and
    /// `1000:b16c`/`b177` (the two ban cooldowns).
    ///
    /// **`cargo mutants` named this cluster**: ~40 MISSED inside
    /// `wander_preamble`, the third and last of the big ones, and the same
    /// cause as the church's and the spoils' -- `difftest.py` compares text
    /// and reads no field.
    #[test]
    fn the_walk_preamble_counters_tick_once_each() {
        // 1000:af04 `jnl 0xaf1d` -- top up only while BELOW district * 10.
        for (district, credit, want) in [(1u8, 0u8, 1u8), (1, 9, 10), (1, 10, 10), (3, 10, 11)] {
            let mut g = quiet_walker();
            g.district = district;
            g.den_loan_credit = credit;
            g.rng = Rng::new(quiet_walk_seed(None));
            term::capture::lines(|| {
                g.wander_preamble(false, &mut no_input()).unwrap();
            });
            assert_eq!(
                g.den_loan_credit, want,
                "district {district} credit {credit}"
            );
        }

        // 1000:af1d/af24/af2b -- three gates, then the increment; the call
        // at 1000:af3d fires only on the turn it becomes exactly 25, and
        // only with a phone.
        let phone_line =
            "Телефон:^6Алё, ты где? Приходи, мы вещицу для тебя раздобыли.(Иди к барыгам)";
        for (found, pistol, phone, start, want, prints) in [
            (true, true, true, 0u8, 1u8, false),
            (true, true, true, 24, 25, true),
            // The phone gates only the MESSAGE; the counter still moves.
            (true, true, false, 24, 25, false),
            (true, true, true, 25, 25, false), // 1000:af2b, the < 25 gate
            (false, true, true, 0, 0, false),  // 1000:af1d, dealers unknown
            (true, false, true, 0, 0, false),  // 1000:af24, no pistol
        ] {
            let mut g = quiet_walker();
            if found {
                g.places.mark_found(Location::Dealers);
            }
            g.pistol.owned = pistol;
            g.has_mobile = phone;
            g.dealer_delivery_counter = start;
            g.rng = Rng::new(quiet_walk_seed(None));
            let out = term::capture::lines(|| {
                g.wander_preamble(false, &mut no_input()).unwrap();
            });
            let label = format!("found {found} pistol {pistol} phone {phone} start {start}");
            assert_eq!(g.dealer_delivery_counter, want, "{label}");
            assert_eq!(
                out.iter().any(|l| l == phone_line),
                prints,
                "{label}: {out:?}"
            );
        }

        // 1000:b11e / 1000:b145 read the countdown BEFORE 1000:b16c /
        // 1000:b177 decrement it, so the message lands on the last turn and
        // that same turn takes it to zero. Both need the den AND a phone.
        let market_line =
            "Телефон:^2Это ты там на базаре шухер наводил? Ну короче там менты свалили.";
        let club_line = "Телефон:^2Ты че там, в клуб-та пойдёшь. Уже утряслось всё.";
        for (start, den, phone, want, prints) in [
            (1u8, true, true, 0u8, true),
            (2, true, true, 1, false),
            (1, false, true, 0, false), // no den: silent, still ticks
            (1, true, false, 0, false), // no phone: 1000:b0ce jumps past both
            (0, true, true, 0, false),  // each `jz` guards its own byte
        ] {
            let mut g = quiet_walker();
            if den {
                g.places.mark_found(Location::Den);
            }
            g.has_mobile = phone;
            g.market_ban_countdown = start;
            g.club_ban_countdown = start;
            g.rng = Rng::new(quiet_walk_seed(None));
            let out = term::capture::lines(|| {
                g.wander_preamble(false, &mut no_input()).unwrap();
            });
            let label = format!("start {start} den {den} phone {phone}");
            assert_eq!(g.market_ban_countdown, want, "market {label}");
            assert_eq!(g.club_ban_countdown, want, "club {label}");
            assert_eq!(
                out.iter().any(|l| l == market_line),
                prints,
                "market {label}"
            );
            assert_eq!(out.iter().any(|l| l == club_line), prints, "club {label}");
        }
    }

    /// `1000:b353`'s bucket chain -- `Random(25)`, `1000:b358` stores
    /// `r + 1`, and `1000:b35c`..`b393` tests the highest boundary first.
    /// The boundaries are 10, 5 and 2 ON THE STORED VALUE, so they fall at
    /// draws 9, 4 and 1.
    #[test]
    fn the_walk_preamble_bucket_boundaries_are_2_5_and_10() {
        for (draw, want) in [
            (0u16, 1u8), // roll 1
            (1, 2),      // roll 2 -- the first boundary
            (3, 2),      // roll 4
            (4, 3),      // roll 5
            (8, 3),      // roll 9
            (9, 4),      // roll 10
            (24, 4),     // roll 25, the top
        ] {
            let mut g = quiet_walker();
            g.rng = Rng::new(quiet_walk_seed(Some(draw)));
            let mut bucket = 0;
            term::capture::lines(|| {
                bucket = g.wander_preamble(false, &mut no_input()).unwrap();
            });
            assert_eq!(bucket, want, "draw {draw} (roll {})", draw + 1);
        }
    }

    /// `1000:b24a`'s ring block and `1000:b2cc`'s class-perk dispatch --
    /// the preamble's two healing paths and the Вор's theft.
    ///
    /// The ring adds a draw (9, `1000:b272`) and class 6 adds two (10 and
    /// 11, `1000:b2fa`/`b321`), so each case names its own draw sequence
    /// after the four discovery rolls.
    ///
    /// **Two mutants of `1000:b251`'s block survive this test and are
    /// EQUIVALENT, not uncovered.** Rewriting its `hp < hpmax` to `<=` adds
    /// 3 at full health and the clamp below puts it straight back;
    /// rewriting the clamp's `hp > hpmax` to `>=` only differs when the two
    /// are already equal, where the assignment is a no-op. Neither changes
    /// any observable state, so no assertion can kill them and none is
    /// written pretending to.
    #[test]
    fn the_walk_preamble_ring_and_class_perks_move_hp() {
        /// Seed for a [`quiet_walker`] whose post-discovery draws are
        /// `mid`, then a bucket, then a quiet church and mage.
        fn seed_after_discovery(mid: &[(u16, u16)]) -> u32 {
            (0..5_000_000u32)
                .find(|&seed| {
                    let mut r = Rng::new(seed);
                    if [10u16, 10, 100, 100].iter().any(|&b| r.below(b) == 0) {
                        return false;
                    }
                    if !mid.iter().all(|&(bound, want)| r.below(bound) == want) {
                        return false;
                    }
                    r.below(25);
                    r.below(200) != 0 && r.below(100) != 0
                })
                .unwrap_or_else(|| panic!("no seed for {mid:?}"))
        }

        // 1000:b251..1000:b26b -- +3, clamped to hpmax, and only when
        // already below it. Draw 9 is non-zero here so no fracture clears.
        for (hp, hpmax, want) in [(10u16, 20u16, 13u16), (18, 20, 20), (20, 20, 20)] {
            let mut g = quiet_walker();
            g.ring_gospodi_pomilui = true;
            g.player.hp = hp;
            g.player.hpmax = hpmax;
            g.rng = Rng::new(seed_after_discovery(&[(20, 1)]));
            term::capture::lines(|| {
                g.wander_preamble(false, &mut no_input()).unwrap();
            });
            assert_eq!(g.player.hp, want, "ring hp {hp}/{hpmax}");
        }

        // Draw 9 == 0: at most ONE fracture clears, jaw first. The leg
        // block at 1000:b289 is reached only with the jaw intact
        // (1000:b280 `jnz 0xb2a7`).
        for (jaw, leg, want_jaw, want_leg) in [
            (true, true, false, true),   // jaw only, leg survives the turn
            (false, true, false, false), // leg clears when the jaw is intact
            (true, false, false, false),
        ] {
            let mut g = quiet_walker();
            g.ring_gospodi_pomilui = true;
            g.player.hp = g.player.hpmax; // no +3, isolate the fracture path
            g.player.broken_jaw = jaw;
            g.player.broken_leg = leg;
            g.rng = Rng::new(seed_after_discovery(&[(20, 0)]));
            term::capture::lines(|| {
                g.wander_preamble(false, &mut no_input()).unwrap();
            });
            assert_eq!(
                (g.player.broken_jaw, g.player.broken_leg),
                (want_jaw, want_leg),
                "fractures jaw {jaw} leg {leg}"
            );
        }

        // 1000:b2cf -- class 4 (Отморозок) heals one scratch a walk, and
        // only below hpmax. Classes 5 and anything unlisted heal nothing.
        for (class, hp, want) in [(4u16, 10u16, 11u16), (4, 20, 20), (5, 10, 10), (1, 10, 10)] {
            let mut g = quiet_walker();
            g.player.class = class;
            g.player.hp = hp;
            g.rng = Rng::new(quiet_walk_seed(None));
            term::capture::lines(|| {
                g.wander_preamble(false, &mut no_input()).unwrap();
            });
            assert_eq!(g.player.hp, want, "class {class} hp {hp}");
        }

        // 1000:b2ea -- the Вор. Draw 10 is `Random(district * 20)` and the
        // theft succeeds on `luck >= r`; draw 11 is `Random(district * 5)`
        // and the take is `r + 1` (1000:b326).
        // The `luck == r` row is the one that tells 1000:b305's `>=` from a
        // `>`: every other row is decided by the inequality's strict part.
        for (luck, r10, r11, gain) in [
            (5u16, 0u16, 2u16, 3i16),
            (5, 0, 0, 1),
            (0, 19, 0, 0),
            (0, 0, 0, 1),
        ] {
            let mut g = quiet_walker();
            g.player.class = 6;
            g.district = 1;
            g.player.luck = luck;
            let mid: &[(u16, u16)] = if gain == 0 {
                &[(20, r10)]
            } else {
                &[(20, r10), (5, r11)]
            };
            g.rng = Rng::new(seed_after_discovery(mid));
            let money = g.player.money;
            let out = term::capture::lines(|| {
                g.wander_preamble(false, &mut no_input()).unwrap();
            });
            assert_eq!(g.player.money - money, gain, "thief luck {luck} r {r10}");
            assert_eq!(
                out.iter()
                    .any(|l| l == &format!("^2Опа бабки! {gain} рублей на пиво!")),
                gain != 0,
                "thief line: {out:?}"
            );
        }
    }

    /// Seed whose next draws are exactly `(bound, value)` in order. `Rng`
    /// has no setter by design (`src/rng.rs`), so a test that wants a given
    /// arm searches for a seed that produces it.
    fn seed_drawing(draws: &[(u16, u16)]) -> u32 {
        (0..2_000_000u32)
            .find(|&seed| {
                let mut r = Rng::new(seed);
                draws.iter().all(|&(bound, want)| r.below(bound) == want)
            })
            .unwrap_or_else(|| panic!("no seed drawing {draws:?}"))
    }

    /// `(kastet, nozhik, dubinka, tesak)` -- the four weapon flags the
    /// spoils tables read, in the order the arms test them.
    type Owned = (bool, bool, bool, bool);

    /// One row of a weapon-spoils table: the arm's draw, the starting
    /// weapon set, and the damage both halves must move by.
    type SpoilRow = (u16, Owned, i64);

    fn arm_player(g: &mut Game, owned: Owned) {
        g.weapon_kastet = owned.0;
        g.weapon_nozhik = owned.1;
        g.weapon_dubinka = owned.2;
        g.weapon_tesak = owned.3;
    }

    /// The weapon-spoils damage table, every arm against every relevant
    /// starting weapon set -- `1000:552c`..`1000:560b` (`spoil_club`) and
    /// `1000:567d`..`1000:57cc` (`spoil_blade`).
    ///
    /// **`cargo mutants` is what said this was missing**: ~39 MISSED across
    /// the `spoil_*` / `grant_oneshot_gift` / `claim_spoils` cluster, the
    /// same structural cause as the church's ~65 -- `difftest.py`'s 363
    /// records compare text and read no field, so the damage arithmetic ran
    /// unchecked.
    ///
    /// Each expected delta is the original's own immediate, read off the
    /// cited store: `1000:5574`/`55da` (+2), `1000:55e6` (+4),
    /// `1000:56cf` (+4), `1000:56e0` (+2), `1000:56ff` (+6),
    /// `1000:577c` (+7), `1000:5794` (+5), `1000:57a5` (+3),
    /// `1000:57c4` (+9) -- summed per arm by the conditions above them,
    /// which are chained `if`s, not a `match`, so one arm can add several.
    ///
    /// Two rows look wrong and are not: a ножик found by a дубинка owner
    /// adds 6 (`56cf`'s 4 plus `56e0`'s 2), and a тесак found by a дубинка
    /// owner adds 12 (`577c`'s 7 plus `5794`'s 5). Both are what the chains
    /// compute; the port transliterates them rather than rationalising.
    #[test]
    fn the_weapon_spoils_table_adds_exactly_these_damages() {
        // (arm draw, starting (kastet, nozhik, dubinka, tesak), delta)
        let club: [SpoilRow; 9] = [
            // 1000:5530 draw 0 -- the кастет, "урон+2".
            (0, (false, false, false, false), 2),
            (0, (true, false, false, false), 0), // already owned: 1000:553a
            (0, (false, true, false, false), 0), // 1000:555f, better weapon
            (0, (false, false, true, false), 0), // 1000:5566
            (0, (false, false, false, true), 0), // 1000:556d
            // 1000:5530 draw 1 -- the дубинка, "урон+4".
            (1, (false, false, false, false), 4), // 1000:55e6
            (1, (true, false, false, false), 2),  // 1000:55da
            (1, (false, true, false, false), 0),  // 1000:55c5
            (1, (false, false, false, true), 0),  // 1000:55cc
        ];
        for (draw, owned, want) in club {
            let mut g = game();
            arm_player(&mut g, owned);
            g.rng = Rng::new(seed_drawing(&[(2, draw)]));
            let (lo, hi) = (g.player.dmg_min, g.player.dmg_max);
            term::capture::lines(|| g.spoil_club());
            assert_eq!(
                (
                    i64::from(g.player.dmg_min) - i64::from(lo),
                    i64::from(g.player.dmg_max) - i64::from(hi),
                ),
                (want, want),
                "club draw {draw} owning {owned:?}"
            );
        }

        let blade: [SpoilRow; 10] = [
            // 1000:5681 draw 0 -- the ножик, "урон+6".
            (0, (false, false, false, false), 6), // 56ff
            (0, (true, false, false, false), 4),  // 56cf
            (0, (false, false, true, false), 6),  // 56cf + 56e0
            (0, (true, false, true, false), 2),   // 56e0 only
            (0, (false, false, false, true), 0),  // 5709's message only
            (0, (false, true, false, false), 0),  // already owned
            // 1000:5681 draw 1 -- the тесак, "урон+9".
            (1, (false, false, false, false), 9), // 57c4
            (1, (true, false, false, false), 7),  // 577c
            (1, (false, true, false, false), 10), // 577c + 57a5
            (1, (false, false, true, false), 12), // 577c + 5794
        ];
        for (draw, owned, want) in blade {
            let mut g = game();
            arm_player(&mut g, owned);
            g.rng = Rng::new(seed_drawing(&[(2, draw)]));
            let (lo, hi) = (g.player.dmg_min, g.player.dmg_max);
            term::capture::lines(|| g.spoil_blade());
            assert_eq!(
                (
                    i64::from(g.player.dmg_min) - i64::from(lo),
                    i64::from(g.player.dmg_max) - i64::from(hi),
                ),
                (want, want),
                "blade draw {draw} owning {owned:?}"
            );
        }

        // Both tables set their flag on the granting pass and refuse the
        // second -- 1000:5541, 1000:55a7, 1000:5698, 1000:573e.
        for (draw, flag) in [(0u16, "kastet"), (1, "dubinka")] {
            let mut g = game();
            g.rng = Rng::new(seed_drawing(&[(2, draw), (2, draw)]));
            term::capture::lines(|| g.spoil_club());
            let set = if flag == "kastet" {
                g.weapon_kastet
            } else {
                g.weapon_dubinka
            };
            assert!(set, "{flag} flag not set");
            let (lo, hi) = (g.player.dmg_min, g.player.dmg_max);
            term::capture::lines(|| g.spoil_club());
            assert_eq!(
                (g.player.dmg_min, g.player.dmg_max),
                (lo, hi),
                "{flag} twice"
            );
        }
    }

    /// `spoil_charm` (`1000:547e`..`5512`) and `spoil_glasses`
    /// (`1000:5613`..`5672`) -- luck, and the two flags that carry no stat.
    #[test]
    fn the_charm_spoils_grant_luck_once_each() {
        // 1000:5493 `add [0x38a4],2` and 1000:54c4 `inc [0x38a4]`.
        for (draw, want) in [(0u16, 2i64), (1, 1), (2, 0)] {
            let mut g = game();
            g.rng = Rng::new(seed_drawing(&[(3, draw), (3, draw)]));
            let luck = g.player.luck;
            term::capture::lines(|| g.spoil_charm());
            assert_eq!(
                i64::from(g.player.luck) - i64::from(luck),
                want,
                "charm draw {draw}"
            );
            // The 1000:548c / 54bd / 54ed gates: a second find grants nothing.
            term::capture::lines(|| g.spoil_charm());
            assert_eq!(
                i64::from(g.player.luck) - i64::from(luck),
                want,
                "charm draw {draw} granted twice"
            );
        }

        // 1000:5621 and 1000:564d -- flags only, no stat moves at all.
        for draw in [0u16, 1] {
            let mut g = game();
            g.rng = Rng::new(seed_drawing(&[(2, draw)]));
            let before = (g.player.luck, g.player.dmg_min, g.player.dmg_max);
            term::capture::lines(|| g.spoil_glasses());
            assert_eq!(
                (g.player.luck, g.player.dmg_min, g.player.dmg_max),
                before,
                "glasses draw {draw} moved a stat"
            );
            assert!(
                if draw == 0 {
                    g.dark_glasses
                } else {
                    g.has_mobile
                },
                "glasses draw {draw} set no flag"
            );
        }
    }

    /// `1000:52e1`..`1000:53f2` -- the post-kill one-shot gift chain, the
    /// second grant site for the same three rings `Game::church`'s arm 2
    /// hands out (`docs/re/progression.md`: the 56 bytes at `1000:8101` and
    /// `1000:532f` compare equal).
    ///
    /// `dmg_min` starts at 3, not the fixture's 1: ring 1 leaves it at 4 and
    /// ring 2's `+= 2` at `1000:53a5` would read the same as `*= 2` from 2.
    #[test]
    fn the_post_kill_gift_chain_grants_each_ring_once_in_order() {
        let mut g = game();
        g.player.dmg_min = 3;
        let before = [
            i64::from(g.player.hp),
            i64::from(g.player.hpmax),
            i64::from(g.player.strength),
            i64::from(g.player.agility),
            i64::from(g.player.vitality),
            i64::from(g.player.luck),
            i64::from(g.player.dmg_min),
            i64::from(g.player.dmg_max),
        ];
        // `player()` starts at strength 5, so ring 1's `+= 1` lands on 6 and
        // 1000:534d's parity test fires; ring 2's `+= 2` is unconditional.
        let want: [[i64; 8]; 3] = [
            [6, 6, 1, 1, 1, 1, 1, 1],
            [30, 30, 5, 5, 5, 5, 3, 5],
            [30, 30, 5, 5, 5, 5, 3, 5],
        ];
        for (round, want) in want.iter().enumerate() {
            let out = term::capture::lines(|| g.grant_oneshot_gift());
            // The header at 1000:52e1 is gated on its own three-way OR,
            // separate from the if/else chain below it, so it still prints
            // on the round that hands out the last ring.
            assert!(
                out.contains(&"^1Оба на! Колечко! Вот свезло, так свезло!".to_string()),
                "round {round} printed no header: {out:?}"
            );
            let got = [
                i64::from(g.player.hp) - before[0],
                i64::from(g.player.hpmax) - before[1],
                i64::from(g.player.strength) - before[2],
                i64::from(g.player.agility) - before[3],
                i64::from(g.player.vitality) - before[4],
                i64::from(g.player.luck) - before[5],
                i64::from(g.player.dmg_min) - before[6],
                i64::from(g.player.dmg_max) - before[7],
            ];
            assert_eq!(&got, want, "gift round {round}");
        }
        assert!(g.oneshot_gift_1 && g.oneshot_gift_2 && g.ring_gospodi_pomilui);

        // With all three set the OR is false and the whole call is silent --
        // the one state that tells the header's gate apart from a constant.
        // Without this line, rewriting the first `!` out of that OR survives.
        let out = term::capture::lines(|| g.grant_oneshot_gift());
        assert!(out.is_empty(), "a fourth visit printed: {out:?}");
    }

    /// Every stat the church's draw-15 table moves, arm by arm --
    /// `1000:7f63`'s `Random(5)` and, inside its `1` arm, `1000:7fff`'s
    /// `Random(4)`.
    ///
    /// **This is the test that was missing, and `cargo mutants` is what said
    /// so.** A run over the tree returned ~65 MISSED mutants inside
    /// `Game::church` alone, nearly all of them `+=` rewritten to `-=` or
    /// `*=` on the award lines. They survived because this port's oracles
    /// compare TEXT -- `difftest.py`'s 363 records read no field at all --
    /// and every church test drove the sermon prose, never the awards. The
    /// whole reward table was code that nothing checked.
    ///
    /// The deltas are the addresses' own: `1000:8022`..`8043` (strength),
    /// `1000:8067` (agility), `1000:808b`..`8094` (vitality),
    /// `1000:80b9` (luck), `1000:8101`..`8134` and `1000:815c`..`8184`
    /// (the two rings), `1000:81e9` (armour), `1000:820d`..`821a`
    /// (понтовость).
    ///
    /// **Both strength parities are driven**, because `1000:8033`'s
    /// `is_multiple_of(2)` reads the ALREADY-incremented strength: starting
    /// even leaves it odd and `dmg_min` still, starting odd leaves it even
    /// and `dmg_min` moves. A single-parity table would leave that branch
    /// undefended the same way the awards were.
    #[test]
    fn the_church_award_table_moves_exactly_these_stats() {
        /// Seed whose next draws are `arm` at `Random(5)` and, when asked,
        /// `sub` at `Random(4)`. `Rng` has no setter by design, so a test
        /// that wants a given arm searches for a seed that produces it.
        fn seed_for(arm: u16, sub: Option<u16>) -> u32 {
            (0..1_000_000u32)
                .find(|&seed| {
                    let mut r = Rng::new(seed);
                    r.below(5) == arm && sub.is_none_or(|w| r.below(4) == w)
                })
                .unwrap_or_else(|| panic!("no seed for arm {arm} sub {sub:?}"))
        }

        /// The fields `1000:7f63`'s table can touch, in `src/model.rs` order.
        fn snapshot(g: &Game) -> [i64; 8] {
            let p = &g.player;
            [
                p.hp.into(),
                p.hpmax.into(),
                p.strength.into(),
                p.agility.into(),
                p.vitality.into(),
                p.luck.into(),
                p.dmg_min.into(),
                p.dmg_max.into(),
            ]
        }

        fn deltas(g: &Game, before: [i64; 8]) -> Vec<i64> {
            snapshot(g)
                .iter()
                .zip(before)
                .map(|(after, b)| after - b)
                .collect()
        }

        // (arm, sub, starting strength, [hp, hpmax, str, agi, vit, luck,
        //  dmg_min, dmg_max])
        let table: [(u16, Option<u16>, u16, [i64; 8]); 8] = [
            // 1000:8022 -- strength. Even start -> odd after, no dmg_min.
            (1, Some(0), 10, [1, 1, 1, 0, 0, 0, 0, 1]),
            // Odd start -> even after, so 1000:8043's dmg_min fires.
            (1, Some(0), 11, [1, 1, 1, 0, 0, 0, 1, 1]),
            (1, Some(1), 10, [0, 0, 0, 1, 0, 0, 0, 0]),
            (1, Some(2), 10, [5, 5, 0, 0, 1, 0, 0, 0]),
            (1, Some(3), 10, [0, 0, 0, 0, 0, 1, 0, 0]),
            // 1000:8101 -- the first ring, same parity rule as above.
            (2, None, 10, [6, 6, 1, 1, 1, 1, 0, 1]),
            (2, None, 11, [6, 6, 1, 1, 1, 1, 1, 1]),
            // 1000:81e9 -- armour only, no fighter stat.
            (3, None, 10, [0, 0, 0, 0, 0, 0, 0, 0]),
        ];

        for (arm, sub, strength, want) in table {
            let mut g = game();
            g.player.strength = strength;
            g.rng = Rng::new(seed_for(arm, sub));
            let before = snapshot(&g);
            let armour_before = g.player.armor;
            let ponty_before = g.pontovost_street;
            term::capture::lines(|| g.church(&mut no_input()));
            assert_eq!(
                deltas(&g, before),
                want.to_vec(),
                "arm {arm} sub {sub:?} strength {strength}"
            );
            // 1000:81e9 `inc byte [0x38b2]` is the armour arm's and nothing
            // else in the table touches armour.
            assert_eq!(
                g.player.armor - armour_before,
                u8::from(arm == 3),
                "arm {arm} moved armour"
            );
            assert_eq!(
                g.pontovost_street, ponty_before,
                "only arm 4 moves понтовость"
            );
        }

        // 1000:81ef's arm: `district * 50 + 50`, and it prints the amount.
        for district in [1u8, 3, 5] {
            let mut g = game();
            g.district = district;
            g.rng = Rng::new(seed_for(4, None));
            let before = snapshot(&g);
            let ponty_before = g.pontovost_street;
            let out = term::capture::lines(|| g.church(&mut no_input()));
            let gain = i16::from(district) * 50 + 50;
            assert_eq!(
                g.pontovost_street - ponty_before,
                gain,
                "district {district}"
            );
            assert_eq!(snapshot(&g), before, "arm 4 moves no fighter stat");
            assert!(
                out.iter().any(|l| l == &format!("^1Получи {gain}!")),
                "arm 4 prints its amount: {out:?}"
            );
        }

        // Arm 2's three gifts fire in order, one per visit, and the third is
        // text-only (1000:818b's gate, 1000:81c4). `player()` starts at
        // strength 5, so ring 1 lands it on 6 (dmg_min +1) and ring 2's
        // unconditional `+= 2` at 1000:817d follows.
        let mut g = game();
        // `dmg_min` starts at 3, not the fixture's 1, on purpose: ring 1
        // leaves it at 2 from the default, and `2 += 2` and `2 *= 2` are
        // both 4 -- the mutant would survive on an arithmetic accident of
        // the fixture rather than on a gap in the assertion. From 3 the two
        // give 6 and 8.
        g.player.dmg_min = 3;
        let before = snapshot(&g);
        for round in 0..3 {
            g.rng = Rng::new(seed_for(2, None));
            term::capture::lines(|| g.church(&mut no_input()));
            let want = match round {
                0 => vec![6, 6, 1, 1, 1, 1, 1, 1],
                _ => vec![30, 30, 5, 5, 5, 5, 3, 5],
            };
            assert_eq!(deltas(&g, before), want, "gift round {round}");
        }
        assert!(g.oneshot_gift_1 && g.oneshot_gift_2 && g.ring_gospodi_pomilui);
    }

    /// `run` is NOT a shop exit. `1000:ded7` compares the buffer against CS
    /// `0x848e`, the one-byte shortstring `01 77` (`w`), and nothing else --
    /// a `run` misses it and takes the back edge at `1000:dee1`'s fall-through
    /// to the prompt, silently. The street is the only dispatch with a second
    /// synonym (`1000:ae86`'s `w` AND `1000:ae97`'s `run`), which is why
    /// `commands::parse` folds the two, and why `shop_turn` must not use it.
    ///
    /// Every location goes through the same shared arm, so this holds for the
    /// vet, club, gym, market and dealers too; the vet's extra `e` exit
    /// (`1000:d6be`) is its own arm above and is unaffected.
    #[test]
    fn run_is_a_street_synonym_and_does_not_leave_a_shop() {
        for loc in [
            Location::Den,
            Location::Club,
            Location::Gym,
            Location::Market,
        ] {
            let mut g = game();
            g.places.mark_found(loc);
            g.location = loc;
            g.mode = Mode::Shop(loc);
            let out = term::capture::lines(|| g.shop_turn(loc, "run", &mut no_input()).unwrap());
            assert!(out.is_empty(), "{loc:?} printed on `run`: {out:?}");
            assert_eq!(g.mode, Mode::Shop(loc), "`run` left {loc:?}");
            assert_eq!(g.location, loc, "`run` left {loc:?}");
        }
    }

    /// `1000:ae13`/`1000:ae1f` via [`Game::enter_district_5`] -- reaching
    /// district 5 sets `rector_showdown`, and the flee refusal at
    /// `1000:48eb` ([`Game::flee`]) is the reader this test drives live.
    ///
    /// **Task 21 changed which method this test drives, and that is the
    /// point.** It used to call `run_combat` and assert the promotion
    /// happened inside the post-fight block; the original promotes at
    /// `1000:ab92`, in the top-of-turn block `1000:ee01` jumps back to, and
    /// `FUN_1000_3d11` has no district write at all
    /// (`tools/re_query.py xrefs-to 20ae:3692`: the only in-play write is
    /// `1000:ab92`). So the old call site was the wrong one and the test
    /// encoded it. The keystroke consumed here is `1000:ac31`'s `ReadLn`,
    /// answered `n` so the save arm is not taken; `1000:addc`'s `ReadKey`
    /// inside `enter_district_5` takes the second.
    #[test]
    fn reaching_district_5_arms_the_rector_showdown_and_flee_refuses() {
        let mut g = game();
        g.district = 4;
        g.player.level = 40; // `player.level >= district * 10`: promotes to 5
        assert!(!g.rector_showdown);
        g.district_advance(&mut input(&["n", ""])).unwrap();
        assert_eq!(g.district, 5);
        assert!(
            g.rector_showdown,
            "1000:ae13 must fire the turn district reaches 5"
        );
        assert!(
            g.places.is_found(Location::Den),
            "1000:ae1f must grant the Den in the same arm"
        );

        // The reader: 1000:48eb refuses `run` outright while rector_showdown
        // is set. Reusing `g` itself -- not a fresh instance with the field
        // poked directly -- is the point: this is the flag `enter_district_5`
        // just armed, changing behaviour on the very game it armed it on,
        // where before Task 20 this arm was reachable only by setting the
        // field directly in a test.
        let fled = g.flee();
        assert!(!fled, "1000:48eb refuses every flee once the flag is set");
    }

    /// A won fight no longer promotes: `FUN_1000_3d11` writes no district
    /// byte, and `1000:ab75` runs at the top of the NEXT turn.
    #[test]
    fn a_won_fight_does_not_advance_the_district_by_itself() {
        let mut g = game();
        g.district = 1;
        g.player.level = 40;
        let mut dead_enemy = punchbag();
        dead_enemy.hp = 0; // already dead: the win branch runs with no draw
        g.run_combat(0, dead_enemy, &mut no_input()).unwrap();
        assert_eq!(
            g.district, 1,
            "1000:3d11 has no write to [0x3692]; only 1000:ab92 does"
        );
        // ...and the very next turn's hook is what collects it.
        g.district_advance(&mut input(&["n"])).unwrap();
        assert_eq!(g.district, 2);
    }

    /// **One district per turn, never four.** `1000:ab75`..`1000:ad12`
    /// contains no backward branch (`ab83`, `ab85`, `ab8d`, `ab8f`, `aba5`,
    /// `abb6`, `abc7`, `ac59` and `ac5b` are all forward), and the only
    /// branch in the image targeting `0xab75` is `1000:ee01`, at the end of
    /// the turn. The port used to run the gate in a `while` loop, which
    /// collapsed all four promotions into the fight that earned them.
    #[test]
    fn the_advance_gains_at_most_one_district_per_turn() {
        let mut g = game();
        g.district = 1;
        g.player.level = 40; // clears every gate up to district 5 at once
        for want in [2u8, 3, 4] {
            g.district_advance(&mut input(&["n"])).unwrap();
            assert_eq!(g.district, want, "one 1000:ab92 per pass, not a loop");
        }
        // The fourth pass reaches 5 and takes the chapter-5 arm, which eats a
        // second line at 1000:addc.
        g.district_advance(&mut input(&["n", ""])).unwrap();
        assert_eq!(g.district, 5);
        // 1000:ab88's `jb` refuses a sixth.
        g.district_advance(&mut input(&["n"])).unwrap();
        assert_eq!(g.district, 5, "1000:ab8f jumps past the increment");
    }

    /// Both gates refuse before anything is read: a failed gate jumps to
    /// `1000:ae18` at `1000:ab85`/`1000:ab8f`, ahead of `1000:ac31`'s
    /// `ReadLn`, so the line the street prompt is about to read must still be
    /// there.
    #[test]
    fn a_refused_advance_prints_nothing_and_consumes_no_line() {
        let mut g = game();
        g.district = 2;
        g.player.level = 19; // 2 * 10 > 19 -- 1000:ab83's `jle` is not taken
        let mut lines = input(&["w"]);
        g.district_advance(&mut lines).unwrap();
        assert_eq!(g.district, 2);
        assert_eq!(
            lines.next().unwrap().unwrap(),
            "w",
            "1000:ab85 jumps past 1000:ac31's ReadLn"
        );
    }

    /// `1000:abce`/`1000:abd3` clear both ban countdowns, and they are
    /// cleared on the advance itself -- not by the save arm, which the `n`
    /// here declines.
    #[test]
    fn the_advance_clears_both_ban_countdowns() {
        let mut g = game();
        g.district = 1;
        g.player.level = 10;
        g.market_ban_countdown = 4;
        g.club_ban_countdown = 3;
        g.district_advance(&mut input(&["n"])).unwrap();
        assert_eq!(g.district, 2);
        assert_eq!(g.market_ban_countdown, 0, "1000:abce");
        assert_eq!(g.club_ban_countdown, 0, "1000:abd3");
    }

    /// `1000:b95e` `cmp byte [0x3b76],0x0` / `1000:b963 jz 0xb968` -- the
    /// market is open only while the countdown is zero, and a non-zero one
    /// takes `1000:b965 jmp 0xc480` to a refusal that neither enters nor
    /// changes anything. `docs/re/port-gaps.md` row 25.
    #[test]
    fn the_market_ban_gate_refuses_entry_while_the_countdown_stands() {
        for standing in [5u8, 1] {
            let mut g = game();
            g.market_ban_countdown = standing;
            let out = term::capture::lines(|| {
                g.dispatch(Command::Market, &mut no_input()).unwrap();
            });
            assert_eq!(out, vec![market::BANNED.to_string()], "1000:c494");
            assert_eq!(g.location, Location::Street, "1000:b965 never enters");
            assert_eq!(g.mode, Mode::Street, "and no submenu is opened");
            assert_eq!(g.market_ban_countdown, standing, "the gate only reads");
        }
        // Zero takes `jz 0xb968`, the intro at 1000:b96b.
        let mut g = game();
        let out = term::capture::lines(|| {
            g.dispatch(Command::Market, &mut no_input()).unwrap();
        });
        assert!(
            out.first().map(|l| l.starts_with("Ты пришел на базар")) == Some(true),
            "1000:b96b, got {out:?}"
        );
        assert_eq!(g.mode, Mode::Shop(Location::Market));
    }

    /// `1000:b954` runs BEFORE `1000:b95e` and its miss jumps to
    /// `1000:c49b`, not `1000:c480`, so an undiscovered market prints its
    /// own refusal whatever the countdown says. A port that tested the ban
    /// first would print the other line.
    #[test]
    fn the_discovery_gate_runs_before_the_market_ban_gate() {
        let mut g = game();
        g.places = Places::from_bytes(&[0u8; 7]);
        g.market_ban_countdown = 5;
        let out = term::capture::lines(|| {
            g.dispatch(Command::Market, &mut no_input()).unwrap();
        });
        assert_eq!(
            out,
            vec!["^6Ты незнаешь, пока ешё, где находтся базар".to_string()],
            "1000:b95b jumps to 0xc49b"
        );
    }

    /// `1000:d788`..`1000:d793` -- the `girl` block heals, charges 12 and
    /// clears the market ban, all three past the refusal at `1000:d706`.
    #[test]
    fn the_girl_clears_the_market_ban_only_when_she_is_paid() {
        let mut g = game();
        g.places.mark_found(Location::Girl);
        g.market_ban_countdown = 4;
        g.player.money = 100;
        g.player.hp = 3;
        term::capture::lines(|| {
            g.dispatch(Command::Girl, &mut no_input()).unwrap();
        });
        assert_eq!(g.market_ban_countdown, 0, "1000:d793");
        assert_eq!(g.player.hp, g.player.hpmax, "1000:d788");
        assert_eq!(g.player.money, 88, "1000:d78e sub 0xc");

        let mut g = game();
        g.places.mark_found(Location::Girl);
        g.market_ban_countdown = 4;
        g.player.money = 11;
        term::capture::lines(|| {
            g.dispatch(Command::Girl, &mut no_input()).unwrap();
        });
        assert_eq!(g.market_ban_countdown, 4, "1000:d706 jumps past 1000:d793");
    }

    /// The countdown's whole life cycle, driven through the real walk
    /// preamble: set to 5 (`1000:c465`, [`crate::market`]), ticked down once
    /// per walk by `1000:b173`, announced on its last turn by `1000:b11e`,
    /// and gone on the fifth. The `== 1` test at `1000:b11e` runs BEFORE the
    /// `dec` at `1000:b173`, so the message and the last tick share a turn.
    #[test]
    fn the_market_ban_ticks_down_once_per_walk_and_announces_its_last_turn() {
        let mut g = game();
        g.market_ban_countdown = market::BAN_TURNS;
        g.has_mobile = true; // 1000:b022's gate on 20ae:38bb
        g.places.mark_found(Location::Den); // 1000:b125's gate on 20ae:3696
        let mut seen = Vec::new();
        let mut announced = Vec::new();
        for _ in 0..market::BAN_TURNS {
            let before = g.market_ban_countdown;
            let out = term::capture::lines(|| {
                g.wander_preamble(false, &mut input(&["", "", "", ""]))
                    .unwrap();
            });
            announced.push(out.iter().any(|l| l.contains("там менты свалили")));
            seen.push((before, g.market_ban_countdown));
        }
        assert_eq!(
            seen,
            vec![(5, 4), (4, 3), (3, 2), (2, 1), (1, 0)],
            "1000:b171 `jz` guards 1000:b173 `dec`"
        );
        assert_eq!(
            announced,
            vec![false, false, false, false, true],
            "1000:b11e fires only on the turn the countdown reads 1"
        );
        // And zero is not decremented into 0xff.
        g.wander_preamble(false, &mut input(&["", "", "", ""]))
            .unwrap();
        assert_eq!(g.market_ban_countdown, 0, "1000:b171 guards the `dec`");
    }

    // -----------------------------------------------------------------
    // The endings -- `docs/re/port-gaps.md` rows 2, 4, 7, 13, 17, 21.
    // -----------------------------------------------------------------

    /// A player who can actually kill both rectors, so the endgame can be
    /// driven to its finish in a test rather than argued about.
    fn champion() -> Game {
        let mut g = game();
        g.district = 5;
        g.rector_showdown = true;
        g.player.level = 40;
        g.player.strength = 400;
        g.player.agility = 400;
        g.player.vitality = 400;
        g.player.luck = 400;
        g.player.dmg_min = 2000;
        g.player.dmg_max = 4000;
        g.player.hpmax = 20000;
        g.player.hp = 20000;
        g
    }

    /// **The game can be finished.** Both rector fights run, the victory
    /// ending fires, the marquee plays and the end screen's `Ты победил.`
    /// banner follows it -- and the game stops.
    ///
    /// This is the point of the batch: before it, `1000:ae18`'s four calls
    /// had no counterpart at all and `run_combat` had no `param_1 == 4`
    /// arm, so no sequence of inputs reached an ending.
    #[test]
    fn the_endgame_can_be_played_to_the_victory_screen() {
        let mut g = champion();
        let script = vec!["k"; 400];
        let out = term::capture::lines(|| {
            g.rector_endgame(&mut input(&script)).unwrap();
        });
        // 1000:3ec5 and 1000:3f43 -- the two openers, so both fights ran.
        assert!(out.iter().any(|l| l.contains("Ну вот мы и встретились")));
        assert!(out
            .iter()
            .any(|l| l.contains("Тут заходит настоящий ректор")));
        // 1000:515f -- the fake-out, which is fight 3's own ending.
        assert!(out.iter().any(|l| l.contains("да это ж не ректор был")));
        // 1000:50ae.. -- fight 4's five victory lines.
        assert!(out.iter().any(|l| l.contains("ТЫ САМЫЙ КРУТОЙ")));
        assert!(out.iter().any(|l| l == "^1А результат:"));
        // The marquee, then the end screen, in that order.
        let marquee = out
            .iter()
            .rposition(|l| crate::text::strip(l).ends_with("ТЫ СУПЕР ГОП"))
            .expect("the marquee plays");
        let banner = out
            .iter()
            .position(|l| l.contains("Ты победил."))
            .expect("the end screen draws");
        assert!(marquee < banner, "1000:5133 calls 0xaec, not 0x74b");
        assert!(!g.running, "the end screen halts");
    }

    /// The two boss values skip the ordinary victory text: no
    /// `^2Враг сдох.` (1000:519d is in the ELSE of 1000:5139), no XP award
    /// line (1000:51a6/51ac) and no "too weak an opponent" pair
    /// (1000:51f6/51fc).
    #[test]
    fn a_boss_kill_prints_none_of_the_ordinary_victory_lines() {
        let mut g = champion();
        let before_level = g.player.level;
        let script = vec!["k"; 400];
        let out = term::capture::lines(|| {
            g.run_combat(3, Game::boss("rektor_ngu_v0"), &mut input(&script))
                .unwrap();
        });
        assert!(!out.iter().any(|l| l.contains("Враг сдох")));
        assert!(!out.iter().any(|l| l.contains("качков опыта")));
        assert!(!out.iter().any(|l| l.contains("слишком слабого мудака")));
        // 1000:513f/5142 set xp := threshold and 1000:5145's
        // FUN_1000_2526(1) spends it with the level cap LIFTED -- the test
        // player starts at MAX_LEVEL, so a capped call would move nothing.
        assert_eq!(g.player.level, before_level + 1, "1000:5094 passes 1");
        assert_eq!(before_level, crate::progress::MAX_LEVEL);
    }

    /// The two stat blocks `FUN_1000_11c2` writes, as the fights receive
    /// them -- `1000:11dc`..`1000:1223` for the constants and
    /// `1000:1228`..`1000:1252` for the four derived fields.
    #[test]
    fn the_boss_constructor_matches_both_of_the_original_stat_blocks() {
        let v0 = Game::boss("rektor_ngu_v0");
        assert_eq!(
            (
                v0.class,
                v0.level,
                v0.strength,
                v0.agility,
                v0.vitality,
                v0.luck,
                v0.armor
            ),
            (10, 125, 41, 50, 123, 36, 60)
        );
        let v1 = Game::boss("rektor_ngu_v1");
        assert_eq!(
            (
                v1.class,
                v1.level,
                v1.strength,
                v1.agility,
                v1.vitality,
                v1.luck,
                v1.armor
            ),
            (10, 160, 50, 60, 188, 32, 80)
        );
        for f in [&v0, &v1] {
            // 1000:1228 / 1000:1234 / 1000:123a / 1000:124f.
            assert_eq!(f.dmg_min, f.strength / 2);
            assert_eq!(f.dmg_max, f.strength);
            assert_eq!(f.hpmax, f.vitality * 5 + f.strength + 10);
            assert_eq!(f.hp, f.hpmax);
            // 1000:1255/125a the break flags.
            assert!(!f.broken_jaw && !f.broken_leg);
        }
    }

    /// `1000:57ce` -- the den errand's reward. `district * 20` понтовость
    /// and `district * 10` xp, the xp going through `FUN_1000_2526(0)`.
    ///
    /// The threshold is pushed out of reach first, so the xp arithmetic is
    /// exact and neither run levels: what is under test is the award, not
    /// the level drain it feeds. The `param_1 = 0` control is run from the
    /// same starting state with the same script, so the difference between
    /// the two is the block and nothing else.
    #[test]
    fn the_den_errand_pays_cred_and_experience_and_nothing_else_does() {
        let run = |kind: u8| {
            let mut g = game();
            g.district = 3;
            g.progress.threshold = u16::MAX;
            let out = term::capture::lines(|| {
                g.run_combat(kind, punchbag_that_dies(), &mut input(&["k"; 40]))
                    .unwrap();
            });
            (g, out)
        };
        let (errand, out) = run(6);
        let (control, control_out) = run(0);

        // 1000:57d4 -- `district * 20`.
        assert_eq!(errand.pontovost_street, control.pontovost_street + 60);
        // 1000:5824..1000:582e -- `district * 10`, on top of the ordinary
        // award the control also gets.
        assert_eq!(errand.progress.xp, control.progress.xp + 30);
        assert!(out
            .iter()
            .any(|l| l.contains("Понтовость улутшилась на 60")));
        assert!(out
            .iter()
            .any(|l| l.contains("Ты получаешь 30 качков опыта за помощь")));
        // 1000:57d2 `jnz 0x5838` -- every other `param_1` skips it.
        assert!(!control_out.iter().any(|l| l.contains("за помощь")));
    }

    /// An enemy soft enough that the default test player kills it in the
    /// first exchange, so the post-victory tail is reached.
    fn punchbag_that_dies() -> Fighter {
        Fighter {
            name: "Мудак".to_string(),
            hp: 1,
            hpmax: 1,
            ..Fighter::default()
        }
    }

    /// `1000:3e8d` / `1000:3ead` / `1000:3f2b` -- the three openers the
    /// `0 | 6` gate does NOT cover, and the silence of everything else.
    #[test]
    fn each_param_value_greets_with_its_own_opener() {
        for (kind, want) in [
            (1u8, "^4Отдай кошелёк урод!"),
            (3, "^2Ну вот мы и встретились мудак!"),
            (4, "^6Тут заходит настоящий ректор."),
        ] {
            let mut g = game();
            let out = term::capture::lines(|| {
                g.run_combat(kind, punchbag(), &mut input(&["run"]))
                    .unwrap();
            });
            assert_eq!(
                out.first().map(String::as_str),
                Some(want),
                "param_1 {kind}"
            );
        }
        // 2 and 5 reach no opener at all: 1000:3d2f jumps past the chain and
        // none of its three compares names them.
        for kind in [2u8, 5] {
            let mut g = game();
            let out = term::capture::lines(|| {
                g.run_combat(kind, punchbag(), &mut input(&["run"]))
                    .unwrap();
            });
            assert!(
                !out.iter()
                    .any(|l| l.contains("кошелёк") || l.contains("встретились")),
                "param_1 {kind} must be silent"
            );
        }
    }

    /// Both deaths run the end screen -- `1000:4fb4` (the rector's) and
    /// `1000:5074` (the plain one), each behind its own ReadKey.
    #[test]
    fn dying_draws_the_end_screen() {
        for rector in [false, true] {
            let mut g = game();
            g.rector_showdown = rector;
            g.player.hp = 1;
            g.player.hpmax = 1;
            let killer = Fighter {
                name: "Мудак".to_string(),
                hp: 5000,
                hpmax: 5000,
                level: 40,
                strength: 400,
                agility: 400,
                dmg_min: 500,
                dmg_max: 900,
                ..Fighter::default()
            };
            let out = term::capture::lines(|| {
                g.run_combat(0, killer, &mut input(&["k"; 60])).unwrap();
            });
            assert!(!g.running, "rector={rector}");
            assert!(
                out.iter().any(|l| l.contains("Ты сдох.")),
                "rector={rector}"
            );
            // 1000:0765 -- the death banner's colour digit is '4'.
            assert!(
                out.iter().any(|l| l.starts_with("          ^4\u{2502}")),
                "rector={rector}"
            );
            assert!(!out.iter().any(|l| l.contains("Ты победил")));
        }
    }

    // -----------------------------------------------------------------
    // The opening and the text dumps -- `docs/re/port-gaps.md` rows 1, 10,
    // 15 and 16. `tools/difftest.py` owns the WORDING of every line here
    // (71 `opening_line` records read straight out of `orig/g.exe`); these
    // tests own the BRANCHES and the interpolation, which no record can see.
    // -----------------------------------------------------------------

    /// `1000:61bb cmp byte [0x3692],0x1` / `1000:61c0 jbe 0x61db` is `help`'s
    /// only branch: one line appears from district 2 on and from nowhere
    /// else. Both sides are asserted, so neither can pass by printing
    /// everything or nothing.
    #[test]
    fn help_gates_exactly_one_line_on_the_district() {
        let gated = crate::text::strip(opening::HELP_PLAIN[opening::HELP_DISTRICT_LINE]);
        let mut seen = Vec::new();
        for district in 1..=5u8 {
            let mut g = game();
            g.district = district;
            let out = term::capture::lines(|| {
                g.dispatch(Command::Help, &mut no_input()).unwrap();
            });
            assert!(
                out.iter().any(|l| l.contains("Броня - насколько")),
                "district {district}: the ungated body must always print"
            );
            seen.push((district, out.iter().any(|l| l.contains(&gated)), out.len()));
        }
        assert_eq!(
            seen.iter()
                .map(|(d, hit, _)| (*d, *hit))
                .collect::<Vec<_>>(),
            vec![(1, false), (2, true), (3, true), (4, true), (5, true)],
            "1000:61c0 is `jbe`, so district 1 alone skips it"
        );
        // And the skip costs exactly one line, not a block.
        assert_eq!(seen[0].2 + 1, seen[1].2, "one WriteLn, not several");
    }

    /// The two composed lines and the two `#`-filled ones are the whole
    /// reason `help` is not a static blob: `1000:5f82`/`5fed` append the
    /// class's rank name, `1000:5f96` the player's name, and `1000:6020`
    /// pushes the class's STRENGTH growth weight into both filled lines.
    #[test]
    fn help_is_personalised_from_the_class_and_the_name() {
        // Class 5 (Гопник) weighs 4/3/3/2 and class 6 (Вор) 3/3/2/4, so the
        // strength weight differs between them and a port reading the wrong
        // column would still differ here.
        for class in [5u16, 6] {
            let mut g = game();
            g.player.class = class;
            g.player.name = "Тест".to_string();
            let out = term::capture::lines(|| {
                g.dispatch(Command::Help, &mut no_input()).unwrap();
            });
            let rank = data::rank_name(class);
            assert_eq!(
                out[0],
                format!("^0Ну слушай, {rank} Тест ^0,в чем тут батва"),
                "1000:5f6a..5fb4"
            );
            assert_eq!(out[2], format!("^0 Например {rank}:"), "1000:5fd8..600e");
            let weight = progress::class_weights(class)[0];
            assert!(
                out[3].contains(&format!("а сила у тебя {weight} -")),
                "1000:6033 fills the STRENGTH weight: {:?}",
                out[3]
            );
            assert!(
                out[4].contains(&format!("понтовости {weight} из 12")),
                "1000:6058 fills the same value: {:?}",
                out[4]
            );
        }
        // The two classes must actually have produced different numbers,
        // or the assertions above would hold for a port that printed a
        // constant.
        assert_ne!(
            progress::class_weights(5)[0],
            progress::class_weights(6)[0],
            "the two probes must disagree or they prove nothing"
        );
    }

    /// `1000:ee04`..`1000:ee8b`: `e` / `exit` at the STREET prompt prints two
    /// lines, then the FULL character sheet (`1000:ee36 call 0x1a03`), then
    /// reads one discarded key (`1000:ee39`).
    #[test]
    fn quitting_prints_the_tail_the_character_sheet_and_eats_one_key() {
        let mut g = game();
        let mut lines = input(&["ignored", "still here"]);
        let out = term::capture::lines(|| {
            g.dispatch(Command::Quit, &mut lines).unwrap();
        });
        assert!(!g.running, "1000:ee43's Halt");
        assert_eq!(out[0], opening::QUIT_TAIL[0]);
        assert_eq!(out[1], opening::QUIT_TAIL[1]);
        // The sheet, not a summary: its header line is what `s` prints too.
        let sheet = character_sheet::lines(&g.player, &g.player.name, &g.sheet_kit());
        assert_eq!(
            &out[2..],
            &sheet[..],
            "1000:ee36 is the whole of FUN_1000_1a03"
        );
        assert_eq!(
            lines.count(),
            1,
            "1000:ee39's ReadKey consumes exactly one line"
        );
    }

    /// `1000:7262`'s chain keys on the DISTRICT, and `1000:7369`'s gate adds
    /// the tutorial for district 1 only. Districts 2, 3 and 4 print the same
    /// wording as `Game::district_advance`, from the OTHER string copy.
    #[test]
    fn the_entry_announcement_is_two_lines_per_district_plus_the_tutorial() {
        let want: Vec<(u8, usize)> = vec![(1, 5), (2, 2), (3, 2), (4, 2), (5, 1)];
        for (district, n) in want {
            let mut g = game();
            g.district = district;
            let out = term::capture::lines(|| g.announce_district());
            assert_eq!(out.len(), n, "district {district}: {out:#?}");
            if district <= 4 {
                let base = usize::from(district - 1) * 2;
                assert_eq!(out[0], opening::START_ARRIVAL[base]);
                assert_eq!(out[1], opening::START_ARRIVAL[base + 1]);
            }
            if district == 1 {
                assert_eq!(&out[2..], &opening::TUTORIAL[..]);
            }
        }
        // District 0 is not a `cmp al,N` arm at all -- nothing is printed,
        // which is what stops `chunks_exact(2).nth(d - 1)` from wrapping
        // into the last pair.
        let mut g = game();
        g.district = 0;
        assert!(term::capture::lines(|| g.announce_district()).is_empty());
    }

    /// `1000:ad12`'s chain has arms for 2, 3 and 4 only, and it reads the
    /// district AFTER `1000:ab92`'s increment -- so the line the player sees
    /// names the district they arrived in, not the one they left.
    #[test]
    fn the_promotion_announces_the_district_it_arrived_in() {
        for from in 1..=3u8 {
            let mut g = game();
            g.district = from;
            g.player.level = 40;
            let out = term::capture::lines(|| {
                g.district_advance(&mut input(&["n"])).unwrap();
            });
            assert_eq!(g.district, from + 1);
            let base = usize::from(from + 1 - 2) * 2;
            // `ends_with`, not `==`: the save prompt above is a `print`
            // with no newline, so `term::capture` glues the bare `\` onto
            // the front of the next line it sees.
            assert!(
                out.iter()
                    .any(|l| l.ends_with(opening::ADVANCE_ARRIVAL[base]))
                    && out
                        .iter()
                        .any(|l| l.ends_with(opening::ADVANCE_ARRIVAL[base + 1])),
                "district {} -> {}: {out:#?}",
                from,
                from + 1
            );
            // The copy it prints is `entry`'s, and for districts 2..4 the
            // two copies read alike -- which is why both are transcribed and
            // `difftest.py` compares them separately.
            assert_eq!(
                opening::ADVANCE_ARRIVAL[base],
                opening::START_ARRIVAL[usize::from(from + 1 - 1) * 2],
                "the two image copies agree today; difftest is what would \
                 notice if one of them ever did not"
            );
        }
    }
}
